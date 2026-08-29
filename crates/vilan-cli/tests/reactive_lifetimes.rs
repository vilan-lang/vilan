//! The reactive graph's lifetime gates (proposal/lifetimes.md §5 and §11's S1
//! line; backlog A28/A29).
//!
//! The lifetime session did not read these facts off the source — it took heap
//! snapshots of RUNNING programs and ran an SCC analysis over them. These tests
//! measure the same way: a browser-target app is built with the real CLI and run
//! under node against a DOM stub, and the assertions are made on what the
//! running program's object graph actually holds.
//!
//! Three gates, each pinning one thing:
//!
//! 1. [`derivations_detach_from_their_source_with_their_boundary`] — A28's leak.
//!    The documented router idiom (`current_path().map(parse)` inside
//!    `mount_root`, then dispose) left one dead subscriber on the module-level
//!    path signal per round, forever, and made every later navigation notify
//!    every dead derivation ever built. 25 rounds; the count must come back 0.
//! 2. [`a_two_way_binding_reads_its_value_from_the_event`] — V3's behavior half.
//!    The event's `target` and the element are DIFFERENT objects in the stub, so
//!    a listener still reaching for its own element reads the wrong string.
//! 3. [`a_disposed_exemplar_holds_no_reactive_cycle`] — the standing no-cycle
//!    gate, the walker in `support/heap_cycles.js`.
//!
//! **The no-cycle gate's contract is the POST-DISPOSAL graph, deliberately.** A
//! mounted app of any realism contains V4, the one *semantic* loop: a handler
//! writes a signal that a binding on the same element reads, so the element
//! reaches the signal and the signal reaches the element. §5 records V4 as
//! dissolved by disposal rather than removed, and the measured mounted state
//! agrees — this exemplar mounted holds V4 plus two live-session loops (a
//! `ReactiveServer`/`ReactiveClient` and its transport handler, a
//! `RemoteSource` lease and its cache), all of which are exactly as long-lived
//! as the thing they belong to. What must be ZERO is what survives the
//! teardown, and that is what this asserts.
//!
//! Every one of V1, V3 and V5 was proven to redden it by planting the bug back:
//! `Signal`'s notify closure capturing the signal instead of the value cell, the
//! write-back listener moved inline beside the effect that captures the element,
//! and `dispose` leaving `DuplexEnd.me` set.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A fresh temp directory for one test's project tree.
fn temp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "vilan_reactive_lifetimes_{tag}_{}",
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

/// The DOM/history stub every harness here builds on: enough of a document to
/// mount into, with `parent`/`children` links so the walk sees a real tree.
const DOM_STUB: &str = r#"class StubElement {
    constructor(tag) {
        this.tagName = tag;
        this.children = [];
        this.parent = null;
        this.listeners = {};
        this._text = "";
        this.value = "";
        this.attributes = {};
        this.style = { setProperty: () => {} };
    }
    set textContent(text) { this._text = text; this.children = []; }
    get textContent() { return this._text; }
    setAttribute(name, value) { this.attributes[name] = value; }
    appendChild(child) {
        if (child.parent) child.parent.children = child.parent.children.filter(c => c !== child);
        child.parent = this;
        this.children.push(child);
    }
    remove() {
        if (this.parent) {
            this.parent.children = this.parent.children.filter(c => c !== this);
            this.parent = null;
        }
    }
    replaceChildren() { for (const c of this.children) c.parent = null; this.children = []; }
    addEventListener(event, handler) { (this.listeners[event] = this.listeners[event] || []).push(handler); }
    fire(event, payload = {}) { for (const h of (this.listeners[event] || [])) h(payload); }
    find(predicate) {
        if (predicate(this)) return this;
        for (const c of this.children) { const hit = c.find(predicate); if (hit) return hit; }
        return null;
    }
}

const documentRoot = new StubElement("div");
global.document = {
    createElement: (tag) => new StubElement(tag),
    createElementNS: (namespace, tag) => new StubElement(tag),
    getElementById: () => documentRoot,
    querySelector: () => null,
    querySelectorAll: () => [],
};
global.location = { pathname: "/" };
global.history = { pushState(state, title, path) { global.location.pathname = path; } };
global.window = { addEventListener: () => {} };
"#;

/// Builds `app.vl` for the browser with the real CLI and runs `harness.js`
/// under node, returning its stdout. Fails loudly with both streams.
fn build_and_run(tag: &str, app: &str, harness: &str, support: &[(&str, &str)]) -> String {
    let dir = temp_project(tag);
    write(
        &dir,
        "vilan.toml",
        &format!(
            "[package]\nname = \"reactive_lifetimes_{tag}\"\nroot = \".\"\nentry = \"app.vl\"\ntarget = \"browser\"\n"
        ),
    );
    write(&dir, "app.vl", app);
    write(&dir, "harness.js", harness);
    for (name, contents) in support {
        write(&dir, name, contents);
    }

    let build = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", dir.to_str().unwrap()])
        .output()
        .expect("run vilan build");
    assert!(
        build.status.success(),
        "vilan build failed:\n{}\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let run = Command::new("node")
        .arg("harness.js")
        .current_dir(&dir)
        .output()
        .expect("run node harness");
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(
        run.status.success(),
        "harness failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
    stdout
}

// --- A28: the derivation combinators are detachable --------------------------

/// The idiom `std::router` documents, run 25 times: derive the typed route from
/// the module-level path signal inside a `mount_root` body, then dispose.
const ROUTER_IDIOM: &str = r#"import std::io::print;
import std::reactive::{ Disposable, Signal };
import std::router::{ current_path, segments };
import std::ui::{ View, mount_root, view };

[derive(PartialEq)]
enum Route {
	Home,
	Page(str),
}

fun parse(path: str): Route {
	let parts = segments(path);
	if parts.len() == 0 { Route::Home } else { Route::Page(parts[0]) }
}

fun label(route: Route): str {
	match route {
		Route::Home => "home",
		Route::Page(let name) => name,
	}
}

fun main() {
	let path = current_path();
	mut round = 0;
	for round < 25 {
		let root = mount_root("app", || view("main").bind_text(path.map(parse).map(label)));
		root.dispose();
		round += 1;
	}
	print(i"subscribers={path.subscribers.read().len()}");
}
"#;

#[test]
fn derivations_detach_from_their_source_with_their_boundary() {
    let harness = format!("{DOM_STUB}\nrequire(\"./app.js\");\n");
    let stdout = build_and_run("router_idiom", ROUTER_IDIOM, &harness, &[]);
    // 25 before A28 (one dead subscriber per mount/dispose round, forever), and
    // it is a time leak too: every navigation notified every dead derivation.
    assert!(
        stdout.contains("subscribers=0"),
        "25 mount/dispose rounds must leave the module path signal with no \
         subscribers; got:\n{stdout}"
    );
}

// --- V3: the write-back reads the event, not the element ---------------------

const TWO_WAY_BINDINGS: &str = r#"import std::io::print;
import std::option::Option::{ None, self };
import std::reactive::{ Signal, draft };
import std::ui::{ View, mount_root, view };

fun main() {
	let typed = Signal::new("");
	let note = draft("", |value: str| { let _seen = value; None });
	let _root = mount_root("app", || view("form")
		.child(view("input").attr("id", "plain").bind_value(typed))
		.child(view("input").attr("id", "note").bind_draft(note)));
	report(|| {
		print(i"signal={typed.get()}");
		print(i"draft={note.local.get()}");
	});
}

/// The harness fires the input events between mount and this call.
[extern("__report")]
external fun report(show: || void): void;
"#;

#[test]
fn a_two_way_binding_reads_its_value_from_the_event() {
    let harness = format!(
        r#"{DOM_STUB}
global.__report = (show) => {{
    // The event's target is NOT the element: a listener still reaching for its
    // own element reads "" here, and the assertions below fail.
    for (const [id, text] of [["plain", "typed into it"], ["note", "drafted"]]) {{
        const input = documentRoot.find(e => e.attributes.id === id);
        input.fire("input", {{ target: {{ value: text }} }});
    }}
    show();
}};
require("./app.js");
"#
    );
    let stdout = build_and_run("two_way", TWO_WAY_BINDINGS, &harness, &[]);
    assert!(
        stdout.contains("signal=typed into it"),
        "bind_value's write-back must read the event's target value; got:\n{stdout}"
    );
    assert!(
        stdout.contains("draft=drafted"),
        "bind_draft's write-back must read the event's target value; got:\n{stdout}"
    );
}

// --- The standing no-cycle gate ----------------------------------------------

/// The exemplar: everything §5 named, in one mounted app. A derivation made
/// OUTSIDE every boundary (the module-level `route`, which nothing disposes and
/// which therefore is what the post-disposal walk still sees), a bound text, a
/// two-way input, a handler that writes signals the view reads (V4), a keyed
/// list, and a live reactive RPC session on an in-process duplex.
const CYCLE_EXEMPLAR: &str = r#"import std::json::json_codec;
import std::reactive::{ Disposable, Signal };
import std::rpc::{ ReactiveClient, ReactiveServer, RemoteSource, duplex_pair };
import std::ui::{ View, mount_root, view };

let path: Signal<str> = Signal::new("/");
let route: Signal<str> = path.map(|value| "route" + value);

fun row(item: str): View {
	view("li").text(item)
}

fun app(items: Signal<List<str>>, draft: Signal<str>): View {
	view("main")
		.child(view("h1").bind_text(route))
		.child(view("input").bind_value(draft))
		.child(view("button").text("add").on("click", || {
			items.update(|&mut list| { list.push(draft.get()); });
			draft.set("");
		}))
		.child(view("ul").bind_each(items, |item| item, |item| row(item)))
}

fun main() {
	let items: Signal<List<str>> = Signal::new(["a", "b"]);
	let draft = Signal::new("");
	let root = mount_root("app", || app(items, draft));

	let status = Signal::new("idle");
	let (client_end, server_end) = duplex_pair();
	let server = ReactiveServer::new(server_end, json_codec());
	let channel = server.expose(status);
	let client = ReactiveClient::new(client_end, json_codec());
	let mirror: RemoteSource<str> = client.source(channel);
	let watching = mirror.sub(|value| keep(value));
	status.set("busy");

	// Everything stays reachable from the harness, so nothing is "acyclic"
	// merely by having been collected.
	keep(path);
	keep(route);
	keep(items);
	keep(draft);
	keep(server);
	keep(client);
	keep(server_end);
	keep(client_end);
	keep(mirror);
	mounted();

	root.dispose();
	watching.dispose();
	server.dispose();
	client.dispose();
	unmounted();
}

/// Park a value where the harness can reach it — the walk's roots.
[extern("__scc_keep")]
external fun keep<T>(value: T): void;

[extern("__scc_mounted")]
external fun mounted(): void;

[extern("__scc_unmounted")]
external fun unmounted(): void;
"#;

#[test]
fn a_disposed_exemplar_holds_no_reactive_cycle() {
    let harness = format!(
        r#"{DOM_STUB}
const v8 = require("v8");
const {{ analyze }} = require("./heap_cycles.js");

const kept = [documentRoot];
global.__scc_keep = (value) => {{ kept.push(value); }};
global.__vilan_scc_roots = kept;

global.__scc_mounted = () => {{
    const input = documentRoot.find(e => e.tagName === "input");
    input.fire("input", {{ target: {{ value: "c" }} }});
    documentRoot.find(e => e.tagName === "button").fire("click", {{}});
    v8.writeHeapSnapshot("./mounted.heapsnapshot");
}};
global.__scc_unmounted = () => {{
    v8.writeHeapSnapshot("./unmounted.heapsnapshot");
}};

require("./app.js");

for (const phase of ["mounted", "unmounted"]) {{
    const result = analyze(`./${{phase}}.heapsnapshot`, {{ rootEdgeName: "__vilan_scc_roots" }});
    console.log(`${{phase}} reachable=${{result.reachable}} cycles=${{result.components.length}}`);
    if (result.components.length > 0) console.log(result.report);
}}
"#
    );
    let stdout = build_and_run(
        "no_cycles",
        CYCLE_EXEMPLAR,
        &harness,
        &[("heap_cycles.js", include_str!("support/heap_cycles.js"))],
    );

    // The mounted line is RECORDED, not asserted: V4 and the live session loops
    // are there by design, and pinning their count would pin an implementation
    // detail of the exemplar rather than a law.
    assert!(
        stdout.contains("mounted reachable="),
        "the walk must reach the mounted app; got:\n{stdout}"
    );
    assert!(
        stdout.contains("unmounted reachable=") && stdout.contains("cycles=0"),
        "a disposed app must hold no reactive cycle; got:\n{stdout}"
    );
}
