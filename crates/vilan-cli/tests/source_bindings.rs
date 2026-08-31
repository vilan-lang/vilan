//! A33: a read-only binding takes a `Source`, and it is a LIVE one.
//!
//! `std::ui`'s read-only binders were widened from the concrete `Signal<T>` to a
//! `Source<T>` bound, so a user's own reactive type can drive them. That the
//! widened signatures ACCEPT such a type is a compile fact, pinned in
//! `vilan-core`'s inference suite. What only a running program can show is that
//! they still bind: a widened `bind_text` that read its source once and never
//! subscribed would type-check identically and pass every compile pin.
//!
//! So this suite builds a browser app with the real CLI, runs it under a DOM
//! stub, and asserts on the DOM twice — after the mount, and after values are
//! pushed through the user type. The exhibit is kolt's `StorageSignal` in
//! miniature: a struct wrapping a `Signal`, implementing `Source` by delegation,
//! with `set` deliberately OFF the trait.
//!
//! The stub is this file's own rather than the one `reactive_lifetimes.rs`
//! carries, and the reason is what each measures: that suite walks the object
//! GRAPH, so its stub records parent/child links and nothing else — attributes,
//! style properties and the `hidden` flag are dropped there on purpose. This
//! suite's whole claim is what those slots hold, so it needs a stub that keeps
//! them and can serialize the tree.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The exhibit: a user type that is a `Source` and is not a `Signal`. `set`
/// lives outside the trait, so nothing a binding does could reach it — a
/// binding that needed the write side would not compile against this at all.
const STORED: &str = r#"
struct Stored<T> {
	inner: Signal<T>,
}

impl Stored<type T> with Source<T> {
	fun get(self): T {
		self.inner.get()
	}

	[must_use]
	fun sub(self, observer: |T| void): Subscription {
		self.inner.sub(observer)
	}
}

impl Stored<type T> {
	fun new(value: T): Stored<T> {
		Stored { inner = Signal::new(value) }
	}

	fun set(self, value: T) {
		self.inner.set(value);
	}
}
"#;

/// A DOM stub that REMEMBERS what a binding wrote — attributes, style
/// properties, the hidden flag, text — and can serialize the tree, which is
/// what makes "the binding fired again" an observable fact rather than an
/// inference from the absence of a crash.
const DOM_STUB: &str = r#"class StubElement {
    constructor(tag) {
        this.tagName = tag;
        this.children = [];
        this.parent = null;
        this.listeners = {};
        this._text = "";
        this.value = "";
        this.hidden = false;
        this.attributes = {};
        this.properties = {};
        this.style = { setProperty: (name, value) => { this.properties[name] = value; } };
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
}

function serialize(node) {
    let out = "<" + node.tagName;
    for (const [name, value] of Object.entries(node.attributes)) out += ` ${name}="${value}"`;
    for (const [name, value] of Object.entries(node.properties)) out += ` ${name}="${value}"`;
    if (node.hidden) out += " hidden";
    out += ">" + node.textContent;
    for (const child of node.children) out += serialize(child);
    return out + "</" + node.tagName + ">";
}

const documentRoot = new StubElement("body");
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
global.__dump = (tag) => console.log(tag + " " + serialize(documentRoot));
"#;

fn temp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "vilan_source_bindings_{tag}_{}",
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

/// Builds `app.vl` for the browser with the real CLI and runs it under the DOM
/// stub, returning stdout. Fails loudly with both streams.
fn build_and_run(tag: &str, app: &str) -> String {
    let dir = temp_project(tag);
    write(
        &dir,
        "vilan.toml",
        &format!(
            "[package]\nname = \"source_bindings_{tag}\"\nroot = \".\"\nentry = \"app.vl\"\ntarget = \"browser\"\n"
        ),
    );
    write(&dir, "app.vl", app);
    write(
        &dir,
        "harness.js",
        &format!("{DOM_STUB}\nrequire(\"./app.js\");\n"),
    );

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

/// Every widened binding, driven by the user `Source`, dumped after the mount
/// and again after the values move.
fn app_source() -> String {
    format!(
        r#"import std::reactive::{{ Signal, Source, Subscription }};
import std::ui::{{ View, mount_root, view }};
{STORED}
/// The harness serializes the mounted tree under this tag.
[extern("__dump")]
external fun dump(tag: str): void;

fun main() {{
	let label: Stored<str> = Stored::new("alpha");
	let classes: Stored<str> = Stored::new("one");
	let href: Stored<str> = Stored::new("/a");
	let width: Stored<str> = Stored::new("10px");
	let visible: Stored<bool> = Stored::new(true);
	let present: Stored<bool> = Stored::new(false);
	let items: Stored<List<str>> = Stored::new(["x"]);

	let _root = mount_root("app", || view("main")
		.child(view("h1").bind_text(label))
		.child(view("p").bind_class(classes))
		.child(view("a").bind_attr("href", href))
		.child(view("div").style_var("--w", width))
		.child(view("i").show(visible))
		.child(view("ul").bind_each(items, |item| item, |item| view("li").text(item)))
		.child(view("aside").when(present, || view("b").text("here"))));
	dump("mounted");

	label.set("beta");
	classes.set("two");
	href.set("/b");
	width.set("20px");
	visible.set(false);
	present.set(true);
	items.set(["y", "z"]);
	dump("updated");
}}
"#
    )
}

#[test]
fn a_user_source_drives_every_widened_binding_and_keeps_driving_it() {
    let stdout = build_and_run("live", &app_source());
    let line = |tag: &str| {
        stdout
            .lines()
            .find(|line| line.starts_with(tag))
            .unwrap_or_else(|| panic!("no {tag} dump in:\n{stdout}"))
            .to_string()
    };
    let mounted = line("mounted");
    let updated = line("updated");

    // The mount reads the source — the half a read-once binding would also pass.
    for expected in [
        "<h1>alpha</h1>",
        r#"<p class="one">"#,
        r#"<a href="/a">"#,
        r#"<div --w="10px">"#,
        "<li>x</li>",
    ] {
        assert!(
            mounted.contains(expected),
            "the mount must render {expected} from the user source; got:\n{mounted}"
        );
    }
    assert!(
        !mounted.contains("<i hidden>"),
        "`show(true)` must leave the element visible; got:\n{mounted}"
    );
    assert!(
        !mounted.contains("<b>here</b>"),
        "`when(false)` must mount no body; got:\n{mounted}"
    );

    // …and the binding is LIVE: a write through the user type reaches the DOM.
    // This is the half that separates a real `Source` binding from one that
    // merely type-checks — every assertion here fails on a read-once binding.
    for expected in [
        "<h1>beta</h1>",
        r#"<p class="two">"#,
        r#"<a href="/b">"#,
        r#"<div --w="20px">"#,
        "<li>y</li>",
        "<li>z</li>",
        "<b>here</b>",
    ] {
        assert!(
            updated.contains(expected),
            "a write through the user source must reach the DOM as {expected}; \
             got:\n{updated}"
        );
    }
    assert!(
        updated.contains("<i hidden>"),
        "`show(false)` must hide the element after the write; got:\n{updated}"
    );
    assert!(
        !updated.contains("<li>x</li>"),
        "`bind_each` must reconcile the removed row away; got:\n{updated}"
    );
}
