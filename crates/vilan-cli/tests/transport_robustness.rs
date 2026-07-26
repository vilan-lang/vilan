//! End-to-end test for K6 transport robustness
//! (vilan/proposal/transport-robustness.md): a generated client rides a real
//! WebSocket to a real server, which is then STOPPED (SIGSTOP — the in-flight
//! call hangs), KILLED (the socket closes), and RESTARTED with different
//! state. Asserts the whole contract: the pending call rejects with a typed
//! transport error (never dangles), the state signal walks
//! Connected → Reconnecting → Connected, a call made while down fails fast,
//! the mirror RE-SYNCS to the restarted server's value through the re-attach
//! hook, and calls work again afterwards.

use std::path::{Path, PathBuf};
use std::process::Command;
// The reconnect test drives the server with `kill -STOP`/`-KILL`; everything
// that exists only to serve it is unix-gated with it (see the test).
#[cfg(unix)]
use std::net::TcpListener;
#[cfg(unix)]
use std::process::{Child, Stdio};
#[cfg(unix)]
use std::sync::mpsc::Receiver;
#[cfg(unix)]
use std::time::{Duration, Instant};

fn temp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vilan_robust_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// Bind an ephemeral port, then release it — a free port for the server (a small
/// TOCTOU window). Fixed literals are unbindable outright inside Windows'
/// Hyper-V/WSL reserved ranges (windows-support.md §4).
///
/// The one probe backlog E19's port-0 rework deliberately LEFT: this test kills
/// the server mid-call and starts a SECOND server process that the client must
/// reconnect to, so the port has to be the same across two independent binds —
/// which a port-0 bind cannot promise. Phase 1 could announce its port for phase
/// 3 to reuse, but that only moves the window (the port is released by the kill),
/// so it buys nothing the probe does not already have.
#[cfg(unix)]
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// A node child whose stdout lines stream to a channel; killed on drop.
#[cfg(unix)]
struct LineChild {
    child: Child,
    lines: Receiver<String>,
}

#[cfg(unix)]
impl LineChild {
    fn spawn(bundle: &Path, argument: Option<&str>) -> LineChild {
        let mut command = Command::new("node");
        command.arg(bundle);
        if let Some(argument) = argument {
            command.arg(argument);
        }
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn node");
        let stdout = child.stdout.take().unwrap();
        let (sender, lines) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            use std::io::BufRead;
            for line in std::io::BufReader::new(stdout)
                .lines()
                .map_while(Result::ok)
            {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        LineChild { child, lines }
    }

    /// Blocks until a stdout line containing `needle` arrives; returns it.
    fn await_line(&self, needle: &str, timeout: Duration) -> String {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "timed out waiting for `{needle}` on stdout"
            );
            match self.lines.recv_timeout(remaining) {
                Ok(line) if line.contains(needle) => return line,
                Ok(_other) => {}
                Err(_) => panic!("stdout ended or timed out before `{needle}`"),
            }
        }
    }

    /// Send a signal by name (`-STOP`, `-KILL`) via `kill(1)`. Unix-only and
    /// permanently so: Windows has no pause/resume analogue for a process
    /// (windows-support.md §1's non-goals) — `SuspendThread` is per-thread and
    /// documented as debugger-only, not a process freeze.
    fn signal(&self, name: &str) {
        let status = Command::new("kill")
            .args([name, &self.child.id().to_string()])
            .status()
            .expect("send signal");
        assert!(status.success(), "kill {name} failed");
    }
}

#[cfg(unix)]
impl Drop for LineChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

const COMMON: &str = r#"import std::reactive::Signal;

[service(StatusClient)]
struct StatusBoard {
	[expose] status: Signal<i32>,
}

impl StatusBoard {
	[rpc]
	fun set_status(self, value: i32): i32 {
		self.status.set(value);
		value
	}

	[rpc]
	fun echo(self, value: i32): i32 {
		value
	}
}
"#;

#[cfg(unix)]
const SERVER: &str = r#"import std::print;
import std::reactive::Signal;
import std::json::json_codec;
import std::option::Option::{ self, Some, None };
import std::process::args;
import std::http::Response;
import std::rpc_server::serve_service;
import common::StatusBoard;

async fun main() {
	let initial = match args().get(0) {
		Some(let raw) => match raw.parse_i32() {
			Some(let value) => value,
			None => 0,
		},
		None => 0,
	};
	let board = StatusBoard { status = Signal::new(initial) };
	serve_service(9297, board.dispatcher().into_protocol(json_codec()), |request| {
		Response::builder().code(404).body("nope").build()
	}, |server| print(i"listening {initial}"));
}
"#;

#[cfg(unix)]
const CLIENT: &str = r#"import std::print;
import std::shared::Shared;
import std::json::json_codec;
import std::result::Result::{ self, Ok, Err };
import std::time::sleep;
import std::process::exit;
import std::rpc::ConnectionState;
import common::{ StatusBoard, StatusClient };

async fun main() {
	match StatusClient::connect("ws://localhost:9297/", json_codec()) {
		Ok(let client) => {
			let state = client.transport.connection_state();
			let fast_fired: Shared<bool> = Shared::new(false);
			let resynced: Shared<bool> = Shared::new(false);

			let watching_state = state.sub(|current| {
				print(i"state:{current.debug()}");
				// The moment the drop is noticed, prove fail-fast: a call
				// while down errors immediately instead of hanging.
				if current == ConnectionState::Reconnecting && !fast_fired.read() {
					fast_fired.write() = true;
					match client.set_status(9) {
						Ok(let value) => print(i"fast:ok:{value}"),
						Err(let error) => print(i"fast:err:{error.debug()}"),
					}
				}
			});

			let watching_mirror = client.status.sub(|value| {
				print(i"mirror:{value}");
				// The restarted server announces itself with status 2; a call
				// on the reconnected transport must succeed again.
				if value == 2 && !resynced.read() {
					resynced.write() = true;
					match client.set_status(5) {
						Ok(let confirmed) => {
							print(i"call:ok:{confirmed}");
							exit(0);
						},
						Err(let error) => print(i"call:err:{error.debug()}"),
					}
				}
			});

			// Give the harness a beat, then send the call that will be caught
			// in flight by the stop/kill.
			sleep(500);
			print("doomed:sent");
			match client.echo(7) {
				Ok(let value) => print(i"doomed:ok:{value}"),
				Err(let error) => print(i"doomed:err:{error.debug()}"),
			}
			// Keep main open through the outage — on node a COMPLETED main
			// exits the process; the success path exits from the mirror sub.
			sleep(600000);
		},
		Err(let error) => print(i"connect failed: {error.debug()}"),
	}
}
"#;

/// UNIX-ONLY, permanently (windows-support.md §1 non-goals, §4): phase 2 freezes
/// the server with `kill -STOP` so a call is caught in flight, which has no
/// Windows analogue. The rest of the K6 contract (fail-fast, typed rejection,
/// mirror re-sync) is only observable *because* the server can be frozen
/// mid-call, so the test is not splittable into a portable half.
#[cfg(unix)]
#[test]
fn a_dropped_connection_reconnects_and_resyncs() {
    let dir = temp_project("reconnect");
    // An ephemeral port, substituted into both halves of the pair.
    let port = free_port().to_string();
    write(
        &dir,
        "vilan.toml",
        "[project]\npackages = [\"common\", \"server\", \"client\"]\n",
    );
    write(&dir, "common/vilan.toml", "[library]\nname = \"common\"\n");
    write(
        &dir,
        "server/vilan.toml",
        "[package]\nname = \"server\"\ntarget = \"node\"\n\n[package.dependencies]\ncommon = { path = \"../common\" }\n",
    );
    write(
        &dir,
        "client/vilan.toml",
        "[package]\nname = \"client\"\ntarget = \"node\"\n\n[package.dependencies]\ncommon = { path = \"../common\" }\n",
    );
    write(&dir, "common/src/lib.vl", COMMON);
    write(&dir, "server/src/main.vl", &SERVER.replace("9297", &port));
    write(&dir, "client/src/main.vl", &CLIENT.replace("9297", &port));

    let build = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", dir.to_str().unwrap()])
        .output()
        .expect("run vilan build");
    assert!(
        build.status.success(),
        "build failed:\n{}{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let wait = Duration::from_secs(20);

    // Phase 1: server up (status 1), client syncs.
    let server = LineChild::spawn(&dir.join("dist/server.js"), Some("1"));
    server.await_line("listening 1", wait);
    let client = LineChild::spawn(&dir.join("dist/client.js"), None);
    client.await_line("state:Connected", wait);
    client.await_line("mirror:1", wait);

    // Phase 2: freeze the server so the next call hangs in flight, then kill.
    server.signal("-STOP");
    client.await_line("doomed:sent", wait);
    std::thread::sleep(Duration::from_millis(300));
    server.signal("-KILL");
    drop(server);

    // The drop is noticed: state flips, a call while down fails fast (fired
    // synchronously inside the state notification, so it precedes the
    // pending rejection on stdout), and the in-flight call REJECTS (typed,
    // never dangling). `await_line` consumes skipped lines, so the order
    // here mirrors the client's emission order.
    client.await_line("state:Reconnecting", wait);
    let fast = client.await_line("fast:err", wait);
    assert!(
        fast.contains("not connected"),
        "call while down should fail fast, got: {fast}"
    );
    let doomed = client.await_line("doomed:err", wait);
    assert!(
        doomed.contains("connection lost"),
        "in-flight call should reject with the drop reason, got: {doomed}"
    );

    // Phase 3: restart with DIFFERENT state — the backoff loop reconnects,
    // the hook re-attaches, the mirror resyncs, calls work again.
    let revived = LineChild::spawn(&dir.join("dist/server.js"), Some("2"));
    revived.await_line("listening 2", wait);
    client.await_line("state:Connected", wait);
    client.await_line("mirror:2", wait);
    client.await_line("call:ok:5", wait);

    let _ = std::fs::remove_dir_all(&dir);
}

/// B21 (FIXED): a unit consuming a DEPENDENCY package's `[service]` without
/// its own `std::rpc` import used to mistype the generated `connect`. The
/// mechanism: the dependency-surface load path never scanned for `[service]`,
/// so `std::rpc` wasn't loaded when the once-only macro registry was built —
/// and the expansion silently fell back to the Rust FIXTURE generator, whose
/// template had gone stale (it still produced the pre-K6 `connect`). The
/// dependency surface now seeds the rpc load like the other two scan sites,
/// and a real std reaching the fallback errors loudly instead of silently
/// generating stale code.
#[test]
fn a_library_service_client_compiles_without_an_rpc_import() {
    let dir = temp_project("b21");
    write(
        &dir,
        "vilan.toml",
        "[project]\npackages = [\"common\", \"app\"]\n",
    );
    write(&dir, "common/vilan.toml", "[library]\nname = \"common\"\n");
    write(
        &dir,
        "app/vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n\n[package.dependencies]\ncommon = { path = \"../common\" }\n",
    );
    write(&dir, "common/src/lib.vl", COMMON);
    write(
        &dir,
        "app/src/main.vl",
        r#"import std::print;
import std::json::json_codec;
import std::result::Result::{ self, Ok, Err };
import common::StatusClient;

async fun main() {
	match StatusClient::connect("ws://localhost:1/", json_codec()) {
		Ok(let client) => print("connected"),
		Err(let error) => print("no server"),
	}
}
main();
"#,
    );
    let build = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", dir.to_str().unwrap()])
        .output()
        .expect("run vilan build");
    assert!(
        build.status.success(),
        "the generated connect mistyped without a consumer-side std::rpc import:\n{}{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}
