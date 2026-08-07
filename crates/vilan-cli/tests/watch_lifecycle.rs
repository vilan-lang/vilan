//! End-to-end pins for how a `--watch` session *ends* (`windows-support.md` §6).
//!
//! `watch_loop` never returns from its loop, so `Ctrl-C` is the only way a watch
//! session finishes — and before this slice that path ran no cleanup at all,
//! leaking one `vilan-watch-<pid>.js` per session into the temp directory (S3
//! delivered only the per-*round* delete, which covers restarts).
//!
//! Unix-gated because the pin has to deliver a real interrupt: `SIGINT` via
//! `kill(1)`. The Windows leg of the same handler (`SetConsoleCtrlHandler`, via
//! `ctrlc`) is exercised by a live `Ctrl-C` in S6's Windows pass — a console
//! control event cannot be raised at another process from a test.

#![cfg(unix)]

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

mod support;

/// A single-package project whose program prints once and exits, so the round's
/// `node` child is gone before the interrupt lands: what remains on disk is
/// exactly the temp script, with no orphan to clean up after the assertion.
fn temp_package(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vilan_watch_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("vilan.toml"), "[package]\nname = \"app\"\n").unwrap();
    std::fs::write(
        dir.join("src/main.vl"),
        "import std::print;\n\nfun main() {\n\tprint(\"round\");\n}\n",
    )
    .unwrap();
    dir
}

/// Waits (bounded) for `condition`, polling — the watcher compiles before it
/// writes anything, and how long that takes is not this test's business.
///
/// This file reached that conclusion before the rest of the family did, and E39
/// cited its comment while setting `WATCH_LIVENESS`; E40 finishes the trade by
/// giving it the shared bound, because 60 s is still a number about the machine
/// sitting in a test that is about `Ctrl-C`.
fn wait_for(label: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + support::WATCH_LIVENESS;
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("timed out waiting for {label}");
}

#[test]
fn ctrl_c_removes_the_watch_script_and_exits_130() {
    let dir = temp_package("ctrlc");
    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", "--watch", dir.to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the watcher");

    // The round's temp script is keyed by the watcher's pid.
    let script = std::env::temp_dir().join(format!("vilan-watch-{}.js", watcher.id()));
    wait_for("the watch script to be written", || script.exists());

    let signalled = Command::new("kill")
        .args(["-INT", &watcher.id().to_string()])
        .status()
        .expect("send SIGINT");
    assert!(signalled.success(), "kill -INT must succeed");

    let status = watcher.wait().expect("reap the watcher");
    let _ = std::fs::remove_dir_all(&dir);

    // Exiting *with a code* is itself the discriminator: without the handler
    // the default SIGINT disposition terminates the process by signal, so
    // `code()` would be `None` — and the script would still be on disk.
    assert_eq!(
        status.code(),
        Some(130),
        "a Ctrl-C'd watch session exits 128 + SIGINT"
    );
    assert!(
        !script.exists(),
        "the session's temp script must not outlive it: {}",
        script.display()
    );
}

/// A9: the `[build] run` hooks belong to the *round*, not to the session — a
/// watch that rebuilds re-runs them, so a Tailwind bridge regenerates its CSS on
/// every edit rather than once at startup. The hook appends a line per round, so
/// the count is the observation.
#[test]
fn build_hooks_run_once_per_watch_round() {
    let dir = temp_package("hooks");
    std::fs::write(
        dir.join("vilan.toml"),
        "[package]\nname = \"app\"\n\n[build]\nrun = [\"echo round >> rounds.txt\"]\n",
    )
    .unwrap();
    let rounds = dir.join("rounds.txt");

    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", "--watch", dir.to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the watcher");

    let lines = || {
        std::fs::read_to_string(&rounds)
            .map(|text| text.lines().count())
            .unwrap_or(0)
    };
    wait_for("the first round's hook", || lines() >= 1);

    // A source edit starts a second round, which must run the hook again.
    std::fs::write(
        dir.join("src/main.vl"),
        "import std::print;\n\nfun main() {\n\tprint(\"round two\");\n}\n",
    )
    .unwrap();
    wait_for("the second round's hook", || lines() >= 2);

    let _ = Command::new("kill")
        .args(["-INT", &watcher.id().to_string()])
        .status();
    let _ = watcher.wait();
    let _ = std::fs::remove_dir_all(&dir);
}
