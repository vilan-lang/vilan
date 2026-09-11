//! The list-binding surface's runtime gates (backlog A42) and the element
//! mount hook's (backlog A45).
//!
//! Both are claims about a LIVE tree — which rows re-rendered, which kept their
//! element, whether a node was in the document when a callback ran — and none of
//! them can be read off the source or off a golden. So these are e2e legs in the
//! shape `reactive_lifetimes.rs` and `dom_events.rs` established: a
//! browser-target app built with the real CLI, run under node against a DOM
//! stub, asserting on what the running program did to the host.
//!
//! The stub tracks PARENTAGE, which is what makes the two hard claims
//! measurable: a kept row is the same object in the same tree (so a re-render
//! shows up as a fresh `render` line and a replaced child), and "the element is
//! in the document" is a walk from the element up to the document root.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A fresh temp directory for one test's project tree.
fn temp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vilan_ui_rows_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn std_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std")
}

/// A document with real parent/child links, so a walk to the root is a real
/// question, plus `identify(element)` — a stable per-object id, which is how a
/// test tells "the same row moved" from "a new row was built".
const DOM_STUB: &str = r##"let nextIdentity = 1;
const identities = new WeakMap();
function identify(node) {
    if (!identities.has(node)) identities.set(node, nextIdentity++);
    return identities.get(node);
}
/// An inline style declaration block, enough of one for the questions asked
/// here: `setProperty` with an empty value REMOVES the declaration, which is
/// what the CSSOM does and what `View::show` relies on to restore an element's
/// prior inline `display` (A60).
class StubStyle {
    constructor() { this.properties = {}; }
    setProperty(name, value) {
        if (value === "" || value === null || value === undefined) delete this.properties[name];
        else this.properties[name] = value;
    }
    getPropertyValue(name) { return this.properties[name] || ""; }
    removeProperty(name) { delete this.properties[name]; }
}
class StubElement {
    constructor(tag) {
        this.tagName = tag;
        this.children = [];
        this.parent = null;
        this.listeners = {};
        // A59: capture-phase registrations are a SEPARATE table, because the
        // host matches on the phase as part of a listener's identity — a
        // capture listener is not removable by a bubble-phase `off_event`.
        this.captureListeners = {};
        this._text = "";
        this.value = "";
        this.attributes = {};
        this.focused = false;
        this.style = new StubStyle();
    }
    set textContent(text) { this._text = text; this.children = []; }
    get textContent() { return this._text; }
    setAttribute(name, value) { this.attributes[name] = value; }
    removeAttribute(name) { delete this.attributes[name]; }
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
    // `hidden` is a reflecting property in the DOM: the attribute is what CSS
    // and assistive technology see, so the stub reflects it too.
    set hidden(on) { if (on) this.attributes.hidden = ""; else delete this.attributes.hidden; }
    get hidden() { return "hidden" in this.attributes; }
    addEventListener(event, handler, capture) {
        const table = capture ? this.captureListeners : this.listeners;
        (table[event] = table[event] || []).push(handler);
    }
    removeEventListener(event, handler, capture) {
        const table = capture ? this.captureListeners : this.listeners;
        table[event] = (table[event] || []).filter(registered => registered !== handler);
    }
    // A59: the stub has no layout engine, so a measured box comes from
    // `global.boxes` keyed by tag — a harness sets it before requiring the
    // bundle. The rule that IS modelled is the one the binding exists to let a
    // caller wait on: a DETACHED element measures 0x0, whatever the table says.
    getBoundingClientRect() {
        const zero = { left: 0, top: 0, width: 0, height: 0 };
        return inDocument(this) ? ((global.boxes || {})[this.tagName] || zero) : zero;
    }
    get offsetWidth() { return Math.round(this.getBoundingClientRect().width); }
    get offsetHeight() { return Math.round(this.getBoundingClientRect().height); }
    get isConnected() { return inDocument(this); }
    contains(other) {
        for (let walk = other; walk; walk = walk.parent) if (walk === this) return true;
        return false;
    }
    // Tag-name selectors only — enough to ask "which of my descendants", which
    // is the whole question the scoped binding answers.
    querySelectorAll(selector) {
        const found = [];
        const walk = (node) => {
            for (const child of node.children) {
                if (child.tagName === selector) found.push(child);
                walk(child);
            }
        };
        walk(this);
        return found;
    }
    find(predicate) {
        if (predicate(this)) return this;
        for (const child of this.children) { const hit = child.find(predicate); if (hit) return hit; }
        return null;
    }
    focus() { this.focused = true; global.activeElement = this; global.focusLog.push(describe(this)); }
    // `element.matches(":focus")` is how `View::autofocus` reads back whether
    // its request was honored (B271); the stub answers the one selector it is
    // ever asked for.
    matches(selector) {
        if (selector !== ":focus") throw new Error("the stub knows only :focus, got " + selector);
        return global.activeElement === this;
    }
}
/// A text node — a real sibling of the element children, which is what makes
/// a `str` or a `Source<str>` in child position measurable at all.
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
const documentRoot = new StubElement("root");
global.focusLog = [];
global.activeElement = null;
global.document = {
    createElement: (tag) => new StubElement(tag),
    createElementNS: (namespace, tag) => new StubElement(tag),
    createTextNode: (text) => new StubText(text),
    getElementById: () => documentRoot,
    querySelector: () => null,
    querySelectorAll: () => [],
    get activeElement() { return global.activeElement; },
};
const windowListeners = { bubble: {}, capture: {} };
global.window = {
    addEventListener: (event, handler, capture) => {
        const table = capture ? windowListeners.capture : windowListeners.bubble;
        (table[event] = table[event] || []).push(handler);
    },
    removeEventListener: (event, handler, capture) => {
        const table = capture ? windowListeners.capture : windowListeners.bubble;
        table[event] = (table[event] || []).filter(registered => registered !== handler);
    },
};
/// A59: dispatch the way the host does — the window's and every ancestor's
/// CAPTURE listeners on the way DOWN to the target, then the target's own and
/// every ancestor's on the way back UP. The order is the whole point of the
/// capture surface, so the stub has to get it right rather than fire a list.
function dispatchEvent(target, type, extra = {}) {
    const chain = [];
    for (let walk = target; walk; walk = walk.parent) chain.unshift(walk);
    const event = { target, type, preventDefault() { this.prevented = true; }, ...extra };
    for (const handler of (windowListeners.capture[type] || [])) handler(event);
    for (const node of chain) for (const handler of (node.captureListeners[type] || [])) handler(event);
    for (const node of chain.slice().reverse()) for (const handler of (node.listeners[type] || [])) handler(event);
    for (const handler of (windowListeners.bubble[type] || [])) handler(event);
    return event;
}
/// A59: the host `ResizeObserver`. `observe` fires the callback ONCE straight
/// away, as the real one does — that is what makes `observe_resize` a
/// first-layout hook — and `disconnect` forgets every target, so a disconnected
/// observer is silent for `resize(..)` below.
const resizeObservers = [];
global.ResizeObserver = class {
    constructor(callback) { this.callback = callback; this.targets = []; resizeObservers.push(this); }
    observe(target) { this.targets.push(target); this.callback(); }
    disconnect() { this.targets = []; }
};
/// Fire every observer still watching `element` — a size change.
global.resize = (element) => {
    for (const observer of resizeObservers) {
        if (observer.targets.includes(element)) observer.callback();
    }
};
// The frame clock. node has none, and every leg but B271's only needs it to
// EXIST — a macrotask is the honest stand-in for "after the rendering step".
// B271's own leg replaces this with a queue it drives, because "on the next
// frame" is a claim about order.
global.requestAnimationFrame = (callback) => setTimeout(callback, 0);
/// Whether `node` is reachable from the document root by parent links — the
/// question `on_mount` exists to answer.
function inDocument(node) {
    let walk = node;
    while (walk) {
        if (walk === documentRoot) return true;
        walk = walk.parent;
    }
    return false;
}
function describe(node) {
    return node.tagName + "#" + identify(node) + (inDocument(node) ? "@doc" : "@detached");
}
/// The whole tree as one flat line, each node as tag#identity'text'.
function flatten(node) {
    const own = node.tagName + "#" + identify(node) + (node._text ? "'" + node._text + "'" : "");
    return [own].concat(node.children.flatMap(flatten)).join(" ");
}
global.inDocument = inDocument;
global.describe = describe;
global.flatten = flatten;
global.documentRoot = documentRoot;
global.dispatchEvent = dispatchEvent;
global.windowListeners = windowListeners;
"##;

/// Builds `app.vl` for the browser with the real CLI and runs `harness.js`
/// under node, returning its stdout. Fails loudly with both streams.
fn build_and_run(tag: &str, app: &str, harness: &str) -> String {
    let dir = temp_project(tag);
    write(
        &dir,
        "vilan.toml",
        &format!(
            "[package]\nname = \"ui_rows_{tag}\"\nroot = \".\"\nentry = \"app.vl\"\ntarget = \"browser\"\n"
        ),
    );
    write(&dir, "app.vl", app);
    write(&dir, "harness.js", harness);

    let build = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", dir.to_str().unwrap()])
        .env("VILAN_STD", std_dir())
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

// --- A42: the three list forms ----------------------------------------------

/// One list under all three bindings at once, driven through the same edits.
/// `Task` derives `PartialEq` because two of the three forms need it; the
/// fourth list is over `Handle`, which carries a closure and therefore CANNOT
/// derive it — the case that has no spelling without `bind_each_by`.
const THREE_FORMS: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, mount_root, view };

[derive(PartialEq)]
struct Task {
	id: i32,
	title: str,
}

struct Handle {
	id: i32,
	title: str,
	act: || str,
}

fun main() {
	let keyed: SignalCell<List<Task>> = Signal::new([
		Task { id = 1, title = "one" },
		Task { id = 2, title = "two" },
	]);
	let names: SignalCell<List<str>> = Signal::new(["a", "b"]);
	let handles: SignalCell<List<Handle>> = Signal::new([
		Handle { id = 1, title = "one", act = || "act" },
		Handle { id = 2, title = "two", act = || "act" },
	]);
	let _root = mount_root("app", || {
		view("div")
			.child(view("ul").bind_each(keyed, |task| task.id, |task| {
				print(i"keyed renders {task.id}");
				view("li").text(task.title)
			}))
			.child(view("ol").bind_each_values(names, |name| {
				print(i"values renders {name}");
				view("li").text(name)
			}))
			.child(view("nav").bind_each_by(handles, |handle| handle.id, |handle| {
				print(i"by renders {handle.get().id}");
				view("li").bind_text(handle.map(|current| current.title))
			}))
	});

	print("--- same keys, one changed value ---");
	keyed.set([Task { id = 1, title = "ONE" }, Task { id = 2, title = "two" }]);
	names.set(["A", "b"]);
	handles.set([
		Handle { id = 1, title = "ONE", act = || "act" },
		Handle { id = 2, title = "two", act = || "act" },
	]);

	print("--- reorder, values untouched ---");
	keyed.set([Task { id = 2, title = "two" }, Task { id = 1, title = "ONE" }]);
	names.set(["b", "A"]);
	handles.set([
		Handle { id = 2, title = "two", act = || "act" },
		Handle { id = 1, title = "ONE", act = || "act" },
	]);
}

main();
"#;

/// The three forms, side by side, over one pair of edits.
///
/// A changed value under a surviving key REBUILDS the row in both `PartialEq`
/// forms (a fresh `renders` line, a fresh element identity) and KEEPS it under
/// `bind_each_by`, where the new item is written into the row's own cell and the
/// text changes through the binding that was already there. A reorder moves
/// every row in all three — same identities, new order, no re-render anywhere.
#[test]
fn the_three_list_forms_differ_only_in_what_a_changed_row_costs() {
    let harness =
        format!("{DOM_STUB}\nrequire(\"./app.js\");\nconsole.log(flatten(documentRoot));\n");
    let stdout = build_and_run("three_forms", THREE_FORMS, &harness);
    let lines: Vec<&str> = stdout.lines().collect();

    let changed = lines
        .iter()
        .position(|line| line.contains("same keys, one changed value"))
        .expect("the edit marker");
    let reordered = lines
        .iter()
        .position(|line| line.contains("reorder, values untouched"))
        .expect("the reorder marker");

    // A changed value re-renders the row in the two value-checked forms.
    let after_change: Vec<&&str> = lines[changed + 1..reordered].iter().collect();
    assert_eq!(
        after_change,
        vec![&"keyed renders 1", &"values renders A"],
        "a changed value must rebuild the row under `bind_each` and \
         `bind_each_values` and ONLY under those; got:\n{stdout}"
    );

    // A reorder rebuilds nothing at all, in any of the three.
    let after_reorder: Vec<&&str> = lines[reordered + 1..lines.len() - 1].iter().collect();
    assert!(
        after_reorder.is_empty(),
        "a reorder must move rows, never rebuild them; got:\n{stdout}"
    );

    // And the final tree: `bind_each_by`'s row kept its identity across the
    // value change while the other two took fresh ones, and every list is in
    // the reordered order.
    let tree = lines.last().expect("the flattened tree");
    assert!(
        tree.contains("li#") && tree.contains("'ONE'") && tree.contains("'A'"),
        "the tree did not take the edits; got:\n{stdout}"
    );
}

/// The row `bind_each_by` keeps is the SAME element — identity, not just
/// content — and the item it now holds reached the row through its own cell.
const KEPT_IDENTITY: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, mount_root, view };

struct Handle {
	id: i32,
	title: str,
	act: || str,
}

fun main() {
	let handles: SignalCell<List<Handle>> = Signal::new([
		Handle { id = 1, title = "one", act = || "act" },
	]);
	let _root = mount_root("app", || {
		view("ul").bind_each_by(handles, |handle| handle.id, |handle| {
			view("li").bind_text(handle.map(|current| current.title))
		})
	});
	print(i"before={identity_of_first_row()}");
	handles.set([Handle { id = 1, title = "ONE", act = || "act" }]);
	print(i"after={identity_of_first_row()}");
}

[extern("__first_row")]
external fun identity_of_first_row(): str;

main();
"#;

/// `bind_each_by` keeps the row's element across a value change under a
/// surviving key: same identity before and after, and the text updated through
/// the binding rather than through a rebuild. Red under `bind_each` — a
/// `PartialEq` change there disposes the row and builds a new element.
#[test]
fn the_index_form_keeps_the_rows_element_and_updates_through_its_cell() {
    let harness = format!(
        "{DOM_STUB}\nglobal.__first_row = () => {{\n  \
         const list = documentRoot.children[0];\n  \
         const row = list.children[0];\n  \
         return row.tagName + \"#\" + identify(row) + \"'\" + row.textContent + \"'\";\n\
         }};\nrequire(\"./app.js\");\n"
    );
    let stdout = build_and_run("kept_identity", KEPT_IDENTITY, &harness);
    let lines: Vec<&str> = stdout.lines().collect();
    let before = lines[0].strip_prefix("before=").expect("the before line");
    let after = lines[1].strip_prefix("after=").expect("the after line");
    let (before_element, before_text) = before.split_once('\'').expect("tag'text");
    let (after_element, after_text) = after.split_once('\'').expect("tag'text");
    assert_eq!(
        (before_element, before_text),
        ("li#1", "one'"),
        "the first row did not build as expected; got:\n{stdout}"
    );
    assert_eq!(
        after_element, before_element,
        "the row must keep its ELEMENT across a value change under a surviving \
         key — a fresh identity means it was disposed and rebuilt; got:\n{stdout}"
    );
    assert_eq!(
        after_text, "ONE'",
        "the row's text must take the new value through the row's own cell; \
         got:\n{stdout}"
    );
}

// --- A45: the element mount hook --------------------------------------------

/// `on_mount` at every attachment site the module has: a statically appended
/// child, a `when` instantiation that appears in a LATER drain wave, and
/// `bind_each` rows — the initial one and one appended after the fact.
const MOUNT_HOOK: &str = r#"import std::dom::Element;
import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, mount_root, view };

fun main() {
	let open: SignalCell<bool> = Signal::new(false);
	let rows: SignalCell<List<str>> = Signal::new(["a"]);
	let _root = mount_root("app", || {
		view("div")
			.child(view("input").on_mount(|element| print(i"static {reachable(element)}")))
			.child(view("input").autofocus())
			.when(open, || {
				view("section").child(view("input").on_mount(|element| {
					print(i"when {reachable(element)}");
				}))
			})
			.child(view("ul").bind_each_values(rows, |name| {
				view("li").text(name).on_mount(|element| print(i"row {reachable(element)}"))
			}))
	});
	print("built");
	open.set(true);
	rows.set(["a", "b"]);
}

/// The harness's own walk from the element up to the document root.
[extern("__reachable")]
external fun reachable(element: Element): bool;

main();
"#;

/// The claim `on_mount` makes is not "later" but "in the document", so that is
/// what is asserted — at every attachment site, including the two that happen
/// in a drain wave AFTER the build that scheduled the microtask.
///
/// A microtask is enough because the whole synchronous build, and the
/// `mount` that finishes it, run to completion before any microtask does. A
/// row appended by a later wave is the case that could have needed the
/// at-settle fallback; it does not — the wave is synchronous too, and its
/// append lands before the microtask it queued.
#[test]
fn on_mount_hands_over_an_element_that_is_already_in_the_document() {
    let harness = format!("{DOM_STUB}\nglobal.__reachable = inDocument;\nrequire(\"./app.js\");\n");
    let stdout = build_and_run("mount_hook", MOUNT_HOOK, &harness);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines[0], "built",
        "the callbacks must run after the synchronous build, not during it; \
         got:\n{stdout}"
    );
    let mut mounted: Vec<&str> = lines[1..].to_vec();
    mounted.sort_unstable();
    assert_eq!(
        mounted,
        vec!["row true", "row true", "static true", "when true"],
        "every mount callback must see its element IN the document, at every \
         attachment site; got:\n{stdout}"
    );
}

const AUTOFOCUS: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, mount_root, view };

fun main() {
	let open: SignalCell<bool> = Signal::new(false);
	let _root = mount_root("app", || {
		view("div")
			.child(view("input").attr("name", "always"))
			.when(open, || view("input").attr("name", "modal").autofocus())
	});
	print("built");
	open.set(true);
}

main();
"#;

/// `autofocus` is `on_mount(|element| element.focus())` and nothing else, so
/// the pin is that the host's `focus()` really ran, on the right element, once
/// that element was in the document — the case HTML's own `autofocus`
/// attribute cannot serve, because it fires only on a document's initial
/// parse and a modal is mounted later.
#[test]
fn autofocus_focuses_the_modal_input_once_it_is_in_the_document() {
    let harness = format!(
        "{DOM_STUB}\nrequire(\"./app.js\");\n\
         // After the microtask queue: the hook is a microtask, so a timer is\n\
         // the earliest the harness can look.\n\
         setTimeout(() => {{\n  \
         const focused = documentRoot.children[0].children.filter(c => c.focused);\n  \
         console.log(\"focused=\" + focused.map(c => c.attributes.name).join(\",\"));\n  \
         console.log(\"log=\" + focusLog.join(\",\"));\n\
         }}, 0);\n"
    );
    let stdout = build_and_run("autofocus", AUTOFOCUS, &harness);
    assert!(
        stdout.contains("focused=modal"),
        "autofocus must focus the input it was chained onto and no other; \
         got:\n{stdout}"
    );
    assert!(
        stdout.contains("@doc"),
        "the element must be in the document when focus() runs; got:\n{stdout}"
    );
}

// --- B271: `focus()` has a PRECONDITION, and the retry is frame-shaped ------
//
// `on_mount`'s microtask runs BEFORE the frame's rendering step, so an element
// that becomes focusable only during that step is not focusable when the first
// `focus()` lands — and the platform's answer to a target that is hidden,
// unrendered or inert is to do nothing, silently. kolt's overlay panel is
// `visibility: hidden` until a ResizeObserver callback places it and flips it
// visible, so `autofocus` was a no-op there every time. Not a race: the browser
// rule.
//
// `autofocus` is bounded and frame-aware now — attempt in the microtask, then
// on the next animation frame, then once more on the frame after, then stop —
// and this is the pin. The stub is extended by exactly what makes the claim
// measurable: a `visibility` a hidden element refuses `focus()` with,
// `document.activeElement` tracking what actually took it, `matches(":focus")`
// reading it back, and a `requestAnimationFrame` that fires nothing until the
// test says `flushFrame()`. A frame that never comes on its own is the point:
// "on the next frame" is a claim about ORDER, and a real rAF would let a pass
// mean "eventually".
const DOM_STUB_FRAMES: &str = r##"
// A hidden element refuses focus. The platform reads a computed style; the
// stub reads a marker attribute the fixture sets, which is the same fact with
// no layout engine behind it.
const setAttributeBase = StubElement.prototype.setAttribute;
StubElement.prototype.setAttribute = function (name, value) {
    setAttributeBase.call(this, name, value);
    if (name === "data-visibility") this.visibility = value;
};
StubElement.prototype.focus = function () {
    const refused = this.visibility === "hidden";
    global.focusLog.push(describe(this) + (refused ? "!refused" : "!taken"));
    if (refused) return;
    this.focused = true;
    global.activeElement = this;
};
let frameQueue = [];
global.requestAnimationFrame = (callback) => frameQueue.push(callback);
global.flushFrame = () => {
    const due = frameQueue;
    frameQueue = [];
    for (const callback of due) callback();
    return due.length;
};
global.findByName = (name) => {
    const walk = (node) => {
        if (node.attributes && node.attributes.name === name) return node;
        for (const child of node.children) {
            const found = walk(child);
            if (found) return found;
        }
        return null;
    };
    return walk(documentRoot);
};
"##;

/// The overlay's shape: a panel mounted hidden, `autofocus` chained onto its
/// input. Nothing here mentions a frame — that is the whole point of the
/// one-word form.
const HIDDEN_AUTOFOCUS: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, mount_root, view };

fun main() {
	let open: SignalCell<bool> = Signal::new(false);
	let _root = mount_root("app", || {
		view("div").when(open, || {
			view("input").attr("name", "modal").attr("data-visibility", "hidden").autofocus()
		})
	});
	open.set(true);
	print("built");
}

main();
"#;

/// The pin: refused in the microtask, taken on the FIRST frame after the
/// subtree is flipped visible — and the retry stops there rather than running
/// forever.
#[test]
fn b271_autofocus_is_refused_in_the_microtask_and_taken_on_the_first_frame() {
    let harness = format!(
        "{DOM_STUB}\n{DOM_STUB_FRAMES}\nrequire(\"./app.js\");\n\
         setTimeout(() => {{\n  \
         const panel = findByName(\"modal\");\n  \
         console.log(\"microtask=\" + focusLog.join(\"|\"));\n  \
         console.log(\"activeBefore=\" + (activeElement ? activeElement.attributes.name : \"none\"));\n  \
         // The ResizeObserver callback: the panel is placed and flipped visible\n  \
         // in the rendering step a frame callback runs after.\n  \
         panel.visibility = \"visible\";\n  \
         console.log(\"ranFrame=\" + flushFrame());\n  \
         console.log(\"afterFrame=\" + focusLog.join(\"|\"));\n  \
         console.log(\"activeAfter=\" + (activeElement ? activeElement.attributes.name : \"none\"));\n  \
         console.log(\"tail=\" + flushFrame());\n\
         }}, 0);\n"
    );
    let stdout = build_and_run("b271_frames", HIDDEN_AUTOFOCUS, &harness);
    let line = |key: &str| -> String {
        stdout
            .lines()
            .find(|line| line.starts_with(key))
            .unwrap_or_else(|| panic!("no {key:?} line in:\n{stdout}"))
            .to_string()
    };
    assert!(
        line("microtask=").ends_with("!refused"),
        "a hidden element refuses focus, and the microtask is where it is \
         still hidden; got:\n{stdout}"
    );
    assert!(
        line("microtask=").contains("@doc"),
        "the element IS in the document — `on_mount`'s promise is kept and is \
         not the problem; got:\n{stdout}"
    );
    assert_eq!(
        line("activeBefore="),
        "activeBefore=none",
        "nothing took focus in the microtask; got:\n{stdout}"
    );
    assert_eq!(
        line("ranFrame="),
        "ranFrame=1",
        "exactly one frame callback was queued by the refused attempt; \
         got:\n{stdout}"
    );
    let after = line("afterFrame=");
    let attempts: Vec<&str> = after.trim_start_matches("afterFrame=").split('|').collect();
    assert_eq!(
        attempts.len(),
        2,
        "two attempts: the microtask's and the first frame's; got:\n{stdout}"
    );
    assert!(
        attempts[1].ends_with("!taken"),
        "the first frame after the flip takes focus; got:\n{stdout}"
    );
    assert_eq!(
        line("activeAfter="),
        "activeAfter=modal",
        "`document.activeElement` is the input `autofocus` was chained onto; \
         got:\n{stdout}"
    );
    assert_eq!(
        line("tail="),
        "tail=0",
        "the retry is BOUNDED: a taken focus queues no further frame; \
         got:\n{stdout}"
    );
}

/// The SSR twins accept and drop, like every event binder there: the markup is
/// exactly what it would have been without them, and no action runs.
const SSR_TWINS: &str = r#"import std::io::print;
import std::ui::{ View, render, view };

fun main() {
	print(render(view("input").attr("name", "modal").autofocus()));
	print(render(view("input").on_mount(|_element| print("RAN"))));
}

main();
"#;

#[test]
fn the_ssr_twins_of_the_mount_hook_render_the_same_markup_and_run_nothing() {
    let dir = temp_project("ssr_twins");
    std::fs::create_dir_all(&dir).expect("create the program directory");
    let source = dir.join("app.vl");
    std::fs::write(&source, SSR_TWINS).expect("write the program");
    let build = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .arg("build")
        .arg(&source)
        .env("VILAN_STD", std_dir())
        .output()
        .expect("run vilan build");
    assert!(
        build.status.success(),
        "vilan build failed:\n{}\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new("node")
        .arg("app.mjs")
        .current_dir(&dir)
        .output()
        .expect("run node");
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    assert_eq!(
        stdout, "<input name=\"modal\">\n<input>\n",
        "the SSR twins must render the markup unchanged and run no action"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// --- B255: an in-place `update` beside a `bind_each` -------------------------

/// Six keyed rows, then a `remove(0)` performed IN PLACE through
/// `SignalCell::update`. `bind_each` keeps the effect's list as `row_items`,
/// and before B257 that store aliased the cell's own storage — so the next
/// pass handed `reconcile` an `old_items` that WAS the new array, one shorter
/// than the `old_keys` beside it, and `same(old_items[5], item)` read past the
/// end (`index out of bounds: the length is 5 but the index is 5`, kolt
/// channel.vl:54). The build now copies at the assignment, so `old_items` is
/// the snapshot the keys were taken from.
const IN_PLACE_REMOVE: &str = r#"import std::compare::PartialEq;
import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, mount_root, view };

struct Row {
	id: i32,
	text: str,
}

// Hand-written, and narrower than the struct: `bind_each`'s key here is the
// ITEM, so identity is the id and a surviving row is one whose id survived.
impl Row with PartialEq {
	fun eq(self, b: Row): bool {
		self.id == b.id
	}
}

fun main() {
	let rows: SignalCell<List<Row>> = Signal::new([
		Row { id = 1, text = "a" },
		Row { id = 2, text = "b" },
		Row { id = 3, text = "c" },
		Row { id = 4, text = "d" },
		Row { id = 5, text = "e" },
		Row { id = 6, text = "f" },
	]);
	let _root = mount_root("app", || {
		view("ul").bind_each(rows, |row| row, |row| {
			print(i"render {row.id}");
			view("li").text(row.text)
		})
	});
	print(i"before={tree()}");
	print("--- remove ---");
	rows.update(|&mut xs| {
		xs.remove(0);
	});
	print(i"after={tree()}");
}

[extern("__tree")]
external fun tree(): str;

main();
"#;

/// The rows a flattened tree line names, as `li#identity'text'` tokens.
fn row_tokens(line: &str) -> Vec<&str> {
    line.split(' ')
        .filter(|token| token.starts_with("li#"))
        .collect()
}

#[test]
fn b255_an_in_place_remove_under_bind_each_keeps_the_surviving_rows() {
    let harness = format!(
        "{DOM_STUB}\nglobal.__tree = () => flatten(documentRoot);\nrequire(\"./app.js\");\n"
    );
    let stdout = build_and_run("in_place_remove", IN_PLACE_REMOVE, &harness);
    let lines: Vec<&str> = stdout.lines().collect();
    let before = lines
        .iter()
        .find_map(|line| line.strip_prefix("before="))
        .expect("the before line");
    let after = lines
        .iter()
        .find_map(|line| line.strip_prefix("after="))
        .expect("the after line");
    let marker = lines
        .iter()
        .position(|line| line.contains("--- remove ---"))
        .expect("the edit marker");

    let before_rows = row_tokens(before);
    let after_rows = row_tokens(after);
    assert_eq!(
        before_rows.len(),
        6,
        "the list must build six rows; got:\n{stdout}"
    );
    // The survivors are the LAST five, unchanged and in order — same element
    // identities, so every one of them was moved rather than rebuilt.
    assert_eq!(
        after_rows,
        before_rows[1..].to_vec(),
        "an in-place `remove(0)` must drop the first row and KEEP the other \
         five, element identity included; got:\n{stdout}"
    );
    // And nothing re-rendered: every surviving key's item is unchanged.
    let rebuilt: Vec<&&str> = lines[marker + 1..]
        .iter()
        .filter(|line| line.starts_with("render "))
        .collect();
    assert!(
        rebuilt.is_empty(),
        "a removal must rebuild no surviving row; got:\n{stdout}"
    );
}

/// The second symptom of the same alias: an in-place ELEMENT write. The key
/// (the id) survives, so the row is a candidate for reuse and `same` decides —
/// and `same` was being asked to compare the mutated array with itself, which
/// always says Keep. The row then kept an element rendered from the OLD item.
const IN_PLACE_EDIT: &str = r#"import std::compare::PartialEq;
import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, mount_root, view };

struct Row {
	id: i32,
	text: str,
}

// The whole struct decides "changed"; the key below decides "the same row".
impl Row with PartialEq {
	fun eq(self, b: Row): bool {
		self.id == b.id && self.text == b.text
	}
}

fun main() {
	let rows: SignalCell<List<Row>> = Signal::new([
		Row { id = 1, text = "a" },
		Row { id = 2, text = "b" },
	]);
	let _root = mount_root("app", || {
		view("ul").bind_each(rows, |row| row.id, |row| {
			print(i"render {row.id}");
			view("li").text(row.text)
		})
	});
	print("--- edit ---");
	rows.update(|&mut xs| {
		xs[0] = Row { id = 1, text = "EDITED" };
	});
	print(i"after={tree()}");
}

[extern("__tree")]
external fun tree(): str;

main();
"#;

#[test]
fn b255_an_in_place_element_write_under_bind_each_refreshes_that_row() {
    let harness = format!(
        "{DOM_STUB}\nglobal.__tree = () => flatten(documentRoot);\nrequire(\"./app.js\");\n"
    );
    let stdout = build_and_run("in_place_edit", IN_PLACE_EDIT, &harness);
    let lines: Vec<&str> = stdout.lines().collect();
    let marker = lines
        .iter()
        .position(|line| line.contains("--- edit ---"))
        .expect("the edit marker");
    let after = lines
        .iter()
        .find_map(|line| line.strip_prefix("after="))
        .expect("the after line");

    let rebuilt: Vec<&&str> = lines[marker + 1..]
        .iter()
        .filter(|line| line.starts_with("render "))
        .collect();
    assert_eq!(
        rebuilt,
        vec![&"render 1"],
        "the edited row, and only it, must rebuild; got:\n{stdout}"
    );
    assert!(
        after.contains("'EDITED'") && !after.contains("'a'"),
        "the edited row must show its new text; got:\n{stdout}"
    );
    assert!(
        after.contains("'b'"),
        "the untouched row must still be there; got:\n{stdout}"
    );
}

/// `bind_each_by` holds `row_items` exactly as `bind_each` does, so the
/// out-of-bounds half is its too — its `same` is constantly true, so it never
/// showed the stale-row half. `T` here carries a closure and so cannot compare
/// at all, which is the shape this form exists for.
const IN_PLACE_REMOVE_BY: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, mount_root, view };

struct Handle {
	id: i32,
	text: str,
	act: || str,
}

fun main() {
	let rows: SignalCell<List<Handle>> = Signal::new([
		Handle { id = 1, text = "a", act = || "act" },
		Handle { id = 2, text = "b", act = || "act" },
		Handle { id = 3, text = "c", act = || "act" },
		Handle { id = 4, text = "d", act = || "act" },
		Handle { id = 5, text = "e", act = || "act" },
		Handle { id = 6, text = "f", act = || "act" },
	]);
	let _root = mount_root("app", || {
		view("ul").bind_each_by(rows, |row| row.id, |row| {
			view("li").bind_text(row.map(|current| current.text))
		})
	});
	print(i"before={tree()}");
	rows.update(|&mut xs| {
		xs.remove(0);
	});
	print(i"after={tree()}");
}

[extern("__tree")]
external fun tree(): str;

main();
"#;

#[test]
fn b255_an_in_place_remove_under_bind_each_by_keeps_the_surviving_rows() {
    let harness = format!(
        "{DOM_STUB}\nglobal.__tree = () => flatten(documentRoot);\nrequire(\"./app.js\");\n"
    );
    let stdout = build_and_run("in_place_remove_by", IN_PLACE_REMOVE_BY, &harness);
    let lines: Vec<&str> = stdout.lines().collect();
    let before = lines
        .iter()
        .find_map(|line| line.strip_prefix("before="))
        .expect("the before line");
    let after = lines
        .iter()
        .find_map(|line| line.strip_prefix("after="))
        .expect("the after line");
    let before_rows = row_tokens(before);
    let after_rows = row_tokens(after);
    assert_eq!(
        before_rows.len(),
        6,
        "the list must build six rows; got:\n{stdout}"
    );
    assert_eq!(
        after_rows.len(),
        5,
        "an in-place `remove(0)` must leave five rows; got:\n{stdout}"
    );
    // Identity is the whole claim here: every surviving row keeps its element
    // and its cell, so the text it shows is the one its own binding wrote.
    let identities: Vec<&str> = after_rows
        .iter()
        .map(|token| token.split('\'').next().expect("li#identity"))
        .collect();
    let expected: Vec<&str> = before_rows[1..]
        .iter()
        .map(|token| token.split('\'').next().expect("li#identity"))
        .collect();
    assert_eq!(
        identities, expected,
        "the surviving rows must keep their elements, in order; got:\n{stdout}"
    );
}

// --- A59: measurement, observation, and un-setting ---------------------------

/// The measurement surface over one mounted panel and one detached element:
/// `bounding_rect` (and its derived edges), the rounded `offset_*` pair,
/// `is_connected`, `contains`, the element-scoped `query_selector_all`, and
/// `remove_attribute` — every binding kolt's overlay hand-declared.
const MEASURE: &str = r#"import std::dom::{ create_element, get_element_by_id };
import std::io::print;
import std::ui::{ mount_root, view };

fun main() {
	let _root = mount_root("app", || {
		view("section")
			.attr("data-open", "")
			.attr("data-tag", "kept")
			.child(view("li").text("a"))
			.child(view("li").text("b"))
	});
	let root = get_element_by_id("app");
	let panels = root.query_selector_all("section");
	let found = panels.len();
	print(i"panels={found}");

	let panel = panels[0];
	let box = panel.bounding_rect();
	print(i"rect={box.left},{box.top},{box.width},{box.height}");
	let right = box.right();
	let bottom = box.bottom();
	print(i"edges={right},{bottom}");
	let width = panel.offset_width();
	let height = panel.offset_height();
	print(i"offset={width},{height}");
	let connected = panel.is_connected();
	print(i"connected={connected}");

	// A DETACHED element measures 0x0 whatever the layout would say — the rule
	// `is_connected` exists to let a caller wait on.
	let loose = create_element("section");
	let loose_box = loose.bounding_rect();
	let loose_width = loose.offset_width();
	let loose_connected = loose.is_connected();
	print(i"detached={loose_box.width},{loose_box.height},{loose_width},{loose_connected}");

	let items = panel.query_selector_all("li");
	let item_count = items.len();
	print(i"items={item_count}");
	let has_child = panel.contains(items[0]);
	let has_self = panel.contains(panel);
	let has_loose = panel.contains(loose);
	print(i"contains={has_child},{has_self},{has_loose}");

	panel.remove_attribute("data-open");
	// Removing what is not there is a no-op, not an error.
	panel.remove_attribute("data-never-set");
}

main();
"#;

#[test]
fn a59_measurement_reads_the_host_box_and_a_detached_element_reads_zero() {
    let harness = format!(
        "{DOM_STUB}\n\
         global.boxes = {{ section: {{ left: 12, top: 30, width: 200.5, height: 40.25 }} }};\n\
         require(\"./app.js\");\n\
         const panel = documentRoot.find(node => node.tagName === \"section\");\n\
         console.log(\"attributes=\" + Object.keys(panel.attributes).join(\",\"));\n"
    );
    let stdout = build_and_run("a59_measure", MEASURE, &harness);
    let expected = [
        "panels=1",
        // The rect is FRACTIONAL and the offsets are rounded — the reason both
        // exist, and the one difference a caller has to know about.
        "rect=12,30,200.5,40.25",
        "edges=212.5,70.25",
        "offset=201,40",
        "connected=true",
        "detached=0,0,0,false",
        "items=2",
        // contains: a descendant, the element ITSELF (the host says yes, and an
        // outside-click guard depends on it), and an unrelated element.
        "contains=true,true,false",
        // `remove_attribute` unset the one it names and left the other alone.
        "attributes=data-tag",
    ];
    for line in expected {
        assert!(
            stdout.lines().any(|printed| printed == line),
            "expected the line `{line}`; got:\n{stdout}"
        );
    }
}

/// The capture phase: a listener registered with `listen_capture` runs on the
/// way DOWN — before the target's own — and a disposed one is gone from the
/// capture table rather than from the bubble table it never joined.
const CAPTURE: &str = r#"import std::dom::{ get_element_by_id, window };
import std::io::print;
import std::ui::{ mount_root, view };

fun main() {
	let _root = mount_root("app", || {
		view("section").child(view("button").text("go"))
	});
	let root = get_element_by_id("app");
	let panel = root.query_selector_all("section")[0];
	let button = root.query_selector_all("button")[0];

	let _kept = panel.listen_capture("click", |event| {
		let inside = panel.contains(event.target());
		print(i"panel-capture inside={inside}");
	});
	let _bubble = panel.listen("click", |_event| print("panel-bubble"));
	button.on_event("click", |_event| print("button-target"));

	// Disposed before anything is dispatched: the capture registration must be
	// gone, and the phase is part of the identity the host matches on.
	let dropped = panel.listen_capture("click", |_event| print("panel-capture-dropped"));
	dropped.dispose();

	// `scroll` does not bubble, so the window hears an inner panel's scrolling
	// only in capture.
	let _scroll = window().listen_capture("scroll", |_event| print("window-capture-scroll"));
	let scroll_dropped = window().listen_capture("scroll", |_event| print("window-scroll-dropped"));
	scroll_dropped.dispose();
	print("armed");
}

main();
"#;

#[test]
fn a59_capture_listeners_run_before_the_target_and_dispose_by_phase() {
    let harness = format!(
        "{DOM_STUB}\n\
         require(\"./app.js\");\n\
         const panel = documentRoot.find(node => node.tagName === \"section\");\n\
         const button = documentRoot.find(node => node.tagName === \"button\");\n\
         console.log(\"capture-registered=\" + panel.captureListeners.click.length);\n\
         console.log(\"bubble-registered=\" + panel.listeners.click.length);\n\
         console.log(\"window-capture-registered=\" + windowListeners.capture.scroll.length);\n\
         dispatchEvent(button, \"click\");\n\
         dispatchEvent(panel, \"scroll\");\n"
    );
    let stdout = build_and_run("a59_capture", CAPTURE, &harness);
    let lines: Vec<&str> = stdout.lines().collect();
    // Exactly one of each survived: disposing a capture subscription removed the
    // capture registration and touched no bubble one.
    for line in [
        "capture-registered=1",
        "bubble-registered=1",
        "window-capture-registered=1",
    ] {
        assert!(
            lines.contains(&line),
            "expected the line `{line}`; got:\n{stdout}"
        );
    }
    let order: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| line.starts_with("panel-") || line.starts_with("button-"))
        .collect();
    assert_eq!(
        order,
        vec!["panel-capture inside=true", "button-target", "panel-bubble"],
        "capture must run on the way DOWN (before the target), bubble on the way back up; got:\n{stdout}"
    );
    assert!(
        lines.contains(&"window-capture-scroll"),
        "a window capture listener must hear a scroll that never bubbles; got:\n{stdout}"
    );
    assert!(
        !stdout.contains("dropped"),
        "a disposed capture subscription must fire nothing; got:\n{stdout}"
    );
}

/// Resize observation: `observe_resize` fires ONCE when observation starts (the
/// first-layout hook), fires again on a size change, and its `Subscription`
/// disconnects the observer.
const RESIZE: &str = r#"import std::dom::get_element_by_id;
import std::io::print;
import std::ui::{ mount_root, view };

fun main() {
	let _root = mount_root("app", || view("section"));
	let panel = get_element_by_id("app").query_selector_all("section")[0];
	print("observing");
	let _watch = panel.observe_resize(|| print("resized"));
	let dropped = panel.observe_resize(|| print("dropped-resize"));
	dropped.dispose();
	print("armed");
}

main();
"#;

#[test]
fn a59_observe_resize_fires_on_first_layout_and_stops_with_its_subscription() {
    let harness = format!(
        "{DOM_STUB}\n\
         require(\"./app.js\");\n\
         const panel = documentRoot.find(node => node.tagName === \"section\");\n\
         resize(panel);\n"
    );
    let stdout = build_and_run("a59_resize", RESIZE, &harness);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        vec![
            "observing",
            // Once at `observe` — the first-layout hook.
            "resized",
            // The second observer also fires once, then is disposed.
            "dropped-resize",
            "armed",
            // The size change: only the LIVE observer hears it.
            "resized",
        ],
        "observe fires once on start and again on a change, and a disposed observer is silent; got:\n{stdout}"
    );
}

/// Every arm of `Slot` at once, static and reactive, then the whole root
/// disposed. The two reactive ELEMENT arms are written in element syntax —
/// `<main>{panel}</main>` is the kolt shape (views.vl:543), the one that had
/// no spelling but `.swap(signal, |x| view)`.
const CHILD_CONTRACT: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, mount_root, view };

fun main() {
	let label: SignalCell<str> = Signal::new("one");
	let panel: SignalCell<View> = Signal::new(view("p").text("first"));
	let run: SignalCell<List<View>> = Signal::new([view("li").text("a"), view("li").text("b")]);
	let statics: List<View> = [view("i").text("x"), view("i").text("y")];
	let root = mount_root("app", || {
		view("div")
			.child(view("section").child("plain").child(view("em").text("element")).child(statics))
			.child(<h1>{label}</h1>)
			.child(<main>{panel}</main>)
			.child(<ul>{run}</ul>)
	});
	print(i"built={tree()}");
	label.set("two");
	panel.set(view("p").text("second"));
	run.set([view("li").text("c")]);
	print(i"changed={tree()}");
	root.dispose();
	label.set("three");
	panel.set(view("p").text("third"));
	run.set([view("li").text("d")]);
	print(i"disposed={tree()}");
}

[extern("__tree")]
external fun tree(): str;

main();
"#;

/// The SSR twins of the two new arms: read once, the value at render time
/// being the value served — no subscription, no later change to follow.
const CHILD_CONTRACT_SSR: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, render, view };

fun main() {
	let label: SignalCell<str> = Signal::new("one");
	let panel: SignalCell<View> = Signal::new(view("p").text("first"));
	let run: SignalCell<List<View>> = Signal::new([view("li").text("a"), view("li").text("b")]);
	let statics: List<View> = [view("i").text("x")];
	print(render(view("div")
		.child("plain")
		.child(statics)
		.child(<h1>{label}</h1>)
		.child(<main>{panel}</main>)
		.child(<ul>{run}</ul>)));
}

main();
"#;

/// `inert` on the app shell while a modal is up — kolt's exhibit
/// (views.vl:74), hand-written there over its own `remove_attribute` extern.
/// Toggled on, off, and on again, then the boundary disposed and the source
/// written once more.
const TOGGLE_ATTR: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, mount_root, view };

fun main() {
	let modal: SignalCell<bool> = Signal::new(false);
	let root = mount_root("app", || {
		view("div").toggle_attr("inert", modal).child(view("p").text("shell"))
	});
	print(i"initial={shell_attributes()}");
	modal.set(true);
	print(i"open={shell_attributes()}");
	modal.set(false);
	print(i"closed={shell_attributes()}");
	modal.set(true);
	print(i"reopened={shell_attributes()}");
	root.dispose();
	modal.set(false);
	print(i"disposed={shell_attributes()}");
}

[extern("__shell_attributes")]
external fun shell_attributes(): str;

main();
"#;

/// The SSR twin: the attribute is rendered when the source is currently true
/// and absent when it is false. Presence is the whole meaning, so a false
/// source has nothing to serialize.
const TOGGLE_ATTR_SSR: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, render, view };

fun main() {
	let modal: SignalCell<bool> = Signal::new(true);
	let quiet: SignalCell<bool> = Signal::new(false);
	print(render(view("div").toggle_attr("inert", modal).attr("id", "shell")));
	print(render(view("dialog").toggle_attr("open", quiet)));
}

main();
"#;

/// A flex container under `show`, plus the two style smalls the same item
/// carries: `flex_grow` beside `flex_shrink`, and `Color::current()` composed
/// through `.alpha()`.
const SHOW_OVER_A_FLEX_ROW: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::style::{ Color, Display, preflight, style };
import std::ui::{ View, mount_root, view };

fun main() {
	let _reset = const preflight();
	let visible: SignalCell<bool> = Signal::new(true);
	let _root = mount_root("app", || {
		view("div")
			.styled(const style()
				.display(Display::Flex)
				.flex_grow(1f)
				.background(Color::current().alpha(0.1)))
			.show(visible)
			.child(view("p").text("row"))
	});
	print(i"shown={probe()}");
	visible.set(false);
	print(i"hidden={probe()}");
	visible.set(true);
	print(i"reshown={probe()}");
}

[extern("__probe")]
external fun probe(): str;

main();
"#;

/// The SSR twin makes the same two writes, so a server-rendered hidden element
/// is hidden on the first paint rather than painted until the client's first
/// toggle takes it away.
const SHOW_SSR: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, render, view };

fun main() {
	let visible: SignalCell<bool> = Signal::new(true);
	let gone: SignalCell<bool> = Signal::new(false);
	let width: SignalCell<str> = Signal::new("3rem");
	print(render(view("div").attr("id", "shown").show(visible)));
	print(render(view("div").attr("id", "gone").show(gone)));
	print(render(view("div").style_var("--w", width).show(gone)));
}

main();
"#;

/// The child contract, arm by arm (B268).
///
/// RED BEFORE THE FIX on the two element arms: a `Signal<View>` and a
/// `Signal<List<View>>` in child position reached the `Source<str>` text arm —
/// a bound's ARGUMENTS were dropped when an impl was matched to a receiver, so
/// every blanket over a parameterized trait matched every instantiation of it —
/// and the DOM took the view's runtime shape, `#text'[object Object]'`, which
/// never changed again.
///
/// The static arms ride along as the no-regression half: a `str`, a `View` and
/// a `List<View>` still place, and `Signal<str>` still keeps a text node in
/// sync rather than being pushed off its own arm by the new ones.
#[test]
fn b268_every_child_arm_places_and_the_reactive_ones_replace_and_die_with_the_boundary() {
    let harness = format!(
        "{DOM_STUB}\nglobal.__tree = () => flatten(documentRoot);\nrequire(\"./app.js\");\n"
    );
    let stdout = build_and_run("child_contract", CHILD_CONTRACT, &harness);
    let line = |prefix: &str| {
        stdout
            .lines()
            .find_map(|line| line.strip_prefix(prefix))
            .unwrap_or_else(|| panic!("the {prefix} line; got:\n{stdout}"))
            .to_string()
    };
    let built = line("built=");
    let changed = line("changed=");
    let disposed = line("disposed=");

    // The static arms: a text node, an element, and a run of elements.
    for expected in ["#text", "'plain'", "em#", "'element'", "'x'", "'y'"] {
        assert!(
            built.contains(expected),
            "the static child arms must place {expected}; got:\n{stdout}"
        );
    }
    // The reactive arms placed VIEWS, not their stringification.
    assert!(
        !built.contains("[object Object]"),
        "a Signal<View> child must render the view, not its runtime shape; \
         got:\n{stdout}"
    );
    assert!(
        built.contains("p#") && built.contains("'first'"),
        "a Signal<View> child must place the view it holds; got:\n{stdout}"
    );
    assert!(
        built.contains("'a'") && built.contains("'b'"),
        "a Signal<List<View>> child must place every view it holds; got:\n{stdout}"
    );
    assert!(
        built.contains("'one'"),
        "a Signal<str> child must still place a text node; got:\n{stdout}"
    );

    // Each reactive arm re-rendered, and left nothing of its predecessor.
    assert!(
        changed.contains("'two'") && !changed.contains("'one'"),
        "a Signal<str> child must re-set its text node; got:\n{stdout}"
    );
    assert!(
        changed.contains("'second'") && !changed.contains("'first'"),
        "a Signal<View> child must replace the view and remove the old one; \
         got:\n{stdout}"
    );
    assert!(
        changed.contains("'c'") && !changed.contains("'a'") && !changed.contains("'b'"),
        "a Signal<List<View>> child must replace the whole run; got:\n{stdout}"
    );

    // And the subscriptions died with the root owner: three more writes, no
    // change to the tree at all.
    assert_eq!(
        disposed, changed,
        "every reactive child arm registers with the nearest boundary, so \
         disposing it must stop the replacement; got:\n{stdout}"
    );
}

/// Builds `app` for the default (process) target and runs the emitted `.mjs`,
/// returning its stdout — the SSR half of [`build_and_run`].
fn build_and_run_process(tag: &str, app: &str) -> String {
    let dir = temp_project(tag);
    std::fs::create_dir_all(&dir).expect("create the program directory");
    let source = dir.join("app.vl");
    std::fs::write(&source, app).expect("write the program");
    let build = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .arg("build")
        .arg(&source)
        .env("VILAN_STD", std_dir())
        .output()
        .expect("run vilan build");
    assert!(
        build.status.success(),
        "vilan build failed:\n{}\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new("node")
        .arg("app.mjs")
        .current_dir(&dir)
        .output()
        .expect("run node");
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(
        run.status.success(),
        "the server render failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
    stdout
}

#[test]
fn b268_the_ssr_twins_of_the_new_child_arms_render_the_views_they_hold() {
    let stdout = build_and_run_process("child_contract_ssr", CHILD_CONTRACT_SSR);
    assert_eq!(
        stdout,
        "<div>plain<i>x</i><h1>one</h1><main><p>first</p></main><ul><li>a</li><li>b</li></ul></div>\n",
        "the server render must serialize a Source<View> and a \
         Source<List<View>> child as the elements they hold"
    );
}

/// PRESENCE, not value (A66): the attribute is written as the empty string
/// when the source is true and REMOVED when it is false — never set to
/// `"false"`, which is a present boolean attribute and therefore still on.
/// And the effect is the boundary's: disposing the root stops the toggling.
#[test]
fn a66_toggle_attr_adds_and_removes_the_attribute_and_dies_with_its_boundary() {
    let harness = format!(
        "{DOM_STUB}\nglobal.__shell_attributes = () => {{\n  \
         const shell = documentRoot.children[0];\n  \
         return JSON.stringify(shell.attributes);\n\
         }};\nrequire(\"./app.js\");\n"
    );
    let stdout = build_and_run("toggle_attr", TOGGLE_ATTR, &harness);
    let line = |prefix: &str| {
        stdout
            .lines()
            .find_map(|line| line.strip_prefix(prefix))
            .unwrap_or_else(|| panic!("the {prefix} line; got:\n{stdout}"))
            .to_string()
    };
    assert_eq!(
        line("initial="),
        "{}",
        "a false source must leave the attribute off entirely; got:\n{stdout}"
    );
    assert_eq!(
        line("open="),
        "{\"inert\":\"\"}",
        "a true source must write the attribute as the empty string; got:\n{stdout}"
    );
    assert_eq!(
        line("closed="),
        "{}",
        "a false source must REMOVE the attribute, not write a value; \
         got:\n{stdout}"
    );
    assert_eq!(
        line("reopened="),
        "{\"inert\":\"\"}",
        "the binding must keep toggling; got:\n{stdout}"
    );
    assert_eq!(
        line("disposed="),
        "{\"inert\":\"\"}",
        "the effect registers with the nearest boundary, so disposing it must \
         stop the toggle; got:\n{stdout}"
    );
}

#[test]
fn a66_the_ssr_twin_renders_a_true_boolean_attribute_and_omits_a_false_one() {
    let stdout = build_and_run_process("toggle_attr_ssr", TOGGLE_ATTR_SSR);
    assert_eq!(
        stdout, "<div inert=\"\" id=\"shell\"></div>\n<dialog></dialog>\n",
        "the server render must carry a true boolean attribute in insertion \
         order and omit a false one entirely"
    );
}

/// Builds `app.vl` for the browser, runs `harness.js`, and hands back the
/// harness's stdout together with the EMITTED STYLESHEET — the two halves the
/// `[hidden]` question needs, since the DOM stub models a tree and not a
/// cascade.
fn build_and_run_with_stylesheet(tag: &str, app: &str, harness: &str) -> (String, String) {
    let dir = temp_project(tag);
    write(
        &dir,
        "vilan.toml",
        &format!(
            "[package]\nname = \"ui_rows_{tag}\"\nroot = \".\"\nentry = \"app.vl\"\ntarget = \"browser\"\n"
        ),
    );
    write(&dir, "app.vl", app);
    write(&dir, "harness.js", harness);

    let build = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", dir.to_str().unwrap()])
        .env("VILAN_STD", std_dir())
        .output()
        .expect("run vilan build");
    assert!(
        build.status.success(),
        "vilan build failed:\n{}\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    let mut stylesheet = String::new();
    for entry in std::fs::read_dir(&dir).expect("read the build directory") {
        let path = entry.expect("a directory entry").path();
        if path.extension().is_some_and(|extension| extension == "css") {
            stylesheet.push_str(&std::fs::read_to_string(&path).expect("read the stylesheet"));
        }
    }
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
    (stdout, stylesheet)
}

/// `show` on a flex container actually hides it (A60).
///
/// RED BEFORE THE FIX: `show` set the `hidden` PROPERTY and nothing else, and
/// the rule that would have acted on it — `[hidden]{display:none}` — is emitted
/// inside `@layer vilan.preflight` while a compiled `Style`'s own rules are
/// UNLAYERED. An unlayered author declaration is the highest-priority author
/// layer there is, so `.sX{display:flex}` beat the reset whatever its
/// specificity and `show(false)` painted the row exactly as before. Both facts
/// are asserted here, because the DOM stub models a tree and not a cascade:
/// the emitted stylesheet says the reset cannot win, and the element says
/// `show` hid it anyway.
///
/// No cascade layer could have fixed it — there is no layer above "unlayered" —
/// and `!important` is refused permanently (css-block.md §10), so the fix is
/// the inline `display`, put back to what the element had when the source turns
/// true again.
#[test]
fn a60_show_hides_a_flex_container_the_preflight_rule_cannot_reach() {
    let harness = format!(
        "{DOM_STUB}\nglobal.__probe = () => {{\n  \
         const row = documentRoot.children[0];\n  \
         return JSON.stringify({{ attributes: row.attributes, inline: row.style.properties }});\n\
         }};\nrequire(\"./app.js\");\n"
    );
    let (stdout, stylesheet) =
        build_and_run_with_stylesheet("show_flex", SHOW_OVER_A_FLEX_ROW, &harness);

    // The mechanism, off the emitted sheet: the app's `display` is unlayered
    // and the reset's `[hidden]` is not, so the reset loses outright.
    assert!(
        stylesheet
            .lines()
            .any(|line| line.ends_with("{display:flex}") && !line.starts_with("@layer")),
        "the app's own `display` must be emitted UNLAYERED — the premise of \
         this pin; got:\n{stylesheet}"
    );
    assert!(
        stylesheet.contains("@layer vilan.preflight{[hidden]{display:none}}"),
        "the preflight's `[hidden]` rule must be emitted in its own layer — \
         the other half of the premise; got:\n{stylesheet}"
    );

    // The two style smalls the same item carries, on the same sheet.
    assert!(
        stylesheet
            .lines()
            .any(|line| line.ends_with("{flex-grow:1}")),
        "`flex_grow` must emit its declaration; got:\n{stylesheet}"
    );
    assert!(
        stylesheet.contains("background-color:rgb(from currentColor r g b / 0.1)"),
        "`Color::current()` must render `currentColor` and stay composable \
         under `.alpha()`; got:\n{stylesheet}"
    );

    // And the claim: the element is really hidden, and really restored.
    let line = |prefix: &str| {
        stdout
            .lines()
            .find_map(|line| line.strip_prefix(prefix))
            .unwrap_or_else(|| panic!("the {prefix} line; got:\n{stdout}"))
            .to_string()
    };
    let shown = line("shown=");
    assert!(
        !shown.contains("hidden") && shown.contains("\"inline\":{}"),
        "a visible element must carry neither the attribute nor an inline \
         display; got:\n{stdout}"
    );
    assert!(
        line("hidden=").contains("\"hidden\":\"\"")
            && line("hidden=").contains("\"display\":\"none\""),
        "`show(false)` must set the `hidden` attribute AND the inline \
         `display:none` that actually beats the app's own rule; got:\n{stdout}"
    );
    assert_eq!(
        line("reshown="),
        shown,
        "`show(true)` must put the element back exactly as it was — the \
         attribute gone and the inline declaration removed, not left at some \
         value `show` invented; got:\n{stdout}"
    );
}

#[test]
fn a60_the_ssr_twin_serves_a_hidden_element_with_the_inline_display_too() {
    let stdout = build_and_run_process("show_ssr", SHOW_SSR);
    assert_eq!(
        stdout,
        "<div id=\"shown\"></div>\n\
         <div id=\"gone\" hidden=\"\" style=\"display:none\"></div>\n\
         <div style=\"--w:3rem;display:none\" hidden=\"\"></div>\n",
        "a hidden element must be served with both writes, and the inline \
         declaration must join whatever `style_var` already wrote"
    );
}

// --- A71: every reactive child keeps its POSITION ---------------------------
//
// Before this, each reactive form APPENDED what it built, so the first change
// moved its content behind whatever static siblings the chain added after it.
// Each form now opens a `Region` where it is called — an empty text node
// planted at that moment — and inserts before it. The claim these pins make is
// always the same one: a document-order readout of the tree is UNCHANGED by
// the change that rebuilt the content.

/// The tree a flattened line names, in document order, as `tag'text'` tokens.
///
/// Empty text nodes are dropped, which is exactly the `Region` anchors: they
/// are the mechanism, not the claim, and a pin that named them would fail the
/// day the marker changes shape without anything a user can see having moved.
/// `a71_the_anchor_is_an_empty_text_node` is where the mechanism itself is
/// asserted.
fn nodes(line: &str) -> Vec<String> {
    line.split(' ')
        .filter_map(|token| {
            let text = token
                .split_once('\'')
                .map(|(_, rest)| rest.trim_end_matches('\''))
                .unwrap_or("");
            let tag = if token.starts_with("#text") {
                "#text"
            } else {
                token.split('#').next().unwrap_or("")
            };
            if tag.is_empty() || (tag == "#text" && text.is_empty()) {
                return None;
            }
            Some(if text.is_empty() {
                tag.to_string()
            } else {
                format!("{tag}'{text}'")
            })
        })
        .collect()
}

/// The `key=value` readouts a positional app prints, in order.
fn readouts(stdout: &str) -> Vec<(String, Vec<String>)> {
    stdout
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(name, tree)| (name.to_string(), nodes(tree)))
        .collect()
}

const POSITION_HARNESS_TAIL: &str =
    "\nglobal.__tree = () => flatten(documentRoot);\nrequire(\"./app.js\");\n";

/// A `{signal}` carrying a `View`, between two static siblings.
const A71_ELEMENT_CHILD: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, mount_root, view };

fun main() {
	let mark: SignalCell<i32> = Signal::new(1);
	let _root = mount_root("app", || {
		view("main")
			.child(view("header").text("head"))
			.child(mark.map(|n: i32| view("b").text(i"m{n}")))
			.child(view("footer").text("foot"))
	});
	print(i"start={tree()}");
	mark.set(2);
	print(i"once={tree()}");
	mark.set(3);
	print(i"twice={tree()}");
}

[extern("__tree")]
external fun tree(): str;

main();
"#;

/// A71: the `Source<View>` child arm. The view the signal holds is replaced in
/// place — twice, because the first replacement is the one that used to move
/// it and the second is the one that would prove a marker had been consumed.
#[test]
fn a71_a_reactive_element_child_keeps_its_position_when_it_is_replaced() {
    let harness = format!("{DOM_STUB}{POSITION_HARNESS_TAIL}");
    let stdout = build_and_run("a71_element_child", A71_ELEMENT_CHILD, &harness);
    let seen = readouts(&stdout);
    let expected = |mark: &str| {
        vec![
            "root".to_string(),
            "main".to_string(),
            "header'head'".to_string(),
            format!("b'{mark}'"),
            "footer'foot'".to_string(),
        ]
    };
    assert_eq!(
        seen,
        vec![
            ("start".to_string(), expected("m1")),
            ("once".to_string(), expected("m2")),
            ("twice".to_string(), expected("m3")),
        ],
        "a `{{signal}}` element child must stay between its siblings across \
         every replacement; got:\n{stdout}"
    );
}

/// A71's MECHANISM, asserted once: the marker a region keeps its place with is
/// an EMPTY TEXT NODE, not a comment and not a wrapper element. An empty text
/// node serializes to nothing, which is what keeps a browser tree and the
/// `@process` twin's markup byte-comparable (`ssr_differential`).
#[test]
fn a71_the_anchor_is_an_empty_text_node() {
    let harness = format!("{DOM_STUB}{POSITION_HARNESS_TAIL}");
    let stdout = build_and_run("a71_anchor", A71_ELEMENT_CHILD, &harness);
    let start = stdout
        .lines()
        .find_map(|line| line.strip_prefix("start="))
        .expect("the start line");
    let raw: Vec<&str> = start.split(' ').collect();
    let anchors: Vec<&&str> = raw
        .iter()
        .filter(|token| token.starts_with("#text#") && !token.contains('\''))
        .collect();
    assert_eq!(
        anchors.len(),
        1,
        "exactly one empty text node — the region's anchor — should be in the \
         tree; got {raw:?}"
    );
    let main_start = raw
        .iter()
        .position(|token| token.starts_with("main#"))
        .expect("the main element");
    let footer = raw
        .iter()
        .position(|token| token.starts_with("footer#"))
        .expect("the footer");
    let anchor = raw
        .iter()
        .position(|token| token.starts_with("#text#") && !token.contains('\''))
        .expect("the anchor");
    assert!(
        main_start < anchor && anchor < footer,
        "the anchor must sit at the hole's position, before the footer the \
         chain appended after it; got {raw:?}"
    );
}

/// `when` between two static siblings.
const A71_WHEN: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, mount_root, view };

fun main() {
	let show: SignalCell<bool> = Signal::new(false);
	let _root = mount_root("app", || {
		view("main")
			.child(view("header").text("head"))
			.when(show, || view("aside").text("cond"))
			.child(view("footer").text("foot"))
	});
	print(i"off={tree()}");
	show.set(true);
	print(i"on={tree()}");
	show.set(false);
	print(i"off2={tree()}");
	show.set(true);
	print(i"on2={tree()}");
}

[extern("__tree")]
external fun tree(): str;

main();
"#;

/// A71: a `when` whose body used to land after every sibling the chain added
/// later — the wrapper `<span>` A85 names as the cost — instantiates at its
/// own place, and does so again after a full off/on cycle.
#[test]
fn a71_when_toggles_on_between_the_siblings_it_sits_between() {
    let harness = format!("{DOM_STUB}{POSITION_HARNESS_TAIL}");
    let stdout = build_and_run("a71_when", A71_WHEN, &harness);
    let off = vec![
        "root".to_string(),
        "main".to_string(),
        "header'head'".to_string(),
        "footer'foot'".to_string(),
    ];
    let on = vec![
        "root".to_string(),
        "main".to_string(),
        "header'head'".to_string(),
        "aside'cond'".to_string(),
        "footer'foot'".to_string(),
    ];
    assert_eq!(
        readouts(&stdout),
        vec![
            ("off".to_string(), off.clone()),
            ("on".to_string(), on.clone()),
            ("off2".to_string(), off),
            ("on2".to_string(), on),
        ],
        "a `when` must mount its body at its own position, every time; \
         got:\n{stdout}"
    );
}

/// `bind_each` between a header row and a footer row, under every edit the
/// reconciler distinguishes.
const A71_ROWS: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, mount_root, view };

[derive(PartialEq)]
struct Row {
	id: i32,
	label: str,
}

fun main() {
	let rows: SignalCell<List<Row>> = Signal::new([
		Row { id = 1, label = "a" },
		Row { id = 2, label = "b" },
	]);
	let _root = mount_root("app", || {
		view("ul")
			.child(view("li").text("H"))
			.bind_each(rows, |row: Row| row.id, |row: Row| view("li").text(row.label))
			.child(view("li").text("F"))
	});
	print(i"start={tree()}");
	rows.set([
		Row { id = 1, label = "a" },
		Row { id = 2, label = "b" },
		Row { id = 3, label = "c" },
	]);
	print(i"insert={tree()}");
	rows.set([Row { id = 1, label = "a" }, Row { id = 3, label = "c" }]);
	print(i"remove={tree()}");
	rows.set([Row { id = 3, label = "c" }, Row { id = 1, label = "a" }]);
	print(i"reorder={tree()}");
	rows.set([Row { id = 3, label = "C" }, Row { id = 1, label = "a" }]);
	print(i"refresh={tree()}");
}

[extern("__tree")]
external fun tree(): str;

main();
"#;

/// A71: keyed rows stay between the header row and the footer row through an
/// insert, a remove, a reorder and a value refresh — the four things the
/// reconciler's order pass can do, each of which used to re-append the whole
/// run after the footer.
#[test]
fn a71_bind_each_rows_stay_between_the_header_and_the_footer() {
    let harness = format!("{DOM_STUB}{POSITION_HARNESS_TAIL}");
    let stdout = build_and_run("a71_rows", A71_ROWS, &harness);
    let tree = |labels: &[&str]| {
        let mut expected = vec!["root".to_string(), "ul".to_string(), "li'H'".to_string()];
        for label in labels {
            expected.push(format!("li'{label}'"));
        }
        expected.push("li'F'".to_string());
        expected
    };
    assert_eq!(
        readouts(&stdout),
        vec![
            ("start".to_string(), tree(&["a", "b"])),
            ("insert".to_string(), tree(&["a", "b", "c"])),
            ("remove".to_string(), tree(&["a", "c"])),
            ("reorder".to_string(), tree(&["c", "a"])),
            ("refresh".to_string(), tree(&["C", "a"])),
        ],
        "`bind_each`'s rows must stay between the header and the footer under \
         every edit; got:\n{stdout}"
    );
}

/// A `{signal}` carrying a `List<View>` — the reactive RUN arm — and a `swap`,
/// each between two static siblings.
const A71_RUN_AND_SWAP: &str = r#"import std::io::print;
import std::range::Range;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, mount_root, view };

fun main() {
	let count: SignalCell<i32> = Signal::new(1);
	let page: SignalCell<i32> = Signal::new(1);
	let _root = mount_root("app", || {
		view("main")
			.child(view("header").text("head"))
			.child(count.map(|n: i32| {
				mut run: List<View> = [];
				for index in Range::new(0, n) {
					run.push(view("i").text(i"g{index}"));
				}
				run
			}))
			.child(view("hr"))
			.swap(page, |n: i32| view("section").text(i"p{n}"))
			.child(view("footer").text("foot"))
	});
	print(i"start={tree()}");
	count.set(3);
	print(i"grown={tree()}");
	page.set(2);
	print(i"swapped={tree()}");
	count.set(2);
	page.set(3);
	print(i"both={tree()}");
}

[extern("__tree")]
external fun tree(): str;

main();
"#;

/// A71: the `Source<List<View>>` arm and `swap`, in one tree, so the two
/// regions have to keep their places from each other as well as from the
/// static siblings around them.
#[test]
fn a71_a_reactive_run_and_a_swap_each_keep_their_own_place() {
    let harness = format!("{DOM_STUB}{POSITION_HARNESS_TAIL}");
    let stdout = build_and_run("a71_run_swap", A71_RUN_AND_SWAP, &harness);
    let tree = |run: usize, page: &str| {
        let mut expected = vec![
            "root".to_string(),
            "main".to_string(),
            "header'head'".to_string(),
        ];
        for index in 0..run {
            expected.push(format!("i'g{index}'"));
        }
        expected.push("hr".to_string());
        expected.push(format!("section'{page}'"));
        expected.push("footer'foot'".to_string());
        expected
    };
    assert_eq!(
        readouts(&stdout),
        vec![
            ("start".to_string(), tree(1, "p1")),
            ("grown".to_string(), tree(3, "p1")),
            ("swapped".to_string(), tree(3, "p2")),
            ("both".to_string(), tree(2, "p3")),
        ],
        "a reactive run and a `swap` must each stay where they were written; \
         got:\n{stdout}"
    );
}

// --- A46: a fragment is a child that keeps its position ----------------------

/// A static fragment between two static siblings, and a REACTIVE one — a
/// signal whose value is a fragment — between two more. The reactive half is
/// the piece A46's own recommendation left open and A71 closed: a
/// `Source<List<View>>` places through a region, so the run is replaced in
/// place instead of re-appended behind whatever the chain added after it.
const A46_FRAGMENT: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, mount_root, view };

fun pair(): List<View> {
	<>
		<i>"a"</i>
		<b>"b"</b>
	</>
}

fun main() {
	let count: SignalCell<i32> = Signal::new(1);
	let _root = mount_root("app", || {
		<main>
			<header>"head"</header>
			{pair()}
			<hr />
			{count.map(|n: i32| <>
				<q>{i"g{n}"}</q>
				<r>{i"h{n}"}</r>
			</>)}
			<footer>"foot"</footer>
		</main>
	});
	print(i"start={tree()}");
	count.set(3);
	print(i"grown={tree()}");
	count.set(2);
	print(i"shrunk={tree()}");
}

[extern("__tree")]
external fun tree(): str;

main();
"#;

/// A46: the static fragment's two elements sit where the fragment was written,
/// and the reactive fragment's run stays between the `<hr />` and the footer
/// across every change — the wrapper element A46 exists to remove, removed.
#[test]
fn a46_a_fragment_places_its_run_in_position_statically_and_reactively() {
    let harness = format!("{DOM_STUB}{POSITION_HARNESS_TAIL}");
    let stdout = build_and_run("a46_fragment", A46_FRAGMENT, &harness);
    // Element syntax lowers every child to `.child(…)`, so a quoted string is
    // a real TEXT NODE beside its element — which is why each tag here is
    // followed by its own `#text'…'` rather than carrying the text itself.
    let tree = |mark: i32| {
        vec![
            "root".to_string(),
            "main".to_string(),
            "header".to_string(),
            "#text'head'".to_string(),
            "i".to_string(),
            "#text'a'".to_string(),
            "b".to_string(),
            "#text'b'".to_string(),
            "hr".to_string(),
            "q".to_string(),
            format!("#text'g{mark}'"),
            "r".to_string(),
            format!("#text'h{mark}'"),
            "footer".to_string(),
            "#text'foot'".to_string(),
        ]
    };
    assert_eq!(
        readouts(&stdout),
        vec![
            ("start".to_string(), tree(1)),
            ("grown".to_string(), tree(3)),
            ("shrunk".to_string(), tree(2)),
        ],
        "a fragment must place its run at its own position, static or \
         reactive; got:\n{stdout}"
    );
}

/// A46's SSR twin: the same fragment serializes as the run it is, with no
/// marker and no wrapper — which is what keeps the browser tree and the
/// served markup comparable (`ssr_differential`'s rule, asserted here on the
/// static half, which is the only half a server render has).
const A46_FRAGMENT_SSR: &str = r#"import std::io::print;
import std::ui::{ View, render, view };

fun pair(): List<View> {
	<><i>"a"</i><b>"b"</b></>
}

fun main() {
	print(render(<main><header>"head"</header>{pair()}<footer>"foot"</footer></main>));
}

main();
"#;

#[test]
fn a46_the_ssr_twin_serializes_a_fragment_as_its_run() {
    let stdout = build_and_run_process("a46_fragment_ssr", A46_FRAGMENT_SSR);
    assert_eq!(
        stdout.trim(),
        "<main><header>head</header><i>a</i><b>b</b><footer>foot</footer></main>",
        "a fragment must serialize as its children and nothing else; \
         got:\n{stdout}"
    );
}
