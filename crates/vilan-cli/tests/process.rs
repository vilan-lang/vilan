//! End-to-end: `std::process`'s surface against the real host process
//! (kolt.local 018) — these run the actual `process.*` bindings rather than
//! pinning generated JS text, the same posture as `fs.rs`.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A fresh temp directory for the test's project tree.
fn temp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vilan_process_{tag}_{}", std::process::id()));
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
fn cwd_reports_a_non_empty_absolute_path_to_the_directory_the_process_runs_in() {
    let dir = temp_project("cwd");
    write(
        &dir,
        "probe.vl",
        r#"import std::print;
import std::process::cwd;

fun main() {
	print(cwd());
}
main();
"#,
    );
    let stdout = run_ok(&dir, "probe.vl");
    let reported = stdout.strip_suffix('\n').unwrap_or(&stdout);
    assert!(!reported.is_empty(), "cwd() printed an empty path");
    assert!(
        Path::new(reported).is_absolute(),
        "cwd() should be an absolute path, was {reported:?}"
    );
    // The probe ran with the fixture directory as its working directory, and
    // the last path component survives any symlink canonicalization the host
    // applies to the prefix (`/tmp` vs `/private/tmp`).
    let basename = dir.file_name().unwrap().to_str().unwrap();
    assert!(
        reported.ends_with(basename),
        "cwd() should end with the fixture directory `{basename}`, was {reported:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
