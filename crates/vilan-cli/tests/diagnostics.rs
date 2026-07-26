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

// --- Which FILE a diagnostic renders against (backlog E16) ------------------
//
// A diagnostic renders against the text of the source its span indexes into.
// Before this, every analyzer diagnostic was rendered against the ENTRY's text
// with the entry's name: a module's error printed a label over an arbitrary
// token of `main.vl`, and — when the drifted offset landed mid-codepoint, which
// CRLF entries make easy — ariadne panicked and took the compiler thread down.
// Only an e2e run sees the rendered output, so these live here.

/// A fresh temp project of several files (`relative path`, contents) — the
/// multi-file half: which file a diagnostic renders in is only observable
/// across two of them.
fn temp_files(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "vilan_diagnostics_{tag}_{}_{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    for (relative, contents) in files {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    dir
}

/// The manifest every multi-file fixture here uses.
const MANIFEST: &str = "[package]\nname = \"app\"\n";

/// The rendered diagnostics of a failing build, as one string.
fn build_stderr(dir: &Path) -> (Output, String) {
    let output = vilan(dir, &["build", "."], true);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (output, stderr)
}

/// The ariadne location line names the file a span belongs to; assert on the
/// file NAME (the path is a temp dir) and on the quoted source line, which is
/// what proves the offsets were applied to the right text.
fn renders_in(stderr: &str, file: &str, quoted_line: &str) -> bool {
    stderr.contains(file) && stderr.contains(quoted_line)
}

#[test]
fn a_module_diagnostic_renders_in_the_module_file() {
    // A plain type error inside an imported module. Before E16 this printed
    // `main.vl` and quoted whatever `main.vl` happened to hold at those offsets.
    let dir = temp_files(
        "module_error",
        &[
            ("vilan.toml", MANIFEST),
            (
                "src/main.vl",
                "import std::print;\nimport pkg::alpha::value;\n\nfun main() {\n\tprint(value());\n}\n",
            ),
            (
                "src/alpha.vl",
                "fun value(): str {\n\tlet x: i32 = \"not an int\";\n\t\"ok\"\n}\n",
            ),
        ],
    );
    let (output, stderr) = build_stderr(&dir);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(!output.status.success(), "the broken build must fail");
    assert!(
        renders_in(&stderr, "alpha.vl", "let x: i32 = \"not an int\";"),
        "the module's error renders in the module, quoting ITS line: {stderr}"
    );
    assert!(
        !stderr.contains("print(value());"),
        "and never quotes the entry's text at the module's offsets: {stderr}"
    );
}

#[test]
fn a_crlf_entry_does_not_make_a_cross_source_span_fatal() {
    // The fatal half of E16. The module's span (into an LF file) indexes the
    // CRLF entry's text mid-codepoint, where ariadne panics with "byte index N
    // is not a char boundary" — the compiler thread dies and the process exits
    // 101. The entry's comment is a run of two-byte `é`s starting at an odd
    // offset, so the module's span (33..39, the `"nope"` literal in `alpha.vl`)
    // lands INSIDE a character rather than between two.
    let entry = format!(
        "//  {}\r\nimport std::print;\r\nimport pkg::alpha::value;\r\n\r\nfun main() {{\r\n\tprint(value());\r\n}}\r\n",
        "é".repeat(40)
    );
    let dir = temp_files(
        "crlf_cross_source",
        &[
            ("vilan.toml", MANIFEST),
            ("src/main.vl", &entry),
            (
                "src/alpha.vl",
                "fun value(): str {\n\tlet x: i32 = \"nope\";\n\t\"ok\"\n}\n",
            ),
        ],
    );
    let (output, stderr) = build_stderr(&dir);
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(
        output.status.code(),
        Some(1),
        "a failed build exits 1; 101 is the compiler thread panicking: {stderr}"
    );
    assert!(
        !stderr.contains("panicked") && !stderr.contains("char boundary"),
        "nothing may panic: {stderr}"
    );
    assert!(
        renders_in(&stderr, "alpha.vl", "let x: i32 = \"nope\";"),
        "and it renders in the module it belongs to: {stderr}"
    );
}

#[test]
fn an_entry_file_diagnostic_still_renders_against_the_entry() {
    // The no-churn half: single-file diagnostics are the common case and must
    // render exactly as before — the entry's name, the entry's line.
    let dir = temp_files(
        "entry_error",
        &[
            ("vilan.toml", MANIFEST),
            (
                "src/main.vl",
                "import std::print;\n\nfun main() {\n\tlet x: i32 = \"not an int\";\n\tprint(x);\n}\n",
            ),
        ],
    );
    let (output, stderr) = build_stderr(&dir);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(!output.status.success(), "the broken build must fail");
    assert!(
        renders_in(&stderr, "main.vl", "let x: i32 = \"not an int\";"),
        "the entry's error renders in the entry: {stderr}"
    );
}

#[test]
fn a_post_analysis_pass_diagnostic_renders_in_the_module_file() {
    // The passes that run AFTER `analyze()` (here the `const` pass) attribute
    // per anchor entity, not "the file being walked" — they walk the whole
    // program, so there is no such file.
    let dir = temp_files(
        "module_const",
        &[
            ("vilan.toml", MANIFEST),
            (
                "src/main.vl",
                "import std::print;\nimport pkg::alpha::N;\n\nfun main() {\n\tprint(N);\n}\n",
            ),
            (
                "src/alpha.vl",
                "mut counter: i32 = 0;\n\nfun bump(): i32 {\n\tcounter = counter + 1;\n\tcounter\n}\n\nlet N = const bump();\n",
            ),
        ],
    );
    let (output, stderr) = build_stderr(&dir);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(!output.status.success(), "the broken build must fail");
    assert!(
        stderr.contains("compile-time-known"),
        "the const failure is reported: {stderr}"
    );
    assert!(
        renders_in(&stderr, "alpha.vl", "let N = const bump();"),
        "and renders in the module that holds the `const`: {stderr}"
    );
}

#[test]
fn a_platform_violation_renders_in_the_file_that_holds_the_call() {
    // The platform checker anchors at the deepest USER-CODE call site on the
    // chain, which is regularly in a module — the anchor's file travels with it.
    let dir = temp_files(
        "module_platform",
        &[
            (
                "vilan.toml",
                "[package]\nname = \"app\"\ntarget = \"browser\"\n",
            ),
            (
                "src/main.vl",
                "import pkg::alpha::go;\n\nfun main() {\n\tlet found = go();\n}\n",
            ),
            (
                "src/alpha.vl",
                "import std::fs::exists;\n\nfun go(): bool {\n\texists(\"cache.txt\")\n}\n",
            ),
        ],
    );
    let (output, stderr) = build_stderr(&dir);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(!output.status.success(), "the broken build must fail");
    assert!(
        stderr.contains("cannot run on `browser`"),
        "the platform violation is reported: {stderr}"
    );
    assert!(
        renders_in(&stderr, "alpha.vl", "exists(\"cache.txt\")"),
        "and renders at the call, in the module holding it: {stderr}"
    );
}

#[test]
fn a_module_warning_renders_in_the_module_file() {
    // Warnings carry the same attribution as errors: an unused `[must_use]`
    // result inside a module is quoted from the module's own text.
    let dir = temp_files(
        "module_warning",
        &[
            ("vilan.toml", MANIFEST),
            (
                "src/main.vl",
                "import pkg::alpha::watch;\n\nfun main() {\n\twatch();\n}\n",
            ),
            (
                "src/alpha.vl",
                "import std::reactive::{ Signal, Source };\n\nfun watch() {\n\tlet count = Signal::new(0);\n\tcount.sub(|value| {});\n}\n",
            ),
        ],
    );
    let (output, stderr) = build_stderr(&dir);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(output.status.success(), "a warning is not fatal: {stderr}");
    assert!(
        stderr.contains("Warning:") && stderr.contains("must_use"),
        "the warning is reported: {stderr}"
    );
    assert!(
        renders_in(&stderr, "alpha.vl", "count.sub(|value| {});"),
        "and renders in the module that holds the call: {stderr}"
    );
}

#[test]
fn a_macro_registration_diagnostic_renders_in_the_file_that_defines_the_macro() {
    // E16's original repro (`macros.rs`): a std file that defines a macro, with
    // no `macro_std` beside `std`. The error's span belongs to the STD file, and
    // before this it was rendered against the entry — whose text is far shorter,
    // so the label silently vanished and the message printed location-less.
    let dir = temp_files(
        "macro_std_missing",
        &[
            ("vilan.toml", MANIFEST),
            (
                "src/main.vl",
                "import std::mine::Marker;\n\nfun main() {\n}\n",
            ),
            (
                // A minimal stand-in std: `VILAN_STD` may name a bare source
                // root. Its grandparent holds no `macro_std`, which is the
                // condition under test.
                "toolchain/std/src/mine.vl",
                "// A std file that defines a macro.\nmacro fun Marker(item: Item): Source {\n\tsource(\"\")\n}\n",
            ),
        ],
    );
    let mut command = Command::new(env!("CARGO_BIN_EXE_vilan"));
    let output = command
        .current_dir(&dir)
        .args(["build", "."])
        .env("NO_COLOR", "1")
        .env("VILAN_STD", dir.join("toolchain/std/src"))
        .output()
        .expect("run vilan");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(
        output.status.code(),
        Some(1),
        "the build fails, without panicking: {stderr}"
    );
    assert!(
        stderr.contains("`macro_std` package was not found"),
        "the macro-registration error is reported: {stderr}"
    );
    assert!(
        renders_in(&stderr, "mine.vl", "macro fun Marker(item: Item): Source {"),
        "and renders in the file that defines the macro: {stderr}"
    );
}
