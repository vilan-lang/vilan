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
            fun describe<S: Serialize>(self, serializer: S) {
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

            fun rebuild<D: Deserialize>(deserializer: D): Outcome<T, E> {
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
    // The FIELD leg left this list with B184 (`b184_a_trait_in_struct_field_
    // position_is_the_hidden_parameter`), the way the parameter leg left it
    // with B186. What is left is the return, the nested spelling, and the
    // CLOSURE parameter — the position that has no generic list to append to.
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
fn b184_a_trait_in_struct_field_position_is_the_hidden_parameter() {
    // SUPERSEDED BY B184 (was `a_trait_in_struct_field_position_is_still_refused`).
    // The field is the third position where a trait annotation is a reading
    // rather than a mistake, and it reads as a HIDDEN type parameter:
    // `struct Kennel { inner: Greet }` is `struct Kennel<#0: Greet> { inner: #0 }`,
    // grounded at the literal. So `.inner` is a `Dog` here — its own field
    // `name` is readable, which is exactly what the refusal used to deny.
    assert_compiles_and_runs(
        &format!(
            r#"{GREET}
            struct Kennel {{ inner: Greet }}
            fun main() {{ print(Kennel {{ inner = Dog {{ name = "rex" }} }}.inner.name); }}
            main();
            "#
        ),
        "rex\n",
    );
}

#[test]
fn b184_case_1_two_bindings_at_one_type() {
    // Valid under all three readings of "all instantiations use the same
    // concrete type", so it decides nothing on its own — it is here because a
    // rule that refuses the field outright (today's, before this lane) reddens
    // it, and because it is the shape every other case is measured against.
    assert_compiles_and_runs(
        &format!(
            r#"{HIDDEN}
            fun main() {{
                let c1 = C {{ x = A {{}} }};
                let c2 = C {{ x = A {{}} }};
                print(c1.x.who() + c2.x.who());
            }}
            main();
            "#
        ),
        "AA\n",
    );
}

#[test]
fn b184_case_2_a_mut_reassigned_at_the_same_type() {
    // The reassignment's type is `C<A>` and the binding's type is `C<A>`, so
    // they agree. Reddened by a per-MENTION rule that minted a second generic
    // at the reassignment — the two would then be independent and refuse.
    assert_compiles_and_runs(
        &format!(
            r#"{HIDDEN}
            fun main() {{
                mut c1 = C {{ x = A {{}} }};
                c1 = C {{ x = A {{}} }};
                print(c1.x.who());
            }}
            main();
            "#
        ),
        "A\n",
    );
}

#[test]
fn b184_case_3_as_written_is_a_scope_error_and_only_that() {
    // The item wrote `c2 = …` with no `let`, which is a plain scope error and
    // stays one: whatever B184 rules, this program is invalid on its first line
    // of trouble, and it must not be invalid for a TYPE reason on top.
    let source = format!(
        r#"{HIDDEN}
        fun main() {{
            mut c1 = C {{ x = A {{}} }};
            c2 = C {{ x = B {{}} }};
        }}
        main();
        "#
    );
    assert_fails_once_with(&source, "cannot find 'c2' in this scope");
    assert_fails_without(&source, "is a trait, not a type");
    assert_fails_without(&source, "hidden type parameter");
}

#[test]
fn b218_case_3_as_intended_names_the_hidden_arguments() {
    // THE B218 pin, and the owner's ruling on Q3. Under B186's display rule an
    // implicit generic renders under its TRAIT's name, which here would print
    // `Expected C, but got C instead.` — a message that reads as a compiler
    // fault. The hidden argument shows instead, in the desugaring's own
    // spelling, and the initializer note that grounded the binding comes with
    // it (the shape B161 already produces one level down).
    let source = format!(
        r#"{HIDDEN}
        fun main() {{
            mut c1 = C {{ x = A {{}} }};
            c1 = C {{ x = B {{}} }};
            print(c1.x.who());
        }}
        main();
        "#
    );
    assert_fails_with(&source, "Expected C<A>, but got C<B> instead.");
    assert_fails_noting(
        &source,
        "Expected C<A>, but got C<B> instead.",
        "C { x = A {} }",
        "the variable's type was inferred from this initializer (C<A>)",
    );
}

#[test]
fn b184_case_4_two_bindings_at_different_types() {
    // THE DISCRIMINATING PIN. Two values, two hidden arguments, two
    // monomorphizations — valid, because the language already answered this at
    // the `let` position (B161) and at the parameter position (B186), and
    // ruling it invalid here would make the field the one position where a
    // trait annotation constrains other people's code. Reddened by the
    // program-wide rule, which refuses it.
    assert_compiles_and_runs(
        &format!(
            r#"{HIDDEN}
            fun main() {{
                let c1 = C {{ x = A {{}} }};
                let c2 = C {{ x = B {{}} }};
                print(c1.x.who());
                print(c2.x.who());
            }}
            main();
            "#
        ),
        "A\nB\n",
    );
}

#[test]
fn b184_a_consumer_of_a_trait_typed_struct_is_generic() {
    // The consequence of case 4 the owner is agreeing to: `fun tell(c: C)` is
    // `fun tell<#0: X>(c: C<#0>)`, so ONE written function takes a `C<A>` and a
    // `C<B>` and dispatches each to its own impl. Under the program-wide rule
    // `tell` would be an ordinary non-generic function and one of the two calls
    // would refuse.
    assert_compiles_and_runs(
        &format!(
            r#"{HIDDEN}
            fun tell(c: C): str {{ c.x.who() }}
            fun main() {{
                print(tell(C {{ x = A {{}} }}));
                print(tell(C {{ x = B {{}} }}));
            }}
            main();
            "#
        ),
        "A\nB\n",
    );
}

#[test]
fn b184_two_mentions_in_one_signature_are_independent() {
    // The same "two annotations, two generics" rule B186 pins one level down
    // (`b186_two_parameters_of_one_trait_are_independent_generics`): `fun
    // both(p: C, q: C)` accepts a `C<A>` and a `C<B>`, because each mention
    // minted its own hidden argument.
    assert_compiles_and_runs(
        &format!(
            r#"{HIDDEN}
            fun both(p: C, q: C): str {{ p.x.who() + q.x.who() }}
            fun main() {{ print(both(C {{ x = A {{}} }}, C {{ x = B {{}} }})); }}
            main();
            "#
        ),
        "AB\n",
    );
}

#[test]
fn b184_two_trait_typed_fields_are_two_hidden_parameters() {
    // One per FIELD, not one per struct — so a struct may hold an `A` and a `B`
    // at once, which is the multi-parameter edge the rule has to answer.
    assert_compiles_and_runs(
        &format!(
            r#"{HIDDEN}
            struct P {{ a: X, b: X }}
            fun main() {{
                let p = P {{ a = A {{}}, b = B {{}} }};
                print(p.a.who() + p.b.who());
            }}
            main();
            "#
        ),
        "AB\n",
    );
}

#[test]
fn b184_a_nested_holder_gains_a_hidden_parameter_of_its_own() {
    // The VIRALITY, which is the price the per-binding rule pays and is
    // invisible exactly as intended: `struct Outer { c: C }` is
    // `struct Outer<#0: X> { c: C<#0> }`. Reddened by a lane that stops the
    // hidden parameter at one level — `Outer`'s field would then be `C` with no
    // argument, which is the erasure B188 closed.
    assert_compiles_and_runs(
        &format!(
            r#"{HIDDEN}
            struct Outer {{ c: C }}
            fun read(o: Outer): str {{ o.c.x.who() }}
            fun main() {{
                print(read(Outer {{ c = C {{ x = A {{}} }} }}));
                print(read(Outer {{ c = C {{ x = B {{}} }} }}));
            }}
            main();
            "#
        ),
        "A\nB\n",
    );
}

#[test]
fn b184_an_impl_subject_grounds_the_hidden_parameter() {
    // A struct with methods, which is what makes the sugar usable at all. The
    // impl is generic over the hidden parameter exactly as `impl C<type S: X>`
    // would be, so `Self` is `C<S>` and `self.x` is the argument's own type.
    assert_compiles_and_runs(
        &format!(
            r#"{HIDDEN}
            impl C {{ fun tell(self): str {{ self.x.who() }} }}
            fun main() {{
                print(C {{ x = A {{}} }}.tell());
                print(C {{ x = B {{}} }}.tell());
            }}
            main();
            "#
        ),
        "A\nB\n",
    );
}

#[test]
fn b184_a_binding_annotation_is_a_constraint_not_a_type() {
    // A `let`'s annotation grounds nothing — the initializer does — so it reads
    // as B161's constraint: the binding keeps `C<A>`, and the annotation only
    // checks that the value really is one of `C`'s.
    assert_compiles_and_runs(
        &format!(
            r#"{HIDDEN}
            fun main() {{
                let c: C = C {{ x = A {{}} }};
                print(c.x.who());
            }}
            main();
            "#
        ),
        "A\n",
    );
    assert_fails_with(
        &format!(
            r#"{HIDDEN}
            struct D {{ v: i32 }}
            fun main() {{ let c: C = D {{ v = 1 }}; }}
            main();
            "#
        ),
        "Expected C, but got D instead.",
    );
}

#[test]
fn b184_a_value_that_lacks_the_trait_is_refused_at_the_literal() {
    // The bound is a real bound: the hidden parameter is declared `: X`, and a
    // field value that does not implement it is refused where every generic
    // bound is refused — at the literal that binds it.
    assert_fails_with(
        &format!(
            r#"{HIDDEN}
            struct N {{ v: i32 }}
            fun main() {{ let c = C {{ x = N {{ v = 1 }} }}; }}
            main();
            "#
        ),
        "'N' does not implement trait 'X', required by a declared bound of 'C'",
    );
}

#[test]
fn b184_the_return_position_refuses() {
    // OUT OF SCOPE for v1, deliberately: a return type has no value in it, so
    // there is nothing to ground the hidden argument from. It is the
    // existential case (Rust's `-> impl Trait`, a different feature from its
    // argument position), and it refuses at the ANNOTATION rather than at every
    // call — which is where the fix goes. Reddened by a lane that quietly makes
    // returns existential.
    let source = format!(
        r#"{HIDDEN}
        fun get(): C {{ C {{ x = A {{}} }} }}
        fun main() {{ print(get().x.who()); }}
        main();
        "#
    );
    assert_fails_with(&source, "carries a hidden type parameter");
    assert_fails_with(&source, "and nothing here can supply one");
    // The steer names the positions that DO ground one, and each of them is a
    // pin above — advice that compiles, not advice.
    assert_fails_with(&source, "a `fun` parameter (`fun f(c: C)`)");
}

#[test]
fn b184_the_type_argument_position_refuses() {
    // The module-level `Context<C>` of the kolt exhibit: a type argument with
    // nothing to ground it. This is the one thing the program-wide rule would
    // have bought, and the honest cost of choosing per-binding instead.
    assert_fails_with(
        &format!(
            r#"{HIDDEN}
            import std::context::Context;
            let ctx = Context<C>::new();
            fun main() {{ print(1); }}
            main();
            "#
        ),
        "carries a hidden type parameter",
    );
}

#[test]
fn b184_the_hidden_argument_is_not_writable() {
    // The author never writes it, and the arity check says so: the hidden
    // parameter is deliberately absent from the DECLARED-parameter table, which
    // is what the check counts, so `C<A>` is over-supply.
    assert_fails_with(
        &format!(
            r#"{HIDDEN}
            fun tell(c: C<A>): str {{ c.x.who() }}
            fun main() {{ print(tell(C {{ x = A {{}} }})); }}
            main();
            "#
        ),
        "`C` takes 0 type arguments, 1 given",
    );
}

#[test]
fn b184_the_sugar_is_refused_on_an_attributed_declaration() {
    // The v1 boundary, and its reason is the paper's own §4: macro reflection
    // is SYNTACTIC — a generator reads the type the author WROTE — so it cannot
    // spell a parameter that was never written, and `[derive(Wire)]` would emit
    // `fun from_json_value(..): Kennel` for a `Kennel` that cannot be named in
    // a return. One report at the annotation beats a page of generated-code
    // follow-ons (B182's rule), so the field keeps the old refusal there.
    let source = format!(
        r#"{GREET}
        [derive(Wire)]
        struct Kennel {{ inner: Greet }}
        fun main() {{ print(1); }}
        main();
        "#
    );
    assert_fails_once_with(&source, "'Greet' is a trait, not a type");
    assert_fails_with(&source, "not on a declaration carrying an attribute");
    assert_fails_without(&source, "from_json_value");
    let diagnostics = failure_diagnostics(&source);
    assert_eq!(
        diagnostics.len(),
        1,
        "one refused annotation is one diagnostic: {diagnostics:#?}"
    );
}

#[test]
fn b218_a_call_argument_mismatch_names_the_hidden_arguments() {
    // B218's shape through the sugar: one written generic, two `C`s at two
    // hidden arguments. Before the hidden argument printed, this read `Expected
    // C, but got C instead.`
    assert_fails_with(
        &format!(
            r#"{HIDDEN}
            fun pair<T>(a: T, b: T): str {{ "ok" }}
            fun main() {{ print(pair(C {{ x = A {{}} }}, C {{ x = B {{}} }})); }}
            main();
            "#
        ),
        "Expected C<A>, but got C<B> instead.",
    );
}

#[test]
fn b218_the_rotate_shape_names_the_hidden_arguments_on_both_reports() {
    // B211's three-way rotate, in the carrier B218 was filed against: TWO
    // reports, and each names which `C` it means. Two reports that both read
    // `Expected C, but got C` is the diagnostic this replaces.
    let source = format!(
        r#"{HIDDEN}
        fun main() {{
            mut p = C {{ x = A {{}} }};
            mut q = C {{ x = B {{}} }};
            let t = p;
            p = q;
            q = t;
            print(p.x.who());
        }}
        main();
        "#
    );
    assert_fails_with(&source, "Expected C<A>, but got C<B> instead.");
    assert_fails_with(&source, "Expected C<B>, but got C<A> instead.");
}

#[test]
fn b184_a_written_parameter_and_a_hidden_one_coexist() {
    // The mixed form, and the shape the estate actually has: a struct that is
    // ALREADY generic and whose trait-typed field's bound mentions its own
    // parameter. Three things have to hold at once and each has its own way to
    // break — the hidden parameter is APPENDED (so `Held<i32>` still writes one
    // argument, and it still means the element), the arity check counts only
    // what the author may write, and the bound is substituted into the
    // mention's terms (`Signal<List<T>>` at `Held<i32>` is `Signal<List<i32>>`,
    // not a bound over the struct's own abstract `T`).
    const HELD: &str = r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };
        struct Held<T> { first: T, list: Signal<List<T>> }
        "#;
    assert_compiles_and_runs(
        &format!(
            r#"{HELD}
            fun count<T>(held: Held<T>): i32 {{ held.list.get().len() }}
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
    // The WRITTEN argument is still checked — only the hidden tail is
    // unwritable, and the report shows both halves.
    assert_fails_with(
        &format!(
            r#"{HELD}
            fun main() {{
                let numbers: Held<i32> = Held {{ first = "z", list = SignalCell::new(["a"]) }};
            }}
            main();
            "#
        ),
        "Expected Held<i32>, but got Held<str, SignalCell<List<str>>> instead.",
    );
}

#[test]
fn b184_the_sugar_emits_exactly_what_the_written_generic_emits() {
    // The claim that makes this sugar and not a new solver mode, checked the
    // only way it can be: BYTE-IDENTICAL JavaScript for the same program spelled
    // both ways. The two sources differ in exactly two lines — the struct's
    // declaration and its consumer's signature — and in nothing the emitter
    // sees.
    const BODY: &str = r#"
        fun main() {
            let c1 = C { x = A { tag = "aa" } };
            let c2 = C { x = B { n = 7 } };
            let c3 = C { x = A { tag = "cc" } };
            print(tell(c1));
            print(tell(c2));
            print(tell(c3));
        }
        main();
        "#;
    const HEAD: &str = r#"
        import std::io::print;
        trait X { fun who(self): str; }
        struct A { tag: str }
        impl A with X { fun who(self): str { self.tag } }
        struct B { n: i32 }
        impl B with X { fun who(self): str { "b" } }
        "#;
    let sugared = compile(&format!(
        "{HEAD}\nstruct C {{ x: X }}\nfun tell(c: C): str {{ c.x.who() }}\n{BODY}"
    ))
    .expect("the sugar compiles");
    let written = compile(&format!(
        "{HEAD}\nstruct C<S: X> {{ x: S }}\nfun tell<S: X>(c: C<S>): str {{ c.x.who() }}\n{BODY}"
    ))
    .expect("the written generic compiles");
    assert_eq!(
        sugared, written,
        "the sugar must emit what the written generic emits, byte for byte"
    );
    // And what they emit is the monomorphized shape, not a dispatched one: two
    // bodies for `tell`, one per hidden argument, sharing across the two `C<A>`
    // values. A rule with a runtime component reddens this.
    assert_eq!(
        emitted_bodies_containing(&sugared, "(c[0])"),
        2,
        "one consumer body per hidden argument:\n{sugared}"
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
fn b243_a_one_block_signal_impl_reaches_source_map_and_effect_on_change() {
    // The shape the item was filed on, against std's own traits: `map` and
    // `effect_on_change` are `Source` defaults, the impl writes one block of
    // `Signal<T>`, and the derived cell tracks the writes. `10` is the
    // owner-registered effect firing on the change, `2` and `10` the derived
    // value before and after.
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
            let doubled = c.map(|v| v * 2);
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
            fun count(self): i32 { self.items.get().len() }
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
    assert_fails_with(
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
            fun count(self): i32 { self.items.get().len() }
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
            fun count(self): i32 { self.by_list.get().len() }
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
            fun count(self): i32 { self.tasks.get().len() }
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
            match client.note("welcome") {
                Ok(let mirror) => {
                    let reading = mirror.sub(|text| print(i"note = {text}"));
                    reading.dispose();
                },
                Err(let error) => print(i"note err {error.debug()}"),
            }
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
#[ignore = "A86: an inherent blanket over a trait subject does not bind the trait's own argument at the call site"]
fn a86_an_inherent_blanket_over_a_trait_binds_its_argument_from_the_receiver() {
    // The member is found and the body monomorphizes — annotating the binding
    // (`let sampled: i32 = cell.sample();`) compiles and prints 7. What does
    // not happen is the INFERENCE: `T` is not bound from the receiver's
    // `Cell<i32>`, so the call's type "is never fully determined" and every
    // use of the result is refused against an unbounded parameter. That is
    // what makes the blanket unshippable for `flatten`: it would demand an
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
#[ignore = "A86: a blanket whose bound argument is itself bounded cannot resolve the receiver to a concrete impl"]
fn a86_a_blanket_over_a_nested_bound_resolves_its_receiver_to_a_concrete_impl() {
    // The nested face, and it fails harder: with the argument of the bound
    // itself bounded (`I: Read<U>` inside `Read<I>`), the receiver is not
    // resolved to a concrete implementation at all, so `self.get()` inside the
    // body resolves to the TRAIT's bodiless requirement and the compiler
    // stops with an internal error naming it — even with the result annotated,
    // and even for a fully concrete receiver. This is the shape the A86
    // `flatten` blanket needs (B268's machinery with B275's gap beside it).
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
            fun describe<S: Serialize>(self, serializer: S) {{
                self.count.describe(serializer);
            }}

            fun rebuild<D: Deserialize>(deserializer: D): Phantom<T> {{
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
