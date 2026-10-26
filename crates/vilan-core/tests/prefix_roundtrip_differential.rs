//! std-prefix P3's gate: the ROUND-TRIP DIFFERENTIAL (`std-prefix.md` §13).
//!
//! Every fixture of the replay harness, in every state its script puts it in,
//! is analyzed over the std prefix three ways and must answer alike, byte for
//! byte, in the id-free observation (`render_observation`: every diagnostic,
//! hover, declaration label, inlay hint and reference) and in the emitted
//! JavaScript:
//!
//! 1. **built** — the std-only prefix world built in this process, the world
//!    cache OFF (P1's in-process shape, the reference);
//! 2. **written** — built again with the world cache pointed at a scratch
//!    directory, which writes the file (or, for a later state of the same
//!    std closure, loads the one an earlier state wrote);
//! 3. **loaded** — the in-process caches cleared, the file looked for again
//!    and served: the world decoded from disk, re-rooted, inserted into the
//!    base cache and validated by content, the package walked over it.
//!
//! And the clean canonical analysis beside them, as the other differentials
//! compare (ids may differ; nothing a user sees may). The counters say the
//! loads happened, so the comparison is not vacuous about the file.
//!
//! Its own binary (named by this file), like the permutation differential:
//! it flips process-global switches (the world cache, the forced prefix) and
//! clears the base cache, which no other test may share a process with.

mod replay_harness;
mod scratch;
mod test_pool;

use std::path::{Path, PathBuf};

use replay_harness::packages::*;
use vilan_core::Platform;
use vilan_core::incremental::{set_std_prefix, set_world_cache};

/// A fresh world-cache directory for one test, under the scratch root.
fn cache_directory(name: &str) -> PathBuf {
    let directory = scratch::root().join(format!(
        "vilan_prefix_file_{name}_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("create the world cache directory");
    directory
}

/// Writes one package for `files` — every leg observes the SAME package, since
/// the rendering names its files by path.
fn write_package(
    name: &str,
    platform: Platform,
    files: &[(String, String)],
    dependencies: &[Dependency],
) -> Package {
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(file, text)| (file.as_str(), text.as_str()))
        .collect();
    Package::write_with(name, platform, &borrowed, dependencies)
}

/// Analyzes `package` over the std prefix with the world cache as `cache`
/// says, after clearing every in-process memory of worlds and files, so the
/// prefix is built or loaded — never hit.
fn observe_fresh(package: &Package, cache: Option<PathBuf>, leg: Leg) -> Observation {
    vilan_core::analyzer::base_cache_clear();
    vilan_core::analyzer::prefix_files_forget();
    set_world_cache(Some(cache));
    set_std_prefix(Some(true));
    let observation = observe(package, Vec::new(), leg);
    set_std_prefix(None);
    set_world_cache(None);
    observation
}

fn first_difference_of(
    label: &str,
    left_name: &str,
    left: &str,
    right_name: &str,
    right: &str,
) -> Option<String> {
    if left == right {
        return None;
    }
    let left_rows: Vec<&str> = left.lines().collect();
    let right_rows: Vec<&str> = right.lines().collect();
    let at = left_rows
        .iter()
        .zip(&right_rows)
        .position(|(a, b)| a != b)
        .unwrap_or(left_rows.len().min(right_rows.len()));
    Some(format!(
        "{label}: {left_name} and {right_name} part at row {at}:\n    {left_name}: {:?}\n    {right_name}: {:?}\n    ({} rows against {})",
        left_rows.get(at),
        right_rows.get(at),
        left_rows.len(),
        right_rows.len()
    ))
}

/// One state of one fixture, the three legs and the clean one: every
/// divergence, with the census of the loaded leg checked for the load.
fn compare_state(
    fixture: &Fixture,
    state: &str,
    files: &[(String, String)],
    cache: &Path,
    divergences: &mut Vec<String>,
) {
    let name = format!(
        "prefix_{}_{}",
        fixture.name,
        state
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .take(30)
            .collect::<String>()
    );
    let label = format!("{} / {state}", fixture.name);
    let package = write_package(&name, fixture.platform, files, fixture.dependencies);
    let built = observe_fresh(&package, None, Leg::Incremental);
    if !built.census.prefix_world {
        // A program the prefix refuses (a package impl a std question reaches,
        // a macro definition) is compared by the other differentials; the
        // file has nothing to say about it.
        package.remove();
        return;
    }
    let written = observe_fresh(&package, Some(cache.to_path_buf()), Leg::Incremental);
    match written.census.prefix_file {
        Some("written" | "loaded") => {}
        other => divergences.push(format!(
            "{label}: the first run with the world cache on neither wrote nor loaded the file: {other:?}"
        )),
    }
    let loaded = observe_fresh(&package, Some(cache.to_path_buf()), Leg::Incremental);
    if loaded.census.prefix_file != Some("loaded") {
        divergences.push(format!(
            "{label}: the second run with the world cache on did not load the file: {:?}",
            loaded.census.prefix_file
        ));
    }
    if !loaded.census.prefix_world {
        divergences.push(format!(
            "{label}: the loaded leg was not built over the prefix"
        ));
    }
    // M158: the prefix key's checks record rides beside the world, written by
    // the analysis that built it and inserted on load — so the loaded leg
    // replays std's Class A checks rather than deriving them. Except where
    // no analysis over the prefix key ever completes: an entry impl a stored
    // module calls (B553) resolves the world once, after the entry walks, in
    // another shape, so the record is never filed under the prefix key (and
    // never read there either).
    if !loaded.census.resolve_deferred && loaded.census.prefix_checks != Some("loaded") {
        divergences.push(format!(
            "{label}: the loaded leg did not load the prefix's checks record: {:?}",
            loaded.census.prefix_checks
        ));
    }
    let clean = observe_fresh(&package, None, Leg::Clean);
    package.remove();
    for (other_name, other) in [("written", &written), ("loaded", &loaded)] {
        divergences.extend(first_difference_of(
            &label,
            "built",
            &built.rendering,
            other_name,
            &other.rendering,
        ));
        if built.javascript != other.javascript {
            divergences.push(format!(
                "{label}: the JavaScript emitted over the built prefix and the {other_name} one differ"
            ));
        }
    }
    divergences.extend(first_difference_of(
        &label,
        "clean",
        &clean.rendering,
        "loaded",
        &loaded.rendering,
    ));
    if clean.javascript != loaded.javascript {
        divergences.push(format!(
            "{label}: the JavaScript emitted by the clean analysis and over the loaded prefix differ"
        ));
    }
}

fn compare_fixtures(name: &str, fixtures: &[&Fixture]) {
    let cache = cache_directory(name);
    // P6a (`std-prefix.md` §14.3): the tier rule's ORACLE is on for every leg
    // (its record rides the world and the file, keyed apart): no package impl
    // of these fixtures could have answered a question std's resolution asked
    // — the guard the rule replaced, demoted to this gate.
    vilan_core::incremental::set_tier_oracle(Some(true));
    let oracle_before = vilan_core::incremental::tier_oracle_hits();
    let (loads_before, _, _) = vilan_core::analyzer::prefix_file_counts();
    let mut divergences = Vec::new();
    let mut states = 0;
    for fixture in fixtures {
        for (state, files) in fixture.states() {
            states += 1;
            compare_state(fixture, &state, &files, &cache, &mut divergences);
        }
    }
    let (loads_after, writes, misses) = vilan_core::analyzer::prefix_file_counts();
    let oracle_hits = vilan_core::incremental::tier_oracle_hits() - oracle_before;
    vilan_core::incremental::set_tier_oracle(None);
    let _ = std::fs::remove_dir_all(&cache);
    assert_eq!(
        oracle_hits, 0,
        "P6a's oracle: a package impl of these fixtures could have answered a question the \
         tiers' resolution asked (the `[vilan phase] tier-oracle` line names it)"
    );
    assert!(
        divergences.is_empty(),
        "{} state(s) answer differently over the persisted std prefix:\n{}",
        divergences.len(),
        divergences.join("\n")
    );
    assert!(
        loads_after - loads_before >= 1 && writes >= 1,
        "the comparison must have loaded a prefix file ({} loads over {states} states, {writes} writes, {misses} misses); it is otherwise vacuous about P3",
        loads_after - loads_before
    );
}

/// **The gate, the classes package**: M110's many-module fixture (cycles, a
/// context, a use-inferred binding, derives, a const, a served impl) in
/// every state of its edit script, and the served-impl package beside it.
#[test]
fn the_classes_package_answers_the_same_over_a_loaded_prefix() {
    compare_fixtures("classes", &[&CLASSES_FIXTURE, &SERVED_IMPL_FIXTURE]);
}

/// **The gate, the other fixtures**: B553's entry impl, B573's browser twins,
/// the re-walk pins' leaf package, the four post-pass packages, the bound
/// fixtures, the effect fixtures, the window fixtures and the labels fixture.
#[test]
fn every_other_fixture_answers_the_same_over_a_loaded_prefix() {
    let mut fixtures: Vec<&Fixture> = vec![
        &ENTRY_IMPL_FIXTURE,
        &PLATFORM_FIXTURE,
        &LEAF_FIXTURE,
        &LABELS_FIXTURE,
    ];
    fixtures.extend(POST_PASS_FIXTURES.iter());
    fixtures.extend(BOUND_FIXTURES.iter());
    fixtures.extend(EFFECT_FIXTURES.iter());
    fixtures.extend(WINDOW_FIXTURES.iter());
    compare_fixtures("fixtures", &fixtures);
}

/// **Non-vacuity**: a file written with a planted writer — the body of a
/// different world under this key's name — is refused by the checksum and
/// the key, never served; a file damaged on disk is a miss, and the analysis
/// over it answers exactly as the built one (it builds).
/// **M152's leg**: the programs with DEPENDENCIES in every state — the
/// std-and-dependencies prefix built, written (the file's tier list naming
/// both tiers, under `dependencies.world`) and loaded — answer alike, and as
/// the clean canonical analysis does, emitted JavaScript included (the
/// dependency tier walks before the package in both shapes, so its
/// declarations are numbered and emitted alike).
#[test]
fn m152_programs_with_dependencies_answer_the_same_over_a_loaded_prefix() {
    compare_fixtures("m152", DEPENDENCY_FIXTURES);
}

#[test]
fn a_damaged_or_foreign_file_is_a_miss_and_the_analysis_builds() {
    let cache = cache_directory("damaged");
    let fixture = &LEAF_FIXTURE;
    let (_, files) = fixture.states().into_iter().next().expect("a base state");
    let package = write_package("prefix_damaged", fixture.platform, &files, &[]);
    let built = observe_fresh(&package, None, Leg::Incremental);
    assert!(
        built.census.prefix_world,
        "the leaf package takes the prefix"
    );
    let written = observe_fresh(&package, Some(cache.clone()), Leg::Incremental);
    assert_eq!(written.census.prefix_file, Some("written"));
    let tier: Vec<PathBuf> = walk(&cache)
        .into_iter()
        .filter(|path| path.file_name().is_some_and(|name| name == "std.world"))
        .collect();
    assert_eq!(tier.len(), 1, "one tier file: {tier:?}");
    let tier = &tier[0];
    let whole = std::fs::read(tier).expect("read the tier");
    assert!(
        whole.len() > 100_000,
        "a std prefix is not tiny: {} bytes",
        whole.len()
    );
    for (damage, expected) in [
        ("truncated", "truncated"),
        ("corrupt", "corrupt"),
        ("garbage", "corrupt"),
    ] {
        let bytes: Vec<u8> = match damage {
            "truncated" => whole[..whole.len() / 2].to_vec(),
            "corrupt" => {
                let mut flipped = whole.clone();
                let middle = flipped.len() / 2;
                flipped[middle] ^= 0x55;
                flipped
            }
            _ => b"vilan-std-prefix\0not really".to_vec(),
        };
        std::fs::write(tier, &bytes).expect("damage the tier");
        let over_damage = observe_fresh(&package, Some(cache.clone()), Leg::Incremental);
        // The damaged file was a miss, the world was built, and the file was
        // written again — the census reports the write, the counters the miss.
        assert_eq!(
            over_damage.census.prefix_file,
            Some("written"),
            "a {damage} file must be rebuilt and rewritten"
        );
        assert_eq!(
            over_damage.rendering, built.rendering,
            "over a {damage} file"
        );
        assert_eq!(
            over_damage.javascript, built.javascript,
            "over a {damage} file"
        );
        let reloaded = observe_fresh(&package, Some(cache.clone()), Leg::Incremental);
        assert_eq!(
            reloaded.census.prefix_file,
            Some("loaded"),
            "the rewritten {expected} file loads"
        );
    }
    // A foreign key under this name: the file of another std closure, copied
    // over. The key in its header disagrees, so it is a miss by key.
    let other_files: Vec<(String, String)> = vec![(
        "main.vl".to_string(),
        "import std::time::now;\n\nfun main() {\n\tlet _ = now();\n}\n".to_string(),
    )];
    let other_package = write_package("prefix_damaged_other", fixture.platform, &other_files, &[]);
    let other = observe_fresh(&other_package, Some(cache.clone()), Leg::Incremental);
    other_package.remove();
    assert_eq!(other.census.prefix_file, Some("written"));
    let other_tier: Vec<PathBuf> = walk(&cache)
        .into_iter()
        .filter(|path| path.file_name().is_some_and(|name| name == "std.world") && path != tier)
        .collect();
    assert_eq!(
        other_tier.len(),
        1,
        "the other closure's tier: {other_tier:?}"
    );
    std::fs::copy(&other_tier[0], tier).expect("plant the foreign file");
    let over_foreign = observe_fresh(&package, Some(cache.clone()), Leg::Incremental);
    assert_eq!(
        over_foreign.census.prefix_file,
        Some("written"),
        "a foreign key is a miss"
    );
    assert_eq!(over_foreign.rendering, built.rendering);
    package.remove();
    let _ = std::fs::remove_dir_all(&cache);
}

/// **M158**: the prefix key's checks record (`std.checks`, beside the world)
/// damaged, truncated or another closure's is a miss of the RECORD alone:
/// the world still loads, std's checks are derived as they were before the
/// record existed, and the analysis answers exactly as the built one — and,
/// having filed the record, writes it again, so the next process loads it.
#[test]
fn m158_a_damaged_or_foreign_checks_record_is_a_miss_of_the_record_alone() {
    let cache = cache_directory("checks");
    let fixture = &LEAF_FIXTURE;
    let (_, files) = fixture.states().into_iter().next().expect("a base state");
    let package = write_package("prefix_checks", fixture.platform, &files, &[]);
    let built = observe_fresh(&package, None, Leg::Incremental);
    assert!(
        built.census.prefix_world,
        "the leaf package takes the prefix"
    );
    let written = observe_fresh(&package, Some(cache.clone()), Leg::Incremental);
    assert_eq!(written.census.prefix_file, Some("written"));
    assert!(written.census.prefix_checks_written);
    let loaded = observe_fresh(&package, Some(cache.clone()), Leg::Incremental);
    assert_eq!(loaded.census.prefix_checks, Some("loaded"));
    assert!(
        loaded.census.records_replayed > 0 && written.census.records_replayed == 0,
        "the loaded record replays std's checks (replayed {} over the record, {} building it)",
        loaded.census.records_replayed,
        written.census.records_replayed
    );
    assert_eq!(loaded.rendering, built.rendering);
    assert_eq!(loaded.javascript, built.javascript);
    let checks: Vec<PathBuf> = walk(&cache)
        .into_iter()
        .filter(|path| path.file_name().is_some_and(|name| name == "std.checks"))
        .collect();
    assert_eq!(checks.len(), 1, "one checks record: {checks:?}");
    let checks = &checks[0];
    let whole = std::fs::read(checks).expect("read the record");
    let other_files: Vec<(String, String)> = vec![(
        "main.vl".to_string(),
        "import std::time::now;\n\nfun main() {\n\tlet _ = now();\n}\n".to_string(),
    )];
    let other_package = write_package("prefix_checks_other", fixture.platform, &other_files, &[]);
    let other = observe_fresh(&other_package, Some(cache.clone()), Leg::Incremental);
    other_package.remove();
    assert!(other.census.prefix_checks_written);
    let foreign: Vec<PathBuf> = walk(&cache)
        .into_iter()
        .filter(|path| path.file_name().is_some_and(|name| name == "std.checks") && path != checks)
        .collect();
    assert_eq!(foreign.len(), 1, "the other closure's record: {foreign:?}");
    let foreign = std::fs::read(&foreign[0]).expect("read the other record");
    for (damage, bytes, expected) in [
        ("truncated", whole[..whole.len() / 2].to_vec(), "truncated"),
        (
            "corrupt",
            {
                let mut flipped = whole.clone();
                let middle = flipped.len() / 2;
                flipped[middle] ^= 0x55;
                flipped
            },
            "corrupt",
        ),
        ("foreign", foreign, "foreign-key"),
    ] {
        std::fs::write(checks, &bytes).expect("damage the record");
        let over_damage = observe_fresh(&package, Some(cache.clone()), Leg::Incremental);
        assert_eq!(
            over_damage.census.prefix_file,
            Some("loaded"),
            "a {damage} record leaves the world's file alone"
        );
        assert_eq!(
            over_damage.census.prefix_checks,
            Some(expected),
            "a {damage} record is a miss of the record"
        );
        assert_eq!(
            over_damage.census.records_replayed, 0,
            "over a {damage} record"
        );
        assert!(
            over_damage.census.prefix_checks_written,
            "the analysis over a {damage} record writes it again"
        );
        assert_eq!(
            over_damage.rendering, built.rendering,
            "over a {damage} record"
        );
        assert_eq!(
            over_damage.javascript, built.javascript,
            "over a {damage} record"
        );
        let healed = observe_fresh(&package, Some(cache.clone()), Leg::Incremental);
        assert_eq!(
            healed.census.prefix_checks,
            Some("loaded"),
            "the rewritten record loads after a {damage} one"
        );
        assert_eq!(healed.rendering, built.rendering, "after a {damage} record");
    }
    package.remove();
    let _ = std::fs::remove_dir_all(&cache);
}

/// **incr-54c's find (P5)**: a std namespace a program's import names
/// (`std::web`, a directory with no body) is part of the prefix world —
/// `namespace_only_modules` — so it is part of the prefix's KEY. Before the
/// fix the closure skipped it, so a prefix built for a program that never
/// named `std::web` was served to one that does, which then resolved
/// `std::web::dmo` as the moved web prelude ("`std::web` moved to
/// `std::web::prelude`") instead of a miss under the namespace — over the
/// persisted file and over the in-process prefix alike.
#[test]
fn a_std_namespace_the_import_names_is_part_of_the_prefix_key() {
    let cache = cache_directory("namespace");
    let empty: Vec<(String, String)> = vec![("main.vl".to_string(), "fun main() {}\n".to_string())];
    let dmo: Vec<(String, String)> = vec![(
        "main.vl".to_string(),
        "import std::web::dmo;\n\nfun main() {}\n".to_string(),
    )];
    let first = write_package("prefix_namespace_empty", Platform::default(), &empty, &[]);
    let second = write_package("prefix_namespace_dmo", Platform::default(), &dmo, &[]);
    let clean = observe_fresh(&second, None, Leg::Clean);
    assert!(
        clean
            .rendering
            .contains("cannot find 'dmo' in the imported path"),
        "the canonical analysis misses `dmo` under the namespace: {}",
        clean.rendering
    );
    // Over the persisted file the empty program wrote.
    let written = observe_fresh(&first, Some(cache.clone()), Leg::Incremental);
    assert_eq!(written.census.prefix_file, Some("written"));
    let over_file = observe_fresh(&second, Some(cache.clone()), Leg::Incremental);
    assert_eq!(
        over_file.rendering, clean.rendering,
        "over the empty program's file"
    );
    // Over the in-process prefix the empty program stored.
    vilan_core::analyzer::base_cache_clear();
    vilan_core::analyzer::prefix_files_forget();
    set_world_cache(Some(None));
    set_std_prefix(Some(true));
    let _ = observe(&first, Vec::new(), Leg::Incremental);
    let in_process = observe(&second, Vec::new(), Leg::Incremental);
    set_std_prefix(None);
    set_world_cache(None);
    assert!(
        in_process.census.prefix_world,
        "the program builds over the prefix"
    );
    assert_eq!(
        in_process.rendering, clean.rendering,
        "over the empty program's in-process prefix"
    );
    first.remove();
    second.remove();
    let _ = std::fs::remove_dir_all(&cache);
}

fn walk(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(directory) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(walk(&path));
        } else {
            found.push(path);
        }
    }
    found
}
