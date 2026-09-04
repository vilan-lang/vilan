//! The drop planner's enrolment counter (M28) — how many scan roots
//! `Analyzer::plan_resource_drops` actually walked in the last analysis, and
//! how many it was offered.
//!
//! Drop planning used to be gated WHOLE-PROGRAM: one `resource` declaration
//! anywhere (std has several) turned the per-body walk on for every body in the
//! program. M28 replaced that gate with a per-body predicate
//! (`Analyzer::resource_reaching_roots`), and the thing a pin has to see is
//! exactly this: that a program with one resource type and a thousand
//! resource-free functions PLANS ONLY the functions that reach it. Timing
//! cannot pin that — a counter can.
//!
//! Thread-local, like `depth_stats`: an analysis is single-threaded, and a
//! nested macro world is another analysis on the SAME thread. `record` is
//! written once per analysis, at the end of the planner, so the outermost
//! analysis — the one a caller asked for — is the one whose numbers stand.

use std::cell::Cell;

thread_local! {
    static PLANNED: Cell<usize> = const { Cell::new(0) };
    static OFFERED: Cell<usize> = const { Cell::new(0) };
}

/// Record one analysis's enrolment: `planned` roots walked out of `offered`
/// bodied functions and closures.
pub(crate) fn record(planned: usize, offered: usize) {
    PLANNED.with(|cell| cell.set(planned));
    OFFERED.with(|cell| cell.set(offered));
}

/// The scan roots the drop planner walked in the last analysis on this thread.
pub fn planned_roots() -> usize {
    PLANNED.with(Cell::get)
}

/// The scan roots that analysis offered it — every bodied function and every
/// closure in the loaded world.
pub fn offered_roots() -> usize {
    OFFERED.with(Cell::get)
}
