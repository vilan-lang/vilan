//! End-to-end test for the A13 swap protocol (hmr.md slice S2b): an HMR-active
//! `run --watch` produces an instrumented browser bundle (the shim plus the S2a
//! `__hmr_adopt*`/`__hmr_expose` emission), and `window.__VILAN_HMR__.swap(text)`
//! carries module state across an in-place re-evaluation. The corpus can't reach
//! this — it needs the shim runtime, a DOM, and a second (edited) bundle — so a
//! browser app is built through the real CLI and driven under node against a DOM
//! stub: state is mutated, a rebuilt bundle is swapped in, and the carry / reset
//! matrix is asserted (value + signal payload carried; excluded + fingerprint-
//! changed + function-local reset; `on_teardown` ran; `stash`/`take` round-trip;
//! the old bundle's subscriptions disposed). A102 adds the `lazy let` row of
//! that matrix, which needs a ROUND to say anything at all: a cell forced before
//! the swap carries its value and the new bundle's initializer never runs, and
//! one still pending carries nothing and is re-minted.
//!
//! The second e2e here is the same machinery pointed at the swap's OTHER
//! teardown (tracker A96): an app that dials a `SocketDuplex` before it mounts
//! is torn down socket-first, and what the duplex's own teardown does decides
//! whether the mount teardowns that follow talk to a socket that is already
//! closing — and whether the dead bundle's duplex redials and lives on beside
//! the new one's. It adds a WebSocket stub to the DOM stub, because a browser
//! answers a send on a closing socket with a console error rather than
//! anything a test can see.
//!
//! House process hygiene: the watcher never exits on its own, so it is killed at
//! the end; the legs are quick-exit (the node server prints and returns).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

mod support;

fn temp_project(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = support::scratch_root().join(format!(
        "vilan_hmr_swap_{tag}_{}_{unique}",
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

/// The dev-channel port from the activation line `hmr: dev channel on 127.0.0.1:<port>`.
fn parse_port(line: &str) -> Option<u16> {
    line.strip_prefix("hmr: dev channel on 127.0.0.1:")?
        .trim()
        .parse()
        .ok()
}

/// This run's dev-channel token (backlog E93), read from the instrumented
/// bundle in `dist/` — the same copy the browser gets. Bounded by `deadline`
/// because round 1 is what writes it.
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

/// A plain HTTP GET against the dev channel, returning the response BODY — or
/// `None` when no whole response arrived inside `timeout`. `token` is this
/// run's dev-channel token, which every route requires (backlog E93).
///
/// A partial read is never handed back (E39). The caller tells bundle B from
/// bundle A by inequality, and a truncated body differs from A exactly as a
/// rebuilt one does, so a read cut short by a loaded machine would masquerade
/// as a finished rebuild and hand the node harness half a bundle. The dev
/// channel closes each connection after responding, so a complete read ends at
/// EOF and the timeout is only ever reached by a stall.
fn http_get(port: u16, path: &str, token: &str, timeout: Duration) -> Option<Vec<u8>> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream.set_read_timeout(Some(timeout)).ok()?;
    write!(
        stream,
        "GET {path}?token={token} HTTP/1.1\r\nHost: localhost\r\n\r\n"
    )
    .ok()?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).ok()?;
    let separator = b"\r\n\r\n";
    let head_end = response
        .windows(separator.len())
        .position(|window| window == separator)?;
    Some(response[head_end + separator.len()..].to_vec())
}

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

/// Waits (bounded) for a stdout line containing `needle`. The Node server leg's
/// `print` output arrives here too — the child inherits the watcher's piped
/// stdout — which is what makes "round 1 has finished" an observable EVENT
/// rather than a guessed margin.
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

/// Waits (bounded) for the dev channel's activation line, returning its port.
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

/// The client app (bundle A). Module bindings span every transfer form — a `mut`
/// value (`count`), a `Signal` payload (`tally`), a plain-data value whose type
/// changes in bundle B (`cfg`, a fingerprint miss), and an excluded closure-holder
/// (`strap`). Each initializer and `main` mark a global counter so the harness can
/// tell carry (initializer NOT re-run) from fresh (re-run). `stash`/`take` and
/// `on_teardown` are exercised; a `tally.effect` under the mount root proves
/// subscription disposal.
const CLIENT_A: &str = r#"import std::ui::{ view, View, mount_root };
import std::reactive::{ Signal, SignalCell };
import std::dev;
import std::option::Option::{ self, Some, None };

[extern("globalThis.__mark")]
external fun mark(tag: str): void;

[extern("globalThis.__record")]
external fun record(tag: str, value: i32): void;

[extern("globalThis.__captureSignal")]
external fun capture_signal(tag: str, signal: SignalCell<i32>): void;

struct Cfg { a: i32 }

struct Strap { fire: || void }

fun seed_count(): i32 {
	mark("count-init");
	0
}

fun seed_cfg(): Cfg {
	mark("cfg-init");
	Cfg { a = 1 }
}

fun fresh_strap(): Strap {
	mark("strap-init");
	Strap { fire = || {} }
}

fun seed_motto(): i32 {
	mark("motto-init");
	41
}

fun seed_dormant(): i32 {
	mark("dormant-init");
	7
}

mut count = seed_count();
let tally: SignalCell<i32> = Signal::new(0);
let cfg: Cfg = seed_cfg();
let strap: Strap = fresh_strap();
lazy let motto: i32 = seed_motto();
lazy let dormant: i32 = seed_dormant();

fun bump() {
	count = count + 1;
	tally.set(tally.get() + 1);
	dev::stash("saved", count);
}

fun main() {
	mark("mount");
	let restored: Option<i32> = dev::take("saved");
	record("restored", restored.unwrap_or(-1));
	record("motto", motto);
	if restored is Some(_) {
		record("dormant", dormant);
	}
	record("cfg-a", cfg.a);
	(strap.fire)();
	dev::on_teardown(|| mark("teardown"));
	capture_signal("tally", tally);
	let _root = mount_root("app", || {
		tally.effect(|v| mark("tally-effect"));
		view("div").child(view("button").on("click", || bump()))
	});
}
"#;

/// Bundle B: `Cfg` gains a field, so `cfg`'s structural fingerprint changes and
/// it fresh-initializes (§4). Everything else is identical, so `count`/`tally`
/// carry and `strap` re-inits as the excluded form.
const CLIENT_B: &str = r#"import std::ui::{ view, View, mount_root };
import std::reactive::{ Signal, SignalCell };
import std::dev;
import std::option::Option::{ self, Some, None };

[extern("globalThis.__mark")]
external fun mark(tag: str): void;

[extern("globalThis.__record")]
external fun record(tag: str, value: i32): void;

[extern("globalThis.__captureSignal")]
external fun capture_signal(tag: str, signal: SignalCell<i32>): void;

struct Cfg { a: i32, b: i32 }

struct Strap { fire: || void }

fun seed_count(): i32 {
	mark("count-init");
	0
}

fun seed_cfg(): Cfg {
	mark("cfg-init");
	Cfg { a = 1, b = 2 }
}

fun fresh_strap(): Strap {
	mark("strap-init");
	Strap { fire = || {} }
}

fun seed_motto(): i32 {
	mark("motto-init");
	99
}

fun seed_dormant(): i32 {
	mark("dormant-init");
	8
}

mut count = seed_count();
let tally: SignalCell<i32> = Signal::new(0);
let cfg: Cfg = seed_cfg();
let strap: Strap = fresh_strap();
lazy let motto: i32 = seed_motto();
lazy let dormant: i32 = seed_dormant();

fun bump() {
	count = count + 1;
	tally.set(tally.get() + 1);
	dev::stash("saved", count);
}

fun main() {
	mark("mount");
	let restored: Option<i32> = dev::take("saved");
	record("restored", restored.unwrap_or(-1));
	record("motto", motto);
	if restored is Some(_) {
		record("dormant", dormant);
	}
	record("cfg-a", cfg.a);
	(strap.fire)();
	dev::on_teardown(|| mark("teardown"));
	capture_signal("tally", tally);
	let _root = mount_root("app", || {
		tally.effect(|v| mark("tally-effect"));
		view("div").child(view("button").on("click", || bump()))
	});
}
"#;

const SERVER: &str = "import std::io::print;\n\nfun main() {\n\tprint(\"server up\");\n}\n";

/// The DOM/host stub plus the swap-matrix assertions, run under node against the
/// two instrumented bundles. The DOM is the shared one (`support/dom/stub.js`,
/// N73) with this suite's host facts layered on it (`support/dom/hmr_swap.js`):
/// `window === globalThis` as in a browser, so the `__hmr_active` helper sees
/// the shim's singleton, and `Blob`/`URL.createObjectURL` stubbed to a `data:`
/// URL (node has no `blob:` loader — the sanctioned S2b fallback). One
/// `ok`/`FAIL` line per named assertion; exits 1 on any failure.
const HARNESS: &str = concat!(
    include_str!("support/dom/stub.js"),
    include_str!("support/dom/hmr_swap.js"),
    r#"
const marks = {};
const records = {};
const signals = {};
globalThis.__mark = (tag) => { marks[tag] = (marks[tag] || 0) + 1; };
globalThis.__record = (tag, value) => { records[tag] = value; };
globalThis.__captureSignal = (tag, signal) => { signals[tag] = signal; };

let failures = 0;
function check(condition, message) {
    if (condition) { console.log("ok   - " + message); }
    else { failures += 1; console.error("FAIL - " + message); }
}

await import("./bundleA.mjs");

check(marks["count-init"] === 1, "A: count initializer ran once");
check(marks["cfg-init"] === 1, "A: cfg initializer ran once");
check(marks["strap-init"] === 1, "A: strap initializer ran once");
check(marks["mount"] === 1, "A: main ran once");
check(records["restored"] === -1, "A: first-boot take() is None");
// A102: `motto` is read in main, so its cell is forced before the swap;
// `dormant` is read only on the restored path, so its cell is still pending.
check(marks["motto-init"] === 1, "A: the forced lazy binding initialized once");
check(records["motto"] === 41, "A: the forced lazy binding read its own value");
check(marks["dormant-init"] === undefined, "A: the unforced lazy binding never ran");
const tallyA = signals["tally"];
check(!!tallyA && tallyA[1].v.length === 1, "A: tally has one live subscriber");

const button = appRoot.find((element) => element.tagName === "button");
check(!!button, "A: a button mounted");
button.click();
button.click();
button.click();

const hmr = globalThis.window.__VILAN_HMR__;
check(hmr.exposed["pkg::count"].getter() === 3, "A: count mutated to 3");
check(hmr.exposed["pkg::tally"].getter() === 3, "A: tally payload mutated to 3");

// Read through a DYNAMIC import so this file carries no module-level `import`
// of its own: the harness is concatenated together from the shared stub, this
// suite's layer and the block you are reading, and a hoisted `import` two
// thirds of the way down the result reads as a mistake.
const { readFileSync } = await import("node:fs");
const bundleB = readFileSync(new URL("./bundleB.js", import.meta.url), "utf8");

// The heal path (the infinite-refresh regression): a `connected` event at our
// own version is a no-op; one ahead of us fetches the current bundle from the
// dev channel and SWAPS. location.reload() must never be involved — a page
// whose server serves a boot-time stale bundle would reload into the same
// stale bytes and loop forever.
globalThis.fetch = () => Promise.resolve({ text: () => Promise.resolve(bundleB) });
await hmr.handleEvent({ kind: "connected", version: hmr.version });
check(marks["teardown"] === undefined, "heal: an up-to-date connected does nothing");
const target = hmr.version + 1;
await hmr.handleEvent({ kind: "connected", version: target });
check(!globalThis.__reloaded, "heal: a stale connected swapped instead of reloading");
check(hmr.version === target, "heal: the singleton's version advanced to the channel's");

check(marks["teardown"] === 1, "swap: on_teardown ran");
check(marks["count-init"] === 1, "swap: count carried (adopt hit — initializer not re-run)");
check(marks["cfg-init"] === 2, "swap: cfg fingerprint changed → fresh init");
check(marks["strap-init"] === 2, "swap: excluded binding fresh init");
check(marks["mount"] === 2, "swap: main re-ran (function-local state reset)");
check(hmr.exposed["pkg::count"].getter() === 3, "swap: count value carried");
check(hmr.exposed["pkg::tally"].getter() === 3, "swap: signal payload carried");
check(records["restored"] === 3, "swap: stash/take round-tripped");
// A102 (R13), both halves. `motto` was `done` before the swap, so its VALUE
// crosses and the new bundle's cell starts forced — bundle B's initializer
// would have answered 99 and never runs. `dormant` was `pending`, so it
// carries nothing: bundle B mints it fresh and the read on the restored path
// runs B's initializer for the first time, answering B's 8.
check(marks["motto-init"] === 1, "swap: the forced lazy binding carried (initializer not re-run)");
check(records["motto"] === 41, "swap: the carried lazy value is the old bundle's, not the new initializer's");
check(marks["dormant-init"] === 1, "swap: the pending lazy binding re-minted and forced fresh");
check(records["dormant"] === 8, "swap: the re-minted lazy binding ran the NEW bundle's initializer");
check(tallyA[1].v.length === 0, "swap: bundle A subscription disposed (old subscribers dead)");
check(!globalThis.__reloaded, "swap: completed without a fallback reload");

process.exit(failures === 0 ? 0 : 1);
"#
);

#[test]
fn the_swap_protocol_carries_state_across_a_rebuilt_bundle() {
    let dir = temp_project("carry");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"swapapp\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    write(&dir, "src/client.vl", CLIENT_A);
    write(&dir, "src/server.vl", SERVER);
    write(&dir, "harness.mjs", HARNESS);

    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", "--watch", "--hmr-port", "0", dir.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn run --watch");

    // Every line, not just the port: the Node server leg inherits the watcher's
    // stdout, so its boot marker arrives here and makes "round 1 has finished"
    // an event this test can wait ON rather than wait OUT.
    let stdout = watcher.stdout.take().unwrap();
    let (sender, lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = sender.send(line);
        }
    });

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let port = wait_for_port(&lines, support::WATCH_LIVENESS)
            .expect("the CLI should announce `hmr: dev channel on 127.0.0.1:<port>`");

        // Round 1 is over once `dist/` has landed AND the server leg has printed
        // its boot line. What stood here was a fixed 800 ms margin, "so the
        // watcher's baseline snapshot is taken before the edit": that margin
        // belonged to a watcher that snapshotted AFTER its first build, and E20
        // moved the snapshot in front of the build precisely so an edit could no
        // longer vanish into it — the margin has been paying for a fixed bug.
        //
        // Round 1 is also the measurement everything after it is budgeted from
        // (E39): a full compile of both legs on this machine, under this load,
        // timed rather than guessed. Its own wait is a liveness bound, because
        // nothing here asserts how fast a compile is.
        let round_one_started = Instant::now();
        assert!(
            wait_for_file(&dir.join("dist/client.js"), support::WATCH_LIVENESS),
            "round 1 should have written dist/client.js"
        );
        assert!(
            wait_for_line(&lines, "server up", support::WATCH_LIVENESS),
            "round 1 should have booted the server leg"
        );
        let round_one = round_one_started.elapsed();
        let budget = support::round_budget(round_one);
        let token = dev_token(&dir, "client", support::WATCH_LIVENESS);

        // Bundle A: the instrumented client (shim + adopt/expose).
        let bundle_a = http_get(port, "/bundle/client.js", &token, budget)
            .expect("the dev channel should serve bundle A whole");
        assert!(
            String::from_utf8_lossy(&bundle_a).contains("__hmr_adopt"),
            "bundle A should carry the S2a adopt instrumentation"
        );
        std::fs::write(dir.join("bundleA.mjs"), &bundle_a).unwrap();

        // Edit the client → bundle B (cfg's type changes), then poll the served
        // bundle until it differs from A. One round's worth of budget, measured
        // off round 1: this rebuild compiles ONE leg where round 1 compiled two,
        // so anything near that multiple is a watcher that stopped reacting.
        write(&dir, "src/client.vl", CLIENT_B);
        let start = Instant::now();
        let bundle_b = loop {
            if let Some(current) = http_get(port, "/bundle/client.js", &token, budget)
                && current != bundle_a
                && current.contains(&b'{')
            {
                break current;
            }
            assert!(
                start.elapsed() < budget,
                "the edited client should rebuild into a new bundle within {budget:?} \
                 (round 1 itself took {round_one:?})"
            );
            std::thread::sleep(Duration::from_millis(200));
        };
        std::fs::write(dir.join("bundleB.js"), &bundle_b).unwrap();

        // Drive the swap under node against the DOM stub.
        let run = Command::new("node")
            .arg("harness.mjs")
            .current_dir(&dir)
            .output()
            .expect("run node harness");
        assert!(
            run.status.success(),
            "swap harness failed:\n{}\n{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        );
    }));

    support::kill_watcher(&mut watcher);
    if outcome.is_ok() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    outcome.unwrap();
}

// --- A96: the swap's socket teardown ----------------------------------------

/// A browser client that DIALS before it mounts — kolt's shape (`client.vl`:47
/// then :60) and the one A96 was filed against: the duplex's HMR teardown is
/// registered at dial, so the swap runs it BEFORE the mount teardown that
/// releases every mirror's lease. Two `RemoteSource`s, each watched under the
/// root owner, so the disposal that follows the socket's teardown owes two
/// `Unsubscribe` frames. `@@TAG@@` is the bundle's identity: bundle B is this
/// same program with the other tag, so every mark says which bundle made it and
/// the rebuilt bytes differ from A's.
const SOCKET_CLIENT: &str = r#"import std::ui::{ view, View, mount_root };
import std::rpc::{ ReactiveClient, RemoteSource, bridge, connect_socket };
import std::json::json_codec;
import std::option::Option::{ self, Some, None };
import std::result::Result::{ self, Ok, Err };

[extern("globalThis.__mark")]
external fun mark(tag: str): void;

[extern("globalThis.__record")]
external fun record(tag: str, value: i32): void;

fun main() {
	mark("@@TAG@@:main");
	match connect_socket("ws://127.0.0.1:1/") {
		Err(let _reason) => mark("@@TAG@@:dial-failed"),
		Ok(let socket) => {
			// Outside the mount, so the swap's teardown cannot dispose it: the
			// old duplex's state is still observable after the bundle is gone.
			let _states = socket.state.on_change(|state| mark(i"@@TAG@@:state:{state.debug()}"));
			let client = ReactiveClient::new(bridge(socket), json_codec());
			let first: RemoteSource<i32> = client.source(7);
			let second: RemoteSource<i32> = client.source(8);
			let _root = mount_root("app", || {
				first.effect(|value| record("@@TAG@@:first", value.unwrap_or(0 - 1)));
				second.effect(|value| record("@@TAG@@:second", value.unwrap_or(0 - 1)));
				view("div")
			});
			mark("@@TAG@@:ready");
		},
	}
}
"#;

fn socket_client_source(tag: &str) -> String {
    SOCKET_CLIENT.replace("@@TAG@@", tag)
}

/// The A96 harness: the swap protocol driven against a WebSocket stub that
/// answers a `Subscribe` with an `Update` and REFUSES to transmit on a socket
/// that is not OPEN — which is the browser's own behaviour, where such a send
/// is the console error "WebSocket is already in CLOSING or CLOSED state" and
/// the frame never leaves. The stub records each one instead of printing it, so
/// the errors A96 was filed for are assertable here.
///
/// A real server is not needed and would not help: what is under test is what
/// the CLIENT does to its own socket between the teardown and the new bundle's
/// dial, and a stub is the only way to see a frame that a browser would drop.
const SOCKET_HARNESS: &str = r#"import fs from "node:fs";

class StubElement {
    constructor(tag) {
        this.tagName = tag;
        this.children = [];
        this.parent = null;
        this.listeners = {};
        this._text = "";
        this.attributes = {};
        this.style = { setProperty: () => {} };
        this.hidden = false;
    }
    set textContent(text) { this._text = text; this.children = []; }
    get textContent() { return this._text; }
    setAttribute(name, value) { this.attributes[name] = value; }
    appendChild(child) {
        if (child.parent) child.parent.children = child.parent.children.filter((c) => c !== child);
        child.parent = this;
        this.children.push(child);
    }
    remove() {
        if (this.parent) {
            this.parent.children = this.parent.children.filter((c) => c !== this);
            this.parent = null;
        }
    }
    replaceChildren() { for (const c of this.children) c.parent = null; this.children = []; }
    addEventListener(event, handler) {
        (this.listeners[event] = this.listeners[event] || []).push(handler);
    }
}

const appRoot = new StubElement("div");
globalThis.window = globalThis; // window === globalThis, as in a browser
globalThis.document = {
    createElement: (tag) => new StubElement(tag),
    getElementById: (id) => (id === "app" ? appRoot : null),
    querySelector: () => null,
    querySelectorAll: () => [],
};
globalThis.location = { reload: () => { globalThis.__reloaded = true; } };
globalThis.Blob = class {
    constructor(parts) { this.__text = parts.join(""); }
};
URL.createObjectURL = (blob) =>
    "data:text/javascript;base64," + Buffer.from(blob.__text).toString("base64");
URL.revokeObjectURL = () => {};

// The socket spy. `readyState` follows the browser's ladder (0 CONNECTING,
// 1 OPEN, 2 CLOSING, 3 CLOSED) because the whole defect lives in the window
// between 2 and 3: `close()` is synchronous, `onclose` is not.
const sockets = [];
const lateTransmits = [];
let announced = 0;

class StubSocket {
    constructor(url, protocols) {
        this.url = url;
        this.protocols = protocols;
        this.readyState = 0;
        this.sent = [];
        this.index = sockets.length;
        sockets.push(this);
        queueMicrotask(() => {
            this.readyState = 1;
            announced += 1;
            // The server's connection announcement — what resolves the dial.
            if (this.onmessage) this.onmessage({ data: "__conn:" + announced });
        });
    }
    send(payload) {
        const frame = String(payload);
        if (this.readyState !== 1) {
            lateTransmits.push({ socket: this.index, readyState: this.readyState, frame });
            return;
        }
        this.sent.push(frame);
        if (!frame.startsWith("d:")) return;
        const subscribe = /^\{"Subscribe":\[(\d+),null\]\}$/.exec(frame.slice(2));
        if (!subscribe) return;
        // The reactive server's seeding answer: channel N holds N * 100.
        const channel = Number(subscribe[1]);
        queueMicrotask(() => {
            if (this.readyState === 1 && this.onmessage) {
                this.onmessage({ data: 'd:{"Update":[' + channel + "," + channel * 100 + "]}" });
            }
        });
    }
    close() {
        if (this.readyState === 3) return;
        this.readyState = 2;
        queueMicrotask(() => {
            this.readyState = 3;
            if (this.onclose) this.onclose();
        });
    }
}
globalThis.WebSocket = StubSocket;

const marks = {};
const records = {};
globalThis.__mark = (tag) => { marks[tag] = (marks[tag] || 0) + 1; };
globalThis.__record = (tag, value) => { records[tag] = value; };

let failures = 0;
function check(condition, message) {
    if (condition) { console.log("ok   - " + message); }
    else { failures += 1; console.error("FAIL - " + message); }
}
async function settle(ticks = 10) {
    for (let index = 0; index < ticks; index++) {
        await new Promise((resolve) => setTimeout(resolve, 0));
    }
}

await import("./bundleA.mjs");
await settle();

check(sockets.length === 1, "A: one socket dialed");
check(marks["A:ready"] === 1, "A: the app dialed, mounted and reached ready");
check(records["A:first"] === 700, "A: the first mirror seeded from the server");
check(records["A:second"] === 800, "A: the second mirror seeded from the server");
check(lateTransmits.length === 0, "A: nothing transmitted on a socket that was not OPEN");

const bundleB = fs.readFileSync(new URL("./bundleB.js", import.meta.url), "utf8");
globalThis.fetch = () => Promise.resolve({ text: () => Promise.resolve(bundleB) });

const hmr = globalThis.window.__VILAN_HMR__;
await hmr.handleEvent({ kind: "connected", version: hmr.version + 1 });
await settle();

// (a) The console errors themselves: one per remote source, on the socket the
// teardown had already put into CLOSING.
check(lateTransmits.length === 0, "swap: no transmit on a socket that is not OPEN");
// (b) The old duplex is closed for good, so `onclose` finds a state that stops
// it: no rejection wave, no redial.
check(marks["A:state:Closed"] === 1, "swap: the old duplex reached Closed");
check(marks["A:state:Reconnecting"] === undefined, "swap: the old duplex never went Reconnecting");
// The dial counter: bundle B's dial and nothing else.
check(sockets.length === 2, "swap: exactly one fresh dial (the new bundle's)");
// (c) One connection left standing, not the zombie pair.
check(
    sockets.filter((socket) => socket.readyState === 1).length === 1,
    "swap: one live connection after the swap settled",
);
// (d) And the new bundle's mirror is live on it (the K6 resync path).
check(records["B:first"] === 700, "swap: the new bundle's mirror resynced");
check(records["B:second"] === 800, "swap: the new bundle's second mirror resynced");
check(!globalThis.__reloaded, "swap: completed without a fallback reload");

if (failures > 0) {
    console.error("late transmits: " + JSON.stringify(lateTransmits));
    console.error("marks: " + JSON.stringify(marks));
    console.error("records: " + JSON.stringify(records));
    console.error(
        "sockets: " +
            JSON.stringify(
                sockets.map((socket) => ({
                    index: socket.index,
                    readyState: socket.readyState,
                    sent: socket.sent,
                })),
            ),
    );
}
process.exit(failures === 0 ? 0 : 1);
"#;

/// A96: the swap must not talk to the socket it just closed, and must not leave
/// the dead bundle's duplex redialing beside the new one's.
///
/// The teardown list runs in REGISTRATION order and the duplex registers at
/// DIAL, before the mount that owns the mirrors — so a teardown that only
/// called `socket.close()` left the duplex reading `Connected` while the
/// browser socket was CLOSING, and every lease released by the mount teardown
/// that followed sent its `Unsubscribe` straight through `send`'s state guard
/// into a `transmit` the browser answers with a console error. Then `onclose`
/// found `Connected` too: rejection wave, backoff redial, and the old bundle's
/// connection alive for the rest of the session. `close_for_good` writes
/// `Closed` first, which makes both halves stop at a guard they already had.
///
/// Driven under the same node/DOM stub as the carry matrix above, plus a
/// WebSocket stub that records a send on a non-OPEN socket rather than printing
/// it — the browser's console error, made assertable.
#[test]
fn a_swap_closes_the_old_duplex_instead_of_talking_to_a_closing_socket() {
    let dir = temp_project("duplex");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"swapapp\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    write(&dir, "src/client.vl", &socket_client_source("A"));
    write(&dir, "src/server.vl", SERVER);
    write(&dir, "harness.mjs", SOCKET_HARNESS);

    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", "--watch", "--hmr-port", "0", dir.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn run --watch");

    let stdout = watcher.stdout.take().unwrap();
    let (sender, lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = sender.send(line);
        }
    });

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let port = wait_for_port(&lines, support::WATCH_LIVENESS)
            .expect("the CLI should announce `hmr: dev channel on 127.0.0.1:<port>`");

        // Round 1 is over once `dist/` has landed AND the server leg printed its
        // boot line — the same event pair (and the same budget derivation) the
        // carry matrix waits on.
        let round_one_started = Instant::now();
        assert!(
            wait_for_file(&dir.join("dist/client.js"), support::WATCH_LIVENESS),
            "round 1 should have written dist/client.js"
        );
        assert!(
            wait_for_line(&lines, "server up", support::WATCH_LIVENESS),
            "round 1 should have booted the server leg"
        );
        let round_one = round_one_started.elapsed();
        let budget = support::round_budget(round_one);
        let token = dev_token(&dir, "client", support::WATCH_LIVENESS);

        let bundle_a = http_get(port, "/bundle/client.js", &token, budget)
            .expect("the dev channel should serve bundle A whole");
        assert!(
            String::from_utf8_lossy(&bundle_a).contains("__hmr_register_teardown"),
            "bundle A should carry the shim's teardown registry"
        );
        std::fs::write(dir.join("bundleA.mjs"), &bundle_a).unwrap();

        write(&dir, "src/client.vl", &socket_client_source("B"));
        let start = Instant::now();
        let bundle_b = loop {
            if let Some(current) = http_get(port, "/bundle/client.js", &token, budget)
                && current != bundle_a
                && current.contains(&b'{')
            {
                break current;
            }
            assert!(
                start.elapsed() < budget,
                "the edited client should rebuild into a new bundle within {budget:?} \
                 (round 1 itself took {round_one:?})"
            );
            std::thread::sleep(Duration::from_millis(200));
        };
        std::fs::write(dir.join("bundleB.js"), &bundle_b).unwrap();

        let run = Command::new("node")
            .arg("harness.mjs")
            .current_dir(&dir)
            .output()
            .expect("run node harness");
        assert!(
            run.status.success(),
            "duplex teardown harness failed:\n{}\n{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr),
        );
    }));

    support::kill_watcher(&mut watcher);
    if outcome.is_ok() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    outcome.unwrap();
}

// --- A119: a USER-WRITTEN `Slot` impl across a swap -------------------------
//
// kolt's `lib/conditional_value.vl` carries the comment `BUG: This breaks on
// HMR` over a hand-rolled `Slot` impl — `when_value`, the conditional form that
// binds the `Some` payload, built on a `Region`, a fresh `Owner` per
// instantiation and `get_owner().defer(..)`. A119 replaces that file with a std
// `when_some`, and the item asks the question ahead of the replacement: if a
// USER `Slot` impl cannot survive an HMR round, that is a defect of its own and
// not something a std form would fix, so it needs its own repro before any of
// it is believed.
//
// This is that repro, and it is kolt's code: the impl below is
// `conditional_value.vl` transcribed, with two marks added (the deferred
// cleanup, and `main`) and the B369 workaround kolt carries — the constructor's
// return type is WRITTEN, because an inferred one loses `C` and reaches an
// internal error. One round: build the row, take it away, swap, and build it
// again from the NEW bundle.
const USER_SLOT_CLIENT: &str = r#"import std::option::Option::{ self, None, Some };
import std::reactive::{
	Owner,
	Signal,
	SignalCell,
	Source,
	get_owner,
	owner_scope,
	run_with_owner,
};
import std::shared::Shared;
import std::ui::{ Region, Row, Slot, View, mount_root, view };

[extern("globalThis.__mark")]
external fun mark(tag: str): void;

/// The return type is WRITTEN, not inferred: left to inference, a generic
/// constructor like this one loses `C` and the compiler dies with an internal
/// error where `place` is reached (tracker B369). kolt's own comment.
fun when_value<T, S: Source<Option<T>>, C: Slot>(
	condition: S,
	body: (|T| C) context owner_scope,
): ConditionalValue<T, S, C> {
	ConditionalValue { condition, body }
}

struct ConditionalValue<T, S: Source<Option<T>>, C: Slot> {
	condition: S,
	body: (|T| C) context owner_scope,
}

impl ConditionalValue<type T, type S: Source<Option<T>>, type C: Slot> with Slot {
	fun place(self, parent: View) {
		let region = Region::open(parent);
		let live_row: Shared<Option<Row>> = Shared::new(None);
		let live_owner: Shared<Option<Owner>> = Shared::new(None);
		get_owner().defer(|| {
			mark("@@TAG@@:defer");
			live_owner.read()?.dispose();
			region.close();
		});
		self.condition.effect(|on| {
			if live_row.read() is Some(let row) {
				live_owner.read()?.dispose();
				let _cut = region.cut_row(row, region.anchor);
				region.drop_row(row);
				region.hold_rows([]);
				live_row.write() = None;
				live_owner.write() = None;
			}
			if on is Some(let value) {
				let owner = Owner::new();
				let row = run_with_owner(owner, || region.open_row((self.body)(value)));
				region.hold_rows([row]);
				live_row.write() = Some(row);
				live_owner.write() = Some(owner);
			}
		});
	}
}

let picked: SignalCell<Option<str>> = Signal::new(None);

fun pick() {
	picked.set(Some("@@TAG@@"));
}

fun drop_pick() {
	picked.set(None);
}

fun main() {
	mark("@@TAG@@:main");
	let _root = mount_root("app", || {
		view("div")
			.child(view("button").on("click", || pick()))
			.child(view("button").on("click", || drop_pick()))
			.child(when_value(picked, |value: str| view("p").text(i"row:{value}")))
	});
}
"#;

fn user_slot_client_source(tag: &str) -> String {
    USER_SLOT_CLIENT.replace("@@TAG@@", tag)
}

/// The same app over std's `when_some` — the shape kolt's file becomes. Every
/// mark the harness reads is in the same place, so the two run under one
/// harness: the form's own cleanup is registered by `place_when_some` rather
/// than by a user body, so the `defer` mark moves to the row body's disposal,
/// which is where a std form can be observed from an app at all.
const WHEN_SOME_CLIENT: &str = r#"import std::option::Option::{ self, None, Some };
import std::reactive::{ Signal, SignalCell, get_owner };
import std::ui::{ View, mount_root, view, when_some };

[extern("globalThis.__mark")]
external fun mark(tag: str): void;

let picked: SignalCell<Option<str>> = Signal::new(None);

fun pick() {
	picked.set(Some("@@TAG@@"));
}

fun drop_pick() {
	picked.set(None);
}

fun main() {
	mark("@@TAG@@:main");
	let _root = mount_root("app", || {
		view("div")
			.child(view("button").on("click", || pick()))
			.child(view("button").on("click", || drop_pick()))
			.child(when_some(picked, |value| {
				get_owner().defer(|| mark("@@TAG@@:defer"));
				view("p").bind_text(value.map(|current| i"row:{current}"))
			}))
	});
}
"#;

fn when_some_client_source(tag: &str) -> String {
    WHEN_SOME_CLIENT.replace("@@TAG@@", tag)
}

/// The round, asserted on the page rather than on the protocol: what a user
/// `Slot` impl owes across a swap is that its row leaves with the old bundle
/// and that the new bundle's impl builds one of its own.
const USER_SLOT_HARNESS: &str = concat!(
    include_str!("support/dom/stub.js"),
    include_str!("support/dom/hmr_swap.js"),
    r#"
const marks = {};
globalThis.__mark = (tag) => { marks[tag] = (marks[tag] || 0) + 1; };

let failures = 0;
function check(condition, message) {
    if (condition) { console.log("ok   - " + message); }
    else { failures += 1; console.error("FAIL - " + message); }
}
const rows = () => appRoot.findAll((element) => element.tagName === "p");
const buttons = () => appRoot.findAll((element) => element.tagName === "button");

await import("./bundleA.mjs");
check(marks["A:main"] === 1, "A: main ran once");
check(buttons().length === 2, "A: the two buttons mounted");
check(rows().length === 0, "A: a `None` builds no row");

buttons()[0].click();
check(rows().length === 1 && rows()[0].textContent === "row:A",
    "A: a `Some` builds the row, with the payload in hand");
buttons()[1].click();
check(rows().length === 0, "A: a `None` takes it away again");

// The signal is module state and CARRIES, so the selection is cleared before
// the swap on purpose: a `Some` crossing the boundary would have the new
// bundle's impl rebuild the OLD bundle's payload, which is a question about the
// carry matrix and not about the impl.
const hmr = globalThis.window.__VILAN_HMR__;
const { readFileSync } = await import("node:fs");
const bundleB = readFileSync(new URL("./bundleB.js", import.meta.url), "utf8");
globalThis.fetch = () => Promise.resolve({ text: () => Promise.resolve(bundleB) });
await hmr.handleEvent({ kind: "connected", version: hmr.version + 1 });

check(marks["A:defer"] === 1,
    "swap: the impl's own deferred cleanup ran with the old root owner");
check(marks["B:main"] === 1, "swap: the new bundle mounted");
check(buttons().length === 2,
    "swap: exactly the new bundle's buttons are in the page");
check(rows().length === 0, "swap: no row is left over");

buttons()[0].click();
check(rows().length === 1 && rows()[0].textContent === "row:B",
    "swap: the NEW bundle's impl builds its row, once");
check(!globalThis.__reloaded, "swap: completed without a fallback reload");

process.exit(failures === 0 ? 0 : 1);
"#
);

/// The same round with the row LIVE across the swap: the selection is a module
/// `let` holding a signal, so its payload carries, and the new bundle's impl
/// has to instantiate from it during its own mount.
const USER_SLOT_LIVE_HARNESS: &str = concat!(
    include_str!("support/dom/stub.js"),
    include_str!("support/dom/hmr_swap.js"),
    r#"
const marks = {};
globalThis.__mark = (tag) => { marks[tag] = (marks[tag] || 0) + 1; };

let failures = 0;
function check(condition, message) {
    if (condition) { console.log("ok   - " + message); }
    else { failures += 1; console.error("FAIL - " + message); }
}
const rows = () => appRoot.findAll((element) => element.tagName === "p");
const buttons = () => appRoot.findAll((element) => element.tagName === "button");

await import("./bundleA.mjs");
buttons()[0].click();
check(rows().length === 1 && rows()[0].textContent === "row:A",
    "A: the row is live going into the swap");

const hmr = globalThis.window.__VILAN_HMR__;
const { readFileSync } = await import("node:fs");
const bundleB = readFileSync(new URL("./bundleB.js", import.meta.url), "utf8");
globalThis.fetch = () => Promise.resolve({ text: () => Promise.resolve(bundleB) });
await hmr.handleEvent({ kind: "connected", version: hmr.version + 1 });

check(marks["A:defer"] === 1, "swap: the live instantiation's cleanup ran");
check(marks["B:main"] === 1, "swap: the new bundle mounted");
check(buttons().length === 2, "swap: exactly the new bundle's buttons are in the page");
// ONE row: the carried `Some` is rebuilt by B's impl, and A's is gone with A.
check(rows().length === 1, "swap: exactly one row stands, not two");
check(rows()[0].textContent === "row:A",
    "swap: and it carries the payload the signal crossed with");

buttons()[1].click();
check(rows().length === 0, "swap: the new bundle's impl can take it away");
buttons()[0].click();
check(rows().length === 1 && rows()[0].textContent === "row:B",
    "swap: and put its own back");
check(!globalThis.__reloaded, "swap: completed without a fallback reload");

process.exit(failures === 0 ? 0 : 1);
"#
);

/// The round both A119 tests drive: bundle A is built and served by a real
/// `run --watch`, the client is edited to bundle B, and the named harness runs
/// the swap under node against the DOM stub. One function rather than a third
/// copy of the drive, because the only thing the two cases differ in is what
/// the harness asserts.
fn drive_user_slot_round(tag: &str, client: &dyn Fn(&str) -> String, harness: &str) {
    let dir = temp_project(tag);
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"swapapp\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    write(&dir, "src/client.vl", &client("A"));
    write(&dir, "src/server.vl", SERVER);
    write(&dir, "harness.mjs", harness);

    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", "--watch", "--hmr-port", "0", dir.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn run --watch");

    let stdout = watcher.stdout.take().unwrap();
    let (sender, lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = sender.send(line);
        }
    });

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let port = wait_for_port(&lines, support::WATCH_LIVENESS)
            .expect("the CLI should announce `hmr: dev channel on 127.0.0.1:<port>`");

        let round_one_started = Instant::now();
        assert!(
            wait_for_file(&dir.join("dist/client.js"), support::WATCH_LIVENESS),
            "round 1 should have written dist/client.js"
        );
        assert!(
            wait_for_line(&lines, "server up", support::WATCH_LIVENESS),
            "round 1 should have booted the server leg"
        );
        let round_one = round_one_started.elapsed();
        let budget = support::round_budget(round_one);
        let token = dev_token(&dir, "client", support::WATCH_LIVENESS);

        let bundle_a = http_get(port, "/bundle/client.js", &token, budget)
            .expect("the dev channel should serve bundle A whole");
        assert!(
            String::from_utf8_lossy(&bundle_a).contains("__hmr_adopt"),
            "bundle A should carry the S2a adopt instrumentation"
        );
        std::fs::write(dir.join("bundleA.mjs"), &bundle_a).unwrap();

        write(&dir, "src/client.vl", &client("B"));
        let start = Instant::now();
        let bundle_b = loop {
            if let Some(current) = http_get(port, "/bundle/client.js", &token, budget)
                && current != bundle_a
                && current.contains(&b'{')
            {
                break current;
            }
            assert!(
                start.elapsed() < budget,
                "the edited client should rebuild into a new bundle within {budget:?} \
                 (round 1 itself took {round_one:?})"
            );
            std::thread::sleep(Duration::from_millis(200));
        };
        std::fs::write(dir.join("bundleB.js"), &bundle_b).unwrap();

        let run = Command::new("node")
            .arg("harness.mjs")
            .current_dir(&dir)
            .output()
            .expect("run node harness");
        assert!(
            run.status.success(),
            "the conditional-form swap harness failed:\n{}\n{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr),
        );
    }));

    support::kill_watcher(&mut watcher);
    if outcome.is_ok() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    outcome.unwrap();
}

/// The round with the selection CLEARED before the swap: the impl's cleanup
/// runs with the old root, nothing is left in the page, and the new bundle's
/// impl builds a row of its own.
#[test]
fn a119_a_user_written_slot_impl_survives_a_swap() {
    drive_user_slot_round("user_slot", &user_slot_client_source, USER_SLOT_HARNESS);
}

/// And the round with the selection HELD across it, which is the shape an
/// author actually edits code in: the payload is module state and carries, so
/// the NEW bundle's impl instantiates from the OLD bundle's value. One row,
/// built once, by B.
#[test]
fn a119_a_user_written_slot_impl_survives_a_swap_with_its_row_live() {
    drive_user_slot_round(
        "user_slot_live",
        &user_slot_client_source,
        USER_SLOT_LIVE_HARNESS,
    );
}

/// And the round over std's OWN form, which is what kolt's file becomes: the
/// same two assertions against `when_some`, so the replacement is held to the
/// standard the hand-written impl was measured against rather than assumed to
/// be at least as good.
#[test]
fn a119_a_when_some_row_survives_a_swap_with_its_row_live() {
    drive_user_slot_round(
        "when_some_live",
        &when_some_client_source,
        USER_SLOT_LIVE_HARNESS,
    );
}
