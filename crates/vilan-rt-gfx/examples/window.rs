//! A window that draws a scene ON DEMAND: one frame when it opens, one per
//! resize or scale change, none while nothing changes; it exits when closed.
//!
//!     cargo run --example window                # until you close it
//!     cargo run --example window -- --self-test # the xvfb proof's script
//!
//! Every frame prints one line (`frame=N size=WxH scale=S ...`), so a test or
//! a person can see exactly when frames were drawn.
//!
//! `--self-test` (X11 only) drives the window through what `tests/window.rs`
//! checks: after the first frame it asks for a new size; after the frame at
//! that size it waits one second and reports how many frames were drawn in
//! it (an idle window draws none); then it sends ITSELF the WM_DELETE_WINDOW
//! message a window manager sends when the user clicks close, through the X
//! server, and exits 0 on the `CloseRequested` that comes back. A watchdog
//! exits 2 if the script stalls.
//!
//! The window's own loop is all there is here: how it meets the vilan
//! program's executor is native-ui.md's Q8, not this example's.

use std::sync::Arc;
use std::time::{Duration, Instant};

use vilan_rt_gfx::winit::application::ApplicationHandler;
use vilan_rt_gfx::winit::dpi::LogicalSize;
use vilan_rt_gfx::winit::event::WindowEvent;
use vilan_rt_gfx::winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use vilan_rt_gfx::winit::window::{Window, WindowId};
use vilan_rt_gfx::{
    Border, Color, FrameOutcome, LinearGradient, Point, Rect, Rectangle, Scene, SceneBuilder,
    Shadow, TextStyle, TextSystem, WindowRenderer,
};

const OPENING_SIZE: LogicalSize<f64> = LogicalSize::new(480.0, 300.0);
const RESIZED_SIZE: LogicalSize<f64> = LogicalSize::new(640.0, 400.0);
const IDLE_PERIOD: Duration = Duration::from_secs(1);
const WATCHDOG: Duration = Duration::from_secs(30);

enum UserEvent {
    IdlePeriodOver,
    WatchdogFired,
}

#[derive(Clone, Copy, PartialEq)]
enum Step {
    AwaitFirstFrame,
    AwaitResizedFrame,
    Idle { frames_before: u32 },
    Closing,
}

struct App {
    proxy: EventLoopProxy<UserEvent>,
    text: TextSystem,
    renderer: Option<WindowRenderer>,
    frames: u32,
    started: Instant,
    self_test: Option<Step>,
    failure: Option<String>,
}

/// The scene for a window `width` x `height` dp: a background, a card with a
/// shadow and a border, a gradient bar the window's width, and a caption.
/// Built from the window's size, so a resize visibly re-lays it out.
fn scene(text: &mut TextSystem, width: f32, height: f32, frame: u32) -> Scene {
    let mut scene = SceneBuilder::new();
    scene.rectangle(Rectangle::new(
        Rect::new(0.0, 0.0, width, height),
        Color::rgb(244, 244, 240),
    ));
    let card = Rect::new(24.0, 24.0, width - 48.0, height - 48.0);
    scene.shadow(Shadow {
        bounds: Rect::new(
            card.left(),
            card.top() + 3.0,
            card.size.width,
            card.size.height,
        ),
        corner_radius: 12.0,
        blur: 16.0,
        color: Color::rgba(0, 0, 0, 70),
    });
    scene.rectangle(
        Rectangle::new(card, Color::WHITE)
            .with_radius(12.0)
            .with_border(Border::all(1.0, Color::rgb(200, 200, 210))),
    );
    scene.rectangle(
        Rectangle::new(
            Rect::new(
                card.left() + 16.0,
                card.top() + 16.0,
                card.size.width - 32.0,
                8.0,
            ),
            LinearGradient {
                start: Point::new(card.left() + 16.0, 0.0),
                end: Point::new(card.right() - 16.0, 0.0),
                start_color: Color::rgb(255, 120, 0),
                end_color: Color::rgb(40, 90, 255),
            },
        )
        .with_radius(4.0),
    );
    let caption = text.layout(
        &format!("vilan-rt-gfx: {width:.0} x {height:.0} dp, frame {frame}"),
        &TextStyle::default(),
        Some(card.size.width - 32.0),
    );
    scene.text(
        &caption,
        Point::new(card.left() + 16.0, card.top() + 40.0),
        Color::rgb(30, 30, 40),
    );
    scene.finish().expect("a balanced scene")
}

impl App {
    fn fail(&mut self, event_loop: &ActiveEventLoop, reason: String) {
        println!("failure: {reason}");
        self.failure = Some(reason);
        event_loop.exit();
    }

    fn draw(&mut self, event_loop: &ActiveEventLoop) {
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        let units = match renderer.units(1.0) {
            Ok(units) => units,
            Err(error) => return self.fail(event_loop, error.to_string()),
        };
        let size = renderer.size();
        let (width, height) = (
            size.width as f32 / units.scale_factor(),
            size.height as f32 / units.scale_factor(),
        );
        let started = Instant::now();
        let scene = scene(&mut self.text, width, height, self.frames + 1);
        let built = started.elapsed();
        match renderer.render(&scene, units, &mut self.text, Color::WHITE) {
            Ok(FrameOutcome::Presented(stats)) => {
                self.frames += 1;
                println!(
                    "frame={} size={}x{} scale={} draws={} scene_us={} encode_submit_us={} since_start_ms={}",
                    self.frames,
                    size.width,
                    size.height,
                    units.scale_factor(),
                    stats.draw_calls,
                    built.as_micros(),
                    stats.cpu_time.as_micros(),
                    self.started.elapsed().as_millis(),
                );
                self.after_frame(size.width, size.height, units.scale_factor());
            }
            Ok(FrameOutcome::Skipped) => println!("frame skipped"),
            Err(error) => self.fail(event_loop, error.to_string()),
        }
    }

    fn after_frame(&mut self, width: u32, height: u32, scale_factor: f32) {
        let Some(renderer) = &self.renderer else {
            return;
        };
        match self.self_test {
            Some(Step::AwaitFirstFrame) => {
                self.self_test = Some(Step::AwaitResizedFrame);
                // Resized arrives, and with it the next frame.
                let _ = renderer.window().request_inner_size(RESIZED_SIZE);
            }
            Some(Step::AwaitResizedFrame) => {
                let wanted = (
                    (RESIZED_SIZE.width * f64::from(scale_factor)).round() as u32,
                    (RESIZED_SIZE.height * f64::from(scale_factor)).round() as u32,
                );
                if (width, height) == wanted {
                    self.self_test = Some(Step::Idle {
                        frames_before: self.frames,
                    });
                    let proxy = self.proxy.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(IDLE_PERIOD);
                        let _ = proxy.send_event(UserEvent::IdlePeriodOver);
                    });
                }
            }
            Some(Step::Idle { .. }) | Some(Step::Closing) | None => {}
        }
    }

    fn close_through_the_window_system(&mut self, event_loop: &ActiveEventLoop) {
        let Some(renderer) = &self.renderer else {
            return;
        };
        #[cfg(target_os = "linux")]
        {
            use vilan_rt_gfx::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            let window_id = match renderer
                .window()
                .window_handle()
                .map(|handle| handle.as_raw())
            {
                Ok(RawWindowHandle::Xlib(handle)) => handle.window as u32,
                Ok(RawWindowHandle::Xcb(handle)) => handle.window.get(),
                other => {
                    return self.fail(
                        event_loop,
                        format!("--self-test closes through X11; the window is {other:?}"),
                    );
                }
            };
            println!("closing window {window_id} through the X server");
            if let Err(error) = x11::send_delete_window(window_id) {
                self.fail(
                    event_loop,
                    format!("sending WM_DELETE_WINDOW failed: {error}"),
                );
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = renderer;
            self.fail(
                event_loop,
                "--self-test closes through X11, which this system lacks".to_string(),
            );
        }
    }
}

#[cfg(target_os = "linux")]
mod x11 {
    use x11rb::protocol::xproto::{ClientMessageEvent, ConnectionExt, EventMask};

    /// What a window manager sends when the user clicks a window's close
    /// button: a WM_PROTOCOLS client message naming WM_DELETE_WINDOW, sent
    /// to the window itself.
    pub fn send_delete_window(window: u32) -> Result<(), Box<dyn std::error::Error>> {
        let (connection, _) = x11rb::connect(None)?;
        let protocols = connection
            .intern_atom(false, b"WM_PROTOCOLS")?
            .reply()?
            .atom;
        let delete_window = connection
            .intern_atom(false, b"WM_DELETE_WINDOW")?
            .reply()?
            .atom;
        let message = ClientMessageEvent::new(32, window, protocols, [delete_window, 0, 0, 0, 0]);
        connection.send_event(false, window, EventMask::NO_EVENT, message)?;
        // A round trip: the server has handled the send before this
        // connection closes (closing first can drop the request).
        connection.get_input_focus()?.reply()?;
        Ok(())
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.renderer.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("vilan-rt-gfx")
            .with_inner_size(OPENING_SIZE);
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => return self.fail(event_loop, format!("no window: {error}")),
        };
        match WindowRenderer::new(event_loop, window.clone()) {
            Ok(renderer) => {
                let adapter = renderer.context().adapter();
                println!(
                    "window adapter={} backend={} type={} scale={}",
                    adapter.name,
                    adapter.backend,
                    adapter.device_type,
                    window.scale_factor()
                );
                self.renderer = Some(renderer);
                window.request_redraw();
            }
            Err(error) => self.fail(event_loop, format!("no renderer: {error}")),
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::IdlePeriodOver => {
                if let Some(Step::Idle { frames_before }) = self.self_test {
                    println!(
                        "idle frames={} over_ms={}",
                        self.frames - frames_before,
                        IDLE_PERIOD.as_millis()
                    );
                    self.self_test = Some(Step::Closing);
                    self.close_through_the_window_system(event_loop);
                }
            }
            UserEvent::WatchdogFired => {
                self.fail(
                    event_loop,
                    format!("the self-test stalled for {} s", WATCHDOG.as_secs()),
                );
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                println!("close-requested frames={}", self.frames);
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size);
                    renderer.window().request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // The new physical size follows as `Resized`; text needs no
                // reshaping (layouts are in dp), only a frame at the new scale.
                println!("scale-factor-changed scale={scale_factor}");
                if let Some(renderer) = &self.renderer {
                    renderer.window().request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.draw(event_loop),
            _ => {}
        }
    }
}

fn main() {
    let self_test = std::env::args().any(|argument| argument == "--self-test");
    let event_loop = match EventLoop::<UserEvent>::with_user_event().build() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            println!("failure: no event loop: {error}");
            std::process::exit(1);
        }
    };
    // On demand: the loop sleeps until an event arrives.
    event_loop.set_control_flow(ControlFlow::Wait);
    let proxy = event_loop.create_proxy();
    if self_test {
        let watchdog = proxy.clone();
        std::thread::spawn(move || {
            std::thread::sleep(WATCHDOG);
            let _ = watchdog.send_event(UserEvent::WatchdogFired);
        });
    }
    let mut app = App {
        proxy,
        text: TextSystem::new(),
        renderer: None,
        frames: 0,
        started: Instant::now(),
        self_test: self_test.then_some(Step::AwaitFirstFrame),
        failure: None,
    };
    if let Err(error) = event_loop.run_app(&mut app) {
        println!("failure: the event loop: {error}");
        std::process::exit(1);
    }
    if app.failure.is_some() {
        std::process::exit(2);
    }
}
