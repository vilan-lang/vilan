//! THE equivalence gate for inferred `const` (proposal/const-eval.md §9.2):
//! over the whole corpus, a build WITH the inference sweep must be
//! observationally identical to the same build WITHOUT it.
//!
//! Inference is the one optimization in the tree that silently rewrites what
//! the emitted program computes, and it rewrites it by *running* that
//! computation in a different engine. Everything that could go wrong with it —
//! a fold that swallows a `print`, a fold that turns a runtime panic into a
//! compile-time one, a fold that materializes a value the program was going to
//! mutate, a fold that reorders an initializer against a dependency — shows up
//! the same way: the program still compiles, and does something else. Only
//! running it can catch that.
//!
//! Method (`check_scope_differential.rs`'s shape, reused): each corpus program
//! is analyzed ONCE, then transformed twice from the same `Program` — once with
//! the sweep's folds installed and once without — so the sweep is the only
//! variable. If the two emissions are byte-identical the sweep changed nothing
//! and there is nothing to run; otherwise BOTH are executed under node and
//! their `(stdout, exit code)` must match exactly.
//!
//! The two builds use DEBUG codegen with `infer_const` forced on, not the
//! release preset, and the difference matters. Inference ships under `release`,
//! but observational neutrality is a property of FOLDING, not of the printer,
//! and pairing it with minification confounds the two. It also runs into a
//! pre-existing bug: release's short-name renaming collides on several corpus
//! programs — `default.vl` emits two module-level `function b`, the second
//! shadowing the first into infinite recursion — which reproduces exactly on
//! the shipped v0.27.0 binary with no inference involved. Isolating the knob is
//! what lets this gate say something about inference rather than about that.
//! The release path is pinned separately, by `vilan-cli/tests/infer_preset.rs`.
//!
//! # One test per corpus program (tracker N52)
//!
//! This gate kept the whole-corpus shape after N49 split its sibling
//! `release_differential.rs` out of it: one `#[test]`, one `thread::scope` over
//! eight static chunks, one clock across 124 programs. What that costs is not
//! visible until it is paid — and this gate escaped paying it only through the
//! run-only-on-difference rule below, which is a shortcut and not a bound. The
//! day folding reached `watch.vl`, the program that never exits, this leg would
//! have inherited its sibling's exact vacuous failure: two builds killed at the
//! deadline and a gate comparing two identical "node did not exit" strings.
//!
//! So the corpus is declared ONCE, in [`corpus_harness`], and shared with the
//! release gate — one roster, one exclusion list, one node deadline, no way for
//! the two to drift. nextest schedules a process per program, a regression names
//! its program in the test id rather than in a message, and one bad program can
//! be re-run on its own. The corpus-wide claim survives as the SUM of the parts,
//! and [`every_corpus_program_has_a_test_of_its_own`] makes the sum whole.
//!
//! # The shortcut's floor, per program (tracker N52)
//!
//! The old loop kept itself honest with a corpus-wide count: the number of
//! programs the sweep CHANGED was asserted at the end of the body, so the day
//! inference stopped folding, the gate failed instead of passing vacuously in a
//! fraction of the time. That number cannot be summed across independent
//! processes, and rebuilding it in a test of its own costs a second whole-corpus
//! compile pass — measured at 61 s under this suite's own parallelism, which
//! would have made the anti-vacuity check the straggler the split existed to
//! remove.
//!
//! So the floor is per program instead, and [`FOLDS`] is where it lives: the
//! programs the sweep changes, written down, checked BOTH ways inside the test
//! that already did the compile. Nothing is compiled twice, a program that
//! stopped folding fails by name rather than decrementing a count nobody reads,
//! and a program that STARTED folding is named too — that is a real change in
//! what inference reaches, and the one-line edit is where it gets noticed.

use std::path::{Path, PathBuf};

use vilan_core::options::{BuildOptions, Preset};
use vilan_core::{PackageSpec, Platform, Workspace, analyze_source, transform};

#[macro_use]
mod corpus_harness;
use corpus_harness::{
    NOT_RUN, assert_every_program_not_run_is_a_corpus_program,
    assert_the_declaration_is_the_corpus, corpus_dir, not_run_reason, run,
};

fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    )
}

/// What one corpus program's two builds came to.
struct Compared {
    /// The emitted JavaScript with the sweep off, then on.
    without: String,
    with: String,
}

/// Analyzes once and transforms twice off the same `Program`, so the ONLY
/// difference between the two emissions is whether the sweep's folds were
/// installed.
/// A corpus program's package root is the corpus DIRECTORY, not the process
/// working directory. Every runner over the corpus has to say so: a corpus
/// program may name a project file — `const asset::bundle` carries a resource
/// beside it into the build (kolt.local 029), and `const asset::read` would
/// read one — and the const channel resolves those against the package root.
/// Compiled under `.`, such a program fails to find a file that is right there
/// beside it.
fn build_both_ways(source: String, root: PathBuf) -> Result<Compared, String> {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                &root,
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            let mut program = match program {
                Some(program) if errors.is_empty() => program,
                _ => return Err(format!("compile failed: {errors:?}")),
            };
            // Debug codegen with the sweep forced on: the knob under test,
            // isolated from the printer (see the module doc).
            let mut options = BuildOptions::from_preset(Preset::Debug);
            options.infer_const = true;

            // The sweep off: `const_results` holds only what the EXPLICIT pass
            // put there, which is what `analyze_source` already left behind.
            let without = transform(&program, &options).map_err(|error| error.msg)?;

            // The sweep on, from the identical program.
            program
                .const_results
                .extend(vilan_core::const_eval::infer(&program, &options));
            let with = transform(&program, &options).map_err(|error| error.msg)?;

            Ok(Compared { without, with })
        })
        .expect("spawn worker")
        .join()
        .unwrap_or_else(|_| Err("worker thread aborted".to_string()))
}

/// Reads `program` out of the corpus and builds it both ways.
fn compare(program: &str) -> Compared {
    let corpus = corpus_dir();
    let path = corpus.join(program);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    build_both_ways(source, corpus).unwrap_or_else(|error| panic!("{program}: {error}"))
}

/// The corpus programs the inference sweep CHANGES — 29 of them when this gate
/// was written (const-eval.md §9.1), 32 today.
///
/// This is the gate's non-vacuity floor, and it is a list rather than a count
/// because a count cannot be summed across one process per program. Every
/// program here must still fold and every program not here must still not, both
/// asserted inside the per-program test that already ran the compile — so the
/// check is free, and it names the program that moved instead of reporting a
/// number that got smaller.
const FOLDS: &[&str] = &[
    "arena.vl",
    "backed-enum-keys.vl",
    "bool.vl",
    "capture-clones.vl",
    "const.vl",
    "default.vl",
    "derive-default.vl",
    "derive-json.vl",
    "element-clones.vl",
    "expression-lift.vl",
    "fixed-arrays.vl",
    "generic-adapter-dispatch.vl",
    "generic-equality.vl",
    "interpolated-multiline-string.vl",
    "iterator-adapters.vl",
    "json-roundtrip.vl",
    "list-sort.vl",
    "macro-block.vl",
    "map.vl",
    "match-ergonomics.vl",
    "math.vl",
    "mut-parameters.vl",
    "number-math.vl",
    "numeric-types.vl",
    "operator-overload.vl",
    "remainder.vl",
    "resource_take.vl",
    "set.vl",
    "time.vl",
    "tuple-access.vl",
    "tuple-spread.vl",
    "unary-minus.vl",
];

/// One corpus program, both ways. The body every generated test runs.
fn inference_is_neutral_on(program: &str) {
    let compared = compare(program);
    let folded = compared.with != compared.without;
    assert_eq!(
        folded,
        FOLDS.contains(&program),
        "`FOLDS` says the inference sweep {} `{program}` and it {} — what the \
         sweep reaches has changed. If that is the intended change, {} in \
         `FOLDS`; if it is not, the sweep has stopped doing its job here and \
         this gate would go green on it while running nothing.",
        if FOLDS.contains(&program) {
            "folds"
        } else {
            "leaves alone"
        },
        if folded { "folds it" } else { "does not" },
        if FOLDS.contains(&program) {
            "delete the line"
        } else {
            "add a line"
        },
    );
    if !folded {
        // The sweep folded nothing reachable here; there is no behaviour
        // difference to look for.
        eprintln!("[infer differential] {program}: the sweep changed nothing");
        return;
    }
    if let Some(why) = not_run_reason(program) {
        // Compiled both ways — which is worth having on its own — and not run.
        // Said out loud, because a skip nobody can see is a skip nobody rereads.
        eprintln!("[infer differential] {program}: folded, not run under node — {why}");
        return;
    }

    let folded = run(&compared.with, "infer", "with");
    let plain = run(&compared.without, "infer", "without");
    match (folded, plain) {
        (Ok(folded), Ok(plain)) => assert!(
            folded == plain,
            "FOLDING CHANGED BEHAVIOUR\n  \
             with the sweep:    exit {}, stdout {:?}\n  \
             without the sweep: exit {}, stdout {:?}",
            folded.1,
            folded.0,
            plain.1,
            plain.0
        ),
        // A run that could not happen is only a failure if the two sides
        // disagree about it.
        (folded, plain) => assert!(
            format!("{folded:?}") == format!("{plain:?}"),
            "one build ran and the other did not\n  with: {folded:?}\n  without: {plain:?}"
        ),
    }
}

/// Writes one test per corpus program, and records the declaration the coverage
/// gate below reads.
///
/// The module is named for the program, so the test id nextest prints is
/// `infer_differential list_sort::is_neutral_under_inference` — the program's
/// own name, in the place a runner shows it.
macro_rules! corpus_programs {
    ($($module:ident => $file:literal,)*) => {
        /// Every corpus program with a test, as `(module name, file name)`.
        const DECLARED: &[(&str, &str)] = &[$((stringify!($module), $file),)*];

        $(
            mod $module {
                #[test]
                fn is_neutral_under_inference() {
                    super::inference_is_neutral_on($file);
                }
            }
        )*
    };
}

corpus_manifest!(corpus_programs);

#[test]
fn every_corpus_program_has_a_test_of_its_own() {
    assert_the_declaration_is_the_corpus(DECLARED);
}

#[test]
fn every_program_not_run_is_still_a_corpus_program() {
    assert_every_program_not_run_is_a_corpus_program(DECLARED);
}

#[test]
fn the_sweep_folds_a_substantial_share_of_the_corpus() {
    // The floor the old whole-corpus loop asserted as a count, kept as a claim
    // about the LIST — and `FOLDS`'s inverse, so an entry that has left the
    // corpus cannot go on standing for a program nobody runs.
    let declared: std::collections::BTreeSet<&str> =
        DECLARED.iter().map(|(_, file)| *file).collect();
    let gone: Vec<&&str> = FOLDS
        .iter()
        .filter(|file| !declared.contains(**file))
        .collect();
    assert!(
        gone.is_empty(),
        "`FOLDS` names {gone:?}, which is not a corpus program — no test asserts \
         it, so it is a floor propping itself up. Delete the entry."
    );
    assert!(
        FOLDS.len() >= 20,
        "the inference sweep changes only {} corpus program(s) — 29 of them at \
         the time this gate was written (const-eval.md §9.1), so at this level \
         every per-program test passes by folding nothing and the gate proves \
         nothing. Check the sweep still runs.",
        FOLDS.len()
    );
    eprintln!(
        "[infer differential] the sweep changes {} of {} corpus programs; {} of \
         them are excluded from the node leg",
        FOLDS.len(),
        DECLARED.len(),
        NOT_RUN.len()
    );
}
