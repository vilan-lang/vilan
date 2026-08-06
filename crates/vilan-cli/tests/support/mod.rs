//! Shared harness helpers for the end-to-end CLI suites.
//!
//! Not a test target itself (cargo compiles only the top-level `tests/*.rs`);
//! each suite that needs it declares `mod support;`.
//!
//! Every declaring suite compiles the WHOLE module, so a helper only some of
//! them use is dead code in the rest — hence the module-wide allow. It is not
//! covering unused code: nothing here is unused by the file as a set.
#![allow(dead_code)]

use std::process::Child;
use std::time::Duration;

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
pub const WATCH_LIVENESS: Duration = Duration::from_secs(120);

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
