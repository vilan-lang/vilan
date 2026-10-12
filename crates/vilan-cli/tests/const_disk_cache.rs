//! The const cache survives the PROCESS (tracker M135, incremental-analysis.md §5).
//!
//! S4's const cache is content-keyed and id-free (`const_cache.rs`: the lowered
//! program IS the key), but lived in process memory only, so every cold `vilan
//! check` re-evaluated the same sites (kolt's client: 112 misses, 173-251 ms of
//! const pass, on every run). The table now sits beside the macro-expansion
//! table in the package's cache directory.
//!
//! **What the pins assert is the COUNT** of const hits and misses, read off the
//! `VILAN_COUNTERS` line rather than a clock: the second cold check of a
//! package reports a hit for every site the first evaluated, and the four ways
//! a hit must NOT happen each get a test - a project file the site read has
//! changed (the transcript replay), a table another BUILD wrote (G28's build
//! identity), a corrupt file (a miss, never fatal), and a toolchain version that
//! is not this one's.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

mod support;

const MAIN: &str = "import std::io::print;\nimport std::web::asset;\n\n\
fun square(n: i32): i32 {\n\tn * n\n}\n\n\
fun main() {\n\
\tlet folded = const 1 + 2 * 3;\n\
\tlet squared = const square(4);\n\
\tlet digest = const asset::digest(\"data.txt\");\n\
\tprint(i\"{folded} {squared} {digest}\");\n\
}\n";

fn package(name: &str) -> PathBuf {
    let dir = support::scratch_root().join(format!(
        "vilan-m135-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).expect("a scratch package");
    std::fs::write(
        dir.join("vilan.toml"),
        format!("[package]\nname = \"{name}\"\n"),
    )
    .expect("the manifest");
    std::fs::write(dir.join("src/main.vl"), MAIN).expect("the entry");
    std::fs::write(dir.join("src/data.txt"), "first contents\n").expect("the data file");
    dir
}

/// Where the CLI keeps a CHECK's tables (`expansion_cache_root`): the user cache
/// root, keyed by the package's canonical path - computed with the CLI's own
/// hasher so the pin cannot assert about a file nobody writes.
fn table(dir: &Path) -> PathBuf {
    let canonical = vilan_core::util::canonical_path(dir);
    let mut hasher = DefaultHasher::new();
    canonical.hash(&mut hasher);
    vilan_embedded::default_check_cache_root()
        .join(format!("{:016x}", hasher.finish()))
        .join(".cache")
        .join("const-sites")
}

/// (hits, misses) of one `vilan check` of `dir` - each call is its own PROCESS,
/// which is the whole subject.
fn const_counts(dir: &Path) -> (u64, u64) {
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .current_dir(dir)
        .arg("check")
        .arg(".")
        .env("VILAN_COUNTERS", "1")
        .output()
        .expect("run vilan");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the fixture must check cleanly; stderr was: {stderr}"
    );
    let line = stderr
        .lines()
        .find(|line| line.contains("const-hits="))
        .unwrap_or_else(|| panic!("no incremental counters line; stderr was: {stderr}"));
    let read = |label: &str| -> u64 {
        line.split_whitespace()
            .find_map(|word| word.strip_prefix(label))
            .and_then(|digits| digits.parse().ok())
            .unwrap_or_else(|| panic!("no {label} in: {line}"))
    };
    (read("const-hits="), read("const-misses="))
}

#[test]
fn the_second_cold_check_hits_every_site_the_first_evaluated() {
    let dir = package("second");
    assert_eq!(const_counts(&dir), (0, 3), "cold: three sites evaluated");
    assert!(table(&dir).is_file(), "the first check wrote its table");
    assert_eq!(
        const_counts(&dir),
        (3, 0),
        "a fresh process serves every site from the table"
    );
    assert_eq!(const_counts(&dir), (3, 0), "and again");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_hit_is_served_only_after_the_files_the_site_read_answer_the_same() {
    let dir = package("replay");
    assert_eq!(const_counts(&dir), (0, 3));
    // The digest site read `data.txt`: its recorded answer is the old digest.
    std::fs::write(dir.join("src/data.txt"), "second contents\n").expect("edit the data file");
    assert_eq!(
        const_counts(&dir),
        (2, 1),
        "the two pure sites hit, the site that read the changed file re-evaluates"
    );
    assert_eq!(
        const_counts(&dir),
        (3, 0),
        "and the new answer is remembered"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_table_another_build_or_toolchain_wrote_is_not_read() {
    let dir = package("stamp");
    assert_eq!(const_counts(&dir), (0, 3));
    let path = table(&dir);
    let written = std::fs::read_to_string(&path).expect("the written table");
    let header: Vec<&str> = written.lines().take(3).collect();
    assert_eq!(header[0], "vilan-const-cache 1");
    assert!(header[1].starts_with("toolchain "), "{header:?}");
    assert_eq!(
        header[2],
        format!("build {}", vilan_core::BUILD_ID),
        "the stamp is the analyzer's build identity (G28)"
    );
    for (prefix, replacement) in [
        ("build ", "build 0123456789abcdef"),
        ("toolchain ", "toolchain 9.99.9-not-this-one"),
    ] {
        let restamped: String = written
            .lines()
            .map(|line| {
                if line.starts_with(prefix) {
                    replacement.to_string()
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<String>>()
            .join("\n");
        std::fs::write(&path, format!("{restamped}\n")).expect("the restamped table");
        assert_eq!(
            const_counts(&dir),
            (0, 3),
            "a table stamped `{replacement}` must be discarded, not read"
        );
        assert_eq!(const_counts(&dir), (3, 0), "and the rewrite is readable");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_corrupt_table_is_a_miss_and_is_repaired() {
    let dir = package("corrupt");
    assert_eq!(const_counts(&dir), (0, 3));
    let path = table(&dir);
    let good = std::fs::read_to_string(&path).expect("the written table");
    let header: String = good.lines().take(3).collect::<Vec<&str>>().join("\n");
    for (label, damaged) in [
        (
            "not the format",
            "garbage that is not a table\n".to_string(),
        ),
        ("header only", format!("{header}\n")),
        (
            "a length that overruns",
            format!("{header}\n0000000000000001 4096\nshort\n"),
        ),
        (
            "a payload that does not decode",
            format!("{header}\n0000000000000001 5\nzzzzz\n"),
        ),
        ("a truncated entry", good[..good.len() - 7].to_string()),
    ] {
        std::fs::write(&path, damaged).expect("the damaged table");
        let (hits, misses) = const_counts(&dir);
        assert_eq!(
            hits + misses,
            3,
            "a corrupt table ({label}) must be ignored and never fail the check"
        );
        assert!(
            misses > 0 || label == "header only",
            "{label}: {hits}/{misses}"
        );
        assert_eq!(
            const_counts(&dir),
            (3, 0),
            "after the check the table ({label}) is whole again"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
