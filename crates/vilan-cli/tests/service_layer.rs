//! End-to-end pins for the server-that-grows layer (`fullstack-dx.md` §4, S1):
//! `Service` + `ServerBuilder::with_service` folds an rpc service's routes and
//! upgrade handshake in front of the app's own `on_request`/`on_upgrade`,
//! longest mount first, independent of call order — and the segment match
//! fixes the old `path.starts_with(…)` collision (§10.8).
//!
//! `rpc_http.rs`, `transport_robustness.rs` and `streaming.rs` are the wire
//! pin for `serve_rpc`/`serve_service`/`serve_connected` themselves (unchanged
//! programs, unchanged output); this file pins what's NEW — the layer those
//! three functions are now four-line bodies over.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

mod support;

fn temp_project(tag: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("vilan_service_layer_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// Runs `vilan run <dir>` under a liveness bound and returns its stdout — the
/// same pattern (and the same E40 rationale for the bound) as `rpc_http.rs`.
fn vilan_run_with_liveness_bound(dir: &Path) -> String {
    let liveness = support::run_liveness();
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
                panic!(
                    "the build+run did not exit within {liveness:?} (a liveness bound, {:?} \
                     per reference compile on this machine — server hung?)",
                    support::reference_compile()
                );
            }
            None => std::thread::sleep(Duration::from_millis(100)),
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
    let unexpected: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert!(
        unexpected.is_empty(),
        "vilan run wrote to stderr:\n{stderr}\nstdout:\n{stdout}"
    );
    stdout
}

#[test]
fn services_answer_before_on_request_regardless_of_call_order() {
    // Rule 1 (§4.3): "services first, then on_request, always" — and the fold
    // is computed at build(), so it must not matter whether `.with_service`
    // was written before or after `.on_request` in the chain. Two servers,
    // opposite orders, both routed to a service that would return a DIFFERENT
    // (and decodable) answer than the on_request fallback ever could.
    let dir = temp_project("order");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    write(
        &dir,
        "src/main.vl",
        r#"import std::print;
import std::shared::Shared;
import std::process::exit;
import std::result::Result::{ self, Ok, Err };
import std::json::json_codec;
import std::rpc::HttpTransport;
import std::http::{ Response, Server };
import std::rpc_server::Service;

[service(Client)]
struct Counter {
	count: Shared<i32>,
}

impl Counter {
	[rpc]
	fun add(self, by: i32): i32 {
		self.count.write() = self.count.read() + by;
		self.count.read()
	}
}

fun main() {
	let a = Counter { count = Shared::new(0) };
	let b = Counter { count = Shared::new(0) };
	// `.with_service` BEFORE `.on_request`.
	Server::builder()
		.port(0)
		.with_service(Service::new(a.dispatcher().into_protocol(json_codec())))
		.on_request(|request| Response::builder().code(404).body("on_request:fallback").build())
		.on_start(|server_a| {
			// `.with_service` AFTER `.on_request` — the opposite order.
			Server::builder()
				.port(0)
				.on_request(|request| Response::builder().code(404).body("on_request:fallback").build())
				.with_service(Service::new(b.dispatcher().into_protocol(json_codec())))
				.on_start(|server_b| run(server_a.port(), server_b.port()))
				.build()
				.start();
		})
		.build()
		.start();
}

fun run(port_a: i32, port_b: i32) {
	let client_a = Client { transport = HttpTransport { url = i"http://localhost:{port_a}/rpc" }, codec = json_codec() };
	let client_b = Client { transport = HttpTransport { url = i"http://localhost:{port_b}/rpc" }, codec = json_codec() };
	match client_a.add(5) {
		Ok(let n) => print(i"a:ok:{n}"),
		Err(let error) => print(i"a:err:{error.to_json()}"),
	}
	match client_b.add(7) {
		Ok(let n) => print(i"b:ok:{n}"),
		Err(let error) => print(i"b:err:{error.to_json()}"),
	}
	exit(0);
}
"#,
    );
    let stdout = vilan_run_with_liveness_bound(&dir);
    assert!(
        stdout.contains("a:ok:5"),
        "with_service written BEFORE on_request did not answer the service:\n{stdout}"
    );
    assert!(
        stdout.contains("b:ok:7"),
        "with_service written AFTER on_request did not answer the service:\n{stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn two_services_on_distinct_mounts_each_answer_their_own() {
    // §4.4: two services, two mounts, one server — each answers only its own
    // routes. The cross-mount call (Notes's client dialed at Board's mount)
    // must fail cleanly rather than silently succeed against the wrong
    // dispatcher, which is the proof routing isn't just "first service wins".
    let dir = temp_project("mounts");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    write(
        &dir,
        "src/main.vl",
        r#"import std::print;
import std::shared::Shared;
import std::process::exit;
import std::result::Result::{ self, Ok, Err };
import std::json::json_codec;
import std::rpc::HttpTransport;
import std::http::{ Response, Server };
import std::rpc_server::Service;

[service(NotesClient)]
struct Notes {
	count: Shared<i32>,
}

impl Notes {
	[rpc]
	fun add(self, by: i32): i32 {
		self.count.write() = self.count.read() + by;
		self.count.read()
	}
}

[service(BoardClient)]
struct Board {
	count: Shared<i32>,
}

impl Board {
	[rpc]
	fun echo(self, value: i32): i32 {
		value
	}
}

fun main() {
	let notes = Notes { count = Shared::new(0) };
	let board = Board { count = Shared::new(0) };
	Server::builder()
		.port(0)
		.with_service(Service::new(notes.dispatcher().into_protocol(json_codec())))
		.with_service(Service::new(board.dispatcher().into_protocol(json_codec())).at("/admin/"))
		.on_request(|request| Response::builder().code(404).body(i"fallback:{request.path()}").build())
		.on_start(|server| run(server.port()))
		.build()
		.start();
}

fun run(port: i32) {
	let notes_client = NotesClient { transport = HttpTransport { url = i"http://localhost:{port}/rpc" }, codec = json_codec() };
	let board_client = BoardClient { transport = HttpTransport { url = i"http://localhost:{port}/admin/rpc" }, codec = json_codec() };
	match notes_client.add(3) {
		Ok(let n) => print(i"notes:{n}"),
		Err(let error) => print(i"notes:err:{error.to_json()}"),
	}
	match board_client.echo(9) {
		Ok(let n) => print(i"board:{n}"),
		Err(let error) => print(i"board:err:{error.to_json()}"),
	}
	// Cross-mount: Notes's client, dialed at Board's route — Board's dispatcher
	// has no "add" method, so this must fail, not silently answer.
	let cross = NotesClient { transport = HttpTransport { url = i"http://localhost:{port}/admin/rpc" }, codec = json_codec() };
	match cross.add(1) {
		Ok(let n) => print(i"cross:ok:{n}"),
		Err(let error) => print(i"cross:err:{error.to_json()}"),
	}
	exit(0);
}
"#,
    );
    let stdout = vilan_run_with_liveness_bound(&dir);
    assert!(
        stdout.contains("notes:3"),
        "the root mount did not answer its own service:\n{stdout}"
    );
    assert!(
        stdout.contains("board:9"),
        "the /admin/ mount did not answer its own service:\n{stdout}"
    );
    assert!(
        stdout.contains("cross:err:"),
        "a client for one mount's protocol reached the OTHER mount's dispatcher instead of failing cleanly:\n{stdout}"
    );
    assert!(
        !stdout.contains("cross:ok:"),
        "a cross-mount call must not succeed — routing let it answer through the wrong service:\n{stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_upgrade_routes_to_its_mounts_service() {
    // §4.4: the upgrade handler is one dispatcher over every mounted service,
    // picked by the upgrade request's PATH. A socket opened at `/admin/` must
    // reach Board's protocol, not Notes's (mounted at `/`) — proven by calling
    // an rpc method that exists on only one side of the mount.
    let dir = temp_project("upgrade");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    write(
        &dir,
        "src/main.vl",
        r#"import std::print;
import std::shared::Shared;
import std::process::exit;
import std::result::Result::{ self, Ok, Err };
import std::json::json_codec;
import std::rpc::connect_socket;
import std::http::{ Response, Server };
import std::rpc_server::Service;

[service(NotesClient)]
struct Notes {
	count: Shared<i32>,
}

impl Notes {
	[rpc]
	fun add(self, by: i32): i32 {
		self.count.write() = self.count.read() + by;
		self.count.read()
	}
}

[service(BoardClient)]
struct Board {
	count: Shared<i32>,
}

impl Board {
	[rpc]
	fun echo(self, value: i32): i32 {
		value
	}
}

fun main() {
	let notes = Notes { count = Shared::new(0) };
	let board = Board { count = Shared::new(0) };
	Server::builder()
		.port(0)
		.with_service(Service::new(notes.dispatcher().into_protocol(json_codec())))
		.with_service(Service::new(board.dispatcher().into_protocol(json_codec())).at("/admin/"))
		.on_request(|request| Response::builder().code(404).body("nope").build())
		.on_start(|server| run(server.port()))
		.build()
		.start();
}

fun run(port: i32) {
	match connect_socket(i"ws://localhost:{port}") {
		Ok(let socket) => {
			let client = NotesClient { transport = socket.transport(), codec = json_codec() };
			match client.add(4) {
				Ok(let n) => print(i"notes-over-ws:{n}"),
				Err(let error) => print(i"notes-over-ws:err:{error.to_json()}"),
			}
		},
		Err(let reason) => print(i"notes-connect-err:{reason}"),
	}
	match connect_socket(i"ws://localhost:{port}/admin/") {
		Ok(let socket) => {
			let client = BoardClient { transport = socket.transport(), codec = json_codec() };
			match client.echo(11) {
				Ok(let n) => print(i"board-over-ws:{n}"),
				Err(let error) => print(i"board-over-ws:err:{error.to_json()}"),
			}
		},
		Err(let reason) => print(i"board-connect-err:{reason}"),
	}
	exit(0);
}
"#,
    );
    let stdout = vilan_run_with_liveness_bound(&dir);
    assert!(
        stdout.contains("notes-over-ws:4"),
        "the default mount's upgrade did not reach Notes's protocol:\n{stdout}"
    );
    assert!(
        stdout.contains("board-over-ws:11"),
        "the /admin/ mount's upgrade did not reach Board's protocol — either it fell through to \
         the default service or the upgrade dispatcher isn't routing by mount at all:\n{stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// --- Raw-socket helpers, for the two pins below that check exact bytes on the
// wire rather than a round-tripped rpc call. ------------------------------------

/// A long-running server, spawned with `node` directly against a built bundle
/// (mirrors `rpc_http.rs`'s `StreamingServer`) so a Rust test can speak raw
/// HTTP to it from outside the process.
struct StreamingServer {
    child: std::process::Child,
    lines: std::sync::mpsc::Receiver<String>,
}

impl StreamingServer {
    fn spawn(dir: &Path) -> StreamingServer {
        let build = Command::new(env!("CARGO_BIN_EXE_vilan"))
            .args(["build", dir.to_str().unwrap()])
            .output()
            .expect("run vilan build");
        assert!(
            build.status.success(),
            "server build failed:\n{}{}",
            String::from_utf8_lossy(&build.stdout),
            String::from_utf8_lossy(&build.stderr)
        );
        let mut child = Command::new("node")
            .arg(dir.join("src/main.mjs"))
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
        StreamingServer { child, lines }
    }

    fn await_line(&self, needle: &str, timeout: Duration) -> String {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "timed out waiting for `{needle}` from the server"
            );
            match self.lines.recv_timeout(remaining) {
                Ok(line) if line.contains(needle) => return line,
                Ok(_other) => {}
                Err(_) => panic!("server stdout ended or timed out before `{needle}`"),
            }
        }
    }
}

impl Drop for StreamingServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One request/response over a fresh connection the SERVER closes (the request
/// must carry `Connection: close`) — the raw response text, headers and body
/// together, exactly as they arrived.
fn raw_http_closed(port: u16, request: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect to the reported port");
    stream
        .write_all(request.as_bytes())
        .expect("send the request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read the response");
    response
}

/// One request over a fresh connection that reads for up to `timeout` and
/// returns whatever arrived — for a STREAMING response (SSE) that never closes
/// on its own, so `read_to_string` would hang.
fn raw_http_bounded(port: u16, request: &str, timeout: Duration) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect to the reported port");
    stream
        .set_read_timeout(Some(timeout))
        .expect("set a read timeout");
    stream
        .write_all(request.as_bytes())
        .expect("send the request");
    let mut buffer = [0u8; 4096];
    let mut response = Vec::new();
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => response.extend_from_slice(&buffer[..n]),
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut =>
            {
                break;
            }
            Err(error) => panic!("read error: {error}"),
        }
    }
    String::from_utf8_lossy(&response).into_owned()
}

const BYTE_IDENTICAL_SERVER: &str = r#"import std::print;
import std::shared::Shared;
import std::json::json_codec;
import std::http::Response;
import std::rpc_server::serve_service;

[service(Client)]
struct Counter {
	count: Shared<i32>,
}

impl Counter {
	[rpc]
	fun add(self, by: i32): i32 {
		self.count.write() = self.count.read() + by;
		self.count.read()
	}
}

fun main() {
	let counter = Counter { count = Shared::new(0) };
	serve_service(0, counter.dispatcher().into_protocol(json_codec()), |request| {
		Response::builder().code(404).body(i"fallback:{request.path()}").build()
	}, |server| print(i"ready {server.port()}"));
}
"#;

#[test]
fn serve_service_over_the_layer_is_byte_identical_on_the_wire() {
    // §4.6/§8: `serve_service` becomes a four-line body over `with_service`,
    // and the claim is that nothing about the wire moved. Pinned as EXACT
    // status lines, header values and bodies for the three mounted routes
    // plus the fallback — captured against the pre-layer implementation
    // (2026-08-11) and reproduced here byte for byte (the connection id is
    // deterministic: `next_connection` starts at 0 per fresh process).
    let dir = temp_project("byte_identical");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    write(&dir, "src/main.vl", BYTE_IDENTICAL_SERVER);

    let server = StreamingServer::spawn(&dir);
    let ready = server.await_line("ready", Duration::from_secs(60));
    let port: u16 = ready
        .split_whitespace()
        .next_back()
        .expect("the ready line carries the bound port")
        .parse()
        .expect("the announced port is a number");

    let rpc_response = raw_http_closed(
        port,
        "POST /rpc HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
    );
    assert!(
        rpc_response.starts_with("HTTP/1.1 200 OK\r\n"),
        "/rpc status line moved:\n{rpc_response}"
    );
    assert!(
        rpc_response.contains("Content-Type: application/json\r\n"),
        "/rpc content type moved:\n{rpc_response}"
    );
    assert!(
        rpc_response.ends_with("{\"Failure\":{\"Decode\":\"missing field 'method'\"}}"),
        "/rpc decode-failure envelope moved:\n{rpc_response}"
    );

    let send_response = raw_http_closed(
        port,
        "POST /send?c=99 HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello",
    );
    assert!(
        send_response.starts_with("HTTP/1.1 204 No Content\r\n"),
        "/send status line moved:\n{send_response}"
    );
    assert!(
        send_response.contains("Content-Type: text/plain\r\n"),
        "/send's default content type moved:\n{send_response}"
    );
    assert!(
        send_response.ends_with("\r\n\r\n"),
        "/send must still carry an empty body:\n{send_response}"
    );

    let events_response = raw_http_bounded(
        port,
        "GET /events HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
        Duration::from_millis(800),
    );
    assert!(
        events_response.starts_with("HTTP/1.1 200 OK\r\n"),
        "/events status line moved:\n{events_response}"
    );
    assert!(
        events_response.contains("Content-Type: text/event-stream\r\n"),
        "/events content type moved:\n{events_response}"
    );
    assert!(
        events_response.contains("Cache-Control: no-cache\r\n"),
        "/events cache-control header moved:\n{events_response}"
    );
    assert!(
        events_response.contains("data: __conn:0\n\n"),
        "the first SSE frame's announcement moved:\n{events_response}"
    );

    let fallback_response = raw_http_closed(
        port,
        "GET /nonexistent HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(
        fallback_response.starts_with("HTTP/1.1 404 Not Found\r\n"),
        "the fallback status line moved:\n{fallback_response}"
    );
    assert!(
        fallback_response.ends_with("fallback:/nonexistent"),
        "the fallback body moved:\n{fallback_response}"
    );

    drop(server);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_segment_match_lets_rpcs_through_where_starts_with_swallowed_it() {
    // §10.8: `path.starts_with("/rpc")` used to answer `/rpcs` (and
    // `/sendmail`, `/events-archive`) through the rpc protocol instead of the
    // app's own route. The layer matches a full path SEGMENT, so `/rpcs` must
    // now reach `on_request` — pinned by a fallback that echoes the exact path
    // it was called with, and a sibling check that the REAL route is untouched.
    let dir = temp_project("segment");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    write(&dir, "src/main.vl", BYTE_IDENTICAL_SERVER);

    let server = StreamingServer::spawn(&dir);
    let ready = server.await_line("ready", Duration::from_secs(60));
    let port: u16 = ready
        .split_whitespace()
        .next_back()
        .expect("the ready line carries the bound port")
        .parse()
        .expect("the announced port is a number");

    for (path, expected_status, expected_body) in [
        ("/rpcs", "HTTP/1.1 404 Not Found", "fallback:/rpcs"),
        ("/sendmail", "HTTP/1.1 404 Not Found", "fallback:/sendmail"),
        (
            "/events-archive",
            "HTTP/1.1 404 Not Found",
            "fallback:/events-archive",
        ),
    ] {
        let response = raw_http_closed(
            port,
            &format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"),
        );
        assert!(
            response.starts_with(expected_status),
            "`{path}` should reach on_request (the old starts_with match swallowed it into the \
             rpc route): {response}"
        );
        assert!(
            response.ends_with(expected_body),
            "`{path}`'s fallback body is wrong: {response}"
        );
    }

    // Sanity: the real route is untouched by the fix.
    let real = raw_http_closed(
        port,
        "POST /rpc HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
    );
    assert!(
        real.starts_with("HTTP/1.1 200 OK\r\n") && real.contains("application/json"),
        "the real /rpc route must still be answered by the service: {real}"
    );

    drop(server);
    let _ = std::fs::remove_dir_all(&dir);
}
