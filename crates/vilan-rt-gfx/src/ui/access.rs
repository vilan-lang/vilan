//! `std::ui`'s tree as the operating system's accessibility API sees it
//! (native-ui.md section 4.6, R10; part P5): the conversion of a [`UiTree`]
//! into an `accesskit` tree, what changed since the last one, and the plan
//! for an assistive tool's action, taken back in through the paths a pointer
//! or a key takes.
//!
//! Everything here is PURE: it reads the tree and answers values. The host
//! (`host/access.rs`) owns the platform adapter, decides when to publish, and
//! carries out a plan inside a turn.
//!
//! # What is published
//!
//! A window node (`Role::Window`, named by the window's title) holds the
//! mounted root. Below it, an element is published when an assistive tool
//! has something to say about it, and left out otherwise — its published
//! descendants then hang from its nearest published ancestor, in tree order:
//!
//! | Element | Published as |
//! |---|---|
//! | a marker | never (it is a place, not an element) |
//! | any element with a role | that role (`Role::TextField` is a text input, `Role::Checkbox` a check box) |
//! | a text, no role | a label whose value is its text (what a platform reads as a label's name; a labelled text keeps its label too) |
//! | an image, no role | an image when it has a label; left out (decorative) when it has none |
//! | a scroll | a scroll view, with its offset and range |
//! | a row, column or stack, no role | a group when labelled, a generic container when focusable, left out otherwise |
//! | the child of a list, no role | a list item (with its place in the list when the list says it holds only some) |
//!
//! An empty, unlabelled text is left out. A button, a check box, a link and a
//! heading are LEAVES: their name is their label, or else their content's
//! text (each text in it, or a labelled element's label, in tree order), and
//! nothing inside them is published (ARIA's "children presentational"). A
//! text input is a leaf too, with one text run: its value is its edited
//! text, its placeholder any other text inside it, and its selection is the
//! edited text's caret and anchor, in graphemes.
//!
//! # Coordinates
//!
//! Bounds are in dp, in the coordinates every box is laid out in (before any
//! scroll's offset); the window node carries the scale (its transform is the
//! window's content scale, device pixels per dp), so `accesskit` answers the
//! platform in physical pixels and a change of scale changes one node. A
//! scroll carries its offset as its transform (a translation by minus the
//! offset), with its own bounds moved back by the offset to stay where it is:
//! a scroll step changes one node, the scroll's.
//!
//! # Ids
//!
//! An element's id is its tree id, which stays the same for the element's
//! life and is never reused for another (the solver's ids carry a version);
//! the window node is id 0, which no element has. A text input's run uses the
//! edited text's id (that element is never published itself).

use std::collections::{HashMap, HashSet};

use accesskit::{
    Action, ActionData, ActionRequest, Affine, Node as AccessNode, NodeId as AccessId,
    Rect as AccessRect, Role as AccessRole, ScrollUnit, TextPosition, TextSelection, Toggled,
    TreeId, TreeInfo, TreeUpdate,
};
use taffy::TraversePartialTree;

use super::{Content, ElementKind, NodeId, Role, UiTree};
use crate::geometry::{Point, Rect};
use crate::text::editing::EditingGeometry;

#[cfg(test)]
mod tests;

/// The window node's id: no element has it (an element's id carries an odd
/// version in its upper half).
pub const WINDOW_NODE: AccessId = AccessId(0);

/// How far an `Item` scroll step moves, in dp: a wheel notch's line, as
/// `std::gfx` reports one (`host/input.rs`'s `DP_PER_SCROLL_LINE`).
const SCROLL_LINE: f32 = 32.0;

/// What an element carries for assistive technology beyond its role and
/// label: set by `std::ui::kit`, settable by an app.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct AccessState {
    /// A two-state control's state (`checked`), or none.
    checked: Option<bool>,
    /// It is shown but does not act (`disabled_state`).
    disabled: bool,
    /// A list holding only some of its items (`items_shown`): the first
    /// child is item `first` of `count`.
    items: Option<(usize, usize)>,
}

/// The window around a tree, as the window node publishes it.
#[derive(Clone, Copy, Debug)]
pub struct AccessWindow<'a> {
    pub title: &'a str,
    /// Device pixels per dp: the window's content scale.
    pub scale: f64,
    /// The window's size in dp.
    pub width: f32,
    pub height: f32,
}

/// A whole published tree: every node, and the one with the focus (the
/// window when no element has it).
#[derive(Clone, Debug, PartialEq)]
pub struct AccessTree {
    pub nodes: Vec<(AccessId, AccessNode)>,
    pub focus: AccessId,
}

/// An element's accessibility id.
pub fn access_id(node: NodeId) -> AccessId {
    AccessId(u64::from(node))
}

fn element_of(id: AccessId) -> NodeId {
    NodeId::from(id.0)
}

fn access_rect(rect: Rect) -> AccessRect {
    AccessRect::new(
        f64::from(rect.left()),
        f64::from(rect.top()),
        f64::from(rect.right()),
        f64::from(rect.bottom()),
    )
}

/// What the build of one tree carries down.
struct Build<'tree> {
    tree: &'tree UiTree,
    nodes: Vec<(AccessId, AccessNode)>,
    /// The elements published, to place the focus.
    published: HashSet<NodeId>,
}

impl UiTree {
    // ------------------------------------------------------------ state ---

    /// Moves when something only an assistive tool sees changes (a role, a
    /// label, a state, the tab order), so the host knows a published tree
    /// is stale though no frame changed.
    pub fn semantics_generation(&self) -> u64 {
        self.semantics_generation
    }

    pub(super) fn semantics_changed(&mut self) {
        self.semantics_generation += 1;
    }

    /// A two-state control's state, or none (`std::ui`'s `checked`).
    pub fn set_checked(&mut self, node: NodeId, checked: Option<bool>) {
        if self.data(node).access.checked != checked {
            self.data_mut(node).access.checked = checked;
            self.semantics_changed();
        }
    }

    pub fn checked(&self, node: NodeId) -> Option<bool> {
        self.data(node).access.checked
    }

    /// Shown, but not acting (`std::ui`'s `disabled_state`): published as
    /// disabled, and offered no action but being scrolled to.
    pub fn set_disabled(&mut self, node: NodeId, disabled: bool) {
        if self.data(node).access.disabled != disabled {
            self.data_mut(node).access.disabled = disabled;
            self.semantics_changed();
        }
    }

    pub fn disabled(&self, node: NodeId) -> bool {
        self.data(node).access.disabled
    }

    /// A list holding only some of its items (`std::ui`'s `items_shown`):
    /// its first child is item `first` (from 0) of `count`, the next
    /// `first + 1`, and so on.
    pub fn set_items_shown(&mut self, node: NodeId, first: usize, count: usize) {
        let items = Some((first, count));
        if self.data(node).access.items != items {
            self.data_mut(node).access.items = items;
            self.semantics_changed();
        }
    }

    // ------------------------------------------------------- conversion ---

    /// `root`'s tree (or none: a window with nothing mounted) as an
    /// assistive tool is told it, under a window node: see the module
    /// documentation.
    pub fn access_tree(&self, root: Option<NodeId>, window: AccessWindow<'_>) -> AccessTree {
        let mut build = Build {
            tree: self,
            nodes: Vec::new(),
            published: HashSet::new(),
        };
        let mut window_node = AccessNode::new(AccessRole::Window);
        window_node.set_label(window.title);
        window_node.set_transform(Affine::scale(window.scale));
        window_node.set_bounds(access_rect(Rect::new(
            0.0,
            0.0,
            window.width,
            window.height,
        )));
        let mut children = Vec::new();
        if let Some(root) = root.filter(|root| self.is_alive(*root)) {
            build.place(root, None, &mut children);
        }
        window_node.set_children(children);
        build.nodes.push((WINDOW_NODE, window_node));
        let focus = self
            .focused()
            .and_then(|focused| {
                let mut current = Some(focused);
                while let Some(node) = current {
                    if build.published.contains(&node) {
                        return Some(access_id(node));
                    }
                    current = self.taffy.parent(node);
                }
                None
            })
            .unwrap_or(WINDOW_NODE);
        AccessTree {
            nodes: build.nodes,
            focus,
        }
    }

    /// The text of `node`'s content, as a name: a labelled element's label,
    /// a text's text, or else its children's, in tree order, joined by a
    /// space.
    fn content_name(&self, node: NodeId, into: &mut String) {
        let data = self.data(node);
        if data.kind == ElementKind::Marker {
            return;
        }
        let own = match (&data.label, &data.content) {
            (Some(label), _) if !label.is_empty() => Some(label.as_str()),
            (None, Content::Text(content)) => Some(content.text.as_str()),
            _ => None,
        };
        if let Some(own) = own {
            let own = own.trim();
            if !own.is_empty() {
                if !into.is_empty() {
                    into.push(' ');
                }
                into.push_str(own);
            }
            return;
        }
        for index in 0..self.taffy.child_count(node) {
            self.content_name(self.taffy.get_child_id(node, index), into);
        }
    }

    /// The first edited text inside `node`, and every other text in it (a
    /// placeholder).
    fn edited_inside(&self, node: NodeId) -> (Option<NodeId>, String) {
        let mut edited = None;
        let mut others = String::new();
        let mut pending = vec![node];
        while let Some(current) = pending.pop() {
            if let Content::Text(content) = &self.data(current).content {
                if content.editing.is_some() && edited.is_none() {
                    edited = Some(current);
                } else if !content.text.trim().is_empty() {
                    if !others.is_empty() {
                        others.push(' ');
                    }
                    others.push_str(content.text.trim());
                }
            }
            for index in (0..self.taffy.child_count(current)).rev() {
                pending.push(self.taffy.get_child_id(current, index));
            }
        }
        (edited, others)
    }

    /// An edited text's graphemes, as its last painted layout records them:
    /// none before its first layout.
    fn editing_geometry(&self, node: NodeId) -> Option<&EditingGeometry> {
        let Content::Text(content) = &self.data(node).content else {
            return None;
        };
        content
            .shaped
            .iter()
            .find(|shaped| shaped.constraint == content.painted_at)
            .or(content.shaped.first())
            .and_then(|shaped| shaped.layout.data.editing.as_ref())
    }

    /// Where `node`'s box is inside `outer` (an ancestor): its laid-out box
    /// moved by the offsets of the scrolls strictly between them.
    fn box_inside(&self, node: NodeId, outer: NodeId) -> Rect {
        let mut bounds = self.data(node).bounds;
        let mut current = node;
        while let Some(parent) = self.taffy.parent(current) {
            if parent == outer {
                break;
            }
            if let Some(scroll) = &self.data(parent).scroll {
                bounds.origin.x -= scroll.offset.x;
                bounds.origin.y -= scroll.offset.y;
            }
            current = parent;
        }
        bounds
    }
}

/// The UTF-16 index `unit` of `text` as an index into `graphemes` (logical
/// order): how many graphemes start before it.
fn grapheme_index(text: &str, graphemes: &[(usize, usize, f32, f32)], unit: usize) -> usize {
    let byte = super::byte_of_unit(text, unit);
    graphemes.iter().filter(|(start, ..)| *start < byte).count()
}

/// `geometry`'s graphemes in logical order, one entry each (a grapheme a
/// font change split is two boxes: joined): start, end, left edge, advance.
fn logical_graphemes(geometry: &EditingGeometry) -> Vec<(usize, usize, f32, f32)> {
    let mut graphemes: Vec<(usize, usize, f32, f32)> = Vec::new();
    let mut boxes: Vec<_> = geometry.graphemes.iter().collect();
    boxes.sort_by(|first, second| {
        (first.start, first.x)
            .partial_cmp(&(second.start, second.x))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for grapheme in boxes {
        match graphemes.last_mut() {
            Some(last) if last.0 == grapheme.start => {
                last.1 = last.1.max(grapheme.end);
                last.3 += grapheme.width;
            }
            _ => graphemes.push((grapheme.start, grapheme.end, grapheme.x, grapheme.width)),
        }
    }
    graphemes
}

/// The role an element is published with, or none (left out), when its
/// parent is (or is not) a list.
fn published_role(tree: &UiTree, node: NodeId, in_list: bool) -> Option<AccessRole> {
    let data = tree.data(node);
    let explicit = match data.role {
        Role::None => None,
        Role::Group => Some(AccessRole::Group),
        Role::Heading => Some(AccessRole::Heading),
        Role::Paragraph => Some(AccessRole::Paragraph),
        Role::Button => Some(AccessRole::Button),
        Role::Link => Some(AccessRole::Link),
        Role::TextField => Some(AccessRole::TextInput),
        Role::Checkbox => Some(AccessRole::CheckBox),
        Role::List => Some(AccessRole::List),
        Role::ListItem => Some(AccessRole::ListItem),
        Role::Image => Some(AccessRole::Image),
    };
    if explicit.is_some() {
        return explicit;
    }
    let labelled = data.label.as_deref().is_some_and(|label| !label.is_empty());
    match data.kind {
        ElementKind::Marker | ElementKind::Viewport => None,
        _ if in_list => Some(AccessRole::ListItem),
        ElementKind::Text => match &data.content {
            Content::Text(content) if labelled || !content.text.trim().is_empty() => {
                Some(AccessRole::Label)
            }
            _ => None,
        },
        ElementKind::Image => labelled.then_some(AccessRole::Image),
        ElementKind::Scroll => Some(AccessRole::ScrollView),
        ElementKind::Row | ElementKind::Column | ElementKind::Stack => {
            if labelled {
                Some(AccessRole::Group)
            } else if data.tab_index.is_some() {
                Some(AccessRole::GenericContainer)
            } else {
                None
            }
        }
    }
}

/// A role whose node is a leaf named by its content (ARIA's "children
/// presentational", and a heading).
fn named_by_content(role: AccessRole) -> bool {
    matches!(
        role,
        AccessRole::Button | AccessRole::CheckBox | AccessRole::Link | AccessRole::Heading
    )
}

impl Build<'_> {
    /// Publishes `node` (when it is published), or its published
    /// descendants, appending their ids to `into`. `list` is the list it is
    /// a direct child of, with its index among that list's children.
    fn place(&mut self, node: NodeId, list: Option<(NodeId, usize)>, into: &mut Vec<AccessId>) {
        let tree = self.tree;
        let data = tree.data(node);
        if data.kind == ElementKind::Marker {
            return;
        }
        let Some(role) = published_role(tree, node, list.is_some()) else {
            self.place_children(node, into);
            return;
        };
        let mut published = AccessNode::new(role);
        let disabled = data.access.disabled;
        let label = data.label.as_deref().filter(|label| !label.is_empty());
        let mut bounds = data.bounds;
        if disabled {
            published.set_disabled();
        }
        if let Some(checked) = data.access.checked {
            published.set_toggled(Toggled::from(checked));
        }
        if data.tab_index.is_some() && !disabled {
            published.add_action(Action::Focus);
        }
        published.add_action(Action::ScrollIntoView);
        if data.clip {
            published.set_clips_children();
        }
        if let Some((list, index)) = list
            && let Some((first, count)) = tree.data(list).access.items
        {
            published.set_position_in_set(first + index + 1);
            published.set_size_of_set(count);
        }
        let mut children = Vec::new();
        if named_by_content(role) {
            if !disabled && role != AccessRole::Heading {
                published.add_action(Action::Click);
            }
            match label {
                Some(label) => published.set_label(label),
                None => {
                    let mut name = String::new();
                    tree.content_name(node, &mut name);
                    if !name.is_empty() {
                        published.set_label(name);
                    }
                }
            }
        } else if role == AccessRole::TextInput {
            if let Some(label) = label {
                published.set_label(label);
            }
            self.text_input(node, &mut published, &mut children);
        } else {
            match (&data.content, label) {
                // A label's text is its value: what the platform reads as
                // its name (`accesskit`'s rule for `Role::Label`).
                (Content::Text(content), label) if role == AccessRole::Label => {
                    if let Some(label) = label {
                        published.set_label(label);
                    }
                    published.set_value(content.text.as_str());
                }
                (Content::Text(content), Some(label)) => {
                    published.set_label(label);
                    if !content.text.is_empty() {
                        published.set_value(content.text.as_str());
                    }
                }
                (Content::Text(content), None) => published.set_label(content.text.as_str()),
                (_, Some(label)) => published.set_label(label),
                (_, None) => {}
            }
            if let Some(scroll) = &data.scroll {
                let (offset, range) = (scroll.offset, scroll.range);
                let horizontal = !matches!(scroll.axis, super::ScrollAxis::Vertical);
                let vertical = !matches!(scroll.axis, super::ScrollAxis::Horizontal);
                if horizontal {
                    published.set_scroll_x(f64::from(offset.x));
                    published.set_scroll_x_min(0.0);
                    published.set_scroll_x_max(f64::from(range.width));
                    if offset.x > 0.0 {
                        published.add_action(Action::ScrollLeft);
                    }
                    if offset.x < range.width {
                        published.add_action(Action::ScrollRight);
                    }
                }
                if vertical {
                    published.set_scroll_y(f64::from(offset.y));
                    published.set_scroll_y_min(0.0);
                    published.set_scroll_y_max(f64::from(range.height));
                    if offset.y > 0.0 {
                        published.add_action(Action::ScrollUp);
                    }
                    if offset.y < range.height {
                        published.add_action(Action::ScrollDown);
                    }
                }
                published.set_clips_children();
                if offset != Point::ZERO {
                    // Its children are drawn moved by minus the offset; its
                    // own box, which the transform moves too, is moved back.
                    published.set_transform(Affine::translate((
                        -f64::from(offset.x),
                        -f64::from(offset.y),
                    )));
                    bounds.origin.x += offset.x;
                    bounds.origin.y += offset.y;
                }
            }
            if role == AccessRole::List {
                let mut index = 0;
                for position in 0..tree.taffy.child_count(node) {
                    let child = tree.taffy.get_child_id(node, position);
                    if tree.kind(child) == ElementKind::Marker {
                        continue;
                    }
                    self.place(child, Some((node, index)), &mut children);
                    index += 1;
                }
            } else {
                self.place_children(node, &mut children);
            }
        }
        published.set_bounds(access_rect(bounds));
        if !children.is_empty() {
            published.set_children(children);
        }
        self.published.insert(node);
        into.push(access_id(node));
        self.nodes.push((access_id(node), published));
    }

    fn place_children(&mut self, node: NodeId, into: &mut Vec<AccessId>) {
        for index in 0..self.tree.taffy.child_count(node) {
            let child = self.tree.taffy.get_child_id(node, index);
            self.place(child, None, into);
        }
    }

    /// A text input's value, placeholder, run and selection (see the module
    /// documentation).
    fn text_input(
        &mut self,
        node: NodeId,
        published: &mut AccessNode,
        children: &mut Vec<AccessId>,
    ) {
        let tree = self.tree;
        let (edited, placeholder) = tree.edited_inside(node);
        if !placeholder.is_empty() {
            published.set_placeholder(placeholder);
        }
        let Some(edited) = edited else {
            return;
        };
        let text = tree.text(edited).unwrap_or("");
        published.set_value(text);
        if tree.data(node).access.disabled {
            return;
        }
        published.add_action(Action::SetValue);
        published.add_action(Action::ReplaceSelectedText);
        let Some(geometry) = tree.editing_geometry(edited) else {
            return;
        };
        let graphemes = logical_graphemes(geometry);
        let mut run = AccessNode::new(AccessRole::TextRun);
        run.set_value(text);
        let lengths: Vec<u8> = graphemes
            .iter()
            .map(|(start, end, _, _)| u8::try_from(end - start).unwrap_or(u8::MAX))
            .collect();
        run.set_character_lengths(lengths);
        run.set_character_positions(
            graphemes
                .iter()
                .map(|(_, _, x, _)| *x)
                .collect::<Vec<f32>>(),
        );
        run.set_character_widths(
            graphemes
                .iter()
                .map(|(_, _, _, width)| *width)
                .collect::<Vec<f32>>(),
        );
        let data = tree.data(edited);
        let content_box = tree.box_inside(edited, node);
        run.set_bounds(access_rect(Rect::new(
            content_box.left() + data.content_offset.x,
            content_box.top() + data.content_offset.y,
            data.content_size.width,
            data.content_size.height,
        )));
        if let Content::Text(content) = &data.content
            && let Some(editing) = &content.editing
        {
            let position = |unit: usize| TextPosition {
                node: access_id(edited),
                character_index: grapheme_index(text, &graphemes, unit),
            };
            published.set_text_selection(TextSelection {
                anchor: position(editing.anchor),
                focus: position(editing.caret),
            });
            published.add_action(Action::SetTextSelection);
        }
        children.push(access_id(edited));
        self.nodes.push((access_id(edited), run));
    }
}

// ---------------------------------------------------------- publishing ---

/// What the platform was last told, so the next update carries only the
/// nodes that changed.
///
/// Every update is built from a whole [`AccessTree`] and compared node by
/// node: a full conversion per published change, which costs a walk of the
/// tree while an assistive tool listens and nothing while none does (the
/// adapter calls for an update only when it is active). Tracking the changed
/// elements instead would mean a hook in every setter of the tree for a
/// saving the measurements do not ask for (`examples/ui_measure.rs`'s
/// `access` lines: the 10,000-row list in a few milliseconds, a windowed
/// list in tens of microseconds).
#[derive(Default)]
pub struct AccessPublisher {
    last: HashMap<AccessId, AccessNode>,
}

impl AccessPublisher {
    /// The update that brings the platform from the last tree to `tree`:
    /// every node when `full` (the platform's first tree, or after it
    /// stopped listening), otherwise the nodes new or changed since.
    pub fn update(&mut self, tree: AccessTree, full: bool) -> TreeUpdate {
        if full {
            self.last.clear();
        }
        let mut changed = Vec::new();
        let mut seen = HashSet::with_capacity(tree.nodes.len());
        for (id, node) in tree.nodes {
            seen.insert(id);
            if self.last.get(&id) != Some(&node) {
                changed.push((id, node.clone()));
                self.last.insert(id, node);
            }
        }
        self.last.retain(|id, _| seen.contains(id));
        TreeUpdate {
            nodes: changed,
            tree: full.then(|| {
                let mut info = TreeInfo::new(WINDOW_NODE);
                info.toolkit_name = Some("vilan std::ui".to_string());
                info.toolkit_version = Some(env!("CARGO_PKG_VERSION").to_string());
                info
            }),
            tree_id: TreeId::ROOT,
            focus: tree.focus,
        }
    }
}

// ------------------------------------------------------------- listing ---

/// `tree` as text, one node a line, indented by depth: its role, its name,
/// its value and placeholder, its states and its actions; a text input's
/// selection in graphemes. No bounds: what a pin of structure, roles, names
/// and states reads (and what `std::ui`'s `accessibility()` answers).
pub fn access_listing(tree: &AccessTree, include_window: bool) -> String {
    let nodes: HashMap<AccessId, &AccessNode> =
        tree.nodes.iter().map(|(id, node)| (*id, node)).collect();
    let mut listing = String::new();
    let mut pending: Vec<(AccessId, usize)> = Vec::new();
    match nodes.get(&WINDOW_NODE) {
        Some(_) if include_window => pending.push((WINDOW_NODE, 0)),
        Some(window) => {
            for child in window.children().iter().rev() {
                pending.push((*child, 0));
            }
        }
        None => {}
    }
    while let Some((id, depth)) = pending.pop() {
        let Some(node) = nodes.get(&id) else {
            continue;
        };
        listing.push_str(&"  ".repeat(depth));
        listing.push_str(&format!("{:?}", node.role()));
        if let Some(label) = node.label() {
            listing.push_str(&format!(" {label:?}"));
        }
        if let Some(value) = node.value() {
            listing.push_str(&format!(" value={value:?}"));
        }
        if let Some(placeholder) = node.placeholder() {
            listing.push_str(&format!(" placeholder={placeholder:?}"));
        }
        match node.toggled() {
            Some(Toggled::True) => listing.push_str(" checked"),
            Some(Toggled::False) => listing.push_str(" unchecked"),
            Some(Toggled::Mixed) => listing.push_str(" mixed"),
            None => {}
        }
        if node.is_disabled() {
            listing.push_str(" disabled");
        }
        if tree.focus == id && id != WINDOW_NODE {
            listing.push_str(" focused");
        }
        if let (Some(position), Some(size)) = (node.position_in_set(), node.size_of_set()) {
            listing.push_str(&format!(" item={position}/{size}"));
        }
        if let (Some(scroll), Some(max)) = (node.scroll_y(), node.scroll_y_max()) {
            listing.push_str(&format!(" scroll_y={scroll}/{max}"));
        }
        if let (Some(scroll), Some(max)) = (node.scroll_x(), node.scroll_x_max()) {
            listing.push_str(&format!(" scroll_x={scroll}/{max}"));
        }
        if node.role() == AccessRole::TextRun {
            listing.push_str(&format!(" graphemes={}", node.character_lengths().len()));
        }
        if let Some(selection) = node.text_selection() {
            listing.push_str(&format!(
                " selection={}..{}",
                selection.anchor.character_index, selection.focus.character_index
            ));
        }
        let actions: Vec<&str> = [
            (Action::Click, "click"),
            (Action::Focus, "focus"),
            (Action::SetValue, "set_value"),
            (Action::ReplaceSelectedText, "replace_selected_text"),
            (Action::SetTextSelection, "set_text_selection"),
            (Action::ScrollUp, "scroll_up"),
            (Action::ScrollDown, "scroll_down"),
            (Action::ScrollLeft, "scroll_left"),
            (Action::ScrollRight, "scroll_right"),
        ]
        .into_iter()
        .filter(|(action, _)| node.supports_action(*action))
        .map(|(_, name)| name)
        .collect();
        if !actions.is_empty() {
            listing.push_str(&format!(" [{}]", actions.join(" ")));
        }
        listing.push('\n');
        for child in node.children().iter().rev() {
            pending.push((*child, depth + 1));
        }
    }
    listing
}

// ------------------------------------------------------------- actions ---

/// One step of an assistive tool's action, as the host carries it out: each
/// is what the program would see from a person doing the same — a focus
/// moved as a press or `focus()` moves it, a scroll into view as Tab's, and
/// the rest as window input queued for the program like any other.
#[derive(Clone, Debug, PartialEq)]
pub enum AccessStep {
    /// The element takes the focus (its blur and focus handlers run).
    Focus(NodeId),
    /// Every scroll around it moves as little as it takes to show it.
    Reveal(NodeId),
    /// The pointer moves to the middle of the element's visible part and the
    /// primary button is pressed and released there (when the element is
    /// what a pointer there hits: see [`UiTree::access_click_point`]).
    Click(NodeId),
    /// A key goes down and comes up: named as `std::gfx` names it.
    Key {
        name: &'static str,
        named: bool,
        shift: bool,
        control: bool,
    },
    /// Text typed (as an input method or a paste delivers it).
    Text(String),
    /// The wheel turned over the middle of a scroll's visible viewport, by
    /// `delta` (dp; positive moves the content right and down).
    Wheel { node: NodeId, delta: Point },
}

fn key(name: &'static str, named: bool, shift: bool) -> AccessStep {
    AccessStep::Key {
        name,
        named,
        shift,
        control: false,
    }
}

impl UiTree {
    /// The steps that carry out `request` in `root`'s tree: none when its
    /// target is not an element of that tree or does not offer the action
    /// (a disabled control, a text input not yet laid out, a selection in
    /// another element). See the module documentation for which element
    /// offers which action.
    pub fn plan_access_action(&self, root: NodeId, request: &ActionRequest) -> Vec<AccessStep> {
        let node = element_of(request.target_node);
        if request.target_node == WINDOW_NODE
            || !self.is_alive(node)
            || self.root_of(node) != root
            || self.kind(node) == ElementKind::Viewport
        {
            return Vec::new();
        }
        let data = self.data(node);
        let disabled = data.access.disabled;
        let focusable = data.tab_index.is_some() && !disabled;
        let focus_first = || {
            if focusable && self.focused() != Some(node) {
                vec![AccessStep::Focus(node)]
            } else {
                Vec::new()
            }
        };
        let editable = data.role == Role::TextField && !disabled;
        match (request.action, &request.data) {
            (Action::Focus, _) if focusable => vec![AccessStep::Focus(node)],
            (Action::Click, _)
                if !disabled && matches!(data.role, Role::Button | Role::Checkbox | Role::Link) =>
            {
                vec![AccessStep::Reveal(node), AccessStep::Click(node)]
            }
            (Action::ScrollIntoView, _) => vec![AccessStep::Reveal(node)],
            (Action::SetValue, Some(ActionData::Value(value))) if editable => {
                // Select everything, then type the value over it (or delete
                // it, for an empty value): what a person does.
                let mut steps = focus_first();
                steps.push(AccessStep::Key {
                    name: "a",
                    named: false,
                    shift: false,
                    control: true,
                });
                steps.push(if value.is_empty() {
                    key("Delete", true, false)
                } else {
                    AccessStep::Text(value.to_string())
                });
                steps
            }
            (Action::ReplaceSelectedText, Some(ActionData::Value(value))) if editable => {
                let mut steps = focus_first();
                steps.push(if value.is_empty() {
                    key("Delete", true, false)
                } else {
                    AccessStep::Text(value.to_string())
                });
                steps
            }
            (Action::SetTextSelection, Some(ActionData::SetTextSelection(selection)))
                if editable =>
            {
                let (edited, _) = self.edited_inside(node);
                let Some(edited) = edited else {
                    return Vec::new();
                };
                let run = access_id(edited);
                if selection.anchor.node != run || selection.focus.node != run {
                    return Vec::new();
                }
                let count = self
                    .editing_geometry(edited)
                    .map_or(0, |geometry| logical_graphemes(geometry).len());
                let anchor = selection.anchor.character_index.min(count);
                let focus = selection.focus.character_index.min(count);
                // Home, then one grapheme at a time: to the anchor, then
                // with Shift to the focus — the caret keys a person uses.
                let mut steps = focus_first();
                steps.push(key("Home", true, false));
                steps.extend((0..anchor).map(|_| key("ArrowRight", true, false)));
                if focus > anchor {
                    steps.extend((anchor..focus).map(|_| key("ArrowRight", true, true)));
                } else {
                    steps.extend((focus..anchor).map(|_| key("ArrowLeft", true, true)));
                }
                steps
            }
            (
                action @ (Action::ScrollUp
                | Action::ScrollDown
                | Action::ScrollLeft
                | Action::ScrollRight),
                unit,
            ) if data.scroll.is_some() => {
                let Some((viewport, _)) = self.scroll_viewport(node) else {
                    return Vec::new();
                };
                let page = |length: f32| (length - SCROLL_LINE).max(SCROLL_LINE);
                let (across, down) = match unit {
                    Some(ActionData::ScrollUnit(ScrollUnit::Item)) => (SCROLL_LINE, SCROLL_LINE),
                    _ => (page(viewport.size.width), page(viewport.size.height)),
                };
                // The wheel's sign: positive moves the content right and
                // down, which scrolls up (left).
                let delta = match action {
                    Action::ScrollUp => Point::new(0.0, down),
                    Action::ScrollDown => Point::new(0.0, -down),
                    Action::ScrollLeft => Point::new(across, 0.0),
                    _ => Point::new(-across, 0.0),
                };
                vec![AccessStep::Wheel { node, delta }]
            }
            _ => Vec::new(),
        }
    }

    /// Where `node` is on the screen: its laid-out box moved by the offsets
    /// of the scrolls around it, and cut to every clipping element around it
    /// (in dp from its root's top-left corner); `None` when nothing of it
    /// shows.
    pub fn visible_box(&self, node: NodeId) -> Option<Rect> {
        let mut visible = self.data(node).bounds;
        let mut current = node;
        while let Some(parent) = self.taffy.parent(current) {
            if self.kind(parent) == ElementKind::Viewport {
                break;
            }
            let data = self.data(parent);
            if let Some(scroll) = &data.scroll {
                visible.origin.x -= scroll.offset.x;
                visible.origin.y -= scroll.offset.y;
            }
            if data.clip {
                // The clip's own box, moved by the scrolls further out.
                let mut area = super::padding_box(data).0;
                let mut outer = parent;
                while let Some(further) = self.taffy.parent(outer) {
                    if self.kind(further) == ElementKind::Viewport {
                        break;
                    }
                    if let Some(scroll) = &self.data(further).scroll {
                        area.origin.x -= scroll.offset.x;
                        area.origin.y -= scroll.offset.y;
                    }
                    outer = further;
                }
                let left = visible.left().max(area.left());
                let top = visible.top().max(area.top());
                let right = visible.right().min(area.right());
                let bottom = visible.bottom().min(area.bottom());
                visible = Rect::new(left, top, right - left, bottom - top);
            }
            current = parent;
        }
        (!visible.is_empty()).then_some(visible)
    }

    /// The point a click on `node` lands on: the middle of its visible box,
    /// when a pointer there hits `node` or an element inside it (nothing
    /// covers it); `None` otherwise.
    pub fn access_click_point(&self, root: NodeId, node: NodeId) -> Option<Point> {
        if !self.is_alive(node) {
            return None;
        }
        let visible = self.visible_box(node)?;
        let point = Point::new(
            visible.left() + visible.size.width / 2.0,
            visible.top() + visible.size.height / 2.0,
        );
        let mut current = self.hit(root, point);
        while let Some(hit) = current {
            if hit == node {
                return Some(point);
            }
            current = self.taffy.parent(hit);
        }
        None
    }
}
