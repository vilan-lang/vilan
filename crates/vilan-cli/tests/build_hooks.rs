//! End-to-end CLI tests for the build-hook staleness predicate and the tier-2
//! boundary (proposal/build-hooks.md §3 and §4.3, slices S1 and S2; the trust
//! model they extend is proposal/build-trust.md).
//!
//! Two behaviors, one file, because they are two halves of one question the
//! manifest asks — *does this command run?*:
//!
//! * **S1, freshness.** A `[[build.hook]]` that declares `inputs` and/or
//!   `outputs` runs when one of them has moved and is skipped when none has,
//!   decided by CONTENT and recorded in `dist/.build-hooks.json`. A `[build]
//!   run` command declares nothing and so runs every time, exactly as it
//!   always has.
//! * **S2, the tier-2 note.** A dependency that declares build hooks does not
//!   get to run them — and now says so, once per build, naming itself. The
//!   opt-in (`build-hooks = true`) parses and is refused too: the point of the
//!   slice is that the syntax is fixed before anything can cross it.
//! * **The watch rounds §9 owed** (G10). The declaration is read by two
//!   consumers — the freshness stamp and `--watch`'s wake-up set — and they
//!   have to be the same reading. These pins drive a real `--watch` session and
//!   observe rounds, which no `build`-at-a-time pin above can.
//!
//! Each test writes a throwaway project tree and drives the built `vilan`
//! binary. The fixtures are per-platform where they have to be: a hook runs
//! through the PLATFORM shell, so `printf` (which `cmd` does not have) is
//! never assumed.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

mod support;

/// A fresh temp directory for one test's project tree.
fn temp_project(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "vilan_hooks_cli_{tag}_{}_{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// Runs the `vilan` binary with `args`, with `NO_COLOR=1` so the dim note and
/// the `Fresh` line can be asserted as literal text.
fn vilan(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .expect("run vilan")
}

fn combined(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// A hook command that appends one line to `file`, in the platform's shell.
/// Counting these lines is how a test knows whether a hook ran.
fn append(file: &str) -> String {
    if cfg!(windows) {
        format!("echo ran>> {file}")
    } else {
        format!("printf 'ran\\n' >> {file}")
    }
}

/// A hook command that writes one line of `text` to `file`, replacing it.
fn write_line(file: &str, text: &str) -> String {
    if cfg!(windows) {
        format!("echo {text}> {file}")
    } else {
        format!("printf '{text}\\n' > {file}")
    }
}

/// How many times the hook wrote its marker. A missing file is zero runs.
/// Counted by non-blank lines, because `cmd`'s `echo` and `printf` disagree
/// about trailing whitespace and line endings and neither is the assertion.
fn runs(dir: &Path, file: &str) -> usize {
    std::fs::read_to_string(dir.join(file))
        .map(|text| text.lines().filter(|line| !line.trim().is_empty()).count())
        .unwrap_or(0)
}

/// The smallest program that compiles and runs.
const MAIN: &str = "import std::print;\nfun main() { print(\"ok\") }\nmain();\n";

/// A `[package]` with one declared hook: it appends to `ran.txt` and writes
/// `generated.txt`, declaring `input.txt` in and `generated.txt` out.
fn declared_hook_manifest() -> String {
    format!(
        "[package]\nname = \"app\"\n\n[[build.hook]]\nname = \"gen\"\nrun = [{}, {}]\n\
         inputs = \"input.txt\"\noutputs = \"generated.txt\"\n",
        toml_string(&append("ran.txt")),
        toml_string(&write_line("generated.txt", "generated"))
    )
}

/// A TOML basic string. The commands carry `'` and `\` on unix, so they cannot
/// be pasted raw.
fn toml_string(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// Writes the standard one-hook project and returns its directory.
fn declared_hook_project(tag: &str) -> PathBuf {
    let dir = temp_project(tag);
    write(&dir, "vilan.toml", &declared_hook_manifest());
    write(&dir, "src/main.vl", MAIN);
    write(&dir, "input.txt", "one\n");
    dir
}

/// Builds `dir`, asserting success.
fn build(dir: &Path) -> String {
    let output = vilan(&["build", dir.to_str().unwrap()]);
    let text = combined(&output);
    assert!(output.status.success(), "build failed:\n{text}");
    text
}

// ── S1: the staleness predicate (§3, one pin per transition of §9's list) ──

#[test]
fn a_declared_hook_runs_cold_and_is_skipped_while_nothing_moves() {
    // The two halves of the whole feature: it runs once (there is no stamp),
    // and then it does not (every declared path re-digests to what was
    // recorded). Three builds, one run.
    let dir = declared_hook_project("fresh");
    let first = build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 1, "the cold build runs it:\n{first}");
    assert!(
        !first.contains("Fresh"),
        "a cold hook is not fresh:\n{first}"
    );

    let second = build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 1, "the second build skips it");
    assert!(
        second.contains("Fresh   gen"),
        "a skipped hook says so, by name — silence is the failure mode this \
         design exists to avoid:\n{second}"
    );
    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 1, "and the third");
    // The build still happened.
    assert!(dir.join("src/main.mjs").is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_moved_input_reruns_the_hook() {
    // The transition the whole predicate exists for. Content, never mtime:
    // the file is rewritten with DIFFERENT bytes.
    let dir = declared_hook_project("input_moved");
    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 1);
    write(&dir, "input.txt", "two\n");
    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 2, "a changed input re-runs it");
    // And settles again.
    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rewriting_an_input_with_the_same_bytes_does_not_rerun_it() {
    // The other half of "content, never mtime" — and the bug the watch loop
    // already refused once. Touching a file must not cost a hook run.
    let dir = declared_hook_project("same_bytes");
    build(&dir);
    write(&dir, "input.txt", "one\n");
    let second = build(&dir);
    assert_eq!(
        runs(&dir, "ran.txt"),
        1,
        "same bytes, same digest, no run:\n{second}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_input_that_appears_where_it_was_missing_reruns_the_hook() {
    // A declared input that is not there is recorded AS missing rather than
    // ignored — a file that was not there is a dependency, and its appearance
    // has to invalidate.
    let dir = declared_hook_project("input_appears");
    std::fs::remove_file(dir.join("input.txt")).unwrap();
    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 1);
    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 1, "still missing, still fresh");
    write(&dir, "input.txt", "arrived\n");
    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 2, "its appearance re-runs it");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_deleted_output_reruns_the_hook() {
    let dir = declared_hook_project("output_gone");
    build(&dir);
    std::fs::remove_file(dir.join("generated.txt")).unwrap();
    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 2, "a missing output is not fresh");
    assert!(dir.join("generated.txt").is_file(), "and it was rebuilt");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_hook_that_does_not_write_a_declared_output_says_so() {
    // A hook that SUCCEEDS and leaves a declared output missing is the one
    // silent failure the predicate can produce: `fingerprint` refuses to
    // record it (§3.1 requires every declared output to exist), so nothing is
    // stamped and the hook re-runs forever — while the build goes on to fail
    // somewhere else entirely, at the import of the module the hook was
    // supposed to write. The manifest said what it would produce; when it does
    // not, the build has to name that, or the user reads a `cannot find` error
    // with no path back to its cause.
    let dir = temp_project("output_never_written");
    write(
        &dir,
        "vilan.toml",
        &format!(
            "[package]\nname = \"app\"\n\n[[build.hook]]\nname = \"gen\"\nrun = {}\n\
             inputs = \"input.txt\"\noutputs = \"generated.txt\"\n",
            toml_string(&append("ran.txt"))
        ),
    );
    write(&dir, "src/main.vl", MAIN);
    write(&dir, "input.txt", "one\n");

    let first = build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 1, "the hook ran:\n{first}");
    assert!(
        first.contains("`gen`") && first.contains("generated.txt"),
        "a hook that did not write its declared output is named, with the \
         output it promised:\n{first}"
    );

    // Still only a note: the hook itself succeeded, so the build's own outcome
    // is unchanged — and the next build re-runs it, because nothing was
    // stamped.
    let second = build(&dir);
    assert_eq!(
        runs(&dir, "ran.txt"),
        2,
        "an unwritten output leaves no stamp, so the hook re-runs:\n{second}"
    );
    assert!(
        !second.contains("Fresh"),
        "and it is never fresh:\n{second}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_written_output_draws_no_note() {
    // The other half of the pin above: the note fires on the missing output,
    // not on every hook that declares one. Non-vacuity in the file rather than
    // in a comment.
    let dir = declared_hook_project("output_written");
    let text = build(&dir);
    assert!(
        !text.contains("did not write"),
        "a hook that produced its declared output is not warned about:\n{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_hand_edited_output_reruns_the_hook() {
    let dir = declared_hook_project("output_edited");
    build(&dir);
    write(&dir, "generated.txt", "tampered\n");
    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 2, "an edited output is not fresh");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_changed_command_string_reruns_the_hook() {
    // Editing the hook's own declaration re-runs it by construction, which is
    // half of why §3.2's accepted unsoundness is bounded.
    let dir = declared_hook_project("command_changed");
    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 1);
    let changed = format!(
        "[package]\nname = \"app\"\n\n[[build.hook]]\nname = \"gen\"\nrun = [{}, {}]\n\
         inputs = \"input.txt\"\noutputs = \"generated.txt\"\n",
        toml_string(&append("ran.txt")),
        toml_string(&write_line("generated.txt", "different"))
    );
    write(&dir, "vilan.toml", &changed);
    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 2, "a changed command is not fresh");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn changing_the_declared_paths_reruns_the_hook() {
    // The declaration is part of the fingerprint, so adding an input is a
    // change even when every file it names is untouched. No rule of its own —
    // it falls out of comparing the whole recorded structure.
    let dir = declared_hook_project("declaration_changed");
    build(&dir);
    write(&dir, "second.txt", "second\n");
    let widened = format!(
        "[package]\nname = \"app\"\n\n[[build.hook]]\nname = \"gen\"\nrun = [{}, {}]\n\
         inputs = [\"input.txt\", \"second.txt\"]\noutputs = \"generated.txt\"\n",
        toml_string(&append("ran.txt")),
        toml_string(&write_line("generated.txt", "generated"))
    );
    write(&dir, "vilan.toml", &widened);
    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 2, "a widened declaration re-runs it");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_declared_directory_digests_its_whole_tree() {
    // The copy case (§2.1's `src/static`): a directory is declared as one
    // path, and a file added anywhere under it moves the digest. This is what
    // stands in for the glob patterns the manifest refuses.
    let dir = temp_project("directory_input");
    write(
        &dir,
        "vilan.toml",
        &format!(
            "[package]\nname = \"app\"\n\n[[build.hook]]\nname = \"copy\"\nrun = {}\n\
             inputs = \"static\"\n",
            toml_string(&append("ran.txt"))
        ),
    );
    write(&dir, "src/main.vl", MAIN);
    write(&dir, "static/a.txt", "a\n");
    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 1);
    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 1, "an untouched tree is fresh");
    write(&dir, "static/nested/b.txt", "b\n");
    build(&dir);
    assert_eq!(
        runs(&dir, "ran.txt"),
        2,
        "a new file in the tree re-runs it"
    );
    write(&dir, "static/a.txt", "changed\n");
    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 3, "so does a changed file in it");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rerun_hooks_runs_a_fresh_hook_anyway() {
    // §3.2's escape, for the hook that reads something it did not declare.
    let dir = declared_hook_project("rerun_flag");
    build(&dir);
    let output = vilan(&["build", dir.to_str().unwrap(), "--rerun-hooks"]);
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert_eq!(runs(&dir, "ran.txt"), 2, "--rerun-hooks ignores freshness");
    assert!(
        !text.contains("Fresh"),
        "and does not claim freshness:\n{text}"
    );
    // The stamp is still correct afterwards, so the next plain build skips.
    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_hook_that_declares_nothing_runs_on_every_build() {
    // Today's behavior, kept exactly: a hook with no `inputs` and no `outputs`
    // is never fresh. Both spellings of it — the `run` list and a
    // `[[build.hook]]` that declares no paths.
    let plain = temp_project("undeclared_run");
    write(
        &plain,
        "vilan.toml",
        &format!(
            "[package]\nname = \"app\"\n\n[build]\nrun = [{}]\n",
            toml_string(&append("ran.txt"))
        ),
    );
    write(&plain, "src/main.vl", MAIN);
    for expected in 1..=3 {
        build(&plain);
        assert_eq!(runs(&plain, "ran.txt"), expected, "`run` runs every build");
    }
    // …and it grows no `dist/`: the stamp is written only where there is a
    // declared hook to stamp, so a `run`-only project is untouched.
    assert!(
        !plain.join("dist").exists(),
        "a `run`-only project gains no dist/"
    );
    let _ = std::fs::remove_dir_all(&plain);

    let named = temp_project("undeclared_table");
    write(
        &named,
        "vilan.toml",
        &format!(
            "[package]\nname = \"app\"\n\n[[build.hook]]\nname = \"always\"\nrun = {}\n",
            toml_string(&append("ran.txt"))
        ),
    );
    write(&named, "src/main.vl", MAIN);
    for expected in 1..=3 {
        let text = build(&named);
        assert_eq!(
            runs(&named, "ran.txt"),
            expected,
            "and so does a named hook"
        );
        assert!(!text.contains("Fresh"), "never fresh:\n{text}");
    }
    let _ = std::fs::remove_dir_all(&named);
}

#[test]
fn a_failing_hook_leaves_no_stamp_so_the_next_build_reruns_it() {
    // A hook that failed did not produce what it promised, so recording it as
    // done would skip it forever. The build fails, naming the hook.
    let dir = temp_project("hook_fails");
    write(
        &dir,
        "vilan.toml",
        &format!(
            "[package]\nname = \"app\"\n\n[[build.hook]]\nname = \"gen\"\n\
             run = [{}, \"exit 3\"]\ninputs = \"input.txt\"\n",
            toml_string(&append("ran.txt"))
        ),
    );
    write(&dir, "src/main.vl", MAIN);
    write(&dir, "input.txt", "one\n");

    let output = vilan(&["build", dir.to_str().unwrap()]);
    let text = combined(&output);
    assert!(!output.status.success(), "a failing hook fails the build");
    assert!(
        text.contains("[[build.hook]]") && text.contains("gen") && text.contains("exit 3"),
        "the failure names the hook and the command:\n{text}"
    );
    assert!(
        !dir.join("src/main.mjs").exists(),
        "the build never happened"
    );

    let second = vilan(&["build", dir.to_str().unwrap()]);
    assert!(!second.status.success());
    assert_eq!(
        runs(&dir, "ran.txt"),
        2,
        "a failed hook is not fresh: {}",
        combined(&second)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_stamp_lives_in_dist_so_removing_dist_rebuilds_the_hooks() {
    // Q2's ruling, made observable: `rm -rf dist` means "rebuild everything,
    // hooks included" — the sentence a user already believes.
    let dir = declared_hook_project("stamp_in_dist");
    build(&dir);
    let stamp = dir.join("dist").join(".build-hooks.json");
    assert!(stamp.is_file(), "the stamp is at dist/.build-hooks.json");
    let text = std::fs::read_to_string(&stamp).unwrap();
    assert!(
        text.contains("\"gen\"") && text.contains("\"input.txt\"") && text.contains("\"outputs\""),
        "the stamp keys on the hook's name and its declared paths:\n{text}"
    );
    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 1);

    std::fs::remove_dir_all(dir.join("dist")).unwrap();
    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 2, "no stamp, so it runs");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_corrupt_stamp_reads_as_no_stamp_rather_than_failing_the_build() {
    // The whole design is safe because its failure direction is a re-run. A
    // truncated write, a hand edit, a version this binary does not know: each
    // costs one hook run, never a wrong build and never an error.
    let dir = declared_hook_project("corrupt_stamp");
    build(&dir);
    let stamp = dir.join("dist").join(".build-hooks.json");
    for corruption in ["", "{", "not json at all", "{\"version\": \"99\"}"] {
        std::fs::write(&stamp, corruption).unwrap();
        let before = runs(&dir, "ran.txt");
        let text = build(&dir);
        assert_eq!(
            runs(&dir, "ran.txt"),
            before + 1,
            "`{corruption}` must re-run rather than be trusted:\n{text}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_hook_removed_from_the_manifest_takes_its_stamp_entry_with_it() {
    // The stamp is a function of what the manifest says TODAY, so it cannot
    // accumulate entries for hooks that no longer exist.
    let dir = declared_hook_project("stamp_pruned");
    build(&dir);
    let stamp = dir.join("dist").join(".build-hooks.json");
    assert!(std::fs::read_to_string(&stamp).unwrap().contains("gen"));
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    build(&dir);
    assert!(
        !stamp.exists(),
        "the last entry removed takes the file with it"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_hook_generates_a_module_the_same_build_compiles_and_then_stops_paying_for_it() {
    // The paper's P6 probe, promoted to a pin and carried one step further:
    // the generated module is absent from a clean tree, present and compiled
    // after one command — and the SECOND build skips the generator while
    // still compiling the module it left behind. That second half is the
    // lucide case in miniature.
    let dir = temp_project("generates_module");
    let generate = if cfg!(windows) {
        "echo fun generated(): i32 { 41 }> src/generated.vl".to_string()
    } else {
        "printf 'fun generated(): i32 { 41 }\\n' > src/generated.vl".to_string()
    };
    write(
        &dir,
        "vilan.toml",
        &format!(
            "[package]\nname = \"app\"\n\n[[build.hook]]\nname = \"icons\"\nrun = [{}, {}]\n\
             inputs = \"icons.lock\"\noutputs = \"src/generated.vl\"\n",
            toml_string(&append("ran.txt")),
            toml_string(&generate)
        ),
    );
    write(
        &dir,
        "src/main.vl",
        "import std::print;\nimport pkg::generated::generated;\n\
         fun main() { print(generated() + 1) }\nmain();\n",
    );
    write(&dir, "icons.lock", "v1\n");
    assert!(!dir.join("src/generated.vl").exists());

    let output = vilan(&["run", dir.to_str().unwrap()]);
    let text = combined(&output);
    assert!(
        output.status.success() && text.contains("42"),
        "the hook produced the module the build consumed:\n{text}"
    );
    assert_eq!(runs(&dir, "ran.txt"), 1);

    let output = vilan(&["run", dir.to_str().unwrap()]);
    let text = combined(&output);
    assert!(output.status.success() && text.contains("42"), "{text}");
    assert_eq!(
        runs(&dir, "ran.txt"),
        1,
        "the generator is fresh, and the module it wrote still compiles:\n{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_runs_no_hook_of_either_form_and_writes_no_stamp() {
    // `vilan check` produces no artifacts, so there is nothing for a hook to
    // feed — and nothing to stamp.
    let dir = declared_hook_project("check");
    let output = vilan(&["check", dir.to_str().unwrap()]);
    assert!(output.status.success(), "{}", combined(&output));
    assert_eq!(runs(&dir, "ran.txt"), 0);
    assert!(!dir.join("dist").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

// ── S2: the tier-2 boundary, said and spelled (§4.3, `build-trust.md` §3) ──

/// Writes an app at `dir` depending on a `[library]` at `dir/dep` that
/// declares a build hook. `grant` is the dependency declaration's trust key.
fn dependency_with_a_hook(dir: &Path, grant: &str) {
    write(
        dir,
        "vilan.toml",
        &format!(
            "[package]\nname = \"app\"\n[package.dependencies]\ndep = {{ path = \"dep\"{grant} }}\n"
        ),
    );
    write(dir, "src/main.vl", MAIN);
    write(
        &dir.join("dep"),
        "vilan.toml",
        &format!(
            "[library]\nname = \"dep\"\n\n[build]\nrun = {}\n",
            toml_string(&append("../dependency-ran.txt"))
        ),
    );
    write(&dir.join("dep"), "src/lib.vl", "fun unused(): i32 { 1 }\n");
}

#[test]
fn an_un_opted_in_dependency_hook_prints_one_note_and_does_not_run() {
    // P5 promoted from a probe to a pin, with its silence closed. Before this
    // slice a dependency's `[build] run` produced no output, no warning and no
    // note — indistinguishable from the toolchain never having looked.
    let dir = temp_project("dep_no_optin");
    dependency_with_a_hook(&dir, "");
    let output = vilan(&["build", dir.to_str().unwrap()]);
    let text = combined(&output);
    assert!(
        output.status.success(),
        "a refused dependency hook is a normal outcome, not a failure:\n{text}"
    );
    assert_eq!(
        runs(&dir, "dependency-ran.txt"),
        0,
        "the dependency's hook did not run:\n{text}"
    );
    assert_eq!(
        text.matches("note: `dep` declares build hooks").count(),
        1,
        "exactly one line, naming the dependency:\n{text}"
    );
    assert!(
        text.contains("build-hooks = true"),
        "and it names the opt-in that would record consent:\n{text}"
    );
    assert!(
        !text.to_lowercase().contains("warning:"),
        "a note, never a warning — §3 calls the refusal a normal outcome:\n{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_opted_in_dependency_hook_still_does_not_run_and_says_so() {
    // The syntax is shipped REFUSING everything: fixed and reviewable before
    // anything can cross it.
    let dir = temp_project("dep_optin");
    dependency_with_a_hook(&dir, ", build-hooks = true");
    let output = vilan(&["build", dir.to_str().unwrap()]);
    let text = combined(&output);
    assert!(
        output.status.success(),
        "the exit code is unchanged:\n{text}"
    );
    assert_eq!(
        runs(&dir, "dependency-ran.txt"),
        0,
        "no dependency hook runs in this slice:\n{text}"
    );
    assert_eq!(
        text.matches("is opted in").count(),
        1,
        "the opt-in case gets its own line, once:\n{text}"
    );
    assert!(
        text.contains("`dep`") && text.contains("did not"),
        "naming the dependency and what did not happen:\n{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_dependency_without_hooks_is_never_mentioned() {
    // The note exists because a dependency asked for something. One that asked
    // for nothing must stay silent, or the line stops carrying information.
    let dir = temp_project("dep_quiet");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\n[package.dependencies]\ndep = { path = \"dep\" }\n",
    );
    write(&dir, "src/main.vl", MAIN);
    write(
        &dir.join("dep"),
        "vilan.toml",
        "[library]\nname = \"dep\"\n",
    );
    write(&dir.join("dep"), "src/lib.vl", "fun unused(): i32 { 1 }\n");
    let output = vilan(&["build", dir.to_str().unwrap()]);
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert!(!text.contains("declares build hooks"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_note_appears_once_per_build_not_once_per_member() {
    // Two workspace members depending on the same package: the note is about
    // the package, so it is said once however many edges reach it.
    let dir = temp_project("dep_shared");
    write(
        &dir,
        "vilan.toml",
        "[project]\npackages = [\"one\", \"two\"]\n",
    );
    for member in ["one", "two"] {
        write(
            &dir.join(member),
            "vilan.toml",
            &format!(
                "[package]\nname = \"{member}\"\n[package.dependencies]\n\
                 dep = {{ path = \"../dep\" }}\n"
            ),
        );
        write(&dir.join(member), "src/main.vl", MAIN);
    }
    write(
        &dir.join("dep"),
        "vilan.toml",
        &format!(
            "[library]\nname = \"dep\"\n\n[build]\nrun = {}\n",
            toml_string(&append("../dependency-ran.txt"))
        ),
    );
    write(&dir.join("dep"), "src/lib.vl", "fun unused(): i32 { 1 }\n");

    let output = vilan(&["build", dir.to_str().unwrap()]);
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert_eq!(
        text.matches("note: `dep` declares build hooks").count(),
        1,
        "once per build, not once per member:\n{text}"
    );
    assert_eq!(runs(&dir, "dependency-ran.txt"), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_grant_written_by_either_member_counts_for_the_shared_dependency() {
    // A package reached through two edges is one row, so the grant on one edge
    // has to reach it — otherwise a real opt-in would be reported as absent.
    let dir = temp_project("dep_shared_grant");
    write(
        &dir,
        "vilan.toml",
        "[project]\npackages = [\"one\", \"two\"]\n",
    );
    for (member, grant) in [("one", ""), ("two", ", build-hooks = true")] {
        write(
            &dir.join(member),
            "vilan.toml",
            &format!(
                "[package]\nname = \"{member}\"\n[package.dependencies]\n\
                 dep = {{ path = \"../dep\"{grant} }}\n"
            ),
        );
        write(&dir.join(member), "src/main.vl", MAIN);
    }
    write(
        &dir.join("dep"),
        "vilan.toml",
        "[library]\nname = \"dep\"\n\n[build]\nrun = \"exit 0\"\n",
    );
    write(&dir.join("dep"), "src/lib.vl", "fun unused(): i32 { 1 }\n");

    let output = vilan(&["build", dir.to_str().unwrap()]);
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert_eq!(
        text.matches("is opted in").count(),
        1,
        "the grant on one edge is the package's answer:\n{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_says_nothing_about_dependency_hooks() {
    // `vilan check` runs no hooks at all, first-party ones included, so there
    // is no refusal to report.
    let dir = temp_project("dep_check");
    dependency_with_a_hook(&dir, "");
    let output = vilan(&["check", dir.to_str().unwrap()]);
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert!(!text.contains("declares build hooks"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

// ── Watch rounds: the declaration is the wake-up set too (§9's owed pin, G10) ──
//
// The defect these close: `inputs` reached the freshness stamp and nothing
// else, so under `vilan build . --watch` an edit to a declared input produced
// ZERO rounds — the loop polled `.vl` sources only — and the hook re-ran only
// when some unrelated `.vl` save happened to wake the session. The stamp was
// right the whole time; the wake-up set was the half that had not been told.
//
// Two observables, because a round and a hook run are different events: the
// `[build] run` command counts ROUNDS (it declares nothing, so it runs on every
// one), and the declared hook counts its own runs (a round whose inputs are
// untouched finds it fresh and skips it).

/// A spawned `--watch` session, killed and reaped when it leaves scope — on the
/// panic path too, so a failing pin never leaves a polling compiler behind. It
/// kills only the child this harness spawned, by handle.
struct Watcher(Child);

impl Drop for Watcher {
    fn drop(&mut self) {
        support::kill_watcher(&mut self.0);
    }
}

/// A manifest whose `[build] run` counts rounds and whose one declared hook
/// (`inputs` given as the TOML value to write) counts its own runs.
fn watch_manifest(inputs: &str) -> String {
    format!(
        "[package]\nname = \"app\"\n\n[build]\nrun = [{}]\n\n[[build.hook]]\nname = \"gen\"\n\
         run = [{}, {}]\ninputs = {inputs}\noutputs = \"generated.txt\"\n",
        toml_string(&append("rounds.txt")),
        toml_string(&append("ran.txt")),
        toml_string(&write_line("generated.txt", "generated"))
    )
}

/// Starts `vilan build --watch` over `dir`. `build`, not `run`: the pins are
/// about the wake-up set, and a build round spawns no program, binds no port
/// and leaves no `node` grandchild to reap.
fn spawn_watch(dir: &Path) -> Watcher {
    Watcher(
        Command::new(env!("CARGO_BIN_EXE_vilan"))
            .args(["build", "--watch", dir.to_str().unwrap()])
            .env("NO_COLOR", "1")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn vilan build --watch"),
    )
}

/// Waits (bounded) for `condition`, returning how long it took. The bound is a
/// LIVENESS bound, never a performance claim: how long a compile takes on the
/// machine running the suite is not what these pins are about.
fn wait_for(label: &str, condition: impl Fn() -> bool) -> Duration {
    let started = Instant::now();
    while started.elapsed() < support::WATCH_LIVENESS {
        if condition() {
            return started.elapsed();
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("timed out waiting for {label}");
}

#[test]
fn an_edited_hook_input_starts_a_watch_round_and_reruns_the_hook() {
    // G10's headline case, as it was measured: edit a file the manifest names
    // in `inputs` and the session must round. Before the fix this waited out
    // its whole bound — the edit reached the stamp's predicate, but no round
    // ever asked the predicate anything.
    let dir = temp_project("watch_input");
    write(&dir, "vilan.toml", &watch_manifest("\"input.txt\""));
    write(&dir, "src/main.vl", MAIN);
    write(&dir, "input.txt", "one\n");
    let _watcher = spawn_watch(&dir);

    wait_for("the first round", || runs(&dir, "rounds.txt") >= 1);
    wait_for("the first round's hook", || runs(&dir, "ran.txt") >= 1);

    write(&dir, "input.txt", "two\n");
    wait_for("the round the edited input starts", || {
        runs(&dir, "rounds.txt") >= 2
    });
    wait_for("the hook the edited input re-runs", || {
        runs(&dir, "ran.txt") >= 2
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_undeclared_file_starts_no_watch_round() {
    // The other half, and the one that keeps the fix honest: the wake-up set is
    // the DECLARED inputs, not the world. A watcher that woke on every file
    // would pass the pin above and re-open the invariant only `.vl` tracking
    // has protected so far — a build that can trigger its own rebuild.
    let dir = temp_project("watch_undeclared");
    write(&dir, "vilan.toml", &watch_manifest("\"input.txt\""));
    write(&dir, "src/main.vl", MAIN);
    write(&dir, "input.txt", "one\n");
    write(&dir, "notes.txt", "not a declared input\n");
    let _watcher = spawn_watch(&dir);

    let first_round = wait_for("the first round", || runs(&dir, "rounds.txt") >= 1);

    // An undeclared, non-`.vl` file moves. Nothing may happen — and "nothing"
    // is only observable by waiting, so the window is this machine's own round
    // scaled up (E32's rule), not a guessed number.
    write(&dir, "notes.txt", "still not a declared input\n");
    let quiet = Instant::now();
    let window = support::round_budget(first_round);
    while quiet.elapsed() < window {
        assert_eq!(
            runs(&dir, "rounds.txt"),
            1,
            "an undeclared file must not start a round"
        );
        std::thread::sleep(Duration::from_millis(200));
    }

    // And the session was alive the whole time, not silently dead — otherwise
    // the assertion above is vacuous and would survive any regression.
    write(&dir, "input.txt", "two\n");
    wait_for("the round a declared input still starts", || {
        runs(&dir, "rounds.txt") >= 2
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_added_under_a_declared_directory_input_starts_a_watch_round() {
    // A declared DIRECTORY means its tree — the reading the stamp already has
    // (`inputs = ["icons"]` digests every file under it), now the watcher's
    // reading too. Watching the directory entry alone would be a different,
    // weaker set: its own mtime moves for an added entry but not for an edit
    // inside it.
    let dir = temp_project("watch_directory");
    write(&dir, "vilan.toml", &watch_manifest("[\"icons\"]"));
    write(&dir, "src/main.vl", MAIN);
    write(&dir, "icons/check.svg", "<svg/>\n");
    let _watcher = spawn_watch(&dir);

    wait_for("the first round", || runs(&dir, "rounds.txt") >= 1);
    wait_for("the first round's hook", || runs(&dir, "ran.txt") >= 1);

    write(&dir, "icons/close.svg", "<svg/>\n");
    wait_for("the round the new icon starts", || {
        runs(&dir, "rounds.txt") >= 2
    });
    wait_for("the hook the new icon re-runs", || {
        runs(&dir, "ran.txt") >= 2
    });
    let _ = std::fs::remove_dir_all(&dir);
}

// ── S6: the generated root, and the formatter's exclusion (§12) ──

/// The module a generator writes. `vilan fmt` expands a single-line body onto
/// its own line, so these exact bytes are one format away from re-staling the
/// hook that declares them — which is §12.1's loop, live in the shipped tree.
/// It is the same spelling the P6 pin above generates, deliberately: the
/// fixture that proved hooks can produce modules is the fixture that proves
/// they fight the formatter.
const GENERATED_MODULE: &str = "fun generated(): i32 { 41 }";

/// A hook command writing [`GENERATED_MODULE`] to `file`, in the platform's shell.
fn generate_module(file: &str) -> String {
    if cfg!(windows) {
        format!("echo {GENERATED_MODULE}> {file}")
    } else {
        format!("printf '{GENERATED_MODULE}\\n' > {file}")
    }
}

/// A package whose hook generates `src/icons/lib.vl`, importable as
/// `pkg::icons` — §12.6's configuration that works on the shipped resolver,
/// since a module's file may be `<root>/<name>/lib.vl` as well as
/// `<root>/<name>.vl`. `generated` is the manifest line under test; `None` is
/// the PLANT the exclusion pins are proven against, and it is also exactly what
/// this tree did before the key existed.
fn generated_root_project(tag: &str, generated: Option<&str>) -> PathBuf {
    let dir = temp_project(tag);
    let declaration = generated
        .map(|root| format!("generated = \"{root}\"\n"))
        .unwrap_or_default();
    write(
        &dir,
        "vilan.toml",
        &format!(
            "[package]\nname = \"app\"\n{declaration}\n[[build.hook]]\nname = \"icons\"\n\
             run = [{}, {}]\ninputs = \"icons.lock\"\noutputs = \"src/icons/lib.vl\"\n",
            toml_string(&append("ran.txt")),
            toml_string(&generate_module("src/icons/lib.vl"))
        ),
    );
    write(
        &dir,
        "src/main.vl",
        "import std::print;\nimport pkg::icons::generated;\n\
         fun main() { print(generated() + 1) }\nmain();\n",
    );
    write(&dir, "icons.lock", "v1\n");
    // The generator redirects into this directory, so it has to exist first —
    // a shell `>` creates the file, never its parent.
    std::fs::create_dir_all(dir.join("src/icons")).unwrap();
    dir
}

/// Formats `dir` through the built binary.
fn fmt(dir: &Path) -> Output {
    vilan(&["fmt", dir.to_str().unwrap()])
}

#[test]
fn fmt_leaves_a_file_under_the_generated_root_byte_identical() {
    // The rule itself (§12.4). The file is one the formatter demonstrably
    // WOULD rewrite — the negative below is what proves that, on the same
    // bytes in the same tree.
    let dir = generated_root_project("fmt_skips", Some("src/icons"));
    build(&dir);
    let generated = dir.join("src/icons/lib.vl");
    let before = std::fs::read(&generated).unwrap();

    let output = fmt(&dir);
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert_eq!(
        std::fs::read(&generated).unwrap(),
        before,
        "a declared product is not formatted:\n{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fmt_formats_the_same_generated_file_when_no_root_is_declared() {
    // The plant, as a pin: identical tree, `generated` absent. The formatter
    // rewrites the file, which is both today's behavior and the first half of
    // §12.1's loop — so the pin above is not vacuous, and this one records the
    // defect the key exists to close.
    let dir = generated_root_project("fmt_plant", None);
    build(&dir);
    let generated = dir.join("src/icons/lib.vl");
    let before = std::fs::read(&generated).unwrap();

    let output = fmt(&dir);
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert_ne!(
        std::fs::read(&generated).unwrap(),
        before,
        "without the declaration the formatter rewrites the product:\n{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fmt_still_formats_a_file_outside_the_generated_root() {
    // The green negative that keeps the exclusion from being "fmt stopped
    // working". One package, one declared root, two files — the one inside is
    // untouched and the one beside it is formatted, in a single run.
    let dir = generated_root_project("fmt_outside", Some("src/icons"));
    build(&dir);
    write(&dir, "src/hand_written.vl", GENERATED_MODULE);
    let generated = dir.join("src/icons/lib.vl");
    let product = std::fs::read(&generated).unwrap();

    let output = fmt(&dir);
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert_eq!(std::fs::read(&generated).unwrap(), product, "{text}");
    assert_ne!(
        std::fs::read_to_string(dir.join("src/hand_written.vl")).unwrap(),
        GENERATED_MODULE,
        "a hand-written module beside the root still formats:\n{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fmt_check_does_not_report_a_generated_file_and_exits_zero() {
    // `--check` and `fmt` are one rule because the exclusion is applied at
    // COLLECTION: a `--check` reporting a file `fmt` will never touch would be
    // a CI failure with no fix. Pinned as the fixed point — format the tree,
    // then check it — because that is the shape CI actually runs, and the
    // product is still unformatted when the check passes.
    let dir = generated_root_project("fmt_check", Some("src/icons"));
    build(&dir);
    let generated = dir.join("src/icons/lib.vl");
    let product = std::fs::read(&generated).unwrap();
    assert!(fmt(&dir).status.success());

    let output = vilan(&["fmt", "--check", dir.to_str().unwrap()]);
    let text = combined(&output);
    assert!(
        output.status.success(),
        "a formatted tree whose only unformatted file is a product is clean:\n{text}"
    );
    assert!(!text.contains("would reformat"), "{text}");
    assert_eq!(
        std::fs::read(&generated).unwrap(),
        product,
        "and the product is still exactly as the generator wrote it"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fmt_leaves_a_generated_file_alone_when_it_is_named_explicitly() {
    // §12.4's most arguable decision, pinned: the exclusion holds however the
    // file is reached. An explicit-path exception would be one the language
    // server could never honor — format-on-save reaches a file by its exact
    // path and nothing else.
    let dir = generated_root_project("fmt_explicit", Some("src/icons"));
    build(&dir);
    let generated = dir.join("src/icons/lib.vl");
    let before = std::fs::read(&generated).unwrap();

    let output = vilan(&["fmt", generated.to_str().unwrap()]);
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert_eq!(std::fs::read(&generated).unwrap(), before, "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fmt_says_once_how_many_generated_files_it_skipped() {
    // Silence is the failure mode this design cannot afford, so the exclusion
    // is announced — once per root, with a count, naming the root. Two files
    // under one root produce ONE line.
    let dir = generated_root_project("fmt_note", Some("src/icons"));
    build(&dir);
    write(&dir, "src/icons/other.vl", GENERATED_MODULE);

    let output = fmt(&dir);
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert_eq!(
        text.matches("generated files not formatted").count(),
        1,
        "one line per root, never one per file:\n{text}"
    );
    assert!(
        text.contains("2 generated files not formatted") && text.contains("icons"),
        "the note carries the count and names the root:\n{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fmt_says_nothing_when_the_exclusion_skipped_nothing() {
    // A note about an exclusion that excluded nothing is noise, and noise is
    // how the honest lines get ignored. A declared root with no `.vl` under it
    // is the clean checkout, and it says nothing.
    let dir = generated_root_project("fmt_quiet", Some("src/icons"));
    let output = fmt(&dir);
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert!(!text.contains("not formatted"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_generated_hook_output_stays_fresh_across_a_format() {
    // THE PIN THE SLICE EXISTS FOR (§12.1, §8's S6 gate). Build, format,
    // build: the hook is `Fresh`, its declared output never moved, and the
    // module it wrote still compiles. Without the declaration this cycle
    // re-runs the generator forever — which the plant below is.
    let dir = generated_root_project("loop_dead", Some("src/icons"));
    let output = vilan(&["run", dir.to_str().unwrap()]);
    let text = combined(&output);
    assert!(
        output.status.success() && text.contains("42"),
        "the generated module compiles:\n{text}"
    );
    assert_eq!(runs(&dir, "ran.txt"), 1);
    let generated = std::fs::read(dir.join("src/icons/lib.vl")).unwrap();

    let formatted = fmt(&dir);
    assert!(formatted.status.success(), "{}", combined(&formatted));
    assert_eq!(
        std::fs::read(dir.join("src/icons/lib.vl")).unwrap(),
        generated,
        "the format left the declared output alone"
    );

    let text = build(&dir);
    assert_eq!(
        runs(&dir, "ran.txt"),
        1,
        "a format does not re-stale the generator:\n{text}"
    );
    assert!(text.contains("Fresh   icons"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn without_a_generated_root_a_format_re_stales_the_hook_forever() {
    // The loop itself, pinned as the defect it is — the same cycle on the same
    // tree with `generated` absent. This is the plant for the pin above, kept
    // as a test because it is the behavior the key changes, and because a
    // future change that quietly fixed it elsewhere should be noticed.
    let dir = generated_root_project("loop_live", None);
    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 1);

    fmt(&dir);
    let text = build(&dir);
    assert_eq!(
        runs(&dir, "ran.txt"),
        2,
        "the reformat re-stales the declared output:\n{text}"
    );
    // And it never settles: the generator rewrote the file unformatted, so the
    // next format moves it again.
    fmt(&dir);
    let text = build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 3, "and again, forever:\n{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_generated_root_outside_the_package_fails_the_build_naming_the_key() {
    // The refusal reaches the user, not just `Manifest::validate` (whose own
    // pins cover the four cases, §12.3). Lexical: `../shared` never exists in
    // this tree, and the refusal does not depend on whether it could.
    let dir = generated_root_project("root_escapes", Some("../shared"));
    let output = vilan(&["build", dir.to_str().unwrap()]);
    let text = combined(&output);
    assert!(!output.status.success(), "{text}");
    assert!(
        text.contains("`[package] generated`") && text.contains("free of `..`"),
        "the refusal names the key and the rule:\n{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
