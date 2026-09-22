//! `lazy` — defer to first demand (`proposal/lazy.md`).
//!
//! One subject module of the `inference` test binary; the harness it is written
//! against lives in `support.rs`.
//!
//! The pins here are the paper's own claims, each read off behaviour rather
//! than off the emitted text where behaviour can say it: a counting side effect
//! proves the memo (§1's "at most once, late"), its ABSENCE proves that a
//! parameter never read never runs, and the forwarding chain proves one memo
//! however deep it goes. The emission pins are the two that behaviour cannot
//! distinguish — a forward that re-wrapped would still print the same thing.

use crate::support::*;

// --- §1, the memo ---------------------------------------------------------

/// "an argument never runs twice, so a side effect can at most happen once,
/// late" — TWO reads of the parameter, ONE evaluation of the argument.
#[test]
fn a_lazy_argument_is_forced_once_however_often_it_is_read() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        mut built = 0;

        fun describe(): str {
            built += 1;
            "value"
        }

        fun twice(lazy message: str): i32 {
            print(message);
            print(message);
            0
        }

        fun main() {
            twice(i"{describe()}");
            print(i"built {built}");
        }
        "#,
        "value\nvalue\nbuilt 1\n",
    );
}

/// "A parameter never read never runs — that is the point (`expect`'s happy
/// path)." The counter stays at zero.
#[test]
fn a_lazy_argument_that_is_never_read_never_runs() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        mut built = 0;

        fun describe(): str {
            built += 1;
            "value"
        }

        fun expect_positive(value: i32, lazy complaint: str): i32 {
            if value > 0 {
                ret value;
            }
            print(complaint);
            0
        }

        fun main() {
            let kept = expect_positive(7, i"{describe()}");
            print(i"{kept}");
            print(i"built {built}");
        }
        "#,
        "7\nbuilt 0\n",
    );
}

/// The forcing point is the FIRST READ, not the call: the callee's own prints
/// bracket it, so the order says which.
#[test]
fn a_lazy_argument_forces_at_the_first_read_not_at_the_call() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun describe(): str {
            print("evaluating");
            "value"
        }

        fun bracket(lazy message: str): i32 {
            print("before");
            print(message);
            print("after");
            0
        }

        fun main() {
            print("calling");
            bracket(i"{describe()}");
        }
        "#,
        "calling\nbefore\nevaluating\nvalue\nafter\n",
    );
}

// --- §1, forwarding -------------------------------------------------------

/// "passing a lazy parameter onward to another lazy position forwards the thunk
/// (one memo, however deep the chain)". Three hops, one evaluation — and none
/// at all when the innermost callee does not read.
#[test]
fn a_forwarding_chain_shares_one_memo() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        mut built = 0;

        fun describe(): str {
            built += 1;
            "value"
        }

        fun inner(lazy message: str, read: bool): i32 {
            if read {
                print(message);
                print(message);
            }
            0
        }

        fun middle(lazy message: str, read: bool): i32 {
            inner(message, read)
        }

        fun outer(lazy message: str, read: bool): i32 {
            middle(message, read)
        }

        fun main() {
            outer(i"{describe()}", false);
            print(i"built {built}");
            outer(i"{describe()}", true);
            print(i"built {built}");
        }
        "#,
        "built 0\nvalue\nvalue\nbuilt 1\n",
    );
}

/// "passing it to an eager position forces it there" — the same reference, in
/// the other kind of position, runs the thunk.
#[test]
fn forwarding_a_lazy_parameter_to_an_eager_position_forces_it() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        mut built = 0;

        fun describe(): str {
            built += 1;
            "value"
        }

        fun eager(message: str): i32 {
            print(message);
            0
        }

        fun forces(lazy message: str): i32 {
            eager(message)
        }

        fun main() {
            forces(i"{describe()}");
            print(i"built {built}");
        }
        "#,
        "value\nbuilt 1\n",
    );
}

/// The emission half of the two pins above, which behaviour cannot tell apart:
/// a forward passes the CELL (no `__force`, no second `__lazy`), and the eager
/// position forces.
///
/// **M81 edited the fixture and not the claim.** It used to call `middle("a")`
/// and `forces("b")`, and a string LITERAL in a lazy position is now lowered
/// eagerly — so the fixture pinned three shapes that its own arguments had
/// elided away. `built()` is a call, which is the argument a thunk exists for,
/// and every assertion below is the one this test always made. The elided
/// forms have their own pins under §M81 further down.
#[test]
fn a_forward_emits_the_cell_and_an_eager_position_emits_a_force() {
    let source = r#"
        import std::io::print;

        fun built(): str {
            "a"
        }

        fun inner(lazy message: str): i32 {
            print(message);
            0
        }

        fun middle(lazy message: str): i32 {
            inner(message)
        }

        fun eager(message: str): i32 {
            print(message);
            0
        }

        fun forces(lazy message: str): i32 {
            eager(message)
        }

        fun main() {
            middle(built());
            forces(built());
        }
        "#;
    // The hop re-wraps nothing and forces nothing.
    assert_emits_containing(
        source,
        "function middle(message) {\n\treturn inner(message);\n}",
    );
    // The eager position forces.
    assert_emits_containing(
        source,
        "function forces(message) {\n\treturn eager(__force(message));\n}",
    );
    // And the call site built exactly one cell per lazy argument.
    assert_emits_containing(source, "__lazy(\"message\", () => {");
}

// --- §1, the three v1 restrictions ----------------------------------------

/// "Data only. … Resources stay eager." — at the DECLARATION, which is wrong
/// whether or not anyone calls it.
#[test]
fn a_lazy_parameter_of_resource_type_is_refused() {
    assert_fails_with(
        r#"
        import std::io::print;
        import std::drop::Drop;

        resource struct Res { tag: str }

        impl Res with Drop {
            fun drop(&mut self) {
                print(self.tag);
            }
        }

        fun takes(lazy handle: Res): i32 {
            0
        }

        fun main() {
            print("x");
        }
        "#,
        "is a `lazy` parameter of the resource type `Res`",
    );
}

/// "a resource local is an R9 capture (rejected)" — the thunk is a closure, so
/// R9 refuses it in R9's own words. No new refusal, deliberately.
#[test]
fn a_lazy_argument_that_names_a_resource_local_is_an_r9_capture() {
    assert_fails_with(
        r#"
        import std::io::print;
        import std::drop::Drop;

        resource struct Res { tag: str }

        impl Res with Drop {
            fun drop(&mut self) {
                print(self.tag);
            }
        }

        fun describe(res: &Res): str {
            res.tag
        }

        fun message(lazy text: str): i32 {
            print(text);
            0
        }

        fun main() {
            let res = Res { tag = "one" };
            message(i"{describe(&res)}");
        }
        "#,
        "a closure cannot capture the resource `res`",
    );
}

/// "a view in the expression is a view capture (rejected)" — rule 3's ban, in
/// rule 3's own words, for the same reason R9 lends the pin above its own: the
/// thunk IS a closure, and every existing capture rule applies to it unchanged.
#[test]
fn a_lazy_argument_that_names_a_view_binding_is_a_view_capture() {
    assert_fails_with(
        r#"
        import std::io::print;

        struct Holder { label: str }

        fun message(lazy text: str): i32 {
            print(text);
            0
        }

        fun main() {
            let holder = Holder { label = "a" };
            let seen = &holder;
            message(i"{seen.label}");
        }
        "#,
        "a closure cannot capture the view 'seen'",
    );
}

/// "Sync only." — the `await` the author wrote.
#[test]
fn an_awaiting_lazy_argument_is_refused_with_the_task_steer() {
    assert_fails_with(
        r#"
        import std::io::print;
        import std::time::sleep;

        async fun slow(): str {
            sleep(1);
            "slow"
        }

        fun message(lazy text: str): i32 {
            print(text);
            0
        }

        async fun main() {
            message(i"{await slow()}");
        }
        "#,
        "write `async <expr>` and pass the `Task`",
    );
}

/// The same rule against the IMPLICIT suspension: a call to an async callee is
/// awaited without a token, and E3 counts both spellings the same way.
#[test]
fn a_lazy_argument_calling_an_async_function_is_refused() {
    assert_fails_with(
        r#"
        import std::io::print;
        import std::time::sleep;

        async fun slow(): str {
            sleep(1);
            "slow"
        }

        fun message(lazy text: str): i32 {
            print(text);
            0
        }

        async fun main() {
            message(i"{slow()}");
        }
        "#,
        "this argument suspends",
    );
}

/// "Context-free. The thunk forces inside the callee, where the call site's
/// ambient contexts may be gone."
#[test]
fn a_context_reading_lazy_argument_is_refused_with_the_closure_steer() {
    assert_fails_with(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };

        fun bump(cell: SignalCell<i32>): str {
            cell.set(cell.get() + 1);
            "bumped"
        }

        fun message(lazy text: str): i32 {
            print(text);
            0
        }

        fun main() {
            let cell: SignalCell<i32> = Signal::new(1);
            message(i"{bump(cell)}");
        }
        "#,
        "this argument requires an ambient context",
    );
}

// --- §1, the trait rule ---------------------------------------------------

/// "a `lazy` parameter is part of the signature; impls must match."
#[test]
fn an_impl_that_drops_the_traits_lazy_is_refused() {
    assert_fails_with(
        r#"
        import std::io::print;

        trait Complainer {
            fun complain(self, lazy message: str): str;
        }

        struct Quiet { tag: str }

        impl Quiet with Complainer {
            fun complain(self, message: str): str {
                self.tag
            }
        }

        fun main() {
            print(Quiet { tag = "q" }.complain("hi"));
        }
        "#,
        "is eager, but `Complainer` declares it `lazy`",
    );
}

/// And the other direction: an impl cannot make a parameter lazy that the trait
/// declared eager — the caller's codegen is the declaration's, not the impl's.
#[test]
fn an_impl_that_adds_a_lazy_the_trait_did_not_declare_is_refused() {
    assert_fails_with(
        r#"
        import std::io::print;

        trait Complainer {
            fun complain(self, message: str): str;
        }

        struct Quiet { tag: str }

        impl Quiet with Complainer {
            fun complain(self, lazy message: str): str {
                self.tag
            }
        }

        fun main() {
            print(Quiet { tag = "q" }.complain("hi"));
        }
        "#,
        "is `lazy`, but `Complainer` declares it eager",
    );
}

/// An impl that AGREES compiles, and the laziness is the trait's: the argument
/// is deferred through the dispatch.
#[test]
fn an_impl_that_agrees_keeps_the_laziness_through_dispatch() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        mut built = 0;

        fun describe(): str {
            built += 1;
            "value"
        }

        trait Complainer {
            fun complain(self, lazy message: str): str;
        }

        struct Quiet { tag: str }

        impl Quiet with Complainer {
            fun complain(self, lazy message: str): str {
                self.tag
            }
        }

        struct Loud { tag: str }

        impl Loud with Complainer {
            fun complain(self, lazy message: str): str {
                message
            }
        }

        fun main() {
            print(Quiet { tag = "q" }.complain(i"{describe()}"));
            print(i"built {built}");
            print(Loud { tag = "l" }.complain(i"{describe()}"));
            print(i"built {built}");
        }
        "#,
        "q\nbuilt 0\nvalue\nbuilt 1\n",
    );
}

// --- §1, the three homes and what `lazy` composes with --------------------

/// The three homes the paper names, in one program: a free `fun`, an `impl`
/// method, and a `trait` signature with a default body.
#[test]
fn lazy_is_accepted_in_all_three_grammar_homes() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun free(lazy message: str): str {
            message
        }

        trait Named {
            fun name(self, lazy fallback: str): str {
                fallback
            }
        }

        struct Thing { label: str }

        impl Thing {
            fun inherent(self, lazy message: str): str {
                message + self.label
            }
        }

        impl Thing with Named {}

        fun main() {
            print(free("a"));
            print(Thing { label = "!" }.inherent("b"));
            print(Thing { label = "!" }.name("c"));
        }
        "#,
        "a\nb!\nc\n",
    );
}

#[test]
fn lazy_does_not_combine_with_own_or_a_view() {
    assert_fails_with(
        "fun f(lazy own message: str): i32 {\n\t0\n}\n\nfun main() {\n\tf(\"a\");\n}\n",
        "there is nothing for `own` or a view (`&`, `&mut`) to transfer or alias",
    );
    assert_fails_with(
        "fun f(lazy message: &str): i32 {\n\t0\n}\n\nfun main() {\n\tf(\"a\");\n}\n",
        "there is nothing for `own` or a view (`&`, `&mut`) to transfer or alias",
    );
}

#[test]
fn lazy_does_not_combine_with_mut() {
    assert_fails_with(
        "fun f(lazy mut message: str): i32 {\n\t0\n}\n\nfun main() {\n\tf(\"a\");\n}\n",
        "there is no by-value copy for `mut` to make writable",
    );
}

#[test]
fn lazy_does_not_combine_with_a_spread() {
    assert_fails_with(
        "fun f<T: (..)>(lazy ...items: T): i32 {\n\t0\n}\n\nfun main() {\n\tf(1, 2);\n}\n",
        "`lazy` defers ONE expression",
    );
}

#[test]
fn a_closure_cannot_take_a_lazy_parameter() {
    assert_fails_with(
        "fun main() {\n\tlet f = |lazy message: str| 0;\n\tf(\"a\");\n}\n",
        "a closure cannot take a `lazy` parameter",
    );
}

#[test]
fn an_external_fun_cannot_take_a_lazy_parameter() {
    assert_fails_with(
        "external fun host(lazy message: str): i32;\n\nfun main() {\n\thost(\"a\");\n}\n",
        "nothing on that side forces a thunk",
    );
}

// --- §5, the interpreter arm ----------------------------------------------

/// "The helper needs its interpreter arm in the same commit (the equivalence
/// gate)." A macro body runs in the native evaluator, so a lazy call INSIDE one
/// exercises `__lazy`/`__force` there and nowhere else — and it must answer the
/// same way: forced once, never-read never run.
#[test]
fn the_force_helper_has_an_interpreter_arm() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        macro fun tagged(item: Item): Source {
            import macro_std::source;
            import macro_std::meta::{ Item, Source, StructItem };
            import macro_std::option::Option::{ self, Some, None };
            import macro_std::build::{ impl_of, fun_of, quote };

            fun pick(use_it: bool, lazy label: str): str {
                if use_it {
                    ret label + "/" + label;
                }
                "none"
            }

            let target = match item.as_struct() {
                Some(let found) => found,
                None => StructItem { name = "?", fields = [], generics = [] },
            };
            let chosen = pick(true, target.name + "!");
            let skipped = pick(false, target.name + "?");
            let reporter = fun_of("report")
                .parameter("self")
                .returns("str")
                .expr(quote("\"" + chosen + " " + skipped + "\""));
            source(impl_of(target.name).member(reporter.render()).render())
        }

        [tagged]
        struct Pack {
            count: i32,
        }

        fun main() {
            print(Pack { count = 2 }.report());
        }
        "#,
        "\"Pack!/Pack! none\"\n",
    );
}

// --- §2, lazy module bindings ---------------------------------------------

/// "The initializer runs at the binding's FIRST USE instead of module load,
/// then memoizes." Both halves are observable in one program: the print order
/// says when, and the single "opening" says how often.
#[test]
fn a_lazy_module_binding_initializes_at_its_first_use_and_memoizes() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun open_db(tag: str): str {
            print(i"opening {tag}");
            tag
        }

        lazy let database: str = open_db("kolt.db");

        fun main() {
            print("start");
            print(database);
            print(database);
            print("done");
        }
        "#,
        "start\nopening kolt.db\nkolt.db\nkolt.db\ndone\n",
    );
}

/// An EAGER module binding beside it, to say what the difference is: the eager
/// one runs before `main` does, the lazy one after `main` asks for it.
#[test]
fn an_eager_module_binding_still_initializes_at_load() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun announce(tag: str): str {
            print(i"opening {tag}");
            tag
        }

        let eager: str = announce("eager");
        lazy let deferred: str = announce("deferred");

        fun main() {
            print("start");
            print(eager);
            print(deferred);
        }
        "#,
        "opening eager\nstart\neager\nopening deferred\ndeferred\n",
    );
}

/// A lazy binding nothing uses runs nothing — the §2 half of `expect`'s happy
/// path, and the reason the declaration may not be emitted for its effects.
#[test]
fn a_lazy_module_binding_nothing_uses_never_initializes() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun announce(): str {
            print("opening");
            "value"
        }

        lazy let unused: str = announce();

        fun main() {
            print("start");
        }
        "#,
        "start\n",
    );
}

/// "an initializer that (transitively) touches its own binding traps with a
/// clear message … via an in-progress flag — not a silent hang."
#[test]
fn a_lazy_initialization_cycle_traps_by_name() {
    assert_run_panics(
        r#"
        import std::io::print;

        fun build(): str {
            i"via {database}"
        }

        lazy let database: str = build();

        fun main() {
            print(database);
        }
        "#,
        "lazy initialization cycle: `database`",
    );
}

/// §6a, the user's call: "A failed initializer poisons the binding … later
/// touches re-panic with the poisoned message." The FIRST touch propagates the
/// author's own panic, so both are observable in one run.
#[test]
fn a_failed_lazy_initializer_poisons_the_binding() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::io::panic;
        import std::reactive::guarded;
        import std::option::Option::{ self, Some, None };

        fun build(): str {
            panic("no database")
        }

        lazy let database: str = build();

        fun touch(): str {
            database
        }

        fun main() {
            match guarded(|| print(touch())) {
                Some(let message) => print(i"first: {message}"),
                None => print("first: clean"),
            }
            match guarded(|| print(touch())) {
                Some(let message) => print(i"second: {message}"),
                None => print("second: clean"),
            }
        }
        "#,
        "first: no database\n\
         second: lazy `database` is poisoned: its initializer panicked: no database\n",
    );
}

/// The emission: the declaration builds the cell and nothing else, and the read
/// forces it. Behaviour proves the timing; this proves the shape the timing
/// rests on.
#[test]
fn a_lazy_module_binding_emits_a_cell_and_its_reads_force() {
    let source = r#"
        import std::io::print;

        fun open_db(): str {
            "kolt.db"
        }

        lazy let database: str = open_db();

        fun main() {
            print(database);
        }
        "#;
    assert_emits_containing(source, "const database = __lazy(\"database\", () => {");
    assert_emits_containing(source, "__force(database)");
}

/// A module binding handed to a lazy PARAMETER is a forward, not a second cell:
/// the two positions share one lowering, so a cell travels as a cell.
#[test]
fn a_lazy_module_binding_forwards_into_a_lazy_parameter() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun announce(): str {
            print("opening");
            "value"
        }

        lazy let database: str = announce();

        fun show(lazy message: str, read: bool): i32 {
            if read {
                print(message);
            }
            0
        }

        fun main() {
            show(database, false);
            print("nothing yet");
            show(database, true);
        }
        "#,
        "nothing yet\nopening\nvalue\n",
    );
}

/// §2: "The initializer is sync and context-free — first touch can happen
/// anywhere, so the deferred code must be self-contained."
#[test]
fn a_context_reading_lazy_initializer_is_refused() {
    assert_fails_with(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };

        let cell: SignalCell<i32> = Signal::new(1);

        fun bump(): str {
            cell.set(cell.get() + 1);
            "bumped"
        }

        lazy let label: str = bump();

        fun main() {
            print(label);
        }
        "#,
        "this initializer requires an ambient context",
    );
}

/// The sync half is the rule EVERY module initializer obeys, lazy or not, and
/// it stays one diagnostic: `lazy` adds no second sentence about the same
/// mistake (B5).
#[test]
fn an_async_lazy_initializer_keeps_the_one_module_initializer_diagnostic() {
    assert_fails_once_with(
        r#"
        import std::io::print;
        import std::fs::read_file_to_str;

        lazy let config: str = read_file_to_str("config.txt");

        fun main() {
            print(config);
        }
        "#,
        "a module-level binding cannot await",
    );
}

/// "platform coloring flows from the initializer exactly as global init colors
/// today" — the reachability path names the binding, so the fence is the same
/// one an eager binding would have earned.
#[test]
fn a_lazy_initializer_colors_its_binding() {
    assert_fails_browser_with(
        r#"
        import std::io::print;
        import std::fs::read_file_to_str;

        lazy let config: str = read_file_to_str("config.txt");

        fun main() {
            print(config);
        }
        "#,
        "requires the `process` layer of `std` and cannot run on `browser`",
    );
}

// --- §3, what a lazy binding is not ---------------------------------------

/// "Lazy local `let` … excluded for symmetry" (§3): an end-of-scope drop would
/// need a runtime was-it-initialized flag, and drop flags are ratified out.
#[test]
fn a_lazy_local_binding_is_refused() {
    assert_fails_with(
        r#"
        import std::io::print;

        fun body() {
            lazy let inner: i32 = 1;
            print(i"{inner}");
        }

        fun main() {
            body();
        }
        "#,
        "is a `lazy` binding inside a body",
    );
}

#[test]
fn a_lazy_binding_is_let_not_mut() {
    assert_fails_with(
        "lazy mut count: i32 = 1;\n\nfun main() {\n\tprint(i\"{count}\");\n}\n",
        "it is `lazy let`; `mut` names a slot anything may rewrite",
    );
}

#[test]
fn a_lazy_binding_declares_its_initializer() {
    assert_fails_with(
        "lazy let count: i32;\n\nfun main() {\n\tprint(i\"{count}\");\n}\n",
        "a lazy binding declares the initializer it defers",
    );
}

#[test]
fn a_lazy_binding_binds_one_name() {
    assert_fails_with(
        "lazy let (a, b): (i32, i32) = (1, 2);\n\nfun main() {\n\tprint(i\"{a}\");\n}\n",
        "a lazy binding binds ONE name to one memo cell",
    );
}

#[test]
fn a_bare_lazy_says_what_the_form_is() {
    assert_fails_with(
        "lazy count = 1;\n\nfun main() {\n\tprint(i\"{count}\");\n}\n",
        "a lazy binding is `lazy let name: T = <initializer>;`",
    );
}

// --- §6b, S3: the std retrofit (A103, completed by A109) ------------------
//
// Five members take a lazy argument: `Option::expect`, `Option::unwrap_or`,
// `Result::expect`, `Result::unwrap_or` and `Result::expect_err` (A109 — the
// one the first pass left behind). Each pin reads the retrofit off
// BEHAVIOUR — a counting side effect in the argument position, which the eager
// spelling ran on the happy path and the lazy one does not — because that is
// the whole of what §6b changed. `unwrap_or_else` stays the explicit form and
// is pinned unchanged beside them.

/// §6b: "`opt.unwrap_or(expensive())` runs `expensive()` on the `Some` path
/// too" — no longer. The `Some` path leaves the counter at zero; the `None`
/// path runs it exactly once.
#[test]
fn option_unwrap_or_defers_its_fallback_to_the_none_path() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        mut built = 0;

        fun expensive(): i32 {
            built += 1;
            99
        }

        fun main() {
            let present: Option<i32> = Some(7);
            print(present.unwrap_or(expensive()));
            print(i"built {built}");
            let absent: Option<i32> = None;
            print(absent.unwrap_or(expensive()));
            print(i"built {built}");
        }
        "#,
        "7\nbuilt 0\n99\nbuilt 1\n",
    );
}

/// §6b: "New `expect(self, lazy message: str)` lands alongside" — `Option` had
/// no `expect` at all before the retrofit, and the one it has builds no message
/// on the `Some` path.
#[test]
fn option_expect_builds_its_message_only_on_the_none_path() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        mut built = 0;

        fun why(): str {
            built += 1;
            "no row"
        }

        fun main() {
            let present: Option<i32> = Some(7);
            print(present.expect(i"{why()}"));
            print(i"built {built}");
        }
        "#,
        "7\nbuilt 0\n",
    );
}

/// The `None` path forces the message and panics with the author's own text —
/// the message is a real `str` by the time `panic` sees it, not a cell.
#[test]
fn option_expect_panics_with_the_message_it_was_given() {
    assert_run_panics(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        fun main() {
            let name = "ada";
            let absent: Option<i32> = None;
            print(absent.expect(i"no row for {name}"));
        }
        "#,
        "no row for ada",
    );
}

/// `Result`'s twin of `unwrap_or`: the `Ok` path runs no fallback.
#[test]
fn result_unwrap_or_defers_its_fallback_to_the_err_path() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::result::Result::{ self, Ok, Err };

        mut built = 0;

        fun expensive(): i32 {
            built += 1;
            99
        }

        fun main() {
            let good: Result<i32, str> = Ok(7);
            print(good.unwrap_or(expensive()));
            print(i"built {built}");
            let bad: Result<i32, str> = Err("boom");
            print(bad.unwrap_or(expensive()));
            print(i"built {built}");
        }
        "#,
        "7\nbuilt 0\n99\nbuilt 1\n",
    );
}

/// `Result`'s twin of `expect`: the `Ok` path builds no message.
#[test]
fn result_expect_builds_its_message_only_on_the_err_path() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::result::Result::{ self, Ok, Err };

        mut built = 0;

        fun why(): str {
            built += 1;
            "no row"
        }

        fun main() {
            let good: Result<i32, str> = Ok(7);
            print(good.expect(i"{why()}"));
            print(i"built {built}");
        }
        "#,
        "7\nbuilt 0\n",
    );
}

/// A109 — `expect_err`'s mirror image, and the member the first pass missed:
/// the same position, the same argument shape, and until now the only one of
/// the five that evaluated it eagerly. The panicking path is the `Ok` one, so
/// the message is built there and nowhere else: an `Err` leaves the counter at
/// zero.
#[test]
fn result_expect_err_builds_its_message_only_on_the_ok_path() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::result::Result::{ self, Ok, Err };

        mut built = 0;

        fun why(): str {
            built += 1;
            "not an error"
        }

        fun main() {
            let bad: Result<i32, str> = Err("boom");
            print(bad.expect_err(i"{why()}"));
            print(i"built {built}");
        }
        "#,
        "boom\nbuilt 0\n",
    );
}

/// The `Ok` path forces the message and panics with the author's own text — a
/// real `str` by the time `panic` sees it, not a cell (`expect`'s pin, on the
/// other arm).
#[test]
fn result_expect_err_panics_with_the_message_it_was_given() {
    assert_run_panics(
        r#"
        import std::io::print;
        import std::result::Result::{ self, Ok, Err };

        fun main() {
            let key = "ada";
            let good: Result<i32, str> = Ok(7);
            print(good.expect_err(i"{key} was fine"));
        }
        "#,
        "ada was fine",
    );
}

/// The memo, through a std member: `unwrap_or`'s fallback is evaluated at most
/// ONCE however many times the value is asked for, and a forwarding chain into
/// it is still one memo (§1's rule reaching std).
#[test]
fn a_lazy_std_fallback_forwards_from_a_lazy_parameter_without_forcing() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        mut built = 0;

        fun expensive(): i32 {
            built += 1;
            99
        }

        fun settle(slot: Option<i32>, lazy fallback: i32): i32 {
            slot.unwrap_or(fallback)
        }

        fun main() {
            print(settle(Some(1), expensive()));
            print(i"built {built}");
            print(settle(None, expensive()));
            print(i"built {built}");
        }
        "#,
        "1\nbuilt 0\n99\nbuilt 1\n",
    );
}

/// The emission half, which behaviour cannot tell apart: the retrofitted
/// positions THUNK at the call site (`__lazy`), and the forwarding hop above
/// passes the cell straight through.
///
/// **M81 edited the fixture and not the claim.** The arguments were `1`, `2`
/// and `"gone"` — all three INERT, and all three now lowered eagerly, so the
/// test asserted a thunk over arguments that no longer build one. The
/// fallbacks are computed by a call here, which is the shape the retrofit
/// exists for; `a_lazy_position_filled_with_a_literal_builds_no_cell` is the
/// inert case's own pin.
#[test]
fn the_retrofitted_std_members_thunk_at_the_call_site() {
    let source = r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        import std::result::Result::{ self, Ok, Err };

        fun fallback(): i32 {
            1
        }

        fun complaint(): str {
            "gone"
        }

        fun main() {
            let absent: Option<i32> = None;
            print(absent.unwrap_or(fallback()));
            let bad: Result<i32, str> = Err("boom");
            print(bad.unwrap_or(fallback()));
            let present: Option<i32> = Some(3);
            print(present.expect(complaint()));
        }
        "#;
    assert_emits_containing(source, "__lazy(\"fallback\", () => {");
    assert_emits_containing(source, "__lazy(\"message\", () => {");
}

// --- §M81, the inert-argument elision -------------------------------------

/// **M81** — a `lazy` position filled with a LITERAL at every call site builds
/// no cell, and the callee's reads do not force.
///
/// 111 of A103's 114 `__lazy` emissions carried a literal, an enum constant or
/// `[]`; each allocated a memo cell at the call site and paid a `__force` on
/// the callee's hot path to defer an expression that cannot have an effect,
/// cannot fail and cannot cycle. The behaviour is unchanged by construction —
/// there is nothing about evaluating `0` that a program can observe the timing
/// of — so this is an EMISSION pin, and the run beside it is what says the
/// value still arrives.
#[test]
fn a_lazy_position_filled_with_a_literal_builds_no_cell() {
    let source = r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        fun main() {
            let absent: Option<i32> = None;
            print(absent.unwrap_or(0));
        }
        "#;
    assert_compiles_and_runs(
        source, "0
",
    );
    let js = compile(source).expect("a clean compile");
    assert!(
        !js.contains("__lazy("),
        "a literal in a lazy position must not build a cell; emitted:\n{js}"
    );
    assert!(
        !js.contains("__force("),
        "and the callee must read the parameter plainly, with no cell to \
         force; emitted:\n{js}"
    );
}

/// The four other inert shapes, each on its own, because the set IS the claim:
/// a negated literal, `[]`, an enum constant and a `bool`.
#[test]
fn the_inert_shapes_each_build_no_cell() {
    for (fallback, expected) in [
        (
            "-1", "-1
",
        ),
        (
            "(0 - 1)", "-1
",
        ),
    ] {
        let source = format!(
            r#"
            import std::io::print;
            import std::option::Option::{{ self, Some, None }};

            fun main() {{
                let absent: Option<i32> = None;
                print(absent.unwrap_or({fallback}));
            }}
            "#
        );
        assert_compiles_and_runs(&source, expected);
    }
    // `[]` — the shape the item names, and the one that allocates.
    let list = r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        fun main() {
            let absent: Option<List<i32>> = None;
            print(absent.unwrap_or([]).len());
        }
        "#;
    assert_compiles_and_runs(
        list, "0
",
    );
    assert!(
        !compile(list).expect("a clean compile").contains("__lazy("),
        "an empty list literal is inert"
    );
    // A `bool` literal.
    let boolean = r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        fun main() {
            let absent: Option<bool> = None;
            print(absent.unwrap_or(false));
        }
        "#;
    assert_compiles_and_runs(
        boolean, "false
",
    );
    assert!(
        !compile(boolean)
            .expect("a clean compile")
            .contains("__lazy("),
        "a `bool` literal is inert"
    );
    // An enum CONSTANT — `None` standing in for an `Option<i32>` fallback.
    let constant = r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        fun main() {
            let absent: Option<Option<i32>> = None;
            let inner = absent.unwrap_or(None);
            print(inner.unwrap_or(7));
        }
        "#;
    assert_compiles_and_runs(
        constant, "7
",
    );
    assert!(
        !compile(constant)
            .expect("a clean compile")
            .contains("__lazy("),
        "a nullary variant constant is inert"
    );
}

/// **The non-elision, and it is the load-bearing half.** A call in a lazy
/// position still thunks, and still defers: the whole feature is that the
/// fallback is not built on the path that does not need it.
#[test]
fn a_call_in_a_lazy_position_still_thunks_and_still_defers() {
    let source = r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        mut built = 0;

        fun expensive(): i32 {
            built += 1;
            99
        }

        fun main() {
            let present: Option<i32> = Some(1);
            print(present.unwrap_or(expensive()));
            print(i"built {built}");
            let absent: Option<i32> = None;
            print(absent.unwrap_or(expensive()));
            print(i"built {built}");
        }
        "#;
    assert_compiles_and_runs(
        source,
        "1
built 0
99
built 1
",
    );
    let js = compile(source).expect("a clean compile");
    assert!(
        js.contains("__lazy(\"fallback\", () => {"),
        "a call argument must still build a cell; emitted:\n{js}"
    );
    assert!(
        js.contains("__force("),
        "and the callee must still force it; emitted:\n{js}"
    );
}

/// **The MIXED program**, which is why the decision is per PARAMETER and not
/// per argument. The callee is emitted once for every call site it has, so its
/// reads either force or they do not: a parameter one site fills with `0` and
/// another with a call keeps its cell at BOTH sites.
#[test]
fn one_thunking_call_site_keeps_the_cell_at_every_other_site() {
    let source = r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        mut built = 0;

        fun expensive(): i32 {
            built += 1;
            99
        }

        fun main() {
            let absent: Option<i32> = None;
            print(absent.unwrap_or(0));
            print(absent.unwrap_or(expensive()));
            print(i"built {built}");
        }
        "#;
    assert_compiles_and_runs(
        source,
        "0
99
built 1
",
    );
    let js = compile(source).expect("a clean compile");
    assert!(
        js.contains("__force("),
        "one thunking site means the callee forces, so the inert site must \
         hand it a cell too; emitted:\n{js}"
    );
    assert_eq!(
        js.matches("__lazy(\"fallback\"").count(),
        2,
        "both sites build a cell — the inert one cannot be elided while the \
         callee it shares forces; emitted:\n{js}"
    );
}

/// The forwarding chain, which is why the elision is taken to a FIXPOINT. A
/// read of an eager parameter is itself inert — its value was fixed at the
/// outer call site and a parameter binding is immutable — so a hop that
/// forwards one stays a plain pass-through instead of re-wrapping the value in
/// a cell the next callee would have to force.
#[test]
fn an_eager_parameter_forwarded_onward_keeps_the_chain_eager() {
    let source = r#"
        import std::io::print;

        fun inner(lazy message: str): i32 {
            print(message);
            0
        }

        fun middle(lazy message: str): i32 {
            inner(message)
        }

        fun main() {
            middle("a");
        }
        "#;
    assert_compiles_and_runs(
        source, "a
",
    );
    let js = compile(source).expect("a clean compile");
    assert!(
        !js.contains("__lazy(") && !js.contains("__force("),
        "the whole chain is eager: one literal at the outermost site, and no \
         cell anywhere; emitted:\n{js}"
    );
    assert!(
        js.contains("function middle(message) {\n\treturn inner(message);\n}"),
        "and the hop is a plain pass-through; emitted:\n{js}"
    );
}

/// A `lazy` parameter of a resource type is still refused at its DECLARATION
/// even when no call site would build a cell for it — the elision is a
/// lowering decision and must not reach into what the program MEANS.
///
/// The first shape of M81 removed the parameter from `lazy_cells`, which is
/// what the declaration-side refusals are gated on, and this refusal stopped
/// firing for an uncalled declaration. That is the pin
/// (`a_lazy_parameter_of_resource_type_is_refused` is the uncalled case; this
/// is the CALLED one, with an inert argument).
#[test]
fn an_inert_argument_does_not_excuse_a_resource_typed_lazy_parameter() {
    assert_fails_with(
        r#"
        import std::io::print;
        import std::drop::Drop;

        resource struct Res { tag: str }

        impl Res with Drop {
            fun drop(&mut self) {
                print(self.tag);
            }
        }

        fun takes(lazy handle: Res): i32 {
            0
        }

        fun main() {
            print("x");
        }
        "#,
        "is a `lazy` parameter of the resource type `Res`",
    );
}

/// `unwrap_or_else` is untouched by the retrofit (§6b: "`unwrap_or_else`
/// remains the explicit form"), and a closure argument is not a thunk.
#[test]
fn unwrap_or_else_is_not_retrofitted() {
    let source = r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        mut built = 0;

        fun expensive(): i32 {
            built += 1;
            99
        }

        fun main() {
            let present: Option<i32> = Some(7);
            print(present.unwrap_or_else(|| expensive()));
            print(i"built {built}");
        }
        "#;
    assert_compiles_and_runs(source, "7\nbuilt 0\n");
    let js = compile(source).expect("a clean compile");
    assert!(
        !js.contains("__lazy("),
        "a closure argument is not a thunk; emitted:\n{js}"
    );
}

/// The copy the eager form made is still made: the fallback is evaluated inside
/// the thunk, but the CLONE stays in the callee, so a `List` fallback hands back
/// a copy and the caller's binding is untouched (R1). A regression here would be
/// an aliasing miscompile the differential's prints could not see.
#[test]
fn a_list_fallback_is_still_copied_out_of_the_callers_binding() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        fun main() {
            mut spare: List<i32> = [];
            let absent: Option<List<i32>> = None;
            mut got = absent.unwrap_or(spare);
            got.push(1);
            print(i"got {got.len()} spare {spare.len()}");
            spare.push(9);
            print(i"got {got.len()} spare {spare.len()}");
        }
        "#,
        "got 1 spare 0\ngot 1 spare 1\n",
    );
}

// --- B344: the data-only rule at a GENERIC lazy parameter ------------------
//
// `lazy fallback: T` is not a resource at the declaration, and inside a generic
// the value standing in the position is `T`-typed too, so both ends of §1's
// "data only" rule used to read clean while the thunk really did own a
// resource. Two fixes, two shapes: the argument check no longer depends on the
// expression map carrying a type a bare name never puts there, and the rule is
// asked again at every resource INSTANTIATION, where the indirect case lives.

/// A resource the caller PRODUCES in a lazy position: no binding is named, so
/// R9's capture scan sees nothing, and the thunk owns a `Res` that nothing
/// forces and nothing destroys. Refused at the instantiation.
#[test]
fn a_generic_lazy_parameter_instantiated_at_a_produced_resource_is_refused() {
    assert_fails_with(
        r#"
        import std::drop::{ Drop, drop };
        import std::io::print;

        resource struct Res { tag: str }
        impl Res with Drop {
            fun drop(&mut self) {
                print(i"drop {self.tag}");
            }
        }

        fun hold<T>(flag: bool, lazy fallback: T): bool {
            flag
        }

        fun make(): Res {
            Res { tag = "made" }
        }

        fun main() {
            print(i"{hold(true, make())}");
        }
        "#,
        "`lazy` parameter `fallback` at the resource `Res`",
    );
}

/// The INDIRECT case: a generic hands its own `T` on to another generic's lazy
/// parameter. Nothing at the inner call site is concretely anything — the
/// argument is a `T`-typed parameter — so only the instantiation knows, and it
/// learns it by the same propagation that carries R11 there. The type does not
/// ground at that hop, so the message says "a resource type" rather than naming
/// the `T` it was written with.
#[test]
fn a_generic_forwarding_its_own_type_into_a_lazy_parameter_is_refused() {
    assert_fails_with(
        r#"
        import std::drop::{ Drop, drop };
        import std::io::print;

        resource struct Res { tag: str }
        impl Res with Drop {
            fun drop(&mut self) {
                print(i"drop {self.tag}");
            }
        }

        fun hold<T>(flag: bool, lazy fallback: T): bool {
            flag
        }

        fun forward<T>(flag: bool, own value: T): bool {
            hold(flag, value)
        }

        fun sink(own value: Res) {
            drop(value);
        }

        fun main() {
            let conn = Res { tag = "indirect" };
            print(i"{forward(true, conn)}");
        }
        "#,
        "`lazy` parameter `fallback` at a resource type",
    );
}

/// A MODULE-LEVEL resource named bare in a lazy position. R9's thunk capture
/// scan exempts it (process lifetime), and the argument check could not see it
/// either, because a bare `Expr::Local` carries no type on its own id — so this
/// was refused by nobody. The argument check owns it now, at the argument's own
/// span.
#[test]
fn a_module_level_resource_in_a_lazy_position_is_refused_at_the_argument() {
    assert_fails_with(
        r#"
        import std::drop::{ Drop, drop };
        import std::io::print;

        resource struct Res { tag: str }
        impl Res with Drop {
            fun drop(&mut self) {
                print(i"drop {self.tag}");
            }
        }

        let shared: Res = Res { tag = "module" };

        fun hold<T>(flag: bool, lazy fallback: T): bool {
            flag
        }

        fun main() {
            print(i"{hold(true, shared)}");
        }
        "#,
        "this argument is the resource `Res`, and it stands in the `lazy` parameter `fallback`",
    );
}

/// B5, the other half of the same change: a resource LOCAL named bare stays
/// R9's, in R9's own words (lazy.md §8), and the instantiation check stands
/// down rather than saying the same thing a second time.
#[test]
fn a_resource_local_in_a_lazy_position_is_still_only_the_r9_capture() {
    let source = r#"
        import std::drop::{ Drop, drop };
        import std::io::print;

        resource struct Res { tag: str }
        impl Res with Drop {
            fun drop(&mut self) {
                print(i"drop {self.tag}");
            }
        }

        fun hold<T>(flag: bool, lazy fallback: T): bool {
            flag
        }

        fun main() {
            let conn = Res { tag = "local" };
            print(i"{hold(true, conn)}");
        }
        "#;
    assert_fails_with(source, "a closure cannot capture the resource `conn`");
    assert_fails_without(source, "instantiates");
    assert_fails_without(source, "this argument is the resource");
}

/// E200: a lazy argument reaching a resource through a FIELD of a resource
/// binding is ONE diagnostic, the argument check's.
///
/// It used to be two. R9's thunk capture scan sees the thunk name `holder` —
/// a resource binding from the enclosing body — and says a closure cannot
/// capture it; the argument check sees the argument's own type is `Conn` and
/// says so at the same span. Two diagnostics for one mistake, which is the
/// class B5 forbids, and neither was wrong: the shape really is both. The
/// argument check owns it because it names the TYPE and the POSITION, which
/// together are what the author has to change — "a closure cannot capture
/// `holder`" is about a closure the author never wrote.
#[test]
fn a_lazy_argument_through_a_resource_field_is_one_diagnostic() {
    let source = r#"
        import std::drop::{ Drop, drop };
        import std::io::print;

        resource struct Conn { tag: str }
        impl Conn with Drop {
            fun drop(&mut self) {
                print(i"drop {self.tag}");
            }
        }

        resource struct Holder { conn: Conn }

        fun hold<T>(flag: bool, lazy fallback: T): bool {
            flag
        }

        fun main() {
            let holder = Holder { conn = Conn { tag = "field" } };
            print(i"{hold(true, holder.conn)}");
        }
        "#;
    assert_fails_with(
        source,
        "this argument is the resource `Conn`, and it stands in the `lazy` parameter `fallback`",
    );
    assert_fails_without(source, "a closure cannot capture");
}

/// The same shape reached through a LOAN of the owner (`&Holder`), which is the
/// item's second form: the thunk names the loan parameter rather than an owned
/// binding, and the answer is the same one diagnostic.
#[test]
fn a_lazy_argument_through_a_loaned_resource_field_is_one_diagnostic() {
    let source = r#"
        import std::drop::{ Drop, drop };
        import std::io::print;

        resource struct Conn { tag: str }
        impl Conn with Drop {
            fun drop(&mut self) {
                print(i"drop {self.tag}");
            }
        }

        resource struct Holder { conn: Conn }

        fun hold<T>(flag: bool, lazy fallback: T): bool {
            flag
        }

        fun peek(holder: &Holder): bool {
            hold(true, holder.conn)
        }

        fun main() {
            let owner = Holder { conn = Conn { tag = "loan" } };
            print(i"{peek(&owner)}");
        }
        "#;
    assert_fails_with(
        source,
        "this argument is the resource `Conn`, and it stands in the `lazy` parameter `fallback`",
    );
    assert_fails_without(source, "a closure cannot capture");
}

/// A lazy parameter instantiated at DATA is untouched — the check is the delta
/// of the instantiation, so nothing about an ordinary generic changes.
#[test]
fn a_generic_lazy_parameter_at_a_data_type_still_compiles() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Note { text: str }

        fun hold<T>(flag: bool, lazy fallback: T): bool {
            flag
        }

        fun main() {
            let note = Note { text = "fine" };
            print(i"{hold(true, note)}");
        }
        "#,
        "true\n",
    );
}

// --- B345: the view-capture scan walks a call's SUBJECT ---------------------

/// Rule 3 over a lazy argument whose view sits in a nested call SUBJECT. The
/// scan walked a call's arguments and not the expression being CALLED, so a
/// view named inside `(build(seen.label))()` was invisible to it — and a thunk
/// is a closure, so this is rule 3's own refusal arriving where it always
/// should have.
#[test]
fn a_view_inside_a_nested_call_subject_in_a_lazy_argument_is_a_view_capture() {
    assert_fails_with(
        r#"
        import std::io::print;

        struct Holder { label: str }

        fun build(text: str): || str {
            || text
        }

        fun message(lazy text: str): i32 {
            print(text);
            0
        }

        fun main() {
            let holder = Holder { label = "a" };
            let seen = &holder;
            message((build(seen.label))());
        }
        "#,
        "a closure cannot capture the view 'seen'",
    );
}

// --- B362 (R4): a function with a `lazy` parameter is not a closure VALUE -----
//
// `lazy` is a promise about the CALL: the argument is wrapped in a thunk at the
// call site and the body forces it. Only a DIRECT call can keep that promise,
// because only a direct call is rewritten — `record_lazy_arguments` skips a
// dispatched or indirect callee. So a function reached through a closure slot
// was handed a plain value and forced it: `__force(7)` reaching `cell.state`
// on a number, a `TypeError` out of a program `vilan check` passed. M81 made
// the case with NO direct call correct (the parameter becomes eager) and left
// the MIXED one, which cannot be fixed at the call site: what escapes into the
// slot is the function itself. Refused at the coercion.

#[test]
fn b362_a_lazy_parameter_function_does_not_coerce_to_a_closure() {
    assert_fails_with(
        r#"
        fun expensive(): i32 {
        	print("computing");
        	42
        }

        fun choose(flag: bool, lazy fallback: i32): i32 {
        	if flag { 1 } else { fallback }
        }

        fun apply(f: |bool, i32| i32): i32 { f(false, 7) }

        fun main() {
        	print(choose(true, expensive()));
        	print(apply(choose));
        }
        "#,
        "but got fn choose(bool, lazy i32): i32",
    );
}

/// The refusal fires with NO direct call beside it too — M81's eager rewrite
/// made that case produce a right ANSWER, but the promise is still one the
/// closure slot cannot carry, and a later direct call would silently change
/// what the slot holds.
#[test]
fn b362_the_coercion_is_refused_even_with_no_direct_call() {
    assert_fails_with(
        r#"
        fun choose(flag: bool, lazy fallback: i32): i32 {
        	if flag { 1 } else { fallback }
        }

        fun apply(f: |bool, i32| i32): i32 { f(false, 7) }

        fun main() { print(apply(choose)); }
        "#,
        "but got fn choose(bool, lazy i32): i32",
    );
}

/// The refusal NAMES the difference, which is the whole of why `lazy` is
/// printed as part of a function type: without it the two sides of the message
/// read identically.
#[test]
fn b362_a_function_types_printed_form_carries_lazy() {
    assert_fails_with(
        r#"
        fun hold(lazy message: str): str { message }

        fun main() {
        	let slot: |str| str = hold;
        	print(slot("x"));
        }
        "#,
        "fn hold(lazy str): str",
    );
}

/// The CONTROLS: a direct call keeps its thunk and the laziness still works,
/// and the SAME function without `lazy` still coerces.
#[test]
fn b362_a_direct_call_still_defers_and_a_plain_function_still_coerces() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun expensive(): i32 {
        	print("computing");
        	42
        }

        fun choose(flag: bool, lazy fallback: i32): i32 {
        	if flag { 1 } else { fallback }
        }

        fun eager(flag: bool, fallback: i32): i32 {
        	if flag { 1 } else { fallback }
        }

        fun apply(f: |bool, i32| i32): i32 { f(false, 7) }

        fun main() {
        	print(choose(true, expensive()));
        	print(choose(false, expensive()));
        	print(apply(eager));
        }
        "#,
        "1\ncomputing\n42\n7\n",
    );
}

/// std's own `lazy`-parameter members all take `self`, which the coercion
/// already declined — so the refusal changes nothing about them, and they go
/// on deferring at a direct call.
#[test]
fn b362_stds_lazy_members_are_untouched() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        fun loud(): i32 {
        	print("built");
        	9
        }

        fun main() {
        	let present: Option<i32> = Some(1);
        	print(present.unwrap_or(loud()));
        	let absent: Option<i32> = None;
        	print(absent.unwrap_or(loud()));
        }
        "#,
        "1\nbuilt\n9\n",
    );
}

/// B362's DISPATCHED face, RULED and closed: thunk at the dispatched call site
/// through the callee's recorded convention.
///
/// A call through a generic BOUND has no impl to resolve at check time, so it
/// resolves to the TRAIT's declaration and the pair `record_lazy_arguments`
/// banks names the declaration's parameter — a different id from the impl's.
/// M81's eager elision is taken per id, so the two halves of one signature
/// could disagree: the caller passed a plain value and the callee's
/// `__force(7)` wrote `.state` on a number, a `TypeError` out of a program
/// `vilan check` passed. They are one CONVENTION now (`lazy_convention_groups`),
/// which is sound for exactly the reason the ruling gives — `lazy` is part of
/// the signature and `check_one_conformance` holds an impl to its trait's
/// answer, so the convention is known at the bound.
///
/// It is NOT closed by refusing `lazy` on a trait member: that was tried and
/// backed out, because a trait member reached by a DIRECT call on a concrete
/// receiver keeps its laziness and the language ships that deliberately —
/// `an_impl_that_agrees_keeps_the_laziness_through_dispatch` and
/// `lazy_is_accepted_in_all_three_grammar_homes` are both pins on it.
///
/// **This pin alone is not the evidence, and it was never red.** It has ONE
/// call site, whose argument is a literal, so M81's elision made the whole
/// parameter eager and the program ran right by accident — which is exactly
/// what the item recorded ("M81 made the NO-direct-call case correct and left
/// the MIXED case as it was"). The two pins below it are the red-first pair:
/// they put an effect on one side of the convention and a literal on the
/// other, in both orders.
#[test]
fn b362_a_lazy_parameter_reached_through_a_bound_is_still_unsound() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Fallback {
        	fun pick(self, flag: bool, lazy other: i32): i32;
        }

        struct Picker { base: i32 }

        impl Picker with Fallback {
        	fun pick(self, flag: bool, lazy other: i32): i32 {
        		if flag { self.base } else { other }
        	}
        }

        fun through<T: Fallback>(value: T): i32 { value.pick(false, 7) }

        fun main() {
        	print(through(Picker { base = 1 }));
        }
        "#,
        "7\n",
    );
}

/// The CONTROL: an INHERENT member keeps `lazy` — it is reached only by a
/// direct call, which is the door the rewrite sees. std's five
/// `expect`/`unwrap_or` members are exactly this shape.
#[test]
fn b362_an_inherent_member_still_takes_a_lazy_parameter() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Picker { base: i32 }

        impl Picker {
        	fun pick(self, flag: bool, lazy other: i32): i32 {
        		if flag { self.base } else { other }
        	}
        }

        fun expensive(): i32 {
        	print("computing");
        	42
        }

        fun main() {
        	let p = Picker { base = 1 };
        	print(p.pick(true, expensive()));
        	print(p.pick(false, expensive()));
        }
        "#,
        "1\ncomputing\n42\n",
    );
}

/// The MIXED case, which is the one M81 could not reach and the one that
/// actually crashed: a DIRECT call whose argument is not inert (so the
/// parameter stays lazy and the body forces) beside a DISPATCHED call whose
/// argument is (so the eager elision would have fired at the declaration's own
/// parameter and passed a plain `7`).
#[test]
fn b362_a_direct_and_a_dispatched_call_agree_on_one_convention() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Fallback {
        	fun pick(self, flag: bool, lazy other: i32): i32;
        }

        struct Picker { base: i32 }

        impl Picker with Fallback {
        	fun pick(self, flag: bool, lazy other: i32): i32 {
        		if flag { self.base } else { other }
        	}
        }

        fun expensive(): i32 {
        	print("computing");
        	42
        }

        fun through<T: Fallback>(value: T): i32 { value.pick(false, 7) }

        fun main() {
        	let direct = Picker { base = 1 };
        	print(direct.pick(true, expensive()));
        	print(direct.pick(false, expensive()));
        	print(through(Picker { base = 1 }));
        }
        "#,
        "1\ncomputing\n42\n7\n",
    );
}

/// The other direction of the same convention: the DIRECT call is the inert
/// one and the DISPATCHED call carries the effect. The parameter must stay
/// lazy for both, and the effect must run exactly once, on the `None` path.
#[test]
fn b362_a_dispatched_call_carrying_the_effect_still_defers_it() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Fallback {
        	fun pick(self, flag: bool, lazy other: i32): i32;
        }

        struct Picker { base: i32 }

        impl Picker with Fallback {
        	fun pick(self, flag: bool, lazy other: i32): i32 {
        		if flag { self.base } else { other }
        	}
        }

        fun expensive(): i32 {
        	print("computing");
        	42
        }

        fun taken<T: Fallback>(value: T): i32 { value.pick(false, expensive()) }
        fun skipped<T: Fallback>(value: T): i32 { value.pick(true, expensive()) }

        fun main() {
        	let picker = Picker { base = 1 };
        	print(picker.pick(true, 7));
        	print(skipped(picker));
        	print(taken(picker));
        }
        "#,
        "1\n1\ncomputing\n42\n",
    );
}

/// The elision M81 built is untouched where the whole convention is inert: a
/// `lazy` parameter every site — direct and dispatched — fills with a literal
/// builds no cell at all.
#[test]
fn b362_a_convention_every_site_fills_inertly_is_still_eager() {
    assert_emits_containing(
        r#"
        import std::io::print;

        trait Fallback {
        	fun pick(self, flag: bool, lazy other: i32): i32;
        }

        struct Picker { base: i32 }

        impl Picker with Fallback {
        	fun pick(self, flag: bool, lazy other: i32): i32 {
        		if flag { self.base } else { other }
        	}
        }

        fun through<T: Fallback>(value: T): i32 { value.pick(false, 7) }

        fun main() {
        	print(Picker { base = 1 }.pick(true, 3));
        	print(through(Picker { base = 1 }));
        }
        "#,
        "\t\t$a = other;",
    );
}
