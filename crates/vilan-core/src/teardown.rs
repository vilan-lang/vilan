//! Where a teardown REGION ends — the one resolution both emitters call
//! (N154; destruction.md §5/§7 as amended by `lifetimes.md` §6, slice S3).
//!
//! The analyzer answers per BINDING ([`DropExtent`], `analyzer/liveness.rs`)
//! with the chain of statements enclosing its last read; an emitter walking a
//! block asks, per declaration, at which of the block's direct statements the
//! region the declaration opens CLOSES. The JS transformer closes a `finally`
//! there and the native emitter writes a `std::mem::drop` after it, so the two
//! backends print their teardowns in one order only while they agree on the
//! answer — which is why it is written once, here, pure over [`Program`],
//! rather than once per emitter (native-48's F97 copied it; N154 moved both
//! onto this).
//!
//! Every function here is a statement-index computation over a range
//! `statements[start..end]` the caller owns (a function body, an `if` arm, a
//! `match` leg, a loop body), with `end` EXCLUSIVE and every answer in
//! `start..=end`.

use crate::analyzer::{DropExtent, Expr, ExprPattern, Program};
use crate::id::Id;
use crate::type_::TypeId;

/// The bindings a direct statement of a block owes a teardown for, in
/// declaration order: a resource `let` still owned at its scope's end, or the
/// resource payloads a destructuring `let` captured out of a consumed subject
/// (B62). Empty for every other statement, and for a binding whose type
/// destroys nothing ([`drops_nontrivially`]).
pub fn statement_teardown(program: &Program<'_>, statement: Id) -> Vec<Id> {
    match program.entity_map.get(&statement) {
        Some(Expr::Variable(variable_id))
            if program.dropped_bindings.contains(variable_id)
                && binding_drops_nontrivially(program, *variable_id) =>
        {
            vec![*variable_id]
        }
        Some(Expr::Destructure(_, pattern)) => {
            let mut captures = Vec::new();
            pattern_captures(pattern, &mut captures);
            captures.retain(|capture| {
                program.dropped_bindings.contains(capture)
                    && binding_drops_nontrivially(program, *capture)
            });
            captures
        }
        _ => Vec::new(),
    }
}

/// Whether a value of `type_id` actually destroys something — a `Drop` impl
/// or a resource member — as opposed to a bare `resource external` leaf with
/// no destructor, whose scope-end drop is a no-op.
pub fn drops_nontrivially(program: &Program<'_>, type_id: TypeId) -> bool {
    program
        .drop_glue
        .get(&type_id)
        .is_some_and(|glue| glue.drop_method.is_some() || !glue.members.is_empty())
}

/// [`drops_nontrivially`] for a `let` binding's declared type.
pub fn binding_drops_nontrivially(program: &Program<'_>, variable_id: Id) -> bool {
    program
        .variables
        .get(&variable_id)
        .is_some_and(|variable| drops_nontrivially(program, variable.type_id))
}

/// A pattern's capture ids, in declaration (source) order.
pub fn pattern_captures(pattern: &ExprPattern, out: &mut Vec<Id>) {
    match pattern {
        ExprPattern::Binding(capture_id) => out.push(*capture_id),
        ExprPattern::Variant(_, _, payload) => {
            for sub_pattern in payload {
                pattern_captures(sub_pattern, out);
            }
        }
        ExprPattern::Tuple(elements) => {
            for (sub_pattern, _) in elements {
                pattern_captures(sub_pattern, out);
            }
        }
        ExprPattern::Array(elements) => {
            for sub_pattern in elements {
                pattern_captures(sub_pattern, out);
            }
        }
        ExprPattern::Wildcard | ExprPattern::Literal(_) => {}
    }
}

/// Where the teardown region of `bindings` ends — an EXCLUSIVE index into
/// `statements`, in `start..=end`. The region opens at `start` (the statement
/// after the declaration, or a body's entry for `own` parameters) and the
/// group discharges together at the LAST of its members' last reads.
///
/// Three refusals all answer `end`, the scope-end law that shipped: a member
/// the dataflow does not answer for (an opaque binding — a capture, a
/// cross-region read, an unfollowable loan), an explicit
/// [`DropExtent::ScopeEnd`], and a chain naming no statement of this range
/// (the read is in the scope's tail, or somewhere this walk does not emit).
/// A last read INSIDE a branch or a loop resolves to that branch or loop
/// statement, so the drop lands at the join and every path through it
/// releases at the one point — §6.3's drop specialization with no runtime
/// flag anywhere.
///
/// **Regions nest.** The region is widened until it covers the last read of
/// every name declared within it ([`widen_over_declarations`]): on the JS
/// backend the region is a block, and a `const` declared inside it dies at
/// its brace.
pub fn region_end(
    program: &Program<'_>,
    bindings: &[Id],
    statements: &[Id],
    start: usize,
    end: usize,
) -> usize {
    let own = own_region_end(program, bindings, statements, start, end);
    widen_over_declarations(program, own, statements, start, end)
}

/// One group's own extent, before nesting is taken into account: the
/// exclusive statement index its last read sits at, `start` when nothing
/// reads it, and `end` for every refusal.
fn own_region_end(
    program: &Program<'_>,
    bindings: &[Id],
    statements: &[Id],
    start: usize,
    end: usize,
) -> usize {
    let mut extent = start.min(end);
    for binding in bindings {
        let Some(binding_extent) = program.drop_extents.get(binding) else {
            return end;
        };
        extent = extent.max(resolve_extent(binding_extent, statements, start, end));
    }
    extent.min(end)
}

/// Grow `extent` until every name declared in `statements[start..extent]` has
/// its last read inside it — a fixpoint, since widening admits more
/// declarations. Monotone and bounded by `end`.
///
/// `statements[index]` is a DIRECT statement of the region being emitted,
/// which is exactly how `liveness::LastUse::declared_binding_extents` keys
/// its map: the innermost statement enclosing each declaration. The two sides
/// must agree, so a key measured from the enclosing function instead would
/// match only at a body's top level and silently skip every nested region
/// (B159). The widening question is deliberately SYNTACTIC
/// (`liveness::LastUse::syntactic_extent`): block scope is about where a name
/// may be written down, not about when a value may be destroyed.
fn widen_over_declarations(
    program: &Program<'_>,
    mut extent: usize,
    statements: &[Id],
    start: usize,
    end: usize,
) -> usize {
    loop {
        let mut widened = extent;
        for index in start..extent {
            let Some(declared) = program.declared_binding_extents.get(&statements[index]) else {
                continue;
            };
            for binding_extent in declared {
                // Measured from the declaring statement itself, not after it:
                // a `for` item or an `is` capture has its last read INSIDE the
                // statement that declares it, and resolving from the next one
                // would find no chain element and refuse.
                widened = widened.max(resolve_extent(binding_extent, statements, index, end));
            }
        }
        if widened == extent {
            return extent;
        }
        extent = widened;
    }
}

/// One [`DropExtent`] resolved against a statement range: the exclusive index
/// its last read sits at, `start` when nothing reads it, and `end` for every
/// refusal (an explicit scope end, or a chain naming no statement of this
/// range — the read is in the scope's tail).
fn resolve_extent(extent: &DropExtent, statements: &[Id], start: usize, end: usize) -> usize {
    let start = start.min(end);
    match extent {
        DropExtent::ScopeEnd => end,
        DropExtent::Declaration => start,
        DropExtent::Statement(chain) => {
            let region = &statements[start..end];
            match chain
                .iter()
                .find_map(|holder| region.iter().position(|statement| statement == holder))
            {
                Some(offset) => start + offset + 1,
                None => end,
            }
        }
    }
}
