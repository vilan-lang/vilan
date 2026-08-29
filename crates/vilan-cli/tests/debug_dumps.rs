//! End-to-end gate for the `-d` / `--debug` dumps (backlog E99).
//!
//! `.parse.out` has always been the POST-desugar tree — `css::rewrite_items`,
//! `elements::rewrite_items` and `lift::rewrite_items` run at every parse entry,
//! so the tree analysis receives is not the tree the parser produced, and until
//! E99 no dump showed the latter. `.parse-raw.out` is that missing stage, and
//! the pair brackets the desugars: what these tests hold is the DIFFERENCE
//! between them, because a dump that showed the same tree twice would be worth
//! nothing.

use std::path::PathBuf;
use std::process::Command;

/// Writes a one-package project into a fresh temp directory and returns its
/// `src/` (where the dumps land — `write_debug` puts them beside the source).
fn stage_project(tag: &str, entry: &str) -> PathBuf {
    let staged = std::env::temp_dir().join(format!("vilan_dumps_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staged);
    let source = staged.join("src");
    std::fs::create_dir_all(&source).expect("create staged project");
    std::fs::write(
        staged.join("vilan.toml"),
        "[package]\nname = \"dumps\"\nversion = \"0.1.0\"\n",
    )
    .expect("write manifest");
    std::fs::write(source.join("main.vl"), entry).expect("write entry");
    source
}

fn build(project_source: &PathBuf, arguments: &[&str]) -> String {
    let project = project_source.parent().expect("project root");
    let mut command = Command::new(env!("CARGO_BIN_EXE_vilan"));
    command
        .arg("build")
        .arg(project.to_str().expect("utf-8 path"));
    command.args(arguments);
    let output = command.output().expect("run vilan");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "build failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
}

/// A `?` in an expression — the desugar `lift::rewrite_items` performs, and the
/// one whose nodes E99's probe found in `.parse.out`.
const LIFTING_ENTRY: &str = r#"import std::print;
import std::option::Option::{ self, Some, None };

fun pick(): Option<i32> {
	Some(3)
}

fun main() {
	let doubled: Option<i32> = pick()? * 2;
	print(doubled.unwrap_or(-1));
}
"#;

// The filed probe, turned around: the raw dump shows the `?` as the parser
// produced it, and the post-desugar dump shows the lift region the rewrite put
// there. Neither claim holds without the other — one dump alone cannot say
// which side of the desugars a node came from.
#[test]
fn the_raw_parse_dump_predates_the_lift_rewrite_and_parse_out_does_not() {
    let source = stage_project("lift", LIFTING_ENTRY);
    build(&source, &["-d"]);

    let raw = std::fs::read_to_string(source.join("main.parse-raw.out"))
        .expect("the raw parse dump is written");
    let desugared =
        std::fs::read_to_string(source.join("main.parse.out")).expect("the parse dump is written");

    assert!(
        !raw.contains("LiftRegion") && !raw.contains("LiftHole"),
        "the raw dump must predate `lift::rewrite_items`, but it carries its nodes:\n{raw}"
    );
    assert!(
        desugared.contains("LiftRegion") && desugared.contains("LiftHole"),
        "`.parse.out` is the POST-desugar tree and must carry the lift nodes:\n{desugared}"
    );
    // Both describe the same program, so the difference is the desugar's doing
    // and nothing else: the raw dump is not empty or truncated.
    assert!(
        raw.contains("pick") && desugared.contains("pick"),
        "both dumps describe the same program"
    );
}

// The dumps stay opt-in: nothing is written without the flag, so a stale
// `parse-raw.out` from an earlier `-d` round is never mistaken for this build's.
#[test]
fn no_dump_is_written_without_the_flag() {
    let source = stage_project("nodump", LIFTING_ENTRY);
    build(&source, &[]);

    for extension in ["parse-raw.out", "parse.out", "analyze.out", "callgraph.out"] {
        let path = source.join(format!("main.{extension}"));
        assert!(
            !path.exists(),
            "{} was written without `-d`",
            path.display()
        );
    }
}

// The flag's help NAMES its stages — E99's other half. A dump nobody can find
// is the same defect as a dump that does not exist, and `-d`'s help used to
// list three files with no word about what any of them held.
#[test]
fn the_debug_flag_help_names_every_stage_it_dumps() {
    for subcommand in ["build", "check"] {
        let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
            .args([subcommand, "--help"])
            .output()
            .expect("run vilan");
        let help = String::from_utf8_lossy(&output.stdout);
        for stage in [
            ".parse-raw.out",
            ".parse.out",
            ".analyze.out",
            ".callgraph.out",
        ] {
            assert!(
                help.contains(stage),
                "`vilan {subcommand} --help` does not name {stage}:\n{help}"
            );
        }
    }
}
