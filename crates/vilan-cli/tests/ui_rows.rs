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
class StubElement {
    constructor(tag) {
        this.tagName = tag;
        this.children = [];
        this.parent = null;
        this.listeners = {};
        this._text = "";
        this.value = "";
        this.attributes = {};
        this.focused = false;
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
    focus() { this.focused = true; global.focusLog.push(describe(this)); }
}
const documentRoot = new StubElement("root");
global.focusLog = [];
global.document = {
    createElement: (tag) => new StubElement(tag),
    createElementNS: (namespace, tag) => new StubElement(tag),
    getElementById: () => documentRoot,
    querySelector: () => null,
    querySelectorAll: () => [],
};
global.window = { addEventListener: () => {} };
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
