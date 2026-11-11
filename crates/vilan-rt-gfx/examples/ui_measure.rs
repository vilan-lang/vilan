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
//! - `rows`: a column of 10,000 rows, each a row of a label and a value text,
//!   at 960 x 640 dp (no virtualisation: every row is laid out).
//!
//! For each: `first_us` (the first layout: every paragraph shaped, the solver
//! cold), `resize_us` (a new width: re-solved, paragraphs re-wrapped from
//! their kept shapings where they can be), `unchanged_us` (nothing changed:
//! no solve), `rescale_us` (a new scale only: re-snapped), `one_change_us`
//! (the first row's minimum height changed: its dirty chain re-solved, and
//! the column places every sibling again), `paint_us` (the scene rebuilt).
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

/// A column of `count` rows, each a label and a value.
fn rows(builder: &mut Builder, count: usize) -> NodeId {
    let column = builder.node(ElementKind::Column);
    for index in 0..count {
        let row = builder.node(ElementKind::Row);
        builder.tree.set_gap(row, 8.0).unwrap();
        builder.padding(row, 4.0);
        let label = builder.text(&format!("Row {index}"));
        builder.tree.set_grow(label, 1.0).unwrap();
        let value = builder.text(&format!("{}", index * 37 % 1000));
        builder.add(row, &[label, value]);
        builder.add(column, &[row]);
    }
    column
}

fn measure(name: &str, build: impl Fn(&mut Builder) -> NodeId, runs: usize) {
    let mut firsts = Vec::new();
    let mut resizes = Vec::new();
    let mut unchanged = Vec::new();
    let mut rescales = Vec::new();
    let mut one_changes = Vec::new();
    let mut paints = Vec::new();
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
        firsts.push(timed(|| {
            tree.layout(root, Size::new(960.0, 640.0), at_100, &mut text)
                .unwrap();
        }));
        resizes.push(timed(|| {
            tree.layout(root, Size::new(720.0, 640.0), at_100, &mut text)
                .unwrap();
        }));
        unchanged.push(timed(|| {
            tree.layout(root, Size::new(720.0, 640.0), at_100, &mut text)
                .unwrap();
        }));
        rescales.push(timed(|| {
            tree.layout(root, Size::new(720.0, 640.0), at_150, &mut text)
                .unwrap();
        }));
        let first_child = tree.children(root)[0];
        tree.set_size(first_child, SizeProperty::MinHeight, Length::Dp(30.0))
            .unwrap();
        one_changes.push(timed(|| {
            tree.layout(root, Size::new(720.0, 640.0), at_150, &mut text)
                .unwrap();
        }));
        paints.push(timed(|| {
            let mut scene = SceneBuilder::new();
            tree.paint(root, &mut scene, &mut text).unwrap();
        }));
    }
    println!("{name}_elements={elements}");
    println!("{name}_first_us={}", median(firsts));
    println!("{name}_resize_us={}", median(resizes));
    println!("{name}_unchanged_us={}", median(unchanged));
    println!("{name}_rescale_us={}", median(rescales));
    println!("{name}_one_change_us={}", median(one_changes));
    println!("{name}_paint_us={}", median(paints));
}

fn main() {
    measure("screen", screen, 21);
    measure("rows", |builder| rows(builder, 10_000), 5);
}
