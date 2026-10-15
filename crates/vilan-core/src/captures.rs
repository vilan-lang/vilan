//! Which bindings a closure CAPTURES (spec §6.9) — one walk for both backends.
//!
//! A closure captures the binding, not its value, so a `mut` binding a
//! closure names is a place two frames share. The native backend boxes such a
//! binding into a counted cell (R3) and reads an argument rooted at it after
//! the call's other arguments (§6.9's native note); the JS backend shares the
//! place for free, and orders those arguments the same way (F138). Both ask
//! the one question this module answers, over one list of children — an arm
//! the list misses is a capture neither backend would see.

use std::collections::HashSet;
use std::hash::BuildHasher;

use crate::analyzer::{Closure, Expr, ExprIfBranch, ExprPattern, Intrinsic, Program};
use crate::fx::FxHashSet;
use crate::id::Id;
use crate::node::Convention;

/// Every sub-expression of `expr_id`, in evaluation order — a closure's body
/// among them — for the walks that only need to recurse.
pub fn expression_children(program: &Program, expr_id: Id) -> Vec<Id> {
    let Some(expr) = program.entity_map.get(&expr_id) else {
        return Vec::new();
    };
    let mut children = Vec::new();
    match expr {
        Expr::Assignment(a, b) | Expr::Binary(_, a, b) | Expr::Index(a, b) => {
            children.push(*a);
            children.push(*b);
        }
        Expr::Async(a)
        | Expr::Await(a)
        | Expr::Ascribe(a)
        | Expr::Unary(_, a)
        | Expr::Reference(a, _)
        | Expr::Dereference(a)
        | Expr::TryAssert(a)
        | Expr::Field(a, _, _)
        | Expr::TupleIndex(a, _, _)
        | Expr::ArrayLen(a, _)
        | Expr::Repeat(a, _) => children.push(*a),
        Expr::FunctionReturn(value) => children.extend(value.iter().copied()),
        Expr::Block((statements, tail)) => {
            children.extend(statements.iter().copied());
            children.push(*tail);
        }
        Expr::List(ids) | Expr::Tuple(ids) => children.extend(ids.iter().copied()),
        Expr::StructInitializer(_, fields) => children.extend(fields.values().copied()),
        Expr::For(condition, (statements, tail)) => {
            children.extend(condition.iter().copied());
            children.extend(statements.iter().copied());
            children.push(*tail);
        }
        Expr::ForEach(iterable, _, (statements, tail)) => {
            children.push(*iterable);
            children.extend(statements.iter().copied());
            children.push(*tail);
        }
        Expr::If(branch) => if_children(branch, &mut children),
        Expr::Match(subject, legs) => {
            children.push(*subject);
            for leg in legs {
                children.extend(leg.guard.iter().copied());
                children.push(leg.body);
            }
        }
        Expr::Is(subject, _) | Expr::Destructure(subject, _) => children.push(*subject),
        Expr::Lift(subject, _, continuation) => {
            children.push(*subject);
            children.push(*continuation);
        }
        Expr::LiftRegion(steps, body) => {
            children.extend(steps.iter().map(|(step, _, _)| *step));
            children.push(*body);
        }
        Expr::TupleComprehension(bindings, body) => {
            for (binder, source) in bindings {
                children.push(*binder);
                children.push(*source);
            }
            children.push(*body);
        }
        Expr::Call(call_id) => {
            if let Some(function_call) = program.function_calls.get(call_id) {
                children.push(function_call.subject_id);
                children.extend(function_call.argument_ids.iter().copied());
            }
        }
        Expr::Closure(closure_id) => {
            if let Some(closure) = program.closures.get(closure_id) {
                children.push(closure.return_);
            }
        }
        _ => {}
    }
    children
}

fn if_children(branch: &ExprIfBranch, children: &mut Vec<Id>) {
    match branch {
        ExprIfBranch::If(condition, (statements, tail), next) => {
            children.push(*condition);
            children.extend(statements.iter().copied());
            children.push(*tail);
            if let Some(next) = next {
                if_children(next, children);
            }
        }
        ExprIfBranch::Else((statements, tail)) => {
            children.extend(statements.iter().copied());
            children.push(*tail);
        }
    }
}

/// The bindings a pattern declares, in source order.
pub fn pattern_bindings(pattern: &ExprPattern, out: &mut Vec<Id>) {
    match pattern {
        ExprPattern::Binding(id) => out.push(*id),
        ExprPattern::Variant(_, _, payload) => {
            for sub in payload {
                pattern_bindings(sub, out);
            }
        }
        ExprPattern::Tuple(elements) => {
            for (element, _) in elements {
                pattern_bindings(element, out);
            }
        }
        ExprPattern::Array(elements) => {
            for element in elements {
                pattern_bindings(element, out);
            }
        }
        ExprPattern::Wildcard | ExprPattern::Literal(_) => {}
    }
}

/// [`pattern_bindings`] into a set — what the closure scan's declared-inside
/// side wants.
pub fn pattern_bindings_into<S: BuildHasher>(pattern: &ExprPattern, out: &mut HashSet<Id, S>) {
    let mut bindings = Vec::new();
    pattern_bindings(pattern, &mut bindings);
    out.extend(bindings);
}

/// A closure's own parameters, its destructured ones included — declared
/// inside it, so never a capture.
pub fn closure_parameter_bindings<S: BuildHasher + Default>(
    program: &Program,
    closure: &Closure,
) -> HashSet<Id, S> {
    let mut bindings: HashSet<Id, S> = closure.parameters.iter().copied().collect();
    for destructure in &closure.parameter_destructures {
        if let Some(Expr::Destructure(_, pattern)) = program.entity_map.get(destructure) {
            pattern_bindings_into(pattern, &mut bindings);
        }
    }
    bindings
}

/// Walks a closure body, collecting the bindings it DECLARES and the bindings
/// it READS. The difference is what it captured.
pub fn scan_closure<S: BuildHasher + Default>(
    program: &Program,
    expr_id: Id,
    declared: &mut HashSet<Id, S>,
    referenced: &mut HashSet<Id, S>,
    visited: &mut HashSet<Id, S>,
) {
    if !visited.insert(expr_id) {
        return;
    }
    match program.entity_map.get(&expr_id) {
        Some(Expr::Variable(binding)) => {
            declared.insert(*binding);
            if let Some(initial) = program
                .variables
                .get(binding)
                .and_then(|variable| variable.initial)
            {
                scan_closure(program, initial, declared, referenced, visited);
            }
        }
        Some(Expr::Local(binding)) => {
            referenced.insert(*binding);
        }
        Some(other) => {
            // A `let` is not the only way a body introduces a name (F20, and
            // F18's destructuring `let`). A match leg's pattern, an `is`
            // test's capture, a destructure's pattern, a `for` binder and a
            // nested closure's parameters all declare INSIDE, and a walk
            // that missed them called them captures: the capture prelude
            // then emitted `let live = live.clone();` for a binding that
            // only exists inside the leg it is bound in, and
            // `future_closure_argument` — which emits one clone per capture
            // — did the same for the three `match` captures and the two
            // destructured names in `std::http`'s response loop.
            match other {
                Expr::Match(_, legs) => {
                    for leg in legs {
                        pattern_bindings_into(&leg.pattern, declared);
                    }
                }
                Expr::Is(_, pattern) | Expr::Destructure(_, pattern) => {
                    pattern_bindings_into(pattern, declared);
                }
                Expr::ForEach(_, item, _) => declared.extend(item.iter().copied()),
                Expr::Closure(closure_id) => {
                    // A NESTED closure's destructured parameters too
                    // (`|(k, _)| k == key` inside `get_post_var`): they are
                    // not in `parameters`, and read as captures of the
                    // OUTER closure they were cloned before they existed.
                    if let Some(closure) = program.closures.get(closure_id) {
                        declared.extend(closure_parameter_bindings::<S>(program, closure));
                    }
                }
                _ => {}
            }
            for child in expression_children(program, expr_id) {
                scan_closure(program, child, declared, referenced, visited);
            }
        }
        None => {}
    }
}

/// R3/F47's table: every `mut` binding some closure captures — a `mut` local,
/// or a `mut` parameter received by value (a `lazy` one is a memo cell, not a
/// place). The native backend boxes each into a counted cell; both backends
/// read an argument rooted at one after the call's later arguments (spec
/// §6.9's view, F129/F138).
pub fn captured_mut_bindings(program: &Program) -> FxHashSet<Id> {
    let mut captured = FxHashSet::default();
    let mut closures: Vec<Id> = program.closures.keys().copied().collect();
    closures.sort_by_key(|closure| closure.0);
    for closure_id in closures {
        let Some(closure) = program.closures.get(&closure_id) else {
            continue;
        };
        // The closure's OWN parameters are declared inside it: `|mut v| { v =
        // v - 1; v }` writes its parameter and captures nothing.
        let mut declared_inside: FxHashSet<Id> = closure_parameter_bindings(program, closure);
        let mut referenced = FxHashSet::default();
        let mut visited = FxHashSet::default();
        scan_closure(
            program,
            closure.return_,
            &mut declared_inside,
            &mut referenced,
            &mut visited,
        );
        for binding in referenced {
            if declared_inside.contains(&binding) {
                continue;
            }
            let mutable_local = program
                .variables
                .get(&binding)
                .is_some_and(|variable| variable.mutable);
            let mutable_parameter = program.parameters.get(&binding).is_some_and(|parameter| {
                parameter.mutable
                    && !parameter.lazy
                    && matches!(parameter.convention, Convention::Bare | Convention::Own)
            });
            if mutable_local || mutable_parameter {
                captured.insert(binding);
            }
        }
    }
    captured
}

/// The intrinsics that write their receiver in place (a `List` pop, a map's
/// insert) — what a walk asking "does this write the binding" treats as a
/// `&mut self` call.
pub fn mutates_its_receiver(intrinsic: Intrinsic) -> bool {
    matches!(
        intrinsic,
        Intrinsic::ListPop
            | Intrinsic::ListRemove
            | Intrinsic::ListInsert
            | Intrinsic::ListSortBy
            | Intrinsic::MapInsert
            | Intrinsic::MapRemove
            | Intrinsic::SetInsert
            | Intrinsic::SetRemove
    )
}

/// The intrinsics that only READ their receiver, through a `&`, answer an
/// owned value and take no closure: a receiver in a captured binding is read
/// through a view of its cell for the call (F49/F103) — after the other
/// arguments — rather than copied out where it stands.
pub fn reads_its_receiver_in_place(intrinsic: Intrinsic) -> bool {
    matches!(
        intrinsic,
        Intrinsic::StrLen
            | Intrinsic::StrTrim
            | Intrinsic::StrToLowercase
            | Intrinsic::StrToUppercase
            | Intrinsic::StrContains
            | Intrinsic::StrStartsWith
            | Intrinsic::StrEndsWith
            | Intrinsic::StrReplace
            | Intrinsic::StrRepeat
            | Intrinsic::StrSplit
            | Intrinsic::StrSubstring
            | Intrinsic::ParseI32
            | Intrinsic::ParseF64
            | Intrinsic::ListLen
            | Intrinsic::ListGet
            | Intrinsic::SetContains
            | Intrinsic::SetLen
            | Intrinsic::MapGet
            | Intrinsic::MapContainsKey
            | Intrinsic::MapLen
            | Intrinsic::CanonicalHash
    )
}
