//! `std::ui`'s element tree, layout and paint (native-ui.md part P2; rulings
//! R4, R5, R7, R9, R10, R16).
//!
//! # What lives here, and why here
//!
//! The RETAINED element tree is this crate's: a [`UiTree`] holds every element
//! of a program — its kind, its layout properties, its paint, its inherited
//! text style, its accessibility role and label — in one `taffy` tree, and a
//! vilan program holds HANDLES to its elements (`host::ui`'s `Node`, one
//! `Rc` per element). The vilan surface (`vilan/std/src/ui.vl`) is the typed
//! builder vocabulary over it; every attribute call crosses once, as numbers,
//! at build time, and nothing crosses per frame but the root's handle.
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
//!   away. Keeping the tree here keeps it, which is what P3's signal-bound
//!   properties lean on (one write, one dirty chain, one re-solve).
//! - **One Rust stack (R12, R14).** The browser target compiles this same
//!   crate to wasm; a tree mirrored in vilan values would be a second
//!   implementation of the same thing on the other side of the seam.
//!
//! # Passes
//!
//! A frame of a mounted root is two passes, kept apart:
//!
//! 1. [`UiTree::layout`]: when the tree's LAYOUT generation moved (a layout
//!    property, a child, a text run, a texture, an inherited text style) or
//!    the size changed, the inherited text styles are resolved down the tree
//!    and the solver runs from the root's viewport; then (and on a scale
//!    change alone) every element's box is SNAPPED to the device pixel grid
//!    by [`Units`]'s one rule, the renderer's (R16).
//! 2. [`UiTree::paint`]: when the PAINT generation or the layout moved, the
//!    root's scene is rebuilt from the snapped boxes; otherwise the scene
//!    built last time is appended again, unchanged.
//!
//! A property that changes paint only (a colour, a radius, an opacity) moves
//! the paint generation alone, so it repaints without a layout pass.
//!
//! # Element kinds
//!
//! | Kind | Layout |
//! |---|---|
//! | row, column | flexbox along x or y: gap, padding, grow, shrink, align, justify, wrap |
//! | stack | every child in ONE grid cell, each placed by its own anchor |
//! | text | a leaf measured by the text system, wrapped to the width it is given |
//! | image | a leaf whose natural size is its texture's, in dp; painted by its fit |
//!
//! No margins (R5): space between siblings is the parent's gap, space inside
//! an element its padding.

use std::rc::Rc;

use taffy::{
    AlignContent, AlignItems, AvailableSpace, Dimension, Display, FlexDirection, FlexWrap,
    LengthPercentage, LengthPercentageAuto, TaffyTree,
};

/// An element's id in its [`UiTree`].
pub use taffy::NodeId;

use crate::color::{Color, Fill};
use crate::geometry::{Corners, Edges, Point, Rect, Size};
use crate::scene::{Border, Image, Rectangle, Scene, SceneBuilder, Shadow};
use crate::text::{FontFamily, TextLayout, TextStyle, TextSystem};
use crate::units::Units;

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
            ElementKind::Viewport => "viewport",
        }
    }

    fn is_parent(self) -> bool {
        matches!(
            self,
            ElementKind::Row | ElementKind::Column | ElementKind::Stack | ElementKind::Viewport
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

/// The accessibility role an element announces (R10). Carried, read by
/// nothing yet: a later part publishes it through the platform.
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

/// What an element paints of its own, under its children.
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
    text: String,
    resolved: Rc<ResolvedText>,
    shaped: Vec<Shaped>,
    /// Paragraphs shaped since the text or its style last changed: what the
    /// reuse rule below saves is pinned by it.
    shapings: u32,
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
        let answers = |shaped: &Shaped| {
            let width = shaped.layout.size().width;
            match (constraint, shaped.constraint) {
                (None, None) => true,
                (None, Some(_)) => false,
                // The same constraint again (a min-content paragraph is
                // wider than its constraint of zero), or one in its range.
                (Some(wanted), Some(kept)) if (kept - wanted).abs() <= WIDTH_EPSILON => true,
                (Some(wanted), kept) => {
                    width <= wanted + WIDTH_EPSILON
                        && kept.is_none_or(|kept| kept + WIDTH_EPSILON >= wanted)
                }
            }
        };
        if let Some(found) = self.shaped.iter().position(answers) {
            let shaped = self.shaped.remove(found);
            let layout = shaped.layout.clone();
            self.shaped.insert(0, shaped);
            return layout;
        }
        let layout = text.layout(&self.text, &self.resolved.style, constraint);
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
}

#[derive(Clone, Debug)]
struct ImageContent {
    image: Option<Image>,
    fit: Fit,
}

#[derive(Clone, Debug)]
enum Content {
    Parent,
    Text(TextContent),
    Image(ImageContent),
}

/// What a root remembers of its last passes.
#[derive(Clone, Debug)]
struct RootState {
    viewport: NodeId,
    /// The style generation the inherited text styles were last resolved at.
    resolved: u64,
    /// The size and layout generation the solver last ran at.
    solved: Option<(Size, u64)>,
    /// The scale and solve count the boxes were last snapped at.
    snapped: Option<(Units, u64)>,
    solves: u64,
    /// The scene painted last, with the paint generation and snap count it
    /// was painted at.
    scene: Option<(u64, u64, Scene)>,
}

/// One element, as the tree keeps it beside the solver's style.
#[derive(Clone, Debug)]
pub struct NodeData {
    kind: ElementKind,
    paint: Paint,
    text: TextOverrides,
    /// The text style it inherits from its parent, then overrides: kept for
    /// every element so a text child added later resolves from it.
    resolved: Rc<ResolvedText>,
    anchor: Option<Anchor>,
    role: Role,
    label: Option<String>,
    content: Content,
    /// Whether a vilan handle to it is alive: an element is freed when it has
    /// neither a handle nor a parent.
    handle: bool,
    root: Option<RootState>,
    /// The snapped box, in dp from the root's top-left corner.
    bounds: Rect,
    /// Where the content box (inside the padding) starts in the box.
    content_offset: Point,
    /// The UNsnapped content size: text is wrapped at the width the solver
    /// gave it, never at a snapped width that could break a line the solver
    /// did not.
    content_size: Size,
}

impl NodeData {
    fn new(kind: ElementKind) -> NodeData {
        let content = match kind {
            ElementKind::Text => Content::Text(TextContent::default()),
            ElementKind::Image => Content::Image(ImageContent {
                image: None,
                fit: Fit::Contain,
            }),
            _ => Content::Parent,
        };
        NodeData {
            kind,
            paint: Paint::default(),
            text: TextOverrides::default(),
            resolved: Rc::new(ResolvedText::default()),
            anchor: None,
            role: Role::None,
            label: None,
            content,
            handle: true,
            root: None,
            bounds: Rect::default(),
            content_offset: Point::default(),
            content_size: Size::default(),
        }
    }
}

/// Why a tree operation was refused. Each is a program's mistake, raised as
/// its failure by the host.
#[derive(Clone, Debug, PartialEq)]
pub enum UiError {
    /// `child` on a text or an image.
    NotAParent(ElementKind),
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
}

impl std::fmt::Display for UiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UiError::NotAParent(kind) => write!(
                formatter,
                "a `{}` element has no children: only a row, a column or a stack takes `child`",
                kind.name()
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

/// Every element of a program: see the module documentation.
pub struct UiTree {
    taffy: TaffyTree<NodeData>,
    /// Moves on every change that can move a box.
    layout_generation: u64,
    /// Moves on every change at all.
    paint_generation: u64,
    /// Moves when an inherited text field, or the tree's shape, changes:
    /// a root resolves its text styles again when it moved.
    style_generation: u64,
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
    }
    style
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
            style_generation: 1,
        }
    }

    /// Elements alive in the tree, viewports included.
    pub fn element_count(&self) -> usize {
        self.taffy.total_node_count()
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

    fn paint_changed(&mut self) {
        self.paint_generation += 1;
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
        self.layout_changed();
    }

    // ------------------------------------------------------------ the tree ---

    /// Appends `child` to `parent`'s children.
    pub fn add_child(&mut self, parent: NodeId, child: NodeId) -> Result<(), UiError> {
        let kind = self.kind(parent);
        if !kind.is_parent() || kind == ElementKind::Viewport {
            return Err(UiError::NotAParent(kind));
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
        self.taffy
            .add_child(parent, child)
            .expect("a child appended");
        self.style_generation += 1;
        self.layout_changed();
        Ok(())
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
            for child in self.children(node) {
                if !self.data(child).handle {
                    pending.push(child);
                }
            }
            // Detaches the children that stay (they have handles).
            let _ = self.taffy.remove(node);
        }
        self.style_generation += 1;
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
    }

    pub fn role(&self, node: NodeId) -> Role {
        self.data(node).role
    }

    pub fn set_label(&mut self, node: NodeId, label: &str) {
        self.data_mut(node).label = Some(label.to_string());
    }

    pub fn label(&self, node: NodeId) -> Option<&str> {
        self.data(node).label.as_deref()
    }

    pub fn set_background(&mut self, node: NodeId, color: Color) {
        self.data_mut(node).paint.background = Some(color);
        self.paint_changed();
    }

    pub fn set_border(&mut self, node: NodeId, widths: Edges, color: Color) {
        let paint = &mut self.data_mut(node).paint;
        paint.border_widths = Edges {
            top: widths.top.max(0.0),
            right: widths.right.max(0.0),
            bottom: widths.bottom.max(0.0),
            left: widths.left.max(0.0),
        };
        paint.border_color = color;
        self.paint_changed();
    }

    pub fn set_corners(&mut self, node: NodeId, corners: Corners) {
        self.data_mut(node).paint.corners = corners;
        self.paint_changed();
    }

    pub fn set_shadow(&mut self, node: NodeId, shadow: DropShadow) {
        self.data_mut(node).paint.shadow = Some(shadow);
        self.paint_changed();
    }

    pub fn set_opacity(&mut self, node: NodeId, opacity: f32) -> Result<(), UiError> {
        let opacity = finite(opacity, "an opacity")?.clamp(0.0, 1.0);
        self.data_mut(node).paint.opacity = opacity;
        self.paint_changed();
        Ok(())
    }

    pub fn paint_of(&self, node: NodeId) -> &Paint {
        &self.data(node).paint
    }

    /// Sets some of the element's text style; the rest stays inherited. A
    /// colour alone changes paint only; every other field reshapes.
    pub fn set_text_style(&mut self, node: NodeId, change: impl FnOnce(&mut TextOverrides)) {
        let before = self.data(node).text.clone();
        change(&mut self.data_mut(node).text);
        let after = self.data(node).text.clone();
        if before == after {
            return;
        }
        self.style_generation += 1;
        let colour_only = TextOverrides {
            color: before.color,
            ..after.clone()
        } == before;
        if colour_only {
            self.paint_changed();
        } else {
            self.layout_changed();
        }
    }

    pub fn text_overrides(&self, node: NodeId) -> &TextOverrides {
        &self.data(node).text
    }

    /// Appends a run of text to a text element.
    pub fn append_text(&mut self, node: NodeId, run: &str) -> Result<(), UiError> {
        let kind = self.kind(node);
        let Content::Text(content) = &mut self.data_mut(node).content else {
            return Err(UiError::WrongKind {
                property: "text",
                kind,
            });
        };
        content.text.push_str(run);
        content.shaped.clear();
        content.shapings = 0;
        self.taffy.mark_dirty(node).expect("an element");
        self.layout_changed();
        Ok(())
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
        self.paint_changed();
        Ok(())
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
            resolved: 0,
            solved: None,
            snapped: None,
            solves: 0,
            scene: None,
        };
        self.data_mut(root).root = Some(state.clone());
        self.style_generation += 1;
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
        // Before deciding whether to solve: a colour alone resolves without
        // a solve, and a reshaping style has moved the layout generation.
        if state.resolved != self.style_generation {
            self.resolve_text_styles(root);
            state.resolved = self.style_generation;
        }
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
                self.taffy
                    .set_style(state.viewport, style)
                    .expect("the viewport's size");
            }
            self.taffy
                .compute_layout_with_measure(
                    state.viewport,
                    taffy::Size {
                        width: AvailableSpace::Definite(width),
                        height: AvailableSpace::Definite(height),
                    },
                    |inputs, _node, data, style| measure(inputs, data, style, text),
                )
                .expect("the layout");
            state.solved = Some((size, self.layout_generation));
            state.solves += 1;
        }
        if solve || state.snapped.map(|(snapped, _)| snapped) != Some(units) {
            self.snap(root, units);
            state.snapped = Some((units, state.solves));
        }
        self.data_mut(root).root = Some(state);
        Ok(solve)
    }

    /// Resolves the inherited text style of every element under `root`, top
    /// down from the default, and marks a text element whose shaping style
    /// changed for the solver.
    fn resolve_text_styles(&mut self, root: NodeId) {
        let mut pending = vec![(root, Rc::new(ResolvedText::default()))];
        while let Some((node, inherited)) = pending.pop() {
            let data = self.data(node);
            let resolved = if data.text.is_empty() {
                inherited
            } else {
                Rc::new(inherited.with(&data.text))
            };
            let data = self.data_mut(node);
            data.resolved = resolved.clone();
            if let Content::Text(content) = &mut data.content {
                let reshape = content.resolved.style != resolved.style;
                content.resolved = resolved.clone();
                if reshape {
                    content.shaped.clear();
                    content.shapings = 0;
                    self.taffy.mark_dirty(node).expect("an element");
                }
            }
            for child in self.children(node) {
                pending.push((child, resolved.clone()));
            }
        }
    }

    /// Snaps every box under `root` to the device grid: each edge on its own,
    /// by [`Units`]' rule (the renderer's), so boxes that share an edge in dp
    /// share it on the device and a hairline stays one pixel (R16).
    fn snap(&mut self, root: NodeId, units: Units) {
        let scale = units.content_scale();
        let mut pending = vec![(root, Point::new(0.0, 0.0))];
        while let Some((node, parent_origin)) = pending.pop() {
            let layout = *self.taffy.layout(node).expect("a laid-out element");
            let origin = Point::new(
                parent_origin.x + layout.location.x,
                parent_origin.y + layout.location.y,
            );
            let (x, width) = units.snap_span(origin.x, layout.size.width);
            let (y, height) = units.snap_span(origin.y, layout.size.height);
            let inset_left = layout.padding.left + layout.border.left;
            let inset_top = layout.padding.top + layout.border.top;
            let data = self.data_mut(node);
            data.bounds = Rect::new(x / scale, y / scale, width / scale, height / scale);
            data.content_offset = Point::new(inset_left, inset_top);
            data.content_size = Size::new(
                (layout.size.width - inset_left - layout.padding.right - layout.border.right)
                    .max(0.0),
                (layout.size.height - inset_top - layout.padding.bottom - layout.border.bottom)
                    .max(0.0),
            );
            for child in self.children(node) {
                pending.push((child, origin));
            }
        }
    }

    /// The element's snapped box from its last layout, in dp from its root's
    /// top-left corner.
    pub fn bounds(&self, node: NodeId) -> Rect {
        self.data(node).bounds
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
                pending.push((child, depth + 1));
            }
        }
        entries
    }

    // --------------------------------------------------------------- paint ---

    /// Appends `root`'s scene to `builder`: rebuilt when anything was
    /// painted or laid out differently since the last paint, the same
    /// shared scene otherwise. Lay the root out first.
    pub fn paint(
        &mut self,
        root: NodeId,
        builder: &mut SceneBuilder,
        text: &mut TextSystem,
    ) -> Result<(), UiError> {
        let mut state = self.root_state(root)?;
        let snaps = state.snapped.map_or(0, |(_, solves)| solves);
        let key = (self.paint_generation, snaps);
        if let Some((generation, painted_at, scene)) = &state.scene
            && (*generation, *painted_at) == key
        {
            builder.scene(scene);
            return Ok(());
        }
        let mut own = SceneBuilder::new();
        self.paint_subtree(root, &mut own, text);
        let scene = own.finish().expect("a balanced element scene");
        builder.scene(&scene);
        state.scene = Some((key.0, key.1, scene));
        self.data_mut(root).root = Some(state);
        Ok(())
    }

    fn paint_subtree(&mut self, root: NodeId, builder: &mut SceneBuilder, text: &mut TextSystem) {
        enum Visit {
            Enter(NodeId),
            Exit,
        }
        let mut pending = vec![Visit::Enter(root)];
        while let Some(visit) = pending.pop() {
            let node = match visit {
                Visit::Exit => {
                    builder.pop();
                    continue;
                }
                Visit::Enter(node) => node,
            };
            let layered = self.data(node).paint.opacity < 1.0;
            if layered {
                builder.push_layer(self.data(node).paint.opacity);
                pending.push(Visit::Exit);
            }
            self.paint_element(node, builder, text);
            for child in self.children(node).into_iter().rev() {
                pending.push(Visit::Enter(child));
            }
        }
    }

    /// One element's own primitives: shadow, background and border, then its
    /// content (an image, a paragraph).
    fn paint_element(&mut self, node: NodeId, builder: &mut SceneBuilder, text: &mut TextSystem) {
        let data = self.data(node);
        let bounds = data.bounds;
        let paint = data.paint.clone();
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
        let bordered = paint.border_widths != Edges::default();
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
                if !content.text.is_empty() {
                    let layout = content.shaped_for(Some(content_size.width), text);
                    builder.text(&layout, placed.origin, color);
                }
            }
            Content::Image(content) => {
                if let Some(image) = &content.image {
                    paint_image(builder, image, content.fit, placed, paint.corners);
                }
            }
        }
    }
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
/// known), nothing for an empty parent.
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
                    if content.text.is_empty() {
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
                    let layout = content.shaped_for(constraint, text);
                    first_baseline = Some(layout.first_baseline());
                    let shaped = layout.size();
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
