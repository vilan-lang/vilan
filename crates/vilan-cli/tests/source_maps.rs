//! E257 S5 end to end (debugging.md §5.1): what `vilan build` writes beside a
//! script, which builds write it, and the stack node prints for an uncaught
//! panic under `vilan run`.
//!
//! The map's own validity — every segment inside its source's text, a decoder
//! round trip, the JavaScript unchanged by marking it, per construct — is
//! `vilan-core`'s `tests/source_maps.rs`. This file pins the CLI's half: the
//! files, the comment that names the map, the presets, and node reading it.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

mod support;

fn vilan() -> Command {
    Command::new(env!("CARGO_BIN_EXE_vilan"))
}

fn project(tag: &str) -> PathBuf {
    let directory =
        support::scratch_dir(&format!("vilan_source_maps_{tag}_{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("scratch directory");
    directory
}

fn write(directory: &Path, relative: &str, contents: &str) {
    let path = directory.join(relative);
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("directories");
    std::fs::write(&path, contents).expect("write");
}

fn succeeded(output: &Output, what: &str) {
    assert!(
        output.status.success(),
        "{what} failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

const PROGRAM: &str = r#"import std::io::print;

fun area(width: i32, height: i32): i32 {
    let product = width * height;
    product
}

fun main() {
    let first = area(3, 4);
    print(first);
}
"#;

fn map_json(path: &Path) -> serde_json::Value {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    serde_json::from_str(&text).expect("the map is JSON")
}

#[test]
fn a_debug_build_writes_its_map_beside_the_script_and_names_it() {
    let directory = project("debug");
    write(&directory, "main.vl", PROGRAM);
    let output = vilan()
        .args(["build", "main.vl"])
        .current_dir(&directory)
        .output()
        .expect("run vilan");
    succeeded(&output, "vilan build");
    let script = std::fs::read_to_string(directory.join("main.mjs")).expect("main.mjs");
    assert!(
        script.ends_with("\n//# sourceMappingURL=main.mjs.map\n"),
        "the script's last line names its map:\n{script}"
    );
    // The script is `--stdout`'s bundle plus that one line: the map changes
    // no byte of the program.
    let printed = vilan()
        .args(["build", "main.vl", "--stdout"])
        .current_dir(&directory)
        .output()
        .expect("run vilan");
    succeeded(&printed, "vilan build --stdout");
    let printed = String::from_utf8(printed.stdout).expect("utf-8");
    assert!(
        !printed.contains("sourceMappingURL"),
        "--stdout prints a bundle, not a build"
    );
    assert_eq!(
        script.strip_suffix("//# sourceMappingURL=main.mjs.map\n"),
        Some(printed.as_str())
    );
    let map = map_json(&directory.join("main.mjs.map"));
    assert_eq!(map["version"], 3);
    assert_eq!(map["file"], "main.mjs");
    // Beside its source, the user's file is spelled relative to the map and
    // carried whole.
    assert_eq!(map["sources"], serde_json::json!(["main.vl"]));
    assert_eq!(map["sourcesContent"], serde_json::json!([PROGRAM]));
    assert_eq!(map["names"], serde_json::json!(["area"]));
    assert!(
        !map["mappings"].as_str().expect("mappings").is_empty(),
        "a program maps somewhere"
    );
}

#[test]
fn a_release_build_writes_none_unless_asked_and_removes_a_stale_one() {
    let directory = project("release");
    write(&directory, "src/main.vl", PROGRAM);
    write(
        &directory,
        "vilan.toml",
        "[package]\nname = \"maps\"\n\n[build]\npreset = \"release\"\n",
    );
    // A map an earlier debug build left behind.
    write(&directory, "src/main.mjs.map", "{}");
    let output = vilan()
        .arg("build")
        .current_dir(&directory)
        .output()
        .expect("run vilan");
    succeeded(&output, "vilan build (release)");
    let script = std::fs::read_to_string(directory.join("src/main.mjs")).expect("main.mjs");
    assert!(!script.contains("sourceMappingURL"), "{script}");
    assert!(
        !directory.join("src/main.mjs.map").exists(),
        "a build without a map removes the stale one"
    );

    // `--source-maps` asks for one under release; the minified script then
    // names it on a line of its own.
    let output = vilan()
        .args(["build", "--source-maps"])
        .current_dir(&directory)
        .output()
        .expect("run vilan");
    succeeded(&output, "vilan build --source-maps (release)");
    let mapped = std::fs::read_to_string(directory.join("src/main.mjs")).expect("main.mjs");
    assert_eq!(
        mapped,
        format!("{script}\n//# sourceMappingURL=main.mjs.map\n"),
        "the release script, unchanged, plus the comment"
    );
    let map = map_json(&directory.join("src/main.mjs.map"));
    assert_eq!(map["sources"], serde_json::json!(["main.vl"]));
}

#[test]
fn the_manifest_turns_the_debug_presets_map_off() {
    let directory = project("off");
    write(&directory, "src/main.vl", PROGRAM);
    write(
        &directory,
        "vilan.toml",
        "[package]\nname = \"maps\"\n\n[build]\nsource-maps = false\n",
    );
    let output = vilan()
        .arg("build")
        .current_dir(&directory)
        .output()
        .expect("run vilan");
    succeeded(&output, "vilan build");
    let script = std::fs::read_to_string(directory.join("src/main.mjs")).expect("main.mjs");
    assert!(!script.contains("sourceMappingURL"));
    assert!(!directory.join("src/main.mjs.map").exists());
}

#[test]
fn an_uncaught_panic_under_run_names_vl_lines() {
    let directory = project("panic");
    write(
        &directory,
        "main.vl",
        r#"import std::io::print;

fun pick(items: List<i32>, index: usize): i32 {
    items[index]
}

fun main() {
    let items = [1, 2];
    print(pick(items, 5));
}
"#,
    );
    let output = vilan()
        .args(["run", "main.vl"])
        .current_dir(&directory)
        .output()
        .expect("run vilan");
    assert!(!output.status.success(), "the panic exits non-zero");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("panicked at main.vl:4:"),
        "the panic's own location:\n{stderr}"
    );
    // `--enable-source-maps`: the frames are `.vl` places, the function
    // named by its source name.
    let frame = stderr
        .lines()
        .find(|line| line.trim_start().starts_with("at pick ("))
        .unwrap_or_else(|| panic!("a frame for `pick`:\n{stderr}"));
    assert!(frame.contains("main.vl:4:"), "{frame}");
    assert!(
        stderr
            .lines()
            .any(|line| line.trim_start().starts_with("at ") && line.contains("main.vl:9:")),
        "the caller's frame, at `main`'s call:\n{stderr}"
    );
}
