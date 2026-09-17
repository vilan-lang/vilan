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
#[test]
fn a_forward_emits_the_cell_and_an_eager_position_emits_a_force() {
    let source = r#"
        import std::io::print;

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
            middle("a");
            forces("b");
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

// --- §6b, S3: the std retrofit (A103) -------------------------------------
//
// Four members take a lazy argument: `Option::expect`, `Option::unwrap_or`,
// `Result::expect` and `Result::unwrap_or`. Each pin reads the retrofit off
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
#[test]
fn the_retrofitted_std_members_thunk_at_the_call_site() {
    let source = r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        import std::result::Result::{ self, Ok, Err };

        fun main() {
            let absent: Option<i32> = None;
            print(absent.unwrap_or(1));
            let bad: Result<i32, str> = Err("boom");
            print(bad.unwrap_or(2));
            let present: Option<i32> = Some(3);
            print(present.expect("gone"));
        }
        "#;
    assert_emits_containing(source, "__lazy(\"fallback\", () => {");
    assert_emits_containing(source, "__lazy(\"message\", () => {");
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
