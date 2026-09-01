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
//! - B186: a TRAIT written as a PARAMETER's annotation is an IMPLICIT GENERIC
//!   — `fun f(x: Trait)` is `fun f<T: Trait>(x: T)`, appended after the
//!   written generics and monomorphized per call like any other. Return
//!   position, struct fields and every nested spelling stay refused.
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

// --- B186: a trait on a PARAMETER is an implicit generic --------------------
//
// The reactive paper's §7.3, ruled WANTED. `fun f(x: Trait)` reads as
// `fun f<T: Trait>(x: T)` — a generic parameter the author did not write,
// appended after the ones they did, monomorphized per call like any other.
//
// The difference from B161's `let` is worth stating, because the two are one
// family and their bodies do NOT see the same thing. A `let`'s initializer
// gives one concrete type at one site, so the binding keeps it and the body
// reads the impl's own surface. A parameter's body is checked ONCE for every
// call site, so what it sees is the BOUND — exactly what a written
// `<T: Trait>` sees. Both readings are "no widening, no `dyn`, static
// dispatch"; only the parameter's is quantified.

/// A parameterized trait over a generic cell, so the trait's ARGUMENT is what
/// separates one instantiation from another — the argument-binding pins.
const HOLDS: &str = r#"
    import std::io::print;
    trait Holds<T> { fun value(self): T; }
    struct Cell<T> { inner: T }
    impl Cell<type T> with Holds<T> { fun value(self): T { self.inner } }
"#;

#[test]
fn b186_a_trait_parameter_annotation_is_an_implicit_generic() {
    // The basic sugar: the position B161 left refused now compiles, and the
    // bound's member dispatches statically to the argument's own impl.
    assert_compiles_and_runs(
        &format!(
            r#"{GREET}
            fun describe(subject: Greet): str {{ subject.greet() }}
            fun main() {{
                print(describe(Dog {{ name = "rex" }}));
                print(describe(Fox {{ name = "vix" }}));
            }}
            main();
            "#
        ),
        "woof\nring\n",
    );
}

#[test]
fn b186_a_trait_parameter_body_sees_the_bound_not_the_argument() {
    // The quantified half, pinned so the family difference cannot drift: the
    // body is checked once against `Greet`, so `Dog`'s own field is NOT
    // readable through it — the same answer a written `<T: Greet>` gives.
    assert_fails_with(
        &format!(
            r#"{GREET}
            fun describe(subject: Greet): str {{ subject.name }}
            fun main() {{ print(describe(Dog {{ name = "rex" }})); }}
            main();
            "#
        ),
        "cannot access field 'name' on type Greet",
    );
}

#[test]
fn b186_an_argument_whose_type_lacks_the_trait_is_refused() {
    assert_fails_with(
        &format!(
            r#"{GREET}
            fun describe(subject: Greet): str {{ subject.greet() }}
            fun main() {{ print(describe(Cat {{ name = "tom" }})); }}
            main();
            "#
        ),
        "'Cat' does not implement trait 'Greet', required by a generic bound of this call",
    );
}

#[test]
fn b186_two_parameters_of_one_trait_are_independent_generics() {
    // §7.3's honest "against": `fun f(a: Greet, b: Greet)` is TWO type
    // parameters, so the two arguments may be different types. A function
    // that needs them equal must say so with one written generic.
    assert_compiles_and_runs(
        &format!(
            r#"{GREET}
            fun pair(a: Greet, b: Greet): str {{ a.greet() + "-" + b.greet() }}
            fun main() {{
                print(pair(Dog {{ name = "rex" }}, Fox {{ name = "vix" }}));
            }}
            main();
            "#
        ),
        "woof-ring\n",
    );
}

#[test]
fn b186_one_written_generic_still_forces_two_parameters_to_agree() {
    // The control for the pin above: the explicit spelling keeps its meaning,
    // so the escape hatch §7.3 promises the guide is really there.
    assert_fails(&format!(
        r#"{GREET}
        fun pair<T: Greet>(a: T, b: T): str {{ a.greet() + "-" + b.greet() }}
        fun main() {{
            print(pair(Dog {{ name = "rex" }}, Fox {{ name = "vix" }}));
        }}
        main();
        "#
    ));
}

#[test]
fn b186_a_parameterized_trait_parameter_binds_the_traits_arguments() {
    assert_compiles_and_runs(
        &format!(
            r#"{HOLDS}
            fun read(cell: Holds<i32>): i32 {{ cell.value() }}
            fun main() {{ print(read(Cell {{ inner = 7 }})); }}
            main();
            "#
        ),
        "7\n",
    );
}

#[test]
fn b186_a_parameterized_trait_parameter_checks_its_arguments() {
    assert_fails_with(
        &format!(
            r#"{HOLDS}
            fun read(cell: Holds<i32>): i32 {{ cell.value() }}
            fun main() {{ print(read(Cell {{ inner = "x" }})); }}
            main();
            "#
        ),
        "'Cell<str>' does not implement trait 'Holds<i32>'",
    );
}

#[test]
fn b186_the_sugar_mixes_with_written_generics() {
    assert_compiles_and_runs(
        &format!(
            r#"{GREET}
            fun tag<T: Greet>(label: T, subject: Greet): str {{
                label.greet() + "/" + subject.greet()
            }}
            fun main() {{
                print(tag(Dog {{ name = "rex" }}, Fox {{ name = "vix" }}));
            }}
            main();
            "#
        ),
        "woof/ring\n",
    );
}

#[test]
fn b186_an_implicit_generic_is_appended_after_the_written_ones() {
    // ORDERING, and the reason it is a pin of its own: the explicit
    // generic-argument spelling this language has is positional
    // (`tag<Dog, Fox>(..)`, no `::<>`), so WHERE the sugar's parameter lands
    // in the list is observable. Appended, `Dog` binds the written `T` and
    // `Fox` the implicit one, and the call runs. Prepended, `Dog` would bind
    // `subject` and `Fox` the label — and both arguments would be refused.
    assert_compiles_and_runs(
        &format!(
            r#"{GREET}
            fun tag<T: Greet>(label: T, subject: Greet): str {{
                label.greet() + "/" + subject.greet()
            }}
            fun main() {{
                print(tag<Dog, Fox>(Dog {{ name = "rex" }}, Fox {{ name = "vix" }}));
            }}
            main();
            "#
        ),
        "woof/ring\n",
    );
}

#[test]
#[ignore = "B186 found this and did not cause it: a PARTIAL generic-argument \
            list leaves the unsupplied parameters uninferred, and reaches \
            emission abstract. Pre-existing — it reproduces on two WRITTEN \
            generics with no sugar in sight — and reported as found-not-fixed \
            for an item of its own."]
fn a_partial_generic_argument_list_still_infers_the_rest() {
    // `tag<Dog>` on a `<T, U>` function supplies one of two. Supplying NONE
    // works (inference from the arguments) and supplying BOTH works; supplying
    // one leaves `U` abstract into emission, where it surfaces as the
    // "resolved to a requirement, which has no body" internal error. B186's
    // sugar reaches the same hole through `tag<Dog>(label, subject: Greet)`,
    // which is how it was found.
    assert_compiles_and_runs(
        &format!(
            r#"{GREET}
            fun tag<T: Greet, U: Greet>(label: T, subject: U): str {{
                label.greet() + "/" + subject.greet()
            }}
            fun main() {{
                print(tag<Dog>(Dog {{ name = "rex" }}, Fox {{ name = "vix" }}));
            }}
            main();
            "#
        ),
        "woof/ring\n",
    );
}

#[test]
fn b186_a_trait_on_a_closure_parameter_is_still_refused() {
    // A closure is not a declaration and has no generic list to append to, so
    // the sugar stops at the one position that can carry it.
    assert_fails_with(
        &format!(
            r#"{GREET}
            fun main() {{
                let describe = |subject: Greet| subject.greet();
                print(describe(Dog {{ name = "rex" }}));
            }}
            main();
            "#
        ),
        "'Greet' is a trait, not a type",
    );
}

#[test]
fn b186_the_refusal_at_the_other_positions_steers_to_the_sugar() {
    // One error identity, a steer that now names the position that WORKS.
    let steer = "a trait names a parameter's bound, not a value type";
    assert_fails_with(
        &format!(
            r#"{GREET}
            fun get(): Greet {{ Dog {{ name = "rex" }} }}
            fun main() {{ print(get().name); }}
            main();
            "#
        ),
        steer,
    );
    assert_fails_with(
        &format!(
            r#"{GREET}
            struct Kennel {{ inner: Greet }}
            fun main() {{ print(Kennel {{ inner = Dog {{ name = "rex" }} }}.inner.name); }}
            main();
            "#
        ),
        steer,
    );
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
        steer,
    );
}

#[test]
fn b186_a_nested_trait_spelling_on_a_parameter_is_still_refused() {
    // The sugar is the parameter's OWN annotation, not any trait spelled
    // under it — `List<Greet>` mints an inner type id the sugar never sees,
    // exactly as B161's nested case does.
    assert_fails_with(
        &format!(
            r#"{GREET}
            fun describe(pack: List<Greet>): i32 {{ pack.length() }}
            fun main() {{ print(describe([Dog {{ name = "a" }}])); }}
            main();
            "#
        ),
        "'Greet' is a trait, not a type",
    );
}

#[test]
fn a_view_annotation_is_transparent_to_the_trait_reading_at_both_positions() {
    // `&` is a CALL CONVENTION, not a type constructor: `walk_type_node`
    // erases it and returns the operand's own type id, so there is no
    // `Reference` type for a trait to be nested inside. `&Greet` is therefore
    // "a view of something implementing Greet" at both annotations, not a
    // nested spelling — the `let` half is B161 as shipped (the binding keeps
    // `Dog`, so `seen.name` reads), and the parameter half is B186 reading the
    // same annotation the same way. Pinned in one test because the two answers
    // have to agree: they are one erasure, and a change to it would move both.
    assert_compiles_and_runs(
        &format!(
            r#"{GREET}
            fun describe(subject: &Greet): str {{ subject.greet() }}
            fun main() {{
                let dog = Dog {{ name = "rex" }};
                let seen: &Greet = & dog;
                print(seen.greet());
                print(seen.name);
                print(describe(& dog));
            }}
            main();
            "#
        ),
        "woof\nrex\nwoof\n",
    );
}

// --- B186: the emission census -----------------------------------------------
//
// §7.3's honest "against": the sugar makes a function SILENTLY generic, so the
// paper asks what that costs in emitted copies. The answer is that it costs
// exactly what writing the generic out costs — the sugar is a surface, and
// M16's emitted-body sharing does the rest. Both halves are pinned: a
// T-dependent body still gets one copy per type, a T-independent one still
// gets one copy in total, and the counts match the written spelling's.

/// The three estate sites the census was taken on, shaped like `std::ui`'s
/// A33-widened bindings (`fun bind_text<S: Source<str>>(self, source: S)`):
/// a T-DEPENDENT body (the bound's member is resolved per impl), a
/// T-INDEPENDENT one, and a two-parameter site. `{bound}` is spliced with the
/// written spelling or with the sugar, and nothing else differs.
fn census_source(bound: &str) -> String {
    let (declaration, annotation) = match bound {
        "written" => ("<S: Greet>", "S"),
        _ => ("", "Greet"),
    };
    format!(
        r#"{GREET}
        // Site 1 — T-DEPENDENT: `greet` resolves to a different function per impl.
        fun bind_text{declaration}(source: {annotation}): str {{
            let painted = source.greet();
            "[" + painted + "]"
        }}
        // Site 2 — T-INDEPENDENT: the body never mentions the bound.
        fun bind_attr{declaration}(name: str, _source: {annotation}): str {{
            let attribute = name + "=1";
            attribute
        }}
        // Site 3 — two sites of the same shape, to show the count is per TYPE
        // and not per call.
        fun bind_class{declaration}(source: {annotation}): str {{
            let classed = source.greet();
            classed + "!"
        }}
        fun main() {{
            let dog = Dog {{ name = "rex" }};
            let fox = Fox {{ name = "vix" }};
            print(bind_text(dog));
            print(bind_text(fox));
            print(bind_attr("a", dog));
            print(bind_attr("b", fox));
            print(bind_class(dog));
            print(bind_class(fox));
        }}
        main();
        "#
    )
}

#[test]
fn b186_the_sugar_emits_exactly_what_the_written_generic_emits() {
    let sugared = compile(&census_source("sugar")).expect("the sugar compiles");
    let written = compile(&census_source("written")).expect("the written form compiles");

    // T-dependent: one copy per type, both spellings.
    assert_eq!(
        emitted_bodies_containing(&sugared, "const painted ="),
        2,
        "a T-dependent sugared body monomorphizes per type:\n{sugared}"
    );
    assert_eq!(
        emitted_bodies_containing(&written, "const painted ="),
        emitted_bodies_containing(&sugared, "const painted ="),
        "the sugar must cost what the written generic costs"
    );

    // T-independent: M16 shares ONE body across both types, both spellings.
    assert_eq!(
        emitted_bodies_containing(&sugared, "const attribute ="),
        1,
        "a T-independent sugared body is shared by M16:\n{sugared}"
    );
    assert_eq!(
        emitted_bodies_containing(&written, "const attribute ="),
        emitted_bodies_containing(&sugared, "const attribute ="),
        "the sugar must cost what the written generic costs"
    );

    assert_eq!(
        emitted_bodies_containing(&sugared, "const classed ="),
        2,
        "the third site monomorphizes per type, not per call:\n{sugared}"
    );
    assert_eq!(
        emitted_bodies_containing(&written, "const classed ="),
        emitted_bodies_containing(&sugared, "const classed ="),
        "the sugar must cost what the written generic costs"
    );
}

#[test]
fn b186_the_sugared_estate_runs() {
    // The emission counts cannot say the shared body is CORRECT at every type
    // it was shared across; running it can.
    assert_compiles_and_runs(
        &census_source("sugar"),
        "[woof]\n[ring]\na=1\nb=1\nwoof!\nring!\n",
    );
}

#[test]
fn b186_a_kolt_shaped_view_extension_takes_a_source_parameter() {
    // The exhibit the owner will write next: a `View` extension bound on
    // `Source<i32>` without a `<S: ..>` list, against the real `std::ui` and
    // the real `std::reactive` — the shape A33 widened `bind_text` into, now
    // spelled the way §7.3 says it should be.
    assert_compiles_browser(
        r#"
        import std::ui::{ View, view, mount_root };
        import std::reactive::{ Signal, SignalCell, Source, Subscription, observe };
        import std::display::Display;

        // A user's own `Source` (A33's motivating shape), so the extension is
        // exercised at two unrelated implementations of the trait.
        struct Doubled { inner: SignalCell<i32> }
        impl Doubled with Source<i32> {
            fun get(self): i32 { self.inner.get() * 2 }
            [must_use]
            fun sub(self, observer: |i32| void): Subscription {
                let subscription = observe(self.inner, |value| { observer(value * 2); });
                observer(self.get());
                subscription
            }
        }

        impl View {
            fun on_interact(self, source: Source<i32>): View {
                let element = self.element;
                source.effect(|value| {
                    element.set_attribute("data-count", value.to_string());
                });
                self
            }
        }

        fun main() {
            let _owner = mount_root("app", || {
                let count = Signal::new(0);
                let doubled = Doubled { inner = count };
                view("div").on_interact(count).on_interact(doubled)
            });
        }
        "#,
    );
}
