//! End-to-end test for `Server` streaming responses (backlog K1): a
//! `ResponseBuilder::streaming` route writes chunks over time through the
//! held-open response — with an ASYNC `on_open` (spawn semantics; the sleeps
//! interleave with serving) — and the same process reads them back through
//! `fetch`'s body stream until the server closes. The rpc realtime/SSE suites
//! cover the service layer's SSE mount built on this; this pins the PUBLIC
//! surface directly.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

mod support;

fn temp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vilan_stream_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// Runs `vilan run <dir>` under a liveness bound — a COMPILE plus the program —
/// and returns its stdout.
///
/// The bound is `support::run_liveness()`, not the 45 s literal that stood here
/// (E40): this test's claim is that the three chunks arrive IN ORDER, which the
/// output pins on its own, so how long the box takes to build and boot the
/// server was never part of it. A stream that never closes still fails here,
/// just later.
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
                     per reference compile on this machine — stream never closed?)",
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
    assert!(
        stderr.trim().is_empty(),
        "vilan run wrote to stderr:\n{stderr}\nstdout:\n{stdout}"
    );
    stdout
}

#[test]
fn a_streaming_response_delivers_chunks_until_close() {
    let dir = temp_project("chunks");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    write(
        &dir,
        "src/main.vl",
        r#"import std::io::print;
import std::process::exit;
import std::time::sleep;
import std::http::{ Server, Response };
import std::fetch::fetch;
import std::bytes::new_text_decoder;

fun main() {
	// Port 0: the OS picks a free port and the ready callback reports it.
	Server::builder()
		.port(0)
		.on_request(|request| {
			if request.path().starts_with("/stream") {
				Response::builder()
					.set_header("Content-Type", "text/event-stream")
					.streaming(|stream| {
						stream.send("one\n");
						sleep(10);
						stream.send("two\n");
						sleep(10);
						stream.send("three\n");
						stream.close();
					})
					.build()
			} else {
				Response::builder().code(404).body("nope").build()
			}
		})
		.on_start(|server| {
			run_client(server.port());
		})
		.build()
		.start();
}

fun run_client(port: i32) {
	let response = fetch(i"http://localhost:{port}/stream");
	let reader = response.body_stream().reader();
	let decoder = new_text_decoder();
	mut received = "";
	for {
		let chunk = reader.read_chunk();
		if chunk.finished() {
			jump break;
		}
		received += decoder.decode(chunk.payload());
	}
	print(received);
	exit(0);
}
"#,
    );
    let stdout = vilan_run_with_liveness_bound(&dir);
    let one = stdout.find("one").expect("first chunk missing");
    let two = stdout.find("two").expect("second chunk missing");
    let three = stdout.find("three").expect("third chunk missing");
    assert!(one < two && two < three, "chunks out of order:\n{stdout}");
    let _ = std::fs::remove_dir_all(&dir);
}
