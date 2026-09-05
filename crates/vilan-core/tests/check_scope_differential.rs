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
type ReuseObservation = (String, String, Option<String>, (usize, usize, usize), usize);

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
                vilan_core::analyzer::table_reuse_census(),
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
/// **M19 T1b's class D probe**, appended to every corpus module beside the
/// three T1 planted.
///
/// It exists because the first cut of T1b's corpus leg was VACUOUS and the
/// stale-table plant proved it: the corpus entry imports its module and calls
/// nothing, so emission prunes every module body, and a differential on
/// emitted JavaScript compares two empty programs. Class D produces no
/// diagnostics at all — its whole output is the tables the transformer reads —
/// so a leg that cannot see the emitted bytes of a module's body cannot see
/// this tranche.
///
/// Each of the three functions lands in a different restored table: `take`
/// returns a place its own frame owns (a `compute_return_clone_sites` row that
/// declines the copy), `copy` binds and hands back a place it does not (one
/// that owes it, plus a `LastUse` row that decides the elision), and `grow`
/// takes a `&mut` receiver whose `bumps` verdict `infer_bumps` has to infer
/// rather than read off the native table.
const PROBE_TABLES: &str = r#"
struct M19ProbeBag { items: List<i32> }

fun m19_probe_take(own bag: M19ProbeBag): List<i32> {
	bag.items
}

fun m19_probe_copy(items: List<i32>): List<i32> {
	let held = items;
	held
}

fun m19_probe_grow(bag: &mut M19ProbeBag, value: i32) {
	bag.items.push(value);
}
"#;

/// The entry the class D leg uses: it REACHES the probe above, so the module's
/// bodies survive emission and the JavaScript comparison has something in it.
fn table_probe_entry(revision: u32) -> String {
    format!(
        "import pkg::module::{{ M19ProbeBag, m19_probe_take, m19_probe_copy, m19_probe_grow }};\n\n\
         fun main() {{\n\tlet revision = {revision};\n\
         \tmut bag = M19ProbeBag {{ items = [revision] }};\n\
         \tm19_probe_grow(&mut bag, 2);\n\
         \tlet held = m19_probe_copy([3]);\n\
         \tlet taken = m19_probe_take(bag);\n\
         \tprint(\"{{held.len()}} {{taken.len()}}\");\n}}\n"
    )
}

/// The corpus, re-hosted one program per package as a MODULE under a dependent
/// entry, each with its probes appended.
fn module_corpus() -> (Vec<PathBuf>, Vec<(PathBuf, PathBuf)>) {
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
            source.push_str(PROBE_TABLES);
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            write_module_package(&name, &source)
        })
        .collect();
    (paths, packages)
}

/// Both corpus legs analyze every package warm, in parallel chunks.
fn observe_corpus(packages: &[(PathBuf, PathBuf)]) -> Vec<ReuseObservation> {
    std::thread::scope(|scope| {
        let workers: Vec<_> = packages
            .chunks(packages.len().div_ceil(16).max(1))
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|(directory, entry)| {
                            let _ = observe_in_package(directory, entry, table_probe_entry(1));
                            observe_in_package(directory, entry, table_probe_entry(2))
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
}

/// **M19 T1b's own differential leg** (`per-module-analysis-reuse.md` §3.3,
/// class D): the same corpus-as-modules sweep, with T1's diagnostic replay ON
/// in BOTH legs and only the class D TABLE restore switched.
///
/// It is a separate leg rather than a widening of T1's for one reason: the
/// tranches fail differently. T1's replay is about what a module PUBLISHES, and
/// its assertion is on the diagnostics. T1b's restore is about the tables the
/// EMITTER reads — `LastUse`, the `bumps` verdicts, the clone-site decisions —
/// and its assertion is on the emitted JavaScript, which is the only place a
/// wrong clone decision or a lost last-use can show up at all. A single leg
/// that turned both off could not tell which half moved a byte.
#[test]
fn corpus_agrees_between_restored_and_recomputed_module_tables() {
    let _guard = OVERRIDE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (paths, packages) = module_corpus();

    vilan_core::analyzer::set_world_reuse(true);
    vilan_core::analyzer::set_world_table_reuse(false);
    vilan_core::analyzer::base_cache_clear();
    let recomputed = observe_corpus(&packages);
    vilan_core::analyzer::set_world_table_reuse(true);
    vilan_core::analyzer::base_cache_clear();
    let restored = observe_corpus(&packages);
    vilan_core::analyzer::base_cache_clear();

    for (directory, _) in &packages {
        let _ = std::fs::remove_dir_all(directory);
    }

    let mut divergences = Vec::new();
    for ((path, restored), recomputed) in paths.iter().zip(&restored).zip(&recomputed) {
        let name = path.file_name().unwrap().to_string_lossy();
        if restored.0 != recomputed.0 {
            divergences.push(format!(
                "{name}: diagnostics differ\n  restored:   {}\n  recomputed: {}",
                restored.0, recomputed.0
            ));
        }
        if restored.1 != recomputed.1 {
            divergences.push(format!("{name}: warnings or per-file attribution differ"));
        }
        if restored.2 != recomputed.2 {
            divergences.push(format!(
                "{name}: emitted JS differs — a restored class D table is not what the \
                 recomputed one says"
            ));
        }
    }
    assert!(
        divergences.is_empty(),
        "{} corpus programs observe the restored class D tables:\n{}",
        divergences.len(),
        divergences.join("\n")
    );

    // Non-vacuity in both directions, exactly as T1's leg asserts it: the
    // restoring leg must have restored, and the recomputing leg must not have.
    // The leg's OWN vacuity trap, and the reason `PROBE_TABLES` exists: class D
    // writes no diagnostic, so the emitted JavaScript is the only witness, and
    // a corpus whose module bodies were all pruned would agree about nothing.
    let emitting = restored.iter().filter(|row| row.2.is_some()).count();
    //
    // The bar is a COUNT, not a fraction, and the count is the measured one:
    // 41 of the 128 corpus programs survive re-hosting as a module under a
    // dependent entry that imports four of its names (the rest are not written
    // to be modules — the leg's subject is the seam, not the corpus, and T1's
    // leg says the same). Forty programs' worth of emitted JavaScript is the
    // sample this leg compares; the floor exists so a future change that prunes
    // the bodies again reds here instead of going quietly vacuous.
    assert!(
        emitting >= 30,
        "only {emitting} of {} corpus modules emitted JavaScript, so the class D \
         comparison is mostly between empty programs — the probe is not reaching \
         the module bodies",
        restored.len()
    );
    let with_tables = restored.iter().filter(|row| row.4 > 0).count();
    assert!(
        with_tables * 10 >= restored.len() * 9,
        "only {with_tables} of {} programs restored a module's TABLES — the leg is \
         nearly vacuous",
        restored.len()
    );
    assert!(
        recomputed.iter().all(|row| row.4 == 0),
        "the recomputing leg restored a table: the switch leaked"
    );
    // And T1's half must have stayed ON in both, or this leg is T1's leg again
    // under a different name.
    assert!(
        recomputed.iter().filter(|row| row.3.0 > 0).count() * 10 >= recomputed.len() * 9,
        "the recomputing leg stopped REPLAYING as well, so the two legs differ in \
         more than the tables"
    );
}

/// **The stale-table plant** (M19 T1b's red-first pin). The seam skips a
/// reused module's bodies and serves rows that describe a different program;
/// the emitted JavaScript must move. A green here with the plant on would mean
/// the tables are not read, and the differential above would be agreeing about
/// nothing.
#[test]
fn a_stale_restored_table_moves_the_emitted_javascript() {
    let _guard = OVERRIDE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // A module whose bodies produce class D rows in three of the restored
    // tables at once: a last use that lets a copy elide, a returned place that
    // owes one, and a `&mut` receiver whose `bumps` verdict is inferred.
    let (directory, entry) = write_module_package(
        "stale",
        "struct Bag { items: List<i32> }\n\n         fun take(own bag: Bag): List<i32> {\n\tbag.items\n}\n\n         fun copy(items: List<i32>): List<i32> {\n\tlet held = items;\n\theld\n}\n\n         fun grow(bag: &mut Bag, value: i32) {\n\tbag.items.push(value);\n}\n",
    );

    // The entry has to REACH the module's functions or emission prunes them and
    // the comparison below is between two empty programs.
    let calling_entry = |revision: u32| {
        format!(
            "import pkg::module::{{ Bag, take, copy, grow }};\n\n             fun main() {{\n\tlet revision = {revision};\n             \tmut bag = Bag {{ items = [revision] }};\n             \tgrow(&mut bag, 2);\n             \tlet held = copy([3]);\n             \tlet taken = take(bag);\n             \tprint(\"{{held.len()}} {{taken.len()}}\");\n}}\n"
        )
    };
    let warm_calls = |directory: &Path, entry: &Path| -> ReuseObservation {
        let _ = observe_in_package(directory, entry, calling_entry(1));
        observe_in_package(directory, entry, calling_entry(2))
    };

    vilan_core::analyzer::set_world_reuse(true);
    vilan_core::analyzer::set_world_table_reuse(true);
    vilan_core::analyzer::set_stale_table_plant(false);
    vilan_core::analyzer::base_cache_clear();
    let honest = warm_calls(&directory, &entry);

    vilan_core::analyzer::set_stale_table_plant(true);
    vilan_core::analyzer::base_cache_clear();
    let planted = warm_calls(&directory, &entry);
    vilan_core::analyzer::set_stale_table_plant(false);

    let _ = std::fs::remove_dir_all(&directory);
    vilan_core::analyzer::base_cache_clear();

    assert!(
        honest.4 > 0 && planted.4 > 0,
        "both legs must have RESTORED a module's tables, or the plant is not \
         planted in the path under test: honest {}, planted {}",
        honest.4,
        planted.4
    );
    assert!(
        honest.2.is_some(),
        "the fixture must emit JavaScript for the plant to move: {}",
        honest.0
    );
    assert_ne!(
        honest.2, planted.2,
        "a reused module whose bodies were SKIPPED and whose rows came back \
         EMPTY emitted byte-identical JavaScript — so the restored class D \
         tables are not read, and every agreement the differential reports is \
         vacuous"
    );
}

/// **The second soundness pin: a restored table must never describe text the
/// world no longer holds.**
///
/// The stale-table plant above proves the rows are READ. This proves the guard
/// that decides when they may be: the module's own bytes change between two
/// analyses, and the record filed under the old ones must not come back. The
/// two halves together are what "a restored table is never read for a module
/// whose content moved" means operationally — one says the rows matter, the
/// other says the key is exact.
///
/// The check is the strongest one available: the emitted JavaScript of the
/// EDITED program, warm, must equal the emitted JavaScript of the same edited
/// program analyzed with no record in the process at all.
#[test]
fn an_edited_module_is_not_served_its_old_tables() {
    let _guard = OVERRIDE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    vilan_core::analyzer::set_world_reuse(true);
    vilan_core::analyzer::set_world_table_reuse(true);
    vilan_core::analyzer::set_stale_table_plant(false);

    // Two revisions of one module whose class D answers differ: the first
    // returns the parameter it owns (a returned place the frame owns, no copy),
    // the second returns a place it borrowed (a copy the transformer has to
    // materialize), so their clone-site tables are not the same table.
    let owned = "fun pick(own items: List<i32>): List<i32> {\n\titems\n}\n";
    let borrowed = "fun pick(items: List<i32>): List<i32> {\n\titems\n}\n";
    let entry_source = |revision: u32| {
        format!(
            "import pkg::module::pick;\n\nfun main() {{\n\tlet revision = {revision};\n\
             \tlet chosen = pick([revision]);\n\tprint(\"{{chosen.len()}}\");\n}}\n"
        )
    };

    let (directory, entry) = write_module_package("edited", owned);
    vilan_core::analyzer::base_cache_clear();
    let _ = observe_in_package(&directory, &entry, entry_source(1));
    let warm = observe_in_package(&directory, &entry, entry_source(2));
    assert!(
        warm.4 > 0,
        "the pin is vacuous unless the second analysis actually restored the \
         module's tables: {:?}",
        warm
    );

    // The edit. The world key does not change — same paths, same package — so
    // what refuses the record is the content hash and nothing else.
    std::fs::write(directory.join("module.vl"), borrowed).expect("rewrite the module");
    let after_edit = observe_in_package(&directory, &entry, entry_source(3));
    assert_eq!(
        after_edit.4, 0,
        "the edited module was served tables recorded against its OLD text"
    );

    // The control: the same edited program with nothing remembered at all.
    vilan_core::analyzer::base_cache_clear();
    let cold = observe_in_package(&directory, &entry, entry_source(3));
    let _ = std::fs::remove_dir_all(&directory);
    vilan_core::analyzer::base_cache_clear();

    assert!(cold.2.is_some(), "the control did not emit: {}", cold.0);
    assert_eq!(
        after_edit.2, cold.2,
        "the analysis after the edit emitted different JavaScript from a cold \
         analysis of the same text"
    );
    assert_eq!(
        after_edit.0, cold.0,
        "the analysis after the edit published different diagnostics"
    );
}
