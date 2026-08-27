//! `AGENTS.md`'s repo map, held to the workspace it describes.
//!
//! The map is the first thing a coding agent reads, and it drifts silently
//! because a stale sentence still compiles. `crates/vilan-ide` landed
//! 2026-08-22 (K9, the completion engine moving out of `vilan-lsp` so the
//! playground could share it); the map still said "five crates" and listed
//! five bullets on 2026-08-27, through two later edits to the same file and
//! several work orders — so every agent briefed from it started with a
//! workspace picture missing a crate, and a completion change had no reason to
//! land anywhere but `vilan-lsp`.
//!
//! This is the same shape as `grammar_sync.rs` and `vilan-lsp`'s
//! `book_sync.rs`: one side of a hand-written pairing is held to a
//! machine-readable source of truth on every suite run. Here the truth is the
//! root `Cargo.toml`'s `members` list — the only list that decides what the
//! workspace actually builds.
//!
//! Deliberately only the *membership* and the *count word*: what each bullet
//! SAYS about its crate is prose no test can judge, and pinning the prose would
//! turn every honest map improvement into a red suite. Membership is the half
//! that has a right answer.

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn agents_map() -> String {
    let path = repo_root().join("AGENTS.md");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// The workspace's `members` paths, in declaration order. A deliberately small
/// scan rather than a TOML dependency: the block is one flat list of quoted
/// strings, and this test exists to notice when that list changes.
fn workspace_members() -> Vec<String> {
    let path = repo_root().join("Cargo.toml");
    let manifest = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let start = manifest
        .find("members = [")
        .expect("the root manifest declares `members = [`");
    let rest = &manifest[start..];
    let end = rest.find(']').expect("the `members` list is closed");
    let members: Vec<String> = rest[..end]
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect();
    assert!(
        !members.is_empty(),
        "no members parsed out of the root manifest — the scan's assumption broke"
    );
    members
}

/// The `crates/…` paths the map gives their own top-level bullet.
fn mapped_crates(map: &str) -> Vec<String> {
    map.lines()
        .filter_map(|line| line.strip_prefix("- `crates/"))
        .filter_map(|rest| rest.split('`').next())
        .map(|name| format!("crates/{name}"))
        .collect()
}

#[test]
fn the_repo_map_gives_every_workspace_member_its_own_bullet() {
    let map = agents_map();
    let mapped = mapped_crates(&map);
    let missing: Vec<String> = workspace_members()
        .into_iter()
        .filter(|member| !mapped.contains(member))
        .collect();
    assert!(
        missing.is_empty(),
        "AGENTS.md's repo map has no bullet for {missing:?} — a crate the workspace \
         builds that an agent reading the map would never learn exists. Add a \
         `- `<path>` — …` bullet under \"The lay of the land\"."
    );
}

#[test]
fn the_repo_map_names_no_crate_the_workspace_does_not_build() {
    let map = agents_map();
    let members = workspace_members();
    let stale: Vec<String> = mapped_crates(&map)
        .into_iter()
        .filter(|mapped| !members.contains(mapped))
        .collect();
    assert!(
        stale.is_empty(),
        "AGENTS.md's repo map bullets {stale:?}, which the root manifest does not \
         list as a workspace member — a renamed or deleted crate left behind in the map."
    );
}

#[test]
fn the_repo_maps_count_word_matches_the_number_of_members() {
    const WORDS: &[&str] = &[
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
        "eleven", "twelve",
    ];
    let members = workspace_members();
    let word = WORDS
        .get(members.len())
        .unwrap_or_else(|| panic!("{} members outgrew the word table", members.len()));
    let expected = format!("Rust workspace, {word} crates,");
    let map = agents_map();
    assert!(
        map.contains(&expected),
        "AGENTS.md's repo map does not open with {expected:?} — the workspace has {} \
         members. (If the sentence was reworded rather than gone stale, reword this \
         gate with it.)",
        members.len()
    );
}
