//! End-to-end: `std::fs`'s surface (F13, fullstack-dx.md §9.3) against the
//! real host filesystem — `node:fs/promises` under the hood, so these run the
//! actual host bindings rather than pinning generated JS text. Two postures
//! are proven distinct: `read_bytes`/`read_dir`/`read_file_to_str` all throw
//! host-side on ANY failure (a missing path included, same as before this
//! surface grew), while `stat` alone is a non-throwing probe (`None` for a
//! missing path, everything else still throws).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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

/// How long a watch probe gets before the run is declared hung.
const WATCH_TIMEOUT: Duration = Duration::from_secs(30);

/// `run_ok`, under a deadline — for the watch probes, whose whole point is
/// that they TERMINATE. A `Watcher` polls on a host timer and a pending host
/// timer keeps node alive, so a watcher that failed to stop would hang the
/// suite forever instead of failing it (that is exactly what happens with
/// `impl Watcher with Drop` removed, which is how these pins were
/// plant-proven). Streams go to files rather than pipes so a child cannot
/// block on a full pipe buffer while this thread polls for its exit; they sit
/// beside the project root, never inside a watched directory.
fn run_ok_within(dir: &Path, relative: &str, limit: Duration) -> String {
    let stdout_path = dir.join("probe.stdout");
    let stderr_path = dir.join("probe.stderr");
    let mut child = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", relative])
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&stdout_path).expect("capture stdout"))
        .stderr(std::fs::File::create(&stderr_path).expect("capture stderr"))
        .spawn()
        .expect("run vilan");
    let deadline = Instant::now() + limit;
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll vilan") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "`vilan run {relative}` did not finish within {limit:?} — a watcher that \
                 never stopped is holding the event loop open"
            );
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let stdout = std::fs::read_to_string(&stdout_path).unwrap_or_default();
    assert!(
        status.success(),
        "vilan run failed:\nstdout:\n{stdout}\nstderr:\n{}",
        std::fs::read_to_string(&stderr_path).unwrap_or_default()
    );
    stdout
}

/// The `describe`/`show` preamble every watch probe shares: pull one change
/// and print it as `<kind> <path>`, which is the whole observable surface.
const WATCH_PREAMBLE: &str = r#"import std::io::print;
import std::drop::drop;
import std::fs;
import std::fs::{ Change, ChangeKind, Watcher, remove, write_file };
import std::time::sleep;

fun describe(change: Change): str {
	match change.kind {
		ChangeKind::Created => i"created {change.path}",
		ChangeKind::Modified => i"modified {change.path}",
		ChangeKind::Removed => i"removed {change.path}",
	}
}

fun show(watcher: Watcher) {
	print(describe(watcher.next()));
}
"#;

/// A watch probe: the shared preamble plus `body` as `main`.
fn watch_probe(body: &str) -> String {
    format!("{WATCH_PREAMBLE}\nfun main() {{\n{body}}}\nmain();\n")
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
        r#"import std::io::print;
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
        r#"import std::io::print;
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
        r#"import std::io::print;
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
        r#"import std::io::print;
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
        r#"import std::io::print;
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
        r#"import std::io::print;
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
        r#"import std::io::print;
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
        r#"import std::io::print;
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
        r#"import std::io::print;
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
        r#"import std::io::print;
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
        r#"import std::io::print;
import std::fs::{ read_file_to_str, rename, stat };

fun main() {
	rename("data/from.txt", "data/to.txt");
	print(stat("data/from.txt").is_none());
	print(read_file_to_str("data/to.txt"));
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(stdout, "true\nnew contents\n");
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
        String::from_utf8_lossy(&payload).len(),
        payload.len(),
        "the fixture must be a payload UTF-8 cannot survive, or it proves nothing"
    );
    write(
        &dir,
        "probe.vl",
        r#"import std::io::print;
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
        r#"import std::io::print;
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
        r#"import std::io::print;
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
        r#"import std::io::print;
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
        r#"import std::io::print;
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
        r#"import std::io::print;
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

// --- the handle tier (kolt.local 031 S3, filesystem.md §3.2/§5) ------------
//
// `File` against the real host `FileHandle`. Two spellings matter beyond the
// ordinary surface: the postfix-off-an-awaited-call idiom (B141's historically
// broken shape, fixed in Order 13 and pinned POSITIVE here — pre-fix these
// printed `undefined` with a clean exit), and the two close paths (Q1's
// ruling: `drop` initiates the close without awaiting it, `with_file` awaits).

#[test]
fn read_at_reads_positionally_with_short_reads_and_zero_at_eof() {
    // The positional primitive's three answers: a full buffer, a short read
    // near the end (normal, not an error), and 0 at end of file.
    let dir = temp_project("file_read_at");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/ten.txt", "0123456789");
    write(
        &dir,
        "probe.vl",
        r#"import std::io::print;
import std::bytes::{ Bytes, decode_utf8 };
import std::fs::File;
import std::drop::drop;

fun main() {
	let file = File::open("data/ten.txt");
	let buffer = Bytes::alloc(4);
	print(file.read_at(buffer, 3));
	print(decode_utf8(buffer.slice(0, 4)));
	print(file.read_at(buffer, 8));
	print(decode_utf8(buffer.slice(0, 2)));
	print(file.read_at(buffer, 100));
	drop(file);
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(
        stdout, "4\n3456\n2\n89\n0\n",
        "read_at must fill from the position, read short near the end, and \
         answer 0 (not throw) past it"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_postfix_read_and_stat_off_the_awaited_constructor_read_the_value() {
    // B141's spellings as POSITIVE runtime tests (filesystem.md §11.1 made
    // the fix S3's prerequisite; this inverts that argument into a gate): a
    // postfix chain straight off the implicitly-awaited `File::open`. Before
    // the Order 13 fix, both printed `undefined` and exited 0.
    let dir = temp_project("file_postfix");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/ten.txt", "0123456789");
    write(
        &dir,
        "probe.vl",
        r#"import std::io::print;
import std::bytes::Bytes;
import std::fs::File;

fun main() {
	let buffer = Bytes::alloc(4);
	print(File::open("data/ten.txt").read_at(buffer, 0));
	print(File::open("data/ten.txt").stat().size);
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(
        stdout, "4\n10\n",
        "the postfix-off-an-awaited-constructor idiom must read the VALUE, \
         not the promise"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn create_truncates_what_was_there_and_write_at_lands_at_the_position() {
    let dir = temp_project("file_create");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/out.txt", "stale contents that must vanish");
    write(
        &dir,
        "probe.vl",
        r#"import std::bytes::encode_utf8;
import std::fs::File;
import std::drop::drop;

fun main() {
	let file = File::create("data/out.txt");
	file.write_at(encode_utf8("fresh"), 0);
	file.write_at(encode_utf8("!"), 5);
	drop(file);
}
main();
"#,
    );
    run_ok(&dir, "probe.vl");
    assert_eq!(
        std::fs::read_to_string(dir.join("data/out.txt")).unwrap(),
        "fresh!",
        "create must truncate, and each write must land at its position"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn modify_edits_in_place_without_truncating() {
    // "r+": read and write through one handle, nothing truncated on open.
    let dir = temp_project("file_modify");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/ten.txt", "0123456789");
    write(
        &dir,
        "probe.vl",
        r#"import std::io::print;
import std::bytes::{ Bytes, encode_utf8 };
import std::fs::File;
import std::drop::drop;

fun main() {
	let file = File::modify("data/ten.txt");
	let buffer = Bytes::alloc(2);
	print(file.read_at(buffer, 0));
	print(file.write_at(encode_utf8("AB"), 1));
	drop(file);
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(stdout, "2\n2\n");
    assert_eq!(
        std::fs::read_to_string(dir.join("data/ten.txt")).unwrap(),
        "0AB3456789",
        "modify must edit in place: bytes outside the write untouched"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn create_new_claims_a_name_exclusively_and_refuses_a_second_claim() {
    // "wx" — the exclusive create: the primitive a lockfile package would be
    // built from (filesystem.md §3.3/§9). The refusal must leave the first
    // claimant's file intact.
    let dir = temp_project("file_create_new");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    std::fs::create_dir_all(dir.join("data")).unwrap();
    write(
        &dir,
        "probe.vl",
        r#"import std::bytes::encode_utf8;
import std::fs::File;
import std::drop::drop;

fun main() {
	let file = File::create_new("data/claim.txt");
	file.write_at(encode_utf8("mine"), 0);
	drop(file);
}
main();
"#,
    );
    run_ok(&dir, "probe.vl");
    assert_eq!(
        std::fs::read_to_string(dir.join("data/claim.txt")).unwrap(),
        "mine"
    );

    write(
        &dir,
        "again.vl",
        r#"import std::fs::File;
import std::drop::drop;

fun main() {
	let file = File::create_new("data/claim.txt");
	drop(file);
}
main();
"#,
    );
    let stderr = run_err(&dir, "again.vl");
    assert!(
        stderr.contains("EEXIST"),
        "a second exclusive claim must refuse with EEXIST; stderr was:\n{stderr}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("data/claim.txt")).unwrap(),
        "mine",
        "the refused claim must not have touched the first claimant's file"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn append_to_appends_and_ignores_write_at_position() {
    // The host-semantics honesty note pinned (POSIX `O_APPEND`, and the doc
    // comment says so): on an append handle every write lands at the END —
    // `write_at`'s position is ignored.
    let dir = temp_project("file_append_to");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/log.txt", "one\n");
    write(
        &dir,
        "probe.vl",
        r#"import std::bytes::encode_utf8;
import std::fs::File;
import std::drop::drop;

fun main() {
	let file = File::append_to("data/log.txt");
	file.write_at(encode_utf8("two\n"), 0);
	drop(file);
}
main();
"#,
    );
    run_ok(&dir, "probe.vl");
    assert_eq!(
        std::fs::read_to_string(dir.join("data/log.txt")).unwrap(),
        "one\ntwo\n",
        "an append handle must append even when the write names position 0"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn file_open_on_a_missing_path_throws_enoent() {
    // The module's throwing posture holds for the handle tier: `open` means
    // "this must exist" — a caller for whom absence is ordinary probes with
    // `stat` first.
    let dir = temp_project("file_open_missing");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(
        &dir,
        "probe.vl",
        r#"import std::fs::File;
import std::drop::drop;

fun main() {
	let file = File::open("nothing/here.txt");
	drop(file);
}
main();
"#,
    );
    let stderr = run_err(&dir, "probe.vl");
    assert!(
        stderr.contains("ENOENT"),
        "opening a missing path must surface the host ENOENT; stderr was:\n{stderr}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn truncate_and_stat_report_through_the_handle() {
    let dir = temp_project("file_truncate");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/ten.txt", "0123456789");
    write(
        &dir,
        "probe.vl",
        r#"import std::io::print;
import std::fs::File;
import std::drop::drop;

fun main() {
	let file = File::modify("data/ten.txt");
	print(file.stat().size);
	file.truncate(4);
	print(file.stat().size);
	file.truncate(6);
	print(file.stat().size);
	drop(file);
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(
        stdout, "10\n4\n6\n",
        "truncate must shrink and extend, and the handle's stat must see each"
    );
    assert_eq!(
        std::fs::read(dir.join("data/ten.txt")).unwrap(),
        b"0123\0\0",
        "the extension past the truncated end must be zero-filled"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn sync_and_data_sync_complete_without_error() {
    // Durability against power loss is not host-testable; what is pinned is
    // that the two bindings reach the real host calls and succeed — `sync` is
    // the capability nothing path-addressed in the module has.
    let dir = temp_project("file_sync");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    std::fs::create_dir_all(dir.join("data")).unwrap();
    write(
        &dir,
        "probe.vl",
        r#"import std::io::print;
import std::bytes::encode_utf8;
import std::fs::File;
import std::drop::drop;

fun main() {
	let file = File::create("data/durable.txt");
	file.write_at(encode_utf8("payload"), 0);
	file.sync();
	file.data_sync();
	print("synced");
	drop(file);
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(stdout, "synced\n");
    assert_eq!(
        std::fs::read_to_string(dir.join("data/durable.txt")).unwrap(),
        "payload"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn an_open_handle_survives_removal_of_its_path() {
    // The TOCTOU-free read-then-act the tier exists for (filesystem.md §3.2):
    // the handle addresses the OPEN FILE, not the path, so once open, nothing
    // re-resolves — remove the path and the handle still stats and reads the
    // file it holds. (Unix-guarded: unlink-while-open is the POSIX promise.)
    let dir = temp_project("file_toctou");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/ten.txt", "0123456789");
    write(
        &dir,
        "probe.vl",
        r#"import std::io::print;
import std::bytes::{ Bytes, decode_utf8 };
import std::fs::{ File, remove, stat };
import std::drop::drop;

fun main() {
	let file = File::open("data/ten.txt");
	remove("data/ten.txt");
	print(stat("data/ten.txt").is_none());
	print(file.stat().size);
	let buffer = Bytes::alloc(10);
	print(file.read_at(buffer, 0));
	print(decode_utf8(buffer.slice(0, 10)));
	drop(file);
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(
        stdout, "true\n10\n10\n0123456789\n",
        "the path is gone (stat None) while the handle still measures and \
         reads the open file — no re-resolution between probe and act"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn with_file_returns_the_bodys_value_from_an_open_file() {
    // The documented idiom end-to-end: the body receives the open file as a
    // per-call parameter (R9's exemption), its value comes back out, and the
    // program continues past the awaited close. (That the close is AWAITED is
    // pinned on the emitted bytes — `with_file_awaits_the_close_before_
    // returning` in the `inference` suite and the `file.vl` corpus golden — since the
    // await IS the ordering.)
    let dir = temp_project("with_file");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/ten.txt", "0123456789");
    write(
        &dir,
        "probe.vl",
        r#"import std::io::print;
import std::bytes::{ Bytes, decode_utf8 };
import std::fs::with_file;

fun main() {
	let head = with_file("data/ten.txt", |file| {
		let buffer = Bytes::alloc(4);
		file.read_at(buffer, 0);
		decode_utf8(buffer.slice(0, 4))
	});
	print(head);
	print("after");
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(stdout, "0123\nafter\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(target_os = "linux")]
#[test]
fn a_dropped_file_closes_the_underlying_descriptor() {
    // The safety net proven at RUNTIME, not just in the emitted bytes: a
    // dropped handle's descriptor really is released. The drop only INITIATES
    // the close (Q1's ruling), so the probe polls /proc/self/fd — bounded —
    // until the count settles back to its baseline; and `with_file`'s count
    // is back to baseline IMMEDIATELY, its close having been awaited.
    // Plant-proven: with `impl File with Drop` removed from fs.vl, the
    // descriptor never closes and the poll times out red.
    //
    // The WARM-UP block before the baseline is load-bearing, and each line of
    // it was found by a real failure mode: the process's first
    // descriptor-based fs op can lazily create process-lifetime
    // infrastructure descriptors (io_uring on modern node/linux), and the
    // first PRINT on a PIPED stdout — which is what the test harness gives
    // the child — materializes the stream's own persistent handle. Either
    // one, first created after the baseline, holds the count one above it
    // forever, with the file itself long closed.
    let dir = temp_project("file_drop_closes");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/ten.txt", "0123456789");
    write(
        &dir,
        "probe.vl",
        r#"import std::io::print;
import std::fs::{ File, read_dir, with_file };
import std::drop::drop;
import std::range::Range;
import std::time::sleep;

fun fd_count(): i32 {
	read_dir("/proc/self/fd").len()
}

fun settled_count(baseline: i32): i32 {
	for _attempt in Range::new(0, 200) {
		if fd_count() == baseline {
			ret baseline;
		}
		sleep(10);
	}
	fd_count()
}

fun main() {
	let warm = File::open("data/ten.txt");
	drop(warm);
	print("warm");
	sleep(300);

	let baseline = fd_count();
	let file = File::open("data/ten.txt");
	print(fd_count() - baseline);
	drop(file);
	print(settled_count(baseline) - baseline);

	let with_baseline = fd_count();
	let size = with_file("data/ten.txt", |f| f.stat().size);
	print(size);
	print(fd_count() - with_baseline);
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(
        stdout, "warm\n1\n0\n10\n0\n",
        "open holds one descriptor; drop releases it (within the poll); \
         with_file's is already released when it returns"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
// The INSTRUMENT is `/proc/self/fd`, which only Linux has — the emission
// property this observes (a temporary's drop at its statement's end) is
// pinned platform-independently in tests/inference/resources.rs; this is
// its Linux-observable half, gated exactly as its descriptor-counting
// sibling above is. (The v0.39.0-cycle Windows leg caught the ungated
// first version — N26's lesson, third instance.)
#[cfg(target_os = "linux")]
fn a_temporary_handle_releases_its_descriptor_at_its_statements_end() {
    // C11, measured — `temporary-drop.md` P6/P7's fd staircase made permanent.
    // `File::open(p).read_at(b, 0)` was the fs tier's INTENDED idiom and leaked
    // its descriptor on every path until process exit: ten of them climbed the
    // count by ten and it never came back down (P6's "after 10 temporaries:
    // 31"). Under statement-end ownership the count is back at its baseline
    // before the next statement is observed, so a straight line of temporaries
    // holds ONE descriptor at a time and a loop of them holds one per
    // iteration.
    //
    // The counts are read IMMEDIATELY, with no settling poll, and that is the
    // whole instrument: P10 measured that Q1's fire-and-forget close is back
    // before the next statement runs, and a poll would let node's own
    // FileHandle finalizer close a LEAKED descriptor and report zero.
    // Plant-proven that way — with the temporary rule disabled the three
    // readings are 1, 2 and 7 rather than 0, 0 and 0.
    //
    // The warm-up ahead of the baseline is load-bearing for the same reasons
    // `a_dropped_file_closes_the_underlying_descriptor` documents: the first
    // descriptor-based fs op can lazily create process-lifetime infrastructure
    // (io_uring), and the first print on a piped stdout materializes that
    // stream's own handle. Both must exist before the baseline is taken.
    let dir = temp_project("file_temporary_closes");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/ten.txt", "0123456789");
    write(
        &dir,
        "probe.vl",
        r#"import std::io::print;
import std::bytes::Bytes;
import std::fs::{ File, read_dir };
import std::drop::drop;
import std::range::Range;
import std::time::sleep;

fun fd_count(): i32 {
	read_dir("/proc/self/fd").len()
}

fun main() {
	let warm = File::open("data/ten.txt");
	drop(warm);
	print("warm");
	sleep(300);

	let baseline = fd_count();
	let buffer = Bytes::alloc(4);

	// The straight-line staircase: two statements, each opening a temporary.
	// Under the leak this climbed to baseline + 1, then + 2, and stayed.
	print(File::open("data/ten.txt").read_at(buffer, 0));
	print(fd_count() - baseline);
	print(File::open("data/ten.txt").stat().size);
	print(fd_count() - baseline);

	// The loop: one per iteration, never N.
	for _round in Range::new(0, 5) {
		print(File::open("data/ten.txt").stat().size);
	}
	print(fd_count() - baseline);
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(
        stdout, "warm\n4\n0\n10\n0\n10\n10\n10\n10\n10\n0\n",
        "every temporary handle is released at its own statement's end, so the \
         descriptor count is back at its baseline before the next statement — \
         straight-line and in a loop alike"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// --- S5: the incremental reader and the writing scoped forms -------------
// (kolt.local 031 S5, filesystem.md §3.4's build note and §5.3)
//
// Against real files, because a cursor's whole claim is that what comes out
// of the chunks equals what is on disk.

#[test]
fn a_reader_reads_a_file_through_in_chunks_that_do_not_divide_it() {
    // The chunk loop, over a size that divides neither the file nor any
    // prefix of it (7 into 30), so every boundary falls mid-content: the
    // concatenation must equal the file byte for byte, and the cursor must
    // land exactly on the length. Plant-proven by advancing the cursor one
    // byte too far (`+ count + 1`), which reddens this and its two siblings
    // below with dropped bytes and a cursor past the end. The plant is an
    // OVERSHOOT rather than the obvious understep because an understepping
    // cursor never reaches the empty read and hangs the loop instead of
    // failing it — worth recording, since a hung probe is what a broken
    // cursor actually looks like.
    let dir = temp_project("reader_chunks");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/thirty.txt", "0123456789abcdefghijABCDEFGHIJ");
    write(
        &dir,
        "probe.vl",
        r#"import std::bytes::decode_utf8;
import std::fs::{ File, Reader };
import std::drop::drop;

fun main() {
	let reader = Reader::of(File::open("data/thirty.txt"));
	mut whole = "";
	mut chunks = 0;
	for {
		let chunk = reader.next(7);
		if chunk.len() == 0 {
			jump break;
		}
		whole += decode_utf8(chunk);
		chunks += 1;
	}
	print(whole);
	print(chunks);
	print(reader.position());
	drop(reader);
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(
        stdout, "0123456789abcdefghijABCDEFGHIJ\n5\n30\n",
        "the chunks must reassemble the file exactly — four full 7-byte reads, \
         a short 2-byte one, then the empty read that ends the loop"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_reader_handles_a_file_shorter_than_one_chunk_and_an_empty_one() {
    // The two degenerate lengths, which are where an off-by-one in the slice
    // or in the EOF test would show: a file smaller than the request answers
    // once and then empty (the short read is NOT the end signal — the empty
    // one after it is), and an empty file answers empty on the very first
    // call with the cursor still at zero.
    let dir = temp_project("reader_small");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/tiny.txt", "abc");
    write(&dir, "data/empty.txt", "");
    write(
        &dir,
        "probe.vl",
        r#"import std::bytes::decode_utf8;
import std::fs::{ File, Reader };
import std::drop::drop;

fun main() {
	let small = Reader::of(File::open("data/tiny.txt"));
	let first = small.next(64);
	print(first.len());
	print(decode_utf8(first));
	print(small.next(64).len());
	print(small.position());
	drop(small);

	let empty = Reader::of(File::open("data/empty.txt"));
	print(empty.next(64).len());
	print(empty.position());
	drop(empty);
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(
        stdout, "3\nabc\n0\n3\n0\n0\n",
        "a short chunk is the content, not the end; the empty one after it is \
         the end; and an empty file is empty on the first call"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_positional_read_through_a_readers_file_leaves_the_cursor_alone() {
    // §3.4's whole point, demonstrated rather than argued: the cursor is a
    // wrapper over a positional primitive, so reading through the reader's
    // own `file` loan does not move it — cursored and positional reads
    // interleave soundly on one open file, which is exactly what the host's
    // hidden position would have destroyed. `seek` then proves the cursor is
    // writable as well as readable. Plant-proven by the same overshooting
    // cursor its sibling above describes: every reading of `position()` here
    // goes wrong under it.
    let dir = temp_project("reader_interleave");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/thirty.txt", "0123456789abcdefghijABCDEFGHIJ");
    write(
        &dir,
        "probe.vl",
        r#"import std::bytes::{ Bytes, decode_utf8 };
import std::fs::{ File, Reader };
import std::drop::drop;

fun main() {
	let reader = Reader::of(File::open("data/thirty.txt"));
	print(decode_utf8(reader.next(4)));
	print(reader.position());

	let buffer = Bytes::alloc(6);
	print(reader.file.read_at(buffer, 20));
	print(decode_utf8(buffer.slice(0, 6)));
	print(reader.position());

	print(decode_utf8(reader.next(4)));
	reader.seek(10i53);
	print(decode_utf8(reader.next(4)));
	reader.seek(999i53);
	print(reader.next(4).len());
	drop(reader);
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(
        stdout, "0123\n4\n6\nABCDEF\n4\n4567\nabcd\n0\n",
        "a positional read through the reader's file must not move the cursor \
         (still 4 after reading at 20), the cursor must resume where it was, \
         a seek must move it, and a seek past the end must read empty"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_writing_scoped_forms_each_open_in_their_own_mode() {
    // §5.3's family, one mode at a time and each proved by what it does to a
    // file that already exists — which is what §3.3 says a constructor's name
    // is for. `create` truncates, `append` lands at the end whatever position
    // it names, `modify` edits in place without truncating. (`create_new`,
    // whose whole content is a refusal, is the sibling below.) Plant-proven:
    // opening `with_file_append` over `File::create` instead of
    // `File::append_to` reddens the second and third readings at once.
    let dir = temp_project("with_file_writing");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/existing.txt", "0123456789");
    write(
        &dir,
        "probe.vl",
        r#"import std::bytes::encode_utf8;
import std::fs::{ read_file_to_str, with_file, with_file_append, with_file_create, with_file_modify };

fun main() {
	// create: truncates what was there, so ten bytes become four.
	with_file_create("data/existing.txt", |file| {
		file.write_at(encode_utf8("abcd"), 0i53);
	});
	print(read_file_to_str("data/existing.txt"));

	// append: position ignored, every write lands at the end.
	with_file_append("data/existing.txt", |file| {
		file.write_at(encode_utf8("EF"), 0i53);
	});
	print(read_file_to_str("data/existing.txt"));

	// modify: in place, no truncation — one byte overwritten at offset 1.
	with_file_modify("data/existing.txt", |file| {
		file.write_at(encode_utf8("Z"), 1i53);
	});
	print(read_file_to_str("data/existing.txt"));

	// the body's value comes back out of a writing form too.
	print(with_file_create("data/fresh.txt", |file| file.write_at(encode_utf8("hello"), 0i53)));
	print(with_file("data/fresh.txt", |file| file.stat().size));
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(
        stdout, "abcd\nabcdEF\naZcdEF\n5\n5\n",
        "create truncates, append ignores the position, modify edits in place, \
         and a writing form returns its body's value"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn with_file_create_new_refuses_an_occupied_path_and_leaves_it_alone() {
    // The exclusive claim in its scoped form: the failure is the OPEN, so the
    // body never runs and the incumbent's bytes are untouched.
    let dir = temp_project("with_file_create_new");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/claimed.txt", "mine");
    write(
        &dir,
        "probe.vl",
        r#"import std::bytes::encode_utf8;
import std::fs::with_file_create_new;

fun main() {
	with_file_create_new("data/claimed.txt", |file| {
		print("the body must not run");
		file.write_at(encode_utf8("theirs"), 0i53);
	});
}
main();
"#,
    );
    let stderr = run_err(&dir, "probe.vl");
    assert!(
        stderr.contains("EEXIST"),
        "an occupied path must fail the exclusive scoped create; stderr was:\n{stderr}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("data/claimed.txt")).unwrap(),
        "mine",
        "the refused claim must not have touched the incumbent's bytes"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_throwing_body_fails_the_scoped_call_and_the_write_before_it_still_landed() {
    // The safety net under §5.3(d)'s emitted shape — `try { body; await
    // close; return } finally { drop }`: a body that throws propagates out of
    // the call (nothing after it runs), and the handle is still closed by the
    // scope-end drop, so what the body wrote before throwing is on disk and
    // readable afterwards. The `finally` is compiler-generated (the drop
    // planner's, not this module's), so there is nothing in `fs.vl` to plant
    // here; what this pin adds over the emission pins is that the throw
    // travels through the generic scoped form at all rather than being
    // swallowed by it.
    let dir = temp_project("with_file_throwing_body");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    std::fs::create_dir_all(dir.join("data")).unwrap();
    write(
        &dir,
        "probe.vl",
        r#"import std::bytes::encode_utf8;
import std::fs::{ File, with_file_create };

fun main() {
	with_file_create("data/half.txt", |file| {
		file.write_at(encode_utf8("written"), 0i53);
		File::open("nothing/here.txt").stat().size
	});
	print("must not be reached");
}
main();
"#,
    );
    let stderr = run_err(&dir, "probe.vl");
    assert!(
        stderr.contains("ENOENT"),
        "a throwing body must fail the scoped call; stderr was:\n{stderr}"
    );
    assert!(
        !stderr.contains("must not be reached"),
        "nothing after the failed call may run; stderr was:\n{stderr}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("data/half.txt")).unwrap(),
        "written",
        "what the body wrote before throwing must still be on disk — the \
         handle was closed by the scope-end drop behind the awaited path"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(target_os = "linux")]
#[test]
fn a_writing_scoped_form_returns_with_its_descriptor_already_closed() {
    // The writing forms release their handle at RUNTIME, and a `Reader`
    // releases the `File` it owns through containment teardown —
    // `drop(reader)` reaching through the plain struct into the field. The
    // drop only INITIATES that close (Q1's (a)), so the reader's count is
    // read through a settling poll while the scoped forms' are read
    // immediately.
    //
    // What this does NOT prove is the ORDERING, and the distinction is worth
    // stating because the counts look like they do: measured with
    // `close_awaited` removed from `scoped_file`, these readings are still
    // zero — `fd_count` itself awaits, which is all the slack a
    // fire-and-forget close needs. The await IS the ordering, so the ordering
    // is pinned on the emitted bytes instead
    // (`every_writing_scoped_form_awaits_its_close` in the `inference` suite,
    // which that same plant reddens). This pin holds the weaker, still-real
    // line: the descriptors come back and the bytes land.
    //
    // The warm-up before the baseline is load-bearing for the reasons
    // `a_dropped_file_closes_the_underlying_descriptor` documents: the first
    // descriptor-based fs op can lazily create process-lifetime
    // infrastructure (io_uring), and the first print on a piped stdout
    // materializes that stream's own handle.
    let dir = temp_project("with_file_writing_fds");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/ten.txt", "0123456789");
    write(
        &dir,
        "probe.vl",
        r#"import std::bytes::encode_utf8;
import std::fs::{ File, Reader, read_dir, with_file_append, with_file_create };
import std::drop::drop;
import std::range::Range;
import std::time::sleep;

fun fd_count(): i32 {
	read_dir("/proc/self/fd").len()
}

fun settled_count(baseline: i32): i32 {
	for _attempt in Range::new(0, 200) {
		if fd_count() == baseline {
			ret baseline;
		}
		sleep(10);
	}
	fd_count()
}

fun main() {
	let warm = File::open("data/ten.txt");
	drop(warm);
	print("warm");
	sleep(300);

	let baseline = fd_count();
	with_file_create("data/written.txt", |file| { file.write_at(encode_utf8("payload"), 0i53); });
	print(fd_count() - baseline);
	with_file_append("data/written.txt", |file| { file.write_at(encode_utf8("!"), 0i53); });
	print(fd_count() - baseline);

	let reader = Reader::of(File::open("data/ten.txt"));
	print(fd_count() - baseline);
	print(reader.next(4).len());
	drop(reader);
	print(settled_count(baseline) - baseline);
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    assert_eq!(
        stdout, "warm\n0\n0\n1\n4\n0\n",
        "a writing scoped form's descriptor is already released when it \
         returns, a Reader holds exactly one while it is alive, and dropping \
         the Reader releases it"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("data/written.txt")).unwrap(),
        "payload!",
        "both writing forms must have landed their bytes"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// --- the watch tier (kolt.local 020) -------------------------------------
//
// Against real file activity, because that is the only thing that proves a
// poller: these programs create, modify and remove files under a live watcher
// and read back what it reports. The mechanism is stat-diffing at a 300 ms
// interval (the compiler's own `--watch` number), so each pull costs up to one
// interval — which is why every probe here runs under a deadline rather than
// `run_ok`'s unbounded wait.

#[test]
fn a_watch_reports_creation_modification_and_removal() {
    // The three kinds, each observed for real and each distinguished — which
    // is the whole argument for polling over `node:fs`'s `watch`, where all
    // three arrive as one ambiguous `"rename"`/`"change"` pair. The modify
    // changes the file's LENGTH as well as its mtime deliberately: the poller
    // compares both, and mtime granularity is a filesystem property (1 second
    // on some), so a same-length rewrite inside one tick is not something this
    // gate should depend on.
    let dir = temp_project("watch_kinds");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/keep.txt", "keep"); // baseline content, so `data` exists
    write(
        &dir,
        "probe.vl",
        &watch_probe(
            r#"	let watcher = Watcher::watch("data");
	write_file("data/a.txt", "one");
	show(watcher);
	write_file("data/a.txt", "one-two-three");
	show(watcher);
	remove("data/a.txt");
	show(watcher);
"#,
        ),
    );
    let stdout = run_ok_within(&dir, "probe.vl", WATCH_TIMEOUT);
    assert_eq!(
        stdout, "created data/a.txt\nmodified data/a.txt\nremoved data/a.txt\n",
        "a poller distinguishes all three, on every platform"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_change_carries_an_addressable_path() {
    // `Change.path` is the watched root joined with the entry's place under
    // it — the string to hand straight to `read_file_to_str`, with no joining
    // at the call site (unlike `read_dir`'s bare names and `read_dir_all`'s
    // relative paths, whose callers often want the name itself).
    let dir = temp_project("watch_path");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/sub/keep.txt", "keep");
    write(
        &dir,
        "probe.vl",
        &watch_probe(
            r#"	let watcher = Watcher::watch_all("data");
	write_file("data/sub/note.txt", "hello");
	let change = watcher.next();
	print(describe(change));
	print(fs::read_file_to_str(change.path));
"#,
        ),
    );
    let stdout = run_ok_within(&dir, "probe.vl", WATCH_TIMEOUT);
    assert_eq!(stdout, "created data/sub/note.txt\nhello\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_recursive_watch_reaches_a_nested_file_and_a_flat_one_does_not() {
    // `watch` / `watch_all` are `read_dir` / `read_dir_all`'s reach, and the
    // `_all` suffix means the same thing here it means there. The flat leg is
    // the load-bearing half: it sleeps three intervals over a nested write and
    // reports nothing, then sees the very next top-level one.
    let dir = temp_project("watch_depth");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "tree/sub/keep.txt", "keep");
    write(
        &dir,
        "probe.vl",
        &watch_probe(
            r#"	let deep = Watcher::watch_all("tree");
	write_file("tree/sub/c.txt", "c");
	show(deep);
	drop(deep);

	let flat = Watcher::watch("tree");
	write_file("tree/sub/d.txt", "d");
	sleep(900);
	write_file("tree/top.txt", "top");
	show(flat);
"#,
        ),
    );
    let stdout = run_ok_within(&dir, "probe.vl", WATCH_TIMEOUT);
    assert_eq!(
        stdout, "created tree/sub/c.txt\ncreated tree/top.txt\n",
        "a flat watch never descends — the nested write produced no change at all"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn two_watchers_are_independent_and_dropping_one_leaves_the_other_live() {
    // Each watcher owns its own baseline, queue and timer: `a` reports only
    // `one`'s activity, and killing it does not disturb `b`, which goes on
    // reporting `two`'s.
    let dir = temp_project("watch_two");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "one/keep.txt", "keep");
    write(&dir, "two/keep.txt", "keep");
    write(
        &dir,
        "probe.vl",
        &watch_probe(
            r#"	let a = Watcher::watch("one");
	let b = Watcher::watch("two");
	write_file("one/x.txt", "x");
	show(a);
	drop(a);
	write_file("two/y.txt", "y");
	show(b);
"#,
        ),
    );
    let stdout = run_ok_within(&dir, "probe.vl", WATCH_TIMEOUT);
    assert_eq!(stdout, "created one/x.txt\ncreated two/y.txt\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_dropped_watcher_stops_polling_and_lets_the_process_end() {
    // The destructor's whole job, and the reason `Watcher` is a resource at
    // all: the poll runs on a host timer, a pending host timer keeps node
    // alive, so a watcher that never stopped is a program that never exits.
    // Plant-proven — with `impl Watcher with Drop` removed from fs.vl this
    // exact probe runs to its last line and then hangs until the deadline
    // kills it (measured: 12 s of nothing, against 1 s whole with the
    // destructor in place).
    let dir = temp_project("watch_drop");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/keep.txt", "keep");
    write(
        &dir,
        "probe.vl",
        &watch_probe(
            r#"	let watcher = Watcher::watch("data");
	write_file("data/a.txt", "one");
	show(watcher);
	drop(watcher);
	print("dropped");
"#,
        ),
    );
    let started = Instant::now();
    let stdout = run_ok_within(&dir, "probe.vl", WATCH_TIMEOUT);
    assert_eq!(stdout, "created data/a.txt\ndropped\n");
    assert!(
        started.elapsed() < WATCH_TIMEOUT,
        "the process must end on its own, not be killed at the deadline"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_watch_on_a_single_file_that_is_not_there_yet_reports_its_whole_life() {
    // Two things at once, and they are the same thing: the watched path need
    // not exist (a poller has nothing to attach to, so absence is just an
    // empty baseline — `fs.watch` throws ENOENT here), and watching a FILE
    // reports that file's own path back rather than something relative to it.
    let dir = temp_project("watch_absent");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/keep.txt", "keep");
    write(
        &dir,
        "probe.vl",
        &watch_probe(
            r#"	let watcher = Watcher::watch("data/later.txt");
	write_file("data/later.txt", "hello");
	show(watcher);
	write_file("data/later.txt", "hello again");
	show(watcher);
	remove("data/later.txt");
	show(watcher);
"#,
        ),
    );
    let stdout = run_ok_within(&dir, "probe.vl", WATCH_TIMEOUT);
    assert_eq!(
        stdout,
        "created data/later.txt\nmodified data/later.txt\nremoved data/later.txt\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_batch_of_simultaneous_changes_arrives_complete_one_pull_at_a_time() {
    // Three files written between two observations are ONE batch, and the
    // whole batch survives: `next` hands them over one pull at a time, none
    // coalesced away and none invented. (Coalescing is precisely what
    // `node:fs`'s `watch` does to a burst.)
    //
    // The ORDER is deliberately not asserted here, and the reason is worth
    // recording rather than hiding: the helper sorts each batch by path so the
    // order does not vary by host, but this host's `readdir` already returns
    // entries sorted, so no ordering assertion written here can tell a sorting
    // poller from a non-sorting one — planted and confirmed. The sort is
    // pinned on the emitted helper instead (`a_batch_of_changes_is_handed_over
    // _path_sorted`, inference suite), and a batch a poll tick happens to
    // split would reorder a strict assertion for a reason that is not a bug.
    let dir = temp_project("watch_batch");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/sub/keep.txt", "keep");
    write(
        &dir,
        "probe.vl",
        &watch_probe(
            r#"	let watcher = Watcher::watch_all("data");
	write_file("data/b.txt", "b");
	write_file("data/a.txt", "a");
	write_file("data/sub/c.txt", "c");
	show(watcher);
	show(watcher);
	show(watcher);
"#,
        ),
    );
    let stdout = run_ok_within(&dir, "probe.vl", WATCH_TIMEOUT);
    let mut lines: Vec<&str> = stdout.lines().collect();
    lines.sort_unstable();
    assert_eq!(
        lines,
        vec![
            "created data/a.txt",
            "created data/b.txt",
            "created data/sub/c.txt"
        ],
        "every change in the burst is reported exactly once"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_directorys_own_mtime_bump_is_not_reported_as_a_change() {
    // Creating `data/sub/c.txt` bumps the mtime of BOTH `data/sub` and `data`
    // in the same tick. Unsuppressed, the path-sorted batch would open with
    // `modified data` — a restatement of an entry change the watcher is
    // already reporting individually, and one that does not fire uniformly
    // across filesystems. Directories report Created and Removed only, so the
    // first pull is the file itself and the second is the NEXT real change.
    let dir = temp_project("watch_dir_mtime");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/sub/keep.txt", "keep");
    write(
        &dir,
        "probe.vl",
        &watch_probe(
            r#"	let watcher = Watcher::watch_all("data");
	write_file("data/sub/c.txt", "c");
	show(watcher);
	write_file("data/top.txt", "top");
	show(watcher);
"#,
        ),
    );
    let stdout = run_ok_within(&dir, "probe.vl", WATCH_TIMEOUT);
    assert_eq!(stdout, "created data/sub/c.txt\ncreated data/top.txt\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_new_directory_is_reported_created_and_its_removal_reported_removed() {
    // The events a directory DOES carry, since it does not carry `Modified`:
    // appearing and disappearing are facts nothing else in the stream states.
    // A removed directory takes its contents with it — each entry is reported
    // in the same batch, path-sorted, so the parent comes first.
    let dir = temp_project("watch_dir_life");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "data/keep.txt", "keep");
    write(
        &dir,
        "probe.vl",
        &watch_probe(
            r#"	let watcher = Watcher::watch_all("data");
	fs::create_dir("data/made");
	show(watcher);
	write_file("data/made/inside.txt", "inside");
	show(watcher);
	fs::remove_dir_all("data/made");
	show(watcher);
	show(watcher);
"#,
        ),
    );
    let stdout = run_ok_within(&dir, "probe.vl", WATCH_TIMEOUT);
    assert_eq!(
        stdout,
        "created data/made\ncreated data/made/inside.txt\nremoved data/made\nremoved data/made/inside.txt\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
