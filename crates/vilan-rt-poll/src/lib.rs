//! `vilan-rt-poll` — the readiness wait behind a native program's event loop
//! (tracker F160).
//!
//! # What it does
//!
//! [`install`] hands `vilan-rt`'s loop ([`vilan_rt::executor::install_readiness`])
//! a wait that blocks until one of the sockets the loop's sources watch is
//! ready, or the nearest deadline passes: `poll(2)` on unix, `WSAPoll` on
//! Windows. Without it the loop polls its sources on a 1 ms tick — an idle
//! server woke a thousand times a second and every request waited up to a
//! millisecond for the next look; with it an idle server sleeps in the kernel
//! and a request is answered as soon as its bytes arrive.
//!
//! The wait is LEVEL-triggered and stateless: each call is handed every socket
//! the turn watches and registers nothing, so it cannot fall out of step with
//! the sources, and the loop's turn — poll every source, one macrotask, drain
//! the microtasks — is exactly what it was. Which sockets, which deadline, and
//! the loopback "bell" another thread rings to end the wait are all
//! `vilan-rt`'s; this crate is the one call `vilan-rt` cannot make without
//! `unsafe`.
//!
//! # Why a crate of its own
//!
//! `vilan-rt` takes no crates.io dependencies and forbids `unsafe` (the
//! manifest says why), and Rust's std has no way to wait on several sockets
//! at once. Called once, from the emitted `main`, before the program's body
//! runs — only by a program that does I/O the loop waits on.

//!
//! # A hosted loop (R8)
//!
//! A program whose loop is driven by a window's event loop
//! ([`vilan_rt::executor::turn`]) cannot block in the wait — its thread is the
//! window's. [`install_hosted`] gives such a loop a helper thread instead
//! ([`Poller`]): only while sockets are watched, it waits on the snapshot the
//! loop's last turn handed it, calls the host's wake, and waits for the next
//! snapshot. A program that watches no socket starts no thread.

#![deny(unsafe_op_in_unsafe_fn)]

use std::io;
use std::net::{Ipv4Addr, UdpSocket};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use vilan_rt::executor::{Pollable, SocketWatcher, Wake, Watch};

/// Installs [`wait`] as the loop's readiness wait. Idempotent: the first
/// install wins.
pub fn install() {
    vilan_rt::executor::install_readiness(wait);
}

/// Makes the CURRENT thread's loop a hosted one (R8) whose sockets are waited
/// on by a [`Poller`]: installs the readiness wait (a hosted turn still LOOKS,
/// without blocking, on its own thread), the host's `wake`
/// ([`vilan_rt::executor::install_host`]) and the poller, which calls the same
/// `wake` when a socket is ready. Call it on the thread that will call
/// [`vilan_rt::executor::turn`], before the program starts.
pub fn install_hosted(wake: Wake) {
    install();
    vilan_rt::executor::install_host(Arc::clone(&wake));
    vilan_rt::executor::install_socket_watcher(Box::new(Poller::new(wake)));
}

/// Where the poller stands — the one piece of state the loop's thread and the
/// poller's thread share, behind [`Shared::state`].
enum Phase {
    /// Nothing to wait on: the poller sleeps on the condition variable.
    Idle,
    /// A snapshot the poller has not picked up yet.
    Armed(Vec<Watch>),
    /// The poller is inside [`wait`] on its snapshot.
    Waiting,
    /// The loop's thread asked the wait to end (and rang the bell) and is
    /// blocked until the poller answers [`Phase::Idle`].
    Interrupting,
    /// The wait completed: what it reported, for the loop's next turn.
    Reported(Vec<Watch>),
    /// The poller's thread is to end.
    Exit,
}

/// How many poller threads are running in the process.
static POLLER_THREADS: AtomicUsize = AtomicUsize::new(0);

/// How many [`Poller`] threads are running in the process right now — what a
/// pin reads to hold that a program watching no socket starts none. A thread
/// counts from just before it is spawned to its last instruction, so a
/// released poller's thread may still count for the moment it takes to leave.
pub fn poller_threads() -> usize {
    POLLER_THREADS.load(Ordering::SeqCst)
}

/// How many waits the loop's thread has interrupted, process-wide.
static INTERRUPTED_WAITS: AtomicUsize = AtomicUsize::new(0);

/// How many times a turn has started while a [`Poller`] was inside its wait
/// and had to end it — what a pin reads to know the case it means to cover
/// (a socket closed while the poller waited on it) actually arose.
pub fn interrupted_waits() -> usize {
    INTERRUPTED_WAITS.load(Ordering::SeqCst)
}

/// What the two threads share.
struct Shared {
    state: Mutex<Phase>,
    changed: Condvar,
    /// The sending end of the poller's bell: the loop's thread rings it to end
    /// a wait early. A loopback UDP pair, as the loop's own bell is, so it
    /// waits in the same `poll` / `WSAPoll` as the sockets on every platform.
    rope: UdpSocket,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Phase> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn wait<'guard>(&self, guard: MutexGuard<'guard, Phase>) -> MutexGuard<'guard, Phase> {
        self.changed
            .wait(guard)
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// The helper thread a hosted loop's sockets are waited on by (R8) — this
/// crate's [`SocketWatcher`].
///
/// The handoff, one snapshot at a time:
///
/// - At the end of a turn that found nothing to do, the loop ARMS the poller
///   with every socket its sources watch. The poller waits on them (and on its
///   bell) with no timeout — deadlines are the host's — and when one reports,
///   it keeps the report, calls the host's wake ONCE, and goes back to sleep
///   on the condition variable. It does not wait on that snapshot again: the
///   wait is level-triggered, so a socket still ready would answer at once,
///   forever, until the loop had acted on it — the busy spin.
/// - At the start of the next turn, before any program code runs, the loop
///   DISARMS it: a report is taken for that turn's poll; a wait still in
///   progress is ended by ringing the bell, and the loop's thread blocks until
///   the poller has left the wait. So no socket in a snapshot is ever closed
///   or replaced while the poller waits on it — sources only run in turns.
/// - Nothing is missed: readiness that arrives after a report, or while the
///   loop is busy, is still there for the next snapshot's wait to report at
///   once, and an interrupted wait's turn looks for itself.
///
/// The thread starts at the first arm and ends at [`SocketWatcher::release`]
/// (no socket watched any more) or when the poller is dropped.
pub struct Poller {
    wake: Wake,
    running: Option<Running>,
}

/// What a running poller thread shares with the loop. The thread itself is
/// detached: it ends on [`Phase::Exit`], and nothing waits for it.
struct Running {
    shared: Arc<Shared>,
}

impl Poller {
    /// A poller that calls `wake` when a snapshot reports. Starts no thread.
    pub fn new(wake: Wake) -> Poller {
        Poller {
            wake,
            running: None,
        }
    }

    /// Starts the thread, with a fresh bell. `None` if the bell cannot be
    /// opened or the thread cannot start.
    fn start(&self) -> Option<Running> {
        let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).ok()?;
        receiver.set_nonblocking(true).ok()?;
        let rope = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).ok()?;
        rope.connect(receiver.local_addr().ok()?).ok()?;
        rope.set_nonblocking(true).ok()?;
        let shared = Arc::new(Shared {
            state: Mutex::new(Phase::Idle),
            changed: Condvar::new(),
            rope,
        });
        // Counted from before the spawn, so a pin reading the count right
        // after an arm sees the thread the arm started.
        POLLER_THREADS.fetch_add(1, Ordering::SeqCst);
        let spawned = std::thread::Builder::new()
            .name("vilan-poller".to_string())
            .spawn({
                let shared = Arc::clone(&shared);
                let wake = Arc::clone(&self.wake);
                move || {
                    poller_thread(&shared, &receiver, &wake);
                    POLLER_THREADS.fetch_sub(1, Ordering::SeqCst);
                }
            });
        if spawned.is_err() {
            POLLER_THREADS.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        Some(Running { shared })
    }

    /// Tells the thread to end, without waiting for it. It is not inside a
    /// wait (the poller is disarmed first), so it leaves at once; the bell is
    /// rung anyway, so a thread somehow still waiting leaves too.
    fn stop(&mut self) {
        if let Some(running) = self.running.take() {
            *running.shared.lock() = Phase::Exit;
            let _ = running.shared.rope.send(&[1]);
            running.shared.changed.notify_all();
        }
    }
}

impl SocketWatcher for Poller {
    fn arm(&mut self, watches: Vec<Watch>) -> bool {
        if self.running.is_none() {
            self.running = self.start();
        }
        let Some(running) = &self.running else {
            return false;
        };
        *running.shared.lock() = Phase::Armed(watches);
        running.shared.changed.notify_all();
        true
    }

    fn disarm(&mut self) -> Option<Vec<Watch>> {
        let running = self.running.as_ref()?;
        let shared = &running.shared;
        let mut state = shared.lock();
        match std::mem::replace(&mut *state, Phase::Idle) {
            Phase::Reported(watches) => Some(watches),
            Phase::Waiting => {
                // Rung UNDER the lock: the poller sees `Interrupting` only once
                // it holds the lock, by which time the datagram is in its bell.
                *state = Phase::Interrupting;
                let _ = shared.rope.send(&[1]);
                INTERRUPTED_WAITS.fetch_add(1, Ordering::SeqCst);
                while !matches!(*state, Phase::Idle) {
                    state = shared.wait(state);
                }
                None
            }
            // Not picked up yet, or nothing armed: there is no wait to end.
            Phase::Armed(_) | Phase::Idle => None,
            Phase::Interrupting | Phase::Exit => {
                unreachable!("only `disarm` interrupts, and only `stop` exits")
            }
        }
    }

    fn release(&mut self) {
        self.stop();
    }
}

impl Drop for Poller {
    fn drop(&mut self) {
        // Disarmed first, so the thread is never left waiting on sockets
        // their owners are about to close.
        let _ = self.disarm();
        self.stop();
    }
}

/// The poller's thread: wait for a snapshot, wait on it, report, repeat.
fn poller_thread(shared: &Shared, bell: &UdpSocket, wake: &Wake) {
    let bell_socket = bell.raw_socket();
    loop {
        let mut watches = {
            let mut state = shared.lock();
            loop {
                match std::mem::replace(&mut *state, Phase::Idle) {
                    Phase::Armed(watches) => {
                        *state = Phase::Waiting;
                        break watches;
                    }
                    Phase::Exit => return,
                    other => {
                        *state = other;
                        state = shared.wait(state);
                    }
                }
            }
        };
        watches.push(Watch {
            socket: bell_socket,
            readable: true,
            writable: false,
            ready: false,
        });
        loop {
            let waited = wait(&mut watches, None);
            let mut state = shared.lock();
            match &*state {
                Phase::Interrupting => {
                    drain(bell);
                    *state = Phase::Idle;
                    shared.changed.notify_all();
                    break;
                }
                Phase::Exit => return,
                _ => {}
            }
            let bell_rang = watches.last().is_some_and(|watch| watch.ready);
            if bell_rang {
                drain(bell);
            }
            let socket_count = watches.len() - 1;
            let sockets = &mut watches[..socket_count];
            match waited {
                // A wait that fails reports everything: nothing is known, so
                // the loop's poll tries every socket, and the poller still
                // waits for the next snapshot rather than failing in a spin.
                Err(_) => sockets.iter_mut().for_each(|watch| watch.ready = true),
                // Only a stale ring: nothing to report, wait again.
                Ok(()) if !sockets.iter().any(|watch| watch.ready) => {
                    drop(state);
                    watches.iter_mut().for_each(|watch| watch.ready = false);
                    continue;
                }
                Ok(()) => {}
            }
            watches.pop();
            *state = Phase::Reported(watches);
            drop(state);
            wake();
            break;
        }
    }
}

/// Takes every datagram rung since the last drain.
fn drain(bell: &UdpSocket) {
    let mut datagram = [0u8; 8];
    while bell.recv(&mut datagram).is_ok() {}
}

/// The wait's timeout in whole milliseconds, ROUNDED UP — a deadline 0.3 ms
/// away waits 1 ms rather than 0, which would be a spin until it passed — and
/// `-1` (no limit) for `None`. Both `poll` and `WSAPoll` take this shape.
fn timeout_millis(timeout: Option<Duration>) -> i32 {
    let Some(timeout) = timeout else {
        return -1;
    };
    let whole = timeout.as_millis();
    let rounded = if timeout > Duration::from_millis(whole as u64) {
        whole + 1
    } else {
        whole
    };
    i32::try_from(rounded).unwrap_or(i32::MAX)
}

/// Blocks until one of `watches` is ready for what it is watched for (or
/// reports an error or a hang-up — `poll` always reports those), or `timeout`
/// passes, and marks each [`Watch::ready`] that reported anything. An
/// interrupted wait returns early and `Ok` with every watch marked: nothing is
/// known, so the loop's next poll tries them all.
#[cfg(unix)]
pub fn wait(watches: &mut [Watch], timeout: Option<Duration>) -> io::Result<()> {
    let mut descriptors: Vec<libc::pollfd> = watches
        .iter()
        .map(|watch| libc::pollfd {
            fd: watch.socket,
            events: (if watch.readable { libc::POLLIN } else { 0 })
                | (if watch.writable { libc::POLLOUT } else { 0 }),
            revents: 0,
        })
        .collect();
    let count = libc::nfds_t::try_from(descriptors.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "too many sockets to watch"))?;
    // SAFETY: `descriptors` is a live, exclusively borrowed array of `count`
    // initialized `pollfd`s for the duration of the call, which is all `poll`
    // reads and writes (it sets each `revents`). The descriptors are sockets
    // the loop's sources own and keep open across the call — the owned loop
    // is single-threaded and blocked here, and a hosted loop's [`Poller`]
    // waits only between turns, which is when no source runs (its disarm
    // ends the wait before the next turn's code) — and an fd that is not
    // open would be answered `POLLNVAL`, not undefined behaviour.
    let answered = unsafe { libc::poll(descriptors.as_mut_ptr(), count, timeout_millis(timeout)) };
    if answered < 0 {
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::Interrupted {
            for watch in watches.iter_mut() {
                watch.ready = true;
            }
            return Ok(());
        }
        return Err(error);
    }
    for (watch, descriptor) in watches.iter_mut().zip(&descriptors) {
        watch.ready = descriptor.revents != 0;
    }
    Ok(())
}

/// Blocks until one of `watches` is ready for what it is watched for (or
/// reports an error or a hang-up — `WSAPoll` always reports those), or
/// `timeout` passes, and marks each [`Watch::ready`] that reported anything.
#[cfg(windows)]
pub fn wait(watches: &mut [Watch], timeout: Option<Duration>) -> io::Result<()> {
    use windows_sys::Win32::Networking::WinSock::{
        POLLRDNORM, POLLWRNORM, SOCKET, SOCKET_ERROR, WSAGetLastError, WSAPOLLFD, WSAPoll,
    };
    let mut descriptors: Vec<WSAPOLLFD> = watches
        .iter()
        .map(|watch| WSAPOLLFD {
            // A `RawSocket` is the `SOCKET`'s value, widened to `u64`.
            fd: watch.socket as SOCKET,
            // `WSAPoll` refuses the flags it does not support (`POLLPRI`), so
            // the two NORMAL-data flags are named rather than `POLLIN`'s pair.
            events: (if watch.readable { POLLRDNORM } else { 0 })
                | (if watch.writable { POLLWRNORM } else { 0 }),
            revents: 0,
        })
        .collect();
    let count = u32::try_from(descriptors.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "too many sockets to watch"))?;
    // SAFETY: `descriptors` is a live, exclusively borrowed array of `count`
    // initialized `WSAPOLLFD`s for the duration of the call, which is all
    // `WSAPoll` reads and writes (it sets each `revents`). The sockets are
    // ones the loop's sources own and keep open across the call (a hosted
    // loop's [`Poller`] waits only between turns, when no source runs), and
    // Winsock
    // is initialized: every one of them was opened through `std::net`, which
    // starts it. The loop always watches its bell, so `count` is never 0.
    let answered = unsafe { WSAPoll(descriptors.as_mut_ptr(), count, timeout_millis(timeout)) };
    if answered == SOCKET_ERROR {
        // SAFETY: reads the calling thread's last Winsock error; no arguments.
        let code = unsafe { WSAGetLastError() };
        return Err(io::Error::from_raw_os_error(code));
    }
    for (watch, descriptor) in watches.iter_mut().zip(&descriptors) {
        watch.ready = descriptor.revents != 0;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_timeout_rounds_up_to_whole_milliseconds_and_none_is_no_limit() {
        assert_eq!(timeout_millis(None), -1);
        assert_eq!(timeout_millis(Some(Duration::ZERO)), 0);
        assert_eq!(timeout_millis(Some(Duration::from_micros(300))), 1);
        assert_eq!(timeout_millis(Some(Duration::from_millis(5))), 5);
        assert_eq!(timeout_millis(Some(Duration::from_micros(5_001))), 6);
        assert_eq!(
            timeout_millis(Some(Duration::from_secs(u64::MAX))),
            i32::MAX
        );
    }
}
