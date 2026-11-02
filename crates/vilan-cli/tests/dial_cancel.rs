//! A179 (proposal/dial-cancel.md §4): a WebSocket dial honours its nursery's
//! cancel signal, and `std::rpc::dial_within` bounds a dial with a deadline.
//!
//! The listeners the dial meets are the HARNESS's own, so every ordering is the
//! harness's sequencing and never a sleep:
//!
//! - a raw TCP listener that accepts and then SHUTS ITS WRITE HALF, so the
//!   first attempt fails before any upgrade; the harness reads the client's own
//!   close (EOF) — the client has seen the failure — and only then tells the
//!   client to cancel, which lands in the backoff;
//! - a raw TCP listener that accepts and never answers the upgrade (the hung
//!   dial A179 is about), held open until the client closes it;
//! - a real vilan service (`std::rpc::server`), bound on `.port(0)` and
//!   announcing its port (`support::port::Server`).
//!
//! The client is told when to cancel through its stdin (`read_line` in a task
//! of the dial's nursery), which the harness writes only after it has OBSERVED
//! the state the cancel must find.
//!
//! Node only: the native backend has no WebSocket client (it refuses the host
//! type `WebSocket`, F1 S1b), so there is no native dial to cancel.
//! `dial_within` itself runs natively over any dial — the native differential
//! `a179_dial_within_answers_the_same_on_both_backends` pins that.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

mod support;

const COMMON: &str = r#"[service(EchoClient)]
struct EchoBoard {
	tag: str,
}

impl EchoBoard {
	[rpc]
	fun echo(self, value: i32): i32 {
		value
	}
}
"#;

/// The service, on a port the OS picks and the server reports
/// (`support::port::ANNOUNCE_PORT`) — or, restarted, on the port it had.
fn server_source() -> String {
    format!(
        r#"import std::http::{{ Response, Server }};
import std::io::print;
import std::json::json_codec;
import std::option::Option::{{ None, Some }};
import std::process::args;
import std::rpc::server::Service;
import common::EchoBoard;

async fun main() {{
	let board = EchoBoard {{ tag = "echo" }};
	// `argv[0]` when the harness restarts the server where it was.
	let port = match args().get(0) {{
		Some(let raw) => raw.parse_i32().unwrap(),
		None => 0,
	}};
	Server::builder()
		.port(port)
		.with_service(Service::new(board.dispatcher().into_protocol(json_codec())))
		.on_request(|request| Response::builder().code(404).body("nope").build())
		.on_start(|server| {announce})
		.build()
		.start();
}}
"#,
        announce = support::port::ANNOUNCE_PORT
    )
}

/// One program, five dials, chosen by `argv[0]`; `argv[1]` is the port.
const CLIENT: &str = r#"import std::io::print;
import std::json::json_codec;
import std::process::{ args, exit, read_line };
import std::reactive::Source;
import std::result::Result::{ self, Err, Ok };
import std::rpc::{ ConnectionState, RpcError, connect_socket, dial_within };
import std::shared::Shared;
import std::task::nursery;
import std::time::{ Timer, sleep };
import common::EchoClient;

async fun main() {
	let mode = args().get(0).unwrap();
	let port = args().get(1).unwrap();
	let url = i"ws://127.0.0.1:{port}/";
	if mode == "socket" {
		// The untyped door, cancelled when the harness says so.
		let answer = nursery(|n| {
			let dialled = async connect_socket(url);
			async {
				read_line();
				n.cancel();
			};
			await dialled
		});
		match answer {
			Ok(let _duplex) => print("socket:open"),
			Err(let reason) => print(i"socket:err:{reason}"),
		}
		// Alive until the harness has checked its listener: a socket closed by
		// the process exiting would prove nothing.
		read_line();
	} else if mode == "service" {
		// The generated client's dial, cancelled when the harness says so.
		let answer = nursery(|n| {
			let dialled = async EchoClient::connect(url, json_codec());
			async {
				read_line();
				n.cancel();
			};
			await dialled
		});
		match answer {
			Ok(let _client) => print("service:open"),
			Err(let error) => print(i"service:err:{error.debug()}"),
		}
		read_line();
	} else if mode == "within" {
		match dial_within(300, || EchoClient::connect(url, json_codec())) {
			Ok(let _client) => print("within:open"),
			Err(let error) => print(i"within:err:{error.debug()}"),
		}
		read_line();
	} else if mode == "within-open" {
		// A deadline the dial beats: the timer must not fire on the connection
		// later. Waiting past the deadline is this program's own clock (the
		// claim is about what happens AFTER it), not the harness's ordering.
		match dial_within(2000, || EchoClient::connect(url, json_codec())) {
			Ok(let client) => {
				print("within-open:open");
				Timer::after(2500).wait();
				print(i"within-open:state:{client.transport.connection_state().get().debug()}");
				match client.echo(7) {
					Ok(let value) => print(i"within-open:call:{value}"),
					Err(let error) => print(i"within-open:call-err:{error.debug()}"),
				}
			},
			Err(let error) => print(i"within-open:err:{error.debug()}"),
		}
	} else if mode == "opened" {
		// The abort lands AFTER the socket opened: not the dial's business.
		let answer = nursery(|n| {
			let connected = EchoClient::connect(url, json_codec());
			n.cancel();
			connected
		});
		match answer {
			Ok(let client) => {
				print(i"opened:state:{client.transport.connection_state().get().debug()}");
				match client.echo(9) {
					Ok(let value) => print(i"opened:call:{value}"),
					Err(let error) => print(i"opened:call-err:{error.debug()}"),
				}
			},
			Err(let error) => print(i"opened:err:{error.debug()}"),
		}
	} else if mode == "redial" {
		// Connected inside a nursery that is then cancelled — the race idiom's
		// aftermath — and later the server goes away and comes back: the
		// redial must not read that cancelled nursery's signal.
		let answer = nursery(|n| {
			let connected = EchoClient::connect(url, json_codec());
			n.cancel();
			connected
		});
		match answer {
			Ok(let client) => {
				let dropped: Shared<bool> = Shared::new(false);
				let watching = client.transport.connection_state().sub(|current| {
					if current == ConnectionState::Reconnecting {
						dropped.write() = true;
						print("redial:reconnecting");
					} else if current == ConnectionState::Connected && dropped.read() {
						print("redial:connected");
						match client.echo(11) {
							Ok(let value) => print(i"redial:call:{value}"),
							Err(let error) => print(i"redial:call-err:{error.debug()}"),
						}
						exit(0);
					} else if current == ConnectionState::Closed {
						print("redial:closed");
						exit(0);
					}
				});
				print("redial:ready");
				// Main stays open through the outage; the subscription exits.
				sleep(600000);
			},
			Err(let error) => print(i"redial:err:{error.debug()}"),
		}
	}
	exit(0);
}
"#;

/// The built project: one `vilan build` per test PROCESS (nextest runs each
/// test in its own, so the directory is keyed by the process id; `cargo test`
/// shares one build among the threads).
struct Built {
    directory: PathBuf,
}

fn built() -> &'static Built {
    static BUILT: OnceLock<Built> = OnceLock::new();
    BUILT.get_or_init(|| {
        let directory =
            support::scratch_dir(&format!("vilan_dial_cancel_{}", std::process::id()));
        let write = |relative: &str, contents: &str| {
            let path = directory.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        };
        write(
            "vilan.toml",
            "[project]\npackages = [\"common\", \"server\", \"client\"]\n",
        );
        write("common/vilan.toml", "[library]\nname = \"common\"\n");
        let package = |name: &str| {
            format!(
                "[package]\nname = \"{name}\"\ntarget = \"node\"\n\n[package.dependencies]\ncommon = {{ path = \"../common\" }}\n"
            )
        };
        write("server/vilan.toml", &package("server"));
        write("client/vilan.toml", &package("client"));
        write("common/src/lib.vl", COMMON);
        write("server/src/main.vl", &server_source());
        write("client/src/main.vl", CLIENT);
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
        Built { directory }
    })
}

/// The client under test: stdin held for the cancel, stdout streamed.
struct Client {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
}

impl Client {
    fn spawn(mode: &str, port: u16) -> Client {
        let mut child = Command::new("node")
            .arg(built().directory.join("dist/client.mjs"))
            .args([mode, &port.to_string()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn node");
        let stdout = child.stdout.take().unwrap();
        let (sender, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let stdin = child.stdin.take();
        Client {
            child,
            stdin,
            lines,
        }
    }

    /// Tell the client to cancel its dial's nursery.
    fn cancel(&mut self) {
        let stdin = self.stdin.as_mut().expect("a client still waiting");
        stdin.write_all(b"cancel\n").expect("write the cancel");
        stdin.flush().expect("flush the cancel");
    }

    /// The next stdout line, bounded by the liveness budget.
    fn next_line(&self) -> String {
        self.lines
            .recv_timeout(support::run_liveness())
            .expect("the client printed nothing")
    }

    /// Let a client that waits for the harness exit.
    fn release(&mut self) {
        let mut stdin = self.stdin.take().expect("release once");
        stdin.write_all(b"exit\n").expect("write the release");
        stdin.flush().expect("flush the release");
    }

    /// Every stdout line until the process exits. A liveness bound, not a
    /// claim about speed: a green run returns when the client exits.
    fn output(mut self) -> Vec<String> {
        let deadline = Instant::now() + support::run_liveness();
        let mut seen = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "the client did not finish; saw {seen:?}");
            match self.lines.recv_timeout(remaining) {
                Ok(line) => seen.push(line),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    panic!("the client did not finish; saw {seen:?}")
                }
            }
        }
        let status = self.child.wait().expect("reap the client");
        assert!(status.success(), "the client failed ({status}); saw {seen:?}");
        seen
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The longest the harness waits for a hung dial's socket to close — under the
/// 300 s at which node's own handshake timeout would close it regardless.
const HUNG_UPGRADE_BOUND: Duration = Duration::from_secs(120);

/// A listener of the harness's own, on a port the OS picked and the listener
/// still holds.
fn listener() -> (TcpListener, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a listener");
    let port = listener.local_addr().expect("the bound address").port();
    (listener, port)
}

/// Accept one connection, bounded by the liveness budget.
fn accept(listener: &TcpListener) -> TcpStream {
    listener
        .set_nonblocking(false)
        .expect("a blocking listener");
    let (sender, receiver) = channel();
    let cloned = listener.try_clone().expect("clone the listener");
    std::thread::spawn(move || {
        let _ = sender.send(cloned.accept().map(|(stream, _)| stream));
    });
    receiver
        .recv_timeout(support::run_liveness())
        .expect("the client never connected")
        .expect("accept the client")
}

/// Read until the peer closes: EOF, or a reset — either is the client's socket
/// going away. The read timeout is a liveness bound (a green run closes in
/// milliseconds), held under undici's 300 s handshake timeout: past that, node
/// closes a hung upgrade on its own, and a dial that never closed its socket
/// would read as one that did.
fn read_until_closed(stream: &mut TcpStream) -> Vec<u8> {
    stream
        .set_read_timeout(Some(support::run_liveness().min(HUNG_UPGRADE_BOUND)))
        .expect("bound the read");
    let mut received = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => return received,
            Ok(count) => received.extend_from_slice(&buffer[..count]),
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => return received,
            Err(error) => panic!(
                "the client's socket was never closed ({error}); it sent {:?}",
                String::from_utf8_lossy(&received)
            ),
        }
    }
}

/// The dial did not try again: no further connection waiting on `listener`
/// carries an upgrade request.
///
/// A further connection may be waiting, and that is node's, not the dial's:
/// closing a `WebSocket` while it is CONNECTING makes undici's pool open one
/// more TCP connection, which sends nothing and is closed after the pool's 4 s
/// keep-alive (reproduced with a bare `new WebSocket(..)` and `close()`, no vilan
/// involved). So each one is read to its close and must have said nothing.
fn no_further_upgrade(listener: &TcpListener) {
    listener
        .set_nonblocking(true)
        .expect("a non-blocking listener");
    loop {
        match listener.accept() {
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return,
            Ok((mut stream, from)) => {
                stream.set_nonblocking(false).expect("a blocking stream");
                let request = read_until_closed(&mut stream);
                assert!(
                    request.is_empty(),
                    "the dial attempted again (from {from}) after it was cancelled: {:?}",
                    String::from_utf8_lossy(&request)
                );
            }
            Err(error) => panic!("accept failed: {error}"),
        }
    }
}

/// §4 (1): the first attempt fails (the listener closes on it before any
/// upgrade), the harness sees the client close its side — the failure is
/// observed — and only then cancels, so the cancel lands in the backoff's
/// wait. The dial ends there with `Cancelled` instead of counting a failed
/// attempt and dialling again: one connection, ever, and the untyped door's
/// sentence.
#[test]
fn a_cancel_during_the_backoff_ends_the_dial_with_cancelled() {
    let (listener, port) = listener();
    let mut client = Client::spawn("socket", port);
    let mut first = accept(&listener);
    first
        .shutdown(std::net::Shutdown::Write)
        .expect("refuse the upgrade");
    let request = read_until_closed(&mut first);
    assert!(
        String::from_utf8_lossy(&request).starts_with("GET / HTTP/1.1"),
        "the client sent its upgrade first: {:?}",
        String::from_utf8_lossy(&request)
    );
    client.cancel();
    assert_eq!(client.next_line(), "socket:err:the dial was cancelled");
    // The dial is over while the client is still alive: nothing dials again.
    no_further_upgrade(&listener);
    client.release();
    assert!(client.output().is_empty());
}

/// §4 (2): a listener that accepts TCP and never answers the upgrade — the hung
/// dial. Cancelled once the harness HOLDS the connection, the dial closes the
/// half-open socket (the listener sees it go) and the generated client's
/// `connect` answers `RpcError::Cancelled`.
#[test]
fn a_cancelled_hung_dial_closes_its_half_open_socket() {
    let (listener, port) = listener();
    let mut client = Client::spawn("service", port);
    let mut held = accept(&listener);
    client.cancel();
    assert_eq!(client.next_line(), "service:err:RpcError::Cancelled");
    // The client is alive and waiting: the close is the dial's, not the exit's.
    let request = read_until_closed(&mut held);
    assert!(
        String::from_utf8_lossy(&request).starts_with("GET / HTTP/1.1"),
        "the client sent its upgrade: {:?}",
        String::from_utf8_lossy(&request)
    );
    no_further_upgrade(&listener);
    client.release();
    assert!(client.output().is_empty());
}

/// §4 (3): `dial_within` against the same hung listener answers
/// `Err(RpcError::Timeout)`, and the listener sees the socket closed — the
/// cancelled dial left nothing running.
#[test]
fn dial_within_times_out_a_hung_dial_and_closes_its_socket() {
    let (listener, port) = listener();
    let mut client = Client::spawn("within", port);
    let mut held = accept(&listener);
    assert_eq!(client.next_line(), "within:err:RpcError::Timeout");
    // The client is alive and waiting: the close is the cancelled dial's.
    let request = read_until_closed(&mut held);
    assert!(
        String::from_utf8_lossy(&request).starts_with("GET / HTTP/1.1"),
        "the client sent its upgrade: {:?}",
        String::from_utf8_lossy(&request)
    );
    no_further_upgrade(&listener);
    client.release();
    assert!(client.output().is_empty());
}

fn echo_server() -> support::port::Server {
    support::port::Server::spawn(
        Command::new("node").arg(built().directory.join("dist/server.mjs")),
    )
}

/// The echo server again, on the port a stopped one had.
fn echo_server_on(port: u16) -> support::port::Server {
    support::port::Server::spawn(
        Command::new("node")
            .arg(built().directory.join("dist/server.mjs"))
            .arg(port.to_string()),
    )
}

/// §4 (4): a dial that beats its deadline answers `Ok`, and the deadline does
/// not fire on the connection afterwards — past it, the duplex is still
/// `Connected` and a call through it succeeds.
#[test]
fn dial_within_answers_a_dial_that_wins_and_leaves_its_connection_alone() {
    let server = echo_server();
    let output = Client::spawn("within-open", server.port()).output();
    assert_eq!(
        output,
        [
            "within-open:open",
            "within-open:state:ConnectionState::Connected",
            "within-open:call:7",
        ]
    );
}

/// §4 (5): an abort that lands after the socket opened leaves the duplex
/// `Connected`: the dial's abort listener is gone the moment it settled.
#[test]
fn an_abort_after_the_socket_opened_leaves_the_duplex_connected() {
    let server = echo_server();
    let output = Client::spawn("opened", server.port()).output();
    assert_eq!(
        output,
        ["opened:state:ConnectionState::Connected", "opened:call:9"]
    );
}

/// §2.1's last arm, the redial: a connection opened inside a nursery that was
/// then cancelled (the race idiom's aftermath) still reconnects after the server
/// goes away and comes back. The socket's drop handler captured that nursery as
/// its ambient, so a redial reading the ambient signal would be cancelled at its
/// first attempt and close the duplex for good; it waits on no signal at all.
#[test]
fn a_connection_from_a_cancelled_nursery_still_redials() {
    let mut server = echo_server();
    let port = server.port();
    let client = Client::spawn("redial", port);
    assert_eq!(client.next_line(), "redial:ready");
    server.stop();
    let _revived = echo_server_on(port);
    let output = client.output();
    assert_eq!(
        output,
        ["redial:reconnecting", "redial:connected", "redial:call:11"]
    );
}
