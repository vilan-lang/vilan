//! G28: the analyzer's build identity (`build.rs`, `vilan_core::BUILD_ID`) - a hash of the
//! crate's own sources. Every on-disk memory stamped with it misses on a dev build between
//! two releases; the rule is pinned here on staged trees, by including the build script.
#![allow(dead_code)]

#[path = "../build.rs"]
mod build_script;
mod scratch;

use std::fs;
use std::path::PathBuf;

fn staged(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = scratch::root().join(format!("vilan-build-id-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src/deeper")).unwrap();
    fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
    for (path, text) in files {
        fs::write(root.join(path), text).unwrap();
    }
    root
}

#[test]
fn the_id_is_sixteen_hex_digits_and_the_exposed_one_is_this_crates() {
    assert_eq!(vilan_core::BUILD_ID.len(), 16, "{}", vilan_core::BUILD_ID);
    assert!(
        vilan_core::BUILD_ID
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    assert_eq!(
        vilan_core::BUILD_ID,
        build_script::build_id(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))),
        "the constant is the hash of the crate's sources as they stand"
    );
}

#[test]
fn two_builds_from_two_source_trees_differ_and_one_tree_hashes_alike() {
    let a = staged(
        "a",
        &[
            ("src/lib.rs", "fn one() {}\n"),
            ("src/deeper/m.rs", "fn two() {}\n"),
        ],
    );
    let same = staged(
        "same",
        &[
            ("src/lib.rs", "fn one() {}\n"),
            ("src/deeper/m.rs", "fn two() {}\n"),
        ],
    );
    let edited = staged(
        "edited",
        &[
            ("src/lib.rs", "fn one() {}\n"),
            ("src/deeper/m.rs", "fn two() { 1; }\n"),
        ],
    );
    let moved = staged(
        "moved",
        &[
            ("src/lib.rs", "fn one() {}\n"),
            ("src/deeper/n.rs", "fn two() {}\n"),
        ],
    );
    let id = build_script::build_id(&a);
    assert_eq!(
        id,
        build_script::build_id(&same),
        "the same sources, the same build, whatever the directory"
    );
    assert_ne!(
        id,
        build_script::build_id(&edited),
        "an edited source is a different build"
    );
    assert_ne!(id, build_script::build_id(&moved), "so is a renamed one");
    fs::write(a.join("src/extra.rs"), "").unwrap();
    assert_ne!(id, build_script::build_id(&a), "and an added file");
    for name in ["a", "same", "edited", "moved"] {
        let _ = fs::remove_dir_all(
            scratch::root().join(format!("vilan-build-id-{name}-{}", std::process::id())),
        );
    }
}
