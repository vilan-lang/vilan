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

const PROGRAM_A: &str = "import std::print;\nfun main() { print(1); }\n";
const PROGRAM_B: &str = "import std::print;\nfun main() { print(2 + 3); }\n";
/// A DISTINCT import set from A/B — and deliberately one that reaches no
/// macro-defining std module, so this fixture's hit deltas belong to the
/// outer analysis alone (`std::time`, the previous choice, drags two macro
/// worlds along and each of those now consults the cache itself).
const PROGRAM_C: &str = "import std::print;\nimport std::math::PI;\nfun main() { print(PI); }\n";

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
        "import std::print;\nmacro fun m(s: Source): Source { s }\nfun main() { print(1); }\n",
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
    const WITH_DERIVE: &str = "import std::print;\n[derive(PartialEq)]\nstruct P { x: i32 }\nfun main() { print(P { x = 1 } == P { x = 1 }); }\n";
    const WITHOUT_DERIVE: &str = "import std::print;\nstruct Q { x: i32 }\nfun main() { print(Q { x = 1 } == Q { x = 1 }); }\n";

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
    const TWO_WORLDS: &str = "import std::print;\n[derive(PartialEq, Debug)]\nstruct P { x: i32 }\nfun main() { print((P { x = 1 } == P { x = 1 }).debug()); }\n";

    vilan_core::analyzer::base_cache_clear();
    vilan_core::macro_world_cache_clear();
    // Prime the ordinary (dependency-free) base so the OUTER analysis below
    // is a hit and every remaining delta belongs to the macro worlds.
    // `import std::print` dispatches no macro of its own.
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
    const DERIVE_DEBUG: &str = "import std::print;\n[derive(Debug)]\nstruct P { x: i32 }\nfun main() { print(P { x = 1 }.debug()); }\n";
    const DERIVE_PARTIAL_EQ: &str = "import std::print;\n[derive(PartialEq)]\nstruct Q { x: i32 }\nfun main() { print(Q { x = 1 } == Q { x = 1 }); }\n";

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
    const MISSING_DERIVE: &str = "import std::print;\n[derive(Debug)]\nstruct P { x: i32 }\nfun main() { print((P { x = 1 } == P { x = 1 }).debug()); }\n";

    const DERIVE_PARTIAL_EQ: &str = "import std::print;\n[derive(PartialEq)]\nstruct Q { x: i32 }\nfun main() { print(Q { x = 1 } == Q { x = 1 }); }\n";

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
    const ENTRY_IMPL: &str = "import std::print;\nstruct Widget { n: i32 }\nimpl Widget { fun doubled(self): i32 { self.n * 2 } }\nfun main() { print(Widget { n = 21 }.doubled()); }\n";

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

/// The M9 store gate (`leak-soak.md` §7.9.4a), the mechanism's stated proof
/// obligation: an opted-in analysis that loaded an OVERLAY-SERVED source owns
/// that text and tree — its program must be their only borrower — so
/// `base_cache_store` refuses to store the world that borrows them. The
/// consequence is deliberate: base-world caching is forfeited while a
/// dependency (or std) file is open in the editor — repeat analyses keep
/// missing — and resumes the moment the buffer closes. Without the gate the
/// stored world would serve a later analysis borrows into freed memory (the
/// §7.9.2 ctrl-Z shape, one seam over).
#[test]
fn a_world_that_loaded_an_overlaid_source_is_not_stored_until_the_buffer_closes() {
    use vilan_core::{MacroLimits, Workspace};

    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    vilan_core::analyzer::base_cache_clear();

    // A workspace with one dependency package: dependency files are exactly
    // the files a multi-package workspace has open in the editor, and —
    // unlike a `pkg::` sibling, which bypasses the base cache anyway — they
    // are loaded into the world the cache stores.
    let root = std::env::temp_dir().join(format!("vilan_m9_gate_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let app_dir = root.join("app");
    std::fs::create_dir_all(&app_dir).expect("app dir");
    let entry_path = app_dir.join("main.vl");
    std::fs::write(
        &entry_path,
        "import std::print;\nimport common::greeting;\n\nfun main() {\n\tprint(greeting());\n}\n",
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
        }],
        entry_dependencies: vec![("common".to_string(), 0)],
        macro_limits: MacroLimits::default(),
    };
    let source: &'static str = Box::leak(
        std::fs::read_to_string(&entry_path)
            .unwrap()
            .into_boxed_str(),
    );

    // One opted-in analysis — the language server's shape — with the handles
    // reclaimed the way their owner would.
    let analyze_owning = || {
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
                assert!(
                    analyzed.diagnostics.is_empty(),
                    "the gate fixture must compile clean, got {:#?}",
                    analyzed.diagnostics
                );
                drop(analyzed.program);
                if let Some(ast) = analyzed.ast {
                    // SAFETY: the program — the tree's only borrower — was
                    // dropped on the line above.
                    unsafe { ast.reclaim() };
                }
                // SAFETY: as above; with the store refused, the program was
                // the owned copies' only borrower.
                unsafe { analyzed.owned_modules.reclaim() };
            })
            .expect("spawn worker")
            .join()
            .expect("worker panicked");
    };

    // Disk-served: the dependency world stores and hits — the baseline that
    // proves the fixture exercises the cache at all.
    analyze_owning();
    let (hits_before, _) = stats();
    analyze_owning();
    let (hits_disk, _) = stats();
    assert_eq!(
        hits_disk,
        hits_before + 1,
        "the disk-served dependency world must hit — the fixture is not \
         reaching the base cache, so the gate assertions below are vacuous"
    );

    // The dependency's lib.vl is now OPEN in the editor, edited: every
    // analysis loads it from the overlay into analysis-owned allocations, so
    // no world may be stored — repeat analyses keep missing and never hit.
    vilan_core::analyzer::base_cache_clear();
    vilan_core::analyzer::set_document_overlay(
        &dep_lib,
        Some("fun greeting(): i32 {\n\t42\n}\n".to_string()),
    );
    let (hits_open_before, misses_open_before) = stats();
    analyze_owning();
    analyze_owning();
    let (hits_open, misses_open) = stats();
    assert_eq!(
        hits_open, hits_open_before,
        "a world that loaded an overlay-served source was stored and served — \
         the M9 store gate is gone, and the served world borrows memory the \
         owning analysis will free (leak-soak.md §7.9.4a)"
    );
    assert_eq!(
        misses_open,
        misses_open_before + 2,
        "with the store refused, every analysis over the open buffer misses"
    );

    // The buffer closes: loads come from disk again, the store resumes, and
    // the very next repeat analysis hits.
    vilan_core::analyzer::set_document_overlay(&dep_lib, None);
    analyze_owning();
    let (hits_closed_before, _) = stats();
    analyze_owning();
    let (hits_closed, _) = stats();
    assert_eq!(
        hits_closed,
        hits_closed_before + 1,
        "once the buffer closes the world must store and hit again"
    );

    vilan_core::analyzer::base_cache_clear();
    let _ = std::fs::remove_dir_all(&root);
}
