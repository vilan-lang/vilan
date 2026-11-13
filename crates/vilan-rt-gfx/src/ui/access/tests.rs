//! Pins of `access.rs`: the conversion (structure, roles, names, states,
//! bounds at a scale and under a scroll, a text input's run and selection),
//! the publisher's updates, and each action's plan carried out at the tree
//! level the way the host carries it out.
//!
//! Text is shaped with the test font (Noto Mono, every advance 0.6 em).

use std::cell::RefCell;
use std::rc::Rc;

use accesskit::{Action, ActionData, ActionRequest, ScrollUnit, TextPosition, TextSelection};

use super::*;
use crate::geometry::{Edges, Size};
use crate::text::TextSystem;
use crate::ui::{EventKind, Handler, Input, Length, Route, ScrollAxis, SizeProperty};
use crate::units::Units;

fn text_system() -> TextSystem {
    let mut system = TextSystem::without_system_fonts();
    let font = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fonts/NotoMono-Regular.ttf"),
    )
    .expect("the test font");
    system.register_font(font).expect("the test font registers");
    system
}

struct Fixture {
    tree: UiTree,
    text: TextSystem,
    log: Rc<RefCell<Vec<String>>>,
}

impl Fixture {
    fn new() -> Fixture {
        Fixture {
            tree: UiTree::new(),
            text: text_system(),
            log: Rc::new(RefCell::new(Vec::new())),
        }
    }

    fn element(&mut self, kind: ElementKind) -> NodeId {
        self.tree.create(kind)
    }

    fn sized(&mut self, kind: ElementKind, width: f32, height: f32) -> NodeId {
        let node = self.element(kind);
        self.tree
            .set_size(node, SizeProperty::Width, Length::Dp(width))
            .unwrap();
        self.tree
            .set_size(node, SizeProperty::Height, Length::Dp(height))
            .unwrap();
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

    fn layout(&mut self, root: NodeId) {
        self.tree
            .layout(
                root,
                Size::new(400.0, 300.0),
                Units::new(1.0, 1.0).unwrap(),
                &mut self.text,
            )
            .unwrap();
    }

    /// A button: a focusable row with the role, a fixed size, a click
    /// handler that logs `name`.
    fn button(&mut self, name: &str, width: f32) -> NodeId {
        let node = self.sized(ElementKind::Row, width, 20.0);
        self.tree.set_role(node, Role::Button);
        self.tree.set_tab_index(node, Some(0));
        self.listen(node, EventKind::Click, &format!("click {name}"));
        node
    }

    fn listen(&mut self, node: NodeId, kind: EventKind, line: &str) {
        let log = Rc::clone(&self.log);
        let line = line.to_string();
        self.tree.add_handler(
            node,
            kind,
            Handler::new(move || log.borrow_mut().push(line.clone())),
        );
    }

    fn listing(&self, root: NodeId) -> String {
        access_listing(&self.tree.access_tree(Some(root), window(1.0)), true)
    }

    fn run(&mut self, root: NodeId, route: Route) {
        for step in route.steps {
            for delivery in &step.deliveries {
                delivery.handler.call();
            }
            if let Some(action) = step.default {
                let more = self.tree.perform(root, action);
                self.run(root, more);
            }
        }
    }

    /// Carries out an action's plan as the host does (`host/access.rs`): a
    /// focus and a reveal at once, the rest as window input routed into
    /// the tree. Answers the handlers' log.
    fn act(&mut self, root: NodeId, request: ActionRequest) -> Vec<String> {
        let steps = self.tree.plan_access_action(root, &request);
        for step in steps {
            let inputs = match step {
                AccessStep::Focus(node) => {
                    for delivery in self.tree.focus(Some(node)) {
                        delivery.handler.call();
                    }
                    Vec::new()
                }
                AccessStep::Reveal(node) => {
                    self.tree.reveal_when_laid_out(node);
                    Vec::new()
                }
                AccessStep::Click(node) => match self.tree.access_click_point(root, node) {
                    Some(point) => vec![
                        Input::PointerMoved(point),
                        Input::Pressed {
                            position: point,
                            time: std::time::Duration::ZERO,
                        },
                        Input::Released(point),
                    ],
                    None => Vec::new(),
                },
                AccessStep::Key { name, shift, .. } => {
                    self.log.borrow_mut().push(format!("key {name}"));
                    vec![Input::Key {
                        name: name.to_string(),
                        shift,
                    }]
                }
                AccessStep::Text(text) => {
                    self.log.borrow_mut().push(format!("text {text}"));
                    vec![Input::Text]
                }
                AccessStep::Wheel { node, delta } => {
                    match self.tree.access_click_point(root, node) {
                        Some(position) => vec![Input::Scrolled { position, delta }],
                        None => Vec::new(),
                    }
                }
            };
            for input in inputs {
                let route = self.tree.route(root, &input);
                self.run(root, route);
            }
        }
        std::mem::take(&mut *self.log.borrow_mut())
    }
}

fn window(scale: f64) -> AccessWindow<'static> {
    AccessWindow {
        title: "pins",
        scale,
        width: 400.0,
        height: 300.0,
    }
}

fn request(action: Action, node: NodeId, data: Option<ActionData>) -> ActionRequest {
    ActionRequest {
        action,
        target_tree: TreeId::ROOT,
        target_node: access_id(node),
        data,
    }
}

fn node_of(tree: &AccessTree, node: NodeId) -> &AccessNode {
    &tree
        .nodes
        .iter()
        .find(|(id, _)| *id == access_id(node))
        .expect("published")
        .1
}

// ---------------------------------------------------------- conversion ---

/// A screen with one of each rule: a labelled root, a heading, an
/// unlabelled row (left out, its children hoisted), a button named by its
/// text (which is not published on its own), a decorative image and a
/// labelled one, a marker, an empty text, a disabled checked checkbox.
#[test]
fn each_element_is_published_by_its_rule_and_the_rest_left_out() {
    let mut fixture = Fixture::new();
    let root = fixture.element(ElementKind::Column);
    fixture.tree.set_label(root, "screen");
    let heading = fixture.text("Title");
    fixture.tree.set_role(heading, Role::Heading);
    let bare = fixture.element(ElementKind::Row);
    let save = fixture.button("save", 60.0);
    fixture.tree.set_label(save, "");
    let save_text = fixture.text("Save");
    fixture.add(save, &[save_text]);
    let decorative = fixture.sized(ElementKind::Image, 10.0, 10.0);
    let meaningful = fixture.sized(ElementKind::Image, 10.0, 10.0);
    fixture.tree.set_label(meaningful, "logo");
    let marker = fixture.element(ElementKind::Marker);
    let empty = fixture.text("");
    let check = fixture.button("check", 20.0);
    fixture.tree.set_role(check, Role::Checkbox);
    fixture.tree.set_label(check, "agree");
    fixture.tree.set_checked(check, Some(true));
    fixture.tree.set_disabled(check, true);
    let counted = fixture.text("3");
    fixture.tree.set_label(counted, "count");
    fixture.add(bare, &[save, decorative, meaningful, marker, empty]);
    fixture.add(root, &[heading, bare, check, counted]);
    fixture.layout(root);
    assert_eq!(
        fixture.listing(root),
        concat!(
            "Window \"pins\"\n",
            "  Group \"screen\"\n",
            "    Heading \"Title\"\n",
            "    Button \"Save\" [click focus]\n",
            "    Image \"logo\"\n",
            "    CheckBox \"agree\" checked disabled\n",
            "    Label \"count\" value=\"3\"\n",
        )
    );
}

#[test]
fn a_list_holding_some_of_its_items_numbers_them_in_the_whole_list() {
    let mut fixture = Fixture::new();
    let list = fixture.element(ElementKind::Column);
    fixture.tree.set_role(list, Role::List);
    let marker = fixture.element(ElementKind::Marker);
    let first = fixture.text("forty");
    let second = fixture.text("forty-one");
    fixture.add(list, &[first, second, marker]);
    fixture.tree.set_items_shown(list, 40, 1000);
    fixture.layout(list);
    assert_eq!(
        fixture.listing(list),
        concat!(
            "Window \"pins\"\n",
            "  List\n",
            "    ListItem \"forty\" item=41/1000\n",
            "    ListItem \"forty-one\" item=42/1000\n",
        )
    );
}

/// Bounds are dp; the window node carries the scale, so a scale change
/// changes one node.
#[test]
fn bounds_are_dp_under_a_window_that_carries_the_scale() {
    let mut fixture = Fixture::new();
    let root = fixture.element(ElementKind::Column);
    fixture.tree.set_padding(root, Edges::all(10.0)).unwrap();
    let button = fixture.button("ok", 50.0);
    fixture.add(root, &[button]);
    fixture.layout(root);
    let tree = fixture.tree.access_tree(Some(root), window(1.5));
    assert_eq!(
        node_of(&tree, button).bounds(),
        Some(AccessRect::new(10.0, 10.0, 60.0, 30.0))
    );
    let window_node = &tree
        .nodes
        .iter()
        .find(|(id, _)| *id == WINDOW_NODE)
        .unwrap()
        .1;
    assert_eq!(window_node.transform(), Some(&Affine::scale(1.5)));
    let mut publisher = AccessPublisher::default();
    publisher.update(tree, true);
    let rescaled = publisher.update(fixture.tree.access_tree(Some(root), window(2.0)), false);
    assert_eq!(
        rescaled.nodes.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        vec![WINDOW_NODE]
    );
}

/// A scroll's offset is its transform: its children keep their laid-out
/// bounds, and its own box stays where it is drawn.
#[test]
fn a_scroll_carries_its_offset_as_its_transform() {
    let mut fixture = Fixture::new();
    let root = fixture.element(ElementKind::Column);
    let scroll = fixture.sized(ElementKind::Scroll, 100.0, 50.0);
    let content = fixture.element(ElementKind::Column);
    let rows: Vec<NodeId> = (0..5)
        .map(|index| fixture.button(&format!("row {index}"), 100.0))
        .collect();
    fixture.add(content, &rows);
    fixture.add(scroll, &[content]);
    fixture.add(root, &[scroll]);
    fixture.layout(root);
    fixture
        .tree
        .set_scroll_offset(scroll, Point::new(0.0, 30.0))
        .unwrap();
    let tree = fixture.tree.access_tree(Some(root), window(1.0));
    let scrolled = node_of(&tree, scroll);
    assert_eq!(scrolled.transform(), Some(&Affine::translate((0.0, -30.0))));
    // Its box, moved by the transform, is where it is laid out.
    assert_eq!(
        scrolled.bounds(),
        Some(AccessRect::new(0.0, 30.0, 100.0, 80.0))
    );
    assert_eq!(
        node_of(&tree, rows[2]).bounds(),
        Some(AccessRect::new(0.0, 40.0, 100.0, 60.0))
    );
    assert_eq!(
        (scrolled.scroll_y(), scrolled.scroll_y_max()),
        (Some(30.0), Some(50.0))
    );
    assert!(scrolled.supports_action(Action::ScrollUp));
    assert!(scrolled.supports_action(Action::ScrollDown));
}

/// The focus is the focused element, or its nearest published ancestor,
/// or the window.
#[test]
fn the_focus_is_the_nearest_published_element_or_the_window() {
    let mut fixture = Fixture::new();
    let root = fixture.element(ElementKind::Column);
    let button = fixture.button("ok", 50.0);
    let inner = fixture.sized(ElementKind::Row, 10.0, 10.0);
    fixture.tree.set_tab_index(inner, Some(-1));
    fixture.add(button, &[inner]);
    fixture.add(root, &[button]);
    fixture.layout(root);
    assert_eq!(
        fixture.tree.access_tree(Some(root), window(1.0)).focus,
        WINDOW_NODE
    );
    fixture.tree.focus(Some(inner));
    assert_eq!(
        fixture.tree.access_tree(Some(root), window(1.0)).focus,
        access_id(button)
    );
}

/// An edited text in a text field: its value, its graphemes (an accent
/// with its letter, an emoji's surrogate pair) and its selection in them.
fn text_field(fixture: &mut Fixture, value: &str) -> (NodeId, NodeId, NodeId) {
    let root = fixture.element(ElementKind::Column);
    let field = fixture.sized(ElementKind::Row, 300.0, 30.0);
    fixture.tree.set_role(field, Role::TextField);
    fixture.tree.set_label(field, "name");
    fixture.tree.set_tab_index(field, Some(0));
    let edited = fixture.text(value);
    fixture.tree.text_edit(edited, 0, 0, false).unwrap();
    fixture.add(field, &[edited]);
    fixture.add(root, &[field]);
    fixture.layout(root);
    (root, field, edited)
}

#[test]
fn a_text_input_publishes_its_value_its_graphemes_and_its_selection() {
    let mut fixture = Fixture::new();
    // "é" as e + U+0301 (one grapheme, three bytes), then "x", then 😀 (four
    // bytes, two UTF-16 units).
    let (root, field, edited) = text_field(&mut fixture, "e\u{301}x\u{1F600}");
    // Caret after "x" (UTF-16 index 3), anchor at the end (index 5).
    fixture.tree.text_edit(edited, 3, 5, true).unwrap();
    fixture.layout(root);
    let tree = fixture.tree.access_tree(Some(root), window(1.0));
    let published = node_of(&tree, field);
    assert_eq!(published.value(), Some("e\u{301}x\u{1F600}"));
    let run = node_of(&tree, edited);
    assert_eq!(run.role(), AccessRole::TextRun);
    assert_eq!(run.character_lengths(), &[3, 1, 4]);
    let selection = published.text_selection().expect("a selection");
    assert_eq!(
        (
            selection.anchor.character_index,
            selection.focus.character_index
        ),
        (3, 2)
    );
    assert_eq!(selection.anchor.node, access_id(edited));
}

// ---------------------------------------------------------- publishing ---

#[test]
fn an_update_carries_only_what_changed_and_the_first_carries_everything() {
    let mut fixture = Fixture::new();
    let root = fixture.element(ElementKind::Column);
    fixture.tree.set_label(root, "root");
    let first = fixture.button("first", 50.0);
    let second = fixture.button("second", 50.0);
    fixture.tree.set_label(first, "first");
    fixture.tree.set_label(second, "second");
    fixture.add(root, &[first, second]);
    fixture.layout(root);
    let mut publisher = AccessPublisher::default();
    let whole = publisher.update(fixture.tree.access_tree(Some(root), window(1.0)), true);
    assert_eq!(whole.nodes.len(), 4);
    assert!(whole.tree.is_some());
    let unchanged = publisher.update(fixture.tree.access_tree(Some(root), window(1.0)), false);
    assert!(unchanged.nodes.is_empty() && unchanged.tree.is_none());
    fixture.tree.set_label(second, "renamed");
    let renamed = publisher.update(fixture.tree.access_tree(Some(root), window(1.0)), false);
    assert_eq!(
        renamed.nodes.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        vec![access_id(second)]
    );
    // Removed: its parent changes; it comes back whole if it returns.
    fixture.tree.remove(second);
    let removed = publisher.update(fixture.tree.access_tree(Some(root), window(1.0)), false);
    assert_eq!(
        removed.nodes.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        vec![access_id(root)]
    );
    let again = publisher.update(fixture.tree.access_tree(Some(root), window(1.0)), true);
    assert_eq!(again.nodes.len(), 3);
    assert!(again.tree.is_some());
}

// ------------------------------------------------------------- actions ---

#[test]
fn focus_moves_the_focus_as_a_press_would_and_a_disabled_control_refuses_it() {
    let mut fixture = Fixture::new();
    let root = fixture.element(ElementKind::Column);
    let enabled = fixture.button("enabled", 50.0);
    let disabled = fixture.button("disabled", 50.0);
    fixture.tree.set_disabled(disabled, true);
    fixture.listen(enabled, EventKind::Focus, "focus enabled");
    fixture.add(root, &[enabled, disabled]);
    fixture.layout(root);
    assert_eq!(
        fixture.act(root, request(Action::Focus, enabled, None)),
        vec!["focus enabled"]
    );
    assert_eq!(fixture.tree.focused(), Some(enabled));
    assert_eq!(
        fixture
            .tree
            .plan_access_action(root, &request(Action::Focus, disabled, None)),
        Vec::new()
    );
}

#[test]
fn click_is_a_press_and_release_on_the_control_and_nothing_else() {
    let mut fixture = Fixture::new();
    let root = fixture.element(ElementKind::Row);
    let first = fixture.button("first", 50.0);
    let second = fixture.button("second", 50.0);
    fixture.add(root, &[first, second]);
    fixture.layout(root);
    assert_eq!(
        fixture.act(root, request(Action::Click, second, None)),
        vec!["click second"]
    );
    // A press focuses it, as a pointer's does.
    assert_eq!(fixture.tree.focused(), Some(second));
    fixture.tree.set_disabled(second, true);
    assert_eq!(
        fixture.act(root, request(Action::Click, second, None)),
        Vec::<String>::new()
    );
}

/// A control scrolled out of sight is scrolled into view first, then
/// clicked where it is drawn.
#[test]
fn click_on_a_control_scrolled_away_reveals_it_first() {
    let mut fixture = Fixture::new();
    let root = fixture.element(ElementKind::Column);
    let scroll = fixture.sized(ElementKind::Scroll, 100.0, 40.0);
    let content = fixture.element(ElementKind::Column);
    let rows: Vec<NodeId> = (0..10)
        .map(|index| fixture.button(&format!("row {index}"), 100.0))
        .collect();
    fixture.add(content, &rows);
    fixture.add(scroll, &[content]);
    fixture.add(root, &[scroll]);
    fixture.layout(root);
    assert_eq!(fixture.tree.access_click_point(root, rows[7]), None);
    assert_eq!(
        fixture.act(root, request(Action::Click, rows[7], None)),
        vec!["click row 7"]
    );
    assert_eq!(
        fixture.tree.scroll_offset(scroll),
        Some(Point::new(0.0, 120.0))
    );
}

/// A control under another element is not clicked: a pointer there would
/// reach the cover.
#[test]
fn click_on_a_covered_control_does_nothing() {
    let mut fixture = Fixture::new();
    let root = fixture.element(ElementKind::Stack);
    let under = fixture.button("under", 50.0);
    let cover = fixture.sized(ElementKind::Row, 50.0, 20.0);
    fixture.listen(cover, EventKind::Click, "click cover");
    fixture.add(root, &[under, cover]);
    fixture.layout(root);
    assert_eq!(
        fixture.act(root, request(Action::Click, under, None)),
        Vec::<String>::new()
    );
}

#[test]
fn set_value_selects_everything_and_types_over_it() {
    let mut fixture = Fixture::new();
    let (root, field, _) = text_field(&mut fixture, "old");
    fixture.listen(field, EventKind::Focus, "focus field");
    fixture.listen(field, EventKind::Key, "key heard");
    fixture.listen(field, EventKind::Text, "text heard");
    assert_eq!(
        fixture.act(
            root,
            request(
                Action::SetValue,
                field,
                Some(ActionData::Value("new".into()))
            )
        ),
        vec![
            "focus field",
            "key a",
            "key heard",
            "text new",
            "text heard"
        ]
    );
    assert_eq!(
        fixture.tree.plan_access_action(
            root,
            &request(Action::SetValue, field, Some(ActionData::Value("".into())))
        ),
        vec![
            AccessStep::Key {
                name: "a",
                named: false,
                shift: false,
                control: true,
            },
            AccessStep::Key {
                name: "Delete",
                named: true,
                shift: false,
                control: false,
            },
        ]
    );
}

#[test]
fn set_text_selection_walks_the_caret_keys_grapheme_by_grapheme() {
    let mut fixture = Fixture::new();
    let (root, field, edited) = text_field(&mut fixture, "e\u{301}x\u{1F600}");
    fixture.tree.focus(Some(field));
    let at = |index: usize| TextPosition {
        node: access_id(edited),
        character_index: index,
    };
    let steps = fixture.tree.plan_access_action(
        root,
        &request(
            Action::SetTextSelection,
            field,
            Some(ActionData::SetTextSelection(TextSelection {
                anchor: at(3),
                focus: at(1),
            })),
        ),
    );
    let keys: Vec<String> = steps
        .iter()
        .map(|step| match step {
            AccessStep::Key { name, shift, .. } => {
                format!("{}{name}", if *shift { "+" } else { "" })
            }
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(
        keys,
        vec![
            "Home",
            "ArrowRight",
            "ArrowRight",
            "ArrowRight",
            "+ArrowLeft",
            "+ArrowLeft"
        ]
    );
    // A selection in another element's run is refused.
    let foreign = TextPosition {
        node: access_id(root),
        character_index: 0,
    };
    assert_eq!(
        fixture.tree.plan_access_action(
            root,
            &request(
                Action::SetTextSelection,
                field,
                Some(ActionData::SetTextSelection(TextSelection {
                    anchor: foreign,
                    focus: foreign,
                })),
            ),
        ),
        Vec::new()
    );
}

#[test]
fn scroll_down_is_a_wheel_step_of_a_page_or_an_item() {
    let mut fixture = Fixture::new();
    let root = fixture.element(ElementKind::Column);
    let scroll = fixture.sized(ElementKind::Scroll, 100.0, 100.0);
    fixture
        .tree
        .set_scroll_axis(scroll, ScrollAxis::Vertical)
        .unwrap();
    let content = fixture.sized(ElementKind::Column, 100.0, 1000.0);
    // A scroll's child keeps its own height only when it may not shrink.
    fixture.tree.set_shrink(content, 0.0).unwrap();
    fixture.add(scroll, &[content]);
    fixture.add(root, &[scroll]);
    fixture.layout(root);
    fixture.listen(scroll, EventKind::Scroll, "wheel");
    assert_eq!(
        fixture.act(root, request(Action::ScrollDown, scroll, None)),
        vec!["wheel"]
    );
    assert_eq!(
        fixture.tree.scroll_offset(scroll),
        Some(Point::new(0.0, 68.0))
    );
    fixture.act(
        root,
        request(
            Action::ScrollUp,
            scroll,
            Some(ActionData::ScrollUnit(ScrollUnit::Item)),
        ),
    );
    assert_eq!(
        fixture.tree.scroll_offset(scroll),
        Some(Point::new(0.0, 36.0))
    );
}

#[test]
fn an_action_outside_the_tree_or_on_the_window_plans_nothing() {
    let mut fixture = Fixture::new();
    let root = fixture.element(ElementKind::Column);
    let button = fixture.button("ok", 50.0);
    fixture.add(root, &[button]);
    let elsewhere = fixture.element(ElementKind::Column);
    let stranger = fixture.button("stranger", 50.0);
    fixture.add(elsewhere, &[stranger]);
    fixture.layout(root);
    let window_request = ActionRequest {
        action: Action::Focus,
        target_tree: TreeId::ROOT,
        target_node: WINDOW_NODE,
        data: None,
    };
    assert_eq!(
        fixture.tree.plan_access_action(root, &window_request),
        Vec::new()
    );
    assert_eq!(
        fixture
            .tree
            .plan_access_action(root, &request(Action::Click, stranger, None)),
        Vec::new()
    );
    assert_eq!(
        fixture
            .tree
            .plan_access_action(root, &request(Action::ScrollIntoView, button, None)),
        vec![AccessStep::Reveal(button)]
    );
}

/// A grapheme a font change splits is two boxes: one character, its
/// advance the sum; graphemes drawn right to left are listed in text order.
#[test]
fn graphemes_are_listed_once_each_in_text_order() {
    use crate::text::editing::{GraphemeBox, LineBox};
    let grapheme = |start: usize, end: usize, x: f32, width: f32| GraphemeBox {
        start,
        end,
        x,
        width,
        line: 0,
        rtl: false,
        word: true,
    };
    let geometry = EditingGeometry {
        text_len: 7,
        // Visual order: the last grapheme drawn first (right to left), and
        // the first one split in two by a font change.
        graphemes: vec![
            grapheme(5, 7, 0.0, 6.0),
            grapheme(0, 3, 6.0, 4.0),
            grapheme(0, 3, 10.0, 2.0),
            grapheme(3, 5, 12.0, 6.0),
        ],
        lines: vec![LineBox {
            top: 0.0,
            height: 10.0,
        }],
    };
    assert_eq!(
        logical_graphemes(&geometry),
        vec![(0, 3, 6.0, 6.0), (3, 5, 12.0, 6.0), (5, 7, 0.0, 6.0)]
    );
}
