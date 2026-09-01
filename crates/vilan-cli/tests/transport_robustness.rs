//! End-to-end tests for K6 transport robustness
//! (proposal/transport-robustness.md): a generated client rides a real
//! WebSocket to a real server, which is then STOPPED (SIGSTOP — the in-flight
//! call hangs), KILLED (the socket closes), and RESTARTED. What the restarted
//! server IS is the variable this file turns: the same service (the happy
//! re-sync path), a service that will not open a session (the re-attach fails),
//! or a DIFFERENT service (the contract drifted under us) — or NOTHING, the
//! server that never comes back and spends the whole retry budget. The three
//! servers are real, built from one source — there is no fault-injection seam
//! in std, and none is needed, because every failure the reconnect path can
//! meet is reachable from `Service`'s own public surface, the fourth case by
//! declining to start one at all.
//!
//! Asserted across the legs: the pending call rejects with a typed transport
//! error (never dangles), the state signal walks Connected → Reconnecting →
//! Connected, a call made while down fails fast, the mirror RE-SYNCS to the
//! restarted server's value through the re-attach hook, calls work again
//! afterwards — and, when the mirrors CANNOT be rebound, the socket reaches the
//! terminal `Closed` instead of claiming to be live over dead channel ids, and
//! the client that held those mirrors disposes itself instead of wedging: on
//! the two refusals (A30) and on the spent retry budget (A31), which is one
//! law read off the state rather than three arms each remembering it.

use std::path::{Path, PathBuf};
use std::process::Command;
// The reconnect tests drive the server with `kill -STOP`/`-KILL`; everything
// that exists only to serve them is unix-gated with them (see the tests).
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
/// The one probe backlog E19's port-0 rework deliberately LEFT: these tests kill
/// the server and start a SECOND server process that the client must reconnect
/// to, so the port has to be the same across two independent binds — which a
/// port-0 bind cannot promise. Phase 1 could announce its port for phase 3 to
/// reuse, but that only moves the window (the port is released by the kill), so
/// it buys nothing the probe does not already have.
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
    fn spawn(bundle: &Path, arguments: &[&str]) -> LineChild {
        let mut command = Command::new("node");
        command.arg(bundle);
        command.args(arguments);
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

    /// Every stdout line up to and INCLUDING the first containing `needle`.
    /// How a negative is asserted here: "the mirror never resynced" is a claim
    /// about the lines between two events, not about any one line — and the
    /// consumed lines are gone once `await_line` has skipped them.
    fn collect_until(&self, needle: &str, timeout: Duration) -> Vec<String> {
        let deadline = Instant::now() + timeout;
        let mut seen: Vec<String> = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "timed out waiting for `{needle}` on stdout; saw: {seen:?}"
            );
            match self.lines.recv_timeout(remaining) {
                Ok(line) => {
                    let matched = line.contains(needle);
                    seen.push(line);
                    if matched {
                        return seen;
                    }
                }
                Err(_) => panic!("stdout ended or timed out before `{needle}`; saw: {seen:?}"),
            }
        }
    }

    /// Blocks until a stdout line containing `needle` arrives; returns it.
    fn await_line(&self, needle: &str, timeout: Duration) -> String {
        self.collect_until(needle, timeout)
            .pop()
            .expect("collect_until returns the matched line last")
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

const COMMON: &str = r#"import std::reactive::{ Signal, SignalCell };

[service(StatusClient)]
struct StatusBoard {
	[expose] status: SignalCell<i32>,
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

/// The server, whose FAILURE MODE is an argument: `<status> <mode>`.
///
/// - `none` — the ordinary service. `Service::new`'s default lifecycle
///   registers a reactive session per connection, so `__attach` answers.
/// - `refuse-attach` — the same service surface, with the connection lifecycle
///   replaced by one that registers NO session. `Service::on_connect` is the
///   documented seam for exactly this ("an app-written attach"), and the
///   generated `__attach` route's own `Option::None` arm is what answers:
///   `RpcError::Remote("unknown connection")`. `__contract` still matches, so
///   the client reaches the attach step and fails THERE — the one ordering the
///   swallowed-`Err` defect needed.
/// - `drift` — a DIFFERENT service on the same mount and port: the server
///   redeployed under the client, which is what the reconnect's `__contract`
///   re-check exists to catch.
#[cfg(unix)]
const SERVER: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::json::json_codec;
import std::option::Option::{ self, Some, None };
import std::process::args;
import std::http::{ Response, Server };
import std::rpc_server::Service;
import common::StatusBoard;

// A second service with a DIFFERENT contract surface — the redeployed server.
[service(DriftClient)]
struct DriftBoard {
	[expose] tally: SignalCell<i32>,
}

impl DriftBoard {
	[rpc]
	fun bump(self, by: i32): i32 {
		self.tally.set(self.tally.get() + by);
		self.tally.get()
	}
}

async fun main() {
	let initial = match args().get(0) {
		Some(let raw) => match raw.parse_i32() {
			Some(let value) => value,
			None => 0,
		},
		None => 0,
	};
	let mode = match args().get(1) {
		Some(let raw) => raw,
		None => "none",
	};
	let board = StatusBoard { status = Signal::new(initial) };
	let service = if mode == "refuse-attach" {
		Service::new(board.dispatcher().into_protocol(json_codec()))
			.on_connect(|connection, wire| print(i"attach-refused:{connection}"))
	} else if mode == "drift" {
		let drifted = DriftBoard { tally = Signal::new(initial) };
		Service::new(drifted.dispatcher().into_protocol(json_codec()))
	} else {
		Service::new(board.dispatcher().into_protocol(json_codec()))
	};
	Server::builder()
		.port(9297)
		.with_service(service)
		.on_request(|request| Response::builder().code(404).body("nope").build())
		.on_start(|server| print(i"listening {initial} {mode}"))
		.build()
		.start();
}
"#;

#[cfg(unix)]
const CLIENT: &str = r#"import std::io::print;
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
					// A30's negative: this socket redialled and its mirrors
					// resynced, so the client must NOT have disposed itself —
					// the inbound handler that just delivered this very value
					// is still installed.
					if client.status.transport.me.read().is_some() {
						print("disposed:false");
					} else {
						print("disposed:true");
					}
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

/// The plain observer: every connection-state transition and every mirror
/// value, and nothing else. What the re-attach failure cases need is not a
/// call to make but a claim to disprove — that the socket says `Connected`
/// while the mirrors are bound to a dead connection's channels — so the client
/// only reports, and the assertions are about what does and does not appear.
///
/// It also reports A30's auto-disposal, which is POLLED rather than reported
/// from the state observer: the observer runs inside `state.set(Closed)`, one
/// line before the client disposes itself, so it could only ever see the
/// before. The slot it reads is `ReactiveClient.transport.me` — the mirror
/// holds that same `DuplexEnd`, which is what makes the disposal visible from
/// the generated client at all.
#[cfg(unix)]
const WATCH_CLIENT: &str = r#"import std::io::print;
import std::json::json_codec;
import std::result::Result::{ self, Ok, Err };
import std::time::sleep;
import std::rpc::ConnectionState;
import common::{ StatusBoard, StatusClient };

async fun main() {
	match StatusClient::connect("ws://localhost:9297/", json_codec()) {
		Ok(let client) => {
			let watching_state = client.transport.connection_state().sub(|current| {
				print(i"state:{current.debug()}");
			});
			let watching_mirror = client.status.sub(|value| {
				print(i"mirror:{value}");
			});
			print("ready");
			// Poll for the terminal state, then report the disposal that
			// follows it. Bounded (30 s) so a run that never closes ends
			// quietly rather than spinning; the harness's own 20 s waits
			// fail the test first.
			mut ticks = 0;
			for ticks < 300 {
				sleep(100);
				if client.transport.connection_state().get() == ConnectionState::Closed {
					ticks = 300;
					if client.status.transport.me.read().is_some() {
						print("disposed:false");
					} else {
						print("disposed:true");
					}
				} else {
					ticks = ticks + 1;
				}
			}
			// Held open through the outage; the harness kills the process.
			sleep(600000);
		},
		Err(let error) => print(i"connect failed: {error.debug()}"),
	}
}
"#;

/// `WATCH_CLIENT` with a longer terminal-state poll, and nothing else changed.
/// The re-attach refusals close for good within a beat of the server coming
/// back, so 30 s of polling is generous for them; the retry budget IS the
/// event in the exhaustion leg — ten attempts, 250 ms doubling to a 4 s cap,
/// about 24 s of sleeps — which leaves that const's bound close enough to the
/// thing it is waiting for to be a timing pin rather than a behaviour one. Same
/// program otherwise, and the two A30 legs keep the 30 s form byte for byte.
#[cfg(unix)]
fn patient_watch_client() -> String {
    assert_eq!(
        WATCH_CLIENT.matches("300").count(),
        2,
        "the poll bound is the only `300` in WATCH_CLIENT; keep this helper in step with it"
    );
    WATCH_CLIENT.replace("300", "900")
}

/// One built project — `common` (the shared `[service]`), `server` (the
/// mode-taking server above) and `client` (whichever program the test drives)
/// — on one ephemeral port, ready to spawn processes from. The port is
/// substituted into both halves at build time, so it lives in the sources
/// rather than on this value.
#[cfg(unix)]
struct ReconnectFixture {
    directory: PathBuf,
}

#[cfg(unix)]
impl ReconnectFixture {
    /// Write the three packages, substitute the port into both halves, build.
    fn build(tag: &str, client_source: &str) -> ReconnectFixture {
        let directory = temp_project(tag);
        let port = free_port().to_string();
        write(
            &directory,
            "vilan.toml",
            "[project]\npackages = [\"common\", \"server\", \"client\"]\n",
        );
        write(
            &directory,
            "common/vilan.toml",
            "[library]\nname = \"common\"\n",
        );
        write(
            &directory,
            "server/vilan.toml",
            "[package]\nname = \"server\"\ntarget = \"node\"\n\n[package.dependencies]\ncommon = { path = \"../common\" }\n",
        );
        write(
            &directory,
            "client/vilan.toml",
            "[package]\nname = \"client\"\ntarget = \"node\"\n\n[package.dependencies]\ncommon = { path = \"../common\" }\n",
        );
        write(&directory, "common/src/lib.vl", COMMON);
        write(
            &directory,
            "server/src/main.vl",
            &SERVER.replace("9297", &port),
        );
        write(
            &directory,
            "client/src/main.vl",
            &client_source.replace("9297", &port),
        );

        let build = Command::new(env!("CARGO_BIN_EXE_vilan"))
            .args(["build", directory.to_str().unwrap()])
            .output()
            .expect("run vilan build");
        assert!(
            build.status.success(),
            "build failed:\n{}{}",
            String::from_utf8_lossy(&build.stdout),
            String::from_utf8_lossy(&build.stderr)
        );
        ReconnectFixture { directory }
    }

    /// A server process serving `status`, in one of the three failure modes.
    fn server(&self, status: &str, mode: &str) -> LineChild {
        LineChild::spawn(&self.directory.join("dist/server.mjs"), &[status, mode])
    }

    fn client(&self) -> LineChild {
        LineChild::spawn(&self.directory.join("dist/client.mjs"), &[])
    }

    /// Only on the success path, deliberately: a failed run leaves the built
    /// project behind to be read.
    fn clean(&self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// UNIX-ONLY, permanently (windows-support.md §1 non-goals, §4): phase 2 freezes
/// the server with `kill -STOP` so a call is caught in flight, which has no
/// Windows analogue. The rest of the K6 contract (fail-fast, typed rejection,
/// mirror re-sync) is only observable *because* the server can be frozen
/// mid-call, so the test is not splittable into a portable half.
///
/// This is also the ATTACH-SUCCEEDS case of A26: the re-attach answers, so the
/// `rebinds` loop runs and every mirror moves to the fresh connection's
/// channels. The two tests below are its failing siblings.
#[cfg(unix)]
#[test]
fn a_dropped_connection_reconnects_and_resyncs() {
    let fixture = ReconnectFixture::build("reconnect", CLIENT);
    let wait = Duration::from_secs(20);

    // Phase 1: server up (status 1), client syncs.
    let server = fixture.server("1", "none");
    server.await_line("listening 1", wait);
    let client = fixture.client();
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
    let revived = fixture.server("2", "none");
    revived.await_line("listening 2", wait);
    client.await_line("state:Connected", wait);
    client.await_line("mirror:2", wait);
    // A30's load-bearing negative: an ordinary redial disposes NOTHING. The
    // two terminal siblings below are the only paths that may.
    let disposal = client.await_line("disposed:", wait);
    assert_eq!(
        disposal, "disposed:false",
        "a resynced connection must keep its client wired to the transport"
    );
    client.await_line("call:ok:5", wait);

    fixture.clean();
}

/// A26 (N16 audit run 2, ruled 2026-08-28): the reconnect's `__attach` FAILS.
/// The restarted server answers `__contract` with the matching hash and then
/// refuses to open a session, so there are no fresh channel ids to rebind to.
///
/// The defect this pins: `reattach_mirrors`'s `Err` arm was `{}`, so the
/// `rebinds` loop was skipped in silence — the socket stayed `Connected`, every
/// mirror kept pointing at the dead connection's channel ids, and nothing
/// anywhere reported it. A live-looking client that can never update again.
/// The failed attach now takes the same terminal `Closed` (+ `close()`) as the
/// contract-drift sibling, which is a state the app's `or`/state machinery can
/// actually see.
#[cfg(unix)]
#[test]
fn a_refused_reattach_closes_the_socket_instead_of_wedging_the_mirrors() {
    let fixture = ReconnectFixture::build("refused_attach", WATCH_CLIENT);
    let wait = Duration::from_secs(20);

    let server = fixture.server("1", "none");
    server.await_line("listening 1", wait);
    let client = fixture.client();
    client.await_line("state:Connected", wait);
    client.await_line("mirror:1", wait);

    server.signal("-KILL");
    drop(server);
    client.await_line("state:Reconnecting", wait);

    // Same surface (so the contract re-check passes), no session registry (so
    // the attach it reaches next fails).
    let revived = fixture.server("2", "refuse-attach");
    revived.await_line("listening 2", wait);
    revived.await_line("attach-refused", wait);

    // The state flips to Connected a beat before the hooks run — by design
    // (§2.5: the re-attach's own rpc call needs a usable transport). What must
    // NOT happen is that it stays there.
    client.await_line("state:Connected", wait);
    let settled = client.collect_until("state:Closed", wait);
    assert!(
        !settled.iter().any(|line| line.contains("mirror:2")),
        "nothing was rebound, so no mirror can have resynced: {settled:?}"
    );
    // A30: the terminal state is followed by the client disposing itself —
    // routes emptied, transport handler cleared. Nothing on this socket can
    // ever update those mirrors again, so nothing should still hold them.
    let disposal = client.await_line("disposed:", wait);
    assert_eq!(
        disposal, "disposed:true",
        "a refused re-attach must leave no wedged client holding the graph"
    );

    fixture.clean();
}

/// The sibling branch, which had no pin of its own: the restarted server is a
/// DIFFERENT service, so the reconnect's `__contract` re-check finds drift.
/// `reattach_mirrors` closes for good rather than feeding typed mirrors from a
/// surface they were not built against (transport-robustness.md §2.5). Only the
/// CONNECT-time refusal was covered (`Err(RpcError::Contract(..))`, pinned in
/// the inference suite); the reconnect-time one is this.
#[cfg(unix)]
#[test]
fn a_server_that_redeploys_a_different_surface_closes_the_socket() {
    let fixture = ReconnectFixture::build("contract_drift", WATCH_CLIENT);
    let wait = Duration::from_secs(20);

    let server = fixture.server("1", "none");
    server.await_line("listening 1", wait);
    let client = fixture.client();
    client.await_line("state:Connected", wait);
    client.await_line("mirror:1", wait);

    server.signal("-KILL");
    drop(server);
    client.await_line("state:Reconnecting", wait);

    let revived = fixture.server("2", "drift");
    revived.await_line("listening 2", wait);

    client.await_line("state:Connected", wait);
    let settled = client.collect_until("state:Closed", wait);
    assert!(
        !settled.iter().any(|line| line.contains("mirror:2")),
        "a drifted surface must not feed the mirrors: {settled:?}"
    );
    // A30, the drift half of the same wiring.
    let disposal = client.await_line("disposed:", wait);
    assert_eq!(
        disposal, "disposed:true",
        "a drifted surface must leave no wedged client holding the graph"
    );

    fixture.clean();
}

/// A31, the THIRD terminal arm and the one A30 left unwired: the retry budget
/// runs out. The server dies and STAYS dead, so `handle_drop` spends all ten
/// attempts against a port nothing answers on and gives up — the same terminal
/// `Closed` the two refusals above reach, arrived at from the transport rather
/// than from the contract.
///
/// The defect this pins: that arm was `duplex.state.set(Closed)` on its own,
/// with no `ReactiveClient` anywhere in scope to dispose — so the ONE outage
/// every app actually meets (a server that does not come back) was the one path
/// that reported the connection over while leaving the client holding every
/// mirror, and the release A30 built ran on the two rarer paths only. The state
/// says the same thing on all three; what it costs is now the same too.
#[cfg(unix)]
#[test]
fn a_spent_retry_budget_closes_for_good_and_disposes_the_client() {
    let fixture = ReconnectFixture::build("retry_exhaustion", &patient_watch_client());
    // The budget itself is ~24 s of backoff sleeps, so this wait covers the
    // whole of it rather than the usual beat-or-two.
    let wait = Duration::from_secs(60);

    let server = fixture.server("1", "none");
    server.await_line("listening 1", wait);
    let client = fixture.client();
    client.await_line("state:Connected", wait);
    client.await_line("mirror:1", wait);

    // The one difference from every other leg in this file: nothing takes the
    // dead server's place. The port stays unbound for the whole budget.
    server.signal("-KILL");
    drop(server);
    client.await_line("state:Reconnecting", wait);

    let settled = client.collect_until("state:Closed", wait);
    assert!(
        !settled.iter().any(|line| line.contains("state:Connected")),
        "nothing came back, so no attempt can have reconnected: {settled:?}"
    );
    // A31's uniform law: the terminal state disposes on this arm exactly as it
    // does on the two refusals — routes emptied, transport handler cleared.
    let disposal = client.await_line("disposed:", wait);
    assert_eq!(
        disposal, "disposed:true",
        "a spent retry budget must leave no wedged client holding the graph"
    );

    fixture.clean();
}

/// The draft leg (A14, `proposal/draft-reconnect.md`): a `Draft` edited
/// WHILE THE CONNECTION IS DOWN re-sends itself when it comes back, over a real
/// socket and a real server restart. Same unix gate and the same reason as the
/// tests above — the outage is produced by killing the server process.
#[cfg(unix)]
const DRAFT_CLIENT: &str = r#"import std::io::print;
import std::shared::Shared;
import std::json::json_codec;
import std::result::Result::{ self, Ok, Err };
import std::option::Option::{ self, Some, None };
import std::time::sleep;
import std::process::exit;
import std::rpc::ConnectionState;
import std::reactive::{ draft, Draft, DraftState };
import common::{ StatusBoard, StatusClient };

async fun main() {
	match StatusClient::connect("ws://localhost:9297/", json_codec()) {
		Ok(let client) => {
			// Every commit attempt is numbered, so "exactly one re-push" is
			// observable rather than inferred.
			let attempts: Shared<i32> = Shared::new(0);
			let title = draft(0, |value: i32| {
				let mine = attempts.read() + 1;
				attempts.write() = mine;
				match client.set_status(value) {
					Ok(let confirmed) => {
						print(i"commit:{mine}:ok:{confirmed}");
						None
					},
					Err(let error) => {
						print(i"commit:{mine}:err:{error.debug()}");
						Some("down")
					},
				}
			});

			// THE FEATURE: one line, and the outage stops eating edits.
			client.transport.on_reconnect(|| title.repush());

			// The mirror folds the server's value in. After the restart it
			// carries the NEW server's value (2), which must not clobber the
			// user's un-pushed 7 — the re-push then knowingly overwrites it.
			let _watching_mirror = client.status.sub(|value| {
				print(i"mirror:{value}");
				title.adopt(value);
			});

			let _watching_draft = title.state.sub(|current| {
				print(i"draft:{current.debug()}");
				if current == DraftState::Synced && title.local.get() == 7 {
					print(i"draft:settled:local:{title.local.get()}:synced:{title.synced.read()}:attempts:{attempts.read()}");
					exit(0);
				}
			});

			let edited: Shared<bool> = Shared::new(false);
			let _watching_state = client.transport.connection_state().sub(|current| {
				print(i"state:{current.debug()}");
				// Edit WHILE DOWN: the commit fail-fast rejects, the draft goes
				// Failed, and the user's value survives in `local`.
				if current == ConnectionState::Reconnecting && !edited.read() {
					edited.write() = true;
					title.push(7);
				}
			});

			print("ready");
			// Hold main open through the outage — on node a COMPLETED main
			// exits the process; the success path exits from the draft sub.
			sleep(600000);
		},
		Err(let error) => print(i"connect failed: {error.debug()}"),
	}
}
"#;

#[cfg(unix)]
#[test]
fn a_dirty_draft_repushes_itself_on_reconnect() {
    let fixture = ReconnectFixture::build("draft_repush", DRAFT_CLIENT);
    let wait = Duration::from_secs(20);

    // Phase 1: server up with status 1; the draft adopts it (clean local).
    // The mirror's first value arrives over the wire, so it lands AFTER the
    // wiring finishes — `await_line` consumes what it skips, so the order
    // here mirrors the client's emission order.
    let server = fixture.server("1", "none");
    server.await_line("listening 1", wait);
    let client = fixture.client();
    client.await_line("state:Connected", wait);
    client.await_line("ready", wait);
    client.await_line("mirror:1", wait);

    // Phase 2: kill the server. The drop is noticed, the client edits the
    // draft while down, and that first commit fails fast — the edit is now
    // stranded in `local` with `synced` behind it.
    server.signal("-KILL");
    drop(server);
    client.await_line("state:Reconnecting", wait);
    let stranded = client.await_line("commit:1:", wait);
    assert!(
        stranded.contains("err"),
        "the edit made while down should fail fast, got: {stranded}"
    );

    // Phase 3: a DIFFERENT server comes up (status 2). The backoff reconnects,
    // the mirror resyncs to 2 — which must NOT take the dirty local — and the
    // app's reconnect hook re-pushes the stranded 7.
    let revived = fixture.server("2", "none");
    revived.await_line("listening 2", wait);
    client.await_line("state:Connected", wait);
    client.await_line("mirror:2", wait);

    let repushed = client.await_line("commit:2:", wait);
    assert!(
        repushed.contains("ok:7"),
        "the re-push should carry the stranded edit and succeed, got: {repushed}"
    );

    // The draft settles on the value the user actually typed, and `synced`
    // agrees — the re-push overwrote the restarted server's 2, which is
    // Draft's documented last-write-wins rule. `attempts:2` is the "exactly
    // one re-push" claim, asserted rather than inferred: one failed commit
    // while down, one re-push, and nothing else — no double-fired hook and
    // no retry loop.
    let settled = client.await_line("draft:settled:", wait);
    assert!(
        settled.contains("local:7") && settled.contains("synced:7"),
        "the draft should settle on the re-pushed value, got: {settled}"
    );
    assert!(
        settled.contains("attempts:2"),
        "the outage should cost exactly two commit attempts, got: {settled}"
    );

    fixture.clean();
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
        r#"import std::io::print;
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
