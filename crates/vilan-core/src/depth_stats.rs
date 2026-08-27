//! The `VILAN_DEPTH_STATS` instrument (backlog B138): how deep each of the
//! analyzer's recursive families actually goes, and what that depth costs in
//! stack bytes — measured, not estimated.
//!
//! The stack margins around the compiler (the CLI's and LSP's 256 MiB worker
//! spawns, the wasm build's 64 MiB `-zstack-size`) were sized by incident, not
//! by measurement: the v0.36.0 gate SIGABRT'd when a modest server program's
//! analysis outgrew libtest's ~2 MiB worker (commit 0fb5e5f0). This module is
//! the `VILAN_PHASE_TIMING` treatment for that blindness — set
//! `VILAN_DEPTH_STATS` and every top-level analysis prints one stderr line
//! with, per family, the peak recursion depth and the stack consumed at that
//! peak, so the next stack question is a run, not a hand-patched experiment.
//!
//! Families (the split matters because their frames differ by orders of
//! magnitude — a giant matched-over-every-arm inference frame against a small
//! guard-loop frame):
//!
//! * `infer` — [`DepthFrame`] in `infer_type_inner`: the type-inference
//!   recursion, including its re-entries through the constraint resolvers and
//!   cross-function return inference.
//! * `type-walk` — [`note`] from `util::RecursionGuard::enter`: the guarded
//!   type-graph walks (`reconcile_type`, `substitute_type`, the transformer's
//!   `resolve_type_id`), on the guard's own depth counter.
//! * `expr-walk` — [`DepthFrame`] in `walk_expr_node`: the phase-1 source
//!   walk, whose depth is the program's syntactic nesting.
//! * `pattern` — [`DepthFrame`] in `resolve_pattern`: pattern nesting.
//! * `parse` — [`DepthFrame`] in `parsing::Parser::parse_atom`: the parser's
//!   own recursion, which runs BEFORE any of the above and is not depth-bounded
//!   (B139's residual). It was added when the arithmetic-nesting plants showed
//!   the parser overflowing a stack the analyzer's bounded walk fits in
//!   comfortably — measuring it is what lets the stack margins be argued from
//!   the WHOLE pipeline rather than from the analyzer alone.
//!
//! Bytes are a stack-pointer high-water mark: [`reset`] anchors the address of
//! a local at analysis start, and a new peak records how far below that anchor
//! the stack has grown. The stack grows downward on every target vilan builds
//! for (x86-64, aarch64, wasm32's linear-memory shadow stack), and the
//! MiB-scale answer swamps the frame-layout noise in the anchor. A depth
//! recorded with no anchor (a unit test entering the analyzer sideways)
//! reports 0 bytes rather than garbage — `saturating_sub` is the whole
//! defense.
//!
//! Counting is gated on the variable (unlike the phase clock's unconditional
//! marks): the hooks sit on the analyzer's hottest paths, where even a
//! thread-local touch per expression node is not obviously noise. Off means
//! one cached-`OnceLock` read and a branch. Thread-local like the phase
//! accumulators, because an analysis is single-threaded; macro worlds are
//! nested analyses on the SAME thread, so their depths accumulate into the
//! outer run's peaks and must not [`reset`] them — the guard lives at the
//! reset call site, beside the phase marks.

use std::cell::Cell;

pub(crate) const INFER: usize = 0;
pub(crate) const TYPE_WALK: usize = 1;
pub(crate) const EXPR_WALK: usize = 2;
pub(crate) const PATTERN: usize = 3;
pub(crate) const PARSE: usize = 4;
pub(crate) const FAMILY_COUNT: usize = 5;
const FAMILY_NAMES: [&str; FAMILY_COUNT] =
    ["infer", "type-walk", "expr-walk", "pattern", "parse"];

thread_local! {
    static CURRENT: [Cell<usize>; FAMILY_COUNT] =
        const { [Cell::new(0), Cell::new(0), Cell::new(0), Cell::new(0), Cell::new(0)] };
    static PEAK: [Cell<usize>; FAMILY_COUNT] =
        const { [Cell::new(0), Cell::new(0), Cell::new(0), Cell::new(0), Cell::new(0)] };
    static PEAK_BYTES: [Cell<usize>; FAMILY_COUNT] =
        const { [Cell::new(0), Cell::new(0), Cell::new(0), Cell::new(0), Cell::new(0)] };
    static CALLS: [Cell<u64>; FAMILY_COUNT] =
        const { [Cell::new(0), Cell::new(0), Cell::new(0), Cell::new(0), Cell::new(0)] };
    static BASELINE_SP: Cell<usize> = const { Cell::new(0) };
    /// Whether an analysis is currently anchored — see [`begin`].
    static ANCHORED: Cell<bool> = const { Cell::new(false) };
}

/// Whether `VILAN_DEPTH_STATS` asks for the per-analysis depth line (any value
/// but empty or `0`), cached the way `phase_timing_enabled` is.
pub(crate) fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("VILAN_DEPTH_STATS").is_ok_and(|value| !value.is_empty() && value != "0")
    })
}

/// The current stack position, approximately: the address of a fresh local.
/// `inline(never)` so the local has a frame of its own to live in; where the
/// address falls within that frame is noise at the MiB scale being measured.
#[inline(never)]
fn approximate_sp() -> usize {
    let marker = 0u8;
    std::ptr::addr_of!(marker) as usize
}

/// One level of a counted recursion, RAII like `RecursionGuard`: hold it for
/// the body, drop restores the depth on every exit path. Inert when the
/// instrument is off.
pub(crate) struct DepthFrame {
    /// The family index, or `FAMILY_COUNT` for the inert (instrument-off) frame.
    family: usize,
}

impl DepthFrame {
    #[inline]
    pub(crate) fn enter(family: usize) -> DepthFrame {
        if !enabled() {
            return DepthFrame {
                family: FAMILY_COUNT,
            };
        }
        let depth = CURRENT.with(|cells| {
            let cell = &cells[family];
            let depth = cell.get() + 1;
            cell.set(depth);
            depth
        });
        note(family, depth);
        DepthFrame { family }
    }
}

impl Drop for DepthFrame {
    fn drop(&mut self) {
        if self.family < FAMILY_COUNT {
            CURRENT.with(|cells| {
                let cell = &cells[self.family];
                cell.set(cell.get().saturating_sub(1));
            });
        }
    }
}

/// Records `depth` as the family's high-water mark if it is one, along with
/// the stack consumed at that moment. `RecursionGuard` reports its own counter
/// through here; the [`DepthFrame`] sites arrive via [`DepthFrame::enter`].
#[inline]
pub(crate) fn note(family: usize, depth: usize) {
    if !enabled() {
        return;
    }
    CALLS.with(|calls| {
        let cell = &calls[family];
        cell.set(cell.get().saturating_add(1));
    });
    PEAK.with(|peaks| {
        let peak = &peaks[family];
        if depth > peak.get() {
            peak.set(depth);
            let consumed = BASELINE_SP.with(Cell::get).saturating_sub(approximate_sp());
            PEAK_BYTES.with(|bytes| bytes[family].set(consumed));
        }
    });
}

/// Anchors this analysis: zeroes the peaks and takes the stack baseline, but
/// only for the FIRST caller — [`report`] releases the anchor again.
///
/// There are two pipelines into an analysis: `analyze_source` (the LSP, wasm,
/// the test harnesses) parses and then calls `analyze`, while the CLI calls
/// `analyze` itself with a tree it parsed. Both must anchor, and whichever runs
/// first must WIN: the parser recurses before `analyze` is entered, so an
/// unconditional re-anchor inside `analyze` would zero the parse family's peak
/// on exactly the pipeline that can see it. Macro worlds must not anchor at all
/// (their depths belong to the outer run) — the guard stays at the call sites,
/// beside the phase marks.
///
/// `CURRENT` is left alone: it is RAII-balanced, and zeroing it here would
/// corrupt any counted frame legitimately still open on this thread.
pub(crate) fn begin() {
    if !enabled() || ANCHORED.with(Cell::get) {
        return;
    }
    ANCHORED.with(|anchored| anchored.set(true));
    PEAK.with(|peaks| peaks.iter().for_each(|cell| cell.set(0)));
    PEAK_BYTES.with(|bytes| bytes.iter().for_each(|cell| cell.set(0)));
    CALLS.with(|calls| calls.iter().for_each(|cell| cell.set(0)));
    BASELINE_SP.with(|cell| cell.set(approximate_sp()));
}

/// The one-line report: `[vilan depth]`-prefixed like the phase line, stderr
/// for the same reason (`build --stdout`'s JavaScript must stay clean). Per
/// family, two whitespace-separated values: the peak depth, then the stack
/// consumed at that peak as `MiB` — e.g. `infer 214 3.02MiB`. The bytes are
/// per-family totals from the analysis baseline, not per-frame costs, and the
/// families overlap on the stack (inference runs inside constraint resolution,
/// display inside inference), so the line's largest MiB figure — not the sum —
/// approximates the analysis's real stack need.
pub(crate) fn report() {
    if !enabled() {
        return;
    }
    ANCHORED.with(|anchored| anchored.set(false));
    let peaks = PEAK.with(|peaks| peaks.each_ref().map(Cell::get));
    let bytes = PEAK_BYTES.with(|bytes| bytes.each_ref().map(Cell::get));
    let calls = CALLS.with(|calls| calls.each_ref().map(Cell::get));
    let mut line = String::from("[vilan depth]");
    for family in 0..FAMILY_COUNT {
        line.push_str(&format!(
            " {} {} {:.2}MiB x{}",
            FAMILY_NAMES[family],
            peaks[family],
            bytes[family] as f64 / (1024.0 * 1024.0),
            calls[family],
        ));
    }
    eprintln!("{line}");
}
