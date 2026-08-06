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

/// What a built example must additionally prove — E22's decide-at-take-up
/// answers, recorded here. An example not listed is BUILD-ONLY: a new one is
/// gated the day it lands and earns richer checks when someone writes them.
/// `ssr` and `walkthrough` keep their own dedicated suites; `fullstack` stays
/// build-only deliberately — the fullstack TEMPLATE's spawn-and-fetch e2e
/// already exercises the served shape, and a second copy here would buy
/// repetition, not coverage.
enum PostBuild {
    /// A terminating node program: run the emitted script, pin exit 0 and the
    /// exact stdout — the same byte-identical bar the corpus holds.
    Run {
        script: &'static str,
        expected_stdout: &'static str,
    },
    /// A browser-family build: the emitted bundle files exist and are
    /// non-empty at their documented paths.
    Artifacts(&'static [&'static str]),
    BuildOnly,
}

fn post_build(directory: &str) -> PostBuild {
    match directory.rsplit('/').next().unwrap_or(directory) {
        "math" => PostBuild::Run {
            script: "main.js",
            expected_stdout: "25\n",
        },
        "rpc" => PostBuild::Run {
            script: "src/main.js",
            expected_stdout: concat!(
                "ok: found ada (@ada)\n",
                "ok: no such user\n",
                "raw error: Remote(\"unknown method: delete_everything\")\n",
                "--- reactive: a remote Source<i32> ---\n",
                "count = 0\n",
                "count = 1\n",
                "count = 2\n",
                "count = 10\n",
                "count = 13\n",
                "count = 16\n",
                "rpc add -> 16\n",
                "--- session: the [service(Client)] paradigm, generated ---\n",
                "status = offline\n",
                "whoami -> not logged in\n",
                "login -> false\n",
                "status = online\n",
                "login -> true\n",
                "whoami -> ada (@ada)\n",
            ),
        },
        "browser" => PostBuild::Artifacts(&["client.js"]),
        "canvas" => PostBuild::Artifacts(&["board.js"]),
        "reactive-ui" => PostBuild::Artifacts(&["app.js", "app.css"]),
        "router" => PostBuild::Artifacts(&["app.js"]),
        "todo" => PostBuild::Artifacts(&["dist/server.js", "dist/client.js", "dist/client.css"]),
        _ => PostBuild::BuildOnly,
    }
}

/// Runs the post-build check; a failure message, or `None` when it holds.
fn check_post_build(directory: &str, staged: &std::path::Path) -> Option<String> {
    match post_build(directory) {
        PostBuild::BuildOnly => None,
        PostBuild::Artifacts(artifacts) => {
            let missing: Vec<&str> = artifacts
                .iter()
                .copied()
                .filter(|artifact| {
                    std::fs::metadata(staged.join(artifact))
                        .map(|meta| meta.len() == 0)
                        .unwrap_or(true)
                })
                .collect();
            (!missing.is_empty())
                .then(|| format!("{directory}: emitted bundle files missing or empty: {missing:?}"))
        }
        PostBuild::Run {
            script,
            expected_stdout,
        } => {
            let output = Command::new("node")
                .arg(script)
                .current_dir(staged)
                .output()
                .expect("run node");
            if !output.status.success() {
                return Some(format!(
                    "{directory}: `node {script}` failed:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            (stdout != expected_stdout).then(|| {
                format!(
                    "{directory}: output drifted\n--- expected\n{expected_stdout}--- got\n{stdout}"
                )
            })
        }
    }
}

/// Every path under `root` with the given extension, recursively.
fn files_with_extension(root: &std::path::Path, extension: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some(extension) {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Whether a page loads `name` through a `<link rel="stylesheet">` — both
/// words inside ONE tag, so a filename mentioned in a comment doesn't count.
fn links_stylesheet(page: &str, name: &str) -> bool {
    page.split("<link")
        .skip(1)
        .filter_map(|rest| rest.split_once('>'))
        .any(|(tag, _)| tag.contains("stylesheet") && tag.contains(name))
}

/// Every stylesheet a build EMITS must be loaded by one of the example's own
/// pages. A `const style()` chain compiled into a sidecar no page links is
/// work redone on every build and thrown away — silent, because the app still
/// runs, just unstyled. `reactive-ui` was in exactly that state: `app.css`
/// emitted (and asserted present, above) and `index.html` never linking it.
///
/// Stated over what the build produced rather than over a per-example list, so
/// a new example is covered the day it lands. No example TRACKS a `.css` file,
/// so every one found here is emitted output.
fn unlinked_stylesheets(directory: &str, staged: &std::path::Path) -> Option<String> {
    let stylesheets = files_with_extension(staged, "css");
    if stylesheets.is_empty() {
        return None;
    }
    let pages: Vec<String> = files_with_extension(staged, "html")
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .collect();
    let unlinked: Vec<String> = stylesheets
        .iter()
        .filter_map(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .filter(|name| !pages.iter().any(|page| links_stylesheet(page, name)))
        .collect();
    (!unlinked.is_empty()).then(|| {
        format!(
            "{directory}: emitted stylesheets that no page links: {unlinked:?} — \
             the const styles compile on every build and are thrown away"
        )
    })
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
            if let Some(failure) = check_post_build(&directory, &staged) {
                failures.push(failure);
                continue;
            }
            if let Some(failure) = unlinked_stylesheets(&directory, &staged) {
                failures.push(failure);
                continue;
            }
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
