//! The main thread's half: the platform event loop (`tao`) and the windows
//! and webviews (`wry`) it serves. See the crate header for the arrangement.

use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopWindowTarget};
use tao::window::{Window, WindowBuilder};
use wry::{WebView, WebViewBuilder};

use crate::{Link, PROGRAM_STACK, Request, Windows, install_link, stop_the_program};

/// Runs `program` — the emitted body of the vilan program's `main`, its
/// `main_guard` included — beside the platform's event loop, and never
/// returns while the loop runs: the process ends when the program's thread
/// does (exit code 0, or the program's own code from its own thread), or
/// when the program fails.
///
/// With no window system (the loop could not start), `program` runs right
/// here on the calling thread instead, and every `open` answers why.
pub fn run(program: fn()) {
    let event_loop = match start_event_loop() {
        Ok(event_loop) => event_loop,
        Err(reason) => {
            install_link(Err(format!(
                "cannot open a window: the platform's window system did not start ({reason})"
            )));
            program();
            return;
        }
    };
    let proxy = event_loop.create_proxy();
    install_link(Ok(Link::new(move |request| {
        proxy.send_event(request).is_ok()
    })));
    let ended = event_loop.create_proxy();
    let spawned = std::thread::Builder::new()
        .name("vilan-program".to_string())
        .stack_size(PROGRAM_STACK)
        .spawn(move || {
            // Sent on the way out whatever the road — a panic escaping the
            // program's own guard included, which is a failure.
            struct Ended(tao::event_loop::EventLoopProxy<Request>);
            impl Drop for Ended {
                fn drop(&mut self) {
                    let code = if std::thread::panicking() { 1 } else { 0 };
                    let _ = self.0.send_event(Request::ProgramEnded { code });
                }
            }
            let _ended = Ended(ended);
            program();
        });
    if let Err(error) = spawned {
        eprintln!("cannot start the program's thread beside the window system: {error}");
        std::process::exit(1);
    }

    let mut windows: Windows<Shown> = Windows::new(stop_the_program);
    event_loop.run(move |event, target, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(Request::Open {
                url,
                title,
                width,
                height,
                window,
                answer,
            }) => {
                let made = Shown::make(target, &url, &title, width, height)
                    .map(|shown| windows.insert(shown, window));
                let _ = answer.send(made);
            }
            Event::UserEvent(Request::Close { id }) => {
                windows.close(id);
            }
            Event::UserEvent(Request::ProgramEnded { code }) => {
                *control_flow = ControlFlow::ExitWithCode(code);
            }
            Event::WindowEvent {
                window_id,
                event: WindowEvent::CloseRequested,
                ..
            } => {
                if let Some(id) = windows.find(|shown| shown.window.id() == window_id) {
                    windows.close(id);
                }
            }
            _ => {}
        }
    });
}

/// The platform event loop, or why it could not start. `tao` PANICS when the
/// window system is absent (GTK's `init` fails with no display), so the start
/// is fenced and the panic's message — not its report on stderr — is the
/// answer.
fn start_event_loop() -> Result<EventLoop<Request>, String> {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let started =
        std::panic::catch_unwind(|| EventLoopBuilder::<Request>::with_user_event().build());
    std::panic::set_hook(previous);
    started.map_err(|payload| {
        payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| {
                payload
                    .downcast_ref::<&str>()
                    .map(|text| (*text).to_string())
            })
            .unwrap_or_else(|| "the window system panicked".to_string())
    })
}

/// A window on the screen and the webview inside it. The webview is declared
/// first so it drops first: a webview outliving its window is undefined on
/// some platforms.
struct Shown {
    _webview: WebView,
    window: Window,
}

impl Shown {
    fn make(
        target: &EventLoopWindowTarget<Request>,
        url: &str,
        title: &str,
        width: u32,
        height: u32,
    ) -> Result<Shown, String> {
        let window = WindowBuilder::new()
            .with_title(title)
            .with_inner_size(LogicalSize::new(f64::from(width), f64::from(height)))
            .build(target)
            .map_err(|error| format!("cannot open a window: {error}"))?;
        let builder = WebViewBuilder::new().with_url(url);
        let webview = build_webview(builder, &window)?;
        Ok(Shown {
            _webview: webview,
            window,
        })
    }
}

/// Linux: inside the GTK box tao gives every window, which serves X11 and
/// Wayland alike (`wry`'s raw-handle path is X11 only).
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
))]
fn build_webview(builder: WebViewBuilder<'_>, window: &Window) -> Result<WebView, String> {
    use tao::platform::unix::WindowExtUnix;
    use wry::WebViewBuilderExtUnix;
    let container = window
        .default_vbox()
        .ok_or("cannot show a page in the window: it has no GTK container")?;
    builder
        .build_gtk(container)
        .map_err(|error| format!("cannot show a page in the window: {error}"))
}

/// macOS and Windows: on the window's native handle.
#[cfg(not(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
)))]
fn build_webview(builder: WebViewBuilder<'_>, window: &Window) -> Result<WebView, String> {
    builder
        .build(window)
        .map_err(|error| format!("cannot show a page in the window: {error}"))
}
