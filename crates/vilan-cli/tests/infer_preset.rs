//! The `[build]` preset split for inferred `const` (proposal/const-eval.md §9),
//! pinned end-to-end through the real binary.
//!
//! `tests/infer_preset/project` is ONE source tree — every binding in it is an
//! ordinary `let`, no `const` keyword anywhere — and `tests/infer_preset/golden`
//! holds what it compiles to under each preset:
//!
//!   * `release.mjs` — the sweep ran: arithmetic, a chain through it, a call
//!     with const-known arguments, and a small list are all literals, and the
//!     functions that produced them are tree-shaken away;
//!   * `debug.mjs` — the sweep did not run, and every one of those bindings is
//!     still the expression the author wrote.
//!
//! The corpus gate cannot reach this: it builds bare `.vl` files with no
//! manifest, so it only ever exercises the debug default. That is by design —
//! it is what makes "no corpus golden moves" a real signal that the preset gate
//! held — but it leaves the release half unpinned, which is what this file is
//! for.
//!
//! The third test is the one that matters most. A fold is only legitimate if it
//! is OBSERVATIONALLY invisible, so both builds are run under node and their
//! output compared: same stdout, same exit code. That is the discipline the
//! whole feature stands on, and `infer_differential.rs` extends it across the
//! entire corpus.
//!
//! Regenerating a golden is the corpus ritual (AGENTS.md): rebuild the debug
//! binary, build the project by hand under each preset, read the diff, and only
//! then copy the artifacts over.

use std::path::{Path, PathBuf};
use std::process::Command;

mod support;

fn project_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/infer_preset/project")
}

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/infer_preset/golden")
}

fn std_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std")
}

/// Copies the project into a scratch directory, rewrites its `preset`, builds,
/// and returns the emitted JavaScript. Building a copy rather than the fixture
/// keeps the source tree clean and lets two presets run without racing.
fn build_under(preset: &str) -> String {
    // Per CALL, not per preset: nextest runs this file's tests concurrently in
    // one process, and two of them build `release`. Sharing a directory meant
    // one test deleting the tree another was mid-build in.
    static ROUND: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let round = ROUND.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let work = support::scratch_root().join(format!(
        "vilan_infer_preset_{}_{preset}_{round}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(work.join("src")).expect("create work dir");

    let manifest = std::fs::read_to_string(project_dir().join("vilan.toml"))
        .expect("read fixture manifest")
        .replace("preset = \"release\"", &format!("preset = \"{preset}\""));
    assert!(
        manifest.contains(&format!("preset = \"{preset}\"")),
        "the fixture manifest must declare a preset this can rewrite"
    );
    std::fs::write(work.join("vilan.toml"), manifest).expect("write manifest");
    std::fs::copy(project_dir().join("src/main.vl"), work.join("src/main.vl"))
        .expect("copy fixture source");

    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .arg("build")
        .arg(&work)
        .env("VILAN_STD", std_dir())
        .output()
        .expect("run vilan build");
    assert!(
        output.status.success(),
        "the {preset} build failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let emitted = std::fs::read_to_string(work.join("src/main.mjs")).expect("read emitted JS");
    let _ = std::fs::remove_dir_all(&work);
    emitted
}

/// The release build folds, byte-for-byte.
#[test]
fn the_release_golden_carries_the_folds() {
    let golden = std::fs::read_to_string(golden_dir().join("release.mjs")).expect("read golden");
    let rebuilt = build_under("release");
    assert_eq!(
        golden, rebuilt,
        "the release golden moved — either the sweep changed what it folds, or \
         codegen did. Read the diff before regenerating (const-eval.md §9)."
    );
    // Stated over the golden itself, so the pin says what "folded" MEANS rather
    // than only that some bytes matched: the arithmetic, the chain through it,
    // the call, and the small list are all literals now.
    for literal in ["=7;", "=14;", "=196;", "=[0,3,6,9];", "=107;"] {
        assert!(
            golden.contains(literal),
            "the release golden is missing the folded literal `{literal}`"
        );
    }
    assert!(
        !golden.contains("1+2*3"),
        "the release golden still carries the unfolded arithmetic"
    );
}

/// The debug twin folds NOTHING — and that is what makes the corpus gate a real
/// signal, since `BuildOptions::default()` is this preset.
#[test]
fn the_debug_golden_carries_no_folds() {
    let golden = std::fs::read_to_string(golden_dir().join("debug.mjs")).expect("read golden");
    let rebuilt = build_under("debug");
    assert_eq!(
        golden, rebuilt,
        "the debug golden moved — the preset gate may have leaked \
         (const-eval.md §9.4)."
    );
    // Every binding is still the expression the author wrote.
    for expression in [
        "const a = 1 + 2 * 3;",
        "const b = a * 2;",
        "const squared = square(b);",
        "const steps = scale(4);",
        "const shifted = offset(a);",
        "const announced = announce();",
    ] {
        assert!(
            golden.contains(expression),
            "the debug golden folded `{expression}`, which the debug preset must \
             never do — folded computation vanishes from stack traces"
        );
    }
}

/// THE EQUIVALENCE GATE: a fold must be observationally invisible. Both builds
/// run under node and must agree exactly — same stdout (`built` from the
/// deliberately unfoldable `announce`, then the sum), same exit code.
///
/// The `built` line is the load-bearing half. `announce` prints, so folding it
/// would silently delete a line from the program's output — the hole §5 did not
/// record and §9.2 closes. If this test ever passes with only the number
/// matching, the effect rule has regressed.
#[test]
fn both_presets_run_identically_under_node() {
    let release = run_under_node("release", &build_under("release"));
    let debug = run_under_node("debug", &build_under("debug"));
    assert_eq!(
        release, debug,
        "the release build is not observationally identical to the debug one — \
         a fold changed what the program does (const-eval.md §9.2)"
    );
    assert!(
        release.0.contains("built"),
        "the printing binding must still print: folding it would delete output \
         from a working program. Got {:?}",
        release.0
    );
    assert!(
        release.0.contains("335"),
        "the computed sum is wrong: {:?}",
        release.0
    );
}

/// Runs emitted JavaScript under node, returning `(stdout, exit code)`.
fn run_under_node(label: &str, javascript: &str) -> (String, i32) {
    let script = support::scratch_root().join(format!(
        "vilan_infer_preset_run_{}_{label}.mjs",
        std::process::id()
    ));
    std::fs::write(&script, javascript).expect("write script");
    let output = Command::new("node")
        .arg(&script)
        .output()
        .expect("run node (the equivalence gate needs it on PATH)");
    let _ = std::fs::remove_file(&script);
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
    )
}

/// Builds a one-file project whose `[build]` section is `build`, answering
/// the build's output and, when it succeeded, what `node` printed (stdout,
/// stderr) running it.
fn build_dbg_project(tag: &str, build: &str) -> (std::process::Output, Option<(String, String)>) {
    build_debugging_project(
        tag,
        build,
        "fun main() {\n\tlet total = dbg(2 * 3) + 1;\n\tprint(total);\n}\n",
    )
}

/// [`build_dbg_project`] over a `src/main.vl` of `source`.
fn build_debugging_project(
    tag: &str,
    build: &str,
    source: &str,
) -> (std::process::Output, Option<(String, String)>) {
    let work =
        support::scratch_root().join(format!("vilan_dbg_policy_{}_{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(work.join("src")).expect("create work dir");
    std::fs::write(
        work.join("vilan.toml"),
        format!("[package]\nname = \"probe\"\n[build]\n{build}"),
    )
    .expect("write manifest");
    std::fs::write(work.join("src/main.vl"), source).expect("write source");
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .arg("build")
        .arg(&work)
        .env("VILAN_STD", std_dir())
        .output()
        .expect("run vilan build");
    let ran = output.status.success().then(|| {
        let run = Command::new("node")
            .arg(work.join("src/main.mjs"))
            .output()
            .expect("run node");
        (
            String::from_utf8_lossy(&run.stdout).into_owned(),
            String::from_utf8_lossy(&run.stderr).into_owned(),
        )
    });
    let _ = std::fs::remove_dir_all(&work);
    (output, ran)
}

/// debugging.md §3.3 (Q4): a release build REFUSES a `dbg(..)` it was not
/// told to keep or strip — at the call, saying how to keep or strip it.
#[test]
fn a_release_build_refuses_a_dbg_left_in_it() {
    let (output, _) = build_dbg_project("refuse", "preset = \"release\"\n");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "the release build must fail");
    assert!(
        stderr.contains("`dbg` left in a release build")
            && stderr.contains("`[build] dbg = \"strip\"`"),
        "{stderr}"
    );
}

/// debugging.md S2 under Q4: a release build refuses a `dbg_stack()` as it
/// refuses a `dbg`, at the call; `strip` makes it nothing and `keep` prints
/// it in release too.
#[test]
fn a_release_build_refuses_strips_or_keeps_a_dbg_stack_as_dbg() {
    let source = "fun main() {\n\tlet total = 7;\n\tdbg_stack();\n\tprint(total);\n}\n";
    let (output, _) = build_debugging_project("stack_refuse", "preset = \"release\"\n", source);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "the release build must fail");
    assert!(
        stderr.contains("`dbg_stack()` left in a release build")
            && stderr.contains("`[build] dbg = \"strip\"`")
            && stderr.contains("main.vl:3:2"),
        "{stderr}"
    );
    let (output, ran) = build_debugging_project(
        "stack_strip",
        "preset = \"release\"\ndbg = \"strip\"\n",
        source,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(ran, Some(("7\n".to_string(), String::new())));
    let (output, ran) = build_debugging_project(
        "stack_keep",
        "preset = \"release\"\ndbg = \"keep\"\n",
        source,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        ran,
        Some((
            "7\n".to_string(),
            "[src/main.vl:3:2] dbg_stack() in main\n  total: i32 = 7\n".to_string()
        ))
    );
}

/// Q4's two overrides: `strip` makes `dbg(x)` its argument and prints
/// nothing; `keep` prints in release too, naming the file from the package
/// root (`src/main.vl`).
#[test]
fn a_release_build_strips_or_keeps_dbg_as_the_manifest_says() {
    let (output, ran) = build_dbg_project("strip", "preset = \"release\"\ndbg = \"strip\"\n");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(ran, Some(("7\n".to_string(), String::new())));
    let (output, ran) = build_dbg_project("keep", "preset = \"release\"\ndbg = \"keep\"\n");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        ran,
        Some((
            "7\n".to_string(),
            "[src/main.vl:2:14] 2 * 3 = 6\n".to_string()
        ))
    );
}
