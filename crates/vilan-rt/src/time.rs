//! `vilan-rt::time` — `std::time`'s host clock (tracker F18 slice 3).
//!
//! `vilan/std/src/time.vl` binds `Date.now` as `now_millis(): f64` —
//! milliseconds since the Unix epoch, which is what `now()` builds an
//! `Instant` from and what `std::rpc::server`'s handshake rate limit stamps
//! each attempt with. That one binding is this module. The timers the same
//! file binds (`__sleep`, `__timer`) are the executor's, which keeps its own
//! MONOTONIC clock: a deadline must not move when the wall clock does, and a
//! timestamp must — which is exactly the difference between `Instant` in Rust
//! and `Date.now` in JavaScript.
//!
//! F156 adds the one other host value the file touches: the `Date` that
//! `Instant::to_iso` builds to render itself ([`HostDate`]).

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

/// `new Date(millis)` — `std::time`'s `external struct HostDate`, which the
/// file builds for ONE reading: `toISOString()` in `Instant::to_iso` (F156).
/// So it carries the time value and nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostDate(i64);

/// The largest time value a JavaScript `Date` holds — ±100,000,000 days from
/// the epoch (ECMA-262 §21.4.1.1); a `Date` past it is an Invalid Date.
const MAX_TIME_VALUE: i64 = 8_640_000_000_000_000;

impl HostDate {
    /// `new Date(millis)`. A value past the `Date` range builds an Invalid Date
    /// on the other backend, which only its rendering can tell — so that is
    /// where this one refuses it, too.
    pub fn new(millis: i64) -> HostDate {
        HostDate(millis)
    }

    /// `date.toISOString()`, byte for byte: `YYYY-MM-DDTHH:mm:ss.sssZ` in UTC,
    /// the proleptic Gregorian calendar, and the EXPANDED year form outside
    /// `0000`-`9999` — a sign and six digits (`+010000`, `-000001`), which is
    /// the spec's (§21.4.4.36) and node's. An Invalid Date throws `RangeError:
    /// Invalid time value` there, and panics with that sentence here.
    pub fn to_iso_string(&self) -> crate::Str {
        crate::str_new(&iso_string(self.0))
    }
}

impl crate::Js for HostDate {
    fn js(&self) -> String {
        crate::panic_with(
            "printing a `Date` is a host object's own inspection, which the native backend does \
             not reproduce",
        )
    }
}

impl crate::Json for HostDate {
    fn json(&self) -> String {
        format!("\"{}\"", iso_string(self.0))
    }
}

/// [`HostDate::to_iso_string`]'s text for a time value.
fn iso_string(millis: i64) -> String {
    if !(-MAX_TIME_VALUE..=MAX_TIME_VALUE).contains(&millis) {
        crate::panic_with("RangeError: Invalid time value");
    }
    const DAY: i64 = 86_400_000;
    let days = millis.div_euclid(DAY);
    let within = millis.rem_euclid(DAY);
    let (year, month, day) = civil_from_days(days);
    let year = if (0..=9999).contains(&year) {
        format!("{year:04}")
    } else if year < 0 {
        format!("-{:06}", -year)
    } else {
        format!("+{year:06}")
    };
    format!(
        "{year}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        within / 3_600_000,
        within / 60_000 % 60,
        within / 1000 % 60,
        within % 1000
    )
}

/// The proleptic Gregorian `(year, month, day)` of a day count from
/// 1970-01-01 — Howard Hinnant's `civil_from_days`, which is exact over the
/// whole `Date` range (eras of 146,097 days, with March as the first month so
/// the leap day ends the year).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// F156: `toISOString`'s text, against node's own answers for the epoch,
    /// both sides of it, both leap days, the edges of the four-digit year, and
    /// the two ends of the `Date` range (node v24.2.0, recorded verbatim).
    #[test]
    fn to_iso_string_is_nodes_byte_for_byte() {
        let cases: &[(i64, &str)] = &[
            (0, "1970-01-01T00:00:00.000Z"),
            (1_700_000_000_000, "2023-11-14T22:13:20.000Z"),
            (-1, "1969-12-31T23:59:59.999Z"),
            (-1_000_000_000_000, "1938-04-24T22:13:20.000Z"),
            (951_782_400_000, "2000-02-29T00:00:00.000Z"),
            (1_709_251_199_999, "2024-02-29T23:59:59.999Z"),
            (-62_167_219_200_000, "0000-01-01T00:00:00.000Z"),
            (-62_167_219_200_001, "-000001-12-31T23:59:59.999Z"),
            (253_402_300_799_999, "9999-12-31T23:59:59.999Z"),
            (253_402_300_800_000, "+010000-01-01T00:00:00.000Z"),
            (8_640_000_000_000_000, "+275760-09-13T00:00:00.000Z"),
            (-8_640_000_000_000_000, "-271821-04-20T00:00:00.000Z"),
        ];
        for (millis, expected) in cases {
            assert_eq!(
                &*HostDate::new(*millis).to_iso_string(),
                *expected,
                "{millis}"
            );
        }
    }

    /// Past the `Date` range node throws `RangeError: Invalid time value`.
    #[test]
    #[should_panic(expected = "Invalid time value")]
    fn to_iso_string_refuses_a_time_past_the_date_range() {
        let _ = HostDate::new(8_640_000_000_000_001).to_iso_string();
    }

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
