//! The vilan program's side of `std::ui` (native-ui.md P2 and P3): the
//! `__ui_*` externs `vilan/std/src/ui.vl` binds, over the program's one
//! [`UiTree`], and the dispatch of a mounted window's events into it.
//!
//! An element crosses as a [`Node`]: a host value holding the element's id,
//! shared by every copy the program makes, and RELEASED when the last copy is
//! gone — the tree frees an element once it has neither a handle nor a parent
//! ([`UiTree::release`]). Every attribute crosses once, as numbers (or a
//! string, or a texture), when the program sets it, and again each time a
//! signal it is bound to writes; a frame crosses only the root's handle.
//!
//! # Changes ask for frames
//!
//! A change to an element of a MOUNTED tree ([`mount`]) asks its window for a
//! frame, once however many changes come before it is drawn: so a signal
//! bound to a property shows its new value with no call of the program's.
//! The frame lays out and paints only what changed (`ui/mod.rs`).
//!
//! # Events
//!
//! A mounted window's events are routed into its tree ([`dispatch`]) before
//! the window's own event callback (if the program set one) sees them. The
//! handlers a route names are called with nothing of the tree borrowed, in
//! order; [`stop`] from inside one ends its event's bubbling. While a handler
//! runs, `std::gfx`'s current event is the window event being routed, which
//! is how a handler reads its position, key or scroll.
//!
//! # The codes
//!
//! `ui.vl`'s, kept in step with it by hand (each table is in both files):
//!
//! | `__ui_set` property | value |
//! |---|---|
//! | 0 grow, 1 shrink, 2 gap | a number (dp for the gap) |
//! | 3 align | 0 start, 1 center, 2 end, 3 stretch, 4 baseline |
//! | 4 justify | 0 start, 1 center, 2 end, 3 space between, 4 space around, 5 space evenly |
//! | 5 wrap | 0 or 1 |
//! | 6 anchor | 0 top left, 1 top, 2 top right, 3 left, 4 centre, 5 right, 6 bottom left, 7 bottom, 8 bottom right, 9 fill |
//! | 7 role | the index of `ui.vl`'s `Role` variant |
//! | 13 fit | 0 contain, 1 cover, 2 fill, 3 none |
//! | 14 padding top, 15 right, 16 bottom, 17 left | dp |
//! | 18 clip | 0 or 1 |
//! | 19 tab index | a whole number; `__ui_set_focusable` takes it away |
//! | 20 scroll axis | 0 vertical, 1 horizontal, 2 both |
//! | 21 checked | -1 none, 0 unchecked, 1 checked (published, P5) |
//! | 22 disabled state | 0 or 1 (published, P5) |
//!
//! `__ui_set_length` properties: 0 width, 1 height, 2 min width, 3 max width,
//! 4 min height, 5 max height; units 0 auto, 1 dp, 2 fraction of the parent.
//! Element kinds: 0 row, 1 column, 2 stack, 3 text, 4 image, 5 scroll, 6 a
//! marker (a region's place among its parent's children, never laid out).
//!
//! Style fields go to a LAYER (`ui/style.rs`): `__ui_style_group` starts a
//! group (-1 a new one, or the number of one to write again),
//! `__ui_style_layer` adds a layer to it (unconditional, or under a condition:
//! bits 1 hovered, 2 pressed, 4 focused, and a window width at least / below,
//! negative for none), the `__ui_style_*` setters write it, and
//! `__ui_style_end` resolves the element. `__ui_style_number`: 0 opacity, 1
//! text size, 2 line height, 3 weight, 4 italic (0 or 1). `__ui_style_color`:
//! 0 background, 1 text colour.
//!
//! Event kinds for `__ui_on`: 0 pointer enter, 1 pointer leave, 2 pointer
//! move, 3 press, 4 release, 5 click, 6 scroll, 7 key, 8 key release, 9 text,
//! 10 focus, 11 blur, 12 resize (never routed: told after a layout, see
//! [`layout`]; `__ui_resized` reads the size while the handler runs).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use taffy::NodeId;
use vilan_rt::{Js, Json, Str, panic_with, str_new};

use super::input::{self as raw_input, RawEvent};
use super::{STATE, TEXT_SCALE, color, with_frame, with_text, with_window};
use crate::geometry::{Corners, Edges, Point, Size};
use crate::scene::Image;
use crate::text::FontFamily;
use crate::ui::{
    AccessWindow, Align, Anchor, Condition, DropShadow, ElementKind, EventKind, Fit, Handler,
    IndexRule, Input, Justify, Length, OutlineEntry, PassStats, Role, Route, ScrollAxis,
    SizeProperty, StyleFields, UiError, UiTree, access_listing,
};
use crate::units::Units;

thread_local! {
    /// The program's elements: one tree per program, on the thread that
    /// draws.
    static TREE: RefCell<UiTree> = RefCell::new(UiTree::new());
    /// The outline `__ui_outline` took last, read by its number and text
    /// externs.
    static OUTLINE: RefCell<Vec<OutlineEntry>> = const { RefCell::new(Vec::new()) };
    /// Each mounted root, and the window it is mounted in.
    static MOUNTS: RefCell<HashMap<NodeId, u32>> = RefCell::new(HashMap::new());
    /// A handler called `stop` for the event being delivered.
    static STOPPED: Cell<bool> = const { Cell::new(false) };
    /// The last frame a mounted tree drew: its layout and paint times and
    /// what they did.
    static LAST_FRAME: Cell<FrameRecord> = const { Cell::new(FrameRecord::NONE) };
    /// The size a resize handler is being told, while it runs.
    static RESIZED: Cell<Size> = const { Cell::new(Size::new(0.0, 0.0)) };
}

/// How many times one [`layout`] lays its tree out: the pass, and once more
/// after each round of handlers (resize, offset) that changed the tree, up to
/// this many in all. A handler that keeps changing what it measures (a size
/// that depends on itself) stops here: the frame draws the last pass, and the
/// change it made asks for the next frame.
const LAYOUT_ROUNDS: usize = 8;

/// What the last frame of a mounted tree took and did.
#[derive(Clone, Copy, Debug, PartialEq)]
struct FrameRecord {
    layout_us: f64,
    paint_us: f64,
    stats: PassStats,
    /// The layout passes the last `__ui_layout` ran, 1 to [`LAYOUT_ROUNDS`].
    passes: usize,
}

impl FrameRecord {
    const NONE: FrameRecord = FrameRecord {
        layout_us: 0.0,
        paint_us: 0.0,
        stats: PassStats {
            solved: false,
            measured: 0,
            moved: 0,
            rebuilt: 0,
        },
        passes: 0,
    };
}

/// An element, as the program holds it: see the module documentation. A copy
/// is the same element, and two are `==` only when they are one.
#[derive(Clone, Debug)]
pub struct Node {
    handle: Rc<NodeHandle>,
}

#[derive(Debug)]
struct NodeHandle {
    id: NodeId,
}

impl Drop for NodeHandle {
    fn drop(&mut self) {
        // At thread exit the tree may already be gone, and with it every
        // element: nothing is left to release.
        let _ = TREE.try_with(|tree| {
            if let Ok(mut tree) = tree.try_borrow_mut() {
                tree.release(self.id);
            }
        });
    }
}

impl Node {
    pub(super) fn id(&self) -> NodeId {
        self.handle.id
    }
}

pub(super) fn with_tree<R>(body: impl FnOnce(&mut UiTree) -> R) -> R {
    TREE.with(|tree| body(&mut tree.borrow_mut()))
}

/// A change to `node`: when it moved a generation of the tree, the window its
/// tree is mounted in is asked for a frame.
fn change<R>(node: &Node, body: impl FnOnce(&mut UiTree, NodeId) -> R) -> R {
    let id = node.id();
    let (result, root) = with_tree(|tree| {
        let before = tree.generations();
        let result = body(tree, id);
        let root = (tree.generations() != before && tree.is_alive(id)).then(|| tree.root_of(id));
        (result, root)
    });
    if let Some(root) = root {
        request_frame(root);
    }
    result
}

fn request_frame(root: NodeId) {
    if let Some(window) = MOUNTS.with(|mounts| mounts.borrow().get(&root).copied()) {
        super::request_redraw(window);
    }
}

/// A refused tree operation is the program's failure, with the reason.
fn or_fail<T>(result: Result<T, UiError>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic_with(&format!("`std::ui`: {error}")),
    }
}

fn code<T: Copy>(table: &[T], code: f64, what: &str) -> T {
    let index = code as usize;
    if code < 0.0 || code.fract() != 0.0 || index >= table.len() {
        panic_with(&format!("{code} is not a `std::ui` {what} code"));
    }
    table[index]
}

/// `__ui_node(kind)` — a new element of `kind`, with no parent.
pub fn node(kind: i32) -> Node {
    let kind = code(
        &[
            ElementKind::Row,
            ElementKind::Column,
            ElementKind::Stack,
            ElementKind::Text,
            ElementKind::Image,
            ElementKind::Scroll,
            ElementKind::Marker,
        ],
        f64::from(kind),
        "element kind",
    );
    Node {
        handle: Rc::new(NodeHandle {
            id: with_tree(|tree| tree.create(kind)),
        }),
    }
}

/// `__ui_set(node, property, value)` — a property that is one number.
pub fn set(node: &Node, property: i32, value: f64) {
    let number = value as f32;
    change(node, |tree, id| match property {
        0 => or_fail(tree.set_grow(id, number)),
        1 => or_fail(tree.set_shrink(id, number)),
        2 => or_fail(tree.set_gap(id, number)),
        3 => {
            let align = code(
                &[
                    Align::Start,
                    Align::Center,
                    Align::End,
                    Align::Stretch,
                    Align::Baseline,
                ],
                value,
                "align",
            );
            or_fail(tree.set_align(id, align))
        }
        4 => {
            let justify = code(
                &[
                    Justify::Start,
                    Justify::Center,
                    Justify::End,
                    Justify::SpaceBetween,
                    Justify::SpaceAround,
                    Justify::SpaceEvenly,
                ],
                value,
                "justify",
            );
            or_fail(tree.set_justify(id, justify))
        }
        5 => or_fail(tree.set_wrap(id, value != 0.0)),
        6 => {
            let anchor = code(
                &[
                    Anchor::TopLeft,
                    Anchor::Top,
                    Anchor::TopRight,
                    Anchor::Left,
                    Anchor::Center,
                    Anchor::Right,
                    Anchor::BottomLeft,
                    Anchor::Bottom,
                    Anchor::BottomRight,
                    Anchor::Fill,
                ],
                value,
                "anchor",
            );
            tree.set_anchor(id, anchor)
        }
        7 => {
            let role = code(
                &[
                    Role::Group,
                    Role::Heading,
                    Role::Paragraph,
                    Role::Button,
                    Role::Link,
                    Role::TextField,
                    Role::Checkbox,
                    Role::List,
                    Role::ListItem,
                    Role::Image,
                ],
                value,
                "role",
            );
            tree.set_role(id, role)
        }
        13 => {
            let fit = code(
                &[Fit::Contain, Fit::Cover, Fit::Fill, Fit::None],
                value,
                "fit",
            );
            or_fail(tree.set_fit(id, fit))
        }
        14..=17 => or_fail(tree.set_padding_side(id, (property - 14) as usize, number)),
        18 => tree.set_clip(id, value != 0.0),
        19 => {
            if !value.is_finite() {
                panic_with("a tab index must be a whole number");
            }
            tree.set_tab_index(id, Some(value as i32))
        }
        20 => {
            let axis = code(
                &[
                    ScrollAxis::Vertical,
                    ScrollAxis::Horizontal,
                    ScrollAxis::Both,
                ],
                value,
                "scroll axis",
            );
            or_fail(tree.set_scroll_axis(id, axis))
        }
        21 => tree.set_checked(id, (value >= 0.0).then_some(value != 0.0)),
        22 => tree.set_disabled(id, value != 0.0),
        _ => panic_with(&format!("{property} is not a `std::ui` property code")),
    });
}

/// `__ui_set_length(node, property, unit, value)` — a size property.
pub fn set_length(node: &Node, property: i32, unit: i32, value: f64) {
    let property = code(
        &[
            SizeProperty::Width,
            SizeProperty::Height,
            SizeProperty::MinWidth,
            SizeProperty::MaxWidth,
            SizeProperty::MinHeight,
            SizeProperty::MaxHeight,
        ],
        f64::from(property),
        "size property",
    );
    let length = match unit {
        0 => Length::Auto,
        1 => Length::Dp(value as f32),
        2 => Length::Fraction(value as f32),
        _ => panic_with(&format!("{unit} is not a `std::ui` length unit code")),
    };
    change(node, |tree, id| {
        or_fail(tree.set_size(id, property, length))
    });
}

// ----------------------------------------------------------------- style ---

/// `__ui_style_group(node, group)` — starts a style group: a new one (-1),
/// or `group` emptied to be written again. Answers the group.
pub fn style_group(node: &Node, group: i32) -> i32 {
    let group = u32::try_from(group).ok();
    with_tree(|tree| tree.style_group(node.id(), group)) as i32
}

/// `__ui_style_layer(node, group, conditional, bits, at_least, below)` — a
/// new layer of `group`: unconditional, or under the condition the bits and
/// the two widths (negative: none) name. Answers its id.
pub fn style_layer(
    node: &Node,
    group: i32,
    conditional: bool,
    bits: i32,
    at_least: f64,
    below: f64,
) -> i32 {
    let condition = conditional.then_some(Condition {
        hovered: bits & 1 != 0,
        pressed: bits & 2 != 0,
        focused: bits & 4 != 0,
        window_at_least: (at_least >= 0.0).then_some(at_least as f32),
        window_below: (below >= 0.0).then_some(below as f32),
    });
    let group = u32::try_from(group).unwrap_or_else(|_| panic_with("a style group is positive"));
    or_fail(with_tree(|tree| {
        tree.style_layer(node.id(), group, condition)
    })) as i32
}

fn layer_fields(node: &Node, layer: i32, change: impl FnOnce(&mut StyleFields)) {
    let layer = u32::try_from(layer).unwrap_or_else(|_| panic_with("a style layer is positive"));
    with_tree(|tree| tree.set_style_fields(node.id(), layer, change));
}

/// `__ui_style_end(node)` — resolves the element's style from its layers.
pub fn style_end(node: &Node) {
    change(node, |tree, id| tree.restyle(id));
}

/// `__ui_style_number(node, layer, property, value)` — 0 opacity, 1 text
/// size, 2 line height, 3 weight, 4 italic.
pub fn style_number(node: &Node, layer: i32, property: i32, value: f64) {
    let number = value as f32;
    if !number.is_finite() {
        panic_with("a style number must be finite");
    }
    layer_fields(node, layer, |fields| match property {
        0 => fields.opacity = Some(number.clamp(0.0, 1.0)),
        1 => fields.text.size = Some(number.max(0.0)),
        2 => fields.text.line_height = Some(number.max(0.0)),
        3 => fields.text.weight = Some(number.clamp(1.0, 1000.0)),
        4 => fields.text.italic = Some(value != 0.0),
        _ => panic_with(&format!("{property} is not a `std::ui` style number code")),
    });
}

/// `__ui_style_color(node, layer, property, colour)` — the background (0) or
/// the text colour (1, inherited).
#[allow(
    clippy::too_many_arguments,
    reason = "an extern's arguments are the vilan call's, one scalar each"
)]
pub fn style_color(
    node: &Node,
    layer: i32,
    property: i32,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) {
    let value = color(red, green, blue, alpha);
    layer_fields(node, layer, |fields| match property {
        0 => fields.background = Some(value),
        1 => fields.text.color = Some(value),
        _ => panic_with(&format!("{property} is not a `std::ui` colour code")),
    });
}

/// `__ui_style_corners(node, layer, top_left, top_right, bottom_right,
/// bottom_left)` — the corner radii, in dp.
pub fn style_corners(
    node: &Node,
    layer: i32,
    top_left: f64,
    top_right: f64,
    bottom_right: f64,
    bottom_left: f64,
) {
    let corners = Corners {
        top_left: (top_left as f32).max(0.0),
        top_right: (top_right as f32).max(0.0),
        bottom_right: (bottom_right as f32).max(0.0),
        bottom_left: (bottom_left as f32).max(0.0),
    };
    layer_fields(node, layer, |fields| fields.corners = Some(corners));
}

/// `__ui_style_border(node, layer, widths, colour)` — widths clockwise from
/// the top, in dp, drawn inside the box.
#[allow(
    clippy::too_many_arguments,
    reason = "an extern's arguments are the vilan call's, one scalar each"
)]
pub fn style_border(
    node: &Node,
    layer: i32,
    top: f64,
    right: f64,
    bottom: f64,
    left: f64,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) {
    let widths = Edges {
        top: (top as f32).max(0.0),
        right: (right as f32).max(0.0),
        bottom: (bottom as f32).max(0.0),
        left: (left as f32).max(0.0),
    };
    let value = color(red, green, blue, alpha);
    layer_fields(node, layer, |fields| fields.border = Some((widths, value)));
}

/// `__ui_style_shadow(node, layer, x, y, blur, spread, colour)` — in dp.
#[allow(
    clippy::too_many_arguments,
    reason = "an extern's arguments are the vilan call's, one scalar each"
)]
pub fn style_shadow(
    node: &Node,
    layer: i32,
    x: f64,
    y: f64,
    blur: f64,
    spread: f64,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) {
    let shadow = DropShadow {
        offset: Point::new(x as f32, y as f32),
        blur: blur as f32,
        spread: spread as f32,
        color: color(red, green, blue, alpha),
    };
    layer_fields(node, layer, |fields| fields.shadow = Some(shadow));
}

/// `__ui_style_font(node, layer, family, name)` — the inherited font family:
/// 0 sans-serif, 1 serif, 2 monospace, 3 the family `name`.
pub fn style_font(node: &Node, layer: i32, family: i32, name: &Str) {
    let family = match family {
        0 => FontFamily::SansSerif,
        1 => FontFamily::Serif,
        2 => FontFamily::Monospace,
        3 => FontFamily::Named(name.to_string()),
        _ => panic_with(&format!("{family} is not a `std::ui` font family code")),
    };
    layer_fields(node, layer, |fields| fields.text.family = Some(family));
}

// ------------------------------------------------------------ the rest ---

/// `__ui_append_text(node, run)` — a run of a text element's paragraph;
/// answers its index.
pub fn append_text(node: &Node, run: &Str) -> i32 {
    change(node, |tree, id| or_fail(tree.append_text(id, run))) as i32
}

/// `__ui_set_run(node, index, run)` — replaces a run (a bound one, written).
pub fn set_run(node: &Node, index: i32, run: &Str) {
    let index = usize::try_from(index).unwrap_or(usize::MAX);
    change(node, |tree, id| or_fail(tree.set_run(id, index, run)));
}

/// `__ui_set_label(node, label)` — the accessibility label (R10).
pub fn set_label(node: &Node, label: &Str) {
    with_tree(|tree| tree.set_label(node.id(), label));
}

// ------------------------------------------------------------ editing ---
//
// `std::ui::kit`'s text input (P4): the program keeps a text's caret and
// selection as UTF-16 indices (`str`'s own); the tree draws them and answers
// where a point or a step lands, from the text's layout (`text/editing.rs`).

fn index_of(value: i32) -> usize {
    usize::try_from(value).unwrap_or(0)
}

/// `__ui_text_edit(node, caret, anchor, active)` — a text's caret and the
/// other end of its selection, drawn while `active` (and then scrolled into
/// view by the next layout).
pub fn text_edit(node: &Node, caret: i32, anchor: i32, active: bool) {
    change(node, |tree, id| {
        or_fail(tree.text_edit(id, index_of(caret), index_of(anchor), active))
    });
}

/// `__ui_text_edit_look(node, caret colour, caret width, selection colour)`
/// — a NEGATIVE alpha leaves that colour to the text's (the caret's) or the
/// text's at a quarter of its alpha (the selection's).
#[allow(
    clippy::too_many_arguments,
    reason = "an extern's arguments are the vilan call's, one scalar each"
)]
pub fn text_edit_look(
    node: &Node,
    caret_red: f64,
    caret_green: f64,
    caret_blue: f64,
    caret_alpha: f64,
    caret_width: f64,
    selection_red: f64,
    selection_green: f64,
    selection_blue: f64,
    selection_alpha: f64,
) {
    let caret =
        (caret_alpha >= 0.0).then(|| color(caret_red, caret_green, caret_blue, caret_alpha));
    let selection = (selection_alpha >= 0.0).then(|| {
        color(
            selection_red,
            selection_green,
            selection_blue,
            selection_alpha,
        )
    });
    change(node, |tree, id| {
        or_fail(tree.text_edit_look(id, caret, caret_width as f32, selection))
    });
}

/// `__ui_text_index_at(node, x, y, rule)` — the UTF-16 index in an edited
/// text the point (dp from the window's top-left, as an event gives it)
/// lands on: 0 the nearest grapheme edge (a press), 1 the start of the
/// grapheme under it (a double click's), 2 a drag's (one grapheme past the
/// edge of what shows when the point is beyond it; `IndexRule`).
pub fn text_index_at(node: &Node, x: f64, y: f64, rule: i32) -> i32 {
    let point = Point::new(x as f32, y as f32);
    let rule = code(
        &[IndexRule::Nearest, IndexRule::Under, IndexRule::Drag],
        f64::from(rule),
        "text index rule",
    );
    let index = with_tree(|tree| {
        with_text(|text| or_fail(tree.text_index_at(node.id(), point, rule, text)))
    });
    i32::try_from(index).unwrap_or(i32::MAX)
}

/// `__ui_text_word(node, index, end)` — the start (or with `end`, the end)
/// of the word around UTF-16 `index` of an edited text: the run of word
/// graphemes, or of the others, that the grapheme there is in.
pub fn text_word(node: &Node, index: i32, end: bool) -> i32 {
    let found = with_tree(|tree| {
        with_text(|text| or_fail(tree.text_word(node.id(), index_of(index), end, text)))
    });
    i32::try_from(found).unwrap_or(i32::MAX)
}

/// `__ui_click_count()` — how many presses in a row the last press was (1
/// a single press, 2 a double click; 0 before any).
pub fn click_count() -> i32 {
    with_tree(|tree| i32::try_from(tree.click_count()).unwrap_or(i32::MAX))
}

/// `__ui_release_pointer(node)` — ends the press (and the capture) the
/// element or one inside it holds: no click follows it.
pub fn release_pointer(node: &Node) {
    change(node, |tree, id| {
        tree.release_pointer(id);
    });
}

/// `__ui_text_step(node, index, forward, word)` — the UTF-16 index of the
/// next (`forward`) or previous grapheme boundary of an edited text, or with
/// `word` the end of the next word or the start of the previous one.
pub fn text_step(node: &Node, index: i32, forward: bool, word: bool) -> i32 {
    let stepped = with_tree(|tree| {
        with_text(|text| or_fail(tree.text_step(node.id(), index_of(index), forward, word, text)))
    });
    i32::try_from(stepped).unwrap_or(i32::MAX)
}

/// `__ui_set_texture(node, texture)` — an image element's picture.
pub fn set_texture(node: &Node, image: &Image) {
    change(node, |tree, id| {
        or_fail(tree.set_texture(id, image.clone()))
    });
}

/// `__ui_add_child(parent, child)` — appends `child`.
pub fn add_child(parent: &Node, child: &Node) {
    let child = child.id();
    change(parent, |tree, parent| {
        or_fail(tree.add_child(parent, child))
    });
}

/// `__ui_insert_before(reference, child)` — `child` immediately before
/// `reference` in `reference`'s parent: moved there when it is already a child
/// of that parent (its state goes with it), placed there otherwise.
pub fn insert_before(reference: &Node, child: &Node) {
    let child = child.id();
    change(reference, |tree, reference| {
        or_fail(tree.insert_before(reference, child))
    });
}

/// `__ui_remove(node)` — `node` leaves its parent for good, and its subtree's
/// handlers are dropped (`UiTree::remove`; B738): AFTER the tree is released,
/// because a handler may hold the last handle of an element, and that
/// handle's drop releases the element in the tree.
pub fn remove(node: &Node) {
    let id = node.id();
    let (handlers, root) = with_tree(|tree| {
        let root = tree.is_alive(id).then(|| tree.root_of(id));
        (tree.remove(id), root)
    });
    drop(handlers);
    if let Some(root) = root
        && root != id
    {
        request_frame(root);
    }
}

/// `__ui_scroll_into_view(node)` — every `scroll` around the element moves as
/// little as it takes to show it (`UiTree::reveal`), as Tab does for the
/// element it focuses; a bound offset follows. An element whose box is not
/// current yet (built since the last layout) is revealed by the next layout
/// (`UiTree::reveal_when_laid_out`).
pub fn scroll_into_view(node: &Node) {
    reveal_element(node.id());
}

/// [`scroll_into_view`] by the element's id: what an assistive tool's scroll
/// into view does too (`host/access.rs`).
pub(super) fn reveal_element(id: NodeId) {
    let root = with_tree(|tree| {
        let moved = tree.reveal_when_laid_out(id);
        // A reveal left for the next layout asks for that layout.
        (moved || tree.reveal_pending()).then(|| tree.root_of(id))
    });
    if let Some(root) = root {
        request_frame(root);
    }
    notify_offsets();
}

/// `__ui_set_focusable(node, focusable)` — focusable in tree order (`true`,
/// tab index 0), or not at all.
pub fn set_focusable(node: &Node, focusable: bool) {
    with_tree(|tree| tree.set_tab_index(node.id(), focusable.then_some(0)));
}

/// `__ui_on(node, kind, handler)` — `handler` called for every event of
/// `kind` the element receives (the table above).
pub fn on(node: &Node, kind: i32, handler: Rc<dyn Fn()>) {
    let kind = code(
        &[
            EventKind::PointerEnter,
            EventKind::PointerLeave,
            EventKind::PointerMove,
            EventKind::Press,
            EventKind::Release,
            EventKind::Click,
            EventKind::Scroll,
            EventKind::Key,
            EventKind::KeyRelease,
            EventKind::Text,
            EventKind::Focus,
            EventKind::Blur,
            EventKind::Resize,
        ],
        f64::from(kind),
        "event kind",
    );
    with_tree(|tree| tree.add_handler(node.id(), kind, Handler::from_rc(handler)));
}

/// `__ui_stop()` — inside a handler: the event bubbles no further, and the
/// framework's own response to it does not happen.
pub fn stop() {
    STOPPED.with(|stopped| stopped.set(true));
}

/// `__ui_focus(node)` — gives the element the focus (whether or not Tab
/// reaches it).
pub fn focus(node: &Node) {
    focus_element(node.id());
}

/// [`focus`] by the element's id: what an assistive tool's focus does too
/// (`host/access.rs`).
pub(super) fn focus_element(id: NodeId) {
    let (deliveries, root) = with_tree(|tree| (tree.focus(Some(id)), tree.root_of(id)));
    for delivery in deliveries {
        delivery.handler.call();
    }
    request_frame(root);
}

/// `__ui_interaction(node)` — bits: 1 hovered, 2 pressed, 4 focused.
pub fn interaction(node: &Node) -> i32 {
    let state = with_tree(|tree| tree.interaction(node.id()));
    i32::from(state.hovered) | i32::from(state.pressed) << 1 | i32::from(state.focused) << 2
}

/// `__ui_set_offset(node, x, y)` — where a scroll's child is moved to.
pub fn set_offset(node: &Node, x: f64, y: f64) {
    change(node, |tree, id| {
        or_fail(tree.set_scroll_offset(id, Point::new(x as f32, y as f32)))
    });
    notify_offsets();
}

/// `__ui_offset(node, which)` — a scroll's offset, x (0) and y (1), and its
/// range, x (2) and y (3), in dp.
pub fn offset(node: &Node, which: i32) -> f64 {
    let (offset, range) =
        with_tree(|tree| (tree.scroll_offset(node.id()), tree.scroll_range(node.id())));
    let (Some(offset), Some(range)) = (offset, range) else {
        panic_with("only a `scroll` has an offset");
    };
    f64::from(match which {
        0 => offset.x,
        1 => offset.y,
        2 => range.width,
        3 => range.height,
        _ => panic_with(&format!("{which} is not a `std::ui` offset number")),
    })
}

/// `__ui_on_offset(node, handler)` — called when a scroll's offset moves
/// other than by `__ui_set_offset`'s value.
pub fn on_offset(node: &Node, handler: Rc<dyn Fn()>) {
    with_tree(|tree| or_fail(tree.on_scroll_offset(node.id(), Handler::from_rc(handler))));
}

fn notify_offsets() {
    for handler in with_tree(UiTree::take_offset_notices) {
        handler.call();
    }
}

fn units(scale_factor: f64) -> Units {
    Units::new(scale_factor as f32, TEXT_SCALE as f32).unwrap_or_else(|error| {
        panic_with(&format!("`std::ui`: {error}"));
    })
}

/// `__ui_layout(root, width, height, scale_factor)` — lays `root` out in a
/// `width` x `height` dp viewport, snapped to the device grid at
/// `scale_factor`. Solves only when something that moves a box changed.
///
/// After the pass, the programs of the scrolls whose offsets it clamped and
/// of the elements whose size it changed (`on_resize`) are told, with the
/// tree not borrowed; when what they did changed the layout, the tree is laid
/// out again, so a frame never shows a layout its own handlers had already
/// replaced (up to [`LAYOUT_ROUNDS`] passes). Recorded as the last frame's
/// layout: every pass's time, and what they did together.
pub fn layout(root: &Node, width: f64, height: f64, scale_factor: f64) {
    let id = root.id();
    let units = units(scale_factor);
    let size = Size::new(width as f32, height as f32);
    let started = Instant::now();
    let mut stats = PassStats::default();
    let mut passes = 0;
    for _ in 0..LAYOUT_ROUNDS {
        passes += 1;
        let pass = with_tree(|tree| {
            with_text(|text| {
                or_fail(tree.layout(id, size, units, text).map(|_| ()));
            });
            tree.last_pass()
        });
        stats.solved |= pass.solved;
        stats.measured += pass.measured;
        stats.moved += pass.moved;
        let laid_out = with_tree(|tree| tree.generations().0);
        notify_offsets();
        // Content that moved under a still pointer moves the hover now.
        let hover = with_tree(|tree| tree.refresh_hover(id));
        run_route(id, hover);
        for (handler, size) in with_tree(UiTree::take_resize_notices) {
            RESIZED.with(|resized| resized.set(size));
            handler.call();
        }
        if with_tree(|tree| tree.generations().0) == laid_out {
            break;
        }
    }
    let took = started.elapsed().as_secs_f64() * 1.0e6;
    LAST_FRAME.with(|last| {
        let mut record = last.get();
        record.layout_us = took;
        record.stats = stats;
        record.passes = passes;
        last.set(record);
    });
}

/// `__ui_resized(which)` — inside a resize handler: the size it is told, its
/// width (0) and height (1) in dp.
pub fn resized(which: i32) -> f64 {
    let size = RESIZED.with(Cell::get);
    f64::from(match which {
        0 => size.width,
        1 => size.height,
        _ => panic_with(&format!("{which} is not a `std::ui` size number")),
    })
}

/// `__ui_paint(frame, root)` — draws `root`, as last laid out, into the
/// frame being filled. Recorded as the last frame's paint.
pub fn paint(frame: u32, root: &Node) {
    let id = root.id();
    let started = Instant::now();
    with_frame(frame, |builder| {
        with_tree(|tree| with_text(|text| or_fail(tree.paint(id, builder, text))))
    });
    let took = started.elapsed().as_secs_f64() * 1.0e6;
    let rebuilt = with_tree(|tree| tree.last_pass().rebuilt);
    LAST_FRAME.with(|last| {
        let mut record = last.get();
        record.paint_us = took;
        record.stats.rebuilt = rebuilt;
        last.set(record);
    });
}

/// `__ui_mount(window, root)` — `root` becomes window `window`'s contents:
/// its frame callback lays the tree out at the window's size and scale and
/// paints it; its events are routed into the tree first; a change to the
/// tree asks for a frame.
pub fn mount(window: u32, root: &Node) {
    let id = root.id();
    if with_tree(|tree| tree.root_of(id)) != id {
        or_fail::<()>(Err(UiError::NotARoot));
    }
    let drawn = root.clone();
    let mounted = with_window(window, |slot| {
        slot.on_frame = Some(Rc::new(move || draw_mounted(&drawn)));
        slot.ui_root = Some(root.clone());
    });
    if mounted.is_some() {
        MOUNTS.with(|mounts| mounts.borrow_mut().insert(id, window));
        super::request_redraw(window);
    }
}

/// A mounted tree's frame: laid out at the frame's size and scale, and
/// painted.
fn draw_mounted(root: &Node) {
    let token = super::frame_token();
    layout(
        root,
        super::frame_number(1),
        super::frame_number(2),
        super::frame_number(3),
    );
    paint(token, root);
}

/// `__ui_frame_number(window, which)` — the last layout and paint (of any
/// tree: a mounted one's frame, or `draw`'s):
/// its layout time (0), its paint time (1) and window `window`'s draw time —
/// encoding, submitting and presenting — of the last frame it presented (2),
/// all in microseconds; whether the solver ran (3), the leaves it measured
/// (4), the boxes that moved (5), the element scenes rebuilt (6), the layout
/// passes it took (7).
pub fn frame_number(window: u32, which: i32) -> f64 {
    let record = LAST_FRAME.with(Cell::get);
    match which {
        0 => record.layout_us,
        1 => record.paint_us,
        2 => with_window(window, |slot| slot.last_draw_us).unwrap_or(0.0),
        3 => f64::from(u8::from(record.stats.solved)),
        4 => record.stats.measured as f64,
        5 => record.stats.moved as f64,
        6 => record.stats.rebuilt as f64,
        7 => record.passes as f64,
        _ => panic_with(&format!("{which} is not a `std::ui` frame number")),
    }
}

/// Routes one window event into `root`'s tree and calls the handlers it
/// names; asks the root's window for a frame when the tree changed.
pub(crate) fn dispatch(root: &Node, raw: &RawEvent) {
    let input = match raw.kind {
        raw_input::POINTER_MOVED => Input::PointerMoved(position(raw)),
        raw_input::POINTER_PRESSED if raw.numbers[2] == 0.0 => Input::Pressed {
            position: position(raw),
            time: Duration::from_secs_f64(raw.numbers[4].max(0.0) / 1000.0),
        },
        raw_input::POINTER_RELEASED if raw.numbers[2] == 0.0 => Input::Released(position(raw)),
        raw_input::POINTER_LEFT => Input::PointerLeft,
        raw_input::SCROLLED => Input::Scrolled {
            position: position(raw),
            delta: Point::new(raw.numbers[2] as f32, raw.numbers[3] as f32),
        },
        raw_input::KEY_PRESSED => Input::Key {
            name: raw.text.clone().unwrap_or_default(),
            shift: (raw.numbers[1] as i32) & 1 != 0,
        },
        raw_input::KEY_RELEASED => Input::KeyReleased,
        raw_input::TEXT_INPUT => Input::Text,
        _ => return,
    };
    let id = root.id();
    let (route, before) = with_tree(|tree| {
        let before = tree.generations();
        (tree.route(id, &input), before)
    });
    // `std::gfx`'s current event is the one being routed while its handlers
    // run: what `current_event()` answers them.
    let outer = STATE.with(|state| state.borrow_mut().current_event.replace(raw.clone()));
    run_route(id, route);
    STATE.with(|state| state.borrow_mut().current_event = outer);
    if with_tree(|tree| tree.generations()) != before {
        request_frame(id);
    }
}

fn position(raw: &RawEvent) -> Point {
    Point::new(raw.numbers[0] as f32, raw.numbers[1] as f32)
}

/// Calls a route's handlers, step by step; a step a handler stopped skips
/// the rest of its handlers and its default; a default that runs may owe
/// handlers of its own.
fn run_route(root: NodeId, route: Route) {
    for step in route.steps {
        STOPPED.with(|stopped| stopped.set(false));
        let mut stopped = false;
        for delivery in &step.deliveries {
            delivery.handler.call();
            if step.stoppable && STOPPED.with(Cell::get) {
                stopped = true;
                break;
            }
        }
        STOPPED.with(|stopped| stopped.set(false));
        if !stopped && let Some(action) = step.default {
            let more = with_tree(|tree| tree.perform(root, action));
            run_route(root, more);
        }
    }
    notify_offsets();
}

/// `__ui_deliver(root, kind, a, b, c, d, e, text)` — routes an event into
/// `root`'s tree as a mounted window would (`std::gfx`'s event table): for a
/// test, or a program that drives its tree itself.
#[allow(
    clippy::too_many_arguments,
    reason = "an extern's arguments are the vilan call's, one scalar each"
)]
pub fn deliver(root: &Node, kind: i32, a: f64, b: f64, c: f64, d: f64, e: f64, text: &Str) {
    let raw = RawEvent {
        kind,
        numbers: [a, b, c, d, e],
        text: Some(text.to_string()),
    };
    dispatch(root, &raw);
}

/// `__ui_node_number(node, which)` — the element's box from the last layout,
/// in dp: x (0), y (1), width (2), height (3); and down the part its children
/// are drawn in (its padding box): the padding above its content box (4), the
/// content box's height (5), the padding below it (6); and its height as the
/// solver gave it, before snapping (7): what a box below it is placed by.
pub fn node_number(node: &Node, which: i32) -> f64 {
    let (bounds, inner, solved) = with_tree(|tree| {
        (
            tree.bounds(node.id()),
            tree.inner_box(node.id()),
            tree.solved_size(node.id()),
        )
    });
    f64::from(match which {
        0 => bounds.left(),
        1 => bounds.top(),
        2 => bounds.size.width,
        3 => bounds.size.height,
        4 => inner.padding_top,
        5 => inner.content_height,
        6 => inner.padding_bottom,
        7 => solved.height,
        _ => panic_with(&format!("{which} is not a `std::ui` box number")),
    })
}

/// `__ui_outline(root)` — takes the outline of `root`'s tree (every element,
/// in paint order) for the two readers below; answers its length.
pub fn outline(root: &Node) -> i32 {
    let entries = with_tree(|tree| tree.outline(root.id()));
    let count = entries.len() as i32;
    OUTLINE.with(|outline| *outline.borrow_mut() = entries);
    count
}

fn with_entry<R>(index: i32, body: impl FnOnce(&OutlineEntry) -> R) -> R {
    OUTLINE.with(|outline| {
        let outline = outline.borrow();
        let entry = usize::try_from(index)
            .ok()
            .and_then(|index| outline.get(index))
            .unwrap_or_else(|| panic_with(&format!("{index} is not in the `std::ui` outline")));
        body(entry)
    })
}

/// `__ui_outline_number(index, which)` — an outline entry's depth (0), and
/// its box in dp: x (1), y (2), width (3), height (4).
pub fn outline_number(index: i32, which: i32) -> f64 {
    with_entry(index, |entry| match which {
        0 => entry.depth as f64,
        1 => f64::from(entry.bounds.left()),
        2 => f64::from(entry.bounds.top()),
        3 => f64::from(entry.bounds.size.width),
        4 => f64::from(entry.bounds.size.height),
        _ => panic_with(&format!("{which} is not a `std::ui` outline number")),
    })
}

/// `__ui_outline_text(index, which)` — an outline entry's kind (0) and its
/// label (1, "" when it has none).
pub fn outline_text(index: i32, which: i32) -> Str {
    with_entry(index, |entry| match which {
        0 => str_new(entry.kind.name()),
        1 => str_new(entry.label.as_deref().unwrap_or("")),
        _ => panic_with(&format!("{which} is not a `std::ui` outline text")),
    })
}

/// `__ui_set_items(node, first, count)` — a list holding only some of its
/// items: its first child is item `first` (from 0) of `count`, for
/// assistive technology (P5; `std::ui::kit::virtual_list` sets it).
pub fn set_items(node: &Node, first: f64, count: f64) {
    let whole = |value: f64| {
        if !(value >= 0.0 && value.is_finite()) {
            panic_with("a list's items are counted from 0");
        }
        value as usize
    };
    with_tree(|tree| tree.set_items_shown(node.id(), whole(first), whole(count)));
}

/// `__ui_accessibility(node)` — what an assistive tool is told of the tree
/// under `node` (`ui/access.rs`'s listing, without the window): one line a
/// published element.
pub fn accessibility(node: &Node) -> Str {
    let window = AccessWindow {
        title: "",
        scale: 1.0,
        width: 0.0,
        height: 0.0,
    };
    let tree = with_tree(|tree| tree.access_tree(Some(node.id()), window));
    str_new(&access_listing(&tree, false))
}

/// The elements alive in the program's tree: what a pin on releasing reads.
#[cfg(test)]
pub(super) fn element_count() -> usize {
    with_tree(|tree| tree.element_count())
}

/// Identity, as every host value's is.
impl PartialEq for Node {
    fn eq(&self, other: &Node) -> bool {
        Rc::ptr_eq(&self.handle, &other.handle)
    }
}

impl Js for Node {
    fn js(&self) -> String {
        panic_with(
            "printing an element is a host object's own inspection, which has no text form \
             (print its `bounds()` or its `outline()`)",
        )
    }
}

impl Json for Node {
    fn json(&self) -> String {
        panic_with("an element is a host object, which has no JSON form")
    }
}
