//! A window's accessibility (native-ui.md section 4.6, R10; part P5): the
//! platform adapter (`accesskit_winit`: AT-SPI on Linux and the BSDs, UI
//! Automation on Windows, NSAccessibility on macOS), what is published to it
//! and when, and the actions an assistive tool asks for, carried out inside a
//! turn. The tree's conversion is `ui/access.rs`'s; this is the host's half.
//!
//! # Lazy, and idle when nobody listens
//!
//! Every window gets an adapter when it is made (the adapter must exist
//! before the window is first shown, so a window is made hidden and shown
//! after). The adapter is INACTIVE until an assistive tool asks for the
//! application's tree; until then [`WindowAccess::publish`] costs a compare
//! of a few numbers and builds nothing, and nothing here wakes the loop. On
//! Linux the adapter keeps one thread of its own (`accesskit_unix`), asleep
//! on the session bus.
//!
//! When a tool starts listening, the adapter's thread calls the activation
//! handler, which marks a full tree owed and wakes the loop
//! ([`HostWake::AccessActivated`]); the next publish sends the whole tree.
//! The mark is set under the adapter's own lock, and the update that pays it
//! is built under the same lock, so the platform's first tree is always a
//! whole one, whichever thread got there first.
//!
//! # When a tree is published
//!
//! After every turn, for each window, if the tool listens and something it
//! can see may have changed: the tree's layout or paint generation, its
//! semantics generation (a role, a label, a state, the tab order), the
//! focus, the window's title, size or scale, or what is mounted. The update
//! carries only the nodes that changed (`AccessPublisher`).
//!
//! # Actions
//!
//! An action arrives on the adapter's thread, crosses to the loop as a
//! [`HostWake::Action`], and waits in its window's queue for a turn, as a
//! window event does. The window's source takes one when no window event is
//! waiting, and carries out its plan (`UiTree::plan_access_action`): a focus
//! and a scroll into view are done at once, as `focus()` and Tab's reveal
//! do them; a click, a key, typed text and a wheel step are QUEUED as the
//! window input they stand for, so the program — its `std::ui` handlers and
//! its own event callback alike — receives exactly what a person's pointer
//! or keyboard would have sent.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use accesskit::{ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, TreeUpdate};
use taffy::NodeId;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::Window;

use super::input::{self as raw_input, NUMBERS, RawEvent, enqueue};
use super::{HostWake, STATE, ui, with_window};
use crate::geometry::Point;
use crate::ui::{AccessPublisher, AccessStep, AccessTree, AccessWindow};

/// A window's adapter and what it last published.
pub(crate) struct WindowAccess {
    adapter: accesskit_winit::Adapter,
    publisher: AccessPublisher,
    /// A whole tree is owed: set when a tool starts listening.
    initial_owed: Arc<AtomicBool>,
    /// What the last publish saw ([`Fingerprint`]), to skip a publish that
    /// would change nothing.
    seen: Option<Fingerprint>,
}

/// Everything a published tree depends on that is cheap to read: equal
/// fingerprints mean an equal tree.
#[derive(Clone, Debug, PartialEq)]
struct Fingerprint {
    generations: (u64, u64, u64),
    focused: Option<NodeId>,
    root: Option<NodeId>,
    title: String,
    scale: f64,
    size: (f64, f64),
}

struct Activation {
    window: u32,
    owed: Arc<AtomicBool>,
    proxy: EventLoopProxy<HostWake>,
}

impl ActivationHandler for Activation {
    /// Called on the adapter's thread, under its lock: the tree is the
    /// loop's, so none is answered here; a whole one is marked owed and the
    /// loop woken to send it.
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        self.owed.store(true, Ordering::SeqCst);
        let _ = self
            .proxy
            .send_event(HostWake::AccessActivated(self.window));
        None
    }
}

struct Actions {
    window: u32,
    proxy: EventLoopProxy<HostWake>,
}

impl ActionHandler for Actions {
    fn do_action(&mut self, request: ActionRequest) {
        // The loop is gone only once the program is ending.
        let _ = self
            .proxy
            .send_event(HostWake::Action(self.window, request));
    }
}

/// Nothing to do when a tool stops listening: the adapter stops calling for
/// updates, and the next activation asks for a whole tree.
struct Deactivation;

impl DeactivationHandler for Deactivation {
    fn deactivate_accessibility(&mut self) {}
}

impl WindowAccess {
    /// The adapter for program window `id`, made while `window` is still
    /// hidden.
    pub(crate) fn new(
        event_loop: &ActiveEventLoop,
        window: &Window,
        id: u32,
        proxy: &EventLoopProxy<HostWake>,
    ) -> WindowAccess {
        let initial_owed = Arc::new(AtomicBool::new(false));
        let adapter = accesskit_winit::Adapter::with_direct_handlers(
            event_loop,
            window,
            Activation {
                window: id,
                owed: Arc::clone(&initial_owed),
                proxy: proxy.clone(),
            },
            Actions {
                window: id,
                proxy: proxy.clone(),
            },
            Deactivation,
        );
        WindowAccess {
            adapter,
            publisher: AccessPublisher::default(),
            initial_owed,
            seen: None,
        }
    }

    /// Hands the adapter a window event (it follows the window's place,
    /// size and focus itself).
    pub(crate) fn process_event(&mut self, window: &Window, event: &winit::event::WindowEvent) {
        self.adapter.process_event(window, event);
    }

    /// Publishes program window `id`'s tree if a tool listens and something
    /// it can see may have changed (the module documentation).
    pub(crate) fn publish(&mut self, id: u32) {
        let Some(fingerprint) = fingerprint(id) else {
            return;
        };
        let owed = self.initial_owed.load(Ordering::SeqCst);
        if !owed && self.seen.as_ref() == Some(&fingerprint) {
            return;
        }
        self.seen = Some(fingerprint);
        let WindowAccess {
            adapter,
            publisher,
            initial_owed,
            ..
        } = self;
        adapter.update_if_active(|| {
            let full = initial_owed.swap(false, Ordering::SeqCst);
            publisher.update(snapshot(id).unwrap_or_else(empty_tree), full)
        });
    }
}

fn empty_tree() -> AccessTree {
    AccessTree {
        nodes: Vec::new(),
        focus: crate::ui::WINDOW_NODE,
    }
}

/// What window `id`'s published tree depends on, or `None` once it is
/// closed.
fn fingerprint(id: u32) -> Option<Fingerprint> {
    let facts = window_facts(id)?;
    let (generations, focused) = ui::with_tree(|tree| {
        let (layout, paint) = tree.generations();
        ((layout, paint, tree.semantics_generation()), tree.focused())
    });
    Some(Fingerprint {
        generations,
        focused,
        root: facts.root,
        title: facts.title,
        scale: facts.scale,
        size: facts.size,
    })
}

/// What the window node says, and what is mounted.
struct WindowFacts {
    title: String,
    /// Device pixels per dp.
    scale: f64,
    /// In dp.
    size: (f64, f64),
    root: Option<NodeId>,
}

fn window_facts(id: u32) -> Option<WindowFacts> {
    STATE.with(|state| {
        let state = state.borrow();
        let slot = state.windows.get(&id)?;
        Some(WindowFacts {
            title: slot.title.clone(),
            scale: slot.content_scale(),
            size: slot.size(),
            root: slot.ui_root.as_ref().map(ui::Node::id),
        })
    })
}

/// Window `id`'s tree as it is now, converted.
fn snapshot(id: u32) -> Option<AccessTree> {
    let facts = window_facts(id)?;
    let window = AccessWindow {
        title: &facts.title,
        scale: facts.scale,
        width: facts.size.0 as f32,
        height: facts.size.1 as f32,
    };
    Some(ui::with_tree(|tree| tree.access_tree(facts.root, window)))
}

// ------------------------------------------------------------- actions ---

/// Queues an assistive tool's `request` for program window `id`: carried
/// out in a turn, by [`perform`].
pub(crate) fn request(id: u32, request: ActionRequest) {
    with_window(id, |slot| slot.access_requests.push_back(request));
}

/// Carries out `request` in `root`'s tree, inside a turn (the module
/// documentation's "Actions").
pub(crate) fn perform(id: u32, root: &ui::Node, request: &ActionRequest) {
    let root_id = root.id();
    let steps = ui::with_tree(|tree| tree.plan_access_action(root_id, request));
    for step in steps {
        match step {
            AccessStep::Focus(node) => ui::focus_element(node),
            AccessStep::Reveal(node) => ui::reveal_element(node),
            AccessStep::Click(node) => {
                let Some(point) = ui::with_tree(|tree| tree.access_click_point(root_id, node))
                else {
                    // Covered, or showing nothing: a pointer there would
                    // reach something else, so nothing is clicked.
                    return;
                };
                queue(id, pointer(raw_input::POINTER_MOVED, point));
                queue(id, pointer(raw_input::POINTER_PRESSED, point));
                queue(id, pointer(raw_input::POINTER_RELEASED, point));
            }
            AccessStep::Key {
                name,
                named,
                shift,
                control,
            } => {
                let modifiers = f64::from(u8::from(shift) | u8::from(control) << 1);
                for kind in [raw_input::KEY_PRESSED, raw_input::KEY_RELEASED] {
                    queue(
                        id,
                        raw(
                            kind,
                            &[0.0, modifiers, f64::from(u8::from(named))],
                            Some(name),
                        ),
                    );
                }
            }
            AccessStep::Text(text) => queue(id, raw(raw_input::TEXT_INPUT, &[], Some(&text))),
            AccessStep::Wheel { node, delta } => {
                let Some(point) = ui::with_tree(|tree| tree.access_click_point(root_id, node))
                else {
                    return;
                };
                queue(
                    id,
                    raw(
                        raw_input::SCROLLED,
                        &[
                            f64::from(point.x),
                            f64::from(point.y),
                            f64::from(delta.x),
                            f64::from(delta.y),
                            0.0,
                        ],
                        None,
                    ),
                );
            }
        }
    }
}

fn queue(id: u32, event: RawEvent) {
    with_window(id, |slot| enqueue(&mut slot.events, event));
}

fn raw(kind: i32, numbers: &[f64], text: Option<&str>) -> RawEvent {
    let mut all = [0.0; NUMBERS];
    all[..numbers.len()].copy_from_slice(numbers);
    RawEvent {
        kind,
        numbers: all,
        text: text.map(str::to_string),
    }
}

/// The primary button's event (or a move) at `point`, no modifiers.
fn pointer(kind: i32, point: Point) -> RawEvent {
    raw(
        kind,
        &[f64::from(point.x), f64::from(point.y), 0.0, 0.0],
        None,
    )
}
