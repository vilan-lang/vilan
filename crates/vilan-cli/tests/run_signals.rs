//! E335: a signal sent to `vilan run` reaches the program it started.
//!
//! `vilan run` waited with `Command::status()` and answered no signal, so a
//! supervisor, an editor task or a harness that stopped it with `SIGTERM` left
//! the program running (a server holding its port, a program parked on stdin),
//! and a program killed by a signal was reported as exit 1. It now forwards
//! `SIGTERM`, `SIGHUP` and `SIGINT` to the child, waits for it, and reports a
//! signal death as a shell does (128 + the signal). Each pin runs on BOTH
//! backends: node, and the native binary `vilan run --backend rust` builds.
//!
//! Unix-gated: the pins deliver real signals with `kill(2)`. The Windows leg
//! is the Job object (`job.rs`), which takes the program when `vilan run` dies.

#![cfg(unix)]

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

mod support;

const BACKENDS: [Option<&str>; 2] = [None, Some("rust")];

/// A liveness bound, never a timing claim: the native leg's first step builds.
const STEP_BOUND: Duration = Duration::from_secs(300);

/// A program that says it is parked, then parks on `read_line` (the test holds
/// the stdin pipe open and never writes).
const PARKED: &str = r#"import std::io::print;
import std::option::Option::{ self, None, Some };
import std::process::read_line;

fun main() {
	print("parked");
	match read_line() {
		Some(let line) => print(i"read [{line}]"),
		None => print("read the end"),
	}
}
"#;

fn temp_project(tag: &str) -> PathBuf {
    let dir =
        support::scratch_root().join(format!("vilan_run_signals_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn spawn_run(dir: &Path, backend: Option<&str>) -> Child {
    let mut command = Command::new(env!("CARGO_BIN_EXE_vilan"));
    command.arg("run");
    if let Some(backend) = backend {
        command.args(["--backend", backend]);
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    command
        .arg("parked.vl")
        .current_dir(dir)
        .env("VILAN_STD", manifest.join("../../vilan/std"))
        .env("VILAN_RT", manifest.join("../vilan-rt"))
        .env(
            "CARGO_TARGET_DIR",
            support::scratch_root().join("process-native"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn vilan run")
}

/// The pids of `pid`'s direct children, read from /proc (Linux) across every
/// thread of it - the run spawns its program from a worker thread.
fn children_of(pid: u32) -> Vec<i32> {
    let mut found = Vec::new();
    let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) else {
        return found;
    };
    for task in tasks.flatten() {
        let text = std::fs::read_to_string(task.path().join("children")).unwrap_or_default();
        found.extend(
            text.split_whitespace()
                .filter_map(|token| token.parse::<i32>().ok()),
        );
    }
    found
}

fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only probes for existence.
    unsafe { libc::kill(pid, 0) == 0 }
}

/// Starts `vilan run` on the parked program, waits until the program has
/// printed, and answers the run, the program's pid and the stdin pipe (kept
/// open: the program must stay parked).
fn start_parked(tag: &str, backend: Option<&str>) -> (Child, i32, PathBuf) {
    let dir = temp_project(tag);
    std::fs::write(dir.join("parked.vl"), PARKED).unwrap();
    let mut run = spawn_run(&dir, backend);
    let mut stdout = run.stdout.take().expect("piped stdout");
    let (sender, printed) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buffer = [0u8; 256];
        let mut seen = String::new();
        while let Ok(count) = stdout.read(&mut buffer) {
            if count == 0 {
                break;
            }
            seen.push_str(&String::from_utf8_lossy(&buffer[..count]));
            if seen.contains("parked") {
                let _ = sender.send(());
                break;
            }
        }
        // Keep draining so the program never blocks on a full pipe.
        while matches!(stdout.read(&mut buffer), Ok(count) if count > 0) {}
    });
    if printed.recv_timeout(STEP_BOUND).is_err() {
        let _ = run.kill();
        let _ = run.wait();
        panic!("{backend:?}: the program never printed `parked`");
    }
    let children = children_of(run.id());
    if children.len() != 1 {
        // By PID: whatever children it has, then the run.
        for child in &children {
            // SAFETY: plain `kill` on pids this test spawned.
            unsafe {
                libc::kill(*child, libc::SIGKILL);
            }
        }
        let _ = run.kill();
        let _ = run.wait();
        panic!("{backend:?}: `vilan run` has exactly the program as a child, found {children:?}");
    }
    (run, children[0], dir)
}

/// Waits (bounded) for the run to exit and answers its status.
fn finish(run: &mut Child, program: i32, backend: Option<&str>) -> std::process::ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(status) = run.try_wait().expect("poll the run") {
            return status;
        }
        if Instant::now() >= deadline {
            // By PID: the program first, then the run.
            // SAFETY: plain `kill` on pids this test spawned.
            unsafe {
                libc::kill(program, libc::SIGKILL);
            }
            let _ = run.kill();
            let _ = run.wait();
            panic!("{backend:?}: `vilan run` did not exit after the signal");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_signal_sent_to_vilan_run_reaches_the_program_and_is_its_exit_status() {
    for (signal, number, name) in [
        (libc::SIGTERM, 15, "TERM"),
        (libc::SIGHUP, 1, "HUP"),
        (libc::SIGINT, 2, "INT"),
    ] {
        for backend in BACKENDS {
            let (mut run, program, dir) =
                start_parked(&format!("{name}_{}", backend.unwrap_or("js")), backend);
            assert!(alive(program), "{backend:?}: the program is parked");
            // To `vilan run` ALONE, as a supervisor sends it.
            // SAFETY: plain `kill` on the run this test spawned.
            unsafe {
                libc::kill(run.id() as i32, signal);
            }
            let status = finish(&mut run, program, backend);
            let gone = !alive(program);
            if !gone {
                // SAFETY: as above, a pid this test spawned.
                unsafe {
                    libc::kill(program, libc::SIGKILL);
                }
            }
            let _ = std::fs::remove_dir_all(&dir);
            assert!(
                gone,
                "SIG{name} on {backend:?}: the program outlived `vilan run`"
            );
            assert_eq!(
                status.code(),
                Some(128 + number),
                "SIG{name} on {backend:?}: `vilan run` reports the program's death by the signal as a shell does ({status:?})"
            );
        }
    }
}
