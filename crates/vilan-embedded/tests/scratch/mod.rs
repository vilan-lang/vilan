//! The test harness's scratch root, and the guard that says so when the disk
//! is full (tracker N82's three helpers, N86's sweep).
//!
//! Not a test target itself (cargo compiles only the top-level `tests/*.rs`);
//! each binary that needs it declares `mod scratch;`, and a shared harness
//! module reaches it as `crate::scratch` — which is why the binaries including
//! one declare this too.
//!
//! **Why not `std::env::temp_dir()`.** On the owner's machine `/tmp` is a
//! 12 GiB tmpfs shared by every worktree, and a full suite under nine lanes
//! filled it: three `No space left on device` failures in one run, green on
//! the re-run, which is a red indistinguishable from a real one and cost two
//! triages before it was filed. `CARGO_TARGET_TMPDIR` is cargo's own scratch
//! for integration tests, under `target/`, so the files land on the filesystem
//! the build artifacts already use and each worktree writes into its own tree
//! rather than into one shared pool.
//!
//! Every helper only some binaries use is dead code in the rest — hence the
//! module-wide allow, as `vilan-cli`'s `support` module carries for the same
//! reason.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// The directory this test binary's scratch goes in. Created if absent.
///
/// `CARGO_TARGET_TMPDIR` is expanded where this module is COMPILED, which is
/// once per including binary, so two binaries never share a root even when
/// they write the same file name.
pub fn root() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let _ = std::fs::create_dir_all(&root);
    root
}

/// A scratch DIRECTORY named `name` under [`root`], emptied first — the
/// drop-in for the `temp_dir().join(..)` + `remove_dir_all` pairs the fixtures
/// were written with.
pub fn dir(name: &str) -> PathBuf {
    let directory = root().join(name);
    let _ = std::fs::remove_dir_all(&directory);
    directory
}

/// A scratch file that removes itself.
///
/// The `.mjs` sites used to `remove_file` after `node` returned, which leaks
/// the file on every path that does not get there: an `.expect` between the
/// write and the run, a panicking assert after it, a process killed under
/// load. A `Drop` closes all of those.
pub struct ScratchFile {
    path: PathBuf,
}

impl ScratchFile {
    /// Writes `contents` to `name` under [`root`].
    pub fn write(name: &str, contents: &str) -> Result<Self, String> {
        let path = root().join(name);
        match std::fs::write(&path, contents) {
            Ok(()) => Ok(Self { path }),
            Err(error) => Err(storage_failure(&path, &error)),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ScratchFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// An IO failure on a scratch path, with the free space NAMED when the
/// filesystem is out of it — so an ENOSPC red arrives saying what it is
/// instead of as `Os { code: 28 }` inside a list of diagnostics, which is the
/// half of N82 that made the spurious reds expensive rather than merely
/// annoying.
pub fn storage_failure(path: &Path, error: &std::io::Error) -> String {
    let full = error.kind() == std::io::ErrorKind::StorageFull || error.raw_os_error() == Some(28);
    if !full {
        return format!("{}: {error}", path.display());
    }
    format!(
        "{}: {error} — the harness's scratch filesystem is FULL{}. This is \
         tracker N82's shape: the red is the disk's, not the compiler's. Free \
         space and re-run before believing it.",
        path.display(),
        free_space(path),
    )
}

/// What `df` says is left where `path` lives, as a parenthetical. Empty when
/// there is no `df` to ask (every non-unix host), because a guard that cannot
/// answer says nothing rather than guessing.
fn free_space(path: &Path) -> String {
    let Some(parent) = path.parent() else {
        return String::new();
    };
    let Ok(output) = std::process::Command::new("df")
        .arg("-Pk")
        .arg(parent)
        .output()
    else {
        return String::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let Some(line) = text.lines().nth(1) else {
        return String::new();
    };
    let fields: Vec<&str> = line.split_whitespace().collect();
    let (Some(available), Some(mount)) = (fields.get(3), fields.last()) else {
        return String::new();
    };
    let Ok(kib) = available.parse::<u64>() else {
        return String::new();
    };
    format!(" ({} MiB free on `{mount}`)", kib / 1024)
}
