//! The compiler-performance baseline harness (`proposal/perf-baseline.md`).
//!
//! Two sections, both measured with `std::time::Instant` and nothing else — no
//! benchmark framework, per the house's minimal-dependency taste:
//!
//! 1. **Phases.** The four pipeline stages called as the library entry points
//!    they already are — `parsing::parse` (+ the `elements` / `lift` desugars),
//!    `analyzer::analyze`, `post_analysis_passes`, `transformer::transform` —
//!    the same seam `VILAN_PHASE_TIMING` marks. Each corpus is measured **cold**
//!    and **warm**, and the difference is *forced*, never assumed: a cold
//!    iteration clears the process-global caches first
//!    (`analyzer::base_cache_clear`, `macro_world_cache_clear`, and — since
//!    backlog M6, 2026-08-19 — `parse_clean_cache_clear`). Leaving that to
//!    chance is the exact drift `suite-speed.md` §2.1/E26 recorded — a number
//!    attributed to a mechanism that the accounting never confirmed.
//! 2. **End to end.** `vilan check` on real packages, spawned exactly as the
//!    suite spawns the binary, so every measurement carries process startup and
//!    a genuinely cold process. Reported in units of a **freshly measured
//!    reference compile** as well as in milliseconds — the convention
//!    `tests/support/mod.rs`'s `reference_compile()` / `run_liveness()`
//!    established, because absolute seconds are a claim about the machine and a
//!    ratio is a claim about the compiler.
//!
//! The LSP edit-latency section lives in `vilan-lsp` (`document.rs`'s
//! `perf_baseline` module) — `Document::analyze_on_this_thread` is private to
//! that crate, and measuring it from here would mean widening an API for a
//! benchmark's convenience.
//!
//! Run both sections (`perf-baseline.md` §3 has the full recipe):
//!
//! ```text
//! cargo nextest run --release --workspace --run-ignored ignored-only \
//!     -E 'test(perf_baseline)' --no-capture > perf.log 2>&1
//! grep '^PERF ' perf.log
//! ```
//!
//! One thing in this file is not a measurement but a *pin*, and runs in the
//! normal gate: `the_const_pass_scales_with_its_const_sites_and_not_with_their_square`
//! (backlog M4, `const-eval.md` §10.4). It asserts a ratio between two
//! measurements taken in one process, never a number of seconds — see its own
//! comment, and `perf-baseline.md` §1.4 for why that is admissible where a
//! relative-regression check is not.
//!
//! `--release` because a debug measurement is a fact about `-O0` — every row
//! stamps its `profile` so the two can never be compared by accident.
//! `--no-capture` streams the rows and makes nextest run the two tests
//! serially; a benchmark measured beside another benchmark measures the
//! scheduler.
//!
//! Every measurement prints one `PERF {…}` JSON line, so two runs diff as text
//! and a run's rows concatenate across the two binaries. The external corpora
//! are addressed by environment variable and **skipped, not failed**, when
//! absent (they live in sibling repositories that a fresh clone does not have):
//! `VILAN_PERF_KOLT`, `VILAN_PERF_WEBSITE`, `VILAN_PERF_TODO`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use vilan_core::{
    BuildOptions, PackageSpec, Platform, Workspace, analyze, elements, lift, parsing,
    post_analysis_passes, transform,
};

// ---------------------------------------------------------------------------
// The summary
// ---------------------------------------------------------------------------

/// One measured row of the machine-readable summary. Written by hand rather
/// than through a serializer: the crate has no `serde` edge and a benchmark is
/// not a reason to open one.
struct Row {
    section: &'static str,
    corpus: String,
    mode: &'static str,
    metric: &'static str,
    /// Which build produced the row. A debug-profile number is a fact about
    /// `-O0`, not about the compiler a user installs, and a baseline that does
    /// not say which it is invites exactly that confusion — so the harness
    /// stamps it rather than trusting the reader to remember.
    profile: &'static str,
    runs: usize,
    min_ms: f64,
    median_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    max_ms: f64,
    note: String,
}

impl Row {
    fn json(&self) -> String {
        format!(
            "{{\"section\":\"{}\",\"corpus\":\"{}\",\"mode\":\"{}\",\"metric\":\"{}\",\
             \"profile\":\"{}\",\"runs\":{},\"min_ms\":{:.2},\"median_ms\":{:.2},\
             \"p95_ms\":{:.2},\"p99_ms\":{:.2},\"max_ms\":{:.2},\"note\":\"{}\"}}",
            self.section,
            self.corpus,
            self.mode,
            self.metric,
            self.profile,
            self.runs,
            self.min_ms,
            self.median_ms,
            self.p95_ms,
            self.p99_ms,
            self.max_ms,
            self.note,
        )
    }
}

/// Nearest-rank percentile over already-sorted samples. `percentile` is a
/// fraction (0.95 for p95); an empty slice is a caller error, not a zero.
fn percentile(sorted: &[Duration], percentile: f64) -> Duration {
    assert!(!sorted.is_empty(), "no samples to take a percentile of");
    let rank = (percentile * sorted.len() as f64).ceil().max(1.0) as usize;
    sorted[rank.min(sorted.len()) - 1]
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// Reduces raw samples to a [`Row`]. Min and median are the headline pair — the
/// min is the machine's best case (the least contended sample) and the median
/// is what a caller actually waits for; a mean would let one scheduler hiccup
/// speak for the run.
fn summarize(
    section: &'static str,
    corpus: &str,
    mode: &'static str,
    metric: &'static str,
    note: &str,
    samples: &[Duration],
) -> Row {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    Row {
        section,
        corpus: corpus.to_string(),
        mode,
        metric,
        profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        runs: sorted.len(),
        min_ms: milliseconds(sorted[0]),
        median_ms: milliseconds(percentile(&sorted, 0.50)),
        p95_ms: milliseconds(percentile(&sorted, 0.95)),
        p99_ms: milliseconds(percentile(&sorted, 0.99)),
        max_ms: milliseconds(*sorted.last().unwrap()),
        note: note.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Section 1 — the four phases, cold and warm
// ---------------------------------------------------------------------------

/// A phase-benchmark subject: an entry source plus the roots it analyzes
/// against. Synthetic subjects carry no files on disk (nothing in the pipeline
/// reads the entry path unless the source imports `pkg::`), which is what keeps
/// the in-repo half of this harness independent of any sibling checkout.
struct PhaseSubject {
    name: String,
    source: String,
    pkg_root: PathBuf,
    entry: PathBuf,
    lines: usize,
}

impl PhaseSubject {
    fn synthetic(name: &str, source: &str) -> PhaseSubject {
        PhaseSubject {
            name: name.to_string(),
            lines: source.lines().count(),
            source: source.to_string(),
            pkg_root: PathBuf::from("."),
            entry: PathBuf::from("perf_entry.vl"),
        }
    }

    /// A real package entry, read from disk. `pkg_root` is the package's source
    /// root — `<package>/src` — which is what `pkg::` resolves against.
    fn package_entry(name: &str, entry: &Path) -> Option<PhaseSubject> {
        let source = std::fs::read_to_string(entry).ok()?;
        Some(PhaseSubject {
            name: name.to_string(),
            lines: source.lines().count(),
            source,
            pkg_root: entry.parent()?.to_path_buf(),
            entry: entry.to_path_buf(),
        })
    }
}

/// The four phase timings of one compile, plus what it produced.
struct PhaseSample {
    parse: Duration,
    analyze: Duration,
    post: Duration,
    transform: Duration,
    total: Duration,
    diagnostics: usize,
    emitted_bytes: usize,
}

/// Compiles `subject` once through the four library entry points, timing each.
///
/// `cold` decides what the compile is allowed to inherit from the ones before
/// it, and that is the whole point of the switch: the resolved pre-entry world
/// (`BASE_CACHE`) and the expanded macro worlds are process-global, so a second
/// compile of the same subject in the same process is a *different measurement*
/// from the first. Clearing them yields the shape a fresh `vilan build` pays;
/// leaving them yields the shape a language-server keystroke or a `--watch`
/// round pays.
///
/// Since backlog M6 (2026-08-19) the third process-global cache —
/// `parse_clean_cached`'s content-keyed store of module texts and ASTs — is
/// cleared too, so "cold" here means *world-cold AND parse-cold*: every cold
/// iteration re-lexes and re-parses every module it loads, the true
/// first-compile shape. (It used to mean world-cold, parse-warm, because that
/// cache had no clearer — cold rows measured before this date are NOT
/// comparable to cold rows measured after it; `perf-baseline.md` §6 records
/// the two side by side.) What still separates this from the end-to-end
/// section's cold is the process itself: startup and binary load are paid
/// only there.
fn measure_phases(
    subject: &PhaseSubject,
    std: &PackageSpec,
    platform: Platform,
    cold: bool,
) -> PhaseSample {
    if cold {
        vilan_core::analyzer::base_cache_clear();
        vilan_core::macro_world_cache_clear();
        vilan_core::parse_clean_cache_clear();
    }

    // The pipeline borrows its source for `'static`, exactly as every front-end
    // does; a benchmark leaks one copy per iteration like the language server
    // leaks one per keystroke.
    let leaked: &'static str = Box::leak(subject.source.clone().into_boxed_str());
    let options = BuildOptions::default();
    let workspace = Workspace::default();

    let started = Instant::now();
    let parse_started = Instant::now();
    let (tree, _parse_errors) = parsing::parse(leaked);
    let mut root = tree.expect("the frontend always returns a tree");
    elements::rewrite_items(&mut root.0, leaked);
    lift::rewrite_items(&mut root.0);
    let parse = parse_started.elapsed();

    let root: &'static _ = Box::leak(Box::new(root));
    let analyze_started = Instant::now();
    let mut program = analyze(
        root,
        leaked,
        std,
        &subject.pkg_root,
        &subject.entry,
        platform,
        &workspace,
    );
    let analyze_time = analyze_started.elapsed();

    let post_started = Instant::now();
    post_analysis_passes(&mut program, platform, &options);
    let post = post_started.elapsed();

    let transform_started = Instant::now();
    let emitted = transform(&program, &options);
    let transform_time = transform_started.elapsed();

    PhaseSample {
        parse,
        analyze: analyze_time,
        post,
        transform: transform_time,
        total: started.elapsed(),
        diagnostics: program.diagnostics.len(),
        emitted_bytes: emitted.map(|javascript| javascript.len()).unwrap_or(0),
    }
}

/// Measures one subject in one mode `runs` times and prints a row per phase
/// plus a row for the whole compile.
fn phase_rows(
    subject: &PhaseSubject,
    std: &PackageSpec,
    platform: Platform,
    cold: bool,
    runs: usize,
) -> Vec<Row> {
    let mode = if cold { "cold" } else { "warm" };
    // A warm mode has to be warm before the clock starts: the first compile of
    // a subject in a process is cold whatever the flag says.
    if !cold {
        let _ = measure_phases(subject, std, platform, false);
    }

    let mut samples: Vec<PhaseSample> = Vec::with_capacity(runs);
    for _ in 0..runs {
        samples.push(measure_phases(subject, std, platform, cold));
    }
    let last = samples.last().expect("at least one run");
    let note = format!(
        "{} lines, {} diagnostics, {} B emitted",
        subject.lines, last.diagnostics, last.emitted_bytes
    );

    let pick = |select: fn(&PhaseSample) -> Duration| -> Vec<Duration> {
        samples.iter().map(select).collect()
    };
    vec![
        summarize(
            "phase",
            &subject.name,
            mode,
            "parse",
            &note,
            &pick(|sample| sample.parse),
        ),
        summarize(
            "phase",
            &subject.name,
            mode,
            "analyze",
            &note,
            &pick(|sample| sample.analyze),
        ),
        summarize(
            "phase",
            &subject.name,
            mode,
            "post_passes",
            &note,
            &pick(|sample| sample.post),
        ),
        summarize(
            "phase",
            &subject.name,
            mode,
            "transform",
            &note,
            &pick(|sample| sample.transform),
        ),
        summarize(
            "phase",
            &subject.name,
            mode,
            "total",
            &note,
            &pick(|sample| sample.total),
        ),
    ]
}

/// The tiny subject: `std::print` and nothing else. Not representative of
/// anything a user compiles — it is the *unit*, the same role
/// `support::reference_compile`'s project plays for the suite's liveness
/// bounds, and it is here so every other number can be read as a multiple of
/// the smallest compile the toolchain can do.
const TINY_SOURCE: &str = "import std::print;\n\nfun main() {\n\tprint(\"perf baseline\");\n}\n";

/// The wide subject: one entry that imports broadly across `std`, which is how
/// a 57-file, 15k-line standard library becomes a *cold whole-world compile*
/// without inventing a 15k-line application to compile. std is the only corpus
/// in this repository big enough to stand for one, and reaching it through an
/// entry (rather than "compiling std") measures what a user's first build
/// measures: the module discovery, load, walk and resolve that a real program's
/// imports drag in.
const WIDE_SOURCE: &str = r#"import std::base64;
import std::bytes::Bytes;
import std::compare::Ordering;
import std::db::Database;
import std::fs;
import std::http::{ Request, Response };
import std::iterator;
import std::json::json_codec;
import std::list::List;
import std::map::Map;
import std::math;
import std::option::Option::{ None, Some, self };
import std::print;
import std::process::env;
import std::random;
import std::reactive::{ Owner, Signal, run_with_owner };
import std::result::Result::{ Err, Ok };
import std::set::Set;
import std::shared::Shared;
import std::string;
import std::time::{ Instant, now };
import std::ui::render;
import std::wire;

fun main() {
	print("perf baseline: wide");
}
"#;

// ---------------------------------------------------------------------------
// Section 2 — end to end, in reference units
// ---------------------------------------------------------------------------

/// A package the harness checks end to end. Read-only: `vilan check` reports
/// diagnostics and writes nothing, which is what lets the sibling corpora be
/// measured where they live instead of being copied.
struct Package {
    name: &'static str,
    /// The environment variable naming it, so a machine without the sibling
    /// checkout skips the row instead of failing the run.
    variable: &'static str,
}

const PACKAGES: &[Package] = &[
    Package {
        name: "todo",
        variable: "VILAN_PERF_TODO",
    },
    Package {
        name: "kolt",
        variable: "VILAN_PERF_KOLT",
    },
    Package {
        name: "website",
        variable: "VILAN_PERF_WEBSITE",
    },
];

/// Spawns `vilan check <directory>` once, returning its wall time and whether
/// it succeeded. The wall includes process startup and binary load on purpose —
/// they are part of what a user waits for, and (per `support/mod.rs`) they are
/// the costs that move most under a loaded machine.
fn check_once(directory: &Path) -> (Duration, bool) {
    let started = Instant::now();
    let status = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["check", directory.to_str().expect("utf-8 corpus path")])
        .output();
    let elapsed = started.elapsed();
    match status {
        Ok(output) => (elapsed, output.status.success()),
        Err(_) => (elapsed, false),
    }
}

/// Writes the reference project — `std::print` and nothing else, the same
/// program `support::reference_compile` builds — into a fresh temporary
/// directory. The unit every end-to-end row is also reported in.
fn write_reference_project() -> Option<PathBuf> {
    let project = std::env::temp_dir().join(format!("vilan_perf_reference_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&project);
    std::fs::create_dir_all(project.join("src")).ok()?;
    std::fs::write(
        project.join("vilan.toml"),
        "[package]\nname = \"reference\"\ntarget = \"node\"\n",
    )
    .ok()?;
    std::fs::write(project.join("src/main.vl"), TINY_SOURCE).ok()?;
    Some(project)
}

// ---------------------------------------------------------------------------
// The run
// ---------------------------------------------------------------------------

/// How much measurement to do. The smoke scale exists so the PR gate can pin
/// that the harness *works* — every code path taken, every row well formed —
/// in seconds rather than minutes.
#[derive(Clone, Copy, PartialEq)]
enum Scale {
    Smoke,
    Full,
}

impl Scale {
    fn phase_runs(self) -> (usize, usize) {
        match self {
            // (cold, warm). Cold runs are the expensive ones and vary least;
            // warm runs are cheap, so more of them.
            Scale::Smoke => (1, 2),
            Scale::Full => (5, 15),
        }
    }

    fn end_to_end_runs(self) -> usize {
        match self {
            Scale::Smoke => 1,
            Scale::Full => 5,
        }
    }
}

fn corpus_path(variable: &str) -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os(variable)?);
    path.join("vilan.toml").is_file().then_some(path)
}

fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    )
}

/// Section 1, on a big stack: the pipeline nests a full analysis inside every
/// macro-world compile, so the measuring thread needs the same room
/// `leak_measurement` and the CLI give theirs.
fn phase_section(scale: Scale) -> Vec<Row> {
    let (cold_runs, warm_runs) = scale.phase_runs();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let std = std_spec();
            let platform = Platform::default();
            let mut subjects = vec![PhaseSubject::synthetic("tiny", TINY_SOURCE)];
            if scale == Scale::Full {
                subjects.push(PhaseSubject::synthetic("std_wide", WIDE_SOURCE));
                for package in PACKAGES {
                    let Some(directory) = corpus_path(package.variable) else {
                        continue;
                    };
                    let entry = directory.join("src/server.vl");
                    if let Some(subject) =
                        PhaseSubject::package_entry(&format!("{}_server", package.name), &entry)
                    {
                        subjects.push(subject);
                    }
                }
            }

            // Prime, then measure. This pass was born to cancel an ordering
            // artifact that no longer exists: before `parse_clean_cache_clear`
            // (backlog M6), cold could not clear the parse cache, so the first
            // subject in the process paid every module's lex-and-parse and the
            // later ones inherited it — cold rows ranked the subjects by
            // *position*. Cold now clears all three caches per iteration, so
            // every cold sample is self-contained; the prime is kept for the
            // narrower job of absorbing the process's ONE-TIME costs (lazily
            // built tables, allocator warm-up), so the first subject's first
            // sample is not also the process's first-ever compile.
            for subject in &subjects {
                let _ = measure_phases(subject, &std, platform, true);
            }

            let mut rows = Vec::new();
            for subject in &subjects {
                rows.extend(phase_rows(subject, &std, platform, true, cold_runs));
                rows.extend(phase_rows(subject, &std, platform, false, warm_runs));
            }
            rows
        })
        .expect("spawn the phase measurement thread")
        .join()
        .expect("the phase measurement thread panicked")
}

/// Section 2. Every row also carries its cost in reference units, which is the
/// number that survives a change of machine.
fn end_to_end_section(scale: Scale) -> Vec<Row> {
    let runs = scale.end_to_end_runs();
    let Some(reference_project) = write_reference_project() else {
        return Vec::new();
    };

    let mut reference_samples = Vec::with_capacity(runs);
    let mut reference_ok = true;
    for _ in 0..runs {
        let (elapsed, ok) = check_once(&reference_project);
        reference_ok &= ok;
        reference_samples.push(elapsed);
    }
    let mut rows = vec![summarize(
        "end_to_end",
        "reference",
        "cold_process",
        "check",
        &format!("the unit; ok={reference_ok}"),
        &reference_samples,
    )];
    let reference_median = rows[0].median_ms;
    let _ = std::fs::remove_dir_all(&reference_project);

    if scale == Scale::Smoke {
        return rows;
    }

    for package in PACKAGES {
        let Some(directory) = corpus_path(package.variable) else {
            continue;
        };
        let mut samples = Vec::with_capacity(runs);
        let mut ok = true;
        for _ in 0..runs {
            let (elapsed, run_ok) = check_once(&directory);
            ok &= run_ok;
            samples.push(elapsed);
        }
        let mut row = summarize(
            "end_to_end",
            package.name,
            "cold_process",
            "check",
            "",
            &samples,
        );
        row.note = format!(
            "{:.1} reference units; ok={ok}",
            row.median_ms / reference_median
        );
        rows.push(row);
    }
    rows
}

/// Prints the human table under the JSON lines. Both, always: the JSON is what
/// a future comparison diffs and the table is what a reader reads.
fn report(rows: &[Row]) {
    for row in rows {
        println!("PERF {}", row.json());
    }
    println!();
    println!(
        "{:<11} {:<16} {:<13} {:<12} {:>5} {:>10} {:>10} {:>10}",
        "section", "corpus", "mode", "metric", "runs", "min ms", "median ms", "max ms",
    );
    for row in rows {
        println!(
            "{:<11} {:<16} {:<13} {:<12} {:>5} {:>10.2} {:>10.2} {:>10.2}",
            row.section,
            row.corpus,
            row.mode,
            row.metric,
            row.runs,
            row.min_ms,
            row.median_ms,
            row.max_ms,
        );
    }
}

/// Every row must be internally consistent — this is what the smoke test pins,
/// and running it on the full scale too costs nothing and catches a percentile
/// bug on real data.
fn assert_rows_are_well_formed(rows: &[Row]) {
    assert!(
        !rows.is_empty(),
        "the harness produced no measurements at all"
    );
    for row in rows {
        assert!(row.runs > 0, "{}: a row with no samples", row.json());
        assert!(
            row.min_ms <= row.median_ms
                && row.median_ms <= row.p95_ms
                && row.p95_ms <= row.p99_ms
                && row.p99_ms <= row.max_ms,
            "{}: the order statistics are not ordered",
            row.json(),
        );
        assert!(
            row.min_ms.is_finite() && row.max_ms.is_finite(),
            "{}: a non-finite timing",
            row.json(),
        );
        assert!(
            !row.corpus.is_empty() && !row.note.contains('"'),
            "{}: a field that would not survive the JSON line",
            row.json(),
        );
    }
}

fn run(scale: Scale) -> Vec<Row> {
    let mut rows = phase_section(scale);
    rows.extend(end_to_end_section(scale));
    report(&rows);
    assert_rows_are_well_formed(&rows);
    rows
}

// ---------------------------------------------------------------------------
// The const pass's scaling pin (backlog M4, `const-eval.md` §10)
// ---------------------------------------------------------------------------

/// A style-heavy entry: `sites` module-level `const` style chains, the shape
/// `vilan-website/src/art.vl` is 79 of and which made the const pass two thirds
/// of that package's compile. Every site is a distinct chain, so no site's
/// result can be shared with another's — the pass has to do `sites` evaluations
/// however clever it gets.
fn style_heavy_source(sites: usize) -> String {
    let mut source = String::from(
        "import std::print;\n\
         import std::style::{ Color, Display, Length, space, style };\n\n",
    );
    for site in 0..sites {
        source.push_str(&format!(
            "let s{site} = const style()\n\
             \t.display(Display::Flex)\n\
             \t.padding(space({}))\n\
             \t.background(Color::gray({}))\n\
             \t.width(Length::px({}.0));\n",
            site % 7,
            (site % 9 + 1) * 100,
            site % 40 + 1,
        ));
    }
    source.push_str("\nfun main() {\n");
    for site in 0..sites {
        source.push_str(&format!("\tprint(s{site}.class_list());\n"));
    }
    source.push_str("}\n");
    source
}

/// The `post_analysis_passes` walls for TWO style-heavy entries, measured
/// warm, ALTERNATELY — small, large, small, large… — and each taken as the
/// MINIMUM of its rounds, the least-contended sample.
///
/// The alternation is the point, and it is a 2026-08-19 repair (found by
/// D17's lane): measured as two sequential blocks, the two mins were drawn
/// from two *disjoint time windows*, and contention that differs between the
/// windows — a sibling suite's compile storm landing on one block and not the
/// other — inflates the ratio instead of cancelling in it. Under a load-25
/// contended full-suite run the pin read **6.63×** against its 6× bound, and
/// passed alone. Interleaving draws both mins from rounds that span the
/// same period, which is §8.4's run-the-binaries-alternately discipline
/// applied inside one process. (Both subjects import the same std names, so
/// they share one base-cache world and stay warm across the alternation.)
fn const_pass_walls(
    small_sites: usize,
    large_sites: usize,
    std: &PackageSpec,
    platform: Platform,
) -> (Duration, Duration) {
    const ROUNDS: usize = 3;
    let small = PhaseSubject::synthetic(
        &format!("style_{small_sites}"),
        &style_heavy_source(small_sites),
    );
    let large = PhaseSubject::synthetic(
        &format!("style_{large_sites}"),
        &style_heavy_source(large_sites),
    );
    // One throwaway compile per subject puts both on the same cache footing,
    // exactly as `phase_section` primes its subjects.
    let _ = measure_phases(&small, std, platform, false);
    let _ = measure_phases(&large, std, platform, false);
    let mut small_wall = Duration::MAX;
    let mut large_wall = Duration::MAX;
    for _ in 0..ROUNDS {
        small_wall = small_wall.min(measure_phases(&small, std, platform, false).post);
        large_wall = large_wall.min(measure_phases(&large, std, platform, false).post);
    }
    (small_wall, large_wall)
}

/// The pass must stay LINEAR in its const sites.
///
/// This is the property M4's fix establishes and the one a future change is
/// most likely to lose: the const pass compiles every `const` site to its own
/// mini-program, so anything it does per site that is a fact about the whole
/// program — rebuilding the transformer's name seed, walking the module tree,
/// scanning every `const` span — multiplies the site count by the program size
/// and turns a linear pass into a quadratic one. Both of those existed; both
/// were hoisted (`const-eval.md` §10).
///
/// Relative by construction, never a fixed number of seconds: the assertion is
/// four times the sites against a bound of six times the time, measured in the
/// same process, in the same session, on the same source shape — the
/// measured-reference discipline `tests/support/mod.rs` established for the
/// suite's liveness bounds, and for the same reason (`suite-speed.md` §5–§7:
/// three separate incidents of a clock in the gate, each fixed by replacing it
/// with a ratio). The headroom is deliberately generous — a 4× ratio with 50 %
/// slack — because the useful failure is a change of SHAPE, and a shape change
/// blows through it while noise does not.
///
/// The 2026-08-19 flake, and why the repair is the MEASUREMENT and not the
/// bound (`perf-baseline.md` §6.3). Under a load-25 contended full-suite run
/// the pin read **6.63×** once and passed alone (found by D17's lane): the
/// two mins were then drawn from two sequential, disjoint time windows, so a
/// wall-clock ratio was load-sensitive by construction — contention landing
/// unevenly across the windows inflates the ratio instead of cancelling in
/// it. The rounds now interleave (see [`const_pass_walls`]); re-measured
/// under a *worse* load (~40) the interleaved pin reads 3.13–3.59× against
/// the quiet machine's 3.44×, so the construction, not the bound, was the
/// flake. Widening instead was tried and measured VACUOUS: a genuinely
/// quadratic plant — one whole-world rebuild plus a re-walk of every other
/// site, per site — reads **7.63–8.01×** at this size, so an 8× bound waves
/// a real quadratic through while 6× catches it. (The heavier historical
/// plant, a whole-program mini-build per other site, read 13.89×.) 6×
/// therefore stands, now with plant measurements on BOTH sides of it.
///
/// Honest about what it does and does not catch, because a pin that is believed
/// to catch more than it does is worse than none. At the site counts a gate can
/// afford, `std`'s own ~4,000 entities dominate the per-site term, so the
/// pre-fix tree passes this too: measured on this machine, pre-fix **3.53×**
/// against the fixed tree's **3.44×** — a real improvement in the absolute
/// numbers (114.6 → 101.6 ms and 404.8 → 349.8 ms) and nothing a ratio bound can
/// separate. What reddens on the pre-fix tree is the counter pin in
/// `vilan-core/tests/inference.rs`
/// (`the_const_pass_builds_one_name_seed_however_many_const_sites_there_are`).
/// This one guards the ASYMPTOTE, and is non-vacuous on its own terms: planting
/// one whole-program mini-build per other const site — the exact shape it
/// exists for — took it to **13.89×** (518.8 ms → 7207.2 ms) and red.
#[test]
fn the_const_pass_scales_with_its_const_sites_and_not_with_their_square() {
    const SITES: usize = 20;
    const FACTOR: usize = 4;
    // Generous, and argued for above: four times the work must not cost six
    // times the time. Not wider — the cheap quadratic plant reads ~7.6–8.0×,
    // so 8 would be vacuous (see the doc comment).
    const BOUND: f64 = 6.0;

    let (small, large) = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(|| {
            let std = std_spec();
            let platform = Platform::default();
            const_pass_walls(SITES, SITES * FACTOR, &std, platform)
        })
        .expect("spawn the const-scaling measurement thread")
        .join()
        .expect("the const-scaling measurement thread panicked");

    let ratio = large.as_secs_f64() / small.as_secs_f64();
    println!(
        "PERF-SCALE const_pass {SITES} sites = {:.2} ms, {} sites = {:.2} ms, ratio {ratio:.2}×",
        milliseconds(small),
        SITES * FACTOR,
        milliseconds(large),
    );
    assert!(
        small > Duration::ZERO,
        "the {SITES}-site measurement read zero, so the ratio means nothing",
    );
    assert!(
        ratio <= BOUND,
        "{}× the const sites cost {ratio:.2}× the post-analysis passes \
         ({:.2} ms → {:.2} ms), over the {BOUND}× bound: the const pass has \
         gone super-linear in its sites (const-eval.md §10)",
        FACTOR,
        milliseconds(small),
        milliseconds(large),
    );
}

#[test]
#[ignore = "the performance baseline: minutes of measurement, run deliberately (proposal/perf-baseline.md §3)"]
fn perf_baseline_full_run() {
    let rows = run(Scale::Full);
    // The full run must have reached both sections; a corpus that silently
    // resolved to nothing would otherwise publish a baseline of the tiny
    // subject alone.
    assert!(
        rows.iter()
            .any(|row| row.section == "phase" && row.corpus == "std_wide"),
        "the cold whole-world subject did not run",
    );
    assert!(
        rows.iter().any(|row| row.section == "end_to_end"),
        "no end-to-end measurement ran",
    );
}

/// The gate's pin on the harness: it runs, on the smallest corpus, and every
/// row it emits is well formed. Seconds, not minutes — the baseline itself is
/// `#[ignore]`d, and this is the thing that keeps it from rotting between the
/// runs that matter.
#[test]
fn perf_baseline_harness_smoke() {
    let rows = run(Scale::Smoke);
    assert!(
        rows.iter()
            .any(|row| row.section == "phase" && row.mode == "cold"),
        "the smoke run measured no cold compile",
    );
    assert!(
        rows.iter()
            .any(|row| row.section == "phase" && row.mode == "warm"),
        "the smoke run measured no warm compile",
    );
    assert!(
        rows.iter().any(|row| row.section == "end_to_end"),
        "the smoke run measured nothing end to end",
    );
}

/// The gate's pin on the *statistics*, over known samples rather than measured
/// ones.
///
/// The smoke run above cannot do this job: at one and two samples per row every
/// order statistic collapses onto the same value, so a percentile that reads
/// the wrong end of the distribution still comes out ordered and still passes.
/// (Verified by planting exactly that bug — `1.0 - fraction` — and watching the
/// smoke test stay green while this one goes red.) A hundred synthetic
/// durations of 1 ms … 100 ms make every rank distinguishable, which is what
/// turns "the rows are ordered" into "the rows are the numbers they claim".
#[test]
fn perf_baseline_summary_reports_the_order_statistics_it_names() {
    let samples: Vec<Duration> = (1..=100).map(Duration::from_millis).collect();
    // Deliberately not in order: `summarize` owns the sort, and a caller that
    // hands it measurement order must get the same answer.
    let shuffled: Vec<Duration> = samples.iter().rev().copied().collect();
    let row = summarize("phase", "fixture", "warm", "total", "", &shuffled);

    assert_eq!(row.runs, 100);
    assert_eq!(row.min_ms, 1.0, "min is the smallest sample");
    // Nearest rank: p50 of 100 samples is the 50th, p95 the 95th, p99 the 99th.
    assert_eq!(row.median_ms, 50.0, "median is the 50th of 100");
    assert_eq!(row.p95_ms, 95.0, "p95 is the 95th of 100");
    assert_eq!(row.p99_ms, 99.0, "p99 is the 99th of 100");
    assert_eq!(row.max_ms, 100.0, "max is the largest sample");

    // A single sample is every statistic at once — the degenerate case the
    // smoke run's cold row actually takes.
    let single = summarize("phase", "fixture", "cold", "total", "", &samples[..1]);
    assert_eq!(
        (
            single.min_ms,
            single.median_ms,
            single.p99_ms,
            single.max_ms
        ),
        (1.0, 1.0, 1.0, 1.0),
    );
}
