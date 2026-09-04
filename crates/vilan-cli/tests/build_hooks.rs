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
const MAIN: &str = "import std::io::print;\nfun main() { print(\"ok\") }\nmain();\n";

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
fn an_empty_directory_added_to_a_declared_tree_reruns_the_hook() {
    // G16. A directory is a MEMBER of the tree, not just a place files are
    // found in: `mkdir static/empty` is a change to what `inputs = ["static"]`
    // names, and so is removing it again. The walk pushed files and links only,
    // so both moved nothing — while the watcher, which inserts an entry per
    // nested directory, woke a round for exactly this change and handed it to a
    // predicate that said `Fresh`.
    let dir = temp_project("directory_input_empty_child");
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

    std::fs::create_dir(dir.join("static/empty")).expect("create the empty subdirectory");
    build(&dir);
    assert_eq!(
        runs(&dir, "ran.txt"),
        2,
        "an empty directory added to the tree re-runs the hook"
    );

    std::fs::remove_dir(dir.join("static/empty")).expect("remove it again");
    build(&dir);
    assert_eq!(
        runs(&dir, "ran.txt"),
        3,
        "and removing it is a change in its own right"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_directory_replaced_by_an_empty_file_reruns_the_hook() {
    // Why the row states its KIND instead of carrying a digest of nothing. Give
    // a directory the digest of the empty byte string and it produces exactly
    // the row an EMPTY FILE at the same key produces, so swapping one for the
    // other is invisible to the predicate — and that swap is not exotic, it is
    // what a generator does when a directory of parts becomes a single file.
    // Each row says what it is, so a path that changes kind is a change.
    //
    // Proven by planting the digest-of-nothing row: this goes red on it, and
    // the pin above stays green, which is what makes the two rows distinct
    // rather than merely present.
    let dir = temp_project("directory_input_kind_swap");
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
    std::fs::create_dir_all(dir.join("static/sub")).expect("an empty subdirectory");

    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 1);
    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 1, "an untouched tree is fresh");

    std::fs::remove_dir(dir.join("static/sub")).unwrap();
    write(&dir, "static/sub", "");
    build(&dir);
    assert_eq!(
        runs(&dir, "ran.txt"),
        2,
        "an empty file where an empty directory was is a change"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ── G15: a declared path that IS a symlink ──
//
// The declaration has two consumers and they have to resolve a path the same
// way. On a LINK they did not: the stamp stat'd with `symlink_metadata`, so a
// link to a directory was not `is_dir()`, fell through to `fs::read` (EISDIR),
// digested as unreadable, and the hook was stale on every build forever and
// silently — while the watcher's `insert_watched_input` followed the same link
// with `fs::metadata` and watched the tree behind it. A link to a FILE hid the
// split, because `fs::read` follows one.
//
// Creating a symlink needs a privilege Windows does not grant by default, so
// these are `cfg(unix)` like every other symlink pin in the tree; nothing they
// pin is platform-specific.

#[cfg(unix)]
#[test]
fn a_declared_directory_input_reached_through_a_symlink_stays_fresh() {
    // G15's own repro, as measured: three builds over `inputs =
    // "linked_static"` ran the hook three times, where the same tree declared
    // by its real name ran it once. The last two assertions are the ones that
    // were red.
    let dir = temp_project("directory_input_link");
    write(
        &dir,
        "vilan.toml",
        &format!(
            "[package]\nname = \"app\"\n\n[[build.hook]]\nname = \"copy\"\nrun = {}\n\
             inputs = \"linked_static\"\n",
            toml_string(&append("ran.txt"))
        ),
    );
    write(&dir, "src/main.vl", MAIN);
    write(&dir, "static/a.txt", "a\n");
    std::os::unix::fs::symlink("static", dir.join("linked_static"))
        .expect("link the declared name at the real directory");

    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 1);
    build(&dir);
    assert_eq!(
        runs(&dir, "ran.txt"),
        1,
        "a declared link to a directory digests as that directory's tree"
    );
    build(&dir);
    assert_eq!(
        runs(&dir, "ran.txt"),
        1,
        "and stays fresh, build after build"
    );

    // And it is fresh rather than frozen: the tree behind the link is still
    // the content, so a change through it re-runs the hook.
    write(&dir, "static/a.txt", "changed\n");
    build(&dir);
    assert_eq!(
        runs(&dir, "ran.txt"),
        2,
        "an edit behind the link is an edit to the declared input"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn a_symlink_inside_a_declared_tree_is_digested_unfollowed() {
    // The fence the fix above must not breach. The TOP-LEVEL declared path is
    // resolved through a link; a link found INSIDE the tree is not, and digests
    // as its own target path — otherwise the walk could leave the declared tree
    // or run into a cycle.
    //
    // Both halves are sharp because the fixture makes following and not
    // following disagree: `a.txt` and `b.txt` hold identical bytes, so
    // re-pointing the link is invisible to a walk that follows it and is a
    // change to one that reads the link itself.
    let dir = temp_project("tree_link");
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
    write(&dir, "static/a.txt", "same\n");
    write(&dir, "static/b.txt", "same\n");
    write(&dir, "outside.txt", "one\n");
    std::os::unix::fs::symlink("a.txt", dir.join("static/link")).expect("link inside the tree");
    std::os::unix::fs::symlink("../outside.txt", dir.join("static/escape"))
        .expect("link out of the tree");

    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 1);
    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 1, "an untouched tree is fresh");

    std::fs::remove_file(dir.join("static/link")).unwrap();
    std::os::unix::fs::symlink("b.txt", dir.join("static/link")).expect("re-point the link");
    build(&dir);
    assert_eq!(
        runs(&dir, "ran.txt"),
        2,
        "the link's target PATH is its content: re-pointing it at a \
         byte-identical file is still a change"
    );

    write(&dir, "outside.txt", "two\n");
    build(&dir);
    assert_eq!(
        runs(&dir, "ran.txt"),
        2,
        "and the tree does not extend through a link that leaves it"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn a_declared_input_that_is_a_broken_link_reads_as_missing() {
    // A link to nothing is the same forever-loop one level down, and the same
    // resolution closes it: the declared path resolves through the link, so a
    // link with no target is a path that is not there — recorded as missing,
    // fresh on the next build, and invalidated when the target appears. It used
    // to be "unreadable", which is never fresh and never explained.
    let dir = declared_hook_project("input_broken_link");
    std::fs::remove_file(dir.join("input.txt")).unwrap();
    std::os::unix::fs::symlink("target.txt", dir.join("input.txt")).expect("link at nothing");

    build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 1);
    let second = build(&dir);
    assert_eq!(
        runs(&dir, "ran.txt"),
        1,
        "a link to nothing is a missing input, not an unreadable one:\n{second}"
    );

    write(&dir, "target.txt", "one\n");
    build(&dir);
    assert_eq!(
        runs(&dir, "ran.txt"),
        2,
        "and the target appearing is the input appearing"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn a_declared_output_that_is_a_broken_link_is_reported_as_not_written() {
    // The other side of the same reading, and the reason it is the right one:
    // an output that is a link to nothing is an output the build cannot use, so
    // it is the "did not write it" note rather than silence. Read as
    // "unreadable" it was neither recorded nor reported — the hook re-ran on
    // every build with nothing said, which is the exact failure the note exists
    // to prevent.
    let dir = temp_project("output_broken_link");
    write(
        &dir,
        "vilan.toml",
        &format!(
            "[package]\nname = \"app\"\n\n[[build.hook]]\nname = \"gen\"\nrun = [{}, {}]\n\
             inputs = \"input.txt\"\noutputs = \"generated.txt\"\n",
            toml_string(&append("ran.txt")),
            toml_string("ln -sf nowhere.txt generated.txt")
        ),
    );
    write(&dir, "src/main.vl", MAIN);
    write(&dir, "input.txt", "one\n");

    let first = build(&dir);
    assert_eq!(runs(&dir, "ran.txt"), 1, "the hook ran:\n{first}");
    assert!(
        first.contains("`gen`") && first.contains("generated.txt"),
        "an output that links to nothing is named as not written:\n{first}"
    );
    let second = build(&dir);
    assert_eq!(
        runs(&dir, "ran.txt"),
        2,
        "and nothing is stamped for it, so the hook re-runs:\n{second}"
    );
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
        "import std::io::print;\nimport pkg::generated::generated;\n\
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

/// [`watch_manifest`] over `input.txt`, plus one more `[build] run` command —
/// the one whose failure the retry pins are about (G14).
///
/// Its position in the round is the whole fixture: it runs AFTER the round
/// counter, so a round is counted whether or not it goes on to fail, and BEFORE
/// the declared hook, which a failed round therefore never reaches. That makes
/// `ran.txt` the observable for "the round completed" and `rounds.txt` the one
/// for "a round started".
fn watch_manifest_failing(command: &str) -> String {
    format!(
        "[package]\nname = \"app\"\n\n[build]\nrun = [{}, {}]\n\n[[build.hook]]\nname = \"gen\"\n\
         run = [{}, {}]\ninputs = \"input.txt\"\noutputs = \"generated.txt\"\n",
        toml_string(&append("rounds.txt")),
        toml_string(command),
        toml_string(&append("ran.txt")),
        toml_string(&write_line("generated.txt", "generated"))
    )
}

/// A hook command that takes a few seconds, in the platform's shell — the
/// widened round B208's queued-during-a-round pin needs, so an edit the test
/// makes the moment it sees the hook START lands INSIDE the round rather than
/// after it. `ping` rather than `timeout` on Windows: `timeout` refuses to run
/// without a console, which a spawned hook does not have.
fn slow_command() -> String {
    if cfg!(windows) {
        "ping -n 4 127.0.0.1 >nul".to_string()
    } else {
        "sleep 3".to_string()
    }
}

/// A hook command that fails ONCE and then succeeds: while `marker` exists it
/// removes it and exits non-zero, so the very next invocation passes. The
/// transient failure G14 is about, made deterministic — no sleeps, no load
/// dependence, and armed by the test at the moment it chooses.
fn fail_once(marker: &str) -> String {
    if cfg!(windows) {
        format!("if exist {marker} (del {marker} & exit 1)")
    } else {
        format!("if [ -f {marker} ]; then rm {marker}; exit 1; fi")
    }
}

/// A hook command that fails EVERY time `marker` exists, counting each
/// invocation in `attempts` first. Counting inside the failing command is what
/// makes "exactly twice per change" observable: the round and its one retry
/// both reach it, and nothing else does.
fn fail_while(marker: &str, attempts: &str) -> String {
    if cfg!(windows) {
        format!("if exist {marker} (echo ran>> {attempts} & exit 1)")
    } else {
        format!("if [ -f {marker} ]; then printf 'ran\\n' >> {attempts}; exit 1; fi")
    }
}

/// Starts `vilan build --watch` over `dir`. `build`, not `run`: the pins are
/// about the wake-up set, and a build round spawns no program, binds no port
/// and leaves no `node` grandchild to reap. The loop's narration (banner,
/// `change detected`, `Running` echoes, any error) lands in `watch.log`
/// beside the project — NOT nulled, because the one Windows CI red this
/// section has had was undiagnosable precisely for want of that log; the
/// name is not `.vl` and not a declared input, so the watched set is
/// unperturbed.
///
/// `VILAN_WATCH_LOG` turns on the loop's own trace beside it (B208): the
/// narration says a round happened, and only the trace says what the POLL saw
/// — which entries moved, which did not, and whether the loop was polling at
/// all. N46's strike was "round 2 never fired", a symptom four different bugs
/// share, and it was unfalsifiable without this. Both files are named so they
/// are neither `.vl` nor a declared input, so the watched set is unperturbed.
fn spawn_watch(dir: &Path) -> Watcher {
    let log = std::fs::File::create(dir.join("watch.log")).expect("create watch.log");
    Watcher(
        Command::new(env!("CARGO_BIN_EXE_vilan"))
            .args(["build", "--watch", dir.to_str().unwrap()])
            .env("NO_COLOR", "1")
            .env("VILAN_WATCH_LOG", dir.join("watch-trace.log"))
            .stdout(Stdio::null())
            .stderr(Stdio::from(log))
            .spawn()
            .expect("spawn vilan build --watch"),
    )
}

/// Waits (bounded) for `condition`, returning how long it took. The bound is a
/// LIVENESS bound, never a performance claim: how long a compile takes on the
/// machine running the suite is not what these pins are about. On expiry the
/// panic carries the whole observable state — the round and hook counts and
/// the watcher's own log — so a red on a runner nobody can shell into is a
/// verdict rather than a mystery (the first Windows red here cost a blind
/// diagnosis for want of exactly this).
fn wait_for_in(dir: &Path, label: &str, condition: impl Fn() -> bool) -> Duration {
    wait_nudged(dir, label, |_attempt| {}, condition)
}

/// [`wait_for_in`], re-invoking `nudge` every ~20 s while it waits. A one-shot
/// trigger that goes unheard for any reason hangs the full bound on an
/// otherwise healthy watcher (the flake-shaped Windows red, hypothesis H-A).
/// Re-touching the trigger makes the positive pins immune to a single lost
/// round without weakening them: if the wake-up set is wrong, no amount of
/// re-touching wakes it.
///
/// The specific loss that motivated it — a round eaten by a transient action
/// failure, the difference consumed before the action ran — is now the loop's
/// own business (G14: a failed round keeps its change and re-fires once), so
/// this is belt-and-braces against the shapes that remain (a filesystem whose
/// mtime granularity swallows a rewrite, a snapshot read racing a write).
///
/// **The nudge is handed its attempt number, and every file-writing caller
/// uses it** (B208). A nudge that rewrote IDENTICAL BYTES could rescue a lost
/// round only at the mtime gate, because that is the only gate identical bytes
/// move: the watcher polls mtimes, but the freshness stamp digests CONTENT, so
/// a hook whose stamp had swallowed the edit stayed `Fresh` through every
/// re-touch — rounds firing, the hook never running, and re-touching provably
/// unable to help however long the bound was. That is the shape of the strike
/// B208 was filed on, and a rescue that cannot reach the second gate is a
/// rescue that hides which gate failed. Writing new bytes each time re-triggers
/// both and weakens nothing: every pin here asserts a COUNT of runs, never the
/// content of the file it counted.
///
/// Two callers never use it, each for its own reason: the negative pin, whose
/// whole claim is that nothing fires, and the retry pins, whose claim is that
/// the RETRY landed the change — a re-touch there would start a fresh round
/// and prove nothing.
fn wait_nudged(
    dir: &Path,
    label: &str,
    nudge: impl Fn(u32),
    condition: impl Fn() -> bool,
) -> Duration {
    let started = Instant::now();
    let mut last_nudge = Instant::now();
    let mut attempt = 0;
    while started.elapsed() < support::WATCH_LIVENESS {
        if condition() {
            return started.elapsed();
        }
        if last_nudge.elapsed() > Duration::from_secs(20) {
            attempt += 1;
            nudge(attempt);
            last_nudge = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let log = std::fs::read_to_string(dir.join("watch.log")).unwrap_or_default();
    let trace = std::fs::read_to_string(dir.join("watch-trace.log")).unwrap_or_default();
    panic!(
        "timed out waiting for {label}\nrounds.txt: {} lines, ran.txt: {} lines\n         --- watch.log ---\n{log}\n--- watch-trace.log (VILAN_WATCH_LOG, B208) ---\n{trace}",
        runs(dir, "rounds.txt"),
        runs(dir, "ran.txt"),
    );
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

    wait_for_in(&dir, "the first round", || runs(&dir, "rounds.txt") >= 1);
    wait_for_in(&dir, "the first round's hook", || {
        runs(&dir, "ran.txt") >= 1
    });

    write(&dir, "input.txt", "two\n");
    wait_nudged(
        &dir,
        "the round the edited input starts",
        |attempt| write(&dir, "input.txt", &format!("two {attempt}\n")),
        || runs(&dir, "rounds.txt") >= 2,
    );
    wait_for_in(&dir, "the hook the edited input re-runs", || {
        runs(&dir, "ran.txt") >= 2
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_edit_landing_while_a_round_runs_starts_exactly_one_more_round() {
    // B208. The loop reads its snapshot BEFORE the action and consumes the
    // difference only when the round succeeds (E20's rule), so an edit made
    // while a round is compiling is still a difference at the next poll. That is
    // the *loop's* half. The other half is the freshness STAMP, which digests a
    // hook's declared inputs — and recorded them AFTER the hook ran until
    // Order 25's seal, so an edit landing between the hook's last command and
    // that re-hash was stamped as already consumed: the round fired, the stamp
    // said `Fresh`, and the edit was gone. Re-touching could not rescue it,
    // because a re-touch of the same bytes moves the mtime and not the digest.
    //
    // So the pin measures BOTH observables across one edit made mid-round: a
    // ROUND started (`rounds.txt`, the undeclared `[build] run`) and the HOOK
    // re-ran (`ran.txt`). And exactly one more of each — the edit must not
    // start a cascade either.
    //
    // The hook's middle command is deliberately slow, so the edit the test
    // makes on seeing `ran.txt` lands while round 1 is still executing the
    // hook. The claim holds whichever side of the round's end the edit lands
    // on, so a box too loaded to place it inside cannot make this flake; the
    // assertion below records which case ran.
    let dir = temp_project("watch_edit_during_round");
    write(
        &dir,
        "vilan.toml",
        &format!(
            "[package]\nname = \"app\"\n\n[build]\nrun = [{}]\n\n[[build.hook]]\nname = \"gen\"\n\
             run = [{}, {}, {}]\ninputs = \"input.txt\"\noutputs = \"generated.txt\"\n",
            toml_string(&append("rounds.txt")),
            toml_string(&append("ran.txt")),
            toml_string(&slow_command()),
            toml_string(&write_line("generated.txt", "generated")),
        ),
    );
    write(&dir, "src/main.vl", MAIN);
    write(&dir, "input.txt", "one\n");
    let _watcher = spawn_watch(&dir);

    // `ran.txt` is the hook's FIRST command, so seeing it means the round is
    // inside the hook and has not reached the stamp.
    let first_round = wait_for_in(&dir, "the first round's hook", || {
        runs(&dir, "ran.txt") >= 1
    });
    write(&dir, "input.txt", "two\n");
    // Round 1 writes `generated.txt` as its LAST hook command, so its absence
    // right now is the proof the edit landed mid-round.
    let landed_mid_round = !dir.join("generated.txt").exists();

    // Not nudged, and that is the whole pin: one edit, one round, one hook run.
    // A re-touch would start a fresh round and prove nothing about the first.
    wait_for_in(&dir, "the round the mid-round edit starts", || {
        runs(&dir, "rounds.txt") >= 2
    });
    wait_for_in(&dir, "the hook the mid-round edit re-runs", || {
        runs(&dir, "ran.txt") >= 2
    });
    assert!(
        landed_mid_round,
        "the edit was meant to land while round 1 was still running its hook — \
         `generated.txt` already existed, so this run measured the ordinary \
         between-rounds edit instead. The counts above still hold; only the \
         mid-round case went unexercised."
    );

    // EXACTLY one more: the difference is consumed by the round that dealt with
    // it, so nothing cascades. The window is this machine's own round scaled up
    // (E32's rule), and it has to outlast the hook's own slow command.
    let quiet = Instant::now();
    let window = support::round_budget(first_round);
    while quiet.elapsed() < window {
        assert_eq!(
            runs(&dir, "rounds.txt"),
            2,
            "one edit is one round: a second round here is a difference the \
             successful round failed to consume"
        );
        assert_eq!(runs(&dir, "ran.txt"), 2, "and one hook run");
        std::thread::sleep(Duration::from_millis(200));
    }
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

    let first_round = wait_for_in(&dir, "the first round", || runs(&dir, "rounds.txt") >= 1);

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
    wait_for_in(&dir, "the round a declared input still starts", || {
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

    wait_for_in(&dir, "the first round", || runs(&dir, "rounds.txt") >= 1);
    wait_for_in(&dir, "the first round's hook", || {
        runs(&dir, "ran.txt") >= 1
    });

    write(&dir, "icons/close.svg", "<svg/>\n");
    wait_nudged(
        &dir,
        "the round the new icon starts",
        |attempt| {
            write(
                &dir,
                "icons/close.svg",
                &format!("<svg id=\"{attempt}\"/>\n"),
            )
        },
        || runs(&dir, "rounds.txt") >= 2,
    );
    wait_for_in(&dir, "the hook the new icon re-runs", || {
        runs(&dir, "ran.txt") >= 2
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_empty_directory_added_under_a_declared_input_rounds_and_reruns_the_hook() {
    // G16, and the reason it is one item and not two: the two consumers have to
    // AGREE about `mkdir icons/empty`, and either answer would have been an
    // answer as long as both gave it. They gave different ones. The watcher
    // inserts an entry per nested directory, so the round fired; the stamp
    // pushed files and links only, so the round it fired judged the hook Fresh
    // and did nothing at all. A round that reliably does nothing is not a safe
    // failure — it is the freshness gate answering a reading of the manifest
    // that the thing which woke it does not share.
    //
    // Measured before the fix on this fixture: `rounds.txt` reached 2 within a
    // second and `ran.txt` sat at 1 for the whole liveness bound.
    let dir = temp_project("watch_empty_nested_directory");
    write(&dir, "vilan.toml", &watch_manifest("[\"icons\"]"));
    write(&dir, "src/main.vl", MAIN);
    write(&dir, "icons/check.svg", "<svg/>\n");
    let _watcher = spawn_watch(&dir);

    wait_for_in(&dir, "the first round", || runs(&dir, "rounds.txt") >= 1);
    wait_for_in(&dir, "the first round's hook", || {
        runs(&dir, "ran.txt") >= 1
    });

    std::fs::create_dir(dir.join("icons/empty")).expect("create the empty subdirectory");
    wait_nudged(
        &dir,
        "the round the empty subdirectory starts",
        // The re-touch N30's pin uses, for the same reason: remove and
        // re-create is the only edit an empty directory has, and either half is
        // a difference on its own. The one nudge with no CONTENT to vary
        // (B208's rule above): an empty directory has none, and its identity
        // moving is what both consumers key on.
        |_attempt| {
            let _ = std::fs::remove_dir(dir.join("icons/empty"));
            let _ = std::fs::create_dir(dir.join("icons/empty"));
        },
        || runs(&dir, "rounds.txt") >= 2,
    );
    wait_for_in(&dir, "the hook the empty subdirectory re-runs", || {
        runs(&dir, "ran.txt") >= 2
    });
    let _ = std::fs::remove_dir_all(&dir);
}

// ── A failed round keeps its change, and is retried exactly once (G14) ──
//
// The defect these close: the loop consumed a snapshot difference BEFORE it ran
// the action (`snapshot = next; action()`), so a round whose action failed for
// any transient reason — a `cmd` hiccup on a loaded runner, a hook racing
// something outside the tree — could never re-fire. The difference was spent and
// the session sat healthy and silent until some unrelated file moved.
//
// The ruling (2026-08-29) is a restored difference and ONE retry, so the two
// pins are the two halves of that sentence: the transient failure loses no
// round, and the persistent one costs exactly two runs and then rests.

#[test]
fn a_transiently_failing_round_keeps_its_change_and_retries() {
    // Arm a `[build] run` command to fail exactly once, edit a declared input,
    // and the hook still runs for that edit: the round the failure ate is given
    // back by the retry.
    //
    // The pin's claim rests on NOT re-touching the trigger — `wait_for_in`
    // never nudges (unlike `wait_nudged`, which the positive G10 pins above
    // use), so the only thing that can produce the second hook run is the loop
    // re-firing a difference it kept. On the unfixed tree this waits out its
    // whole bound.
    let dir = temp_project("watch_transient_failure");
    write(
        &dir,
        "vilan.toml",
        &watch_manifest_failing(&fail_once("fail-once.txt")),
    );
    write(&dir, "src/main.vl", MAIN);
    write(&dir, "input.txt", "one\n");
    let _watcher = spawn_watch(&dir);

    // The hook, not the round counter: `ran.txt` is written after the failing
    // command, so seeing it proves round 1 got past the command before the test
    // arms it.
    wait_for_in(&dir, "the first round", || runs(&dir, "rounds.txt") >= 1);
    wait_for_in(&dir, "the first round's hook", || {
        runs(&dir, "ran.txt") >= 1
    });

    // Arm the failure, then make the ONE edit whose round it eats. The marker is
    // neither a `.vl` file nor a declared input, so arming starts no round of
    // its own — which is exactly what the negative pin above proves.
    write(&dir, "fail-once.txt", "fail the next round\n");
    write(&dir, "input.txt", "two\n");

    wait_for_in(&dir, "the hook the retried round re-runs", || {
        runs(&dir, "ran.txt") >= 2
    });
    // The failure really happened — otherwise the pin would be green on a tree
    // where nothing was ever retried, and the marker is how that is visible: the
    // command deletes it on the invocation it fails.
    assert!(
        !dir.join("fail-once.txt").exists(),
        "the armed command must have run and failed once"
    );
    // Two rounds' worth of the counter for one edit: the round that failed and
    // the retry that succeeded (plus round 1).
    assert!(
        runs(&dir, "rounds.txt") >= 3,
        "the failed round and its retry are two rounds: {}",
        runs(&dir, "rounds.txt")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_persistently_failing_round_runs_twice_and_then_waits() {
    // The guard, without which the pin above would describe a spin: a command
    // that fails every time runs the round and ONE retry per change, and then
    // the session goes quiet until the next change — it does not hammer the
    // failing tree once per poll.
    let dir = temp_project("watch_persistent_failure");
    write(
        &dir,
        "vilan.toml",
        &watch_manifest_failing(&fail_while("armed.txt", "attempts.txt")),
    );
    write(&dir, "src/main.vl", MAIN);
    write(&dir, "input.txt", "one\n");
    let _watcher = spawn_watch(&dir);

    let first_round = wait_for_in(&dir, "the first round", || runs(&dir, "rounds.txt") >= 1);
    wait_for_in(&dir, "the first round's hook", || {
        runs(&dir, "ran.txt") >= 1
    });

    write(&dir, "armed.txt", "fail every round\n");
    write(&dir, "input.txt", "two\n");
    wait_for_in(&dir, "the failed round and its one retry", || {
        runs(&dir, "attempts.txt") >= 2
    });

    // Exactly two, held across a window scaled to this machine's own round
    // (E32's rule): a third attempt would mean the difference was never
    // consumed, which is the hot loop the once-only guard exists to prevent.
    let quiet = Instant::now();
    let window = support::round_budget(first_round);
    while quiet.elapsed() < window {
        assert_eq!(
            runs(&dir, "attempts.txt"),
            2,
            "a failing round is retried once, never spun"
        );
        std::thread::sleep(Duration::from_millis(200));
    }

    // And the session is alive rather than wedged on the failure — otherwise the
    // window above is vacuous. Disarm, edit, and the next change rounds normally.
    std::fs::remove_file(dir.join("armed.txt")).expect("disarm the failing command");
    write(&dir, "input.txt", "three\n");
    wait_for_in(&dir, "the round a later change still starts", || {
        runs(&dir, "ran.txt") >= 2
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_declared_directory_input_appearing_empty_starts_a_watch_round() {
    // N30. A declared directory that does not exist yet is the ordinary state
    // of a generated tree, and CREATING it is the change the build is waiting
    // for — `asset::read_dir` failed on it and recorded the miss, and the same
    // call against an empty directory succeeds. But the watched set expanded a
    // directory to its FILES and inserted nothing for the directory itself, so
    // an empty one contributed no entry: the appearance was invisible and the
    // session sat still until somebody happened to put a file in it.
    //
    // Measured before the fix on this fixture: round 1, then `mkdir icons` and
    // nothing at all, then the first file inside it firing round 2 — the narrow
    // shape the item describes, with the empty step the only silent one.
    let dir = temp_project("watch_empty_directory");
    write(&dir, "vilan.toml", &watch_manifest("[\"icons\"]"));
    write(&dir, "src/main.vl", MAIN);
    // No `icons/` — the directory is declared and missing, which builds fine.
    let _watcher = spawn_watch(&dir);

    wait_for_in(&dir, "the first round", || runs(&dir, "rounds.txt") >= 1);
    wait_for_in(&dir, "the first round's hook", || {
        runs(&dir, "ran.txt") >= 1
    });

    std::fs::create_dir(dir.join("icons")).expect("create the empty declared directory");
    wait_nudged(
        &dir,
        "the round the appearing EMPTY directory starts",
        // The re-touch a lost round needs, in the one form available to a
        // directory with nothing in it: remove and re-create. Either half is a
        // snapshot difference on its own, so a poll landing between them is
        // fine, and a missing declared input builds cleanly. No CONTENT to vary
        // (B208's rule on `wait_nudged`): an empty directory has none.
        |_attempt| {
            let _ = std::fs::remove_dir(dir.join("icons"));
            let _ = std::fs::create_dir(dir.join("icons"));
        },
        || runs(&dir, "rounds.txt") >= 2,
    );
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
        "import std::io::print;\nimport pkg::icons::generated;\n\
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

// ── S7: symlinks are project layout, not an escape (G17/G18/G19) ──
//
// The doctrine the owner ruled (G19, and `const.md` §9.2): a symlink is a
// SUPPORTED SPELLING of project layout. Resolve links honestly, apply every
// fence and scope to the RESOLVED tree, guard against cycles — and never say a
// link is illegitimate. These pins are the terminal half of that; the predicate
// itself is pinned over paths in `vilan-core::manifest`, and the editor half in
// `vilan-lsp`.
//
// All `cfg(unix)`, like every other symlink pin in the tree: creating a link
// needs a privilege Windows does not grant by default. The FIXES are
// platform-neutral — the Windows half of symlink behavior is chartered for
// audit run 7 / Order 24, and nothing here is a statement about it.

/// G17's tree: the package's declared `generated` root is a LINK to a tree
/// outside it, which is how a shared icon set or a sibling generator's output
/// gets its name inside a package. Returns `(outer, package)` — the products
/// live at `outer/outside/icons`, reachable as `package/src/icons`.
#[cfg(unix)]
fn symlinked_generated_project(tag: &str) -> (PathBuf, PathBuf) {
    let outer = temp_project(tag);
    let package = outer.join("package");
    std::fs::create_dir_all(outer.join("outside/icons")).unwrap();
    std::fs::create_dir_all(package.join("src")).unwrap();
    write(
        &package,
        "vilan.toml",
        &format!(
            "[package]\nname = \"app\"\ngenerated = \"src/icons\"\n\n[[build.hook]]\n\
             name = \"icons\"\nrun = [{}, {}]\ninputs = \"icons.lock\"\n\
             outputs = \"src/icons/lib.vl\"\n",
            toml_string(&append("ran.txt")),
            toml_string(&generate_module("src/icons/lib.vl"))
        ),
    );
    write(
        &package,
        "src/main.vl",
        "import std::io::print;\nimport pkg::icons::generated;\n\
         fun main() { print(generated() + 1) }\nmain();\n",
    );
    write(&package, "icons.lock", "v1\n");
    std::os::unix::fs::symlink("../../outside/icons", package.join("src/icons")).unwrap();
    (outer, package)
}

#[cfg(unix)]
#[test]
fn fmt_leaves_a_product_under_a_symlinked_generated_root_alone() {
    // G17, audit run 6's F5: the containment check missed through a link and
    // the exclusion FAILED OPEN — `vilan fmt` rewrote the product, re-staling
    // the hook that digests it, which is §12.1's loop live again. Both spellings
    // are pinned, because both consumers exist: the walk that finds the file
    // under the directory, and the explicit path (the shape format-on-save
    // reaches a file by).
    let (outer, package) = symlinked_generated_project("fmt_link");
    build(&package);
    let product = package.join("src/icons/lib.vl");
    let before = std::fs::read(&product).unwrap();

    let output = fmt(&package);
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert_eq!(
        std::fs::read(&product).unwrap(),
        before,
        "a product behind a symlinked root is not formatted:\n{text}"
    );
    assert!(
        text.contains("generated file") && text.contains("not formatted"),
        "and the exclusion says so — silence is how the loop came back:\n{text}"
    );

    let named = vilan(&["fmt", product.to_str().unwrap()]);
    let text = combined(&named);
    assert!(named.status.success(), "{text}");
    assert_eq!(
        std::fs::read(&product).unwrap(),
        before,
        "however the file is reached, links included:\n{text}"
    );
    let _ = std::fs::remove_dir_all(&outer);
}

#[cfg(unix)]
#[test]
fn a_hook_writing_through_a_symlinked_generated_root_stays_fresh_across_a_format() {
    // The loop itself, over a link: build, format, build. This is the pin the
    // FIX exists for — without it the second build re-runs the generator, and
    // keeps re-running it forever.
    let (outer, package) = symlinked_generated_project("loop_link");
    build(&package);
    assert_eq!(runs(&package, "ran.txt"), 1);

    assert!(fmt(&package).status.success());
    let text = build(&package);
    assert_eq!(
        runs(&package, "ran.txt"),
        1,
        "a format does not re-stale a generator writing through a link:\n{text}"
    );
    assert!(text.contains("Fresh   icons"), "{text}");
    let _ = std::fs::remove_dir_all(&outer);
}

/// A package with a directory link inside it, for G18's walk pins. `link` is
/// the link's name under `src`, `target` its (relative) target.
#[cfg(unix)]
fn linked_walk_project(tag: &str, links: &[(&str, &str)]) -> PathBuf {
    let dir = temp_project(tag);
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    // Deliberately unformatted, so "the walk finished" and "the walk found it"
    // are distinguishable from the output alone.
    write(&dir, "src/main.vl", "fun  main( ) { }\n");
    for (link, target) in links {
        std::os::unix::fs::symlink(target, dir.join("src").join(link)).unwrap();
    }
    dir
}

#[cfg(unix)]
#[test]
fn fmt_terminates_on_a_directory_cycle_and_reports_each_file_once() {
    // G18, audit run 6's F6. `src/l1 -> .` with a second link beside it fans the
    // walk out exponentially and `vilan fmt --check` never returns — ZERO output,
    // because the report comes after collection. One link alone did return, but
    // only because the kernel's own ELOOP stopped it at forty levels, after
    // reporting the same file forty-one times.
    //
    // The timeout is the instrument: this pin must prove the hang is GONE, and
    // "the test passed" is not that proof if it could pass by hanging the
    // harness. Generous (60 s against a walk that now visits three directories)
    // because the suite runs it under full lane load.
    let dir = linked_walk_project("cycle", &[("l1", "."), ("l2", ".")]);
    let started = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["fmt", "--check", dir.to_str().unwrap()])
        .env("NO_COLOR", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run vilan fmt");
    let output = loop {
        match child.try_wait().expect("wait on vilan fmt") {
            Some(_) => break child.wait_with_output().expect("collect vilan fmt"),
            None if started.elapsed() > Duration::from_secs(60) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("`vilan fmt --check` did not terminate on a directory cycle");
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    };
    let text = combined(&output);
    assert_eq!(
        text.matches("would reformat").count(),
        1,
        "one file, reported once — a cycle re-walked is the same file under \
         another name:\n{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn fmt_stays_inside_the_project_tree_and_says_where_it_stopped() {
    // G18's other half: an ordinary directory link ESCAPES the package, so
    // `vilan fmt .` rewrote files in someone else's tree — measured, not
    // theorized. The scope is the resolved PROJECT tree (G19's doctrine: fences
    // apply to the resolved tree), and the walk says which link it stopped at
    // rather than skipping in silence.
    let outer = temp_project("escape");
    let package = outer.join("package");
    std::fs::create_dir_all(outer.join("outside")).unwrap();
    std::fs::create_dir_all(package.join("src")).unwrap();
    write(&package, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&package, "src/main.vl", "fun  main( ) { }\n");
    let stranger = outer.join("outside/other.vl");
    std::fs::write(&stranger, "fun  other( ) { }\n").unwrap();
    std::os::unix::fs::symlink("../../outside", package.join("src/linked")).unwrap();

    let output = vilan(&["fmt", package.to_str().unwrap()]);
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert_eq!(
        std::fs::read_to_string(&stranger).unwrap(),
        "fun  other( ) { }\n",
        "a tree outside the project is not this command's to rewrite:\n{text}"
    );
    assert_ne!(
        std::fs::read_to_string(package.join("src/main.vl")).unwrap(),
        "fun  main( ) { }\n",
        "while the package's own source is formatted as always:\n{text}"
    );
    assert!(
        text.contains("linked") && text.contains("outside this project"),
        "the note names the link and the reason, and calls neither illegitimate:\n{text}"
    );
    let _ = std::fs::remove_dir_all(&outer);
}

#[cfg(unix)]
#[test]
fn fmt_follows_a_directory_link_that_stays_inside_the_project() {
    // The green negative that keeps the scope from reading as "links are
    // refused". A link to a sibling tree INSIDE the project is ordinary layout
    // and is walked — G19's ruling, in the one place it is observable.
    let dir = temp_project("inside_link");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "src/main.vl", "fun main() {}\n");
    write(&dir, "shared/helper.vl", "fun  helper( ) { }\n");
    std::os::unix::fs::symlink("../shared", dir.join("src/shared")).unwrap();

    let output = vilan(&["fmt", dir.to_str().unwrap()]);
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert_eq!(
        std::fs::read_to_string(dir.join("shared/helper.vl")).unwrap(),
        "fun helper() {}\n",
        "a link inside the project is layout, and its tree formats:\n{text}"
    );
    assert!(
        !text.contains("outside this project"),
        "and nothing is said about it:\n{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn fmt_counts_a_file_reached_under_two_names_once() {
    // G22. G18 gave the walk a cycle guard keyed on DIRECTORY identity, which
    // is the whole question for a cycle and only half of it for a link: a
    // filesystem hands one FILE to a walk under as many names as point at it,
    // and the collector took every one. `vilan fmt --check` then printed two
    // `would reformat` lines for one file and counted it twice (so the exit
    // code was right for the wrong reason), and `vilan fmt` formatted it twice.
    //
    // Two link shapes in one tree, and they are not the same measurement:
    //
    //   * a DIRECTORY link inside the project (`src/shared -> ../shared`) —
    //     supported layout, walked since G19. G18's guard already covers it,
    //     because the second name reaches a directory it has seen; this half is
    //     the CONTROL that says the new guard did not break the old one.
    //   * a FILE link beside its target (`src/alias.vl -> real.vl`) — one file,
    //     two names, in one directory, and no directory-keyed guard can see it.
    //     This is G22, and it is the half that was red: the old walk reported
    //     `src/alias.vl` and `src/real.vl` as two files.
    //
    // Two distinct files need formatting, so the pin is not "reports once" — it
    // is "reports each FILE once", and a guard that collapsed the two real files
    // into one would fail it exactly as the missing guard did.
    let dir = temp_project("two_names");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "src/main.vl", "fun main() {}\n");
    write(&dir, "shared/helper.vl", "fun  helper( ) { }\n");
    write(&dir, "src/real.vl", "fun  real( ) { }\n");
    std::os::unix::fs::symlink("../shared", dir.join("src/shared"))
        .expect("a directory link inside the project");
    std::os::unix::fs::symlink("real.vl", dir.join("src/alias.vl"))
        .expect("a file link beside its target");

    let output = vilan(&["fmt", "--check", dir.to_str().unwrap()]);
    let text = combined(&output);

    assert_eq!(
        text.matches("would reformat").count(),
        2,
        "two files need formatting and there are two of them however many names \
         reach them — one through a directory link, one through a file link:\n{text}"
    );
    assert!(
        text.contains("helper.vl"),
        "the file under the linked directory is one of the two:\n{text}"
    );
    assert!(
        text.contains("real.vl") || text.contains("alias.vl"),
        "and the doubly-named file is the other, under whichever name the walk \
         reached first:\n{text}"
    );
    assert!(
        !output.status.success(),
        "`--check` still fails when something would be reformatted:\n{text}"
    );

    // The rewrite agrees with the count: one file, formatted once, and reachable
    // as formatted under BOTH names — the link is layout, not a second file.
    let rewrite = fmt(&dir);
    let rewritten = combined(&rewrite);
    assert!(rewrite.status.success(), "{rewritten}");
    assert_eq!(
        rewritten.matches("formatted").count(),
        2,
        "the rewrite formats each file once:\n{rewritten}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("src/real.vl")).unwrap(),
        "fun real() {}\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("src/alias.vl")).unwrap(),
        "fun real() {}\n",
        "the link's spelling reaches the same formatted bytes"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("shared/helper.vl")).unwrap(),
        "fun helper() {}\n"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fmt_counts_a_file_named_by_two_overlapping_roots_once() {
    // B213 — G22's sibling by symptom, a different mechanism. G22 gave one
    // WALK one identity set; `fmt` then built a fresh walk per command-line
    // root, so the set did not span roots and `vilan fmt --check src src/pkg`
    // printed `src/pkg/helper.vl` twice. No symlink is involved: `src/pkg` is
    // simply named twice, once on its own and once inside `src`.
    //
    // Two files need formatting, so — like G22's pin — this is "each FILE
    // once", not "one line": a fix that collapsed the two real files into one
    // would fail it exactly as the missing guard did. Both root ORDERS are
    // asserted, because the parent-first and child-first walks reach the shared
    // subtree at different moments.
    let dir = temp_project("overlapping_roots");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "src/top.vl", "fun  top( ) { }\n");
    write(&dir, "src/pkg/helper.vl", "fun  helper( ) { }\n");
    let src = dir.join("src");
    let pkg = dir.join("src/pkg");

    for roots in [
        [src.to_str().unwrap(), pkg.to_str().unwrap()],
        [pkg.to_str().unwrap(), src.to_str().unwrap()],
    ] {
        let output = vilan(&["fmt", "--check", roots[0], roots[1]]);
        let text = combined(&output);
        assert_eq!(
            text.matches("would reformat").count(),
            2,
            "two files need formatting, and naming their directory twice on the \
             command line does not make three: {roots:?}\n{text}"
        );
        assert_eq!(
            text.matches("helper.vl").count(),
            1,
            "the file both roots reach is reported once: {roots:?}\n{text}"
        );
        assert!(
            !output.status.success(),
            "`--check` still fails when something would be reformatted:\n{text}"
        );
    }

    // And the rewrite agrees: one file, formatted once.
    let rewrite = vilan(&["fmt", src.to_str().unwrap(), pkg.to_str().unwrap()]);
    let rewritten = combined(&rewrite);
    assert!(rewrite.status.success(), "{rewritten}");
    assert_eq!(
        rewritten.matches("formatted").count(),
        2,
        "the rewrite formats each file once:\n{rewritten}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("src/pkg/helper.vl")).unwrap(),
        "fun helper() {}\n"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fmt_walks_every_disjoint_root_it_is_given() {
    // The control for the pin above: sharing one identity set across roots must
    // not make a LATER root a no-op. Two roots that overlap in nothing, each
    // holding a file that needs formatting, and both are reported.
    let dir = temp_project("disjoint_roots");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "src/alpha/one.vl", "fun  one( ) { }\n");
    write(&dir, "src/beta/two.vl", "fun  two( ) { }\n");

    let output = vilan(&[
        "fmt",
        "--check",
        dir.join("src/alpha").to_str().unwrap(),
        dir.join("src/beta").to_str().unwrap(),
    ]);
    let text = combined(&output);
    assert_eq!(
        text.matches("would reformat").count(),
        2,
        "disjoint roots are each walked whole:\n{text}"
    );
    assert!(text.contains("one.vl") && text.contains("two.vl"), "{text}");

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

// ── S8: the same layout, in Windows' spelling (audit run 7, Order 24) ──
//
// S7 above pins the symlink doctrine and says why every pin in it is
// `cfg(unix)`: creating a symlink needs a privilege Windows does not grant by
// default. That left the doctrine unmeasured on the platform whose link
// semantics differ most from the ones it was written against, which is what
// audit run 7 chartered. These are the other half. CI's Windows leg is the only
// instrument that runs them — on a unix host they are compiled away, so a green
// local suite says nothing about them at all.
//
// A JUNCTION does the work, because it needs no privilege: a directory reparse
// point that `fs::metadata` resolves through, `fs::symlink_metadata().
// is_symlink()` reports as a link, and `fs::read_link` reads — the same three
// calls the CLI makes of a unix symlink, so it reaches every branch the S7 pins
// do. Three differences are why these exist rather than being inferred from the
// unix run:
//
// * the target a junction stores is ABSOLUTE (Windows resolves it when the link
//   is made), where a unix symlink stores the bytes it was handed;
// * `fs::canonicalize` answers with a VERBATIM (`\\?\`) path, so every
//   containment and identity test here compares across a seam that does not
//   exist on unix;
// * the filesystem FOLDS CASE, so two spellings that are two paths on unix name
//   one directory here.
//
// Only the last pin needs the privilege — a RELATIVE directory symlink is the
// one shape a junction cannot stand in for — and it skips with a printed note
// rather than failing when the machine does not grant it.
//
// Every pin here tears its links down BEFORE asserting. A leaked junction is not
// the harmless litter a leaked temp directory is: the cycle fixture is a trap for
// anything that later walks `%TEMP%` naively, and cleaning up first means a
// failing assertion still leaves the runner clean.

/// Creates a directory junction at `link` pointing at `target`, and asserts it
/// exists afterwards — a fixture that silently failed to appear would make every
/// pin below vacuously green.
///
/// Spawned as `cmd /S /C` rather than through `Command::args`, for two reasons
/// that are both load-bearing. `mklink` is a `cmd` BUILTIN, so there is no
/// executable to spawn and the shell is not a convenience. And `cmd` re-parses
/// the command line with its own quoting rules: `/S` tells it to take everything
/// after `/C` verbatim instead of running the quote-stripping pass that mangles a
/// quoted path, and [`CommandExt::raw_arg`] is the matching half — `Command`'s
/// ordinary quoting would backslash-escape the inner quotes for a C runtime that
/// `cmd` is not, and `cmd` would pass the escapes through to `mklink` as part of
/// the path. Temp paths here carry a process id, and a CI runner's carry spaces,
/// so quoting them is not optional.
///
/// [`CommandExt::raw_arg`]: std::os::windows::process::CommandExt::raw_arg
#[cfg(windows)]
fn junction(link: &Path, target: &Path) {
    use std::os::windows::process::CommandExt;

    let output = Command::new("cmd")
        .arg("/S")
        .arg("/C")
        .raw_arg(format!(
            "mklink /J \"{}\" \"{}\"",
            link.display(),
            target.display()
        ))
        .output()
        .expect("run mklink");
    assert!(
        output.status.success() && link.exists(),
        "mklink /J {} -> {} did not create a junction:\n{}",
        link.display(),
        target.display(),
        combined(&output)
    );
}

/// Removes `dir`, taking the named junctions out first.
///
/// `remove_dir` on a reparse point removes the LINK and never touches what it
/// points at, which is the whole reason the order matters: the targets here are
/// inside the same fixture, and one of them is a cycle. Doing it by name rather
/// than trusting a recursive delete to recognize a reparse point keeps the
/// cleanup a statement about this tree instead of a bet on `remove_dir_all`.
#[cfg(windows)]
fn remove_tree_with_junctions(dir: &Path, junctions: &[&str]) {
    for link in junctions {
        let _ = std::fs::remove_dir(dir.join(link));
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// Whether this machine can create a directory SYMLINK: Developer Mode, or the
/// `SeCreateSymbolicLinkPrivilege` an elevated shell holds. Neither is on by
/// default, which is why every other fixture here is a junction.
///
/// Probed by trying it rather than by reading a policy, because the privilege is
/// precisely "did this call succeed" — a guess would be the wrong kind of green.
#[cfg(windows)]
fn windows_symlinks_available() -> bool {
    let probe = temp_project("symlink_probe");
    std::fs::create_dir_all(probe.join("target")).expect("probe fixture");
    let available =
        std::os::windows::fs::symlink_dir(probe.join("target"), probe.join("link")).is_ok();
    let _ = std::fs::remove_dir(probe.join("link"));
    let _ = std::fs::remove_dir_all(&probe);
    available
}

#[cfg(windows)]
#[test]
fn fmt_terminates_on_a_junction_cycle_and_reports_each_file_once() {
    // G18's cycle, and audit run 7's F6 — the SAME hazard, guarded by a
    // different mechanism, which is why the unix twin
    // (`fmt_terminates_on_a_directory_cycle_and_reports_each_file_once`) does
    // not cover this. There the guard keys on `(device, inode)`, a number the
    // kernel hands out; here `DirectoryIdentity` is a PATH, and the guard is
    // only as good as the resolution behind it. F6 found that resolution was
    // `util::canonical_path`, which never fails — where it cannot resolve, it
    // degrades to a LEXICAL normalization, so `src/l1`, `src/l1/l1`,
    // `src/l1/l1/l1` become three keys for one directory, `visited` never
    // collides, and the arm that stops the walk cannot run. Nothing else stands
    // behind it: `TreeWalk::walk` has no depth cap, and there is no ELOOP here.
    //
    // Worth being exact about what this pin does and does not discriminate. The
    // fixture below is caught by BOTH spellings, because a shallow junction
    // resolves fine and the two helpers agree while it does; the fix matters in
    // the corner where resolution FAILS, which no portable fixture can force.
    // So this is a regression pin on the guard as a whole — remove it, or let
    // junctions read as ordinary directories, and it goes red — rather than the
    // discriminating pin for F6, which is a defect of expressiveness (`Some` was
    // the only value the old arm could return) and is argued at its own site.
    //
    // The TIMEOUT is the instrument, exactly as in the unix twin. This pin has
    // to prove the hang is gone, and "the test passed" is not that proof if it
    // could pass by hanging the harness instead. Generous (60 s against a walk
    // that now visits three directories) because the suite runs it under full
    // lane load.
    let dir = temp_project("junction_cycle");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    // Deliberately unformatted, so "the walk finished" and "the walk found it"
    // are distinguishable from the output alone.
    write(&dir, "src/main.vl", "fun  main( ) { }\n");
    let src = dir.join("src");
    junction(&src.join("l1"), &src);
    junction(&src.join("l2"), &src);

    let started = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["fmt", "--check", dir.to_str().unwrap()])
        .env("NO_COLOR", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run vilan fmt");
    let output = loop {
        match child.try_wait().expect("wait on vilan fmt") {
            Some(_) => break child.wait_with_output().expect("collect vilan fmt"),
            None if started.elapsed() > Duration::from_secs(60) => {
                let _ = child.kill();
                let _ = child.wait();
                remove_tree_with_junctions(&dir, &["src/l1", "src/l2"]);
                panic!("`vilan fmt --check` did not terminate on a junction cycle");
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    };
    let text = combined(&output);
    remove_tree_with_junctions(&dir, &["src/l1", "src/l2"]);
    assert_eq!(
        text.matches("would reformat").count(),
        1,
        "one file, reported once — a cycle re-walked is the same directory \
         under another name, whatever name reached it:\n{text}"
    );
}

#[cfg(windows)]
#[test]
fn a_junction_inside_a_declared_tree_is_digested_unfollowed() {
    // The fence `collect_tree` draws: the TOP-LEVEL declared path is resolved
    // through a link, a link found INSIDE the tree is not, and it digests as its
    // own target path. Its unix twin
    // (`a_symlink_inside_a_declared_tree_is_digested_unfollowed`) pins the rule;
    // what it cannot pin is that a JUNCTION is seen at all. The branch turns on
    // `symlink_metadata().is_symlink()`, which on Windows answers for two
    // reparse tags rather than one — a junction is `IO_REPARSE_TAG_MOUNT_POINT`,
    // not `IO_REPARSE_TAG_SYMLINK` — and on `read_link`, which has to strip the
    // NT-internal `\??\` prefix off the absolute target Windows stored. Read as
    // an ordinary directory instead, a junction would be FOLLOWED here, and a
    // cycle or an escape would follow from that.
    //
    // Both halves are sharp because the fixture makes following and not
    // following disagree: `static/a` and `static/b` are byte-identical trees, so
    // re-pointing the junction is invisible to a walk that follows it and is a
    // change to one that reads the link.
    let dir = temp_project("tree_junction");
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
    write(&dir, "static/a/x.txt", "same\n");
    write(&dir, "static/b/x.txt", "same\n");
    write(&dir, "outside/note.txt", "one\n");
    junction(&dir.join("static/link"), &dir.join("static/a"));
    junction(&dir.join("static/escape"), &dir.join("outside"));

    build(&dir);
    let cold = runs(&dir, "ran.txt");
    build(&dir);
    let untouched = runs(&dir, "ran.txt");

    std::fs::remove_dir(dir.join("static/link")).unwrap();
    junction(&dir.join("static/link"), &dir.join("static/b"));
    build(&dir);
    let repointed = runs(&dir, "ran.txt");

    write(&dir, "outside/note.txt", "two\n");
    build(&dir);
    let after_escape = runs(&dir, "ran.txt");
    remove_tree_with_junctions(&dir, &["static/link", "static/escape"]);

    assert_eq!(cold, 1);
    assert_eq!(untouched, 1, "an untouched tree is fresh");
    assert_eq!(
        repointed, 2,
        "the junction's target PATH is its content: re-pointing it at a \
         byte-identical tree is still a change"
    );
    assert_eq!(
        after_escape, 2,
        "and the tree does not extend through a junction that leaves it"
    );
}

#[cfg(windows)]
#[test]
fn a_declared_directory_input_reached_through_a_junction_stays_fresh() {
    // G15's alignment, in Windows' spelling: the stamp and the watcher resolve
    // a declared path the same way, so a declared name that IS a link to a
    // directory digests as that directory's tree instead of failing to read and
    // re-running the hook on every build, silently, forever. The unix twin
    // (`a_declared_directory_input_reached_through_a_symlink_stays_fresh`) pins
    // the alignment; it cannot pin that `fs::metadata` resolves THROUGH a
    // junction while `fs::symlink_metadata` stops at it, which is the distinction
    // the whole fix rests on and is a separate implementation on this platform.
    let dir = temp_project("directory_input_junction");
    write(
        &dir,
        "vilan.toml",
        &format!(
            "[package]\nname = \"app\"\n\n[[build.hook]]\nname = \"copy\"\nrun = {}\n\
             inputs = \"linked_static\"\n",
            toml_string(&append("ran.txt"))
        ),
    );
    write(&dir, "src/main.vl", MAIN);
    write(&dir, "static/a.txt", "a\n");
    junction(&dir.join("linked_static"), &dir.join("static"));

    build(&dir);
    let first = runs(&dir, "ran.txt");
    build(&dir);
    let second = runs(&dir, "ran.txt");
    build(&dir);
    let third = runs(&dir, "ran.txt");

    // And it is fresh rather than frozen: the tree behind the junction is still
    // the content, so a change through it re-runs the hook.
    write(&dir, "static/a.txt", "changed\n");
    build(&dir);
    let after_edit = runs(&dir, "ran.txt");
    remove_tree_with_junctions(&dir, &["linked_static"]);

    assert_eq!(first, 1);
    assert_eq!(
        second, 1,
        "a declared junction to a directory digests as that directory's tree"
    );
    assert_eq!(third, 1, "and stays fresh, build after build");
    assert_eq!(
        after_edit, 2,
        "an edit behind the junction is an edit to the declared input"
    );
}

/// G17's tree in Windows' spelling: the package's declared `generated` root is a
/// JUNCTION to a tree outside it. Returns `(outer, package)` — the products live
/// at `outer/outside/icons`, reachable as `package/src/icons`.
#[cfg(windows)]
fn junctioned_generated_project(tag: &str) -> (PathBuf, PathBuf) {
    let outer = temp_project(tag);
    let package = outer.join("package");
    std::fs::create_dir_all(outer.join("outside/icons")).unwrap();
    std::fs::create_dir_all(package.join("src")).unwrap();
    write(
        &package,
        "vilan.toml",
        &format!(
            "[package]\nname = \"app\"\ngenerated = \"src/icons\"\n\n[[build.hook]]\n\
             name = \"icons\"\nrun = [{}, {}]\ninputs = \"icons.lock\"\n\
             outputs = \"src/icons/lib.vl\"\n",
            toml_string(&append("ran.txt")),
            toml_string(&generate_module("src/icons/lib.vl"))
        ),
    );
    write(
        &package,
        "src/main.vl",
        "import std::io::print;\nimport pkg::icons::generated;\n\
         fun main() { print(generated() + 1) }\nmain();\n",
    );
    write(&package, "icons.lock", "v1\n");
    junction(&package.join("src/icons"), &outer.join("outside/icons"));
    (outer, package)
}

#[cfg(windows)]
#[test]
fn fmt_leaves_a_product_under_a_junctioned_generated_root_alone() {
    // G17's fail-OPEN: the containment check missed through a link, so `vilan
    // fmt` rewrote the product and re-staled the hook that digests it — §12.1's
    // loop, live. `generated_root_covering` closes it with two ladders (the
    // SPELLED ancestry the walk reached the file through, and the RESOLVED one
    // an editor opens it by), and both are driven here: the directory walk, then
    // the explicit path.
    //
    // What the unix twin (`fmt_leaves_a_product_under_a_symlinked_generated_
    // root_alone`) cannot reach is the `\\?\` seam. Every comparison in both
    // ladders is between a path `fs::canonicalize` produced — verbatim — and one
    // built by joining, and they only meet because `util::strip_verbatim_prefix`
    // takes the prefix off first. On unix that helper is a no-op and the seam is
    // not there to get wrong; here it is the difference between the exclusion
    // holding and the loop coming back.
    let (outer, package) = junctioned_generated_project("fmt_junction");
    build(&package);
    let product = package.join("src/icons/lib.vl");
    let before = std::fs::read(&product).unwrap();

    let output = fmt(&package);
    let walked = combined(&output);
    let after_walk = std::fs::read(&product).unwrap();

    let named = vilan(&["fmt", product.to_str().unwrap()]);
    let by_name = combined(&named);
    let after_name = std::fs::read(&product).unwrap();
    remove_tree_with_junctions(&package, &["src/icons"]);
    let _ = std::fs::remove_dir_all(&outer);

    assert!(output.status.success(), "{walked}");
    assert_eq!(
        after_walk, before,
        "a product behind a junctioned root is not formatted:\n{walked}"
    );
    assert!(
        walked.contains("generated file") && walked.contains("not formatted"),
        "and the exclusion says so — silence is how the loop came back:\n{walked}"
    );
    assert!(named.status.success(), "{by_name}");
    assert_eq!(
        after_name, before,
        "however the file is reached, junctions included:\n{by_name}"
    );
}

#[cfg(windows)]
#[test]
fn a_relative_directory_symlink_inside_the_project_is_followed() {
    // The one shape a junction cannot stand in for, and so the one pin here that
    // needs the privilege: a junction always stores an ABSOLUTE target, resolved
    // when it was created, while a symlink can store `..\shared` and be resolved
    // against the link's own directory on every open. That is a different code
    // path in the OS, and it is the shape a project checked out of git on a
    // machine with Developer Mode on actually has.
    //
    // The behavior is G19's ruling — a link inside the project is ordinary
    // layout, and is walked — pinned on unix by
    // `fmt_follows_a_directory_link_that_stays_inside_the_project`. The green
    // negative matters as much as the cycle pin above: a scope that terminated by
    // refusing every link would pass that one and fail the doctrine.
    if !windows_symlinks_available() {
        eprintln!(
            "SKIPPED a_relative_directory_symlink_inside_the_project_is_followed: \
             creating a directory symlink needs Developer Mode or \
             SeCreateSymbolicLinkPrivilege, which this machine does not grant. \
             Every other Windows link pin uses an unprivileged junction and ran."
        );
        return;
    }
    let dir = temp_project("inside_symlink");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "src/main.vl", "fun main() {}\n");
    write(&dir, "shared/helper.vl", "fun  helper( ) { }\n");
    std::os::windows::fs::symlink_dir(r"..\shared", dir.join("src/shared"))
        .expect("a relative directory symlink inside the project");

    let output = vilan(&["fmt", dir.to_str().unwrap()]);
    let text = combined(&output);
    let helper = std::fs::read_to_string(dir.join("shared/helper.vl")).unwrap();
    // A symlink comes out with `remove_dir` for the same reason a junction does.
    remove_tree_with_junctions(&dir, &["src/shared"]);

    assert!(output.status.success(), "{text}");
    assert_eq!(
        helper, "fun helper() {}\n",
        "a relative link inside the project is layout, and its tree formats:\n{text}"
    );
    assert!(
        !text.contains("outside this project"),
        "and nothing is said about it:\n{text}"
    );
}

// ── The wall-clock waits' suite placement (tracker N46) ───────────────────────
//
// Every test that drives a live `--watch` session and then waits for a ROUND
// belongs to `.config/nextest.toml`'s `wall-clock-waits` group, which runs them
// one at a time. The reason is the 301 s red this file's own pins have paid
// three times: watch sessions, each spawning a watcher and a compile, all
// eligible to run at once inside an interleave already 16 wide. The language
// server's `package_recolor_tests` are in the group for the same reason and
// join it from the other side of the workspace; they are named directly in the
// filterset, and this file's scan does not reach them.
//
// The group is selected by a filterset, and part of that filterset is a NAME
// pattern — which is exactly the kind of thing that rots when somebody adds a
// pin (or renames one) without knowing the pattern exists. This is the check
// that keeps it honest, and it is not theoretical: it is what found the two
// members outside the HMR suites, `split`'s `a_watch_round_clears_the_chunks_a
// _build_left` and `serve_build`'s `run_watch_tells_its_child_it_is_watching`.

/// Binaries whose every test is a watch session, so the filterset takes them
/// whole.
const WATCH_SESSION_BINARIES: &[&str] = &[
    "hmr",
    "hmr_swap",
    "hmr_css_matrix",
    "watch_lifecycle",
    "watch_leg_reuse",
];

/// Binaries with a watch family inside a larger suite, selected by name.
const MIXED_BINARIES: &[&str] = &[
    "build_hooks",
    "assets",
    "asset_bundle",
    "split",
    "serve_build",
];

/// The name substrings the filterset's `test(/watch|round/)` matches.
const NAME_MARKERS: &[&str] = &["watch", "round"];

/// How a test says it drives a live session: the local spawner, or the flag
/// handed straight to the binary.
const SPAWNS_A_WATCHER: &[&str] = &["spawn_watch(", "\"--watch\""];

fn suite_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests")
}

/// Every `#[test]` in `source` that drives a watch session, by name.
///
/// Line oriented, and comments are dropped first: three of this tree's tests
/// only MENTION `--watch` in the prose above them (`asset_bundle`'s containment
/// refusal is one), and counting those would put the gate in the business of
/// arguing about doc comments.
fn tests_that_drive_a_watch_session(source: &str) -> Vec<String> {
    let mut driving = Vec::new();
    let mut current: Option<String> = None;
    let mut recent_attributes = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        if trimmed.starts_with('#') {
            recent_attributes.push(trimmed.to_string());
        }
        if let Some(rest) = line.strip_prefix("fn ") {
            let is_test = recent_attributes
                .iter()
                .any(|line| line.starts_with("#[test]"));
            recent_attributes.clear();
            current = is_test
                .then(|| {
                    rest.split('(')
                        .next()
                        .unwrap_or_default()
                        .trim()
                        .to_string()
                })
                .filter(|name| !name.is_empty());
            continue;
        }
        if line.starts_with('}') {
            current = None;
            continue;
        }
        if let Some(name) = &current
            && SPAWNS_A_WATCHER.iter().any(|marker| line.contains(marker))
            && !driving.contains(name)
        {
            driving.push(name.clone());
        }
    }
    driving
}

#[test]
fn every_test_that_drives_a_watch_session_is_in_the_group() {
    let mut stray = Vec::new();
    let entries = std::fs::read_dir(suite_root()).expect("the CLI's tests directory");
    for entry in entries {
        let path = entry.expect("a directory entry").path();
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let binary = path
            .file_stem()
            .expect("a file stem")
            .to_string_lossy()
            .into_owned();
        if WATCH_SESSION_BINARIES.contains(&binary.as_str()) {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("read a suite");
        for name in tests_that_drive_a_watch_session(&source) {
            let selected = MIXED_BINARIES.contains(&binary.as_str())
                && NAME_MARKERS.iter().any(|marker| name.contains(marker));
            if !selected {
                stray.push(format!("  {binary}::{name}"));
            }
        }
    }
    assert!(
        stray.is_empty(),
        "these tests drive a live `--watch` session but are not selected by \
         `.config/nextest.toml`'s `watch-rounds` filterset, so they run against the \
         full 16-wide interleave and pay N46's 301 s bound. Either name the test so \
         `test(/watch|round/)` reaches it and add its binary to MIXED_BINARIES here, \
         or add the whole binary to both this list and the filterset:\n{}",
        stray.join("\n")
    );
}

/// Members the scan above cannot reach, because they live outside this crate's
/// suite directory: the language server's package-recolor pins, which wait on a
/// debounced re-analysis instead of on a watch round, and M26's cancellation
/// pins, which wait on the same thing plus a keystroke burst of debounce
/// windows. Named here so the config check below covers them, and so dropping
/// either from the filterset is a red rather than a silence.
const MEMBERS_OUTSIDE_THIS_CRATE: &[&str] =
    &["binary(vilan-lsp) & test(/package_recolor_tests|cancellation_tests|watched_files_tests/)"];

#[test]
fn the_group_is_declared_the_way_this_file_reads_it() {
    // The other half. The check above is worth nothing if the filterset it
    // describes is not the filterset that ships — a group renamed, a binary
    // dropped from the union, `max-threads` raised back to the default — so the
    // config is read and held against the same lists.
    let config = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.config/nextest.toml"),
    )
    .expect("the committed nextest profile");
    assert!(
        config.contains("wall-clock-waits = { max-threads = 1 }"),
        "the group must exist and must be ONE thread — that is the whole fix:\n{config}"
    );
    assert!(
        config.contains("test-group = 'wall-clock-waits'"),
        "an override must actually join the group:\n{config}"
    );
    assert!(
        config.contains("test(/watch|round/)"),
        "the name pattern this file reimplements must be the one in the filterset"
    );
    for binary in WATCH_SESSION_BINARIES.iter().chain(MIXED_BINARIES) {
        assert!(
            config.contains(&format!("binary({binary})")),
            "`{binary}` holds watch sessions but the filterset does not name it"
        );
    }
    for member in MEMBERS_OUTSIDE_THIS_CRATE {
        assert!(
            config.contains(member),
            "`{member}` waits on a wall clock for another thread's work and the \
             filterset does not select it"
        );
    }
}
