//! The corpus's stdin fixtures (A173 S1), declared ONCE for every harness that
//! RUNS a corpus program.
//!
//! A corpus program that reads stdin has its input beside it: `name.vl` reads
//! `name.stdin`. Every runner pipes that file in where it otherwise gives the
//! program an EMPTY stdin (`Stdio::null()`), so the two builds a corpus
//! differential compares, and the two backends `native_differential` compares,
//! read identical lines. A program with no fixture still gets the empty stdin,
//! which reads as an immediate end (`read_line()` is `None`, `scan()` is `""`).
//!
//! Included by `#[path]` from `vilan-core`'s corpus harness and from
//! `vilan-cli`'s native differential, so the convention has one spelling.

use std::path::{Path, PathBuf};
use std::process::Stdio;

/// The extension a stdin fixture carries.
pub const EXTENSION: &str = "stdin";

/// The fixture beside `program` (a `.vl` path), if it has one.
pub fn fixture_for(program: &Path) -> Option<PathBuf> {
    let fixture = program.with_extension(EXTENSION);
    fixture.is_file().then_some(fixture)
}

/// The stdin a run of `program` gets: its fixture, opened, or the empty stdin.
pub fn stdin_for(program: &Path) -> Stdio {
    match fixture_for(program) {
        Some(fixture) => match std::fs::File::open(&fixture) {
            Ok(file) => Stdio::from(file),
            Err(error) => panic!("open the stdin fixture {}: {error}", fixture.display()),
        },
        None => Stdio::null(),
    }
}
