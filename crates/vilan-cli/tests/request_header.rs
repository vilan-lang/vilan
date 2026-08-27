//! End-to-end test for `std::http`'s `Request::header` (kolt.local 025 ask a):
//! a request header is readable through the supported surface, so a conditional
//! request (`If-None-Match`) can be answered in user land at all.
//!
//! The pin has to speak raw HTTP from outside the process, because the three
//! claims are all about what the WIRE carried: a header that is present, one
//! that is absent, and one whose name the caller spells in a different case
//! than the client sent. Node lowercases every incoming header name, so the
//! case leg is the one that fails if `header` forwards the name unchanged.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

fn temp_project(tag: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("vilan_request_header_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// A node child whose stdout lines stream to a channel; killed on drop so a
/// panic cannot leak a listener.
struct ServerChild {
    child: Child,
    lines: Receiver<String>,
}

impl ServerChild {
    fn spawn(bundle: &Path) -> ServerChild {
        let mut child = Command::new("node")
            .arg(bundle)
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
        ServerChild { child, lines }
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
                Err(_) => panic!("server stdout ended before `{needle}`"),
            }
        }
    }
}

impl Drop for ServerChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One GET over a fresh connection, carrying `headers` verbatim on the wire.
fn http_get(port: u16, path: &str, headers: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect to the reported port");
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{headers}Connection: close\r\n\r\n")
                .as_bytes(),
        )
        .expect("send the request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read the response");
    response
}

fn body_of(response: &str) -> &str {
    response
        .split_once("\r\n\r\n")
        .map(|(_head, body)| body)
        .unwrap_or(response)
}

#[test]
fn a_request_header_is_readable_present_absent_and_case_insensitively() {
    let dir = temp_project("read");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    // The handler asks with the CANONICAL casing (`If-None-Match`), which is
    // never what node stores — the accessor is what bridges the two. The
    // absent header must read `None` rather than an empty string, so that
    // "sent with no value" and "not sent" stay distinguishable.
    write(
        &dir,
        "src/main.vl",
        r#"import std::print;
import std::http::{ Server, Response };
import std::option::Option::{ None, Some, self };

fun show(label: str, value: Option<str>): str {
	match value {
		Some(let text) => i"{label}={text}",
		None => i"{label}=<none>",
	}
}

fun main() {
	Server::builder()
		.port(0)
		.on_request(|request| {
			let etag = show("etag", request.header("If-None-Match"));
			let absent = show("absent", request.header("X-Not-Sent"));
			let dup = show("dup", request.header("x-dup"));
			let proto = show("proto", request.header("constructor"));
			Response::builder().body(i"{etag} {absent} {dup} {proto}").build()
		})
		.on_start(|server| print(i"port={server.port()}"))
		.build()
		.start();
}
"#,
    );

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

    let server = ServerChild::spawn(&dir.join("src/main.mjs"));
    let announced = server.await_line("port=", Duration::from_secs(60));
    let port: u16 = announced
        .split_whitespace()
        .find_map(|field| field.strip_prefix("port="))
        .expect("the announced line carries `port=<n>`")
        .parse()
        .expect("the announced port is a number");

    // The client sends `If-None-Match` in yet a THIRD casing, so neither side
    // of the comparison is already lowercase on the wire.
    let response = http_get(
        port,
        "/",
        "IF-NONE-MATCH: \"v1\"\r\nX-Dup: one\r\nX-Dup: two\r\n",
    );
    assert!(
        response.starts_with("HTTP/1.1 200"),
        "the server did not answer: {response}"
    );
    let body = body_of(&response);

    // Present, found through a case-mismatched lookup on BOTH sides.
    assert!(
        body.contains("etag=\"v1\""),
        "a present header read back wrong (case-insensitive lookup): {body}"
    );
    // Absent is `None`, not `Some("")`.
    assert!(
        body.contains("absent=<none>"),
        "an absent header must read None: {body}"
    );
    // A repeated ordinary header reads back as node joined it — documented
    // behaviour, pinned so it cannot drift into silently dropping one.
    assert!(
        body.contains("dup=one, two"),
        "a repeated header must read back node's join: {body}"
    );

    // An INHERITED property name is not a header: presence is `Object.hasOwn`,
    // so `constructor` reads None rather than stringifying Object.prototype's.
    assert!(
        body.contains("proto=<none>"),
        "an inherited property name must not read back as a header: {body}"
    );

    drop(server);
    let _ = std::fs::remove_dir_all(&dir);
}
