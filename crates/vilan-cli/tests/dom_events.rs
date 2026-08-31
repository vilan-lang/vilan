//! The `std::dom` event surface's runtime gates (proposal/router.md §5; backlog
//! A27, kolt.local 037).
//!
//! Three capabilities land together because the exhibit that named them — kolt's
//! sidebar-resize drag — needs all three at once, and none of them can be
//! measured by compiling: whether a listener is really registered, really
//! removed, and really handed the coordinates it claims are only observable in a
//! running program. So these are e2e legs in the shape `router.rs` and
//! `reactive_lifetimes.rs` established: a browser-target app built with the real
//! CLI, run under node against a DOM stub, asserting on what the running program
//! did to the host.
//!
//! The stub is deliberately stricter than the ones next door in one respect: its
//! `removeEventListener` is real (identity-matched, exactly as a browser's is),
//! so a `dispose` that hands back a *different* closure than the one registered
//! silently removes nothing and every negative assertion here fails. That is the
//! property the whole `listen` design rests on (§5.2) and it is not otherwise
//! checkable.
//!
//! Four gates:
//!
//! 1. [`pointer_coordinates_read_the_event_the_host_dispatched`] — `pointer_x` /
//!    `pointer_y` over `clientX` / `clientY`, against real dispatched events.
//! 2. [`window_listen_registers_and_its_subscription_unhooks`] — the window is a
//!    listen target at all (A27's gap), and the negative half: after `dispose`,
//!    firing the event again does *nothing*.
//! 3. [`element_listen_registers_and_its_subscription_unhooks`] — the same verb
//!    on the other target, since `listen` is one shape over two targets.
//! 4. [`the_drag_exhibit_rewrites_onto_std_surface_alone`] — 037's acceptance
//!    test: kolt's `on_drag` with its three hand-rolled bindings deleted.
//!
//! And one marking gate,
//! [`the_event_surfaces_externs_are_marked_by_the_audit_rule`], over the
//! declarations themselves: `retains` says the host keeps what it is handed
//! (`lifetimes.md` §6.4 and its §S4 audit table), which registration does and
//! removal does not. It is a declaration gate rather than a behavioral one
//! because retention is not observable through a listener at all — a closure
//! cannot capture a resource, so there is nothing whose drop order could move.
//! The behavior of the flag itself is pinned language-side (`resources.rs`, the
//! S4 battery); what is unpinnable there and pinned here is which way each of
//! THESE six declarations is marked.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A fresh temp directory for one test's project tree.
fn temp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vilan_dom_events_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// A document, a window, and elements that all register AND remove listeners the
/// way a browser does — by handler identity. `count(target, event)` is what the
/// negative assertions read.
const DOM_STUB: &str = r#"class StubTarget {
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
    // Identity-matched, exactly as the DOM's is. A `dispose` that reconstructs
    // the handler instead of holding the registered one removes NOTHING here.
    removeEventListener(event, handler) {
        this.listeners[event] = (this.listeners[event] || []).filter(h => h !== handler);
    }
    count(event) { return (this.listeners[event] || []).length; }
    // Slice: a handler that disposes its own registration must not perturb the
    // iteration it is being dispatched from.
    fire(event, payload = {}) { for (const h of (this.listeners[event] || []).slice()) h(payload); }
    find(predicate) {
        if (predicate(this)) return this;
        for (const c of this.children) { const hit = c.find(predicate); if (hit) return hit; }
        return null;
    }
}

const documentRoot = new StubTarget("div");
global.document = {
    createElement: (tag) => new StubTarget(tag),
    createElementNS: (namespace, tag) => new StubTarget(tag),
    getElementById: () => documentRoot,
    querySelector: () => null,
    querySelectorAll: () => [],
};
global.location = { pathname: "/" };
global.history = { pushState(state, title, path) { global.location.pathname = path; } };
global.window = new StubTarget("window");

let failures = 0;
const assert = (condition, message) => {
    if (!condition) { failures += 1; console.log("FAIL - " + message); }
    else console.log("ok   - " + message);
};
const done = () => process.exit(failures === 0 ? 0 : 1);
"#;

/// Builds `app.vl` for the browser with the real CLI and runs `harness.js` under
/// node, returning its stdout. Fails loudly with both streams.
fn build_and_run(tag: &str, app: &str, harness: &str) -> String {
    let dir = temp_project(tag);
    write(
        &dir,
        "vilan.toml",
        &format!(
            "[package]\nname = \"dom_events_{tag}\"\nroot = \".\"\nentry = \"app.vl\"\ntarget = \"browser\"\n"
        ),
    );
    write(&dir, "app.vl", app);
    write(&dir, "harness.js", harness);

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
        "dom-events harness failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
    stdout
}

// --- 1. The pointer accessors, against real dispatched events ----------------

/// `clientX`/`clientY` are read off the event the host dispatched, not off
/// anything the program captured — the whole reason the accessors live on
/// `Event` (§5.3). Two different events in one session, so a cached first read
/// would be visible.
const POINTER_COORDINATES: &str = r#"import std::io::print;
import std::dom::{ Event, get_element_by_id, window };

fun main() {
	let target = get_element_by_id("app");
	target.on_event("pointerdown", |event| {
		print(i"element {event.pointer_x()},{event.pointer_y()}");
	});
	window().on_event("pointermove", |event| {
		print(i"window {event.pointer_x()},{event.pointer_y()}");
	});
}
main();
"#;

#[test]
fn pointer_coordinates_read_the_event_the_host_dispatched() {
    let harness = format!(
        r#"{DOM_STUB}
require("./app.js");
documentRoot.fire("pointerdown", {{ clientX: 12.5, clientY: 40 }});
window.fire("pointermove", {{ clientX: 300, clientY: 7 }});
window.fire("pointermove", {{ clientX: -4, clientY: 0 }});
done();
"#
    );
    let stdout = build_and_run("pointer_coordinates", POINTER_COORDINATES, &harness);
    assert!(
        stdout.contains("element 12.5,40"),
        "pointer_x/pointer_y must read clientX/clientY off an element event; got:\n{stdout}"
    );
    assert!(
        stdout.contains("window 300,7") && stdout.contains("window -4,0"),
        "each dispatched event carries its own coordinates (no caching, negatives \
         and zero included); got:\n{stdout}"
    );
}

// --- 2/3. `listen` on both targets, and the negative -------------------------

/// One program exercising both targets. `report` hands control back to the
/// harness between phases, so the assertions run at chosen points inside the
/// program's own lifetime rather than after it has finished.
const LISTEN_AND_DISPOSE: &str = r#"import std::io::print;
import std::dom::{ Event, get_element_by_id, window };
import std::reactive::{ Disposable, Shared };

[extern("__phase")]
external fun phase(name: str, body: || void): void;

fun main() {
	let element = get_element_by_id("app");
	let element_hits: Shared<i32> = Shared::new(0);
	let window_hits: Shared<i32> = Shared::new(0);

	let element_subscription = element.listen("click", |_| {
		element_hits.write() = element_hits.read() + 1;
	});
	let window_subscription = window().listen("resize", |_| {
		window_hits.write() = window_hits.read() + 1;
	});

	phase("registered", || {});
	phase("after-dispose", || {
		element_subscription.dispose();
		window_subscription.dispose();
	});
	// Disposing twice is a no-op, not a second unhook: the release cell is
	// one-shot, which is what makes an owner safe to hand an already-disposed
	// handle (proposal/router.md §5.2).
	window_subscription.dispose();
	element_subscription.dispose();
	phase("after-second-dispose", || {});

	print(i"element_hits={element_hits.read()} window_hits={window_hits.read()}");
}
main();
"#;

#[test]
fn window_listen_registers_and_its_subscription_unhooks() {
    let stdout = listen_and_dispose_run("window_target");
    assert!(
        stdout.contains("ok   - window listener registered"),
        "the window must be a listen target at all (A27); got:\n{stdout}"
    );
    assert!(
        stdout.contains("ok   - window listener unhooked by dispose"),
        "disposing the Subscription must remove the window listener; got:\n{stdout}"
    );
    assert!(
        stdout.contains("ok   - firing after dispose delivered nothing"),
        "the negative: a fired event must not reach a disposed listener; got:\n{stdout}"
    );
}

#[test]
fn element_listen_registers_and_its_subscription_unhooks() {
    let stdout = listen_and_dispose_run("element_target");
    assert!(
        stdout.contains("ok   - element listener registered"),
        "`listen` is one verb over two targets; got:\n{stdout}"
    );
    assert!(
        stdout.contains("ok   - element listener unhooked by dispose"),
        "disposing the Subscription must remove the element listener; got:\n{stdout}"
    );
    assert!(
        stdout.contains("ok   - a second dispose unhooked nothing new"),
        "dispose is one-shot; got:\n{stdout}"
    );
}

/// Both gates read the same program — `listen` is one shape over two targets,
/// so the two claims are made about one run each, under their own project tag
/// (the two tests run concurrently and must not share a build directory).
fn listen_and_dispose_run(tag: &str) -> String {
    let harness = format!(
        r#"{DOM_STUB}
global.__phase = (name, body) => {{
    if (name === "registered") {{
        assert(window.count("resize") === 1, "window listener registered");
        assert(documentRoot.count("click") === 1, "element listener registered");
        window.fire("resize");
        documentRoot.fire("click");
    }}
    if (name === "after-dispose") {{
        body();
        assert(window.count("resize") === 0, "window listener unhooked by dispose");
        assert(documentRoot.count("click") === 0, "element listener unhooked by dispose");
        window.fire("resize");
        documentRoot.fire("click");
        assert(true, "firing after dispose delivered nothing");
    }}
    if (name === "after-second-dispose") {{
        // Nothing was re-registered in between, so "unhooked nothing new" is a
        // claim about dispose being idempotent, not about the counts moving.
        assert(window.count("resize") === 0 && documentRoot.count("click") === 0,
            "a second dispose unhooked nothing new");
    }}
    body();
}};
require("./app.js");
done();
"#
    );
    let stdout = build_and_run(tag, LISTEN_AND_DISPOSE, &harness);
    // The counters are the other half of "delivered nothing": one delivery each,
    // from the pre-dispose fire only.
    assert!(
        stdout.contains("element_hits=1 window_hits=1"),
        "each listener must have fired exactly once — before dispose, never \
         after; got:\n{stdout}"
    );
    stdout
}

// --- 4. The exhibit: 037's acceptance test -----------------------------------

/// kolt's `View.on_drag` (`kolt/src/views.vl`), rewritten onto std surface
/// ALONE. Deleted against the original, with nothing added in their place:
/// `impl Event { pointer_x, pointer_y }`, and the
/// `window.addEventListener`/`window.removeEventListener` extern pair. The
/// `mut dispose` closure survives — it is the arming shape, not a workaround —
/// but its body is now two `Subscription::dispose` calls.
///
/// A drag is the canonical case for why element-local `on_event` cannot carry
/// this: the pointer leaves the element mid-drag.
const DRAG_EXHIBIT: &str = r#"import std::io::print;
import std::dom::{ Event, window };
import std::reactive::{ Disposable, Signal };
import std::ui::{ View, mount_root, view };

[extern("__phase")]
external fun phase(name: str): void;

struct DragEvent {
	drag_start: (f64, f64),
	drag_end: (f64, f64),
}

impl DragEvent {
	fun drag_offset(self): (f64, f64) {
		(self.drag_end.0 - self.drag_start.0, self.drag_end.1 - self.drag_start.1)
	}
}

impl View {
	fun on_drag(self, handler: || (|DragEvent| void, || void)): View {
		self.on_event("pointerdown", |down_event| {
			mut dispose;
			let drag_start = (down_event.pointer_x(), down_event.pointer_y());
			mut drag_event = DragEvent { drag_start = drag_start, drag_end = drag_start };
			let (callback_move_handler, callback_end_handler) = handler();
			let move_subscription = window().listen("pointermove", |move_event| {
				drag_event.drag_end = (move_event.pointer_x(), move_event.pointer_y());
				callback_move_handler(drag_event);
			});
			let up_subscription = window().listen("pointerup", |_| {
				callback_end_handler();
				dispose();
			});
			dispose = || {
				move_subscription.dispose();
				up_subscription.dispose();
			};
		})
	}
}

fun main() {
	let sidebar_width = Signal::new(240f);
	mount_root("app", || view("div").on_drag(|| {
		let start_width = sidebar_width.get();
		(|event: DragEvent| {
			let end_width = start_width + event.drag_offset().0;
			sidebar_width.set(end_width.clamp(100f, 800f));
		}, || {
			print(i"settled={sidebar_width.get()}");
		})
	}));
	phase("mounted");
}
main();
"#;

#[test]
fn the_drag_exhibit_rewrites_onto_std_surface_alone() {
    let harness = format!(
        r#"{DOM_STUB}
global.__phase = () => {{
    const handle = documentRoot.children[0];
    assert(window.count("pointermove") === 0 && window.count("pointerup") === 0,
        "no window listeners are armed before a drag");

    handle.fire("pointerdown", {{ clientX: 100, clientY: 10 }});
    assert(window.count("pointermove") === 1 && window.count("pointerup") === 1,
        "pointerdown armed exactly one window listener of each kind");

    window.fire("pointermove", {{ clientX: 160, clientY: 10 }});
    window.fire("pointerup", {{ clientX: 160, clientY: 10 }});
    assert(window.count("pointermove") === 0 && window.count("pointerup") === 0,
        "pointerup disposed both Subscriptions");

    // The negative that the hand-rolled removal existed to buy.
    window.fire("pointermove", {{ clientX: 900, clientY: 10 }});

    // A second drag must arm cleanly and leave nothing behind either: a
    // registration that never unhooks is a leak that only shows up on repeat.
    handle.fire("pointerdown", {{ clientX: 0, clientY: 0 }});
    assert(window.count("pointermove") === 1 && window.count("pointerup") === 1,
        "a second drag arms without accumulating");
    window.fire("pointermove", {{ clientX: 50, clientY: 0 }});
    window.fire("pointerup", {{}});
    assert(window.count("pointermove") === 0 && window.count("pointerup") === 0,
        "no window listeners survive the second drag");
    done();
}};
require("./app.js");
"#
    );
    let stdout = build_and_run("drag_exhibit", DRAG_EXHIBIT, &harness);
    for claim in [
        "no window listeners are armed before a drag",
        "pointerdown armed exactly one window listener of each kind",
        "pointerup disposed both Subscriptions",
        "a second drag arms without accumulating",
        "no window listeners survive the second drag",
    ] {
        assert!(
            stdout.contains(&format!("ok   - {claim}")),
            "the drag exhibit must hold `{claim}`; got:\n{stdout}"
        );
    }
    // 240 + 60, clamped to [100, 800] — the drag's arithmetic ran off the
    // pointer coordinates. Then the post-dispose `pointermove` at clientX=900
    // would have pushed it to the 800 clamp had the listener survived, so the
    // SECOND settle proves the negative numerically: 300 + 50.
    assert!(
        stdout.contains("settled=300"),
        "the first drag must settle at 240 + 60; got:\n{stdout}"
    );
    assert!(
        stdout.contains("settled=350") && !stdout.contains("settled=800"),
        "a move fired after dispose must not move the width (a surviving \
         listener would have clamped it to 800); got:\n{stdout}"
    );
}

// --- The marking gate: `retains` marks registration, never removal -----------

/// Every `[extern]` in `std::dom`'s event surface, as `(function name, binding
/// text, retains)`. Read off the declarations by pairing each attribute with the
/// `external fun` it precedes, so reordering, reindenting, or moving a
/// declaration between `impl` blocks cannot fake a pass.
fn event_surface_externs() -> Vec<(String, String, bool)> {
    let source = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std/src/browser/dom.vl"),
    )
    .expect("read std/src/browser/dom.vl");

    let mut found = Vec::new();
    for (index, _) in source.match_indices("[extern(") {
        let rest = &source[index + "[extern(".len()..];
        let close = rest.find(")]").expect("an unterminated [extern(..)]");
        let binding = rest[..close].to_string();
        let tail = &rest[close + ")]".len()..];
        let marker = "external fun ";
        let Some(start) = tail.find(marker) else {
            continue;
        };
        let name: String = tail[start + marker.len()..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        let retains = binding
            .rsplit(',')
            .next()
            .is_some_and(|last| last.trim() == "retains");
        found.push((name, binding, retains));
    }
    assert!(
        found.len() > 6,
        "the extern reader found almost nothing in dom.vl — the reader is broken, \
         not the declarations"
    );
    found
}

#[test]
fn the_event_surfaces_externs_are_marked_by_the_audit_rule() {
    let externs = event_surface_externs();
    let marking = |name: &str| -> (String, bool) {
        externs
            .iter()
            .find(|(found, _, _)| found == name)
            .map(|(_, binding, retains)| (binding.clone(), *retains))
            .unwrap_or_else(|| panic!("`std::dom` declares no `external fun {name}`"))
    };

    // Registration: the host STORES the vilan closure and calls it later, which
    // is the audit table's own sentence for `browser/dom.vl`.
    for name in ["on", "on_event"] {
        let (binding, retains) = marking(name);
        assert!(
            binding.contains("addEventListener"),
            "`{name}` should bind addEventListener; got `{binding}`"
        );
        assert!(
            retains,
            "`{name}` must be marked `retains`: the host keeps the handler past \
             the call. Declared as `[extern({binding})]`"
        );
    }
    // Both targets declare both verbs — `Element` and `Window` each contribute a
    // pair, so four registrations in total, and a missing one would silently
    // shrink the surface `listen` is built on.
    let registrations = externs
        .iter()
        .filter(|(_, binding, _)| binding.contains("addEventListener"))
        .count();
    assert_eq!(
        registrations, 4,
        "both targets must declare `on` and `on_event`; found {registrations} \
         addEventListener bindings"
    );
    assert!(
        externs
            .iter()
            .filter(|(_, binding, _)| binding.contains("addEventListener"))
            .all(|(_, _, retains)| *retains),
        "every addEventListener binding must carry `retains`"
    );

    // Removal: nothing is kept past the call, so marking it would be the
    // over-marking the §S4 audit caught on `appendChild` (proposal/router.md
    // §5.2). kolt's hand-roll marks both; this surface deliberately does not.
    let (binding, retains) = marking("off_event");
    assert!(
        binding.contains("removeEventListener"),
        "`off_event` should bind removeEventListener; got `{binding}`"
    );
    assert!(
        !retains,
        "`off_event` must NOT be marked `retains` — removal keeps nothing past \
         the call. Declared as `[extern({binding})]`"
    );
    let removals = externs
        .iter()
        .filter(|(_, binding, _)| binding.contains("removeEventListener"))
        .collect::<Vec<_>>();
    assert_eq!(
        removals.len(),
        2,
        "both targets must declare the removal twin `listen` is built on; found \
         {}",
        removals.len()
    );
    assert!(
        removals.iter().all(|(_, _, retains)| !*retains),
        "no removeEventListener binding may carry `retains`"
    );
}
