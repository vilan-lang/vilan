//! Every example under `vilan/examples/` builds, from exactly what a fresh
//! clone carries (E22).
//!
//! Two of the nine were gated before this: `ssr` by `ssr_fullstack.rs`, which
//! builds and runs it end to end, and `walkthrough` by
//! `workspace.rs::the_walkthrough_example_builds`, which pins the three files
//! the book tells readers to expect. The other seven — `browser`, `fullstack`,
//! `math`, `reactive-ui`, `router`, `rpc`, `todo` — were reachable only by the
//! parse gate, so a change that broke one of them compiled, shipped, and waited
//! for a reader to find it. Those two suites keep their specific claims; this
//! one makes the weaker claim about all nine.
//!
//! Two properties, and the second is what keeps the first honest:
//!
//! 1. **The list is discovered, not written down.** Examples come from reading
//!    the directory, so a new one is gated the day it lands rather than the day
//!    someone remembers to add it here. `every_example_directory_is_a_vilan_project`
//!    is the other half of that: a subdirectory without a manifest fails the
//!    suite instead of being quietly skipped, which is the one way enumeration
//!    could go vacuous.
//!
//! 2. **Only TRACKED files are staged.** Each example is copied to a temp
//!    directory through `git ls-files`, so the build starts from what a clone
//!    has — never from emitted output sitting in the working tree. Building in
//!    place would let a stale `dist/` or a leftover bundle answer for a build
//!    that no longer works, which is precisely the failure a gate exists to
//!    catch. It also keeps this suite hermetic and parallel-safe against the
//!    in-tree walkthrough build, and leaves the working tree untouched.

use std::path::PathBuf;
use std::process::Command;

/// The repository root (this crate is `crates/vilan-cli`).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every direct subdirectory of `vilan/examples`, sorted, as repo-relative
/// paths.
///
/// Deliberately unfiltered — a subdirectory is an example whether or not it
/// carries a manifest, so one that lacks it is a failure rather than a silent
/// omission from the build set.
fn example_directories() -> Vec<String> {
    let root = repo_root().join("vilan/examples");
    let mut names: Vec<String> = std::fs::read_dir(&root)
        .expect("read vilan/examples")
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| format!("vilan/examples/{}", entry.file_name().to_string_lossy()))
        .collect();
    names.sort();
    assert!(
        !names.is_empty(),
        "no examples found under {} — this gate would pass vacuously",
        root.display()
    );
    names
}

/// The tracked files under `directory` (repo-relative), via `git ls-files`.
fn tracked_files_under(directory: &str) -> Vec<String> {
    let listing = Command::new("git")
        .args(["ls-files", "-z", "--", directory])
        .current_dir(repo_root())
        .output()
        .expect("git ls-files");
    assert!(listing.status.success(), "git ls-files failed");
    String::from_utf8_lossy(&listing.stdout)
        .split('\0')
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect()
}

/// Copies `directory`'s tracked files into a fresh temp tree and returns its
/// root, preserving the layout below `directory` so relative paths inside the
/// example (a workspace member, a sibling module) still resolve.
fn stage(directory: &str) -> PathBuf {
    let tag = directory.rsplit('/').next().expect("example name");
    let staged = std::env::temp_dir().join(format!("vilan_example_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staged);

    let root = repo_root();
    let files = tracked_files_under(directory);
    assert!(
        !files.is_empty(),
        "{directory} has no tracked files — an example that exists only in the \
         working tree is not one a clone can build"
    );
    for file in &files {
        let relative = file
            .strip_prefix(directory)
            .and_then(|rest| rest.strip_prefix('/'))
            .expect("tracked path sits under the example");
        let destination = staged.join(relative);
        std::fs::create_dir_all(destination.parent().expect("a parent")).unwrap();
        std::fs::copy(root.join(file), &destination).unwrap();
    }
    staged
}

#[test]
fn every_example_builds() {
    let mut failures = Vec::new();
    for directory in example_directories() {
        let staged = stage(&directory);
        let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
            .args(["build", staged.to_str().expect("utf-8 temp path")])
            .output()
            .expect("run vilan");

        if output.status.success() {
            let _ = std::fs::remove_dir_all(&staged);
        } else {
            // The staged tree is deliberately left behind for a failure: it is
            // the exact input that broke, and reproducing by hand means getting
            // the tracked-files-only staging right.
            failures.push(format!(
                "--- {directory} (staged at {})\n{}{}",
                staged.display(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "examples that no longer build:\n\n{}",
        failures.join("\n")
    );
}

#[test]
fn every_example_directory_is_a_vilan_project() {
    let root = repo_root();
    let missing: Vec<String> = example_directories()
        .into_iter()
        .filter(|directory| !root.join(directory).join("vilan.toml").is_file())
        .collect();
    assert!(
        missing.is_empty(),
        "these live under vilan/examples but carry no vilan.toml, so \
         `every_example_builds` would skip them:\n{}",
        missing.join("\n")
    );
}
