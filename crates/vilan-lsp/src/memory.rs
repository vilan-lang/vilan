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
