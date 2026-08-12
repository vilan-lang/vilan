//! Rung 1 — the served build (proposal/fullstack-dx.md §5.4, S3) and its
//! dev-mode freshness policy (proposal/dev-refresh.md §5, item 1).
//!
//! `ServerBuilder::serve_build(build_of("client")!)` replaces the three boot
//! reads and the five-line content-type table every server in this language
//! used to write. Four claims are pinned here, each of which the ceremony it
//! replaces could not make:
//!
//!   1. one route per artifact, at `/<file name>`, with the content type its
//!      extension implies — and every path they do not claim still reaches the
//!      app's own handler, whatever order the chain was written in;
//!   2. **a leg that gains `split = true` serves its chunks with no server
//!      edit** — planted by adding `split` to a project whose server file is
//!      then asserted byte-identical;
//!   3. an artifact the build named and did not write stops the server, naming
//!      the file and the leg, instead of 404ing per request for the life of the
//!      process;
//!   4. the dev policy, both ways: bytes that move under a running server are
//!      served fresh while `run --watch` owns it (E55's defect) and served from
//!      the boot-time copy otherwise.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

mod support;

/// A browser leg that compiles a `const style()`, so its build emits a sidecar
/// and the manifest names one — two artifacts to route instead of one.
const STYLED_CLIENT: &str = r#"import std::style::{ Display, Style, style };
import std::ui::{ mount_root, view };

fun panel(): Style {
	style().display(Display::Flex)
}

fun main() {
	let card = const panel();
	let _root = mount_root("app", || view("main").styled(card).text("served"));
}
"#;

/// A two-entry project: a browser client and a server that serves its build and
/// nothing else. `Client::Router` is the split fixture's own entry, so
/// `split = true` has three arms to chunk; `Client::Styled` emits a stylesheet.
#[derive(Clone, Copy, PartialEq)]
enum Client {
    Router,
    Styled,
}

fn stage(tag: &str, port: u16, client: Client, split: bool) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let staged = std::env::temp_dir().join(format!(
        "vilan_serve_build_{tag}_{}_{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&staged);
    std::fs::create_dir_all(staged.join("src")).expect("create the staging directory");
    write_manifest(&staged, split);
    let source = match client {
        Client::Router => std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/split/project/app.vl"),
        )
        .expect("the split fixture's client"),
        Client::Styled => STYLED_CLIENT.to_string(),
    };
    std::fs::write(staged.join("src/client.vl"), source).expect("write the client");
    std::fs::write(staged.join("src/server.vl"), server_source(port)).expect("write the server");
    staged
}

/// The manifest, with or without the client leg's `split`. Writing it is the
/// ONLY edit the split pin makes — the server file is asserted unchanged.
fn write_manifest(staged: &Path, split: bool) {
    let client = if split {
        "[entry.client]\ntarget = \"browser\"\nsplit = true\n"
    } else {
        "[entry.client]\ntarget = \"browser\"\n"
    };
    std::fs::write(
        staged.join("vilan.toml"),
        format!("[package]\nname = \"served\"\n\n{client}\n[entry.server]\n"),
    )
    .expect("write the manifest");
}

fn server_source(port: u16) -> String {
    format!(
        "import std::build::require_build;\n\
         import std::http::{{ Request, Response, Server }};\n\
         import std::io::print;\n\
         \n\
         async fun main() {{\n\
         \tlet build = require_build(\"client\");\n\
         \tServer::builder()\n\
         \t\t.port({port})\n\
         \t\t.serve_build(build)\n\
         \t\t.on_request(|request| Response::builder().set_header(\"Content-Type\", \"text/html\").body(\"<div id=\\\"app\\\"></div>\").build())\n\
         \t\t.on_start(|server| print(\"listening\"))\n\
         \t\t.build()\n\
         \t\t.start();\n\
         }}\n"
    )
}

fn vilan(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .expect("run vilan")
}

fn build(staged: &Path) {
    let output = vilan(&["build", staged.to_str().expect("utf-8 temp path")]);
    assert!(
        output.status.success(),
        "vilan build failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Bind an ephemeral port and release it — the standard small TOCTOU window
/// this suite's server tests all take.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind an ephemeral port")
        .local_addr()
        .expect("read the bound address")
        .port()
}

/// Spawn the built server from the project root, with `env` applied — which is
/// how the dev policy's two modes are told apart.
fn serve(staged: &Path, env: &[(&str, &str)]) -> Child {
    let mut command = Command::new("node");
    command
        .arg("dist/server.mjs")
        .current_dir(staged)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (name, value) in env {
        command.env(name, value);
    }
    command.spawn().expect("spawn the server")
}

fn wait_for_port(port: u16) -> bool {
    let deadline = Instant::now() + support::run_liveness();
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// A plain HTTP GET, returning `(status line + headers, body)`.
fn http_get(port: u16, path: &str) -> (String, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect for GET");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set a read timeout");
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .expect("send GET");
    let mut response = Vec::new();
    let _ = stream.read_to_end(&mut response);
    let text = String::from_utf8_lossy(&response).into_owned();
    match text.split_once("\r\n\r\n") {
        Some((head, body)) => (head.to_string(), body.to_string()),
        None => (text, String::new()),
    }
}

fn stop(server: &mut Child) {
    let _ = server.kill();
    let _ = server.wait();
}

#[test]
fn serve_build_answers_every_artifact_and_leaves_the_rest_to_the_app() {
    let port = free_port();
    let staged = stage("routes", port, Client::Styled, false);
    build(&staged);
    let mut server = serve(&staged, &[]);
    assert!(wait_for_port(port), "the server should bind {port}");

    let (head, body) = http_get(port, "/client.js");
    assert!(
        head.contains("Content-Type: text/javascript"),
        "the bundle's extension implies its content type:\n{head}"
    );
    assert_eq!(
        body,
        std::fs::read_to_string(staged.join("dist/client.js")).expect("the bundle"),
        "and the bytes are the build's own"
    );

    let (head, body) = http_get(port, "/client.css");
    assert!(
        head.contains("Content-Type: text/css"),
        "the style sidecar too — the build said it emitted one:\n{head}"
    );
    assert_eq!(
        body,
        std::fs::read_to_string(staged.join("dist/client.css")).expect("the sidecar")
    );

    // A cache-buster is not a different file.
    let (head, _) = http_get(port, "/client.js?v=2");
    assert!(
        head.contains("Content-Type: text/javascript"),
        "a query string does not change which artifact was asked for:\n{head}"
    );

    // …and everything the build does not claim still reaches `on_request`,
    // which is what keeps a client-routed SPA's deep links working.
    let (head, body) = http_get(port, "/some/deep/link");
    assert!(
        head.contains("Content-Type: text/html") && body.contains("id=\"app\""),
        "an unclaimed path falls through to the app's own handler:\n{head}\n{body}"
    );
    // The `dist/` path is not a route: `serve_build` serves a build, not a
    // directory.
    let (_, body) = http_get(port, "/dist/client.js");
    assert!(
        body.contains("id=\"app\""),
        "nothing but `/<file name>` is claimed"
    );

    stop(&mut server);
    let _ = std::fs::remove_dir_all(&staged);
}

#[test]
fn a_leg_that_gains_split_serves_its_chunks_with_no_server_edit() {
    // The S3 gate, and what `bundle-splitting.md` §3 wanted the sidecar for in
    // the first place. The ONLY edit between the two halves of this test is
    // `split = true` in the manifest; the server file is asserted byte-identical
    // across it, so the routes can only have come from the build.
    let port = free_port();
    let staged = stage("split", port, Client::Router, false);
    let server_before = std::fs::read_to_string(staged.join("src/server.vl")).expect("the server");

    build(&staged);
    let mut server = serve(&staged, &[]);
    assert!(wait_for_port(port), "the server should bind {port}");
    let (_, body) = http_get(port, "/client.Route_Home.js");
    assert!(
        body.contains("id=\"app\""),
        "with no split there is no chunk to serve, so the app's handler answers"
    );
    stop(&mut server);

    // The one edit.
    write_manifest(&staged, true);
    build(&staged);
    assert_eq!(
        std::fs::read_to_string(staged.join("src/server.vl")).expect("the server"),
        server_before,
        "the server file must not move — that is the whole claim"
    );

    let mut server = serve(&staged, &[]);
    assert!(wait_for_port(port), "the server should bind {port} again");
    let mut served = 0;
    for arm in ["Route_Home", "Route_Docs", "Route_NotFound"] {
        let file = format!("client.{arm}.js");
        let on_disk = std::fs::read_to_string(staged.join("dist").join(&file))
            .unwrap_or_else(|error| panic!("read dist/{file}: {error}"));
        let (head, body) = http_get(port, &format!("/{file}"));
        assert!(
            head.contains("Content-Type: text/javascript"),
            "the chunk route carries the chunk's content type:\n{head}"
        );
        assert_eq!(body, on_disk, "/{file} should serve the emitted chunk");
        served += 1;
    }
    assert_eq!(served, 3, "every arm's chunk appeared as a route");
    stop(&mut server);
    let _ = std::fs::remove_dir_all(&staged);
}

#[test]
fn an_artifact_the_build_named_and_did_not_write_stops_the_server() {
    // §5.4: a missing artifact is a broken BUILD, and it is loud at boot rather
    // than a 404 per request for the life of the process. The manifest still
    // names `client.css`, so removing the file is exactly the "the build said
    // it wrote this" case.
    let port = free_port();
    let staged = stage("missing", port, Client::Styled, false);
    build(&staged);
    std::fs::remove_file(staged.join("dist/client.css")).expect("remove the sidecar");

    let output = Command::new("node")
        .arg("dist/server.mjs")
        .current_dir(&staged)
        .output()
        .expect("run the server");
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.status.success(),
        "a server that cannot serve its own build must not start:\n{report}"
    );
    assert!(
        report.contains("dist/client.css") && report.contains("client"),
        "and must name the file and the leg:\n{report}"
    );
    let _ = std::fs::remove_dir_all(&staged);
}

#[test]
fn the_dev_policy_revalidates_only_while_watching() {
    // `dev-refresh.md` §5, item 1 — E55's headline defect, at the one call site
    // that can close it. A server holds its assets in a closure for the life of
    // the process, so bytes that move on disk under a running server were served
    // stale forever. `serve_build` re-reads per request while `run --watch` owns
    // the process, and serves the boot-time copy otherwise.
    let port = free_port();
    let staged = stage("fresh", port, Client::Styled, false);
    build(&staged);
    let bundle = staged.join("dist/client.js");
    let original = std::fs::read_to_string(&bundle).expect("the bundle");

    // Release: the boot-time copy, whatever happens on disk afterwards.
    let mut server = serve(&staged, &[]);
    assert!(wait_for_port(port), "the server should bind {port}");
    let (_, before) = http_get(port, "/client.js");
    assert_eq!(before, original);
    std::fs::write(&bundle, "// MOVED\n").expect("move the bytes");
    let (_, after) = http_get(port, "/client.js");
    assert_eq!(
        after, original,
        "outside a watch the server serves what it read at boot — no syscall per request"
    );
    stop(&mut server);
    std::fs::write(&bundle, &original).expect("restore the bundle");

    // Watching: fresh, per request, with no restart and no signalling protocol.
    let mut server = serve(&staged, &[("VILAN_WATCHING", "1")]);
    assert!(wait_for_port(port), "the watched server should bind {port}");
    let (_, before) = http_get(port, "/client.js");
    assert_eq!(before, original);
    std::fs::write(&bundle, "// MOVED\n").expect("move the bytes");
    let (_, after) = http_get(port, "/client.js");
    assert_eq!(
        after, "// MOVED\n",
        "under `run --watch` every request is an opportunity to be fresh"
    );
    stop(&mut server);
    let _ = std::fs::remove_dir_all(&staged);
}

#[test]
fn is_watching_is_false_outside_a_watch() {
    // Uniform (`dev-refresh.md` §5's scope ruling): DEFINED under every run,
    // `true` only under one — so a program branches on it without knowing how
    // it was started.
    let staged = stage("plainrun", free_port(), Client::Styled, false);
    std::fs::write(
        staged.join("src/server.vl"),
        "import std::io::print;\nimport std::process::is_watching;\n\nfun main() {\n\tprint(i\"watching={is_watching()}\");\n}\n",
    )
    .expect("write the probe");
    let output = vilan(&["run", staged.to_str().expect("utf-8 temp path")]);
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(
        report.contains("watching=false"),
        "a plain `vilan run` is not a watch:\n{report}"
    );
    let _ = std::fs::remove_dir_all(&staged);
}

#[test]
#[cfg(unix)]
fn run_watch_tells_its_child_it_is_watching() {
    // The other half: the watcher really does set the signal on the child it
    // spawns. Without this the policy above is a claim about an environment
    // variable nobody sets.
    let staged = stage("watchrun", free_port(), Client::Styled, false);
    std::fs::write(
        staged.join("src/server.vl"),
        "import std::io::print;\nimport std::process::is_watching;\nimport std::time::sleep;\n\n\
         async fun main() {\n\tprint(i\"watching={is_watching()}\");\n\tsleep(600000);\n}\n",
    )
    .expect("write the probe");

    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args([
            "run",
            "--watch",
            "--no-hmr",
            staged.to_str().expect("utf-8 temp path"),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env("NO_COLOR", "1")
        .spawn()
        .expect("spawn run --watch");

    let mut stdout = watcher.stdout.take().expect("the watcher's stdout");
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // Byte at a time: the round's output is unterminated until the child
        // prints, and a line-buffered reader would block past the answer.
        let mut seen = String::new();
        let mut byte = [0u8; 1];
        while stdout.read(&mut byte).unwrap_or(0) == 1 {
            seen.push(byte[0] as char);
            if seen.contains("watching=true") || seen.contains("watching=false") {
                break;
            }
        }
        let _ = sender.send(seen);
    });
    let seen = receiver
        .recv_timeout(support::WATCH_LIVENESS)
        .unwrap_or_default();
    support::kill_watcher(&mut watcher);
    let _ = std::fs::remove_dir_all(&staged);
    assert!(
        seen.contains("watching=true"),
        "`run --watch` must tell its Node child it is watching:\n{seen}"
    );
}
