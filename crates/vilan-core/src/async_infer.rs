//! Async inference: decides which functions and closures are async — i.e. which
//! must compile to a JS `async function`/`async () =>`, and which calls must be
//! implicitly `await`ed.
//!
//! Like the `context` pass, this is an effect over the [call graph](crate::call_graph):
//! the leaves are async externs and explicit `await`s, and the effect propagates
//! callee → caller (a function that calls an async function implicitly awaits it,
//! so it becomes async too). An `async { .. }` block is a separate, always-async
//! closure node with no call edge into it, so it is a natural boundary: the
//! awaits inside a spawned promise don't make the enclosing function async.
//!
//! A trait/generic-bounded method call (`(self.inner).fetch()` where `self.inner:
//! T`, `T: Fetcher`) can't be pinned to a single callee at the graph's
//! pre-monomorphization granularity, so the call graph records it as
//! [`CallTarget::Indirect`](crate::call_graph::CallTarget). But Vilan has no `dyn`:
//! every such call resolves to a statically-known impl at each monomorphization.
//! So the effect still propagates — through the *contract*. A dispatched method is
//! treated as async if **any** candidate (an impl's member, or the trait's default)
//! is async, because then some monomorphization awaits it and the caller must be
//! `async`. Over-marking a purely-sync instance is harmless: the transformer awaits
//! a non-promise, which is a JS no-op. Without this the transformer (which resolves
//! the concrete async callee post-monomorphization) would emit an `await` inside a
//! function this pass left non-`async` — invalid JavaScript.
//!
//! The result is `Program::async_functions`, read by the transformer.

use crate::analyzer::{Expr, GenericDispatch, Program, SourceId};
use crate::call_graph::{Call, CallGraph, CallTarget, IndirectReason};
use crate::fx::{FxHashMap as HashMap, FxHashSet as HashSet};
use crate::id::Id;
use crate::type_::{Type, TypeId};

/// Computes the async set and stores it on the program.
///
/// Takes the analysis tail's shared call graph rather than building its own
/// (E35): it writes only `async_functions` / `async_values` / `awaited_calls` /
/// `adapted_instances` and diagnostics, none of which the graph is derived
/// from, so its view of the program is bit-for-bit the one it used to build.
pub fn infer(program: &mut Program, graph: &CallGraph) {
    let mut async_set: HashSet<Id> = HashSet::default();

    // Every value each binding ever holds — its initializer plus every
    // reassignment (`mut` rebinds) — for async ADOPTION: a binding holding
    // an async closure through any of them awaits when called. The same one
    // walk of the entity map finds the `async { .. }` blocks.
    let mut held_values: HashMap<Id, Vec<Id>> = HashMap::default();
    for (variable_id, variable) in &program.variables {
        if let Some(initial) = variable.initial {
            held_values.entry(*variable_id).or_default().push(initial);
        }
    }
    let mut async_blocks: Vec<Id> = Vec::new();
    // And A.4's escape positions: every store into a struct field, filtered
    // to the plain closure-typed ones below (`Escapes`).
    let mut field_stores: Vec<FieldStore> = Vec::new();
    for expr in program.entity_map.values() {
        match expr {
            Expr::Assignment(target_id, value_id) => match program.entity_map.get(target_id) {
                Some(Expr::Local(variable_id)) => {
                    held_values.entry(*variable_id).or_default().push(*value_id);
                }
                Some(Expr::Field(_, struct_id, field_index)) => field_stores.push(FieldStore {
                    struct_id: *struct_id,
                    field_index: *field_index,
                    value_id: *value_id,
                }),
                _ => {}
            },
            // The initializer expr carries its OWN id; the struct def
            // resolves through `struct_initializer_to_def`.
            Expr::StructInitializer(initializer_id, field_values) => {
                if let Some(struct_id) = program.struct_initializer_to_def.get(initializer_id) {
                    field_stores.extend(field_values.iter().map(|(field_index, value_id)| {
                        FieldStore {
                            struct_id: *struct_id,
                            field_index: *field_index,
                            value_id: *value_id,
                        }
                    }));
                }
            }
            Expr::Async(closure_id) => async_blocks.push(*closure_id),
            _ => {}
        }
    }
    let escapes = Escapes::of(program, field_stores);

    // --- Seeds ---
    // Declared-async functions and externs (an extern is async only by
    // declaration, having no body to infer from).
    for (id, function) in &program.functions {
        if function.is_async {
            async_set.insert(*id);
        }
    }
    for (id, external) in &program.external_functions {
        if external.is_async {
            async_set.insert(*id);
        }
    }
    // A node whose own body awaits.
    for node in graph.nodes() {
        if graph.node_awaits(node.id()) {
            async_set.insert(node.id());
        }
    }
    // An `async { .. }` block lowers to an always-async closure.
    async_set.extend(async_blocks);

    // --- Fixpoint: a node that calls an async function/extern implicitly
    // awaits it, so it is async too — interleaved with the ADAPTATION
    // worklist (async-polymorphism.md A.1) until both are stable: adaptation
    // can flip a base function async (it calls an adapted async instance),
    // which the plain fixpoint must then propagate to ITS callers, which can
    // in turn create new adapted instances. Both are monotone over
    // `async_set`, so this terminates.
    // M110 S3b (M127, Order 50): the fixpoint starts from the world's COLD
    // result — the least fixpoint over the cold nodes with every hot callee
    // unknown — when the record carries one; the hot nodes and whatever they
    // make async are what the rounds below still settle. Never from the last
    // analysis's settled set (the plant): a monotone fixpoint cannot shrink
    // from a seed that already holds an effect the keystroke removed.
    // Read and written only while the record describes this analysis's
    // rewrite state: a lowered `run` is a call of its body, so the cold
    // result moves with the rewrite (a hot edit that stands it down makes a
    // prefix function synchronous again).
    // M159: the seam as this pass finds it, on the census (the counters line
    // prints it; the lone-document differential asserts it).
    if let Some(seam) = program.seam.as_ref()
        && !crate::macros::in_macro_world()
    {
        let state = crate::incremental::PostSeamState {
            seeded: seam.seeded,
            record: seam.record.is_some(),
            matches: seam.record_matches,
            cold: seam
                .record
                .as_ref()
                .is_some_and(|record| record.async_cold.get().is_some()),
        };
        crate::incremental::update_census(|census| census.post_seam = Some(state));
    }
    let record = program
        .seam
        .as_ref()
        .filter(|seam| seam.record_matches)
        .and_then(|seam| seam.record.clone());
    let seeded = record
        .as_ref()
        .and_then(|record| record.async_cold.get())
        .is_some();
    // M138: the program's OWN seeds — what the cold computations below start
    // from — before any recorded result joins them.
    let own_seeds = async_set.clone();
    // M138's seeded form: the cold instance record's settled set holds S3b's
    // cold result and the worklist's cold flips, so the first round grows the
    // set only by what the hot set makes async.
    let instances = record
        .as_ref()
        .and_then(|record| record.async_instances.get().cloned());
    if let Some(instances) = instances.as_ref() {
        async_set.extend(instances.settled.iter().copied());
    }
    if let Some(cold) = record.as_ref().and_then(|record| record.async_cold.get()) {
        async_set.extend(cold.iter().copied());
    }
    if seeded || instances.is_some() {
        crate::incremental::update_census(|census| census.seeded_passes += 1);
    }
    // M138: the worklist re-runs whole while a round grows the async set, but
    // the DECISIONS pass (each instance's emission data and its diagnostics,
    // which reads the set and moves nothing) runs once, over the last round's
    // stable flags: on kolt it was most of each round, and every round but the
    // last threw its answer away.
    let adaptive = adaptive_params_of(program);
    ADAPTATION_INFERS.with(|count| count.set(count.get() + 1));
    let (adaptation, cold_result, filed) = {
        let adapt = Adapt {
            program,
            graph,
            held_values: &held_values,
            adaptive: &adaptive,
            escapes: &escapes,
        };
        let mut seeding = instances.as_deref().map(Seeding::new);
        let worklist = loop {
            base_fixpoint(program, graph, &held_values, &mut async_set);
            let before = async_set.len();
            let worklist = adaptation_worklist(adapt, &mut async_set, seeding.as_mut());
            if async_set.len() == before {
                break worklist;
            }
        };
        let adaptation = adaptation_decisions(adapt, &async_set, worklist, seeding.as_ref());
        if let Some(seeding) = seeding.as_ref() {
            let rewalked = seeding.rewalked;
            let served = seeding.record.keys.len() as u64 - rewalked;
            crate::incremental::update_census(|census| {
                census.async_keys_served += served;
                census.async_keys_rewalked += rewalked;
            });
        }
        // The record this analysis filed (or read without a cold result yet)
        // takes the cold result now: the base fixpoint over the cold nodes
        // alone, in the cold view, from the program's own seeds, so a hot
        // callee is never in the set and never counts as suspending. The
        // plant records the settled set instead.
        let cold_result = record.as_ref().filter(|_| !seeded).map(|_| {
            if crate::incremental::planted(crate::incremental::Plant::PostSeedFromSettled) {
                async_set.clone()
            } else {
                let mut cold: HashSet<Id> = own_seeds
                    .iter()
                    .copied()
                    .filter(|id| program.cold_entity(*id))
                    .collect();
                cold_base_fixpoint(adapt, &mut cold);
                cold
            }
        });
        // M138: and the cold instance record, once per post-pass record.
        let filed = record
            .as_ref()
            .filter(|record| record.async_instances.get().is_none())
            .filter(|_| !crate::macros::in_macro_world())
            .map(|_| std::sync::Arc::new(cold_instance_record(adapt, &own_seeds)));
        (adaptation, cold_result, filed)
    };
    for (error, source) in adaptation.diagnostics {
        program.push_diagnostic(error, source);
    }
    program.adapted_instances = adaptation.instances;
    if let (Some(record), Some(cold)) = (record.as_ref(), cold_result) {
        let mut cold: Vec<Id> = cold.into_iter().collect();
        cold.sort_unstable_by_key(|id| id.0);
        let _ = record.async_cold.set(cold);
    }
    if let (Some(record), Some(filed)) = (record.as_ref(), filed) {
        let bytes = filed.bytes;
        if record.async_instances.set(filed).is_ok() {
            // The record's bytes join its post record's under the checks
            // cache's budget (M46), as if it had been filed with it.
            if let Some(seam) = program.seam.as_ref() {
                crate::analyzer::checked_cache_grow_post(
                    &seam.key,
                    &seam.prefix_hashes,
                    seam.cold_fingerprint,
                    bytes,
                );
            }
            crate::incremental::update_census(|census| {
                census.async_records_filed += 1;
                census.async_record_bytes += bytes as u64;
            });
        }
    }

    // --- Materialize the value-flow channels for emission (J2): adopted
    // bindings — unannotated ones holding an async closure — join
    // `async_values`, so the transformer's Local-subject await covers them;
    // calls through an async field or an async-returning call land in
    // `awaited_calls` for the transformer's non-Local subjects (in the
    // divergence check's pass over the calls, below). The adopted bindings
    // join before the divergence and initializer checks so both see the full
    // set.
    let adopted: Vec<Id> = program
        .variables
        .keys()
        .filter(|variable| {
            !program.async_values.contains(variable)
                && binding_holds_async(
                    program,
                    **variable,
                    &held_values,
                    View::plain(&async_set),
                    0,
                )
        })
        .copied()
        .collect();
    program.async_values.extend(adopted);
    // --- The J2 divergence check, post-adaptation: an async closure into a
    // `sync`-contract parameter, or into a host (`external`) callback that
    // cannot await it, is refused. (A PLAIN value-returning parameter now
    // ADAPTS instead of erroring — the worklist above; void-returning
    // parameters stay legal as spawn semantics.) M138: one pass over the
    // calls answers both checks and the value-flow awaits above (each call's
    // subject read once); the host refusals still follow every `sync` one.
    let mut awaited: Vec<Id> = Vec::new();
    let mut divergences: Vec<(crate::error::Error, SourceId)> = Vec::new();
    let mut host_divergences: Vec<(crate::error::Error, SourceId)> = Vec::new();
    let no_flags: HashMap<Id, bool> = HashMap::default();
    for (call_id, function_call) in &program.function_calls {
        let subject = program.entity_map.get(&function_call.subject_id);
        let awaits = match subject {
            Some(Expr::Field(_, struct_id, index)) => {
                program.async_fields.contains(&(*struct_id, *index))
            }
            Some(Expr::Call(inner)) => call_returns_async_closure(program, *inner),
            // A directly-applied async closure literal (the lowered `run`
            // body): a statically-known await point.
            Some(Expr::Closure(closure_id)) => async_set.contains(closure_id),
            _ => false,
        };
        if awaits {
            awaited.push(*call_id);
        }
        let Some(Expr::Local(target)) = subject else {
            continue;
        };
        if let Some(function) = program.functions.get(target) {
            for (argument, parameter) in function_call.argument_ids.iter().zip(&function.parameters)
            {
                if program.async_values.contains(parameter) {
                    continue;
                }
                let Some(parameter_record) = program.parameters.get(parameter) else {
                    continue;
                };
                // A `sync`-marked parameter is a deliberate contract
                // (async-polymorphism.md A.2) — refused, with the contract's
                // steer. The marker is the whole test: WHAT the callback returns
                // decides adaptation, not the contract. Without it there is
                // nothing to diverge from here — a plain value-returning
                // parameter ADAPTS (the instance worklist handled it), a void one
                // is spawn semantics, and an unresolved return can't be a known
                // lie. (B61: gating this on a resolved non-void return let
                // `sync || void` accept an awaiting closure that `sync || i32`
                // refused, so the marker did not bite on a void callback.)
                if !program.sync_values.contains(parameter) {
                    continue;
                }
                if !parameter_is_closure(program, *parameter) {
                    continue;
                }
                // The argument VALUE is async through any channel — a literal, a
                // declared/adopted binding or parameter, an `async` field read,
                // an async-returning call (the same shapes that make a call
                // through it await).
                if !value_async_in(
                    program,
                    &held_values,
                    View::plain(&async_set),
                    &no_flags,
                    &[],
                    *argument,
                ) {
                    continue;
                }
                divergences.push(anchored(
                    program,
                    *parameter,
                    format!(
                        "`{}` requires a synchronous closure (`sync`): its completion is part of the declaring function's synchronous protocol; move the async work outside the callback (e.g. a `turn` with an awaiting body, `Draft`, or a spawned `async` block)",
                        parameter_record.name
                    ),
                    None,
                ));
            }
            continue;
        }
        // Host boundary: an `external` function cannot await a vilan closure
        // — its value-returning closure parameters are implicitly `sync`
        // (async-polymorphism.md A.4); void ones keep spawn semantics.
        //
        // `Context::run` is `external` only as a type-checking fiction: the
        // context threading pass erases every `run` call it accepts (the body
        // becomes a direct call), so `run` is never a host boundary at
        // runtime. A `run` call survives to this pass only when
        // `thread_contexts` refused the rewrite and already said why (a
        // coverage or `run`-shape error); re-judging its body here as a
        // host-await misuse cascades a false secondary — anchored in std —
        // on top of that primary (E68).
        if Some(*target) == program.context_run_fn_id
            || Some(*target) == program.context_clear_fn_id
        {
            continue;
        }
        let Some(external) = program.external_functions.get(target) else {
            continue;
        };
        for (argument, parameter) in function_call.argument_ids.iter().zip(&external.parameters) {
            // A DECLARED `async |…| T` parameter is the typed channel: the
            // host explicitly contracts to await the closure (it receives an
            // async function — `__nursery_run` awaits its body this way).
            if program.async_values.contains(parameter) {
                continue;
            }
            let Some(parameter_record) = program.parameters.get(parameter) else {
                continue;
            };
            let Some(Type::Closure(_, return_type, _, _)) = program
                .type_id_to_type_map
                .get(&parameter_record.type_id)
                .cloned()
            else {
                continue;
            };
            if matches!(
                program.type_id_to_type_map.get(&return_type),
                Some(Type::Void) | Some(Type::Unknown) | Some(Type::Unresolved) | None
            ) {
                continue;
            }
            if !value_async_in(
                program,
                &held_values,
                View::plain(&async_set),
                &no_flags,
                &[],
                *argument,
            ) {
                continue;
            }
            host_divergences.push(anchored(
                program,
                *argument,
                format!(
                    "`{}` is a host (`external`) function: it cannot await a Vilan closure, so this parameter only accepts synchronous closures (or a void-returning one for spawn semantics)",
                    external.name
                ),
                None,
            ));
        }
    }
    program.awaited_calls.extend(awaited);
    divergences.extend(host_divergences);
    // The same divergence through the FIELD channel: an async closure stored
    // into a plain closure field (struct literal or field assignment) with a
    // non-void return — every later read-and-call would hand back a promise
    // typed `T`. Declared `async || T` fields are exempt (that is the fix).
    for escape in &escapes.field_stores {
        if !value_async_in(
            program,
            &held_values,
            View::plain(&async_set),
            &no_flags,
            &[],
            escape.value_id,
        ) {
            continue;
        }
        divergences.push(anchored(
            program,
            escape.value_id,
            field_escape_message(program, escape),
            None,
        ));
    }

    // And through the RETURN channel: a function whose declared return type
    // is a plain closure (non-void) returning an async closure. The declared
    // `async || T` return marker is the fix.
    for &(function_id, value_id) in &escapes.return_sites {
        if !value_async_in(
            program,
            &held_values,
            View::plain(&async_set),
            &no_flags,
            &[],
            value_id,
        ) {
            continue;
        }
        divergences.push(anchored(
            program,
            value_id,
            return_escape_message(program, function_id),
            None,
        ));
    }

    for (error, source) in divergences {
        program.push_diagnostic(error, source);
    }

    // --- Module-level initializers cannot suspend (backlog §J.3,
    // `top-level-await.md`): they run at module load, where there is no
    // enclosing function to become async. Initialization is synchronous *on
    // purpose* — not for want of an emission model — because the order it
    // runs in is derived (B33), and suspending a derived sequence makes an
    // unrelated binding's completion depend on an order the user never wrote.
    //
    // The rule is AWAIT-shaped, and it takes two complementary checks:
    //
    //   - the CALL check catches the implicit await — a call to an
    //     (inferred-)async function, which would leave a live promise typed
    //     as `T` (`state + 1` on it is garbage);
    //   - the AWAIT check catches the explicit one, whose operand is not a
    //     call at all: `await` of a `Task`-valued binding, of a spawn, of a
    //     `Task` returned by a plain sync function. A call-shaped check is
    //     blind to every one of those, and they emitted real top-level await
    //     (`top-level-await.md` §1.3) — ungated, and miscompiled on the Node
    //     and HMR legs.
    //
    // Creating an async closure (or an `async { .. }` block) in an
    // initializer stays legal, and so does *spawning* — `let pending:
    // Task<T> = async f();` — because nothing suspends at load. That is why
    // the await check reads the graph's initializer awaits, which do not
    // descend into a created closure: the closure's body is not the
    // initializer. `const` initializers never reach here (they are
    // compile-time; the graph skips them).
    //
    // F6 gates the check exactly as it gates emission and coloring: a
    // binding the entry never reaches never runs, so it cannot await —
    // with no user `main` (a library, a fragment) every binding is checked,
    // since each runs in some dependent program.
    // Which module bindings RUN is a whole reachability walk from `main`
    // (`reachable_bindings`: 30 ms of kolt's client leg, Order 50), asked only
    // to drop refusals at bindings nothing reaches — so the refusals are
    // derived first, per binding, and the walk is paid only when one exists.
    // Deriving is pure (the targets are read, nothing is written), so the
    // order of the refusals that survive is the bindings' order either way.
    let module_bindings: HashSet<Id> = program.module_level_bindings().into_iter().collect();
    let mut initializer_refusals: Vec<(crate::error::Error, SourceId)> = Vec::new();
    let mut refusal_bindings: Vec<(Id, usize)> = Vec::new();
    for binding in program.module_level_bindings() {
        let refusals_before = initializer_refusals.len();
        for call in graph.initializer_calls_of(binding) {
            let async_target = match call.target {
                // A call whose async closure arguments ADAPT the callee
                // (async-polymorphism.md A.1) awaits like any async call —
                // and a module initializer cannot.
                CallTarget::Function(callee)
                    if {
                        let bits = bits_for_call(
                            program,
                            &held_values,
                            View::plain(&async_set),
                            &adaptive,
                            &HashMap::default(),
                            &[],
                            callee,
                            call.call_id,
                        );
                        !bits.is_empty()
                            && program
                                .adapted_instances
                                .get(&(callee, bits))
                                .is_some_and(|instance| instance.is_async)
                    } =>
                {
                    Some(callee)
                }
                CallTarget::Function(callee) | CallTarget::External(callee) => {
                    async_set.contains(&callee).then_some(callee)
                }
                CallTarget::Indirect(
                    IndirectReason::GenericMember | IndirectReason::TraitDispatch,
                ) => dispatch_candidates(program, call.call_id)
                    .into_iter()
                    .find(|member| async_set.contains(member)),
                // A call through an `async || T`-typed value awaits too — as
                // does a directly-applied async closure literal (the lowered
                // `run` body).
                _ => {
                    program.function_calls.get(&call.call_id).and_then(
                        |function_call| match program.entity_map.get(&function_call.subject_id) {
                            Some(Expr::Local(target)) if program.async_values.contains(target) => {
                                Some(*target)
                            }
                            Some(Expr::Closure(closure_id)) if async_set.contains(closure_id) => {
                                Some(*closure_id)
                            }
                            _ => None,
                        },
                    )
                }
            };
            let Some(target) = async_target else {
                continue;
            };
            let target_name = program
                .functions
                .get(&target)
                .map(|function| function.name)
                .or_else(|| {
                    program
                        .external_functions
                        .get(&target)
                        .map(|external| external.name)
                });
            let binding_name = program
                .variables
                .get(&binding)
                .map(|variable| variable.name)
                .unwrap_or("_");
            // A nameless target is an awaiting closure applied directly (an
            // adopted value, or a lowered `run` body) — phrase it as what it
            // is rather than backticking a description.
            let culprit = match target_name {
                Some(name) => format!("calls `{name}`, which is async"),
                None => "runs a closure that awaits".to_string(),
            };
            initializer_refusals.push(anchored(
                program,
                call.call_id,
                format!(
                    "the initializer of `{binding_name}` {culprit}: a module-level \
                     binding cannot await (module initialization is synchronous); wrap \
                     the work in a function and call it from `main`"
                ),
                None,
            ));
        }

        // The explicit `await`, whose operand is not an async call. Only when
        // the call check stayed silent: `await ready()` is BOTH an await and
        // an async call, and the call form's message names the callee, so it
        // is the better of the two — one refusal per binding, never a pair
        // for one line. One error per binding, at its first `await`: further
        // awaits in the same initializer are the same defect with the same
        // fix.
        if initializer_refusals.len() == refusals_before
            && let Some(&await_id) = graph.initializer_awaits_of(binding).first()
        {
            let binding_name = program
                .variables
                .get(&binding)
                .map(|variable| variable.name)
                .unwrap_or("_");
            // Two steers, both true of what the user wrote. When the
            // operand names a module-level binding it is already there
            // and already initialized (B33 orders it first), so the fix
            // is to move only the `await`. Otherwise the operand is an
            // expression — a spawn, or a call returning a `Task` — and
            // the fix is to bind it first.
            let awaited_binding = match program.entity_map.get(&await_id) {
                Some(Expr::Await(operand_id)) => match program.entity_map.get(operand_id) {
                    Some(Expr::Local(target) | Expr::Variable(target))
                        if module_bindings.contains(target) =>
                    {
                        program.variables.get(target).map(|variable| variable.name)
                    }
                    _ => None,
                },
                _ => None,
            };
            // The trailing clause is a general fact about module-level
            // spawning, not a claim about the operand — `await` on a
            // non-`Task` is legal too, and the steer must stay true there.
            let steer = match awaited_binding {
                Some(name) => format!(
                    "hold `{name}` here and `await` it in `main` — spawning at module \
                         level does not suspend, so the work still starts at load"
                ),
                None => "bind the `Task` here (`let pending: Task<T> = async …;`) and \
                             `await` it in `main` — spawning at module level does not \
                             suspend, so the work still starts at load"
                    .to_string(),
            };
            initializer_refusals.push(anchored(
                program,
                await_id,
                format!(
                    "the initializer of `{binding_name}` awaits: a module-level binding \
                         cannot suspend (module initialization is synchronous)"
                ),
                Some(crate::error::Note::here(span_of(program, await_id), steer)),
            ));
        }
        if initializer_refusals.len() > refusals_before {
            refusal_bindings.push((binding, initializer_refusals.len()));
        }
    }
    if !refusal_bindings.is_empty() {
        let running_bindings = crate::platform_color::entry_function(program)
            .map(|entry| crate::platform_color::reachable_bindings(program, graph, entry, &[]));
        let mut kept: Vec<(crate::error::Error, SourceId)> = Vec::new();
        let mut from = 0;
        for (binding, to) in refusal_bindings {
            let runs = running_bindings
                .as_ref()
                .is_none_or(|running| running.contains(&binding));
            if runs {
                kept.extend(initializer_refusals[from..to].iter().cloned());
            }
            from = to;
        }
        initializer_refusals = kept;
    }
    for (error, source) in initializer_refusals {
        program.push_diagnostic(error, source);
    }

    // --- B119 (view-invalidation.md §7): materialize the per-CALL-SITE
    // answer the view rule reads. The fixpoint above already decided it for
    // every edge — it is what made each caller async — but it decided it as a
    // per-NODE boolean and threw the sites away. One more pass over the same
    // edges, with the same `call_suspends`, records them.
    //
    // Runs here, after `async_values` / `awaited_calls` took their adopted and
    // value-flow entries, so the channels those opened are seen; and once,
    // per analysis, not per query.
    //
    // Initializer calls are deliberately absent: a module-level initializer
    // that suspends is refused outright above, and the invalidation scan does
    // not walk initializers, so there is no site here to answer for.
    let mut suspending_calls: HashSet<Id> = HashSet::default();
    let mut dispatch_verdicts: HashMap<(bool, Option<TypeId>, &str), bool> = HashMap::default();
    for node in graph.nodes() {
        for call in graph.calls_of(node.id()) {
            if call_suspends_memoized(
                program,
                call,
                &held_values,
                &async_set,
                &mut dispatch_verdicts,
            ) {
                suspending_calls.insert(call.call_id);
            }
        }
    }
    program.suspending_calls = suspending_calls;

    for (error, source) in object_asyncness_refusals(program, &async_set) {
        program.push_diagnostic(error, source);
    }

    program.async_functions = async_set;
}

/// A124 R3 / trait-objects.md §5 (i): a trait member's DECLARED asyncness binds
/// in object position.
///
/// B29 lets an impl disagree with its trait's declaration because every
/// dispatch is monomorphized: `fun consume<T: Fetch>(v: T)` is emitted once per
/// `T`, and each instance awaits or does not by the member it bound. A call
/// through a `dyn` is emitted ONCE, against the declaration, for every value
/// the object may hold — so a sync declaration's call site does not await, and
/// an async implementation behind it hands back a promise where a value was
/// typed: `print(f.get())` printed `Promise { <pending> }`.
///
/// Refused at the COERCION, which is the one place the concrete type and the
/// object's trait are both in hand; and only for the members a call actually
/// reaches through an object of that trait (the table's own slot set), since a
/// member no object call reaches is never dispatched through one and keeps
/// B29's freedom untouched. The other disagreement — an async declaration, a
/// sync implementation — is sound as it stands: the call site awaits, and
/// awaiting a plain value is a no-op.
fn object_asyncness_refusals(
    program: &Program,
    async_set: &HashSet<Id>,
) -> Vec<(crate::error::Error, SourceId)> {
    let mut refusals = Vec::new();
    if program.dyn_coercions.is_empty() && program.dyn_tuple_coercions.is_empty() {
        return refusals;
    }
    // B430: an element-wise tuple erasure is one coercion per erased element,
    // each anchored at the tuple value.
    let mut coercions: Vec<(&Id, &(TypeId, Id, Vec<TypeId>))> = program
        .dyn_coercions
        .iter()
        .chain(
            program
                .dyn_tuple_coercions
                .iter()
                .flat_map(|(expr_id, elements)| {
                    elements
                        .iter()
                        .flatten()
                        .map(move |element| (expr_id, element))
                }),
        )
        .collect();
    coercions.sort_by_key(|(expr_id, _)| expr_id.0);
    for (expr_id, (subject_type_id, trait_id, trait_arguments)) in coercions {
        let mut members: Vec<&str> = program
            .dyn_dispatched_members
            .iter()
            .filter(|(object_trait, _)| object_trait == trait_id)
            .map(|(_, member)| *member)
            .collect();
        members.sort_unstable();
        for member in members {
            let Some((declaration_id, declaring_trait_id)) =
                trait_declaration(program, *trait_id, member)
            else {
                continue;
            };
            if async_set.contains(&declaration_id) {
                continue;
            }
            let wanted = crate::impl_select::WantedTrait {
                trait_id: *trait_id,
                arguments: trait_arguments,
            };
            let Some(selected) = crate::impl_select::select_member(
                program,
                None,
                *subject_type_id,
                member,
                Some(wanted),
            ) else {
                continue;
            };
            if selected.member_id == declaration_id || !async_set.contains(&selected.member_id) {
                continue;
            }
            let trait_name = program
                .traits
                .get(&declaring_trait_id)
                .map(|trait_| trait_.name)
                .unwrap_or("the trait");
            let subject_name = match program.type_id_to_type_map.get(subject_type_id) {
                Some(Type::Struct(id, _)) => program.structs.get(id).map(|found| found.name),
                Some(Type::Enum(id, _)) => program.enums.get(id).map(|found| found.name),
                _ => None,
            }
            .map(|name| format!("`{name}`'s"))
            .unwrap_or_else(|| "this value's".to_string());
            refusals.push(anchored(
                program,
                *expr_id,
                format!(
                    "{subject_name} `{member}` is async, but `{trait_name}::{member}` is \
                     declared sync, so this value cannot become a `dyn {trait_name}`: a call \
                     through an object is compiled once, against the declaration, and would \
                     hand back an unawaited promise. Declare `{trait_name}::{member}` `async` \
                     (an implementation may then be either), or keep the value behind a \
                     generic bound, where each implementation is awaited as it is written"
                ),
                None,
            ));
        }
    }
    refusals
}

/// The member `name` as `trait_id` or one of its supertraits DECLARES it, with
/// the declaring trait.
fn trait_declaration(program: &Program, trait_id: Id, name: &str) -> Option<(Id, Id)> {
    let mut stack = vec![trait_id];
    let mut seen: HashSet<Id> = HashSet::default();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(trait_) = program.traits.get(&id) else {
            continue;
        };
        if let Some(member_id) = trait_.declarations.get(name) {
            return Some((*member_id, id));
        }
        for supertrait_type_id in &trait_.supertraits {
            if let Some(Type::Trait(super_id, _)) =
                program.type_id_to_type_map.get(supertrait_type_id)
            {
                stack.push(*super_id);
            }
        }
    }
    None
}

/// Whether a call THROUGH `subject_id` is an await point (J2): the subject is
/// an `async || T`-typed value (declared), a binding HOLDING an async closure
/// (adoption), a read of an `async || T` field, a call to a function whose
/// declared return type carries the marker, or an async closure LITERAL
/// applied directly (the `context` pass lowers `run(value, body)` to
/// `body(value)` — a statically-known callee, awaited like a direct call).
fn subject_awaits(
    program: &Program,
    subject_id: Id,
    held_values: &HashMap<Id, Vec<Id>>,
    view: View,
    depth: u32,
) -> bool {
    match program.entity_map.get(&subject_id) {
        Some(Expr::Local(target)) => {
            program.async_values.contains(target)
                || binding_holds_async(program, *target, held_values, view, depth)
        }
        Some(Expr::Field(_, struct_id, index)) => {
            program.async_fields.contains(&(*struct_id, *index))
        }
        Some(Expr::Call(inner)) => call_returns_async_closure(program, *inner),
        Some(Expr::Closure(closure_id)) => view.contains(program, *closure_id),
        _ => false,
    }
}

/// Adoption (J2): whether the binding holds an async closure through any of
/// its values — the initializer or, for a `mut` binding, any assigned value.
/// A held value counts when it is an async closure literal, a read of an
/// async field, an async-returning call, or another binding that holds one
/// (chains resolve a few hops; the cap keeps a cyclic rebind from looping).
fn binding_holds_async(
    program: &Program,
    variable_id: Id,
    held_values: &HashMap<Id, Vec<Id>>,
    view: View,
    depth: u32,
) -> bool {
    if depth > 4 {
        return false;
    }
    view.note_held(variable_id);
    let Some(held) = held_values.get(&variable_id) else {
        return false;
    };
    held.iter()
        .filter(|value_id| view.admits_held(program, **value_id))
        .any(|value_id| match program.entity_map.get(value_id) {
            Some(Expr::Closure(closure_id)) => view.contains(program, *closure_id),
            Some(Expr::Field(_, struct_id, index)) => {
                program.async_fields.contains(&(*struct_id, *index))
            }
            Some(Expr::Call(inner)) => call_returns_async_closure(program, *inner),
            Some(Expr::Local(source)) => {
                binding_holds_async(program, *source, held_values, view, depth + 1)
            }
            _ => false,
        })
}

/// Whether the call at `call_id` targets a function whose DECLARED return
/// type is `async || T` — its result is an async closure, so calling that
/// result awaits.
fn call_returns_async_closure(program: &Program, call_id: Id) -> bool {
    program
        .function_calls
        .get(&call_id)
        .is_some_and(|function_call| {
            matches!(
                program.entity_map.get(&function_call.subject_id),
                Some(Expr::Local(target)) if program.async_returning.contains(target)
            )
        })
}

/// The concrete member ids a trait/generic-bounded dispatch at `call_id` could
/// resolve to across monomorphizations: an impl's member for the method, or the
/// trait's own default. The async fixpoint marks the caller async if any is.
pub(crate) fn dispatch_candidates(program: &Program, call_id: Id) -> Vec<Id> {
    // A124 R3: a call through an OBJECT reaches every implementation of the
    // member under the trait that declares it (the analyzer resolved the call
    // to that declaration), plus the declaration itself — a default body is the
    // slot for a type that does not override it.
    if let Some(member) = program.dyn_method_calls.get(&call_id)
        && let Some(Expr::Local(declaration)) = program
            .function_calls
            .get(&call_id)
            .and_then(|call| program.entity_map.get(&call.subject_id))
        && let Some((declaring_trait, _)) = program
            .traits
            .iter()
            .find(|(_, trait_)| trait_.declarations.values().any(|id| id == declaration))
    {
        return trait_method_candidates(program, *declaring_trait, member)
            .as_ref()
            .clone();
    }
    let Some(dispatch) = dispatch_at(program, call_id) else {
        return Vec::new();
    };
    match dispatch {
        GenericDispatch::OnConstraint(constraint_id, member) => {
            // Single-bound `T: Trait` resolves precisely to that trait's impls —
            // or, when the member is a SUPERTRAIT's (`P: Pipe<T>` calling
            // `Flow::start`), to the impls of the supertrait that declares it.
            // Reading only the bound's own trait found nothing there and fell
            // back to every same-named member in the program, so a generic
            // `p.start()` reached `std::http`'s `Server::start` and colored a
            // browser build that never touches a server (collections-44's find).
            //
            // F28: a MULTI-bound parameter (`T: Wire + Keyed<K>`) names its
            // bounds in `generic_bounds`, and the member is looked up in each
            // of them. The dispatch record carries one bound, and reading only
            // that one found `key` in none of `Wire`'s chain, so `element.key()`
            // in `std::rpc` fell back to every `key` in the program —
            // `std::web::dom`'s `Event::key` among them — and the fence std's
            // `rpc::server` declares refused kolt's server for a DOM call
            // nothing can make.
            let mut bounds: Vec<Id> = program
                .generic_bounds
                .get(&constraint_id)
                .into_iter()
                .flatten()
                .filter_map(|bound| trait_of(program, *bound))
                .collect();
            if let Some(trait_id) = trait_of(program, constraint_id)
                && !bounds.contains(&trait_id)
            {
                bounds.push(trait_id);
            }
            // M132: the candidate list per (declaring trait, member) is the
            // program's memo, and the union is deduplicated by a set — the
            // linear `contains` made a site over a trait with N impls cost N².
            let mut declaring: Vec<Id> = Vec::new();
            for trait_id in bounds {
                for found in declaring_traits_in_chain(program, trait_id, member) {
                    if !declaring.contains(&found) {
                        declaring.push(found);
                    }
                }
            }
            let precise: Vec<Id> = match declaring.as_slice() {
                [only] => trait_method_candidates(program, *only, member)
                    .as_ref()
                    .clone(),
                _ => {
                    let mut seen: HashSet<Id> = HashSet::default();
                    let mut union = Vec::new();
                    for trait_id in declaring {
                        for candidate in trait_method_candidates(program, trait_id, member).iter() {
                            if seen.insert(*candidate) {
                                union.push(*candidate);
                            }
                        }
                    }
                    union
                }
            };
            // A parameter whose bounds declare no such member (a bound the
            // record cannot name) falls back to every same-named member —
            // over-approximate but sound.
            if precise.is_empty() {
                members_named(program, member)
            } else {
                precise
            }
        }
        // An INHERITED default called on a concrete value: the receiver is
        // KNOWN, and it is narrowed by exactly the same fact as the `self` call
        // below (B254). Recording this dispatch at all means the receiver's own
        // impl chain reached a trait declaring `member` — so an unrelated type's
        // same-named member can never be selected here either.
        GenericDispatch::OnType(Some(_), member) => trait_subject_candidates(program, member),
        // A `self` call inside a default BODY: narrowed to what a `Self` can be.
        GenericDispatch::OnType(None, member) => trait_subject_candidates(program, member),
    }
}

/// The candidate set for an `OnType` dispatch — a `self` re-dispatch inside a
/// trait DEFAULT body (`OnType(None, member)`) and an INHERITED default reached
/// on a concrete value (`OnType(Some(receiver), member)`) alike.
///
/// The record does not carry its trait, so the fallback answer is every
/// same-named member — and that set is wide by a class no such call can ever
/// select. `Self` inside a default body is the type the default is
/// SPECIALIZED for, and that type implements the trait whose body this is: a
/// trait that declares `member`, since finding the member in the trait is what
/// recorded the dispatch. So an IMPL-declared candidate is reachable only if
/// its subject implements some trait declaring `member`; an unrelated type's
/// inherent same-named method is not reachable at all. (The candidate may
/// still be inherent — an impl's own member outranks the trait's, which is
/// what `dispatch_candidates_for` reads for a concrete receiver — so the test
/// is on the SUBJECT, never on the member's own home.)
///
/// A49 is why this matters. `Source::sub` became a trait DEFAULT whose body
/// calls `self.get()`, so every program that also spells an async inherent
/// `get` — the `[service]` macro generates one per `[rpc]` route — colored
/// that default async, and with it every `S: Source<T>` dispatch to `sub`.
/// `std::rpc`'s `|| source.sub(..)` starters, stored into plain
/// `|| Subscription` fields, then reported the field-escape divergence against
/// std's own source. Same shape as the same-named-STATIC miscoloring
/// `is_self_method` closed: a name collision reaching across unrelated types.
///
/// B254 is the KNOWN-receiver sibling, and the argument is the same one: a
/// dispatch is recorded here only because the receiver's own impl chain reached
/// a trait declaring `member` (`inherited_default_candidates`), so the receiver
/// implements such a trait and an unrelated type's member is no more selectable
/// than it was for `Self`. Only the `_for` call sites narrowed before, and they
/// are the per-instantiation refinement rather than the answer every caller
/// gets. `call_graph::successors` and `init_order` read this same set to build
/// reachability edges, so the narrowing removes bogus edges there too — strictly
/// more accurate in the same direction.
///
/// Memoized per member name on the `Program` (M103). The receiver-reachability
/// half — the trait closure, the implementor heads and the `subject_applies`
/// test of every non-nominal implementor — reads only the program's traits,
/// impls and interned type slots, which are settled before the post-passes run;
/// it is asked once per dispatch SITE, and the bound proof inside
/// `subject_applies` grows with every blanket impl std declares (maps-45's map
/// and set operators doubled it: 1.9G instructions of a kolt `check`). The
/// `is_self_method` filter reads parameters, so it stays outside the memo and
/// runs per call, as before.
fn trait_subject_candidates(program: &Program, member: &str) -> Vec<Id> {
    TRAIT_SUBJECT_ASKED.with(|count| count.set(count.get() + 1));
    let cached = program.trait_subject_memo().get(member).cloned();
    let reachable = match cached {
        Some(reachable) => reachable,
        None => {
            TRAIT_SUBJECT_COMPUTED.with(|count| count.set(count.get() + 1));
            let computed = trait_subject_reachable(program, member);
            program
                .trait_subject_memo()
                .insert(member.to_string(), computed.clone());
            computed
        }
    };
    match reachable {
        Some(mut candidates) => {
            candidates.retain(|member_id| is_self_method(program, *member_id));
            candidates
        }
        None => members_named(program, member),
    }
}

thread_local! {
    /// How many times [`trait_subject_candidates`] was asked on this thread,
    /// and how many of those COMPUTED the reachable set rather than reading the
    /// program's memo (M103) — the memo's instrument: the answer is the same
    /// either way, so only the counts can see it. Thread-local because an
    /// analysis is single-threaded and plain `cargo test` runs several at once.
    static TRAIT_SUBJECT_ASKED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static TRAIT_SUBJECT_COMPUTED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// `(asked, computed)` for [`trait_subject_candidates`] on this thread since
/// the last [`reset_trait_subject_counts`]. See [`TRAIT_SUBJECT_ASKED`].
pub fn trait_subject_counts() -> (usize, usize) {
    (
        TRAIT_SUBJECT_ASKED.with(std::cell::Cell::get),
        TRAIT_SUBJECT_COMPUTED.with(std::cell::Cell::get),
    )
}

/// Zeroes this thread's [`trait_subject_counts`].
pub fn reset_trait_subject_counts() {
    TRAIT_SUBJECT_ASKED.with(|count| count.set(0));
    TRAIT_SUBJECT_COMPUTED.with(|count| count.set(0));
}

/// [`trait_subject_candidates`]' memoized half: every impl member and trait
/// declaration named `member` whose subject can be a `Self` of a trait that
/// declares it, before the receiver filter — or `None` when no trait declares
/// `member` at all (the caller falls back to [`members_named`]).
fn trait_subject_reachable(program: &Program, member: &str) -> Option<Vec<Id>> {
    // The traits that could own the body: the ones declaring `member`, plus —
    // since a member reached from a SUPERTRAIT is declared there and inherited
    // here (B205) — every trait whose supertraits reach one. Closing the set
    // downward once is what lets the impl scan below read `trait_ids` flat,
    // rather than expanding each impl's provided set at every call site.
    let mut declaring: HashSet<Id> = program
        .traits
        .iter()
        .filter(|(_, trait_)| trait_.declarations.contains_key(member))
        .map(|(trait_id, _)| *trait_id)
        .collect();
    if declaring.is_empty() {
        return None;
    }
    loop {
        let inheriting: Vec<Id> = program
            .traits
            .iter()
            .filter(|(trait_id, trait_)| {
                !declaring.contains(trait_id)
                    && trait_.supertraits.iter().any(|supertrait| {
                        matches!(
                            program.type_id_to_type_map.get(supertrait),
                            Some(Type::Trait(super_id, _)) if declaring.contains(super_id)
                        )
                    })
            })
            .map(|(trait_id, _)| *trait_id)
            .collect();
        if inheriting.is_empty() {
            break;
        }
        declaring.extend(inheriting);
    }
    // Every subject that implements one of them — the types this body can
    // specialize to. Nominal heads are matched by head (a conditional impl and
    // its unconditional twin share one, which keeps the answer an
    // over-approximation); a subject that is not nominal — a blanket
    // `impl type T with ..`, a primitive — is kept whole and matched with the
    // impl-selection rule instead.
    let mut heads: HashSet<Id> = HashSet::default();
    let mut wide: Vec<TypeId> = Vec::new();
    for implementation in &program.implementations {
        if !implementation
            .trait_ids
            .iter()
            .any(|trait_id| declaring.contains(trait_id))
        {
            continue;
        }
        match subject_head(program, implementation.subject) {
            Some(head) => {
                heads.insert(head);
            }
            None => wide.push(implementation.subject),
        }
    }
    let mut candidates: Vec<Id> = Vec::new();
    for implementation in &program.implementations {
        let Some(member_id) = implementation.declarations.get(member) else {
            continue;
        };
        let reachable = match subject_head(program, implementation.subject) {
            Some(head) => {
                heads.contains(&head)
                    || wide.iter().any(|subject| {
                        crate::impl_select::subject_applies(
                            program,
                            *subject,
                            implementation.subject,
                        )
                    })
            }
            // The candidate's own subject is not nominal either (a blanket
            // impl's member): it applies to whatever binds, this trait's
            // implementors included.
            None => true,
        };
        if reachable {
            candidates.push(*member_id);
        }
    }
    // A trait's own declaration — the requirement or the default — has no
    // concrete subject to test, and the enclosing trait's is always in here.
    for trait_ in program.traits.values() {
        if let Some(member_id) = trait_.declarations.get(member) {
            candidates.push(*member_id);
        }
    }
    Some(candidates)
}

/// An impl subject's nominal head, or `None` when it has none to match on.
fn subject_head(program: &Program, subject: TypeId) -> Option<Id> {
    match program.type_id_to_type_map.get(&subject) {
        Some(Type::Struct(id, _) | Type::Enum(id, _)) => Some(*id),
        _ => None,
    }
}

/// Like [`dispatch_candidates`], but with a resolved concrete RECEIVER type
/// when a per-instantiation walk has one (platform coloring's refinement):
/// only the members that type actually selects — its own impl's declared
/// member, else the trait defaults its impls inherit, else (M133) the members
/// of the BLANKET impls whose subject applies to it. Nominal matching
/// (conditional impls over the same head both match) keeps it a sound
/// over-approximation. A `None` receiver is the plain over-approximation.
pub(crate) fn dispatch_candidates_for(
    program: &Program,
    call_id: Id,
    receiver: Option<TypeId>,
) -> Vec<Id> {
    let Some(receiver) = receiver else {
        return dispatch_candidates(program, call_id);
    };
    let Some(dispatch) = dispatch_at(program, call_id) else {
        return Vec::new();
    };
    let member = match dispatch {
        GenericDispatch::OnConstraint(_, member) | GenericDispatch::OnType(_, member) => member,
    };
    let key = (receiver, member.to_string());
    let cached = program.receiver_dispatch_memo().get(&key).cloned();
    let selected = match cached {
        Some(selected) => selected,
        None => {
            let computed =
                std::sync::Arc::new(receiver_selected_members(program, receiver, member));
            program
                .receiver_dispatch_memo()
                .insert(key, computed.clone());
            computed
        }
    };
    match selected.as_ref() {
        Some(members) => members.clone(),
        // Nothing the receiver selects is known (an unexpected receiver shape,
        // or a generic left in it): stay sound.
        None => dispatch_candidates(program, call_id),
    }
}

/// [`dispatch_candidates_for`]'s memoized half: what a dispatch of `member`
/// on the concrete `receiver` can select, or `None` when that cannot be told
/// from the receiver (the caller takes every candidate).
fn receiver_selected_members(program: &Program, receiver: TypeId, member: &str) -> Option<Vec<Id>> {
    let receiver_head = match program.type_id_to_type_map.get(&receiver) {
        Some(Type::Struct(id, _) | Type::Enum(id, _)) => *id,
        _ => return None,
    };
    let matching: Vec<&crate::analyzer::Implementation> = program
        .implementations
        .iter()
        .filter(|implementation| {
            subject_head(program, implementation.subject) == Some(receiver_head)
        })
        .collect();
    // The receiver's own impls: a declared member wins outright.
    let declared: Vec<Id> = matching
        .iter()
        .filter_map(|implementation| implementation.declarations.get(member).copied())
        .collect();
    if !declared.is_empty() {
        return Some(declared);
    }
    // Else the trait defaults those impls inherit.
    let defaults = inherited_defaults(program, &matching, member);
    if !defaults.is_empty() {
        return Some(defaults);
    }
    // M133: no impl of the receiver's own head names the member, so what it
    // reaches comes through a BLANKET impl — `impl type S: Flow<str> with
    // Slot` for a `SignalCell<str>` receiver. Only the blankets whose subject
    // APPLIES (the shape matches and every binder's bounds hold) can be
    // selected at monomorphization, which is the same proof impl selection
    // makes there; before this, a receiver that implements `Slot` through a
    // blanket walked every `Slot` impl in std. The proof needs the receiver
    // whole: a type still carrying a generic cannot be proved against a bound,
    // and is left to the whole list.
    if !type_is_ground(program, receiver, 0) {
        return None;
    }
    let blankets: Vec<&crate::analyzer::Implementation> = program
        .implementations
        .iter()
        .filter(|implementation| {
            subject_head(program, implementation.subject).is_none()
                && (implementation.declarations.contains_key(member)
                    || implementation.trait_ids.iter().any(|trait_id| {
                        program
                            .traits
                            .get(trait_id)
                            .is_some_and(|trait_| trait_.declarations.contains_key(member))
                    }))
                && crate::impl_select::subject_applies(program, implementation.subject, receiver)
        })
        .collect();
    let mut selected: Vec<Id> = Vec::new();
    for implementation in &blankets {
        match implementation.declarations.get(member) {
            Some(declared) => selected.push(*declared),
            None => selected.extend(inherited_defaults(program, &[implementation], member)),
        }
    }
    selected.dedup();
    // A ground receiver that applies to no blanket either is a shape this
    // narrowing does not know: stay sound.
    (!selected.is_empty()).then_some(selected)
}

/// The trait defaults for `member` that `implementations` inherit.
fn inherited_defaults(
    program: &Program,
    implementations: &[&crate::analyzer::Implementation],
    member: &str,
) -> Vec<Id> {
    implementations
        .iter()
        .flat_map(|implementation| implementation.trait_ids.iter())
        .filter_map(|trait_id| {
            program
                .traits
                .get(trait_id)
                .and_then(|trait_| trait_.declarations.get(member).copied())
        })
        .collect()
}

/// Whether `type_id` names a type with no generic left in it — the only kind
/// a blanket impl's bounds can be proved against outside an instantiation.
fn type_is_ground(program: &Program, type_id: TypeId, depth: u32) -> bool {
    if depth > 16 {
        return false;
    }
    let all = |ids: &[TypeId]| ids.iter().all(|id| type_is_ground(program, *id, depth + 1));
    match program.type_id_to_type_map.get(&type_id) {
        Some(
            Type::Struct(_, arguments)
            | Type::Enum(_, arguments)
            | Type::Trait(_, arguments)
            | Type::Dyn(_, arguments),
        ) => all(arguments),
        Some(Type::Tuple(elements, _)) => all(elements),
        Some(Type::Array(element, length)) => all(&[*element, *length]),
        Some(Type::Closure(parameters, return_type, _, _)) => {
            all(parameters) && type_is_ground(program, *return_type, depth + 1)
        }
        Some(Type::Length(_) | Type::Function(_) | Type::Module(_) | Type::Never) => true,
        _ => false,
    }
}

/// The `GenericDispatch` recorded for a call site — keyed by the call id (an
/// `OnType` re-dispatch) or by its subject (an `OnConstraint` bounded call),
/// mirroring how the call graph and transformer read it.
pub(crate) fn dispatch_at<'src>(
    program: &Program<'src>,
    call_id: Id,
) -> Option<GenericDispatch<'src>> {
    if let Some(dispatch) = program.generic_dispatch.get(&call_id) {
        return Some(*dispatch);
    }
    let subject_id = program.function_calls.get(&call_id)?.subject_id;
    program.generic_dispatch.get(&subject_id).copied()
}

/// The trait a single bound's constraint id denotes, if it resolves to one.
fn trait_of(program: &Program, constraint_id: TypeId) -> Option<Id> {
    match program.type_id_to_type_map.get(&constraint_id) {
        Some(Type::Trait(trait_id, _)) => Some(*trait_id),
        _ => None,
    }
}

/// Every member named `member` an impl of `trait_id` provides, plus the trait's
/// own default for it — the candidates a dispatch bounded by that trait selects
/// among at monomorphization.
/// The traits in `trait_id`'s supertrait chain (itself first) that DECLARE
/// `member` — the traits whose impls a bound-directed call of `member` can
/// reach. Empty when none does, which leaves the caller its fallback.
fn declaring_traits_in_chain(program: &Program, trait_id: Id, member: &str) -> Vec<Id> {
    let mut pending = vec![trait_id];
    let mut seen: Vec<Id> = Vec::new();
    let mut declaring = Vec::new();
    while let Some(id) = pending.pop() {
        if seen.contains(&id) {
            continue;
        }
        seen.push(id);
        let Some(trait_) = program.traits.get(&id) else {
            continue;
        };
        if trait_.declarations.contains_key(member) {
            declaring.push(id);
            continue;
        }
        for supertrait in &trait_.supertraits {
            if let Some(Type::Trait(super_id, _)) = program.type_id_to_type_map.get(supertrait) {
                pending.push(*super_id);
            }
        }
    }
    declaring
}

fn trait_method_candidates(
    program: &Program,
    trait_id: Id,
    member: &str,
) -> std::sync::Arc<Vec<Id>> {
    let key = (trait_id, member.to_string());
    if let Some(cached) = program.trait_method_memo().get(&key) {
        return cached.clone();
    }
    let computed = std::sync::Arc::new(compute_trait_method_candidates(program, trait_id, member));
    program.trait_method_memo().insert(key, computed.clone());
    computed
}

fn compute_trait_method_candidates(program: &Program, trait_id: Id, member: &str) -> Vec<Id> {
    let mut candidates = Vec::new();
    if let Some(trait_) = program.traits.get(&trait_id)
        && let Some(default_id) = trait_.declarations.get(member)
    {
        candidates.push(*default_id);
    }
    for implementation in &program.implementations {
        if implementation.trait_ids.contains(&trait_id)
            && let Some(member_id) = implementation.declarations.get(member)
        {
            candidates.push(*member_id);
        }
    }
    candidates
}

/// Whether `member_id` is callable AS A METHOD — its first parameter is named
/// `self`. `Analyzer::is_self_method`'s post-analysis twin, over a `Program`.
///
/// A dispatched `receiver.name()` can only ever select a member with a
/// receiver, so a same-named STATIC is not a candidate for it however sound the
/// rest of the over-approximation is. Leaving statics in is not merely
/// imprecise, it is wrong in a way users feel: `std::js::promise::Promise::all` is
/// an `async external` static, `promise` is a force-loaded core module, and so
/// every `xs.iter().all(p)` — an `OnType` re-dispatch, which cannot pin its
/// trait and falls back to this scan — used to color its whole caller async,
/// down to an `async` `main`. Nothing there is reachable from the call.
fn is_self_method(program: &Program, member_id: Id) -> bool {
    let first_parameter_id = match program.entity_map.get(&member_id) {
        Some(crate::analyzer::Expr::Function(function_id)) => program
            .functions
            .get(function_id)
            .and_then(|function| function.parameters.first().copied()),
        Some(crate::analyzer::Expr::ExternalFunction(external_function_id)) => program
            .external_functions
            .get(external_function_id)
            .and_then(|external| external.parameters.first().copied()),
        _ => None,
    };
    first_parameter_id
        .and_then(|parameter_id| program.parameters.get(&parameter_id))
        .map(|parameter| parameter.name == "self")
        .unwrap_or(false)
}

/// Every impl member and trait default with the given name — the fallback when a
/// dispatch's trait can't be pinned (a multi-bound parameter, or an `OnType`
/// re-dispatch). Over-approximate but sound: it can only add async-ness. Members
/// with no receiver are excluded — see [`is_self_method`].
fn members_named(program: &Program, member: &str) -> Vec<Id> {
    let mut candidates = Vec::new();
    for implementation in &program.implementations {
        if let Some(member_id) = implementation.declarations.get(member) {
            candidates.push(*member_id);
        }
    }
    for trait_ in program.traits.values() {
        if let Some(member_id) = trait_.declarations.get(member) {
            candidates.push(*member_id);
        }
    }
    candidates.retain(|member_id| is_self_method(program, *member_id));
    candidates
}

// ---------------------------------------------------------------------------
// Adaptation (async-polymorphism.md A.1): the instance worklist.
// ---------------------------------------------------------------------------

/// The worklist's output: the per-instance emission decisions and the
/// diagnostics it found (transitive `sync` violations, dispatch refusals).
struct Adaptation {
    instances: HashMap<(Id, Vec<Id>), crate::analyzer::AdaptedInstance>,
    /// Each diagnostic with the file its span indexes into (backlog E16) —
    /// this pass runs over the whole program, so the file comes from the
    /// diagnostic's anchor entity, not from "the file being walked".
    diagnostics: Vec<(crate::error::Error, SourceId)>,
}

/// One instance's identity: the function and WHICH of its closure parameters
/// are async, sorted for a stable key.
type InstanceKey = (Id, Vec<Id>);

/// Record the call that instantiates an adapted instance, keeping the LEAST id.
///
/// The origin is the span every transitive violation found inside the instance
/// anchors at (`origin.unwrap_or(call_id)` in `sync_violations_at` and its
/// siblings), so it is an ANSWER, not bookkeeping. The worklist that discovers
/// instances is seeded from `program.functions.keys()` — hash order — so
/// "whichever call discovered it first" was a coin flip; ids are minted in walk
/// order, so the least one is the earliest instantiating call in the program.
fn record_origin(origins: &mut HashMap<InstanceKey, Id>, key: &InstanceKey, call_id: Id) {
    match origins.get_mut(key) {
        Some(existing) if existing.0 <= call_id.0 => {}
        Some(existing) => *existing = call_id,
        None => {
            origins.insert(key.clone(), call_id);
        }
    }
}

/// The component of a root function: itself plus its transitively nested
/// closures — they share the instance's bits context.
fn component_of(graph: &CallGraph, root: Id) -> Vec<Id> {
    let mut members = vec![root];
    let mut cursor = 0;
    while cursor < members.len() {
        if let Some(children) = graph.closure_children_of(members[cursor]) {
            members.extend(children.iter().copied());
        }
        cursor += 1;
    }
    members
}

thread_local! {
    /// M138's instrument: the passes (one per analysis, a macro world's
    /// included), the worklist ROUNDS and the DECISIONS passes this thread ran
    /// since the last [`reset_adaptation_counts`]. The answer is the
    /// same either way, so only the counts can see that the decisions run once.
    static ADAPTATION_INFERS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static ADAPTATION_ROUNDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static ADAPTATION_DECISIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// `(passes, worklist rounds, decisions passes)` on this thread since the
/// last [`reset_adaptation_counts`].
pub fn adaptation_counts() -> (usize, usize, usize) {
    (
        ADAPTATION_INFERS.with(std::cell::Cell::get),
        ADAPTATION_ROUNDS.with(std::cell::Cell::get),
        ADAPTATION_DECISIONS.with(std::cell::Cell::get),
    )
}

/// Zeroes this thread's [`adaptation_counts`].
pub fn reset_adaptation_counts() {
    ADAPTATION_INFERS.with(|count| count.set(0));
    ADAPTATION_ROUNDS.with(|count| count.set(0));
    ADAPTATION_DECISIONS.with(|count| count.set(0));
}

/// What every step of the adaptation reads and never writes.
#[derive(Clone, Copy)]
struct Adapt<'p, 'src> {
    program: &'p Program<'src>,
    graph: &'p CallGraph,
    held_values: &'p HashMap<Id, Vec<Id>>,
    adaptive: &'p HashMap<Id, HashSet<Id>>,
    /// A.4's escape positions, collected once per analysis — shared by the
    /// per-instance escape check and the global divergence checks.
    escapes: &'p Escapes,
}

/// The plain closure-typed positions an async value would lie through: the
/// stores into plain closure fields and the returns from plain closure-typed
/// functions (A.4).
struct Escapes {
    field_stores: Vec<FieldStore>,
    return_sites: Vec<(Id, Id)>,
}

impl Escapes {
    /// The plain positions among every field store (`stores`, gathered by the
    /// pass's one walk of the entity map) and every return site.
    fn of(program: &Program, stores: Vec<FieldStore>) -> Escapes {
        Escapes {
            field_stores: plain_closure_field_stores(program, stores),
            return_sites: plain_closure_return_sites(program),
        }
    }
}

/// One evaluation of an instance against the current verdicts: every member
/// of its component flagged until stable, each callee instance a call names
/// handed to `encounter` (which answers that instance's flag, discovering it
/// when it is new). Answers the member flags; the instance awaits when its
/// root's flag holds.
fn evaluate_instance(
    adapt: Adapt,
    view: View,
    key: &InstanceKey,
    encounter: &mut dyn FnMut(InstanceKey, Id) -> bool,
) -> HashMap<Id, bool> {
    let program = adapt.program;
    let (root, ref bits) = *key;
    let members = component_of(adapt.graph, root);
    let mut flags: HashMap<Id, bool> = members
        .iter()
        .map(|member| (*member, view.contains(program, *member)))
        .collect();
    loop {
        let mut changed = false;
        for member in &members {
            for call in adapt.graph.calls_of(*member) {
                let awaits = match call.target {
                    CallTarget::Function(callee) => {
                        let callee_bits = bits_for_call(
                            program,
                            adapt.held_values,
                            view,
                            adapt.adaptive,
                            &flags,
                            bits,
                            callee,
                            call.call_id,
                        );
                        if callee_bits.is_empty() {
                            view.contains(program, callee)
                        } else {
                            encounter((callee, callee_bits), call.call_id)
                        }
                    }
                    CallTarget::External(callee) => view.contains(program, callee),
                    CallTarget::Indirect(
                        IndirectReason::GenericMember | IndirectReason::TraitDispatch,
                    ) => view.dispatch_async(program, call.call_id),
                    _ => {
                        subject_adapted_here(program, adapt.held_values, &flags, bits, call.call_id)
                            || program.function_calls.get(&call.call_id).is_some_and(
                                |function_call| {
                                    subject_awaits(
                                        program,
                                        function_call.subject_id,
                                        adapt.held_values,
                                        view,
                                        0,
                                    )
                                },
                            )
                    }
                };
                if awaits && !flags[member] {
                    flags.insert(*member, true);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    flags
}

/// One round of the instance worklist's answer: every instance key the round
/// evaluated or discovered with its async flag, and the call each was first
/// instantiated by. Over a world's record (M138) the recorded cold keys the
/// round never re-walked are NOT here: [`Seeding`] answers them.
struct Worklist {
    instance_async: HashMap<InstanceKey, bool>,
    origins: HashMap<InstanceKey, Id>,
}

/// One round of the instance worklist (async-polymorphism.md A.1): every
/// function as its base instance, every instance a call discovers, until each
/// flag is stable over `async_set` — which a base instance's flips extend.
/// The caller re-runs a round from scratch while a round grows the set.
///
/// M138: over a world's cold instance record (`seeding`), the round starts
/// from the recorded cold verdicts — the hot functions' base instances are
/// what it evaluates, and a recorded key is re-walked only when something it
/// READ moved (an id it read as not async became async, a binding it read
/// gained a value from outside the cold set, a dispatch it read gained an
/// async candidate) or a callee instance it read flipped. Without one it is
/// the whole-program round it always was.
fn adaptation_worklist(
    adapt: Adapt,
    async_set: &mut HashSet<Id>,
    mut seeding: Option<&mut Seeding>,
) -> Worklist {
    ADAPTATION_ROUNDS.with(|count| count.set(count.get() + 1));
    let program = adapt.program;
    let mut instance_async: HashMap<InstanceKey, bool> = HashMap::default();
    let mut origins: HashMap<InstanceKey, Id> = HashMap::default();
    let mut dependents: HashMap<InstanceKey, HashSet<InstanceKey>> = HashMap::default();
    let mut pending: Vec<InstanceKey> = Vec::new();
    let mut queued: HashSet<InstanceKey> = HashSet::default();
    fn push(pending: &mut Vec<InstanceKey>, queued: &mut HashSet<InstanceKey>, key: InstanceKey) {
        if queued.insert(key.clone()) {
            pending.push(key);
        }
    }
    match seeding.as_deref_mut() {
        None => {
            for function_id in program.functions.keys() {
                push(&mut pending, &mut queued, (*function_id, Vec::new()));
            }
        }
        Some(seeding) => {
            for function_id in program.functions.keys() {
                if !program.cold_entity(*function_id) {
                    push(&mut pending, &mut queued, (*function_id, Vec::new()));
                }
            }
            seeding.round_start(adapt, async_set);
            for key in seeding.rewalked_keys() {
                instance_async.insert(key.clone(), seeding.recorded_flag_of(&key));
                push(&mut pending, &mut queued, key);
            }
        }
    }

    // Module initializers can also instantiate adapted callees (a top-level
    // `let ids = xs.map(async)`); discover those instances up front so the
    // module-initializer check can refuse the await (an initializer cannot).
    // Asked live on every round, records or not: the reads are no key's.
    for binding in program.module_level_bindings() {
        for call in adapt.graph.initializer_calls_of(binding) {
            if let CallTarget::Function(callee) = call.target {
                let bits = bits_for_call(
                    program,
                    adapt.held_values,
                    View::plain(async_set),
                    adapt.adaptive,
                    &HashMap::default(),
                    &[],
                    callee,
                    call.call_id,
                );
                if !bits.is_empty() {
                    let key = (callee, bits);
                    record_origin(&mut origins, &key, call.call_id);
                    let known = instance_async.contains_key(&key)
                        || seeding
                            .as_deref()
                            .is_some_and(|seeding| seeding.serves(&key));
                    if !known {
                        instance_async.insert(key.clone(), false);
                        push(&mut pending, &mut queued, key);
                    }
                }
            }
        }
    }

    // --- The worklist: process instances until every async flag is stable.
    // Flags only move false -> true, so this terminates. Every call of every
    // member is enumerated on every pass — even members already known async
    // — because enumeration is also DISCOVERY: a call with async closure
    // arguments mints the callee's adapted instance.
    while let Some(key) = pending.pop() {
        queued.remove(&key);
        let flags = {
            let seeding_view = seeding.as_deref();
            let mut encounter = |callee_key: InstanceKey, call_id: Id| -> bool {
                dependents
                    .entry(callee_key.clone())
                    .or_default()
                    .insert(key.clone());
                // Every encounter, not just the discovering one: the worklist
                // is seeded from a hash-ordered key set, so "the call that
                // discovered it first" is not a stable answer, and this id
                // becomes the SPAN a transitive violation reports at.
                record_origin(&mut origins, &callee_key, call_id);
                if let Some(flag) = instance_async.get(&callee_key) {
                    return *flag;
                }
                if let Some(flag) =
                    seeding_view.and_then(|seeding| seeding.served_flag(&callee_key))
                {
                    return flag;
                }
                // Discover the callee instance; re-run this one when its flag
                // lands.
                instance_async.insert(callee_key.clone(), false);
                if queued.insert(callee_key.clone()) {
                    pending.push(callee_key);
                }
                false
            };
            evaluate_instance(adapt, View::plain(async_set), &key, &mut encounter)
        };
        let root_flag = flags[&key.0];
        let previous = instance_async.insert(key.clone(), root_flag).or_else(|| {
            seeding
                .as_deref()
                .and_then(|seeding| seeding.served_flag(&key))
        });
        if root_flag && previous != Some(true) {
            // The flag flipped on: dependents must re-evaluate — the round's
            // own, and the recorded keys that read this instance.
            for dependent in dependents.get(&key).cloned().unwrap_or_default() {
                push(&mut pending, &mut queued, dependent);
            }
            if let Some(seeding) = seeding.as_deref_mut() {
                for dependent in seeding.recorded_dependents(&key) {
                    if let Some(rewalked) = seeding.rewalk(dependent) {
                        instance_async
                            .entry(rewalked.clone())
                            .or_insert_with(|| seeding.recorded_flag_of(&rewalked));
                        push(&mut pending, &mut queued, rewalked);
                    }
                }
            }
        }
        // A base instance's flips are ordinary coloring: the plain emission
        // of this function/those closures is async.
        if key.1.is_empty() {
            for (member, flag) in &flags {
                if *flag && async_set.insert(*member) {
                    // M138: a member the record's cold world did not make
                    // async re-walks the recorded keys that read it.
                    if let Some(seeding) = seeding.as_deref_mut() {
                        for rewalked in seeding.moved(program, *member) {
                            instance_async
                                .entry(rewalked.clone())
                                .or_insert_with(|| seeding.recorded_flag_of(&rewalked));
                            push(&mut pending, &mut queued, rewalked);
                        }
                    }
                }
            }
        }
    }

    Worklist {
        instance_async,
        origins,
    }
}

/// One instance's emission data and the diagnostics it would report, before
/// the first-wins dedup (`reported`): the unit the decisions pass computes per
/// key, and what M138's record keeps per cold key.
#[derive(Clone, Debug, Default)]
struct KeyDecision {
    info: crate::analyzer::AdaptedInstance,
    /// Whether the instance earns an `adapted_instances` entry: an adapted
    /// instance always (the caller emits it by key), a base one only when it
    /// carries emission data.
    kept: bool,
    reports: Vec<Report>,
}

impl KeyDecision {
    fn has_output(&self) -> bool {
        self.kept || !self.reports.is_empty()
    }

    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + (self.info.awaited_calls.len() + self.info.async_closures.len())
                * std::mem::size_of::<Id>()
            + self
                .info
                .callee_bits
                .values()
                .map(|bits| std::mem::size_of::<(Id, Vec<Id>)>() + bits.len() * 4)
                .sum::<usize>()
            + self.reports.len() * std::mem::size_of::<Report>()
    }
}

/// A diagnostic one instance would report, as the dedup keys it — the
/// message is spelled at emission, at the instance's CURRENT origin.
#[derive(Clone, Copy, Debug)]
enum Report {
    /// A transitive `sync` violation: an argument async only through the
    /// instance's bits, into a `sync` parameter.
    Sync { call_id: Id, parameter: Id },
    /// The host-boundary twin: into an `external` function's value-returning
    /// closure parameter.
    Extern {
        call_id: Id,
        parameter: Id,
        callee: Id,
    },
    /// An async closure argument at a trait/generic dispatch whose candidates
    /// would need adaptation.
    Dispatch { call_id: Id, position: usize },
    /// A.4's adapted-instance escape into a plain closure field.
    FieldEscape(FieldStore),
    /// A.4's adapted-instance escape through a plain closure-typed return.
    ReturnEscape { function_id: Id, value_id: Id },
}

/// The decisions for ONE instance over the round's final verdicts (every flag
/// stable, the async set final): its emission data and the diagnostics it
/// would report. `flag_of` answers a callee instance's verdict; every one it
/// is asked is handed to `callee_read` too (the cold record's dependents).
fn decide_instance(
    adapt: Adapt,
    view: View,
    key: &InstanceKey,
    flag_of: &dyn Fn(&InstanceKey) -> Option<bool>,
    callee_read: &mut dyn FnMut(&InstanceKey),
) -> KeyDecision {
    let program = adapt.program;
    let (root, ref bits) = *key;
    let members = component_of(adapt.graph, root);
    // Recompute the stable flags for this context (cheap; flags are final so
    // one pass over the loop from the worklist would converge identically —
    // reuse the same evaluation by seeding from the global set and iterating).
    let mut flags: HashMap<Id, bool> = members
        .iter()
        .map(|member| (*member, view.contains(program, *member)))
        .collect();
    let mut callee_flag = |callee_key: &InstanceKey| {
        callee_read(callee_key);
        flag_of(callee_key) == Some(true)
    };
    // A BASE instance's flags are the async set's already: the last round
    // re-evaluated it against every final callee verdict (a flip re-queues its
    // dependents) and wrote each member it flipped into the set — and that
    // round wrote nothing new, or it would not have been the last. So the loop
    // below would find no member to flip, and only an adapted instance (whose
    // bits make its members async where the set does not) runs it.
    loop {
        if bits.is_empty() {
            break;
        }
        let mut changed = false;
        for member in &members {
            if flags[member] {
                continue;
            }
            let awaits = adapt
                .graph
                .calls_of(*member)
                .iter()
                .any(|call| match call.target {
                    CallTarget::Function(callee) => {
                        view.contains(program, callee) || {
                            let callee_bits = bits_for_call(
                                program,
                                adapt.held_values,
                                view,
                                adapt.adaptive,
                                &flags,
                                bits,
                                callee,
                                call.call_id,
                            );
                            !callee_bits.is_empty() && callee_flag(&(callee, callee_bits))
                        }
                    }
                    CallTarget::External(callee) => view.contains(program, callee),
                    CallTarget::Indirect(
                        IndirectReason::GenericMember | IndirectReason::TraitDispatch,
                    ) => view.dispatch_async(program, call.call_id),
                    _ => {
                        subject_adapted_here(program, adapt.held_values, &flags, bits, call.call_id)
                            || program.function_calls.get(&call.call_id).is_some_and(
                                |function_call| {
                                    subject_awaits(
                                        program,
                                        function_call.subject_id,
                                        adapt.held_values,
                                        view,
                                        0,
                                    )
                                },
                            )
                    }
                });
            if awaits {
                flags.insert(*member, true);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut decision = KeyDecision {
        info: crate::analyzer::AdaptedInstance {
            is_async: flags[&root],
            ..Default::default()
        },
        kept: false,
        reports: Vec::new(),
    };
    for member in &members {
        if *flags.get(member).unwrap_or(&false) && !bits.is_empty() && *member != root {
            decision.info.async_closures.insert(*member);
        }
        for call in adapt.graph.calls_of(*member) {
            match call.target {
                CallTarget::Function(callee) => {
                    let callee_bits = bits_for_call(
                        program,
                        adapt.held_values,
                        view,
                        adapt.adaptive,
                        &flags,
                        bits,
                        callee,
                        call.call_id,
                    );
                    if !callee_bits.is_empty() {
                        if callee_flag(&(callee, callee_bits.clone())) {
                            decision.info.awaited_calls.insert(call.call_id);
                        }
                        decision.info.callee_bits.insert(call.call_id, callee_bits);
                    }
                    // Transitive `sync`/host violations: an argument that is
                    // async ONLY through this instance's bits, flowing into a
                    // refusing position.
                    sync_violations_at(
                        program,
                        adapt.held_values,
                        view,
                        &flags,
                        bits,
                        callee,
                        call.call_id,
                        &mut decision.reports,
                    );
                }
                CallTarget::External(callee) => {
                    extern_violations_at(
                        program,
                        adapt.held_values,
                        view,
                        &flags,
                        bits,
                        callee,
                        call.call_id,
                        &mut decision.reports,
                    );
                }
                CallTarget::Indirect(
                    IndirectReason::GenericMember | IndirectReason::TraitDispatch,
                ) => {
                    dispatch_refusals_at(
                        program,
                        adapt.held_values,
                        view,
                        &flags,
                        bits,
                        call.call_id,
                        &mut decision.reports,
                    );
                }
                _ => {}
            }
            if subject_adapted_here(program, adapt.held_values, &flags, bits, call.call_id) {
                decision.info.awaited_calls.insert(call.call_id);
            }
        }
    }
    // A.4's adapted-instance escape: a value async only through THESE bits
    // leaving the instance through a plain field or a plain declared return.
    // A base instance carries no bits and so can escape nothing the global
    // checks did not already see.
    if !bits.is_empty() {
        escape_violations_in(
            program,
            adapt.held_values,
            view,
            &flags,
            bits,
            adapt.escapes,
            &mut decision.reports,
        );
    }
    // Base instances only earn an entry when they carry emission data;
    // adapted instances always do (the caller emits them by key).
    decision.kept = !bits.is_empty()
        || !decision.info.awaited_calls.is_empty()
        || !decision.info.callee_bits.is_empty()
        || !decision.info.async_closures.is_empty();
    decision
}

/// The worklist's decisions, over its LAST round (every flag stable, the async
/// set final): each instance's emission data and the context-dependent
/// diagnostics. Reads the set; moves nothing. Over a world's record (M138) a
/// recorded cold key the rounds never re-walked replays its recorded decision.
fn adaptation_decisions(
    adapt: Adapt,
    async_set: &HashSet<Id>,
    worklist: Worklist,
    seeding: Option<&Seeding>,
) -> Adaptation {
    let Worklist {
        instance_async,
        origins,
    } = worklist;
    ADAPTATION_DECISIONS.with(|count| count.set(count.get() + 1));
    let program = adapt.program;
    let flag_of = |callee_key: &InstanceKey| {
        instance_async
            .get(callee_key)
            .copied()
            .or_else(|| seeding.and_then(|seeding| seeding.served_flag(callee_key)))
    };
    // Every key with output: the live keys' decisions, then the recorded
    // decisions the rounds left standing.
    let mut decided: Vec<(InstanceKey, std::borrow::Cow<KeyDecision>)> = Vec::new();
    for key in instance_async.keys() {
        let decision = decide_instance(adapt, View::plain(async_set), key, &flag_of, &mut |_| {});
        if decision.has_output() {
            decided.push((key.clone(), std::borrow::Cow::Owned(decision)));
        }
    }
    if let Some(seeding) = seeding {
        for (key, decision) in seeding.served_decisions() {
            decided.push((key.clone(), std::borrow::Cow::Borrowed(decision)));
        }
    }
    let origin_of = |key: &InstanceKey| {
        let live = origins.get(key).copied();
        let recorded = seeding.and_then(|seeding| seeding.recorded_origin(key));
        match (live, recorded) {
            (Some(live), Some(recorded)) => {
                Some(if live.0 <= recorded.0 { live } else { recorded })
            }
            (live, recorded) => live.or(recorded),
        }
    };
    // C1: this pass is FIRST-WINS across instances — `reported` and
    // `reported_escapes` let one instance claim a violation and silence the
    // others — and each instance anchors its diagnostic at its OWN origin. So
    // a hash-ordered walk chose WHICH call site a transitive `sync` / host /
    // escape violation named, and whether a note attached at all. Walk the
    // instances in origin order instead: the earliest-minted instantiating
    // call wins, base instances (no origin, and no diagnostics of their own)
    // first. Only an instance that reports can be ordered against another.
    let mut reporting: Vec<(Option<u32>, u32, Vec<u32>, usize)> = decided
        .iter()
        .enumerate()
        .filter(|(_, (_, decision))| !decision.reports.is_empty())
        .map(|(position, (key, _))| {
            (
                origin_of(key).map(|origin| origin.0),
                key.0.0,
                key.1.iter().map(|bit| bit.0).collect::<Vec<u32>>(),
                position,
            )
        })
        .collect();
    reporting.sort();
    let mut diagnostics: Vec<(crate::error::Error, SourceId)> = Vec::new();
    let mut reported: HashSet<(Id, Id)> = HashSet::default();
    // Deduplicated by value expression, so one escaping store reports once
    // however many instances reach it.
    let mut reported_escapes: HashSet<Id> = HashSet::default();
    for (_, _, _, position) in reporting {
        let (key, decision) = &decided[position];
        let origin = origin_of(key);
        for report in &decision.reports {
            emit_report(
                program,
                *report,
                origin,
                &mut reported,
                &mut reported_escapes,
                &mut diagnostics,
            );
        }
    }
    let instances = decided
        .into_iter()
        .filter(|(_, decision)| decision.kept)
        .map(|(key, decision)| (key, decision.into_owned().info))
        .collect();
    Adaptation {
        instances,
        diagnostics,
    }
}

/// Speaks one instance's report, unless an earlier instance claimed it, at
/// the instance's origin (the call that instantiated it) with a note at the
/// violating call when the two differ.
fn emit_report(
    program: &Program,
    report: Report,
    origin: Option<Id>,
    reported: &mut HashSet<(Id, Id)>,
    reported_escapes: &mut HashSet<Id>,
    diagnostics: &mut Vec<(crate::error::Error, SourceId)>,
) {
    match report {
        Report::Sync { call_id, parameter } => {
            if !reported.insert((call_id, parameter)) {
                return;
            }
            let parameter_name = program
                .parameters
                .get(&parameter)
                .map(|parameter| parameter.name)
                .unwrap_or("the parameter");
            let primary = origin.unwrap_or(call_id);
            let note = (primary != call_id).then(|| crate::error::Note {
                span: span_of(program, call_id),
                msg: format!("forwarded into the `sync` parameter `{parameter_name}` here"),
                source: program.note_source_of(call_id),
            });
            diagnostics.push(anchored(
                program,
                primary,
                format!(
                    "this call passes an async closure that reaches `{parameter_name}`, which requires a synchronous closure (`sync`); move the async work outside the callback (e.g. a `turn` with an awaiting body, `Draft`, or a spawned `async` block)"
                ),
                note,
            ));
        }
        Report::Extern {
            call_id,
            parameter,
            callee,
        } => {
            if !reported.insert((call_id, parameter)) {
                return;
            }
            let external_name = program
                .external_functions
                .get(&callee)
                .map(|external| external.name)
                .unwrap_or("the host function");
            let primary = origin.unwrap_or(call_id);
            let note = (primary != call_id).then(|| crate::error::Note {
                span: span_of(program, call_id),
                msg: format!("forwarded to the host function `{external_name}` here"),
                source: program.note_source_of(call_id),
            });
            diagnostics.push(anchored(
                program,
                primary,
                format!(
                    "this call passes an async closure that reaches the host (`external`) function `{external_name}`, which cannot await a Vilan closure: only synchronous closures can cross"
                ),
                note,
            ));
        }
        Report::Dispatch { call_id, position } => {
            if !reported.insert((call_id, Id(position as u32))) {
                return;
            }
            diagnostics.push(anchored(
                program,
                call_id,
                "an async closure cannot adapt a trait/generic-dispatched call (the concrete callee varies per instantiation); bind the callee concretely, or declare the trait parameter `async || T`".to_string(),
                None,
            ));
        }
        Report::FieldEscape(store) => {
            if !reported_escapes.insert(store.value_id) {
                return;
            }
            let field_name = field_of(program, &store)
                .map(|field| field.name)
                .unwrap_or("the field");
            let struct_name = program
                .structs
                .get(&store.struct_id)
                .map(|struct_| struct_.name)
                .unwrap_or("the struct");
            escape_report(
                program,
                origin,
                store.value_id,
                format!(
                    "this call passes an async closure that reaches `{struct_name}`'s field `{field_name}`, which awaits nothing — a later call through the field would hand back a promise; declare the field `async || T` (or return void for spawn semantics)"
                ),
                format!("stored into the plain field `{field_name}` here"),
                diagnostics,
            );
        }
        Report::ReturnEscape {
            function_id,
            value_id,
        } => {
            if !reported_escapes.insert(value_id) {
                return;
            }
            let name = program
                .functions
                .get(&function_id)
                .map(|function| function.name)
                .unwrap_or("this function");
            escape_report(
                program,
                origin,
                value_id,
                format!(
                    "this call passes an async closure that reaches `{name}`'s declared return type, which awaits nothing — the caller would hold a promise typed as a plain closure; declare it `async || T` (or return void for spawn semantics)"
                ),
                format!("returned from `{name}` here"),
                diagnostics,
            );
        }
    }
}

/// An escape's diagnostic: at the instance's origin, with a note at the
/// escaping value when the two differ.
fn escape_report(
    program: &Program,
    origin: Option<Id>,
    value_id: Id,
    message: String,
    note_msg: String,
    diagnostics: &mut Vec<(crate::error::Error, SourceId)>,
) {
    let primary = origin.unwrap_or(value_id);
    let note = (primary != value_id).then(|| crate::error::Note {
        span: span_of(program, value_id),
        msg: note_msg,
        source: program.note_source_of(value_id),
    });
    diagnostics.push(anchored(program, primary, message, note));
}

// ---------------------------------------------------------------------------
// M138 (Order 54): the seeded form — the adaptation over a world's COLD
// instance record.
// ---------------------------------------------------------------------------

/// The async set as the adaptation reads it: the plain set, or the COLD view
/// the world's record is computed in — the cold-only set, every value a
/// non-cold module holds in a cold binding skipped — with, in the record's
/// own run, every read a key's verdict depends on LOGGED.
#[derive(Clone, Copy)]
struct View<'a> {
    set: &'a HashSet<Id>,
    /// The cold view: a held value outside the cold set is not the cold
    /// world's (`admits_held`). A non-cold id is never in the cold set, so a
    /// read of one answers "not async" by itself.
    cold: bool,
    /// The record's run: the reads of the key being evaluated.
    reads: Option<&'a std::cell::RefCell<KeyReads>>,
}

impl<'a> View<'a> {
    fn plain(set: &'a HashSet<Id>) -> View<'a> {
        View {
            set,
            cold: false,
            reads: None,
        }
    }

    /// Whether `id` is async. Logged when it answers "no" — an answer of
    /// "yes" cannot move (a later analysis's set only holds more) — by id when
    /// the id is cold, and as a read the record cannot validate when it is not
    /// (no cold key names a non-cold entity directly; one that ever does is
    /// re-walked on every analysis).
    fn contains(&self, program: &Program, id: Id) -> bool {
        let async_ = self.set.contains(&id);
        if !async_ && let Some(reads) = self.reads {
            let mut reads = reads.borrow_mut();
            if program.cold_entity(id) {
                reads.ids.push(id);
            } else {
                reads.unrecordable = true;
            }
        }
        async_
    }

    /// Whether any candidate of the trait/generic dispatch at `call_id` is
    /// async — the fixpoint's over-approximation. Logged by CALL when it
    /// answers "no": a later analysis's hot impls can widen the candidates as
    /// well as make one async, so the record asks the dispatch again.
    fn dispatch_async(&self, program: &Program, call_id: Id) -> bool {
        let async_ = dispatch_candidates(program, call_id)
            .iter()
            .any(|candidate| self.set.contains(candidate));
        if !async_ && let Some(reads) = self.reads {
            reads.borrow_mut().dispatches.push(call_id);
        }
        async_
    }

    /// Logs a read of `binding`'s held values: a later analysis's hot module
    /// may assign it a value the cold world never held.
    fn note_held(&self, binding: Id) {
        if let Some(reads) = self.reads {
            reads.borrow_mut().bindings.push(binding);
        }
    }

    /// Whether a held value counts: in the cold view, only a cold one.
    fn admits_held(&self, program: &Program, value_id: Id) -> bool {
        !self.cold || program.cold_entity(value_id)
    }
}

/// What one key's evaluations and decision READ, in the record's run.
#[derive(Default)]
struct KeyReads {
    /// Cold ids read as not async.
    ids: Vec<Id>,
    /// Bindings whose held values were read.
    bindings: Vec<Id>,
    /// Dispatch calls whose candidates read as not async.
    dispatches: Vec<Id>,
    /// A non-cold id was read directly: the verdict is re-walked always.
    unrecordable: bool,
}

impl KeyReads {
    fn absorb(&mut self, other: KeyReads) {
        self.ids.extend(other.ids);
        self.bindings.extend(other.bindings);
        self.dispatches.extend(other.dispatches);
        self.unrecordable |= other.unrecordable;
    }
}

/// One cold key of the record.
#[derive(Debug)]
struct RecordedKey {
    key: InstanceKey,
    /// Its verdict over the cold nodes alone — a lower bound of every later
    /// analysis's, since the cold view reads nothing the hot set decides.
    flag: bool,
    /// The least cold call that instantiated it.
    origin: Option<Id>,
    /// The recorded keys that read its verdict.
    dependents: Vec<u32>,
    /// Its decision, when it has output (boxed: most keys have none, and the
    /// record holds one row per cold key).
    decision: Option<Box<KeyDecision>>,
}

/// M138 (Order 54): the async pass's COLD instance record — the adaptation
/// (base fixpoint, worklist rounds, decisions) run over the cold nodes alone,
/// every non-cold id unknown (not async) and every value a non-cold module
/// holds in a cold binding skipped, with each key's reads logged. Filed once
/// per post-pass record (`PostRecord::async_instances`) by the first analysis
/// that reads the record while its context log matches; read by every later
/// one. A pure function of the cold tree in that rewrite state, so the hot
/// set never enters it.
pub(crate) struct AsyncInstanceRecord {
    /// The cold run's settled async set — the seed, a lower bound of every
    /// served analysis's set (the cold view is monotone below it).
    settled: HashSet<Id>,
    keys: Vec<RecordedKey>,
    index: HashMap<InstanceKey, u32>,
    /// The keys that read an id as not async, by id.
    readers_by_id: HashMap<Id, Vec<u32>>,
    /// The keys that read a binding's held values, by binding.
    readers_by_binding: HashMap<Id, Vec<u32>>,
    /// The keys that read a dispatch, by the dispatched member's NAME — a
    /// candidate that becomes async mid-round re-walks them.
    readers_by_name: HashMap<String, Vec<u32>>,
    /// The keys that read each dispatch call, asked again at every round's
    /// start (a hot impl can widen the candidates).
    dispatch_reads: Vec<(Id, Vec<u32>)>,
    /// The keys whose reads the record cannot validate.
    unrecordable: Vec<u32>,
    /// The keys whose decision has output.
    with_output: Vec<u32>,
    /// What the record retains, in bytes (M46's currency).
    pub bytes: usize,
}

impl std::fmt::Debug for AsyncInstanceRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AsyncInstanceRecord")
            .field("settled", &self.settled.len())
            .field("keys", &self.keys.len())
            .field("with_output", &self.with_output.len())
            .field("bytes", &self.bytes)
            .finish()
    }
}

/// The record's own run (see [`AsyncInstanceRecord`]): `seeds` are the
/// program's own seeds (declared async, awaiting nodes, async blocks).
fn cold_instance_record(adapt: Adapt, seeds: &HashSet<Id>) -> AsyncInstanceRecord {
    let program = adapt.program;
    let mut cold: HashSet<Id> = seeds
        .iter()
        .copied()
        .filter(|id| program.cold_entity(*id))
        .collect();
    let round = loop {
        cold_base_fixpoint(adapt, &mut cold);
        let before = cold.len();
        let round = cold_round(adapt, &mut cold);
        if cold.len() == before {
            break round;
        }
    };
    let ColdRound {
        instance_async,
        origins,
        mut dependents,
        mut reads,
    } = round;
    // The decisions over the cold run's final verdicts, logged into the same
    // reads; a callee verdict a decision reads is a dependency too.
    let current = std::cell::RefCell::new(KeyReads::default());
    let view = View {
        set: &cold,
        cold: true,
        reads: Some(&current),
    };
    let flag_of = |key: &InstanceKey| instance_async.get(key).copied();
    let mut decisions: HashMap<InstanceKey, KeyDecision> = HashMap::default();
    for key in instance_async.keys() {
        let decision = decide_instance(adapt, view, key, &flag_of, &mut |callee_key| {
            dependents
                .entry(callee_key.clone())
                .or_default()
                .insert(key.clone());
        });
        reads.entry(key.clone()).or_default().absorb(current.take());
        if decision.has_output() {
            decisions.insert(key.clone(), decision);
        }
    }
    let mut ordered: Vec<&InstanceKey> = instance_async.keys().collect();
    ordered
        .sort_by_cached_key(|key| (key.0.0, key.1.iter().map(|bit| bit.0).collect::<Vec<u32>>()));
    let index: HashMap<InstanceKey, u32> = ordered
        .iter()
        .enumerate()
        .map(|(position, key)| ((*key).clone(), position as u32))
        .collect();
    let mut record = AsyncInstanceRecord {
        settled: HashSet::default(),
        keys: Vec::with_capacity(ordered.len()),
        index: HashMap::default(),
        readers_by_id: HashMap::default(),
        readers_by_binding: HashMap::default(),
        readers_by_name: HashMap::default(),
        dispatch_reads: Vec::new(),
        unrecordable: Vec::new(),
        with_output: Vec::new(),
        bytes: 0,
    };
    let mut dispatch_readers: HashMap<Id, Vec<u32>> = HashMap::default();
    let mut bytes = 0usize;
    for (position, key) in ordered.iter().enumerate() {
        let position = position as u32;
        let mut key_dependents: Vec<u32> = dependents
            .get(*key)
            .into_iter()
            .flatten()
            .filter_map(|dependent| index.get(dependent).copied())
            .collect();
        key_dependents.sort_unstable();
        key_dependents.dedup();
        let mut key_reads = reads.remove(*key).unwrap_or_default();
        if key_reads.unrecordable {
            record.unrecordable.push(position);
        }
        key_reads.ids.sort_unstable_by_key(|id| id.0);
        key_reads.ids.dedup();
        for id in &key_reads.ids {
            record.readers_by_id.entry(*id).or_default().push(position);
        }
        key_reads.bindings.sort_unstable_by_key(|id| id.0);
        key_reads.bindings.dedup();
        for binding in &key_reads.bindings {
            record
                .readers_by_binding
                .entry(*binding)
                .or_default()
                .push(position);
        }
        key_reads.dispatches.sort_unstable_by_key(|id| id.0);
        key_reads.dispatches.dedup();
        for call_id in &key_reads.dispatches {
            dispatch_readers.entry(*call_id).or_default().push(position);
            if let Some(member) = dispatched_member(program, *call_id) {
                let readers = record
                    .readers_by_name
                    .entry(member.to_string())
                    .or_default();
                if readers.last() != Some(&position) {
                    readers.push(position);
                }
            }
        }
        let decision = decisions.remove(*key).map(Box::new);
        if decision.is_some() {
            record.with_output.push(position);
        }
        bytes += std::mem::size_of::<RecordedKey>()
            + key.1.len() * std::mem::size_of::<Id>()
            + key_dependents.len() * 4
            + (key_reads.ids.len() + key_reads.bindings.len() + key_reads.dispatches.len()) * 8
            + decision.as_deref().map_or(0, KeyDecision::bytes);
        record.keys.push(RecordedKey {
            key: (*key).clone(),
            flag: instance_async[*key],
            origin: origins.get(*key).copied(),
            dependents: key_dependents,
            decision,
        });
    }
    let mut dispatch_reads: Vec<(Id, Vec<u32>)> = dispatch_readers.into_iter().collect();
    dispatch_reads.sort_unstable_by_key(|(call_id, _)| call_id.0);
    record.dispatch_reads = dispatch_reads;
    record.index = index;
    bytes += cold.len() * std::mem::size_of::<Id>() * 2;
    record.settled = cold;
    record.bytes = bytes;
    record
}

/// The member a dispatch call names, by the record that dispatches it.
fn dispatched_member<'src>(program: &Program<'src>, call_id: Id) -> Option<&'src str> {
    if let Some(member) = program.dyn_method_calls.get(&call_id) {
        return Some(member);
    }
    match dispatch_at(program, call_id)? {
        GenericDispatch::OnConstraint(_, member) | GenericDispatch::OnType(_, member) => {
            Some(member)
        }
    }
}

/// The base fixpoint over the COLD nodes in the cold view: S3b's cold result,
/// and the record's run's half of each round.
fn cold_base_fixpoint(adapt: Adapt, cold: &mut HashSet<Id>) {
    let program = adapt.program;
    loop {
        let mut changed = false;
        for node in adapt.graph.nodes() {
            let id = node.id();
            if cold.contains(&id) || !program.cold_entity(id) {
                continue;
            }
            let view = View {
                set: cold,
                cold: true,
                reads: None,
            };
            if adapt
                .graph
                .calls_of(id)
                .iter()
                .any(|call| call_suspends(program, call, adapt.held_values, view))
            {
                cold.insert(id);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// One worklist round of the record's run, from scratch, over the cold keys:
/// the cold functions' base instances and what cold calls discover, read in
/// the cold view with every key's reads logged.
struct ColdRound {
    instance_async: HashMap<InstanceKey, bool>,
    origins: HashMap<InstanceKey, Id>,
    dependents: HashMap<InstanceKey, HashSet<InstanceKey>>,
    reads: HashMap<InstanceKey, KeyReads>,
}

fn cold_round(adapt: Adapt, cold: &mut HashSet<Id>) -> ColdRound {
    let program = adapt.program;
    let mut instance_async: HashMap<InstanceKey, bool> = HashMap::default();
    let mut origins: HashMap<InstanceKey, Id> = HashMap::default();
    let mut dependents: HashMap<InstanceKey, HashSet<InstanceKey>> = HashMap::default();
    let mut reads: HashMap<InstanceKey, KeyReads> = HashMap::default();
    let mut pending: Vec<InstanceKey> = Vec::new();
    let mut queued: HashSet<InstanceKey> = HashSet::default();
    // Sorted, so the record's run is a function of the cold tree alone and
    // not of the map's iteration order.
    let mut roots: Vec<Id> = program
        .functions
        .keys()
        .copied()
        .filter(|function_id| program.cold_entity(*function_id))
        .collect();
    roots.sort_unstable_by_key(|id| std::cmp::Reverse(id.0));
    for root in roots {
        let key = (root, Vec::new());
        queued.insert(key.clone());
        pending.push(key);
    }
    for binding in program.module_level_bindings() {
        if !program.cold_entity(binding) {
            continue;
        }
        for call in adapt.graph.initializer_calls_of(binding) {
            if let CallTarget::Function(callee) = call.target
                && program.cold_entity(callee)
            {
                let view = View {
                    set: cold,
                    cold: true,
                    reads: None,
                };
                let bits = bits_for_call(
                    program,
                    adapt.held_values,
                    view,
                    adapt.adaptive,
                    &HashMap::default(),
                    &[],
                    callee,
                    call.call_id,
                );
                if !bits.is_empty() {
                    let key = (callee, bits);
                    instance_async.entry(key.clone()).or_insert(false);
                    record_origin(&mut origins, &key, call.call_id);
                    if queued.insert(key.clone()) {
                        pending.push(key);
                    }
                }
            }
        }
    }
    let current = std::cell::RefCell::new(KeyReads::default());
    while let Some(key) = pending.pop() {
        queued.remove(&key);
        let flags = {
            let view = View {
                set: cold,
                cold: true,
                reads: Some(&current),
            };
            let mut encounter = |callee_key: InstanceKey, call_id: Id| -> bool {
                // A cold body naming a non-cold callee's instance is not the
                // cold world's to record: its verdict is re-walked always.
                if !program.cold_entity(callee_key.0) {
                    current.borrow_mut().unrecordable = true;
                    return false;
                }
                dependents
                    .entry(callee_key.clone())
                    .or_default()
                    .insert(key.clone());
                record_origin(&mut origins, &callee_key, call_id);
                match instance_async.get(&callee_key) {
                    Some(flag) => *flag,
                    None => {
                        instance_async.insert(callee_key.clone(), false);
                        if queued.insert(callee_key.clone()) {
                            pending.push(callee_key);
                        }
                        false
                    }
                }
            };
            evaluate_instance(adapt, view, &key, &mut encounter)
        };
        reads.entry(key.clone()).or_default().absorb(current.take());
        let root_flag = flags[&key.0];
        let previous = instance_async.insert(key.clone(), root_flag);
        if root_flag && previous != Some(true) {
            for dependent in dependents.get(&key).cloned().unwrap_or_default() {
                if queued.insert(dependent.clone()) {
                    pending.push(dependent);
                }
            }
        }
        if key.1.is_empty() {
            for (member, flag) in &flags {
                if *flag {
                    cold.insert(*member);
                }
            }
        }
    }
    ColdRound {
        instance_async,
        origins,
        dependents,
        reads,
    }
}

/// A served analysis's replay state over a world's [`AsyncInstanceRecord`]:
/// which recorded keys it re-walks (something they read moved), the rest
/// answered from the record.
struct Seeding<'r> {
    record: &'r AsyncInstanceRecord,
    /// The recorded keys re-walked — by index.
    invalid: Vec<bool>,
    rewalked: u64,
    /// The ids of the async set already compared with the record's.
    seen: HashSet<Id>,
    /// Whether the once-per-analysis invalidations ran.
    started: bool,
    /// `false` under [`crate::incremental::Plant::AsyncRecordUnguarded`]: no
    /// read is validated.
    guarded: bool,
}

impl<'r> Seeding<'r> {
    fn new(record: &'r AsyncInstanceRecord) -> Seeding<'r> {
        Seeding {
            record,
            invalid: vec![false; record.keys.len()],
            rewalked: 0,
            seen: HashSet::default(),
            started: false,
            guarded: !crate::incremental::planted(crate::incremental::Plant::AsyncRecordUnguarded),
        }
    }

    fn index_of(&self, key: &InstanceKey) -> Option<usize> {
        self.record.index.get(key).map(|index| *index as usize)
    }

    /// Whether the record answers `key` (recorded and not re-walked).
    fn serves(&self, key: &InstanceKey) -> bool {
        self.index_of(key).is_some_and(|index| !self.invalid[index])
    }

    /// The recorded verdict of a key the record answers.
    fn served_flag(&self, key: &InstanceKey) -> Option<bool> {
        self.index_of(key)
            .filter(|index| !self.invalid[*index])
            .map(|index| self.record.keys[index].flag)
    }

    /// The recorded verdict — a re-walked key's starting point (a lower
    /// bound of its verdict now).
    fn recorded_flag_of(&self, key: &InstanceKey) -> bool {
        self.index_of(key)
            .is_some_and(|index| self.record.keys[index].flag)
    }

    fn recorded_origin(&self, key: &InstanceKey) -> Option<Id> {
        self.index_of(key)
            .and_then(|index| self.record.keys[index].origin)
    }

    fn recorded_dependents(&self, key: &InstanceKey) -> Vec<u32> {
        self.index_of(key)
            .map(|index| self.record.keys[index].dependents.clone())
            .unwrap_or_default()
    }

    /// Marks a recorded key re-walked; answers it when it was not already.
    fn rewalk(&mut self, index: u32) -> Option<InstanceKey> {
        let index = index as usize;
        if self.invalid[index] {
            return None;
        }
        self.invalid[index] = true;
        self.rewalked += 1;
        Some(self.record.keys[index].key.clone())
    }

    fn rewalk_all(&mut self, indices: &[u32]) -> Vec<InstanceKey> {
        indices
            .iter()
            .filter_map(|index| self.rewalk(*index))
            .collect()
    }

    /// `id` is async in this analysis: when the record's cold world did not
    /// make it so, the keys that read it as not async — by id, and by name
    /// for a dispatch candidate — are re-walked.
    fn moved(&mut self, program: &Program, id: Id) -> Vec<InstanceKey> {
        if !self.guarded || self.record.settled.contains(&id) || !self.seen.insert(id) {
            return Vec::new();
        }
        let mut rewalked = Vec::new();
        if let Some(readers) = self.record.readers_by_id.get(&id) {
            rewalked.extend(self.rewalk_all(readers));
        }
        let name = program
            .functions
            .get(&id)
            .map(|function| function.name)
            .or_else(|| {
                program
                    .external_functions
                    .get(&id)
                    .map(|external| external.name)
            });
        if let Some(readers) = name.and_then(|name| self.record.readers_by_name.get(name)) {
            rewalked.extend(self.rewalk_all(readers));
        }
        rewalked
    }

    /// A round's start: once per analysis the keys the record cannot
    /// validate and the readers of a binding a non-cold module now assigns;
    /// every round the async set's ids the record's set lacks and every
    /// dispatch a recorded key read, asked again.
    fn round_start(&mut self, adapt: Adapt, async_set: &HashSet<Id>) {
        if !self.guarded {
            return;
        }
        let program = adapt.program;
        if !self.started {
            self.started = true;
            let unrecordable = self.record.unrecordable.clone();
            self.rewalk_all(&unrecordable);
            for (binding, readers) in &self.record.readers_by_binding {
                let foreign = adapt.held_values.get(binding).is_some_and(|held| {
                    held.iter().any(|value_id| !program.cold_entity(*value_id))
                });
                if foreign {
                    for reader in readers {
                        self.rewalk(*reader);
                    }
                }
            }
        }
        let mut moved: Vec<Id> = async_set
            .iter()
            .copied()
            .filter(|id| !self.record.settled.contains(id) && !self.seen.contains(id))
            .collect();
        moved.sort_unstable_by_key(|id| id.0);
        for id in moved {
            self.moved(program, id);
        }
        let view = View::plain(async_set);
        let mut verdicts: HashMap<(bool, Option<TypeId>, &str), bool> = HashMap::default();
        for (call_id, readers) in &self.record.dispatch_reads {
            if readers.iter().all(|reader| self.invalid[*reader as usize]) {
                continue;
            }
            let async_ = match dispatch_at(program, *call_id) {
                Some(dispatch) if !program.dyn_method_calls.contains_key(call_id) => {
                    let key = match dispatch {
                        GenericDispatch::OnConstraint(constraint_id, member) => {
                            (true, Some(constraint_id), member)
                        }
                        GenericDispatch::OnType(type_id, member) => (false, type_id, member),
                    };
                    *verdicts
                        .entry(key)
                        .or_insert_with(|| view.dispatch_async(program, *call_id))
                }
                _ => view.dispatch_async(program, *call_id),
            };
            if async_ {
                for reader in readers {
                    self.rewalk(*reader);
                }
            }
        }
    }

    /// Every recorded key re-walked so far.
    fn rewalked_keys(&self) -> Vec<InstanceKey> {
        self.invalid
            .iter()
            .enumerate()
            .filter(|(_, invalid)| **invalid)
            .map(|(index, _)| self.record.keys[index].key.clone())
            .collect()
    }

    /// The recorded decisions with output of the keys never re-walked.
    fn served_decisions(&self) -> impl Iterator<Item = (&'r InstanceKey, &'r KeyDecision)> + '_ {
        self.record
            .with_output
            .iter()
            .map(|index| *index as usize)
            .filter(|index| !self.invalid[*index])
            .filter_map(|index| {
                let recorded = &self.record.keys[index];
                recorded
                    .decision
                    .as_deref()
                    .map(|decision| (&recorded.key, decision))
            })
    }
}

/// Whether the call's subject is async THROUGH this instance's context: a
/// call through an adapted parameter, or through a binding holding a closure
/// that is async under these bits.
fn subject_adapted_here(
    program: &Program,
    held_values: &HashMap<Id, Vec<Id>>,
    flags: &HashMap<Id, bool>,
    bits: &[Id],
    call_id: Id,
) -> bool {
    program
        .function_calls
        .get(&call_id)
        .and_then(|function_call| program.entity_map.get(&function_call.subject_id))
        .is_some_and(|subject| match subject {
            Expr::Local(target) => {
                bits.contains(target)
                    || held_values.get(target).is_some_and(|held| {
                        held.iter().any(|held_id| {
                            matches!(
                                program.entity_map.get(held_id),
                                Some(Expr::Closure(closure_id))
                                    if flags.get(closure_id).copied().unwrap_or(false)
                            )
                        })
                    })
            }
            _ => false,
        })
}

/// Whether the parameter's type is a closure at all — the shape a `sync`
/// CONTRACT applies to. A contract binds whatever the callback returns:
/// `sync || void` says the callback completes before the declaring function's
/// protocol moves on, exactly as `sync || i32` does (B61).
fn parameter_is_closure(program: &Program, parameter_id: Id) -> bool {
    program
        .parameters
        .get(&parameter_id)
        .and_then(|parameter| program.type_id_to_type_map.get(&parameter.type_id))
        .is_some_and(|type_| matches!(type_, Type::Closure(..)))
}

/// Whether the parameter's type is a closure with a RESOLVED, non-void
/// return — the shape that ADAPTS. Not the shape a contract applies to: see
/// `parameter_is_closure` for that.
fn closure_return_is_value(program: &Program, parameter_id: Id) -> bool {
    let Some(parameter) = program.parameters.get(&parameter_id) else {
        return false;
    };
    let Some(Type::Closure(_, return_type, _, _)) =
        program.type_id_to_type_map.get(&parameter.type_id).cloned()
    else {
        return false;
    };
    !matches!(
        program.type_id_to_type_map.get(&return_type),
        Some(Type::Void) | Some(Type::Unknown) | Some(Type::Unresolved) | None
    )
}

/// A diagnostic anchored at an entity: reported at that entity's span, in the
/// file that span indexes into (backlog E16). Span and file always come from
/// the SAME entity, so a diagnostic this whole-program pass raises renders in —
/// and is published to — the file it is about, not the entry it was reached
/// from; and generated code re-anchors at the attribute that generated it
/// (`Program::anchored`), which is the only location a reader can act on.
/// One store into a closure-typed struct field: which field, and the value
/// expression that lands in it.
#[derive(Clone, Copy, Debug)]
struct FieldStore {
    struct_id: Id,
    field_index: usize,
    value_id: Id,
}

/// Whether a type is a closure whose return awaits something worth lying
/// about — i.e. NOT void (A.3: void positions keep spawn semantics) and not
/// still unresolved.
fn plain_closure_position(program: &Program, type_id: TypeId) -> bool {
    let Some(Type::Closure(_, return_type, _, _)) = program.type_id_to_type_map.get(&type_id)
    else {
        return false;
    };
    !matches!(
        program.type_id_to_type_map.get(return_type),
        Some(Type::Void) | Some(Type::Unknown) | Some(Type::Unresolved) | None
    )
}

/// The stores — struct literals and field assignments, gathered by `infer`'s
/// one walk of the entity map — whose target is a PLAIN
/// (non-`async`) closure-typed field with a non-void return: the positions
/// where an async value would lie about what a later call through the field
/// hands back. Collected in one place because two checks ask the same question
/// of the same positions: the global divergence check, and its per-instance
/// twin (A.4's adapted-instance escape, closed 2026-08-04).
fn plain_closure_field_stores(program: &Program, mut stores: Vec<FieldStore>) -> Vec<FieldStore> {
    stores.retain(|store| {
        !program
            .async_fields
            .contains(&(store.struct_id, store.field_index))
            && field_of(program, store)
                .is_some_and(|field| plain_closure_position(program, field.type_id))
    });
    stores
}

/// The field a store targets.
fn field_of<'program>(
    program: &'program Program,
    store: &FieldStore,
) -> Option<&'program crate::analyzer::Field<'program>> {
    program
        .structs
        .get(&store.struct_id)?
        .fields
        .get(store.field_index)
}

/// Every return site whose function declares a PLAIN (non-`async`)
/// closure-typed return with a non-void result — the return-position twin of
/// [`plain_closure_field_stores`].
fn plain_closure_return_sites(program: &Program) -> Vec<(Id, Id)> {
    program
        .return_sites
        .iter()
        .copied()
        .filter(|(function_id, _)| {
            !program.async_returning.contains(function_id)
                && program
                    .functions
                    .get(function_id)
                    .and_then(|function| function.return_type_id)
                    .is_some_and(|declared| plain_closure_position(program, declared))
        })
        .collect()
}

/// The field-escape wording, shared by the global check and its per-instance
/// twin so both name the same fix.
fn field_escape_message(program: &Program, store: &FieldStore) -> String {
    let struct_name = program
        .structs
        .get(&store.struct_id)
        .map(|struct_| struct_.name)
        .unwrap_or("the struct");
    let field_name = field_of(program, store)
        .map(|field| field.name)
        .unwrap_or("the field");
    format!(
        "field `{field_name}` of `{struct_name}` receives an async closure, but its type awaits nothing; declare it `async || T` (or return void for spawn semantics)"
    )
}

/// The return-escape wording, likewise shared.
fn return_escape_message(program: &Program, function_id: Id) -> String {
    let name = program
        .functions
        .get(&function_id)
        .map(|function| function.name)
        .unwrap_or("this function");
    format!(
        "`{name}` returns an async closure, but its declared return type awaits nothing; declare it `async || T` (or return void for spawn semantics)"
    )
}

fn anchored(
    program: &Program,
    id: Id,
    msg: String,
    note: Option<crate::error::Note>,
) -> (crate::error::Error, SourceId) {
    program.anchored(
        crate::error::Error {
            trace: Vec::new(),
            note,
            span: span_of(program, id),
            msg,
        },
        id,
    )
}

/// The span of an entity, or an empty fallback.
fn span_of(program: &Program, id: Id) -> crate::span::Span {
    program
        .span_map
        .get(&id)
        .map(|span| **span)
        .unwrap_or(crate::span::Span { start: 0, end: 0 })
}

/// Transitive `sync` violations at one call: an argument async ONLY through
/// the instance's bits flowing into a `sync`-contract parameter. (The direct
/// case — async at the call site itself — is the global divergence check's.)
fn sync_violations_at(
    program: &Program,
    held_values: &HashMap<Id, Vec<Id>>,
    view: View,
    flags: &HashMap<Id, bool>,
    bits: &[Id],
    callee: Id,
    call_id: Id,
    reports: &mut Vec<Report>,
) {
    let Some(function) = program.functions.get(&callee) else {
        return;
    };
    let Some(function_call) = program.function_calls.get(&call_id) else {
        return;
    };
    let empty_flags = HashMap::default();
    for (argument, parameter) in function_call.argument_ids.iter().zip(&function.parameters) {
        // B61: the contract, not the adaptation shape — a `sync` marker binds a
        // void-returning callback too.
        if !program.sync_values.contains(parameter) || !parameter_is_closure(program, *parameter) {
            continue;
        }
        if !value_async_in(program, held_values, view, flags, bits, *argument) {
            continue;
        }
        // Direct violations report at the global check with the plain steer.
        if value_async_in(program, held_values, view, &empty_flags, &[], *argument) {
            continue;
        }
        reports.push(Report::Sync {
            call_id,
            parameter: *parameter,
        });
    }
}

/// Transitive host-boundary violations: an argument async only through the
/// instance's bits flowing into an `external` function's value-returning
/// closure parameter.
fn extern_violations_at(
    program: &Program,
    held_values: &HashMap<Id, Vec<Id>>,
    view: View,
    flags: &HashMap<Id, bool>,
    bits: &[Id],
    callee: Id,
    call_id: Id,
    reports: &mut Vec<Report>,
) {
    // The `Context::run` intrinsic is not a host boundary — see the direct
    // host-boundary check in `infer` (E68): a surviving `run` call means
    // `thread_contexts` already refused and reported it.
    if Some(callee) == program.context_run_fn_id || Some(callee) == program.context_clear_fn_id {
        return;
    }
    let Some(external) = program.external_functions.get(&callee) else {
        return;
    };
    let Some(function_call) = program.function_calls.get(&call_id) else {
        return;
    };
    let empty_flags = HashMap::default();
    for (argument, parameter) in function_call.argument_ids.iter().zip(&external.parameters) {
        // The typed channel: a declared `async |…| T` parameter means the
        // host awaits the closure itself (`__nursery_run`'s body parameter).
        if program.async_values.contains(parameter) {
            continue;
        }
        if !closure_return_is_value(program, *parameter) {
            continue;
        }
        if !value_async_in(program, held_values, view, flags, bits, *argument) {
            continue;
        }
        if value_async_in(program, held_values, view, &empty_flags, &[], *argument) {
            continue;
        }
        reports.push(Report::Extern {
            call_id,
            parameter: *parameter,
            callee,
        });
    }
}

/// Adaptation cannot ride a trait/generic dispatch (the callee varies per
/// instantiation) — an async closure argument at such a call is refused
/// unless every candidate's parameter at that position is `async`-declared
/// (the typed channel, which works today).
fn dispatch_refusals_at(
    program: &Program,
    held_values: &HashMap<Id, Vec<Id>>,
    view: View,
    flags: &HashMap<Id, bool>,
    bits: &[Id],
    call_id: Id,
    reports: &mut Vec<Report>,
) {
    let Some(function_call) = program.function_calls.get(&call_id) else {
        return;
    };
    let candidates = dispatch_candidates(program, call_id);
    if candidates.is_empty() {
        return;
    }
    for (position, argument) in function_call.argument_ids.iter().enumerate() {
        if !value_async_in(program, held_values, view, flags, bits, *argument) {
            continue;
        }
        // Refuse only where a candidate would need ADAPTATION: a plain,
        // value-returning closure parameter at this position. All-`async`
        // candidates are the typed channel and work as-is.
        let needs_adaptation = candidates.iter().any(|candidate| {
            program
                .functions
                .get(candidate)
                .and_then(|function| function.parameters.get(position))
                .is_some_and(|parameter| {
                    !program.async_values.contains(parameter)
                        && !program.sync_values.contains(parameter)
                        && closure_return_is_value(program, *parameter)
                })
        });
        if !needs_adaptation {
            continue;
        }
        reports.push(Report::Dispatch { call_id, position });
    }
}

/// A.4's **adapted-instance escape**, closed 2026-08-04: a value that is async
/// ONLY through this instance's bits, escaping into a plain closure-typed field
/// or through a plain declared return.
///
/// A.4 recorded that adaptation covers the closures a body CALLS, and asserted
/// that a body which stores or returns one instead "uses the existing rules
/// (the field/return divergence checks catch lies)". Those checks run outside
/// any instance context — `no_flags`, empty bits — so they only ever saw the
/// GLOBAL async channels. A plain parameter that went async at one call site
/// was invisible to them, and the value escaped with its asyncness stripped: a
/// `|| i32` field holding a promise, and a later call through it typed `i32`.
///
/// This is the same two checks with the context they were missing, not a new
/// check class, so it inherits their exemptions verbatim — a void position
/// keeps spawn semantics (A.3), a declared `async || T` position is the fix.
/// The conservative ruling stands where A.4 put it: the escape is an ERROR
/// naming the adaptation, not a per-instance asyncness for the field or the
/// return, which is the whole-program effect-row problem A.4 declined.
///
/// Scanning every store/return per instance is sound because the report is
/// gated on "async THROUGH the bits and not without them": the bits are this
/// callee's own parameter ids and the flags are this component's own members,
/// so a position outside the instance cannot pass both halves. The direct case
/// is the global check's, exactly as `sync_violations_at` leaves it.
fn escape_violations_in(
    program: &Program,
    held_values: &HashMap<Id, Vec<Id>>,
    view: View,
    flags: &HashMap<Id, bool>,
    bits: &[Id],
    escapes: &Escapes,
    reports: &mut Vec<Report>,
) {
    let empty_flags = HashMap::default();
    let escapes_here = |value_id: Id| {
        value_async_in(program, held_values, view, flags, bits, value_id)
            && !value_async_in(program, held_values, view, &empty_flags, &[], value_id)
    };
    // In the cold view a position outside the cold set is not asked at all:
    // it cannot pass both halves (above), and asking would read the hot
    // set's own ids.
    let asked = |value_id: Id| !view.cold || program.cold_entity(value_id);
    for store in &escapes.field_stores {
        if asked(store.value_id) && escapes_here(store.value_id) {
            reports.push(Report::FieldEscape(*store));
        }
    }
    for (function_id, value_id) in &escapes.return_sites {
        if asked(*value_id) && escapes_here(*value_id) {
            reports.push(Report::ReturnEscape {
                function_id: *function_id,
                value_id: *value_id,
            });
        }
    }
}

/// Whether `value_id` is an async closure VALUE in an instance's context:
/// the global channels (typed, adopted holds, fields, returning calls), an
/// adapted parameter of the enclosing instance, or a closure literal whose
/// flag (component-local or global) is set.
fn value_async_in(
    program: &Program,
    held_values: &HashMap<Id, Vec<Id>>,
    view: View,
    flags: &HashMap<Id, bool>,
    bits: &[Id],
    value_id: Id,
) -> bool {
    let closure_async = |closure_id: &Id| {
        flags
            .get(closure_id)
            .copied()
            .unwrap_or_else(|| view.contains(program, *closure_id))
    };
    match program.entity_map.get(&value_id) {
        Some(Expr::Closure(closure_id)) | Some(Expr::Async(closure_id)) => {
            closure_async(closure_id)
        }
        Some(Expr::Local(target)) => {
            bits.contains(target) || program.async_values.contains(target) || {
                view.note_held(*target);
                held_values.get(target).is_some_and(|held| {
                    held.iter()
                        .filter(|held_id| view.admits_held(program, **held_id))
                        .any(|held_id| match program.entity_map.get(held_id) {
                            Some(Expr::Closure(closure_id)) => closure_async(closure_id),
                            Some(Expr::Field(_, struct_id, index)) => {
                                program.async_fields.contains(&(*struct_id, *index))
                            }
                            Some(Expr::Call(inner)) => call_returns_async_closure(program, *inner),
                            _ => false,
                        })
                })
            }
        }
        Some(Expr::Field(_, struct_id, index)) => {
            program.async_fields.contains(&(*struct_id, *index))
        }
        Some(Expr::Call(inner)) => call_returns_async_closure(program, *inner),
        _ => false,
    }
}

/// The callee's adapted-parameter set for one call, in an instance's
/// context — empty when nothing adapts.
fn bits_for_call(
    program: &Program,
    held_values: &HashMap<Id, Vec<Id>>,
    view: View,
    adaptive: &HashMap<Id, HashSet<Id>>,
    flags: &HashMap<Id, bool>,
    bits: &[Id],
    callee: Id,
    call_id: Id,
) -> Vec<Id> {
    let Some(adaptive_params) = adaptive.get(&callee) else {
        return Vec::new();
    };
    let Some(function) = program.functions.get(&callee) else {
        return Vec::new();
    };
    let Some(function_call) = program.function_calls.get(&call_id) else {
        return Vec::new();
    };
    let mut callee_bits: Vec<Id> = function_call
        .argument_ids
        .iter()
        .zip(&function.parameters)
        .filter(|(argument, parameter)| {
            adaptive_params.contains(*parameter)
                && value_async_in(program, held_values, view, flags, bits, **argument)
        })
        .map(|(_, parameter)| *parameter)
        .collect();
    callee_bits.sort_by_key(|parameter| parameter.0);
    callee_bits
}

/// The plain (single-bit) async fixpoint: a node that calls an async
/// function/extern/value implicitly awaits it, so it is async too. Run to
/// stability; interleaved with the adaptation worklist by `infer`.
fn base_fixpoint(
    program: &Program,
    graph: &CallGraph,
    held_values: &HashMap<Id, Vec<Id>>,
    async_set: &mut HashSet<Id>,
) {
    loop {
        let mut changed = false;
        for node in graph.nodes() {
            let id = node.id();
            if async_set.contains(&id) {
                continue;
            }
            let view = View::plain(async_set);
            let calls_async = graph
                .calls_of(id)
                .iter()
                .any(|call| call_suspends(program, call, held_values, view));
            if calls_async {
                async_set.insert(id);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// Whether one call site suspends its caller — i.e. whether the emission
/// `await`s it, which is the same thing: an `await` yields to the microtask
/// queue whatever it is handed, so any turn may run before the caller resumes.
///
/// This is the single per-call test [`base_fixpoint`] propagates over AND the
/// one B119's view rule reads back (`Program::suspending_calls`). One
/// definition, so the checker's answer and the emitter's cannot drift.
///
/// A trait/generic-bounded dispatch is suspending when ANY candidate impl is,
/// matching what the fixpoint already does to decide the caller's own
/// asyncness. It is an over-approximation on purpose: the concrete member is
/// chosen per monomorphized instance, and no pre-monomorphization pass can
/// know which one this site gets.
fn call_suspends(
    program: &Program,
    call: &Call,
    held_values: &HashMap<Id, Vec<Id>>,
    view: View,
) -> bool {
    match call.target {
        CallTarget::Function(callee) | CallTarget::External(callee) => {
            view.contains(program, callee)
        }
        CallTarget::Indirect(IndirectReason::GenericMember | IndirectReason::TraitDispatch) => {
            view.dispatch_async(program, call.call_id)
        }
        // A call through an `async || T`-typed value IS an await point —
        // asyncness rides the type (J2), or the VALUE FLOW: a binding holding
        // an async closure, an async field read, an async-returning call, a
        // directly-applied async closure literal (the lowered `run` body).
        // Other higher-order calls stay conservative (the concrete target
        // isn't recoverable), as do variant constructors.
        _ => program
            .function_calls
            .get(&call.call_id)
            .is_some_and(|function_call| {
                subject_awaits(program, function_call.subject_id, held_values, view, 0)
            }),
    }
}

/// [`call_suspends`] with the DISPATCH verdict memoized by dispatch key.
///
/// `dispatch_candidates` scans every impl and every trait for the member name,
/// so asking it once per indirect edge — rather than once per node that has not
/// flipped yet, which is all the fixpoint ever does — is what an unmemoized
/// materialization pass measured at +2.0 ms of an 18 ms post-pass phase. The
/// verdict is a function of the dispatch record alone, so one entry per
/// distinct `(constraint-or-type, member)` answers every site that shares it:
/// +0.3 ms.
///
/// Sound only once `async_set` is FINAL — a growing set would strand a `false`
/// — which is exactly where the materialization runs, and why `base_fixpoint`
/// cannot share the memo.
fn call_suspends_memoized<'src>(
    program: &Program<'src>,
    call: &Call,
    held_values: &HashMap<Id, Vec<Id>>,
    async_set: &HashSet<Id>,
    verdicts: &mut HashMap<(bool, Option<TypeId>, &'src str), bool>,
) -> bool {
    if !matches!(
        call.target,
        CallTarget::Indirect(IndirectReason::GenericMember | IndirectReason::TraitDispatch)
    ) {
        return call_suspends(program, call, held_values, View::plain(async_set));
    }
    // No dispatch record is `dispatch_candidates`' own empty answer.
    let Some(dispatch) = dispatch_at(program, call.call_id) else {
        return false;
    };
    let key = match dispatch {
        GenericDispatch::OnConstraint(constraint_id, member) => (true, Some(constraint_id), member),
        GenericDispatch::OnType(type_id, member) => (false, type_id, member),
    };
    if let Some(verdict) = verdicts.get(&key) {
        return *verdict;
    }
    let verdict = dispatch_candidates(program, call.call_id)
        .iter()
        .any(|member| async_set.contains(member));
    verdicts.insert(key, verdict);
    verdict
}

/// A function's ADAPTIVE parameters: plain (unmarked), closure-typed, with a
/// RESOLVED non-void return. `sync`/`async`-marked and void/unresolved ones
/// never adapt (contract, spawn, or no known lie).
fn adaptive_params_of(program: &Program) -> HashMap<Id, HashSet<Id>> {
    let mut adaptive: HashMap<Id, HashSet<Id>> = HashMap::default();
    for (function_id, function) in &program.functions {
        let params: HashSet<Id> = function
            .parameters
            .iter()
            .filter(|parameter| {
                !program.sync_values.contains(parameter)
                    && !program.async_values.contains(parameter)
                    && closure_return_is_value(program, **parameter)
            })
            .copied()
            .collect();
        if !params.is_empty() {
            adaptive.insert(*function_id, params);
        }
    }
    adaptive
}
