//! End-to-end (A24, fullstack-dx.md §9.5): `mount`/`mount_root` on a missing
//! element id used to throw a bare "Cannot read properties of null (reading
//! …)" from `element.clear()`, with the id the caller got wrong appearing
//! nowhere in the message. `mount_target` now checks `get_element_by_id`'s
//! result before touching it, so the failure names the id instead.
//!
//! Runs the compiled client under the A10 DOM stub (the same hand-rolled
//! `document`/`window` shim `ssr_differential.rs`/`ssr_fullstack.rs` use) —
//! real `node`, not a compile-only pin — with `getElementById` returning
//! `null` for the missing-id case and a real stub element for the happy
//! path, so both the failing AND the unaffected path are proven against
//! actual execution, not assumed from reading the source.

use std::path::{Path, PathBuf};
use std::process::Command;

fn temp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "vilan_mount_missing_id_{tag}_{}",
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

const CLIENT: &str = r#"import std::ui::{ mount_root, view };

fun main() {
	let _root = mount_root("app", || view("div").text("hi"));
}
main();
"#;

/// A minimal DOM/owner stub — just enough for `mount_root`'s build (`comp`,
/// `turn`) and `mount` (`get_element_by_id`, `element.clear()`,
/// `element.appendChild`) to run. `getElementById` is the one knob: the
/// missing-id test never returns an element for ANY id, and the happy-path
/// test returns a real stub element for `"app"`.
fn harness(get_element_by_id_body: &str) -> String {
    format!(
        r#"class StubElement {{
    constructor(tag) {{ this.tagName = tag; this.children = []; }}
    appendChild(child) {{ this.children.push(child); }}
    replaceChildren() {{ this.children = []; }}
    clear() {{ this.replaceChildren(); }}
    setAttribute() {{}}
    set textContent(text) {{ this._text = text; }}
    get textContent() {{ return this._text; }}
}}
global.document = {{
    createElement: (tag) => new StubElement(tag),
    createElementNS: (ns, tag) => new StubElement(tag),
    getElementById: (id) => {{ {get_element_by_id_body} }},
    querySelector: () => null,
    querySelectorAll: () => [],
}};
global.window = {{ addEventListener: () => {{}} }};

require("./client.js");
console.log("mounted-ok");
"#
    )
}

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

fn project(tag: &str, get_element_by_id_body: &str) -> PathBuf {
    let dir = temp_project(tag);
    write(&dir, "client.vl", CLIENT);
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"client\"\nroot = \".\"\nentry = \"client.vl\"\ntarget = \"browser\"\n",
    );
    write(&dir, "harness.js", &harness(get_element_by_id_body));
    build(&dir);
    dir
}

#[test]
fn mount_root_on_a_missing_id_panics_naming_the_id() {
    // `getElementById` always misses — the id the app asked for is never on
    // the (stub) page, exactly `get_element_by_id`'s real `null`-for-missing
    // contract.
    let dir = project("missing", "return null;");
    let output = Command::new("node")
        .arg("harness.js")
        .current_dir(&dir)
        .output()
        .expect("run node harness");
    assert!(
        !output.status.success(),
        "mount_root on a missing id should fail (exit non-zero), not silently succeed"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("mounted-ok"),
        "mount should never have returned; stdout was:\n{stdout}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("mount: no element with id 'app'"),
        "expected the panic to name the missing id 'app'; stderr was:\n{stderr}"
    );
    // The OLD failure mode this replaces — pin that it's gone, not just that
    // SOMETHING throws.
    assert!(
        !stderr.contains("Cannot read properties of null"),
        "the old, id-less null-dereference message should not resurface; stderr was:\n{stderr}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mount_root_on_an_existing_id_is_unaffected() {
    // The happy path: `getElementById("app")` returns a real element, exactly
    // as it always has — the guard adds nothing to this path but the check
    // itself.
    let dir = project(
        "present",
        r#"if (id === "app") return new StubElement("div"); return null;"#,
    );
    let output = Command::new("node")
        .arg("harness.js")
        .current_dir(&dir)
        .output()
        .expect("run node harness");
    assert!(
        output.status.success(),
        "mount_root on an existing id should succeed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("mounted-ok"),
        "expected the happy path to reach past mount; stdout was:\n{stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
