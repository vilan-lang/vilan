//! Timers on elements (F176, RULED 2026-11-12: vilan code gets a timer, the
//! host repeats no input). A timer belongs to an ELEMENT, as its handlers
//! do: it is kept here, in the tree, with the element's id, and it ends
//! with the element — a final [`UiTree::remove`] takes the subtree's timers
//! out with its handlers, and an element freed for want of a handle and a
//! parent drops its timers with it. A cancelled timer is gone at once.
//! Neither ever fires afterwards.
//!
//! # Time
//!
//! The tree reads no clock. Every operation that needs the time is HANDED
//! it, as a [`Duration`] from a fixed origin (the host's start, the clock a
//! press's time is stamped by): so the pins choose the time, and nothing
//! here sleeps or waits. Waiting is the host's (`host/ui/timers.rs`): it
//! asks [`UiTree::next_timer_due`] and arms ONE executor timer for it,
//! whatever the number of timers.
//!
//! # The rules
//!
//! - A timer fires at its due time or later, never before: a pass at `now`
//!   fires the timers due at or before `now`.
//! - Timers due in one pass fire in the order of their due times, and two due
//!   at the same time in the order they were set (`setTimeout`'s rule).
//! - A pass fires only the timers that existed when it began: one set by a
//!   handler of the pass waits for the next pass, even with no delay, so a
//!   handler that sets a timer cannot keep a pass going.
//! - The pass hands out one handler at a time ([`UiTree::pop_due_timer`]), so
//!   a handler that cancels a later timer of the same pass, or removes its
//!   element, stops it firing.
//! - A REPEATING timer keeps its phase: its next due time is the last one plus
//!   its period, never the moment it happened to fire plus its period, so
//!   lateness does not accumulate. A pass later than a whole period skips the
//!   periods it missed (one call, then the next due time after `now`):
//!   a stalled program is not handed a burst.

use std::collections::BTreeMap;
use std::time::Duration;

use super::{Handler, NodeId, UiTree};

/// The shortest period a repeating timer runs at: a smaller one (zero, say)
/// would fire at every pass.
pub const MIN_PERIOD: Duration = Duration::from_millis(1);

/// A timer's name: unique for the program's life, never reused, so a stale
/// one (a timer that fired, was cancelled or went with its element) names
/// nothing. Its order is the order timers were set in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimerId(u64);

impl TimerId {
    /// The id as the program holds it: a whole number (exact up to 2^53,
    /// which no program reaches).
    pub fn number(self) -> f64 {
        self.0 as f64
    }

    /// The id the program handed back, if it can be one.
    pub fn from_number(number: f64) -> Option<TimerId> {
        (number.is_finite() && number >= 1.0 && number.fract() == 0.0)
            .then_some(TimerId(number as u64))
    }
}

struct Entry {
    node: NodeId,
    due: Duration,
    period: Option<Duration>,
    handler: Handler,
}

/// Every live timer of the program's tree.
#[derive(Default)]
pub(super) struct Timers {
    entries: BTreeMap<TimerId, Entry>,
    /// The last id given out (ids start at 1).
    last: u64,
}

impl Timers {
    /// Takes out every timer of `node`, for the caller to drop.
    pub(super) fn take_node(&mut self, node: NodeId) -> Vec<Handler> {
        if self.entries.is_empty() {
            return Vec::new();
        }
        let ids: Vec<TimerId> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.node == node)
            .map(|(id, _)| *id)
            .collect();
        ids.into_iter()
            .filter_map(|id| self.entries.remove(&id))
            .map(|entry| entry.handler)
            .collect()
    }
}

/// The due time after `due` for a timer of `period` fired at `now`: the next
/// step of its phase that is after `now`.
pub(super) fn next_due(due: Duration, period: Duration, now: Duration) -> Duration {
    let next = due + period;
    if next > now {
        return next;
    }
    // Late by at least a whole period: skip the periods missed.
    let missed = (now - due).as_nanos() / period.as_nanos();
    let steps = u32::try_from(missed + 1).unwrap_or(u32::MAX);
    due + period * steps
}

impl UiTree {
    /// Sets a timer on `node`: `handler` is due `delay` after `now`, and
    /// again every `period` after that when there is one (at least
    /// [`MIN_PERIOD`]). Refused when the element is gone (such a timer would
    /// never fire): the handler comes back, for the caller to drop with the
    /// tree no longer borrowed.
    pub fn add_timer(
        &mut self,
        node: NodeId,
        now: Duration,
        delay: Duration,
        period: Option<Duration>,
        handler: Handler,
    ) -> Result<TimerId, Handler> {
        if !self.is_alive(node) {
            return Err(handler);
        }
        self.timers.last += 1;
        let id = TimerId(self.timers.last);
        self.timers.entries.insert(
            id,
            Entry {
                node,
                due: now + delay,
                period: period.map(|period| period.max(MIN_PERIOD)),
                handler,
            },
        );
        Ok(id)
    }

    /// Cancels a timer: it never fires again. Its handler is answered for the
    /// caller to drop with the tree no longer borrowed (it may hold the last
    /// handle of an element). A timer already gone is no error.
    pub fn cancel_timer(&mut self, id: TimerId) -> Option<Handler> {
        self.timers.entries.remove(&id).map(|entry| entry.handler)
    }

    /// The earliest due time of any timer: when the host must turn the
    /// program next for one. `None` with no timers — an idle program waits
    /// for nothing.
    pub fn next_timer_due(&self) -> Option<Duration> {
        self.timers.entries.values().map(|entry| entry.due).min()
    }

    /// The mark a firing pass begins with: [`UiTree::pop_due_timer`] fires
    /// only timers set before it.
    pub fn timer_mark(&self) -> TimerId {
        TimerId(self.timers.last + 1)
    }

    /// The next timer of the pass begun at `mark` that is due at `now`: the
    /// earliest due, the first set among equals. A one-shot timer leaves the
    /// tree; a repeating one stays, due at its next step (the module's
    /// rules). `None` when no timer of the pass is due.
    pub fn pop_due_timer(&mut self, now: Duration, mark: TimerId) -> Option<Handler> {
        let (id, _) = self
            .timers
            .entries
            .range(..mark)
            .filter(|(_, entry)| entry.due <= now)
            .min_by_key(|(id, entry)| (entry.due, **id))?;
        let id = *id;
        let entry = self.timers.entries.get_mut(&id).expect("the timer found");
        match entry.period {
            Some(period) => {
                entry.due = next_due(entry.due, period, now);
                Some(entry.handler.clone())
            }
            None => self.timers.entries.remove(&id).map(|entry| entry.handler),
        }
    }

    /// How many timers are live: what the pins of a timer's end read.
    pub fn timer_count(&self) -> usize {
        self.timers.entries.len()
    }
}

#[cfg(test)]
mod tests;
