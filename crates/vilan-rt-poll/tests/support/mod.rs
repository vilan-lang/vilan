//! The REFERENCE HOST (R8): a plain-Rust event loop that drives a vilan
//! program through `vilan_rt::executor::turn` the way a window's event loop
//! will, with no window in it.
//!
//! It owns its thread's waiting and nothing else: after each turn it sleeps on
//! a condition variable until the turn's deadline or until the wake it
//! installed is called, from whatever thread. A condition variable rather than
//! `park`, deliberately: the executor still unparks the loop's thread on every
//! wake, and a host that slept in `park` would be woken by that and prove
//! nothing about the installed wake — `winit` does not hear `unpark`.
//!
//! Every count a pin asserts on is the host's own: how many turns it ran, how
//! many times its wake was called, how many times it went idle. And every run
//! has a BACKSTOP — a wall-clock bound far past anything a working loop needs
//! — so a wake that never comes fails the pin instead of hanging it.

#![allow(dead_code, reason = "each pin binary uses its own part of the host")]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use vilan_rt::executor::{
    Finished, Interest, IoSource, Program, Task, Turn, Wake, promise, register_io,
};

/// How long any one run may take before the host calls it hung.
pub const BACKSTOP: Duration = Duration::from_secs(10);

/// What the wake writes and the host's sleep reads.
#[derive(Default)]
struct Note {
    woken: Mutex<bool>,
    rung: Condvar,
    wakes: AtomicUsize,
    idles: Mutex<usize>,
    went_idle: Condvar,
}

/// The reference host. Made on the thread whose loop it drives.
pub struct Host {
    note: Arc<Note>,
    turns: Cell<u64>,
}

/// What the host's view of the turns needs: the program's, or the root-less
/// loop's.
pub trait Driven {
    fn turn(&self) -> Turn;
}

impl<T: Clone + 'static> Driven for Program<T> {
    fn turn(&self) -> Turn {
        Program::turn(self)
    }
}

/// A sync `main`'s loop: `vilan_rt::executor::turn`.
pub struct Pending;

impl Driven for Pending {
    fn turn(&self) -> Turn {
        vilan_rt::executor::turn()
    }
}

/// A handle another thread uses to watch the host go idle.
#[derive(Clone)]
pub struct IdleWatch(Arc<Note>);

impl IdleWatch {
    /// Blocks until the host has gone idle at least `count` times in all.
    pub fn until_idle(&self, count: usize) {
        let deadline = Instant::now() + BACKSTOP;
        let mut idles = self.0.idles.lock().unwrap_or_else(PoisonError::into_inner);
        while *idles < count {
            let now = Instant::now();
            assert!(now < deadline, "the host never went idle");
            idles = self
                .0
                .went_idle
                .wait_timeout(idles, deadline - now)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }
}

impl Host {
    /// A host whose loop has a socket watcher (`vilan_rt_poll::install_hosted`).
    pub fn with_sockets() -> Host {
        let host = Host::bare();
        vilan_rt_poll::install_hosted(host.wake());
        host
    }

    /// A host that installs only its wake — no readiness wait, no watcher.
    pub fn without_sockets() -> Host {
        let host = Host::bare();
        vilan_rt::executor::install_host(host.wake());
        host
    }

    fn bare() -> Host {
        Host {
            note: Arc::new(Note::default()),
            turns: Cell::new(0),
        }
    }

    /// The wake this host installs: note it, count it, ring the sleeper.
    fn wake(&self) -> Wake {
        let note = Arc::clone(&self.note);
        Arc::new(move || {
            note.wakes.fetch_add(1, Ordering::SeqCst);
            *note.woken.lock().unwrap_or_else(PoisonError::into_inner) = true;
            note.rung.notify_all();
        })
    }

    /// How many times the installed wake has been called.
    pub fn wakes(&self) -> usize {
        self.note.wakes.load(Ordering::SeqCst)
    }

    /// How many turns the host has run.
    pub fn turns(&self) -> u64 {
        self.turns.get()
    }

    pub fn idle_watch(&self) -> IdleWatch {
        IdleWatch(Arc::clone(&self.note))
    }

    /// Runs `program` until its loop finishes, and answers its value.
    pub fn run<T: Clone + 'static>(&self, program: Program<T>) -> T {
        self.drive(&program);
        program.finish()
    }

    /// Turns `loop_` until it finishes — the host's whole event loop.
    pub fn drive(&self, loop_: &impl Driven) -> Finished {
        let started = Instant::now();
        loop {
            // Cleared BEFORE the turn: a wake that arrives during it is a
            // reason to turn again, and must not be lost.
            *self
                .note
                .woken
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = false;
            self.turns.set(self.turns.get() + 1);
            match loop_.turn() {
                Turn::Finished(finished) => return finished,
                Turn::Idle { deadline, .. } => self.sleep(deadline, started),
            }
        }
    }

    /// Sleeps until `deadline` or a wake, whichever is first.
    fn sleep(&self, deadline: Option<Instant>, started: Instant) {
        {
            let mut idles = self
                .note
                .idles
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            *idles += 1;
            self.note.went_idle.notify_all();
        }
        let backstop = started + BACKSTOP;
        let mut woken = self
            .note
            .woken
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        while !*woken {
            let now = Instant::now();
            assert!(
                now < backstop,
                "the host waited past its backstop: a wake never came"
            );
            let until = deadline.map_or(backstop, |deadline| deadline.min(backstop));
            if now >= until {
                if deadline.is_some_and(|deadline| now >= deadline) {
                    return;
                }
                continue;
            }
            woken = self
                .note
                .rung
                .wait_timeout(woken, until - now)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }
}

/// A source that settles a task with the first value another thread sends —
/// how a pin hands a result from a helper thread into the program, ordered by
/// the channel and the loop's wake rather than by any sleep. It watches no
/// socket, so only the waker the sender holds can end the loop's wait.
pub struct Arrival<T> {
    receiver: Receiver<T>,
    resolve: RefCell<Option<Rc<dyn Fn(T)>>>,
    done: Cell<bool>,
}

impl<T: 'static> IoSource for Arrival<T> {
    fn poll(&self) -> bool {
        if self.done.get() {
            return false;
        }
        match self.receiver.try_recv() {
            Ok(value) => {
                self.done.set(true);
                if let Some(resolve) = self.resolve.borrow_mut().take() {
                    resolve(value);
                }
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => panic!("the sender went away without sending"),
        }
    }

    fn interest(&self, _interest: &mut Interest) -> bool {
        true
    }

    fn is_live(&self) -> bool {
        !self.done.get()
    }
}

/// Registers an [`Arrival`] on `receiver` and answers the task it settles.
pub fn arrival<T: Clone + 'static>(receiver: Receiver<T>) -> Task<T> {
    let source = Rc::new(Arrival {
        receiver,
        resolve: RefCell::new(None),
        done: Cell::new(false),
    });
    let held = Rc::clone(&source);
    let task = promise(Rc::new(move |resolve: Rc<dyn Fn(T)>| {
        *held.resolve.borrow_mut() = Some(resolve);
    }));
    register_io(source as Rc<dyn IoSource>);
    task
}

/// Runs `body` on a fresh thread — a fresh loop — and answers its result,
/// with the panic carried across.
pub fn on_fresh_thread<R: Send + 'static>(body: impl FnOnce() -> R + Send + 'static) -> R {
    match std::thread::spawn(body).join() {
        Ok(result) => result,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}
