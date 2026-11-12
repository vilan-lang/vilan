//! Interaction (native-ui.md section 4.5, part P3): hit testing over the
//! snapped boxes, the hover / press / focus state the style conditions read,
//! event routing with bubbling, the tab order, and scrolling.
//!
//! # Routing
//!
//! [`UiTree::route`] takes one window input and answers a [`Route`]: the
//! handlers to call, in order, in STEPS. A step is one event: a pointer
//! entering and leaving elements, a focus moving, or one bubbling event (a
//! press, a click, a key) delivered to its target first and then to every
//! ancestor that listens. A handler of a bubbling event may STOP it: no
//! ancestor after it hears it, and the framework's own response to it — Tab
//! moving the focus, a scroll moving its child — does not happen. Enter,
//! leave, focus and blur do not bubble and cannot be stopped.
//!
//! The tree does not call handlers: the host does, with nothing of the tree
//! borrowed (a handler sets properties, which change the tree), between
//! steps checking whether one stopped the event. The interaction state is
//! changed by `route` itself, so a handler already reads the new state, and
//! a condition on it has already restyled its element.
//!
//! # The events
//!
//! | Event | Target | Bubbles |
//! |---|---|---|
//! | pointer enter, leave | each element the pointer came into or left (an element and its ancestors are all under the pointer) | no |
//! | pointer move | the element under the pointer | yes |
//! | press, release | the element under the pointer, the primary button only | yes |
//! | click | after a release, the innermost element holding both where the press and the release were | yes |
//! | scroll | the element under the pointer; the first `scroll` on the way up that can move takes it | yes, until a scroll moves |
//! | key, key release, text | the focused element, or the root when nothing is | yes |
//! | focus, blur | the element gaining or losing the focus | no |
//!
//! A press focuses the innermost focusable element at or above where it
//! landed, and takes the focus away when there is none. Tab moves the focus
//! to the next element of the tab order, Shift+Tab to the previous one,
//! wrapping at the ends.
//!
//! # Hit testing
//!
//! Back to front: a later sibling is above an earlier one, a child above its
//! parent. An element is hit inside its box with its corners rounded; a
//! clipping element (`clip`, every `scroll`) has its children hit only
//! inside its padding box, and a scroll's children are hit where they are
//! drawn, moved by its offset. Every element is hit, whatever it paints (a
//! row with no background still has a box).

use std::rc::Rc;

use taffy::TraversePartialTree;

use super::style::Interaction;
use super::{ElementKind, NodeData, NodeId, ScrollAxis, UiTree, clamp_offset, inside_rounded};
use crate::geometry::{Point, Rect};

/// The offset along one axis that shows the span `start..end` (relative to a
/// viewport of length `length` scrolled to `offset`) with the least movement:
/// `offset` itself when it shows the span already; the span's start when the
/// span lies before the viewport or is longer than it; its end at the
/// viewport's end otherwise.
fn shown(start: f32, end: f32, length: f32, offset: f32) -> f32 {
    if start < offset || end - start > length {
        start
    } else if end > offset + length {
        end - length
    } else {
        offset
    }
}

/// A vilan handler, as the tree keeps it: a `|| void` closure the host calls.
#[derive(Clone)]
pub struct Handler(Rc<dyn Fn()>);

impl Handler {
    pub fn new(body: impl Fn() + 'static) -> Handler {
        Handler(Rc::new(body))
    }

    pub fn from_rc(body: Rc<dyn Fn()>) -> Handler {
        Handler(body)
    }

    pub fn call(&self) {
        (self.0)()
    }
}

impl std::fmt::Debug for Handler {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Handler")
    }
}

/// The events an element can listen for (the module documentation's table).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    PointerEnter,
    PointerLeave,
    PointerMove,
    Press,
    Release,
    Click,
    Scroll,
    Key,
    KeyRelease,
    Text,
    Focus,
    Blur,
}

/// One window input, as the tree routes it. Positions are dp from the
/// root's top-left corner. Only the primary button's presses and releases
/// are inputs: another button reaches the window's own event callback only.
#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    PointerMoved(Point),
    Pressed(Point),
    Released(Point),
    PointerLeft,
    /// `delta` as `std::gfx` reports it: positive means the content should
    /// move right and down.
    Scrolled {
        position: Point,
        delta: Point,
    },
    /// A key went down: its name as `std::gfx` gives it (`"Tab"`, `"a"`).
    Key {
        name: String,
        shift: bool,
    },
    KeyReleased,
    Text,
}

/// A handler to call, and for what.
#[derive(Clone, Debug)]
pub struct Delivery {
    pub node: NodeId,
    pub kind: EventKind,
    pub handler: Handler,
}

/// What happens when a step's event is not stopped.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DefaultAction {
    MoveFocus { backward: bool },
    Scroll { node: NodeId, delta: Point },
}

/// One event's handlers, in order, and whether one of them may stop it.
#[derive(Clone, Debug, Default)]
pub struct Step {
    pub deliveries: Vec<Delivery>,
    pub stoppable: bool,
    pub default: Option<DefaultAction>,
}

/// What an input does: see the module documentation.
#[derive(Clone, Debug, Default)]
pub struct Route {
    pub steps: Vec<Step>,
}

impl Route {
    fn plain(&mut self, deliveries: Vec<Delivery>) {
        if !deliveries.is_empty() {
            self.steps.push(Step {
                deliveries,
                stoppable: false,
                default: None,
            });
        }
    }

    fn bubbling(&mut self, deliveries: Vec<Delivery>, default: Option<DefaultAction>) {
        if !deliveries.is_empty() || default.is_some() {
            self.steps.push(Step {
                deliveries,
                stoppable: true,
                default,
            });
        }
    }

    /// Every delivery, in order: what a test reads.
    pub fn deliveries(&self) -> impl Iterator<Item = &Delivery> {
        self.steps.iter().flat_map(|step| step.deliveries.iter())
    }
}

#[derive(Clone, Copy)]
enum Flag {
    Hovered,
    Pressed,
    Focused,
}

impl UiTree {
    // --------------------------------------------------------- listening ---

    /// Adds a handler for `kind` on `node`, after the ones it has.
    pub fn add_handler(&mut self, node: NodeId, kind: EventKind, handler: Handler) {
        self.data_mut(node).handlers.push((kind, handler));
    }

    /// Makes `node` focusable at `index` in the tab order (see `NodeData`'s
    /// `tab_index`), or not focusable (`None`).
    pub fn set_tab_index(&mut self, node: NodeId, index: Option<i32>) {
        self.data_mut(node).tab_index = index;
    }

    pub fn focused(&self) -> Option<NodeId> {
        self.focused.filter(|node| self.is_alive(*node))
    }

    pub fn hovered(&self) -> Option<NodeId> {
        self.hovered.filter(|node| self.is_alive(*node))
    }

    pub fn pressed(&self) -> Option<NodeId> {
        self.pressed.filter(|node| self.is_alive(*node))
    }

    pub fn interaction(&self, node: NodeId) -> Interaction {
        self.data(node).interaction
    }

    // ------------------------------------------------------- hit testing ---

    /// The element under `point` in `root`'s tree, from its last layout: the
    /// module documentation's rule.
    pub fn hit(&self, root: NodeId, point: Point) -> Option<NodeId> {
        self.hit_node(root, point)
    }

    fn hit_node(&self, node: NodeId, point: Point) -> Option<NodeId> {
        let data = self.data(node);
        if data.kind == ElementKind::Marker {
            return None;
        }
        let inside = inside_rounded(data.bounds, data.paint.corners, point);
        let children_reachable = if data.clip {
            let (area, corners) = super::padding_box(data);
            inside && inside_rounded(area, corners, point)
        } else {
            true
        };
        if children_reachable {
            let inner = match &data.scroll {
                Some(scroll) => Point::new(point.x + scroll.offset.x, point.y + scroll.offset.y),
                None => point,
            };
            for index in (0..self.taffy.child_count(node)).rev() {
                let child = self.taffy.get_child_id(node, index);
                if let Some(hit) = self.hit_node(child, inner) {
                    return Some(hit);
                }
            }
        }
        inside.then_some(node)
    }

    /// `node`, then each ancestor up to its root.
    fn chain(&self, node: NodeId) -> Vec<NodeId> {
        let mut chain = vec![node];
        let mut current = node;
        while let Some(parent) = self.taffy.parent(current) {
            if self.kind(parent) == ElementKind::Viewport {
                break;
            }
            chain.push(parent);
            current = parent;
        }
        chain
    }

    /// `node`'s handlers for `kind`, then each ancestor's.
    fn bubble(&self, node: NodeId, kind: EventKind) -> Vec<Delivery> {
        let mut deliveries = Vec::new();
        for node in self.chain(node) {
            self.listeners(node, kind, &mut deliveries);
        }
        deliveries
    }

    fn listeners(&self, node: NodeId, kind: EventKind, deliveries: &mut Vec<Delivery>) {
        for (listened, handler) in &self.data(node).handlers {
            if *listened == kind {
                deliveries.push(Delivery {
                    node,
                    kind,
                    handler: handler.clone(),
                });
            }
        }
    }

    /// Sets `flag` to `value` on `node`, restyling it when a condition of
    /// its style reads the change.
    fn set_flag(&mut self, node: NodeId, flag: Flag, value: bool) {
        let data: &mut NodeData = self.data_mut(node);
        let slot = match flag {
            Flag::Hovered => &mut data.interaction.hovered,
            Flag::Pressed => &mut data.interaction.pressed,
            Flag::Focused => &mut data.interaction.focused,
        };
        if *slot == value {
            return;
        }
        *slot = value;
        if data.layers.reads_interaction() {
            self.restyle(node);
        }
    }

    /// `node`'s chain, if it is alive and in `root`'s tree.
    fn chain_in(&self, root: NodeId, node: Option<NodeId>) -> Vec<NodeId> {
        match node.filter(|node| self.is_alive(*node)) {
            Some(node) => {
                let chain = self.chain(node);
                if chain.last() == Some(&root) {
                    chain
                } else {
                    Vec::new()
                }
            }
            None => Vec::new(),
        }
    }

    /// The pointer is over `hit` now: leaves (deepest first), then enters
    /// (outermost first).
    fn hover(&mut self, root: NodeId, hit: Option<NodeId>, route: &mut Route) {
        let old = self.chain_in(root, self.hovered);
        let new = hit.map(|hit| self.chain(hit)).unwrap_or_default();
        let mut deliveries = Vec::new();
        for node in &old {
            if !new.contains(node) {
                self.set_flag(*node, Flag::Hovered, false);
                self.listeners(*node, EventKind::PointerLeave, &mut deliveries);
            }
        }
        for node in new.iter().rev() {
            if !old.contains(node) {
                self.set_flag(*node, Flag::Hovered, true);
                self.listeners(*node, EventKind::PointerEnter, &mut deliveries);
            }
        }
        self.hovered = hit;
        route.plain(deliveries);
    }

    // ------------------------------------------------------------- focus ---

    /// Moves the focus to `target` (or takes it away): the blur of the
    /// element that had it, then the focus of the one that has it.
    pub fn focus(&mut self, target: Option<NodeId>) -> Vec<Delivery> {
        let target = target.filter(|node| self.is_alive(*node));
        let old = self.focused();
        if old == target {
            return Vec::new();
        }
        let mut deliveries = Vec::new();
        if let Some(old) = old {
            self.set_flag(old, Flag::Focused, false);
            self.listeners(old, EventKind::Blur, &mut deliveries);
        }
        self.focused = target;
        if let Some(target) = target {
            self.set_flag(target, Flag::Focused, true);
            self.listeners(target, EventKind::Focus, &mut deliveries);
        }
        deliveries
    }

    /// The focusable elements of `root`'s tree that Tab reaches, in Tab
    /// order: every positive tab index first, lowest first (tree order
    /// between equals), then every 0 in tree order. A negative index is
    /// focusable but not reached by Tab.
    pub fn tab_order(&self, root: NodeId) -> Vec<NodeId> {
        let mut positive = Vec::new();
        let mut zero = Vec::new();
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            match self.data(node).tab_index {
                Some(index) if index > 0 => positive.push((index, node)),
                Some(0) => zero.push(node),
                _ => {}
            }
            for index in (0..self.taffy.child_count(node)).rev() {
                pending.push(self.taffy.get_child_id(node, index));
            }
        }
        // A stable sort: tree order between equal indices.
        positive.sort_by_key(|(index, _)| *index);
        positive
            .into_iter()
            .map(|(_, node)| node)
            .chain(zero)
            .collect()
    }

    fn move_focus(&mut self, root: NodeId, backward: bool) -> Vec<Delivery> {
        let order = self.tab_order(root);
        if order.is_empty() {
            return Vec::new();
        }
        let at = self
            .focused()
            .and_then(|focused| order.iter().position(|node| *node == focused));
        let next = match (at, backward) {
            (None, false) => 0,
            (None, true) => order.len() - 1,
            (Some(at), false) => (at + 1) % order.len(),
            (Some(at), true) => (at + order.len() - 1) % order.len(),
        };
        self.focus(Some(order[next]))
    }

    // ----------------------------------------------------------- routing ---

    /// Routes one input in `root`'s tree: see the module documentation.
    pub fn route(&mut self, root: NodeId, input: &Input) -> Route {
        let mut route = Route::default();
        match input {
            Input::PointerMoved(position) => {
                self.pointer = Some(*position);
                let hit = self.hit(root, *position);
                self.hover(root, hit, &mut route);
                if let Some(hit) = hit {
                    route.bubbling(self.bubble(hit, EventKind::PointerMove), None);
                }
            }
            Input::PointerLeft => {
                self.pointer = None;
                self.hover(root, None, &mut route);
            }
            Input::Pressed(position) => {
                self.pointer = Some(*position);
                let hit = self.hit(root, *position);
                self.hover(root, hit, &mut route);
                self.release_press();
                if let Some(hit) = hit {
                    for node in self.chain(hit) {
                        self.set_flag(node, Flag::Pressed, true);
                    }
                    self.pressed = Some(hit);
                }
                let focusable = hit.and_then(|hit| {
                    self.chain(hit)
                        .into_iter()
                        .find(|node| self.data(*node).tab_index.is_some())
                });
                let focus = self.focus(focusable);
                route.plain(focus);
                if let Some(hit) = hit {
                    route.bubbling(self.bubble(hit, EventKind::Press), None);
                }
            }
            Input::Released(position) => {
                self.pointer = Some(*position);
                let hit = self.hit(root, *position);
                self.hover(root, hit, &mut route);
                let pressed = self.pressed();
                self.release_press();
                if let Some(hit) = hit {
                    route.bubbling(self.bubble(hit, EventKind::Release), None);
                    let target = pressed.and_then(|pressed| self.common_ancestor(pressed, hit));
                    if let Some(target) = target {
                        route.bubbling(self.bubble(target, EventKind::Click), None);
                    }
                }
            }
            Input::Scrolled { position, delta } => {
                self.pointer = Some(*position);
                let hit = self.hit(root, *position);
                self.hover(root, hit, &mut route);
                if let Some(hit) = hit {
                    let mut deliveries = Vec::new();
                    let mut default = None;
                    for node in self.chain(hit) {
                        self.listeners(node, EventKind::Scroll, &mut deliveries);
                        if self.can_scroll(node, *delta) {
                            default = Some(DefaultAction::Scroll {
                                node,
                                delta: *delta,
                            });
                            break;
                        }
                    }
                    route.bubbling(deliveries, default);
                }
            }
            Input::Key { name, shift } => {
                let target = self.key_target(root);
                let default =
                    (name == "Tab").then_some(DefaultAction::MoveFocus { backward: *shift });
                route.bubbling(self.bubble(target, EventKind::Key), default);
            }
            Input::KeyReleased => {
                let target = self.key_target(root);
                route.bubbling(self.bubble(target, EventKind::KeyRelease), None);
            }
            Input::Text => {
                let target = self.key_target(root);
                route.bubbling(self.bubble(target, EventKind::Text), None);
            }
        }
        route
    }

    /// The framework's response to an event nobody stopped. Answers the
    /// handlers it owes in turn (a focus moving, the pointer over another
    /// element once the content moved under it), each step not stoppable.
    pub fn perform(&mut self, root: NodeId, action: DefaultAction) -> Route {
        let mut route = Route::default();
        match action {
            DefaultAction::MoveFocus { backward } => {
                let focus = self.move_focus(root, backward);
                route.plain(focus);
                // Tab brings what it focused into view, and the content may
                // have moved under a still pointer.
                if let Some(focused) = self.focused()
                    && self.reveal(focused)
                    && let Some(pointer) = self.pointer
                {
                    let hit = self.hit(root, pointer);
                    self.hover(root, hit, &mut route);
                }
            }
            DefaultAction::Scroll { node, delta } => {
                if !self.is_alive(node) {
                    return route;
                }
                let Some(scroll) = self.data(node).scroll.as_ref() else {
                    return route;
                };
                let moved = clamp_offset(
                    Point::new(scroll.offset.x - delta.x, scroll.offset.y - delta.y),
                    scroll.range,
                    scroll.axis,
                );
                if moved != scroll.offset {
                    self.data_mut(node)
                        .scroll
                        .as_mut()
                        .expect("a scroll")
                        .offset = moved;
                    self.offset_notices.push(node);
                    self.invalidate(node);
                    // The content moved under a still pointer.
                    if let Some(pointer) = self.pointer {
                        let hit = self.hit(root, pointer);
                        self.hover(root, hit, &mut route);
                    }
                }
            }
        }
        route
    }

    /// Scrolls every `scroll` around `node` as little as it takes to show
    /// it, innermost first: a scroll that shows all of it already does not
    /// move; one where it lies above (left) moves so its top (left) edge is
    /// the viewport's, one where it lies below (right) so its bottom (right)
    /// edge is — or its top, when it is larger than the viewport. Each outer
    /// scroll then shows what the inner one shows of it. From the last
    /// layout, as hit testing is. Answers whether an offset moved; a moved
    /// one is told to its program as a wheel's is.
    pub fn reveal(&mut self, node: NodeId) -> bool {
        if !self.is_alive(node) {
            return false;
        }
        let bounds = self.data(node).bounds;
        // The part to show, in the coordinates every box is laid out in
        // (before any scroll's offset), moved by the offsets of the scrolls
        // already passed on the way up.
        let (mut left, mut top) = (bounds.left(), bounds.top());
        let (mut right, mut bottom) = (bounds.right(), bounds.bottom());
        let mut moved = false;
        let mut current = node;
        while let Some(parent) = self.taffy.parent(current) {
            if self.kind(parent) == ElementKind::Viewport {
                break;
            }
            current = parent;
            let data = self.data(parent);
            let Some(scroll) = &data.scroll else {
                continue;
            };
            let view = Rect::new(
                data.bounds.left() + data.content_offset.x,
                data.bounds.top() + data.content_offset.y,
                data.content_size.width,
                data.content_size.height,
            );
            let wanted = Point::new(
                shown(
                    left - view.left(),
                    right - view.left(),
                    view.size.width,
                    scroll.offset.x,
                ),
                shown(
                    top - view.top(),
                    bottom - view.top(),
                    view.size.height,
                    scroll.offset.y,
                ),
            );
            let offset = clamp_offset(wanted, scroll.range, scroll.axis);
            if offset != scroll.offset {
                self.data_mut(parent)
                    .scroll
                    .as_mut()
                    .expect("a scroll")
                    .offset = offset;
                self.offset_notices.push(parent);
                self.invalidate(parent);
                moved = true;
            }
            // What this scroll now shows of it, as its parent sees it.
            left = (left - offset.x).max(view.left());
            right = (right - offset.x).min(view.right()).max(left);
            top = (top - offset.y).max(view.top());
            bottom = (bottom - offset.y).min(view.bottom()).max(top);
        }
        moved
    }

    fn key_target(&self, root: NodeId) -> NodeId {
        let focused = self.chain_in(root, self.focused);
        focused.first().copied().unwrap_or(root)
    }

    /// The focus is taken away with no blur: what had it is being removed,
    /// handlers and all (`UiTree::remove`).
    pub(super) fn clear_focus(&mut self) {
        if let Some(old) = self.focused() {
            self.set_flag(old, Flag::Focused, false);
        }
        self.focused = None;
    }

    /// The hovered element is inside `removed`, which is leaving: the part
    /// of the hover chain inside it is no longer hovered, and the hover rests
    /// on `parent` (still under the pointer as far as anything knows until
    /// the pointer moves again), with no leave delivered to the elements
    /// going.
    pub(super) fn unhover_inside(&mut self, removed: NodeId, parent: Option<NodeId>) {
        if let Some(hovered) = self.hovered() {
            for node in self.chain(hovered) {
                self.set_flag(node, Flag::Hovered, false);
                if node == removed {
                    break;
                }
            }
        }
        self.hovered = parent;
    }

    pub(super) fn release_press(&mut self) {
        if let Some(pressed) = self.pressed() {
            for node in self.chain(pressed) {
                self.set_flag(node, Flag::Pressed, false);
            }
        }
        self.pressed = None;
    }

    fn common_ancestor(&self, first: NodeId, second: NodeId) -> Option<NodeId> {
        let first = self.chain(first);
        self.chain(second)
            .into_iter()
            .find(|node| first.contains(node))
    }

    /// Whether `node` is a scroll that `delta` would move.
    fn can_scroll(&self, node: NodeId, delta: Point) -> bool {
        let Some(scroll) = &self.data(node).scroll else {
            return false;
        };
        let moved = clamp_offset(
            Point::new(scroll.offset.x - delta.x, scroll.offset.y - delta.y),
            scroll.range,
            scroll.axis,
        );
        moved != scroll.offset
    }

    /// Where a scroll's content box is on the screen: its viewport.
    pub fn scroll_viewport(&self, node: NodeId) -> Option<(Rect, ScrollAxis)> {
        let data = self.data(node);
        let scroll = data.scroll.as_ref()?;
        Some((super::padding_box(data).0, scroll.axis))
    }
}
