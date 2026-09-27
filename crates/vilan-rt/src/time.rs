//! `vilan-rt::time` — `std::time`'s host clock (tracker F18 slice 3).
//!
//! `vilan/std/src/time.vl` binds `Date.now` as `now_millis(): f64` —
//! milliseconds since the Unix epoch, which is what `now()` builds an
//! `Instant` from and what `std::rpc_server`'s handshake rate limit stamps
//! each attempt with. That one binding is this module. The timers the same
//! file binds (`__sleep`, `__timer`) are the executor's, which keeps its own
//! MONOTONIC clock: a deadline must not move when the wall clock does, and a
//! timestamp must — which is exactly the difference between `Instant` in Rust
//! and `Date.now` in JavaScript.

use std::time::{SystemTime, UNIX_EPOCH};

/// `Date.now()` — whole milliseconds since the Unix epoch, as the double the
/// host answers. JavaScript's value is an integer (the spec's time value is
/// integral milliseconds), so the sub-millisecond part is dropped rather than
/// carried as a fraction the other backend never shows. A clock set before
/// 1970 answers the negative distance, as `Date.now` would.
pub fn now_millis() -> f64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(since) => since.as_millis() as f64,
        Err(before) => -(before.duration().as_millis() as f64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Integral, and after this code was written (2026-09-24 is
    /// 1,790,208,000,000 ms) — which is a claim about the clock's UNIT and
    /// EPOCH, the two things a wrong conversion gets wrong, and not about any
    /// duration.
    #[test]
    fn now_is_whole_milliseconds_since_the_unix_epoch() {
        let now = now_millis();
        assert_eq!(now.fract(), 0.0);
        assert!(now > 1_790_000_000_000.0, "{now}");
        assert!(now < 1_790_000_000_000.0 * 10.0, "{now}");
    }
}
