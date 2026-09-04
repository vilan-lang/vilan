//! E36 — THE gate for the release preset: over the whole corpus, a
//! `preset = "release"` build must be observationally identical to the debug
//! build of the same program.
//!
//! Until this existed the release emission path was ungated. The corpus gate
//! (`vilan-cli/tests/corpus.rs`) builds bare `.vl` files with no manifest, so it
//! only ever exercises the debug default — deliberately, because that is what
//! makes "no corpus golden moved" a real signal — and `infer_preset.rs` pins the
//! release path on a single fixture. Everything else release does to a program
//! was unobserved, which is how B69 shipped: minification's short-name renaming
//! collided on seven corpus programs, on a v0.27.0 binary, for as long as anyone
//! had been able to run `vilan build` on a release manifest.
//!
//! **Not a golden.** Release output legitimately differs from debug in almost
//! every byte — different names, no indentation, no padding, folded constants —
//! so there is nothing to compare bytes against and a release golden would pin
//! the minifier's current letters rather than its correctness. What the two
//! builds owe each other is BEHAVIOUR, and the only instrument that reads it is
//! node. That matters more here than anywhere else in the tree: a renaming bug
//! emits perfectly valid JavaScript (`default.vl`'s two `function b` compile
//! fine and recurse forever), so "it still builds" proves nothing at all.
//!
//! Method (`infer_differential.rs`'s shape, reused): each corpus program is
//! analyzed ONCE and transformed twice off the same `Program` — debug, then
//! release with the inference sweep's folds installed, exactly as `vilan build`
//! wires it — and both emissions are run under node with their `(stdout, exit
//! code)` compared. Unlike the inference differential there is no
//! run-only-on-difference shortcut to take, since release always differs; the
//! whole corpus runs twice, which is what the leg costs.
//!
//! # One test per corpus program (tracker N49)
//!
//! That cost used to be paid by ONE test. It looped the corpus behind an 8-way
//! `thread::scope`, and at 615.5 s under load it was the union's last finisher
//! — a straggler nextest could do nothing about, because the whole leg was a
//! single unit of scheduling that could not use more than its own eight threads
//! however idle the box was, and whose static chunking made the slowest chunk
//! the whole test's clock. Its failure message had to name the program itself,
//! for the same reason.
//!
//! So the corpus is DECLARED — in [`corpus_harness`], once for this gate and its
//! inference sibling both (tracker N52) — and a macro writes a test per program.
//! nextest then schedules 124 independent processes across every core it has, a
//! regression names its program in the test id rather than in a message, and one
//! bad program can be re-run on its own. The corpus-wide claim survives as the
//! SUM of the parts, and [`every_corpus_program_has_a_test_of_its_own`] is what
//! makes the sum whole: a `.vl` added to `vilan/test/` and not declared is red,
//! by name, in both binaries.
//!
//! **And the split named its program on the first run.** 600 of those 615
//! seconds were `watch.vl` alone, which never exits under node: both builds ran
//! to `NODE_TIMEOUT`, both were killed at 300 s, and the gate compared two
//! identical "node did not exit" strings and passed. The corpus was never the
//! cost and the whole-corpus loop is what hid it — a per-program time is the
//! thing nobody could read while every program shared one clock.

use std::path::{Path, PathBuf};

use vilan_core::options::{BuildOptions, Preset};
use vilan_core::{PackageSpec, Platform, Workspace, analyze_source, transform};

#[macro_use]
mod corpus_harness;
use corpus_harness::{
    assert_every_program_not_run_is_a_corpus_program, assert_the_declaration_is_the_corpus,
    corpus_dir, not_run_reason, run,
};

fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    )
}

/// What one corpus program's two builds came to.
struct Compared {
    debug: String,
    release: String,
}

/// Analyzes once and transforms twice off the same `Program`, so the only
/// difference between the two emissions is the preset. The release half runs the
/// inference sweep the way `vilan build` does — after analysis, gated on the
/// resolved options — so what this gate reads is the release path a user gets,
/// not an approximation of it.
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
            let debug_options = BuildOptions::from_preset(Preset::Debug);
            let release_options = BuildOptions::from_preset(Preset::Release);

            // Debug first: it must see `const_results` as analysis left them.
            let debug = transform(&program, &debug_options).map_err(|error| error.msg)?;

            program
                .const_results
                .extend(vilan_core::const_eval::infer(&program, &release_options));
            let release = transform(&program, &release_options).map_err(|error| error.msg)?;

            Ok(Compared { debug, release })
        })
        .expect("spawn worker")
        .join()
        .unwrap_or_else(|_| Err("worker thread aborted".to_string()))
}

/// One corpus program, both ways. The body every generated test runs.
fn the_release_preset_is_neutral_on(program: &str) {
    let corpus = corpus_dir();
    let path = corpus.join(program);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let compared = build_both_ways(source, corpus).unwrap_or_else(|error| panic!("{error}"));

    if let Some(why) = not_run_reason(program) {
        // Compiled both ways — which is worth having on its own — and not run.
        // Said out loud, because a skip nobody can see is a skip nobody rereads.
        eprintln!(
            "[release differential] {program}: compiled both ways, not run under node — {why}"
        );
        return;
    }

    let release = run(&compared.release, "release", "release");
    let debug = run(&compared.debug, "release", "debug");
    match (release, debug) {
        (Ok(release), Ok(debug)) => assert!(
            release == debug,
            "THE RELEASE PRESET CHANGED BEHAVIOUR\n  \
             release: exit {}, stdout {:?}\n  \
             debug:   exit {}, stdout {:?}\n  \
             release emitted:\n{}",
            release.1,
            release.0,
            debug.1,
            debug.0,
            compared.release
        ),
        // A run that could not happen is only a failure if the two sides
        // disagree about it.
        (release, debug) => assert!(
            format!("{release:?}") == format!("{debug:?}"),
            "one build ran and the other did not\n  release: {release:?}\n  debug: {debug:?}"
        ),
    }
}

/// Writes one test per corpus program, and records the declaration the coverage
/// gate below reads.
///
/// The module is named for the program, so the test id nextest prints is
/// `release_differential list_sort::survives_the_release_preset` — the program's
/// own name, in the place a runner shows it.
macro_rules! corpus_programs {
    ($($module:ident => $file:literal,)*) => {
        /// Every corpus program with a test, as `(module name, file name)`.
        const DECLARED: &[(&str, &str)] = &[$((stringify!($module), $file),)*];

        $(
            mod $module {
                #[test]
                fn survives_the_release_preset() {
                    super::the_release_preset_is_neutral_on($file);
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
