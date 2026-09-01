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
//! **Not every leak is forever.** A site that keeps the handle —
//! [`Leaked::leak`] — can give the allocation back once the owner knows every
//! borrow of it is dead ([`Leaked::reclaim`], `unsafe`, the contract in its
//! doc); the reclaim calls [`release`] with the bytes the leak recorded, and
//! [`outstanding`] is the signed net (recorded − released) per site. [`bytes`]
//! stays the GROSS figure, so a pin that says "this analysis recorded its
//! source to the byte" keeps reading the same number after the same bytes were
//! reclaimed. This is the M7 refinement (`leak-soak.md` §7): the language
//! server's entry text and AST are leaked per analysis as before, and
//! reclaimed when the `Document` drops the analysis that borrowed them.
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
//! binary in any case. A release is legitimately CROSS-thread — the shipped
//! server records on an analysis thread that then dies and releases on the
//! runtime thread that drops the `Document` — which is why [`outstanding`] is
//! signed: a thread that only released reads negative, and a harness that
//! reads every thread's counters inside that thread (the leak soak does) sums
//! to the exact net.
//!
//! **Not every retention is a leak, and the tally covers both** (backlog M11).
//! A `Box::leak` is the obvious way to hold memory for the life of a process;
//! a process-global cache with per-key overwrite as its only eviction is the
//! other, and it is the bigger one. `BASE_CACHE` retains a whole resolved
//! `World` per `BaseCacheKey` and `macros`' `FAILURES` retains rendered error
//! text per failing definition set — neither is a `Box::leak`, so neither
//! violated this module's literal contract, and that was exactly the finding:
//! `[vilan leak] total` omitted the largest per-process retention, and the
//! soak's strongest assertion (`total == counts().named()`) is blind to an
//! unrecorded site by construction. Both now [`record`] on insert and
//! [`release`] on eviction, so [`outstanding`] at those sites is the LIVE
//! retention rather than a running total — the one shape difference from the
//! leak sites, where gross and net differ only where a handle was reclaimed.
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
    /// The one site the language server reclaims (`analyze_source_reclaimable`
    /// hands the tree back as a [`Leaked`] handle; the `Document` that owns the
    /// `Program` releases it) — so its [`outstanding`] balance, not its gross
    /// [`bytes`], is what a session-leak claim reads.
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
    /// An overlay-served module's source, parsed into an ANALYSIS-OWNED
    /// allocation during an opted-in analysis (M9, `leak-soak.md` §7.9.4)
    /// instead of an entry in the process-global caches. Like the entry
    /// sites, reclaimed when the owning analysis is superseded or closed —
    /// so its [`outstanding`] balance, not its gross [`bytes`], is what the
    /// session-leak claim reads.
    OwnedModuleText,
    /// An overlay-served module's AST, analysis-owned (M9). Shallow record,
    /// like the cache-bounded AST sites: the claim is that it nets to zero.
    OwnedModuleAst,
    /// An overlay-served non-clean module's rendered parse errors,
    /// analysis-owned (M9). Shallow record of the slice.
    OwnedModuleErrors,
    /// The macro world's ambient-prelude import text.
    MacroPreludeText,
    /// A macro world's blanked entry source (content-keyed by `WORLDS`).
    MacroWorldText,
    /// A compiled macro world's `Program` (content-keyed by `WORLDS`).
    MacroWorldProgram,
    /// A macro world's blanked entry AST — the tree `analyze_source` leaks when
    /// it runs INSIDE a world compile (`in_macro_world`), kept by the cached
    /// world's program. Its own site so that `EntryAst` means exactly the
    /// top-level entry's tree (the one the language server reclaims), and this
    /// is bounded by `WORLDS`/`FAILURES` like the world's text and program.
    /// Recorded with the same tree-proportional estimate as `EntryAst`.
    MacroWorldAst,
    /// A macro's raw expansion text (content-keyed by `cached_run`).
    MacroExpansion,
    /// `parse_generated`'s leaked copy of the source it parses.
    MacroParseText,
    /// `parse_generated`'s leaked AST.
    MacroParseAst,
    /// The wasm front-end's entry source text (content-interned: one per
    /// distinct compiled source, however many Runs repeat it).
    WasmEntryText,
    /// A resolved pre-entry `World` retained by `analyzer`'s `BASE_CACHE`, one
    /// per distinct `BaseCacheKey` (backlog M11). Not a `Box::leak` — the
    /// world is an ordinary value in a process-global map — but a RETENTION of
    /// exactly the kind this tally exists to make countable, and until M11 the
    /// largest one in the process was the one `[vilan leak] total` could not
    /// see. Recorded SOURCE-PROPORTIONALLY: the total bytes of the module
    /// texts the world was built from, which is what the derived analyzer
    /// state it retains scales with. Like the AST sites this is a proportional
    /// figure and not a heap audit — the real retention is a multiple of it,
    /// and the texts themselves belong to `ParseCleanCache*`, which is shared
    /// across worlds and already counted there. Released on eviction: a
    /// per-key overwrite, a stale-hit removal, and `base_cache_clear` each
    /// give the world's bytes back, so [`outstanding`] is the live retention
    /// and its growth is the growth of the key set.
    BaseCacheWorld,
    /// The rendered error text `macros`' `FAILURES` cache retains, one entry
    /// per distinct (definition set, layout) whose world failed to compile
    /// (backlog M11). Recorded as the bytes of the messages, notes and trace
    /// labels it holds; released when an entry is displaced or
    /// `macro_world_cache_clear` drops the map. Its sibling `WORLDS` needs no
    /// release: what that cache holds is genuinely `Box::leak`ed
    /// (`MacroWorldText`/`MacroWorldProgram`/`MacroWorldAst`) and clearing the
    /// map does not give one byte of it back.
    MacroFailureText,
}

/// The number of [`LeakSite`] variants — keep in step with the enum.
const SITE_COUNT: usize = 21;

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
    LeakSite::OwnedModuleText,
    LeakSite::OwnedModuleAst,
    LeakSite::OwnedModuleErrors,
    LeakSite::MacroPreludeText,
    LeakSite::MacroWorldText,
    LeakSite::MacroWorldProgram,
    LeakSite::MacroWorldAst,
    LeakSite::MacroExpansion,
    LeakSite::MacroParseText,
    LeakSite::MacroParseAst,
    LeakSite::WasmEntryText,
    LeakSite::BaseCacheWorld,
    LeakSite::MacroFailureText,
];

thread_local! {
    /// Gross bytes leaked per site — what [`record`] adds to.
    static COUNTERS: [Cell<usize>; SITE_COUNT] = const { [const { Cell::new(0) }; SITE_COUNT] };
    /// Bytes given back per site — what [`release`] adds to. Kept apart from
    /// `COUNTERS` rather than subtracted from it, so [`bytes`] keeps meaning
    /// "what this thread leaked" and a cross-thread release cannot underflow.
    static RELEASED: [Cell<usize>; SITE_COUNT] = const { [const { Cell::new(0) }; SITE_COUNT] };
}

/// Records `bytes` newly leaked at `site` on the current thread.
#[inline]
pub fn record(site: LeakSite, bytes: usize) {
    COUNTERS.with(|counters| {
        let cell = &counters[site as usize];
        cell.set(cell.get() + bytes);
    });
}

/// Records `bytes` given back at `site` on the current thread — the
/// counterpart of [`record`], called by [`Leaked::reclaim`] with exactly the
/// bytes that handle recorded. Never subtracts from [`bytes`]; see
/// [`outstanding`] for the net.
#[inline]
pub fn release(site: LeakSite, bytes: usize) {
    RELEASED.with(|released| {
        let cell = &released[site as usize];
        cell.set(cell.get() + bytes);
    });
}

/// The bytes leaked at `site` on the current thread since the last [`reset`]
/// — GROSS: a later reclaim does not lower it.
pub fn bytes(site: LeakSite) -> usize {
    COUNTERS.with(|counters| counters[site as usize].get())
}

/// The bytes given back at `site` on the current thread since the last
/// [`reset`].
pub fn released(site: LeakSite) -> usize {
    RELEASED.with(|released| released[site as usize].get())
}

/// The net bytes still leaked at `site` on the current thread: recorded minus
/// released. Signed, because the release may happen on a different thread
/// from the record (module doc) — a thread that only dropped reads negative.
pub fn outstanding(site: LeakSite) -> isize {
    bytes(site) as isize - released(site) as isize
}

/// The total bytes leaked across every site on the current thread (gross).
pub fn total() -> usize {
    COUNTERS.with(|counters| counters.iter().map(Cell::get).sum())
}

/// The total bytes given back across every site on the current thread.
pub fn released_total() -> usize {
    RELEASED.with(|released| released.iter().map(Cell::get).sum())
}

/// The net bytes still leaked across every site on the current thread.
pub fn outstanding_total() -> isize {
    total() as isize - released_total() as isize
}

/// Zeroes every counter on the current thread (call between measurements) —
/// the recorded and the released sides both.
pub fn reset() {
    COUNTERS.with(|counters| {
        for cell in counters {
            cell.set(0);
        }
    });
    RELEASED.with(|released| {
        for cell in released {
            cell.set(0);
        }
    });
}

/// A `Box::leak` whose site kept the handle, so the allocation can be given
/// back once every borrow of it is dead.
///
/// [`Leaked::leak`] does what the bare leak sites do — records `bytes` at
/// `site`, leaks the box, hands out the `&'static T` — and additionally
/// returns this handle. The handle has **no `Drop`**: dropping it keeps the
/// leak, which is exactly what a caller that does not opt in gets (the macro
/// world's nested compile, the wasm front end, every test that calls
/// `analyze_source`). Reclaiming is the explicit, `unsafe` act, because a
/// `'static` borrow is a promise the type system has already handed out.
///
/// The language server is the one owner today (`leak-soak.md` §7): its
/// `Document` holds the `Program` and the two handles the program borrows
/// from, drops the program, then reclaims.
pub struct Leaked<T: ?Sized + 'static> {
    pointer: std::ptr::NonNull<T>,
    site: LeakSite,
    bytes: usize,
}

// SAFETY: the handle is unique ownership of the allocation with no accessor —
// the `&'static T` went out at `leak` and is the program's business — so it
// moves between threads exactly as the `Box<T>` it came from would, and
// sharing a `&Leaked<T>` exposes nothing but two integers. The bounds mirror
// `Box<T>`'s.
unsafe impl<T: ?Sized + Send + 'static> Send for Leaked<T> {}
unsafe impl<T: ?Sized + Sync + 'static> Sync for Leaked<T> {}

impl<T: ?Sized + 'static> Leaked<T> {
    /// Leaks `value`, recording `bytes` at `site` on this thread, and returns
    /// the handle with the `'static` borrow the leaking site needs. The borrow
    /// and every reference derived from it are what [`reclaim`](Self::reclaim)
    /// invalidates.
    pub fn leak(value: Box<T>, site: LeakSite, bytes: usize) -> (Leaked<T>, &'static T) {
        record(site, bytes);
        // `Box::into_raw` keeps the allocation's full provenance on the raw
        // pointer; the shared borrow below is derived from it, so freeing
        // through the raw pointer later is the same allocation, same tag.
        let raw = Box::into_raw(value);
        // SAFETY: `Box::into_raw` never returns null.
        let pointer = unsafe { std::ptr::NonNull::new_unchecked(raw) };
        // SAFETY: the allocation is live and nobody else has a mutable path to
        // it — `'static` here is the leak's promise, honoured until `reclaim`.
        let borrow: &'static T = unsafe { &*raw };
        (
            Leaked {
                pointer,
                site,
                bytes,
            },
            borrow,
        )
    }

    /// The bytes this handle recorded at its site — what [`reclaim`](Self::reclaim)
    /// will release.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// The site this handle recorded at.
    pub fn site(&self) -> LeakSite {
        self.site
    }

    /// Frees the allocation and releases its recorded bytes at its site on the
    /// CURRENT thread (the leak may have been recorded on another; see the
    /// module doc on cross-thread release).
    ///
    /// # Safety
    ///
    /// Every reference derived from the `&'static T` that [`leak`](Self::leak)
    /// returned must be dead: nothing may read the allocation after this call.
    /// In the compiler that means the `Program` (and anything that borrowed
    /// from it) built over this allocation has been dropped, and no
    /// process-global retains a borrow into it — `leak-soak.md` §7.2 is the
    /// audit that establishes the latter for the entry text and AST.
    pub unsafe fn reclaim(self) {
        // SAFETY: `pointer` came from `Box::into_raw` in `leak`, is freed
        // exactly once (`self` is consumed), and the caller promises no
        // outstanding reference — the contract above.
        drop(unsafe { Box::from_raw(self.pointer.as_ptr()) });
        release(self.site, self.bytes);
    }
}

/// The current thread's counters as one line: the total, then every nonzero
/// site by name — the same per-site split the leak harness asserts on, so a
/// field report and a harness run read identically (backlog E24). Cumulative
/// since thread start: production never calls [`reset`], so the line is the
/// thread's whole history, and growth shows as growth. A thread that has
/// given bytes back appends a `reclaimed` clause in the same shape; a thread
/// that has not (the shipped server's analysis thread, which records and dies
/// while the runtime thread releases) prints exactly what it always did.
pub fn report() -> String {
    let mut parts = Vec::new();
    for site in ALL_SITES {
        let leaked = bytes(site);
        if leaked > 0 {
            parts.push(format!("{site:?} {leaked} B"));
        }
    }
    let mut line = if parts.is_empty() {
        "total 0 B".to_string()
    } else {
        format!("total {} B: {}", total(), parts.join(", "))
    };
    let mut reclaimed = Vec::new();
    for site in ALL_SITES {
        let given_back = released(site);
        if given_back > 0 {
            reclaimed.push(format!("{site:?} {given_back} B"));
        }
    }
    if !reclaimed.is_empty() {
        line.push_str(&format!(
            "; reclaimed {} B: {}",
            released_total(),
            reclaimed.join(", ")
        ));
    }
    line
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

    /// A reclaim shows on the report as its own clause, in the same per-site
    /// shape — and the gross `total` is unchanged by it, which is the whole
    /// point of keeping the two sides apart.
    #[test]
    fn the_report_appends_what_was_reclaimed() {
        reset();
        record(LeakSite::EntryAst, 40);
        record(LeakSite::LspEntryText, 10);
        release(LeakSite::EntryAst, 40);
        assert_eq!(
            report(),
            "total 50 B: LspEntryText 10 B, EntryAst 40 B; reclaimed 40 B: EntryAst 40 B"
        );
        reset();
    }

    /// `Leaked::leak` records exactly what a bare `record` would, the borrow
    /// reads the value, and `reclaim` releases the same bytes so the site's
    /// outstanding balance nets to zero while its gross stays.
    #[test]
    fn a_leaked_handle_records_on_leak_and_releases_on_reclaim() {
        reset();
        let text: Box<str> = "seven bytes".to_string().into_boxed_str();
        let (handle, borrow) = Leaked::leak(text, LeakSite::LspEntryText, 11);
        assert_eq!(borrow, "seven bytes");
        assert_eq!(bytes(LeakSite::LspEntryText), 11);
        assert_eq!(released(LeakSite::LspEntryText), 0);
        assert_eq!(outstanding(LeakSite::LspEntryText), 11);
        assert_eq!(handle.bytes(), 11);
        assert_eq!(handle.site(), LeakSite::LspEntryText);
        // SAFETY: `borrow` is not used past this point.
        unsafe { handle.reclaim() };
        assert_eq!(bytes(LeakSite::LspEntryText), 11, "the gross record stands");
        assert_eq!(released(LeakSite::LspEntryText), 11);
        assert_eq!(outstanding(LeakSite::LspEntryText), 0);
        assert_eq!(outstanding_total(), 0);
        reset();
    }

    /// Dropping the handle WITHOUT reclaiming keeps the leak: nothing is
    /// released, the borrow stays valid — today's behaviour for every caller
    /// that does not opt in.
    #[test]
    // The explicit `drop` of a non-`Drop` value IS the thing under test.
    #[allow(clippy::drop_non_drop)]
    fn dropping_a_leaked_handle_keeps_the_leak() {
        reset();
        let (handle, borrow) =
            Leaked::leak(Box::new([1u8, 2, 3]), LeakSite::ParseCleanCacheText, 3);
        drop(handle);
        assert_eq!(borrow, &[1, 2, 3]);
        assert_eq!(released(LeakSite::ParseCleanCacheText), 0);
        assert_eq!(outstanding(LeakSite::ParseCleanCacheText), 3);
        reset();
    }

    /// A release on a thread that recorded nothing reads as a negative
    /// balance — the shipped server's shape (records on the analysis thread,
    /// releases on the runtime thread), which a cross-thread sum nets to zero.
    #[test]
    fn a_cross_thread_release_reads_negative_on_the_releasing_thread() {
        reset();
        // The borrow is discarded at the join: `_` binds nothing, so no
        // reference outlives the reclaim below.
        let (handle, _) = std::thread::spawn(|| {
            reset();
            let leaked = Leaked::leak(Box::new(7u64), LeakSite::EntryAst, 8);
            assert_eq!(outstanding(LeakSite::EntryAst), 8);
            leaked
        })
        .join()
        .unwrap();
        // SAFETY: the borrow was discarded above; nothing reads the allocation.
        unsafe { handle.reclaim() };
        assert_eq!(bytes(LeakSite::EntryAst), 0);
        assert_eq!(released(LeakSite::EntryAst), 8);
        assert_eq!(outstanding(LeakSite::EntryAst), -8);
        reset();
    }
}
