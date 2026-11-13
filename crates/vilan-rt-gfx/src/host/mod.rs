//! The vilan program's side of `std::gfx`: the functions the emitted Rust of
//! a program that reaches `std::gfx` calls (`vilan/std/src/gfx.vl` binds
//! each as an `__gfx_*` extern), and the HOST that runs the program's event
//! loop inside the window system's (native-ui.md R8, section 11.4).
//!
//! # One thread
//!
//! The window system's loop (`winit`) owns the process's main thread, and the
//! vilan program runs ON THAT SAME THREAD, driven through the executor's
//! hosted turn ([`vilan_rt::executor::turn`], [`Program::turn`]): R8, RULED,
//! instead of the second thread `std::window` (M2) uses. So a program's
//! signals, scene and text are on the thread that draws, a resize is handled
//! in step (the new size is recorded, a turn runs, the frame is presented,
//! all inside the one `Resized` handler), and nothing crosses threads but a
//! wake.
//!
//! The emitted `main` of such a program is the usual one with three changes
//! (`vilan-rust`): [`install`] first, which opens the window system's loop on
//! the main thread and makes the executor a hosted one whose wake is that
//! loop's proxy (`vilan_rt_poll::install_hosted`, so sockets are waited on by
//! a helper thread only while there are sockets), and the loop functions
//! [`run_pending`] / [`block_on`] in place of the executor's owned ones. Each
//! runs the window system's loop ([`shell`]) and turns the program inside it:
//!
//! - program code runs ONLY inside a turn — a window event is queued on its
//!   window's [`WindowSource`] and then a turn is run, and the source hands
//!   it to the program;
//! - `Turn::Idle { deadline }` becomes `ControlFlow::WaitUntil(deadline)`, or
//!   `Wait` with no deadline: an idle program costs no CPU and draws nothing;
//! - `Turn::Finished` ends the window system's loop, and the loop function
//!   returns as the owned one would;
//! - a root failure that raises out of a turn is caught in the handler, the
//!   loop exits, and the failure is raised again once `run_app` has returned,
//!   into the emitted `main`'s guard (exit code 1, node's message).
//!
//! When the window system cannot start (no display), [`install`] records why,
//! every `open` answers that reason as an `Err`, and the loop functions are
//! the owned ones: the program runs as it would without `std::gfx`.
//!
//! # Frames, on demand
//!
//! A frame is drawn when the window system asks for one (`RedrawRequested`),
//! which happens when the program asks ([`request_redraw`], or setting its
//! frame callback), when the window opens, resizes or changes scale, and
//! when the system exposes it. The frame callback runs INSIDE a turn, from the
//! window's source, filling a [`SceneBuilder`] through the drawing externs
//! ([`rectangle`] and the rest), and the host presents the finished scene
//! when the turn returns. Nothing draws on a timer.
//!
//! # Windows
//!
//! `open` is ASYNC: the window is made by the host between turns, where the
//! window system's loop is at hand (winit makes windows only there), and the
//! answer — the window's id, or why it could not be made (no GPU adapter, a
//! surface the system refused) — reaches the program from the window's
//! source in the next turn. A window crosses as its id; everything else
//! crosses as numbers, strings, and the two host values `std::gfx` declares
//! (`TextLayout`, `Texture`: this crate's [`TextLayout`] and [`Image`]).
//!
//! Closing the LAST window stops the program the way SIGTERM does
//! (`vilan_rt::executor::request_termination`): servers drain, the loop ends,
//! `main` returns, exit code 0 — as `std::window` does. A close request (the
//! window's close button) is delivered to the program as an event, and the
//! window closes after that turn.
//!
//! # Not here yet
//!
//! More than one window is not refused, but the first version is built and
//! proven for one (native-ui.md section 8). The user's text-scale setting is
//! not read: the text scale is 1. No input method (IME) composition.

mod access;
mod input;
mod shell;
pub mod ui;

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use vilan_rt::bytes::Bytes;
use vilan_rt::executor::{Interest, IoSource, Program, Task, promise, register_io};
use vilan_rt::{Js, Json, Str, panic_with, str_new};
use winit::event_loop::EventLoop;
use winit::window::Window;

use crate::color::{Color, Fill, LinearGradient};
use crate::geometry::{Corners, Edges, Point, Rect, Transform};
use crate::gpu::{AdapterPreference, GpuContext};
use crate::offscreen::OffscreenRenderer;
use crate::scene::{Border, Image, Rectangle, Scene, SceneBuilder, Shadow, Underline};
use crate::text::{FontFamily, TextLayout, TextStyle, TextSystem};
use crate::units::Units;

pub(crate) use input::RawEvent;

/// The user event the window system's loop is woken with, from any thread:
/// the executor's wake, or an accessibility adapter's (`access.rs`).
#[derive(Debug)]
pub(crate) enum HostWake {
    Executor,
    /// An assistive tool started listening to program window `0`: a whole
    /// tree is owed.
    AccessActivated(u32),
    /// An assistive tool asks program window `0` for an action.
    Action(u32, accesskit::ActionRequest),
}

/// The text scale every frame is drawn at in the first version: the user's
/// setting is not read yet.
pub(crate) const TEXT_SCALE: f64 = 1.0;

/// How an open's answer reaches the program: the promise's resolve.
type Answer = Rc<dyn Fn(Result<u32, Str>)>;

/// Whether the window system started, decided once by [`install`].
enum WindowSystem {
    /// [`install`] has not run on this thread: a program whose `main` was not
    /// emitted for `std::gfx` (a unit test, the leak census's thread).
    NotInstalled,
    Unavailable(String),
    Ready,
}

/// What an open (or opening) window is, as the program and the host share
/// it. The GPU half — the surface and its renderer — is the host's alone
/// ([`shell`]), because no extern ever needs it.
pub(crate) struct WindowSlot {
    pub(crate) title: String,
    /// The size asked for, in dp, until the window exists.
    pub(crate) requested: (f64, f64),
    pub(crate) background: Color,
    /// `None` until the host has made the window.
    pub(crate) window: Option<Arc<Window>>,
    /// The open's answer, once the host has one, for the source to deliver.
    pub(crate) answer: Option<Result<u32, Str>>,
    pub(crate) resolve: Option<Answer>,
    /// The window's size in device pixels and its scale factor.
    pub(crate) device_size: (u32, u32),
    pub(crate) scale_factor: f64,
    /// Where the pointer was last seen, in dp: a button event carries no
    /// position of its own.
    pub(crate) pointer: (f64, f64),
    pub(crate) modifiers: winit::keyboard::ModifiersState,
    /// Whether the window has the system's keyboard focus (winit's
    /// `Focused`; F176): `false` until the system says it has.
    pub(crate) focused: bool,
    pub(crate) on_frame: Option<Rc<dyn Fn()>>,
    pub(crate) on_event: Option<Rc<dyn Fn()>>,
    /// The `std::ui` tree mounted in the window (`ui::mount`): its events
    /// are routed into it before `on_event` hears them.
    pub(crate) ui_root: Option<ui::Node>,
    /// How long the last presented frame took to draw (encode, submit,
    /// present), in microseconds.
    pub(crate) last_draw_us: f64,
    /// Frames asked for (`request_redraw`) since the window was asked for:
    /// what the pins of "a change asks for a frame" read.
    pub(crate) redraw_requests: u64,
    pub(crate) events: VecDeque<RawEvent>,
    /// A frame is due: the source builds it in the next turn.
    pub(crate) frame_due: bool,
    /// A frame built and not yet presented.
    pub(crate) scene: Option<Scene>,
    /// The program closed the window, or its close request was delivered:
    /// the host closes it after this turn.
    pub(crate) closing: bool,
    /// Actions an assistive tool asked for, carried out in turns
    /// (`access.rs`).
    pub(crate) access_requests: VecDeque<accesskit::ActionRequest>,
}

impl WindowSlot {
    /// Device pixels per dp: the scale factor times the text scale, the
    /// scale every frame of this window is drawn at.
    pub(crate) fn content_scale(&self) -> f64 {
        self.scale_factor * TEXT_SCALE
    }

    /// The window's size in dp.
    fn size(&self) -> (f64, f64) {
        if self.window.is_none() {
            return self.requested;
        }
        let scale = self.content_scale();
        (
            f64::from(self.device_size.0) / scale,
            f64::from(self.device_size.1) / scale,
        )
    }
}

/// Everything the program's externs and the host share, on the main thread.
pub(crate) struct State {
    system: WindowSystem,
    next_window: u32,
    pub(crate) windows: BTreeMap<u32, WindowSlot>,
    /// The event `__gfx_event_next` took last, read by the number and text
    /// externs.
    current_event: Option<RawEvent>,
}

/// A frame being filled: the token the program's `Frame` holds, the
/// builder, and what the frame callback may ask of it.
struct FrameInProgress {
    token: u32,
    builder: SceneBuilder,
    size: (f64, f64),
    scale_factor: f64,
}

thread_local! {
    /// The window system's loop, from [`install`] until a loop function runs
    /// it.
    static EVENT_LOOP: RefCell<Option<EventLoop<HostWake>>> = const { RefCell::new(None) };
    pub(crate) static STATE: RefCell<State> = const {
        RefCell::new(State {
            system: WindowSystem::NotInstalled,
            next_window: 1,
            windows: BTreeMap::new(),
            current_event: None,
        })
    };
    static FRAME: RefCell<Option<FrameInProgress>> = const { RefCell::new(None) };
    static NEXT_FRAME_TOKEN: Cell<u32> = const { Cell::new(1) };
    /// The program's fonts: one text system per program (font discovery is
    /// not free), made on first use.
    static TEXT: RefCell<Option<TextSystem>> = const { RefCell::new(None) };
    /// The device offscreen frames are drawn with, per adapter preference,
    /// made on the first offscreen frame and kept: making one costs a
    /// hundred milliseconds and more. NEVER DROPPED: `wgpu` reads thread-local
    /// storage of its own when a device or queue drops, and at thread exit
    /// that storage may already be gone (the process aborted, "cannot access
    /// a Thread Local Storage value during or after destruction", in
    /// `wgpu_core::snatch` under a debug build). The thread is the program's
    /// main thread, so its end is the process's, and the system takes the
    /// device back.
    static OFFSCREEN_DEVICES: std::mem::ManuallyDrop<RefCell<Vec<(AdapterPreference, GpuContext)>>> =
        const { std::mem::ManuallyDrop::new(RefCell::new(Vec::new())) };
}

/// Runs `body` with the program's text system.
pub(crate) fn with_text<R>(body: impl FnOnce(&mut TextSystem) -> R) -> R {
    TEXT.with(|text| body(text.borrow_mut().get_or_insert_with(TextSystem::new)))
}

/// `vilan_rt_gfx::host::install()` — the first line of a `std::gfx`
/// program's `main`: opens the window system's loop on this (the main)
/// thread and makes the executor's loop a hosted one woken through it. With
/// no window system, records why and installs the owned loop's readiness
/// wait instead.
pub fn install() {
    match EventLoop::<HostWake>::with_user_event().build() {
        Ok(event_loop) => {
            let proxy = Mutex::new(event_loop.create_proxy());
            vilan_rt_poll::install_hosted(Arc::new(move || {
                // The loop is gone only once the program is ending.
                let _ = proxy
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .send_event(HostWake::Executor);
            }));
            EVENT_LOOP.with(|slot| *slot.borrow_mut() = Some(event_loop));
            STATE.with(|state| state.borrow_mut().system = WindowSystem::Ready);
        }
        Err(error) => {
            vilan_rt_poll::install();
            STATE.with(|state| {
                state.borrow_mut().system = WindowSystem::Unavailable(format!(
                    "cannot open a window: the window system did not start ({error})"
                ))
            });
        }
    }
}

fn take_event_loop() -> Option<EventLoop<HostWake>> {
    EVENT_LOOP.with(|slot| slot.borrow_mut().take())
}

/// `vilan_rt::executor::run_pending`'s place in a `std::gfx` program with a
/// synchronous `main`: the window system's loop, turning the executor's loop
/// until it finishes. The owned loop when there is no window system.
pub fn run_pending() {
    match take_event_loop() {
        Some(event_loop) => shell::run(event_loop, &vilan_rt::executor::turn),
        None => vilan_rt::executor::run_pending(),
    }
}

/// `vilan_rt::executor::block_on`'s place in a `std::gfx` program with an
/// `async` `main`: the root task started, the window system's loop turning
/// it until the loop finishes, and the root's value. The owned loop when
/// there is no window system.
pub fn block_on<T: Clone + 'static>(body: impl Future<Output = T> + 'static) -> T {
    match take_event_loop() {
        Some(event_loop) => {
            let program = Program::start(body);
            shell::run(event_loop, &|| program.turn());
            program.finish()
        }
        None => vilan_rt::executor::block_on(body),
    }
}

// ------------------------------------------------------------- windows ---

/// `__gfx_open(title, width, height, red, green, blue, alpha)` —
/// `WindowBuilder::open`: a window `width` x `height` dp, titled `title`,
/// cleared to the background colour every frame. Answers once the host has
/// made the window, or why it could not.
#[allow(
    clippy::too_many_arguments,
    reason = "an extern's arguments are the vilan call's, one scalar each"
)]
pub fn open(
    title: &Str,
    width: f64,
    height: f64,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) -> Task<Result<u32, Str>> {
    let refusal = STATE.with(|state| match &state.borrow().system {
        WindowSystem::Ready => None,
        WindowSystem::Unavailable(reason) => Some(reason.clone()),
        WindowSystem::NotInstalled => Some(
            "cannot open a window: this program's `main` did not start the window system"
                .to_string(),
        ),
    });
    let refusal = refusal.or_else(|| {
        (!(width >= 1.0 && height >= 1.0 && width.is_finite() && height.is_finite())).then(|| {
            format!("cannot open a window of {width} x {height} dp: both sides must be at least 1")
        })
    });
    if let Some(reason) = refusal {
        return promise(Rc::new(move |resolve: Answer| {
            resolve(Err(str_new(&reason)))
        }));
    }
    let id = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let id = state.next_window;
        state.next_window += 1;
        state.windows.insert(
            id,
            WindowSlot {
                title: title.to_string(),
                requested: (width, height),
                background: Color::new(red as f32, green as f32, blue as f32, alpha as f32),
                window: None,
                answer: None,
                resolve: None,
                device_size: (0, 0),
                scale_factor: 1.0,
                pointer: (0.0, 0.0),
                modifiers: winit::keyboard::ModifiersState::empty(),
                focused: false,
                on_frame: None,
                on_event: None,
                ui_root: None,
                last_draw_us: 0.0,
                redraw_requests: 0,
                events: VecDeque::new(),
                frame_due: false,
                scene: None,
                closing: false,
                access_requests: VecDeque::new(),
            },
        );
        id
    });
    register_io(Rc::new(WindowSource { id }));
    let task = promise(Rc::new(move |resolve: Answer| {
        STATE.with(|state| {
            if let Some(slot) = state.borrow_mut().windows.get_mut(&id) {
                slot.resolve = Some(resolve);
            }
        })
    }));
    // No wake: program code runs only inside a turn (or before the first),
    // and the host makes every window asked for when the turn returns.
    task
}

/// Runs `body` on window `id`'s slot, if it is open.
fn with_window<R>(id: u32, body: impl FnOnce(&mut WindowSlot) -> R) -> Option<R> {
    STATE.with(|state| state.borrow_mut().windows.get_mut(&id).map(body))
}

/// `__gfx_close(id)` — `Window::close`. Closing a closed window does
/// nothing; closing the last one stops the program.
pub fn close(id: u32) {
    with_window(id, |slot| slot.closing = true);
}

/// `__gfx_request_redraw(id)` — a frame, soon. Many requests before the frame
/// is drawn are one frame.
pub fn request_redraw(id: u32) {
    with_window(id, |slot| {
        slot.redraw_requests += 1;
        if let Some(window) = &slot.window {
            window.request_redraw();
        }
    });
}

/// `__gfx_on_frame(id, callback)` — the window's frame callback, replacing
/// the one before; a frame is requested, so the new callback is seen.
pub fn on_frame(id: u32, callback: Rc<dyn Fn()>) {
    with_window(id, |slot| slot.on_frame = Some(callback));
    request_redraw(id);
}

/// `__gfx_on_event(id, callback)` — called, inside a turn, whenever events
/// are waiting for the window; it reads them with [`event_next`].
pub fn on_event(id: u32, callback: Rc<dyn Fn()>) {
    with_window(id, |slot| slot.on_event = Some(callback));
}

/// `__gfx_window_number(id, which)` — the window's width (0) and height (1)
/// in dp, its scale factor (2), and whether it has the system's keyboard
/// focus (3: 1 or 0). A closed window answers 0.
pub fn window_number(id: u32, which: i32) -> f64 {
    with_window(id, |slot| match which {
        0 => slot.size().0,
        1 => slot.size().1,
        2 => slot.scale_factor,
        3 => f64::from(u8::from(slot.focused)),
        _ => panic_with(&format!("{which} is not a `std::gfx` window number")),
    })
    .unwrap_or(0.0)
}

/// `__gfx_event_next(id)` — takes window `id`'s next event, answering its
/// kind (the table in `input.rs`), or -1 when none is waiting.
pub fn event_next(id: u32) -> i32 {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let next = state
            .windows
            .get_mut(&id)
            .and_then(|slot| slot.events.pop_front());
        let kind = next.as_ref().map_or(-1, |event| event.kind);
        state.current_event = next;
        kind
    })
}

/// `__gfx_event_number(index)` — a number of the event `event_next` took.
pub fn event_number(index: i32) -> f64 {
    STATE.with(|state| {
        let state = state.borrow();
        let event = state
            .current_event
            .as_ref()
            .unwrap_or_else(|| panic_with("no `std::gfx` event is being read"));
        usize::try_from(index)
            .ok()
            .and_then(|index| event.numbers.get(index).copied())
            .unwrap_or_else(|| panic_with(&format!("{index} is not a `std::gfx` event number")))
    })
}

/// `__gfx_event_integer(index)` — a number of the event `event_next` took
/// that is a whole number (a button, the modifier bits, a flag).
pub fn event_integer(index: i32) -> i32 {
    event_number(index) as i32
}

/// `__gfx_event_current()` — the kind of the event being delivered (the one
/// `event_next` took last, or the one a `std::ui` handler runs for), or -1.
pub fn event_current() -> i32 {
    STATE.with(|state| {
        state
            .borrow()
            .current_event
            .as_ref()
            .map_or(-1, |event| event.kind)
    })
}

/// `__gfx_event_text()` — the string of the event `event_next` took (a key's
/// name, typed text), or "" for an event that carries none.
pub fn event_text() -> Str {
    STATE.with(|state| {
        str_new(
            state
                .borrow()
                .current_event
                .as_ref()
                .and_then(|event| event.text.as_deref())
                .unwrap_or(""),
        )
    })
}

/// A window's events and frames, as the program's loop sees them: live from
/// the moment it is asked for until it is closed, watching nothing — the
/// host queues its events and runs a turn, and its poll hands them to the
/// program, inside that turn.
struct WindowSource {
    id: u32,
}

/// What a window's source has to do next, decided with the state borrowed
/// and done without it (the program's callbacks call back into the state).
enum Work {
    Answer(Answer, Result<u32, Str>),
    Events(Rc<dyn Fn()>),
    /// A mounted tree's events, then (if the program listens too) its
    /// callback, which reads the same events again.
    UiEvents(ui::Node, Vec<RawEvent>, Option<Rc<dyn Fn()>>),
    Frame(Option<Rc<dyn Fn()>>, (f64, f64), f64),
    /// An assistive tool's action in a mounted tree (`access.rs`).
    Access(ui::Node, accesskit::ActionRequest),
    Nothing,
}

impl IoSource for WindowSource {
    /// One kind of work per poll, in order: the open's answer, then the
    /// waiting events, then a due frame. A `true` sends the loop back through
    /// the microtasks before the next poll, so a frame is built after every
    /// reaction to the events before it has settled.
    fn poll(&self) -> bool {
        let work = STATE.with(|state| {
            let mut state = state.borrow_mut();
            let Some(slot) = state.windows.get_mut(&self.id) else {
                return Work::Nothing;
            };
            if let Some(answer) = slot.answer.take() {
                let resolve = slot.resolve.take();
                if answer.is_err() {
                    // A window that could not be made never existed: its
                    // slot goes with the answer, and its source with it.
                    state.windows.remove(&self.id);
                }
                return match resolve {
                    Some(resolve) => Work::Answer(resolve, answer),
                    None => Work::Nothing,
                };
            }
            // An assistive tool's action, when no window event waits: one
            // at a time, since it may queue window input of its own.
            if slot.events.is_empty() && !slot.access_requests.is_empty() {
                match &slot.ui_root {
                    Some(root) => {
                        let request = slot.access_requests.pop_front().expect("one waits");
                        return Work::Access(root.clone(), request);
                    }
                    // Nothing mounted: nothing can act.
                    None => slot.access_requests.clear(),
                }
            }
            if !slot.events.is_empty()
                && let Some(root) = &slot.ui_root
            {
                let events: Vec<RawEvent> = slot.events.iter().cloned().collect();
                if slot.on_event.is_none() {
                    slot.events.clear();
                }
                return Work::UiEvents(root.clone(), events, slot.on_event.clone());
            }
            if !slot.events.is_empty() {
                match &slot.on_event {
                    Some(callback) => return Work::Events(Rc::clone(callback)),
                    // Nobody listens: the events are dropped, as a browser
                    // drops an event with no listener — and the poll goes on
                    // to a due frame. Answering `Nothing` here left that
                    // frame due with no turn owed for it: a window with no
                    // event callback that the pointer entered (a crossing
                    // event is queued with the frame's request) never drew.
                    None => slot.events.clear(),
                }
            }
            if slot.frame_due && slot.window.is_some() {
                slot.frame_due = false;
                return Work::Frame(slot.on_frame.clone(), slot.size(), slot.scale_factor);
            }
            Work::Nothing
        });
        match work {
            Work::Answer(resolve, answer) => {
                resolve(answer);
                true
            }
            Work::Events(callback) => {
                callback();
                true
            }
            Work::UiEvents(root, events, callback) => {
                for event in &events {
                    ui::dispatch(&root, event);
                }
                if let Some(callback) = callback {
                    callback();
                }
                true
            }
            Work::Frame(callback, size, scale_factor) => {
                let scene = build_frame(size, scale_factor, callback.as_deref());
                with_window(self.id, |slot| slot.scene = Some(scene));
                true
            }
            Work::Access(root, request) => {
                access::perform(self.id, &root, &request);
                true
            }
            Work::Nothing => false,
        }
    }

    fn interest(&self, _interest: &mut Interest) -> bool {
        true
    }

    fn is_live(&self) -> bool {
        STATE.with(|state| state.borrow().windows.contains_key(&self.id))
    }
}

// --------------------------------------------------------------- frames ---

/// Runs `draw` (a frame callback, which reads [`frame_number`] and calls the
/// drawing externs) with a fresh builder, and answers the scene it built.
/// Nests: a frame drawn while another is being filled (an offscreen frame
/// from inside a window's frame callback) keeps the outer one aside.
fn build_frame(size: (f64, f64), scale_factor: f64, draw: Option<&dyn Fn()>) -> Scene {
    let token = NEXT_FRAME_TOKEN.with(|next| {
        let token = next.get();
        next.set(token.wrapping_add(1).max(1));
        token
    });
    let outer = FRAME.with(|frame| {
        frame.borrow_mut().replace(FrameInProgress {
            token,
            builder: SceneBuilder::new(),
            size,
            scale_factor,
        })
    });
    // Restores the outer frame whatever the road out — a panic in the
    // callback included, which the host re-raises as the program's failure.
    struct Restore(Option<Option<FrameInProgress>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            if let Some(outer) = self.0.take() {
                FRAME.with(|frame| *frame.borrow_mut() = outer);
            }
        }
    }
    let mut restore = Restore(Some(outer));
    if let Some(draw) = draw {
        draw();
    }
    let finished = FRAME.with(|frame| {
        let mut frame = frame.borrow_mut();
        let built = frame.take().expect("the frame being built");
        *frame = restore.0.take().expect("the outer frame");
        built
    });
    finished
        .builder
        .finish()
        .unwrap_or_else(|error| panic_with(&format!("a `std::gfx` frame's scene: {error}")))
}

/// Runs `body` on the frame `token` names — the one being filled — or fails
/// the program: a `Frame` drawn into after its callback returned.
fn with_frame(token: u32, body: impl FnOnce(&mut SceneBuilder)) {
    let drawn = FRAME.with(|frame| match frame.borrow_mut().as_mut() {
        Some(frame) if frame.token == token => {
            body(&mut frame.builder);
            true
        }
        _ => false,
    });
    if !drawn {
        panic_with(
            "a `std::gfx` `Frame` was drawn into after its frame callback returned: a frame \
             is filled only while its callback runs",
        );
    }
}

/// `__gfx_frame_token()` — the token of the frame being filled, which the
/// program's `Frame` carries into every drawing call.
pub fn frame_token() -> u32 {
    frame_number(0) as u32
}

/// `__gfx_frame_number(which)` — the frame being filled: its token (0), its
/// width (1) and height (2) in dp, and its scale factor (3).
pub fn frame_number(which: i32) -> f64 {
    FRAME.with(|frame| {
        let frame = frame.borrow();
        let frame = frame
            .as_ref()
            .unwrap_or_else(|| panic_with("no `std::gfx` frame is being filled"));
        match which {
            0 => f64::from(frame.token),
            1 => frame.size.0,
            2 => frame.size.1,
            3 => frame.scale_factor,
            _ => panic_with(&format!("{which} is not a `std::gfx` frame number")),
        }
    })
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect::new(x as f32, y as f32, width as f32, height as f32)
}

fn corners(top_left: f64, top_right: f64, bottom_right: f64, bottom_left: f64) -> Corners {
    Corners {
        top_left: top_left as f32,
        top_right: top_right as f32,
        bottom_right: bottom_right as f32,
        bottom_left: bottom_left as f32,
    }
}

fn color(red: f64, green: f64, blue: f64, alpha: f64) -> Color {
    Color::new(red as f32, green as f32, blue as f32, alpha as f32)
}

/// `__gfx_rectangle(frame, ...)` — `Frame::rectangle`: bounds, corner radii
/// clockwise from the top left, the fill (`fill` 0: solid in the first
/// colour; 1: a linear gradient from the first colour at `start` to the
/// second at `end`), border widths clockwise from the top and the border's
/// colour. Every length in dp.
#[allow(
    clippy::too_many_arguments,
    reason = "an extern's arguments are the vilan call's, one scalar each: a rectangle crosses as \
              numbers so nothing is allocated per primitive"
)]
pub fn rectangle(
    frame: u32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    radius_top_left: f64,
    radius_top_right: f64,
    radius_bottom_right: f64,
    radius_bottom_left: f64,
    fill: i32,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
    end_red: f64,
    end_green: f64,
    end_blue: f64,
    end_alpha: f64,
    start_x: f64,
    start_y: f64,
    end_x: f64,
    end_y: f64,
    border_top: f64,
    border_right: f64,
    border_bottom: f64,
    border_left: f64,
    border_red: f64,
    border_green: f64,
    border_blue: f64,
    border_alpha: f64,
) {
    let first = color(red, green, blue, alpha);
    let fill = match fill {
        0 => Fill::Solid(first),
        1 => Fill::LinearGradient(LinearGradient {
            start: Point::new(start_x as f32, start_y as f32),
            end: Point::new(end_x as f32, end_y as f32),
            start_color: first,
            end_color: color(end_red, end_green, end_blue, end_alpha),
        }),
        _ => panic_with(&format!("{fill} is not a `std::gfx` fill code")),
    };
    let rectangle = Rectangle {
        bounds: rect(x, y, width, height),
        corner_radii: corners(
            radius_top_left,
            radius_top_right,
            radius_bottom_right,
            radius_bottom_left,
        ),
        fill,
        border: Border {
            widths: Edges {
                top: border_top as f32,
                right: border_right as f32,
                bottom: border_bottom as f32,
                left: border_left as f32,
            },
            color: color(border_red, border_green, border_blue, border_alpha),
        },
    };
    with_frame(frame, |builder| builder.rectangle(rectangle));
}

/// `__gfx_shadow(frame, x, y, width, height, radius, blur, colour)`.
#[allow(
    clippy::too_many_arguments,
    reason = "an extern's arguments are the vilan call's, one scalar each"
)]
pub fn shadow(
    frame: u32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    corner_radius: f64,
    blur: f64,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) {
    let shadow = Shadow {
        bounds: rect(x, y, width, height),
        corner_radius: corner_radius as f32,
        blur: blur as f32,
        color: color(red, green, blue, alpha),
    };
    with_frame(frame, |builder| builder.shadow(shadow));
}

/// `__gfx_text(frame, layout, x, y, colour)` — a shaped paragraph with its
/// top-left corner at `(x, y)`.
#[allow(
    clippy::too_many_arguments,
    reason = "an extern's arguments are the vilan call's, one scalar each"
)]
pub fn text(
    frame: u32,
    layout: &TextLayout,
    x: f64,
    y: f64,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) {
    with_frame(frame, |builder| {
        builder.text(
            layout,
            Point::new(x as f32, y as f32),
            color(red, green, blue, alpha),
        )
    });
}

/// `__gfx_underline(frame, x, y, width, thickness, colour)`.
#[allow(
    clippy::too_many_arguments,
    reason = "an extern's arguments are the vilan call's, one scalar each"
)]
pub fn underline(
    frame: u32,
    x: f64,
    y: f64,
    width: f64,
    thickness: f64,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) {
    let underline = Underline {
        origin: Point::new(x as f32, y as f32),
        width: width as f32,
        thickness: thickness as f32,
        color: color(red, green, blue, alpha),
    };
    with_frame(frame, |builder| builder.underline(underline));
}

/// `__gfx_texture(frame, texture, x, y, width, height, radii)` — pixels
/// stretched over the bounds, clipped to the corner radii.
#[allow(
    clippy::too_many_arguments,
    reason = "an extern's arguments are the vilan call's, one scalar each"
)]
pub fn texture(
    frame: u32,
    image: &Image,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    radius_top_left: f64,
    radius_top_right: f64,
    radius_bottom_right: f64,
    radius_bottom_left: f64,
) {
    let bounds = rect(x, y, width, height);
    let radii = corners(
        radius_top_left,
        radius_top_right,
        radius_bottom_right,
        radius_bottom_left,
    );
    with_frame(frame, |builder| builder.image(image, bounds, radii));
}

/// `__gfx_push_clip(frame, x, y, width, height, radii)`.
#[allow(
    clippy::too_many_arguments,
    reason = "an extern's arguments are the vilan call's, one scalar each"
)]
pub fn push_clip(
    frame: u32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    radius_top_left: f64,
    radius_top_right: f64,
    radius_bottom_right: f64,
    radius_bottom_left: f64,
) {
    let bounds = rect(x, y, width, height);
    let radii = corners(
        radius_top_left,
        radius_top_right,
        radius_bottom_right,
        radius_bottom_left,
    );
    with_frame(frame, |builder| builder.push_clip(bounds, radii));
}

/// `__gfx_push_transform(frame, x, y, scale)` — translate, then scale
/// uniformly, what follows until its `pop`.
pub fn push_transform(frame: u32, x: f64, y: f64, scale: f64) {
    let transform = Transform {
        translation: Point::new(x as f32, y as f32),
        scale: scale as f32,
    };
    with_frame(frame, |builder| builder.push_transform(transform));
}

/// `__gfx_push_layer(frame, opacity)`.
pub fn push_layer(frame: u32, opacity: f64) {
    with_frame(frame, |builder| builder.push_layer(opacity as f32));
}

/// `__gfx_pop(frame)`.
pub fn pop(frame: u32) {
    with_frame(frame, SceneBuilder::pop);
}

// ----------------------------------------------------------------- text ---

/// `__gfx_text_layout(text, family, name, size, line_height, weight, italic,
/// max_width)` — `TextLayout::new` / `wrapped`: `text` shaped in the style,
/// on one line, or wrapped at `max_width` dp when it is not negative.
/// `family` is 0 sans-serif, 1 serif, 2 monospace, 3 the family `name`.
#[allow(
    clippy::too_many_arguments,
    reason = "an extern's arguments are the vilan call's, one scalar each"
)]
pub fn text_layout(
    text: &Str,
    family: i32,
    name: &Str,
    size: f64,
    line_height: f64,
    weight: f64,
    italic: bool,
    max_width: f64,
) -> TextLayout {
    let family = match family {
        0 => FontFamily::SansSerif,
        1 => FontFamily::Serif,
        2 => FontFamily::Monospace,
        3 => FontFamily::Named(name.to_string()),
        _ => panic_with(&format!("{family} is not a `std::gfx` font family code")),
    };
    let style = TextStyle {
        family,
        size: size as f32,
        line_height: line_height as f32,
        weight: weight as f32,
        italic,
    };
    let max_width = (max_width >= 0.0).then_some(max_width as f32);
    with_text(|system| system.layout(text, &style, max_width))
}

/// `__gfx_layout_number(layout, which)` — a layout's width (0) and height
/// (1) in dp, its first baseline in dp from its top (2), its line count (3).
pub fn layout_number(layout: &TextLayout, which: i32) -> f64 {
    match which {
        0 => f64::from(layout.size().width),
        1 => f64::from(layout.size().height),
        2 => f64::from(layout.first_baseline()),
        3 => layout.line_count() as f64,
        _ => panic_with(&format!("{which} is not a `std::gfx` layout number")),
    }
}

/// `__gfx_register_font(data)` — registers the faces in a font file, for
/// every later layout; answers the family names they registered under.
pub fn register_font(data: &Bytes) -> Result<Vec<Str>, Str> {
    with_text(|system| system.register_font(data.to_vec()))
        .map(|families| families.iter().map(|family| str_new(family)).collect())
        .map_err(|error| str_new(&error.to_string()))
}

// ------------------------------------------------------------- textures ---

/// `__gfx_texture_from_rgba(width, height, pixels)` — pixels COPIED in (the
/// caller may write the `Bytes` it gave), uploaded to the GPU on the first
/// frame that draws them.
pub fn texture_from_rgba(width: u32, height: u32, pixels: &Bytes) -> Result<Image, Str> {
    Image::from_rgba(width, height, pixels.to_vec()).map_err(|error| str_new(&error.to_string()))
}

/// `__gfx_texture_size(texture, which)` — its width (0) or height (1) in
/// pixels.
pub fn texture_size(image: &Image, which: i32) -> u32 {
    match which {
        0 => image.width(),
        1 => image.height(),
        _ => panic_with(&format!("{which} is not a `std::gfx` texture number")),
    }
}

// ------------------------------------------------------------ offscreen ---

/// `__gfx_offscreen(width, height, scale_factor, software, background,
/// draw)` — `Offscreen::render`: draws one frame of `width` x `height` dp at
/// `scale_factor` into a texture and answers its pixels (RGBA, sRGB, rows top
/// to bottom; `round(width * scale_factor)` x `round(height * scale_factor)`
/// of them). `software` asks for a software adapter only, for output that is
/// the same on every machine that has one. Synchronous: it waits for the GPU.
#[allow(
    clippy::too_many_arguments,
    reason = "an extern's arguments are the vilan call's, one scalar each"
)]
pub fn offscreen(
    width: f64,
    height: f64,
    scale_factor: f64,
    software: bool,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
    draw: Rc<dyn Fn()>,
) -> Result<Bytes, Str> {
    let units = Units::new(scale_factor as f32, TEXT_SCALE as f32)
        .map_err(|error| str_new(&error.to_string()))?;
    let device_width = (width * scale_factor).round();
    let device_height = (height * scale_factor).round();
    if !(device_width >= 1.0 && device_height >= 1.0 && device_width <= f64::from(u32::MAX))
        || device_height > f64::from(u32::MAX)
    {
        return Err(str_new(&format!(
            "an offscreen frame of {width} x {height} dp at {scale_factor} is {device_width} x \
             {device_height} device pixels: each side must be at least 1"
        )));
    }
    let preference = if software {
        AdapterPreference::Software
    } else {
        AdapterPreference::HighPerformance
    };
    let context = offscreen_device(preference).map_err(|error| str_new(&error.to_string()))?;
    let mut renderer = OffscreenRenderer::new(context, device_width as u32, device_height as u32)
        .map_err(|error| str_new(&error.to_string()))?;
    let scene = build_frame((width, height), scale_factor, Some(&*draw));
    let background = color(red, green, blue, alpha);
    with_text(|text| renderer.render(&scene, units, text, background))
        .map_err(|error| str_new(&error.to_string()))?;
    renderer
        .read_rgba()
        .map(Bytes::from_vec)
        .map_err(|error| str_new(&error.to_string()))
}

fn offscreen_device(preference: AdapterPreference) -> Result<GpuContext, crate::GfxError> {
    let cached = OFFSCREEN_DEVICES.with(|devices| {
        devices
            .borrow()
            .iter()
            .find(|(kept, _)| *kept == preference)
            .map(|(_, context)| context.clone())
    });
    if let Some(context) = cached {
        return Ok(context);
    }
    let context = GpuContext::headless(preference)?;
    OFFSCREEN_DEVICES.with(|devices| devices.borrow_mut().push((preference, context.clone())));
    Ok(context)
}

// ------------------------------------------- the two host values' faces ---

/// Identity, as a host object's is: two layouts are one when they are the
/// same shaped value.
impl PartialEq for TextLayout {
    fn eq(&self, other: &TextLayout) -> bool {
        Arc::ptr_eq(&self.data, &other.data)
    }
}

impl Js for TextLayout {
    fn js(&self) -> String {
        panic_with(
            "printing a `TextLayout` is a host object's own inspection, which has no text form \
             (print its `size()` or `line_count()`)",
        )
    }
}

impl Json for TextLayout {
    fn json(&self) -> String {
        panic_with("a `TextLayout` is a host object, which has no JSON form")
    }
}

/// Identity, as `std::image::Image`'s is.
impl PartialEq for Image {
    fn eq(&self, other: &Image) -> bool {
        Arc::ptr_eq(&self.data, &other.data)
    }
}

impl Js for Image {
    fn js(&self) -> String {
        panic_with(
            "printing a `Texture` is a host object's own inspection, which has no text form \
             (print its `width()` and `height()`)",
        )
    }
}

impl Json for Image {
    fn json(&self) -> String {
        panic_with("a `Texture` is a host object, which has no JSON form")
    }
}

#[cfg(test)]
mod tests;
