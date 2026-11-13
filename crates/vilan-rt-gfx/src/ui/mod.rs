//! `std::ui`'s element tree, layout, paint and interaction (native-ui.md
//! parts P2 and P3; rulings R4, R5, R7, R9, R10, R16).
//!
//! # What lives here, and why here
//!
//! The RETAINED element tree is this crate's: a [`UiTree`] holds every element
//! of a program — its kind, its layout properties, its style layers, its
//! inherited text style, its accessibility role and label, its event handlers,
//! its focus and scroll state — in one `taffy` tree, and a vilan program holds
//! HANDLES to its elements (`host::ui`'s `Node`, one `Rc` per element). The
//! vilan surface (`vilan/std/src/ui.vl`) is the typed builder vocabulary over
//! it; every attribute call crosses once, as numbers, when it is set (again
//! when a signal it is bound to writes), and nothing crosses per frame but the
//! root's handle.
//!
//! The tree is here and not in vilan values because of what layout needs:
//!
//! - **Text measures itself during layout.** The solver asks a text element
//!   "how big are you at this width?" several times per pass. Only `|| void`
//!   closures cross the host boundary, so a vilan measure callback could take
//!   no arguments and answer nothing; the text system is this crate's anyway.
//! - **The solver's per-node cache is the incremental layout.** `taffy` keeps
//!   a cache per node and clears it up the ancestor chain when a node's style
//!   changes; a tree rebuilt into the solver for every pass would throw that
//!   away. Keeping the tree here keeps it: one write, one dirty chain, one
//!   re-solve that measures only what changed.
//! - **One Rust stack (R12, R14).** The browser target compiles this same
//!   crate to wasm; a tree mirrored in vilan values would be a second
//!   implementation of the same thing on the other side of the seam.
//!
//! # Passes, and what a change costs
//!
//! A frame of a mounted root is two passes, kept apart:
//!
//! 1. [`UiTree::layout`]: the inherited text styles of the subtrees whose
//!    style changed are resolved again; then, when the LAYOUT generation
//!    moved (a layout property, a child, a text run, a texture, a reshaping
//!    text style) or the size changed, the solver runs from the root's
//!    viewport — re-measuring only what its cache lost, the dirty chain —
//!    and every element's box is SNAPPED to the device pixel grid by
//!    [`Units`]' one rule, the renderer's (R16). A box whose size changed is
//!    marked for repaint; a box that only moved marks its PARENT (F170).
//! 2. [`UiTree::paint`]: every element keeps the SCENE of its own subtree
//!    (`std::gfx`'s nested [`Scene`]), in its OWN coordinates (its box's
//!    top-left corner at the origin), and its parent's scene places it at
//!    its offset in the parent's box ([`SceneBuilder::scene_at`]). Boxes are
//!    snapped, so every offset is whole device pixels, and the encoder adds
//!    it after snapping: a placed scene draws exactly the pixels it drew in
//!    window coordinates. A change marks the one element (and the chain of
//!    scenes above it, which only recombine their children's kept scenes)
//!    for rebuilding; everything else is appended as it was built. A frame
//!    with nothing changed appends the root's kept scene, and an insert at
//!    the top of a long list rebuilds the newcomer and the chain above it,
//!    not every row it pushed down.
//!
//! So a property that changes paint only (a colour, a radius, an opacity, a
//! condition's look) repaints its element without a layout pass, and a
//! scroll moves a viewport's offset without laying out or repainting what is
//! inside it.
//!
//! # Interaction
//!
//! Hit testing, the hover / press / focus state the style conditions read,
//! event routing with bubbling, the tab order and scrolling are `input.rs`'s;
//! style layers and conditions are `style.rs`'s.
//!
//! # Element kinds
//!
//! | Kind | Layout |
//! |---|---|
//! | row, column | flexbox along x or y: gap, padding, grow, shrink, align, justify, wrap |
//! | stack | every child in ONE grid cell, each placed by its own anchor |
//! | text | a leaf measured by the text system, wrapped to the width it is given |
//! | image | a leaf whose natural size is its texture's, in dp; painted by its fit |
//! | icon | a leaf measured as a square of its size, or else of the inherited text size; a path in the inherited text colour (`icon.rs`) |
//! | scroll | a clipped viewport over its one child, which keeps its own size |
//!
//! No margins (R5): space between siblings is the parent's gap, space inside
//! an element its padding.

mod access;
mod icon;
mod input;
mod style;
mod timers;

use std::rc::Rc;

use taffy::{
    AlignContent, AlignItems, AvailableSpace, Dimension, Display, FlexDirection, FlexWrap,
    LengthPercentage, LengthPercentageAuto, Overflow, TaffyTree, TraversePartialTree,
};

/// An element's id in its [`UiTree`].
pub use taffy::NodeId;

pub use access::{
    AccessPublisher, AccessStep, AccessTree, AccessWindow, WINDOW_NODE, access_id, access_listing,
};
pub use icon::IconShape;
pub use input::{
    DefaultAction, Delivery, EventKind, Handler, Input, MULTI_CLICK_DISTANCE, MULTI_CLICK_TIME,
    Route, Step,
};
pub use style::{Condition, Interaction, StyleFields};
pub use timers::{MIN_PERIOD, TimerId};

use crate::color::{Color, Fill};
use crate::geometry::{Corners, Edges, Point, Rect, Size, Transform};
use crate::scene::{Border, Image, Rectangle, Scene, SceneBuilder, Shadow};
use crate::text::{FontFamily, TextLayout, TextStyle, TextSystem};
use crate::units::Units;
use style::Layers;

#[cfg(test)]
mod tests;

/// What an element is. The viewport is the tree's own: the box a root is laid
/// out in, the window's size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElementKind {
    Row,
    Column,
    Stack,
    Text,
    Image,
    Scroll,
    /// A path in a square box, in the inherited text colour (`icon.rs`).
    Icon,
    /// A position among its parent's children, holding none of its own: where
    /// a keyed list's or a conditional's elements go (`std::ui`'s regions). It
    /// is never laid out, painted, hit or listed.
    Marker,
    Viewport,
}

impl ElementKind {
    /// The kind's name as the surface spells its constructor.
    pub fn name(self) -> &'static str {
        match self {
            ElementKind::Row => "row",
            ElementKind::Column => "column",
            ElementKind::Stack => "stack",
            ElementKind::Text => "text",
            ElementKind::Image => "image",
            ElementKind::Scroll => "scroll",
            ElementKind::Icon => "icon",
            ElementKind::Marker => "marker",
            ElementKind::Viewport => "viewport",
        }
    }

    fn is_parent(self) -> bool {
        matches!(
            self,
            ElementKind::Row
                | ElementKind::Column
                | ElementKind::Stack
                | ElementKind::Scroll
                | ElementKind::Viewport
        )
    }
}

/// A length a size property takes: dp, a fraction of the parent's content
/// box, or decided by the content and the parent (`Auto`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Length {
    Auto,
    Dp(f32),
    Fraction(f32),
}

/// The six size properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeProperty {
    Width,
    Height,
    MinWidth,
    MaxWidth,
    MinHeight,
    MaxHeight,
}

/// Where a row's or a column's children sit across its axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
    Stretch,
    Baseline,
}

/// How a row's or a column's children share the space along its axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Justify {
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// Where an element sits in a stack: an edge, a corner, the centre, or the
/// whole stack. Outside a stack it means nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
    Fill,
}

/// How an image's texture is drawn in its box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    /// Keep the aspect, inside the box, centred.
    Contain,
    /// Keep the aspect, filling the box, centred; the overflow is clipped.
    Cover,
    /// Stretch to exactly the box.
    Fill,
    /// The texture's own size in dp, centred; the overflow is clipped.
    None,
}

/// The directions a `scroll` moves its child in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollAxis {
    /// Up and down: the child is as wide as the viewport and as tall as it
    /// wants.
    Vertical,
    /// Left and right: the child is as tall as the viewport and as wide as it
    /// wants.
    Horizontal,
    /// Both: the child is as large as it wants each way.
    Both,
}

/// An element's padding box seen down its height ([`UiTree::inner_box`]):
/// its content box's height and the padding above and below it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InnerBox {
    pub padding_top: f32,
    pub content_height: f32,
    pub padding_bottom: f32,
}

/// Which index of an edited text a point lands on ([`UiTree::text_index_at`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexRule {
    /// The grapheme edge nearest the point: where a press puts the caret.
    Nearest,
    /// The start of the grapheme under the point: whose word a double click
    /// selects.
    Under,
    /// A drag's: [`Nearest`](IndexRule::Nearest) while the point is across
    /// the part of the text that shows; past its left or right edge (the
    /// pointer outside the field, held there by the capture), the first
    /// index out from that edge whose caret does not show whole — one
    /// grapheme beyond what shows — so each move extends the selection a
    /// grapheme further and the caret's reveal scrolls the field by it. In
    /// a text that shows whole, the end it is past.
    Drag,
    /// A multi-line drag's (a text area's): [`Drag`](IndexRule::Drag) while
    /// the point is level with the part of the text that shows; above or
    /// below it, the first index up or down from the line at that edge
    /// whose line does not show whole, at the point's x — one line beyond
    /// what shows, so each move extends the selection a line further and
    /// the caret's reveal scrolls the text by it. In a text that shows
    /// whole, the start or the end it is past.
    DragLines,
}

/// The accessibility role an element announces (R10), published through the
/// platform by `access.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Role {
    #[default]
    None,
    Group,
    Heading,
    Paragraph,
    Button,
    Link,
    TextField,
    /// A multi-line text field (`std::ui::kit`'s text area).
    TextArea,
    Checkbox,
    List,
    ListItem,
    Image,
}

/// A shadow under an element's box: offset, blur, spread, colour, in dp.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DropShadow {
    pub offset: Point,
    pub blur: f32,
    pub spread: f32,
    pub color: Color,
}

/// The text style an element SETS; every unset field is inherited from its
/// parent (R7: text style is the one inherited thing).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextOverrides {
    pub family: Option<FontFamily>,
    pub size: Option<f32>,
    pub line_height: Option<f32>,
    pub weight: Option<f32>,
    pub italic: Option<bool>,
    pub color: Option<Color>,
}

impl TextOverrides {
    fn is_empty(&self) -> bool {
        *self == TextOverrides::default()
    }

    /// Whether `self` and `other` differ only in colour: a change between
    /// them repaints and never reshapes.
    fn differs_in_colour_only(&self, other: &TextOverrides) -> bool {
        TextOverrides {
            color: other.color,
            ..self.clone()
        } == *other
    }
}

/// An element's text style after inheritance: what its text is shaped and
/// drawn with.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedText {
    pub style: TextStyle,
    pub color: Color,
}

impl Default for ResolvedText {
    /// The root's: `std::gfx`'s default text style, in black.
    fn default() -> Self {
        ResolvedText {
            style: TextStyle::default(),
            color: Color::new(0.0, 0.0, 0.0, 1.0),
        }
    }
}

impl ResolvedText {
    fn with(&self, overrides: &TextOverrides) -> ResolvedText {
        let mut resolved = self.clone();
        if let Some(family) = &overrides.family {
            resolved.style.family = family.clone();
        }
        if let Some(size) = overrides.size {
            resolved.style.size = size;
        }
        if let Some(line_height) = overrides.line_height {
            resolved.style.line_height = line_height;
        }
        if let Some(weight) = overrides.weight {
            resolved.style.weight = weight;
        }
        if let Some(italic) = overrides.italic {
            resolved.style.italic = italic;
        }
        if let Some(color) = overrides.color {
            resolved.color = color;
        }
        resolved
    }
}

/// What an element paints of its own, under its children: its effective
/// style's paint fields.
#[derive(Clone, Debug, PartialEq)]
pub struct Paint {
    pub background: Option<Color>,
    pub border_widths: Edges,
    pub border_color: Color,
    pub corners: Corners,
    pub shadow: Option<DropShadow>,
    pub opacity: f32,
}

impl Default for Paint {
    fn default() -> Self {
        Paint {
            background: None,
            border_widths: Edges::default(),
            border_color: Color::new(0.0, 0.0, 0.0, 0.0),
            corners: Corners::default(),
            shadow: None,
            opacity: 1.0,
        }
    }
}

/// A paragraph shaped at one width constraint, kept for the constraints it
/// also answers (see [`TextContent::shaped_for`]).
#[derive(Clone, Debug)]
struct Shaped {
    constraint: Option<f32>,
    layout: TextLayout,
}

#[derive(Clone, Debug, Default)]
struct TextContent {
    /// The paragraph's runs, one per `child`, in order: a run bound to a
    /// signal is replaced in place when it writes.
    runs: Vec<String>,
    /// The runs, joined: what is shaped.
    text: String,
    resolved: Rc<ResolvedText>,
    shaped: Vec<Shaped>,
    /// Paragraphs shaped since the text or its style last changed: what the
    /// reuse rule below saves is pinned by it.
    shapings: u32,
    /// Its caret and selection, when it is being edited (`text_edit`).
    editing: Option<Editing>,
    /// The width constraint an edited text was last laid out at (its content
    /// box's width less the caret's room: the width its paint shapes it at),
    /// set by the layout's snap: the layout the caret questions are answered
    /// from, painted or not (a window-less `--check` never paints).
    laid_out_at: Option<f32>,
}

/// A text being edited (`std::ui::kit`'s text input, native-ui.md P4): its
/// caret and selection, which it paints itself because only its layout knows
/// where its graphemes are. The program keeps the indices; the tree draws
/// them and answers where a point or a step lands (`text/editing.rs`).
///
/// An edited text is shaped with its graphemes' geometry, is never shorter
/// than one line (an empty one still has a caret), and is `caret_width`
/// wider than its text (room for the caret after the last grapheme).
#[derive(Clone, Debug, PartialEq)]
struct Editing {
    /// UTF-16 indices into the text, as the program counts (each read
    /// against the text as it is when it is used): the caret, and the other
    /// end of the selection (the caret itself when nothing is selected).
    caret: usize,
    anchor: usize,
    /// The caret and the selection are drawn (the input has the focus).
    active: bool,
    /// The caret's colour (the text's when `None`) and width in dp.
    caret_color: Option<Color>,
    caret_width: f32,
    /// The selection's colour (the text's at a quarter of its alpha when
    /// `None`).
    selection_color: Option<Color>,
    /// A word longer than a line breaks where it must (a multi-line field,
    /// `text_multiline`); otherwise text wraps only between words.
    break_words: bool,
}

impl Default for Editing {
    fn default() -> Editing {
        Editing {
            caret: 0,
            anchor: 0,
            active: false,
            caret_color: None,
            caret_width: 1.0,
            selection_color: None,
            break_words: false,
        }
    }
}

/// The byte offset of UTF-16 index `index` in `text` (the program's indices,
/// as `str`'s `len` and `substring` count): clamped to the text, and the
/// start of the character a unit inside a surrogate pair belongs to.
fn byte_of_unit(text: &str, index: usize) -> usize {
    let mut units = 0;
    for (byte, character) in text.char_indices() {
        let width = character.len_utf16();
        if units + width > index {
            return byte;
        }
        units += width;
    }
    text.len()
}

/// The UTF-16 index of byte offset `byte` in `text`.
fn unit_of_byte(text: &str, byte: usize) -> usize {
    text.char_indices()
        .take_while(|(at, _)| *at < byte)
        .map(|(_, character)| character.len_utf16())
        .sum()
}

/// How many shaped widths a text element keeps: a pass measures at its
/// min-content, max-content and final widths.
const SHAPED_KEPT: usize = 4;

/// Widths closer than this are one width (dp): a measured width read back as
/// the final one must not reshape, or wrap differently, by a rounding error.
const WIDTH_EPSILON: f32 = 1.0e-3;

impl TextContent {
    /// A paragraph shaped at `constraint` (wrapped at it; `None` is one line).
    ///
    /// Greedy line breaking makes one shaping answer a RANGE: a paragraph
    /// shaped at constraint `C` whose widest line is `W` breaks exactly the
    /// same at every `C'` with `W <= C' <= C` — every line still fits, and
    /// every break happened because the next word overflowed `C >= C'`. So
    /// the solver's max-content measure serves the final width it then
    /// assigns, and nothing is shaped twice for the same lines.
    fn shaped_for(&mut self, constraint: Option<f32>, text: &mut TextSystem) -> TextLayout {
        if let Some(found) = self
            .shaped
            .iter()
            .position(|shaped| shaped.answers(constraint))
        {
            let shaped = self.shaped.remove(found);
            let layout = shaped.layout.clone();
            self.shaped.insert(0, shaped);
            return layout;
        }
        let layout = match &self.editing {
            Some(editing) if editing.break_words => {
                text.editable_layout_breaking_words(&self.text, &self.resolved.style, constraint)
            }
            Some(_) => text.editable_layout(&self.text, &self.resolved.style, constraint),
            None => text.layout(&self.text, &self.resolved.style, constraint),
        };
        self.shapings += 1;
        self.shaped.insert(
            0,
            Shaped {
                constraint,
                layout: layout.clone(),
            },
        );
        self.shaped.truncate(SHAPED_KEPT);
        layout
    }

    fn reshape(&mut self) {
        self.shaped.clear();
        self.shapings = 0;
    }

    /// The kept paragraph [`shaped_for`](Self::shaped_for) would answer for
    /// `constraint`, without shaping one: what a reader with no text system
    /// (the accessibility conversion) reads.
    fn kept_for(&self, constraint: Option<f32>) -> Option<&TextLayout> {
        self.shaped
            .iter()
            .find(|shaped| shaped.answers(constraint))
            .map(|shaped| &shaped.layout)
    }
}

impl Shaped {
    /// Whether this paragraph has the lines a paragraph shaped at
    /// `constraint` would have (see [`TextContent::shaped_for`]).
    fn answers(&self, constraint: Option<f32>) -> bool {
        let width = self.layout.size().width;
        match (constraint, self.constraint) {
            (None, None) => true,
            (None, Some(_)) => false,
            // The same constraint again (a min-content paragraph is wider
            // than its constraint of zero), or one in its range.
            (Some(wanted), Some(kept)) if (kept - wanted).abs() <= WIDTH_EPSILON => true,
            (Some(wanted), kept) => {
                width <= wanted + WIDTH_EPSILON
                    && kept.is_none_or(|kept| kept + WIDTH_EPSILON >= wanted)
            }
        }
    }
}

#[derive(Clone, Debug)]
struct ImageContent {
    image: Option<Image>,
    fit: Fit,
}

/// A `scroll`'s state: its axis, where its child is scrolled to, and how far
/// it can go.
#[derive(Clone, Debug)]
struct ScrollState {
    axis: ScrollAxis,
    /// How far the child is moved up and left, in dp: within `0..=range`.
    offset: Point,
    /// How far the child reaches past the viewport, from the last layout.
    range: Size,
    /// Told when the offset moves other than by the program setting it (a
    /// wheel, a clamp), so a bound signal follows.
    on_offset: Option<Handler>,
    /// The layout generation the range was measured at. The range is CURRENT
    /// only while the tree's layout generation is still that one: an offset set
    /// while it is not (before the first layout, or after a change that can
    /// move a box — a list that just grew) is kept as asked, and the next
    /// layout clamps it into the range it measures, and tells the program when
    /// that moved it.
    measured_at: Option<u64>,
}

#[derive(Clone, Debug)]
enum Content {
    Parent,
    Text(TextContent),
    Image(ImageContent),
    Icon(icon::IconContent),
}

/// What a root remembers of its last passes.
#[derive(Clone, Debug)]
struct RootState {
    viewport: NodeId,
    /// The size and layout generation the solver last ran at.
    solved: Option<(Size, u64)>,
    /// The scale the boxes were last snapped at, and the solve count then.
    snapped: Option<(Units, u64)>,
    solves: u64,
}

/// One element, as the tree keeps it beside the solver's style.
#[derive(Clone, Debug)]
pub struct NodeData {
    kind: ElementKind,
    /// The style's layers, and what they resolve to.
    layers: Layers,
    paint: Paint,
    text: TextOverrides,
    /// The text style it inherits from its parent, then overrides: kept for
    /// every element so a text child added later resolves from it.
    resolved: Rc<ResolvedText>,
    anchor: Option<Anchor>,
    role: Role,
    label: Option<String>,
    /// Its states for assistive technology (`access.rs`).
    access: access::AccessState,
    content: Content,
    /// Children painted inside its padding box only.
    clip: bool,
    scroll: Option<ScrollState>,
    /// Its place in the tab order: `None` not focusable, a negative number
    /// focusable by a press or by the program only, 0 in tree order, a
    /// positive number before every 0, lowest first.
    tab_index: Option<i32>,
    interaction: Interaction,
    handlers: Vec<(EventKind, Handler)>,
    /// Whether a vilan handle to it is alive: an element is freed when it has
    /// neither a handle nor a parent.
    handle: bool,
    root: Option<RootState>,
    /// The snapped box, in dp from the root's top-left corner (a scrolled
    /// element's box is where it is laid out, before the scroll's offset).
    bounds: Rect,
    /// Where its box sits in its parent's box (in the window, for a root):
    /// the offset its parent's scene places its scene at. Kept beside
    /// `bounds` so the snap can tell a box that moved with its parent (its
    /// parent's scene still places it right) from one that moved within it.
    placed: Point,
    /// Where the content box (inside the padding) starts in the box.
    content_offset: Point,
    /// The UNsnapped content size: text is wrapped at the width the solver
    /// gave it, never at a snapped width that could break a line the solver
    /// did not.
    content_size: Size,
    /// The scene of its subtree from the last paint, kept until something
    /// in it changes. When it is `None`, so is every ancestor's.
    scene: Option<Scene>,
    /// Whether that scene draws the box's snapped size (a background, a
    /// border, a shadow, a clip). Its content (a text, an image) is drawn in
    /// the content box, which is not snapped; and a box's snapped size
    /// depends on where it sits (each edge is rounded on its own), so a
    /// box that only moved may change size by a pixel without its scene
    /// changing.
    scene_reads_size: bool,
}

impl NodeData {
    fn new(kind: ElementKind) -> NodeData {
        let content = match kind {
            ElementKind::Text => Content::Text(TextContent::default()),
            ElementKind::Image => Content::Image(ImageContent {
                image: None,
                fit: Fit::Contain,
            }),
            ElementKind::Icon => Content::Icon(icon::IconContent::default()),
            _ => Content::Parent,
        };
        NodeData {
            kind,
            layers: Layers::default(),
            paint: Paint::default(),
            text: TextOverrides::default(),
            resolved: Rc::new(ResolvedText::default()),
            anchor: None,
            role: Role::None,
            label: None,
            access: access::AccessState::default(),
            content,
            clip: kind == ElementKind::Scroll,
            scroll: (kind == ElementKind::Scroll).then(|| ScrollState {
                axis: ScrollAxis::Vertical,
                offset: Point::ZERO,
                range: Size::default(),
                on_offset: None,
                measured_at: None,
            }),
            tab_index: None,
            interaction: Interaction::default(),
            handlers: Vec::new(),
            handle: true,
            root: None,
            bounds: Rect::default(),
            placed: Point::default(),
            content_offset: Point::default(),
            content_size: Size::default(),
            scene: None,
            scene_reads_size: false,
        }
    }
}

/// Why a tree operation was refused. Each is a program's mistake, raised as
/// its failure by the host.
#[derive(Clone, Debug, PartialEq)]
pub enum UiError {
    /// `child` on a text or an image.
    NotAParent(ElementKind),
    /// A second child for a `scroll`.
    OneChild,
    /// An element given a second parent: an element is in one place.
    AlreadyPlaced,
    /// An element made a child of itself or of its own descendant.
    Cycle,
    /// An element laid out or painted as a root while it is inside another.
    NotARoot,
    /// A property set on an element of a kind that has no such property.
    WrongKind {
        property: &'static str,
        kind: ElementKind,
    },
    /// A number that is not finite where a length or a factor is wanted.
    NotFinite(&'static str),
    /// A run index a text does not have.
    NoSuchRun(usize),
    /// A caret question to a text that is not being edited.
    NotEdited,
    /// An element placed before one that is in no parent.
    NoPosition,
    /// An icon's shape or size that cannot be drawn, and why.
    InvalidIcon(String),
}

impl std::fmt::Display for UiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UiError::NotAParent(kind) => write!(
                formatter,
                "a `{}` element has no children: only a row, a column, a stack or a scroll \
                 takes `child`",
                kind.name()
            ),
            UiError::OneChild => write!(
                formatter,
                "a `scroll` has one child, which it scrolls: put a column (or a row) of the \
                 content inside it"
            ),
            UiError::AlreadyPlaced => write!(
                formatter,
                "this element is already a child of another: an element is in one place in the \
                 tree, so build a second element for a second place"
            ),
            UiError::Cycle => write!(
                formatter,
                "an element cannot be a child of itself or of an element inside it"
            ),
            UiError::NotARoot => write!(
                formatter,
                "this element is inside another, so it cannot be laid out or drawn as a root: \
                 lay out the element at the top of its tree"
            ),
            UiError::WrongKind { property, kind } => {
                write!(formatter, "a `{}` element has no `{property}`", kind.name())
            }
            UiError::NotFinite(what) => write!(formatter, "{what} must be a finite number"),
            UiError::NoSuchRun(index) => write!(formatter, "a text has no run {index}"),
            UiError::NotEdited => write!(
                formatter,
                "a text answers where its caret goes only while it is edited (`std::ui::kit`'s \
                 text input edits its own)"
            ),
            UiError::NoPosition => write!(
                formatter,
                "an element can be placed before another only when that one is inside a parent"
            ),
            UiError::InvalidIcon(reason) => write!(formatter, "{reason}"),
        }
    }
}

impl std::error::Error for UiError {}

/// One element as [`UiTree::outline`] reports it.
#[derive(Clone, Debug, PartialEq)]
pub struct OutlineEntry {
    pub depth: usize,
    pub kind: ElementKind,
    pub label: Option<String>,
    pub bounds: Rect,
}

/// What the last layout and paint passes did: what the "a change touches
/// only what it must" pins read, and what the frame-time measurements print.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PassStats {
    /// The last layout ran the solver.
    pub solved: bool,
    /// Leaves the solver measured in the last layout (a text or an image the
    /// solver's cache could not answer).
    pub measured: usize,
    /// Elements whose snapped box changed in the last layout.
    pub moved: usize,
    /// Elements whose subtree scene the last paint rebuilt (every other one
    /// was appended as kept).
    pub rebuilt: usize,
}

/// Every element of a program: see the module documentation.
pub struct UiTree {
    taffy: TaffyTree<NodeData>,
    /// Moves on every change that can move a box.
    layout_generation: u64,
    /// Moves on every change that can change a pixel.
    paint_generation: u64,
    /// Elements whose subtree's inherited text style must be resolved again
    /// before the next layout (a text style set, a child added).
    pending_text: Vec<NodeId>,
    /// Elements with a layer that reads the window's width.
    width_conditional: Vec<NodeId>,
    /// The width of the window the last layout was at.
    window_width: Option<f32>,
    /// The interaction state (`input.rs`).
    hovered: Option<NodeId>,
    /// The element the primary button went down on: it holds the pointer
    /// (its CAPTURE, `input.rs`) until the button comes up.
    pressed: Option<NodeId>,
    focused: Option<NodeId>,
    pointer: Option<Point>,
    /// The root whose window the pointer is in (its last pointer input).
    pointer_root: Option<NodeId>,
    /// Something moved under the pointer since the hover was last worked
    /// out (a box, an offset, an element removed): the next
    /// [`UiTree::refresh_hover`] hit tests again.
    hover_stale: bool,
    /// The last press, for counting presses in a row (`input.rs`).
    last_press: Option<input::PressRecord>,
    /// Scrolls whose offset moved by the framework, to tell their programs.
    offset_notices: Vec<NodeId>,
    /// Elements listening for their size whose size the last layouts changed,
    /// to tell their programs.
    resize_notices: Vec<NodeId>,
    /// An element to show (Tab's, `scroll_into_view`'s) asked for while its
    /// tree's layout was pending: its box was not yet where it will be, so it
    /// is revealed at the end of the next layout of its tree instead.
    pending_reveal: Option<NodeId>,
    /// Elements the solver was told changed since it last ran: with their
    /// ancestors, the only elements whose children it can have moved.
    dirty: Vec<NodeId>,
    stats: PassStats,
    /// The leaves the last solve measured, once each.
    measured: Vec<NodeId>,
    /// Edited texts whose caret the next layout scrolls into view.
    caret_reveals: Vec<NodeId>,
    /// Moves when only what assistive technology reads changed (`access.rs`).
    semantics_generation: u64,
    /// Every element's timers (`timers.rs`, F176).
    timers: timers::Timers,
}

impl Default for UiTree {
    fn default() -> Self {
        UiTree::new()
    }
}

fn dimension(length: Length) -> Dimension {
    match length {
        Length::Auto => Dimension::auto(),
        Length::Dp(value) => Dimension::length(value),
        Length::Fraction(value) => Dimension::percent(value),
    }
}

fn length_percentage_auto(length: Length) -> LengthPercentageAuto {
    match length {
        Length::Auto => LengthPercentageAuto::auto(),
        Length::Dp(value) => LengthPercentageAuto::length(value),
        Length::Fraction(value) => LengthPercentageAuto::percent(value),
    }
}

fn finite(value: f32, what: &'static str) -> Result<f32, UiError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(UiError::NotFinite(what))
    }
}

/// An anchor as the grid's self-alignment on each axis.
fn anchor_alignment(anchor: Anchor) -> (AlignItems, AlignItems) {
    let (start, center, end, stretch) = (
        AlignItems::START,
        AlignItems::CENTER,
        AlignItems::END,
        AlignItems::STRETCH,
    );
    match anchor {
        Anchor::TopLeft => (start, start),
        Anchor::Top => (center, start),
        Anchor::TopRight => (end, start),
        Anchor::Left => (start, center),
        Anchor::Center => (center, center),
        Anchor::Right => (end, center),
        Anchor::BottomLeft => (start, end),
        Anchor::Bottom => (center, end),
        Anchor::BottomRight => (end, end),
        Anchor::Fill => (stretch, stretch),
    }
}

/// A scroll's flex arrangement for its axis: the child keeps its own size
/// along every axis that scrolls (it never shrinks to the viewport), and
/// fills the viewport across the axis that does not.
fn scroll_style(style: &mut taffy::Style, axis: ScrollAxis) {
    style.display = Display::Flex;
    let (direction, align, overflow) = match axis {
        ScrollAxis::Vertical => (
            FlexDirection::Column,
            AlignItems::STRETCH,
            taffy::Point {
                x: Overflow::Visible,
                y: Overflow::Hidden,
            },
        ),
        ScrollAxis::Horizontal => (
            FlexDirection::Row,
            AlignItems::STRETCH,
            taffy::Point {
                x: Overflow::Hidden,
                y: Overflow::Visible,
            },
        ),
        ScrollAxis::Both => (
            FlexDirection::Column,
            AlignItems::START,
            taffy::Point {
                x: Overflow::Hidden,
                y: Overflow::Hidden,
            },
        ),
    };
    style.flex_direction = direction;
    style.align_items = align;

    // `Hidden`: the scroll's own smallest size is not its content's, so a
    // parent can make it smaller than what it shows; no scrollbar space.
    style.overflow = overflow;
    style.scrollbar_width = 0.0;
}

/// The solver's style a new element of `kind` starts from.
fn initial_style(kind: ElementKind) -> taffy::Style {
    let mut style = taffy::Style::DEFAULT;
    // Every element sits in the first cell of a grid parent: a stack puts all
    // of its children in one cell, and a flex parent ignores the placement.
    style.grid_row = taffy::style_helpers::line(1);
    style.grid_column = taffy::style_helpers::line(1);
    match kind {
        ElementKind::Row | ElementKind::Column => {
            style.display = Display::Flex;
            style.flex_direction = if kind == ElementKind::Row {
                FlexDirection::Row
            } else {
                FlexDirection::Column
            };
            style.align_items = AlignItems::STRETCH;
            style.justify_content = AlignContent::START;
        }
        ElementKind::Stack => {
            style.display = Display::Grid;
            // An unanchored child sits at the top left.
            style.justify_items = AlignItems::START;
            style.align_items = AlignItems::START;
        }
        ElementKind::Scroll => scroll_style(&mut style, ScrollAxis::Vertical),
        ElementKind::Viewport => {
            style.display = Display::Grid;
            // The root fills the window unless it sizes itself.
            style.justify_items = AlignItems::STRETCH;
            style.align_items = AlignItems::STRETCH;
        }
        ElementKind::Text => {
            style.display = Display::Flex;
        }
        ElementKind::Image => {
            style.display = Display::Flex;
            style.item_is_replaced = true;
        }
        ElementKind::Icon => {
            style.display = Display::Flex;
            style.item_is_replaced = true;
        }
        // No box: the solver skips it, and so does every pass after it.
        ElementKind::Marker => {
            style.display = Display::None;
        }
    }
    style
}

/// Whether `point` is inside `bounds` with its corners rounded by `corners`.
fn inside_rounded(bounds: Rect, corners: Corners, point: Point) -> bool {
    if point.x < bounds.left()
        || point.y < bounds.top()
        || point.x >= bounds.right()
        || point.y >= bounds.bottom()
    {
        return false;
    }
    let half = bounds.size.width.min(bounds.size.height) / 2.0;
    let corner_test = |radius: f32, center_x: f32, center_y: f32, beyond: bool| {
        let radius = radius.clamp(0.0, half);
        if radius <= 0.0 || !beyond {
            return true;
        }
        let (dx, dy) = (point.x - center_x, point.y - center_y);
        dx * dx + dy * dy <= radius * radius
    };
    let (left, top, right, bottom) = (bounds.left(), bounds.top(), bounds.right(), bounds.bottom());
    let r = corners;
    corner_test(
        r.top_left,
        left + r.top_left.min(half),
        top + r.top_left.min(half),
        point.x < left + r.top_left.min(half) && point.y < top + r.top_left.min(half),
    ) && corner_test(
        r.top_right,
        right - r.top_right.min(half),
        top + r.top_right.min(half),
        point.x > right - r.top_right.min(half) && point.y < top + r.top_right.min(half),
    ) && corner_test(
        r.bottom_right,
        right - r.bottom_right.min(half),
        bottom - r.bottom_right.min(half),
        point.x > right - r.bottom_right.min(half) && point.y > bottom - r.bottom_right.min(half),
    ) && corner_test(
        r.bottom_left,
        left + r.bottom_left.min(half),
        bottom - r.bottom_left.min(half),
        point.x < left + r.bottom_left.min(half) && point.y > bottom - r.bottom_left.min(half),
    )
}

impl UiTree {
    pub fn new() -> UiTree {
        let mut taffy = TaffyTree::new();
        // Snapping is ours: the renderer's rule, on the device grid, not
        // `taffy`'s rounding to whole dp.
        taffy.disable_rounding();
        UiTree {
            taffy,
            layout_generation: 1,
            paint_generation: 1,
            pending_text: Vec::new(),
            width_conditional: Vec::new(),
            window_width: None,
            hovered: None,
            pressed: None,
            focused: None,
            pointer: None,
            pointer_root: None,
            hover_stale: false,
            last_press: None,
            offset_notices: Vec::new(),
            resize_notices: Vec::new(),
            pending_reveal: None,
            dirty: Vec::new(),
            stats: PassStats::default(),
            measured: Vec::new(),
            caret_reveals: Vec::new(),
            semantics_generation: 1,
            timers: timers::Timers::default(),
        }
    }

    /// Elements alive in the tree, viewports included.
    pub fn element_count(&self) -> usize {
        self.taffy.total_node_count()
    }

    /// What the last layout and paint did.
    pub fn last_pass(&self) -> PassStats {
        self.stats
    }

    /// The leaves the last solve measured, each once.
    pub fn last_measured(&self) -> &[NodeId] {
        &self.measured
    }

    /// The layout and paint generations: when neither moved, nothing a frame
    /// shows changed.
    pub fn generations(&self) -> (u64, u64) {
        (self.layout_generation, self.paint_generation)
    }

    /// Whether `node` is an element of this tree still.
    pub fn is_alive(&self, node: NodeId) -> bool {
        self.taffy.get_node_context(node).is_some()
    }

    fn data(&self, node: NodeId) -> &NodeData {
        self.taffy
            .get_node_context(node)
            .expect("every element carries its data")
    }

    fn data_mut(&mut self, node: NodeId) -> &mut NodeData {
        self.taffy
            .get_node_context_mut(node)
            .expect("every element carries its data")
    }

    fn layout_changed(&mut self) {
        self.layout_generation += 1;
        self.paint_generation += 1;
    }

    /// `node`'s own look changed: its scene, and the chain of scenes above
    /// it, are rebuilt at the next paint; every other scene is kept.
    fn invalidate(&mut self, node: NodeId) {
        self.paint_generation += 1;
        let mut current = Some(node);
        while let Some(node) = current {
            let Some(data) = self.taffy.get_node_context_mut(node) else {
                return;
            };
            if data.scene.take().is_none() {
                // Every ancestor of an element with no kept scene has none
                // either (a scene is built after its children's).
                return;
            }
            current = self.taffy.parent(node);
        }
    }

    /// A new element of `kind`, with a live handle and no parent.
    pub fn create(&mut self, kind: ElementKind) -> NodeId {
        self.taffy
            .new_leaf_with_context(initial_style(kind), NodeData::new(kind))
            .expect("a new element")
    }

    pub fn kind(&self, node: NodeId) -> ElementKind {
        self.data(node).kind
    }

    fn update_style(&mut self, node: NodeId, change: impl FnOnce(&mut taffy::Style)) {
        let mut style = self.taffy.style(node).expect("an element's style").clone();
        change(&mut style);
        self.taffy
            .set_style(node, style)
            .expect("an element's style");
        self.dirty.push(node);
        self.layout_changed();
    }

    /// The element `node`'s tree hangs from: itself when it has no parent
    /// (or only its viewport).
    pub fn root_of(&self, node: NodeId) -> NodeId {
        let mut current = node;
        while let Some(parent) = self.taffy.parent(current) {
            if self.kind(parent) == ElementKind::Viewport {
                break;
            }
            current = parent;
        }
        current
    }

    // ------------------------------------------------------------ the tree ---

    /// Appends `child` to `parent`'s children.
    pub fn add_child(&mut self, parent: NodeId, child: NodeId) -> Result<(), UiError> {
        self.check_placement(parent, child)?;
        self.taffy
            .add_child(parent, child)
            .expect("a child appended");
        self.placed(parent, child);
        Ok(())
    }

    /// Puts `child` immediately before `reference`, in `reference`'s parent: a
    /// child of that parent already MOVES there (its state — hover, press,
    /// focus, scroll offset, cached layout — goes with it); any other is
    /// placed there as `add_child` places one at the end.
    ///
    /// The solver is told that the parent changed, and nothing else: it lays
    /// the parent's children out again from their cached sizes, measuring only
    /// a newcomer's leaves.
    pub fn insert_before(&mut self, reference: NodeId, child: NodeId) -> Result<(), UiError> {
        let parent = self
            .taffy
            .parent(reference)
            .filter(|parent| self.kind(*parent) != ElementKind::Viewport)
            .ok_or(UiError::NoPosition)?;
        if child == reference {
            return Ok(());
        }
        if self.taffy.parent(child) == Some(parent) {
            let from = self.index_of(parent, child);
            self.taffy
                .remove_child_at_index(parent, from)
                .expect("a child moved");
            let to = self.index_of(parent, reference);
            self.taffy
                .insert_child_at_index(parent, to, child)
                .expect("a child moved");
            self.dirty.push(parent);
            self.invalidate(parent);
            self.layout_changed();
            return Ok(());
        }
        self.check_placement(parent, child)?;
        let to = self.index_of(parent, reference);
        self.taffy
            .insert_child_at_index(parent, to, child)
            .expect("a child inserted");
        self.placed(parent, child);
        Ok(())
    }

    /// Whether `child` may become a child of `parent`; a root laid out on its
    /// own loses its viewport when it may.
    fn check_placement(&mut self, parent: NodeId, child: NodeId) -> Result<(), UiError> {
        let kind = self.kind(parent);
        if !kind.is_parent() || kind == ElementKind::Viewport {
            return Err(UiError::NotAParent(kind));
        }
        if kind == ElementKind::Scroll
            && self.kind(child) != ElementKind::Marker
            && self
                .children(parent)
                .iter()
                .any(|existing| self.kind(*existing) != ElementKind::Marker)
        {
            return Err(UiError::OneChild);
        }
        if parent == child || self.is_ancestor(child, parent) {
            return Err(UiError::Cycle);
        }
        if let Some(current) = self.taffy.parent(child) {
            if self.kind(current) != ElementKind::Viewport {
                return Err(UiError::AlreadyPlaced);
            }
            // A root that was laid out on its own joins a tree: its viewport
            // goes.
            self.drop_viewport(child);
        }
        Ok(())
    }

    /// `child` was placed in `parent`: the parent is laid out and painted
    /// again, and the child's text style resolved from its new parent.
    fn placed(&mut self, parent: NodeId, child: NodeId) {
        self.dirty.push(parent);
        self.pending_text.push(child);
        self.invalidate(parent);
        self.layout_changed();
    }

    /// `child`'s index among `parent`'s children, searched from both ends at
    /// once: a list's region inserts at its end (its anchor is its last
    /// child) or near its start, and either is found in a step or two.
    fn index_of(&self, parent: NodeId, child: NodeId) -> usize {
        let count = self.taffy.child_count(parent);
        let (mut front, mut back) = (0, count);
        while front < back {
            if self.taffy.get_child_id(parent, front) == child {
                return front;
            }
            back -= 1;
            if self.taffy.get_child_id(parent, back) == child {
                return back;
            }
            front += 1;
        }
        panic!("a child is among its parent's children")
    }

    /// REMOVES `node` from its parent for good (a keyed list's row going, a
    /// conditional turning off): the parent is laid out and painted again,
    /// the hover, press and focus inside the subtree are cleared (the press is
    /// cancelled: no click follows it, and its capture ends, so the rest of
    /// the drag goes to whatever is under the pointer), and every handler of the subtree is
    /// TAKEN OUT of the tree and answered, for the caller to drop with the
    /// tree no longer borrowed — its timers' handlers with them (F176: a
    /// timer is the same kind of thing, and never fires after its element's
    /// removal).
    ///
    /// Dropping the handlers is what ends an element (B738): a handler that
    /// captures its own element (`on_click(|| row.focus())`) holds that
    /// element's handle, and the element holds the handler, so with the
    /// handlers kept the element would outlive every other handle and live
    /// until the program ends. Removal is the one place an element of a tree
    /// leaves it (a move is not a removal), so it is where its handlers go;
    /// the element itself is then freed when its last handle is gone, as any
    /// parentless element is. An element removed while the program still
    /// holds it stays, with no handlers and no parent.
    pub fn remove(&mut self, node: NodeId) -> Vec<Handler> {
        if !self.is_alive(node) {
            return Vec::new();
        }
        let parent = self
            .taffy
            .parent(node)
            .filter(|parent| self.kind(*parent) != ElementKind::Viewport);
        let inside = |tree: &UiTree, other: Option<NodeId>| {
            other.is_some_and(|other| {
                tree.is_alive(other) && (other == node || tree.is_ancestor(node, other))
            })
        };
        if inside(self, self.pressed) {
            self.release_press();
        }
        if inside(self, self.focused) {
            self.clear_focus();
        }
        if inside(self, self.hovered) {
            self.unhover_inside(node, parent);
        }
        // What is under the pointer is known again after the next layout,
        // when what moved into the removed element's place has its box.
        self.hover_stale = true;
        let mut handlers = Vec::new();
        let mut pending = vec![node];
        while let Some(current) = pending.pop() {
            let data = self.data_mut(current);
            handlers.extend(data.handlers.drain(..).map(|(_, handler)| handler));
            if let Some(scroll) = data.scroll.as_mut()
                && let Some(handler) = scroll.on_offset.take()
            {
                handlers.push(handler);
            }
            handlers.extend(self.timers.take_node(current));
            pending.extend(self.children(current));
        }
        if let Some(parent) = parent {
            self.taffy
                .remove_child(parent, node)
                .expect("a child removed");
            self.dirty.push(parent);
            self.invalidate(parent);
            if !self.data(node).handle {
                self.free(node);
            }
        }
        self.layout_changed();
        handlers
    }

    fn is_ancestor(&self, ancestor: NodeId, node: NodeId) -> bool {
        let mut current = self.taffy.parent(node);
        while let Some(parent) = current {
            if parent == ancestor {
                return true;
            }
            current = self.taffy.parent(parent);
        }
        false
    }

    pub fn children(&self, node: NodeId) -> Vec<NodeId> {
        self.taffy.children(node).unwrap_or_default()
    }

    /// The element's vilan handle is gone. It is freed now if nothing holds
    /// it, with every element under it that has no handle either; one that
    /// still has a handle stays, parentless, until its own goes.
    pub fn release(&mut self, node: NodeId) {
        if self.taffy.get_node_context(node).is_none() {
            return;
        }
        self.data_mut(node).handle = false;
        let parent = self.taffy.parent(node);
        let placed = parent.is_some_and(|parent| self.kind(parent) != ElementKind::Viewport);
        if !placed {
            self.free(node);
        }
    }

    fn drop_viewport(&mut self, root: NodeId) {
        if let Some(state) = self.data_mut(root).root.take() {
            let _ = self.taffy.remove(state.viewport);
        }
    }

    fn free(&mut self, node: NodeId) {
        let mut pending = vec![node];
        while let Some(node) = pending.pop() {
            self.drop_viewport(node);
            // Its timers go with it, as its handlers do (in `taffy`'s data).
            drop(self.timers.take_node(node));
            for child in self.children(node) {
                if !self.data(child).handle {
                    pending.push(child);
                } else {
                    // It stays, parentless: its subtree's text resolves
                    // from the default when it is placed or laid out again.
                    self.pending_text.push(child);
                }
            }
            // Detaches the children that stay (they have handles).
            let _ = self.taffy.remove(node);
        }
        self.layout_changed();
    }

    // ---------------------------------------------------------- properties ---

    pub fn set_size(
        &mut self,
        node: NodeId,
        property: SizeProperty,
        length: Length,
    ) -> Result<(), UiError> {
        if let Length::Dp(value) | Length::Fraction(value) = length {
            finite(value, "a length")?;
        }
        self.update_style(node, |style| match property {
            SizeProperty::Width => style.size.width = dimension(length),
            SizeProperty::Height => style.size.height = dimension(length),
            SizeProperty::MinWidth => style.min_size.width = length_percentage_auto(length),
            SizeProperty::MaxWidth => style.max_size.width = length_percentage_auto(length),
            SizeProperty::MinHeight => style.min_size.height = length_percentage_auto(length),
            SizeProperty::MaxHeight => style.max_size.height = length_percentage_auto(length),
        });
        Ok(())
    }

    /// How much of a parent's leftover space along its axis this element
    /// takes, against its siblings' (0: none).
    pub fn set_grow(&mut self, node: NodeId, grow: f32) -> Result<(), UiError> {
        let grow = finite(grow, "a grow factor")?.max(0.0);
        self.update_style(node, |style| style.flex_grow = grow);
        Ok(())
    }

    /// How much of a parent's shortfall this element gives up (1 by default;
    /// 0: never smaller than its content).
    pub fn set_shrink(&mut self, node: NodeId, shrink: f32) -> Result<(), UiError> {
        let shrink = finite(shrink, "a shrink factor")?.max(0.0);
        self.update_style(node, |style| style.flex_shrink = shrink);
        Ok(())
    }

    pub fn set_padding(&mut self, node: NodeId, padding: Edges) -> Result<(), UiError> {
        for side in [padding.top, padding.right, padding.bottom, padding.left] {
            finite(side, "a padding")?;
        }
        self.update_style(node, |style| {
            style.padding = taffy::Rect {
                left: LengthPercentage::length(padding.left.max(0.0)),
                right: LengthPercentage::length(padding.right.max(0.0)),
                top: LengthPercentage::length(padding.top.max(0.0)),
                bottom: LengthPercentage::length(padding.bottom.max(0.0)),
            }
        });
        Ok(())
    }

    /// One side of the padding: 0 top, 1 right, 2 bottom, 3 left.
    pub fn set_padding_side(
        &mut self,
        node: NodeId,
        side: usize,
        value: f32,
    ) -> Result<(), UiError> {
        let value = LengthPercentage::length(finite(value, "a padding")?.max(0.0));
        self.update_style(node, |style| match side {
            0 => style.padding.top = value,
            1 => style.padding.right = value,
            2 => style.padding.bottom = value,
            _ => style.padding.left = value,
        });
        Ok(())
    }

    fn require_flex(&self, node: NodeId, property: &'static str) -> Result<(), UiError> {
        match self.kind(node) {
            ElementKind::Row | ElementKind::Column => Ok(()),
            kind => Err(UiError::WrongKind { property, kind }),
        }
    }

    /// The space between a row's or a column's children, along its axis and
    /// (when it wraps) between its lines.
    pub fn set_gap(&mut self, node: NodeId, gap: f32) -> Result<(), UiError> {
        self.require_flex(node, "gap")?;
        let gap = LengthPercentage::length(finite(gap, "a gap")?.max(0.0));
        self.update_style(node, |style| {
            style.gap = taffy::Size {
                width: gap,
                height: gap,
            }
        });
        Ok(())
    }

    pub fn set_align(&mut self, node: NodeId, align: Align) -> Result<(), UiError> {
        self.require_flex(node, "align")?;
        self.update_style(node, |style| {
            style.align_items = match align {
                Align::Start => AlignItems::START,
                Align::Center => AlignItems::CENTER,
                Align::End => AlignItems::END,
                Align::Stretch => AlignItems::STRETCH,
                Align::Baseline => AlignItems::BASELINE,
            }
        });
        Ok(())
    }

    pub fn set_justify(&mut self, node: NodeId, justify: Justify) -> Result<(), UiError> {
        self.require_flex(node, "justify")?;
        self.update_style(node, |style| {
            style.justify_content = match justify {
                Justify::Start => AlignContent::START,
                Justify::Center => AlignContent::CENTER,
                Justify::End => AlignContent::END,
                Justify::SpaceBetween => AlignContent::SPACE_BETWEEN,
                Justify::SpaceAround => AlignContent::SPACE_AROUND,
                Justify::SpaceEvenly => AlignContent::SPACE_EVENLY,
            }
        });
        Ok(())
    }

    /// Children that do not fit along the axis start a new line.
    pub fn set_wrap(&mut self, node: NodeId, wrap: bool) -> Result<(), UiError> {
        self.require_flex(node, "wrap")?;
        self.update_style(node, |style| {
            style.flex_wrap = if wrap {
                FlexWrap::Wrap
            } else {
                FlexWrap::NoWrap
            };
            // Wrapped lines pack at the start, as the children on a line do.
            style.align_content = AlignContent::START;
        });
        Ok(())
    }

    pub fn set_anchor(&mut self, node: NodeId, anchor: Anchor) {
        let (justify, align) = anchor_alignment(anchor);
        self.data_mut(node).anchor = Some(anchor);
        self.update_style(node, |style| {
            style.justify_self = Some(justify);
            style.align_self = Some(align);
        });
    }

    pub fn set_role(&mut self, node: NodeId, role: Role) {
        self.data_mut(node).role = role;
        self.semantics_changed();
    }

    pub fn role(&self, node: NodeId) -> Role {
        self.data(node).role
    }

    pub fn set_label(&mut self, node: NodeId, label: &str) {
        self.data_mut(node).label = Some(label.to_string());
        self.semantics_changed();
    }

    pub fn label(&self, node: NodeId) -> Option<&str> {
        self.data(node).label.as_deref()
    }

    /// Children painted (and hit) inside the element's padding box only.
    pub fn set_clip(&mut self, node: NodeId, clip: bool) {
        if self.data(node).clip != clip {
            self.data_mut(node).clip = clip || self.kind(node) == ElementKind::Scroll;
            self.invalidate(node);
        }
    }

    // --------------------------------------------------------------- style ---

    /// Starts a style group on `node`: a new one after every other (`None`),
    /// or `group` emptied to be written again (a bound style's new value).
    /// Answers the group.
    pub fn style_group(&mut self, node: NodeId, group: Option<u32>) -> u32 {
        let layers = &mut self.data_mut(node).layers;
        match group {
            Some(group) => {
                layers.clear_group(group);
                group
            }
            None => layers.new_group(),
        }
    }

    /// A new layer of `group` on `node`, unconditional or applying while
    /// `condition` holds. Answers the layer's id, which the field setters
    /// name.
    pub fn style_layer(
        &mut self,
        node: NodeId,
        group: u32,
        condition: Option<Condition>,
    ) -> Result<u32, UiError> {
        if let Some(condition) = condition {
            for limit in [condition.window_at_least, condition.window_below]
                .into_iter()
                .flatten()
            {
                finite(limit, "a window width")?;
            }
            if condition.reads_width() && !self.width_conditional.contains(&node) {
                self.width_conditional.push(node);
            }
        }
        Ok(self.data_mut(node).layers.add(group, condition))
    }

    /// Writes fields of layer `layer` of `node` (a layer that is gone is
    /// ignored: its group was written again under the caller).
    pub fn set_style_fields(
        &mut self,
        node: NodeId,
        layer: u32,
        change: impl FnOnce(&mut StyleFields),
    ) {
        if let Some(fields) = self.data_mut(node).layers.fields_mut(layer) {
            change(fields);
        }
    }

    /// Resolves `node`'s effective style from its layers, now: a change of a
    /// text field that shapes (font, size, line height, weight, italic)
    /// lays the text under it out again; any other change repaints the
    /// element, and a text colour the texts under it.
    pub fn restyle(&mut self, node: NodeId) {
        let width = self.window_width;
        let data = self.data(node);
        let fields = data.layers.resolve(data.interaction, width);
        let paint = fields.paint();
        if paint != data.paint {
            self.data_mut(node).paint = paint;
            self.invalidate(node);
        }
        let before = self.data(node).text.clone();
        if fields.text != before {
            let colour_only = before.differs_in_colour_only(&fields.text);
            self.data_mut(node).text = fields.text;
            self.pending_text.push(node);
            if colour_only {
                self.paint_generation += 1;
            } else {
                self.layout_changed();
            }
        }
    }

    /// Writes the direct layer (see `style.rs`) and resolves.
    fn set_direct(&mut self, node: NodeId, change: impl FnOnce(&mut StyleFields)) {
        change(self.data_mut(node).layers.direct());
        self.restyle(node);
    }

    pub fn set_background(&mut self, node: NodeId, color: Color) {
        self.set_direct(node, |fields| fields.background = Some(color));
    }

    pub fn set_border(&mut self, node: NodeId, widths: Edges, color: Color) {
        let widths = Edges {
            top: widths.top.max(0.0),
            right: widths.right.max(0.0),
            bottom: widths.bottom.max(0.0),
            left: widths.left.max(0.0),
        };
        self.set_direct(node, |fields| fields.border = Some((widths, color)));
    }

    pub fn set_corners(&mut self, node: NodeId, corners: Corners) {
        self.set_direct(node, |fields| fields.corners = Some(corners));
    }

    pub fn set_shadow(&mut self, node: NodeId, shadow: DropShadow) {
        self.set_direct(node, |fields| fields.shadow = Some(shadow));
    }

    pub fn set_opacity(&mut self, node: NodeId, opacity: f32) -> Result<(), UiError> {
        let opacity = finite(opacity, "an opacity")?.clamp(0.0, 1.0);
        self.set_direct(node, |fields| fields.opacity = Some(opacity));
        Ok(())
    }

    pub fn paint_of(&self, node: NodeId) -> &Paint {
        &self.data(node).paint
    }

    /// Sets some of the element's text style (its direct layer); the rest
    /// stays inherited. A colour alone changes paint only; every other field
    /// reshapes.
    pub fn set_text_style(&mut self, node: NodeId, change: impl FnOnce(&mut TextOverrides)) {
        self.set_direct(node, |fields| change(&mut fields.text));
    }

    /// The element's effective text overrides.
    pub fn text_overrides(&self, node: NodeId) -> &TextOverrides {
        &self.data(node).text
    }

    // ---------------------------------------------------------------- text ---

    /// Appends a run of text to a text element; answers its index, by which
    /// [`UiTree::set_run`] replaces it.
    pub fn append_text(&mut self, node: NodeId, run: &str) -> Result<usize, UiError> {
        let kind = self.kind(node);
        let Content::Text(content) = &mut self.data_mut(node).content else {
            return Err(UiError::WrongKind {
                property: "text",
                kind,
            });
        };
        content.runs.push(run.to_string());
        let index = content.runs.len() - 1;
        if !run.is_empty() {
            content.text.push_str(run);
            content.reshape();
            self.text_changed(node);
        }
        Ok(index)
    }

    /// Replaces run `index` of a text element (a run bound to a signal,
    /// written). The same text again changes nothing.
    pub fn set_run(&mut self, node: NodeId, index: usize, run: &str) -> Result<(), UiError> {
        let kind = self.kind(node);
        let Content::Text(content) = &mut self.data_mut(node).content else {
            return Err(UiError::WrongKind {
                property: "text",
                kind,
            });
        };
        let Some(slot) = content.runs.get_mut(index) else {
            return Err(UiError::NoSuchRun(index));
        };
        if slot == run {
            return Ok(());
        }
        run.clone_into(slot);
        content.text = content.runs.concat();
        content.reshape();
        self.text_changed(node);
        Ok(())
    }

    /// A text's paragraph changed: the solver measures it again (and only
    /// it, through the cache), and it repaints.
    fn text_changed(&mut self, node: NodeId) {
        self.taffy.mark_dirty(node).expect("an element");
        self.dirty.push(node);
        self.invalidate(node);
        self.layout_changed();
    }

    pub fn text(&self, node: NodeId) -> Option<&str> {
        match &self.data(node).content {
            Content::Text(content) => Some(&content.text),
            _ => None,
        }
    }

    pub fn set_texture(&mut self, node: NodeId, image: Image) -> Result<(), UiError> {
        let kind = self.kind(node);
        let Content::Image(content) = &mut self.data_mut(node).content else {
            return Err(UiError::WrongKind {
                property: "texture",
                kind,
            });
        };
        // The solver keeps the aspect wherever one side is decided (a width
        // set, a stretch, a grid cell), not only where it asks the measure.
        let aspect = image.width() as f32 / image.height() as f32;
        content.image = Some(image);
        self.invalidate(node);
        self.update_style(node, |style| style.aspect_ratio = Some(aspect));
        Ok(())
    }

    pub fn set_fit(&mut self, node: NodeId, fit: Fit) -> Result<(), UiError> {
        let kind = self.kind(node);
        let Content::Image(content) = &mut self.data_mut(node).content else {
            return Err(UiError::WrongKind {
                property: "fit",
                kind,
            });
        };
        content.fit = fit;
        self.invalidate(node);
        Ok(())
    }

    fn text_content(&mut self, node: NodeId) -> Result<&mut TextContent, UiError> {
        let kind = self.kind(node);
        match &mut self.data_mut(node).content {
            Content::Text(content) => Ok(content),
            _ => Err(UiError::WrongKind {
                property: "caret",
                kind,
            }),
        }
    }

    /// `node`, a text, EDITED (`Editing`): its caret and the other end of
    /// its selection at the program's UTF-16 indices `caret` and `anchor`,
    /// drawn while `active`. The first call makes it an edited text, which
    /// lays it out again; every later one repaints it alone. While it is
    /// active, the next layout scrolls its caret into view.
    pub fn text_edit(
        &mut self,
        node: NodeId,
        caret: usize,
        anchor: usize,
        active: bool,
    ) -> Result<(), UiError> {
        let content = self.text_content(node)?;
        let first = content.editing.is_none();
        let editing = content.editing.get_or_insert_with(Editing::default);
        let changed = (editing.caret, editing.anchor, editing.active) != (caret, anchor, active);
        editing.caret = caret;
        editing.anchor = anchor;
        editing.active = active;
        if first {
            // Shaped again, with its graphemes' geometry, and measured again.
            content.reshape();
            self.text_changed(node);
        } else if changed {
            self.invalidate(node);
        }
        if active && !self.caret_reveals.contains(&node) {
            self.caret_reveals.push(node);
        }
        Ok(())
    }

    /// An edited text's caret colour (its text colour when `None`) and width
    /// in dp, and its selection's colour (its text colour at a quarter of
    /// its alpha when `None`).
    pub fn text_edit_look(
        &mut self,
        node: NodeId,
        caret_color: Option<Color>,
        caret_width: f32,
        selection_color: Option<Color>,
    ) -> Result<(), UiError> {
        let caret_width = finite(caret_width, "a caret width")?.max(0.0);
        let content = self.text_content(node)?;
        let editing = content.editing.get_or_insert_with(Editing::default);
        let reshapes = editing.caret_width != caret_width;
        editing.caret_color = caret_color;
        editing.caret_width = caret_width;
        editing.selection_color = selection_color;
        if reshapes {
            // The caret's room is part of its measure.
            content.reshape();
            self.text_changed(node);
        } else {
            self.invalidate(node);
        }
        Ok(())
    }

    /// `node`, a text, edited as a MULTI-LINE field's (`std::ui::kit`'s text
    /// area): a word longer than its line breaks where it must, so nothing
    /// runs out of the box it wraps in. Lays it out again the first time.
    pub fn text_multiline(&mut self, node: NodeId) -> Result<(), UiError> {
        let content = self.text_content(node)?;
        let editing = content.editing.get_or_insert_with(Editing::default);
        if !editing.break_words {
            editing.break_words = true;
            content.reshape();
            self.text_changed(node);
        }
        Ok(())
    }

    /// An edited text's layout at the width it was last laid out at (one
    /// line, before its first layout), shaped again if its text changed
    /// since.
    fn edited_layout(
        &mut self,
        node: NodeId,
        text: &mut TextSystem,
    ) -> Result<TextLayout, UiError> {
        let content = self.text_content(node)?;
        if content.editing.is_none() {
            return Err(UiError::NotEdited);
        }
        let constraint = content.laid_out_at;
        Ok(content.shaped_for(constraint, text))
    }

    /// The UTF-16 index in an edited text that `point` (dp from its root's
    /// top-left corner, as the window reports the pointer: the offsets of
    /// the scrolls around it are taken into account) lands on, by `rule`
    /// ([`IndexRule`]).
    pub fn text_index_at(
        &mut self,
        node: NodeId,
        point: Point,
        rule: IndexRule,
        text: &mut TextSystem,
    ) -> Result<usize, UiError> {
        let layout = self.edited_layout(node, text)?;
        let origin = self.content_origin_on_screen(node);
        let local = |point: Point| Point::new(point.x - origin.x, point.y - origin.y);
        // The part of the text that shows: a drag's rules read past its
        // edges.
        let shown = self.shown_on_screen(node);
        let byte = match rule {
            IndexRule::Nearest => layout.index_at(local(point)),
            IndexRule::Under => layout.grapheme_at(local(point)),
            IndexRule::DragLines if point.y < shown.top() || point.y > shown.bottom() => {
                let downward = point.y > shown.bottom();
                let inside = Point::new(
                    point.x.clamp(shown.left(), shown.right()),
                    point.y.clamp(shown.top(), shown.bottom()),
                );
                let goal = local(inside).x;
                let mut at = layout.index_at(local(inside)).ok_or(UiError::NotEdited)?;
                // Its line shows whole: the caret's top and bottom (its
                // line's) inside what shows, to a thousandth of a dp.
                let shows_whole = |index: usize| {
                    layout.caret(index, 0.0).is_some_and(|caret| {
                        let top = origin.y + caret.top();
                        top >= shown.top() - WIDTH_EPSILON
                            && top + caret.size.height <= shown.bottom() + WIDTH_EPSILON
                    })
                };
                // Out from the edge a line at a time to the first line that
                // does not show whole: its reveal scrolls it in, and the
                // next move lands one further.
                while shows_whole(at) {
                    let next = layout
                        .vertical(at, goal, if downward { 1 } else { -1 })
                        .ok_or(UiError::NotEdited)?;
                    if next == at {
                        break;
                    }
                    at = next;
                }
                Some(at)
            }
            IndexRule::Drag | IndexRule::DragLines => {
                let inside = Point::new(point.x.clamp(shown.left(), shown.right()), point.y);
                let mut at = layout.index_at(local(inside)).ok_or(UiError::NotEdited)?;
                let rightward = point.x > shown.right();
                if rightward || point.x < shown.left() {
                    let width = self.caret_width(node);
                    let caret_x = |index: usize| {
                        layout
                            .caret(index, width)
                            .map(|caret| origin.x + caret.left())
                    };
                    let shows_whole = |index: usize| {
                        caret_x(index)
                            .is_some_and(|x| x >= shown.left() && x + width <= shown.right())
                    };
                    // The neighbouring index drawn further the pointer's way
                    // (logical order runs either way in a mixed-direction
                    // text).
                    let outward = |from: usize| {
                        let here = caret_x(from)?;
                        [true, false]
                            .into_iter()
                            .filter_map(|forward| layout.step(from, forward, false))
                            .filter(|index| *index != from)
                            .filter_map(|index| Some((index, caret_x(index)? - here)))
                            .filter(|(_, moved)| {
                                if rightward {
                                    *moved > 0.0
                                } else {
                                    *moved < 0.0
                                }
                            })
                            .min_by(|first, second| first.1.abs().total_cmp(&second.1.abs()))
                            .map(|(index, _)| index)
                    };
                    // Out from the edge to the first index whose caret does
                    // not show whole: its reveal scrolls it in, and the next
                    // move lands one further.
                    while shows_whole(at) {
                        match outward(at) {
                            Some(next) => at = next,
                            None => break,
                        }
                    }
                }
                Some(at)
            }
        }
        .ok_or(UiError::NotEdited)?;
        Ok(unit_of_byte(self.text(node).unwrap_or(""), byte))
    }

    /// The start (or with `end`, the end) of the run of word graphemes — or
    /// of the others — around the grapheme at UTF-16 `index` of an edited
    /// text (the last grapheme at the end of the text): a double click's word.
    pub fn text_word(
        &mut self,
        node: NodeId,
        index: usize,
        end: bool,
        text: &mut TextSystem,
    ) -> Result<usize, UiError> {
        let layout = self.edited_layout(node, text)?;
        let content = self.text(node).unwrap_or("");
        let (start, finish) = layout
            .word_around(byte_of_unit(content, index))
            .ok_or(UiError::NotEdited)?;
        Ok(unit_of_byte(content, if end { finish } else { start }))
    }

    fn caret_width(&self, node: NodeId) -> f32 {
        match &self.data(node).content {
            Content::Text(TextContent {
                editing: Some(editing),
                ..
            }) => editing.caret_width,
            _ => 0.0,
        }
    }

    /// The part of `node`'s content box that shows, in dp from its root's
    /// top-left corner: its box moved by the offset of every scroll around
    /// it and cut by every clipping element around it (to its padding box,
    /// where its children are drawn and hit).
    fn shown_on_screen(&self, node: NodeId) -> Rect {
        let data = self.data(node);
        let (mut left, mut top) = (
            data.bounds.left() + data.content_offset.x,
            data.bounds.top() + data.content_offset.y,
        );
        let (mut right, mut bottom) = (
            left + data.content_size.width,
            top + data.content_size.height,
        );
        let mut current = node;
        while let Some(parent) = self.taffy.parent(current) {
            if self.kind(parent) == ElementKind::Viewport {
                break;
            }
            let data = self.data(parent);
            if let Some(scroll) = &data.scroll {
                left -= scroll.offset.x;
                right -= scroll.offset.x;
                top -= scroll.offset.y;
                bottom -= scroll.offset.y;
            }
            if data.clip {
                // Both in the parent's coordinates now: the offsets of the
                // scrolls further up move them alike.
                let (area, _) = padding_box(data);
                left = left.max(area.left());
                top = top.max(area.top());
                right = right.min(area.right()).max(left);
                bottom = bottom.min(area.bottom()).max(top);
            }
            current = parent;
        }
        Rect::new(left, top, right - left, bottom - top)
    }

    /// The UTF-16 index of the next (`forward`) or previous grapheme
    /// boundary of an edited text from `index`, or with `word` the end of the
    /// next word or the start of the previous one (`text/editing.rs`).
    pub fn text_step(
        &mut self,
        node: NodeId,
        index: usize,
        forward: bool,
        word: bool,
        text: &mut TextSystem,
    ) -> Result<usize, UiError> {
        let layout = self.edited_layout(node, text)?;
        let content = self.text(node).unwrap_or("");
        let byte = byte_of_unit(content, index);
        let stepped = layout.step(byte, forward, word).ok_or(UiError::NotEdited)?;
        Ok(unit_of_byte(content, stepped))
    }

    /// The UTF-16 index of the start (or with `end`, the last caret
    /// position) of the visual line `index` of an edited text is on: Home's
    /// and End's in a text area (`text/editing.rs` says which line an index
    /// at a soft wrap is on).
    pub fn text_line_end(
        &mut self,
        node: NodeId,
        index: usize,
        end: bool,
        text: &mut TextSystem,
    ) -> Result<usize, UiError> {
        let layout = self.edited_layout(node, text)?;
        let content = self.text(node).unwrap_or("");
        let byte = byte_of_unit(content, index);
        let found = layout.line_end(byte, end).ok_or(UiError::NotEdited)?;
        Ok(unit_of_byte(content, found))
    }

    /// The UTF-16 index of an edited text `lines` visual lines below
    /// `index`'s (above, when negative) nearest `goal_x` (dp from the text's
    /// left: the x a run of vertical moves started from); its start past
    /// the first line, its end past the last.
    pub fn text_vertical(
        &mut self,
        node: NodeId,
        index: usize,
        goal_x: f32,
        lines: isize,
        text: &mut TextSystem,
    ) -> Result<usize, UiError> {
        let goal_x = finite(goal_x, "a goal x")?;
        let layout = self.edited_layout(node, text)?;
        let content = self.text(node).unwrap_or("");
        let byte = byte_of_unit(content, index);
        let found = layout
            .vertical(byte, goal_x, lines)
            .ok_or(UiError::NotEdited)?;
        Ok(unit_of_byte(content, found))
    }

    /// The caret's box at UTF-16 `index` of an edited text, in dp from the
    /// text's top-left corner (its line's top and height; no width): where
    /// a vertical move's goal x comes from, and how far a page moved.
    pub fn text_caret_box(
        &mut self,
        node: NodeId,
        index: usize,
        text: &mut TextSystem,
    ) -> Result<Rect, UiError> {
        let layout = self.edited_layout(node, text)?;
        let content = self.text(node).unwrap_or("");
        layout
            .caret(byte_of_unit(content, index), 0.0)
            .ok_or(UiError::NotEdited)
    }

    /// Where `node`'s content box is drawn, in dp from its root's top-left
    /// corner: its laid-out place moved by the offset of every scroll around
    /// it.
    fn content_origin_on_screen(&self, node: NodeId) -> Point {
        let data = self.data(node);
        let mut origin = Point::new(
            data.bounds.left() + data.content_offset.x,
            data.bounds.top() + data.content_offset.y,
        );
        let mut current = node;
        while let Some(parent) = self.taffy.parent(current) {
            if self.kind(parent) == ElementKind::Viewport {
                break;
            }
            if let Some(scroll) = &self.data(parent).scroll {
                origin.x -= scroll.offset.x;
                origin.y -= scroll.offset.y;
            }
            current = parent;
        }
        origin
    }

    /// Scrolls the caret of every edited text that asked into view (after a
    /// layout, so from its boxes).
    fn reveal_carets(&mut self, text: &mut TextSystem) {
        for node in std::mem::take(&mut self.caret_reveals) {
            if !self.is_alive(node) {
                continue;
            }
            let data = self.data(node);
            let origin = Point::new(
                data.bounds.left() + data.content_offset.x,
                data.bounds.top() + data.content_offset.y,
            );
            let Ok(layout) = self.edited_layout(node, text) else {
                continue;
            };
            let content = self.text(node).unwrap_or("");
            let Content::Text(TextContent {
                editing: Some(editing),
                ..
            }) = &self.data(node).content
            else {
                continue;
            };
            if !editing.active {
                continue;
            }
            let width = editing.caret_width;
            if let Some(caret) = layout.caret(byte_of_unit(content, editing.caret), width) {
                let area = Rect::new(
                    origin.x + caret.left(),
                    origin.y + caret.top(),
                    width,
                    caret.size.height,
                );
                self.reveal_area(node, area);
            }
        }
    }

    /// The element's text style after inheritance, as of the last layout.
    pub fn resolved_text(&self, node: NodeId) -> ResolvedText {
        (*self.data(node).resolved).clone()
    }

    // -------------------------------------------------------------- layout ---

    fn root_state(&mut self, root: NodeId) -> Result<RootState, UiError> {
        if let Some(parent) = self.taffy.parent(root)
            && self.kind(parent) != ElementKind::Viewport
        {
            return Err(UiError::NotARoot);
        }
        if let Some(state) = &self.data(root).root {
            return Ok(state.clone());
        }
        let viewport = self.create(ElementKind::Viewport);
        self.data_mut(viewport).handle = false;
        self.taffy
            .add_child(viewport, root)
            .expect("the root in its viewport");
        let state = RootState {
            viewport,
            solved: None,
            snapped: None,
            solves: 0,
        };
        self.data_mut(root).root = Some(state.clone());
        self.pending_text.push(root);
        Ok(state)
    }

    /// Lays `root` out in a `size` dp viewport (it fills it unless it sizes
    /// itself) and snaps every box to the device grid at `units`. Runs the
    /// solver only when the size or the layout generation moved; snaps only
    /// when the solver ran or the scale changed. Answers whether the solver
    /// ran.
    pub fn layout(
        &mut self,
        root: NodeId,
        size: Size,
        units: Units,
        text: &mut TextSystem,
    ) -> Result<bool, UiError> {
        let width = finite(size.width, "a layout width")?.max(0.0);
        let height = finite(size.height, "a layout height")?.max(0.0);
        let size = Size::new(width, height);
        let mut state = self.root_state(root)?;
        let mut chains = None;
        self.stats.solved = false;
        self.stats.measured = 0;
        self.stats.moved = 0;
        // A new window width first: a condition on it may change a style,
        // and a style may change the layout.
        if self.window_width != Some(width) {
            self.window_width = Some(width);
            self.restyle_width_conditional();
        }
        // Before deciding whether to solve: a colour alone resolves without
        // a solve, and a reshaping style has moved the layout generation.
        self.resolve_pending_text();
        let solve = state.solved != Some((size, self.layout_generation));
        if solve {
            let viewport_style = self.taffy.style(state.viewport).expect("the viewport");
            if viewport_style.size.width != Dimension::length(width)
                || viewport_style.size.height != Dimension::length(height)
            {
                let mut style = viewport_style.clone();
                style.size = taffy::Size {
                    width: Dimension::length(width),
                    height: Dimension::length(height),
                };
                // The root's cell is exactly the viewport: FIXED tracks, so
                // the grid never asks the root for its content's smallest or
                // largest size. An `auto` track would, and `taffy` keys those
                // answers by the parent's size: after a resize the first
                // re-solve with the root dirty measured every leaf of the
                // tree again (180,000 measures for a 10,000-row list) — the
                // resize pass itself had answered those questions from
                // entries made at the old size.
                style.grid_template_columns = vec![taffy::style_helpers::length(width)];
                style.grid_template_rows = vec![taffy::style_helpers::length(height)];
                self.taffy
                    .set_style(state.viewport, style)
                    .expect("the viewport's size");
                self.dirty.push(state.viewport);
            }
            // The chains the solver re-runs: every other element keeps its
            // cached layout unless its parent placed it elsewhere.
            let mut changed = std::collections::HashSet::new();
            let mut other_trees = Vec::new();
            for node in std::mem::take(&mut self.dirty) {
                if !self.is_alive(node) {
                    continue;
                }
                if node != state.viewport && self.root_of(node) != root {
                    // Another tree's change, for its own next solve.
                    other_trees.push(node);
                    continue;
                }
                let mut current = Some(node);
                while let Some(node) = current {
                    if !self.is_alive(node) || !changed.insert(node) {
                        break;
                    }
                    current = self.taffy.parent(node);
                }
            }
            self.dirty = other_trees;
            chains = Some(changed);
            let mut measured = 0;
            let mut leaves = std::mem::take(&mut self.measured);
            leaves.clear();
            self.taffy
                .compute_layout_with_measure(
                    state.viewport,
                    taffy::Size {
                        width: AvailableSpace::Definite(width),
                        height: AvailableSpace::Definite(height),
                    },
                    |inputs, node, data, style| {
                        measured += 1;
                        if leaves.last() != Some(&node) && !leaves.contains(&node) {
                            leaves.push(node);
                        }
                        measure(inputs, data, style, text)
                    },
                )
                .expect("the layout");
            self.measured = leaves;
            self.stats.solved = true;
            self.stats.measured = measured;
            state.solved = Some((size, self.layout_generation));
            state.solves += 1;
        }
        let rescaled = state.snapped.map(|(snapped, _)| snapped) != Some(units);
        if solve || rescaled {
            // A new scale moves every box; a solve, only what it re-ran.
            self.snap(root, units, chains.as_ref().filter(|_| !rescaled));
            state.snapped = Some((units, state.solves));
        }
        self.data_mut(root).root = Some(state);
        if let Some(node) = self.pending_reveal
            && self.is_alive(node)
            && self.root_of(node) == root
        {
            self.pending_reveal = None;
            self.reveal(node);
        }
        self.reveal_carets(text);
        Ok(solve)
    }

    fn restyle_width_conditional(&mut self) {
        let nodes = std::mem::take(&mut self.width_conditional);
        let mut kept = Vec::with_capacity(nodes.len());
        for node in nodes {
            if self.is_alive(node) && self.data(node).layers.reads_width() {
                self.restyle(node);
                kept.push(node);
            }
        }
        self.width_conditional = kept;
    }

    /// Resolves the inherited text style of every subtree a change named
    /// (a style set, a child added, a root laid out first), top down from
    /// its parent's, and marks a text element whose shaping style changed
    /// for the solver and one whose colour changed for paint.
    fn resolve_pending_text(&mut self) {
        let pending = std::mem::take(&mut self.pending_text);
        for start in pending {
            if !self.is_alive(start) {
                continue;
            }
            let inherited = match self.taffy.parent(start) {
                Some(parent) if self.kind(parent) != ElementKind::Viewport => {
                    self.data(parent).resolved.clone()
                }
                _ => Rc::new(ResolvedText::default()),
            };
            let mut walk = vec![(start, inherited)];
            while let Some((node, inherited)) = walk.pop() {
                let data = self.data(node);
                let resolved = if data.text.is_empty() {
                    inherited
                } else {
                    Rc::new(inherited.with(&data.text))
                };
                let data = self.data_mut(node);
                let previous = std::mem::replace(&mut data.resolved, resolved.clone());
                let mut reshape = false;
                let mut repaint = false;
                match &mut data.content {
                    Content::Text(content) => {
                        reshape = content.resolved.style != resolved.style;
                        repaint = content.resolved.color != resolved.color;
                        content.resolved = resolved.clone();
                        if reshape {
                            content.reshape();
                        }
                    }
                    Content::Icon(content) => {
                        (reshape, repaint) = content.follows(&previous, &resolved);
                    }
                    Content::Parent | Content::Image(_) => {}
                }
                if reshape {
                    self.taffy.mark_dirty(node).expect("an element");
                    self.dirty.push(node);
                    self.layout_changed();
                }
                if reshape || repaint {
                    self.invalidate(node);
                }
                for index in 0..self.taffy.child_count(node) {
                    let child = self.taffy.get_child_id(node, index);
                    walk.push((child, resolved.clone()));
                }
            }
        }
    }

    /// Snaps every box under `root` to the device grid: each edge on its own,
    /// by [`Units`]' rule (the renderer's), so boxes that share an edge in dp
    /// share it on the device and a hairline stays one pixel (R16). A scroll's
    /// range follows its child, and its offset is clamped into it.
    ///
    /// What a box that moved repaints (F170): an element's scene is in its
    /// own coordinates, placed by its parent's scene at its offset in the
    /// parent's box. A box whose size or content box changed is repainted; a
    /// box that only moved within its parent repaints nothing of its own, its
    /// parent's scene places it anew; a box that moved with its parent
    /// changes no scene at all.
    ///
    /// With `chains` (the elements the solver re-ran, and their ancestors),
    /// only what can have moved is visited: the chains, the children the
    /// chains placed, and everything under a box that moved. An element off
    /// the chains whose box is where it was kept its cached layout — the
    /// solver lays a node's children out from its own final size and style,
    /// and neither changed — so nothing under it moved either.
    fn snap(
        &mut self,
        root: NodeId,
        units: Units,
        chains: Option<&std::collections::HashSet<NodeId>>,
    ) {
        let scale = units.content_scale();
        // Each with its parent's layout origin and its parent's snapped box's
        // corner (the window's for the root).
        let mut pending = vec![(root, Point::ZERO, Point::ZERO)];
        let mut scrolls = Vec::new();
        while let Some((node, parent_origin, parent_corner)) = pending.pop() {
            let layout = *self.taffy.layout(node).expect("a laid-out element");
            let origin = Point::new(
                parent_origin.x + layout.location.x,
                parent_origin.y + layout.location.y,
            );
            let (x, width) = units.snap_span(origin.x, layout.size.width);
            let (y, height) = units.snap_span(origin.y, layout.size.height);
            let inset_left = layout.padding.left + layout.border.left;
            let inset_top = layout.padding.top + layout.border.top;
            let bounds = Rect::new(x / scale, y / scale, width / scale, height / scale);
            let content_offset = Point::new(inset_left, inset_top);
            let content_size = Size::new(
                (layout.size.width - inset_left - layout.padding.right - layout.border.right)
                    .max(0.0),
                (layout.size.height - inset_top - layout.padding.bottom - layout.border.bottom)
                    .max(0.0),
            );
            let placed = Point::new(
                bounds.left() - parent_corner.x,
                bounds.top() - parent_corner.y,
            );
            let data = self.data_mut(node);
            let moved = data.bounds != bounds
                || data.content_offset != content_offset
                || data.content_size != content_size;
            let reshaped = (data.bounds.size != bounds.size && data.scene_reads_size)
                || data.content_offset != content_offset
                || data.content_size != content_size;
            let replaced = data.placed != placed;
            let resized = data.bounds.size != bounds.size
                && data
                    .handlers
                    .iter()
                    .any(|(kind, _)| *kind == EventKind::Resize);
            data.bounds = bounds;
            data.placed = placed;
            data.content_offset = content_offset;
            data.content_size = content_size;
            if let Content::Text(TextContent {
                editing: Some(editing),
                laid_out_at,
                ..
            }) = &mut data.content
            {
                *laid_out_at = Some((content_size.width - editing.caret_width).max(0.0));
            }
            if data.scroll.is_some() {
                scrolls.push(node);
            }
            if moved {
                self.stats.moved += 1;
                self.hover_stale = true;
            }
            if reshaped {
                self.invalidate(node);
            }
            if replaced
                && let Some(parent) = self.taffy.parent(node)
                && self.kind(parent) != ElementKind::Viewport
            {
                self.invalidate(parent);
            }
            if resized {
                self.resize_notices.push(node);
            }
            let descend = moved || chains.is_none_or(|chains| chains.contains(&node));
            if descend {
                for index in 0..self.taffy.child_count(node) {
                    let child = self.taffy.get_child_id(node, index);
                    if self.kind(child) != ElementKind::Marker {
                        pending.push((child, origin, bounds.origin));
                    }
                }
            }
        }
        for scroll in scrolls {
            self.measure_scroll_range(scroll);
        }
    }

    /// A scroll's range from its child's laid-out box, and its offset
    /// clamped into it.
    fn measure_scroll_range(&mut self, node: NodeId) {
        let generation = self.layout_generation;
        let data = self.data(node);
        let content = Rect::new(
            data.bounds.left() + data.content_offset.x,
            data.bounds.top() + data.content_offset.y,
            data.content_size.width,
            data.content_size.height,
        );
        let mut reach = Size::new(0.0, 0.0);
        for index in 0..self.taffy.child_count(node) {
            let child = self.data(self.taffy.get_child_id(node, index));
            if child.kind == ElementKind::Marker {
                continue;
            }
            let child = child.bounds;
            reach.width = reach.width.max(child.right() - content.left());
            reach.height = reach.height.max(child.bottom() - content.top());
        }
        let range = Size::new(
            (reach.width - content.size.width).max(0.0),
            (reach.height - content.size.height).max(0.0),
        );
        let scroll = self.data_mut(node).scroll.as_mut().expect("a scroll");
        scroll.range = range;
        scroll.measured_at = Some(generation);
        let clamped = clamp_offset(scroll.offset, range, scroll.axis);
        if clamped != scroll.offset {
            scroll.offset = clamped;
            self.offset_notices.push(node);
            self.hover_stale = true;
            self.invalidate(node);
        }
    }

    /// The element's snapped box from its last layout, in dp from its root's
    /// top-left corner.
    pub fn bounds(&self, node: NodeId) -> Rect {
        self.data(node).bounds
    }

    /// The element's size as the solver gave it in its last layout, in dp,
    /// before snapping: what the boxes after it in its parent are placed by
    /// (a snapped size depends on where the box sits).
    pub fn solved_size(&self, node: NodeId) -> Size {
        let size = self.taffy.layout(node).expect("a laid-out element").size;
        Size::new(size.width, size.height)
    }

    /// The part of the element its children are drawn in, from its last
    /// layout — its padding box (a clipping element's clip) — down its
    /// height: the content box and the padding above and below it.
    pub fn inner_box(&self, node: NodeId) -> InnerBox {
        let data = self.data(node);
        let (area, _) = padding_box(data);
        let content_top = data.bounds.top() + data.content_offset.y;
        let content_height = data.content_size.height;
        InnerBox {
            padding_top: (content_top - area.top()).max(0.0),
            content_height,
            padding_bottom: (area.bottom() - content_top - content_height).max(0.0),
        }
    }

    /// Every element under `root`, `root` first, in tree (paint) order.
    pub fn outline(&self, root: NodeId) -> Vec<OutlineEntry> {
        let mut entries = Vec::new();
        let mut pending = vec![(root, 0)];
        while let Some((node, depth)) = pending.pop() {
            let data = self.data(node);
            entries.push(OutlineEntry {
                depth,
                kind: data.kind,
                label: data.label.clone(),
                bounds: data.bounds,
            });
            for child in self.children(node).into_iter().rev() {
                if self.kind(child) != ElementKind::Marker {
                    pending.push((child, depth + 1));
                }
            }
        }
        entries
    }

    // -------------------------------------------------------------- scroll ---

    /// Which way a scroll moves; its child is laid out again.
    pub fn set_scroll_axis(&mut self, node: NodeId, axis: ScrollAxis) -> Result<(), UiError> {
        let kind = self.kind(node);
        let Some(scroll) = self.data_mut(node).scroll.as_mut() else {
            return Err(UiError::WrongKind {
                property: "axis",
                kind,
            });
        };
        scroll.axis = axis;
        self.update_style(node, |style| scroll_style(style, axis));
        Ok(())
    }

    /// Where a scroll's child is moved to, as the program sets it: clamped
    /// into the range of the last layout while that range is current, kept as
    /// asked while a layout is pending (the next one clamps it, and tells the
    /// program when that moved it). A paint-only change of the scroll: nothing
    /// inside it is laid out or repainted.
    pub fn set_scroll_offset(&mut self, node: NodeId, offset: Point) -> Result<(), UiError> {
        let kind = self.kind(node);
        finite(offset.x, "a scroll offset")?;
        finite(offset.y, "a scroll offset")?;
        let generation = self.layout_generation;
        let Some(scroll) = self.data_mut(node).scroll.as_mut() else {
            return Err(UiError::WrongKind {
                property: "offset",
                kind,
            });
        };
        let clamped = if scroll.measured_at == Some(generation) {
            clamp_offset(offset, scroll.range, scroll.axis)
        } else {
            offset
        };
        if clamped != scroll.offset {
            scroll.offset = clamped;
            self.hover_stale = true;
            self.invalidate(node);
        }
        if clamped != offset {
            self.offset_notices.push(node);
        }
        Ok(())
    }

    pub fn scroll_offset(&self, node: NodeId) -> Option<Point> {
        self.data(node).scroll.as_ref().map(|scroll| scroll.offset)
    }

    /// How far a scroll's child reaches past its viewport, from the last
    /// layout.
    pub fn scroll_range(&self, node: NodeId) -> Option<Size> {
        self.data(node).scroll.as_ref().map(|scroll| scroll.range)
    }

    /// Called when the offset moves other than by `set_scroll_offset`'s
    /// value: a wheel, a clamp.
    pub fn on_scroll_offset(&mut self, node: NodeId, handler: Handler) -> Result<(), UiError> {
        let kind = self.kind(node);
        let Some(scroll) = self.data_mut(node).scroll.as_mut() else {
            return Err(UiError::WrongKind {
                property: "offset",
                kind,
            });
        };
        scroll.on_offset = Some(handler);
        Ok(())
    }

    /// The resize handlers of the elements whose size a layout changed since
    /// the last call, each element once, with the size it has now (in dp).
    pub fn take_resize_notices(&mut self) -> Vec<(Handler, Size)> {
        let mut notices = std::mem::take(&mut self.resize_notices);
        let mut seen = std::collections::HashSet::new();
        notices.retain(|node| seen.insert(*node));
        let mut calls = Vec::new();
        for node in notices {
            if !self.is_alive(node) {
                continue;
            }
            let data = self.data(node);
            for (kind, handler) in &data.handlers {
                if *kind == EventKind::Resize {
                    calls.push((handler.clone(), data.bounds.size));
                }
            }
        }
        calls
    }

    /// The handlers of the scrolls whose offsets moved by the framework
    /// since the last call, each once.
    pub fn take_offset_notices(&mut self) -> Vec<Handler> {
        let mut notices = std::mem::take(&mut self.offset_notices);
        notices.sort_by_key(|node| u64::from(*node));
        notices.dedup();
        notices
            .into_iter()
            .filter(|node| self.is_alive(*node))
            .filter_map(|node| self.data(node).scroll.as_ref()?.on_offset.clone())
            .collect()
    }

    // --------------------------------------------------------------- paint ---

    /// Appends `root`'s scene to `builder`: each element's scene rebuilt
    /// where something in it changed since the last paint, kept otherwise.
    /// Lay the root out first.
    pub fn paint(
        &mut self,
        root: NodeId,
        builder: &mut SceneBuilder,
        text: &mut TextSystem,
    ) -> Result<(), UiError> {
        self.root_state(root)?;
        self.stats.rebuilt = 0;
        let scene = self.compose(root, text);
        builder.scene_at(&scene, self.data(root).placed);
        Ok(())
    }

    /// `node`'s subtree scene, in its own coordinates (its box's top-left
    /// corner at the origin): the kept one, or rebuilt from its own look and
    /// its children's (kept or rebuilt) scenes, each placed at its offset in
    /// this box. Every box is snapped, so every offset is a whole number of
    /// device pixels and a placed scene draws the pixels it drew in window
    /// coordinates (see `SceneBuilder::scene_at`).
    fn compose(&mut self, node: NodeId, text: &mut TextSystem) -> Scene {
        if let Some(scene) = &self.data(node).scene {
            return scene.clone();
        }
        let count = self.taffy.child_count(node);
        let mut children = Vec::with_capacity(count);
        for index in 0..count {
            let child = self.taffy.get_child_id(node, index);
            // One look at the child's data for its kind, its place and its
            // kept scene: a long list's parent reads ten thousand of them.
            let data = self.data(child);
            if data.kind == ElementKind::Marker {
                continue;
            }
            let placed = data.placed;
            let scene = match &data.scene {
                Some(scene) => scene.clone(),
                None => self.compose(child, text),
            };
            children.push((scene, placed));
        }
        let mut builder = SceneBuilder::new();
        let opacity = self.data(node).paint.opacity;
        if opacity < 1.0 {
            builder.push_layer(opacity);
        }
        let mut reads_size = self.paint_element(node, &mut builder, text);
        let data = self.data(node);
        let clipped = data.clip && !children.is_empty();
        reads_size |= clipped;
        if clipped {
            let (area, corners) = padding_box_of(own_box(data), &data.paint);
            builder.push_clip(area, corners);
        }
        let shifted = data
            .scroll
            .as_ref()
            .map(|scroll| scroll.offset)
            .filter(|offset| *offset != Point::ZERO);
        if let Some(offset) = shifted {
            builder.push_transform(Transform::translate(-offset.x, -offset.y));
        }
        for (child, placed) in &children {
            builder.scene_at(child, *placed);
        }
        if shifted.is_some() {
            builder.pop();
        }
        if clipped {
            builder.pop();
        }
        if opacity < 1.0 {
            builder.pop();
        }
        let scene = builder.finish().expect("a balanced element scene");
        self.stats.rebuilt += 1;
        let data = self.data_mut(node);
        data.scene = Some(scene.clone());
        data.scene_reads_size = reads_size;
        scene
    }

    /// One element's own primitives, in its own coordinates: shadow,
    /// background and border, then its content (an image, a paragraph).
    /// Answers whether they draw the box's snapped size (see
    /// `NodeData::scene_reads_size`).
    fn paint_element(
        &mut self,
        node: NodeId,
        builder: &mut SceneBuilder,
        text: &mut TextSystem,
    ) -> bool {
        let data = self.data(node);
        let bounds = own_box(data);
        let paint = data.paint.clone();
        let bordered = paint.border_widths != Edges::default();
        let reads_size = paint.shadow.is_some() || paint.background.is_some() || bordered;
        if let Some(shadow) = paint.shadow {
            let spread = shadow.spread;
            builder.shadow(Shadow {
                bounds: Rect::new(
                    bounds.left() + shadow.offset.x - spread,
                    bounds.top() + shadow.offset.y - spread,
                    (bounds.size.width + 2.0 * spread).max(0.0),
                    (bounds.size.height + 2.0 * spread).max(0.0),
                ),
                corner_radius: (paint.corners.top_left + spread).max(0.0),
                blur: shadow.blur.max(0.0),
                color: shadow.color,
            });
        }
        if paint.background.is_some() || bordered {
            builder.rectangle(Rectangle {
                bounds,
                corner_radii: paint.corners,
                fill: Fill::Solid(paint.background.unwrap_or(Color::new(0.0, 0.0, 0.0, 0.0))),
                border: Border {
                    widths: paint.border_widths,
                    color: paint.border_color,
                },
            });
        }
        let color = data.resolved.color;
        let resolved = data.resolved.clone();
        // The content box inside the SNAPPED box, at the solver's size.
        let content_size = data.content_size;
        let placed = Rect::new(
            bounds.left() + data.content_offset.x,
            bounds.top() + data.content_offset.y,
            content_size.width,
            content_size.height,
        );
        match &mut self.data_mut(node).content {
            Content::Parent => {}
            Content::Text(content) => {
                // An edited text's layout is shaped at its text's width, not
                // the caret's extra room (see `Editing`).
                let width = match &content.editing {
                    Some(editing) => (content_size.width - editing.caret_width).max(0.0),
                    None => content_size.width,
                };
                let shown = !content.text.is_empty() || content.editing.is_some();
                if shown {
                    content.laid_out_at = Some(width);
                    let layout = content.shaped_for(Some(width), text);
                    match content.editing.as_ref().filter(|editing| editing.active) {
                        Some(editing) => {
                            paint_edited(builder, &content.text, &layout, editing, placed, color)
                        }
                        None => builder.text(&layout, placed.origin, color),
                    }
                }
            }
            Content::Image(content) => {
                if let Some(image) = &content.image {
                    paint_image(builder, image, content.fit, placed, paint.corners);
                }
            }
            Content::Icon(content) => content.paint(builder, placed.origin, &resolved),
        }
        reads_size
    }
}

/// An edited text with the focus: its selection under its glyphs, and its
/// caret over them, clamped into its box.
fn paint_edited(
    builder: &mut SceneBuilder,
    text: &str,
    layout: &TextLayout,
    editing: &Editing,
    placed: Rect,
    color: Color,
) {
    let origin = placed.origin;
    let selection_color = editing.selection_color.unwrap_or(Color::new(
        color.red,
        color.green,
        color.blue,
        color.alpha * 0.25,
    ));
    let (caret, anchor) = (
        byte_of_unit(text, editing.caret),
        byte_of_unit(text, editing.anchor),
    );
    for area in layout.selection(anchor, caret).unwrap_or_default() {
        builder.rectangle(Rectangle {
            bounds: Rect::new(
                origin.x + area.left(),
                origin.y + area.top(),
                area.size.width,
                area.size.height,
            ),
            corner_radii: Corners::default(),
            fill: Fill::Solid(selection_color),
            border: Border::NONE,
        });
    }
    builder.text(layout, origin, color);
    if let Some(caret) = layout.caret(caret, editing.caret_width) {
        let x = caret
            .left()
            .min(placed.size.width - editing.caret_width)
            .max(0.0);
        builder.rectangle(Rectangle {
            bounds: Rect::new(
                origin.x + x,
                origin.y + caret.top(),
                caret.size.width,
                caret.size.height,
            ),
            corner_radii: Corners::default(),
            fill: Fill::Solid(editing.caret_color.unwrap_or(color)),
            border: Border::NONE,
        });
    }
}

/// An offset kept within `0..=range` on the axes `axis` scrolls, and at 0 on
/// the others.
fn clamp_offset(offset: Point, range: Size, axis: ScrollAxis) -> Point {
    let (horizontal, vertical) = match axis {
        ScrollAxis::Vertical => (false, true),
        ScrollAxis::Horizontal => (true, false),
        ScrollAxis::Both => (true, true),
    };
    Point::new(
        if horizontal {
            offset.x.clamp(0.0, range.width)
        } else {
            0.0
        },
        if vertical {
            offset.y.clamp(0.0, range.height)
        } else {
            0.0
        },
    )
}

/// An element's box in its own coordinates: where its scene draws it.
fn own_box(data: &NodeData) -> Rect {
    Rect {
        origin: Point::ZERO,
        size: data.bounds.size,
    }
}

/// An element's padding box — inside its border — with its corners reduced by
/// the border: where a clipping element's children show.
fn padding_box(data: &NodeData) -> (Rect, Corners) {
    padding_box_of(data.bounds, &data.paint)
}

/// The padding box of a box at `bounds` painted with `paint`.
fn padding_box_of(bounds: Rect, paint: &Paint) -> (Rect, Corners) {
    let widths = paint.border_widths;
    let area = Rect::new(
        bounds.left() + widths.left,
        bounds.top() + widths.top,
        (bounds.size.width - widths.left - widths.right).max(0.0),
        (bounds.size.height - widths.top - widths.bottom).max(0.0),
    );
    let corners = paint.corners;
    let inner = |radius: f32, first: f32, second: f32| (radius - first.max(second)).max(0.0);
    (
        area,
        Corners {
            top_left: inner(corners.top_left, widths.top, widths.left),
            top_right: inner(corners.top_right, widths.top, widths.right),
            bottom_right: inner(corners.bottom_right, widths.bottom, widths.right),
            bottom_left: inner(corners.bottom_left, widths.bottom, widths.left),
        },
    )
}

/// Draws `image` in `area` by its fit rule.
fn paint_image(builder: &mut SceneBuilder, image: &Image, fit: Fit, area: Rect, corners: Corners) {
    if area.is_empty() {
        return;
    }
    let natural = Size::new(image.width() as f32, image.height() as f32);
    let drawn = match fit {
        Fit::Fill => area.size,
        Fit::Contain | Fit::Cover => {
            let horizontal = area.size.width / natural.width;
            let vertical = area.size.height / natural.height;
            let scale = if fit == Fit::Contain {
                horizontal.min(vertical)
            } else {
                horizontal.max(vertical)
            };
            Size::new(natural.width * scale, natural.height * scale)
        }
        Fit::None => natural,
    };
    let destination = Rect::new(
        area.left() + (area.size.width - drawn.width) / 2.0,
        area.top() + (area.size.height - drawn.height) / 2.0,
        drawn.width,
        drawn.height,
    );
    let overflows = drawn.width > area.size.width + WIDTH_EPSILON
        || drawn.height > area.size.height + WIDTH_EPSILON;
    if overflows {
        builder.push_clip(area, corners);
        builder.image(image, destination, Corners::default());
        builder.pop();
    } else {
        builder.image(image, destination, corners);
    }
}

/// The solver's measure of a leaf: a paragraph wrapped to the width it is
/// offered, an image at its texture's size (the aspect kept when one side is
/// known), an icon at its side, nothing for an empty parent.
fn measure(
    inputs: taffy::LayoutInput,
    data: Option<&mut NodeData>,
    style: &taffy::Style,
    text: &mut TextSystem,
) -> taffy::LayoutOutput {
    // A paragraph's first baseline, from its content box's top, for a row
    // that aligns its children on their baselines.
    let mut first_baseline = None;
    let mut output = taffy::compute_leaf_layout(
        inputs,
        style,
        |_, _| 0.0,
        |known, available| {
            let Some(data) = data else {
                return taffy::Size::ZERO;
            };
            match &mut data.content {
                Content::Parent => taffy::Size::ZERO,
                Content::Text(content) => {
                    if content.text.is_empty() && content.editing.is_none() {
                        return taffy::Size {
                            width: known.width.unwrap_or(0.0),
                            height: known.height.unwrap_or(0.0),
                        };
                    }
                    let constraint = known.width.or(match available.width {
                        AvailableSpace::Definite(width) => Some(width.max(0.0)),
                        AvailableSpace::MinContent => Some(0.0),
                        AvailableSpace::MaxContent => None,
                    });
                    // An edited text is shaped at its width less the caret's
                    // room — the width its paint shapes it at — so a text
                    // wrapping at a given width wraps where it is drawn.
                    let caret_room = content
                        .editing
                        .as_ref()
                        .map_or(0.0, |editing| editing.caret_width);
                    let constraint = constraint.map(|width| (width - caret_room).max(0.0));
                    let layout = content.shaped_for(constraint, text);
                    first_baseline = Some(layout.first_baseline());
                    let mut shaped = layout.size();
                    if let Some(editing) = &content.editing {
                        // Room for the caret after the last grapheme, and
                        // one line for an empty text's caret.
                        shaped.width += editing.caret_width;
                        if let Some(caret) = layout.caret(0, 0.0) {
                            shaped.height = shaped.height.max(caret.size.height);
                        }
                    }
                    taffy::Size {
                        width: known.width.unwrap_or(shaped.width),
                        height: known.height.unwrap_or(shaped.height),
                    }
                }
                Content::Image(content) => {
                    let Some(image) = &content.image else {
                        return taffy::Size {
                            width: known.width.unwrap_or(0.0),
                            height: known.height.unwrap_or(0.0),
                        };
                    };
                    let natural_width = image.width() as f32;
                    let natural_height = image.height() as f32;
                    match (known.width, known.height) {
                        (Some(width), Some(height)) => taffy::Size { width, height },
                        (Some(width), None) => taffy::Size {
                            width,
                            height: width * natural_height / natural_width,
                        },
                        (None, Some(height)) => taffy::Size {
                            width: height * natural_width / natural_height,
                            height,
                        },
                        (None, None) => taffy::Size {
                            width: natural_width,
                            height: natural_height,
                        },
                    }
                }
                Content::Icon(content) => content.measure(&data.resolved, known),
            }
        },
    );
    if let Some(baseline) = first_baseline {
        // Padding is a length in std::ui (never a percentage), so its raw
        // value is its dp.
        let inset = style.padding.top.into_raw().value() + style.border.top.into_raw().value();
        output.baselines = taffy::Baselines::from_first(Some(inset + baseline));
    }
    output
}
