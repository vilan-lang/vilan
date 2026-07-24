//! The macro engine's conformance gate (proposal/macro-engine.md §5): the
//! fueled `js::Node` interpreter must agree with a real JS engine on every
//! corpus program inside its subset. Each admitted program is compiled once,
//! then executed BOTH ways — formatted and run under node, and evaluated by
//! `interpreter::run_program` — and the (stdout, exit code) pairs must match
//! exactly. Programs outside the subset (async, host capabilities) are listed
//! with the reason; everything else MUST pass, so a pure program regressing
//! into "unsupported" fails the suite rather than silently skipping.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use vilan_core::interpreter::{self, FailureKind, Limits};
use vilan_core::{
    BuildOptions, PackageSpec, Platform, Workspace, analyze_source, transform, transform_to_ast,
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
    ("nursery.vl", "async (the nursery join awaits)"),
    (
        "reactive-turns.vl",
        "async (the turn-follows-continuation section)",
    ),
    ("process-env.vl", "host environment (`__env`, `__args`)"),
    ("crypto.vl", "async + host WebCrypto (`crypto.subtle`)"),
    ("db.vl", "host database (`node:sqlite`)"),
    (
        "time.vl",
        "host clock + timers (`Date.now`, `Date#toISOString`, `setTimeout`)",
    ),
];

/// How long a corpus program gets under node before the run is declared hung.
const NODE_TIMEOUT: Duration = Duration::from_secs(30);

/// Runs `node <path>` under a deadline, returning its `(stdout, exit code)`.
///
/// Replaces a `timeout 30 node …` shell-out: `timeout(1)` is a coreutils binary
/// that does not exist on Windows, and this gate MUST run there
/// (windows-support.md §4). Capture semantics are the ones `Command::output()`
/// gives — both pipes drained to EOF, stdout returned lossily-decoded — with the
/// drain done by two reader threads so a chatty program cannot deadlock against
/// a full pipe buffer while the main thread polls for exit. A child still alive
/// at the deadline is killed and reported as a timeout instead of silently
/// comparing truncated output.
fn run_node(path: &Path, limit: Duration) -> Result<(String, i32), String> {
    let mut child = Command::new("node")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not run node: {error}"))?;
    let mut stdout_pipe = child.stdout.take().expect("piped stdout");
    let mut stderr_pipe = child.stderr.take().expect("piped stderr");
    let reading_stdout = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut bytes);
        bytes
    });
    let reading_stderr = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut bytes);
        bytes
    });

    let deadline = Instant::now() + limit;
    let status = loop {
        match child
            .try_wait()
            .map_err(|error| format!("waiting on node: {error}"))?
        {
            Some(status) => break Some(status),
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    };
    // Both threads end at EOF — which the kill guarantees on the timeout path.
    let stdout = reading_stdout.join().unwrap_or_default();
    let _ = reading_stderr.join();
    match status {
        Some(status) => Ok((
            String::from_utf8_lossy(&stdout).into_owned(),
            status.code().unwrap_or(-1),
        )),
        None => Err(format!(
            "node did not exit within {}s (killed); it had printed {} bytes",
            limit.as_secs(),
            stdout.len()
        )),
    }
}

/// Runs `source` through the pipeline once, then both execution paths.
/// Returns `(node stdout, node exit code, interpreter result)`.
#[allow(clippy::type_complexity)]
fn both_ways(
    source: String,
    fuel: u64,
) -> Result<(String, i32, Result<(String, i32), (FailureKind, String)>), String> {
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
                _ => return Err(format!("compile failed: {errors:?}")),
            };
            let options = BuildOptions::default();

            // Path 1: the formatter + a real JS engine.
            let text = transform(&program, &options).map_err(|error| error.msg)?;
            use std::sync::atomic::{AtomicU32, Ordering};
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("vilan_equiv_{}_{unique}.js", std::process::id()));
            std::fs::write(&path, text).map_err(|error| error.to_string())?;
            let run = run_node(&path, NODE_TIMEOUT);
            let _ = std::fs::remove_file(&path);
            let (node_stdout, node_exit) = run?;

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
            Ok((node_stdout, node_exit, interpreted))
        })
        .expect("spawn worker")
        .join()
        .unwrap_or_else(|_| Err("worker thread aborted".to_string()))
}

#[test]
fn every_admitted_corpus_program_is_equivalent_interpreted() {
    let corpus = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/test");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&corpus)
        .expect("corpus directory")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            (path.extension()? == "vl").then_some(path)
        })
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no corpus programs found");

    let mut failures = Vec::new();
    let mut checked = 0;
    for path in &paths {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if EXCLUDED.iter().any(|(excluded, _)| *excluded == name) {
            continue;
        }
        let source = std::fs::read_to_string(path).expect("read corpus file");
        match both_ways(source, 50_000_000) {
            Ok((node_stdout, node_exit, Ok((interp_stdout, interp_exit)))) => {
                checked += 1;
                if node_stdout != interp_stdout || node_exit != interp_exit {
                    let first_diff = node_stdout
                        .lines()
                        .zip(interp_stdout.lines())
                        .enumerate()
                        .find(|(_, (a, b))| a != b)
                        .map(|(line, (a, b))| format!("line {}: node {a:?} vs interp {b:?}", line + 1))
                        .unwrap_or_else(|| {
                            format!(
                                "lengths/exits differ (node {} lines exit {node_exit}, interp {} lines exit {interp_exit})",
                                node_stdout.lines().count(),
                                interp_stdout.lines().count()
                            )
                        });
                    failures.push(format!("{name}: {first_diff}"));
                }
            }
            Ok((_, _, Err((kind, message)))) => {
                failures.push(format!("{name}: interpreter failed ({kind:?}): {message}"));
            }
            Err(error) => failures.push(format!("{name}: {error}")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} corpus programs diverged:\n{}",
        failures.len(),
        checked + failures.len(),
        failures.join("\n")
    );
    assert!(checked > 60, "suspiciously few programs checked: {checked}");
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
        import std::print;

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
// `run_node` replaced a `timeout 30 node …` shell-out, so its contract is
// pinned directly rather than only through the corpus sweep above: the capture
// semantics the equivalence check compares against, and the two edges the
// rewrite exists to get right — a program that never exits, and one that
// outdruns the pipe buffer.

/// Writes a scratch `.js` file for the runner pins; returns its path.
fn scratch_js(tag: &str, source: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "vilan_run_node_{tag}_{}_{unique}.js",
        std::process::id()
    ));
    std::fs::write(&path, source).expect("write the scratch program");
    path
}

#[test]
fn the_node_runner_captures_stdout_and_a_zero_exit() {
    let path = scratch_js("ok", "console.log('one');\nconsole.log('two');\n");
    let run = run_node(&path, NODE_TIMEOUT);
    let _ = std::fs::remove_file(&path);
    assert_eq!(run, Ok(("one\ntwo\n".to_string(), 0)));
}

#[test]
fn the_node_runner_reports_a_nonzero_exit_code() {
    // The equivalence check compares exit codes, so a wrong code is a silent
    // false pass: pin that the child's own code survives, stdout included.
    let path = scratch_js("exit", "console.log('bye');\nprocess.exit(3);\n");
    let run = run_node(&path, NODE_TIMEOUT);
    let _ = std::fs::remove_file(&path);
    assert_eq!(run, Ok(("bye\n".to_string(), 3)));
}

#[test]
fn the_node_runner_kills_a_program_that_never_exits() {
    // What `timeout 30` used to do. The deadline must be enforced by the
    // runner, not by the suite noticing a hang an hour later.
    let path = scratch_js("hang", "setInterval(() => {}, 1000);\n");
    let started = Instant::now();
    let run = run_node(&path, Duration::from_millis(500));
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
    let run = run_node(&path, NODE_TIMEOUT);
    let _ = std::fs::remove_file(&path);
    let (stdout, exit) = run.expect("a chatty program must still run to completion");
    assert_eq!(exit, 0);
    assert_eq!(stdout.len(), 1024 * 1024, "every byte must be captured");
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
