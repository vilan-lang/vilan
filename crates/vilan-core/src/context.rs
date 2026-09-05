//! The `context` threading pass: compiles `std::context::Context` away by
//! threading each context's value as a hidden parameter through every function
//! that transitively reads it, and capturing it into closures that read it.
//!
//! A context is a `Context::new()` value referenced by name (`count_context`).
//! `count_context.run(value, body)` makes `value` the context's value for the
//! dynamic extent of `body`; `count_context.get()` reads it. The pass:
//!
//!   1. Finds every `get`/`run`/`new` site and the context (the receiver
//!      binding) each refers to.
//!   2. Infers, over the [call graph](crate::call_graph), the set of functions
//!      and closures that transitively reach a `get` — these "need" the context
//!      (backward reachability from `get` sites; a closure passed to `run` is a
//!      natural boundary because `run` is an external call, so the need never
//!      propagates past it to the caller).
//!   3. Checks coverage per call: every needs-context node must receive the
//!      value — a function from a needs-context caller, a `run` closure from
//!      `run`, a captured closure from its definition scope. A node that can be
//!      entered without the value (a needs-context `main`, a global initializer,
//!      a needs-context function reachable only indirectly) is a compile error,
//!      not a silent miscompile.
//!   4. Rewrites the IR: appends the hidden parameter to each needs-context
//!      function and `run` closure, threads it at every call, replaces `get()`
//!      with a read of the in-scope parameter, lowers `run(value, body)` to
//!      `body(value)`, and lowers `Context::new()` to an opaque value.
//!
//! The pass is a no-op for programs that never create a `Context`, so it can't
//! change the output of any existing program.

use std::collections::VecDeque;

use crate::analyzer::{Expr, Program, SourceId};
use crate::call_graph::{CallGraph, CallTarget, IndirectReason, Node};
use crate::error::{Error, Note, TraceHop};
use crate::fx::{FxHashMap as HashMap, FxHashSet as HashSet};
use crate::id::Id;
use crate::type_::Type;

/// How many upstream calls a coverage refusal's requirement trace labels
/// (backlog E78) before it elides the rest behind an honest "… N more" tail.
/// Six keeps a full report about a screen tall: chains deeper than that are
/// recursion- or framework-shaped noise past the point where the reader has
/// seen where the requirement enters, and the ENTRY side is what the cap
/// keeps — the outermost uncovered frames, where the missing `run` belongs —
/// while the read end is always visible anyway as the primary (or its C3
/// note, when the read sits in std).
const TRACE_CAP: usize = 6;

/// Entry point: thread every context in `program`, or record diagnostics if any
/// context is read where its value can't be supplied.
///
/// Returns the call graph it built — but ONLY on the paths where it applied no
/// rewrite, in which case the graph still describes the program and the rest of
/// the analysis tail can share it instead of building a second one (E35). When
/// [`apply`] ran, the graph is stale by construction — the rewrite deletes call
/// edges (a threaded `get()` becomes an `Expr::Local` read, a consumed `run`
/// becomes `Expr::Null`) and mints new ones (the hidden context argument) —
/// and `None` says so. Returning it is unreachable on that path rather than
/// merely discouraged, which is the point of spelling the answer as an
/// `Option`: this is the one graph in the pipeline that cannot be shared, and
/// a comment would not have stopped anyone.
pub fn thread_contexts(program: &mut Program) -> Option<CallGraph> {
    let (Some(get_fn), Some(run_fn), Some(new_fn)) = (
        program.context_get_fn_id,
        program.context_run_fn_id,
        program.context_new_fn_id,
    ) else {
        // `context.vl` wasn't loaded — no contexts to thread, and no graph
        // built to hand on.
        return None;
    };
    // Absent only against an older `context.vl` without `get_safe`.
    let get_safe_fn = program.context_get_safe_fn_id;

    let graph = CallGraph::build(program);
    let plan = match analyze(program, &graph, get_fn, get_safe_fn, run_fn, new_fn) {
        Ok(plan) => plan,
        Err(errors) => {
            for (error, source) in errors {
                program.push_diagnostic(error, source);
            }
            // Diagnostics only: the tables are untouched, so the graph stands.
            return Some(graph);
        }
    };

    // Publish the context-dependent nodes (functions / `run` closures that take a
    // hidden context parameter) so `check_context_drops` can reject a `drop` body
    // that requires an ambient context (destruction.md §8). Not a graph input.
    program.context_dependent_functions = plan.param_nodes.iter().map(|(_, node)| *node).collect();

    // E124: the bindings this pass recognized as ambient contexts, published
    // for the dead-item paint. Recorded BEFORE the `is_empty` short-circuit
    // below: a context that is created and never read is rewritten by nothing
    // and is exactly as load-bearing a declaration as one that is
    // (`dead-code-paint.md` §1.7). Not a graph input either.
    program.context_bindings = plan.contexts.clone();

    if plan.is_empty() {
        // Nothing to rewrite — the common case, since most programs create no
        // context at all. The graph is still the program's.
        return Some(graph);
    }
    drop(graph);
    apply(program, plan);
    None
}

/// A `get()`/`get_safe()` call: the call entity, the context it reads, the
/// function or closure it sits in, and the read's FLAVOR — a strict `get`
/// demands the bare value (and the coverage fence); a `safe` read receives
/// `Option<T>` and never fences (reactive-turns.md §5.1).
struct GetSite {
    call_id: Id,
    context: Id,
    owner: Node,
    safe: bool,
}

/// A `run(value, body)` call: the call entity, the context, the value argument,
/// the body argument entity (the call's new subject), and — for a closure
/// LITERAL body — the closure. `None` when the body is an injected
/// `context`-typed closure VALUE (proposal/ambient-owner.md §5), which
/// carries its own hidden parameter and needs no capture marking.
struct RunSite {
    call_id: Id,
    value_id: Id,
    closure_entity: Id,
    closure_id: Option<Id>,
}

/// How a threaded call site obtains one context's argument: the caller's
/// own parameter (bare or already-`Option`), that parameter `Some`-wrapped
/// (the covered→safe boundary), or a literal `None` (an entry point with no
/// value — a top-level call, or the inlined entry `main`).
enum ThreadForm {
    Param { owner: Node },
    WrapSome { owner: Node },
    NoneLiteral,
}

/// The rewrite to apply once analysis succeeds. Node ids are sorted/owned so the
/// plan outlives the borrow of the call graph.
#[derive(Default)]
struct Plan {
    contexts: Vec<Id>,
    /// Nodes (functions and `run` closures) that receive their own hidden
    /// parameter, as `(context, node)`.
    param_nodes: Vec<(Id, Id)>,
    /// Captured closures that read the context from an enclosing node, as
    /// `(context, closure, provider node)` — the closure reuses the provider's
    /// parameter rather than taking its own.
    captures: Vec<(Id, Id, Id)>,
    /// `get()`/`get_safe()` calls to replace with a read of the in-scope
    /// parameter; `wrap_some` marks a safe read inside a STRICT holder, whose
    /// bare value must be `Some`-wrapped.
    gets: Vec<(Id, Id, Node, bool)>,
    /// Calls to needs-context functions, to thread one context's argument
    /// into, as `(call, context, form)`. ONE channel for every append, built
    /// context-by-context, so a call site's arguments accumulate in
    /// `contexts` order — matching the callee's parameter order — whatever
    /// mix of forms it needs.
    thread_calls: Vec<(Id, Id, ThreadForm)>,
    /// Safe reads INSIDE the entry `main`, which the transformer inlines at
    /// top level (it can carry no hidden parameter): each becomes a literal
    /// `None`.
    none_gets: Vec<Id>,
    /// The `Option::Some` / `Option::None` variant entities, for synthesizing
    /// wraps; resolved once when any safe site exists.
    some_variant: Option<Id>,
    none_variant: Option<Id>,
    /// `run` calls to lower to `body(value)`.
    runs: Vec<RunSite>,
    /// `Context::new()` calls to lower to an opaque value.
    news: Vec<Id>,
    /// Spawn registration (async-polymorphism.md Part B): each `async` spawn
    /// entity that can see the ambient nursery, as `(spawn entity, context,
    /// owner, owner holds the bare value)`. The rewrite records a read of the
    /// owner's parameter into `Program::spawn_nursery_sources`; the
    /// transformer passes it as `__task`'s third argument.
    spawns: Vec<(Id, Id, Node, bool)>,
}

impl Plan {
    fn is_empty(&self) -> bool {
        self.gets.is_empty()
            && self.runs.is_empty()
            && self.news.is_empty()
            && self.spawns.is_empty()
    }
}

/// The entity a call's subject resolves to, if it is a direct `Expr::Local`.
fn call_target(program: &Program, call_id: Id) -> Option<Id> {
    let subject_id = program.function_calls.get(&call_id)?.subject_id;
    match program.entity_map.get(&subject_id)? {
        Expr::Local(target) => Some(*target),
        _ => None,
    }
}

/// The binding an entity resolves to, if it is a direct `Expr::Local`.
fn local_target(program: &Program, entity_id: Id) -> Option<Id> {
    match program.entity_map.get(&entity_id)? {
        Expr::Local(target) => Some(*target),
        _ => None,
    }
}

/// A diagnostic anchored at an entity: the message at that entity's span, with
/// the file the span indexes into (backlog E16). This pass walks the whole
/// program, so the file comes from the anchor — never from "the entry".
fn anchored(program: &Program, anchor: Id, msg: String) -> (Error, SourceId) {
    program.anchored(
        Error {
            trace: Vec::new(),
            note: None,
            span: span_of(program, anchor),
            msg,
        },
        anchor,
    )
}

/// [`anchored`], carrying the E78 requirement trace (one label per uncovered
/// user-written call, ordered entry → site) and, when the offending site sits
/// in library code (std or a dependency package — C3a/E84), the C3 note that
/// demotes the library frame.
fn anchored_tracing(
    program: &Program,
    anchor: Id,
    msg: String,
    trace: Vec<TraceHop>,
    note: Option<Note>,
) -> (Error, SourceId) {
    program.anchored(
        Error {
            note,
            trace,
            span: span_of(program, anchor),
            msg,
        },
        anchor,
    )
}

/// The C3 note that demotes a library-internal site under a user-anchored
/// primary (diagnostics-standard.md A2, widened by C3a/E84 from std to any
/// dependency package): the site's own span, labeled with the library
/// function whose body holds it (`the read is inside `get_owner` here`), in
/// its own file. `what` names the site's role — "read" for a strict `get`,
/// "injected call" for a call through a `context`-clause closure.
fn library_frame_note(
    program: &Program,
    graph: &CallGraph,
    site: Id,
    site_owner: Id,
    what: &str,
    anchor: Id,
) -> Note {
    // The nearest enclosing FUNCTION of the site's owner node (a closure hops
    // its lexical parents), for the label; a site with no named holder (a
    // module initializer's read) names its package instead.
    let mut current = site_owner;
    let holder = loop {
        if let Some(function) = program.functions.get(&current) {
            break function.name.to_string();
        }
        match graph.closure_parent_of(current) {
            Some(parent) => current = parent,
            None => break package_name_of(program, site),
        }
    };
    Note {
        span: span_of(program, site),
        msg: format!("the {what} is inside `{holder}` here"),
        // The `Note::source` contract: name the file only when it differs
        // from the primary span's.
        source: program
            .note_source_of(site)
            .filter(|source| Some(*source) != program.note_source_of(anchor)),
    }
}

/// The name a demoted site's package was recorded under in `layer_platforms`
/// (the same canonicalized containment test platform coloring uses): `std`,
/// an entry dependency's import name, or the literal `a dependency` for a
/// package only reachable transitively. The `std` fallback keeps the label
/// honest for a std source that predates `layer_platforms` recording.
fn package_name_of(program: &Program, site: Id) -> String {
    program
        .source_of(site)
        .and_then(|source| program.canonical_sources.get(source.0 as usize))
        .and_then(|path| {
            program
                .layer_platforms
                .iter()
                .find(|(root, _, _, _)| path.starts_with(root))
                .map(|(_, name, _, _)| name.clone())
        })
        .unwrap_or_else(|| "std".to_string())
}

fn span_of(program: &Program, id: Id) -> crate::span::Span {
    program
        .span_map
        .get(&id)
        .map(|span| **span)
        .unwrap_or(crate::span::Span { start: 0, end: 0 })
}

/// Where a note ABOUT A CALL points: the callee's own name when the analyzer
/// recorded one, the whole call expression otherwise.
///
/// A method call's span starts at the head of its receiver chain — `root =
/// root.child(row` / `.render_body())` is one call, and its span is both of
/// those lines — so a note that takes it underlines the whole chain and says
/// "this call" about all of it; in a component-shaped body the chain IS the
/// body (B229). `member_name_spans` holds the precise callee span for exactly
/// the calls that have one (a method's `.render_body`); a plain `label()` has
/// no member name and its own span is already tight.
fn call_anchor_span(program: &Program, call: Id) -> crate::span::Span {
    program
        .member_name_spans
        .get(&call)
        .copied()
        .unwrap_or_else(|| span_of(program, call))
}

fn context_name<'a>(program: &'a Program, context: Id) -> &'a str {
    program
        .variables
        .get(&context)
        .map(|variable| variable.name)
        .unwrap_or("context")
}

/// Analyzes the program's contexts, producing the rewrite plan or the
/// diagnostics that block it.
fn analyze(
    program: &Program,
    graph: &CallGraph,
    get_fn: Id,
    get_safe_fn: Option<Id>,
    run_fn: Id,
    new_fn: Id,
) -> Result<Plan, Vec<(Error, SourceId)>> {
    let mut errors = Vec::new();

    // The entry `main` is special: the transformer inlines its body as the
    // program's top-level statements, so it can never carry a hidden
    // parameter — and semantically it IS the uncovered root. Its safe reads
    // become literal `None`s; a STRICT-needy main fences like any
    // top-level-called function.
    let entry_main: Option<Id> = program
        .scopes
        .get(&program.global_scope_id)
        .and_then(|scope| scope.name_to_id_map.get("main"))
        .copied()
        .filter(|id| program.functions.contains_key(id));

    // call id -> the function/closure it sits in.
    let mut owner_of: HashMap<Id, Node> = HashMap::default();
    for node in graph.nodes() {
        for call in graph.calls_of(node.id()) {
            owner_of.insert(call.call_id, *node);
        }
    }

    // --- Collect get/run/new sites. ---
    let mut gets: Vec<GetSite> = Vec::new();
    let mut runs: Vec<RunSite> = Vec::new();
    let mut news: Vec<Id> = Vec::new();
    let mut contexts: HashSet<Id> = HashSet::default();

    for (&call_id, function_call) in &program.function_calls {
        let Some(target) = call_target(program, call_id) else {
            continue;
        };
        if target == new_fn {
            news.push(call_id);
        } else if target == get_fn || Some(target) == get_safe_fn {
            let safe = Some(target) == get_safe_fn;
            // `receiver.get()` / `receiver.get_safe()` — argument 0 is the
            // receiver.
            let receiver = function_call.argument_ids.first().copied();
            let context = receiver.and_then(|receiver| local_target(program, receiver));
            let (Some(context), Some(&owner)) = (context, owner_of.get(&call_id)) else {
                let method = if safe { "get_safe" } else { "get" };
                errors.push(anchored(
                    program,
                    call_id,
                    format!("`{method}` must be called on a context bound to a name"),
                ));
                continue;
            };
            contexts.insert(context);
            gets.push(GetSite {
                call_id,
                context,
                owner,
                safe,
            });
        } else if target == run_fn {
            // `receiver.run(value, body)` — arguments [receiver, value, body].
            let arguments = &function_call.argument_ids;
            let context = arguments
                .first()
                .copied()
                .and_then(|receiver| local_target(program, receiver));
            let value_id = arguments.get(1).copied();
            let closure_entity = arguments.get(2).copied();
            let closure_id =
                closure_entity.and_then(|entity| match program.entity_map.get(&entity) {
                    Some(Expr::Closure(closure_id)) => Some(*closure_id),
                    _ => None,
                });
            // An injected `context`-typed closure VALUE is a legal body when
            // its clause is exactly this context (the deferred argument is
            // what `run` supplies) — proposal/ambient-owner.md §5.
            let injected_body = closure_entity
                .and_then(|entity| match program.entity_map.get(&entity) {
                    Some(Expr::Local(target)) => program.parameter_contexts.get(target),
                    _ => None,
                })
                .is_some_and(|clause| context.is_some_and(|context| clause == &vec![context]));
            let (Some(context), Some(value_id), Some(closure_entity)) =
                (context, value_id, closure_entity)
            else {
                errors.push(anchored(
                    program,
                    call_id,
                    "`run` must be called on a named context with a closure literal body"
                        .to_string(),
                ));
                continue;
            };
            if closure_id.is_none() && !injected_body {
                errors.push(anchored(program, call_id, "`run` must be called on a named context with a closure literal body, or a closure value whose type is `context`-annotated with exactly this context"
                        .to_string()));
                continue;
            }
            contexts.insert(context);
            runs.push(RunSite {
                call_id,
                value_id,
                closure_entity,
                closure_id,
            });
        }
    }

    // --- Spawn registration (async-polymorphism.md Part B): every `async`
    // spawn is a SAFE read of the `std::task` ambient nursery, so a spawn in
    // a nursery's dynamic extent registers its task. Engaged only when some
    // call to a nursery-establishing construct exists — `nursery`, or
    // `OwnedNursery.enter` (destruction.md §9) — so a program that merely loads
    // `std::task` (say, for `settle_all`) compiles untouched. The spawn's owner
    // is the node containing the spawn expression (the spawn closure's parent);
    // a module-level spawn has none and stays free-floating.
    let nursery_engaged = [program.nursery_fn_id, program.owned_nursery_enter_fn_id]
        .into_iter()
        .flatten()
        .any(|establishing_fn| {
            program
                .function_calls
                .values()
                .any(|call| local_target(program, call.subject_id) == Some(establishing_fn))
        });
    let nursery_context: Option<Id> = program.nursery_ambient_id.filter(|_| nursery_engaged);
    let mut spawn_sites: Vec<(Id, Node)> = Vec::new();
    if let Some(context) = nursery_context {
        contexts.insert(context);
        for (&entity_id, expr) in &program.entity_map {
            let Expr::Async(closure_id) = expr else {
                continue;
            };
            let Some(parent) = graph.closure_parent_of(*closure_id) else {
                continue;
            };
            let Some(&owner) = graph.nodes().iter().find(|node| node.id() == parent) else {
                continue;
            };
            spawn_sites.push((entity_id, owner));
        }
        // Deterministic plan order (entity_map iteration is not).
        spawn_sites.sort_by_key(|(entity_id, _)| entity_id.0);
    }

    if contexts.is_empty() && program.parameter_contexts.is_empty() {
        return if errors.is_empty() {
            // No reads, no runs — but `Context::new()` calls still lower to
            // their opaque value (previously they emitted a dangling call).
            Ok(Plan {
                news,
                ..Default::default()
            })
        } else {
            Err(errors)
        };
    }

    // The body closure of every `run`, mapped to the context it binds (the
    // run's receiver). A closure passed to `run` receives the value as a
    // parameter rather than capturing it.
    let mut run_closures: HashMap<Id, Id> = HashMap::default();
    for site in &runs {
        if let (Some(closure_id), Some(context)) = (
            site.closure_id,
            program
                .function_calls
                .get(&site.call_id)
                .and_then(|call| call.argument_ids.first().copied())
                .and_then(|receiver| local_target(program, receiver)),
        ) {
            run_closures.insert(closure_id, context);
        }
    }

    let mut plan = Plan {
        contexts: {
            let mut sorted: Vec<Id> = contexts.iter().copied().collect();
            sorted.sort_by_key(|id| id.0);
            sorted
        },
        news,
        runs,
        ..Default::default()
    };

    // --- Dispatch edges the shared graph deliberately leaves indirect
    // (backlog B14): a call the analyzer routed through trait dispatch — a
    // trait method on a concrete receiver (`OnType`, which may land on the
    // trait's DEFAULT body) or a generic-bounded member (`OnConstraint`) —
    // has no `CallTarget::Function` edge, so the trait default's gets looked
    // unreachable and its callers uncovered. The graph stays untouched (it is
    // also async inference's graph; conservative edges there would
    // over-propagate async-ness); the context analysis adds the edges
    // LOCALLY: for each dispatch site, every candidate callee — the named
    // member's trait default plus every implementation's override, across the
    // traits declaring that name. Over-approximation is sound here (an extra
    // caller edge only strengthens the coverage demand); the same sites join
    // the threading plan, and a callee that turns out not to need the value
    // simply ignores the extra argument.
    // The name and candidate lookups live in `dispatch_refine` (shared with
    // the const-only capability check since B143).
    let dispatch_member_name =
        |call_id: Id| -> Option<&str> { crate::dispatch_refine::member_name_at(program, call_id) };
    let dispatch_candidates =
        |name: &str| -> Vec<Id> { crate::dispatch_refine::candidates_of(program, name) };
    // (caller node, call id, candidate callees) per dispatch site.
    let mut dispatch_sites: Vec<(Id, Id, Vec<Id>)> = Vec::new();
    // callee -> the nodes that may reach it through dispatch.
    let mut dispatch_callers: HashMap<Id, Vec<Id>> = HashMap::default();
    for node in graph.nodes() {
        for call in graph.calls_of(node.id()) {
            if !matches!(
                call.target,
                CallTarget::Indirect(IndirectReason::TraitDispatch | IndirectReason::GenericMember)
            ) {
                continue;
            }
            let Some(name) = dispatch_member_name(call.call_id) else {
                continue;
            };
            let candidates = dispatch_candidates(name);
            for &candidate in &candidates {
                dispatch_callers
                    .entry(candidate)
                    .or_default()
                    .push(node.id());
            }
            dispatch_sites.push((node.id(), call.call_id, candidates));
        }
    }

    // --- Entry points the call graph cannot see (for the coverage check's
    // dead-code exemption): a function with NO caller edges is either dead —
    // it cannot run, so it cannot run uncovered — or entered from OUTSIDE the
    // graph, which must stay checked. Outside entries: calls made by
    // top-level statements (the graph has no top-level node), and functions
    // taken as values (called indirectly; the value-use error also fires).
    let owned_call_ids: HashSet<Id> = graph
        .nodes()
        .iter()
        .flat_map(|node| graph.calls_of(node.id()))
        .map(|call| call.call_id)
        .collect();
    let top_level_targets: HashSet<Id> = program
        .function_calls
        .iter()
        .filter(|(call_id, _)| !owned_call_ids.contains(call_id))
        .filter_map(|(_, call)| local_target(program, call.subject_id))
        .collect();
    // Every entity that names a callee IN CALL POSITION. `function_calls`
    // holds the calls the solver WIRED; an arity-invalid call is refused
    // before wiring, and its subject would otherwise read as the callee taken
    // as a value (B241) — the same "a written call the site scan cannot see"
    // hole B229 found under `run`, with a different lid. A call written with
    // the wrong number of arguments is still a CALL of the name it wrote.
    let call_subject_entities: HashSet<Id> = program
        .function_calls
        .values()
        .map(|call| call.subject_id)
        .chain(
            program
                .arity_invalid_calls
                .iter()
                .map(|(_, subject_id)| *subject_id),
        )
        .collect();
    let value_taken: HashSet<Id> = program
        .entity_map
        .iter()
        .filter_map(|(entity_id, expr)| match expr {
            Expr::Local(target)
                if program.functions.contains_key(target)
                    && !call_subject_entities.contains(entity_id) =>
            {
                Some(*target)
            }
            _ => None,
        })
        .collect();

    // --- Coverage-only dispatch refinement (element-syntax H8 →
    // proposal/requirement-polymorphism.md): the union edges above stay for
    // needs/strict/threading — sound, and the hidden value physically flows
    // caller → generic body → impl either way — but COVERAGE follows the
    // resolved instantiation: per site, only the callees the recorded
    // dispatch actually selects, resolved per entry for `OnConstraint` sites
    // and by the receiver's head for `OnType` ones, with every unresolvable
    // shape widening back to the whole candidate list. The resolution rules
    // live in [`crate::dispatch_refine`] (extracted for the const-only
    // capability check, backlog B143 — see that module's documentation).
    // Incoming direct calls per function, and top-level incoming calls (the
    // A2 walk-back below reads these; the refinement recomputes its own
    // internally, so its edges are a function of program + graph alone).
    let mut incoming_calls: HashMap<Id, Vec<(Id, Id)>> = HashMap::default();
    for node in graph.nodes() {
        for call in graph.calls_of(node.id()) {
            if let CallTarget::Function(target) = call.target {
                incoming_calls
                    .entry(target)
                    .or_default()
                    .push((node.id(), call.call_id));
            }
        }
    }
    let mut top_level_incoming: HashMap<Id, Vec<Id>> = HashMap::default();
    for (call_id, call) in &program.function_calls {
        if owned_call_ids.contains(call_id) {
            continue;
        }
        if let Some(target) = local_target(program, call.subject_id) {
            top_level_incoming.entry(target).or_default().push(*call_id);
        }
    }
    // A `Node` edge draws a coverage edge from that caller; a `TopLevel`
    // edge marks the callee outside-entered — always uncovered.
    let refinement_sites: Vec<crate::dispatch_refine::DispatchSite> = dispatch_sites
        .iter()
        .map(
            |(owner, site_call, candidates)| crate::dispatch_refine::DispatchSite {
                owner: crate::dispatch_refine::RefinedCaller::Node(*owner),
                call: *site_call,
                candidates: candidates.clone(),
            },
        )
        .collect();
    let mut coverage_dispatch_callers: HashMap<Id, Vec<Id>> = HashMap::default();
    let mut coverage_outside: HashSet<Id> = HashSet::default();
    for edge in crate::dispatch_refine::refined_edges(program, graph, &refinement_sites) {
        match edge.caller {
            crate::dispatch_refine::RefinedCaller::Node(caller) => {
                coverage_dispatch_callers
                    .entry(edge.callee)
                    .or_default()
                    .push(caller);
            }
            crate::dispatch_refine::RefinedCaller::TopLevel => {
                coverage_outside.insert(edge.callee);
            }
        }
    }

    // --- The A2 walk-back's inputs (E74, diagnostics-standard.md §1). ---
    // A coverage refusal whose offending site sits in library code — std
    // (`effect`/`map`/`or` all funnel to `get_owner`'s strict read) or a
    // dependency package (E84) — was caused by user code calling into that
    // library, so the diagnostic must lead with the user's call, not the
    // library's body. The reporting loops below walk from the site's owner
    // back along the same edges the strictness climbed, and these are those
    // edges at call-site grain.
    let mut dispatch_incoming: HashMap<Id, Vec<(Id, Id)>> = HashMap::default();
    for (owner, call_id, candidates) in &dispatch_sites {
        for &candidate in candidates {
            dispatch_incoming
                .entry(candidate)
                .or_default()
                .push((*owner, *call_id));
        }
    }
    // C3a's demotion domain (E84): code the user did not write — a std module
    // or a dependency package's, both disk-loaded (an overlaid buffer is live
    // user territory and anchors at itself). The anchoring walk (A2/E74) and
    // hop labeling (E78) both go through this one predicate, so std and
    // dependencies demote and trace identically.
    let library_spanned = |id: Id| -> bool {
        program.source_of(id).is_some_and(|source| {
            program.std_sources.contains(&source) || program.dependency_sources.contains(&source)
        })
    };
    // Whether a dispatch site can actually select `candidate` — the
    // concrete-receiver narrowing the coverage refinement uses, applied at
    // one site: a recorded receiver's HEAD selects among candidates, so a
    // site whose receiver picks a different member never takes the blame for
    // this one. `OnConstraint` sites resolve per ENTRY, not per site, so they
    // stay admitted (the union), exactly as conservative as the union edges.
    let dispatch_admits = |call_id: Id, candidate: Id| -> bool {
        match crate::async_infer::dispatch_at(program, call_id) {
            Some(crate::analyzer::GenericDispatch::OnType(Some(receiver), member)) => {
                match program.type_id_to_type_map.get(&receiver) {
                    Some(resolved) if crate::impl_select::is_resolvable(resolved) => {
                        let selected =
                            crate::dispatch_refine::impl_members_for(program, receiver, member);
                        selected.is_empty() || selected.contains(&candidate)
                    }
                    _ => true,
                }
            }
            _ => true,
        }
    };

    // --- Injected (`context`-typed) closures — proposal/ambient-owner.md §5. ---
    // A clause on a parameter's closure type defers that closure's context
    // binding to its CALL sites: the literal passed in takes its own hidden
    // parameter (no creation capture), each call through the parameter is a
    // read-like demand on the caller (and a threading site), and the value
    // may only flow where the threading can follow it — a call, a forward to
    // a parameter with the SAME clause, or `run`'s body position.
    let mut deferred: HashMap<Id, HashSet<Id>> = HashMap::default(); // ctx -> closures
    let mut injected_calls: HashMap<Id, Vec<(Node, Id)>> = HashMap::default(); // ctx -> (caller, call)
    // The working clause map: declared clauses (parameters AND `let`
    // annotations) plus ADOPTED ones — an unannotated closure-literal binding
    // passed into a clause position adopts that clause (`let add = || ..;`
    // then `.on("click", add)`), exactly as if the literal were written
    // inline: its literal defers, and its direct calls become injected calls.
    let mut value_contexts: HashMap<Id, Vec<Id>> = program.parameter_contexts.clone();
    {
        // Validate each clause names actual contexts, and admit them to the
        // per-context loop (a clause context may have no direct get/run).
        for (&parameter, clause) in &program.parameter_contexts {
            for &context in clause {
                let is_context = program
                    .variables
                    .get(&context)
                    .and_then(|variable| program.type_id_to_type_map.get(&variable.type_id))
                    .is_some_and(|type_| {
                        matches!(
                            type_,
                            Type::Struct(id, _)
                                if program
                                    .structs
                                    .get(id)
                                    .is_some_and(|struct_| struct_.name == "Context")
                        )
                    });
                if is_context {
                    contexts.insert(context);
                } else {
                    errors.push(anchored(
                        program,
                        parameter,
                        "this parameter's `context` clause names a value that is not a context"
                            .to_string(),
                    ));
                }
            }
        }

        // Closure literals landing in annotated positions defer; annotated
        // values may forward to a parameter with the SAME clause.
        let mut allowed_forwards: HashSet<Id> = HashSet::default();

        // Clause-typed LET bindings (the ui-boundary follow-up): the
        // binding is a NAMED injected closure. Its initializer literal
        // defers exactly like a literal in a clause parameter position; a
        // same-clause value initializer is a forward; anything else is an
        // escape the threading cannot follow.
        for (&binding_id, clause) in &program.parameter_contexts {
            let Some(variable) = program.variables.get(&binding_id) else {
                // Parameters share the map but have no variable record.
                continue;
            };
            let Some(initial) = variable.initial else {
                continue;
            };
            match program.entity_map.get(&initial) {
                Some(Expr::Closure(closure_id)) => {
                    for &context in clause {
                        deferred.entry(context).or_default().insert(*closure_id);
                    }
                }
                Some(Expr::Local(source))
                    if program.parameter_contexts.get(source) == Some(clause) =>
                {
                    allowed_forwards.insert(initial);
                }
                _ => {
                    errors.push(anchored(program, initial, "a `context`-typed binding takes a closure literal, or a value with the same `context` clause"
                            .to_string()));
                }
            }
        }

        // Adoption: an argument binding with NO clause of its own, whose
        // initial is a closure literal, adopts the parameter's clause.
        let adoptable = |source: Id| -> Option<Id> {
            let variable = program.variables.get(&source)?;
            let initial = variable.initial?;
            match program.entity_map.get(&initial) {
                Some(Expr::Closure(closure_id)) => Some(*closure_id),
                _ => None,
            }
        };
        for (&call_id, function_call) in &program.function_calls {
            let Some(target) = call_target(program, call_id) else {
                continue;
            };
            let Some(function) = program.functions.get(&target) else {
                continue;
            };
            for (argument, parameter) in function_call.argument_ids.iter().zip(&function.parameters)
            {
                let Some(clause) = value_contexts.get(parameter).cloned() else {
                    continue;
                };
                match program.entity_map.get(argument) {
                    Some(Expr::Closure(closure_id)) => {
                        for &context in &clause {
                            deferred.entry(context).or_default().insert(*closure_id);
                        }
                    }
                    Some(Expr::Local(source)) if value_contexts.get(source) == Some(&clause) => {
                        allowed_forwards.insert(*argument);
                    }
                    Some(Expr::Local(source))
                        if !value_contexts.contains_key(source) && adoptable(*source).is_some() =>
                    {
                        let closure_id = adoptable(*source).expect("just matched");
                        value_contexts.insert(*source, clause.clone());
                        for &context in &clause {
                            deferred.entry(context).or_default().insert(closure_id);
                        }
                        allowed_forwards.insert(*argument);
                    }
                    _ => {
                        errors.push(anchored(program, *argument, "a `context`-typed parameter takes a closure literal, a value with the same `context` clause, or a local closure binding (which adopts the clause)"
                                .to_string()));
                    }
                }
            }
        }

        // Calls THROUGH an annotated (or adopted) value — after adoption, so
        // a named handler's direct calls demand and thread like any injected
        // call.
        for node in graph.nodes() {
            for call in graph.calls_of(node.id()) {
                let Some(function_call) = program.function_calls.get(&call.call_id) else {
                    continue;
                };
                if let Some(Expr::Local(target)) = program.entity_map.get(&function_call.subject_id)
                    && let Some(clause) = value_contexts.get(target)
                {
                    for &context in clause {
                        injected_calls
                            .entry(context)
                            .or_default()
                            .push((*node, call.call_id));
                    }
                }
            }
        }

        // The value-flow restriction: everywhere else an annotated value
        // appears is an escape the threading cannot follow.
        let run_body_entities: HashSet<Id> =
            plan.runs.iter().map(|site| site.closure_entity).collect();
        for (&entity, expr) in &program.entity_map {
            let Expr::Local(target) = expr else {
                continue;
            };
            if !value_contexts.contains_key(target) {
                continue;
            }
            if call_subject_entities.contains(&entity)
                || allowed_forwards.contains(&entity)
                || run_body_entities.contains(&entity)
            {
                continue;
            }
            errors.push(anchored(program, entity, "an injected (`context`-typed) closure can only be called, forwarded to a parameter with the same `context` clause, or passed to `run`"
                    .to_string()));
        }
    }
    plan.contexts = {
        let mut sorted: Vec<Id> = contexts.iter().copied().collect();
        sorted.sort_by_key(|id| id.0);
        sorted
    };

    // --- The `run` sites the solver never selected (B229). ---
    // A `run` whose argument failed to type is never SELECTED, and an
    // unselected method call is wired nowhere: the collection loop above scans
    // `function_calls` and so cannot see it. The site is then missing from
    // `runs`, the context it binds looks bound nowhere, and every strict read
    // of it fences — three refusals about a missing `run` the program plainly
    // writes, printed ahead of the one about the argument that actually
    // failed. The shape survives in `unresolved_method_calls`, so the contexts
    // those sites name stand their coverage verdict down: coverage is not a
    // question that can be answered about a program whose `run` sites are not
    // all on record, and the diagnostic explaining why is already in hand.
    //
    // Narrow on purpose: only the contexts an unresolved `run` actually names
    // are excused — every other context in the same program keeps its verdict.
    //
    // The stand-down shipped with a second narrowing, a `program.diagnostics`
    // non-empty guard, because a stalled `MethodCall` had no residual of its
    // own: on an otherwise clean program the excuse would have turned a
    // coverage fence into a silent miscompile, which is the one thing this
    // check exists to prevent. B232 gave the stalled call its own residual, so
    // an unresolved `run` IS a diagnostic and the guard asked a question that
    // can no longer have the answer it was written for.
    let unresolved_run_contexts: HashSet<Id> = program
        .unresolved_method_calls
        .iter()
        .filter(|(_, _, member_name)| *member_name == "run")
        .filter_map(|(_, subject_id, _)| local_target(program, *subject_id))
        .filter(|context| contexts.contains(context))
        .collect();

    // --- Per-context effect inference + coverage. ---
    for &context in &plan.contexts {
        // A `run` for this context is written but unresolved (B229): neither
        // the verdict nor the rewrite is this pass's to record — the program
        // has not type-checked, and its own diagnostic says so.
        if unresolved_run_contexts.contains(&context) {
            continue;
        }
        // Seed with the nodes that directly read this context.
        let mut needs: HashSet<Id> = HashSet::default();
        let mut worklist: Vec<Id> = Vec::new();
        for get in gets.iter().filter(|get| get.context == context) {
            if needs.insert(get.owner.id()) {
                worklist.push(get.owner.id());
            }
        }
        // A call through an injected closure demands the context on its
        // caller, exactly like a read (proposal/ambient-owner.md §5).
        for (owner, _call) in injected_calls.get(&context).into_iter().flatten() {
            if needs.insert(owner.id()) {
                worklist.push(owner.id());
            }
        }
        // A spawn demands the ambient nursery on its owner — SAFE (a
        // free-floating spawn is legal, its read is simply absent), so it
        // joins `needs` but never the strict set.
        if Some(context) == nursery_context {
            for (_, owner) in &spawn_sites {
                if needs.insert(owner.id()) {
                    worklist.push(owner.id());
                }
            }
        }
        // A closure that RECEIVES the value as its own parameter — a `run`
        // body for this context, or a deferred (injected) literal — does not
        // capture from its creator, so needs must not leak to its parent.
        let own_param_closure = |id: Id| -> bool {
            run_closures.get(&id) == Some(&context)
                || deferred
                    .get(&context)
                    .is_some_and(|closures| closures.contains(&id))
        };
        // Backward reachability: a caller of a needs-context node needs it too
        // — through direct edges, through dispatch (B14), and — for CAPTURING
        // closures only — through the enclosing scope (the closure reads its
        // provider's parameter, so the provider must hold one; a stored
        // notify closure created inside `map` makes `map` needy, and `map`
        // created under a turn then hands that turn to the closure).
        while let Some(id) = worklist.pop() {
            for caller in graph.callers_of(id) {
                if needs.insert(caller.id()) {
                    worklist.push(caller.id());
                }
            }
            for caller in dispatch_callers.get(&id).into_iter().flatten() {
                if needs.insert(*caller) {
                    worklist.push(*caller);
                }
            }
            if !own_param_closure(id)
                && let Some(parent) = graph.closure_parent_of(id)
                && needs.insert(parent)
            {
                worklist.push(parent);
            }
        }

        // --- Flavor (reactive-turns.md §5.1): STRICT nodes hold the bare
        // value (a strict `get`, or a call through an injected closure,
        // reaches them) and keep the coverage fence; the rest of `needs` is
        // SAFE — it holds `Option<T>` and never fences. Strictness propagates
        // backward exactly like `needs` (a caller of a strict node must
        // supply the bare value), so strict ⊆ needs.
        let mut strict: HashSet<Id> = HashSet::default();
        let mut strict_worklist: Vec<Id> = Vec::new();
        for get in gets
            .iter()
            .filter(|get| get.context == context && !get.safe)
        {
            if strict.insert(get.owner.id()) {
                strict_worklist.push(get.owner.id());
            }
        }
        for (owner, _call) in injected_calls.get(&context).into_iter().flatten() {
            if strict.insert(owner.id()) {
                strict_worklist.push(owner.id());
            }
        }
        loop {
            while let Some(id) = strict_worklist.pop() {
                for caller in graph.callers_of(id) {
                    if strict.insert(caller.id()) {
                        strict_worklist.push(caller.id());
                    }
                }
                for caller in dispatch_callers.get(&id).into_iter().flatten() {
                    if strict.insert(*caller) {
                        strict_worklist.push(*caller);
                    }
                }
                if !own_param_closure(id)
                    && let Some(parent) = graph.closure_parent_of(id)
                    && strict.insert(parent)
                {
                    strict_worklist.push(parent);
                }
            }
            // A dispatch site whose needy candidates MIX flavors would need
            // two argument forms at one call — promote its safe candidates
            // to strict (they gain the fence) and re-propagate.
            let mut promoted = false;
            for (_caller, _call_id, candidates) in &dispatch_sites {
                let needy: Vec<Id> = candidates
                    .iter()
                    .copied()
                    .filter(|candidate| needs.contains(candidate))
                    .collect();
                if needy.is_empty() || needy.iter().all(|id| !strict.contains(id)) {
                    continue;
                }
                for id in needy {
                    if strict.insert(id) {
                        strict_worklist.push(id);
                        promoted = true;
                    }
                }
            }
            if !promoted {
                break;
            }
        }

        let run_closure_ids: HashSet<Id> = run_closures
            .iter()
            .filter(|(_, bound)| **bound == context)
            .map(|(closure, _)| *closure)
            // A deferred (injected) literal behaves like a `run` body here:
            // it always takes its own hidden parameter and is covered by
            // construction — its callers supply the value.
            .chain(deferred.get(&context).into_iter().flatten().copied())
            .collect();

        // Classify each needs-context node.
        let is_function = |id: Id| program.functions.contains_key(&id);

        // --- Coverage (greatest fixpoint): assume every node is covered, then
        // remove any that can be entered without the value. A `run` closure
        // always receives the value from `run`, so it is covered even when it
        // doesn't read the context itself (a nested closure may capture it).
        // Only STRICT nodes are checked — a safe node legitimately runs
        // uncovered (its parameter is then `None`). ---
        let mut bound: HashSet<Id> = needs
            .iter()
            .copied()
            .chain(run_closure_ids.iter().copied())
            .collect();
        // The inlined entry `main` never receives a value.
        if let Some(main) = entry_main {
            bound.remove(&main);
        }
        loop {
            let mut removed = false;
            for &id in &strict {
                if !bound.contains(&id) || run_closure_ids.contains(&id) {
                    continue;
                }
                let covered = if is_function(id) {
                    // Coverage reads the REFINED dispatch edges: a candidate
                    // no recorded instantiation selects has no coverage
                    // caller (and, with no other edges, is exempt — it cannot
                    // run); one selected from a top-level call is entered
                    // from outside the graph — uncovered.
                    let callers = graph.callers_of(id);
                    let through_dispatch = coverage_dispatch_callers.get(&id);
                    let no_edges =
                        callers.is_empty() && through_dispatch.is_none_or(|list| list.is_empty());
                    if coverage_outside.contains(&id)
                        || top_level_targets.contains(&id)
                        || value_taken.contains(&id)
                    {
                        // Entered from outside the graph — uncovered
                        // regardless of any covered caller edges (one bound
                        // caller must not launder an uncovered top-level
                        // entry).
                        false
                    } else if no_edges {
                        // No caller edges and no outside entry: dead code is
                        // exempt (it cannot run).
                        true
                    } else {
                        callers.iter().all(|caller| bound.contains(&caller.id()))
                            && through_dispatch
                                .into_iter()
                                .flatten()
                                .all(|caller| bound.contains(caller))
                    }
                } else {
                    // A captured closure is covered iff its defining scope is.
                    graph
                        .closure_parent_of(id)
                        .map(|parent| bound.contains(&parent))
                        .unwrap_or(false)
                };
                if !covered {
                    bound.remove(&id);
                    removed = true;
                }
            }
            if !removed {
                break;
            }
        }

        // The A2 walk-back (E74, widened to any dependency package by
        // C3a/E84): a refused site whose own span sits in library code
        // anchors at the user-written calls that enter the library on an
        // uncovered path — the async-polymorphism origin discipline
        // (`record_origin`: ids are minted in walk order, so call-id order
        // is program order), with the library site demoted to the C3 note.
        // E74 kept only the least-id entry; E78 keeps them all — each
        // uncovered entry becomes its own diagnostic (fixing the first must
        // not merely reveal the next), returned in id order so the least-id
        // one still leads, exactly where E74 anchored. A site in user code
        // anchors at itself and returns no entries here. The walk descends
        // the same edges the strictness climbed — direct calls, admitted
        // dispatch calls, the capture hop — and only through UNBOUND
        // callers: a covered caller is not on the uncovered path and must
        // not take the blame. No user entry found (an uncovered read
        // reachable only from a library's own load would be the library's
        // bug, not the user's) falls back to the library anchor.
        let user_entries_of = |site: Id, start: Id| -> Vec<Id> {
            if !library_spanned(site) {
                return Vec::new();
            }
            let mut entries: Vec<Id> = Vec::new();
            let mut visited: HashSet<Id> = HashSet::default();
            let mut walk: Vec<Id> = vec![start];
            while let Some(node) = walk.pop() {
                if !visited.insert(node) {
                    continue;
                }
                for &(caller, call_id) in incoming_calls.get(&node).into_iter().flatten() {
                    if bound.contains(&caller) {
                        continue;
                    }
                    if library_spanned(call_id) {
                        walk.push(caller);
                    } else {
                        entries.push(call_id);
                    }
                }
                for &(caller, call_id) in dispatch_incoming.get(&node).into_iter().flatten() {
                    if bound.contains(&caller) || !dispatch_admits(call_id, node) {
                        continue;
                    }
                    if library_spanned(call_id) {
                        walk.push(caller);
                    } else {
                        entries.push(call_id);
                    }
                }
                // A top-level call is an uncovered entry by construction; it
                // has no node to walk onward to.
                for &call_id in top_level_incoming.get(&node).into_iter().flatten() {
                    if !library_spanned(call_id) {
                        entries.push(call_id);
                    }
                }
                // The capture hop: an unbound closure's uncovered-ness came
                // through its defining scope.
                if let Some(parent) = graph.closure_parent_of(node)
                    && !bound.contains(&parent)
                {
                    walk.push(parent);
                }
            }
            // One dispatch site reaches the walk once per visited candidate;
            // a diagnostic per site, not per candidate.
            entries.sort_by_key(|entry| entry.0);
            entries.dedup();
            entries
        };

        // The requirement trace (backlog E78): the refusal keeps the PATH
        // the walk traverses, not just its endpoint. One hop per uncovered
        // user-written call upstream of `start` (the frame holding the
        // primary), breadth-first so a hop's depth is its least distance
        // from that frame. A covered caller's edge is skipped exactly as in
        // the walk above — a providing call is never labeled and stops the
        // trace — and library-internal calls (std's or a dependency
        // package's, E84) are traversed but never labeled (A2 demotes
        // library frames; the C3 note already names the library site).
        // The capture hop crosses no call site, so it adds no label and no
        // depth: the closure blames its defining scope's callers directly.
        struct Hop {
            call: Id,
            /// A dispatch edge is union-admitted (row 222's residual): the
            /// site MAY select the needy candidate, so its label must not
            /// overclaim.
            dispatch: bool,
            depth: usize,
        }
        let uncovered_hops_from = |start: Id| -> Vec<Hop> {
            let mut hops: Vec<Hop> = Vec::new();
            let mut visited: HashSet<Id> = HashSet::default();
            let mut frontier: VecDeque<(Id, usize)> = VecDeque::from([(start, 0)]);
            while let Some((node, depth)) = frontier.pop_front() {
                if !visited.insert(node) {
                    continue;
                }
                for &(caller, call_id) in incoming_calls.get(&node).into_iter().flatten() {
                    if bound.contains(&caller) {
                        continue;
                    }
                    if !library_spanned(call_id) {
                        hops.push(Hop {
                            call: call_id,
                            dispatch: false,
                            depth,
                        });
                    }
                    frontier.push_back((caller, depth + 1));
                }
                for &(caller, call_id) in dispatch_incoming.get(&node).into_iter().flatten() {
                    if bound.contains(&caller) || !dispatch_admits(call_id, node) {
                        continue;
                    }
                    if !library_spanned(call_id) {
                        hops.push(Hop {
                            call: call_id,
                            dispatch: true,
                            depth,
                        });
                    }
                    frontier.push_back((caller, depth + 1));
                }
                for &call_id in top_level_incoming.get(&node).into_iter().flatten() {
                    if !library_spanned(call_id) {
                        hops.push(Hop {
                            call: call_id,
                            dispatch: false,
                            depth,
                        });
                    }
                }
                if let Some(parent) = graph.closure_parent_of(node)
                    && !bound.contains(&parent)
                {
                    frontier.push_back((parent, depth));
                }
            }
            hops
        };

        // Hops as trace labels, ordered entry → read: decreasing depth reads
        // from the outermost uncovered frame down toward the primary (a
        // level order — on a many-chains DAG every hop's own upstream hop
        // precedes it), ties in id (program) order; one label per call site.
        // Past TRACE_CAP the entry side is kept and the rest elides behind
        // the honest tail. Each label follows the `Note::source` contract:
        // the file is named only when it differs from the anchor's.
        let trace_of = |start: Id, anchor: Id| -> Vec<TraceHop> {
            let mut hops = uncovered_hops_from(start);
            hops.sort_by(|a, b| b.depth.cmp(&a.depth).then(a.call.0.cmp(&b.call.0)));
            let mut seen: HashSet<Id> = HashSet::default();
            hops.retain(|hop| seen.insert(hop.call));
            let anchor_source = program.note_source_of(anchor);
            let locate = |call: Id| {
                program
                    .note_source_of(call)
                    .filter(|source| Some(*source) != anchor_source)
            };
            let elided = hops.len().saturating_sub(TRACE_CAP);
            let mut entries: Vec<TraceHop> = hops
                .iter()
                .take(TRACE_CAP)
                .map(|hop| TraceHop {
                    note: Note {
                        span: call_anchor_span(program, hop.call),
                        msg: if hop.dispatch {
                            "the context requirement may flow through this call (dispatch may select a reader)"
                                .to_string()
                        } else {
                            "the context requirement flows through this call".to_string()
                        },
                        source: locate(hop.call),
                    },
                    call: true,
                })
                .collect();
            if elided > 0 {
                let last = &hops[TRACE_CAP - 1];
                let plural = if elided == 1 { "call" } else { "calls" };
                entries.push(TraceHop {
                    note: Note {
                        span: call_anchor_span(program, last.call),
                        msg: format!("… {elided} more uncovered {plural} on this path"),
                        source: locate(last.call),
                    },
                    call: false,
                });
            }
            entries
        };

        // Any STRICT get whose owner stayed unbound is read outside every
        // `run`; a safe read never fences.
        for get in gets
            .iter()
            .filter(|get| get.context == context && !get.safe)
        {
            if bound.contains(&get.owner.id()) {
                continue;
            }
            let message = format!(
                "context `{}` is read here, but this code can be reached without an enclosing `run`",
                context_name(program, context)
            );
            let entries = user_entries_of(get.call_id, get.owner.id());
            if entries.is_empty() {
                // A user-written read anchors at itself (E74), now carrying
                // its upstream chain; a library-spanned read no user entry
                // reaches keeps the bare library anchor, trace-free.
                let trace = if library_spanned(get.call_id) {
                    Vec::new()
                } else {
                    trace_of(get.owner.id(), get.call_id)
                };
                errors.push(anchored_tracing(program, get.call_id, message, trace, None));
            } else {
                for entry in entries {
                    // The chain climbs from the frame holding the entry call;
                    // a top-level entry has no frame and nothing above it.
                    let trace = owner_of
                        .get(&entry)
                        .map(|frame| trace_of(frame.id(), entry))
                        .unwrap_or_default();
                    errors.push(anchored_tracing(
                        program,
                        entry,
                        message.clone(),
                        trace,
                        Some(library_frame_note(
                            program,
                            graph,
                            get.call_id,
                            get.owner.id(),
                            "read",
                            entry,
                        )),
                    ));
                }
            }
        }
        // Calling an injected closure IS a read: its deferred argument comes
        // from the caller, so an unbound caller has nothing to supply.
        for (owner, call_id) in injected_calls.get(&context).into_iter().flatten() {
            if bound.contains(&owner.id()) {
                continue;
            }
            let message = format!(
                "an injected closure is called here, but this code can be reached without an enclosing `run` for context `{}`",
                context_name(program, context)
            );
            let entries = user_entries_of(*call_id, owner.id());
            if entries.is_empty() {
                let trace = if library_spanned(*call_id) {
                    Vec::new()
                } else {
                    trace_of(owner.id(), *call_id)
                };
                errors.push(anchored_tracing(program, *call_id, message, trace, None));
            } else {
                for entry in entries {
                    let trace = owner_of
                        .get(&entry)
                        .map(|frame| trace_of(frame.id(), entry))
                        .unwrap_or_default();
                    errors.push(anchored_tracing(
                        program,
                        entry,
                        message.clone(),
                        trace,
                        Some(library_frame_note(
                            program,
                            graph,
                            *call_id,
                            owner.id(),
                            "injected call",
                            entry,
                        )),
                    ));
                }
            }
        }

        // A needs-context function used as a value could be called indirectly,
        // bypassing the threaded parameter — refuse rather than miscompile.
        let needs_functions: HashSet<Id> = needs
            .iter()
            .copied()
            .filter(|&id| is_function(id))
            .collect();
        for (&entity_id, expr) in &program.entity_map {
            if let Expr::Local(target) = expr
                && needs_functions.contains(target)
                && !call_subject_entities.contains(&entity_id)
            {
                errors.push(anchored(
                    program,
                    entity_id,
                    format!(
                        "`{}` reads context `{}`, so it can't be used as a value",
                        program
                            .functions
                            .get(target)
                            .map(|function| function.name)
                            .unwrap_or("function"),
                        context_name(program, context)
                    ),
                ));
            }
        }

        if !errors.is_empty() {
            continue;
        }

        // --- Record the rewrite for this context. ---
        // Functions and `run` closures take their own parameter. Every `run`
        // closure does, even one not in `needs`, since `run` always passes it
        // the value (a nested closure may capture it).
        let mut param_nodes: HashSet<Id> = run_closure_ids.clone();
        // node -> the node whose parameter it reads (itself, or the capture
        // provider) — the parameter's FLAVOR is the provider's.
        let mut provider_of: HashMap<Id, Id> = HashMap::default();
        // Nodes with no value source: the inlined entry `main` (it can carry
        // no hidden parameter), and any closure whose provider chain roots at
        // it — their safe reads and threads become literal `None`s.
        let mut none_rooted: HashSet<Id> = HashSet::default();
        if let Some(main) = entry_main
            && needs.contains(&main)
        {
            none_rooted.insert(main);
        }
        for &id in &needs {
            if entry_main == Some(id) {
                continue;
            }
            if is_function(id) || run_closure_ids.contains(&id) {
                param_nodes.insert(id);
                provider_of.insert(id, id);
            } else {
                // A captured closure: walk up to the nearest enclosing node
                // that holds the value (a function or `run` closure). A walk
                // that lands on the entry `main` first has no value to
                // capture — the closure is None-rooted.
                let mut provider = graph.closure_parent_of(id);
                loop {
                    match provider {
                        Some(parent) if entry_main == Some(parent) => {
                            none_rooted.insert(id);
                            break;
                        }
                        Some(parent)
                            if is_function(parent) || run_closure_ids.contains(&parent) =>
                        {
                            plan.captures.push((context, id, parent));
                            provider_of.insert(id, parent);
                            break;
                        }
                        Some(parent) => provider = graph.closure_parent_of(parent),
                        None => {
                            none_rooted.insert(id);
                            break;
                        }
                    }
                }
            }
        }
        for &id in &param_nodes {
            provider_of.entry(id).or_insert(id);
        }
        for id in param_nodes {
            plan.param_nodes.push((context, id));
        }
        // A parameter holds the BARE value when its provider is strict or a
        // `run` closure (which `run` hands the bare value); otherwise it
        // holds `Option<T>`.
        let holds_bare = |node: Id| -> bool {
            provider_of
                .get(&node)
                .map(|provider| strict.contains(provider) || run_closure_ids.contains(provider))
                .unwrap_or(false)
        };

        for get in gets.iter().filter(|get| get.context == context) {
            if none_rooted.contains(&get.owner.id()) {
                // Only reachable for SAFE reads (a strict get here already
                // failed the fence): the value is definitionally absent.
                plan.none_gets.push(get.call_id);
                continue;
            }
            // A safe read of a BARE holder wraps; everything else reads the
            // parameter as-is (bare for strict gets, `Option` for safe reads
            // in safe holders).
            let wrap_some = get.safe && holds_bare(get.owner.id());
            plan.gets.push((get.call_id, context, get.owner, wrap_some));
        }

        // Spawn registration: a spawn whose owner has a value source reads it
        // (bare, or `Option`-wrapped in a safe holder). A none-rooted owner —
        // the inlined entry `main`, or a closure rooted there — has no value:
        // its spawns stay free-floating, no entry recorded.
        if Some(context) == nursery_context {
            for &(spawn_entity, owner) in &spawn_sites {
                if none_rooted.contains(&owner.id()) {
                    continue;
                }
                plan.spawns
                    .push((spawn_entity, context, owner, holds_bare(owner.id())));
            }
        }

        // Thread the value into every call from a needs-context node to a
        // needs-context function — direct calls, and dispatch sites whose
        // candidate callees include a needy one (B14; a candidate that does
        // not need the value ignores the extra trailing argument). The
        // argument form follows the flavors: bare→bare and Option→Option
        // pass the parameter through; a BARE holder supplying a SAFE callee
        // `Some`-wraps (the covered→safe boundary). Safe→strict cannot occur
        // (strictness propagated to the caller).
        for &node_id in &needs {
            let Some(&owner) = graph.nodes().iter().find(|node| node.id() == node_id) else {
                continue;
            };
            for call in graph.calls_of(node_id) {
                if let CallTarget::Function(callee) = call.target
                    && needs.contains(&callee)
                {
                    if none_rooted.contains(&node_id) {
                        // No value here: safe callees get `None` (a
                        // strict callee under a None root already
                        // fenced).
                        if !strict.contains(&callee) {
                            plan.thread_calls.push((
                                call.call_id,
                                context,
                                ThreadForm::NoneLiteral,
                            ));
                        }
                        continue;
                    }
                    let form = if !strict.contains(&callee) && holds_bare(node_id) {
                        ThreadForm::WrapSome { owner }
                    } else {
                        ThreadForm::Param { owner }
                    };
                    plan.thread_calls.push((call.call_id, context, form));
                }
            }
        }
        for (caller, call_id, candidates) in &dispatch_sites {
            if !needs.contains(caller) {
                continue;
            }
            let needy: Vec<Id> = candidates
                .iter()
                .copied()
                .filter(|candidate| needs.contains(candidate))
                .collect();
            if needy.is_empty() {
                continue;
            }
            let Some(&owner) = graph.nodes().iter().find(|node| node.id() == *caller) else {
                continue;
            };
            // Mixed flavors were promoted away: needy candidates are now all
            // strict or all safe.
            let callee_safe = needy.iter().all(|id| !strict.contains(id));
            if none_rooted.contains(caller) {
                if callee_safe {
                    plan.thread_calls
                        .push((*call_id, context, ThreadForm::NoneLiteral));
                }
                continue;
            }
            let form = if callee_safe && holds_bare(*caller) {
                ThreadForm::WrapSome { owner }
            } else {
                ThreadForm::Param { owner }
            };
            plan.thread_calls.push((*call_id, context, form));
        }
        // Calls through injected closures: the caller's value rides as the
        // deferred trailing argument (the bare channel).
        for (owner, call_id) in injected_calls.get(&context).into_iter().flatten() {
            plan.thread_calls
                .push((*call_id, context, ThreadForm::Param { owner: *owner }));
        }
        // Top-level calls to safe functions: the entry point with no value —
        // a literal `None` rides along. (Top-level calls to STRICT functions
        // already failed the fence.)
        for (&call_id, function_call) in &program.function_calls {
            if owned_call_ids.contains(&call_id) {
                continue;
            }
            let Some(target) = local_target(program, function_call.subject_id) else {
                continue;
            };
            if needs.contains(&target) && !strict.contains(&target) {
                plan.thread_calls
                    .push((call_id, context, ThreadForm::NoneLiteral));
            }
        }
    }

    // Safe reads synthesize `Some`/`None` — resolve the `Option` variant
    // entities once. Missing `Option` with safe sites in play is a hard
    // error rather than a miscompile. Spawn demand can create WrapSome
    // boundaries (a covered holder feeding a safe spawn-owning helper)
    // without any `get_safe` in the program, so every synthesizing thread
    // form counts, not just the literal `None`s.
    let any_safe = gets.iter().any(|get| get.safe);
    let any_synthesized = plan
        .thread_calls
        .iter()
        .any(|(_, _, form)| matches!(form, ThreadForm::NoneLiteral | ThreadForm::WrapSome { .. }));
    if any_safe || any_synthesized {
        let variants = program
            .enums
            .values()
            .find(|enum_| enum_.name == "Option")
            .and_then(|enum_| program.scopes.get(&enum_.variants_scope_id))
            .map(|scope| {
                (
                    scope.name_to_id_map.get("Some").copied(),
                    scope.name_to_id_map.get("None").copied(),
                )
            });
        match variants {
            Some((Some(some_variant), Some(none_variant))) => {
                plan.some_variant = Some(some_variant);
                plan.none_variant = Some(none_variant);
            }
            // No anchor entity: a missing std module is the toolchain's
            // problem, reported against the entry.
            _ => errors.push((
                Error {
                    trace: Vec::new(),
                    note: None,
                    span: crate::span::Span { start: 0, end: 0 },
                    msg: "`get_safe` needs `std::option::Option` loaded".to_string(),
                },
                SourceId(0),
            )),
        }
    }

    if errors.is_empty() {
        Ok(plan)
    } else {
        Err(errors)
    }
}

/// Applies a validated plan, mutating the IR in place.
fn apply(program: &mut Program, plan: Plan) {
    let mut next_id = program.next_entity_id;
    let mut fresh = || {
        let id = Id(next_id);
        next_id += 1;
        id
    };

    // (context, node) -> the parameter id that holds the value inside that node.
    let mut source: HashMap<(Id, Id), Id> = HashMap::default();

    // Give each function and `run` closure its own hidden parameter. The
    // parameter is deliberately record-less (no `parameters` entry, no span,
    // no `expr_types` label — it is not source), so it is marked in
    // `context_hidden_parameters` for tooling to recognize and answer
    // honestly (editing-dx.md §19.3).
    for &(context, node) in &plan.param_nodes {
        let parameter = fresh();
        program
            .entity_map
            .insert(parameter, Expr::Parameter(parameter));
        program.context_hidden_parameters.insert(parameter, context);
        if let Some(function) = program.functions.get_mut(&node) {
            function.parameters.push(parameter);
        } else if let Some(closure) = program.closures.get_mut(&node) {
            closure.parameters.push(parameter);
        }
        source.insert((context, node), parameter);
    }

    // A captured closure reuses its provider's parameter.
    for &(context, closure, provider) in &plan.captures {
        if let Some(&parameter) = source.get(&(context, provider)) {
            source.insert((context, closure), parameter);
        }
    }

    // Spawn registration: each spawn reads the ambient nursery from its
    // owner's in-scope parameter. The transformer passes the value as
    // `__task`'s third argument (unwrapping the `Option` of a safe holder).
    for &(spawn_entity, context, owner, bare) in &plan.spawns {
        if let Some(&parameter) = source.get(&(context, owner.id())) {
            let reference = fresh();
            program.entity_map.insert(reference, Expr::Local(parameter));
            program
                .spawn_nursery_sources
                .insert(spawn_entity, (reference, !bare));
        }
    }

    let empty_span = crate::span::Span { start: 0, end: 0 };
    // Synthesizes `Some(parameter)`: a fresh call to the `Option::Some`
    // variant constructor. The transformer lowers a variant-subject call to
    // the variant value directly, so no method records are needed.
    let wrap_in_some = |program: &mut Program, parameter: Id, next: &mut dyn FnMut() -> Id| {
        let some_variant = plan
            .some_variant
            .expect("safe sites resolved the Option variants");
        let subject = next();
        program
            .entity_map
            .insert(subject, Expr::Local(some_variant));
        let value_reference = next();
        program
            .entity_map
            .insert(value_reference, Expr::Local(parameter));
        let call = next();
        program.function_calls.insert(
            call,
            crate::analyzer::FunctionCall {
                id: call,
                subject_id: subject,
                generic_argument_ids: Vec::new(),
                argument_ids: vec![value_reference],
                arguments_span: empty_span,
            },
        );
        program.entity_map.insert(call, Expr::Call(call));
        call
    };

    // `get()` becomes a read of the in-scope parameter; a safe read of a
    // BARE holder becomes `Some(parameter)` (the get's own call entity is
    // rewritten into the wrap, its method records purged like `run`'s).
    for &(call_id, context, owner, wrap_some) in &plan.gets {
        if let Some(&parameter) = source.get(&(context, owner.id())) {
            if wrap_some {
                let some_variant = plan
                    .some_variant
                    .expect("safe sites resolved the Option variants");
                let subject = fresh();
                program
                    .entity_map
                    .insert(subject, Expr::Local(some_variant));
                let value_reference = fresh();
                program
                    .entity_map
                    .insert(value_reference, Expr::Local(parameter));
                if let Some(call) = program.function_calls.get_mut(&call_id) {
                    // Record the subject this rewire erases — the wired
                    // `Local(get_safe_fn)` naming the SOURCE callee — so
                    // tooling can still answer it (editing-dx.md §19.3).
                    let erased_subject = call.subject_id;
                    call.subject_id = subject;
                    call.generic_argument_ids = Vec::new();
                    call.argument_ids = vec![value_reference];
                    program
                        .context_erased_subjects
                        .insert(call_id, erased_subject);
                }
                program.method_call_substitution.remove(&call_id);
                program.generic_dispatch.remove(&call_id);
            } else {
                program.entity_map.insert(call_id, Expr::Local(parameter));
            }
        }
    }

    // Each call to a needs-context function gets the value appended as an
    // argument — the caller's parameter, `Some`-wrapped at a covered→safe
    // boundary.
    for &(call_id, context, ref form) in &plan.thread_calls {
        let argument = match *form {
            ThreadForm::Param { owner } => {
                let Some(&parameter) = source.get(&(context, owner.id())) else {
                    continue;
                };
                let reference = fresh();
                program.entity_map.insert(reference, Expr::Local(parameter));
                reference
            }
            ThreadForm::WrapSome { owner } => {
                let Some(&parameter) = source.get(&(context, owner.id())) else {
                    continue;
                };
                wrap_in_some(program, parameter, &mut fresh)
            }
            ThreadForm::NoneLiteral => {
                let none_variant = plan
                    .none_variant
                    .expect("safe sites resolved the Option variants");
                let reference = fresh();
                program
                    .entity_map
                    .insert(reference, Expr::Local(none_variant));
                reference
            }
        };
        if let Some(call) = program.function_calls.get_mut(&call_id) {
            call.argument_ids.push(argument);
        }
    }

    // Safe reads inside the inlined entry `main` are literal `None`s.
    for &call_id in &plan.none_gets {
        let none_variant = plan
            .none_variant
            .expect("safe sites resolved the Option variants");
        program
            .entity_map
            .insert(call_id, Expr::Local(none_variant));
    }

    // `run(value, body)` becomes `body(value)`: the body closure is the new
    // call subject, the value its sole argument (binding the closure's hidden
    // parameter).
    for site in &plan.runs {
        if let Some(call) = program.function_calls.get_mut(&site.call_id) {
            // As with the covered `get_safe`: the erased subject is the
            // wired `Local(run_fn)`, recorded for tooling (§19.3).
            let erased_subject = call.subject_id;
            call.subject_id = site.closure_entity;
            call.argument_ids = vec![site.value_id];
            program
                .context_erased_subjects
                .insert(site.call_id, erased_subject);
        }
        // The call entity keeps its id, so purge the METHOD-call records the
        // analyzer attached to `Context::run` — a stale substitution would
        // make the emitter monomorphize the new subject (for a value body, a
        // plain parameter) as if it were a generic function.
        program.method_call_substitution.remove(&site.call_id);
        program.generic_dispatch.remove(&site.call_id);
    }

    // `Context::new()` lowers to an opaque value; its binding is now unused.
    for &call_id in &plan.news {
        program.entity_map.insert(call_id, Expr::Null);
    }

    program.next_entity_id = next_id;
}
