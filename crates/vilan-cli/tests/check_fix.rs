//! `vilan check --fix` (I5, `proposal/index-type.md` §8.1): the numeric
//! mismatches' fixes applied to a package to a fixed point, then the ordinary
//! check. The edits themselves are `vilan_ide::numeric_fix`'s, the function
//! the language server's quick fixes call, and are pinned there; these pins
//! hold the DRIVER — the rounds, what it may edit, what it leaves, and the
//! exit code.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

mod support;

/// A fresh single-package project holding `source` as its entry.
fn temp_package(tag: &str, source: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = support::scratch_root().join(format!(
        "vilan_check_fix_{tag}_{}_{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("vilan.toml"), "[package]\nname = \"app\"\n").unwrap();
    std::fs::write(dir.join("src/main.vl"), source).unwrap();
    dir
}

fn vilan(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vilan"))
        .current_dir(dir)
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .expect("run vilan")
}

fn entry(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("src/main.vl")).unwrap()
}

const TAKE: &str = concat!(
    "fun take(xs: List<str>, at: usize): str {\n",
    "\txs[at]\n",
    "}\n",
    "\n",
);

#[test]
fn every_steered_mismatch_is_converted_and_the_check_then_passes() {
    let dir = temp_package(
        "convert",
        &format!(
            "{TAKE}{}",
            concat!(
                "fun main() {\n",
                "\tlet xs = [\"a\", \"b\", \"c\"];\n",
                "\tlet width: i32 = 2;\n",
                "\tprint(take(xs, width + 0));\n",
                "\tlet at: usize = 1;\n",
                "\tlet wide: i32 = at;\n",
                "\tprint(i\"{width < at} {wide}\");\n",
                "}\n",
            )
        ),
    );
    assert!(
        !vilan(&dir, &["check", "."]).status.success(),
        "the program is refused before the fix, so the pin is not vacuous"
    );
    let output = vilan(&dir, &["check", "--fix", "."]);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let text = entry(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        output.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("fixed 3 numeric mismatches in 1 file"),
        "{stdout}"
    );
    assert!(stdout.contains("no errors"), "{stdout}");
    // The argument: the whole value, parenthesized. The annotation: the name.
    // The comparison: the NON-index operand converts to `usize`.
    assert!(
        text.contains("print(take(xs, (width + 0).as_usize()));"),
        "{text}"
    );
    assert!(text.contains("let wide: i32 = at.as_i32();"), "{text}");
    assert!(text.contains("{width.as_usize() < at}"), "{text}");
}

#[test]
fn a_literal_counter_is_declared_usize_and_its_other_uses_convert_in_later_rounds() {
    let dir = temp_package(
        "declare",
        &format!(
            "{TAKE}{}",
            concat!(
                "fun step(value: i32): i32 {\n",
                "\tvalue + 1\n",
                "}\n",
                "\n",
                "fun main() {\n",
                "\tlet xs = [\"a\", \"b\", \"c\"];\n",
                "\tmut at = 0;\n",
                "\tat = step(at);\n",
                "\tprint(take(xs, at));\n",
                "}\n",
            )
        ),
    );
    let output = vilan(&dir, &["check", "--fix", "."]);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let text = entry(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(output.status.success(), "{stdout}");
    // Round one declares the counter; the declaration then moves the
    // mismatch to `step`'s `i32` world, and rounds two and three convert at
    // its boundary — never a `.as_usize()` on `at` itself.
    assert!(text.contains("\tmut at: usize = 0;\n"), "{text}");
    assert!(
        text.contains("\tat = step(at.as_i32()).as_usize();\n"),
        "{text}"
    );
    assert!(!text.contains("at.as_usize()"), "{text}");
}

#[test]
fn what_needs_a_person_is_left_and_reported_with_a_failing_exit() {
    // A `str` is not a number: no conversion names it, so the fix pass
    // leaves the line alone and the check that follows reports it.
    let dir = temp_package(
        "residue",
        &format!(
            "{TAKE}{}",
            concat!(
                "fun main() {\n",
                "\tlet xs = [\"a\", \"b\"];\n",
                "\tlet width: i32 = 1;\n",
                "\tprint(take(xs, width));\n",
                "\tlet label: str = width;\n",
                "\tprint(label);\n",
                "}\n",
            )
        ),
    );
    let output = vilan(&dir, &["check", "--fix", "."]);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let text = entry(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!output.status.success(), "{stdout}{stderr}");
    assert!(
        stdout.contains("fixed 1 numeric mismatch in 1 file"),
        "{stdout}"
    );
    assert!(
        text.contains("print(take(xs, width.as_usize()));"),
        "{text}"
    );
    assert!(text.contains("let label: str = width;"), "{text}");
    assert!(
        stderr.contains("Expected str, but got i32 instead."),
        "{stderr}"
    );
}

#[test]
fn a_clean_package_is_left_byte_for_byte() {
    let source = "fun main() {\n\tlet xs = [1, 2];\n\tprint(i\"{xs[0]}\");\n}\n";
    let dir = temp_package("clean", source);
    let output = vilan(&dir, &["check", "--fix", "."]);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let text = entry(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout.contains("fixed 0 numeric mismatches in 0 files"),
        "{stdout}"
    );
    assert_eq!(text, source);
}

#[test]
fn fix_and_watch_are_refused_together() {
    let dir = temp_package("watch", "fun main() {}\n");
    let output = vilan(&dir, &["check", "--fix", "--watch", "."]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("cannot be used with"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
