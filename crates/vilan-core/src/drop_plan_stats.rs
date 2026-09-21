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
    static ASKED: Cell<usize> = const { Cell::new(0) };
    static NOMINALS: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
    static LITERAL_EVIDENCE: Cell<usize> = const { Cell::new(0) };
}

/// B365: how many times the enrolment gate's STRUCT-LITERAL arm was the thing
/// that answered "this body reaches a resource".
///
/// The arm compared the literal's OWN expression id against the set of struct
/// DEFINITION ids, so it answered `false` for every program ever compiled — a
/// vacuous arm, invisible because the recorded-type check above it catches the
/// common case. The live hole is a literal whose type the solver has not
/// recorded at the gate, and the only honest way to pin the repair is to count
/// the arm: a pin over a program where nothing else can supply the evidence
/// reads zero before the fix and non-zero after.
pub(crate) fn note_literal_evidence() {
    LITERAL_EVIDENCE.with(|cell| cell.set(cell.get().saturating_add(1)));
}

/// [`note_literal_evidence`]'s count for the last analysis on this thread.
pub fn literal_evidence() -> usize {
    LITERAL_EVIDENCE.with(Cell::get)
}

/// Zero the literal-evidence counter — called where the planner starts, so the
/// number a pin reads belongs to the analysis it just ran.
pub(crate) fn reset_literal_evidence() {
    LITERAL_EVIDENCE.with(|cell| cell.set(0));
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

/// M19 T1c: how many bodies the ENROLMENT GATE actually walked.
///
/// [`planned_roots`] is M28's number — how much of the program the planner
/// walked once it had been told which bodies to walk. This is the number the
/// gate itself costs, and it is the drop planner's whole price on a program
/// that reaches almost no resource: the gate runs a subtree walk to
/// EXHAUSTION in every body that has no resource evidence in it, which is
/// nearly all of them. A reused module serves its answer from the record
/// instead (M19 T1c), so on a warm analysis this collapses to the bodies the
/// entry brought.
pub(crate) fn record_gate(asked: usize) {
    ASKED.with(|cell| cell.set(asked));
}

/// The bodies the enrolment gate walked in the last analysis on this thread.
pub fn asked_roots() -> usize {
    ASKED.with(Cell::get)
}

/// M49: the last analysis's resource-reaching nominal fingerprints, `(world,
/// entry)` — the two halves `Analyzer::drop_nominals_fingerprints` splits the
/// set into.
///
/// The world half is the enrolment record's restore condition; the entry half
/// is in no condition at all and exists so a pin can state the split as the
/// property it is, rather than as its consequence: two entries of one package
/// that differ in a `resource` declaration agree on the world half and differ
/// on the entry half, and the second one's enrolment is restored anyway.
pub(crate) fn record_nominals(world: u64, entry: u64) {
    NOMINALS.with(|cell| cell.set((world, entry)));
}

/// The `(world, entry)` fingerprints of the last analysis on this thread (M49).
pub fn nominals_fingerprints() -> (u64, u64) {
    NOMINALS.with(Cell::get)
}
