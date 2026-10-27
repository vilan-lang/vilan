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

    fn write(&mut self, bytes: impl AsRef<[u8]>) {
        let stdin = self.stdin.as_mut().expect("stdin still open");
        stdin
            .write_all(bytes.as_ref())
            .expect("write the child's stdin");
        stdin.flush().expect("flush the child's stdin");
    }

    /// Waits until `text` has appeared `count` MORE times than it had when
    /// called — the program's own clock, read off its output.
    fn wait_for_more(&mut self, text: &str, count: usize) {
        let target = self.stdout.matches(text).count() + count;
        let deadline = Instant::now() + STEP_BOUND;
        while self.stdout.matches(text).count() < target {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.chunks.recv_timeout(left) {
                Ok(chunk) => self.stdout.push_str(&String::from_utf8_lossy(&chunk)),
                Err(_) => {
                    self.abandon();
                    panic!(
                        "{}: waited for {count} more {text:?} and the program printed only {:?}",
                        self.label, self.stdout
                    );
                }
            }
        }
    }

    /// Ends the program's input: its next read sees the end.
    fn close(&mut self) {
        self.stdin = None;
    }

    /// A red step's teardown: the input closes FIRST, because a SIGKILL of `vilan
    /// run` cannot be forwarded to the program it started (node, or the native
    /// binary; E335 forwards the catchable signals), and a program parked on stdin ends at its end of input.
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

// ---------------------------------------------------------------------------
// A173 S2: the key stream — `process::keys()`, `Keys`, the std `KeyDecoder`.
// ---------------------------------------------------------------------------

/// A key program that prints every press while a sibling task ticks: the
/// ticks are the program's own clock, which is how the session below proves
/// time passed between two writes without a sleep in the harness.
const TICKING_KEYS: &str = r#"import std::debug::Debug;
import std::io::print;
import std::option::Option::{ self, None, Some };
import std::process::keys;
import std::task;
import std::time;

fun main() {
	task::nursery(|n| {
		async {
			mut tick = 0;
			for {
				time::sleep(20);
				tick += 1;
				print(i"tick {tick}");
			}
		};
		async {
			let keys = keys();
			print("ready");
			for {
				match keys.next_press() {
					Some(let press) => print(press.debug()),
					None => jump break,
				}
			}
			print("keys ended");
			n.cancel();
		};
	});
	print("joined");
}
"#;

/// The scripted key session (A173 S2, terminal.md §4.4): one byte script,
/// written in pieces through a PIPE, decodes to the same presses on both
/// backends. On a pipe there is no escape timeout (§4.3): an ESC written
/// alone and followed — after the program has ticked three times, far past a
/// terminal's 50 ms — by `[A` is ONE `Up`, never `Escape` and two characters.
/// A modifier parameter split mid-sequence, a scalar split between its bytes,
/// `\r` and `\n` in two writes (one Enter), and a paste whose closing bracket
/// comes in a later write each decode whole. The closed pipe resolves the last
/// lone ESC to `Escape` and ends the stream (`None`), and the loop's ticker
/// is cancelled by the key task.
#[test]
fn a173_s2_a_scripted_key_session_decodes_identically_over_a_pipe() {
    for backend in BACKENDS {
        let dir = temp_project(&format!("key_session_{}", backend.unwrap_or("js")));
        write(&dir, "key_session.vl", TICKING_KEYS);
        let mut conversation = Conversation::start(&dir, "key_session.vl", backend);
        conversation.wait_for("ready\n");
        conversation.write(b"\x1b");
        conversation.wait_for_more("tick", 3);
        conversation.write(b"[A");
        conversation.wait_for("key = Key::Up");
        conversation.write(b"\x1b[1;5");
        conversation.wait_for_more("tick", 1);
        conversation.write(b"C");
        conversation.write(&"é".as_bytes()[..1]);
        conversation.wait_for_more("tick", 1);
        conversation.write(&"é".as_bytes()[1..]);
        conversation.write(b"\r");
        conversation.wait_for("key = Key::Enter");
        conversation.write(b"\n\x1b[200~a\nb");
        conversation.wait_for_more("tick", 1);
        conversation.write(b"\x1b[201~\x1b");
        conversation.wait_for("Key::Paste");
        conversation.close();
        let (code, stdout) = conversation.finish();
        assert_eq!(code, Some(0), "{backend:?}: {stdout}");
        let keys: Vec<&str> = stdout
            .lines()
            .filter(|line| !line.starts_with("tick "))
            .collect();
        assert_eq!(
            keys,
            [
                "ready",
                "Press { key = Key::Up, ctrl = false, alt = false, shift = false }",
                "Press { key = Key::Right, ctrl = true, alt = false, shift = false }",
                "Press { key = Key::Char(\"\u{e9}\"), ctrl = false, alt = false, shift = false }",
                "Press { key = Key::Enter, ctrl = false, alt = false, shift = false }",
                "Press { key = Key::Paste(\"a\\nb\"), ctrl = false, alt = false, shift = false }",
                "Press { key = Key::Escape, ctrl = false, alt = false, shift = false }",
                "keys ended",
                "joined",
            ],
            "{backend:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// One `Keys` at a time, and its `Drop` is what frees the terminal (A173 S2,
/// terminal.md §3.3): an explicit `drop` lets the next `keys()` open, and so
/// does a `Keys` whose task a nursery CANCELLED while it was parked on a
/// silent, open stdin — the cancellation unwinds the task and runs the
/// `Drop`. Over a pipe, where no terminal mode exists to observe, the
/// one-at-a-time rule is the observable: a `Keys` that did not drop would
/// make the next `keys()` panic.
#[test]
fn a173_s2_a_dropped_or_cancelled_keys_frees_the_next_one() {
    for backend in BACKENDS {
        let dir = temp_project(&format!("keys_drop_{}", backend.unwrap_or("js")));
        write(
            &dir,
            "keys_drop.vl",
            r#"import std::drop::drop;
import std::io::print;
import std::option::Option::{ self, None, Some };
import std::process::keys;
import std::task;
import std::time;

fun main() {
	let first = keys();
	drop(first);
	let second = keys();
	print("opened after an explicit drop");
	drop(second);
	task::nursery(|n| {
		async {
			let held = keys();
			print("held");
			match held.next() {
				Some(let key) => print("the child read a key"),
				None => print("the child read the end"),
			}
		};
		async {
			time::sleep(10);
			n.cancel();
		};
	});
	let third = keys();
	print("opened after a cancelled task's keys");
}
"#,
        );
        let mut conversation = Conversation::start(&dir, "keys_drop.vl", backend);
        conversation.wait_for("opened after a cancelled task's keys\n");
        let (code, stdout) = conversation.finish();
        assert_eq!(code, Some(0), "{backend:?}");
        assert_eq!(
            stdout, "opened after an explicit drop\nheld\nopened after a cancelled task's keys\n",
            "{backend:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A second `keys()` while one lives PANICS, naming why (A173 S2, terminal.md
/// §3.3): two owners of one terminal mode cannot both restore it.
#[test]
fn a173_s2_a_second_live_keys_panics() {
    for backend in BACKENDS {
        let dir = temp_project(&format!("keys_twice_{}", backend.unwrap_or("js")));
        write(
            &dir,
            "keys_twice.vl",
            r#"import std::io::print;
import std::process::keys;

fun main() {
	let first = keys();
	print("first");
	let second = keys();
	print("unreachable");
	first.next();
	second.next();
}
"#,
        );
        let output = vilan_run(&dir, "keys_twice.vl", backend)
            .stdin(Stdio::null())
            .output()
            .expect("run vilan");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "{backend:?}: {stderr}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "first\n",
            "{backend:?}"
        );
        assert!(
            stderr.contains("a `Keys` is already live"),
            "{backend:?}: {stderr}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The line read and the key read share ONE stdin pump (A173 S2): a `prompt`
/// answered with a line, then a `Keys` over the rest — no byte is lost or
/// read twice between the two, and S1's line rules hold unchanged (`\r\n` one
/// ending; the `\n` after a `\r` belongs to the line, not to the keys).
#[test]
fn a173_s2_a_line_read_then_a_key_read_share_the_pump() {
    for backend in BACKENDS {
        let dir = temp_project(&format!("line_then_keys_{}", backend.unwrap_or("js")));
        write(
            &dir,
            "line_then_keys.vl",
            r#"import std::debug::Debug;
import std::io::print;
import std::option::Option::{ self, None, Some };
import std::process::{ keys, prompt };

fun main() {
	match prompt("name? ") {
		Some(let name) => print(i"[{name}]"),
		None => print("no name"),
	}
	let keys = keys();
	for {
		match keys.next() {
			Some(let key) => print(key.debug()),
			None => jump break,
		}
	}
}
"#,
        );
        let mut conversation = Conversation::start(&dir, "line_then_keys.vl", backend);
        conversation.wait_for("name? ");
        conversation.write(b"ann\r\nxy\x1b[B");
        conversation.close();
        let (code, stdout) = conversation.finish();
        assert_eq!(code, Some(0), "{backend:?}");
        assert_eq!(
            stdout, "name? [ann]\nKey::Char(\"x\")\nKey::Char(\"y\")\nKey::Down\n",
            "{backend:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A `vilan run` child on a PSEUDO-TERMINAL (A173 S2, terminal.md §4.4 step
/// 3): its stdin and stdout are the pty's slave, so `keys()` meets a real
/// terminal and enters raw mode — never the terminal this test runs in, which
/// stays untouched. The test keeps its own handle on the slave to read the
/// terminal's flags from outside, and the child leads a session of its own so
/// a signal can be sent to it and to the program it starts together.
#[cfg(unix)]
mod pty {
    use std::fs::File;
    use std::io::{Read, Write};
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::process::CommandExt;
    use std::path::Path;
    use std::process::{Child, Stdio};
    use std::sync::mpsc;
    use std::time::Instant;

    use super::{STEP_BOUND, vilan_run};

    /// The terminal flags a raw mode turns off.
    #[derive(Debug, PartialEq, Eq)]
    pub struct Flags {
        pub canonical: bool,
        pub echo: bool,
        pub signals: bool,
    }

    /// Cooked mode, as a fresh pty starts.
    pub const COOKED: Flags = Flags {
        canonical: true,
        echo: true,
        signals: true,
    };

    /// Raw mode.
    pub const RAW: Flags = Flags {
        canonical: false,
        echo: false,
        signals: false,
    };

    pub struct Session {
        label: String,
        child: Child,
        master: File,
        slave: OwnedFd,
        chunks: mpsc::Receiver<Vec<u8>>,
        pub output: Vec<u8>,
    }

    impl Session {
        pub fn start(dir: &Path, relative: &str, backend: Option<&str>) -> Session {
            let mut master_fd = 0;
            let mut slave_fd = 0;
            // SAFETY: `openpty` writes two descriptors through the pointers
            // on success; the name, termios and winsize are optional.
            let opened = unsafe {
                libc::openpty(
                    &raw mut master_fd,
                    &raw mut slave_fd,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null(),
                )
            };
            assert_eq!(opened, 0, "openpty");
            // SAFETY: both descriptors were just opened and are owned here.
            let (master, slave) =
                unsafe { (File::from_raw_fd(master_fd), OwnedFd::from_raw_fd(slave_fd)) };
            let mut command = vilan_run(dir, relative, backend);
            command
                .stdin(Stdio::from(slave.try_clone().expect("dup the slave")))
                .stdout(Stdio::from(slave.try_clone().expect("dup the slave")))
                .stderr(File::create(dir.join("stderr.log")).expect("stderr log"));
            // SAFETY: `setsid` is async-signal-safe, the one call made between
            // the fork and the exec.
            unsafe {
                command.pre_exec(|| {
                    libc::setsid();
                    Ok(())
                });
            }
            let child = command.spawn().expect("spawn vilan run on a pty");
            let mut reader = master.try_clone().expect("dup the master");
            let (sender, chunks) = mpsc::channel();
            std::thread::spawn(move || {
                let mut buffer = [0u8; 4096];
                // The master reads EIO once every slave descriptor is closed.
                while let Ok(count) = reader.read(&mut buffer) {
                    if count == 0 || sender.send(buffer[..count].to_vec()).is_err() {
                        break;
                    }
                }
            });
            Session {
                label: format!("{relative} on {} (pty)", backend.unwrap_or("js")),
                child,
                master,
                slave,
                chunks,
                output: Vec::new(),
            }
        }

        /// The flags the terminal holds right now.
        pub fn flags(&self) -> Flags {
            // SAFETY: a zeroed `termios` is a valid out-parameter, filled by
            // `tcgetattr` on the slave this session keeps open.
            let mut termios: libc::termios = unsafe { std::mem::zeroed() };
            let read = unsafe { libc::tcgetattr(self.slave.as_raw_fd(), &raw mut termios) };
            assert_eq!(read, 0, "tcgetattr");
            Flags {
                canonical: termios.c_lflag & libc::ICANON != 0,
                echo: termios.c_lflag & libc::ECHO != 0,
                signals: termios.c_lflag & libc::ISIG != 0,
            }
        }

        fn contains(&self, needle: &[u8]) -> bool {
            self.output
                .windows(needle.len())
                .any(|window| window == needle)
        }

        /// Waits until the terminal has shown `text`.
        pub fn wait_for(&mut self, text: &str) {
            let deadline = Instant::now() + STEP_BOUND;
            while !self.contains(text.as_bytes()) {
                let left = deadline.saturating_duration_since(Instant::now());
                match self.chunks.recv_timeout(left) {
                    Ok(chunk) => self.output.extend_from_slice(&chunk),
                    Err(_) => {
                        let _ = self.child.kill();
                        panic!(
                            "{}: waited for {text:?}; the terminal showed {:?}",
                            self.label,
                            String::from_utf8_lossy(&self.output)
                        );
                    }
                }
            }
        }

        pub fn shows(&self, bytes: &[u8]) -> bool {
            self.contains(bytes)
        }

        /// Types `bytes` at the terminal.
        pub fn write(&mut self, bytes: &[u8]) {
            self.master.write_all(bytes).expect("type at the pty");
        }

        /// Sends `signal` to the session the child leads — `vilan run` and the
        /// program it started together.
        pub fn signal(&self, signal: libc::c_int) {
            let group = self.child.id() as libc::pid_t;
            // SAFETY: a plain `killpg` of the group this test created.
            let sent = unsafe { libc::killpg(group, signal) };
            assert_eq!(sent, 0, "killpg");
        }

        /// Waits for `vilan run` to exit and answers its status.
        pub fn finish(mut self) -> (std::process::ExitStatus, Flags, Vec<u8>) {
            let deadline = Instant::now() + STEP_BOUND;
            let status = loop {
                if let Some(status) = self.child.try_wait().expect("poll the child") {
                    break status;
                }
                if Instant::now() >= deadline {
                    let _ = self.child.kill();
                    panic!("{}: the program did not exit", self.label);
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            };
            // What the program wrote on its way out is in the pty's buffer.
            while let Ok(chunk) = self
                .chunks
                .recv_timeout(std::time::Duration::from_millis(200))
            {
                self.output.extend_from_slice(&chunk);
            }
            let flags = self.flags();
            (status, flags, self.output)
        }
    }
}

/// A `keys()` program on a terminal: raw mode on, bracketed paste on, then
/// the loop's end restores both (A173 S2, terminal.md §3.3, §4.4 step 3). A
/// lone ESC on a TERMINAL is `Escape` after the 50 ms timeout (the session
/// writes nothing after it until it is shown), an arrow arrives whole, Ctrl-C
/// is a KEY — the program stays alive — and the terminal shows `\r\n` line
/// ends because output processing stays on under raw mode, as libuv's does.
#[cfg(unix)]
#[test]
fn a173_s2_keys_enters_raw_mode_on_a_terminal_and_drop_restores_it() {
    for backend in BACKENDS {
        let dir = temp_project(&format!("keys_pty_{}", backend.unwrap_or("js")));
        write(
            &dir,
            "keys_pty.vl",
            r#"import std::debug::Debug;
import std::io::print;
import std::option::Option::{ self, None, Some };
import std::process::{ Key, keys };

fun main() {
	let keys = keys();
	print("ready");
	for {
		match keys.next() {
			Some(Key::Char("q")) => jump break,
			Some(let key) => print(key.debug()),
			None => jump break,
		}
	}
	print("after the loop");
}
"#,
        );
        let mut session = pty::Session::start(&dir, "keys_pty.vl", backend);
        assert_eq!(session.flags(), pty::COOKED, "{backend:?}: before");
        session.wait_for("ready\r\n");
        assert_eq!(session.flags(), pty::RAW, "{backend:?}: while `Keys` lives");
        assert!(session.shows(b"\x1b[?2004hready"), "{backend:?}: paste on");
        session.write(b"\x1b");
        session.wait_for("Key::Escape\r\n");
        session.write(b"\x1b[A");
        session.wait_for("Key::Up\r\n");
        session.write(b"\x03");
        session.wait_for("Key::Ctrl(\"c\")\r\n");
        session.write(b"q");
        session.wait_for("after the loop\r\n");
        let (status, flags, output) = session.finish();
        assert_eq!(status.code(), Some(0), "{backend:?}");
        assert_eq!(flags, pty::COOKED, "{backend:?}: after");
        let output = String::from_utf8_lossy(&output);
        assert!(
            output.contains("\u{1b}[?2004lafter the loop"),
            "{backend:?}: paste off at the drop: {output:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The roads a `Keys`'s `Drop` does not take still restore the terminal
/// (A173 S2, terminal.md §3.3): `process::exit` while it lives (no destructor
/// runs; node's `'exit'` listener, natively `vilan-rt`'s exit hooks), a
/// `panic` (the unwind), and SIGTERM, SIGHUP and SIGINT (a handler that
/// restores and re-raises, so the process still dies of the signal).
#[cfg(unix)]
#[test]
fn a173_s2_exit_panic_and_signals_restore_the_terminal() {
    let program = |ending: &str| {
        format!(
            r#"import std::io::{{ panic, print }};
import std::process::{{ exit, keys }};

fun main() {{
	let keys = keys();
	print("ready");
	if keys.next().is_some() {{
		{ending}
	}}
	// A use AFTER the ending keeps the handle live through it: without one,
	// its `Drop` would run at the last use above and restore the terminal
	// before the ending, and the roads under test would never be taken.
	print(i"after the ending: {{keys.next().is_none()}}");
}}
"#
        )
    };
    for backend in BACKENDS {
        for (name, ending, code) in [
            ("exit", "exit(3);", Some(3)),
            ("panic", "panic(\"boom\");", Some(1)),
        ] {
            let dir = temp_project(&format!("keys_{name}_{}", backend.unwrap_or("js")));
            write(&dir, "ending.vl", &program(ending));
            let mut session = pty::Session::start(&dir, "ending.vl", backend);
            session.wait_for("ready\r\n");
            assert_eq!(session.flags(), pty::RAW, "{backend:?} {name}");
            session.write(b"k");
            let (status, flags, output) = session.finish();
            assert_eq!(status.code(), code, "{backend:?} {name}");
            assert_eq!(flags, pty::COOKED, "{backend:?} {name}: restored");
            assert!(
                String::from_utf8_lossy(&output).contains("\u{1b}[?2004l"),
                "{backend:?} {name}: paste off"
            );
            let _ = std::fs::remove_dir_all(&dir);
        }
        for (name, signal) in [
            ("sigterm", libc::SIGTERM),
            ("sighup", libc::SIGHUP),
            ("sigint", libc::SIGINT),
        ] {
            let dir = temp_project(&format!("keys_{name}_{}", backend.unwrap_or("js")));
            write(
                &dir,
                "ending.vl",
                &program("print(\"read\");\n\t\tkeys.next();"),
            );
            let mut session = pty::Session::start(&dir, "ending.vl", backend);
            session.wait_for("ready\r\n");
            session.write(b"k");
            session.wait_for("read\r\n");
            assert_eq!(session.flags(), pty::RAW, "{backend:?} {name}");
            session.signal(signal);
            // `vilan run` itself dies of the signal too and does not wait for
            // the program: the paste switch going off is the program's own
            // word that its handler ran, and the handler restores the
            // terminal BEFORE it.
            session.wait_for("\u{1b}[?2004l");
            let (status, flags, _) = session.finish();
            assert!(!status.success(), "{backend:?} {name}: {status:?}");
            assert_eq!(flags, pty::COOKED, "{backend:?} {name}: restored");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

// ---------------------------------------------------------------------------
// E332: `io::write` — stdout with no newline, flushed, on both backends.
// ---------------------------------------------------------------------------

/// `io::write(text)` (E332): the bytes reach stdout exactly as written — no
/// newline added, a `\r` kept, in order with `print` — and FLUSHED by the
/// write itself. The proof of the flush is the sequencing: the write happens
/// while a read is ALREADY parked (so the read's own flush ran before it),
/// and the test types nothing until the written text has arrived; natively a
/// write left in Rust's line-buffered stdout would never arrive, and the
/// session would hang. The program's last write ends the output with no
/// newline after it.
#[test]
fn e332_write_puts_unterminated_text_on_stdout_flushed() {
    for backend in BACKENDS {
        let dir = temp_project(&format!("write_{}", backend.unwrap_or("js")));
        write(
            &dir,
            "write.vl",
            r#"import std::io::{ print, write };
import std::option::Option::{ self, None, Some };
import std::process::read_line;
import std::task;
import std::time;

fun main() {
	task::nursery(|n| {
		async {
			match read_line() {
				Some(let line) => print(i"[{line}]"),
				None => print("the end"),
			}
			n.cancel();
		};
		async {
			time::sleep(10);
			write("par");
			write("tial\r");
			time::sleep(1000000);
		};
	});
	write("tail");
}
"#,
        );
        let mut conversation = Conversation::start(&dir, "write.vl", backend);
        conversation.wait_for("partial\r");
        assert_eq!(conversation.stdout, "partial\r", "{backend:?}");
        conversation.write("ok\n");
        let (code, stdout) = conversation.finish();
        assert_eq!(code, Some(0), "{backend:?}");
        assert_eq!(stdout, "partial\r[ok]\ntail", "{backend:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
