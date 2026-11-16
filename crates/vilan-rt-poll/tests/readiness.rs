//! F160: the loop waits for READINESS once `vilan_rt_poll::install` has run —
//! a socket, a timer, or another thread's wake ends the wait — and an idle
//! loop does not spin.
//!
//! Each pin counts the loop's turns (`vilan_rt::executor::turns`): the 1 ms
//! tick this replaced took one per millisecond, so a bound well under the
//! window's length in milliseconds is the difference between the two. Every
//! wait a pin depends on has a backstop timer, so a wake that never comes
//! fails the pin instead of hanging it.

use std::cell::{Cell, RefCell};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use vilan_rt::executor::{
    Interest, IoSource, TimerHandle, block_on, loop_waker, register_io, sleep, timer, turns,
};

/// How long a backstop waits before it ends a pin's loop. Far past anything a
/// working wake takes, so reaching it is the failure.
const BACKSTOP_MILLIS: i32 = 5_000;

/// A source with nothing to watch that stays live until `until` says
/// otherwise — the shape of stdin's source, whose work arrives on a thread.
struct Quiet {
    polls: Cell<u64>,
    live: Rc<Cell<bool>>,
}

impl IoSource for Quiet {
    fn poll(&self) -> bool {
        self.polls.set(self.polls.get() + 1);
        false
    }

    fn interest(&self, _interest: &mut Interest) -> bool {
        true
    }

    fn is_live(&self) -> bool {
        self.live.get()
    }
}

#[test]
fn an_idle_source_does_not_wake_the_loop_while_a_timer_runs_out() {
    vilan_rt_poll::install();
    let (elapsed, taken) = block_on(async {
        let live = Rc::new(Cell::new(true));
        register_io(Rc::new(Quiet {
            polls: Cell::new(0),
            live: Rc::clone(&live),
        }));
        let before = turns();
        let started = Instant::now();
        sleep(300, None).await;
        let elapsed = started.elapsed();
        live.set(false);
        (elapsed, turns() - before)
    });
    assert!(
        elapsed >= Duration::from_millis(300),
        "the timer ran out: {elapsed:?}"
    );
    // The tick took ~300 turns here; the wait takes the timer's own, plus a
    // few for an early return the contract allows.
    assert!(
        taken <= 20,
        "{taken} turns over an idle 300 ms: the loop is spinning"
    );
}

/// The source of the cross-thread pin: progress once the other thread's flag
/// is up, then done. It watches nothing — only the waker can end the wait.
struct Flagged {
    flag: Arc<AtomicBool>,
    seen: Cell<Option<Instant>>,
    backstop: TimerHandle,
}

impl IoSource for Flagged {
    fn poll(&self) -> bool {
        if self.seen.get().is_none() && self.flag.load(Ordering::SeqCst) {
            self.seen.set(Some(Instant::now()));
            self.backstop.cancel();
            return true;
        }
        false
    }

    fn interest(&self, _interest: &mut Interest) -> bool {
        true
    }

    fn is_live(&self) -> bool {
        self.seen.get().is_none()
    }
}

#[test]
fn a_wake_from_another_thread_ends_the_wait() {
    vilan_rt_poll::install();
    let (raised, seen, taken) = block_on(async {
        let flag = Arc::new(AtomicBool::new(false));
        let backstop = timer(BACKSTOP_MILLIS);
        let source = Rc::new(Flagged {
            flag: Arc::clone(&flag),
            seen: Cell::new(None),
            backstop: backstop.clone(),
        });
        register_io(Rc::clone(&source) as Rc<dyn IoSource>);
        let waker = loop_waker();
        let before = turns();
        let worker = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            let raised = Instant::now();
            flag.store(true, Ordering::SeqCst);
            waker.wake();
            raised
        });
        // Nothing else wakes the loop: this task waits on the backstop, which
        // the source cancels when it sees the flag — a `true` verdict is the
        // backstop running out, the wake lost.
        let ran_out = backstop.wait(None).await;
        assert!(!ran_out, "the other thread's wake never reached the loop");
        let raised = worker.join().expect("the waking thread");
        (raised, source.seen.get(), turns() - before)
    });
    let seen = seen.expect("the flag was seen");
    assert!(
        seen.duration_since(raised) < Duration::from_millis(500),
        "the wake reached the loop {:?} after it was raised",
        seen.duration_since(raised)
    );
    assert!(taken <= 20, "{taken} turns around one wake");
}

/// Re-checks `ready` every 25 ms — a pin's own waiting, coarse on purpose so
/// it adds few turns of its own.
async fn until(ready: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_millis(BACKSTOP_MILLIS as u64);
    while !ready() {
        assert!(Instant::now() < deadline, "the awaited state never came");
        sleep(25, None).await;
    }
}

/// The source of the socket pin: a listener that accepts one connection.
struct Accepting {
    listener: TcpListener,
    accepted: RefCell<Option<(TcpStream, Instant)>>,
    backstop: TimerHandle,
}

impl IoSource for Accepting {
    fn poll(&self) -> bool {
        if self.accepted.borrow().is_some() {
            return false;
        }
        match self.listener.accept() {
            Ok((stream, _)) => {
                *self.accepted.borrow_mut() = Some((stream, Instant::now()));
                self.backstop.cancel();
                true
            }
            Err(_) => false,
        }
    }

    fn interest(&self, interest: &mut Interest) -> bool {
        interest.watch(&self.listener, true, false);
        true
    }

    fn is_live(&self) -> bool {
        self.accepted.borrow().is_none()
    }
}

#[test]
fn a_socket_becoming_readable_ends_the_wait() {
    vilan_rt_poll::install();
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    listener.set_nonblocking(true).expect("nonblocking");
    let port = listener.local_addr().expect("address").port();
    let (connected, accepted, taken) = block_on(async move {
        let backstop = timer(BACKSTOP_MILLIS);
        let source = Rc::new(Accepting {
            listener,
            accepted: RefCell::new(None),
            backstop: backstop.clone(),
        });
        register_io(Rc::clone(&source) as Rc<dyn IoSource>);
        let before = turns();
        let client = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            let connected = Instant::now();
            let stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
            (stream, connected)
        });
        // As above: only the listener's readiness can end this wait early.
        let ran_out = backstop.wait(None).await;
        assert!(
            !ran_out,
            "the connection's readiness never reached the loop"
        );
        let (_stream, connected) = client.join().expect("the client thread");
        let accepted = source.accepted.borrow().as_ref().map(|(_, at)| *at);
        (connected, accepted, turns() - before)
    });
    let accepted = accepted.expect("the connection was accepted");
    assert!(
        accepted.duration_since(connected) < Duration::from_millis(500),
        "accepted {:?} after the connect",
        accepted.duration_since(connected)
    );
    assert!(taken <= 20, "{taken} turns around one accept");
}

/// An HTTP server, idle and then busy: idle, it takes almost no turns; busy,
/// it answers a few hundred keep-alive requests on one connection, each as
/// soon as it arrives (the tick answered at most one a millisecond).
#[test]
fn an_idle_server_sleeps_and_a_busy_one_answers_keep_alive_requests() {
    use vilan_rt::http::{Handler, Request, Response, create_server};

    vilan_rt_poll::install();
    const REQUESTS: usize = 300;
    let (idle_turns, answers) = block_on(async {
        let handler: Handler = Rc::new(|request: Request, response: Response| {
            vilan_rt::executor::pin_future(async move {
                response.end(&format!("ok {}", request.url()));
            })
        });
        let server = create_server(handler);
        server.listen(0, Rc::new(|| {}));
        let port = server.port() as u16;
        let before = turns();
        sleep(300, None).await;
        let idle_turns = turns() - before;
        let client = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
            stream.set_nodelay(true).expect("nodelay");
            let mut answers = Vec::new();
            let mut pending = Vec::new();
            for index in 0..REQUESTS {
                write!(stream, "GET /{index} HTTP/1.1\r\nHost: a\r\n\r\n").expect("send");
                answers.push(read_answer(&mut stream, &mut pending));
            }
            answers
        });
        until(|| client.is_finished()).await;
        server.close(Rc::new(|| {}));
        (idle_turns, client.join().expect("the client thread"))
    });
    assert!(
        idle_turns <= 20,
        "{idle_turns} turns while the server sat idle for 300 ms: the loop is spinning"
    );
    assert_eq!(answers.len(), REQUESTS);
    for (index, answer) in answers.iter().enumerate() {
        assert_eq!(answer, &format!("ok /{index}"));
    }
}

/// One `Content-Length` answer's body off a kept-alive connection, keeping
/// whatever arrived past it in `pending`.
fn read_answer(stream: &mut TcpStream, pending: &mut Vec<u8>) -> String {
    loop {
        if let Some(end) = pending.windows(4).position(|window| window == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&pending[..end]).into_owned();
            let length: usize = head
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().ok())?
                })
                .expect("a Content-Length answer");
            if pending.len() >= end + 4 + length {
                let body =
                    String::from_utf8_lossy(&pending[end + 4..end + 4 + length]).into_owned();
                pending.drain(..end + 4 + length);
                return body;
            }
        }
        let mut chunk = [0u8; 4096];
        let read = stream.read(&mut chunk).expect("read");
        assert!(read > 0, "the server closed a kept-alive connection");
        pending.extend_from_slice(&chunk[..read]);
    }
}
