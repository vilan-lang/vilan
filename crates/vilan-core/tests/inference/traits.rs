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

/// kolt's `store.vl:27`, retyped: the hand-written two-parameter conditional
/// `Wire` impl the marker sweep found, variant-tagged exactly as `Option`'s
/// is. Shared by the pins below so each exercises the SAME impl the field
/// report named.
///
/// It is written over a USER enum rather than over `Result` on purpose. kolt's
/// was a `Result` impl, and A82 is std adopting it — so a pin that hand-wrote
/// one would be a duplicate impl the day A82 landed, and would have been
/// testing "std has no impl for this" rather than "a hand-written impl is
/// read". `Outcome` is `Result` in all but name and belongs to nobody, which
/// is what makes these pins about the PREDICATE.
const OUTCOME_WIRE_IMPL: &str = r#"
        enum Outcome<T, E> {
            Good(T),
            Bad(E),
        }

        impl Outcome<type T: Wire, type E: Wire> with Wire {
            fun describe<S: Serialize>(self, serializer: &mut S) {
                match self {
                    Outcome::Good(let value) => {
                        serializer.begin_variant("Good", 1);
                        value.describe(serializer);
                        serializer.end_variant();
                    },
                    Outcome::Bad(let error) => {
                        serializer.begin_variant("Bad", 1);
                        error.describe(serializer);
                        serializer.end_variant();
                    },
                }
            }

            fun rebuild<D: Deserialize>(deserializer: &mut D): Outcome<T, E> {
                let tag = deserializer.variant_tag();
                match tag {
                    "Good" => {
                        deserializer.begin_variant("Good", 1);
                        let value = T::rebuild(deserializer);
                        deserializer.end_variant();
                        Outcome::Good(value)
                    },
                    _ => {
                        deserializer.begin_variant("Bad", 1);
                        let error = E::rebuild(deserializer);
                        deserializer.end_variant();
                        Outcome::Bad(error)
                    },
                }
            }
        }
"#;

/// A trait, two implementations, and a struct that names the trait at a field.
/// The paper's R-programs are written against exactly this head.
const HIDDEN: &str = r#"
    import std::io::print;
    trait X { fun who(self): str; }
    struct A {}
    impl A with X { fun who(self): str { "A" } }
    struct B {}
    impl B with X { fun who(self): str { "B" } }
    struct C { x: X }
"#;

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

// --- B182: the refusal's `Unknown` carries its provenance, so it stands alone ---
//
// B161 resolves a refused annotation to `Unknown` "so the one report stands
// alone instead of cascading". It did not: `Unknown` says nothing about WHY,
// so every use of the refused thing filed its own report in the vocabulary of
// a type nobody wrote. kolt's two `[expose] … : Signal<…>` fields produced 53
// diagnostics for two mistakes. The slot is now the provenance, and the checks
// that meet it stand down — the b154 family's move, one mistake to one report.

#[test]
fn one_refused_expose_field_alone_is_one_diagnostic() {
    // kolt's 53-error pile in its SMALLEST form: one `[expose]` field, no
    // method bodies, no second offense, nothing else in the file. It reported
    // three — the refusal plus the expansion's two unbindable generics — with
    // the root printed LAST, which is the whole exhibit in miniature and the
    // cheapest thing to keep red if any of the three parts regresses.
    let source = r#"
        import std::io::print;
        import std::reactive::Signal;
        [service(TestClient)]
        struct TestStore {
            [expose] items: Signal<List<i32>>,
        }
        fun main() { print(1); }
        main();
        "#;
    let diagnostics = failure_diagnostics(source);
    assert_eq!(
        diagnostics.len(),
        1,
        "one refused annotation is one diagnostic: {diagnostics:#?}"
    );
    assert!(
        diagnostics[0].0.contains("'Signal' is a trait, not a type"),
        "and it is the refusal itself: {diagnostics:#?}"
    );
}

#[test]
fn a_refused_field_does_not_cascade_through_its_uses() {
    // The refusal, and nothing else. Every use of the field is a use of a slot
    // one diagnostic has already accounted for: the method call would have
    // said "cannot call method 'greet' on unknown" and the field read "cannot
    // access field 'name' on ...", both of which name a type the author never
    // wrote and neither of which is a fix.
    //
    // Read at a CLOSURE parameter since B184: the field this was written on is
    // the hidden parameter now, and a closure — which has no generic list to
    // append an implicit parameter to — is the remaining position whose own
    // slot is the refused annotation and is read straight back by the body.
    let source = format!(
        r#"{GREET}
        fun main() {{
            let describe = |subject: Greet| subject.greet() + subject.name;
            print(describe(Dog {{ name = "rex" }}));
        }}
        main();
        "#
    );
    assert_fails_once_with(&source, "'Greet' is a trait, not a type");
    assert_fails_without(&source, "on unknown");
    assert_fails_without(&source, "cannot access field");
    let diagnostics = failure_diagnostics(&source);
    assert_eq!(
        diagnostics.len(),
        1,
        "one refused annotation is one diagnostic: {diagnostics:#?}"
    );
}

#[test]
fn an_unrelated_unknown_still_reports_beside_a_refused_one() {
    // E104's lesson, at this family's grain: the stand-down is asked PER
    // RECEIVER, of the slot that receiver reads — never of "is this type
    // unknown", which would silence a mistake nobody has been told about.
    //
    // The unrelated unknown is B188's: `Holder` is written with no type
    // argument, the arity refusal reports that, and the slot resolves to
    // `Unknown` under a rule this family knows nothing about. So a call on it
    // still refuses — in the same program whose `Kennel` field stands down.
    //
    // It used to be `thing: Nope`, an UNRESOLVED name. B189's first sibling
    // gave that arm the same provenance the bare-trait arm has (a `cannot find
    // type` refusal is a root like any other), which made the old shape
    // vacuous: the pin would have passed by silencing both. The rule for
    // choosing a replacement is the one that made this pin worth having —
    // the unknown has to come from a refusal this map does not carry.
    //
    // The refused annotation is a CLOSURE parameter since B184, for the reason
    // the pin above gives: a struct field names the hidden parameter now.
    let source = format!(
        r#"{GREET}
        struct Holder<T> {{ v: T }}
        struct Other {{ held: Holder }}
        fun main() {{
            let speak = |subject: Greet| subject.greet();
            let other = Other {{ held = 1 }};
            print(other.held.length());
        }}
        main();
        "#
    );
    assert_fails_with(&source, "'Greet' is a trait, not a type");
    assert_fails_with(&source, "`Holder` takes 1 type argument, 0 given");
    assert_fails_with(&source, "cannot call method 'length' on unknown");
    // And the refused field's own use is still silent — the two answers are
    // independent, which is the whole point of keying on the slot.
    assert_fails_without(&source, "'greet' on unknown");
}

// --- B189: three siblings B182's stand-down did not reach --------------------
//
// Same pile, three provenances. B182 gave the bare-trait refusal's `Unknown`
// slot a provenance and stood three consumers down on it; each sibling below is
// a report that reaches its subject by a route the SLOT does not travel — a
// second refusal arm that was never instrumented, a derive that templates from
// the annotation's SPELLING, and a generated call whose argument has a
// perfectly good type and a refused EXPOSURE.

#[test]
fn an_unresolved_field_annotation_does_not_cascade_through_its_uses() {
    // The first sibling. `cannot find type 'Nope'` is a refusal exactly as
    // "`Greet` is a trait, not a type" is, and it resolved to `Unknown` one
    // match arm below the instrumented one — so every use of the field said
    // "cannot call method 'length' on unknown", a second report about a type
    // nobody wrote and no more a fix than the bare-trait cascade was.
    let source = r#"
        import std::io::print;
        struct Holder { inner: Nope }
        fun main() {
            let holder = Holder { inner = 1 };
            print(holder.inner.length());
        }
        main();
        "#;
    assert_fails_once_with(source, "cannot find type 'Nope'");
    assert_fails_without(source, "on unknown");
    let diagnostics = failure_diagnostics(source);
    assert_eq!(
        diagnostics.len(),
        1,
        "one unresolved annotation is one diagnostic: {diagnostics:#?}"
    );
}

#[test]
fn a_derive_over_a_refused_field_writes_no_follow_ons() {
    // The second sibling. `[derive(Wire)]` builds its bodies out of the field's
    // ANNOTATION TEXT — `{type}::from_json_value(..)`, `{type}::rebuild(..)` —
    // and the text survives the refusal, so the generated code asked the TRAIT
    // for members no trait has. Those are not the annotation's slot: they are
    // fresh paths with type ids of their own, which is why B182's provenance
    // could not reach them and why this one is filed on the trait the spelling
    // names.
    //
    // The derive's own field check goes with them: "`inner` is `Greet`, which
    // is not Wire" answers a question nobody asked — `Greet` is not a field
    // type at all, which the author has already been told.
    let source = format!(
        r#"{GREET}
        [derive(Wire)]
        struct Kennel {{ inner: Greet }}
        fun main() {{ print(1); }}
        main();
        "#
    );
    assert_fails_once_with(&source, "'Greet' is a trait, not a type");
    assert_fails_without(&source, "from_json_value");
    assert_fails_without(&source, "rebuild");
    assert_fails_without(&source, "which is not Wire");
    let diagnostics = failure_diagnostics(&source);
    assert_eq!(
        diagnostics.len(),
        1,
        "one refused annotation is one diagnostic: {diagnostics:#?}"
    );
}

#[test]
fn a_static_miss_on_an_unrefused_trait_still_reports_beside_a_refused_one() {
    // E104's lesson for the second sibling, keyed where that sibling keys: on
    // the TRAIT. `Ping` was never written as a field type, so nobody has been
    // told anything about it, and asking it for a member it does not declare is
    // a mistake of its own — in the same program whose `Greet` paths are
    // silent.
    let source = format!(
        r#"{GREET}
        trait Ping {{ fun ping(self): str; }}
        [derive(Wire)]
        struct Kennel {{ inner: Greet }}
        fun main() {{ print(Ping::pong()); }}
        main();
        "#
    );
    assert_fails_with(&source, "'Greet' is a trait, not a type");
    assert_fails_with(&source, "cannot find 'pong' in Ping");
    assert_fails_without(&source, "in Greet");
}

#[test]
fn an_expose_of_a_non_wire_element_reports_once() {
    // The third sibling, and the one whose subject is not an `Unknown` at all:
    // `SignalCell<List<Workspace>>` is a perfectly good type, and it is the
    // EXPOSURE that is refused. The `[service]` expansion then writes two
    // shapes that fail the same `Wire` bound — the server's
    // `session.expose(self.workspaces)` and the client's mirror,
    // `RemoteSource<List<Workspace>>` — and both reported it again, over a span
    // covering the whole struct, as "in code generated by this attribute".
    let source = r#"
        import std::io::print;
        import std::reactive::SignalCell;
        struct Workspace { id: i32 }
        [service(KoltClient)]
        struct KoltStore {
            [expose] workspaces: SignalCell<List<Workspace>>,
        }
        fun main() { print("kolt"); }
        main();
        "#;
    assert_fails_once_with(source, "its element `List<Workspace>` is not Wire");
    assert_fails_without(source, "in code generated by this attribute");
    let diagnostics = failure_diagnostics(source);
    assert_eq!(
        diagnostics.len(),
        1,
        "one refused exposure is one diagnostic: {diagnostics:#?}"
    );
}

#[test]
fn an_author_written_bound_failure_on_the_same_type_still_reports() {
    // E104's lesson for the third sibling. The covered set covers the
    // EXPOSURE — the calls the expansion writes — and nothing else. A call the
    // author wrote themselves that fails the same bound is their own site, in
    // their own file, and it still reports.
    let source = r#"
        import std::io::print;
        import std::reactive::SignalCell;
        import std::wire::Wire;
        struct Workspace { id: i32 }
        fun send<T: Wire>(value: T): i32 { 1 }
        [service(KoltClient)]
        struct KoltStore {
            [expose] workspaces: SignalCell<List<Workspace>>,
        }
        fun main() {
            let one: List<Workspace> = [Workspace { id = 1 }];
            print(i"{send(one)}");
        }
        main();
        "#;
    assert_fails_with(source, "its element `List<Workspace>` is not Wire");
    assert_fails_with(
        source,
        "'List<Workspace>' does not implement trait 'Wire', required by a generic bound",
    );
}

/// kolt's own shape, one file: a `[service]` struct whose fields are `[expose]`d
/// and whose `[rpc]` bodies write them through the reactive setter. The first
/// field's annotation is the mistake the exhibit was — a bare `Signal<…>` where
/// the cell type belongs. The second is a SEPARATE offense (a `Workspace` that
/// is not Wire) written into the same struct on purpose: the stand-down must
/// not take it with the first.
const SERVICE_EXHIBIT: &str = r#"
    import std::io::print;
    import std::reactive::{ Signal, SignalCell };
    struct Workspace { id: i32 }
    [service(KoltClient)]
    struct KoltStore {
        [expose] tasks: Signal<List<i32>>,
        [expose] workspaces: SignalCell<List<Workspace>>,
    }
    impl KoltStore {
        [rpc]
        fun add_task(self, id: i32): i32 {
            self.tasks.set_with(|list| {
                mut updated = list;
                updated.push(id);
                updated
            });
            2
        }
    }
    fun main() { print("kolt"); }
    main();
"#;

#[test]
fn a_refused_field_under_service_generation_produces_no_generated_code_follow_ons() {
    // The exhibit's loudest voices, all of them consequences of the annotation
    // one line up: the setter call on a receiver with no type, and the
    // expansion's own `expose` call, whose generics cannot bind because the
    // value it is handed has no type. Both said so in the vocabulary of code
    // the author never wrote — "in code generated by this attribute" over a
    // span covering the whole struct — which reads as a compiler fault.
    assert_fails_once_with(SERVICE_EXHIBIT, "'Signal' is a trait, not a type");
    assert_fails_without(SERVICE_EXHIBIT, "on unknown");
    assert_fails_without(SERVICE_EXHIBIT, "cannot infer");
    assert_fails_without(SERVICE_EXHIBIT, "cannot be checked");
}

#[test]
fn the_stand_down_does_not_hide_a_second_independent_offense() {
    // E104's lesson. `workspaces` is not the refused field and its problem is
    // nobody's consequence: `Workspace` is not Wire, so exposing a
    // `List<Workspace>` is its own mistake and the author has been told
    // nothing about it. A stand-down asked once for the whole struct would
    // have swallowed it.
    assert_fails_with(SERVICE_EXHIBIT, "is not Wire");
}

#[test]
fn the_kolt_shaped_exhibit_is_one_diagnostic_per_mistake() {
    // The exhibit's whole point, stated as a NUMBER so a regression in any of
    // the parts shows up here rather than in a lane's reading of a log. Two
    // mistakes were written into this struct on purpose — a bare `Signal`
    // where the cell type belongs, and a `Workspace` that is not Wire — and
    // after B189 there are exactly two diagnostics. It was 21 before B182, 4
    // after it (the second mistake still restated at the whole struct's span
    // by both halves of the expansion), and 2 now.
    let diagnostics = failure_diagnostics(SERVICE_EXHIBIT);
    assert_eq!(
        diagnostics.len(),
        2,
        "two mistakes, two diagnostics: {diagnostics:#?}"
    );
}

/// A `[service]` whose refused field is joined by an INDEPENDENT generated
/// failure: the `[rpc]` method takes a `Workspace`, which is not Wire, so the
/// expansion's own encode/decode code fails at the whole declaration's span.
/// The ordering pin below needs exactly that shape — a diagnostic that ENCLOSES
/// the refusal — and `SERVICE_EXHIBIT` stopped supplying one when B189's third
/// sibling stood the exposure's generated restatements down. The enclosing
/// diagnostics here are nobody's consequence, so they stay, and the rule they
/// were always about (roots first) is still under test.
const ORDERING_EXHIBIT: &str = r#"
    import std::io::print;
    import std::reactive::Signal;
    struct Workspace { id: i32 }
    [service(KoltClient)]
    struct KoltStore {
        [expose] tasks: Signal<List<i32>>,
    }
    impl KoltStore {
        [rpc]
        fun touch(self, w: Workspace): i32 { 1 }
    }
    fun main() { print("kolt"); }
    main();
"#;

#[test]
fn a_refusal_that_stood_something_down_prints_before_what_encloses_it() {
    // The ordering rule (`Program::normalize_diagnostic_order`). A generated
    // diagnostic re-anchors at the WHOLE declaration (standard A2), whose span
    // opens before the field annotation inside it — so plain positional order
    // printed the consequence first and buried the cause. kolt's owner read
    // "cannot infer 'S'" and never reached the refused field two pages down.
    let diagnostics = failure_diagnostics(ORDERING_EXHIBIT);
    let refusal = diagnostics
        .iter()
        .position(|(message, _)| message.contains("'Signal' is a trait, not a type"))
        .unwrap_or_else(|| panic!("expected the refusal: {diagnostics:#?}"));
    let refusal_span = diagnostics[refusal].1.clone();
    let enclosing: Vec<usize> = diagnostics
        .iter()
        .enumerate()
        .filter(|(_, (_, span))| {
            span.start <= refusal_span.start
                && span.end >= refusal_span.end
                && *span != refusal_span
        })
        .map(|(index, _)| index)
        .collect();
    assert!(
        !enclosing.is_empty(),
        "the pin needs a diagnostic that encloses the refusal, or it proves nothing: \
         {diagnostics:#?}"
    );
    for index in enclosing {
        assert!(
            refusal < index,
            "the refusal must print before {:?}, which encloses it: {diagnostics:#?}",
            diagnostics[index].0
        );
    }
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

// --- B192: a PARTIAL generic-argument list binds positionally, inference the rest ---
//
// The written list is a PREFIX, not the whole binding. `tag<Dog>` on a
// `<T, U>` function fixes `T` and leaves `U` to the arguments, exactly as
// writing nothing leaves both to them. Before B192 the transformer read a
// non-empty written list as the WHOLE substitution (`call_substitution`'s
// first arm zipped it against the callee's parameters and returned), so `U`
// reached emission abstract and `subject.greet()` resolved to the trait's
// bodyless requirement — the "resolved to a requirement, which has no body"
// internal error, at emission, on a program the analyzer had fully typed.

#[test]
fn a_partial_generic_argument_list_still_infers_the_rest() {
    // `tag<Dog>` on a `<T, U>` function supplies one of two. Supplying NONE
    // works (inference from the arguments) and supplying BOTH works; supplying
    // one used to leave `U` abstract into emission. B186's sugar reaches the
    // same hole through `tag<Dog>(label, subject: Greet)`, which is how it was
    // found.
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
fn a_full_generic_argument_list_still_binds_every_parameter() {
    // The control the partial case is measured against: with the whole list
    // written, the written arguments alone decide the instantiation and the
    // inferred bindings must not disturb them. `Fox` is written for `U` while
    // the ARGUMENT is a `Fox` too, so a merge that let inference overwrite the
    // written prefix would still pass here — which is why the pin below writes
    // the two the other way round.
    assert_compiles_and_runs(
        &format!(
            r#"{GREET}
            fun tag<T: Greet, U: Greet>(label: T, subject: U): str {{
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
fn the_written_prefix_outranks_what_inference_would_have_bound() {
    // Precedence, stated where it is observable: `Greet`'s `greet` is chosen
    // by the generic's binding, and here the two parameters take the SAME
    // argument type. Written `<Fox, Dog>` against `(Dog, Dog)` arguments would
    // print "woof/woof" if inference won and "ring/woof" if the written prefix
    // does — and the written prefix is what the author asked for. (A `Dog` is
    // accepted for a `U = Fox` parameter only because both satisfy the bound
    // the body actually calls through; the point of the pin is WHICH impl the
    // instance is specialized with.)
    let source = format!(
        r#"{GREET}
        fun tag<T: Greet, U: Greet>(label: T, subject: U): str {{
            T::greet(label) + "/" + U::greet(subject)
        }}
        fun main() {{
            print(tag<Fox, Dog>(Fox {{ name = "vix" }}, Dog {{ name = "rex" }}));
        }}
        main();
        "#
    );
    assert_compiles_and_runs(&source, "ring/woof\n");
}

#[test]
fn a_generic_argument_list_longer_than_the_parameter_list_is_refused() {
    // The one shape a prefix cannot be: longer than what it prefixes. `tag`
    // declares one generic, so the second written argument binds nothing — and
    // before B192 it was SILENTLY DROPPED, the call compiling and running as
    // though only `<Dog>` had been written. Under-supply is inference's job;
    // over-supply is a mistake with nowhere to put the extra.
    assert_fails_with(
        &format!(
            r#"{GREET}
            fun tag<T: Greet>(label: T): str {{ label.greet() }}
            fun main() {{
                print(tag<Dog, Fox>(Dog {{ name = "rex" }}));
            }}
            main();
            "#
        ),
        "`tag` takes at most 1 type argument, 2 given",
    );
}

#[test]
fn a_generic_argument_list_on_a_non_generic_function_is_refused() {
    // The zero-parameter edge of the same rule: there is no prefix at all, so
    // every written argument is an extra one.
    assert_fails_with(
        &format!(
            r#"{GREET}
            fun bark(): str {{ "woof" }}
            fun main() {{ print(bark<Dog>()); }}
            main();
            "#
        ),
        "`bark` takes no type arguments, 1 given",
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
    let steer = "a trait names a bound, and a value needs a type";
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
    // The FIELD leg left this list with B184 and CAME BACK with A124 R3, which
    // withdrew the hidden parameter at that position — so a field is a refusal
    // again, steered to `dyn Greet` rather than to the sugar
    // (`b184_a_trait_at_a_struct_field_is_refused_and_steers_to_dyn`). The
    // parameter leg left with B186 and stays gone. What this pins is the
    // return, the nested spelling, and the CLOSURE parameter — the position
    // that has no generic list to append to.
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
    assert_fails_with(
        &format!(
            r#"{GREET}
            fun main() {{
                let speak = |subject: Greet| subject.greet();
                print(speak(Dog {{ name = "rex" }}));
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
            fun on_change(self, observer: |i32| void): Subscription {
                observe(self.inner, |value| { observer(value * 2); })
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

// --- B175: an associated function reached through a BOUND types as the -------
// --- BINDER, not as the bound ------------------------------------------------
//
// The impl-path form of B162's `Trait::func`. `T::default()` under `T: Default`
// resolves to the trait's own `fun default(): Self`, and the `Self`-return
// specialization that makes a trait member's return concrete is driven by the
// RECEIVER — read off the member's first parameter. An associated function has
// no `self`, so nothing fired and the call typed as `Default` itself.
//
// std's `List<T: Add + Default>::sum`/`product` are the exhibit: `mut total =
// T::default()` made `total` trait-typed, so `total += item` reached the binary
// operator check with a `Type::Trait` left operand — the single reason B170 had
// to put that shape on the check's skip list, and (because a skipped operator
// keeps the anything-goes native emission) a live miscompile of `sum` over any
// nominal element type.
//
// The fix specializes `Self` against the BINDER the path named, structurally,
// exactly as the receiver branch does for a `self` method.

const WALLET: &str = r#"
    import std::io::print;
    import std::default::Default;
    import std::operators::Add;
    struct Money { cents: i32 }
    impl Money with Add {
        fun add(self, other: Money): Money { Money { cents = self.cents + other.cents } }
    }
    impl Money with Default {
        fun default(): Money { Money { cents = 0 } }
    }
"#;

#[test]
fn b175_a_bound_associated_call_types_as_the_type_parameter() {
    // The inference claim itself, asked the only way a parameter can be asked:
    // a `Type::Generic` compares equal to whatever is expected of it, so an
    // annotation cannot tell the two apart — a MEMBER lookup can, because it
    // reports the type it searched. Pre-fix: "Default has no method".
    let source = r#"
        import std::default::Default;
        import std::operators::Add;
        impl List<type T: Add + Default> {
            fun probe(self): T {
                let total = T::default();
                total.no_such_member()
            }
        }
        fun main() { print(1); }
        "#;
    assert_fails_with(source, "T has no method 'no_such_member'");
    // And the misleading half is GONE, not merely joined by a better one.
    assert_fails_without(source, "Default has no method");
}

#[test]
fn b175_a_single_bound_associated_call_types_as_the_type_parameter_too() {
    // The item filed the MULTI-bound (`Add + Default`) shape, but the cause is
    // not the multiplicity — the receiver-driven specialization cannot fire for
    // an associated function whatever the bound list looks like. One bound
    // behaved identically before the fix, and must behave identically after.
    assert_fails_with(
        r#"
        import std::default::Default;
        impl List<type T: Default> {
            fun probe(self): T {
                let total = T::default();
                total.no_such_member()
            }
        }
        fun main() { print(1); }
        "#,
        "T has no method 'no_such_member'",
    );
}

#[test]
fn b175_a_nominal_elements_sum_dispatches_its_add() {
    // THE MISCOMPILE, run. `total` arrived at `+=` typed as `Default`, the
    // operator check skipped that shape, and no `Add` dispatch was recorded —
    // so the emission stayed the host's `+` over two lowered structs and
    // `[40] + [2]` came back as the string "402", whose slot 0 is "4".
    // Pre-fix this printed "4\n0\n".
    assert_compiles_and_runs(
        &format!(
            r#"{WALLET}
            fun main() {{
                mut wallet = List::new();
                wallet.push(Money {{ cents = 40 }});
                wallet.push(Money {{ cents = 2 }});
                print(wallet.sum().cents);
            }}
            "#
        ),
        "42\n",
    );
}

#[test]
fn b175_an_empty_nominal_list_sums_to_the_elements_default() {
    // The other half of `sum`'s body, and the one that reads `T::default()`'s
    // value rather than its type: with no element to seed from, the fallback IS
    // the answer. It must be `Money`'s own default, not a trait-typed nothing.
    assert_compiles_and_runs(
        &format!(
            r#"{WALLET}
            fun main() {{
                let empty: List<Money> = List::new();
                print(empty.sum().cents);
            }}
            "#
        ),
        "0\n",
    );
}

#[test]
fn b175_the_trait_path_still_types_as_the_trait() {
    // B162's boundary, unmoved: `Trait::func()` names the TRAIT, not a bound
    // binder, so its `Self` return has no binder to specialize to and stays
    // abstract. The fix keys on the accessor's recorded constraint, which only
    // the bound path has — this pin is what keeps it from widening into one
    // that re-points every `Self` return at whatever is convenient.
    assert_compiles_and_runs(
        &format!(
            r#"{MAKER}
            fun main() {{
                print(Maker::make());
            }}
            "#
        ),
        "trait default\n",
    );
}

// --- B202: a refused exposure generates NOTHING -------------------------------
//
// B189's residual, and the last voice in the `[expose]` pile. The `service`
// macro reads an exposed field's element off the field's SOLE type argument —
// it runs before any type resolves, so the `Source` impl the compiler checks
// against is not there to read — and when it could not read one it pushed the
// literal `"_"` and carried it into all three places the element is named: the
// contract surface, the client's `RemoteSource<_>` mirror, and `connect`'s
// channel binding. `_` is not a type. So a field exposing something that is no
// source at all was told so once, correctly, at the field — and then twice more
// as `cannot find type '_'`, in code the author never wrote.
//
// A field whose element cannot be read now generates nothing at all: no surface
// entry, no mirror, no `session.expose` call. That leaves one sentence per
// mistake, and it leaves the compiler owing a sentence for every field it
// skips — including the one shape the two rules used to disagree about, a
// `Source` whose element is not written as an argument, which the check now
// refuses rather than letting the exposure quietly not happen.

/// A `[service]` whose one exposed field is no `Source` at all.
const NOT_A_SOURCE: &str = r#"
    import std::io::print;
    [service(KoltClient)]
    struct KoltStore {
        [expose] tasks: i32,
    }
    impl KoltStore {
        [rpc]
        fun bump(self, id: i32): i32 { id }
    }
    fun main() { print("kolt"); }
    main();
"#;

#[test]
fn a_refused_exposure_is_exactly_one_diagnostic_at_the_field() {
    assert_fails_once_with(NOT_A_SOURCE, "does not implement `std::Source`");
    assert_fails_without(NOT_A_SOURCE, "cannot find type '_'");
    assert_fails_without(NOT_A_SOURCE, "cannot infer");
    let diagnostics = failure_diagnostics(NOT_A_SOURCE);
    assert_eq!(
        diagnostics.len(),
        1,
        "one refused exposure is one diagnostic: {diagnostics:#?}"
    );
    // At the FIELD's annotation — the one place editing it means anything.
    assert_fails_spanning(NOT_A_SOURCE, "i32", "does not implement `std::Source`");
}

#[test]
fn a_good_field_beside_a_refused_one_is_untouched() {
    // The skip is per field. `names` is a perfectly good exposure and the
    // expansion still writes its surface entry, its mirror and its `expose`
    // call; only `tasks` generates nothing.
    let source = r#"
        import std::io::print;
        import std::reactive::SignalCell;
        [service(KoltClient)]
        struct KoltStore {
            [expose] tasks: i32,
            [expose] names: SignalCell<str>,
        }
        impl KoltStore {
            [rpc]
            fun bump(self, id: i32): i32 { id }
        }
        fun main() { print("kolt"); }
        main();
        "#;
    assert_fails_once_with(source, "does not implement `std::Source`");
    let diagnostics = failure_diagnostics(source);
    assert_eq!(
        diagnostics.len(),
        1,
        "the good field beside it draws nothing: {diagnostics:#?}"
    );
}

#[test]
fn a_service_whose_exposures_are_all_good_still_mirrors_them() {
    // The positive control the skip could break: with a refused field gone from
    // `exposed_names`, a good one must still reach all three places the element
    // is named. `peek` is the observable — it names the client's mirror field
    // AND its element type, so a skip that widened to every field would fail
    // here rather than passing quietly on a program that compiles because the
    // mirror it does not use is missing.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::SignalCell;
        import std::rpc::{ Transport, RemoteSource };
        [service(KoltClient)]
        struct KoltStore {
            [expose] names: SignalCell<str>,
        }
        impl KoltStore {
            [rpc]
            fun bump(self, id: i32): i32 { id }
        }
        fun peek<T: Transport>(client: KoltClient<T>): RemoteSource<str> {
            client.names
        }
        fun main() { print("kolt"); }
        main();
        "#,
        "kolt\n",
    );
}

#[test]
fn an_exposed_source_whose_element_is_not_a_written_argument_is_refused() {
    // The shape the two rules used to disagree about. `Feed` implements
    // `Source<Note>`, so the analyzer's reconciliation is happy; the expansion
    // reads the annotation, sees no type argument, and has nothing to build the
    // mirror from. It used to render `_` and fail with `cannot find type '_'`.
    // Now that it generates nothing, silence would be the alternative — the
    // exposure simply not happening, with the program compiling — so the check
    // says so at the field instead.
    let source = r#"
        import std::io::print;
        import std::reactive::{ Source, SignalCell, Subscription };
        [derive(Wire)]
        struct Note { id: i32 }
        struct Feed {
            inner: SignalCell<Note>,
        }
        impl Feed with Source<Note> {
            fun get(self): Note { self.inner.get() }
            fun on_change(self, observer: |Note| void): Subscription { self.inner.on_change(observer) }
        }
        [service(FeedClient)]
        struct Store {
            [expose] feed: Feed,
        }
        impl Store {
            [rpc]
            fun ping(self): i32 { 1 }
        }
        fun main() { print("ok"); }
        main();
        "#;
    assert_fails_once_with(source, "its element is not written as a type argument");
    assert_fails_without(source, "cannot find type '_'");
}

#[test]
fn a_user_source_written_with_its_element_still_exposes() {
    // The control for the refusal above, and A32's own case: a source of one's
    // own is exposable exactly when its element is where the expansion reads
    // it. `Feed<Note>` is; `Feed` is not.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Source, SignalCell, Subscription };
        [derive(Wire)]
        struct Note { id: i32 }
        struct Feed<T> {
            inner: SignalCell<T>,
        }
        impl Feed<type T> with Source<T> {
            fun get(self): T { self.inner.get() }
            fun on_change(self, observer: |T| void): Subscription { self.inner.on_change(observer) }
        }
        [service(FeedClient)]
        struct Store {
            [expose] feed: Feed<Note>,
        }
        impl Store {
            [rpc]
            fun ping(self): i32 { 1 }
        }
        fun main() { print("ok"); }
        main();
        "#,
        "ok\n",
    );
}

// --- B205: a supertrait member's `Self` inside a sub-trait's default body -----
//
// `trait Doubler with Add { fun twice(self): Self { self.add(self) } }` was two
// errors on a program with no mistake in it: `Expected Doubler, but got Add
// instead.` at the call and `Expected Add, but got Doubler instead.` at the
// argument. Inside a default body `self` is the trait's own abstract type
// (`Type::Trait(Doubler, [])`), and `add` is declared in `Add`'s terms — its
// `Self` return and its `= Self`-defaulted `b` both resolve to
// `Type::Trait(Add, [])`, which is a different type. So the argument was
// refused, and the call's type was refused by the enclosing default's own
// declared `Self`.
//
// The OPERATOR spelling has dispatched since B193, on exactly this shape. The
// explicit method spelling now reaches the same place: a member found in a
// SUPERTRAIT records the pair at the lookup, and both halves — the argument
// check, which reads a parameter type straight off the declaration, and the
// `Self`-return specialization, which already substitutes structurally for a
// concrete receiver — rebind it to the sub-trait.

/// Two `Add` impls under one `Doubler`, so a passing run proves the default
/// DISPATCHED rather than resolving to one answer for everybody. Both spellings
/// stand side by side in the same trait.
const DOUBLER: &str = r#"
    import std::io::print;
    import std::operators::Add;

    trait Doubler with Add {
        fun twice(self): Self {
            self.add(self)
        }
        fun twice_with_the_operator(self): Self {
            self + self
        }
    }

    struct Money { cents: i32 }
    impl Money with Add {
        fun add(self, b: Money): Money { Money { cents = self.cents + b.cents } }
    }
    impl Money with Doubler {}

    struct Tag { text: str }
    impl Tag with Add {
        fun add(self, b: Tag): Tag { Tag { text = self.text + b.text } }
    }
    impl Tag with Doubler {}
"#;

#[test]
fn b205_both_spellings_of_a_supertrait_call_resolve_in_a_default_body() {
    assert_compiles_and_runs(
        &format!(
            r#"{DOUBLER}
            fun main() {{
                print(Money {{ cents = 3 }}.twice().cents);
                print(Money {{ cents = 3 }}.twice_with_the_operator().cents);
            }}
            main();
            "#
        ),
        "6\n6\n",
    );
}

#[test]
fn b205_a_supertrait_call_in_a_default_body_dispatches_per_specialization() {
    // The claim the compile alone cannot make: `twice` is ONE body, and each
    // impl's own `add` is what runs in it.
    assert_compiles_and_runs(
        &format!(
            r#"{DOUBLER}
            fun main() {{
                print(Money {{ cents = 3 }}.twice().cents);
                print(Tag {{ text = "ab" }}.twice().text);
            }}
            main();
            "#
        ),
        "6\nabab\n",
    );
}

#[test]
fn b205_the_supertrait_chain_is_walked_the_whole_way() {
    // The rebinding keys on the trait that DECLARES the member, whatever depth
    // it sits at — and a user trait, so the rule is not `Add`'s. Two calls
    // chained also prove the CALL's own type came back as the sub-trait: the
    // second `.join` is made on the first one's result.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Base {
            fun join(self, other: Self): Self;
        }
        trait Middle with Base {}
        trait Top with Middle {
            fun tripled(self): Self {
                self.join(self).join(self)
            }
        }

        struct Tag { text: str }
        impl Tag with Base {
            fun join(self, other: Tag): Tag { Tag { text = self.text + other.text } }
        }
        impl Tag with Middle {}
        impl Tag with Top {}

        fun main() { print(Tag { text = "x" }.tripled().text); }
        main();
        "#,
        "xxx\n",
    );
}

#[test]
fn b205_an_unrelated_traits_method_is_still_refused_in_a_default_body() {
    // The control. The rebinding fires only for a member the sub-trait's own
    // supertrait walk found; a method no supertrait promises is still nothing
    // `Self` can do here, and widening the walk is exactly the failure this pin
    // exists to catch.
    assert_fails_with(
        r#"
        import std::io::print;
        import std::operators::{ Add, Mul };

        trait Doubler with Add {
            fun twice(self): Self {
                self.mul(self)
            }
        }

        fun main() { print("x"); }
        main();
        "#,
        "Doubler has no method 'mul'",
    );
}

// --- B216: a PARAMETERIZED supertrait clause keeps `Self` in a default body --
//
// B205 rebound a supertrait member's `Self` to the sub-trait, GATED to an
// argument-less `with` clause: write `with Add<i32>` and `b: B` (`i32`) and the
// `Self` return part company, while both still resolve to the one type
// `Type::Trait(Add, [])` under `B = Self`. Nothing in the RESOLVED types
// separates them — only the WRITTEN name does — so a blanket rewrite would have
// made `b` the sub-trait and `self.add(1)` was refused with `Expected Bumper,
// but got Add instead.` (the argument), plus a refusal of the call's own type.
//
// The written-name rule B206 built for LABELS is what tells the two apart, and
// it now runs in method resolution's substitution too: a position spelled `Self`
// takes the sub-trait, a position spelled with one of the supertrait's own
// parameter names takes the matching `with`-clause argument.

#[test]
fn b216_a_parameterized_supertrait_clause_binds_its_argument_and_keeps_self() {
    // The repro. `b: B` is the clause's `i32` (so the literal `1` is accepted)
    // and the `Self` return is `Bumper` (so `.cents` resolves on the result).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::operators::Add;

        trait Bumper with Add<i32> {
            fun bumped(self): Self {
                self.add(1)
            }
        }

        struct Money { cents: i32 }
        impl Money with Add<i32> {
            fun add(self, b: i32): Money { Money { cents = self.cents + b } }
        }
        impl Money with Bumper {}

        fun main() { print(Money { cents = 3 }.bumped().cents); }
        main();
        "#,
        "4\n",
    );
}

#[test]
fn b216_a_parameterized_supertrait_default_dispatches_per_specialization() {
    // The claim the compile alone cannot make: `bumped` is ONE body and each
    // impl's own `add` runs in it, with the clause argument bound per subject.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::operators::Add;

        trait Bumper with Add<i32> {
            fun bumped(self): Self {
                self.add(1)
            }
        }

        struct Money { cents: i32 }
        impl Money with Add<i32> {
            fun add(self, b: i32): Money { Money { cents = self.cents + b } }
        }
        impl Money with Bumper {}

        struct Tag { text: str }
        impl Tag with Add<i32> {
            fun add(self, b: i32): Tag { Tag { text = self.text + b } }
        }
        impl Tag with Bumper {}

        fun main() {
            print(Money { cents = 3 }.bumped().cents);
            print(Tag { text = "x" }.bumped().text);
        }
        main();
        "#,
        "4\nx1\n",
    );
}

#[test]
fn b216_a_two_parameter_supertrait_clause_binds_both_arguments_and_the_self_return() {
    // Two written arguments, so the clause's substitution and the `Self` return
    // are exercised together: `a: A` is `str` and `b: B` is `i32` from the
    // clause, while the return is the SUB-trait — the half that was refused.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Blender<A, B> {
            fun blend(self, a: A, b: B): Self;
        }

        trait Blended with Blender<str, i32> {
            fun blended(self): Self {
                self.blend("!", 1)
            }
        }

        struct Tag { text: str }
        impl Tag with Blender<str, i32> {
            fun blend(self, a: str, b: i32): Tag { Tag { text = self.text + a + b } }
        }
        impl Tag with Blended {}

        fun main() { print(Tag { text = "x" }.blended().text); }
        main();
        "#,
        "x!1\n",
    );
}

#[test]
fn b216_a_defaulted_parameter_the_clause_left_out_still_means_the_sub_trait() {
    // The written name is looked up BY POSITION in the declaring trait's
    // parameter list, and a `= Self` parameter the clause did not reach (`B` is
    // index 1, the clause wrote one argument) falls back to the sub-trait —
    // which is precisely what the default says. So `self` is a legal second
    // argument here while the first is the clause's `str`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Blender<A, B = Self> {
            fun blend(self, a: A, b: B): Self;
        }

        trait Blended with Blender<str> {
            fun blended(self): Self {
                self.blend("!", self)
            }
        }

        struct Tag { text: str }
        impl Tag with Blender<str> {
            fun blend(self, a: str, b: Tag): Tag { Tag { text = self.text + a + b.text } }
        }
        impl Tag with Blended {}

        fun main() { print(Tag { text = "x" }.blended().text); }
        main();
        "#,
        "x!x\n",
    );
}

#[test]
fn b216_the_argument_less_clause_still_takes_the_sub_trait_everywhere() {
    // B205's control, standing next to the parameterized shape in ONE program:
    // with nothing written in the clause, `= Self` means exactly `Self` and
    // BOTH positions are the sub-trait — the blanket rewrite B205 installed.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::operators::Add;

        trait Doubler with Add {
            fun twice(self): Self {
                self.add(self)
            }
        }

        trait Bumper with Add<i32> {
            fun bumped(self): Self {
                self.add(1)
            }
        }

        struct Money { cents: i32 }
        impl Money with Add { fun add(self, b: Money): Money { Money { cents = self.cents + b.cents } } }
        impl Money with Doubler {}

        struct Tally { hits: i32 }
        impl Tally with Add<i32> { fun add(self, b: i32): Tally { Tally { hits = self.hits + b } } }
        impl Tally with Bumper {}

        fun main() {
            print(Money { cents = 3 }.twice().cents);
            print(Tally { hits = 3 }.bumped().hits);
        }
        main();
        "#,
        "6\n4\n",
    );
}

#[test]
fn b216_a_parameterized_supertrait_argument_of_the_wrong_type_is_still_refused() {
    // The rebinding must not become a licence: the clause argument is a real
    // expectation, so a `str` where the clause wrote `i32` is refused — and
    // named as `i32`, not as `Add` and not as `Bumper`.
    assert_fails_with(
        r#"
        import std::io::print;
        import std::operators::Add;

        trait Bumper with Add<i32> {
            fun bumped(self): Self {
                self.add("nope")
            }
        }

        fun main() { print("x"); }
        main();
        "#,
        "Expected i32, but got str instead.",
    );
}

// --- B235: a `= Self` default is not a bound --------------------------------
//
// A defaulted trait parameter (`trait Mixer<A = Self, B = Self>`) interns as
// its DEFAULT's type rather than as a binder, and `generic_bound_traits`'s
// fallback — "an unlisted parameter's own type IS its single bound", which is
// how `<T: Display>` recovers `Display` from the id — then read that default as
// a requirement. Reached through a sub-trait's parameterized clause (`trait
// Mixed with Mixer<i32, str>`) the clause argument substitutes the parameter
// and is checked against it, so the declaring trait was demanded of its own
// arguments: `'i32' does not implement trait 'Mixer'`, and the same for `str`.
//
// A default says what a position MEANS when no argument is supplied; it never
// says what an argument must be. `Add<i32>` escaped only because `i32`
// genuinely implements `Add`. A parameter's own bounds are untouched — they
// live in `generic_bounds`, which the walk consults first.

#[test]
fn b235_a_self_defaulted_parameter_is_not_required_of_the_clause_argument() {
    // The filed shape, and both defaulted parameters at once.
    assert_compiles(
        r#"
        trait Mixer<A = Self, B = Self> {
            fun mix(self, a: A, b: B): str;
        }
        trait Mixed with Mixer<i32, str> {
            fun describe(self): str { self.mix(1, "two") }
        }
        fun main() {}
        "#,
    );
}

#[test]
fn b235_the_add_control_still_compiles() {
    // `Add<B = Self>` under a parameterized clause was the shape that ESCAPED —
    // `i32` implements `Add`, so demanding it of the argument happened to hold.
    // It still compiles, for the right reason now.
    assert_compiles(
        r#"
        import std::operators::Add;
        trait Adder with Add<i32> {
            fun twice(self): Self { self.add(1) }
        }
        fun main() {}
        "#,
    );
}

#[test]
fn b235_a_genuinely_bounded_parameter_still_refuses_its_argument() {
    // The counterweight: a WRITTEN bound is a requirement, and a clause
    // argument that cannot meet it is refused exactly as before.
    assert_fails_with(
        r#"
        import std::display::Display;
        trait Shower<A: Display> {
            fun show_it(self, a: A): str;
        }
        struct Opaque { n: i32 }
        trait Shown with Shower<Opaque> {
            fun go(self): str { self.show_it(Opaque { n = 1 }) }
        }
        fun main() {}
        "#,
        "'Opaque' does not implement trait 'Display', required by a generic bound of this call",
    );
}

#[test]
fn b235_a_defaulted_parameter_the_clause_left_out_is_still_no_bound() {
    // The argument-less clause: the default MEANS the sub-trait, and still
    // requires nothing of anybody (B216 pins what the position resolves TO).
    assert_compiles(
        r#"
        trait Mixer<A = Self> {
            fun mix(self, a: A): str;
        }
        trait Mixed with Mixer {
            fun describe(self): str { self.mix(self) }
        }
        fun main() {}
        "#,
    );
}

#[test]
fn b184_a_trait_at_a_struct_field_is_refused_and_steers_to_dyn() {
    // A124 R3, RULED 2026-09-22 — **BREAKING**, and the withdrawal of B184's
    // reading at this one position. A bare trait at a field was sugar for a
    // hidden type parameter (`struct C { x: X }` = `struct C<#0: X> { x: #0 }`,
    // grounded per literal), and A124's probe (g) is what it cost: a `Holder`
    // over a root and a `Holder` over a mapped node were two types, so one list
    // of both was refused, and the parameter was viral through every embedding
    // struct. The field takes the OBJECT instead, and the refusal says so.
    //
    // B186's parameter and B161's binding are unchanged; the steer still names
    // them, and `dyn_objects.rs` holds what the field now does.
    let source = format!(
        r#"{HIDDEN}
        fun main() {{ let c = C {{ x = A {{}} }}; print(c.x.who()); }}
        main();
        "#
    );
    assert_fails_once_with(&source, "'X' is a trait, not a type");
    assert_fails_with(&source, "`dyn X` for a field");
    assert_fails_with(&source, "`fun f(x: X)` for a parameter");
}

#[test]
fn b184_the_dyn_field_the_steer_names_is_what_to_write() {
    // Advice that compiles, which is the standing rule for a steer: the very
    // program above with the steer taken. Two impls in one field type, which is
    // also what the hidden parameter could not do.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait X { fun who(self): str; }
        struct A {}
        impl A with X { fun who(self): str { "A" } }
        struct B {}
        impl B with X { fun who(self): str { "B" } }
        struct C { x: dyn X }
        fun main() {
            let a = A {};
            let b = B {};
            let cs: List<C> = [ C { x = a }, C { x = b } ];
            for c in cs { print(c.x.who()); }
        }
        main();
        "#,
        "A\nB\n",
    );
}

#[test]
fn b184_case_3_as_written_is_a_scope_error_and_it_is_not_the_only_one() {
    // The item's case 3 kept, with the one assertion the withdrawal moves: the
    // missing `let` is still a plain scope error, and the field is NOW also
    // refused — two mistakes, two reports, neither standing in for the other.
    let source = format!(
        r#"{HIDDEN}
        fun main() {{
            mut c1 = C {{ x = A {{}} }};
            c2 = C {{ x = B {{}} }};
        }}
        main();
        "#
    );
    assert_fails_with(&source, "cannot find 'c2' in this scope");
    assert_fails_once_with(&source, "'X' is a trait, not a type");
    assert_fails_without(&source, "hidden type parameter");
}

#[test]
fn b184_an_attributed_declaration_takes_the_same_steer() {
    // The v1 boundary that no longer needs a boundary. `[derive]` used to get a
    // second sentence explaining why the sugar was refused on an attributed
    // declaration (macro reflection is syntactic, so a generator cannot spell a
    // parameter nobody wrote). The sugar is gone everywhere now, and `dyn X` is
    // a written type a generator CAN spell, so the ordinary steer is the whole
    // answer — and it is still one report, not a page of generated-code
    // follow-ons (B182's rule).
    let source = format!(
        r#"{GREET}
        [derive(Wire)]
        struct Kennel {{ inner: Greet }}
        fun main() {{ print(1); }}
        main();
        "#
    );
    assert_fails_once_with(&source, "'Greet' is a trait, not a type");
    assert_fails_with(&source, "`dyn Greet` for a field");
    assert_fails_without(&source, "not on a declaration carrying an attribute");
    assert_fails_without(&source, "from_json_value");
    let diagnostics = failure_diagnostics(&source);
    assert_eq!(
        diagnostics.len(),
        1,
        "one refused annotation is one diagnostic: {diagnostics:#?}"
    );
}

#[test]
fn b184_a_written_generic_beside_a_trait_typed_field_still_works_as_dyn() {
    // The mixed form B184 pinned, carried across: a struct that is ALREADY
    // generic and whose trait-typed field mentions its own parameter. Under the
    // sugar the hidden parameter was APPENDED, so `Held<i32>` still wrote one
    // argument; as a `dyn` there is no second parameter at all, which is the
    // simplification the withdrawal buys — `Held<i32>` is `Held<i32>` whatever
    // the field holds.
    const HELD: &str = r#"
        import std::io::print;
        import std::reactive::{ Source, SignalCell };
        struct Held<T> { first: T, list: dyn Source<List<T>> }
        "#;
    assert_compiles_and_runs(
        &format!(
            r#"{HELD}
            fun count<T>(held: Held<T>): usize {{ held.list.get().len() }}
            fun first<T>(held: Held<T>): T {{ held.first }}
            fun main() {{
                let numbers: Held<i32> = Held {{ first = 0, list = SignalCell::new([1, 2]) }};
                let words = Held {{ first = "z", list = SignalCell::new(["a"]) }};
                print(count(numbers) + count(words));
                print(first(words));
            }}
            main();
            "#
        ),
        "3\nz\n",
    );
    // The WRITTEN argument is still checked, and it is now the ONLY one: the
    // report names `Held<i32>` against `Held<str>`, where the sugar's report
    // had to carry the unwritable tail as well.
    assert_fails_with(
        &format!(
            r#"{HELD}
            fun main() {{
                let numbers: Held<i32> = Held {{ first = "z", list = SignalCell::new(["a"]) }};
            }}
            main();
            "#
        ),
        "Expected Held<i32>, but got Held<str> instead.",
    );
}

// --- B252: a refused RETURN annotation stands its uses down too --------------
//
// B182's stand-down is keyed on the annotation's SLOT — the type id the
// refusal resolved to `Unknown` — and every consumer reaches it through the
// expression that reads it: a binding, a parameter, a field. A CALL reads its
// callee's return annotation, but the call's own result type is a fresh id the
// solver grounds from the signature, not the annotation's slot, so the covered
// set missed it and a refused return cascaded (`cannot call method 'who' on
// unknown`) where the same trait refused at a closure PARAMETER stood down.
// The callee's declared return type id is what the two have in common.

#[test]
fn b252_a_refused_return_annotation_does_not_cascade_through_its_uses() {
    // The exhibit: one mistake, one report. `Greet` in return position is
    // refused (with the steer to a generic return), and the call's use is that
    // refusal restated in the vocabulary of a type the author never wrote.
    let source = format!(
        r#"{GREET}
        fun pick(): Greet {{ Dog {{ name = "rex" }} }}
        fun main() {{
            print(pick().greet());
        }}
        main();
        "#
    );
    assert_fails_once_with(&source, "'Greet' is a trait, not a type");
    assert_fails_without(&source, "on unknown");
    let diagnostics = failure_diagnostics(&source);
    assert_eq!(
        diagnostics.len(),
        1,
        "one refused annotation is one diagnostic: {diagnostics:#?}"
    );
}

#[test]
fn b252_a_field_read_through_a_refused_return_stands_down_as_well() {
    // The second of B182's three consumers, reached the same way: the field
    // access asks the same covered set of the same slot, so widening the set
    // answers for both halves at once.
    let source = format!(
        r#"{GREET}
        fun pick(): Greet {{ Dog {{ name = "rex" }} }}
        fun main() {{
            print(pick().name);
        }}
        main();
        "#
    );
    assert_fails_once_with(&source, "'Greet' is a trait, not a type");
    assert_fails_without(&source, "cannot access field");
    let diagnostics = failure_diagnostics(&source);
    assert_eq!(
        diagnostics.len(),
        1,
        "one refused annotation is one diagnostic: {diagnostics:#?}"
    );
}

#[test]
fn b252_an_unrelated_unknown_still_reports_beside_a_refused_return() {
    // E104's lesson at this grain, the same control B182's own pin carries: the
    // stand-down is asked PER CALL, of the slot that call's callee declares —
    // never of "is this type unknown". B188's arity refusal resolves `Holder`
    // to `Unknown` under a rule this family knows nothing about, and a call on
    // THAT still refuses, in the same program whose `pick()` stands down.
    let source = format!(
        r#"{GREET}
        struct Holder<T> {{ v: T }}
        struct Other {{ held: Holder }}
        fun pick(): Greet {{ Dog {{ name = "rex" }} }}
        fun main() {{
            print(pick().greet());
            let other = Other {{ held = 1 }};
            print(other.held.length());
        }}
        main();
        "#
    );
    assert_fails_with(&source, "'Greet' is a trait, not a type");
    assert_fails_with(&source, "`Holder` takes 1 type argument, 0 given");
    assert_fails_with(&source, "cannot call method 'length' on unknown");
    assert_fails_without(&source, "'greet' on unknown");
}

#[test]
fn b252_a_missing_method_on_a_well_typed_return_still_reports() {
    // The non-vacuity control: the widening must not silence a call whose
    // callee's return annotation is perfectly good. `Dog` has no `bark`, and
    // that is a mistake nobody has been told about.
    let source = format!(
        r#"{GREET}
        fun pick(): Dog {{ Dog {{ name = "rex" }} }}
        fun main() {{
            print(pick().bark());
        }}
        main();
        "#
    );
    assert_fails_with(&source, "no method 'bark'");
}

// --- B243: a one-block sub-trait impl substitutes the whole chain ------------
//
// `impl Cell<type T> with Signal<T>` reaches `Source`'s defaults — `map`,
// `effect_on_change` — through `Signal`'s own `with Source<T>`, and those
// bodies are written in `Source`'s parameters, which are DIFFERENT constraint
// ids from `Signal`'s. `inherited_default_bindings` bound only the parameters
// of the trait the `with` clause names, so every one of them stayed abstract:
// `x.map(|v| v * 2)` typed `v` as the trait's `T` and steered the author to
// "add it where `T` is declared on `trait Source`" — a std file, and no fix.
// Splitting the impl into `with Source<T>` + `with Signal<T>` worked only
// because it made `Source` a clause trait; that is A49's recipe for kolt's
// `StorageSignalCell`, and it is no longer required.
//
// B216's `supertrait_position_type` is the neighbour: the same "a supertrait's
// member is written in ITS terms" fact, decided there for an ambiguous `Self`
// position and here for the trait's own parameters. The walk is B164's
// `trait_with_supertraits_at`, which already carries each trait's arguments
// through the chain.

#[test]
fn b243_a_one_block_sub_trait_impl_grounds_a_supertrait_defaults_closure_parameter() {
    // The exhibit, in the user's own vocabulary so nothing depends on std's
    // shape: `mapped` is a `Src` default, the impl names only `Sig`, and the
    // closure parameter must be the `i32` the receiver binds. Before the fix
    // this refused with "`*` on `T` needs `T: Mul`", naming a parameter
    // declared on a trait the author did not write the impl against.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Src<T> {
            fun get(self): T;
            fun mapped(self, transform: sync |T| T): T { transform(self.get()) }
        }

        trait Sig<T> with Src<T> {
            fun label(self): str;
        }

        struct Cell<T> { v: T }

        impl Cell<type T> with Sig<T> {
            fun get(self): T { self.v }
            fun label(self): str { "cell" }
        }

        fun main() {
            let c = Cell { v = 3 };
            print(c.mapped(|n| n * 2));
            print(c.label());
        }
        main();
        "#,
        "6\ncell\n",
    );
}

#[test]
fn b243_a_one_block_signal_impl_reaches_source_sub_and_effect_on_change() {
    // The shape the item was filed on, against std's own traits: `sub` and
    // `effect_on_change` are `Source` defaults, the impl writes one block of
    // `Signal<T>`, and a subscriber tracks the writes. `15` is the
    // owner-registered effect firing on the change, `2` and `10` the doubled
    // value before and after.
    //
    // Re-derived at A124 S2c: the pin was written over `c.map(..)`, which was a
    // `Source` DEFAULT then and is a BLANKET over `S: Source<T>` now — and a
    // blanket is not found on a type that implements `Source` only through a
    // one-block sub-trait impl (B419, filed from this lane with a std-free
    // repro; Order 43). The defaults B243 is about are still reached, and
    // this pin holds them.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, Source, SignalCell, Subscription, comp };

        struct Cell<T> { inner: SignalCell<T> }

        impl Cell<type T> with Signal<T> {
            fun get(self): T { self.inner.get() }
            [must_use]
            fun on_change(self, observer: |T| void): Subscription {
                self.inner.on_change(observer)
            }
            fun set(self, value: T) { self.inner.set(value) }
            fun notify(self) { self.inner.notify() }
        }

        fun main() {
            let c = Cell { inner = Signal::new(1) };
            let doubled: SignalCell<i32> = Signal::new(0);
            let _watch = c.sub(|v| doubled.set(v * 2));
            print(doubled.get());
            let (_built, scope) = comp(|| {
                c.effect_on_change(|v| print(v + 10));
            });
            c.set(5);
            print(doubled.get());
            scope.dispose();
        }
        main();
        "#,
        "2\n15\n10\n",
    );
}

/// The half of the pre-flip b243 pin the flip moved out of reach: `map` is a
/// blanket over `S: Source<T>` since A124 S2c, and a blanket is not found on a
/// type that implements `Source` only through a one-block `impl .. with
/// Signal<T>` — "Cell<i32> has no method 'map'". Kept as the program the pin
/// used to be, so the fix turns it green as written.
#[test]
#[ignore = "B419: a blanket over a supertrait is not found through a one-block sub-trait impl"]
fn b419_a_blanket_map_reaches_a_one_block_signal_impl() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, Source, SignalCell, Subscription };

        struct Cell<T> { inner: SignalCell<T> }

        impl Cell<type T> with Signal<T> {
            fun get(self): T { self.inner.get() }
            [must_use]
            fun on_change(self, observer: |T| void): Subscription {
                self.inner.on_change(observer)
            }
            fun set(self, value: T) { self.inner.set(value) }
            fun notify(self) { self.inner.notify() }
        }

        fun main() {
            let c = Cell { inner = Signal::new(1) };
            let doubled = c.map(|v| v * 2);
            print(doubled.get());
            c.set(5);
            print(doubled.get());
        }
        main();
        "#,
        "2\n10\n",
    );
}

#[test]
fn b243_the_split_impl_still_works() {
    // A49's recipe, kept green: making `Source` a clause trait of its own was
    // the workaround, and the fix must not cost it. Same program, two blocks.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, Source, SignalCell, Subscription };

        struct Cell<T> { inner: SignalCell<T> }

        impl Cell<type T> with Source<T> {
            fun get(self): T { self.inner.get() }
            [must_use]
            fun on_change(self, observer: |T| void): Subscription {
                self.inner.on_change(observer)
            }
        }

        impl Cell<type T> with Signal<T> {
            fun set(self, value: T) { self.inner.set(value) }
            fun notify(self) { self.inner.notify() }
        }

        fun main() {
            let c = Cell { inner = Signal::new(1) };
            let doubled = c.map(|v| v * 2);
            print(doubled.get());
        }
        main();
        "#,
        "2\n",
    );
}

#[test]
fn b243_a_supertrait_parameter_the_clause_fixes_grounds_to_what_it_fixed() {
    // The chain carries ARGUMENTS, not just names (B164's substitution, which
    // is what `trait_with_supertraits_at` is for): a sub-trait that fixes its
    // supertrait's parameter to a concrete type grounds the default's closure
    // parameter to THAT type, not to the impl's own.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Src<T> {
            fun get(self): T;
            fun mapped(self, transform: sync |T| T): T { transform(self.get()) }
        }

        trait Counted with Src<i32> {
            fun label(self): str;
        }

        struct Cell { v: i32 }

        impl Cell with Counted {
            fun get(self): i32 { self.v }
            fun label(self): str { "cell" }
        }

        fun main() {
            let c = Cell { v = 4 };
            print(c.mapped(|n| n + 1));
        }
        main();
        "#,
        "5\n",
    );
}

#[test]
fn b243_an_unbounded_parameter_is_still_refused_in_the_default_itself() {
    // The non-vacuity control: the widening grounds a parameter at the CALL,
    // and must not make the default's own body typecheck against a parameter
    // that promises nothing. `T` is unbounded on `Src`, so `*` inside the
    // default is the same refusal it always was.
    assert_fails_with(
        r#"
        import std::io::print;

        trait Src<T> {
            fun get(self): T;
            fun twice(self): T { self.get() * 2 }
        }

        struct Cell<T> { v: T }

        impl Cell<type T> with Src<T> {
            fun get(self): T { self.v }
        }

        fun main() {
            let c = Cell { v = 3 };
            print(c.twice());
        }
        main();
        "#,
        "needs `T: Mul`",
    );
}

#[test]
fn b245_the_full_mixer_program_compiles_end_to_end() {
    // rigid-28's find, whole: B235 fixed the trait-side read of the `= Self`
    // default and left `impl Cup with Mixed` refusing with `parameter 1 of
    // `Cup`'s `mix` is `i32`, but `Mixed` declares `Cup`` — twice, once per
    // defaulted parameter.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        trait Blender<A = Self, B = Self> {
            fun mix(self, a: A, b: B): str;
        }
        trait Mixed with Blender<i32, str> {
            fun describe(self): str { self.mix(1, "two") }
        }
        struct Cup {}
        impl Cup with Mixed {
            fun mix(self, a: i32, b: str): str { b }
        }
        fun main() { print(Cup {}.describe()); }
        main();
        "#,
        "two\n",
    );
}

#[test]
fn b245_the_conformance_site_still_refuses_a_signature_the_clause_denies() {
    // The counterweight to the pin above: recovering the supertrait's arguments
    // must not make conformance accept anything — an impl whose `mix` takes a
    // `str` where the clause wrote `i32` is still wrong, and now says so
    // against the ARGUMENT rather than against the subject.
    assert_fails_with(
        r#"
        trait Blender<A = Self, B = Self> {
            fun mix(self, a: A, b: B): str;
        }
        trait Mixed with Blender<i32, str> {
            fun describe(self): str { self.mix(1, "two") }
        }
        struct Cup {}
        impl Cup with Mixed {
            fun mix(self, a: str, b: str): str { b }
        }
        fun main() {}
        "#,
        "parameter 1 of `Cup`'s `mix` is `str`, but `Mixed` declares `i32`",
    );
}

#[test]
fn b245_b180s_impl_path_reads_the_clause_argument_not_self() {
    // The operand half, through an INHERITED default: `lt` is `PartialOrd`'s
    // own body and its `b: B` is the `Feet` the clause wrote. Pre-fix this was
    // refused with "`Meters`'s `lt` accepts `Meters`, but the right operand is
    // `Feet`" — the `Self` fallback, over a clause that said otherwise.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::compare::{ PartialEq, PartialOrd, Ordering };
        import std::option::Option;
        struct Meters { n: i32 }
        struct Feet { n: i32 }
        impl Meters with PartialEq<Feet> {
            fun eq(self, b: Feet): bool { self.n == b.n }
        }
        impl Meters with PartialOrd<Feet> {
            fun partial_compare(self, b: Feet): Option<Ordering> {
                Option::Some(Ordering::Less)
            }
        }
        fun main() { print(Meters { n = 1 } < Feet { n = 5 }); }
        main();
        "#,
        "true\n",
    );
}

#[test]
fn b245_an_unsupplied_default_still_means_self_at_the_operand() {
    // The control that keeps the `Self` fallback honest, and the shape the
    // corpus has: `impl Num with Ord` supplies no argument anywhere in the
    // chain, so `PartialOrd`'s `B` — and `PartialEq`'s, which `PartialOrd`'s
    // own clause passes it — is the subject.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::compare::{ Eq, Ord, PartialEq, PartialOrd, Ordering };
        import std::option::Option;
        struct Num { n: i32 }
        impl Num with PartialEq { fun eq(self, b: Num): bool { self.n == b.n } }
        impl Num with Eq {}
        impl Num with PartialOrd {
            fun partial_compare(self, b: Num): Option<Ordering> {
                Option::Some(self.compare(b))
            }
        }
        impl Num with Ord {
            fun compare(self, b: Num): Ordering {
                if self.n < b.n { Ordering::Less }
                else if self.n > b.n { Ordering::Greater }
                else { Ordering::Equal }
            }
        }
        fun main() { print(Num { n = 1 } < Num { n = 2 }); }
        main();
        "#,
        "true\n",
    );
}

#[test]
fn b245_a_wrong_operand_is_still_refused_on_the_clause_argument() {
    // The refusal B180 exists for, now measured against the argument the clause
    // wrote: `Meters < Meters` is wrong where the clause says the operand is a
    // `Feet`, and the report names `Feet`.
    assert_fails_with(
        r#"
        import std::compare::{ PartialEq, PartialOrd, Ordering };
        import std::option::Option;
        struct Meters { n: i32 }
        struct Feet { n: i32 }
        impl Meters with PartialEq<Feet> {
            fun eq(self, b: Feet): bool { self.n == b.n }
        }
        impl Meters with PartialOrd<Feet> {
            fun partial_compare(self, b: Feet): Option<Ordering> {
                Option::Some(Ordering::Less)
            }
        }
        fun main() { let _ = Meters { n = 1 } < Meters { n = 2 }; }
        "#,
        "`Meters`'s `lt` accepts `Feet`, but the right operand is `Meters`",
    );
}

#[test]
fn b245_the_add_operand_rule_still_reads_a_written_argument() {
    // B180's own shape, unmoved: an impl that declares its `add` outright never
    // went through the defaulted position, and still does not.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::operators::Add;
        struct Meters { n: i32 }
        struct Feet { n: i32 }
        impl Meters with Add<Feet> {
            fun add(self, b: Feet): Meters { Meters { n = self.n + b.n } }
        }
        fun main() { print((Meters { n = 1 } + Feet { n = 2 }).n); }
        main();
        "#,
        "3\n",
    );
}

#[test]
fn b245_a_defaulted_parameter_the_clause_argument_grounds_reaches_a_supertrait() {
    // The chain link the identity buys: `PartialOrd<B = Self> with
    // PartialEq<B>` passes its OWN defaulted parameter to its supertrait, so
    // `impl Meters with PartialOrd<Feet>` must reach `PartialEq` at `Feet` —
    // which is what makes the `eq` impl above conform rather than being told it
    // owes a `Meters`.
    assert_fails_with(
        r#"
        import std::compare::{ PartialEq, PartialOrd, Ordering };
        import std::option::Option;
        struct Meters { n: i32 }
        struct Feet { n: i32 }
        impl Meters with PartialEq<Feet> {
            fun eq(self, b: Meters): bool { true }
        }
        impl Meters with PartialOrd<Feet> {
            fun partial_compare(self, b: Feet): Option<Ordering> {
                Option::Some(Ordering::Less)
            }
        }
        fun main() {}
        "#,
        "parameter 1 of `Meters`'s `eq` is `Meters`, but `PartialEq` declares `Feet`",
    );
}

/// A generic `[service]` subject. A52 pinned the CASCADE out of the expansion
/// here — `'contract_hash' is already defined for 'StoreClient<T>'` (the client
/// struct's own transport parameter is spelled `T` and collided with the
/// subject's) plus `` `Store` takes 1 type argument, 0 given `` — and said in
/// this comment that a curated refusal was owed and could not be written without
/// first recording what the unfixed shape said. B266 wrote it: the refusal is at
/// the attribute now, and this pin is the same exhibit with the debt paid.
#[test]
fn a52_a_generic_service_subject_is_refused_by_the_expansion() {
    let source = r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };
        [derive(Wire, PartialEq, Debug)]
        struct Task { id: i32 }
        [service(StoreClient)]
        struct Store<T: Wire + PartialEq> {
            [expose] items: SignalCell<List<T>>,
        }
        impl Store<type T: Wire + PartialEq> {
            [rpc]
            fun count(self): usize { self.items.get().len() }
        }
        fun main() { print("store"); }
        main();
        "#;
    assert_fails_once_with(source, "`[service]` cannot take a generic subject");
    assert_fails_without(
        source,
        "'contract_hash' is already defined for 'StoreClient<T>'",
    );
    assert_fails_without(source, "`Store` takes 1 type argument, 0 given");
}

/// The other spelling: naming the source by TRAIT. B184 made a trait-typed
/// field legal sugar over a hidden type parameter and carved attributed
/// declarations out, because `[derive]` and `[service]` write code from the
/// types the author wrote. This is that carve-out reached through `[expose]`,
/// and it is one curated sentence that names the rule.
#[test]
fn a52_an_expose_of_a_trait_typed_source_field_is_refused_at_the_annotation() {
    let source = r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source };
        [derive(Wire, PartialEq, Debug)]
        struct Task { id: i32 }
        [service(StoreClient)]
        struct Store {
            [expose] items: Source<List<Task>>,
        }
        fun main() { print("store"); }
        main();
        "#;
    // A124 R3: the carve-out's own sentence is gone with the sugar it explained.
    // An `[expose]`d field naming a trait takes the ordinary refusal and the
    // ordinary steer — and `dyn Source<List<Task>>` is a written type the
    // `[service]` generator can spell, which is what made the carve-out
    // unnecessary rather than merely reworded.
    assert_fails_with(source, "'Source' is a trait, not a type");
    assert_fails_with(source, "`dyn Source` for a field");
    assert_fails_without(
        source,
        "A field MAY name a trait, but not on a declaration carrying an attribute",
    );
}

/// The control that keeps both of the above from reading as "an exposed field
/// must be a `SignalCell`": a user type that implements `Source<T>` — declared
/// generic, applied concretely — is exposable, and the generated
/// `session.expose(self.items)` infers `T` off its impl.
#[test]
fn a52_an_expose_of_a_user_source_type_is_accepted() {
    assert_compiles(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source, Subscription };
        [derive(Wire, PartialEq, Debug)]
        struct Task { id: i32 }
        struct Stored<T> { inner: SignalCell<T> }
        impl Stored<type T> with Source<T> {
            fun get(self): T { self.inner.get() }
            [must_use]
            fun on_change(self, observer: |T| void): Subscription { self.inner.on_change(observer) }
        }
        [service(StoreClient)]
        struct Store {
            [expose] items: Stored<List<Task>>,
        }
        impl Store {
            [rpc]
            fun count(self): usize { self.items.get().len() }
        }
        fun main() { print(Store { items = Stored { inner = Signal::new([]) } }.contract_hash()); }
        main();
        "#,
    );
}

/// The bare `[expose(keyed)]` over a `List<T>` — A39's refused shape, still
/// refused, with the sentence that now names the way out.
#[test]
fn an_expose_keyed_list_without_a_key_type_names_the_attribute_argument() {
    assert_fails_with(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };
        [derive(Wire, PartialEq, Debug)]
        struct Task { id: str }
        [service(StoreClient)]
        struct Store {
            [expose(keyed)] tasks: SignalCell<List<Task>>,
        }
        fun main() { print("store"); }
        main();
        "#,
        "nothing names its KEY type",
    );
}

/// The key is named, and the collection is one the keyed exposure cannot read.
/// `expose_keyed` takes a `Source<List<T>>` and `expose_keyed_map` a
/// `Source<Map<K, V>>`; those two are what the expansion picks between, off the
/// annotation, before any type resolves — so a third collection has to be told
/// so here or it would simply not be exposed and nothing would say why (B202).
#[test]
fn an_expose_keyed_with_a_key_type_still_needs_a_list_or_a_map() {
    assert_fails_with(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };
        [derive(Wire, PartialEq, Debug)]
        struct Task { id: str }
        [service(StoreClient)]
        struct Store {
            [expose(keyed = str)] tasks: SignalCell<Task>,
        }
        fun main() { print("store"); }
        main();
        "#,
        "its collection is not written as a `List<T>` or a `Map<K, V>`",
    );
}

/// The argument is a TYPE, and the parser says so where it stands rather than
/// backtracking the whole attribute and reporting it as a missing field name.
/// This is the one attribute argument in the language that is a type rather
/// than a word, so the refusal has to say what shape is wanted.
#[test]
fn an_expose_keyed_argument_that_is_not_a_type_is_refused_where_it_stands() {
    assert_fails_with(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };
        [derive(Wire, PartialEq, Debug)]
        struct Task { id: str }
        [service(StoreClient)]
        struct Store {
            [expose(keyed = 3)] tasks: SignalCell<List<Task>>,
        }
        fun main() { print("store"); }
        main();
        "#,
        "`[expose(keyed = …)]`'s argument is a TYPE",
    );
}

/// Both accepted forms, in one program: the `Map` element that names its own
/// key and takes the bare attribute (A39, unchanged), and the `List` element
/// that names its key in the attribute (A51). The control for the three
/// refusals above.
#[test]
fn both_keyed_expose_spellings_compile_side_by_side() {
    assert_compiles(
        r#"
        import std::io::print;
        import std::map::Map;
        import std::reactive::{ Signal, SignalCell };
        import std::wire::Keyed;
        [derive(Wire, PartialEq, Debug)]
        struct Task { id: str }
        impl Task with Keyed<str> {
            fun key(self): str { self.id }
        }
        [service(StoreClient)]
        struct Store {
            [expose(keyed)] by_map: SignalCell<Map<str, Task>>,
            [expose(keyed = str)] by_list: SignalCell<List<Task>>,
        }
        impl Store {
            [rpc]
            fun count(self): usize { self.by_list.get().len() }
        }
        fun main() {
            print(Store { by_map = Signal::new(Map::new()), by_list = Signal::new([]) }.contract_hash());
        }
        main();
        "#,
    );
}

/// A56 / R6: the field names its key TWICE — once in the attribute, once in the
/// `Map` element — and the two disagree.
///
/// The argument used to simply win, which made this silent at the attribute and
/// loud one layer down: before this refusal the program below reported
/// `'Task' does not implement trait 'Keyed<i32>'` FOUR times, each at a call the
/// `[service]` expansion wrote and the author never did. Neither spelling is
/// knowably the intended one, so the refusal names both and offers the two ways
/// out rather than preferring one.
#[test]
fn a56_an_expose_keyed_argument_that_disagrees_with_the_map_key_is_refused() {
    assert_fails_once_with(
        r#"
        import std::io::print;
        import std::map::Map;
        import std::reactive::{ Signal, SignalCell };
        import std::wire::Keyed;
        [derive(Wire, PartialEq, Debug)]
        struct Task { id: str }
        impl Task with Keyed<str> {
            fun key(self): str { self.id }
        }
        [service(StoreClient)]
        struct Store {
            [expose(keyed = i32)] tasks: SignalCell<Map<str, Task>>,
        }
        fun main() { print("store"); }
        main();
        "#,
        "names its key twice and the two disagree",
    );
}

/// A56's covering half, stated as its own claim: the refusal above STANDS ALONE.
/// The generated subscription is handed the same field and fails the same
/// `Keyed<K>` bound for the same reason, and B189's covered set is what keeps it
/// from saying so four more times at a span the author never wrote.
#[test]
fn a56_the_disagreeing_key_refusal_stands_down_the_generated_bound_failures() {
    assert_fails_without(
        r#"
        import std::io::print;
        import std::map::Map;
        import std::reactive::{ Signal, SignalCell };
        import std::wire::Keyed;
        [derive(Wire, PartialEq, Debug)]
        struct Task { id: str }
        impl Task with Keyed<str> {
            fun key(self): str { self.id }
        }
        [service(StoreClient)]
        struct Store {
            [expose(keyed = i32)] tasks: SignalCell<Map<str, Task>>,
        }
        fun main() { print("store"); }
        main();
        "#,
        "required by a generic bound of this call",
    );
}

/// A56's control: an argument written BESIDE a `Map` element is still accepted
/// when the two agree. The refusal is about the disagreement and nothing else —
/// writing the key in both places is redundant, not wrong, and a `[service]`
/// that did it before this landed keeps compiling and keeps its hash.
#[test]
fn a56_an_expose_keyed_argument_that_agrees_with_the_map_key_still_compiles() {
    assert_compiles(
        r#"
        import std::io::print;
        import std::map::Map;
        import std::reactive::{ Signal, SignalCell };
        import std::wire::Keyed;
        [derive(Wire, PartialEq, Debug)]
        struct Task { id: str }
        impl Task with Keyed<str> {
            fun key(self): str { self.id }
        }
        [service(StoreClient)]
        struct Store {
            [expose(keyed = str)] tasks: SignalCell<Map<str, Task>>,
        }
        impl Store {
            [rpc]
            fun count(self): usize { self.tasks.get().len() }
        }
        fun main() {
            print(Store { tasks = Signal::new(Map::new()) }.contract_hash());
        }
        main();
        "#,
    );
}

// --- B258: overriding a trait DEFAULT the receiver's own FIELD type inherits
// ------------------------------------------------------------------------
// A miscompile of the `context` pass's flavor propagation, not of dispatch:
// `candidates_of` is NAME-keyed, so a dispatch site's candidate list held every
// override of every trait declaring the name — including ones the receiver
// could never select. One strict candidate promotes a whole site's flavor
// (`settle_strict`), so `Mirror`'s STRICT override of `Tag::label` rewrote
// `self.cell.label()` — a call on a `Cell` FIELD, which inherits the
// owner-OPTIONAL default — into a bare hand-off with no value to hand: the
// default's `get_safe()` read `Some(undefined)` and answered `under undefined`
// where it should have answered "no scope". std's shape was
// `RemoteSource::map` over `status`'s own `self.cache.map(..)`
// (`register_with_owner` on an undefined owner), which is why `map` was
// inherent until this closed. Fixed by narrowing an `OnType` site with a KNOWN
// receiver to the members that receiver's head selects — the narrowing
// `dispatch_refine` already ran for coverage, now shared with the flavor.

/// The shape itself: a `Mirror` whose FIELD is a `Cell` calling the trait
/// default `Cell` inherits, with `Mirror` overriding that same default in its
/// own sub-trait impl. The field call must run the DEFAULT, with the FIELD as
/// the receiver (`default(cell)`, never `override(mirror)`), and must read the
/// context SAFELY — absent outside a `run`, present inside one.
#[test]
fn b258_a_field_call_on_an_inherited_default_is_not_promoted_by_a_siblings_override() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::context::Context;
        import std::option::Option::{ None, Some, self };

        let scope: Context<str> = Context::new();

        trait Tag<T> {
            fun raw(self): T;
            fun name(self): str;

            // The DEFAULT: an owner-OPTIONAL read.
            fun label(self): str {
                match scope.get_safe() {
                    Some(let value) => i"default({self.name()}) under {value}",
                    None => i"default({self.name()}), no scope",
                }
            }
        }

        struct Cell<T> { value: T }

        impl Cell<type T> with Tag<T> {
            fun raw(self): T { self.value }
            fun name(self): str { "cell" }
        }

        struct Mirror<T> { cell: Cell<T> }

        impl Mirror<type T> with Tag<Option<T>> {
            fun raw(self): Option<T> { Some(self.cell.raw()) }
            fun name(self): str { "mirror" }

            // The OVERRIDE, and a STRICT read.
            fun label(self): str {
                i"override({self.name()}) under {scope.get()}"
            }
        }

        impl Mirror<type T> {
            // The field call: `self.cell` is a `Cell<T>`, which INHERITS.
            fun field_label(self): str { self.cell.label() }
        }

        fun main() {
            let mirror = Mirror { cell = Cell { value = 7 } };
            print(mirror.field_label());
            print(scope.run("s", || mirror.field_label()));
            print(scope.run("s", || mirror.label()));
        }
        main();
        "#,
        "default(cell), no scope\n\
         default(cell) under s\n\
         override(mirror) under s\n",
    );
}

/// The control: where the override IS the receiver's own type, the strict read
/// still fences. `mirror.label()` outside every `run` is the coverage refusal —
/// narrowing the candidate list must not weaken the law on the receiver that
/// really does select the strict body.
#[test]
fn b258_the_override_still_fences_on_its_own_receiver() {
    assert_fails_with(
        r#"
        import std::io::print;
        import std::context::Context;
        import std::option::Option::{ None, Some, self };

        let scope: Context<str> = Context::new();

        trait Tag<T> {
            fun raw(self): T;

            fun label(self): str {
                match scope.get_safe() {
                    Some(let value) => i"default under {value}",
                    None => "default, no scope",
                }
            }
        }

        struct Cell<T> { value: T }

        impl Cell<type T> with Tag<T> {
            fun raw(self): T { self.value }
        }

        struct Mirror<T> { cell: Cell<T> }

        impl Mirror<type T> with Tag<Option<T>> {
            fun raw(self): Option<T> { Some(self.cell.raw()) }

            fun label(self): str {
                i"override under {scope.get()}"
            }
        }

        fun main() {
            let mirror = Mirror { cell = Cell { value = 7 } };
            print(mirror.label());
        }
        main();
        "#,
        "context `scope` is read here, but this code can be reached without an enclosing `run`",
    );
}

// --- §9.2: a signal HANDLE in a return position (Order 31, handles-31) ------

/// The `[rpc]` Wire rule moves inward by one type argument when the return is
/// a handle, and this is where it lands.
///
/// A `SignalCell<T>` return is not meant to be Wire and never crosses: the
/// source stays on the server, a `ChannelId` goes on the wire, and the client
/// mirrors it. What DOES cross is the element, in every `Update` frame the
/// channel carries — so the refusal names the element, in the return type's own
/// vocabulary, rather than saying `SignalCell<Secret>` "is not Wire", which
/// would be true and useless (nothing about the wrapper is the problem).
#[test]
fn a_handle_return_whose_element_is_not_wire_is_refused_at_the_element() {
    assert_fails_with(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };
        struct Secret { token: str }
        [service(StoreClient)]
        struct Store {
            secret: SignalCell<Secret>,
        }
        impl Store {
            [rpc]
            fun watch(self): SignalCell<Secret> { self.secret }
        }
        fun main() { print("store"); }
        main();
        "#,
        "returns a signal handle whose element `Secret` is not Wire",
    );
}

/// The control, and the two shapes the mapping admits: `SignalCell<T>` becomes
/// `RemoteSource<T>` at the client and `Option<SignalCell<T>>` becomes
/// `Option<RemoteSource<T>>`, both over a Wire element.
#[test]
fn a_handle_return_over_a_wire_element_compiles_in_both_forms() {
    assert_compiles(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        import std::reactive::{ Signal, SignalCell };
        [derive(Wire, PartialEq, Debug)]
        struct Task { id: i32 }
        [service(StoreClient)]
        struct Store {
            task: SignalCell<Task>,
        }
        impl Store {
            [rpc]
            fun watch(self, id: i32): SignalCell<Task> { self.task }

            [rpc]
            fun find(self, id: i32): Option<SignalCell<Task>> {
                if id == 0 { Some(self.task) } else { None }
            }
        }
        fun main() { print(Store { task = Signal::new(Task { id = 1 }) }.contract_hash()); }
        main();
        "#,
    );
}

/// B326 (Order 35 R4): `Option<KeyedCell<K, T>>` is NOT a handle return, and
/// says so in the METHOD's own vocabulary.
///
/// std's `[service]` macro deliberately declines the form — its own rule is
/// `handle_key(inner) == ""`, because reading it as a handle would mint a PLAIN
/// mirror over a keyed channel, and supporting it properly needs a per-KEY
/// `Absent` beside A92's per-source one, which nothing has asked for. The
/// Rust-side return rule descended anyway, so the ordinary Wire refusal stood
/// down and the author read `'Option<KeyedCell<i32, Task>>' does not implement
/// trait 'Wire'` out of code the attribute generated — twice, naming functions
/// nobody wrote. The two halves agree now and the form falls to the rule that
/// was always going to refuse it.
#[test]
fn b326_an_option_around_a_keyed_handle_falls_to_the_ordinary_wire_refusal() {
    let source = r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        import std::rpc::KeyedCell;
        import std::wire::{ Keyed, Wire };
        [derive(Wire, PartialEq, Debug)]
        struct Task { id: i32, title: str }
        impl Task with Keyed<i32> {
            fun key(self): i32 { self.id }
        }
        [service(BoardClient)]
        struct Board { seen: i32 }
        impl Board {
            [rpc]
            fun tasks(self, page: i32): Option<KeyedCell<i32, Task>> { None }
        }
        fun main() { print("board"); }
        main();
        "#;
    assert_fails_once_with(
        source,
        "return type of `[rpc]` method `tasks` is `Option<KeyedCell<i32, Task>>`, which is not \
         Wire",
    );
    assert_fails_without(source, "in code generated by this attribute");
}

/// The control the refusal above is held against: `Option<SignalCell<T>>` is
/// still a handle return and still maps to `Option<RemoteSource<T>>`. Only the
/// KEYED inner is declined, which is exactly the macro's own rule.
#[test]
fn b326_an_option_around_a_plain_handle_is_still_a_handle_return() {
    assert_compiles(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        import std::reactive::{ Signal, SignalCell };
        [derive(Wire, PartialEq, Debug)]
        struct Task { id: i32 }
        [service(StoreClient)]
        struct Store {
            task: SignalCell<Task>,
        }
        impl Store {
            [rpc]
            fun find(self, id: i32): Option<SignalCell<Task>> {
                if id == 0 { Some(self.task) } else { None }
            }
        }
        fun main() { print(Store { task = Signal::new(Task { id = 1 }) }.contract_hash()); }
        main();
        "#,
    );
}

/// B327: the `[derive(Hashable)]` boundary sees a HAND-WRITTEN `impl .. with
/// Hashable`.
///
/// The check asked a syntactic predicate that read the scalars and the names of
/// the derives and backed enums, so an impl written by hand was invisible to it
/// and a field of such a type was refused for not being Hashable — B289's class
/// for `Wire`, and it only ever rejected valid programs. It asks
/// `resolved_type_is_hashable` now, which is the same oracle the `[rpc]` keyed
/// return check asks, so the two cannot disagree about one type.
#[test]
fn b327_a_hand_written_hashable_impl_satisfies_the_derive_boundary() {
    assert_compiles(
        r#"
        import std::io::print;
        import std::hash::{ Hash, Hashable, canonical_hash };
        struct Custom { id: i32 }
        impl Custom with Hashable {
            fun hash(self): Hash { canonical_hash(self.id) }
        }
        [derive(Hashable)]
        struct Key { inner: Custom }
        fun main() { print("key"); }
        main();
        "#,
    );
}

/// The control: the same field with NO `Hashable` impl anywhere is still
/// refused, and the refusal now names the impl among the shapes it admits.
#[test]
fn b327_a_field_with_no_hashable_impl_is_still_refused() {
    assert_fails_with(
        r#"
        import std::io::print;
        struct Custom { id: i32 }
        [derive(Hashable)]
        struct Key { inner: Custom }
        fun main() { print("key"); }
        main();
        "#,
        "field `inner` of `[derive(Hashable)]` type `Key` is `Custom`, which is not `Hashable`",
    );
    assert_fails_with(
        r#"
        import std::io::print;
        struct Custom { id: i32 }
        [derive(Hashable)]
        struct Key { inner: Custom }
        fun main() { print("key"); }
        main();
        "#,
        "or a type with an `impl .. with Hashable`",
    );
}

/// B319: a KEYED handle's key is held to `Wire + Hashable` in the method's own
/// vocabulary, on the annotation the author wrote.
///
/// The return rule read the second type argument (the element) and never the
/// first, so `KeyedCell<NotWire, Task>` passed here and failed inside the
/// generated `reply_source_keyed<K: Wire + Hashable, ..>` — four reports, each
/// naming a function nobody wrote, none of them saying which annotation was
/// wrong. Both halves of the bound at once, because a key that is neither fails
/// both and two sentences about one annotation is the defect this closes; the
/// generated-code failures for that key stand down (B303's stand-down for the
/// `Wire` half, B319's own for `Hashable`).
#[test]
fn a_keyed_handle_return_whose_key_is_not_wire_is_refused_at_the_key() {
    let neither = r#"
        import std::io::print;
        import std::rpc::KeyedCell;
        import std::shared::Shared;
        import std::wire::{ Keyed, Wire };
        struct NotWire { blob: Shared<i32> }
        [derive(Wire, PartialEq, Debug)]
        struct Task { id: i32 }
        impl Task with Keyed<NotWire> {
            fun key(self): NotWire { NotWire { blob = Shared::new(self.id) } }
        }
        [service(BoardClient)]
        struct Board { seen: i32 }
        impl Board {
            [rpc]
            fun tasks(self, page: i32): KeyedCell<NotWire, Task> { KeyedCell::new([]) }
        }
        fun main() { print("board"); }
        main();
        "#;
    assert_fails_once_with(
        neither,
        "returns a keyed handle whose key `NotWire` is neither Wire nor Hashable",
    );
    assert_fails_once_with(neither, "`[rpc]` method `tasks` returns a keyed handle");
    assert_fails_without(neither, "in code generated by this attribute");

    // Wire and NOT Hashable: the generated bound is the pair, so the half that
    // is missing is the half the message names.
    let unhashable = r#"
        import std::io::print;
        import std::rpc::KeyedCell;
        import std::wire::{ Keyed, Wire };
        [derive(Wire, PartialEq, Debug)]
        struct Slug { text: str }
        [derive(Wire, PartialEq, Debug)]
        struct Task { id: i32, title: str }
        impl Task with Keyed<Slug> {
            fun key(self): Slug { Slug { text = self.title } }
        }
        [service(BoardClient)]
        struct Board { seen: i32 }
        impl Board {
            [rpc]
            fun tasks(self, page: i32): KeyedCell<Slug, Task> { KeyedCell::new([]) }
        }
        fun main() { print("board"); }
        main();
        "#;
    assert_fails_once_with(
        unhashable,
        "`[rpc]` method `tasks` returns a keyed handle whose key `Slug` is not Hashable",
    );
    assert_fails_without(unhashable, "in code generated by this attribute");
}

/// The controls: a scalar key, and a key that is Wire and Hashable by DERIVE —
/// which is also what holds the two Hashable oracles together, since the
/// resolved one this rule asks and the syntactic one the derive boundary asks
/// must answer alike or a key the derive admits is refused here.
#[test]
fn a_keyed_handle_return_over_a_wire_hashable_key_compiles() {
    assert_compiles(
        r#"
        import std::io::print;
        import std::rpc::KeyedCell;
        import std::wire::{ Keyed, Wire };
        [derive(Wire, PartialEq, Debug)]
        struct Task { id: i32, title: str }
        impl Task with Keyed<i32> {
            fun key(self): i32 { self.id }
        }
        [service(BoardClient)]
        struct Board { seen: i32 }
        impl Board {
            [rpc]
            fun tasks(self, page: i32): KeyedCell<i32, Task> { KeyedCell::new([]) }
        }
        fun main() { print("board"); }
        main();
        "#,
    );
    assert_compiles(
        r#"
        import std::io::print;
        import std::hash::Hashable;
        import std::rpc::KeyedCell;
        import std::wire::{ Keyed, Wire };
        [derive(Wire, Hashable, PartialEq, Debug)]
        struct Slug { text: str }
        [derive(Wire, PartialEq, Debug)]
        struct Task { id: i32, title: str }
        impl Task with Keyed<Slug> {
            fun key(self): Slug { Slug { text = self.title } }
        }
        [service(BoardClient)]
        struct Board { seen: i32 }
        impl Board {
            [rpc]
            fun tasks(self, page: i32): KeyedCell<Slug, Task> { KeyedCell::new([]) }
        }
        fun main() { print("board"); }
        main();
        "#,
    );
    // And the third control, which is the one the message's steer promises and
    // the only one a name set could not answer: `Hashable` by a HAND-WRITTEN
    // impl. The rule asks the impl table, as `resolved_type_is_wire` does since
    // B289, so this key is as good as a derived one.
    assert_compiles(
        r#"
        import std::io::print;
        import std::hash::{ Hashable, Hash, canonical_hash };
        import std::rpc::KeyedCell;
        import std::wire::{ Keyed, Wire };
        [derive(Wire, PartialEq, Debug)]
        struct Slug { text: str }
        impl Slug with Hashable {
            fun hash(self): Hash { canonical_hash(self.text) }
        }
        [derive(Wire, PartialEq, Debug)]
        struct Task { id: i32, title: str }
        impl Task with Keyed<Slug> {
            fun key(self): Slug { Slug { text = self.title } }
        }
        [service(BoardClient)]
        struct Board { seen: i32 }
        impl Board {
            [rpc]
            fun tasks(self, page: i32): KeyedCell<Slug, Task> { KeyedCell::new([]) }
        }
        fun main() { print("board"); }
        main();
        "#,
    );
}

/// The recognition boundary, said out loud. The expansion reads a handle
/// return off the WRITTEN spelling — it runs before any type resolves, exactly
/// as `[expose]` does — so a user type that implements `Source<T>` in a return
/// position is NOT read as a handle. It is not silently mis-generated either:
/// it falls to the ordinary `[rpc]` Wire rule, which refuses it by name. The
/// same type is still exposable as a FIELD (`a52_an_expose_of_a_user_source_
/// type_is_accepted` is the other half), because there the `[expose]` marker
/// is what declares the intent and the analyzer reconciles the impl.
#[test]
fn a_user_source_type_in_a_return_position_is_not_read_as_a_handle() {
    assert_fails_with(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source, Subscription };
        [derive(Wire, PartialEq, Debug)]
        struct Task { id: i32 }
        struct Stored<T> { inner: SignalCell<T> }
        impl Stored<type T> with Source<T> {
            fun get(self): T { self.inner.get() }
            [must_use]
            fun on_change(self, observer: |T| void): Subscription { self.inner.on_change(observer) }
        }
        [service(StoreClient)]
        struct Store {
            items: Stored<Task>,
        }
        impl Store {
            [rpc]
            fun items(self): Stored<Task> { self.items }
        }
        fun main() { print("store"); }
        main();
        "#,
        "which is not Wire",
    );
}

// --- B284: `[expose]` on a struct that HANDLES and does not SERVE -----------

/// B284: `[expose]` on a `[client_service]`-ONLY struct was a silent no-op.
///
/// It compiled, it contributed an `expose:` entry to the struct's contract
/// hash — so the two peers had to agree about a channel neither could
/// mint — and nothing ever minted one: a client-side struct has no reactive
/// session to export out of, and the expansion generates no `__attach` route
/// for it at all. Refused at the attribute now, in the field's own vocabulary,
/// for the same reason every other arm of `check_expose_fields` is said there
/// (B202): the expansion skips such a field, so without this nothing says why.
#[test]
fn b284_an_expose_on_a_client_service_only_struct_is_refused() {
    assert_fails_once_with(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };
        [client_service]
        struct Handlers {
            [expose] tally: SignalCell<i32>,
        }
        impl Handlers {
            [rpc]
            fun session_revoked(self, reason: str) { print(reason); }
        }
        fun main() { print(Handlers { tally = Signal::new(0) }.contract_hash()); }
        main();
        "#,
        "carries only `[client_service]`",
    );
}

/// B284's keyed spelling: the refusal is about the STRUCT, not about the shape
/// of the exposure, so `[expose(keyed)]` over a `Map` element on a client-only
/// struct is refused by the same arm — and by it ALONE, rather than also
/// collecting the keyed arms' own complaints, because the early arm answers
/// first and the field is done.
#[test]
fn b284_an_expose_keyed_on_a_client_service_only_struct_is_refused_once() {
    assert_fails_once_with(
        r#"
        import std::io::print;
        import std::map::Map;
        import std::reactive::{ Signal, SignalCell };
        import std::wire::Keyed;
        [derive(Wire, PartialEq, Debug)]
        struct Task { id: str }
        impl Task with Keyed<str> {
            fun key(self): str { self.id }
        }
        [client_service]
        struct Handlers {
            [expose(keyed)] tasks: SignalCell<Map<str, Task>>,
        }
        impl Handlers {
            [rpc]
            fun session_revoked(self, reason: str) { print(reason); }
        }
        fun main() { print(Handlers { tasks = Signal::new(Map::new()) }.contract_hash()); }
        main();
        "#,
        "carries only `[client_service]`",
    );
}

/// B284's control, and the reason the refusal is keyed on the DECLARATION
/// rather than on `[client_service]` being present: a PEER struct carries both
/// attributes, so it serves as well as handles — it has a dispatcher, an
/// `__attach` route and a reactive session per connection, and its exposures
/// are real. A program that wrote one before this landed keeps compiling and
/// keeps its hash.
#[test]
fn b284_an_expose_on_a_peer_struct_that_also_serves_still_compiles() {
    assert_compiles(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };
        [service(PeerClient)]
        [client_service]
        struct Peer {
            [expose] tally: SignalCell<i32>,
        }
        impl Peer {
            [rpc]
            fun bump(self, by: i32) { self.tally.set(self.tally.get() + by); }
        }
        fun main() { print(Peer { tally = Signal::new(0) }.contract_hash()); }
        main();
        "#,
    );
}

/// B284's other control: a client-only struct with no `[expose]` at all is
/// untouched. The refusal is the attribute's, not the attribute pair's.
#[test]
fn b284_a_client_service_only_struct_without_an_expose_still_compiles() {
    assert_compiles(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };
        [client_service]
        struct Handlers {
            tally: SignalCell<i32>,
        }
        impl Handlers {
            [rpc]
            fun session_revoked(self, reason: str) { print(reason); }
        }
        fun main() { print(Handlers { tally = Signal::new(0) }.contract_hash()); }
        main();
        "#,
    );
}

// --- A107 (R6): a `void` `[rpc]` return -------------------------------------

/// A107: a `[rpc]` method that returns NOTHING is admitted, in both of void's
/// spellings — the omitted return type and an explicit `: void`.
///
/// It is the one return type that is not Wire and does not have to be: there
/// is no reply PAYLOAD, so the reply is the ack envelope the protocol already
/// writes and the generated stub awaits it (`rpc::call_ack`, answering
/// `Result<void, RpcError>`). Before this, both spellings were refused — the omitted
/// one as "must declare a Wire type", the written one as "`void`, which is not
/// Wire" — and kolt's `store.vl` wrote `bool` for a method with nothing to
/// report, with a FIXME naming this item.
///
/// The end-to-end behaviour, including the ordering that distinguishes this
/// from a notification, is `service_layer.rs`'s
/// `an_awaited_void_rpc_acks_after_its_handler_ran`; this is the admission.
#[test]
fn a107_a_void_rpc_return_is_admitted_in_both_spellings() {
    for source in [
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };
        [service(StoreClient)]
        struct Store { rows: SignalCell<i32> }
        impl Store {
            [rpc]
            fun bump(self, by: i32) { self.rows.set(self.rows.get() + by); }
        }
        fun main() { print(Store { rows = Signal::new(0) }.contract_hash()); }
        main();
        "#,
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };
        [service(StoreClient)]
        struct Store { rows: SignalCell<i32> }
        impl Store {
            [rpc]
            fun bump(self, by: i32): void { self.rows.set(self.rows.get() + by); }
        }
        fun main() { print(Store { rows = Signal::new(0) }.contract_hash()); }
        main();
        "#,
    ] {
        assert_compiles(source);
    }
}

/// The first control: admitting `void` widened NOTHING else. A return type
/// that is a real type and is not Wire is refused exactly as before, in the
/// same words.
#[test]
fn a107_a_non_wire_rpc_return_is_still_refused() {
    assert_fails_with(
        r#"
        import std::io::print;
        struct Opaque { body: || void }
        [service(StoreClient)]
        struct Store { name: str }
        impl Store {
            [rpc]
            fun look(self): Opaque { Opaque { body = || {} } }
        }
        fun main() { print("store"); }
        main();
        "#,
        "of `[rpc]` method `look` is `Opaque`, which is not Wire",
    );
}

/// The second control, and the one that says where the two void calls part: on
/// a `[client_service]` subject a DECLARED return type is still refused,
/// `void` included, because omission is that direction's only legal spelling.
///
/// This is not an oversight and it is not a narrower rule than the server's.
/// The `s:` lane has no reverse reply lane at all (§9.3, R4), so there is no
/// ack for that direction's caller to await — a client-side `[rpc]` method is
/// a notification, and the refusal's own steer ("Drop the return type") is
/// exactly right for someone who wrote `: void` there.
#[test]
fn a107_a_void_return_on_a_client_service_is_still_the_notification_refusal() {
    assert_fails_with(
        r#"
        import std::io::print;
        [client_service]
        struct Handlers { tag: str }
        impl Handlers {
            [rpc]
            fun revoked(self, reason: str): void { print(reason); }
        }
        fun main() { print(Handlers { tag = "h" }.contract_hash()); }
        main();
        "#,
        "but a `[client_service]` method is a NOTIFICATION",
    );
}

/// Ledger row 5 (`{label} of \x60[rpc]\x60 method \x60{method_name}\x60 must declare a
/// Wire type`) is still REACHABLE after A107 took the return type off it: an
/// unannotated PARAMETER is what it now speaks about, and that is a shape a
/// person writes.
///
/// Pinned because the widening could have retired a message silently, and a
/// ledger row whose text lives in the tree but whose firing does not is the
/// defect N98's family exists to find.
#[test]
fn a107_the_declare_a_wire_type_refusal_still_reaches_an_unannotated_parameter() {
    assert_fails_with(
        r#"
        import std::io::print;
        [service(StoreClient)]
        struct Store { name: str }
        impl Store {
            [rpc]
            fun keep(self, row) { print("kept"); }
        }
        fun main() { print("store"); }
        main();
        "#,
        "must declare a Wire type",
    );
}

// --- B285: `[expose(keyed = K)]` over a `KeyedCell<K2, T>` ------------------

/// B285: the `KeyedCell<K, T>` twin of A56/R6 — the field names its key twice
/// and the two disagree.
///
/// The cell's own `K` won, and had to: `expose_keyed_cell` and the
/// `KeyedSource<K, T>` mirror are typed at it, so a disagreeing argument could
/// only generate code that does not compile. What was wrong was winning
/// SILENTLY — the program below hashed byte-identically to the one that writes
/// `keyed = str`, and mirrored by a key the author had spelled otherwise on the
/// same line.
#[test]
fn b285_an_expose_keyed_argument_that_disagrees_with_the_cells_key_is_refused() {
    assert_fails_once_with(
        r#"
        import std::io::print;
        import std::rpc::KeyedCell;
        import std::wire::Keyed;
        [derive(Wire, PartialEq, Debug)]
        struct Task { id: str }
        impl Task with Keyed<str> {
            fun key(self): str { self.id }
        }
        [service(StoreClient)]
        struct Store {
            [expose(keyed = i32)] tasks: KeyedCell<str, Task>,
        }
        fun main() { print("store"); }
        main();
        "#,
        "names its key twice and the two disagree",
    );
}

/// B285's span, stated as its own claim: the ARGUMENT is the half under
/// discussion — the cell's type is not wrong, and one of the two ways out does
/// not touch it — so the refusal points there and not at the annotation. A56's
/// rule, applied to the second spelling that carries a key.
#[test]
fn b285_the_disagreeing_cell_key_refusal_spans_the_attribute_argument() {
    assert_fails_spanning(
        r#"
        import std::io::print;
        import std::rpc::KeyedCell;
        import std::wire::Keyed;
        [derive(Wire, PartialEq, Debug)]
        struct Task { id: str }
        impl Task with Keyed<str> {
            fun key(self): str { self.id }
        }
        [service(StoreClient)]
        struct Store {
            [expose(keyed = i32)] tasks: KeyedCell<str, Task>,
        }
        fun main() { print("store"); }
        main();
        "#,
        "i32",
        "names its key twice and the two disagree",
    );
}

/// B285's control: an argument that AGREES with the cell's key is redundant,
/// not wrong. It compiles, and it hashes exactly as the bare form does — the
/// surface entry is built from the cell's own written key either way.
#[test]
fn b285_an_expose_keyed_argument_that_agrees_with_the_cells_key_still_compiles() {
    assert_compiles(
        r#"
        import std::io::print;
        import std::rpc::KeyedCell;
        import std::wire::Keyed;
        [derive(Wire, PartialEq, Debug)]
        struct Task { id: str }
        impl Task with Keyed<str> {
            fun key(self): str { self.id }
        }
        [service(StoreClient)]
        struct Store {
            [expose(keyed = str)] tasks: KeyedCell<str, Task>,
        }
        impl Store {
            [rpc]
            fun count(self): i32 { 1 }
        }
        fun main() {
            print(Store { tasks = KeyedCell::new([]) }.contract_hash());
        }
        main();
        "#,
    );
}

/// B285's other two controls, which are the shapes the refusal must NOT reach:
/// the bare `[expose(keyed)]` over a cell (A54's spelling), and the bare
/// `[expose]` over one — which is the SAME keyed channel, because the cell
/// names both types itself and there is nothing for the attribute to add.
#[test]
fn b285_a_keyed_cell_exposed_without_an_argument_still_compiles() {
    assert_compiles(
        r#"
        import std::io::print;
        import std::rpc::KeyedCell;
        import std::wire::Keyed;
        [derive(Wire, PartialEq, Debug)]
        struct Task { id: str }
        impl Task with Keyed<str> {
            fun key(self): str { self.id }
        }
        [service(StoreClient)]
        struct Store {
            [expose(keyed)] keyed_tasks: KeyedCell<str, Task>,
            [expose] bare_tasks: KeyedCell<str, Task>,
        }
        impl Store {
            [rpc]
            fun count(self): i32 { 1 }
        }
        fun main() {
            let keyed: KeyedCell<str, Task> = KeyedCell::new([Task { id = "a" }]);
            let bare: KeyedCell<str, Task> = KeyedCell::new([Task { id = "b" }]);
            print(Store { keyed_tasks = keyed, bare_tasks = bare }.contract_hash());
        }
        main();
        "#,
    );
}

// --- A78: a handle-returning service on a CONNECTIONLESS mount -------------

/// A78: a `local_rpc` transport over a protocol no connection stamped cannot
/// answer a handle-returning method, and says so AT WIRING TIME.
///
/// A handle's reply is a channel id minted in the CONNECTION's capability
/// table; `into_protocol` leaves the connection unstamped (`for_connection` is
/// what stamps it), so the call could only ever fail. It used to fail at the
/// first such call, as an `RpcError::Remote` raised inside generated code that
/// named neither the method nor the wiring that had to change — and a service
/// whose handle method is called on some later code path shipped with the
/// defect latent. The `[service]` expansion records its handle methods on the
/// dispatcher (`Dispatcher::handles`), so `local_rpc` can see them before a
/// call is made.
#[test]
fn a78_a_handle_service_on_an_unstamped_local_rpc_protocol_is_refused_at_wiring() {
    assert_run_panics(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };
        import std::json::json_codec;
        import std::rpc::local_rpc;
        [service(NotesClient)]
        struct Notes {
            body: SignalCell<str>,
        }
        impl Notes {
            [rpc]
            fun note(self, id: str): SignalCell<str> { self.body }
        }
        fun main() {
            let notes = Notes { body = Signal::new("hello") };
            let transport = local_rpc(notes.dispatcher().into_protocol(json_codec()));
            print("wired");
        }
        main();
        "#,
        "a signal handle is returned by `note`",
    );
}

/// A78's control, and the whole of the test the refusal makes: a STAMPED
/// protocol is admitted, and the handle round-trips in process.
///
/// `vilan/examples/rpc` is written this way — register a session, stamp the
/// protocol with its connection, serve handles locally — and the refusal must
/// not reach it. The test is structural (was a connection stamped?) rather
/// than a `session_of` lookup for this pin's sake and the example's alike: a
/// session may legitimately be registered after the transport is built, and an
/// ordering the app is free to choose must not decide whether its build
/// survives.
#[test]
fn a78_a_handle_service_on_a_stamped_local_rpc_protocol_still_round_trips() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };
        import std::result::Result::{ self, Ok, Err };
        import std::json::{ Json, FromJson };
        import std::json::json_codec;
        import std::rpc::{ local_rpc, duplex_pair, register_session, ReactiveClient, RemoteSource };
        [service(NotesClient)]
        struct Notes {
            body: SignalCell<str>,
        }
        impl Notes {
            [rpc]
            fun note(self, id: str): SignalCell<str> { self.body }

            // A round trip to settle the mint the lease below issues.
            [rpc]
            fun ping(self): i32 { 1 }
        }
        fun main() {
            let notes = Notes { body = Signal::new("hello") };
            let (client_end, server_end) = duplex_pair();
            let connection = 0;
            register_session(connection, server_end, json_codec());
            let transport = local_rpc(notes
                .dispatcher()
                .into_protocol(json_codec())
                .for_connection(connection));
            let reactive = ReactiveClient::new(client_end, json_codec());
            let client = NotesClient { transport, codec = json_codec(), reactive };
            // Sync since A92: the mirror is in hand, and the `sub` is what
            // issues the call — which is what this pin is about.
            let mirror: RemoteSource<str> = client.note("welcome");
            let reading = mirror.sub(|text| print(i"note = {text}"));
            match client.ping() {
                Ok(let _settled) => {},
                Err(let _unreached) => {},
            }
            reading.dispose();
        }
        main();
        "#,
        "note = hello\n",
    );
}

/// A78's other control: a service with NO handle method records nothing, so a
/// plain `local_rpc` mount is untouched. The refusal is about the one return
/// shape that needs a connection, and a dispatcher written by hand records
/// nothing either — which is why `handles` is a record and not a rule.
#[test]
fn a78_a_plain_service_on_an_unstamped_local_rpc_protocol_still_round_trips() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::result::Result::{ self, Ok, Err };
        import std::json::{ Json, FromJson };
        import std::json::json_codec;
        import std::rpc::local_rpc;
        [service(NotesClient)]
        struct Notes {
            label: str,
        }
        impl Notes {
            [rpc]
            fun touch(self): i32 { 7 }
        }
        fun main() {
            let notes = Notes { label = "n" };
            let transport = local_rpc(notes.dispatcher().into_protocol(json_codec()));
            let client = NotesClient { transport, codec = json_codec() };
            print(i"touch = {client.touch().unwrap_or(0)}");
        }
        main();
        "#,
        "touch = 7\n",
    );
}

#[test]
fn a86_an_inherent_blanket_over_a_trait_binds_its_argument_from_the_receiver() {
    // `T` is written inside the BOUND, not in the subject, so the
    // receiver/subject reconciliation binds only `S` and left `T` a hole: the
    // call's type "was never fully determined" and every use of the result was
    // refused against an unbounded parameter, so an annotation
    // (`let sampled: i32 = cell.sample();`) had to do the solver's work. The
    // receiver's own `Cell<i32>: Read<i32>` decides `T`, and B300 binds it
    // (`bind_subject_bound_binders`). Unannotated here on purpose — that is
    // the half that was broken, and `flatten` would otherwise demand an
    // annotation at every call site that has done without one since A4,
    // `vilan/test/reactive-flatten.vl` included.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Read<T> { fun get(self): T; }
        struct Cell<T> { value: T }
        impl Cell<type T> with Read<T> {
            fun get(self): T { self.value }
        }

        impl type S: Read<type T> {
            fun sample(self): T { self.get() }
        }

        fun main() {
            let cell = Cell { value = 7 };
            let sampled = cell.sample();
            print(sampled + 1);
        }

        main();
        "#,
        "8\n",
    );
}

#[test]
fn a86_a_blanket_over_a_nested_bound_resolves_its_receiver_to_a_concrete_impl() {
    // The nested face, and it failed harder: with the argument of the bound
    // itself bounded (`I: Read<U>` inside `Read<I>`), `I` stayed a hole too,
    // so `self.get().get()` inside the body dispatched on an unresolved
    // receiver, resolved to the TRAIT's bodiless requirement and stopped the
    // compiler with an internal error naming it — even with the result
    // annotated, and even for a fully concrete receiver. Binding `I` from the
    // receiver's `Read` impl makes `I`'s OWN bound answerable, which is what
    // binds `U`, so B300's worklist closes both at once. This is the shape the
    // A86 `flatten` blanket needs.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Read<T> { fun get(self): T; }
        struct Cell<T> { value: T }
        impl Cell<type T> with Read<T> {
            fun get(self): T { self.value }
        }

        impl type S: Read<type I: Read<type U>> {
            fun join(self): U { self.get().get() }
        }

        fun main() {
            let inner = Cell { value = 7 };
            let outer = Cell { value = inner };
            let joined: i32 = outer.join();
            print(joined);
        }

        main();
        "#,
        "7\n",
    );
}

#[test]
fn b300_a_bound_argument_under_a_constructor_binds_from_the_receiver() {
    // A86's `or` shape: the binder sits UNDER a constructor in the bound
    // (`Read<Option<type T>>`), so binding it needs the receiver's provided
    // `Option<i32>` matched against the written `Option<T>` — not the
    // whole-argument shortcut. `initial: T` is the parameter B300's item says
    // an ARGUMENT could fix; nothing is annotated here, and the `None` arm
    // proves the parameter typed as `i32` rather than riding the argument.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        trait Read<T> { fun get(self): T; }
        struct Cell<T> { value: T }
        impl Cell<type T> with Read<T> {
            fun get(self): T { self.value }
        }

        impl type S: Read<Option<type T>> {
            fun or_default(self, initial: T): T {
                match self.get() {
                    Some(let inner) => inner,
                    None => initial,
                }
            }
        }

        fun main() {
            let filled: Cell<Option<i32>> = Cell { value = Some(7) };
            let empty: Cell<Option<i32>> = Cell { value = None };
            print(filled.or_default(1));
            print(empty.or_default(2));
        }

        main();
        "#,
        "7\n2\n",
    );
}

#[test]
fn b300_a_multi_parameter_bound_binds_every_argument_from_the_receiver() {
    // The multi-parameter face: both of the bound's arguments are binders, and
    // each must come from the receiver's own impl independently — the two
    // results are used at DIFFERENT concrete types with no annotation, so a
    // half-done job shows up on whichever side was left a hole.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Pair<A, B> {
            fun left(self): A;
            fun right(self): B;
        }
        struct Two<A, B> { first: A, second: B }
        impl Two<type A, type B> with Pair<A, B> {
            fun left(self): A { self.first }
            fun right(self): B { self.second }
        }

        impl type S: Pair<type A, type B> {
            fun first_of(self): A { self.left() }
            fun second_of(self): B { self.right() }
        }

        fun main() {
            let two = Two { first = 7, second = "x" };
            print(two.first_of() + 1);
            print(two.second_of() + "!");
        }

        main();
        "#,
        "8\nx!\n",
    );
}

#[test]
fn b300_a_bound_binder_is_bound_for_a_constructor_headed_subject_too() {
    // The mixed face: the subject is constructor-headed (`Holder<type S>`) and
    // the bound rides its binder, so the subject reconciliation binds `S` and
    // the bound pass must still reach `T` through `S`'s own impl. Nothing here
    // is a blanket.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Read<T> { fun get(self): T; }
        struct Cell<T> { value: T }
        impl Cell<type T> with Read<T> {
            fun get(self): T { self.value }
        }
        struct Holder<S> { source: S }

        impl Holder<type S: Read<type T>> {
            fun read(self): T { self.source.get() }
        }

        fun main() {
            let held = Holder { source = Cell { value = 7 } };
            let value = held.read();
            print(value + 1);
        }

        main();
        "#,
        "8\n",
    );
}

#[test]
fn b300_a_specific_impl_still_outranks_the_bounded_blanket() {
    // The ordering control: binding the bound's argument must not change WHICH
    // impl answers. `Cell`'s own inherent `sample` is more specific than the
    // blanket over every `Read`, so it wins here exactly as it did before —
    // and the blanket still answers for a type that has no inherent one.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Read<T> { fun get(self): T; }
        struct Cell<T> { value: T }
        impl Cell<type T> with Read<T> {
            fun get(self): T { self.value }
        }
        struct Boxed<T> { value: T }
        impl Boxed<type T> with Read<T> {
            fun get(self): T { self.value }
        }
        impl Cell<type T> {
            fun sample(self): T { print("inherent"); self.value }
        }

        impl type S: Read<type T> {
            fun sample(self): T { print("blanket"); self.get() }
        }

        fun main() {
            let cell = Cell { value = 7 };
            print(cell.sample() + 1);
            let boxed = Boxed { value = 9 };
            print(boxed.sample() + 1);
        }

        main();
        "#,
        "inherent\n8\nblanket\n10\n",
    );
}

#[test]
fn b300_an_unbounded_blanket_binder_is_left_alone() {
    // The leniency control: a subject binder whose bound names no arguments
    // (or none at all) has nothing to ground from the receiver, and the bound
    // pass must leave it exactly as the subject reconciliation left it.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Tag { fun tag(self): str; }
        struct Cell<T> { value: T }
        impl Cell<type T> with Tag {
            fun tag(self): str { "cell" }
        }

        impl type S: Tag {
            fun describe(self): str { i"<{self.tag()}>" }
        }

        fun main() {
            let cell = Cell { value = 7 };
            print(cell.describe());
        }

        main();
        "#,
        "<cell>\n",
    );
}

// ---------------------------------------------------------------------------
// B371 — a nested-bound blanket reached from inside a GENERIC body
// ---------------------------------------------------------------------------
//
// `self.map(f).flatten()` inside `fun switch<U, I: Source<U>>` stopped the
// compiler with "internal: a call resolved to `Source`'s requirement `get`,
// which has no body", pointed at std's `flatten`. B300's worklist grounds a
// bound's binders from the receiver — and here the receiver is
// `SignalCell<I>`, so `Source`'s argument comes back as the CALLER's own `I`,
// which the worklist declined as "not concrete". `flatten`'s `I` was left out
// of the recorded substitution, and the instance the caller's instantiation
// emitted had nothing to resolve `self.get().get()` through. Bound to the
// caller's parameter it composes (B244): the caller's instance grounds `I`,
// and the caller's declared `I: Source<U>` answers `flatten`'s `U` in turn.
// The item suspected the chained receiver against the `Option` twin's
// pattern-bound one; the twin works because it binds nothing through the
// abstract `I` at all — it is the control below.

/// The minimal form, no std: a nested-bound blanket's member called at an
/// abstract `I` from a generic function.
#[test]
fn b371_a_nested_bound_blanket_called_at_an_abstract_parameter_dispatches() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Read<T> { fun get(self): T; }
        struct Cell<T> { value: T }
        impl Cell<type T> with Read<T> {
            fun get(self): T { self.value }
        }

        impl type S: Read<type I: Read<type U>> {
            fun join(self): U { self.get().get() }
        }

        fun join_at<U, I: Read<U>>(outer: Cell<I>): U {
            outer.join()
        }

        fun main() {
            print(join_at(Cell { value = Cell { value = 7 } }));
            print(join_at(Cell { value = Cell { value = "x" } }));
        }
        "#,
        "7\nx\n",
    );
}

/// The second half: the member's `U` is the CALLER's `U`, read off the
/// caller's declared `I: Read<U>` — so the result carries the caller's bound.
/// With `I` bound but that step missing, `joined` typed as `join`'s own
/// unbounded `U` and the `==` was refused.
#[test]
fn b371_the_members_result_carries_the_callers_bound() {
    assert_compiles_and_runs(
        r#"
        import std::compare::PartialEq;
        import std::io::print;

        trait Read<T> { fun get(self): T; }
        struct Cell<T> { value: T }
        impl Cell<type T> with Read<T> {
            fun get(self): T { self.value }
        }

        impl type S: Read<type I: Read<type U>> {
            fun join(self): U { self.get().get() }
        }

        fun same_at<U: PartialEq, I: Read<U>>(outer: Cell<I>, other: U): bool {
            let joined = outer.join();
            joined == other
        }

        fun main() {
            print(same_at(Cell { value = Cell { value = 7 } }, 7));
            print(same_at(Cell { value = Cell { value = "a" } }, "b"));
        }
        "#,
        "true\nfalse\n",
    );
}

/// The item's repro: `switch` as a blanket method over std's `flatten`,
/// following the inner cell across a set. Since A124 S2c `map(f).flatten()`
/// answers a cold node, so the four B371 bodies end in `.cell()` — the
/// `SignalCell<U>` their signatures name; the generic `map` reaching the body
/// at all is B408's.
#[test]
fn b371_switch_over_std_flatten_runs_as_a_blanket_method() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source, run_with_owner, Owner };

        impl type S: Source<type T> {
            fun switch_to<U, I: Source<U>>(self, f: sync |T| I): SignalCell<U> {
                self.map(f).flatten().cell()
            }
        }

        fun main() {
            let n = Signal::new(1);
            run_with_owner(Owner::new(), || {
                let doubled: SignalCell<i32> = n.switch_to(|m| Signal::new(m * 2));
                print(doubled.get());
                n.set(5);
                print(doubled.get());
            });
        }
        "#,
        "2\n10\n",
    );
}

/// The free-function spelling the item also named.
#[test]
fn b371_switch_over_std_flatten_runs_as_a_free_function() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source, run_with_owner, Owner };

        fun switch_to<T, U, S: Source<T>, I: Source<U>>(source: S, f: sync |T| I): SignalCell<U> {
            source.map(f).flatten().cell()
        }

        fun main() {
            let n = Signal::new(1);
            run_with_owner(Owner::new(), || {
                let doubled: SignalCell<i32> = switch_to(n, |m| Signal::new(m * 2));
                print(doubled.get());
                n.set(5);
                print(doubled.get());
            });
        }
        "#,
        "2\n10\n",
    );
}

/// The annotation the item's repro carried to keep B300(a)'s inference gap
/// out of the picture is no longer needed: `flatten`'s `U` binds to the
/// caller's `U` through the caller's own `I: Source<U>`.
#[test]
fn b371_the_result_infers_without_an_annotation() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source, run_with_owner, Owner };

        impl type S: Source<type T> {
            fun switch_to<U, I: Source<U>>(self, f: sync |T| I): SignalCell<U> {
                self.map(f).flatten().cell()
            }
        }

        fun main() {
            let n = Signal::new(1);
            run_with_owner(Owner::new(), || {
                let doubled = n.switch_to(|m| Signal::new(m * 2));
                n.set(4);
                print(doubled.get() + 1);
            });
        }
        "#,
        "9\n",
    );
}

/// The control: the OPTION twin, whose inner dispatch is on a pattern-bound
/// local, compiled and ran before and must still.
#[test]
fn b371_the_optional_twin_is_unchanged() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, None, Some };
        import std::reactive::{ Signal, SignalCell, Source, run_with_owner, Owner };

        impl type S: Source<type T> {
            fun and_then_to<U, I: Source<U>>(self, f: sync |T| Option<I>): SignalCell<Option<U>> {
                self.map(f).flatten().cell()
            }
        }

        fun main() {
            let n = Signal::new(1);
            run_with_owner(Owner::new(), || {
                let doubled: SignalCell<Option<i32>> = n.and_then_to(|m| if m > 2 {
                    Some(Signal::new(m * 2))
                } else {
                    None
                });
                print(doubled.get().unwrap_or(-1));
                n.set(5);
                print(doubled.get().unwrap_or(-1));
            });
        }
        "#,
        "-1\n10\n",
    );
}

#[test]
fn a86_two_blankets_bounded_at_different_arguments_may_share_a_member_name() {
    // The duplicate-member rule compared inherent subjects with `compare_type`,
    // which consults a bound's TRAIT and not its ARGUMENTS, so any two blankets
    // over one parameterized trait read as one name declared twice. They are
    // two impls: no `Option` is a `Read`, so nothing satisfies both bounds —
    // and this is the pair A86's `flatten` needs (the join, and the join over
    // an optional inner). The rule now reads the binders' bounds, which is what
    // the coherence rule one tier down (`same_impl_type_shape`) already says.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        trait Read<T> { fun get(self): T; }
        struct Cell<T> { value: T }
        impl Cell<type T> with Read<T> {
            fun get(self): T { self.value }
        }

        impl type S: Read<type I: Read<type U>> {
            fun join(self): U { self.get().get() }
        }
        impl type S: Read<Option<type I: Read<type U>>> {
            fun join(self): Option<U> {
                match self.get() {
                    Some(let inner) => Some(inner.get()),
                    None => None,
                }
            }
        }

        fun main() {
            let nested = Cell { value = Cell { value = 7 } };
            print(nested.join() + 1);
            let optional: Cell<Option<Cell<i32>>> = Cell { value = Some(Cell { value = 3 }) };
            print(optional.join().unwrap_or(0));
            let empty: Cell<Option<Cell<i32>>> = Cell { value = None };
            print(empty.join().unwrap_or(0));
        }

        main();
        "#,
        "8\n3\n0\n",
    );
}

#[test]
fn a86_two_blankets_bounded_the_same_way_still_collide() {
    // The control: identical bounds are the repeat the rule exists to refuse,
    // because tier 1 of method resolution takes the first inherent candidate
    // without ranking — a silent pick between two bodies.
    assert_fails_with(
        r#"
        trait Read<T> { fun get(self): T; }
        struct Cell<T> { value: T }
        impl Cell<type T> with Read<T> {
            fun get(self): T { self.value }
        }

        impl type S: Read<type T> {
            fun peek(self): T { self.get() }
        }
        impl type O: Read<type V> {
            fun peek(self): V { self.get() }
        }

        fun main() { }
        "#,
        "'peek' is already defined for",
    );
}

#[test]
fn a86_a_blanket_and_a_constructor_headed_impl_still_collide() {
    // The other control: the bounds clause reaches BARE binders only. A
    // blanket against a concrete subject is the overlap B73 named, and it is
    // still refused.
    assert_fails_with(
        r#"
        trait Read<T> { fun get(self): T; }
        struct Cell<T> { value: T }
        impl Cell<type T> with Read<T> {
            fun get(self): T { self.value }
        }

        impl type S: Read<type T> {
            fun peek(self): T { self.get() }
        }
        impl Cell<type T> {
            fun peek(self): T { self.value }
        }

        fun main() { }
        "#,
        "'peek' is already defined for",
    );
}

/// The `[rpc]` face of the exhibit: one service whose only interesting feature
/// is the return type, so a pin swaps exactly one spelling to move between the
/// faces. `Opaque` is the not-Wire type and `Phantom<T>` the unbounded-binder
/// one (`Handle<T>`'s shape, written by hand).
fn wire_impl_service() -> String {
    format!(
        r#"
        import std::io::print;
        import std::wire::{{ Deserialize, Serialize, Wire }};
        struct Opaque {{ body: || void }}
        struct Phantom<T> {{ count: i53 }}
        impl Phantom<type T> with Wire {{
            fun describe<S: Serialize>(self, serializer: &mut S) {{
                self.count.describe(serializer);
            }}

            fun rebuild<D: Deserialize>(deserializer: &mut D): Phantom<T> {{
                Phantom {{ count = i53::rebuild(deserializer) }}
            }}
        }}
        {OUTCOME_WIRE_IMPL}
        [service(StoreClient)]
        struct Store {{ name: str }}
        impl Store {{
            [rpc]
            fun look(self, id: i53): Outcome<i53, str> {{ Outcome::Bad("missing") }}
        }}
        fun main() {{ print("store"); }}
        main();
        "#
    )
}

/// The exhibit, and the whole of B289 in one program. `std::wire` shipped no
/// `Result` impl (A82 lands it); kolt wrote one by hand and still could not
/// return a `Result<i53, str>` from an `[rpc]` method, because the Wire
/// predicate was a syntactic allowlist of six scalar spellings,
/// `List`/`Option`/`Map`, and the `[derive(Wire)]` NAMES — an `impl .. with
/// Wire` was invisible to it.
///
/// The impl is now the answer: the predicate consults the trait table, so a
/// type that implements `Wire` is Wire wherever the boundary is asked.
#[test]
fn a_hand_written_wire_impl_admits_its_type_in_an_rpc_signature() {
    assert_compiles(&wire_impl_service());
}

/// The refuse face, and the thing a NAME-keyed scan could never do: `Outcome`
/// has an impl, so a scan over names would admit every `Outcome` in the
/// program. Recursion into the arguments happens exactly where the IMPL's own
/// binder bounds demand it — `impl Outcome<type T: Wire, type E: Wire>` binds
/// both — so the argument that is not Wire refuses the signature, and the
/// refusal quotes the type that carries it.
#[test]
fn a_hand_written_wire_impls_bounds_still_refuse_an_argument_that_is_not_wire() {
    assert_fails_with(
        &wire_impl_service().replace("Outcome<i53, str>", "Outcome<Opaque, str>"),
        "is `Outcome<Opaque, str>`, which is not Wire",
    );
}

/// C7's phantom rule, expressed by the impl rather than by a special case in
/// the predicate: a binder with NO bound reaches its argument with no
/// requirement, so `Phantom<Opaque>` is as sendable as `Phantom<i53>` — the
/// argument never reaches the payload. The same shape the derive gives
/// `Handle<T>` (`impl Handle<type T> with Wire`), written by hand so the rule
/// is pinned at the impl and not at the derive.
#[test]
fn a_wire_impl_whose_binder_carries_no_bound_admits_any_argument() {
    assert_compiles(
        &wire_impl_service()
            .replace("Outcome<i53, str>", "Phantom<Opaque>")
            .replace(r#"Outcome::Bad("missing")"#, "Phantom { count = 1i53 }"),
    );
}

/// The second boundary: the `[derive(Wire)]` all-fields check reads the same
/// predicate, so a field typed by a hand-implemented Wire type passes it. It
/// was refused before B289 for the same reason the signature was — the field's
/// SPELLING was the test.
///
/// B289 left it compiling no further than the boundary, because of a coupling
/// the allowlist had been hiding: `[derive(Wire)]` also emitted `Json`/`FromJson`
/// ("additive beside the JSON impls until the codec re-plumb consumes it"), so
/// a field type needed a `Json` impl as well as a `Wire` one and the generated
/// body said so in `to_json`'s vocabulary rather than the boundary's. B301
/// finished the re-plumb: this ADMITS now, which is the face B289 meant it to
/// have.
#[test]
fn a_derive_wire_field_may_be_a_hand_implemented_wire_type() {
    assert_compiles(&format!(
        r#"
        import std::io::print;
        import std::wire::{{ Deserialize, Serialize, Wire }};
        {OUTCOME_WIRE_IMPL}
        [derive(Wire)]
        struct Row {{ id: i53, outcome: Outcome<i53, str> }}
        fun main() {{ print("row"); }}
        main();
        "#
    ));
}

/// The sibling that showed the residue was the DERIVE's and not the
/// hand-written impl's: `Map` is Wire by std's own `impl Map<type K: Hashable +
/// Wire, type V: Wire> with Wire` (A39) and the Wire boundary has admitted it
/// by name since, while `[derive(Wire)]`'s JSON half never could generate for
/// it. With the codec re-plumb done (B301) the derive asks only for Wire, and
/// the `Map` field is as ordinary as the `i53` beside it.
#[test]
fn a_derive_wire_field_may_be_a_map() {
    assert_compiles(
        r#"
        import std::io::print;
        import std::map::Map;
        [derive(Wire)]
        struct Row { id: i53, tags: Map<str, i32> }
        fun main() { print("row"); }
        main();
        "#,
    );
}

/// B301's third shape, and kolt's own (`store.vl:184`): `Result` has been Wire
/// since A82 and is not Json, so a derived struct could not hold one either.
#[test]
fn a_derive_wire_field_may_be_a_result() {
    assert_compiles(
        r#"
        import std::io::print;
        import std::result::Result;
        [derive(Wire)]
        struct Outcome { id: i53, value: Result<i32, str> }
        fun main() { print("outcome"); }
        main();
        "#,
    );
}

/// The control that keeps the split honest in the other direction: a type that
/// wants BOTH codecs asks for both, and gets both. `[derive(Json, Wire)]` is
/// one declaration, two derives, two passes of the generator — the JSON pair
/// and the §6.1 visitor, each over the same fields.
#[test]
fn a_derive_json_wire_type_carries_both_codecs() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::json::{ Json, FromJson };
        import std::result::Result::{ self, Ok, Err };
        import std::wire::Wire;
        [derive(Json, Wire)]
        struct Point { x: i32, y: i32 }
        fun main() {
            let point = Point { x = 1, y = 2 };
            print(point.to_json());
            match Point::from_json(point.to_json()) {
                Ok(let back) => print(back.x + back.y),
                Err(let reason) => print(reason),
            }
        }
        main();
        "#,
        "{\"x\":1,\"y\":2}\n3\n",
    );
}

/// And the face the split creates: `[derive(Wire)]` ALONE does not hand its
/// subject a JSON codec. `to_json` is not a method on a Wire type any more,
/// and the refusal says so in the ordinary missing-method vocabulary rather
/// than from inside generated code.
#[test]
fn a_derive_wire_type_has_no_json_codec_of_its_own() {
    assert_fails_with(
        r#"
        import std::io::print;
        import std::wire::Wire;
        [derive(Wire)]
        struct Point { x: i32, y: i32 }
        fun main() { print(Point { x = 1, y = 2 }.to_json()); }
        main();
        "#,
        "to_json",
    );
}

/// And its refuse face at the same boundary, for the same reason as the
/// signature's: the impl's binder bounds are what recurse.
#[test]
fn a_derive_wire_field_of_a_hand_implemented_type_still_checks_its_arguments() {
    assert_fails_with(
        &format!(
            r#"
        import std::io::print;
        import std::wire::{{ Deserialize, Serialize, Wire }};
        struct Opaque {{ body: || void }}
        {OUTCOME_WIRE_IMPL}
        [derive(Wire)]
        struct Row {{ id: i53, outcome: Outcome<Opaque, str> }}
        fun main() {{ print("row"); }}
        main();
        "#
        ),
        "is `Outcome<Opaque, str>`, which is not Wire",
    );
}

/// The third boundary: an `[expose]`d source's ELEMENT. The element comes off
/// the `Source` impl and has no written node of its own, which is why this
/// site always read a resolved type — B289 is what makes the rule it reads the
/// same rule as the other three.
#[test]
fn an_exposed_element_may_be_a_hand_implemented_wire_type() {
    assert_compiles(&format!(
        r#"
        import std::io::print;
        import std::reactive::{{ Signal, SignalCell }};
        import std::wire::{{ Deserialize, Serialize, Wire }};
        {OUTCOME_WIRE_IMPL}
        [service(StoreClient)]
        struct Store {{
            [expose] outcome: SignalCell<Outcome<i53, str>>,
        }}
        impl Store {{
            [rpc]
            fun ping(self): i53 {{ 1i53 }}
        }}
        fun main() {{ print("store"); }}
        main();
        "#
    ));
}

/// The fourth boundary: a handle RETURN's element (§9.2, row 411). The
/// spelling still decides that the return IS a handle — the `[service]`
/// expansion reads the same annotation to shape the stub, and it runs before
/// any type resolves — but the element it is JUDGED at is the resolved one, so
/// a hand-implemented Wire element crosses.
#[test]
fn a_handle_returns_element_may_be_a_hand_implemented_wire_type() {
    assert_compiles(&format!(
        r#"
        import std::io::print;
        import std::reactive::{{ Signal, SignalCell }};
        import std::wire::{{ Deserialize, Serialize, Wire }};
        {OUTCOME_WIRE_IMPL}
        [service(StoreClient)]
        struct Store {{
            outcome: SignalCell<Outcome<i53, str>>,
        }}
        impl Store {{
            [rpc]
            fun watch(self): SignalCell<Outcome<i53, str>> {{ self.outcome }}
        }}
        fun main() {{ print("store"); }}
        main();
        "#
    ));
}

/// A39 admitted `Map` two orders ago and no refusal ever said so: all four
/// texts listed `List`/`Option` and stopped. They now name `Map` and the impl
/// escape hatch — the sentence that would have saved kolt the forty lines it
/// wrote instead — and this holds all four at once, which is also the pin that
/// they ARE four texts saying one thing.
#[test]
fn the_wire_refusal_names_map_and_the_impl_among_the_shapes_it_admits() {
    let admitted = "`List`/`Option`/`Map` of Wire";
    let escape = "or a type with an `impl .. with Wire`";
    for (source, head) in [
        (
            r#"
        import std::io::print;
        struct Opaque { body: || void }
        [service(StoreClient)]
        struct Store { name: str }
        impl Store {
            [rpc]
            fun look(self): Opaque { Opaque { body = || {} } }
        }
        fun main() { print("store"); }
        main();
        "#,
            "of `[rpc]` method `look` is `Opaque`, which is not Wire",
        ),
        (
            r#"
        import std::io::print;
        struct Opaque { body: || void }
        [derive(Wire)]
        struct Row { hidden: Opaque }
        fun main() { print("row"); }
        main();
        "#,
            "of `[derive(Wire)]` type `Row` is `Opaque`, which is not Wire",
        ),
        (
            r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };
        struct Opaque { body: || void }
        [service(StoreClient)]
        struct Store { [expose] hidden: SignalCell<Opaque> }
        impl Store {
            [rpc]
            fun ping(self): i53 { 1i53 }
        }
        fun main() { print("store"); }
        main();
        "#,
            "its element `Opaque` is not Wire",
        ),
        (
            r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };
        struct Opaque { body: || void }
        [service(StoreClient)]
        struct Store { hidden: SignalCell<Opaque> }
        impl Store {
            [rpc]
            fun watch(self): SignalCell<Opaque> { self.hidden }
        }
        fun main() { print("store"); }
        main();
        "#,
            "returns a signal handle whose element `Opaque` is not Wire",
        ),
    ] {
        assert_fails_with(source, head);
        assert_fails_with(source, admitted);
        assert_fails_with(source, escape);
    }
}

// --- B299: a BARE TRAIT in impl-subject position means "every implementer" ---
//
// `impl Source<type I> { fun flatten(self) .. }` parsed and then refused every
// use of `self`: the body read the subject literally, so `self` was a value of
// bare trait type — which vilan has none of — and the spelling meant nothing a
// body could use while looking exactly like the one that does. RULED a DESUGAR
// (2026-09-11): it means `impl type S: Source<type I> { .. }`, the universal
// reading B186's parameters and B184's fields already give a bare trait, and
// the reading SELECTION has always given it (`impl Iterator<type T> with
// Iterable<T>` is how std writes "every iterator also iterates").

#[test]
fn b299_a_bare_trait_impl_subject_makes_self_the_implementing_type() {
    // The filed exhibit, run: `self.get()` inside the body dispatches to the
    // receiver's own implementation, and the head's `T` binds from it, so the
    // result is an `i32` at the call site with nothing annotated.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Read<T> { fun get(self): T; }
        struct Cell<T> { value: T }
        impl Cell<type T> with Read<T> {
            fun get(self): T { self.value }
        }

        impl Read<type T> {
            fun sample(self): T { self.get() }
        }

        fun main() {
            let cell = Cell { value = 7 };
            print(cell.sample() + 1);
        }

        main();
        "#,
        "8\n",
    );
}

#[test]
fn b299_the_bare_trait_head_dispatches_per_implementation() {
    // The claim the compile alone cannot make: ONE body, and each receiver's
    // own `get` is what runs in it.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Read<T> { fun get(self): T; }
        struct Cell<T> { value: T }
        impl Cell<type T> with Read<T> {
            fun get(self): T { self.value }
        }
        struct Doubled { value: i32 }
        impl Doubled with Read<i32> {
            fun get(self): i32 { self.value * 2 }
        }

        impl Read<type T> {
            fun sample(self): T { self.get() }
        }

        fun main() {
            print(Cell { value = 7 }.sample());
            print(Doubled { value = 7 }.sample());
        }

        main();
        "#,
        "7\n14\n",
    );
}

#[test]
fn b299_the_owners_probe_head_compiles_and_runs() {
    // The head B299 was filed from, over std's own `Source`: a source of an
    // OPTIONAL source, with the inner binder anonymous (B294's `_`). `self.get()`
    // works in it, and so does `inner.get()` on what it yields.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        import std::reactive::{ Signal, SignalCell, Source };

        impl Source<Option<type _: Source<type U>>> {
            fun peek(self): Option<U> {
                match self.get() {
                    Some(let inner) => Some(inner.get()),
                    None => None,
                }
            }
        }

        fun main() {
            let inner = Signal::new(7);
            let outer: SignalCell<Option<SignalCell<i32>>> = Signal::new(Some(inner));
            print(outer.peek().unwrap_or(0));
            let empty: SignalCell<Option<SignalCell<i32>>> = Signal::new(None);
            print(empty.peek().unwrap_or(0));
        }

        main();
        "#,
        "7\n0\n",
    );
}

#[test]
fn b299_a_body_naming_the_implicit_binder_is_told_the_spelling() {
    // The implicit binder has NO name, so a body reaching for one finds
    // nothing — and the fix is a spelling, not a missing declaration. One
    // steer; the impl itself is legitimate.
    assert_fails_with(
        r#"
        trait Read<T> { fun get(self): T; }
        struct Cell<T> { value: T }
        impl Cell<type T> with Read<T> {
            fun get(self): T { self.value }
        }

        impl Read<type T> {
            fun sample(self): T {
                let me: S = self;
                me.get()
            }
        }

        fun main() { }
        "#,
        "name it in the head: `impl type S: Read<..>`",
    );
}

#[test]
fn b299_self_names_the_implementing_type_in_a_bare_trait_impl() {
    // The spelling the steer offers, working: `Self` is the implementing type
    // inside such a body — which is what std's `impl Iterator<type T> with
    // Iterable<T> { fun iter(self): Self { self } }` has always relied on.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Read<T> { fun get(self): T; }
        struct Cell<T> { value: T }
        impl Cell<type T> with Read<T> {
            fun get(self): T { self.value }
        }

        impl Read<type T> {
            fun twice(self): T {
                let me: Self = self;
                me.get()
            }
        }

        fun main() { print(Cell { value = 7 }.twice()); }

        main();
        "#,
        "7\n",
    );
}

#[test]
fn b299_a_trait_hung_static_is_still_reached_by_the_traits_name() {
    // The std control the desugar must not cost: `impl Iterator<type T> {
    // fun from_fn(..) }` hangs a STATIC off the trait, reached as
    // `Iterator::from_fn`. The impl still registers under the trait subject,
    // so the name still finds it.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::iterator::Iterator;
        import std::option::Option::{ self, Some, None };

        fun main() {
            mut remaining = 3;
            let counted = Iterator::from_fn(|| {
                if remaining > 0 {
                    remaining = remaining - 1;
                    Some(remaining)
                } else {
                    None
                }
            });
            for value in counted {
                print(value);
            }
        }

        main();
        "#,
        "2\n1\n0\n",
    );
}

// --- B297: an impl SUBJECT is walked once, so one mistake is one error -------
//
// `Node::Impl` walked the subject twice — once for its type, and again
// per-argument to bank the arguments `self`'s variant patterns substitute
// through — and each walk prepped its own deferred reference, so an unresolved
// name in the head was reported TWICE for one mistake. The arguments are read
// off the resolved subject now; there is nothing to walk a second time.

#[test]
fn b297_an_unresolved_name_in_an_impl_subject_is_reported_once() {
    assert_fails_once_with(
        r#"
        trait Read<T> { fun get(self): T; }
        struct Cell<T> { value: T }
        impl Cell<type T> with Read<T> {
            fun get(self): T { self.value }
        }

        impl Read<Option<Nope>> {
            fun sample(self) { }
        }

        fun main() { }
        "#,
        "cannot find type 'Nope'",
    );
}

#[test]
fn b297_the_let_annotation_is_the_control() {
    // The same name in the position that always reported once.
    assert_fails_once_with(
        r#"
        import std::option::Option::{ self, Some, None };
        fun main() {
            let x: Option<Nope> = None;
        }
        "#,
        "cannot find type 'Nope'",
    );
}

#[test]
fn b297_a_concrete_impl_subject_reports_its_unresolved_name_once_too() {
    // Not a trait-subject rule: the second walk was the ARGUMENT bank, which a
    // concrete generic application reaches identically.
    assert_fails_once_with(
        r#"
        struct Holder<T> { value: T }

        impl Holder<Nope> {
            fun sample(self) { }
        }

        fun main() { }
        "#,
        "cannot find type 'Nope'",
    );
}

#[test]
fn b297_the_subject_arguments_still_type_selfs_variant_patterns() {
    // The behaviour the second walk existed for, unchanged: inside
    // `impl Option<(T, U)>`, `Some`'s payload is the TUPLE, not `Option`'s own
    // abstract `T`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        impl Option<(type T, type U)> {
            fun left(self, fallback: T): T {
                match self {
                    Some((let first, _)) => first,
                    None => fallback,
                }
            }
        }

        fun main() {
            let pair: Option<(i32, str)> = Some((7, "x"));
            print(pair.left(0));
            let empty: Option<(i32, str)> = None;
            print(empty.left(1));
        }

        main();
        "#,
        "7\n1\n",
    );
}

// --- B315: two blankets whose bounds OVERLAP ------------------------------------
// A86 taught the duplicate-inherent-member rule to read a bound's ARGUMENTS, so
// two blankets over one parameterized trait stopped colliding wholesale. What it
// asked for was SAMENESS, and sameness is narrower than the thing the rule
// exists to prevent: a pair whose clauses differ but overlap — some type
// satisfies both — was admitted, and tier 1 of method resolution takes the first
// inherent candidate UNRANKED, so the winner was declaration order. The rule
// refuses overlap now, and the clause that keeps A86's own pair legal is the one
// that says a BOUNDED binder admits only types carrying the traits it demands.

#[test]
fn b315_two_blankets_whose_bounds_overlap_are_refused() {
    // `S: Read<type I>` accepts every `S` that reads anything at all, so an `S`
    // reading an `Option` satisfies both clauses and both impls claim `peek`.
    assert_fails_with(
        r#"
        import std::option::Option;

        trait Read<T> { fun get(self): T; }
        struct Cell<T> { value: T }
        impl Cell<type T> with Read<T> {
            fun get(self): T { self.value }
        }

        impl type S: Read<type I> {
            fun peek(self): I { self.get() }
        }
        impl type O: Read<Option<type V>> {
            fun peek(self): Option<V> { self.get() }
        }

        fun main() { }
        "#,
        "'peek' is declared by two blanket impls whose bounds OVERLAP",
    );
}

#[test]
fn b315_the_overlap_refusal_names_the_two_fixes_and_points_at_the_first_impl() {
    // C3: the message says what to do, and the note says where the other one is.
    assert_fails_noting(
        r#"
        import std::option::Option;

        trait Read<T> { fun get(self): T; }
        struct Cell<T> { value: T }
        impl Cell<type T> with Read<T> {
            fun get(self): T { self.value }
        }

        impl type S: Read<type I> {
            fun peek(self): I { self.get() }
        }
        impl type O: Read<Option<type V>> {
            fun peek(self): Option<V> { self.get() }
        }

        fun main() { }
        "#,
        "Narrow one bound so the two are disjoint, or declare 'peek' on a trait",
        "peek",
        "'peek' is already defined here",
    );
}

#[test]
fn b315_two_blankets_bounded_at_disjoint_concrete_arguments_may_share_a_name() {
    // The non-overlapping control: two WRITTEN types that are not the same type
    // name disjoint sets, so nothing satisfies both clauses.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Read<T> { fun get(self): T; }
        struct Cell<T> { value: T }
        impl Cell<type T> with Read<T> {
            fun get(self): T { self.value }
        }

        impl type S: Read<i32> {
            fun peek(self): i32 { self.get() + 1 }
        }
        impl type O: Read<str> {
            fun peek(self): str { self.get() }
        }

        fun main() {
            let numbers = Cell { value = 41 };
            print(numbers.peek());
            let words = Cell { value = "hi" };
            print(words.peek());
        }

        main();
        "#,
        "42\nhi\n",
    );
}

#[test]
fn b315_a_bounded_binder_does_not_admit_a_type_that_misses_its_bound() {
    // A86's pair, stated as the overlap question it now is: the first clause's
    // `type I` demands `Read`, no `Option` is a `Read`, so an `S` cannot satisfy
    // both — and the two impls stay two impls. (The behaviour is A86's own pin
    // `a86_two_blankets_bounded_at_different_arguments_may_share_a_member_name`;
    // this one holds the CLAUSE that keeps it, by putting a bound on the binder
    // that the overlapping pair above leaves off.)
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        trait Read<T> { fun get(self): T; }
        struct Cell<T> { value: T }
        impl Cell<type T> with Read<T> {
            fun get(self): T { self.value }
        }

        impl type S: Read<type I: Read<type U>> {
            fun peek(self): U { self.get().get() }
        }
        impl type O: Read<Option<type V: Read<type W>>> {
            fun peek(self): Option<W> {
                match self.get() {
                    Some(let inner) => Some(inner.get()),
                    None => None,
                }
            }
        }

        fun main() {
            let nested = Cell { value = Cell { value = 7 } };
            print(nested.peek());
            let optional: Cell<Option<Cell<i32>>> = Cell { value = Some(Cell { value = 3 }) };
            print(optional.peek().unwrap_or(0));
        }

        main();
        "#,
        "7\n3\n",
    );
}

#[test]
fn b315_a_binder_bounded_by_a_trait_the_written_type_does_implement_overlaps() {
    // The other side of the same clause: make the written type carry the trait
    // the binder demands and the two clauses DO overlap, so the pair is refused.
    // This is the item's own framing — the hypothetical `Option: Source`.
    assert_fails_with(
        r#"
        import std::option::Option::{ self, Some, None };

        trait Read<T> { fun get(self): T; }
        struct Cell<T> { value: T }
        impl Cell<type T> with Read<T> {
            fun get(self): T { self.value }
        }
        impl Option<type T> with Read<T> {
            fun get(self): T {
                match self {
                    Some(let value) => value,
                    None => panic("empty"),
                }
            }
        }

        impl type S: Read<type I: Read<type U>> {
            fun peek(self): U { self.get().get() }
        }
        impl type O: Read<Option<type V: Read<type W>>> {
            fun peek(self): Option<W> { None }
        }

        fun main() { }
        "#,
        "'peek' is declared by two blanket impls whose bounds OVERLAP",
    );
}

// --- B330: two blankets bounded DIFFERENTLY (Order 35 ruling R5) ---------------
// B315 refuses two blankets whose bound clauses OVERLAP, and it answers that
// question from the declarations. When both argument binders are BOUNDED, and
// bounded by different traits, the declarations do not contain the answer:
// whether some third type carries both is a fact about every type in this
// program and in every program that will import it. The pair is admitted —
// deliberately, and these two pins are what makes it a recorded behaviour
// rather than a gap nobody measured. The refusal belongs at the CALL, where the
// receiver is known; B318's S4 (the per-importer namespace) is where it is
// queued.

#[test]
fn b330_two_blankets_bounded_differently_may_share_a_name() {
    // Neither clause is a subset of the other and neither names a written type,
    // so `bound_argument_positions_overlap` has nothing to compare and the
    // duplicate family stands down.
    assert_compiles(
        r#"
        import std::debug::Debug;

        trait Read<T> { fun get(self): T; }
        trait Tagged { fun tag(self): str; }

        struct Cell<T> { value: T }
        impl Cell<type T> with Read<T> {
            fun get(self): T { self.value }
        }

        impl type S: Read<type I: Debug> {
            fun peek(self): str { "debug side" }
        }
        impl type O: Read<type J: Tagged> {
            fun peek(self): str { "tagged side" }
        }

        fun main() { }
        "#,
    );
}

/// B318 S4 — the admission's cost is now REFUSED, at the call, where the
/// witness is.
///
/// A receiver satisfying BOTH bounds had two inherent candidates and tier 1
/// took the first without ranking, so DECLARATION ORDER picked the body and the
/// same program with the two `impl` blocks swapped printed the other answer.
/// The declarations cannot see that — whether a third type carries both
/// `Debug` and `Tagged` is not a question they answer — so the pair stays
/// admitted (the pin above) and the SITE, which knows its receiver, refuses.
/// Both orders refuse identically, which is the whole point: the answer is no
/// longer a function of which block was written first.
#[test]
fn b330_a_receiver_satisfying_both_bounds_is_refused_at_the_call() {
    let program = r#"
        import std::io::print;
        import std::debug::Debug;

        trait Read<T> { fun get(self): T; }
        trait Tagged { fun tag(self): str; }

        struct Cell<T> { value: T }
        impl Cell<type T> with Read<T> {
            fun get(self): T { self.value }
        }

        struct Both { n: i32 }
        impl Both with Debug {
            fun debug(self): str { i"Both({self.n})" }
        }
        impl Both with Tagged {
            fun tag(self): str { "both" }
        }

        IMPLS

        fun main() {
            let cell: Cell<Both> = Cell { value = Both { n = 1 } };
            print(cell.peek());
        }
        main();
        "#;
    let debug_first = r#"
        impl type S: Read<type I: Debug> {
            fun peek(self): str { "debug side" }
        }
        impl type O: Read<type J: Tagged> {
            fun peek(self): str { "tagged side" }
        }
        "#;
    let tagged_first = r#"
        impl type O: Read<type J: Tagged> {
            fun peek(self): str { "tagged side" }
        }
        impl type S: Read<type I: Debug> {
            fun peek(self): str { "debug side" }
        }
        "#;
    for implementations in [debug_first, tagged_first] {
        assert_fails_with(
            &program.replace("IMPLS", implementations),
            "this receiver satisfies the bounds of TWO blanket `impl` blocks that both declare \
             'peek'",
        );
    }
    // The fix the message names, taken: narrowing one bound so the two are
    // disjoint leaves one candidate, and the program runs again.
    let narrowed = r#"
        impl type S: Read<type I: Debug> {
            fun peek(self): str { "debug side" }
        }
        "#;
    assert_compiles_and_runs(&program.replace("IMPLS", narrowed), "debug side\n");
}

#[test]
fn b315_two_blankets_bounded_the_same_way_keep_the_already_defined_message() {
    // The families stay apart: an identical pair is still "already defined",
    // because what is wrong there is that one thing was written twice.
    assert_fails_with(
        r#"
        trait Read<T> { fun get(self): T; }
        struct Cell<T> { value: T }
        impl Cell<type T> with Read<T> {
            fun get(self): T { self.value }
        }

        impl type S: Read<type T> {
            fun peek(self): T { self.get() }
        }
        impl type O: Read<type V> {
            fun peek(self): V { self.get() }
        }

        fun main() { }
        "#,
        "'peek' is already defined for",
    );
}

// --- B302: a written PATH renders as written ------------------------------------
// `render_type` turns a written type annotation back into source — for the code
// a `[derive(..)]` generates, and for every message that quotes a type AS THE
// AUTHOR WROTE IT. It knew a name (`i32`) and a generic application
// (`List<i32>`) and fell back to `_` for everything else, and a `::` path is
// everything else: `[rpc] fun note(self, id: i32): models::Note` was refused as
// "return type … is `_`, which is not Wire", naming nothing the author had
// typed. The arm is one fix for every caller, because every caller is quoting
// the same spelling back.

#[test]
fn b302_the_rpc_return_refusal_names_the_path_as_written() {
    assert_fails_with(
        r#"
        import std::io::print;
        mod models {
            struct Note { body: || void }
        }
        [service(StoreClient)]
        struct Store { name: str }
        impl Store {
            [rpc]
            fun note(self, id: i32): models::Note { models::Note { body = || {} } }
        }
        fun main() { print("store"); }
        main();
        "#,
        "return type of `[rpc]` method `note` is `models::Note`, which is not Wire",
    );
}

#[test]
fn b302_the_rpc_parameter_refusal_names_the_path_as_written() {
    assert_fails_with(
        r#"
        import std::io::print;
        mod models {
            struct Note { body: || void }
        }
        [service(StoreClient)]
        struct Store { name: str }
        impl Store {
            [rpc]
            fun keep(self, note: models::Note): i53 { 1i53 }
        }
        fun main() { print("store"); }
        main();
        "#,
        "of `[rpc]` method `keep` is `models::Note`, which is not Wire",
    );
}

#[test]
fn b302_a_path_with_generic_arguments_renders_both_halves() {
    // The last segment is the only one that can carry arguments, and it does.
    assert_fails_with(
        r#"
        import std::io::print;
        mod models {
            struct Holder<T> { body: || void, value: T }
        }
        [service(StoreClient)]
        struct Store { name: str }
        impl Store {
            [rpc]
            fun hold(self): models::Holder<i32> {
                models::Holder { body = || {}, value = 1 }
            }
        }
        fun main() { print("store"); }
        main();
        "#,
        "return type of `[rpc]` method `hold` is `models::Holder<i32>`, which is not Wire",
    );
}

#[test]
fn b302_a_path_of_three_segments_renders_whole() {
    assert_fails_with(
        r#"
        import std::io::print;
        mod models {
            mod deep {
                struct Note { body: || void }
            }
        }
        [service(StoreClient)]
        struct Store { name: str }
        impl Store {
            [rpc]
            fun note(self): models::deep::Note { models::deep::Note { body = || {} } }
        }
        fun main() { print("store"); }
        main();
        "#,
        "return type of `[rpc]` method `note` is `models::deep::Note`, which is not Wire",
    );
}

/// B329: the `[expose]` ELEMENT refusal quotes the annotation as written.
///
/// The element it tests comes off the field's `Source` impl and is a resolved
/// type id, so it rendered through `pretty_print_type` — by BARE NAME. Every
/// sibling refusal in this family renders the written node through
/// `render_type` (B302), so a field annotated `SignalCell<models::Note>` was
/// told its element `Note` is not Wire while the derive boundaries, the `[rpc]`
/// parameter and return, and the `[expose]` FIELD refusal one arm over all said
/// `models::Note`. One annotation, two answers.
#[test]
fn b329_the_expose_element_refusal_names_the_path_as_written() {
    assert_fails_with(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };
        mod models {
            struct Note { body: || void }
        }
        [service(SessionClient)]
        struct Session {
            [expose] note: SignalCell<models::Note>,
        }
        fun main() { print("session"); }
        main();
        "#,
        "is `[expose]`d, but its element `models::Note` is not Wire",
    );
}

/// The control: a bare-name element still renders bare, and the field refusal
/// beside it is unmoved. Nothing about the resolved-type KEY changes — the
/// stand-down that suppresses the generated mirror's own bound failure compares
/// against `pretty_print_type` on both sides, so the report is still ONE.
#[test]
fn b329_a_bare_element_name_still_renders_bare_and_reports_once() {
    assert_fails_once_with(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };
        struct Note { body: || void }
        [service(SessionClient)]
        struct Session {
            [expose] note: SignalCell<Note>,
        }
        fun main() { print("session"); }
        main();
        "#,
        "is `[expose]`d, but its element `Note` is not Wire",
    );
}

#[test]
fn b302_the_derive_wire_field_refusal_names_the_path_as_written() {
    assert_fails_with(
        r#"
        import std::io::print;
        mod models {
            struct Note { body: || void }
        }
        [derive(Wire)]
        struct Row { note: models::Note }
        fun main() { print("row"); }
        main();
        "#,
        "field `note` of `[derive(Wire)]` type `Row` is `models::Note`, which is not Wire",
    );
}

#[test]
fn b302_the_derive_hashable_field_refusal_names_the_path_as_written() {
    assert_fails_with(
        r#"
        import std::io::print;
        mod models {
            struct Note { body: || void }
        }
        [derive(Hashable)]
        struct Row { note: models::Note }
        fun main() { print("row"); }
        main();
        "#,
        "field `note` of `[derive(Hashable)]` type `Row` is `models::Note`, which is not `Hashable`",
    );
}

#[test]
fn b302_the_client_service_notification_refusal_names_the_path_as_written() {
    assert_fails_with(
        r#"
        import std::io::print;
        mod models {
            [derive(Wire)]
            struct Note { title: str }
        }
        [client_service]
        struct Watcher { name: str }
        impl Watcher {
            [rpc]
            fun ping(self): models::Note { models::Note { title = "x" } }
        }
        fun main() { print("w"); }
        main();
        "#,
        "return type of `[rpc]` method `ping` is `models::Note`, but a `[client_service]` method \
         is a NOTIFICATION",
    );
}

#[test]
fn b302_a_bare_name_and_a_generic_application_still_render_as_they_did() {
    // The control: the two forms the renderer already knew are untouched.
    assert_fails_with(
        r#"
        import std::io::print;
        struct Opaque { body: || void }
        [service(StoreClient)]
        struct Store { name: str }
        impl Store {
            [rpc]
            fun look(self): List<Opaque> { [] }
        }
        fun main() { print("store"); }
        main();
        "#,
        "return type of `[rpc]` method `look` is `List<Opaque>`, which is not Wire",
    );
}

// --- B279: the structural guard behind B258's silence, and the `candidates_of`
// consumer sweep -----------------------------------------------------------------
// `candidates_of` is NAME-keyed and program-wide: every override of every trait
// declaring the dispatched name, whatever the receiver. B254 and B258 were both
// that list consumed as though it were receiver-specific, and each was closed by
// narrowing at its own site. What was missing is the INVARIANT — nothing forbade
// a future over-approximation from promoting a node to strict through an edge the
// coverage walk's narrower set never sees, which would hand that node the
// bare-value fence and no caller checked for having a value: B258's silent
// `undefined` again, from the other direction. The `context` pass now asserts it
// and REFUSES (a never-silent `internal:`) rather than emitting. The pins below
// are one per consumer of the name-keyed list, plus the geometry the guard must
// NOT fire on.

/// Consumer 1, the `context` pass reading an edge as a DEMAND. Two unrelated
/// traits declare `label`, so one name-keyed candidate list holds both — and the
/// strict override belongs to a type this program never calls through. The site
/// on the OTHER trait must stay safe (its default reads the context optionally),
/// and the guard must not read the unreachable override as a node it cannot
/// prove covered.
#[test]
fn b279_a_strict_override_in_an_unrelated_trait_neither_promotes_nor_trips_the_guard() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::context::Context;
        import std::option::Option::{ None, Some, self };

        let scope: Context<str> = Context::new();

        trait Tag {
            fun label(self): str {
                match scope.get_safe() {
                    Some(let value) => i"tag under {value}",
                    None => "tag, no scope",
                }
            }
        }

        trait Badge {
            fun label(self): str {
                i"badge under {scope.get()}"
            }
        }

        struct Cell { value: i32 }
        impl Cell with Tag { }

        struct Mirror { value: i32 }
        impl Mirror with Badge { }

        fun describe(cell: Cell): str {
            cell.label()
        }

        fun main() {
            print(describe(Cell { value = 1 }));
            scope.run("here", || print(describe(Cell { value = 2 })));
        }
        main();
        "#,
        "tag, no scope\ntag under here\n",
    );
}

/// The same geometry with the strict body REACHED: the guard must not stand in
/// for the coverage refusal, and the coverage refusal must still fire.
#[test]
fn b279_the_strict_body_of_the_other_trait_still_fences_when_it_is_called() {
    assert_fails_with(
        r#"
        import std::io::print;
        import std::context::Context;
        import std::option::Option::{ None, Some, self };

        let scope: Context<str> = Context::new();

        trait Tag {
            fun label(self): str {
                match scope.get_safe() {
                    Some(let value) => i"tag under {value}",
                    None => "tag, no scope",
                }
            }
        }

        trait Badge {
            fun label(self): str {
                i"badge under {scope.get()}"
            }
        }

        struct Cell { value: i32 }
        impl Cell with Tag { }

        struct Mirror { value: i32 }
        impl Mirror with Badge { }

        fun main() {
            print(Mirror { value = 1 }.label());
        }
        main();
        "#,
        "this code can be reached without an enclosing `run`",
    );
}

/// Consumer 2, the const-only capability check reading an edge as a REFUSAL.
/// `emit` is reached ONLY through a bounded generic's trait dispatch — the shape
/// B143 found escaping — and the refusal fires. This is the direction that must
/// never be narrowed: a candidate dropped here is a compile-time-only capability
/// shipped into a runtime path in silence.
#[test]
fn b279_the_const_only_check_still_refuses_through_a_bounded_generics_dispatch() {
    assert_fails_with(
        r#"
        import std::asset::emit;

        trait Paint { fun paint(self); }

        struct Wall { }
        impl Wall with Paint {
            fun paint(self) { emit("css", ".wall{}"); }
        }

        fun run_it<T: Paint>(subject: T) {
            subject.paint();
        }

        fun main() {
            run_it(Wall { });
        }
        main();
        "#,
        "compile-time-only",
    );
}

/// The same check under the name-keyed list's own over-approximation: a SECOND
/// trait declares `paint` too, and the candidate list therefore holds a member
/// no `T: Paint` receiver could select. Over-asking cannot make this check miss
/// — the refusal still fires — which is why the list is left unnarrowed here.
#[test]
fn b279_a_same_named_member_on_an_unrelated_trait_does_not_let_the_const_only_check_miss() {
    assert_fails_with(
        r#"
        import std::asset::emit;

        trait Paint { fun paint(self); }
        trait Coat { fun paint(self); }

        struct Wall { }
        impl Wall with Paint {
            fun paint(self) { emit("css", ".wall{}"); }
        }

        struct Fence { }
        impl Fence with Coat {
            fun paint(self) { }
        }

        fun run_it<T: Paint>(subject: T) {
            subject.paint();
        }

        fun main() {
            run_it(Wall { });
            Fence { }.paint();
        }
        main();
        "#,
        "compile-time-only",
    );
}

/// The other half of consumer 2's direction: a clean instantiation of the same
/// generic stays ADMITTED. The check refuses a path, not a name.
#[test]
fn b279_a_clean_instantiation_of_the_same_generic_is_still_admitted() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::asset::emit;

        trait Paint { fun paint(self); }

        struct Wall { }
        impl Wall with Paint {
            fun paint(self) { emit("css", ".wall{}"); }
        }

        struct Board { }
        impl Board with Paint {
            fun paint(self) { print("board"); }
        }

        fun run_it<T: Paint>(subject: T) {
            subject.paint();
        }

        fun prime(): i32 {
            run_it(Wall { });
            1
        }

        fun main() {
            let _primed = const prime();
            run_it(Board { });
        }
        main();
        "#,
        "board\n",
    );
}

/// The geometry the guard must NOT fire on, and the one that made a first
/// formulation of it wrong: an impl of the bounded trait that NOTHING
/// instantiates, whose body reads the context strictly. It has no direct
/// callers and no refined dispatch edge — the walk resolves `run_it`'s site to
/// `Board` alone — so it is exempt as dead, and it IS dead. The site still
/// contributed an edge (to `Board::paint`), which is what says the refinement
/// resolved rather than dropped it.
#[test]
fn b279_an_uninstantiated_strict_impl_of_a_bounded_trait_is_dead_not_refused() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::context::Context;

        let scope: Context<str> = Context::new();

        trait Paint { fun paint(self): str; }

        struct Wall { }
        impl Wall with Paint {
            fun paint(self): str { i"wall under {scope.get()}" }
        }

        struct Board { }
        impl Board with Paint {
            fun paint(self): str { "board" }
        }

        fun run_it<T: Paint>(subject: T): str {
            subject.paint()
        }

        fun main() {
            print(run_it(Board { }));
        }
        main();
        "#,
        "board\n",
    );
}

// --- B316: the trait-typed position, asked the other way round -------------------
// `reconcile_type(Concrete, Trait)` accepts when the concrete implements the
// trait; `reconcile_type(Trait, Concrete)` had no arm and refused. A CALL is the
// one position that asks in that order — the bindings key on the CALLEE's
// generics, so the parameter goes first — and every other position reconciles
// value-first and accepts. RULED (Order 34, R5): the universal reading, the two
// orders agree. The eighteen programs B306's backstop measured are not
// recoverable (they were counted by an instrument that was never landed), so the
// pin set is the exhibit the count NAMED — `Doubler`'s supertrait defaults —
// with the representative shapes around it.

#[test]
fn b316_a_supertrait_default_hands_self_to_the_inherited_requirement() {
    // The exhibit. Inside a default body `Self` interns as the bare trait, so
    // `self.add(self)` passes a `Doubler` where `Add::add`'s `Self`-typed
    // operand wants an `Add` — "Expected Add, but got Doubler" — and the
    // supertrait clause is exactly the promise that every `Doubler` is an `Add`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::operators::Add;

        trait Doubler with Add {
            fun twice(self): Self { self.add(self) }
        }

        struct Money { cents: i32 }
        impl Money with Add {
            fun add(self, b: Money): Money { Money { cents = self.cents + b.cents } }
        }
        impl Money with Doubler {}

        fun main() { print(Money { cents = 3 }.twice().cents); }
        main();
        "#,
        "6\n",
    );
}

#[test]
fn b316_a_two_level_supertrait_chain_still_agrees() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::operators::Add;

        trait Doubler with Add {
            fun twice(self): Self { self.add(self) }
        }

        trait Quad with Doubler {
            fun four(self): Self { self.twice().twice() }
        }

        struct Money { cents: i32 }
        impl Money with Add {
            fun add(self, b: Money): Money { Money { cents = self.cents + b.cents } }
        }
        impl Money with Doubler {}
        impl Money with Quad {}

        fun main() { print(Money { cents = 3 }.four().cents); }
        main();
        "#,
        "12\n",
    );
}

#[test]
fn b316_a_default_forwarding_a_self_typed_parameter_agrees() {
    // The same shape without an operator: a default body taking `other: Self`
    // and calling a sibling default on it.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Named { fun name(self): str; }

        trait Greeter with Named {
            fun greet(self): str { i"hi {self.name()}" }
            fun both(self, other: Self): str { i"{self.greet()} {other.greet()}" }
        }

        struct Person { n: str }
        impl Person with Named { fun name(self): str { self.n } }
        impl Person with Greeter {}

        fun main() { print(Person { n = "a" }.both(Person { n = "b" })); }
        main();
        "#,
        "hi a hi b\n",
    );
}

#[test]
fn b316_a_trait_typed_position_takes_a_struct_that_implements_it() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait A { fun name(self): str; }
        struct Bag { n: i32 }
        impl Bag with A { fun name(self): str { "bag" } }

        fun show(v: A): str { v.name() }

        fun main() { print(show(Bag { n = 1 })); }
        main();
        "#,
        "bag\n",
    );
}

#[test]
fn b316_a_parameterized_trait_position_binds_its_argument_from_the_value() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Feed<T> { fun feed(self): T; }
        struct Nums { n: i32 }
        impl Nums with Feed<i32> { fun feed(self): i32 { self.n } }

        fun take(f: Feed<i32>): i32 { f.feed() }

        fun main() { print(take(Nums { n = 4 })); }
        main();
        "#,
        "4\n",
    );
}

#[test]
fn b316_a_value_that_does_not_implement_the_trait_is_still_refused() {
    // The control the universal reading rests on: "satisfied by a value that
    // implements it" is a real test, not an acceptance of everything.
    assert_fails(
        r#"
        import std::io::print;

        trait A { fun name(self): str; }
        struct Bag { n: i32 }
        struct Other { n: i32 }
        impl Bag with A { fun name(self): str { "bag" } }

        fun show(v: A): str { v.name() }

        fun main() { print(show(Other { n = 1 })); }
        main();
        "#,
    );
}

#[test]
fn b316_an_unrelated_trait_pair_is_still_refused() {
    // The supertrait arm is the supertrait relation and nothing wider: two
    // traits with no clause between them do not reconcile.
    assert_fails(
        r#"
        import std::io::print;

        trait Named { fun name(self): str; }
        trait Sized2 { fun size(self): i32; }

        trait Greeter with Named {
            fun both(self, other: Sized2): str { i"{self.name()} {other.size()}" }
        }

        struct Person { n: str }
        impl Person with Named { fun name(self): str { self.n } }
        impl Person with Greeter {}

        fun main() { print(Person { n = "a" }.both(Person { n = "b" })); }
        main();
        "#,
    );
}

/// The fence's NON-DIAGNOSTIC face (B355, R3), and the helper both B279 pins
/// are written against since the broad gate landed.
///
/// Analyzes `source` on a large-stack worker with the fallback recorder zeroed
/// THERE (the isolation `dispatch_selections` documents: an analysis is
/// single-threaded, the suite is not), enumerates the coverage pass's dispatch
/// sites exactly as it does, runs the refinement, and hands back — per site
/// whose dispatched member is `member` — the candidate list each fallback
/// widened to, rendered as the impl-subject names the candidates belong to.
///
/// Diagnostics are TOLERATED: the programs below carry one by construction (a
/// generic taken as a value is what makes the level unresolvable in the first
/// place), and under E189's broad gate the coverage refusal that used to be
/// the only observation of this property is no longer emitted at all.
fn dispatch_fallback_subjects(source: &str, member: &str) -> Vec<Vec<String>> {
    use vilan_core::call_graph::{CallGraph, CallTarget, IndirectReason};
    use vilan_core::dispatch_refine::{
        self, DispatchSite, RefinedCaller, candidates_of, member_name_at,
    };
    use vilan_core::id::Id;
    use vilan_core::type_::Type;

    let source = source.to_string();
    let member = member.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (program, _errors) = analyze_source(
                leaked,
                &std_spec(),
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            let program = program.expect("analysis should produce a program");
            let graph = CallGraph::build(&program);
            let mut sites: Vec<DispatchSite> = Vec::new();
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
            let wanted: Vec<Id> = sites
                .iter()
                .filter(|site| member_name_at(&program, site.call) == Some(member.as_str()))
                .map(|site| site.call)
                .collect();
            dispatch_refine::reset_dispatch_fallbacks();
            let _edges = dispatch_refine::refined_edges(&program, &graph, &sites);
            // The impl subject each candidate belongs to, by name — what the
            // claim is about ("`Wall::paint` is in the set").
            let subject_of = |candidate: Id| -> String {
                for implementation in &program.implementations {
                    if implementation
                        .declarations
                        .values()
                        .any(|declared| *declared == candidate)
                    {
                        return match program.type_id_to_type_map.get(&implementation.subject) {
                            Some(Type::Struct(struct_id, _)) => program
                                .structs
                                .get(struct_id)
                                .map(|declared| declared.name.to_string())
                                .unwrap_or_else(|| "?".to_string()),
                            _ => "?".to_string(),
                        };
                    }
                }
                "?".to_string()
            };
            dispatch_refine::dispatch_fallbacks()
                .into_iter()
                .filter(|(call, _)| wanted.contains(call))
                .map(|(_, candidates)| {
                    let mut names: Vec<String> = candidates.into_iter().map(subject_of).collect();
                    names.sort();
                    names.dedup();
                    names
                })
                .collect()
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked")
}

/// B279's invariant, held where a guard can be held: the coverage walk's
/// dead-code exemption is read off the REFINED dispatch edges, and it is sound
/// only because every fallback in `refined_edges` widens to the WHOLE candidate
/// list. Here the dispatching level is taken as a value, so its entries cannot
/// be enumerated and the site resolves to nothing — the fallback must therefore
/// widen to every candidate, `Wall::paint` among them. The moment a fallback
/// narrows instead of widening, `Wall::paint` is exempted as dead, no caller of
/// it is checked for having a context to hand, and this program compiles and
/// prints `undefined` — which is B258's silence from the other direction.
///
/// **Re-pinned on the fence itself (B355, R3).** It used to observe this
/// THROUGH the coverage refusal, which E189's broad gate no longer emits for a
/// program carrying another diagnostic — and this program carries two by
/// construction. The property was always about the candidate SET, not about a
/// message; it is read off the set now.
#[test]
fn b279_an_unresolvable_dispatch_site_still_fences_its_candidates() {
    let widened = dispatch_fallback_subjects(
        r#"
        import std::io::print;
        import std::context::Context;

        let scope: Context<str> = Context::new();

        trait Paint { fun paint(self): str; }

        struct Wall { }
        impl Wall with Paint {
            fun paint(self): str { i"wall under {scope.get()}" }
        }

        struct Board { }
        impl Board with Paint {
            fun paint(self): str { "board" }
        }

        fun run_it<T: Paint>(subject: T): str {
            subject.paint()
        }

        fun main() {
            let taken = run_it;
            print(taken(Board { }));
        }
        main();
        "#,
        "paint",
    );
    assert!(
        !widened.is_empty(),
        "the site must FALL BACK — its level is taken as a value"
    );
    for candidates in &widened {
        assert!(
            candidates.iter().any(|subject| subject == "Wall")
                && candidates.iter().any(|subject| subject == "Board"),
            "a fallback must widen to EVERY candidate, `Wall::paint` among them: {candidates:?}"
        );
    }
}

/// The same shape one level deeper — the unresolvable level FORWARDS into the
/// dispatching one — so the widening has to survive the recursion into the
/// entry's own enclosing function. Re-pinned on the fence for B279's own
/// reason (B355).
#[test]
fn b279_an_unresolvable_level_above_the_dispatch_still_fences() {
    let widened = dispatch_fallback_subjects(
        r#"
        import std::io::print;
        import std::context::Context;

        let scope: Context<str> = Context::new();

        trait Paint { fun paint(self): str; }

        struct Wall { }
        impl Wall with Paint {
            fun paint(self): str { i"wall under {scope.get()}" }
        }

        struct Board { }
        impl Board with Paint {
            fun paint(self): str { "board" }
        }

        fun run_it<T: Paint>(subject: T): str {
            subject.paint()
        }

        fun forward<U: Paint>(subject: U): str { run_it(subject) }

        fun main() {
            let taken = forward;
            print(taken(Board { }));
        }
        main();
        "#,
        "paint",
    );
    assert!(
        !widened.is_empty(),
        "the site must FALL BACK — the level above it is taken as a value"
    );
    for candidates in &widened {
        assert!(
            candidates.iter().any(|subject| subject == "Wall")
                && candidates.iter().any(|subject| subject == "Board"),
            "a fallback must widen to EVERY candidate, `Wall::paint` among them: {candidates:?}"
        );
    }
}

// --- B334 (R4): a receiverless call means the FREE function ------------------
//
// A method name shadowed a same-named free function inside its own `impl`
// block, and the call reported the METHOD's arity — with no qualified escape
// from inside a package (`pkg::ui::when(..)` is "`pkg` is a namespace, not a
// value", a self-import is a cycle). std's own positional value forms had to
// build their struct literal inline because of it. R4 ruled the spelling: a
// method needs a receiver, so a bare call cannot have meant one.

/// The shape, with a USER impl (A99 deleted the std exhibit): the free `tint`
/// takes one argument, the method takes two beside `self`, and the bare call
/// inside the method body means the free one — which is what the RESULT says,
/// since a resolution to the member could only have been an arity error.
#[test]
fn b334_a_receiverless_call_inside_an_impl_means_the_free_function() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Board { label: str }

        fun tint(name: str): str { i"free:{name}" }

        impl Board {
            fun tint(self, name: str, extra: str): str {
                tint(name) + extra
            }
        }

        fun main() {
            print(Board { label = "b" }.tint("x", "!"));
        }
        main();
        "#,
        "free:x!\n",
    );
}

/// ORDER INDEPENDENCE, and the two controls that keep the rule narrow, in one
/// program. The method is declared BELOW the body that calls the free function
/// (the rule reads the impl body's scope, not the walk's progress through it);
/// `self.tint(..)` is still the method, because a receiver is what a method
/// needs; and a RECEIVERLESS associated function (`shade`) still shadows the
/// free one of the same name, because a bare call to it IS a call to it.
#[test]
fn b334_the_rule_is_narrow_to_a_receiverless_call_subject() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Board { label: str }

        fun tint(name: str): str { i"free:{name}" }
        fun shade(): str { "free-shade" }

        impl Board {
            fun paint(self): str {
                tint(self.label) + "|" + Board::origin() + "|" + self.tint("m", "!")
            }

            fun tint(self, name: str, extra: str): str {
                i"method:{name}{extra}"
            }

            fun origin(): str {
                shade()
            }

            fun shade(): str {
                "assoc"
            }
        }

        fun main() {
            print(Board { label = "b" }.paint());
        }
        main();
        "#,
        "free:b|assoc|method:m!\n",
    );
}

/// The control the rule must not swallow: with NO free function of that name,
/// the ordinary walk still answers and the refusal is the method's arity,
/// exactly as it was. A rule that silently resolved to nothing here would turn
/// a readable arity error into "cannot find `paint`".
#[test]
fn b334_a_receiverless_call_with_no_free_function_still_reports_the_method() {
    assert_fails_with(
        r#"
        import std::io::print;

        struct Board { label: str }

        impl Board {
            fun paint(self, extra: str): str {
                paint(self)
            }
        }

        fun main() {
            print(Board { label = "b" }.paint("!"));
        }
        "#,
        "`paint` expects 2 arguments, but got 1 instead",
    );
}

/// The other control: the name as a VALUE, not a call subject. `let held =
/// tint;` inside the impl still takes the MEMBER — the rule is about what a
/// CALL can have meant, and a value mention is not a call. What proves it is
/// the refusal: a method has no value form, so resolving to the member is
/// refused where resolving to the free function would have compiled.
#[test]
fn b334_a_bare_value_mention_inside_an_impl_still_takes_the_member() {
    assert_fails_with(
        r#"
        import std::io::print;

        struct Board { label: str }

        fun tint(name: str): str { i"free:{name}" }

        impl Board {
            fun tint(self, name: str, extra: str): str {
                i"method:{name}{extra}"
            }

            fun taken(self): str {
                let held = tint;
                held(self, "v", "!")
            }
        }

        fun main() {
            print(Board { label = "b" }.taken());
        }
        "#,
        "a method has no value form",
    );
}

// --- B359: a trait DEFAULT body's `self.member(..)` is the TRAIT's member ---
//
// R1, ruled at Order 38's GO: inside a default body `Self` is opaque, so an
// implementor's INHERENT members are not in scope there (Rust's rule, and what
// the generic-bound route already did). Outside a default body inherent-wins is
// unchanged — the last pin of this block is that control.
//
// Face 1 was the hijack: a default written against the trait's `push` called
// `Bag`'s inherent `push`, so what a default MEANT depended on names its author
// could not know, and an implementor adding an inherent method later silently
// changed every default that called the same name. Face 2 was the miscompile:
// where the hijacked member was `external` the specialized default emitted a
// mangled name nothing defined — clean at check, `ReferenceError` at runtime.

/// Face 1. `Pusher::push_twice`'s body calls the TRAIT's `push` (the default
/// that prints `trait push`), never `Bag`'s inherent one — so `count` stays 0.
#[test]
fn b359_a_default_body_reaches_the_traits_member_not_the_implementors_inherent_one() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Bag { count: i32 }

        impl Bag {
            fun push(&mut self, value: str) {
                print("inherent push");
                self.count += 1;
            }
        }

        trait Pusher<T> {
            fun push(&mut self, value: T) { print("trait push"); }
            fun push_twice(&mut self, value: T) { self.push(value); self.push(value); }
        }

        impl Bag with Pusher<str> {}

        fun main() {
            mut bag = Bag { count = 0 };
            bag.push_twice("x");
            print(i"count {bag.count}");
        }
        "#,
        "trait push\ntrait push\ncount 0\n",
    );
}

/// The control, and the half of R1 that did NOT move: an ORDINARY call site is
/// not inside a default body, so `bag.push("y")` still reaches the inherent
/// member exactly as it always has.
#[test]
fn b359_an_ordinary_call_site_still_reaches_the_inherent_member() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Bag { count: i32 }

        impl Bag {
            fun push(&mut self, value: str) {
                print("inherent push");
                self.count += 1;
            }
        }

        trait Pusher<T> {
            fun push(&mut self, value: T) { print("trait push"); }
            fun push_twice(&mut self, value: T) { self.push(value); self.push(value); }
        }

        impl Bag with Pusher<str> {}

        fun main() {
            mut bag = Bag { count = 0 };
            bag.push("y");
            print(i"count {bag.count}");
        }
        "#,
        "inherent push\ncount 1\n",
    );
}

/// Face 2, the miscompile: `List`'s own `push` is `external`, and the
/// specialized default emitted it by MANGLED NAME — `function $a(self, value) {
/// $b(self, value); $b(self, value); }` against a `$b` nothing defined, which
/// checked clean and threw `ReferenceError: $b is not defined`. The program
/// runs now, and the length proves `List`'s `push` is NOT what `push_twice`
/// reaches: the trait's default prints instead, and the list stays empty.
#[test]
fn b359_a_default_body_over_an_external_inherent_member_runs() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Pusher<T> {
            fun push(&mut self, value: T) { print("trait push"); }
            fun push_twice(&mut self, value: T) { self.push(value); self.push(value); }
        }

        impl List<type T> with Pusher<T> {}

        fun main() {
            mut plain: List<str> = [];
            plain.push_twice("x");
            print(i"len {plain.len()}");
        }
        "#,
        "trait push\ntrait push\nlen 0\n",
    );
}

/// The two ROUTES to one default now agree. Reached through a generic bound
/// (`fill<S: Pusher<str>>`) the call has always dispatched to the trait's
/// `push`; reached through the default body it dispatched to the inherent one.
/// Both print the same thing, and a direct call after them still appends.
#[test]
fn b359_the_generic_bound_route_and_the_default_body_route_agree() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Pusher<T> {
            fun push(&mut self, value: T) { print("trait push"); }
            fun push_twice(&mut self, value: T) { self.push(value); self.push(value); }
        }

        impl List<type T> with Pusher<T> {}

        fun fill<S: Pusher<str>>(target: &mut S) { target.push("g"); }

        fun main() {
            mut plain: List<str> = [];
            plain.push_twice("x");
            fill(&mut plain);
            print(i"len {plain.len()}");
            plain.push("direct");
            print(i"len {plain.len()}");
        }
        "#,
        "trait push\ntrait push\ntrait push\nlen 0\nlen 1\n",
    );
}

/// An impl's OVERRIDE of the trait member is what a default body reaches — the
/// trait-scoped lookup takes the impl's declaration first and only then the
/// trait's own default, so R1 is "the trait's member", not "the trait's body".
#[test]
fn b359_an_impl_override_of_the_trait_member_wins_inside_a_default_body() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Bag { count: i32 }

        impl Bag {
            fun push(&mut self, value: str) { print("inherent push"); }
        }

        trait Pusher<T> {
            fun push(&mut self, value: T) { print("trait push"); }
            fun push_twice(&mut self, value: T) { self.push(value); self.push(value); }
        }

        impl Bag with Pusher<str> {
            fun push(&mut self, value: str) {
                print("impl push");
                self.count += 1;
            }
        }

        fun main() {
            mut bag = Bag { count = 0 };
            bag.push_twice("x");
            print(i"count {bag.count}");
        }
        "#,
        "impl push\nimpl push\ncount 2\n",
    );
}

/// The SUPERTRAIT face. The default lives in `Super`, whose `tick` the
/// implementor provides through `impl Cell with Sub` — a clause that never
/// names `Super`. The wanted-trait filter is a membership test on the clause's
/// own traits, so it turned that impl down and the call fell to the by-name
/// lookup, which the inherent `tick` won. `std`'s own `Source<T>::sub` has
/// exactly this shape (kolt's `StorageSignalCell` writes `impl .. with
/// Signal<T>`), which is why the trait-scoped lookup now walks the type's
/// provided traits for the ones whose supertrait closure reaches the declaring
/// one.
#[test]
fn b359_a_supertrait_default_reaches_the_sub_traits_impl_not_the_inherent_member() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Super {
            fun tick(self): i32;
            fun twice(self): i32 { self.tick() + self.tick() }
        }

        trait Sub with Super {
            fun label(self): str;
        }

        struct Cell { n: i32 }

        impl Cell {
            fun tick(self): i32 { print("inherent tick"); 100 }
        }

        impl Cell with Sub {
            fun tick(self): i32 { print("trait tick"); 1 }
            fun label(self): str { "cell" }
        }

        fun main() {
            let c = Cell { n = 0 };
            print(c.twice());
        }
        "#,
        "trait tick\ntrait tick\n2\n",
    );
}

/// An OPERATOR inside a default body is a call on the trait's member too
/// (B193's channel): `self + self` over the supertrait `Add` reaches `impl
/// Money with Add`'s `add`, not `Money`'s inherent one. 21 + 21 = 42; the
/// inherent `add` answers a zero.
#[test]
fn b359_an_operator_in_a_default_body_reaches_the_traits_member() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::operators::Add;

        struct Money { cents: i32 }

        impl Money {
            fun add(self, other: Money): Money {
                print("inherent add");
                Money { cents = 0 }
            }
        }

        impl Money with Add {
            fun add(self, other: Money): Money {
                Money { cents = self.cents + other.cents }
            }
        }

        trait Doubler with Add {
            fun twice(self): Self { self + self }
        }

        impl Money with Doubler {}

        fun main() {
            print(Money { cents = 21 }.twice().cents);
        }
        "#,
        "42\n",
    );
}

/// A `for` LOOP inside a default body drives the trait's protocol member on the
/// same channel (the loop is a call site like any other, B91/B56): `for value
/// in self` reaches `impl Countdown with Stream`'s `next`, never the inherent
/// one — which would have printed and yielded nothing.
#[test]
fn b359_a_for_loop_in_a_default_body_drives_the_traits_protocol_member() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Countdown { n: i32 }

        impl Countdown {
            fun next(&mut self): Option<i32> {
                print("inherent next");
                None
            }
        }

        trait Stream {
            fun next(&mut self): Option<i32>;

            fun total(&mut self): i32 {
                mut sum = 0;
                for value in self {
                    sum += value;
                }
                sum
            }
        }

        impl Countdown with Stream {
            fun next(&mut self): Option<i32> {
                if self.n > 0 {
                    self.n -= 1;
                    Some(self.n + 1)
                } else {
                    None
                }
            }
        }

        fun main() {
            mut c = Countdown { n = 3 };
            print(c.total());
        }
        "#,
        "6\n",
    );
}

/// The one BEHAVIOUR CHANGE R1 carries through std, pinned at its value.
/// `Ord::clamp`'s default is `self.min(max).max(min)`, and every integer
/// primitive also declares an INHERENT `min`/`max` over the host's `Math.min`/
/// `Math.max` — which the default body used to reach. It reaches `Ord`'s own
/// `min`/`max` defaults now: the same answers through `compare`, which is what
/// this pin holds (the emitted JS differs, and `number-math.mjs` moved with
/// it — its runtime output is byte-identical).
#[test]
fn b359_ords_clamp_default_still_answers_over_the_integers() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            let low: i32 = 2;
            print(low.clamp(3, 7));
            print(9.clamp(0, 5));
            print(4.clamp(0, 5));
            print(8u32.clamp(0u32, 5u32));
        }
        "#,
        "3\n5\n4\n5\n",
    );
}

// --- B390: an impl whose SUBJECT is refused provides nothing. The subject
// --- resolved to `Unknown`, which compares equal to every type, so the block
// --- entered every candidate set: a call on an innocent type was reported
// --- ambiguous "between `Root` and `unknown`", and every type satisfied the
// --- block's trait.

const B390_REFUSED_SUBJECT: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "trait Named {\n",
    "\tfun name(self): str;\n",
    "}\n",
    "\n",
    "struct Root {}\n",
    "\n",
    "struct Leaf<T> { value: T }\n",
    "\n",
    "impl Root with Named {\n",
    "\tfun name(self): str { \"root\" }\n",
    "}\n",
    "\n",
    "impl Leaf<i32, str> with Named {\n",
    "\tfun name(self): str { \"leaf\" }\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet root = Root {};\n",
    "\tprint(root.name());\n",
    "}\n",
);

/// The item's repro: ONE error, the arity refusal at the impl. Red before the
/// fix with a second, "'name' is ambiguous on 'Root': both 'Root' and
/// 'unknown' provide it", at the innocent call.
#[test]
fn b390_a_refused_impl_subject_reports_once_at_the_impl() {
    let diagnostics = failure_diagnostics(B390_REFUSED_SUBJECT);
    assert_eq!(
        diagnostics.len(),
        1,
        "one refused subject is one diagnostic: {diagnostics:#?}"
    );
    assert!(
        diagnostics[0]
            .0
            .contains("`Leaf` takes 1 type argument, 2 given"),
        "{diagnostics:#?}"
    );
}

/// The block's trait is provided to NOTHING — not by method call, not at a
/// bound. Red before the fix, which reported only the arity refusal: the
/// refused block made `i32` a `Named`, so both uses checked clean.
#[test]
fn b390_a_refused_impl_subject_provides_its_trait_to_no_type() {
    let source = concat!(
        "import std::io::print;\n",
        "\n",
        "trait Named {\n",
        "\tfun name(self): str;\n",
        "}\n",
        "\n",
        "struct Leaf<T> { value: T }\n",
        "\n",
        "impl Leaf<i32, str> with Named {\n",
        "\tfun name(self): str { \"leaf\" }\n",
        "}\n",
        "\n",
        "fun shout<T: Named>(value: T): str { value.name() }\n",
        "\n",
        "fun main() {\n",
        "\tprint(shout(5));\n",
        "\tprint(5.name());\n",
        "}\n",
    );
    assert_fails_with(source, "'i32' does not implement trait 'Named'");
    assert_fails_with(source, "i32 has no method 'name'");
}

/// B391: a `[service]`'s GENERATED call to its client's `[rpc]` member is
/// admitted under the module that declared the service — B354's rule for
/// generated code, which the call-site export gate did not ask. `export impl
/// Door` curates the module's exports, the plain `impl Peer` is then not
/// exported, and the generated proxy's `ping` call — filed under the derived
/// sentinel, which reached nothing — was refused as reaching a hidden block of
/// its OWN module. Red before the fix: "'ping' is provided by an `impl` in
/// module ... that ... does not export". The `impl Door` without `export` was
/// the control that always compiled.
#[test]
fn b391_a_services_generated_client_call_is_admitted_under_its_own_module() {
    let exported = concat!(
        "import std::io::print;\n",
        "import std::reactive::{ Signal, SignalCell };\n",
        "import std::rpc_server::{ Connection, Service };\n",
        "import std::json::json_codec;\n",
        "\n",
        "[client_service]\n",
        "struct Peer {\n",
        "\tseen: SignalCell<str>,\n",
        "}\n",
        "\n",
        "impl Peer {\n",
        "\t[rpc]\n",
        "\tfun ping(self, note: str) {\n",
        "\t\tself.seen.set(note);\n",
        "\t}\n",
        "}\n",
        "\n",
        "[service(DoorClient, client = Peer)]\n",
        "struct Door {\n",
        "\tclient: PeerProxy,\n",
        "}\n",
        "\n",
        "export impl Door {\n",
        "\t[rpc]\n",
        "\tfun knock(self): i32 {\n",
        "\t\tself.client.ping(\"knock\");\n",
        "\t\t1\n",
        "\t}\n",
        "}\n",
        "\n",
        "fun main() {\n",
        "\tprint(\"ok\");\n",
        "}\n",
    );
    assert_compiles(exported);
    assert_compiles(&exported.replace("export impl Door", "impl Door"));
}

/// E220: the missing-member steer renders a SUPERTRAIT's member at the
/// arguments the supertrait is reached with. Red before the fix: "declare `fun
/// read(self): i32`", the `with` clause's own argument bound onto `Base`'s `T`.
#[test]
fn e220_a_supertrait_reached_at_a_constructed_argument_is_rendered_at_it() {
    let source = concat!(
        "trait Base<T> {\n",
        "\tfun read(self): T;\n",
        "}\n",
        "\n",
        "trait Feed<T> with Base<List<T>> {\n",
        "\tfun feeds(self): bool { true }\n",
        "}\n",
        "\n",
        "struct Cell {}\n",
        "\n",
        "impl Cell with Feed<i32> {}\n",
    );
    assert_fails_with(
        source,
        "'Cell' does not implement trait 'Feed<i32>': missing 'read'; declare `fun read(self): List<i32>`",
    );
    assert_fails_without(source, "declare `fun read(self): i32`");
}

/// The control: the trait implemented DIRECTLY still renders at its own clause
/// argument (green before and after).
#[test]
fn e220_the_directly_implemented_trait_is_rendered_at_its_clause() {
    assert_fails_with(
        concat!(
            "trait Base<T> {\n",
            "\tfun read(self): T;\n",
            "}\n",
            "\n",
            "struct Cell {}\n",
            "\n",
            "impl Cell with Base<i32> {}\n",
        ),
        "'Cell' does not implement trait 'Base<i32>': missing 'read'; declare `fun read(self): i32`",
    );
}

// ---------------------------------------------------------------------------
// B408 — a BLANKET method reached through an ABSTRACT bound
// ---------------------------------------------------------------------------
//
// `fun f<S: Src<i32>>(s: S) { s.twice() }` was refused "S has no method
// 'twice'" when `twice` is a blanket over `S: Src<T>`: the bounded-parameter
// arm of method lookup searched the bound TRAITS for the name and nothing
// else, so no blanket was ever consulted for an abstract receiver — while the
// same call on a concrete receiver resolved. std met it as `.cell()` and
// `.distinct()` in generic code, and A124 S2c's blanket `map` would have met it
// at every generic `s.map(..)`. A blanket now answers when the parameter's
// DECLARED bounds entail every bound its binders carry; the call binds the
// blanket's binders to the caller's own parameters and composes per instance.

/// The item's repro, the explicit generic spelling.
#[test]
fn b408_a_blanket_method_is_reachable_through_an_explicit_generic_bound() {
    assert_compiles_and_runs(
        concat!(
            "import std::io::print;\n",
            "trait Src<T> { fun get(self): T; }\n",
            "struct Root { v: i32 }\n",
            "impl Root with Src<i32> { fun get(self): i32 { self.v } }\n",
            "impl type S: Src<type T> { fun twice(self): (T, T) { (self.get(), self.get()) } }\n",
            "fun through<S: Src<i32>>(s: S): i32 {\n",
            "\tlet (a, b) = s.twice();\n",
            "\ta + b\n",
            "}\n",
            "fun main() { print(through(Root { v = 2 })); }\n",
        ),
        "4\n",
    );
}

/// The implicit spelling (B186): a trait written as the parameter's type is
/// the same bounded parameter, and reaches the same blanket.
#[test]
fn b408_a_blanket_method_is_reachable_through_an_implicit_trait_parameter() {
    assert_compiles_and_runs(
        concat!(
            "import std::io::print;\n",
            "trait Src<T> { fun get(self): T; }\n",
            "struct Root { v: i32 }\n",
            "impl Root with Src<i32> { fun get(self): i32 { self.v } }\n",
            "impl type S: Src<type T> { fun twice(self): (T, T) { (self.get(), self.get()) } }\n",
            "fun through(s: Src<i32>): i32 {\n",
            "\tlet (a, b) = s.twice();\n",
            "\ta + b\n",
            "}\n",
            "fun main() { print(through(Root { v = 5 })); }\n",
        ),
        "10\n",
    );
}

/// The blanket's bare-trait subject spelling (`impl Src<type T>`, B299) is the
/// same blanket and is reached the same way.
#[test]
fn b408_a_bare_trait_subject_blanket_is_reachable_through_a_bound() {
    assert_compiles_and_runs(
        concat!(
            "import std::io::print;\n",
            "trait Src<T> { fun get(self): T; }\n",
            "struct Root { v: i32 }\n",
            "impl Root with Src<i32> { fun get(self): i32 { self.v } }\n",
            "impl Src<type T> { fun twice(self): (T, T) { (self.get(), self.get()) } }\n",
            "fun through<S: Src<i32>>(s: S): i32 {\n",
            "\tlet (a, b) = s.twice();\n",
            "\ta + b\n",
            "}\n",
            "fun main() { print(through(Root { v = 3 })); }\n",
        ),
        "6\n",
    );
}

/// A SUPERTRAIT of the declared bound carries the blanket's bound, at the
/// arguments the chain passes (`S: Sig<i32>` provides `Src<i32>`); the
/// blanket's `T` is the caller's own `T` where the bound writes one, so two
/// instantiations each get their own; and the blanket member's own generic
/// binds at the call.
#[test]
fn b408_a_blanket_is_reached_through_a_supertrait_a_caller_parameter_and_own_generics() {
    assert_compiles_and_runs(
        concat!(
            "import std::io::print;\n",
            "trait Src<T> { fun get(self): T; }\n",
            "trait Sig<T> with Src<T> { fun name(self): str; }\n",
            "struct Root { v: i32 }\n",
            "impl Root with Src<i32> { fun get(self): i32 { self.v } }\n",
            "impl Root with Sig<i32> { fun name(self): str { \"root\" } }\n",
            "struct Word { w: str }\n",
            "impl Word with Src<str> { fun get(self): str { self.w } }\n",
            "impl type S: Src<type T> {\n",
            "\tfun twice(self): (T, T) { (self.get(), self.get()) }\n",
            "\tfun pair_with<U>(self, u: U): (T, U) { (self.get(), u) }\n",
            "}\n",
            "fun through<S: Sig<i32>>(s: S): i32 { let (a, b) = s.twice(); a + b }\n",
            "fun first<T, S: Src<T>>(s: S): T { let (a, _) = s.twice(); a }\n",
            "fun paired<S: Src<i32>>(s: S): str { let (n, w) = s.pair_with(\"x\"); i\"{n}{w}\" }\n",
            "fun main() {\n",
            "\tprint(through(Root { v = 2 }));\n",
            "\tprint(first(Root { v = 3 }));\n",
            "\tprint(first(Word { w = \"hi\" }));\n",
            "\tprint(paired(Root { v = 4 }));\n",
            "}\n",
        ),
        "4\n3\nhi\n4x\n",
    );
}

/// A blanket that provides a TRAIT's member (`impl type S: Src<i32> with
/// Doubler`) is reached through a bound that never names that trait.
#[test]
fn b408_a_trait_member_a_blanket_provides_is_reachable_through_a_bound() {
    assert_compiles_and_runs(
        concat!(
            "import std::io::print;\n",
            "trait Src<T> { fun get(self): T; }\n",
            "trait Doubler { fun doubled(self): i32; }\n",
            "struct Root { v: i32 }\n",
            "impl Root with Src<i32> { fun get(self): i32 { self.v } }\n",
            "impl type S: Src<i32> with Doubler { fun doubled(self): i32 { self.get() * 2 } }\n",
            "fun through<S: Src<i32>>(s: S): i32 { s.doubled() }\n",
            "fun main() { print(through(Root { v = 21 })); }\n",
        ),
        "42\n",
    );
}

/// std's shape, the item's own: `.cell()` and `.distinct()` — blankets over
/// `S: Source<T>` — called in generic code, and what they return read at the
/// concrete caller after a write.
#[test]
fn b408_std_cell_and_distinct_are_reachable_in_generic_code() {
    assert_compiles_and_runs(
        concat!(
            "import std::io::print;\n",
            "import std::reactive::{ Distinct, Signal, SignalCell, Source };\n",
            "fun cached<S: Source<i32>>(s: S): SignalCell<i32> { s.cell() }\n",
            "fun deduped<S: Source<i32>>(s: S): Distinct<S, i32> { s.distinct() }\n",
            "fun main() {\n",
            "\tlet a = Signal::new(3);\n",
            "\tlet c = cached(a);\n",
            "\tlet d = deduped(a);\n",
            "\ta.set(5);\n",
            "\tprint(i\"{c.get()} {d.get()}\");\n",
            "}\n",
        ),
        "5 5\n",
    );
}

/// The blanket's bound must be ENTAILED, arguments included: a blanket over
/// `Src<str>` is not reached through `S: Src<i32>` (`compare_type` would have
/// admitted it — it never reads a bound's arguments).
#[test]
fn b408_a_blanket_bounded_at_other_arguments_is_not_reached() {
    assert_fails_with(
        concat!(
            "import std::io::print;\n",
            "trait Src<T> { fun get(self): T; }\n",
            "struct Root { v: i32 }\n",
            "impl Root with Src<i32> { fun get(self): i32 { self.v } }\n",
            "impl type S: Src<str> { fun shout(self): str { self.get() } }\n",
            "fun through<S: Src<i32>>(s: S): str { s.shout() }\n",
            "fun main() { print(through(Root { v = 2 })); }\n",
        ),
        "S has no method 'shout'",
    );
}

/// A blanket over a trait the parameter is NOT bounded by is not reached — an
/// abstract parameter implements what its bounds say and nothing else (B173).
#[test]
fn b408_a_blanket_over_an_unrelated_trait_is_not_reached() {
    assert_fails_with(
        concat!(
            "import std::io::print;\n",
            "trait Src<T> { fun get(self): T; }\n",
            "trait Other { fun o(self): i32; }\n",
            "struct Root { v: i32 }\n",
            "impl Root with Src<i32> { fun get(self): i32 { self.v } }\n",
            "impl type S: Other { fun twice(self): i32 { self.o() * 2 } }\n",
            "fun through<S: Src<i32>>(s: S): i32 { s.twice() }\n",
            "fun main() { print(through(Root { v = 2 })); }\n",
        ),
        "S has no method 'twice'",
    );
}

/// A NESTED binder's bound (`Src<type T: PartialEq>`) must hold of what the
/// caller's bound writes there: refused where the caller's `T` carries no
/// `PartialEq`, reached where it does.
#[test]
fn b408_a_nested_binder_bound_must_hold_of_the_callers_argument() {
    let blanket = concat!(
        "import std::io::print;\n",
        "import std::compare::PartialEq;\n",
        "trait Src<T> { fun get(self): T; }\n",
        "struct Root { v: i32 }\n",
        "impl Root with Src<i32> { fun get(self): i32 { self.v } }\n",
        "impl type S: Src<type T: PartialEq> { fun steady(self): bool { self.get() == self.get() } }\n",
    );
    assert_compiles_and_runs(
        &format!(
            "{blanket}{}",
            concat!(
                "fun held<T: PartialEq, S: Src<T>>(s: S): bool { s.steady() }\n",
                "fun main() { print(held(Root { v = 1 })); }\n",
            )
        ),
        "true\n",
    );
    assert_fails_with(
        &format!(
            "{blanket}{}",
            concat!(
                "fun unheld<T, S: Src<T>>(s: S): bool { s.steady() }\n",
                "fun main() { print(unheld(Root { v = 1 })); }\n",
            )
        ),
        "S has no method 'steady'",
    );
}

/// Through the bound `S` is OPAQUE (B359's rule for a default body, Rust's for
/// a bound): the concrete type's own same-named inherent member is not in
/// scope there, so the blanket answers — while the concrete call keeps the
/// inherent (B300's ranking).
#[test]
fn b408_through_a_bound_the_blanket_answers_not_the_concrete_inherent() {
    assert_compiles_and_runs(
        concat!(
            "import std::io::print;\n",
            "trait Src<T> { fun get(self): T; }\n",
            "struct Root { v: i32 }\n",
            "impl Root with Src<i32> { fun get(self): i32 { self.v } }\n",
            "impl Root { fun label(self): str { \"inherent\" } }\n",
            "impl type S: Src<type T> { fun label(self): str { \"blanket\" } }\n",
            "fun through<S: Src<i32>>(s: S): str { s.label() }\n",
            "fun main() { print(Root { v = 1 }.label()); print(through(Root { v = 1 })); }\n",
        ),
        "inherent\nblanket\n",
    );
}

/// A TRAIT's member a blanket provides keeps ONE answer per type: through the
/// bound the call re-dispatches at monomorphization to the most specific impl
/// of the trait (spec types.md "Dispatch through a bound"), so a type with its
/// own `impl Root with Doubler` runs its own body there exactly as at a
/// concrete call, and a type with none runs the blanket's.
#[test]
fn b408_a_blanket_provided_trait_member_re_dispatches_to_the_most_specific_impl() {
    assert_compiles_and_runs(
        concat!(
            "import std::io::print;\n",
            "trait Src<T> { fun get(self): T; }\n",
            "trait Doubler { fun doubled(self): i32; }\n",
            "struct Root { v: i32 }\n",
            "struct Leaf { v: i32 }\n",
            "impl Root with Src<i32> { fun get(self): i32 { self.v } }\n",
            "impl Leaf with Src<i32> { fun get(self): i32 { self.v } }\n",
            "impl type S: Src<i32> with Doubler { fun doubled(self): i32 { self.get() * 2 } }\n",
            "impl Root with Doubler { fun doubled(self): i32 { 1000 } }\n",
            "fun through<S: Src<i32>>(s: S): i32 { s.doubled() }\n",
            "fun main() {\n",
            "\tprint(Root { v = 21 }.doubled());\n",
            "\tprint(through(Root { v = 21 }));\n",
            "\tprint(through(Leaf { v = 4 }));\n",
            "}\n",
        ),
        "1000\n1000\n8\n",
    );
}

// ---------------------------------------------------------------------------
// B409 — a SAME-TRAIT bound checked at the implemented arguments
// ---------------------------------------------------------------------------
//
// `impl M<type S: Src<type X>, X, type U> with Src<U>`: a default inherited
// through that impl was checked at `S: Src<U>` — "Root does not implement
// Src<str>" — instead of the upstream's `S: Src<X>`. The binder written in a
// TRAIT's head (`Src<type X>`) was registered as `Src`'s own parameter id (the
// B77 alias an impl subject's `Wrapper<type T>` takes for its TYPE's
// parameter), so the `with Src<U>` clause binding `Src`'s `T := U` rebound
// `X` too. A trait's head now lends its binders its parameters' bounds, not
// their ids. std::reactive's private `Upstream` blanket was the workaround.

/// reactive-41's repro: the inherited default runs at the implemented
/// argument while the bound holds at the upstream's.
#[test]
fn b409_a_same_trait_bound_is_checked_at_the_upstreams_arguments() {
    assert_compiles_and_runs(
        concat!(
            "import std::io::print;\n",
            "trait Src<T> {\n",
            "\tfun get(self): T;\n",
            "\tfun twice(self): (T, T) { (self.get(), self.get()) }\n",
            "}\n",
            "struct Root { v: i32 }\n",
            "impl Root with Src<i32> { fun get(self): i32 { self.v } }\n",
            "struct M<S, X, U> { up: S, f: |X| U }\n",
            "impl M<type S: Src<type X>, X, type U> with Src<U> {\n",
            "\tfun get(self): U { (self.f)(self.up.get()) }\n",
            "}\n",
            "fun main() {\n",
            "\tlet m = M<Root, i32, str> { up = Root { v = 2 }, f = |n| i\"<{n}>\" };\n",
            "\tlet (a, b) = m.twice();\n",
            "\tprint(a + b);\n",
            "}\n",
        ),
        "<2><2>\n",
    );
}

/// The node shape std's `Map` has, read through GENERIC code: built inside
/// `fun doubled<S: Src<i32>>` and read through a trait default in a second
/// generic function. The default's instance ran under its own substitution
/// alone, dropping the enclosing `S := Root`, and the impl's `self.up.get()`
/// reached `Src`'s body-less requirement (the never-silent internal error) —
/// the second wall between std's nodes and a plain `Source` bound.
#[test]
fn b409_a_node_over_a_callers_parameter_reads_through_an_inherited_default() {
    assert_compiles_and_runs(
        concat!(
            "import std::io::print;\n",
            "trait Src<T> {\n",
            "\tfun get(self): T;\n",
            "\tfun twice(self): (T, T) { (self.get(), self.get()) }\n",
            "}\n",
            "struct Root { value: i32 }\n",
            "impl Root with Src<i32> { fun get(self): i32 { self.value } }\n",
            "struct Mapped<S, T, U> { up: S, transform: |T| U }\n",
            "impl Mapped<type S: Src<type T>, T, type U> with Src<U> {\n",
            "\tfun get(self): U { (self.transform)(self.up.get()) }\n",
            "}\n",
            "fun read_twice<U, R: Src<U>>(source: R): (U, U) { source.twice() }\n",
            "fun doubled<S: Src<i32>>(source: S): (str, str) {\n",
            "\tread_twice(Mapped<S, i32, str> { up = source, transform = |n| i\"<{n * 2}>\" })\n",
            "}\n",
            "fun main() {\n",
            "\tlet (a, b) = doubled(Root { value = 4 });\n",
            "\tprint(a + b);\n",
            "}\n",
        ),
        "<8><8>\n",
    );
}

/// The answer no longer depends on declaration ORDER: with the trait written
/// after the impl (the path that always minted a fresh binder), the same.
#[test]
fn b409_the_trait_declared_after_the_impl_answers_the_same() {
    assert_compiles_and_runs(
        concat!(
            "import std::io::print;\n",
            "struct Root { v: i32 }\n",
            "struct M<S, X, U> { up: S, f: |X| U }\n",
            "impl M<type S: Src<type X>, X, type U> with Src<U> {\n",
            "\tfun get(self): U { (self.f)(self.up.get()) }\n",
            "}\n",
            "trait Src<T> {\n",
            "\tfun get(self): T;\n",
            "\tfun twice(self): (T, T) { (self.get(), self.get()) }\n",
            "}\n",
            "impl Root with Src<i32> { fun get(self): i32 { self.v } }\n",
            "fun main() {\n",
            "\tlet m = M<Root, i32, str> { up = Root { v = 3 }, f = |n| i\"[{n}]\" };\n",
            "\tlet (a, b) = m.twice();\n",
            "\tprint(a + b);\n",
            "}\n",
        ),
        "[3][3]\n",
    );
}

/// What the alias carried is kept: a binder in a trait's head still INHERITS
/// the bound the trait declares for that position (`trait Holds<T: Named>`),
/// so the blanket's body may call `name()` on it.
#[test]
fn b409_a_binder_in_a_trait_head_still_inherits_the_parameters_bound() {
    assert_compiles_and_runs(
        concat!(
            "import std::io::print;\n",
            "trait Named { fun name(self): str; }\n",
            "trait Holds<T: Named> { fun held(self): T; }\n",
            "struct Dog {}\n",
            "impl Dog with Named { fun name(self): str { \"dog\" } }\n",
            "struct Kennel { dog: Dog }\n",
            "impl Kennel with Holds<Dog> { fun held(self): Dog { self.dog } }\n",
            "impl type S: Holds<type X> { fun held_name(self): str { self.held().name() } }\n",
            "fun main() { print(Kennel { dog = Dog {} }.held_name()); }\n",
        ),
        "dog\n",
    );
}

// ---------------------------------------------------------------------------
// B411 — a default's closure parameter typed through a NESTED binder
// ---------------------------------------------------------------------------
//
// `impl Sw<type I: Src<type U>> with Src<U>` reads the trait's argument out of
// `I`'s BOUND. A default the impl inherits (`show(self, f: |T| str)`) was
// specialized with `T := U` while `U` itself was still a hole — the receiver's
// shape binds `I`, and nothing grounded the binder written inside its bound —
// so the closure's parameter was the bare `U` and `s.show(|v| i"{v + 1}")` was
// refused. The inherited default now grounds the bound's binders from the
// receiver's own impls, as a declared member's call already did (B300(a)).
// std::reactive's nodes carry phantom parameters (`Switch<S, T, I, U>`) to
// avoid exactly this shape.

/// reactive-41's repro.
#[test]
fn b411_a_defaults_closure_parameter_is_typed_through_a_nested_binder() {
    assert_compiles_and_runs(
        concat!(
            "import std::io::print;\n",
            "trait Src<T> {\n",
            "\tfun get(self): T;\n",
            "\tfun show(self, f: |T| str): str { f(self.get()) }\n",
            "}\n",
            "struct Root { v: i32 }\n",
            "impl Root with Src<i32> { fun get(self): i32 { self.v } }\n",
            "struct Sw<I> { inner: I }\n",
            "impl Sw<type I: Src<type U>> with Src<U> {\n",
            "\tfun get(self): U { self.inner.get() }\n",
            "}\n",
            "fun main() {\n",
            "\tlet s = Sw { inner = Root { v = 1 } };\n",
            "\tprint(s.show(|v| i\"{v + 1}\"));\n",
            "}\n",
        ),
        "2\n",
    );
}

/// Two levels of nesting, and a default whose closure RETURNS the nested
/// type (`map_to<V>(self, f: |T| V): V`) — the own generic binds from it.
#[test]
fn b411_a_doubly_nested_binder_types_a_defaults_closure() {
    assert_compiles_and_runs(
        concat!(
            "import std::io::print;\n",
            "trait Src<T> {\n",
            "\tfun get(self): T;\n",
            "\tfun map_to<V>(self, f: |T| V): V { f(self.get()) }\n",
            "}\n",
            "struct Root { v: str }\n",
            "impl Root with Src<str> { fun get(self): str { self.v } }\n",
            "struct Sw<I> { inner: I }\n",
            "impl Sw<type I: Src<type U>> with Src<U> {\n",
            "\tfun get(self): U { self.inner.get() }\n",
            "}\n",
            "fun main() {\n",
            "\tlet s = Sw { inner = Sw { inner = Root { v = \"ab\" } } };\n",
            "\tprint(s.map_to(|text| text.len()));\n",
            "}\n",
        ),
        "2\n",
    );
}

/// The node shape std's `Switch` would take without its phantom `U`
/// (`Sw<S, T, I>` with `I: Src<type U>` in the impl's head): read through an
/// inherited default directly and through a generic caller.
#[test]
fn b411_a_switch_shaped_node_reads_its_value_type_from_the_selected_source() {
    assert_compiles_and_runs(
        concat!(
            "import std::io::print;\n",
            "trait Src<T> {\n",
            "\tfun get(self): T;\n",
            "\tfun show(self, f: |T| str): str { f(self.get()) }\n",
            "}\n",
            "struct Root { v: i32 }\n",
            "impl Root with Src<i32> { fun get(self): i32 { self.v } }\n",
            "struct Word { w: str }\n",
            "impl Word with Src<str> { fun get(self): str { self.w } }\n",
            "struct Sw<S, T, I> { up: S, select: |T| I }\n",
            "impl Sw<type S: Src<type T>, T, type I: Src<type U>> with Src<U> {\n",
            "\tfun get(self): U { (self.select)(self.up.get()).get() }\n",
            "}\n",
            "fun shown<U, R: Src<U>>(r: R, f: |U| str): str { r.show(f) }\n",
            "fun main() {\n",
            "\tlet s = Sw<Root, i32, Word> { up = Root { v = 2 }, select = |n| Word { w = i\"w{n}\" } };\n",
            "\tprint(s.show(|text| text + \"!\"));\n",
            "\tprint(shown(s, |text| text + \"?\"));\n",
            "}\n",
        ),
        "w2!\nw2?\n",
    );
}
