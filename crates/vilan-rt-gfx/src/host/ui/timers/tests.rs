//! Pins of the element timers' host half (F176) and of the window's focus:
//! the externs over the executor's real loop, turned by hand as the shell
//! turns it. No pin waits for time to pass: a timer that must not fire is set
//! a minute out, one that must fire is due at once, and a deadline is held
//! between bounds that follow from the order of the calls alone.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use vilan_rt::executor::{Finished, Turn, turn};
use vilan_rt::str_new;

use super::super::{Node, add_child, deliver, node, on, remove, window_focused};
use super::{advance_clock, cancel_timer, covering_milliseconds, timer, timer_count};
use crate::host::input::FOCUS_CHANGED;
use crate::host::shell::{deadline_reached, wait_for};

const MINUTE: f64 = 60_000.0;

fn logging(log: &Rc<RefCell<Vec<String>>>, name: &str) -> Rc<dyn Fn()> {
    let log = Rc::clone(log);
    let name = name.to_string();
    Rc::new(move || log.borrow_mut().push(name.clone()))
}

fn row() -> Node {
    node(0)
}

/// The deadline a hosted turn leaves, or why it has none.
fn deadline_after_a_turn() -> Option<Instant> {
    match turn() {
        Turn::Idle { deadline, .. } => deadline,
        Turn::Finished(Finished::Drained) => None,
        other => panic!("an unexpected turn: {other:?}"),
    }
}

#[test]
fn with_no_timer_nothing_is_armed_and_the_loop_waits_for_nothing() {
    assert_eq!(deadline_after_a_turn(), None);
    let element = row();
    let id = timer(&element, MINUTE, -1.0, Rc::new(|| {}));
    assert!(deadline_after_a_turn().is_some());
    cancel_timer(id);
    assert_eq!(
        deadline_after_a_turn(),
        None,
        "the last timer cancelled leaves nothing armed: an idle window wakes zero times"
    );
    assert_eq!(wait_for(None, None), winit::event_loop::ControlFlow::Wait);
}

#[test]
fn many_timers_are_one_deadline_at_the_earliest() {
    let element = row();
    let before = Instant::now();
    timer(&element, 3.0 * MINUTE, -1.0, Rc::new(|| {}));
    let earliest = timer(&element, MINUTE, -1.0, Rc::new(|| {}));
    timer(&element, 2.0 * MINUTE, MINUTE, Rc::new(|| {}));
    let deadline = deadline_after_a_turn().expect("one deadline");
    let minute = Duration::from_secs(60);
    assert!(deadline >= before + minute, "never before the earliest");
    assert!(
        deadline <= Instant::now() + minute + Duration::from_millis(1),
        "the earliest, not a later one"
    );
    cancel_timer(earliest);
    let next = deadline_after_a_turn().expect("the next earliest");
    assert!(next >= before + 2 * minute);
}

#[test]
fn a_turn_fires_the_due_timer_in_it_and_not_the_one_a_minute_out() {
    let element = row();
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    timer(&element, MINUTE, -1.0, logging(&log, "a minute out"));
    timer(&element, 0.0, -1.0, logging(&log, "due now"));
    assert!(
        log.borrow().is_empty(),
        "a timer never fires inside its own setting"
    );
    let deadline = deadline_after_a_turn();
    assert_eq!(*log.borrow(), ["due now"]);
    assert!(deadline.is_some(), "the minute's timer is still armed");
}

#[test]
fn removing_the_element_for_good_disarms_its_timers() {
    let parent = node(1);
    let child = row();
    add_child(&parent, &child);
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    timer(&child, 0.0, 5.0, logging(&log, "child"));
    remove(&child);
    assert_eq!(deadline_after_a_turn(), None);
    assert!(log.borrow().is_empty());
}

#[test]
fn a_cancelled_repeating_timer_fires_no_more() {
    let element = row();
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let id = timer(&element, 0.0, MINUTE, logging(&log, "tick"));
    let _ = deadline_after_a_turn();
    assert_eq!(*log.borrow(), ["tick"]);
    cancel_timer(id);
    cancel_timer(id);
    assert_eq!(deadline_after_a_turn(), None);
    assert_eq!(*log.borrow(), ["tick"]);
}

/// The shell turns the program at the end of a wait only when the deadline
/// is reached: a drain wake, or a wake before the deadline, is no timer's.
#[test]
fn a_wake_owes_a_turn_only_at_or_after_the_deadline() {
    let now = Instant::now();
    let second = Duration::from_secs(1);
    assert!(!deadline_reached(None, now), "no deadline: no turn");
    assert!(!deadline_reached(Some(now + second), now));
    assert!(deadline_reached(Some(now), now));
    assert!(deadline_reached(Some(now), now + second));
}

#[test]
fn the_executor_timer_covers_the_wait_in_whole_milliseconds_rounded_up() {
    let ms = Duration::from_millis;
    assert_eq!(covering_milliseconds(ms(10), ms(0)), 10);
    assert_eq!(
        covering_milliseconds(ms(10) + Duration::from_micros(1), ms(0)),
        11
    );
    assert_eq!(covering_milliseconds(ms(10), ms(25)), 0, "already due");
    assert_eq!(
        covering_milliseconds(Duration::from_secs(u64::MAX / 4), ms(0)),
        i32::MAX
    );
}

/// F176: the window's focus is told to every element listening, in tree
/// order, as the current event; and read back afterwards.
#[test]
fn a_focus_change_is_told_to_every_listener_in_tree_order_and_read_back() {
    let root = node(1);
    let first = row();
    let second = row();
    add_child(&root, &first);
    add_child(&root, &second);
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    for (element, name) in [(&second, "second"), (&root, "root"), (&first, "first")] {
        let log = Rc::clone(&log);
        let name = name.to_string();
        on(
            element,
            13,
            Rc::new(move || {
                let kind = crate::host::event_current();
                let focused = crate::host::event_number(0);
                log.borrow_mut().push(format!("{name} {kind} {focused}"));
            }),
        );
    }
    assert_eq!(
        window_focused(&first),
        0,
        "a tree in no window is unfocused"
    );
    deliver(&root, FOCUS_CHANGED, 1.0, 0.0, 0.0, 0.0, 0.0, &str_new(""));
    assert_eq!(
        *log.borrow(),
        ["root 11 1", "first 11 1", "second 11 1"],
        "every listener, the root first, then its children in order"
    );
    assert_eq!(window_focused(&second), 1);
    deliver(&root, FOCUS_CHANGED, 0.0, 0.0, 0.0, 0.0, 0.0, &str_new(""));
    assert_eq!(window_focused(&first), 0);
    assert_eq!(log.borrow().len(), 6);
}

/// A program that drives its tree itself stops the timers' clock and steps
/// it: each timer fires at its own time on the way, in order, a repeating one
/// at every period it crosses, a timer set by a handler on the way when it
/// falls due by the end; and nothing is armed in the executor meanwhile, so
/// no timer wakes the program or keeps it running.
#[test]
fn a_stopped_clock_fires_only_as_the_program_steps_it() {
    let element = row();
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    advance_clock(0.0);
    timer(&element, 530.0, 530.0, logging(&log, "blink"));
    let chained = {
        let log = Rc::clone(&log);
        // On an element that outlives the handler: a timer on one the handler
        // alone held would end with it.
        let element = element.clone();
        Rc::new(move || {
            log.borrow_mut().push("once".to_string());
            timer(&element, 0.0, -1.0, logging(&log, "set by once"));
        })
    };
    timer(&element, 100.0, -1.0, chained);
    assert_eq!(timer_count(), 2);
    assert_eq!(
        deadline_after_a_turn(),
        None,
        "stopped: nothing armed, nothing wakes"
    );
    advance_clock(99.0);
    assert!(log.borrow().is_empty(), "not yet due");
    advance_clock(1.0);
    assert_eq!(*log.borrow(), ["once", "set by once"]);
    advance_clock(429.0);
    assert_eq!(log.borrow().len(), 2);
    advance_clock(1.0);
    assert_eq!(log.borrow().last().map(String::as_str), Some("blink"));
    advance_clock(1060.0);
    assert_eq!(
        *log.borrow(),
        ["once", "set by once", "blink", "blink", "blink"],
        "every period crossed, none skipped"
    );
    assert_eq!(timer_count(), 1);
    assert_eq!(deadline_after_a_turn(), None);
}
