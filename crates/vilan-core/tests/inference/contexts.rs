//! The context-threading pass (`context.rs`): what the rewrite leaves in the
//! tree the emitters read. The diagnostics the pass reports are pinned where
//! their subject lives (`callable.rs`, `bounds.rs`, `macros.rs`); these pins
//! own the SHAPE of the rewrite.

use super::support::*;

/// The threaded arguments of every call `relay` makes to `needy`, in source
/// order: the entity appended to each call's argument list.
fn threaded_arguments(source: &str, caller: &str, callee: &str) -> Vec<u32> {
    let source = source.to_string();
    let caller = caller.to_string();
    let callee = callee.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            assert!(
                errors.is_empty(),
                "expected a clean analysis, got: {:#?}",
                errors.iter().map(|error| &error.msg).collect::<Vec<_>>()
            );
            let program = program.expect("a program");
            let function_named = |name: &str| {
                program
                    .functions
                    .iter()
                    .find(|(_, function)| function.name == name)
                    .map(|(id, _)| *id)
                    .unwrap_or_else(|| panic!("no function `{name}`"))
            };
            let caller_id = function_named(&caller);
            let callee_id = function_named(&callee);
            let caller_span = *program.span_map[&caller_id];
            let mut calls: Vec<(usize, u32)> = program
                .function_calls
                .iter()
                .filter(|(call_id, call)| {
                    matches!(
                        program.entity_map.get(&call.subject_id),
                        Some(vilan_core::analyzer::Expr::Local(target)) if *target == callee_id
                    ) && program.span_map.get(call_id).is_some_and(|span| {
                        span.start >= caller_span.start && span.end <= caller_span.end
                    })
                })
                .map(|(call_id, call)| {
                    (
                        program.span_map[call_id].start,
                        call.argument_ids.last().expect("a threaded argument").0,
                    )
                })
                .collect();
            calls.sort_unstable();
            calls.into_iter().map(|(_, argument)| argument).collect()
        })
        .expect("spawn worker")
        .join()
        .expect("the analysis thread")
}

const RELAY: &str = r#"
import std::io::print;
import std::context::Context;

let current: Context<str> = Context::new();

fun needy(tag: str) {
    print(tag + " " + current.get());
}

fun relay(times: i32) {
    needy("first");
    if times > 1 {
        needy("then");
    } else {
        needy("else");
    }
    for tag in ["loop", "again"] {
        needy(tag);
    }
    needy("last");
}

fun main() {
    current.run("ctx", || {
        relay(2);
    });
}
"#;

/// M137: a node's threaded calls share ONE reference entity per context — the
/// same `Expr::Local(parameter)` read appended to every call — rather than a
/// fresh entity per call (66k of them on kolt's client, doubling the entity
/// map on every analysis). Five calls in sequence, in both arms of an `if`,
/// in a loop and last; one entity.
#[test]
fn m137_a_nodes_threaded_calls_share_one_reference_entity() {
    let arguments = threaded_arguments(RELAY, "relay", "needy");
    assert_eq!(arguments.len(), 5, "five threaded calls: {arguments:?}");
    assert!(
        arguments.iter().all(|argument| *argument == arguments[0]),
        "every threaded call of one node passes the same reference entity: {arguments:?}"
    );
}

/// M137's boundary: a CLOSURE that reads its provider's parameter keeps a
/// reference entity of its own — the node is the caller, so the entity never
/// crosses a closure boundary — and a `Some(..)`-wrapped argument (the
/// covered-to-safe boundary) stays a fresh call per site.
#[test]
fn m137_a_closure_keeps_its_own_reference_and_a_wrap_stays_per_call() {
    let source = r#"
import std::io::print;
import std::context::Context;
import std::option::Option::{ Some, None };

let current: Context<str> = Context::new();

fun needy(tag: str) {
    print(tag + " " + current.get());
}

fun peek(): str {
    match current.get_safe() {
        Some(let value) => value,
        None => "none",
    }
}

fun relay() {
    needy("outer");
    let again = || needy("inner");
    again();
    needy("outer again");
    print(peek());
    print(peek());
}

fun main() {
    current.run("ctx", || {
        relay();
    });
}
"#;
    let needy = threaded_arguments(source, "relay", "needy");
    assert_eq!(needy.len(), 3, "three threaded calls of `needy`: {needy:?}");
    // Source order: outer, inner (the closure), outer again.
    assert_eq!(needy[0], needy[2], "the node's own calls share: {needy:?}");
    assert_ne!(
        needy[0], needy[1],
        "the closure keeps its own entity: {needy:?}"
    );
    let peek = threaded_arguments(source, "relay", "peek");
    assert_eq!(peek.len(), 2, "two calls of `peek`: {peek:?}");
    assert_ne!(
        peek[0], peek[1],
        "a `Some(..)` wrap is a call per site: {peek:?}"
    );
}
