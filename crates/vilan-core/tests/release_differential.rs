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
///
/// A LIVENESS bound, not a performance assertion — this gate's claim is that
/// the two presets print the same thing, never that either prints it quickly —
/// so the number only has to be too large for a healthy program and finite for
/// a hung one. A green run never pays it: the loop breaks the moment the child
/// exits.
///
/// It was 30 s, and 30 s is not too large for a healthy corpus program on a
/// contended box: this binary was one of the >120 s members of two sibling
/// lanes' unions under ten-lane load (tracker N46), which is the same disease
/// E39/E40 treated across the watch family — a fixed clock around real work,
/// measuring the runner rather than the program. 300 s is the value that family
/// settled on and the reasoning transfers unchanged.
const NODE_TIMEOUT: Duration = Duration::from_secs(300);

/// Corpus programs this gate does not run, with the reason — `infer_differential.
/// rs`'s list plus one: their output is not a function of their source alone, so
/// "both builds printed the same thing" is not a claim either build can make.
/// They are still COMPILED both ways below, which is itself worth having; only
/// the node leg is skipped.
const NOT_RUN: &[(&str, &str)] = &[
    ("time.vl", "host clock: two runs print different timestamps"),
    ("crypto.vl", "host WebCrypto: a fresh random draw per run"),
    ("db.vl", "host database: touches the filesystem"),
    ("process-env.vl", "reads the host environment and argv"),
    // Sequential runs agree; the divergence appears only under this gate's own
    // 8-way parallelism, which is the proof that it is scheduling and not
    // codegen. E32's rule applies — a wall-clock margin is not an assertion.
    (
        "nursery.vl",
        "host timers 10ms apart decide the print order, which load can reorder",
    ),
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
        let mut bytes = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut bytes);
        bytes
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
    let stderr = String::from_utf8_lossy(&reading_stderr.join().unwrap_or_default()).into_owned();
    match status {
        // A failing build's stderr is the whole diagnosis (`SyntaxError:
        // Identifier 'b' has already been declared`), so it rides along with the
        // exit code rather than being dropped.
        Some(status) if !status.success() => Ok((
            format!("{stdout}--- stderr ---\n{stderr}"),
            status.code().unwrap_or(-1),
        )),
        Some(status) => Ok((stdout, status.code().unwrap_or(-1))),
        None => Err(format!(
            "node did not exit within {}s (killed)",
            NODE_TIMEOUT.as_secs()
        )),
    }
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

/// Writes `javascript` to a uniquely named scratch file and runs it.
fn run(javascript: &str, label: &str) -> Result<(String, i32), String> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "vilan_release_diff_{}_{unique}_{label}.mjs",
        std::process::id()
    ));
    std::fs::write(&path, javascript).map_err(|error| error.to_string())?;
    let outcome = run_node(&path);
    let _ = std::fs::remove_file(&path);
    outcome
}

#[test]
fn the_release_preset_is_observationally_neutral_over_the_corpus() {
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

    // Each program is an independent compile-and-compare, in the corpus gate's
    // 8-way shape — the leg runs the whole corpus under node TWICE, so the
    // parallelism is what keeps it affordable.
    let outcomes: Vec<(usize, Vec<String>)> = std::thread::scope(|scope| {
        let workers: Vec<_> = programs
            .chunks(programs.len().div_ceil(8).max(1))
            .map(|chunk| {
                let corpus = &corpus;
                scope.spawn(move || {
                    let mut failures = Vec::new();
                    let mut ran = 0usize;
                    for (name, path) in chunk {
                        let source = std::fs::read_to_string(path).expect("read corpus file");
                        let compared = match build_both_ways(source, corpus.clone()) {
                            Ok(compared) => compared,
                            Err(error) => {
                                failures.push(format!("{name}: {error}"));
                                continue;
                            }
                        };
                        if NOT_RUN.iter().any(|(excluded, _)| excluded == name) {
                            continue;
                        }
                        let release = run(&compared.release, "release");
                        let debug = run(&compared.debug, "debug");
                        ran += 1;
                        match (release, debug) {
                            (Ok(release), Ok(debug)) => {
                                if release != debug {
                                    failures.push(format!(
                                        "{name}: THE RELEASE PRESET CHANGED BEHAVIOUR\n  \
                                         release: exit {}, stdout {:?}\n  \
                                         debug:   exit {}, stdout {:?}\n  \
                                         release emitted:\n{}",
                                        release.1, release.0, debug.1, debug.0, compared.release
                                    ));
                                }
                            }
                            (release, debug) => {
                                // A run that could not happen is only a failure
                                // if the two sides disagree about it.
                                if format!("{release:?}") != format!("{debug:?}") {
                                    failures.push(format!(
                                        "{name}: one build ran and the other did not\n  \
                                         release: {release:?}\n  debug: {debug:?}"
                                    ));
                                }
                            }
                        }
                    }
                    (ran, failures)
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("differential worker"))
            .collect()
    });

    let ran: usize = outcomes.iter().map(|(ran, _)| ran).sum();
    let failures: Vec<String> = outcomes
        .into_iter()
        .flat_map(|(_, failures)| failures)
        .collect();
    assert!(
        failures.is_empty(),
        "{} corpus program(s) do not survive the release preset:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // Non-vacuity. Every program that compiles must have been RUN — this gate
    // has no "nothing changed, nothing to run" shortcut to hide behind, so a
    // collapse in this number means the corpus stopped being reached, not that
    // release got quieter.
    assert!(
        ran >= 60,
        "only {ran} corpus program(s) reached node — the gate is close to \
         vacuous. Every corpus program that compiles should run both ways."
    );
    eprintln!("[release differential] {ran} programs run under both presets");
}
