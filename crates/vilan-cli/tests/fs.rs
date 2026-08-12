//! End-to-end: `std::fs`'s surface (F13, fullstack-dx.md §9.3) against the
//! real host filesystem — `node:fs/promises` under the hood, so these run the
//! actual host bindings rather than pinning generated JS text. Two postures
//! are proven distinct: `read_bytes`/`read_dir`/`read_file_to_str` all throw
//! host-side on ANY failure (a missing path included, same as before this
//! surface grew), while `stat` alone is a non-throwing probe (`None` for a
//! missing path, everything else still throws).

use std::path::{Path, PathBuf};
use std::process::Command;

/// A fresh temp directory for the test's project tree.
fn temp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vilan_fs_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Writes `contents` to `dir/relative`, creating parent directories.
fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// Runs `vilan run <relative>` inside `dir`, asserting success, and returns
/// stdout.
fn run_ok(dir: &Path, relative: &str) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", relative])
        .current_dir(dir)
        .output()
        .expect("run vilan");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "vilan run failed:\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
}

#[test]
fn read_bytes_reads_the_real_bytes_a_buffer_hands_back() {
    let dir = temp_project("read_bytes");
    // Not valid UTF-8 on its own (a lone continuation byte, 0x80) — proves
    // `read_bytes` is a true binary read, not `read_file_bytes`'s old
    // decode-to-str behavior wearing a new name.
    write(&dir, "data/blob.bin", "");
    std::fs::write(dir.join("data/blob.bin"), [0x41u8, 0x42, 0x80, 0x43]).unwrap();
    write(
        &dir,
        "probe.vl",
        r#"import std::print;
import std::fs::read_bytes;

fun main() {
	let bytes = read_bytes("data/blob.bin");
	print(bytes.len());
	print(bytes.get(0));
	print(bytes.get(1));
	print(bytes.get(2));
	print(bytes.get(3));
	print(bytes.to_hex());
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(stdout, "4\n65\n66\n128\n67\n41428043\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn read_dir_lists_entry_names_flat() {
    let dir = temp_project("read_dir");
    write(&dir, "data/a.txt", "a");
    write(&dir, "data/b.txt", "b");
    write(&dir, "data/sub/c.txt", "c"); // a subdirectory entry, not descended into
    write(
        &dir,
        "probe.vl",
        r#"import std::print;
import std::fs::read_dir;

fun main() {
	let names = read_dir("data");
	print(names.len());
	for name in names.sort() {
		print(name);
	}
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    // Flat: three entries (two files, one subdirectory NAME, not walked into).
    assert_eq!(stdout, "3\na.txt\nb.txt\nsub\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stat_reports_size_mtime_and_kind_for_a_file_and_a_directory() {
    let dir = temp_project("stat_hit");
    write(&dir, "data/five.txt", "12345");
    write(
        &dir,
        "probe.vl",
        r#"import std::print;
import std::fs::stat;
import std::option::Option::{ self, Some, None };

fun main() {
	match stat("data/five.txt") {
		Some(let info) => {
			print(info.size);
			print(info.is_directory);
			print(info.modified_at_ms > 0.0);
		},
		None => print("MISSING"),
	}
	match stat("data") {
		Some(let info) => print(info.is_directory),
		None => print("MISSING"),
	}
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(stdout, "5\nfalse\ntrue\ntrue\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stat_on_a_missing_path_is_none_not_a_throw() {
    let dir = temp_project("stat_miss");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n"); // just for a stable cwd
    write(
        &dir,
        "probe.vl",
        r#"import std::print;
import std::fs::stat;
import std::option::Option::{ self, Some, None };

fun main() {
	match stat("nothing/here.txt") {
		Some(let _info) => print("SHOULD-BE-NONE"),
		None => print("none-as-expected"),
	}
	// A probe doesn't stop the program: prove ordinary control flow resumes.
	print("still-running");
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(stdout, "none-as-expected\nstill-running\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn read_bytes_still_throws_host_side_on_a_missing_path() {
    // The DELIBERATE split (F13, fs.vl's header comment): unlike `stat`,
    // `read_bytes` keeps `read_file_to_str`'s old throwing posture — a missing
    // path is a bug the same as a permissions error, not a probe result.
    let dir = temp_project("read_bytes_missing");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(
        &dir,
        "probe.vl",
        r#"import std::print;
import std::fs::read_bytes;

fun main() {
	print(read_bytes("nothing/here.bin").len());
}
main();
"#,
    );
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", "probe.vl"])
        .current_dir(&dir)
        .output()
        .expect("run vilan");
    assert!(
        !output.status.success(),
        "read_bytes on a missing path should throw host-side (exit non-zero), not succeed"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ENOENT"),
        "expected the host ENOENT to surface; stderr was:\n{stderr}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn read_file_to_str_is_unaffected_by_the_read_file_bytes_rename() {
    // `read_file_bytes` (misleadingly named — it decoded to `str`) was renamed
    // to `read_file_encoded`; `read_file_to_str`, its sole caller, must keep
    // working unchanged.
    let dir = temp_project("read_to_str");
    write(&dir, "data/greeting.txt", "hello, fs");
    write(
        &dir,
        "probe.vl",
        r#"import std::print;
import std::fs::read_file_to_str;

fun main() {
	print(read_file_to_str("data/greeting.txt"));
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(stdout, "hello, fs\n");
    let _ = std::fs::remove_dir_all(&dir);
}
