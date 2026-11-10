//! `vilan-rt::fs` — the plain half of what `std::fs` binds (tracker F18
//! slice 2).
//!
//! # What this is a twin OF
//!
//! `vilan/std/src/fs.vl` binds `node:fs/promises`. The bindings that
//! take no options object are here — read, write, append, copy, rename,
//! remove, list, make and remove a directory — because each is one call into
//! Rust's own `std::fs` and because `serve_build`'s dev-mode freshness read
//! (`asset_body` under `run --watch`) is on the path `Server::builder()`
//! reaches.
//!
//! The `FsOptions` forms (`mkdir { recursive }`, `rm { recursive, force }`,
//! `cp { recursive }`, `readdir { withFileTypes }`) joined them with F156: the
//! options object is [`FsOptions`], three shared flags behind the setters
//! `std::fs` calls, and each `_with` call reads the flags it has a meaning
//! for, as the host call does. Their results and their failures are node's:
//! the cases are recorded at each function, measured against node v24.2.0.
//!
//! # The divergence, written down
//!
//! node's `fs/promises` calls are genuinely asynchronous — the work happens on
//! its thread pool and the loop keeps turning. These are BLOCKING reads and
//! writes inside an `async fn`, so a slow file stalls the executor's turn
//! instead of overlapping with it. Making them genuinely async needs either a
//! thread pool (this runtime is single-threaded by design, and `Rc` is
//! everywhere) or `io_uring` (a dependency and `unsafe`), so the honest answer
//! for now is the blocking one with the cost named here. Nothing about
//! ORDERING changes: an `await` on one of these still suspends and resumes at
//! the points J6's model says it does.
//!
//! # Failure
//!
//! A rejected `fs/promises` call throws on the JS backend, and a throw that
//! nothing catches ends the program with its message. So a failure here is
//! [`crate::panic_with`] carrying node's own `code: message, syscall 'path'`
//! shape, which `main_guard` then reports the way node reports a rejection.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::bytes::Bytes;
use crate::{Str, str_new};

/// node's message shape for a failed call, so a native failure reads like the
/// JS one: `ENOENT: no such file or directory, open '/x'`.
fn fail(syscall: &str, path: &str, error: &std::io::Error) -> ! {
    let code = match error.kind() {
        std::io::ErrorKind::NotFound => "ENOENT",
        std::io::ErrorKind::PermissionDenied => "EACCES",
        std::io::ErrorKind::AlreadyExists => "EEXIST",
        std::io::ErrorKind::InvalidInput => "EINVAL",
        std::io::ErrorKind::NotADirectory => "ENOTDIR",
        std::io::ErrorKind::IsADirectory => "EISDIR",
        std::io::ErrorKind::DirectoryNotEmpty => "ENOTEMPTY",
        _ => "EIO",
    };
    crate::panic_with(&format!("{code}: {error}, {syscall} '{path}'"))
}

/// `readFile(path)` with no encoding — a node `Buffer`, which IS a
/// `Uint8Array`, so it binds straight to `Bytes`.
pub async fn read_bytes(path: Str) -> Bytes {
    match std::fs::read(Path::new(&*path)) {
        Ok(bytes) => Bytes::from_vec(bytes),
        Err(error) => fail("open", &path, &error),
    }
}

/// `readFile(path, encoding)`. The encoding is accepted and only `"utf8"` is
/// supported, which is the only one `std::fs` ever passes: a vilan `str` is
/// UTF-8 and there is nothing else to decode into.
pub async fn read_text(path: Str, encoding: Str) -> Str {
    if !matches!(&*encoding, "utf8" | "utf-8" | "UTF-8") {
        crate::panic_with(&format!(
            "the native backend reads a file as utf8; `{encoding}` is not a decoding it has"
        ));
    }
    match std::fs::read_to_string(Path::new(&*path)) {
        Ok(text) => str_new(&text),
        Err(error) => fail("open", &path, &error),
    }
}

/// `writeFile(path, contents)` — creates or truncates.
pub async fn write_text(path: Str, contents: Str) {
    if let Err(error) = std::fs::write(Path::new(&*path), contents.as_bytes()) {
        fail("open", &path, &error);
    }
}

/// `writeFile(path, bytes)`.
pub async fn write_bytes(path: Str, contents: Bytes) {
    if let Err(error) = std::fs::write(Path::new(&*path), &*contents.as_slice()) {
        fail("open", &path, &error);
    }
}

/// node's `fs.Stats`, reduced to the three fields `std::fs` reads off it —
/// `std::fs`'s `external struct RawStat` (F18 slice 3: `require_build` probes
/// the build manifest with `stat` before it reads it, which puts this on
/// `Server::builder().serve_build(..)`'s boot path).
#[derive(Clone, Debug, PartialEq)]
pub struct RawStat {
    size: i32,
    mtime_ms: f64,
    is_directory: bool,
}

impl RawStat {
    /// `stats.size`. A file past `i32::MAX` bytes saturates, where the host's
    /// double would carry it: `std::fs` declares the field `i32`.
    pub fn size(&self) -> i32 {
        self.size
    }

    /// `stats.mtimeMs` — milliseconds since the epoch, FRACTIONAL, as node's
    /// plain (non-`bigint`) stat answers it.
    pub fn mtime_ms(&self) -> f64 {
        self.mtime_ms
    }

    /// `stats.isDirectory()`.
    pub fn is_directory(&self) -> bool {
        self.is_directory
    }
}

impl crate::Js for RawStat {
    fn js(&self) -> String {
        crate::panic_with(
            "printing an `fs.Stats` is a host object's own inspection, which the native backend \
             does not reproduce",
        )
    }
}

/// `__fs_stat(path)` — `stat`, with a missing path answered `None` rather
/// than thrown (`transformer.rs`'s helper of the same name catches `ENOENT`
/// and nothing else), and every other failure thrown as node throws it.
/// Symlinks are FOLLOWED, as `fs.promises.stat` follows them.
pub async fn stat(path: Str) -> Option<RawStat> {
    match std::fs::metadata(Path::new(&*path)) {
        Ok(metadata) => {
            let mtime_ms = metadata
                .modified()
                .ok()
                .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|since| since.as_secs_f64() * 1000.0)
                .unwrap_or(0.0);
            Some(RawStat {
                size: i32::try_from(metadata.len()).unwrap_or(i32::MAX),
                mtime_ms,
                is_directory: metadata.is_dir(),
            })
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => fail("stat", &path, &error),
    }
}

/// `appendFile(path, contents)` — creates the file if it is not there.
pub async fn append(path: Str, contents: Str) {
    use std::io::Write as _;
    let opened = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(Path::new(&*path));
    match opened {
        Ok(mut file) => {
            if let Err(error) = file.write_all(contents.as_bytes()) {
                fail("write", &path, &error);
            }
        }
        Err(error) => fail("open", &path, &error),
    }
}

/// `copyFile(from, to)`.
pub async fn copy(from: Str, to: Str) {
    if let Err(error) = std::fs::copy(Path::new(&*from), Path::new(&*to)) {
        fail("copyfile", &from, &error);
    }
}

/// `rename(from, to)`.
pub async fn rename(from: Str, to: Str) {
    if let Err(error) = std::fs::rename(Path::new(&*from), Path::new(&*to)) {
        fail("rename", &from, &error);
    }
}

/// `unlink(path)`.
pub async fn remove(path: Str) {
    if let Err(error) = std::fs::remove_file(Path::new(&*path)) {
        fail("unlink", &path, &error);
    }
}

/// `readdir(path)` — the entry NAMES, which is what node answers without
/// `withFileTypes`. Sorted, because node's order is the directory's and a
/// program that prints it on both backends has to see one answer.
pub async fn read_dir(path: Str) -> Vec<Str> {
    let read = match std::fs::read_dir(Path::new(&*path)) {
        Ok(read) => read,
        Err(error) => fail("scandir", &path, &error),
    };
    let mut names: Vec<Str> = read
        .flatten()
        .map(|entry| str_new(&entry.file_name().to_string_lossy()))
        .collect();
    names.sort();
    names
}

/// `mkdir(path)` — one level, and an existing directory is an error, which is
/// node's behaviour without `{ recursive: true }`.
pub async fn create_dir(path: Str) {
    if let Err(error) = std::fs::create_dir(Path::new(&*path)) {
        fail("mkdir", &path, &error);
    }
}

/// `rmdir(path)` — an empty directory only, as node's is.
pub async fn remove_dir(path: Str) {
    if let Err(error) = std::fs::remove_dir(Path::new(&*path)) {
        fail("rmdir", &path, &error);
    }
}

/// The host options object `std::fs` builds with `Object()` and fills with
/// setters (`std::fs`'s `external struct FsOptions`, F156).
///
/// The flags are SHARED, as the host `{}` is: the setters take `self`, so a
/// rule-1 copy of the value must still be the object they write into.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FsOptions(Rc<OptionFlags>);

#[derive(Debug, Default, PartialEq)]
struct OptionFlags {
    recursive: Cell<bool>,
    force: Cell<bool>,
    with_file_types: Cell<bool>,
}

impl FsOptions {
    /// `Object()` — a fresh `{}`: every flag absent, which each host call
    /// reads as `false`.
    pub fn new() -> FsOptions {
        FsOptions::default()
    }

    /// `options.recursive = value`.
    pub fn set_recursive(&self, value: bool) {
        self.0.recursive.set(value);
    }

    /// `options.force = value`.
    pub fn set_force(&self, value: bool) {
        self.0.force.set(value);
    }

    /// `options.withFileTypes = value`.
    pub fn set_with_file_types(&self, value: bool) {
        self.0.with_file_types.set(value);
    }
}

impl crate::Js for FsOptions {
    fn js(&self) -> String {
        crate::panic_with(
            "printing an fs options object is a host object's own inspection, which the native \
             backend does not reproduce",
        )
    }
}

/// `mkdir(path, options)`. Without `recursive` it is [`create_dir`]. With it,
/// `mkdir -p`: every missing directory is made and an existing DIRECTORY is
/// success, while an existing file at `path` is `EEXIST` and a file on the
/// way is `ENOTDIR` — node's three answers.
pub async fn mkdir_with(path: Str, options: FsOptions) {
    if !options.0.recursive.get() {
        return create_dir(path).await;
    }
    // `create_dir_all` answers an existing directory with success and an
    // existing file with the OS's `EEXIST`, which is node's split.
    let target = Path::new(&*path);
    if let Err(error) = std::fs::create_dir_all(target) {
        // A FILE above the path is node's `ENOTDIR` on every OS. Unix says so
        // itself; Windows reports the file as the ancestor that "already
        // exists" (os error 183), which would print `EEXIST`.
        if blocked_by_a_file(target) {
            let error = std::io::Error::from(std::io::ErrorKind::NotADirectory);
            fail("mkdir", &path, &error);
        }
        fail("mkdir", &path, &error);
    }
}

/// Whether the nearest existing ancestor of `path` (the path itself left out)
/// is something other than a directory — what makes a recursive `mkdir` fail
/// with node's `ENOTDIR`. A link is followed, as `mkdir` follows it.
fn blocked_by_a_file(path: &Path) -> bool {
    path.ancestors()
        .skip(1)
        .filter(|ancestor| !ancestor.as_os_str().is_empty())
        .find_map(|ancestor| std::fs::metadata(ancestor).ok())
        .is_some_and(|metadata| !metadata.is_dir())
}

/// `rm(path, options)`. A directory needs `recursive` (`EISDIR`-shaped
/// refusal without it, as node's `ERR_FS_EISDIR`); a missing path is a no-op
/// under `force` and `ENOENT` without it. A symlink is removed ITSELF, never
/// what it points at — including a link to a directory.
pub async fn rm_with(path: Str, options: FsOptions) {
    let target = Path::new(&*path);
    let metadata = match std::fs::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && options.0.force.get() => {
            return;
        }
        Err(error) => fail("lstat", &path, &error),
    };
    let removed = if metadata.is_dir() {
        if !options.0.recursive.get() {
            let error = std::io::Error::from(std::io::ErrorKind::IsADirectory);
            fail("rm", &path, &error);
        }
        std::fs::remove_dir_all(target)
    } else {
        std::fs::remove_file(target)
    };
    match removed {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && options.0.force.get() => {}
        Err(error) => fail("rm", &path, &error),
    }
}

/// `cp(from, to, options)` — `std::fs::copy_dir`'s host call.
///
/// node's answers, case by case: a missing `from` is `ENOENT` (`lstat`); a
/// directory needs `recursive`; copying a directory into itself is refused;
/// a directory onto a file and a file onto a directory are refused; the
/// destination's missing parents are made; a file already there is
/// OVERWRITTEN and an entry with no counterpart under `from` is left alone (a
/// merge, not a mirror). A SYMLINK is copied as a link, and its target is
/// resolved against the source link's directory first — node's default
/// (`verbatimSymlinks: false`) — so a relative link still names the same file
/// from its new place. Directory entries are copied in name order.
pub async fn cp_with(from: Str, to: Str, options: FsOptions) {
    let source = Path::new(&*from);
    let destination = Path::new(&*to);
    let metadata = match std::fs::symlink_metadata(source) {
        Ok(metadata) => metadata,
        Err(error) => fail("lstat", &from, &error),
    };
    if metadata.is_dir() {
        if !options.0.recursive.get() {
            crate::panic_with(&format!(
                "Recursive option is required to copy a directory: cp returned EISDIR ({from} is \
                 a directory (not copied)) {from}"
            ));
        }
        if destination.exists() && !destination.is_dir() {
            crate::panic_with(&format!(
                "Cannot overwrite non-directory with directory: cp returned EISDIR (cannot \
                 overwrite non-directory {to} with directory {from}) {to}"
            ));
        }
        if is_inside(source, destination) {
            crate::panic_with(&format!(
                "Invalid src or dest: cp returned EINVAL (cannot copy {from} to a subdirectory \
                 of self {to}) {to}"
            ));
        }
    } else if destination.is_dir() {
        crate::panic_with(&format!(
            "Cannot overwrite directory with non-directory: cp returned ENOTDIR (cannot \
             overwrite directory {to} with non-directory {from}) {to}"
        ));
    }
    if let Some(parent) = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        fail("mkdir", &parent.to_string_lossy(), &error);
    }
    copy_entry(source, destination);
}

/// Whether `inner` is `outer` or under it, by their absolute forms — the
/// self-copy refusal's test. `inner` need not exist yet.
fn is_inside(outer: &Path, inner: &Path) -> bool {
    let absolute = |path: &Path| -> PathBuf {
        std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
    };
    let outer = std::fs::canonicalize(outer).unwrap_or_else(|_| absolute(outer));
    let inner = absolute(inner);
    let inner = match (inner.parent(), inner.file_name()) {
        (Some(parent), Some(name)) => std::fs::canonicalize(parent)
            .map(|parent| parent.join(name))
            .unwrap_or(inner),
        _ => inner,
    };
    inner.starts_with(&outer)
}

/// One step of [`cp_with`]'s walk: `source` (a file, a link or a directory,
/// by its own `lstat`) to `destination`.
fn copy_entry(source: &Path, destination: &Path) {
    let shown = |path: &Path| path.to_string_lossy().into_owned();
    let metadata = match std::fs::symlink_metadata(source) {
        Ok(metadata) => metadata,
        Err(error) => fail("lstat", &shown(source), &error),
    };
    let kind = metadata.file_type();
    if kind.is_dir() {
        if !destination.is_dir()
            && let Err(error) = std::fs::create_dir(destination)
        {
            fail("mkdir", &shown(destination), &error);
        }
        let mut names: Vec<std::ffi::OsString> = match std::fs::read_dir(source) {
            Ok(read) => read.flatten().map(|entry| entry.file_name()).collect(),
            Err(error) => fail("opendir", &shown(source), &error),
        };
        names.sort();
        for name in names {
            copy_entry(&source.join(&name), &destination.join(&name));
        }
    } else if kind.is_symlink() {
        let link = match std::fs::read_link(source) {
            Ok(link) => link,
            Err(error) => fail("readlink", &shown(source), &error),
        };
        let resolved = if link.is_absolute() {
            link
        } else {
            let base = source.parent().unwrap_or(Path::new(""));
            std::path::absolute(base.join(&link)).unwrap_or_else(|_| base.join(&link))
        };
        if std::fs::symlink_metadata(destination).is_ok()
            && let Err(error) = std::fs::remove_file(destination)
        {
            fail("unlink", &shown(destination), &error);
        }
        if let Err(error) = make_symlink(&resolved, destination) {
            fail("symlink", &shown(destination), &error);
        }
    } else if let Err(error) = std::fs::copy(source, destination) {
        fail("copyfile", &shown(source), &error);
    }
}

#[cfg(unix)]
fn make_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn make_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    if target.is_dir() {
        std::os::windows::fs::symlink_dir(target, link)
    } else {
        std::os::windows::fs::symlink_file(target, link)
    }
}

/// One `readdir { withFileTypes: true }` entry — `std::fs`'s `external struct
/// RawEntry`, a host `Dirent`. The kinds are the entry's OWN (`lstat`), as a
/// dirent's are: a symlink to a directory is a symlink and not a directory.
#[derive(Clone, Debug, PartialEq)]
pub struct RawEntry {
    name: Str,
    is_directory: bool,
    is_file: bool,
    is_symlink: bool,
}

impl RawEntry {
    /// `dirent.name`.
    pub fn name(&self) -> Str {
        Rc::clone(&self.name)
    }

    /// `dirent.isDirectory()`.
    pub fn is_directory(&self) -> bool {
        self.is_directory
    }

    /// `dirent.isFile()`.
    pub fn is_file(&self) -> bool {
        self.is_file
    }

    /// `dirent.isSymbolicLink()`.
    pub fn is_symlink(&self) -> bool {
        self.is_symlink
    }
}

impl crate::Js for RawEntry {
    fn js(&self) -> String {
        crate::panic_with(
            "printing a `Dirent` is a host object's own inspection, which the native backend \
             does not reproduce",
        )
    }
}

/// `readdir(path, options)` — `std::fs::scan_dir`'s host call, which always
/// passes `withFileTypes`: without it node answers the names alone, which is
/// [`read_dir`] and a different declared type, so this refuses it rather than
/// answer `Dirent`s the host would not have. Sorted by name, for
/// [`read_dir`]'s reason. A missing path is `ENOENT` and a file is `ENOTDIR`,
/// node's two.
pub async fn scan_dir(path: Str, options: FsOptions) -> Vec<RawEntry> {
    if !options.0.with_file_types.get() {
        crate::panic_with(
            "the native `readdir` with options answers `Dirent`s only (`withFileTypes`)",
        );
    }
    let read = match std::fs::read_dir(Path::new(&*path)) {
        Ok(read) => read,
        Err(error) => fail("scandir", &path, &error),
    };
    let mut entries: Vec<RawEntry> = read
        .flatten()
        .map(|entry| {
            let kind = entry.file_type().ok();
            RawEntry {
                name: str_new(&entry.file_name().to_string_lossy()),
                is_directory: kind.is_some_and(|kind| kind.is_dir()),
                is_file: kind.is_some_and(|kind| kind.is_file()),
                is_symlink: kind.is_some_and(|kind| kind.is_symlink()),
            }
        })
        .collect();
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    entries
}

/// `__fs_read_dir_all(path)` — `std::fs::read_dir_all`: every path under
/// `path`, relative to it and joined with the host separator, as node's
/// `readdir { recursive: true }` answers them. Sorted, for [`read_dir`]'s
/// reason; a symlinked directory is listed and not entered, as node does
/// not enter one.
pub async fn read_dir_all(path: Str) -> Vec<Str> {
    let root = Path::new(&*path);
    let mut found = Vec::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        let read = match std::fs::read_dir(root.join(&relative)) {
            Ok(read) => read,
            Err(error) => fail("scandir", &root.join(&relative).to_string_lossy(), &error),
        };
        for entry in read.flatten() {
            let child = relative.join(entry.file_name());
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                pending.push(child.clone());
            }
            found.push(str_new(&child.to_string_lossy()));
        }
    }
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::block_on;

    fn scratch(name: &str) -> std::path::PathBuf {
        let directory =
            std::env::temp_dir().join(format!("vilan-rt-fs-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("create the scratch directory");
        directory
    }

    /// `stat` answers the three fields for a file and a directory, and `None`
    /// — not a throw — for a path that is not there, which is the whole of
    /// `__fs_stat`'s contract.
    #[test]
    fn stat_reads_a_file_and_a_directory_and_answers_none_for_a_missing_path() {
        let directory = std::env::temp_dir().join(format!("vilan-rt-stat-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("create the directory");
        let file = directory.join("five.txt");
        std::fs::write(&file, b"12345").expect("write the file");
        let probed = directory.clone();
        crate::executor::block_on(async move {
            let directory = probed;
            let file_stat = stat(str_new(&file.to_string_lossy()))
                .await
                .expect("a file");
            assert_eq!(file_stat.size(), 5);
            assert!(!file_stat.is_directory());
            assert!(file_stat.mtime_ms() > 1_000_000_000_000.0);
            let directory_stat = stat(str_new(&directory.to_string_lossy()))
                .await
                .expect("a directory");
            assert!(directory_stat.is_directory());
            assert!(
                stat(str_new(&directory.join("absent").to_string_lossy()))
                    .await
                    .is_none()
            );
        });
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// Runs `body` and answers the sentence it panicked with — a failed host
    /// call's message, which is what a vilan program would die printing.
    fn failure_of<T: Clone + std::fmt::Debug + 'static>(
        body: impl std::future::Future<Output = T> + 'static,
    ) -> String {
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| block_on(body)));
        let payload = caught.expect_err("the call should have failed");
        payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|text| text.to_string()))
            .unwrap_or_default()
    }

    fn options(recursive: bool, force: bool) -> FsOptions {
        let options = FsOptions::new();
        options.set_recursive(recursive);
        options.set_force(force);
        options
    }

    fn text(path: &std::path::Path) -> Str {
        str_new(&path.to_string_lossy())
    }

    /// F156: `mkdir { recursive }` is `mkdir -p` — idempotent over an
    /// existing directory — and node's `EEXIST` over a file and `ENOTDIR`
    /// through one; without the flag it is the one-level, strict `mkdir`.
    #[test]
    fn mkdir_with_recursive_makes_the_chain_and_fails_where_node_does() {
        let directory = scratch("mkdir-with");
        let deep = directory.join("a/b/c");
        let file = directory.join("file");
        std::fs::write(&file, b"x").expect("write the file");
        let (deep_path, file_path) = (text(&deep), text(&file));
        block_on(async move {
            mkdir_with(deep_path.clone(), options(true, false)).await;
            mkdir_with(deep_path, options(true, false)).await;
        });
        assert!(deep.is_dir());
        let over_file = failure_of(mkdir_with(file_path.clone(), options(true, false)));
        assert!(over_file.starts_with("EEXIST:"), "{over_file}");
        let through_file = failure_of(mkdir_with(text(&file.join("below")), options(true, false)));
        assert!(through_file.starts_with("ENOTDIR:"), "{through_file}");
        let strict = failure_of(mkdir_with(text(&deep), options(false, false)));
        assert!(strict.starts_with("EEXIST:"), "{strict}");
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// F156: `rm { recursive, force }` takes a tree and a file, is a no-op
    /// over a missing path, and removes a symlink ITSELF — never the
    /// directory it points at. Without `force` a missing path is `ENOENT`.
    #[cfg(unix)]
    #[test]
    fn rm_with_removes_trees_files_and_links_and_forgives_a_missing_path() {
        let directory = scratch("rm-with");
        let tree = directory.join("tree");
        std::fs::create_dir_all(tree.join("inner")).expect("make the tree");
        std::fs::write(tree.join("inner/f"), b"x").expect("write into the tree");
        let file = directory.join("file");
        std::fs::write(&file, b"x").expect("write the file");
        let link = directory.join("link");
        std::os::unix::fs::symlink(&tree, &link).expect("link the tree");
        let (tree_path, file_path, link_path) = (text(&tree), text(&file), text(&link));
        let missing = text(&directory.join("missing"));
        let missing_again = missing.clone();
        block_on(async move {
            rm_with(link_path, options(true, true)).await;
            rm_with(missing, options(true, true)).await;
        });
        assert!(
            !link.exists() && tree.join("inner/f").is_file(),
            "the link went, not the tree"
        );
        block_on(async move {
            rm_with(tree_path, options(true, true)).await;
            rm_with(file_path, options(true, true)).await;
        });
        assert!(!tree.exists() && !file.exists());
        let strict = failure_of(rm_with(missing_again, options(true, false)));
        assert!(strict.starts_with("ENOENT:"), "{strict}");
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// F156: `cp { recursive }` merges a tree into an existing one (a file
    /// there is overwritten, one with no counterpart is kept), makes a fresh
    /// destination's parents, copies a symlink as a link to the source's
    /// target made absolute (node's `verbatimSymlinks: false`), and refuses
    /// what node refuses: a missing source, a copy into itself, a directory
    /// onto a file, and a file onto a directory.
    #[cfg(unix)]
    #[test]
    fn cp_with_merges_trees_copies_links_and_refuses_what_node_refuses() {
        let directory = scratch("cp-with");
        let source = directory.join("src");
        std::fs::create_dir_all(source.join("sub")).expect("make the source");
        std::fs::write(source.join("a.txt"), b"new").expect("write a");
        std::fs::write(source.join("sub/b.txt"), b"b").expect("write b");
        std::os::unix::fs::symlink("a.txt", source.join("link")).expect("link a");
        let destination = directory.join("dst");
        std::fs::create_dir_all(&destination).expect("make the destination");
        std::fs::write(destination.join("a.txt"), b"old").expect("write the old a");
        std::fs::write(destination.join("kept.txt"), b"kept").expect("write the kept file");
        let fresh = directory.join("fresh/one/two");
        let (source_path, destination_path, fresh_path) =
            (text(&source), text(&destination), text(&fresh));
        let (source_again, fresh_again) = (source_path.clone(), fresh_path.clone());
        block_on(async move {
            cp_with(source_path.clone(), destination_path, options(true, false)).await;
            cp_with(source_path, fresh_path, options(true, false)).await;
        });
        assert_eq!(std::fs::read(destination.join("a.txt")).unwrap(), b"new");
        assert_eq!(
            std::fs::read(destination.join("kept.txt")).unwrap(),
            b"kept"
        );
        assert_eq!(std::fs::read(fresh.join("sub/b.txt")).unwrap(), b"b");
        let copied = std::fs::read_link(destination.join("link")).expect("a link was copied");
        assert_eq!(copied, std::path::absolute(source.join("a.txt")).unwrap());
        let missing = failure_of(cp_with(
            text(&directory.join("missing")),
            text(&directory.join("x")),
            options(true, false),
        ));
        assert!(missing.starts_with("ENOENT:"), "{missing}");
        let into_itself = failure_of(cp_with(
            source_again.clone(),
            text(&source.join("sub/inside")),
            options(true, false),
        ));
        assert!(into_itself.contains("cannot copy"), "{into_itself}");
        let onto_file = failure_of(cp_with(
            source_again,
            text(&source.join("a.txt")),
            options(true, false),
        ));
        assert!(onto_file.contains("non-directory"), "{onto_file}");
        let onto_directory = failure_of(cp_with(
            text(&source.join("a.txt")),
            fresh_again,
            options(true, false),
        ));
        assert!(
            onto_directory.contains("overwrite directory"),
            "{onto_directory}"
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// F156: `readdir { withFileTypes }` answers each entry's OWN kind — a
    /// symlink to a directory is a symlink and neither of the other two —
    /// sorted by name; a missing path is `ENOENT` and a file `ENOTDIR`. And
    /// `read_dir_all` lists every path under the root, relative, without
    /// entering a linked directory.
    #[cfg(unix)]
    #[test]
    fn scan_dir_reports_own_kinds_and_read_dir_all_lists_relative_paths() {
        let directory = scratch("scan-dir");
        std::fs::create_dir_all(directory.join("sub/deeper")).expect("make the tree");
        std::fs::write(directory.join("b.txt"), b"x").expect("write b");
        std::fs::write(directory.join("sub/c.txt"), b"x").expect("write c");
        std::os::unix::fs::symlink(directory.join("sub"), directory.join("a-link"))
            .expect("link the subdirectory");
        let scanned_path = text(&directory);
        let listed_path = scanned_path.clone();
        let with_types = FsOptions::new();
        with_types.set_with_file_types(true);
        let (scanned, listed) = block_on(async move {
            (
                scan_dir(scanned_path, with_types).await,
                read_dir_all(listed_path).await,
            )
        });
        let kinds: Vec<(String, bool, bool, bool)> = scanned
            .iter()
            .map(|entry| {
                (
                    entry.name().to_string(),
                    entry.is_directory(),
                    entry.is_file(),
                    entry.is_symlink(),
                )
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("a-link".to_string(), false, false, true),
                ("b.txt".to_string(), false, true, false),
                ("sub".to_string(), true, false, false),
            ]
        );
        let listed: Vec<&str> = listed.iter().map(|path| &**path).collect();
        assert_eq!(
            listed,
            vec!["a-link", "b.txt", "sub", "sub/c.txt", "sub/deeper"]
        );
        let with_types = || {
            let options = FsOptions::new();
            options.set_with_file_types(true);
            options
        };
        let missing = failure_of(scan_dir(text(&directory.join("missing")), with_types()));
        assert!(missing.starts_with("ENOENT:"), "{missing}");
        let file = failure_of(scan_dir(text(&directory.join("b.txt")), with_types()));
        assert!(file.starts_with("ENOTDIR:"), "{file}");
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_write_and_a_read_round_trip_text_and_bytes() {
        let directory = scratch("round-trip");
        let text_path = str_new(&directory.join("a.txt").to_string_lossy());
        let bytes_path = str_new(&directory.join("b.bin").to_string_lossy());
        let directory_path = str_new(&directory.to_string_lossy());
        block_on(async move {
            write_text(text_path.clone(), str_new("héllo")).await;
            assert_eq!(
                &*read_text(text_path.clone(), str_new("utf8")).await,
                "héllo"
            );
            append(text_path.clone(), str_new("!")).await;
            assert_eq!(
                &*read_text(text_path.clone(), str_new("utf8")).await,
                "héllo!"
            );
            write_bytes(bytes_path.clone(), Bytes::from_vec(vec![0, 1, 255])).await;
            assert_eq!(
                read_bytes(bytes_path.clone()).await.to_vec(),
                vec![0, 1, 255]
            );
            // A directory lists what was written into it, in one order.
            let listed = read_dir(directory_path.clone()).await;
            let names: Vec<&str> = listed.iter().map(|name| &**name).collect();
            assert_eq!(names, vec!["a.txt", "b.bin"]);
            remove(bytes_path).await;
            let listed = read_dir(directory_path.clone()).await;
            assert_eq!(listed.len(), 1, "the removed file is gone");
        });
        let _ = std::fs::remove_dir_all(&directory);
    }
}
