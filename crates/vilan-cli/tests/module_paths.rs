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
                "import std::io::print;\nimport pkg::helper::greet;\n\nfun main() {\n\tprint(greet());\n}\n",
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
                "import std::io::print;\nimport pkg::Helper::greet;\n\nfun main() {\n\tprint(greet());\n}\n",
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

// --- Exact-case ENTRY resolution (windows-support.md §12's residual) ---

#[test]
fn an_exact_case_entry_builds_with_no_diagnostics() {
    // The entry file gets the same rule as a module: `vilan build Main.vl` on
    // NTFS opens `main.vl` and succeeds, and the identical command fails on
    // Linux. This is the happy path — command line, `[package] entry`, and an
    // `[entry.<name>] path`, all spelled exactly — which must stay silent on
    // every platform.
    //
    // As with the module arm, the MISMATCH cannot be exercised end to end on a
    // case-sensitive filesystem (the wrong spelling never opens, so the failure
    // is the ordinary read error); the windows-latest CI leg is that e2e, and
    // the checker itself is pinned in `main.rs`'s `tests`.
    let root = temp_root("entry-case");
    write_package(
        &root,
        &[(
            "main.vl",
            "import std::io::print;\n\nfun main() {\n\tprint(\"hi\");\n}\n",
        )],
    );

    // 1. The path named on the command line.
    let output = vilan(&root, &["run", "main.vl"]);
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert!(text.contains("hi"), "{text}");
    assert!(
        !text.contains("exact case"),
        "an exact-case entry must not trip the check: {text}"
    );

    // 2. `[package] entry`, resolved against the package root.
    std::fs::write(
        root.join("vilan.toml"),
        "[package]\nname = \"paths\"\nroot = \".\"\nentry = \"main.vl\"\n",
    )
    .unwrap();
    let output = vilan(&root, &["build"]);
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert!(!text.contains("exact case"), "{text}");

    // 3. An `[entry.<name>] path` through a subdirectory — the shape whose
    //    DIRECTORY component the checker also covers.
    std::fs::create_dir_all(root.join("web")).unwrap();
    std::fs::write(
        root.join("web/client.vl"),
        "fun main() {\n\tlet ready = true;\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("vilan.toml"),
        "[package]\nname = \"paths\"\nroot = \".\"\n\n\
         [entry.app]\npath = \"main.vl\"\n\n\
         [entry.client]\npath = \"web/client.vl\"\ntarget = \"browser\"\n",
    )
    .unwrap();
    let output = vilan(&root, &["build"]);
    let text = combined(&output);
    assert!(output.status.success(), "{text}");
    assert!(!text.contains("exact case"), "{text}");

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
                "import std::io::print;\nimport pkg::helper::greet;\n\nfun main() {\n\tprint(greet());\n}\n",
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
    // A name NO prelude carries, so the steer is what is under test: `print`
    // used to serve here and is now ambient, which compiled the fixture clean.
    write_package(
        &package,
        &[(
            "main.vl",
            "fun main() {\n\tlet m: Map<str, i32> = Map::new();\n}\n",
        )],
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
        text.contains("import std::map::Map;"),
        "the import steer must survive a non-UTF-8 std path: {text}"
    );

    // And a program that DOES import it compiles through the same root.
    std::fs::write(
        package.join("main.vl"),
        "import std::io::print;\n\nfun main() {\n\tprint(\"hi\");\n}\n",
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

// --- Which `std` a path-addressed file compiles against (tracker N56) --------
//
// A file names a location, and a location decides a toolchain. Before N56 the
// PROCESS working directory got a vote: std resolution walked the entry's
// ancestors for a `vilan/std` checkout and then walked the shell's, so
// `vilan check ~/code/app/src/x.vl` typed by someone standing in this repository
// compiled that application against the working tree's std — 37
// `macro PartialEq's definition did not compile` from here, 0 from the
// application's own directory, on one unchanged file. `file_project` has
// resolved the PACKAGE from the file's own location since G20; these pin that
// the std comes from the same place, and that the one case where the working
// directory still legitimately answers — a bare file belonging to no package —
// is the case that keeps it.

/// A second checkout, standing in for another clone of the toolchain: a
/// `vilan/std` package with no modules at all, so anything compiled against it
/// fails on its first import. Returns the directory to stand in.
fn a_stand_in_checkout(root: &Path) -> PathBuf {
    let other = root.join("other");
    let std = other.join("vilan").join("std");
    std::fs::create_dir_all(std.join("src")).unwrap();
    std::fs::write(std.join("vilan.toml"), "[library]\nname = \"std\"\n").unwrap();
    let macro_std = other.join("vilan").join("macro_std");
    std::fs::create_dir_all(macro_std.join("src")).unwrap();
    std::fs::write(
        macro_std.join("vilan.toml"),
        "[library]\nname = \"macro_std\"\n",
    )
    .unwrap();
    other
}

/// `vilan` with `$VILAN_STD` explicitly OUT of the environment: these tests are
/// about the resolution that runs when nothing names a std, and an inherited
/// variable would answer for all of them and pin nothing.
fn vilan_without_std_env(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vilan"))
        .current_dir(dir)
        .env("NO_COLOR", "1")
        .env_remove("VILAN_STD")
        .args(args)
        .output()
        .expect("run vilan")
}

#[test]
fn a_file_in_a_package_takes_its_own_toolchains_std_from_any_directory() {
    let root = temp_root("std-by-location");
    let package = root.join("app");
    write_package(
        &package,
        &[(
            "main.vl",
            "import std::io::print;\n\nfun main() {\n\tprint(\"hi\");\n}\n",
        )],
    );
    let other = a_stand_in_checkout(&root);
    let entry = package.join("main.vl");
    let entry = entry.to_str().expect("a UTF-8 temp path");

    // The same absolute path, checked from two directories. Neither is named in
    // the command; only one of them holds a checkout.
    let from_its_own_directory = vilan_without_std_env(&package, &["check", entry]);
    let from_another_checkout = vilan_without_std_env(&other, &["check", entry]);

    assert!(
        from_its_own_directory.status.success(),
        "the fixture must be clean where its own toolchain answers: {}",
        combined(&from_its_own_directory)
    );
    assert_eq!(
        combined(&from_another_checkout),
        combined(&from_its_own_directory),
        "one file, one verdict: the working directory is not a toolchain"
    );
    assert_eq!(
        from_another_checkout.status.code(),
        from_its_own_directory.status.code()
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_bare_file_belonging_to_no_package_still_takes_the_working_directorys_checkout() {
    // The case the rule KEEPS, and the control that makes the pin above a claim
    // rather than a tautology: it is the same stand-in checkout, and it does
    // change the answer — for a scratch program with no `vilan.toml` at or above
    // it, which has no toolchain of its own for the shell's to override.
    let root = temp_root("std-bare-file");
    let bare = root.join("bare");
    std::fs::create_dir_all(&bare).unwrap();
    let scratch = bare.join("scratch.vl");
    std::fs::write(
        &scratch,
        "import std::io::print;\n\nfun main() {\n\tprint(\"hi\");\n}\n",
    )
    .unwrap();
    let other = a_stand_in_checkout(&root);
    let path = scratch.to_str().expect("a UTF-8 temp path");

    let from_its_own_directory = vilan_without_std_env(&bare, &["check", path]);
    assert!(
        from_its_own_directory.status.success(),
        "with no checkout anywhere, the binary's own std compiles it: {}",
        combined(&from_its_own_directory)
    );
    let from_the_checkout = vilan_without_std_env(&other, &["check", path]);
    assert!(
        !from_the_checkout.status.success(),
        "a bare file compiles against the checkout the shell is standing in, and \
         this one has an empty std — so the stand-in checkout above is reachable \
         and it does decide verdicts: {}",
        combined(&from_the_checkout)
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_cascade_of_macro_definition_failures_names_the_std_that_was_resolved() {
    // Which std answered is invisible on a clean compile and invisible on a
    // broken one, and this is the failure where it is the whole question: a
    // mismatched std fails EVERY derive in the file at once, so the screen
    // fills with a message about the user's own code. Two failing macro
    // definitions are a cascade; one is a macro.
    let root = temp_root("std-named-in-cascade");
    let package = root.join("app");
    write_package(
        &package,
        &[(
            "main.vl",
            "fun main() {\n\tlet first = macro { 42 };\n\tlet second = macro { 43 };\n}\n",
        )],
    );
    let std = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std");
    let entry = package.join("main.vl");
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .current_dir(&package)
        .env("NO_COLOR", "1")
        .env("VILAN_STD", &std)
        .arg("check")
        .arg(&entry)
        .output()
        .expect("run vilan");
    let text = combined(&output);
    assert!(!output.status.success(), "{text}");
    assert_eq!(
        text.matches("Error: the `macro { .. }` block's definition did not compile")
            .count(),
        2,
        "the fixture must produce a cascade: {text}"
    );
    assert!(
        text.contains("2 macro definitions failed to compile"),
        "the note counts them: {text}"
    );
    assert!(
        text.contains(&std.display().to_string()),
        "and names the std this compile resolved: {text}"
    );

    // One failure is not a cascade, and a note on it would be noise.
    std::fs::write(
        package.join("main.vl"),
        "fun main() {\n\tlet only = macro { 42 };\n}\n",
    )
    .unwrap();
    let single = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .current_dir(&package)
        .env("NO_COLOR", "1")
        .env("VILAN_STD", &std)
        .arg("check")
        .arg(&entry)
        .output()
        .expect("run vilan");
    let text = combined(&single);
    assert!(
        text.contains("definition did not compile") && !text.contains("macro definitions failed"),
        "a lone failure carries no note: {text}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
