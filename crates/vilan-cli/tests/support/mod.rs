//! Shared harness helpers for the end-to-end CLI suites.
//!
//! Not a test target itself (cargo compiles only the top-level `tests/*.rs`);
//! each suite that needs it declares `mod support;`.

use std::process::Child;

/// Kills a `run --watch` session, reaps it, and removes the temp script it
/// leaves behind. Best effort throughout — a teardown must never fail a test.
///
/// `Child::kill` is `SIGKILL`, which no handler can catch, so the session's
/// Ctrl-C hook (`main.rs::install_watch_interrupt_hook`) never runs and its
/// `vilan-watch-<pid>.js` outlives the run — ~4 leaked temp files per full
/// suite run (`windows-support.md` §12). The script is keyed by the CLI's pid,
/// which the harness holds, so the harness can clean up what it spawned. The
/// pid is read *before* the kill, so the path is right regardless of what
/// reaping does to the handle.
///
/// This is harness-only cleanup: the product's own teardown (the per-round
/// delete in `run_watch`, the Ctrl-C hook) is unchanged and is what a real
/// session relies on.
pub fn kill_watcher(watcher: &mut Child) {
    let script = std::env::temp_dir().join(format!("vilan-watch-{}.js", watcher.id()));
    let _ = watcher.kill();
    let _ = watcher.wait();
    let _ = std::fs::remove_file(script);
}
