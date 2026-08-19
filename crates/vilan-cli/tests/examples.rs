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

mod support;
use support::ladder::documented_legs;

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
            script: "main.mjs",
            expected_stdout: "25\n",
        },
        "rpc" => PostBuild::Run {
            script: "src/main.mjs",
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
        "todo" => PostBuild::Artifacts(&["dist/server.mjs", "dist/client.js", "dist/client.css"]),
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

/// One field of a build manifest (`dist/<leg>.chunks.json`) as the CLI writes
/// it — `"leg": "client"` → `Some("client")`, `"styles": null` → `None`. The
/// shape is the toolchain's own, pinned byte-for-byte by
/// `tests/split/golden/app.chunks.json`, so a one-line reader is enough here.
fn manifest_string_field<'manifest>(
    manifest: &'manifest str,
    field: &str,
) -> Option<&'manifest str> {
    let needle = format!("\"{field}\": \"");
    let start = manifest.find(&needle)? + needle.len();
    manifest[start..]
        .split_once('"')
        .map(|(value, _rest)| value)
}

/// The stylesheets a server in the staged tree links BY WRITING THE PAGE
/// (fullstack-dx.md §5.5, rung 2): for every leg whose document a `.vl`
/// source writes with `Document::of(build)`, the `styles` its build manifest
/// names. That document carries the `<link>` if and only if the build emitted
/// styles — it is derived from the very manifest read here — and it is written
/// at boot, never to disk, so no `.html` can vouch for it (§16.2, E65).
///
/// Read from every `.vl` in the tree rather than from the entries alone:
/// `std::document` is process-coloured, so a browser leg cannot carry the
/// call, and a server may build its page in a module the entry imports. A call
/// in a module no entry reaches is the one blind spot, accepted over restating
/// the module loader in a gate.
fn documented_stylesheets(staged: &std::path::Path) -> Vec<String> {
    let legs: Vec<String> = files_with_extension(staged, "vl")
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .flat_map(|source| documented_legs(&source))
        .collect();
    files_with_extension(staged, "json")
        .iter()
        .filter(|path| path.to_string_lossy().ends_with(".chunks.json"))
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .filter(|manifest| {
            manifest_string_field(manifest, "leg")
                .is_some_and(|leg| legs.iter().any(|documented| documented == leg))
        })
        .filter_map(|manifest| manifest_string_field(&manifest, "styles").map(str::to_string))
        .collect()
}

/// Every stylesheet a build EMITS must be loaded by a page the example serves.
/// A `const style()` chain compiled into a sidecar no page links is work
/// redone on every build and thrown away — silent, because the app still
/// runs, just unstyled. `reactive-ui` was in exactly that state: `app.css`
/// emitted (and asserted present, above) and `index.html` never linking it.
///
/// Two sources of truth for "linked", because the ladder has two ways to have
/// a page: an `.html` in the tree that `<link>`s the sheet (rungs 0–1, the
/// shell on disk), or a server that writes the page itself from the build
/// (rung 2, `documented_stylesheets`). Stated over what the build produced
/// rather than over a per-example list, so a new example is covered the day
/// it lands. No example TRACKS a `.css` file, so every one found here is
/// emitted output.
fn unlinked_stylesheets(directory: &str, staged: &std::path::Path) -> Option<String> {
    let stylesheets = files_with_extension(staged, "css");
    if stylesheets.is_empty() {
        return None;
    }
    let pages: Vec<String> = files_with_extension(staged, "html")
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .collect();
    let documented = documented_stylesheets(staged);
    let unlinked: Vec<String> = stylesheets
        .iter()
        .filter_map(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .filter(|name| !pages.iter().any(|page| links_stylesheet(page, name)))
        .filter(|name| !documented.contains(name))
        .collect();
    (!unlinked.is_empty()).then(|| {
        format!(
            "{directory}: emitted stylesheets that no page links: {unlinked:?} — \
             no `.html` in the tree has a `<link rel=\"stylesheet\">` to it and no \
             server writes its leg's document with `Document::of(build)`, so the \
             const styles compile on every build and are thrown away"
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

// --- the second source of truth for "linked", pinned both ways (E65) --------

/// A fresh scratch tree shaped like a staged example, with the given
/// `(relative path, contents)` files written into it.
fn scratch_tree(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("vilan_example_pin_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (relative, contents) in files {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    root
}

/// A build manifest as `write_chunks` writes one for a leg that did not split.
fn manifest(leg: &str, styles: Option<&str>) -> String {
    let styles = styles.map_or_else(|| "null".to_string(), |name| format!("\"{name}\""));
    format!(
        "{{\n\t\"leg\": \"{leg}\",\n\t\"entry\": \"{leg}.js\",\n\t\"styles\": {styles},\n\t\"classic_script\": false,\n\t\"chunks\": []\n}}\n"
    )
}

const RUNG_2_SERVER: &str = "import std::build::require_build;\n\
     import std::document::Document;\n\
     import std::http::{ Response, Server };\n\n\
     async fun main() {\n\
     \tlet build = require_build(\"client\");\n\
     \tlet page = Document::of(build).title(\"Todo\").html();\n\
     \tServer::builder()\n\
     \t\t.port(8080)\n\
     \t\t.serve_build(build)\n\
     \t\t.on_request(|request| Response::builder().body(page).build())\n\
     \t\t.build()\n\
     \t\t.start();\n\
     }\n";

#[test]
fn a_document_of_call_resolves_to_the_leg_its_build_names() {
    // The ladder's idiom: the build bound once in `main`, the document built
    // from the binding.
    assert_eq!(documented_legs(RUNG_2_SERVER), vec!["client"]);
    // Inline, and through `build_of` with the `!` a `Result` wants.
    assert_eq!(
        documented_legs("fun main() { let page = Document::of(require_build(\"admin\")).html(); }"),
        vec!["admin"]
    );
    assert_eq!(
        documented_legs(
            "fun main() { let build = build_of(\"client\")!; let page = Document::of(build).html(); }"
        ),
        vec!["client"]
    );
    // A type annotation and a `mut` binding are still the binding.
    assert_eq!(
        documented_legs(
            "fun main() { let build: LegBuild = require_build(\"client\"); Document::of(build); }"
        ),
        vec!["client"]
    );
    assert_eq!(
        documented_legs(
            "fun main() { mut build = require_build(\"client\"); Document::of(build); }"
        ),
        vec!["client"]
    );
    // Two legs, two documents — each credited to its own build, once.
    assert_eq!(
        documented_legs(
            "fun main() {\n\
             \tlet client = require_build(\"client\");\n\
             \tlet admin = require_build(\"admin\");\n\
             \tlet home = Document::of(client).html();\n\
             \tlet panel = Document::of(admin).html();\n\
             \tlet again = Document::of(client).title(\"x\").html();\n\
             }"
        ),
        vec!["admin", "client"]
    );
}

#[test]
fn a_document_of_in_a_comment_or_a_string_is_not_a_call() {
    // The reason this reads tokens and not text: mention the call without
    // making it, and nothing is documented.
    let commented = "fun main() {\n\
         \tlet build = require_build(\"client\");\n\
         \t// Document::of(build) would write the page; this server reads one.\n\
         \tlet page = require_shell(\"src/app.html\", build).html();\n\
         }";
    assert!(
        documented_legs(commented).is_empty(),
        "a comment is not a call"
    );
    let quoted = "fun main() {\n\
         \tlet build = require_build(\"client\");\n\
         \tlet note = \"Document::of(build)\";\n\
         }";
    assert!(documented_legs(quoted).is_empty(), "a string is not a call");
    // And a call whose build this cannot trace to a leg is not guessed at.
    let untraceable = "fun page(build: LegBuild): str { Document::of(build).html() }";
    assert!(
        documented_legs(untraceable).is_empty(),
        "a build that arrives through a parameter names no leg here"
    );
}

#[test]
fn a_server_that_writes_its_legs_document_links_that_builds_stylesheet() {
    // Rung 2 over a staged tree: no `.html` anywhere, `dist/client.css`
    // emitted, and the server writing the client leg's document — the exact
    // state `every_example_builds` refused by construction before E65.
    let linked = scratch_tree(
        "linked",
        &[
            ("src/server.vl", RUNG_2_SERVER),
            ("dist/client.css", ".a{color:red}"),
            (
                "dist/client.chunks.json",
                &manifest("client", Some("client.css")),
            ),
        ],
    );
    assert_eq!(documented_stylesheets(&linked), vec!["client.css"]);
    assert_eq!(unlinked_stylesheets("linked", &linked), None);
    let _ = std::fs::remove_dir_all(&linked);

    // The same tree with the call commented out is the failure it always was:
    // the second truth adds a way to be linked, never a way to be excused.
    let unlinked = scratch_tree(
        "unlinked",
        &[
            (
                "src/server.vl",
                &RUNG_2_SERVER.replace("\tlet page = Document::of", "\t// let page = Document::of"),
            ),
            ("dist/client.css", ".a{color:red}"),
            (
                "dist/client.chunks.json",
                &manifest("client", Some("client.css")),
            ),
        ],
    );
    assert!(documented_stylesheets(&unlinked).is_empty());
    let failure = unlinked_stylesheets("unlinked", &unlinked).expect("the sheet is unlinked");
    assert!(failure.contains("client.css"), "{failure}");
    let _ = std::fs::remove_dir_all(&unlinked);
}

#[test]
fn a_document_links_only_the_stylesheet_of_the_build_it_was_given() {
    // Two browser legs, one document: the `admin` leg's sheet is still
    // unlinked, and is named as such — writing the `client` page vouches for
    // `client.css` and nothing else.
    let tree = scratch_tree(
        "two_legs",
        &[
            ("src/server.vl", RUNG_2_SERVER),
            ("dist/client.css", ".a{color:red}"),
            (
                "dist/client.chunks.json",
                &manifest("client", Some("client.css")),
            ),
            ("dist/admin.css", ".b{color:blue}"),
            (
                "dist/admin.chunks.json",
                &manifest("admin", Some("admin.css")),
            ),
        ],
    );
    assert_eq!(documented_stylesheets(&tree), vec!["client.css"]);
    let failure = unlinked_stylesheets("two_legs", &tree).expect("admin.css is unlinked");
    assert!(
        failure.contains("admin.css") && !failure.contains("client.css"),
        "{failure}"
    );
    let _ = std::fs::remove_dir_all(&tree);
}
