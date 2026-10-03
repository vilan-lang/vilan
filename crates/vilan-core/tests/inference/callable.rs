//! B340 — the CALL OPERATOR: `std::operators::Callable` makes a struct VALUE
//! callable, and `x(args)` resolves as the method `call` on it.
//!
//! The marker carries no signature: the impl's own `call` declares the arity
//! and the types, so every question a call asks — arity, argument types,
//! generic binding, a `context` clause — is the ordinary method path's, and
//! these pins are written to hold that equivalence rather than a second set of
//! rules. Q1's coercion (a `Callable` where a closure is wanted) and Q2's field
//! rule (`(a.b)(c)`) are the ruled 2026-09-14 halves.
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

/// kolt's `src/lib/scale_step.vl` (read-only in this lane), with the FIXME it
/// carries taken: `fun get(self, n)` becomes `fun call(self, n)` under
/// `Callable`, so `space(2f)` is the call the file wanted to write. The shape
/// travels as a fixture so the language pin and the estate's use case are the
/// same program.
const SCALE_STEP: &str = r#"
        import std::operators::Callable;

        struct Scale {
            unit: f64,
        }

        impl Scale {
            fun new(rem: f64): Scale {
                Scale { unit = rem }
            }
        }

        impl Scale with Callable {
            fun call(self, n: f64): f64 {
                self.unit * n
            }
        }
"#;

// --- The call itself ------------------------------------------------------

#[test]
fn a_callable_value_is_called_like_a_function() {
    assert_compiles_and_runs(
        &format!(
            r#"
        import std::io::print;
        import std::display::Display;
        {SCALE_STEP}
        fun main() {{
            let space = Scale::new(0.25);
            print(space(2f).to_string());
        }}
        "#
        ),
        "0.5\n",
    );
}

#[test]
fn calling_a_callable_is_the_same_as_calling_call() {
    assert_compiles_and_runs(
        &format!(
            r#"
        import std::io::print;
        import std::display::Display;
        {SCALE_STEP}
        fun main() {{
            let space = Scale::new(0.25);
            print((space(2f) == space.call(2f)).to_string());
        }}
        "#
        ),
        "true\n",
    );
}

#[test]
fn a_callable_call_checks_its_argument_types_at_the_call() {
    assert_fails_with(
        &format!(
            r#"
        {SCALE_STEP}
        fun main() {{
            let space = Scale::new(0.25);
            let bad = space("two");
        }}
        "#
        ),
        "Expected f64, but got str instead.",
    );
}

#[test]
fn a_callable_call_checks_its_arity_at_the_call() {
    assert_fails_with(
        &format!(
            r#"
        {SCALE_STEP}
        fun main() {{
            let space = Scale::new(0.25);
            let bad = space(1f, 2f);
        }}
        "#
        ),
        "`call` expects 1 argument, but got 2 instead.",
    );
}

#[test]
fn a_callable_call_takes_its_return_type_from_call() {
    assert_fails_with(
        &format!(
            r#"
        {SCALE_STEP}
        fun main() {{
            let space = Scale::new(0.25);
            let bad: str = space(2f);
        }}
        "#
        ),
        "Expected str, but got f64 instead.",
    );
}

/// A `context` clause is a property of `call`'s BODY, so a call through the
/// operator inherits it exactly as `space.call(2f)` would: reached outside an
/// enclosing `run`, the context read is refused and the trace names the chain.
#[test]
fn a_context_clause_on_call_is_inherited_at_the_call() {
    assert_fails_with(
        r#"
        import std::context::Context;
        import std::operators::Callable;

        let flavor: Context<i32> = Context::new();

        struct Ambient {
            n: i32,
        }

        impl Ambient with Callable {
            fun call(self): i32 {
                self.n + flavor.get()
            }
        }

        fun main() {
            let ambient = Ambient { n = 1 };
            let read = ambient();
        }
        "#,
        "context `flavor` is read here, but this code can be reached without an enclosing `run`",
    );
}

#[test]
fn a_callable_resolves_through_a_generic_impl() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::Display;
        import std::operators::Callable;

        struct Boxed<T> {
            value: T,
        }

        impl Boxed<type T> with Callable {
            fun call(self): T {
                self.value
            }
        }

        fun main() {
            let held = Boxed { value = 7 };
            print(held().to_string());
        }
        "#,
        "7\n",
    );
}

// --- The two refusals -----------------------------------------------------

#[test]
fn implementing_callable_without_a_call_method_is_refused() {
    assert_fails_with(
        r#"
        import std::operators::Callable;

        struct Scale {
            unit: f64,
        }

        impl Scale with Callable {}

        fun main() {
            let s = Scale { unit = 0.25 };
        }
        "#,
        "implementing `Callable` requires a `call` method",
    );
}

/// `call` declared in a SEPARATE inherent block satisfies the marker: the
/// refusal is about the type having no `call` at all, not about where the
/// author chose to write it.
#[test]
fn a_call_method_in_a_separate_block_satisfies_callable() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::Display;
        import std::operators::Callable;

        struct Scale {
            unit: f64,
        }

        impl Scale {
            fun call(self, n: f64): f64 {
                self.unit * n
            }
        }

        impl Scale with Callable {}

        fun main() {
            let space = Scale { unit = 0.25 };
            print(space(2f).to_string());
        }
        "#,
        "0.5\n",
    );
}

#[test]
fn a_call_method_without_callable_keeps_the_refusal_and_steers_to_the_marker() {
    assert_fails_with(
        r#"
        struct Scale {
            unit: f64,
        }

        impl Scale {
            fun call(self, n: f64): f64 {
                self.unit * n
            }
        }

        fun main() {
            let space = Scale { unit = 0.25 };
            let bad = space(2f);
        }
        "#,
        "it declares a `call` method but does not implement `Callable` — write `impl Scale with \
         Callable` to call it as a function",
    );
}

/// A type with no `call` at all keeps the bare message: the steer is evidence
/// of a `call` method, not decoration on every non-callable value.
#[test]
fn a_plain_struct_called_keeps_the_bare_refusal() {
    assert_fails_without(
        r#"
        struct Point {
            x: i32,
        }

        fun main() {
            let p = Point { x = 1 };
            let bad = p(2);
        }
        "#,
        "implement `Callable`",
    );
}

/// The struct TYPE NAME is not a value. `Scale(0.25)` keeps its own
/// "construct it" refusal even when `Scale` is `Callable` — the constructor
/// shape is the mistake being steered away from, and the marker does not make
/// a type into an instance. (The bare-name guard reaches this shape first; the
/// call-subject arm's own struct refusal is the other half, and neither moved.)
#[test]
fn a_callable_struct_type_name_keeps_the_construction_refusal() {
    let source = format!(
        r#"
        {SCALE_STEP}
        fun main() {{
            let bad = Scale(0.25);
        }}
        "#
    );
    assert_fails_with(
        &source,
        "`Scale` is a type, not a value; construct it (`Scale { .. }`) or call a static like \
         `Scale::new(..)`",
    );
    assert_fails_without(&source, "no method 'call'");
}

// --- Q1: the coercion (fn-coercion.md §1's coercible set) -----------------

#[test]
fn a_callable_coerces_into_an_annotated_closure_binding() {
    assert_compiles_and_runs(
        &format!(
            r#"
        import std::io::print;
        import std::display::Display;
        {SCALE_STEP}
        fun main() {{
            let space = Scale::new(0.25);
            let step: |f64| f64 = space;
            print(step(8f).to_string());
        }}
        "#
        ),
        "2\n",
    );
}

#[test]
fn a_callable_coerces_into_a_closure_parameter() {
    assert_compiles_and_runs(
        &format!(
            r#"
        import std::io::print;
        import std::display::Display;
        {SCALE_STEP}
        fun apply(f: |f64| f64, n: f64): f64 {{
            f(n)
        }}
        fun main() {{
            let space = Scale::new(0.25);
            print(apply(space, 4f).to_string());
        }}
        "#
        ),
        "1\n",
    );
}

#[test]
fn a_callable_coerces_into_a_generic_closure_argument() {
    assert_compiles_and_runs(
        &format!(
            r#"
        import std::io::print;
        import std::display::Display;
        {SCALE_STEP}
        fun main() {{
            let space = Scale::new(0.25);
            let scaled = [1f, 2f, 3f].map(space);
            for value in scaled {{
                print(value.to_string());
            }}
        }}
        "#
        ),
        "0.25\n0.5\n0.75\n",
    );
}

/// The coercion is by ARITY and TYPE, like the function one: a `Callable`
/// whose `call` does not match the slot is the ordinary mismatch, not a
/// silently-accepted wrap.
#[test]
fn a_callable_whose_call_does_not_match_the_slot_is_refused() {
    assert_fails(&format!(
        r#"
        {SCALE_STEP}
        fun main() {{
            let space = Scale::new(0.25);
            let step: |f64, f64| f64 = space;
        }}
        "#
    ));
}

// --- Q2: a `Callable` FIELD is called through the field -------------------

#[test]
fn a_callable_field_called_as_a_method_steers_to_parens() {
    assert_fails_with(
        &format!(
            r#"
        {SCALE_STEP}
        struct Theme {{
            space: Scale,
        }}
        fun main() {{
            let theme = Theme {{ space = Scale::new(0.25) }};
            let bad = theme.space(2f);
        }}
        "#
        ),
        "`space` is a field holding a closure or a `Callable`: parenthesize the field access to \
         call it, `(x.space)()`",
    );
}

#[test]
fn a_callable_field_is_called_through_the_parenthesized_field() {
    assert_compiles_and_runs(
        &format!(
            r#"
        import std::io::print;
        import std::display::Display;
        {SCALE_STEP}
        struct Theme {{
            space: Scale,
        }}
        fun main() {{
            let theme = Theme {{ space = Scale::new(0.25) }};
            print((theme.space)(2f).to_string());
        }}
        "#
        ),
        "0.5\n",
    );
}

// --- const parity ---------------------------------------------------------

/// A `Callable` value is plain DATA, so it is a const result today: the call
/// operator runs through the interpreter with no arm of its own, because the
/// lowering is the method call the analyzer already resolved.
#[test]
fn a_callable_call_folds_in_a_const_expression() {
    assert_emits_containing(
        &format!(
            r#"
        import std::io::print;
        import std::display::Display;
        {SCALE_STEP}
        fun main() {{
            let folded = const Scale::new(0.25)(4f);
            print(folded.to_string());
        }}
        "#
        ),
        "const folded = 1;",
    );
}

// --- B462: a tuple variant where a closure is expected --------------------------
//
// `Some` named without a call, at a position whose expected type is a closure,
// IS that closure — `|A, B| E<..>`, its generics taken from the expected type.
// Only a tuple variant, and only where a closure is expected (B348's rule
// shape); a payload variant named as a value anywhere else is refused (it
// compiled to the bare tag).

#[test]
fn b462_a_variant_passed_to_map_is_its_constructor() {
    assert_compiles_and_runs(
        r#"
        fun main() {
            let xs = [1, 2].map(Some);
            print(xs.len());
            print(xs[1].unwrap());
        }
        "#,
        "2\n2\n",
    );
}

#[test]
fn b462_the_generics_come_from_the_expected_closure_type() {
    assert_compiles_and_runs(
        r#"
        fun main() {
            let f: |i32| Option<i32> = Some;
            print(f(3).unwrap());
            let g: |str| Result<str, i32> = Ok;
            print(g("ok").unwrap());
        }
        "#,
        "3\nok\n",
    );
}

#[test]
fn b462_a_user_variant_with_two_payloads_coerces_to_a_two_parameter_closure() {
    assert_compiles_and_runs(
        r#"
        enum Shape {
            Rect(i32, i32),
            Dot,
        }
        fun build(make: |i32, i32| Shape): Shape {
            make(2, 3)
        }
        fun area(shape: Shape): i32 {
            match shape {
                Shape::Rect(let w, let h) => w * h,
                Shape::Dot => 0,
            }
        }
        fun main() {
            print(area(build(Shape::Rect)));
        }
        "#,
        "6\n",
    );
}

#[test]
fn b462_a_source_derivation_takes_the_variant() {
    // The sketch's `count.derive(Some)`, as A142 spells it (sealed with `.memo()`).
    // `derive`'s body is a `context`-typed closure parameter since A142 S2, and
    // B478 admits a variant there as at a plain closure position.
    assert_compiles_and_runs(
        r#"
        import std::reactive::SignalCell;
        fun main() {
            let count = SignalCell::new(4);
            let wrapped = count.derive(Some).memo();
            print(wrapped.get().unwrap());
            count.set(5);
            print(wrapped.get().unwrap());
        }
        "#,
        "4\n5\n",
    );
}

// --- B478: a named function or a variant at a `context`-typed closure ------------
// --- parameter coerces as at a plain one: the context is the CALLER's to supply. -

#[test]
fn b478_a_named_function_is_a_derive_body_and_an_effect_body() {
    assert_compiles_and_runs(
        r#"
        import std::reactive::{ Owner, SignalCell, Source, run_with_owner };
        fun show(value: i32) {
            print(i"show {value}");
        }
        fun double(value: i32): i32 {
            value * 2
        }
        fun main() {
            let count = SignalCell::new(4);
            let doubled = count.derive(double).memo();
            let owner = Owner::new();
            run_with_owner(owner, || {
                count.effect(show);
            });
            count.set(5);
            print(doubled.get());
            owner.dispose();
        }
        "#,
        "show 4\nshow 5\n10\n",
    );
}

/// The rule at the other three landings a clause has — a binding, a field
/// and a return — each handing a named function on to a call through the value.
#[test]
fn b478_a_named_function_lands_at_a_context_typed_binding_field_and_return() {
    assert_compiles_and_runs(
        r#"
        import std::context::Context;
        let current: Context<i32> = Context::new();
        fun shout(value: i32): i32 {
            value * 100
        }
        struct Held {
            body: (|i32| i32) context current,
        }
        fun make(): (|i32| i32) context current {
            shout
        }
        fun main() {
            let bound: (|i32| i32) context current = shout;
            let held = Held { body = shout };
            let made = make();
            current.run(7, || {
                print(bound(1) + (held.body)(2) + made(3));
            });
        }
        "#,
        "600\n",
    );
}

/// A function that READS one of the injected contexts is not admitted by the
/// rule: it is the context pass's own refusal of a context-reading function
/// used as a value, and a literal at the call is the spelling.
#[test]
fn b478_a_named_function_that_reads_an_injected_context_is_still_refused() {
    assert_fails_with(
        r#"
        import std::reactive::{ SignalCell, Source };
        let other: SignalCell<i32> = SignalCell::new(10);
        fun plus_other(value: i32): i32 {
            value + other.track()
        }
        fun main() {
            let count = SignalCell::new(4);
            let summed = count.derive(plus_other).memo();
            print(summed.get());
        }
        "#,
        "`plus_other` reads context `tracking`, so it can't be used as a value",
    );
}

/// A closure PARAMETER carries no clause, so it is not one of these — it is
/// still told what the position takes.
#[test]
fn b478_a_clause_less_closure_parameter_is_still_refused() {
    assert_fails_with(
        r#"
        import std::reactive::{ SignalCell, Source };
        fun relabel(count: SignalCell<i32>, label: |i32| str): str {
            count.derive(label).sample()
        }
        fun main() {
            print(relabel(SignalCell::new(1), |n| i"n={n}"));
        }
        "#,
        "a `context`-typed parameter takes a closure literal, a named function or variant",
    );
}

#[test]
fn b462_a_payload_variant_where_no_closure_is_expected_is_refused() {
    let refusal = "`Some` carries a payload, so it is not a value on its own: call it (`Some(x)`), \
                   or pass it where a closure is expected, where it stands for `|x| Some(x)`";
    assert_fails_with("fun main() {\n    let f = Some;\n}\n", refusal);
    assert_fails_with(
        "fun main() {\n    let o: Option<i32> = Some;\n    print(o.is_some());\n}\n",
        refusal,
    );
}

#[test]
fn b462_a_unit_variant_does_not_coerce() {
    assert_fails(
        r#"
        fun main() {
            let xs = [1, 2].map(None);
        }
        "#,
    );
}
