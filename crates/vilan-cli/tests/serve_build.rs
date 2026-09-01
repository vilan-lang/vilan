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
//!      the boot-time copy otherwise;
//!   5. **an artifact reaches the wire as the build wrote it** — byte for byte,
//!      including every byte no UTF-8 decode survives (kolt.local 030), with
//!      the content type its extension implies and the charset rule that a raw
//!      body forces (kolt.local 022).

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

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

fn stage(tag: &str, client: Client, split: bool) -> PathBuf {
    stage_serving(tag, client, split, server_source())
}

/// [`stage`] with the server file supplied, for the pins whose subject is what
/// the server chain says (the caching hook) rather than what the build emits.
fn stage_serving(tag: &str, client: Client, split: bool, server: String) -> PathBuf {
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
    std::fs::write(staged.join("src/server.vl"), server).expect("write the server");
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

/// The server under test, asking for port 0 and announcing what it got.
///
/// N40: the port used to be an ephemeral one this file bound, read and released
/// before the build that baked it in — see `support::port` for the window that
/// opened and why the server picking its own closes it.
fn server_source() -> String {
    let announce = support::port::ANNOUNCE_PORT;
    format!(
        "import std::build::require_build;\n\
         import std::http::{{ Request, Response, Server }};\n\
         import std::io::print;\n\
         \n\
         async fun main() {{\n\
         \tlet build = require_build(\"client\");\n\
         \tServer::builder()\n\
         \t\t.port(0)\n\
         \t\t.serve_build(build)\n\
         \t\t.on_request(|request| Response::builder().set_header(\"Content-Type\", \"text/html\").body(\"<div id=\\\"app\\\"></div>\").build())\n\
         \t\t.on_start(|server| {announce})\n\
         \t\t.build()\n\
         \t\t.start();\n\
         }}\n"
    )
}

/// [`server_source`] with the opt-in caching hook on the chain (kolt.local
/// 025b): the two-tier policy every static layer converges on, written as one
/// expression over the artifact's route — the stylesheet is treated as the
/// fingerprinted tier (long-lived, no validator), everything else as the shell
/// tier (revalidated, `no-cache`).
fn cached_server_source() -> String {
    let announce = support::port::ANNOUNCE_PORT;
    format!(
        "import std::build::require_build;\n\
         import std::http::{{ CachePolicy, Request, Response, Server }};\n\
         import std::io::print;\n\
         \n\
         async fun main() {{\n\
         \tlet build = require_build(\"client\");\n\
         \tServer::builder()\n\
         \t\t.port(0)\n\
         \t\t.serve_build(build)\n\
         \t\t.cache_build(|url| if url == \"/client.css\" {{\n\
         \t\t\tCachePolicy::none().cache_control(\"public, max-age=31536000, immutable\")\n\
         \t\t}} else {{\n\
         \t\t\tCachePolicy::validated().cache_control(\"no-cache\")\n\
         \t\t}})\n\
         \t\t.on_request(|request| Response::builder().set_header(\"Content-Type\", \"text/html\").body(\"<div id=\\\"app\\\"></div>\").build())\n\
         \t\t.on_start(|server| {announce})\n\
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

/// Spawn the built server from the project root, with `env` applied — which is
/// how the dev policy's two modes are told apart — and wait for it to announce
/// the port it bound.
///
/// Returning a *listening* server with a *reported* port is what retires this
/// suite's `free_port` + `wait_for_port` pair (N40): there is no number to guess
/// and nothing to wait for that the announcement has not already proven.
fn serve(staged: &Path, env: &[(&str, &str)]) -> support::port::Server {
    let mut command = Command::new("node");
    command
        .arg("dist/server.mjs")
        .current_dir(staged)
        .stderr(Stdio::null());
    for (name, value) in env {
        command.env(name, value);
    }
    support::port::Server::spawn(&mut command)
}

/// [`http_get_raw`] carrying `headers` verbatim on the wire — each line must
/// be `\r\n`-terminated. For the conditional requests the caching pin makes.
fn http_get_with(port: u16, path: &str, headers: &str) -> (String, Vec<u8>) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect for GET");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set a read timeout");
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: localhost\r\n{headers}Connection: close\r\n\r\n"
    )
    .expect("send GET");
    let mut response = Vec::new();
    let _ = stream.read_to_end(&mut response);
    let separator = b"\r\n\r\n";
    match response
        .windows(separator.len())
        .position(|window| window == separator)
    {
        Some(at) => (
            String::from_utf8_lossy(&response[..at]).into_owned(),
            response[at + separator.len()..].to_vec(),
        ),
        None => (String::from_utf8_lossy(&response).into_owned(), Vec::new()),
    }
}

/// The value of `name` in a response head, case-insensitively.
fn header_value(head: &str, name: &str) -> Option<String> {
    let wanted = name.to_ascii_lowercase();
    head.lines().skip(1).find_map(|line| {
        let (candidate, value) = line.split_once(':')?;
        (candidate.trim().to_ascii_lowercase() == wanted).then(|| value.trim().to_string())
    })
}

/// A plain HTTP GET, returning `(status line + headers, RAW body bytes)`.
///
/// The body is never decoded. That matters here and nowhere else in this file:
/// the assertions about a `.png` are about the exact bytes on the wire, and a
/// `String::from_utf8_lossy` would replace every byte this pipeline used to
/// destroy with the same U+FFFD, hiding the defect it is asked to prove gone.
fn http_get_raw(port: u16, path: &str) -> (String, Vec<u8>) {
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
    // The header block is ASCII, so it can be found by bytes without decoding
    // the body that follows it.
    let separator = b"\r\n\r\n";
    match response
        .windows(separator.len())
        .position(|window| window == separator)
    {
        Some(at) => (
            String::from_utf8_lossy(&response[..at]).into_owned(),
            response[at + separator.len()..].to_vec(),
        ),
        None => (String::from_utf8_lossy(&response).into_owned(), Vec::new()),
    }
}

/// The same GET with the body as text, for the artifacts that are text.
fn http_get(port: u16, path: &str) -> (String, String) {
    let (head, body) = http_get_raw(port, path);
    (head, String::from_utf8_lossy(&body).into_owned())
}

/// The headers the APPLICATION set, in wire order: the response head minus the
/// status line and minus the ones node writes for every response whatever the
/// handler did. What remains is exactly what `serve_build` chose to send, which
/// is the only way to assert that it chose to send *nothing else*.
fn response_headers(head: &str) -> Vec<String> {
    const NODES_OWN: [&str; 5] = [
        "date",
        "connection",
        "content-length",
        "transfer-encoding",
        "keep-alive",
    ];
    head.lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .filter(|line| {
            let name = line
                .split(':')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            !NODES_OWN.contains(&name.as_str())
        })
        .map(|line| line.trim().to_string())
        .collect()
}

#[test]
fn serve_build_answers_every_artifact_and_leaves_the_rest_to_the_app() {
    let staged = stage("routes", Client::Styled, false);
    build(&staged);
    let mut server = serve(&staged, &[]);
    let port = server.port();

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
    // The DEFAULT is untouched by the opt-in caching hook (kolt.local 025b):
    // a server that never called `cache_build` sends exactly the headers it
    // sent before the hook existed. Asserted as a header COUNT and not only as
    // two absences, so a policy leaking into the default path is caught
    // whatever header it leaks — the `fullstack-dx.md` §5.10 fence is about
    // defaults, and this is the pin on it.
    assert_eq!(
        response_headers(&head),
        vec!["Content-Type: text/javascript; charset=utf-8".to_string()],
        "no policy on the chain means no policy headers on the wire:\n{head}"
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

    server.stop();
    let _ = std::fs::remove_dir_all(&staged);
}

/// kolt.local 025b, the opted-in half: one `.cache_build(…)` on the chain and
/// the served artifacts answer conditional requests and carry the policy the
/// route asked for — the two-tier shape the motivating exhibit hand-rolled and
/// then surrendered on. Pinned end to end over a real build, because every
/// claim is about the wire: which status each arm answers with, which headers
/// ride on each, and whether a 304 leaks a body.
///
/// The default half of the same ruling is pinned in
/// `serve_build_answers_every_artifact_and_leaves_the_rest_to_the_app`: with no
/// policy on the chain, `Content-Type` is the only header that goes out.
#[test]
fn an_opted_in_serve_build_revalidates_and_carries_its_per_route_cache_control() {
    let staged = stage_serving("cached", Client::Styled, false, cached_server_source());
    build(&staged);
    let mut server = serve(&staged, &[]);
    let port = server.port();

    // The validating tier: a 200 with the validator, the content type, and the
    // route's own Cache-Control.
    let (head, body) = http_get_with(port, "/client.js", "");
    assert!(
        head.starts_with("HTTP/1.1 200"),
        "an unconditional GET is a 200:\n{head}"
    );
    let etag = header_value(&head, "ETag").unwrap_or_else(|| {
        panic!("the validating tier mints an ETag over the artifact's bytes:\n{head}")
    });
    assert!(
        etag.starts_with('"') && etag.ends_with('"') && etag.len() == 34,
        "the validator is `etag_of`'s quoted 32-hex digest, not something re-invented here: {etag}"
    );
    assert_eq!(
        header_value(&head, "Cache-Control").as_deref(),
        Some("no-cache"),
        "the route's policy header reaches the 200:\n{head}"
    );
    assert_eq!(
        header_value(&head, "Content-Type").as_deref(),
        Some("text/javascript; charset=utf-8"),
        "and the extension still types the body:\n{head}"
    );
    assert_eq!(
        String::from_utf8_lossy(&body),
        std::fs::read_to_string(staged.join("dist/client.js")).expect("the bundle"),
        "the opted-in 200 serves the same bytes the default one did"
    );

    // The revalidation: the client comes back with what it was given.
    let (head, body) = http_get_with(port, "/client.js", &format!("If-None-Match: {etag}\r\n"));
    assert!(
        head.starts_with("HTTP/1.1 304"),
        "a matching If-None-Match answers 304:\n{head}"
    );
    assert!(body.is_empty(), "a 304 carries no body");
    assert_eq!(
        header_value(&head, "ETag").as_deref(),
        Some(etag.as_str()),
        "the 304 echoes the validator, so the NEXT conditional matches too:\n{head}"
    );
    assert_eq!(
        header_value(&head, "Cache-Control").as_deref(),
        Some("no-cache"),
        "and the policy rides the 304 arm as well — RFC 9110 §15.4.5, and the \
         whole reason `etag_response` hands back an open builder:\n{head}"
    );
    assert_eq!(
        header_value(&head, "Content-Type"),
        None,
        "a 304 has no content to type:\n{head}"
    );

    // A stale validator gets the bytes back.
    let (head, body) = http_get_with(port, "/client.js", "If-None-Match: \"0000\"\r\n");
    assert!(
        head.starts_with("HTTP/1.1 200") && !body.is_empty(),
        "a stale validator gets the fresh representation:\n{head}"
    );

    // The OTHER tier, per route: fingerprinted-style caching, no validator.
    // This is the pin that the policy is asked per artifact and not once.
    let (head, _) = http_get_with(port, "/client.css", "");
    assert_eq!(
        header_value(&head, "Cache-Control").as_deref(),
        Some("public, max-age=31536000, immutable"),
        "the second tier gets its own header, keyed on the route:\n{head}"
    );
    assert_eq!(
        header_value(&head, "ETag"),
        None,
        "`CachePolicy::none()` mints no validator, so this tier spends no digest:\n{head}"
    );
    assert_eq!(
        header_value(&head, "Content-Type").as_deref(),
        Some("text/css; charset=utf-8"),
        "and the un-validated arm still types the body from the extension:\n{head}"
    );

    // A path the build does not claim is still the app's, policy or no policy.
    let (head, body) = http_get_with(port, "/some/deep/link", "");
    assert!(
        header_value(&head, "Cache-Control").is_none()
            && String::from_utf8_lossy(&body).contains("id=\"app\""),
        "the hook covers the build's routes and nothing else:\n{head}"
    );

    server.stop();
    let _ = std::fs::remove_dir_all(&staged);
}

#[test]
fn a_leg_that_gains_split_serves_its_chunks_with_no_server_edit() {
    // The S3 gate, and what `bundle-splitting.md` §3 wanted the sidecar for in
    // the first place. The ONLY edit between the two halves of this test is
    // `split = true` in the manifest; the server file is asserted byte-identical
    // across it, so the routes can only have come from the build.
    let staged = stage("split", Client::Router, false);
    let server_before = std::fs::read_to_string(staged.join("src/server.vl")).expect("the server");

    build(&staged);
    let mut server = serve(&staged, &[]);
    let port = server.port();
    let (_, body) = http_get(port, "/client.Route_Home.js");
    assert!(
        body.contains("id=\"app\""),
        "with no split there is no chunk to serve, so the app's handler answers"
    );
    server.stop();

    // The one edit.
    write_manifest(&staged, true);
    build(&staged);
    assert_eq!(
        std::fs::read_to_string(staged.join("src/server.vl")).expect("the server"),
        server_before,
        "the server file must not move — that is the whole claim"
    );

    // A restart is a fresh bind, so the port is asked for again — the server
    // reports whatever the OS gave it this time.
    let mut server = serve(&staged, &[]);
    let port = server.port();
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
    server.stop();
    let _ = std::fs::remove_dir_all(&staged);
}

#[test]
fn an_artifact_the_build_named_and_did_not_write_stops_the_server() {
    // §5.4: a missing artifact is a broken BUILD, and it is loud at boot rather
    // than a 404 per request for the life of the process. The manifest still
    // names `client.css`, so removing the file is exactly the "the build said
    // it wrote this" case.
    let staged = stage("missing", Client::Styled, false);
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
fn an_artifact_with_an_unknown_extension_is_skipped_with_a_warning_not_silently() {
    // kolt.local 022(b): the §5.10 fence stands — `serve_build` serves a
    // build, not a directory, and an extension the content-type table does not
    // name is not served — but the drop is no longer silent: boot names the
    // artifact it skipped.
    //
    // The fixture is a `.zip`, and it used to be a `.png`. That swap IS the
    // fence being re-drawn rather than removed (kolt.local 022): the table is
    // generated from `mime-db` now and knows every kind a build emits, so a
    // `.png` is served and would prove nothing here. An archive is still
    // outside the fence — no build emits one, and no page loads one as a
    // sub-resource — so it is what an unnamed extension looks like today.
    let staged = stage("unknown_ext", Client::Styled, false);
    build(&staged);
    // Teach the manifest an artifact the table does not know — a `.zip` chunk
    // entry — with the file ON DISK, so what this test observes is the
    // extension skip and not the missing-artifact stop.
    let manifest_path = staged.join("dist/client.chunks.json");
    let manifest = std::fs::read_to_string(&manifest_path).expect("the manifest");
    assert!(
        manifest.contains("\"chunks\": []"),
        "the fixture's manifest shape moved under this test: {manifest}"
    );
    std::fs::write(
        &manifest_path,
        manifest.replace("\"chunks\": []", "\"chunks\": [{\"file\": \"bundle.zip\"}]"),
    )
    .expect("name the unknown-extension artifact");
    std::fs::write(staged.join("dist/bundle.zip"), "zip-bytes").expect("write the artifact");

    let mut server = serve(&staged, &[]);
    let port = server.port();

    // The unknown extension is still not served: the app's fallback answers.
    let (_, body) = http_get(port, "/bundle.zip");
    assert!(
        body.contains("id=\"app\""),
        "an unknown-extension artifact must fall through to the app, not be served:\n{body}"
    );
    // And the known artifacts still are — the asset list is unchanged.
    let (head, _) = http_get(port, "/client.js");
    assert!(
        head.contains("Content-Type: text/javascript"),
        "the bundle's route must be untouched by the skipped artifact:\n{head}"
    );
    let (head, _) = http_get(port, "/client.css");
    assert!(
        head.contains("Content-Type: text/css"),
        "the sidecar's route must be untouched by the skipped artifact:\n{head}"
    );

    server.stop();
    let boot_output = server.stdout();
    assert!(
        boot_output.contains(
            "warning: the `client` build names dist/bundle.zip, whose extension \
             `serve_build` has no content type for — the artifact is not served"
        ),
        "the boot must name the artifact it skipped:\n{boot_output}"
    );
    let _ = std::fs::remove_dir_all(&staged);
}

/// A real 1x1 PNG — 70 bytes, and not valid UTF-8 at byte 0 (`0x89`).
///
/// The number is the point. Decoded as text and re-encoded, these 70 bytes
/// become 94: every byte that is not a legal UTF-8 sequence is replaced by
/// U+FFFD, which is three bytes on the way back out. That is exactly how kolt
/// measured this defect — a 483-byte favicon served as 853 bytes — at a size a
/// test can assert (kolt.local 030).
const ONE_PIXEL_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xfc, 0xcf, 0xc0, 0x50,
    0x0f, 0x00, 0x04, 0x85, 0x01, 0x80, 0x84, 0xa9, 0x8c, 0x21, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// A `wOF2`-signed blob carrying every one of the 256 byte values.
///
/// Not a loadable font, and it does not need to be: what is under test is the
/// pipeline's fidelity, and a body containing all 256 values is the strongest
/// statement of it — there is no byte left for the pipeline to get wrong.
fn every_byte_woff2() -> Vec<u8> {
    let mut bytes = b"wOF2".to_vec();
    bytes.extend((0..=255u8).rev());
    bytes
}

/// Plant `files` into the build manifest's chunk list and onto disk, so
/// `load_build` reads them as artifacts of the build.
fn plant_artifacts(staged: &Path, files: &[(&str, Vec<u8>)]) {
    let manifest_path = staged.join("dist/client.chunks.json");
    let manifest = std::fs::read_to_string(&manifest_path).expect("the manifest");
    assert!(
        manifest.contains("\"chunks\": []"),
        "the fixture's manifest shape moved under this test: {manifest}"
    );
    let chunks = files
        .iter()
        .map(|(name, _)| format!("{{\"file\": \"{name}\"}}"))
        .collect::<Vec<_>>()
        .join(", ");
    std::fs::write(
        &manifest_path,
        manifest.replace("\"chunks\": []", &format!("\"chunks\": [{chunks}]")),
    )
    .expect("name the planted artifacts");
    for (name, bytes) in files {
        std::fs::write(staged.join("dist").join(name), bytes).expect("write the artifact");
    }
}

#[test]
fn a_binary_artifact_reaches_the_wire_as_the_build_wrote_it() {
    // kolt.local 030. Until this landed, `BuildAsset.content` was a `str` and
    // every artifact was UTF-8-decoded on the way in, so `serve_build` could
    // not carry a byte-typed artifact at any point in its chain — which is why
    // kolt had to hand-roll a static layer to serve its own favicon, and why
    // a complete content-type table could not be exercised.
    let staged = stage("binary", Client::Styled, false);
    build(&staged);
    let woff2 = every_byte_woff2();
    plant_artifacts(
        &staged,
        &[
            ("favicon.png", ONE_PIXEL_PNG.to_vec()),
            ("brand.woff2", woff2.clone()),
            ("LOGO.PNG", ONE_PIXEL_PNG.to_vec()),
        ],
    );

    let mut server = serve(&staged, &[]);
    let port = server.port();

    let (head, body) = http_get_raw(port, "/favicon.png");
    assert!(
        head.contains("Content-Type: image/png"),
        "a `.png` artifact must be typed by the table, not skipped:\n{head}"
    );
    assert_eq!(
        body, ONE_PIXEL_PNG,
        "the favicon must reach the wire byte for byte"
    );
    // Said as a size too, because the size is how this defect was found: a
    // decode-and-re-encode of these bytes is 94 long, not 70.
    assert_eq!(
        body.len(),
        70,
        "the favicon grew or shrank in transit — a lossy decode inflates these \
         70 bytes to 94 (kolt measured 483 -> 853 on its own)"
    );

    let (head, body) = http_get_raw(port, "/brand.woff2");
    assert!(
        head.contains("Content-Type: font/woff2"),
        "a `.woff2` artifact must be typed by the table:\n{head}"
    );
    assert_eq!(
        body, woff2,
        "a font carrying all 256 byte values must survive the server unchanged"
    );

    // A font and an image are typed WITHOUT a charset: a charset parameter on a
    // binary type is meaningless, and on `application/json` it is an error.
    assert!(
        !head.contains("charset"),
        "a binary artifact must not be served with a charset:\n{head}"
    );

    // The extension is matched case-insensitively: `LOGO.PNG` is one file to
    // the build that wrote it, so it is one row to the table that types it.
    let (head, body) = http_get_raw(port, "/LOGO.PNG");
    assert!(
        head.contains("Content-Type: image/png"),
        "an uppercase extension must reach the same row as its lowercase twin:\n{head}"
    );
    assert_eq!(body, ONE_PIXEL_PNG, "and be served byte for byte too");

    // And the artifacts that were always served are still exactly themselves.
    let (_, body) = http_get_raw(port, "/client.js");
    let on_disk = std::fs::read(staged.join("dist/client.js")).expect("the bundle");
    assert_eq!(body, on_disk, "the bundle must be byte-identical too");

    server.stop();
    let _ = std::fs::remove_dir_all(&staged);
}

#[test]
fn a_text_artifact_spells_its_charset_and_a_json_one_does_not() {
    // kolt.local 022. The body goes out as raw bytes now, so nothing downstream
    // implies an encoding: an unspelled `text/css` is decoded by whatever the
    // browser defaults to. `application/json` is the converse — utf8 BY SPEC,
    // where naming a charset is the error rather than the fix — and
    // `.webmanifest` needs its own media type because Chrome rejects a manifest
    // served as anything else.
    let staged = stage("charset", Client::Styled, false);
    build(&staged);
    plant_artifacts(
        &staged,
        &[
            ("data.json", b"{\"ok\":true}".to_vec()),
            ("site.webmanifest", b"{\"name\":\"served\"}".to_vec()),
        ],
    );

    let mut server = serve(&staged, &[]);
    let port = server.port();

    for (path, expected) in [
        ("/client.js", "Content-Type: text/javascript; charset=utf-8"),
        ("/client.css", "Content-Type: text/css; charset=utf-8"),
    ] {
        let (head, _) = http_get(port, path);
        assert!(
            head.contains(expected),
            "{path} must be served `{expected}` — a raw byte body carries no \
             encoding of its own:\n{head}"
        );
    }

    let (head, body) = http_get(port, "/data.json");
    assert!(
        head.contains("Content-Type: application/json"),
        "a `.json` artifact must be served as json:\n{head}"
    );
    assert!(
        !head.contains("charset"),
        "json is utf8 by spec and takes no charset parameter:\n{head}"
    );
    assert_eq!(body, "{\"ok\":true}");

    let (head, _) = http_get(port, "/site.webmanifest");
    assert!(
        head.contains("Content-Type: application/manifest+json"),
        "Chrome rejects a manifest served as anything but \
         `application/manifest+json`:\n{head}"
    );

    server.stop();
    let _ = std::fs::remove_dir_all(&staged);
}

#[test]
fn the_dev_policy_revalidates_only_while_watching() {
    // `dev-refresh.md` §5, item 1 — E55's headline defect, at the one call site
    // that can close it. A server holds its assets in a closure for the life of
    // the process, so bytes that move on disk under a running server were served
    // stale forever. `serve_build` re-reads per request while `run --watch` owns
    // the process, and serves the boot-time copy otherwise.
    let staged = stage("fresh", Client::Styled, false);
    build(&staged);
    let bundle = staged.join("dist/client.js");
    let original = std::fs::read_to_string(&bundle).expect("the bundle");

    // Release: the boot-time copy, whatever happens on disk afterwards.
    let mut server = serve(&staged, &[]);
    let port = server.port();
    let (_, before) = http_get(port, "/client.js");
    assert_eq!(before, original);
    std::fs::write(&bundle, "// MOVED\n").expect("move the bytes");
    let (_, after) = http_get(port, "/client.js");
    assert_eq!(
        after, original,
        "outside a watch the server serves what it read at boot — no syscall per request"
    );
    server.stop();
    std::fs::write(&bundle, &original).expect("restore the bundle");

    // Watching: fresh, per request, with no restart and no signalling protocol.
    let mut server = serve(&staged, &[("VILAN_WATCHING", "1")]);
    let port = server.port();
    let (_, before) = http_get(port, "/client.js");
    assert_eq!(before, original);
    std::fs::write(&bundle, "// MOVED\n").expect("move the bytes");
    let (_, after) = http_get(port, "/client.js");
    assert_eq!(
        after, "// MOVED\n",
        "under `run --watch` every request is an opportunity to be fresh"
    );
    server.stop();
    let _ = std::fs::remove_dir_all(&staged);
}

#[test]
fn is_watching_is_false_outside_a_watch() {
    // Uniform (`dev-refresh.md` §5's scope ruling): DEFINED under every run,
    // `true` only under one — so a program branches on it without knowing how
    // it was started.
    // The probe IS the server here — it never binds, so it is staged directly
    // rather than staged as a server and then overwritten.
    let staged = stage_serving(
        "plainrun",
        Client::Styled,
        false,
        "import std::io::print;\nimport std::watch::is_watching;\n\nfun main() {\n\tprint(i\"watching={is_watching()}\");\n}\n".to_string(),
    );
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
    // The probe stays alive only long enough for the harness to read its
    // marker, then SELF-EXPIRES: kill_watcher cannot reap the watcher's node
    // grandchild (the E60 mechanism), so an unbounded sleep here leaked one
    // process per run. Fifteen seconds is orders beyond the marker's
    // boot-time print under any load, and an orphan now dies on its own —
    // the watcher's restart loop may respawn it once inside the kill window,
    // and that respawn self-expires the same way.
    //
    // It never binds a port, so — like `plainrun` — it is staged directly.
    let staged = stage_serving(
        "watchrun",
        Client::Styled,
        false,
        "import std::io::print;\nimport std::watch::is_watching;\nimport std::time::sleep;\n\n\
         async fun main() {\n\tprint(i\"watching={is_watching()}\");\n\tsleep(15000);\n}\n"
            .to_string(),
    );

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
