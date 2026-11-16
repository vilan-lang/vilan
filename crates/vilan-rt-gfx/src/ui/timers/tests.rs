//! Pins of the element timers (F176), with the time handed in: no clock is
//! read and nothing sleeps. Each pin's planted bug is named beside it.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use super::super::{ElementKind, Handler, UiTree};
use super::{MIN_PERIOD, TimerId, next_due};

fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}

/// A handler that writes `name` into `log` when called.
fn logging(log: &Rc<RefCell<Vec<String>>>, name: &str) -> Handler {
    let log = Rc::clone(log);
    let name = name.to_string();
    Handler::new(move || log.borrow_mut().push(name.clone()))
}

/// Runs one pass at `now` as the host does: every due timer of the pass, one
/// at a time, each called with the tree free.
fn pass(tree: &mut UiTree, now: Duration) {
    let mark = tree.timer_mark();
    while let Some(handler) = tree.pop_due_timer(now, mark) {
        handler.call();
    }
}

#[test]
fn a_timer_fires_once_at_its_time_and_never_before() {
    let mut tree = UiTree::new();
    let node = tree.create(ElementKind::Row);
    let log = Rc::default();
    tree.add_timer(node, ms(10), ms(50), None, logging(&log, "fired"))
        .expect("a live element takes a timer");
    assert_eq!(tree.next_timer_due(), Some(ms(60)));
    pass(&mut tree, ms(59));
    assert!(log.borrow().is_empty(), "one millisecond early is early");
    pass(&mut tree, ms(60));
    assert_eq!(*log.borrow(), ["fired"]);
    pass(&mut tree, ms(500));
    assert_eq!(*log.borrow(), ["fired"], "a one-shot timer fires once");
    assert_eq!(tree.timer_count(), 0);
    assert_eq!(tree.next_timer_due(), None);
}

#[test]
fn a_cancelled_timer_never_fires_and_hands_its_handler_back() {
    let mut tree = UiTree::new();
    let node = tree.create(ElementKind::Row);
    let log = Rc::default();
    let id = tree
        .add_timer(node, ms(0), ms(20), None, logging(&log, "fired"))
        .unwrap();
    assert!(
        tree.cancel_timer(id).is_some(),
        "the handler comes back to drop"
    );
    assert_eq!(tree.next_timer_due(), None, "nothing left to wake for");
    pass(&mut tree, ms(1000));
    assert!(log.borrow().is_empty());
    assert!(
        tree.cancel_timer(id).is_none(),
        "cancelling twice is nothing"
    );
}

#[test]
fn removing_an_element_for_good_ends_its_subtrees_timers_and_no_other() {
    let mut tree = UiTree::new();
    let root = tree.create(ElementKind::Column);
    let row = tree.create(ElementKind::Row);
    let inner = tree.create(ElementKind::Text);
    let sibling = tree.create(ElementKind::Row);
    tree.add_child(root, row).unwrap();
    tree.add_child(row, inner).unwrap();
    tree.add_child(root, sibling).unwrap();
    let log = Rc::default();
    for (node, name) in [(row, "row"), (inner, "inner"), (sibling, "sibling")] {
        tree.add_timer(node, ms(0), ms(30), None, logging(&log, name))
            .unwrap();
    }
    let handlers = tree.remove(row);
    assert_eq!(
        handlers.len(),
        2,
        "the row's and the text's timers leave with the subtree's handlers"
    );
    drop(handlers);
    pass(&mut tree, ms(30));
    assert_eq!(*log.borrow(), ["sibling"]);
}

#[test]
fn an_element_freed_for_want_of_a_handle_drops_its_timers() {
    let mut tree = UiTree::new();
    let node = tree.create(ElementKind::Row);
    let log = Rc::default();
    tree.add_timer(node, ms(0), ms(30), Some(ms(30)), logging(&log, "fired"))
        .unwrap();
    // No parent and now no handle: freed, with its timers.
    tree.release(node);
    assert_eq!(tree.timer_count(), 0);
    assert_eq!(tree.next_timer_due(), None);
    pass(&mut tree, ms(1000));
    assert!(log.borrow().is_empty());
}

#[test]
fn a_gone_element_refuses_a_timer_and_hands_the_handler_back() {
    let mut tree = UiTree::new();
    let node = tree.create(ElementKind::Row);
    tree.release(node);
    let log = Rc::default();
    assert!(
        tree.add_timer(node, ms(0), ms(10), None, logging(&log, "x"))
            .is_err()
    );
    assert_eq!(tree.timer_count(), 0);
}

#[test]
fn a_repeating_timer_keeps_its_phase_and_skips_the_periods_a_late_pass_missed() {
    let mut tree = UiTree::new();
    let node = tree.create(ElementKind::Row);
    let log = Rc::default();
    tree.add_timer(node, ms(0), ms(100), Some(ms(100)), logging(&log, "tick"))
        .unwrap();
    // Late by 30 ms: the next is still due at 200, not at 230.
    pass(&mut tree, ms(130));
    assert_eq!(tree.next_timer_due(), Some(ms(200)));
    // Late by more than two whole periods: one call, then the next step of
    // the phase after now — no burst for the periods missed.
    pass(&mut tree, ms(450));
    assert_eq!(log.borrow().len(), 2);
    assert_eq!(tree.next_timer_due(), Some(ms(500)));
    // Exactly on a step: due again one period on.
    pass(&mut tree, ms(500));
    assert_eq!(tree.next_timer_due(), Some(ms(600)));
    assert_eq!(log.borrow().len(), 3);
}

#[test]
fn the_next_due_rule_by_itself() {
    assert_eq!(next_due(ms(100), ms(100), ms(100)), ms(200));
    assert_eq!(next_due(ms(100), ms(100), ms(199)), ms(200));
    assert_eq!(next_due(ms(100), ms(100), ms(200)), ms(300));
    assert_eq!(next_due(ms(100), ms(100), ms(450)), ms(500));
}

#[test]
fn a_period_below_the_floor_runs_at_the_floor() {
    let mut tree = UiTree::new();
    let node = tree.create(ElementKind::Row);
    let log = Rc::default();
    tree.add_timer(node, ms(0), ms(0), Some(ms(0)), logging(&log, "tick"))
        .unwrap();
    pass(&mut tree, ms(0));
    assert_eq!(tree.next_timer_due(), Some(MIN_PERIOD));
}

#[test]
fn timers_due_in_one_pass_fire_by_due_time_then_by_the_order_they_were_set() {
    let mut tree = UiTree::new();
    let node = tree.create(ElementKind::Row);
    let log = Rc::default();
    tree.add_timer(node, ms(0), ms(50), None, logging(&log, "late"))
        .unwrap();
    tree.add_timer(node, ms(0), ms(20), None, logging(&log, "early, first"))
        .unwrap();
    tree.add_timer(node, ms(0), ms(20), None, logging(&log, "early, second"))
        .unwrap();
    pass(&mut tree, ms(60));
    assert_eq!(*log.borrow(), ["early, first", "early, second", "late"]);
}

#[test]
fn many_timers_are_one_deadline_at_the_earliest_and_none_is_no_deadline() {
    let mut tree = UiTree::new();
    let node = tree.create(ElementKind::Row);
    assert_eq!(
        tree.next_timer_due(),
        None,
        "an idle tree waits for nothing"
    );
    for delay in [300, 40, 900, 41] {
        tree.add_timer(node, ms(5), ms(delay), None, Handler::new(|| {}))
            .unwrap();
    }
    assert_eq!(tree.next_timer_due(), Some(ms(45)));
}

#[test]
fn a_timer_set_during_a_pass_waits_for_the_next_pass_even_with_no_delay() {
    let tree = Rc::new(RefCell::new(UiTree::new()));
    let node = tree.borrow_mut().create(ElementKind::Row);
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let (inner_tree, inner_log) = (Rc::clone(&tree), Rc::clone(&log));
    let setter = Handler::new(move || {
        inner_log.borrow_mut().push("setter".to_string());
        let again = logging(&inner_log, "set by the setter");
        inner_tree
            .borrow_mut()
            .add_timer(node, ms(10), ms(0), None, again)
            .unwrap();
    });
    tree.borrow_mut()
        .add_timer(node, ms(0), ms(10), None, setter)
        .unwrap();
    let mark = tree.borrow().timer_mark();
    loop {
        let next = tree.borrow_mut().pop_due_timer(ms(10), mark);
        let Some(handler) = next else { break };
        handler.call();
    }
    assert_eq!(*log.borrow(), ["setter"]);
    assert_eq!(tree.borrow().next_timer_due(), Some(ms(10)));
    pass(&mut tree.borrow_mut(), ms(10));
    assert_eq!(*log.borrow(), ["setter", "set by the setter"]);
}

#[test]
fn a_handler_that_cancels_a_later_timer_of_its_pass_stops_it_firing() {
    let tree = Rc::new(RefCell::new(UiTree::new()));
    let node = tree.borrow_mut().create(ElementKind::Row);
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let victim = tree
        .borrow_mut()
        .add_timer(node, ms(0), ms(20), None, logging(&log, "victim"))
        .unwrap();
    let (inner_tree, inner_log) = (Rc::clone(&tree), Rc::clone(&log));
    let canceller = Handler::new(move || {
        inner_log.borrow_mut().push("canceller".to_string());
        drop(inner_tree.borrow_mut().cancel_timer(victim));
    });
    tree.borrow_mut()
        .add_timer(node, ms(0), ms(10), None, canceller)
        .unwrap();
    let mark = tree.borrow().timer_mark();
    loop {
        let next = tree.borrow_mut().pop_due_timer(ms(30), mark);
        let Some(handler) = next else { break };
        handler.call();
    }
    assert_eq!(*log.borrow(), ["canceller"]);
}

#[test]
fn a_timer_id_crosses_as_a_whole_number_and_nothing_else_names_one() {
    let id = TimerId(42);
    assert_eq!(TimerId::from_number(id.number()), Some(id));
    for number in [0.0, -1.0, 1.5, f64::NAN, f64::INFINITY] {
        assert_eq!(TimerId::from_number(number), None, "{number}");
    }
}
