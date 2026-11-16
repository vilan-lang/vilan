//! The program's half and the main thread's bookkeeping, driven WITHOUT a
//! window system: a thread stands in for the platform loop, answering the
//! requests a real one would. The real loop is the end-to-end proof's
//! (`crates/vilan-cli/tests/desktop_shell.rs`, under xvfb).

use std::sync::atomic::AtomicUsize;
use std::time::{Duration, Instant};

use super::*;

/// A stand-in platform loop: answers every `Open` with `answer(..)`, hands
/// each opened window's state to `opened`, and ignores the rest.
fn stand_in(answer: fn() -> Result<u32, String>, opened: mpsc::Sender<Arc<WindowState>>) -> Link {
    let (requests, received) = mpsc::channel::<Request>();
    std::thread::spawn(move || {
        while let Ok(request) = received.recv() {
            if let Request::Open {
                window,
                answer: reply,
                ..
            } = request
            {
                let answered = answer();
                if answered.is_ok() {
                    let _ = opened.send(window);
                }
                let _ = reply.send(answered);
            }
        }
    });
    Link::new(move |request| requests.send(request).is_ok())
}

#[test]
fn open_answers_the_shells_id_and_the_window_keeps_the_loop_until_it_closes() {
    vilan_rt_poll::install();
    let (opened, windows) = mpsc::channel();
    let link = stand_in(|| Ok(7), opened);
    let id = open_through(&link, "http://127.0.0.1:1/", "t", 640, 480).expect("opened");
    assert_eq!(id, 7);
    let window = windows.recv().expect("the stand-in saw the window");
    // The platform loop closes the window 300 ms from now, from its thread.
    let closer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        window.mark_closed();
    });
    let started = Instant::now();
    let turns_before = vilan_rt::executor::turns();
    vilan_rt::executor::run_pending();
    let waited = started.elapsed();
    let turns = vilan_rt::executor::turns() - turns_before;
    closer.join().expect("closer");
    assert!(
        waited >= Duration::from_millis(250),
        "the loop ended before the window closed ({waited:?})"
    );
    // F160's measure: a loop that ticked would have taken ~300 turns here.
    assert!(
        turns <= 20,
        "an open window kept the loop TICKING: {turns} turns in {waited:?}"
    );
}

#[test]
fn a_refused_window_answers_the_reason_and_holds_nothing_open() {
    let (opened, _windows) = mpsc::channel();
    let link = stand_in(|| Err("no webview here".to_string()), opened);
    assert_eq!(
        open_through(&link, "about:blank", "t", 10, 10),
        Err("no webview here".to_string())
    );
    let started = Instant::now();
    vilan_rt::executor::run_pending();
    assert!(
        started.elapsed() < Duration::from_millis(200),
        "a window that never opened kept the loop alive"
    );
}

#[test]
fn a_window_with_a_zero_side_is_refused_before_the_shell_is_asked() {
    let link = Link::new(|_| panic!("the shell was asked"));
    for (width, height) in [(0, 10), (10, 0), (0, 0)] {
        let refused = open_through(&link, "about:blank", "t", width, height)
            .expect_err("a zero side is refused");
        assert!(
            refused.contains(&format!("{width} x {height}")),
            "{refused}"
        );
    }
}

#[test]
fn a_shell_that_has_stopped_is_an_err_not_a_hang() {
    // Refused at the send…
    let gone = Link::new(|_| false);
    assert_eq!(
        open_through(&gone, "about:blank", "t", 10, 10),
        Err(SHELL_GONE.to_string())
    );
    // …and accepted, then dropped unanswered.
    let dropped = Link::new(|request| {
        drop(request);
        true
    });
    assert_eq!(
        open_through(&dropped, "about:blank", "t", 10, 10),
        Err(SHELL_GONE.to_string())
    );
}

static LAST_CLOSED_BOOK: AtomicUsize = AtomicUsize::new(0);

fn record_last_closed_book() {
    LAST_CLOSED_BOOK.fetch_add(1, Ordering::SeqCst);
}

#[test]
fn the_book_numbers_windows_and_only_the_last_close_stops_the_program() {
    let state = || Arc::new(WindowState::new(vilan_rt::executor::loop_waker()));
    let mut windows: Windows<&str> = Windows::new(record_last_closed_book);
    let (first_state, second_state) = (state(), state());
    let first = windows.insert("first", Arc::clone(&first_state));
    let second = windows.insert("second", Arc::clone(&second_state));
    assert_eq!((first, second), (1, 2));
    assert_eq!(windows.find(|window| *window == "second"), Some(2));
    assert_eq!(windows.find(|window| *window == "third"), None);

    assert!(!windows.close(99), "an id never issued closes nothing");
    assert!(windows.close(first));
    assert!(first_state.is_closed() && !second_state.is_closed());
    assert_eq!(
        LAST_CLOSED_BOOK.load(Ordering::SeqCst),
        0,
        "a window closed while another is open stopped the program"
    );
    assert!(
        !windows.close(first),
        "a second close of one window is a no-op"
    );

    assert!(windows.close(second));
    assert!(second_state.is_closed());
    assert_eq!(LAST_CLOSED_BOOK.load(Ordering::SeqCst), 1);

    // Ids are never reused, so a stale `Window` cannot close a newer one.
    let third = windows.insert("third", state());
    assert_eq!(third, 3);
}
