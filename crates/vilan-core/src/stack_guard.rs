//! The stack-remaining probe (N121): a recursion that is about to exhaust the
//! thread it runs on PANICS — which every long-lived surface's fence already
//! turns into one honest internal-error diagnostic — instead of running into
//! the guard page, where Rust's handler prints `has overflowed its stack` and
//! calls `abort()` from whichever thread it is on. No `catch_unwind` and no
//! `join` observes an abort: before this, one runaway walk (B385's
//! resource-in-container descent, B387's supertrait-blanket proof) took the
//! whole language server down with the buffer that triggered it, and the
//! client restarted it into the same buffer.
//!
//! **Two halves.** A thread that knows its stack DECLARES it —
//! [`with_declared_stack`], called at the top of the thread with the size its
//! `Builder::stack_size` spawned it with — and the analyzer's recursive
//! families PROBE it: [`ensure_sufficient_stack`] at the four funnels every
//! walk passes through (the depth instrument's four analyzer families,
//! `depth_stats`): `infer_type_inner`, `walk_expr_node`, `resolve_pattern`,
//! and `util::RecursionGuard::enter`, the guard `substitute_type`,
//! `reconcile_type` and impl selection's `subject_shape_matches` all enter —
//! which is the one both B385's and B387's cycles went through on every lap.
//! And a fifth for the SYNTACTIC visitors that run before any of those (N128):
//! `Node::for_each_child`, which `collect_module_paths` and every other
//! whole-tree scan recurse through once per level of nesting.
//! The probe sits on the funnels rather than in each walk because a runaway
//! recursion is, by definition, one nobody predicted: a probe that has to be
//! placed in the walk that runs away is a probe placed after the fact.
//!
//! **Which way it fails, and why that way** (Order 40's rule for a guard: say
//! which way it fails closed). A probe that fires refuses the WHOLE analysis
//! — a panic, never a "no" or a "yes" handed back into the walk, because a
//! walk's early answer is exactly how B394's depth cap manufactured a proof.
//! The analysis ends with no program and the fence's diagnostic; nothing the
//! walk half-computed escapes.
//!
//! **Where the floor is.** Rust's std does not say how much stack a thread has
//! left, and the platform queries that would (`pthread_getattr_np`, the
//! Windows TIB, nothing at all on wasm32) are the dependency this module
//! exists not to take. The thread that SPAWNED the stack does know, so it
//! says: [`with_declared_stack`] records the stack pointer at its call — the
//! top of the thread, give or take the closure's own frame — and the floor is
//! that address less the declared size, plus a red zone. A probe on a thread
//! that declared nothing (a libtest worker, a unit test entering the analyzer
//! sideways, the playground's wasm main thread) is inert, which is exactly
//! the behaviour before N121: an undeclared stack is not guessed at.
//!
//! **The red zone** is what must still be free when the probe fires: the
//! stack between two probes (the deepest single frame chain that passes no
//! funnel — a nested macro-world parse is the largest, the parser being
//! bounded but unprobed), plus the panic machinery itself (the default hook
//! formats the message and, with `RUST_BACKTRACE` set, symbolizes a
//! backtrace). An eighth of the declared stack, and never less than
//! [`RED_ZONE_MINIMUM`]: 16 MiB of the analysis threads' 128, which is still
//! ~2.4x the measured whole-pipeline worst case left usable (the CLI's
//! `COMPILER_STACK_SIZE` records the measurement).
//!
//! Stack addresses grow DOWN on every target vilan builds for (x86-64,
//! aarch64, wasm32's linear-memory shadow stack) — `depth_stats` makes the
//! same assumption for the same reason.

use std::cell::Cell;

/// The least red zone a declaration gets, whatever its size: 1 MiB, enough for
/// the unwinder and a symbolized backtrace with room over.
pub const RED_ZONE_MINIMUM: usize = 1024 * 1024;

/// One thread's declared stack, as the probe reads it.
#[derive(Clone, Copy)]
struct Declared {
    /// The address below which a probe refuses (the stack's end plus the red
    /// zone).
    floor: usize,
    /// The stack pointer where the declaration was made — the thread's top, for
    /// the message's "used" figure.
    anchor: usize,
    /// The size the thread was spawned with.
    size: usize,
}

thread_local! {
    static DECLARED: Cell<Option<Declared>> = const { Cell::new(None) };
}

/// The current stack position, approximately: the address of a fresh local in
/// the caller's frame. Where in the frame it falls is noise at the MiB scale
/// the red zone is sized in.
#[inline(always)]
fn approximate_sp() -> usize {
    let marker = 0u8;
    std::hint::black_box(std::ptr::addr_of!(marker)) as usize
}

/// The red zone a stack of `stack_size` bytes keeps free: an eighth of it, at
/// least [`RED_ZONE_MINIMUM`], and never more than half — a declaration too
/// small to hold a red zone and an analysis both refuses early rather than
/// probing a floor above its own top.
fn red_zone(stack_size: usize) -> usize {
    (stack_size / 8).max(RED_ZONE_MINIMUM).min(stack_size / 2)
}

/// Runs `body` with this thread's stack DECLARED: `stack_size` is the size the
/// thread was spawned with (`std::thread::Builder::stack_size`), and this call
/// belongs at the TOP of that thread, before anything deep runs — the floor is
/// measured down from here, so a declaration made halfway down a stack would
/// place it past the real end.
///
/// A declaration nested inside another (a thread that declares, then calls
/// something that declares again) takes over for `body` and hands back after
/// it, on every exit path, an unwind included.
pub fn with_declared_stack<R>(stack_size: usize, body: impl FnOnce() -> R) -> R {
    let anchor = approximate_sp();
    let declared = Declared {
        floor: anchor
            .saturating_sub(stack_size)
            .saturating_add(red_zone(stack_size)),
        anchor,
        size: stack_size,
    };
    struct Restore(Option<Declared>);
    impl Drop for Restore {
        fn drop(&mut self) {
            DECLARED.with(|cell| cell.set(self.0));
        }
    }
    let _restore = Restore(DECLARED.with(|cell| cell.replace(Some(declared))));
    body()
}

/// The size this thread DECLARED ([`with_declared_stack`]), or `None` on a
/// thread that declared nothing — where every probe is inert. For the pins
/// that hold a thread-spawning site to its declaration (N128): a spawn that
/// forgets to declare has probes that never fire, and nothing else can see
/// that from outside the thread.
#[doc(hidden)]
pub fn declared_stack_size() -> Option<usize> {
    DECLARED.with(Cell::get).map(|declared| declared.size)
}

/// The probe: panics when this thread declared its stack
/// ([`with_declared_stack`]) and the caller's frame is already inside the red
/// zone; returns otherwise, and always on a thread that declared nothing.
/// `walk` names the funnel for the panic message, which is the "details on
/// stderr" the fence's diagnostic points at.
///
/// One thread-local read and one comparison on the path that returns — the
/// funnels it sits in run once per expression, pattern and type-walk level.
#[inline]
pub fn ensure_sufficient_stack(walk: &'static str) {
    let Some(declared) = DECLARED.with(Cell::get) else {
        return;
    };
    let position = approximate_sp();
    if position < declared.floor {
        exhausted(walk, position, declared);
    }
}

#[cold]
#[inline(never)]
fn exhausted(walk: &'static str, position: usize, declared: Declared) -> ! {
    let used = declared.anchor.saturating_sub(position);
    panic!(
        "the compiler's recursion reached the end of its stack in {walk} ({used} of the \
         {size} bytes this thread declared are in use, {red_zone} of them kept free to report \
         it): a recursion this deep is a runaway walk in the compiler, refused here rather \
         than left to overflow and abort the process (N121)",
        size = declared.size,
        red_zone = red_zone(declared.size),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Recurses until the probe refuses, one probe per level, with a frame
    /// the optimizer cannot fold away.
    #[allow(
        unconditional_recursion,
        reason = "the plant IS an unbounded recursion; the stack probe inside it is the only way out"
    )]
    fn descend(level: usize) -> usize {
        ensure_sufficient_stack("the test descent");
        let padding = std::hint::black_box([level as u8; 512]);
        descend(level + 1) + padding[0] as usize
    }

    #[test]
    fn an_undeclared_thread_is_never_refused() {
        // No declaration: the probe must return even far below any floor a
        // declaration would have set. Eight levels is enough to prove it is
        // not consulting a stale declaration.
        fn shallow(level: usize) -> usize {
            ensure_sufficient_stack("the test descent");
            if level == 8 {
                level
            } else {
                shallow(level + 1)
            }
        }
        assert_eq!(shallow(0), 8);
    }

    #[test]
    fn a_declared_thread_refuses_before_its_stack_ends_and_hands_back_its_declaration() {
        const SIZE: usize = 4 * 1024 * 1024;
        let outcome = std::thread::Builder::new()
            .stack_size(SIZE)
            .spawn(|| {
                let refused = std::panic::catch_unwind(|| with_declared_stack(SIZE, || descend(0)));
                // The declaration is handed back on the unwind: the thread is
                // undeclared again, so a probe here is inert.
                let restored = DECLARED.with(Cell::get).is_none();
                (refused, restored)
            })
            .expect("spawn")
            .join()
            .expect("the probe's panic is caught inside the thread");
        let message = outcome
            .0
            .expect_err("the descent is refused")
            .downcast::<String>()
            .expect("a formatted message");
        assert!(
            message.contains("the test descent") && message.contains("N121"),
            "{message}"
        );
        assert!(outcome.1, "the declaration is restored after the unwind");
    }

    #[test]
    fn the_red_zone_is_an_eighth_at_least_a_mebibyte_and_at_most_half() {
        assert_eq!(red_zone(128 * 1024 * 1024), 16 * 1024 * 1024);
        assert_eq!(red_zone(4 * 1024 * 1024), RED_ZONE_MINIMUM);
        assert_eq!(red_zone(1024 * 1024), 512 * 1024);
    }
}
