//! End-to-end gate for `vilan build --print-chunks` (bundle-splitting.md S1):
//! the report is analysis-only — the router example prints its route-chunk
//! plan AND still builds its artifact — and prints nothing without the flag.
//! The walkthrough example pins the MODULE case: pages defined in a sibling
//! `views.vl` module chunk exactly like entry-file pages (only std is eager
//! by residence). The plan's numbers (function counts, memberships) are
//! pinned; the byte estimates are not, so an edit to an example's page
//! bodies doesn't break these gates spuriously.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Copies `vilan/examples/<name>` into a fresh temp directory.
fn stage_example(name: &str, tag: &str) -> PathBuf {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../vilan/examples")
        .join(name);
    let staged = std::env::temp_dir().join(format!("vilan_chunks_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staged);
    copy_tree(&source, &staged);
    staged
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).expect("read example directory") {
        let entry = entry.unwrap();
        let destination = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &destination);
        } else {
            std::fs::copy(entry.path(), &destination).unwrap();
        }
    }
}

#[test]
fn the_router_example_reports_its_route_chunks_and_still_builds() {
    let staged = stage_example("router", "report");
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args([
            "build",
            staged.to_str().expect("utf-8 temp path"),
            "--print-chunks",
        ])
        .output()
        .expect("run vilan");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "build failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    // The plan, as the proposal predicts for the router example: one
    // recognized `swap` match, three route chunks, each arm's exclusively
    // reachable pages in its chunk — and the shared `items` pair split
    // nowhere (items_layout/items_list sit under `Route::Items(..)` alone).
    assert!(
        stdout.contains("1 splittable match, 3 route chunks"),
        "missing plan header:\n{stdout}"
    );
    assert!(
        stdout.contains("chunk `Route::Home`: 1 function") && stdout.contains("(home_page)"),
        "missing Home chunk:\n{stdout}"
    );
    assert!(
        stdout.contains("chunk `Route::Items(..)`: 3 functions")
            && stdout.contains("(item_detail, items_layout, items_list)"),
        "missing Items chunk:\n{stdout}"
    );
    assert!(
        stdout.contains("chunk `Route::NotFound`: 1 function")
            && stdout.contains("(not_found_page)"),
        "missing NotFound chunk:\n{stdout}"
    );

    // Analysis-only: the ordinary artifact is still written.
    assert!(
        staged.join("app.js").exists(),
        "--print-chunks must not suppress the build"
    );
    let _ = std::fs::remove_dir_all(&staged);
}

#[test]
fn module_resident_pages_chunk_like_entry_pages() {
    // The walkthrough's pages live in `views.vl`, not the entry — the shape
    // real apps use. An entry-only membership rule planned "1 splittable
    // match, 0 route chunks" here; only std is excluded by residence.
    let staged = stage_example("walkthrough", "modules");
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args([
            "build",
            staged.to_str().expect("utf-8 temp path"),
            "--print-chunks",
        ])
        .output()
        .expect("run vilan");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "build failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("1 splittable match, 3 route chunks"),
        "missing plan header:\n{stdout}"
    );
    // Each page function lands in its own arm's chunk (helpers ride along;
    // the exact helper sets may evolve with the example, the pages may not).
    for (chunk, page) in [
        ("chunk `Route::Home`", "home_page"),
        ("chunk `Route::Note(..)`", "note_page"),
        ("chunk `Route::NotFound`", "not_found_page"),
    ] {
        let line = stdout
            .lines()
            .find(|line| line.contains(chunk))
            .unwrap_or_else(|| panic!("missing {chunk}:\n{stdout}"));
        assert!(line.contains(page), "{page} not in its chunk: {line}");
    }
    let _ = std::fs::remove_dir_all(&staged);
}

#[test]
fn without_the_flag_the_report_is_absent() {
    let staged = stage_example("router", "silent");
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", staged.to_str().expect("utf-8 temp path")])
        .output()
        .expect("run vilan");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "build failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !stdout.contains("[vilan chunks]"),
        "report printed without --print-chunks:\n{stdout}"
    );
    let _ = std::fs::remove_dir_all(&staged);
}
