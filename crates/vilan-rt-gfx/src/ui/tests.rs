//! Pins of the element tree: one per layout property, the stack's anchors,
//! text measurement and inheritance, image fit, snapping, the retained
//! passes, freeing, and every refusal.
//!
//! Text is shaped with the test font only (Noto Mono, every advance 0.6 em),
//! so a paragraph's width is arithmetic: ten characters at 10 dp are 60 dp.

use super::*;
use crate::scene::Command;

fn text_system() -> TextSystem {
    let mut system = TextSystem::without_system_fonts();
    let font = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fonts/NotoMono-Regular.ttf"),
    )
    .expect("the test font");
    system.register_font(font).expect("the test font registers");
    system
}

fn units(scale: f32) -> Units {
    Units::new(scale, 1.0).expect("units")
}

struct Fixture {
    tree: UiTree,
    text: TextSystem,
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

    /// A fixed-size empty box: a column with a width and a height.
    fn block(&mut self, width: f32, height: f32) -> NodeId {
        let node = self.element(ElementKind::Column);
        self.tree
            .set_size(node, SizeProperty::Width, Length::Dp(width))
            .unwrap();
        self.tree
            .set_size(node, SizeProperty::Height, Length::Dp(height))
            .unwrap();
        node
    }

    fn paragraph(&mut self, text: &str, size: f32) -> NodeId {
        let node = self.element(ElementKind::Text);
        self.tree.append_text(node, text).unwrap();
        self.tree
            .set_text_style(node, |style| style.size = Some(size));
        node
    }

    fn add(&mut self, parent: NodeId, children: &[NodeId]) {
        for child in children {
            self.tree.add_child(parent, *child).unwrap();
        }
    }

    fn layout(&mut self, root: NodeId, width: f32, height: f32) -> bool {
        self.layout_at(root, width, height, 1.0)
    }

    fn layout_at(&mut self, root: NodeId, width: f32, height: f32, scale: f32) -> bool {
        self.tree
            .layout(root, Size::new(width, height), units(scale), &mut self.text)
            .unwrap()
    }

    /// `(x, y, width, height)` of an element's box.
    fn rect(&self, node: NodeId) -> (f32, f32, f32, f32) {
        let bounds = self.tree.bounds(node);
        (
            bounds.left(),
            bounds.top(),
            bounds.size.width,
            bounds.size.height,
        )
    }
}

fn row(fixture: &mut Fixture, width: f32, height: f32) -> NodeId {
    let row = fixture.element(ElementKind::Row);
    fixture
        .tree
        .set_size(row, SizeProperty::Width, Length::Dp(width))
        .unwrap();
    fixture
        .tree
        .set_size(row, SizeProperty::Height, Length::Dp(height))
        .unwrap();
    row
}

// ------------------------------------------------------- flex properties ---

#[test]
fn a_row_lays_its_children_left_to_right_and_a_column_top_to_bottom() {
    let mut fixture = Fixture::new();
    let row = row(&mut fixture, 300.0, 100.0);
    let (a, b) = (fixture.block(40.0, 20.0), fixture.block(60.0, 30.0));
    fixture.add(row, &[a, b]);
    fixture.layout(row, 300.0, 100.0);
    assert_eq!(fixture.rect(a), (0.0, 0.0, 40.0, 20.0));
    assert_eq!(fixture.rect(b), (40.0, 0.0, 60.0, 30.0));

    let column = fixture.element(ElementKind::Column);
    let (c, d) = (fixture.block(40.0, 20.0), fixture.block(60.0, 30.0));
    fixture.add(column, &[c, d]);
    fixture.layout(column, 300.0, 100.0);
    assert_eq!(fixture.rect(c), (0.0, 0.0, 40.0, 20.0));
    assert_eq!(fixture.rect(d), (0.0, 20.0, 60.0, 30.0));
}

#[test]
fn gap_spaces_siblings_and_not_the_ends() {
    let mut fixture = Fixture::new();
    let row = row(&mut fixture, 300.0, 100.0);
    let children = [
        fixture.block(10.0, 10.0),
        fixture.block(10.0, 10.0),
        fixture.block(10.0, 10.0),
    ];
    fixture.add(row, &children);
    fixture.tree.set_gap(row, 8.0).unwrap();
    fixture.layout(row, 300.0, 100.0);
    let lefts: Vec<f32> = children
        .iter()
        .map(|child| fixture.rect(*child).0)
        .collect();
    assert_eq!(lefts, vec![0.0, 18.0, 36.0]);
}

#[test]
fn padding_insets_the_children_on_each_side() {
    let mut fixture = Fixture::new();
    let column = fixture.element(ElementKind::Column);
    let child = fixture.block(10.0, 10.0);
    fixture.add(column, &[child]);
    fixture
        .tree
        .set_padding(
            column,
            Edges {
                top: 4.0,
                right: 6.0,
                bottom: 8.0,
                left: 12.0,
            },
        )
        .unwrap();
    fixture.tree.set_align(column, Align::Start).unwrap();
    fixture
        .tree
        .set_size(column, SizeProperty::Width, Length::Auto)
        .unwrap();
    let root = fixture.element(ElementKind::Stack);
    fixture.add(root, &[column]);
    fixture.layout(root, 300.0, 100.0);
    assert_eq!(fixture.rect(child), (12.0, 4.0, 10.0, 10.0));
    // The column hugs its child plus its padding (it sits in a stack).
    assert_eq!(fixture.rect(column), (0.0, 0.0, 28.0, 22.0));
}

#[test]
fn grow_shares_the_leftover_space_by_factor() {
    let mut fixture = Fixture::new();
    let row = row(&mut fixture, 300.0, 100.0);
    let (fixed, one, two) = (
        fixture.block(60.0, 10.0),
        fixture.block(0.0, 10.0),
        fixture.block(0.0, 10.0),
    );
    fixture.add(row, &[fixed, one, two]);
    fixture.tree.set_grow(one, 1.0).unwrap();
    fixture.tree.set_grow(two, 2.0).unwrap();
    fixture.layout(row, 300.0, 100.0);
    assert_eq!(fixture.rect(one), (60.0, 0.0, 80.0, 10.0));
    assert_eq!(fixture.rect(two), (140.0, 0.0, 160.0, 10.0));
}

#[test]
fn shrink_gives_up_the_shortfall_by_factor_and_zero_holds() {
    let mut fixture = Fixture::new();
    let row = row(&mut fixture, 100.0, 50.0);
    let (holds, gives) = (fixture.block(80.0, 10.0), fixture.block(80.0, 10.0));
    fixture.add(row, &[holds, gives]);
    fixture.tree.set_shrink(holds, 0.0).unwrap();
    fixture.layout(row, 100.0, 50.0);
    assert_eq!(fixture.rect(holds).2, 80.0);
    assert_eq!(fixture.rect(gives).2, 20.0);
    // The default shrink (1) shares the shortfall equally between equals.
    fixture.tree.set_shrink(holds, 1.0).unwrap();
    fixture.layout(row, 100.0, 50.0);
    assert_eq!(fixture.rect(holds).2, 50.0);
    assert_eq!(fixture.rect(gives).2, 50.0);
}

#[test]
fn align_places_children_across_the_axis() {
    let cases = [
        (Align::Start, 0.0, 20.0),
        (Align::Center, 40.0, 20.0),
        (Align::End, 80.0, 20.0),
        (Align::Stretch, 0.0, 100.0),
    ];
    for (align, top, height) in cases {
        let mut fixture = Fixture::new();
        let row = row(&mut fixture, 300.0, 100.0);
        let child = fixture.block(30.0, 20.0);
        if align == Align::Stretch {
            fixture
                .tree
                .set_size(child, SizeProperty::Height, Length::Auto)
                .unwrap();
        }
        fixture.add(row, &[child]);
        fixture.tree.set_align(row, align).unwrap();
        fixture.layout(row, 300.0, 100.0);
        assert_eq!(
            (fixture.rect(child).1, fixture.rect(child).3),
            (top, height),
            "{align:?}"
        );
    }
}

#[test]
fn align_baseline_lines_up_first_baselines() {
    let mut fixture = Fixture::new();
    let row = row(&mut fixture, 300.0, 100.0);
    let (small, large) = (
        fixture.paragraph("small", 10.0),
        fixture.paragraph("large", 20.0),
    );
    fixture.add(row, &[small, large]);
    fixture.tree.set_align(row, Align::Baseline).unwrap();
    fixture.layout(row, 300.0, 100.0);
    let first_baseline = |fixture: &mut Fixture, node: NodeId, size: f32| {
        let layout = fixture.text.layout(
            "x",
            &TextStyle {
                size,
                ..TextStyle::default()
            },
            None,
        );
        fixture.rect(node).1 + layout.first_baseline()
    };
    let small_baseline = first_baseline(&mut fixture, small, 10.0);
    let large_baseline = first_baseline(&mut fixture, large, 20.0);
    assert!(
        (small_baseline - large_baseline).abs() < 1.0,
        "{small_baseline} {large_baseline}"
    );
    assert!(fixture.rect(small).1 > 0.0, "the small text moved down");
}

#[test]
fn justify_shares_the_space_along_the_axis() {
    let cases = [
        (Justify::Start, [0.0_f32, 20.0]),
        (Justify::Center, [130.0, 150.0]),
        (Justify::End, [260.0, 280.0]),
        (Justify::SpaceBetween, [0.0, 280.0]),
        (Justify::SpaceAround, [65.0, 215.0]),
        (Justify::SpaceEvenly, [86.666_67, 193.333_33]),
    ];
    for (justify, expected) in cases {
        let mut fixture = Fixture::new();
        let row = row(&mut fixture, 300.0, 100.0);
        let children = [fixture.block(20.0, 10.0), fixture.block(20.0, 10.0)];
        fixture.add(row, &children);
        fixture.tree.set_justify(row, justify).unwrap();
        fixture.layout(row, 300.0, 100.0);
        for (child, left) in children.iter().zip(expected) {
            // Snapped at 100%: the nearest whole dp.
            assert_eq!(fixture.rect(*child).0, left.round(), "{justify:?}");
        }
    }
}

#[test]
fn wrap_starts_a_new_line_and_the_gap_spaces_the_lines() {
    let mut fixture = Fixture::new();
    let row = row(&mut fixture, 100.0, 200.0);
    let children = [
        fixture.block(40.0, 10.0),
        fixture.block(40.0, 10.0),
        fixture.block(40.0, 10.0),
    ];
    fixture.add(row, &children);
    fixture.tree.set_gap(row, 5.0).unwrap();
    fixture.layout(row, 100.0, 200.0);
    // Without wrap the three shrink onto one line.
    assert_eq!(fixture.rect(children[2]).1, 0.0);
    fixture.tree.set_wrap(row, true).unwrap();
    fixture.layout(row, 100.0, 200.0);
    assert_eq!(fixture.rect(children[0]), (0.0, 0.0, 40.0, 10.0));
    assert_eq!(fixture.rect(children[1]), (45.0, 0.0, 40.0, 10.0));
    assert_eq!(fixture.rect(children[2]), (0.0, 15.0, 40.0, 10.0));
}

#[test]
fn width_and_height_take_dp_fractions_and_auto() {
    let mut fixture = Fixture::new();
    let row = row(&mut fixture, 200.0, 100.0);
    let child = fixture.element(ElementKind::Column);
    fixture.add(row, &[child]);
    fixture
        .tree
        .set_size(child, SizeProperty::Width, Length::Fraction(0.25))
        .unwrap();
    fixture
        .tree
        .set_size(child, SizeProperty::Height, Length::Dp(30.0))
        .unwrap();
    fixture.layout(row, 200.0, 100.0);
    assert_eq!(fixture.rect(child), (0.0, 0.0, 50.0, 30.0));
    // Auto in a row: as wide as its (empty) content, stretched across.
    fixture
        .tree
        .set_size(child, SizeProperty::Width, Length::Auto)
        .unwrap();
    fixture
        .tree
        .set_size(child, SizeProperty::Height, Length::Auto)
        .unwrap();
    fixture.layout(row, 200.0, 100.0);
    assert_eq!(fixture.rect(child), (0.0, 0.0, 0.0, 100.0));
}

#[test]
fn min_and_max_clamp_a_size() {
    let mut fixture = Fixture::new();
    let row = row(&mut fixture, 300.0, 100.0);
    let (wide, narrow) = (fixture.block(0.0, 10.0), fixture.block(0.0, 10.0));
    fixture.add(row, &[wide, narrow]);
    fixture.tree.set_grow(wide, 1.0).unwrap();
    fixture.tree.set_grow(narrow, 1.0).unwrap();
    fixture
        .tree
        .set_size(wide, SizeProperty::MaxWidth, Length::Dp(100.0))
        .unwrap();
    fixture
        .tree
        .set_size(narrow, SizeProperty::MinHeight, Length::Dp(40.0))
        .unwrap();
    fixture.layout(row, 300.0, 100.0);
    assert_eq!(fixture.rect(wide).2, 100.0);
    assert_eq!(fixture.rect(narrow).2, 200.0);
    assert_eq!(fixture.rect(narrow).3, 40.0);
    fixture
        .tree
        .set_size(narrow, SizeProperty::MaxHeight, Length::Dp(5.0))
        .unwrap();
    fixture
        .tree
        .set_size(narrow, SizeProperty::MinHeight, Length::Auto)
        .unwrap();
    fixture
        .tree
        .set_size(wide, SizeProperty::MinWidth, Length::Dp(250.0))
        .unwrap();
    fixture.layout(row, 300.0, 100.0);
    assert_eq!(fixture.rect(narrow).3, 5.0);
    assert_eq!(fixture.rect(wide).2, 250.0, "min wins over max, as in CSS");
}

#[test]
fn flex_properties_on_a_leaf_are_refused_by_name() {
    let mut fixture = Fixture::new();
    let text = fixture.element(ElementKind::Text);
    let stack = fixture.element(ElementKind::Stack);
    assert_eq!(
        fixture.tree.set_gap(text, 4.0),
        Err(UiError::WrongKind {
            property: "gap",
            kind: ElementKind::Text
        })
    );
    assert_eq!(
        fixture.tree.set_wrap(stack, true),
        Err(UiError::WrongKind {
            property: "wrap",
            kind: ElementKind::Stack
        })
    );
    assert!(fixture.tree.set_justify(stack, Justify::Center).is_err());
    assert!(fixture.tree.set_align(text, Align::Center).is_err());
}

// ----------------------------------------------------------------- stack ---

#[test]
fn a_stack_is_as_large_as_its_largest_child_and_layers_them_top_left() {
    let mut fixture = Fixture::new();
    let stack = fixture.element(ElementKind::Stack);
    let (large, small) = (fixture.block(80.0, 40.0), fixture.block(20.0, 60.0));
    fixture.add(stack, &[large, small]);
    let root = fixture.element(ElementKind::Stack);
    fixture.add(root, &[stack]);
    fixture.layout(root, 300.0, 200.0);
    assert_eq!(fixture.rect(stack), (0.0, 0.0, 80.0, 60.0));
    assert_eq!(fixture.rect(large), (0.0, 0.0, 80.0, 40.0));
    assert_eq!(fixture.rect(small), (0.0, 0.0, 20.0, 60.0));
}

#[test]
fn each_anchor_places_a_child_in_the_stack() {
    let cases = [
        (Anchor::TopLeft, (0.0, 0.0, 20.0, 10.0)),
        (Anchor::Top, (40.0, 0.0, 20.0, 10.0)),
        (Anchor::TopRight, (80.0, 0.0, 20.0, 10.0)),
        (Anchor::Left, (0.0, 20.0, 20.0, 10.0)),
        (Anchor::Center, (40.0, 20.0, 20.0, 10.0)),
        (Anchor::Right, (80.0, 20.0, 20.0, 10.0)),
        (Anchor::BottomLeft, (0.0, 40.0, 20.0, 10.0)),
        (Anchor::Bottom, (40.0, 40.0, 20.0, 10.0)),
        (Anchor::BottomRight, (80.0, 40.0, 20.0, 10.0)),
    ];
    for (anchor, expected) in cases {
        let mut fixture = Fixture::new();
        let stack = fixture.element(ElementKind::Stack);
        fixture
            .tree
            .set_size(stack, SizeProperty::Width, Length::Dp(100.0))
            .unwrap();
        fixture
            .tree
            .set_size(stack, SizeProperty::Height, Length::Dp(50.0))
            .unwrap();
        let child = fixture.block(20.0, 10.0);
        fixture.add(stack, &[child]);
        fixture.tree.set_anchor(child, anchor);
        fixture.layout(stack, 300.0, 200.0);
        assert_eq!(fixture.rect(child), expected, "{anchor:?}");
    }
}

#[test]
fn the_fill_anchor_stretches_an_unsized_child_over_the_stack() {
    let mut fixture = Fixture::new();
    let stack = fixture.element(ElementKind::Stack);
    for (property, value) in [(SizeProperty::Width, 100.0), (SizeProperty::Height, 50.0)] {
        fixture
            .tree
            .set_size(stack, property, Length::Dp(value))
            .unwrap();
    }
    let child = fixture.element(ElementKind::Column);
    fixture.add(stack, &[child]);
    fixture.tree.set_anchor(child, Anchor::Fill);
    fixture.layout(stack, 300.0, 200.0);
    assert_eq!(fixture.rect(child), (0.0, 0.0, 100.0, 50.0));
}

#[test]
fn a_root_fills_the_viewport_unless_it_sizes_itself() {
    let mut fixture = Fixture::new();
    let root = fixture.element(ElementKind::Column);
    fixture.layout(root, 320.0, 240.0);
    assert_eq!(fixture.rect(root), (0.0, 0.0, 320.0, 240.0));
    fixture
        .tree
        .set_size(root, SizeProperty::Width, Length::Dp(100.0))
        .unwrap();
    fixture.layout(root, 320.0, 240.0);
    assert_eq!(fixture.rect(root), (0.0, 0.0, 100.0, 240.0));
}

// ------------------------------------------------------------------ text ---

#[test]
fn a_paragraph_measures_its_one_line_and_wraps_to_its_width() {
    let mut fixture = Fixture::new();
    let column = fixture.element(ElementKind::Column);
    fixture.tree.set_align(column, Align::Start).unwrap();
    // Twenty characters at 10 dp: 6 dp each.
    let text = fixture.paragraph("aaaa bbbb cccc dddd", 10.0);
    fixture.add(column, &[text]);
    fixture.layout(column, 300.0, 200.0);
    let one_line = fixture.rect(text);
    assert_eq!(one_line.2.round(), 114.0, "19 characters on one line");
    // Squeezed to 60 dp, two words a line: "aaaa bbbb" is 54 dp.
    fixture.layout(column, 60.0, 200.0);
    let wrapped = fixture.rect(text);
    assert_eq!(wrapped.2.round(), 54.0, "{wrapped:?}");
    assert_eq!(wrapped.3, 25.0, "two lines of 12.5 dp: {wrapped:?}");
}

#[test]
fn a_paragraph_is_never_narrower_than_its_longest_word() {
    let mut fixture = Fixture::new();
    // A row narrower than "wordy" (five characters, 30 dp): the paragraph
    // shrinks to its longest word and no further, overflowing the row.
    let row = row(&mut fixture, 20.0, 100.0);
    let text = fixture.paragraph("wordy word", 10.0);
    fixture.add(row, &[text]);
    fixture.layout(row, 20.0, 100.0);
    assert_eq!(fixture.rect(text).2.round(), 30.0, "five characters");
    // The same in a stack, whose cell is sized from its children's smallest
    // and largest widths.
    let stack = fixture.element(ElementKind::Stack);
    fixture
        .tree
        .set_size(stack, SizeProperty::Width, Length::Dp(20.0))
        .unwrap();
    let in_stack = fixture.paragraph("wordy word", 10.0);
    fixture.add(stack, &[in_stack]);
    fixture.layout(stack, 300.0, 100.0);
    assert_eq!(fixture.rect(in_stack).2.round(), 30.0, "in a stack");
}

#[test]
fn a_measured_width_is_reused_for_the_final_width_without_reshaping() {
    let mut fixture = Fixture::new();
    let column = fixture.element(ElementKind::Column);
    fixture.tree.set_align(column, Align::Start).unwrap();
    let text = fixture.paragraph("one two three", 10.0);
    fixture.add(column, &[text]);
    fixture.layout(column, 300.0, 100.0);
    let shapings = |fixture: &Fixture| match &fixture.tree.data(text).content {
        Content::Text(content) => content.shapings,
        _ => panic!("a text element"),
    };
    // Its min-content width (a word a line) and its one line, which answers
    // the max-content width, the definite width offered, and the final one.
    assert_eq!(shapings(&fixture), 2);
    let mut builder = SceneBuilder::new();
    fixture
        .tree
        .paint(column, &mut builder, &mut fixture.text)
        .unwrap();
    assert_eq!(shapings(&fixture), 2, "painting reshapes nothing");
}

#[test]
fn text_style_is_inherited_field_by_field_and_overrides_win() {
    let mut fixture = Fixture::new();
    let outer = fixture.element(ElementKind::Column);
    let inner = fixture.element(ElementKind::Row);
    let text = fixture.element(ElementKind::Text);
    fixture.tree.append_text(text, "hi").unwrap();
    fixture.add(outer, &[inner]);
    fixture.add(inner, &[text]);
    let red = Color::new(1.0, 0.0, 0.0, 1.0);
    fixture.tree.set_text_style(outer, |style| {
        style.size = Some(20.0);
        style.color = Some(red);
        style.family = Some(FontFamily::Monospace);
    });
    fixture
        .tree
        .set_text_style(inner, |style| style.weight = Some(700.0));
    fixture
        .tree
        .set_text_style(text, |style| style.size = Some(12.0));
    fixture.layout(outer, 300.0, 100.0);
    let resolved = fixture.tree.resolved_text(text);
    assert_eq!(resolved.style.size, 12.0, "its own size wins");
    assert_eq!(resolved.style.weight, 700.0, "from its parent");
    assert_eq!(
        resolved.style.family,
        FontFamily::Monospace,
        "from the root"
    );
    assert_eq!(resolved.color, red, "from the root");
    assert_eq!(resolved.style.line_height, 1.25, "the default");
    // A sibling text added later inherits from where it is placed.
    let late = fixture.element(ElementKind::Text);
    fixture.tree.append_text(late, "late").unwrap();
    fixture.add(inner, &[late]);
    fixture.layout(outer, 300.0, 100.0);
    assert_eq!(fixture.tree.resolved_text(late).style.size, 20.0);
}

#[test]
fn an_inherited_size_change_relays_out_the_text_under_it() {
    let mut fixture = Fixture::new();
    let column = fixture.element(ElementKind::Column);
    fixture.tree.set_align(column, Align::Start).unwrap();
    let text = fixture.paragraph("abcde", 10.0);
    fixture.tree.set_text_style(text, |style| style.size = None);
    fixture.add(column, &[text]);
    fixture
        .tree
        .set_text_style(column, |style| style.size = Some(10.0));
    fixture.layout(column, 300.0, 100.0);
    assert_eq!(fixture.rect(text).2.round(), 30.0);
    fixture
        .tree
        .set_text_style(column, |style| style.size = Some(20.0));
    assert!(fixture.layout(column, 300.0, 100.0), "the solver ran");
    assert_eq!(fixture.rect(text).2.round(), 60.0);
}

#[test]
fn a_text_colour_change_repaints_without_a_layout_pass() {
    let mut fixture = Fixture::new();
    let column = fixture.element(ElementKind::Column);
    let text = fixture.paragraph("abc", 10.0);
    fixture.add(column, &[text]);
    fixture.layout(column, 300.0, 100.0);
    let layout_before = fixture.tree.layout_generation;
    let paint_before = fixture.tree.paint_generation;
    fixture.tree.set_text_style(column, |style| {
        style.color = Some(Color::new(0.0, 0.0, 1.0, 1.0));
    });
    assert_eq!(fixture.tree.layout_generation, layout_before);
    assert!(fixture.tree.paint_generation > paint_before);
    assert!(!fixture.layout(column, 300.0, 100.0), "no solve");
    assert_eq!(
        fixture.tree.resolved_text(text).color,
        Color::new(0.0, 0.0, 1.0, 1.0),
        "but the colour resolves"
    );
}

// ----------------------------------------------------------------- image ---

fn texture(width: u32, height: u32) -> Image {
    Image::from_rgba(width, height, vec![255; (width * height * 4) as usize]).unwrap()
}

#[test]
fn an_image_is_its_textures_size_in_dp_and_keeps_its_aspect() {
    let mut fixture = Fixture::new();
    let column = fixture.element(ElementKind::Column);
    fixture.tree.set_align(column, Align::Start).unwrap();
    let image = fixture.element(ElementKind::Image);
    fixture.tree.set_texture(image, texture(40, 20)).unwrap();
    fixture.add(column, &[image]);
    fixture.layout(column, 300.0, 200.0);
    assert_eq!(fixture.rect(image), (0.0, 0.0, 40.0, 20.0));
    fixture
        .tree
        .set_size(image, SizeProperty::Width, Length::Dp(100.0))
        .unwrap();
    fixture.layout(column, 300.0, 200.0);
    assert_eq!(fixture.rect(image), (0.0, 0.0, 100.0, 50.0));
    // In a stack's cell too, where the solver sizes the cell from the child.
    let stack = fixture.element(ElementKind::Stack);
    let in_stack = fixture.element(ElementKind::Image);
    fixture.tree.set_texture(in_stack, texture(40, 20)).unwrap();
    fixture
        .tree
        .set_size(in_stack, SizeProperty::Width, Length::Dp(120.0))
        .unwrap();
    fixture.add(stack, &[in_stack]);
    fixture.layout(stack, 300.0, 200.0);
    assert_eq!(fixture.rect(in_stack), (0.0, 0.0, 120.0, 60.0));
}

/// The image primitive (and clip) each fit draws in a 100 x 50 box for a
/// 20 x 20 texture.
fn painted_image(fit: Fit) -> (Rect, bool) {
    let mut fixture = Fixture::new();
    let image = fixture.element(ElementKind::Image);
    fixture.tree.set_texture(image, texture(20, 20)).unwrap();
    fixture.tree.set_fit(image, fit).unwrap();
    for (property, value) in [(SizeProperty::Width, 100.0), (SizeProperty::Height, 50.0)] {
        fixture
            .tree
            .set_size(image, property, Length::Dp(value))
            .unwrap();
    }
    let root = fixture.element(ElementKind::Stack);
    fixture.add(root, &[image]);
    fixture.layout(root, 300.0, 200.0);
    let mut builder = SceneBuilder::new();
    fixture
        .tree
        .paint(root, &mut builder, &mut fixture.text)
        .unwrap();
    let scene = builder.finish().unwrap();
    let commands = flatten(&scene);
    let clipped = commands
        .iter()
        .any(|command| matches!(command, Command::PushClip { .. }));
    let bounds = commands
        .iter()
        .find_map(|command| match command {
            Command::Image { bounds, .. } => Some(*bounds),
            _ => None,
        })
        .expect("an image drawn");
    (bounds, clipped)
}

fn flatten(scene: &Scene) -> Vec<Command> {
    let mut commands = Vec::new();
    for command in scene.commands.iter() {
        match command {
            Command::Scene(nested) => commands.extend(flatten(nested)),
            other => commands.push(other.clone()),
        }
    }
    commands
}

#[test]
fn each_fit_draws_the_texture_by_its_rule() {
    assert_eq!(
        painted_image(Fit::Contain),
        (Rect::new(25.0, 0.0, 50.0, 50.0), false)
    );
    assert_eq!(
        painted_image(Fit::Cover),
        (Rect::new(0.0, -25.0, 100.0, 100.0), true)
    );
    assert_eq!(
        painted_image(Fit::Fill),
        (Rect::new(0.0, 0.0, 100.0, 50.0), false)
    );
    assert_eq!(
        painted_image(Fit::None),
        (Rect::new(40.0, 15.0, 20.0, 20.0), false)
    );
}

// -------------------------------------------------------------- snapping ---

#[test]
fn boxes_snap_to_the_device_grid_each_edge_on_its_own() {
    let mut fixture = Fixture::new();
    let row = row(&mut fixture, 100.0, 20.0);
    let thirds: Vec<NodeId> = (0..3).map(|_| fixture.block(0.0, 10.0)).collect();
    fixture.add(row, &thirds);
    for third in &thirds {
        fixture.tree.set_grow(*third, 1.0).unwrap();
    }
    for scale in [1.0, 1.25, 1.5, 2.0] {
        fixture.layout_at(row, 100.0, 20.0, scale);
        let mut previous_right = 0.0;
        for third in &thirds {
            let (x, _, width, _) = fixture.rect(*third);
            let device_x = x * scale;
            let device_width = width * scale;
            assert!(
                (device_x - device_x.round()).abs() < 1.0e-3
                    && (device_width - device_width.round()).abs() < 1.0e-3,
                "{x} + {width} at {scale} is not on the device grid"
            );
            assert!(
                (device_x - previous_right).abs() < 1.0e-3,
                "a seam or an overlap at {scale}"
            );
            previous_right = device_x + device_width;
        }
        assert!((previous_right - 100.0 * scale).abs() < 1.0e-3);
    }
}

#[test]
fn a_one_dp_divider_is_one_device_pixel_at_150_percent_wherever_it_sits() {
    let mut fixture = Fixture::new();
    let column = fixture.element(ElementKind::Column);
    let mut dividers = Vec::new();
    for _ in 0..6 {
        let spacer = fixture.block(10.0, 7.3);
        let divider = fixture.block(10.0, 1.0);
        fixture.add(column, &[spacer, divider]);
        dividers.push(divider);
    }
    fixture.layout_at(column, 100.0, 100.0, 1.5);
    for divider in dividers {
        let device_height = fixture.rect(divider).3 * 1.5;
        assert!(
            (device_height - 1.0).abs() < 1.0e-3,
            "a 1 dp divider is {device_height} device pixels"
        );
    }
}

// ------------------------------------------------------ retained passes ---

#[test]
fn the_solver_runs_on_a_change_or_a_new_size_and_a_scale_only_resnaps() {
    let mut fixture = Fixture::new();
    let row = row(&mut fixture, 100.0, 20.0);
    let child = fixture.block(10.0, 10.0);
    fixture.add(row, &[child]);
    fixture
        .tree
        .set_size(row, SizeProperty::Width, Length::Fraction(1.0))
        .unwrap();
    assert!(fixture.layout(row, 300.0, 100.0), "the first layout solves");
    assert!(!fixture.layout(row, 300.0, 100.0), "nothing changed");
    assert!(fixture.layout(row, 400.0, 100.0), "a resize solves");
    assert_eq!(fixture.rect(row).2, 400.0);
    assert!(
        !fixture.layout_at(row, 400.0, 100.0, 1.5),
        "a scale only snaps"
    );
    fixture
        .tree
        .set_background(child, Color::new(1.0, 0.0, 0.0, 1.0));
    assert!(!fixture.layout_at(row, 400.0, 100.0, 1.5), "paint only");
    fixture.tree.set_grow(child, 1.0).unwrap();
    assert!(
        fixture.layout_at(row, 400.0, 100.0, 1.5),
        "a layout property"
    );
    assert_eq!(fixture.rect(child).2, 400.0);
}

#[test]
fn an_unchanged_root_paints_the_same_retained_scene() {
    let mut fixture = Fixture::new();
    let root = fixture.element(ElementKind::Column);
    let child = fixture.block(10.0, 10.0);
    fixture.add(root, &[child]);
    fixture
        .tree
        .set_background(child, Color::new(1.0, 0.0, 0.0, 1.0));
    let paint = |fixture: &mut Fixture| {
        fixture.layout(root, 100.0, 100.0);
        let mut builder = SceneBuilder::new();
        fixture
            .tree
            .paint(root, &mut builder, &mut fixture.text)
            .unwrap();
        match &builder.finish().unwrap().commands[..] {
            [Command::Scene(scene)] => scene.clone(),
            other => panic!("one nested scene, not {other:?}"),
        }
    };
    let first = paint(&mut fixture);
    let second = paint(&mut fixture);
    assert!(first.same_as(&second), "nothing changed: the same scene");
    fixture
        .tree
        .set_background(child, Color::new(0.0, 1.0, 0.0, 1.0));
    let third = paint(&mut fixture);
    assert!(!third.same_as(&second), "a colour changed: a new scene");
}

#[test]
fn paint_order_is_tree_order_with_each_elements_own_look_first() {
    let mut fixture = Fixture::new();
    let root = fixture.element(ElementKind::Stack);
    let card = fixture.block(50.0, 50.0);
    fixture.tree.set_shadow(
        card,
        DropShadow {
            offset: Point::new(0.0, 2.0),
            blur: 4.0,
            spread: 1.0,
            color: Color::new(0.0, 0.0, 0.0, 0.5),
        },
    );
    fixture
        .tree
        .set_background(card, Color::new(1.0, 1.0, 1.0, 1.0));
    let label = fixture.paragraph("x", 10.0);
    let badge = fixture.block(8.0, 8.0);
    fixture
        .tree
        .set_background(badge, Color::new(1.0, 0.0, 0.0, 1.0));
    fixture.tree.set_opacity(badge, 0.5).unwrap();
    fixture.add(card, &[label]);
    fixture.add(root, &[card, badge]);
    fixture.layout(root, 100.0, 100.0);
    let mut builder = SceneBuilder::new();
    fixture
        .tree
        .paint(root, &mut builder, &mut fixture.text)
        .unwrap();
    let kinds: Vec<&str> = flatten(&builder.finish().unwrap())
        .iter()
        .map(|command| match command {
            Command::Shadow(shadow) => {
                assert_eq!(shadow.bounds, Rect::new(-1.0, 1.0, 52.0, 52.0));
                "shadow"
            }
            Command::Rectangle(_) => "rectangle",
            Command::Text { .. } => "text",
            Command::PushLayer { .. } => "layer",
            Command::Pop => "pop",
            _ => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        ["shadow", "rectangle", "text", "layer", "rectangle", "pop"]
    );
}

// ------------------------------------------------------ the tree's shape ---

#[test]
fn an_element_is_freed_when_it_has_neither_a_handle_nor_a_parent() {
    let mut fixture = Fixture::new();
    let parent = fixture.element(ElementKind::Column);
    let kept = fixture.element(ElementKind::Text);
    let dropped = fixture.element(ElementKind::Text);
    fixture.add(parent, &[kept, dropped]);
    fixture.layout(parent, 100.0, 100.0);
    // Parent, two children, the viewport.
    assert_eq!(fixture.tree.element_count(), 4);
    fixture.tree.release(dropped);
    assert_eq!(fixture.tree.element_count(), 4, "its parent holds it");
    fixture.tree.release(parent);
    assert_eq!(
        fixture.tree.element_count(),
        1,
        "the parent, its viewport and the handle-less child are gone"
    );
    fixture.tree.release(kept);
    assert_eq!(fixture.tree.element_count(), 0);
}

#[test]
fn an_element_is_in_one_place_and_never_inside_itself() {
    let mut fixture = Fixture::new();
    let (first, second) = (
        fixture.element(ElementKind::Column),
        fixture.element(ElementKind::Row),
    );
    let child = fixture.element(ElementKind::Text);
    fixture.add(first, &[child]);
    assert_eq!(
        fixture.tree.add_child(second, child),
        Err(UiError::AlreadyPlaced)
    );
    fixture.add(first, &[second]);
    assert_eq!(fixture.tree.add_child(second, first), Err(UiError::Cycle));
    assert_eq!(fixture.tree.add_child(first, first), Err(UiError::Cycle));
    assert_eq!(
        fixture.tree.add_child(child, second),
        Err(UiError::NotAParent(ElementKind::Text))
    );
}

#[test]
fn an_element_inside_another_is_not_a_root_and_a_laid_out_root_can_join_a_tree() {
    let mut fixture = Fixture::new();
    let (parent, child) = (
        fixture.element(ElementKind::Column),
        fixture.element(ElementKind::Column),
    );
    fixture.layout(child, 50.0, 50.0);
    fixture.add(parent, &[child]);
    assert_eq!(
        fixture
            .tree
            .layout(child, Size::new(10.0, 10.0), units(1.0), &mut fixture.text),
        Err(UiError::NotARoot)
    );
    // Its viewport went when it joined: the parent, the child, the parent's
    // viewport once laid out.
    fixture.layout(parent, 50.0, 50.0);
    assert_eq!(fixture.tree.element_count(), 3);
}

#[test]
fn a_property_of_another_kind_is_refused_by_name() {
    let mut fixture = Fixture::new();
    let row = fixture.element(ElementKind::Row);
    assert_eq!(
        fixture.tree.append_text(row, "x"),
        Err(UiError::WrongKind {
            property: "text",
            kind: ElementKind::Row
        })
    );
    assert!(fixture.tree.set_texture(row, texture(1, 1)).is_err());
    assert!(fixture.tree.set_fit(row, Fit::Cover).is_err());
    assert_eq!(
        fixture.tree.set_grow(row, f32::NAN),
        Err(UiError::NotFinite("a grow factor"))
    );
}

#[test]
fn the_role_and_label_are_carried() {
    let mut fixture = Fixture::new();
    let node = fixture.element(ElementKind::Row);
    assert_eq!(fixture.tree.role(node), Role::None);
    assert_eq!(fixture.tree.label(node), None);
    fixture.tree.set_role(node, Role::Button);
    fixture.tree.set_label(node, "Save");
    assert_eq!(fixture.tree.role(node), Role::Button);
    assert_eq!(fixture.tree.label(node), Some("Save"));
}

#[test]
fn the_outline_lists_every_element_in_paint_order_with_its_depth() {
    let mut fixture = Fixture::new();
    let root = fixture.element(ElementKind::Column);
    let (row, image) = (
        fixture.element(ElementKind::Row),
        fixture.element(ElementKind::Image),
    );
    let text = fixture.paragraph("a", 10.0);
    fixture.add(root, &[row, image]);
    fixture.add(row, &[text]);
    fixture.tree.set_label(image, "logo");
    fixture.layout(root, 100.0, 100.0);
    let outline: Vec<(usize, &str, Option<String>)> = fixture
        .tree
        .outline(root)
        .into_iter()
        .map(|entry| (entry.depth, entry.kind.name(), entry.label))
        .collect();
    assert_eq!(
        outline,
        vec![
            (0, "column", None),
            (1, "row", None),
            (2, "text", None),
            (1, "image", Some("logo".to_string())),
        ]
    );
}
