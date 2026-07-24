//! The cross-implementation differential for A7 SSR (proposal/ssr.md §2, §4 S1,
//! §5's drift gate): ONE shared component module, compiled twice, must produce
//! the same tree from `std::ui`'s two layers — the browser layer building a live
//! DOM and the process layer building an HTML string.
//!
//! - The BROWSER leg builds `component.vl` + a `mount_root` client, runs it under
//!   the A10 DOM stub, and serializes the mounted tree.
//! - The PROCESS leg builds the SAME `component.vl` + a `render` server, and
//!   `render(app())` is the string.
//!
//! THE CANONICAL FORM. The DOM records the properties the browser ui writes —
//! `className`, `hidden`, `value` — separately from `setAttribute`. Left alone,
//! that property-vs-attribute divide would false-diff against the process ui,
//! which keeps ONE ordered attribute list. So the stub folds those properties
//! INTO an ordered `[name, value]` list at write time (className→`class`,
//! `hidden`→a `hidden` attribute, `value`→a `value` attribute), and serializes
//! with the exact escaping and void-element rules the process `render` uses. The
//! canonical form is therefore that serialization; with the mapping applied on
//! the browser side, structural equality is byte equality — the two trees agree
//! on tags, attributes (and their insertion order), text, and nesting.
//!
//! Not exercised by THIS differential (all covered by the inference snapshot
//! pins instead): `on_event` — present in both layers, but a handler that touches
//! the event needs the browser-only `std::dom::Event`, so it is not part of a
//! shared component; `style_var` — the stub no-ops `style.setProperty`, so its
//! folded `style` attribute is not observable; `mount` — a client entry, not a view.

use std::path::{Path, PathBuf};
use std::process::Command;

fn temp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vilan_ssr_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// The shared component — identical bytes in both legs (written to each package).
/// It exercises every read-once binding form: static `class`/`attr`, `bind_text`,
/// `bind_class`, `bind_attr`, `bind_each` (keyed, over a list), `when` (taken),
/// `show` (hidden), `swap` (a value branch), `bind_value`, a discarded `on`
/// handler, and nested composition — with `&`/`<`/`>`/`"` in the data to drive
/// escaping on both sides.
const COMPONENT: &str = r#"import std::ui::{ view, View };
import std::reactive::Signal;

[derive(PartialEq)]
enum Tab {
	Home,
	Settings,
}

[derive(PartialEq)]
struct Row {
	id: i32,
	label: str,
}

fun app(): View {
	let title = Signal::new("Dashboard & <you>");
	let cls = Signal::new("live");
	let href = Signal::new("/home");
	let rows: Signal<List<Row>> = Signal::new([
		Row { id = 1, label = "first \"one\"" },
		Row { id = 2, label = "second & third" },
	]);
	let show_banner = Signal::new(true);
	let hide_note = Signal::new(false);
	let tab = Signal::new(Tab::Settings);
	let query = Signal::new("hello");
	view("main")
		.class("app")
		.attr("id", "root")
		.child(view("h1").bind_text(title))
		.child(view("a").bind_class(cls).bind_attr("href", href).text("link"))
		.child(view("ul").bind_each(rows, |r| r.id, |r| view("li").text(r.label)))
		.child(view("section").when(show_banner, || view("p").text("banner")))
		.child(view("aside").show(hide_note))
		.child(view("nav").swap(tab, |t| match t {
			Tab::Home => view("a").text("home"),
			Tab::Settings => view("a").text("settings & more"),
		}))
		.child(view("input").attr("type", "text").bind_value(query))
		.child(view("button").text("save").on("click", || query.set("x")))
}
"#;

const CLIENT: &str = r#"import std::ui::mount_root;
import pkg::component::app;

fun main() {
	let _root = mount_root("app", || app());
}
main();
"#;

const SERVER: &str = r#"import std::ui::render;
import std::print;
import pkg::component::app;

fun main() {
	print(render(app()));
}
main();
"#;

/// The DOM/history stub plus the canonical serializer (see the module doc).
const HARNESS: &str = r#"const VOID = new Set(["area","base","br","col","embed","hr","img","input","link","meta","source","track","wbr"]);
const escapeText = s => s.replaceAll("&","&amp;").replaceAll("<","&lt;").replaceAll(">","&gt;");
const escapeAttr = s => s.replaceAll("&","&amp;").replaceAll('"',"&quot;");

class StubElement {
    constructor(tag) {
        this.tagName = tag;
        this.children = [];
        this.parent = null;
        this.listeners = {};
        this.text = "";
        this.attributes = [];
        this.style = { setProperty: (n, v) => this._upsertStyle(n, v) };
    }
    _upsert(name, value) {
        const i = this.attributes.findIndex(([n]) => n === name);
        if (i >= 0) this.attributes[i] = [name, value]; else this.attributes.push([name, value]);
    }
    _remove(name) { this.attributes = this.attributes.filter(([n]) => n !== name); }
    _upsertStyle(name, value) {
        const cur = this.attributes.find(([n]) => n === "style");
        const decl = name + ":" + value;
        this._upsert("style", cur ? cur[1] + ";" + decl : decl);
    }
    set className(v) { this._upsert("class", v); }
    get className() { const a = this.attributes.find(([n]) => n === "class"); return a ? a[1] : ""; }
    setAttribute(name, value) { this._upsert(name, value); }
    set hidden(v) { if (v) this._upsert("hidden", ""); else this._remove("hidden"); }
    get hidden() { return this.attributes.some(([n]) => n === "hidden"); }
    set value(v) { this._upsert("value", v); }
    get value() { const a = this.attributes.find(([n]) => n === "value"); return a ? a[1] : ""; }
    set textContent(text) { this.text = text; this.children = []; }
    get textContent() { return this.text; }
    appendChild(child) {
        if (child.parent) child.parent.children = child.parent.children.filter(c => c !== child);
        child.parent = this; this.children.push(child);
    }
    remove() { if (this.parent) { this.parent.children = this.parent.children.filter(c => c !== this); this.parent = null; } }
    replaceChildren() { for (const c of this.children) c.parent = null; this.children = []; }
    addEventListener(event, handler) { (this.listeners[event] = this.listeners[event] || []).push(handler); }
}
function serialize(el) {
    let out = "<" + el.tagName;
    for (const [name, value] of el.attributes) out += ` ${name}="${escapeAttr(value)}"`;
    out += ">";
    if (VOID.has(el.tagName)) return out;
    out += escapeText(el.text);
    for (const c of el.children) out += serialize(c);
    return out + "</" + el.tagName + ">";
}

const root = new StubElement("app-root");
global.document = {
    createElement: (tag) => new StubElement(tag),
    getElementById: (id) => (id === "app" ? root : null),
    querySelector: () => null, querySelectorAll: () => [],
};
global.window = { addEventListener: () => {} };
global.location = { pathname: "/" };

require("./client.js");
console.log(serialize(root.children[0]));
"#;

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

#[test]
fn ssr_process_render_matches_browser_dom_tree() {
    let root = temp_project("differential");
    let client = root.join("client");
    let server = root.join("server");

    // The SAME component source in both packages — the differential's premise.
    write(&client, "component.vl", COMPONENT);
    write(&server, "component.vl", COMPONENT);
    write(&client, "client.vl", CLIENT);
    write(&server, "server.vl", SERVER);
    write(
        &client,
        "vilan.toml",
        "[package]\nname = \"client\"\nroot = \".\"\nentry = \"client.vl\"\ntarget = \"browser\"\n",
    );
    write(
        &server,
        "vilan.toml",
        "[package]\nname = \"server\"\nroot = \".\"\nentry = \"server.vl\"\n",
    );
    write(&client, "harness.js", HARNESS);

    build(&client);
    build(&server);

    // Browser leg: mount under the DOM stub, serialize the canonical form.
    let browser = Command::new("node")
        .arg("harness.js")
        .current_dir(&client)
        .output()
        .expect("run node harness");
    assert!(
        browser.status.success(),
        "browser harness failed:\n{}\n{}",
        String::from_utf8_lossy(&browser.stdout),
        String::from_utf8_lossy(&browser.stderr)
    );
    let browser_tree = String::from_utf8_lossy(&browser.stdout)
        .trim_end()
        .to_string();

    // Process leg: `render(app())`.
    let server_run = Command::new("node")
        .arg("server.js")
        .current_dir(&server)
        .output()
        .expect("run node server");
    assert!(
        server_run.status.success(),
        "server run failed:\n{}\n{}",
        String::from_utf8_lossy(&server_run.stdout),
        String::from_utf8_lossy(&server_run.stderr)
    );
    let server_markup = String::from_utf8_lossy(&server_run.stdout)
        .trim_end()
        .to_string();

    assert_eq!(
        browser_tree, server_markup,
        "the browser DOM tree and the server render diverged"
    );
    // Guard against a stub that silently renders nothing (both empty would pass).
    assert!(
        server_markup.contains("<main class=\"app\" id=\"root\">")
            && server_markup.contains("<li>second &amp; third</li>")
            && server_markup.contains("<aside hidden=\"\">"),
        "rendered markup is missing expected structure: {server_markup}"
    );

    let _ = std::fs::remove_dir_all(&root);
}
