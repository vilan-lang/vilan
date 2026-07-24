//! The full-stack SSR proof (proposal/ssr.md §1, §4 S2): the `examples/ssr`
//! workspace, built and run end to end. Two phases over one build:
//!
//! 1. **Server render.** `node dist/server.js` serves the page; a GET asserts the
//!    served HTML carries the RENDERED content — the signal-fed list items, the
//!    escaped heading, the `when` branch, the read-once button — spliced into the
//!    shell at the `<!--ssr-->` marker, all present BEFORE any client JS runs.
//! 2. **Client replace.** The built `dist/client.js` is driven under the A10 DOM
//!    stub against a container PRE-POPULATED with the server-rendered nodes (a
//!    simulated SSR page). Booting the bundle must REPLACE the container: the old
//!    server nodes detached, the live UI mounted in their place, a signal write
//!    (a button click) propagating to the NEW tree, and the old nodes receiving
//!    nothing. Render, then replace — no adoption (proposal/ssr.md §1 step 3).
//!
//! The pre-populated container is a HAND-BUILT mirror of `render(app())`'s output,
//! not a real HTML parse: the stub has no parser, and the replace path only needs
//! the container non-empty with foreign nodes the client did not build. The node
//! texts mirror the server markup exactly, and phase 1 has already asserted that
//! same markup is what the server serves — so the simulation is faithful to the
//! page the browser would parse.
//!
//! House process hygiene: the server never exits on its own, so it is killed at
//! the end (inside a `catch_unwind` so a failed assertion still tears it down);
//! the client leg is a quick-exit node run. The example is copied to a temp dir
//! and given a free port, so the test is hermetic and parallel-safe.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

fn temp_project(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("vilan_ssr_{tag}_{}_{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// The in-repo `examples/ssr` workspace (this crate is `crates/vilan-cli`).
fn example_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../vilan/examples/ssr")
        .canonicalize()
        .expect("locate examples/ssr")
}

/// Copy a directory tree, skipping build artifacts (`dist`) so the temp copy
/// builds clean.
fn copy_tree(source: &Path, destination: &Path) {
    std::fs::create_dir_all(destination).unwrap();
    for entry in std::fs::read_dir(source).unwrap().flatten() {
        let name = entry.file_name();
        if name == "dist" {
            continue;
        }
        let from = entry.path();
        let to = destination.join(&name);
        if from.is_dir() {
            copy_tree(&from, &to);
        } else {
            std::fs::copy(&from, &to).unwrap();
        }
    }
}

fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// Bind an ephemeral port, then release it — a free port for the server (a small
/// TOCTOU window, standard for this kind of test).
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn build(dir: &Path) {
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", dir.to_str().unwrap()])
        .output()
        .expect("run vilan build");
    assert!(
        output.status.success(),
        "vilan build failed for {}:\n{}\n{}",
        dir.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Poll until the server accepts a connection (or the deadline passes).
fn wait_for_port(port: u16, deadline: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// A plain HTTP GET, returning the response body bytes.
fn http_get(port: u16, path: &str) -> Vec<u8> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect for GET");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .expect("send GET");
    let mut response = Vec::new();
    let _ = stream.read_to_end(&mut response);
    let separator = b"\r\n\r\n";
    match response
        .windows(separator.len())
        .position(|window| window == separator)
    {
        Some(index) => response[index + separator.len()..].to_vec(),
        None => response,
    }
}

/// The A10 DOM stub plus the replace-matrix assertions, run under node against the
/// built `dist/client.js` (passed as argv[2]). One `ok`/`FAIL` line per assertion;
/// exits 1 on any failure.
const BOOT_HARNESS: &str = r#"class StubElement {
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
    set className(v) { this.attributes["class"] = v; }
    get className() { return this.attributes["class"] || ""; }
    setAttribute(name, value) { this.attributes[name] = value; }
    set value(v) { this.attributes["value"] = v; }
    get value() { return this.attributes["value"] || ""; }
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
    addEventListener(event, handler) { (this.listeners[event] = this.listeners[event] || []).push(handler); }
    click() { for (const h of (this.listeners.click || [])) h({ preventDefault() {} }); }
    find(predicate) {
        if (predicate(this)) return this;
        for (const c of this.children) { const hit = c.find(predicate); if (hit) return hit; }
        return null;
    }
    findAll(predicate, acc) {
        acc = acc || [];
        if (predicate(this)) acc.push(this);
        for (const c of this.children) c.findAll(predicate, acc);
        return acc;
    }
}

let failures = 0;
function check(condition, message) {
    if (condition) console.log("ok   - " + message);
    else { failures += 1; console.error("FAIL - " + message); }
}

// Simulate the SSR page: the browser parsed the served markup into #app. A
// hand-built mirror of render(app())'s output — the stub has no HTML parser, and
// the replace path only needs the container non-empty with foreign nodes the
// client did not build. The texts mirror the server markup phase 1 asserted.
const container = new StubElement("div");
const serverMain = new StubElement("main");
serverMain.className = "app";
const serverList = new StubElement("ul");
const serverLi1 = new StubElement("li"); serverLi1.textContent = "Render on the server";
const serverLi2 = new StubElement("li"); serverLi2.textContent = "Replace on boot";
serverList.appendChild(serverLi1);
serverList.appendChild(serverLi2);
const serverButton = new StubElement("button"); serverButton.textContent = "idle";
serverMain.appendChild(serverList);
serverMain.appendChild(serverButton);
container.appendChild(serverMain);

// Pre-boot: the container holds the server-rendered tree, before any client JS.
check(container.children.length === 1 && container.children[0] === serverMain, "pre-boot: container holds the server-rendered <main>");

global.document = {
    createElement: (tag) => new StubElement(tag),
    getElementById: (id) => (id === "app" ? container : null),
    querySelector: () => null,
    querySelectorAll: () => [],
};
global.window = { addEventListener: () => {} };
global.location = { pathname: "/" };

// Boot the client bundle: its top-level mount_root("app", ...) runs on require.
require(process.argv[2]);

// Post-boot: the container was REPLACED — old server nodes detached, live tree in.
check(container.children.length === 1, "boot: container has exactly one child (the live root)");
const liveMain = container.children[0];
check(liveMain !== serverMain, "boot: the child is a NEWLY BUILT node, not the server <main>");
check(liveMain.tagName === "main" && liveMain.className === "app", "boot: the live root is <main class=app>");
check(serverMain.parent === null, "boot: the server <main> was detached (replaceChildren)");

// The live UI is present: the signal-fed list rendered fresh.
const liveItems = liveMain.findAll((el) => el.tagName === "li").map((el) => el.textContent);
check(liveItems.length === 2 && liveItems[0] === "Render on the server" && liveItems[1] === "Replace on boot", "boot: the live list rendered from the signal");

// Bindings are live: a signal write (a button click) propagates to the NEW tree.
const liveButton = liveMain.find((el) => el.tagName === "button");
check(!!liveButton && liveButton !== serverButton, "boot: a live button mounted (distinct from the server one)");
check(liveButton.textContent === "idle", "boot: live button initial bound text is 'idle'");
liveButton.click();
check(liveButton.textContent === "clicked", "live: the signal write propagated to the new tree (binding fired)");

// The old server nodes are gone from the live page and inert.
check(container.find((el) => el === serverMain) === null, "dead: the server <main> is gone from the live container");
check(container.find((el) => el === serverList) === null, "dead: the server list is gone from the live container");
check(container.find((el) => el === serverButton) === null, "dead: the server button is gone from the live container");
check(serverButton.textContent === "idle", "dead: the detached server button got no update from the write");

process.exit(failures === 0 ? 0 : 1);
"#;

#[test]
fn ssr_serves_rendered_markup_then_the_client_replaces_it() {
    let dir = temp_project("fullstack");
    copy_tree(&example_dir(), &dir);

    // A free port injected into the server source (both the `.port(..)` and the
    // cosmetic banner) so the test is hermetic and parallel-safe.
    let port = free_port();
    let server_source = dir.join("server/src/main.vl");
    let patched = std::fs::read_to_string(&server_source)
        .unwrap()
        .replace("8791", &port.to_string());
    std::fs::write(&server_source, patched).unwrap();

    write(&dir, "boot_harness.js", BOOT_HARNESS);
    build(&dir);

    // The server runs from the project root (it reads `dist/client.js` and
    // `server/src/app.html` by relative path).
    let mut server = Command::new("node")
        .arg("dist/server.js")
        .current_dir(&dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn node server");

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert!(
            wait_for_port(port, Duration::from_secs(20)),
            "the SSR server should accept connections on port {port}"
        );

        // --- Phase 1: the served HTML carries the rendered content, pre-JS. ---
        let page = String::from_utf8_lossy(&http_get(port, "/")).to_string();
        assert!(
            !page.contains("<!--ssr-->"),
            "the shell marker should be replaced by the render:\n{page}"
        );
        assert!(
            page.contains("<div id=\"app\"><main class=\"app\">"),
            "the render should be spliced inside #app:\n{page}"
        );
        // The signal-fed list items.
        assert!(
            page.contains("<li>Render on the server</li>"),
            "missing list item 1:\n{page}"
        );
        assert!(
            page.contains("<li>Replace on boot</li>"),
            "missing list item 2:\n{page}"
        );
        // The escaped heading — `&`/`<`/`>` are entities in the served bytes.
        assert!(
            page.contains("<h1>Tasks &amp; &lt;notes&gt;</h1>"),
            "heading not escaped:\n{page}"
        );
        // The `when` branch, rendered.
        assert!(
            page.contains("<p>server-rendered, then replaced</p>"),
            "missing when branch:\n{page}"
        );
        // The read-once bound button.
        assert!(
            page.contains("<button>idle</button>"),
            "missing bound button:\n{page}"
        );
        // The page references the client bundle, which the server also serves.
        assert!(
            page.contains("src=\"/client.js\""),
            "missing client script tag:\n{page}"
        );
        let bundle = String::from_utf8_lossy(&http_get(port, "/client.js")).to_string();
        assert!(
            bundle.contains("mount_root") && bundle.contains("replaceChildren"),
            "GET /client.js should return the replace-capable bundle"
        );

        // --- Phase 2: boot the client bundle; assert the container was replaced. ---
        let run = Command::new("node")
            .arg("boot_harness.js")
            .arg(dir.join("dist/client.js"))
            .current_dir(&dir)
            .output()
            .expect("run node boot harness");
        assert!(
            run.status.success(),
            "client boot harness failed:\n{}\n{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        );
    }));

    let _ = server.kill();
    let _ = server.wait();
    if outcome.is_ok() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    outcome.unwrap();
}
