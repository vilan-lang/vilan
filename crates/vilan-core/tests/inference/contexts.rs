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

fun peek_current(): str {
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
    print(peek_current());
    print(peek_current());
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
    let peek = threaded_arguments(source, "relay", "peek_current");
    assert_eq!(peek.len(), 2, "two calls of `peek_current`: {peek:?}");
    assert_ne!(
        peek[0], peek[1],
        "a `Some(..)` wrap is a call per site: {peek:?}"
    );
}

/// papers-54's B?2: the context pass threads its argument to a `for` loop's
/// implicit `next` as to a written call — a sync `Iterator` reading the
/// ambient nursery, called outside a nursery and inside one. The loop's call
/// was keyed by the loop in the call graph and had no `FunctionCall` to take
/// the argument, so `next` ran without it (a `TypeError` on JS).
#[test]
fn a_for_loops_next_reading_a_context_is_handed_it() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::iterator::Iterator;
        import std::option::Option::{ self, None, Some };
        import std::task;

        struct Countdown {
            left: i32,
        }

        impl Countdown with Iterator<i32> {
            fun next(&mut self): Option<i32> {
                if self.left == 0 {
                    ret None;
                }
                self.left = self.left - 1;
                if task::ambient_signal().is_some() {
                    Some(self.left + 100)
                } else {
                    Some(self.left)
                }
            }
        }

        fun count_down(label: str) {
            let counter = Countdown { left = 2 };
            for value in counter {
                print(i"{label} {value}");
            }
        }

        fun main() {
            count_down("outside");
            task::nursery(|_| {
                count_down("inside");
            });
        }
        main();
        "#,
        "outside 1\noutside 0\ninside 101\ninside 100\n",
    );
}

// --- B677: a closure PARAMETER at a `context`-typed parameter, and the silent
// twin — a plain closure parameter called inside an owner-establishing body.

/// B677's program: `while_on` runs `provide` under an owner of its own and
/// disposes it; `register` defers `-a` to the ambient owner.
const B677_THREE_OWNERS: &str = r#"
import std::io::print;
import std::reactive::{ Owner, get_owner, owner_scope, run_with_owner };

fun register(name: str) {
    print(i"+{name}");
    get_owner().defer(|| print(i"-{name}"));
}

fun while_on(provide: {provide}) {
    let owner = Owner::new();
    run_with_owner(owner, {argument});
    owner.dispose();
    print("disposed");
}

fun main() {
    let root = Owner::new();
    run_with_owner(root, || {
        while_on(|| register("a"));
    });
}
main();
"#;

#[test]
fn b677_a_closure_parameter_at_a_context_typed_parameter_is_told_its_clause() {
    // `run_with_owner(owner, provide)` with `provide: || void` was refused with
    // the generic landing sentence, whose FIRST choice — a closure literal —
    // is the trap (`|| provide()` compiles and runs `provide` under the owner
    // `while_on`'s caller was made with). The refusal names the parameter and
    // the fix, with the argument's own closure type, the landing's `sync`
    // marker and its clause; the literal wrap is listed last with why.
    assert_fails_with(
        &B677_THREE_OWNERS
            .replace("{provide}", "|| void")
            .replace("{argument}", "provide"),
        "`provide` is a closure parameter; to run it under this context, type it `(sync || void) context owner_scope`",
    );
    assert_fails_with(
        &B677_THREE_OWNERS
            .replace("{provide}", "|| void")
            .replace("{argument}", "provide"),
        "a closure called inside a literal keeps the context it was made in: `|| provide()` would run `provide` under the context the caller was made with",
    );
}

#[test]
fn b677_the_parameter_typed_with_the_clause_runs_under_the_owner_at_the_call() {
    // The fix the message names: with the clause, `provide` takes the owner at
    // the call, so `-a` prints when `while_on`'s owner is disposed — before
    // `disposed`. (With the literal wrap, `-a` never printed: the registration
    // belonged to `root`, which nothing disposes.)
    assert_compiles_and_runs(
        &B677_THREE_OWNERS
            .replace("{provide}", "(sync || void) context owner_scope")
            .replace("{argument}", "provide"),
        "+a\n-a\ndisposed\n",
    );
}

#[test]
fn b677_the_literal_wrap_of_a_plain_closure_parameter_is_warned() {
    // Door 2's control too: a `|| void` provider is outside the implied
    // clause (views only), so it still captures at creation.
    // The accepted-but-wrong form: `|| provide()` at `run_with_owner`. The
    // warning sits on the call and names the parameter, the body and the
    // clause that fixes it. The program still runs (and still never prints
    // `-a`: the warning is the only thing that says why).
    let source = B677_THREE_OWNERS
        .replace("{provide}", "|| void")
        .replace("{argument}", "|| provide()");
    assert_warns_spanning(
        &source,
        "provide()",
        "`provide` is a closure parameter called inside `run_with_owner`'s body, which establishes an owner: what it builds belongs to the owner it was made under, not this one, and is released with that owner; type it `(sync || void) context owner_scope` so it takes the owner at the call",
    );
    assert_compiles_and_runs(&source, "+a\ndisposed\n");
}

#[test]
fn b677_the_fixed_parameter_and_a_value_answering_parameter_are_not_warned() {
    // A parameter carrying the clause takes the owner at the call — the fixed
    // form, not warned — and a parameter answering a VALUE (`|i32| i32`) builds
    // nothing under an owner. A call to the plain parameter outside any owner
    // body is not warned either.
    let source = r#"
import std::reactive::{ Signal, SignalCell, owner_scope };
import std::web::ui::{ View, swap };

fun page(
    show: SignalCell<bool>,
    probe: (sync |i32| View) context owner_scope,
    count: |i32| i32,
    plain: |i32| View,
): View {
    let outside = plain(0);
    <div>
        {swap(show, |on| probe(count(if on { 1 } else { 0 })))}
        {outside}
    </div>
}

fun main() {}
main();
"#;
    let warnings = warning_diagnostics_with_std_on(source, std_spec(), Platform::Browser);
    assert!(
        !warnings
            .iter()
            .any(|(message, _)| message.contains("is a closure parameter called inside")),
        "no B677 warning: {warnings:#?}"
    );
}

// --- B677 door 2: a closure parameter answering a view takes the call owner --
//
// `closure-owner.md` §4, RULED 2026-11-02: a `fun`'s closure parameter whose
// result is `View` (or a `Slot`) carries `context owner_scope` by default, and
// `sync` with it — the clause is implied into the parameter's TYPE, so it
// lands, threads and prints as a written one does.

/// B677's three-owner program with a VIEW provider: `register` defers `-a` to
/// the ambient owner and answers a view.
const B677_VIEW_OWNERS: &str = r#"
import std::io::print;
import std::reactive::{ Owner, get_owner, owner_scope, run_with_owner };
import std::web::ui::{ View, view };

fun register(name: str): View {
    print(i"+{name}");
    get_owner().defer(|| print(i"-{name}"));
    view("div")
}

fun while_on(provide: {provide}) {
    let owner = Owner::new();
    let _shown = run_with_owner(owner, provide);
    owner.dispose();
    print("disposed");
}

fun main() {
    let root = Owner::new();
    run_with_owner(root, || {
        while_on(|| register("a"));
    });
}
main();
"#;

#[test]
fn b677_door_2_a_view_parameter_takes_the_owner_at_the_call_without_the_clause() {
    // Door 1 refused `run_with_owner(owner, provide)` for a plain `|| View`
    // and named the clause; the clause is now implied, so the parameter
    // forwards as written and what it builds belongs to `while_on`'s owner:
    // `-a` prints when that owner is disposed.
    assert_compiles_and_runs(
        &B677_VIEW_OWNERS.replace("{provide}", "|| View"),
        "+a\n-a\ndisposed\n",
    );
    // The written clause is accepted and adds nothing.
    assert_compiles_and_runs(
        &B677_VIEW_OWNERS.replace("{provide}", "(sync || View) context owner_scope"),
        "+a\n-a\ndisposed\n",
    );
}

#[test]
fn b677_door_2_a_view_parameter_in_owner_bodies_is_not_warned() {
    // Door 1's View half is gone with its cause: `probe` and `banner` take the
    // owner of the `swap`/`when`/`each` body that calls them, nested literal
    // or not, and a clause naming another context keeps it after the implied
    // one (kolt's `page: (|str| View) context page_focus`).
    let source = r#"
import std::context::Context;
import std::reactive::{ Signal, SignalCell };
import std::web::ui::{ View, each, swap, when };

let focus: Context<str> = Context::new();

fun framed(body: (|| View) context focus): View {
    focus.run("page", || body())
}

fun page(
    show: SignalCell<bool>,
    items: SignalCell<List<i32>>,
    path: SignalCell<str>,
    probe: |i32| View,
    banner: || View,
    shown: (|str| View) context focus,
): View {
    <div>
        {swap(show, |on| probe(if on { 1 } else { 0 }))}
        {when(show, || banner())}
        {each(items, |item| item, |item| probe(item))}
        {swap(path, |now| <div>{framed(|| shown(now))}</div>)}
    </div>
}

fun main() {}
main();
"#;
    let warnings = warning_diagnostics_with_std_on(source, std_spec(), Platform::Browser);
    assert!(
        !warnings
            .iter()
            .any(|(message, _)| message.contains("is a closure parameter called inside")),
        "no B677 warning: {warnings:#?}"
    );
}

#[test]
fn b677_door_2_a_stored_view_parameter_needs_a_field_typed_with_the_clause() {
    // The implied clause makes the parameter an INJECTED closure, with §8.5's
    // restrictions: a plain `|| View` field cannot hold it, and the refusal
    // prints the implied clause the way a written one prints. A field typed
    // with the clause holds it.
    let plain = r#"
import std::reactive::owner_scope;
import std::web::ui::{ View, view };

struct Held {
    body: || View,
}

fun keep(body: || View): Held {
    Held { body }
}

fun main() {
    let held = keep(|| view("div"));
}
main();
"#;
    assert_fails_with(
        plain,
        "an injected (`context`-typed) closure can only be called, forwarded to a position with the same `context` clause, or passed to `run`",
    );
    assert_compiles(&plain.replace(
        "    body: || View,\n",
        "    body: (|| View) context owner_scope,\n",
    ));
    // A diagnostic that prints the parameter's type prints the implied clause
    // exactly as a written one prints (it is in the type).
    assert_fails_with(
        "import std::web::ui::{ View, view };\n\nfun keep(body: || View): i32 {\n    let n: i32 = body;\n    n\n}\n\nfun main() {\n    print(keep(|| view(\"div\")));\n}\nmain();\n",
        "Expected i32, but got (|| View) context owner_scope instead.",
    );
}
