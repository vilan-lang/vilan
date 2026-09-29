//! The commit stamp both binaries carry (proposal/releases.md §4), shared by
//! `vilan-cli`'s and `vilan-lsp`'s build scripts through `#[path]` so the two
//! halves of one toolchain can never stamp differently: `vilan --version`
//! prints `vilan <version> (<short-sha>)`, and the language server answers
//! `serverInfo.version` = `<version> (<short-sha>)` (E231), which is what lets
//! the editor tell two builds of ONE version apart. Builds without a git
//! checkout (a source tarball) stamp `unknown`.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Prints the `rerun-if-changed` lines that re-stamp on a commit or a branch
/// switch, and returns the stamp: `<sha>`, `<sha>-dirty`, or `unknown`.
///
/// A LINKED worktree's `.git` is a file (`gitdir: <path>`) naming a private
/// directory whose `HEAD` moves on every commit there, while the branch refs
/// and `packed-refs` live in the COMMON directory it names in `commondir`.
/// Watching only `<root>/.git/HEAD` — which does not exist there — left the
/// stamp at whatever commit the first build saw (install-dev.sh's re-stamp
/// retry is the scar), so both layouts are resolved here.
pub fn stamp(repo_root: &Path) -> String {
    if let Some((git_dir, common_dir)) = git_dirs(repo_root) {
        println!("cargo:rerun-if-changed={}", git_dir.join("HEAD").display());
        println!(
            "cargo:rerun-if-changed={}",
            common_dir.join("packed-refs").display()
        );
        if let Ok(head) = std::fs::read_to_string(git_dir.join("HEAD"))
            && let Some(reference) = head.trim().strip_prefix("ref: ")
        {
            println!(
                "cargo:rerun-if-changed={}",
                common_dir.join(reference).display()
            );
        }
    }
    match git(repo_root, &["rev-parse", "--short=9", "HEAD"]) {
        Some(sha) => {
            // `-uno`: untracked files don't change the binary. (A dirty
            // working tree can go stale between builds — acceptable for a
            // dev-only marker; release builds are always clean checkouts.)
            let dirty = git(repo_root, &["status", "--porcelain", "-uno"])
                .is_some_and(|status| !status.is_empty());
            if dirty { format!("{sha}-dirty") } else { sha }
        }
        None => "unknown".to_string(),
    }
}

/// `(git dir, common dir)` for a checkout or a linked worktree, or `None`
/// outside git.
fn git_dirs(repo_root: &Path) -> Option<(PathBuf, PathBuf)> {
    let dot_git = repo_root.join(".git");
    if dot_git.is_dir() {
        return Some((dot_git.clone(), dot_git));
    }
    let pointer = std::fs::read_to_string(&dot_git).ok()?;
    let git_dir = PathBuf::from(pointer.trim().strip_prefix("gitdir:")?.trim());
    let git_dir = if git_dir.is_absolute() {
        git_dir
    } else {
        repo_root.join(git_dir)
    };
    let common_dir = match std::fs::read_to_string(git_dir.join("commondir")) {
        Ok(common) => {
            let common = PathBuf::from(common.trim());
            if common.is_absolute() {
                common
            } else {
                git_dir.join(common)
            }
        }
        Err(_) => git_dir.clone(),
    };
    Some((git_dir, common_dir))
}

fn git(repo_root: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(repo_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8(output.stdout).ok()?.trim().to_string())
}
