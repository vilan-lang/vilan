//! R8: a vilan program driven by a HOST event loop through
//! `vilan_rt::executor::turn`, with the host's wake installed and (for the
//! socket pins) the poller of `vilan_rt_poll::install_hosted`.
//!
//! Every pin runs its program on a fresh thread — a fresh loop — under the
//! reference host (`support`), and takes its ordering from the host's own
//! counts, the channels it hands results through, and the loop's wake —
//! never from a sleep. Termination is process-wide, so its pins live in files
//! of their own.

mod support;

use std::cell::{Cell, RefCell};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use support::{Host, Pending, arrival, on_fresh_thread};
use vilan_rt::executor::{
    Finished, Interest, IoSource, Program, TimerHandle, block_on, loop_waker, nursery_new,
    nursery_run, pin_future, promise, queue_microtask, register_io, settle_all, sleep, spawn,
    timer, yield_now,
};
use vilan_rt::http::{Handler, Request, Response, create_server};

/// Held by every pin that starts a poller thread or reads the process-wide
/// poller counts (`poller_threads`, `interrupted_waits`): under `cargo test`
/// a binary's pins are threads of ONE process, and another pin's poller would
/// move the counts this one reads. Under nextest each pin is its own process
/// and the lock is never contended.
static POLLER_COUNTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn poller_counts() -> std::sync::MutexGuard<'static, ()> {
    POLLER_COUNTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A recorder the ordering pins write into.
#[derive(Clone, Default)]
struct Log(Rc<RefCell<Vec<String>>>);

impl Log {
    fn push(&self, entry: impl Into<String>) {
        self.0.borrow_mut().push(entry.into());
    }

    fn taken(&self) -> Vec<String> {
        self.0.borrow().clone()
    }
}

// ---------------------------------------------------------------- timers ---

#[test]
fn timers_fire_on_time_and_in_order_under_a_host() {
    let (fired, wakes, turns) = on_fresh_thread(|| {
        let host = Host::without_sockets();
        let started = Instant::now();
        let fired = host.run(Program::start(async move {
            let fired: Rc<RefCell<Vec<(u64, Duration)>>> = Rc::default();
            let mut tasks = Vec::new();
            for delay in [60u64, 20, 40, 20] {
                let fired = Rc::clone(&fired);
                tasks.push(spawn(
                    async move {
                        sleep(delay as i32, None).await;
                        fired.borrow_mut().push((delay, started.elapsed()));
                    },
                    "test",
                ));
            }
            settle_all(tasks).await;
            fired.borrow().clone()
        }));
        (fired, host.wakes(), host.turns())
    });
    let order: Vec<u64> = fired.iter().map(|(delay, _)| *delay).collect();
    assert_eq!(order, [20, 20, 40, 60], "by deadline, then registration");
    for (delay, at) in &fired {
        assert!(
            *at >= Duration::from_millis(*delay),
            "the {delay} ms timer fired early, at {at:?}"
        );
        assert!(
            *at < Duration::from_millis(*delay) + Duration::from_millis(500),
            "the {delay} ms timer fired late, at {at:?}"
        );
    }
    // A timer is a deadline the host waits out: nothing has to wake it, and
    // it turns about once per distinct deadline.
    assert_eq!(wakes, 0, "no wake is needed for a timer");
    assert!(turns <= 12, "{turns} host turns for three deadlines");
}

/// One program, every ordering rule J6 pinned: eager spawns, microtasks to
/// exhaustion before each macrotask, one timer per macrotask, `yield_now`'s
/// hop, `queue_microtask`, a nursery's join. Run under the owned loop and
/// under the host, the logs are the same — with and without a live I/O
/// source, which moves the loop onto its other macrotask path.
async fn ordering_program(log: Log, with_source: bool) {
    let live = Rc::new(Cell::new(with_source));
    if with_source {
        register_io(Rc::new(Quiet {
            live: Rc::clone(&live),
        }));
    }
    log.push("main start");
    let mut tasks = Vec::new();
    for (label, delay) in [("t0-a", 0), ("t0-b", 0), ("t3", 3), ("t1", 1)] {
        let log = log.clone();
        tasks.push(spawn(
            async move {
                log.push(format!("{label} before"));
                sleep(delay, None).await;
                log.push(format!("{label} woke"));
                yield_now().await;
                log.push(format!("{label} hop"));
                let inner = log.clone();
                queue_microtask(move || inner.push(format!("{label} microtask")));
                log.push(format!("{label} after queue"));
            },
            "test",
        ));
    }
    log.push("main spawned");
    let nursery = nursery_new(None);
    let joined = nursery.clone();
    let nursery_log = log.clone();
    nursery_run(
        nursery,
        pin_future(async move {
            for step in 0..3 {
                let log = nursery_log.clone();
                vilan_rt::executor::spawn_in(
                    async move {
                        sleep(0, None).await;
                        log.push(format!("child {step}"));
                    },
                    "test",
                    Some(joined.clone()),
                );
            }
            nursery_log.push("nursery body");
        }),
    )
    .await;
    log.push("nursery joined");
    settle_all(tasks).await;
    log.push("main end");
    live.set(false);
}

/// Live while `live` says so, watching nothing, never progressing.
struct Quiet {
    live: Rc<Cell<bool>>,
}

impl IoSource for Quiet {
    fn poll(&self) -> bool {
        false
    }

    fn interest(&self, _interest: &mut Interest) -> bool {
        true
    }

    fn is_live(&self) -> bool {
        self.live.get()
    }
}

fn ordering_matches_the_owned_loop(with_source: bool) {
    let owned = on_fresh_thread(move || {
        let log = Log::default();
        block_on(ordering_program(log.clone(), with_source));
        log.taken()
    });
    let hosted = on_fresh_thread(move || {
        let host = Host::with_sockets();
        let log = Log::default();
        host.run(Program::start(ordering_program(log.clone(), with_source)));
        log.taken()
    });
    assert!(owned.len() > 20, "the program ran: {owned:?}");
    assert_eq!(hosted, owned, "the host ran a different program");
}

#[test]
fn microtask_and_macrotask_order_under_a_host_is_the_owned_loops() {
    ordering_matches_the_owned_loop(false);
}

#[test]
fn microtask_and_macrotask_order_beside_a_live_source_is_the_owned_loops() {
    ordering_matches_the_owned_loop(true);
}

#[test]
fn a_sync_mains_pending_work_runs_under_a_host() {
    let (log, finished) = on_fresh_thread(|| {
        let host = Host::without_sockets();
        let log = Log::default();
        // A sync `main` left a microtask and a timer behind.
        let late = log.clone();
        queue_microtask(move || late.push("microtask"));
        let timed = log.clone();
        spawn(
            async move {
                sleep(5, None).await;
                timed.push("timer");
            },
            "test",
        );
        log.push("main end");
        let finished = host.drive(&Pending);
        (log.taken(), finished)
    });
    assert_eq!(log, ["main end", "microtask", "timer"]);
    assert_eq!(finished, Finished::Drained);
}

#[test]
fn a_failing_root_raises_out_of_the_hosts_turn() {
    let outcome = on_fresh_thread(|| {
        let host = Host::without_sockets();
        let program = Program::start(async {
            sleep(1, None).await;
            vilan_rt::panic_with("the root failed");
        });
        vilan_rt::guarded(|| {
            host.drive(&program);
        })
        .err()
        .map(|message| message.to_string())
    });
    assert_eq!(outcome.as_deref(), Some("the root failed"));
}

// ------------------------------------------------------------ cross-thread ---

/// The source of the cross-thread pin: progress once the other thread's flag
/// is up. It watches nothing — only the waker can end the host's sleep.
struct Flagged {
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    seen: Cell<Option<Instant>>,
    backstop: TimerHandle,
}

impl IoSource for Flagged {
    fn poll(&self) -> bool {
        if self.seen.get().is_none() && self.flag.load(std::sync::atomic::Ordering::SeqCst) {
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
fn a_wake_from_another_thread_reaches_the_host() {
    let (raised, seen, wakes) = on_fresh_thread(|| {
        let host = Host::without_sockets();
        let idle = host.idle_watch();
        let (raised, seen) = host.run(Program::start(async move {
            let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let backstop = timer(5_000);
            let source = Rc::new(Flagged {
                flag: std::sync::Arc::clone(&flag),
                seen: Cell::new(None),
                backstop: backstop.clone(),
            });
            register_io(Rc::clone(&source) as Rc<dyn IoSource>);
            let waker = loop_waker();
            // The raise waits for the host to be ASLEEP — its first idle, which
            // comes after this turn — so the wake is what must end the sleep.
            let worker = std::thread::spawn(move || {
                idle.until_idle(1);
                let raised = Instant::now();
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
                waker.wake();
                raised
            });
            let ran_out = backstop.wait(None).await;
            assert!(!ran_out, "the other thread's wake never reached the host");
            (worker.join().expect("the waking thread"), source.seen.get())
        }));
        (raised, seen, host.wakes())
    });
    let seen = seen.expect("the flag was seen");
    assert!(seen >= raised);
    assert!(
        seen.duration_since(raised) < Duration::from_millis(500),
        "the wake reached the host {:?} after it was raised",
        seen.duration_since(raised)
    );
    assert!(wakes >= 1, "the host's own wake carried it");
}

// ------------------------------------------------------------------ sockets ---

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
        let read = stream
            .read(&mut chunk)
            .expect("an answer before the read timeout");
        assert!(read > 0, "the server closed a kept-alive connection");
        pending.extend_from_slice(&chunk[..read]);
    }
}

/// A client on its own thread: `requests` keep-alive requests on one
/// connection, then one on a second connection. The read timeout is the
/// pin's failure, not its ordering: every answer is awaited, never slept for.
fn client(port: u16, requests: usize) -> Vec<String> {
    let mut answers = Vec::new();
    for connection in 0..2 {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        stream.set_nodelay(true).expect("nodelay");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("timeout");
        let mut pending = Vec::new();
        let count = if connection == 0 { requests } else { 1 };
        for index in 0..count {
            write!(
                stream,
                "GET /{connection}/{index} HTTP/1.1\r\nHost: a\r\n\r\n"
            )
            .expect("send");
            answers.push(read_answer(&mut stream, &mut pending));
        }
    }
    answers
}

#[test]
fn an_http_round_trip_runs_under_a_host() {
    let _serial = poller_counts();
    const REQUESTS: usize = 200;
    let (answers, threads_while_serving, threads_after, wakes, turns) = on_fresh_thread(|| {
        let host = Host::with_sockets();
        let (answers, threads_while_serving) = host.run(Program::start(async {
            let handler: Handler = Rc::new(|request: Request, response: Response| {
                pin_future(async move {
                    response.end(&format!("ok {}", request.url()));
                })
            });
            let server = create_server(handler);
            server.listen(0, Rc::new(|| {}));
            let port = server.port() as u16;
            let (sender, receiver) = mpsc::channel();
            let waker = loop_waker();
            std::thread::spawn(move || {
                let answers = client(port, REQUESTS);
                let _ = sender.send(answers);
                waker.wake();
            });
            let answers = arrival(receiver).await;
            let threads_while_serving = vilan_rt_poll::poller_threads();
            server.close(Rc::new(|| {}));
            (answers, threads_while_serving)
        }));
        (
            answers,
            threads_while_serving,
            wait_for_poller_threads(0),
            host.wakes(),
            host.turns(),
        )
    });
    assert_eq!(answers.len(), REQUESTS + 1);
    for (index, answer) in answers.iter().take(REQUESTS).enumerate() {
        assert_eq!(answer, &format!("ok /0/{index}"));
    }
    assert_eq!(answers[REQUESTS], "ok /1/0");
    assert_eq!(
        threads_while_serving, 1,
        "one poller while a socket is watched"
    );
    assert_eq!(
        threads_after, 0,
        "the poller ends once no socket is watched"
    );
    assert!(
        wakes > REQUESTS,
        "{wakes} wakes: each request reached the host through the poller"
    );
    // One report, one wake, per snapshot, and a turn arms at most one
    // snapshot: a poller that looked at a still-ready socket again before the
    // loop had acted on it would wake the host over and over in between.
    // (The one more is the client thread's own wake, with the answers.)
    assert!(
        wakes <= turns as usize + 1,
        "{wakes} wakes for {turns} turns: the poller is spinning on a level it already reported"
    );
}

/// Waits for the process's poller-thread count to reach `count` — a released
/// poller's thread leaves on its own — and answers the count it saw last.
fn wait_for_poller_threads(count: usize) -> usize {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let threads = vilan_rt_poll::poller_threads();
        if threads == count || Instant::now() >= deadline {
            return threads;
        }
        std::thread::yield_now();
    }
}

#[test]
fn a_program_that_watches_no_socket_starts_no_thread() {
    let _serial = poller_counts();
    let (during, after) = on_fresh_thread(|| {
        let host = Host::with_sockets();
        let during = host.run(Program::start(async {
            let live = Rc::new(Cell::new(true));
            register_io(Rc::new(Quiet {
                live: Rc::clone(&live),
            }));
            sleep(30, None).await;
            let during = vilan_rt_poll::poller_threads();
            live.set(false);
            during
        }));
        (during, vilan_rt_poll::poller_threads())
    });
    assert_eq!(
        during, 0,
        "a poller thread for a loop that watches no socket"
    );
    assert_eq!(after, 0);
}

#[test]
fn an_idle_hosted_server_makes_zero_wakes_and_almost_no_turns() {
    let _serial = poller_counts();
    let (wakes, turns, idle_turns) = on_fresh_thread(|| {
        let host = Host::with_sockets();
        let host = Rc::new(host);
        let measured = Rc::clone(&host);
        let (wakes, turns, idle_turns) = host.run(Program::start(async move {
            let handler: Handler = Rc::new(|_request: Request, response: Response| {
                pin_future(async move { response.end("never asked") })
            });
            let server = create_server(handler);
            server.listen(0, Rc::new(|| {}));
            // One short sleep first, so the poller is armed and waiting before
            // the window opens.
            sleep(5, None).await;
            let (wakes, turns, loop_turns) = (
                measured.wakes(),
                measured.turns(),
                vilan_rt::executor::turns(),
            );
            sleep(300, None).await;
            let measured = (
                measured.wakes() - wakes,
                measured.turns() - turns,
                vilan_rt::executor::turns() - loop_turns,
            );
            server.close(Rc::new(|| {}));
            measured
        }));
        (wakes, turns, idle_turns)
    });
    assert_eq!(wakes, 0, "the host was woken while the server sat idle");
    // The 300 ms window is one deadline: the host turns once for it, and the
    // loop takes a handful of macrotask steps. The tick took ~300 of each.
    assert!(turns <= 3, "{turns} host turns over an idle 300 ms");
    assert!(
        idle_turns <= 10,
        "{idle_turns} loop steps over an idle 300 ms"
    );
}

/// A listener that is REPLACED while the poller waits on it: when `replace`
/// is set, the next poll closes the listener and binds another (on Linux
/// usually the same descriptor number), whose port goes to the client.
struct Replaced {
    listener: RefCell<Option<TcpListener>>,
    replace: Cell<bool>,
    replaced_port: RefCell<Option<mpsc::Sender<u16>>>,
    accepted: Cell<bool>,
    on_accept: RefCell<Option<Rc<dyn Fn(())>>>,
}

impl IoSource for Replaced {
    fn poll(&self) -> bool {
        if self.replace.replace(false) {
            // Closed FIRST, then bound: the old socket is gone before the new
            // one exists, which is what lets the descriptor be reused.
            drop(self.listener.borrow_mut().take());
            let fresh = TcpListener::bind(("127.0.0.1", 0)).expect("the replacement");
            fresh.set_nonblocking(true).expect("nonblocking");
            let port = fresh.local_addr().expect("address").port();
            *self.listener.borrow_mut() = Some(fresh);
            if let Some(sender) = self.replaced_port.borrow_mut().take() {
                let _ = sender.send(port);
            }
            return true;
        }
        if self.accepted.get() {
            return false;
        }
        let listener = self.listener.borrow();
        match listener.as_ref().map(TcpListener::accept) {
            Some(Ok(_)) => {
                self.accepted.set(true);
                if let Some(resolve) = self.on_accept.borrow_mut().take() {
                    resolve(());
                }
                true
            }
            _ => false,
        }
    }

    fn interest(&self, interest: &mut Interest) -> bool {
        if !self.accepted.get()
            && let Some(listener) = self.listener.borrow().as_ref()
        {
            interest.watch(listener, true, false);
        }
        true
    }

    fn is_live(&self) -> bool {
        !self.accepted.get()
    }
}

#[test]
fn replacing_a_socket_while_the_poller_waits_on_it_is_seen() {
    let _serial = poller_counts();
    let interrupts = on_fresh_thread(|| {
        let host = Host::with_sockets();
        let interrupts_before = vilan_rt_poll::interrupted_waits();
        host.run(Program::start(async {
            let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
            listener.set_nonblocking(true).expect("nonblocking");
            let (sender, receiver) = mpsc::channel();
            let source = Rc::new(Replaced {
                listener: RefCell::new(Some(listener)),
                replace: Cell::new(false),
                replaced_port: RefCell::new(Some(sender)),
                accepted: Cell::new(false),
                on_accept: RefCell::new(None),
            });
            register_io(Rc::clone(&source) as Rc<dyn IoSource>);
            let held = Rc::clone(&source);
            let accepted = promise(Rc::new(move |resolve: Rc<dyn Fn(())>| {
                *held.on_accept.borrow_mut() = Some(resolve);
            }));
            // The poller is armed with the first listener at the end of each
            // turn. A turn whose start had to interrupt it waiting — the
            // counter says which — is where the replacement is asked for, so
            // that turn's poll closes the listener and binds another.
            let before = vilan_rt_poll::interrupted_waits();
            while vilan_rt_poll::interrupted_waits() == before {
                sleep(5, None).await;
            }
            source.replace.set(true);
            std::thread::spawn(move || {
                let port = receiver.recv().expect("the replacement's port");
                // Held open until the process ends: the accept is the event.
                std::mem::forget(TcpStream::connect(("127.0.0.1", port)).expect("connect"));
            });
            // No timer is left and the source watches only the replacement,
            // so its readiness, through the poller, is the one thing that can
            // wake the host now; the host's backstop is the failure.
            accepted.await;
        }));
        vilan_rt_poll::interrupted_waits() - interrupts_before
    });
    assert!(
        interrupts >= 1,
        "no turn had to interrupt a waiting poller, so this pin proved nothing"
    );
}
