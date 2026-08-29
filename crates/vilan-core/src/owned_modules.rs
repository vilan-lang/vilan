//! Analysis-owned module allocations (M9, `leak-soak.md` §7.9.4).
//!
//! `parse_clean_cached` and the loader's error cache are process-global and
//! content-keyed with no eviction — right for what they were built for (E12:
//! std and dependency modules, stable disk content reused by every compile in
//! the process) and a session leak for what an editor feeds them: an OPEN
//! buffer's content changes with every landed keystroke and can never recur,
//! so a dependent document's re-analysis leaks one text + tree of the edited
//! file per keystroke (§7.5), plus the pre-cleanliness text copy on broken
//! contents (§7.9.1). Evicting the shared entry instead was designed and
//! proved unsound as a contained change (§7.9.2: live adopted programs,
//! in-flight analyses, `BASE_CACHE`'s content-revalidated worlds — a ctrl-Z
//! use-after-free — immortal macro worlds, and content aliasing each break
//! it), so the churning content is never shared in the first place.
//!
//! During an analysis that OPTED IN (`analyze_source_owning_overlay_modules`
//! — the language server's entry point), a module load served from the
//! open-document overlay bypasses the process-global caches and parses into
//! allocations collected on this thread-local scope: [`Leaked`] handles for
//! the text and tree, plus the rendered errors when the content does not
//! parse clean. The scope drains into `AnalyzedEntry` beside the entry's own
//! two handles, the server's `AnalyzedProgram` owns the lot, and its `Drop`
//! reclaims them after the program — M7's proven pattern one level down. The
//! bound is the open set: outstanding module bytes are the sum, over open
//! documents, of the overlay-served modules their CURRENT analysis loaded,
//! and per-distinct-content growth is zero.
//!
//! With no scope active — the CLI, the wasm front end (which serves
//! everything from the overlay and must keep the global caches), tests, and
//! transient editor queries (`module_importables`, platform inference) —
//! nothing changes, byte for byte: activation is the explicit opt-in, not an
//! ambient property of the overlay. A macro-world compile keeps the global
//! caches even under an active scope (`in_macro_world` marks the region):
//! its world outlives every analysis by design (§7.9.4b).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::analyzer::LoadedModule;
use crate::leak_tally::Leaked;
use crate::node::NodeList;
use crate::span::Spanned;

/// What one overlay-served module load allocated: the text, the tree parsed
/// from it (which borrows the text), and — when the content did not parse
/// clean — the rendered errors. Handles only; the `&'static` borrows went
/// into the analysis as a `LoadedModule`.
pub(crate) struct OwnedModuleAllocation {
    pub(crate) text: Leaked<str>,
    pub(crate) ast: Leaked<Spanned<NodeList<'static>>>,
    pub(crate) parse_errors: Option<Leaked<[crate::analyzer::ModuleParseError]>>,
}

/// Every allocation one analysis's overlay-served module loads made — the
/// drained scope, carried on `AnalyzedEntry` and owned by whoever owns the
/// `Program` built over it (the server's `AnalyzedProgram`). Dropping it
/// WITHOUT [`reclaim`](OwnedModules::reclaim) keeps the allocations leaked —
/// the same contract as every [`Leaked`] handle, and the panic path's story.
#[derive(Default)]
pub struct OwnedModules {
    allocations: Vec<OwnedModuleAllocation>,
}

impl OwnedModules {
    /// No modules owned — every analysis that did not opt in, and the
    /// degraded no-program document.
    pub fn none() -> OwnedModules {
        OwnedModules::default()
    }

    /// How many overlay-served modules this analysis owns.
    pub fn len(&self) -> usize {
        self.allocations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.allocations.is_empty()
    }

    /// Frees every owned allocation and releases its recorded bytes at the
    /// `OwnedModule*` sites (on the CURRENT thread — the release may be
    /// cross-thread, like every reclaim).
    ///
    /// # Safety
    ///
    /// Every reference derived from these loads must be dead: the `Program`
    /// built by the analysis that collected them has been dropped, and no
    /// global retains one — which is the mechanism's own invariant (the
    /// base-world store gate refuses to store a world that loaded any
    /// overlay-served source, and a macro-world compile never loads through
    /// the scope; `leak-soak.md` §7.9.4a/b).
    pub unsafe fn reclaim(self) {
        for allocation in self.allocations {
            // SAFETY: the caller's contract above — no borrower of any of
            // the three survives. The tree borrows the text, so nothing
            // here reads either: reclaim only frees.
            unsafe {
                allocation.text.reclaim();
                allocation.ast.reclaim();
                if let Some(parse_errors) = allocation.parse_errors {
                    parse_errors.reclaim();
                }
            }
        }
    }
}

/// The active scope's state: the collected allocations and the per-scope
/// path memo — a module reached twice in one analysis (a lib surface and a
/// direct import, say) is parsed and owned ONCE (§7.9.4d). The memo is keyed
/// by the loader's REQUESTED path, uncanonicalized on purpose: the loader
/// join-builds one spelling per module, canonicalizing costs a filesystem
/// round trip per load (the overlay map pays it because correctness there
/// needs it; this map is an optimization), and a miss on a second spelling
/// is harmless — the analysis owns and reclaims one more copy.
struct ScopeState {
    allocations: Vec<OwnedModuleAllocation>,
    memo: HashMap<PathBuf, LoadedModule>,
}

thread_local! {
    /// The current analysis's collection scope, when one is active. One per
    /// thread suffices: an analysis runs whole on its (big-stack) thread,
    /// and the only nested analyses — macro-world compiles — deliberately
    /// bypass the scope.
    static SCOPE: RefCell<Option<ScopeState>> = const { RefCell::new(None) };
}

/// The RAII activation of the collection scope, held by
/// `analyze_source_owning_overlay_modules` around the analysis. Dropping it
/// WITHOUT [`drain`](CollectionScope::drain) — the caller unwound —
/// deactivates the scope and keeps the collected allocations leaked, exactly
/// the M7 panic story: once per compiler bug, not a session rate.
pub(crate) struct CollectionScope {
    /// Constructible only through [`activate`](CollectionScope::activate).
    _private: (),
}

impl CollectionScope {
    /// Activates the scope on this thread. Panics if one is already active:
    /// the opted-in entry point is not re-entrant on a thread (a nested
    /// analysis is a macro-world compile, which must not opt in).
    pub(crate) fn activate() -> CollectionScope {
        SCOPE.with(|scope| {
            let mut scope = scope.borrow_mut();
            assert!(
                scope.is_none(),
                "an owned-modules collection scope is already active on this thread"
            );
            *scope = Some(ScopeState {
                allocations: Vec::new(),
                memo: HashMap::new(),
            });
        });
        CollectionScope { _private: () }
    }

    /// Deactivates the scope and hands its collected allocations to the
    /// caller — the drain into `AnalyzedEntry`.
    pub(crate) fn drain(self) -> OwnedModules {
        let state = SCOPE.with(|scope| scope.borrow_mut().take());
        // `Drop` would only re-take the (now empty) slot; skip it so the
        // deactivation happens exactly once, here.
        std::mem::forget(self);
        OwnedModules {
            allocations: state.map(|state| state.allocations).unwrap_or_default(),
        }
    }
}

impl Drop for CollectionScope {
    fn drop(&mut self) {
        // The guard's owner unwound before draining: deactivate, dropping
        // the collected handles WITHOUT reclaim — the allocations stay
        // leaked (see the struct doc).
        SCOPE.with(|scope| {
            scope.borrow_mut().take();
        });
    }
}

/// Whether a collection scope is active on this thread — the loader's check.
pub(crate) fn collecting() -> bool {
    SCOPE.with(|scope| scope.borrow().is_some())
}

/// Whether the active analysis loaded any overlay-served source — the flag
/// the base-world store gate reads (§7.9.4a): a stored world outlives the
/// analysis, so it must never borrow what the analysis owns.
pub(crate) fn analysis_owns_overlay_loads() -> bool {
    SCOPE.with(|scope| {
        scope
            .borrow()
            .as_ref()
            .is_some_and(|state| !state.allocations.is_empty())
    })
}

/// The scope's memo of `path` — the loader's requested spelling — if this
/// analysis already parsed it (§7.9.4d).
pub(crate) fn memoized(path: &Path) -> Option<LoadedModule> {
    SCOPE.with(|scope| {
        scope
            .borrow()
            .as_ref()
            .and_then(|state| state.memo.get(path).copied())
    })
}

/// Records one parsed overlay-served module on the active scope: the
/// allocation handles for the eventual reclaim, and the borrows under the
/// requested path for the memo. Must only be called with a scope active (the
/// loader checks [`collecting`] first).
pub(crate) fn adopt(path: &Path, allocation: OwnedModuleAllocation, loaded: LoadedModule) {
    SCOPE.with(|scope| {
        let mut scope = scope.borrow_mut();
        let state = scope
            .as_mut()
            .expect("owned_modules::adopt requires an active collection scope");
        state.allocations.push(allocation);
        state.memo.insert(path.to_path_buf(), loaded);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::leak_tally::{self, LeakSite};

    fn owned_allocation(text: &str) -> (OwnedModuleAllocation, LoadedModule) {
        let (text_handle, text) = Leaked::leak(
            text.to_string().into_boxed_str(),
            LeakSite::OwnedModuleText,
            text.len(),
        );
        let tree: Box<Spanned<NodeList<'static>>> = Box::new((Vec::new(), (0..0).into()));
        let bytes = std::mem::size_of_val(&*tree);
        let (ast_handle, ast) = Leaked::leak(tree, LeakSite::OwnedModuleAst, bytes);
        (
            OwnedModuleAllocation {
                text: text_handle,
                ast: ast_handle,
                parse_errors: None,
            },
            LoadedModule {
                ast,
                text,
                parse_errors: &[],
            },
        )
    }

    /// The scope collects while active, memoizes by path, and drains into an
    /// `OwnedModules` whose reclaim nets the sites to zero.
    #[test]
    fn a_scope_collects_memoizes_and_reclaims_to_zero() {
        leak_tally::reset();
        assert!(!collecting());
        assert!(memoized(Path::new("/m9/probe.vl")).is_none());
        let scope = CollectionScope::activate();
        assert!(collecting());
        assert!(!analysis_owns_overlay_loads(), "no load yet");
        let (allocation, loaded) = owned_allocation("owned probe text");
        adopt(Path::new("/m9/probe.vl"), allocation, loaded);
        assert!(analysis_owns_overlay_loads());
        let memoized_loaded =
            memoized(Path::new("/m9/probe.vl")).expect("the adopted path memoizes");
        assert!(
            std::ptr::eq(memoized_loaded.text, loaded.text),
            "the memo serves the same allocation, not a reparse"
        );
        let owned = scope.drain();
        assert!(!collecting(), "draining deactivates the scope");
        assert_eq!(owned.len(), 1);
        let outstanding_before = leak_tally::outstanding(LeakSite::OwnedModuleText);
        assert!(outstanding_before > 0);
        // SAFETY: `loaded`'s borrows are not read past this point.
        unsafe { owned.reclaim() };
        assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleText), 0);
        assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleAst), 0);
        leak_tally::reset();
    }

    /// Dropping the guard without draining (the unwind path) deactivates the
    /// scope and keeps the allocations leaked — nothing is released.
    #[test]
    fn an_undrained_scope_deactivates_and_keeps_the_leak() {
        leak_tally::reset();
        let scope = CollectionScope::activate();
        let (allocation, loaded) = owned_allocation("unwound analysis text");
        adopt(Path::new("/m9/unwound.vl"), allocation, loaded);
        drop(scope);
        assert!(!collecting());
        assert_eq!(leak_tally::released(LeakSite::OwnedModuleText), 0);
        assert_eq!(leak_tally::released(LeakSite::OwnedModuleAst), 0);
        assert!(leak_tally::outstanding(LeakSite::OwnedModuleText) > 0);
        leak_tally::reset();
    }

    /// Activation is not re-entrant: a second scope on the same thread is a
    /// bug (a nested analysis is a macro-world compile, which must not opt
    /// in), and it says so.
    #[test]
    #[should_panic(expected = "already active")]
    fn a_nested_activation_panics() {
        let _outer = CollectionScope::activate();
        let _inner = CollectionScope::activate();
    }
}
