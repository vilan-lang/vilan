//! Traits as a NAMESPACE and as a CONSTRAINT — the two halves of the trait
//! surface that are not method dispatch.
//!
//! - B162: a trait may declare ASSOCIATED FUNCTIONS (no `self` receiver), with
//!   default bodies. `Trait::func(..)` calls the trait's own default body;
//!   `Type::func(..)` calls that impl's override, as it always has.
//! - B161: a TRAIT written as a `let` binding's annotation is a CHECKED
//!   CONSTRAINT on the binding's inferred type, not the binding's type — no
//!   `dyn`, no widening. The binding keeps its concrete type; the annotation
//!   only checks that type implements the trait.
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

// --- B162: associated functions on a trait ---

// The shape the feature was ruled for, one step from the reactive exhibit: the
// canonical-cell constructor lives ON the trait, with a default body returning
// the canonical impl, so `Signal::new(v)` keeps its spelling while `Signal`
// becomes a trait.
const SIGNAL: &str = r#"
    import std::io::print;
    struct SignalCell<T> { value: T }
    struct OtherSignal<T> { value: T }
    trait Signal<T> {
        fun new(initial: T): SignalCell<T> { SignalCell { value = initial } }
    }
    impl SignalCell<type T> with Signal<T> {}
    impl OtherSignal<type T> with Signal<T> {}
    impl SignalCell<type T> {
        fun new(initial: T): SignalCell<T> { SignalCell { value = initial } }
    }
    impl OtherSignal<type T> {
        fun new(initial: T): OtherSignal<T> { OtherSignal { value = initial } }
    }
"#;

// A trait whose default body and whose impl's override are TELLABLE APART in
// the output — the ruled resolution is about which of two bodies runs, so it
// can only be pinned by two bodies that say different things.
const MAKER: &str = r#"
    import std::io::print;
    trait Maker {
        fun make(): str { "trait default" }
    }
    struct Boxed { tag: str }
    impl Boxed with Maker {
        fun make(): str { "impl override" }
    }
"#;

#[test]
fn a_traits_associated_function_is_callable_on_the_trait() {
    assert_compiles_and_runs(
        &format!(
            r#"{SIGNAL}
            fun main() {{
                let cell = Signal::new(7);
                print(cell.value);
            }}
            main();
            "#
        ),
        "7\n",
    );
}

#[test]
fn the_trait_path_reaches_the_traits_own_body_never_an_impls_override() {
    // The ruled resolution, both ways in ONE program: `Trait::func` is the
    // trait's default body even though an impl overrides it, and that impl's
    // override is reached through its own type's path.
    assert_compiles_and_runs(
        &format!(
            r#"{MAKER}
            fun main() {{
                print(Maker::make());
                print(Boxed::make());
            }}
            main();
            "#
        ),
        "trait default\nimpl override\n",
    );
}

#[test]
fn a_traits_associated_function_binds_its_generic_from_the_call() {
    // The trait's own parameter `T` is bound by the argument, the way a
    // generic function's is — the associated function is a namespaced static,
    // not a dispatch.
    assert_compiles_and_runs(
        &format!(
            r#"{SIGNAL}
            fun main() {{
                print(Signal::new("hi").value);
            }}
            main();
            "#
        ),
        "hi\n",
    );
}

#[test]
fn an_associated_function_is_reached_through_a_supertrait() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Base { fun tag(): str { "base" } }
        trait Sub with Base {}
        fun main() { print(Sub::tag()); }
        main();
        "#,
        "base\n",
    );
}

#[test]
fn an_associated_function_without_a_default_body_is_refused_on_the_trait_path() {
    // The trait declares the requirement but has no body behind it, so
    // `Trait::func(..)` would resolve to a signature — B55's internal error
    // route, refused here from day one. Both spellings are named: the one that
    // works today, and the one that would make this call work.
    let source = r#"
        import std::io::print;
        trait Maker {
            fun make(): str;
        }
        struct Boxed { tag: str }
        impl Boxed with Maker {
            fun make(): str { "impl" }
        }
        fun main() { print(Maker::make()); }
        main();
        "#;
    assert_fails_with(source, "'Maker::make' has no default body");
    assert_fails_with(source, "'<Type>::make(..)'");
    assert_fails_with(source, "give 'make' a default body on 'Maker'");
}

#[test]
fn a_type_path_without_an_override_steers_to_the_trait_spelling() {
    // An associated function has no receiver, so a default body is not
    // inherited onto the implementing type's path the way a `self` method's
    // is. The refusal names the path that does reach it.
    assert_fails_with(
        r#"
        import std::io::print;
        trait Maker { fun make(): str { "d" } }
        struct Boxed { tag: str }
        impl Boxed with Maker {}
        fun main() { print(Boxed::make()); }
        main();
        "#,
        "call 'Maker::make(..)'",
    );
}

#[test]
fn an_unknown_name_on_a_trait_path_is_still_not_found() {
    // The new resolution adds a tier; it must not swallow the plain miss.
    assert_fails_with(
        r#"
        import std::io::print;
        trait Maker { fun make(): str { "d" } }
        fun main() { print(Maker::bake()); }
        main();
        "#,
        "cannot find 'bake' in Maker",
    );
}

// --- B161: a trait annotation as a checked constraint on a binding ---

const GREET: &str = r#"
    import std::io::print;
    trait Greet { fun greet(self): str; }
    struct Dog { name: str }
    struct Cat { name: str }
    struct Fox { name: str }
    impl Dog with Greet { fun greet(self): str { "woof" } }
    impl Fox with Greet { fun greet(self): str { "ring" } }
"#;

#[test]
fn a_trait_annotation_keeps_the_bindings_concrete_type() {
    // Not a `dyn` and not a widening: `d` is a `Dog`, so its own field is
    // readable and its `greet` is the statically resolved one. A widening
    // would have erased both.
    assert_compiles_and_runs(
        &format!(
            r#"{GREET}
            fun main() {{
                let d: Greet = Dog {{ name = "rex" }};
                print(d.name);
                print(d.greet());
            }}
            main();
            "#
        ),
        "rex\nwoof\n",
    );
}

#[test]
fn a_binding_whose_type_lacks_the_annotated_trait_is_refused() {
    let source = format!(
        r#"{GREET}
        fun main() {{
            let d: Greet = Cat {{ name = "tom" }};
            print(d.name);
        }}
        main();
        "#
    );
    // The caret is on the CONSTRAINT — the annotation is what failed. (The
    // fourth `Greet` in the source: the declaration, two `with` clauses, then
    // the annotation.)
    assert_fails_spanning_nth(&source, "Greet", 3, "does not implement trait 'Greet'");
    assert_fails_with(&source, "a trait annotation on a binding is a CONSTRAINT");
}

#[test]
fn a_parameterized_trait_annotation_checks_its_arguments() {
    assert_fails_with(
        &format!(
            r#"{SIGNAL}
            fun main() {{
                let cell: Signal<str> = SignalCell::new(1);
                print(cell.value);
            }}
            main();
            "#
        ),
        "'SignalCell<i32>' does not implement trait 'Signal<str>'",
    );
}

#[test]
fn a_binding_annotated_with_a_trait_a_std_type_implements_is_accepted() {
    // UNIVERSAL: every trait name in the position gets this reading, std's
    // included — there is no per-trait list.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::Display;
        fun main() {
            let n: Display = 5;
            print(n + 1);
        }
        main();
        "#,
        "6\n",
    );
}

#[test]
fn two_if_arms_of_the_same_type_satisfy_a_trait_annotation() {
    // The owner's legal example: the arms unify to ONE concrete type, and the
    // annotation meets that type.
    assert_compiles_and_runs(
        &format!(
            r#"{SIGNAL}
            fun choose(condition: bool): i32 {{
                let cell: Signal<i32> =
                    if condition {{ SignalCell::new(1) }} else {{ SignalCell::new(2) }};
                cell.value
            }}
            fun main() {{ print(choose(true)); }}
            main();
            "#
        ),
        "1\n",
    );
}

#[test]
fn two_if_arms_of_different_types_fail_at_unification_not_at_the_trait() {
    // The owner's illegal example. BOTH arms implement `Signal<i32>` — and it
    // is still refused, because the annotation is not a widening and there is
    // no one type for the arms to meet in. The report is the ordinary
    // mismatch, at the arms, and the trait is never consulted: a trait error
    // here would say the impl was missing when it is not.
    let source = format!(
        r#"{SIGNAL}
        fun choose(condition: bool): i32 {{
            let cell: Signal<i32> =
                if condition {{ SignalCell::new(1) }} else {{ OtherSignal::new(2) }};
            cell.value
        }}
        fun main() {{ print(choose(true)); }}
        main();
        "#
    );
    assert_fails_with(&source, "`if` arms have mismatched types");
    assert_fails_without(&source, "does not implement trait");
    // And the annotation itself is READ, not refused: the old
    // trait-is-not-a-type report at this position is gone.
    assert_fails_without(&source, "is a trait, not a type");
}

#[test]
fn a_trait_annotation_is_not_a_widening_for_reassignment_either() {
    // The binding's type is `Dog`, so a `Fox` cannot be stored in it — even
    // though `Fox` implements the annotated trait. One concrete type per
    // binding, checked wide, kept narrow.
    let source = format!(
        r#"{GREET}
        fun main() {{
            mut d: Greet = Dog {{ name = "rex" }};
            d = Fox {{ name = "f" }};
            print(d.name);
        }}
        main();
        "#
    );
    assert_fails_with(&source, "Expected Dog, but got Fox instead.");
    // The annotation was READ as a constraint (it did not refuse), and the
    // type it left on the binding is the initializer's — which is what makes
    // the reassignment the only report.
    assert_fails_without(&source, "is a trait, not a type");
}

#[test]
fn a_bounded_generic_satisfies_a_trait_annotation() {
    // The same `satisfies_trait_bound` a call's bound goes through: a generic
    // parameter satisfies the constraint through its own declared bound.
    assert_compiles_and_runs(
        &format!(
            r#"{GREET}
            fun describe<T: Greet>(subject: T): str {{
                let inner: Greet = subject;
                inner.greet()
            }}
            fun main() {{ print(describe(Dog {{ name = "rex" }})); }}
            main();
            "#
        ),
        "woof\n",
    );
}

// --- B161: NARROWED, not repealed — every other position still refuses ---

#[test]
fn a_trait_nested_in_a_binding_annotation_is_still_refused() {
    // §12.2's silently heterogeneous `List<Trait>`: the constraint reading is
    // the binding's OWN annotation, not any trait spelled anywhere under it.
    assert_fails_with(
        &format!(
            r#"{GREET}
            fun main() {{
                let pack: List<Greet> = [Dog {{ name = "a" }}];
                print(pack.length());
            }}
            main();
            "#
        ),
        "'Greet' is a trait, not a type",
    );
}

#[test]
fn a_trait_in_parameter_position_is_still_refused() {
    assert_fails_with(
        &format!(
            r#"{GREET}
            fun describe(subject: Greet): str {{ subject.greet() }}
            fun main() {{ print(describe(Dog {{ name = "rex" }})); }}
            main();
            "#
        ),
        "'Greet' is a trait, not a type",
    );
}

#[test]
fn a_trait_in_return_position_is_still_refused() {
    assert_fails_with(
        &format!(
            r#"{GREET}
            fun get(): Greet {{ Dog {{ name = "rex" }} }}
            fun main() {{ print(get().name); }}
            main();
            "#
        ),
        "'Greet' is a trait, not a type",
    );
}

#[test]
fn a_trait_in_struct_field_position_is_still_refused() {
    assert_fails_with(
        &format!(
            r#"{GREET}
            struct Kennel {{ inner: Greet }}
            fun main() {{ print(Kennel {{ inner = Dog {{ name = "rex" }} }}.inner.name); }}
            main();
            "#
        ),
        "'Greet' is a trait, not a type",
    );
}
