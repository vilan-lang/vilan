//! M209: the per-keystroke platform answer is walked once per import graph,
//! not once per keystroke — and moves the moment anything the walk read moves.
//!
//! Each "keystroke" here is what the server does for one: the buffer goes into
//! the overlay (`did_change`), then the two per-analysis askers run on this
//! thread — `resolve_project_context` (the platform colour and the dead-module
//! slice) and `RootResolver::roots` (which entry's world serves the file). The
//! count is `vilan_core::package_reach::recorded_walk_count`, this thread's
//! reach walks.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use super::{BuildPlatform, resolve_project_context};
use crate::world::{RootResolver, WorldRoots};

const MANIFEST: &str = "[package]\nname = \"app\"\ndefault-entry = \"server\"\n\n\
     [entry.client]\ntarget = \"browser\"\n\n[entry.server]\n";
const CLIENT: &str = "import pkg::model::total;\n\nfun main() {\n\ttotal();\n}\n";
const SERVER: &str = "fun main() {}\n";
const MODEL: &str = "export fun total(): i32 {\n\t1\n}\n";
const ORPHAN: &str = "export fun nobody(): i32 {\n\t2\n}\n";

/// A scratch package, unique per test and per call: the overlay and the
/// server's reach memo are process-global.
fn package(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let directory =
        std::env::temp_dir().join(format!("vilan-m209-{tag}-{}-{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(directory.join("src")).expect("a scratch package");
    std::fs::write(directory.join("vilan.toml"), MANIFEST).expect("the manifest");
    for (relative, contents) in files {
        std::fs::write(directory.join("src").join(relative), contents).expect("a source");
    }
    directory
}

/// What one keystroke's two askers answer for `path` with its buffer at `text`.
struct Answer {
    platform: Option<BuildPlatform>,
    roots: WorldRoots,
    walks: u64,
}

fn keystroke(path: &Path, text: &str) -> Answer {
    vilan_core::analyzer::set_document_overlay(path, Some(text.to_string()));
    let before = vilan_core::package_reach::recorded_walk_count();
    let context = resolve_project_context(path, text);
    let roots = RootResolver::for_file(path)
        .expect("a package")
        .roots(path, text);
    Answer {
        platform: context.platform,
        roots,
        walks: vilan_core::package_reach::recorded_walk_count() - before,
    }
}

fn is_browser(platform: Option<BuildPlatform>) -> bool {
    matches!(platform, Some(BuildPlatform::Browser))
}

fn is_node(platform: Option<BuildPlatform>) -> bool {
    matches!(platform, Some(BuildPlatform::Node { .. }))
}

fn close(paths: &[&Path]) {
    for path in paths {
        vilan_core::analyzer::set_document_overlay(path, None);
    }
}

/// The item's pin: a session of body edits walks each entry's graph ONCE —
/// the first keystroke walks the client leg and the server leg (the resolver
/// shares the client's walk), and the next twenty walk nothing — with the
/// same answer every time.
#[test]
fn a_session_of_body_edits_walks_each_entry_once() {
    let directory = package(
        "body",
        &[
            ("client.vl", CLIENT),
            ("server.vl", SERVER),
            ("model.vl", MODEL),
        ],
    );
    let model = directory.join("src/model.vl");
    let first = keystroke(&model, MODEL);
    assert!(is_browser(first.platform), "the client reaches model.vl");
    assert_eq!(
        first.walks, 2,
        "the first keystroke walks the client leg and the server leg, once each"
    );
    for round in 0..20 {
        let text = format!("export fun total(): i32 {{\n\t{round} + 1\n}}\n");
        let answer = keystroke(&model, &text);
        assert_eq!(answer.walks, 0, "body edit {round} walked an import graph");
        assert!(is_browser(answer.platform));
        assert_eq!(answer.roots, first.roots);
    }
    close(&[&model]);
}

/// An import added in a SIBLING's buffer moves the answer on the very next
/// keystroke: the orphan, coloured by the default entry while nothing reaches
/// it, is the client's once the client imports it — and the default entry's
/// again once the import is typed away.
#[test]
fn an_import_added_to_an_entry_moves_the_answer() {
    let directory = package(
        "import",
        &[
            ("client.vl", CLIENT),
            ("server.vl", SERVER),
            ("model.vl", MODEL),
            ("orphan.vl", ORPHAN),
        ],
    );
    let client = directory.join("src/client.vl");
    let orphan = directory.join("src/orphan.vl");
    let unreached = keystroke(&orphan, ORPHAN);
    assert!(
        is_node(unreached.platform),
        "no entry reaches it: the default entry's"
    );
    assert_eq!(unreached.roots.primary, None);

    let importing = format!("import pkg::orphan::nobody;\n{CLIENT}");
    vilan_core::analyzer::set_document_overlay(&client, Some(importing));
    let reached = keystroke(&orphan, ORPHAN);
    assert!(
        is_browser(reached.platform),
        "the client now imports orphan.vl: a stale walk still says the default entry"
    );
    assert_eq!(
        reached.roots.primary,
        Some(vilan_core::util::canonical_path(&client)),
        "and the client's world serves it"
    );
    assert!(
        reached.walks >= 1,
        "the client's import list moved, so its leg walked again"
    );

    vilan_core::analyzer::set_document_overlay(&client, Some(CLIENT.to_string()));
    let again = keystroke(&orphan, ORPHAN);
    assert!(is_node(again.platform), "the import is gone again");
    assert_eq!(again.roots.primary, None);
    close(&[&client, &orphan]);
}

/// A FILE created on disk moves the answer: the client imports
/// `pkg::gadget::run`, which resolves to nothing until `gadget.vl` appears —
/// and `gadget.vl` imports the widget, so the widget is the client's from then
/// on. No buffer changed; only a probe's answer did.
#[test]
fn a_file_created_on_disk_moves_the_answer() {
    let client_text = "import pkg::gadget::run;\n\nfun main() {\n\trun();\n}\n";
    let directory = package(
        "created",
        &[
            ("client.vl", client_text),
            ("server.vl", SERVER),
            ("widget.vl", ORPHAN),
        ],
    );
    let widget = directory.join("src/widget.vl");
    let before = keystroke(&widget, ORPHAN);
    assert!(is_node(before.platform), "nothing reaches the widget yet");
    std::fs::write(
        directory.join("src/gadget.vl"),
        "import pkg::widget::nobody;\n\nexport fun run(): i32 {\n\tnobody()\n}\n",
    )
    .expect("gadget.vl");
    let after = keystroke(&widget, ORPHAN);
    assert!(
        is_browser(after.platform),
        "gadget.vl now exists and imports the widget: a stale walk misses the new file"
    );
    assert!(after.walks >= 1);
    close(&[&widget]);
}

/// The manifest is read afresh on every keystroke and names the walk's key:
/// pointing the client entry at a file that imports nothing un-reaches the
/// model, and the default entry colours it.
#[test]
fn a_manifest_change_moves_the_answer() {
    let directory = package(
        "manifest",
        &[
            ("client.vl", CLIENT),
            ("server.vl", SERVER),
            ("model.vl", MODEL),
            ("bare.vl", SERVER),
        ],
    );
    let model = directory.join("src/model.vl");
    assert!(is_browser(keystroke(&model, MODEL).platform));
    std::fs::write(
        directory.join("vilan.toml"),
        MANIFEST.replace("[entry.client]\n", "[entry.client]\npath = \"bare.vl\"\n"),
    )
    .expect("the manifest");
    let after = keystroke(&model, MODEL);
    assert!(
        is_node(after.platform),
        "the client entry is bare.vl now, which reaches nothing"
    );
    close(&[&model]);
}

/// The file's own `[platform(..)]` declaration is read off the live buffer on
/// every keystroke, never remembered with the walk: declaring `node` in a
/// module the browser entry reaches moves its colour at once, with no walk.
#[test]
fn a_platform_declaration_in_the_buffer_is_seen() {
    let directory = package(
        "declared",
        &[
            ("client.vl", CLIENT),
            ("server.vl", SERVER),
            ("model.vl", MODEL),
        ],
    );
    let model = directory.join("src/model.vl");
    assert!(is_browser(keystroke(&model, MODEL).platform));
    let declared = format!("[platform(\"node\")] mod self;\n\n{MODEL}");
    let answer = keystroke(&model, &declared);
    assert!(
        is_node(answer.platform),
        "the buffer declares node, which outranks the colour"
    );
    assert_eq!(
        answer.walks, 0,
        "a declaration is not an import: nothing to walk"
    );
    assert!(
        is_browser(keystroke(&model, MODEL).platform),
        "and typed away again"
    );
    close(&[&model]);
}

/// An UNSAVED buffer is a module too (the loader's overlay): opening a new,
/// never-saved `gadget.vl` that imports the widget makes the widget the
/// client's, and closing it un-reaches it again — the probe's answer is the
/// overlay's as much as the disk's.
#[test]
fn an_unsaved_buffer_opened_or_closed_moves_the_answer() {
    let client_text = "import pkg::gadget::run;\n\nfun main() {\n\trun();\n}\n";
    let directory = package(
        "unsaved",
        &[
            ("client.vl", client_text),
            ("server.vl", SERVER),
            ("widget.vl", ORPHAN),
        ],
    );
    let widget = directory.join("src/widget.vl");
    let gadget = directory.join("src/gadget.vl");
    assert!(is_node(keystroke(&widget, ORPHAN).platform));
    vilan_core::analyzer::set_document_overlay(
        &gadget,
        Some("import pkg::widget::nobody;\n\nexport fun run(): i32 {\n\tnobody()\n}\n".into()),
    );
    assert!(
        is_browser(keystroke(&widget, ORPHAN).platform),
        "the open buffer gadget.vl imports the widget"
    );
    vilan_core::analyzer::set_document_overlay(&gadget, None);
    assert!(
        is_node(keystroke(&widget, ORPHAN).platform),
        "closed unsaved, gadget.vl is gone again"
    );
    close(&[&widget]);
}
