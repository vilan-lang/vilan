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

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use vilan_core::{BuildOptions, PackageSpec, Platform, Workspace, analyze_source, transform};

static OVERRIDE_LOCK: Mutex<()> = Mutex::new(());

fn std_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std")
}

fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(&std_root())
}

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
