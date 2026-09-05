//! The macro engine's conformance gate (proposal/macro-engine.md §5): the
//! fueled `js::Node` interpreter must agree with a real JS engine on every
//! corpus program inside its subset. Each admitted program is compiled once,
//! then executed BOTH ways — formatted and run under node, and evaluated by
//! `interpreter::run_program` — and the (stdout, exit code) pairs must match
//! exactly. Programs outside the subset (async, host capabilities) are listed
//! with the reason; everything else MUST pass, so a pure program regressing
//! into "unsupported" fails the suite rather than silently skipping.
//!
//! # One test per corpus program (tracker N53)
//!
//! This gate was the last reader of `vilan/test/` still shaped the way N49 found
//! its siblings: ONE `#[test]` looping the whole corpus behind an 8-way
//! `thread::scope`, and a private `run_node` whose deadline was still the 30 s
//! the differentials moved off (N46: these binaries were among the >120 s
//! members of two sibling lanes' unions under ten-lane load, so a fixed 30 s
//! clock around real work measures the runner and not the program).
//!
//! Both are now the shared [`corpus_harness`]'s — one declared roster for all
//! three gates, one node runner, one deadline — so nextest schedules a process
//! per program, a regression names its program in the test id rather than in a
//! message, one bad program can be re-run on its own, and a per-program time
//! becomes readable where a single clock over 124 programs hid it. The
//! corpus-wide claim survives as the SUM of the parts, and
//! [`every_corpus_program_has_a_test_of_its_own`] is what makes the sum whole: a
//! `.vl` added to `vilan/test/` and not declared in the harness is red, by name,
//! in all three binaries.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use vilan_core::interpreter::{self, FailureKind, Limits};
use vilan_core::{
    BuildOptions, PackageSpec, Platform, Workspace, analyze_source, transform, transform_to_ast,
};

#[macro_use]
mod corpus_harness;
use corpus_harness::{
    NODE_TIMEOUT, assert_the_declaration_is_the_corpus, corpus_dir, run_node_within,
};

fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    )
}

/// Corpus files outside the interpreter's subset, with the capability that
/// excludes them. Everything not listed here must pass the equivalence check.
const EXCLUDED: &[(&str, &str)] = &[
    ("adapt.vl", "async (adapted instances await)"),
    ("async-await.vl", "async"),
    ("async-promise-all.vl", "async"),
    (
        "await-postfix.vl",
        "async + host timer (`sleep`) — every helper there awaits",
    ),
    ("nursery.vl", "async (the nursery join awaits)"),
    (
        "reactive-turns.vl",
        "async (the turn-follows-continuation section)",
    ),
    ("process-env.vl", "host environment (`__env`, `__args`)"),
    ("crypto.vl", "async + host WebCrypto (`crypto.subtle`)"),
    ("db.vl", "host database (`node:sqlite`)"),
    (
        "file.vl",
        "async + host filesystem (`node:fs/promises` handles)",
    ),
    (
        "time.vl",
        "host clock + timers (`Date.now`, `Date#toISOString`, `setTimeout`)",
    ),
    (
        "watch.vl",
        "async + host filesystem + a host timer (`std::fs::Watcher` polls on `setTimeout`)",
    ),
];

/// Runs `source` through the pipeline once, then both execution paths.
/// Returns `(node stdout, node stderr, node exit code, interpreter result)`.
///
/// node's stderr is kept OUT of the compared value and carried beside it:
/// the interpreter has no stderr to answer with, so appending it the way the
/// differentials do (they compare node against node) would make every
/// nonzero-exit program — `resource_exit.vl` exits 7 — diverge on a string the
/// other side cannot produce. It still rides into the failure message, where a
/// `SyntaxError` out of a broken emission is the whole diagnosis.
/// A corpus program's package root is the corpus DIRECTORY, not the process
/// working directory. Every runner over the corpus has to say so: a corpus
/// program may name a project file — `const asset::bundle` carries a resource
/// beside it into the build (kolt.local 029), and `const asset::read` would
/// read one — and the const channel resolves those against the package root.
/// Compiled under `.`, such a program fails to find a file that is right there
/// beside it.
type BothWays = (
    String,
    String,
    i32,
    Result<(String, i32), (FailureKind, String)>,
);

fn both_ways(source: String, root: PathBuf, fuel: u64) -> Result<BothWays, String> {
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
            let program = match program {
                Some(program) if errors.is_empty() => program,
                _ => return Err(format!("compile failed: {errors:?}")),
            };
            let options = BuildOptions::default();

            // Path 1: the formatter + a real JS engine.
            let text = transform(&program, &options).map_err(|error| error.msg)?;
            use std::sync::atomic::{AtomicU32, Ordering};
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("vilan_equiv_{}_{unique}.mjs", std::process::id()));
            std::fs::write(&path, text).map_err(|error| error.to_string())?;
            let run = run_node_within(&path, NODE_TIMEOUT);
            let _ = std::fs::remove_file(&path);
            let (node_stdout, node_stderr, node_exit) = run?;

            // Path 2: the interpreter over the transformer's own AST.
            let ast = transform_to_ast(&program, &options).map_err(|error| error.msg)?;
            let interpreted = match interpreter::run_program(
                &ast,
                Limits {
                    fuel,
                    call_depth: 2048,
                },
            ) {
                Ok(run) => Ok((run.stdout, run.exit_code)),
                Err(failure) => Err((failure.kind, failure.message)),
            };
            Ok((node_stdout, node_stderr, node_exit, interpreted))
        })
        .expect("spawn worker")
        .join()
        .unwrap_or_else(|_| Err("worker thread aborted".to_string()))
}

/// The reason `program` is outside the interpreter's subset, if it is.
fn excluded_reason(program: &str) -> Option<&'static str> {
    EXCLUDED
        .iter()
        .find(|(excluded, _)| *excluded == program)
        .map(|(_, why)| *why)
}

/// One corpus program, both ways. The body every generated test runs.
fn the_interpreter_agrees_on(program: &str) {
    if let Some(why) = excluded_reason(program) {
        // Said out loud, because a skip nobody can see is a skip nobody rereads.
        eprintln!("[interpreter] {program}: outside the interpreter's subset — {why}");
        return;
    }
    let corpus = corpus_dir();
    let path = corpus.join(program);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    match both_ways(source, corpus, 50_000_000) {
        Ok((node_stdout, node_stderr, node_exit, Ok((interp_stdout, interp_exit)))) => {
            if node_stdout == interp_stdout && node_exit == interp_exit {
                return;
            }
            let first_diff = node_stdout
                .lines()
                .zip(interp_stdout.lines())
                .enumerate()
                .find(|(_, (node, interp))| node != interp)
                .map(|(line, (node, interp))| {
                    format!("line {}: node {node:?} vs interp {interp:?}", line + 1)
                })
                .unwrap_or_else(|| {
                    format!(
                        "lengths/exits differ (node {} lines exit {node_exit}, interp {} \
                         lines exit {interp_exit})",
                        node_stdout.lines().count(),
                        interp_stdout.lines().count()
                    )
                });
            let stderr = if node_stderr.trim().is_empty() {
                String::new()
            } else {
                format!("\n  node stderr: {node_stderr}")
            };
            panic!("{program}: {first_diff}{stderr}");
        }
        Ok((_, _, _, Err((kind, message)))) => {
            panic!("{program}: interpreter failed ({kind:?}): {message}")
        }
        Err(error) => panic!("{program}: {error}"),
    }
}

/// Writes one test per corpus program, and records the declaration the coverage
/// gate below reads.
///
/// The module is named for the program, so the test id nextest prints is
/// `interpreter list_sort::is_equivalent_interpreted` — the program's own name,
/// in the place a runner shows it.
macro_rules! corpus_programs {
    ($($module:ident => $file:literal,)*) => {
        /// Every corpus program with a test, as `(module name, file name)`.
        const DECLARED: &[(&str, &str)] = &[$((stringify!($module), $file),)*];

        $(
            mod $module {
                #[test]
                fn is_equivalent_interpreted() {
                    super::the_interpreter_agrees_on($file);
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
fn every_excluded_program_is_still_a_corpus_program() {
    // `EXCLUDED`'s inverse (N42's shape, N50's family). An exemption only ever
    // subtracts work, so a name that has left the corpus goes on subtracting it
    // from nothing, and the next program to take that name inherits a skip
    // nobody chose.
    let declared: std::collections::BTreeSet<&str> =
        DECLARED.iter().map(|(_, file)| *file).collect();
    let gone: Vec<&str> = EXCLUDED
        .iter()
        .map(|(file, _)| *file)
        .filter(|file| !declared.contains(file))
        .collect();
    assert!(
        gone.is_empty(),
        "`EXCLUDED` names {gone:?}, which is not a corpus program — the exclusion \
         excludes nothing. Delete the entry."
    );

    // Non-vacuity, the bound the whole-corpus loop asserted at the end of its own
    // body: nearly all of the corpus is inside the interpreter's subset, and a
    // gate whose subset has quietly emptied proves nothing.
    let admitted = DECLARED.len() - EXCLUDED.len();
    assert!(
        admitted > 60,
        "only {admitted} corpus program(s) are inside the interpreter's subset — \
         the gate is close to vacuous. A pure program regressing into \
         \"unsupported\" belongs in a red test, not in `EXCLUDED`."
    );
    eprintln!(
        "[interpreter] {admitted} of {} corpus programs run both ways; {} are \
         outside the subset",
        DECLARED.len(),
        EXCLUDED.len()
    );
}

// --- Failure-mode pins -------------------------------------------------------

/// Compiles and runs the INTERPRETER ONLY — no node. The failure-mode pins
/// exercise programs a real JS engine would run forever (that's the point of
/// fuel), so they must never reach `both_ways`'s node half.
fn interpret(source: &str, fuel: u64) -> Result<(String, i32), (FailureKind, String)> {
    let source = source.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            let program = match program {
                Some(program) if errors.is_empty() => program,
                _ => panic!("compile failed: {errors:?}"),
            };
            let ast = transform_to_ast(&program, &BuildOptions::default())
                .unwrap_or_else(|error| panic!("transform failed: {}", error.msg));
            match interpreter::run_program(
                &ast,
                Limits {
                    fuel,
                    call_depth: 2048,
                },
            ) {
                Ok(run) => Ok((run.stdout, run.exit_code)),
                Err(failure) => Err((failure.kind, failure.message)),
            }
        })
        .expect("spawn worker")
        .join()
        .expect("worker thread aborted")
}

#[test]
fn fuel_exhaustion_is_a_clean_error() {
    let (kind, message) = interpret(
        r#"
        fun main() {
            mut n = 0;
            for {
                n = n + 1;
            }
        }

        main();
        "#,
        10_000,
    )
    .expect_err("an infinite loop must exhaust fuel");
    assert_eq!(kind, FailureKind::Fuel);
    assert!(message.contains("fuel"), "unexpected message: {message}");
}

#[test]
fn runaway_recursion_hits_the_depth_cap() {
    let (kind, _) = interpret(
        r#"
        fun forever(n: i32): i32 {
            forever(n + 1)
        }

        fun main() {
            forever(0);
        }

        main();
        "#,
        50_000_000,
    )
    .expect_err("unbounded recursion must hit the depth cap");
    assert_eq!(kind, FailureKind::Depth);
}

#[test]
fn an_impure_capability_is_a_clean_unsupported_error() {
    let (kind, message) = interpret(
        r#"
        import std::random;
        import std::io::print;

        fun main() {
            print(random::range_i32(1, 6));
        }

        main();
        "#,
        1_000_000,
    )
    .expect_err("randomness must be unavailable at expansion time");
    assert_eq!(kind, FailureKind::Unsupported);
    assert!(
        message.contains("not available at expansion time"),
        "unexpected message: {message}"
    );
}

// --- The portable node runner (windows-support.md §4) ------------------------
//
// `corpus_harness::run_node_within` replaced a `timeout 30 node …` shell-out, so
// its contract is pinned directly rather than only through the corpus sweep
// above: the capture semantics the equivalence check compares against, and the
// two edges the rewrite exists to get right — a program that never exits, and
// one that outruns the pipe buffer. The pins live here because this is where the
// runner was written; it is the shared one now, and all three corpus gates rest
// on what they say.

/// Writes a scratch `.js` file for the runner pins; returns its path.
fn scratch_js(tag: &str, source: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "vilan_run_node_{tag}_{}_{unique}.mjs",
        std::process::id()
    ));
    std::fs::write(&path, source).expect("write the scratch program");
    path
}

#[test]
fn the_node_runner_captures_stdout_and_a_zero_exit() {
    let path = scratch_js("ok", "console.log('one');\nconsole.log('two');\n");
    let run = run_node_within(&path, NODE_TIMEOUT);
    let _ = std::fs::remove_file(&path);
    assert_eq!(run, Ok(("one\ntwo\n".to_string(), String::new(), 0)));
}

#[test]
fn the_node_runner_reports_a_nonzero_exit_code() {
    // The equivalence check compares exit codes, so a wrong code is a silent
    // false pass: pin that the child's own code survives, stdout included.
    let path = scratch_js("exit", "console.log('bye');\nprocess.exit(3);\n");
    let run = run_node_within(&path, NODE_TIMEOUT);
    let _ = std::fs::remove_file(&path);
    assert_eq!(run, Ok(("bye\n".to_string(), String::new(), 3)));
}

#[test]
fn the_node_runner_kills_a_program_that_never_exits() {
    // What `timeout 30` used to do. The deadline must be enforced by the
    // runner, not by the suite noticing a hang an hour later.
    let path = scratch_js("hang", "setInterval(() => {}, 1000);\n");
    let started = Instant::now();
    let run = run_node_within(&path, Duration::from_millis(500));
    let _ = std::fs::remove_file(&path);
    let error = run.expect_err("a program that never exits must time out");
    assert!(
        error.contains("did not exit"),
        "unexpected message: {error}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "the runner returned only after {:?} — the kill did not land",
        started.elapsed()
    );
}

#[test]
fn the_node_runner_survives_more_output_than_a_pipe_holds() {
    // The reason the pipes are drained by threads instead of after the wait: a
    // pipe buffer is ~64 KiB, and a child blocked writing into a full one never
    // exits — so a poll-then-read runner would hang here until the deadline and
    // then compare TRUNCATED output. 1 MiB is comfortably past any platform's
    // buffer.
    let path = scratch_js(
        "chatty",
        "const line = 'x'.repeat(1023);\nfor (let i = 0; i < 1024; i += 1) console.log(line);\n",
    );
    let run = run_node_within(&path, NODE_TIMEOUT);
    let _ = std::fs::remove_file(&path);
    let (stdout, _, exit) = run.expect("a chatty program must still run to completion");
    assert_eq!(exit, 0);
    assert_eq!(stdout.len(), 1024 * 1024, "every byte must be captured");
}

#[test]
fn the_differentials_runner_carries_a_failing_program_s_stderr() {
    // The other policy over the same capture, which the two differentials read
    // and this gate deliberately does not (see `both_ways`): they compare node
    // against node, so a failing build's stderr rides along with the exit code
    // instead of leaving them to compare two empty stdouts. Pinned here because
    // an empty-stdout failure is exactly the one that passes vacuously.
    let path = scratch_js("stderr", "console.log('out');\nthrow new Error('boom');\n");
    let run = corpus_harness::run_node(&path);
    let _ = std::fs::remove_file(&path);
    let (captured, exit) = run.expect("a throwing program still exits");
    assert_ne!(exit, 0);
    assert!(
        captured.starts_with("out\n--- stderr ---\n") && captured.contains("boom"),
        "unexpected capture: {captured:?}"
    );
}

#[test]
fn a_panic_surfaces_as_thrown_with_its_message() {
    let (kind, message) = interpret(
        r#"
        import std::io::panic;

        fun main() {
            panic("boom at expansion time");
        }

        main();
        "#,
        1_000_000,
    )
    .expect_err("panic must surface as a thrown failure");
    assert_eq!(kind, FailureKind::Thrown);
    assert!(
        message.contains("boom at expansion time"),
        "unexpected message: {message}"
    );
}
