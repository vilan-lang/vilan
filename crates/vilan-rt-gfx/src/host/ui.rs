//! The vilan program's side of `std::ui` (native-ui.md P2): the `__ui_*`
//! externs `vilan/std/src/ui.vl` binds, over the program's one [`UiTree`].
//!
//! An element crosses as a [`Node`]: a host value holding the element's id,
//! shared by every copy the program makes, and RELEASED when the last copy is
//! gone — the tree frees an element once it has neither a handle nor a parent
//! ([`UiTree::release`]). Every attribute crosses once, as numbers (or a
//! string, or a texture), when the program sets it; a frame crosses only the
//! root's handle and the frame's token.
//!
//! The codes below are `ui.vl`'s, kept in step with it by hand (each table is
//! in both files):
//!
//! | `__ui_set` property | value |
//! |---|---|
//! | 0 grow, 1 shrink, 2 gap | a number (dp for the gap) |
//! | 3 align | 0 start, 1 center, 2 end, 3 stretch, 4 baseline |
//! | 4 justify | 0 start, 1 center, 2 end, 3 space between, 4 space around, 5 space evenly |
//! | 5 wrap | 0 or 1 |
//! | 6 anchor | 0 top left, 1 top, 2 top right, 3 left, 4 centre, 5 right, 6 bottom left, 7 bottom, 8 bottom right, 9 fill |
//! | 7 role | the index of `ui.vl`'s `Role` variant |
//! | 8 opacity | 0 to 1 |
//! | 9 text size (dp), 10 line height, 11 weight, 12 italic (0 or 1) | a number |
//! | 13 fit | 0 contain, 1 cover, 2 fill, 3 none |
//! | 14 padding top, 15 right, 16 bottom, 17 left | dp |
//!
//! `__ui_set_length` properties: 0 width, 1 height, 2 min width, 3 max width,
//! 4 min height, 5 max height; units 0 auto, 1 dp, 2 fraction of the parent.
//! `__ui_set_corners`: the corner radii, clockwise from the top left. `__ui_set_color`: 0
//! background, 1 text colour. Element kinds: 0 row, 1 column, 2 stack, 3
//! text, 4 image.

use std::cell::RefCell;
use std::rc::Rc;

use taffy::NodeId;
use vilan_rt::{Js, Json, Str, panic_with, str_new};

use super::{TEXT_SCALE, color, with_frame, with_text};
use crate::geometry::{Corners, Edges, Point, Size};
use crate::scene::Image;
use crate::text::FontFamily;
use crate::ui::{
    Align, Anchor, DropShadow, ElementKind, Fit, Justify, Length, OutlineEntry, Role, SizeProperty,
    UiError, UiTree,
};
use crate::units::Units;

thread_local! {
    /// The program's elements: one tree per program, on the thread that
    /// draws.
    static TREE: RefCell<UiTree> = RefCell::new(UiTree::new());
    /// The outline `__ui_outline` took last, read by its number and text
    /// externs.
    static OUTLINE: RefCell<Vec<OutlineEntry>> = const { RefCell::new(Vec::new()) };
}

/// An element, as the program holds it: see the module documentation. A copy
/// is the same element, and two are `==` only when they are one.
#[derive(Clone, Debug)]
pub struct Node {
    handle: Rc<Handle>,
}

#[derive(Debug)]
struct Handle {
    id: NodeId,
}

impl Drop for Handle {
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
    fn id(&self) -> NodeId {
        self.handle.id
    }
}

fn with_tree<R>(body: impl FnOnce(&mut UiTree) -> R) -> R {
    TREE.with(|tree| body(&mut tree.borrow_mut()))
}

/// A refused tree operation is the program's failure, with the reason.
fn or_fail(result: Result<(), UiError>) {
    if let Err(error) = result {
        panic_with(&format!("`std::ui`: {error}"));
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
        ],
        f64::from(kind),
        "element kind",
    );
    Node {
        handle: Rc::new(Handle {
            id: with_tree(|tree| tree.create(kind)),
        }),
    }
}

/// `__ui_set(node, property, value)` — a property that is one number.
pub fn set(node: &Node, property: i32, value: f64) {
    let id = node.id();
    let number = value as f32;
    with_tree(|tree| match property {
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
        8 => or_fail(tree.set_opacity(id, number)),
        9 => tree.set_text_style(id, |text| text.size = Some(number.max(0.0))),
        10 => tree.set_text_style(id, |text| text.line_height = Some(number.max(0.0))),
        11 => tree.set_text_style(id, |text| text.weight = Some(number.clamp(1.0, 1000.0))),
        12 => tree.set_text_style(id, |text| text.italic = Some(value != 0.0)),
        13 => {
            let fit = code(
                &[Fit::Contain, Fit::Cover, Fit::Fill, Fit::None],
                value,
                "fit",
            );
            or_fail(tree.set_fit(id, fit))
        }
        14..=17 => or_fail(tree.set_padding_side(id, (property - 14) as usize, number)),
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
    let id = node.id();
    with_tree(|tree| or_fail(tree.set_size(id, property, length)));
}

/// `__ui_set_corners(node, top_left, top_right, bottom_right, bottom_left)`
/// — the corner radii, in dp.
pub fn set_corners(
    node: &Node,
    top_left: f64,
    top_right: f64,
    bottom_right: f64,
    bottom_left: f64,
) {
    let id = node.id();
    let corners = Corners {
        top_left: (top_left as f32).max(0.0),
        top_right: (top_right as f32).max(0.0),
        bottom_right: (bottom_right as f32).max(0.0),
        bottom_left: (bottom_left as f32).max(0.0),
    };
    with_tree(|tree| tree.set_corners(id, corners));
}

/// `__ui_set_color(node, property, colour)` — the background (0) or the
/// text colour (1, inherited).
pub fn set_color(node: &Node, property: i32, red: f64, green: f64, blue: f64, alpha: f64) {
    let id = node.id();
    let value = color(red, green, blue, alpha);
    with_tree(|tree| match property {
        0 => tree.set_background(id, value),
        1 => tree.set_text_style(id, |text| text.color = Some(value)),
        _ => panic_with(&format!("{property} is not a `std::ui` colour code")),
    });
}

/// `__ui_set_border(node, widths, colour)` — widths clockwise from the top,
/// in dp, drawn inside the box.
#[allow(
    clippy::too_many_arguments,
    reason = "an extern's arguments are the vilan call's, one scalar each"
)]
pub fn set_border(
    node: &Node,
    top: f64,
    right: f64,
    bottom: f64,
    left: f64,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) {
    let id = node.id();
    let widths = Edges {
        top: top as f32,
        right: right as f32,
        bottom: bottom as f32,
        left: left as f32,
    };
    with_tree(|tree| tree.set_border(id, widths, color(red, green, blue, alpha)));
}

/// `__ui_set_shadow(node, x, y, blur, spread, colour)` — in dp.
#[allow(
    clippy::too_many_arguments,
    reason = "an extern's arguments are the vilan call's, one scalar each"
)]
pub fn set_shadow(
    node: &Node,
    x: f64,
    y: f64,
    blur: f64,
    spread: f64,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) {
    let id = node.id();
    let shadow = DropShadow {
        offset: Point::new(x as f32, y as f32),
        blur: blur as f32,
        spread: spread as f32,
        color: color(red, green, blue, alpha),
    };
    with_tree(|tree| tree.set_shadow(id, shadow));
}

/// `__ui_set_font(node, family, name)` — the inherited font family: 0
/// sans-serif, 1 serif, 2 monospace, 3 the family `name`.
pub fn set_font(node: &Node, family: i32, name: &Str) {
    let family = match family {
        0 => FontFamily::SansSerif,
        1 => FontFamily::Serif,
        2 => FontFamily::Monospace,
        3 => FontFamily::Named(name.to_string()),
        _ => panic_with(&format!("{family} is not a `std::ui` font family code")),
    };
    let id = node.id();
    with_tree(|tree| tree.set_text_style(id, |text| text.family = Some(family)));
}

/// `__ui_append_text(node, run)` — a run of a text element's paragraph.
pub fn append_text(node: &Node, run: &Str) {
    let id = node.id();
    with_tree(|tree| or_fail(tree.append_text(id, run)));
}

/// `__ui_set_label(node, label)` — the accessibility label (R10).
pub fn set_label(node: &Node, label: &Str) {
    let id = node.id();
    with_tree(|tree| tree.set_label(id, label));
}

/// `__ui_set_texture(node, texture)` — an image element's picture.
pub fn set_texture(node: &Node, image: &Image) {
    let id = node.id();
    with_tree(|tree| or_fail(tree.set_texture(id, image.clone())));
}

/// `__ui_add_child(parent, child)` — appends `child`.
pub fn add_child(parent: &Node, child: &Node) {
    let (parent, child) = (parent.id(), child.id());
    with_tree(|tree| or_fail(tree.add_child(parent, child)));
}

fn units(scale_factor: f64) -> Units {
    Units::new(scale_factor as f32, TEXT_SCALE as f32).unwrap_or_else(|error| {
        panic_with(&format!("`std::ui`: {error}"));
    })
}

/// `__ui_layout(root, width, height, scale_factor)` — lays `root` out in a
/// `width` x `height` dp viewport, snapped to the device grid at
/// `scale_factor`. Solves only when something that moves a box changed.
pub fn layout(root: &Node, width: f64, height: f64, scale_factor: f64) {
    let id = root.id();
    let units = units(scale_factor);
    let size = Size::new(width as f32, height as f32);
    with_tree(|tree| {
        with_text(|text| {
            or_fail(tree.layout(id, size, units, text).map(|_| ()));
        })
    });
}

/// `__ui_paint(frame, root)` — draws `root`, as last laid out, into the
/// frame being filled.
pub fn paint(frame: u32, root: &Node) {
    let id = root.id();
    with_frame(frame, |builder| {
        with_tree(|tree| with_text(|text| or_fail(tree.paint(id, builder, text))))
    });
}

/// `__ui_node_number(node, which)` — the element's box from the last layout,
/// in dp: x (0), y (1), width (2), height (3).
pub fn node_number(node: &Node, which: i32) -> f64 {
    let bounds = with_tree(|tree| tree.bounds(node.id()));
    f64::from(match which {
        0 => bounds.left(),
        1 => bounds.top(),
        2 => bounds.size.width,
        3 => bounds.size.height,
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
