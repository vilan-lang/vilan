//! Two LIFETIME steers over resolved calls: a derivation built where it will
//! outlive the scope that owns it (tracker A135 door c, A136 door b).
//!
//! `std::reactive`'s `.cell()` is OWNER-TIED: its registration on the upstream
//! goes to the ambient owner and is released with it. Two std seams hold what a
//! call site builds for LONGER than that call site's owner, and a `.cell()`
//! written into either is a bug the type system cannot see:
//!
//! - **A `Memo` maker** (A136). `memo.get_or_insert(key, || …)` runs the maker
//!   once, under the FIRST caller's owner, and keeps what it built for the life
//!   of the memo. A `.cell()` (or an `effect`) made there dies with that first
//!   caller — the page that asked first — and every later ask is answered with
//!   the dead cell: stale on return, never updating again. What the maker builds
//!   outlives the caller, so a derivation in a maker is `.cell_global()` and a
//!   lease belongs at the call site, on what the memo answers.
//! - **A handle-returning `[rpc]` method** (A135). The route exports whatever the
//!   method answers through the connection's capability table, deduped by the
//!   CELL's identity (A92). A body whose tail is `.cell()` mints a fresh cell per
//!   call, so the dedup never hits — a capability and a forward per call for a
//!   value the connection already carries — and the cell's upstream
//!   registration lives until the connection closes (the dispatcher runs the
//!   handler under the connection's owner since A135 door b; before that, for
//!   the process). Return a cell that outlives the call.
//!
//! Both are WARNINGS and both are STATIC and SYNTACTIC in the rulings' sense:
//! the first reads the calls a maker closure literal makes DIRECTLY (a closure
//! the maker merely creates is its own node, and inert until something runs
//! it); the second reads the method's TAIL expression (a `ret` elsewhere in the
//! body is not followed). Code inside the standard library is exempt, as every
//! std-internal use of a deprecation is: std's own seams are written knowingly.

use crate::analyzer::{Expr, Program, SourceId};
use crate::call_graph::CallTarget;
use crate::error::Error;
use crate::fx::{FxHashMap as HashMap, FxHashSet as HashSet};
use crate::id::Id;
use crate::span::Span;

/// Run both steers and add their warnings to the program.
pub fn check_lifetime_steers(program: &mut Program) {
    let found = {
        let targets = call_targets(program);
        handle_tail_warnings(program, &targets)
    };
    let mut seen: HashSet<(u32, usize, usize)> = HashSet::default();
    for (warning, source) in found {
        // One site, one warning: a package with two entries analyzes a shared
        // module once per entry world.
        if !seen.insert((source.0, warning.span.start, warning.span.end)) {
            continue;
        }
        program
            .warning_sources
            .resize(program.warnings.len(), source);
        program.warnings.push(warning);
        program.warning_sources.push(source);
    }
}

/// Every resolved call in the program, by its call id.
fn call_targets(program: &Program) -> HashMap<Id, CallTarget> {
    let graph = program.call_graph();
    let mut targets = HashMap::default();
    for node in graph.nodes() {
        for call in graph.calls_of(node.id()) {
            targets.insert(call.call_id, call.target);
        }
    }
    targets
}

/// The functions named `name` declared in the std file `file` (a path relative
/// to std's `src/`, compared by its trailing components).
fn std_functions(program: &Program, file: &str, names: &[&str]) -> HashSet<Id> {
    program
        .functions
        .values()
        .filter(|function| names.contains(&function.name))
        .filter_map(|function| {
            let source = program.source_of(function.id)?;
            let in_file = program.std_sources.contains(&source)
                && program
                    .sources
                    .get(source.0 as usize)
                    .is_some_and(|path| path.ends_with(file));
            in_file.then_some(function.id)
        })
        .collect()
}

/// Whether `id` sits in user code (not in the standard library).
fn in_user_code(program: &Program, id: Id) -> bool {
    program
        .source_of(id)
        .is_some_and(|source| !program.std_sources.contains(&source))
}

/// The span a warning about a call anchors at: the member name when it is a
/// method call (`.cell()`), else the whole call.
fn call_span(program: &Program, call_id: Id) -> Span {
    program
        .member_name_spans
        .get(&call_id)
        .copied()
        .or_else(|| program.span_map.get(&call_id).map(|span| **span))
        .unwrap_or(Span { start: 0, end: 0 })
}

/// A135 door (c): a handle-returning `[rpc]` method whose tail is `.cell()`.
///
/// A method is known to return a handle by the generated route that exports
/// its answer: the `[service]` expansion writes `rpc::reply_source*(__request,
/// self.<method>(..))` for exactly those methods, so the argument of that call
/// names the method, resolved.
fn handle_tail_warnings(
    program: &Program,
    targets: &HashMap<Id, CallTarget>,
) -> Vec<(Error, SourceId)> {
    let repliers = std_functions(
        program,
        "rpc.vl",
        &["reply_source", "reply_source_option", "reply_source_keyed"],
    );
    if repliers.is_empty() {
        return Vec::new();
    }
    let derivations = std_functions(program, "reactive.vl", &["cell"]);
    let mut methods: Vec<Id> = Vec::new();
    for (call_id, target) in targets {
        let CallTarget::Function(callee) = target else {
            continue;
        };
        if !repliers.contains(callee) {
            continue;
        }
        let Some(method) = program
            .function_calls
            .get(call_id)
            .and_then(|call| call.argument_ids.get(1))
            .and_then(|argument| called_function(program, targets, *argument))
        else {
            continue;
        };
        if in_user_code(program, method) && !methods.contains(&method) {
            methods.push(method);
        }
    }
    let mut found = Vec::new();
    for method in methods {
        let Some(function) = program.functions.get(&method) else {
            continue;
        };
        let Some(tail_call) = tail_derivation(program, targets, &derivations, function.body.1)
        else {
            continue;
        };
        let name = function.name;
        found.push(program.anchored(
            Error {
                trace: Vec::new(),
                span: call_span(program, tail_call),
                msg: format!(
                    "`{name}` returns a signal handle it builds with `.cell()` on every call: \
                     each call mints a fresh cell, so the reply never matches a channel this \
                     connection already carries (a capability and a forward per call), and \
                     the cell's subscription lives until the connection closes. Return a cell \
                     that outlives the call — keep it on the service, keyed by the arguments \
                     (a `Memo` whose maker writes `.cell_global()`)"
                ),
                note: None,
            },
            tail_call,
        ));
    }
    found
}

/// The function a call expression resolves to, if it is a direct call.
fn called_function(
    program: &Program,
    targets: &HashMap<Id, CallTarget>,
    expression: Id,
) -> Option<Id> {
    let Expr::Call(call_id) = program.entity_map.get(&expression)? else {
        return None;
    };
    match targets.get(call_id)? {
        CallTarget::Function(function) => Some(*function),
        _ => None,
    }
}

/// The call id of a derivation constructor standing at `expression`'s tail:
/// the expression itself, a block's tail, or the payload of a `Some(..)`.
fn tail_derivation(
    program: &Program,
    targets: &HashMap<Id, CallTarget>,
    derivations: &HashSet<Id>,
    expression: Id,
) -> Option<Id> {
    match program.entity_map.get(&expression)? {
        Expr::Block((_statements, tail)) => tail_derivation(program, targets, derivations, *tail),
        Expr::Call(call_id) => match targets.get(call_id)? {
            CallTarget::Function(callee) if derivations.contains(callee) => Some(*call_id),
            CallTarget::Variant(_) => {
                let payload = program.function_calls.get(call_id)?.argument_ids.first()?;
                tail_derivation(program, targets, derivations, *payload)
            }
            _ => None,
        },
        _ => None,
    }
}
