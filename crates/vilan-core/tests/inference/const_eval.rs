//! G24 — `const let` and `const fun`: compile-time CLOSURE bindings and
//! declaration-gated const functions.
//!
//! `const let NAME[: T] = EXPR;` is `let NAME = const EXPR;` plus the one
//! addition `const-eval.md` §11 states — the result may be a closure over
//! plain data — and `const fun` is the opt-in promise that a body is
//! const-evaluable, checked at the declaration. `const mut` is refused.
//!
//! The snapshot's runtime half is what these pins are mostly about: a const
//! closure ships as the closure's OWN body with its captures baked, emitted
//! once, no wrapper — which is why several of them read the emitted JS rather
//! than only the program's behaviour.
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

// --- `const let`: the grammar and the fold --------------------------------

#[test]
fn a_const_let_binding_folds_like_a_const_initializer() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::Display;

        const let width = 2 * 3;

        fun main() {
            print(width.to_string());
        }
        "#,
        "6\n",
    );
}

#[test]
fn a_const_let_binding_is_compile_time_known_to_a_later_const_expression() {
    assert_emits_containing(
        r#"
        import std::io::print;
        import std::display::Display;

        const let width = 2 * 3;

        fun main() {
            let doubled = const width * 2;
            print(doubled.to_string());
        }
        "#,
        "= 12;",
    );
}

/// R3: the declaration is a STATEMENT, so it is legal in a body as well as at
/// module level.
#[test]
fn a_const_let_binding_is_legal_inside_a_function_body() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::Display;

        fun main() {
            const let local = 6f;
            print(local.to_string());
        }
        "#,
        "6\n",
    );
}

#[test]
fn a_const_let_binding_takes_an_annotation() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        const let label: str = "ok";

        fun main() {
            print(label);
        }
        "#,
        "ok\n",
    );
}

#[test]
fn const_mut_is_refused_and_names_both_spellings() {
    assert_fails_with(
        r#"
        fun main() {
            const mut counter = 1;
        }
        "#,
        "a compile-time value has no runtime mutation",
    );
}

// --- The closure snapshot (§11's admission) -------------------------------

/// The headline pin, in both directions: `const add(1f, 2f)` folds to `3` at
/// build time, and `add(x, y)` at runtime calls the SNAPSHOT — the closure's
/// own emitted body.
#[test]
fn a_const_let_closure_folds_a_const_call_and_runs_at_runtime() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::Display;

        const let add = |a: f64, b: f64| a + b;

        fun main() {
            print(const add(1f, 2f).to_string());
            print(add(10f, 5f).to_string());
        }
        "#,
        "3\n15\n",
    );
}

/// `const fun scale_step` + `const let space = scale_step(0.25)`: the padding
/// folds at build time, and the runtime `space(n)` is ONE emitted closure with
/// `0.25` baked — no IIFE, no wrapper, the closure's own body.
#[test]
fn a_const_fun_returning_a_closure_bakes_its_captures_into_one_emitted_arrow() {
    assert_emits_containing(
        r#"
        import std::io::print;
        import std::display::Display;

        const fun scale_step(rem: f64): |f64| f64 {
            |n: f64| rem * n
        }

        const let space = scale_step(0.25);

        fun main() {
            print(const space(2f).to_string());
            print(space(8f).to_string());
        }
        "#,
        "0.25 * n",
    );
}

#[test]
fn a_const_fun_snapshot_runs_to_the_same_answers_at_both_times() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::Display;

        const fun scale_step(rem: f64): |f64| f64 {
            |n: f64| rem * n
        }

        const let space = scale_step(0.25);

        fun main() {
            print(const space(2f).to_string());
            print(space(8f).to_string());
        }
        "#,
        "0.5\n2\n",
    );
}

/// The `const let` admission is the DECLARATION's, not the keyword's: a plain
/// `let x = const <closure>` keeps §1's plain-data refusal — and R2's steer
/// names the declaration that admits one.
#[test]
fn a_closure_result_outside_const_let_is_refused_and_steers_to_the_declaration() {
    assert_fails_with(
        r#"
        fun main() {
            let add = const |a: f64, b: f64| a + b;
        }
        "#,
        "A compile-time CLOSURE is spelled as a declaration: write `const let add = ..;`",
    );
}

/// R2's other half: a `const` expression reading a plain binding is refused,
/// and the refusal names the one keyword that would resolve it.
#[test]
fn a_const_expression_reading_a_runtime_binding_steers_to_const_let() {
    assert_fails_with(
        r#"
        fun main() {
            let space = |n: f64| n * 2f;
            let a = const space(2f);
        }
        "#,
        "Declare it `const let space = ..;` to make it compile-time-known",
    );
}

/// The steer is EVIDENCE-BASED: a parameter cannot be declared `const let`, so
/// it gets the refusal alone (B83 — an impossible steer is worse than none).
#[test]
fn a_const_expression_reading_a_parameter_gets_no_const_let_steer() {
    assert_fails_without(
        r#"
        fun scaled(n: f64): f64 {
            const n * 2f
        }
        fun main() {
            let a = scaled(2f);
        }
        "#,
        "Declare it `const let",
    );
}

/// A captured RUNTIME binding is the existing refusal at the capture, not a
/// snapshot: `const let` promises compile-time data all the way down. (A `let`
/// over a LITERAL is already compile-time-known — `classify`'s own rule — so
/// the fixture uses the one binding form that never can be.)
#[test]
fn a_const_let_closure_capturing_a_runtime_binding_is_refused_at_the_capture() {
    assert_fails_with(
        r#"
        fun main() {
            mut factor = 3f;
            factor = 4f;
            const let scaled = |n: f64| n * factor;
        }
        "#,
        "`factor` is a runtime value; a `const` expression reads only compile-time-known bindings",
    );
}

/// The complement, and the reason the fixture above needs a `mut`: a `let`
/// over a literal IS compile-time-known, so a closure closing over one
/// snapshots with the literal baked.
#[test]
fn a_const_let_closure_capturing_a_literal_binding_bakes_it() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::Display;

        fun main() {
            let factor = 3f;
            const let scaled = |n: f64| n * factor;
            print(scaled(2f).to_string());
        }
        "#,
        "6\n",
    );
}

// --- `const fun`: the declaration gate ------------------------------------

#[test]
fn a_const_fun_reaching_a_host_capability_errors_at_the_declaration() {
    assert_fails_with(
        r#"
        import std::process;

        const fun home(): Option<str> {
            process::env("HOME")
        }

        fun main() {
            let h = home();
        }
        "#,
        "`home` is declared `const fun`, but its body reaches `env`, which has no compile-time \
         answer",
    );
}

/// Transitive: the gate reads the CALL GRAPH, so a body that reaches the
/// capability one call down is refused at its own declaration too.
#[test]
fn a_const_fun_reaching_a_capability_through_a_callee_errors_at_the_declaration() {
    assert_fails_with(
        r#"
        import std::process;

        fun inner(): Option<str> {
            process::env("HOME")
        }

        const fun outer(): Option<str> {
            inner()
        }

        fun main() {
            let h = outer();
        }
        "#,
        "`outer` is declared `const fun`, but its body reaches `env`",
    );
}

/// NOT a colouring requirement: a `const fun` is still an ordinary function at
/// runtime, called with runtime arguments.
#[test]
fn a_const_fun_is_still_callable_at_runtime_with_runtime_arguments() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::Display;

        const fun twice(n: f64): f64 {
            n * 2f
        }

        fun main() {
            let runtime = 4f;
            print(twice(runtime).to_string());
            print(const twice(3f).to_string());
        }
        "#,
        "8\n6\n",
    );
}

/// §1's Zig-shaped rule stands: a PLAIN `fun` is const-callable, so the
/// declaration is an opt-in guarantee rather than a requirement.
#[test]
fn a_plain_fun_is_still_const_callable() {
    assert_emits_containing(
        r#"
        import std::io::print;
        import std::display::Display;

        fun twice(n: f64): f64 {
            n * 2f
        }

        fun main() {
            print(const twice(3f).to_string());
        }
        "#,
        "\"6\"",
    );
}
