//! `vilan-rt::fs` — the plain half of what `std::fs` binds (tracker F18
//! slice 2).
//!
//! # What this is a twin OF
//!
//! `vilan/std/src/process/fs.vl` binds `node:fs/promises`. The bindings that
//! take no options object are here — read, write, append, copy, rename,
//! remove, list, make and remove a directory — because each is one call into
//! Rust's own `std::fs` and because `serve_build`'s dev-mode freshness read
//! (`asset_body` under `run --watch`) is on the path `Server::builder()`
//! reaches. The `FsOptions` forms (`mkdir { recursive }`, `rm { force }`,
//! `cp`, `readdir { withFileTypes }`) are NOT here: an options object is a
//! host value with setters, and the shape it wants is its own item.
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

use std::path::Path;

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
