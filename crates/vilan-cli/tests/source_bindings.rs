//! A33: a read-only binding takes a `Source`, and it is a LIVE one.
//!
//! `std::ui`'s read-only binders were widened from the concrete `SignalCell<T>` to a
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

mod support;

/// The exhibit: a user type that is a `Source` and is not a `Signal`. `set`
/// lives outside the trait, so nothing a binding does could reach it — a
/// binding that needed the write side would not compile against this at all.
const STORED: &str = r#"
struct Stored<T> {
	inner: SignalCell<T>,
}

impl Stored<type T> with Source<T> {
	fun get(self): T {
		self.inner.get()
	}

	[must_use]
	fun on_settle(self, subscriber: Subscriber): Subscription {
		self.inner.on_settle(subscriber)
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
const DOM_STUB: &str = concat!(
    include_str!("support/dom/stub.js"),
    include_str!("support/dom/source_bindings.js"),
);

fn temp_project(tag: &str) -> PathBuf {
    let dir = support::scratch_root().join(format!(
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
        r#"import std::reactive::{{ Signal, SignalCell, Source, Subscriber, Subscription }};
import std::ui::{{ View, each, mount_root, view, when }};
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
		.child(view("ul").child(each(items, |item| item, |item| view("li").text(item))))
		.child(view("aside").child(when(present, || view("b").text("here")))));
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
    // A60: `show(false)` makes two writes — the `hidden` attribute and the
    // inline `display:none` that actually hides a styled element. This stub
    // serializes an inline style property as an attribute of its own name.
    assert!(
        updated.contains(r#"<i display="none" hidden>"#),
        "`show(false)` must hide the element after the write; got:\n{updated}"
    );
    assert!(
        !updated.contains("<li>x</li>"),
        "`each` must reconcile the removed row away; got:\n{updated}"
    );
}

// ── B168: `swap` joins them, and it is live too ──────────────────────────────
//
// A33 held `View::swap` back for an inference gap, not for a write — B168 closed
// the gap and the three that waited (`swap`, `swap_split`, `chunk_preload`)
// widened. The compile facts are pinned in `vilan-core`'s bounds suite; what
// only a running program shows is the same thing it showed for the other eight:
// that a widened `swap` still SUBSCRIBES. A `swap` that read its source once
// would mount the right subtree and then never move again, and it would type,
// build and pass every compile pin.

/// The route swap, driven by a user `Source`, dumped at the mount and again
/// after the route moves. `swap` also DISPOSES the previous subtree, so the
/// second dump asserts the old section is gone as well as the new one present —
/// a read-once binding fails on both halves.
#[test]
fn a_user_source_drives_swap_and_keeps_driving_it() {
    let app = format!(
        r#"import std::reactive::{{ Signal, SignalCell, Source, Subscriber, Subscription }};
import std::ui::{{ View, mount_root, swap, view }};
{STORED}
/// The harness serializes the mounted tree under this tag.
[extern("__dump")]
external fun dump(tag: str): void;

fun main() {{
	let route: Stored<str> = Stored::new("home");

	let _root = mount_root("app", || view("main")
		.child(swap(route, |current| view("section").text(i"page {{current}}"))));
	dump("mounted");

	route.set("docs");
	dump("updated");
}}
"#
    );
    let stdout = build_and_run("swap", &app);
    let line = |tag: &str| {
        stdout
            .lines()
            .find(|line| line.starts_with(tag))
            .unwrap_or_else(|| panic!("no {tag} dump in:\n{stdout}"))
            .to_string()
    };
    let mounted = line("mounted");
    let updated = line("updated");

    assert!(
        mounted.contains("<section>page home</section>"),
        "the mount must render the user source's current value; got:\n{mounted}"
    );
    assert!(
        updated.contains("<section>page docs</section>"),
        "a write through the user source must swap the subtree; got:\n{updated}"
    );
    assert!(
        !updated.contains("page home"),
        "`swap` must remove the previous subtree; got:\n{updated}"
    );
}

// ── A142 S7: `when_live`, a store's variant as content ─────────────────────────
//
// `when_some` hands its body a cell of the WHOLE payload, so the body's bindings
// rerun on every payload write. `when_live` follows only the store's
// discriminant slot and hands the body the payload's own `Store<P>`: a write
// inside the payload updates the one binding that reads it, and only a variant
// change builds or tears down. Only a running program shows that — a helper that
// rebuilt on every payload write would compile, mount and update identically.

/// B526 (mirrored-store.md S0, Q11): a `when_live` body's binding reads the
/// LAST payload when the variant ends, not the one the body was built over. A
/// device renamed through another handle and then taken offline: the body's
/// own derivation sees `phone` on the rename, and when the variant ends in the
/// next turn — where a derivation settles ahead of the effect that tears the
/// body down — it sees `phone` again; before the fix it read `laptop`, the
/// payload the body was built over, for that turn.
#[test]
fn b526_a_when_live_body_reads_the_last_payload_as_its_variant_ends() {
    let app = r#"import std::io::print;
import std::reactive::{ Flow, Pipe, Signal, Source, batch };
import std::store::{ Storable, Store };
import std::ui::{ View, mount_root, view, when_live };

[derive(PartialEq, Storable)]
struct Device {
	name: str,
}

[derive(PartialEq, Storable)]
enum Presence {
	Offline,
	Online(Device),
}

fun main() {
	let presence = Store::new(Presence::Online(Device { name = "laptop" }));
	let _root = mount_root("app", || view("main").child(when_live(presence.online(), |device| {
		// A DERIVATION in the body: a turn settles it ahead of the effect that
		// tears the body down, so it runs in the turn that ends the variant.
		let shown = device
			.name()
			.derive(|name| {
				print(i"saw {name}");
				name
			})
			.memo();
		view("p").bind_text(shown)
	})));
	batch(|| {
		let _renamed = presence.online().name().patch("phone");
	});
	batch(|| presence.set(Presence::Offline));
	print("done");
}
"#;
    let stdout = build_and_run("when_live_last", app);
    let seen: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("saw ") || *line == "done")
        .collect();
    assert_eq!(
        seen,
        vec!["saw laptop", "saw phone", "saw phone", "done"],
        "the body is built over `laptop`, the rename reaches it, and the turn that \
         ends the variant reads the LAST payload (`phone`), never the one the body was \
         built over; got:\n{stdout}"
    );
}

/// A store whose variant carries a payload, behind `when_live`. The body PRINTS
/// each time it is built, so the build count is on stdout beside the dumps.
#[test]
fn a142_s7_when_live_rebuilds_only_when_the_variant_changes() {
    let app = r#"import std::io::print;
import std::reactive::{ Signal, Source };
import std::store::{ Storable, Store };
import std::ui::{ View, mount_root, view, when_live };

/// The harness serializes the mounted tree under this tag.
[extern("__dump")]
external fun dump(tag: str): void;

[derive(PartialEq, Storable)]
struct Device {
	name: str,
	since: i32,
}

[derive(PartialEq, Storable)]
enum Presence {
	Offline,
	Online(Device),
}

fun main() {
	let presence = Store::new(Presence::Online(Device { name = "laptop", since = 1 }));
	let _root = mount_root("app", || view("main").child(when_live(presence.online(), |device| {
		print("built");
		// The handles bind through the `Flow` arms alone (B476): a text binding
		// and an attribute value.
		view("p").attr("title", device.name()).bind_text(device.name())
	})));
	dump("mounted");
	let _renamed = presence.online().name().patch("phone");
	presence.set(Presence::Online(Device { name = "phone", since = 2 }));
	dump("patched");
	presence.set(Presence::Offline);
	dump("offline");
	presence.set(Presence::Online(Device { name = "tablet", since = 3 }));
	dump("online");
}
"#;
    let stdout = build_and_run("when_live", app);
    let lines: Vec<&str> = stdout.lines().collect();
    let position = |tag: &str| {
        lines
            .iter()
            .position(|line| line.starts_with(tag))
            .unwrap_or_else(|| panic!("no {tag} dump in:\n{stdout}"))
    };
    let builds_before = |index: usize| {
        lines[..index]
            .iter()
            .filter(|line| **line == "built")
            .count()
    };
    let (mounted, patched, offline, online) = (
        position("mounted"),
        position("patched"),
        position("offline"),
        position("online"),
    );
    assert!(
        lines[mounted].contains(r#"<p title="laptop">laptop</p>"#),
        "mount:\n{stdout}"
    );
    assert!(
        lines[patched].contains(r#"<p title="phone">phone</p>"#),
        "a payload write must reach the body's binding:\n{stdout}"
    );
    assert_eq!(
        builds_before(patched),
        1,
        "a payload write must not rebuild the body:\n{stdout}"
    );
    assert!(
        !lines[offline].contains("<p>"),
        "the variant ending must take the body down:\n{stdout}"
    );
    assert!(
        lines[online].contains(r#"<p title="tablet">tablet</p>"#),
        "rebuilt:\n{stdout}"
    );
    assert_eq!(
        builds_before(online),
        2,
        "the variant coming back builds the body once more:\n{stdout}"
    );
}
