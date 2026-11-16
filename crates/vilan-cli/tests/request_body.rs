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
        Server::spawn(&mut self.command(backend))
    }

    /// [`Built::spawn`] with the server's stderr written to `log`, for a case
    /// whose claim is what the server REPORTED (M179).
    fn spawn_logging(&self, backend: Backend, log: &Path) -> Server {
        let file = std::fs::File::create(log).expect("create the stderr log");
        Server::spawn(self.command(backend).stderr(file))
    }

    fn command(&self, backend: Backend) -> Command {
        let mut command = match backend {
            Backend::Node => {
                let mut command = Command::new("node");
                command.arg(&self.module);
                command
            }
            Backend::Native => Command::new(&self.native_binary),
        };
        command.current_dir(&self.staged);
        // M164's measuring condition, so a peak-RSS pin reads the process and
        // not glibc's per-thread arenas.
        command.env("MALLOC_ARENA_MAX", "1");
        command
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
import std::http::{ Request, Response, Server, refused_body_census };
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
			if request.path() == "/census" {
				ret Response::builder().body(i"refused_read={refused_body_census()}").build();
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
    expect_status(&response, "HTTP/1.1 413 Payload Too Large")?;
    expect_header(&response, "Connection: close")?;
    expect_body(&response, DEFAULT_REFUSAL)
}

/// How many body bytes the server has read off connections after declining
/// their bodies (`std::http::refused_body_census`): zero is the contract.
fn refused_body_read(port: u16) -> Result<usize, String> {
    let response = exchange(
        port,
        "GET /census HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
        b"",
    )?;
    expect_status(&response, "HTTP/1.1 200 OK")?;
    body_of(&response)
        .strip_prefix("refused_read=")
        .and_then(|count| count.parse().ok())
        .ok_or_else(|| format!("the census answered:\n{response}"))
}

/// The writer of a refused body STALLS: after the `413` the server reads none
/// of it. The proof is the SERVER's count of the body bytes it read after the
/// refusal (B687): zero. How much the client got out before its write failed is
/// the two kernels' socket buffers — 26.7, 56 and all 64 MiB on three windows
/// runs under node — so it is reported and never judged. The secondary check
/// has no figure: the connection ENDS, the write failing or the server's end of
/// it read. The body is 64 MiB so that a server that DID read it is caught
/// reading: a 2 MiB body could sit wholly in the buffers with the server
/// reading nothing — which the head-only exchange above already proves.
fn the_writer_of_a_refused_body_stalls(port: u16, _backend: Backend) -> Result<(), String> {
    let total = 64 * MIB;
    let before = refused_body_read(port)?;
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
        "HTTP/1.1 413 Payload Too Large",
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
    // A write that never failed: the kernels took the whole body. The
    // connection must still end — read to the server's FIN or reset.
    let ended = failed || read_response(&mut stream).is_ok();
    // Counted after the connection ended, so every byte the server could still
    // take off it is in the figure.
    let after = refused_body_read(port)?;
    if after != before {
        return Err(format!(
            "the server read {} body bytes after its refusal (the writer got {written} of {total} \
             out, failed: {failed})",
            after - before
        ));
    }
    if !ended {
        return Err(format!(
            "the refused connection never ended: the writer got all {total} bytes out and no \
             close followed within the liveness bound"
        ));
    }
    Ok(())
}

/// A client that streams its body while it reads still gets the `413`: the
/// refused connection half-closes and lingers before the close, so the reset
/// that closing over unread bytes sends cannot overtake the answer.
fn a_streaming_client_reads_the_refusal(port: u16, _backend: Backend) -> Result<(), String> {
    let outcome = streamed(port, &post_head("/echo", 2 * MIB), 2 * MIB, false)?;
    expect_status(&outcome, "HTTP/1.1 413 Payload Too Large")?;
    expect_body(&outcome, DEFAULT_REFUSAL)
}

/// A chunked body over the default, counted as it arrives: 1.5 MiB in 64 KiB
/// chunks and NO terminating chunk, so only a server that counts can answer.
fn chunked_over_the_default_is_refused_as_it_arrives(
    port: u16,
    _backend: Backend,
) -> Result<(), String> {
    let outcome = streamed(port, &chunked_head("/echo"), 3 * MIB / 2, true)?;
    expect_status(&outcome, "HTTP/1.1 413 Payload Too Large")?;
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
    expect_status(&response, "HTTP/1.1 413 Payload Too Large")?;
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

/// A body within the cap that the handler never read is DRAINED: the
/// connection stays alive, so a request pipelined behind it is answered too —
/// on both backends since the native server keeps connections (F152).
fn an_unread_body_within_the_cap_is_drained(port: u16, _backend: Backend) -> Result<(), String> {
    let response = exchange(
        port,
        "POST /auth HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 5\r\n\r\nhello\
         GET /auth HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
        b"",
    )?;
    let answered = response.matches("HTTP/1.1 401 Unauthorized").count();
    let expected = 2;
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
    expect_status(&over, "HTTP/1.1 413 Payload Too Large")?;
    expect_body(&over, "the request body is larger than 512 bytes")
}

/// The PER-ROUTE override: the handler raises its request's cap before the
/// read — a 3 MiB body is read whole, a 5 MiB one refused at its declaration.
fn a_route_raises_its_own_cap(port: u16, _backend: Backend) -> Result<(), String> {
    let within = streamed(port, &post_head("/route", 3 * MIB), 3 * MIB, false)?;
    expect_status(&within, "HTTP/1.1 200 OK")?;
    expect_body(&within, "len=3145728")?;
    let over = exchange(port, &post_head("/route", 5 * MIB), b"")?;
    expect_status(&over, "HTTP/1.1 413 Payload Too Large")?;
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
    // The app's own 413 reads node's phrase on both backends (F151): the
    // whole status line is compared, as everywhere else.
    let too_big = exchange(port, &signed_in(30_000_000), b"")?;
    expect_status(&too_big, "HTTP/1.1 413 Payload Too Large")?;
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

// ------------------------------------------------------ a handler panic ---

/// M179's program: `/boom`'s handler panics, every other path answers.
fn panicking_server() -> String {
    format!(
        r#"
import std::http::{{ Response, Server }};
import std::io::{{ panic, print }};

fun main() {{
	Server::builder()
		.port(0)
		.on_request(|request| {{
			if request.path() == "/boom" {{
				panic("handler blew up");
			}}
			Response::builder().body("still here").build()
		}})
		.on_start(|server| {ANNOUNCE_PORT})
		.build()
		.start();
}}
"#
    )
}

/// M179: a handler that PANICS is answered `500` with `Connection: close`, the
/// panic is reported ONCE — the unobserved-task line, `unhandled task error
/// (spawned in an http request handler): panicked at <site>: <message>`, the
/// same on both backends — and the server goes on answering. Before, node's
/// process ended with the panic (the `createServer` callback's promise rejected
/// unhandled) and the native server reported it and left the response open.
#[test]
fn m179_a_handler_panic_answers_500_reports_once_and_the_server_goes_on_on_both_backends() {
    let built = Built::new("handler_panic", &panicking_server());
    let get =
        |path: &str| format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    let mut failures = Vec::new();
    for backend in [Backend::Node, Backend::Native] {
        let log = built.staged.join(format!("stderr-{backend:?}.log"));
        let mut server = built.spawn_logging(backend, &log);
        let port = server.port();
        let answered = exchange(port, &get("/boom"), b"").and_then(|response| {
            expect_code(&response, 500)?;
            expect_header(&response, "Connection: close")?;
            expect_body(&response, "internal server error")
        });
        let served_on = exchange(port, &get("/"), b"").and_then(|response| {
            expect_status(&response, "HTTP/1.1 200 OK")?;
            expect_body(&response, "still here")
        });
        server.stop();
        let stderr = std::fs::read_to_string(&log).unwrap_or_default();
        let reports = stderr
            .lines()
            .filter(|line| {
                line.starts_with(
                    "unhandled task error (spawned in an http request handler): panicked at ",
                ) && line.ends_with(": handler blew up")
            })
            .count();
        for (name, outcome) in [
            ("/boom answers 500", answered),
            ("/ still answers", served_on),
        ] {
            if let Err(failure) = outcome {
                failures.push(format!(
                    "{backend:?} / {name}: {failure}\nstderr:\n{stderr}"
                ));
            }
        }
        if reports != 1 {
            failures.push(format!(
                "{backend:?}: the panic is reported once, got {reports} report(s):\n{stderr}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} case(s) failed:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

// ------------------------------------------------------------ the stream ---

/// M176's program: the body read as a STREAM. `/hash` hashes it as it arrives —
/// sha-256 over the sha-256s of its 256 KiB blocks, so the digest does not
/// depend on how the pieces fell, and only one block is ever held — reading
/// `next(65536)`; `/hash-huge` reads with a `size` far past any body;
/// `/pieces` reports how many pieces a read took; `/count` reads under the
/// default 1 MiB cap; `/partial` reads ten bytes and answers; the two
/// `/..-then-..` routes read one body both ways.
const STREAM_SERVER: &str = r#"import std::io::print;
import std::bytes::Bytes;
import std::crypto::sha256;
import std::http::{ BodyReader, Request, Response, Server, refused_body_census };

fun hashed(reader: BodyReader, size: usize): Response {
	let block_size = 262144usize;
	let block = Bytes::alloc(block_size);
	mut filled = 0usize;
	mut total = 0usize;
	mut digests = Bytes::alloc(0);
	for {
		let wanted = if block_size - filled < size { block_size - filled } else { size };
		let piece = match reader.next(wanted) {
			Some(let piece) => piece,
			None => jump break,
		};
		block.copy_into(piece, filled);
		filled = filled + piece.len();
		total = total + piece.len();
		if filled == block_size {
			digests = Bytes::concat(digests, sha256(block));
			filled = 0;
		}
	}
	if filled > 0 {
		digests = Bytes::concat(digests, sha256(block.slice(0, filled)));
	}
	Response::builder().body(i"len={total} sha={sha256(digests).to_hex()}").build()
}

fun counted(reader: BodyReader, size: usize): Response {
	mut pieces = 0;
	mut total = 0usize;
	mut largest = 0usize;
	for {
		let piece = match reader.next(size) {
			Some(let piece) => piece,
			None => jump break,
		};
		pieces = pieces + 1;
		total = total + piece.len();
		if piece.len() > largest {
			largest = piece.len();
		}
	}
	let after = match reader.next(size) {
		Some(let _piece) => "more",
		None => "none",
	};
	Response::builder().body(i"pieces={pieces} len={total} largest={largest} after={after}").build()
}

fun main() {
	Server::builder()
		.port(0)
		.on_request(|request| {
			let path = request.path();
			if path == "/census" {
				ret Response::builder().body(i"refused_read={refused_body_census()}").build();
			}
			if path == "/hash" {
				ret hashed(request.max_body_bytes(100 * 1024 * 1024).body_stream(), 65536);
			}
			if path == "/hash-huge" {
				ret hashed(request.max_body_bytes(100 * 1024 * 1024).body_stream(), 1073741824);
			}
			if path == "/pieces-huge" {
				ret counted(request.body_stream(), 1073741824);
			}
			if path == "/pieces-7" {
				ret counted(request.body_stream(), 7);
			}
			if path == "/count" {
				ret counted(request.body_stream(), 65536);
			}
			if path == "/partial" {
				let reader = request.body_stream();
				let first = reader.next(10);
				ret Response::builder().body(i"took={first.unwrap().len()}").build();
			}
			if path == "/stream-then-bytes" {
				let reader = request.body_stream();
				let _first = reader.next(4);
				let all = request.bytes();
				ret Response::builder().body(i"read both ways: {all.len()}").build();
			}
			if path == "/bytes-then-stream" {
				let all = request.body();
				let _reader = request.body_stream();
				ret Response::builder().body(i"read both ways: {all}").build();
			}
			if path == "/stream-twice" {
				let first = request.body_stream().next(5).unwrap();
				let rest = request.body_stream().next(100).unwrap();
				ret Response::builder().body(i"first={first.len()} rest={rest.len()}").build();
			}
			Response::builder().code(404).body("no route").build()
		})
		.on_start(|server| ANNOUNCE)
		.build()
		.start();
}
"#;

fn stream_server() -> String {
    STREAM_SERVER.replace("ANNOUNCE", ANNOUNCE_PORT)
}

/// `/hash`'s digest of `data`, computed here: sha-256 over the sha-256s of its
/// 256 KiB blocks.
fn block_digest(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut digests = Vec::new();
    for block in data.chunks(256 * 1024) {
        digests.extend_from_slice(&Sha256::digest(block));
    }
    let digest = Sha256::digest(&digests);
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("len={} sha={hex}", data.len())
}

/// `length` bytes that are not all one value (an xorshift), so a piece lost,
/// repeated or reordered changes the digest.
fn varied_bytes(length: usize) -> Vec<u8> {
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

/// A process's peak resident set (`VmHWM`), in kB — `None` off Linux, where
/// there is no `/proc` to read it from.
fn peak_rss_kb(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    status.lines().find_map(|line| {
        line.strip_prefix("VmHWM:")
            .and_then(|rest| rest.trim().trim_end_matches("kB").trim().parse().ok())
    })
}

/// Sends `data` as the body of a POST to `path` from a second thread, in 64 KiB
/// writes, while this thread reads the response.
fn uploaded(port: u16, path: &str, data: Vec<u8>) -> Result<String, String> {
    let mut stream = connect(port)?;
    stream
        .write_all(post_head(path, data.len()).as_bytes())
        .map_err(|error| format!("send the head: {error}"))?;
    let mut writer = stream
        .try_clone()
        .map_err(|error| format!("a second handle on the socket: {error}"))?;
    let sender = std::thread::spawn(move || {
        for piece in data.chunks(64 * 1024) {
            if writer.write_all(piece).is_err() {
                return;
            }
        }
    });
    let response = read_response(&mut stream);
    let _ = stream.shutdown(std::net::Shutdown::Both);
    sender.join().expect("the sending thread");
    response
}

/// M176's measurement: a 64 MiB upload hashed from the stream, against the
/// whole-body read's cost (M164: 25 MiB read whole is ~25 MB natively and
/// ~56 MB on node). The figure is the server's PEAK resident set grown over
/// the upload — `VmHWM`, after a small upload has paid for every lazy start-up
/// cost — and the bounds are the backends' own: natively the stream holds a
/// slice and a block (measured ~0.5 MB), so 8 MiB is generous; node allocates
/// a fresh buffer per socket read and frees them when its collector gets to
/// them, so its peak is the collector's lag, not what the stream holds
/// (measured ~20-30 MB, where node's own `on("data")` counting loop measures
/// ~41 MB) — bounded at 48 MiB, under the upload itself and far under the
/// whole read's two-and-a-bit times it. Off Linux the digest is still checked
/// and the figure is not read.
fn the_upload_is_hashed_at_a_bounded_cost(built: &Built, backend: Backend) -> Result<(), String> {
    let mut server = built.spawn(backend);
    let port = server.port();
    let outcome = (|| {
        let warm = varied_bytes(300 * 1024);
        let response = uploaded(port, "/hash", warm.clone())?;
        expect_body(&response, &block_digest(&warm))?;
        let before = peak_rss_kb(server.id());
        let upload = varied_bytes(64 * MIB);
        let expected = block_digest(&upload);
        let response = uploaded(port, "/hash", upload)?;
        expect_status(&response, "HTTP/1.1 200 OK")?;
        expect_body(&response, &expected)?;
        let (Some(before), Some(after)) = (before, peak_rss_kb(server.id())) else {
            return Ok(());
        };
        let grown = after - before;
        let bound = match backend {
            Backend::Native => 8 * 1024,
            Backend::Node => 48 * 1024,
        };
        eprintln!(
            "{backend:?}: a 64 MiB upload hashed from the stream grew the peak by {grown} kB"
        );
        if grown > bound {
            return Err(format!(
                "a 64 MiB upload read as a stream grew the server's peak by {grown} kB (bound \
                 {bound} kB)"
            ));
        }
        Ok(())
    })();
    server.stop();
    outcome
}

#[test]
fn m176_a_64_mib_upload_is_hashed_from_the_stream_at_a_bounded_cost_on_both_backends() {
    let built = Built::new("request_body_stream_cost", &stream_server());
    let mut failures = Vec::new();
    for backend in [Backend::Node, Backend::Native] {
        if let Err(failure) = the_upload_is_hashed_at_a_bounded_cost(&built, backend) {
            failures.push(format!("{backend:?}: {failure}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// The whole body arrives through the stream, in order, whatever the piece
/// size: a 3 MiB upload hashed at 64 KiB pieces and with a `size` far past the
/// body.
fn the_stream_carries_the_whole_body(port: u16, _backend: Backend) -> Result<(), String> {
    let upload = varied_bytes(3 * MIB + 12345);
    let expected = block_digest(&upload);
    expect_body(&uploaded(port, "/hash", upload.clone())?, &expected)?;
    expect_body(&uploaded(port, "/hash-huge", upload)?, &expected)
}

/// `/pieces-..`'s answer, as `(pieces, total, largest, after)`.
fn pieces_of(response: &str) -> Result<(usize, usize, usize, String), String> {
    let field = |name: &str| {
        body_of(response)
            .split(' ')
            .find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
            .map(str::to_string)
            .ok_or_else(|| format!("no `{name}` in:\n{response}"))
    };
    let number = |name: &str| {
        field(name)?
            .parse::<usize>()
            .map_err(|error| format!("`{name}`: {error}"))
    };
    Ok((
        number("pieces")?,
        number("len")?,
        number("largest")?,
        field("after")?,
    ))
}

/// A `next(size)` larger than the body answers what there is, then `None` —
/// and `None` again after the end. A small size splits what arrived and never
/// hands out more than it asked for; an empty body ends at once.
fn a_size_past_the_body_answers_what_there_is(port: u16, _backend: Backend) -> Result<(), String> {
    let response = exchange(port, &post_head("/pieces-huge", 1000), &[b'a'; 1000])?;
    let (pieces, total, _largest, after) = pieces_of(&response)?;
    if pieces == 0 || total != 1000 || after != "none" {
        return Err(format!(
            "expected the 1000 bytes and then `None`, twice:\n{response}"
        ));
    }
    let response = exchange(port, &post_head("/pieces-7", 100), &[b'a'; 100])?;
    let (pieces, total, largest, after) = pieces_of(&response)?;
    if pieces < 15 || total != 100 || largest != 7 || after != "none" {
        return Err(format!(
            "expected 100 bytes in pieces of at most 7, then `None`:\n{response}"
        ));
    }
    let response = exchange(port, &post_head("/pieces-huge", 0), b"")?;
    expect_body(&response, "pieces=0 len=0 largest=0 after=none")
}

/// A chunked body is decoded through the stream: the pieces are the data, not
/// the framing.
fn a_chunked_body_streams_decoded(port: u16, _backend: Backend) -> Result<(), String> {
    let data = varied_bytes(200 * 1024);
    let mut wire = Vec::new();
    let mut offset = 0;
    // Uneven chunks: one byte, a prime, a block, the rest.
    for size in [1usize, 4093, 65536, data.len()] {
        let size = size.min(data.len() - offset);
        if size == 0 {
            break;
        }
        wire.extend_from_slice(format!("{size:x}\r\n").as_bytes());
        wire.extend_from_slice(&data[offset..offset + size]);
        wire.extend_from_slice(b"\r\n");
        offset += size;
    }
    wire.extend_from_slice(b"0\r\n\r\n");
    let response = exchange(port, &chunked_head("/hash"), &wire)?;
    expect_status(&response, "HTTP/1.1 200 OK")?;
    expect_body(&response, &block_digest(&data))
}

/// A declared `Content-Length` over the cap is refused at the stream's first
/// read, before a byte of the body is sent.
fn a_declared_body_over_the_cap_is_refused_at_the_first_read(
    port: u16,
    _backend: Backend,
) -> Result<(), String> {
    let response = exchange(port, &post_head_keep_alive("/count", 2 * MIB), b"")?;
    expect_status(&response, "HTTP/1.1 413 Payload Too Large")?;
    expect_header(&response, "Connection: close")?;
    expect_body(&response, DEFAULT_REFUSAL)
}

/// The cap MID-STREAM: a chunked body read piece by piece is refused at the
/// chunk that crosses 1 MiB — after the handler has read most of a mebibyte —
/// with the same `413`, and the server reads none of what the client keeps
/// writing after it (the census, B687: the writer stalls).
fn the_cap_refuses_mid_stream_and_the_writer_stalls(
    port: u16,
    _backend: Backend,
) -> Result<(), String> {
    let before = refused_body_read(port)?;
    let outcome = streamed(port, &chunked_head("/count"), 64 * MIB, true)?;
    expect_status(&outcome, "HTTP/1.1 413 Payload Too Large")?;
    expect_body(&outcome, DEFAULT_REFUSAL)?;
    let after = refused_body_read(port)?;
    if after != before {
        return Err(format!(
            "the server read {} body bytes after refusing the stream",
            after - before
        ));
    }
    Ok(())
}

/// A body is read whole or as a stream, not both: either order is refused
/// `500` with a sentence naming both calls, and the handler ends there (its
/// own answer, which says `read both ways`, never goes out).
fn a_body_is_read_one_way(port: u16, _backend: Backend) -> Result<(), String> {
    let response = exchange(port, &post_head("/stream-then-bytes", 10), b"0123456789")?;
    expect_status(&response, "HTTP/1.1 500 Internal Server Error")?;
    expect_body(
        &response,
        "`Request::bytes()` or `body()` was called after `Request::body_stream()` had started \
         reading the body as a stream: a request body is read whole or as a stream, not both",
    )?;
    let response = exchange(port, &post_head("/bytes-then-stream", 10), b"0123456789")?;
    expect_status(&response, "HTTP/1.1 500 Internal Server Error")?;
    expect_body(
        &response,
        "`Request::body_stream()` was called after `Request::bytes()` or `body()` had read the \
         body whole: a request body is read whole or as a stream, not both",
    )?;
    // Two readers of one stream are one stream: the second continues.
    let response = exchange(port, &post_head("/stream-twice", 10), b"0123456789")?;
    expect_body(&response, "first=5 rest=5")
}

/// A stream the handler leaves unfinished is an unread body (M164's rule):
/// within the cap it is drained — the connection stays alive and a request
/// pipelined behind it is answered, on both backends since the native server
/// keeps connections (F152) — and past it, chunked, the connection closes
/// with the census still at zero.
fn an_unfinished_stream_is_discarded_as_an_unread_body(
    port: u16,
    _backend: Backend,
) -> Result<(), String> {
    let body = vec![b'a'; 200 * 1024];
    let mut wire = format!(
        "POST /partial HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    wire.extend_from_slice(&body);
    wire.extend_from_slice(b"POST /partial HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 12\r\nConnection: close\r\n\r\nhello world!");
    let mut stream = connect(port)?;
    stream
        .write_all(&wire)
        .map_err(|error| format!("send: {error}"))?;
    let response = read_response(&mut stream)?;
    let answered = response.matches("took=10").count();
    let expected = 2;
    if answered != expected {
        return Err(format!(
            "expected {expected} answer(s) on the connection, got {answered}:\n{response}"
        ));
    }
    let before = refused_body_read(port)?;
    let outcome = streamed(port, &chunked_head("/partial"), 4 * MIB, true)?;
    expect_status(&outcome, "HTTP/1.1 200 OK")?;
    expect_header(&outcome, "Connection: close")?;
    expect_body(&outcome, "took=10")?;
    let after = refused_body_read(port)?;
    if after != before {
        return Err(format!(
            "the server read {} body bytes of a stream it had left",
            after - before
        ));
    }
    Ok(())
}

#[test]
fn m176_a_body_read_as_a_stream_on_both_backends() {
    let built = Built::new("request_body_stream", &stream_server());
    built.check(&[
        (
            "the stream carries the whole body",
            the_stream_carries_the_whole_body,
        ),
        (
            "a size past the body answers what there is",
            a_size_past_the_body_answers_what_there_is,
        ),
        (
            "a chunked body streams decoded",
            a_chunked_body_streams_decoded,
        ),
        (
            "a declared body over the cap is refused at the first read",
            a_declared_body_over_the_cap_is_refused_at_the_first_read,
        ),
        (
            "the cap refuses mid-stream and the writer stalls",
            the_cap_refuses_mid_stream_and_the_writer_stalls,
        ),
        ("a body is read one way", a_body_is_read_one_way),
        (
            "an unfinished stream is discarded as an unread body",
            an_unfinished_stream_is_discarded_as_an_unread_body,
        ),
    ]);
}

// ----------------------------------------------------------- the form ---

/// M177's program: `multipart/form-data` read off the stream. `/form-<size>`
/// describes every part — its name, filename and type, and its bytes digested
/// as `/hash` digests a body, read with `next(<size>)` — and `/form-skip`
/// passes the part named `file` over unread. A mounted rpc service shows its
/// POST leg still refusing a form.
const FORM_SERVER: &str = r#"import std::io::print;
import std::bytes::Bytes;
import std::crypto::sha256;
import std::http::{ Multipart, PartReader, Request, Response, Server };
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

fun digest(reader: PartReader, size: usize): str {
	let block_size = 262144usize;
	let block = Bytes::alloc(block_size);
	mut filled = 0usize;
	mut total = 0usize;
	mut digests = Bytes::alloc(0);
	for {
		let wanted = if block_size - filled < size { block_size - filled } else { size };
		let piece = match reader.next(wanted) {
			Some(let piece) => piece,
			None => jump break,
		};
		block.copy_into(piece, filled);
		filled = filled + piece.len();
		total = total + piece.len();
		if filled == block_size {
			digests = Bytes::concat(digests, sha256(block));
			filled = 0;
		}
	}
	if filled > 0 {
		digests = Bytes::concat(digests, sha256(block.slice(0, filled)));
	}
	i"len={total} sha={sha256(digests).to_hex()}"
}

fun describe(request: Request, size: usize, skip: str): Response {
	let form = Multipart::of(request.max_body_bytes(100 * 1024 * 1024));
	mut lines = "";
	for {
		let part = match form.next_part() {
			Some(let part) => part,
			None => jump break,
		};
		let file = match part.filename {
			Some(let name) => i" filename={name}",
			None => "",
		};
		let content = if part.name == skip { "skipped" } else { digest(part.reader, size) };
		lines = lines + i"[{part.name}{file} type={part.content_type}] {content}\n";
	}
	Response::builder().body(lines).build()
}

fun main() {
	let counter = Counter { count = Shared::new(0) };
	Server::builder()
		.port(0)
		.with_service(Service::new(counter.dispatcher().into_protocol(json_codec())).at("/svc/"))
		.on_request(|request| {
			let path = request.path();
			if path == "/form-65536" {
				ret describe(request, 65536, "");
			}
			if path == "/form-4096" {
				ret describe(request, 4096, "");
			}
			if path == "/form-7" {
				ret describe(request, 7, "");
			}
			if path == "/form-1" {
				ret describe(request, 1, "");
			}
			if path == "/form-skip" {
				ret describe(request, 65536, "file");
			}
			Response::builder().code(404).body("no route").build()
		})
		.on_start(|server| ANNOUNCE)
		.build()
		.start();
}
"#;

fn form_server() -> String {
    FORM_SERVER.replace("ANNOUNCE", ANNOUNCE_PORT)
}

const BOUNDARY: &str = "----vilanBoundary7MA4YWxkTrZu0gW";

fn form_type() -> String {
    format!("multipart/form-data; boundary={BOUNDARY}")
}

/// A form body: a preamble, each part's head lines and bytes, the closing
/// delimiter and an epilogue — the two a parser must skip.
fn form_body(parts: &[(&[&str], &[u8])]) -> Vec<u8> {
    let mut body = b"a preamble the form skips\r\n".to_vec();
    for (headers, data) in parts {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        for header in *headers {
            body.extend_from_slice(header.as_bytes());
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(data);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\nan epilogue").as_bytes());
    body
}

/// POSTs `body` as `content_type` to `path` from a second thread while this one
/// reads the response.
fn posted_form(port: u16, path: &str, content_type: &str, body: Vec<u8>) -> Result<String, String> {
    let mut stream = connect(port)?;
    stream
        .write_all(
            format!(
                "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: {content_type}\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .as_bytes(),
        )
        .map_err(|error| format!("send the head: {error}"))?;
    let mut writer = stream
        .try_clone()
        .map_err(|error| format!("a second handle on the socket: {error}"))?;
    let sender = std::thread::spawn(move || {
        for piece in body.chunks(64 * 1024) {
            if writer.write_all(piece).is_err() {
                return;
            }
        }
    });
    let response = read_response(&mut stream);
    let _ = stream.shutdown(std::net::Shutdown::Both);
    sender.join().expect("the sending thread");
    response
}

const TITLE_HEADERS: &[&str] = &["Content-Disposition: form-data; name=\"title\""];
const FILE_HEADERS: &[&str] = &[
    "Content-Disposition: form-data; name=\"file\"; filename=\"a;b \\\"q\\\".bin\"",
    "Content-Type: application/octet-stream",
];
const TITLE: &[u8] = b"hello, form";

/// `/form-..`'s description of the title-and-file form.
fn described(file: &[u8], skipped: bool) -> String {
    let file_content = if skipped {
        "skipped".to_string()
    } else {
        block_digest(file)
    };
    format!(
        "[title type=text/plain] {}\n[file filename=a;b \"q\".bin type=application/octet-stream] \
         {file_content}\n",
        block_digest(TITLE)
    )
}

/// M177's measurement: a two-part form whose file part is 10 MiB, read at
/// 64 KiB pieces, grows the server's peak resident set by what the stream
/// holds — the bounds and their reasons are M176's (measured past a 200 KiB
/// warm-up form: natively 80 kB, on node 21-27 MB of collector lag, where a
/// 10 MiB body read whole costs 10.1 MB and 22 MB).
fn a_10_mib_file_part_is_read_at_a_bounded_cost(
    built: &Built,
    backend: Backend,
) -> Result<(), String> {
    let mut server = built.spawn(backend);
    let port = server.port();
    let outcome = (|| {
        let warm = varied_bytes(200 * 1024);
        let response = posted_form(
            port,
            "/form-65536",
            &form_type(),
            form_body(&[(TITLE_HEADERS, TITLE), (FILE_HEADERS, &warm)]),
        )?;
        expect_body(&response, &described(&warm, false))?;
        let before = peak_rss_kb(server.id());
        let file = varied_bytes(10 * MIB);
        let response = posted_form(
            port,
            "/form-65536",
            &form_type(),
            form_body(&[(TITLE_HEADERS, TITLE), (FILE_HEADERS, &file)]),
        )?;
        expect_status(&response, "HTTP/1.1 200 OK")?;
        expect_body(&response, &described(&file, false))?;
        let (Some(before), Some(after)) = (before, peak_rss_kb(server.id())) else {
            return Ok(());
        };
        let grown = after - before;
        let bound = match backend {
            Backend::Native => 8 * 1024,
            Backend::Node => 48 * 1024,
        };
        eprintln!("{backend:?}: a form with a 10 MiB file part grew the peak by {grown} kB");
        if grown > bound {
            return Err(format!(
                "a form with a 10 MiB file part grew the server's peak by {grown} kB (bound \
                 {bound} kB)"
            ));
        }
        Ok(())
    })();
    server.stop();
    outcome
}

#[test]
fn m177_a_10_mib_file_part_is_read_at_a_bounded_cost_on_both_backends() {
    let built = Built::new("request_body_form_cost", &form_server());
    let mut failures = Vec::new();
    for backend in [Backend::Node, Backend::Native] {
        if let Err(failure) = a_10_mib_file_part_is_read_at_a_bounded_cost(&built, backend) {
            failures.push(format!("{backend:?}: {failure}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// A delimiter split across the body's pieces is found whole: the file part
/// read at `next(1)`, `next(7)` and `next(4096)` — each read of the part takes
/// the body that size at a time once what the head's read brought in is used
/// up, so the closing delimiter arrives across many piece edges — and at
/// 64 KiB, all to the same bytes. The file is 200 KiB, past the 64 KiB the
/// head's read takes, and 1 byte off a multiple of 7 and of 4096.
fn a_delimiter_split_across_pieces_is_found(port: u16, _backend: Backend) -> Result<(), String> {
    let file = varied_bytes(200 * 1024 + 4093);
    let body = form_body(&[(TITLE_HEADERS, TITLE), (FILE_HEADERS, &file)]);
    for path in ["/form-1", "/form-7", "/form-4096", "/form-65536"] {
        let response = posted_form(port, path, &form_type(), body.clone())?;
        expect_status(&response, "HTTP/1.1 200 OK")
            .and_then(|()| expect_body(&response, &described(&file, false)))
            .map_err(|failure| format!("{path}: {failure}"))?;
    }
    Ok(())
}

/// A part passed over unread is drained by the next `next_part`, which finds
/// the part after it: here a third field after the skipped file.
fn a_part_skipped_unread_is_drained(port: u16, _backend: Backend) -> Result<(), String> {
    let file = varied_bytes(300 * 1024);
    let after_headers: &[&str] = &["Content-Disposition: form-data; name=\"after\""];
    let body = form_body(&[
        (TITLE_HEADERS, TITLE),
        (FILE_HEADERS, &file),
        (after_headers, b"the last field"),
    ]);
    let response = posted_form(port, "/form-skip", &form_type(), body)?;
    expect_status(&response, "HTTP/1.1 200 OK")?;
    expect_body(
        &response,
        &format!(
            "{}[after type=text/plain] {}\n",
            described(&file, true),
            block_digest(b"the last field")
        ),
    )
}

/// A malformed form is refused `400` with the defect named.
fn a_malformed_form_is_refused_with_its_defect(port: u16, _backend: Backend) -> Result<(), String> {
    let file = varied_bytes(1000);
    let good = form_body(&[(TITLE_HEADERS, TITLE), (FILE_HEADERS, &file)]);
    let no_colon = form_body(&[
        (TITLE_HEADERS, TITLE),
        (
            &[
                "Content-Disposition: form-data; name=\"file\"",
                "Content-Type application/octet-stream",
            ],
            &file,
        ),
    ]);
    let mut padded_wrong = b"\r\n".to_vec();
    padded_wrong.extend_from_slice(format!("--{BOUNDARY}xyz\r\n").as_bytes());
    padded_wrong.extend_from_slice(&good);
    let cases: [(&str, String, Vec<u8>, String); 6] = [
        (
            "no boundary",
            "multipart/form-data".to_string(),
            good.clone(),
            "the multipart/form-data request names no boundary in its Content-Type \
             (`multipart/form-data`)"
                .to_string(),
        ),
        (
            "not a form",
            "application/json".to_string(),
            good.clone(),
            "the request body is not multipart/form-data (its Content-Type is \
             `application/json`)"
                .to_string(),
        ),
        (
            "a missing terminator",
            form_type(),
            good[..good.len() - 40].to_vec(),
            format!(
                "malformed multipart/form-data: the body ended before the closing \
                 `--{BOUNDARY}--` delimiter"
            ),
        ),
        (
            "a header without a colon",
            form_type(),
            no_colon,
            "malformed multipart/form-data: a part header has no colon: `Content-Type \
             application/octet-stream`"
                .to_string(),
        ),
        (
            "a delimiter line with text after the boundary",
            form_type(),
            padded_wrong,
            format!(
                "malformed multipart/form-data: a delimiter line carries `xyz` after \
                 `--{BOUNDARY}`, where only `--` or the line end may follow"
            ),
        ),
        (
            "no delimiter at all",
            form_type(),
            b"just some text".to_vec(),
            format!("malformed multipart/form-data: the body has no `--{BOUNDARY}` delimiter"),
        ),
    ];
    for (label, content_type, body, message) in cases {
        let response = posted_form(port, "/form-65536", &content_type, body)?;
        expect_status(&response, "HTTP/1.1 400 Bad Request")
            .and_then(|()| expect_body(&response, &message))
            .map_err(|failure| format!("{label}: {failure}"))?;
    }
    Ok(())
}

/// The rpc POST leg still refuses a form (its CSRF gate, `docs/std/rpc.md`): a
/// `multipart/form-data` frame is a cross-site form's, answered `400` before
/// the codec sees it.
fn the_rpc_post_leg_still_refuses_a_form(port: u16, _backend: Backend) -> Result<(), String> {
    let response = posted_form(
        port,
        "/svc/rpc",
        &form_type(),
        form_body(&[(TITLE_HEADERS, b"{\"method\":\"add\",\"args\":[1]}")]),
    )?;
    expect_status(&response, "HTTP/1.1 400 Bad Request")?;
    if !body_of(&response).starts_with("an rpc frame is posted as ") {
        return Err(format!(
            "expected the content-type gate's refusal, got:\n{response}"
        ));
    }
    Ok(())
}

#[test]
fn m177_a_multipart_form_read_part_by_part_on_both_backends() {
    let built = Built::new("request_body_form", &form_server());
    built.check(&[
        (
            "a delimiter split across pieces is found",
            a_delimiter_split_across_pieces_is_found,
        ),
        (
            "a part skipped unread is drained",
            a_part_skipped_unread_is_drained,
        ),
        (
            "a malformed form is refused with its defect",
            a_malformed_form_is_refused_with_its_defect,
        ),
        (
            "the rpc POST leg still refuses a form",
            the_rpc_post_leg_still_refuses_a_form,
        ),
    ]);
}
