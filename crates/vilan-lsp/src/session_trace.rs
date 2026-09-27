//! E106: the server's own session trace — per-request timing and retained-state
//! cardinalities, logged to the client's output channel.
//!
//! The owner reports the language server "slowing down quite a bit" over a
//! working session. The item asks to MEASURE before designing a reclaim, and the
//! decisive datapoint — a `Vilan: Restart Language Server` that did not help
//! while a VS Code restart did — moved the prime suspicion to the editor side
//! (the extension's half is instrumented there). This is the other half: what a
//! long-lived server process can say about itself.
//!
//! **Timing, and — since E166 — memory.** Every request handler already runs
//! through one synchronous fence (`Backend::fenced`), which makes it the single
//! seam where every request can be timed without touching a handler.
//!
//! This module used to refuse to read process RSS at all, on `leak_tally`'s
//! standing ruling: RSS is dominated by allocator retention from building and
//! dropping a whole `Program` per analysis, which swamps the genuine
//! per-analysis leak and is far too noisy to ATTRIBUTE anything to. That ruling
//! stands for attribution and is the wrong one for a status page. E106 was
//! found by reading RSS beside `mallinfo2`'s in-use/free split — 738 → 320 MB
//! in use with 421 MB retained free after closing eighteen documents — and the
//! field reading that started it was a resident figure off the owner's own
//! server (4.13 GB after eight hours). A page that reports cardinalities and no
//! bytes cannot carry either number, so the next report arrived without them.
//! The split is what makes the reading honest: resident size that stays up
//! while in-use bytes fall is the allocator's to hand back (M64's `malloc_trim`),
//! not the server's to free.
//!
//! What is still NOT here is the leak tally's own balance: it is THREAD-LOCAL
//! and every analysis runs on its own spawned big-stack thread, so read from
//! the request thread it would report that thread's reclaims (a large negative
//! number) and nothing about the session. `crate::document`'s harness reads it
//! where it is recorded, which is the only place it means anything.
//!
//! **What CAN grow without bound is state, and state is countable.** The
//! server's retained maps are its session memory: one entry per open document,
//! per cached semantic-token answer, per manifest, and per line index. A
//! cardinality that climbs while the open-document count does not is a leak with
//! a name, and that is a fact the trace reports directly rather than inferring.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// A single request is called slow past this, and says so by name and duration.
/// Tighter than the extension's own threshold, because this measures the
/// server's work alone with no transport or extension-host scheduling in it.
pub const SLOW_REQUEST_MS: u128 = 250;

/// A full summary — the tally plus the state cardinalities — is emitted every
/// this many requests. Roughly a few minutes of active editing, so a session
/// long enough to feel slow carries several, and their state counts read as a
/// growth curve rather than a single reading.
pub const SUMMARY_EVERY_REQUESTS: u64 = 500;

/// Timings for one request method.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RequestStat {
    pub count: u64,
    pub total_ms: u128,
    pub max_ms: u128,
}

/// The server's retained-state cardinalities at one instant — the counts that
/// answer "is this session accumulating?" without guessing at bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StateSizes {
    pub documents: usize,
    /// M63: how many of those documents still hold a `Program`. Bounded by
    /// `document::RETAINED_PROGRAMS` once the retention rule has run, so a
    /// number above it on a settled session is the rule not holding.
    pub programs: usize,
    pub semantic_token_cache: usize,
    pub manifests: usize,
    pub pending: usize,
    pub line_indices: usize,
}

/// E179: what the ANALYZER's base cache is holding, for the retained-state
/// line — the worlds it is keeping, what they weigh, and the budget that bounds
/// them (M67).
///
/// Its own struct rather than three more [`StateSizes`] fields, and the
/// distinction is not cosmetic: `StateSizes` is the SERVER's own maps, one
/// entry per open document, and the open-edit-close pins compare two of them
/// for equality to prove a session returns to baseline. The base cache is
/// process-global and analyzer-owned — shared with every other analysis in the
/// process, and deliberately NOT emptied by closing a document — so folding it
/// into that tuple would make those comparisons depend on something no
/// document owns.
///
/// Both byte figures are RESIDENT bytes, which is the unit the reader means by
/// "how much memory is this holding": the budget is denominated in the M11
/// tally's recorded bytes internally, and is converted here through the one
/// public factor so the two numbers on the line can be compared by eye
/// (`base_cache_weight` at or just under `base_cache_budget` is the bound
/// working; well over it is M67's eviction not running).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BaseCacheSizes {
    /// How many worlds the cache retains right now (M11).
    pub worlds: usize,
    /// What those worlds weigh, in resident bytes (M50's re-denomination).
    pub weight_bytes: usize,
    /// The bound in force, in the same unit (M24/M67).
    pub budget_bytes: usize,
}

impl BaseCacheSizes {
    /// The cache's own numbers, read at this instant.
    pub fn sample() -> BaseCacheSizes {
        BaseCacheSizes {
            worlds: vilan_core::analyzer::base_cache_retained(),
            weight_bytes: vilan_core::analyzer::base_cache_retained_weight(),
            budget_bytes: vilan_core::analyzer::base_cache_budget()
                .saturating_mul(vilan_core::analyzer::BASE_CACHE_WEIGHT_FACTOR),
        }
    }
}

/// M26: what the analysis scheduler did this session.
///
/// `started` counts every analysis the server began; `landed` those that
/// became the document's analyzed snapshot; `cancelled` those a newer edit
/// stopped part-way (`proposal/editor-latency.md` §4.2). The three do not sum:
/// an analysis that ran to the end and was then dropped by `land`'s revision
/// check — E117's correctness path, which cancellation sits on top of and never
/// replaces — is counted as started and as neither of the others.
///
/// The line it prints is a diagnosis in itself. `cancelled` climbing with
/// `started` while `landed` barely moves is a session typing faster than it can
/// analyze: before M26 that state cost a whole analysis per superseded
/// keystroke, and the number is how the owner sees whether it still does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnalysisCounts {
    pub started: u64,
    pub landed: u64,
    pub cancelled: u64,
    /// **M27**: the EDITOR TABLES each landed analysis built — `lsp-index`
    /// (references, symbols) plus `lsp-landed` (E121's captured walk), summed
    /// and maxed over the session in whole milliseconds.
    ///
    /// A fifth per-keystroke cost, and until this line the only place it was
    /// visible was a `VILAN_PHASE_TIMING` run nobody does in a real session.
    /// It ran 110–292 ms of wall per keystroke on a real browser application
    /// (E126, 2026-09-04) — against `analyze` itself at ~1,100 ms, so it is a
    /// tenth to a quarter of what a keystroke costs and it is proportional to
    /// the PROGRAM rather than to the edited buffer. `max` is the number to
    /// read first: a mean hides the analysis that made the editor wait.
    pub index_total_ms: u128,
    pub index_max_ms: u128,
}

/// The live counters behind [`AnalysisCounts`], incremented from the analysis
/// tasks.
///
/// An owned value the server holds one of, not a bare `static`, for the reason
/// [`RequestTally`] gives: a process-global counter shared across a parallel
/// test runner is the classic flaky measurement, and the cancellation pins read
/// these numbers as assertions.
#[derive(Debug, Default)]
pub struct AnalysisTally {
    started: AtomicU64,
    landed: AtomicU64,
    cancelled: AtomicU64,
    /// M27, in whole milliseconds — see [`AnalysisCounts::index_total_ms`].
    index_total_ms: AtomicU64,
    index_max_ms: AtomicU64,
}

impl AnalysisTally {
    pub fn record_started(&self) {
        self.started.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_landed(&self) {
        self.landed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_cancelled(&self) {
        self.cancelled.fetch_add(1, Ordering::Relaxed);
    }

    /// M27: one landed analysis's editor-table cost. Called beside
    /// [`record_landed`](AnalysisTally::record_landed) — a cancelled or
    /// dropped analysis built no tables the editor ever read, and counting one
    /// would report a cost the user never waited for.
    ///
    /// `fetch_max` rather than a compare-exchange loop: the two counters are a
    /// diagnostic, and the ordering between them is not a fact anything
    /// asserts on.
    pub fn record_index(&self, elapsed: std::time::Duration) {
        let milliseconds = elapsed.as_millis().min(u128::from(u64::MAX)) as u64;
        self.index_total_ms
            .fetch_add(milliseconds, Ordering::Relaxed);
        self.index_max_ms.fetch_max(milliseconds, Ordering::Relaxed);
    }

    pub fn counts(&self) -> AnalysisCounts {
        AnalysisCounts {
            started: self.started.load(Ordering::Relaxed),
            landed: self.landed.load(Ordering::Relaxed),
            cancelled: self.cancelled.load(Ordering::Relaxed),
            index_total_ms: u128::from(self.index_total_ms.load(Ordering::Relaxed)),
            index_max_ms: u128::from(self.index_max_ms.load(Ordering::Relaxed)),
        }
    }
}

/// What the caller should log after recording one request, if anything.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TraceEvent {
    /// Nothing to say — the common case, so a quiet session logs nothing.
    Quiet,
    /// One request crossed [`SLOW_REQUEST_MS`].
    Slow(String),
    /// The request count reached a summary boundary; the caller supplies the
    /// state sizes and calls [`RequestTally::summary`].
    Summarize,
}

/// The per-method timing tally.
///
/// An owned value rather than a bare global so the pins below exercise their own
/// instance: the shipped server holds one in a `static`, and a process-global
/// counter shared across a parallel test runner is the classic flaky measurement
/// (`leak_tally`'s "E12 pointer-identity lesson").
#[derive(Debug, Default)]
pub struct RequestTally {
    methods: BTreeMap<&'static str, RequestStat>,
    total_requests: u64,
}

impl RequestTally {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold one round trip in, and say whether it is worth logging.
    ///
    /// The summary boundary is checked on the TOTAL request count, not per
    /// method, so the emission rate is the session's own pace whatever mix of
    /// requests the editor happens to be sending.
    pub fn record(&mut self, request: &'static str, elapsed_ms: u128) -> TraceEvent {
        let stat = self.methods.entry(request).or_default();
        stat.count += 1;
        stat.total_ms += elapsed_ms;
        stat.max_ms = stat.max_ms.max(elapsed_ms);
        let (count, max_ms) = (stat.count, stat.max_ms);
        self.total_requests += 1;

        if elapsed_ms >= SLOW_REQUEST_MS {
            return TraceEvent::Slow(format!(
                "slow request: {request} took {elapsed_ms} ms \
                 (request {count} of this method; slowest so far {max_ms} ms)"
            ));
        }
        if self.total_requests.is_multiple_of(SUMMARY_EVERY_REQUESTS) {
            return TraceEvent::Summarize;
        }
        TraceEvent::Quiet
    }

    // The two read-only views onto the tally, driven by this module's own pins.
    // The server never reads either — it only ever `record`s and asks for a
    // summary — so they are compiled with the tests that use them rather than
    // carried dead in the shipped binary.
    #[cfg(test)]
    pub fn total_requests(&self) -> u64 {
        self.total_requests
    }

    #[cfg(test)]
    pub fn stat(&self, request: &str) -> Option<RequestStat> {
        self.methods.get(request).copied()
    }

    /// The session summary: what the server is spending its time on, and what it
    /// is holding on to.
    ///
    /// Methods are ordered by TOTAL time descending — the order that answers
    /// "what is this session waiting on", where a per-call mean would hide a
    /// cheap request being sent thousands of times (semantic tokens on every
    /// keystroke is exactly that shape).
    pub fn summary(
        &self,
        state: StateSizes,
        base_cache: BaseCacheSizes,
        analyses: AnalysisCounts,
        memory: crate::memory::Memory,
    ) -> String {
        let mut ordered: Vec<(&&'static str, &RequestStat)> = self.methods.iter().collect();
        // Name breaks the tie so the line is stable between two equal totals,
        // which is what makes two summaries in one session comparable by eye.
        ordered.sort_by(|left, right| {
            right
                .1
                .total_ms
                .cmp(&left.1.total_ms)
                .then_with(|| left.0.cmp(right.0))
        });

        let mut out = format!(
            "session trace after {} requests\n  \
             retained state: documents={} programs={} semantic_token_cache={} manifests={} \
             pending={} line_indices={} base_cache_worlds={} base_cache_weight={} \
             base_cache_budget={}\n  \
             {}\n  \
             analyses: started={} landed={} cancelled={}\n  \
             lsp-index (M27, editor tables per landed analysis): total={}ms max={}ms\n  \
             requests (count / mean ms / max ms), slowest total first:",
            self.total_requests,
            state.documents,
            state.programs,
            state.semantic_token_cache,
            state.manifests,
            state.pending,
            state.line_indices,
            // E179: the base cache beside the maps it is NOT one of. `programs=`
            // and `memory:` were already here and said nothing about the
            // analyzer's own retained worlds — which on kolt's nineteen files
            // weighed 266 MiB, the single largest thing the server holds, and
            // were invisible on the one page the owner reads when a session
            // starts feeling slow.
            base_cache.worlds,
            crate::memory::mib(Some(base_cache.weight_bytes)),
            crate::memory::mib(Some(base_cache.budget_bytes)),
            memory.line(),
            analyses.started,
            analyses.landed,
            analyses.cancelled,
            analyses.index_total_ms,
            analyses.index_max_ms,
        );
        if ordered.is_empty() {
            out.push_str("\n    (none yet)");
        }
        for (request, stat) in ordered {
            // Integer tenths: the trace is read as text in an output channel,
            // and a float here would drag locale-shaped formatting into a log
            // line that wants to diff cleanly against the next one.
            let mean_tenths = (stat.total_ms * 10) / u128::from(stat.count);
            out.push_str(&format!(
                "\n    {request}: {} / {}.{} / {}",
                stat.count,
                mean_tenths / 10,
                mean_tenths % 10,
                stat.max_ms,
            ));
        }
        out
    }
}

/// The shipped server's single tally. Poison-recovering like the server's other
/// shared locks (E97): a panicked handler must not convert an instrument into a
/// panic on every later request — the whole point of the fence it hangs off.
pub static TALLY: Mutex<Option<RequestTally>> = Mutex::new(None);

/// Record one request against the process tally.
pub fn record(request: &'static str, elapsed_ms: u128) -> TraceEvent {
    let mut guard = TALLY
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    guard
        .get_or_insert_with(RequestTally::new)
        .record(request, elapsed_ms)
}

/// The process tally's summary, given the caller's state cardinalities and
/// analysis counts.
pub fn summary(
    state: StateSizes,
    base_cache: BaseCacheSizes,
    analyses: AnalysisCounts,
    memory: crate::memory::Memory,
) -> String {
    let mut guard = TALLY
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    guard
        .get_or_insert_with(RequestTally::new)
        .summary(state, base_cache, analyses, memory)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recorded_request_accumulates_count_total_and_max() {
        let mut tally = RequestTally::new();
        assert_eq!(tally.record("hover", 5), TraceEvent::Quiet);
        assert_eq!(tally.record("hover", 15), TraceEvent::Quiet);
        assert_eq!(tally.record("hover", 10), TraceEvent::Quiet);
        assert_eq!(
            tally.stat("hover"),
            Some(RequestStat {
                count: 3,
                total_ms: 30,
                max_ms: 15,
            }),
        );
        assert_eq!(tally.total_requests(), 3);
    }

    // The slow line is the instrument's whole point: a session that feels slow
    // must leave a named, timed record behind, and a quiet one must leave none.
    #[test]
    fn a_slow_request_is_named_and_a_fast_one_is_silent() {
        let mut tally = RequestTally::new();
        assert_eq!(
            tally.record("completion", SLOW_REQUEST_MS - 1),
            TraceEvent::Quiet,
            "under the threshold says nothing",
        );
        let TraceEvent::Slow(line) = tally.record("completion", SLOW_REQUEST_MS) else {
            panic!("the threshold itself is slow");
        };
        assert!(
            line.contains("completion") && line.contains(&format!("{SLOW_REQUEST_MS} ms")),
            "the line names the method and its duration: {line}",
        );
        assert!(
            line.contains("request 2 of this method"),
            "…and how deep into the session it is: {line}",
        );
    }

    // The summary boundary counts TOTAL requests, so it fires at the session's
    // own pace whatever mix of methods the editor sends.
    #[test]
    fn a_summary_is_asked_for_every_n_requests_across_all_methods() {
        let mut tally = RequestTally::new();
        let mut summaries = 0;
        for index in 0..(SUMMARY_EVERY_REQUESTS * 2) {
            // Alternate methods, so a per-method boundary would fire at the
            // wrong count and this pin would catch it.
            let request = if index % 2 == 0 {
                "hover"
            } else {
                "completion"
            };
            if tally.record(request, 1) == TraceEvent::Summarize {
                summaries += 1;
            }
        }
        assert_eq!(summaries, 2, "one per {SUMMARY_EVERY_REQUESTS} requests");
    }

    // A slow request at a summary boundary reports SLOW: the named outlier is
    // the more actionable of the two, and the next request brings the summary
    // around anyway.
    #[test]
    fn a_slow_request_on_a_summary_boundary_reports_the_slow_line() {
        let mut tally = RequestTally::new();
        for _ in 0..(SUMMARY_EVERY_REQUESTS - 1) {
            tally.record("hover", 1);
        }
        assert!(matches!(
            tally.record("hover", SLOW_REQUEST_MS + 100),
            TraceEvent::Slow(_)
        ));
    }

    // The summary orders by TOTAL time, not by the per-call mean — a cheap
    // request sent on every keystroke outweighs a rare expensive one, and it is
    // the one the session is actually waiting on.
    #[test]
    fn the_summary_orders_by_total_time_and_reports_retained_state() {
        let mut tally = RequestTally::new();
        for _ in 0..100 {
            tally.record("semantic_tokens_full", 10);
        }
        tally.record("formatting", 200);

        let summary = tally.summary(
            StateSizes {
                documents: 4,
                programs: 2,
                semantic_token_cache: 4,
                manifests: 1,
                pending: 0,
                line_indices: 37,
            },
            BaseCacheSizes::default(),
            AnalysisCounts {
                started: 9,
                landed: 3,
                cancelled: 5,
                index_total_ms: 0,
                index_max_ms: 0,
            },
            crate::memory::Memory::default(),
        );
        let tokens_at = summary
            .find("semantic_tokens_full")
            .expect("the token row is present");
        let formatting_at = summary.find("formatting").expect("the formatting row");
        assert!(
            tokens_at < formatting_at,
            "1000 ms of token requests outranks one 200 ms format: {summary}",
        );
        assert!(
            summary.contains("semantic_tokens_full: 100 / 10.0 / 10"),
            "count / mean / max, as integers: {summary}",
        );
        assert!(
            summary.contains(
                "documents=4 programs=2 semantic_token_cache=4 manifests=1 pending=0 \
                 line_indices=37 base_cache_worlds=0 base_cache_weight=0.0 MiB \
                 base_cache_budget=0.0 MiB"
            ),
            "the retained-state cardinalities are the growth evidence: {summary}",
        );
        assert!(
            summary.contains("analyses: started=9 landed=3 cancelled=5"),
            "M26: the scheduler's own counts ride the same line — the three do \
             not sum, because an analysis that finished and was then dropped by \
             `land` is started and neither of the others: {summary}",
        );
    }

    // A mean that is not a whole number still renders — the tenths are computed
    // in integers so the line diffs cleanly against the next summary.
    /// E166: the status page carries the three memory figures and M63's
    /// retained-program count — the numbers E106 and M63 were found with, in
    /// the report the owner sends when a session starts feeling slow.
    ///
    /// The page's job is to be QUOTED, so this pin is on the whole line: the
    /// order, the labels and the unit are what make two readings from one
    /// session comparable by eye.
    #[test]
    fn the_summary_reports_the_process_memory_and_the_retained_programs() {
        let mut tally = RequestTally::new();
        tally.record("hover", 3);
        let summary = tally.summary(
            StateSizes {
                documents: 18,
                programs: 2,
                semantic_token_cache: 18,
                manifests: 1,
                pending: 0,
                line_indices: 61,
            },
            // E179: the base cache's own numbers, from the budget walk this
            // page exists to make readable — twelve worlds at 266 MiB under
            // M67's 192 MiB bound is the state that says the bound is not
            // holding, and it is the state the owner reported with nothing on
            // the page to name it.
            BaseCacheSizes {
                worlds: 12,
                weight_bytes: 278_921_216,
                budget_bytes: 201_326_592,
            },
            AnalysisCounts::default(),
            crate::memory::Memory {
                resident_bytes: Some(532_824_064),
                heap_in_use_bytes: Some(522_533_888),
                heap_retained_bytes: Some(103_317_504),
            },
        );
        assert!(
            summary.contains(
                "retained state: documents=18 programs=2 semantic_token_cache=18 \
                 manifests=1 pending=0 line_indices=61 base_cache_worlds=12 \
                 base_cache_weight=266.0 MiB base_cache_budget=192.0 MiB"
            ),
            "M63's own number belongs beside the documents it bounds, and E179's \
             three beside them — the base cache is the largest thing the server \
             retains and the page did not name it: {summary}",
        );
        assert!(
            summary.contains(
                "memory: rss=508.1 MiB heap_in_use=498.3 MiB heap_retained_free=98.5 MiB"
            ),
            "E166's three figures, in MiB, on their own line: {summary}",
        );
    }

    #[test]
    fn a_fractional_mean_renders_to_one_decimal() {
        let mut tally = RequestTally::new();
        tally.record("hover", 1);
        tally.record("hover", 2);
        let summary = tally.summary(
            StateSizes::default(),
            BaseCacheSizes::default(),
            AnalysisCounts::default(),
            crate::memory::Memory::default(),
        );
        assert!(
            summary.contains("hover: 2 / 1.5 / 2"),
            "1 and 2 average to 1.5: {summary}",
        );
    }

    // An empty tally is a legitimate reading (a session that has answered
    // nothing yet), and must render rather than panic on the division.
    #[test]
    fn an_empty_tally_summarizes_without_dividing_by_zero() {
        let tally = RequestTally::new();
        let summary = tally.summary(
            StateSizes::default(),
            BaseCacheSizes::default(),
            AnalysisCounts::default(),
            crate::memory::Memory::default(),
        );
        assert!(summary.contains("(none yet)"), "{summary}");
    }

    /// **M27**: the editor tables are on the session trace.
    ///
    /// `lsp-index` — the reference/entity index plus E121's landed walk — is a
    /// fifth per-keystroke cost, 110–292 ms of wall on a real browser
    /// application against `analyze`'s ~1,100 ms (E126, 2026-09-04). It was
    /// visible only under `VILAN_PHASE_TIMING`, which nobody sets in a live
    /// session, so a session that felt slow could not be told apart from one
    /// that was slow for a reason the trace already named.
    ///
    /// `max` is asserted alongside `total` because they answer different
    /// questions: total is what the session spent, max is the single analysis
    /// the user waited on.
    #[test]
    fn the_summary_reports_what_the_editor_tables_cost() {
        let tally = AnalysisTally::default();
        tally.record_landed();
        tally.record_index(std::time::Duration::from_millis(110));
        tally.record_landed();
        tally.record_index(std::time::Duration::from_millis(292));
        let counts = tally.counts();
        assert_eq!(counts.index_total_ms, 402);
        assert_eq!(
            counts.index_max_ms, 292,
            "the peak is kept, not averaged away — it is the keystroke the editor stalled on",
        );

        let summary = RequestTally::new().summary(
            StateSizes::default(),
            BaseCacheSizes::default(),
            counts,
            crate::memory::Memory::default(),
        );
        assert!(
            summary.contains(
                "lsp-index (M27, editor tables per landed analysis): \
                              total=402ms max=292ms"
            ),
            "the trace must name the cost and its number, or it is not evidence: {summary}",
        );
    }
}
