//! End-to-end test for `std::http`'s ETag/304 helpers (kolt.local 025 ask c):
//! a browser-shaped client revalidating with `If-None-Match` gets its 304 off
//! the wire, with the validator echoed and no body — and the policy header the
//! app chained after `etag_response` rides along on BOTH arms.
//!
//! The pin speaks raw HTTP from outside the process (the `request_header.rs`
//! pattern that verified 025 ask a) because every claim here is about what the
//! wire carries: the status line, which headers each arm sends, and whether a
//! 304 leaks a body.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

fn temp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "vilan_conditional_get_{tag}_{}",
        std::process::id()
    ));
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

/// One request over a fresh connection, carrying `headers` verbatim on the
/// wire (each line `\r\n`-terminated).
fn http_request(port: u16, method: &str, path: &str, headers: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect to the reported port");
    stream
        .write_all(
            format!(
                "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{headers}Connection: close\r\n\r\n"
            )
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

/// The value of `name` in the response head, or `None` — a case-insensitive
/// scan, since header casing on the wire is the server's business.
fn header_of(response: &str, name: &str) -> Option<String> {
    let head = response.split("\r\n\r\n").next().unwrap_or(response);
    let wanted = name.to_ascii_lowercase();
    head.lines().skip(1).find_map(|line| {
        let (candidate, value) = line.split_once(':')?;
        (candidate.trim().to_ascii_lowercase() == wanted).then(|| value.trim().to_string())
    })
}

#[test]
fn a_revalidating_client_gets_304_and_every_other_form_gets_the_bytes() {
    let dir = temp_project("roundtrip");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    // The kolt-shaped composition: the validator computed ONCE at boot
    // (`etag_of`), the conditional answered per request (`etag_response`),
    // and the app's own cache policy chained after it — which must reach the
    // wire on the 304 arm as well as the 200, or a revalidated response
    // would silently shed its Cache-Control.
    write(
        &dir,
        "src/main.vl",
        r#"import std::io::print;
import std::bytes::encode_utf8;
import std::http::{ Server, etag_of, etag_response };

async fun main() {
	let page = encode_utf8("hello, cache");
	let tag = etag_of(page);
	print(i"tag={tag}");
	Server::builder()
		.port(0)
		.on_request(|request| {
			etag_response(request, tag, page, "text/plain; charset=utf-8")
				.set_header("Cache-Control", "public, max-age=3600")
				.build()
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
    let tag = server
        .await_line("tag=", Duration::from_secs(60))
        .split_once("tag=")
        .expect("the announced line carries `tag=<etag>`")
        .1
        .to_string();
    let port: u16 = server
        .await_line("port=", Duration::from_secs(60))
        .split_once("port=")
        .expect("the announced line carries `port=<n>`")
        .1
        .trim()
        .parse()
        .expect("the announced port is a number");

    // An unconditional GET: the full representation, with the validator and
    // the content type set by the helper and the policy header chained after.
    let response = http_request(port, "GET", "/", "");
    assert!(
        response.starts_with("HTTP/1.1 200"),
        "an unconditional GET must be a 200: {response}"
    );
    assert_eq!(body_of(&response), "hello, cache");
    assert_eq!(
        header_of(&response, "ETag").as_deref(),
        Some(tag.as_str()),
        "the 200 must carry the ETag the boot announced"
    );
    assert_eq!(
        header_of(&response, "Content-Type").as_deref(),
        Some("text/plain; charset=utf-8")
    );
    assert_eq!(
        header_of(&response, "Cache-Control").as_deref(),
        Some("public, max-age=3600")
    );

    // The revalidation: If-None-Match carrying the exact tag → 304, EMPTY
    // body, the ETag echoed (so the client's next conditional still matches),
    // no Content-Type (there is no content), and the chained policy header
    // still present — RFC 9110 §15.4.5's shape.
    let matched = http_request(port, "GET", "/", &format!("If-None-Match: {tag}\r\n"));
    assert!(
        matched.starts_with("HTTP/1.1 304"),
        "a matching If-None-Match must answer 304: {matched}"
    );
    assert_eq!(body_of(&matched), "", "a 304 must not carry a body");
    assert_eq!(
        header_of(&matched, "ETag").as_deref(),
        Some(tag.as_str()),
        "the 304 must echo the ETag"
    );
    assert_eq!(
        header_of(&matched, "Content-Type"),
        None,
        "a 304 has no content to type"
    );
    assert_eq!(
        header_of(&matched, "Cache-Control").as_deref(),
        Some("public, max-age=3600"),
        "the chained policy header must reach the 304 arm too"
    );

    // A stale validator → the fresh bytes, revalidator included.
    let stale = http_request(port, "GET", "/", "If-None-Match: \"0000\"\r\n");
    assert!(
        stale.starts_with("HTTP/1.1 200"),
        "a stale validator must get the bytes: {stale}"
    );
    assert_eq!(body_of(&stale), "hello, cache");
    assert_eq!(header_of(&stale, "ETag").as_deref(), Some(tag.as_str()));

    // The list form: our tag between two others still matches.
    let listed = http_request(
        port,
        "GET",
        "/",
        &format!("If-None-Match: \"aa\", {tag}, \"bb\"\r\n"),
    );
    assert!(
        listed.starts_with("HTTP/1.1 304"),
        "the list form must match: {listed}"
    );

    // `If-None-Match: *` matches any representation the server holds.
    let star = http_request(port, "GET", "/", "If-None-Match: *\r\n");
    assert!(
        star.starts_with("HTTP/1.1 304"),
        "the * form must match: {star}"
    );

    // Weak comparison, as RFC 9110 mandates for If-None-Match: a proxy that
    // weakened the tag on the way out (`W/`) must not break revalidation.
    let weak = http_request(port, "GET", "/", &format!("If-None-Match: W/{tag}\r\n"));
    assert!(
        weak.starts_with("HTTP/1.1 304"),
        "a weakened validator must still match: {weak}"
    );

    // The method gate: on a non-GET/HEAD method a matching If-None-Match
    // means 412 by RFC, never 304 — the helper's documented answer is to not
    // consult the header at all and serve the full response.
    let post = http_request(port, "POST", "/", &format!("If-None-Match: {tag}\r\n"));
    assert!(
        post.starts_with("HTTP/1.1 200"),
        "a POST must not be revalidated into a 304: {post}"
    );
    assert_eq!(body_of(&post), "hello, cache");

    drop(server);
    let _ = std::fs::remove_dir_all(&dir);
}
