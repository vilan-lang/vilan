//! The analyzer's BUILD IDENTITY (G28; std-prefix.md Q5): a hash of vilan-core's own
//! sources, computed once here and read from `vilan_core::BUILD_ID`.
//!
//! Every on-disk memory the compiler keeps is only as good as the code that wrote it. The
//! macro-expansion table was stamped `toolchain <version>` plus `macro_std`'s hash, so two
//! dev builds between releases - one with a changed interpreter or expansion engine - shared
//! a stamp, and the second was served the first's expansions until `vilan cache clean`.
//! The persisted std prefix will need the same identity in its key (std-prefix.md Q5), so it
//! is named once, here, rather than once per cache.
//!
//! The hash covers every file under `src/` (relative path and bytes, in sorted order) and
//! the manifest, not a git sha: a dirty tree, a worktree and a source tarball all hash by
//! what they compile. It is SHA-256 truncated to 64 bits - stable across Rust versions and
//! platforms, which `DefaultHasher` does not promise - and it moves with any edit to the
//! crate, whether or not the edit changes behaviour: a spurious miss costs one cold check,
//! a stale hit costs a wrong answer.

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets it"));
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=src");
    println!(
        "cargo:rustc-env=VILAN_CORE_BUILD_ID={}",
        build_id(&manifest_dir)
    );
}

/// The id of the crate at `manifest_dir`: sixteen lowercase hex digits. Public so
/// `tests/build_id.rs` can include this file and pin the rule on a staged tree.
#[allow(dead_code)]
pub fn build_id(manifest_dir: &Path) -> String {
    let mut files = Vec::new();
    collect(&manifest_dir.join("src"), &mut files);
    files.push(manifest_dir.join("Cargo.toml"));
    files.sort();
    let mut hasher = Sha256::new();
    for path in &files {
        let relative = path.strip_prefix(manifest_dir).unwrap_or(path);
        // `/`-separated whatever the host's separator, so one tree hashes alike everywhere.
        let name: Vec<String> = relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect();
        let bytes = fs::read(path).unwrap_or_default();
        hasher.update(name.join("/").as_bytes());
        hasher.update([0]);
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    let digest = hasher.finalize();
    digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn collect(directory: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, files);
        } else {
            files.push(path);
        }
    }
}
