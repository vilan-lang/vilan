//! Measurements of `std::ui`'s layout and paint passes (native-ui.md P2),
//! printed as `key=value` lines:
//!
//!     cargo run --release --example ui_measure
//!     cargo run --example ui_measure            (the debug build)
//!
//! Two trees, built with the element tree's own API (what a vilan program's
//! `__ui_*` calls reach):
//!
//! - `screen`: `vilan/examples/ui`'s documentation page, its structure
//!   (25 elements), at 960 x 640 dp;
//! - `rows`: a heading above a `scroll` over a column of 10,000 rows, each a
//!   row of a label and a value text, at 960 x 640 dp (no virtualisation:
//!   every row is laid out) — `vilan/examples/ui-list`'s structure.
//!
//! For each: `first_us` and `first_paint_us` (the first layout and paint:
//! every paragraph shaped, the solver cold, every scene built),
//! `unchanged_us` (a frame with nothing changed: no solve, the kept scene),
//! `resize_us` (a new width), `rescale_us` (a new scale only: re-snapped).
//! For the list: `one_text_*` (one row's value text replaced by one of the
//! same width: the layout pass, the leaves it measured, the boxes that moved,
//! the paint pass and the scenes it rebuilt), `scroll_step_*` (the offset
//! moved by 96 dp, laid out and painted), and `one_text_after_resize_*` (the
//! same change in the first solve after a resize) against
//! `one_text_settled_us` (the solve after that).
//! Times are wall time on this thread, medians over runs: the passes are
//! CPU-bound and single-threaded, so on an unloaded core wall time is CPU
//! time, and on a loaded machine they are upper bounds. Text is shaped with
//! the test font (`tests/fonts/`).

use std::time::Instant;

use vilan_rt_gfx::ui::{Align, ElementKind, Length, NodeId, SizeProperty, UiTree};
use vilan_rt_gfx::{Edges, SceneBuilder, Size, TextSystem, Units};

fn timed(body: impl FnOnce()) -> u128 {
    let start = Instant::now();
    body();
    start.elapsed().as_micros()
}

fn median(mut values: Vec<u128>) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}

struct Builder {
    tree: UiTree,
}

impl Builder {
    fn node(&mut self, kind: ElementKind) -> NodeId {
        self.tree.create(kind)
    }

    fn text(&mut self, words: &str) -> NodeId {
        let node = self.node(ElementKind::Text);
        self.tree.append_text(node, words).unwrap();
        node
    }

    fn add(&mut self, parent: NodeId, children: &[NodeId]) {
        for child in children {
            self.tree.add_child(parent, *child).unwrap();
        }
    }

    fn padding(&mut self, node: NodeId, value: f32) {
        self.tree.set_padding(node, Edges::all(value)).unwrap();
    }
}

const LONG: &str = "Rows and columns place their children in a line, a gap apart; a stack puts them on top of one another, each anchored to an edge or a corner. A paragraph wraps to the width its column gives it.";

/// `vilan/examples/ui`'s page.
fn screen(builder: &mut Builder) -> NodeId {
    let root = builder.node(ElementKind::Column);
    let header = builder.node(ElementKind::Row);
    builder.padding(header, 16.0);
    builder.tree.set_gap(header, 20.0).unwrap();
    builder.tree.set_align(header, Align::Center).unwrap();
    let brand = builder.text("vilan");
    let spacer = builder.node(ElementKind::Row);
    builder.tree.set_grow(spacer, 1.0).unwrap();
    let links = [
        builder.text("Guide"),
        builder.text("Library"),
        builder.text("Examples"),
    ];
    builder.add(header, &[brand, spacer]);
    builder.add(header, &links);
    let body = builder.node(ElementKind::Row);
    builder.tree.set_grow(body, 1.0).unwrap();
    let sidebar = builder.node(ElementKind::Column);
    builder
        .tree
        .set_size(sidebar, SizeProperty::Width, Length::Dp(200.0))
        .unwrap();
    builder.tree.set_shrink(sidebar, 0.0).unwrap();
    builder.padding(sidebar, 16.0);
    for name in ["Overview", "Elements", "Layout", "Style", "Drawing"] {
        let entry = builder.text(name);
        builder.padding(entry, 6.0);
        builder.add(sidebar, &[entry]);
    }
    let content = builder.node(ElementKind::Column);
    builder.tree.set_grow(content, 1.0).unwrap();
    builder.padding(content, 28.0);
    builder.tree.set_gap(content, 16.0).unwrap();
    let title = builder.text("Layout without a style sheet");
    let first = builder.text(LONG);
    let feature = builder.node(ElementKind::Row);
    builder.tree.set_gap(feature, 20.0).unwrap();
    builder.tree.set_align(feature, Align::Start).unwrap();
    let card = builder.node(ElementKind::Stack);
    builder.padding(card, 8.0);
    let picture = builder.node(ElementKind::Image);
    builder
        .tree
        .set_size(picture, SizeProperty::Width, Length::Dp(240.0))
        .unwrap();
    builder
        .tree
        .set_size(picture, SizeProperty::Height, Length::Dp(120.0))
        .unwrap();
    let badge = builder.text("NEW");
    builder
        .tree
        .set_anchor(badge, vilan_rt_gfx::ui::Anchor::TopRight);
    builder.add(card, &[picture, badge]);
    let caption = builder.node(ElementKind::Column);
    builder.tree.set_grow(caption, 1.0).unwrap();
    builder.tree.set_gap(caption, 8.0).unwrap();
    let caption_title = builder.text("A picture, cropped to cover its box");
    let caption_text = builder.text(LONG);
    builder.add(caption, &[caption_title, caption_text]);
    builder.add(feature, &[card, caption]);
    let last = builder.text(LONG);
    builder.add(content, &[title, first, feature, last]);
    builder.add(body, &[sidebar, content]);
    builder.add(root, &[header, body]);
    root
}

/// A column of a heading and a `scroll` (growing into the rest) over a
/// column of `count` rows, each a label and a value centred across the row:
/// the exit test's list (`vilan/examples/ui-list`), its structure.
fn rows(builder: &mut Builder, count: usize) -> NodeId {
    let root = builder.node(ElementKind::Column);
    let heading = builder.text("10000 rows: scroll, and click one");
    builder.padding(heading, 12.0);
    let scroll = builder.node(ElementKind::Scroll);
    builder.tree.set_grow(scroll, 1.0).unwrap();
    builder.add(root, &[heading, scroll]);
    let column = builder.node(ElementKind::Column);
    builder.add(scroll, &[column]);
    for index in 0..count {
        let row = builder.node(ElementKind::Row);
        builder.tree.set_gap(row, 8.0).unwrap();
        builder.tree.set_align(row, Align::Center).unwrap();
        builder
            .tree
            .set_padding(
                row,
                Edges {
                    top: 6.0,
                    right: 12.0,
                    bottom: 6.0,
                    left: 12.0,
                },
            )
            .unwrap();
        let label = builder.text(&format!("Row {index}"));
        builder.tree.set_grow(label, 1.0).unwrap();
        let value = builder.text("0 clicks");
        builder.add(row, &[label, value]);
        builder.add(column, &[row]);
    }
    root
}

/// The scroll of `rows`'s list.
fn scroll_of(tree: &UiTree, root: NodeId) -> Option<NodeId> {
    let children = tree.children(root);
    let scroll = *children.get(1)?;
    (tree.kind(scroll) == ElementKind::Scroll).then_some(scroll)
}

/// The value text of row `index` of `rows`'s list.
fn value_of(tree: &UiTree, root: NodeId, index: usize) -> Option<NodeId> {
    let scroll = scroll_of(tree, root)?;
    let column = tree.children(scroll)[0];
    let row = tree.children(column)[index];
    Some(tree.children(row)[1])
}

#[derive(Default)]
struct Samples {
    rows: Vec<(&'static str, Vec<u128>)>,
}

impl Samples {
    fn push(&mut self, key: &'static str, value: u128) {
        match self.rows.iter_mut().find(|(name, _)| *name == key) {
            Some((_, values)) => values.push(value),
            None => self.rows.push((key, vec![value])),
        }
    }
}

fn measure(name: &str, build: impl Fn(&mut Builder) -> NodeId, runs: usize) {
    let mut samples = Samples::default();
    let mut elements = 0;
    let at_100 = Units::new(1.0, 1.0).unwrap();
    let at_150 = Units::new(1.5, 1.0).unwrap();
    for _ in 0..runs {
        let mut text = TextSystem::without_system_fonts();
        let font = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fonts/NotoMono-Regular.ttf"),
        )
        .expect("the test font");
        text.register_font(font).unwrap();
        let mut builder = Builder {
            tree: UiTree::new(),
        };
        let root = build(&mut builder);
        elements = builder.tree.element_count();
        let tree = &mut builder.tree;
        let paint = |tree: &mut UiTree, text: &mut TextSystem| {
            let mut scene = SceneBuilder::new();
            tree.paint(root, &mut scene, text).unwrap();
        };
        samples.push(
            "first_us",
            timed(|| {
                tree.layout(root, Size::new(960.0, 640.0), at_100, &mut text)
                    .unwrap();
            }),
        );
        samples.push("first_measured", tree.last_pass().measured as u128);
        samples.push("first_paint_us", timed(|| paint(tree, &mut text)));
        samples.push(
            "unchanged_us",
            timed(|| {
                tree.layout(root, Size::new(960.0, 640.0), at_100, &mut text)
                    .unwrap();
                paint(tree, &mut text);
            }),
        );
        if let Some(value) = value_of(tree, root, 5_000) {
            // One row's text changes, its width the same: the solver
            // measures that text again and places the column's rows; one
            // row's scene and the chain above it are rebuilt.
            tree.set_run(value, 0, "1 clicks").unwrap();
            samples.push(
                "one_text_layout_us",
                timed(|| {
                    tree.layout(root, Size::new(960.0, 640.0), at_100, &mut text)
                        .unwrap();
                }),
            );
            samples.push("one_text_measured", tree.last_pass().measured as u128);
            samples.push("one_text_moved", tree.last_pass().moved as u128);
            samples.push("one_text_paint_us", timed(|| paint(tree, &mut text)));
            samples.push("one_text_rebuilt", tree.last_pass().rebuilt as u128);
            // A scroll: the offset moves; nothing is laid out, and only the
            // scroll's own scene is rebuilt.
            let mut offset = 0.0;
            samples.push(
                "scroll_step_us",
                timed(|| {
                    offset += 96.0;
                    let scroll = scroll_of(tree, root).expect("the list's scroll");
                    tree.set_scroll_offset(scroll, vilan_rt_gfx::Point::new(0.0, offset))
                        .unwrap();
                    tree.layout(root, Size::new(960.0, 640.0), at_100, &mut text)
                        .unwrap();
                    paint(tree, &mut text);
                }),
            );
            samples.push("scroll_step_rebuilt", tree.last_pass().rebuilt as u128);
        }
        samples.push(
            "resize_us",
            timed(|| {
                tree.layout(root, Size::new(720.0, 640.0), at_100, &mut text)
                    .unwrap();
            }),
        );
        samples.push(
            "rescale_us",
            timed(|| {
                tree.layout(root, Size::new(720.0, 640.0), at_150, &mut text)
                    .unwrap();
            }),
        );
        if let Some(value) = value_of(tree, root, 5_000) {
            // The same change in the first solve after a resize: `taffy`'s
            // per-node caches kept entries the resize pass did not ask for
            // again, and this pass misses them (see the report).
            tree.set_run(value, 0, "2 clicks").unwrap();
            samples.push(
                "one_text_after_resize_us",
                timed(|| {
                    tree.layout(root, Size::new(720.0, 640.0), at_150, &mut text)
                        .unwrap();
                }),
            );
            samples.push(
                "one_text_after_resize_measured",
                tree.last_pass().measured as u128,
            );
            tree.set_run(value, 0, "3 clicks").unwrap();
            samples.push(
                "one_text_settled_us",
                timed(|| {
                    tree.layout(root, Size::new(720.0, 640.0), at_150, &mut text)
                        .unwrap();
                }),
            );
        }
    }
    println!("{name}_elements={elements}");
    for (key, values) in samples.rows {
        println!("{name}_{key}={}", median(values));
    }
}

fn main() {
    measure("screen", screen, 21);
    measure("rows", |builder| rows(builder, 10_000), 5);
}
