//! End-to-end pins for the server-that-grows layer (`fullstack-dx.md` §4, S1):
//! `Service` + `ServerBuilder::with_service` folds an rpc service's routes and
//! upgrade handshake in front of the app's own `on_request`/`on_upgrade`,
//! longest mount first, independent of call order — and the segment match
//! fixes the old `path.starts_with(…)` collision (§10.8).
//!
//! `rpc_http.rs`, `transport_robustness.rs` and `streaming.rs` drive the
//! layer's routes and lifecycle over real wires; this file pins the layer's
//! own contract — the fold, the mounts, the segment match, and the recorded
//! wire bytes (below) that survive the retirement of the `serve_rpc`/
//! `serve_service`/`serve_connected` boot functions the layer replaced (E71).

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
        .filter(|line| !line.is_empty() && !is_node_windows_teardown_noise(line))
        .collect();
    assert!(
        unexpected.is_empty(),
        "vilan run wrote to stderr:\n{stderr}\nstdout:\n{stdout}"
    );
    stdout
}

/// Windows only: node's own shutdown race, not output from the program —
/// `uv_async_send` aborting on a closing handle during exit teardown
/// (nodejs/node#56645 / #58091; `rpc_http.rs` documents the mechanism and
/// this exact tolerance, and the css e2e carries its sentinel twin). The
/// abort lands strictly AFTER the program's complete stdout, which every
/// caller here asserts on; exactly this assertion line is tolerated, and
/// anything else on stderr still fails the test.
fn is_node_windows_teardown_noise(line: &str) -> bool {
    cfg!(windows)
        && line.starts_with("Assertion failed: !(handle->flags & UV_HANDLE_CLOSING)")
        && line.contains("async.c")
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
        r#"import std::io::print;
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
        r#"import std::io::print;
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
        r#"import std::io::print;
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

/// The wire-contract server: one service on `/`, a path-echoing fallback. Its
/// responses are pinned below as literal bytes recorded BEFORE the layer
/// existed, so the wire contract survives each respelling of the boot code —
/// pre-layer `serve_service` (captured 2026-08-11), then `serve_service` as
/// sugar over the layer, now the builder chain itself (`serve_service`
/// retired 2026-08-20, E71).
const BYTE_IDENTICAL_SERVER: &str = r#"import std::io::print;
import std::shared::Shared;
import std::json::json_codec;
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
	let counter = Counter { count = Shared::new(0) };
	Server::builder()
		.port(0)
		.with_service(Service::new(counter.dispatcher().into_protocol(json_codec())))
		.on_request(|request| Response::builder().code(404).body(i"fallback:{request.path()}").build())
		.on_start(|server| print(i"ready {server.port()}"))
		.build()
		.start();
}
"#;

#[test]
fn the_builders_wire_matches_the_bytes_recorded_from_serve_service() {
    // §4.6/§8's claim was that `serve_service` over the layer moved nothing
    // on the wire, pinned as EXACT status lines, header values and bodies for
    // the three mounted routes plus the fallback — captured against the
    // pre-layer implementation (2026-08-11). E71 retired `serve_service`
    // (2026-08-20); the recorded bytes below are that capture, unchanged, and
    // the builder chain — the layer the trio was sugar over — must still
    // serve every one of them (the connection id stays deterministic:
    // `next_connection` starts at 0 per fresh process).
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

// --- A38: one service instance per connection (`Service::factory`) -------------

/// The factory server used by the two pins below: `Notes` is built once per
/// connection, so its counter and its `[expose]`d mirror belong to that client
/// alone, and `whoami` can answer from the `Connection` the factory closed over
/// — the identity a method could not learn when one instance served the whole
/// process (`transport-rpc.md` Q9).
const FACTORY_SERVER: &str = r#"import std::io::print;
import std::process::exit;
import std::reactive::{ Signal, SignalCell };
import std::result::Result::{ self, Ok, Err };
import std::json::json_codec;
import std::http::{ Response, Server };
import std::rpc_server::{ Connection, Service };

[service(NotesClient)]
struct Notes {
	who: str,
	[expose] count: SignalCell<i32>,
}

impl Notes {
	[rpc]
	fun add(self, by: i32): i32 {
		self.count.set(self.count.get() + by);
		self.count.get()
	}

	[rpc]
	fun whoami(self): str {
		self.who
	}
}
"#;

#[test]
fn a_factory_service_builds_one_instance_per_connection() {
    // A38: `Service::factory(build, codec)` calls `build` once per connection
    // and every route of that connection's dispatcher captures ITS instance.
    // Two clients therefore count separately, see their own `[expose]`d
    // mirror, and read back their own identity — none of which is expressible
    // when `Service::new`'s single protocol answers every connection.
    let dir = temp_project("factory");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    write(
        &dir,
        "src/main.vl",
        &format!(
            "{FACTORY_SERVER}{}",
            r#"
fun main() {
	Server::builder()
		.port(0)
		.with_service(Service::factory(|connection: Connection| Notes {
			who = i"conn-{connection.id}",
			count = Signal::new(0),
		}, json_codec()))
		.on_request(|request| Response::builder().code(404).body("nope").build())
		.on_start(|server| run(server.port()))
		.build()
		.start();
}

fun run(port: i32) {
	match NotesClient::connect(i"ws://localhost:{port}/", json_codec()) {
		Ok(let a) => {
			match NotesClient::connect(i"ws://localhost:{port}/", json_codec()) {
				Ok(let b) => {
					let watch_a = a.count.sub(|value| print(i"a-mirror:{value}"));
					let watch_b = b.count.sub(|value| print(i"b-mirror:{value}"));
					print(i"a-add:{a.add(2).unwrap_or(0 - 1)}");
					print(i"a-add:{a.add(2).unwrap_or(0 - 1)}");
					print(i"b-add:{b.add(5).unwrap_or(0 - 1)}");
					let a_who = a.whoami().unwrap_or("?");
					let b_who = b.whoami().unwrap_or("?");
					print(i"a-who:{a_who}");
					print(i"b-who:{b_who}");
					print(i"hash:{a.contract_hash()}");
					watch_a.dispose();
					watch_b.dispose();
				},
				Err(let error) => print(i"b-err:{error.debug()}"),
			}
		},
		Err(let error) => print(i"a-err:{error.debug()}"),
	}
	exit(0);
}
"#
        ),
    );
    let stdout = vilan_run_with_liveness_bound(&dir);
    for expected in [
        // Each connection counts in its own cell: A reaches 4 while B, adding
        // 5 to a counter that has never been touched, reaches exactly 5.
        "a-add:2",
        "a-add:4",
        "b-add:5",
        // Each connection's `[expose]`d mirror is its own instance's cell.
        "a-mirror:4",
        "b-mirror:5",
        // The identity a route closed over — impossible with one instance.
        "a-who:conn-0",
        "b-who:conn-1",
    ] {
        assert!(
            stdout.contains(expected),
            "a factory service must build one instance per connection; `{expected}` is \
             missing:\n{stdout}"
        );
    }
    for forbidden in [
        "a-mirror:5",
        "a-mirror:7",
        "b-mirror:2",
        "b-mirror:4",
        "b-add:9",
    ] {
        assert!(
            !stdout.contains(forbidden),
            "`{forbidden}` means the two connections shared one instance's state:\n{stdout}"
        );
    }
    // Contract hash unaffected by the per-connection shape: it hashes methods
    // and exposures (`add(i32)->i32;whoami()->str;expose:count:i32;`), neither
    // of which the factory touches. Recorded here so a future change to the
    // generated surface has to say so out loud.
    assert!(
        stdout.contains("hash:d1d5fba0"),
        "the contract hash moved — the factory shape must not change the hashed \
         surface:\n{stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_factory_services_connectionless_rpc_post_is_refused_in_as_many_words() {
    // A38's one refusal: the `{mount}rpc` POST leg carries no connection, so a
    // service that builds one instance per connection has nothing to answer it
    // with — no instance, no session, no identity. Answering it from some other
    // client's instance would be worse than refusing, so it refuses (501) and
    // says why. `Service::new`'s shared protocol still answers the same leg,
    // which the sibling assertion holds down.
    let dir = temp_project("factory_post");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    write(
        &dir,
        "src/main.vl",
        &format!(
            "{FACTORY_SERVER}{}",
            r#"
fun main() {
	let shared = Notes { who = "shared", count = Signal::new(0) };
	Server::builder()
		.port(0)
		.with_service(Service::factory(|connection: Connection| Notes {
			who = i"conn-{connection.id}",
			count = Signal::new(0),
		}, json_codec()))
		.with_service(Service::new(shared.dispatcher().into_protocol(json_codec())).at("/shared/"))
		.on_request(|request| Response::builder().code(404).body("nope").build())
		.on_start(|server| print(i"ready {server.port()}"))
		.build()
		.start();
}
"#
        ),
    );

    let server = StreamingServer::spawn(&dir);
    let ready = server.await_line("ready", Duration::from_secs(60));
    let port: u16 = ready
        .split_whitespace()
        .next_back()
        .expect("the ready line carries the bound port")
        .parse()
        .expect("the announced port is a number");

    let refused = raw_http_closed(
        port,
        "POST /rpc HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
    );
    assert!(
        refused.starts_with("HTTP/1.1 501 "),
        "a factory service's POST rpc leg must be refused, not answered: {refused}"
    );
    assert!(
        refused.contains("Service::factory") && refused.contains("WebSocket"),
        "the refusal must name the cause and the way out: {refused}"
    );

    let answered = raw_http_closed(
        port,
        "POST /shared/rpc HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 2\r\nConnection: \
         close\r\n\r\n{}",
    );
    assert!(
        answered.starts_with("HTTP/1.1 200 OK\r\n") && answered.contains("application/json"),
        "a stateless `Service::new` still answers its POST rpc leg: {answered}"
    );

    drop(server);
    let _ = std::fs::remove_dir_all(&dir);
}

// --- A40: the pre-upgrade gate, and the subprotocol echo ----------------------

/// One raw WebSocket upgrade request, with `extra` folded in before the blank
/// line — the 101 (or the refusal) exactly as it arrived. Bounded rather than
/// read-to-close: an accepted upgrade holds the socket open forever.
fn raw_upgrade(port: u16, path: &str, extra: &str) -> String {
    raw_http_bounded(
        port,
        &format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nUpgrade: websocket\r\nConnection: \
             Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: \
             13\r\n{extra}\r\n"
        ),
        Duration::from_millis(1500),
    )
}

/// A service that gates its upgrades: `"good"` is `ada`, any other token is
/// forbidden, no token at all is unauthorized. The mechanism is the app's —
/// std verifies nothing — so the pin uses the cheapest possible check.
const AUTHORIZED_SERVER: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::result::Result;
import std::json::json_codec;
import std::http::{ Response, Server };
import std::rpc_server::{ Connection, Handshake, Reject, Service, Session };

[service(NotesClient)]
struct Notes {
	who: str,
	[expose] count: SignalCell<i32>,
}

impl Notes {
	[rpc]
	fun whoami(self): str {
		self.who
	}
}

fun main() {
	Server::builder()
		.port(0)
		.with_service(Service::factory(|connection: Connection| Notes {
			who = connection.session.identity,
			count = Signal::new(0),
		}, json_codec())
			.authorize(|handshake: Handshake| match handshake.token() {
				Some(let token) => if token == "good" {
					Result::Ok(Session::of("ada").with_credential(token))
				} else {
					Result::Err(Reject::Forbidden)
				},
				None => Result::Err(Reject::Unauthorized),
			}))
		.on_request(|request| Response::builder().code(404).body("nope").build())
		.on_start(|server| print(i"ready {server.port()}"))
		.build()
		.start();
}
"#;

/// Boot `source` as a long-running server and return `(server, port)`.
fn spawn_service_server(tag: &str, source: &str) -> (StreamingServer, u16) {
    let dir = temp_project(tag);
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    write(&dir, "src/main.vl", source);
    let server = StreamingServer::spawn(&dir);
    let ready = server.await_line("ready", Duration::from_secs(60));
    let port: u16 = ready
        .split_whitespace()
        .next_back()
        .expect("the ready line carries the bound port")
        .parse()
        .expect("the announced port is a number");
    (server, port)
}

#[test]
fn the_handshake_echoes_the_subprotocol_it_selected() {
    // RFC 6455 §4.2.2: a client that offered subprotocols must hear the
    // server's selection back in the 101. This server never read the header
    // and never echoed one, so a BROWSER — which enforces the rule — closed
    // every connection a page opened with a subprotocol, silently. The echo is
    // independent of `authorize` and pinned here on an ungated service, offer
    // by offer; an offer of nothing must still produce the byte-identical
    // handshake it always did.
    let (server, port) = spawn_service_server("echo", BYTE_IDENTICAL_SERVER);

    let selected = raw_upgrade(
        port,
        "/",
        "Sec-WebSocket-Protocol: vilan-rpc, token.abc\r\n",
    );
    assert!(
        selected.starts_with("HTTP/1.1 101 Switching Protocols\r\n"),
        "the offer must still be upgraded: {selected}"
    );
    assert!(
        selected.contains("Sec-WebSocket-Protocol: vilan-rpc\r\n"),
        "the server must echo the subprotocol it selected, or a browser closes the \
         connection: {selected}"
    );
    assert!(
        !selected.contains("token.abc"),
        "the credential rides the offer; the server selects the PROTOCOL, never echoes the \
         token back: {selected}"
    );

    // Not among the offers it knows: the client's first choice is selected, so
    // the connection is not closed for want of an echo.
    let unknown = raw_upgrade(port, "/", "Sec-WebSocket-Protocol: chat, superchat\r\n");
    assert!(
        unknown.contains("Sec-WebSocket-Protocol: chat\r\n"),
        "an offer with no vilan-rpc in it still needs an echo from the list: {unknown}"
    );

    // A credential is not a protocol: an offer of nothing else selects
    // nothing, rather than naming `token.…` as the protocol in play and
    // writing the credential back out in the reply.
    let credential_only = raw_upgrade(port, "/", "Sec-WebSocket-Protocol: token.secret\r\n");
    assert!(
        credential_only.starts_with("HTTP/1.1 101 Switching Protocols\r\n"),
        "an ungated service still upgrades it: {credential_only}"
    );
    assert!(
        !credential_only.contains("Sec-WebSocket-Protocol"),
        "a `token.` offer must never be selected or echoed: {credential_only}"
    );

    // No offer, no echo — the handshake byte-for-byte as it was before A40.
    let silent = raw_upgrade(port, "/", "");
    assert!(
        silent.starts_with("HTTP/1.1 101 Switching Protocols\r\n"),
        "a bare handshake must still upgrade: {silent}"
    );
    assert!(
        !silent.contains("Sec-WebSocket-Protocol"),
        "a client that offered nothing must be sent no selection: {silent}"
    );

    drop(server);
}

#[test]
fn an_unauthorized_handshake_is_refused_before_the_upgrade() {
    // A40: `authorize` runs on the upgrade REQUEST. A refusal writes a status
    // line on the raw socket and destroys it — no 101, no connection id, no
    // reactive session, no service instance — which is the whole reason the
    // gate is here and not inside a method. The three answers are distinct
    // because the app's three answers are.
    let (server, port) = spawn_service_server("authorize", AUTHORIZED_SERVER);

    let missing = raw_upgrade(port, "/", "");
    assert!(
        missing.starts_with("HTTP/1.1 401 Unauthorized\r\n"),
        "a handshake with no credential must be refused 401: {missing}"
    );
    let forbidden = raw_upgrade(
        port,
        "/",
        "Sec-WebSocket-Protocol: vilan-rpc, token.bad\r\n",
    );
    assert!(
        forbidden.starts_with("HTTP/1.1 403 Forbidden\r\n"),
        "a credential the app rejects must be refused 403: {forbidden}"
    );
    for refusal in [&missing, &forbidden] {
        assert!(
            !refusal.contains("101 Switching Protocols"),
            "a refused handshake must never be upgraded: {refusal}"
        );
    }

    let admitted = raw_upgrade(
        port,
        "/",
        "Sec-WebSocket-Protocol: vilan-rpc, token.good\r\n",
    );
    assert!(
        admitted.starts_with("HTTP/1.1 101 Switching Protocols\r\n")
            && admitted.contains("Sec-WebSocket-Protocol: vilan-rpc\r\n"),
        "the credential the app accepts must be upgraded, echo included: {admitted}"
    );

    // The connectionless legs carry no handshake to gate, so an authorized
    // service refuses them rather than leaving them as the open door around
    // `authorize`. The rpc leg answers in the protocol's own vocabulary —
    // where `RpcError::Unauthorized`, constructed nowhere in std until now,
    // gets its producer.
    let posted = raw_http_closed(
        port,
        "POST /rpc HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
    );
    assert!(
        posted.starts_with("HTTP/1.1 401 Unauthorized\r\n") && posted.contains("Unauthorized"),
        "an authorized service's POST rpc leg must answer a typed Unauthorized failure: {posted}"
    );
    let streamed = raw_http_bounded(
        port,
        "GET /events HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
        Duration::from_millis(1500),
    );
    assert!(
        streamed.starts_with("HTTP/1.1 401 Unauthorized\r\n"),
        "an authorized service's SSE leg must be refused too: {streamed}"
    );

    drop(server);
}

#[test]
fn the_connection_ceiling_refuses_the_handshake_over_it() {
    // The DoS half of A40, and the part that works with `authorize` absent: a
    // ceiling on live connections, refused at the handshake with 429 rather
    // than after a socket, a session and an instance already exist. Two
    // sockets are held open across the third attempt, which is what makes the
    // count a count.
    let source = BYTE_IDENTICAL_SERVER.replace(
        ".with_service(Service::new(counter.dispatcher().into_protocol(json_codec())))",
        ".with_service(Service::new(counter.dispatcher().into_protocol(json_codec())).max_connections(2))",
    );
    assert!(
        source.contains("max_connections(2)"),
        "the ceiling must actually be spliced into the server source"
    );
    let (server, port) = spawn_service_server("ceiling", &source);

    let mut held = Vec::new();
    for attempt in 0..2 {
        let mut stream =
            TcpStream::connect(("127.0.0.1", port)).expect("connect to the reported port");
        stream
            .set_read_timeout(Some(Duration::from_millis(1500)))
            .expect("set a read timeout");
        stream
            .write_all(
                "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nUpgrade: websocket\r\nConnection: \
                 Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: \
                 13\r\n\r\n"
                    .as_bytes(),
            )
            .expect("send the upgrade");
        let mut buffer = [0u8; 512];
        let read = stream.read(&mut buffer).expect("read the handshake reply");
        let reply = String::from_utf8_lossy(&buffer[..read]).into_owned();
        assert!(
            reply.starts_with("HTTP/1.1 101 "),
            "connection {attempt} is under the ceiling and must be upgraded: {reply}"
        );
        held.push(stream);
    }

    let over = raw_upgrade(port, "/", "");
    assert!(
        over.starts_with("HTTP/1.1 429 Too Many Requests\r\n"),
        "the handshake over the ceiling must be refused 429, not upgraded: {over}"
    );

    // A slot released by a closed connection is a slot again.
    drop(held.pop());
    let after = {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let reply = raw_upgrade(port, "/", "");
            if reply.starts_with("HTTP/1.1 101 ") || Instant::now() > deadline {
                break reply;
            }
        }
    };
    assert!(
        after.starts_with("HTTP/1.1 101 "),
        "closing a connection must return its slot to the ceiling: {after}"
    );

    drop(held);
    drop(server);
}

#[test]
fn an_authorized_client_connects_with_its_credential_and_is_that_identity() {
    // The whole A40 loop, end to end and in one process: the client offers
    // `["vilan-rpc", "token.good"]` through `connect_with`, the server selects
    // and echoes `vilan-rpc`, `authorize` turns the credential into a
    // `Session`, and A38's factory builds the instance from it — so `whoami`
    // answers with an identity that arrived on the HANDSHAKE and was never a
    // parameter of any call.
    let dir = temp_project("credential");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    write(
        &dir,
        "src/main.vl",
        &(AUTHORIZED_SERVER
            .replace(
                "import std::io::print;",
                "import std::io::print;\nimport std::process::exit;\nimport std::rpc::rpc_protocols;\nimport std::result::Result::{ Ok, Err };",
            )
            .replace(
                r#"		.on_start(|server| print(i"ready {server.port()}"))"#,
                "		.on_start(|server| run(server.port()))",
            )
            + r#"
fun run(port: i32) {
	match NotesClient::connect_with(i"ws://localhost:{port}/", json_codec(), rpc_protocols("good")) {
		Ok(let client) => {
			let who = client.whoami().unwrap_or("?");
			print(i"who:{who}");
		},
		Err(let error) => print(i"err:{error.debug()}"),
	}
	exit(0);
}
"#),
    );
    let stdout = vilan_run_with_liveness_bound(&dir);
    assert!(
        stdout.contains("who:ada"),
        "the credential offered on the handshake must reach the factory as the connection's \
         identity:\n{stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_greeting_bound_destroys_a_silent_socket_and_spares_a_speaking_one() {
    // The third of A40's cheap limits: a socket that completes the handshake
    // and then says nothing is destroyed, so a slowloris costs a timer rather
    // than a connection slot for the life of the process. It is a GREETING
    // bound, not an idle one — disarmed by the first inbound byte — which is
    // the half that matters, because a client that connected and is only
    // watching mirrors sends nothing for hours and must not be touched.
    let source = BYTE_IDENTICAL_SERVER.replace(
        ".with_service(Service::new(counter.dispatcher().into_protocol(json_codec())))",
        ".with_service(Service::new(counter.dispatcher().into_protocol(json_codec())).handshake_timeout(500))",
    );
    assert!(source.contains("handshake_timeout(500)"));
    let (server, port) = spawn_service_server("greeting", &source);

    // Silent: the 101 lands, then the server hangs up on its own.
    let mut quiet = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    quiet
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set a read timeout");
    quiet
        .write_all(
            "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nUpgrade: websocket\r\nConnection: \
             Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: \
             13\r\n\r\n"
                .as_bytes(),
        )
        .expect("send the upgrade");
    let mut buffer = [0u8; 512];
    let first = quiet.read(&mut buffer).expect("read the 101");
    assert!(
        String::from_utf8_lossy(&buffer[..first]).starts_with("HTTP/1.1 101 "),
        "the handshake itself is not what the bound refuses"
    );
    // Whatever else arrives (the `__conn:` frame), the stream must reach EOF.
    let closed = loop {
        match quiet.read(&mut buffer) {
            Ok(0) => break true,
            Ok(_more) => {}
            Err(_timeout) => break false,
        }
    };
    assert!(
        closed,
        "a socket that upgraded and then said nothing must be destroyed by the greeting bound"
    );

    // Speaking: one masked ping is a greeting. The pong proves it was heard,
    // and the socket must still be there well past the bound.
    let mut talker = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    talker
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set a read timeout");
    talker
        .write_all(
            "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nUpgrade: websocket\r\nConnection: \
             Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: \
             13\r\n\r\n"
                .as_bytes(),
        )
        .expect("send the upgrade");
    let handshake = talker.read(&mut buffer).expect("read the 101");
    assert!(String::from_utf8_lossy(&buffer[..handshake]).starts_with("HTTP/1.1 101 "));
    // FIN + opcode 0x9 (ping), masked, empty payload.
    talker
        .write_all(&[0x89, 0x80, 0x01, 0x02, 0x03, 0x04])
        .expect("send a ping");
    let mut saw_pong = false;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        match talker.read(&mut buffer) {
            Ok(0) => panic!("a socket that greeted the server must not be destroyed by the bound"),
            Ok(read) => {
                if buffer[..read].windows(2).any(|pair| pair == [0x8a, 0x00]) {
                    saw_pong = true;
                    break;
                }
            }
            Err(_timeout) => break,
        }
    }
    assert!(
        saw_pong,
        "the ping must be answered, which is what disarms the bound"
    );
    assert!(
        talker
            .write_all(&[0x89, 0x80, 0x01, 0x02, 0x03, 0x04])
            .is_ok(),
        "the socket must still be live past the greeting bound"
    );

    drop(server);
}

/// A `[expose(keyed)]` service, end to end over a real WebSocket: the keyed
/// channel the macro mints, the `KeyedSource` mirror the generated client
/// carries, and a per-key subscription taken through it (A39).
const KEYED_SERVICE: &str = r#"import std::io::print;
import std::process::exit;
import std::reactive::{ Signal, SignalCell };
import std::result::Result::{ self, Ok, Err };
import std::json::json_codec;
import std::http::{ Response, Server };
import std::map::Map;
import std::rpc_server::Service;
import std::wire::{ Keyed, Wire };

[derive(Wire, PartialEq, Debug)]
struct Message {
	id: str,
	channel: i32,
	body: str,
}

impl Message with Keyed<str> {
	fun key(self): str {
		self.id
	}
}

[service(ChatClient)]
struct Chat {
	[expose] topic: SignalCell<str>,
	[expose(keyed)] messages: SignalCell<Map<str, Message>>,
}

impl Chat {
	[rpc]
	fun post(self, id: str, channel: i32, body: str): i32 {
		self.messages.update(|&mut store| {
			store.insert(id, Message { id, channel, body });
		});
		self.messages.get().len()
	}

	[rpc]
	fun edit(self, id: str, body: str): bool {
		match self.messages.get().get(id) {
			Some(let held) => {
				self.messages.update(|&mut store| {
					store.insert(id, Message { id, channel = held.channel, body });
				});
				true
			},
			None => false,
		}
	}
}

// The same surface with the exposure shape as its ONLY difference — the twin
// that shows the contract hash moving for the keyed form and only for it.
[service(PlainChatClient)]
struct PlainChat {
	[expose] topic: SignalCell<str>,
	[expose] messages: SignalCell<Map<str, Message>>,
}

impl PlainChat {
	[rpc]
	fun post(self, id: str, channel: i32, body: str): i32 {
		0
	}

	[rpc]
	fun edit(self, id: str, body: str): bool {
		false
	}
}

let chat: Chat = Chat { topic = Signal::new("general"), messages = Signal::new(Map::new()) };

fun main() {
	Server::builder()
		.port(0)
		.with_service(Service::new(chat.dispatcher().into_protocol(json_codec())))
		.on_request(|request| Response::builder().code(404).body("nope").build())
		.on_start(|server| run(server.port()))
		.build()
		.start();
}

fun render(list: List<Message>): str {
	mut out = "";
	for message in list {
		out = out + message.id + "=" + message.body + " ";
	}
	out
}

fun run(port: i32) {
	match ChatClient::connect(i"ws://localhost:{port}/", json_codec()) {
		Ok(let client) => {
			// Both mirrors are the generated ones: `topic` is a
			// `RemoteSource<str>`, `messages` a `KeyedSource<str, Message>`.
			let topic = client.topic.sub(|value| print(i"topic:{value}"));
			let watch = client.messages.sub_key("m2", |value| match value {
				Some(let message) => print(i"m2:{message.body}"),
				None => print("m2:absent"),
			});
			print(i"post:{client.post("m1", 0, "hello").unwrap_or(0 - 1)}");
			print(i"post:{client.post("m2", 0, "world").unwrap_or(0 - 1)}");
			print(i"edit:{client.edit("m1", "hello again").unwrap_or(false)}");
			print(i"edit:{client.edit("m2", "world again").unwrap_or(false)}");
			// The per-key mirror holds ITS message and no other, even though
			// the service's map holds two.
			print(i"held:{render(client.messages.get().unwrap_or([]))}");
			print(i"topic-held:{client.topic.get().unwrap_or("?")}");
			let plain = PlainChat { topic = Signal::new(""), messages = Signal::new(Map::new()) };
			print(i"hash:{client.contract_hash()}");
			print(i"plain-hash:{plain.contract_hash()}");
			print(i"fault:{client.messages.fault().is_some()}");
			watch.dispose();
			topic.dispose();
		},
		Err(let error) => print(i"err:{error.debug()}"),
	}
	exit(0);
}
"#;

#[test]
fn an_expose_keyed_field_mirrors_as_a_keyed_source_the_generated_client_can_subscribe_per_key() {
    // The macro half of A39, exercised where it actually has to work: a real
    // server, a real handshake, the generated `connect`. `topic` is a plain
    // `[expose]` and mirrors as a `RemoteSource<str>`; `messages` is
    // `[expose(keyed)]` over a `Map<str, Message>` and mirrors as a
    // `KeyedSource<str, Message>`, which is what makes `sub_key` reachable
    // from generated client code at all — the thing A39 recorded as stopping
    // at the client, because the `ReactiveClient` behind the generated mirrors
    // is not public (A30) and a hand-wired channel could not be reached.
    //
    // The load-bearing line is `held:`: the client leased ONE key, so its
    // mirror holds exactly that message while the service's map holds two.
    // Under `[expose]` the same subscription would have carried both, and
    // every other message the service ever accepts.
    let dir = temp_project("keyed");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    write(&dir, "src/main.vl", KEYED_SERVICE);
    let stdout = vilan_run_with_liveness_bound(&dir);
    for expected in [
        // The per-key mirror seeds absent, then follows its own key alone.
        "m2:absent",
        "m2:world",
        "m2:world again",
        // The plain mirror beside it is untouched by any of this.
        "topic:general",
        "topic-held:general",
        // Two messages posted, one message held.
        "post:1",
        "post:2",
        "edit:true",
        "held:m2=world again",
        "fault:false",
    ] {
        assert!(
            stdout.contains(expected),
            "`{expected}` is missing from the keyed service's run:\n{stdout}"
        );
    }
    assert!(
        !stdout.contains("held:m1="),
        "a per-key subscription received a message it never asked for:\n{stdout}"
    );
    // The contract hash moves for the keyed form and ONLY for it. `PlainChat`
    // is the same surface with the exposure shape as its only difference, and
    // it hashes differently — a client built against one cannot connect to the
    // other, which is exactly right: the frames differ. The plain-`[expose]`
    // hash pinned in `a_factory_service_builds_one_instance_per_connection`
    // (`d1d5fba0`) is the other half of the claim: it did not move at all.
    assert!(
        stdout.contains("hash:43077e29"),
        "the keyed service's contract hash moved:\n{stdout}"
    );
    assert!(
        stdout.contains("plain-hash:c63e39e3"),
        "the plain twin's contract hash moved:\n{stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
