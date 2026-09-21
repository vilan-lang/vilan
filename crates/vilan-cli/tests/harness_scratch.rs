//! The test harness writes its scratch under `target/`, not under `/tmp`
//! (tracker N82, swept by N86).
//!
//! `/tmp` on the owner's machine is a 12 GiB tmpfs shared by every worktree.
//! A full suite under nine lanes filled it — three `No space left on device`
//! failures in one run, green on the re-run, which is a red indistinguishable
//! from a real one and cost two triages before it was filed. Every test binary
//! that stages a package, writes an `.mjs` and runs `node`, or builds a project
//! is a writer, and there are dozens of them.
//!
//! `CARGO_TARGET_TMPDIR` is cargo's own scratch for integration tests, inside
//! the target directory: the files land on the filesystem the build artifacts
//! already use, and each worktree writes into its own tree rather than into one
//! shared pool. The helpers that wrap it are `vilan-core/tests/scratch/mod.rs`
//! (declared `mod scratch;` by the binary that needs it),
//! `vilan-cli/tests/support/mod.rs`'s `scratch_root`, and the `inference`
//! binary's own copy in `tests/inference/support.rs`.
//!
//! This gate is the rule: a test file does not name `std::env::temp_dir()`.
//! The sweep is finished but for three files, and the tail is NAMED rather
//! than left to be rediscovered — [`BINARIES_STILL_ON_THE_SHARED_TMPFS`] is
//! what is left, and [`PATHS_THE_BINARY_OWNS`] is what cannot move at all.

use std::path::{Path, PathBuf};

/// The `vilan-cli` suites still writing to the shared tmpfs, by file (N86's
/// tail, swept by N102).
///
/// **Thirty-eight at Order 37, NONE now** (the last three — `reactive_lifetimes`,
/// `ssr_differential`, `ui_rows` — joined `support` at Order 38's integration,
/// once reactive-38's pins had landed). The list stays as the gate's shape: a
/// suite that cannot move yet is named here, never silently exempt.
///
/// How it got here: The tail was listed rather than
/// ported because porting looked like a structural edit to forty files that
/// five lanes were adding tests to. It was not: each one builds its scratch
/// path inline from `std::env::temp_dir()` in ONE helper, so a suite costs one
/// call site and a `mod support;` line — thirty-eight call sites across
/// thirty-five files, and no test logic touched. `CARGO_TARGET_TMPDIR` is
/// expanded per including binary, so a suite that joins `support` gets a root
/// nothing else writes into, which is what the old inline paths were faking
/// with a process id.
///
/// Listed by FILE and not by COUNT, deliberately: a suite here may grow
/// another scratch path without reding this gate, because reding it would
/// punish the lane that added a test rather than the one that owns the sweep.
/// What the gate does catch is a NEW file joining the list — a suite written
/// after this rule exists has no excuse — and a listed file that stopped
/// naming it, which must be delisted so the list cannot outlive the work
/// (N42's rule for an exemption that only ever subtracts).
const BINARIES_STILL_ON_THE_SHARED_TMPFS: &[&str] = &[];

/// Files that name the call because the scratch root CANNOT serve them — with
/// the reason.
///
/// Three reasons, all of them the environment's rather than the harness's, and
/// each found by porting the file and watching it fail (N102):
///
/// - **The path is the BINARY's.** `vilan run --watch` writes its script to
///   `env::temp_dir()` (`main.rs`'s `watch_script_path`), and the harnesses
///   that assert the script is written and then removed have to look where it
///   actually lands. A scratch root would be a test waiting five minutes for a
///   file nothing will write — which is exactly what N86's first sweep
///   produced here.
/// - **The scratch root is inside a checkout.** `CARGO_TARGET_TMPDIR` is
///   `<worktree>/target/tmp`, so a test whose premise is "no vilan checkout at
///   or above this directory" cannot use it: the ancestor walk for `vilan/std`
///   finds the worktree's own, the embedded toolchain is never materialized,
///   and the stand-in checkout the test stages decides nothing.
/// - **The path is too LONG.** A `sockaddr_un` must fit in about 108 bytes,
///   and a lane's worktree spends sixty of them before the project name.
///
/// This is a different exemption from the one above: those files are waiting
/// to be ported, these cannot be.
const PATHS_THE_BINARY_OWNS: &[(&str, &str)] = &[
    (
        "vilan-cli/tests/watch_lifecycle.rs",
        "`vilan run --watch`'s own `vilan-watch-<pid>.mjs`",
    ),
    (
        "vilan-cli/tests/support/mod.rs",
        "the same script, asserted by the shared watch harness",
    ),
    (
        "vilan-cli/tests/install.rs",
        "a machine with NO checkout — under `<worktree>/target/tmp` the \
         ancestor walk finds the worktree's own `vilan/std`, the embedded \
         toolchain is never materialized, and the content-keyed cache this \
         suite asserts about is never written (N102)",
    ),
    (
        "vilan-cli/tests/module_paths.rs",
        "the same reason, from the other side: these pins turn on WHICH \
         checkout the ancestor walk reaches, and a scratch root inside one \
         makes the stand-in checkout they stage decide nothing (N102)",
    ),
    (
        "vilan-cli/tests/fs.rs",
        "`SUN_LEN` — `scan_dir_does_not_follow_symlinks_…` binds a unix socket \
         INSIDE its project tree (the program under test scans the directory \
         the socket is in), and a `sockaddr_un` path must fit in about 108 \
         bytes. `CARGO_TARGET_TMPDIR` is `<worktree>/target/tmp`, sixty \
         characters into the budget before the project name in a lane's \
         worktree, and the bind fails outright (N102)",
    ),
];

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root resolves")
}

/// Every `.rs` file under any crate's `tests/` directory, as a repository
/// -relative path with `/` separators.
fn test_files() -> Vec<(String, String)> {
    fn walk(directory: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        let mut sorted: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        sorted.sort();
        for path in sorted {
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                out.push(path);
            }
        }
    }
    let crates = repository_root().join("crates");
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(&crates) else {
        panic!("no crates directory at {}", crates.display());
    };
    let mut roots: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    roots.sort();
    for root in roots {
        walk(&root.join("tests"), &mut files);
    }
    files
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(&crates)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            (relative, text)
        })
        .collect()
}

/// Whether `text` names `temp_dir()` in CODE — a line's `//` comment is cut
/// off first, so the helpers' own prose explaining why not to call it does not
/// count as calling it.
fn names_temp_dir(text: &str) -> bool {
    text.lines().any(|line| {
        let code = match line.find("//") {
            Some(offset) => &line[..offset],
            None => line,
        };
        code.contains("temp_dir()")
    })
}

#[test]
fn no_test_file_writes_to_the_shared_tmpfs() {
    let files = test_files();
    assert!(
        files.len() > 80,
        "the walk found {} test files",
        files.len()
    );

    let mut unlisted = Vec::new();
    let mut listed_clean = Vec::new();
    for (relative, text) in &files {
        // This file names the call in order to forbid it — in the predicate,
        // and in the sentence a failure prints. Skipping itself is cheaper and
        // clearer than spelling the needle in pieces so the scan cannot see it.
        if relative == "vilan-cli/tests/harness_scratch.rs" {
            continue;
        }
        let listed = BINARIES_STILL_ON_THE_SHARED_TMPFS.contains(&relative.as_str())
            || PATHS_THE_BINARY_OWNS
                .iter()
                .any(|(file, _)| *file == relative);
        match (names_temp_dir(text), listed) {
            (true, false) => unlisted.push(format!("  {relative}")),
            (false, true) => listed_clean.push(format!("  {relative}")),
            _ => {}
        }
    }
    assert!(
        unlisted.is_empty(),
        "{} test file(s) write their scratch to `std::env::temp_dir()`. That is \
         a tmpfs every worktree shares, and filling it produces reds that are \
         green on a re-run (N82). Use `scratch::root()` — `mod scratch;` in a \
         `vilan-core` or `vilan-embedded-std` suite, `support::scratch_root()` \
         in a `vilan-cli` one:\n{}",
        unlisted.len(),
        unlisted.join("\n")
    );
    assert!(
        listed_clean.is_empty(),
        "{} file(s) recorded as still on the shared tmpfs no longer name it. \
         Delete the entry — a list that only ever subtracts work cannot red by \
         being wrong, which is how it would outlive the sweep:\n{}",
        listed_clean.len(),
        listed_clean.join("\n")
    );
}
