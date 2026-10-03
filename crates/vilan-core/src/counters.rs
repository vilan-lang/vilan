//! `VILAN_COUNTERS` — the compiler's own WORK COUNTS (M105 S5, M108, M106).
//!
//! Every figure here is a count, never a clock: the same program on the same
//! compiler reads the same numbers on every machine and under any load, which
//! is what lets a gate hold them (`performance-gates.md` §3, tier T1) and what
//! the owner's M106 ruling asks of cost attribution ("computational complexity,
//! not time passed").
//!
//! Two families:
//!
//! - **Heap bytes**, through [`CountingAllocator`]. A binary that installs it
//!   as its `#[global_allocator]` (the CLI and the language server do) and
//!   calls [`arm_from_env`] first thing reads its LIVE heap, the PEAK of the
//!   live heap since the last [`reset_heap_peak`], and the allocations made.
//!   Peak RSS is what a user sees; the live heap is what the compiler is
//!   responsible for — the gap is the allocator's (fragmentation, per-thread
//!   arenas, pages never returned), and M108 needs the two told apart.
//!   Disarmed (the default) it costs one relaxed load and a branch per
//!   allocation and counts nothing.
//! - **Analyzer work**: type slots minted ([`type_slots_minted`]), and the
//!   counters that already live beside the passes they measure
//!   (`analyzer::inference_entry_count`, `impl_select::applying_computed`,
//!   `analyzer::generic_bound_checks`, …). [`counters_line`] prints them in one
//!   `[vilan counters]` stderr line per analysis when `VILAN_COUNTERS` is set.
//!
//! The thread-local counts are per THREAD, like every analyzer counter: an
//! analysis runs on one thread, and plain `cargo test` runs several in one
//! process. The heap counts are per PROCESS (an allocator has no other scope).

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};

static ARMED: AtomicBool = AtomicBool::new(false);
static LIVE_BYTES: AtomicIsize = AtomicIsize::new(0);
static PEAK_BYTES: AtomicIsize = AtomicIsize::new(0);
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);

/// The system allocator, counting while [`arm`]ed. Install it in a binary:
///
/// ```ignore
/// #[global_allocator]
/// static ALLOCATOR: vilan_core::counters::CountingAllocator = vilan_core::counters::CountingAllocator;
/// ```
pub struct CountingAllocator;

#[inline]
fn record_allocation(size: usize) {
    if !ARMED.load(Ordering::Relaxed) {
        return;
    }
    let size = size as isize;
    let live = LIVE_BYTES.fetch_add(size, Ordering::Relaxed) + size;
    // The peak is read first and written only when it moves: a `fetch_max` on
    // every allocation is a compare-exchange loop where this is a plain load.
    if live > PEAK_BYTES.load(Ordering::Relaxed) {
        PEAK_BYTES.fetch_max(live, Ordering::Relaxed);
    }
    ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    ALLOCATED_BYTES.fetch_add(size as u64, Ordering::Relaxed);
}

#[inline]
fn record_deallocation(size: usize) {
    if ARMED.load(Ordering::Relaxed) {
        LIVE_BYTES.fetch_sub(size as isize, Ordering::Relaxed);
    }
}

// SAFETY: every method forwards to `System` with the caller's own arguments and
// returns its answer unchanged; the counting touches only atomics, never the
// memory handed out, and never allocates.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded under this method's own contract.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record_allocation(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded under this method's own contract.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            record_allocation(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        record_deallocation(layout.size());
        // SAFETY: forwarded under this method's own contract.
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: forwarded under this method's own contract.
        let moved = unsafe { System.realloc(pointer, layout, new_size) };
        if !moved.is_null() {
            record_deallocation(layout.size());
            record_allocation(new_size);
        }
        moved
    }
}

/// Starts counting heap bytes. Allocations made before this are not counted,
/// and their frees are (the live figure is clamped at zero when read), so arm
/// at the top of `main`.
pub fn arm() {
    ARMED.store(true, Ordering::Relaxed);
}

/// Arms the heap count when `VILAN_COUNTERS` asks for the counters line.
pub fn arm_from_env() {
    if counters_enabled() {
        arm();
    }
}

/// Whether the heap is being counted (a [`CountingAllocator`] is installed AND
/// [`arm`]ed — the first cannot be asked, so a reader that sees zeros from an
/// armed process is in a binary that did not install it).
pub fn heap_armed() -> bool {
    ARMED.load(Ordering::Relaxed)
}

/// One reading of the heap counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeapReading {
    /// Bytes allocated and not yet freed.
    pub live: u64,
    /// The largest `live` since the last [`reset_heap_peak`].
    pub peak: u64,
    /// Allocations made (a `realloc` counts as one).
    pub allocations: u64,
    /// Bytes requested over all of them.
    pub allocated: u64,
}

pub fn heap_reading() -> HeapReading {
    HeapReading {
        live: LIVE_BYTES.load(Ordering::Relaxed).max(0) as u64,
        peak: PEAK_BYTES.load(Ordering::Relaxed).max(0) as u64,
        allocations: ALLOCATIONS.load(Ordering::Relaxed),
        allocated: ALLOCATED_BYTES.load(Ordering::Relaxed),
    }
}

/// Restarts the peak at the current live figure, so the next reading's peak
/// is the high-water mark of the work in between — how the per-pass split
/// attributes a peak to the pass that reached it.
pub fn reset_heap_peak() {
    PEAK_BYTES.store(LIVE_BYTES.load(Ordering::Relaxed), Ordering::Relaxed);
}

/// Whether `VILAN_COUNTERS` asks for the `[vilan counters]` line (any value but
/// empty or `0`). Read once and cached.
pub fn counters_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("VILAN_COUNTERS").is_ok_and(|value| !value.is_empty() && value != "0")
    })
}

thread_local! {
    static TYPE_SLOTS_MINTED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Type slots this thread's analyzers have minted — the interned type table's
/// growth, and M108's measure: every slot is a `Type` held until the program
/// drops. Monotonic; read as a difference around the work in question.
pub fn type_slots_minted() -> u64 {
    TYPE_SLOTS_MINTED.with(std::cell::Cell::get)
}

thread_local! {
    static LATE_TYPE_SLOT_WRITES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Writes that CHANGED an existing type slot after the analysis' checks began
/// (M108). Zero is the invariant substitution sharing rests on: a settled slot
/// that mentions no generic reads the same to every holder of its id.
pub fn late_type_slot_writes() -> u64 {
    LATE_TYPE_SLOT_WRITES.with(std::cell::Cell::get)
}

pub(crate) fn count_late_type_slot_write() {
    LATE_TYPE_SLOT_WRITES.with(|count| count.set(count.get() + 1));
}

thread_local! {
    static SETTLED_TYPE_SLOTS_MINTED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Type slots minted after an analysis' types SETTLED — by the checks passes,
/// the classification and the tables after the constraint fixpoint (M108).
/// Each distinct type is minted once there and shared after
/// (`Analyzer::type_id_for_type`), so this grows with the program's distinct
/// types and not with its call sites: the T1 pin's measure.
pub fn settled_type_slots_minted() -> u64 {
    SETTLED_TYPE_SLOTS_MINTED.with(std::cell::Cell::get)
}

pub(crate) fn count_settled_type_slot() {
    SETTLED_TYPE_SLOTS_MINTED.with(|count| count.set(count.get() + 1));
}

pub(crate) fn count_type_slot() {
    TYPE_SLOTS_MINTED.with(|count| count.set(count.get() + 1));
}

/// `12.3MB` for a byte count.
pub fn megabytes(bytes: u64) -> String {
    format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
}

/// The heap part of a pass or counters line (` heap=…MB peak=…MB`), empty when
/// the heap is not being counted. Resets the peak, so each line's peak is its
/// own span's.
pub fn heap_fragment() -> String {
    if !heap_armed() {
        return String::new();
    }
    let reading = heap_reading();
    reset_heap_peak();
    format!(
        " heap={} peak={}",
        megabytes(reading.live),
        megabytes(reading.peak)
    )
}

/// One `[vilan counters] <label> …` line when `VILAN_COUNTERS` is set: the
/// type slots this thread has minted so far and, when the heap is counted, the
/// live heap and the peak since the previous line. The analysis prints one at
/// each of its phase boundaries, so the lines read as a heap profile by phase.
pub fn checkpoint(label: &str) {
    if !counters_enabled() || crate::macros::in_macro_world() {
        return;
    }
    eprintln!(
        "[vilan counters] {label} slots={} late-writes={}{}",
        type_slots_minted(),
        late_type_slot_writes(),
        heap_fragment()
    );
}
