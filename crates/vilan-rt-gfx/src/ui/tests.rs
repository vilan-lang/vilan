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
    // Its one line, which answers the max-content width, the definite width
    // offered, and the final one. (No min-content width: the root's cell is
    // the viewport's fixed size, so nothing asks the root's smallest size.)
    assert_eq!(shapings(&fixture), 1);
    let mut builder = SceneBuilder::new();
    fixture
        .tree
        .paint(column, &mut builder, &mut fixture.text)
        .unwrap();
    assert_eq!(shapings(&fixture), 1, "painting reshapes nothing");
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

/// The commands a scene draws, nested scenes inlined and placed ones moved
/// into the window's coordinates: what the element scenes, each in its own
/// coordinates, add up to.
fn flatten(scene: &Scene) -> Vec<Command> {
    let mut commands = Vec::new();
    for command in scene.commands.iter() {
        match command {
            Command::Scene(nested) => commands.extend(flatten(nested)),
            Command::PlacedScene { scene, offset } => commands.extend(
                flatten(scene)
                    .into_iter()
                    .map(|command| moved_by(command, *offset)),
            ),
            other => commands.push(other.clone()),
        }
    }
    commands
}

/// `command` moved by `offset` dp. (No element scene holds a scaling
/// transform, under which a move would not commute with the transform.)
fn moved_by(command: Command, offset: Point) -> Command {
    let rect = |rect: Rect| {
        Rect::new(
            rect.left() + offset.x,
            rect.top() + offset.y,
            rect.size.width,
            rect.size.height,
        )
    };
    let point = |point: Point| Point::new(point.x + offset.x, point.y + offset.y);
    match command {
        Command::Rectangle(mut rectangle) => {
            rectangle.bounds = rect(rectangle.bounds);
            if let Fill::LinearGradient(gradient) = &mut rectangle.fill {
                gradient.start = point(gradient.start);
                gradient.end = point(gradient.end);
            }
            Command::Rectangle(rectangle)
        }
        Command::Shadow(mut shadow) => {
            shadow.bounds = rect(shadow.bounds);
            Command::Shadow(shadow)
        }
        Command::Text {
            layout,
            origin,
            color,
        } => Command::Text {
            layout,
            origin: point(origin),
            color,
        },
        Command::Underline(mut underline) => {
            underline.origin = point(underline.origin);
            Command::Underline(underline)
        }
        Command::Image {
            image,
            bounds,
            corner_radii,
        } => Command::Image {
            image,
            bounds: rect(bounds),
            corner_radii,
        },
        Command::Path {
            path,
            origin,
            scale,
            paint,
            color,
        } => Command::Path {
            path,
            origin: point(origin),
            scale,
            paint,
            color,
        },
        Command::PushClip {
            bounds,
            corner_radii,
        } => Command::PushClip {
            bounds: rect(bounds),
            corner_radii,
        },
        Command::PushTransform(transform) => {
            assert_eq!(transform.scale, 1.0, "an element scene only translates");
            Command::PushTransform(transform)
        }
        other @ (Command::PushLayer { .. } | Command::Pop) => other,
        Command::Scene(_) | Command::PlacedScene { .. } => {
            unreachable!("flatten inlines nested scenes first")
        }
    }
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
            [Command::PlacedScene { scene, .. }] => scene.clone(),
            other => panic!("one placed scene, not {other:?}"),
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

// ================================================================ part P3 ===

/// The commands a root paints, flattened, as text: two paints compare equal
/// exactly when they draw the same thing.
fn painted(fixture: &mut Fixture, root: NodeId) -> String {
    let mut builder = SceneBuilder::new();
    fixture
        .tree
        .paint(root, &mut builder, &mut fixture.text)
        .unwrap();
    let text = format!("{:?}", flatten(&builder.finish().unwrap()));
    // Which text system shaped a run is not what it draws.
    let mut kept = String::new();
    let mut rest = text.as_str();
    while let Some(at) = rest.find("system: ") {
        kept.push_str(&rest[..at]);
        rest = rest[at..].trim_start_matches("system: ");
        rest = rest.trim_start_matches(|character: char| character.is_ascii_digit());
    }
    kept.push_str(rest);
    kept
}

/// A small screen with boxes at fractional dp positions, which snap
/// differently at every scale.
fn fractional_screen(fixture: &mut Fixture) -> NodeId {
    let root = fixture.element(ElementKind::Column);
    fixture.tree.set_gap(root, 3.3).unwrap();
    fixture.tree.set_padding(root, Edges::all(5.5)).unwrap();
    for (index, words) in ["first line", "second", "third and last"]
        .iter()
        .enumerate()
    {
        let row = fixture.element(ElementKind::Row);
        fixture.tree.set_gap(row, 7.7).unwrap();
        let label = fixture.paragraph(words, 11.0 + index as f32);
        let mark = fixture.block(9.4, 6.2);
        fixture
            .tree
            .set_background(mark, Color::new(1.0, 0.0, 0.0, 1.0));
        fixture.add(row, &[label, mark]);
        fixture.add(root, &[row]);
    }
    root
}

#[test]
fn a_kept_scene_after_a_rescale_draws_what_a_fresh_paint_draws() {
    let mut kept = Fixture::new();
    let root = fractional_screen(&mut kept);
    kept.layout_at(root, 200.0, 120.0, 1.0);
    painted(&mut kept, root);
    kept.layout_at(root, 200.0, 120.0, 1.5);
    let after_rescale = painted(&mut kept, root);
    let mut fresh = Fixture::new();
    let fresh_root = fractional_screen(&mut fresh);
    fresh.layout_at(fresh_root, 200.0, 120.0, 1.5);
    assert_eq!(after_rescale, painted(&mut fresh, fresh_root));
}

// ------------------------------------------------- dirty marking scope ---

/// A column of `count` rows, each a row of a label and a value text: the
/// list's shape. Answers the column and each row's value text.
fn list(fixture: &mut Fixture, count: usize) -> (NodeId, Vec<NodeId>) {
    let column = fixture.element(ElementKind::Column);
    let mut values = Vec::new();
    for index in 0..count {
        let row = fixture.element(ElementKind::Row);
        fixture.tree.set_gap(row, 8.0).unwrap();
        let label = fixture.paragraph(&format!("Row {index}"), 10.0);
        fixture.tree.set_grow(label, 1.0).unwrap();
        let value = fixture.paragraph("000", 10.0);
        fixture.add(row, &[label, value]);
        fixture.add(column, &[row]);
        values.push(value);
    }
    (column, values)
}

fn paint_stats(fixture: &mut Fixture, root: NodeId) -> PassStats {
    let mut builder = SceneBuilder::new();
    fixture
        .tree
        .paint(root, &mut builder, &mut fixture.text)
        .unwrap();
    fixture.tree.last_pass()
}

#[test]
fn a_text_change_measures_that_text_alone_and_repaints_its_chain() {
    let mut fixture = Fixture::new();
    let (column, values) = list(&mut fixture, 50);
    fixture.layout(column, 300.0, 2000.0);
    paint_stats(&mut fixture, column);
    fixture.tree.set_run(values[20], 0, "999").unwrap();
    assert!(fixture.layout(column, 300.0, 2000.0), "the solver ran");
    let stats = paint_stats(&mut fixture, column);
    // The changed text (at the several widths its row asks), and nothing
    // else: none of the other 99 texts is measured again.
    assert_eq!(fixture.tree.last_measured(), [values[20]], "{stats:?}");
    assert_eq!(stats.moved, 0, "the same width: no box moved");
    // The text's scene, its row's, the column's: three of 151 elements.
    assert_eq!(stats.rebuilt, 3, "{stats:?}");
}

#[test]
fn a_paint_only_change_repaints_without_a_solve_and_only_its_chain() {
    let mut fixture = Fixture::new();
    let (column, values) = list(&mut fixture, 20);
    fixture.layout(column, 300.0, 1000.0);
    paint_stats(&mut fixture, column);
    let row = fixture.tree.children(column)[7];
    fixture
        .tree
        .set_background(row, Color::new(1.0, 0.0, 0.0, 1.0));
    assert!(!fixture.layout(column, 300.0, 1000.0), "no solve");
    assert_eq!(
        paint_stats(&mut fixture, column).rebuilt,
        2,
        "the row and the column"
    );
    // Nothing changed: the kept root scene, nothing rebuilt.
    assert!(!fixture.layout(column, 300.0, 1000.0));
    assert_eq!(paint_stats(&mut fixture, column).rebuilt, 0);
    // The same text again is no change at all.
    fixture.tree.set_run(values[3], 0, "000").unwrap();
    assert!(!fixture.layout(column, 300.0, 1000.0));
    assert_eq!(paint_stats(&mut fixture, column).rebuilt, 0);
}

#[test]
fn a_re_solve_after_a_resize_measures_only_what_changed() {
    let mut fixture = Fixture::new();
    let (column, values) = list(&mut fixture, 40);
    fixture.layout(column, 300.0, 1000.0);
    fixture.layout(column, 240.0, 1000.0);
    fixture.tree.set_run(values[10], 0, "998").unwrap();
    fixture.layout(column, 240.0, 1000.0);
    let stats = fixture.tree.last_pass();
    // The root's cell is the viewport's fixed size: nothing asks the root for
    // its content's smallest or largest size, so the resize leaves no
    // question unanswered for this pass (with an `auto` cell it measured all
    // 80 texts again): the changed row's two texts at most.
    let row = fixture.tree.children(column)[10];
    let measured = fixture.tree.last_measured();
    assert!(
        !measured.is_empty()
            && measured
                .iter()
                .all(|node| fixture.tree.children(row).contains(node)),
        "{measured:?} {stats:?}"
    );
}

/// Snapping after a solve visits only the solver's dirty chains, the
/// children they placed, and what moved: the boxes are those of a full snap
/// after every kind of change.
#[test]
fn snapping_only_what_the_solver_re_ran_gives_every_box_a_full_snap_gives() {
    let mut fixture = Fixture::new();
    let (column, values) = list(&mut fixture, 12);
    let boxes = |fixture: &Fixture| {
        fixture
            .tree
            .outline(column)
            .into_iter()
            .map(|entry| entry.bounds)
            .collect::<Vec<_>>()
    };
    fixture.layout_at(column, 300.0, 600.0, 1.25);
    let rows = fixture.tree.children(column);
    type Step<'step> = Box<dyn Fn(&mut Fixture) + 'step>;
    let steps: Vec<Step> = vec![
        Box::new(|fixture| {
            fixture
                .tree
                .set_run(values[2], 0, "a much longer value")
                .unwrap();
        }),
        Box::new(|fixture| {
            fixture.tree.set_padding(rows[5], Edges::all(9.5)).unwrap();
        }),
        Box::new(|fixture| {
            fixture.tree.set_gap(column, 3.3).unwrap();
        }),
        Box::new(|fixture| {
            fixture.tree.set_grow(values[8], 1.0).unwrap();
        }),
        Box::new(|fixture| {
            let added = fixture.paragraph("late", 13.0);
            fixture.add(rows[0], &[added]);
        }),
    ];
    for step in steps {
        step(&mut fixture);
        fixture.layout_at(column, 300.0, 600.0, 1.25);
        let incremental = boxes(&fixture);
        // A full snap: a new scale and back again snaps every box.
        fixture.layout_at(column, 300.0, 600.0, 2.0);
        fixture.layout_at(column, 300.0, 600.0, 1.25);
        assert_eq!(incremental, boxes(&fixture));
    }
}

// ------------------------------------------------------- hit testing ---

#[test]
fn hit_testing_is_back_to_front_and_honours_rounded_corners_and_clips() {
    let mut fixture = Fixture::new();
    let stack = fixture.element(ElementKind::Stack);
    let under = fixture.block(100.0, 100.0);
    let over = fixture.block(50.0, 50.0);
    let inner = fixture.block(20.0, 20.0);
    fixture.add(over, &[inner]);
    fixture.add(stack, &[under, over]);
    fixture.layout(stack, 200.0, 200.0);
    let hit = |fixture: &Fixture, x: f32, y: f32| fixture.tree.hit(stack, Point::new(x, y));
    assert_eq!(
        hit(&fixture, 10.0, 10.0),
        Some(inner),
        "the innermost, topmost"
    );
    assert_eq!(
        hit(&fixture, 40.0, 40.0),
        Some(over),
        "a later sibling is above"
    );
    assert_eq!(hit(&fixture, 80.0, 80.0), Some(under));
    assert_eq!(
        hit(&fixture, 150.0, 150.0),
        Some(stack),
        "the stack's own box"
    );
    assert_eq!(hit(&fixture, 250.0, 10.0), None);
    // A rounded corner is outside the element.
    fixture.tree.set_corners(under, Corners::all(30.0));
    assert_eq!(hit(&fixture, 98.0, 98.0), Some(stack));
    assert_eq!(hit(&fixture, 90.0, 60.0), Some(under));
    // A child overflowing its parent is hit outside it, unless the parent
    // clips.
    let wide = fixture.block(80.0, 10.0);
    let small = fixture.block(20.0, 20.0);
    fixture.add(small, &[wide]);
    let column = fixture.element(ElementKind::Column);
    fixture.tree.set_align(column, Align::Start).unwrap();
    fixture.add(column, &[small]);
    fixture.layout(column, 200.0, 200.0);
    assert_eq!(fixture.tree.hit(column, Point::new(60.0, 5.0)), Some(wide));
    fixture.tree.set_clip(small, true);
    fixture.layout(column, 200.0, 200.0);
    assert_eq!(
        fixture.tree.hit(column, Point::new(60.0, 5.0)),
        Some(column)
    );
    assert_eq!(fixture.tree.hit(column, Point::new(10.0, 5.0)), Some(wide));
}

// ------------------------------------------------------- routing ---

/// A log of the handlers called, and a way to add one that writes it.
struct Recorder {
    log: Rc<std::cell::RefCell<Vec<String>>>,
}

impl Recorder {
    fn new() -> Recorder {
        Recorder {
            log: Rc::new(std::cell::RefCell::new(Vec::new())),
        }
    }

    fn listen(&self, fixture: &mut Fixture, node: NodeId, kind: EventKind, name: &str) {
        let log = Rc::clone(&self.log);
        let line = format!("{kind:?} {name}");
        fixture.tree.add_handler(
            node,
            kind,
            Handler::new(move || log.borrow_mut().push(line.clone())),
        );
    }

    /// Routes `input` and calls every handler, as the host does when nothing
    /// stops the event; answers the log and empties it.
    fn route(&self, fixture: &mut Fixture, root: NodeId, input: Input) -> Vec<String> {
        let route = fixture.tree.route(root, &input);
        self.run(fixture, root, route);
        std::mem::take(&mut *self.log.borrow_mut())
    }

    fn run(&self, fixture: &mut Fixture, root: NodeId, route: Route) {
        for step in route.steps {
            for delivery in &step.deliveries {
                delivery.handler.call();
            }
            if let Some(action) = step.default {
                let more = fixture.tree.perform(root, action);
                self.run(fixture, root, more);
            }
        }
    }
}

/// A press at `position` at time 0: presses at one place count up, which
/// changes nothing a pin without a count reads.
fn press(position: Point) -> Input {
    press_at(position, 0)
}

/// A press at `position`, `milliseconds` after the clock's origin.
fn press_at(position: Point, milliseconds: u64) -> Input {
    Input::Pressed {
        position,
        time: std::time::Duration::from_millis(milliseconds),
    }
}

/// A root column holding a card holding a button, side by side with a
/// second button: `(root, card, first, second)`.
fn buttons(fixture: &mut Fixture) -> (NodeId, NodeId, NodeId, NodeId) {
    let root = fixture.element(ElementKind::Row);
    fixture.tree.set_align(root, Align::Start).unwrap();
    let card = fixture.element(ElementKind::Column);
    fixture.tree.set_padding(card, Edges::all(10.0)).unwrap();
    let first = fixture.block(40.0, 20.0);
    let second = fixture.block(40.0, 20.0);
    fixture.add(card, &[first]);
    fixture.add(root, &[card, second]);
    fixture.layout(root, 300.0, 100.0);
    (root, card, first, second)
}

#[test]
fn an_event_reaches_its_target_first_then_each_listening_ancestor() {
    let mut fixture = Fixture::new();
    let (root, card, first, second) = buttons(&mut fixture);
    let recorder = Recorder::new();
    for (node, name) in [(root, "root"), (card, "card"), (first, "first")] {
        recorder.listen(&mut fixture, node, EventKind::Press, name);
        recorder.listen(&mut fixture, node, EventKind::Click, name);
    }
    recorder.listen(&mut fixture, second, EventKind::Click, "second");
    // `first` is at (10, 10) in the card at (0, 0).
    let at = Point::new(20.0, 20.0);
    assert_eq!(
        recorder.route(&mut fixture, root, press(at)),
        ["Press first", "Press card", "Press root"]
    );
    assert_eq!(
        recorder.route(&mut fixture, root, Input::Released(at)),
        ["Click first", "Click card", "Click root"]
    );
    // Pressed on `first`, released on `second`: the click goes to the
    // innermost element holding both, the root.
    recorder.route(&mut fixture, root, press(at));
    let second_at = Point::new(fixture.rect(second).0 + 5.0, 5.0);
    assert_eq!(
        recorder.route(&mut fixture, root, Input::Released(second_at)),
        ["Click root"]
    );
}

#[test]
fn a_stopped_event_goes_no_further_and_its_default_does_not_happen() {
    let mut fixture = Fixture::new();
    let (root, card, first, _) = buttons(&mut fixture);
    fixture.tree.set_tab_index(first, Some(0));
    let route = fixture.tree.route(
        root,
        &Input::Key {
            name: "Tab".to_string(),
            shift: false,
        },
    );
    // Nothing is focused: the root is the target, and the default moves the
    // focus. A stop is the host's (`host::ui`'s `run_route`), which skips
    // the rest of a stoppable step and its default.
    assert_eq!(route.steps.len(), 1);
    assert!(route.steps[0].stoppable);
    assert_eq!(
        route.steps[0].default,
        Some(DefaultAction::MoveFocus { backward: false })
    );
    let _ = card;
}

#[test]
fn the_pointer_entering_and_leaving_is_told_to_each_element_once() {
    let mut fixture = Fixture::new();
    let (root, card, first, second) = buttons(&mut fixture);
    let recorder = Recorder::new();
    for (node, name) in [
        (root, "root"),
        (card, "card"),
        (first, "first"),
        (second, "second"),
    ] {
        recorder.listen(&mut fixture, node, EventKind::PointerEnter, name);
        recorder.listen(&mut fixture, node, EventKind::PointerLeave, name);
    }
    let on_first = Point::new(20.0, 20.0);
    assert_eq!(
        recorder.route(&mut fixture, root, Input::PointerMoved(on_first)),
        [
            "PointerEnter root",
            "PointerEnter card",
            "PointerEnter first"
        ],
        "outermost first"
    );
    assert_eq!(
        recorder.route(
            &mut fixture,
            root,
            Input::PointerMoved(Point::new(21.0, 20.0))
        ),
        Vec::<String>::new(),
        "a move inside tells nobody"
    );
    let on_second = Point::new(fixture.rect(second).0 + 5.0, 5.0);
    assert_eq!(
        recorder.route(&mut fixture, root, Input::PointerMoved(on_second)),
        [
            "PointerLeave first",
            "PointerLeave card",
            "PointerEnter second"
        ],
        "deepest left first; the root is still under the pointer"
    );
    assert_eq!(
        recorder.route(&mut fixture, root, Input::PointerLeft),
        ["PointerLeave second", "PointerLeave root"]
    );
}

// ------------------------------------------------------- conditions ---

fn red() -> Color {
    Color::new(1.0, 0.0, 0.0, 1.0)
}

fn blue() -> Color {
    Color::new(0.0, 0.0, 1.0, 1.0)
}

/// A conditional layer on `node` setting the background to `color`.
fn when(fixture: &mut Fixture, node: NodeId, condition: Condition, color: Color) {
    let group = fixture.tree.style_group(node, None);
    let layer = fixture
        .tree
        .style_layer(node, group, Some(condition))
        .unwrap();
    fixture
        .tree
        .set_style_fields(node, layer, |fields| fields.background = Some(color));
    fixture.tree.restyle(node);
}

#[test]
fn each_condition_applies_while_it_holds_and_repaints_its_element_alone() {
    let mut fixture = Fixture::new();
    let (root, card, first, second) = buttons(&mut fixture);
    fixture.tree.set_tab_index(first, Some(0));
    fixture.tree.set_background(first, blue());
    let hovered = Condition {
        hovered: true,
        ..Condition::default()
    };
    let pressed = Condition {
        pressed: true,
        ..Condition::default()
    };
    let focused = Condition {
        focused: true,
        ..Condition::default()
    };
    when(&mut fixture, first, hovered, red());
    when(&mut fixture, second, pressed, red());
    when(&mut fixture, card, focused, red());
    fixture.layout(root, 300.0, 100.0);
    paint_stats(&mut fixture, root);
    let background = |fixture: &Fixture, node| fixture.tree.paint_of(node).background;

    // Hovered: the pointer over `first`.
    fixture
        .tree
        .route(root, &Input::PointerMoved(Point::new(20.0, 20.0)));
    assert_eq!(background(&fixture, first), Some(red()));
    assert!(!fixture.layout(root, 300.0, 100.0), "a look: no solve");
    assert_eq!(
        paint_stats(&mut fixture, root).rebuilt,
        3,
        "first, card, root"
    );
    fixture
        .tree
        .route(root, &Input::PointerMoved(Point::new(200.0, 90.0)));
    assert_eq!(background(&fixture, first), Some(blue()), "the base again");

    // Pressed: held while the button is down, wherever the pointer goes.
    let on_second = Point::new(fixture.rect(second).0 + 5.0, 5.0);
    fixture.tree.route(root, &press(on_second));
    assert_eq!(background(&fixture, second), Some(red()));
    fixture
        .tree
        .route(root, &Input::PointerMoved(Point::new(200.0, 90.0)));
    assert_eq!(background(&fixture, second), Some(red()), "still down");
    fixture
        .tree
        .route(root, &Input::Released(Point::new(200.0, 90.0)));
    assert_eq!(background(&fixture, second), None);

    // Focused: on the element that holds the focus only — not its parent.
    fixture.tree.focus(Some(first));
    assert_eq!(background(&fixture, card), None, "the card is not focused");
    fixture.tree.focus(Some(card));
    assert_eq!(background(&fixture, card), Some(red()));
}

#[test]
fn a_window_width_condition_follows_the_layout_width() {
    let mut fixture = Fixture::new();
    let root = fixture.element(ElementKind::Column);
    fixture.tree.set_align(root, Align::Start).unwrap();
    let text = fixture.paragraph("abc", 10.0);
    fixture.add(root, &[text]);
    let group = fixture.tree.style_group(text, None);
    let narrow = fixture
        .tree
        .style_layer(
            text,
            group,
            Some(Condition {
                window_below: Some(200.0),
                ..Condition::default()
            }),
        )
        .unwrap();
    fixture
        .tree
        .set_style_fields(text, narrow, |fields| fields.text.size = Some(20.0));
    fixture.tree.restyle(text);
    fixture.layout(root, 300.0, 100.0);
    assert_eq!(fixture.rect(text).2.round(), 18.0, "three 6 dp advances");
    // A text size is a shaping field: the narrow window lays it out again.
    assert!(fixture.layout(root, 150.0, 100.0));
    assert_eq!(fixture.rect(text).2.round(), 36.0);
    assert!(fixture.layout(root, 300.0, 100.0));
    assert_eq!(fixture.rect(text).2.round(), 18.0);
}

#[test]
fn a_conditions_fields_win_over_the_unconditional_ones_in_any_order() {
    let mut fixture = Fixture::new();
    let node = fixture.block(10.0, 10.0);
    let hovered = Condition {
        hovered: true,
        ..Condition::default()
    };
    // The condition is written first, an unconditional background after it
    // (a later `style` call: a later group).
    when(&mut fixture, node, hovered, red());
    let later = fixture.tree.style_group(node, None);
    let layer = fixture.tree.style_layer(node, later, None).unwrap();
    fixture
        .tree
        .set_style_fields(node, layer, |fields| fields.background = Some(blue()));
    fixture.tree.restyle(node);
    fixture.layout(node, 50.0, 50.0);
    fixture
        .tree
        .route(node, &Input::PointerMoved(Point::new(5.0, 5.0)));
    assert_eq!(fixture.tree.paint_of(node).background, Some(red()));
}

#[test]
fn a_style_group_written_again_replaces_only_its_own_layers() {
    let mut fixture = Fixture::new();
    let node = fixture.block(10.0, 10.0);
    let kept = fixture.tree.style_group(node, None);
    let layer = fixture.tree.style_layer(node, kept, None).unwrap();
    fixture.tree.set_style_fields(node, layer, |fields| {
        fields.corners = Some(Corners::all(3.0))
    });
    let bound = fixture.tree.style_group(node, None);
    let layer = fixture.tree.style_layer(node, bound, None).unwrap();
    fixture
        .tree
        .set_style_fields(node, layer, |fields| fields.background = Some(red()));
    fixture.tree.restyle(node);
    // The bound style writes a value without a background.
    fixture.tree.style_group(node, Some(bound));
    let layer = fixture.tree.style_layer(node, bound, None).unwrap();
    fixture
        .tree
        .set_style_fields(node, layer, |fields| fields.opacity = Some(0.5));
    fixture.tree.restyle(node);
    let paint = fixture.tree.paint_of(node);
    assert_eq!(paint.background, None, "the old value's background went");
    assert_eq!(paint.opacity, 0.5);
    assert_eq!(paint.corners, Corners::all(3.0), "the other group stays");
}

// ------------------------------------------------------------ focus ---

#[test]
fn the_tab_order_is_positive_indices_first_then_tree_order_and_wraps() {
    let mut fixture = Fixture::new();
    let root = fixture.element(ElementKind::Column);
    let nodes: Vec<NodeId> = (0..5).map(|_| fixture.block(10.0, 10.0)).collect();
    fixture.add(root, &nodes);
    fixture.tree.set_tab_index(nodes[0], Some(0));
    fixture.tree.set_tab_index(nodes[1], Some(2));
    fixture.tree.set_tab_index(nodes[2], Some(-1));
    fixture.tree.set_tab_index(nodes[3], Some(1));
    fixture.tree.set_tab_index(nodes[4], Some(0));
    assert_eq!(
        fixture.tree.tab_order(root),
        [nodes[3], nodes[1], nodes[0], nodes[4]],
        "1, 2, then the zeros in tree order; -1 is not reached"
    );
    fixture.layout(root, 100.0, 100.0);
    let recorder = Recorder::new();
    let tab = |shift| Input::Key {
        name: "Tab".to_string(),
        shift,
    };
    recorder.route(&mut fixture, root, tab(false));
    assert_eq!(fixture.tree.focused(), Some(nodes[3]), "the first");
    recorder.route(&mut fixture, root, tab(true));
    assert_eq!(
        fixture.tree.focused(),
        Some(nodes[4]),
        "back wraps to the last"
    );
    recorder.route(&mut fixture, root, tab(false));
    assert_eq!(fixture.tree.focused(), Some(nodes[3]), "forward wraps");
    // A focused element with -1 is outside the order: Tab starts again.
    fixture.tree.focus(Some(nodes[2]));
    recorder.route(&mut fixture, root, tab(false));
    assert_eq!(fixture.tree.focused(), Some(nodes[3]));
}

#[test]
fn a_press_focuses_the_nearest_focusable_and_keys_go_to_the_focus() {
    let mut fixture = Fixture::new();
    let (root, card, first, second) = buttons(&mut fixture);
    fixture.tree.set_tab_index(card, Some(0));
    let recorder = Recorder::new();
    recorder.listen(&mut fixture, card, EventKind::Focus, "card");
    recorder.listen(&mut fixture, card, EventKind::Blur, "card");
    recorder.listen(&mut fixture, card, EventKind::Key, "card");
    recorder.listen(&mut fixture, root, EventKind::Key, "root");
    // `first` is not focusable; its parent the card is.
    assert_eq!(
        recorder.route(&mut fixture, root, press(Point::new(20.0, 20.0))),
        ["Focus card"]
    );
    assert_eq!(fixture.tree.focused(), Some(card));
    assert!(fixture.tree.interaction(card).focused);
    assert_eq!(
        recorder.route(
            &mut fixture,
            root,
            Input::Key {
                name: "a".to_string(),
                shift: false
            }
        ),
        ["Key card", "Key root"],
        "to the focus, then up"
    );
    // A press on nothing focusable takes the focus away.
    let on_second = Point::new(fixture.rect(second).0 + 5.0, 5.0);
    assert_eq!(
        recorder.route(&mut fixture, root, press(on_second)),
        ["Blur card"]
    );
    assert_eq!(fixture.tree.focused(), None);
    assert_eq!(
        recorder.route(
            &mut fixture,
            root,
            Input::Key {
                name: "a".to_string(),
                shift: false
            }
        ),
        ["Key root"],
        "no focus: the root"
    );
    let _ = first;
}

// ------------------------------------------------------------ scroll ---

/// A 100 x 100 scroll over a column of ten 50 dp rows: `(scroll, rows)`.
fn scrolled_list(fixture: &mut Fixture) -> (NodeId, Vec<NodeId>) {
    let scroll = fixture.element(ElementKind::Scroll);
    fixture
        .tree
        .set_size(scroll, SizeProperty::Width, Length::Dp(100.0))
        .unwrap();
    fixture
        .tree
        .set_size(scroll, SizeProperty::Height, Length::Dp(100.0))
        .unwrap();
    let column = fixture.element(ElementKind::Column);
    let rows: Vec<NodeId> = (0..10).map(|_| fixture.block(100.0, 50.0)).collect();
    fixture.add(column, &rows);
    fixture.add(scroll, &[column]);
    let root = fixture.element(ElementKind::Stack);
    fixture.add(root, &[scroll]);
    fixture.layout(root, 300.0, 300.0);
    (scroll, rows)
}

#[test]
fn a_scroll_clips_its_child_and_moves_it_by_its_offset_without_a_layout() {
    let mut fixture = Fixture::new();
    let (scroll, rows) = scrolled_list(&mut fixture);
    let root = fixture.tree.root_of(scroll);
    assert_eq!(
        fixture.rect(rows[9]),
        (0.0, 450.0, 100.0, 50.0),
        "laid out whole"
    );
    assert_eq!(
        fixture.tree.scroll_range(scroll),
        Some(Size::new(0.0, 400.0))
    );
    paint_stats(&mut fixture, root);
    fixture
        .tree
        .set_scroll_offset(scroll, Point::new(0.0, 120.0))
        .unwrap();
    assert!(!fixture.layout(root, 300.0, 300.0), "no solve");
    assert_eq!(
        paint_stats(&mut fixture, root).rebuilt,
        2,
        "the scroll and the root"
    );
    let mut builder = SceneBuilder::new();
    fixture
        .tree
        .paint(root, &mut builder, &mut fixture.text)
        .unwrap();
    let commands = flatten(&builder.finish().unwrap());
    let clip = commands.iter().position(|command| {
        matches!(command, Command::PushClip { bounds, .. } if *bounds == Rect::new(0.0, 0.0, 100.0, 100.0))
    });
    let moved = commands.iter().position(|command| {
        matches!(command, Command::PushTransform(transform) if *transform == Transform::translate(0.0, -120.0))
    });
    assert!(
        clip.is_some() && moved > clip,
        "clipped, then moved: {commands:?}"
    );
    // Clamped to the range.
    fixture
        .tree
        .set_scroll_offset(scroll, Point::new(30.0, 900.0))
        .unwrap();
    assert_eq!(
        fixture.tree.scroll_offset(scroll),
        Some(Point::new(0.0, 400.0)),
        "a vertical scroll does not move across, and stops at its range"
    );
}

#[test]
fn hits_inside_a_scroll_follow_its_offset_and_stop_at_its_edge() {
    let mut fixture = Fixture::new();
    let (scroll, rows) = scrolled_list(&mut fixture);
    let root = fixture.tree.root_of(scroll);
    assert_eq!(
        fixture.tree.hit(root, Point::new(10.0, 60.0)),
        Some(rows[1])
    );
    fixture
        .tree
        .set_scroll_offset(scroll, Point::new(0.0, 120.0))
        .unwrap();
    // Content y 180: the fourth row.
    assert_eq!(
        fixture.tree.hit(root, Point::new(10.0, 60.0)),
        Some(rows[3])
    );
    // Below the viewport, the rows laid out there are clipped away.
    assert_eq!(fixture.tree.hit(root, Point::new(10.0, 130.0)), Some(root));
}

#[test]
fn the_wheel_scrolls_the_innermost_scroll_that_can_move() {
    let mut fixture = Fixture::new();
    let (scroll, _) = scrolled_list(&mut fixture);
    let root = fixture.tree.root_of(scroll);
    let recorder = Recorder::new();
    recorder.listen(&mut fixture, scroll, EventKind::Scroll, "scroll");
    recorder.listen(&mut fixture, root, EventKind::Scroll, "root");
    let down = |amount: f32| Input::Scrolled {
        position: Point::new(10.0, 10.0),
        delta: Point::new(0.0, -amount),
    };
    // It moves: the scroll takes the wheel, the root never hears it.
    assert_eq!(
        recorder.route(&mut fixture, root, down(64.0)),
        ["Scroll scroll"]
    );
    assert_eq!(
        fixture.tree.scroll_offset(scroll),
        Some(Point::new(0.0, 64.0))
    );
    // At the end it cannot move: the wheel bubbles on.
    fixture
        .tree
        .set_scroll_offset(scroll, Point::new(0.0, 400.0))
        .unwrap();
    assert_eq!(
        recorder.route(&mut fixture, root, down(64.0)),
        ["Scroll scroll", "Scroll root"]
    );
    // A wheel move tells the program, once.
    assert_eq!(
        fixture.tree.take_offset_notices().len(),
        0,
        "a set is not told"
    );
    recorder.route(
        &mut fixture,
        root,
        Input::Scrolled {
            position: Point::new(10.0, 10.0),
            delta: Point::new(0.0, 32.0),
        },
    );
    assert_eq!(
        fixture.tree.take_offset_notices().len(),
        0,
        "no handler set"
    );
}

/// A `width` x `height` scroll along `axis` over `child`, in a stack laid
/// out at 300 x 300: the scroll.
fn scroll_over(
    fixture: &mut Fixture,
    axis: ScrollAxis,
    width: f32,
    height: f32,
    child: NodeId,
) -> NodeId {
    let scroll = fixture.element(ElementKind::Scroll);
    fixture.tree.set_scroll_axis(scroll, axis).unwrap();
    fixture
        .tree
        .set_size(scroll, SizeProperty::Width, Length::Dp(width))
        .unwrap();
    fixture
        .tree
        .set_size(scroll, SizeProperty::Height, Length::Dp(height))
        .unwrap();
    fixture.add(scroll, &[child]);
    let root = fixture.element(ElementKind::Stack);
    fixture.add(root, &[scroll]);
    fixture.layout(root, 300.0, 300.0);
    scroll
}

/// F177 (RULED 2026-11-12): a scroll's one child never shrinks along the axis
/// the scroll moves it on, whatever shrink factor the program gave it. A
/// child whose size is set is the case flexbox shrinks by default: its
/// automatic minimum is the smaller of that size and its content's (here
/// none), so without the rule it is squeezed into the viewport.
#[test]
fn a_scrolls_child_keeps_its_size_along_each_axis_the_scroll_moves() {
    // Vertical: as tall as it asks; across, stretched to the viewport.
    let mut fixture = Fixture::new();
    let tall = fixture.element(ElementKind::Column);
    fixture
        .tree
        .set_size(tall, SizeProperty::Height, Length::Dp(500.0))
        .unwrap();
    let scroll = scroll_over(&mut fixture, ScrollAxis::Vertical, 100.0, 100.0, tall);
    assert_eq!(fixture.rect(tall), (0.0, 0.0, 100.0, 500.0));
    assert_eq!(
        fixture.tree.scroll_range(scroll),
        Some(Size::new(0.0, 400.0))
    );

    // Horizontal: as wide as it asks; across, stretched to the viewport.
    let mut fixture = Fixture::new();
    let wide = fixture.element(ElementKind::Column);
    fixture
        .tree
        .set_size(wide, SizeProperty::Width, Length::Dp(300.0))
        .unwrap();
    let scroll = scroll_over(&mut fixture, ScrollAxis::Horizontal, 100.0, 80.0, wide);
    assert_eq!(fixture.rect(wide), (0.0, 0.0, 300.0, 80.0));
    assert_eq!(
        fixture.tree.scroll_range(scroll),
        Some(Size::new(200.0, 0.0))
    );

    // Both: its own size on both axes.
    let mut fixture = Fixture::new();
    let large = fixture.block(300.0, 500.0);
    let scroll = scroll_over(&mut fixture, ScrollAxis::Both, 100.0, 100.0, large);
    assert_eq!(fixture.rect(large), (0.0, 0.0, 300.0, 500.0));
    assert_eq!(
        fixture.tree.scroll_range(scroll),
        Some(Size::new(200.0, 400.0))
    );
}

#[test]
fn a_scrolls_child_shrinks_by_no_factor_the_program_gives_it() {
    let mut fixture = Fixture::new();
    let tall = fixture.element(ElementKind::Column);
    fixture
        .tree
        .set_size(tall, SizeProperty::Height, Length::Dp(500.0))
        .unwrap();
    fixture.tree.set_shrink(tall, 3.0).unwrap();
    let scroll = scroll_over(&mut fixture, ScrollAxis::Vertical, 100.0, 100.0, tall);
    assert_eq!(fixture.rect(tall).3, 500.0, "set before it was placed");
    fixture.tree.set_shrink(tall, 1.0).unwrap();
    fixture.layout(fixture.tree.root_of(scroll), 300.0, 300.0);
    assert_eq!(fixture.rect(tall).3, 500.0, "set while it is placed");
    // Removed, it is the program's factor again: in a row it shrinks.
    let removed = fixture.tree.remove(tall);
    drop(removed);
    let row = row(&mut fixture, 100.0, 600.0);
    fixture
        .tree
        .set_size(tall, SizeProperty::Width, Length::Dp(300.0))
        .unwrap();
    fixture.add(row, &[tall]);
    fixture.layout(row, 100.0, 600.0);
    assert_eq!(
        fixture.rect(tall).2,
        100.0,
        "a row's child shrinks as before"
    );
}

/// The rule is along the scroll's axis only: across it a child still takes
/// the viewport's size, so a paragraph in a vertical scroll wraps at its
/// width, and one in a horizontal scroll keeps its one line.
#[test]
fn across_its_axis_a_scrolls_child_still_fits_the_viewport() {
    let mut fixture = Fixture::new();
    // Twenty-four characters at 10 dp: 144 dp on one line.
    let paragraph = fixture.paragraph("aaaa bbbb cccc dddd eeee", 10.0);
    scroll_over(&mut fixture, ScrollAxis::Vertical, 100.0, 100.0, paragraph);
    let (_, _, width, height) = fixture.rect(paragraph);
    assert_eq!(width, 100.0, "stretched across");
    assert!(height > 20.0, "wrapped at the viewport's width: {height}");

    let mut fixture = Fixture::new();
    let paragraph = fixture.paragraph("aaaa bbbb cccc dddd eeee", 10.0);
    let scroll = scroll_over(
        &mut fixture,
        ScrollAxis::Horizontal,
        100.0,
        100.0,
        paragraph,
    );
    let (_, _, width, height) = fixture.rect(paragraph);
    assert_eq!(
        (width, height),
        (144.0, 100.0),
        "one line, stretched across"
    );
    assert_eq!(
        fixture.tree.scroll_range(scroll),
        Some(Size::new(44.0, 0.0))
    );

    // Both: the paragraph keeps its one line, and its own height.
    let mut fixture = Fixture::new();
    let paragraph = fixture.paragraph("aaaa bbbb cccc dddd eeee", 10.0);
    scroll_over(&mut fixture, ScrollAxis::Both, 100.0, 100.0, paragraph);
    let (_, _, width, height) = fixture.rect(paragraph);
    assert_eq!(width, 144.0, "one line");
    assert!(height < 20.0, "its own height: {height}");
}

// ------------------------------------------- inserting, removing, moving ---

/// A row of the list's shape: a label and a value, as `list` builds them.
fn list_row(fixture: &mut Fixture, label: &str) -> NodeId {
    let row = fixture.element(ElementKind::Row);
    fixture.tree.set_gap(row, 8.0).unwrap();
    let name = fixture.paragraph(label, 10.0);
    fixture.tree.set_grow(name, 1.0).unwrap();
    let value = fixture.paragraph("000", 10.0);
    fixture.add(row, &[name, value]);
    row
}

/// The boxes of every element under `root`, by a FULL snap: a new scale and
/// back again snaps every box (`snapping_only_what_the_solver_re_ran_..`).
fn full_snap_boxes(fixture: &mut Fixture, root: NodeId) -> Vec<Rect> {
    let boxes = |fixture: &Fixture| {
        fixture
            .tree
            .outline(root)
            .into_iter()
            .map(|entry| entry.bounds)
            .collect::<Vec<_>>()
    };
    let incremental = boxes(fixture);
    fixture.layout_at(root, 300.0, 2000.0, 2.0);
    fixture.layout_at(root, 300.0, 2000.0, 1.0);
    assert_eq!(incremental, boxes(fixture), "the incremental snap");
    incremental
}

/// R9: a row inserted at the top tells the solver its PARENT changed and
/// nothing else — the solver measures the newcomer's texts alone and places
/// the other rows from its cache — and every box is the one a full snap
/// gives.
#[test]
fn an_inserted_row_is_measured_alone_and_the_rows_below_it_move() {
    let mut fixture = Fixture::new();
    let (column, _) = list(&mut fixture, 50);
    fixture.layout(column, 300.0, 2000.0);
    paint_stats(&mut fixture, column);
    let rows = fixture.tree.children(column);
    let added = list_row(&mut fixture, "Row new");
    fixture.tree.insert_before(rows[0], added).unwrap();
    assert!(fixture.layout(column, 300.0, 2000.0), "the solver ran");
    let texts = fixture.tree.children(added);
    let measured = fixture.tree.last_measured();
    assert!(
        !measured.is_empty() && measured.iter().all(|node| texts.contains(node)),
        "only the newcomer's texts: {measured:?}"
    );
    assert_eq!(fixture.tree.children(column)[0], added);
    assert_eq!(fixture.tree.children(column)[1..], rows[..]);
    assert_eq!(fixture.rect(added).1, 0.0);
    assert_eq!(fixture.rect(rows[0]).1, fixture.rect(added).3);
    full_snap_boxes(&mut fixture, column);
}

#[test]
fn a_removed_row_is_measured_by_nobody_and_its_place_closes() {
    let mut fixture = Fixture::new();
    let (column, _) = list(&mut fixture, 30);
    fixture.layout(column, 300.0, 2000.0);
    let rows = fixture.tree.children(column);
    let gone_at = fixture.rect(rows[10]).1;
    let handlers = fixture.tree.remove(rows[10]);
    assert!(handlers.is_empty());
    assert!(fixture.layout(column, 300.0, 2000.0), "the solver ran");
    assert!(fixture.tree.last_measured().is_empty(), "nothing measured");
    assert_eq!(fixture.tree.children(column).len(), 29);
    assert_eq!(fixture.rect(rows[11]).1, gone_at, "the next row closed up");
    // The tree's own elements have no handle to wait for (a host element
    // does: `host::ui`'s `Node`), so the removed row is still alive until
    // its handle goes — then freed, with its texts.
    assert!(fixture.tree.is_alive(rows[10]));
    let texts = fixture.tree.children(rows[10]);
    for text in &texts {
        fixture.tree.release(*text);
    }
    fixture.tree.release(rows[10]);
    assert!(!fixture.tree.is_alive(rows[10]));
    assert!(texts.iter().all(|text| !fixture.tree.is_alive(*text)));
    full_snap_boxes(&mut fixture, column);
}

/// A move is one element of the parent's children changing place: its
/// state goes with it — the pointer's hover and press, the focus, its cached
/// layout (nothing is measured) — and nothing else changes.
#[test]
fn a_moved_row_keeps_its_hover_press_and_focus_and_measures_nothing() {
    let mut fixture = Fixture::new();
    let (column, _) = list(&mut fixture, 20);
    fixture.layout(column, 300.0, 2000.0);
    let rows = fixture.tree.children(column);
    let moving = rows[5];
    fixture.tree.set_tab_index(moving, Some(0));
    let (x, y, width, height) = fixture.rect(moving);
    let over = Point::new(x + width / 2.0, y + height / 2.0);
    fixture.tree.route(column, &press(over));
    let before = fixture.tree.interaction(moving);
    assert!(
        before.hovered && before.pressed && before.focused,
        "{before:?}"
    );
    fixture.tree.insert_before(rows[0], moving).unwrap();
    assert_eq!(fixture.tree.children(column)[0], moving);
    assert_eq!(fixture.tree.interaction(moving), before);
    let pressed = fixture.tree.pressed();
    assert!(pressed.is_some());
    assert_eq!(fixture.tree.focused(), Some(moving));
    assert!(fixture.layout(column, 300.0, 2000.0), "the solver ran");
    assert!(fixture.tree.last_measured().is_empty(), "nothing measured");
    assert_eq!(fixture.rect(moving).1, 0.0, "it is first");
    // Moving it back in front of its old successor restores every box.
    let original_next = rows[6];
    fixture.tree.insert_before(original_next, moving).unwrap();
    assert_eq!(fixture.tree.children(column), rows);
    fixture.layout(column, 300.0, 2000.0);
    assert_eq!(fixture.rect(moving), (x, y, width, height));
    // The release over the moved row is still a click on it.
    let recorder = Recorder::new();
    recorder.listen(&mut fixture, moving, EventKind::Click, "moved");
    assert_eq!(
        recorder.route(&mut fixture, column, Input::Released(over)),
        ["Click moved"]
    );
}

/// Removal ends the element's interaction: the hover inside it rests on its
/// parent, a press inside it is cancelled (no click follows the release),
/// and the focus inside it is taken away with no blur — its handlers are the
/// caller's to drop, every one of the subtree's, the scroll's offset handler
/// included.
#[test]
fn a_removed_row_takes_its_hover_press_focus_and_handlers_with_it() {
    let mut fixture = Fixture::new();
    let (column, _) = list(&mut fixture, 10);
    fixture.layout(column, 300.0, 2000.0);
    let rows = fixture.tree.children(column);
    let going = rows[3];
    let text = fixture.tree.children(going)[0];
    fixture.tree.set_tab_index(going, Some(0));
    let recorder = Recorder::new();
    recorder.listen(&mut fixture, going, EventKind::Blur, "blur going");
    recorder.listen(&mut fixture, text, EventKind::Click, "click text");
    recorder.listen(&mut fixture, column, EventKind::Click, "click column");
    let (x, y, width, height) = fixture.rect(going);
    let over = Point::new(x + width / 2.0, y + height / 2.0);
    recorder.route(&mut fixture, column, press(over));
    assert_eq!(fixture.tree.focused(), Some(going));
    let handlers = fixture.tree.remove(going);
    assert_eq!(handlers.len(), 2, "the row's blur and its text's click");
    assert_eq!(fixture.tree.focused(), None);
    assert_eq!(fixture.tree.pressed(), None);
    assert_eq!(fixture.tree.hovered(), Some(column));
    assert_eq!(fixture.tree.interaction(going), Interaction::default());
    assert!(fixture.tree.interaction(column).hovered);
    assert!(
        !fixture.tree.interaction(column).pressed,
        "the press is over"
    );
    fixture.layout(column, 300.0, 2000.0);
    // The release lands on whatever row closed up under the pointer: there
    // was no press to finish, so nobody hears a click — and no blur ran.
    assert_eq!(
        recorder.route(&mut fixture, column, Input::Released(over)),
        Vec::<String>::new()
    );
}

#[test]
fn placing_before_an_element_needs_a_parent_and_the_one_place_rule_holds() {
    let mut fixture = Fixture::new();
    let column = fixture.element(ElementKind::Column);
    let first = fixture.block(10.0, 10.0);
    let alone = fixture.block(10.0, 10.0);
    let newcomer = fixture.block(10.0, 10.0);
    fixture.add(column, &[first]);
    assert_eq!(
        fixture.tree.insert_before(alone, newcomer),
        Err(UiError::NoPosition)
    );
    let elsewhere = fixture.element(ElementKind::Row);
    let placed = fixture.block(10.0, 10.0);
    fixture.add(elsewhere, &[placed]);
    assert_eq!(
        fixture.tree.insert_before(first, placed),
        Err(UiError::AlreadyPlaced)
    );
    assert_eq!(
        fixture.tree.insert_before(first, column),
        Err(UiError::Cycle)
    );
    fixture.tree.insert_before(first, first).unwrap();
    assert_eq!(fixture.tree.children(column), [first], "itself: no change");
}

/// A region's marker holds a place and nothing else: no box (a column's gap
/// does not count it), no paint, no hit, no outline entry, no tab stop, and
/// it does not count as a scroll's one child.
#[test]
fn a_marker_holds_a_place_and_is_never_laid_out_painted_hit_or_listed() {
    let mut fixture = Fixture::new();
    let column = fixture.element(ElementKind::Column);
    fixture.tree.set_gap(column, 5.0).unwrap();
    let above = fixture.block(40.0, 10.0);
    let marker = fixture.element(ElementKind::Marker);
    let below = fixture.block(40.0, 10.0);
    fixture.tree.set_tab_index(marker, Some(0));
    fixture.add(column, &[above, marker, below]);
    fixture.layout(column, 100.0, 100.0);
    assert_eq!(fixture.rect(below).1, 15.0, "one gap, not two");
    let kinds: Vec<_> = fixture
        .tree
        .outline(column)
        .into_iter()
        .map(|entry| entry.kind)
        .collect();
    assert_eq!(
        kinds,
        [
            ElementKind::Column,
            ElementKind::Column,
            ElementKind::Column
        ]
    );
    assert_eq!(
        fixture.tree.hit(column, Point::new(0.0, 10.5)),
        Some(column)
    );
    // Placed before the marker: between the two blocks.
    let between = fixture.block(40.0, 10.0);
    fixture.tree.insert_before(marker, between).unwrap();
    fixture.layout(column, 100.0, 100.0);
    assert_eq!(fixture.rect(between).1, 15.0);
    assert_eq!(fixture.rect(below).1, 30.0);
    // Painted: three blocks with backgrounds, nothing for the marker.
    for block in [above, between, below] {
        fixture
            .tree
            .set_background(block, Color::new(1.0, 0.0, 0.0, 1.0));
    }
    let stats = paint_stats(&mut fixture, column);
    assert_eq!(stats.rebuilt, 4, "the column and its three blocks");
    // A scroll's one child is its one element; its markers do not count.
    let scroll = fixture.element(ElementKind::Scroll);
    let scroll_marker = fixture.element(ElementKind::Marker);
    let content = fixture.element(ElementKind::Column);
    let tall = fixture.block(40.0, 400.0);
    fixture.add(content, &[tall]);
    fixture.add(scroll, &[scroll_marker]);
    fixture.tree.insert_before(scroll_marker, content).unwrap();
    let second = fixture.block(1.0, 1.0);
    assert_eq!(
        fixture.tree.add_child(scroll, second),
        Err(UiError::OneChild)
    );
    fixture
        .tree
        .set_size(scroll, SizeProperty::Height, Length::Dp(100.0))
        .unwrap();
    fixture.layout(scroll, 100.0, 100.0);
    assert_eq!(
        fixture.tree.scroll_range(scroll),
        Some(Size::new(0.0, 300.0))
    );
}

// ------------------------------------------------- scrolling into view ---

/// A scroll of `count` focusable 30 dp rows in a 100 dp viewport, inside a
/// root column: `(root, scroll, rows)`.
fn scrolled_rows(fixture: &mut Fixture, count: usize) -> (NodeId, NodeId, Vec<NodeId>) {
    let root = fixture.element(ElementKind::Column);
    let scroll = fixture.element(ElementKind::Scroll);
    fixture
        .tree
        .set_size(scroll, SizeProperty::Height, Length::Dp(100.0))
        .unwrap();
    let column = fixture.element(ElementKind::Column);
    let mut rows = Vec::new();
    for _ in 0..count {
        let row = fixture.block(50.0, 30.0);
        fixture.tree.set_tab_index(row, Some(0));
        fixture.add(column, &[row]);
        rows.push(row);
    }
    fixture.add(scroll, &[column]);
    fixture.add(root, &[scroll]);
    fixture.layout(root, 200.0, 300.0);
    (root, scroll, rows)
}

fn tab(fixture: &mut Fixture, root: NodeId, backward: bool) {
    let route = fixture.tree.route(
        root,
        &Input::Key {
            name: "Tab".to_string(),
            shift: backward,
        },
    );
    Recorder::new().run(fixture, root, route);
}

/// Tab brings the element it focuses into view: as little scrolling as it
/// takes — none while the element shows, its bottom edge to the viewport's
/// going down, its top edge to the viewport's going up — and a wrap to the
/// first element scrolls back to the top. The program is told, as a wheel's
/// move is.
#[test]
fn tab_scrolls_the_focused_element_into_view_by_as_little_as_it_takes() {
    let mut fixture = Fixture::new();
    let (root, scroll, rows) = scrolled_rows(&mut fixture, 10);
    let offset = |fixture: &Fixture| fixture.tree.scroll_offset(scroll).unwrap().y;
    for _ in 0..3 {
        tab(&mut fixture, root, false);
    }
    assert_eq!(fixture.tree.focused(), Some(rows[2]));
    assert_eq!(offset(&fixture), 0.0, "rows 0 to 2 show (0..90 of 100)");
    tab(&mut fixture, root, false);
    assert_eq!(offset(&fixture), 20.0, "row 3 (90..120): its bottom at 100");
    assert_eq!(
        fixture.tree.take_offset_notices().len(),
        0,
        "no handler set"
    );
    for _ in 0..6 {
        tab(&mut fixture, root, false);
    }
    assert_eq!(fixture.tree.focused(), Some(rows[9]));
    assert_eq!(offset(&fixture), 200.0, "the last row's bottom at 100");
    tab(&mut fixture, root, true);
    tab(&mut fixture, root, true);
    assert_eq!(fixture.tree.focused(), Some(rows[7]));
    assert_eq!(offset(&fixture), 200.0, "rows 7 to 9 show (210..300)");
    tab(&mut fixture, root, true);
    assert_eq!(offset(&fixture), 180.0, "row 6 (180..210): its top at 0");
    for _ in 0..4 {
        tab(&mut fixture, root, false);
    }
    assert_eq!(fixture.tree.focused(), Some(rows[0]), "wrapped");
    assert_eq!(offset(&fixture), 0.0);
}

/// `reveal` (`Element::scroll_into_view`) through two scrolls: the inner one
/// shows the element, then the outer one shows what the inner one shows of
/// it.
#[test]
fn revealing_an_element_scrolls_every_scroll_around_it_innermost_first() {
    let mut fixture = Fixture::new();
    let outer = fixture.element(ElementKind::Scroll);
    fixture
        .tree
        .set_size(outer, SizeProperty::Height, Length::Dp(100.0))
        .unwrap();
    let outer_column = fixture.element(ElementKind::Column);
    let spacer = fixture.block(50.0, 200.0);
    let (_, inner, rows) = scrolled_rows(&mut fixture, 10);
    // `scrolled_rows` gave the inner scroll a root column of its own; hang
    // that column under the outer scroll's column, after a spacer.
    let inner_parent = fixture.tree.root_of(inner);
    fixture.add(outer_column, &[spacer, inner_parent]);
    fixture.add(outer, &[outer_column]);
    fixture.layout(outer, 200.0, 100.0);
    assert!(fixture.tree.reveal(rows[5]));
    // Inner: row 5 at 150..180 of a 100 dp viewport: offset 80.
    assert_eq!(fixture.tree.scroll_offset(inner).unwrap().y, 80.0);
    // Outer: the inner scroll at 200..300 shows rows 5 at its 70..100, so
    // that part (270..300) is brought to the outer viewport's bottom.
    assert_eq!(fixture.tree.scroll_offset(outer).unwrap().y, 200.0);
    assert!(
        !fixture.tree.reveal(rows[5]),
        "shown already: nothing moves"
    );
}

// ------------------------------------------------------------ editing ---
//
// `std::ui::kit`'s text input (P4): an EDITED text (`UiTree::text_edit`).
// Noto Mono at 10 dp: every grapheme the font draws is 6 dp wide (to a
// thousandth: its advance is 0.6 em in font units), and a line height of 1.2
// makes a line 12 dp, so no box is snapped off its dp.

/// A paragraph of `text` at 10 dp with 12 dp lines.
fn line_paragraph(fixture: &mut Fixture, text: &str) -> NodeId {
    let node = fixture.paragraph(text, 10.0);
    fixture
        .tree
        .set_text_style(node, |style| style.line_height = Some(1.2));
    node
}

/// [`line_paragraph`], edited, its caret at UTF-16 index `caret`.
fn edited(fixture: &mut Fixture, text: &str, caret: usize, active: bool) -> NodeId {
    let node = line_paragraph(fixture, text);
    fixture.tree.text_edit(node, caret, caret, active).unwrap();
    node
}

/// A 30 dp wide horizontal scroll over `text`, edited: `(root, scroll, text)`.
fn scrolled_field(fixture: &mut Fixture, text: &str) -> (NodeId, NodeId, NodeId) {
    let scroll = fixture.element(ElementKind::Scroll);
    fixture
        .tree
        .set_scroll_axis(scroll, ScrollAxis::Horizontal)
        .unwrap();
    fixture
        .tree
        .set_size(scroll, SizeProperty::Width, Length::Dp(30.0))
        .unwrap();
    let node = edited(fixture, text, 0, false);
    fixture.add(scroll, &[node]);
    let root = fixture.element(ElementKind::Stack);
    fixture.tree.set_padding(root, Edges::all(5.0)).unwrap();
    fixture.add(root, &[scroll]);
    fixture.layout(root, 300.0, 300.0);
    (root, scroll, node)
}

#[test]
fn an_edited_text_is_never_shorter_than_a_line_and_has_room_for_its_caret() {
    let mut fixture = Fixture::new();
    let column = fixture.element(ElementKind::Column);
    fixture.tree.set_align(column, Align::Start).unwrap();
    let plain = line_paragraph(&mut fixture, "abc");
    let empty = edited(&mut fixture, "", 0, false);
    let full = edited(&mut fixture, "abc", 0, false);
    fixture.add(column, &[plain, empty, full]);
    fixture.layout(column, 300.0, 300.0);
    let (_, _, plain_width, line) = fixture.rect(plain);
    assert_eq!((plain_width, line), (18.0, 12.0));
    // One line's height even when empty, and one caret width (1 dp) wider.
    assert_eq!(fixture.rect(empty).3, line);
    assert_eq!(fixture.rect(full).2, 19.0);
    // A wider caret is more room.
    fixture.tree.text_edit_look(full, None, 3.0, None).unwrap();
    fixture.layout(column, 300.0, 300.0);
    assert_eq!(fixture.rect(full).2, 21.0);
}

#[test]
fn steps_count_utf16_units_and_move_a_whole_grapheme() {
    let mut fixture = Fixture::new();
    // "a", an e with a combining acute (two units), a thumbs-up with a skin
    // tone (two code points, four units), "b": eight units, four graphemes.
    let node = edited(&mut fixture, "ae\u{301}\u{1F44D}\u{1F3FD}b", 0, false);
    let mut forward = vec![0];
    for _ in 0..5 {
        let at = *forward.last().unwrap();
        forward.push(
            fixture
                .tree
                .text_step(node, at, true, false, &mut fixture.text)
                .unwrap(),
        );
    }
    assert_eq!(forward, [0, 1, 3, 7, 8, 8]);
    let mut back = vec![8];
    for _ in 0..5 {
        let at = *back.last().unwrap();
        back.push(
            fixture
                .tree
                .text_step(node, at, false, false, &mut fixture.text)
                .unwrap(),
        );
    }
    assert_eq!(back, [8, 7, 3, 1, 0, 0]);
    // From inside a grapheme (between the e and its accent; inside the
    // surrogate pair): from its start.
    assert_eq!(
        fixture
            .tree
            .text_step(node, 2, true, false, &mut fixture.text)
            .unwrap(),
        3
    );
    assert_eq!(
        fixture
            .tree
            .text_step(node, 4, false, false, &mut fixture.text)
            .unwrap(),
        1
    );
    // Words: the end of the next one, the start of the previous one.
    let words = edited(&mut fixture, "milk and eggs", 0, false);
    assert_eq!(
        fixture
            .tree
            .text_step(words, 0, true, true, &mut fixture.text)
            .unwrap(),
        4
    );
    assert_eq!(
        fixture
            .tree
            .text_step(words, 13, false, true, &mut fixture.text)
            .unwrap(),
        9
    );
}

#[test]
fn a_point_finds_its_index_through_the_scroll_around_the_text() {
    let mut fixture = Fixture::new();
    let (root, scroll, node) = scrolled_field(&mut fixture, "abcdefghij");
    // The text starts at x 5 (the root's padding); "d" is 18..24 in it.
    let at = |fixture: &mut Fixture, x: f32| {
        fixture
            .tree
            .text_index_at(
                node,
                Point::new(x, 8.0),
                IndexRule::Nearest,
                &mut fixture.text,
            )
            .unwrap()
    };
    assert_eq!(at(&mut fixture, 5.0 + 20.0), 3);
    assert_eq!(at(&mut fixture, 5.0 + 22.0), 4);
    assert_eq!(at(&mut fixture, -40.0), 0);
    assert_eq!(at(&mut fixture, 400.0), 10);
    // Scrolled 20 dp: the same point is 20 dp further into the text.
    fixture
        .tree
        .set_scroll_offset(scroll, Point::new(20.0, 0.0))
        .unwrap();
    fixture.layout(root, 300.0, 300.0);
    assert_eq!(at(&mut fixture, 5.0), 3);
}

#[test]
fn an_active_caret_is_scrolled_into_view_by_the_next_layout() {
    let mut fixture = Fixture::new();
    let (root, scroll, node) = scrolled_field(&mut fixture, "abcdefghij");
    // 60 dp of text and a 1 dp caret in a 30 dp viewport.
    assert_eq!(
        fixture.tree.scroll_range(scroll),
        Some(Size::new(31.0, 0.0))
    );
    // Inactive (no focus): the scroll stays.
    fixture.tree.text_edit(node, 10, 10, false).unwrap();
    fixture.layout(root, 300.0, 300.0);
    assert_eq!(fixture.tree.scroll_offset(scroll), Some(Point::ZERO));
    // Active at the end: the caret's right edge at the viewport's.
    fixture.tree.text_edit(node, 10, 10, true).unwrap();
    assert!(!fixture.layout(root, 300.0, 300.0), "no solve for a caret");
    assert_eq!(
        fixture.tree.scroll_offset(scroll),
        Some(Point::new(31.0, 0.0))
    );
    assert_eq!(
        fixture.tree.take_offset_notices().len(),
        0,
        "no program listens to this scroll"
    );
    // A caret already in view moves nothing; one before the view brings its
    // left edge to the viewport's.
    fixture.tree.text_edit(node, 8, 8, true).unwrap();
    fixture.layout(root, 300.0, 300.0);
    assert_eq!(
        fixture.tree.scroll_offset(scroll),
        Some(Point::new(31.0, 0.0))
    );
    fixture.tree.text_edit(node, 2, 2, true).unwrap();
    fixture.layout(root, 300.0, 300.0);
    let offset = fixture.tree.scroll_offset(scroll).unwrap();
    assert_eq!((offset.x.round(), offset.y), (12.0, 0.0));
}

/// The rectangles an edited text's paint draws: its selection's and its
/// caret's, in order, around its text.
fn edited_paint(fixture: &mut Fixture, root: NodeId) -> Vec<String> {
    let mut builder = SceneBuilder::new();
    fixture
        .tree
        .paint(root, &mut builder, &mut fixture.text)
        .unwrap();
    flatten(&builder.finish().unwrap())
        .into_iter()
        .filter_map(|command| match command {
            Command::Rectangle(rectangle) => {
                let bounds = rectangle.bounds;
                Some(format!(
                    "rect {:.1} {:.1} {:.1} {:.1}",
                    bounds.left(),
                    bounds.top(),
                    bounds.size.width,
                    bounds.size.height
                ))
            }
            Command::Text { .. } => Some("text".to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_focused_edited_text_paints_its_selection_under_and_its_caret_over_it() {
    let mut fixture = Fixture::new();
    let column = fixture.element(ElementKind::Column);
    fixture.tree.set_align(column, Align::Start).unwrap();
    let node = edited(&mut fixture, "abcd", 0, false);
    fixture.add(column, &[node]);
    fixture.layout(column, 300.0, 300.0);
    assert_eq!(edited_paint(&mut fixture, column), ["text"], "no focus");
    // "bc" selected, the caret after it (2 dp wide).
    fixture.tree.text_edit_look(node, None, 2.0, None).unwrap();
    fixture.tree.text_edit(node, 3, 1, true).unwrap();
    fixture.layout(column, 300.0, 300.0);
    assert_eq!(
        edited_paint(&mut fixture, column),
        [
            "rect 6.0 0.0 12.0 12.0".to_string(),
            "text".to_string(),
            "rect 18.0 0.0 2.0 12.0".to_string(),
        ]
    );
    // Moving the caret repaints the text alone: no layout.
    fixture.tree.text_edit(node, 4, 4, true).unwrap();
    assert!(!fixture.layout(column, 300.0, 300.0), "no solve");
    assert_eq!(
        edited_paint(&mut fixture, column),
        ["text", "rect 24.0 0.0 2.0 12.0"]
    );
    assert_eq!(fixture.tree.last_pass().rebuilt, 2, "the text and the root");
}

#[test]
fn caret_questions_to_a_text_not_edited_or_not_a_text_are_refused() {
    let mut fixture = Fixture::new();
    let plain = fixture.paragraph("abc", 10.0);
    assert_eq!(
        fixture
            .tree
            .text_step(plain, 0, true, false, &mut fixture.text),
        Err(UiError::NotEdited)
    );
    let row = fixture.element(ElementKind::Row);
    assert_eq!(
        fixture.tree.text_edit(row, 0, 0, true),
        Err(UiError::WrongKind {
            property: "caret",
            kind: ElementKind::Row,
        })
    );
}

// ---------------------------------------------------- multi-line editing ---
//
// `std::ui::kit`'s text area (P5): an edited text marked multi-line
// (`UiTree::text_multiline`), wrapping at its width. Noto Mono at 10 dp with
// 12 dp lines, as above: a grapheme is 6.001 dp wide, so five fit in 30.005.

/// [`edited`], multi-line, in a column `width` dp wide: `(column, text)`.
fn multiline(fixture: &mut Fixture, text: &str, width: f32) -> (NodeId, NodeId) {
    let column = fixture.element(ElementKind::Column);
    fixture
        .tree
        .set_size(column, SizeProperty::Width, Length::Dp(width))
        .unwrap();
    let node = edited(fixture, text, 0, false);
    fixture.tree.text_multiline(node).unwrap();
    fixture.add(column, &[node]);
    fixture.layout(column, 300.0, 300.0);
    (column, node)
}

#[test]
fn an_edited_text_wraps_at_its_width_less_the_carets_room() {
    let mut fixture = Fixture::new();
    // "abcde" is 30.005 dp: in 31 dp, less the 1 dp caret, it does not fit,
    // and "e" goes to a second line — where its paint, which shapes at the
    // same 30 dp, draws it.
    let (_, node) = multiline(&mut fixture, "abcde", 31.0);
    assert_eq!(fixture.rect(node).3, 24.0);
    // In 32 dp it fits on one.
    let (_, node) = multiline(&mut fixture, "abcde", 32.0);
    assert_eq!(fixture.rect(node).3, 12.0);
}

#[test]
fn a_multi_line_text_breaks_a_word_longer_than_its_line_and_a_one_line_one_does_not() {
    let mut fixture = Fixture::new();
    let (_, node) = multiline(&mut fixture, "abcdefghij", 32.0);
    assert_eq!(fixture.rect(node).3, 24.0, "two lines of five");
    let column = fixture.element(ElementKind::Column);
    fixture
        .tree
        .set_size(column, SizeProperty::Width, Length::Dp(32.0))
        .unwrap();
    let single = edited(&mut fixture, "abcdefghij", 0, false);
    fixture.add(column, &[single]);
    fixture.layout(column, 300.0, 300.0);
    assert_eq!(fixture.rect(single).3, 12.0, "between words only: one line");
}

#[test]
fn line_ends_and_vertical_moves_count_utf16_units_through_the_tree() {
    let mut fixture = Fixture::new();
    // "é" (e and a combining acute: two units, one grapheme) "bcd", a
    // break, "xy", a break, "longer": units e 0, accent 1, b 2, c 3, d 4,
    // break 5, x 6, y 7, break 8, l 9 ...
    let (_, node) = multiline(&mut fixture, "e\u{301}bcd\nxy\nlonger", 200.0);
    let tree = &mut fixture.tree;
    let text = &mut fixture.text;
    let caret = tree.text_caret_box(node, 4, text).unwrap();
    assert_eq!(caret.top(), 0.0);
    assert_eq!(caret.size.height, 12.0);
    let goal = caret.left();
    assert!((goal - 18.0).abs() < 0.01, "{goal}");
    // Down to the short line: its end, before its break; down again: the
    // same x in "longer"; two lines up: back to the goal.
    assert_eq!(tree.text_vertical(node, 4, goal, 1, text).unwrap(), 8);
    assert_eq!(tree.text_vertical(node, 8, goal, 1, text).unwrap(), 12);
    assert_eq!(tree.text_vertical(node, 12, goal, -2, text).unwrap(), 4);
    assert_eq!(tree.text_vertical(node, 4, goal, -1, text).unwrap(), 0);
    assert_eq!(tree.text_vertical(node, 12, goal, 1, text).unwrap(), 15);
    assert_eq!(tree.text_line_end(node, 7, false, text).unwrap(), 6);
    assert_eq!(tree.text_line_end(node, 7, true, text).unwrap(), 8);
    assert_eq!(tree.text_line_end(node, 1, true, text).unwrap(), 5);
    assert_eq!(tree.text_line_end(node, 10, true, text).unwrap(), 15);
    assert_eq!(
        tree.text_vertical(node, 4, f32::NAN, 1, text),
        Err(UiError::NotFinite("a goal x"))
    );
}

#[test]
fn a_caret_at_a_soft_wrap_is_drawn_at_the_start_of_the_lower_line_and_a_selection_a_box_a_line() {
    let mut fixture = Fixture::new();
    // In 32 dp (31 for the text): "abcd " and "efgh".
    let (column, node) = multiline(&mut fixture, "abcd efgh", 32.0);
    assert_eq!(fixture.rect(node).3, 24.0);
    // The wrap point, index 5: the lower line's start.
    fixture.tree.text_edit(node, 5, 5, true).unwrap();
    fixture.layout(column, 300.0, 300.0);
    assert_eq!(
        edited_paint(&mut fixture, column),
        ["text", "rect 0.0 12.0 1.0 12.0"]
    );
    // "cd " and "ef" selected: one box on each line, the caret after "f".
    fixture.tree.text_edit(node, 7, 2, true).unwrap();
    fixture.layout(column, 300.0, 300.0);
    assert_eq!(
        edited_paint(&mut fixture, column),
        [
            "rect 12.0 0.0 18.0 12.0",
            "rect 0.0 12.0 12.0 12.0",
            "text",
            "rect 12.0 12.0 1.0 12.0"
        ]
    );
}

#[test]
fn caret_questions_answer_from_the_laid_out_width_before_any_paint() {
    let mut fixture = Fixture::new();
    // Laid out and never painted (a window-less `--check`): "abcd " and
    // "efgh" are two lines already, so Down from the start reaches "e".
    let (_, node) = multiline(&mut fixture, "abcd efgh", 32.0);
    assert_eq!(
        fixture
            .tree
            .text_vertical(node, 0, 0.0, 1, &mut fixture.text)
            .unwrap(),
        5
    );
    assert_eq!(
        fixture
            .tree
            .text_line_end(node, 0, true, &mut fixture.text)
            .unwrap(),
        4
    );
}

/// A vertical scroll two lines (24 dp) tall over a five-line multi-line
/// text, inside a stack with 5 dp of padding: `(root, scroll, text)`.
fn scrolled_area(fixture: &mut Fixture) -> (NodeId, NodeId, NodeId) {
    let scroll = fixture.element(ElementKind::Scroll);
    fixture
        .tree
        .set_size(scroll, SizeProperty::Width, Length::Dp(100.0))
        .unwrap();
    fixture
        .tree
        .set_size(scroll, SizeProperty::Height, Length::Dp(24.0))
        .unwrap();
    let node = edited(fixture, "l1\nl2\nl3\nl4\nl5", 0, false);
    fixture.tree.text_multiline(node).unwrap();
    fixture.add(scroll, &[node]);
    let root = fixture.element(ElementKind::Stack);
    fixture.tree.set_padding(root, Edges::all(5.0)).unwrap();
    fixture.add(root, &[scroll]);
    fixture.layout(root, 300.0, 300.0);
    (root, scroll, node)
}

#[test]
fn a_drag_past_the_bottom_or_top_lands_one_line_beyond_what_shows() {
    let mut fixture = Fixture::new();
    let (root, scroll, node) = scrolled_area(&mut fixture);
    let drag = |fixture: &mut Fixture, y: f32| {
        fixture
            .tree
            .text_index_at(
                node,
                Point::new(8.0, y),
                IndexRule::DragLines,
                &mut fixture.text,
            )
            .unwrap()
    };
    // Lines 0 and 1 show (y 5..29): below them, line 2's start ("l3" at 6).
    assert_eq!(drag(&mut fixture, 100.0), 6);
    // Its reveal scrolls one line; the next move lands on line 3.
    fixture.tree.text_edit(node, 6, 0, true).unwrap();
    fixture.layout(root, 300.0, 300.0);
    assert_eq!(
        fixture.tree.scroll_offset(scroll),
        Some(Point::new(0.0, 12.0))
    );
    assert_eq!(drag(&mut fixture, 100.0), 9);
    // Above, line 0 (scrolled away) at the pointer's x.
    assert_eq!(drag(&mut fixture, -40.0), 0);
    // Level with what shows, the plain rule: the point's own index (y 20
    // is line 2's now, x 8 the start of "l3").
    assert_eq!(drag(&mut fixture, 20.0), 6);
    // Past the last line, in a text that shows whole: its end.
    let (_, short) = multiline(&mut fixture, "ab\ncd", 100.0);
    let end = fixture
        .tree
        .text_index_at(
            short,
            Point::new(1.0, 500.0),
            IndexRule::DragLines,
            &mut fixture.text,
        )
        .unwrap();
    assert_eq!(end, 5);
}

// ------------------------------------------------- sizes and stale boxes ---
//
// Part P4's virtual list reads its viewport's height after a layout (a resize
// notice), builds rows that are not laid out yet, and may scroll to a row it
// added in the same turn: three things the tree answers from boxes, each pinned
// here at the tree's own level.

#[test]
fn a_resize_notice_tells_the_first_size_and_each_change_and_nothing_else() {
    let mut fixture = Fixture::new();
    let root = fixture.element(ElementKind::Column);
    let watched = fixture.element(ElementKind::Column);
    fixture.tree.set_grow(watched, 1.0).unwrap();
    let unwatched = fixture.block(40.0, 40.0);
    let spacer = fixture.block(40.0, 10.0);
    fixture.add(root, &[spacer, watched, unwatched]);
    let heard = Rc::new(std::cell::Cell::new(0));
    let counter = Rc::clone(&heard);
    fixture.tree.add_handler(
        watched,
        EventKind::Resize,
        Handler::new(move || counter.set(counter.get() + 1)),
    );
    let sizes = |fixture: &mut Fixture| -> Vec<Size> {
        fixture
            .tree
            .take_resize_notices()
            .into_iter()
            .map(|(handler, size)| {
                handler.call();
                size
            })
            .collect()
    };
    fixture.layout(root, 100.0, 200.0);
    assert_eq!(
        sizes(&mut fixture),
        [Size::new(100.0, 150.0)],
        "the first layout gives it a size: told once, with it"
    );
    assert_eq!(heard.get(), 1);
    fixture.layout(root, 100.0, 200.0);
    assert_eq!(sizes(&mut fixture), [], "the same size: nothing");
    // It MOVES (the spacer grows) and shrinks; then only moves.
    fixture
        .tree
        .set_size(spacer, SizeProperty::Height, Length::Dp(30.0))
        .unwrap();
    fixture.layout(root, 100.0, 200.0);
    assert_eq!(sizes(&mut fixture), [Size::new(100.0, 130.0)]);
    fixture
        .tree
        .set_size(spacer, SizeProperty::Height, Length::Dp(50.0))
        .unwrap();
    fixture
        .tree
        .set_size(unwatched, SizeProperty::Height, Length::Dp(20.0))
        .unwrap();
    fixture.layout(root, 100.0, 200.0);
    assert_eq!(
        sizes(&mut fixture),
        [],
        "moved down 20 and grown by 20 below: the same size, so nothing"
    );
    fixture.layout(root, 100.0, 400.0);
    assert_eq!(
        sizes(&mut fixture),
        [Size::new(100.0, 330.0)],
        "a new window size reaches it"
    );
    // Removed: its handlers go with it, the resize one included.
    let handlers = fixture.tree.remove(watched);
    assert_eq!(handlers.len(), 1);
    fixture.layout(root, 100.0, 300.0);
    assert_eq!(sizes(&mut fixture), []);
}

#[test]
fn an_offset_set_while_a_layout_is_pending_is_kept_until_that_layout_clamps_it() {
    let mut fixture = Fixture::new();
    let (scroll, rows) = scrolled_list(&mut fixture);
    let root = fixture.tree.root_of(scroll);
    let column = fixture.tree.children(scroll)[0];
    assert_eq!(
        fixture.tree.scroll_range(scroll),
        Some(Size::new(0.0, 400.0))
    );
    // Two rows more, and an offset past the old range in the same breath (a
    // list that grew, scrolled to its new end): kept as asked.
    let more = [fixture.block(100.0, 50.0), fixture.block(100.0, 50.0)];
    fixture.add(column, &more);
    fixture
        .tree
        .set_scroll_offset(scroll, Point::new(0.0, 500.0))
        .unwrap();
    assert_eq!(
        fixture.tree.scroll_offset(scroll),
        Some(Point::new(0.0, 500.0)),
        "the range is stale until the layout: not clamped to 400"
    );
    assert!(fixture.tree.take_offset_notices().is_empty());
    fixture.layout(root, 300.0, 300.0);
    assert_eq!(
        fixture.tree.scroll_offset(scroll),
        Some(Point::new(0.0, 500.0)),
        "inside the range the layout measured"
    );
    assert!(fixture.tree.take_offset_notices().is_empty());
    // Past even the new range: the layout clamps it, and says so.
    fixture.tree.remove(rows[0]);
    fixture
        .tree
        .set_scroll_offset(scroll, Point::new(0.0, 900.0))
        .unwrap();
    fixture.layout(root, 300.0, 300.0);
    assert_eq!(
        fixture.tree.scroll_offset(scroll),
        Some(Point::new(0.0, 450.0))
    );
    assert_eq!(fixture.tree.take_offset_notices().len(), 0, "no listener");
    // With the range current, a set clamps at once (as before).
    fixture
        .tree
        .set_scroll_offset(scroll, Point::new(0.0, 900.0))
        .unwrap();
    assert_eq!(
        fixture.tree.scroll_offset(scroll),
        Some(Point::new(0.0, 450.0))
    );
}

#[test]
fn tab_onto_an_element_built_since_the_last_layout_reveals_it_after_the_next_layout() {
    let mut fixture = Fixture::new();
    let (root, scroll, rows) = scrolled_rows(&mut fixture, 6);
    let column = fixture.tree.children(scroll)[0];
    // Rows 0..6 at 30 dp in a 100 dp viewport: the last at 150..180.
    fixture.tree.focus(Some(rows[5]));
    assert!(fixture.tree.reveal(rows[5]));
    assert_eq!(fixture.tree.scroll_offset(scroll).unwrap().y, 80.0);
    // A row arrives after the focused one and is not laid out: its box is
    // empty, at the origin. Tab focuses it; revealing that box would scroll to
    // the top.
    let fresh = fixture.block(50.0, 30.0);
    fixture.tree.set_tab_index(fresh, Some(0));
    fixture.add(column, &[fresh]);
    tab(&mut fixture, root, false);
    assert_eq!(fixture.tree.focused(), Some(fresh));
    assert_eq!(
        fixture.tree.scroll_offset(scroll).unwrap().y,
        80.0,
        "nothing moves before the layout"
    );
    assert!(fixture.tree.reveal_pending());
    fixture.layout(root, 200.0, 300.0);
    // The fresh row at 180..210: its bottom at the viewport's.
    assert_eq!(fixture.tree.scroll_offset(scroll).unwrap().y, 110.0);
    assert!(!fixture.tree.reveal_pending());
}

// ------------------------------------------------------- pointer capture ---

/// `buttons`, with every pointer event of `first`, `second`, `card` and the
/// root recorded.
fn captured_buttons(fixture: &mut Fixture) -> (Recorder, NodeId, NodeId, NodeId, NodeId) {
    let (root, card, first, second) = buttons(fixture);
    let recorder = Recorder::new();
    for (node, name) in [
        (root, "root"),
        (card, "card"),
        (first, "first"),
        (second, "second"),
    ] {
        for kind in [
            EventKind::PointerEnter,
            EventKind::PointerLeave,
            EventKind::PointerMove,
            EventKind::Release,
            EventKind::Click,
        ] {
            recorder.listen(fixture, node, kind, name);
        }
    }
    (recorder, root, card, first, second)
}

/// The press captures the pointer: every move and the release go to the
/// element it landed on (bubbling from there), wherever the pointer goes —
/// over another element, outside the root. Enter and leave follow the
/// pointer; `pressed` holds on the capturing chain until the release while
/// `hovered` is geometric; a release outside the tree clicks nothing.
#[test]
fn a_press_captures_the_pointer_until_the_release_wherever_it_goes() {
    let mut fixture = Fixture::new();
    let (recorder, root, card, first, second) = captured_buttons(&mut fixture);
    let on_first = Point::new(20.0, 20.0);
    recorder.route(&mut fixture, root, Input::PointerMoved(on_first));
    recorder.route(&mut fixture, root, press(on_first));
    assert_eq!(fixture.tree.pressed(), Some(first));
    let on_second = Point::new(fixture.rect(second).0 + 5.0, 5.0);
    assert_eq!(
        recorder.route(&mut fixture, root, Input::PointerMoved(on_second)),
        [
            "PointerLeave first",
            "PointerLeave card",
            "PointerEnter second",
            "PointerMove first",
            "PointerMove card",
            "PointerMove root"
        ],
        "the hover follows the pointer, the move goes to the capture"
    );
    for (node, pressed, hovered) in [
        (first, true, false),
        (card, true, false),
        (root, true, true),
        (second, false, true),
    ] {
        let state = fixture.tree.interaction(node);
        assert_eq!((state.pressed, state.hovered), (pressed, hovered));
    }
    // Off the root (the window keeps sending a held button's moves).
    let outside = Point::new(-40.0, 500.0);
    assert_eq!(
        recorder.route(&mut fixture, root, Input::PointerMoved(outside)),
        [
            "PointerLeave second",
            "PointerLeave root",
            "PointerMove first",
            "PointerMove card",
            "PointerMove root"
        ]
    );
    assert_eq!(
        recorder.route(&mut fixture, root, Input::Released(outside)),
        ["Release first", "Release card", "Release root"],
        "the release goes to the capture, and outside it is no click"
    );
    assert_eq!(fixture.tree.pressed(), None);
    assert!(!fixture.tree.interaction(first).pressed);
    // The capture is over: a move goes to what is under the pointer again.
    assert_eq!(
        recorder.route(&mut fixture, root, Input::PointerMoved(on_second)),
        [
            "PointerEnter root",
            "PointerEnter second",
            "PointerMove second",
            "PointerMove root"
        ]
    );
}

/// A release back over the pressed element clicks it; a release over
/// another one clicks the innermost element holding both — the release
/// itself going to the capture either way.
#[test]
fn the_click_is_decided_by_where_the_release_lands() {
    let mut fixture = Fixture::new();
    let (recorder, root, _, _, second) = captured_buttons(&mut fixture);
    let on_first = Point::new(20.0, 20.0);
    let on_second = Point::new(fixture.rect(second).0 + 5.0, 5.0);
    recorder.route(&mut fixture, root, press(on_first));
    recorder.route(&mut fixture, root, Input::PointerMoved(on_second));
    recorder.route(&mut fixture, root, Input::PointerMoved(on_first));
    assert_eq!(
        recorder.route(&mut fixture, root, Input::Released(on_first)),
        [
            "Release first",
            "Release card",
            "Release root",
            "Click first",
            "Click card",
            "Click root"
        ]
    );
    recorder.route(&mut fixture, root, press(on_first));
    assert_eq!(
        recorder.route(&mut fixture, root, Input::Released(on_second)),
        [
            "PointerLeave first",
            "PointerLeave card",
            "PointerEnter second",
            "Release first",
            "Release card",
            "Release root",
            "Click root"
        ]
    );
}

/// The pointer leaving the window ends no capture: the hover empties, and
/// the moves and the release that follow still reach the capture.
#[test]
fn the_pointer_leaving_the_window_keeps_the_capture() {
    let mut fixture = Fixture::new();
    let (recorder, root, _, first, _) = captured_buttons(&mut fixture);
    let on_first = Point::new(20.0, 20.0);
    recorder.route(&mut fixture, root, Input::PointerMoved(on_first));
    recorder.route(&mut fixture, root, press(on_first));
    assert_eq!(
        recorder.route(&mut fixture, root, Input::PointerLeft),
        [
            "PointerLeave first",
            "PointerLeave card",
            "PointerLeave root"
        ]
    );
    assert_eq!(fixture.tree.pressed(), Some(first));
    let outside = Point::new(900.0, 20.0);
    assert_eq!(
        recorder.route(&mut fixture, root, Input::PointerMoved(outside)),
        ["PointerMove first", "PointerMove card", "PointerMove root"]
    );
    assert_eq!(
        recorder.route(&mut fixture, root, Input::Released(outside)),
        ["Release first", "Release card", "Release root"]
    );
}

/// Removing the capturing element (or an ancestor of it) ends the capture
/// and cancels the press: the rest of the drag goes to whatever is under
/// the pointer, as with no press held, and nothing clicks.
#[test]
fn removing_the_capturing_element_ends_the_capture_and_its_click() {
    let mut fixture = Fixture::new();
    let (recorder, root, card, first, second) = captured_buttons(&mut fixture);
    let on_first = Point::new(20.0, 20.0);
    recorder.route(&mut fixture, root, Input::PointerMoved(on_first));
    recorder.route(&mut fixture, root, press(on_first));
    drop(fixture.tree.remove(card));
    assert_eq!(fixture.tree.pressed(), None);
    assert!(!fixture.tree.interaction(first).pressed);
    fixture.layout(root, 300.0, 100.0);
    let on_second = Point::new(fixture.rect(second).0 + 5.0, 5.0);
    let moved = recorder.route(&mut fixture, root, Input::PointerMoved(on_second));
    assert_eq!(
        moved.last().map(String::as_str),
        Some("PointerMove root"),
        "{moved:?}"
    );
    assert!(
        moved.contains(&"PointerMove second".to_string()),
        "{moved:?}"
    );
    assert_eq!(
        recorder.route(&mut fixture, root, Input::Released(on_second)),
        ["Release second", "Release root"],
        "a release with no press held goes to what is under the pointer, and clicks nothing"
    );
}

/// The program's own release (`Element::release_pointer`, which a control
/// disabled mid-drag calls) does what removal does, for the element holding
/// the press or any ancestor of it; for another element it does nothing.
#[test]
fn releasing_the_pointer_ends_the_capture_held_inside_the_element() {
    let mut fixture = Fixture::new();
    let (recorder, root, card, first, second) = captured_buttons(&mut fixture);
    let on_first = Point::new(20.0, 20.0);
    recorder.route(&mut fixture, root, press(on_first));
    assert!(!fixture.tree.release_pointer(second));
    assert_eq!(fixture.tree.pressed(), Some(first));
    assert!(fixture.tree.release_pointer(card));
    assert_eq!(fixture.tree.pressed(), None);
    assert!(!fixture.tree.interaction(first).pressed);
    assert!(!fixture.tree.interaction(card).pressed);
    assert!(
        !fixture.tree.release_pointer(card),
        "nothing left to release"
    );
    let on_second = Point::new(fixture.rect(second).0 + 5.0, 5.0);
    let moved = recorder.route(&mut fixture, root, Input::PointerMoved(on_second));
    assert!(
        moved.contains(&"PointerMove second".to_string()),
        "{moved:?}"
    );
    assert!(
        !moved.contains(&"PointerMove first".to_string()),
        "{moved:?}"
    );
    assert_eq!(
        recorder.route(&mut fixture, root, Input::Released(on_first)),
        [
            "PointerLeave second",
            "PointerEnter card",
            "PointerEnter first",
            "Release first",
            "Release card",
            "Release root"
        ],
        "released back over it: no click, the press was let go"
    );
}

/// A press counts with the one before it while it lands on the same element,
/// within the distance and the time of it; by the time the input carries.
#[test]
fn presses_in_a_row_are_counted_by_element_place_and_time() {
    let mut fixture = Fixture::new();
    let (root, _, _, second) = buttons(&mut fixture);
    let at = Point::new(20.0, 20.0);
    let count = |fixture: &mut Fixture, input: Input| {
        fixture.tree.route(root, &input);
        fixture.tree.route(root, &Input::Released(at));
        fixture.tree.click_count()
    };
    assert_eq!(fixture.tree.click_count(), 0, "before any press");
    assert_eq!(count(&mut fixture, press_at(at, 1000)), 1);
    assert_eq!(count(&mut fixture, press_at(at, 1300)), 2);
    // Within the distance, each axis, and the time of the LAST press.
    let near = Point::new(at.x + MULTI_CLICK_DISTANCE, at.y - MULTI_CLICK_DISTANCE);
    assert_eq!(count(&mut fixture, press_at(near, 1700)), 3);
    assert_eq!(
        count(&mut fixture, press_at(near, 2200)),
        4,
        "exactly the time"
    );
    assert_eq!(count(&mut fixture, press_at(near, 2701)), 1, "too late");
    let far = Point::new(near.x + MULTI_CLICK_DISTANCE + 0.5, near.y);
    assert_eq!(count(&mut fixture, press_at(far, 2800)), 1, "too far");
    let on_second = Point::new(fixture.rect(second).0 + 1.0, 5.0);
    let next_to_it = Point::new(fixture.rect(second).0 - 1.0, 5.0);
    assert_eq!(count(&mut fixture, press_at(next_to_it, 3000)), 1);
    assert_eq!(
        count(&mut fixture, press_at(on_second, 3100)),
        1,
        "near enough, but on another element"
    );
    assert_eq!(count(&mut fixture, press_at(on_second, 3200)), 2);
    // A press outside the tree counts nothing, and breaks the run.
    assert_eq!(
        count(&mut fixture, press_at(Point::new(-50.0, 5.0), 3300)),
        0
    );
    assert_eq!(count(&mut fixture, press_at(on_second, 3400)), 1);
    // A clock that went backwards (another source of times) starts again.
    assert_eq!(count(&mut fixture, press_at(on_second, 100)), 1);
}

/// Content that moves under a still pointer moves the hover at the next
/// layout (`refresh_hover`, which the host asks after each one): a deferred
/// reveal (F174), a program's scroll, a removed row — with the leaves and
/// enters owed, and nothing when nothing moved.
#[test]
fn content_moving_under_a_still_pointer_moves_the_hover_after_the_layout() {
    let mut fixture = Fixture::new();
    let (root, scroll, rows) = scrolled_rows(&mut fixture, 6);
    let column = fixture.tree.children(scroll)[0];
    let recorder = Recorder::new();
    for (index, row) in rows.iter().enumerate() {
        recorder.listen(
            &mut fixture,
            *row,
            EventKind::PointerEnter,
            &format!("row {index}"),
        );
        recorder.listen(
            &mut fixture,
            *row,
            EventKind::PointerLeave,
            &format!("row {index}"),
        );
    }
    let refresh = |fixture: &mut Fixture| {
        let route = fixture.tree.refresh_hover(root);
        recorder.run(fixture, root, route);
        std::mem::take(&mut *recorder.log.borrow_mut())
    };
    // The pointer over row 1 (30..60), still.
    let pointer = Point::new(10.0, 45.0);
    assert_eq!(
        recorder.route(&mut fixture, root, Input::PointerMoved(pointer)),
        ["PointerEnter row 1"]
    );
    assert!(refresh(&mut fixture).is_empty(), "nothing moved");
    // A reveal left for the next layout: a row built since the last one.
    let fresh = fixture.block(50.0, 30.0);
    fixture.add(column, &[fresh]);
    assert!(!fixture.tree.reveal_when_laid_out(fresh));
    fixture.layout(root, 200.0, 300.0);
    // The fresh row at 180..210, its bottom at the viewport's: offset 110,
    // so the pointer is over 155 in the column: row 5.
    assert_eq!(fixture.tree.scroll_offset(scroll).unwrap().y, 110.0);
    assert_eq!(
        refresh(&mut fixture),
        ["PointerLeave row 1", "PointerEnter row 5"]
    );
    assert!(fixture.tree.interaction(rows[5]).hovered);
    // The program's own scroll.
    fixture
        .tree
        .set_scroll_offset(scroll, Point::new(0.0, 0.0))
        .unwrap();
    fixture.layout(root, 200.0, 300.0);
    assert_eq!(
        refresh(&mut fixture),
        ["PointerLeave row 5", "PointerEnter row 1"]
    );
    // Row 1 removed: row 2 moves into its place under the pointer (the
    // removed row is told nothing: its handlers went with it).
    drop(fixture.tree.remove(rows[1]));
    fixture.layout(root, 200.0, 300.0);
    assert_eq!(refresh(&mut fixture), ["PointerEnter row 2"]);
    // The pointer gone from the window: nothing to work out.
    recorder.route(&mut fixture, root, Input::PointerLeft);
    fixture
        .tree
        .set_scroll_offset(scroll, Point::new(0.0, 30.0))
        .unwrap();
    fixture.layout(root, 200.0, 300.0);
    assert!(refresh(&mut fixture).is_empty());
}

/// A drag's index: the nearest edge across the part of the field that
/// shows; past its right or left edge, one grapheme beyond the one at that
/// edge — so each move extends one grapheme further, and the caret's reveal
/// scrolls the field by it.
#[test]
fn a_drag_past_the_fields_edge_lands_one_grapheme_beyond_what_shows() {
    let mut fixture = Fixture::new();
    // 6 dp a character; the field shows 30 dp (five characters) from x 5.
    let (root, scroll, node) = scrolled_field(&mut fixture, "abcdefghij");
    let at = |fixture: &mut Fixture, x: f32, rule: IndexRule| {
        fixture
            .tree
            .text_index_at(node, Point::new(x, 8.0), rule, &mut fixture.text)
            .unwrap()
    };
    // Inside: the nearest edge, as a press's.
    assert_eq!(at(&mut fixture, 5.0 + 20.0, IndexRule::Drag), 3);
    // Past the right edge (35): the first index out from it whose caret does
    // not show whole — 5, whose caret stands at the edge — however far past;
    // where a press's rule answers the end.
    assert_eq!(at(&mut fixture, 36.0, IndexRule::Drag), 5);
    assert_eq!(at(&mut fixture, 400.0, IndexRule::Drag), 5);
    assert_eq!(at(&mut fixture, 400.0, IndexRule::Nearest), 10);
    // Past the left edge at the start: nothing before it.
    assert_eq!(at(&mut fixture, -40.0, IndexRule::Drag), 0);
    // A drag held past the right edge: each move puts the caret there, the
    // layout reveals it, and the next move lands one grapheme further, the
    // field scrolling with it.
    let mut offsets = Vec::new();
    for expected in [5, 6, 7, 8] {
        let index = at(&mut fixture, 400.0, IndexRule::Drag);
        assert_eq!(index, expected);
        fixture.tree.text_edit(node, index, 0, true).unwrap();
        fixture.layout(root, 300.0, 300.0);
        offsets.push(fixture.tree.scroll_offset(scroll).unwrap().x);
    }
    assert!(
        offsets.windows(2).all(|pair| pair[1] > pair[0]) && offsets[0] > 0.0,
        "{offsets:?}"
    );
    // Past the left edge with the start scrolled away: one before the first
    // index that shows.
    fixture
        .tree
        .set_scroll_offset(scroll, Point::new(30.0, 0.0))
        .unwrap();
    fixture.layout(root, 300.0, 300.0);
    assert_eq!(at(&mut fixture, 5.0, IndexRule::Drag), 5);
    assert_eq!(at(&mut fixture, -40.0, IndexRule::Drag), 4);
    // A text that shows whole: past either edge, its end.
    let (_, _, short) = scrolled_field(&mut fixture, "ab");
    for (x, expected) in [(400.0, 2), (-40.0, 0)] {
        let index = fixture
            .tree
            .text_index_at(
                short,
                Point::new(x, 8.0),
                IndexRule::Drag,
                &mut fixture.text,
            )
            .unwrap();
        assert_eq!(index, expected);
    }
    // The grapheme under a point, and its word.
    let (_, _, words) = scrolled_field(&mut fixture, "ab cd");
    let under = fixture
        .tree
        .text_index_at(
            words,
            Point::new(5.0 + 11.0, 8.0),
            IndexRule::Under,
            &mut fixture.text,
        )
        .unwrap();
    assert_eq!(under, 1, "the right half of b is b");
    let word = |fixture: &mut Fixture, index: usize, end: bool| {
        fixture
            .tree
            .text_word(words, index, end, &mut fixture.text)
            .unwrap()
    };
    assert_eq!(
        (word(&mut fixture, 1, false), word(&mut fixture, 1, true)),
        (0, 2)
    );
    assert_eq!(
        (word(&mut fixture, 2, false), word(&mut fixture, 2, true)),
        (2, 3)
    );
    assert_eq!(
        (word(&mut fixture, 5, false), word(&mut fixture, 5, true)),
        (3, 5)
    );
}

// ============================================ F170: scenes in own coordinates ===

/// Glyphs for the encoder without a rasteriser: every glyph an 8 x 10 mask
/// whose top is 8 above the pen. Records what it is asked for, less the text
/// system (two fixtures have two).
struct StandInGlyphs {
    requests: Vec<(u32, u32, u32, u8)>,
}

impl crate::encode::GlyphSource for StandInGlyphs {
    fn glyph(&mut self, key: crate::text::GlyphRasterKey) -> Option<crate::atlas::AtlasGlyph> {
        self.requests
            .push((key.font, key.glyph, key.size_bits, key.subpixel_bin));
        Some(crate::atlas::AtlasGlyph {
            kind: crate::atlas::PageKind::Mask,
            page: 0,
            x: 0,
            y: 0,
            width: 8,
            height: 10,
            left: 0,
            top: 8,
        })
    }

    fn page_size(&self) -> u32 {
        256
    }
}

/// What the encoder makes of a scene: every instance in device pixels, the
/// draw calls, and the glyphs (with their subpixel bins) it asked for.
#[derive(Debug, PartialEq)]
struct Encoded {
    quads: Vec<crate::encode::QuadInstance>,
    sprites: Vec<crate::encode::SpriteInstance>,
    batches: Vec<crate::encode::Batch>,
    glyphs: Vec<(u32, u32, u32, u8)>,
}

fn encoded(scene: &Scene, scale: f32) -> Encoded {
    let mut encoder = crate::encode::Encoder::default();
    let mut frame = crate::encode::EncodedFrame::default();
    let mut glyphs = StandInGlyphs {
        requests: Vec::new(),
    };
    encoder.encode(scene, units(scale), (600, 600), &mut glyphs, &mut frame);
    Encoded {
        quads: frame.quads,
        sprites: frame.sprites,
        batches: frame.batches,
        glyphs: glyphs.requests,
    }
}

fn root_scene(fixture: &mut Fixture, root: NodeId) -> Scene {
    let mut builder = SceneBuilder::new();
    fixture
        .tree
        .paint(root, &mut builder, &mut fixture.text)
        .unwrap();
    builder.finish().unwrap()
}

/// The same primitives with every element scene's offset folded into them:
/// a scene in window coordinates, as the tree painted before F170.
fn in_window_coordinates(scene: &Scene) -> Scene {
    let mut builder = SceneBuilder::new();
    for command in flatten(scene) {
        match command {
            Command::Rectangle(rectangle) => builder.rectangle(rectangle),
            Command::Shadow(shadow) => builder.shadow(shadow),
            Command::Text {
                layout,
                origin,
                color,
            } => builder.text(&layout, origin, color),
            Command::Underline(underline) => builder.underline(underline),
            Command::Image {
                image,
                bounds,
                corner_radii,
            } => builder.image(&image, bounds, corner_radii),
            Command::Path {
                path,
                origin,
                scale,
                paint,
                color,
            } => builder.path(&path, origin, scale, paint, color),
            Command::PushClip {
                bounds,
                corner_radii,
            } => builder.push_clip(bounds, corner_radii),
            Command::PushTransform(transform) => builder.push_transform(transform),
            Command::PushLayer { opacity } => builder.push_layer(opacity),
            Command::Pop => builder.pop(),
            Command::Scene(_) | Command::PlacedScene { .. } => {
                unreachable!("flatten inlines nested scenes")
            }
        }
    }
    builder.finish().unwrap()
}

/// `fractional_screen` and, below it: a bordered card (a 1 dp hairline,
/// rounded, a shadow past its box, clipping a paragraph wider than it), a
/// scroll scrolled by a fractional offset over rows of fractional height,
/// a focused edited text with a selection, and a picture cropped to cover
/// its box (its top 25 dp above the box: a half pixel at 150%, in the
/// element's own coordinates a negative one). Laid out at `scale`.
fn busy_screen(fixture: &mut Fixture, scale: f32) -> NodeId {
    let root = fractional_screen(fixture);
    let card = fixture.block(61.3, 23.7);
    fixture
        .tree
        .set_background(card, Color::new(1.0, 1.0, 1.0, 1.0));
    fixture
        .tree
        .set_border(card, Edges::all(1.0), Color::new(0.0, 0.0, 0.0, 1.0));
    fixture.tree.set_corners(card, Corners::all(4.0));
    fixture.tree.set_shadow(
        card,
        DropShadow {
            offset: Point::new(0.5, 1.5),
            blur: 3.0,
            spread: 0.7,
            color: Color::new(0.0, 0.0, 0.0, 0.5),
        },
    );
    fixture.tree.set_clip(card, true);
    let overflowing = fixture.paragraph("words wider than their card", 9.0);
    fixture.add(card, &[overflowing]);
    let scroll = fixture.element(ElementKind::Scroll);
    fixture
        .tree
        .set_size(scroll, SizeProperty::Width, Length::Dp(80.5))
        .unwrap();
    fixture
        .tree
        .set_size(scroll, SizeProperty::Height, Length::Dp(40.3))
        .unwrap();
    let column = fixture.element(ElementKind::Column);
    for index in 0..9 {
        let row = fixture.block(70.0, 9.3);
        if index % 2 == 0 {
            fixture.tree.set_background(row, blue());
        }
        fixture.add(column, &[row]);
    }
    fixture.add(scroll, &[column]);
    let field = edited(fixture, "select me", 0, false);
    fixture.tree.text_edit_look(field, None, 1.0, None).unwrap();
    fixture.tree.text_edit(field, 6, 2, true).unwrap();
    let picture = fixture.element(ElementKind::Image);
    fixture.tree.set_texture(picture, texture(20, 20)).unwrap();
    fixture.tree.set_fit(picture, Fit::Cover).unwrap();
    for (property, value) in [(SizeProperty::Width, 100.0), (SizeProperty::Height, 50.0)] {
        fixture
            .tree
            .set_size(picture, property, Length::Dp(value))
            .unwrap();
    }
    fixture.add(root, &[card, scroll, field, picture]);
    fixture.layout_at(root, 200.0, 320.0, scale);
    fixture
        .tree
        .set_scroll_offset(scroll, Point::new(0.0, 13.3))
        .unwrap();
    fixture.layout_at(root, 200.0, 320.0, scale);
    root
}

/// F170: each element's scene is in its own coordinates and its parent's
/// places it at a snapped box's corner, so the frame is, instance for
/// instance, the one the same primitives draw in window coordinates —
/// boxes, the hairline, the shadow past the box, the rounded clip, the
/// scroll's offset, glyph bins and baselines, the caret and the selection —
/// at every scale.
#[test]
fn placed_element_scenes_draw_what_window_coordinates_draw() {
    for scale in [1.0, 1.25, 1.5, 2.0] {
        let mut fixture = Fixture::new();
        let root = busy_screen(&mut fixture, scale);
        let scene = root_scene(&mut fixture, root);
        let placed = encoded(&scene, scale);
        assert!(
            placed.quads.len() > 5 && placed.glyphs.len() > 20,
            "too little drawn at {scale} to compare"
        );
        // Not `assert_eq!`: a frame's dump is thousands of lines.
        assert!(
            placed == encoded(&in_window_coordinates(&scene), scale),
            "at {scale}, the placed scenes draw other instances than window coordinates"
        );
    }
}

/// One row of `moving_rows`: a label (11 dp text, a fractional line height)
/// and a 7.3 dp mark; every other row has a background, whose snapped
/// height changes with where the row sits.
fn moving_row(fixture: &mut Fixture, id: usize, tall: bool) -> NodeId {
    let row = fixture.element(ElementKind::Row);
    fixture.tree.set_gap(row, 2.5).unwrap();
    if id.is_multiple_of(2) {
        fixture.tree.set_background(row, blue());
    }
    let label = fixture.paragraph(&format!("row {id}"), 11.0);
    let mark = fixture.block(9.4, if tall { 21.9 } else { 7.3 });
    fixture.tree.set_background(mark, red());
    fixture.add(row, &[label, mark]);
    row
}

/// A column of `moving_row`s, ids in `order`, `tall` the one with a taller
/// mark; laid out at 150% and painted.
fn moving_rows(fixture: &mut Fixture, order: &[usize], tall: Option<usize>) -> NodeId {
    let column = fixture.element(ElementKind::Column);
    fixture.tree.set_padding(column, Edges::all(3.3)).unwrap();
    for id in order {
        let row = moving_row(fixture, *id, tall == Some(*id));
        fixture.add(column, &[row]);
    }
    fixture.layout_at(column, 200.0, 400.0, 1.5);
    column
}

/// F170: rows that moved keep their scenes, and the frame is still the one a
/// tree built fresh in the new order draws — through an insert at the top, a
/// move from the bottom to the top, a removal, and a sibling growing — at
/// 150%, where a moved row's snapped height can change by a pixel (so a row
/// with a background is repainted, and one without is not).
#[test]
fn a_moved_element_draws_what_a_fresh_one_draws() {
    let mut fixture = Fixture::new();
    let mut order: Vec<usize> = (0..12).collect();
    let column = moving_rows(&mut fixture, &order, None);
    let mut rows: Vec<NodeId> = fixture.tree.children(column);
    root_scene(&mut fixture, column);
    let compare = |fixture: &mut Fixture, order: &[usize], tall: Option<usize>, step: &str| {
        fixture.layout_at(column, 200.0, 400.0, 1.5);
        let moved = encoded(&root_scene(fixture, column), 1.5);
        let mut fresh = Fixture::new();
        let fresh_column = moving_rows(&mut fresh, order, tall);
        assert!(
            moved == encoded(&root_scene(&mut fresh, fresh_column), 1.5),
            "after {step}, the kept scenes draw other instances than fresh ones"
        );
    };

    let inserted = moving_row(&mut fixture, 12, false);
    fixture.tree.insert_before(rows[0], inserted).unwrap();
    rows.insert(0, inserted);
    order.insert(0, 12);
    compare(&mut fixture, &order, None, "an insert at the top");

    let last = rows.pop().unwrap();
    fixture.tree.insert_before(rows[0], last).unwrap();
    rows.insert(0, last);
    let id = order.pop().unwrap();
    order.insert(0, id);
    compare(
        &mut fixture,
        &order,
        None,
        "the bottom row moved to the top",
    );

    let removed = rows.remove(3);
    drop(fixture.tree.remove(removed));
    order.remove(3);
    compare(&mut fixture, &order, None, "a removal");

    let grown = order[2];
    let mark = fixture.tree.children(rows[2])[1];
    fixture
        .tree
        .set_size(mark, SizeProperty::Height, Length::Dp(21.9))
        .unwrap();
    compare(&mut fixture, &order, Some(grown), "a sibling grew");
}

/// F170's dirty rule, by counting: a move repaints no element of its own,
/// only the parent that places it; a newcomer is painted once; every row
/// that moved keeps its scene, the same one.
#[test]
fn a_list_operation_rebuilds_the_newcomer_and_the_parent_only() {
    let mut fixture = Fixture::new();
    let (column, _) = list(&mut fixture, 50);
    fixture.layout(column, 300.0, 2000.0);
    paint_stats(&mut fixture, column);
    let kept_scene = |fixture: &Fixture, node: NodeId| {
        fixture
            .tree
            .data(node)
            .scene
            .clone()
            .expect("a painted row keeps its scene")
    };
    let rows = fixture.tree.children(column);
    let watched = rows[30];
    let before = kept_scene(&fixture, watched);

    // Insert at the top: the new row, its two texts, and the column.
    let inserted = fixture.element(ElementKind::Row);
    let label = fixture.paragraph("new", 10.0);
    let value = fixture.paragraph("000", 10.0);
    fixture.add(inserted, &[label, value]);
    fixture.tree.insert_before(rows[0], inserted).unwrap();
    fixture.layout(column, 300.0, 2000.0);
    assert!(
        fixture.tree.last_pass().moved >= 150,
        "every row moved down"
    );
    assert_eq!(paint_stats(&mut fixture, column).rebuilt, 4);
    assert!(
        kept_scene(&fixture, watched).same_as(&before),
        "kept, moved"
    );

    // The bottom row to the top: the column alone.
    fixture.tree.insert_before(inserted, rows[49]).unwrap();
    fixture.layout(column, 300.0, 2000.0);
    assert_eq!(paint_stats(&mut fixture, column).rebuilt, 1);

    // A removal: the column alone.
    drop(fixture.tree.remove(inserted));
    fixture.layout(column, 300.0, 2000.0);
    assert_eq!(paint_stats(&mut fixture, column).rebuilt, 1);

    // A sibling grows (taller, its texts as wide as before): it (its texts
    // moved in it) and the column.
    fixture
        .tree
        .set_padding(
            rows[10],
            Edges {
                top: 4.0,
                right: 0.0,
                bottom: 4.0,
                left: 0.0,
            },
        )
        .unwrap();
    fixture.layout(column, 300.0, 2000.0);
    assert_eq!(paint_stats(&mut fixture, column).rebuilt, 2);
    assert!(kept_scene(&fixture, watched).same_as(&before));
}

/// F170: a box moved inside a parent whose box did not change (a fixed-size
/// row whose gap grew, with no look of its own) is placed anew by that
/// parent: the parent's scene is rebuilt, the moved child's is kept.
#[test]
fn a_box_moved_inside_an_unchanged_parent_is_placed_anew() {
    let spaced = |fixture: &mut Fixture, gap: f32| {
        let row = row(fixture, 200.0, 40.0);
        fixture.tree.set_gap(row, gap).unwrap();
        let (first, second) = (fixture.block(30.0, 10.0), fixture.block(30.0, 10.0));
        fixture.tree.set_background(first, red());
        fixture.tree.set_background(second, blue());
        fixture.add(row, &[first, second]);
        (row, second)
    };
    let mut fixture = Fixture::new();
    let (row, second) = spaced(&mut fixture, 4.0);
    fixture.layout(row, 300.0, 100.0);
    root_scene(&mut fixture, row);
    let before = fixture.tree.data(second).scene.clone().unwrap();
    fixture.tree.set_gap(row, 25.0).unwrap();
    fixture.layout(row, 300.0, 100.0);
    assert_eq!(fixture.rect(second).0, 55.0);
    let moved = encoded(&root_scene(&mut fixture, row), 1.0);
    assert_eq!(fixture.tree.last_pass().rebuilt, 1, "the row alone");
    assert!(
        fixture
            .tree
            .data(second)
            .scene
            .as_ref()
            .unwrap()
            .same_as(&before)
    );
    let mut fresh = Fixture::new();
    let (fresh_row, _) = spaced(&mut fresh, 25.0);
    fresh.layout(fresh_row, 300.0, 100.0);
    assert!(moved == encoded(&root_scene(&mut fresh, fresh_row), 1.0));
}
