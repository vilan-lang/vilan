//! The host: the window system's loop (`winit`) on the main thread, turning
//! the vilan program's executor inside its handlers (native-ui.md R8). See
//! the module above for the arrangement; this file is its mechanics.
//!
//! Every handler that has something for the program queues it and either
//! turns at once (a frame asked for, a resize, a close request: work that
//! ends in a presented frame or a closed window, done in step) or notes that
//! a turn is owed, which `about_to_wait` pays once for the whole batch of
//! events the system delivered (pointer motion, keys, the executor's wake, a
//! deadline reached). After every turn [`Shell::settle`] does what only the
//! host can: make the windows asked for, close the windows closed, present
//! the frames built.
//!
//! # The wakes after a present (F169)
//!
//! On X11 the window and its GPU surface share ONE connection to the X server
//! (`winit`'s, handed to `wgpu` as the surface's display), and the driver may
//! read that connection from a thread of its own while it finishes a present:
//! Mesa's swapchain thread ("WSI swapchain q") asks the server for the
//! window's geometry and waits for the reply, and every event that arrives
//! meanwhile — a key, the window manager's close — is read off the socket
//! into the client-side queue on that thread. `winit` checks that queue only
//! when its loop wakes, and its loop sleeps on the socket, which then has
//! nothing to read: the event waits for the next one to arrive. Observed
//! under xvfb with Mesa's software Vulkan (about 5 runs in 100 of the 10,000-
//! row list lost a key or the close); traced to the thread by reading which
//! thread took each event off the socket.
//!
//! So after every presented frame the loop wakes a few times on its own, on
//! [`PresentDrain`]'s schedule, and each wake has `winit` drain the queue: an
//! event the driver's thread read in that time reaches the program within the
//! next step of the schedule. The schedule ends two seconds after the last
//! present, and the loop sleeps as before — an idle window costs no wake.

use std::any::Any;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::Arc;
use std::time::{Duration, Instant};

use vilan_rt::executor::Turn;
use vilan_rt::{panic_with, str_new};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{Ime, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use super::input::{RawEvent, enqueue};
use super::{HostWake, STATE, TEXT_SCALE, with_text};
use crate::units::Units;
use crate::window::{FrameOutcome, WindowRenderer};

/// Runs the window system's loop until the program's loop finishes (or
/// fails), turning it with `turn`. A failure raised out of a turn is raised
/// again here, after the loop has ended.
pub(crate) fn run(event_loop: EventLoop<HostWake>, turn: &dyn Fn() -> Turn) {
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut shell = Shell {
        shares_x_connection: shares_x_connection(&event_loop),
        turn,
        renderers: HashMap::new(),
        ids: HashMap::new(),
        started: false,
        owes_a_turn: false,
        deadline: None,
        drain: PresentDrain::default(),
        finished: false,
        failure: None,
    };
    let ran = event_loop.run_app(&mut shell);
    let failure = shell.failure.take();
    // The windows go before the loop function returns: their surfaces first,
    // then the windows the program's slots still hold (a SIGTERM ends the
    // loop with a window open).
    drop(shell);
    STATE.with(|state| state.borrow_mut().windows.clear());
    if let Some(payload) = failure {
        resume_unwind(payload);
    }
    if let Err(error) = ran {
        panic_with(&format!("the window system's loop failed: {error}"));
    }
}

/// Whether the loop is `winit`'s X11 one, whose connection the GPU surface
/// shares (the module's "The wakes after a present"). Wayland's client
/// library is built for several readers (a reader announces itself and the
/// others are woken with what it read), and Windows and macOS deliver window
/// events through a queue of the thread's own: none of them needs the wakes.
#[cfg(all(unix, not(target_vendor = "apple"), not(target_os = "android")))]
fn shares_x_connection(event_loop: &EventLoop<HostWake>) -> bool {
    use winit::platform::x11::EventLoopExtX11;
    event_loop.is_x11()
}

#[cfg(not(all(unix, not(target_vendor = "apple"), not(target_os = "android"))))]
fn shares_x_connection(_event_loop: &EventLoop<HostWake>) -> bool {
    false
}

struct Shell<'turn> {
    turn: &'turn dyn Fn() -> Turn,
    /// The loop is X11's: every present starts [`PresentDrain`]'s schedule.
    shares_x_connection: bool,
    /// Each open window's surface and renderer, by the program's id.
    renderers: HashMap<u32, WindowRenderer>,
    /// The program's id of each open window.
    ids: HashMap<WindowId, u32>,
    started: bool,
    /// Something was queued for the program, or it was woken, since its last
    /// turn.
    owes_a_turn: bool,
    /// The last turn's deadline: turn again then, if nothing comes first.
    deadline: Option<Instant>,
    /// The wakes owed after the last presented frame (F169).
    drain: PresentDrain,
    finished: bool,
    failure: Option<Box<dyn Any + Send>>,
}

/// When the loop wakes after a presented frame, though nothing asked it to,
/// so `winit` reads events a driver's thread took off the shared X
/// connection (the module's "The wakes after a present"). A wake runs no turn:
/// it only lets `winit` look at its queue again.
#[derive(Default)]
pub(crate) struct PresentDrain {
    /// The last present, while some step of the schedule is still ahead.
    presented: Option<Instant>,
    /// The first step of [`PresentDrain::STEPS`] not yet passed.
    step: usize,
}

impl PresentDrain {
    /// The wakes, as times after the last present: doubling from one frame at
    /// 60 Hz, so an event read off the socket while the driver finishes a
    /// present waits at most as long again as it had been since that present,
    /// and the last a little over two seconds on. A present that takes the
    /// driver longer than that (a machine stalled for seconds) is the one
    /// case left uncovered; the next event of any kind still delivers it.
    pub(crate) const STEPS: [Duration; 8] = [
        Duration::from_millis(16),
        Duration::from_millis(32),
        Duration::from_millis(64),
        Duration::from_millis(128),
        Duration::from_millis(256),
        Duration::from_millis(512),
        Duration::from_millis(1024),
        Duration::from_millis(2048),
    ];

    /// A frame was presented at `now`: the schedule starts again from it.
    pub(crate) fn presented(&mut self, now: Instant) {
        self.presented = Some(now);
        self.step = 0;
    }

    /// The next wake still ahead of `now`, passing the ones `now` has
    /// reached; `None` once the schedule is over (and until a present).
    pub(crate) fn next_wake(&mut self, now: Instant) -> Option<Instant> {
        let presented = self.presented?;
        while let Some(step) = Self::STEPS.get(self.step) {
            let wake = presented + *step;
            if wake > now {
                return Some(wake);
            }
            self.step += 1;
        }
        self.presented = None;
        None
    }
}

/// The control flow to wait with: until the earlier of the program's
/// deadline and the next drain wake, or for an event when there is neither.
/// Never `Poll`: an idle loop sleeps.
pub(crate) fn wait_for(deadline: Option<Instant>, drain_wake: Option<Instant>) -> ControlFlow {
    match (deadline, drain_wake) {
        (Some(deadline), Some(wake)) => ControlFlow::WaitUntil(deadline.min(wake)),
        (Some(at), None) | (None, Some(at)) => ControlFlow::WaitUntil(at),
        (None, None) => ControlFlow::Wait,
    }
}

impl Shell<'_> {
    /// Turns the program until it has nothing left to do now, settling after
    /// each turn — a settle that answers an open or closes the last window
    /// owes the program another turn at once.
    fn turn_now(&mut self, event_loop: &ActiveEventLoop) {
        while !self.finished {
            self.owes_a_turn = false;
            match catch_unwind(AssertUnwindSafe(self.turn)) {
                Err(payload) => return self.fail(event_loop, payload),
                Ok(Turn::Finished(_)) => {
                    self.finished = true;
                    event_loop.exit();
                    return;
                }
                Ok(Turn::Idle { deadline, .. }) => self.deadline = deadline,
            }
            if let Err(message) = self.settle(event_loop) {
                // A failure the host found (a frame it could not present) is
                // raised as the program's: the guard's hook prints it in the
                // program's own failure shape, and the loop ends with it.
                let payload =
                    catch_unwind(|| panic_with(&message)).expect_err("`panic_with` always panics");
                return self.fail(event_loop, payload);
            }
            if !self.owes_a_turn {
                return;
            }
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, payload: Box<dyn Any + Send>) {
        self.failure = Some(payload);
        self.finished = true;
        event_loop.exit();
    }

    /// The host's half of a turn: windows made, windows closed, frames
    /// presented.
    fn settle(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        self.make_windows(event_loop);
        self.close_windows();
        self.present()
    }

    /// Makes every window the program asked for since the last settle, and
    /// leaves each answer for its source to deliver.
    fn make_windows(&mut self, event_loop: &ActiveEventLoop) {
        let asked: Vec<(u32, String, (f64, f64))> = STATE.with(|state| {
            state
                .borrow()
                .windows
                .iter()
                .filter(|(_, slot)| slot.window.is_none() && slot.answer.is_none())
                .map(|(id, slot)| (*id, slot.title.clone(), slot.requested))
                .collect()
        });
        for (id, title, (width, height)) in asked {
            let made = make_window(event_loop, &title, width, height);
            STATE.with(|state| {
                let mut state = state.borrow_mut();
                let Some(slot) = state.windows.get_mut(&id) else {
                    return;
                };
                match made {
                    Ok(renderer) => {
                        let window = Arc::clone(renderer.window());
                        let size = renderer.size();
                        slot.device_size = (size.width, size.height);
                        slot.scale_factor = window.scale_factor();
                        slot.answer = Some(Ok(id));
                        self.ids.insert(window.id(), id);
                        window.request_redraw();
                        slot.window = Some(window);
                        self.renderers.insert(id, renderer);
                    }
                    Err(reason) => slot.answer = Some(Err(str_new(&reason))),
                }
            });
            self.owes_a_turn = true;
        }
    }

    /// Closes every window the program closed or whose close request it was
    /// handed; closing the last window stops the program as SIGTERM does.
    fn close_windows(&mut self) {
        let (closed, none_left) = STATE.with(|state| {
            let mut state = state.borrow_mut();
            let closing: Vec<u32> = state
                .windows
                .iter()
                .filter(|(_, slot)| slot.closing)
                .map(|(id, _)| *id)
                .collect();
            for id in &closing {
                state.windows.remove(id);
            }
            (closing, state.windows.is_empty())
        });
        if closed.is_empty() {
            return;
        }
        for id in closed {
            if let Some(renderer) = self.renderers.remove(&id) {
                self.ids.remove(&renderer.window().id());
            }
        }
        if none_left {
            // Unless a stop is already under way: a second request ends the
            // process at once, with exit code 1.
            if !vilan_rt::executor::termination_requested() {
                vilan_rt::executor::request_termination();
            }
            self.owes_a_turn = true;
        }
    }

    /// Presents every frame the last turn built.
    fn present(&mut self) -> Result<(), String> {
        let built: Vec<(u32, crate::Scene, crate::Color, f64)> = STATE.with(|state| {
            state
                .borrow_mut()
                .windows
                .iter_mut()
                .filter_map(|(id, slot)| {
                    let scene = slot.scene.take()?;
                    Some((*id, scene, slot.background, slot.scale_factor))
                })
                .collect()
        });
        for (id, scene, background, scale_factor) in built {
            let Some(renderer) = self.renderers.get_mut(&id) else {
                continue;
            };
            let units = Units::new(scale_factor as f32, TEXT_SCALE as f32)
                .map_err(|error| error.to_string())?;
            let started = Instant::now();
            let rendered = with_text(|text| renderer.render(&scene, units, text, background));
            let took = started.elapsed().as_secs_f64() * 1.0e6;
            match rendered {
                Ok(FrameOutcome::Presented(_)) => {
                    super::with_window(id, |slot| slot.last_draw_us = took);
                    if self.shares_x_connection {
                        self.drain.presented(Instant::now());
                    }
                }
                // A minimised or covered window, or a surface that had to be
                // reconfigured: the system asks for a frame again when the
                // window changes.
                Ok(FrameOutcome::Skipped) => {}
                Err(error) => return Err(format!("a `std::gfx` window's frame failed: {error}")),
            }
        }
        Ok(())
    }

    /// Queues `event` for window `id`, and notes that the program is owed a
    /// turn.
    fn queue(&mut self, id: u32, event: RawEvent) {
        STATE.with(|state| {
            if let Some(slot) = state.borrow_mut().windows.get_mut(&id) {
                enqueue(&mut slot.events, event);
            }
        });
        self.owes_a_turn = true;
    }
}

/// A window and the renderer on its surface, or why not.
fn make_window(
    event_loop: &ActiveEventLoop,
    title: &str,
    width: f64,
    height: f64,
) -> Result<WindowRenderer, String> {
    let attributes = Window::default_attributes()
        .with_title(title)
        .with_inner_size(LogicalSize::new(width, height));
    let window = event_loop
        .create_window(attributes)
        .map_err(|error| format!("cannot open a window: {error}"))?;
    WindowRenderer::new(event_loop, Arc::new(window))
        .map_err(|error| format!("cannot draw in a window: {error}"))
}

impl ApplicationHandler<HostWake> for Shell<'_> {
    /// A wait that ended at its time owes the program a turn only when the
    /// time was the program's deadline: a drain wake ([`PresentDrain`]) is
    /// for `winit`'s queue alone, and turns the program only if that queue
    /// held an event for it.
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::ResumeTimeReached { .. })
            && self
                .deadline
                .is_some_and(|deadline| deadline <= Instant::now())
        {
            self.owes_a_turn = true;
        }
    }

    /// The first turn: the program's `main` has run to its first wait (and
    /// asked for its window, usually), before the loop started.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !self.started {
            self.started = true;
            self.turn_now(event_loop);
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: HostWake) {
        self.owes_a_turn = true;
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(&id) = self.ids.get(&window_id) else {
            return;
        };
        let content_scale = STATE.with(|state| {
            state
                .borrow()
                .windows
                .get(&id)
                .map_or(1.0, super::WindowSlot::content_scale)
        });
        match event {
            WindowEvent::RedrawRequested => {
                super::with_window(id, |slot| slot.frame_due = true);
                self.turn_now(event_loop);
            }
            WindowEvent::Resized(size) => {
                // In step: the surface follows, the program hears the new
                // size, and the frame at that size is presented before this
                // handler returns.
                if let Some(renderer) = self.renderers.get_mut(&id) {
                    renderer.resize(size);
                }
                let scale = super::with_window(id, |slot| {
                    slot.device_size = (size.width.max(1), size.height.max(1));
                    slot.frame_due = true;
                    slot.content_scale()
                })
                .unwrap_or(1.0);
                self.queue(
                    id,
                    RawEvent::resized(
                        f64::from(size.width.max(1)) / scale,
                        f64::from(size.height.max(1)) / scale,
                    ),
                );
                self.turn_now(event_loop);
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                super::with_window(id, |slot| {
                    slot.scale_factor = scale_factor;
                    slot.frame_due = true;
                });
                self.queue(id, RawEvent::scale_changed(scale_factor));
            }
            WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                // Delivered, then closed when the turn returns.
                self.queue(id, RawEvent::close_requested());
                super::with_window(id, |slot| slot.closing = true);
                self.turn_now(event_loop);
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = (position.x / content_scale, position.y / content_scale);
                super::with_window(id, |slot| slot.pointer = (x, y));
                self.queue(id, RawEvent::pointer_moved(x, y));
            }
            WindowEvent::CursorLeft { .. } => self.queue(id, RawEvent::pointer_left()),
            WindowEvent::MouseInput { state, button, .. } => {
                let ((x, y), modifiers) =
                    super::with_window(id, |slot| (slot.pointer, slot.modifiers))
                        .unwrap_or_default();
                self.queue(id, RawEvent::pointer_button(state, x, y, button, modifiers));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let ((x, y), modifiers) =
                    super::with_window(id, |slot| (slot.pointer, slot.modifiers))
                        .unwrap_or_default();
                self.queue(
                    id,
                    RawEvent::scrolled(x, y, delta, content_scale, modifiers),
                );
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                super::with_window(id, |slot| slot.modifiers = modifiers.state());
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => {
                let modifiers = super::with_window(id, |slot| slot.modifiers).unwrap_or_default();
                for raw in RawEvent::key(&event, modifiers) {
                    self.queue(id, raw);
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => self.queue(id, RawEvent::text_input(text)),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.owes_a_turn {
            self.turn_now(event_loop);
        }
        if !self.finished {
            let drain_wake = self.drain.next_wake(Instant::now());
            event_loop.set_control_flow(wait_for(self.deadline, drain_wake));
        }
    }
}
