//! End-to-end tests for the A13 dev channel and its full-stack coordination
//! (hmr.md slices S1 and S3).
//!
//! `the_dev_channel_drives_the_watch_round` (S1): `run --watch` on a workspace
//! with a browser leg stands up an SSE dev channel, and each watch round pushes
//! the byte-diff verdict to connected browsers — `swap` on a code change, `css`
//! on a stylesheet-only change, `error` on a compile failure (with the next good
//! round clearing it) — while the artifact routes serve the shim-instrumented
//! bundle and the CSS sidecar.
//!
//! `a_server_edit_restarts_quietly_and_a_shared_edit_swaps` (S3): the two rows of
//! the §6 coordination matrix the S1 test doesn't reach — a **server-only** edit
//! restarts the Node child (witnessed by its per-source boot marker on the
//! watcher's captured stdout) while pushing *nothing* to the browser, and a
//! **shared** edit (a `common` module both legs embed) both restarts the server
//! and pushes a `swap`.
//!
//! House process hygiene (the watcher never exits on its own): the legs are
//! quick-exit (the node server prints and returns), so killing the watcher at
//! the end orphans nothing.
//!
//! Every `deadline` here is `support::WATCH_LIVENESS` — a liveness bound, not a
//! performance assertion. It was a literal 20 s, which is a *compile* budget on
//! a contended box, and that is what failed `hmr_swap` under a loaded suite
//! (E39). Nothing in this file asserts how fast a round is, so the bound only
//! has to be finite. The per-test margins (`sleep(500/800 ms)`) and the
//! negative windows were the separate shape §6 recorded and left standing;
//! E41 is that pass. Every negative assertion here is now anchored BETWEEN two
//! positive events — `assert_none_before` closes a quiet SSE window on an event
//! the next round is guaranteed to push, and the `PROBE_RAN` checks are instant
//! scans placed after a strictly later round's evidence. A fixed window has the
//! wrong sense for a negative: it passes as soon as it stops reading, so a slow
//! box makes it prove *less*, and the quiet windows went vacuously green exactly
//! under the contention that made a spurious push likeliest. Event-anchored, a
//! slow box only lengthens the window, which can add evidence but never remove
//! it. The `sleep(500/800 ms)` margins are gone for the same reason E39 removed
//! its own: they paid for the baseline-snapshot race E20 fixed at its root
//! (the watcher snapshots BEFORE the first action, so an edit landing during
//! the initial build triggers a round rather than being swallowed).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{ChildStderr, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

mod support;

fn temp_project(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "vilan_hmr_cli_{tag}_{}_{unique}",
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

/// A browser client that emits one CSS line from a `const` initializer. The
/// initializer always returns `1`, so changing only `css_marker` leaves the JS
/// bundle byte-identical (a clean CSS-only round); changing `code_marker`
/// changes the bundle (a swap round).
fn client_source(code_marker: &str, css_marker: &str) -> String {
    format!(
        "import std::io::print;\nimport std::asset::emit;\n\nfun styles(): i32 {{\n\temit(\"css\", \".{css_marker}{{color:red}}\");\n\t1\n}}\n\nlet _s = const styles();\n\nfun main() {{\n\tprint(\"{code_marker}\");\n}}\n"
    )
}

/// Extracts the dev-channel port from the activation line
/// `hmr: dev channel on 127.0.0.1:<port>`.
fn parse_port(line: &str) -> Option<u16> {
    let rest = line.strip_prefix("hmr: dev channel on 127.0.0.1:")?;
    rest.trim().parse().ok()
}

/// A raw SSE client over a `TcpStream`, accumulating bytes and yielding one
/// event `kind` at a time (skipping the whitespace of the HTTP head and the
/// `data:`/blank-line framing).
struct SseClient {
    stream: TcpStream,
    buffer: Vec<u8>,
    cursor: usize,
}

impl SseClient {
    fn connect(port: u16, token: &str) -> SseClient {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect to dev channel");
        write!(
            stream,
            "GET /events?token={token} HTTP/1.1\r\nHost: localhost\r\n\r\n"
        )
        .expect("send SSE request");
        stream
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        SseClient {
            stream,
            buffer: Vec::new(),
            cursor: 0,
        }
    }

    /// The raw JSON payload of the next `data: {json}` frame, or `None` at the
    /// deadline.
    fn next_payload(&mut self, deadline: Duration) -> Option<String> {
        let start = Instant::now();
        loop {
            // Consume any complete line already buffered.
            while let Some(newline) = self.buffer[self.cursor..]
                .iter()
                .position(|&byte| byte == b'\n')
            {
                let line_end = self.cursor + newline;
                let line =
                    String::from_utf8_lossy(&self.buffer[self.cursor..line_end]).into_owned();
                self.cursor = line_end + 1;
                if let Some(payload) = line.trim_end().strip_prefix("data: ") {
                    return Some(payload.to_string());
                }
            }
            if start.elapsed() >= deadline {
                return None;
            }
            let mut chunk = [0u8; 1024];
            match self.stream.read(&mut chunk) {
                Ok(0) => return None,
                Ok(read) => self.buffer.extend_from_slice(&chunk[..read]),
                // A read timeout is expected between rounds — keep waiting.
                Err(_) => {}
            }
        }
    }

    /// The `kind` of the next `data: {json}` frame, or `None` at the deadline.
    fn next_kind(&mut self, deadline: Duration) -> Option<String> {
        self.next_payload(deadline)
            .and_then(|payload| kind_of(&payload))
    }

    /// Reads events until one matches `expected` (ignoring others, e.g. the
    /// connect-time `connected`), or fails at the deadline.
    fn expect_kind(&mut self, expected: &str, deadline: Duration) {
        self.expect_event(expected, deadline);
    }

    /// Like [`SseClient::expect_kind`], but returns the matching event's raw JSON
    /// payload — so a test can inspect its `message` (the error overlay's text)
    /// or `asset` (the css sidecar's name).
    fn expect_event(&mut self, expected: &str, deadline: Duration) -> String {
        let start = Instant::now();
        while start.elapsed() < deadline {
            match self.next_payload(deadline - start.elapsed()) {
                Some(payload) if kind_of(&payload).as_deref() == Some(expected) => return payload,
                Some(_other) => continue,
                None => break,
            }
        }
        panic!("did not observe a `{expected}` event within the deadline");
    }

    /// Asserts that none of the `forbidden` kinds arrive before `closing` — an
    /// event the NEXT round is guaranteed to push. Other kinds (the connect-time
    /// `connected`) are ignored; this is the server-only round's "the browser is
    /// told nothing" assertion.
    ///
    /// The window is anchored at BOTH ends by events: it opens at whatever
    /// positive event the caller just observed (the restarted child's boot
    /// marker) and closes at `closing`. SSE is an ordered stream, so anything
    /// the quiet round pushed must arrive *before* `closing` — a slow box only
    /// lengthens the window, which can add evidence but never remove it.
    ///
    /// That is the E41 fix. The fixed-duration `assert_no(2000ms)` this replaces
    /// had the opposite sense: it passed as soon as a read timed out, so the
    /// slower the box, the less of the stream it actually read, and the window
    /// went *vacuously* green exactly when contention made it most likely to
    /// matter. `closing` never arriving is now a failure, not a pass.
    fn assert_none_before(&mut self, forbidden: &[&str], closing: &str, deadline: Duration) {
        let start = Instant::now();
        while start.elapsed() < deadline {
            match self.next_kind(deadline - start.elapsed()) {
                Some(kind) if kind == closing => return,
                Some(kind) => assert!(
                    !forbidden.contains(&kind.as_str()),
                    "a `{kind}` event was pushed before the closing `{closing}` \
                     (the preceding round must be silent)"
                ),
                None => break,
            }
        }
        panic!(
            "the closing `{closing}` event never arrived — the quiet window never \
             closed, so nothing was proven about it"
        );
    }

    /// Asserts nothing more is waiting on the stream.
    ///
    /// Call this only AFTER a positive event has proven the latest round's push
    /// was issued and flushed (its restarted child printed, or its bundle is
    /// being served — both strictly follow the push). At that point a pending
    /// event is a SECOND push, which is the one case
    /// [`SseClient::assert_none_before`] cannot see on its own: when the quiet
    /// round and the closing round would push the same `kind`, the closing read
    /// consumes the spurious one and the real one is left here.
    fn assert_nothing_pending(&mut self, forbidden: &[&str]) {
        while let Some(kind) = self.next_kind(Duration::from_millis(250)) {
            assert!(
                !forbidden.contains(&kind.as_str()),
                "a second `{kind}` event was pushed — the quiet round was not silent"
            );
        }
    }
}

/// The `"kind"` field of a tiny event JSON body, by hand (no JSON crate).
fn kind_of(json: &str) -> Option<String> {
    let after = json.split("\"kind\":\"").nth(1)?;
    Some(after.split('"').next()?.to_string())
}

/// This run's dev-channel token (backlog E93), read from the instrumented
/// bundle in `dist/` — the SAME copy the browser gets, and the only place the
/// CLI writes it besides the Node child's environment. Deliberately not printed
/// by the CLI and deliberately not passed to these tests any other way: reading
/// it here is the real delivery path, so a shim that stopped carrying it would
/// take every dev-channel test in this file down with it.
///
/// Bounded by `deadline` because round 1 is what writes `dist/`.
fn dev_token(dir: &Path, leg: &str, deadline: Duration) -> String {
    let bundle = dir.join("dist").join(format!("{leg}.js"));
    let start = Instant::now();
    loop {
        if let Ok(text) = std::fs::read_to_string(&bundle)
            && let Some(after) = text.split("var TOKEN = \"").nth(1)
            && let Some(token) = after.split('"').next()
            && !token.is_empty()
            && token != "__VILAN_HMR_TOKEN__"
        {
            return token.to_string();
        }
        assert!(
            start.elapsed() < deadline,
            "the instrumented bundle {} should carry this run's token within {deadline:?}",
            bundle.display()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A GET against the dev channel, presenting this run's token as every route
/// requires (backlog E93). The app's OWN server is reached with [`http_get`]
/// instead — it has no token and wants none.
fn dev_get(port: u16, path: &str, token: &str) -> Vec<u8> {
    http_get(port, &format!("{path}?token={token}"))
}

/// One request against the dev channel, returning its STATUS LINE and headers
/// as text. A refusal has no body to inspect, so the status is the assertion —
/// and reading a `403` here is what proves the gate answered rather than the
/// route.
///
/// `/events` is included among the routes checked this way, which is why this
/// reads only the head and then drops the socket: a *successful* SSE request
/// would hold the connection open forever, so `read_to_end` would be a hang
/// rather than a failure if the gate ever stopped refusing.
fn http_request_head(port: u16, method: &str, target: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect for a head request");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(
        stream,
        "{method} {target} HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n"
    )
    .expect("send the request");
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while head.len() < 4096 {
        match stream.read(&mut byte) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                head.push(byte[0]);
                if head.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
        }
    }
    String::from_utf8_lossy(&head).into_owned()
}

/// [`http_request_head`] for a GET.
fn http_get_head(port: u16, target: &str) -> String {
    http_request_head(port, "GET", target)
}

/// A plain (non-SSE) HTTP GET, returning the response body as bytes (the
/// connection closes after the response). Used verbatim against the app's own
/// server, and via [`dev_get`] against the dev channel.
fn http_get(port: u16, path: &str) -> Vec<u8> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect for GET");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(stream, "GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").expect("send GET");
    let mut response = Vec::new();
    let _ = stream.read_to_end(&mut response);
    // Split off the body after the header terminator.
    let separator = b"\r\n\r\n";
    match response
        .windows(separator.len())
        .position(|window| window == separator)
    {
        Some(index) => response[index + separator.len()..].to_vec(),
        None => response,
    }
}

/// Waits (bounded) for `path` to exist — round 1 has written `dist/`.
fn wait_for_file(path: &Path, deadline: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < deadline {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// Drains one child stream on a thread, forwarding every line into `sender`.
fn drain_into(stream: impl Read + Send + 'static, sender: mpsc::Sender<String>) {
    std::thread::spawn(move || {
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            let _ = sender.send(line);
        }
    });
}

/// Drains the watcher's stdout on a thread, forwarding every line to a channel.
/// The Node server's `print` output flows here too — `spawn_node` gives the child
/// no stdio of its own, so it inherits the watcher's stdout (the piped fd) — which
/// is how the coordination-matrix test witnesses a server restart: a per-source
/// boot marker printed by the freshly spawned child.
fn drain_stdout(stdout: ChildStdout) -> mpsc::Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    drain_into(stdout, sender);
    receiver
}

/// Both streams into one channel — for a test that watches stdout markers *and*
/// a diagnostic, which since `windows-support.md` §6 renders to stderr.
fn drain_both(stdout: ChildStdout, stderr: ChildStderr) -> mpsc::Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    drain_into(stdout, sender.clone());
    drain_into(stderr, sender);
    receiver
}

/// Waits (bounded) for the activation line and returns its announced port.
fn wait_for_port(lines: &mpsc::Receiver<String>, deadline: Duration) -> Option<u16> {
    let start = Instant::now();
    while start.elapsed() < deadline {
        match lines.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                if let Some(port) = parse_port(&line) {
                    return Some(port);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => return None,
        }
    }
    None
}

/// Waits (bounded) for a stdout line containing `needle` (a server boot marker).
fn wait_for_line(lines: &mpsc::Receiver<String>, needle: &str, deadline: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < deadline {
        match lines.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                if line.contains(needle) {
                    return true;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => return false,
        }
    }
    false
}

#[test]
fn the_dev_channel_drives_the_watch_round() {
    let dir = temp_project("channel");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    write(&dir, "src/client.vl", &client_source("a", "x1"));
    write(
        &dir,
        "src/server.vl",
        "import std::io::print;\n\nfun main() {\n\tprint(\"server\");\n}\n",
    );

    // `--hmr-port 0` asks for an ephemeral port; the CLI announces the bound one.
    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", "--watch", "--hmr-port", "0", dir.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn run --watch");

    // Drain BOTH streams on threads (so neither pipe fills), forwarding every
    // line: the boot markers arrive on stdout, the ariadne diagnostic on stderr.
    let lines = drain_both(
        watcher.stdout.take().unwrap(),
        watcher.stderr.take().unwrap(),
    );

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let deadline = support::WATCH_LIVENESS;
        let port = wait_for_port(&lines, deadline)
            .expect("the CLI should announce `hmr: dev channel on 127.0.0.1:<port>`");

        // Round 1 has run once `dist/client.css` lands. The margin that used to
        // follow paid for the baseline-snapshot race E20 fixed at its root (the
        // watcher snapshots before the first action), so it is gone.
        assert!(
            wait_for_file(&dir.join("dist/client.css"), deadline),
            "round 1 should have written dist/client.css"
        );

        let token = dev_token(&dir, "client", deadline);
        let mut sse = SseClient::connect(port, &token);

        // (a) A code change → `swap`.
        write(&dir, "src/client.vl", &client_source("b", "x1"));
        sse.expect_kind("swap", deadline);

        // (b) A stylesheet-only change (bundle byte-identical) → `css`, and the
        // event names its sidecar so the shim bumps only that stylesheet <link>.
        write(&dir, "src/client.vl", &client_source("b", "x2"));
        let css_event = sse.expect_event("css", deadline);
        assert!(
            css_event.contains("\"asset\":\"client.css\""),
            "the css event should name its changed sidecar: {css_event}"
        );

        // (c) A syntax error → `error` carrying the REAL compiler diagnostics
        // (the S1 residue closed): the message names the failing file and the
        // actual parse error — not the old generic "build failed" string.
        write(&dir, "src/client.vl", "fun main( {\n");
        let error_event = sse.expect_event("error", deadline);
        assert!(
            error_event.contains("client.vl"),
            "the error event should name the failing file: {error_event}"
        );
        assert!(
            error_event.contains("expected"),
            "the error event should carry the real parse diagnostic, \
             not the generic fallback: {error_event}"
        );
        assert!(
            !error_event.contains("build failed; see the terminal"),
            "the generic fallback string must be gone now that real text is threaded: {error_event}"
        );
        // Terminal-unchanged A/B: the SAME diagnostic is still rendered to the
        // watcher's terminal (ariadne, on stderr since windows-support.md §6),
        // in the same round — the overlay capture is additive (a second sink),
        // never a redirect.
        assert!(
            wait_for_line(&lines, "expected", deadline),
            "the terminal must still print the diagnostic (the overlay capture is additive)"
        );
        // A fix → the next good round (which clears the overlay browser-side).
        write(&dir, "src/client.vl", &client_source("c", "x2"));
        sse.expect_kind("swap", deadline);

        // (d) The artifact routes: the browser bundle carries the shim (the
        // singleton marker), and the sidecar serves the current CSS.
        let bundle =
            String::from_utf8_lossy(&dev_get(port, "/bundle/client.js", &token)).into_owned();
        assert!(
            bundle.contains("window.__VILAN_HMR__"),
            "the served bundle should carry the dev-runtime shim:\n{bundle}"
        );
        let css = String::from_utf8_lossy(&dev_get(port, "/asset/client.css", &token)).into_owned();
        assert_eq!(
            css, ".x2{color:red}\n",
            "the sidecar should serve the current CSS"
        );

        // Path traversal is refused — with the token (the guard's own 404) and,
        // since backlog E93, without it (the gate refuses before the guard is
        // reached). Neither serves a byte.
        let traversal = dev_get(port, "/bundle/../secret.js", &token);
        assert!(
            traversal.is_empty(),
            "a traversal path must not serve any bytes"
        );
        let untokened = http_get_head(port, "/bundle/../secret.js");
        assert!(
            untokened.starts_with("HTTP/1.1 403 Forbidden"),
            "an untokened request is refused before the traversal guard: {untokened:?}"
        );

        // (e) The gate itself, on the wire the browser actually uses: the same
        // routes that just answered are refused outright without this run's
        // token (backlog E93). This is the whole of what a page the developer
        // happens to visit while `run --watch` runs can reach — the compiled
        // bundle, the sidecar, the diagnostics stream, and the reload trigger.
        for (method, route) in [
            ("GET", "/events"),
            ("GET", "/bundle/client.js"),
            ("GET", "/asset/client.css"),
            ("POST", "/refresh"),
        ] {
            let refused = http_request_head(port, method, route);
            assert!(
                refused.starts_with("HTTP/1.1 403 Forbidden"),
                "{method} {route} without the token must be refused: {refused:?}"
            );
            let wrong = http_request_head(
                port,
                method,
                &format!("{route}?token=00000000000000000000000000000000"),
            );
            assert!(
                wrong.starts_with("HTTP/1.1 403 Forbidden"),
                "{method} {route} with a wrong token must be refused: {wrong:?}"
            );
        }
    }));

    support::kill_watcher(&mut watcher);
    let _ = std::fs::remove_dir_all(&dir);
    outcome.unwrap();
}

/// The client leg of the B87 repro: a module-level spawn plus a binding that
/// AWAITS it. `pending` is a `Task` (not a transferable form, so it is
/// excluded from adopt), but `value: i32` is `TransferForm::Value`, so it IS
/// wrapped in the `__hmr_adopt` thunk — which is built `is_async: false`.
fn awaiting_initializer_source() -> String {
    "import std::io::print;\nimport std::task::Task;\nimport std::time::sleep;\n\n     fun ready(): i32 {\n\tsleep(0);\n\t7\n}\n\n     let pending: Task<i32> = async ready();\nlet value: i32 = await pending;\n\n     fun main() {\n\tprint(value);\n}\n"
        .to_string()
}

/// B87 — the adopt thunk cannot carry an `await`, and now it never has to.
///
/// Before B86a closed the await-shaped hole, a watch round over this program
/// compiled CLEAN and emitted `return await (pending);` inside the
/// `is_async: false` thunk: a dev bundle that did not parse at all
/// (`node --check` → "SyntaxError: Unexpected reserved word"), so the whole
/// dev loop was dead, not degraded (`top-level-await.md` §1.5).
///
/// The adopt contract is deliberately NOT redesigned — the paper records it as
/// latent, load-bearing only if top-level await is ever allowed (§4.2). What
/// is claimed instead is that the shape is UNREACHABLE from vilan source, and
/// that is what this pins: the same watch round now fails at compile, and no
/// bundle carrying the unparseable shape is ever written.
#[test]
fn an_awaiting_initializer_cannot_reach_the_hmr_adopt_thunk() {
    let dir = temp_project("adopt_await");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    write(&dir, "src/client.vl", &client_source("a", "x1"));
    write(
        &dir,
        "src/server.vl",
        "import std::io::print;\n\nfun main() {\n\tprint(\"server\");\n}\n",
    );

    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", "--watch", "--hmr-port", "0", dir.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn run --watch");
    let lines = drain_both(
        watcher.stdout.take().unwrap(),
        watcher.stderr.take().unwrap(),
    );

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let deadline = support::WATCH_LIVENESS;
        let port = wait_for_port(&lines, deadline)
            .expect("the CLI should announce `hmr: dev channel on 127.0.0.1:<port>`");
        assert!(
            wait_for_file(&dir.join("dist/client.js"), deadline),
            "round 1 should have written dist/client.js"
        );
        std::thread::sleep(Duration::from_millis(500));
        let token = dev_token(&dir, "client", deadline);
        let mut sse = SseClient::connect(port, &token);

        // The edit that used to produce the unparseable bundle.
        write(&dir, "src/client.vl", &awaiting_initializer_source());

        // It is refused at COMPILE, and the refusal is the module-init rule —
        // not some incidental later failure.
        let error_event = sse.expect_event("error", deadline);
        assert!(
            error_event.contains("cannot suspend"),
            "the round should fail with the module-initializer refusal: {error_event}"
        );

        // And the bundle on disk is still round 1's — never one carrying the
        // shape that does not parse. `return await (` is the emitter's
        // spelling and appears nowhere in the hand-written shim, so it is a
        // faithful witness for "an await was walked into the thunk".
        let bundle = std::fs::read_to_string(dir.join("dist/client.js")).expect("read bundle");
        assert!(
            !bundle.contains("return await ("),
            "a bundle carrying `return await (` reached dist/ — the adopt \
             thunk is synchronous, so this does not parse:\n{bundle}"
        );
        assert!(
            !bundle.contains("pkg::value"),
            "the awaited binding must never be handed to the adopt thunk:\n{bundle}"
        );
        // The same, through the route the browser actually fetches.
        let served =
            String::from_utf8_lossy(&dev_get(port, "/bundle/client.js", &token)).into_owned();
        assert!(
            !served.contains("return await (") && !served.contains("pkg::value"),
            "the served bundle must not carry the awaited binding's thunk:\n{served}"
        );
    }));

    support::kill_watcher(&mut watcher);
    let _ = std::fs::remove_dir_all(&dir);
    outcome.unwrap();
}

/// A `common` library both legs import (`pkg::common::banner`). Editing it
/// changes both bundles — the shared-edit row of the §6 matrix.
fn common_source(banner: &str) -> String {
    format!("fun banner(): str {{\n\t\"{banner}\"\n}}\n")
}

/// A browser client that embeds `banner()` (so a shared edit changes this
/// bundle) and emits one CSS line (so the sidecar exists but a server-only edit
/// leaves it untouched — the "no css either" half of the quiet assertion).
fn shared_client_source(css_marker: &str) -> String {
    format!(
        "import std::io::print;\nimport std::asset::emit;\nimport pkg::common::banner;\n\n\
         fun styles(): i32 {{\n\temit(\"css\", \".{css_marker}{{color:red}}\");\n\t1\n}}\n\n\
         let _s = const styles();\n\nfun main() {{\n\tprint(banner());\n}}\n"
    )
}

/// A server that prints a per-source boot marker AND the shared banner, so the
/// watcher's captured stdout witnesses each restart: a server-only edit bumps
/// the marker; a shared edit bumps the banner.
fn shared_server_source(server_marker: &str) -> String {
    format!(
        "import std::io::print;\nimport pkg::common::banner;\n\n\
         fun main() {{\n\tprint(\"server-up {server_marker} banner=\" + banner());\n}}\n"
    )
}

/// The two §6 coordination-matrix rows the S1 e2e doesn't reach (hmr.md §§6, 11
/// S3): a server-only edit restarts the Node child while pushing nothing to the
/// browser, and a shared edit (a `common` module both legs embed) restarts the
/// server AND pushes a `swap`.
#[test]
fn a_server_edit_restarts_quietly_and_a_shared_edit_swaps() {
    let dir = temp_project("matrix");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    write(&dir, "src/common.vl", &common_source("BANNER_ONE"));
    write(&dir, "src/client.vl", &shared_client_source("x1"));
    write(&dir, "src/server.vl", &shared_server_source("SRVMARK_ONE"));

    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", "--watch", "--hmr-port", "0", dir.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn run --watch");

    let lines = drain_stdout(watcher.stdout.take().unwrap());

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let deadline = support::WATCH_LIVENESS;
        let port = wait_for_port(&lines, deadline)
            .expect("the CLI should announce `hmr: dev channel on 127.0.0.1:<port>`");

        // Round 1 is done once dist lands and the server has printed its boot
        // marker. No margin follows: the watcher snapshots its baseline BEFORE
        // the first action (E20), so an edit landing at any point simply
        // triggers a round — the sleep here was a vestige of that race.
        assert!(
            wait_for_file(&dir.join("dist/client.js"), deadline),
            "round 1 should have written dist/client.js"
        );
        assert!(
            wait_for_line(&lines, "SRVMARK_ONE", deadline),
            "the server leg should have booted in round 1"
        );

        let token = dev_token(&dir, "client", deadline);
        let mut sse = SseClient::connect(port, &token);

        // Row 1 — server-only edit: the server bundle changes, the client bundle
        // does not. The Node child restarts (its new boot marker appears on
        // stdout) and NO `swap`/`css` reaches the connected browser — K6
        // reconnect carries it across the restart (hmr.md §6). Observing the
        // restart first makes the quiet window deterministic: the round's push
        // (here, none) is issued before the child it spawned can print.
        write(&dir, "src/server.vl", &shared_server_source("SRVMARK_TWO"));
        assert!(
            wait_for_line(&lines, "SRVMARK_TWO", deadline),
            "a server-only edit should restart the Node child"
        );

        // Row 2 — shared edit: a change to `common.vl`, which both legs embed.
        // The server restarts (the banner it prints changes) AND a `swap` reaches
        // the browser (its bundle changed too, so the byte-diff classifies both).
        // Its `swap` doubles as the closing anchor for row 1's quiet window: the
        // marker above opened the window, this event closes it, and SSE ordering
        // puts any push from the server-only round strictly between them.
        write(&dir, "src/common.vl", &common_source("BANNER_TWO"));
        sse.assert_none_before(&["swap", "css"], "swap", deadline);
        assert!(
            wait_for_line(&lines, "banner=BANNER_TWO", deadline),
            "a shared edit should restart the Node child with the new shared code"
        );
        // The shared round's child has booted, so that round's push was issued
        // and flushed well before: anything still pending is a SECOND push —
        // the only way the server-only round could have spoken without being
        // caught above, since both rounds push the same `swap` kind.
        sse.assert_nothing_pending(&["swap", "css"]);
    }));

    support::kill_watcher(&mut watcher);
    let _ = std::fs::remove_dir_all(&dir);
    outcome.unwrap();
}

/// The per-leg skip (backlog E12, half b): a client-only edit recompiles the
/// client and SKIPS the server — the server's `.vl` sources are unchanged, so
/// its previous artifact is reused and the round prints `hmr: skipped server
/// (sources unchanged)` — while the served client bundle still reflects the
/// edit (the parse cache is content-keyed, never stale). Same single-watcher,
/// quick-exit-legs hygiene as the matrix test.
#[test]
fn a_client_only_edit_skips_the_server_and_still_updates_the_client() {
    let dir = temp_project("skip");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    write(
        &dir,
        "src/client.vl",
        &client_source("clientmark_one", "x1"),
    );
    write(
        &dir,
        "src/server.vl",
        "import std::io::print;\n\nfun main() {\n\tprint(\"server-booted\");\n}\n",
    );

    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", "--watch", "--hmr-port", "0", dir.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn run --watch");

    let lines = drain_stdout(watcher.stdout.take().unwrap());

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let deadline = support::WATCH_LIVENESS;
        let port = wait_for_port(&lines, deadline)
            .expect("the CLI should announce `hmr: dev channel on 127.0.0.1:<port>`");

        // Round 1 compiles both legs and boots the server. No margin follows:
        // the watcher's baseline snapshot precedes its first action (E20), so
        // an edit can no longer be swallowed by the initial build.
        assert!(
            wait_for_file(&dir.join("dist/client.js"), deadline),
            "round 1 should have written dist/client.js"
        );
        assert!(
            wait_for_line(&lines, "server-booted", deadline),
            "the server leg should have booted in round 1"
        );

        let token = dev_token(&dir, "client", deadline);
        let mut sse = SseClient::connect(port, &token);
        let bundle_before =
            String::from_utf8_lossy(&dev_get(port, "/bundle/client.js", &token)).into_owned();
        assert!(
            bundle_before.contains("clientmark_one"),
            "the round-1 client bundle carries the original marker"
        );
        let server_before = std::fs::read(dir.join("dist/server.mjs")).expect("dist/server.mjs");

        // A client-only edit: the client bundle changes, the server's sources do
        // not — so the round SKIPS the server (prints the skip line) and pushes a
        // `swap` for the client.
        write(
            &dir,
            "src/client.vl",
            &client_source("clientmark_two", "x1"),
        );
        assert!(
            wait_for_line(&lines, "hmr: skipped server (sources unchanged)", deadline),
            "a client-only edit must skip recompiling the server"
        );
        sse.expect_kind("swap", deadline);

        // The served client bundle reflects the edit — the content-keyed cache
        // returns the NEW parse, never the stale one.
        let bundle_after =
            String::from_utf8_lossy(&dev_get(port, "/bundle/client.js", &token)).into_owned();
        assert!(
            bundle_after.contains("clientmark_two"),
            "the served client bundle must reflect the edit:\n{bundle_after}"
        );
        assert!(
            !bundle_after.contains("clientmark_one"),
            "the stale client content must be gone"
        );

        // Reuse fidelity: the skipped server leg's dist bytes are the round-1
        // artifact, untouched by the skip round.
        let server_after = std::fs::read(dir.join("dist/server.mjs")).expect("dist/server.mjs");
        assert_eq!(
            server_after, server_before,
            "a skipped leg's dist bytes must be exactly the reused artifact"
        );
        server_after
    }));

    support::kill_watcher(&mut watcher);

    // The cache-hit A/B (review finding, 2026-07-21): after a round that went
    // THROUGH the caches (round 2 skipped the server; the client compiled via
    // parse-cache hits for std), a fresh one-shot build of the same sources
    // must reproduce the reused server bundle byte-for-byte.
    if let Ok(reused) = &outcome {
        let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
            .args(["build", dir.to_str().unwrap()])
            .output()
            .expect("run one-shot build");
        assert!(
            output.status.success(),
            "the one-shot rebuild should succeed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let fresh = std::fs::read(dir.join("dist/server.mjs")).expect("dist/server.mjs");
        assert_eq!(
            &fresh, reused,
            "a one-shot build must equal the reused (cache-hit round) artifact"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
    outcome.unwrap();
}

/// Removes ANSI SGR escape sequences (`\x1b[…m`) so a terminal capture can be
/// asserted as plain text regardless of coloring.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        if character == '\x1b' {
            // Consume the escape body up to and including its final letter.
            for escape_char in chars.by_ref() {
                if escape_char.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(character);
        }
    }
    out
}

/// Terminal-unchanged (the overlay capture is additive, not a redirect): a broken
/// one-shot `vilan build` still renders the ariadne diagnostic to the terminal —
/// the `build` path passes no overlay sink, so its output is the pre-change shape.
/// This pins the key lines; the HMR path shares the same `compile_to_js`/`report`
/// terminal rendering, so its terminal output is unchanged too.
#[test]
fn a_broken_build_still_renders_the_terminal_diagnostic() {
    let dir = temp_project("terminal");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "src/main.vl", "fun main( {\n");
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", dir.to_str().unwrap()])
        .output()
        .expect("run vilan build");
    let _ = std::fs::remove_dir_all(&dir);

    assert!(!output.status.success(), "a broken build must fail");
    // ariadne renders diagnostics to STDERR (windows-support.md §6, ratified
    // call (f) — they used to go to stdout, where they could corrupt
    // `build --stdout`). Strip ANSI to assert the plain shape regardless of
    // whether the stream was colored.
    let stderr = strip_ansi(&String::from_utf8_lossy(&output.stderr));
    assert!(
        stderr.contains("Error:"),
        "the ariadne error header is present:\n{stderr}"
    );
    assert!(
        stderr.contains("expected"),
        "the real diagnostic message is present:\n{stderr}"
    );
    assert!(
        stderr.contains("main.vl"),
        "the diagnostic names the source file:\n{stderr}"
    );
}

/// A node leg whose `main` prints a distinguishing marker — so the watcher's
/// captured stdout witnesses which Node leg actually ran.
fn node_marker(marker: &str) -> String {
    format!("import std::io::print;\n\nfun main() {{\n\tprint(\"{marker}\");\n}}\n")
}

/// Accumulates the watcher's stdout into a shared buffer (rather than a consuming
/// channel), so a test can both wait for a marker AND assert one never appears —
/// the negative the A15 test needs (the non-selected leg must never run).
fn collect_stdout(stdout: ChildStdout) -> Arc<Mutex<Vec<String>>> {
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let sink = buffer.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            sink.lock().unwrap().push(line);
        }
    });
    buffer
}

/// Polls the accumulated stdout for any line containing `needle`, up to `deadline`.
fn buffer_has(buffer: &Arc<Mutex<Vec<String>>>, needle: &str, deadline: Duration) -> bool {
    let start = Instant::now();
    loop {
        if buffer
            .lock()
            .unwrap()
            .iter()
            .any(|line| line.contains(needle))
        {
            return true;
        }
        if start.elapsed() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Polls the accumulated stdout for the dev-channel activation line's port.
fn port_from_buffer(buffer: &Arc<Mutex<Vec<String>>>, deadline: Duration) -> Option<u16> {
    let start = Instant::now();
    loop {
        if let Some(port) = buffer
            .lock()
            .unwrap()
            .iter()
            .find_map(|line| parse_port(line))
        {
            return Some(port);
        }
        if start.elapsed() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A15 (`--entry`): a workspace with TWO Node legs (the kolt shape — a `server`
/// and a `probe`) plus a browser leg. `run --watch --entry server` runs the
/// chosen `server` leg (its boot marker appears), while the non-selected `probe`
/// leg still COMPILES into the workspace (`dist/probe.mjs` exists) but is never
/// launched (its marker never appears). HMR rounds then work under the selection:
/// a client edit swaps, a server edit restarts the chosen leg — and the probe
/// still never runs. Same single-watcher, quick-exit-legs process hygiene as the
/// matrix test.
#[test]
fn run_watch_honors_entry_and_hmr_rounds_work_for_the_chosen_leg() {
    let dir = temp_project("entry");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\n\n[entry.client]\ntarget = \"browser\"\n\n\
         [entry.server]\n\n[entry.probe]\n",
    );
    write(&dir, "src/client.vl", &client_source("c1", "x1"));
    write(&dir, "src/server.vl", &node_marker("SERVER_UP one"));
    write(&dir, "src/probe.vl", &node_marker("PROBE_RAN"));

    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args([
            "run",
            "--watch",
            "--hmr-port",
            "0",
            "--entry",
            "server",
            dir.to_str().unwrap(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn run --watch --entry");

    let buffer = collect_stdout(watcher.stdout.take().unwrap());

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let deadline = support::WATCH_LIVENESS;
        let port = port_from_buffer(&buffer, deadline)
            .expect("the CLI should announce `hmr: dev channel on 127.0.0.1:<port>`");

        // Round 1: the browser leg compiled, the SELECTED server ran, and the
        // non-selected probe COMPILED (its bundle exists) but never RAN.
        assert!(
            wait_for_file(&dir.join("dist/client.js"), deadline),
            "round 1 should have written dist/client.js"
        );
        assert!(
            buffer_has(&buffer, "SERVER_UP one", deadline),
            "the `--entry server` leg should run"
        );
        assert!(
            dir.join("dist/probe.mjs").exists(),
            "the non-selected probe leg still compiles into the workspace"
        );

        let token = dev_token(&dir, "client", deadline);
        let mut sse = SseClient::connect(port, &token);

        // A client edit → the browser swaps under the selected-entry watcher.
        write(&dir, "src/client.vl", &client_source("c2", "x1"));
        sse.expect_kind("swap", deadline);
        // Round 2's swap is the anchor for round 1's negative: a probe launched
        // in round 1 would have printed a whole compile ago. So this is an
        // INSTANT scan of everything captured so far, and a slow box only gives
        // a wrongly-launched probe more time to show up — where the fixed
        // 700 ms window it replaces proved less the slower the box got (E41).
        assert!(
            !buffer_has(&buffer, "PROBE_RAN", Duration::ZERO),
            "the non-selected probe leg must not be launched"
        );

        // A server edit → the chosen Node child restarts (its new marker prints);
        // nothing is pushed to the browser and the probe still never runs.
        write(&dir, "src/server.vl", &node_marker("SERVER_UP two"));
        assert!(
            buffer_has(&buffer, "SERVER_UP two", deadline),
            "a server edit should restart the `--entry` leg"
        );

        // Nothing follows the server round in this test, so a deliberately
        // broken client supplies the closing anchor its quiet window needs.
        // `error` is the right sentinel precisely because it is NEITHER
        // forbidden kind: the round is guaranteed to push it, and SSE ordering
        // then puts any `swap`/`css` from the server-only round strictly before
        // it — with no ambiguity between the two rounds, which is what a `swap`
        // closing anchor would have had here (the matrix test can afford one
        // because a following stdout marker lets it check for a second push).
        write(&dir, "src/client.vl", "fun main( {\n");
        sse.assert_none_before(&["swap", "css"], "error", deadline);
        assert!(
            !buffer_has(&buffer, "PROBE_RAN", Duration::ZERO),
            "the probe leg still never runs"
        );
    }));

    support::kill_watcher(&mut watcher);
    let _ = std::fs::remove_dir_all(&dir);
    outcome.unwrap();
}

/// A/B (backlog E12): the content-addressed parse cache and the watch path must
/// not change a byte. A one-shot `vilan build` and a `run --watch` round compile
/// the SAME sources; the server leg (a node bundle, uninstrumented in both) must
/// come out byte-identical — proving the caching/skip machinery is transparent
/// to emitted output, the same guarantee the corpus gate makes for one-shot.
#[test]
fn a_watch_round_server_bundle_equals_a_one_shot_build() {
    let dir = temp_project("ab");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    write(&dir, "src/client.vl", &client_source("ab_client", "x1"));
    write(
        &dir,
        "src/server.vl",
        "import std::io::print;\n\nfun main() {\n\tprint(\"server-booted\");\n}\n",
    );

    // One-shot build (a fresh process, cold cache) → capture the server bundle.
    let status = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", dir.to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("run vilan build");
    assert!(status.success(), "the one-shot build should succeed");
    let one_shot_server =
        std::fs::read(dir.join("dist/server.mjs")).expect("build wrote dist/server.mjs");

    // A watch round rewrites dist/ from the same sources; its (uninstrumented)
    // server bundle must match byte-for-byte.
    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", "--watch", "--hmr-port", "0", dir.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn run --watch");
    let lines = drain_stdout(watcher.stdout.take().unwrap());

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let deadline = support::WATCH_LIVENESS;
        wait_for_port(&lines, deadline).expect("the dev channel should announce its port");
        // The server boots only after the round has written every dist bundle.
        assert!(
            wait_for_line(&lines, "server-booted", deadline),
            "round 1 should compile and boot the server"
        );
        let watched_server = std::fs::read(dir.join("dist/server.mjs"))
            .expect("the watch round wrote dist/server.mjs");
        assert_eq!(
            one_shot_server, watched_server,
            "a watch round's server bundle must be byte-identical to a one-shot build's"
        );
    }));

    support::kill_watcher(&mut watcher);
    let _ = std::fs::remove_dir_all(&dir);
    outcome.unwrap();
}

/// E16's residual: the overlay names the file each diagnostic BELONGS to. A
/// broken `pkg::common` module reaches the browser as `common.vl:<line>:<col>`
/// — the entry (`client.vl`) heads the overlay, but the location line points at
/// the module, and the line/column are resolved against the module's own text.
/// (Before this, every diagnostic was located as if its span indexed the entry:
/// wrong file, wrong position.)
#[test]
fn the_overlay_locates_a_module_diagnostic_in_its_own_module() {
    let dir = temp_project("overlay_module");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\n\n[entry.client]\ntarget = \"browser\"\n",
    );
    write(&dir, "src/common.vl", &common_source("BANNER_ONE"));
    write(&dir, "src/client.vl", &shared_client_source("x1"));

    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", "--watch", "--hmr-port", "0", dir.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn run --watch");
    let lines = drain_both(
        watcher.stdout.take().unwrap(),
        watcher.stderr.take().unwrap(),
    );

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let deadline = support::WATCH_LIVENESS;
        let port = wait_for_port(&lines, deadline)
            .expect("the CLI should announce `hmr: dev channel on 127.0.0.1:<port>`");
        assert!(
            wait_for_file(&dir.join("dist/client.js"), deadline),
            "round 1 should have written dist/client.js"
        );

        let token = dev_token(&dir, "client", deadline);
        let mut sse = SseClient::connect(port, &token);
        // Break the MODULE, on its own second line, leaving the entry intact.
        // (The margin that stood here paid for E20's baseline-snapshot race.)
        write(
            &dir,
            "src/common.vl",
            "fun banner(): str {\n\tmissing_name()\n}\n",
        );
        let error_event = sse.expect_event("error", deadline);
        assert!(
            error_event.contains("common.vl:2:"),
            "the overlay should locate the diagnostic in the module: {error_event}"
        );
        assert!(
            !error_event.contains("client.vl:2:"),
            "the entry must not be credited with the module's diagnostic: {error_event}"
        );
        // The leg's entry still heads the overlay — it names which build failed.
        assert!(
            error_event.contains("client.vl: 1 error"),
            "the overlay header should still name the failing leg: {error_event}"
        );
    }));

    support::kill_watcher(&mut watcher);
    let _ = std::fs::remove_dir_all(&dir);
    outcome.unwrap();
}

/// E80: a context-coverage refusal reaches the overlay WITH its E78 chain, each
/// hop located in the file the call sits in. The read lives in the module
/// (`common.vl`), the uncovered call in the entry (`client.vl`): the overlay
/// locates the primary in the module and renders the hop as an indented
/// `via <client.vl>:13:8 — …` line — the entry's own line/column, resolved
/// against the entry's text — between the message and the end of the
/// diagnostic, and names that location nowhere else (the shim would count a
/// bare location line as a second error).
#[test]
fn the_overlay_traces_a_cross_module_requirement_chain_in_each_hops_file() {
    let dir = temp_project("overlay_trace");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\n\n[entry.client]\ntarget = \"browser\"\n",
    );
    write(&dir, "src/common.vl", &common_source("BANNER_ONE"));
    write(&dir, "src/client.vl", &shared_client_source("x1"));

    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", "--watch", "--hmr-port", "0", dir.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn run --watch");
    let lines = drain_both(
        watcher.stdout.take().unwrap(),
        watcher.stderr.take().unwrap(),
    );

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let deadline = support::WATCH_LIVENESS;
        let port = wait_for_port(&lines, deadline)
            .expect("the CLI should announce `hmr: dev channel on 127.0.0.1:<port>`");
        assert!(
            wait_for_file(&dir.join("dist/client.js"), deadline),
            "round 1 should have written dist/client.js"
        );

        let token = dev_token(&dir, "client", deadline);
        let mut sse = SseClient::connect(port, &token);
        // The module now reads a context that nothing provides; the entry's
        // `print(banner())` (line 13, column 8 of `shared_client_source`) is
        // the one uncovered call on the path.
        write(
            &dir,
            "src/common.vl",
            "import std::context::Context;\n\nlet current: Context<i32> = Context::new();\n\n\
             fun banner(): str {\n\tcurrent.get();\n\t\"BANNER_ONE\"\n}\n",
        );
        let error_event = sse.expect_event("error", deadline);
        assert!(
            error_event.contains("common.vl:6:2"),
            "the primary is located at the read, in the module: {error_event}"
        );
        let hop = "via ";
        let hop_at = error_event
            .find(hop)
            .unwrap_or_else(|| panic!("the overlay carries a trace line: {error_event}"));
        assert!(
            error_event[hop_at..].starts_with("via ")
                && error_event[hop_at..]
                    .contains("client.vl:13:8 — the context requirement flows through this call"),
            "the hop is located in the ENTRY, against the entry's text: {error_event}"
        );
        assert!(
            hop_at > error_event.find("common.vl:6:2").unwrap(),
            "the chain follows the primary it belongs to: {error_event}"
        );
        assert_eq!(
            error_event.matches("client.vl:13:8").count(),
            1,
            "the hop's location is named once, on its trace line: {error_event}"
        );
        assert!(
            error_event.contains("client.vl: 1 error"),
            "one diagnostic, one error in the header: {error_event}"
        );
    }));

    support::kill_watcher(&mut watcher);
    let _ = std::fs::remove_dir_all(&dir);
    outcome.unwrap();
}

// --- E55 (css half): the css hot-swap must not round-trip a stale server -----

/// A server shaped exactly like `examples/todo/src/server.vl`: `fs::read` the
/// browser's stylesheet ONCE at boot and serve that snapshot at `/client.css`
/// for the life of the process. A css-only watch round never restarts this
/// server (hmr.md §6, `classify`), so its route is the trap the shim must not
/// fall into — proving the fix needs a server that behaves exactly like the
/// real-world idiom it's fixing, not a stand-in.
fn boot_time_css_server_source() -> String {
    "import std::fs;\nimport std::http::{ Server, Response };\nimport std::io::print;\nimport std::process;\n\n\
     fun main() {\n\
     \tlet client_css = fs::read_file_to_str(\"dist/client.css\");\n\
     \tServer::builder()\n\
     \t\t.port(0)\n\
     \t\t.on_request(|request| {\n\
     \t\t\tmatch request.path() {\n\
     \t\t\t\t\"/client.css\" => Response::builder().set_header(\"Content-Type\", \"text/css\").body(client_css).build(),\n\
     \t\t\t\t\"/shutdown\" => {\n\
     \t\t\t\t\tprocess::exit(0);\n\
     \t\t\t\t\tResponse::builder().body(\"\").build()\n\
     \t\t\t\t}\n\
     \t\t\t\t_ => Response::builder().code(404).body(\"\").build(),\n\
     \t\t\t}\n\
     \t\t})\n\
     \t\t.on_start(|server| print(i\"css-server-up {server.port()}\"))\n\
     \t\t.build()\n\
     \t\t.start();\n\
     }\n"
        .to_string()
}

/// Waits (bounded) for this test's own boot marker (`css-server-up <port>`,
/// printed by [`boot_time_css_server_source`]) and returns the port it names —
/// `wait_for_port`'s twin for the OTHER server this test runs.
fn wait_for_css_server_port(lines: &mpsc::Receiver<String>, deadline: Duration) -> Option<u16> {
    let start = Instant::now();
    while start.elapsed() < deadline {
        match lines.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                if let Some(port) = line
                    .strip_prefix("css-server-up ")
                    .and_then(|rest| rest.trim().parse().ok())
                {
                    return Some(port);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => return None,
        }
    }
    None
}

/// The node harness that drives the REAL shipped shim (the bytes served by the
/// dev channel, unmodified) under a minimal DOM stub. `__SERVER_PORT__` is the
/// boot-time-stale server's port — substituted before writing. Three stub
/// `<link>`s stand in for a page: `ghost` names an asset the dev channel does
/// NOT have (the 404/keep-old-sheet path), `client` is the one the css event
/// actually names, and `other` is a sidecar the event must leave untouched
/// (the asset-matching semantics, hmr.md §2). Node's own global `fetch` is
/// used unstubbed, so a request to `http://127.0.0.1:<PORT>/asset/...` is a
/// REAL request to the REAL dev channel spawned by this test — the wiring
/// under test, not a mock of it.
const CSS_HARNESS_TEMPLATE: &str = r#"import fs from "node:fs";

class StubLink {
    constructor(href) { this.href = href; this.rel = "stylesheet"; this.disabled = false; }
}
class StyleHost {
    constructor() { this.children = []; }
    appendChild(el) { this.children.push(el); return el; }
}
class StubStyle {
    constructor(tag) { this.tagName = tag; this._text = ""; }
    set textContent(t) { this._text = t; }
    get textContent() { return this._text; }
}

const ghostLink = new StubLink("http://127.0.0.1:__SERVER_PORT__/ghost.css");
const clientLink = new StubLink("http://127.0.0.1:__SERVER_PORT__/client.css");
const otherLink = new StubLink("http://127.0.0.1:__SERVER_PORT__/other.css");
const clientHref = clientLink.href;
const head = new StyleHost();

globalThis.window = globalThis;
globalThis.document = {
    querySelectorAll: (selector) =>
        selector === 'link[rel="stylesheet"]' ? [ghostLink, clientLink, otherLink] : [],
    createElement: (tag) => new StubStyle(tag),
    // `handleEvent` clears a lingering error overlay on every non-error event
    // (removeOverlay → getElementById) — none is ever present here.
    getElementById: () => null,
    head,
    documentElement: head,
};
globalThis.location = { reload: () => { globalThis.__reloaded = true; } };
// No EventSource under node — the shim's own connect() no-ops without one.

let failures = 0;
function check(condition, message) {
    if (condition) { console.log("ok   - " + message); }
    else { failures += 1; console.error("FAIL - " + message); }
}

await import("./bundleA.mjs");
const hmr = globalThis.window.__VILAN_HMR__;
check(!!hmr, "the shim installed the singleton");

const freshOnDisk = fs.readFileSync("dist/client.css", "utf8");
const staleSnapshot = fs.readFileSync("stale-snapshot.css", "utf8");
check(freshOnDisk !== staleSnapshot, "harness sanity: round 2's css differs from the boot-time snapshot");

// (1) The event names an asset the dev channel does NOT have (`dist/ghost.css`
// was never written) — the fetch 404s. Sane failure: warn, touch nothing.
await hmr.handleEvent({ kind: "css", version: hmr.version, asset: "ghost.css" });
check(head.children.length === 0, "a missing asset creates no <style>");
check(ghostLink.disabled === false, "a missing asset leaves its <link> enabled");

// (2) The real fix: a `css` event for `client.css` fetches the CURRENT bytes
// from the dev channel and applies them — never the stale server route.
await hmr.handleEvent({ kind: "css", version: hmr.version, asset: "client.css" });
check(clientLink.disabled === true, "the stale <link> is disabled once fresh css lands");
check(clientLink.href === clientHref, "the <link> href is left untouched (never re-pointed at the stale route)");
check(head.children.length === 1, "exactly one <style> was injected");
const style = head.children[0];
check(style.textContent === freshOnDisk, "the injected <style> carries the CURRENT dist/client.css bytes");
check(style.textContent !== staleSnapshot, "the injected <style> differs from the server's boot-time snapshot");
check(otherLink.disabled === false, "an unrelated sidecar's <link> is untouched (asset-matching preserved)");

// (3) A second `css` event for the SAME asset updates the SAME <style> —
// no duplicate element, no href churn, no reload.
await hmr.handleEvent({ kind: "css", version: hmr.version, asset: "client.css" });
check(head.children.length === 1, "a repeated css event updates the existing <style>, no duplicate");
check(head.children[0] === style, "the same <style> element is reused across swaps");
check(!globalThis.__reloaded, "a css hot-swap never reloads the page");

// The verdict travels on stdout, not only the exit code: node's Windows
// shutdown race (see the tolerance at the call site) can abort the process
// AFTER this line runs, corrupting the status of a fully-passed run.
console.log(failures === 0 ? "css harness verdict: PASS" : "css harness verdict: FAIL");
process.exit(failures === 0 ? 0 : 1);
"#;

/// The E55 css half, pinned end to end: a style-only edit reaches the browser
/// leg through the dev channel even though the workspace's own server — shaped
/// exactly like `examples/todo` — reads its stylesheet once at boot and is
/// never restarted by a css-only round (hmr.md §6). Before the fix,
/// `bumpStylesheets` only cache-busted the `<link>`'s existing href, which
/// pointed right back at that stale boot-time snapshot; this test fails red
/// against that code (verified by reverting the shim change) because no
/// `<style>` element — and no request to the dev channel — is ever produced.
///
/// The bundle driven here is not hand-written: it's fetched from the dev
/// channel exactly as a real page would, so the harness exercises the shim
/// byte-for-byte as shipped, not a copy of it.
#[test]
fn a_css_push_heals_a_boot_time_stale_server_route() {
    let dir = temp_project("css_fresh");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    write(&dir, "src/client.vl", &client_source("a", "x1"));
    write(&dir, "src/server.vl", &boot_time_css_server_source());

    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", "--watch", "--hmr-port", "0", dir.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn run --watch");

    let lines = drain_both(
        watcher.stdout.take().unwrap(),
        watcher.stderr.take().unwrap(),
    );

    // E60: the server leg deliberately breaks the house quick-exit rule (the
    // whole point is a server that OUTLIVES rounds), so it must die by the
    // harness's hand — killing the watcher only orphans its node grandchild.
    // The port escapes the assertion closure so cleanup runs red or green.
    let css_server_port = std::cell::Cell::new(None::<u16>);

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let deadline = support::WATCH_LIVENESS;
        let dev_port = wait_for_port(&lines, deadline)
            .expect("the CLI should announce `hmr: dev channel on 127.0.0.1:<port>`");
        assert!(
            wait_for_file(&dir.join("dist/client.css"), deadline),
            "round 1 should have written dist/client.css"
        );
        let server_port = wait_for_css_server_port(&lines, deadline)
            .expect("the boot-time-stale server should announce `css-server-up <port>`");
        css_server_port.set(Some(server_port));

        // Round 1's boot-time snapshot — what the server read once, at start,
        // and will keep serving verbatim through a css-only round.
        let stale_snapshot = http_get(server_port, "/client.css");
        assert_eq!(
            stale_snapshot,
            b".x1{color:red}\n".to_vec(),
            "the server's boot-time read should be round 1's css"
        );

        let token = dev_token(&dir, "client", deadline);
        let mut sse = SseClient::connect(dev_port, &token);

        // A style-only edit (the bundle stays byte-identical) → a `css` event
        // naming its sidecar.
        write(&dir, "src/client.vl", &client_source("a", "x2"));
        let css_event = sse.expect_event("css", deadline);
        assert!(
            css_event.contains("\"asset\":\"client.css\""),
            "the css event should name its changed sidecar: {css_event}"
        );

        // The hazard, confirmed directly: the server was never restarted, so
        // its OWN route still serves round 1's bytes — anything relying on
        // THIS route (a cache-busted refetch of the <link>'s own href, the
        // pre-fix behavior) would round-trip the very staleness this fix
        // closes.
        let still_stale = http_get(server_port, "/client.css");
        assert_eq!(
            still_stale, stale_snapshot,
            "a css-only round must not restart the server — its route stays stale"
        );

        // The dev channel, meanwhile, already has the fresh bytes (S0/S1 —
        // `write_assets` runs every round); this is the route the fix must use.
        let dev_channel_css = dev_get(dev_port, "/asset/client.css", &token);
        assert_eq!(
            dev_channel_css,
            b".x2{color:red}\n".to_vec(),
            "the dev channel should serve round 2's css"
        );

        std::fs::write(dir.join("stale-snapshot.css"), &stale_snapshot).unwrap();

        // The bundle the browser actually runs — fetched from the dev channel,
        // exactly as the real shim's own `<script>` origin would be.
        let bundle_a = dev_get(dev_port, "/bundle/client.js", &token);
        assert!(
            String::from_utf8_lossy(&bundle_a).contains("window.__VILAN_HMR__"),
            "the served bundle should carry the dev-runtime shim"
        );
        std::fs::write(dir.join("bundleA.mjs"), &bundle_a).unwrap();

        let harness = CSS_HARNESS_TEMPLATE.replace("__SERVER_PORT__", &server_port.to_string());
        std::fs::write(dir.join("harness.mjs"), harness).unwrap();

        let run = Command::new("node")
            .arg("harness.mjs")
            .current_dir(&dir)
            .output()
            .expect("run node harness");
        let stdout = String::from_utf8_lossy(&run.stdout);
        let stderr = String::from_utf8_lossy(&run.stderr);
        // Windows only: node's own shutdown race, not a harness failure.
        // `uv_async_send` aborts on a closing handle during exit teardown
        // (nodejs/node#56645 / #58091 — rpc_http.rs documents the same
        // tolerance), and the abort lands strictly AFTER the harness's
        // complete stdout, corrupting the exit code of a fully-passed run.
        // The stdout verdict sentinel is the truth on that path: a failed
        // check prints FAIL and stays red regardless of this tolerance.
        let windows_teardown_abort = cfg!(windows)
            && stdout.contains("css harness verdict: PASS")
            && stderr.contains("Assertion failed: !(handle->flags & UV_HANDLE_CLOSING)")
            && stderr.contains("async.c");
        assert!(
            run.status.success() || windows_teardown_abort,
            "css harness failed:\n{stdout}\n{stderr}"
        );
    }));

    support::kill_watcher(&mut watcher);
    // The server survives the watcher by design, so ask it to exit and — on
    // the green path only, so a red run's own panic is never masked — assert
    // it actually died. Each poll RE-SENDS /shutdown: a request that lands
    // exits the process within milliseconds, and a connect that refuses is
    // the death witness. This assert is the pin that E60's leak cannot
    // silently return.
    if let Some(port) = css_server_port.get() {
        let start = Instant::now();
        let dead = loop {
            match std::net::TcpStream::connect(("127.0.0.1", port)) {
                Err(_) => break true,
                Ok(mut stream) => {
                    use std::io::Write;
                    let _ = stream.write_all(
                        b"GET /shutdown HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
                    );
                    if start.elapsed() > support::WATCH_LIVENESS {
                        break false;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        };
        if outcome.is_ok() {
            assert!(
                dead,
                "the boot-time-stale server must exit on /shutdown — \
                 an orphan here is E60's leak returning"
            );
        }
    }
    if outcome.is_ok() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    outcome.unwrap();
}
// --- kolt.local 007, face one: a stylesheet that is NEW this session ---------

/// [`client_source`]'s sibling for a css *presence* transition. The `const`
/// initializer still folds to `1`, so the emitted browser bundle is byte-
/// identical whichever `asset_kind` is passed (verified with two `vilan build`s:
/// `dist/client.js` diffs clean) — only the compile-time `emit`'s asset KIND
/// moves. `"txt"` therefore compiles a leg with NO stylesheet at all
/// (`dist/client.css` absent, the build manifest's `"styles": null`), and
/// `"css"` compiles the SAME bundle WITH one, so a watch round between the two
/// is a clean css-only round whose sidecar goes `None` -> `Some`: the
/// first-ever stylesheet of the session.
fn presence_client_source(asset_kind: &str) -> String {
    format!(
        "import std::io::print;\nimport std::asset::emit;\n\nfun styles(): i32 {{\n\temit(\"{asset_kind}\", \".added{{color:red}}\");\n\t1\n}}\n\nlet _s = const styles();\n\nfun main() {{\n\tprint(\"a\");\n}}\n"
    )
}

/// A server shaped like the real idiom `dev-loop.md` documents: it decides the
/// page's markup ONCE, at boot, and serves that snapshot for the life of the
/// process. Here the boot-time decision is the one that matters to kolt.local
/// 007 — whether to render a `<link>` for the client leg's style sidecar, which
/// `fs::stat(..).is_some()` answers (the probe that replaced the deleted
/// `fs::exists`). Booted in a round that emitted no stylesheet, the page
/// it serves carries the app's own hand-written `theme.css` and NOTHING for
/// `client.css`; and since a css-only round never restarts this server
/// (hmr.md §6, `classify`), it never will.
///
/// That is the cause behind the item's question — "a style that is new this
/// round may have nothing to supersede: where does it land?" — made real
/// rather than stubbed: the document genuinely lacks the `<link>`, because the
/// process that rendered it predates the stylesheet.
fn boot_rendered_page_server_source() -> String {
    "import std::fs;\nimport std::http::{ Server, Response };\nimport std::io::print;\nimport std::process;\n\n\
     fun main() {\n\
     \tlet sidecar = if fs::stat(\"dist/client.css\").is_some() { \"<link rel=\\\"stylesheet\\\" href=\\\"/client.css\\\">\" } else { \"\" };\n\
     \tlet page = i\"<!doctype html><head><link rel=\\\"stylesheet\\\" href=\\\"/theme.css\\\">{sidecar}</head><body><script src=\\\"/client.js\\\"></script></body>\";\n\
     \tServer::builder()\n\
     \t\t.port(0)\n\
     \t\t.on_request(|request| {\n\
     \t\t\tmatch request.path() {\n\
     \t\t\t\t\"/\" => Response::builder().set_header(\"Content-Type\", \"text/html\").body(page).build(),\n\
     \t\t\t\t\"/shutdown\" => {\n\
     \t\t\t\t\tprocess::exit(0);\n\
     \t\t\t\t\tResponse::builder().body(\"\").build()\n\
     \t\t\t\t}\n\
     \t\t\t\t_ => Response::builder().code(404).body(\"\").build(),\n\
     \t\t\t}\n\
     \t\t})\n\
     \t\t.on_start(|server| print(i\"css-server-up {server.port()}\"))\n\
     \t\t.build()\n\
     \t\t.start();\n\
     }\n"
        .to_string()
}

/// [`CSS_HARNESS_TEMPLATE`]'s twin for the cell it does not reach: the `<link>`
/// is **absent**. Same node DOM stub, same REAL shipped shim fetched from the
/// REAL dev channel, but the stub document is the one
/// [`boot_rendered_page_server_source`] actually served this round — one
/// hand-written `theme.css` sheet and no `client.css` link at all.
/// `__SERVER_PORT__` is that server's port, substituted before writing.
///
/// The existing harness covers (link present, asset present) and (link present,
/// asset missing). This is (link ABSENT, asset present): the stylesheet exists
/// and the dev channel serves it, and the page has nowhere to put it.
const NEW_STYLESHEET_HARNESS_TEMPLATE: &str = r#"import fs from "node:fs";

class StubLink {
    constructor(href) { this.href = href; this.rel = "stylesheet"; this.disabled = false; }
}
class StyleHost {
    constructor() { this.children = []; }
    appendChild(el) { this.children.push(el); return el; }
}
class StubStyle {
    constructor(tag) { this.tagName = tag; this._text = ""; }
    set textContent(t) { this._text = t; }
    get textContent() { return this._text; }
}

// The document the server rendered at boot, when the client leg emitted no
// stylesheet: the app's own hand-written sheet, and NO <link> for client.css.
const themeLink = new StubLink("http://127.0.0.1:__SERVER_PORT__/theme.css");
const head = new StyleHost();

globalThis.window = globalThis;
globalThis.document = {
    querySelectorAll: (selector) =>
        selector === 'link[rel="stylesheet"]' ? [themeLink] : [],
    createElement: (tag) => new StubStyle(tag),
    getElementById: () => null,
    head,
    documentElement: head,
};
globalThis.location = { reload: () => { globalThis.__reloaded = true; } };

let failures = 0;
function check(condition, message) {
    if (condition) { console.log("ok   - " + message); }
    else { failures += 1; console.error("FAIL - " + message); }
}

await import("./bundleA.mjs");
const hmr = globalThis.window.__VILAN_HMR__;
check(!!hmr, "the shim installed the singleton");

const added = fs.readFileSync("dist/client.css", "utf8");
check(added.length > 0, "harness sanity: this round wrote a first-ever dist/client.css");

// The event the round actually pushed, replayed through the REAL handler.
await hmr.handleEvent({ kind: "css", version: hmr.version, asset: "client.css" });

// THE PIN. The old `bumpStylesheets` walked link[rel="stylesheet"], matched on
// the asset's basename, found nothing, and returned undefined — no <style>
// injected, no request ever reaching the dev channel. A stylesheet the dev
// channel is serving must still reach the page: with no <link> to supersede,
// the fresh sheet is appended to <head> on its own.
check(
    head.children.some((element) => element.textContent === added),
    "the first-ever stylesheet reaches the page as a <style> carrying dist/client.css",
);
check(
    head.children.length <= 1,
    "healing a link-less sheet injects at most one <style>, not a stack",
);
check(themeLink.disabled === false, "the app's unrelated hand-written sheet stays enabled");
check(!globalThis.__reloaded, "healing a first-ever stylesheet still never reloads the page");

// The verdict travels on stdout, not only the exit code — see the tolerance at
// the call site (node's Windows shutdown race).
console.log(failures === 0 ? "css harness verdict: PASS" : "css harness verdict: FAIL");
process.exit(failures === 0 ? 0 : 1);
"#;

/// kolt.local 007, face one ("newly **added** css styles do not HMR
/// correctly"), pinned end to end — the apply-layer half of the two classifier
/// cells in `hmr.rs` (`a_first_ever_stylesheet_is_not_a_css_hot_swap`,
/// `a_removed_stylesheet_is_not_a_css_hot_swap`).
///
/// `a_css_push_heals_a_boot_time_stale_server_route` covers the cells where the
/// `<link>` is PRESENT: the asset changed (the sheet is superseded with fresh
/// bytes) and the asset is missing (404 → the never-reload discipline keeps the
/// old sheet). It never reaches the cell the item actually asks about — the
/// `<link>` **absent** — because its client leg emits css from round 1, so the
/// page always has one.
///
/// Here it does not. Round 1's client emits a `txt` asset and no stylesheet, so
/// the server boots, probes `dist/client.css`, finds nothing, and renders a page
/// with only its own `theme.css`. The edit then switches the compile-time emit
/// to `css` — the bundle stays byte-identical, so the round is css-only — and
/// the CLI pushes `{"kind":"css","asset":"client.css"}` for a stylesheet the
/// document has no `<link>` for. The server is not restarted by a css round, so
/// the page never gains one either. `bumpStylesheets` walks the links, matches
/// none, injects nothing, and returns `undefined`: the styles are invisible
/// until the developer reloads by hand. That is the item's "may have nothing to
/// supersede — where does it land?", answered: nowhere.
///
/// CORRECT: a `css` push whose sidecar the dev channel is serving must reach
/// the page. With a `<link>` to supersede the shim disables it and shadows it
/// with a `<style>` (hmr.md's 2026-08-10 appendix); with none, the same
/// `<style>` is simply appended to `<head>` on its own — once, updated in place
/// on a later event, and without touching the sheets the page already has.
///
/// This half stands whatever the classifier does. A push is BROADCAST to every
/// connected client, so a tab that loaded the page before the stylesheet existed
/// receives the same event as one that loaded after — the apply layer must
/// handle a link-less document regardless.
///
/// FIXED (Order 15) at both layers, and the round-level half MOVED WITH THE
/// CLASSIFIER exactly as this pin's original text said it would: a presence
/// transition is no longer a `css` hot-swap, so the round now pushes `swap`
/// DECLARING the round's stylesheet set — and the harness half below still
/// drives the `css` handler directly, because a link-less document must be
/// handled on that path too (a later round that changes this same sheet pushes
/// `css` to a tab whose page still has no `<link>` for it). The apply rule is
/// one rule with two triggers: with a `<link>` to supersede the shim disables it
/// and shadows it; with none, the same `<style>` joins `<head>` on its own.
/// Every cell of the matrix is pinned in `tests/hmr_css_matrix.rs`.
#[test]
fn a_first_ever_stylesheet_reaches_a_page_that_has_no_link_for_it() {
    let dir = temp_project("css_new_sheet");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    write(&dir, "src/client.vl", &presence_client_source("txt"));
    write(&dir, "src/server.vl", &boot_rendered_page_server_source());

    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", "--watch", "--hmr-port", "0", dir.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn run --watch");

    let lines = drain_both(
        watcher.stdout.take().unwrap(),
        watcher.stderr.take().unwrap(),
    );

    // E60: this server outlives rounds by design, so it must die by the
    // harness's hand — killing the watcher only orphans its node grandchild.
    // The port escapes the assertion closure so cleanup runs red or green.
    let page_server_port = std::cell::Cell::new(None::<u16>);

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let deadline = support::WATCH_LIVENESS;
        let dev_port = wait_for_port(&lines, deadline)
            .expect("the CLI should announce `hmr: dev channel on 127.0.0.1:<port>`");
        assert!(
            wait_for_file(&dir.join("dist/client.js"), deadline),
            "round 1 should have written dist/client.js"
        );
        let server_port = wait_for_css_server_port(&lines, deadline)
            .expect("the boot-rendered-page server should announce `css-server-up <port>`");
        page_server_port.set(Some(server_port));

        // Round 1 emitted a `txt` asset and no stylesheet at all.
        assert!(
            !dir.join("dist/client.css").exists(),
            "round 1's client leg must emit no stylesheet — the point of this test"
        );

        // The document the browser loaded: the app's own sheet, no client.css.
        let boot_page = String::from_utf8_lossy(&http_get(server_port, "/")).into_owned();
        assert!(
            boot_page.contains("href=\"/theme.css\""),
            "the boot-rendered page should carry the app's own sheet: {boot_page}"
        );
        assert!(
            !boot_page.contains("client.css"),
            "the boot-rendered page must have NO <link> for the sidecar that did \
             not exist yet: {boot_page}"
        );

        let token = dev_token(&dir, "client", deadline);
        let mut sse = SseClient::connect(dev_port, &token);

        // The edit that ADDS the first-ever stylesheet. Only the compile-time
        // emit's asset kind changes, so the browser bundle is byte-identical
        // and the ONLY thing this round changed is the stylesheet's presence.
        write(&dir, "src/client.vl", &presence_client_source("css"));
        // The classification: a sidecar that APPEARED is a browser-output
        // change, not the in-place replacement of a loaded sheet's text — so
        // `swap`, declaring the round's stylesheet set (`hmr.rs`'s
        // `a_first_ever_stylesheet_is_not_a_css_hot_swap` pins the decision;
        // this pins the event that reaches the wire).
        let swap_event = sse.expect_event("swap", deadline);
        assert!(
            swap_event.contains("\"sheets\":[\"client.css\"]"),
            "the round should declare the newly-added sidecar: {swap_event}"
        );
        assert!(
            wait_for_file(&dir.join("dist/client.css"), deadline),
            "the round should have written the new dist/client.css"
        );

        // The cause, confirmed directly: a round that changed no server bundle
        // never restarts the server — `swap` or `css` alike — so the page it
        // serves STILL has no <link> for the sheet. This is why the apply layer
        // has to handle a link-less document: nothing else ever will.
        let after_page = String::from_utf8_lossy(&http_get(server_port, "/")).into_owned();
        assert!(
            !after_page.contains("client.css"),
            "a client-only round must not restart the server — the document \
             never gains the missing <link>: {after_page}"
        );

        // The dev channel, meanwhile, is serving the new stylesheet happily —
        // so this is not the 404/never-reload path, it is a healthy push with
        // nowhere to land.
        let dev_channel_css = dev_get(dev_port, "/asset/client.css", &token);
        assert_eq!(
            dev_channel_css,
            b".added{color:red}\n".to_vec(),
            "the dev channel should serve the newly-added stylesheet"
        );

        // The bundle the browser actually runs, fetched exactly as a real page's
        // <script> origin would fetch it.
        let bundle_a = dev_get(dev_port, "/bundle/client.js", &token);
        assert!(
            String::from_utf8_lossy(&bundle_a).contains("window.__VILAN_HMR__"),
            "the served bundle should carry the dev-runtime shim"
        );
        std::fs::write(dir.join("bundleA.mjs"), &bundle_a).unwrap();

        let harness =
            NEW_STYLESHEET_HARNESS_TEMPLATE.replace("__SERVER_PORT__", &server_port.to_string());
        std::fs::write(dir.join("harness.mjs"), harness).unwrap();

        let run = Command::new("node")
            .arg("harness.mjs")
            .current_dir(&dir)
            .output()
            .expect("run node harness");
        let stdout = String::from_utf8_lossy(&run.stdout);
        let stderr = String::from_utf8_lossy(&run.stderr);
        // Windows only: node's own shutdown race, not a harness failure — the
        // same tolerance `a_css_push_heals_a_boot_time_stale_server_route`
        // documents at length (nodejs/node#56645 / #58091). The stdout verdict
        // sentinel is the truth on that path.
        let windows_teardown_abort = cfg!(windows)
            && stdout.contains("css harness verdict: PASS")
            && stderr.contains("Assertion failed: !(handle->flags & UV_HANDLE_CLOSING)")
            && stderr.contains("async.c");
        assert!(
            run.status.success() || windows_teardown_abort,
            "new-stylesheet harness failed:\n{stdout}\n{stderr}"
        );
    }));

    support::kill_watcher(&mut watcher);
    // The server survives the watcher by design, so ask it to exit and — on the
    // green path only, so a red run's own panic is never masked — assert it
    // actually died. Each poll RE-SENDS /shutdown: a request that lands exits
    // the process within milliseconds, and a connect that refuses is the death
    // witness.
    if let Some(port) = page_server_port.get() {
        let start = Instant::now();
        let dead = loop {
            match std::net::TcpStream::connect(("127.0.0.1", port)) {
                Err(_) => break true,
                Ok(mut stream) => {
                    use std::io::Write;
                    let _ = stream.write_all(
                        b"GET /shutdown HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
                    );
                    if start.elapsed() > support::WATCH_LIVENESS {
                        break false;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        };
        if outcome.is_ok() {
            assert!(
                dead,
                "the boot-rendered-page server must exit on /shutdown — \
                 an orphan here is E60's leak returning"
            );
        }
    }
    if outcome.is_ok() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    outcome.unwrap();
}

/// kolt.local 007's WORST finding, pinned end to end: **removing a stylesheet
/// used to reassert it.**
///
/// `main.rs`'s dist writer only ever wrote `dist/<leg>.css` inside
/// `if let Some(css)`, and the one thing that deleted anything
/// (`sweep_stale_chunks`) matched only `<leg>.<arm>.js` and `<leg>.chunks.json`
/// — so a round that stopped emitting a stylesheet left the PREVIOUS round's
/// sidecar on disk. The classifier then announced `css` for it (its css line
/// asked only `old.css != leg.css`), the shim fetched `/asset/client.css`, got a
/// healthy **200** carrying those stale bytes, and injected them as a `<style>`
/// superseding the `<link>`. Deleting a stylesheet re-applied it, and nothing
/// looked wrong anywhere: no 404, no warning, no failed round.
///
/// Fixed at both ends, and this pin holds both:
///
/// - `sweep_stale_sidecar` — the leg's dist namespace belongs to its LAST build
///   (`bundle-splitting.md` §S3 item 4), the sidecar included — so
///   `dist/client.css` is gone and the dev channel 404s it. There are no stale
///   bytes to serve, to anyone, ever again.
/// - the classifier — a presence transition is a `swap` declaring the round's
///   stylesheet set, which no longer names `client.css`. That empty declaration
///   is what tells the browser to withdraw its copy (pinned per cell in
///   `tests/hmr_css_matrix.rs`), and it comes from the ROUND, never from a
///   failed fetch: a 404 stays governed by the never-reload discipline.
#[test]
fn a_removed_stylesheet_leaves_no_sidecar_and_declares_none() {
    let dir = temp_project("css_removed");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    // Round 1 emits a stylesheet; the edit below switches the compile-time emit
    // to a `txt` asset, so the bundle stays byte-identical and the ONLY thing
    // the round changes is the stylesheet's presence.
    write(&dir, "src/client.vl", &presence_client_source("css"));
    write(
        &dir,
        "src/server.vl",
        "import std::io::print;\n\nfun main() {\n\tprint(\"server\");\n}\n",
    );

    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", "--watch", "--hmr-port", "0", dir.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn run --watch");

    let lines = drain_both(
        watcher.stdout.take().unwrap(),
        watcher.stderr.take().unwrap(),
    );

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let deadline = support::WATCH_LIVENESS;
        let port = wait_for_port(&lines, deadline)
            .expect("the CLI should announce `hmr: dev channel on 127.0.0.1:<port>`");
        let sidecar = dir.join("dist/client.css");
        assert!(
            wait_for_file(&sidecar, deadline),
            "round 1 should have written dist/client.css"
        );
        let token = dev_token(&dir, "client", deadline);
        assert_eq!(
            dev_get(port, "/asset/client.css", &token),
            b".added{color:red}\n".to_vec(),
            "round 1's sidecar should be served by the dev channel"
        );
        let mut sse = SseClient::connect(port, &token);

        // The edit that REMOVES the stylesheet.
        write(&dir, "src/client.vl", &presence_client_source("txt"));

        let swap_event = sse.expect_event("swap", deadline);
        assert!(
            swap_event.contains("\"sheets\":[]"),
            "the round must DECLARE that it emits no stylesheet — an empty set is \
             the statement that withdraws the page's copy: {swap_event}"
        );

        // The resurrection fix: the sidecar is gone from disk, so the dev
        // channel has nothing stale left to hand anybody.
        let start = Instant::now();
        while sidecar.exists() && start.elapsed() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            !sidecar.exists(),
            "a round that emits no stylesheet must leave none behind — \
             {} survived the round that stopped emitting it",
            sidecar.display()
        );
        let head = http_get_head(port, &format!("/asset/client.css?token={token}"));
        assert!(
            head.starts_with("HTTP/1.1 404"),
            "the dev channel must no longer serve the removed stylesheet: {head}"
        );
    }));

    support::kill_watcher(&mut watcher);
    if outcome.is_ok() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    outcome.unwrap();
}

// --- dev-refresh.md §5 item 2: `std::watch::force_refresh()` -------------------

/// A server that calls `std::watch::force_refresh()` on every request except
/// `/shutdown` (E60: this server deliberately outlives rounds — the whole
/// point is to answer a trigger request after round 1 — so it needs its own
/// death, exactly the css e2e's mimic-server shape).
fn force_refresh_server_source() -> String {
    "import std::watch;\nimport std::http::{ Response, Server };\nimport std::io::print;\nimport std::process;\n\n\
     fun main() {\n\
     \tServer::builder()\n\
     \t\t.port(0)\n\
     \t\t.on_request(|request| {\n\
     \t\t\tmatch request.path() {\n\
     \t\t\t\t\"/shutdown\" => {\n\
     \t\t\t\t\tprocess::exit(0);\n\
     \t\t\t\t\tResponse::builder().body(\"\").build()\n\
     \t\t\t\t}\n\
     \t\t\t\t_ => {\n\
     \t\t\t\t\twatch::force_refresh();\n\
     \t\t\t\t\tResponse::builder().body(\"triggered\").build()\n\
     \t\t\t\t}\n\
     \t\t\t}\n\
     \t\t})\n\
     \t\t.on_start(|server| print(i\"refresh-server-up {server.port()}\"))\n\
     \t\t.build()\n\
     \t\t.start();\n\
     }\n"
        .to_string()
}

/// Waits (bounded) for this test's own boot marker (`refresh-server-up
/// <port>`, printed by [`force_refresh_server_source`]) and returns the port
/// it names — `wait_for_port`'s twin for the OTHER server this test runs.
fn wait_for_refresh_server_port(lines: &mpsc::Receiver<String>, deadline: Duration) -> Option<u16> {
    let start = Instant::now();
    while start.elapsed() < deadline {
        match lines.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                if let Some(port) = line
                    .strip_prefix("refresh-server-up ")
                    .and_then(|rest| rest.trim().parse().ok())
                {
                    return Some(port);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => return None,
        }
    }
    None
}

/// The pin: a server program calls `force_refresh()`, and a connected fake
/// browser — the raw [`SseClient`] this file already has, standing in for the
/// dev channel's real audience — receives the `reload` event over the exact
/// wire path `dev-refresh.md` §5 item 2 describes: the watcher hands the
/// server `VILAN_HMR_PORT` (the node-child spawn site), the app POSTs
/// `/refresh`, and the channel broadcasts `reload`.
///
/// The doctrine half — `reload` fires ONCE and no `css`/`swap`/`connected`
/// event path ever calls `location.reload()` — is `hmr_shim.js`'s
/// `fetchAndSwap` comment, unchanged here: `css`'s never-reload is pinned by
/// `a_css_push_heals_a_boot_time_stale_server_route` (this file) and
/// `swap`/`connected`'s by `hmr_swap.rs`'s "heal" assertions (a stale
/// `connected` swaps, never reloads — the exact shape `fetchAndSwap` also
/// serves `swap` through). Both were verified non-vacuous by planting the
/// violation the doctrine forbids (making a version-gap `connected` call
/// `reload()` instead of swapping) and watching `hmr_swap.rs` go red, then
/// reverting — a manual check, not a standing test, since it plants a bug in
/// shipped code rather than in this fixture.
#[test]
fn force_refresh_reloads_a_connected_browser_once() {
    let dir = temp_project("force_refresh");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    write(&dir, "src/client.vl", &client_source("a", "x1"));
    write(&dir, "src/server.vl", &force_refresh_server_source());

    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", "--watch", "--hmr-port", "0", dir.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn run --watch");
    let lines = drain_both(
        watcher.stdout.take().unwrap(),
        watcher.stderr.take().unwrap(),
    );

    // E60: escapes the assertion closure so cleanup runs red or green.
    let refresh_server_port = std::cell::Cell::new(None::<u16>);

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let deadline = support::WATCH_LIVENESS;
        let dev_port = wait_for_port(&lines, deadline)
            .expect("the CLI should announce `hmr: dev channel on 127.0.0.1:<port>`");
        let server_port = wait_for_refresh_server_port(&lines, deadline)
            .expect("the force-refresh server should announce `refresh-server-up <port>`");
        refresh_server_port.set(Some(server_port));

        let token = dev_token(&dir, "client", deadline);
        let mut sse = SseClient::connect(dev_port, &token);

        // Trigger the server's route — it calls `force_refresh()`, which
        // POSTs `/refresh` on the dev channel it was handed over
        // `VILAN_HMR_PORT` at spawn.
        let body = http_get(server_port, "/trigger");
        assert_eq!(
            body,
            b"triggered".to_vec(),
            "the server route should have run (and called force_refresh on the way)"
        );

        // The connected fake browser sees the broadcast `reload` event.
        sse.expect_kind("reload", deadline);
    }));

    support::kill_watcher(&mut watcher);
    // The server survives the watcher by design (E60): ask it to exit and —
    // on the green path only, so a red run's own panic is never masked —
    // assert it actually died.
    if let Some(port) = refresh_server_port.get() {
        let start = Instant::now();
        let dead = loop {
            match std::net::TcpStream::connect(("127.0.0.1", port)) {
                Err(_) => break true,
                Ok(mut stream) => {
                    use std::io::Write;
                    let _ = stream.write_all(
                        b"GET /shutdown HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
                    );
                    if start.elapsed() > support::WATCH_LIVENESS {
                        break false;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        };
        if outcome.is_ok() {
            assert!(
                dead,
                "the force-refresh server must exit on /shutdown — \
                 an orphan here is E60's leak returning"
            );
        }
    }
    if outcome.is_ok() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    outcome.unwrap();
}

/// The other half of the pin: with no watch session — a plain `vilan run`,
/// so `VILAN_HMR_PORT` is never set — `force_refresh()` is a no-op. No dev
/// channel exists to POST to, so the only way this could fail is by hanging
/// or panicking; a clean exit with the program's own trailing print is the
/// whole assertion.
#[test]
fn force_refresh_is_a_no_op_outside_a_watch_session() {
    let dir = temp_project("force_refresh_noop");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    write(
        &dir,
        "src/main.vl",
        "import std::watch;\nimport std::io::print;\n\nfun main() {\n\twatch::force_refresh();\n\tprint(\"done\");\n}\n",
    );

    let liveness = support::run_liveness();
    let mut child = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", dir.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn vilan run");
    let deadline = Instant::now() + liveness;
    let status = loop {
        match child.try_wait().expect("poll vilan run") {
            Some(status) => break status,
            None if Instant::now() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "`vilan run` (no --watch, no VILAN_HMR_PORT) did not exit within {liveness:?} \
                     — force_refresh() should be a no-op, not a hang"
                );
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    };
    let mut stdout = String::new();
    let mut stderr = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut stdout)
        .unwrap();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(
        status.success(),
        "force_refresh() outside a watch session must not fail the program:\n{stdout}\n{stderr}"
    );
    assert_eq!(stdout.trim(), "done");

    let _ = std::fs::remove_dir_all(&dir);
}
