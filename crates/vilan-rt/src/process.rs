//! `vilan-rt::process` — `std::process` natively (F?4).
//!
//! # What this is a twin OF
//!
//! `vilan/std/src/process.vl` binds node's `process` object: `args`, `cwd` and
//! `exit` are one call into Rust's own `std` each, and `env` is an intrinsic the
//! emitter writes inline.

use std::io::Write;

use crate::{Str, panic_with, str_new};

// ------------------------------------------------------------ the surface --

/// `process::args()` — `process.argv.slice(2)`: the arguments after the
/// program itself. A native binary's `argv[0]` is the binary, so one is
/// skipped where node skips two (node and the script). An argument that is
/// not UTF-8 is decoded lossily, as node decodes it.
pub fn args() -> Vec<Str> {
    std::env::args_os()
        .skip(1)
        .map(|argument| str_new(&argument.to_string_lossy()))
        .collect()
}

/// `process::cwd()` — `process.cwd()`. node throws when the directory is
/// gone (`ENOENT: process.cwd failed with error no such file or directory`),
/// and a throw nobody catches ends the program with its message: so does this.
pub fn cwd() -> Str {
    match std::env::current_dir() {
        Ok(directory) => str_new(&directory.to_string_lossy()),
        Err(error) => panic_with(&format!("process.cwd failed with error {error}")),
    }
}

/// `process::exit(code)` — `process.exit(code)`: the process ends here, running
/// no destructor and no pending task, exactly as node's does. Stdout is flushed
/// first: Rust's stdout is line-buffered, and an unterminated line held there
/// would be lost where node has already written it.
pub fn exit(code: i32) -> ! {
    let _ = std::io::stdout().flush();
    std::process::exit(code)
}
