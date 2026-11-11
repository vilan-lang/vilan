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

use std::any::Any;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::Arc;
use std::time::Instant;

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
        turn,
        renderers: HashMap::new(),
        ids: HashMap::new(),
        started: false,
        owes_a_turn: false,
        deadline: None,
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

struct Shell<'turn> {
    turn: &'turn dyn Fn() -> Turn,
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
    finished: bool,
    failure: Option<Box<dyn Any + Send>>,
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
            match with_text(|text| renderer.render(&scene, units, text, background)) {
                Ok(FrameOutcome::Presented(_)) => {}
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
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::ResumeTimeReached { .. }) {
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
            event_loop.set_control_flow(match self.deadline {
                Some(deadline) => ControlFlow::WaitUntil(deadline),
                None => ControlFlow::Wait,
            });
        }
    }
}
