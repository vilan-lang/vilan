//! Embeds the `std` and `macro_std` package trees — and the native backend's
//! runtime crate — into the binary.
//!
//! Walks `vilan/std` and `vilan/macro_std` at the workspace root and generates
//! `$OUT_DIR/embedded_std.rs`: a sorted `FILES` table of
//! `("std/src/print.vl", include_str!(..))` entries plus a `CONTENT_HASH` over
//! the whole set. The hash keys the on-disk materialization cache
//! (`~/.vilan/std-cache/<hash>/`), so a rebuilt binary with different std never
//! reads a stale cache.
//!
//! `crates/vilan-rt` is embedded the same way under its OWN table and its own
//! hash (`RT_FILES`, `RT_CONTENT_HASH`, tracker F19): the emit-Rust backend
//! writes a cargo project that depends on that crate by path, and a released
//! toolchain has no source sibling to point at. Two tables rather than one
//! because the two trees move independently — a std edit must not re-materialize
//! the runtime, and a runtime edit must not invalidate every machine's std cache.
//!
//! `include_str!` makes every embedded file a compile input of this crate
//! (edits re-embed automatically); the `rerun-if-changed` directives on the
//! directories cover the set itself changing (files added or removed). This
//! lives in its own leaf crate so a std edit relinks the binaries without
//! recompiling the compiler.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let vilan_root = manifest_dir.join("../../vilan");
    let out_path = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("embedded_std.rs");

    let mut files = Vec::new();
    for package in ["std", "macro_std"] {
        let root = vilan_root.join(package);
        assert!(
            root.join("vilan.toml").is_file(),
            "embedded-std build: {} is not a package directory (missing vilan.toml) — \
             is this a complete checkout?",
            root.display()
        );
        collect(&root, Path::new(package), &mut files);
    }
    // Deterministic table and hash, independent of directory iteration order.
    files.sort();

    let mut generated = String::new();
    write_table(
        &mut generated,
        "CONTENT_HASH",
        "FILES",
        "The embedded `std` and `macro_std` package trees",
        &files,
    );

    let runtime_root = manifest_dir.join("../vilan-rt");
    let mut runtime_files = Vec::new();
    collect_rust(&runtime_root, Path::new("vilan-rt"), &mut runtime_files);
    assert!(
        runtime_files
            .iter()
            .any(|(key, _)| key == "vilan-rt/src/lib.rs"),
        "embedded-std build: {} holds no src/lib.rs — is this a complete checkout?",
        runtime_root.display()
    );
    runtime_files.sort();
    write_table(
        &mut generated,
        "RT_CONTENT_HASH",
        "RT_FILES",
        "The embedded `vilan-rt` source tree (tracker F19)",
        &runtime_files,
    );
    generated.push_str(&runtime_manifest(&runtime_root));

    let mut out = fs::File::create(&out_path).unwrap();
    out.write_all(generated.as_bytes()).unwrap();
}

/// Appends one `(hash, table)` pair to the generated module.
fn write_table(
    generated: &mut String,
    hash_name: &str,
    table_name: &str,
    what: &str,
    files: &[(String, PathBuf)],
) {
    let mut hasher = Fnv1a64::new();
    for (key, path) in files {
        hasher.write(key.as_bytes());
        hasher.write(&fs::read(path).unwrap());
    }
    generated.push_str(&format!(
        "/// A hash of every path and its contents in [`{table_name}`] — the key of\n\
         /// its materialization cache directory.\n\
         pub static {hash_name}: &str = \"{:016x}\";\n\n",
        hasher.finish()
    ));
    generated.push_str(&format!(
        "/// {what}, as (path relative to the\n\
         /// materialization root, contents) — sorted by path.\n\
         pub static {table_name}: &[(&str, &str)] = &[\n"
    ));
    for (key, path) in files {
        generated.push_str(&format!(
            "    ({:?}, include_str!({:?})),\n",
            key,
            path.canonicalize().unwrap()
        ));
    }
    generated.push_str("];\n\n");
}

/// The `Cargo.toml` the materialized runtime carries.
///
/// It is GENERATED from the real one rather than embedded verbatim, because the
/// real one is a workspace member: its `[lints] workspace = true` resolves only
/// inside this repository's workspace and would refuse to build anywhere else.
/// So the `[package]` block is copied through (name, version, edition, licence,
/// description — the identity a dependent's lockfile records) and the rest is
/// written here: an empty `[dependencies]`, because the crate has none by rule,
/// and a `[workspace]` of its own so cargo stops walking up out of the cache.
fn runtime_manifest(runtime_root: &Path) -> String {
    let path = runtime_root.join("Cargo.toml");
    println!("cargo:rerun-if-changed={}", path.display());
    let manifest = fs::read_to_string(&path).unwrap();
    let start = manifest
        .find("[package]")
        .expect("vilan-rt's manifest declares [package]");
    let block = &manifest[start..];
    let end = block[1..]
        .find("\n[")
        .map(|offset| offset + 2)
        .unwrap_or(block.len());
    let package = block[..end].trim_end();
    let generated = format!("{package}\n\n[dependencies]\n\n[workspace]\n");
    format!(
        "/// The `Cargo.toml` [`RT_FILES`] is materialized with — the real crate's\n\
         /// `[package]` block, an empty `[dependencies]` and a `[workspace]` of its\n\
         /// own (the real manifest's `[lints] workspace = true` resolves only inside\n\
         /// this repository).\n\
         pub static RT_MANIFEST: &str = {generated:?};\n"
    )
}

/// Collects every `.vl` and `vilan.toml` under `directory` into `files` as
/// (forward-slash key relative to the toolchain root, absolute path), and emits
/// `rerun-if-changed` for each directory so added or removed files regenerate
/// the table.
fn collect(directory: &Path, prefix: &Path, files: &mut Vec<(String, PathBuf)>) {
    println!("cargo:rerun-if-changed={}", directory.display());
    for entry in fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let relative = prefix.join(entry.file_name());
        if path.is_dir() {
            collect(&path, &relative, files);
        } else if path.extension().is_some_and(|extension| extension == "vl")
            || entry.file_name() == "vilan.toml"
        {
            files.push((key_of(&relative), path));
        }
    }
}

/// Collects every `.rs` file under `directory` into `files` as (forward-slash
/// key relative to the materialization root, absolute path) — [`collect`]'s
/// twin for the runtime crate, whose sources are Rust rather than vilan. Its
/// `Cargo.toml` is deliberately NOT collected: [`runtime_manifest`] generates
/// the one the cache carries.
fn collect_rust(directory: &Path, prefix: &Path, files: &mut Vec<(String, PathBuf)>) {
    println!("cargo:rerun-if-changed={}", directory.display());
    for entry in fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let relative = prefix.join(entry.file_name());
        if path.is_dir() {
            collect_rust(&path, &relative, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push((key_of(&relative), path));
        }
    }
}

/// A collected file's table key: its path relative to the materialization root,
/// forward-slashed on every platform so the generated table is identical
/// wherever it was built.
fn key_of(relative: &Path) -> String {
    relative
        .components()
        .map(|component| component.as_os_str().to_str().unwrap())
        .collect::<Vec<_>>()
        .join("/")
}

/// FNV-1a, 64-bit — tiny, dependency-free, and stable across builds. The hash
/// only keys a cache directory; it has no security role.
struct Fnv1a64(u64);

impl Fnv1a64 {
    fn new() -> Self {
        Fnv1a64(0xcbf2_9ce4_8422_2325)
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn finish(&self) -> u64 {
        self.0
    }
}
