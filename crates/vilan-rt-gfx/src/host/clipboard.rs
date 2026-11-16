//! The system clipboard's TEXT (F173, RULED 2026-11-12: `arboard`, pinned,
//! behind a two-function `std::gfx` surface so the dependency stays
//! replaceable): `__gfx_clipboard_text` and `__gfx_set_clipboard_text`. This
//! file is the only one that names `arboard`, and only in
//! [`system_clipboard`]: everything else speaks [`ClipboardBackend`], which a
//! test fake implements too.
//!
//! # One clipboard, on a thread of its own
//!
//! The process has ONE clipboard object, made lazily — on the first read or
//! write, never before — on a thread of its own ([`Worker`]), which takes the
//! program's requests in the order they were made. A write is handed over and
//! returns at once; a read answers through a promise, delivered in a later
//! turn by an [`IoSource`] the worker's answer wakes the loop for. So the
//! window's thread never waits on the clipboard: on X11 a read asks the
//! selection's owner (another program) for the text, and an owner that is slow
//! or stuck would otherwise freeze the window. The wait is bounded by
//! `arboard`: an X11 read with no answer from the owner gives up after four
//! seconds (its `LONG_TIMEOUT_DUR`), and the read answers `None`. Requests
//! keep their order, so a read made after a write answers what was written
//! (or what another program wrote since).
//!
//! # X11, and F169
//!
//! `arboard`'s X11 half opens its OWN connections to the X server (`x11rb`'s
//! pure-Rust one: one for the thread that serves other programs' pastes while
//! this program owns the clipboard, one per read) and never touches
//! `winit`'s. So it is not a second reader of the connection the window and
//! its GPU surface share, which is what F169's wakes (`shell.rs`'s module doc)
//! are about. On Wayland, `winit` speaks Wayland and the clipboard is X11's
//! through XWayland: Wayland's data-control protocol (`arboard`'s
//! `wayland-data-control` feature) is left out, because GNOME offers no
//! data-control protocol at all, because a Wayland read waits on a pipe the
//! owner may never close (no bound), and for its weight (`wl-clipboard-rs`
//! and its dependencies). A Wayland session with no XWayland has no clipboard
//! here: reads answer `None`, writes do nothing.
//!
//! # The end
//!
//! On X11 (and Wayland) the text this program copied is SERVED by this
//! program, for as long as it lives. The clipboard object lives as long as the
//! window system's loop: [`shut_down`] drops it after the loop has ended
//! (`shell.rs`'s `run`, and the owned loop's end where there is no window
//! system), and `arboard` then offers the text to the desktop's clipboard
//! manager, if one runs (it waits up to 100 ms for it), so a copy can outlive
//! the program; without a manager it ends with the program, as every X11
//! program's copy does.
//!
//! # No display
//!
//! Where there is no clipboard to reach (no display: a `--check` run, a CI
//! machine, an SSH session), making the clipboard object fails, once, and
//! from then on every read answers `None` and every write does nothing:
//! nothing panics and nothing waits.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc;
use std::thread::JoinHandle;

use vilan_rt::executor::{Interest, IoSource, Task, loop_waker, promise, register_io};
use vilan_rt::{Str, str_new};

/// A clipboard's text, read and written: the system's ([`system_clipboard`])
/// or a test's.
pub(crate) trait ClipboardBackend: Send {
    /// The clipboard's text: `Ok(None)` when it holds none (or holds only
    /// something that is not text), `Err` when it could not be read.
    fn text(&mut self) -> Result<Option<String>, String>;

    /// Makes `text` the clipboard's.
    fn set_text(&mut self, text: String) -> Result<(), String>;
}

/// What makes the process's clipboard object, on the worker's thread.
type Make = Box<dyn FnOnce() -> Result<Box<dyn ClipboardBackend>, String> + Send>;

/// The system's clipboard, through `arboard`: text only (no images, no HTML).
fn system_clipboard() -> Result<Box<dyn ClipboardBackend>, String> {
    struct System(arboard::Clipboard);
    impl ClipboardBackend for System {
        fn text(&mut self) -> Result<Option<String>, String> {
            match self.0.get_text() {
                Ok(text) => Ok(Some(text)),
                Err(arboard::Error::ContentNotAvailable) => Ok(None),
                Err(error) => Err(error.to_string()),
            }
        }

        fn set_text(&mut self, text: String) -> Result<(), String> {
            self.0.set_text(text).map_err(|error| error.to_string())
        }
    }
    arboard::Clipboard::new()
        .map(|clipboard| Box::new(System(clipboard)) as Box<dyn ClipboardBackend>)
        .map_err(|error| error.to_string())
}

enum Request {
    Read(Box<dyn FnOnce(Option<String>) + Send>),
    Write(String),
}

/// The thread that owns the process's clipboard object and takes its
/// requests in order. Dropping it ends the thread once the requests already
/// made are done, and drops the clipboard object ON that thread (`arboard`'s
/// hand-over to a clipboard manager happens there), before the drop returns.
pub(crate) struct Worker {
    requests: Option<mpsc::Sender<Request>>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    /// Starts the thread; `make` makes the clipboard object on it, when the
    /// first request arrives.
    pub(crate) fn start(make: Make) -> Worker {
        let (requests, received) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("vilan clipboard".to_string())
            .spawn(move || serve(make, &received))
            // A thread that cannot start: no clipboard, as with no display.
            .ok();
        Worker {
            requests: thread.is_some().then_some(requests),
            thread,
        }
    }

    /// Reads the clipboard's text, after every request made before; `answer`
    /// is called on the worker's thread, with `None` when there is no text or
    /// no clipboard.
    pub(crate) fn read(&self, answer: impl FnOnce(Option<String>) + Send + 'static) {
        let request = Request::Read(Box::new(answer));
        let refused = match &self.requests {
            Some(requests) => requests.send(request).err().map(|refused| refused.0),
            None => Some(request),
        };
        if let Some(Request::Read(answer)) = refused {
            answer(None);
        }
    }

    /// Makes `text` the clipboard's, after every request made before.
    pub(crate) fn write(&self, text: String) {
        if let Some(requests) = &self.requests {
            let _ = requests.send(Request::Write(text));
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // The channel's end is the thread's: it finishes what was asked and
        // returns.
        drop(self.requests.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The worker's loop: the clipboard object made at the first request (or not
/// at all), then every request in order until the channel ends.
fn serve(make: Make, received: &mpsc::Receiver<Request>) {
    let Ok(first) = received.recv() else {
        return;
    };
    let mut clipboard = make().ok();
    let mut next = Some(first);
    while let Some(request) = next {
        match request {
            Request::Read(answer) => answer(
                clipboard
                    .as_mut()
                    .and_then(|clipboard| clipboard.text().ok().flatten()),
            ),
            Request::Write(text) => {
                if let Some(clipboard) = clipboard.as_mut() {
                    // A write that failed leaves the clipboard as it was; the
                    // next read says what it holds.
                    let _ = clipboard.set_text(text);
                }
            }
        }
        next = received.recv().ok();
    }
    drop(clipboard);
}

thread_local! {
    /// The process's clipboard worker, started at the first request.
    static WORKER: RefCell<Option<Worker>> = const { RefCell::new(None) };
}

fn with_worker(body: impl FnOnce(&Worker)) {
    WORKER.with(|worker| {
        let mut worker = worker.borrow_mut();
        body(worker.get_or_insert_with(|| Worker::start(Box::new(system_clipboard))));
    });
}

/// Puts a test's clipboard in the system's place, for this thread.
#[cfg(test)]
pub(crate) fn use_for_tests(make: Make) {
    WORKER.with(|worker| *worker.borrow_mut() = Some(Worker::start(make)));
}

/// A clipboard held in the program's memory: what a program that tests
/// itself with no display puts in the system's place
/// (`__gfx_use_memory_clipboard`).
struct Memory(Option<String>);

impl ClipboardBackend for Memory {
    fn text(&mut self) -> Result<Option<String>, String> {
        Ok(self.0.clone())
    }

    fn set_text(&mut self, text: String) -> Result<(), String> {
        self.0 = Some(text);
        Ok(())
    }
}

/// `__gfx_use_memory_clipboard()` — `std::gfx::use_memory_clipboard`: from
/// now on the program's clipboard is an empty one in its own memory, behind
/// the same worker, so reads and writes keep their order and a read still
/// answers in a later turn; the system's clipboard is never touched again.
/// For a program that tests its own copy and paste with no display (the kit
/// examples' `--check` listings), where the system's would answer `None`.
pub fn use_memory() {
    let make: Make = Box::new(|| Ok(Box::new(Memory(None)) as Box<dyn ClipboardBackend>));
    let old = WORKER.with(|worker| worker.borrow_mut().replace(Worker::start(make)));
    // Its requests are done when it drops (`Worker`'s `Drop`).
    drop(old);
}

/// `__gfx_clipboard_text()` — `std::gfx::clipboard_text`: the clipboard's
/// text, or `None` (no text, no clipboard, or no answer from its owner in
/// `arboard`'s four seconds). Async: the answer reaches the program in a
/// later turn, and the window never waits for it.
pub fn text() -> Task<Option<Str>> {
    promise(Rc::new(|resolve: Rc<dyn Fn(Option<Str>)>| {
        let (sender, received) = mpsc::channel();
        register_io(Rc::new(ReadAnswer {
            received,
            resolve,
            done: Cell::new(false),
        }));
        let waker = loop_waker();
        with_worker(|worker| {
            worker.read(move |text| {
                // The read's source is gone only when the program is.
                let _ = sender.send(text);
                waker.wake();
            });
        });
    }))
}

/// `__gfx_set_clipboard_text(text)` — `std::gfx::set_clipboard_text`: `text`
/// becomes the clipboard's, for this program to serve to others (X11,
/// Wayland) or handed to the system (Windows, macOS). Returns at once;
/// nothing happens where there is no clipboard.
pub fn set_text(text: &Str) {
    with_worker(|worker| worker.write(text.to_string()));
}

/// Drops the process's clipboard object (and its thread), after the window
/// system's loop has ended: the module's "The end".
pub(crate) fn shut_down() {
    let worker = WORKER.with(|worker| worker.borrow_mut().take());
    drop(worker);
}

/// [`shut_down`] when it is dropped: held by the loop functions
/// (`run_pending`, `block_on`) across the loop, so the clipboard goes when
/// the loop does, a failure raised out of it included.
pub(crate) struct ShutDownAfter;

impl Drop for ShutDownAfter {
    fn drop(&mut self) {
        shut_down();
    }
}

/// One read's answer, on its way from the worker's thread: the source that
/// resolves the read's promise in a turn, live until it has.
struct ReadAnswer {
    received: mpsc::Receiver<Option<String>>,
    resolve: Rc<dyn Fn(Option<Str>)>,
    done: Cell<bool>,
}

impl IoSource for ReadAnswer {
    fn poll(&self) -> bool {
        if self.done.get() {
            return false;
        }
        match self.received.try_recv() {
            Ok(text) => {
                self.done.set(true);
                (self.resolve)(text.as_deref().map(str_new));
                true
            }
            Err(mpsc::TryRecvError::Empty) => false,
            // The worker is gone without answering: no text.
            Err(mpsc::TryRecvError::Disconnected) => {
                self.done.set(true);
                (self.resolve)(None);
                true
            }
        }
    }

    /// Nothing to watch: the worker wakes the loop when the answer is in.
    fn interest(&self, _interest: &mut Interest) -> bool {
        true
    }

    fn is_live(&self) -> bool {
        !self.done.get()
    }
}

#[cfg(test)]
mod tests;
