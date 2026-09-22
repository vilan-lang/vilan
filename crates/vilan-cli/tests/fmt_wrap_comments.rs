//! E205: `[fmt] wrap_comments` end to end, through the `vilan` binary.
//!
//! The rules and the filler are pinned inside `vilan-core`
//! (`formatter::comment_reflow`, `formatter::comment_wrapping`). What is under
//! test here is the half only the CLI can answer: that the key is read from
//! the package's own `vilan.toml`, that the DEFAULT — no key at all — leaves a
//! long comment exactly where its author left it, and that the nearest
//! manifest wins so a workspace can set it once and a member turn it off.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

/// A fresh scratch tree under the target directory rather than the system temp
/// (tracker N82/N86: `/tmp` is one tmpfs shared by every worktree, and a suite
/// run under lane load filled it). The staging directory carries the process
/// id so two binaries cannot collide.
fn tree(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("e205_{tag}_{}_{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (name, contents) in files {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("create the scratch tree");
        std::fs::write(&path, contents).expect("write a fixture file");
    }
    dir
}

fn vilan_fmt(dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vilan"))
        .current_dir(dir)
        .env("NO_COLOR", "1")
        .arg("fmt")
        .arg(".")
        .output()
        .expect("run vilan fmt")
}

fn read(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name)).expect("read the formatted file")
}

/// A manifest with no `[fmt]` section at all — the shape every package in the
/// estate has today.
const SILENT: &str = "[package]\nname = \"wrapprobe\"\n";

/// The same, opted in.
const OPTED_IN: &str = "[package]\nname = \"wrapprobe\"\n\n[fmt]\nwrap_comments = true\n";

/// A canonical program whose one comment runs well past the budget.
const LONG_COMMENT: &str = "// the formatter has laid code out to a width since the day it existed and left every comment exactly as typed\nfun main() {}\n";

/// What the fill answers for [`LONG_COMMENT`] at the top level.
const WRAPPED: &str = "// the formatter has laid code out to a width since the day it existed and left every comment\n// exactly as typed\nfun main() {}\n";

#[test]
fn the_default_leaves_a_long_comment_exactly_as_written() {
    // The promise the opt-in rests on: a package that says nothing gets the
    // formatter it had, byte for byte, and `fmt` reports the tree clean.
    let dir = tree(
        "default",
        &[("vilan.toml", SILENT), ("src/main.vl", LONG_COMMENT)],
    );
    let output = vilan_fmt(&dir);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(read(&dir, "src/main.vl"), LONG_COMMENT);
}

#[test]
fn the_key_wraps_the_comment_and_the_run_is_idempotent() {
    let dir = tree(
        "opted-in",
        &[("vilan.toml", OPTED_IN), ("src/main.vl", LONG_COMMENT)],
    );
    let first = vilan_fmt(&dir);
    assert!(first.status.success(), "{first:?}");
    assert_eq!(read(&dir, "src/main.vl"), WRAPPED);
    // A second run must find nothing to do — the exit code says so, not just
    // the bytes.
    let second = vilan_fmt(&dir);
    assert!(second.status.success(), "{second:?}");
    assert_eq!(read(&dir, "src/main.vl"), WRAPPED);
    assert!(
        String::from_utf8_lossy(&second.stdout).is_empty(),
        "a formatted tree is silent: {second:?}"
    );
}

#[test]
fn false_is_read_as_false_and_not_as_absent() {
    // The distinction the climb turns on: a manifest that says `false` has an
    // opinion, and the search must stop at it rather than keep climbing.
    let declared = "[package]\nname = \"wrapprobe\"\n\n[fmt]\nwrap_comments = false\n";
    let dir = tree(
        "declared-off",
        &[("vilan.toml", declared), ("src/main.vl", LONG_COMMENT)],
    );
    assert!(vilan_fmt(&dir).status.success());
    assert_eq!(read(&dir, "src/main.vl"), LONG_COMMENT);
}

#[test]
fn the_nearest_manifest_wins_over_the_workspace_above_it() {
    // A workspace opts in; one member opts out. Each member is formatted by
    // its own answer, in ONE run over the whole tree.
    let workspace =
        "[project]\npackages = [\"wrapped\", \"kept\"]\n\n[fmt]\nwrap_comments = true\n";
    let dir = tree(
        "workspace",
        &[
            ("vilan.toml", workspace),
            ("wrapped/vilan.toml", "[package]\nname = \"wrapped\"\n"),
            ("wrapped/src/main.vl", LONG_COMMENT),
            (
                "kept/vilan.toml",
                "[package]\nname = \"kept\"\n\n[fmt]\nwrap_comments = false\n",
            ),
            ("kept/src/main.vl", LONG_COMMENT),
        ],
    );
    assert!(vilan_fmt(&dir).status.success());
    assert_eq!(read(&dir, "wrapped/src/main.vl"), WRAPPED);
    assert_eq!(read(&dir, "kept/src/main.vl"), LONG_COMMENT);
}

// --- E215: `[fmt] comment_width` ----------------------------------------
// The key the default flip waits on. E205 filled to the CODE width, which is
// a width nobody chose for prose: std writes its comments to ~84 columns, so
// opting in at 100 would have moved 6,719 comment lines. These pin the half
// only the CLI can answer — the key reaching the printer from a manifest, and
// each key taking its OWN nearest declaration.

/// Opted in at the width std opts in at (E215's R2).
const AT_84: &str =
    "[package]\nname = \"wrapprobe\"\n\n[fmt]\nwrap_comments = true\ncomment_width = 84\n";

/// What the fill answers for [`LONG_COMMENT`] at 84 columns — narrower than
/// [`WRAPPED`], which is the point of the key.
const WRAPPED_AT_84: &str = "// the formatter has laid code out to a width since the day it existed and left\n// every comment exactly as typed\nfun main() {}\n";

#[test]
fn a_width_other_than_the_code_width_is_the_one_the_fill_uses() {
    let dir = tree(
        "width-84",
        &[("vilan.toml", AT_84), ("src/main.vl", LONG_COMMENT)],
    );
    let first = vilan_fmt(&dir);
    assert!(first.status.success(), "{first:?}");
    assert_eq!(read(&dir, "src/main.vl"), WRAPPED_AT_84);
    assert_ne!(
        WRAPPED_AT_84, WRAPPED,
        "the declared width must differ from the code width's answer"
    );
    for line in WRAPPED_AT_84.lines().filter(|line| line.starts_with("//")) {
        assert!(
            line.chars().count() <= 84,
            "over the declared width: {line}"
        );
    }
    let second = vilan_fmt(&dir);
    assert!(second.status.success(), "{second:?}");
    assert_eq!(read(&dir, "src/main.vl"), WRAPPED_AT_84);
}

#[test]
fn the_width_alone_wraps_nothing() {
    // `comment_width` is read only when `wrap_comments` is on: a package that
    // declares a width and nothing else keeps today's formatter exactly.
    let width_only = "[package]\nname = \"wrapprobe\"\n\n[fmt]\ncomment_width = 84\n";
    let dir = tree(
        "width-only",
        &[("vilan.toml", width_only), ("src/main.vl", LONG_COMMENT)],
    );
    assert!(vilan_fmt(&dir).status.success());
    assert_eq!(read(&dir, "src/main.vl"), LONG_COMMENT);
}

#[test]
fn each_key_takes_its_own_nearest_declaration() {
    // A workspace sets the width once; one member turns the wrapping on and
    // one leaves it off. The member that wraps must do it at the WORKSPACE's
    // width — the climb resolves the two keys independently, so a member
    // declaring one of them does not shadow the other.
    let workspace = "[project]\npackages = [\"wrapped\", \"kept\"]\n\n[fmt]\ncomment_width = 84\n";
    let dir = tree(
        "split-keys",
        &[
            ("vilan.toml", workspace),
            (
                "wrapped/vilan.toml",
                "[package]\nname = \"wrapped\"\n\n[fmt]\nwrap_comments = true\n",
            ),
            ("wrapped/src/main.vl", LONG_COMMENT),
            ("kept/vilan.toml", "[package]\nname = \"kept\"\n"),
            ("kept/src/main.vl", LONG_COMMENT),
        ],
    );
    assert!(vilan_fmt(&dir).status.success());
    assert_eq!(read(&dir, "wrapped/src/main.vl"), WRAPPED_AT_84);
    assert_eq!(read(&dir, "kept/src/main.vl"), LONG_COMMENT);
}
