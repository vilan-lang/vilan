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

mod support;

/// A fresh temp directory for one test's project tree.
fn temp_project(tag: &str) -> PathBuf {
    let dir = support::scratch_root().join(format!("vilan_ui_rows_{tag}_{}", std::process::id()));
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
const DOM_STUB: &str = concat!(
    include_str!("support/dom/stub.js"),
    include_str!("support/dom/ui_rows.js"),
);

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
/// derive it — the case that has no spelling without `each_by`.
const THREE_FORMS: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, each, each_by, each_values, mount_root, view };

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
			.child(view("ul").child(each(keyed, |task| task.id, |task| {
				print(i"keyed renders {task.id}");
				view("li").text(task.title)
			})))
			.child(view("ol").child(each_values(names, |name| {
				print(i"values renders {name}");
				view("li").text(name)
			})))
			.child(view("nav").child(each_by(handles, |handle| handle.id, |handle| {
				print(i"by renders {handle.get().id}");
				view("li").bind_text(handle.map(|current| current.title))
			})))
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
/// `each_by`, where the new item is written into the row's own cell and the
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
        "a changed value must rebuild the row under `each` and \
         `each_values` and ONLY under those; got:\n{stdout}"
    );

    // A reorder rebuilds nothing at all, in any of the three.
    let after_reorder: Vec<&&str> = lines[reordered + 1..lines.len() - 1].iter().collect();
    assert!(
        after_reorder.is_empty(),
        "a reorder must move rows, never rebuild them; got:\n{stdout}"
    );

    // And the final tree: `each_by`'s row kept its identity across the
    // value change while the other two took fresh ones, and every list is in
    // the reordered order.
    let tree = lines.last().expect("the flattened tree");
    assert!(
        tree.contains("li#") && tree.contains("'ONE'") && tree.contains("'A'"),
        "the tree did not take the edits; got:\n{stdout}"
    );
}

/// The row `each_by` keeps is the SAME element — identity, not just
/// content — and the item it now holds reached the row through its own cell.
const KEPT_IDENTITY: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, each_by, mount_root, view };

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
		view("ul").child(each_by(handles, |handle| handle.id, |handle| {
			view("li").bind_text(handle.map(|current| current.title))
		}))
	});
	print(i"before={identity_of_first_row()}");
	handles.set([Handle { id = 1, title = "ONE", act = || "act" }]);
	print(i"after={identity_of_first_row()}");
}

[extern("__first_row")]
external fun identity_of_first_row(): str;

main();
"#;

/// `each_by` keeps the row's element across a value change under a
/// surviving key: same identity before and after, and the text updated through
/// the binding rather than through a rebuild. Red under `each` — a
/// `PartialEq` change there disposes the row and builds a new element.
#[test]
fn the_index_form_keeps_the_rows_element_and_updates_through_its_cell() {
    let harness = format!(
        "{DOM_STUB}\nglobal.__first_row = () => {{\n  \
         const list = documentRoot.children[0];\n  \
         const row = list.children.find((node) => node.tagName !== \"#text\");\n  \
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
/// `each` rows — the initial one and one appended after the fact.
const MOUNT_HOOK: &str = r#"import std::dom::Element;
import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, each_values, mount_root, view, when };

fun main() {
	let open: SignalCell<bool> = Signal::new(false);
	let rows: SignalCell<List<str>> = Signal::new(["a"]);
	let _root = mount_root("app", || {
		view("div")
			.child(view("input").on_mount(|element| print(i"static {reachable(element)}")))
			.child(view("input").autofocus())
			.child(when(open, || {
				view("section").child(view("input").on_mount(|element| {
					print(i"when {reachable(element)}");
				}))
			}))
			.child(view("ul").child(each_values(rows, |name| {
				view("li").text(name).on_mount(|element| print(i"row {reachable(element)}"))
			})))
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
import std::ui::{ View, mount_root, view, when };

fun main() {
	let open: SignalCell<bool> = Signal::new(false);
	let _root = mount_root("app", || {
		view("div")
			.child(view("input").attr("name", "always"))
			.child(when(open, || view("input").attr("name", "modal").autofocus()))
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
// and this is the pin. What makes the claim measurable is the SHARED stub's
// own focus model (`support/dom/stub.js`, ui-40): `focus()` refuses a target
// whose resolved `visibility` is `hidden` (inherited, as the platform
// resolves it) and logs `!hidden`, `document.activeElement` tracks what
// actually took focus, and `matches(":focus")` reads it back. This suite
// carried a PRIVATE `focus()` override reading a `data-visibility` marker
// until N122 retired it onto that model; what it still adds is its own frame
// clock — a `requestAnimationFrame` that fires nothing until the test says
// `flushFrame()`. A frame that never comes on its own is the point: "on the
// next frame" is a claim about ORDER, and a real rAF would let a pass mean
// "eventually".
const DOM_STUB_FRAMES: &str = r##"
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

/// The overlay's shape: a panel mounted `visibility: hidden`, `autofocus`
/// chained onto its input. Nothing here mentions a frame — that is the whole
/// point of the one-word form.
const HIDDEN_AUTOFOCUS: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, mount_root, view, when };

fun main() {
	let open: SignalCell<bool> = Signal::new(false);
	let placed: SignalCell<str> = Signal::new("hidden");
	let _root = mount_root("app", || {
		view("div").child(when(open, || {
			view("input").attr("name", "modal").style_var("visibility", placed).autofocus()
		}))
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
         panel.style.setProperty(\"visibility\", \"visible\");\n  \
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
        line("microtask=").ends_with("!hidden"),
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
        attempts[1].ends_with("@doc"),
        "the first frame after the flip takes focus (the shared stub logs a \
         taken focus bare, a refused one with its reason); got:\n{stdout}"
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

/// N122: the shared stub REFUSES the `focus()` the platform would, and says
/// why. `support/dom/stub.js` is read by eight suites, and ui-40 gave its
/// `focus()` two fidelity rules every focus pin now stands on — a target must
/// be a FOCUSABLE AREA (a bare `<div>` is not; any written `tabindex` makes one
/// programmatically focusable), and it must not resolve `visibility: hidden`
/// (inherited) — so they are pinned through a program: a refused focus leaves
/// `document.activeElement` where it was, reads `false` from
/// `matches(":focus")`, dispatches no `focusin`, and logs its reason.
const FOCUS_REFUSALS: &str = r#"import std::dom::{ active_element, create_element, get_element_by_id, is_null, window };
import std::io::print;

fun main() {
	mut heard = 0;
	let _heard = window().listen_capture("focusin", |_event| {
		heard = heard + 1;
	});
	let root = get_element_by_id("app");
	let bare = create_element("div");
	root.append(bare);
	bare.focus();
	print(i"bare took={bare.matches(":focus")} active_null={is_null(active_element())} heard={heard}");
	let programmatic = create_element("div");
	programmatic.set_attribute("tabindex", "-1");
	root.append(programmatic);
	programmatic.focus();
	print(i"negative_tabindex took={programmatic.matches(":focus")} heard={heard}");
	let shade = create_element("section");
	root.append(shade);
	let inside = create_element("input");
	shade.append(inside);
	shade.set_style_property("visibility", "hidden");
	inside.focus();
	print(i"hidden_ancestor took={inside.matches(":focus")} kept={programmatic.matches(":focus")} heard={heard}");
}

main();
"#;

#[test]
fn n122_the_shared_stub_refuses_a_focus_the_platform_would() {
    let harness = format!(
        "{DOM_STUB}\nrequire(\"./app.js\");\nconsole.log(\"log=\" + focusLog.join(\"|\"));\n"
    );
    let stdout = build_and_run("n122_refusals", FOCUS_REFUSALS, &harness);
    let line = |key: &str| -> String {
        stdout
            .lines()
            .find(|line| line.starts_with(key))
            .unwrap_or_else(|| panic!("no {key:?} line in:\n{stdout}"))
            .to_string()
    };
    assert_eq!(
        line("bare "),
        "bare took=false active_null=true heard=0",
        "a bare `<div>` is not a focusable area: the request does nothing, \
         silently, as the platform's does; got:\n{stdout}"
    );
    assert_eq!(
        line("negative_tabindex "),
        "negative_tabindex took=true heard=1",
        "a written `tabindex`, even a negative one, makes an element \
         programmatically focusable — and a focus that takes dispatches \
         `focusin`; got:\n{stdout}"
    );
    assert_eq!(
        line("hidden_ancestor "),
        "hidden_ancestor took=false kept=true heard=1",
        "an input under a `visibility: hidden` ancestor is refused, focus stays \
         where it was, and no `focusin` fires; got:\n{stdout}"
    );
    let log = line("log=");
    let entries: Vec<&str> = log.trim_start_matches("log=").split('|').collect();
    assert_eq!(entries.len(), 3, "one entry per request; got:\n{stdout}");
    assert!(
        entries[0].starts_with("div#") && entries[0].ends_with("!unfocusable"),
        "the refusal names its reason; got:\n{stdout}"
    );
    assert!(
        entries[1].starts_with("div#") && entries[1].ends_with("@doc"),
        "a taken focus is logged bare; got:\n{stdout}"
    );
    assert!(
        entries[2].starts_with("input#") && entries[2].ends_with("!hidden"),
        "the refusal names its reason; got:\n{stdout}"
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

// --- B255: an in-place `update` beside a `each` -------------------------

/// Six keyed rows, then a `remove(0)` performed IN PLACE through
/// `SignalCell::update`. `each` keeps the effect's list as `row_items`,
/// and before B257 that store aliased the cell's own storage — so the next
/// pass handed `reconcile` an `old_items` that WAS the new array, one shorter
/// than the `old_keys` beside it, and `same(old_items[5], item)` read past the
/// end (`index out of bounds: the length is 5 but the index is 5`, kolt
/// channel.vl:54). The build now copies at the assignment, so `old_items` is
/// the snapshot the keys were taken from.
const IN_PLACE_REMOVE: &str = r#"import std::compare::PartialEq;
import std::hash::{ Hash, Hashable };
import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, each, mount_root, view };

struct Row {
	id: i32,
	text: str,
}

// Hand-written, and narrower than the struct: `each`'s key here is the
// ITEM, so identity is the id and a surviving row is one whose id survived.
impl Row with PartialEq {
	fun eq(self, b: Row): bool {
		self.id == b.id
	}
}

// A125's obligation, and the reason `[derive(Hashable)]` is WRONG here: the
// derive hashes the whole value, and two rows with one id and two labels are
// `==` above while hashing apart — which is the coarse-`==` key the index
// cannot place. The hash reads exactly the field the equality reads.
impl Row with Hashable {
	fun hash(self): Hash {
		self.id.hash()
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
		view("ul").child(each(rows, |row| row, |row| {
			print(i"render {row.id}");
			view("li").text(row.text)
		}))
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
fn b255_an_in_place_remove_under_each_keeps_the_surviving_rows() {
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
import std::ui::{ View, each, mount_root, view };

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
		view("ul").child(each(rows, |row| row.id, |row| {
			print(i"render {row.id}");
			view("li").text(row.text)
		}))
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
fn b255_an_in_place_element_write_under_each_refreshes_that_row() {
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

/// `each_by` holds `row_items` exactly as `each` does, so the
/// out-of-bounds half is its too — its `same` is constantly true, so it never
/// showed the stale-row half. `T` here carries a closure and so cannot compare
/// at all, which is the shape this form exists for.
const IN_PLACE_REMOVE_BY: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, each_by, mount_root, view };

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
		view("ul").child(each_by(rows, |row| row.id, |row| {
			view("li").bind_text(row.map(|current| current.text))
		}))
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
fn b255_an_in_place_remove_under_each_by_keeps_the_surviving_rows() {
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
/// no spelling but a `swap(signal, |x| view)` over it.
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
	print(i"emptied={tree()}");
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
    let emptied = line("emptied=");
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

    // The boundary removed what it placed (A88): each reactive arm's content
    // is out of the document, and so is the marker its region kept its place
    // with. The STATIC arms are untouched — their lifetime is their parent
    // element's, not a boundary's.
    for gone in ["'second'", "'c'", "'two'"] {
        assert!(
            !emptied.contains(gone),
            "a disposed boundary must take the reactive arm's {gone} out of \
             the document; got:\n{stdout}"
        );
    }
    for kept in ["'plain'", "'element'", "'x'", "'y'"] {
        assert!(
            emptied.contains(kept),
            "a static child arm belongs to its parent, not to the boundary, \
             so {kept} must stay; got:\n{stdout}"
        );
    }
    assert!(
        !emptied
            .split(' ')
            .any(|token| token.starts_with("#text#") && !token.contains('\'')),
        "a closed region leaves no anchor behind; got:\n{stdout}"
    );

    // And the subscriptions died with the root owner: three more writes, no
    // change to the tree at all.
    assert_eq!(
        disposed, emptied,
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
import std::ui::{ View, mount_root, view, when };

fun main() {
	let show: SignalCell<bool> = Signal::new(false);
	let _root = mount_root("app", || {
		view("main")
			.child(view("header").text("head"))
			.child(when(show, || view("aside").text("cond")))
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

/// `each` between a header row and a footer row, under every edit the
/// reconciler distinguishes.
const A71_ROWS: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, each, mount_root, view };

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
			.child(each(rows, |row: Row| row.id, |row: Row| view("li").text(row.label)))
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
fn a71_each_rows_stay_between_the_header_and_the_footer() {
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
        "`each`'s rows must stay between the header and the footer under \
         every edit; got:\n{stdout}"
    );
}

/// A `{signal}` carrying a `List<View>` — the reactive RUN arm — and a `swap`,
/// each between two static siblings.
const A71_RUN_AND_SWAP: &str = r#"import std::io::print;
import std::range::Range;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, mount_root, swap, view };

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
			.child(swap(page, |n: i32| view("section").text(i"p{n}")))
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

// --- A88: a boundary REMOVES what it placed ---------------------------------
//
// Disposal used to end the subscriptions and leave the DOM alone, on the
// assumption that a disposed subtree leaves with its parent. That is true for
// a component under its own root and FALSE for a PORTAL, whose container
// outlives the boundary that filled it — kolt's overlay hand-wrote a
// `defer(|| live_panel.remove())` saying exactly this. `Region::close` now
// takes the live nodes out of the document before dropping the anchor, so the
// portal container is empty after the owner goes, for every form at once.

/// One portal container, filled by a separate boundary. Each form is given its
/// own container so the readout says WHICH one leaked, and the containers are
/// mounted into the document up front, so they are the survivors and the
/// boundary is the thing that goes.
const A88_PORTAL: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell, comp };
import std::ui::{ View, each, mount, swap, view, when };

[derive(PartialEq)]
struct Row {
	id: i32,
	label: str,
}

fun main() {
	let shell = view("div");
	let when_host = view("section");
	let swap_host = view("section");
	let rows_host = view("section");
	let signal_host = view("section");
	let run_host = view("section");
	mount("app", shell
		.child(when_host)
		.child(swap_host)
		.child(rows_host)
		.child(signal_host)
		.child(run_host));
	let flag: SignalCell<bool> = Signal::new(true);
	let page: SignalCell<i32> = Signal::new(1);
	let rows: SignalCell<List<Row>> = Signal::new([
		Row { id = 1, label = "a" },
		Row { id = 2, label = "b" },
	]);
	let one: SignalCell<View> = Signal::new(view("u").text("u"));
	let many: SignalCell<List<View>> = Signal::new([view("s").text("s")]);
	let (_built, scope) = comp(|| {
		when_host.child(when(flag, || view("aside").text("cond")));
		swap_host.child(swap(page, |n: i32| view("article").text(i"p{n}")));
		rows_host.child(each(rows, |row: Row| row.id, |row: Row| view("li").text(row.label)));
		signal_host.child(one);
		run_host.child(many)
	});
	print(i"live={tree()}");
	scope.dispose();
	print(i"disposed={tree()}");
}

[extern("__tree")]
external fun tree(): str;

main();
"#;

/// A88: after the boundary is disposed every portal container is EMPTY —
/// content and anchor both — on all five forms. Before this, each container
/// kept its live element (or its rows) and its marker text node for as long as
/// the container itself lived.
#[test]
fn a88_a_disposed_boundary_empties_the_portal_containers_it_filled() {
    let harness = format!("{DOM_STUB}{POSITION_HARNESS_TAIL}");
    let stdout = build_and_run("a88_portal", A88_PORTAL, &harness);
    let seen = readouts(&stdout);
    assert_eq!(
        seen[0],
        (
            "live".to_string(),
            vec![
                "root".to_string(),
                "div".to_string(),
                "section".to_string(),
                "aside'cond'".to_string(),
                "section".to_string(),
                "article'p1'".to_string(),
                "section".to_string(),
                "li'a'".to_string(),
                "li'b'".to_string(),
                "section".to_string(),
                "u'u'".to_string(),
                "section".to_string(),
                "s's'".to_string(),
            ]
        ),
        "every form must be live before the disposal; got:\n{stdout}"
    );
    assert_eq!(
        seen[1],
        (
            "disposed".to_string(),
            vec![
                "root".to_string(),
                "div".to_string(),
                "section".to_string(),
                "section".to_string(),
                "section".to_string(),
                "section".to_string(),
                "section".to_string(),
            ]
        ),
        "a disposed boundary must leave nothing behind in a container that \
         survives it; got:\n{stdout}"
    );
}

/// A88's MECHANISM, asserted where the readout above cannot see it: the
/// ANCHORS go too. `nodes` drops empty text nodes deliberately (they are the
/// mechanism, not the claim), so the raw line is read here — five regions
/// planted five markers, and after the disposal there are none.
#[test]
fn a88_the_anchors_go_with_the_content() {
    let harness = format!("{DOM_STUB}{POSITION_HARNESS_TAIL}");
    let stdout = build_and_run("a88_anchors", A88_PORTAL, &harness);
    let anchors = |prefix: &str| {
        stdout
            .lines()
            .find_map(|line| line.strip_prefix(prefix))
            .unwrap_or_else(|| panic!("the {prefix} line in:\n{stdout}"))
            .split(' ')
            .filter(|token| token.starts_with("#text#") && !token.contains('\''))
            .count()
    };
    // Five region anchors, plus one marker per ROW (A91): the `when`'s
    // instantiation, the `swap`'s subtree and the two `each` rows are
    // rows and carry one each; the two reactive CHILD arms place the views
    // they were handed and carry none.
    assert_eq!(
        anchors("live="),
        9,
        "five region anchors and four row markers:\n{stdout}"
    );
    assert_eq!(
        anchors("disposed="),
        0,
        "a closed region leaves no marker behind:\n{stdout}"
    );
}

/// A85: the five value forms, each in a MIDDLE position of one chain, so every
/// region has to keep its place from the static siblings AND from the other
/// four. This is the shape the item exists for — `<ul>{header}{each(..)}
/// {when(..)}</ul>` — and it is unwritable with the methods, which can only
/// append.
const A85_VALUE_FORMS: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, each, each_by, each_values, mount_root, swap, view, when };

fun main() {
	let rows: SignalCell<List<str>> = Signal::new(["a", "b"]);
	let more: SignalCell<bool> = Signal::new(false);
	let page: SignalCell<i32> = Signal::new(1);
	let _root = mount_root("app", || {
		view("main")
			.child(view("header").text("H"))
			.child(each(rows, |item: str| item, |item: str| view("li").text(item)))
			.child(view("hr"))
			.child(each_values(rows, |item: str| view("p").text(item)))
			.child(view("hr"))
			.child(each_by(rows, |item| item, |cell| view("q").bind_text(cell)))
			.child(when(more, || view("b").text("M")))
			.child(swap(page, |n: i32| view("section").text(i"p{n}")))
			.child(view("footer").text("F"))
	});
	print(i"start={tree()}");
	more.set(true);
	print(i"more={tree()}");
	rows.set(["b", "a", "c"]);
	print(i"rows={tree()}");
	page.set(2);
	print(i"page={tree()}");
	more.set(false);
	print(i"less={tree()}");
}

[extern("__tree")]
external fun tree(): str;

main();
"#;

/// The tree `A85_VALUE_FORMS` must show: the header, the three runs separated
/// by their `hr`s, the conditional, the swap, the footer — in the order the
/// chain writes them, never at the end.
fn a85_tree(rows: &[&str], more: bool, page: &str) -> Vec<String> {
    let mut expected = vec![
        "root".to_string(),
        "main".to_string(),
        "header'H'".to_string(),
    ];
    for row in rows {
        expected.push(format!("li'{row}'"));
    }
    expected.push("hr".to_string());
    for row in rows {
        expected.push(format!("p'{row}'"));
    }
    expected.push("hr".to_string());
    for row in rows {
        expected.push(format!("q'{row}'"));
    }
    if more {
        expected.push("b'M'".to_string());
    }
    expected.push(format!("section'{page}'"));
    expected.push("footer'F'".to_string());
    expected
}

/// A85's whole claim, in one tree: a conditional, a swap and three keyed runs
/// each sit where their `{hole}` was written, between static siblings and
/// between each other, across a toggle, a reorder-with-insert and a swap.
#[test]
fn a85_the_five_value_forms_place_where_their_hole_is() {
    let harness = format!("{DOM_STUB}{POSITION_HARNESS_TAIL}");
    let stdout = build_and_run("a85_value_forms", A85_VALUE_FORMS, &harness);
    assert_eq!(
        readouts(&stdout),
        vec![
            ("start".to_string(), a85_tree(&["a", "b"], false, "p1")),
            ("more".to_string(), a85_tree(&["a", "b"], true, "p1")),
            ("rows".to_string(), a85_tree(&["b", "a", "c"], true, "p1")),
            ("page".to_string(), a85_tree(&["b", "a", "c"], true, "p2")),
            ("less".to_string(), a85_tree(&["b", "a", "c"], false, "p2")),
        ],
        "every value form must hold the position its child hole was written \
         at; got:\n{stdout}"
    );
}

/// A85 §3e's ownership property, which is the reason the body is a
/// `context`-typed FIELD and not a plain closure: a `when` VALUE's body runs
/// under the owner ambient at `place`, and toggling the condition off disposes
/// the instantiation — so the effect the body registered stops firing, where a
/// plain-closure field would have registered it into whatever boundary the
/// VALUE was built in and kept it alive (measured by slots-33, and still the
/// control in `inference/bounds.rs`).
const A85_OWNERSHIP: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell, Source };
import std::ui::{ View, mount_root, view, when };

fun main() {
	let label: SignalCell<str> = Signal::new("one");
	let on: SignalCell<bool> = Signal::new(true);
	let _root = mount_root("app", || {
		view("main").child(when(on, || {
			label.effect(|value: str| print(i"body sees {value}"));
			view("b").text("body")
		}))
	});
	label.set("two");
	on.set(false);
	// Nothing may answer this: the instantiation is gone, and the body's
	// effect went with its owner.
	label.set("three");
	label.set("four");
	// Toggling back on rebuilds from scratch, and the fresh body reads the
	// CURRENT value — which is why "three" is written and then overwritten:
	// a leak shows up as an extra line, not as a different one.
	on.set(true);
}

main();
"#;

#[test]
fn a85_a_toggled_off_when_value_disposes_the_body_it_built() {
    let harness = format!("{DOM_STUB}{POSITION_HARNESS_TAIL}");
    let stdout = build_and_run("a85_ownership", A85_OWNERSHIP, &harness);
    assert_eq!(
        stdout, "body sees one\nbody sees two\nbody sees four\n",
        "a toggled-off `when` value must dispose its body's owner: the two \
         writes while it is off answer nothing, and the line after the toggle \
         back on is the FRESH body reading the current value. A body stored as \
         a plain closure would have kept the first instantiation's effect \
         alive under the enclosing boundary and printed `three` too; \
         got:\n{stdout}"
    );
}

/// A85 §3d: the server twin's value forms. A render is one pass in source
/// order, so a value placed in a child hole writes its content exactly there —
/// and the markup is what the CHAIN below it produces, which is the claim the
/// two halves are held to. A99 retired the `View` methods, so the second half
/// is the element-syntax `{hole}` spelling of the same three values rather
/// than `.each_values(..)`/`.when(..)`/`.swap(..)`.
const A85_VALUE_FORMS_SSR: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, each_values, render, swap, view, when };

fun main() {
	let rows: SignalCell<List<str>> = Signal::new(["a", "b"]);
	let more: SignalCell<bool> = Signal::new(true);
	let page: SignalCell<i32> = Signal::new(1);
	print(render(view("main")
		.child(view("header").text("H"))
		.child(each_values(rows, |item: str| view("li").text(item)))
		.child(when(more, || view("b").text("M")))
		.child(swap(page, |n: i32| view("section").text(i"p{n}")))
		.child(view("footer").text("F"))));
	print(render(<main>
		<header>"H"</header>
		{each_values(rows, |item: str| view("li").text(item))}
		{when(more, || view("b").text("M"))}
		{swap(page, |n: i32| view("section").text(i"p{n}"))}
		<footer>"F"</footer>
	</main>));
}

main();
"#;

/// The twin renders what the value places, and the `child` spelling and the
/// `{hole}` spelling agree byte for byte — which they must, since a hole IS a
/// `child` call after the desugar.
#[test]
fn a85_the_ssr_twins_of_the_value_forms_render_what_the_methods_do() {
    let stdout = build_and_run_process("a85_value_forms_ssr", A85_VALUE_FORMS_SSR);
    let expected = "<main><header>H</header><li>a</li><li>b</li><b>M</b>\
                    <section>p1</section><footer>F</footer></main>";
    assert_eq!(
        stdout,
        format!("{expected}\n{expected}\n"),
        "the server twin must render a value form exactly where its hole is, \
         and exactly as the `child` form does"
    );
}

/// A91: a render closure yields any `Slot`. Three runs over the same list, one
/// per row shape the widening admits — a FRAGMENT row (two elements), a TEXT
/// row (one text node), and a VALUE-FORM row (a `when`, which grows and shrinks
/// after it was placed and is the shape no snapshot of a row's nodes could
/// follow) — driven through a reorder-with-insert, a toggle and a removal.
const A91_ROW_SHAPES: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, each_values, mount_root, view, when };

fun main() {
	let rows: SignalCell<List<str>> = Signal::new(["a", "b"]);
	let flag: SignalCell<bool> = Signal::new(true);
	let _root = mount_root("app", || {
		view("main")
			.child(view("header").text("H"))
			.child(each_values(rows, |item: str| [view("i").text(item), view("b").text(item)]))
			.child(view("hr"))
			.child(each_values(rows, |item: str| item))
			.child(view("em").text("E"))
			.child(each_values(rows, |item: str| when(flag, || view("u").text(item))))
			.child(view("footer").text("F"))
	});
	print(i"start={tree()}");
	rows.set(["b", "a", "c"]);
	print(i"reorder={tree()}");
	flag.set(false);
	print(i"off={tree()}");
	rows.set(["c"]);
	print(i"remove={tree()}");
	flag.set(true);
	print(i"on={tree()}");
}

[extern("__tree")]
external fun tree(): str;

main();
"#;

/// The tree `A91_ROW_SHAPES` must show for a given list and `when` state.
fn a91_tree(rows: &[&str], on: bool) -> Vec<String> {
    let mut expected = vec![
        "root".to_string(),
        "main".to_string(),
        "header'H'".to_string(),
    ];
    for row in rows {
        expected.push(format!("i'{row}'"));
        expected.push(format!("b'{row}'"));
    }
    expected.push("hr".to_string());
    for row in rows {
        expected.push(format!("#text'{row}'"));
    }
    expected.push("em'E'".to_string());
    if on {
        for row in rows {
            expected.push(format!("u'{row}'"));
        }
    }
    expected.push("footer'F'".to_string());
    expected
}

/// The whole of A91 in one tree: every row shape keeps the run's order through
/// a reorder-with-insert, a removal, and a change INSIDE a row that the
/// reconciler never sees. The last readout is the one that would catch a
/// removed row's owner surviving: the two rows that left would put their `u`
/// back when the flag turns on again.
#[test]
fn a91_a_fragment_a_text_and_a_value_form_row_reorder_and_remove_as_units() {
    let harness = format!("{DOM_STUB}{POSITION_HARNESS_TAIL}");
    let stdout = build_and_run("a91_row_shapes", A91_ROW_SHAPES, &harness);
    assert_eq!(
        readouts(&stdout),
        vec![
            ("start".to_string(), a91_tree(&["a", "b"], true)),
            ("reorder".to_string(), a91_tree(&["b", "a", "c"], true)),
            ("off".to_string(), a91_tree(&["b", "a", "c"], false)),
            ("remove".to_string(), a91_tree(&["c"], false)),
            ("on".to_string(), a91_tree(&["c"], true)),
        ],
        "a row of any `Slot` shape must move and die as a unit; got:\n{stdout}"
    );
}

/// The mechanism, asserted where the readout above cannot see it: a row costs
/// one MARKER — an empty text node planted before its content — and a region
/// still costs exactly one anchor. Three runs of two rows each plus their three
/// region anchors is nine empty text nodes; after the run shrinks to one row
/// per list, three rows and three anchors is six.
#[test]
fn a91_a_row_costs_one_empty_text_marker() {
    let harness = format!("{DOM_STUB}{POSITION_HARNESS_TAIL}");
    let stdout = build_and_run("a91_markers", A91_ROW_SHAPES, &harness);
    let markers = |prefix: &str| {
        stdout
            .lines()
            .find_map(|line| line.strip_prefix(prefix))
            .unwrap_or_else(|| panic!("the {prefix} line in:\n{stdout}"))
            .split(' ')
            .filter(|token| token.starts_with("#text#") && !token.contains('\''))
            .count()
    };
    // Three region anchors, two rows each (six markers), and — inside each of
    // the two value-form rows — a `when`'s own region anchor and the row IT
    // placed: 3 + 6 + 2 + 2.
    assert_eq!(markers("start="), 13, "two rows per run:\n{stdout}");
    // Three rows each now: 3 + 9 + 3 + 3.
    assert_eq!(markers("reorder="), 18, "three rows per run:\n{stdout}");
    // One row each, with the `when` toggled OFF, so its region is there and
    // its row is not: 3 + 3 + 1 + 0.
    assert_eq!(markers("remove="), 7, "one row per run:\n{stdout}");
}

/// A91's server twin: the row renders what its `Slot` renders, in place, and a
/// value-form row is a `when` that the server takes or drops. The `child`
/// spelling and the element-syntax `{hole}` spelling agree, as they must (A99
/// retired the `each_values` method the second half used to write).
const A91_ROWS_SSR: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, each_values, render, view, when };

fun main() {
	let rows: SignalCell<List<str>> = Signal::new(["a", "b"]);
	let flag: SignalCell<bool> = Signal::new(true);
	print(render(view("main")
		.child(view("header").text("H"))
		.child(each_values(rows, |item: str| [view("i").text(item), view("b").text(item)]))
		.child(each_values(rows, |item: str| item))
		.child(each_values(rows, |item: str| when(flag, || view("u").text(item))))
		.child(view("footer").text("F"))));
	print(render(<main>
		<header>"H"</header>
		{each_values(rows, |item: str| [view("i").text(item), view("b").text(item)])}
		{each_values(rows, |item: str| item)}
		{each_values(rows, |item: str| when(flag, || view("u").text(item)))}
		<footer>"F"</footer>
	</main>));
}

main();
"#;

// --- A98: the order pass leaves an unmoved row alone -------------------------

/// The counting tail. `__cost()` reports, and resets, the two numbers A98 is
/// about: rows CUT out of the document by the order pass (one `extractContents`
/// each) and rows BUILT from scratch (one `createElement` each, since every row
/// here is one `<li>`). Before A98 the cut count was the whole live run on every
/// pass, whatever the edit was.
const A98_COST_HARNESS_TAIL: &str = r#"
global.__cost = (() => {
    let cut = 0;
    let built = 0;
    const element = document.createElement;
    document.createElement = (tag) => { built += 1; return element(tag); };
    const proto = Object.getPrototypeOf(document.createRange());
    const extract = proto.extractContents;
    proto.extractContents = function (...args) { cut += 1; return extract.apply(this, args); };
    return () => { const line = `cut=${cut} built=${built}`; cut = 0; built = 0; return line; };
})();
global.__tree = () => flatten(documentRoot);
require("./app.js");
"#;

/// Eight edits over one four-row run, each driven from the same starting list
/// so the costs are comparable, and each printed as `pass|tree|cost`. The
/// starting list is restored between them and that restore's cost is discarded:
/// what is being measured is the edit, not the round trip.
const A98_ORDER_PASS: &str = r#"import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, each, mount_root, view };

[derive(PartialEq)]
struct Row {
	id: i32,
	text: str,
}

fun row(id: i32, text: str): Row {
	Row { id = id, text = text }
}

fun main() {
	let base: List<Row> = [row(1, "a"), row(2, "b"), row(3, "c"), row(4, "d")];
	let rows: SignalCell<List<Row>> = Signal::new(base);
	let _root = mount_root("app", || {
		view("ul")
			.child(view("li").text("H"))
			.child(each(rows, |item: Row| item.id, |item: Row| view("li").text(item.text)))
			.child(view("li").text("F"))
	});
	let _built = cost();

	rows.set([row(1, "a"), row(2, "b"), row(3, "c"), row(4, "d"), row(5, "e")]);
	print(i"append|{tree()}|{cost()}");
	rows.set(base);
	let _a = cost();

	rows.set([row(0, "z"), row(1, "a"), row(2, "b"), row(3, "c"), row(4, "d")]);
	print(i"prepend|{tree()}|{cost()}");
	rows.set(base);
	let _b = cost();

	rows.set([row(2, "b"), row(3, "c"), row(4, "d")]);
	print(i"remove-first|{tree()}|{cost()}");
	rows.set(base);
	let _c = cost();

	rows.set([row(1, "a"), row(2, "b"), row(4, "d")]);
	print(i"remove-middle|{tree()}|{cost()}");
	rows.set(base);
	let _d = cost();

	rows.set([row(1, "a"), row(2, "B"), row(3, "c"), row(4, "d")]);
	print(i"relabel|{tree()}|{cost()}");
	rows.set(base);
	let _e = cost();

	rows.set([row(4, "d"), row(1, "a"), row(2, "b"), row(3, "c")]);
	print(i"last-to-front|{tree()}|{cost()}");
	rows.set(base);
	let _f = cost();

	rows.set([row(2, "b"), row(3, "c"), row(4, "d"), row(1, "a")]);
	print(i"first-to-last|{tree()}|{cost()}");
	rows.set(base);
	let _g = cost();

	rows.set([row(4, "d"), row(3, "c"), row(2, "b"), row(1, "a")]);
	print(i"reverse|{tree()}|{cost()}");
}

[extern("__tree")]
external fun tree(): str;

[extern("__cost")]
external fun cost(): str;

main();
"#;

/// A98: the order pass cuts and re-inserts only the rows that MOVED.
///
/// Both halves are asserted per pass, and both are needed. The COST is the
/// claim — an append costs no cut at all where it used to cut the whole run —
/// and the TREE is what says the cheaper pass still produced the right order,
/// which a count alone cannot. The eight edits are the shapes a reconciled run
/// actually takes; `reverse` is the control that is genuinely O(n) and must
/// stay correct rather than get faster.
///
/// Non-vacuity: with the A98 skip removed (every row cut, every row
/// re-inserted) the seven cheap lines read `cut=4`/`cut=3` and the pin is red
/// seven times over.
#[test]
fn a98_the_order_pass_leaves_an_unmoved_row_alone() {
    let harness = format!("{DOM_STUB}{A98_COST_HARNESS_TAIL}");
    let stdout = build_and_run("a98_order_pass", A98_ORDER_PASS, &harness);
    let seen: Vec<(String, Vec<String>, String)> = stdout
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('|');
            let name = parts.next()?.to_string();
            let tree = nodes(parts.next()?);
            let cost = parts.next()?.to_string();
            Some((name, tree, cost))
        })
        .collect();
    let tree = |labels: &[&str]| {
        let mut expected = vec!["root".to_string(), "ul".to_string(), "li'H'".to_string()];
        for label in labels {
            expected.push(format!("li'{label}'"));
        }
        expected.push("li'F'".to_string());
        expected
    };
    let expected: Vec<(String, Vec<String>, String)> = vec![
        // An appended row: nothing already in the run moved, so nothing is cut,
        // and the one new row is built before the anchor.
        ("append", tree(&["a", "b", "c", "d", "e"]), "cut=0 built=1"),
        // A prepended row: the four survivors are still in ascending order, so
        // the new row is threaded in before the first one's marker.
        ("prepend", tree(&["z", "a", "b", "c", "d"]), "cut=0 built=1"),
        // A removal cuts exactly the row that is going, wherever it sat.
        ("remove-first", tree(&["b", "c", "d"]), "cut=1 built=0"),
        ("remove-middle", tree(&["a", "b", "d"]), "cut=1 built=0"),
        // A changed value rebuilds that row alone: the cut is its own, and the
        // three rows around it never move.
        ("relabel", tree(&["a", "B", "c", "d"]), "cut=1 built=1"),
        // One row dragged across the whole run, both directions. The backward
        // scan settles the other three for the first, the forward scan for the
        // second — which is why both scans exist.
        (
            "last-to-front",
            tree(&["d", "a", "b", "c"]),
            "cut=1 built=0",
        ),
        (
            "first-to-last",
            tree(&["b", "c", "d", "a"]),
            "cut=1 built=0",
        ),
        // The control: a reverse genuinely moves everything but one row.
        ("reverse", tree(&["d", "c", "b", "a"]), "cut=3 built=0"),
    ]
    .into_iter()
    .map(|(name, tree, cost)| (name.to_string(), tree, cost.to_string()))
    .collect();
    assert_eq!(
        seen, expected,
        "A98: only a row whose position changed may be cut and re-inserted, and \
         the run must still read in the new order; got:\n{stdout}"
    );
}

#[test]
fn a91_the_ssr_twin_renders_what_each_row_shape_renders() {
    let stdout = build_and_run_process("a91_rows_ssr", A91_ROWS_SSR);
    let expected = "<main><header>H</header><i>a</i><b>a</b><i>b</i><b>b</b>ab\
                    <u>a</u><u>b</u><footer>F</footer></main>";
    assert_eq!(
        stdout,
        format!("{expected}\n{expected}\n"),
        "the server twin must render a fragment row, a text row and a \
         value-form row as what they are, in place"
    );
}

// --- A110 door 1: nested `swap`s on one source ------------------------------

/// The shape the item was filed from (kolt `views.vl:72`), minimized: an OUTER
/// `swap` keyed on a PROJECTION of the route and an INNER `swap` keyed on the
/// route itself, so both forms stand on one source and the outer reaches it one
/// derivation — one wave — later than the inner.
///
/// Projecting the outer key is the right way to stop the outer form rebuilding
/// on every navigation (`place_swap` dedups on `==`), and it is what the guide
/// teaches. The cost, before door 1, was that the inner form's effect and the
/// outer's owner disposal raced: the inner rendered under an `Owner` the outer's
/// `defer` had already run, into a `Region` whose anchor `close()` had already
/// removed — a subtree nothing would ever dispose, in no document.
///
/// Every inner build counts itself and registers a cleanup on its own ambient
/// owner, so `builds` and `teardowns` are the creation/disposal pair the item
/// asks to be equal.
const NESTED_SWAP_ON_ONE_SOURCE: &str = r#"import std::io::print;
import std::reactive::{ Disposable, FlushPolicy, Signal, SignalCell, Source, get_owner, turn };
import std::ui::{ View, mount_root, swap, view };

[derive(PartialEq)]
enum Shell {
	Login,
	App,
}

let route: SignalCell<str> = Signal::new("/app/one");
let builds: SignalCell<i32> = Signal::new(0);
let teardowns: SignalCell<i32> = Signal::new(0);

fun shell_of(path: str): Shell {
	if path.starts_with("/app") { Shell::App } else { Shell::Login }
}

fun page(path: str): View {
	builds.set_with(|count| count + 1);
	get_owner().defer(|| {
		teardowns.set_with(|count| count + 1);
	});
	view("p").text(path)
}

fun report(label: str) {
	print(i"{label} builds={builds.get()} teardowns={teardowns.get()} tree={shape()}");
}

fun main() {
	let _root = mount_root("app", || view("main").child(swap(route.map(shell_of), |shell: Shell| {
		match shell {
			Shell::Login => view("section").text("sign in"),
			Shell::App => view("div").child(swap(route, |path: str| page(path))),
		}
	})));
	report("mounted");

	// Sign out with NO ambient turn: `SignalCell::notify` takes its inline arm
	// and walks a snapshot of `route`'s subscriber list. The derivation runs
	// first, the outer effect disposes the App shell depth-first, and the inner
	// effect is still in the snapshot.
	route.set("/login");
	report("inline-signout");

	// Back in, then out again inside a TURN — the cadence every `View.on`
	// dispatch and `mount_root` establishes.
	route.set("/app/two");
	report("back-in");
	turn(FlushPolicy::AtSuspension, || {
		route.set("/login");
	});
	report("turn-signout");
}

/// The document as markup, so "it built into a closed region" is a fact about
/// the tree rather than a count.
[extern("__shape")]
external fun shape(): str;

main();
"#;

#[test]
fn a110_nested_swaps_on_one_source_build_no_orphan_subtree_on_sign_out() {
    let harness = format!(
        "{DOM_STUB}\nglobal.__shape = () => documentRoot.render();\nrequire(\"./app.js\");\n"
    );
    let stdout = build_and_run("a110_nested_swap", NESTED_SWAP_ON_ONE_SOURCE, &harness);
    let line = |prefix: &str| {
        stdout
            .lines()
            .find_map(|line| line.strip_prefix(prefix))
            .unwrap_or_else(|| panic!("the {prefix} line; got:\n{stdout}"))
            .trim()
            .to_string()
    };

    // The mount builds the App shell and one page under it.
    assert_eq!(
        line("mounted "),
        "builds=1 teardowns=0 tree=<root><main><div><p>/app/one</p></div></main></root>",
        "the mount must build the App shell and one page; got:\n{stdout}"
    );

    // The inline sign-out is the face door 1 closes. Before it, `builds` went
    // to 2 here — a page rendered for `/login` under an owner the outer shell's
    // disposal had already passed, into a region whose anchor was gone — and
    // `teardowns` stayed at 1, so the creation/disposal pair did not balance
    // and the orphan was in no document at all.
    assert_eq!(
        line("inline-signout "),
        "builds=1 teardowns=1 tree=<root><main><section>sign in</section></main></root>",
        "signing out inline must not build the inner page again, and every page \
         owner built must have been disposed; got:\n{stdout}"
    );

    assert_eq!(
        line("back-in "),
        "builds=2 teardowns=1 tree=<root><main><div><p>/app/two</p></div></main></root>",
        "navigating back in must build exactly one page; got:\n{stdout}"
    );

    // In a TURN the inner page is not built at all, and THAT is door 2. Under
    // door 1 alone this read `builds=3 teardowns=3`: the wave ran in
    // subscription order, the outer reached `route` one derivation later than
    // the inner, so the inner form rendered a page for `/login` and the outer
    // tore it down in the next wave — a wasted build door 1 could only make
    // safe. Door 2 runs the derivation in the wave's FIRST phase, so the outer
    // effect is in the queue before phase 2 starts and its id is the lower one
    // (a parent form's effect is created when the parent is PLACED, its child's
    // when the parent's render runs), and the inner subscription is disposed
    // before its turn in the queue comes. The pair still balances, which is
    // door 1's claim, and there is one fewer of each.
    assert_eq!(
        line("turn-signout "),
        "builds=2 teardowns=2 tree=<root><main><section>sign in</section></main></root>",
        "signing out in a turn must build no page for the shell it is leaving, \
         and must leave no live page owner and no orphan subtree; got:\n{stdout}"
    );
}

// --- A105: `bind_attr` over a `Source<Option<str>>` -------------------------

/// `bind_attr` at both value types, on one element. The `str` arm is the
/// control — it must behave exactly as it always did — and the `Option` arm is
/// the new one: `Some` sets, `None` REMOVES. kolt hand-wrote this as
/// `bind_attr_proper` (styles.vl:36) because std had no form for it, and its
/// customer is `[data-dragging="row"] *`, a selector that reads PRESENCE, so
/// writing "" between drags is the wrong answer rather than a tidier one.
///
/// The last two lines are the boundary: the effect is the nearest boundary's,
/// like every binding's, so disposing the root stops both.
const BIND_ATTR_OPTION: &str = r#"import std::io::print;
import std::option::Option::{ self, None, Some };
import std::reactive::{ Disposable, Signal, SignalCell };
import std::ui::{ View, mount_root, view };

fun main() {
	let dragging: SignalCell<Option<str>> = Signal::new(None);
	let plain: SignalCell<str> = Signal::new("one");
	let root = mount_root("app", || {
		view("div")
			.bind_attr("data-dragging", dragging)
			.bind_attr("x-plain", plain)
			.child(view("p").text("shell"))
	});
	print(i"initial={shell_attributes()}");
	dragging.set(Some("row"));
	print(i"dragging-row={shell_attributes()}");
	dragging.set(Some("col"));
	print(i"dragging-col={shell_attributes()}");
	dragging.set(None);
	print(i"released={shell_attributes()}");

	// The empty string is a VALUE, not an absence — the distinction the
	// `Option` bound exists to make.
	dragging.set(Some(""));
	print(i"empty-string={shell_attributes()}");
	dragging.set(None);
	print(i"released-again={shell_attributes()}");

	// The `str` arm, unchanged.
	plain.set("two");
	print(i"plain={shell_attributes()}");

	root.dispose();
	dragging.set(Some("row"));
	plain.set("three");
	print(i"disposed={shell_attributes()}");
}

[extern("__shell_attributes")]
external fun shell_attributes(): str;

main();
"#;

#[test]
fn a105_bind_attr_over_an_option_source_sets_and_removes_the_attribute() {
    let harness = format!(
        "{DOM_STUB}\nglobal.__shell_attributes = () => {{\n  \
         const shell = documentRoot.children[0];\n  \
         return JSON.stringify(shell.attributes);\n\
         }};\nrequire(\"./app.js\");\n"
    );
    let stdout = build_and_run("bind_attr_option", BIND_ATTR_OPTION, &harness);
    let line = |prefix: &str| {
        stdout
            .lines()
            .find_map(|line| line.strip_prefix(prefix))
            .unwrap_or_else(|| panic!("the {prefix} line; got:\n{stdout}"))
            .to_string()
    };
    // A `None` at mount writes nothing at all — not `""`, not `"None"`.
    assert_eq!(
        line("initial="),
        "{\"x-plain\":\"one\"}",
        "a `None` must leave the attribute off entirely; got:\n{stdout}"
    );
    assert_eq!(
        line("dragging-row="),
        "{\"x-plain\":\"one\",\"data-dragging\":\"row\"}",
        "a `Some` must set the attribute to its text; got:\n{stdout}"
    );
    assert_eq!(
        line("dragging-col="),
        "{\"x-plain\":\"one\",\"data-dragging\":\"col\"}",
        "a second `Some` must replace the value; got:\n{stdout}"
    );
    assert_eq!(
        line("released="),
        "{\"x-plain\":\"one\"}",
        "a `None` must REMOVE the attribute, not write a value; got:\n{stdout}"
    );
    // The whole reason the bound is `Option<str>` and not `str`.
    assert_eq!(
        line("empty-string="),
        "{\"x-plain\":\"one\",\"data-dragging\":\"\"}",
        "`Some(\"\")` must write a present, empty attribute; got:\n{stdout}"
    );
    assert_eq!(
        line("released-again="),
        "{\"x-plain\":\"one\"}",
        "and the next `None` must remove it again; got:\n{stdout}"
    );
    assert_eq!(
        line("plain="),
        "{\"x-plain\":\"two\"}",
        "the `str` arm must still track; got:\n{stdout}"
    );
    assert_eq!(
        line("disposed="),
        "{\"x-plain\":\"two\"}",
        "both bindings are the boundary's, so disposing the root must stop \
         them; got:\n{stdout}"
    );
}

// --- A115: element syntax admits an `Option` attribute ----------------------
//
// `bind_attr` (A105) took `Source<Option<str>>` and `AttrValue` — the trait
// element syntax desugars THROUGH (`.attr`, never `.bind_attr`) — had arms for
// `str` and `Source<str>` only, so `<div data-dragging(maybe)>` was refused at
// the bound while the method form of the same binding was fine. Two arms close
// it on both twins: a static `Option<str>` writes nothing for a `None`, and a
// `Source<Option<str>>` tracks with a `None` REMOVING the attribute, which is
// `AttrBinding`'s own arm reached from the sugar.

/// A STATIC `Option<str>` through element syntax, both ways at once: the `Some`
/// attribute is written and the `None` one is not there at all.
const ELEMENT_OPTION_ATTRIBUTE: &str = r#"import std::io::print;
import std::option::Option::{ self, None, Some };
import std::ui::{ View, mount_root, view };

fun main() {
	let held: Option<str> = Some("row");
	let absent: Option<str> = None;
	let blank: Option<str> = Some("");
	mount_root("app", || {
		<div data-held(held) data-absent(absent) data-blank(blank) id("shell")></div>
	});
	print(i"attributes={shell_attributes()}");
}

[extern("__shell_attributes")]
external fun shell_attributes(): str;

main();
"#;

#[test]
fn a115_element_syntax_takes_a_static_option_attribute() {
    let harness = format!(
        "{DOM_STUB}\nglobal.__shell_attributes = () => {{\n  \
         const shell = documentRoot.children[0];\n  \
         return JSON.stringify(shell.attributes);\n\
         }};\nrequire(\"./app.js\");\n"
    );
    let stdout = build_and_run(
        "element_option_attribute",
        ELEMENT_OPTION_ATTRIBUTE,
        &harness,
    );
    let attributes = stdout
        .lines()
        .find_map(|line| line.strip_prefix("attributes="))
        .unwrap_or_else(|| panic!("the attributes line; got:\n{stdout}"));
    assert_eq!(
        attributes, "{\"data-held\":\"row\",\"data-blank\":\"\",\"id\":\"shell\"}",
        "a `Some` writes its text, `Some(\"\")` a present empty attribute, and a \
         `None` nothing at all; got:\n{stdout}"
    );
}

/// A REACTIVE `Source<Option<str>>` through element syntax — the arm whose
/// customer is a presence selector, where no string turns the attribute off.
const ELEMENT_OPTION_SOURCE_ATTRIBUTE: &str = r#"import std::io::print;
import std::option::Option::{ self, None, Some };
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, mount_root, view };

fun main() {
	let dragging: SignalCell<Option<str>> = Signal::new(None);
	let root = mount_root("app", || <div data-dragging(dragging) id("shell")></div>);
	print(i"initial={shell_attributes()}");
	dragging.set(Some("row"));
	print(i"dragging-row={shell_attributes()}");
	dragging.set(Some(""));
	print(i"empty-string={shell_attributes()}");
	dragging.set(None);
	print(i"released={shell_attributes()}");
	root.dispose();
	dragging.set(Some("row"));
	print(i"disposed={shell_attributes()}");
}

[extern("__shell_attributes")]
external fun shell_attributes(): str;

main();
"#;

#[test]
fn a115_element_syntax_tracks_an_option_source_attribute() {
    let harness = format!(
        "{DOM_STUB}\nglobal.__shell_attributes = () => {{\n  \
         const shell = documentRoot.children[0];\n  \
         return JSON.stringify(shell.attributes);\n\
         }};\nrequire(\"./app.js\");\n"
    );
    let stdout = build_and_run(
        "element_option_source_attribute",
        ELEMENT_OPTION_SOURCE_ATTRIBUTE,
        &harness,
    );
    let line = |prefix: &str| {
        stdout
            .lines()
            .find_map(|line| line.strip_prefix(prefix))
            .unwrap_or_else(|| panic!("the {prefix} line; got:\n{stdout}"))
            .to_string()
    };
    assert_eq!(
        line("initial="),
        "{\"id\":\"shell\"}",
        "a `None` at mount writes nothing; got:\n{stdout}"
    );
    assert_eq!(
        line("dragging-row="),
        "{\"id\":\"shell\",\"data-dragging\":\"row\"}",
        "a `Some` sets the attribute; got:\n{stdout}"
    );
    assert_eq!(
        line("empty-string="),
        "{\"id\":\"shell\",\"data-dragging\":\"\"}",
        "`Some(\"\")` is a present, empty attribute; got:\n{stdout}"
    );
    assert_eq!(
        line("released="),
        "{\"id\":\"shell\"}",
        "a `None` REMOVES it; got:\n{stdout}"
    );
    assert_eq!(
        line("disposed="),
        "{\"id\":\"shell\"}",
        "the binding is the boundary's, so disposal stops it; got:\n{stdout}"
    );
}

/// The SSR twin of both arms, from the sugar: a `Some` renders, a `None` is
/// omitted, and `Some("")` stays a present empty attribute.
const ELEMENT_OPTION_ATTRIBUTE_SSR: &str = r#"import std::io::print;
import std::option::Option::{ self, None, Some };
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, render, view };

fun main() {
	let held: Option<str> = Some("row");
	let absent: Option<str> = None;
	let live: SignalCell<Option<str>> = Signal::new(Some("cell"));
	let gone: SignalCell<Option<str>> = Signal::new(None);
	print(render(<div data-held(held) id("shell")></div>));
	print(render(<section data-absent(absent)></section>));
	print(render(<article data-live(live)></article>));
	print(render(<aside data-gone(gone)></aside>));
}

main();
"#;

#[test]
fn a115_the_ssr_twin_renders_an_option_attribute_from_element_syntax() {
    let stdout =
        build_and_run_process("element_option_attribute_ssr", ELEMENT_OPTION_ATTRIBUTE_SSR);
    assert_eq!(
        stdout,
        "<div data-held=\"row\" id=\"shell\"></div>\n<section></section>\n\
         <article data-live=\"cell\"></article>\n<aside></aside>\n",
        "the server render carries a `Some` from either arm and omits a `None`"
    );
}

/// The SSR twin: read once, and a `None` renders nothing. There is no
/// `remove_attribute` to mirror, because nothing was written — `toggle_attr`'s
/// arrangement exactly.
const BIND_ATTR_OPTION_SSR: &str = r#"import std::io::print;
import std::option::Option::{ self, None, Some };
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, render, view };

fun main() {
	let held: SignalCell<Option<str>> = Signal::new(Some("row"));
	let absent: SignalCell<Option<str>> = Signal::new(None);
	let blank: SignalCell<Option<str>> = Signal::new(Some(""));
	print(render(view("div").bind_attr("data-dragging", held).attr("id", "shell")));
	print(render(view("section").bind_attr("data-dragging", absent)));
	print(render(view("aside").bind_attr("data-dragging", blank)));
}

main();
"#;

#[test]
fn a105_the_ssr_twin_renders_a_some_attribute_and_omits_a_none() {
    let stdout = build_and_run_process("bind_attr_option_ssr", BIND_ATTR_OPTION_SSR);
    assert_eq!(
        stdout,
        "<div data-dragging=\"row\" id=\"shell\"></div>\n<section></section>\n<aside data-dragging=\"\"></aside>\n",
        "the server render must carry a `Some` attribute in insertion order, \
         omit a `None` entirely, and keep `Some(\"\")` as a present empty one"
    );
}

// --- A119: `when_some` — the conditional form that BINDS the value ----------
//
// `when` takes a `Source<bool>` and `swap` rebuilds on every distinct value, so
// "show this while the source is `Some`, with the payload in hand" had no std
// spelling and kolt wrote its own (`lib/conditional_value.vl`). R6 at Order
// 39's GO: the row is KEPT across a payload change and the body is handed a
// derived cell, which is `each_by`'s row cell applied to one row.
//
// The three transitions are one program, because the claim is about the
// sequence: the identity of the `<p>` across a `Some -> Some` is the whole
// ruling, and only a run that built it first can say whether it survived.

const WHEN_SOME: &str = r#"import std::io::print;
import std::option::Option::{ self, None, Some };
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, mount_root, view, when_some };

struct Account {
	id: i32,
	name: str,
}

fun main() {
	let selected: SignalCell<Option<Account>> = Signal::new(None);
	let root = mount_root("app", || {
		view("div")
			.child(view("h1").text("head"))
			.child(when_some(selected, |account| {
				print("build");
				view("p").bind_text(account.map(|current| current.name))
			}))
			.child(view("footer").text("foot"))
	});
	print(i"none={tree()}");
	selected.set(Some(Account { id = 1, name = "Ada" }));
	print(i"some={tree()}");

	// `Some -> Some` with a CHANGED payload: the row must be the same node,
	// carrying the new text. A rebuild would print `build` again and mint a new
	// identity — which is exactly what `swap` would have done.
	selected.set(Some(Account { id = 2, name = "Grace" }));
	print(i"changed={tree()}");

	selected.set(None);
	print(i"cleared={tree()}");

	// And back again, to prove the removal left the region able to build.
	selected.set(Some(Account { id = 3, name = "Ida" }));
	print(i"again={tree()}");

	root.dispose();
	selected.set(Some(Account { id = 4, name = "Nobody" }));
	print(i"disposed={tree()}");
}

[extern("__tree")]
external fun tree(): str;

main();
"#;

/// B369: a user-written generic `Slot` impl built by a generic function with
/// NO declared return type. The claim is about a live tree, which is why it is
/// here and not an inference pin: before the fix the call typed as `Holder<C>`,
/// `place` resolved to `Slot`'s bodyless requirement, and the build died with
/// an `internal:` error anchored in std's `open_row_before`. The pin is that
/// the `<span>` the body builds is actually PLACED, between its two static
/// siblings.
const B369_INFERRED_CONSTRUCTOR: &str = r#"import std::io::print;
import std::ui::{ Region, Slot, View, mount_root, view };

struct Holder<C: Slot> {
	body: || C,
}

impl Holder<type C: Slot> with Slot {
	fun place(self, parent: View) {
		let region = Region::open(parent);
		let _row = region.open_row((self.body)());
	}
}

fun hold<C: Slot>(body: || C) {
	Holder { body }
}

fun main() {
	let _root = mount_root("app", || {
		view("main")
			.child(view("header").text("H"))
			.child(hold(|| view("span").text("x")))
			.child(view("footer").text("F"))
	});
	print(i"placed={tree()}");
}

[extern("__tree")]
external fun tree(): str;

main();
"#;

#[test]
fn a119_when_some_keeps_its_row_across_a_payload_change() {
    let harness = format!(
        "{DOM_STUB}\nglobal.__tree = () => flatten(documentRoot);\nrequire(\"./app.js\");\n"
    );
    let stdout = build_and_run("when_some", WHEN_SOME, &harness);
    let line = |prefix: &str| {
        stdout
            .lines()
            .find_map(|line| line.strip_prefix(prefix))
            .unwrap_or_else(|| panic!("the {prefix} line; got:\n{stdout}"))
            .to_string()
    };
    // A `None` builds nothing, and the static siblings keep their places
    // around the anchor the region planted (A71).
    let none = line("none=");
    assert!(
        !none.contains(" p#"),
        "a `None` must build no row; got:\n{stdout}"
    );
    assert!(
        none.contains("h1#") && none.contains("footer#"),
        "the static siblings must be there; got:\n{stdout}"
    );

    let some = line("some=");
    let row_identity = some
        .split_whitespace()
        .find(|token| token.starts_with("p#"))
        .unwrap_or_else(|| panic!("a `Some` must build the row; got:\n{stdout}"))
        .to_string();
    assert!(
        row_identity.contains("'Ada'"),
        "the body must see the payload; got:\n{stdout}"
    );

    // THE RULING: the row survives, so its identity is unchanged and only the
    // text moved.
    let changed = line("changed=");
    let changed_identity = changed
        .split_whitespace()
        .find(|token| token.starts_with("p#"))
        .unwrap_or_else(|| panic!("the row must still be there; got:\n{stdout}"))
        .to_string();
    assert_eq!(
        changed_identity.split('\'').next(),
        row_identity.split('\'').next(),
        "a changed payload must KEEP the row (same node), not rebuild it; \
         got:\n{stdout}"
    );
    assert!(
        changed_identity.contains("'Grace'"),
        "and the cell must carry the new value into the binding; got:\n{stdout}"
    );
    assert_eq!(
        stdout.matches("build\n").count(),
        2,
        "the body must run once per INSTANTIATION — twice here (the first \
         `Some` and the one after the `None`), never for a payload change; \
         got:\n{stdout}"
    );

    assert!(
        !line("cleared=").contains(" p#"),
        "a `None` must remove the row; got:\n{stdout}"
    );
    assert!(
        line("again=").contains("'Ida'"),
        "and the next `Some` must build a fresh one; got:\n{stdout}"
    );
    // Disposal is the enclosing boundary's, and it takes everything the form
    // registered: the live instantiation's owner, the row, and the region's
    // anchor — so the static siblings close up around where it stood, exactly
    // as `when`'s do. The `build` count above is the other half of the claim:
    // it is still 2 after this write, so the subscription is gone.
    let disposed = line("disposed=");
    assert!(
        !disposed.contains(" p#") && !disposed.contains("#text#"),
        "disposing the root must take the row AND the region's anchor; \
         got:\n{stdout}"
    );
}

/// The SSR twin: read once. A `Some` renders the body with its cell holding the
/// current payload; a `None` renders nothing at all.
const WHEN_SOME_SSR: &str = r#"import std::io::print;
import std::option::Option::{ self, None, Some };
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, render, view, when_some };

struct Account {
	id: i32,
	name: str,
}

fun main() {
	let held: SignalCell<Option<Account>> = Signal::new(Some(Account { id = 1, name = "Ada" }));
	let absent: SignalCell<Option<Account>> = Signal::new(None);
	print(render(view("aside").child(when_some(held, |account| {
		view("p").bind_text(account.map(|current| current.name))
	}))));
	print(render(view("aside").child(when_some(absent, |account| {
		view("p").bind_text(account.map(|current| current.name))
	}))));
}

main();
"#;

#[test]
fn a119_the_ssr_twin_renders_a_some_body_and_omits_a_none() {
    let stdout = build_and_run_process("when_some_ssr", WHEN_SOME_SSR);
    assert_eq!(
        stdout, "<aside><p>Ada</p></aside>\n<aside></aside>\n",
        "the server render must carry the `Some` body with its payload and \
         render nothing for a `None`"
    );
}

#[test]
fn b369_an_inferred_generic_constructor_places_its_slot_in_the_live_tree() {
    let harness = format!("{DOM_STUB}{POSITION_HARNESS_TAIL}");
    let stdout = build_and_run(
        "b369_inferred_constructor",
        B369_INFERRED_CONSTRUCTOR,
        &harness,
    );
    assert_eq!(
        readouts(&stdout),
        vec![(
            "placed".to_string(),
            vec![
                "root".to_string(),
                "main".to_string(),
                "header'H'".to_string(),
                "span'x'".to_string(),
                "footer'F'".to_string(),
            ],
        )],
        "the body the un-annotated generic constructor holds must be placed at \
         its own hole, between the two static siblings; got:\n{stdout}"
    );
}

// --- A121: the DOM reads a focus scope is written on -------------------------
//
// `proposal/focus-scope.md` §7 named the reads `std::dom` did not have, and §8
// named what the test harness would need before any of them could be pinned.
// Both landed together, and this is the pin that the HARNESS half is not
// silently load-bearing: every addition is asserted here through a program
// that reads it, so a stub that stopped answering reds here rather than
// leaving a focus walk quietly finding nothing.

/// Every new read at once, over the tree that separates them: a natively
/// focusable tag beside a bare one (the per-tag `tabIndex` default), a written
/// `tabindex`, a boolean attribute absent then present, an INHERITED
/// `visibility: hidden`, the parent link and its null at the root,
/// `document.activeElement` before and after a focus, and the `focusin` that
/// focus dispatches — carrying `related_target`, where focus came FROM.
const FOCUS_READS: &str = r#"import std::dom::{ active_element, create_element, get_element_by_id, is_null, window };
import std::io::print;

fun main() {
	let root = get_element_by_id("app");
	let native = create_element("input");
	native.set_attribute("tabindex", "3");
	root.append(native);
	let plain = create_element("div");
	plain.set_attribute("tabindex", "7");
	root.append(plain);
	let inner = create_element("button");
	plain.append(inner);
	let shy = create_element("section");
	root.append(shy);
	let shy_child = create_element("input");
	shy.append(shy_child);
	shy.set_style_property("visibility", "hidden");

	print(i"tabindex bare={shy.tab_index()} native={inner.tab_index()} written={plain.tab_index()}");
	print(i"parent of_inner={inner.parent().tab_index()} root_has_none={is_null(root.parent())}");
	print(i"attribute before={plain.has_attribute("hidden")}");
	plain.set_attribute("hidden", "");
	print(i"attribute after={plain.has_attribute("hidden")}");
	print(i"visibility own={shy.computed_style("visibility")} inherited={shy_child.computed_style("visibility")} elsewhere={native.computed_style("visibility")}");
	print(i"active before={is_null(active_element())}");
	native.focus();
	print(i"active after={active_element().tab_index()}");
	let _heard = window().listen_capture("focusin", |event| {
		print(i"focusin at={event.target().tab_index()} from={event.related_target().tab_index()}");
	});
	let _left = window().listen_capture("focusout", |event| {
		let to = event.related_target();
		let named = if is_null(to) { "null" } else { i"{to.tab_index()}" };
		print(i"focusout at={event.target().tab_index()} to={named} active_null={is_null(active_element())}");
	});
	inner.focus();
	print(i"descendants={root.query_selector_all("*").len()}");
}

main();
"#;

#[test]
fn a121_the_dom_reads_a_focus_scope_needs_answer_off_the_host() {
    // The `blur()` after the program is A128's stub addition: focus leaving the
    // document, which the host reports as a `focusout` whose `relatedTarget` is
    // null.
    let harness = format!("{DOM_STUB}\nrequire(\"./app.js\");\nactiveElement.blur();\n");
    let stdout = build_and_run("a121_reads", FOCUS_READS, &harness);
    let line = |key: &str| -> String {
        stdout
            .lines()
            .find(|line| line.starts_with(key))
            .unwrap_or_else(|| panic!("no {key:?} line in:\n{stdout}"))
            .to_string()
    };
    assert_eq!(
        line("tabindex "),
        "tabindex bare=-1 native=0 written=7",
        "`tab_index` is the attribute reflected with a PER-TAG default: -1 for \
         a tag that is not focusable, 0 for one that is, the written value \
         when there is one; got:\n{stdout}"
    );
    assert_eq!(
        line("parent "),
        "parent of_inner=7 root_has_none=true",
        "`parent` is the parent ELEMENT, and it is null at the root — which is \
         where an ancestor walk terminates; got:\n{stdout}"
    );
    assert_eq!(
        line("attribute before="),
        "attribute before=false",
        "`has_attribute` reads presence, and the attribute is not there yet; \
         got:\n{stdout}"
    );
    assert_eq!(
        line("attribute after="),
        "attribute after=true",
        "`has_attribute` reads presence — the write is what it sees; \
         got:\n{stdout}"
    );
    assert_eq!(
        line("visibility "),
        "visibility own=hidden inherited=hidden elsewhere=visible",
        "`computed_style` is the RESOLVED value, so `visibility: hidden` \
         answers for the subtree that inherits it and for nothing else — the \
         one predicate a 0x0 measurement cannot make, since a hidden element \
         keeps its box; got:\n{stdout}"
    );
    assert_eq!(
        line("active before="),
        "active before=true",
        "`active_element` is null while nothing has focus; got:\n{stdout}"
    );
    assert_eq!(
        line("active after="),
        "active after=3",
        "`active_element` names the element that took focus — the read \
         `matches(\":focus\")` cannot make about a element you do not hold; \
         got:\n{stdout}"
    );
    assert_eq!(
        line("focusin "),
        "focusin at=0 from=3",
        "a focus that takes DISPATCHES `focusin`, in the capture phase a \
         containment guard listens in, and `related_target` is where focus \
         came FROM; got:\n{stdout}"
    );
    let leaves: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("focusout "))
        .collect();
    assert_eq!(
        leaves,
        [
            "focusout at=3 to=0 active_null=true",
            "focusout at=0 to=null active_null=true"
        ],
        "A128: a focus that takes dispatches `focusout` on the OLD holder \
         first — while `active_element` is the body, as in the platform — and \
         its `related_target` is where focus WENT; a `blur()` with nowhere to \
         go dispatches one whose `related_target` is NULL, which is focus \
         leaving the document; got:\n{stdout}"
    );
    assert_eq!(
        line("descendants="),
        "descendants=5",
        "`query_selector_all(\"*\")` is every DESCENDANT element in document \
         order — the walk's enumeration, with the predicates left per-element; \
         got:\n{stdout}"
    );
}

// --- A121 S1: the tabbable walk, and a scope that traps without `inert` ------

/// The walk, with one element per reason it can be dropped and one per
/// position in the order. The tree is built through `std::dom` directly: this
/// is a `std::dom` claim, and going through `std::ui` would only put a view
/// layer between the assertion and the thing asserted.
const TABBABLE_WALK: &str = r#"import std::dom::{ create_element, get_element_by_id };
import std::io::print;

fun main() {
	let root = get_element_by_id("app");
	let panel = create_element("div");
	root.append(panel);

	let late = create_element("input");
	late.set_attribute("tabindex", "3");
	panel.append(late);
	let natural = create_element("input");
	panel.append(natural);
	let opted_out = create_element("input");
	opted_out.set_attribute("tabindex", "-1");
	panel.append(opted_out);
	let off = create_element("input");
	off.set_attribute("disabled", "");
	panel.append(off);
	let veiled = create_element("section");
	veiled.set_attribute("hidden", "");
	panel.append(veiled);
	let under_veiled = create_element("input");
	veiled.append(under_veiled);
	let invisible = create_element("section");
	invisible.set_style_property("visibility", "hidden");
	panel.append(invisible);
	let under_invisible = create_element("input");
	invisible.append(under_invisible);
	let unlaid = create_element("textarea");
	panel.append(unlaid);
	let early = create_element("input");
	early.set_attribute("tabindex", "1");
	panel.append(early);

	mut order = "";
	for element in panel.tabbable() {
		order = order + i"{element.tab_index()},";
	}
	print(i"descendants={panel.query_selector_all("*").len()} order={order}");
	print(i"kept late={late.is_tabbable()} natural={natural.is_tabbable()} early={early.is_tabbable()}");
	print(i"dropped opted_out={opted_out.is_tabbable()} disabled={off.is_tabbable()} veiled={under_veiled.is_tabbable()} invisible={under_invisible.is_tabbable()} unlaid={unlaid.is_tabbable()}");

	let empty = create_element("div");
	root.append(empty);
	print(i"fallback before={empty.has_attribute("tabindex")} took={empty.focus_first()} after={empty.has_attribute("tabindex")}");
	print(i"first took={panel.focus_first()} landed={early.matches(":focus")}");
}

main();
"#;

/// The boxes every focus pin needs — the walk's last predicate is a 0x0
/// measurement, and the stub has no layout engine, so a tag with no entry
/// here measures 0x0 and is NOT tabbable, which is what `textarea` is doing
/// in the tree above.
const FOCUS_STUB_EXTRAS: &str = r##"
global.boxes = {
    input: { left: 0, top: 0, width: 100, height: 20 },
    button: { left: 0, top: 0, width: 60, height: 20 },
    div: { left: 0, top: 0, width: 200, height: 40 },
    section: { left: 0, top: 0, width: 200, height: 40 },
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
global.at = () => (global.activeElement ? global.activeElement.attributes.name : "none");
"##;

#[test]
fn a121_the_tabbable_walk_implements_the_selectors_definition() {
    let harness = format!("{DOM_STUB}{FOCUS_STUB_EXTRAS}\nrequire(\"./app.js\");\n");
    let stdout = build_and_run("a121_walk", TABBABLE_WALK, &harness);
    let line = |key: &str| -> String {
        stdout
            .lines()
            .find(|line| line.starts_with(key))
            .unwrap_or_else(|| panic!("no {key:?} line in:\n{stdout}"))
            .to_string()
    };
    assert_eq!(
        line("descendants="),
        "descendants=10 order=1,3,0,",
        "Tab visits POSITIVE `tabindex` values first in ascending order, then \
         everything else in document order — the HTML specification's \
         sequential focus order restricted to a subtree, which is why \
         `tabbable` answers a List and not `querySelectorAll`'s order; \
         got:\n{stdout}"
    );
    assert_eq!(
        line("kept "),
        "kept late=true natural=true early=true",
        "a written `tabindex` and a natively focusable tag are both tabbable; \
         got:\n{stdout}"
    );
    assert_eq!(
        line("dropped "),
        "dropped opted_out=false disabled=false veiled=false invisible=false \
         unlaid=false",
        "each of the five reasons drops its element: a negative `tabindex` \
         (the author's opt-out), `disabled`, a `hidden` ANCESTOR, an INHERITED \
         `visibility: hidden` (which keeps its box, so no measurement sees \
         it), and a 0x0 box; got:\n{stdout}"
    );
    assert_eq!(
        line("fallback "),
        "fallback before=false took=true after=true",
        "a panel with nothing tabbable inside it takes focus ITSELF, and only \
         because `focus_first` writes `tabindex=\"-1\"` first: a bare `<div>` \
         is not a focusable area, so `focus()` on one does nothing at all — \
         which is what the hand-written fallback in every overlay has been \
         doing; got:\n{stdout}"
    );
    assert_eq!(
        line("first "),
        "first took=true landed=true",
        "`focus_first` focuses the first element in TAB order, not in document \
         order; got:\n{stdout}"
    );
}

/// A `Wrap` scope — a menu. The panel is a portal-shaped sibling of the rest
/// of the page, and the page stays live: a Tab pressed outside the scope is
/// not touched, which is A121's "without `inert`" stated as a test.
const WRAP_SCOPE: &str = r#"import std::dom::window;
import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ FocusContainment, View, focus_scope, mount_root, view, when };

fun main() {
	let open: SignalCell<bool> = Signal::new(false);
	let root = mount_root("app", || {
		view("div")
			.child(view("input").attr("name", "before"))
			.child(when(open, || {
				view("div")
					.attr("name", "panel")
					.attr("tabindex", "-1")
					.on_mount(|element| {
						let _scope = focus_scope(element, FocusContainment::Wrap);
						print("installed");
					})
					.child(view("input").attr("name", "first"))
					.child(view("input").attr("name", "middle"))
					.child(view("input").attr("name", "last"))
			}))
			.child(view("input").attr("name", "after"))
	});
	let _toggle = root.take(window().listen("toggle", |_event| {
		open.set(!open.get());
	}));
	print("built");
}

main();
"#;

#[test]
fn a121_wrap_cycles_at_the_ends_and_leaves_the_rest_of_the_page_alone() {
    let harness = format!(
        "{DOM_STUB}{FOCUS_STUB_EXTRAS}\nrequire(\"./app.js\");\n\
         window.fire(\"toggle\", {{}});\n\
         setTimeout(() => {{\n  \
         findByName(\"last\").focus();\n  \
         const tail = dispatchEvent(findByName(\"last\"), \"keydown\", {{ key: \"Tab\" }});\n  \
         console.log(\"forward=\" + at() + \" prevented=\" + !!tail.prevented);\n  \
         const head = dispatchEvent(activeElement, \"keydown\", {{ key: \"Tab\", shiftKey: true }});\n  \
         console.log(\"backward=\" + at() + \" prevented=\" + !!head.prevented);\n  \
         findByName(\"middle\").focus();\n  \
         const middle = dispatchEvent(findByName(\"middle\"), \"keydown\", {{ key: \"Tab\" }});\n  \
         console.log(\"middle=\" + at() + \" prevented=\" + !!middle.prevented);\n  \
         findByName(\"before\").focus();\n  \
         const outside = dispatchEvent(findByName(\"before\"), \"keydown\", {{ key: \"Tab\" }});\n  \
         console.log(\"outside=\" + at() + \" prevented=\" + !!outside.prevented);\n  \
         const other = dispatchEvent(findByName(\"first\"), \"keydown\", {{ key: \"Escape\" }});\n  \
         console.log(\"otherkey=\" + at() + \" prevented=\" + !!other.prevented);\n  \
         findByName(\"panel\").focus();\n  \
         const onRoot = dispatchEvent(findByName(\"panel\"), \"keydown\", {{ key: \"Tab\" }});\n  \
         console.log(\"fromRoot=\" + at() + \" prevented=\" + !!onRoot.prevented);\n  \
         findByName(\"panel\").focus();\n  \
         const backRoot = dispatchEvent(findByName(\"panel\"), \"keydown\", {{ key: \"Tab\", shiftKey: true }});\n  \
         console.log(\"backFromRoot=\" + at() + \" prevented=\" + !!backRoot.prevented);\n\
         }}, 0);\n"
    );
    let stdout = build_and_run("a121_wrap", WRAP_SCOPE, &harness);
    let line = |key: &str| -> String {
        stdout
            .lines()
            .find(|line| line.starts_with(key))
            .unwrap_or_else(|| panic!("no {key:?} line in:\n{stdout}"))
            .to_string()
    };
    assert_eq!(
        line("forward="),
        "forward=first prevented=true",
        "Tab from the LAST tabbable wraps to the first, and the default is \
         cancelled — otherwise the platform would move focus as well; \
         got:\n{stdout}"
    );
    assert_eq!(
        line("backward="),
        "backward=last prevented=true",
        "Shift+Tab from the FIRST wraps to the last; got:\n{stdout}"
    );
    assert_eq!(
        line("middle="),
        "middle=middle prevented=false",
        "between the ends the scope does NOTHING and the platform moves \
         focus — the stub has no sequential navigation of its own, so focus \
         staying put IS the untouched case; got:\n{stdout}"
    );
    assert_eq!(
        line("outside="),
        "outside=before prevented=false",
        "a Tab pressed OUTSIDE the scope is untouched: the page stays live, \
         which is what `Contain`/`Wrap` buy over `inert`; got:\n{stdout}"
    );
    assert_eq!(
        line("otherkey="),
        "otherkey=before prevented=false",
        "a key that is not Tab is not the scope's business; got:\n{stdout}"
    );
    assert_eq!(
        line("fromRoot="),
        "fromRoot=first prevented=true",
        "focus on the PANEL itself is inside the scope and on no tab stop — \
         which is where `focus_first`'s fallback leaves it — so Tab enters \
         the order at its start instead of leaving the scope; got:\n{stdout}"
    );
    assert_eq!(
        line("backFromRoot="),
        "backFromRoot=last prevented=true",
        "and Shift+Tab from the panel enters at the END; got:\n{stdout}"
    );
}

/// A `Contain` scope and a nested one, in the shape that makes the stack
/// necessary: both panels are SIBLINGS in the document (an overlay is a
/// portal — a submenu opened from inside a menu's body mounts into the
/// driver's container, not into its parent's panel), so
/// `menu.contains(submenu)` is false and a containment test written on DOM
/// ancestry would yank focus out of the submenu the moment it opened.
const CONTAIN_SCOPE: &str = r#"import std::dom::window;
import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ FocusContainment, View, focus_scope, mount_root, view, when };

fun main() {
	let menu: SignalCell<bool> = Signal::new(false);
	let submenu: SignalCell<bool> = Signal::new(false);
	let root = mount_root("app", || {
		view("div")
			.child(view("input").attr("name", "outside"))
			.child(when(menu, || {
				view("div")
					.attr("name", "menu")
					.on_mount(|element| {
						let _scope = focus_scope(element, FocusContainment::Contain);
					})
					.child(view("input").attr("name", "menu-item"))
			}))
			.child(when(submenu, || {
				view("div")
					.attr("name", "sub")
					.on_mount(|element| {
						let _scope = focus_scope(element, FocusContainment::Contain);
					})
					.child(view("input").attr("name", "sub-item"))
			}))
	});
	let _open_menu = root.take(window().listen("menu", |_event| {
		menu.set(!menu.get());
	}));
	let _open_sub = root.take(window().listen("sub", |_event| {
		submenu.set(!submenu.get());
	}));
	print("built");
}

main();
"#;

/// The steps, in a raw string and at one indentation: a nested `setTimeout`
/// ladder puts JS lines eight columns inward inside a Rust literal, which is
/// what N98's prose gate reads as a lost line continuation. One `await` per
/// wave says the same thing flat.
const CONTAIN_STEPS: &str = r##"
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
(async () => {
  window.fire("menu", {});
  await tick();
  console.log("siblings=" + findByName("menu").contains(findByName("outside")));
  findByName("outside").focus();
  console.log("pulled=" + at());
  window.fire("sub", {});
  await tick();
  console.log("nested=" + findByName("menu").contains(findByName("sub")));
  findByName("sub-item").focus();
  console.log("inSub=" + at());
  findByName("outside").focus();
  console.log("pulledBySub=" + at());
  window.fire("sub", {});
  await tick();
  console.log("subGone=" + !!findByName("sub"));
  findByName("outside").focus();
  console.log("handedBack=" + at());
  window.fire("menu", {});
  await tick();
  findByName("outside").focus();
  console.log("released=" + at());
})();
"##;

#[test]
fn a121_contain_pulls_focus_back_and_the_nested_scope_takes_over() {
    let harness = format!("{DOM_STUB}{FOCUS_STUB_EXTRAS}\nrequire(\"./app.js\");\n{CONTAIN_STEPS}");
    let stdout = build_and_run("a121_contain", CONTAIN_SCOPE, &harness);
    let line = |key: &str| -> String {
        stdout
            .lines()
            .find(|line| line.starts_with(key))
            .unwrap_or_else(|| panic!("no {key:?} line in:\n{stdout}"))
            .to_string()
    };
    assert_eq!(
        line("siblings="),
        "siblings=false",
        "the panel does not contain the page — the control for the assertions \
         below; got:\n{stdout}"
    );
    assert_eq!(
        line("pulled="),
        "pulled=menu-item",
        "focus arriving OUTSIDE a `Contain` scope is pulled back to the first \
         tabbable thing inside it; got:\n{stdout}"
    );
    assert_eq!(
        line("nested="),
        "nested=false",
        "the submenu is a SIBLING of the menu, not a descendant — which is \
         why the containment test cannot be DOM ancestry; got:\n{stdout}"
    );
    assert_eq!(
        line("inSub="),
        "inSub=sub-item",
        "focus inside the nested scope stays there: the menu's guard is no \
         longer topmost and returns immediately, where a per-scope guard \
         reading only its own root would yank it; got:\n{stdout}"
    );
    assert_eq!(
        line("pulledBySub="),
        "pulledBySub=sub-item",
        "while the submenu is open, IT is the scope that pulls focus back; \
         got:\n{stdout}"
    );
    assert_eq!(
        line("handedBack="),
        "handedBack=menu-item",
        "the submenu popped with its owner, so the menu is topmost again and \
         guards once more — the stack hands back; got:\n{stdout}"
    );
    assert_eq!(
        line("released="),
        "released=outside",
        "with every scope disposed nothing guards, and the page has its focus \
         back; got:\n{stdout}"
    );
}

/// The restore, and both of its guards. A scope remembers what held focus at
/// INSTALL and gives it back at disposal — unless the app moved focus
/// deliberately in the meantime, in which case the app's choice stands.
const RESTORE_SCOPE: &str = r#"import std::dom::window;
import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ FocusContainment, View, focus_scope, mount_root, view, when };

fun main() {
	let open: SignalCell<bool> = Signal::new(false);
	let root = mount_root("app", || {
		view("div")
			.child(view("input").attr("name", "opener"))
			.child(view("input").attr("name", "elsewhere"))
			.child(when(open, || {
				view("div")
					.attr("name", "panel")
					.on_mount(|element| {
						let _scope = focus_scope(element, FocusContainment::Wrap);
					})
					.child(view("input").attr("name", "inside"))
			}))
	});
	let _toggle = root.take(window().listen("toggle", |_event| {
		open.set(!open.get());
	}));
	print("built");
}

main();
"#;

/// The steps, flat for `CONTAIN_STEPS`' reason.
const RESTORE_STEPS: &str = r##"
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
(async () => {
  await tick();
  findByName("opener").focus();
  window.fire("toggle", {});
  await tick();
  findByName("inside").focus();
  console.log("taken=" + at());
  window.fire("toggle", {});
  await tick();
  console.log("restored=" + at());
  findByName("opener").focus();
  window.fire("toggle", {});
  await tick();
  findByName("elsewhere").focus();
  window.fire("toggle", {});
  await tick();
  console.log("kept=" + at());
})();
"##;

#[test]
fn a121_a_scope_restores_the_focus_it_took_and_not_the_focus_it_was_given() {
    let harness = format!("{DOM_STUB}{FOCUS_STUB_EXTRAS}\nrequire(\"./app.js\");\n{RESTORE_STEPS}");
    let stdout = build_and_run("a121_restore", RESTORE_SCOPE, &harness);
    let line = |key: &str| -> String {
        stdout
            .lines()
            .find(|line| line.starts_with(key))
            .unwrap_or_else(|| panic!("no {key:?} line in:\n{stdout}"))
            .to_string()
    };
    assert_eq!(
        line("taken="),
        "taken=inside",
        "the control: focus is inside the scope while it is open; \
         got:\n{stdout}"
    );
    assert_eq!(
        line("restored="),
        "restored=opener",
        "disposal gives focus back to whatever held it when the scope was \
         installed — otherwise focus falls to `<body>` and the keyboard user \
         is at the top of the page; got:\n{stdout}"
    );
    assert_eq!(
        line("kept="),
        "kept=elsewhere",
        "and it restores only if the focus is still OURS: an app that moved \
         focus deliberately while the overlay was open keeps it; \
         got:\n{stdout}"
    );
}

// --- A121 S2: the SHOW takes the focus, and `autofocus` says where ----------

/// The overlay's real shape: the scope is installed at mount, the panel is
/// hidden until something places it, and the DRIVER calls `focus_initial`
/// when it flips visibility — the `Shared<Option<FocusScope>>` here is the
/// per-open handle every driver already keeps beside its `focused` flag.
const SHOW_TAKES_FOCUS: &str = r#"import std::dom::window;
import std::io::print;
import std::option::Option::{ self, None, Some };
import std::reactive::{ Signal, SignalCell };
import std::shared::Shared;
import std::ui::{ FocusContainment, FocusScope, View, focus_scope, mount_root, view, when };

fun main() {
	let open: SignalCell<bool> = Signal::new(false);
	let held: Shared<Option<FocusScope>> = Shared::new(None);
	let root = mount_root("app", || {
		view("div")
			.child(view("input").attr("name", "outside"))
			.child(when(open, || {
				view("div")
					.attr("name", "panel")
					.on_mount(|element| {
						element.set_style_property("visibility", "hidden");
						held.write() = Some(focus_scope(element, FocusContainment::Wrap));
					})
					.child(view("input").attr("name", "first"))
					.child(view("input").attr("name", "marked").autofocus())
			}))
	});
	let _open = root.take(window().listen("open", |_event| {
		open.set(true);
	}));
	let _show = root.take(window().listen("show", |_event| {
		match held.read() {
			Some(let scope) => print(i"took={scope.focus_initial()}"),
			None => print("no scope"),
		}
	}));
	print("built");
}

main();
"#;

#[test]
fn a121_the_show_takes_the_focus_and_autofocus_says_where() {
    let harness = format!(
        "{DOM_STUB}{FOCUS_STUB_EXTRAS}\nrequire(\"./app.js\");\n\
         window.fire(\"open\", {{}});\n\
         setTimeout(() => {{\n  \
         console.log(\"markup=\" + JSON.stringify(findByName(\"marked\").attributes));\n  \
         window.fire(\"show\", {{}});\n  \
         console.log(\"early=\" + at());\n  \
         findByName(\"panel\").style.setProperty(\"visibility\", \"visible\");\n  \
         window.fire(\"show\", {{}});\n  \
         console.log(\"shown=\" + at());\n  \
         findByName(\"first\").focus();\n  \
         window.fire(\"show\", {{}});\n  \
         console.log(\"latched=\" + at());\n\
         }}, 0);\n"
    );
    let stdout = build_and_run("a121_show", SHOW_TAKES_FOCUS, &harness);
    let line = |key: &str| -> String {
        stdout
            .lines()
            .find(|line| line.starts_with(key))
            .unwrap_or_else(|| panic!("no {key:?} line in:\n{stdout}"))
            .to_string()
    };
    assert_eq!(
        line("markup="),
        "markup={\"name\":\"marked\",\"autofocus\":\"\"}",
        "`View::autofocus` WRITES the attribute now (A121 §6.1) — that is how \
         a scope, devtools and a markup assertion can see which element the \
         author chose. It moves emitted markup, which is why it is pinned; \
         got:\n{stdout}"
    );
    let takes: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("took="))
        .collect();
    assert_eq!(
        takes,
        vec!["took=false", "took=true", "took=true"],
        "a show that fires while the panel is still hidden answers FALSE and \
         focuses nothing (B271: the target must be rendered and visible at \
         the call), so the driver asks again on its next pass — and the \
         answer latches once it has taken; got:\n{stdout}"
    );
    assert_eq!(
        line("early="),
        "early=none",
        "nothing took focus while the panel was hidden; got:\n{stdout}"
    );
    assert_eq!(
        line("shown="),
        "shown=marked",
        "`focus_initial` prefers the `[autofocus]` descendant over the first \
         tabbable one — the author saying so beats the order; got:\n{stdout}"
    );
    assert_eq!(
        line("latched="),
        "latched=first",
        "it is IDEMPOTENT: a second show does not yank focus back off \
         whatever the user has since moved it to; got:\n{stdout}"
    );
}

/// The chained sugar: no show hook, no driver, no handle — the scope installs
/// itself and takes the focus on `autofocus`'s bounded clock.
const FOCUS_SCOPE_SUGAR: &str = r#"import std::dom::window;
import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ FocusContainment, View, mount_root, view, when };

fun main() {
	let open: SignalCell<bool> = Signal::new(false);
	let root = mount_root("app", || {
		view("div")
			.child(view("input").attr("name", "outside"))
			.child(when(open, || {
				view("div")
					.attr("name", "panel")
					.focus_scope(FocusContainment::Wrap)
					.child(view("input").attr("name", "first"))
					.child(view("input").attr("name", "last"))
			}))
	});
	let _open = root.take(window().listen("open", |_event| {
		open.set(true);
	}));
	print("built");
}

main();
"#;

#[test]
fn a121_the_chained_focus_scope_installs_the_trap_and_takes_the_focus() {
    let harness = format!(
        "{DOM_STUB}{FOCUS_STUB_EXTRAS}\nrequire(\"./app.js\");\n\
         window.fire(\"open\", {{}});\n\
         setTimeout(() => {{\n  \
         console.log(\"focused=\" + at());\n  \
         findByName(\"last\").focus();\n  \
         const tail = dispatchEvent(findByName(\"last\"), \"keydown\", {{ key: \"Tab\" }});\n  \
         console.log(\"trapped=\" + at() + \" prevented=\" + !!tail.prevented);\n\
         }}, 0);\n"
    );
    let stdout = build_and_run("a121_sugar", FOCUS_SCOPE_SUGAR, &harness);
    let line = |key: &str| -> String {
        stdout
            .lines()
            .find(|line| line.starts_with(key))
            .unwrap_or_else(|| panic!("no {key:?} line in:\n{stdout}"))
            .to_string()
    };
    assert_eq!(
        line("focused="),
        "focused=first",
        "the sugar takes the initial focus itself, on `autofocus`'s clock — \
         the ordinary case, the one with no show hook of its own; \
         got:\n{stdout}"
    );
    assert_eq!(
        line("trapped="),
        "trapped=first prevented=true",
        "and it installed the TRAP as well: Tab from the last tabbable wraps; \
         got:\n{stdout}"
    );
}

/// The SSR twins of both forms: accepted and dropped, like every event binder
/// there — except the `autofocus` ATTRIBUTE, which the browser twin now
/// writes and the server twin deliberately does not (process/ui.vl says why).
const FOCUS_SSR_TWINS: &str = r#"import std::io::print;
import std::ui::{ FocusContainment, View, render, view };

fun main() {
	print(render(view("div").focus_scope(FocusContainment::Contain).child(view("input"))));
	print(render(view("input").autofocus()));
}

main();
"#;

#[test]
fn a121_the_ssr_twins_render_the_same_markup_and_trap_nothing() {
    let stdout = build_and_run_process("a121_ssr", FOCUS_SSR_TWINS);
    assert_eq!(
        stdout, "<div><input></div>\n<input>\n",
        "a server render has no focus to trap: both forms serialize exactly \
         what they would without them, and the `autofocus` attribute stays a \
         client-side write (a served one would focus the element at the \
         browser's own initial parse, which is a behaviour change to every \
         server-rendered page that chains it)"
    );
}

// --- A128: `FocusScope::on_leave`, the `focusout` path (focus-scope.md §S3) ---
//
// RULED 2026-09-25: the handler fires from a capture-phase `focusout` whose
// `relatedTarget` lies outside the scope AND every scope above it, and a null
// `relatedTarget` (focus left the document) does nothing. Q2 stands: `Wrap`
// has no Tab-out arm, because under `Wrap` focus never leaves by Tab — which
// the first pin below holds.

/// Three scopes: a `Wrap` menu, a `Wrap` submenu that mounts BESIDE it (the
/// portal shape — the submenu is above the menu on the stack and not inside
/// it in the document), and a `Contain` modal opened on its own afterwards.
/// Every handler prints which scope heard the leave and whether `to` is the
/// element focus went to; the harness interleaves step markers, so the pin
/// reads one ordered transcript and silence is as visible as a fire.
const ON_LEAVE_SCOPES: &str = r#"import std::dom::window;
import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ FocusContainment, View, focus_scope, mount_root, view, when };

fun main() {
	let menu: SignalCell<bool> = Signal::new(false);
	let submenu: SignalCell<bool> = Signal::new(false);
	let modal: SignalCell<bool> = Signal::new(false);
	let root = mount_root("app", || {
		view("div")
			.child(view("input").attr("name", "outside").attr("data-outside", ""))
			.child(view("input").attr("name", "elsewhere"))
			.child(when(menu, || {
				view("div")
					.attr("name", "menu")
					.on_mount(|element| {
						let scope = focus_scope(element, FocusContainment::Wrap);
						scope.on_leave(|to| print(i"leave menu to_outside={to.has_attribute("data-outside")}"));
					})
					.child(view("input").attr("name", "menu-first"))
					.child(view("input").attr("name", "menu-last"))
			}))
			.child(when(submenu, || {
				view("div")
					.attr("name", "sub")
					.on_mount(|element| {
						let scope = focus_scope(element, FocusContainment::Wrap);
						scope.on_leave(|to| print(i"leave sub to_outside={to.has_attribute("data-outside")}"));
					})
					.child(view("input").attr("name", "sub-item"))
			}))
			.child(when(modal, || {
				view("div")
					.attr("name", "modal")
					.on_mount(|element| {
						let scope = focus_scope(element, FocusContainment::Contain);
						scope.on_leave(|to| print(i"leave modal to_outside={to.has_attribute("data-outside")}"));
					})
					.child(view("input").attr("name", "modal-first"))
					.child(view("input").attr("name", "modal-last"))
			}))
	});
	let _menu = root.take(window().listen("menu", |_event| {
		menu.set(!menu.get());
	}));
	let _sub = root.take(window().listen("sub", |_event| {
		submenu.set(!submenu.get());
	}));
	let _modal = root.take(window().listen("modal", |_event| {
		modal.set(!modal.get());
	}));
	print("built");
}

main();
"#;

/// The steps, flat for `CONTAIN_STEPS`' reason. Each `step` line marks what
/// the lines after it answer.
const ON_LEAVE_STEPS: &str = r##"
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
const step = (name) => console.log("step " + name);
(async () => {
  window.fire("menu", {});
  await tick();
  step("wrap-tab");
  findByName("menu-last").focus();
  const forward = dispatchEvent(findByName("menu-last"), "keydown", { key: "Tab" });
  const backward = dispatchEvent(activeElement, "keydown", { key: "Tab", shiftKey: true });
  console.log("wrapped=" + at() + " prevented=" + !!forward.prevented + "," + !!backward.prevented);
  step("within");
  findByName("menu-first").focus();
  step("into-scope-above");
  window.fire("sub", {});
  await tick();
  findByName("sub-item").focus();
  step("out-of-both");
  findByName("outside").focus();
  window.fire("sub", {});
  await tick();
  step("back-in");
  findByName("menu-first").focus();
  step("left-document");
  activeElement.blur();
  step("wrap-programmatic");
  findByName("menu-last").focus();
  findByName("outside").focus();
  step("page-to-page");
  findByName("elsewhere").focus();
  window.fire("menu", {});
  await tick();
  window.fire("modal", {});
  await tick();
  findByName("modal-last").focus();
  step("contain-tab");
  dispatchEvent(findByName("modal-last"), "keydown", { key: "Tab" });
  step("contain-click-outside");
  findByName("outside").focus();
  console.log("pulled=" + at());
  step("contain-left-document");
  activeElement.blur();
  step("end");
})();
"##;

#[test]
fn a128_on_leave_fires_where_focus_went_and_only_when_it_left_every_scope() {
    let harness =
        format!("{DOM_STUB}{FOCUS_STUB_EXTRAS}\nrequire(\"./app.js\");\n{ON_LEAVE_STEPS}");
    let stdout = build_and_run("a128_on_leave", ON_LEAVE_SCOPES, &harness);
    let transcript: Vec<&str> = stdout
        .lines()
        .filter(|line| {
            line.starts_with("step ")
                || line.starts_with("leave ")
                || line.starts_with("wrapped=")
                || line.starts_with("pulled=")
        })
        .collect();
    assert_eq!(
        transcript,
        [
            // Q2 CONFIRMED: under `Wrap` focus never leaves by Tab — the wrap
            // moves it from one end to the other, `focusout` names an element
            // inside the scope, and nothing fires.
            "step wrap-tab",
            "wrapped=menu-last prevented=true,true",
            // A move between two elements of one scope is not a leave.
            "step within",
            // Into the submenu: a SIBLING in the document, but ABOVE the menu
            // on the stack — the portal case, silent by the ruling's "every
            // scope above it".
            "step into-scope-above",
            // Out of the submenu to the page: it left the submenu AND the menu
            // beneath it (the element it left reaches from both), and each
            // handler hears where focus went. Registration order, outermost
            // first.
            "step out-of-both",
            "leave menu to_outside=true",
            "leave sub to_outside=true",
            // Focus arriving from outside is `focusin` business; the
            // `focusout` it causes is on an element no scope holds.
            "step back-in",
            // A NULL `relatedTarget` — focus left the document for the
            // browser's chrome — does nothing.
            "step left-document",
            // `Wrap` lets focus leave by any route but Tab, and that route is
            // a leave.
            "step wrap-programmatic",
            "leave menu to_outside=true",
            // Focus moving between two page elements while the menu is open
            // left nothing: the element it left was never the scope's.
            "step page-to-page",
            // `Contain`'s own wrap is silent as `Wrap`'s is.
            "step contain-tab",
            // A click outside a `Contain` scope: the leave fires WITH the
            // target, and then the guard pulls focus back — whose `focusout`
            // is on the outside element, not a leave.
            "step contain-click-outside",
            "leave modal to_outside=true",
            "pulled=modal-first",
            "step contain-left-document",
            "step end",
        ],
        "`FocusScope::on_leave` fires from a capture-phase `focusout` whose \
         `related_target` is outside the scope and every scope above it, with \
         that element; a null target is silent; got:\n{stdout}"
    );
}

// --- A112 S3: the delta-driven `each` ----------------------------------------

/// Eleven edits over one `ListCell` under `each`, each printed as
/// `pass|tree|cost`, then the two measurements the slice exists for: the same
/// push into a PLAIN `SignalCell` (the control, which keeps the pass) and one
/// push and one removal at 1,000 rows. `keys` counts calls of the key function,
/// which is what a pass spends on every row and the op path on none but the
/// row that arrived.
const A112_S3_OPS: &str = r#"import std::io::print;
import std::reactive::{ ListCell, SequenceCell, Signal, SignalCell, Source };
import std::shared::Shared;
import std::ui::{ View, each, mount_root, view };

[derive(PartialEq, Hashable)]
struct Row {
	id: i32,
	text: str,
}

fun row(id: i32, text: str): Row {
	Row { id = id, text = text }
}

let keys: Shared<i32> = Shared::new(0);

fun keyed(item: Row): i32 {
	keys.write() = keys.read() + 1;
	item.id
}

fun spent(): str {
	let line = i"{cost()} keys={keys.read()}";
	keys.write() = 0;
	line
}

fun main() {
	let rows: ListCell<Row> = ListCell<Row>::of([row(1, "a"), row(2, "b"), row(3, "c"), row(4, "d")]);
	let _root = mount_root("app", || {
		view("ul")
			.child(view("li").text("H"))
			.child(each(rows, keyed, |item: Row| view("li").text(item.text)))
			.child(view("li").text("F"))
	});
	print(i"mount|{tree()}|{spent()}");
	rows.push(row(5, "e"));
	print(i"push|{tree()}|{spent()}");
	rows.prepend(row(0, "z"));
	print(i"prepend|{tree()}|{spent()}");
	rows.remove_at(2);
	print(i"remove|{tree()}|{spent()}");
	rows.set_at(1, row(1, "A"));
	print(i"set_at|{tree()}|{spent()}");
	rows.set_at(1, row(1, "A"));
	print(i"set_at-same|{tree()}|{spent()}");
	rows.insert_all(2, [row(7, "g"), row(8, "h")]);
	print(i"insert_all|{tree()}|{spent()}");
	rows.edit(|&mut list| {
		list.push(row(9, "i"));
		list.remove_at(0);
		list.insert_at(1, row(6, "f"));
	});
	print(i"edit|{tree()}|{spent()}");
	rows.move_range(0, 1, 3);
	print(i"move|{tree()}|{spent()}");
	rows.set([row(1, "a"), row(2, "b")]);
	print(i"set|{tree()}|{spent()}");
	rows.clear();
	print(i"clear|{tree()}|{spent()}");

	// The control: a plain cell keeps the pass.
	let plain: SignalCell<List<Row>> = Signal::new([row(1, "a"), row(2, "b"), row(3, "c")]);
	let _second = mount_root("app", || {
		view("ol").child(each(plain, keyed, |item: Row| view("li").text(item.text)))
	});
	let _reset = spent();
	plain.update(|&mut list| { list.push(row(4, "d")); });
	print(i"plain-push||{spent()}");

	// At scale: one push into 1,000 rows, then one removal.
	mut many: List<Row> = [];
	mut index = 0;
	for index < 1000 {
		many.push(row(index, "r"));
		index += 1;
	}
	let big: ListCell<Row> = ListCell<Row>::of(many);
	let _third = mount_root("app", || {
		view("ol").child(each(big, keyed, |item: Row| view("li").text(item.text)))
	});
	let _mounted = spent();
	big.push(row(1000, "new"));
	print(i"big-push||{spent()}");
	big.remove_at(500);
	print(i"big-remove||{spent()}");
}

[extern("__tree")]
external fun tree(): str;

[extern("__cost")]
external fun cost(): str;

main();
"#;

/// A112 S3: a `ListCell` under `each` is followed by its OPS.
///
/// The claim is the cost column — one push into 1,000 rows builds ONE row and
/// calls the key function ONCE, a removal cuts one row and calls it never —
/// and the tree column is what says the cheaper path still built the right run
/// after every edit in the vocabulary: a splice at each end and in the middle,
/// an element changed in place (and "changed" to an equal value, which costs
/// nothing), a batch of three inside one `edit`, a `Move`, a wholesale `set`
/// (a `Reset`, which is the pass) and a `clear`. The plain `SignalCell` beside
/// it is the control: the same push costs a whole pass there, `keys=8` over
/// four rows.
///
/// Non-vacuity: with the op path planted out (`place_each` answering every
/// source with the pass, as it did before S3), the tree column is unchanged and
/// the cost column reds — `big-push` reads `keys=2002` and `push` `keys=10`.
#[test]
fn a112_s3_a_list_cell_under_each_builds_only_the_rows_its_ops_name() {
    let harness = format!("{DOM_STUB}{A98_COST_HARNESS_TAIL}");
    let stdout = build_and_run("a112_s3_ops", A112_S3_OPS, &harness);
    let seen: Vec<(String, Vec<String>, String)> = stdout
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('|');
            let name = parts.next()?.to_string();
            let tree = nodes(parts.next()?);
            let cost = parts.next()?.to_string();
            Some((name, tree, cost))
        })
        .collect();
    let tree = |labels: &[&str]| {
        let mut expected = vec!["root".to_string(), "ul".to_string(), "li'H'".to_string()];
        for label in labels {
            expected.push(format!("li'{label}'"));
        }
        expected.push("li'F'".to_string());
        expected
    };
    let none: Vec<String> = Vec::new();
    let expected: Vec<(String, Vec<String>, String)> = vec![
        // The first build is the pass: seven elements, four keys read twice
        // (the plan and the key list it keeps).
        ("mount", tree(&["a", "b", "c", "d"]), "cut=0 built=7 keys=8"),
        (
            "push",
            tree(&["a", "b", "c", "d", "e"]),
            "cut=0 built=1 keys=1",
        ),
        (
            "prepend",
            tree(&["z", "a", "b", "c", "d", "e"]),
            "cut=0 built=1 keys=1",
        ),
        (
            "remove",
            tree(&["z", "a", "c", "d", "e"]),
            "cut=1 built=0 keys=0",
        ),
        // Same key, different item: that row rebuilt, nothing else touched.
        (
            "set_at",
            tree(&["z", "A", "c", "d", "e"]),
            "cut=1 built=1 keys=2",
        ),
        // Same key, EQUAL item: a `Keep`, as the pass would have made it.
        (
            "set_at-same",
            tree(&["z", "A", "c", "d", "e"]),
            "cut=0 built=0 keys=1",
        ),
        (
            "insert_all",
            tree(&["z", "A", "g", "h", "c", "d", "e"]),
            "cut=0 built=2 keys=2",
        ),
        // Three ops, one notification: the push and the insert build a row
        // each, the removal cuts one.
        (
            "edit",
            tree(&["A", "f", "g", "h", "c", "d", "e", "i"]),
            "cut=1 built=2 keys=2",
        ),
        // A `Move` is the pass at the moved list: the one row that moved is cut
        // and put back (A98), nothing is built.
        (
            "move",
            tree(&["f", "g", "h", "A", "c", "d", "e", "i"]),
            "cut=1 built=0 keys=16",
        ),
        // A `Reset` is the pass: every row whose key left is cut.
        ("set", tree(&["a", "b"]), "cut=8 built=2 keys=4"),
        ("clear", tree(&[]), "cut=2 built=0 keys=0"),
        ("plain-push", none.clone(), "cut=0 built=1 keys=8"),
        ("big-push", none.clone(), "cut=0 built=1 keys=1"),
        ("big-remove", none, "cut=1 built=0 keys=0"),
    ]
    .into_iter()
    .map(|(name, tree, cost)| (name.to_string(), tree, cost.to_string()))
    .collect();
    assert_eq!(
        seen, expected,
        "A112 S3: a `ListCell` under `each` must build only the rows its ops \
         name and still show the right run; a plain `SignalCell` keeps the \
         pass; got:\n{stdout}"
    );
}

/// The S2 random walk (`vilan/test/list-cell.vl`, claim 10), extended THROUGH
/// `each`: 300 turns of 1–4 ops each, drawn from the whole vocabulary and made
/// inside one `batch` (so one drain applies several ops in order), with four
/// runs mounted on it — `each` over the cell, `each` over a `map_each` chained
/// on it, `each_values` over the cell, and (S3b) `each_by` over the cell, whose
/// rows show their text through their own cell. After every turn the program
/// reads the four runs back out of the document and panics on the first one
/// that differs from what the cell holds.
const A112_S3_WALK: &str = r#"import std::io::{ panic, print };
import std::reactive::{ ListCell, SequenceCell, Signal, SignalCell, Source, batch, map_each };
import std::shared::Shared;
import std::ui::{ View, each, each_by, each_values, mount_root, view };

[derive(PartialEq, Hashable)]
struct Row {
	id: i32,
	text: str,
}

/// A seeded MINSTD Lehmer sequence, as in `list-cell.vl`: deterministic, so the
/// printed tail is a golden.
let seed: Shared<i53> = Shared::new(7i53);

fun next_random(bound: i32): i32 {
	let state = (seed.read() * 16807i53) % 2147483647i53;
	seed.write() = state;
	(state % bound.as_i53()).as_i32()
}

/// Every row gets an id no other row has had, so the keys stay unique.
let ids: Shared<i32> = Shared::new(0);

fun fresh(): Row {
	let id = ids.read() + 1;
	ids.write() = id;
	Row { id = id, text = i"{id}" }
}

let renders: Shared<i32> = Shared::new(0);

fun rendered(item: Row): View {
	renders.write() = renders.read() + 1;
	view("li").text(item.text)
}

/// `each_by`'s rows are counted apart, so the three runs S3 pinned keep their
/// number and this one states its own.
let by_renders: Shared<i32> = Shared::new(0);

fun rendered_by(cell: SignalCell<Row>): View {
	by_renders.write() = by_renders.read() + 1;
	view("li").bind_text(cell.map(|item: Row| item.text))
}

fun joined(values: List<Row>, suffix: str): str {
	mut out = "";
	mut first = true;
	for value in values {
		if !first {
			out = out + ",";
		}
		out = out + value.text + suffix;
		first = false;
	}
	out
}

fun main() {
	let walk: ListCell<Row> = ListCell<Row>::of([fresh(), fresh(), fresh()]);
	let starred = map_each(walk, |item: Row| Row { id = item.id, text = item.text + "*" });
	let _root = mount_root("app", || {
		view("div")
			.child(view("ol").child(each(walk, |item: Row| item.id, |item: Row| rendered(item))))
			.child(view("ol").child(each(starred, |item: Row| item.id, |item: Row| rendered(item))))
			.child(view("ol").child(each_values(walk, |item: Row| rendered(item))))
			.child(view("ol").child(each_by(walk, |item: Row| item.id, |cell: SignalCell<Row>| rendered_by(cell))))
	});
	renders.write() = 0;
	by_renders.write() = 0;
	mut naive = 0;
	mut turn = 1;
	for turn <= 300 {
		let op_count = 1 + next_random(4);
		batch(|| {
			mut made = 0;
			for made < op_count {
				let size = walk.size();
				let choice = next_random(24);
				// Biased to GROW, as S2's walk is.
				if choice < 7 || size == 0 {
					walk.push(fresh());
				} else if choice < 10 {
					walk.insert_at(next_random(size + 1), fresh());
				} else if choice < 12 {
					walk.remove_at(next_random(size));
				} else if choice < 13 {
					// An element changed in place: a new row under the same key.
					let at = next_random(size);
					let held = walk.get()[at];
					walk.set_at(at, Row { id = held.id, text = held.text + "'" });
				} else if choice < 14 {
					// ...and an element replaced by a different key.
					walk.set_at(next_random(size), fresh());
				} else if choice < 17 {
					let from = next_random(size);
					let count = 1 + next_random(size - from);
					walk.move_range(from, count, next_random(size - count + 1));
				} else if choice < 18 {
					walk.pop();
				} else if choice < 19 && size > 12 {
					walk.remove_range(next_random(size), 1 + next_random(4));
				} else if choice < 20 && size > 20 {
					walk.truncate(next_random(size));
				} else if choice < 21 && size > 16 {
					// The wholesale write: a `Reset`, the pass.
					mut rows: List<Row> = [];
					mut fill = 0;
					for fill < 1 + next_random(6) {
						rows.push(fresh());
						fill += 1;
					}
					walk.set(rows);
				} else if choice < 22 {
					// The compat door: one element edited, one appended.
					mut edited = walk.get();
					let at = next_random(size);
					edited[at] = Row { id = edited[at].id, text = edited[at].text + "~" };
					edited.push(fresh());
					walk.reconcile_to(edited);
				} else {
					// The batch door, inside the batch.
					let at = next_random(size);
					walk.edit(|&mut list| {
						list.insert_at(at, fresh());
						list.remove_at(0);
						list.push(fresh());
					});
				}
				made += 1;
			}
		});
		let held = walk.get();
		naive += held.len() * 3;
		let wanted = i"{joined(held, "")}|{joined(held, "*")}|{joined(held, "")}|{joined(held, "")}";
		let shown = lists();
		if shown != wanted {
			panic(i"turn {turn}: the document shows\n  {shown}\nwhere the cell holds\n  {wanted}");
		}
		turn += 1;
	}
	print(i"walk: turns=300 length={walk.get().len()} ids={ids.read()} renders={renders.read()} by={by_renders.read()} rerun={naive}");
}

[extern("__lists")]
external fun lists(): str;

main();
"#;

/// Every `<ol>` in document order, each as its rows' labels joined by commas,
/// and the lists joined by `|` — the program compares this against the cell.
const A112_S3_WALK_HARNESS_TAIL: &str = r#"
global.__lists = () => {
    const out = [];
    const visit = (node) => {
        if (node.tagName === "ol") {
            out.push(node.children.filter((child) => child.tagName === "li").map((child) => child._text).join(","));
        }
        node.children.forEach(visit);
    };
    visit(documentRoot);
    return out.join("|");
};
require("./app.js");
"#;

/// A112 S3: the law, in the DOM. Every assertion is inside the program (a
/// `panic` names the turn and both runs); this reads the exit code and holds
/// the printed tail, so a walk that stops reaching the vocabulary — or a
/// generator change — reds instead of passing quietly.
///
/// Non-vacuity, three plants, each red at a named turn: the `Move` arm handing
/// the pass `source.get()` — the state after the WHOLE drain, which is what the
/// patch held at Order 39 wrote — instead of the list after the move (turn 4);
/// `splice_rows` placing arrivals at the anchor instead of before the next
/// row's marker (turn 1); and the `SetAt` arm skipping the rebuild when only
/// the key matches (turn 13). The same three planted into `place_each_by`
/// alone red the fourth run, and only it, at the same turns — 4, 1 and
/// 13 — the last as `each_by`'s own form of it, a same-key `SetAt`
/// that skips the write into the row's cell.
#[test]
fn a112_s3_the_random_walk_holds_through_each() {
    let harness = format!("{DOM_STUB}{A112_S3_WALK_HARNESS_TAIL}");
    let stdout = build_and_run("a112_s3_walk", A112_S3_WALK, &harness);
    assert_eq!(
        stdout, "walk: turns=300 length=0 ids=587 renders=3306 by=1078 rerun=12258\n",
        "A112 S3's walk tail moved: the renders count is rows BUILT across \
         the three S3 runs, `by` the rows `each_by` built (S3b), the rerun \
         the rows a pass would have looked at over the three; got:\n{stdout}"
    );
}

// --- A112 S3b: `each_by` on the op path ---------------------------------------

/// `A112_S3_OPS` over `each_by`: the same edits, each printed as
/// `pass|tree|cost`, where the cost adds `maps` — the calls of the row's own
/// `map`, which is one per row BUILT plus one per write into a row's CELL. That
/// column is what separates `each_by` from `each`: a changed item under a
/// surviving key is written into the row's cell (`maps=1`, nothing built), and
/// a pass writes into EVERY surviving row's cell whether it changed or not.
const A112_S3B_OPS: &str = r#"import std::io::print;
import std::reactive::{ ListCell, SequenceCell, Signal, SignalCell, Source };
import std::shared::Shared;
import std::ui::{ View, each_by, mount_root, view };

struct Row {
	id: i32,
	text: str,
}

fun row(id: i32, text: str): Row {
	Row { id = id, text = text }
}

let keys: Shared<i32> = Shared::new(0);
let maps: Shared<i32> = Shared::new(0);

fun keyed(item: Row): i32 {
	keys.write() = keys.read() + 1;
	item.id
}

fun label(cell: SignalCell<Row>): View {
	view("li").bind_text(cell.map(|item: Row| {
		maps.write() = maps.read() + 1;
		item.text
	}))
}

fun spent(): str {
	let line = i"{cost()} keys={keys.read()} maps={maps.read()}";
	keys.write() = 0;
	maps.write() = 0;
	line
}

fun main() {
	let rows: ListCell<Row> = ListCell<Row>::of([row(1, "a"), row(2, "b"), row(3, "c"), row(4, "d")]);
	let _root = mount_root("app", || {
		view("ul")
			.child(view("li").text("H"))
			.child(each_by(rows, keyed, |cell: SignalCell<Row>| label(cell)))
			.child(view("li").text("F"))
	});
	print(i"mount|{tree()}|{spent()}");
	rows.push(row(5, "e"));
	print(i"push|{tree()}|{spent()}");
	rows.prepend(row(0, "z"));
	print(i"prepend|{tree()}|{spent()}");
	rows.remove_at(2);
	print(i"remove|{tree()}|{spent()}");
	rows.set_at(1, row(1, "A"));
	print(i"set_at|{tree()}|{spent()}");
	rows.set_at(1, row(1, "A"));
	print(i"set_at-same|{tree()}|{spent()}");
	rows.set_at(2, row(33, "k"));
	print(i"set_at-key|{tree()}|{spent()}");
	rows.insert_all(2, [row(7, "g"), row(8, "h")]);
	print(i"insert_all|{tree()}|{spent()}");
	rows.edit(|&mut list| {
		list.push(row(9, "i"));
		list.remove_at(0);
		list.insert_at(1, row(6, "f"));
	});
	print(i"edit|{tree()}|{spent()}");
	rows.move_range(0, 1, 3);
	print(i"move|{tree()}|{spent()}");
	rows.set([row(1, "a"), row(2, "b")]);
	print(i"set|{tree()}|{spent()}");
	rows.clear();
	print(i"clear|{tree()}|{spent()}");

	// The control: a plain cell keeps the pass.
	let plain: SignalCell<List<Row>> = Signal::new([row(1, "a"), row(2, "b"), row(3, "c")]);
	let _second = mount_root("app", || {
		view("ol").child(each_by(plain, keyed, |cell: SignalCell<Row>| label(cell)))
	});
	let _reset = spent();
	plain.update(|&mut list| { list.push(row(4, "d")); });
	print(i"plain-push||{spent()}");

	// At scale: one push into 1,000 rows, one in-place change, one removal.
	mut many: List<Row> = [];
	mut index = 0;
	for index < 1000 {
		many.push(row(index, "r"));
		index += 1;
	}
	let big: ListCell<Row> = ListCell<Row>::of(many);
	let _third = mount_root("app", || {
		view("ol").child(each_by(big, keyed, |cell: SignalCell<Row>| label(cell)))
	});
	let _mounted = spent();
	big.push(row(1000, "new"));
	print(i"big-push||{spent()}");
	big.set_at(250, row(250, "changed"));
	print(i"big-set_at||{spent()}");
	big.remove_at(500);
	print(i"big-remove||{spent()}");
}

[extern("__tree")]
external fun tree(): str;

[extern("__cost")]
external fun cost(): str;

main();
"#;

/// A112 S3b: a `ListCell` under `each_by` is followed by its OPS, as `each`'s
/// run has been since S3.
///
/// The claim is the cost column at 1,000 rows — one push builds ONE row, calls
/// the key function once and runs one row `map`; an in-place change under the
/// same key builds NOTHING and runs exactly one row `map` (the write into that
/// row's cell, which is what `each_by` is for); a removal cuts one row and
/// touches no other. The tree column is what says the cheaper path still
/// shows the right run after every edit in the vocabulary, including a `SetAt`
/// that changes the KEY (the row is rebuilt, as the pass would) and the
/// `Move`/`Reset` fallbacks to the pass. The plain `SignalCell` beside it is
/// the control: the same push costs a whole pass there, and writes into every
/// surviving row's cell (`maps=4` over three rows and one arrival).
///
/// Non-vacuity: on the tree before S3b (`place_each_by` answering every source
/// with the pass) the tree column is unchanged and the cost column reds —
/// `big-push` reads `keys=2002 maps=1001`.
#[test]
fn a112_s3b_a_list_cell_under_each_by_builds_only_the_rows_its_ops_name() {
    let harness = format!("{DOM_STUB}{A98_COST_HARNESS_TAIL}");
    let stdout = build_and_run("a112_s3b_ops", A112_S3B_OPS, &harness);
    let seen: Vec<(String, Vec<String>, String)> = stdout
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('|');
            let name = parts.next()?.to_string();
            let tree = nodes(parts.next()?);
            let cost = parts.next()?.to_string();
            Some((name, tree, cost))
        })
        .collect();
    let tree = |labels: &[&str]| {
        let mut expected = vec!["root".to_string(), "ul".to_string(), "li'H'".to_string()];
        for label in labels {
            expected.push(format!("li'{label}'"));
        }
        expected.push("li'F'".to_string());
        expected
    };
    let none: Vec<String> = Vec::new();
    let expected: Vec<(String, Vec<String>, String)> = vec![
        // The first build is the pass: seven elements, four keys read twice
        // (the plan and the key list it keeps), one row `map` per row.
        (
            "mount",
            tree(&["a", "b", "c", "d"]),
            "cut=0 built=7 keys=8 maps=4",
        ),
        (
            "push",
            tree(&["a", "b", "c", "d", "e"]),
            "cut=0 built=1 keys=1 maps=1",
        ),
        (
            "prepend",
            tree(&["z", "a", "b", "c", "d", "e"]),
            "cut=0 built=1 keys=1 maps=1",
        ),
        (
            "remove",
            tree(&["z", "a", "c", "d", "e"]),
            "cut=1 built=0 keys=0 maps=0",
        ),
        // Same key, different item: the row is KEPT and its cell written —
        // where `each` rebuilds it (`cut=1 built=1`).
        (
            "set_at",
            tree(&["z", "A", "c", "d", "e"]),
            "cut=0 built=0 keys=1 maps=1",
        ),
        // Same key, equal item: still a write into the cell, as the pass's
        // `Keep` makes one — `each_by` asks nothing of `T`, so it cannot tell.
        (
            "set_at-same",
            tree(&["z", "A", "c", "d", "e"]),
            "cut=0 built=0 keys=1 maps=1",
        ),
        // A different KEY at that position: the row that was there is gone and a
        // new one is built, as the pass would plan it.
        (
            "set_at-key",
            tree(&["z", "A", "k", "d", "e"]),
            "cut=1 built=1 keys=2 maps=1",
        ),
        (
            "insert_all",
            tree(&["z", "A", "g", "h", "k", "d", "e"]),
            "cut=0 built=2 keys=2 maps=2",
        ),
        // Three ops, one notification: the push and the insert build a row
        // each, the removal cuts one.
        (
            "edit",
            tree(&["A", "f", "g", "h", "k", "d", "e", "i"]),
            "cut=1 built=2 keys=2 maps=2",
        ),
        // A `Move` is the pass at the moved list: the one row that moved is cut
        // and put back (A98), nothing is built, and — the pass's price under
        // `each_by` — every kept row's cell is written.
        (
            "move",
            tree(&["f", "g", "h", "A", "k", "d", "e", "i"]),
            "cut=1 built=0 keys=16 maps=8",
        ),
        // A `Reset` is the pass: every row whose key left is cut, and the one
        // key that survives (`1`, now "A" -> "a") keeps its row and has its
        // cell written, which is where this differs from `each`'s `cut=8`.
        ("set", tree(&["a", "b"]), "cut=7 built=1 keys=4 maps=2"),
        ("clear", tree(&[]), "cut=2 built=0 keys=0 maps=0"),
        ("plain-push", none.clone(), "cut=0 built=1 keys=8 maps=4"),
        ("big-push", none.clone(), "cut=0 built=1 keys=1 maps=1"),
        ("big-set_at", none.clone(), "cut=0 built=0 keys=1 maps=1"),
        ("big-remove", none, "cut=1 built=0 keys=0 maps=0"),
    ]
    .into_iter()
    .map(|(name, tree, cost)| (name.to_string(), tree, cost.to_string()))
    .collect();
    assert_eq!(
        seen, expected,
        "A112 S3b: a `ListCell` under `each_by` must build only the rows its \
         ops name, write only the cells they name, and still show the right \
         run; a plain `SignalCell` keeps the pass; got:\n{stdout}"
    );
}

// --- M86: a `ListCell` write copies no whole list -----------------------------

/// A 1,000-row `ListCell` with a `map_each`, an `each` and an `each_by` on it,
/// driven through the vocabulary; after each write the program prints how many
/// WHOLE-LIST copies the write made. A deep copy of a list is the emitted
/// `__clone`, which is `value.map(__clone)`, so the harness counts
/// `Array.prototype.map` calls on arrays of 1,000 elements or more — a row's
/// own copy (a two-element array) is not counted, a copy of the run is.
const M86_COPIES: &str = r#"import std::io::print;
import std::option::Option::{ self, None, Some };
import std::reactive::{ ListCell, SequenceCell, Signal, SignalCell, Source, map_each };
import std::ui::{ View, each, each_by, mount_root, view };

[derive(PartialEq, Hashable)]
struct Row {
	id: i32,
	text: str,
}

fun row(id: i32, text: str): Row {
	Row { id = id, text = text }
}

fun main() {
	mut many: List<Row> = [];
	mut index = 0;
	for index < 1000 {
		many.push(row(index, "r"));
		index += 1;
	}
	let cell: ListCell<Row> = ListCell<Row>::of(many);
	let ids = map_each(cell, |item: Row| item.id);
	let _root = mount_root("app", || {
		view("div")
			.child(view("ol").child(each(cell, |item: Row| item.id, |item: Row| view("li").text(item.text))))
			.child(view("ol").child(each_by(cell, |item: Row| item.id, |held: SignalCell<Row>| {
				view("li").bind_text(held.map(|item: Row| item.text))
			})))
	});
	let _mounted = wholes();
	cell.push(row(1000, "pushed"));
	print(i"push wholes={wholes()}");
	cell.insert_at(10, row(1001, "inserted"));
	print(i"insert_at wholes={wholes()}");
	cell.remove_at(20);
	print(i"remove_at wholes={wholes()}");
	cell.set_at(30, row(30, "changed"));
	print(i"set_at wholes={wholes()}");
	cell.move_range(0, 2, 5);
	print(i"move_range wholes={wholes()}");
	let size = cell.size();
	print(i"size={size} wholes={wholes()}");
	let total: i32 = cell.peek(|list| list.len());
	let third: Option<Row> = cell.peek(|list| list.get(2));
	let text = match third {
		Some(let found) => found.text,
		None => "none",
	};
	print(i"peek={total} third={text} wholes={wholes()}");
	cell.edit(|&mut list| {
		list.push(row(1002, "edited"));
		list.remove_at(0);
	});
	print(i"edit wholes={wholes()}");
	mut edited = cell.get();
	print(i"get wholes={wholes()}");
	edited[40] = row(edited[40].id, "reconciled");
	cell.reconcile_to(edited);
	print(i"reconcile_to wholes={wholes()}");
	print(i"size={cell.size()} ids={ids.size()} rows={rows()}");
}

[extern("__wholes")]
external fun wholes(): i32;

[extern("__rows")]
external fun rows(): str;

main();
"#;

/// Counts whole-list copies (see `M86_COPIES`) and reads the two runs' lengths.
const M86_HARNESS_TAIL: &str = r#"
global.__wholes = (() => {
    let wholes = 0;
    const map = Array.prototype.map;
    Array.prototype.map = function (...args) {
        if (this.length >= 1000) wholes += 1;
        return map.apply(this, args);
    };
    return () => { const seen = wholes; wholes = 0; return seen; };
})();
global.__rows = () => {
    const out = [];
    const visit = (node) => {
        if (node.tagName === "ol") out.push(node.children.filter((child) => child.tagName === "li").length);
        node.children.forEach(visit);
    };
    visit(documentRoot);
    return out.join("/");
};
require("./app.js");
"#;

/// M86: a write to a 1,000-row `ListCell` copies no whole list — not the cell's
/// own, not a derived `map_each` cell's, and not on its way to `each` or
/// `each_by` — and `peek`, the read that borrows (R-e), answers a length and one
/// element without copying the run.
///
/// Before M86 every `splice` read the cell's length through `get()`, which is a
/// deep copy of the run, and the everyday defaults (`push`, `insert_at`) read it
/// twice — once to find the end, once inside `splice` — so a push into 1,000
/// rows copied the run twice and a `map_each` over it copied ITS run once more
/// (6.27 M of the 6.30 M Ir a push cost under `each`). `set_at` copied the run
/// to read one element, and `reconcile_to` copied it to diff it. What still
/// copies is what must, and the pin states it rather than hiding it: `get()`
/// hands out a value (one); `edit` hands its body a recorder over a list of its
/// own and takes the result back (two, per batch, however many mutations it
/// holds); and a `Move` is still the whole-list PASS in both `each` and
/// `each_by` (A112 S3's fallback), whose own copies of the run are the eleven —
/// the `ListCell` and the `map_each` make none of them.
///
/// Non-vacuity: on the tree before M86 this reads `push wholes=3`,
/// `insert_at wholes=2`, `remove_at wholes=2`, `set_at wholes=2`,
/// `size=1001 wholes=1`, `edit wholes=6` and `reconcile_to wholes=3`.
#[test]
fn m86_a_list_cell_write_copies_no_whole_list() {
    let harness = format!("{DOM_STUB}{M86_HARNESS_TAIL}");
    let stdout = build_and_run("m86_copies", M86_COPIES, &harness);
    assert_eq!(
        stdout,
        concat!(
            "push wholes=0\n",
            "insert_at wholes=0\n",
            "remove_at wholes=0\n",
            "set_at wholes=0\n",
            "move_range wholes=11\n",
            "size=1001 wholes=0\n",
            "peek=1001 third=r wholes=0\n",
            "edit wholes=2\n",
            "get wholes=1\n",
            "reconcile_to wholes=0\n",
            "size=1001 ids=1001 rows=1001/1001\n",
        ),
        "M86: a `ListCell` write must copy no whole list; got:\n{stdout}"
    );
}
