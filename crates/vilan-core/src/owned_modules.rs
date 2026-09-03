//! Analysis-owned module allocations (M9, `leak-soak.md` §7.9.4), shared with
//! the stored base worlds that borrow them (M23).
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
//! parse clean. The scope drains into `AnalyzedEntry`, the server's
//! `AnalyzedProgram` owns the lot, and its `Drop` gives them back — M7's
//! proven pattern one level down.
//!
//! **M23 — the claim.** M9's first shape gave each allocation exactly one
//! owner, and paid for it with §7.9.4a's store gate: a base world that
//! loaded an overlay-served source was never stored, so an entry importing
//! any OPEN sibling rebuilt the whole pre-entry world on every keystroke
//! (kolt's `client.vl`: `base` 1.4–2.7 s, every keystroke, forever). The
//! ownership is a REFERENCE COUNT now. Every party that holds a reference
//! derived from an allocation holds a [`ModuleClaim`] on it: the analysis
//! that parsed it (drained into `AnalyzedEntry`, released by
//! `AnalyzedProgram::drop`), and any stored base world built over it
//! (claimed at `base_cache_store`, released when the world is displaced,
//! evicted stale, evicted by the byte budget, or cleared). The allocation is
//! freed exactly when the LAST claim is released, so no live program, no
//! in-flight analysis and no stored world can ever read freed memory —
//! §7.9.2's hazards all land on a claim rather than on an ordering rule.
//! Nothing is shared by CONTENT: a claim is a claim on one allocation, so
//! §7.9.2's content-aliasing hazard has nothing to alias.
//!
//! The bound moves accordingly: outstanding module bytes are the overlay-
//! served modules the open documents' CURRENT analyses loaded, PLUS one set
//! per retained base world — which is what M24's byte budget bounds. Per-
//! distinct-content growth is still zero.
//!
//! With no scope active — the CLI, the wasm front end (which serves
//! everything from the overlay and must keep the global caches), tests, and
//! transient editor queries (`module_importables`, platform inference) —
//! nothing changes, byte for byte: activation is the explicit opt-in, not an
//! ambient property of the overlay. Such an analysis has nowhere to put a
//! claim, so the base cache refuses to SERVE it a claimed world (a miss,
//! counted) rather than handing out an unclaimed borrow. A macro-world
//! compile keeps the global caches even under an active scope
//! (`in_macro_world` marks the region): its world outlives every analysis by
//! design (§7.9.4b), and for the same reason it is never served a claimed
//! world either.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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

impl OwnedModuleAllocation {
    /// Frees the three allocations and releases their recorded bytes.
    ///
    /// # Safety
    ///
    /// Every reference derived from this load must be dead — which is what
    /// the claim count decides, so the only caller is
    /// [`ModuleClaim::release`] on the last claim.
    unsafe fn free(self) {
        // SAFETY: the caller's contract. The tree borrows the text, so
        // nothing here reads either: reclaim only frees.
        unsafe {
            self.text.reclaim();
            self.ast.reclaim();
            if let Some(parse_errors) = self.parse_errors {
                parse_errors.reclaim();
            }
        }
    }
}

/// One party's claim on an [`OwnedModuleAllocation`] — the license to hold a
/// reference derived from it (M23).
///
/// Two kinds of party take one: the analysis that parsed the module (through
/// the collection scope, drained into `AnalyzedEntry` and released by the
/// server's `AnalyzedProgram::drop`) and a stored base world that borrows it
/// (taken in `base_cache_store`, released on every eviction path). A claim is
/// taken by CLONING an existing one, always while the cloner already holds a
/// live claim, so the count can never be resurrected from zero.
///
/// Like [`Leaked`], this has **no `Drop`**: dropping a claim keeps the
/// allocation leaked, which is the unwound analysis's story (M7's, one level
/// down) and what makes reclaiming an explicit, `unsafe` act rather than an
/// accident of scope. [`release`](ModuleClaim::release) is the only way back.
pub(crate) struct ModuleClaim(Arc<OwnedModuleAllocation>);

impl ModuleClaim {
    /// A second claim on the same allocation, for a second party.
    pub(crate) fn clone_claim(&self) -> ModuleClaim {
        ModuleClaim(Arc::clone(&self.0))
    }

    /// The text bytes this allocation records — what a release at the last
    /// claim gives back at `OwnedModuleText`. The measurement surface for the
    /// retention a stored world's claims represent.
    pub(crate) fn text_bytes(&self) -> usize {
        self.0.text.bytes()
    }

    /// Gives this claim back. Frees the allocation only if it was the LAST
    /// one — `Arc::into_inner` is the whole protocol.
    ///
    /// # Safety
    ///
    /// Every reference this claim's holder derived from the allocation must
    /// be dead: for an analysis, the `Program` built over it has been
    /// dropped; for a stored base world, the world has been removed from the
    /// cache and dropped. Other holders' references stay valid — that is what
    /// the count is for.
    pub(crate) unsafe fn release(self) {
        if let Some(allocation) = Arc::into_inner(self.0) {
            // SAFETY: this was the last claim, so by the contract above no
            // reference derived from the allocation survives anywhere.
            unsafe { allocation.free() };
        }
    }
}

/// Every claim one analysis holds on an overlay-served module allocation —
/// the drained scope, carried on `AnalyzedEntry` and owned by whoever owns
/// the `Program` built over it (the server's `AnalyzedProgram`). One claim
/// per module the analysis PARSED, plus one per module a base-cache hit
/// served it out of a stored world (M23). Dropping it WITHOUT
/// [`reclaim`](OwnedModules::reclaim) keeps the allocations leaked — the same
/// contract as every [`Leaked`] handle, and the panic path's story.
#[derive(Default)]
pub struct OwnedModules {
    claims: Vec<ModuleClaim>,
}

impl OwnedModules {
    /// No modules owned — every analysis that did not opt in, and the
    /// degraded no-program document.
    pub fn none() -> OwnedModules {
        OwnedModules::default()
    }

    /// How many overlay-served module allocations this analysis holds a
    /// claim on.
    pub fn len(&self) -> usize {
        self.claims.len()
    }

    pub fn is_empty(&self) -> bool {
        self.claims.is_empty()
    }

    /// Gives back every claim this analysis holds. An allocation whose LAST
    /// claim this was is freed and its bytes released at the `OwnedModule*`
    /// sites (on the CURRENT thread — the release may be cross-thread, like
    /// every reclaim); one a stored base world still claims survives, and is
    /// freed when that world is evicted (M23).
    ///
    /// # Safety
    ///
    /// Every reference THIS analysis derived from these loads must be dead:
    /// the `Program` it built has been dropped. Nothing else is required —
    /// the other holders' claims are exactly what keeps their references
    /// valid.
    pub unsafe fn reclaim(self) {
        for claim in self.claims {
            // SAFETY: the caller's contract above — this analysis's program,
            // the only thing that borrowed through this claim, is gone.
            unsafe { claim.release() };
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
    claims: Vec<ModuleClaim>,
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
                claims: Vec::new(),
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
            claims: state.map(|state| state.claims).unwrap_or_default(),
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

/// A second claim on every allocation the active analysis holds — what
/// `base_cache_store` takes for the world it is about to store (M23).
///
/// Taken from inside the analysis that already holds the originals, so every
/// clone happens while the count is provably nonzero. A superset of what the
/// world actually borrows is fine and is what this is: the store runs right
/// after the load loop, so the scope holds exactly that loop's loads, and an
/// unnecessary claim costs one retained copy until the world is evicted, not
/// soundness. Empty (and cheap) for the overwhelmingly common analysis that
/// loaded no overlay-served module at all.
pub(crate) fn claim_overlay_loads() -> Vec<ModuleClaim> {
    SCOPE.with(|scope| {
        scope
            .borrow()
            .as_ref()
            .map(|state| state.claims.iter().map(ModuleClaim::clone_claim).collect())
            .unwrap_or_default()
    })
}

/// Hands the active analysis its own claims on the allocations a base-cache
/// HIT just served it (M23), returning `false` — and taking nothing — when
/// there is no scope to hold them.
///
/// A world's clone borrows every module the stored world loaded, so the
/// analysis that receives it must hold a claim on each before the cache lock
/// is released. `false` is the caller's signal to treat the hit as a miss:
/// an analysis with nowhere to put a claim (the CLI, the wasm front end, a
/// transient reader) must never be handed a borrow it cannot keep alive.
pub(crate) fn adopt_claims(claims: Vec<ModuleClaim>) -> bool {
    SCOPE.with(|scope| {
        let mut scope = scope.borrow_mut();
        match scope.as_mut() {
            Some(state) => {
                state.claims.extend(claims);
                true
            }
            None => false,
        }
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
        state.claims.push(ModuleClaim(Arc::new(allocation)));
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
        assert!(claim_overlay_loads().is_empty(), "no load yet");
        let (allocation, loaded) = owned_allocation("owned probe text");
        adopt(Path::new("/m9/probe.vl"), allocation, loaded);
        assert_eq!(claim_overlay_loads().len(), 1);
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

    /// M23's protocol at its own level: a SECOND claim on the allocation —
    /// the one a stored base world takes — keeps it alive after the analysis
    /// gives its claim back, and the last release is what frees it.
    #[test]
    fn the_last_claim_frees_and_no_earlier_one_does() {
        leak_tally::reset();
        let scope = CollectionScope::activate();
        let (allocation, loaded) = owned_allocation("claimed probe text");
        let text_bytes = loaded.text.len();
        adopt(Path::new("/m23/claimed.vl"), allocation, loaded);
        // The store's claim, taken while the analysis still holds its own.
        let stored: Vec<ModuleClaim> = claim_overlay_loads();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].text_bytes(), text_bytes);
        let owned = scope.drain();

        // SAFETY: this analysis's borrows are dead; the stored claim's are
        // not, and it is what keeps the allocation alive.
        unsafe { owned.reclaim() };
        assert_eq!(
            leak_tally::outstanding(LeakSite::OwnedModuleText),
            text_bytes as isize,
            "the analysis's release must NOT free an allocation a stored \
             world still claims"
        );
        for claim in stored {
            // SAFETY: the stored world has been dropped; this is the last
            // claim.
            unsafe { claim.release() };
        }
        assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleText), 0);
        assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleAst), 0);
        leak_tally::reset();
    }

    /// `adopt_claims` refuses when there is no scope — the caller's signal to
    /// treat a base-cache hit on a CLAIMED world as a miss rather than hand
    /// out a borrow nothing keeps alive.
    #[test]
    fn adopting_claims_without_a_scope_refuses() {
        assert!(!collecting());
        assert!(!adopt_claims(Vec::new()));
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
