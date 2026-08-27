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

#[test]
fn the_codegen_failure_renders_in_the_entry_that_lacks_main() {
    // `transform`'s ONE failure — a program with no `main` — and E16's recorded
    // leftover. It is structural: its subject is the ABSENCE of a definition, so
    // it carries no span into any file (`0..0`), and attributing it to the entry
    // is not a fallback — the entry's global scope is where `main` was looked
    // for. The module here is the discriminator: a diagnostic that took the last
    // loaded source, or the module the program spends its text on, would name
    // `alpha.vl`.
    let dir = temp_files(
        "codegen_no_main",
        &[
            ("vilan.toml", MANIFEST),
            (
                "src/main.vl",
                "import pkg::alpha::value;\n\nfun helper(): str {\n\tvalue()\n}\n",
            ),
            ("src/alpha.vl", "fun value(): str {\n\t\"ok\"\n}\n"),
        ],
    );
    let (output, stderr) = build_stderr(&dir);
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(
        output.status.code(),
        Some(1),
        "the build fails, without panicking: {stderr}"
    );
    assert!(
        stderr.contains("Cannot execute program without a main function"),
        "the codegen failure is reported: {stderr}"
    );
    assert!(
        renders_in(&stderr, "main.vl", "import pkg::alpha::value;"),
        "and renders in the ENTRY, quoting its first line: {stderr}"
    );
    assert!(
        !stderr.contains("alpha.vl"),
        "never in the module it happened to load: {stderr}"
    );
}

/// The leak tally's production surface (backlog E24): with `VILAN_LEAK_REPORT`
/// set, every top-level analysis prints one cumulative per-site line to
/// stderr — the same split the leak harness asserts on, so a live session's
/// growth can be corroborated in the field, not RSS-inferred. Off by default,
/// and stderr-only for the same reason warnings are: it must never corrupt
/// `build --stdout`'s JavaScript.
#[test]
fn leak_report_env_var_prints_the_per_site_split() {
    let dir = temp_package(
        "leakreport",
        "import std::print;\nfun main() { print(7); }\n",
    );
    let mut command = Command::new(env!("CARGO_BIN_EXE_vilan"));
    command
        .current_dir(&dir)
        .args(["build"])
        .env("VILAN_LEAK_REPORT", "1");
    let output = command.output().expect("run vilan");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("[vilan leak]"),
        "VILAN_LEAK_REPORT=1 printed no leak report; stderr was: {stderr}"
    );
    assert!(
        stderr.contains("ParseCleanCacheText") && stderr.contains("total"),
        "the report must carry the per-site split the harness asserts on \
         (every front-end parses std through `parse_clean_cached`, so that \
         site is always in a build's line); stderr was: {stderr}"
    );

    let mut command = Command::new(env!("CARGO_BIN_EXE_vilan"));
    command
        .current_dir(&dir)
        .args(["build"])
        .env_remove("VILAN_LEAK_REPORT");
    let output = command.output().expect("run vilan");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("[vilan leak]"),
        "the leak report must be off by default; stderr was: {stderr}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The std-tax arc's instrument (proposal/analysis-reuse.md §6): with
/// `VILAN_PHASE_TIMING` set, every top-level analysis prints one stderr line
/// splitting the wall between loading+walking, `build()`, and the
/// whole-program checks — the split every reuse slice is measured against.
/// Off by default, stderr-only, like the leak report beside it.
#[test]
fn phase_timing_env_var_prints_the_phase_split() {
    let dir = temp_package(
        "phasetiming",
        "import std::print;\nfun main() { print(7); }\n",
    );
    let mut command = Command::new(env!("CARGO_BIN_EXE_vilan"));
    command
        .current_dir(&dir)
        .args(["build"])
        .env("VILAN_PHASE_TIMING", "1");
    let output = command.output().expect("run vilan");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("[vilan phase]"),
        "VILAN_PHASE_TIMING=1 printed no phase line; stderr was: {stderr}"
    );
    assert!(
        stderr.contains("load+walk") && stderr.contains("build") && stderr.contains("checks"),
        "the phase line must carry the three-phase split the arc measures \
         against; stderr was: {stderr}"
    );

    let mut command = Command::new(env!("CARGO_BIN_EXE_vilan"));
    command
        .current_dir(&dir)
        .args(["build"])
        .env_remove("VILAN_PHASE_TIMING");
    let output = command.output().expect("run vilan");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("[vilan phase]"),
        "the phase line must be off by default; stderr was: {stderr}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The post-pass half of the split, per pass (backlog M5,
/// `perf-baseline.md` §6): the aggregate `post-passes` wall could not say
/// which pass moved, and it printed only on the `analyze_source` path — a CLI
/// build showed no post-pass line at all, which is why attributing M4 meant
/// hand-patching `Instant` marks into `post_analysis_passes` three times. The
/// breakdown now prints from inside that one shared function, so this pins
/// the property that mattered: the CLI pipeline shows it too, named per pass,
/// with the const pass's lowering/interpreting sub-split.
#[test]
fn phase_timing_env_var_prints_the_post_pass_breakdown() {
    let dir = temp_package(
        "phasepostpasses",
        "import std::print;\nfun main() { print(7); }\n",
    );
    let mut command = Command::new(env!("CARGO_BIN_EXE_vilan"));
    command
        .current_dir(&dir)
        .args(["build"])
        .env("VILAN_PHASE_TIMING", "1");
    let output = command.output().expect("run vilan");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("post-passes"),
        "VILAN_PHASE_TIMING=1 printed no post-pass line on the CLI pipeline; \
         stderr was: {stderr}"
    );
    for bucket in [
        "call-graph",
        "async-infer",
        "view-suspensions",
        "async-drops",
        "context-drops",
        "platform-color",
        "const-eval",
        "const-lower",
        "const-interp",
        "init-order",
    ] {
        assert!(
            stderr.contains(bucket),
            "the post-pass line must carry the `{bucket}` bucket — the whole \
             point is that the next attribution is a run, not a hand-patch; \
             stderr was: {stderr}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The B138 instrument: with `VILAN_DEPTH_STATS` set, every top-level
/// analysis prints one stderr line with, per recursive family, the peak
/// recursion depth and the stack consumed at that peak — the numbers the
/// compiler's stack margins are sized against (the v0.36.0 incident, commit
/// 0fb5e5f0, was sized by SIGABRT instead). Off by default, stderr-only,
/// like the phase line beside it.
#[test]
fn depth_stats_env_var_prints_the_depth_line() {
    let dir = temp_package(
        "depthstats",
        "import std::print;\nfun main() { print(7); }\n",
    );
    let mut command = Command::new(env!("CARGO_BIN_EXE_vilan"));
    command
        .current_dir(&dir)
        .args(["build"])
        .env("VILAN_DEPTH_STATS", "1");
    let output = command.output().expect("run vilan");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("[vilan depth]"),
        "VILAN_DEPTH_STATS=1 printed no depth line; stderr was: {stderr}"
    );
    for family in ["infer", "type-walk", "expr-walk", "pattern", "parse"] {
        assert!(
            stderr.contains(family),
            "the depth line must name the `{family}` family; stderr was: {stderr}"
        );
    }
    assert!(
        stderr.contains("MiB"),
        "each family carries the stack consumed at its peak, in MiB; \
         stderr was: {stderr}"
    );

    let mut command = Command::new(env!("CARGO_BIN_EXE_vilan"));
    command
        .current_dir(&dir)
        .args(["build"])
        .env_remove("VILAN_DEPTH_STATS");
    let output = command.output().expect("run vilan");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("[vilan depth]"),
        "the depth line must be off by default; stderr was: {stderr}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The batch half of the blackout (`editing-dx.md` S6/§13.1, the P29 shape).
///
/// `check`'s whole job is to answer questions about a file the user is still
/// writing, and it used to answer only one: a file whose parse was not clean was
/// not analyzed at all, so one missing `;` anywhere blinded it to every type
/// error in the rest of the file. It now analyzes the salvaged tree — the same
/// tree the language server has analyzed since the H6 cutover.
///
/// `build` keeps its contract, which is the other half of the pin: it reports the
/// parse errors and stops, because a recovered tree is not something to emit from.
const BROKEN_PLUS_TYPE_ERRORS: &str = "import std::print;\n\
                                       fun broken() {\n\
                                       \tlet a: i32 = 1\n\
                                       \tprint(a);\n\
                                       }\n\
                                       fun main() {\n\
                                       \tlet bad: i32 = \"text\";\n\
                                       \tlet other: str = 5;\n\
                                       \tprint(bad);\n\
                                       }\n";

#[test]
fn check_analyzes_a_file_that_did_not_parse_cleanly() {
    let dir = temp_package("check_salvage", BROKEN_PLUS_TYPE_ERRORS);
    let output = vilan(&dir, &["check", "."], true);
    let _ = std::fs::remove_dir_all(&dir);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "a broken file still fails `check`"
    );
    assert!(
        stderr.contains("expected `;` to end this statement"),
        "the parse error is still reported: {stderr}"
    );
    assert!(
        stderr.contains("Expected i32, but got str instead."),
        "the first type error, in the OTHER function, survives it: {stderr}"
    );
    assert!(
        stderr.contains("Expected str, but got i32 instead."),
        "and so does the second: {stderr}"
    );
}

#[test]
fn build_reports_only_the_parse_error_and_emits_nothing() {
    let dir = temp_package("build_salvage", BROKEN_PLUS_TYPE_ERRORS);
    let output = vilan(&dir, &["build", "."], true);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let emitted = dir.join("dist").exists() || dir.join("src/main.mjs").exists();
    let _ = std::fs::remove_dir_all(&dir);

    assert!(!output.status.success(), "a broken file fails `build`");
    assert!(
        stderr.contains("expected `;` to end this statement"),
        "the parse error is reported: {stderr}"
    );
    assert!(
        !stderr.contains("Expected i32, but got str instead."),
        "`build` does not analyze a recovered tree — its output contract is \
         unchanged (§13.1: change `check`, leave `build`): {stderr}"
    );
    assert!(
        !emitted,
        "and nothing is written from a tree that did not parse"
    );
}

// --- E76: a header and the label under it name the same position -------------
//
// Every span reaches ariadne in ONE index space (char offsets, converted once
// per span by `char_range` in main.rs). Before that, `IndexType::Byte` let
// ariadne derive a cross-source group's `file:line:col` sub-header from the
// label's already-converted CHAR offset as if it were still bytes, so any
// multibyte character earlier in the noted file dragged the sub-header a
// couple of lines above the label it heads (`reactive.vl:363:26` over a
// line-365 label). These pins parse the rendered report itself: the line a
// header names must be the gutter line number of the first quoted source line
// under it. std's own files carry the multibyte characters that made the two
// diverge, so the cross-source pins are non-vacuous against the byte-mode
// renderer (proven red on a revert of the `char_range` conversion).

/// The line number named by the first rendered header mentioning `file`
/// (`╭─[ file:line:col ]` or `├─[ file:line:col ]`), plus the gutter number
/// and text of the first quoted source line under it.
fn header_line_and_first_quoted_line(stderr: &str, file: &str) -> Option<(usize, usize, String)> {
    let mut lines = stderr.lines();
    let header = lines.find(|line| line.contains("─[ ") && line.contains(file))?;
    let inside = header.split("─[ ").nth(1)?.split(" ]").next()?;
    let mut fields = inside.rsplitn(3, ':');
    let _column: usize = fields.next()?.parse().ok()?;
    let header_line: usize = fields.next()?.parse().ok()?;
    let quoted = lines.find_map(|line| {
        let trimmed = line.trim_start();
        let digits: String = trimmed.chars().take_while(char::is_ascii_digit).collect();
        let rest = trimmed[digits.len()..].trim_start();
        (!digits.is_empty() && rest.starts_with('│')).then(|| {
            (
                digits.parse::<usize>().unwrap(),
                rest[3..].trim().to_string(),
            )
        })
    })?;
    Some((header_line, quoted.0, quoted.1))
}

#[test]
fn e76_the_coverage_notes_sub_header_agrees_with_its_label() {
    // The E74 flavor: `Signal::effect` at the top of `main` anchors at the
    // user's call and notes std's strict read — a cross-source note into
    // `reactive.vl`, whose sub-header must head the very line it labels.
    let dir = temp_package(
        "e76_coverage",
        "import std::print;\n\
         import std::reactive::Signal;\n\
         \n\
         fun main() {\n\
         \tlet count = Signal::new(1);\n\
         \tcount.effect(|value| print(value));\n\
         }\n\
         main();\n",
    );
    let (output, stderr) = build_stderr(&dir);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(!output.status.success(), "the uncovered effect must fail");
    let (header_line, label_line, quoted) =
        header_line_and_first_quoted_line(&stderr, "reactive.vl")
            .unwrap_or_else(|| panic!("a reactive.vl sub-header must render: {stderr}"));
    assert!(
        quoted.contains("owner_scope.get()"),
        "the note labels std's strict read: {stderr}"
    );
    assert_eq!(
        header_line, label_line,
        "the sub-header names the line its label sits on: {stderr}"
    );
}

#[test]
fn e76_the_generic_leak_notes_sub_header_agrees_with_its_label() {
    // The R11 flavor: `Option::map` at a resource is rejected at the user's
    // instantiation with a note into the generic body — a cross-source note
    // into `option.vl`, the other note-producer live today.
    let dir = temp_package(
        "e76_generic_leak",
        "import std::option::Option::{ self, Some };\n\
         resource struct Db { handle: i32 }\n\
         fun main() {\n\
         \tlet db = Db { handle = 1 };\n\
         \tlet opt: Option<Db> = Some(db);\n\
         \tlet n = opt.map(|d| d.handle);\n\
         }\n\
         main();\n",
    );
    let (output, stderr) = build_stderr(&dir);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(!output.status.success(), "the resource `map` must fail");
    let (header_line, label_line, quoted) = header_line_and_first_quoted_line(&stderr, "option.vl")
        .unwrap_or_else(|| panic!("an option.vl sub-header must render: {stderr}"));
    assert!(
        quoted.contains("Some(fn(x))"),
        "the note labels `map`'s own arm: {stderr}"
    );
    assert_eq!(
        header_line, label_line,
        "the sub-header names the line its label sits on: {stderr}"
    );
}

#[test]
fn e76_a_same_file_note_is_unmoved_by_the_char_conversion() {
    // The no-churn half: a note in the SAME file renders under the one
    // header, and that header still names the primary's exact position even
    // with multibyte trivia above it (the case where byte and char offsets
    // diverge — a wrong conversion would move this header).
    let dir = temp_package(
        "e76_same_file",
        "// café — café — café — multibyte trivia so byte and char offsets diverge\n\
         struct Cat { name: str }\n\
         fun main() {\n\
         \tlet w = welcome(Cat { name = \"tom\" });\n\
         }\n\
         trait Greet {\n\
         \tfun greet(self): str;\n\
         }\n\
         fun welcome<type T: Greet>(guest: T): str {\n\
         \tguest.greet()\n\
         }\n\
         main();\n",
    );
    let (output, stderr) = build_stderr(&dir);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(!output.status.success(), "the unmet bound must fail");
    let (header_line, label_line, quoted) = header_line_and_first_quoted_line(&stderr, "main.vl")
        .unwrap_or_else(|| panic!("the main.vl header must render: {stderr}"));
    assert!(
        stderr.contains("main.vl:4:10 ]"),
        "the header names the primary's exact line:col, unmoved: {stderr}"
    );
    assert_eq!(header_line, 4, "…which is line 4: {stderr}");
    assert_eq!(header_line, label_line, "…the line the primary labels");
    assert!(
        quoted.contains("welcome(Cat"),
        "quoting the call line: {stderr}"
    );
    assert!(
        !stderr.contains("├─["),
        "one file, one group — no sub-header at all: {stderr}"
    );
    assert!(
        stderr.contains("the bound is declared here"),
        "and the note still renders with it: {stderr}"
    );
}

#[test]
fn e84_a_dependency_read_reports_at_the_users_call() {
    // The E84 flavor of the e76 coverage pin (diagnostics-standard.md C3a,
    // ruled 2026-08-22): the demotion is not std-specific — a strict read
    // inside a PATH-DEPENDENCY package anchors at the user's call in
    // `main.vl`, the read demotes to the cross-source note in the package's
    // own file, and the package's INTERNAL frames (`middle()`, the
    // `deep_read()` call) are traversed but never labeled. Pre-widening
    // (the probe, 2026-08-24) the primary rendered inside `lib.vl` and both
    // internal call lines rendered as hops.
    let dir = temp_files(
        "e84_dependency",
        &[
            (
                "app/vilan.toml",
                "[package]\nname = \"app\"\n\n[package.dependencies]\ndepctx = { path = \"../depctx\" }\n",
            ),
            (
                "app/src/main.vl",
                "import std::print;\nimport depctx::entry;\n\nfun main() {\n\tprint(entry());\n}\nmain();\n",
            ),
            ("depctx/vilan.toml", "[library]\nname = \"depctx\"\n"),
            (
                "depctx/src/lib.vl",
                "import std::context::Context;\n\nlet current: Context<i32> = Context::new();\n\nfun deep_read(): i32 {\n\tcurrent.get()\n}\n\nfun middle(): i32 {\n\tdeep_read()\n}\n\nfun entry(): i32 {\n\tmiddle()\n}\n",
            ),
        ],
    );
    let (output, stderr) = build_stderr(&dir.join("app"));
    let _ = std::fs::remove_dir_all(&dir);

    assert!(!output.status.success(), "the uncovered read must fail");
    assert!(
        renders_in(&stderr, "main.vl", "print(entry());"),
        "the primary renders at the user's call: {stderr}"
    );
    assert!(
        stderr.contains("the read is inside `deep_read` here"),
        "the demotion note names the package function: {stderr}"
    );
    assert!(
        renders_in(&stderr, "lib.vl", "current.get()"),
        "the note renders in the package's own file: {stderr}"
    );
    assert!(
        stderr.find("main.vl").unwrap() < stderr.find("lib.vl").unwrap(),
        "the user's file leads; the package file is the sub-report: {stderr}"
    );
    assert!(
        !stderr.contains("middle()"),
        "package-internal frames are never labeled, so their lines never render: {stderr}"
    );
}

#[test]
fn e90_a_workspace_member_read_reports_at_itself() {
    // The member carve-out, end to end through the real manifest chain (E90,
    // diagnostics-standard.md C3a ruling note): the SAME package shape as
    // `e84_a_dependency_read_reports_at_the_users_call`, now declared in the
    // enclosing `[project]`'s `packages` — so it is the user's own code, and
    // the demotion never happens: the primary renders at the read in the
    // member's file, no C3 demotion note, and the member-internal call
    // frames render as labeled hops. Only the declaration differs from the
    // e84 fixture — membership, never path.
    let dir = temp_files(
        "e90_member",
        &[
            (
                "vilan.toml",
                "[project]\npackages = [\"app\", \"common\"]\n",
            ),
            (
                "app/vilan.toml",
                "[package]\nname = \"app\"\n\n[package.dependencies]\ncommon = { path = \"../common\" }\n",
            ),
            (
                "app/src/main.vl",
                "import std::print;\nimport common::entry;\n\nfun main() {\n\tprint(entry());\n}\nmain();\n",
            ),
            ("common/vilan.toml", "[library]\nname = \"common\"\n"),
            (
                "common/src/lib.vl",
                "import std::context::Context;\n\nlet current: Context<i32> = Context::new();\n\nfun deep_read(): i32 {\n\tcurrent.get()\n}\n\nfun middle(): i32 {\n\tdeep_read()\n}\n\nfun entry(): i32 {\n\tmiddle()\n}\n",
            ),
        ],
    );
    let (output, stderr) = build_stderr(&dir.join("app"));
    let _ = std::fs::remove_dir_all(&dir);

    assert!(!output.status.success(), "the uncovered read must fail");
    assert!(
        renders_in(&stderr, "lib.vl", "current.get()"),
        "the primary renders at the read, in the member's own file: {stderr}"
    );
    assert!(
        stderr.find("lib.vl").unwrap() < stderr.find("main.vl").unwrap(),
        "the member's file LEADS the report — it holds the primary, not a sub-report \
         (the demoted rendering leads with main.vl): {stderr}"
    );
    assert!(
        !stderr.contains("the read is inside"),
        "no demotion note for a workspace member: {stderr}"
    );
    assert!(
        stderr.contains("middle()"),
        "member-internal frames are labeled hops, so their lines render: {stderr}"
    );
    assert!(
        renders_in(&stderr, "main.vl", "print(entry());"),
        "the user-side hop still renders in main.vl: {stderr}"
    );
}
