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
//! The run-only-on-difference rule is what keeps this affordable, and it also
//! makes the gate self-reporting: the count of programs the sweep changed is
//! asserted to be substantial, so the day inference silently stops folding
//! anything, this fails instead of passing vacuously in a fraction of the time.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use vilan_core::options::{BuildOptions, Preset};
use vilan_core::{PackageSpec, Platform, Workspace, analyze_source, transform};

fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    )
}

/// How long a corpus program gets under node before the run is declared hung.
const NODE_TIMEOUT: Duration = Duration::from_secs(30);

/// Corpus programs this gate does not run, with the reason. These are the
/// programs whose OUTPUT is not a function of their source alone — a clock, a
/// random draw, a port, a database — so "both builds printed the same thing" is
/// not a claim either build can make. They are still COMPILED both ways below
/// and their emissions compared byte-for-byte; only the node leg is skipped.
const NOT_RUN: &[(&str, &str)] = &[
    ("time.vl", "host clock: two runs print different timestamps"),
    ("crypto.vl", "host WebCrypto: a fresh random draw per run"),
    ("db.vl", "host database: touches the filesystem"),
    ("process-env.vl", "reads the host environment and argv"),
];

/// Runs `node <path>` under a deadline, returning `(stdout, exit code)`. Both
/// pipes are drained by reader threads so a chatty program cannot deadlock
/// against a full pipe buffer (the shape `tests/interpreter.rs` established;
/// `timeout(1)` is not available on Windows and this gate must run there).
fn run_node(path: &Path) -> Result<(String, i32), String> {
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
        let mut sink = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut sink);
    });
    let deadline = Instant::now() + NODE_TIMEOUT;
    let status = loop {
        match child.try_wait().map_err(|error| error.to_string())? {
            Some(status) => break Some(status),
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    };
    let stdout = String::from_utf8_lossy(&reading_stdout.join().unwrap_or_default()).into_owned();
    let _ = reading_stderr.join();
    match status {
        Some(status) => Ok((stdout, status.code().unwrap_or(-1))),
        None => Err(format!(
            "node did not exit within {}s (killed)",
            NODE_TIMEOUT.as_secs()
        )),
    }
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
fn build_both_ways(source: String) -> Result<Compared, String> {
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

/// Writes `javascript` to a uniquely named scratch file and runs it.
fn run(javascript: &str, label: &str) -> Result<(String, i32), String> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "vilan_infer_diff_{}_{unique}_{label}.js",
        std::process::id()
    ));
    std::fs::write(&path, javascript).map_err(|error| error.to_string())?;
    let outcome = run_node(&path);
    let _ = std::fs::remove_file(&path);
    outcome
}

#[test]
fn inference_is_observationally_neutral_over_the_corpus() {
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

    let programs: Vec<(String, PathBuf)> = paths
        .into_iter()
        .map(|path| {
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                path,
            )
        })
        .collect();

    // Each program is an independent compile-and-compare (corpus.rs's shape).
    let outcomes: Vec<(usize, usize, Vec<String>)> = std::thread::scope(|scope| {
        let workers: Vec<_> = programs
            .chunks(programs.len().div_ceil(8).max(1))
            .map(|chunk| {
                scope.spawn(move || {
                    let mut failures = Vec::new();
                    let mut changed = 0usize;
                    let mut ran = 0usize;
                    for (name, path) in chunk {
                        let source = std::fs::read_to_string(path).expect("read corpus file");
                        let compared = match build_both_ways(source) {
                            Ok(compared) => compared,
                            Err(error) => {
                                failures.push(format!("{name}: {error}"));
                                continue;
                            }
                        };
                        if compared.with == compared.without {
                            // The sweep folded nothing reachable here; there is
                            // no behaviour difference to look for.
                            continue;
                        }
                        changed += 1;
                        if NOT_RUN.iter().any(|(excluded, _)| excluded == name) {
                            continue;
                        }
                        let folded = run(&compared.with, "with");
                        let plain = run(&compared.without, "without");
                        ran += 1;
                        match (folded, plain) {
                            (Ok(folded), Ok(plain)) => {
                                if folded != plain {
                                    failures.push(format!(
                                        "{name}: FOLDING CHANGED BEHAVIOUR\n  \
                                         with the sweep:    exit {}, stdout {:?}\n  \
                                         without the sweep: exit {}, stdout {:?}",
                                        folded.1, folded.0, plain.1, plain.0
                                    ));
                                }
                            }
                            (folded, plain) => {
                                // A run that could not happen is only a failure
                                // if the two sides disagree about it.
                                if format!("{folded:?}") != format!("{plain:?}") {
                                    failures.push(format!(
                                        "{name}: one build ran and the other did not\n  \
                                         with: {folded:?}\n  without: {plain:?}"
                                    ));
                                }
                            }
                        }
                    }
                    (changed, ran, failures)
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("differential worker"))
            .collect()
    });

    let changed: usize = outcomes.iter().map(|(changed, ..)| changed).sum();
    let ran: usize = outcomes.iter().map(|(_, ran, _)| ran).sum();
    let failures: Vec<String> = outcomes
        .into_iter()
        .flat_map(|(_, _, failures)| failures)
        .collect();
    assert!(
        failures.is_empty(),
        "{} corpus program(s) are not neutral under inference:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // Non-vacuity. If the sweep ever stops folding, this gate would pass
    // instantly and prove nothing — so the number of programs it CHANGED is
    // part of the contract, not a statistic.
    assert!(
        changed >= 20,
        "inference changed only {changed} corpus program(s) — 29 of them at the \
         time this gate was written (const-eval.md §9.1), so at this level it is \
         close to vacuous and would pass in a fraction of the time while proving \
         nothing. Check the sweep still runs."
    );
    eprintln!("[infer differential] {changed} programs changed, {ran} run under node");
}
