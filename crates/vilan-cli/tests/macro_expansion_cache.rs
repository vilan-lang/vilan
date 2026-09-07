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
    dir.join("dist").join(".macro-expansions")
}

/// How many macro worlds one `vilan check` of `dir` compiled, off the phase
/// row. Each call is its own PROCESS, which is the whole subject here.
fn worlds_compiled(dir: &Path) -> usize {
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .current_dir(dir)
        .args(["check", "."])
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
    // The sentence a user already believes, and the reason the table lives in
    // `dist/` rather than in `~/.vilan/`: a machine-global cache keyed on a
    // project path is the thing nobody can reason about from a fresh clone, and
    // a stale one is unreachable to `rm -rf`.
    let dir = temp_package("removed");
    assert_eq!(worlds_compiled(&dir), 1, "cold");
    assert_eq!(worlds_compiled(&dir), 0, "warm");
    std::fs::remove_dir_all(dir.join("dist")).expect("remove the build directory");
    assert_eq!(
        worlds_compiled(&dir),
        1,
        "`rm -rf dist` must mean recompile everything, macro worlds included"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
