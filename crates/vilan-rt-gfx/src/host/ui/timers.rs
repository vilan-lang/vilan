//! `std::ui`'s element timers on the program's side (F176): the
//! `__ui_timer` and `__ui_cancel_timer` externs over the tree's timers
//! (`ui/timers.rs`, where the rules are), and the WAIT for them.
//!
//! # One executor timer
//!
//! The tree keeps every element's timers; the host keeps at most ONE timer
//! of the executor's (`vilan_rt::executor::timer`), armed for the earliest
//! due time of them all and armed again whenever that time changes (a timer
//! set, cancelled, fired, or gone with its element). Its firing runs a pass
//! ([`fire`]) inside a turn, in a task of its own, as a `std::time::Timer`
//! would: so element timers fire in the program's own loop on both of its
//! loops — the window system's (the host's `shell.rs`), where the executor's
//! earliest deadline is the turn's `Turn::Idle { deadline }` and so already
//! one of `wait_for`'s, and the owned loop of a program with no display.
//! Built on the executor's deadline rather than beside it, the wake rules
//! hold without a second wait in the shell: with no timer nothing is armed
//! and an idle window wakes zero times; many timers are one deadline; and the
//! shell turns the program at a deadline only when the deadline is reached
//! (`shell.rs`'s `deadline_reached`).
//!
//! # Time
//!
//! Due times are read from the host's one clock (`input::host_clock`, the
//! clock a press's time is stamped by). The executor's timer is armed for
//! the WHOLE milliseconds that cover the wait (rounded up), so when it fires
//! the clock has reached the due time and the pass finds the timer due.
//!
//! An armed timer keeps the program running, as a pending `std::time::Timer`
//! does: a repeating timer on an element that stays is a program that does
//! not end by itself (a window's is ended by its close; a termination ends it
//! whatever is armed).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use vilan_rt::executor::{TimerHandle, spawn, timer as executor_timer};
use vilan_rt::panic_with;

use super::{Node, with_tree};
use crate::host::input::host_clock;
use crate::ui::{Handler, TimerId};

thread_local! {
    /// The executor timer armed for the earliest due time, if any.
    static ARMED: RefCell<Option<Armed>> = const { RefCell::new(None) };
}

struct Armed {
    due: Duration,
    handle: TimerHandle,
}

/// `__ui_timer(node, delay, period, handler)` — a timer on the element:
/// `handler` called `delay` milliseconds from now, and again every `period`
/// milliseconds after that when `period` is not negative (at least
/// `ui::MIN_PERIOD`). Answers the timer's id for `__ui_cancel_timer`.
pub fn timer(node: &Node, delay: f64, period: f64, handler: Rc<dyn Fn()>) -> f64 {
    let delay = milliseconds(delay, "delay");
    let period = (period >= 0.0 || period.is_nan()).then(|| milliseconds(period, "period"));
    let added = with_tree(|tree| {
        tree.add_timer(
            node.id(),
            host_clock(),
            delay,
            period,
            Handler::from_rc(handler),
        )
    });
    let id = match added {
        Ok(id) => id.number(),
        // A gone element's: dropped here, with the tree released.
        Err(handler) => {
            drop(handler);
            0.0
        }
    };
    rearm();
    id
}

/// `__ui_cancel_timer(id)` — the timer never fires again. A timer that
/// already fired, was cancelled, or went with its element: nothing happens.
pub fn cancel_timer(id: f64) {
    let handler = TimerId::from_number(id).and_then(|id| with_tree(|tree| tree.cancel_timer(id)));
    drop(handler);
    rearm();
}

/// A number of milliseconds as a duration: a negative one is none (a delay
/// already past fires at the next pass, as `std::time` does); one that is no
/// number at all is the program's mistake.
fn milliseconds(value: f64, what: &str) -> Duration {
    if !value.is_finite() {
        panic_with(&format!(
            "a `std::ui` timer's {what} must be a finite number of milliseconds, not {value}"
        ));
    }
    Duration::from_secs_f64(value.max(0.0) / 1000.0)
}

/// Arms the executor's timer for the earliest due time of the tree's timers,
/// when that is not the time it is armed for: cancelling the one armed
/// before, and arming none when no timer is left.
pub(crate) fn rearm() {
    let due = with_tree(|tree| tree.next_timer_due());
    let old = ARMED.with(|armed| {
        let mut armed = armed.borrow_mut();
        if armed.as_ref().map(|armed| armed.due) == due {
            return None;
        }
        let old = armed.take();
        if let Some(due) = due {
            let handle = executor_timer(covering_milliseconds(due, host_clock()));
            let waited = handle.clone();
            spawn(
                async move {
                    if waited.wait(None).await {
                        fire();
                    }
                },
                "std::ui timer",
            );
            *armed = Some(Armed { due, handle });
        }
        old
    });
    // Cancelled with nothing borrowed: its waiting task ends with `false`.
    if let Some(old) = old {
        old.handle.cancel();
    }
}

/// The whole milliseconds from `now` that reach `due`: rounded UP, so a
/// timer armed for them never fires before `due` on the same clock.
pub(crate) fn covering_milliseconds(due: Duration, now: Duration) -> i32 {
    let wait = due.saturating_sub(now).as_nanos().div_ceil(1_000_000);
    i32::try_from(wait).unwrap_or(i32::MAX)
}

/// The executor's timer fired: one pass over the tree's timers due now (the
/// rules are `ui/timers.rs`'s), each handler called with nothing of the tree
/// borrowed, and the next due time armed — on the way out of a failing
/// handler too.
fn fire() {
    struct ArmAfter;
    impl Drop for ArmAfter {
        fn drop(&mut self) {
            rearm();
        }
    }
    // This is the armed timer firing: nothing is armed now.
    ARMED.with(|armed| armed.borrow_mut().take());
    let _arm_after = ArmAfter;
    let now = host_clock();
    let mark = with_tree(|tree| tree.timer_mark());
    while let Some(handler) = with_tree(|tree| tree.pop_due_timer(now, mark)) {
        handler.call();
        drop(handler);
    }
}

#[cfg(test)]
mod tests;
