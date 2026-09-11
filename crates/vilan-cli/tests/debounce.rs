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

/// Runs `vilan run <dir>` under a liveness bound and returns stdout, insisting
/// that the program reported NOTHING — which is the right default here: every
/// phase of the exhibit is a line in a sequence, and a report on stderr means
/// something failed that the sequence cannot see.
fn run_project(dir: &Path) -> String {
    let (stdout, stderr) = run_project_reporting(dir);
    assert!(
        stderr.is_empty(),
        "the debounce program wrote to stderr:\n{stderr}\n{stdout}"
    );
    stdout
}

/// The same run, handing BACK what the program reported (N71).
///
/// A free task that fails reports to the console and the program carries on —
/// that is the whole contract of a free task — so "this failure was reported
/// AND this loop survived" is one claim about two streams, and a runner that
/// asserts stderr empty can only ever see half of it. The seam is here rather
/// than in the emitted runtime deliberately: a hook inside `__task`'s reporting
/// path would change the bytes of every program that spawns a task, which is a
/// corpus-golden move for a test's convenience. What the runtime writes is
/// already observable; it was the harness that was throwing it away.
fn run_project_reporting(dir: &Path) -> (String, String) {
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
    (stdout, stderr)
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

/// B277: the driving loop runs under the AMBIENT NURSERY, so a nursery
/// cancellation unwinds it at its parked `wait`.
const DEBOUNCE_AFTER_A_NURSERY_CANCEL: &str = r#"import std::io::print;
import std::task::{ Nursery, nursery };
import std::time::{ Debounce, Duration, sleep };

async fun main() {
	let debounce = Debounce::new(Duration::millis(50));
	// The driving loop is spawned inside the nursery's extent, so the
	// cancellation below unwinds it where it is parked — mid-window, with a
	// deadline still pushed and nothing fired.
	nursery(|n: Nursery| {
		debounce.run(|| print("cancelled-never"));
		sleep(10);
		n.cancel();
	});
	print("mark-cancelled");

	// And the debounce is still a debounce.
	debounce.run(|| print("after-nursery-cancel"));
	sleep(1000);
	print("mark-done");
}
"#;

/// A cancelled nursery leaves the `Debounce` usable, not inert.
#[test]
fn b277_a_debounce_survives_the_cancellation_of_the_nursery_that_drove_it() {
    // B277. `run` sets `running` before spawning the loop and the loop cleared
    // it at its own end — an end a cancellation never reaches. The task
    // unwound with `running` still true, and every later `run` pushed a
    // deadline that nothing was reading: the value was INERT for the rest of
    // its life, silently. `after-nursery-cancel` is the line that disappeared.
    //
    // The fix is a `finally` around the loop rather than a nursery-cancellation
    // special case, because the flag must be cleared on EVERY unwind — a
    // callback of the app's own that throws killed the loop the same way.
    // `pending` and `timer` are left where they are: the next `run` opens a
    // fresh window over them, exactly as it does after `cancel()`.
    let dir = temp_project("nursery_cancel");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    write(&dir, "src/main.vl", DEBOUNCE_AFTER_A_NURSERY_CANCEL);
    let stdout = run_project(&dir);

    let lines: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(
        lines,
        // The cancelled window fires nothing — it was cancelled mid-wait.
        ["mark-cancelled", "after-nursery-cancel", "mark-done"],
        "a debounce must survive its nursery; got:\n{stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// B277's other half: the app's OWN callback throws.
const DEBOUNCE_AFTER_A_THROWING_CALLBACK: &str = r#"import std::io::{ print, panic };
import std::time::{ Debounce, Duration, sleep };

async fun main() {
	let debounce = Debounce::new(Duration::millis(50));
	// The fire calls this, and it throws: the driving loop unwinds at the
	// call site, exactly where a nursery cancellation unwinds it at the
	// parked wait. Nothing awaits the loop, so the failure takes the
	// free-task reporting path — the console — and the program carries on.
	debounce.run(|| panic("the callback exploded"));
	sleep(1000);
	print("mark-threw");

	// And the debounce is still a debounce.
	debounce.run(|| print("after-throw"));
	sleep(1000);
	print("mark-done");
}
"#;

/// A callback that throws leaves the `Debounce` usable, and says so on the way
/// out (N71: the seam that makes the second half assertable).
#[test]
fn b277_a_debounce_survives_a_callback_that_throws_and_the_failure_is_reported() {
    // B277's fix is a `finally` around the loop rather than a
    // nursery-cancellation special case, precisely because the flag must be
    // cleared on EVERY unwind. The nursery half has been pinned since the fix
    // landed; this is the half the harness could not see, because a free task
    // reports its failure to the console and the runner asserted stderr empty.
    //
    // Both halves of the claim are asserted here: the failure WAS reported
    // (unobserved free task, one line, naming the spawn origin), and the loop
    // SURVIVED it (`after-throw` fires from a fresh window over the same
    // value). Dropping either one would leave a green test over a dead
    // debounce or over a silently swallowed failure.
    let dir = temp_project("throwing_callback");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    write(&dir, "src/main.vl", DEBOUNCE_AFTER_A_THROWING_CALLBACK);
    let (stdout, stderr) = run_project_reporting(&dir);

    let lines: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(
        lines,
        // Nothing prints for the first window: its callback threw instead.
        ["mark-threw", "after-throw", "mark-done"],
        "a debounce must survive its own callback's failure; got:\n{stdout}\n{stderr}"
    );
    assert!(
        stderr.contains("unhandled task error") && stderr.contains("the callback exploded"),
        "the driving loop's failure must be REPORTED, not swallowed; \
         stderr was:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("unhandled task error").count(),
        1,
        "one failure, one report; stderr was:\n{stderr}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
