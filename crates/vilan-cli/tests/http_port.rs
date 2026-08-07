//! End-to-end test for `std::http`'s bound port (backlog E19): a program asks
//! for port 0 — "any free port" — and the `Server` handed to `on_start` reports
//! the one the OS actually gave it. The pin is that the reported number is a
//! real, reachable listener: the harness reads it off stdout and speaks HTTP to
//! it from OUTSIDE the process.
//!
//! This is what retires the probe-then-substitute pattern the port-using suites
//! used to share (bind an ephemeral port, release it, hope nothing takes it
//! before the program binds — a race that struck three times in one day).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

fn temp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vilan_http_port_{tag}_{}", std::process::id()));
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

/// One HTTP GET over a fresh connection, returning the whole response text.
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

#[test]
fn a_port_zero_server_reports_the_port_it_bound() {
    let dir = temp_project("zero");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    write(
        &dir,
        "src/main.vl",
        r#"import std::print;
import std::http::{ Server, Response };

fun main() {
	// Port 0 asks the OS for any free port; `on_start`'s server carries the
	// one it got, and `url()` is built from the same number.
	Server::builder()
		.port(0)
		.on_request(|request| Response::builder().body("pong").build())
		.on_start(|server| print(i"port={server.port()} url={server.url()}"))
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

    // Not the requested 0, and not a stale literal: a real OS-assigned port.
    assert_ne!(port, 0, "port 0 was reported back instead of the bound one");
    assert!(
        announced.contains(&format!("url=http://localhost:{port}/")),
        "url() must be built from the bound port, got: {announced}"
    );

    // Reachable from another process — the claim the number is only worth
    // anything for.
    let response = http_get(port, "/ping");
    assert!(
        response.starts_with("HTTP/1.1 200"),
        "the reported port did not serve: {response}"
    );
    assert!(
        response.ends_with("pong"),
        "wrong body from the reported port: {response}"
    );

    drop(server);
    let _ = std::fs::remove_dir_all(&dir);
}
