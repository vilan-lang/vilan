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
//!   position and every nested spelling stay refused.
//! - B184: a TRAIT written as a STRUCT FIELD's annotation is a HIDDEN type
//!   parameter of the struct — `struct C { x: Trait }` is
//!   `struct C<#0: Trait> { x: #0 }`, one per such field, grounded per LITERAL.
//!   `C` is really `C<impl Trait>`: a mention of it in a parameter or an impl
//!   subject mints a fresh implicit generic, a field of another struct makes
//!   THAT struct generic in turn, and every other position refuses because it
//!   has no value to ground the argument from. Emission is byte-identical to
//!   the written `struct C<S: Trait> { x: S }`.
//! - B218: the hidden argument PRINTS. `Expected C<A>, but got C<B> instead.`
//!   over B186's trait-name display, which at a field yields the useless
//!   `Expected C, but got C` — a deliberate deviation at the one place the
//!   display rule does not work (trait-typed-fields.md rev 2, Q3).
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
            fun sub(self, observer: |Note| void): Subscription { self.inner.sub(observer) }
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
            fun sub(self, observer: |T| void): Subscription { self.inner.sub(observer) }
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

// --- B184: a trait annotation on a STRUCT FIELD is a hidden type parameter ---
//
// The rule, stated once: `struct C { x: X }` desugars to `struct C<#0: X> { x: #0 }`
// where `#0` is a parameter the author never writes and never sees in a type
// argument list. Every mention of `C` in a type position is really `C<impl X>`,
// and the argument comes from the VALUE — the literal binds it, a parameter
// takes it from the call, a field takes it from the struct around it. That is
// the PER-BINDING rule the language already had at `let` (B161) and at a
// parameter (B186), one and two levels down; the alternative — one type per
// field program-wide — is what makes the owner's fourth case invalid, and it is
// the rule these pins discriminate against.
//
// The owner's four cases (trait-typed-fields.md rev 2 §R3) are pinned first,
// verbatim, because case 4 IS the decision.

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
