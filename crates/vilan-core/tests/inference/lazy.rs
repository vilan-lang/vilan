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
