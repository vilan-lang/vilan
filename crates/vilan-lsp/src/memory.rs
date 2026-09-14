//! M64 / E166: what this process's memory looks like, and the one thing the
//! server can do about it.
//!
//! Two facts and one action, all three of them Linux-shaped and all three
//! no-ops elsewhere:
//!
//! - **Resident size**, from `/proc/self/statm` — the number the owner read off
//!   the live server (4.13 GB after eight hours, M63's field reading) and the
//!   one a report arrives with.
//! - **The heap's own split**, from glibc's `mallinfo2`: bytes IN USE and bytes
//!   FREE BUT RETAINED. This is the pair that located E106 (738 → 320 MB in
//!   use, 421 MB retained free after closing eighteen documents) and the reason
//!   RSS alone is not a verdict — a resident figure that stays up while in-use
//!   bytes fall is the allocator's to hand back, not the server's to free.
//! - **[`trim`]**, glibc's `malloc_trim(0)`: ask the allocator to hand the
//!   retained half back to the OS. That 421 MB is exactly what a session of
//!   opening and closing files ratchets up and never gives back (M64), and
//!   nothing in the server can release it — the memory is already free as far
//!   as the program is concerned; it is glibc holding the arenas.
//!
//! **Why `libc` is a real dependency of this crate** (M64's ruling, 2026-09-13):
//! `malloc_trim` has no `std` spelling, and hand-declaring an `extern "C"` block
//! for it — which `document.rs`'s test-only `mallinfo2` reader does — is the
//! wrong posture for a call in the SHIPPED server. `libc` was already in the
//! lockfile (and so already covered by `THIRD-PARTY-NOTICES.txt`); the edge is
//! now declared for `cfg(target_os = "linux")`, the platform whose `/proc` and
//! whose allocator this module reads. The two glibc-only symbols are gated a
//! second time, on `target_env = "gnu"`: a musl build still reports resident
//! size and simply has no heap split to report.
//!
//! Every reader answers `None` (and [`trim`] answers `false`) off that
//! platform, so a caller never branches on the host: it asks, and prints what
//! it got.

/// One reading of the process's memory, as the status page reports it
/// (E166).
///
/// Every field is optional because every field is a platform reading: this is
/// what the host was able to say, not what the server believes. `None` prints
/// as `?`, which is a fact about the instrument and reads as one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Memory {
    /// Resident set size in bytes.
    pub resident_bytes: Option<usize>,
    /// Heap bytes allocated and not yet freed (`mallinfo2().uordblks`).
    pub heap_in_use_bytes: Option<usize>,
    /// Heap bytes freed by the program but retained by the allocator
    /// (`mallinfo2().fordblks`) — what [`trim`] asks glibc to give back.
    pub heap_retained_bytes: Option<usize>,
}

impl Memory {
    /// Read the host now.
    pub fn sample() -> Memory {
        let (in_use, retained) = match heap_split_bytes() {
            Some((in_use, retained)) => (Some(in_use), Some(retained)),
            None => (None, None),
        };
        Memory {
            resident_bytes: resident_bytes(),
            heap_in_use_bytes: in_use,
            heap_retained_bytes: retained,
        }
    }

    /// The status page's memory line (E166).
    ///
    /// MiB with one decimal, because the numbers this line exists to carry are
    /// hundreds of megabytes and a byte count at that size is read wrong: the
    /// owner's report said "4.13 GB", not "4432068608". Integer tenths rather
    /// than a float for the reason the request table next to it gives — this is
    /// a log line that wants to diff cleanly against the next one, and a float
    /// drags locale-shaped formatting into it.
    pub fn line(&self) -> String {
        format!(
            "memory: rss={} heap_in_use={} heap_retained_free={}",
            mib(self.resident_bytes),
            mib(self.heap_in_use_bytes),
            mib(self.heap_retained_bytes),
        )
    }
}

/// `bytes` as `N.M MiB`, or `?` where the host said nothing.
///
/// Shared with the retained-state line (E179), which carries the base cache's
/// weight and budget in the same unit and must render them the same way — two
/// spellings of "N.M MiB" on one page is exactly the kind of drift the summary
/// exists to be read quickly.
pub(crate) fn mib(bytes: Option<usize>) -> String {
    match bytes {
        // Tenths of a MiB, computed in integers: `bytes * 10 / MiB` cannot
        // overflow a `usize` on any address space this process runs in (the
        // numerator is at most ten times the process's own size).
        Some(bytes) => {
            let tenths = bytes * 10 / (1024 * 1024);
            format!("{}.{} MiB", tenths / 10, tenths % 10)
        }
        None => "?".to_string(),
    }
}

/// Resident set size in bytes, from `/proc/self/statm` (resident pages × the
/// page size). `None` where `/proc` is not there — Windows and macOS both,
/// which is why the reader is gated rather than merely tolerant.
///
/// The page size is ASKED for rather than assumed: `statm` counts pages, and
/// the harness's `pages * 4` is a 4 KiB assumption that is right on every x86
/// Linux and wrong by 4× or 16× on an arm64 kernel built with larger pages —
/// which would make the one number the owner reports off by an order of
/// magnitude on exactly the machine that reports it.
#[cfg(target_os = "linux")]
pub fn resident_bytes() -> Option<usize> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: usize = statm.split_whitespace().nth(1)?.parse().ok()?;
    // SAFETY: `sysconf` reads a static system parameter and touches nothing of
    // ours. A negative answer means "no limit / unknown", which is not a page
    // size — fall back to the 4 KiB every Linux this server ships to uses.
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    let page_size = if page_size > 0 {
        page_size as usize
    } else {
        4096
    };
    Some(pages * page_size)
}

#[cfg(not(target_os = "linux"))]
pub fn resident_bytes() -> Option<usize> {
    None
}

/// The allocator's own split of the heap — `(in use, free but retained)` in
/// bytes, from glibc's `mallinfo2`.
///
/// The same reading `document.rs`'s `leak_measurement::heap_split_bytes` takes
/// for the leak harness (`leak-soak.md` §7.7); this is its shipped twin,
/// through `libc` rather than a hand-written `extern` block.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn heap_split_bytes() -> Option<(usize, usize)> {
    // SAFETY: `mallinfo2` reads the allocator's own statistics and touches
    // nothing else. glibc ≥ 2.33 exports it; `libc` declares it for exactly
    // this target.
    let info = unsafe { libc::mallinfo2() };
    Some((info.uordblks, info.fordblks))
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
pub fn heap_split_bytes() -> Option<(usize, usize)> {
    None
}

/// The heap's IN-USE half alone (`mallinfo2().uordblks`), the reading a release
/// is measured against (M68).
///
/// Cheap enough to take on the landing path: 266 and 990 ns per call over two
/// runs of a thousand calls, in a server-shaped process with kolt's nineteen
/// files open (`mallinfo2_cost`, release, loadavg 42–58) — four orders of
/// magnitude under the trim it decides, and five under the landing it rides
/// on.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn heap_in_use_bytes() -> Option<usize> {
    heap_split_bytes().map(|(in_use, _)| in_use)
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
pub fn heap_in_use_bytes() -> Option<usize> {
    None
}

/// The least a release must hand back before the server asks glibc to walk its
/// arenas (M68).
///
/// 4 MiB, and the measurement is why this is a FLOOR rather than a policy.
/// `trim_cost_across_a_sibling_checkout` drives twelve BACKGROUND landings
/// over kolt's nineteen open files — a document's analysis lands, is adopted,
/// and is released by the retention rule because the focus is elsewhere, which
/// is `analyze_and_publish`'s tail — and every one of them released between
/// 10.7 and 81.2 MiB and got 14.0 to 135.3 MiB of RESIDENT SIZE back for 3.4
/// to 32.6 ms of CPU (release, two runs, loadavg 42–68, per-landing mean 10.7
/// and 16.9 ms). The candidate floors, over those same landings:
///
/// ```text
/// floor    landings skipped   CPU saved   resident size forgone
///  4 MiB        0 / 12           0 ms            0 MiB
/// 16 MiB        5 / 12        33.6 ms          107 MiB
/// 32 MiB        7 / 12        67.9 ms          251 MiB
/// 64 MiB        7 / 12        67.9 ms          251 MiB
/// ```
///
/// Three megabytes of resident size per millisecond of a background thread is
/// not a trade this server should take, and that is the whole argument for a
/// LOW floor: what the threshold is for is the release that cannot pay for the
/// walk at all. glibc walks its arenas in proportion to THEIR size, not to what
/// was just handed back, so a release of a few hundred kilobytes has nothing
/// page-shaped to give and still costs the whole walk. No landing in a
/// kolt-sized session is that small — the floor does not fire there, and that
/// is the measurement's verdict rather than a disappointment.
pub const TRIM_MIN_RELEASED_BYTES: usize = 4 * 1024 * 1024;

/// [`trim`], but only if the heap's in-use bytes have fallen by more than
/// [`TRIM_MIN_RELEASED_BYTES`] since `before` — the reading
/// [`heap_in_use_bytes`] took ahead of the release. Answers whether it trimmed.
///
/// M64 asked for the trim after EVERY release, which is right at a close and
/// too eager at the analysis-landing seam: the dependency sweep re-analyzes
/// every open importer of an edited file, each of those landings releases the
/// program it just brought (the document is not one of the focused few), and
/// each release walked the arenas. Measured in CPU rather than wall, that walk
/// is 3.4–32.6 ms per landing — the item's estimate of ~100 ms per background
/// document was a wall-clock difference across whole opens — and the reading
/// that gates it is under a microsecond. So the conditional is cheap enough to
/// ask at every landing, and it is set where a release stops being able to
/// return a page's worth, not where the trim stops being worth its CPU.
///
/// A host that cannot read its heap gets M64's unconditional trim, which on
/// that host is the no-op [`trim`] already is.
pub fn trim_if_released(before: Option<usize>) -> bool {
    if !worth_trimming(before, heap_in_use_bytes()) {
        return false;
    }
    trim()
}

/// The decision [`trim_if_released`] makes, alone: did the heap's in-use half
/// fall by at least [`TRIM_MIN_RELEASED_BYTES`] between the two readings?
///
/// Separated from the call so a pin can hold the POLICY without asking the
/// allocator anything — `malloc_trim` answers whether it found something to
/// give back, which is a fact about the host's arenas at that instant and not
/// about this rule.
///
/// A missing reading answers `true`: a host that cannot say what it released
/// gets M64's unconditional trim, which on such a host is the no-op [`trim`]
/// already is. A heap that GREW between the readings answers `false` — the
/// subtraction saturates, and a landing that allocated more than the release
/// handed back has nothing to trim by definition.
pub fn worth_trimming(before: Option<usize>, now: Option<usize>) -> bool {
    match (before, now) {
        (Some(before), Some(now)) => before.saturating_sub(now) >= TRIM_MIN_RELEASED_BYTES,
        _ => true,
    }
}

/// Ask glibc to return the heap's retained-free arenas to the OS
/// (`malloc_trim(0)`), answering whether it says it gave anything back.
///
/// M64: closing eighteen documents returned 418 MB to the allocator's free list
/// and 62 MB to the OS — the difference is this call's whole subject. It is
/// asked after a document CLOSES and after an analysis is DROPPED (M63's
/// release), which are the two moments this server hands back a whole
/// program's worth of allocations at once; never per keystroke, because the
/// walk is proportional to the arenas and the memory it would find is about to
/// be reused by the next analysis anyway.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn trim() -> bool {
    // SAFETY: `malloc_trim` walks the allocator's free lists and releases what
    // it can; it takes no pointer from us and invalidates nothing we hold.
    unsafe { libc::malloc_trim(0) == 1 }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
pub fn trim() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// E166's formatting pin: the three numbers, in MiB, in the order the page
    /// prints them.
    #[test]
    fn the_memory_line_renders_three_figures_in_mebibytes() {
        let memory = Memory {
            resident_bytes: Some(4_331_798_528),
            heap_in_use_bytes: Some(335_544_320),
            heap_retained_bytes: Some(441_450_496),
        };
        assert_eq!(
            memory.line(),
            "memory: rss=4131.1 MiB heap_in_use=320.0 MiB heap_retained_free=421.0 MiB",
        );
    }

    /// A host that answers nothing prints `?` rather than a zero: a missing
    /// reading and a heap of zero bytes are different facts, and a status page
    /// that renders them identically is the one that gets quoted in a report.
    #[test]
    fn a_reading_the_host_cannot_take_prints_a_question_mark() {
        assert_eq!(
            Memory::default().line(),
            "memory: rss=? heap_in_use=? heap_retained_free=?",
        );
    }

    /// The tenths are real: 1.5 MiB is not 1 MiB, and a page that truncated to
    /// whole mebibytes would report the growth this instrument exists to show
    /// in steps of a megabyte.
    #[test]
    fn a_fractional_mebibyte_renders_to_one_decimal() {
        assert_eq!(mib(Some(1024 * 1024 * 3 / 2)), "1.5 MiB");
        assert_eq!(mib(Some(0)), "0.0 MiB");
    }

    /// THE PIN (M68): the trim at the release seam is conditional on how much
    /// the release handed back, and the rule is stated on the READINGS rather
    /// than on what glibc then says it found.
    ///
    /// The threshold is a floor, not a policy — every background landing in a
    /// kolt-sized session releases 10.7 to 81.2 MiB and gets 14.0 to 135.2 MiB
    /// of resident size back for 3.4 to 18.6 ms of CPU — so what this holds is
    /// that a release too small to return a page's worth does not pay for the
    /// arena walk, and that everything a real session releases still does.
    #[test]
    fn a_release_below_the_threshold_does_not_earn_the_arena_walk() {
        let after = 400 * 1024 * 1024;
        assert!(
            !worth_trimming(Some(after + TRIM_MIN_RELEASED_BYTES - 1), Some(after)),
            "a release one byte short of the floor buys nothing page-shaped",
        );
        assert!(
            worth_trimming(Some(after + TRIM_MIN_RELEASED_BYTES), Some(after)),
            "the floor itself trims — the boundary belongs to the release",
        );
        assert!(
            worth_trimming(Some(after + 68 * 1024 * 1024), Some(after)),
            "a background landing's own release (68 MiB, measured) trims: the \
             threshold must not turn M64 off",
        );
        assert!(
            !worth_trimming(Some(after), Some(after + 1024 * 1024)),
            "a heap that grew between the readings released nothing",
        );
        assert!(
            worth_trimming(None, Some(after)),
            "a host that cannot read its heap keeps M64's unconditional trim",
        );
        assert!(worth_trimming(Some(after), None));
    }

    /// The platform readers, on the platform this suite runs on: whatever the
    /// host answers, it answers CONSISTENTLY — a `Some` from one call is a
    /// `Some` from the next, and the two heap halves arrive together or not at
    /// all. A pin on the VALUES would be a pin on the box.
    #[test]
    fn the_readers_agree_with_themselves() {
        assert_eq!(resident_bytes().is_some(), resident_bytes().is_some());
        let sample = Memory::sample();
        assert_eq!(
            sample.heap_in_use_bytes.is_some(),
            sample.heap_retained_bytes.is_some(),
            "the two halves come from one `mallinfo2` call and cannot disagree",
        );
        #[cfg(target_os = "linux")]
        assert!(
            sample.resident_bytes.is_some_and(|bytes| bytes > 0),
            "this suite runs on Linux, where /proc/self/statm is readable",
        );
    }
}
