//! `std::time::Debounce`'s runtime gate (tracker A63).
//!
//! A debounce is a mechanism, not a number: at most one timer in flight however
//! many times `run` is called, each call pushing the deadline out AND replacing
//! the callback, and the loop re-reading the deadline after every wait so a call
//! that arrived during a wait cannot fire early. It runs the LAST callback, not
//! the first — trailing edge. None of that is visible in a compile, so this is
//! an e2e leg: a node-target program built and run by the real CLI, printing a
//! marker per phase.
//!
//! **The assertion is the exact printed sequence**, which makes every negative
//! half an assertion too — a burst that fired three times, or fired the first
//! callback, or a cancel that fired anything, all show as a line in the wrong
//! place rather than as a missing `assert!`.
//!
//! **On load (E32).** Every burst here is dispatched in ONE tick — the calls are
//! back-to-back statements, so no scheduling delay can separate them — and every
//! wait for a fire is far longer than the window it is waiting on, which is the
//! safe direction: a stretched sleep only ever waits longer. The one phase that
//! measures a gap uses a 2 s window and a 100 ms gap, so it would take a 20x
//! overshoot of that sleep to split the window.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

mod support;

/// A fresh temp directory for the test's project tree.
fn temp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vilan_debounce_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// Runs `vilan run <dir>` under a liveness bound and returns stdout. The bound
/// is `support::run_liveness()` and not a literal for E40's reason: it wraps a
/// compile plus a program, and nothing here claims a speed.
fn run_project(dir: &Path) -> String {
    let liveness = support::run_liveness() + Duration::from_secs(30);
    let mut child = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", dir.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn vilan run");
    let deadline = Instant::now() + liveness;
    loop {
        match child.try_wait().expect("poll vilan run") {
            Some(_status) => break,
            None if Instant::now() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("the build+run did not exit within {liveness:?}");
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    let mut stdout = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut stdout)
        .unwrap();
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(
        stderr.is_empty(),
        "the debounce program wrote to stderr:\n{stderr}\n{stdout}"
    );
    stdout
}

/// Seven phases, each closed by a marker so a misplaced fire is nameable.
const DEBOUNCE_EXHIBIT: &str = r#"import std::io::print;
import std::time::{ Debounce, Duration, sleep };

async fun main() {
	// 1. A burst in ONE tick fires once, with the LAST callback.
	let burst = Debounce::new(Duration::millis(50));
	burst.run(|| print("burst-first"));
	burst.run(|| print("burst-second"));
	burst.run(|| print("burst-third"));
	sleep(1000);
	print("mark-a");

	// 2. A call after the window has closed opens a fresh one.
	burst.run(|| print("burst-again"));
	sleep(1000);
	print("mark-b");

	// 3. A call DURING the wait pushes the deadline out: two calls 100ms apart
	//    inside a 2s window are ONE fire, of the second callback. The timer
	//    armed for the first call fires early and the loop takes another turn.
	let pushed = Debounce::new(Duration::seconds(2));
	pushed.run(|| print("pushed-early"));
	sleep(100);
	pushed.run(|| print("pushed-late"));
	sleep(4000);
	print("mark-c");

	// 4. `cancel` fires nothing.
	let cancelled = Debounce::new(Duration::millis(50));
	cancelled.run(|| print("cancelled-never"));
	cancelled.cancel();
	sleep(1000);
	print("mark-d");

	// 5. …and the debounce is reusable afterwards.
	cancelled.run(|| print("after-cancel"));
	sleep(1000);
	print("mark-e");

	// 6. A cancel and a run in the SAME tick: the still-parked loop takes the
	//    new deadline, and no second loop starts to fire it twice.
	let racy = Debounce::new(Duration::millis(50));
	racy.run(|| print("racy-dropped"));
	racy.cancel();
	racy.run(|| print("racy-kept"));
	sleep(1000);
	print("mark-f");

	// 7. Copies share the ONE debounce, as copies of a `Timer` share one timer.
	let shared = Debounce::new(Duration::millis(50));
	let alias = shared;
	shared.run(|| print("alias-dropped"));
	alias.run(|| print("alias-kept"));
	sleep(1000);
	print("mark-g");
}
"#;

/// Trailing edge, the deadline pushed out per call, the callback replaced, and
/// `cancel` firing nothing — as one exact sequence.
#[test]
fn a_burst_fires_once_with_the_last_callback_and_cancel_fires_nothing() {
    let dir = temp_project("exhibit");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    write(&dir, "src/main.vl", DEBOUNCE_EXHIBIT);
    let stdout = run_project(&dir);

    let lines: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let expected = [
        // One fire for three calls, and it is the third callback.
        "burst-third",
        "mark-a",
        // A fresh window after the previous one closed.
        "burst-again",
        "mark-b",
        // The pushed deadline: one fire, the second callback.
        "pushed-late",
        "mark-c",
        // A cancel fires NOTHING — no line before this marker.
        "mark-d",
        // …and the debounce still works.
        "after-cancel",
        "mark-e",
        // A cancel and a run in one tick: one fire, the run's callback.
        "racy-kept",
        "mark-f",
        // Two handles, one debounce: one fire, the last callback.
        "alias-kept",
        "mark-g",
    ];
    assert_eq!(
        lines, expected,
        "the debounce exhibit printed the wrong sequence; got:\n{stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
