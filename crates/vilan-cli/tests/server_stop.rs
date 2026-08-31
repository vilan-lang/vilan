//! End-to-end pins for `ServerBuilder::on_stop` made real (`fullstack-dx.md`
//! §9.1): `build()` used to drop the callback on the floor — it type-checked,
//! read correctly, and never ran, because there was no `Server::stop` to fire
//! it from. Pinned both ways: `on_stop` fires once the listener has actually
//! closed, and never fires on a server that is simply left running.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

mod support;

fn temp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vilan_server_stop_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

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
/// this exact tolerance). The abort lands strictly AFTER the program's
/// complete stdout, which every caller here asserts on; exactly this
/// assertion line is tolerated, and anything else on stderr still fails.
fn is_node_windows_teardown_noise(line: &str) -> bool {
    cfg!(windows)
        && line.starts_with("Assertion failed: !(handle->flags & UV_HANDLE_CLOSING)")
        && line.contains("async.c")
}

/// A long-running server, spawned with `node` directly against a built bundle
/// (mirrors `rpc_http.rs`'s `StreamingServer`) so a Rust test can drive it —
/// and, after it stops, probe whether the port is still listening — from
/// outside the process.
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

/// One GET over a fresh connection the server closes, returning the raw
/// response text.
fn http_get(port: u16, path: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect to the reported port");
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .expect("send the request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read the response");
    response
}

/// Whether a fresh connection attempt to `port` succeeds at all — the
/// listener-closed half of the pin. A short connect timeout so a hung
/// listener fails the test instead of the harness.
fn port_accepts_connections(port: u16) -> bool {
    TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().unwrap(),
        Duration::from_millis(500),
    )
    .is_ok()
}

/// Polls `port_accepts_connections` until it reports refused, or `timeout`
/// runs out. `node:net`'s `close()` marks the handle closed synchronously (so
/// `on_stop` firing already proves `.close()` was called) but libuv defers
/// the actual OS-level unbind to its next event-loop tick — a real, narrow
/// gap independent of this layer's own logic, observed directly against a
/// minimal `node:http` server with no vilan involved at all. A single
/// immediate check makes that libuv scheduling detail the thing under test;
/// polling for the listener to actually close (as opposed to polling for
/// nothing, ever, on a `stop()` that silently no-ops) is what the pin is for.
fn eventually_refuses_connections(port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if !port_accepts_connections(port) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn on_stop_fires_once_the_listener_has_actually_closed() {
    // `stop()` is triggered by an ordinary request (`/__stop`) rather than a
    // fixed delay, so there is no race between "the harness checks the port"
    // and "the server decided to stop on its own clock": the port is proven
    // live BEFORE the stop request, and proven closed once `on_stop`'s line
    // has printed — node's `Server.close()` refuses new connections
    // synchronously, before the close callback (and so before `on_stop`)
    // ever runs, so there is no race on that side either.
    let dir = temp_project("fires");
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
import std::option::Option::{ self, Some, None };
import std::http::{ Response, Server };

let live: Shared<Option<Server>> = Shared::new(None);

fun main() {
	Server::builder()
		.port(0)
		.on_request(|request| {
			if request.path() == "/__stop" {
				match live.read() {
					Some(let server) => server.stop(),
					None => {},
				}
			}
			Response::builder().body("hi").build()
		})
		.on_start(|server| {
			live.write() = Some(server);
			print(i"started {server.port()}");
		})
		.on_stop(|server| print(i"stopped {server.port()}"))
		.build()
		.start();
}
"#,
    );

    let server = StreamingServer::spawn(&dir);
    let started = server.await_line("started", Duration::from_secs(60));
    let port: u16 = started
        .split_whitespace()
        .next_back()
        .expect("the started line carries the bound port")
        .parse()
        .expect("the announced port is a number");

    assert!(
        port_accepts_connections(port),
        "the server did not accept connections before being asked to stop"
    );

    let stop_response = http_get(port, "/__stop");
    assert!(
        stop_response.starts_with("HTTP/1.1 200 OK"),
        "the /__stop request itself must still be answered (it stops the server, not itself): \
         {stop_response}"
    );

    let stopped = server.await_line("stopped", Duration::from_secs(10));
    assert!(
        stopped.contains(&port.to_string()),
        "on_stop's `Server` should report the same port the server bound: {stopped}"
    );
    assert!(
        eventually_refuses_connections(port, Duration::from_secs(2)),
        "on_stop fired, but the listener never stopped accepting connections — stop() must CLOSE it"
    );

    drop(server);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn on_stop_never_fires_when_the_server_is_never_stopped() {
    let dir = temp_project("never");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    write(
        &dir,
        "src/main.vl",
        r#"import std::io::print;
import std::http::{ Response, Server };
import std::process::exit;

fun main() {
	Server::builder()
		.port(0)
		.on_request(|request| Response::builder().body("hi").build())
		.on_start(|server| {
			print(i"started {server.port()}");
			// Deliberately never calls `server.stop()`.
			exit(0);
		})
		.on_stop(|server| print("on_stop:fired"))
		.build()
		.start();
}
"#,
    );
    let stdout = vilan_run_with_liveness_bound(&dir);
    assert!(
        stdout.contains("started"),
        "the server never started:\n{stdout}"
    );
    assert!(
        !stdout.contains("on_stop:fired"),
        "on_stop fired on a server that was never stopped:\n{stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
