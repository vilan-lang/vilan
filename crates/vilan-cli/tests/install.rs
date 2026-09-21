//! The installed-binary smoke test (proposal/releases.md §8, slice 1): a
//! `vilan` copied out of the repo — no checkout in any ancestor, no
//! `$VILAN_STD`, a fresh `$HOME` — must still compile and run a program, by
//! materializing its embedded std into `~/.vilan/std-cache/<hash>/`. This is
//! the exact shape of every binary the install script delivers.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A scratch "user machine": an empty home and a working directory, both under
/// the system temp dir (outside any checkout), holding a copy of the binary.
struct Machine {
    root: PathBuf,
    home: PathBuf,
    binary: PathBuf,
}

impl Machine {
    /// A scratch machine root.
    ///
    /// `std::env::temp_dir()` and NOT the support scratch root, for the reason
    /// this suite exists (N102): `CARGO_TARGET_TMPDIR` is
    /// `<worktree>/target/tmp`, which is INSIDE a vilan checkout, and the
    /// premise here is a machine with no checkout at all. Under a scratch root
    /// the ancestor walk finds the worktree's own `vilan/std` and the embedded
    /// toolchain is never materialized, so the cache the test asserts about is
    /// never written. Recorded in `harness_scratch.rs`'s
    /// `PATHS_THE_BINARY_OWNS`: not waiting to be ported, unable to be.
    fn new(name: &str) -> Machine {
        let root =
            std::env::temp_dir().join(format!("vilan-install-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let home = root.join("home");
        fs::create_dir_all(&home).expect("create scratch home");
        // The destination keeps the SOURCE's file name, so the copy is
        // `vilan.exe` on Windows without a second spelling of the name to keep
        // in sync (windows-support.md §4). Every later use goes through
        // `self.binary`, so there is nothing else to derive.
        let source = Path::new(env!("CARGO_BIN_EXE_vilan"));
        let binary = root.join(
            source
                .file_name()
                .expect("the built binary has a file name"),
        );
        fs::copy(source, &binary).expect("copy the binary out of the repo");
        Machine { root, home, binary }
    }

    fn vilan(&self, arguments: &[&str]) -> Output {
        // Retry ETXTBSY: a concurrent test's fork can briefly hold this
        // binary's just-written fd until its own exec closes it (CLOEXEC), and
        // an exec landing in that window fails spuriously. Cargo carries the
        // same retry for the same race.
        const ETXTBSY: i32 = 26;
        let mut attempts = 0;
        loop {
            let result = Command::new(&self.binary)
                .args(arguments)
                .current_dir(&self.root)
                .env_remove("VILAN_STD")
                // F19's twin of `VILAN_STD`: an installed machine names neither,
                // and a lane's shell may well have it set.
                .env_remove("VILAN_RT")
                .env("HOME", &self.home)
                .env_remove("USERPROFILE")
                .output();
            match result {
                Err(error) if error.raw_os_error() == Some(ETXTBSY) && attempts < 100 => {
                    attempts += 1;
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                other => return other.expect("run the copied binary"),
            }
        }
    }
}

impl Drop for Machine {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn an_installed_binary_compiles_and_runs_without_a_checkout() {
    let machine = Machine::new("run");
    fs::write(
        machine.root.join("hello.vl"),
        "import std::io::print;\n\nfun main() {\n    print(\"hello from an installed vilan\");\n}\n",
    )
    .expect("write hello.vl");

    let first = machine.vilan(&["run", "hello.vl"]);
    assert!(
        first.status.success(),
        "first run failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(
        String::from_utf8_lossy(&first.stdout).contains("hello from an installed vilan"),
        "program output missing: {}",
        String::from_utf8_lossy(&first.stdout)
    );

    // The embedded toolchain landed in the scratch home, keyed by content.
    let cache = machine.home.join(".vilan").join("std-cache");
    let entries: Vec<_> = fs::read_dir(&cache)
        .expect("the std cache directory exists")
        .filter_map(Result::ok)
        .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        .collect();
    assert_eq!(entries.len(), 1, "one content-keyed cache entry");
    let toolchain = entries[0].path();
    assert!(toolchain.join("std/vilan.toml").is_file());
    assert!(toolchain.join("macro_std/vilan.toml").is_file());

    // Second run: the cache is reused, not rebuilt (the tree is immutable).
    let manifest_modified = modified(&toolchain.join("std/vilan.toml"));
    let second = machine.vilan(&["run", "hello.vl"]);
    assert!(second.status.success(), "second run failed");
    assert_eq!(
        modified(&toolchain.join("std/vilan.toml")),
        manifest_modified,
        "a warm cache must not be rewritten"
    );
}

/// F19: `--backend rust` from an INSTALLED layout — no checkout in any
/// ancestor, no `$VILAN_RT` — materializes the embedded runtime crate under
/// `~/.vilan/rt-cache/<hash>/vilan-rt/` and builds against it.
///
/// This is the gap the slice closed: S1a resolved the runtime through
/// `$VILAN_RT` or a compile-time source sibling, so a released binary had
/// neither and the second backend was a from-source tool. The pin is
/// non-vacuous by construction — remove the materialization fallback and the
/// build fails with "needs the `vilan-rt` crate".
///
/// It pays a real `cargo build` of the runtime plus the program, so it is one
/// test over one small program; the corpus-wide claim is
/// `native_differential`'s, which points `VILAN_RT` at the tree on purpose.
#[test]
fn an_installed_binary_builds_natively_against_its_embedded_runtime() {
    if which_cargo().is_none() {
        eprintln!("skipped: the native backend needs `cargo` on PATH");
        return;
    }
    let machine = Machine::new("native");
    fs::write(
        machine.root.join("native.vl"),
        "import std::io::print;\n\nfun main() {\n    print(6 * 7);\n}\n",
    )
    .expect("write native.vl");

    let run = machine.vilan(&["run", "--backend", "rust", "native.vl"]);
    assert!(
        run.status.success(),
        "the installed binary could not build natively:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "42\n",
        "the native binary's output"
    );

    // The runtime landed in the scratch home under its own content-keyed root,
    // beside the std cache and not inside it.
    let cache = machine.home.join(".vilan").join("rt-cache");
    let entries: Vec<_> = fs::read_dir(&cache)
        .expect("the runtime cache directory exists")
        .filter_map(Result::ok)
        .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        .collect();
    assert_eq!(entries.len(), 1, "one content-keyed runtime entry");
    let materialized = entries[0].path().join("vilan-rt");
    assert!(materialized.join("Cargo.toml").is_file());
    assert!(materialized.join("src/lib.rs").is_file());
    // The materialized manifest is standalone: no `[lints] workspace = true`
    // (which resolves only inside the vilan workspace) and a `[workspace]` of
    // its own, so cargo stops walking up out of the cache.
    let manifest = fs::read_to_string(materialized.join("Cargo.toml")).expect("read the manifest");
    assert!(
        !manifest.contains("workspace = true"),
        "the materialized manifest inherits workspace lints:\n{manifest}"
    );
    assert!(manifest.contains("[workspace]"), "{manifest}");

    // B346's one-root rule: a named root that is not one is REFUSED, never
    // silently replaced by the embedded copy.
    let misnamed = machine.root.join("not-the-runtime");
    fs::create_dir_all(&misnamed).expect("create the misnamed root");
    let refused = Command::new(&machine.binary)
        .args(["build", "--backend", "rust", "native.vl"])
        .current_dir(&machine.root)
        .env_remove("VILAN_STD")
        .env("HOME", &machine.home)
        .env_remove("USERPROFILE")
        .env("VILAN_RT", &misnamed)
        .output()
        .expect("run the copied binary");
    assert!(!refused.status.success(), "a misnamed VILAN_RT must refuse");
    let message = String::from_utf8_lossy(&refused.stderr);
    assert!(
        message.contains("`VILAN_RT` is set to") && message.contains("no `Cargo.toml`"),
        "the refusal must name the variable and what is wrong with it:\n{message}"
    );
}

/// Whether `cargo` is on PATH — the native backend shells out to it, and a
/// machine without it skips rather than fails.
fn which_cargo() -> Option<()> {
    Command::new("cargo")
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|_| ())
}

#[test]
fn version_reports_the_toolchain_and_its_build() {
    let machine = Machine::new("version");
    let output = machine.vilan(&["--version"]);
    assert!(output.status.success());
    let version = String::from_utf8_lossy(&output.stdout);
    // `vilan <semver> (<sha or unknown>)` — the sha keeps alpha bug reports precise.
    assert!(
        version.starts_with("vilan ")
            && version.contains(" (")
            && version.trim_end().ends_with(')'),
        "unexpected --version shape: {version}"
    );
}

fn modified(path: &Path) -> std::time::SystemTime {
    fs::metadata(path)
        .expect("metadata")
        .modified()
        .expect("mtime")
}
