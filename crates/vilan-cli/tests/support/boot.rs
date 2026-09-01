//! Booting a built server to watch it STOP — the §10.7 "refuse to boot"
//! harness (proposal/fullstack-dx.md), shared by the suites that pin a
//! `ShellFault` refusal: `shell_check.rs` (a hand-authored shell against its
//! build) and `document.rs` (a generated document's `head`/`body` markup
//! against the same rules).

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// What a boot did.
pub struct Boot {
    /// The server was still running when the wait ran out — for every refusal
    /// pin that IS the failure: the check did not fire and the process took
    /// the port and the event loop with it.
    pub started: bool,
    /// It exited non-zero, which is what refusing to boot looks like.
    pub refused: bool,
    /// Everything it printed, stdout and stderr together.
    pub report: String,
}

/// Boot the built server from the project root and wait for it to STOP.
///
/// A server that refuses to boot exits on its own; one that (wrongly) started
/// holds the event loop for as long as anything lets it, so the wait is bounded
/// and a child still alive at the deadline is killed BY THE HARNESS and reported
/// as a started server rather than left to outlive the suite.
// The child IS reaped on both paths — the `try_wait` loop below reaps a server
// that exited on its own, and the `started` branch kills and `wait`s one that
// did not. `zombie_processes` cannot follow a `try_wait` through a loop, so it
// sees only the spawn.
#[allow(
    clippy::zombie_processes,
    reason = "reaped by the try_wait loop or by the kill/wait below"
)]
pub fn boot(staged: &Path) -> Boot {
    let log = staged.join("boot.log");
    let file = std::fs::File::create(&log).expect("create the boot log");
    let mut server: Child = Command::new("node")
        .arg("dist/server.mjs")
        .current_dir(staged)
        .stdout(Stdio::from(file.try_clone().expect("clone the log handle")))
        .stderr(Stdio::from(file))
        .spawn()
        .expect("spawn the server");

    let deadline = Instant::now() + super::run_liveness();
    let mut refused = false;
    let mut started = true;
    while Instant::now() < deadline {
        match server.try_wait() {
            Ok(Some(status)) => {
                started = false;
                refused = !status.success();
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(error) => panic!("wait for the server: {error}"),
        }
    }
    if started {
        let _ = server.kill();
        let _ = server.wait();
    }
    let report = std::fs::read_to_string(&log).unwrap_or_default();
    Boot {
        started,
        refused,
        report,
    }
}

/// The claim every fault pin makes: the server stopped, non-zero, saying so.
pub fn assert_refused(boot: &Boot, expected: &[&str]) {
    assert!(
        !boot.started,
        "the server STARTED over markup that does not match its build — the check did not fire:\n{}",
        boot.report
    );
    assert!(
        boot.refused,
        "a refused boot must exit non-zero:\n{}",
        boot.report
    );
    for needle in expected {
        assert!(
            boot.report.contains(needle),
            "the refusal should name {needle}:\n{}",
            boot.report
        );
    }
}
