//! N90: `vilan fmt --check` reports a file it could not format as a DISTINCT
//! non-zero outcome, naming the file and what the formatter met.
//!
//! `formatter::format` answers the original bytes on every way out — a source
//! that does not lex, one that does not parse, a construct the printer has no
//! rule for, a reprint its own safety net threw away — so `fmt` compared its
//! answer to the file, found them equal, and called the file already-formatted.
//! `export let x = 1;` rode that silence through a whole order with
//! `vilan fmt --check vilan/std` green over it, and `[deprecated(..)]` did the
//! same; each was caught by an idempotency pin on ONE file rather than by the
//! gate that walks the tree.
//!
//! Three outcomes now, three codes: `0` clean, `1` the tree is not formatted
//! (`--check` found files that would reformat, or a write failed), `2` the
//! formatter could not format a file at all. `2` outranks `1`, because a run
//! that met a file it could not format has established nothing about the rest.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

/// A fresh scratch tree under the target directory rather than the system temp
/// (tracker N82: `/tmp` is one tmpfs shared by every worktree, and a suite run
/// under lane load filled it).
fn tree(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("n90_{tag}_{}_{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create the scratch tree");
    std::fs::write(dir.join("vilan.toml"), "[package]\nname = \"fmtprobe\"\n")
        .expect("write the manifest");
    for (name, contents) in files {
        std::fs::write(dir.join(name), contents).expect("write a fixture file");
    }
    dir
}

fn vilan_fmt(dir: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vilan"))
        .current_dir(dir)
        .env("NO_COLOR", "1")
        .arg("fmt")
        .args(arguments)
        .arg(".")
        .output()
        .expect("run vilan fmt")
}

fn streams(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The canonical spelling of the probe program, so "clean" means clean.
const CLEAN: &str = "fun main() {\n\tlet x = 1;\n}\n";

/// The same program with the spacing a reprint canonicalizes away.
const UNFORMATTED: &str = "fun main() {\n\tlet  x  =  1;\n}\n";

/// A file the formatter cannot format at all.
const UNPARSEABLE: &str = "fun main() {\n\tlet x = ;\n}\n";

#[test]
fn a_clean_tree_is_silent_and_exits_zero() {
    let dir = tree("clean", &[("main.vl", CLEAN)]);
    let output = vilan_fmt(&dir, &["--check"]);
    let text = streams(&output);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(output.status.code(), Some(0), "{text}");
    assert!(
        !text.contains("declined"),
        "nothing declined, so nothing is reported: {text}"
    );
}

#[test]
fn an_unformatted_tree_still_exits_one() {
    // The code `--check` already spends, unchanged: this is the tree's fault,
    // not the formatter's, and the two must stay tellable apart.
    let dir = tree("unformatted", &[("main.vl", UNFORMATTED)]);
    let output = vilan_fmt(&dir, &["--check"]);
    let text = streams(&output);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(text.contains("would reformat"), "{text}");
    assert!(!text.contains("declined"), "{text}");
}

#[test]
fn a_file_the_formatter_cannot_format_is_named_and_exits_two() {
    let dir = tree("declined", &[("main.vl", UNPARSEABLE)]);
    let output = vilan_fmt(&dir, &["--check"]);
    let text = streams(&output);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        output.status.code(),
        Some(2),
        "a file that did not format is its own outcome, not `1` and not `0`: {text}"
    );
    assert!(
        text.contains("declined") && text.contains("main.vl"),
        "the file is named: {text}"
    );
    assert!(
        text.contains("it does not parse"),
        "and so is what the formatter met: {text}"
    );
    assert!(
        !text.contains("would reformat"),
        "a file that did not format did not 'would reformat': {text}"
    );
}

#[test]
fn the_same_file_is_reported_in_write_mode_too() {
    // The silence was in both modes: `vilan fmt` wrote nothing for this file
    // and said nothing about it either.
    let dir = tree("write", &[("main.vl", UNPARSEABLE)]);
    let output = vilan_fmt(&dir, &[]);
    let text = streams(&output);
    let after = std::fs::read_to_string(dir.join("main.vl")).expect("read the fixture back");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(output.status.code(), Some(2), "{text}");
    assert!(
        text.contains("declined") && text.contains("main.vl"),
        "{text}"
    );
    assert_eq!(
        after, UNPARSEABLE,
        "a file the formatter declined is left exactly as it was"
    );
}

#[test]
fn a_decline_outranks_an_unformatted_neighbour() {
    // Both facts are reported, and the EXIT CODE is the one that says the
    // formatter could not do its job: `--check`'s green means "this tree is
    // formatted", and a file nobody could format makes that unanswerable.
    let dir = tree(
        "both",
        &[("broken.vl", UNPARSEABLE), ("messy.vl", UNFORMATTED)],
    );
    let output = vilan_fmt(&dir, &["--check"]);
    let text = streams(&output);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(output.status.code(), Some(2), "{text}");
    assert!(
        text.contains("declined") && text.contains("broken.vl"),
        "{text}"
    );
    assert!(
        text.contains("would reformat") && text.contains("messy.vl"),
        "the unformatted neighbour is still reported: {text}"
    );
}
