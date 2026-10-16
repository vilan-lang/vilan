//! End-to-end: `std::process`'s surface against the real host process
//! (kolt.local 018) — these run the actual `process.*` bindings rather than
//! pinning generated JS text, the same posture as `fs.rs`. Each pin runs on
//! BOTH backends: the JS one under node, and the native one (`--backend rust`,
//! A173 S1 and F?4) as the binary `vilan run` builds.
//!
//! The stdin pins drive a `vilan run` child through a PIPE the test holds, and
//! every step waits on the child's own OUTPUT before the next write — a prompt
//! that has appeared, a tick that has printed — never on a sleep: the order
//! under test is the harness's sequencing, not the box's timing.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

mod support;

/// A fresh temp directory for the test's project tree.
fn temp_project(tag: &str) -> PathBuf {
    let dir = support::scratch_root().join(format!("vilan_process_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Writes `contents` to `dir/relative`, creating parent directories.
fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// The two backends a pin runs on: `None` is the default (JS under node).
const BACKENDS: [Option<&str>; 2] = [None, Some("rust")];

/// `vilan run [--backend <backend>] <relative>` inside `dir`, not yet started.
/// The native leg builds against this tree's `vilan-rt` and std into one cargo
/// target directory shared by this binary's tests (cargo locks it itself), so
/// the runtime compiles once.
fn vilan_run(dir: &Path, relative: &str, backend: Option<&str>) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_vilan"));
    command.arg("run");
    if let Some(backend) = backend {
        command.args(["--backend", backend]);
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    command
        .arg(relative)
        .current_dir(dir)
        .env("VILAN_STD", manifest.join("../../vilan/std"))
        .env("VILAN_RT", manifest.join("../vilan-rt"))
        .env(
            "CARGO_TARGET_DIR",
            support::scratch_root().join("process-native"),
        );
    command
}

/// Runs `vilan run <relative>` on `backend` inside `dir`, asserting success,
/// and returns stdout.
fn run_ok_on(dir: &Path, relative: &str, backend: Option<&str>) -> String {
    let output = vilan_run(dir, relative, backend)
        .stdin(Stdio::null())
        .output()
        .expect("run vilan");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "vilan run failed:\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
}

#[test]
fn cwd_reports_a_non_empty_absolute_path_to_the_directory_the_process_runs_in() {
    for backend in BACKENDS {
        cwd_reports_the_directory_on(backend);
    }
}

/// F?4: natively `cwd` was refused with F1's S1b message; it is
/// `std::env::current_dir` now, and answers what node answers.
fn cwd_reports_the_directory_on(backend: Option<&str>) {
    let dir = temp_project(&format!("cwd_{}", backend.unwrap_or("js")));
    write(
        &dir,
        "probe.vl",
        r#"import std::io::print;
import std::process::cwd;

fun main() {
	print(cwd());
}
main();
"#,
    );
    let stdout = run_ok_on(&dir, "probe.vl", backend);
    let reported = stdout.strip_suffix('\n').unwrap_or(&stdout);
    assert!(
        !reported.is_empty(),
        "{backend:?}: cwd() printed an empty path"
    );
    assert!(
        Path::new(reported).is_absolute(),
        "{backend:?}: cwd() should be an absolute path, was {reported:?}"
    );
    // The probe ran with the fixture directory as its working directory, and
    // the last path component survives any symlink canonicalization the host
    // applies to the prefix (`/tmp` vs `/private/tmp`).
    let basename = dir.file_name().unwrap().to_str().unwrap();
    assert!(
        reported.ends_with(basename),
        "{backend:?}: cwd() should end with the fixture directory `{basename}`, was {reported:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// A173 S1: stdin, read incrementally and awaited, on both backends.
// ---------------------------------------------------------------------------

/// How long a step waits for the child's next output before calling the run
/// HUNG. A liveness bound, never a timing claim: a healthy step answers as soon
/// as the child prints, and on a contended box the native leg's first step
/// includes a cargo build. The claims below are all "this happens at all, in
/// this order", so the only cost of the number is how long a red takes.
const STEP_BOUND: Duration = Duration::from_secs(300);

/// A `vilan run` child the test talks to: its stdin is the test's pipe, and
/// its stdout streams back through a reader thread, so a step can wait on the
/// text the program has printed so far.
struct Conversation {
    label: String,
    child: Child,
    stdin: Option<ChildStdin>,
    chunks: mpsc::Receiver<Vec<u8>>,
    stdout: String,
}

impl Conversation {
    fn start(dir: &Path, relative: &str, backend: Option<&str>) -> Conversation {
        let mut child = vilan_run(dir, relative, backend)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn vilan run");
        let mut stdout = child.stdout.take().expect("piped stdout");
        let (sender, chunks) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 4096];
            while let Ok(count) = stdout.read(&mut buffer) {
                if count == 0 || sender.send(buffer[..count].to_vec()).is_err() {
                    break;
                }
            }
        });
        Conversation {
            label: format!("{relative} on {}", backend.unwrap_or("js")),
            stdin: child.stdin.take(),
            child,
            chunks,
            stdout: String::new(),
        }
    }

    /// Waits until the program's stdout CONTAINS `text` — printed with or
    /// without a newline after it, which is the point for a prompt.
    fn wait_for(&mut self, text: &str) {
        let deadline = Instant::now() + STEP_BOUND;
        while !self.stdout.contains(text) {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.chunks.recv_timeout(left) {
                Ok(chunk) => self.stdout.push_str(&String::from_utf8_lossy(&chunk)),
                Err(_) => {
                    self.abandon();
                    panic!(
                        "{}: waited for {text:?} and the program printed only {:?}",
                        self.label, self.stdout
                    );
                }
            }
        }
    }

    fn write(&mut self, text: &str) {
        let stdin = self.stdin.as_mut().expect("stdin still open");
        stdin
            .write_all(text.as_bytes())
            .expect("write the child's stdin");
        stdin.flush().expect("flush the child's stdin");
    }

    /// Ends the program's input: its next read sees the end.
    fn close(&mut self) {
        self.stdin = None;
    }

    /// A red step's teardown: the input closes FIRST, because killing `vilan
    /// run` does not reach the program it started (node, or the native
    /// binary), and a program parked on stdin ends at its end of input.
    fn abandon(&mut self) {
        self.close();
        let _ = self.child.kill();
    }

    /// Waits for the program to EXIT — with the test's end of the pipe still
    /// open unless [`Conversation::close`] ran — and answers its exit code and
    /// everything it printed.
    fn finish(mut self) -> (Option<i32>, String) {
        let deadline = Instant::now() + STEP_BOUND;
        let status = loop {
            if let Some(status) = self.child.try_wait().expect("poll the child") {
                break status;
            }
            if Instant::now() >= deadline {
                self.abandon();
                panic!(
                    "{}: the program did not exit with its stdin pipe open; it printed {:?}",
                    self.label, self.stdout
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        while let Ok(chunk) = self.chunks.recv_timeout(STEP_BOUND) {
            self.stdout.push_str(&String::from_utf8_lossy(&chunk));
        }
        (status.code(), self.stdout)
    }
}

/// The interactive prompt (A173 S1, E?5): `prompt` writes its text with no
/// newline and FLUSHED — the test writes nothing until the text is on the
/// pipe, so a prompt held in a buffer never gets its answer — then reads the
/// line typed after it. At the end of input `prompt` is `None`, and the
/// program exits with its own code through `exit` (natively F?4's `exit`).
#[test]
fn a173_a_prompt_appears_before_its_answer_is_typed_and_the_end_is_none() {
    for backend in BACKENDS {
        let dir = temp_project(&format!("prompt_{}", backend.unwrap_or("js")));
        write(
            &dir,
            "prompt.vl",
            r#"import std::io::print;
import std::option::Option::{ self, None, Some };
import std::process::{ exit, prompt };

fun main() {
	match prompt("name? ") {
		Some(let name) => print(i"hello, {name}"),
		None => print("no name"),
	}
	match prompt("again? ") {
		Some(let name) => print(i"hello again, {name}"),
		None => {
			print("no answer");
			exit(3);
		}
	}
	print("exit returned");
}
"#,
        );
        let mut conversation = Conversation::start(&dir, "prompt.vl", backend);
        conversation.wait_for("name? ");
        assert_eq!(
            conversation.stdout, "name? ",
            "{backend:?}: the prompt is the whole output before the answer"
        );
        conversation.write("ann\n");
        conversation.wait_for("again? ");
        conversation.close();
        let (code, stdout) = conversation.finish();
        assert_eq!(
            stdout, "name? hello, ann\nagain? no answer\n",
            "{backend:?}"
        );
        assert_eq!(
            code,
            Some(3),
            "{backend:?}: `exit(3)` is the process's code"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The pump lets go of an idle stdin (A173 S1): a program that read the one
/// line it wanted EXITS when `main` ends, though the writer still holds the
/// pipe open. Without the JS pump's `unref` (or a native source that stays
/// live) the program waits for the writer to close — forever, here, since the
/// test closes it only after the exit. `main` ends on a plain statement on
/// purpose: a `void` main whose TAIL is a `match` is emitted with a
/// `process.exit` after it (the find filed beside A173 S1), which would end the
/// process whatever the pump held and make this pin vacuous on JS.
#[test]
fn a173_a_stdin_pipe_held_open_does_not_hold_the_process() {
    for backend in BACKENDS {
        let dir = temp_project(&format!("held_open_{}", backend.unwrap_or("js")));
        write(
            &dir,
            "one_line.vl",
            r#"import std::io::print;
import std::option::Option::{ self, None, Some };
import std::process::read_line;

fun main() {
	match read_line() {
		Some(let line) => print(i"read [{line}]"),
		None => print("read the end"),
	}
	print("main ends");
}
"#,
        );
        let mut conversation = Conversation::start(&dir, "one_line.vl", backend);
        conversation.write("one\ntwo\n");
        conversation.wait_for("main ends\n");
        let (code, stdout) = conversation.finish();
        assert_eq!(code, Some(0), "{backend:?}");
        assert_eq!(stdout, "read [one]\nmain ends\n", "{backend:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A read AWAITS (A173 S1): while `read_line` waits, the event loop keeps
/// turning — a sibling task's timer fires and prints three times, and only
/// then does the test type the line the read answers with. A blocking read
/// would stop the ticks until input arrived, and the test waits for the third
/// tick before it writes anything.
#[test]
fn a173_a_timer_keeps_ticking_while_a_read_waits() {
    for backend in BACKENDS {
        let dir = temp_project(&format!("ticking_{}", backend.unwrap_or("js")));
        write(
            &dir,
            "ticking.vl",
            r#"import std::io::print;
import std::option::Option::{ self, None, Some };
import std::process::read_line;
import std::task;
import std::time;

fun main() {
	task::nursery(|n| {
		async {
			for tick in [0, 1, 2] {
				time::sleep(10);
				print(i"tick {tick}");
			}
		};
		match read_line() {
			Some(let line) => print(i"read [{line}]"),
			None => print("read the end"),
		}
	});
}
"#,
        );
        let mut conversation = Conversation::start(&dir, "ticking.vl", backend);
        conversation.wait_for("tick 2\n");
        conversation.write("late\n");
        conversation.close();
        let (code, stdout) = conversation.finish();
        assert_eq!(code, Some(0), "{backend:?}");
        assert_eq!(
            stdout, "tick 0\ntick 1\ntick 2\nread [late]\n",
            "{backend:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The ambient cancel signal reaches a parked read (A173 S1): a child task
/// waiting on `read_line` inside a nursery is cancelled by its sibling, the
/// nursery joins, and control returns — with stdin silent and still open. The
/// read abandoned its WAIT, not stdin: the next read, outside the nursery, gets
/// the line the test types once the join is printed.
#[test]
fn a173_a_cancelled_read_returns_control_and_the_next_read_gets_the_line() {
    for backend in BACKENDS {
        let dir = temp_project(&format!("cancelled_{}", backend.unwrap_or("js")));
        write(
            &dir,
            "cancelled.vl",
            r#"import std::io::print;
import std::option::Option::{ self, None, Some };
import std::process::read_line;
import std::task;
import std::time;

fun main() {
	task::nursery(|n| {
		async {
			match read_line() {
				Some(let line) => print(i"the child read [{line}]"),
				None => print("the child read the end"),
			}
		};
		async {
			time::sleep(10);
			n.cancel();
		};
	});
	print("cancelled and joined");
	match read_line() {
		Some(let line) => print(i"next read: [{line}]"),
		None => print("next read: the end"),
	}
}
"#,
        );
        let mut conversation = Conversation::start(&dir, "cancelled.vl", backend);
        conversation.wait_for("cancelled and joined\n");
        conversation.write("kept\n");
        let (code, stdout) = conversation.finish();
        assert_eq!(code, Some(0), "{backend:?}");
        assert_eq!(
            stdout, "cancelled and joined\nnext read: [kept]\n",
            "{backend:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// F?4: `args()` natively — `process.argv.slice(2)`'s twin, the arguments
/// after the program, in order, with a hyphenated one passed through.
#[test]
fn f4_args_are_the_invocation_tail_on_both_backends() {
    for backend in BACKENDS {
        let dir = temp_project(&format!("args_{}", backend.unwrap_or("js")));
        write(
            &dir,
            "args.vl",
            r#"import std::io::print;
import std::process::args;

fun main() {
	let tail = args();
	print(tail.len());
	for argument in tail {
		print(i"[{argument}]");
	}
}
"#,
        );
        let output = vilan_run(&dir, "args.vl", backend)
            .args(["first", "--second", "with space"])
            .stdin(Stdio::null())
            .output()
            .expect("run vilan");
        assert!(
            output.status.success(),
            "{backend:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "3\n[first]\n[--second]\n[with space]\n",
            "{backend:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// F?4: `exit(code)` natively — the process ends at the call with `code`,
/// what was printed before it is out (flushed), and nothing after it runs.
#[test]
fn f4_exit_ends_the_process_with_its_code_on_both_backends() {
    for backend in BACKENDS {
        let dir = temp_project(&format!("exit_{}", backend.unwrap_or("js")));
        write(
            &dir,
            "exit.vl",
            r#"import std::io::print;
import std::process::exit;

fun main() {
	print("before");
	exit(4);
	print("after");
}
"#,
        );
        let output = vilan_run(&dir, "exit.vl", backend)
            .stdin(Stdio::null())
            .output()
            .expect("run vilan");
        assert_eq!(
            output.status.code(),
            Some(4),
            "{backend:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "before\n",
            "{backend:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
