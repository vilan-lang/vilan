//! End-to-end cancellation over real IO (async-polymorphism.md Part B's open
//! pin): a node process serves an endpoint that never answers in time, a
//! nursery-spawned task `fetch`es it, and `n.cancel()` must abort the request
//! IN FLIGHT — the ambient `AbortSignal` riding `std::fetch` — so the join
//! returns promptly instead of waiting the server out. A broken bridge shows
//! up as the watchdog killing a hung run.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

mod support;

/// How long the join itself may take once the request is in flight — the ONE
/// claim in this file that is about time, and therefore the one budget measured
/// from the program's own scale rather than the machine's (E32's rule, E40's
/// pass).
///
/// The program sets both ends of that scale: the server sleeps 60 s before it
/// answers, and the client cancels 150 ms in. A join that returns promptly after
/// the abort finishes in well under a second; a join that waits the server out
/// takes 60. 20 s is two orders of magnitude above the first and a third of the
/// way to the second, so it stays non-vacuous while a loaded runner cannot
/// reach it — and, crucially, the window it bounds no longer contains the
/// compile (see `Run` below).
const JOIN_BUDGET: Duration = Duration::from_secs(20);

/// A fresh temp directory for the test's project tree.
fn temp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vilan_cancel_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Writes `contents` to `dir/relative`, creating parent directories.
fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// One `vilan run`: everything it printed, and WHEN each line arrived.
///
/// The arrival times are the point (E40). The program's own markers are the only
/// boundary that separates the compile from the run without moving the compile
/// out of the process, and this file's timing claim is about the run: `elapsed`
/// between two markers measures the emitted program, `elapsed` from spawn
/// measures whichever runner CI happened to schedule.
struct Run {
    stdout: String,
    arrivals: Vec<(Instant, String)>,
}

impl Run {
    /// When the first line containing `needle` arrived.
    fn arrival(&self, needle: &str) -> Option<Instant> {
        self.arrivals
            .iter()
            .find(|(_, line)| line.contains(needle))
            .map(|(at, _)| *at)
    }
}

/// Runs `vilan run <dir>` under a liveness bound, timestamping its stdout.
///
/// The bound is `support::run_liveness()`, not the 45 s literal that stood here
/// (E40): it wrapped a COMPILE plus the program, and it is what the v0.32.0 CI
/// run died on — at exactly 45.0 s, on both the ubuntu and the windows leg, in a
/// suite that otherwise passed 3044 of 3046. "The abort never landed" is not
/// what a timeout here proves and never was; the assertions below prove it.
fn vilan_run_with_liveness_bound(dir: &Path) -> Run {
    let liveness = support::run_liveness();
    let mut child = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", dir.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn vilan run");

    // The node child inherits this pipe, so its `print`s land here as they
    // happen — which is what makes "the server is up" and "the join returned"
    // events with times rather than two strings in a buffer.
    let stdout = child.stdout.take().unwrap();
    let (sender, received) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = sender.send((Instant::now(), line));
        }
    });

    let deadline = Instant::now() + liveness;
    loop {
        match child.try_wait().expect("poll vilan run") {
            Some(_status) => break,
            None if Instant::now() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "the build+run did not exit within {liveness:?} (a liveness bound, {:?} \
                     per reference compile on this machine)",
                    support::reference_compile()
                );
            }
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    }
    let _ = reader.join();
    let arrivals: Vec<(Instant, String)> = received.try_iter().collect();

    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    let stdout: String = arrivals
        .iter()
        .map(|(_, line)| line.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        stderr.trim().is_empty(),
        "vilan run wrote to stderr:\n{stderr}\nstdout:\n{stdout}"
    );
    Run { stdout, arrivals }
}

#[test]
fn cancel_aborts_an_in_flight_fetch() {
    let dir = temp_project("fetch_abort");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    write(
        &dir,
        "src/main.vl",
        &r#"import std::io::print;
import std::process::exit;
import std::time::sleep;
import std::task::nursery;
import std::fetch::fetch;
import std::http::{ Server, Response };

fun main() {
	// Port 0: the OS picks a free port and the ready callback reports it.
	Server::builder()
		.port(0)
		.on_request(|request| {
			// Never answers within the test window: only an in-flight abort
			// lets the client finish.
			sleep(60000);
			Response::builder().body("too late").build()
		})
		.on_start(|server| {
			// The run's own starting gun: the compile is over, the server is
			// listening, and everything the timing claim below is about happens
			// after this line.
			print("server-up");
			run_client(server.port());
		})
		.build()
		.start();
}

fun fetch_hanging(port: i32): i32 {
	let response = fetch(i"http://localhost:{port}/hang");
	print("unreachable-response");
	response.status()
}

fun run_client(port: i32) {
	nursery(|n| {
		let _ = async fetch_hanging(port);
		sleep(150);   // let the request get in flight
		n.cancel();   // aborts it via the ambient signal
		0
	});
	print("aborted-fast");
	exit(0);
}
"#,
    );
    let run = vilan_run_with_liveness_bound(&dir);
    assert!(
        run.stdout.contains("aborted-fast"),
        "the nursery never returned:\n{}",
        run.stdout
    );
    assert!(
        !run.stdout.contains("unreachable-response"),
        "the fetch completed instead of aborting:\n{}",
        run.stdout
    );

    // The promptness claim, over the PROGRAM's window: server up → join
    // returned. The 30 s that stood here started at `vilan run`'s spawn, so it
    // was a compile budget that happened to be spelled as a latency assertion —
    // and, being tighter than the watchdog around it, the first thing a loaded
    // box would break. Between these two markers there is no compiler.
    let served = run
        .arrival("server-up")
        .expect("the program should announce its server before the client runs");
    let aborted = run
        .arrival("aborted-fast")
        .expect("the program should announce the join returning");
    let join = aborted.duration_since(served);
    assert!(
        join < JOIN_BUDGET,
        "the join should return promptly after the abort; it took {join:?} of the \
         {JOIN_BUDGET:?} the program's own scale allows (the server answers at 60s, \
         the client cancels at 150ms)"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
