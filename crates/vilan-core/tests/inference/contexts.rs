//! The context-threading pass (`context.rs`): what the rewrite leaves in the
//! tree the emitters read. The diagnostics the pass reports are pinned where
//! their subject lives (`callable.rs`, `bounds.rs`, `macros.rs`); these pins
//! own the SHAPE of the rewrite, and the coverage answer where it turns on
//! the pass's own dispatch refinement (a generic body's bound dispatch,
//! resolved per entry).

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

// --- Coverage through a trait DEFAULT method's own dispatch sites. ---
//
// `Attrs::gap` is a generic default whose body dispatches on its parameter
// (`gap.bind(..)` under `V: MaybeSignal<Dp>`): the blanket (static) arm reads
// no owner, the reactive arm reads `owner_scope`. Which arm a call runs is a
// fact about that CALL's `V`, and coverage follows it per entry — for a
// default reached through trait dispatch exactly as for an inherent method
// reached by a direct call. The refinement used to stop at "the default is
// dispatch-reachable, so its entries cannot be enumerated" and charge the
// reactive arm's owner to every call, the static ones included.

/// The fixture: a trait with the generic default (`gap`), a default that
/// forwards to it (`padded`), a default that establishes its own owner
/// (`owned_gap`) and one that clears it (`cleared_gap`); `Row` inherits them
/// all, `Loud` overrides `gap` with a body that reads nothing; an inherent
/// twin of the cleared shape; a generic wrapper (`spaced`) and a bounded one
/// (`apply`). `body` is `main`'s.
fn default_method_program(body: &str) -> String {
    format!(
        r#"
import std::io::print;
import std::reactive::{{ MaybeSignal, Signal, SignalCell, Owner, run_with_owner, owner_scope }};

struct Dp {{ value: f64 }}
struct Row {{ id: i32 }}
struct Loud {{ id: i32 }}

trait Attrs {{
    fun id(self): i32;
    fun gap<V: MaybeSignal<Dp>>(self, own gap: V): Self {{
        let id = self.id();
        gap.bind(|value: Dp| print(i"gap {{id}} {{value.value}}"));
        self
    }}
    fun padded<V: MaybeSignal<Dp>>(self, own padding: V): Self {{
        self.gap(padding)
    }}
    fun owned_gap<V: MaybeSignal<Dp>>(self, own gap: V): Self {{
        let id = self.id();
        owner_scope.run(Owner::new(), || {{
            gap.bind(|value: Dp| print(i"owned {{id}} {{value.value}}"));
        }});
        self
    }}
    fun cleared_gap<V: MaybeSignal<Dp>>(self, own gap: V): Self {{
        let id = self.id();
        owner_scope.clear(|| {{
            gap.bind(|value: Dp| print(i"cleared {{id}} {{value.value}}"));
        }});
        self
    }}
}}

impl Row with Attrs {{
    fun id(self): i32 {{ self.id }}
}}

impl Row {{
    fun inherent_cleared<V: MaybeSignal<Dp>>(self, own gap: V): Self {{
        let id = self.id;
        owner_scope.clear(|| {{
            gap.bind(|value: Dp| print(i"cleared {{id}} {{value.value}}"));
        }});
        self
    }}
    fun inherent_owned<V: MaybeSignal<Dp>>(self, own gap: V): Self {{
        let id = self.id;
        owner_scope.run(Owner::new(), || {{
            gap.bind(|value: Dp| print(i"owned {{id}} {{value.value}}"));
        }});
        self
    }}
}}

impl Loud with Attrs {{
    fun id(self): i32 {{ self.id }}
    fun gap<V: MaybeSignal<Dp>>(self, own gap: V): Self {{
        print(i"loud {{self.id}}");
        self
    }}
}}

fun spaced<V: MaybeSignal<Dp>>(own row: Row, own gap: V): Row {{
    row.gap(gap)
}}

fun apply<T: Attrs>(own target: T, own cell: SignalCell<Dp>): T {{
    target.gap(cell)
}}

fun cell(): SignalCell<Dp> {{
    Signal::new(Dp {{ value = 1.0 }})
}}

fun main() {{
{body}
}}
main();
"#
    )
}

const UNCOVERED_OWNER: &str =
    "context `owner_scope` is read here, but this code can be reached without an enclosing `run`";

#[test]
fn a_default_methods_bound_dispatch_resolves_per_call_site() {
    // The report: the static call needs no owner, the reactive one has one.
    assert_compiles_and_runs(
        &default_method_program(
            "    let row = Row { id = 1 }.gap(Dp { value = 4.0 });
    run_with_owner(Owner::new(), || { row.gap(cell()); });",
        ),
        "gap 1 4\ngap 1 1\n",
    );
}

#[test]
fn a_default_method_called_with_a_reactive_argument_still_needs_an_owner() {
    // The other direction: per-site resolution must not lose the reactive
    // arm's read. One ownerless reactive call is refused, alone or beside a
    // static one.
    assert_fails_with(
        &default_method_program("    let row = Row { id = 1 }.gap(cell());"),
        UNCOVERED_OWNER,
    );
    assert_fails_with(
        &default_method_program(
            "    let row = Row { id = 1 }.gap(Dp { value = 4.0 });
    row.gap(cell());",
        ),
        UNCOVERED_OWNER,
    );
}

#[test]
fn a_wrapper_forwarding_into_a_default_method_resolves_at_the_wrappers_call() {
    // `spaced` hands its own `V` to `row.gap(..)`: the binding at that
    // dispatch site is `spaced`'s parameter, so the answer is each call of
    // `spaced`.
    assert_compiles_and_runs(
        &default_method_program("    let row = spaced(Row { id = 1 }, Dp { value = 4.0 });"),
        "gap 1 4\n",
    );
    assert_compiles_and_runs(
        &default_method_program(
            "    run_with_owner(Owner::new(), || { spaced(Row { id = 1 }, cell()); });",
        ),
        "gap 1 1\n",
    );
    assert_fails_with(
        &default_method_program("    let row = spaced(Row { id = 1 }, cell());"),
        UNCOVERED_OWNER,
    );
}

#[test]
fn a_default_calling_a_default_resolves_at_the_outer_call() {
    // `padded` calls `self.gap(padding)` — a receiver-less dispatch, entered
    // with `padded`'s own parameter — so the answer is each call of `padded`.
    assert_compiles_and_runs(
        &default_method_program(
            "    let row = Row { id = 1 }.padded(Dp { value = 4.0 });
    run_with_owner(Owner::new(), || { row.padded(cell()); });",
        ),
        "gap 1 4\ngap 1 1\n",
    );
    assert_fails_with(
        &default_method_program("    let row = Row { id = 1 }.padded(cell());"),
        UNCOVERED_OWNER,
    );
}

#[test]
fn a_receiverless_dispatch_resolves_by_its_defaults_self() {
    // `padded`'s `self.gap(..)` dispatches on the default's `Self`, and each
    // entry of `padded` records what that is (B742). `Loud` overrides `gap`
    // with a body that reads nothing, so `Loud { .. }.padded(cell())` never
    // runs the default `gap` whose reactive arm reads the owner. This pin
    // used to hold the opposite verdict, as the conservative posture of a
    // refinement that kept every candidate at a receiver-less site.
    assert_compiles_and_runs(
        &default_method_program("    let loud = Loud { id = 2 }.padded(cell());"),
        "loud 2\n",
    );
    assert_compiles_and_runs(
        &default_method_program("    let loud = Loud { id = 2 }.gap(cell());"),
        "loud 2\n",
    );
    // A receiver that inherits `gap` still reaches the reactive arm.
    assert_fails_with(
        &default_method_program("    let row = Row { id = 1 }.padded(cell());"),
        UNCOVERED_OWNER,
    );
}

#[test]
fn a_bounded_site_enters_only_the_members_its_instantiations_select() {
    // `apply<T: Attrs>` lists `Attrs::gap`'s default among `target.gap(..)`'s
    // candidates by name. Instantiated only with `Loud`, which overrides it,
    // the default is not entered there and its reactive arm is not charged
    // to `apply`; instantiated with `Row`, which inherits it, it is.
    assert_compiles_and_runs(
        &default_method_program("    let loud = apply(Loud { id = 2 }, cell());"),
        "loud 2\n",
    );
    assert_fails_with(
        &default_method_program("    let row = apply(Row { id = 2 }, cell());"),
        UNCOVERED_OWNER,
    );
    assert_fails_with(
        &default_method_program(
            "    let loud = apply(Loud { id = 2 }, cell());
    let row = apply(Row { id = 2 }, cell());",
        ),
        UNCOVERED_OWNER,
    );
}

#[test]
fn a_top_level_call_of_a_default_method_is_an_entry_beside_a_functions() {
    // A top-level statement owns no graph node; its call is an entry all the
    // same, and resolves like the one inside `main`.
    let program = default_method_program("    let other = Row { id = 2 }.gap(Dp { value = 5.0 });");
    assert_compiles(&program.replace(
        "\nfun main() {",
        "\nlet row = Row { id = 1 }.gap(Dp { value = 4.0 });\n\nfun main() {",
    ));
    assert_fails_with(
        &program.replace(
            "\nfun main() {",
            "\nlet row = Row { id = 1 }.gap(cell());\n\nfun main() {",
        ),
        UNCOVERED_OWNER,
    );
}

#[test]
fn a_generic_body_that_establishes_the_owner_covers_its_own_dispatch() {
    // The dispatch sits inside `owner_scope.run(..)` in the generic body, so
    // the reactive arm gets THAT owner whatever the caller holds. A per-entry
    // edge charged to the caller regardless refused this for every ownerless
    // caller — through a default and through an inherent method alike.
    assert_compiles_and_runs(
        &default_method_program("    let row = Row { id = 1 }.owned_gap(cell());"),
        "owned 1 1\n",
    );
    assert_compiles_and_runs(
        &default_method_program("    let row = Row { id = 1 }.inherent_owned(cell());"),
        "owned 1 1\n",
    );
}

#[test]
fn a_generic_body_that_clears_the_owner_uncovers_its_own_dispatch() {
    // And the mirror: inside `owner_scope.clear(..)` the reactive arm has no
    // owner even when the caller does. Charged to the (covered) caller, the
    // inherent form compiled and read an owner that was not there.
    let owned_call = |call: &str| {
        default_method_program(&format!(
            "    owner_scope.run(Owner::new(), || {{ Row {{ id = 1 }}.{call}; }});"
        ))
    };
    assert_fails_with(&owned_call("cleared_gap(cell())"), UNCOVERED_OWNER);
    assert_fails_with(&owned_call("inherent_cleared(cell())"), UNCOVERED_OWNER);
    // The static arm reads nothing, cleared or not.
    assert_compiles_and_runs(
        &owned_call("cleared_gap(Dp { value = 2.0 })"),
        "cleared 1 2\n",
    );
    assert_compiles_and_runs(
        &owned_call("inherent_cleared(Dp { value = 2.0 })"),
        "cleared 1 2\n",
    );
}

/// How many times the refinement fell back to the whole candidate list at
/// the `bind` site inside `gap`, with the pass's dispatch sites enumerated as
/// it enumerates them — minus, when `omit_entries` is set, the sites that
/// dispatch `gap` itself.
fn gap_bind_fallbacks(source: &str, omit_entries: bool) -> usize {
    use vilan_core::call_graph::{CallGraph, CallTarget, IndirectReason};
    use vilan_core::dispatch_refine::{
        self, DispatchSite, RefinedCaller, candidates_of, member_name_at,
    };

    let source = source.to_string();
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
            let graph = CallGraph::build(&program);
            let mut sites: Vec<DispatchSite> = Vec::new();
            let mut bind_sites = Vec::new();
            for node in graph.nodes() {
                for call in graph.calls_of(node.id()) {
                    if !matches!(
                        call.target,
                        CallTarget::Indirect(
                            IndirectReason::TraitDispatch | IndirectReason::GenericMember
                        )
                    ) {
                        continue;
                    }
                    let Some(name) = member_name_at(&program, call.call_id) else {
                        continue;
                    };
                    if omit_entries && name == "gap" {
                        continue;
                    }
                    if name == "bind"
                        && program
                            .functions
                            .get(&node.id())
                            .is_some_and(|function| function.name == "gap")
                    {
                        bind_sites.push(call.call_id);
                    }
                    sites.push(DispatchSite {
                        owner: RefinedCaller::Node(node.id()),
                        call: call.call_id,
                        candidates: candidates_of(
                            &program,
                            program.admitting_file(call.call_id),
                            name,
                        ),
                    });
                }
            }
            assert_eq!(bind_sites.len(), 1, "the fixture has one `bind` in `gap`");
            dispatch_refine::reset_dispatch_fallbacks();
            let _edges = dispatch_refine::refined_edges(&program, &graph, &sites);
            dispatch_refine::dispatch_fallbacks()
                .into_iter()
                .filter(|(call, _)| bind_sites.contains(call))
                .count()
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked")
}

#[test]
fn a_dispatch_the_consumer_did_not_list_keeps_the_fallback() {
    // The refinement's consumers own which calls are sites (the const-only
    // check drops const subtrees; this pass lists no `for` loop's `next`). A
    // dispatch left out still RUNS what it selects, so a member it can select
    // has an entry the refinement cannot charge: that level keeps every
    // candidate, where resolving it from the listed entries alone would draw
    // no edge for the unlisted one — an under-ask, in silence.
    let program = default_method_program(
        "    let row = Row { id = 1 }.gap(Dp { value = 4.0 });
    run_with_owner(Owner::new(), || { row.gap(cell()); });",
    );
    assert_eq!(
        gap_bind_fallbacks(&program, false),
        0,
        "every entry listed: the site resolves per entry, no fallback"
    );
    assert!(
        gap_bind_fallbacks(&program, true) > 0,
        "the `gap` dispatches unlisted: the site must keep every candidate"
    );
}

// --- B742: a trait default's `self` call resolves per entry of the default. ---
//
// `self.note()` inside a default dispatches on the default's `Self`, and every
// entry of the default says what that is: a concrete receiver, a bounded one
// (whose parameter the walk follows to the bounded function's own entries),
// another default's `Self`, or `self` handed on to a bounded generic. The
// refinement used to keep every candidate at such a site, so a strict impl
// that only covered code selects was charged to every uncovered entry.

/// A trait whose defaults reach a `note` that `Deep` reads strictly: through
/// `self` (`announce`), through another default (`herald`) and through a
/// bounded generic handed `self` (`hand_off`). `Quiet` overrides `announce`
/// and has a strict `note` nothing reaches. Forwarders one, two and three
/// levels deep, and a generic struct's method that forwards to the default.
fn self_note_program(body: &str) -> String {
    format!(
        "import std::context::Context;

let depth: Context<i32> = Context::new();

trait Note {{
\tfun note(self);

\tfun announce(self) {{
\t\tself.note();
\t}}

\tfun herald(self) {{
\t\tself.announce();
\t}}

\tfun hand_off(self) {{
\t\tannounced(self);
\t}}
}}

struct Plain {{}}

struct Deep {{}}

struct Quiet {{}}

impl Plain with Note {{
\tfun note(self) {{
\t\tprint(\"plain\");
\t}}
}}

impl Deep with Note {{
\tfun note(self) {{
\t\tprint(i\"deep at {{depth.get()}}\");
\t}}
}}

impl Quiet with Note {{
\tfun note(self) {{
\t\tprint(i\"quiet at {{depth.get()}}\");
\t}}

\tfun announce(self) {{
\t\tprint(\"quiet\");
\t}}
}}

fun announced<T: Note>(item: T) {{
\titem.announce();
}}

fun relayed<T: Note>(item: T) {{
\tannounced(item);
}}

fun forwarded<T: Note>(item: T) {{
\trelayed(item);
}}

struct Holder<T: Note> {{
\titem: T,
}}

impl Holder<type T: Note> {{
\tfun show(self) {{
\t\tself.item.announce();
\t}}
}}

fun main() {{
{body}
}}
"
    )
}

const UNCOVERED_DEPTH: &str =
    "context `depth` is read here, but this code can be reached without an enclosing `run`";

#[test]
fn b742_a_default_reached_through_a_bounded_generic_resolves_at_the_generics_call() {
    // The report: only the covered call selects `Deep`'s strict `note`.
    assert_compiles_and_runs(
        &self_note_program(
            "\tannounced(Plain {});
\tdepth.run(5, || announced(Deep {}));",
        ),
        "plain\ndeep at 5\n",
    );
    // The one-level form: the default called on a concrete receiver.
    assert_compiles_and_runs(
        &self_note_program(
            "\tPlain {}.announce();
\tdepth.run(5, || Deep {}.announce());",
        ),
        "plain\ndeep at 5\n",
    );
}

#[test]
fn b742_three_levels_of_forwarding_resolve_at_the_outermost_call() {
    assert_compiles_and_runs(
        &self_note_program(
            "\tforwarded(Plain {});
\tdepth.run(5, || forwarded(Deep {}));",
        ),
        "plain\ndeep at 5\n",
    );
}

#[test]
fn b742_a_default_calling_a_default_resolves_at_the_outer_call() {
    assert_compiles_and_runs(
        &self_note_program(
            "\tPlain {}.herald();
\tdepth.run(5, || Deep {}.herald());",
        ),
        "plain\ndeep at 5\n",
    );
}

#[test]
fn b742_a_default_handing_self_to_a_bounded_generic_resolves_at_the_defaults_call() {
    assert_compiles_and_runs(
        &self_note_program(
            "\tPlain {}.hand_off();
\tdepth.run(5, || Deep {}.hand_off());",
        ),
        "plain\ndeep at 5\n",
    );
}

#[test]
fn b742_a_generic_structs_method_forwarding_to_a_default_resolves_at_its_call() {
    assert_compiles_and_runs(
        &self_note_program(
            "\tHolder { item = Plain {} }.show();
\tdepth.run(5, || Holder { item = Deep {} }.show());",
        ),
        "plain\ndeep at 5\n",
    );
}

#[test]
fn b742_an_override_between_two_defaults_keeps_its_receiver_out() {
    // `Quiet` overrides `announce`: neither `herald`'s `self.announce()` nor
    // `announced`'s `item.announce()` runs the default for it, so the default's
    // `self.note()` never selects `Quiet`'s strict `note` — although the same
    // sites do run the default for `Deep`, under `run`.
    assert_compiles_and_runs(
        &self_note_program(
            "\tQuiet {}.herald();
\tannounced(Quiet {});
\tdepth.run(5, || Deep {}.herald());",
        ),
        "quiet\nquiet\ndeep at 5\n",
    );
}

#[test]
fn b742_a_top_level_entry_resolves_its_receiver() {
    // A top-level call enters the default directly; its dispatch record still
    // names the receiver.
    let program = self_note_program("\tdepth.run(5, || Deep {}.announce());");
    assert_compiles(&program.replace("\nfun main() {", "\nPlain {}.announce();\n\nfun main() {"));
    assert_fails_with(
        &program.replace("\nfun main() {", "\nDeep {}.announce();\n\nfun main() {"),
        UNCOVERED_DEPTH,
    );
}

#[test]
fn b742_the_strict_impl_reached_without_the_context_is_still_refused() {
    // Every shape above, with `Deep` reached from `main` and nothing around it.
    for body in [
        "\tannounced(Deep {});",
        "\tannounced(Plain {});\n\tannounced(Deep {});",
        "\tDeep {}.announce();",
        "\tforwarded(Deep {});",
        "\tDeep {}.herald();",
        "\tDeep {}.hand_off();",
        "\tHolder { item = Deep {} }.show();",
        "\tdepth.run(5, || Deep {}.herald());\n\tQuiet {}.herald();\n\tDeep {}.herald();",
    ] {
        assert_fails_with(&self_note_program(body), UNCOVERED_DEPTH);
    }
}

#[test]
fn b742_a_dyn_receiver_keeps_every_candidate() {
    // A call through a `dyn` object erases the receiver: the entry says
    // nothing about `Self`, so the default's `self.note()` keeps every
    // candidate and `Deep`'s strict read is charged to `main`, as before.
    assert_fails_with(
        &self_note_program(
            "\tlet noted: dyn Note = Plain {};
\tnoted.announce();
\tdepth.run(5, || Deep {}.announce());",
        ),
        UNCOVERED_DEPTH,
    );
}

// --- B741: a refusal's trace follows the refined edges. ---
//
// One generic body bound to a static value and to a signal: only the signal's
// call reaches the reactive arm's owner read. The trace climbed every call of
// the body, so the static calls were named on the path too. (A library body
// made each of them a refusal of its own: `macros::b741_*`.)

/// `shown` binds directly, `relayed` forwards to it, and `Badge::labelled`
/// is a trait default that binds; `calls` are `main`'s, with `cell` a signal.
fn shown_program(calls: &str) -> String {
    format!(
        "import std::reactive::{{MaybeSignal, Signal, SignalCell}};

fun shown<V: MaybeSignal<str>>(own value: V) {{
\tvalue.bind(|text: str| print(text));
}}

fun relayed<V: MaybeSignal<str>>(own value: V) {{
\tshown(value);
}}

trait Badge {{
\tfun labelled<V: MaybeSignal<str>>(self, own value: V) {{
\t\tvalue.bind(|text: str| print(text));
\t}}
}}

struct Tag {{}}

impl Tag with Badge {{}}

fun main() {{
\tlet cell: SignalCell<str> = Signal::new(\"signal\");
{calls}
}}
"
    )
}

#[test]
fn b741_a_generic_bodys_refusal_names_only_its_reactive_call() {
    assert_traces(
        &shown_program("\tshown(\"static\");\n\tshown(cell);\n\tshown(\"static again\");"),
        UNCOVERED_OWNER,
        &[("shown(cell)", 0, "flows through this call")],
    );
}

#[test]
fn b741_a_forwarding_generics_refusal_names_only_its_reactive_path() {
    assert_traces(
        &shown_program("\trelayed(\"static\");\n\trelayed(cell);\n\trelayed(\"static again\");"),
        UNCOVERED_OWNER,
        &[
            ("relayed(cell)", 0, "flows through this call"),
            ("shown(value)", 0, "flows through this call"),
        ],
    );
}

#[test]
fn b741_a_trait_defaults_refusal_names_only_its_reactive_call() {
    assert_traces(
        &shown_program(
            "\tTag {}.labelled(\"static\");\n\tTag {}.labelled(cell);\n\tTag {}.labelled(\"static again\");",
        ),
        UNCOVERED_OWNER,
        &[("labelled", 2, "dispatch may select a reader")],
    );
}

// --- M203: what the refinement's cost work must not move. ---
//
// The context pass settles a per-entry edge per context only where a node on
// its hand-off path can establish (or clear) some context; every other path
// joins the site-level edges once, before the per-context loop. The pins
// above hold the `run` and the `clear` body; these hold the other two
// establishing shapes, a DECLARING generic body and a DEFERRED literal.

/// The fixture plus `extra` declarations ahead of `main`.
fn default_method_program_with(extra: &str, body: &str) -> String {
    default_method_program(body).replace("\nfun main() {", &format!("\n{extra}\n\nfun main() {{"))
}

#[test]
fn m203_a_declaring_generic_body_takes_its_dispatch_on_its_own_clause() {
    // `declared_gap` declares `owner_scope`, so the reactive arm under its
    // `row.gap(..)` has the owner `declared_gap`'s callers must hand it: an
    // ownerless caller is refused ONCE, at the call, by the clause — never
    // also for the read inside, which charging the caller with the edge would
    // add.
    let declared =
        "fun declared_gap<V: MaybeSignal<Dp>>(own row: Row, own gap: V): Row context owner_scope {
    row.gap(gap)
}";
    assert_compiles_and_runs(
        &default_method_program_with(
            declared,
            "    run_with_owner(Owner::new(), || { declared_gap(Row { id = 1 }, cell()); });",
        ),
        "gap 1 1\n",
    );
    let ownerless = default_method_program_with(
        declared,
        "    let row = declared_gap(Row { id = 1 }, cell());",
    );
    assert_fails_once_with(
        &ownerless,
        "context `owner_scope` is required by `declared_gap`'s `context` clause",
    );
    assert_fails_without(
        &ownerless,
        "is read here, but this code can be reached without",
    );
}

#[test]
fn m203_a_deferred_literal_in_a_generic_body_takes_the_owner_it_is_handed() {
    // The `row.gap(..)` dispatch sits in a literal written at a `context
    // owner_scope` position, so it runs with whatever owner `with_owner`
    // hands that literal — here one `with_owner` makes — and not with
    // `deferred_gap`'s caller's, which has none.
    assert_compiles_and_runs(
        &default_method_program_with(
            "fun with_owner(body: (|| void) context owner_scope) {
    run_with_owner(Owner::new(), || body());
}

fun deferred_gap<V: MaybeSignal<Dp>>(own row: Row, own gap: V) {
    with_owner(|| { row.gap(gap); });
}",
            "    deferred_gap(Row { id = 1 }, cell());",
        ),
        "gap 1 1\n",
    );
}

#[test]
fn m203_the_impls_a_selection_reads_are_the_ones_a_scan_finds() {
    // `impl_members_for_bound`'s first filter, answered from an index built
    // once per program: an impl is read when it declares the member, when one
    // of its traits declares it, or when it lists one of the bound's traits.
    // Each of the three is a key of the index, and each is pinned here
    // against the scan it replaced, over std's impls and the fixture's.
    let source = default_method_program("    let row = Row { id = 1 }.gap(Dp { value = 4.0 });");
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            use vilan_core::dispatch_refine::{impls_a_selection_reads, impls_implementing};
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            assert!(errors.is_empty(), "expected a clean analysis");
            let program = program.expect("a program");
            let scan = |member: &str, traits: &[vilan_core::id::Id]| -> Vec<usize> {
                program
                    .implementations
                    .iter()
                    .enumerate()
                    .filter(|(_, implementation)| {
                        implementation.declarations.contains_key(member)
                            || implementation.trait_ids.iter().any(|trait_id| {
                                traits.contains(trait_id)
                                    || program.traits.get(trait_id).is_some_and(|trait_| {
                                        trait_.declarations.contains_key(member)
                                    })
                            })
                    })
                    .map(|(index, _)| index)
                    .collect()
            };
            let mut members: Vec<&str> = program
                .implementations
                .iter()
                .flat_map(|implementation| implementation.declarations.keys().copied())
                .chain(
                    program
                        .traits
                        .values()
                        .flat_map(|trait_| trait_.declarations.keys().copied()),
                )
                .collect();
            members.sort_unstable();
            members.dedup();
            assert!(members.contains(&"gap") && members.contains(&"bind"));
            // By member: an impl that declares it (`Loud::gap`), one that
            // inherits it from its trait (`Row`'s `gap`), both at once.
            for member in &members {
                assert_eq!(
                    impls_a_selection_reads(&program, member, &[]),
                    scan(member, &[]),
                    "the impls a selection of `{member}` reads"
                );
            }
            // By trait: every trait as a bound, with a member no impl or
            // trait declares (the trait's own impls alone) and with each
            // member the trait declares (both keys at once).
            for trait_ in program.traits.values() {
                let bound = [trait_.id];
                for member in std::iter::once("m203 no such member")
                    .chain(trait_.declarations.keys().copied())
                {
                    assert_eq!(
                        impls_a_selection_reads(&program, member, &bound),
                        scan(member, &bound),
                        "the impls a selection of `{member}` under `{}` reads",
                        trait_.name
                    );
                }
                let implementing: Vec<usize> = program
                    .implementations
                    .iter()
                    .enumerate()
                    .filter(|(_, implementation)| implementation.trait_ids.contains(&trait_.id))
                    .map(|(index, _)| index)
                    .collect();
                assert_eq!(
                    impls_implementing(&program, trait_.id),
                    implementing,
                    "the impls of `{}`",
                    trait_.name
                );
            }
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked");
}

#[test]
fn m203_a_function_called_only_with_the_wrong_arity_elsewhere_keeps_the_fallback() {
    // The refinement counts only a WIRED call's subject as call position, so
    // `spaced`, named once by a call refused for its arity, is a function
    // taken as a value there: its entries cannot be enumerated and the `bind`
    // site it reaches keeps every candidate. The context pass's own value-use
    // set reads that subject as a call (B241) — the two are answered by one
    // scan now (M203), and the refinement must keep its own answer. Read
    // through the fence's face: the pass stands down in a program that does
    // not type, after refining.
    let program = default_method_program(
        "    let row = spaced(Row { id = 1 }, Dp { value = 4.0 });
    let other = spaced(Row { id = 2 });",
    );
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            use vilan_core::call_graph::CallGraph;
            use vilan_core::dispatch_refine::{
                dispatch_fallbacks, member_name_at, reset_dispatch_fallbacks,
            };
            let leaked: &'static str = Box::leak(program.into_boxed_str());
            reset_dispatch_fallbacks();
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            let fallbacks = dispatch_fallbacks();
            assert!(
                errors
                    .iter()
                    .any(|error| error.msg.contains("expects 2 arguments, but got 1")),
                "the fixture's arity error: {:#?}",
                errors.iter().map(|error| &error.msg).collect::<Vec<_>>()
            );
            let program = program.expect("a program");
            let graph = CallGraph::build(&program);
            let bind_sites: Vec<_> = graph
                .nodes()
                .iter()
                .filter(|node| {
                    program
                        .functions
                        .get(&node.id())
                        .is_some_and(|function| function.name == "gap")
                })
                .flat_map(|node| graph.calls_of(node.id()))
                .filter(|call| member_name_at(&program, call.call_id) == Some("bind"))
                .map(|call| call.call_id)
                .collect();
            assert_eq!(bind_sites.len(), 1, "the fixture has one `bind` in `gap`");
            assert!(
                fallbacks.iter().any(|(call, _)| bind_sites.contains(call)),
                "`spaced` is taken as a value for the refinement: the `bind` site keeps every \
                 candidate"
            );
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked");
}

// --- B740: a hidden parameter's FLAVOUR is the function's, so it is bare only
// where every entry into the function has a value to hand. ---
//
// One strict candidate of a dispatch made the parameter strict for every
// instance of the generic and for the safe candidates beside it, and a caller
// with no context handed such a node nothing: JavaScript read `undefined`, and
// the safe read answered `Some(undefined)`. A node now holds the bare value
// only when it is strict AND covered; every other needy node holds the
// `Option` (`None` from a caller without the context), and a strict read in a
// covered `Option` holder unwraps.

/// The flavour of each hidden parameter of every function named `name`, in
/// source order: `true` for `Option<T>`, `false` for the bare value.
fn hidden_parameter_flavours(source: &str, name: &str) -> Vec<bool> {
    let source = source.to_string();
    let name = name.to_string();
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
            let mut functions: Vec<(usize, Vec<bool>)> = program
                .functions
                .iter()
                .filter(|(id, function)| {
                    function.name == name && program.span_map.contains_key(*id)
                })
                .map(|(id, function)| {
                    let flavours = function
                        .parameters
                        .iter()
                        .filter(|parameter| {
                            program.context_hidden_parameters.contains_key(*parameter)
                        })
                        .map(|parameter| {
                            program
                                .context_optional_hidden_parameters
                                .contains(parameter)
                        })
                        .collect();
                    (program.span_map[id].start, flavours)
                })
                .collect();
            functions.sort_unstable_by_key(|(start, _)| *start);
            functions
                .into_iter()
                .flat_map(|(_, flavours)| flavours)
                .collect()
        })
        .expect("spawn worker")
        .join()
        .expect("the analysis thread")
}

/// A trait with a SAFE reader (`str`) and a STRICT one (`i32`) of a user
/// context, a generic dispatching through it, and `main` writing `body`.
fn depth_notes(body: &str) -> String {
    format!(
        "import std::context::Context;

let depth_context: Context<i32> = Context::new();

trait Note {{
\tfun note(self);
}}

impl str with Note {{
\tfun note(self) {{
\t\tmatch depth_context.get_safe() {{
\t\t\tSome(let depth) => print(i\"{{self}} at depth {{depth}}\"),
\t\t\tNone => print(i\"{{self}} without depth\"),
\t\t}}
\t}}
}}

impl i32 with Note {{
\tfun note(self) {{
\t\tprint(i\"{{self}} at depth {{depth_context.get()}}\");
\t}}
}}

fun noted<V: Note>(value: V) {{
\tvalue.note();
}}

fun main() {{
{body}
}}
"
    )
}

#[test]
fn b740_a_safe_read_behind_a_strict_candidate_answers_none_without_the_context() {
    assert_compiles_and_runs(
        &depth_notes(
            "    noted(\"top\");
    depth_context.run(1, || {
        noted(\"inside\");
        noted(2);
    });",
        ),
        "top without depth\ninside at depth 1\n2 at depth 1\n",
    );
}

#[test]
fn b740_a_safe_candidate_called_directly_without_the_context_answers_none() {
    // No generic instance is reached without the context here: the safe
    // candidate itself is, by a direct call from `main`. It is a candidate of
    // the mixed site in `noted` all the same.
    assert_compiles_and_runs(
        &depth_notes(
            "    \"direct\".note();
    depth_context.run(3, || {
        \"direct inside\".note();
        noted(4);
    });",
        ),
        "direct without depth\ndirect inside at depth 3\n4 at depth 3\n",
    );
}

#[test]
fn b740_a_generic_reached_without_the_context_holds_an_option() {
    // `noted` is reached from `main`: its parameter, and with it every
    // needy candidate of its site, holds the `Option` — the strict `i32` one
    // too, which is covered (only `run` reaches its instance) and unwraps.
    let source = depth_notes(
        "    noted(\"top\");
    depth_context.run(1, || noted(2));",
    );
    assert_eq!(hidden_parameter_flavours(&source, "noted"), vec![true]);
    assert_eq!(hidden_parameter_flavours(&source, "note"), vec![true, true]);
}

#[test]
fn b740_a_generic_every_entry_of_which_holds_the_context_stays_bare() {
    // The shape the rule must not move: every call of `noted` sits under
    // `run`, so `noted` and the site's candidates are covered and keep the
    // bare value — the safe `str` one included (a mixed site promotes it).
    let source = depth_notes(
        "    depth_context.run(1, || {
        noted(\"inside\");
        noted(2);
    });",
    );
    assert_eq!(hidden_parameter_flavours(&source, "noted"), vec![false]);
    assert_eq!(
        hidden_parameter_flavours(&source, "note"),
        vec![false, false]
    );
}

#[test]
fn b740_a_demoted_holder_unwraps_for_a_bare_callee_and_an_injected_call() {
    // The strict candidate is demoted to the `Option` by the site in `noted`;
    // what it hands a strict helper (a direct call) and the closure it was
    // given (an injected call) is the unwrapped value.
    assert_compiles_and_runs(
        "import std::context::Context;

let depth_context: Context<i32> = Context::new();

trait Note {
\tfun note(self, body: (sync || void) context depth_context);
}

impl str with Note {
\tfun note(self, body: (sync || void) context depth_context) {
\t\tmatch depth_context.get_safe() {
\t\t\tSome(let depth) => print(i\"{self} at depth {depth}\"),
\t\t\tNone => print(i\"{self} without depth\"),
\t\t}
\t}
}

fun helper(value: i32) {
\tprint(i\"{value} helped at depth {depth_context.get()}\");
}

impl i32 with Note {
\tfun note(self, body: (sync || void) context depth_context) {
\t\thelper(self);
\t\tbody();
\t}
}

fun noted<V: Note>(value: V) {
\tvalue.note(|| print(i\"body at depth {depth_context.get()}\"));
}

fun main() {
\tnoted(\"top\");
\tdepth_context.run(1, || {
\t\tnoted(\"inside\");
\t\tnoted(2);
\t});
}
",
        "top without depth\ninside at depth 1\n2 helped at depth 1\nbody at depth 1\n",
    );
}
