//! The macro expansion table survives the PROCESS (tracker M33, macro-engine.md
//! §6.5).
//!
//! §6 settled the unit of caching — the expansion's source text, keyed by the
//! macro's reachable definition set × the invocation's input source — and then
//! made the store in-memory per process, with an on-disk layer noted as "later,
//! optional, safe because the key already covers everything". Optional it was
//! not: the LSP and an HMR loop pay for a macro world once, while every `vilan
//! check`, every `vilan build` and every cold benchmark row is a fresh process
//! and pays for all four of kolt's again — for std files and a toolchain that
//! did not change between runs. So the table is written under the package's
//! build directory and read back on the next process.
//!
//! **What the pins here assert is the COUNT of compiled worlds**, read off the
//! `macro-worlds` phase row rather than off a clock: a cache is either
//! answering or it is not, and "the second check was faster" is a claim about
//! the machine. The first `check` of these fixtures compiles a world; the
//! second compiles none.
//!
//! **And they assert the four ways a hit must NOT happen.** A stale expansion
//! is a miscompile, which §6 names as the worst outcome available, so each
//! invalidator gets its own test: the macro's own source changing (the key's
//! own business — `world_key` is the blanked file's content hash), the
//! toolchain version changing, `macro_std` changing, and a corrupt file, which
//! must be ignored rather than fatal. The last is not a performance property:
//! a build that fails because a cache was truncated by a full disk is worse
//! than one that recompiles.
//!
//! Each test gets its own package directory, because the table is per package
//! and these run concurrently.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

/// A package that defines its own macro and uses it. Package-local rather than
/// leaning on a std derive, so a test can EDIT the macro's source and watch the
/// key move — std belongs to the toolchain and is not a test's to change.
fn entry(tag: i32) -> String {
    format!(
        "macro fun tagged(item: Item, arguments: Arguments): Source {{\n\
         \tsource(\"fun tag_of(): i32 {{\\n\\t{tag}\\n}}\\n\")\n\
         }}\n\n\
         [tagged]\n\
         struct Point {{ x: i32, y: i32 }}\n\n\
         fun main() {{\n\
         \tlet n = tag_of();\n\
         }}\n"
    )
}

fn temp_package(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "vilan-m33-cache-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id(),
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).expect("a scratch package");
    std::fs::write(
        dir.join("vilan.toml"),
        format!("[package]\nname = \"{name}\"\ndefault-entry = \"main\"\n\n[entry.main]\n"),
    )
    .expect("the manifest");
    std::fs::write(dir.join("src/main.vl"), entry(7)).expect("the entry");
    dir
}

fn cache_file(dir: &Path) -> PathBuf {
    cache_dir(dir).join("macro-expansions")
}

/// Where a CHECK's table lives (tracker N92): under the user cache root, keyed
/// by the package's canonical path, because `vilan check` emits no artifacts
/// and so has no build directory of its own to keep memory in. Found by
/// searching rather than by recomputing the hash, so this helper cannot drift
/// from the CLI's key and quietly assert about a file nobody writes — a root
/// holding exactly one entry after a fresh package is checked is the premise,
/// and it is asserted.
fn cache_dir(dir: &Path) -> PathBuf {
    // The CLI's own canonical form and the CLI's own hasher (`expansion_cache_root`),
    // so the two cannot disagree: `std::fs::canonicalize` answers the extended-length
    // `\\?\C:\…` form on Windows where `canonical_path` does not, and Order 37's seal
    // found the four pins of this file looking under a hash nobody wrote to.
    let root = check_cache_root();
    let canonical = vilan_core::util::canonical_path(dir);
    let mut hasher = DefaultHasher::new();
    canonical.hash(&mut hasher);
    root.join(format!("{:016x}", hasher.finish()))
        .join(".cache")
}

/// `~/.vilan/check-cache` — the toolchain's own answer, under its home rules
/// (`USERPROFILE` first on Windows), so the test looks where the CLI writes.
fn check_cache_root() -> PathBuf {
    vilan_embedded_std::default_check_cache_root()
}

/// The BUILD's table, which stays where N63 put it: `dist/.cache`, inside the
/// package, where `rm -rf dist` reaches it.
fn build_cache_file(dir: &Path) -> PathBuf {
    dir.join("dist").join(".cache").join("macro-expansions")
}

/// How many macro worlds one `vilan build` of `dir` compiled — the same probe
/// as [`worlds_compiled`], for the goal that DOES own a `dist/`.
fn worlds_compiled_by_build(dir: &Path) -> usize {
    worlds_from(dir, &["build", "."])
}

/// How many macro worlds one `vilan check` of `dir` compiled, off the phase
/// row. Each call is its own PROCESS, which is the whole subject here.
fn worlds_compiled(dir: &Path) -> usize {
    worlds_from(dir, &["check", "."])
}

/// The phase-row probe, shared by the `check` and `build` spellings.
fn worlds_from(dir: &Path, arguments: &[&str]) -> usize {
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .current_dir(dir)
        .args(arguments)
        .env("VILAN_PHASE_TIMING", "1")
        .output()
        .expect("run vilan");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the fixture must check cleanly; stderr was: {stderr}"
    );
    let row = stderr
        .lines()
        .find(|line| line.contains("macro-worlds"))
        .unwrap_or_else(|| panic!("no `macro-worlds` row; stderr was: {stderr}"));
    let words: Vec<&str> = row.split_whitespace().collect();
    let index = words
        .iter()
        .position(|word| *word == "macro-worlds")
        .expect("the row carries its label");
    words[index + 1]
        .parse()
        .unwrap_or_else(|_| panic!("the count must be a number: {row}"))
}

#[test]
fn the_second_check_of_an_unchanged_package_compiles_no_macro_world() {
    let dir = temp_package("unchanged");
    assert_eq!(
        worlds_compiled(&dir),
        1,
        "a cold check of a package defining one macro compiles its world"
    );
    assert!(
        cache_file(&dir).is_file(),
        "the cold check must have written the package's expansion table to \
         {}",
        cache_file(&dir).display()
    );
    assert_eq!(
        worlds_compiled(&dir),
        0,
        "the second check is a fresh process, and it must answer from the \
         table on disk without compiling the world at all — that is the whole \
         item"
    );
    // A third, to say the reading is the steady state and not an artefact of
    // the write that preceded it.
    assert_eq!(
        worlds_compiled(&dir),
        0,
        "and it stays zero: reading the table must not invalidate it"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn editing_the_macro_invalidates_by_content() {
    let dir = temp_package("edited");
    assert_eq!(worlds_compiled(&dir), 1, "cold");
    assert_eq!(worlds_compiled(&dir), 0, "warm");
    // The macro's BODY changes, so the definition set's content hash — which is
    // what the key is built from — moves, and the old entry is unreachable
    // rather than wrong. This is the std-edit case in the form a test can
    // reach: std's derive modules are macro-defining files exactly like this
    // one, and `world_key` does not know which package a file came from.
    std::fs::write(dir.join("src/main.vl"), entry(11)).expect("the edited entry");
    assert_eq!(
        worlds_compiled(&dir),
        1,
        "an edited macro must recompile its world: the cached expansion was \
         written for a definition that no longer exists"
    );
    assert_eq!(worlds_compiled(&dir), 0, "and the edit is then itself warm");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_toolchain_stamp_that_is_not_this_compilers_is_not_read() {
    let dir = temp_package("stamped");
    assert_eq!(worlds_compiled(&dir), 1, "cold");
    assert_eq!(worlds_compiled(&dir), 0, "warm");
    // The key covers the macro and its input; it does not and cannot cover the
    // COMPILER, so the file carries the toolchain version and a hash of
    // `macro_std`'s sources in a header. Either moving discards the whole file
    // rather than any single entry, because no entry under an old stamp can be
    // trusted.
    let path = cache_file(&dir);
    let written = std::fs::read_to_string(&path).expect("the written table");
    assert!(
        written.contains("toolchain ") && written.contains("macro_std "),
        "the header must name both stamps: {written}"
    );
    let bumped: String = written
        .lines()
        .map(|line| {
            if line.starts_with("toolchain ") {
                "toolchain 9.99.9-not-this-one".to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<String>>()
        .join("\n");
    std::fs::write(&path, format!("{bumped}\n")).expect("the restamped table");
    assert_eq!(
        worlds_compiled(&dir),
        1,
        "a table written by another toolchain must be discarded, not read"
    );

    // The `macro_std` half of the stamp, the same way. It is hashed rather than
    // versioned because a toolchain built from a checkout can change
    // `macro_std` without changing a version number, which is the tree everyone
    // developing the compiler is standing in.
    let written = std::fs::read_to_string(&path).expect("the rewritten table");
    let restamped: String = written
        .lines()
        .map(|line| {
            if line.starts_with("macro_std ") {
                "macro_std 0000000000000000".to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<String>>()
        .join("\n");
    std::fs::write(&path, format!("{restamped}\n")).expect("the restamped table");
    assert_eq!(
        worlds_compiled(&dir),
        1,
        "a table written against a different `macro_std` must be discarded too"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_corrupt_table_is_ignored_rather_than_fatal() {
    let dir = temp_package("corrupt");
    assert_eq!(worlds_compiled(&dir), 1, "cold");
    assert_eq!(worlds_compiled(&dir), 0, "warm");
    let path = cache_file(&dir);
    // Three shapes of damage, each one a thing a real filesystem produces: a
    // file that is not the format at all, a header with no entries under it,
    // and an entry whose declared length runs past the end (a truncated write,
    // which is what a full disk or a killed process leaves behind). Each must
    // recompile — `worlds_compiled` asserts the exit status, so a fatal read
    // would fail here rather than return a number.
    let stamped = std::fs::read_to_string(&path).expect("the written table");
    let header: String = stamped.lines().take(3).collect::<Vec<&str>>().join("\n");
    for (label, damaged) in [
        (
            "not the format",
            "garbage that is not a table at all\n".to_string(),
        ),
        ("header only, no trailing newline", header.clone()),
        (
            "an entry whose length overruns",
            format!("{header}\n0000000000000001 4096\nshort\n"),
        ),
    ] {
        std::fs::write(&path, &damaged).expect("the damaged table");
        assert_eq!(
            worlds_compiled(&dir),
            1,
            "a corrupt table ({label}) must be ignored and the world \
             recompiled, never fail the check"
        );
        // And the damage is repaired rather than carried: the flush rewrites
        // the file whole, so the next process reads a good table.
        assert_eq!(
            worlds_compiled(&dir),
            0,
            "after recompiling, the table ({label}) must have been rewritten \
             whole and be readable again"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn removing_the_build_directory_means_recompile_everything() {
    // The sentence a user already believes, and the reason the BUILD's table
    // lives in `dist/` rather than in `~/.vilan/`: a machine-global cache keyed
    // on a project path is the thing nobody can reason about from a fresh
    // clone, and a stale one is unreachable to `rm -rf`.
    //
    // Asserted through `build` since N92, which is the goal that owns `dist/`.
    // It used to be asserted through `check`, and that was the defect: `check`
    // emits no artifacts, so it was creating a build directory in a tree nobody
    // asked to build in order to have somewhere to keep the table.
    let dir = temp_package("removed");
    assert_eq!(worlds_compiled_by_build(&dir), 1, "cold");
    assert_eq!(worlds_compiled_by_build(&dir), 0, "warm");
    assert!(
        build_cache_file(&dir).is_file(),
        "a build's table is in the package's own `dist/.cache`"
    );
    std::fs::remove_dir_all(dir.join("dist")).expect("remove the build directory");
    assert_eq!(
        worlds_compiled_by_build(&dir),
        1,
        "`rm -rf dist` must mean recompile everything, macro worlds included"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_check_writes_nothing_at_all_into_the_package() {
    // Tracker N92. `check` used to create `dist/.cache/` — a build directory in
    // a tree nobody asked to build — so a read-only-sounding command mutated
    // the package it was pointed at. N63's ruling stands for the BUILD's table
    // and is asserted above; a check has no artifacts and so no `dist/` of its
    // own, and its table went to `~/.vilan/check-cache/<hash>` instead.
    //
    // The claim is the WHOLE tree, not the absence of one file: anything a
    // check ever starts leaving behind reds here, named.
    let dir = temp_package("check_writes_nothing");
    let before = tree_under(&dir);
    assert_eq!(
        worlds_compiled(&dir),
        1,
        "the fixture must compile a world, or the check under test did nothing"
    );

    assert_eq!(
        tree_under(&dir),
        before,
        "a `vilan check` must leave the package byte-for-byte as it found it — \
         no `dist/`, no cache, nothing"
    );
    assert!(
        !dir.join("dist").exists(),
        "and above all no build directory: `check` emits no artifacts"
    );

    // The table it DID write is out of the tree, and answers the second check.
    assert!(
        cache_file(&dir).is_file(),
        "the check's table is under the user cache root, at {}",
        cache_file(&dir).display()
    );
    assert_eq!(
        worlds_compiled(&dir),
        0,
        "and it is read back by the next process, which is the whole of M33"
    );

    let _ = std::fs::remove_dir_all(cache_dir(&dir).parent().expect("the entry directory"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_check_and_a_build_of_one_package_keep_separate_tables() {
    // Two roots, one package: the build's in `dist/.cache` where `rm -rf dist`
    // reaches it, the check's under the user cache root where nothing in the
    // package can be disturbed by it. Both are content-keyed and stamped, so
    // two tables is a redundancy and never a disagreement — and `rm -rf dist`
    // still means what it says for the build without silently un-warming a
    // check of a tree that was never built.
    let dir = temp_package("two_roots");
    assert_eq!(worlds_compiled(&dir), 1, "the check is cold");
    assert!(!dir.join("dist").exists(), "and created no `dist/`");
    assert_eq!(worlds_compiled_by_build(&dir), 1, "the build is cold too");
    assert!(
        build_cache_file(&dir).is_file(),
        "the build wrote the package's own table"
    );
    assert_eq!(worlds_compiled(&dir), 0, "the check stays warm");
    assert_eq!(worlds_compiled_by_build(&dir), 0, "so does the build");

    std::fs::remove_dir_all(dir.join("dist")).expect("remove the build directory");
    assert_eq!(
        worlds_compiled(&dir),
        0,
        "`rm -rf dist` is the BUILD's gesture: a check of a tree that was never \
         built must not depend on a directory that was never there"
    );

    let _ = std::fs::remove_dir_all(cache_dir(&dir).parent().expect("the entry directory"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every path under `root`, relative and `/`-joined, sorted — directories
/// included, so a directory created and left empty is visible too.
fn tree_under(root: &Path) -> Vec<String> {
    fn walk(root: &Path, at: &Path, found: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(at) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            found.push(
                path.strip_prefix(root)
                    .expect("a path under the root it was read from")
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/"),
            );
            if path.is_dir() {
                walk(root, &path, found);
            }
        }
    }
    let mut found = Vec::new();
    walk(root, root, &mut found);
    found.sort();
    found
}
