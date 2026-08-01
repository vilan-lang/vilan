//! Per-site leaked-byte counters (analysis-reuse.md E3 Phase 1).
//!
//! The compiler leaks a handful of `&'static` allocations per analysis by
//! design: the source text and AST arenas the `Program` borrows for `'static`
//! outlive the analysis on purpose. Backlog E3 asks that these leaks be
//! *measured*, not RSS-inferred — RSS is dominated by allocator retention from
//! rebuilding and dropping the reachable `Program` every call, which swamps the
//! few KiB of genuine per-analysis leak and is far too noisy to gate on.
//!
//! Every `Box::leak`/`String::leak` site in `vilan-core`, `vilan-lsp`, and
//! `vilan-wasm` calls [`record`] with a [`LeakSite`] tag and the byte count it
//! just made immortal.
//! A test reads a site's total with [`bytes`], the sum with [`total`], and
//! zeroes them between measurements with [`reset`].
//!
//! **Thread-local, not process-global.** Analysis runs inline on one thread
//! (the LSP and the leak harness each spawn a big-stack thread and run the
//! whole pipeline on it), so a thread-local counter tallies exactly the leaks
//! of the analyses that ran on the measuring thread — immune to the parallel
//! test runner, where a process-global counter's before/after deltas are
//! famously flaky (the E12 pointer-identity lesson). The cost is one `Cell` add
//! per leak, noise next to the heap allocation being leaked, so the tally stays
//! always-on rather than behind `cfg(test)` — which would not survive
//! `vilan-core` being built as a (non-test) dependency of `vilan-lsp`'s test
//! binary in any case.
//!
//! Text-site counts are exact byte lengths. AST-site counts differ by what
//! their assertions need: the entry AST — the one site *allowed* to grow per
//! analysis — records a tree-proportional estimate (node count × node size),
//! so growth in the tree is visible to the counters; the cache-bounded AST
//! sites record the shallow `size_of_val` of the leaked box, because their
//! assertions care only that the site plateaus at zero, and zero is zero at
//! any depth. No AST figure is a deep heap audit — `MacroWorldProgram`'s
//! retained program in particular is far larger than its shallow record, and
//! is bounded by the world cache, not by this tally.

use std::cell::Cell;

/// A named `Box::leak`/`String::leak` site. Discriminants index the counters.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LeakSite {
    /// The LSP entry source text, leaked so the `Program` can borrow it.
    LspEntryText,
    /// The entry file's parsed AST, leaked in `analyze_source`. Recorded as a
    /// tree-proportional estimate, not the shallow root box (see module doc).
    EntryAst,
    /// `parse_clean_cached`'s leaked source (content-keyed: one per content).
    ParseCleanCacheText,
    /// `parse_clean_cached`'s leaked AST (content-keyed: one per content).
    ParseCleanCacheAst,
    /// A `macro { .. }` block's synthetic entry name.
    MacroBlockEntryName,
    /// A dependency package's display name.
    DisplayName,
    /// A non-clean module's leaked source in the loader's error path.
    ModuleErrorText,
    /// A non-clean module's leaked AST/error slice in the loader's error path.
    ModuleErrorAst,
    /// The macro world's ambient-prelude import text.
    MacroPreludeText,
    /// A macro world's blanked entry source (content-keyed by `WORLDS`).
    MacroWorldText,
    /// A compiled macro world's `Program` (content-keyed by `WORLDS`).
    MacroWorldProgram,
    /// A macro's raw expansion text (content-keyed by `cached_run`).
    MacroExpansion,
    /// `parse_generated`'s leaked copy of the source it parses.
    MacroParseText,
    /// `parse_generated`'s leaked AST.
    MacroParseAst,
    /// The wasm front-end's entry source text (content-interned: one per
    /// distinct compiled source, however many Runs repeat it).
    WasmEntryText,
}

/// The number of [`LeakSite`] variants — keep in step with the enum.
const SITE_COUNT: usize = 15;

/// Every site in declaration order — keep in step with the enum; [`report`]
/// iterates it.
const ALL_SITES: [LeakSite; SITE_COUNT] = [
    LeakSite::LspEntryText,
    LeakSite::EntryAst,
    LeakSite::ParseCleanCacheText,
    LeakSite::ParseCleanCacheAst,
    LeakSite::MacroBlockEntryName,
    LeakSite::DisplayName,
    LeakSite::ModuleErrorText,
    LeakSite::ModuleErrorAst,
    LeakSite::MacroPreludeText,
    LeakSite::MacroWorldText,
    LeakSite::MacroWorldProgram,
    LeakSite::MacroExpansion,
    LeakSite::MacroParseText,
    LeakSite::MacroParseAst,
    LeakSite::WasmEntryText,
];

thread_local! {
    static COUNTERS: [Cell<usize>; SITE_COUNT] = const { [const { Cell::new(0) }; SITE_COUNT] };
}

/// Records `bytes` newly leaked at `site` on the current thread.
#[inline]
pub fn record(site: LeakSite, bytes: usize) {
    COUNTERS.with(|counters| {
        let cell = &counters[site as usize];
        cell.set(cell.get() + bytes);
    });
}

/// The bytes leaked at `site` on the current thread since the last [`reset`].
pub fn bytes(site: LeakSite) -> usize {
    COUNTERS.with(|counters| counters[site as usize].get())
}

/// The total bytes leaked across every site on the current thread.
pub fn total() -> usize {
    COUNTERS.with(|counters| counters.iter().map(Cell::get).sum())
}

/// Zeroes every counter on the current thread (call between measurements).
pub fn reset() {
    COUNTERS.with(|counters| {
        for cell in counters {
            cell.set(0);
        }
    });
}

/// The current thread's counters as one line: the total, then every nonzero
/// site by name — the same per-site split the leak harness asserts on, so a
/// field report and a harness run read identically (backlog E24). Cumulative
/// since thread start: production never calls [`reset`], so the line is the
/// thread's whole history, and growth shows as growth.
pub fn report() -> String {
    let mut parts = Vec::new();
    for site in ALL_SITES {
        let leaked = bytes(site);
        if leaked > 0 {
            parts.push(format!("{site:?} {leaked} B"));
        }
    }
    if parts.is_empty() {
        return "total 0 B".to_string();
    }
    format!("total {} B: {}", total(), parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The report is the harness's split: the total, then each nonzero site by
    /// its enum name — and ONLY nonzero sites, so a quiet thread reads
    /// "total 0 B" rather than fifteen zeros. Counters are thread-local and
    /// the test runner gives each test its own thread, so the reset here
    /// cannot race a neighbor.
    #[test]
    fn the_report_names_nonzero_sites_and_the_total() {
        reset();
        assert_eq!(report(), "total 0 B");
        record(LeakSite::EntryAst, 40);
        record(LeakSite::MacroWorldText, 100);
        assert_eq!(report(), "total 140 B: EntryAst 40 B, MacroWorldText 100 B");
        reset();
    }
}
