//! `vilan-rt-webview` — `std::window` on the native backend: a desktop window
//! showing a web page in the operating system's webview (F1 M2;
//! native-objective.md D2, D2a, D4, D8).
//!
//! # The two threads
//!
//! A platform's window system wants its event loop on the process's MAIN
//! thread — macOS refuses any other, GTK is bound to the thread that started
//! it, and Windows delivers a window's messages to the thread that made it —
//! and that loop never returns while there are windows to serve. The vilan
//! program has an event loop of its own, the runtime's executor
//! (`vilan_rt::executor`), which runs its servers, timers and tasks and blocks
//! in the readiness wait between turns. Neither loop can be pumped from inside
//! the other without polling one on a tick, so each gets a thread:
//!
//! - [`run`] — the emitted `main` of a program that reaches `std::window` is
//!   one call to it — starts the platform loop on the main thread, and runs
//!   the PROGRAM (its `main_guard`, its executor, its in-process server) on a
//!   second thread with an 8 MiB stack, the size a Linux main thread has.
//! - The program asks the main thread for a window through the platform
//!   loop's proxy ([`open`]) and waits for the answer — one round trip, once
//!   per window, while the window is made; the platform loop never waits on
//!   the program, so the two cannot deadlock.
//! - An open window is an `IoSource` of the program's loop ([`WindowSource`]):
//!   it keeps the loop alive while the window is open (a program with no
//!   server stays up until its window closes) and asks for no socket and no
//!   deadline, so the loop sleeps in its readiness wait. When the window
//!   closes, the main thread marks it closed and rings the loop's bell
//!   (`LoopWaker`): no tick, in either direction.
//! - Closing the LAST window is the program's termination request
//!   (`vilan_rt::executor::request_termination`), the graceful stop SIGTERM
//!   makes: servers drain, the loop ends, `main` returns — and when the
//!   program's thread ends, the platform loop exits the process with code 0.
//!   A program that fails exits from its own thread with its own code, as it
//!   does without a window (`vilan_rt::main_guard`).
//!
//! When the window system cannot start (no display, no session bus), the
//! program runs on the main thread with no platform loop and every [`open`]
//! answers the reason as an `Err` — the server still serves, and the program
//! decides.
//!
//! # Why a crate of its own
//!
//! `vilan-rt` takes no crates.io dependencies; this crate is `wry` and `tao`.
//! It is outside the compiler's workspace, with its own lockfile and its own
//! CI gates — the manifest says why.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};

use vilan_rt::executor::{Interest, IoSource, LoopWaker};
use vilan_rt::{Str, str_new};

mod shell;

pub use shell::run;

/// The stack the program's thread is spawned with: what a Linux main thread
/// has by default, so a program recurses as deep with a window as without.
pub const PROGRAM_STACK: usize = 8 << 20;

/// What the program's thread asks of the main thread.
pub(crate) enum Request {
    /// Make a window showing `url`, and answer its id or why not.
    Open {
        url: String,
        title: String,
        width: u32,
        height: u32,
        window: Arc<WindowState>,
        answer: mpsc::Sender<Result<u32, String>>,
    },
    /// Close the window with this id (a no-op once it is closed).
    Close { id: u32 },
    /// The program's thread has ended, with this exit code.
    ProgramEnded { code: i32 },
}

/// Where a window stands, shared between the program's thread (which reads
/// it from the window's [`WindowSource`]) and the main thread (which closes
/// it).
pub(crate) struct WindowState {
    closed: AtomicBool,
    /// The program loop's waker, taken on the program's thread when the window
    /// was asked for.
    waker: LoopWaker,
}

impl WindowState {
    pub(crate) fn new(waker: LoopWaker) -> WindowState {
        WindowState {
            closed: AtomicBool::new(false),
            waker,
        }
    }

    /// Marks the window closed and wakes the program's loop, which then finds
    /// its source dead and drops it.
    pub(crate) fn mark_closed(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.waker.wake();
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
}

/// How the program's thread reaches the main thread: a send that answers
/// `false` once the platform loop is gone.
pub(crate) struct Link {
    send: Mutex<Box<dyn FnMut(Request) -> bool + Send>>,
}

impl Link {
    pub(crate) fn new(send: impl FnMut(Request) -> bool + Send + 'static) -> Link {
        Link {
            send: Mutex::new(Box::new(send)),
        }
    }

    fn send(&self, request: Request) -> bool {
        let mut send = self
            .send
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (*send)(request)
    }
}

/// The process's link to its platform loop, set once by [`run`]: `Ok` when
/// the loop started, `Err` with the reason when it could not.
static SHELL: OnceLock<Result<Link, String>> = OnceLock::new();

pub(crate) fn install_link(link: Result<Link, String>) {
    let _ = SHELL.set(link);
}

/// An open window, as the program's loop sees it: live until the window
/// closes, waiting on nothing the loop could watch — the main thread rings
/// the loop's bell when the window closes.
pub(crate) struct WindowSource {
    window: Arc<WindowState>,
}

impl IoSource for WindowSource {
    fn poll(&self) -> bool {
        false
    }

    /// Complete with nothing in it: the only event is the window closing, and
    /// it arrives from another thread, through the waker.
    fn interest(&self, _interest: &mut Interest) -> bool {
        true
    }

    fn is_live(&self) -> bool {
        !self.window.is_closed()
    }
}

/// `__window_open(url, title, width, height)` — `std::window`'s
/// `WindowBuilder::open`: a window `width` x `height` logical pixels titled
/// `title`, showing `url`. `Ok` with the window's id once it exists (its page
/// loads after); `Err` with the reason when there is no window system or the
/// window or its webview could not be made.
pub fn open(url: &Str, title: &Str, width: u32, height: u32) -> Result<u32, Str> {
    let Some(shell) = SHELL.get() else {
        return Err(str_new(
            "cannot open a window: this program's `main` did not start the desktop shell",
        ));
    };
    match shell {
        Ok(link) => {
            open_through(link, url, title, width, height).map_err(|reason| str_new(&reason))
        }
        Err(reason) => Err(str_new(reason)),
    }
}

/// [`open`] through a given link — the half the unit tests drive with a
/// stand-in for the platform loop.
pub(crate) fn open_through(
    link: &Link,
    url: &str,
    title: &str,
    width: u32,
    height: u32,
) -> Result<u32, String> {
    if width == 0 || height == 0 {
        return Err(format!(
            "cannot open a window of {width} x {height}: both sides must be at least 1"
        ));
    }
    let window = Arc::new(WindowState::new(vilan_rt::executor::loop_waker()));
    let (answer, answered) = mpsc::channel();
    let sent = link.send(Request::Open {
        url: url.to_string(),
        title: title.to_string(),
        width,
        height,
        window: Arc::clone(&window),
        answer,
    });
    if !sent {
        return Err(SHELL_GONE.to_string());
    }
    let id = answered.recv().map_err(|_| SHELL_GONE.to_string())??;
    vilan_rt::executor::register_io(Rc::new(WindowSource { window }));
    Ok(id)
}

const SHELL_GONE: &str = "cannot open a window: the desktop shell has stopped";

/// `__window_close(id)` — `std::window`'s `Window::close`. Closing a window
/// that is already closed does nothing; closing the last one stops the
/// program as closing it by hand does.
pub fn close(id: u32) {
    if let Some(Ok(link)) = SHELL.get() {
        link.send(Request::Close { id });
    }
}

/// The main thread's book of open windows, by the id the program holds —
/// generic over what a window IS so the bookkeeping is tested without a
/// window system.
pub(crate) struct Windows<W> {
    next_id: u32,
    open: HashMap<u32, (W, Arc<WindowState>)>,
    /// What closing the last window does: [`stop_the_program`] in a running
    /// program, a recorder in the unit tests.
    on_last_closed: fn(),
}

/// The program's graceful stop, as closing its last window asks for it —
/// unless one is already under way (a second request would end the process
/// at once, with exit code 1).
pub(crate) fn stop_the_program() {
    if !vilan_rt::executor::termination_requested() {
        vilan_rt::executor::request_termination();
    }
}

impl<W> Windows<W> {
    pub(crate) fn new(on_last_closed: fn()) -> Windows<W> {
        Windows {
            next_id: 1,
            open: HashMap::new(),
            on_last_closed,
        }
    }

    /// Records a window made for a program's request; answers its id.
    pub(crate) fn insert(&mut self, window: W, state: Arc<WindowState>) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.open.insert(id, (window, state));
        id
    }

    /// The id of the open window `matches` picks out, if any.
    pub(crate) fn find(&self, matches: impl Fn(&W) -> bool) -> Option<u32> {
        self.open
            .iter()
            .find(|(_, (window, _))| matches(window))
            .map(|(id, _)| *id)
    }

    /// Closes window `id`: drops it (which takes it off the screen), tells the
    /// program, and — when it was the last — stops the program. Answers
    /// whether `id` was open.
    pub(crate) fn close(&mut self, id: u32) -> bool {
        let Some((window, state)) = self.open.remove(&id) else {
            return false;
        };
        drop(window);
        state.mark_closed();
        if self.open.is_empty() {
            (self.on_last_closed)();
        }
        true
    }
}

#[cfg(test)]
mod tests;
