//! kolt.local 007's **apply matrix**, pinned cell by cell: the classification a
//! round makes (`hmr.rs`'s `classify`, unit-pinned there) reaching a real page.
//!
//! The item asks for the whole matrix rather than the two reported faces —
//! **(asset new / changed / removed) × (event named / nameless) × (link present
//! / absent)** — plus the `swap` axis the fix adds (a swap declares the round's
//! stylesheet set, because an S2 swap re-evaluates a bundle without reloading
//! the document and so refreshes no stylesheet on its own).
//!
//! The shim driven here is the REAL shipped one: `run --watch` instruments a
//! browser leg with it and `dist/client.js` is read back byte-for-byte. What is
//! stubbed is `fetch` — deliberately, because the matrix needs an asset table
//! this test CONTROLS (present, changed under it, absent) and two documents that
//! differ only in whether the `<link>` is there. The real wiring — a real dev
//! channel answering a real `fetch` from this shim — is anchored by
//! `tests/hmr.rs`'s `a_css_push_heals_a_boot_time_stale_server_route` and
//! `a_first_ever_stylesheet_reaches_a_page_that_has_no_link_for_it`, which pin
//! one cell of each column against the running channel.
//!
//! Two node runs, one per `<link>` column, because the shim's shadow map lives
//! for the life of a bundle instance: a second document needs a second instance.

use std::io::{BufRead, BufReader};
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
        "vilan_css_matrix_{tag}_{}_{unique}",
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

fn wait_for_file(path: &Path, deadline: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < deadline {
        if path.is_file() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// Waits (bounded) for a line containing `needle` in the watcher's own trace
/// (`VILAN_WATCH_LOG`, B208) — the loop narrates every round there, so "round 1
/// has finished" is an EVENT this test can wait ON rather than a state it has
/// to infer from the files the round was in the middle of writing (N35).
fn wait_for_trace_line(trace: &Path, needle: &str, deadline: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < deadline {
        if std::fs::read_to_string(trace).is_ok_and(|text| text.contains(needle)) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// Whether `path` is a whole ES module — node's own parser, which is the reader
/// that matters, since the harness loads this file with `await import(..)`.
///
/// N35: a truncated bundle is what the harness reported as `SyntaxError:
/// Unexpected end of input`, and asking node to PARSE the file is the only
/// check that cannot be satisfied by half of one. Substring tests cannot: the
/// shim is prepended, so `window.__VILAN_HMR__` is present in the first
/// kilobyte of a file that may be a hundred times that.
fn parses_as_a_module(path: &Path) -> bool {
    Command::new("node")
        .arg("--check")
        .arg(path)
        .output()
        .is_ok_and(|check| check.status.success())
}

/// The number of matrix cells each column drives — asserted, so a harness that
/// dies before its assertions (or loses some to a refactor) cannot report a
/// matrix it never drove.
const LINKED_CELLS: usize = 27;
const LINKLESS_CELLS: usize = 19;

const CLIENT: &str = "import std::io::print;\n\nfun main() {\n\tprint(\"client up\");\n}\n";
const SERVER: &str = "import std::io::print;\n\nfun main() {\n\tprint(\"server up\");\n}\n";

/// The DOM/host stub every column shares. `nextSibling` is real (the shim
/// inserts a shadow `<style>` immediately after the `<link>` it supersedes, to
/// keep the sheet's place in the cascade), `removeChild` is real (a withdrawn
/// sheet must actually leave the document), and `fetch` answers from an asset
/// table this harness edits between cells — a sidecar the dev channel does not
/// have answers 404, exactly as `serve_asset` does.
const PRELUDE: &str = r#"
let failures = 0;
function check(condition, message) {
    if (condition) { console.log("ok   - " + message); }
    else { failures += 1; console.error("FAIL - " + message); }
}

class Node_ {
    constructor(tag) { this.tagName = tag; this.parentNode = null; this._text = ""; this.attributes = {}; }
    set textContent(text) { this._text = text; }
    get textContent() { return this._text; }
    setAttribute(name, value) { this.attributes[name] = value; }
    get nextSibling() {
        if (!this.parentNode) { return null; }
        const index = this.parentNode.children.indexOf(this);
        return this.parentNode.children[index + 1] || null;
    }
}
class Head {
    constructor() { this.children = []; }
    appendChild(element) { element.parentNode = this; this.children.push(element); return element; }
    insertBefore(element, reference) {
        element.parentNode = this;
        const index = reference ? this.children.indexOf(reference) : -1;
        if (index === -1) { this.children.push(element); } else { this.children.splice(index, 0, element); }
        return element;
    }
    removeChild(element) {
        this.children = this.children.filter((child) => child !== element);
        element.parentNode = null;
        return element;
    }
    styles() { return this.children.filter((child) => child.tagName === "style"); }
    styleFor(name) { return this.styles().find((style) => style.attributes["data-vilan-hmr"] === name) || null; }
}
class Link extends Node_ {
    constructor(head, href) { super("link"); this.href = href; this.rel = "stylesheet"; this.disabled = false; head.appendChild(this); }
}

const head = new Head();
const ORIGIN = "http://127.0.0.1:4321/";

// The dev channel's asset table, as this harness chooses to shape it.
let assets = {};
const requested = [];
globalThis.fetch = (url) => {
    const path = String(url).split("?")[0];
    if (path.indexOf("/bundle/") !== -1) {
        // The swap's own bundle fetch: a trivial module, so the swap protocol
        // completes and the cell under test stays about stylesheets.
        return Promise.resolve({ ok: true, status: 200, text: () => Promise.resolve("export {};\n") });
    }
    const name = path.slice(path.lastIndexOf("/") + 1);
    requested.push(name);
    if (!Object.prototype.hasOwnProperty.call(assets, name)) {
        return Promise.resolve({ ok: false, status: 404, text: () => Promise.resolve("") });
    }
    return Promise.resolve({ ok: true, status: 200, text: () => Promise.resolve(assets[name]) });
};

globalThis.window = globalThis;
globalThis.location = { reload: () => { globalThis.__reloaded = true; } };
globalThis.Blob = class { constructor(parts) { this.__text = parts.join(""); } };
URL.createObjectURL = (blob) => "data:text/javascript;base64," + Buffer.from(blob.__text).toString("base64");
URL.revokeObjectURL = () => {};
"#;

/// **Column: `<link>` PRESENT.** The page links `client.css` (this leg's own
/// sidecar), `other.css` (a sheet the page links but that is not this leg's)
/// and `gone.css` (linked, but the channel has no such asset).
const LINKED_HARNESS: &str = r#"
const clientLink = new Link(head, ORIGIN + "client.css");
const otherLink = new Link(head, ORIGIN + "other.css");
const goneLink = new Link(head, ORIGIN + "gone.css");
globalThis.document = {
    querySelectorAll: (selector) => (selector === 'link[rel="stylesheet"]' ? head.children.filter((c) => c.tagName === "link") : []),
    createElement: (tag) => new Node_(tag),
    getElementById: () => null,
    head,
    documentElement: head,
};

assets = { "client.css": "CLIENT_V1", "other.css": "OTHER_V1" };

await import("./bundle.mjs");
const hmr = globalThis.window.__VILAN_HMR__;
check(!!hmr, "the shim installed the singleton");

// (named x new x link present) — the sheet is loaded by a <link> and the shim
// has no copy of it yet: the <link> is disabled and shadowed by a <style>.
await hmr.handleEvent({ kind: "css", version: hmr.version, asset: "client.css" });
check(clientLink.disabled === true, "named x new x present: the superseded <link> is disabled");
check(head.styleFor("client.css") !== null, "named x new x present: a <style> carries the sheet");
check(head.styleFor("client.css").textContent === "CLIENT_V1", "named x new x present: it carries the channel's bytes");
check(head.children.indexOf(head.styleFor("client.css")) === head.children.indexOf(clientLink) + 1,
    "named x new x present: the <style> takes the <link>'s place in the cascade, not the end of <head>");
check(otherLink.disabled === false && goneLink.disabled === false,
    "named x new x present: no other <link> is touched (asset matching)");

// (named x changed x link present) — the same sheet again with new bytes:
// the SAME element updates. No stack, no second fetch path.
assets["client.css"] = "CLIENT_V2";
const clientStyle = head.styleFor("client.css");
await hmr.handleEvent({ kind: "css", version: hmr.version, asset: "client.css" });
check(head.styles().length === 1, "named x changed x present: no duplicate <style>");
check(head.styleFor("client.css") === clientStyle, "named x changed x present: the same element is reused");
check(clientStyle.textContent === "CLIENT_V2", "named x changed x present: it carries the new bytes");

// (named x removed x link present) — the page links it, the channel 404s. The
// never-reload discipline: warn, and leave the stylesheet exactly as it was.
await hmr.handleEvent({ kind: "css", version: hmr.version, asset: "gone.css" });
check(goneLink.disabled === false, "named x removed x present: a 404 leaves the <link> enabled");
check(head.styleFor("gone.css") === null, "named x removed x present: a 404 injects nothing");
check(head.styles().length === 1, "named x removed x present: a 404 changes nothing at all");

// A failed fetch must never WITHDRAW a sheet either — the round says what was
// removed (a swap's `sheets`), a 404 never does.
delete assets["client.css"];
await hmr.handleEvent({ kind: "css", version: hmr.version, asset: "client.css" });
check(clientStyle.textContent === "CLIENT_V2", "a 404 for a sheet already applied keeps the current bytes");
check(head.styleFor("client.css") === clientStyle, "a 404 never withdraws a sheet");
assets["client.css"] = "CLIENT_V3";

// (nameless x changed x link present) — every stylesheet <link> the page has,
// each fetched by its OWN basename (hmr.md §2), including one that 404s.
assets["other.css"] = "OTHER_V2";
await hmr.handleEvent({ kind: "css", version: hmr.version });
check(head.styleFor("client.css").textContent === "CLIENT_V3", "nameless x changed x present: this leg's sheet refreshed");
check(head.styleFor("other.css") !== null && head.styleFor("other.css").textContent === "OTHER_V2",
    "nameless x changed x present: a linked sheet that is not this leg's refreshed too");
check(otherLink.disabled === true, "nameless x changed x present: its <link> is superseded as well");
check(goneLink.disabled === false, "nameless x removed x present: the 404 in the sweep changed nothing");

// (named x changed x present, foreign sheet) — a named event for a sheet this
// page links but does not own touches only that one.
assets["other.css"] = "OTHER_V3";
assets["client.css"] = "CLIENT_V4";
await hmr.handleEvent({ kind: "css", version: hmr.version, asset: "other.css" });
check(head.styleFor("other.css").textContent === "OTHER_V3", "named x changed x present: the named sheet updated");
check(head.styleFor("client.css").textContent === "CLIENT_V3", "named x changed x present: the sheet NOT named is untouched");

// swap x declares — the round changed a bundle, and its sidecars come with it.
// Before the fix a swap refreshed no stylesheet at all: an S2 swap re-evaluates
// the bundle, it does not reload the document, so every round that changed a
// bundle AND its css dropped the css on the floor.
assets["client.css"] = "CLIENT_V5";
assets["other.css"] = "OTHER_V4";
await hmr.handleEvent({ kind: "swap", version: hmr.version + 1, sheets: ["client.css", "other.css"] });
check(head.styleFor("client.css").textContent === "CLIENT_V5", "swap x declares: the swap carried this leg's stylesheet");
check(head.styleFor("other.css").textContent === "OTHER_V4", "swap x declares: and every declared sheet the page links");
check(hmr.version === 2, "swap x declares: the bundle still swapped");

// swap x declares NONE — the removal cell, the only statement that withdraws.
await hmr.handleEvent({ kind: "swap", version: hmr.version + 1, sheets: [] });
check(head.styleFor("client.css") === null, "swap x declares none: this leg's <style> is withdrawn from the document");
check(clientLink.disabled === true, "swap x declares none: its <link> stays disabled (never re-pointed at the app's own stale route)");
check(head.styleFor("other.css") !== null && head.styleFor("other.css").textContent === "OTHER_V4",
    "swap x declares none: a sheet that is not this leg's is NOT withdrawn — the shim withdraws only its own");

check(!globalThis.__reloaded, "no cell in this column ever reloaded the page");
console.log(failures === 0 ? "css matrix verdict: PASS" : "css matrix verdict: FAIL");
process.exit(failures === 0 ? 0 : 1);
"#;

/// **Column: `<link>` ABSENT.** The page carries the app's own hand-written
/// `theme.css` and nothing else — the document a boot-time-rendered server
/// served before the stylesheet existed (kolt.local 007's face one).
const LINKLESS_HARNESS: &str = r#"
const themeLink = new Link(head, ORIGIN + "theme.css");
globalThis.document = {
    querySelectorAll: (selector) => (selector === 'link[rel="stylesheet"]' ? head.children.filter((c) => c.tagName === "link") : []),
    createElement: (tag) => new Node_(tag),
    getElementById: () => null,
    head,
    documentElement: head,
};

assets = { "client.css": "CLIENT_V1", "admin.css": "ADMIN_V1" };

await import("./bundle.mjs");
const hmr = globalThis.window.__VILAN_HMR__;
check(!!hmr, "the shim installed the singleton");

// (nameless x new x link absent) — a nameless event walks the page's <link>s,
// and a sheet with no <link> is in no list to walk. This is WHY the CLI always
// names its sidecar, and it is the defined behaviour, not a gap.
await hmr.handleEvent({ kind: "css", version: hmr.version });
check(head.styleFor("client.css") === null, "nameless x new x absent: a nameless event cannot reach an unlinked sheet");
check(themeLink.disabled === false, "nameless x new x absent: the app's own sheet 404s and is left enabled");

// (named x new x link absent) — THE reported face. The dev channel serves the
// sheet, the document has nowhere to put it: it joins <head> on its own.
await hmr.handleEvent({ kind: "css", version: hmr.version, asset: "client.css" });
const style = head.styleFor("client.css");
check(style !== null, "named x new x absent: the first-ever stylesheet reaches the page as a <style>");
check(style.textContent === "CLIENT_V1", "named x new x absent: carrying the channel's bytes");
check(head.styles().length === 1, "named x new x absent: exactly one <style>, not a stack");
check(themeLink.disabled === false, "named x new x absent: the app's unrelated sheet stays enabled");

// (named x changed x link absent) — the same element updates in place.
assets["client.css"] = "CLIENT_V2";
await hmr.handleEvent({ kind: "css", version: hmr.version, asset: "client.css" });
check(head.styleFor("client.css") === style, "named x changed x absent: the same <style> is reused");
check(style.textContent === "CLIENT_V2", "named x changed x absent: it carries the new bytes");
check(head.styles().length === 1, "named x changed x absent: still exactly one <style>");

// (named x new x link absent, NOT this leg's sheet) — every browser leg's page
// gets the same broadcast. A sheet this document neither links nor owns applies
// nowhere: no <style>, and no request either.
const before = requested.length;
await hmr.handleEvent({ kind: "css", version: hmr.version, asset: "admin.css" });
check(head.styleFor("admin.css") === null, "named x new x absent (foreign leg): another leg's sheet is not injected here");
check(requested.length === before, "named x new x absent (foreign leg): and is not even fetched");

// (named x removed x link absent) — the channel 404s: warn, keep. A failed
// fetch never withdraws a sheet that is already applied.
delete assets["client.css"];
await hmr.handleEvent({ kind: "css", version: hmr.version, asset: "client.css" });
check(head.styleFor("client.css") === style && style.textContent === "CLIENT_V2",
    "named x removed x absent: a 404 keeps the stylesheet exactly as it was");

// swap x declares — the path the CLASSIFIER now takes for a presence
// transition: a first-ever sidecar is a browser-output change, so the round
// pushes `swap` declaring it, and the reconcile lands it on a link-less page.
assets["client.css"] = "CLIENT_V3";
await hmr.handleEvent({ kind: "swap", version: hmr.version + 1, sheets: ["client.css"] });
check(head.styleFor("client.css").textContent === "CLIENT_V3", "swap x declares x absent: the declared sheet lands with no <link> to supersede");
check(head.styles().length === 1, "swap x declares x absent: still one <style>");

// swap x declares NONE — the removal cell on a link-less page.
await hmr.handleEvent({ kind: "swap", version: hmr.version + 1, sheets: [] });
check(head.styleFor("client.css") === null, "swap x declares none x absent: the <style> is withdrawn");
check(head.styles().length === 0, "swap x declares none x absent: nothing of the shim's is left in <head>");
check(themeLink.disabled === false, "swap x declares none x absent: the app's own sheet is untouched throughout");

check(!globalThis.__reloaded, "no cell in this column ever reloaded the page");
console.log(failures === 0 ? "css matrix verdict: PASS" : "css matrix verdict: FAIL");
process.exit(failures === 0 ? 0 : 1);
"#;

/// Runs one column's harness and requires `cells` PASSING assertions from it —
/// not merely a zero exit. A harness that threw before its first `check`, or
/// that a refactor quietly shortened, exits 0 with `failures === 0` and would
/// otherwise report a matrix it never drove.
fn run_harness(dir: &Path, name: &str, body: &str, cells: usize) {
    std::fs::write(dir.join(name), format!("{PRELUDE}{body}")).unwrap();
    let run = Command::new("node")
        .arg(name)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|error| panic!("run node {name}: {error}"));
    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    // Windows only: node's own shutdown race (nodejs/node#56645), the same
    // tolerance `tests/hmr.rs`'s css e2e documents. The stdout verdict is truth.
    let windows_teardown_abort = cfg!(windows)
        && stdout.contains("css matrix verdict: PASS")
        && stderr.contains("Assertion failed: !(handle->flags & UV_HANDLE_CLOSING)");
    assert!(
        run.status.success() || windows_teardown_abort,
        "{name} failed:\n{stdout}\n{stderr}"
    );
    let passed = stdout
        .lines()
        .filter(|line| line.starts_with("ok   - "))
        .count();
    assert_eq!(
        passed, cells,
        "{name} should have driven {cells} matrix cells, drove {passed}:\n{stdout}"
    );
}

#[test]
fn n35_a_truncated_bundle_is_not_mistaken_for_a_whole_one() {
    // The race the read below closes, asked of the two readers directly. The
    // watcher writes `dist/client.js` while this suite reads it, and what the
    // old loop asked of what it read was two SUBSTRING tests — both of which a
    // prefix satisfies, because the shim is prepended and carries both needles
    // in its first kilobyte. A prefix is what the harness then imported, and
    // `SyntaxError: Unexpected end of input` is what it reported (green in
    // isolation, green on the rerun, failing only under load). node's parser is
    // the reader that can tell a prefix from a file.
    let dir = temp_project("truncation");
    std::fs::create_dir_all(&dir).unwrap();
    let whole = "var HMR = {};\nwindow.__VILAN_HMR__ = HMR;\n\
                 export function boot(page) {\n    return page;\n}\n";
    let cut = &whole[..whole.find("export function").unwrap() + 30];

    for text in [whole, cut] {
        assert!(
            text.contains("window.__VILAN_HMR__") && !text.contains("__VILAN_HMR_BUNDLE__"),
            "both readings satisfy the substring tests — that is the whole \
             problem: {text:?}"
        );
    }

    let module = dir.join("whole.mjs");
    std::fs::write(&module, whole).unwrap();
    assert!(
        parses_as_a_module(&module),
        "the whole module parses, so the check costs a healthy read nothing"
    );
    let partial = dir.join("cut.mjs");
    std::fs::write(&partial, cut).unwrap();
    assert!(
        !parses_as_a_module(&partial),
        "a bundle cut mid-function must not pass for a whole one"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// kolt.local 007's apply matrix. One `run --watch` produces the instrumented
/// bundle; each column then drives the real shim's real `handleEvent` through
/// its cells.
#[test]
fn the_css_apply_matrix_holds_in_every_cell() {
    let dir = temp_project("apply");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"cssmatrix\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    write(&dir, "src/client.vl", CLIENT);
    write(&dir, "src/server.vl", SERVER);

    // The watcher narrates its rounds into this file (B208), which is what
    // turns "round 1 has finished" into something to wait on (N35).
    let trace = dir.join("watch-trace.log");
    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", "--watch", "--hmr-port", "0", dir.to_str().unwrap()])
        .env("VILAN_WATCH_LOG", &trace)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn run --watch");

    let stdout = watcher.stdout.take().unwrap();
    let (sender, _lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = sender.send(line);
        }
    });

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let bundle_path = dir.join("dist/client.js");
        // N35 — the round's own completion signal, ahead of every read of what
        // the round wrote. `dist/client.js` appearing means the round has
        // STARTED writing it, and this test then read the file while the
        // watcher was still writing it: under suite load it once took away a
        // truncated bundle that satisfied both substring tests below and threw
        // `SyntaxError: Unexpected end of input` inside the node harness, green
        // in isolation and green on the rerun. The verdict line is written
        // after the round's action returns, so everything the round writes is
        // on disk by the time it appears.
        assert!(
            wait_for_trace_line(&trace, "round 1 end verdict=", support::WATCH_LIVENESS),
            "round 1 should have finished and said so in the watch trace"
        );
        assert!(
            wait_for_file(&bundle_path, support::WATCH_LIVENESS),
            "round 1 should have written dist/client.js"
        );
        // The bundle is read back rather than fetched: what this matrix needs
        // from the channel is the SHIM's bytes, and `dist/client.js` is the very
        // copy the channel serves (`serve_asset` reads it from the same file).
        //
        // Read until it PARSES, and bounded. The substring tests say the shim is
        // there and its bundle placeholder was substituted, which is a claim
        // about the CONTENT and is exactly what half a file can satisfy; node's
        // parser is what says the file is whole. The old loop asked only the two
        // substrings and had no deadline at all, so a round that never produced
        // an instrumented bundle hung here instead of failing.
        let module = dir.join("bundle.mjs");
        let started = Instant::now();
        loop {
            let text = std::fs::read_to_string(&bundle_path).expect("read the instrumented bundle");
            // Written before it is judged, because the judgement is node's and
            // node reads a file: what the harness imports is the copy that
            // passed, byte for byte.
            std::fs::write(&module, &text).unwrap();
            if text.contains("window.__VILAN_HMR__")
                && !text.contains("__VILAN_HMR_BUNDLE__")
                && parses_as_a_module(&module)
            {
                break;
            }
            assert!(
                started.elapsed() < support::WATCH_LIVENESS,
                "dist/client.js should have settled into a whole instrumented \
                 bundle within {:?} (read {} bytes last)",
                support::WATCH_LIVENESS,
                text.len()
            );
            std::thread::sleep(Duration::from_millis(50));
        }

        run_harness(&dir, "linked.mjs", LINKED_HARNESS, LINKED_CELLS);
        run_harness(&dir, "linkless.mjs", LINKLESS_HARNESS, LINKLESS_CELLS);
    }));

    support::kill_watcher(&mut watcher);
    if outcome.is_ok() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    outcome.unwrap();
}
