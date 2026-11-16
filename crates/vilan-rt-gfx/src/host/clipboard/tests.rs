//! Pins of the clipboard's host half (F173) with a fake clipboard in the
//! system's place: the worker's order, a clipboard that cannot be made (no
//! display) or fails, its life, and the extern through its promise. Ordering
//! comes from the channels' own sequencing, never from a sleep. The system's
//! clipboard is never touched here; the real one is proven under xvfb only
//! (`crates/vilan-cli/tests/gfx_window.rs`).

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use vilan_rt::str_new;

use super::{ClipboardBackend, Make, Worker, set_text, shut_down, text, use_for_tests};

/// A clipboard in memory: what it holds, whether its reads fail, how many
/// were made, and whether it was dropped.
#[derive(Clone, Default)]
struct Fake {
    held: Arc<Mutex<Option<String>>>,
    reads_fail: bool,
    made: Arc<AtomicUsize>,
    dropped: Arc<AtomicBool>,
}

struct FakeClipboard(Fake);

impl ClipboardBackend for FakeClipboard {
    fn text(&mut self) -> Result<Option<String>, String> {
        if self.0.reads_fail {
            return Err("the owner did not answer".to_string());
        }
        Ok(self.0.held.lock().expect("the fake's text").clone())
    }

    fn set_text(&mut self, text: String) -> Result<(), String> {
        *self.0.held.lock().expect("the fake's text") = Some(text);
        Ok(())
    }
}

impl Drop for FakeClipboard {
    fn drop(&mut self) {
        self.0.dropped.store(true, Ordering::SeqCst);
    }
}

impl Fake {
    fn make(&self) -> Make {
        let fake = self.clone();
        Box::new(move || {
            fake.made.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(FakeClipboard(fake)) as Box<dyn ClipboardBackend>)
        })
    }
}

/// A read through the worker, waited for on its own channel.
fn read(worker: &Worker) -> Option<String> {
    let (sender, received) = mpsc::channel();
    worker.read(move |text| sender.send(text).expect("the test waits"));
    received.recv().expect("every read is answered")
}

#[test]
fn a_write_then_a_read_answers_what_was_written() {
    let worker = Worker::start(Fake::default().make());
    assert_eq!(read(&worker), None, "an empty clipboard has no text");
    worker.write("copied".to_string());
    assert_eq!(read(&worker).as_deref(), Some("copied"));
}

#[test]
fn requests_are_taken_in_the_order_they_were_made() {
    let worker = Worker::start(Fake::default().make());
    worker.write("first".to_string());
    let (sender, received) = mpsc::channel();
    let after_first = sender.clone();
    worker.read(move |text| after_first.send(text).expect("waited"));
    worker.write("second".to_string());
    worker.read(move |text| sender.send(text).expect("waited"));
    assert_eq!(received.recv().expect("one").as_deref(), Some("first"));
    assert_eq!(received.recv().expect("two").as_deref(), Some("second"));
}

#[test]
fn with_no_clipboard_to_reach_a_read_answers_none_and_a_write_does_nothing() {
    let worker = Worker::start(Box::new(|| Err("no display".to_string())));
    worker.write("lost".to_string());
    assert_eq!(read(&worker), None);
    assert_eq!(read(&worker), None, "and keeps answering");
    drop(worker);
}

#[test]
fn a_read_that_fails_answers_none_and_the_clipboard_keeps_serving() {
    let fake = Fake {
        reads_fail: true,
        ..Fake::default()
    };
    let worker = Worker::start(fake.make());
    worker.write("kept".to_string());
    assert_eq!(read(&worker), None);
    assert_eq!(
        fake.held.lock().expect("the fake's text").as_deref(),
        Some("kept"),
        "the write after a failure still landed"
    );
}

#[test]
fn the_clipboard_is_made_once_at_the_first_request_and_dropped_on_its_thread_with_the_worker() {
    let fake = Fake::default();
    let worker = Worker::start(fake.make());
    assert_eq!(
        fake.made.load(Ordering::SeqCst),
        0,
        "nothing asked, nothing made"
    );
    worker.write("a".to_string());
    assert_eq!(read(&worker).as_deref(), Some("a"));
    assert_eq!(read(&worker).as_deref(), Some("a"));
    assert_eq!(
        fake.made.load(Ordering::SeqCst),
        1,
        "one object for the process"
    );
    assert!(!fake.dropped.load(Ordering::SeqCst));
    drop(worker);
    assert!(
        fake.dropped.load(Ordering::SeqCst),
        "dropped, on its thread, before the worker's drop returned"
    );
}

/// A program that never touches the clipboard never makes one: no X11
/// connection, no serving thread of `arboard`'s. Proven by the join (a
/// clipboard made eagerly would have been made before the thread ended).
#[test]
fn a_worker_never_asked_never_makes_a_clipboard() {
    let fake = Fake::default();
    drop(Worker::start(fake.make()));
    assert_eq!(fake.made.load(Ordering::SeqCst), 0);
}

#[test]
fn the_extern_answers_through_its_promise_and_the_shut_down_drops_the_clipboard() {
    let fake = Fake::default();
    use_for_tests(fake.make());
    set_text(&str_new("pasted"));
    let answered = vilan_rt::executor::block_on(async { text().await });
    assert_eq!(answered.as_deref(), Some("pasted"));
    shut_down();
    assert!(fake.dropped.load(Ordering::SeqCst));
}
