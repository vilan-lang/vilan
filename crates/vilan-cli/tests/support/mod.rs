//! Shared harness helpers for the end-to-end CLI suites.
//!
//! Not a test target itself (cargo compiles only the top-level `tests/*.rs`);
//! each suite that needs it declares `mod support;`.
//!
//! Every declaring suite compiles the WHOLE module, so a helper only some of
//! them use is dead code in the rest — hence the module-wide allow. It is not
//! covering unused code: nothing here is unused by the file as a set.
#![allow(dead_code)]

pub mod boot;
pub mod ladder;
pub mod port;

/// The directory this test binary's scratch goes in — cargo's own
/// `CARGO_TARGET_TMPDIR` under `target/`, never `std::env::temp_dir()`
/// (tracker N82, swept by N86).
///
/// `/tmp` on the owner's machine is a 12 GiB tmpfs shared by every worktree,
/// and a full suite under nine lanes filled it: `No space left on device`
/// failures that are green on a re-run, which is a red indistinguishable from
/// a real one. `CARGO_TARGET_TMPDIR` is expanded where this module is
/// COMPILED, once per including suite, so two suites never share a root even
/// when they write the same file name.
pub fn scratch_root() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let _ = std::fs::create_dir_all(&root);
    root
}

/// A scratch DIRECTORY named `name` under [`scratch_root`], emptied first.
pub fn scratch_dir(name: &str) -> PathBuf {
    let directory = scratch_root().join(name);
    let _ = std::fs::remove_dir_all(&directory);
    directory
}

/// An IO failure on a scratch path, with the free space NAMED when the
/// filesystem is out of it — so an ENOSPC red arrives saying what it is
/// instead of as `Os { code: 28 }` inside a list of diagnostics (N82's guard).
pub fn storage_failure(path: &Path, error: &std::io::Error) -> String {
    let full = error.kind() == std::io::ErrorKind::StorageFull || error.raw_os_error() == Some(28);
    if !full {
        return format!("{}: {error}", path.display());
    }
    format!(
        "{}: {error} — the harness's scratch filesystem is FULL{}. This is \
         tracker N82's shape: the red is the disk's, not the compiler's. Free \
         space and re-run before believing it.",
        path.display(),
        scratch_free_space(path),
    )
}

/// What `df` says is left where `path` lives, as a parenthetical. Empty where
/// there is no `df` to ask, because a guard that cannot answer says nothing.
fn scratch_free_space(path: &Path) -> String {
    let Some(parent) = path.parent() else {
        return String::new();
    };
    let Ok(output) = std::process::Command::new("df")
        .arg("-Pk")
        .arg(parent)
        .output()
    else {
        return String::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let Some(line) = text.lines().nth(1) else {
        return String::new();
    };
    let fields: Vec<&str> = line.split_whitespace().collect();
    let (Some(available), Some(mount)) = (fields.get(3), fields.last()) else {
        return String::new();
    };
    let Ok(kib) = available.parse::<u64>() else {
        return String::new();
    };
    format!(" ({} MiB free on `{mount}`)", kib / 1024)
}

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// How long a `run --watch` harness waits for something that must eventually
/// happen — the dev channel's activation line, round 1's `dist/`, a rebuilt
/// bundle. It is a LIVENESS bound, not a performance assertion: no test in this
/// family claims how fast a build is, so this number only has to be too large
/// for a healthy round and finite for a hung one. A green run never pays it.
///
/// E39: it used to be 20 s, which under a loaded suite is not too large for a
/// healthy round. `run --watch`'s first round is a full compile of every leg —
/// a browser bundle over `std::ui` plus a server — and on a contended box that
/// alone runs past 20 s, so `hmr_swap` failed on the *machine's* speed while
/// asserting nothing about it. Same disease as E32's cancellation family
/// (compile inside the timed window), and the same cure: stop letting a slow
/// machine compete with a budget that was never measuring it.
/// `watch_lifecycle.rs` reached this conclusion first — "how long that takes is
/// not this test's business" — at 60 s.
///
/// The number is set from a measurement, not a feeling: one ordinary
/// `vilan build` of `hmr_swap`'s own two-leg project — the identical work round
/// 1 does — costs ~34 s wall on a 16-core box carrying a load average of ~38.
/// 120 s was tried first and is only ~3.5× that, thin enough that a box running
/// five overlapping suites consumed it. 300 s keeps ~9× headroom at that load
/// and is still finite, which is the whole job. A green run never pays any of
/// it: every wait returns the moment its condition holds.
pub const WATCH_LIVENESS: Duration = Duration::from_secs(300);

/// A budget for a watch round, expressed in units of the round this machine
/// just paid for. `first_round` is the measured cost of round 1 — a full
/// compile of every leg, here, now, under whatever load the box is under — so a
/// later round taking several times that is a stuck watcher, while the same
/// round on a machine four times slower is not.
///
/// This is E32's rule for a test that cannot move its compile out of the timed
/// window: the compile stays inside, but the budget is calibrated against it
/// instead of guessed, so it measures the PROGRAM (a rebuild reacting to an
/// edit) rather than the machine. The floor keeps a suspiciously fast round 1
/// from producing a hair-trigger budget; the ceiling keeps a pathological one
/// from producing an unbounded wait.
pub fn round_budget(first_round: Duration) -> Duration {
    (first_round * 4).clamp(Duration::from_secs(20), WATCH_LIVENESS)
}

/// A `vilan run`'s liveness bound, expressed in `reference_compile` units.
///
/// Measured on a 16-core box (E40, 2026-08-07), one `vilan build` each:
///
/// | project                            | idle    | load avg ~28 |
/// |------------------------------------|---------|--------------|
/// | the reference (`std::io::print` only)  | 227 ms  | 490 ms       |
/// | a node app importing `std::time`   | 6.07 s  | 11.4 s       |
/// | `vilan/benchmarks`, the heaviest   | 13.4 s  | 24.5 s       |
///
/// In reference units that is 27x idle / 23x loaded for the typical member and
/// **59x idle / 51x loaded** for the heaviest. The ratio barely moves across a
/// 60x range of compile weight, which is the fact that makes a cheap reference a
/// valid probe of an expensive compile: contention scales both by the same ~2x.
/// So the family's worst healthy compile is ~60 reference units, and 240 is
/// E39's 4x over it.
const RUN_LIVENESS_REFERENCE_UNITS: u32 = 240;

/// How long a harness waits for a whole `vilan run <project>` — a COMPILE plus
/// the emitted program — to exit on its own.
///
/// A liveness bound, not a performance assertion, and the same disease E32 and
/// E39 treated: `cancellation`, `benchmarks`, `owned_nursery`, `rpc_http`,
/// `split` and `streaming` each wrapped one of these in a bare literal (45 s,
/// 90 s, 20 s, 60 s), and every one of those literals is a *compile* budget
/// nobody wrote on purpose. The v0.32.0 CI run failed on two of them — on both
/// ubuntu and windows, each at exactly its ceiling — while 3044 of 3046 tests
/// passed: shared 4-core runners take ~54 min for a suite a 16-core dev box runs
/// in ~8.6, and under that interleave a fixed clock around a compile is a bet on
/// the runner, not on the program.
///
/// What each of those tests actually claims is pinned by its output — the abort
/// aborted, the counts are the counts, the chunks are in order — so this number
/// only has to be too large for a healthy run and finite for a hung one. Where a
/// test additionally claims something about *time*, that claim keeps its own
/// tight budget measured from the program's own scale (E32's rule); this bound
/// is never the thing under test.
///
/// The value is measured, not felt: `RUN_LIVENESS_REFERENCE_UNITS` above carries
/// the numbers. The clamp is E39's — the floor keeps a suspiciously fast
/// reference from producing a hair trigger (the heaviest member of the family
/// costs 13.4 s on an idle box, so 60 s is still 4x it), the ceiling keeps a
/// pathological one from producing an unbounded wait. A green run never pays any
/// of it: every wait returns the moment its process exits.
pub fn run_liveness() -> Duration {
    (reference_compile() * RUN_LIVENESS_REFERENCE_UNITS)
        .clamp(Duration::from_secs(60), WATCH_LIVENESS)
}

/// What one ordinary compile costs on THIS machine, right now — the unit the
/// bounds above are denominated in. Measured once, lazily, and only by a test
/// that needs it.
///
/// The reference project imports `std::io::print` and nothing else, which is what
/// keeps the probe cheap (~0.23 s idle) rather than representative in weight:
/// per the table on `RUN_LIVENESS_REFERENCE_UNITS`, a 60x heavier compile slows
/// down by the same factor under the same contention, so the cheap one measures
/// the machine just as well and the suite does not pay a real compile per test
/// to find that out. It is the CLI's own `build`, spawned exactly as the tests
/// spawn it, so it carries process startup, binary load and `std` analysis — the
/// costs that actually move under a loaded runner.
///
/// A measurement that fails or hangs yields the CEILING, never a small number: a
/// broken probe must not be able to manufacture a tight budget, and a bound that
/// is merely too generous fails only a test that was hung anyway.
pub fn reference_compile() -> Duration {
    static MEASURED: OnceLock<Duration> = OnceLock::new();
    *MEASURED.get_or_init(measure_reference_compile)
}

fn measure_reference_compile() -> Duration {
    let project = scratch_root().join(format!("vilan_reference_compile_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&project);
    let written = std::fs::create_dir_all(project.join("src")).and_then(|()| {
        std::fs::write(
            project.join("vilan.toml"),
            "[package]\nname = \"reference\"\ntarget = \"node\"\n",
        )?;
        std::fs::write(
            project.join("src/main.vl"),
            "import std::io::print;\n\nfun main() {\n\tprint(\"reference\");\n}\n",
        )
    });
    if written.is_err() {
        return WATCH_LIVENESS;
    }

    let started = Instant::now();
    let measured = spawn_reference_build(&project).and_then(|mut build| {
        // Bounded like everything else here: a compiler that never returns must
        // not take the measuring helper down with it, silently, inside a test
        // that is about something else.
        let deadline = started + WATCH_LIVENESS;
        loop {
            match build.try_wait() {
                Ok(Some(status)) if status.success() => return Some(started.elapsed()),
                Ok(Some(_)) => return None,
                Ok(None) if Instant::now() >= deadline => {
                    let _ = build.kill();
                    let _ = build.wait();
                    return None;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                Err(_) => return None,
            }
        }
    });
    let _ = std::fs::remove_dir_all(&project);
    measured.unwrap_or(WATCH_LIVENESS)
}

fn spawn_reference_build(project: &std::path::Path) -> Option<Child> {
    Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", project.to_str()?])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()
}

/// Kills a `run --watch` session, reaps it, and removes the temp script it
/// leaves behind. Best effort throughout — a teardown must never fail a test.
///
/// `Child::kill` is `SIGKILL`, which no handler can catch, so the session's
/// Ctrl-C hook (`main.rs::install_watch_interrupt_hook`) never runs and its
/// `vilan-watch-<pid>.mjs` outlives the run — ~4 leaked temp files per full
/// suite run (`windows-support.md` §12). The script is keyed by the CLI's pid,
/// which the harness holds, so the harness can clean up what it spawned. The
/// pid is read *before* the kill, so the path is right regardless of what
/// reaping does to the handle.
///
/// This is harness-only cleanup: the product's own teardown (the per-round
/// delete in `run_watch`, the Ctrl-C hook) is unchanged and is what a real
/// session relies on.
pub fn kill_watcher(watcher: &mut Child) {
    // NOT [`scratch_root`] (N86): this path is the BINARY's. `vilan run
    // --watch` writes its script to `env::temp_dir()` (`main.rs`'s
    // `watch_script_path`), and a harness that looks anywhere else waits for
    // a file nothing will write.
    let script = std::env::temp_dir().join(format!("vilan-watch-{}.mjs", watcher.id()));
    let _ = watcher.kill();
    let _ = watcher.wait();
    let _ = std::fs::remove_file(script);
}
