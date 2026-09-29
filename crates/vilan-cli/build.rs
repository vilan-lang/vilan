//! Stamps the binary with its git commit (proposal/releases.md §4):
//! `vilan --version` prints `vilan <version> (<short-sha>)`, so bug reports
//! against moving alpha builds are precise. The stamp itself is
//! `build_stamp.rs`, shared with `vilan-lsp`'s build script (E231).

use std::path::Path;

#[path = "build_stamp.rs"]
mod build_stamp;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let repo_root = Path::new(&manifest_dir).join("../..");
    let stamp = build_stamp::stamp(&repo_root);
    println!("cargo:rustc-env=VILAN_BUILD_SHA={stamp}");
    // The compile target, so `vilan upgrade` downloads its own platform's
    // asset (`vilan-<target>.tar.gz`).
    println!(
        "cargo:rustc-env=VILAN_TARGET={}",
        std::env::var("TARGET").unwrap()
    );
}
