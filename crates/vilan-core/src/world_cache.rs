//! std-prefix P4 — the world cache's HOUSEKEEPING (`std-prefix.md` §13 P4): what
//! lives under `~/.vilan/cache/worlds` (or `VILAN_WORLD_CACHE`), and the three
//! ways it shrinks.
//!
//! P3 writes one directory per key — `<key hash>/std.world`, or
//! `dependencies.world` for a program with dependencies (M152) — and the std
//! import index beside them (`std-index/<std hash>.index`, 3 KB each), and never
//! removes a sibling. Every dev build of vilan-core is a new build id, so a
//! developer's root gains a set of files per rebuild (incr-53b's N?1). This
//! module is the rule for taking them away:
//!
//! - [`prune`] — `vilan cache prune`: a directory written by ANOTHER build of
//!   the analyzer (G28's stamp: the file's header carries the build id in
//!   clear, so it is told apart without decoding the world), older than the
//!   age guard unless `--all`; this build's directories are never pruned (the
//!   next analysis would write them straight back), and the std index goes
//!   under `--all` only.
//! - [`clean`] — `vilan cache clean`: everything.
//! - [`bound`] — at every write: the root held to a byte budget
//!   ([`max_bytes`], 256 MiB by default — the check cache's), oldest first by
//!   last USE (a load touches its file), never the directory just written.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// The default byte budget of the world cache: the check cache's (N137).
pub const DEFAULT_MAX_BYTES: u64 = 256 * 1024 * 1024;

/// The name of the std import index's directory under the root.
pub const STD_INDEX_DIRECTORY: &str = "std-index";

/// One directory under the root.
#[derive(Clone, Debug)]
pub struct WorldCacheEntry {
    /// The directory's name: a key hash, or [`STD_INDEX_DIRECTORY`].
    pub name: String,
    pub path: PathBuf,
    /// Bytes on disk, summed over the directory.
    pub bytes: u64,
    /// Since its newest file was written or last LOADED (a load touches it) —
    /// `None` where the platform will not answer (treated as young).
    pub age: Option<Duration>,
    /// The analyzer build the world was written by, read off the file's
    /// header; `None` for the std index, or a file whose header does not read.
    pub build: Option<String>,
    /// The tiers the file carries (`std`, `dependencies`).
    pub tiers: Vec<String>,
    /// Whether THIS build wrote it: never pruned.
    pub current: bool,
}

/// The world cache's byte budget: `VILAN_WORLD_CACHE_BYTES` when set to a
/// number, else [`DEFAULT_MAX_BYTES`].
pub fn max_bytes() -> u64 {
    std::env::var("VILAN_WORLD_CACHE_BYTES")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_MAX_BYTES)
}

/// Every directory under `root`, the std index included.
pub fn entries(root: &Path) -> Vec<WorldCacheEntry> {
    let Ok(read) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let now = SystemTime::now();
    let mut found = Vec::new();
    for entry in read.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let (bytes, newest) = directory_bytes_and_newest(&path);
        let age = newest.and_then(|newest| now.duration_since(newest).ok());
        let header = (name != STD_INDEX_DIRECTORY)
            .then(|| crate::prefix_file::directory_key(&path))
            .flatten();
        let build = header.as_ref().map(|key| key.build_id.clone());
        found.push(WorldCacheEntry {
            current: build.as_deref() == Some(crate::BUILD_ID),
            tiers: header.map(|key| key.tiers).unwrap_or_default(),
            build,
            name,
            path,
            bytes,
            age,
        });
    }
    found.sort_by(|left, right| left.name.cmp(&right.name));
    found
}

/// `vilan cache prune`: every directory another build wrote (or whose header
/// does not read), older than `max_age` when one is given; the std index only
/// when `max_age` is `None` (`--all`). Returns what went (or, under `dry_run`,
/// would go).
pub fn prune(root: &Path, max_age: Option<Duration>, dry_run: bool) -> Vec<WorldCacheEntry> {
    let mut removed = Vec::new();
    for entry in entries(root) {
        if entry.current {
            continue;
        }
        let index = entry.name == STD_INDEX_DIRECTORY;
        let stale = match max_age {
            None => true,
            Some(_) if index => false,
            Some(max_age) => entry.age.is_some_and(|age| age > max_age),
        };
        if stale && (dry_run || std::fs::remove_dir_all(&entry.path).is_ok()) {
            removed.push(entry);
        }
    }
    removed
}

/// `vilan cache clean`: every directory under `root`. Every file is a cache in
/// the strict sense — the next analysis that wants one writes it.
pub fn clean(root: &Path, dry_run: bool) -> Vec<WorldCacheEntry> {
    entries(root)
        .into_iter()
        .filter(|entry| dry_run || std::fs::remove_dir_all(&entry.path).is_ok())
        .collect()
}

/// The write-time bound: the oldest (least recently used) world directories go
/// until the root is within `max_bytes`, never `keep` (the directory just
/// written) nor the std index. Returns what went.
pub fn bound(root: &Path, max_bytes: u64, keep: Option<&Path>) -> Vec<WorldCacheEntry> {
    let mut all = entries(root);
    let mut total: u64 = all.iter().map(|entry| entry.bytes).sum();
    if total <= max_bytes {
        return Vec::new();
    }
    // Oldest first; an entry whose age the platform will not give is young.
    all.sort_by(|left, right| {
        right
            .age
            .unwrap_or_default()
            .cmp(&left.age.unwrap_or_default())
            .then_with(|| left.name.cmp(&right.name))
    });
    let mut removed = Vec::new();
    for entry in all {
        if total <= max_bytes {
            break;
        }
        if entry.name == STD_INDEX_DIRECTORY || keep.is_some_and(|keep| keep == entry.path) {
            continue;
        }
        if std::fs::remove_dir_all(&entry.path).is_ok() {
            total = total.saturating_sub(entry.bytes);
            removed.push(entry);
        }
    }
    removed
}

/// Bytes under `directory` and the newest modification time of any file in it.
fn directory_bytes_and_newest(directory: &Path) -> (u64, Option<SystemTime>) {
    let Ok(read) = std::fs::read_dir(directory) else {
        return (0, None);
    };
    let mut bytes = 0;
    let mut newest: Option<SystemTime> = None;
    for entry in read.flatten() {
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_dir() {
            let (inner, inner_newest) = directory_bytes_and_newest(&entry.path());
            bytes += inner;
            newest = newest.max(inner_newest);
        } else {
            bytes += metadata.len();
            newest = newest.max(metadata.modified().ok());
        }
    }
    (bytes, newest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "vilan-world-cache-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("create the root");
        directory
    }

    fn write_world(root: &Path, build: &str, closure: &str, bytes: usize) -> PathBuf {
        let key = crate::prefix_file::Key {
            format: crate::prefix_file::FORMAT_VERSION,
            build_id: build.to_string(),
            platform: "Node { version: 24 }".to_string(),
            closure: vec![closure.to_string()],
            std_content: 7,
            macro_limits: (1_000_000, 64),
            entry_prelude: None,
            workspace: Vec::new(),
            tiers: vec!["std".to_string()],
        };
        let written =
            crate::prefix_file::write(root, &key, &[PathBuf::from("/std")], &vec![0u8; bytes], b"")
                .expect("write a world");
        written
            .path
            .parent()
            .expect("the key directory")
            .to_path_buf()
    }

    fn age(path: &Path, days: u64) {
        let world = std::fs::read_dir(path)
            .expect("the key directory")
            .flatten()
            .next()
            .expect("its file")
            .path();
        let file = std::fs::File::options()
            .append(true)
            .open(world)
            .expect("open the world file");
        file.set_modified(SystemTime::now() - Duration::from_secs(days * 24 * 60 * 60))
            .expect("age the file");
    }

    /// P4: `prune` takes another build's directories past the age guard (all of
    /// them under `--all`), never this build's; `clean` takes everything; the
    /// header names each directory's build and tiers.
    #[test]
    fn p4_prune_takes_foreign_builds_and_keeps_this_one() {
        let root = scratch("prune");
        let mine = write_world(&root, crate::BUILD_ID, "io", 16);
        let old_foreign = write_world(&root, "0123456789abcdef", "io", 16);
        let young_foreign = write_world(&root, "fedcba9876543210", "path", 16);
        age(&mine, 30);
        age(&old_foreign, 30);
        std::fs::create_dir_all(root.join(STD_INDEX_DIRECTORY)).expect("the index directory");
        std::fs::write(root.join(STD_INDEX_DIRECTORY).join("a.index"), b"x").expect("an index");
        let listed = entries(&root);
        assert_eq!(listed.len(), 4, "{listed:#?}");
        let current: Vec<&WorldCacheEntry> = listed.iter().filter(|entry| entry.current).collect();
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].path, mine);
        assert_eq!(current[0].tiers, vec!["std".to_string()]);
        let seven_days = Some(Duration::from_secs(7 * 24 * 60 * 60));
        let dry = prune(&root, seven_days, true);
        assert_eq!(
            dry.len(),
            1,
            "a dry run names the stale foreign build: {dry:#?}"
        );
        assert!(old_foreign.is_dir(), "and deletes nothing");
        let removed = prune(&root, seven_days, false);
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].path, old_foreign);
        assert!(!old_foreign.exists() && young_foreign.is_dir() && mine.is_dir());
        let all = prune(&root, None, false);
        assert_eq!(
            all.len(),
            2,
            "--all takes the young foreign build and the index: {all:#?}"
        );
        assert!(mine.is_dir(), "this build's world is never pruned");
        let cleaned = clean(&root, false);
        assert_eq!(cleaned.len(), 1);
        assert!(!mine.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// P4: the write-time bound takes the least recently used worlds first until
    /// the root fits, and never the directory just written.
    #[test]
    fn p4_the_bound_takes_the_oldest_until_the_root_fits() {
        let root = scratch("bound");
        let oldest = write_world(&root, crate::BUILD_ID, "a", 1000);
        let middle = write_world(&root, crate::BUILD_ID, "b", 1000);
        let newest = write_world(&root, crate::BUILD_ID, "c", 1000);
        age(&oldest, 3);
        age(&middle, 2);
        age(&newest, 1);
        let total: u64 = entries(&root).iter().map(|entry| entry.bytes).sum();
        assert!(
            bound(&root, total, None).is_empty(),
            "a root within its budget loses nothing"
        );
        let removed = bound(&root, total - 1, Some(&oldest));
        assert_eq!(removed.len(), 1, "{removed:#?}");
        assert_eq!(
            removed[0].path, middle,
            "the oldest is kept as the one just written"
        );
        assert!(oldest.is_dir() && newest.is_dir());
        let _ = std::fs::remove_dir_all(&root);
    }
}
