//! End-to-end: `std::process`'s surface against the real host process
//! (kolt.local 018) — these run the actual `process.*` bindings rather than
//! pinning generated JS text, the same posture as `fs.rs`. Each pin runs on
//! BOTH backends: the JS one under node, and the native one (`--backend rust`,
//! F?4) as the binary `vilan run` builds.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod support;

/// A fresh temp directory for the test's project tree.
fn temp_project(tag: &str) -> PathBuf {
    let dir = support::scratch_root().join(format!("vilan_process_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Writes `contents` to `dir/relative`, creating parent directories.
fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// The two backends a pin runs on: `None` is the default (JS under node).
const BACKENDS: [Option<&str>; 2] = [None, Some("rust")];

/// `vilan run [--backend <backend>] <relative>` inside `dir`, not yet started.
/// The native leg builds against this tree's `vilan-rt` and std into one cargo
/// target directory shared by this binary's tests (cargo locks it itself), so
/// the runtime compiles once.
fn vilan_run(dir: &Path, relative: &str, backend: Option<&str>) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_vilan"));
    command.arg("run");
    if let Some(backend) = backend {
        command.args(["--backend", backend]);
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    command
        .arg(relative)
        .current_dir(dir)
        .env("VILAN_STD", manifest.join("../../vilan/std"))
        .env("VILAN_RT", manifest.join("../vilan-rt"))
        .env(
            "CARGO_TARGET_DIR",
            support::scratch_root().join("process-native"),
        );
    command
}

/// Runs `vilan run <relative>` on `backend` inside `dir`, asserting success,
/// and returns stdout.
fn run_ok_on(dir: &Path, relative: &str, backend: Option<&str>) -> String {
    let output = vilan_run(dir, relative, backend)
        .stdin(Stdio::null())
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
    for backend in BACKENDS {
        cwd_reports_the_directory_on(backend);
    }
}

/// F?4: natively `cwd` was refused with F1's S1b message; it is
/// `std::env::current_dir` now, and answers what node answers.
fn cwd_reports_the_directory_on(backend: Option<&str>) {
    let dir = temp_project(&format!("cwd_{}", backend.unwrap_or("js")));
    write(
        &dir,
        "probe.vl",
        r#"import std::io::print;
import std::process::cwd;

fun main() {
	print(cwd());
}
main();
"#,
    );
    let stdout = run_ok_on(&dir, "probe.vl", backend);
    let reported = stdout.strip_suffix('\n').unwrap_or(&stdout);
    assert!(
        !reported.is_empty(),
        "{backend:?}: cwd() printed an empty path"
    );
    assert!(
        Path::new(reported).is_absolute(),
        "{backend:?}: cwd() should be an absolute path, was {reported:?}"
    );
    // The probe ran with the fixture directory as its working directory, and
    // the last path component survives any symlink canonicalization the host
    // applies to the prefix (`/tmp` vs `/private/tmp`).
    let basename = dir.file_name().unwrap().to_str().unwrap();
    assert!(
        reported.ends_with(basename),
        "{backend:?}: cwd() should end with the fixture directory `{basename}`, was {reported:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// F?4: `args()` natively — `process.argv.slice(2)`'s twin, the arguments
/// after the program, in order, with a hyphenated one passed through.
#[test]
fn f4_args_are_the_invocation_tail_on_both_backends() {
    for backend in BACKENDS {
        let dir = temp_project(&format!("args_{}", backend.unwrap_or("js")));
        write(
            &dir,
            "args.vl",
            r#"import std::io::print;
import std::process::args;

fun main() {
	let tail = args();
	print(tail.len());
	for argument in tail {
		print(i"[{argument}]");
	}
}
"#,
        );
        let output = vilan_run(&dir, "args.vl", backend)
            .args(["first", "--second", "with space"])
            .stdin(Stdio::null())
            .output()
            .expect("run vilan");
        assert!(
            output.status.success(),
            "{backend:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "3\n[first]\n[--second]\n[with space]\n",
            "{backend:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// F?4: `exit(code)` natively — the process ends at the call with `code`,
/// what was printed before it is out (flushed), and nothing after it runs.
#[test]
fn f4_exit_ends_the_process_with_its_code_on_both_backends() {
    for backend in BACKENDS {
        let dir = temp_project(&format!("exit_{}", backend.unwrap_or("js")));
        write(
            &dir,
            "exit.vl",
            r#"import std::io::print;
import std::process::exit;

fun main() {
	print("before");
	exit(4);
	print("after");
}
"#,
        );
        let output = vilan_run(&dir, "exit.vl", backend)
            .stdin(Stdio::null())
            .output()
            .expect("run vilan");
        assert_eq!(
            output.status.code(),
            Some(4),
            "{backend:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "before\n",
            "{backend:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
