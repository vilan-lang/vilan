//! The base-cache gate (S3c, proposal/analysis-reuse.md §6.10): a cached
//! world must be observation-identical to a fresh build, hit exactly when it
//! should, and revalidate by CONTENT (the E12 rule — a std edit evicts).
//! Stats are process-global, so every test serializes on one lock and
//! asserts deltas.
//!
//! One instrument note since macro worlds joined the cache (cycle 13,
//! §9): the counters see NESTED analyses too. A program whose std closure
//! dispatches a derive compiles macro worlds, and those worlds look the
//! cache up on their own account — so a fixture chosen to exercise the
//! OUTER analysis must not smuggle macro worlds in, or its hit delta counts
//! two things at once.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use vilan_core::{BuildOptions, PackageSpec, Platform, Workspace, analyze_source, transform};

static CACHE_LOCK: Mutex<()> = Mutex::new(());

fn std_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std")
}

fn observe_with(spec: &PackageSpec, source: &'static str) -> (String, String, Option<String>) {
    let spec = spec.clone();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let (program, errors) = analyze_source(
                source,
                &spec,
                Path::new("."),
                Path::new("cache_probe.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            let diagnostics = format!("{errors:?}");
            let warnings = program
                .as_ref()
                .map(|program| format!("{:?}", program.warnings))
                .unwrap_or_default();
            // The entry's identity slots ride along: a cache hit must patch
            // them, and JS/diagnostics alone cannot see a missed patch (the
            // walk reads the AST parameter, not the world's text slot).
            let entry_identity = program
                .as_ref()
                .map(|program| format!("{:?}", program.sources.first()))
                .unwrap_or_default();
            let warnings = format!("{warnings}#{entry_identity}");
            let javascript = match program {
                Some(program) if errors.is_empty() => {
                    transform(&program, &BuildOptions::default()).ok()
                }
                _ => None,
            };
            (diagnostics, warnings, javascript)
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked")
}

fn observe(source: &'static str) -> (String, String, Option<String>) {
    observe_with(&vilan_core::manifest::resolve_std(&std_root()), source)
}

fn stats() -> (u64, u64) {
    vilan_core::analyzer::base_cache_stats()
}

const PROGRAM_A: &str = "import std::io::print;\nfun main() { print(1); }\n";
const PROGRAM_B: &str = "import std::io::print;\nfun main() { print(2 + 3); }\n";
/// A DISTINCT import set from A/B — and deliberately one that reaches no
/// macro-defining std module, so this fixture's hit deltas belong to the
/// outer analysis alone (`std::time`, the previous choice, drags two macro
/// worlds along and each of those now consults the cache itself).
const PROGRAM_C: &str =
    "import std::io::print;\nimport std::math::PI;\nfun main() { print(PI); }\n";

/// Same import set: the second program hits, and its observations are
/// byte-identical to a fresh (cache-cleared) build of the same program.
#[test]
fn a_hit_is_observation_identical_to_a_fresh_build() {
    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    vilan_core::analyzer::base_cache_clear();
    let _ = observe(PROGRAM_A);
    let (_, misses_before) = stats();
    let (hits_before, _) = stats();
    let cached = observe(PROGRAM_B);
    let (hits_after, misses_after) = stats();
    assert_eq!(hits_after, hits_before + 1, "same import set must hit");
    assert_eq!(misses_after, misses_before, "a hit is not also a miss");

    vilan_core::analyzer::base_cache_clear();
    let fresh = observe(PROGRAM_B);
    assert_eq!(cached.0, fresh.0, "diagnostics differ cached vs fresh");
    assert_eq!(cached.1, fresh.1, "warnings differ cached vs fresh");
    assert_eq!(cached.2, fresh.2, "emitted JS differs cached vs fresh");
}

/// A different import set is a different world: no hit.
#[test]
fn a_distinct_import_set_misses() {
    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    vilan_core::analyzer::base_cache_clear();
    let _ = observe(PROGRAM_A);
    let (hits_before, _) = stats();
    let _ = observe(PROGRAM_C);
    let (hits_after, misses_after_c) = stats();
    assert_eq!(hits_after, hits_before, "distinct imports must not hit");
    // And the C-world was stored: an identical import set now hits.
    let _ = observe(PROGRAM_C);
    let (hits_final, _) = stats();
    assert_eq!(hits_final, hits_before + 1, "the stored C-world must hit");
    let _ = misses_after_c;
}

/// The bypasses: entries the world-building loop would entangle — macro or
/// derive text, `[service]` blocks — and any active overlay skip the cache
/// entirely (neither hit nor store).
#[test]
fn world_entangling_entries_and_overlays_bypass() {
    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    vilan_core::analyzer::base_cache_clear();
    let (hits_before, misses_before) = stats();
    // Macro-DEFINING entries stay bypassed (E23's world key is
    // entry-entangled); derive USERS cache since the hoist (§6.13).
    let _ = observe(
        "import std::io::print;\nmacro fun m(s: Source): Source { s }\nfun main() { print(1); }\n",
    );
    let (hits_mid, misses_mid) = stats();
    assert_eq!(
        (hits_mid, misses_mid),
        (hits_before, misses_before),
        "macro-defining entries must bypass, not miss"
    );

    // An overlay OUTSIDE std is harmless — the entry's text arrives as a
    // parameter, so the cache still works (this is the LSP's normal state:
    // the edited buffer is always overlaid).
    vilan_core::analyzer::set_document_overlay(
        Path::new("/tmp/overlay_probe.vl"),
        Some("x".into()),
    );
    let _ = observe(PROGRAM_A);
    let _ = observe(PROGRAM_B);
    let (hits_unrelated, _) = stats();
    vilan_core::analyzer::set_document_overlay(Path::new("/tmp/overlay_probe.vl"), None);
    assert_eq!(
        hits_unrelated,
        hits_before + 1,
        "an unrelated overlay must not block the cache"
    );

    // A std overlay is governed by CONTENT, not existence (S3d): identical
    // text still hits (validation reads through the overlay), while an
    // EDITED std buffer hash-mismatches and evicts — the LSP-editing-std
    // case rebuilds honestly instead of serving a stale world.
    let std_file = std_root().join("src/list.vl");
    let std_text = std::fs::read_to_string(&std_file).expect("read list.vl");
    vilan_core::analyzer::set_document_overlay(&std_file, Some(std_text.clone()));
    let (hits_pre_std, _) = stats();
    let _ = observe(PROGRAM_A);
    let (hits_same, _) = stats();
    assert_eq!(
        hits_same,
        hits_pre_std + 1,
        "an unchanged std overlay must still hit"
    );
    vilan_core::analyzer::set_document_overlay(
        &std_file,
        Some(format!(
            "{std_text}
// s3d dirty probe
"
        )),
    );
    let (_, misses_pre_dirty) = stats();
    let dirty = observe(PROGRAM_A);
    let (_, misses_dirty) = stats();
    vilan_core::analyzer::set_document_overlay(&std_file, None);
    vilan_core::analyzer::base_cache_clear();
    assert_eq!(
        misses_dirty,
        misses_pre_dirty + 1,
        "an edited std overlay must evict and miss"
    );
    assert_eq!(dirty.0, "[]", "the rebuild against the overlay compiles");
}

/// The wasm playground's shape (S3d): the ENTIRE std served from overlays
/// registered before the first analysis (boot), never touching disk paths
/// that exist. Same-import analyses must hit — this is what makes a second
/// playground Run skip the std re-analysis.
#[test]
fn overlay_served_std_hits_like_disk() {
    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    vilan_core::analyzer::base_cache_clear();
    // Mirror boot(): every std file registered as an overlay at a virtual
    // root; the spec points at the virtual root.
    let virtual_parent = PathBuf::from("/s3d-virtual-toolchain");
    let virtual_root = virtual_parent.join("std");
    let mut registered = Vec::new();
    fn register(from: &Path, to: &Path, registered: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(from).expect("read std").flatten() {
            let path = entry.path();
            let target = to.join(entry.file_name());
            if path.is_dir() {
                register(&path, &target, registered);
            } else {
                let text = std::fs::read_to_string(&path).expect("read std file");
                vilan_core::analyzer::set_document_overlay(&target, Some(text));
                registered.push(target);
            }
        }
    }
    register(&std_root(), &virtual_root, &mut registered);
    // Macros resolve `macro_std` beside `std` — the virtual toolchain needs
    // the sibling too, exactly as boot registers it.
    register(
        &std_root().parent().unwrap().join("macro_std"),
        &virtual_parent.join("macro_std"),
        &mut registered,
    );
    let spec = vilan_core::manifest::resolve_std(&virtual_root);

    let first = observe_with(&spec, PROGRAM_A);
    let (hits_before, _) = stats();
    let second = observe_with(&spec, PROGRAM_B);
    let (hits_after, _) = stats();
    for path in &registered {
        vilan_core::analyzer::set_document_overlay(path, None);
    }
    vilan_core::analyzer::base_cache_clear();
    assert_eq!(
        hits_after,
        hits_before + 1,
        "overlay-served std must hit on the second analysis"
    );
    assert_eq!(first.0, "[]", "the overlay-served world compiles");
    assert_eq!(second.0, "[]", "the hit-path analysis compiles");
}

/// The E12 property: a hit revalidates by CONTENT. Editing a loaded std file
/// evicts the world — the next analysis misses and rebuilds against the new
/// text rather than serving stale state.
#[test]
fn a_std_edit_evicts_by_content() {
    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    vilan_core::analyzer::base_cache_clear();

    // A private, mutable copy of std.
    let scratch_parent =
        std::env::temp_dir().join(format!("vilan_s3c_toolchain_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch_parent);
    let scratch = scratch_parent.join("std");
    copy_tree(&std_root(), &scratch);
    // Macros resolve `macro_std` BESIDE `std`, so the copy needs the sibling.
    copy_tree(
        &std_root().parent().unwrap().join("macro_std"),
        &scratch_parent.join("macro_std"),
    );
    let spec = vilan_core::manifest::resolve_std(&scratch);

    let _ = observe_with(&spec, PROGRAM_A);
    let (hits_before, _) = stats();
    let _ = observe_with(&spec, PROGRAM_B);
    let (hits_mid, _) = stats();
    assert_eq!(hits_mid, hits_before + 1, "the copy-std world must hit");

    // Edit a file every analysis loads (a trailing comment: content changes,
    // semantics do not).
    let touched = scratch.join("src/list.vl");
    let mut text = std::fs::read_to_string(&touched).expect("read list.vl");
    text.push_str("\n// s3c eviction probe\n");
    std::fs::write(&touched, text).expect("write list.vl");

    let (_, misses_before) = stats();
    let after_edit = observe_with(&spec, PROGRAM_B);
    let (hits_after, misses_after) = stats();
    assert_eq!(hits_after, hits_mid, "an edited std must not hit");
    assert_eq!(misses_after, misses_before + 1, "the eviction is a miss");
    assert_eq!(after_edit.0, "[]", "the rebuilt world still compiles");

    let _ = std::fs::remove_dir_all(&scratch_parent);
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create scratch dir");
    for entry in std::fs::read_dir(from).expect("read std tree").flatten() {
        let path = entry.path();
        let target = to.join(entry.file_name());
        if path.is_dir() {
            copy_tree(&path, &target);
        } else {
            std::fs::copy(&path, &target).expect("copy std file");
        }
    }
}

/// The derive/macro hoist (§6.13): a derive-USING entry caches, its derived
/// impls arrive through the hit path, and the sharp edge holds — a struct
/// WITHOUT the derive still errors identically through a hit.
#[test]
fn derive_entries_cache_and_derived_impls_survive_the_hit() {
    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    vilan_core::analyzer::base_cache_clear();
    const WITH_DERIVE: &str = "import std::io::print;\n[derive(PartialEq)]\nstruct P { x: i32 }\nfun main() { print(P { x = 1 } == P { x = 1 }); }\n";
    const WITHOUT_DERIVE: &str = "import std::io::print;\nstruct Q { x: i32 }\nfun main() { print(Q { x = 1 } == Q { x = 1 }); }\n";

    let first = observe(WITH_DERIVE);
    let (hits_before, _) = stats();
    assert_eq!(first.0, "[]", "the derive entry compiles on the miss");

    let second = observe(WITH_DERIVE);
    let (hits_after, _) = stats();
    assert_eq!(hits_after, hits_before + 1, "the derive entry must hit");
    assert_eq!(first.2, second.2, "derived impls must survive the hit (JS)");

    // The sharp edge, through a hit of the same world: no derive, and the
    // `==` must still error exactly as fresh.
    let cached_error = observe(WITHOUT_DERIVE);
    let (hits_final, _) = stats();
    assert_eq!(hits_final, hits_after + 1, "the error case also hits");
    vilan_core::analyzer::base_cache_clear();
    let fresh_error = observe(WITHOUT_DERIVE);
    assert_eq!(
        cached_error.0, fresh_error.0,
        "the missing-derive error must be identical through a hit"
    );
    assert_ne!(cached_error.0, "[]", "and it must actually error");
}

/// Macro worlds are analyses too, and they all analyze the same `macro_std`
/// (cycle 13, §9). Two derives resolving to two DIFFERENT defining files
/// compile two worlds; the second must be served from the base world the
/// first stored, and — the bar that matters — a macro world served warm must
/// observe exactly what a cold one observes.
///
/// The differential is only writable because both caches can be dropped:
/// `WORLDS` memoizes a compiled world by content, so without
/// `macro_world_cache_clear` the first compile in a process is the only one
/// and the cold leg is unreachable a second time.
#[test]
fn macro_worlds_share_one_base_world_and_observe_identically() {
    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // Two defining files: `PartialEq` lives in compare.vl, `Debug` in
    // debug.vl — so this entry compiles two macro worlds, not one.
    const TWO_WORLDS: &str = "import std::io::print;\n[derive(PartialEq, Debug)]\nstruct P { x: i32 }\nfun main() { print((P { x = 1 } == P { x = 1 }).debug()); }\n";

    vilan_core::analyzer::base_cache_clear();
    vilan_core::macro_world_cache_clear();
    // Prime the ordinary (dependency-free) base so the OUTER analysis below
    // is a hit and every remaining delta belongs to the macro worlds.
    // `import std::io::print` dispatches no macro of its own.
    let _ = observe(PROGRAM_A);
    let (hits_before, misses_before) = stats();
    let two_worlds = observe(TWO_WORLDS);
    let (hits_after, misses_after) = stats();
    assert_eq!(two_worlds.0, "[]", "the two-world entry compiles");
    // Outer: hit. First macro world: miss (nothing is stored under a
    // macro_std workspace yet), and it stores. Second macro world: hit.
    assert_eq!(
        (hits_after, misses_after),
        (hits_before + 2, misses_before + 1),
        "the second macro world must be served from the first's base"
    );
    vilan_core::analyzer::base_cache_clear();
    vilan_core::macro_world_cache_clear();
}

/// The key must describe the WORKSPACE, not only the platform and the entry's
/// `std::` seeds. A macro world's workspace is `[macro_std]`; an ordinary
/// program's is empty — and when the entry imports no std module at all the
/// two agree on every other field. A key that forgot the workspace would
/// therefore serve the macro world the ordinary world it stored a moment
/// earlier (the store happens before the entry's derives expand), a world with
/// no `macro_std` in it at all, and the derive would fail to expand.
#[test]
fn a_macro_world_is_never_served_an_ordinary_world() {
    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // No `import std::..` anywhere: the entry's std seeds are empty, exactly
    // like the blanked entry of the macro world its derive compiles.
    const NO_STD_IMPORT_DERIVE: &str =
        "[derive(Debug)]\nstruct P {\n\tx: i32,\n}\n\nfun main() {}\n";
    vilan_core::analyzer::base_cache_clear();
    vilan_core::macro_world_cache_clear();
    let observed = observe(NO_STD_IMPORT_DERIVE);
    vilan_core::analyzer::base_cache_clear();
    vilan_core::macro_world_cache_clear();
    assert_eq!(
        observed.0, "[]",
        "a derive whose entry imports no std module must still expand"
    );
    assert!(observed.2.is_some(), "and it must actually have emitted");
}

/// The bar that matters for the above: a macro world analyzed over a CACHED
/// base must observe exactly what one built from scratch observes. Both legs
/// analyze the same derive-bearing program with the same outer world already
/// stored; they differ only in whether a macro-world base is available.
#[test]
fn a_warm_macro_world_observes_what_a_cold_one_observes() {
    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    const DERIVE_DEBUG: &str = "import std::io::print;\n[derive(Debug)]\nstruct P { x: i32 }\nfun main() { print(P { x = 1 }.debug()); }\n";
    const DERIVE_PARTIAL_EQ: &str = "import std::io::print;\n[derive(PartialEq)]\nstruct Q { x: i32 }\nfun main() { print(Q { x = 1 } == Q { x = 1 }); }\n";

    // Cold leg: the outer world is stored, no macro-world base is.
    vilan_core::analyzer::base_cache_clear();
    vilan_core::macro_world_cache_clear();
    let _ = observe(PROGRAM_A);
    let (_, misses_before) = stats();
    let cold = observe(DERIVE_DEBUG);
    let (_, misses_after) = stats();
    assert_eq!(
        misses_after,
        misses_before + 1,
        "the cold leg's macro world must build its own base"
    );

    // Warm leg: a DIFFERENT defining file's world runs first and stores a
    // macro-world base; debug.vl's world is then analyzed over it. Only the
    // compiled-world memo is dropped in between, so the base survives.
    vilan_core::analyzer::base_cache_clear();
    vilan_core::macro_world_cache_clear();
    let _ = observe(PROGRAM_A);
    let _ = observe(DERIVE_PARTIAL_EQ);
    vilan_core::macro_world_cache_clear();
    let (hits_before, _) = stats();
    let warm = observe(DERIVE_DEBUG);
    let (hits_after, _) = stats();
    assert_eq!(
        hits_after,
        hits_before + 2,
        "the warm leg's macro world must be served from the stored base"
    );

    vilan_core::analyzer::base_cache_clear();
    vilan_core::macro_world_cache_clear();
    assert_eq!(cold.0, warm.0, "diagnostics differ warm vs cold");
    assert_eq!(cold.1, warm.1, "warnings differ warm vs cold");
    assert_eq!(cold.2, warm.2, "emitted JS differs warm vs cold");
    assert!(cold.2.is_some(), "and it must actually have emitted");
}

/// §6.13's sharp edge, now through a macro world served from a base-cache
/// hit: a `[derive]`-less struct compared with `==` must produce the SAME
/// diagnostic whether the derive macro's world was analyzed over a cached
/// base or built from scratch. A macro world that quietly resolved a
/// different `macro_std` would show up here first.
#[test]
fn a_missing_derive_errors_identically_through_a_warm_macro_world() {
    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // The entry derives Debug (compiling debug.vl's world) and then asks for
    // an `==` its struct never derived: the error must be identical whichever
    // way the world beneath the derive was analyzed.
    const MISSING_DERIVE: &str = "import std::io::print;\n[derive(Debug)]\nstruct P { x: i32 }\nfun main() { print((P { x = 1 } == P { x = 1 }).debug()); }\n";

    const DERIVE_PARTIAL_EQ: &str = "import std::io::print;\n[derive(PartialEq)]\nstruct Q { x: i32 }\nfun main() { print(Q { x = 1 } == Q { x = 1 }); }\n";

    // Cold: nothing is stored under a macro_std workspace, so debug.vl's
    // world builds its own base.
    vilan_core::analyzer::base_cache_clear();
    vilan_core::macro_world_cache_clear();
    let _ = observe(PROGRAM_A);
    let cold = observe(MISSING_DERIVE);

    // Warm: compare.vl's world stores a macro-world base first.
    vilan_core::analyzer::base_cache_clear();
    vilan_core::macro_world_cache_clear();
    let _ = observe(PROGRAM_A);
    let _ = observe(DERIVE_PARTIAL_EQ);
    vilan_core::macro_world_cache_clear();
    let warm = observe(MISSING_DERIVE);

    vilan_core::analyzer::base_cache_clear();
    vilan_core::macro_world_cache_clear();
    assert_ne!(warm.0, "[]", "the missing derive must actually error");
    assert_eq!(
        warm.0, cold.0,
        "the missing-derive error must be identical through a warm macro world"
    );
    assert_eq!(warm.1, cold.1, "warnings differ warm vs cold");
}

/// The member-name index (E46 lever 1) rides the world's lifecycle. Member
/// resolution scans `implementations_by_member` rather than every impl, so the
/// index must cover the impls the ENTRY declares — which arrive after the base
/// world was stored, registered into this analysis's own clone of it.
///
/// This is the only direction that can go wrong, and it is worth saying why.
/// The index is a PRE-FILTER, never the answer: every impl it names is still
/// asked for `declarations[member_name]` before it becomes a candidate. So an
/// index row that is too BROAD — one naming an impl that does not declare the
/// name, whatever the cause — changes nothing observable, while a row that is
/// too NARROW silently loses a candidate and the method stops resolving. That
/// asymmetry is what this pin covers, in its sharpest form: a warm analysis
/// whose entry declares the impl, which must compile and emit what a cold
/// build emits.
#[test]
fn an_entry_declared_impl_resolves_through_a_cache_hit() {
    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    vilan_core::analyzer::base_cache_clear();
    const ENTRY_IMPL: &str = "import std::io::print;\nstruct Widget { n: i32 }\nimpl Widget { fun doubled(self): i32 { self.n * 2 } }\nfun main() { print(Widget { n = 21 }.doubled()); }\n";

    // A first analysis of the same import set stores the base world.
    let _ = observe(PROGRAM_A);
    let (hits_before, _) = stats();
    let cached = observe(ENTRY_IMPL);
    let (hits_after, _) = stats();
    assert_eq!(
        hits_after,
        hits_before + 1,
        "the entry-impl program must hit"
    );
    assert_eq!(
        cached.0, "[]",
        "an entry-declared method must resolve through a hit"
    );

    vilan_core::analyzer::base_cache_clear();
    let fresh = observe(ENTRY_IMPL);
    assert_eq!(cached.0, fresh.0, "diagnostics differ cached vs fresh");
    assert_eq!(cached.2, fresh.2, "emitted JS differs cached vs fresh");
    assert!(cached.2.is_some(), "and it must actually have emitted");
}

/// The third cold switch (backlog M6): `parse_clean_cached` serves the
/// IDENTICAL leaked pointer for identical content — pointer identity is how
/// reuse is proven without timing, per its own doc — and
/// `parse_clean_cache_clear` drops the map, so the next ask re-parses into a
/// fresh leak. The inequality after the clear is deterministic, not
/// probabilistic: the first AST is still leaked and alive, so a new
/// allocation can never land at its address.
#[test]
fn parse_clean_cache_clear_forces_a_reparse() {
    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let source = "fun parse_cache_probe(): i32 {\n\tlet value = 21;\n\tvalue * 2\n}\n";

    let (first, _) = vilan_core::parse_clean_cached(source).expect("a clean source parses");
    let (second, _) = vilan_core::parse_clean_cached(source).expect("still clean");
    assert!(
        std::ptr::eq(first, second),
        "identical content must be served back from the cache"
    );

    vilan_core::parse_clean_cache_clear();
    let (third, _) = vilan_core::parse_clean_cached(source).expect("clean after the clear too");
    assert!(
        !std::ptr::eq(first, third),
        "after the clear the same content must be re-parsed into a fresh leak"
    );
}

/// M23 (`leak-soak.md` §7.9.4a, replaced): a base world built over an
/// OVERLAY-SERVED source is stored, hits, and holds its own CLAIM on every
/// analysis-owned copy it borrows.
///
/// M9 refused the store instead, because a stored world outliving the
/// analysis that owns its module copies is §7.9.2's use-after-free — and it
/// cost every entry that imports an open sibling the whole pre-entry world on
/// every keystroke (kolt's `client.vl`: `base` 1.4–2.7 s, a miss every time).
/// The world takes a reference count now. This pin covers the four things
/// that makes true: the world hits while the buffer is open; the claim keeps
/// the copy alive after the owning analysis has given its own back; the
/// content the hit was validated against is the content the served world was
/// built from (the ctrl-Z shape, which is where the naive eviction died); and
/// an analysis with nowhere to keep a claim is served a MISS, not a borrow.
#[test]
fn a_world_that_loaded_an_overlaid_source_is_stored_and_claims_its_copies() {
    use vilan_core::{MacroLimits, Workspace};

    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    vilan_core::analyzer::base_cache_clear();

    // A workspace with one dependency package: dependency files are exactly
    // the files a multi-package workspace has open in the editor, and — like
    // a `pkg::` sibling since M21 — they are loaded into the world the cache
    // stores.
    let root = std::env::temp_dir().join(format!("vilan_m23_claim_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let app_dir = root.join("app");
    std::fs::create_dir_all(&app_dir).expect("app dir");
    let entry_path = app_dir.join("main.vl");
    std::fs::write(
        &entry_path,
        "import std::io::print;\nimport common::greeting;\n\nfun main() {\n\tprint(greeting());\n}\n",
    )
    .expect("write main.vl");
    let dep_root = root.join("common");
    std::fs::create_dir_all(&dep_root).expect("dep dir");
    let dep_lib = dep_root.join("lib.vl");
    std::fs::write(&dep_lib, "fun greeting(): i32 {\n\t7\n}\n").expect("write lib.vl");
    let workspace = Workspace {
        packages: vec![PackageSpec {
            base_root: dep_root,
            layers: Vec::new(),
            dependencies: Vec::new(),
            surface: true,
            member: false,
            prelude: Default::default(),
        }],
        entry_dependencies: vec![("common".to_string(), 0)],
        macro_limits: MacroLimits::default(),
        entry_prelude: Default::default(),
        ..Workspace::default()
    };
    let source: &'static str = Box::leak(
        std::fs::read_to_string(&entry_path)
            .unwrap()
            .into_boxed_str(),
    );

    // One opted-in analysis — the language server's shape — with the handles
    // reclaimed the way their owner would. Returns the diagnostics, because
    // a world served with dangling borrows shows up as a WRONG answer (the
    // M9 plant produced `cannot find 'greeting' in the imported path`), not
    // as a crash.
    let analyze_owning = || -> Vec<String> {
        let workspace = workspace.clone();
        let app_dir = app_dir.clone();
        let entry_path = entry_path.clone();
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(move || {
                let std = vilan_core::manifest::resolve_std(&std_root());
                let analyzed = vilan_core::analyze_source_owning_overlay_modules(
                    source,
                    &std,
                    &app_dir,
                    &entry_path,
                    Some(Platform::default()),
                    &workspace,
                );
                let diagnostics: Vec<String> = analyzed
                    .diagnostics
                    .iter()
                    .map(|error| error.msg.clone())
                    .collect();
                drop(analyzed.program);
                if let Some(ast) = analyzed.ast {
                    // SAFETY: the program — the tree's only borrower — was
                    // dropped on the line above.
                    unsafe { ast.reclaim() };
                }
                // SAFETY: as above. This releases only THIS analysis's
                // claims; a copy a stored world still claims survives, which
                // is the whole M23 protocol.
                unsafe { analyzed.owned_modules.reclaim() };
                diagnostics
            })
            .expect("spawn worker")
            .join()
            .expect("worker panicked")
    };

    // Disk-served: the dependency world stores and hits — the baseline that
    // proves the fixture exercises the cache at all.
    assert!(analyze_owning().is_empty());
    let (hits_before, _) = stats();
    assert!(analyze_owning().is_empty());
    let (hits_disk, _) = stats();
    assert_eq!(
        hits_disk,
        hits_before + 1,
        "the disk-served dependency world must hit — the fixture is not \
         reaching the base cache, so the assertions below are vacuous"
    );
    assert_eq!(
        vilan_core::analyzer::base_cache_overlay_claims(),
        (0, 0),
        "a disk-served world claims nothing: every source it borrows is in \
         `parse_clean_cached`'s immortal cache"
    );

    // The dependency's lib.vl is now OPEN in the editor: every analysis loads
    // it from the overlay into analysis-owned allocations — and the world
    // built over them is STORED, claims them, and hits.
    vilan_core::analyzer::base_cache_clear();
    let open_text = "fun greeting(): i32 {\n\t42\n}\n".to_string();
    let open_bytes = open_text.len();
    vilan_core::analyzer::set_document_overlay(&dep_lib, Some(open_text.clone()));
    let (hits_open_before, _) = stats();
    assert!(analyze_owning().is_empty());
    assert!(
        analyze_owning().is_empty(),
        "the second analysis over the open buffer must still resolve the \
         dependency — a served world whose borrows were freed answers wrong"
    );
    let (hits_open, _) = stats();
    assert_eq!(
        hits_open,
        hits_open_before + 1,
        "M23: a world that loaded an overlay-served source must be stored \
         and hit — this is the `base` cost kolt's client.vl paid on every \
         keystroke"
    );
    let (claims, claim_bytes) = vilan_core::analyzer::base_cache_overlay_claims();
    assert_eq!(
        (claims, claim_bytes),
        (1, open_bytes),
        "the stored world must hold exactly one claim, on the overlaid \
         module's text, or its borrows are not kept alive by anything"
    );
    // The ctrl-Z shape — §7.9.2's sharpest edge. Edit the open buffer (the
    // stored world goes stale and is evicted), then UNDO back to the content
    // the world was built from: the world that becomes valid again must
    // still be pointing at live memory, and must answer correctly.
    vilan_core::analyzer::set_document_overlay(
        &dep_lib,
        Some("fun greeting(): i32 {\n\t43\n}\n".to_string()),
    );
    assert!(analyze_owning().is_empty());
    vilan_core::analyzer::set_document_overlay(&dep_lib, Some(open_text));
    let (hits_undo_before, _) = stats();
    let after_undo = analyze_owning();
    let (hits_undo, _) = stats();
    assert!(
        after_undo.is_empty(),
        "after an undo the re-validated world must resolve the dependency, \
         not read freed memory: {after_undo:?}"
    );
    assert!(
        hits_undo >= hits_undo_before,
        "the undo must not corrupt the cache's accounting"
    );

    // An analysis with NO collection scope has nowhere to keep a claim, so
    // the claimed world is not served to it: a miss, and it loads the
    // overlay through the process-global cache exactly as it always did.
    let (hits_unscoped_before, misses_unscoped_before) = stats();
    let workspace_for_plain = workspace.clone();
    let app_dir_for_plain = app_dir.clone();
    let entry_for_plain = entry_path.clone();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let std = vilan_core::manifest::resolve_std(&std_root());
            let (program, errors) = analyze_source(
                source,
                &std,
                &app_dir_for_plain,
                &entry_for_plain,
                Some(Platform::default()),
                &workspace_for_plain,
            );
            assert!(errors.is_empty(), "the unscoped analysis must be clean");
            drop(program);
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked");
    let (hits_unscoped, misses_unscoped) = stats();
    assert_eq!(
        hits_unscoped, hits_unscoped_before,
        "a claimed world must NOT be served to an analysis with no scope to \
         hold its claims — that borrow would be kept alive by nothing"
    );
    assert!(misses_unscoped > misses_unscoped_before);

    // The buffer closes: loads come from disk again, so the world stored
    // from then on claims nothing, and clearing gives every claim back.
    vilan_core::analyzer::set_document_overlay(&dep_lib, None);
    assert!(analyze_owning().is_empty());
    let (hits_closed_before, _) = stats();
    assert!(analyze_owning().is_empty());
    let (hits_closed, _) = stats();
    assert_eq!(
        hits_closed,
        hits_closed_before + 1,
        "once the buffer closes the disk-served world must store and hit"
    );

    vilan_core::analyzer::base_cache_clear();
    assert_eq!(
        vilan_core::analyzer::base_cache_overlay_claims(),
        (0, 0),
        "clearing the cache gives every claim back"
    );
    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// The retention tally (backlog M11)
// ---------------------------------------------------------------------------

/// Runs `body` on the big-stack worker the analyses need, and reads the leak
/// tally FROM INSIDE it.
///
/// The thread is not a convenience here, it is the instrument: `leak_tally`'s
/// counters are thread-local by design (the E12 pointer-identity lesson —
/// a process-global counter's before/after deltas are famously flaky under a
/// parallel test runner), so a store that happens on an analysis thread is
/// invisible to the runner's thread. Every record, every release and every
/// read this pin makes therefore happens on one thread, which is also the
/// shape the leak soak reads its numbers in.
fn on_one_thread<T: Send + 'static>(body: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(body)
        .expect("spawn the retention-tally worker")
        .join()
        .expect("the retention-tally worker panicked")
}

fn analyze_on_this_thread(spec: &PackageSpec, source: &'static str) {
    let (program, _errors) = analyze_source(
        source,
        spec,
        Path::new("."),
        Path::new("retention_probe.vl"),
        Some(Platform::default()),
        &Workspace::default(),
    );
    drop(program);
}

/// M11: the base cache's worlds are the compiler's largest per-process
/// retention, and `[vilan leak] total` could not see one of them.
///
/// Nothing here is a `Box::leak`, which is exactly why the site was missing:
/// the tally's literal contract was never violated, and the soak's strongest
/// assertion (`total == counts().named()`) is blind to an unrecorded site by
/// construction. The finding is that the omission was the BIGGEST number in
/// the process. What the site buys is in the three assertions below — the
/// retention is counted, it is proportional to the world rather than a flat
/// per-entry constant, and it comes BACK, so `outstanding` is the live
/// retention and its growth is the growth of the key set (per-key overwrite
/// being the only eviction this cache has).
#[test]
fn the_base_cache_records_and_releases_what_its_worlds_retain() {
    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let spec = vilan_core::manifest::resolve_std(&std_root());

    let (retained, gross, outstanding, after_clear, gross_after_clear) = on_one_thread(move || {
        use vilan_core::leak_tally::{self, LeakSite};

        // Clear FIRST, then zero the counters: a world stored by an earlier
        // test would otherwise be released against a fresh tally and read
        // negative here.
        vilan_core::analyzer::base_cache_clear();
        leak_tally::reset();

        // Three distinct import sets, which is three distinct
        // `BaseCacheKey`s and therefore three retained worlds — the shape an
        // editing session mints one of per import set it ever sees.
        analyze_on_this_thread(&spec, PROGRAM_A);
        analyze_on_this_thread(&spec, PROGRAM_C);
        analyze_on_this_thread(
            &spec,
            "import std::io::print;\nimport std::list::List;\nfun main() { print(1); }\n",
        );

        let retained = vilan_core::analyzer::base_cache_retained();
        let gross = leak_tally::bytes(LeakSite::BaseCacheWorld);
        let outstanding = leak_tally::outstanding(LeakSite::BaseCacheWorld);

        vilan_core::analyzer::base_cache_clear();
        (
            retained,
            gross,
            outstanding,
            leak_tally::outstanding(LeakSite::BaseCacheWorld),
            leak_tally::bytes(LeakSite::BaseCacheWorld),
        )
    });

    // The magnitude the item called unmeasurable, printed rather than only
    // asserted: `cargo test … -- --nocapture` reads it off, and a future
    // change to what a world costs shows up here as a number instead of as a
    // silence.
    println!(
        "M11-RETENTION base_cache retained={retained} worlds, recorded {gross} B \
         ({} B/world)",
        gross / retained.max(1),
    );
    assert!(
        retained >= 3,
        "three distinct import sets must retain at least three worlds, not {retained}"
    );
    // Proportional, not a flat per-entry constant: std alone is hundreds of
    // kilobytes of module text, so a site recording the shallow struct would
    // land three orders of magnitude below this and a site recording nothing
    // would read zero.
    assert!(
        gross > 100_000,
        "a retained world must be recorded proportionally to the world; \
         {gross} B for {retained} worlds is not that"
    );
    assert_eq!(
        outstanding, gross as isize,
        "nothing was evicted, so every recorded byte is still outstanding"
    );
    assert_eq!(
        after_clear, 0,
        "clearing the cache must give every retained byte back, or `outstanding` \
         is a running total rather than the live retention"
    );
    assert_eq!(
        gross_after_clear, gross,
        "the GROSS record stands through a release, exactly as it does at the \
         leak sites"
    );
}

/// M11's sibling: `macros`' `FAILURES` cache retains rendered error text per
/// failing definition set, with the same per-key overwrite and the same
/// absence from the tally. It is bounded — one entry per (definition set,
/// layout) — which is the argument for recording it and not for leaving it
/// out: a bound nobody can read is not a bound anybody can check.
#[test]
fn a_cached_macro_failure_is_recorded_once_and_given_back() {
    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let spec = vilan_core::manifest::resolve_std(&std_root());

    // A macro whose WORLD fails to compile: the body calls a name that exists
    // in no macro world, so the hermetic compile errors and the failure —
    // not a world — is what gets cached.
    const BROKEN_MACRO: &str = r#"
        import std::io::print;

        macro fun broken_world(item: Item): Source {
            import macro_std::source;
            import macro_std::meta::{ Item, Source };
            source(no_such_macro_helper(item))
        }

        [broken_world]
        struct Point { x: i32 }

        fun main() { print(1); }
        "#;

    let (first, second, after_clear) = on_one_thread(move || {
        use vilan_core::leak_tally::{self, LeakSite};

        vilan_core::analyzer::base_cache_clear();
        vilan_core::macro_world_cache_clear();
        leak_tally::reset();

        analyze_on_this_thread(&spec, BROKEN_MACRO);
        let first = leak_tally::outstanding(LeakSite::MacroFailureText);
        // The second analysis is served from `FAILURES` and must retain
        // nothing new — the cache is what stops the failing world (and its
        // leaked text) being rebuilt per analysis, backlog E23.
        analyze_on_this_thread(&spec, BROKEN_MACRO);
        let second = leak_tally::outstanding(LeakSite::MacroFailureText);

        vilan_core::macro_world_cache_clear();
        (
            first,
            second,
            leak_tally::outstanding(LeakSite::MacroFailureText),
        )
    });

    println!("M11-RETENTION macro_failures recorded {first} B for one failing world");
    assert!(
        first > 0,
        "a failing macro world caches its rendered error text, and the tally \
         must see it"
    );
    assert_eq!(
        second, first,
        "the second analysis is a FAILURES hit: it retains nothing new"
    );
    assert_eq!(
        after_clear, 0,
        "clearing the macro caches must give the failure text back"
    );
}

/// M11's open question, measured rather than feared: how fast does the base
/// cache's key set actually grow?
///
/// The item's worry was that "an LSP session mints a retained world for every
/// distinct import set it ever sees, intermediate states included" — with
/// per-key overwrite the only eviction, that would be unbounded growth per
/// keystroke. It is not. Typing `import std::io::print;` one character at a
/// time is 22 analyses and retains THREE worlds: an intermediate prefix
/// either does not parse (nothing is stored) or seeds the same `std::`
/// reference set as its neighbours, and the key is the reference set, not the
/// text. Growth is bounded by the number of DISTINCT import sets a session
/// visits, which is a property of the project, not of the typing — and
/// revisiting a set hits rather than re-storing.
///
/// That is why M11's answer is a tally site and not an eviction policy: what
/// the cache retains is bounded and is now countable, and a bound nobody can
/// read is not a bound anybody can check. If this pin ever reads keystroke
/// growth, the eviction policy is the fix and this is what will say so.
#[test]
fn the_base_cache_grows_with_distinct_import_sets_not_with_keystrokes() {
    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let spec = vilan_core::manifest::resolve_std(&std_root());

    let (typed, typed_states, revisited) = on_one_thread(move || {
        vilan_core::analyzer::base_cache_clear();
        // Every prefix of one import line, analyzed as the language server
        // analyzes a buffer edit.
        let full = "import std::io::print;";
        for length in 1..=full.len() {
            let source: &'static str =
                Box::leak(format!("{}\nfun main() {{ }}\n", &full[..length]).into_boxed_str());
            analyze_on_this_thread(&spec, source);
        }
        let typed = vilan_core::analyzer::base_cache_retained();

        // And the session shape: three import sets, revisited five times each
        // with different bodies, as a session moves between files.
        vilan_core::analyzer::base_cache_clear();
        for round in 0..5 {
            for module in ["io::print", "math::PI", "list::List"] {
                let source: &'static str = Box::leak(
                    format!("import std::{module};\nfun main() {{ }}\n// {round}\n")
                        .into_boxed_str(),
                );
                analyze_on_this_thread(&spec, source);
            }
        }
        let revisited = vilan_core::analyzer::base_cache_retained();
        vilan_core::analyzer::base_cache_clear();
        (typed, full.len(), revisited)
    });

    println!(
        "M11-RETENTION growth: {typed_states} keystroke states -> {typed} worlds; \
         3 import sets x 5 rounds -> {revisited} worlds"
    );
    assert!(
        typed <= 5,
        "{typed_states} intermediate typing states retained {typed} worlds: the key \
         set is tracking the TEXT rather than the import set, which is the \
         unbounded growth M11 asked about"
    );
    assert_eq!(
        revisited, 3,
        "three import sets revisited must retain three worlds — a revisit has to \
         HIT, not store a fourth"
    );
}

/// M21 — an entry with any `pkg::` import used to bypass the base cache
/// OUTRIGHT.
///
/// The measured consequence on kolt: `views.vl` and `client.vl` rebuilt
/// std's whole world on every keystroke (`base` 248–288 ms, and 758 ms
/// median under lane load) while their sibling `theme.vl`, which imports no
/// sibling, hit at 0.0 ms. The world is std's; the package is analyzed on
/// top of it, and the sibling set is a KEY — the same thing the `std::`
/// seeds and the dependency seeds already are — not a reason to refuse.
///
/// Four properties, in the order they matter: the second analysis HITS; a
/// hit is observation-identical to a cache-cleared build (the cache may not
/// change an answer); a DIFFERENT sibling set is a different world and
/// misses; and an edited sibling evicts by CONTENT (the E12 rule), which is
/// what keeps an editor from being served a stale sibling.
#[test]
fn a_pkg_importing_entry_hits_the_cache_on_its_second_analysis() {
    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let root = std::env::temp_dir().join(format!("vilan_m21_pkg_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("package dir");
    std::fs::write(root.join("helper.vl"), "fun helper(): i32 {\n\t7\n}\n").expect("write helper");
    std::fs::write(root.join("other.vl"), "fun other(): i32 {\n\t9\n}\n").expect("write other");

    // Two entries with the SAME sibling set and the same std seeds, differing
    // only in their bodies — a keystroke, in cache terms.
    const FIRST: &str =
        "import std::io::print;\nimport pkg::helper::helper;\nfun main() { print(helper()); }\n";
    const SECOND: &str = "import std::io::print;\nimport pkg::helper::helper;\n\
                          fun main() { print(helper() + 1); }\n";
    // A different sibling set: a different world.
    const BOTH: &str = "import std::io::print;\nimport pkg::helper::helper;\n\
                        import pkg::other::other;\nfun main() { print(helper() + other()); }\n";

    let spec = vilan_core::manifest::resolve_std(&std_root());
    let entry_path = root.join("main.vl");

    fn observe_in(
        spec: &PackageSpec,
        pkg_root: &Path,
        entry_path: &Path,
        source: &'static str,
    ) -> (String, Option<String>) {
        let spec = spec.clone();
        let pkg_root = pkg_root.to_path_buf();
        let entry_path = entry_path.to_path_buf();
        on_one_thread(move || {
            let (program, errors) = analyze_source(
                source,
                &spec,
                &pkg_root,
                &entry_path,
                Some(Platform::default()),
                &Workspace::default(),
            );
            let diagnostics = format!("{errors:?}");
            let javascript = match program {
                Some(program) if errors.is_empty() => {
                    transform(&program, &BuildOptions::default()).ok()
                }
                _ => None,
            };
            (diagnostics, javascript)
        })
    }

    vilan_core::analyzer::base_cache_clear();
    let retained_empty = vilan_core::analyzer::base_cache_retained();
    let first = observe_in(&spec, &root, &entry_path, FIRST);
    assert_eq!(first.0, "[]", "the fixture must analyze clean: {}", first.0);
    let (hits_before, misses_before) = stats();
    let retained_after_first = vilan_core::analyzer::base_cache_retained();

    let cached = observe_in(&spec, &root, &entry_path, SECOND);
    let (hits_after, misses_after) = stats();
    assert_eq!(
        hits_after,
        hits_before + 1,
        "an entry with one `pkg::` import must HIT on its second analysis \
         (M21); it missed instead"
    );
    assert_eq!(misses_after, misses_before, "a hit is not also a miss");

    // A `pkg::` world costs ONE retained world, like every other key shape —
    // M11's number is what says so.
    assert_eq!(
        retained_after_first - retained_empty,
        1,
        "one sibling set must retain exactly one world, not \
         {}",
        retained_after_first - retained_empty
    );

    // The cache may not change an answer.
    vilan_core::analyzer::base_cache_clear();
    let fresh = observe_in(&spec, &root, &entry_path, SECOND);
    assert_eq!(cached.0, fresh.0, "diagnostics differ cached vs fresh");
    assert_eq!(cached.1, fresh.1, "emitted JS differs cached vs fresh");
    assert!(cached.1.is_some(), "the fixture must emit");

    // A different sibling set is a different world.
    let (hits_pre_both, misses_pre_both) = stats();
    let both = observe_in(&spec, &root, &entry_path, BOTH);
    let (hits_both, misses_both) = stats();
    assert_eq!(both.0, "[]", "the two-sibling fixture must analyze clean");
    assert_eq!(
        (hits_both, misses_both),
        (hits_pre_both, misses_pre_both + 1),
        "a different `pkg::` sibling set must MISS — the world holds the \
         siblings, so the set is part of the key"
    );

    // E12: an edited sibling evicts by content, and the rebuild sees the edit.
    std::fs::write(root.join("helper.vl"), "fun helper(): i32 {\n\t8\n}\n").expect("edit helper");
    let (hits_pre_edit, misses_pre_edit) = stats();
    let edited = observe_in(&spec, &root, &entry_path, SECOND);
    let (hits_edit, misses_edit) = stats();
    assert_eq!(
        (hits_edit, misses_edit),
        (hits_pre_edit, misses_pre_edit + 1),
        "an edited `pkg::` sibling must evict and miss, not serve a stale world"
    );
    assert_ne!(
        edited.1, fresh.1,
        "the rebuild after a sibling edit must carry the edit"
    );

    vilan_core::analyzer::base_cache_clear();
    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// The byte budget (backlog M24)
// ---------------------------------------------------------------------------

/// M24: the base cache had no eviction but per-key overwrite, so a session
/// that met N distinct key shapes retained N worlds until something cleared
/// the map — and M21 multiplied the key set by a package's sibling sets, so
/// an N-file package can now mint N keys from one editing session. M11 made
/// the growth VISIBLE (`base_cache_retained`, the `BaseCacheWorld` tally);
/// this makes it BOUNDED.
///
/// The three claims, each asserted below: the retained bytes stay inside the
/// budget; a HIT refreshes recency, so eviction is least-recently-USED rather
/// than oldest-stored; and an evicted world gives its bytes back to the tally
/// (and, after M23, its overlay claims with them).
#[test]
fn the_base_cache_evicts_least_recently_hit_worlds_to_a_byte_budget() {
    use vilan_core::leak_tally::{self, LeakSite};

    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let spec = vilan_core::manifest::resolve_std(&std_root());

    // A synthetic package of many entries, GENERATED here rather than checked
    // in: `entry_<i>.vl` imports `pkg::mod_<i>`, so each entry mints its own
    // base-cache key (the sibling set is part of it since M21) while every
    // world is the same size — the sibling texts are fixed-width, so the
    // budget arithmetic below is exact rather than approximate.
    const ENTRIES: usize = 6;
    let root = std::env::temp_dir().join(format!("vilan_m24_budget_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch dir");
    let mut entry_paths = Vec::new();
    let mut entry_sources: Vec<&'static str> = Vec::new();
    for index in 0..ENTRIES {
        std::fs::write(
            root.join(format!("mod_{index}.vl")),
            format!("fun value_{index}(): i32 {{\n\t{:04}\n}}\n", 1000 + index),
        )
        .expect("write module");
        let entry_path = root.join(format!("entry_{index}.vl"));
        let source = format!(
            "import pkg::mod_{index}::value_{index};\n\nfun main() {{\n\tlet _v = value_{index}();\n}}\n"
        );
        std::fs::write(&entry_path, &source).expect("write entry");
        entry_sources.push(Box::leak(source.into_boxed_str()));
        entry_paths.push(entry_path);
    }

    let analyze = |index: usize| {
        let spec = spec.clone();
        let pkg_root = root.clone();
        let entry_path = entry_paths[index].clone();
        let source = entry_sources[index];
        let diagnostics = on_one_thread(move || {
            let (program, errors) = analyze_source(
                source,
                &spec,
                &pkg_root,
                &entry_path,
                Some(Platform::default()),
                &Workspace::default(),
            );
            let diagnostics = format!("{errors:?}");
            drop(program);
            diagnostics
        });
        assert_eq!(diagnostics, "[]", "entry {index} must analyze clean");
    };

    vilan_core::analyzer::set_base_cache_budget(vilan_core::analyzer::BASE_CACHE_DEFAULT_BUDGET);
    vilan_core::analyzer::base_cache_clear();
    assert_eq!(vilan_core::analyzer::base_cache_retained_bytes(), 0);

    // Under the generous default, the growth M24 exists to bound is real:
    // three distinct keys retain three worlds. (The vacuity guard — with no
    // growth here the budget below would have nothing to bound.)
    analyze(0);
    let one_world = vilan_core::analyzer::base_cache_retained_bytes();
    assert!(
        one_world > 0,
        "a stored world must be recorded as retaining something"
    );
    analyze(1);
    analyze(2);
    assert_eq!(
        vilan_core::analyzer::base_cache_retained(),
        3,
        "three distinct sibling sets retain three worlds under the default \
         budget — this is M24's finding, and the pin's vacuity guard"
    );
    assert_eq!(
        vilan_core::analyzer::base_cache_retained_bytes(),
        3 * one_world,
        "the worlds are the same size by construction, so the budget \
         arithmetic below is exact"
    );

    // The budget takes effect the moment it is set, not at the next store:
    // two worlds' worth of budget keeps the two most recently used.
    leak_tally::reset();
    let budget = 2 * one_world;
    vilan_core::analyzer::set_base_cache_budget(budget);
    assert_eq!(vilan_core::analyzer::base_cache_budget(), budget);
    assert_eq!(
        vilan_core::analyzer::base_cache_retained(),
        2,
        "the budget must evict down to what fits"
    );
    assert!(
        vilan_core::analyzer::base_cache_retained_bytes() <= budget,
        "retained {} B over a {budget} B budget",
        vilan_core::analyzer::base_cache_retained_bytes(),
    );
    assert_eq!(
        leak_tally::released(LeakSite::BaseCacheWorld),
        one_world,
        "an evicted world gives its recorded bytes back to the tally — the \
         M11 site is what makes the bound checkable at all"
    );

    // Entry 0 was the least recently used, so it is the one that went.
    let (hits_before, misses_before) = stats();
    analyze(0);
    let (hits_after, misses_after) = stats();
    assert_eq!(
        (hits_after, misses_after),
        (hits_before, misses_before + 1),
        "the least-recently-used world must be the evicted one"
    );

    // A HIT refreshes recency. The cache now holds {1, 2, 0} trimmed to the
    // budget — re-analyze entry 2 to hit it, then store a fresh key, and the
    // world that goes must be the one nothing has touched.
    vilan_core::analyzer::set_base_cache_budget(vilan_core::analyzer::BASE_CACHE_DEFAULT_BUDGET);
    vilan_core::analyzer::base_cache_clear();
    analyze(3);
    analyze(4);
    let (hits_pre_touch, _) = stats();
    analyze(3); // the HIT that refreshes entry 3's recency
    let (hits_post_touch, _) = stats();
    assert_eq!(
        hits_post_touch,
        hits_pre_touch + 1,
        "the refreshing analysis must actually hit"
    );
    vilan_core::analyzer::set_base_cache_budget(budget);
    analyze(5); // a third key: the budget evicts one, and it must be entry 4
    assert_eq!(vilan_core::analyzer::base_cache_retained(), 2);
    let (hits_pre_3, misses_pre_3) = stats();
    analyze(3);
    let (hits_post_3, misses_post_3) = stats();
    assert_eq!(
        (hits_post_3, misses_post_3),
        (hits_pre_3 + 1, misses_pre_3),
        "the world a hit refreshed must survive the eviction — otherwise the \
         policy is oldest-stored, not least-recently-used"
    );
    let (hits_pre_4, misses_pre_4) = stats();
    analyze(4);
    let (hits_post_4, misses_post_4) = stats();
    assert_eq!(
        (hits_post_4, misses_post_4),
        (hits_pre_4, misses_pre_4 + 1),
        "the untouched world must be the one that was evicted"
    );

    // A budget smaller than a single world does not turn the cache off: the
    // world just stored is exempt, so the bound is "the budget, or one
    // world, whichever is more".
    vilan_core::analyzer::set_base_cache_budget(1);
    assert!(
        vilan_core::analyzer::base_cache_retained() <= 1,
        "a one-byte budget must evict everything it is allowed to"
    );
    let (hits_tiny, misses_tiny) = stats();
    analyze(0);
    analyze(0);
    let (hits_tiny_after, misses_tiny_after) = stats();
    assert_eq!(
        (hits_tiny_after, misses_tiny_after),
        (hits_tiny + 1, misses_tiny + 1),
        "even at a one-byte budget the world just stored survives to serve \
         the next analysis of the same key"
    );

    vilan_core::analyzer::set_base_cache_budget(vilan_core::analyzer::BASE_CACHE_DEFAULT_BUDGET);
    vilan_core::analyzer::base_cache_clear();
    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// M19 T1 — module reuse over a cached world
// (`per-module-analysis-reuse.md` §4.1). The base cache decides WHICH world an
// analysis gets; T1 decides whether that world's modules keep their checks.
// The key is the same key — `BaseCacheKey` plus every loaded source's content
// plus "the entry did not move this module's type slots" — so these pins
// belong beside the ones above, and they are about the third term as much as
// the first two.

/// A package with two siblings and an entry that imports whichever the caller
/// names. Returns the root and the entry path; the caller removes the root.
fn write_reuse_package(name: &str) -> (PathBuf, PathBuf) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "vilan_m19_reuse_{name}_{}_{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("package dir");
    // `loaded.vl` carries a MODULE-LOCAL Class A diagnostic — an assignment to
    // an immutable `let`, which `check_readonly_mutation` refuses. It is the
    // thing that has to survive being replayed.
    std::fs::write(
        root.join("loaded.vl"),
        "export fun value(): i32 {\n\tlet total = 1;\n\ttotal = 2;\n\ttotal\n}\n",
    )
    .expect("write loaded");
    std::fs::write(root.join("other.vl"), "export fun other(): i32 {\n\t9\n}\n")
        .expect("write other");
    // Never imported by any entry below: the world never loads it, so it is
    // not in the key and not in the content validation.
    std::fs::write(
        root.join("unloaded.vl"),
        "export fun spare(): i32 {\n\t3\n}\n",
    )
    .expect("write unloaded");
    let entry = root.join("main.vl");
    (root, entry)
}

/// What a reuse pin reads back: the published diagnostics, and the census
/// `(reused, entry-dirty, world sources)` of the analysis that produced them.
fn observe_reuse(
    spec: &PackageSpec,
    pkg_root: &Path,
    entry_path: &Path,
    source: &'static str,
) -> (String, (usize, usize, usize)) {
    let spec = spec.clone();
    let pkg_root = pkg_root.to_path_buf();
    let entry_path = entry_path.to_path_buf();
    on_one_thread(move || {
        let (_program, errors) = analyze_source(
            source,
            &spec,
            &pkg_root,
            &entry_path,
            Some(Platform::default()),
            &Workspace::default(),
        );
        (format!("{errors:?}"), vilan_core::analyzer::reuse_census())
    })
}

const REUSE_ENTRY_A: &str = "import pkg::loaded::value;\nfun main() { let a = value(); }\n";
const REUSE_ENTRY_B: &str = "import pkg::loaded::value;\nfun main() { let a = value() + 1; }\n";
const REUSE_ENTRY_TWO: &str = "import pkg::loaded::value;\nimport pkg::other::other;\n\
                               fun main() { let a = value() + other(); }\n";
const REUSE_ENTRY_NONE: &str = "fun main() { let a = 1; }\n";

/// The invalidation cases §4.1's key is built to answer, in one pin because
/// they only mean anything against each other.
///
/// The claim under test is that reuse is exactly as invalidating as the world
/// is — no more (an unloaded sibling is not in the world and must not cost
/// anything) and no less (a loaded sibling's edit must take the record with
/// it, never serve a stale diagnostic).
#[test]
fn module_reuse_follows_the_world_key_through_every_edit() {
    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    vilan_core::analyzer::set_world_reuse(true);
    vilan_core::analyzer::base_cache_clear();

    let (root, entry) = write_reuse_package("key");
    let spec = vilan_core::manifest::resolve_std(&std_root());

    // 1. The first analysis is a MISS: it derives the module's diagnostic and
    //    records it. Nothing is reused.
    let first = observe_reuse(&spec, &root, &entry, REUSE_ENTRY_A);
    assert!(
        first.0.contains("total"),
        "the fixture must produce a module diagnostic: {}",
        first.0
    );
    assert_eq!(first.1.0, 0, "a miss reuses nothing: {:?}", first.1);

    // 2. EDIT THE ENTRY — the keystroke this tranche exists for. The world
    //    hits, the modules are reused, and the module's diagnostic is
    //    published from the record rather than re-derived.
    let edited_entry = observe_reuse(&spec, &root, &entry, REUSE_ENTRY_B);
    assert!(
        edited_entry.1.0 > 0,
        "an entry-only edit must reuse the world's modules: {:?}",
        edited_entry.1
    );
    assert_eq!(
        first.0, edited_entry.0,
        "the replayed diagnostic must be the derived one, byte for byte"
    );

    // 3. EDIT AN UNLOADED MODULE — a sibling no entry imports is not in the
    //    world, not in the key, and not in the content validation, so it costs
    //    nothing. This is the half of §4.1 that makes the coarse key
    //    affordable.
    std::fs::write(
        root.join("unloaded.vl"),
        "export fun spare(): i32 {\n\t4\n}\n",
    )
    .expect("edit unloaded");
    let after_unloaded = observe_reuse(&spec, &root, &entry, REUSE_ENTRY_B);
    assert!(
        after_unloaded.1.0 > 0,
        "editing a sibling the world never loaded must not cost the reuse: \
         {:?}",
        after_unloaded.1
    );

    // 4. EDIT THE LOADED MODULE — the world is stale by content (E12), so it
    //    is evicted, the analysis misses, nothing is reused, and the NEW text
    //    is what gets checked. A replayed diagnostic here would be the stale
    //    one, which is the failure this whole seam has to be incapable of.
    std::fs::write(
        root.join("loaded.vl"),
        "export fun value(): i32 {\n\tlet total = 1;\n\ttotal = 2;\n\ttotal = 3;\n\ttotal\n}\n",
    )
    .expect("edit loaded");
    let after_loaded = observe_reuse(&spec, &root, &entry, REUSE_ENTRY_B);
    assert_eq!(
        after_loaded.1.0, 0,
        "an edited loaded module must evict the world AND the record: {:?}",
        after_loaded.1
    );
    assert_ne!(
        first.0, after_loaded.0,
        "the analysis after a module edit must publish the EDITED module's \
         diagnostics — two refusals now, not one"
    );

    // 5. ADD A MODULE — a different sibling set is a different world (M21), so
    //    the record for the one-sibling world cannot be served to it.
    let added = observe_reuse(&spec, &root, &entry, REUSE_ENTRY_TWO);
    assert_eq!(
        added.1.0, 0,
        "a new sibling is a new world: nothing to reuse yet ({:?})",
        added.1
    );
    let added_again = observe_reuse(&spec, &root, &entry, REUSE_ENTRY_TWO);
    assert!(
        added_again.1.0 > added.1.0,
        "the two-sibling world's own second analysis must reuse: {:?}",
        added_again.1
    );
    assert!(
        added_again.1.2 > after_loaded.1.2,
        "the two-sibling world must hold one more source than the one-sibling \
         world ({} vs {})",
        added_again.1.2,
        after_loaded.1.2
    );

    // 6. REMOVE THE MODULES — an entry that imports no sibling is a std-only
    //    world again, and the sibling's diagnostic goes with it.
    let removed = observe_reuse(&spec, &root, &entry, REUSE_ENTRY_NONE);
    assert!(
        !removed.0.contains("total"),
        "a module nobody imports must not be checked, let alone replayed: {}",
        removed.0
    );

    let _ = std::fs::remove_dir_all(&root);
    vilan_core::analyzer::base_cache_clear();
}

/// The M24 / M26 interplay, which is about the record OUTLIVING or being
/// TRUNCATED by the mechanisms either side of it.
///
/// M24 evicts a world for bytes. The record is a separate map, so it survives
/// — and it must be harmless that it does: the world it described is gone, the
/// next analysis misses and re-derives, and it publishes exactly what the
/// replay would have.
///
/// M26 cancels an analysis at a phase boundary, which leaves the check phase a
/// PREFIX of itself. A prefix recorded as if it were whole would publish a
/// truncated module on every later hit, so a cancelled analysis records
/// nothing at all.
#[test]
fn the_checks_record_survives_eviction_and_refuses_a_cancelled_phase() {
    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    vilan_core::analyzer::set_world_reuse(true);
    vilan_core::analyzer::base_cache_clear();

    let (root, entry) = write_reuse_package("evict");
    let spec = vilan_core::manifest::resolve_std(&std_root());
    let budget_before = vilan_core::analyzer::base_cache_budget();

    let derived = observe_reuse(&spec, &root, &entry, REUSE_ENTRY_A);
    assert!(derived.0.contains("total"), "{}", derived.0);
    let replayed = observe_reuse(&spec, &root, &entry, REUSE_ENTRY_B);
    assert!(
        replayed.1.0 > 0,
        "the warm analysis must reuse: {:?}",
        replayed.1
    );

    // M24: a zero budget evicts everything the moment it is set. The record
    // outlives the world it describes, and the next analysis — a MISS, since
    // there is no world to hit — re-derives instead of replaying.
    vilan_core::analyzer::set_base_cache_budget(0);
    let after_eviction = observe_reuse(&spec, &root, &entry, REUSE_ENTRY_B);
    vilan_core::analyzer::set_base_cache_budget(budget_before);
    assert_eq!(
        after_eviction.1.0, 0,
        "reuse keys on the HIT: an evicted world cannot be reused over ({:?})",
        after_eviction.1
    );
    assert_eq!(
        derived.0, after_eviction.0,
        "eviction may not change an answer"
    );

    // M26: an analysis cancelled before it starts stops at the PARSE boundary
    // (`lib.rs`, `editor-latency.md` §4.2) — it never reaches `analyze`, so it
    // stores no world and records no checks. That is the property asserted
    // here, and it is the one that keeps the record safe: the store guard
    // inside `analyze_over_world` refuses a cancelled phase because a
    // truncated check sequence is a PREFIX of itself and a prefix recorded as
    // a whole would silence a real refusal on every later hit — but nothing
    // deterministic can reach that guard today, because the boundary above it
    // fires first. So this leg pins the boundary: if M26 ever moves a
    // checkpoint past the world store, the analysis after a cancel will HIT a
    // world whose record is empty, and both assertions below go red before a
    // truncated record can be published to anyone.
    vilan_core::analyzer::base_cache_clear();
    let cancelled = {
        let spec = spec.clone();
        let root = root.clone();
        let entry = entry.clone();
        on_one_thread(move || {
            let token = vilan_core::cancel::CancelToken::new();
            token.cancel();
            let _scope = token.install();
            let (program, _errors) = analyze_source(
                REUSE_ENTRY_A,
                &spec,
                &root,
                &entry,
                Some(Platform::default()),
                &Workspace::default(),
            );
            program.is_none()
        })
    };
    assert!(
        cancelled,
        "the pre-cancelled analysis must produce no program"
    );
    let after_cancel = observe_reuse(&spec, &root, &entry, REUSE_ENTRY_B);
    assert_eq!(
        after_cancel.1.0, 0,
        "a cancelled analysis must leave no world behind to reuse over: {:?}",
        after_cancel.1
    );
    assert_eq!(
        derived.0, after_cancel.0,
        "a cancelled analysis must not record its truncated phase: the module \
         reported nothing under it, and replaying that would silence a real \
         refusal"
    );

    let _ = std::fs::remove_dir_all(&root);
    vilan_core::analyzer::base_cache_clear();
}

/// M23's overlay, over the widened seam. A sibling served from the document
/// overlay is analysis-owned; the world that loaded it takes a claim and is
/// stored, which is what makes a keystroke in the entry hit at all. The record
/// rides that same world — so an overlay EDIT to the sibling must evict both,
/// exactly as a disk edit does, and the analysis after it must publish the
/// overlay's text and not the record's memory of the previous one.
#[test]
fn an_overlay_served_sibling_edit_evicts_the_record_with_the_world() {
    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    vilan_core::analyzer::set_world_reuse(true);
    vilan_core::analyzer::base_cache_clear();

    let (root, entry) = write_reuse_package("overlay");
    let spec = vilan_core::manifest::resolve_std(&std_root());
    let loaded = root.join("loaded.vl");

    // The buffer the editor holds: the same module, one refusal.
    vilan_core::analyzer::set_document_overlay(
        &loaded,
        Some("export fun value(): i32 {\n\tlet total = 1;\n\ttotal = 2;\n\ttotal\n}\n".to_string()),
    );
    let derived = observe_reuse(&spec, &root, &entry, REUSE_ENTRY_A);
    assert!(derived.0.contains("total"), "{}", derived.0);
    let replayed = observe_reuse(&spec, &root, &entry, REUSE_ENTRY_B);
    assert!(
        replayed.1.0 > 0,
        "an overlay-served world must still be hit and reused (M23): {:?}",
        replayed.1
    );
    assert_eq!(derived.0, replayed.0, "the replay changed the answer");

    // The editor fixes the module. Every load and every per-hit validation
    // reads through the overlay, so the world hash-mismatches and goes — and
    // the record goes with the answer.
    vilan_core::analyzer::set_document_overlay(
        &loaded,
        Some(
            "export fun value(): i32 {\n\tlet mut total = 1;\n\ttotal = 2;\n\ttotal\n}\n"
                .to_string(),
        ),
    );
    let fixed = observe_reuse(&spec, &root, &entry, REUSE_ENTRY_B);
    assert!(
        !fixed.0.contains("total"),
        "the overlay edit must be seen: a replayed diagnostic here is the \
         stale one, published over a buffer that no longer says it — {}",
        fixed.0
    );

    vilan_core::analyzer::set_document_overlay(&loaded, None);
    let _ = std::fs::remove_dir_all(&root);
    vilan_core::analyzer::base_cache_clear();
}

/// M41: `type_id_sources` — T0's per-`TypeId` minting-source census — is
/// ~4 bytes for every type a world ever minted, and `base_cache_world_bytes`
/// could not see it. That made M24's LRU budget optimistic by exactly that
/// much on every retained world, and the direction matters: a budget that
/// under-counts what it retains is a budget the session exceeds silently.
///
/// Three claims, and the third is the item's:
///
/// 1. the split is EXHAUSTIVE — texts plus census is the very figure the
///    budget is compared against, so neither half can quietly stop counting;
/// 2. the census is a real, non-zero share of a std world (it is not a
///    rounding error being accounted for form's sake);
/// 3. **the tally MOVES when a world gains types, and moves on TYPES rather
///    than on text.** The two sibling fixtures below are within a byte of the
///    same length; one declares thirty-two nominals and the other is the same
///    bulk in comments. If the tally were only a second reading of the text
///    length — which is exactly what it was — the two worlds would be
///    recorded as worth the same. They are not.
#[test]
fn the_world_tally_counts_the_type_id_census_and_moves_when_a_world_gains_types() {
    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let spec = vilan_core::manifest::resolve_std(&std_root());

    let root = std::env::temp_dir().join(format!("vilan_m41_census_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("package dir");
    // Type-dense: thirty-two nominals, each minting types of its own.
    let mut dense = String::new();
    for index in 0..32 {
        dense.push_str(&format!("struct Dense{index:02} {{\n\tvalue: i32,\n}}\n"));
    }
    // Type-free, and the same bulk: comment lines, which mint nothing.
    let mut sparse = String::new();
    for index in 0..32 {
        sparse.push_str(&format!("// sparse{index:02} .............\n//\n//\n"));
    }
    assert_eq!(
        dense.len(),
        sparse.len(),
        "the fixtures must be the same length, or this pin reads a text \
         difference and calls it a type difference"
    );
    std::fs::write(root.join("dense.vl"), &dense).expect("write dense");
    std::fs::write(root.join("sparse.vl"), &sparse).expect("write sparse");
    const DENSE: &str = "import std::io::print;\nimport pkg::dense::Dense00;\n\
                         fun main() { print(1); }\n";
    const SPARSE: &str = "import std::io::print;\nimport pkg::sparse;\n\
                          fun main() { print(1); }\n";

    let entry_path = root.join("main.vl");
    let read_split = |spec: &PackageSpec, source: &'static str| {
        let spec = spec.clone();
        let pkg_root = root.clone();
        let entry_path = entry_path.clone();
        on_one_thread(move || {
            // One world at a time, so the split belongs to a known program
            // rather than to whatever an earlier test left behind.
            vilan_core::analyzer::base_cache_clear();
            let (program, errors) = analyze_source(
                source,
                &spec,
                &pkg_root,
                &entry_path,
                Some(Platform::default()),
                &Workspace::default(),
            );
            drop(program);
            let (texts, census) = vilan_core::analyzer::base_cache_retained_split();
            (
                vilan_core::analyzer::base_cache_retained_bytes(),
                texts,
                census,
                format!("{errors:?}"),
            )
        })
    };
    let (dense_bytes, dense_texts, dense_census, dense_errors) = read_split(&spec, DENSE);
    let (sparse_bytes, sparse_texts, sparse_census, sparse_errors) = read_split(&spec, SPARSE);
    vilan_core::analyzer::base_cache_clear();
    let _ = std::fs::remove_dir_all(&root);

    println!(
        "M41-CENSUS dense: {dense_bytes} B = {dense_texts} B texts + {dense_census} B census; \
         sparse: {sparse_bytes} B = {sparse_texts} B texts + {sparse_census} B census"
    );
    assert_eq!(dense_errors, "[]", "the dense fixture must analyze clean");
    assert_eq!(sparse_errors, "[]", "the sparse fixture must analyze clean");

    // (1) Exhaustive: the two halves ARE the budgeted figure.
    assert_eq!(
        dense_texts + dense_census,
        dense_bytes,
        "the split must account for every byte the budget is compared against"
    );
    assert_eq!(sparse_texts + sparse_census, sparse_bytes);

    // (2) Real: a std world mints thousands of types, so its census is tens of
    // kilobytes — not zero, which is what the tally used to report.
    assert!(
        sparse_census > 10_000,
        "a std world's `TypeId` census must be a real share of what it \
         retains, not {sparse_census} B"
    );

    // (3) The item's pin. Same text, more types, bigger tally.
    assert_eq!(
        dense_texts, sparse_texts,
        "the fixtures were written the same length, so the TEXT halves must \
         agree — if they do not, the census comparison below proves nothing"
    );
    assert!(
        dense_census > sparse_census,
        "a world that gained thirty-two nominals must be recorded as worth \
         more than one that gained the same bulk in comments: {dense_census} B \
         is not more than {sparse_census} B"
    );
}

/// The process CPU this process has burned, in milliseconds — every thread's,
/// summed, read off `/proc/self/stat`'s `utime`/`stime` (fields 14 and 15).
///
/// Wall is not an instrument on this tree's box: the numbers below are taken
/// beside other lanes and the load average swings by an order of magnitude
/// between runs. `None` where the file is not there (every non-Linux host),
/// which is why the measurement that reads it refuses rather than reporting a
/// wall number and calling it CPU.
fn process_cpu_ms() -> Option<f64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    // The comm field can contain spaces and parentheses; everything after the
    // LAST `)` is the space-separated remainder, whose first entry is `state`
    // (field 3).
    let rest = stat.rsplit_once(')')?.1;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    // `sysconf(_SC_CLK_TCK)` is 100 on every Linux this runs on; the value is
    // fixed in the kernel ABI (USER_HZ), not a tunable.
    Some((utime + stime) as f64 * 10.0)
}

fn loadavg_1m() -> String {
    std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|text| text.split_whitespace().next().map(str::to_string))
        .unwrap_or_else(|| "?".to_string())
}

/// M36: **the per-process floor an on-disk base cache would remove.**
///
/// The base cache is process-global and in memory. Inside one process the
/// second analysis of an import set is a hit and costs almost nothing; across
/// processes there is no cache at all, so every process that analyzes any
/// program pays the whole cold `std` analysis again. That is what N52's split
/// bought its schedulability with: one process per corpus program took the
/// inference differential from 12.4 s as a single unit to 47.3 s across 128,
/// and the release differential pays the same bill.
///
/// This measures the floor rather than asserting a budget on it. Cold is a
/// cleared cache; warm is the very next analysis of the same import set,
/// which is the cache hit an on-disk world would give the SECOND PROCESS. The
/// difference is what a cross-process cache is worth per process, and the
/// ratio is what says whether it is worth building.
///
/// **What it does not do, and why the item stays open.** Serving that hit
/// across processes means writing a `World<'static>` to a file and reading it
/// back. The world is a graph of `&'src str` into the parse cache's leaked
/// module texts and of `Span` offsets into them, spread over some forty maps;
/// nothing in the tree serializes it, and the design that would (texts plus
/// offsets, keyed by M21's key — the std sources' content hashes and the
/// toolchain hash — under M24's byte budget for eviction, M9's leak-soak rules
/// for what a served world may retain, a temp-file-plus-rename write so a
/// killed process cannot leave a half-file, and a checksum that turns a
/// corrupt file into a MISS rather than into a wrong answer) is a tranche of
/// its own. The number below is what that tranche would buy.
#[test]
#[ignore = "M36: a MEASUREMENT of the cross-process floor, not a budget — it prints the cold/warm split a cross-process world cache would remove; run deliberately"]
fn the_cold_std_analysis_is_the_per_process_floor_an_on_disk_world_would_remove() {
    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let spec = vilan_core::manifest::resolve_std(&std_root());
    let load = loadavg_1m();

    let (cold, warm, hits, misses) = on_one_thread(move || {
        // Warm the process ONCE first and throw the result away: the parse
        // cache, the interner and the allocator's arenas are per-process too,
        // and this measurement is about the base cache alone.
        // A realistic std closure, not a one-import toy: the floor an on-disk
        // world removes is the BASE world's, and the base world is whatever
        // `std` surface the program reaches. A corpus program reaches a good
        // deal of it.
        const WIDE_A: &str = "import std::io::print;\nimport std::list::List;\n\
                              import std::map::Map;\nimport std::set::Set;\n\
                              import std::json;\nimport std::math::PI;\n\
                              fun main() { print(PI); }\n";
        const WIDE_B: &str = "import std::io::print;\nimport std::list::List;\n\
                              import std::map::Map;\nimport std::set::Set;\n\
                              import std::json;\nimport std::math::PI;\n\
                              fun main() { print(PI + 1.0); }\n";
        vilan_core::analyzer::base_cache_clear();
        analyze_on_this_thread(&spec, WIDE_A);

        vilan_core::analyzer::base_cache_clear();
        let (hits_before, misses_before) = stats();
        let before = process_cpu_ms();
        analyze_on_this_thread(&spec, WIDE_A);
        let cold = before.zip(process_cpu_ms()).map(|(a, b)| b - a);
        // Same import set, different body: the base-cache hit.
        let before = process_cpu_ms();
        analyze_on_this_thread(&spec, WIDE_B);
        let warm = before.zip(process_cpu_ms()).map(|(a, b)| b - a);
        let (hits_after, misses_after) = stats();
        vilan_core::analyzer::base_cache_clear();
        (
            cold,
            warm,
            hits_after - hits_before,
            misses_after - misses_before,
        )
    });

    let (Some(cold), Some(warm)) = (cold, warm) else {
        panic!(
            "no process CPU clock on this host (no /proc/self/stat), so this \
             measurement would be a wall number wearing a CPU label (M15)"
        );
    };
    println!(
        "M36-FLOOR profile={} cold={cold:.0} ms warm={warm:.0} ms floor={:.0} ms \
         ({:.0}% of a cold analysis) hits={hits} misses={misses} load={load}",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        cold - warm,
        (cold - warm) / cold.max(1.0) * 100.0,
    );
    // The instrument, not the finding: one miss then one hit is the shape the
    // two numbers are supposed to be, and without it they measure something
    // else entirely.
    assert_eq!(misses, 1, "the cold analysis must MISS exactly once");
    assert_eq!(hits, 1, "the warm analysis must HIT exactly once");
    // Non-vacuity: if a warm analysis cost what a cold one costs, there would
    // be no floor to lift and nothing for M36 to build. A THIRD of a cold
    // analysis is the bar, not half — the measured share is 46% and the point
    // of the bar is to catch the day it goes to nothing, not to encode this
    // run's figure as a budget.
    assert!(
        cold - warm > cold / 3.0,
        "the base world is only {:.0} ms of a {cold:.0} ms cold analysis, so \
         there is no per-process floor worth removing (warm {warm:.0} ms, \
         loadavg {load})",
        cold - warm,
    );
}
