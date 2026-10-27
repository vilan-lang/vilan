//! M164: a `std::http` request body is capped, and read when the handler asks
//! for it — on BOTH backends, over a real socket.
//!
//! Every claim here is about what the WIRE carried and when, so the harness
//! speaks raw HTTP from outside the process. The orderings are the harness's
//! own, never a sleep: a request whose body the client never SENDS can only be
//! answered by a server that did not wait for it, and a chunked body the client
//! never ENDS can only be refused by a server that counted it as it came. A
//! server that waited would answer nothing, and the read's liveness bound turns
//! that into a failure.
//!
//! Each program is built once per backend — node, and `--backend rust` run as
//! its binary — and every case runs against both; the failures of every case on
//! both legs are collected and reported together, so one red case cannot hide
//! the next.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;

mod support;

use support::port::{ANNOUNCE_PORT, Server};

const MIB: usize = 1024 * 1024;

/// The two backends a case runs against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Backend {
    Node,
    Native,
}

/// A program built for both backends, ready to spawn.
struct Built {
    staged: PathBuf,
    native_binary: PathBuf,
    module: String,
}

impl Built {
    /// Writes `source` as `<name>.vl` under a fresh staging directory and builds
    /// it for node and natively. Each test owns its program NAME, so two tests
    /// never build into one another's binary.
    fn new(name: &str, source: &str) -> Built {
        let staged = support::scratch_dir(&format!("request_body_{name}_{}", std::process::id()));
        std::fs::create_dir_all(&staged).expect("create the staging directory");
        let file = format!("{name}.vl");
        std::fs::write(staged.join(&file), source).expect("write the program");
        let native = vilan(&staged)
            .args(["build", "--backend", "rust", &file])
            .output()
            .expect("build natively");
        assert!(
            native.status.success(),
            "the native leg did not build:\n{}{}",
            String::from_utf8_lossy(&native.stdout),
            String::from_utf8_lossy(&native.stderr)
        );
        let binary = String::from_utf8_lossy(&native.stdout)
            .lines()
            .find_map(|line| line.split(" -> ").nth(1).map(str::to_string))
            .expect("`vilan build` says where the binary is");
        let node = vilan(&staged)
            .args(["build", &file])
            .output()
            .expect("build for node");
        assert!(
            node.status.success(),
            "the node leg did not build:\n{}{}",
            String::from_utf8_lossy(&node.stdout),
            String::from_utf8_lossy(&node.stderr)
        );
        Built {
            native_binary: staged.join(binary),
            module: format!("{name}.mjs"),
            staged,
        }
    }

    fn spawn(&self, backend: Backend) -> Server {
        let mut command = match backend {
            Backend::Node => {
                let mut command = Command::new("node");
                command.arg(&self.module);
                command
            }
            Backend::Native => Command::new(&self.native_binary),
        };
        command.current_dir(&self.staged);
        Server::spawn(&mut command)
    }

    /// Runs every case against a fresh server per backend and panics with
    /// every failure at once.
    fn check(&self, cases: &[(&str, Case)]) {
        let mut failures = Vec::new();
        for backend in [Backend::Node, Backend::Native] {
            let mut server = self.spawn(backend);
            for (name, case) in cases {
                if let Err(failure) = case(server.port(), backend) {
                    failures.push(format!("{backend:?} / {name}: {failure}"));
                }
            }
            server.stop();
        }
        assert!(
            failures.is_empty(),
            "{} case(s) failed:\n{}",
            failures.len(),
            failures.join("\n\n")
        );
    }
}

impl Drop for Built {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.staged);
    }
}

type Case = fn(u16, Backend) -> Result<(), String>;

fn vilan(staged: &Path) -> Command {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut command = Command::new(env!("CARGO_BIN_EXE_vilan"));
    command
        .current_dir(staged)
        .env("VILAN_STD", manifest.join("../../vilan/std"))
        .env("VILAN_RT", manifest.join("../vilan-rt"))
        .env(
            "CARGO_TARGET_DIR",
            Path::new(env!("CARGO_TARGET_TMPDIR")).join("request-body-native"),
        );
    command
}

// ------------------------------------------------------------- the wire ---

/// A connection to the server — an `Err` rather than a panic when it is gone,
/// so a server that died on one case reports every case after it too.
fn connect(port: u16) -> Result<TcpStream, String> {
    let stream = TcpStream::connect(("127.0.0.1", port))
        .map_err(|error| format!("the server is gone ({error})"))?;
    // A LIVENESS bound: a server that waits for a body the client never sends
    // answers nothing, and this turns the hang into a red.
    stream
        .set_read_timeout(Some(support::run_liveness()))
        .map_err(|error| format!("bound the read: {error}"))?;
    Ok(stream)
}

/// Reads until the server closes, keeping what arrived before a reset (a
/// closed connection with bytes still unread is answered with one).
fn read_response(stream: &mut TcpStream) -> Result<String, String> {
    let mut response = Vec::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => response.extend_from_slice(&buffer[..count]),
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => break,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                return Err(format!(
                    "no answer within the liveness bound; read so far: {:?}",
                    String::from_utf8_lossy(&response)
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {
                return Err(format!(
                    "no answer within the liveness bound; read so far: {:?}",
                    String::from_utf8_lossy(&response)
                ));
            }
            Err(error) => return Err(format!("read failed: {error}")),
        }
    }
    Ok(String::from_utf8_lossy(&response).into_owned())
}

/// One exchange: the head, any body bytes, then the response to the close.
fn exchange(port: u16, head: &str, body: &[u8]) -> Result<String, String> {
    let mut stream = connect(port)?;
    stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(body))
        .map_err(|error| format!("send failed: {error}"))?;
    read_response(&mut stream)
}

/// Sends `head`, then `total` body bytes from a second thread — in chunks
/// when `chunked`, with no terminating chunk — while this thread reads the
/// response.
fn streamed(port: u16, head: &str, total: usize, chunked: bool) -> Result<String, String> {
    let mut stream = connect(port)?;
    stream
        .write_all(head.as_bytes())
        .map_err(|error| format!("send the head: {error}"))?;
    let mut writer = stream
        .try_clone()
        .map_err(|error| format!("a second handle on the socket: {error}"))?;
    let sender = std::thread::spawn(move || {
        let piece = vec![b'a'; 64 * 1024];
        let mut sent = 0;
        while sent < total {
            let count = piece.len().min(total - sent);
            let written = if chunked {
                writer
                    .write_all(format!("{count:x}\r\n").as_bytes())
                    .and_then(|()| writer.write_all(&piece[..count]))
                    .and_then(|()| writer.write_all(b"\r\n"))
            } else {
                writer.write_all(&piece[..count])
            };
            if written.is_err() {
                return;
            }
            sent += count;
        }
    });
    let response = read_response(&mut stream).unwrap_or_else(|failure| failure);
    // The sender ends either way: the server closed the socket under it, or it
    // wrote everything.
    let _ = stream.shutdown(std::net::Shutdown::Both);
    sender.join().expect("the sending thread");
    Ok(response)
}

fn status_of(response: &str) -> &str {
    response.lines().next().unwrap_or("")
}

fn body_of(response: &str) -> &str {
    response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or("")
}

fn expect_status(response: &str, status: &str) -> Result<(), String> {
    if status_of(response) == status {
        Ok(())
    } else {
        Err(format!("expected `{status}`, got:\n{response}"))
    }
}

fn expect_body(response: &str, body: &str) -> Result<(), String> {
    if body_of(response) == body {
        Ok(())
    } else {
        Err(format!("expected the body `{body}`, got:\n{response}"))
    }
}

fn expect_header(response: &str, line: &str) -> Result<(), String> {
    let head = response
        .split_once("\r\n\r\n")
        .map_or(response, |(head, _)| head);
    if head.lines().any(|each| each.eq_ignore_ascii_case(line)) {
        Ok(())
    } else {
        Err(format!("expected the header `{line}`, got:\n{response}"))
    }
}

fn expect_code(response: &str, code: u16) -> Result<(), String> {
    if status_of(response).starts_with(&format!("HTTP/1.1 {code} ")) {
        Ok(())
    } else {
        Err(format!("expected a {code}, got:\n{response}"))
    }
}

/// A POST head that asks the server to close after answering, so a reader
/// ends at the close rather than at node's keep-alive timeout.
fn post_head(path: &str, length: usize) -> String {
    format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {length}\r\n\
         Connection: close\r\n\r\n"
    )
}

/// A POST head that leaves the connection to the server: whether the answer
/// says `Connection: close` is then the server's own decision.
fn post_head_keep_alive(path: &str, length: usize) -> String {
    format!("POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {length}\r\n\r\n")
}

fn chunked_head(path: &str) -> String {
    format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nTransfer-Encoding: chunked\r\n\
         Connection: close\r\n\r\n"
    )
}

// ------------------------------------------------------------ the cap ---

/// The cap's program: no `max_body_bytes` anywhere, so every request is under
/// the 1 MiB default — the echo route, and a mounted rpc service's POST leg.
const CAP_SERVER: &str = r#"import std::io::print;
import std::http::{ Request, Response, Server };
import std::json::json_codec;
import std::rpc::server::Service;
import std::shared::Shared;

[service(CounterClient)]
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

fun echo(request: Request): Response {
	let body = request.bytes();
	Response::builder().body(i"len={body.len()}").build()
}

fun main() {
	let counter = Counter { count = Shared::new(0) };
	Server::builder()
		.port(0)
		.with_service(Service::new(counter.dispatcher().into_protocol(json_codec())).at("/svc/"))
		.on_request(|request| {
			if request.path() == "/auth" {
				ret Response::builder().code(401).body("sign in").build();
			}
			if request.path() == "/twice" {
				let first = request.bytes();
				let second = request.bytes();
				let text = request.body();
				ret Response::builder()
					.body(i"len={first.len()} same={second.len() == first.len()} text={text}")
					.build();
			}
			echo(request)
		})
		.on_start(|server| ANNOUNCE)
		.build()
		.start();
}
"#;

fn cap_server() -> String {
    CAP_SERVER.replace("ANNOUNCE", ANNOUNCE_PORT)
}

const DEFAULT_REFUSAL: &str = "the request body is larger than 1048576 bytes";

/// A 2 MiB POST against the default: answered `413` with only its HEAD sent —
/// the body is never read, because the client never wrote it — and the
/// connection closing.
fn declared_over_the_default_is_refused_before_the_body(
    port: u16,
    _backend: Backend,
) -> Result<(), String> {
    let response = exchange(port, &post_head_keep_alive("/echo", 2 * MIB), b"")?;
    expect_status(&response, "HTTP/1.1 413 Content Too Large")?;
    expect_header(&response, "Connection: close")?;
    expect_body(&response, DEFAULT_REFUSAL)
}

/// The writer of a refused body STALLS: after the `413` the server reads none
/// of it, so the client gets out only what the two kernels' socket buffers hold
/// — then the server closes and the write fails. The body is 64 MiB because a
/// loopback socket's buffers hold several MiB (the client's send buffer grows to
/// `tcp_wmem`'s 4 MiB), so a 2 MiB body could sit wholly in them with the
/// server reading nothing — which the head-only exchange above already proves.
fn the_writer_of_a_refused_body_stalls(port: u16, _backend: Backend) -> Result<(), String> {
    let total = 64 * MIB;
    let mut stream = connect(port)?;
    stream
        .write_all(post_head_keep_alive("/echo", total).as_bytes())
        .map_err(|error| format!("send the head: {error}"))?;
    let mut answer = [0u8; 256];
    let read = stream
        .read(&mut answer)
        .map_err(|error| format!("read the answer: {error}"))?;
    expect_status(
        &String::from_utf8_lossy(&answer[..read]),
        "HTTP/1.1 413 Content Too Large",
    )?;
    let piece = vec![b'a'; 64 * 1024];
    let mut written = 0;
    let failed = loop {
        if written >= total {
            break false;
        }
        match stream.write(&piece) {
            Ok(0) | Err(_) => break true,
            Ok(count) => written += count,
        }
    };
    if !failed || written >= 16 * MIB {
        return Err(format!(
            "the writer got {written} of {total} bytes out (failed: {failed}): the server read \
             the body"
        ));
    }
    Ok(())
}

/// A client that streams its body while it reads still gets the `413`: the
/// refused connection half-closes and lingers before the close, so the reset
/// that closing over unread bytes sends cannot overtake the answer.
fn a_streaming_client_reads_the_refusal(port: u16, _backend: Backend) -> Result<(), String> {
    let outcome = streamed(port, &post_head("/echo", 2 * MIB), 2 * MIB, false)?;
    expect_status(&outcome, "HTTP/1.1 413 Content Too Large")?;
    expect_body(&outcome, DEFAULT_REFUSAL)
}

/// A chunked body over the default, counted as it arrives: 1.5 MiB in 64 KiB
/// chunks and NO terminating chunk, so only a server that counts can answer.
fn chunked_over_the_default_is_refused_as_it_arrives(
    port: u16,
    _backend: Backend,
) -> Result<(), String> {
    let outcome = streamed(port, &chunked_head("/echo"), 3 * MIB / 2, true)?;
    expect_status(&outcome, "HTTP/1.1 413 Content Too Large")?;
    expect_body(&outcome, DEFAULT_REFUSAL)
}

/// A chunked body within the cap is decoded — the native server refused every
/// chunked request with `411` before M164.
fn chunked_within_the_cap_is_decoded(port: u16, _backend: Backend) -> Result<(), String> {
    let response = exchange(
        port,
        &chunked_head("/echo"),
        b"5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n",
    )?;
    expect_status(&response, "HTTP/1.1 200 OK")?;
    expect_body(&response, "len=11")
}

/// A body within the cap is read whole, streamed in pieces.
fn a_body_within_the_cap_is_read_whole(port: u16, _backend: Backend) -> Result<(), String> {
    let outcome = streamed(port, &post_head("/echo", 900 * 1024), 900 * 1024, false)?;
    expect_status(&outcome, "HTTP/1.1 200 OK")?;
    expect_body(&outcome, "len=921600")
}

/// A client that hangs up halfway through a body ends that request and
/// nothing else. Before M164 the node server DIED here: the whole-body read
/// rejected with `aborted` and nothing handled it.
fn a_client_gone_mid_body_leaves_the_server_serving(
    port: u16,
    _backend: Backend,
) -> Result<(), String> {
    {
        let mut stream = connect(port)?;
        stream
            .write_all(post_head("/echo", 500_000).as_bytes())
            .and_then(|()| stream.write_all(&[b'a'; 1000]))
            .map_err(|error| format!("send: {error}"))?;
    }
    // Two full exchanges after the hang-up: the server must still answer.
    for _ in 0..2 {
        let response = exchange(
            port,
            "GET /auth HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
            b"",
        )
        .map_err(|failure| format!("the server stopped answering: {failure}"))?;
        expect_status(&response, "HTTP/1.1 401 Unauthorized")?;
    }
    Ok(())
}

/// The rpc POST leg reads through `Request::bytes`, so it is under the same
/// cap: a 2 MiB frame is refused before the codec sees a byte of it.
fn the_rpc_post_leg_is_under_the_cap(port: u16, _backend: Backend) -> Result<(), String> {
    let response = exchange(
        port,
        &format!(
            "POST /svc/rpc HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\n\r\n",
            2 * MIB
        ),
        b"",
    )?;
    expect_status(&response, "HTTP/1.1 413 Content Too Large")?;
    expect_body(&response, DEFAULT_REFUSAL)?;
    // And a frame within it still reaches the codec (`{}` has no method: the
    // decode failure's 400 envelope).
    let decoded = exchange(
        port,
        "POST /svc/rpc HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\n\
         Content-Length: 2\r\nConnection: close\r\n\r\n",
        b"{}",
    )?;
    expect_status(&decoded, "HTTP/1.1 400 Bad Request")?;
    expect_body(
        &decoded,
        "{\"Failure\":{\"Decode\":\"missing field 'method'\"}}",
    )
}

#[test]
fn m164_the_default_cap_refuses_an_oversized_body_without_reading_it_on_both_backends() {
    let built = Built::new("request_body_cap", &cap_server());
    built.check(&[
        (
            "a declared 2 MiB body is refused before it is sent",
            declared_over_the_default_is_refused_before_the_body,
        ),
        (
            "the writer of a refused body stalls",
            the_writer_of_a_refused_body_stalls,
        ),
        (
            "a streaming client reads the refusal",
            a_streaming_client_reads_the_refusal,
        ),
        (
            "a chunked body over the cap is refused as it arrives",
            chunked_over_the_default_is_refused_as_it_arrives,
        ),
        (
            "a chunked body within the cap is decoded",
            chunked_within_the_cap_is_decoded,
        ),
        (
            "a body within the cap is read whole",
            a_body_within_the_cap_is_read_whole,
        ),
        (
            "a client gone mid-body leaves the server serving",
            a_client_gone_mid_body_leaves_the_server_serving,
        ),
        (
            "the rpc POST leg is under the cap",
            the_rpc_post_leg_is_under_the_cap,
        ),
    ]);
}

// ------------------------------------------------------- the lazy body ---

/// A handler that answers from the HEAD — `401` for a request that is not
/// signed in — on a body far over the cap: the client sends only the head, so
/// the `401` proves the body was never asked for, and the response closes the
/// connection (a body over the cap is not drained).
fn a_handler_answering_from_the_head_never_reads_the_body(
    port: u16,
    _backend: Backend,
) -> Result<(), String> {
    let response = exchange(port, &post_head_keep_alive("/auth", 2 * MIB), b"")?;
    expect_status(&response, "HTTP/1.1 401 Unauthorized")?;
    expect_header(&response, "Connection: close")?;
    expect_body(&response, "sign in")
}

/// A body within the cap that the handler never read is DRAINED: on node the
/// connection stays alive, so a request pipelined behind it is answered too;
/// the native server (no keep-alive) discards it and closes with a FIN.
fn an_unread_body_within_the_cap_is_drained(port: u16, backend: Backend) -> Result<(), String> {
    let response = exchange(
        port,
        "POST /auth HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 5\r\n\r\nhello\
         GET /auth HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
        b"",
    )?;
    let answered = response.matches("HTTP/1.1 401 Unauthorized").count();
    let expected = match backend {
        Backend::Node => 2,
        Backend::Native => 1,
    };
    if answered != expected {
        return Err(format!(
            "expected {expected} answer(s) on the connection, got {answered}:\n{response}"
        ));
    }
    Ok(())
}

/// The body is read once: a second `bytes()` and a `body()` after it answer
/// the same content.
fn a_second_read_answers_the_same_body(port: u16, _backend: Backend) -> Result<(), String> {
    let response = exchange(
        port,
        "POST /twice HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 11\r\nConnection: close\r\n\r\n",
        b"hello world",
    )?;
    expect_status(&response, "HTTP/1.1 200 OK")?;
    expect_body(&response, "len=11 same=true text=hello world")
}

#[test]
fn m164_a_handler_reads_the_body_only_when_it_asks_on_both_backends() {
    let built = Built::new("request_body_lazy", &cap_server());
    built.check(&[
        (
            "a handler answering from the head never reads the body",
            a_handler_answering_from_the_head_never_reads_the_body,
        ),
        (
            "an unread body within the cap is drained",
            an_unread_body_within_the_cap_is_drained,
        ),
        (
            "a second read answers the same body",
            a_second_read_answers_the_same_body,
        ),
    ]);
}

// ------------------------------------------------------- the overrides ---

/// The overrides' program: the server's own cap LOWERED to 512 bytes, two
/// mounts above it (the longer one narrower), a mounted rpc service under a
/// mount of its own, a route that raises its cap in the handler, and the shape
/// of kolt's `/api/upload` — an auth check and a declared-length check off the
/// head, then the read under the route's own 25 MB.
const OVERRIDES_SERVER: &str = r#"import std::io::print;
import std::http::{ Request, Response, Server };
import std::json::json_codec;
import std::rpc::server::Service;
import std::shared::Shared;

[service(CounterClient)]
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

let max_upload_bytes = 26214400;

fun echo(request: Request): Response {
	let body = request.bytes();
	Response::builder().body(i"len={body.len()}").build()
}

fun plain(code: i32, text: str): Response {
	Response::builder().code(code).body(text).build()
}

fun upload(request: Request): Response {
	request.header("X-Signed-In") is Some(let _user) else ret plain(401, "Sign in to upload.");
	let declared = request
		.header("Content-Length")
		.and_then(|length| length.parse_f64())
		.unwrap_or(0);
	declared <= max_upload_bytes.as_f64() else ret plain(413, "Files can be up to 25 MB.");
	let bytes = request.max_body_bytes(max_upload_bytes.as_usize()).bytes();
	Response::builder().body(i"stored={bytes.len()}").build()
}

fun main() {
	let counter = Counter { count = Shared::new(0) };
	Server::builder()
		.port(0)
		.max_body_bytes(512)
		.max_body_bytes_under("/mount", 3145728)
		.max_body_bytes_under("/mount/narrow/", 1024)
		.max_body_bytes_under("/svc/", 3145728)
		.with_service(Service::new(counter.dispatcher().into_protocol(json_codec())).at("/svc/"))
		.on_request(|request| {
			let path = request.path();
			if path == "/api/upload" {
				ret upload(request);
			}
			if path == "/route" {
				ret echo(request.max_body_bytes(4194304));
			}
			echo(request)
		})
		.on_start(|server| ANNOUNCE)
		.build()
		.start();
}
"#;

/// `max_body_bytes(512)` moves the server's own cap — down, here.
fn the_server_cap_is_the_builders(port: u16, _backend: Backend) -> Result<(), String> {
    let within = exchange(port, &post_head("/echo", 500), &[b'a'; 500])?;
    expect_status(&within, "HTTP/1.1 200 OK")?;
    expect_body(&within, "len=500")?;
    let over = exchange(port, &post_head("/echo", 600), b"")?;
    expect_status(&over, "HTTP/1.1 413 Content Too Large")?;
    expect_body(&over, "the request body is larger than 512 bytes")
}

/// The PER-ROUTE override: the handler raises its request's cap before the
/// read — a 3 MiB body is read whole, a 5 MiB one refused at its declaration.
fn a_route_raises_its_own_cap(port: u16, _backend: Backend) -> Result<(), String> {
    let within = streamed(port, &post_head("/route", 3 * MIB), 3 * MIB, false)?;
    expect_status(&within, "HTTP/1.1 200 OK")?;
    expect_body(&within, "len=3145728")?;
    let over = exchange(port, &post_head("/route", 5 * MIB), b"")?;
    expect_status(&over, "HTTP/1.1 413 Content Too Large")?;
    expect_body(&over, "the request body is larger than 4194304 bytes")
}

/// The PER-MOUNT override: every path at or under `/mount` takes its 3 MiB —
/// by whole segments, so `/mountain` keeps the server's 512 — and the longer
/// mount under it (`/mount/narrow/`) wins with its 1 KiB.
fn a_mount_sets_the_cap_under_it(port: u16, _backend: Backend) -> Result<(), String> {
    let under = streamed(port, &post_head("/mount/files", 2 * MIB), 2 * MIB, false)?;
    expect_status(&under, "HTTP/1.1 200 OK")?;
    expect_body(&under, "len=2097152")?;
    let itself = streamed(port, &post_head("/mount?x=1", 2 * MIB), 2 * MIB, false)?;
    expect_status(&itself, "HTTP/1.1 200 OK")?;
    let beside = exchange(port, &post_head("/mountain", 2 * MIB), b"")?;
    expect_body(&beside, "the request body is larger than 512 bytes")?;
    let narrower = exchange(port, &post_head("/mount/narrow/x", 2000), b"")?;
    expect_body(&narrower, "the request body is larger than 1024 bytes")
}

/// A mounted rpc service under a mount of its own: the POST leg reads a 2 MiB
/// frame (the codec refuses it as JSON — the cap did not), where the server's
/// 512 would have refused it.
fn a_service_mount_takes_its_mounts_cap(port: u16, _backend: Backend) -> Result<(), String> {
    let outcome = streamed(
        port,
        &format!(
            "POST /svc/rpc HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n",
            2 * MIB
        ),
        2 * MIB,
        false,
    )?;
    expect_status(&outcome, "HTTP/1.1 400 Bad Request")?;
    if !body_of(&outcome).contains("Decode") {
        return Err(format!("the frame must reach the codec: {}", outcome));
    }
    Ok(())
}

/// kolt's `/api/upload`, as the exhibit: refused from the HEAD when the client
/// is not signed in (a 20 MB body never sent), its own `413` for a declared
/// size over 25 MB, and an upload over the server's 512 bytes stored under the
/// route's own 25 MB.
fn the_upload_route_shape(port: u16, _backend: Backend) -> Result<(), String> {
    let anonymous = exchange(port, &post_head("/api/upload", 20_000_000), b"")?;
    expect_status(&anonymous, "HTTP/1.1 401 Unauthorized")?;
    expect_body(&anonymous, "Sign in to upload.")?;
    let signed_in = |length: usize| {
        format!(
            "POST /api/upload HTTP/1.1\r\nHost: 127.0.0.1\r\nX-Signed-In: ada\r\n\
             Content-Length: {length}\r\nConnection: close\r\n\r\n"
        )
    };
    // The app's own 413: node names it `Payload Too Large`, the native server
    // RFC 9110's `Content Too Large`, so only the code is compared here.
    let too_big = exchange(port, &signed_in(30_000_000), b"")?;
    expect_code(&too_big, 413)?;
    expect_body(&too_big, "Files can be up to 25 MB.")?;
    let stored = streamed(port, &signed_in(5 * MIB), 5 * MIB, false)?;
    expect_status(&stored, "HTTP/1.1 200 OK")?;
    expect_body(&stored, "stored=5242880")
}

#[test]
fn m164_the_server_a_mount_and_a_route_each_set_the_cap_on_both_backends() {
    let built = Built::new(
        "request_body_overrides",
        &OVERRIDES_SERVER.replace("ANNOUNCE", ANNOUNCE_PORT),
    );
    built.check(&[
        (
            "the server's cap is the builder's",
            the_server_cap_is_the_builders,
        ),
        ("a route raises its own cap", a_route_raises_its_own_cap),
        (
            "a mount sets the cap under it",
            a_mount_sets_the_cap_under_it,
        ),
        (
            "a service mount takes its mount's cap",
            a_service_mount_takes_its_mounts_cap,
        ),
        ("the upload route's shape", the_upload_route_shape),
    ]);
}
