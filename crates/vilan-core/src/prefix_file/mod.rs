//! std-prefix P3 — the PERSISTED std prefix (`std-prefix.md` §5–§6, §13 P3):
//! the std-only prefix world (P1) written once per (analyzer build, std
//! content, platform, std closure, macro limits) under the world cache and
//! loaded by every later process instead of being analyzed again.
//!
//! [`codec`] is the encoding; this module is the FILE: its key and name, its
//! header and checksum, the write (a temporary and a rename) and the read (a
//! MISS — never an error — for a file that is absent, unreadable, corrupt,
//! truncated, of another format version or of another key). The world itself
//! is encoded and decoded by `analyzer.rs` (`World` is its type) and fixed up
//! there after the load (`std-prefix.md` §3: the paths re-rooted to this
//! process's std, the derived indexes left to rebuild, the base cache's own
//! entry inserted); [`reroot`] is the path mapping both sides share.
//!
//! **The layout.** `MAGIC`, then the [`Header`] written inline (its own
//! strings in place, since the string table comes after the body), then the
//! body and the string table as [`codec::Writer::finish`] hands them back. The
//! header carries the [`Key`] in clear — so a reader compares, never trusts a
//! file name — the lengths of the two blocks that follow, and a checksum over
//! both. A key field that moves is a different file: the directory is named by
//! the key's hash.
//!
//! **The tiers.** A file carries a tier list from this first version. A
//! program with dependencies (M152) takes a prefix of std AND the dependency
//! modules its package requests — one world, its key naming both tiers
//! (`["std", "dependencies"]`) and carrying the dependency tier's rows (its
//! requests, a hash over its files) beside the workspace's — and joined the
//! format without a change to it.

pub mod codec;

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::fx::FxHashMap as HashMap;
use crate::id::Id;
use crate::span::Span;

/// Bumped by hand when the encoding or the world's rows change in a way the
/// build id does not already capture (it captures every edit to vilan-core,
/// so this is the format's own history, not the invalidation rule).
pub(crate) const FORMAT_VERSION: u32 = 1;

const MAGIC: &[u8] = b"vilan-std-prefix\0";

/// The tiers a file can carry, in order: std's, then the dependency tier a
/// program with dependencies adds (M152).
pub(crate) const STD_TIER: &str = "std";
pub(crate) const DEPENDENCIES_TIER: &str = "dependencies";

/// The file name inside the key's directory, by the key's LAST tier: a
/// std-only world is `std.world`, a std-and-dependencies one
/// `dependencies.world` (its key — and so its directory — differs anyway; the
/// name says what the file holds to a reader of the cache).
pub(crate) const STD_TIER_FILE: &str = "std.world";
pub(crate) const DEPENDENCIES_TIER_FILE: &str = "dependencies.world";

/// M158 (Order 54): the prefix key's CHECKS record (M19's `WorldChecks`
/// minus its package-keyed per-item records), beside the world it describes
/// in the key's directory — written at the end of the analysis that built the
/// world, so the world itself is written before any record exists and a
/// process that loads only the world (an older writer, a failed write) still
/// loads it. The same framing as the world (magic, header with the key in
/// clear, checksum): a damaged or foreign file is a miss of the record only.
pub(crate) const STD_CHECKS_FILE: &str = "std.checks";
pub(crate) const DEPENDENCIES_CHECKS_FILE: &str = "dependencies.checks";

/// What a prefix world is a function of (`std-prefix.md` §6), in clear.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Key {
    /// [`FORMAT_VERSION`].
    pub format: u32,
    /// The analyzer's build identity (`crate::BUILD_ID`, G28): a rebuilt
    /// vilan-core misses.
    pub build_id: String,
    /// The platform, rendered.
    pub platform: String,
    /// The std CLOSURE (§4.3), sorted — the module set, not the entry's seeds.
    pub closure: Vec<String>,
    /// A hash over the closure's files under every std root (and `lib.vl`):
    /// an edited std misses. The base cache re-validates the loaded world's
    /// exact sources by content on the hit (the E12 rule), so this is the
    /// key's half and that is the proof's.
    pub std_content: u64,
    /// The expansion budgets (two budgets resolve two worlds).
    pub macro_limits: (u64, u32),
    /// The entry package's prelude (`BaseCacheKey::entry_prelude`): it seeds
    /// the closure and the package modules' ambient scope.
    pub entry_prelude: Option<String>,
    /// The workspace, rendered (`BaseCacheKey::workspace`), and — for a
    /// world with a dependency tier (M152) — the tier's rows (its requests and
    /// a hash over its files) after them.
    pub workspace: Vec<String>,
    /// The tiers this file carries, in order: `std`, then `dependencies` for
    /// a program that has them (M152).
    pub tiers: Vec<String>,
}

impl Key {
    /// The directory under the world cache that holds this key's tiers:
    /// sixteen hex digits of the key's rendering.
    pub(crate) fn directory_name(&self) -> String {
        let rendered = format!("{self:?}");
        format!("{:016x}", crate::content_hash(&rendered))
    }
}

/// The header, written inline before the body.
#[derive(serde::Serialize, serde::Deserialize)]
struct Header {
    key: Key,
    /// The std roots the body's paths are rooted at (canonical, the base
    /// root then each layer's, in the spec's order — `BaseCacheKey::std_roots`),
    /// so the loader can re-root them to its own.
    std_roots: Vec<PathBuf>,
    body_len: u64,
    table_len: u64,
    /// `content_hash_bytes` over the body and the table.
    checksum: u64,
}

/// Why a file was not served. Every one is a miss; the reason rides the
/// census and the counters line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Miss {
    /// No file under this key's directory.
    Absent,
    /// The file could not be read.
    Unreadable,
    /// Not a prefix file, or one whose bytes do not add up (the checksum,
    /// a length past the end).
    Corrupt,
    /// Shorter than its header says.
    Truncated,
    /// Another format version.
    ForeignVersion,
    /// Another key (a rebuilt vilan-core, an edited std, another platform
    /// or closure) under this key's name.
    ForeignKey,
    /// The world's bytes decoded to nothing usable (the analyzer's own
    /// rows moved under the same build id — which the build id should make
    /// impossible — or a file written by a planted writer).
    Undecodable,
}

impl Miss {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Miss::Absent => "absent",
            Miss::Unreadable => "unreadable",
            Miss::Corrupt => "corrupt",
            Miss::Truncated => "truncated",
            Miss::ForeignVersion => "foreign-version",
            Miss::ForeignKey => "foreign-key",
            Miss::Undecodable => "undecodable",
        }
    }
}

/// A file read and validated: its body and string table, leaked so the
/// decoded world's strings can borrow them for the rest of the process.
pub(crate) struct Loaded {
    pub body: &'static [u8],
    pub table: &'static [u8],
    pub std_roots: Vec<PathBuf>,
    pub bytes: usize,
    pub read: Duration,
}

/// A wall-clock start, where the platform has one. `Instant::now()` aborts
/// on `wasm32-unknown-unknown` (B432); the playground has no filesystem and
/// never reaches a read or a write, but the code must still build there.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn now() -> Option<std::time::Instant> {
    Some(std::time::Instant::now())
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn now() -> Option<std::time::Instant> {
    None
}

/// Microseconds since [`now`]'s reading, 0 without a clock.
pub(crate) fn micros_since(started: Option<std::time::Instant>) -> u64 {
    started.map_or(0, |started| started.elapsed().as_micros() as u64)
}

/// Where this key's std tier lives under `directory`.
pub(crate) fn tier_path(directory: &Path, key: &Key) -> PathBuf {
    let file = match key.tiers.last().map(String::as_str) {
        Some(DEPENDENCIES_TIER) => DEPENDENCIES_TIER_FILE,
        _ => STD_TIER_FILE,
    };
    directory.join(key.directory_name()).join(file)
}

/// M158: where this key's checks record lives under `directory` — beside
/// its world, named by the same last tier.
pub(crate) fn checks_path(directory: &Path, key: &Key) -> PathBuf {
    let file = match key.tiers.last().map(String::as_str) {
        Some(DEPENDENCIES_TIER) => DEPENDENCIES_CHECKS_FILE,
        _ => STD_CHECKS_FILE,
    };
    directory.join(key.directory_name()).join(file)
}

/// Whether a file exists under this key's name (not whether it would load).
#[cfg(test)]
pub(crate) fn exists(directory: &Path, key: &Key) -> bool {
    tier_path(directory, key).is_file()
}

/// What a write produced.
pub(crate) struct Written {
    pub path: PathBuf,
    pub bytes: usize,
    pub took: Duration,
}

/// Writes the std tier for `key`: a temporary beside the target and a rename,
/// so a reader never sees a half-written file. `body` and `table` are the
/// codec's two blocks; `std_roots` the canonical roots the body's paths are
/// rooted at.
pub(crate) fn write(
    directory: &Path,
    key: &Key,
    std_roots: &[PathBuf],
    body: &[u8],
    table: &[u8],
) -> Result<Written, String> {
    write_file(tier_path(directory, key), key, std_roots, body, table)
}

/// M158: writes the checks record for `key` beside its world, framed as the
/// world is.
pub(crate) fn write_checks(
    directory: &Path,
    key: &Key,
    std_roots: &[PathBuf],
    body: &[u8],
    table: &[u8],
) -> Result<Written, String> {
    write_file(checks_path(directory, key), key, std_roots, body, table)
}

fn write_file(
    path: PathBuf,
    key: &Key,
    std_roots: &[PathBuf],
    body: &[u8],
    table: &[u8],
) -> Result<Written, String> {
    let started = now();
    let parent = path
        .parent()
        .expect("the tier file sits in the key's directory");
    std::fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    let mut checksum_input = Vec::with_capacity(body.len() + table.len());
    checksum_input.extend_from_slice(body);
    checksum_input.extend_from_slice(table);
    let header = Header {
        key: key.clone(),
        std_roots: std_roots.to_vec(),
        body_len: body.len() as u64,
        table_len: table.len() as u64,
        checksum: crate::content_hash_bytes(&checksum_input),
    };
    let header_bytes = codec::encode_inline(&header).map_err(|error| error.to_string())?;
    let mut file = Vec::with_capacity(MAGIC.len() + header_bytes.len() + checksum_input.len());
    file.extend_from_slice(MAGIC);
    file.extend_from_slice(&header_bytes);
    file.extend_from_slice(&checksum_input);
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temporary = parent.join(format!(
        ".{file_name}.{}-{:?}.tmp",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::write(&temporary, &file)
        .map_err(|error| format!("{}: {error}", temporary.display()))?;
    if let Err(error) = std::fs::rename(&temporary, &path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("{}: {error}", path.display()));
    }
    Ok(Written {
        path,
        bytes: file.len(),
        took: Duration::from_micros(micros_since(started)),
    })
}

/// P4: the key a cache directory's world was written under, read off its
/// file's header alone (the world itself is never decoded) — `None` when the
/// directory holds no tier file or its header does not read.
pub(crate) fn directory_key(directory: &Path) -> Option<Key> {
    use std::io::Read as _;
    for file in [STD_TIER_FILE, DEPENDENCIES_TIER_FILE] {
        let Ok(handle) = std::fs::File::open(directory.join(file)) else {
            continue;
        };
        // The header is the key, the roots and three numbers: a few KB at most.
        let mut head = Vec::new();
        if handle.take(64 * 1024).read_to_end(&mut head).is_err() {
            return None;
        }
        if head.len() < MAGIC.len() || &head[..MAGIC.len()] != MAGIC {
            return None;
        }
        let (header, _): (Header, usize) = codec::decode_inline(&head[MAGIC.len()..]).ok()?;
        return Some(header.key);
    }
    None
}

/// Reads and validates the std tier for `key`. Every failure is a [`Miss`].
pub(crate) fn read(directory: &Path, key: &Key) -> Result<Loaded, Miss> {
    read_file(tier_path(directory, key), key)
}

/// M158: reads and validates the checks record for `key`. Every failure is a
/// [`Miss`] of the record alone.
pub(crate) fn read_checks(directory: &Path, key: &Key) -> Result<Loaded, Miss> {
    read_file(checks_path(directory, key), key)
}

fn read_file(path: PathBuf, key: &Key) -> Result<Loaded, Miss> {
    let started = now();
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Err(Miss::Absent),
        Err(_) => return Err(Miss::Unreadable),
    };
    let validated = validate(&bytes, key)?;
    let (body_start, body_end, table_end, std_roots) = validated;
    // P4: a load is a USE — the write-time bound takes the least recently
    // used world first. Best effort: a read-only root keeps its write time.
    if let Ok(handle) = std::fs::File::options().append(true).open(&path) {
        let _ = handle.set_modified(std::time::SystemTime::now());
    }
    let leaked: &'static [u8] = Box::leak(bytes.into_boxed_slice());
    crate::leak_tally::record(crate::leak_tally::LeakSite::PrefixFile, leaked.len());
    Ok(Loaded {
        body: &leaked[body_start..body_end],
        table: &leaked[body_end..table_end],
        std_roots,
        bytes: leaked.len(),
        read: Duration::from_micros(micros_since(started)),
    })
}

/// The header check: the magic, the format, the key, the lengths, the
/// checksum. Returns the body's and the table's bounds and the stored roots.
fn validate(bytes: &[u8], key: &Key) -> Result<(usize, usize, usize, Vec<PathBuf>), Miss> {
    if bytes.len() < MAGIC.len() || &bytes[..MAGIC.len()] != MAGIC {
        return Err(Miss::Corrupt);
    }
    let (header, used): (Header, usize) =
        codec::decode_inline(&bytes[MAGIC.len()..]).map_err(|_| Miss::Corrupt)?;
    if header.key.format != key.format {
        return Err(Miss::ForeignVersion);
    }
    if header.key != *key {
        return Err(Miss::ForeignKey);
    }
    let body_start = MAGIC.len() + used;
    let body_end = body_start
        .checked_add(usize::try_from(header.body_len).map_err(|_| Miss::Corrupt)?)
        .ok_or(Miss::Corrupt)?;
    let table_end = body_end
        .checked_add(usize::try_from(header.table_len).map_err(|_| Miss::Corrupt)?)
        .ok_or(Miss::Corrupt)?;
    if table_end > bytes.len() {
        return Err(Miss::Truncated);
    }
    if table_end != bytes.len() {
        return Err(Miss::Corrupt);
    }
    if crate::content_hash_bytes(&bytes[body_start..table_end]) != header.checksum {
        return Err(Miss::Corrupt);
    }
    Ok((body_start, body_end, table_end, header.std_roots))
}

/// A path mapping from one set of std roots to another, by position: a path
/// under `from[i]` becomes the same path under `to[i]`. Longest root first,
/// so a layer nested under the base root maps to the layer. A path under none
/// of them is left as it is (the entry's slot, a package path a prefix never
/// holds).
pub(crate) fn reroot(from: &[PathBuf], to: &[PathBuf]) -> impl Fn(&Path) -> Option<PathBuf> {
    debug_assert_eq!(from.len(), to.len(), "std roots map by position");
    let mut pairs: Vec<(PathBuf, PathBuf)> = from.iter().cloned().zip(to.iter().cloned()).collect();
    pairs.sort_by_key(|(from, _)| std::cmp::Reverse(from.as_os_str().len()));
    move |path: &Path| {
        pairs.iter().find_map(|(from, to)| {
            path.strip_prefix(from)
                .ok()
                .map(|relative| to.join(relative))
        })
    }
}

/// `Analyzer::span_map`'s rows, written by value (`std-prefix.md` §2 row 5)
/// and in id order, so the bytes are a function of the map's content.
pub(crate) fn serialize_span_refs<S: serde::Serializer>(
    map: &HashMap<Id, &Span>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut rows: Vec<(Id, Span)> = map.iter().map(|(id, span)| (*id, **span)).collect();
    rows.sort_unstable_by_key(|(id, _)| id.0);
    serde::Serialize::serialize(&rows, serializer)
}

/// The rows back as references: one leaked slice per load (eight bytes a
/// row — the paper's "by value, 8 bytes either way" — counted by the leak
/// tally), so the field keeps its `&'src Span` shape and the 228 sites that
/// read it are untouched.
pub(crate) fn deserialize_span_refs<'de, 'src, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<HashMap<Id, &'src Span>, D::Error> {
    let rows: Vec<(Id, Span)> = serde::Deserialize::deserialize(deserializer)?;
    crate::leak_tally::record(
        crate::leak_tally::LeakSite::PrefixFile,
        rows.len() * std::mem::size_of::<(Id, Span)>(),
    );
    let leaked: &'static [(Id, Span)] = Box::leak(rows.into_boxed_slice());
    Ok(leaked.iter().map(|(id, span)| (*id, span)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> Key {
        Key {
            format: FORMAT_VERSION,
            build_id: "deadbeefdeadbeef".to_string(),
            platform: "Node { version: 24 }".to_string(),
            closure: vec!["io".to_string(), "path".to_string()],
            std_content: 7,
            macro_limits: (1_000_000, 64),
            entry_prelude: None,
            workspace: Vec::new(),
            tiers: vec!["std".to_string()],
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "vilan-prefix-file-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        directory
    }

    /// M152: a key that names the dependency tier is its own file —
    /// `dependencies.world` in its own directory, beside a std-only key's
    /// `std.world` — and neither reads the other's bytes.
    #[test]
    fn m152_a_dependency_tier_is_its_own_file_and_never_the_std_tiers() {
        let directory = scratch("tiers");
        let std_only = key();
        let mut with_dependencies = key();
        with_dependencies.workspace = vec![
            "package 0 root=/shapes surface=true member=false prelude=false layers=[] deps=[]"
                .to_string(),
            "request 0::geometry".to_string(),
            "content 0123456789abcdef".to_string(),
        ];
        with_dependencies.tiers = vec![STD_TIER.to_string(), DEPENDENCIES_TIER.to_string()];
        assert!(tier_path(&directory, &std_only).ends_with(STD_TIER_FILE));
        assert!(tier_path(&directory, &with_dependencies).ends_with(DEPENDENCIES_TIER_FILE));
        assert_ne!(
            std_only.directory_name(),
            with_dependencies.directory_name()
        );
        let roots = vec![PathBuf::from("/std")];
        write(&directory, &std_only, &roots, b"std body", b"").expect("write std");
        assert_eq!(
            read(&directory, &with_dependencies).err(),
            Some(Miss::Absent)
        );
        write(&directory, &with_dependencies, &roots, b"both", b"").expect("write both");
        assert_eq!(read(&directory, &std_only).expect("std").body, b"std body");
        assert_eq!(
            read(&directory, &with_dependencies).expect("both").body,
            b"both"
        );
        // A content row that moved is another key.
        let mut edited = with_dependencies.clone();
        edited.workspace[2] = "content fedcba9876543210".to_string();
        assert_eq!(read(&directory, &edited).err(), Some(Miss::Absent));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_written_file_reads_back_and_every_damage_is_a_miss() {
        let directory = scratch("roundtrip");
        let key = key();
        let roots = vec![PathBuf::from("/std")];
        let body = b"the body".to_vec();
        let table = b"\x01\x03abc".to_vec();
        assert!(!exists(&directory, &key));
        assert_eq!(read(&directory, &key).err(), Some(Miss::Absent));
        let written = write(&directory, &key, &roots, &body, &table).expect("write");
        assert!(exists(&directory, &key));
        assert_eq!(written.path, tier_path(&directory, &key));
        let loaded = read(&directory, &key).expect("read");
        assert_eq!(loaded.body, &body[..]);
        assert_eq!(loaded.table, &table[..]);
        assert_eq!(loaded.std_roots, roots);
        assert_eq!(loaded.bytes, written.bytes);
        // A foreign version.
        let mut other = key.clone();
        other.format += 1;
        write(&directory, &other, &roots, &body, &table).expect("write");
        std::fs::copy(tier_path(&directory, &other), tier_path(&directory, &key)).expect("copy");
        assert_eq!(read(&directory, &key).err(), Some(Miss::ForeignVersion));
        // A foreign key under this key's name: another build, another std.
        for change in [
            |key: &mut Key| key.build_id = "0000000000000000".to_string(),
            |key: &mut Key| key.std_content += 1,
            |key: &mut Key| key.closure.push("time".to_string()),
            |key: &mut Key| key.platform = "Browser".to_string(),
        ] {
            let mut other = key.clone();
            change(&mut other);
            write(&directory, &other, &roots, &body, &table).expect("write");
            std::fs::copy(tier_path(&directory, &other), tier_path(&directory, &key))
                .expect("copy");
            assert_eq!(read(&directory, &key).err(), Some(Miss::ForeignKey));
        }
        // Truncated, corrupt, not a prefix file at all.
        write(&directory, &key, &roots, &body, &table).expect("write");
        let whole = std::fs::read(tier_path(&directory, &key)).expect("read bytes");
        std::fs::write(tier_path(&directory, &key), &whole[..whole.len() - 3]).expect("truncate");
        assert_eq!(read(&directory, &key).err(), Some(Miss::Truncated));
        let mut flipped = whole.clone();
        let last = flipped.len() - 1;
        flipped[last] ^= 0xff;
        std::fs::write(tier_path(&directory, &key), &flipped).expect("corrupt");
        assert_eq!(read(&directory, &key).err(), Some(Miss::Corrupt));
        std::fs::write(tier_path(&directory, &key), b"not a prefix").expect("garbage");
        assert_eq!(read(&directory, &key).err(), Some(Miss::Corrupt));
        std::fs::write(tier_path(&directory, &key), b"").expect("empty");
        assert_eq!(read(&directory, &key).err(), Some(Miss::Corrupt));
        // And the good file again, by name: no temporary left beside it.
        write(&directory, &key, &roots, &body, &table).expect("write");
        assert!(read(&directory, &key).is_ok());
        let leftovers: Vec<_> = std::fs::read_dir(tier_path(&directory, &key).parent().unwrap())
            .expect("list")
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn the_directory_name_is_a_function_of_the_key() {
        let a = key();
        let mut b = key();
        assert_eq!(a.directory_name(), b.directory_name());
        b.closure.push("time".to_string());
        assert_ne!(a.directory_name(), b.directory_name());
        assert_eq!(a.directory_name().len(), 16);
    }

    #[test]
    fn rerooting_maps_the_longest_root_and_leaves_the_rest() {
        let from = vec![PathBuf::from("/a/std"), PathBuf::from("/a/std/layers/web")];
        let to = vec![PathBuf::from("/b/std"), PathBuf::from("/b/std/layers/web")];
        let map = reroot(&from, &to);
        assert_eq!(
            map(Path::new("/a/std/io.vl")),
            Some(PathBuf::from("/b/std/io.vl"))
        );
        assert_eq!(
            map(Path::new("/a/std/layers/web/ui.vl")),
            Some(PathBuf::from("/b/std/layers/web/ui.vl"))
        );
        assert_eq!(map(Path::new("/elsewhere/main.vl")), None);
        assert_eq!(map(Path::new("")), None);
    }
}
