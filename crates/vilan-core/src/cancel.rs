//! M26 (`proposal/editor-latency.md` §4.2): the cancellation token a
//! superseded analysis is stopped with.
//!
//! E117 gave every analysis the world revision it read and made `land` DROP a
//! result that a newer revision has overtaken. That closes the correctness
//! hole — a stale result can never become the published one — but it closes it
//! at the END: the superseded analysis still runs to completion on its 128 MiB
//! thread, and a keystroke burst pays one whole analysis per debounce window
//! for answers nobody will ever see. This is the optimisation on top: the
//! scheduler tells the analysis that its answer is already worthless, and the
//! analysis stops.
//!
//! **Cancellation is never what correctness rests on.** `land` and
//! `plan_publish` keep their revision-stamp checks exactly as they were; a
//! cancelled analysis is dropped by the same code that would have dropped a
//! superseded one that ran to the end. Every checkpoint here may therefore be
//! removed, or fail to fire, without changing what the editor shows — only how
//! much CPU it took to show it.
//!
//! **Ambient, not threaded.** The token is installed on the analysis thread
//! and read from a thread-local, the way
//! [`owned_modules::CollectionScope`](crate::owned_modules) is: an analysis
//! runs whole on its own big-stack thread, so "the analysis running on this
//! thread" and "the analysis" are the same thing. The alternative — a
//! `should_continue: &dyn Fn() -> bool` parameter threaded from
//! `analyze_source` through `analyze`, `analyze_inner`, `analyze_over_world`
//! and `post_analysis_passes` — puts a parameter on five signatures and every
//! test that calls them, to say something none of them branches on.
//!
//! **A macro world is exempt.** A `macro`-defining entry compiles a nested
//! analysis on the same thread and the result is stored in the process-global
//! `WORLDS` cache, which outlives every analysis. A half-built world stored
//! there would be read back by a LATER analysis as if it were whole, which is
//! precisely the class of bug cancellation must not be able to introduce — so
//! [`cancelled`] answers `false` inside one, and the nested compile always
//! runs to the end. Everything the outer analysis does after it is still
//! cancellable.
//!
//! **Where the checkpoints are.** All of them are downstream of every
//! process-global store: the base world cache is written inside `analyze_inner`
//! BEFORE the entry tail begins (`analyzer.rs`'s `base_cache_store`), the parse
//! caches are written per module during the load, and each entry they hold is
//! complete when it is stored. So no checkpoint can leave a shared cache
//! holding a partial value, and a cancelled analysis's only residue is the
//! allocations its own `AnalyzedProgram` owns and reclaims.

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The handle a scheduler keeps on one running analysis. Cloning shares the
/// flag: the scheduler holds one clone and the analysis thread the other.
///
/// The flag is one-way. An analysis that has been told its answer is worthless
/// can never be told otherwise — the newer edit that superseded it does not
/// un-happen — so [`cancel`](CancelToken::cancel) has no counterpart, and a
/// checkpoint that reads `true` may skip everything after it without
/// re-checking.
#[derive(Clone, Debug, Default)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
}

impl CancelToken {
    /// A fresh, uncancelled token.
    pub fn new() -> CancelToken {
        CancelToken::default()
    }

    /// Tell the analysis holding this token that its result is already
    /// superseded. Idempotent.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Relaxed);
    }

    /// Whether [`cancel`](CancelToken::cancel) has been called.
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }

    /// Install this token as the current thread's, for as long as the returned
    /// guard lives. The previous token (there is none in the shipped server —
    /// one analysis per thread — but a test may nest) is restored on drop, so
    /// the installation is a save/restore rather than an assertion.
    pub fn install(&self) -> CancelScope {
        CancelScope {
            previous: CURRENT.with(|current| current.borrow_mut().replace(self.clone())),
        }
    }
}

thread_local! {
    /// The token of the analysis running on this thread, when the front end
    /// installed one. `None` for every caller that does not cancel — the CLI,
    /// the wasm front end, the test harnesses — for whom [`cancelled`] is a
    /// constant `false`.
    static CURRENT: RefCell<Option<CancelToken>> = const { RefCell::new(None) };
}

/// The RAII installation of a [`CancelToken`] on this thread.
#[must_use = "the token is installed only while the scope is alive"]
pub struct CancelScope {
    previous: Option<CancelToken>,
}

impl Drop for CancelScope {
    fn drop(&mut self) {
        let previous = self.previous.take();
        CURRENT.with(|current| *current.borrow_mut() = previous);
    }
}

/// Whether the analysis running on this thread has been cancelled — the
/// checkpoint every phase boundary and every long loop reads.
///
/// One `RefCell` borrow and one relaxed atomic load. Against a 950 ms analysis
/// that is nanoseconds per call, which is what lets the checkpoints sit inside
/// loops rather than only between phases.
///
/// Always `false` inside a macro world: see the module doc.
pub fn cancelled() -> bool {
    if crate::macros::in_macro_world() {
        return false;
    }
    CURRENT.with(|current| {
        current
            .borrow()
            .as_ref()
            .is_some_and(CancelToken::is_cancelled)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thread_with_no_token_is_never_cancelled() {
        assert!(!cancelled());
    }

    #[test]
    fn an_installed_token_is_read_by_the_checkpoint() {
        let token = CancelToken::new();
        let _scope = token.install();
        assert!(!cancelled(), "a fresh token is not cancelled");
        token.cancel();
        assert!(cancelled(), "the checkpoint reads the token that was set");
        assert!(token.is_cancelled());
    }

    #[test]
    fn the_scope_restores_the_previous_token() {
        let outer = CancelToken::new();
        let outer_scope = outer.install();
        {
            let inner = CancelToken::new();
            let _inner_scope = inner.install();
            inner.cancel();
            assert!(cancelled(), "the inner token is the one in force");
        }
        assert!(
            !cancelled(),
            "the inner scope's drop restored the outer token, which is not cancelled",
        );
        drop(outer_scope);
        assert!(!cancelled(), "and the outer scope's drop leaves no token");
    }

    /// The token is one-way and shared by clone: the scheduler's copy and the
    /// analysis thread's copy are the same flag.
    #[test]
    fn a_clone_shares_the_flag() {
        let scheduler = CancelToken::new();
        let analysis = scheduler.clone();
        assert!(!analysis.is_cancelled());
        scheduler.cancel();
        assert!(analysis.is_cancelled());
    }
}
