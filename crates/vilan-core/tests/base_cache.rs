//! The base-cache gate (S3c, proposal/analysis-reuse.md §6.10): a cached
//! world must be observation-identical to a fresh build, hit exactly when it
//! should, and revalidate by CONTENT (the E12 rule — a std edit evicts).
//! Stats are process-global, so every test serializes on one lock and
//! asserts deltas.

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
const PROGRAM_C: &str =
    "import std::print;\nimport std::time::sleep;\nfun main() { sleep(1); print(9); }\n";

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
