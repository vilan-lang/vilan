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

/// One package: `module.vl` holding `module_source`, and an entry that imports
/// it. Returns the directory (the caller removes it) and the entry path.
fn write_module_package(name: &str, module_source: &str) -> (PathBuf, PathBuf) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!(
        "vilan_m19_t1_{name}_{}_{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("create the package directory");
    std::fs::write(directory.join("module.vl"), module_source).expect("write the module");
    let entry = directory.join("main.vl");
    (directory, entry)
}

/// The entry that makes `module.vl` a LOADED sibling — held fixed except for
/// the digit, which is the keystroke.
fn module_entry(revision: u32) -> String {
    format!("import pkg::module;\n\nfun main() {{\n\tlet revision = {revision};\n}}\n")
}

/// Everything the M19 differential compares, plus the census that says whether
/// the run it came from actually reused anything (a differential that agreed
/// because nothing was reused would be vacuous).
type ReuseObservation = (String, String, Option<String>, (usize, usize, usize));

fn observe_in_package(
    pkg_root: &Path,
    entry_path: &Path,
    entry_source: String,
) -> ReuseObservation {
    let pkg_root = pkg_root.to_path_buf();
    let entry_path = entry_path.to_path_buf();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(entry_source.into_boxed_str());
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                &pkg_root,
                &entry_path,
                Some(Platform::default()),
                &Workspace::default(),
            );
            let diagnostics = format!("{errors:?}");
            // The FILE each diagnostic and warning publishes to rides in the
            // comparison beside its text: a replayed note carries a `SourceId`
            // INDEX (§3.2), and an index that drifted would show up here and
            // nowhere else.
            let warnings = program
                .as_ref()
                .map(|program| {
                    format!(
                        "{:?}#{:?}#{:?}",
                        program.warnings, program.warning_sources, program.diagnostic_sources
                    )
                })
                .unwrap_or_default();
            let javascript = match program {
                Some(program) if errors.is_empty() => {
                    transform(&program, &BuildOptions::default()).ok()
                }
                _ => None,
            };
            (
                diagnostics,
                warnings,
                javascript,
                vilan_core::analyzer::reuse_census(),
            )
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked")
}

/// A warm pair over one package: analysis 1 fills the world and records its
/// modules' checks, analysis 2 hits that world — and, unless reuse is off,
/// replays them. The SECOND observation is the one compared.
fn warm_pair(pkg_root: &Path, entry_path: &Path) -> ReuseObservation {
    let _ = observe_in_package(pkg_root, entry_path, module_entry(1));
    observe_in_package(pkg_root, entry_path, module_entry(2))
}

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

/// The Class A WARNING probe appended to every corpus module: a deprecated
/// function and a local call to it. A warning rather than a refusal, so the
/// module still analyzes and the emitted JS stays in the comparison.
const PROBE_WARNING: &str = r#"
[deprecated("use m19_probe_fresh")]
fun m19_probe_stale(): i32 {
	0
}

fun m19_probe_fresh(): i32 {
	m19_probe_stale()
}
"#;

/// The Class A REFUSAL probe, appended to every second module: a write to an
/// immutable `let`, which `check_readonly_mutation` refuses inside the
/// module's own body. This is the case a replayed diagnostic has to carry and
/// that no warning stands in for.
const PROBE_REFUSAL: &str = r#"
fun m19_probe_refusal(): i32 {
	let total = 1;
	total = 2;
	total
}
"#;

/// The Class A RESOURCE probe, appended to every third module: a resource
/// consumed twice, which `check_resource_moves` refuses. That check is the
/// largest single one the widened seam skips and it is INERT in a program
/// that declares no resource — and only three corpus programs declare one, so
/// without this the differential would be silent about exactly the check the
/// tranche buys the most from. The refusal names the binding, which is what
/// the leg counts.
const PROBE_RESOURCE: &str = r#"
resource struct M19ProbeGuard { tag: str }

impl M19ProbeGuard with Drop {
	fun drop(&mut self) {
	}
}

fun m19_probe_resource() {
	let m19_probe_held = M19ProbeGuard { tag = "probe" };
	drop(m19_probe_held);
	drop(m19_probe_held);
}
"#;

/// **The differential**, extended to the widened seam and the load-bearing
/// gate of the whole tranche (§5, and the owner's Q2 answer: replay is allowed
/// *with the differential as the standing gate*).
///
/// Every corpus program is re-hosted as a package MODULE under a dependent
/// entry — which is the shape T1 exists for and the one the single-file corpus
/// sweep above cannot reach, since a lone entry has no siblings to reuse. Each
/// package is analyzed twice; the second analysis is compared between a leg
/// that REPLAYS its modules' remembered checks and a leg that re-derives them.
/// Diagnostics, warnings, their per-file attribution and the emitted JS must
/// agree byte for byte.
///
/// The corpus programs are not written to be modules and many of them will not
/// be clean ones. That is fine and deliberate: the differential's subject is
/// the SEAM, and whatever a program means as a module it means identically on
/// both legs.
#[test]
fn corpus_agrees_between_replayed_and_rederived_module_checks() {
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

    // One package per program, written once and shared by both legs — so the
    // two legs differ in the switch and in nothing else, the package root
    // included (it is part of the base-cache key).
    //
    // Every module gets a PROBE appended, and it is what keeps the leg from
    // being vacuous: the corpus is the golden corpus, so as modules its
    // programs are clean, and a differential over sixty clean modules agrees
    // whether the replay works or not — which is exactly what a planted
    // no-op splice proved. The probes give every module something of its own
    // to remember. They alternate deliberately:
    //
    //  - a `[deprecated]` function and a local call to it — a Class A
    //    WARNING, which leaves the program analyzable and so keeps the
    //    emitted JS in the comparison;
    //  - on every second module, a write to an immutable `let` as well — a
    //    Class A REFUSAL, which is the case a replayed diagnostic has to
    //    carry and which no warning can stand in for.
    let packages: Vec<(PathBuf, PathBuf)> = paths
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let mut source = std::fs::read_to_string(path).expect("read corpus file");
            source.push_str(PROBE_WARNING);
            if index % 2 == 1 {
                source.push_str(PROBE_REFUSAL);
            }
            if index % 3 == 0 {
                source.insert_str(0, "import std::drop::{ Drop, drop };\n");
                source.push_str(PROBE_RESOURCE);
            }
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            write_module_package(&name, &source)
        })
        .collect();

    let observe_all = || -> Vec<ReuseObservation> {
        std::thread::scope(|scope| {
            let workers: Vec<_> = packages
                .chunks(packages.len().div_ceil(16).max(1))
                .map(|chunk| {
                    scope.spawn(move || {
                        chunk
                            .iter()
                            .map(|(directory, entry)| warm_pair(directory, entry))
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

    vilan_core::analyzer::set_world_reuse(false);
    vilan_core::analyzer::base_cache_clear();
    let derived = observe_all();
    vilan_core::analyzer::set_world_reuse(true);
    vilan_core::analyzer::base_cache_clear();
    let replayed = observe_all();
    vilan_core::analyzer::base_cache_clear();

    for (directory, _) in &packages {
        let _ = std::fs::remove_dir_all(directory);
    }

    let mut divergences = Vec::new();
    for ((path, replayed), derived) in paths.iter().zip(&replayed).zip(&derived) {
        let name = path.file_name().unwrap().to_string_lossy();
        if replayed.0 != derived.0 {
            divergences.push(format!(
                "{name}: diagnostics differ\n  replayed: {}\n  derived:  {}",
                replayed.0, derived.0
            ));
        }
        if replayed.1 != derived.1 {
            divergences.push(format!(
                "{name}: warnings or per-file attribution differ\n  replayed: {}\n  derived:  {}",
                replayed.1, derived.1
            ));
        }
        if replayed.2 != derived.2 {
            divergences.push(format!("{name}: emitted JS differs"));
        }
    }
    assert!(
        divergences.is_empty(),
        "{} corpus programs observe the widened check scope:\n{}",
        divergences.len(),
        divergences.join("\n")
    );

    // Non-vacuity, in both directions: the replaying leg must have reused, and
    // the re-deriving leg must not have. A differential over two runs that
    // both did the same thing proves nothing.
    let reusing = replayed
        .iter()
        .filter(|observation| observation.3.0 > 0)
        .count();
    assert!(
        reusing * 10 >= replayed.len() * 9,
        "only {reusing} of {} programs reused a module — the differential is \
         nearly vacuous; the packages are not hitting the base cache",
        replayed.len()
    );
    assert!(
        derived.iter().all(|observation| observation.3.0 == 0),
        "the re-deriving leg reused a module: the switch leaked"
    );
    // And the probes must actually have landed: a corpus of modules with
    // nothing to say agrees under any splice at all.
    let refusing = derived
        .iter()
        .filter(|observation| {
            observation.0.contains("m19_probe") || observation.0.contains("total")
        })
        .count();
    let warning = derived
        .iter()
        .filter(|observation| observation.1.contains("m19_probe_stale"))
        .count();
    assert!(
        refusing * 3 >= derived.len(),
        "only {refusing} of {} modules produced a replayable REFUSAL — the probe did not land, and the leg proves only that skipping CLEAN modules is free",
        derived.len()
    );
    assert!(
        warning * 3 >= derived.len() * 2,
        "only {warning} of {} modules produced a replayable WARNING — the probe did not land",
        derived.len()
    );
    let resourced = derived
        .iter()
        .filter(|observation| observation.0.contains("m19_probe_held"))
        .count();
    assert!(
        resourced >= 5,
        "only {resourced} modules produced the resource-move refusal — `check_resource_moves`, the largest check the seam skips, is INERT in a program that declares no resource, so without the probe this leg says nothing about it"
    );
}
