//! M110 Q3 — the EDIT-REPLAY DIFFERENTIAL (`incremental-analysis.md` §6).
//!
//! Every incremental slice answers from something it remembered: a base-cache
//! world (S3c), a module's replayed checks (M19), the stored PREFIX of an entry's
//! world with only the edited module and its importers re-walked (S1). This
//! binary is the gate that holds all of them to one claim: **an incremental
//! analysis answers exactly what a clean one does**. Packages are edited the
//! way an editor edits them — an open buffer moves (the document overlay), the
//! edited file is the analysis's hot seed (`Workspace::hot_seeds`, what the
//! language server passes), and each edit is UNDONE again, because the undo is
//! where stale-cache bugs live — and after every step the incremental analysis
//! is compared with a clean one (`vilan_core::incremental::clean_analysis`: no
//! base-cache world, no replayed record, no hot set).
//!
//! What is compared is what a user can observe, rendered without ids
//! (`render_observation`): every diagnostic and warning with the file it
//! publishes to, its span, message, note and trace; every hover type label,
//! declaration label and inlay hint at its (file, span); every resolved name
//! and type reference from where it is written to where it points. Ids and
//! `SourceId` indices are the one thing an incremental world is ALLOWED to
//! number differently (S1 walks the hot set after the prefix), and nothing a
//! user sees is keyed by them. Emitted JS is compared too, where the program is
//! clean enough to emit.
//!
//! Two legs: the §6 EDIT CLASSES over hand-written multi-module packages
//! (a body literal's type, a context read, a sleep, a module binding's element
//! type, a written return, a field rename, an impl in an unimported module, a
//! derive, an import, a `const` callee, an import cycle, a prefix module
//! changing under a seed elsewhere, a browser entry reaching a node-only call),
//! and the CORPUS, every program re-hosted as a package module with an
//! importer and a bystander. Each slice's planted bug (`incremental::Plant`)
//! must turn the classes leg red — the M57 lesson.
//!
//! The overlay and the plant are process-global, so the tests serialize on
//! [`SWITCH_LOCK`] (plain `cargo test` runs them as threads of one process).

mod replay_harness;
mod scratch;

use std::path::PathBuf;
use std::sync::Mutex;

use replay_harness::packages::*;
use vilan_core::Platform;
use vilan_core::incremental::{Census, Plant, set_plant};

static SWITCH_LOCK: Mutex<()> = Mutex::new(());

/// Applies `edit`, observes incrementally and cleanly, compares; then undoes it
/// and does the same. Answers every divergence found and the incremental
/// censuses (for the caller's non-vacuity checks).
fn replay(package: &mut Package, edit: &Edit, divergences: &mut Vec<String>) -> Vec<Census> {
    let before = package.text(edit.file).to_string();
    let mut after = before.clone();
    for (find, replace) in edit.replacements {
        assert!(
            after.contains(find),
            "{}: the edit's anchor {find:?} is not in {}",
            edit.label,
            edit.file
        );
        after = after.replacen(find, replace, 1);
    }
    let seed = package.path(edit.seed.unwrap_or(edit.file));
    let mut censuses = Vec::new();
    for (phase, text) in [("edit", after), ("undo", before)] {
        package.set(edit.file, text);
        let incremental = observe(package, vec![seed.clone()], Leg::Incremental);
        let clean = observe(package, Vec::new(), Leg::Clean);
        let same_shape = observe(package, vec![seed.clone()], Leg::CleanSameShape);
        if incremental.rendering != clean.rendering {
            divergences.push(format!(
                "{} ({phase}): the incremental analysis differs from the clean one at {}",
                edit.label,
                first_difference(&incremental.rendering, &clean.rendering)
            ));
        }
        if incremental.javascript != same_shape.javascript {
            divergences.push(format!(
                "{} ({phase}): the emitted JS differs from a clean analysis of the same \
                 shape (incremental {} bytes, clean {} bytes)",
                edit.label,
                incremental.javascript.as_ref().map_or(0, String::len),
                same_shape.javascript.as_ref().map_or(0, String::len),
            ));
        }
        // Whether the program emits at all is a user-visible answer, so it
        // has to agree with the canonical world too.
        if incremental.javascript.is_some() != clean.javascript.is_some() {
            divergences.push(format!(
                "{} ({phase}): one analysis emits and the other does not",
                edit.label
            ));
        }
        censuses.push(incremental.census);
    }
    censuses
}

/// [`replay`] for [`STD_EDIT`]: std's file moves (and moves back), the seed
/// stays on the module whose const site calls it.
fn replay_std_edit(package: &Package, divergences: &mut Vec<String>) -> Vec<Census> {
    let path = std::fs::canonicalize(replay_harness::std_root().join(STD_EDIT_FILE))
        .expect("std's math module");
    let before = std::fs::read_to_string(&path).expect("read std's math module");
    assert!(
        before.contains(STD_EDIT.0),
        "{STD_EDIT_LABEL}: the edit's anchor is not in {STD_EDIT_FILE}"
    );
    let after = before.replacen(STD_EDIT.0, STD_EDIT.1, 1);
    let seed = package.path("data_user.vl");
    let mut censuses = Vec::new();
    for (phase, text) in [("edit", Some(after)), ("undo", None)] {
        vilan_core::analyzer::set_document_overlay(&path, text);
        let incremental = observe(package, vec![seed.clone()], Leg::Incremental);
        let clean = observe(package, Vec::new(), Leg::Clean);
        let same_shape = observe(package, vec![seed.clone()], Leg::CleanSameShape);
        if incremental.rendering != clean.rendering {
            divergences.push(format!(
                "{STD_EDIT_LABEL} ({phase}): the incremental analysis differs from the clean one at {}",
                first_difference(&incremental.rendering, &clean.rendering)
            ));
        }
        if incremental.javascript != same_shape.javascript
            || incremental.javascript.is_some() != clean.javascript.is_some()
        {
            divergences.push(format!(
                "{STD_EDIT_LABEL} ({phase}): the emitted JS differs from a clean analysis"
            ));
        }
        censuses.push(incremental.census);
    }
    censuses
}

/// What [`replay_the_classes`] observed: every divergence, every incremental
/// census, and the censuses of the edits whose SHAPE is asserted besides
/// their agreement — M121's served hot impls and B553's deferred entry.
struct Replayed {
    divergences: Vec<String>,
    censuses: Vec<Census>,
    served: Vec<Census>,
    entry_impl: Vec<Census>,
}

/// Runs every class edit over a fresh package, then the platform edit, B553's
/// entry impl and the leaf edits over their own, answering the divergences and
/// every incremental census.
fn replay_the_classes() -> Replayed {
    let mut divergences = Vec::new();
    let mut censuses = Vec::new();
    vilan_core::analyzer::base_cache_clear();
    let mut package = classes_package();
    // The first analysis of the package, before any edit: a cold world.
    let first = observe(&package, vec![package.path("views.vl")], Leg::Incremental);
    let clean = observe(&package, Vec::new(), Leg::Clean);
    if first.rendering != clean.rendering {
        divergences.push(format!(
            "the cold analysis differs from the clean one at {}",
            first_difference(&first.rendering, &clean.rendering)
        ));
    }
    for edit in CLASS_EDITS {
        censuses.extend(replay(&mut package, edit, &mut divergences));
    }
    censuses.extend(replay_std_edit(&package, &mut divergences));
    let mut served = Vec::new();
    for edit in SERVED_IMPL_EDITS {
        served.extend(replay(&mut package, edit, &mut divergences));
    }
    censuses.extend(served.iter().copied());
    package.remove();
    let mut package = Package::write(
        "entry_impl",
        Platform::default(),
        &[
            ("main.vl", ENTRY_IMPL_MAIN),
            ("shapes.vl", ENTRY_IMPL_SHAPES),
            ("waver.vl", ENTRY_IMPL_WAVER),
        ],
    );
    let mut entry_impl = Vec::new();
    for edit in ENTRY_IMPL_EDITS {
        entry_impl.extend(replay(&mut package, edit, &mut divergences));
    }
    censuses.extend(entry_impl.iter().copied());
    package.remove();
    let mut package = platform_package();
    for edit in PLATFORM_EDITS {
        censuses.extend(replay(&mut package, edit, &mut divergences));
    }
    package.remove();
    let mut package = leaf_package();
    for edit in LEAF_EDITS {
        censuses.extend(replay(&mut package, edit, &mut divergences));
    }
    package.remove();
    // M110 S2a: the bound audit's verdicts at PREFIX sites are recorded and
    // replayed; the plants that serve a stale record (the audit skipped
    // without recording, the impl guard off) must turn this package red.
    for fixture in BOUND_FIXTURES {
        let mut package = fixture.write();
        for edit in fixture.edits {
            censuses.extend(replay(&mut package, edit, &mut divergences));
        }
        package.remove();
    }
    vilan_core::analyzer::base_cache_clear();
    Replayed {
        divergences,
        censuses,
        served,
        entry_impl,
    }
}

/// **The gate.** Every §6 edit class, applied and undone, answers what a clean
/// analysis answers — byte for byte over everything an editor reads, and the
/// emitted JS wherever the program emits.
#[test]
fn every_edit_class_answers_what_a_clean_analysis_answers() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    let Replayed {
        divergences,
        censuses,
        served,
        entry_impl,
    } = replay_the_classes();
    let mut divergences = divergences;
    let mut censuses = censuses;
    // The post-pass class (pass map §6 "does not prove" 2; B575): a prefix
    // module's verdict a POST pass decides, under keystrokes elsewhere. In
    // the gate only: no plant targets a post pass, and each replay of them
    // is 48 analyses.
    for fixture in POST_PASS_FIXTURES {
        let mut package = fixture.write();
        for edit in fixture.edits {
            censuses.extend(replay(&mut package, edit, &mut divergences));
        }
        package.remove();
    }
    vilan_core::analyzer::base_cache_clear();
    assert!(
        divergences.is_empty(),
        "{} step(s) of the edit script observe incremental analysis:\n{}",
        divergences.len(),
        divergences.join("\n")
    );
    // Non-vacuity: the script has to have exercised the hot-set world, hit and
    // miss, or the comparison above says nothing about S1. Every edit here is
    // outside the entry, so without S1 nothing would be reused at all.
    let hot_worlds = censuses.iter().filter(|census| census.hot_world).count();
    let hot_hits = censuses
        .iter()
        .filter(|census| census.hot_world && census.base_hits > 0)
        .count();
    let replayed = censuses
        .iter()
        .filter(|census| census.records_replayed > 0)
        .count();
    eprintln!(
        "{} incremental analyses: {hot_worlds} hot-set worlds, {hot_hits} of them served \
         from the base cache, {replayed} replaying module records",
        censuses.len()
    );
    assert!(
        hot_hits >= 8 && replayed >= 8,
        "the script must exercise the hot-set world from the cache (served {hot_hits}, \
         replaying {replayed}); the differential above is otherwise vacuous about S1"
    );
    // M121: the hot impls no prefix question reaches are SERVED — a hot-set
    // world, the warm keystroke from the cache — not refused, which is what
    // S1's spelling guard did to every one of them.
    assert!(
        served
            .iter()
            .all(|census| census.hot_world && census.hot_refusal.is_none()),
        "a hot impl the stored prefix never asked about is served by the hot-set world: \
         {served:#?}"
    );
    // Each edit's first analysis stores the prefix its new seed keys; the undo
    // after it is the warm keystroke, served from that prefix.
    assert!(
        served
            .iter()
            .skip(1)
            .step_by(2)
            .all(|census| census.base_hits > 0),
        "the warm keystrokes reuse the stored prefix: {served:#?}"
    );
    // M110 S2a: the bound audit served PREFIX sites from the record on the
    // warm keystrokes (the undo of every edit is one) — or the comparison
    // above said nothing about the record.
    let bound_served: u64 = censuses
        .iter()
        .map(|census| census.bound_sites_served)
        .sum();
    let bound_checked: u64 = censuses
        .iter()
        .map(|census| census.bound_sites_checked)
        .sum();
    eprintln!("bound audit: {bound_served} sites served from records, {bound_checked} checked");
    assert!(
        bound_served > 0 && bound_checked > 0,
        "the classes leg must serve bound-audit sites from a reused module's record \
         (served {bound_served}, checked {bound_checked})"
    );
    // M110 S4: the const cache served sites — or every comparison above said
    // nothing about it.
    let const_hits: u64 = censuses.iter().map(|census| census.const_cache_hits).sum();
    assert!(
        const_hits > 0,
        "the classes leg must serve const sites from the const cache"
    );
    // B553: the entry's impl is one a STORED module calls, so the entry's own
    // keystrokes resolve the world once, after the entry walks — and agree.
    // A keystroke in the calling module makes it hot: it resolves after the
    // entry walked anyway, so its world is served as an ordinary hot set.
    let (entry_keystrokes, caller_keystrokes) = entry_impl.split_at(2);
    assert!(
        entry_keystrokes
            .iter()
            .all(|census| census.resolve_deferred && !census.hot_world),
        "an entry impl a stored module calls defers the world's resolve: {entry_impl:#?}"
    );
    assert!(
        caller_keystrokes
            .iter()
            .all(|census| !census.resolve_deferred && census.hot_world),
        "the calling module, hot, is served without deferring: {entry_impl:#?}"
    );
}

/// Non-vacuity of the post-pass class: each of its packages carries, in its
/// PREFIX module, a diagnostic a post pass decides — or the class says nothing
/// about what a reusing analysis drops.
#[test]
fn each_post_pass_package_carries_a_prefix_verdict() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    for fixture in POST_PASS_FIXTURES {
        let package = fixture.write();
        let clean = observe(&package, Vec::new(), Leg::Clean);
        let module = package.path("m.vl").display().to_string();
        package.remove();
        let diagnostics = clean
            .rendering
            .split("# warnings")
            .next()
            .unwrap_or("")
            .lines()
            .filter(|row| row.starts_with(&module))
            .count();
        assert!(
            diagnostics >= 1,
            "{}: the prefix module carries a post-pass verdict:\n{}",
            fixture.name,
            clean.rendering
        );
    }
}

// --- the corpus ----------------------------------------------------------------

/// **The corpus leg.** Every corpus program, re-hosted as the module of a
/// package with an importer (`user.vl`, in the module's hot set) and a
/// bystander (`side.vl`, in the stored prefix), edited and undone as above.
/// The corpus programs are not written to be modules and many are not clean
/// ones: what each means as a module it means identically to both analyses,
/// which is all the leg asks.
#[test]
fn the_corpus_as_modules_answers_what_a_clean_analysis_answers() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
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
    vilan_core::analyzer::base_cache_clear();
    let results: Vec<(String, Vec<String>, Vec<Census>)> = std::thread::scope(|scope| {
        let workers: Vec<_> = paths
            .chunks(paths.len().div_ceil(16).max(1))
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|path| {
                            let mut source =
                                std::fs::read_to_string(path).expect("read corpus file");
                            source.push_str(PROBE_REFUSAL);
                            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
                            let mut package = Package::write(
                                &name,
                                Platform::default(),
                                &[
                                    ("main.vl", CORPUS_MAIN),
                                    ("module.vl", &source),
                                    ("user.vl", CORPUS_USER),
                                    ("side.vl", CORPUS_SIDE),
                                ],
                            );
                            let mut divergences = Vec::new();
                            let mut censuses = Vec::new();
                            for edit in CORPUS_EDITS {
                                censuses.extend(replay(&mut package, edit, &mut divergences));
                            }
                            package.remove();
                            (name, divergences, censuses)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("worker panicked"))
            .collect()
    });
    vilan_core::analyzer::base_cache_clear();
    let divergences: Vec<String> = results
        .iter()
        .flat_map(|(name, divergences, _)| {
            divergences
                .iter()
                .map(move |divergence| format!("{name}: {divergence}"))
        })
        .collect();
    assert!(
        divergences.is_empty(),
        "{} corpus step(s) observe incremental analysis:\n{}",
        divergences.len(),
        divergences.join("\n")
    );
    // Non-vacuity: the corpus has to have run through the hot-set world, from
    // the cache, often enough to say something about it. Many corpus programs
    // are refused one (an impl on a std type, a use-inferred binding), which is
    // the guards working; the floor is on the ones that were not.
    let censuses: Vec<&Census> = results
        .iter()
        .flat_map(|(_, _, censuses)| censuses)
        .collect();
    let hot_hits = censuses
        .iter()
        .filter(|census| census.hot_world && census.base_hits > 0)
        .count();
    let hot_worlds = censuses.iter().filter(|census| census.hot_world).count();
    eprintln!(
        "{} incremental corpus analyses: {hot_worlds} hot-set worlds, {hot_hits} served from the base cache",
        censuses.len()
    );
    assert!(
        hot_hits >= 20,
        "only {hot_hits} corpus analyses were hot-set worlds served from the cache — the leg \
         says too little about S1"
    );
}

// --- the planted bugs (Q3's non-vacuity) -----------------------------------------

/// The classes leg with `plant` planted: answers the divergences it found.
fn replay_with_plant(plant: Plant) -> Vec<String> {
    set_plant(Some(plant));
    let replayed = std::panic::catch_unwind(replay_the_classes);
    set_plant(None);
    replayed
        .expect("the classes leg panicked under a plant")
        .divergences
}

/// S1's plant: the hot modules' remembered checks replayed as if they were the
/// stored prefix's. A Class A refusal typed into a hot module is then served
/// stale — the gate must see it.
#[test]
fn the_differential_sees_a_hot_module_replayed_from_a_record() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let divergences = replay_with_plant(Plant::HotSetReplay);
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("Class A refusal")),
        "the hot-set replay plant must turn the Class A refusal edit red; it found: {divergences:#?}"
    );
}

/// S1's plant: the stored prefix served without its content check. A prefix
/// module edited under a seed elsewhere is then served stale.
#[test]
fn the_differential_sees_a_prefix_served_unvalidated() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let divergences = replay_with_plant(Plant::PrefixUnvalidated);
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("PREFIX module the leaf imports")),
        "the unvalidated-prefix plant must turn the prefix edit red; it found: {divergences:#?}"
    );
}

/// S1's plant: the impl guard dropped. A hot module's inherent impl on a
/// prefix type is then invisible to the prefix caller that uses it.
#[test]
fn the_differential_sees_a_hot_impl_the_prefix_needed() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let divergences = replay_with_plant(Plant::ImplGuardOff);
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("inherent impl the prefix calls")),
        "the impl-guard plant must turn the hot impl edit red; it found: {divergences:#?}"
    );
}

/// S1's plant: the use-inferred-binding guard dropped. A hot module that
/// decides a prefix binding's type by its first use — a push into a module's
/// empty list — is then walked after the prefix decided it, where the
/// canonical world walks it first. (A context's `run` no longer is such a
/// hazard: since B584 a call on a context's open value slot binds to the slot
/// itself, so the prefix's calls take whatever `run` fills it, in either walk
/// order — the context edit agrees under the plant, and the guard's context
/// arm is incr's to retire.)
#[test]
fn the_differential_sees_a_binding_the_hot_set_should_have_decided() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let divergences = replay_with_plant(Plant::UseInferredGuardOff);
    assert!(
        !divergences
            .iter()
            .any(|divergence| divergence.contains("grounds a prefix context")),
        "B584: the context edit is order-free, even with the guard off: {divergences:#?}"
    );
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("from the module loaded first")),
        "the use-inferred plant must turn the module-binding push red; it found: {divergences:#?}"
    );
}

/// B573: a keystroke in a twinned module of a BROWSER package, served from the
/// stored world (a hot-set hit), walks the module's twins under the browser —
/// the stored world carries the platform it was built for. Before B573 the
/// canonical and the hot-set world chose the `@process` twin alike, so the
/// differential agreed with itself; this asserts the twin.
#[test]
fn a_hot_twin_module_keeps_its_platforms_twin() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    vilan_core::analyzer::base_cache_clear();
    let package = platform_package();
    let seed = vec![package.path("twin.vl")];
    let _ = observe(&package, seed.clone(), Leg::Incremental);
    let warm = observe(&package, seed, Leg::Incremental);
    package.remove();
    vilan_core::analyzer::base_cache_clear();
    assert!(
        warm.census.hot_world && warm.census.base_hits == 1,
        "the twin module's keystroke is a hot-set hit: {:?}",
        warm.census
    );
    let javascript = warm.javascript.expect("the browser package emits");
    assert!(
        javascript.contains("browser twin") && !javascript.contains("process twin"),
        "the browser world holds the browser twin:\n{javascript}"
    );
}

/// S4's plant: a remembered const site served without asking its project reads
/// again. The input-file edit is then served stale.
#[test]
fn the_differential_sees_a_const_site_served_without_its_reads() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    vilan_core::const_cache::clear();
    let divergences = replay_with_plant(Plant::ConstCacheUnvalidated);
    vilan_core::const_cache::clear();
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("input file a const site reads")),
        "the unvalidated-const plant must turn the input-file edit red; it found: {divergences:#?}"
    );
}

/// S4's plant: a const site keyed without the world declarations it reaches. A
/// callee edited — in the package, and in std — is then served stale.
#[test]
fn the_differential_sees_a_const_key_without_its_callees() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    vilan_core::const_cache::clear();
    let divergences = replay_with_plant(Plant::ConstKeyWithoutWorld);
    vilan_core::const_cache::clear();
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("const callee's value edited")),
        "the callee-blind key plant must turn the const callee edit red; it found: {divergences:#?}"
    );
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains(STD_EDIT_LABEL)),
        "the callee-blind key plant must turn the std edit red; it found: {divergences:#?}"
    );
}

/// S2a's plant: the bound audit skips a reused module's sites and records
/// nothing for them. A prefix module's bound refusal is then dropped on the
/// hit — the gate must see it.
#[test]
fn the_differential_sees_a_bound_audit_served_without_its_record() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let divergences = replay_with_plant(Plant::BoundAuditUnrecorded);
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("bound refusal")),
        "the unrecorded-audit plant must turn the bound package red; it found: {divergences:#?}"
    );
}

/// S2a's second plant: a module's recorded audit replayed without asking
/// whether a LATE impl answers one of its questions. The impl a prefix
/// module's bound is satisfied through is moved to another type in a hot
/// module, and the record keeps the answer the world no longer gives. (S1's
/// impl guard is not what protects the record — the audit's questions are
/// asked after the store, so the record carries its own test — which is why
/// `ImplGuardOff` leaves this package green.)
#[test]
fn the_differential_sees_a_bound_record_kept_over_a_moved_impl() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let divergences = replay_with_plant(Plant::BoundRecordUnguarded);
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("bound is satisfied through, moved")),
        "the unguarded-record plant must turn the moved-impl edit red; it found: {divergences:#?}"
    );
}

/// S2b's plant: the tail skips a reused module's label rendering and drops
/// the restored rows — the prefix's hover types and declaration labels are
/// then missing on the hit; the gate must see it.
#[test]
fn the_differential_sees_label_tables_served_without_their_rows() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let divergences = replay_with_plant(Plant::LabelTablesUnrecorded);
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("type ") || divergence.contains("declaration ")),
        "the unrecorded-labels plant must turn a hover label red; it found: {divergences:#?}"
    );
}

/// S2b's other seam: an analysis NO front end seeded (a plain base-cache hit
/// — the CLI's watch, an editor leg served without a seed) reuses modules
/// whose record was filed by an unseeded analysis and so carries no label
/// rows; the tail must render their labels, not skip them. The hit's
/// rendering is compared with the clean one in full (the hover types and
/// declaration labels included), where the fixup that filed rows only for a
/// seeded analysis left four of vilan-lsp's hovers empty.
#[test]
fn a_reused_module_keeps_its_labels_without_a_seed() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    vilan_core::analyzer::base_cache_clear();
    let package = leaf_package();
    let cold = observe(&package, Vec::new(), Leg::Incremental);
    let warm = observe(&package, Vec::new(), Leg::Incremental);
    let clean = observe(&package, Vec::new(), Leg::Clean);
    package.remove();
    vilan_core::analyzer::base_cache_clear();
    assert!(
        cold.census.base_misses == 1 && warm.census.base_hits == 1,
        "the second unseeded analysis is served the stored world: {:?} / {:?}",
        cold.census,
        warm.census
    );
    assert!(
        warm.census.records_replayed > 0,
        "the hit replays module records: {:?}",
        warm.census
    );
    assert!(
        warm.rendering == clean.rendering,
        "an unseeded hit renders every label a clean analysis renders: {}",
        first_difference(&warm.rendering, &clean.rendering)
    );
}

/// B569 (lang-a-49, at C3's request): a reused PREFIX module's labelled
/// tuples — the labels its hover rows and declaration labels print, which S2b
/// records per module and restores on reuse, and the label refusal the tuple
/// rule records during inference — render on every keystroke elsewhere, and
/// on an unseeded hit, exactly as a clean analysis renders them.
#[test]
fn b569_a_reused_modules_tuple_labels_render_as_a_clean_analysis_renders() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    vilan_core::analyzer::base_cache_clear();
    let mut divergences = Vec::new();
    let mut censuses = Vec::new();
    let mut package = LABELS_FIXTURE.write();
    for edit in LABELS_FIXTURE.edits {
        censuses.extend(replay(&mut package, edit, &mut divergences));
    }
    vilan_core::analyzer::base_cache_clear();
    let cold = observe(&package, Vec::new(), Leg::Incremental);
    let warm = observe(&package, Vec::new(), Leg::Incremental);
    let clean = observe(&package, Vec::new(), Leg::Clean);
    package.remove();
    vilan_core::analyzer::base_cache_clear();
    assert!(divergences.is_empty(), "{divergences:#?}");
    assert!(
        censuses.iter().any(|census| census.records_replayed > 0),
        "the keystrokes reuse the prefix module's records: {censuses:#?}"
    );
    assert!(
        cold.census.base_misses == 1 && warm.census.base_hits == 1,
        "the second unseeded analysis is served the stored world: {:?} / {:?}",
        cold.census,
        warm.census
    );
    assert!(
        warm.rendering == clean.rendering,
        "an unseeded hit renders every label a clean analysis renders: {}",
        first_difference(&warm.rendering, &clean.rendering)
    );
    for printed in [
        "(min: i32, max: i32)",
        "(x: f64, y: f64)",
        "`z` is not a label of `(x: i32, y: i32)`",
    ] {
        assert!(
            clean.rendering.contains(printed),
            "the fixture renders {printed:?} (or it says nothing about labels):\n{}",
            clean.rendering
        );
    }
}

/// M121 / B553's cost: a check whose late files write no impl on a type they
/// do not declare RECORDS NOTHING — the pre-entry resolve asks the impl table
/// thousands of questions and none of them is kept (`reach_questions` 0), which
/// is every `vilan check` of the gate's examples. An entry that does write one
/// (B553's package) records, or the zero says nothing.
#[test]
fn a_check_whose_late_files_write_no_foreign_impl_records_nothing() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    vilan_core::analyzer::base_cache_clear();
    let package = leaf_package();
    let check = observe(&package, Vec::new(), Leg::Incremental);
    package.remove();
    let package = Package::write(
        "entry_impl_census",
        Platform::default(),
        &[
            ("main.vl", ENTRY_IMPL_MAIN),
            ("shapes.vl", ENTRY_IMPL_SHAPES),
            ("waver.vl", ENTRY_IMPL_WAVER),
        ],
    );
    let foreign = observe(&package, Vec::new(), Leg::Incremental);
    package.remove();
    vilan_core::analyzer::base_cache_clear();
    assert!(
        check.census.base_misses == 1 && check.census.reach_questions == 0,
        "a check with no foreign impl in its entry records nothing: {:?}",
        check.census
    );
    assert!(
        foreign.census.reach_questions > 0,
        "an entry impl on a module's type is recorded against: {:?}",
        foreign.census
    );
}

// --- the re-walk counter pins (Q9) ------------------------------------------------

/// What a keystroke in `file` re-walks once the hot-set world is warm: the
/// census of the SECOND of two analyses seeded there (the first stores the
/// prefix).
fn rewalked_on_a_warm_keystroke(package: &mut Package, file: &str) -> Census {
    let original = package.text(file).to_string();
    let seed = vec![package.path(file)];
    package.set(file, format!("{original}\n"));
    let _ = observe(package, seed.clone(), Leg::Incremental);
    package.set(file, original);
    observe(package, seed, Leg::Incremental).census
}

/// The re-walk counts the paper's §4.4 names, pinned on counts rather than
/// clocks (Q9): a leaf importer's keystroke re-walks the leaf and the entry; a
/// cycle member's re-walks the cycle and the entry; the entry's own re-walks
/// the entry alone — each served from the base cache. Run twice: as built,
/// and with the hot set planted as the WHOLE package, which must move every
/// count (the pins' non-vacuity).
#[test]
fn a_keystroke_rewalks_its_hot_set_and_nothing_else() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let measure = || {
        vilan_core::analyzer::base_cache_clear();
        let mut package = leaf_package();
        let leaf = rewalked_on_a_warm_keystroke(&mut package, "views.vl");
        let cycle = rewalked_on_a_warm_keystroke(&mut package, "cycle_b.vl");
        let entry = rewalked_on_a_warm_keystroke(&mut package, "main.vl");
        let hub = rewalked_on_a_warm_keystroke(&mut package, "hub.vl");
        package.remove();
        vilan_core::analyzer::base_cache_clear();
        (leaf, cycle, entry, hub)
    };
    set_plant(None);
    let (leaf, cycle, entry, hub) = measure();
    assert!(
        leaf.hot_world && leaf.base_hits == 1,
        "a leaf keystroke is a hot-set hit: {leaf:?}"
    );
    assert_eq!(
        (leaf.hot_modules, leaf.sources_walked),
        (2, 2),
        "a keystroke in a leaf importer re-walks the leaf and the entry: {leaf:?}"
    );
    assert!(
        cycle.hot_world && cycle.base_hits == 1,
        "a cycle keystroke is a hot-set hit: {cycle:?}"
    );
    assert_eq!(
        (cycle.hot_modules, cycle.sources_walked),
        (3, 3),
        "a keystroke in a cycle member re-walks the cycle and the entry: {cycle:?}"
    );
    assert!(
        !entry.hot_world && entry.base_hits == 1,
        "the entry's keystroke hits the ordinary world: {entry:?}"
    );
    assert_eq!(
        entry.sources_walked, 1,
        "the entry's keystroke re-walks the entry: {entry:?}"
    );
    // `hub` is imported by `views` and by the entry: its hot set is all three.
    assert_eq!(
        (hub.hot_modules, hub.sources_walked),
        (3, 3),
        "a shared module's keystroke re-walks it and its importers: {hub:?}"
    );
    assert!(
        leaf.package_modules == 5 && leaf.records_replayed > 0,
        "the census reads the whole package and the replayed prefix: {leaf:?}"
    );

    set_plant(Some(Plant::WholePackageHot));
    let (leaf, cycle, _, hub) = measure();
    set_plant(None);
    assert!(
        leaf.sources_walked > 2 && cycle.sources_walked > 3 && hub.sources_walked > 3,
        "with the hot set planted as the whole package every count must move, or the \
         pins above are vacuous: leaf {leaf:?}, cycle {cycle:?}, hub {hub:?}"
    );
}
