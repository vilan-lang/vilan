//! N159: the pool size of a test that fans out inside itself.
//!
//! nextest's `-j` counts TESTS, and a few of the suite's tests run a worker pool of their
//! own (the replay and edit-replay corpus legs 16 ways, the check-scope legs 16 and 8, the
//! docs gate and the CLI corpus 8, `marker_census` up to 8, the style and example builds
//! one thread per fixture). On a box shared by several lanes each of those counted as one
//! slot while holding a dozen busy threads, so a `-j 6` run could hold 60+. The suite is
//! throughput-bound, so a narrower pool costs the one test wall time and the box nothing.
//!
//! The pool is, in order: `VILAN_TEST_POOL` when set (a positive number; `0` or garbage is
//! ignored); under nextest (`NEXTEST` is set in every test process, and
//! `NEXTEST_TEST_THREADS` carries `-j`) [`DEFAULT_UNDER_NEXTEST`]; otherwise what the test
//! asked for - `cargo test` runs one binary at a time and the pool is the only parallelism.
//!
//! Shared by `vilan-core`'s and `vilan-cli`'s test binaries (the latter includes this file
//! through `#[path]`), so there is one definition of the rule.
#![allow(dead_code)]

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The pool width under nextest when nothing names one: two workers keep a `-j 6` run to a
/// dozen busy threads however many pool-shaped tests are scheduled together.
pub const DEFAULT_UNDER_NEXTEST: usize = 2;

/// The cap, if there is one.
pub fn cap() -> Option<usize> {
    cap_from(
        std::env::var("VILAN_TEST_POOL").ok().as_deref(),
        std::env::var_os("NEXTEST").is_some() || std::env::var_os("NEXTEST_TEST_THREADS").is_some(),
    )
}

/// [`cap`] over explicit inputs (the seam the pin uses).
pub fn cap_from(pool: Option<&str>, under_nextest: bool) -> Option<usize> {
    if let Some(width) = pool
        .and_then(|text| text.trim().parse::<usize>().ok())
        .filter(|width| *width > 0)
    {
        return Some(width);
    }
    under_nextest.then_some(DEFAULT_UNDER_NEXTEST)
}

/// `wanted` workers, or fewer when the cap says so; never fewer than one.
pub fn workers(wanted: usize) -> usize {
    cap().map_or(wanted, |cap| wanted.min(cap)).max(1)
}

/// `f` over every item with at most [`workers`]`(wanted)` threads (a shared queue, so a slow
/// item does not hold a chunk of fast ones behind it), the answers in the items' order. A
/// panic inside `f` is resumed on the caller's thread once the workers have stopped.
pub fn map<T: Sync, R: Send>(items: &[T], wanted: usize, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let answers: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers(wanted).min(items.len().max(1)))
            .map(|_| {
                scope.spawn(|| {
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(index) else { break };
                        let answer = f(item);
                        answers
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)[index] =
                            Some(answer);
                    }
                })
            })
            .collect();
        for handle in handles {
            if let Err(panic) = handle.join() {
                std::panic::resume_unwind(panic);
            }
        }
    });
    answers
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .into_iter()
        .map(|answer| answer.expect("every item was answered"))
        .collect()
}
