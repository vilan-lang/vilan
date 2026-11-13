//! Pins of the `icon` element (gfx-path.md S3, R9 and R10): its measure and
//! default size, where and how its path is drawn, its colour inherited (and
//! following a change, a hovered button's included) or set, what a change
//! of shape, of size and of place costs, and what assistive technology is
//! told of it.
//!
//! Text is shaped with the test font (Noto Mono, every advance 0.6 em).

use super::*;
use crate::color::Color;
use crate::geometry::Size;
use crate::path::{FillRule, Stroke};
use crate::scene::{Command, Scene};
use crate::text::TextSystem;
use crate::ui::{
    AccessWindow, Align, Condition, ElementKind, Input, Length, PassStats, Role, SizeProperty,
    access_listing,
};
use crate::units::Units;

const CHECK: &str = "M20 6 9 17l-5-5";
const CHEVRON: &str = "m6 9 6 6 6-6";

fn text_system() -> TextSystem {
    let mut system = TextSystem::without_system_fonts();
    let font = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fonts/NotoMono-Regular.ttf"),
    )
    .expect("the test font");
    system.register_font(font).expect("the test font registers");
    system
}

fn red() -> Color {
    Color::new(1.0, 0.0, 0.0, 1.0)
}

fn blue() -> Color {
    Color::new(0.0, 0.0, 1.0, 1.0)
}

/// A Lucide-style shape: `data` in a 24-unit box, stroked 2 units wide.
fn stroked(data: &str) -> IconShape {
    IconShape::new(
        Path::from_svg(data).expect("the data parses"),
        24.0,
        PathPaint::Stroke(Stroke::new(2.0)),
    )
    .expect("a shape")
}

struct Fixture {
    tree: UiTree,
    text: TextSystem,
}

/// One path drawn, in window coordinates.
#[derive(Debug, PartialEq)]
struct Drawn {
    origin: Point,
    scale: f32,
    color: Color,
}

impl Fixture {
    fn new() -> Fixture {
        Fixture {
            tree: UiTree::new(),
            text: text_system(),
        }
    }

    fn element(&mut self, kind: ElementKind) -> NodeId {
        self.tree.create(kind)
    }

    fn icon(&mut self, data: &str) -> NodeId {
        let node = self.element(ElementKind::Icon);
        self.tree.set_icon(node, stroked(data)).unwrap();
        node
    }

    fn text(&mut self, words: &str) -> NodeId {
        let node = self.element(ElementKind::Text);
        self.tree.append_text(node, words).unwrap();
        node
    }

    fn add(&mut self, parent: NodeId, children: &[NodeId]) {
        for child in children {
            self.tree.add_child(parent, *child).unwrap();
        }
    }

    fn layout_at(&mut self, root: NodeId, scale: f32) -> bool {
        self.tree
            .layout(
                root,
                Size::new(400.0, 300.0),
                Units::new(scale, 1.0).unwrap(),
                &mut self.text,
            )
            .unwrap()
    }

    fn layout(&mut self, root: NodeId) -> bool {
        self.layout_at(root, 1.0)
    }

    fn rect(&self, node: NodeId) -> (f32, f32, f32, f32) {
        let bounds = self.tree.bounds(node);
        (
            bounds.left(),
            bounds.top(),
            bounds.size.width,
            bounds.size.height,
        )
    }

    /// Paints `root`: every path it draws, in window coordinates, and what
    /// the pass did.
    fn paint(&mut self, root: NodeId) -> (Vec<Drawn>, PassStats) {
        let mut builder = SceneBuilder::new();
        self.tree.paint(root, &mut builder, &mut self.text).unwrap();
        let mut drawn = Vec::new();
        paths(&builder.finish().unwrap(), Point::ZERO, &mut drawn);
        (drawn, self.tree.last_pass())
    }

    fn listing(&self, root: NodeId) -> String {
        let window = AccessWindow {
            title: "pins",
            scale: 1.0,
            width: 400.0,
            height: 300.0,
        };
        access_listing(&self.tree.access_tree(Some(root), window), false)
    }
}

/// The paths `scene` draws, its placed scenes moved by their offsets.
fn paths(scene: &Scene, offset: Point, into: &mut Vec<Drawn>) {
    for command in scene.commands.iter() {
        match command {
            Command::Scene(nested) => paths(nested, offset, into),
            Command::PlacedScene { scene, offset: by } => {
                paths(scene, Point::new(offset.x + by.x, offset.y + by.y), into)
            }
            Command::Path {
                origin,
                scale,
                color,
                ..
            } => into.push(Drawn {
                origin: Point::new(origin.x + offset.x, origin.y + offset.y),
                scale: *scale,
                color: *color,
            }),
            _ => {}
        }
    }
}

/// A column holding `children`, its children at their own width.
fn column(fixture: &mut Fixture, children: &[NodeId]) -> NodeId {
    let column = fixture.element(ElementKind::Column);
    fixture.tree.set_align(column, Align::Start).unwrap();
    fixture.add(column, children);
    column
}

// ---------------------------------------------------------------- size ---

/// R9: square; unset, the inherited text size; `size` sets a side. A width
/// sizes the box alone (see the next pin for what is drawn in it).
#[test]
fn an_icon_is_a_square_of_its_size_or_else_the_inherited_text_size() {
    let mut fixture = Fixture::new();
    let inherited = fixture.icon(CHECK);
    let sized = fixture.icon(CHECK);
    fixture.tree.set_icon_size(sized, Some(24.0)).unwrap();
    let wide = fixture.icon(CHECK);
    fixture
        .tree
        .set_size(wide, SizeProperty::Width, Length::Dp(40.0))
        .unwrap();
    let root = column(&mut fixture, &[inherited, sized, wide]);
    fixture.layout(root);
    assert_eq!(
        fixture.rect(inherited),
        (0.0, 0.0, 14.0, 14.0),
        "the default text size"
    );
    assert_eq!(fixture.rect(sized), (0.0, 14.0, 24.0, 24.0));
    assert_eq!(
        fixture.rect(wide),
        (0.0, 38.0, 40.0, 14.0),
        "a width is the box's"
    );

    // The inherited size, set above it, and changed: the icon follows.
    fixture
        .tree
        .set_text_style(root, |style| style.size = Some(20.0));
    assert!(fixture.layout(root), "a new text size is a layout");
    assert_eq!(fixture.rect(inherited), (0.0, 0.0, 20.0, 20.0));
    assert_eq!(
        fixture.rect(sized),
        (0.0, 20.0, 24.0, 24.0),
        "its own size stays"
    );

    // Unset again: the text size.
    fixture.tree.set_icon_size(sized, None).unwrap();
    assert!(fixture.layout(root));
    assert_eq!(fixture.rect(sized), (0.0, 20.0, 20.0, 20.0));
}

/// An icon in a row beside its label, centred across the row, takes part
/// in the row as an image does, and its box snaps to the device grid.
#[test]
fn an_icon_sits_in_a_row_beside_text_and_snaps_like_any_box() {
    let mut fixture = Fixture::new();
    let icon = fixture.icon(CHECK);
    let label = fixture.text("Saved");
    let row = fixture.element(ElementKind::Row);
    fixture.tree.set_gap(row, 8.0).unwrap();
    fixture.tree.set_align(row, Align::Center).unwrap();
    fixture
        .tree
        .set_padding(row, crate::geometry::Edges::all(0.3))
        .unwrap();
    fixture.add(row, &[icon, label]);
    fixture
        .tree
        .set_text_style(row, |style| style.size = Some(15.0));
    let root = column(&mut fixture, &[row]);
    fixture.layout(root);
    let (x, y, width, height) = fixture.rect(icon);
    assert_eq!((width, height), (15.0, 15.0));
    let label_box = fixture.rect(label);
    assert_eq!(label_box.0, (x + width + 8.0).round(), "the gap after it");
    let middle = |(_, top, _, tall): (f32, f32, f32, f32)| top + tall / 2.0;
    assert!(
        (middle(fixture.rect(icon)) - middle(label_box)).abs() <= 1.0,
        "centred with the text"
    );
    assert_eq!((x, y), (x.round(), y.round()), "snapped at 100%");

    // At 150% every edge is a whole device pixel.
    fixture.layout_at(root, 1.5);
    let (x, y, width, height) = fixture.rect(icon);
    for edge in [x, y, x + width, y + height] {
        let device = edge * 1.5;
        assert!((device - device.round()).abs() < 1e-3, "{edge} dp at 150%");
    }
    // And the path is drawn from the snapped box's corner.
    let (drawn, _) = fixture.paint(root);
    assert_eq!(drawn.len(), 1);
    assert_eq!(drawn[0].origin, Point::new(x, y));
}

/// The shape is drawn at the icon's side (scale = side / box) from the
/// content box's top-left corner, as a text draws its glyphs: a box made
/// larger (given a size, or stretched across a column) does not grow the
/// icon. With no shape the icon draws nothing and keeps its size.
#[test]
fn the_path_is_drawn_at_its_side_from_the_content_boxs_corner() {
    let mut fixture = Fixture::new();
    let icon = fixture.icon(CHECK);
    fixture.tree.set_icon_size(icon, Some(18.0)).unwrap();
    fixture
        .tree
        .set_size(icon, SizeProperty::Width, Length::Dp(48.0))
        .unwrap();
    fixture
        .tree
        .set_size(icon, SizeProperty::Height, Length::Dp(32.0))
        .unwrap();
    fixture
        .tree
        .set_padding(icon, crate::geometry::Edges::all(4.0))
        .unwrap();
    let empty = fixture.element(ElementKind::Icon);
    // A column that stretches its children: the icon's box is the column's
    // width, and the icon is its side.
    let stretched = fixture.icon(CHECK);
    let inner = fixture.element(ElementKind::Column);
    fixture
        .tree
        .set_size(inner, SizeProperty::Width, Length::Dp(200.0))
        .unwrap();
    fixture.add(inner, &[stretched]);
    let root = column(&mut fixture, &[icon, empty, inner]);
    fixture.layout(root);
    assert_eq!(fixture.rect(icon), (0.0, 0.0, 48.0, 32.0));
    assert_eq!(
        fixture.rect(empty),
        (0.0, 32.0, 14.0, 14.0),
        "no shape, still its size"
    );
    assert_eq!(
        fixture.rect(stretched),
        (0.0, 46.0, 200.0, 14.0),
        "stretched across"
    );
    let (drawn, _) = fixture.paint(root);
    let black = Color::new(0.0, 0.0, 0.0, 1.0);
    assert_eq!(
        drawn,
        [
            Drawn {
                origin: Point::new(4.0, 4.0),
                scale: 18.0 / 24.0,
                color: black,
            },
            Drawn {
                origin: Point::new(0.0, 46.0),
                scale: 14.0 / 24.0,
                color: black,
            },
        ],
        "the empty icon draws none"
    );
}

// -------------------------------------------------------------- colour ---

/// R9: the inherited text colour, as `currentColor`; a change of it
/// repaints the icon (with no layout pass), and a colour set on the icon
/// itself wins.
#[test]
fn an_icon_is_drawn_in_the_inherited_text_colour_and_follows_its_change() {
    let mut fixture = Fixture::new();
    let inherited = fixture.icon(CHECK);
    let own = fixture.icon(CHECK);
    fixture
        .tree
        .set_text_style(own, |style| style.color = Some(blue()));
    let root = column(&mut fixture, &[inherited, own]);
    fixture
        .tree
        .set_text_style(root, |style| style.color = Some(red()));
    fixture.layout(root);
    let colours = |drawn: &[Drawn]| drawn.iter().map(|drawn| drawn.color).collect::<Vec<_>>();
    let (drawn, _) = fixture.paint(root);
    assert_eq!(colours(&drawn), [red(), blue()], "inherited, then its own");

    let green = Color::new(0.0, 1.0, 0.0, 1.0);
    fixture
        .tree
        .set_text_style(root, |style| style.color = Some(green));
    assert!(!fixture.layout(root), "a colour is no layout");
    let (drawn, stats) = fixture.paint(root);
    assert_eq!(colours(&drawn), [green, blue()], "the override holds");
    assert_eq!(
        stats.rebuilt, 2,
        "the inheriting icon and the root: {stats:?}"
    );
}

/// A hovered button turns its text and its icon red together: one
/// condition on the button, inherited by both.
#[test]
fn a_hovered_buttons_icon_turns_with_its_text() {
    let mut fixture = Fixture::new();
    let icon = fixture.icon(CHECK);
    let label = fixture.text("Save");
    let button = fixture.element(ElementKind::Row);
    fixture.tree.set_role(button, Role::Button);
    fixture.add(button, &[icon, label]);
    let group = fixture.tree.style_group(button, None);
    let layer = fixture
        .tree
        .style_layer(
            button,
            group,
            Some(Condition {
                hovered: true,
                ..Condition::default()
            }),
        )
        .unwrap();
    fixture
        .tree
        .set_style_fields(button, layer, |fields| fields.text.color = Some(red()));
    fixture.tree.restyle(button);
    let root = column(&mut fixture, &[button]);
    fixture.layout(root);
    let (drawn, _) = fixture.paint(root);
    assert_eq!(drawn[0].color, Color::new(0.0, 0.0, 0.0, 1.0));

    fixture
        .tree
        .route(root, &Input::PointerMoved(Point::new(4.0, 4.0)));
    assert!(!fixture.layout(root), "a look: no solve");
    assert_eq!(fixture.tree.resolved_text(label).color, red(), "the text");
    let (drawn, stats) = fixture.paint(root);
    assert_eq!(drawn[0].color, red(), "and the icon");
    assert_eq!(stats.rebuilt, 4, "the icon, the text, the button, the root");

    fixture
        .tree
        .route(root, &Input::PointerMoved(Point::new(300.0, 200.0)));
    fixture.layout(root);
    let (drawn, _) = fixture.paint(root);
    assert_eq!(drawn[0].color, Color::new(0.0, 0.0, 0.0, 1.0), "and back");
}

// ---------------------------------------------------------------- cost ---

/// A new shape (a signal-bound `of`: a chevron that flips) repaints the icon
/// and the chain above it and runs no solve; the same drawing again (the
/// same data parsed again) changes nothing.
#[test]
fn a_new_shape_repaints_its_chain_alone_and_the_same_one_nothing() {
    let mut fixture = Fixture::new();
    let icon = fixture.icon(CHEVRON);
    let other = fixture.icon(CHECK);
    let row = fixture.element(ElementKind::Row);
    fixture.add(row, &[icon, other]);
    let root = column(&mut fixture, &[row]);
    fixture.layout(root);
    fixture.paint(root);

    fixture.tree.set_icon(icon, stroked(CHECK)).unwrap();
    assert!(!fixture.layout(root), "a shape is no layout");
    let (_, stats) = fixture.paint(root);
    assert_eq!(stats.rebuilt, 3, "the icon, its row, the root: {stats:?}");
    assert!(
        fixture
            .tree
            .icon_shape(icon)
            .unwrap()
            .path()
            .same_shape(fixture.tree.icon_shape(other).unwrap().path())
    );

    let generations = fixture.tree.generations();
    fixture.tree.set_icon(icon, stroked(CHECK)).unwrap();
    assert_eq!(fixture.tree.generations(), generations, "the same drawing");
    let filled = IconShape::new(
        Path::from_svg(CHECK).unwrap(),
        24.0,
        PathPaint::Fill(FillRule::NonZero),
    )
    .unwrap();
    fixture.tree.set_icon(icon, filled).unwrap();
    assert_ne!(
        fixture.tree.generations(),
        generations,
        "another paint is a change"
    );
    fixture.layout(root);
    assert_eq!(fixture.paint(root).1.rebuilt, 3);
}

/// An icon that only moves (padding above it grows) keeps its scene: its
/// parent places it anew, and the path is drawn where the box went.
#[test]
fn an_icon_that_moves_keeps_its_scene() {
    let mut fixture = Fixture::new();
    let icon = fixture.icon(CHECK);
    let root = column(&mut fixture, &[icon]);
    fixture.layout(root);
    fixture.paint(root);
    fixture.tree.set_padding_side(root, 0, 10.0).unwrap();
    assert!(fixture.layout(root), "padding is a layout");
    let (drawn, stats) = fixture.paint(root);
    assert_eq!(stats.rebuilt, 1, "the root alone, not the icon: {stats:?}");
    assert_eq!(drawn[0].origin, Point::new(0.0, 10.0));
}

/// A size change is a layout of the icon, measured alone.
#[test]
fn a_size_change_measures_the_icon_alone() {
    let mut fixture = Fixture::new();
    let icon = fixture.icon(CHECK);
    let words = fixture.text("words");
    let root = column(&mut fixture, &[words, icon]);
    fixture.layout(root);
    fixture.tree.set_icon_size(icon, Some(30.0)).unwrap();
    assert!(fixture.layout(root));
    assert_eq!(fixture.tree.last_measured(), [icon]);
    assert_eq!(fixture.rect(icon).2, 30.0);
    let (drawn, _) = fixture.paint(root);
    assert_eq!(drawn[0].scale, 30.0 / 24.0);
}

// ------------------------------------------------------------- refusals ---

#[test]
fn an_icons_properties_are_refused_on_another_kind_and_a_bad_size_or_box_by_reason() {
    let mut fixture = Fixture::new();
    let text = fixture.text("a");
    assert_eq!(
        fixture.tree.set_icon(text, stroked(CHECK)),
        Err(UiError::WrongKind {
            property: "icon",
            kind: ElementKind::Text
        })
    );
    let icon = fixture.icon(CHECK);
    for size in [-1.0, f32::NAN, f32::INFINITY] {
        assert!(
            matches!(
                fixture.tree.set_icon_size(icon, Some(size)),
                Err(UiError::InvalidIcon(_))
            ),
            "{size}"
        );
    }
    let path = Path::from_svg(CHECK).unwrap();
    for view_box in [0.0, -24.0, f32::NAN] {
        assert!(
            IconShape::new(path.clone(), view_box, PathPaint::Fill(FillRule::NonZero)).is_err(),
            "{view_box}"
        );
    }
    assert!(IconShape::new(path, 24.0, PathPaint::Stroke(Stroke::new(-1.0))).is_err());
}

// ------------------------------------------------------- accessibility ---

/// R10: decorative unless labelled — published as nothing, in a list too;
/// labelled, an image named by its label. Inside a labelled button it says
/// nothing; an icon-only button is named by its icon's label.
#[test]
fn an_icon_is_decorative_unless_labelled_and_an_image_when_it_is() {
    let mut fixture = Fixture::new();
    let decorative = fixture.icon(CHECK);
    let labelled = fixture.icon(CHECK);
    fixture.tree.set_label(labelled, "done");
    let save_icon = fixture.icon(CHECK);
    let save_text = fixture.text("Save");
    let save = fixture.element(ElementKind::Row);
    fixture.tree.set_role(save, Role::Button);
    fixture.tree.set_label(save, "save");
    fixture.add(save, &[save_icon, save_text]);
    let trash_icon = fixture.icon(CHEVRON);
    fixture.tree.set_label(trash_icon, "delete");
    let trash = fixture.element(ElementKind::Row);
    fixture.tree.set_role(trash, Role::Button);
    fixture.add(trash, &[trash_icon]);
    let item_icon = fixture.icon(CHECK);
    let item_text = fixture.text("Milk");
    let list = fixture.element(ElementKind::Column);
    fixture.tree.set_role(list, Role::List);
    fixture.add(list, &[item_icon, item_text]);
    let root = column(&mut fixture, &[decorative, labelled, save, trash, list]);
    fixture.layout(root);
    assert_eq!(
        fixture.listing(root),
        concat!(
            "Image \"done\"\n",
            "Button \"save\" [click]\n",
            "Button \"delete\" [click]\n",
            "List\n",
            "  ListItem \"Milk\"\n",
        )
    );
    let outline = fixture.tree.outline(root);
    let kinds: Vec<_> = outline
        .iter()
        .map(|entry| (entry.kind.name(), entry.label.as_deref()))
        .collect();
    assert_eq!(kinds[1], ("icon", None));
    assert_eq!(kinds[2], ("icon", Some("done")));
}
