//! M36's spike: what a CROSS-PROCESS base cache would have to write, and what
//! it could ever buy.
//!
//! `base_cache.rs`'s M36 measurement answers half the question — with the
//! parse cache already warm, the base world is 190 ms of a 370–400 ms cold
//! analysis. But a second PROCESS starts with everything cold, and the two
//! candidate on-disk units remove different halves of that: serializing the
//! ANALYZED world removes the parse and the resolve both; serializing the
//! parsed ASTs and re-resolving removes only the parse. Which half is which
//! is the number that decides the unit, and nothing in the tree measured it.
//!
//! Everything here is a MEASUREMENT rather than a budget, so everything here
//! is `#[ignore]`d and prints. The findings it produced are written up in
//! `proposal/analysis-reuse.md` §6.15.

use std::path::{Path, PathBuf};

use vilan_core::{PackageSpec, Platform, Workspace, analyze_source};

fn std_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std")
}

/// The same wide-`std` closure `base_cache.rs`'s M36 measurement uses, so the
/// two are comparable line for line.
const WIDE_A: &str = "import std::io::print;\nimport std::list::List;\n\
                      import std::map::Map;\nimport std::set::Set;\n\
                      import std::json;\nimport std::math::PI;\n\
                      fun main() { print(PI); }\n";
const WIDE_B: &str = "import std::io::print;\nimport std::list::List;\n\
                      import std::map::Map;\nimport std::set::Set;\n\
                      import std::json;\nimport std::math::PI;\n\
                      fun main() { print(PI + 1.0); }\n";

fn process_cpu_ms() -> Option<f64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let rest = stat.rsplit_once(')')?.1;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some((utime + stime) as f64 * 10.0)
}

fn loadavg_1m() -> String {
    std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|text| text.split_whitespace().next().map(str::to_string))
        .unwrap_or_else(|| "?".to_string())
}

fn on_one_thread<T: Send + 'static>(body: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(body)
        .expect("spawn the spike worker")
        .join()
        .expect("the spike worker panicked")
}

fn analyze_on_this_thread(spec: &PackageSpec, source: &'static str) {
    let (program, _errors) = analyze_source(
        source,
        spec,
        Path::new("."),
        Path::new("world_cache_spike.vl"),
        Some(Platform::default()),
        &Workspace::default(),
    );
    drop(program);
}

/// M36: **the true per-process floor, split into the part each candidate
/// on-disk unit would remove.**
///
/// Three analyses in one fresh process, in order:
///
/// 1. `first` — everything cold: the std sources are read, parsed and leaked,
///    the world is resolved, the entry walks and checks. This is what every
///    corpus-program process pays today (N52).
/// 2. `reparse_free` — the base cache cleared but the PARSE cache still warm:
///    the world is resolved again from already-parsed trees. This is
///    `base_cache.rs`'s "cold".
/// 3. `hit` — the same import set, a base-cache hit: the world is cloned and
///    only the entry walks. This is `base_cache.rs`'s "warm", and it already
///    includes re-reading and hashing every loaded std source, which is the
///    E12 revalidation an on-disk hit would also pay.
///
/// `first - reparse_free` is therefore the PARSE share, and `reparse_free -
/// hit` is the RESOLVE share. An on-disk cache of the ANALYZED world removes
/// both; an on-disk cache of the parsed ASTs removes only the first.
#[test]
#[ignore = "M36: a MEASUREMENT of what each candidate on-disk unit would remove, not a budget; run deliberately"]
fn the_per_process_floor_splits_into_a_parse_share_and_a_resolve_share() {
    split("wide", WIDE_A, WIDE_B);
}

/// The same split for a NARROW closure — one import, the shape most of the
/// suite's 5,089 test processes actually analyze. The wide figure above is
/// the ceiling of what a cross-process cache is worth per process; this is
/// the floor of it, and the truth for any particular test is between them.
#[test]
#[ignore = "M36: a MEASUREMENT of the narrow-closure end of the same split; run deliberately"]
fn the_narrow_closure_pays_a_smaller_floor_than_the_wide_one() {
    split("narrow", NARROW_A, NARROW_B);
}

const NARROW_A: &str = "import std::io::print;\nfun main() { print(1); }\n";
const NARROW_B: &str = "import std::io::print;\nfun main() { print(2 + 3); }\n";

fn split(label: &'static str, a: &'static str, b: &'static str) {
    let spec = vilan_core::manifest::resolve_std(&std_root());
    let load = loadavg_1m();

    let (first, reparse_free, hit, texts, census) = on_one_thread(move || {
        vilan_core::analyzer::base_cache_clear();
        let before = process_cpu_ms();
        analyze_on_this_thread(&spec, a);
        let first = before.zip(process_cpu_ms()).map(|(x, y)| y - x);

        vilan_core::analyzer::base_cache_clear();
        let before = process_cpu_ms();
        analyze_on_this_thread(&spec, a);
        let reparse_free = before.zip(process_cpu_ms()).map(|(x, y)| y - x);

        let before = process_cpu_ms();
        analyze_on_this_thread(&spec, b);
        let hit = before.zip(process_cpu_ms()).map(|(x, y)| y - x);

        let (texts, census) = vilan_core::analyzer::base_cache_retained_split();
        (first, reparse_free, hit, texts, census)
    });

    let (Some(first), Some(reparse_free), Some(hit)) = (first, reparse_free, hit) else {
        panic!(
            "no process CPU clock on this host (no /proc/self/stat), so this \
             measurement would be a wall number wearing a CPU label (M15)"
        );
    };
    println!(
        "M36-SPLIT closure={label} profile={} first={first:.0} ms reparse_free={reparse_free:.0} ms \
         hit={hit:.0} ms parse_share={:.0} ms resolve_share={:.0} ms \
         world_texts={texts} B type_census={census} B load={load}",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        first - reparse_free,
        reparse_free - hit,
    );
    assert!(first > 0.0, "the first analysis must cost something");
}

/// The resident bytes ONE retained world costs — the size any on-disk unit
/// would have to write and read back.
///
/// `base_cache_retained_bytes` is deliberately source-proportional (M11): it
/// counts the module texts a world was built from plus T0's per-`TypeId`
/// census, because that is a figure an eviction can recompute without a heap
/// audit. It is not what the world WEIGHS — the ~229 analyzer tables the
/// resolve fills are derived state it does not try to count, and those tables
/// are exactly what a serializer would have to emit.
///
/// So this measures the weight instead, by minting distinct keys for one
/// closure: `macro_limits` is part of `BaseCacheKey`, and a workspace that
/// differs only in its macro fuel resolves the same modules into a world
/// stored under a different key. Analyze the same wide-`std` program under N
/// such workspaces, with the parse cache and the interner already warm, and
/// the resident growth is N worlds and almost nothing else.
#[test]
#[ignore = "M36: a MEASUREMENT of what one retained world weighs; RSS-based, so it is indicative rather than a budget — run deliberately"]
fn one_retained_world_weighs_far_more_than_the_texts_it_was_built_from() {
    let load = loadavg_1m();
    const WORLDS: u64 = 8;

    let (growth, control, recorded, count) = on_one_thread(move || {
        let spec = vilan_core::manifest::resolve_std(&std_root());
        // Warm everything that is per-process but not per-world: the parse
        // cache's leaked texts and trees, the interner, the allocator's
        // arenas. Whatever these cost, they cost it once and are not what is
        // being weighed.
        vilan_core::analyzer::base_cache_clear();
        analyze_on_this_thread(&spec, WIDE_A);
        vilan_core::analyzer::base_cache_clear();

        // The CONTROL: the same N analyses of the same program under ONE key,
        // so exactly one world is stored and the other N-1 are hits. Its
        // resident growth is the per-analysis garbage the allocator has not
        // handed back — everything the measurement below must not credit to
        // the worlds.
        let control_before = resident_bytes();
        for _ in 0..WORLDS {
            analyze_on_this_thread(&spec, WIDE_A);
        }
        let control_after = resident_bytes();
        let control = control_after
            .zip(control_before)
            .map(|(after, before)| after.saturating_sub(before));
        vilan_core::analyzer::base_cache_clear();

        let before = resident_bytes();
        for fuel in 0..WORLDS {
            let mut workspace = Workspace::default();
            // Same sources, same closure, a key of its own.
            workspace.macro_limits.fuel = 1_000_000 + fuel;
            let (program, _errors) = analyze_source(
                WIDE_A,
                &spec,
                Path::new("."),
                Path::new("world_cache_spike.vl"),
                Some(Platform::default()),
                &workspace,
            );
            drop(program);
        }
        let after = resident_bytes();
        let count = vilan_core::analyzer::base_cache_retained();
        let recorded = vilan_core::analyzer::base_cache_retained_bytes();
        let growth = after
            .zip(before)
            .map(|(after, before)| after.saturating_sub(before));
        vilan_core::analyzer::base_cache_clear();
        (growth, control, recorded, count)
    });

    let (Some(growth), Some(control)) = (growth, control) else {
        panic!("no /proc/self/statm on this host, so there is no resident-size instrument");
    };
    // The control retained ONE world and the measurement `WORLDS`, so their
    // difference is `WORLDS - 1` worlds; both paid the same per-analysis
    // transient. Both raw figures are printed, because the derived one is a
    // difference of two blunt readings and a reader should be able to see
    // what it was derived from.
    let per_world = growth.saturating_sub(control) / (WORLDS as usize - 1);
    println!(
        "M36-WEIGHT profile={} worlds={count} rss_growth={growth} B \
         rss_control={control} B rss_per_world={per_world} B \
         recorded_total={recorded} B recorded_per_world={} B load={load}",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        recorded / count.max(1),
    );
    assert_eq!(
        count, WORLDS as usize,
        "each distinct macro-fuel key must retain a world of its own, or this \
         is weighing something other than {WORLDS} worlds"
    );
}

/// Resident set size in bytes, from `/proc/self/statm` field 2 (pages).
fn resident_bytes() -> Option<usize> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: usize = statm.split_whitespace().nth(1)?.parse().ok()?;
    Some(pages * 4096)
}

/// M50: **the world's weight, pinned against the counter within the stated
/// factor.**
///
/// The sibling measurement above found that a retained world weighs far more
/// than `base_cache_retained_bytes` records, and left the number indicative.
/// M24's budget was denominated in that counter, so "512 MiB retained" was a
/// bound on the order of 12 GB resident — the cache was bounded, and not where
/// M24 thought. `BASE_CACHE_WEIGHT_FACTOR` is the ratio, measured, and
/// `BASE_CACHE_DEFAULT_BUDGET` is the resident bound divided by it.
///
/// A constant taken from one measurement rots silently, so this is a pin and
/// not a comment: the same eight-key weighing, asserting that the factor the
/// harness measures is within **3×** of the one the compiler ships. The band is
/// wide on purpose — RSS is a blunt instrument and this asserts a DENOMINATION,
/// not a byte count — and it still reds by orders of magnitude on the change
/// that matters, which is the counter quietly starting (or stopping) to count
/// the derived tables that make up the gap.
///
/// It DECLINES rather than fails where the instrument is not available: no
/// `/proc/self/statm`, or a control that grew at least as much as the
/// measurement, means there is no reading here to assert on.
#[test]
fn one_retained_world_weighs_what_the_shipped_factor_says_it_does() {
    let load = loadavg_1m();
    const WORLDS: u64 = 8;
    /// How far the measured factor may sit from the shipped one, either way.
    const SLACK: f64 = 3.0;

    let (growth, control, recorded, count) = on_one_thread(move || {
        let spec = vilan_core::manifest::resolve_std(&std_root());
        // Everything per-process but not per-world, warmed first: the parse
        // cache's leaked texts and trees, the interner, the allocator's arenas.
        // A per-world figure must not carry them — and the fact that they are
        // NOT the gap is half of M50's finding.
        vilan_core::analyzer::base_cache_clear();
        analyze_on_this_thread(&spec, WIDE_A);
        vilan_core::analyzer::base_cache_clear();

        let control_before = resident_bytes();
        for _ in 0..WORLDS {
            analyze_on_this_thread(&spec, WIDE_A);
        }
        let control = resident_bytes()
            .zip(control_before)
            .map(|(after, before)| after.saturating_sub(before));
        vilan_core::analyzer::base_cache_clear();

        let before = resident_bytes();
        for fuel in 0..WORLDS {
            let mut workspace = Workspace::default();
            workspace.macro_limits.fuel = 1_000_000 + fuel;
            let (program, _errors) = analyze_source(
                WIDE_A,
                &spec,
                Path::new("."),
                Path::new("world_weight_pin.vl"),
                Some(Platform::default()),
                &workspace,
            );
            drop(program);
        }
        let growth = resident_bytes()
            .zip(before)
            .map(|(after, before)| after.saturating_sub(before));
        let count = vilan_core::analyzer::base_cache_retained();
        let recorded = vilan_core::analyzer::base_cache_retained_bytes();
        let weight = vilan_core::analyzer::base_cache_retained_weight();
        assert_eq!(
            weight,
            recorded * vilan_core::analyzer::BASE_CACHE_WEIGHT_FACTOR,
            "the reported weight is the recorded figure re-denominated, and \
             nothing else"
        );
        vilan_core::analyzer::base_cache_clear();
        (growth, control, recorded, count)
    });

    assert_eq!(
        count, WORLDS as usize,
        "each distinct macro-fuel key must retain a world of its own, or this \
         is weighing something other than {WORLDS} worlds"
    );

    let (Some(growth), Some(control)) = (growth, control) else {
        println!("M50-WEIGHT DECLINED: no /proc/self/statm on this host");
        return;
    };
    if growth <= control {
        println!(
            "M50-WEIGHT DECLINED: rss_growth={growth} B did not exceed \
             rss_control={control} B, so there is no per-world reading (load={load})"
        );
        return;
    }
    let per_world = (growth - control) / (WORLDS as usize - 1);
    let recorded_per_world = recorded / count.max(1);
    let measured = per_world as f64 / recorded_per_world.max(1) as f64;
    let shipped = vilan_core::analyzer::BASE_CACHE_WEIGHT_FACTOR as f64;
    println!(
        "M50-WEIGHT rss_per_world={per_world} B recorded_per_world={recorded_per_world} B \
         measured_factor={measured:.1} shipped_factor={shipped:.0} load={load}"
    );
    assert!(
        measured >= shipped / SLACK && measured <= shipped * SLACK,
        "a retained world weighs {measured:.1}x what the counter records, and \
         `BASE_CACHE_WEIGHT_FACTOR` says {shipped:.0}x — the budget is \
         denominated on that factor, so re-measure it (the harness above prints \
         the raw figures) rather than widening this band"
    );
}
