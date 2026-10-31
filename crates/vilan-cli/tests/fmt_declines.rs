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

/// N138: a cargo workspace's `target/` is not walked, tag or no tag. Cargo
/// writes `CACHEDIR.TAG` only into a target directory it creates, so a
/// `target/` made before the first build carries none — and the scratch
/// projects the suite leaves under `target/tmp` (unformatted, some
/// unformattable) turned the repository's `vilan fmt --check .` red. A module
/// DIRECTORY named `target` in a vilan package, with no `Cargo.toml` beside
/// it, is formatted like any other.
#[test]
fn a_cargo_target_directory_is_not_walked_and_a_module_named_target_is() {
    let dir = tree("n138", &[("main.vl", CLEAN)]);
    std::fs::write(dir.join("Cargo.toml"), "[workspace]\n").expect("write the cargo manifest");
    let scratch = dir.join("target/tmp/probe");
    std::fs::create_dir_all(&scratch).expect("create the scratch tree");
    std::fs::write(scratch.join("unformatted.vl"), "fun main(){let x=1;}\n")
        .expect("write an unformatted scratch file");
    assert!(!dir.join("target/CACHEDIR.TAG").exists());
    let output = vilan_fmt(&dir, &["--check"]);
    assert_eq!(output.status.code(), Some(0), "{}", streams(&output));
    assert!(
        !streams(&output).contains("unformatted.vl"),
        "{}",
        streams(&output)
    );

    // The same `target/` with no `Cargo.toml` beside it is a package directory.
    std::fs::remove_file(dir.join("Cargo.toml")).expect("remove the cargo manifest");
    let output = vilan_fmt(&dir, &["--check"]);
    assert_eq!(output.status.code(), Some(1), "{}", streams(&output));
    assert!(
        streams(&output).contains("unformatted.vl"),
        "{}",
        streams(&output)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// B671: kolt's task table kept a map in step with `insert`/`remove` arms in a
/// `match ... { ... };` statement. The `;` is the statement's, the tree keeps
/// no trace of it, and the reprint used to drop it: the net threw the whole
/// file's reprint away and `vilan fmt` declined (exit 2) over a file `vilan
/// check` accepts.
const TERMINATED_MATCH: &str = "import std::hash_map::HashMap;\n\nfun note(held: HashMap<i32, str>, id: i32, found: Option<str>) {\n\tmatch found {\n\t\tSome(let text) => held.insert(id, text),\n\t\tNone => held.remove(id)\n\t};\n}\n";

#[test]
fn a_match_statement_with_a_semicolon_is_formatted_not_declined() {
    let canonical = TERMINATED_MATCH;
    let messy = TERMINATED_MATCH
        .replace("\n\t\t", " ")
        .replace("\n\t};", " };");
    assert_ne!(messy, canonical);
    let dir = tree("b671", &[("main.vl", &messy)]);
    // The unformatted file is reported as unformatted (1), not declined (2).
    let output = vilan_fmt(&dir, &["--check"]);
    let text = streams(&output);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(
        text.contains("would reformat") && !text.contains("declined"),
        "{text}"
    );
    // Written, it round-trips to the canonical text, and that text is clean.
    let output = vilan_fmt(&dir, &[]);
    assert_eq!(output.status.code(), Some(0), "{}", streams(&output));
    let after = std::fs::read_to_string(dir.join("main.vl")).expect("read the file back");
    assert_eq!(after, canonical);
    let output = vilan_fmt(&dir, &["--check"]);
    assert_eq!(output.status.code(), Some(0), "{}", streams(&output));
    let _ = std::fs::remove_dir_all(&dir);
}

/// E353: a node under `// vilan-fmt: skip` keeps its written layout, and the
/// file it sits in is otherwise canonical - so `--check` passes over it (a
/// skip never fails the check) and LISTS it, `path:line: vilan-fmt: skip` at
/// the directive's line, so the workaround stays in sight of the bug it works
/// around. Write mode keeps the node and lists nothing.
const SKIPPED: &str =
    "fun main() {\n\tlet a = 1;\n\t// vilan-fmt: skip\n\tlet   b =   [1,2,\n\t\t3];\n}\n";

#[test]
fn a_skipped_node_never_fails_check_and_check_lists_it() {
    let dir = tree("e353", &[("main.vl", SKIPPED)]);
    let output = vilan_fmt(&dir, &["--check"]);
    let text = streams(&output);
    assert_eq!(output.status.code(), Some(0), "{text}");
    assert!(text.contains("main.vl:3: vilan-fmt: skip"), "{text}");
    assert!(!text.contains("would reformat"), "{text}");
    let output = vilan_fmt(&dir, &[]);
    let text = streams(&output);
    assert_eq!(output.status.code(), Some(0), "{text}");
    assert!(!text.contains("vilan-fmt: skip"), "{text}");
    let after = std::fs::read_to_string(dir.join("main.vl")).expect("read the file back");
    assert_eq!(after, SKIPPED);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The listing does not hide the rest of the file: a skipped node beside one
/// that would reformat is listed, and the file still fails the check.
#[test]
fn a_skip_beside_an_unformatted_line_is_listed_and_the_file_still_fails() {
    let messy = SKIPPED.replace("let a = 1;", "let  a  =  1;");
    let dir = tree("e353_messy", &[("main.vl", &messy)]);
    let output = vilan_fmt(&dir, &["--check"]);
    let text = streams(&output);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(
        text.contains("main.vl:3: vilan-fmt: skip") && text.contains("would reformat"),
        "{text}"
    );
}
