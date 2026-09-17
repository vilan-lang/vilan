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
//! The sweep is not finished, and the tail is NAMED rather than left to be
//! rediscovered — [`BINARIES_STILL_ON_THE_SHARED_TMPFS`] is what is left.

use std::path::{Path, PathBuf};

/// The `vilan-cli` suites still writing to the shared tmpfs, by file (N86's
/// tail).
///
/// Every one of them builds its own scratch path inline rather than through
/// `support`, and most do not declare `mod support;` at all — so porting them
/// is a structural edit to forty files that five lanes are adding tests to
/// this order, which is a merge cost out of all proportion to the fix. They
/// are listed instead, so the sweep's remainder is a fact in the tree rather
/// than a memory.
///
/// Listed by FILE and not by COUNT, deliberately: a suite here may grow
/// another scratch path without reding this gate, because reding it would
/// punish the lane that added a test rather than the one that owns the sweep.
/// What the gate does catch is a NEW file joining the list — a suite written
/// after this rule exists has no excuse — and a listed file that stopped
/// naming it, which must be delisted so the list cannot outlive the work
/// (N42's rule for an exemption that only ever subtracts).
const BINARIES_STILL_ON_THE_SHARED_TMPFS: &[&str] = &[
    "vilan-cli/tests/brew_formula.rs",
    "vilan-cli/tests/build_explain.rs",
    "vilan-cli/tests/build_manifest.rs",
    "vilan-cli/tests/conditional_get.rs",
    "vilan-cli/tests/database.rs",
    "vilan-cli/tests/debug_dumps.rs",
    "vilan-cli/tests/diagnostics.rs",
    "vilan-cli/tests/dom_events.rs",
    "vilan-cli/tests/element_head_order.rs",
    "vilan-cli/tests/fs.rs",
    "vilan-cli/tests/git_deps.rs",
    "vilan-cli/tests/hmr_overlay.rs",
    "vilan-cli/tests/http_port.rs",
    "vilan-cli/tests/infer_preset.rs",
    "vilan-cli/tests/install.rs",
    "vilan-cli/tests/macro_expansion_cache.rs",
    "vilan-cli/tests/macro_std.rs",
    "vilan-cli/tests/macro_world_phase.rs",
    "vilan-cli/tests/module_paths.rs",
    "vilan-cli/tests/mount_missing_id.rs",
    "vilan-cli/tests/npm_stub.rs",
    "vilan-cli/tests/perf_baseline.rs",
    "vilan-cli/tests/print_chunks.rs",
    "vilan-cli/tests/process.rs",
    "vilan-cli/tests/reactive_lifetimes.rs",
    "vilan-cli/tests/reactive_selection.rs",
    "vilan-cli/tests/release_scripts.rs",
    "vilan-cli/tests/request_header.rs",
    "vilan-cli/tests/router.rs",
    "vilan-cli/tests/source_bindings.rs",
    "vilan-cli/tests/source_encoding.rs",
    "vilan-cli/tests/ssr_differential.rs",
    "vilan-cli/tests/storage_handle.rs",
    "vilan-cli/tests/style_chain_order.rs",
    "vilan-cli/tests/style_when.rs",
    "vilan-cli/tests/ui_rows.rs",
    "vilan-cli/tests/upgrade.rs",
    "vilan-cli/tests/workspace.rs",
];

/// Files that name the call because the path is the BINARY's, not the
/// harness's — with what writes it.
///
/// `vilan run --watch` writes its script to `env::temp_dir()`
/// (`main.rs`'s `watch_script_path`), and the two harnesses that assert the
/// script is written and then removed have to look where it actually lands. A
/// scratch root would be a test waiting five minutes for a file nothing will
/// write — which is exactly what N86's first sweep produced here, and what
/// this entry exists so nobody repeats.
///
/// This is a different exemption from the one above: those files are waiting
/// to be ported, these two must never be.
const PATHS_THE_BINARY_OWNS: &[(&str, &str)] = &[
    (
        "vilan-cli/tests/watch_lifecycle.rs",
        "`vilan run --watch`'s own `vilan-watch-<pid>.mjs`",
    ),
    (
        "vilan-cli/tests/support/mod.rs",
        "the same script, asserted by the shared watch harness",
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
