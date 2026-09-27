//! Pins for the embedded toolchain (proposal/releases.md §3): the embedded
//! table must mirror the working tree exactly (both directions — a collector
//! that misses a directory is a silently incomplete toolchain), and
//! materialization must produce a complete, idempotent, real-file copy — and
//! PRUNE the root it just grew (L21), which is what keeps a machine that builds
//! the toolchain from source from accumulating one std tree per build.

use std::fs;
use std::path::{Path, PathBuf};

use vilan_embedded::{
    CONTENT_HASH, FILES, RT_CONTENT_HASH, RT_CRYPTO_MANIFEST, RT_FILES, RT_MANIFEST,
    RT_SQLITE_MANIFEST, materialize_into,
};

mod scratch;

fn vilan_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan")
}

#[test]
fn the_table_carries_the_expected_packages() {
    let keys: Vec<&str> = FILES.iter().map(|(key, _)| *key).collect();
    // Both package manifests (resolve_std needs them for the platform layers),
    // a base module, a layer module, and macro_std's entry.
    for expected in [
        "std/vilan.toml",
        "std/src/lib.vl",
        "std/src/reactive.vl",
        "std/src/browser/dom.vl",
        "std/src/process/fs.vl",
        "macro_std/vilan.toml",
        "macro_std/src/lib.vl",
    ] {
        assert!(
            keys.contains(&expected),
            "missing embedded file: {expected}"
        );
    }
    assert!(keys.is_sorted(), "the table must be sorted (stable hash)");
    for key in &keys {
        assert!(
            !key.contains('\\') && !key.starts_with('/'),
            "keys are relative, forward-slash paths: {key}"
        );
    }
    assert_eq!(CONTENT_HASH.len(), 16, "the hash is 16 hex characters");
}

#[test]
fn the_table_matches_the_working_tree_in_both_directions() {
    let root = vilan_root();
    // Embedded → disk: every entry is byte-identical to the checkout.
    for (key, contents) in FILES {
        let on_disk = fs::read_to_string(root.join(key))
            .unwrap_or_else(|error| panic!("embedded {key} missing on disk: {error}"));
        assert!(
            on_disk == *contents,
            "embedded {key} differs from the working tree (stale build script output?)"
        );
    }
    // Disk → embedded: every toolchain file in the checkout is embedded.
    let mut on_disk = Vec::new();
    for package in ["std", "macro_std"] {
        walk(&root.join(package), Path::new(package), &mut on_disk);
    }
    let keys: Vec<&str> = FILES.iter().map(|(key, _)| *key).collect();
    for file in on_disk {
        assert!(
            keys.contains(&file.as_str()),
            "{file} exists in the checkout but is not embedded (collector gap)"
        );
    }
}

/// F19's table, held to the working tree in both directions exactly as std's is
/// — and its manifest held to being STANDALONE, which is the one way the two
/// tables differ: the real `crates/vilan-rt/Cargo.toml` is a workspace member
/// (`[lints] workspace = true`), so embedding it verbatim would materialize a
/// crate that only builds inside this repository.
#[test]
fn the_runtime_table_matches_the_crate_and_its_manifest_stands_alone() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let keys: Vec<&str> = RT_FILES.iter().map(|(key, _)| *key).collect();
    assert!(keys.contains(&"vilan-rt/src/lib.rs"), "{keys:?}");
    assert!(keys.is_sorted(), "the table must be sorted (stable hash)");
    assert!(
        !keys.contains(&"vilan-rt/Cargo.toml"),
        "the manifest is generated, not embedded: {keys:?}"
    );
    assert_eq!(RT_CONTENT_HASH.len(), 16);
    assert_ne!(
        RT_CONTENT_HASH, CONTENT_HASH,
        "the two trees take separate hashes so neither invalidates the other"
    );
    for (key, contents) in RT_FILES {
        let on_disk = fs::read_to_string(root.join(key))
            .unwrap_or_else(|error| panic!("embedded {key} missing on disk: {error}"));
        assert!(
            on_disk == *contents,
            "embedded {key} differs from the working tree (stale build script output?)"
        );
    }
    let mut on_disk = Vec::new();
    walk_rust(&root.join("vilan-rt"), Path::new("vilan-rt"), &mut on_disk);
    for file in on_disk {
        assert!(
            keys.contains(&file.as_str()),
            "{file} exists in the crate but is not embedded (collector gap)"
        );
    }

    let real = fs::read_to_string(root.join("vilan-rt/Cargo.toml")).expect("the real manifest");
    assert!(
        real.contains("workspace = true"),
        "the premise of the generated manifest is that the real one inherits \
         workspace lints; it no longer does, so the generation can be simplified"
    );
    assert!(!RT_MANIFEST.contains("workspace = true"), "{RT_MANIFEST}");
    assert!(RT_MANIFEST.contains("[workspace]"), "{RT_MANIFEST}");
    assert!(RT_MANIFEST.contains("name = \"vilan-rt\""), "{RT_MANIFEST}");
    assert!(RT_MANIFEST.contains("edition = \"2024\""), "{RT_MANIFEST}");
    assert!(RT_MANIFEST.contains("[dependencies]"), "{RT_MANIFEST}");
}

/// F19: materializing the runtime is complete, idempotent, and lands the crate
/// under `<hash>/vilan-rt/` with a manifest beside its sources.
#[test]
fn the_runtime_materializes_completely_and_idempotently() {
    let cache_root = scratch::root().join(format!(
        "vilan-embedded-rt-test-{}-{RT_CONTENT_HASH}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&cache_root);

    let crate_dir = vilan_embedded::materialize_rt_into(&cache_root).expect("first");
    assert!(crate_dir.ends_with(Path::new(RT_CONTENT_HASH).join("vilan-rt")));
    assert_eq!(
        fs::read_to_string(crate_dir.join("Cargo.toml")).expect("manifest"),
        RT_MANIFEST
    );
    for (key, contents) in RT_FILES {
        let written = fs::read_to_string(cache_root.join(RT_CONTENT_HASH).join(key)).expect(key);
        assert!(written == *contents, "materialized {key} differs");
    }

    let before = fs::metadata(crate_dir.join("src/lib.rs"))
        .unwrap()
        .modified()
        .unwrap();
    let again = vilan_embedded::materialize_rt_into(&cache_root).expect("second");
    assert_eq!(again, crate_dir);
    assert_eq!(
        before,
        fs::metadata(crate_dir.join("src/lib.rs"))
            .unwrap()
            .modified()
            .unwrap(),
        "an existing cache entry must not be rewritten"
    );

    let _ = fs::remove_dir_all(&cache_root);
}

/// F18 slice 2 and F40: the two OPTIONAL runtime crates ride the runtime's
/// table and its cache — every source of each is embedded (a collector gap
/// would materialize a crate that does not build), each materialized manifest
/// KEEPS its crates.io dependency (the reason it is a crate apart) and drops
/// the workspace-only lints, and both land BESIDE `vilan-rt` under the one
/// hash, which is where the generated cargo manifest looks for them.
#[test]
fn the_optional_crates_embed_and_materialize_beside_the_runtime() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let keys: Vec<&str> = RT_FILES.iter().map(|(key, _)| *key).collect();
    let optional = [
        ("vilan-rt-sqlite", RT_SQLITE_MANIFEST, "rusqlite"),
        ("vilan-rt-crypto", RT_CRYPTO_MANIFEST, "getrandom"),
    ];
    for (directory, manifest, dependency) in optional {
        let mut on_disk = Vec::new();
        walk_rust(&root.join(directory), Path::new(directory), &mut on_disk);
        assert!(
            on_disk.iter().any(|file| file.ends_with("src/lib.rs")),
            "{directory} has a library root: {on_disk:?}"
        );
        for file in on_disk {
            assert!(
                keys.contains(&file.as_str()),
                "{file} exists in the crate but is not embedded (collector gap)"
            );
        }
        assert!(
            manifest.contains(&format!("name = \"{directory}\"")),
            "{manifest}"
        );
        assert!(manifest.contains(dependency), "{manifest}");
        assert!(!manifest.contains("workspace = true"), "{manifest}");
        assert!(manifest.contains("[workspace]"), "{manifest}");
    }

    let cache_root = scratch::root().join(format!(
        "vilan-embedded-optional-test-{}-{RT_CONTENT_HASH}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&cache_root);
    let runtime = vilan_embedded::materialize_rt_into(&cache_root).expect("materialize");
    let beside = runtime.parent().expect("the runtime sits under its hash");
    for (directory, manifest, _) in optional {
        assert_eq!(
            fs::read_to_string(beside.join(directory).join("Cargo.toml")).expect(directory),
            manifest,
            "{directory}'s manifest lands beside the runtime"
        );
        assert!(
            beside.join(directory).join("src").join("lib.rs").is_file(),
            "{directory}'s sources land beside the runtime"
        );
    }
    let _ = fs::remove_dir_all(&cache_root);
}

/// Every `.rs` file under `directory`, as the table's keys spell them.
fn walk_rust(directory: &Path, prefix: &Path, out: &mut Vec<String>) {
    for entry in fs::read_dir(directory)
        .expect("read the runtime crate")
        .flatten()
    {
        let path = entry.path();
        let relative = prefix.join(entry.file_name());
        if path.is_dir() {
            walk_rust(&path, &relative, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(
                relative
                    .components()
                    .map(|component| component.as_os_str().to_str().unwrap())
                    .collect::<Vec<_>>()
                    .join("/"),
            );
        }
    }
}

#[test]
fn materialization_is_complete_and_idempotent() {
    let cache_root = scratch::root().join(format!(
        "vilan-embedded-test-{}-{CONTENT_HASH}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&cache_root);

    let std_dir = materialize_into(&cache_root).expect("first materialization");
    assert!(std_dir.ends_with(Path::new(CONTENT_HASH).join("std")));
    assert!(std_dir.join("vilan.toml").is_file(), "std manifest");
    // macro_std must land beside std — resolve_macro_std finds it as a sibling.
    let toolchain_root = std_dir.parent().unwrap();
    assert!(toolchain_root.join("macro_std/vilan.toml").is_file());
    for (key, contents) in FILES {
        let written = fs::read_to_string(toolchain_root.join(key)).expect(key);
        assert!(written == *contents, "materialized {key} differs");
    }

    // A second call finds the cache and does not rewrite it.
    let before = fs::metadata(std_dir.join("vilan.toml"))
        .unwrap()
        .modified()
        .unwrap();
    let again = materialize_into(&cache_root).expect("second materialization");
    assert_eq!(again, std_dir);
    let after = fs::metadata(std_dir.join("vilan.toml"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(
        before, after,
        "an existing cache entry must not be rewritten"
    );

    let _ = fs::remove_dir_all(&cache_root);
}

#[test]
fn pruning_removes_only_entries_older_than_the_guard() {
    use vilan_embedded::prune_stale;

    let cache_root = scratch::root().join(format!("vilan-embedded-prune-{}", std::process::id()));
    let _ = fs::remove_dir_all(&cache_root);
    for entry in [
        "fresh-entry",
        "stale-entry",
        ".staging-stale",
        ".staging-fresh",
    ] {
        fs::create_dir_all(cache_root.join(entry).join("std")).expect("create entry");
    }
    let one_day = std::time::Duration::from_secs(24 * 60 * 60);
    let long_ago = std::time::SystemTime::now() - 2 * one_day;
    for stale in ["stale-entry", ".staging-stale"] {
        backdate(&cache_root.join(stale), long_ago);
    }

    assert_eq!(
        prune_stale(&cache_root, one_day),
        2,
        "both backdated dirs go"
    );
    assert!(
        cache_root.join("fresh-entry").is_dir(),
        "young entries stay"
    );
    assert!(
        cache_root.join(".staging-fresh").is_dir(),
        "a staging dir inside the guard may be a materialization in flight"
    );
    assert!(!cache_root.join("stale-entry").exists());
    assert!(!cache_root.join(".staging-stale").exists());

    // A missing root is a quiet no-op, not an error.
    let _ = fs::remove_dir_all(&cache_root);
    assert_eq!(prune_stale(&cache_root, one_day), 0);
}

/// L21: materializing a NEW hash prunes the root it just grew.
///
/// `vilan upgrade` was the only pruner, and a toolchain refreshed from source
/// never runs it — so every lane binary and every release build left a tree
/// behind forever (374 entries, 316 MB on the owner's machine in two months).
/// The moment the cache GROWS is the moment worth pruning at, and it is the
/// only moment: an existing entry returns before any of this.
///
/// The guard is what makes it safe rather than the ordering: an entry's mtime
/// is its creation time and nothing touches it after the rename, so the tree
/// written a moment ago is the youngest thing in the root. Both halves are
/// pinned here — the week-old sibling goes, the fresh one stays, and the entry
/// this call just wrote is present at the end.
#[test]
fn materializing_a_new_hash_prunes_a_stale_sibling_and_keeps_a_fresh_one() {
    let cache_root = scratch::root().join(format!(
        "vilan-embedded-materialize-prune-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&cache_root);
    for sibling in ["fresh-sibling", "stale-sibling"] {
        fs::create_dir_all(cache_root.join(sibling).join("std")).expect("seed sibling");
    }
    let eight_days = std::time::Duration::from_secs(8 * 24 * 60 * 60);
    backdate(
        &cache_root.join("stale-sibling"),
        std::time::SystemTime::now() - eight_days,
    );

    let std_dir = materialize_into(&cache_root).expect("materialize");

    assert!(
        !cache_root.join("stale-sibling").exists(),
        "a sibling past the seven-day guard must go when a new hash lands"
    );
    assert!(
        cache_root.join("fresh-sibling").is_dir(),
        "a sibling inside the guard may belong to a running binary and must stay"
    );
    assert!(
        std_dir.join("vilan.toml").is_file(),
        "the entry this call wrote must survive its own prune"
    );

    let _ = fs::remove_dir_all(&cache_root);
}

/// L21: the current hash survives, at any age.
///
/// Two independent reasons, and the pin holds both at once. A cache HIT returns
/// before the prune runs at all — nothing landed, so nothing is swept — and
/// `prune` exempts [`CONTENT_HASH`]'s own entry however old it is, because
/// deleting a tree the running compile is reading buys nothing: the next
/// resolution writes it straight back.
#[test]
fn the_current_hash_survives_its_own_prune_however_old_it_is() {
    let cache_root = scratch::root().join(format!(
        "vilan-embedded-current-survives-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&cache_root);
    let std_dir = materialize_into(&cache_root).expect("materialize");
    let current = cache_root.join(CONTENT_HASH);
    let eight_days = std::time::Duration::from_secs(8 * 24 * 60 * 60);
    backdate(&current, std::time::SystemTime::now() - eight_days);

    // The cache-hit path: it returns before the prune, and answers the same.
    assert_eq!(materialize_into(&cache_root).expect("second"), std_dir);
    assert!(current.is_dir(), "a cache hit must not sweep anything");

    // And the sweep itself declines it, under the age guard and under `--all`.
    let one_day = std::time::Duration::from_secs(24 * 60 * 60);
    assert!(
        vilan_embedded::prune(&cache_root, Some(one_day), false).is_empty(),
        "this binary's own tree is never pruned by age"
    );
    assert!(
        vilan_embedded::prune(&cache_root, None, false).is_empty(),
        "this binary's own tree is never pruned by `--all` either"
    );
    assert!(current.is_dir());

    let _ = fs::remove_dir_all(&cache_root);
}

/// L21: a dry run reports exactly what a real run would remove, and removes
/// nothing.
///
/// The two calls are made against the SAME root in sequence — the dry run
/// first, then the real one — so the claim is not that the two lists look alike
/// but that the first list is the second's, entry for entry.
#[test]
fn a_dry_run_names_what_would_go_and_removes_nothing() {
    let cache_root = scratch::root().join(format!("vilan-embedded-dry-run-{}", std::process::id()));
    let _ = fs::remove_dir_all(&cache_root);
    for entry in ["fresh-entry", "stale-entry", ".staging-stale"] {
        fs::create_dir_all(cache_root.join(entry).join("std")).expect("seed entry");
        fs::write(cache_root.join(entry).join("std/vilan.toml"), "x").expect("seed file");
    }
    let one_day = std::time::Duration::from_secs(24 * 60 * 60);
    let long_ago = std::time::SystemTime::now() - 2 * one_day;
    for stale in ["stale-entry", ".staging-stale"] {
        backdate(&cache_root.join(stale), long_ago);
    }

    let would_go = vilan_embedded::prune(&cache_root, Some(one_day), true);
    let mut named: Vec<&str> = would_go.iter().map(|entry| entry.name.as_str()).collect();
    named.sort_unstable();
    assert_eq!(named, [".staging-stale", "stale-entry"]);
    assert!(
        would_go.iter().all(|entry| entry.bytes > 0),
        "a dry run reports sizes, so it must read them: {would_go:?}"
    );
    for entry in ["fresh-entry", "stale-entry", ".staging-stale"] {
        assert!(cache_root.join(entry).is_dir(), "a dry run removed {entry}");
    }

    let went = vilan_embedded::prune(&cache_root, Some(one_day), false);
    let mut really: Vec<&str> = went.iter().map(|entry| entry.name.as_str()).collect();
    really.sort_unstable();
    assert_eq!(really, named, "the dry run named something else than went");
    assert!(cache_root.join("fresh-entry").is_dir());
    assert!(!cache_root.join("stale-entry").exists());

    let _ = fs::remove_dir_all(&cache_root);
}

/// L21: `--all` drops the age guard and nothing else — `prune(.., None, ..)`.
#[test]
fn pruning_everything_ignores_the_age_guard_and_keeps_the_current_tree() {
    let cache_root =
        scratch::root().join(format!("vilan-embedded-prune-all-{}", std::process::id()));
    let _ = fs::remove_dir_all(&cache_root);
    materialize_into(&cache_root).expect("materialize");
    for entry in ["fresh-entry", "another-fresh-entry"] {
        fs::create_dir_all(cache_root.join(entry).join("std")).expect("seed entry");
    }

    let one_day = std::time::Duration::from_secs(24 * 60 * 60);
    assert!(
        vilan_embedded::prune(&cache_root, Some(one_day), false).is_empty(),
        "nothing here is a day old"
    );
    let went = vilan_embedded::prune(&cache_root, None, false);
    assert_eq!(went.len(), 2, "`--all` takes both young siblings: {went:?}");
    assert!(
        cache_root.join(CONTENT_HASH).is_dir(),
        "`--all` still keeps this binary's own tree"
    );

    let _ = fs::remove_dir_all(&cache_root);
}

/// Set a cache entry's modification time, so the prune guard sees it as old.
///
/// Replaces a `touch -d 2020-01-01` shell-out, which is coreutils and does not
/// exist on Windows (windows-support.md §4). The subject is a DIRECTORY —
/// `prune_stale` reads directory mtimes — and that is what makes the open
/// platform-shaped: unix opens a directory read-only and `futimens` is happy,
/// while Windows will not hand out a directory handle at all without
/// `FILE_FLAG_BACKUP_SEMANTICS`, and `SetFileInformationByHandle` wants write
/// access. That is exactly the dance the `filetime` crate does; two cfg'd lines
/// are cheaper than the dependency.
fn backdate(directory: &Path, to: std::time::SystemTime) {
    let mut options = fs::OpenOptions::new();
    #[cfg(unix)]
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        options.write(true).custom_flags(FILE_FLAG_BACKUP_SEMANTICS);
    }
    options
        .open(directory)
        .expect("open the cache entry")
        .set_modified(to)
        .expect("backdate the cache entry");
}

/// The build script's collection rule, restated independently: every `.vl` and
/// `vilan.toml` under the package directory.
fn walk(directory: &Path, prefix: &Path, files: &mut Vec<String>) {
    for entry in fs::read_dir(directory).expect("readable package directory") {
        let entry = entry.expect("readable entry");
        let path = entry.path();
        let relative = prefix.join(entry.file_name());
        if path.is_dir() {
            walk(&path, &relative, files);
        } else if path.extension().is_some_and(|extension| extension == "vl")
            || entry.file_name() == "vilan.toml"
        {
            files.push(
                relative
                    .components()
                    .map(|component| component.as_os_str().to_str().unwrap())
                    .collect::<Vec<_>>()
                    .join("/"),
            );
        }
    }
}
