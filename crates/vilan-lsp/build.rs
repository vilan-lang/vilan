//! Stamps the language server with its git commit (E231): `vilan-lsp
//! --version` and the `initialize` result's `serverInfo.version` both read
//! `<version> (<short-sha>)`, exactly as `vilan --version` does, so the editor
//! can tell a sealed-tip server from a stale one of the same crate version.
//! The stamp is `vilan-cli`'s, by path, so the two binaries cannot disagree.

use std::path::Path;

#[path = "../vilan-cli/build_stamp.rs"]
mod build_stamp;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let repo_root = Path::new(&manifest_dir).join("../..");
    let stamp = build_stamp::stamp(&repo_root);
    println!("cargo:rustc-env=VILAN_BUILD_SHA={stamp}");
}
