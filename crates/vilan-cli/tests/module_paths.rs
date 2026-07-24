//! End-to-end pins for module resolution's PATH semantics
//! (windows-support.md §5): exact-case resolution, and paths that survive the
//! trip through the loader without a lossy `String` round-trip.
//!
//! These need the real binary reading a real tree — the properties are about
//! bytes on disk, which the in-process pins in `vilan-core` cannot express.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

/// A fresh temp directory for one test's project tree.
fn temp_root(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("vilan_paths_{tag}_{}_{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Writes a single-package project (manifest + files) into `dir`.
fn write_package(dir: &Path, files: &[(&str, &str)]) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("vilan.toml"),
        "[package]\nname = \"paths\"\nroot = \".\"\n",
    )
    .unwrap();
    for (name, contents) in files {
        std::fs::write(dir.join(name), contents).unwrap();
    }
}

/// Runs the `vilan` binary in `dir`. `NO_COLOR` keeps the assertions plain.
fn vilan(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vilan"))
        .current_dir(dir)
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("run vilan")
}

/// Everything the CLI wrote, both streams.
fn combined(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

// --- Exact-case module resolution (windows-support.md §5, ratified call (c)) ---

#[test]
fn an_exact_case_import_resolves_with_no_diagnostics() {
    // The happy path, which is what runs on every compile: the imported name
    // and the file on disk agree byte for byte, so the check is silent and the
    // module loads.
    //
    // The MISMATCH arm cannot be exercised end to end on Linux — a wrong-case
    // import never resolves on a case-sensitive filesystem, so the loader skips
    // it long before the check runs, and the failure is an ordinary
    // unresolved-import error. The windows-latest CI leg is that e2e; the
    // checker itself is pinned directly in `analyzer::path_tests`.
    let root = temp_root("case-exact");
    write_package(
        &root,
        &[
            (
                "main.vl",
                "import std::print;\nimport pkg::helper::greet;\n\nfun main() {\n\tprint(greet());\n}\n",
            ),
            ("helper.vl", "export fun greet(): str {\n\t\"hello\"\n}\n"),
        ],
    );

    let output = vilan(&root, &["run", "main.vl"]);
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert!(text.contains("hello"), "{text}");
    assert!(
        !text.contains("exact case"),
        "an exact-case import must not trip the case check: {text}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_wrong_case_import_does_not_resolve_on_a_case_sensitive_filesystem() {
    // The Linux half of the same rule, and the reason the mismatch arm is CI's
    // to prove: here the wrong spelling simply does not resolve. Windows is
    // where it WOULD resolve — and where the check turns it into a diagnostic
    // instead of a program that builds on one machine and not another.
    let root = temp_root("case-wrong");
    write_package(
        &root,
        &[
            (
                "main.vl",
                "import std::print;\nimport pkg::Helper::greet;\n\nfun main() {\n\tprint(greet());\n}\n",
            ),
            ("helper.vl", "export fun greet(): str {\n\t\"hello\"\n}\n"),
        ],
    );

    let output = vilan(&root, &["build"]);
    assert!(
        !output.status.success(),
        "a wrong-case import must not build: {}",
        combined(&output)
    );
    let _ = std::fs::remove_dir_all(&root);
}

// --- Paths that a lossy `String` round-trip would destroy ---

/// A directory name that is valid on the filesystem but NOT valid UTF-8 — the
/// unix stand-in for a Windows path with an unpaired surrogate. Both are paths
/// `to_string_lossy` mangles into U+FFFD, so a `String` round-trip reopens the
/// WRONG file (windows-support.md §5).
#[cfg(unix)]
fn non_utf8_name() -> std::ffi::OsString {
    use std::os::unix::ffi::OsStringExt;
    std::ffi::OsString::from_vec(b"vilan-\xff\xfe-dir".to_vec())
}

#[cfg(unix)]
#[test]
fn a_package_under_a_non_utf8_directory_resolves_its_own_modules() {
    // `resolve_module_file` used to hand back `to_string_lossy()` Strings that
    // the loader re-parsed into paths: under this directory the sibling module
    // silently failed to open and its exports vanished.
    let root = temp_root("non-utf8-pkg");
    let package = root.join(non_utf8_name());
    write_package(
        &package,
        &[
            (
                "main.vl",
                "import std::print;\nimport pkg::helper::greet;\n\nfun main() {\n\tprint(greet());\n}\n",
            ),
            ("helper.vl", "export fun greet(): str {\n\t\"hello\"\n}\n"),
        ],
    );

    // Run from the PARENT, so the entry path — and therefore the package root
    // the sibling module resolves against — actually carries the non-UTF-8
    // component. (From inside the package the CLI works in `.`-relative paths
    // and never sees it.)
    let entry = Path::new(&non_utf8_name()).join("main.vl");
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .current_dir(&root)
        .env("NO_COLOR", "1")
        .arg("run")
        .arg(&entry)
        .output()
        .expect("run vilan");
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert!(text.contains("hello"), "{text}");
    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn a_std_root_that_is_not_utf8_still_loads_and_still_steers() {
    // The same round-trip, one level up: `$VILAN_STD` reached through a
    // directory name that is not UTF-8. Two things had to survive it — the
    // module FILES (or nothing compiles) and the B4 import-steer inventory,
    // whose `path.to_str()` gate used to drop every std module silently, so a
    // missing import lost its suggestion.
    let real_std = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std");
    assert!(real_std.join("vilan.toml").is_file(), "the std package");

    let root = temp_root("non-utf8-std");
    let link = root.join(non_utf8_name());
    std::os::unix::fs::symlink(&real_std, &link).expect("symlink std under a non-UTF-8 name");
    // `macro_std` is found BESIDE `std`, so it needs a sibling link here too.
    std::os::unix::fs::symlink(real_std.join("../macro_std"), root.join("macro_std"))
        .expect("symlink macro_std beside it");

    let package = root.join("app");
    write_package(
        &package,
        &[("main.vl", "fun main() {\n\tprint(\"hi\");\n}\n")],
    );
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .current_dir(&package)
        .env("NO_COLOR", "1")
        .env("VILAN_STD", &link)
        .args(["build"])
        .output()
        .expect("run vilan");
    let text = combined(&output);
    assert!(
        text.contains("import std::io::print;"),
        "the import steer must survive a non-UTF-8 std path: {text}"
    );

    // And a program that DOES import it compiles through the same root.
    std::fs::write(
        package.join("main.vl"),
        "import std::print;\n\nfun main() {\n\tprint(\"hi\");\n}\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .current_dir(&package)
        .env("NO_COLOR", "1")
        .env("VILAN_STD", &link)
        .args(["build"])
        .output()
        .expect("run vilan");
    assert!(
        output.status.success(),
        "std must load through a non-UTF-8 path: {}",
        combined(&output)
    );
    let _ = std::fs::remove_dir_all(&root);
}
