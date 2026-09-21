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
//! as the thing they belong to. The walk reports them as TWO strongly connected
//! components, not three cycles — loops that share a node are one component —
//! so the mounted line reads `cycles=2` (measured at Order 38: a component of
//! 64 around the element tree, V4's, and one of 30 around the session's
//! closures), and that figure is this paragraph, not a disagreement with it. What must be ZERO is what survives the
//! teardown, and that is what this asserts.
//!
//! Every one of V1, V3 and V5 was proven to redden it by planting the bug back:
//! `Signal`'s notify closure capturing the signal instead of the value cell, the
//! write-back listener moved inline beside the effect that captures the element,
//! and `dispose` leaving `DuplexEnd.me` set.

use std::path::{Path, PathBuf};
use std::process::Command;

mod support;

/// A fresh temp directory for one test's project tree.
fn temp_project(tag: &str) -> PathBuf {
    let dir = support::scratch_root().join(format!(
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
const DOM_STUB: &str = concat!(
    include_str!("support/dom/stub.js"),
    include_str!("support/dom/reactive_lifetimes.js"),
);

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
import std::ui::{ View, each, mount_root, view };

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
		.child(view("ul").child(each(items, |item| item, |item| row(item))))
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
    // detail of the exemplar rather than a law. Recording it means PRINTING it
    // — `cargo nextest run -p vilan-cli --test reactive_lifetimes -E
    // 'test(a_disposed_exemplar_holds_no_reactive_cycle)' --no-capture` — so
    // the number a change to the graph moves can be read off the gate that
    // measures it instead of re-derived by hand (C14 S3 read it this way).
    println!("{stdout}");
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
/// handle's reply, a `each` row rebuilt while its first fetch is out.
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

// --- A110 door 1: a disposed observer never fires ---------------------------

/// The item's probe, inline: a `route` cell, a `shell` derived from it, an outer
/// observer of `shell` that disposes a boundary, and an inner observer of
/// `route` owned by that boundary. One `route.set`, no ambient turn.
///
/// `SignalCell::notify`'s inline arm walks a SNAPSHOT of the subscriber list.
/// The derivation's subscriber runs first and cascades depth-first — derived
/// `set` → the outer observer → the boundary disposed and the inner
/// subscription detached from the LIST — but the snapshot the loop is walking
/// still holds it, so it was called next. For a `swap` that is the leak the
/// item names: `live_owner` (already disposed) no-ops, `render` runs under a
/// fresh `Owner` the boundary's `defer` has already passed, and `open_row`
/// inserts before an anchor `region.close()` removed.
const A110_DISPOSED_OBSERVER_INLINE: &str = r#"import std::io::print;
import std::reactive::{ Disposable, Owner, Signal, SignalCell, Source, owner_scope };

fun main() {
	let route: SignalCell<i32> = Signal::new(0);
	let shell: SignalCell<i32> = route.map(|value| value / 10);
	let boundary = Owner::new();
	let fired: SignalCell<i32> = Signal::new(0);

	owner_scope.run(boundary, || {
		route.effect_on_change(|value: i32| {
			print(i"INNER fired with {value} (boundary disposed: {boundary.is_disposed()})");
			fired.set_with(|count| count + 1);
		});
	});
	let _outer = shell.on_change(|_projected: i32| {
		print("outer disposes the inner boundary");
		boundary.dispose();
	});

	// No ambient turn: `notify` takes the inline arm.
	route.set(10);
	print(i"inline-fired={fired.get()}");
	print(i"inline-subscribers={route.subscribers.read().len()}");
}
"#;

#[test]
fn a110_a_disposed_observer_does_not_fire_from_an_inline_notifys_snapshot() {
    // Before door 1 this printed `INNER fired with 10 (boundary disposed:
    // true)` and `inline-fired=1` — the scrub's own comment promises "a
    // disposed observer never fires", and that held only under an ambient
    // turn. The flag is `Subscriber.live`, lowered by `Subscription::dispose`
    // and read by both notification loops.
    let harness = format!("{DOM_STUB}\nrequire(\"./app.js\");\n");
    let stdout = build_and_run("a110_inline", A110_DISPOSED_OBSERVER_INLINE, &harness, &[]);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "outer disposes the inner boundary",
            // No `INNER fired` line at all: the inner observer is off.
            "inline-fired=0",
            // …and detached, which is what it always was.
            "inline-subscribers=1",
        ],
        "a disposed observer must not fire from an inline notify's snapshot; got:\n{stdout}"
    );
}

/// The same law inside a turn, at the two places the pending-queue scrub cannot
/// reach.
///
/// **One wave.** `drain` takes a whole wave OUT of `pending` before running it,
/// so a dispose from inside that wave scrubs a queue the remaining subscribers
/// are no longer in. Both observers here stand directly on `route`, the
/// disposing one registered first, so they land in one wave in subscription
/// order.
///
/// **Through a derivation.** The inner subscriber is reached one derivation
/// later than the disposer, so it is enqueued while the wave is already
/// running. Since A110 door 2 the derivation runs in the wave's FIRST phase, so
/// the inner effect joins the same effect wave; before door 2 it waited for
/// wave 2. Either way the disposer runs first and the wave the inner is in has
/// already been taken out of `pending`, which is the face no removal can reach.
///
/// **Both halves register the DISPOSER first**, and that is load-bearing since
/// door 2: the effect queue runs in ascending subscriber id, so the creation
/// order is what puts the disposer ahead of its victim. A hand-written pair in
/// the other order is two INDEPENDENT observers of one source, whose relative
/// order is explicitly not part of the contract (reactive-turns.md §7.10) — and
/// with the victim first the victim simply fires, live, which measures nothing
/// about door 1. A genuinely nested form has this order by construction: the
/// parent's effect is created when the parent is placed, the child's when the
/// parent's render runs.
const A110_DISPOSED_OBSERVER_IN_A_TURN: &str = r#"import std::io::print;
import std::reactive::{
	Disposable, FlushPolicy, Owner, Signal, SignalCell, Source, owner_scope, turn,
};

fun main() {
	// One wave: the outer is registered FIRST, so it runs before the inner.
	let route: SignalCell<i32> = Signal::new(0);
	let boundary = Owner::new();
	let fired: SignalCell<i32> = Signal::new(0);
	let _outer = route.on_change(|_value: i32| {
		print("one-wave: outer disposes the inner boundary");
		boundary.dispose();
	});
	owner_scope.run(boundary, || {
		route.effect_on_change(|value: i32| {
			print(i"one-wave: INNER fired with {value}");
			fired.set_with(|count| count + 1);
		});
	});
	turn(FlushPolicy::AtEnd, || {
		route.set(10);
	});
	print(i"one-wave-fired={fired.get()}");

	// Through a derivation: the inner observes a DERIVATION of the source, so
	// it is enqueued while the wave is already running and the outer disposes
	// it from inside that wave. The outer is created FIRST, so ascending
	// subscriber id runs it first (A110 door 2).
	let path: SignalCell<i32> = Signal::new(0);
	let gate: SignalCell<i32> = path.map(|value| value / 10);
	let later_boundary = Owner::new();
	let later_fired: SignalCell<i32> = Signal::new(0);
	let _later_outer = path.on_change(|_value: i32| {
		print("later-wave: outer disposes the inner boundary");
		later_boundary.dispose();
	});
	owner_scope.run(later_boundary, || {
		gate.effect_on_change(|value: i32| {
			print(i"later-wave: INNER fired with {value}");
			later_fired.set_with(|count| count + 1);
		});
	});
	turn(FlushPolicy::AtEnd, || {
		path.set(10);
	});
	print(i"later-wave-fired={later_fired.get()}");
}
"#;

#[test]
fn a110_a_disposed_observer_does_not_fire_from_a_wave_the_drain_already_took_out() {
    // Both halves printed an `INNER fired` line and a `-fired=1` before door 1.
    // The `later-wave` half is the one the item's "pending-queue scrub cannot
    // help" sentence undercounts: the scrub is not merely too late there, it
    // does not run — the disposer is an owner's cleanup closure whose captured
    // turn is `None`, so `dispose` scrubs nothing while the subscriber sits in
    // the draining turn's own queue.
    let harness = format!("{DOM_STUB}\nrequire(\"./app.js\");\n");
    let stdout = build_and_run(
        "a110_in_a_turn",
        A110_DISPOSED_OBSERVER_IN_A_TURN,
        &harness,
        &[],
    );
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "one-wave: outer disposes the inner boundary",
            "one-wave-fired=0",
            "later-wave: outer disposes the inner boundary",
            "later-wave-fired=0",
        ],
        "a disposed observer must not fire from a wave already taken out of the \
         queue, nor from a queue the disposer's own turn cannot reach; got:\n{stdout}"
    );
}

// --- FIND-2 (papers-38): the scrub resolves its turn the way `notify` does ---

/// `Subscription::dispose` read its turn with `turn_scope.get_safe()` alone,
/// while `SignalCell::notify` and `defer_to_turn` both read "the ambient turn,
/// ELSE the currently DRAINING one". `turn` calls `drain(fresh)` AFTER
/// `turn_scope.run(fresh, body)` has returned, so a drain runs OUTSIDE its own
/// turn's context extent — and a dispose reached from inside a notify is every
/// form's teardown, since a form's own effect is what disposes the previous
/// instantiation. Those disposals read `None` and scrubbed nothing at all.
///
/// The shape, one cell and two positions in one wave: an effect that PARKS the
/// victim (it writes a second cell the victim observes, and the wave it lands
/// in has already been taken out of `pending`), then — at a higher subscriber
/// id, so it runs second since A110 door 2 ordered the effect queue — the
/// disposer. That is the only arrangement in which the scrub has anything to
/// do, and it is the arrangement a nested form has: the enclosing form's
/// teardown runs while the subtree's own effects are parked.
///
/// Before door 2 this was written as a DERIVATION parking the victim for wave 2
/// with the disposer second in wave 1. Door 2 runs derivations in the wave's
/// first phase, so the victim joined the same effect wave and the queue the
/// scrub reads was empty by the time the disposer ran — the assertion would
/// still have read 0, vacuously. An effect doing the parking puts the entry
/// back where the scrub can be measured.
///
/// The program drives its OWN `Turn` rather than using `turn(..)`, because the
/// queue length has to be read from INSIDE the drain — which is the only place
/// the scrub's effect is visible now that the liveness flag stops the delivery
/// either way. `pending-after-dispose` is that reading, and it is the
/// assertion that belongs to this fix rather than to door 1.
const FIND2_A_DISPOSE_FROM_INSIDE_A_DRAIN: &str = r#"import std::io::print;
import std::reactive::{
	Disposable, Owner, Signal, SignalCell, Source, Turn, drain, owner_scope, turn_scope,
};
import std::shared::Shared;

/// BOTH of a turn's queues, since A110 door 2 split them: a parked effect sits
/// in `pending` and a derivation in `pending_derived`, and the scrub's claim is
/// about whichever one holds the entry.
fun parked(turn: Turn): i32 {
	turn.pending.read().len() + turn.pending_derived.read().len()
}

fun main() {
	let own_turn = Turn::new();
	let trigger: SignalCell<i32> = Signal::new(0);
	let relay: SignalCell<i32> = Signal::new(0);
	let boundary = Owner::new();
	let fired: Shared<i32> = Shared::new(0);

	// The VICTIM stands on `relay`, not on `trigger`, so it is not in the wave
	// — it is PARKED into the queue by an effect that is.
	owner_scope.run(boundary, || {
		relay.effect_on_change(|value: i32| {
			fired.write() = fired.read() + 1;
			print(i"VICTIM fired with {value} (boundary disposed: {boundary.is_disposed()})");
		});
	});
	// First in the wave (the lower id): park the victim.
	let _parker = trigger.on_change(|value: i32| {
		relay.set(value * 10);
	});
	// Second in the wave: the dispose every form's teardown is, reached from
	// inside the drain with no ambient turn of its own.
	let _disposer = trigger.on_change(|_value: i32| {
		boundary.dispose();
		print(i"pending-after-dispose={parked(own_turn)}");
	});

	// `run` enqueues; `drain` settles, from OUTSIDE the context extent.
	turn_scope.run(own_turn, || {
		trigger.set(1);
	});
	drain(own_turn);
	print(i"victim-fired={fired.read()}");

	// The control: the same dispose made from the turn's BODY, where
	// `turn_scope` IS established and the scrub always worked.
	let second_turn = Turn::new();
	let second_trigger: SignalCell<i32> = Signal::new(0);
	let second_derived: SignalCell<i32> = second_trigger.map(|value| value * 10);
	let second_boundary = Owner::new();
	let second_fired: Shared<i32> = Shared::new(0);
	owner_scope.run(second_boundary, || {
		second_derived.effect_on_change(|value: i32| {
			second_fired.write() = second_fired.read() + 1;
			print(i"CONTROL fired with {value}");
		});
	});
	turn_scope.run(second_turn, || {
		second_trigger.set(1);
		second_boundary.dispose();
		print(i"control-pending-after-dispose={parked(second_turn)}");
	});
	drain(second_turn);
	print(i"control-fired={second_fired.read()}");
}
"#;

#[test]
fn find2_a_dispose_reached_from_inside_a_drain_scrubs_the_draining_turns_queue() {
    // TWO claims, red against two different plants.
    //
    // `victim-fired=0` is A110 door 1's (the liveness flag): on `0fa109eb` the
    // victim fired once with its boundary already disposed.
    //
    // `pending-after-dispose=0` is THIS fix's: on the door-1 commit it still
    // read 1, because `dispose` asked `turn_scope.get_safe()` and nothing
    // else. A disposed subscriber left in the queue is a disposed subtree's
    // notify closure — and everything it captured — held reachable until the
    // drain ends, which for a `swap` tearing down a thousand rows inside one
    // wave is a thousand of them.
    let harness = format!("{DOM_STUB}\nrequire(\"./app.js\");\n");
    let stdout = build_and_run(
        "find2_scrub",
        FIND2_A_DISPOSE_FROM_INSIDE_A_DRAIN,
        &harness,
        &[],
    );
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "pending-after-dispose=0",
            "victim-fired=0",
            // 1, and correctly: the dispose happens BEFORE the drain, so the
            // only thing queued is the DERIVATION's own entry — in
            // `pending_derived` since door 2, which is why the program counts
            // both queues — and the victim has not been enqueued yet. The
            // control's claim is the line below it.
            "control-pending-after-dispose=1",
            "control-fired=0",
        ],
        "a dispose from inside a drain must scrub the draining turn's queue, and \
         the victim must not fire either way; got:\n{stdout}"
    );
}

// --- A110 door 2: derivations to a fixpoint, then effects in ascending id ----

/// The ordering contract (`reactive-turns.md` §7, RULED 2026-09-21), on the
/// shape it is a contract ABOUT: **nested forms**.
///
/// Two halves, and the first is why the second is possible. Both stand on one
/// cell with the outer observer reaching it one derivation further away — the
/// idiom the guide teaches, because projecting the outer key is what stops the
/// outer form rebuilding on every change — and in both the INNER effect is
/// created by the OUTER's own first run, which is what makes the pair nested
/// rather than two independent observers of one source (`fresh_id` is
/// monotonic, so "created by my render" is "has a higher id than me").
///
/// **`order:`** — nothing is disposed, so both fire and the order is readable:
/// the outer's line comes first. Before door 2 the inner's came first, and by a
/// whole wave — `drain` ran the wave in SUBSCRIPTION order, where the outer is
/// not on the cell's list at all (it rides the derived cell), so no order read
/// off one cell's list could ever put it first.
///
/// **`nested:`** — the outer's run DISPOSES the previous instantiation, which
/// is what every form's teardown does. The inner observer of the instantiation
/// being replaced fires ZERO times, where under door 1 alone it fired once,
/// live, for a value the outer was about to exclude — a wasted subtree build,
/// and a panic if that arm is `unreachable`. `built`/`torn` stay a balanced
/// pair, which is door 1's claim and is unaffected.
///
/// Order among INDEPENDENT observers of one source is explicitly NOT part of
/// the contract (§7.10), so neither half is written on a hand-made sibling
/// pair — and A110's own probe, which is one, is not this item's pin.
const A110_DOOR2_NESTED_FORMS: &str = r#"import std::io::print;
import std::reactive::{
	Disposable, FlushPolicy, Owner, Signal, SignalCell, Source, get_owner, owner_scope,
	run_with_owner, turn,
};

fun main() {
	// --- order: the outer runs first, and both run.
	let source: SignalCell<i32> = Signal::new(0);
	let coarse: SignalCell<i32> = source.map(|value| value / 10);
	let host = Owner::new();
	mut wired = false;
	run_with_owner(host, || {
		coarse.effect(|value: i32| {
			print(i"order: OUTER with {value}");
			if !wired {
				wired = true;
				source.effect_on_change(|inner: i32| {
					print(i"order: inner with {inner}");
				});
			}
		});
	});
	turn(FlushPolicy::AtEnd, || {
		source.set(10);
	});

	// --- nested: the outer's run replaces the instantiation the inner belongs
	// to, so the inner never runs at all.
	let route: SignalCell<i32> = Signal::new(0);
	let shell: SignalCell<i32> = route.map(|value| value / 10);
	let boundary = Owner::new();
	let built: SignalCell<i32> = Signal::new(0);
	let torn: SignalCell<i32> = Signal::new(0);
	let inner_fired: SignalCell<i32> = Signal::new(0);
	mut instance: Option<Owner> = None;
	run_with_owner(boundary, || {
		shell.effect(|value: i32| {
			print(i"nested: OUTER renders for shell={value}");
			match instance {
				Some(let previous) => previous.dispose(),
				None => {},
			}
			let fresh = Owner::new();
			instance = Some(fresh);
			owner_scope.run(fresh, || {
				built.set_with(|count| count + 1);
				get_owner().defer(|| {
					torn.set_with(|count| count + 1);
				});
				route.effect_on_change(|inner: i32| {
					print(i"nested: inner fired with {inner}");
					inner_fired.set_with(|count| count + 1);
				});
			});
		});
	});
	turn(FlushPolicy::AtEnd, || {
		route.set(10);
	});
	print(i"nested: built={built.get()} torn={torn.get()} inner-fired={inner_fired.get()}");
}
"#;

#[test]
fn a110_door2_a_parent_forms_effect_runs_before_anything_its_render_created() {
    // Red on the door-1 commit, in both halves and for the same reason. The
    // `order:` half printed `inner` before the second `OUTER` line (the outer
    // was a whole wave behind, since it reaches the source through the
    // derivation). The `nested:` half printed a `nested: inner fired with 10`
    // line and `built=3 torn=2 inner-fired=1` — the wasted build A110 was
    // filed for.
    let harness = format!("{DOM_STUB}\nrequire(\"./app.js\");\n");
    let stdout = build_and_run("a110_door2_nested", A110_DOOR2_NESTED_FORMS, &harness, &[]);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            // The eager first call, before anything has changed.
            "order: OUTER with 0",
            // One `set`, one wave: the derivation settles in phase 1, then the
            // effects run in ascending id — the parent's first.
            "order: OUTER with 1",
            "order: inner with 10",
            "nested: OUTER renders for shell=0",
            "nested: OUTER renders for shell=1",
            // No `nested: inner fired` line at all, and the pair balances: two
            // instantiations built, the first one torn down.
            "nested: built=2 torn=1 inner-fired=0",
        ],
        "a parent form's effect must run before anything its render created, and \
         an instantiation the parent has replaced must not run at all; got:\n{stdout}"
    );
}

/// The other half of the rule: **derivations run to a FIXPOINT** before any
/// effect, so an effect never reads a half-updated graph.
///
/// The shape is a diamond with a two-deep chain on one arm — an effect standing
/// on the root that READS a derivation two hops away. Under door 2 the chain is
/// pulled all the way through in phase 1 and the effect reads 11. Before it, the
/// effect was in the same wave as the first link of the chain and ran after it
/// in subscription order, so it read the SECOND link's stale value: `5/3`, where
/// 3 is what `twice` held before the write and 11 is what it holds one wave
/// later. That is a glitch in the sense `enqueue`'s dedup already promised not
/// to have — "each subscriber fires once" extended to "each subscriber fires
/// once, on final values".
///
/// It fires ONCE, which is the second claim: the chain's two intermediate
/// writes do not each wake the effect.
const A110_DOOR2_DERIVATION_FIXPOINT: &str = r#"import std::io::print;
import std::reactive::{
	FlushPolicy, Owner, Signal, SignalCell, Source, run_with_owner, turn,
};

fun main() {
	let root: SignalCell<i32> = Signal::new(1);
	let once: SignalCell<i32> = root.map(|value| value * 2);
	let twice: SignalCell<i32> = once.map(|value| value + 1);
	let seen: SignalCell<str> = Signal::new("");
	let watcher = Owner::new();
	run_with_owner(watcher, || {
		root.effect_on_change(|value: i32| {
			seen.set_with(|log| i"{log}{value}/{twice.get()},");
		});
	});
	print(i"before root={root.get()} twice={twice.get()}");
	turn(FlushPolicy::AtEnd, || {
		root.set(5);
	});
	print(i"fixpoint={seen.get()}");
}
"#;

#[test]
fn a110_door2_an_effect_reads_a_derivation_chains_final_value_once() {
    // Red on the door-1 commit: `fixpoint=5/3,` — the effect ran between the
    // chain's first and second link and read the value `twice` held BEFORE the
    // write. The count is the same either way (the dedup was never the defect),
    // so the claim is the VALUE.
    let harness = format!("{DOM_STUB}\nrequire(\"./app.js\");\n");
    let stdout = build_and_run(
        "a110_door2_fixpoint",
        A110_DOOR2_DERIVATION_FIXPOINT,
        &harness,
        &[],
    );
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec!["before root=1 twice=3", "fixpoint=5/11,"],
        "an effect must see a derivation chain settled, and see it once; got:\n{stdout}"
    );
}

// --- A114: an effect with an OWNER PER RUN ----------------------------------

/// `Source::scoped_effect` and the free `on_cleanup` (tracker A114, R2 at Order
/// 39's GO), on the sequence A113 row 4 named as missing: a per-run cleanup.
///
/// Four sections, and the first is the item's own user-code probe
/// (`sweeps/order38/probes/scoped_effect.vl`) run through the std form. Its
/// output here is byte-identical to the probe's on the same tree, which is the
/// claim that the mechanism moved into std unchanged.
///
/// **`inline:`** — one owner per run. Each run registers an `on_cleanup` and a
/// nested `effect` on a SECOND source; the cleanup runs before the next run, the
/// nested subscription dies with the run (so `other`'s changes reach exactly the
/// current run), and the LAST run is released by the enclosing boundary —
/// `other-subscribers=0` after it, which is the leak a form-less per-run owner
/// has no other way to close.
///
/// **`turn:`** — the same inside a turn, counted: runs and cleanups differ by
/// exactly one while the effect is live (the current run has not been cleaned
/// up yet) and are EQUAL once the boundary goes. Disposal count = creation
/// count is the item's own acceptance line.
///
/// **`plain:`** — `on_cleanup` under an ordinary boundary, which is the other
/// half of "one name, the ambient owner decides": it runs ONCE, at teardown,
/// and not per change.
///
/// **`lazy:`** — `scoped_effect_on_change` makes no immediate run, exactly as
/// `effect_on_change` makes no immediate call.
const A114_SCOPED_EFFECT: &str = r#"import std::io::print;
import std::reactive::{
	Disposable, FlushPolicy, Owner, Signal, SignalCell, Source, on_cleanup, run_with_owner,
	turn,
};

fun main() {
	let id: SignalCell<i32> = Signal::new(1);
	let other: SignalCell<str> = Signal::new("a");
	let boundary = Owner::new();
	run_with_owner(boundary, || {
		id.scoped_effect(|value: i32| {
			print(i"inline: run {value}");
			on_cleanup(|| print(i"inline: cleanup {value}"));
			other.effect(|text: str| print(i"inline: inner {value} sees {text}"));
		});
	});
	id.set(2);
	other.set("b");
	id.set(3);
	print("inline: dispose boundary");
	boundary.dispose();
	id.set(4);
	other.set("c");
	print(i"inline: other-subscribers={other.subscribers.read().len()}");

	let key: SignalCell<i32> = Signal::new(0);
	let runs: SignalCell<i32> = Signal::new(0);
	let cleanups: SignalCell<i32> = Signal::new(0);
	let scope = Owner::new();
	run_with_owner(scope, || {
		key.scoped_effect(|_value: i32| {
			runs.set_with(|count| count + 1);
			on_cleanup(|| cleanups.set_with(|count| count + 1));
		});
	});
	turn(FlushPolicy::AtEnd, || {
		key.set(1);
	});
	turn(FlushPolicy::AtEnd, || {
		key.set(2);
	});
	print(i"turn: runs={runs.get()} cleanups={cleanups.get()}");
	scope.dispose();
	print(i"turn: after-dispose runs={runs.get()} cleanups={cleanups.get()}");

	let ticks: SignalCell<i32> = Signal::new(0);
	let plain = Owner::new();
	run_with_owner(plain, || {
		on_cleanup(|| print("plain: cleanup"));
		ticks.effect(|value: i32| print(i"plain: tick {value}"));
	});
	ticks.set(1);
	ticks.set(2);
	plain.dispose();
	print("plain: disposed");

	let lazy_key: SignalCell<i32> = Signal::new(0);
	let lazy_runs: SignalCell<i32> = Signal::new(0);
	let lazy_scope = Owner::new();
	run_with_owner(lazy_scope, || {
		lazy_key.scoped_effect_on_change(|_value: i32| {
			lazy_runs.set_with(|count| count + 1);
		});
	});
	print(i"lazy: after-attach runs={lazy_runs.get()}");
	lazy_key.set(1);
	print(i"lazy: after-change runs={lazy_runs.get()}");
	lazy_scope.dispose();
}
"#;

#[test]
fn a114_a_scoped_effect_releases_each_runs_registrations_before_the_next_run() {
    let harness = format!("{DOM_STUB}\nrequire(\"./app.js\");\n");
    let stdout = build_and_run("a114_scoped", A114_SCOPED_EFFECT, &harness, &[]);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            // Run 1, and its nested subscription sees the other source's
            // current value through the eager `effect`.
            "inline: run 1",
            "inline: inner 1 sees a",
            // The cleanup runs BEFORE the next run, not after it.
            "inline: cleanup 1",
            "inline: run 2",
            "inline: inner 2 sees a",
            // `other` changed: exactly the CURRENT run's nested subscription
            // hears it. Under a plain `effect` there would be two by now.
            "inline: inner 2 sees b",
            "inline: cleanup 2",
            "inline: run 3",
            "inline: inner 3 sees b",
            "inline: dispose boundary",
            // The last run is the BOUNDARY's to release, which is what the
            // `on_cleanup` inside `scoped_runner` is for.
            "inline: cleanup 3",
            "inline: other-subscribers=0",
            // Three runs, two cleanups: the live run has not been cleaned up.
            "turn: runs=3 cleanups=2",
            // …and then it has. Disposal count = creation count.
            "turn: after-dispose runs=3 cleanups=3",
            // `on_cleanup` under an ordinary boundary: once, at teardown.
            "plain: tick 0",
            "plain: tick 1",
            "plain: tick 2",
            "plain: cleanup",
            "plain: disposed",
            // The lazy twin makes no immediate run.
            "lazy: after-attach runs=0",
            "lazy: after-change runs=1",
        ],
        "a scoped effect must give every run its own owner and release it before \
         the next run; got:\n{stdout}"
    );
}

/// A run whose body THROWS still had its owner installed as the current one, so
/// whatever it registered before the throw is released by the next run — and by
/// the boundary if there is no next run.
///
/// The order inside the observer is what makes this true and is written that
/// way deliberately: release the previous run, install the fresh owner, THEN
/// call the body. A body that throws with the fresh owner already installed
/// leaks nothing; a body called before the install would leak everything it had
/// registered.
const A114_THROWING_BODY: &str = r#"import std::io::{ panic, print };
import std::reactive::{
	Disposable, Owner, Signal, SignalCell, Source, guarded, on_cleanup, run_with_owner,
};

fun main() {
	let id: SignalCell<i32> = Signal::new(0);
	let released: SignalCell<i32> = Signal::new(0);
	let scope = Owner::new();
	run_with_owner(scope, || {
		id.scoped_effect(|value: i32| {
			on_cleanup(|| released.set_with(|count| count + 1));
			if value == 1 {
				panic("the body refused");
			}
		});
	});
	let failure = guarded(|| {
		id.set(1);
	});
	print(i"caught={failure.is_some()} released={released.get()}");
	id.set(2);
	print(i"after-next released={released.get()}");
	scope.dispose();
	print(i"after-dispose released={released.get()}");
}
"#;

#[test]
fn a114_a_throwing_scoped_effect_body_does_not_leak_the_run_it_started() {
    let harness = format!("{DOM_STUB}\nrequire(\"./app.js\");\n");
    let stdout = build_and_run("a114_throwing", A114_THROWING_BODY, &harness, &[]);
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            // The throw is caught by the program; run 0's cleanup ran on the
            // way in, which is the one release that had to happen.
            "caught=true released=1",
            // The FAILED run's own cleanup is released by the next run.
            "after-next released=2",
            // And the last run by the boundary: three runs, three releases.
            "after-dispose released=3",
        ],
        "a throwing run must not leak what it registered before it threw; \
         got:\n{stdout}"
    );
}
