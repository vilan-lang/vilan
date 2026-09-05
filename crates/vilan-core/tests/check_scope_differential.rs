//! The S1 differential gate (proposal/analysis-reuse.md §6): entry-scoped
//! checks may skip std-defined entities ONLY if that is unobservable from
//! outside. Two halves hold that:
//!
//! 1. The **std-clean invariant**: every std module, loaded and checked under
//!    FULL scan, produces zero diagnostics and zero warnings — so the
//!    definition-site diagnostics the scoped run skips are known to not
//!    exist.
//! 2. The **differential sweep**: the whole corpus analyzed both ways —
//!    scoped (the default) and full-scan (forced) — must agree byte-for-byte
//!    on diagnostics, warnings, and emitted JS.
//!
//! `set_full_scan_checks` is process-global, so every test here serializes on
//! one lock: under cargo test these share a process, and a leaked override
//! would quietly turn the differential vacuous.

mod replay_harness;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use replay_harness::{
    module_entry, observe_in_package, std_root, std_spec, warm_pair, write_module_package,
};
use vilan_core::{BuildOptions, Platform, Workspace, analyze_source, transform};

static OVERRIDE_LOCK: Mutex<()> = Mutex::new(());

/// One analysis + transform on a big-stack worker, mirroring the real
/// pipeline. Returns everything the differential compares: the debug
/// rendering of diagnostics and warnings, and the emitted JS (`None` when
/// the program did not analyze cleanly).
fn compile_observation(source: &str, platform: Platform) -> (String, String, Option<String>) {
    let source = source.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                Path::new("."),
                Path::new("differential.vl"),
                Some(platform),
                &Workspace::default(),
            );
            let diagnostics = format!("{errors:?}");
            let warnings = program
                .as_ref()
                .map(|program| format!("{:?}", program.warnings))
                .unwrap_or_default();
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

/// The std module names importable on `platform`: base modules plus the
/// matching layer's, by file stem (`lib` is the package surface, not a
/// module).
fn std_modules_for(layer_directory: Option<&str>) -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();
    let mut collect = |directory: PathBuf| {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|extension| extension == "vl") {
                let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
                // `lib` is the package surface, not a module. `null` is a
                // literal keyword — a bare `import std::null;` cannot parse —
                // and it sits in the analyzer's always-loaded core set, so
                // every leg still loads and checks it without the import.
                if stem != "lib" && stem != "null" {
                    names.insert(stem);
                }
            }
        }
    };
    collect(std_root().join("src"));
    if let Some(layer) = layer_directory {
        collect(std_root().join("src").join(layer));
    }
    names.into_iter().collect()
}

/// The invariant the scoped run leans on: every std module, force-loaded and
/// checked under FULL scan, is clean — zero diagnostics, zero warnings. If a
/// std change ever trips this, the scoped skip would be hiding that
/// diagnostic from every user build, so this test failing is a release
/// blocker, not a flake.
#[test]
fn every_std_module_is_clean_under_full_scan() {
    let _guard = OVERRIDE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    vilan_core::analyzer::set_full_scan_checks(true);
    for (layer, platform) in [
        (Some("process"), Platform::default()),
        (Some("browser"), Platform::Browser),
    ] {
        let mut source = String::new();
        for name in std_modules_for(layer) {
            source.push_str(&format!("import std::{name};\n"));
        }
        source.push_str("fun main() {}\n");
        let (diagnostics, warnings, _) = compile_observation(&source, platform);
        assert_eq!(
            diagnostics, "[]",
            "std must be clean under full scan ({layer:?}): {diagnostics}"
        );
        assert_eq!(
            warnings, "[]",
            "std must be warning-clean under full scan ({layer:?}): {warnings}"
        );
    }
    vilan_core::analyzer::set_full_scan_checks(false);
}

/// The recording that the skip keys on: a plain analysis marks the loaded
/// std modules as frozen sources, and never the entry.
#[test]
fn std_sources_are_recorded_and_the_entry_is_not() {
    let _guard = OVERRIDE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (program, errors) = {
        let source: &'static str = "fun main() {}";
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(move || {
                analyze_source(
                    source,
                    &std_spec(),
                    Path::new("."),
                    Path::new("probe.vl"),
                    Some(Platform::default()),
                    &Workspace::default(),
                )
            })
            .expect("spawn worker")
            .join()
            .expect("worker panicked")
    };
    assert!(errors.is_empty(), "{errors:?}");
    let program = program.expect("program");
    assert!(
        program.std_sources.len() >= 15,
        "a trivial entry loads the always-on std core; only {} sources marked",
        program.std_sources.len()
    );
    assert!(
        !program
            .std_sources
            .contains(&vilan_core::analyzer::SourceId(0)),
        "the entry must never be a frozen source"
    );
}

/// The differential itself: every corpus program, analyzed scoped and
/// full-scan, agrees on diagnostics, warnings, and emitted JS.
#[test]
fn corpus_agrees_between_scoped_and_full_scan_checks() {
    let _guard = OVERRIDE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let corpus = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/test");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&corpus)
        .expect("corpus directory")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            (path.extension()? == "vl").then_some(path)
        })
        .collect();
    paths.sort();
    assert!(paths.len() > 60, "suspiciously few corpus programs");

    let observe_all = || -> Vec<(String, String, Option<String>)> {
        std::thread::scope(|scope| {
            let workers: Vec<_> = paths
                .chunks(paths.len().div_ceil(8).max(1))
                .map(|chunk| {
                    scope.spawn(move || {
                        chunk
                            .iter()
                            .map(|path| {
                                let source =
                                    std::fs::read_to_string(path).expect("read corpus file");
                                compile_observation(&source, Platform::default())
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            workers
                .into_iter()
                .flat_map(|worker| worker.join().expect("worker panicked"))
                .collect()
        })
    };

    vilan_core::analyzer::set_full_scan_checks(false);
    let scoped = observe_all();
    vilan_core::analyzer::set_full_scan_checks(true);
    let full = observe_all();
    vilan_core::analyzer::set_full_scan_checks(false);

    let mut divergences = Vec::new();
    for ((path, scoped), full) in paths.iter().zip(&scoped).zip(&full) {
        let name = path.file_name().unwrap().to_string_lossy();
        if scoped.0 != full.0 {
            divergences.push(format!(
                "{name}: diagnostics differ\n  scoped: {}\n  full:   {}",
                scoped.0, full.0
            ));
        }
        if scoped.1 != full.1 {
            divergences.push(format!(
                "{name}: warnings differ\n  scoped: {}\n  full:   {}",
                scoped.1, full.1
            ));
        }
        if scoped.2 != full.2 {
            divergences.push(format!("{name}: emitted JS differs"));
        }
    }
    assert!(
        divergences.is_empty(),
        "{} corpus programs observe the check scope:\n{}",
        divergences.len(),
        divergences.join("\n")
    );
}

// ---------------------------------------------------------------------------
// M19 T1 — the WIDENED seam (`per-module-analysis-reuse.md` §5).
//
// S1 above may skip a std entity because std's diagnostics are known ABSENT.
// T1 widens the skip to every module of a base-CACHED world, where they are
// only REMEMBERED — the module's own Class A diagnostics, recorded by the
// analysis that derived them and spliced back in by every later one. The
// std-clean invariant cannot carry that, so the differential does, and these
// are its legs: replay must equal re-derivation, on a corpus, byte for byte.
//
// `set_world_reuse` is process-global like `set_full_scan_checks`, so every
// test here takes the same `OVERRIDE_LOCK`.
//
// The CORPUS sweep these two pins share a seam with lives in
// `replay_differential`, its own binary since tracker N57: it costs 60-170 s,
// and a targeted run of this file was paying that to ask about S1. The package
// fixtures all three use are `replay_harness`'s.

/// **The replay pin** (§5, T1's second gate). A package module with a
/// deliberate error, analyzed from a dependent twice: the second analysis
/// publishes the identical diagnostic, at the identical span, attributed to
/// the identical file — without having re-derived it.
#[test]
fn a_reused_module_replays_its_own_diagnostic() {
    let _guard = OVERRIDE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    vilan_core::analyzer::set_world_reuse(true);
    vilan_core::analyzer::base_cache_clear();

    // `let` is immutable, so the write is R-checked at the assignment site —
    // `check_readonly_mutation`, Class A, and inside the MODULE's own body.
    let (directory, entry) = write_module_package(
        "replay",
        "fun broken(): i32 {\n\tlet total = 1;\n\ttotal = 2;\n\ttotal\n}\n",
    );

    let first = observe_in_package(&directory, &entry, module_entry(1));
    assert!(
        first.0.contains("total"),
        "the fixture must produce a MODULE diagnostic to replay, got: {}",
        first.0
    );
    assert_eq!(
        first.3.0, 0,
        "the first analysis is a base-cache MISS: it derives and records, it \
         does not reuse"
    );

    let second = observe_in_package(&directory, &entry, module_entry(2));
    assert!(
        second.3.0 > 0,
        "the second analysis must hit the world and reuse its modules; census \
         (reused, dirty, sources) = {:?}",
        second.3
    );
    assert_eq!(
        first.0, second.0,
        "the replayed diagnostic must be byte-identical to the derived one — \
         same message, same span, same order"
    );
    assert_eq!(
        first.1, second.1,
        "the replayed diagnostic must publish to the same FILE: replayed \
         `Note.source` and `diagnostic_sources` are indices into the world's \
         `sources` vector (§3.2)"
    );

    let _ = std::fs::remove_dir_all(&directory);
    vilan_core::analyzer::base_cache_clear();
}

/// **The red-first pin** (§5, T1's third gate). The planted disable switch
/// must move the WORK and must not move the ANSWER: with reuse off the same
/// warm analysis reuses nothing, and publishes exactly what it published with
/// reuse on.
#[test]
fn disabling_world_reuse_changes_the_work_and_not_the_answer() {
    let _guard = OVERRIDE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (directory, entry) = write_module_package(
        "switch",
        "fun broken(): i32 {\n\tlet total = 1;\n\ttotal = 2;\n\ttotal\n}\n\n\
         [deprecated(\"use broken\")]\nfun stale(): i32 {\n\t0\n}\n\n\
         fun caller(): i32 {\n\tstale()\n}\n",
    );

    vilan_core::analyzer::set_world_reuse(true);
    vilan_core::analyzer::base_cache_clear();
    let reused = warm_pair(&directory, &entry);

    vilan_core::analyzer::set_world_reuse(false);
    vilan_core::analyzer::base_cache_clear();
    let derived = warm_pair(&directory, &entry);
    vilan_core::analyzer::set_world_reuse(true);

    assert!(
        reused.3.0 > 0,
        "the pin is vacuous unless the reusing leg actually reused: {:?}",
        reused.3
    );
    assert_eq!(
        derived.3.0, 0,
        "the switch must turn the widened seam OFF, not merely narrow it: {:?}",
        derived.3
    );
    assert_eq!(reused.0, derived.0, "replay changed the diagnostics");
    assert_eq!(
        reused.1, derived.1,
        "replay changed the warnings or their attribution"
    );
    assert_eq!(reused.2, derived.2, "replay changed the emitted JS");

    let _ = std::fs::remove_dir_all(&directory);
    vilan_core::analyzer::base_cache_clear();
}

/// The owner's Q5, made a test: the `Note.source` index invariant is an
/// ASSERTION, not a comment. A replayed note carries an index into the world's
/// `sources` vector; reorder that vector under a record and the replay must
/// refuse rather than point every remembered note at the wrong file.
#[test]
#[should_panic(expected = "the world's `sources` vector moved")]
fn the_note_source_index_invariant_is_asserted() {
    let sources = vec![
        PathBuf::from("main.vl"),
        PathBuf::from("theme.vl"),
        PathBuf::from("views.vl"),
    ];
    let recorded = vilan_core::analyzer::replay_sources_fingerprint(&sources);
    // Same files, different order — the exact shape a later tranche that
    // prunes or re-sorts `sources` would produce, and the one that silently
    // re-homes a note today.
    let reordered = vec![
        PathBuf::from("main.vl"),
        PathBuf::from("views.vl"),
        PathBuf::from("theme.vl"),
    ];
    assert_ne!(
        recorded,
        vilan_core::analyzer::replay_sources_fingerprint(&reordered),
        "the fingerprint must be order-sensitive, or the assertion below is \
         vacuous"
    );
    vilan_core::analyzer::assert_replay_sources_stable(recorded, &reordered);
}
