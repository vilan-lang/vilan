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

/// G26: `str::index_of` / `last_index_of` (over the host's `indexOf` /
/// `lastIndexOf`) failed any `const` evaluation — "the string method
/// `indexOf` is not available at expansion time" — while `contains`, `split`
/// and `substring` ran. Present, absent, an empty needle (0 and the length,
/// as the host places it) and a position after a non-BMP character, which
/// counts UTF-16 code units like `len` does.
#[test]
fn g26_index_of_and_last_index_of_evaluate_at_expansion_time() {
    let source = r#"
        import std::io::print;
        import std::option::Option::{ self, None, Some };

        fun find(text: str): usize {
            match text.index_of("b") {
                Some(let at) => at,
                None => 99,
            }
        }

        fun positions(): str {
            let text = "a-b-c";
            i"{text.index_of("-").unwrap_or(99)} {text.last_index_of("-").unwrap_or(99)} {text.index_of("z").is_none()} {text.last_index_of("z").is_none()} {text.index_of("").unwrap_or(99)} {text.last_index_of("").unwrap_or(99)} {"a😀b".index_of("b").unwrap_or(99)}"
        }

        fun main() {
            print(const find("abc"));
            print(const positions());
            print(positions());
        }
        "#;
    assert_emits_containing(source, "\"1 3 true true 0 5 3\"");
    assert_compiles_and_runs(source, "1\n1 3 true true 0 5 3\n1 3 true true 0 5 3\n");
}

/// G26's census, as a class: EVERY method std declares on `str` over a host
/// binding (`impl str`'s externals — `len` through `code_at`, `parse_i32`,
/// `parse_f64`, the two raw searches — and the std surface built on them)
/// evaluates at expansion time, to what the program computes at run time.
#[test]
fn g26_every_str_method_std_declares_evaluates_at_expansion_time() {
    let source = r#"
        import std::display::Display;
        import std::io::print;

        fun census(): str {
            let text = " a-b-c ";
            let parts = [
                i"{text.len()}",
                text.trim(),
                text.to_uppercase(),
                text.to_lowercase(),
                i"{text.contains("b")}",
                i"{text.starts_with(" ")}",
                i"{text.ends_with("c")}",
                text.replace("-", "+"),
                text.repeat(2),
                i"{text.split("-").len()}",
                text.substring(1, 3),
                i"{text.code_at(1)}",
                i"{"12".parse_i32().unwrap_or(0)}",
                i"{"1.5".parse_f64().unwrap_or(0.0)}",
                i"{text.index_of("-").unwrap_or(99)}",
                i"{text.last_index_of("-").unwrap_or(99)}",
                text.strip_prefix(" a").unwrap_or("?"),
                text.strip_suffix("c ").unwrap_or("?"),
            ];
            parts.join("|")
        }

        fun main() {
            print(const census());
            print(census());
        }
        "#;
    let expected = "7|a-b-c| A-B-C | a-b-c |true|true|false| a+b+c | a-b-c  a-b-c |3|a-|97|12|1.5|2|4|-b-c | a-b-";
    assert_compiles_and_runs(source, &format!("{expected}\n{expected}\n"));
}

/// The trait and the two implementors the G25 const pins evaluate.
const G25_SOURCES: &str = r#"
    import std::io::print;

    trait Src {
        fun get(&self): i32;
    }

    struct Fixed {
        value: i32,
    }

    impl Fixed with Src {
        fun get(&self): i32 {
            self.value
        }
    }

    struct Doubled {
        value: i32,
    }

    impl Doubled with Src {
        fun get(&self): i32 {
            self.value * 2
        }
    }
"#;

/// G25: a `const` evaluation that builds a trait object INSIDE it — a `let`
/// typed `dyn Src`, a function returning one, a list of them — failed "`$a` is
/// not defined": the object's method table is a declaration the emitter adds
/// beside the functions, and the const world never recorded it as reached, so
/// the evaluation named a table nothing declared. The table is a keyed
/// emission like an instance now, and the values come out as plain data.
#[test]
fn g25_a_const_evaluation_dispatches_through_a_trait_object() {
    assert_compiles_and_runs(
        &format!(
            r#"{G25_SOURCES}
            fun total(): i32 {{
                let source: dyn Src = Fixed {{ value = 4 }};
                source.get()
            }}

            fun make(doubled: bool): dyn Src {{
                if doubled {{ Doubled {{ value = 3 }} }} else {{ Fixed {{ value = 3 }} }}
            }}

            fun sum(): i32 {{
                let sources: List<dyn Src> = [Fixed {{ value = 1 }}, Doubled {{ value = 5 }}];
                mut sum = 0;
                for source in sources {{
                    sum += source.get();
                }}
                sum
            }}

            const let T: i32 = total();

            fun main() {{
                print(T);
                print(const make(true).get());
                print(const make(false).get());
                print(const sum());
            }}
            "#
        ),
        "4\n6\n3\n11\n",
    );
}

/// G25: a `const` whose VALUE is or holds a trait object — `const let C: dyn
/// Src`, a struct with a `dyn` field — is refused as that, not as "a `Shared`
/// cell" (or, before the table was reached, "`$a` is not defined"): the
/// object's table is code the program builds, which no build can write down
/// as data, and the steer keeps the concrete type at the const.
#[test]
fn g25_a_const_holding_a_trait_object_is_refused_by_name() {
    for (declaration, read) in [
        ("const let C: dyn Src = Fixed { value = 3 };", "C.get()"),
        (
            "struct Holder { source: dyn Src }\nconst let C: Holder = Holder { source = Fixed { value = 3 } };",
            "C.source.get()",
        ),
        (
            "const let C: List<dyn Src> = [Fixed { value = 3 }];",
            "C.len()",
        ),
    ] {
        assert_fails_with(
            &format!(
                r#"{G25_SOURCES}
                {declaration}

                fun main() {{
                    print({read});
                }}
                "#
            ),
            "this evaluates to a trait object (a `dyn` value), whose method table is code the \
             program builds rather than data a build can write down",
        );
    }
}
