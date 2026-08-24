//! The completeness gate for `THIRD-PARTY-NOTICES.txt` (backlog F10): the
//! release archives, npm packages, and brew install ship the notices for the
//! statically linked crates, and the file is checked in (like the vsix's
//! `editors/vscode/ThirdPartyNotices.txt`) rather than generated in CI. That
//! makes drift the failure mode — a dependency added without regenerating the
//! notices would ship uncovered — so this test walks `Cargo.lock` and requires
//! every third-party package to appear in the notices. Regenerate with:
//!
//!   cargo about generate about.hbs -o THIRD-PARTY-NOTICES.txt
//!
//! (config in `about.toml`; `cargo install cargo-about --features cli` once).
//!
//! The check is name-level on purpose: version bumps only change version
//! strings inside the file (cargo-about rewrites them on regeneration), and
//! pinning versions here would make every routine bump a two-file change with
//! no coverage gained.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repo root resolves")
}

/// The workspace's own crates — first-party, not third-party.
const WORKSPACE_CRATES: &[&str] = &[
    "vilan-cli",
    "vilan-core",
    "vilan-embedded-std",
    "vilan-lsp",
    "vilan-ide",
];

#[test]
fn every_locked_dependency_appears_in_the_notices() {
    let root = repo_root();
    let lock = std::fs::read_to_string(root.join("Cargo.lock")).expect("read Cargo.lock");
    let notices =
        std::fs::read_to_string(root.join("THIRD-PARTY-NOTICES.txt")).expect("read the notices");

    let mut missing = BTreeSet::new();
    let mut lines = lock.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim() != "[[package]]" {
            continue;
        }
        let Some(name_line) = lines.peek() else {
            break;
        };
        let Some(name) = name_line
            .trim()
            .strip_prefix("name = \"")
            .and_then(|rest| rest.strip_suffix('"'))
        else {
            continue;
        };
        if WORKSPACE_CRATES.contains(&name) {
            continue;
        }
        // The notices list crates as `name version` in their `Covers:` lines;
        // require the name bounded by a space so `serde` cannot be satisfied
        // by `serde_json`.
        if !notices.contains(&format!(" {name} ")) && !notices.contains(&format!(": {name} ")) {
            missing.insert(name.to_string());
        }
    }

    assert!(
        missing.is_empty(),
        "Cargo.lock has {} package(s) the third-party notices do not cover — \
         regenerate with `cargo about generate about.hbs -o THIRD-PARTY-NOTICES.txt`:\n  {}",
        missing.len(),
        missing.into_iter().collect::<Vec<_>>().join("\n  ")
    );
}
