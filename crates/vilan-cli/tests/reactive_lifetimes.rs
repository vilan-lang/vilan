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
const DOM_STUB: &str = r##"class StubElement {
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
    // A71: `appendChild`'s positional counterpart. `std::ui`'s `Region`
    // plants an empty text node and inserts its content BEFORE it, so a
    // reactive run keeps its place among static siblings.
    insertBefore(child, anchor) {
        if (child.parent) child.parent.children = child.parent.children.filter(c => c !== child);
        child.parent = this;
        const at = this.children.lastIndexOf(anchor);
        if (at < 0) this.children.push(child); else this.children.splice(at, 0, child);
        return child;
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

/// A text node — a real sibling of the element children: what a `str` or a
/// `Source<str>` child rides, and what `std::ui`'s `Region` plants (empty) as
/// the anchor it inserts before (A71).
class StubText {
    constructor(text) { this.tagName = "#text"; this.children = []; this.parent = null; this._text = text; }
    set textContent(text) { this._text = text; }
    get textContent() { return this._text; }
    remove() {
        if (this.parent) {
            this.parent.children = this.parent.children.filter(c => c !== this);
            this.parent = null;
        }
    }
}

const documentRoot = new StubElement("div");
global.document = {
    createElement: (tag) => new StubElement(tag),
    createElementNS: (namespace, tag) => new StubElement(tag),
    createTextNode: (text) => new StubText(text),
    getElementById: () => documentRoot,
    querySelector: () => null,
    querySelectorAll: () => [],
};
global.location = { pathname: "/" };
global.history = { pushState(state, title, path) { global.location.pathname = path; } };
global.window = { addEventListener: () => {} };
"##;

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
import std::reactive::{ Disposable, Signal, SignalCell };
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
import std::reactive::{ Signal, SignalCell, draft };
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
import std::reactive::{ Disposable, Signal, SignalCell };
import std::rpc::{ ReactiveClient, ReactiveServer, RemoteSource, duplex_pair };
import std::ui::{ View, mount_root, view };

let path: SignalCell<str> = Signal::new("/");
let route: SignalCell<str> = path.map(|value| "route" + value);

fun row(item: str): View {
	view("li").text(item)
}

fun app(items: SignalCell<List<str>>, draft: SignalCell<str>): View {
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
	let items: SignalCell<List<str>> = Signal::new(["a", "b"]);
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

// --- B291: an owner has a DISPOSED state ------------------------------------

/// The async shape the item names: a scope torn down while a continuation that
/// registers into it is still in flight — a route switched away before a
/// handle's reply, a `bind_each` row rebuilt while its first fetch is out.
const LATE_REGISTRATION: &str = r#"import std::io::print;
import std::reactive::{ Disposable, Owner, Signal, SignalCell, owner_scope };

async fun main() {
	let count: SignalCell<i32> = Signal::new(0);
	let fired: SignalCell<i32> = Signal::new(0);
	let owner = Owner::new();

	// The continuation's owner is captured at CREATION and disposed before the
	// continuation ever runs.
	owner_scope.run(owner, || {
		async {
			let _tick: i32 = await async 1;
			count.effect(|_value: i32| {
				fired.set_with(|n| n + 1);
			});
		};
	});
	owner.dispose();
	let _first: i32 = await async 1;
	let _second: i32 = await async 1;

	// `effect` is `effect_on_change` plus one immediate call, and the immediate
	// call is the observer's contract, not a subscription — it still happens.
	print(i"at-registration={fired.get()}");
	count.set(1);
	count.set(2);
	print(i"after-disposal={fired.get() - 1}");
	print(i"subscribers={count.subscribers.read().len()}");
	owner.dispose();
	count.set(3);
	print(i"after-second-dispose={fired.get() - 1}");
}
"#;

#[test]
fn b291_an_effect_registered_after_its_owner_was_disposed_never_fires_again() {
    // Before the flag, `Owner::dispose` emptied the cleanup list and kept no
    // record, so this `take` parked a cleanup on a list nothing runs again:
    // `after-disposal=2` (the observer fired on every later `set` for the rest
    // of the session) and only a SECOND `dispose` released it. The immediate
    // call at registration is deliberately NOT what changed — `effect`'s
    // contract is one call with the current value, and it is made before the
    // subscription is handed anywhere.
    let harness = format!("{DOM_STUB}\nrequire(\"./app.js\");\n");
    let stdout = build_and_run("late_registration", LATE_REGISTRATION, &harness, &[]);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "at-registration=1",
            "after-disposal=0",
            "subscribers=0",
            "after-second-dispose=0",
        ],
        "a late registration must be disposed on the spot; got:\n{stdout}"
    );
}

/// The three faces of the flag, synchronously: `dispose` is idempotent, a late
/// `defer` runs now, a late `take` disposes on the spot — and a LIVE owner
/// still parks everything it is given, which is the control.
const DISPOSED_OWNER_STATE: &str = r#"import std::io::print;
import std::reactive::{ Disposable, Owner, Signal, SignalCell };

fun main() {
	let ran: SignalCell<i32> = Signal::new(0);
	let owner = Owner::new();
	owner.defer(|| {
		ran.set_with(|n| n + 1);
	});
	owner.dispose();
	print(i"first-dispose={ran.get()}");
	owner.dispose();
	print(i"second-dispose={ran.get()}");

	// A cleanup deferred to an owner that is already gone runs NOW — the
	// promise to release is kept the only way it still can be.
	owner.defer(|| {
		ran.set_with(|n| n + 1);
	});
	print(i"late-defer={ran.get()}");

	// And a disposable TAKEN by a dead owner is disposed on the spot: the
	// observer is off the signal before the next write.
	let count: SignalCell<i32> = Signal::new(0);
	let seen: SignalCell<i32> = Signal::new(0);
	let late = owner.take(count.on_change(|_value: i32| {
		seen.set_with(|n| n + 1);
	}));
	count.set(1);
	print(i"late-take={seen.get()} subscribers={count.subscribers.read().len()}");
	late.dispose();
	print(i"disposing-it-again={count.subscribers.read().len()}");

	// The control: a live owner parks its cleanups and releases them as a
	// group, exactly as before.
	let live = Owner::new();
	let parked: SignalCell<i32> = Signal::new(0);
	live.defer(|| {
		parked.set_with(|n| n + 1);
	});
	print(i"parked={parked.get()}");
	live.dispose();
	print(i"released={parked.get()}");
}
"#;

#[test]
fn b291_a_disposed_owner_is_idempotent_and_releases_what_it_is_given_at_once() {
    let harness = format!("{DOM_STUB}\nrequire(\"./app.js\");\n");
    let stdout = build_and_run("disposed_owner", DISPOSED_OWNER_STATE, &harness, &[]);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "first-dispose=1",
            // Idempotent: the second `dispose` runs nothing. Before the flag
            // this was true only because the list had been emptied, which is
            // exactly what made a LATE registration immortal.
            "second-dispose=1",
            "late-defer=2",
            "late-take=0 subscribers=0",
            "disposing-it-again=0",
            "parked=0",
            "released=1",
        ],
        "the disposed owner's state went differently:\n{stdout}"
    );
}

// --- B292: the drain is exception-safe, and so is a disposal group ----------

/// A throwing observer, and a throwing cleanup, with a harness that can see the
/// throw — vilan has no exception syntax, so `__step` is the only way a program
/// can report that the error really left the reactive core rather than being
/// swallowed there.
const THROWING_OBSERVER: &str = r#"import std::io::{ panic, print };
import std::reactive::{ Disposable, FlushPolicy, Owner, Signal, SignalCell, turn };

/// The harness runs each step inside a JS `try`/`catch` and prints what it
/// caught.
[extern("__step")]
external fun step(body: || void): void;

fun main() {
	let a: SignalCell<i32> = Signal::new(0);
	let b: SignalCell<i32> = Signal::new(0);
	let seen_a: SignalCell<i32> = Signal::new(0);
	let seen_b: SignalCell<i32> = Signal::new(0);
	let armed: SignalCell<bool> = Signal::new(true);

	let _watch_a = a.on_change(|value: i32| {
		seen_a.set(value);
		if armed.get() {
			armed.set(false);
			panic("observer exploded");
		}
	});
	let _watch_b = b.on_change(|value: i32| {
		seen_b.set(value);
	});

	// Both writes land in ONE turn, so both observers are in one wave.
	step(|| {
		turn(FlushPolicy::AtEnd, || {
			a.set(1);
			b.set(1);
		});
	});
	print(i"after-throw:a={seen_a.get()} b={seen_b.get()}");

	// The scheduler survived. These two have NO ambient turn, so they resolve
	// through `draining_turns.last()` — which is exactly where the stuck turn
	// used to be, swallowing every write in the program from here on.
	a.set(2);
	b.set(2);
	print(i"after-recovery:a={seen_a.get()} b={seen_b.get()}");

	// A disposal group is a list of promises, so it FINISHES past a throwing
	// cleanup and raises the failure afterwards.
	let owner = Owner::new();
	let released: SignalCell<i32> = Signal::new(0);
	owner.defer(|| {
		released.set_with(|n| n + 1);
	});
	owner.defer(|| {
		panic("cleanup exploded");
	});
	owner.defer(|| {
		released.set_with(|n| n + 1);
	});
	step(|| {
		owner.dispose();
	});
	print(i"released={released.get()}");
}
"#;

#[test]
fn b292_a_throwing_observer_leaves_the_turn_drainable_and_the_error_visible() {
    // B292. `drain` set `draining = true`, pushed the turn onto
    // `draining_turns`, and restored neither on the way out of a throw, so one
    // observer that panicked — or one host API that refused its argument, the
    // Chrome `insertRule` shape the item names — left the turn DRAINING
    // forever and on the stack. Every later write then resolved to it
    // (`Signal::notify`'s no-ambient-turn arm joins `draining_turns.last()`)
    // and enqueued into a queue nothing would ever flush: the reactive graph
    // was off for the rest of the session, with no second error to show for
    // it. `after-recovery` is that line.
    //
    // The drain restores under a FINALLY and lets the throw keep unwinding
    // untouched, rather than catching and continuing the wave: a throwing
    // observer means the graph is mid-update, and the write that started the
    // drain is the honest place for the failure to surface. `b=0` on the
    // `after-throw` line is the price, and it is deliberate. A disposal group
    // is the opposite case and gets the opposite guard — see `released`.
    let harness = format!(
        r#"{DOM_STUB}
global.__step = (body) => {{
    try {{
        body();
    }} catch (error) {{
        console.log(`caught:${{error && error.message ? error.message : String(error)}}`);
    }}
}};
require("./app.js");
"#
    );
    let stdout = build_and_run("throwing_observer", THROWING_OBSERVER, &harness, &[]);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            // The throw left the core with its own message.
            "caught:observer exploded",
            // The wave was abandoned at the thrower, deliberately.
            "after-throw:a=1 b=0",
            // And the scheduler is usable again — this is the line that was
            // `a=1 b=0` for the rest of the session.
            "after-recovery:a=2 b=2",
            "caught:cleanup exploded",
            // Both surviving cleanups ran: a disposal group finishes. Before
            // B292 this was 1, and B291's idempotence made it permanent — a
            // second `dispose` no longer picks up what the throw skipped.
            "released=2",
        ],
        "the reactive core's exception safety went differently:\n{stdout}"
    );
}
