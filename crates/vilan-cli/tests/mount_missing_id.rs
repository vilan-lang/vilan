//! End-to-end (A24, fullstack-dx.md §9.5): `mount`/`mount_root` on a missing
//! element id used to throw a bare "Cannot read properties of null (reading
//! …)" from `element.clear()`, with the id the caller got wrong appearing
//! nowhere in the message. `mount_target` now checks `get_element_by_id`'s
//! result before touching it, so the failure names the id instead.
//!
//! Runs the compiled client under the shared DOM stub (`support/dom/stub.js`,
//! N73) — real `node`, not a compile-only pin — with `getElementById` missing
//! for the missing-id case and answering for the happy path, so both the
//! failing AND the unaffected path are proven against actual execution, not
//! assumed from reading the source.

use std::path::{Path, PathBuf};
use std::process::Command;

mod support;

fn temp_project(tag: &str) -> PathBuf {
    let dir = support::scratch_root().join(format!(
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

/// The DOM the suite runs against (N73/N75): the shared stub, and nothing
/// layered on it. Every other browser-bundle suite concatenates a
/// `support/dom/<suite>.js` of its own, and this one has no extras to put
/// there — its only knob is `installStubDocument`'s `rootId`, which differs
/// between the two legs and therefore belongs at the install call below rather
/// than in a file shared by both.
const DOM_STUB: &str = include_str!("support/dom/stub.js");

/// One leg's harness. `root_id` is the id `getElementById` answers with the
/// document root, written as a JSON string: `"app"` is the happy path, and any
/// other id is the missing one — `mount_root`'s missing-id path is the reason
/// the shared stub's `rootId` knob exists at all, because every other suite is
/// content for every id to answer.
fn harness(root_id: &str) -> String {
    format!(
        "{DOM_STUB}\ninstallStubDocument({{ rootTag: \"div\", rootId: {root_id} }});\n\
         require(\"./client.js\");\nconsole.log(\"mounted-ok\");\n"
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

fn project(tag: &str, root_id: &str) -> PathBuf {
    let dir = temp_project(tag);
    write(&dir, "client.vl", CLIENT);
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"client\"\nroot = \".\"\nentry = \"client.vl\"\ntarget = \"browser\"\n",
    );
    write(&dir, "harness.js", &harness(root_id));
    build(&dir);
    dir
}

#[test]
fn mount_root_on_a_missing_id_panics_naming_the_id() {
    // `getElementById` answers for one id and the app asks for another, so the
    // miss is the stub's real `null`-for-missing contract rather than a
    // special case: the root is on the page, under a different id.
    let dir = project("missing", "\"not-the-apps-id\"");
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
    let dir = project("present", "\"app\"");
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
