//! Three LIFETIME steers over resolved calls: a derivation built where it will
//! outlive the scope that owns it (tracker A135 door c, A136 door b, A141's
//! door a under R-f).
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
//!   registration lives as long as the handler's owner: the connection's under
//!   `Service::factory` (A135 door b), the service's under `Service::new` (A141
//!   door b); before A135, the process's. Return a cell that outlives the call.
//! - **A store written in an `[rpc]` handler** (A141, R-f). A handler runs
//!   under the connection's owner when its instance is per connection
//!   (`Service::factory`), and a structure the instances share — a
//!   module-level store — outlives that connection. A `.cell()` the handler
//!   STORES there (through `Shared::write`, or built in a `Shared<Option<T>>::
//!   get_or_insert` maker) dies with the connection that built it while the
//!   store keeps handing it out. A `Memo` maker is the first steer's, raised
//!   everywhere.
//!
//! Both are WARNINGS and both are STATIC and SYNTACTIC in the rulings' sense:
//! the first reads the calls a maker closure literal makes DIRECTLY (a closure
//! the maker merely creates is its own node, and inert until something runs
//! it); the second reads the method's TAIL expression (a `ret` elsewhere in the
//! body is not followed); the third reads a handler's own statements — the
//! value stored through a `write()` view (assigned, or an argument of a method
//! called on the view) and the calls a `get_or_insert` maker literal makes
//! directly. Code inside the standard library is exempt, as every
//! std-internal use of a deprecation is: std's own seams are written knowingly.

use crate::analyzer::{Expr, Program, SourceId};
use crate::call_graph::CallTarget;
use crate::error::Error;
use crate::fx::{FxHashMap as HashMap, FxHashSet as HashSet};
use crate::id::Id;
use crate::span::Span;

/// Run the three steers and add their warnings to the program.
pub fn check_lifetime_steers(program: &mut Program) {
    let found = {
        let targets = call_targets(program);
        let mut found = memo_maker_warnings(program, &targets);
        found.extend(handle_tail_warnings(program, &targets));
        found.extend(handler_store_warnings(program, &targets));
        found
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

/// The `external` functions named `name` declared in the std file `file` —
/// [`std_functions`]' twin for a leaf with no body (`Shared::write`).
fn std_externals(program: &Program, file: &str, names: &[&str]) -> HashSet<Id> {
    program
        .external_functions
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

/// The owner-taking calls a maker must not make: std's `.cell()` and every std
/// `effect` spelling (the trait's and the mirrors' overrides alike).
fn owner_taking_functions(program: &Program) -> HashSet<Id> {
    let mut owner_taking = std_functions(program, "reactive.vl", &["cell", "memo"]);
    for function in program.functions.values() {
        if !matches!(
            function.name,
            "effect" | "effect_on_change" | "scoped_effect"
        ) {
            continue;
        }
        if let Some(source) = program.source_of(function.id)
            && program.std_sources.contains(&source)
        {
            owner_taking.insert(function.id);
        }
    }
    owner_taking
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

/// The name a warning quotes for an owner-taking callee.
fn callee_label(program: &Program, callee: Id) -> String {
    match program.functions.get(&callee) {
        Some(function) if function.name == "cell" => "`.cell()`".to_string(),
        Some(function) if function.name == "memo" => "`.memo()`".to_string(),
        Some(function) => format!("`{}`", function.name),
        None => "this call".to_string(),
    }
}

/// A136 door (b): an owner-taking call syntactically inside a `Memo` maker.
fn memo_maker_warnings(
    program: &Program,
    targets: &HashMap<Id, CallTarget>,
) -> Vec<(Error, SourceId)> {
    let makers = std_functions(program, "memo.vl", &["get_or", "get_or_insert"]);
    if makers.is_empty() {
        return Vec::new();
    }
    let owner_taking = owner_taking_functions(program);
    let graph = program.call_graph();
    let mut found = Vec::new();
    for (call_id, target) in targets {
        let CallTarget::Function(callee) = target else {
            continue;
        };
        if !makers.contains(callee) || !in_user_code(program, *call_id) {
            continue;
        }
        let Some(maker) = program
            .function_calls
            .get(call_id)
            .and_then(|call| call.argument_ids.last())
            .and_then(|argument| closure_of(program, *argument))
        else {
            continue;
        };
        for inner in graph.calls_of(maker) {
            // The NAMED member, whatever the dispatch: `source.effect(..)` on a
            // concrete cell is a trait default the transformer re-dispatches per
            // type (an indirect target in the graph), and it is the same
            // owner-taking call.
            let Some(inner_callee) = named_callee(program, inner.call_id) else {
                continue;
            };
            if !owner_taking.contains(&inner_callee) {
                continue;
            }
            let label = callee_label(program, inner_callee);
            found.push(program.anchored(
                Error {
                    trace: Vec::new(),
                    span: call_span(program, inner.call_id),
                    msg: format!(
                        "{label} inside a `Memo` maker ties what it builds to the FIRST \
                         caller's owner, and the memo keeps it after that owner is gone: \
                         every later ask is answered with a dead one. What a maker builds \
                         outlives the caller — a derivation in a maker is sealed with \
                         `.memo_global()` (`.cell_global()` for the writable face), and a lease \
                         (`.memo()`, `.cell()`, `effect`) belongs at the call site, on what the \
                         memo answers"
                    ),
                    note: None,
                },
                inner.call_id,
            ));
        }
    }
    found
}

/// A135 door (c): a handle-returning `[rpc]` method whose tail is `.cell()` or
/// `.memo()` — the warning names whichever it is.
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
        &[
            "reply_source",
            "reply_source_option",
            "reply_source_keyed",
            "reply_source_memo",
            "reply_source_memo_option",
        ],
    );
    if repliers.is_empty() {
        return Vec::new();
    }
    let derivations = std_functions(program, "reactive.vl", &["cell", "memo"]);
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
        // The seal the tail names (`.cell()` or `.memo()`), and its program-lifetime
        // twin for the steer — the warning names what the body wrote.
        let seal = named_callee(program, tail_call)
            .and_then(|callee| program.functions.get(&callee))
            .map(|callee| callee.name)
            .unwrap_or("cell");
        found.push(program.anchored(
            Error {
                trace: Vec::new(),
                span: call_span(program, tail_call),
                msg: format!(
                    "`{name}` returns a signal handle it builds with `.{seal}()` on every call: \
                     each call mints a fresh cell, so the reply never matches a channel this \
                     connection already carries (a capability and a forward per call), and \
                     the cell's subscription lives as long as the handler's owner (the \
                     connection under `Service::factory`, the service under `Service::new`). \
                     Return a cell that outlives the call — keep it on the service, keyed by \
                     the arguments (a `Memo` whose maker writes `.{seal}_global()`)"
                ),
                note: None,
            },
            tail_call,
        ));
    }
    found
}

/// A141 (R-f, door a's warning): a `.cell()` an `[rpc]` handler STORES on a
/// structure that outlives the call — through `Shared::write` (assigned to the
/// view, or an argument of a method called on it) or built directly in a
/// `Shared<Option<T>>::get_or_insert` maker.
///
/// A handler is found through the generated route that runs it: every route
/// the `[service]` expansion writes wraps its turn in `rpc::under_connection(
/// __request, || ..)`, and the user functions the route's closures call are
/// the handlers.
fn handler_store_warnings(
    program: &Program,
    targets: &HashMap<Id, CallTarget>,
) -> Vec<(Error, SourceId)> {
    let wrappers = std_functions(program, "rpc.vl", &["under_connection"]);
    if wrappers.is_empty() {
        return Vec::new();
    }
    let writes = std_externals(program, "shared.vl", &["write"]);
    let makers = std_functions(program, "shared.vl", &["get_or_insert"]);
    let derivations = std_functions(program, "reactive.vl", &["cell", "memo"]);
    let graph = program.call_graph();
    // The handlers, and every call their bodies make directly.
    let mut handlers: Vec<Id> = Vec::new();
    for (call_id, target) in targets {
        let CallTarget::Function(callee) = target else {
            continue;
        };
        if !wrappers.contains(callee) || !in_user_code(program, *call_id) {
            continue;
        }
        let mut closures: Vec<Id> = program
            .function_calls
            .get(call_id)
            .and_then(|call| call.argument_ids.last())
            .and_then(|argument| closure_of(program, *argument))
            .into_iter()
            .collect();
        while let Some(closure) = closures.pop() {
            for inner in graph.calls_of(closure) {
                if let CallTarget::Function(function) = inner.target
                    && in_user_code(program, function)
                    && program.functions.contains_key(&function)
                    && !handlers.contains(&function)
                {
                    handlers.push(function);
                }
                if let Some(call) = program.function_calls.get(&inner.call_id) {
                    closures.extend(
                        call.argument_ids
                            .iter()
                            .filter_map(|argument| closure_of(program, *argument)),
                    );
                }
            }
        }
    }
    let mut handler_calls: HashSet<Id> = HashSet::default();
    for handler in &handlers {
        for inner in graph.calls_of(*handler) {
            handler_calls.insert(inner.call_id);
        }
    }
    let is_write = |expression: Id| -> bool {
        let mut expression = expression;
        while let Some(Expr::Dereference(inner)) = program.entity_map.get(&expression) {
            expression = *inner;
        }
        match program.entity_map.get(&expression) {
            Some(Expr::Call(call_id)) => matches!(
                targets.get(call_id),
                Some(CallTarget::External(callee)) if writes.contains(callee)
            ),
            _ => false,
        }
    };
    let mut stored: Vec<Id> = Vec::new();
    // Assigned to a `write()` view: `self.slot.write() = Some(… .cell())`.
    for expression in program.entity_map.values() {
        let Expr::Assignment(target, value) = expression else {
            continue;
        };
        if !is_write(*target) {
            continue;
        }
        if let Some(cell) = tail_derivation(program, targets, &derivations, *value)
            && handler_calls.contains(&cell)
        {
            stored.push(cell);
        }
    }
    for call_id in &handler_calls {
        let Some(call) = program.function_calls.get(call_id) else {
            continue;
        };
        // An argument of a method called on a `write()` view:
        // `self.kept.write().push(… .cell())`.
        if call
            .argument_ids
            .first()
            .is_some_and(|receiver| is_write(*receiver))
        {
            for argument in call.argument_ids.iter().skip(1) {
                if let Some(cell) = tail_derivation(program, targets, &derivations, *argument) {
                    stored.push(cell);
                }
            }
        }
        // Built in a `Shared<Option<T>>::get_or_insert` maker (I8).
        if named_callee(program, *call_id).is_some_and(|callee| makers.contains(&callee))
            && let Some(maker) = call
                .argument_ids
                .last()
                .and_then(|argument| closure_of(program, *argument))
        {
            for inner in graph.calls_of(maker) {
                if named_callee(program, inner.call_id)
                    .is_some_and(|callee| derivations.contains(&callee))
                {
                    stored.push(inner.call_id);
                }
            }
        }
    }
    stored.sort_unstable_by_key(|call_id| call_id.0);
    stored.dedup();
    stored
        .into_iter()
        .map(|cell| {
            let label = named_callee(program, cell)
                .map(|callee| callee_label(program, callee))
                .unwrap_or_else(|| "`.cell()`".to_string());
            program.anchored(
                Error {
                    trace: Vec::new(),
                    span: call_span(program, cell),
                    msg: format!(
                        "{label} stored on a structure that outlives the call, in an `[rpc]` \
                         handler: the seal is owned by the handler's owner — the CONNECTION's \
                         under `Service::factory` — so a store every connection shares keeps \
                         handing it out dead once that connection closes. A derivation cached \
                         across calls is `.memo_global()` or `.cell_global()`"
                    ),
                    note: None,
                },
                cell,
            )
        })
        .collect()
}

/// The function a call NAMES — the declaration its subject resolved to — before
/// any per-type re-dispatch. `None` for a call through a value.
fn named_callee(program: &Program, call_id: Id) -> Option<Id> {
    let subject = program.function_calls.get(&call_id)?.subject_id;
    match program.entity_map.get(&subject)? {
        Expr::Local(target) if program.functions.contains_key(target) => Some(*target),
        _ => None,
    }
}

/// The closure an expression IS (a literal written in place), if any.
fn closure_of(program: &Program, expression: Id) -> Option<Id> {
    match program.entity_map.get(&expression)? {
        Expr::Closure(closure) => Some(*closure),
        _ => None,
    }
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
            CallTarget::Variant(_) => {
                let payload = program.function_calls.get(call_id)?.argument_ids.first()?;
                tail_derivation(program, targets, derivations, *payload)
            }
            // The named member, as the maker check reads it: a per-type
            // re-dispatch is still a call to `.cell()`.
            _ => named_callee(program, *call_id)
                .filter(|callee| derivations.contains(callee))
                .map(|_| *call_id),
        },
        _ => None,
    }
}
