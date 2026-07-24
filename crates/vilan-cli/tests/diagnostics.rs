//! End-to-end pins for how the CLI *delivers* a diagnostic
//! (`windows-support.md` §6): which stream it lands on, and whether it colors.
//!
//! Both properties were wrong before this slice and neither is expressible
//! in-process: ariadne rendered with color unconditionally (the audit's probe
//! found 7 ANSI escapes in a redirected file under `NO_COLOR=1`, contradicting
//! `paint.rs`'s per-stream contract), and errors went to **stdout** while
//! warnings had already moved to stderr precisely so they could not corrupt
//! `build --stdout`'s JavaScript. Errors join them (ratified call (f)).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

/// A fresh temp directory holding one test's single-package project.
fn temp_package(tag: &str, source: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "vilan_diagnostics_{tag}_{}_{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("vilan.toml"), "[package]\nname = \"app\"\n").unwrap();
    std::fs::write(dir.join("src/main.vl"), source).unwrap();
    dir
}

/// Runs the `vilan` binary in `dir` with both streams **piped** — never a
/// terminal, which is exactly the redirected-output case the color gate is
/// about.
fn vilan(dir: &Path, args: &[&str], no_color: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_vilan"));
    command.current_dir(dir).args(args);
    if no_color {
        command.env("NO_COLOR", "1");
    } else {
        command.env_remove("NO_COLOR");
    }
    command.output().expect("run vilan")
}

/// A program that cannot parse — the shortest reliable diagnostic.
const BROKEN: &str = "fun main( {\n";

/// A program that compiles but warns: an unbound `[must_use]` subscription. It
/// is the corruption case with teeth — stdout carries real JavaScript *and* a
/// diagnostic exists, so "the diagnostic is not in the JS" is a live claim.
const WARNING: &str = "import std::reactive::{ Signal, Source };\n\
                       \n\
                       fun main() {\n\
                       \tlet count = Signal::new(0);\n\
                       \tcount.sub(|value| {});\n\
                       }\n";

/// The audit's probe, turned into a gate: a compile error with `NO_COLOR=1` and
/// both streams redirected must write **zero** ANSI escapes anywhere. Before
/// this slice ariadne ignored the gate entirely and wrote them into the file.
#[test]
fn a_redirected_no_color_diagnostic_carries_no_ansi_escapes() {
    let dir = temp_package("no_color", BROKEN);
    let output = vilan(&dir, &["build", "."], true);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(!output.status.success(), "the broken build must fail");
    assert!(
        !output.stdout.contains(&0x1b),
        "stdout must be escape-free: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        !output.stderr.contains(&0x1b),
        "stderr must be escape-free: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The same, without `NO_COLOR`: a *piped* stream is not a terminal, so the
/// other half of the gate must suppress color on its own. (This is the shape
/// every e2e test in the suite reads, which is why none of them changed.)
#[test]
fn a_piped_diagnostic_carries_no_ansi_escapes_even_without_no_color() {
    let dir = temp_package("piped", BROKEN);
    let output = vilan(&dir, &["build", "."], false);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(!output.status.success(), "the broken build must fail");
    assert!(
        !output.stdout.contains(&0x1b) && !output.stderr.contains(&0x1b),
        "a pipe is not a terminal, so nothing may color:\nstdout: {:?}\nstderr: {:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Ratified call (f): a failing build's diagnostic is on **stderr**, and stdout
/// stays empty.
#[test]
fn a_failing_builds_diagnostic_goes_to_stderr() {
    let dir = temp_package("stderr", BROKEN);
    let output = vilan(&dir, &["build", "."], true);
    let _ = std::fs::remove_dir_all(&dir);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "the broken build must fail");
    assert!(
        stderr.contains("Error:") && stderr.contains("expected") && stderr.contains("main.vl"),
        "stderr carries the ariadne header, the message and the file: {stderr}"
    );
    assert!(
        stdout.is_empty(),
        "a failing build writes nothing to stdout: {stdout}"
    );
}

/// `build --stdout` purity, failing case: stdout is the JavaScript channel, so
/// a build that produces none must produce *nothing* — no header, no message,
/// no file name.
#[test]
fn build_stdout_of_a_failing_program_carries_no_diagnostic() {
    let dir = temp_package("stdout_broken", BROKEN);
    let output = vilan(&dir, &["build", ".", "--stdout"], true);
    let _ = std::fs::remove_dir_all(&dir);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!output.status.success(), "the broken build must fail");
    assert!(
        !stdout.contains("Error:") && !stdout.contains("expected") && !stdout.contains("main.vl"),
        "no diagnostic text may reach the JavaScript channel: {stdout}"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Error:"),
        "...and it is on stderr instead"
    );
}

/// `build --stdout` purity, succeeding-with-a-warning case: real JavaScript on
/// stdout, the diagnostic on stderr, and the two never mix. This is the exact
/// scenario the warnings were moved for; the errors now share it.
#[test]
fn build_stdout_javascript_is_never_mixed_with_a_diagnostic() {
    let dir = temp_package("stdout_warning", WARNING);
    let output = vilan(&dir, &["build", ".", "--stdout"], true);
    let _ = std::fs::remove_dir_all(&dir);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "the program compiles: {stderr}");
    assert!(
        stdout.contains("const count"),
        "stdout is the emitted JavaScript: {stdout}"
    );
    assert!(
        !stdout.contains("Warning:") && !stdout.contains("must_use"),
        "the warning must not be spliced into the JavaScript: {stdout}"
    );
    assert!(
        stderr.contains("Warning:") && stderr.contains("must_use"),
        "the warning is on stderr: {stderr}"
    );
}
