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
fn read_dir_all_lists_every_entry_recursively_as_relative_paths() {
    let dir = temp_project("read_dir_all");
    write(&dir, "data/a.txt", "a");
    write(&dir, "data/sub/c.txt", "c");
    write(&dir, "data/sub/deep/d.txt", "d");
    write(
        &dir,
        "probe.vl",
        r#"import std::print;
import std::fs::read_dir_all;

fun main() {
	let entries = read_dir_all("data");
	print(entries.len());
	for entry in entries.sort() {
		print(entry);
	}
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    // Five entries: three files by their RELATIVE paths, plus the two
    // subdirectories as entries of their own — what the host's recursive
    // `readdir` hands back. The probe sorts: the host order is not promised.
    // This exact assertion is also the runtime probe of the host's
    // `{ recursive: true }` support (kolt.local 019) — a node too old for the
    // option (< 18.17) ignores it and lists `data` flat, failing loudly here.
    assert_eq!(
        stdout,
        "5\na.txt\nsub\nsub/c.txt\nsub/deep\nsub/deep/d.txt\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The other half of N25's fix, and the half a Linux machine can actually
/// prove: normalizing the host separator must NOT touch a backslash that is
/// part of a NAME. `\` is a legal filename byte on Unix, so an unconditional
/// rewrite in `__fs_read_dir_all` would corrupt a real file to fix a problem
/// Unix does not have — which is what the first attempt at this fix did. The
/// glue is gated on `path.sep` instead, so this file survives here and the
/// Windows separator is still normalized there.
#[cfg(unix)]
#[test]
fn a_backslash_in_a_name_survives_read_dir_all() {
    let dir = temp_project("read_dir_all_backslash");
    write(&dir, "data/od\\d.txt", "odd");
    write(
        &dir,
        "probe.vl",
        r#"import std::print;
import std::fs::read_dir_all;

fun main() {
	for entry in read_dir_all("data").sort() {
		print(entry);
	}
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(
        stdout, "od\\d.txt\n",
        "a backslash in a NAME is not a separator"
    );
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

// --- atomic replace (kolt.local 031, proposal/filesystem.md §10) ----------

#[test]
fn write_atomic_creates_a_file_that_did_not_exist() {
    let dir = temp_project("atomic_create");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/keep.txt", "sibling");
    write(
        &dir,
        "probe.vl",
        r#"import std::print;
import std::fs::{ read_file_to_str, write_atomic };

fun main() {
	write_atomic("data/store.json", "[{\"id\":1}]");
	print(read_file_to_str("data/store.json"));
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(stdout, "[{\"id\":1}]\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn write_atomic_replaces_an_existing_file_and_leaves_no_temporary_behind() {
    // The temporary is a uniquely-named SIBLING, so a successful replace must
    // leave the target's directory holding exactly what it held before plus
    // nothing: a leaked `.<uuid>.tmp` here would mean the rename never ran.
    let dir = temp_project("atomic_replace");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/store.json", "[{\"id\":1}]");
    write(
        &dir,
        "probe.vl",
        r#"import std::print;
import std::fs::{ read_file_to_str, write_atomic };

fun main() {
	write_atomic("data/store.json", "[]");
	print(read_file_to_str("data/store.json"));
	write_atomic("data/store.json", "[{\"id\":2}]");
	print(read_file_to_str("data/store.json"));
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(stdout, "[]\n[{\"id\":2}]\n");

    let mut left: Vec<String> = std::fs::read_dir(dir.join("data"))
        .expect("the data directory survives")
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(
        left,
        vec!["store.json".to_string()],
        "two atomic replaces must leave the target alone, with no `.tmp` sibling stranded"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn write_atomic_throws_host_side_when_the_target_directory_does_not_exist() {
    // The temporary is a SIBLING of the target, never a system temp file — so
    // a target whose directory is missing fails at the write of the temporary,
    // keeping `write_file`'s throwing posture rather than half-succeeding.
    let dir = temp_project("atomic_missing_dir");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(
        &dir,
        "probe.vl",
        r#"import std::fs::write_atomic;

fun main() {
	write_atomic("nothing/here/store.json", "[]");
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
        "write_atomic into a missing directory should throw host-side, not succeed"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ENOENT"),
        "expected the host ENOENT to surface; stderr was:\n{stderr}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rename_moves_a_file_and_replaces_an_existing_destination() {
    // Both halves matter to `write_atomic`: the source stops existing, and an
    // occupied destination is REPLACED rather than refused — the latter is the
    // whole reason a temp-sibling-plus-rename is an atomic replace at all.
    let dir = temp_project("rename");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/from.txt", "new contents");
    write(&dir, "data/to.txt", "old contents");
    write(
        &dir,
        "probe.vl",
        r#"import std::print;
import std::fs::{ exists, read_file_to_str, rename };

fun main() {
	rename("data/from.txt", "data/to.txt");
	print(exists("data/from.txt"));
	print(read_file_to_str("data/to.txt"));
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(stdout, "false\nnew contents\n");
    let _ = std::fs::remove_dir_all(&dir);
}

// --- the write and directory gaps (kolt.local 031, filesystem.md §3.1 S1) --

/// Runs `vilan run <relative>` inside `dir` expecting a host-side throw, and
/// returns stderr. The whole module throws on failure except `stat` and
/// `remove_dir_all`, so several pins below are assertions about a refusal.
fn run_err(dir: &Path, relative: &str) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", relative])
        .current_dir(dir)
        .output()
        .expect("run vilan");
    assert!(
        !output.status.success(),
        "expected a host-side throw (non-zero exit); stdout was:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A payload chosen to be destroyed by a text round trip: the PNG magic
/// number, then every one of the 256 byte values. Decoding this as UTF-8 and
/// re-encoding does not give it back — which is the 483-to-853-byte favicon
/// lesson (v0.36.0's `serve_build` entry) in fixture form.
fn binary_payload() -> Vec<u8> {
    let mut bytes = vec![0x89u8, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend(0u8..=255u8);
    bytes
}

#[test]
fn write_bytes_round_trips_a_binary_payload_byte_identically() {
    // The gap this whole slice was named for: `read_bytes` landed with F13 and
    // `writeFile` stayed typed `(path, contents: str)`, so bytes could come in
    // and never go out. The proof that matters is not that the call succeeds —
    // it is that the file on the other side is byte-for-byte the file that went
    // in, which a decode-and-re-encode would not be.
    let dir = temp_project("write_bytes");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    let payload = binary_payload();
    std::fs::create_dir_all(dir.join("data")).unwrap();
    std::fs::write(dir.join("data/payload.bin"), &payload).unwrap();
    // The fixture really is one a text round trip would corrupt — otherwise
    // this test would pass over a `write_file` that decoded.
    assert_ne!(
        String::from_utf8_lossy(&payload).as_bytes().len(),
        payload.len(),
        "the fixture must be a payload UTF-8 cannot survive, or it proves nothing"
    );
    write(
        &dir,
        "probe.vl",
        r#"import std::print;
import std::fs::{ read_bytes, write_bytes };

fun main() {
	let bytes = read_bytes("data/payload.bin");
	write_bytes("data/copy.bin", bytes);
	let back = read_bytes("data/copy.bin");
	print(back.len());
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(stdout, format!("{}\n", payload.len()));
    let written = std::fs::read(dir.join("data/copy.bin")).expect("the copy exists");
    assert_eq!(
        written.len(),
        payload.len(),
        "a byte write that changes the LENGTH is the favicon bug wearing a new name"
    );
    assert_eq!(written, payload, "every byte must survive the round trip");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn write_bytes_atomic_replaces_a_binary_file_whole_and_strands_no_temporary() {
    // `write_atomic`'s byte twin: same uniquely-named sibling, same rename, so
    // the same two claims hold — the target is replaced whole, and the
    // temporary is gone. A leaked `.<uuid>.tmp` here means the rename never ran.
    let dir = temp_project("write_bytes_atomic");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    let payload = binary_payload();
    std::fs::create_dir_all(dir.join("data")).unwrap();
    std::fs::write(dir.join("data/payload.bin"), &payload).unwrap();
    std::fs::write(dir.join("data/icon.bin"), b"stale").unwrap();
    write(
        &dir,
        "probe.vl",
        r#"import std::print;
import std::fs::{ read_bytes, write_bytes_atomic };

fun main() {
	let bytes = read_bytes("data/payload.bin");
	write_bytes_atomic("data/icon.bin", bytes);
	print(read_bytes("data/icon.bin").len());
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(stdout, format!("{}\n", payload.len()));
    assert_eq!(
        std::fs::read(dir.join("data/icon.bin")).unwrap(),
        payload,
        "the target must hold the new bytes exactly"
    );
    let mut left: Vec<String> = std::fs::read_dir(dir.join("data"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(
        left,
        vec!["icon.bin".to_string(), "payload.bin".to_string()],
        "an atomic byte replace must leave no `.tmp` sibling behind"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn append_adds_to_the_end_and_creates_a_file_that_was_not_there() {
    // Both halves of `appendFile`'s contract, because both are things a caller
    // relies on: it never truncates what is already there, and a missing file
    // is created rather than an ENOENT.
    let dir = temp_project("append");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/log.txt", "one\n");
    write(
        &dir,
        "probe.vl",
        r#"import std::fs::append;

fun main() {
	append("data/log.txt", "two\n");
	append("data/log.txt", "three\n");
	append("data/fresh.txt", "made\n");
}
main();
"#,
    );
    run_ok(&dir, "probe.vl");
    assert_eq!(
        std::fs::read_to_string(dir.join("data/log.txt")).unwrap(),
        "one\ntwo\nthree\n",
        "append must extend, never truncate"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("data/fresh.txt")).unwrap(),
        "made\n",
        "append to a missing path must create it"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn copy_duplicates_a_file_and_overwrites_an_occupied_destination() {
    // The distinction from `rename` is the point: the SOURCE survives. An
    // occupied destination is replaced, matching `rename` and `write_file`.
    let dir = temp_project("copy");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/from.txt", "new contents");
    write(&dir, "data/to.txt", "old contents");
    write(
        &dir,
        "probe.vl",
        r#"import std::fs::copy;

fun main() {
	copy("data/from.txt", "data/to.txt");
	copy("data/from.txt", "data/made.txt");
}
main();
"#,
    );
    run_ok(&dir, "probe.vl");
    assert_eq!(
        std::fs::read_to_string(dir.join("data/to.txt")).unwrap(),
        "new contents",
        "an occupied destination is overwritten"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("data/made.txt")).unwrap(),
        "new contents",
        "a missing destination is created"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("data/from.txt")).unwrap(),
        "new contents",
        "a copy is not a move — the source must survive"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn remove_deletes_a_file_and_refuses_a_directory() {
    // `unlink` is the file form and the split is the host's: a directory is
    // `remove_dir`/`remove_dir_all`'s job, and calling `remove` on one refuses
    // rather than quietly recursing.
    let dir = temp_project("remove");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/gone.txt", "delete me");
    write(&dir, "data/keep.txt", "keep me");
    write(&dir, "data/sub/inner.txt", "inside");
    write(
        &dir,
        "probe.vl",
        r#"import std::fs::remove;

fun main() {
	remove("data/gone.txt");
}
main();
"#,
    );
    run_ok(&dir, "probe.vl");
    assert!(!dir.join("data/gone.txt").exists(), "the file must be gone");
    assert!(
        dir.join("data/keep.txt").exists(),
        "its neighbour must be untouched"
    );

    write(
        &dir,
        "on_dir.vl",
        r#"import std::fs::remove;

fun main() {
	remove("data/sub");
}
main();
"#,
    );
    let stderr = run_err(&dir, "on_dir.vl");
    assert!(
        stderr.contains("EISDIR") || stderr.contains("EPERM"),
        "removing a directory with the FILE form must refuse (EISDIR on linux, \
         EPERM on macOS); stderr was:\n{stderr}"
    );
    assert!(
        dir.join("data/sub/inner.txt").exists(),
        "the refused directory must be intact"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn update_revises_a_file_in_place_and_leaves_no_temporary() {
    // Read-modify-write with the write half atomic: the revision lands, and
    // `write_atomic`'s temporary is renamed away rather than stranded.
    let dir = temp_project("update");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/store.json", "[1]");
    write(
        &dir,
        "probe.vl",
        r#"import std::print;
import std::fs::{ read_file_to_str, update };

fun main() {
	update("data/store.json", |text| i"{text} + revised");
	print(read_file_to_str("data/store.json"));
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(stdout, "[1] + revised\n");
    assert_eq!(
        std::fs::read_to_string(dir.join("data/store.json")).unwrap(),
        "[1] + revised",
        "the revision must be what is on disk, not just what was printed"
    );
    let left: Vec<String> = std::fs::read_dir(dir.join("data"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        left,
        vec!["store.json".to_string()],
        "update writes atomically, so no `.tmp` sibling may survive it"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn create_dir_makes_one_level_and_refuses_both_an_existing_and_a_missing_parent() {
    // The strict form, kept strict on purpose: it is the module's only
    // exclusive-create primitive until the handle tier's `create_new` lands.
    let dir = temp_project("create_dir");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/keep.txt", "anchor");
    write(
        &dir,
        "probe.vl",
        r#"import std::fs::create_dir;

fun main() {
	create_dir("data/fresh");
}
main();
"#,
    );
    run_ok(&dir, "probe.vl");
    assert!(
        dir.join("data/fresh").is_dir(),
        "the directory must exist afterwards"
    );

    write(
        &dir,
        "again.vl",
        r#"import std::fs::create_dir;

fun main() {
	create_dir("data/fresh");
}
main();
"#,
    );
    let stderr = run_err(&dir, "again.vl");
    assert!(
        stderr.contains("EEXIST"),
        "creating an existing directory must refuse, not succeed quietly; \
         stderr was:\n{stderr}"
    );

    write(
        &dir,
        "deep.vl",
        r#"import std::fs::create_dir;

fun main() {
	create_dir("data/no/such/parent");
}
main();
"#,
    );
    let stderr = run_err(&dir, "deep.vl");
    assert!(
        stderr.contains("ENOENT"),
        "create_dir is ONE level — a missing parent must refuse; stderr was:\n{stderr}"
    );
    assert!(
        !dir.join("data/no").exists(),
        "a refused create_dir must not have made anything"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn create_dir_all_makes_the_whole_chain_and_running_it_twice_is_fine() {
    // Idempotence is the difference from `create_dir` and it is the reason this
    // one exists: "make sure this place exists before I write into it" has no
    // business failing because it already ran.
    let dir = temp_project("create_dir_all");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(
        &dir,
        "probe.vl",
        r#"import std::fs::{ create_dir_all, write_file };

fun main() {
	create_dir_all("data/a/b/c");
	create_dir_all("data/a/b/c");
	write_file("data/a/b/c/inside.txt", "landed");
}
main();
"#,
    );
    run_ok(&dir, "probe.vl");
    assert!(
        dir.join("data/a/b/c").is_dir(),
        "every missing level must have been created"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("data/a/b/c/inside.txt")).unwrap(),
        "landed",
        "the created chain must be writable — the whole point of the call"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn remove_dir_removes_an_empty_directory_and_refuses_a_full_one() {
    // The strict removal, kept for the same reason as the strict create: it
    // refuses to destroy anything the caller did not know was there.
    let dir = temp_project("remove_dir");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    std::fs::create_dir_all(dir.join("data/empty")).unwrap();
    write(&dir, "data/full/inner.txt", "inside");
    write(
        &dir,
        "probe.vl",
        r#"import std::fs::remove_dir;

fun main() {
	remove_dir("data/empty");
}
main();
"#,
    );
    run_ok(&dir, "probe.vl");
    assert!(
        !dir.join("data/empty").exists(),
        "the empty directory must be gone"
    );

    write(
        &dir,
        "full.vl",
        r#"import std::fs::remove_dir;

fun main() {
	remove_dir("data/full");
}
main();
"#,
    );
    let stderr = run_err(&dir, "full.vl");
    assert!(
        stderr.contains("ENOTEMPTY"),
        "a non-empty directory must refuse the strict removal; stderr was:\n{stderr}"
    );
    assert!(
        dir.join("data/full/inner.txt").exists(),
        "the refused directory's contents must be intact"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn remove_dir_all_removes_a_whole_tree_and_a_missing_path_is_a_no_op() {
    // The module's SECOND non-throwing call, and the exception is deliberate:
    // "make sure this is gone" is already satisfied by a path that was never
    // there. The program continuing past the missing-path call is the proof.
    let dir = temp_project("remove_dir_all");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/tree/a.txt", "a");
    write(&dir, "data/tree/sub/deep/b.txt", "b");
    write(&dir, "data/keep.txt", "anchor");
    write(
        &dir,
        "probe.vl",
        r#"import std::print;
import std::fs::remove_dir_all;

fun main() {
	remove_dir_all("data/tree");
	remove_dir_all("data/never/existed/at/all");
	print("survived");
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(
        stdout, "survived\n",
        "a missing path must be a no-op, not a throw — the program has to reach the print"
    );
    assert!(
        !dir.join("data/tree").exists(),
        "the whole tree must be gone, subdirectories included"
    );
    assert!(
        dir.join("data/keep.txt").exists(),
        "nothing outside the named tree may be touched"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn copy_dir_copies_a_whole_tree_and_merges_rather_than_mirrors() {
    // Two claims a caller will hit within a day of using it: the tree arrives
    // in full, and files already at the destination with no counterpart in the
    // source are LEFT ALONE. That is a merge-and-replace, not a mirror.
    let dir = temp_project("copy_dir");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/src/a.txt", "fresh a");
    write(&dir, "data/src/sub/b.txt", "fresh b");
    write(&dir, "data/dest/a.txt", "stale a");
    write(&dir, "data/dest/mine.txt", "only mine");
    write(
        &dir,
        "probe.vl",
        r#"import std::fs::copy_dir;

fun main() {
	copy_dir("data/src", "data/dest");
	copy_dir("data/src", "data/made");
}
main();
"#,
    );
    run_ok(&dir, "probe.vl");
    assert_eq!(
        std::fs::read_to_string(dir.join("data/dest/a.txt")).unwrap(),
        "fresh a",
        "a file already at the destination is overwritten"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("data/dest/sub/b.txt")).unwrap(),
        "fresh b",
        "the copy is recursive — subdirectories come too"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("data/dest/mine.txt")).unwrap(),
        "only mine",
        "a destination file with no counterpart in the source survives: merge, not mirror"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("data/made/sub/b.txt")).unwrap(),
        "fresh b",
        "a destination that does not exist yet is created, parents included"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn scan_dir_reports_each_entry_kind_from_one_host_call() {
    // What `read_dir` discards. The assertion is the kinds, not the names:
    // `read_dir` already pins the names, and the reason `scan_dir` exists is
    // that recovering the kind afterwards costs a `stat` per entry.
    let dir = temp_project("scan_dir");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/a.txt", "a");
    write(&dir, "data/sub/c.txt", "c");
    write(
        &dir,
        "probe.vl",
        r#"import std::print;
import std::fs::scan_dir;

fun main() {
	let entries = scan_dir("data");
	print(entries.len());
	for line in entries.map(|e| i"{e.name} dir={e.is_directory} file={e.is_file} link={e.is_symlink}").sort() {
		print(line);
	}
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(
        stdout, "2\na.txt dir=false file=true link=false\nsub dir=true file=false link=false\n",
        "a file and a directory must be told apart by the scan itself"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn scan_dir_does_not_follow_symlinks_and_reports_other_kinds_as_none_of_the_three() {
    // The two honesty claims in `Entry`'s doc comment, pinned against the real
    // host. A symlink is a symlink and NOT the thing it points at (dirent kinds
    // are lstat's, so a walker can see the link before it follows it into a
    // loop), and a kind outside the three — here a unix socket — reads back as
    // all three `false` rather than being guessed at.
    use std::os::unix::net::UnixListener;

    let dir = temp_project("scan_dir_links");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/real_file.txt", "x");
    std::fs::create_dir_all(dir.join("data/real_dir")).unwrap();
    std::os::unix::fs::symlink("real_file.txt", dir.join("data/link_to_file")).unwrap();
    std::os::unix::fs::symlink("real_dir", dir.join("data/link_to_dir")).unwrap();
    let _socket = UnixListener::bind(dir.join("data/a_socket")).expect("bind a unix socket");
    write(
        &dir,
        "probe.vl",
        r#"import std::print;
import std::fs::scan_dir;

fun main() {
	for line in scan_dir("data").map(|e| i"{e.name} dir={e.is_directory} file={e.is_file} link={e.is_symlink}").sort() {
		print(line);
	}
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(
        stdout,
        "a_socket dir=false file=false link=false\n\
         link_to_dir dir=false file=false link=true\n\
         link_to_file dir=false file=false link=true\n\
         real_dir dir=true file=false link=false\n\
         real_file.txt dir=false file=true link=false\n",
        "a symlink must report as a symlink and not as its target, and a kind \
         outside the three must be all three false"
    );
    drop(_socket);
    let _ = std::fs::remove_dir_all(&dir);
}
