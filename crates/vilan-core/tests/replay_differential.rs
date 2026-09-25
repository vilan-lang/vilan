//! M19 T1's corpus replay differential, alone in a binary of its own (tracker
//! N57).
//!
//! It re-hosts every corpus program as a package MODULE under a dependent
//! entry, analyzes each package warm twice, and compares a leg that REPLAYS its
//! modules' remembered checks against one that re-derives them. That is the
//! right standing gate for M19 T1 — and it costs 60-170 s, which made it the
//! wrong thing to have inside `check_scope_differential`: a targeted
//! `-p vilan-core --test check_scope_differential` and every whole-crate gate
//! paid a corpus sweep to ask about the S1 seam. One binary, and
//! `.config/nextest.toml` starts it first, because a leg this long scheduled
//! late IS the critical path. It holds TWO tests since M76 (the module-shaped
//! sweep and the entry-shaped one), and both flip the process-global
//! `set_world_reuse` switch — so they take [`SWITCH_LOCK`], which nextest's
//! process-per-test never contends and plain `cargo test`, whose tests are
//! threads of one process, needs (N132, N129's class).
//!
//! **What "reused" is held to** (N132). The replaying leg's pairs share the base
//! cache's LRU with fifteen other workers, and a pair whose first analysis runs
//! long can find its world evicted before its second analysis reads it: a cold
//! miss that replays nothing and says nothing about the seam. So each pin holds
//! every pair that was SERVED from the base cache
//! (`vilan_core::analyzer::served_from_base_cache`) to reuse — exactly, no
//! exceptions — and holds the hit rate to a 90% floor, so a pin cannot pass on a
//! cache that served nothing. The budget is left at its default; lifting it to
//! unlimited (Order 41's fix) cost ~410 MiB of peak RSS for the run.
//!
//! The fixtures it builds are `replay_harness`'s, shared with the fast T1 pins
//! that stayed behind.

mod replay_harness;
mod scratch;

use std::path::PathBuf;

use replay_harness::{
    ReuseObservation, warm_open_pair, warm_pair, write_module_package, write_open_module_package,
};

/// Serializes the two tests: both flip `set_world_reuse`, which is
/// process-global (see the module comment).
static SWITCH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The replaying leg's non-vacuity, in the one form an LRU cannot flake
/// (N132): every pair whose world was SERVED from the base cache reused at
/// least one module, and at least 90% of the pairs were served. A pair the LRU
/// evicted before its second analysis is a miss, not a seam defect, and is
/// counted against the floor rather than failing the pin.
fn assert_every_hit_reused(replayed: &[ReuseObservation], names: &[PathBuf], leg: &str) {
    let hits: Vec<usize> = (0..replayed.len())
        .filter(|&index| replayed[index].4)
        .collect();
    let hit_but_idle: Vec<String> = hits
        .iter()
        .filter(|&&index| replayed[index].3.0 == 0)
        .map(|&index| {
            names[index]
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert!(
        hit_but_idle.is_empty(),
        "{leg}: {} pair(s) were served their world from the base cache and reused \
         nothing — the seam, not the LRU: {hit_but_idle:?}",
        hit_but_idle.len()
    );
    assert!(
        hits.len() * 10 >= replayed.len() * 9,
        "{leg}: only {} of {} pairs were served from the base cache — under the \
         90% floor, so the differential is close to vacuous",
        hits.len(),
        replayed.len()
    );
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
[resource] struct M19ProbeGuard { tag: str }

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
    let _switch = SWITCH_LOCK
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
    assert_every_hit_reused(&replayed, &paths, "the module-shaped sweep");
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

/// **The differential over an ENTRY-SHAPED world** (M76) — the same claim as
/// the sweep above, for the second shape the base cache holds.
///
/// M70 stored a world for a module a front end opened AS the entry, and left
/// its checks record withheld: such a world resolves inside the post-entry
/// `build()`, so a record of checks that ran over it was "a claim the seam has
/// not been proved to support". M76 files it, narrowing the READING side by
/// the alias-reach closure instead — and this is the proof, built exactly like
/// the sweep above so the two are read together: every corpus program becomes
/// the OPENED file of a three-file package, analyzed warm twice, and a leg
/// that replays its modules' remembered checks is compared against one that
/// re-derives them.
///
/// The probes are in `probe.vl`, the one module the record is read for: the
/// opened file is `SourceId(0)` and never reusable, and `ring.vl` reaches the
/// alias and is held back by construction. Its own test rather than a second
/// leg inside the sweep above, because nextest runs the two in parallel and a
/// sequential leg would have doubled the binary's wall.
#[test]
fn an_entry_shaped_world_agrees_between_replayed_and_rederived_module_checks() {
    let _switch = SWITCH_LOCK
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

    // The corpus program rides in `probe.vl` — the module whose remembered
    // checks the second analysis replays — with the same alternating probes
    // the sweep above appends, and for the same reason: sixty clean modules
    // agree whether the replay works or not.
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
            write_open_module_package(&name, &source)
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
                            .map(|(directory, opened)| warm_open_pair(directory, opened))
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

    // The budget stays at its default (N132): sixteen workers store a world
    // each, the default budget holds ~28-30 of them, and a pair whose first
    // analysis runs long can find its world evicted before its second reads
    // it. That miss is the LRU's, and the reuse assertion below counts it
    // against a hit-rate floor instead of failing on it.
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
        "{} corpus programs observe the entry-shaped check scope:\n{}",
        divergences.len(),
        divergences.join("\n")
    );

    // Non-vacuity, in both directions and with the PACKAGE module named: an
    // entry-shaped world whose only reused sources were std's would prove
    // nothing about M76, because std's Class A diagnostics are known absent
    // through S1's freeze whatever this seam does.
    assert_every_hit_reused(&replayed, &paths, "the entry-shaped sweep");
    let rederived: usize = derived
        .iter()
        .filter(|observation| observation.3.0 > 0)
        .count();
    assert_eq!(
        rederived, 0,
        "the re-deriving leg must reuse nothing — {rederived} pairs did, so the \
         two legs were not compared against different work"
    );
}
