//! `vilan-rt::process` — `std::process` natively (A173 S1 and F?4).
//!
//! # What this is a twin OF
//!
//! `vilan/std/src/process.vl` binds node's `process` object and the runtime
//! helpers of `transformer.rs::helper_source`: `__stdin_line` (the incremental
//! line reader behind `read_line`/`scan`/`prompt`), `__stdin_bytes` (A173 S2:
//! the byte read behind `process::Keys`, whose decoder is std's own vilan) and
//! `__stdout_write` (`prompt`'s and `io::write`'s unterminated write). `args`,
//! `cwd` and `exit` are one call into Rust's own `std` each; `env` is an
//! intrinsic the emitter writes inline. The key stream's terminal MODE is not
//! here: raw mode is FFI, which this crate forbids, so it is `vilan-rt-term`'s
//! (linked only by a program that reaches `keys()`), and [`at_exit`] is the one
//! seam between the two.
//!
//! # The stdin reader
//!
//! The JS helper is ONE shared pump over `process.stdin`: chunks append to a
//! buffer, waiters queue and are answered in order, and stdin is let go
//! (`pause()` + `unref()`) whenever nobody waits. A LINE waiter takes the
//! bytes to the next line ending; a BYTES waiter (S2) takes everything
//! buffered, so a line read and a key read never race for a byte. This is that
//! pump with the same rules:
//!
//! * `std::io::Stdin` has no non-blocking mode, so a lazily started READER
//!   THREAD does the blocking `read`. It reads ON DEMAND — one chunk per
//!   request on a channel — and sends the chunk back over `std::sync::mpsc`.
//!   The thread is detached: returning from `main` ends the process with it,
//!   blocked in its read or not.
//! * The executor learns of a chunk through an [`IoSource`] that is LIVE only
//!   while a read waits, and whose `poll` is a `try_recv` — the native twin of
//!   the `unref`: an idle stdin never holds the loop open, and a pending read
//!   always does.
//! * A cancelled wait (the ambient signal fired) removes that waiter; a chunk
//!   already requested stays requested, and the line it brings is kept for the
//!   next reader — cancellation abandons the WAIT, never a line.
//! * `\n`, `\r\n` and a lone `\r` each end a line. A `\r` answers at once and
//!   remembers it ([`Stdin::carriage`]), so a `\n` arriving next — in the same
//!   chunk or the next — is the same ending: the split is the same whatever the
//!   chunk boundaries were, which is what lets the native differential compare
//!   the two backends over a piped fixture.
//! * Lines are split on BYTES (`\r` and `\n` never occur inside a UTF-8
//!   sequence) and decoded per line, lossily — node's `setEncoding("utf8")`
//!   decoder also answers U+FFFD for bytes that are not UTF-8.
//!
//! * A bytes read may carry a WAIT (the key decoder's escape timeout on a
//!   terminal): if it runs out first the read answers an EMPTY chunk, and the
//!   bytes that arrive afterwards stay for the next read.
//!
//! The cost named, as the executor names its own: while a read waits, the loop
//! wakes every millisecond to look ([`crate::executor`]'s `IO_POLL_INTERVAL`) —
//! the price of the no-`unsafe`, no-dependency rule (Order 37 R8). Blocking on
//! the channel until the next timer deadline when stdin is the only live work
//! is a refinement for later, not a correctness question.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::io::{Read, Write};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::mpsc;
use std::task::{Context, Poll};

use crate::bytes::Bytes;
use crate::executor::{
    CancelSignal, IoSource, Parked, TimerHandle, TimerWait, raise_cancelled, register_io, timer,
};
use crate::{Str, panic_with, str_new};

// ------------------------------------------------------------ the surface --

/// `process::args()` — `process.argv.slice(2)`: the arguments after the
/// program itself. A native binary's `argv[0]` is the binary, so one is
/// skipped where node skips two (node and the script). An argument that is
/// not UTF-8 is decoded lossily, as node decodes it.
pub fn args() -> Vec<Str> {
    std::env::args_os()
        .skip(1)
        .map(|argument| str_new(&argument.to_string_lossy()))
        .collect()
}

/// `process::cwd()` — `process.cwd()`. node throws when the directory is
/// gone (`ENOENT: process.cwd failed with error no such file or directory`),
/// and a throw nobody catches ends the program with its message: so does this.
pub fn cwd() -> Str {
    match std::env::current_dir() {
        Ok(directory) => str_new(&directory.to_string_lossy()),
        Err(error) => panic_with(&format!("process.cwd failed with error {error}")),
    }
}

/// `process::exit(code)` — `process.exit(code)`: the process ends here, running
/// no destructor and no pending task, exactly as node's does. Stdout is flushed
/// first: Rust's stdout is line-buffered, and an unterminated line held there
/// would be lost where node has already written it.
pub fn exit(code: i32) -> ! {
    run_exit_hooks();
    let _ = std::io::stdout().flush();
    std::process::exit(code)
}

/// The hooks the process runs on its way out — [`exit`], and the end of the
/// emitted `main` whether it returned or failed ([`crate::main_guard`]).
///
/// A173 S2's seam: `vilan-rt-term` registers the terminal's restore here when
/// `keys()` enters raw mode, because the two roads a `Keys`'s `Drop` does not
/// run on — `process::exit`, which runs no destructor (node's own does not
/// either), and a failure that leaves the handle in a task the loop never
/// finishes — still end here. node's twin is the helper's `'exit'` listener.
/// A hook must be idempotent: a `Drop` may have run it already.
static EXIT_HOOKS: std::sync::Mutex<Vec<fn()>> = std::sync::Mutex::new(Vec::new());

/// Registers `hook` to run at process end (once per registration; a hook
/// registered twice runs twice, so a caller registers once).
pub fn at_exit(hook: fn()) {
    EXIT_HOOKS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(hook);
}

/// Runs every [`at_exit`] hook, in registration order.
pub fn run_exit_hooks() {
    let hooks = EXIT_HOOKS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    for hook in hooks {
        hook();
    }
}

/// `__stdout_write(text)` — `process.stdout.write(text)`: no newline, and
/// FLUSHED, so a prompt is on the screen before the read that follows parks
/// (Rust's stdout is a `LineWriter`, which would hold an unterminated line).
/// A failed write is dropped, as node's asynchronous `'error'` on a closed
/// stdout is unobserved by the program.
pub fn stdout_write(text: &str) {
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(text.as_bytes());
    let _ = stdout.flush();
}

/// `__stdin_line(signal)` — the next line of stdin without its ending,
/// `None` once stdin has ended; see the module comment.
pub fn stdin_line(signal: Option<CancelSignal>) -> StdinLine {
    StdinLine {
        read: QueuedRead::new(Want::Line, signal),
    }
}

/// The future [`stdin_line`] returns. Public because emitted code's TYPE
/// inference names it, never its text.
pub struct StdinLine {
    read: QueuedRead,
}

impl Future for StdinLine {
    type Output = Option<Str>;

    fn poll(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Str>> {
        match self.as_mut().get_mut().read.poll() {
            Poll::Ready(Answer::Line(line)) => Poll::Ready(line),
            Poll::Ready(Answer::Bytes(_)) => unreachable!("a line read answered with bytes"),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// `__stdin_bytes(signal, wait_ms)` — everything stdin has buffered, awaiting
/// a chunk when nothing is; `None` once stdin has ended and every byte was
/// taken; an EMPTY chunk when `wait_ms` (unless negative) passes first. The
/// byte read behind `process::Keys` (A173 S2); see the module comment.
pub fn stdin_bytes(signal: Option<CancelSignal>, wait_ms: i32) -> StdinBytes {
    StdinBytes {
        read: QueuedRead::new(Want::Bytes, signal),
        wait_ms,
        deadline: None,
    }
}

/// The future [`stdin_bytes`] returns.
pub struct StdinBytes {
    read: QueuedRead,
    wait_ms: i32,
    /// The wait's timer and its waiter, once the read has had to park.
    deadline: Option<(TimerHandle, TimerWait)>,
}

impl Future for StdinBytes {
    type Output = Option<Bytes>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Bytes>> {
        let this = self.as_mut().get_mut();
        match this.read.poll() {
            Poll::Ready(Answer::Bytes(bytes)) => {
                if let Some((handle, _)) = this.deadline.take() {
                    handle.cancel();
                }
                return Poll::Ready(bytes);
            }
            Poll::Ready(Answer::Line(_)) => unreachable!("a bytes read answered with a line"),
            Poll::Pending => {}
        }
        if this.wait_ms < 0 {
            return Poll::Pending;
        }
        let (_, wait) = this.deadline.get_or_insert_with(|| {
            let handle = timer(this.wait_ms);
            let wait = handle.wait(None);
            (handle, wait)
        });
        match Pin::new(wait).poll(context) {
            // The wait ran out with nothing read: the waiter leaves the queue,
            // and what arrives later is the next read's.
            Poll::Ready(_) => {
                this.deadline = None;
                this.read.withdraw();
                Poll::Ready(Some(Bytes::from_vec(Vec::new())))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// One read's place in the shared queue — the half [`StdinLine`] and
/// [`StdinBytes`] share: the abort check, the queueing, the answer.
struct QueuedRead {
    want: Want,
    signal: Option<CancelSignal>,
    waiter: Option<Rc<Waiter>>,
}

impl QueuedRead {
    fn new(want: Want, signal: Option<CancelSignal>) -> QueuedRead {
        QueuedRead {
            want,
            signal,
            waiter: None,
        }
    }

    fn poll(&mut self) -> Poll<Answer> {
        // An answer already left wins over an abort that came after it: on the
        // JS side the promise resolved first, and the abort listener was gone.
        let answered = self
            .waiter
            .as_ref()
            .and_then(|waiter| waiter.answer.borrow_mut().take());
        if let Some(answer) = answered {
            self.waiter = None;
            return Poll::Ready(answer);
        }
        // The abort is checked BEFORE the waiter queues, as the promise
        // constructor's first line does, and on every later poll — a cancel
        // wakes this task through the signal's park below.
        if self.signal.as_ref().is_some_and(CancelSignal::aborted) {
            self.withdraw();
            raise_cancelled();
        }
        let waiter = match &self.waiter {
            Some(waiter) => Rc::clone(waiter),
            None => {
                // The prompt, or anything else written unterminated, is out
                // before the read parks.
                let _ = std::io::stdout().flush();
                let waiter = Rc::new(Waiter {
                    want: self.want,
                    task: RefCell::new(None),
                    answer: RefCell::new(None),
                });
                STDIN.with(|stdin| stdin.borrow_mut().waiters.push_back(Rc::clone(&waiter)));
                self.waiter = Some(Rc::clone(&waiter));
                waiter
            }
        };
        *waiter.task.borrow_mut() = Parked::current();
        pump();
        let answered = waiter.answer.borrow_mut().take();
        if let Some(answer) = answered {
            self.waiter = None;
            return Poll::Ready(answer);
        }
        if let Some(signal) = &self.signal {
            signal.park();
        }
        Poll::Pending
    }

    /// Gives up this read's place in the queue, if it holds one.
    fn withdraw(&mut self) {
        if let Some(waiter) = self.waiter.take() {
            withdraw(&waiter);
        }
    }
}

impl Drop for QueuedRead {
    /// A read dropped while it waits (its task unwound) gives up its place, so
    /// the next line goes to the next reader rather than to nobody.
    fn drop(&mut self) {
        self.withdraw();
    }
}

/// What a waiter takes from the buffer.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Want {
    /// The bytes to the next line ending, decoded.
    Line,
    /// Everything buffered, as bytes.
    Bytes,
}

/// What the pump leaves a waiter.
enum Answer {
    Line(Option<Str>),
    Bytes(Option<Bytes>),
}

/// One read's place in the queue: what it wants, the task to wake, and the
/// answer the pump leaves for it.
struct Waiter {
    want: Want,
    task: RefCell<Option<Parked>>,
    answer: RefCell<Option<Answer>>,
}

/// Takes `waiter` out of the queue, if it is still there.
fn withdraw(waiter: &Rc<Waiter>) {
    STDIN.with(|stdin| {
        stdin
            .borrow_mut()
            .waiters
            .retain(|queued| !Rc::ptr_eq(queued, waiter));
    });
}

// -------------------------------------------------------------- the pump --

/// The shared reader state — `__vilan_stdin` on the JS side.
struct Stdin {
    /// The reader thread's two channels, once a read has started it.
    reader: Option<Reader>,
    /// Bytes read and not yet handed out.
    buffer: Vec<u8>,
    /// Stdin ended (or failed): what is buffered is all there will be.
    ended: bool,
    /// The last line ended at a `\r`: a `\n` arriving next belongs to it.
    carriage: bool,
    /// A chunk request is in flight on the reader thread.
    requested: bool,
    /// The [`StdinSource`] is in the executor's I/O registry.
    registered: bool,
    /// The reads waiting, oldest first.
    waiters: VecDeque<Rc<Waiter>>,
}

struct Reader {
    requests: mpsc::Sender<()>,
    chunks: mpsc::Receiver<Option<Vec<u8>>>,
}

thread_local! {
    static STDIN: RefCell<Stdin> = const {
        RefCell::new(Stdin {
            reader: None,
            buffer: Vec::new(),
            ended: false,
            carriage: false,
            requested: false,
            registered: false,
            waiters: VecDeque::new(),
        })
    };
}

/// `__stdin_pump`: answers waiters from the buffer while it holds what they
/// want, and otherwise asks the reader thread for more (once) and makes sure
/// the executor is watching for it.
fn pump() {
    let mut answered: Vec<(Rc<Waiter>, Answer)> = Vec::new();
    let needs_source = STDIN.with(|stdin| {
        let mut stdin = stdin.borrow_mut();
        let stdin = &mut *stdin;
        while !stdin.waiters.is_empty() {
            if stdin.carriage && !stdin.buffer.is_empty() {
                if stdin.buffer[0] == b'\n' {
                    stdin.buffer.remove(0);
                }
                stdin.carriage = false;
                continue;
            }
            let want = stdin.waiters.front().expect("the loop saw a waiter").want;
            let answer = match want {
                Want::Bytes if !stdin.buffer.is_empty() => {
                    Answer::Bytes(Some(Bytes::from_vec(std::mem::take(&mut stdin.buffer))))
                }
                Want::Bytes if stdin.ended => Answer::Bytes(None),
                Want::Bytes => break,
                Want::Line => {
                    if let Some(end) = stdin
                        .buffer
                        .iter()
                        .position(|byte| *byte == b'\n' || *byte == b'\r')
                    {
                        stdin.carriage = stdin.buffer[end] == b'\r';
                        let line: Vec<u8> = stdin.buffer.drain(..=end).take(end).collect();
                        Answer::Line(Some(str_new(&String::from_utf8_lossy(&line))))
                    } else if stdin.ended {
                        let rest = std::mem::take(&mut stdin.buffer);
                        Answer::Line(
                            (!rest.is_empty()).then(|| str_new(&String::from_utf8_lossy(&rest))),
                        )
                    } else {
                        break;
                    }
                }
            };
            let waiter = stdin.waiters.pop_front().expect("the loop saw a waiter");
            answered.push((waiter, answer));
        }
        if stdin.waiters.is_empty() || stdin.ended {
            return false;
        }
        if !stdin.requested {
            let reader = stdin.reader.get_or_insert_with(start_reader);
            // A reader thread that is gone has ended stdin; its channel's
            // disconnect is what the source's `try_recv` then reports.
            let _ = reader.requests.send(());
            stdin.requested = true;
        }
        !std::mem::replace(&mut stdin.registered, true)
    });
    // Woken OUTSIDE the borrow: a wake only queues a task id, but the waiter's
    // own future reads the answer when it is next polled, never here.
    for (waiter, answer) in answered {
        *waiter.answer.borrow_mut() = Some(answer);
        if let Some(task) = waiter.task.borrow().as_ref() {
            task.wake();
        }
    }
    if needs_source {
        register_io(Rc::new(StdinSource));
    }
}

/// The reader thread, started by the first read that finds the buffer short.
fn start_reader() -> Reader {
    let (requests, requested) = mpsc::channel::<()>();
    let (sent, chunks) = mpsc::channel::<Option<Vec<u8>>>();
    let spawned = std::thread::Builder::new()
        .name("vilan-stdin".to_string())
        .spawn(move || {
            let mut stdin = std::io::stdin();
            let mut buffer = vec![0u8; 8192];
            while requested.recv().is_ok() {
                let chunk = loop {
                    match stdin.read(&mut buffer) {
                        Ok(0) => break None,
                        Ok(count) => break Some(buffer[..count].to_vec()),
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                        // A read that fails ends stdin, as node's `'error'`
                        // does in the helper.
                        Err(_) => break None,
                    }
                };
                let ended = chunk.is_none();
                if sent.send(chunk).is_err() || ended {
                    break;
                }
            }
        });
    if let Err(error) = spawned {
        panic_with(&format!("could not start the stdin reader: {error}"));
    }
    Reader { requests, chunks }
}

/// The executor's view of stdin: live while a read waits.
struct StdinSource;

impl IoSource for StdinSource {
    fn poll(&self) -> bool {
        let received = STDIN.with(|stdin| {
            let mut stdin = stdin.borrow_mut();
            if !stdin.requested {
                return false;
            }
            let Some(reader) = &stdin.reader else {
                return false;
            };
            let chunk = match reader.chunks.try_recv() {
                Ok(chunk) => chunk,
                Err(mpsc::TryRecvError::Empty) => return false,
                Err(mpsc::TryRecvError::Disconnected) => None,
            };
            stdin.requested = false;
            match chunk {
                Some(bytes) => stdin.buffer.extend_from_slice(&bytes),
                None => stdin.ended = true,
            }
            true
        });
        if received {
            pump();
        }
        received
    }

    fn is_live(&self) -> bool {
        STDIN.with(|stdin| {
            let mut stdin = stdin.borrow_mut();
            let live = !stdin.waiters.is_empty();
            if !live {
                // The registry drops a source that answers `false`; the next
                // read that has to wait registers a fresh one.
                stdin.registered = false;
            }
            live
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds `chunks` through the pump's splitting rule exactly as the reader
    /// would deliver them, and answers every line read until the end.
    fn lines_of(chunks: &[&[u8]]) -> Vec<Option<String>> {
        STDIN.with(|stdin| {
            let mut stdin = stdin.borrow_mut();
            stdin.buffer.clear();
            stdin.ended = false;
            stdin.carriage = false;
            stdin.requested = true;
            stdin.registered = true;
        });
        let mut answers = Vec::new();
        let mut pending: VecDeque<&[u8]> = chunks.iter().copied().collect();
        loop {
            let waiter = Rc::new(Waiter {
                want: Want::Line,
                task: RefCell::new(None),
                answer: RefCell::new(None),
            });
            STDIN.with(|stdin| stdin.borrow_mut().waiters.push_back(Rc::clone(&waiter)));
            loop {
                pump();
                if waiter.answer.borrow().is_some() {
                    break;
                }
                STDIN.with(|stdin| {
                    let mut stdin = stdin.borrow_mut();
                    match pending.pop_front() {
                        Some(chunk) => stdin.buffer.extend_from_slice(chunk),
                        None => stdin.ended = true,
                    }
                });
            }
            let Some(Answer::Line(answer)) = waiter.answer.borrow_mut().take() else {
                panic!("a line waiter is answered with a line");
            };
            let done = answer.is_none();
            answers.push(answer.map(|line| line.to_string()));
            if done {
                break;
            }
        }
        answers
    }

    fn some(lines: &[&str]) -> Vec<Option<String>> {
        lines
            .iter()
            .map(|line| Some(line.to_string()))
            .chain(std::iter::once(None))
            .collect()
    }

    #[test]
    fn the_three_line_endings_each_end_one_line() {
        assert_eq!(
            lines_of(&[b"lf\ncrlf\r\ncr\rlast"]),
            some(&["lf", "crlf", "cr", "last"])
        );
    }

    #[test]
    fn a_blank_line_is_a_line_and_the_end_is_none() {
        assert_eq!(lines_of(&[b"a\n\nb\n"]), some(&["a", "", "b"]));
        assert_eq!(lines_of(&[]), some(&[]));
    }

    #[test]
    fn a_crlf_split_across_two_chunks_is_one_ending() {
        assert_eq!(
            lines_of(&[b"ann\r", b"\nbob\r", b"\r\n"]),
            some(&["ann", "bob", ""])
        );
    }

    /// A173 S2: a BYTES waiter takes everything buffered at once, a line
    /// waiter queued behind it waits for the next bytes, and at the end a
    /// bytes waiter is `None`. A `\n` that ends a `\r`-ended line belongs to
    /// that line, whoever reads next.
    #[test]
    fn a_bytes_waiter_takes_everything_buffered_and_none_at_the_end() {
        STDIN.with(|stdin| {
            let mut stdin = stdin.borrow_mut();
            stdin.buffer = b"one\r\nab".to_vec();
            stdin.ended = false;
            stdin.carriage = false;
            stdin.requested = true;
            stdin.registered = true;
            stdin.waiters.clear();
        });
        let waiter = |want: Want| {
            let waiter = Rc::new(Waiter {
                want,
                task: RefCell::new(None),
                answer: RefCell::new(None),
            });
            STDIN.with(|stdin| stdin.borrow_mut().waiters.push_back(Rc::clone(&waiter)));
            waiter
        };
        let line = waiter(Want::Line);
        pump();
        assert!(
            matches!(line.answer.borrow_mut().take(), Some(Answer::Line(Some(text))) if &*text == "one")
        );
        let bytes = waiter(Want::Bytes);
        let after = waiter(Want::Line);
        pump();
        assert!(
            matches!(bytes.answer.borrow_mut().take(), Some(Answer::Bytes(Some(chunk))) if chunk.to_vec() == b"ab"),
            "the `\\n` of the `\\r\\n` is the line's, not the keys'"
        );
        assert!(
            after.answer.borrow().is_none(),
            "the line waits for more bytes"
        );
        STDIN.with(|stdin| stdin.borrow_mut().ended = true);
        let ended = waiter(Want::Bytes);
        pump();
        assert!(matches!(
            after.answer.borrow_mut().take(),
            Some(Answer::Line(None))
        ));
        assert!(matches!(
            ended.answer.borrow_mut().take(),
            Some(Answer::Bytes(None))
        ));
    }

    #[test]
    fn a_utf8_scalar_split_across_chunks_decodes_whole() {
        let bytes = "é\n".as_bytes();
        assert_eq!(lines_of(&[&bytes[..1], &bytes[1..]]), some(&["é"]));
    }
}
