//! `Node`'s in-place size is a COST, not a detail (tracker M32).
//!
//! The parser moves `Spanned<Node>` by value through every precedence level:
//! each level's `Option<Spanned<Node>>` return copies the whole enum up the
//! recursion, so `parse_member_accessor` and `parse_binary_level` were the two
//! top `memcpy` callers of a cold check (400k + 258k + 238k and 279k + 277k
//! calls on kolt's client, inside a `malloc`/`free`/`memcpy` family that is
//! ~19% of the check's 7.19e9 Ir). The enum's width multiplies every one of
//! those moves, and nothing in the type system makes it visible: a field added
//! to `Func` or a variant given one more `Vec` widens every expression the
//! parser returns, at no call site anyone edits.
//!
//! So the width is pinned here as a CEILING rather than an equality. An
//! equality would red on any platform whose pointer or `usize` width differs
//! and would have to be re-blessed for every harmless narrowing; a ceiling
//! reds on exactly the change that costs — a variant growing past what the
//! measured layout allows — and says in its message what to do about it.
//!
//! **What the ceiling is.** `M32` measured `Node` at 320 bytes
//! (`Spanned<Node>` 336) with `Func` inlined into `Node::Func` /
//! `Node::MacroFun`. `Func` alone is 312 of that: seventeen declaration
//! fields, paid on every expression move. Boxing those two variants took the
//! enum to 144 (`Spanned<Node>` 160) and cost, on the profiling profile:
//!
//! | corpus                         | before Ir     | after Ir      |    |
//! |--------------------------------|---------------|---------------|----|
//! | E121's 1,791-function exhibit  |   492,588,405 |   462,602,072 | −6.09% |
//! | kolt's `client.vl`             | 5,475,728,661 | 5,301,416,240 | −3.18% |
//!
//! The next ceiling down is 136 — `Node::Trait` and `Node::StructInitializer`,
//! both four-field item declarations — and after those, 120 (`Node::Element`,
//! `Node::Struct`, `Node::Enum`). Boxing all five would land the enum at 96,
//! below which sits `Node::Call` at 88, which is an EXPRESSION and would trade
//! the memcpy for an allocation on the hot path. That is the residual, and the
//! bound below is set where the measurement actually is.
//!
//! **Why a ceiling and not the clippy lint.** `large_enum_variant` is allowed
//! workspace-wide with a comment claiming the memcpy is one "the profiler never
//! showed". The profiler showed it; the comment is corrected in `Cargo.toml`.
//! The lint still cannot make this call — it fires on a size RATIO between
//! variants and knows nothing about which of them the parser returns — so the
//! judgement stays here, in a number a profile produced.

use vilan_core::node::Node;
use vilan_core::span::Spanned;

/// The measured ceiling for `Node` itself, in bytes. Raising it is a
/// performance decision and belongs in a commit that says so.
const NODE_CEILING: usize = 144;

#[test]
fn node_stays_within_the_width_the_parser_was_measured_at() {
    let width = std::mem::size_of::<Node<'static>>();
    assert!(
        width <= NODE_CEILING,
        "`Node` is {width} bytes, over the {NODE_CEILING}-byte ceiling M32 \
         measured. The parser moves `Spanned<Node>` by value through every \
         precedence level, so this width is paid on every expression return — \
         box the variant that grew (`Node::Func` and `Node::MacroFun` already \
         are) rather than raising the ceiling, and if you do raise it, carry \
         the callgrind Ir that justifies it."
    );
}

#[test]
fn the_spanned_node_the_parser_returns_stays_within_it_too() {
    // What the recursion actually moves is `Option<Spanned<Node>>`, and
    // `Span` is two `usize`s: pinning `Node` alone would miss a `Span` that
    // grew. `Option` adds nothing — `Node`'s discriminant leaves a niche.
    let spanned = std::mem::size_of::<Spanned<Node<'static>>>();
    let optional = std::mem::size_of::<Option<Spanned<Node<'static>>>>();
    let ceiling = NODE_CEILING + std::mem::size_of::<vilan_core::span::Span>();
    assert!(
        spanned <= ceiling,
        "`Spanned<Node>` is {spanned} bytes, over the {ceiling}-byte ceiling — \
         see the module comment"
    );
    assert_eq!(
        optional, spanned,
        "`Option<Spanned<Node>>` must ride `Node`'s discriminant niche: the \
         parser returns the option, and a widening here doubles nothing but \
         the memcpy"
    );
}

/// The pin is only worth what its subject is worth: if `Func` ever stops being
/// the fat thing, the boxing above stops being the reason `Node` is small and
/// the ceiling above stops explaining itself. Asserting that `Func` is WIDER
/// than the enum that holds it is what keeps the two comments honest — and it
/// is what reds if someone "simplifies" `Node::Func(Box<Func>)` back to
/// `Node::Func(Func)` without reading either.
#[test]
fn func_is_wider_than_the_enum_that_boxes_it() {
    let func = std::mem::size_of::<vilan_core::node::Func<'static>>();
    let node = std::mem::size_of::<Node<'static>>();
    assert!(
        func > node,
        "`Func` is {func} bytes and `Node` is {node}: `Func` no longer \
         dominates the enum, so M32's boxing is no longer what keeps `Node` \
         narrow — re-measure before trusting the ceiling"
    );
}
