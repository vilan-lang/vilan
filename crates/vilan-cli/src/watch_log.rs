//! A timestamped trace of what the `--watch` loop SAW and what it DECIDED
//! (tracker B208).
//!
//! The watcher is a poll loop over a map of path → modification time
//! ([`crate::watch_snapshot`]), and its whole observable surface is one line
//! per round. That is enough to say a round happened and nothing at all to say
//! why one did not: a wake-up can be lost because the poll never saw the write
//! (a snapshot read racing it, an mtime that did not move, a path that left the
//! watched set), because the difference was consumed by a round that did not
//! act on it, or because the loop was still inside the previous round. Those
//! are four different bugs with one symptom — "round 2 never fired" — and B208
//! is the recorded strike nobody could tell apart: round 1 in 0.6 s, round 2
//! never in 300 s, while the harness re-touched the trigger fifteen times.
//!
//! So the trace records, with a clock: the baseline and every path seeded into
//! it, every poll that found a difference (naming each added, removed and moved
//! path with both timestamps), the heartbeat of the polls that found none, and
//! every round's start, verdict and duration — including whether the round
//! consumed its difference or kept it for the retry.
//!
//! **Off unless asked.** `VILAN_WATCH_LOG` names a file to append to, or `-` /
//! `1` / `stderr` for stderr. Unset, every function here is a load of a
//! `OnceLock` and a return — the poll rate is 300 ms and a watch session lives
//! for hours, so the trace is a diagnostic instrument and not a mode anything
//! should run in by default. A path that cannot be opened disables the trace
//! with one warning: a watch session must not die over its own logging.
//!
//! The file is opened in APPEND mode and each line is flushed as it is written,
//! so a session killed mid-round (which is how the pins end one) still leaves
//! everything it had said.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime};

/// Where the trace goes, decided once per process from `VILAN_WATCH_LOG`.
enum Sink {
    Off,
    Stderr,
    File(Mutex<std::fs::File>),
}

/// The sink plus the instant the session started, so every line carries an
/// elapsed time the reader can subtract without parsing a wall clock.
struct Trace {
    sink: Sink,
    started: Instant,
}

static TRACE: OnceLock<Trace> = OnceLock::new();

fn trace() -> &'static Trace {
    TRACE.get_or_init(|| Trace {
        sink: open_sink(),
        started: Instant::now(),
    })
}

fn open_sink() -> Sink {
    let Ok(destination) = std::env::var("VILAN_WATCH_LOG") else {
        return Sink::Off;
    };
    match destination.as_str() {
        "" | "0" | "off" => Sink::Off,
        "-" | "1" | "stderr" => Sink::Stderr,
        path => match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            Ok(file) => Sink::File(Mutex::new(file)),
            Err(error) => {
                eprintln!("warning: VILAN_WATCH_LOG cannot open {path}: {error}");
                Sink::Off
            }
        },
    }
}

/// Whether the trace is on — the one guard callers need before building a
/// message that costs anything (a snapshot diff walks two maps).
pub fn enabled() -> bool {
    !matches!(trace().sink, Sink::Off)
}

/// Writes one line, prefixed with the seconds since the session started.
pub fn line(message: &str) {
    let trace = trace();
    let elapsed = trace.started.elapsed().as_secs_f64();
    // The pid is on every line, not just the banner: a suite runs several
    // watch sessions and a shared `VILAN_WATCH_LOG` is the convenient way to
    // collect them, so a line has to say which session it came from.
    let line = format!(
        "[watch-log {}] {elapsed:9.3}s {message}\n",
        std::process::id()
    );
    match &trace.sink {
        Sink::Off => {}
        Sink::Stderr => {
            let _ = std::io::stderr().write_all(line.as_bytes());
        }
        Sink::File(file) => {
            let mut file = file
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let _ = file.write_all(line.as_bytes());
            let _ = file.flush();
        }
    }
}

/// A modification time as the trace prints one: microseconds since the epoch,
/// which is the resolution the question needs (an mtime that did not move
/// across a rewrite is the hypothesis this exists to test) in a form that
/// subtracts by eye.
pub fn stamp(time: SystemTime) -> String {
    match time.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(since) => format!("{}.{:06}", since.as_secs(), since.subsec_micros()),
        Err(_) => "pre-epoch".to_string(),
    }
}

/// The difference between two snapshots, rendered: every path added, removed or
/// moved, with both timestamps for a move.
///
/// This is the line that answers B208's question. The loop compares the two
/// maps for equality and acts on the boolean; the trace names WHICH entries
/// disagreed, so a round that fired for a reason nobody expected, and a poll
/// that found nothing while a file was being rewritten under it, are both
/// legible after the fact.
pub fn snapshot_diff(
    previous: &std::collections::BTreeMap<std::path::PathBuf, SystemTime>,
    next: &std::collections::BTreeMap<std::path::PathBuf, SystemTime>,
) -> String {
    let mut rendered = String::new();
    let mut differences = 0;
    for (path, moved) in next {
        match previous.get(path) {
            None => {
                differences += 1;
                let _ = write!(rendered, "\n    + {} @{}", path.display(), stamp(*moved));
            }
            Some(before) if before != moved => {
                differences += 1;
                let _ = write!(
                    rendered,
                    "\n    ~ {} @{} -> @{}",
                    path.display(),
                    stamp(*before),
                    stamp(*moved)
                );
            }
            Some(_) => {}
        }
    }
    for path in previous.keys() {
        if !next.contains_key(path) {
            differences += 1;
            let _ = write!(rendered, "\n    - {}", path.display());
        }
    }
    format!("{differences} difference(s){rendered}")
}

/// The session banner: what is watched, and the baseline's size.
pub fn session_start(roots: &[std::path::PathBuf], baseline: usize) {
    if !enabled() {
        return;
    }
    let watched: Vec<String> = roots
        .iter()
        .map(|root| root.display().to_string())
        .collect();
    line(&format!(
        "session start roots=[{}] baseline={baseline} entries",
        watched.join(", ")
    ));
}

/// One recorded input the first round discovered, and what the baseline did
/// with it — the E20 rule at the one place it can silently swallow an edit.
pub fn seed(path: &Path, modified: SystemTime, started: SystemTime, seeded: bool) {
    if !enabled() {
        return;
    }
    line(&format!(
        "seed {} @{} (session started @{}) -> {}",
        path.display(),
        stamp(modified),
        stamp(started),
        if seeded {
            "into the baseline"
        } else {
            "LEFT OUT (modified at or after the session started; the next poll fires)"
        }
    ));
}
