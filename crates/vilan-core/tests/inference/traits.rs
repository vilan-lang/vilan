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
    let source = format!(
        r#"{GREET}
        struct Kennel {{ inner: Greet }}
        impl Kennel {{
            fun speak(self): str {{ self.inner.greet() }}
            fun tag(self): str {{ self.inner.name }}
        }}
        fun main() {{
            let kennel = Kennel {{ inner = Dog {{ name = "rex" }} }};
            print(kennel.speak());
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
    let source = format!(
        r#"{GREET}
        struct Holder<T> {{ v: T }}
        struct Kennel {{ inner: Greet }}
        struct Other {{ held: Holder }}
        impl Kennel {{
            fun speak(self): str {{ self.inner.greet() }}
        }}
        fun main() {{
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
