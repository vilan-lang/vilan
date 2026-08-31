//! Resources and affine moves (`destruction.md`): the `resource` modifier, R1
//! through R12, the `drop` sink, the generic exactly-once rule, and the std
//! resources `Database` and `File`.
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

// --- C4 S1 chunk 1: the `resource` declaration modifier (surface only) -------
//
// destruction.md §3: `resource` is a declaration modifier in `external`'s
// position, canonical order `resource external struct`. This chunk parses,
// carries, and formats the flag with NO classification or affine checking yet,
// so a `resource` type still compiles and runs exactly like its data
// counterpart. (Formatter round-trip is pinned beside its neighbours in
// `formatter.rs`'s `mod reformats`.)

#[test]
fn resource_struct_parses_and_is_inert() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        resource struct Session {
            id: i32,
            name: str,
        }
        fun main() {
            let s = Session { id = 1, name = "a" };
            print(s.name);
        }
        "#,
        "a\n",
    );
}

#[test]
fn resource_struct_with_generics_parses() {
    // Generics on a resource declaration parse and carry through — the flag is
    // independent of the generic parameters.
    assert_compiles_and_runs(
        r#"
        import std::print;
        resource struct Wrapper<T> {
            value: T,
        }
        fun main() {
            let w = Wrapper { value = 42 };
            print(w.value);
        }
        "#,
        "42\n",
    );
}

#[test]
fn resource_enum_parses_and_is_inert() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        resource enum Color {
            Red,
            Green,
            Blue,
        }
        fun main() {
            let c = Color::Green;
            match c {
                Color::Red => print("red"),
                Color::Green => print("green"),
                Color::Blue => print("blue"),
            }
        }
        "#,
        "green\n",
    );
}

#[test]
fn resource_external_struct_parses() {
    // The leaf case: an opaque host resource declares its own resource-ness,
    // in canonical order `resource external struct` (destruction.md §3).
    assert_compiles(
        r#"
        resource external struct Database;
        fun main() {}
        "#,
    );
}

#[test]
fn resource_struct_carries_a_derive_through_expansion() {
    // The flag survives macro expansion: a `[derive(..)]` on a `resource struct`
    // still synthesizes, and the derived `==` works — expansion keeps the
    // modifier (the item is boxed, not rebuilt).
    assert_compiles_and_runs(
        r#"
        import std::print;
        [derive(PartialEq, Debug)]
        resource struct Session {
            id: i32,
            name: str,
        }
        fun main() {
            let a = Session { id = 1, name = "x" };
            let b = Session { id = 1, name = "x" };
            print(a == b);
        }
        "#,
        "true\n",
    );
}

#[test]
fn resource_on_a_function_is_rejected() {
    // `resource` is a type-declaration modifier — anywhere but a struct/enum it
    // steers (destruction.md §3's classification role).
    assert_fails_with("resource fun foo() {}\n", "type-declaration modifier");
}

#[test]
fn resource_on_an_impl_is_rejected() {
    assert_fails_with("resource impl Foo {}\n", "type-declaration modifier");
}

#[test]
fn resource_on_a_let_is_rejected() {
    assert_fails_with(
        r#"
        fun main() {
            resource let x = 1;
        }
        "#,
        "type-declaration modifier",
    );
}

#[test]
fn resource_on_a_trait_is_rejected() {
    assert_fails_with("resource trait Foo {}\n", "type-declaration modifier");
}

#[test]
fn resource_after_external_is_rejected() {
    // Canonical order is `resource external struct`; the reverse is not a
    // program (destruction.md §3 fixes the order).
    assert_fails("external resource struct Database;\n");
}

// === C4 S1 chunk 2: resource CLASSIFICATION + its cheap consumers ===============
// (destruction.md §3 classification, §4 R10/R12, §8 derive interaction). No move/
// loan machinery (R1–R9, R11) and no destructors yet — this chunk only makes
// classification observable through the three cheap checks.

// --- Classification: `type_is_resource` across the containment shapes ----------
// Each shape is observed through a consumer (R10/R12), since classification is
// internal; the point is that the QUERY marks the whole from any resource member.

#[test]
fn resource_classification_direct_declared() {
    // A leaf declared `resource` is a resource — observed via R12 (`print`).
    assert_fails_with(
        r#"
        import std::io::print;
        resource struct Db { handle: i32 }
        fun main() {
            let d = Db { handle = 1 };
            print(d);
        }
        "#,
        "resource",
    );
}

#[test]
fn resource_classification_nested_struct_containment() {
    // A struct with a resource FIELD is a resource, with no `resource` modifier
    // of its own (containment infers — the Wire/Hashable shape, polarity flipped).
    assert_fails_with(
        r#"
        import std::io::print;
        resource struct Db { handle: i32 }
        struct Session { db: Db }
        fun main() {
            let s = Session { db = Db { handle = 1 } };
            print(s);
        }
        "#,
        "resource",
    );
}

#[test]
fn resource_classification_enum_payload_containment() {
    // An enum with a resource PAYLOAD is a resource — observed via R10 (a
    // `List<Holder>` argument is rejected because `Holder` is a resource).
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        enum Holder { Has(Db), Empty }
        fun sink(items: List<Holder>) {}
        fun main() {}
        "#,
        "resource",
    );
}

#[test]
fn resource_classification_tuple_member_containment() {
    // A tuple with a resource MEMBER is a resource — observed via R10.
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        fun sink(items: List<(Db, i32)>) {}
        fun main() {}
        "#,
        "resource",
    );
}

#[test]
fn resource_classification_non_resource_control() {
    // The control: a plain aggregate with no resource anywhere is NOT a resource —
    // it flows into `any` and into a native container freely.
    assert_compiles(
        r#"
        import std::io::print;
        struct Plain { x: i32 }
        fun sink(items: List<Plain>) {}
        fun main() {
            let p = Plain { x = 1 };
            print(p);
        }
        "#,
    );
}

// --- Per-instantiation classification: `Option<Db>` yes, `Option<i32>` no ------

#[test]
fn resource_classification_option_of_resource_is_a_resource() {
    // `Option<Database>` is a resource INSTANTIATION (per-instantiation, like
    // async/platform bits) — observed via R12: an `Option<Db>` value cannot
    // coerce to `any`.
    assert_fails_with(
        r#"
        import std::io::print;
        import std::option::Option::{ self, None };
        resource struct Db { handle: i32 }
        fun main() {
            let o: Option<Db> = None;
            print(o);
        }
        "#,
        "resource",
    );
}

#[test]
fn resource_classification_option_of_data_is_not_a_resource() {
    // The same shape at `i32` stays data — `Option<i32>` coerces to `any` freely,
    // proving classification is decided per substituted instantiation.
    assert_compiles(
        r#"
        import std::io::print;
        import std::option::Option::{ self, None };
        fun main() {
            let o: Option<i32> = None;
            print(o);
        }
        "#,
    );
}

// --- R10: native containers / external generics reject resource arguments ------
// `Option` is the sanctioned container; List/Map/Set and Shared/Task/Promise/
// Context reject (destruction.md §4 R10).

#[test]
fn r10_list_rejects_a_resource_argument() {
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        fun sink(items: List<Db>) {}
        fun main() {}
        "#,
        "resource",
    );
}

#[test]
fn r10_map_rejects_a_resource_argument() {
    assert_fails_with(
        r#"
        import std::map::Map;
        resource struct Db { handle: i32 }
        fun sink(table: Map<str, Db>) {}
        fun main() {}
        "#,
        "resource",
    );
}

#[test]
fn r10_set_rejects_a_resource_argument() {
    assert_fails_with(
        r#"
        import std::set::Set;
        resource struct Db { handle: i32 }
        fun sink(items: Set<Db>) {}
        fun main() {}
        "#,
        "resource",
    );
}

#[test]
fn r10_shared_rejects_a_resource_argument() {
    assert_fails_with(
        r#"
        import std::shared::Shared;
        resource struct Db { handle: i32 }
        fun sink(cell: Shared<Db>) {}
        fun main() {}
        "#,
        "resource",
    );
}

#[test]
fn r10_task_rejects_a_resource_argument() {
    // One of the external generics (Task/Promise/Context) — the same reject path.
    assert_fails_with(
        r#"
        import std::task::Task;
        resource struct Db { handle: i32 }
        fun sink(handle: Task<Db>) {}
        fun main() {}
        "#,
        "resource",
    );
}

#[test]
fn r10_option_accepts_a_resource_argument() {
    // `Option` is the sanctioned resource container — never flagged by R10.
    assert_compiles(
        r#"
        import std::option::Option;
        resource struct Db { handle: i32 }
        fun sink(item: Option<Db>) {}
        fun main() {}
        "#,
    );
}

// --- A19: R10 is asked per INSTANTIATION, not per written head — a resource
// --- reaching a container through a generic aggregate's member is still R10's.

#[test]
fn r10_refuses_a_resource_reaching_shared_through_a_generic_field() {
    // The general case, which `Signal` is only one instance of: `Cell<T>`'s
    // `Shared<T>` holds nothing at its declaration and a `Shared<Db>` here.
    // The diagnostic anchors at what the user wrote (A2) and names the path the
    // resource took to get there (B3).
    assert_fails_spanning(
        r#"
        import std::shared::Shared;
        resource struct Db { handle: i32 }
        struct Cell<T> { value: Shared<T> }
        fun sink(cell: Cell<Db>) {}
        fun main() {}
        "#,
        "Cell<Db>",
        "`Shared` cannot hold the resource `Db`, reached through `Cell.value`",
    );
}

#[test]
fn r10_refuses_a_signal_of_a_resource() {
    // `Signal<T>`'s storage IS a `Shared<T>` (signal-update.md §6), so
    // `Signal<Database>` is `Shared<Database>` by another name — and used to
    // compile clean while the direct spelling was refused.
    assert_fails_spanning(
        r#"
        import std::reactive::Signal;
        import std::db::Database;
        fun sink(cell: Signal<Database>) {}
        fun main() {}
        "#,
        "Signal<Database>",
        "`Shared` cannot hold the resource `Database`, reached through `Signal.value`",
    );
}

#[test]
fn r10_leaves_a_signal_of_data_alone() {
    // The other direction: the descent looks at the INSTANTIATED member, so a
    // data argument reaches a `Shared<i32>` / `Shared<List<str>>` and stops.
    assert_compiles(
        r#"
        import std::reactive::Signal;
        fun main() {
            let count: Signal<i32> = Signal::new(1);
            let names: Signal<List<str>> = Signal::new(["a"]);
            count.set(2);
            names.set(["b"]);
        }
        "#,
    );
}

#[test]
fn r10_refuses_a_resource_reaching_a_list_through_two_generic_fields() {
    // The descent is transitive, and the path names every step it took.
    assert_fails_spanning(
        r#"
        resource struct Db { handle: i32 }
        struct Inner<T> { items: List<T> }
        struct Outer<T> { inner: Inner<T> }
        fun sink(outer: Outer<Db>) {}
        fun main() {}
        "#,
        "Outer<Db>",
        "`List` cannot hold the resource `Db`, reached through `Outer.inner.items`",
    );
}

#[test]
fn r10_leaves_a_generic_aggregate_over_a_resource_alone() {
    // A generic struct is not itself a container: `Holder<Db>` keeps the
    // resource in a field, which is what R10's own steer recommends. Only a
    // NATIVE container beneath it is refused.
    assert_compiles(
        r#"
        resource struct Db { handle: i32 }
        struct Holder<T> { value: T }
        fun sink(holder: Holder<Db>) {}
        fun main() {}
        "#,
    );
}

// --- B103: R10 is asked of an INFERRED container too — containment decides -----
// --- whatever the type's provenance (destruction.md §4 R10).

/// A resource whose teardown is observable, for the B103 routes. Each route is
/// the SAME program in a different spelling: a `List` (or `Shared`, or a tuple)
/// holding a `Guard`, with nothing written down for the walk to record.
fn b103_program(body: &str) -> String {
    format!(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Guard {{ label: str }}
        impl Guard with Drop {{ fun drop(&mut self) {{ print(i"dropped {{self.label}}"); }} }}
        {body}
        "#
    )
}

#[test]
fn r10_refuses_an_inferred_list_of_a_resource() {
    // The filed repro. `mut arr: List<Guard> = [..]` was rejected as designed and
    // the same program with the annotation DELETED compiled — and leaked, since a
    // `List` is not a resource by containment and the binding took no teardown.
    assert_fails_once_with(
        &b103_program(
            r#"
        fun main() {
            mut arr = [Guard { label = "one" }];
            print("end");
        }
        "#,
        ),
        "`List` cannot hold the resource `Guard`",
    );
}

#[test]
fn r10_refuses_a_list_grown_from_an_empty_literal() {
    // The element type arrives at the `push`, not at the literal — an empty
    // literal mints an inference slot (B16), and the slot grounds to `Guard`.
    assert_fails_once_with(
        &b103_program(
            r#"
        fun main() {
            mut arr = [];
            arr.push(Guard { label = "one" });
        }
        "#,
        ),
        "`List` cannot hold the resource `Guard`",
    );
}

#[test]
fn r10_refuses_a_list_from_an_inferred_return_type() {
    // No annotation anywhere: the function's return type is inferred from its
    // tail, and the caller's binding from the call.
    assert_fails_once_with(
        &b103_program(
            r#"
        fun make() { [Guard { label = "one" }] }
        fun main() {
            let xs = make();
        }
        "#,
        ),
        "`List` cannot hold the resource `Guard`",
    );
}

#[test]
fn r10_refuses_a_list_a_generic_hands_back_at_a_resource() {
    // `List<T>` is written, but it is a container at a RESOURCE only where `T`
    // binds — and that binding is inferred at the call.
    assert_fails_once_with(
        &b103_program(
            r#"
        fun wrap<T>(own value: T): List<T> { [value] }
        fun main() {
            let xs = wrap(Guard { label = "one" });
        }
        "#,
        ),
        "`List` cannot hold the resource `Guard`",
    );
}

#[test]
fn r10_refuses_a_list_a_closure_returns() {
    // A closure's return type is never written, so the container exists only in
    // the solver's answer for the call.
    assert_fails_once_with(
        &b103_program(
            r#"
        fun main() {
            let make = || [Guard { label = "one" }];
            let xs = make();
        }
        "#,
        ),
        "`List` cannot hold the resource `Guard`",
    );
}

#[test]
fn r10_refuses_a_container_reached_through_an_inferred_aggregate() {
    // A19's per-instantiation descent, at an instantiation nobody wrote:
    // `Inner<Guard>` comes from the field initializer's type.
    assert_fails_once_with(
        &b103_program(
            r#"
        struct Inner<T> { items: List<T> }
        fun main() {
            let holder = Inner { items = [Guard { label = "one" }] };
        }
        "#,
        ),
        "`List` cannot hold the resource `Guard`, reached through `Inner.items`",
    );
}

#[test]
fn r10_refuses_an_inferred_shared_of_a_resource() {
    // Not only `List`: every rejecting head is asked the same question, and
    // `Shared::new(..)` names none of them.
    assert_fails_once_with(
        &b103_program(
            r#"
        import std::shared::Shared;
        fun main() {
            let cell = Shared::new(Guard { label = "one" });
        }
        "#,
        ),
        "`Shared` cannot hold the resource `Guard`",
    );
}

#[test]
fn r10_refuses_a_list_nested_inside_an_inferred_list() {
    // The descent now enters a container's OWN type arguments. `List<List<Guard>>`
    // holds no resource argument — `List<Guard>` is not a resource — so the outer
    // head alone answers "no", and the inner spelling that used to carry the
    // report does not exist here.
    assert_fails_once_with(
        &b103_program(
            r#"
        fun main() {
            mut nested = [[Guard { label = "one" }]];
        }
        "#,
        ),
        "`List` cannot hold the resource `Guard`",
    );
}

#[test]
fn r10_refuses_a_list_inside_an_inferred_tuple() {
    // A tuple is a value aggregate with no head to ask, and the descent had no
    // arm for one at all.
    assert_fails_once_with(
        &b103_program(
            r#"
        fun main() {
            let pair = (1, [Guard { label = "one" }]);
        }
        "#,
        ),
        "`List` cannot hold the resource `Guard`",
    );
}

#[test]
fn r10_refuses_a_list_argument_bound_to_a_generic_parameter() {
    // The container type exists only as the callee's `List<T>` and the argument's
    // own (unrecorded) type: R10 asked per instantiation is what sees it.
    assert_fails_once_with(
        &b103_program(
            r#"
        fun consume<T>(own items: List<T>) {}
        fun main() {
            consume([Guard { label = "one" }]);
        }
        "#,
        ),
        "`List` cannot hold the resource `Guard`",
    );
}

#[test]
fn r10_refuses_a_container_a_generic_body_builds() {
    // The half no whole-program sweep can reach: the `List<Guard>` exists only
    // INSIDE the instantiated body, and the caller's program never has the type.
    assert_fails_once_with(
        &b103_program(
            r#"
        fun stash<T>(own value: T) { let items = [value]; }
        fun main() {
            stash(Guard { label = "one" });
        }
        "#,
        ),
        "`List` cannot hold the resource `Guard`",
    );
}

#[test]
fn r10_refuses_a_container_temporary_that_binds_no_name() {
    // Nothing records this list's type: it is never bound, and `len` is native so
    // there is no body for the per-instantiation scan. The method call's own
    // substitution is where the receiver's type is written down.
    assert_fails_once_with(
        &b103_program(
            r#"
        fun main() {
            let count = [Guard { label = "one" }].len();
        }
        "#,
        ),
        "`List` cannot hold the resource `Guard`",
    );
}

#[test]
fn r10_reports_an_inferred_container_once_per_container() {
    // The multiplicity IS the claim (B5). The inferred type reaches the check
    // from the literal, the binding, both reads, and the `Inner<Guard>` holding
    // it — and a second binding of the same type is the same fact.
    assert_fails_once_with(
        &b103_program(
            r#"
        fun main() {
            mut first = [Guard { label = "one" }];
            mut second = [Guard { label = "two" }];
            first.push(Guard { label = "three" });
        }
        "#,
        ),
        "`List` cannot hold the resource `Guard`",
    );
}

#[test]
fn r10_reports_an_annotated_container_once_even_though_inference_agrees() {
    // The written spelling keeps its anchor and its single report: the inferred
    // tier never repeats a container a spelling already named.
    assert_fails_once_with(
        &b103_program(
            r#"
        fun main() {
            mut arr: List<Guard> = [Guard { label = "one" }];
        }
        "#,
        ),
        "`List` cannot hold the resource `Guard`",
    );
}

#[test]
fn r10_leaves_an_inferred_container_of_data_alone() {
    // The other direction, and the reason the sweep cannot simply reject every
    // inferred container: a `List` of data is the ordinary case, and the resource
    // beside it drops on its own.
    assert_compiles_and_runs(
        &b103_program(
            r#"
        fun main() {
            mut names = ["a"];
            names.push("b");
            let held = Guard { label = "one" };
            print(i"{names.len()}");
        }
        "#,
        ),
        "dropped one\n2\n",
    );
}

#[test]
fn r10_leaves_an_inferred_fixed_array_of_resources_alone() {
    // The pinned control, in the inferred spelling: `[Guard; 2]` is a value
    // aggregate, a resource BY CONTAINMENT, and it drops in reverse element
    // order — which is why R10 must not reach it, annotation or none.
    assert_compiles_and_runs(
        &b103_program(
            r#"
        fun main() {
            let pair: [Guard; 2] = [Guard { label = "one" }, Guard { label = "two" }];
            print("end");
        }
        "#,
        ),
        "dropped two\ndropped one\nend\n",
    );
}

// --- R12: a resource cannot coerce to `any` (argument, binding, return) --------

#[test]
fn r12_rejects_a_resource_argument_to_any() {
    // The `print(db)` case named in the proposal — `any` is a data sink.
    assert_fails_with(
        r#"
        import std::io::print;
        resource struct Db { handle: i32 }
        fun main() {
            let d = Db { handle = 1 };
            print(d);
        }
        "#,
        "resource",
    );
}

#[test]
fn r12_rejects_a_resource_bound_to_any() {
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        fun main() {
            let d = Db { handle = 1 };
            let sink: any = d;
        }
        "#,
        "resource",
    );
}

#[test]
fn r12_rejects_a_resource_returned_as_any() {
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        fun leak(): any {
            let d = Db { handle = 1 };
            d
        }
        fun main() {}
        "#,
        "resource",
    );
}

#[test]
fn r12_accepts_a_data_value_in_all_three_positions() {
    // The control: a plain value flows into `any` in every position.
    assert_compiles(
        r#"
        import std::io::print;
        struct Plain { x: i32 }
        fun echo(): any {
            let p = Plain { x = 1 };
            print(p);
            let sink: any = Plain { x = 2 };
            Plain { x = 3 }
        }
        fun main() {}
        "#,
    );
}

// --- Derives: Wire / Hashable / PartialEq reject a resource field --------------
// A resource is not plain data: it cannot be sent, hashed by value, or compared
// by copy (destruction.md §8). The resource message takes precedence over the
// generic not-Wire / not-Hashable one.

#[test]
fn derive_wire_rejects_a_resource_field() {
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        [derive(Wire)]
        struct Envelope { db: Db }
        fun main() {}
        "#,
        "resource",
    );
}

#[test]
fn derive_hashable_rejects_a_resource_field() {
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        [derive(Hashable)]
        struct Key { db: Db }
        fun main() {}
        "#,
        "resource",
    );
}

#[test]
fn derive_partialeq_rejects_a_resource_field() {
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        [derive(PartialEq)]
        struct Pair { db: Db }
        fun main() {}
        "#,
        "resource",
    );
}

#[test]
fn derive_accepts_a_data_type() {
    // The control: the same three derives on a plain-data struct compile.
    // `Json` is deliberately not added here — `Wire` already synthesizes the
    // `Json`/`FromJson` impls (§3.9), so combining the two derives on one
    // type is a duplicate-impl conflict unrelated to this file's checks;
    // Json's own no-resource control is `b120_a_json_derived_struct_with_no_resource_stays_legal`.
    assert_compiles(
        r#"
        [derive(Wire, Hashable, PartialEq)]
        struct Point { x: i32, y: i32 }
        fun main() {}
        "#,
    );
}

#[test]
fn resource_classification_fixed_array_containment() {
    // A fixed array of resources is a resource (destruction.md §3: any resource
    // element marks the whole aggregate) — observed via R12 on an annotated
    // `any` binding.
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        fun main() {
            let pair: [Db; 2] = [Db { handle = 1 }, Db { handle = 2 }];
            let laundered: any = pair;
        }
        "#,
        "resource",
    );
}

// A METHOD's `any` parameter is covered too: a concrete-receiver method call
// resolves through the same `subject -> Local(callee)` path as the convention
// checks, so R12 sees its parameters. (The residue is dispatched callees —
// recorded in destruction-impl-plan.md §2.)
#[test]
fn r12_rejects_a_resource_method_argument_to_any() {
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        struct Sink { count: i32 }
        impl Sink {
            fun swallow(self, value: any) {}
        }
        fun main() {
            let db = Db { handle = 1 };
            let sink = Sink { count = 0 };
            sink.swallow(db);
        }
        "#,
        "resource",
    );
}

// === C4 S1 chunk 3: the affine move checker (destruction.md §4, R1–R9) ==========
// Static validation only — no `Drop`, no lowering, no `take`/`replace`. A resource
// has a single owner: it MOVES on binding / `own`-passing / return / construction,
// and is LOANED through `self`/`&`/`&mut`. Each rule gets its own reject AND accept
// pins, plus the ordering-sensitive edges (nested blocks, cross-arm, shadowing).

/// Pins a use-after-move: a primary "after it was moved" diagnostic whose
/// secondary NOTE ("was moved here") is anchored at the `move_occurrence`-th
/// (0-based) occurrence of `name` — the move site, distinct from the use.
#[track_caller]
fn assert_use_after_move_noting(source: &str, name: &str, move_occurrence: usize) {
    let mut start = 0;
    let mut at = None;
    for _ in 0..=move_occurrence {
        at = source[start..].find(name).map(|found| start + found);
        match at {
            Some(position) => start = position + 1,
            None => panic!("occurrence {move_occurrence} of {name:?} not found"),
        }
    }
    let expected = at.unwrap()..at.unwrap() + name.len();
    let diagnostics = failure_diagnostics_with_notes(source);
    let matching: Vec<_> = diagnostics
        .iter()
        .filter(|(message, _, _)| message.contains("after it was moved"))
        .collect();
    assert!(
        !matching.is_empty(),
        "no use-after-move diagnostic; got: {diagnostics:#?}"
    );
    assert!(
        matching.iter().any(|(_, _, note)| note
            .as_ref()
            .is_some_and(|(msg, range, _)| msg.contains("was moved here") && *range == expected)),
        "no use-after-move notes 'was moved here' at occurrence {move_occurrence} of {name:?} \
         ({expected:?}); got: {matching:#?}"
    );
}

// --- R1: `let b = a` moves; a later use of `a` is use-after-move (with note) ----

#[test]
fn r1_let_move_then_use_is_use_after_move_with_note() {
    // The note points at the MOVE site (`let heir = donor`, occurrence 1 of
    // "donor"), the primary at the later use (`&donor`, occurrence 2).
    assert_use_after_move_noting(
        r#"
        resource struct Db { handle: i32 }
        fun peek(d: &Db) {}
        fun main() {
            let donor = Db { handle = 1 };
            let heir = donor;
            peek(&donor);
        }
        "#,
        "donor",
        1,
    );
}

#[test]
fn r1_let_move_without_later_use_compiles() {
    // The move alone is fine — a resource may be re-bound; only a LATER use errors.
    assert_compiles(
        r#"
        resource struct Db { handle: i32 }
        fun sink(own d: Db) {}
        fun main() {
            let donor = Db { handle = 1 };
            let heir = donor;
            sink(heir);
        }
        "#,
    );
}

#[test]
fn r1_double_let_move_is_use_after_move() {
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        fun main() {
            let a = Db { handle = 1 };
            let b = a;
            let c = a;
        }
        "#,
        "after it was moved",
    );
}

// --- R3: `own` moves; `self`/`&`/`&mut`/bare are loans -------------------------

#[test]
fn r3_own_argument_at_last_use_compiles() {
    assert_compiles(
        r#"
        resource struct Db { handle: i32 }
        fun sink(own d: Db) {}
        fun peek(d: &Db) {}
        fun main() {
            let a = Db { handle = 1 };
            peek(&a);
            sink(a);
        }
        "#,
    );
}

#[test]
fn r3_own_argument_not_last_use_is_rejected() {
    // `sink(a)` moves `a`; the later `peek(&a)` — even a loan — is use-after-move.
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        fun sink(own d: Db) {}
        fun peek(d: &Db) {}
        fun main() {
            let a = Db { handle = 1 };
            sink(a);
            peek(&a);
        }
        "#,
        "after it was moved",
    );
}

#[test]
fn r3_loans_never_move_a_resource() {
    // `&`, `&mut`, a method receiver, and repeated loans all leave the binding
    // owned — a later move is fine.
    assert_compiles(
        r#"
        resource struct Db { handle: i32 }
        impl Db { fun ping(&self) {} }
        fun peek(d: &Db) {}
        fun poke(d: &mut Db) {}
        fun sink(own d: Db) {}
        fun main() {
            mut a = Db { handle = 1 };
            peek(&a);
            poke(&mut a);
            a.ping();
            peek(&a);
            sink(a);
        }
        "#,
    );
}

#[test]
fn r3_method_loan_after_a_later_use_compiles() {
    // Calling a method through a loan, then using the binding again, is fine —
    // the receiver loan does not consume it.
    assert_compiles(
        r#"
        resource struct Db { handle: i32 }
        impl Db { fun ping(&self) {} }
        fun sink(own d: Db) {}
        fun main() {
            let a = Db { handle = 1 };
            a.ping();
            a.ping();
            sink(a);
        }
        "#,
    );
}

#[test]
fn r3_own_self_receiver_moves_the_subject() {
    // The receiver is parameter 0, so `own self` reaches the SAME accounting an
    // `own` argument does: the call is a move and a later use is use-after-move,
    // with the note at the call.
    assert_use_after_move_noting(
        r#"
        resource struct Db { handle: i32 }
        impl Db {
            fun close(own self) {}
            fun ping(&self) {}
        }
        fun main() {
            let database = Db { handle = 1 };
            database.close();
            database.ping();
        }
        "#,
        "database",
        1,
    );
}

#[test]
fn r3_bare_self_receiver_stays_a_loan() {
    // The 973-method case: a bare `self` receiver is a LOAN (R3), not a
    // by-value take. B60 must not widen to it — every `Database` call site in
    // std and the corpus depends on this.
    assert_compiles(
        r#"
        resource struct Db { handle: i32 }
        impl Db { fun ping(self) {} }
        fun sink(own d: Db) {}
        fun main() {
            let a = Db { handle = 1 };
            a.ping();
            a.ping();
            sink(a);
        }
        "#,
    );
}

// --- R3, the loan-consumption half: a body may only consume what it OWNS ------
//
// B60's root cause. A loan changes no ownership, so a body that moves its
// loaned parameter out hands the caller a second owner while the caller's
// binding stays live and still drops at scope end — one value destroyed twice.
// `own` is the only convention a body may consume.

#[test]
fn r3_consuming_a_loaned_receiver_is_rejected() {
    // The `Option::unwrap(self)` shape, concrete: `match self` consumes the
    // subject (R6), but `self` is only loaned.
    assert_fails_with(
        r#"
        import std::io::panic;
        resource struct Db { handle: i32 }
        resource enum Slot { Full(Db), Empty }
        impl Slot {
            fun into_inner(self): Db {
                match self {
                    Slot::Full(let inner) => inner,
                    _ => panic("empty"),
                }
            }
        }
        fun sink(own d: Db) {}
        fun main() {
            let slot = Slot::Full(Db { handle = 1 });
            sink(slot.into_inner());
        }
        "#,
        "a loan changes no ownership",
    );
}

#[test]
fn r3_consuming_a_loaned_parameter_is_rejected() {
    // Not a receiver question: a bare (non-`self`) resource parameter is a loan
    // too, so returning it moves it out of a loan.
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        fun steal(d: Db): Db { d }
        fun sink(own d: Db) {}
        fun main() {
            let a = Db { handle = 1 };
            sink(steal(a));
        }
        "#,
        "a loan changes no ownership",
    );
}

#[test]
fn r3_consuming_a_ref_parameter_is_rejected() {
    // The `&`/`&mut` view conventions are loans by the same rule; the fix hint
    // names the parameter's own spelling.
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        resource struct Wrap { inner: Db }
        impl Wrap {
            fun leak(&self): Wrap { self }
        }
        fun main() {
            let w = Wrap { inner = Db { handle = 1 } };
            let stolen = w.leak();
        }
        "#,
        "a loan changes no ownership",
    );
}

#[test]
fn r3_an_own_parameter_may_be_consumed() {
    // The accept half: `own` is exactly the convention that may be moved out.
    assert_compiles(
        r#"
        resource struct Db { handle: i32 }
        fun forward(own d: Db): Db { d }
        fun sink(own d: Db) {}
        fun main() {
            let a = Db { handle = 1 };
            sink(forward(a));
        }
        "#,
    );
}

// --- B60: a consuming call routes into the existing move accounting ------------
//
// `Option::unwrap(own self)` is the spec's R11 example. Every edge shape below
// is decided by the rule that ALREADY governs `own` arguments — this arc adds
// no branch/loop/field/re-init logic of its own, it only makes the call a move.

#[test]
fn b60_a_consuming_call_kills_the_source_binding() {
    // The B60 headline: `o.is_some()` after `o.unwrap()` used to compile clean.
    assert_use_after_move_noting(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        resource struct Res { n: i32 }
        fun main() {
            let slot: Option<Res> = Some(Res { n = 1 });
            let taken = slot.unwrap();
            print(taken.n);
            print(slot.is_some());
        }
        "#,
        "slot",
        1,
    );
}

#[test]
fn b60_a_consuming_call_in_one_branch_is_a_conditional_move() {
    // R7's precedent, unchanged: moved on one path and not another is an error,
    // because end-of-scope ownership must be static (no runtime drop flags).
    assert_fails_with(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        resource struct Res { n: i32 }
        fun main() {
            let slot: Option<Res> = Some(Res { n = 1 });
            if (true) {
                print(slot.unwrap().n);
            }
        }
        "#,
        "moved on one path",
    );
}

#[test]
fn b60_a_consuming_call_in_a_loop_is_rejected() {
    // R8's precedent: the move would repeat on the next iteration.
    assert_fails_with(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        resource struct Res { n: i32 }
        fun main() {
            let slot: Option<Res> = Some(Res { n = 1 });
            mut index = 0;
            for index < 2 {
                print(slot.unwrap().n);
                index = index + 1;
            }
        }
        "#,
        "declared outside this loop",
    );
}

#[test]
fn b60_a_consuming_call_on_a_field_is_a_partial_move() {
    // R5's precedent: v1 has no partial moves, so `holder.slot.unwrap()` is
    // rejected exactly like `own`-passing the field. `Option::take` is the
    // sanctioned way out of a live aggregate.
    assert_fails_with(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        resource struct Res { n: i32 }
        struct Holder { slot: Option<Res> }
        fun main() {
            let holder = Holder { slot = Some(Res { n = 1 }) };
            print(holder.slot.unwrap().n);
        }
        "#,
        "no partial moves",
    );
}

#[test]
fn b60_reinitialization_after_a_consuming_call_compiles() {
    // The binding-move precedent for a `mut` binding (`scan_move`'s assignment
    // arm re-owns unconditionally): re-initializing after the move is legal,
    // and the drop planner emits no overwrite-drop for the moved-out value —
    // so each resource is destroyed exactly once, in reverse declaration order.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        import std::option::Option::{ self, Some, None };
        resource struct Res { tag: str }
        impl Res with Drop {
            fun drop(&mut self) {
                print(i"drop {self.tag}");
            }
        }
        fun main() {
            mut slot: Option<Res> = Some(Res { tag = "first" });
            let taken = slot.unwrap();
            print(i"got {taken.tag}");
            slot = Some(Res { tag = "second" });
            print("end");
        }
        "#,
        "got first\ndrop first\ndrop second\nend\n",
    );
}

#[test]
fn b60_a_data_option_is_unaffected_by_the_consuming_call() {
    // Rule 1's half: `Option<i32>` is not a resource, so `own self` COPIES and
    // the source stays readable and correct. B60 must not touch the data world.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            let slot: Option<i32> = Some(5);
            print(slot.unwrap());
            print(slot.is_some());
            print(slot.unwrap());
        }
        "#,
        "5\ntrue\n5\n",
    );
}

// --- B63: Option's remaining combinators at a resource instantiation ----------
//
// B60 left nine rejecting. Three (`is_some_and`, `ok_or`, `unzip`) were plain
// `own self` conversions blocked only on the share elision, which B63(a) above
// unblocks. The other six read `self` twice — `match self { Some(_) => self }`
// — or built a `(self, b)` tuple, which is a STORE and so a copy R1 forbids;
// rewriting them over `is`, which LOANS, settles each on its own merits.
//
// Three of the six now work at a resource. Three still reject, and BECAUSE THEY
// MUST: each has a path that discards a resource value it was handed, and a
// generic body cannot destroy a `T` (destruction.md §6). What changed for those
// is the diagnostic — one error naming the value that genuinely cannot be
// handled, where before the first error named `self` and pointed at a fix that
// does not fix it. Data behaviour is unchanged throughout: pinned below, and in
// bytes by the corpus (`equality.js` / `generic-equality.js` lose the tuple).

#[test]
fn b63_is_some_and_at_a_resource_instantiation() {
    // CORRECTED by B66, and this is the arc's headline compat finding.
    //
    // B63 §8.2 converted `is_some_and` to `own self` and pinned it ACCEPTED at
    // a resource, noting an absent `drop a` that it attributed to B62. The
    // attribution was wrong: B62 destroys a *concrete* capture, and this body
    // is generic, where nothing can. The absent drop was never B62's missing
    // line — it was §8.3's own rule going unenforced.
    //
    // `is_some_and` HANDS THE PAYLOAD TO A CLOSURE AND DISCARDS IT: a
    // closure-valued callee loans every argument (`callee_conventions` answers
    // `None`), so `fn(x)` does not move `x` in, and `x` dies with the arm. That
    // is exactly §8.3's "a combinator with a path that discards a resource
    // value it was handed is impossible at a resource instantiation", the rule
    // that already rejects `or` / `xor` / `unwrap_or` — so this rejects too,
    // and for the same reason. The conservative error is the honest state; the
    // alternative is the silent leak this pin used to assert.
    assert_fails_spanning(
        r#"
        import std::print;
        import std::drop::Drop;
        import std::option::Option::{ self, Some, None };
        resource struct Res { tag: str, n: i32 }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            let slot: Option<Res> = Some(Res { tag = "a", n = 7 });
            print(slot.is_some_and(|r| r.n == 7));
            print("end");
        }
        "#,
        "slot.is_some_and(|r| r.n == 7)",
        "a resource-typed value still owns its payload where its scope ends",
    );
}

#[test]
fn b63_is_some_and_at_data_is_untouched_by_the_b66_rejection() {
    // The other half of the ruling: a data instantiation is never enqueued for
    // an R11 check, so `is_some_and` is exactly as it was for every non-resource
    // caller — which is all of them, in std, the corpus and the examples.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            let slot: Option<i32> = Some(7);
            print(slot.is_some_and(|n| n == 7));
            let empty: Option<i32> = None;
            print(empty.is_some_and(|n| n == 7));
        }
        "#,
        "true\nfalse\n",
    );
}

#[test]
fn b63_ok_or_at_a_resource_instantiation() {
    // The payload moves into the `Ok`, which the caller then owns and destroys
    // exactly once — one `drop b`, after `end`.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        import std::option::Option::{ self, Some, None };
        import std::result::Result::{ self, Ok, Err };
        resource struct Res { tag: str, n: i32 }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            let slot: Option<Res> = Some(Res { tag = "b", n = 1 });
            let outcome = slot.ok_or("missing");
            print(outcome.is_ok());
            print("end");
        }
        "#,
        "true\ndrop b\nend\n",
    );
}

#[test]
fn b63_unzip_at_a_resource_instantiation() {
    // The resource half of the pair lands in the returned tuple's first slot,
    // the data half reads back correctly, and the tuple destroys the one
    // resource once.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        import std::option::Option::{ self, Some, None };
        resource struct Res { tag: str, n: i32 }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            let paired: Option<(Res, i32)> = Some((Res { tag = "p", n = 4 }, 9));
            let unzipped = paired.unzip();
            print(unzipped.1.unwrap_or(0));
            print("end");
        }
        "#,
        "9\ndrop p\nend\n",
    );
}

#[test]
fn b63_inspect_at_a_resource_instantiation() {
    // The rewrite's clearest win. `inspect` must read the payload WITHOUT
    // consuming it and then hand the option back; `is Some(let x)` loans, so
    // the receiver survives the test intact and is returned by the `own`
    // parameter's move — one value, one drop.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        import std::option::Option::{ self, Some, None };
        resource struct Res { tag: str, n: i32 }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            let slot: Option<Res> = Some(Res { tag = "c", n = 3 });
            let back = slot.inspect(|r| print(i"saw {r.tag}"));
            print("end");
        }
        "#,
        "saw c\ndrop c\nend\n",
    );
}

#[test]
fn b63_or_else_at_a_resource_instantiation() {
    // `or_else` is resource-clean where `or` is not, and the difference is
    // exact: the fallback is PRODUCED on the `None` path rather than handed in
    // and discarded, and a `self` that reaches that path is `None` — no payload
    // to destroy. One resource is built and destroyed once.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        import std::option::Option::{ self, Some, None };
        resource struct Res { tag: str, n: i32 }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            let empty: Option<Res> = None;
            let filled = empty.or_else(|| Some(Res { tag = "e", n = 5 }));
            print("end");
        }
        "#,
        "drop e\nend\n",
    );
}

#[test]
fn b63_eq_at_a_resource_instantiation() {
    // NOT red at v0.25.0, and that is the finding: `affine-moves.md` §6 listed
    // `eq` among the combinators that "reject at a resource instantiation", and
    // it did not — the old `match (self, b)` moved nothing out of a loan, so
    // the rule it was said to break never applied. It compiled and ran there
    // too. This pin is the one nobody had written, and the rewrite is a shape
    // win rather than a fix: both sides stay LOANS, read once per path, and the
    // per-comparison tuple is gone (`equality.js`, `generic-equality.js`).
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::compare::PartialEq;
        import std::option::Option::{ self, Some, None };
        resource struct Res { tag: str, n: i32 }
        impl Res with PartialEq { fun eq(self, other: Res): bool { self.n == other.n } }
        fun sink(own o: Option<Res>) {}
        fun main() {
            let a: Option<Res> = Some(Res { tag = "a", n = 7 });
            let b: Option<Res> = Some(Res { tag = "b", n = 7 });
            print(a == b);
            sink(a);
            sink(b);
        }
        "#,
        "true\n",
    );
}

#[test]
fn b63_or_at_a_resource_rejects_the_discarded_alternative() {
    // `Some(a).or(b)` must destroy `b`, which a generic body cannot do. `b`
    // stays a LOAN so the rejection is forced: declaring `own b` would make it
    // COMPILE and silently leak (the every-path gap recorded in
    // `affine-moves.md` §6). The error now names `b`, not `self`.
    assert_only_failure_noting_into_std(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        resource struct Db { handle: i32 }
        fun main() {
            let opt: Option<Db> = Some(Db { handle = 1 });
            let other: Option<Db> = None;
            let picked = opt.or(other);
        }
        "#,
        "not move-clean when instantiated with a resource",
        "the loaned parameter `b` is moved out here",
    );
}

#[test]
fn b63_xor_at_a_resource_rejects_the_two_some_discard() {
    // `Some(a).xor(Some(b))` is `None` — it discards BOTH. R7 catches it on the
    // `own` declarations: moved on one path but not all. The whole diagnostic
    // is the single honest sentence, where before it was two errors led by a
    // distraction about `self`.
    assert_fails_with(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        resource struct Db { handle: i32 }
        fun main() {
            let opt: Option<Db> = Some(Db { handle = 1 });
            let other: Option<Db> = None;
            let picked = opt.xor(other);
        }
        "#,
        "a resource-typed value is moved on one path but not all",
    );
}

#[test]
fn b63_unwrap_or_at_a_resource_rejects_the_discarded_fallback() {
    // `Some(v).unwrap_or(f)` must destroy `f`. `own self` removes the receiver
    // from the report, leaving one error that names the fallback — and
    // `unwrap_or_else`, which produces its fallback instead, is the spelling
    // that works.
    assert_only_failure_noting_into_std(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        resource struct Db { handle: i32 }
        fun main() {
            let opt: Option<Db> = Some(Db { handle = 1 });
            let value = opt.unwrap_or(Db { handle = 9 });
        }
        "#,
        "not move-clean when instantiated with a resource",
        "the loaned parameter `fallback` is moved out here",
    );
}

#[test]
fn b63_unwrap_or_else_is_the_resource_clean_fallback() {
    // The steer `unwrap_or`'s rejection implies, pinned so the recommendation
    // cannot rot: producing the fallback instead of handing it in is clean.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        import std::option::Option::{ self, Some, None };
        resource struct Res { tag: str, n: i32 }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            let empty: Option<Res> = None;
            let value = empty.unwrap_or_else(|| Res { tag = "f", n = 2 });
            print(value.n);
        }
        "#,
        "2\ndrop f\n",
    );
}

#[test]
fn b63_the_rewritten_combinators_are_unchanged_at_data() {
    // Rule 1's half, over all nine: `own self` COPIES for data, and `is` tests
    // decide exactly what the `match`es decided. Every line below is
    // byte-identical to the v0.25.0 output.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            let some: Option<i32> = Some(3);
            let none: Option<i32> = None;
            print(some.is_some_and(|n| n > 2));
            print(none.is_some_and(|n| n > 2));
            print(some.ok_or("missing").is_ok());
            print(none.ok_or("missing").is_ok());
            let pair: Option<(i32, str)> = Some((1, "x"));
            let (left, right) = pair.unzip();
            print(left.unwrap_or(0));
            print(some.or(none).unwrap_or(0));
            print(none.or(some).unwrap_or(0));
            print(none.or_else(|| Some(9)).unwrap_or(0));
            print(some.or_else(|| Some(9)).unwrap_or(0));
            print(some.xor(none).unwrap_or(0));
            print(some.xor(Some(4)).unwrap_or(0));
            print(none.xor(none).unwrap_or(0));
            print(some.inspect(|n| print(i"saw {n}")).unwrap_or(0));
            print(some == Some(3));
            print(some == Some(4));
            print(some == none);
            print(none == none);
            print(some.unwrap_or(7));
            print(none.unwrap_or(7));
            print(some.is_some());
        }
        "#,
        "true\nfalse\ntrue\nfalse\n1\n3\n3\n9\n3\n3\n0\n0\nsaw 3\n3\ntrue\nfalse\nfalse\ntrue\n3\n7\ntrue\n",
    );
}

#[test]
fn b63_a_data_option_survives_the_own_self_combinators() {
    // The B60 companion, extended to the nine converted here: `own` copies for
    // data, so the source binding stays readable and correct after each call.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            let slot: Option<i32> = Some(5);
            print(slot.is_some_and(|n| n == 5));
            print(slot.inspect(|n| print(n)).is_some());
            print(slot.or(None).unwrap_or(0));
            print(slot.unwrap_or(0));
            print(slot.is_some());
        }
        "#,
        "true\n5\ntrue\n5\n5\ntrue\n",
    );
}

// --- R4: returns move out, through `if`/`match` tails; a diverging leg exempt ---

#[test]
fn r4_return_moves_a_binding_out() {
    assert_compiles(
        r#"
        resource struct Db { handle: i32 }
        fun give(own d: Db): Db { d }
        fun main() { let x = give(Db { handle = 1 }); }
        "#,
    );
}

#[test]
fn r4_return_through_if_tails_moves_each_branch() {
    // Each branch tail produces the returned resource — an R4 move-out per branch,
    // not a conditional move (the branches do not rejoin into continuing code).
    assert_compiles(
        r#"
        resource struct Db { handle: i32 }
        fun pick(c: bool): Db {
            if c { Db { handle = 1 } } else { Db { handle = 2 } }
        }
        fun main() { let x = pick(true); }
        "#,
    );
}

#[test]
fn r4_return_same_binding_through_both_if_tails_compiles() {
    assert_compiles(
        r#"
        resource struct Db { handle: i32 }
        fun pick(c: bool): Db {
            let d = Db { handle = 1 };
            if c { d } else { d }
        }
        fun main() { let x = pick(true); }
        "#,
    );
}

#[test]
fn r4_diverging_leg_is_exempt_from_every_path() {
    // `d` is moved on the `then` path; the `else` diverges (`ret`) and never
    // reaches the merge, so the every-path requirement is satisfied.
    assert_compiles(
        r#"
        resource struct Db { handle: i32 }
        fun sink(own d: Db) {}
        fun f(c: bool) {
            let d = Db { handle = 1 };
            if c { sink(d); } else { ret; }
        }
        fun main() { f(true); }
        "#,
    );
}

// --- R5: struct literals move in; a resource field is loan-only ----------------

#[test]
fn r5_struct_literal_moves_a_resource_in_then_use_after() {
    assert_use_after_move_noting(
        r#"
        resource struct Db { handle: i32 }
        resource struct Session { db: Db }
        fun peek(d: &Db) {}
        fun main() {
            let conn = Db { handle = 1 };
            let session = Session { db = conn };
            peek(&conn);
        }
        "#,
        "conn",
        1,
    );
}

#[test]
fn r5_field_copy_out_is_rejected() {
    // `let x = s.db` would copy a resource out of a live aggregate — R5 reject.
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        resource struct Session { db: Db }
        fun main() {
            let s = Session { db = Db { handle = 1 } };
            let x = s.db;
        }
        "#,
        "no partial moves",
    );
}

#[test]
fn r5_partial_move_out_via_own_argument_is_rejected() {
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        resource struct Session { db: Db }
        fun sink(own d: Db) {}
        fun f(own s: Session) {
            sink(s.db);
        }
        fun main() {}
        "#,
        "no partial moves",
    );
}

#[test]
fn r5_field_loans_are_accepted() {
    // `&self.db`, `&mut self.db`, and a method through the field are all loans.
    assert_compiles(
        r#"
        resource struct Db { handle: i32 }
        impl Db { fun ping(&self) {} }
        resource struct Session { db: Db }
        fun peek(d: &Db) {}
        fun poke(d: &mut Db) {}
        fun main() {
            mut s = Session { db = Db { handle = 1 } };
            peek(&s.db);
            poke(&mut s.db);
            s.db.ping();
        }
        "#,
    );
}

// --- R6: match by value consumes the subject; `match &x` inspects --------------

#[test]
fn r6_match_by_value_consumes_the_subject() {
    // After a by-value match the subject is dead; a second by-value match is
    // use-after-move.
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        enum Holder { Has(Db), Empty }
        fun sink(own d: Db) {}
        fun f(own h: Holder) {
            match h { Holder::Has(let d) => sink(d), Holder::Empty => {}, }
            match h { Holder::Empty => {}, Holder::Has(let d) => sink(d), }
        }
        fun main() {}
        "#,
        "after it was moved",
    );
}

#[test]
fn r6_match_captures_move_the_payload() {
    // The `Some(let d)` capture moves the payload into the arm, where it is moved
    // on once — clean.
    assert_compiles(
        r#"
        resource struct Db { handle: i32 }
        enum Holder { Has(Db), Empty }
        fun sink(own d: Db) {}
        fun f(own h: Holder) {
            match h { Holder::Has(let d) => sink(d), Holder::Empty => {}, }
        }
        fun main() {}
        "#,
    );
}

#[test]
fn r6_match_on_a_loan_inspects_without_consuming() {
    // `match &h` is a loan — the subject stays alive, so a second inspection and a
    // later loan both work.
    assert_compiles(
        r#"
        resource struct Db { handle: i32 }
        enum Holder { Has(Db), Empty }
        fun peek(h: &Holder) {}
        fun f(h: &Holder) {
            match &h { Holder::Has(let d) => {}, Holder::Empty => {}, }
            match &h { Holder::Empty => {}, Holder::Has(let d) => {}, }
            peek(h);
        }
        fun main() {}
        "#,
    );
}

// --- R7: a binding must be moved on every path through a scope, or none --------

#[test]
fn r7_conditional_move_on_one_path_is_rejected() {
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        fun sink(own d: Db) {}
        fun f(c: bool) {
            let d = Db { handle = 1 };
            if c { sink(d); }
        }
        fun main() { f(true); }
        "#,
        "moved on one path",
    );
}

#[test]
fn r7_move_on_both_paths_compiles() {
    assert_compiles(
        r#"
        resource struct Db { handle: i32 }
        fun sink(own d: Db) {}
        fun f(c: bool) {
            let d = Db { handle = 1 };
            if c { sink(d); } else { sink(d); }
        }
        fun main() { f(true); }
        "#,
    );
}

#[test]
fn r7_move_on_neither_path_compiles() {
    assert_compiles(
        r#"
        resource struct Db { handle: i32 }
        fun sink(own d: Db) {}
        fun other() {}
        fun f(c: bool) {
            let d = Db { handle = 1 };
            if c { other(); } else { other(); }
            sink(d);
        }
        fun main() { f(true); }
        "#,
    );
}

#[test]
fn r7_move_in_one_match_arm_and_loan_in_another_is_rejected() {
    // Across arms: `d` is moved in `A`, loaned in `B` — divergent state at the
    // merge, so R7 rejects (a use follows to make the divergence observable).
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        enum Sig { A, B }
        fun sink(own d: Db) {}
        fun peek(d: &Db) {}
        fun f(s: Sig) {
            let d = Db { handle = 1 };
            match s { Sig::A => sink(d), Sig::B => peek(&d), }
            peek(&d);
        }
        fun main() {}
        "#,
        "moved on one path",
    );
}

// --- R8: no moves of an outer binding inside a repeatable interior -------------

#[test]
fn r8_moving_an_outer_binding_inside_a_loop_is_rejected() {
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        fun sink(own d: Db) {}
        fun f() {
            let d = Db { handle = 1 };
            for { sink(d); }
        }
        fun main() { f(); }
        "#,
        "declared outside this loop",
    );
}

#[test]
fn r8_moving_a_loop_local_binding_compiles() {
    // A binding declared INSIDE the loop is fresh each iteration — moving it is
    // fine.
    assert_compiles(
        r#"
        resource struct Db { handle: i32 }
        fun sink(own d: Db) {}
        fun f() {
            for { let d = Db { handle = 1 }; sink(d); }
        }
        fun main() { f(); }
        "#,
    );
}

// --- R9: closures / spawns cannot capture a resource; params are exempt --------

#[test]
fn r9_closure_capturing_a_resource_is_rejected() {
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        fun sink(own d: Db) {}
        fun run_it(body: || void) { body(); }
        fun f() {
            let d = Db { handle = 1 };
            run_it(|| sink(d));
        }
        fun main() { f(); }
        "#,
        "cannot capture the resource",
    );
}

#[test]
fn r9_spawn_capturing_a_resource_is_rejected() {
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        fun sink(own d: Db) {}
        fun f() {
            let d = Db { handle = 1 };
            async { sink(d); }
        }
        fun main() { f(); }
        "#,
        "cannot capture the resource",
    );
}

#[test]
fn r9_closure_resource_parameter_is_not_a_capture() {
    // A closure's OWN resource parameter is per-call, not a capture — the
    // `nursery(|n| ..)` shape. Using it via a method loan is clean.
    assert_compiles(
        r#"
        resource struct Db { handle: i32 }
        impl Db { fun ping(&self) {} }
        fun with_db(body: (|Db| void)) {}
        fun main() {
            with_db(|d| d.ping());
        }
        "#,
    );
}

#[test]
fn r9_injected_context_clause_body_is_exempt() {
    // The spec's canonical injected body: a `context`-clause closure whose
    // resource parameter is a per-call loan, not a capture.
    assert_compiles(
        r#"
        import std::context::Context;
        resource struct Db { handle: i32 }
        impl Db { fun ping(&self) {} }
        let flag: Context<i32> = Context::new();
        fun with_db(body: (|Db| void) context flag) {}
        fun main() {
            with_db(|d| d.ping());
        }
        "#,
    );
}

#[test]
fn r9_closure_capturing_an_outer_resource_beside_its_param_is_rejected() {
    // Seeding the closure's parameter must NOT exempt a genuine outer capture.
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        fun sink(own d: Db) {}
        fun with_db(body: (|Db| void)) {}
        fun f() {
            let outer = Db { handle = 1 };
            with_db(|d| sink(outer));
        }
        fun main() { f(); }
        "#,
        "cannot capture the resource",
    );
}

// --- R9 module-level exemption (destruction.md §4, amended 2026-07-19) ----------
// A closure referencing a MODULE-LEVEL resource is not a capture: the global is
// loan-only with process lifetime (§5's corollary), so the closure can never own
// it and no second owner is created. Locals and parameters stay rejected, and the
// §5 loan-only policing still fires for a CONSUMING use inside a closure body.

#[test]
fn r9_module_level_resource_in_a_sync_closure_is_exempt() {
    // The sync closure (`Expr::Closure`) form: a method loan of the module global.
    assert_compiles(
        r#"
        resource struct Res { handle: i32 }
        impl Res { fun ping(&self) {} }
        let res: Res = Res { handle = 1 };
        fun run_it(body: || void) { body(); }
        fun main() {
            run_it(|| res.ping());
        }
        "#,
    );
}

#[test]
fn r9_module_level_resource_in_an_async_closure_is_exempt() {
    // The async-block form (`Expr::Async` wrapping a block) — same exemption path.
    assert_compiles(
        r#"
        resource struct Res { handle: i32 }
        impl Res { fun ping(&self) {} }
        let res: Res = Res { handle = 1 };
        fun main() {
            let _ = async { res.ping(); };
        }
        "#,
    );
}

#[test]
fn r9_module_level_resource_in_a_spawn_is_exempt() {
    // The fire-and-forget spawn form (`async expr`, also `Expr::Async`).
    assert_compiles(
        r#"
        resource struct Res { handle: i32 }
        impl Res { fun ping(&self) {} }
        let res: Res = Res { handle = 1 };
        fun main() {
            let _ = async res.ping();
        }
        "#,
    );
}

#[test]
fn r9_module_level_resource_in_a_nested_closure_is_exempt() {
    // A closure inside a closure: the free variable is module-level regardless of
    // how many closures enclose it, so the exemption holds at any nesting depth.
    assert_compiles(
        r#"
        resource struct Res { handle: i32 }
        impl Res { fun ping(&self) {} }
        let res: Res = Res { handle = 1 };
        fun run_it(body: || void) { body(); }
        fun main() {
            run_it(|| {
                let inner = || res.ping();
                inner();
            });
        }
        "#,
    );
}

#[test]
fn r9_kolt_hook_shape_over_a_module_level_database_compiles() {
    // The kolt-migration motivation: a `Shared<Fn>` hook closure that reaches a
    // MODULE-LEVEL `Database` and writes a module-level `Signal` — the exact shape
    // that produced 18 R9 errors before the exemption. Real std types. (The
    // end-to-end run over node:sqlite is proven separately by the CLI; the S4a
    // Database pins likewise assert_compiles here.)
    assert_compiles(
        r#"
        import std::reactive::Signal;
        import std::shared::Shared;
        import std::db::Database;
        struct Workspace { id: i32, name: str }
        let db: Database = Database::open(":memory:");
        let workspaces: Signal<List<Workspace>> = Signal::new([]);
        fun main() {
            let create = |name: str| {
                let id = db.prepare("INSERT INTO workspace (name) VALUES (?)").run([name]);
                workspaces.set_with(|list| {
                    mut updated = list;
                    updated.push(Workspace { id = id, name = name });
                    updated
                });
                id
            };
            let hook = Shared::new(create);
            let _ = hook.read()("Inbox");
        }
        "#,
    );
}

#[test]
fn r9_local_resource_in_a_closure_is_still_rejected() {
    // The contrast to the exemption: the SAME loan shape over a LOCAL resource is
    // a capture (a second owner) — still rejected. Only the binding site differs.
    assert_fails_with(
        r#"
        resource struct Res { handle: i32 }
        impl Res { fun ping(&self) {} }
        fun run_it(body: || void) { body(); }
        fun main() {
            let res = Res { handle = 1 };
            run_it(|| res.ping());
        }
        "#,
        "cannot capture the resource",
    );
}

#[test]
fn r9_parameter_resource_in_a_closure_is_still_rejected() {
    // A function PARAMETER is not module-level, so a closure capturing it is a
    // capture — still rejected. The exemption is module-level only.
    assert_fails_with(
        r#"
        resource struct Res { handle: i32 }
        impl Res { fun ping(&self) {} }
        fun run_it(body: || void) { body(); }
        fun holds(r: Res) {
            run_it(|| r.ping());
        }
        fun main() {}
        "#,
        "cannot capture the resource",
    );
}

#[test]
fn r9_consuming_a_module_global_inside_a_closure_via_let_is_rejected() {
    // The exemption is for LOANS only. Consuming the module global inside the
    // closure body (`let mine = res`) still trips the §5 loan-only check: the
    // move scan covers closure bodies, not just top-level function bodies.
    assert_fails_with(
        r#"
        resource struct Res { handle: i32 }
        let res: Res = Res { handle = 1 };
        fun run_it(body: || void) { body(); }
        fun main() {
            run_it(|| {
                let mine = res;
            });
        }
        "#,
        "module-level resource",
    );
}

#[test]
fn r9_dropping_a_module_global_inside_a_closure_is_rejected() {
    // `drop(res)` inside a closure is an own-move of a process-lifetime binding —
    // rejected by the §5 loan-only check, which fires inside closure bodies.
    assert_fails_with(
        r#"
        import std::drop::drop;
        resource struct Res { handle: i32 }
        let res: Res = Res { handle = 1 };
        fun run_it(body: || void) { body(); }
        fun main() {
            run_it(|| {
                drop(res);
            });
        }
        "#,
        "module-level resource",
    );
}

// --- OwnedNursery: the resource-owner story (destruction.md §9) ----------------

#[test]
fn owned_nursery_is_a_resource_use_after_move_is_rejected() {
    // `OwnedNursery` is a `resource` — moving it consumes it, and a use after
    // the move is an error. Pinned against the REAL std type, not a stand-in.
    assert_fails_with(
        r#"
        import std::task::OwnedNursery;
        fun take(own owner: OwnedNursery) {}
        fun main() {
            let owner = OwnedNursery::new();
            take(owner);
            take(owner);
        }
        "#,
        "after it was moved",
    );
}

#[test]
fn owned_nursery_enter_loans_the_owner_and_accepts_a_spawning_body() {
    // `enter(&self, ..)` LOANS the owner (it survives the call, so `cancel`
    // afterward is legal), and its injected `context ambient_nursery` body may
    // spawn — the registration path — and is accepted (R9 exempts the injected
    // clause). The real `OwnedNursery`, exercising the §9 API end to end.
    assert_compiles(
        r#"
        import std::task::OwnedNursery;
        import std::time::sleep;
        fun main() {
            let owner = OwnedNursery::new();
            let _ = owner.enter(|| {
                let _ = async sleep(10);
                0
            });
            owner.cancel();
        }
        "#,
    );
}

#[test]
fn a_spawn_capturing_an_owned_nursery_is_rejected() {
    // R9 with the real type: a spawn that captures the owner is rejected. This
    // is exactly why `Draft`/the SSE pump cannot make their cell a resource and
    // let a handler closure capture it — the migration deferred with C4 S4b.
    assert_fails_with(
        r#"
        import std::task::OwnedNursery;
        fun main() {
            let owner = OwnedNursery::new();
            let _ = async owner.cancel();
        }
        "#,
        "cannot capture the resource",
    );
}

#[test]
fn owned_nursery_enter_runs_its_body_then_drops_clean() {
    // End to end at unit scale: `enter` runs the body (sync here), yields its
    // value, and the owner's `Drop` (cancel) runs at scope end without error.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::task::OwnedNursery;
        fun main() {
            let owner = OwnedNursery::new();
            let value = owner.enter(|| {
                print("in-body");
                7
            });
            print(value);
        }
        "#,
        "in-body\n7\n",
    );
}

// --- Ordering-sensitive edges -------------------------------------------------

#[test]
fn edge_move_in_a_nested_block_kills_the_outer_binding() {
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        fun sink(own d: Db) {}
        fun f() {
            let d = Db { handle = 1 };
            { sink(d); }
            sink(d);
        }
        fun main() { f(); }
        "#,
        "after it was moved",
    );
}

#[test]
fn edge_shadowing_rebinds_a_fresh_owner() {
    // `let d = ..; let d = ..` — the second `d` is a distinct owner, so moving the
    // first and then the second is clean.
    assert_compiles(
        r#"
        resource struct Db { handle: i32 }
        fun sink(own d: Db) {}
        fun f() {
            let d = Db { handle = 1 };
            sink(d);
            let d = Db { handle = 2 };
            sink(d);
        }
        fun main() { f(); }
        "#,
    );
}

// --- Local shadowing & self-referential initializers -------------------------
// B34 + proposal/local-shadowing.md: a local binding is visible from the end
// of its declaring construct; a later same-name declaration shadows from its
// own point on. `let x = x;` used to send the analyzer into a stack-overflow
// abort; same-scope rebinding used to bind EVERY use to the last declaration
// (the emitted JS threw a TDZ ReferenceError at runtime).

#[test]
fn a_self_referential_local_initializer_is_a_clean_error() {
    // The initializer sits inside the declaring statement, so it never sees
    // the binding being declared; with no enclosing `x` that is a plain
    // cannot-find, noted at the declaration.
    assert_fails_noting(
        "fun main() { let x = x; }",
        "cannot find 'x' in this scope",
        "x",
        "an initializer cannot read its own binding",
    );
}

#[test]
fn a_self_referential_local_initializer_is_spanned_at_the_read() {
    assert_fails_spanning_nth(
        "fun main() { let x = x; }",
        "x",
        1,
        "cannot find 'x' in this scope",
    );
}

#[test]
fn a_self_referential_local_with_a_following_mutation_is_a_clean_error() {
    // The assignment routes `check_readonly_mutation` → `readonly_root` into
    // the copy-chain walk one pass earlier than the view checks — a distinct
    // crash entry before the guard.
    assert_fails_with(
        "fun main() { mut x = x; x = 1; }",
        "cannot find 'x' in this scope",
    );
}

#[test]
fn a_module_level_bare_self_reference_does_not_overflow_the_analyzer() {
    // `let a = a;` at module level stays representable (module bindings are
    // order-independent); the copy-chain cycle guard keeps analysis alive and
    // the ungrounded binding reports. Upgrading this to B33's
    // initialization-cycle message is a recorded polish
    // (proposal/local-shadowing.md §6).
    assert_fails_with(
        "let a = a;
        fun main() {}",
        "type of variable 'a' could not be resolved",
    );
}

#[test]
fn a_module_level_bare_copy_cycle_does_not_overflow_the_analyzer() {
    // `let a = b; let b = a;` — the two-member `Expr::Local` cycle recursed
    // `view_binding_mutability` unboundedly before the seen-set.
    assert_fails_with(
        "let a = b;
        let b = a;
        fun main() {}",
        "could not be resolved",
    );
}

#[test]
fn a_same_scope_rebinding_binds_each_use_positionally() {
    // Both prints used to bind the SECOND `d` (resolution ran against the
    // final scope map), so the emitted JS read `d` before its declaration —
    // a TDZ ReferenceError at runtime from a cleanly-compiling program.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let d = 1;
            print(d);
            let d = 2;
            print(d);
        }
        "#,
        "1\n2\n",
    );
}

#[test]
fn a_shadowing_initializer_reads_the_prior_binding() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let x = 1;
            let x = x + 1;
            print(x);
        }
        "#,
        "2\n",
    );
}

#[test]
fn a_block_shadow_ends_with_its_block() {
    // Rust's rule: before the inner `let`, the outer binding is the visible
    // one; after the block, it is again.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let x = 1;
            {
                print(x);
                let x = 2;
                print(x);
            }
            print(x);
        }
        "#,
        "1\n2\n1\n",
    );
}

#[test]
fn a_let_shadows_a_parameter_from_its_point_on() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun f(x: i32): i32 {
            let y = x;
            let x = 10;
            x + y
        }
        fun main() { print(f(3)); }
        "#,
        "13\n",
    );
}

#[test]
fn a_destructure_initializer_never_sees_its_own_binders() {
    // The binder pattern precedes the initializer textually; visibility is
    // the END of the whole statement, so `(b, a)` reads the prior pair.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            let (a, b) = (1, 2);
            let (a, b) = (b, a);
            print(a);
            print(b);
        }
        "#,
        "2\n1\n",
    );
}

#[test]
fn a_for_item_is_shadowable_inside_its_body() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            for x in [1, 2] {
                let x = x * 10;
                print(x);
            }
        }
        "#,
        "10\n20\n",
    );
}

#[test]
fn a_match_capture_is_shadowable_inside_its_arm() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            match Some(1) {
                Some(let v) => {
                    let v = v + 1;
                    print(v);
                }
                None => {}
            }
        }
        "#,
        "2\n",
    );
}

#[test]
fn a_use_before_the_declaration_is_an_error_pointing_at_it() {
    let source = r#"
        import std::print;
        fun main() {
            print(x);
            let x = 1;
        }
        "#;
    assert_fails_spanning(source, "x", "cannot find 'x' in this scope");
    assert_fails_noting_nth(
        source,
        "cannot find 'x' in this scope",
        "x",
        1,
        "a local binding is visible only after its declaration",
    );
}

#[test]
fn a_closure_cannot_capture_a_binding_declared_after_it() {
    assert_fails_with(
        r#"
        fun main() {
            let f = |n: i32| x + n;
            let x = 1;
            let _ = f(1);
            let _ = x;
        }
        "#,
        "cannot find 'x' in this scope",
    );
}

#[test]
fn a_module_binding_may_still_be_read_before_its_declaration() {
    // Module-level bindings stay order-independent (B33 orders emission);
    // positional visibility is a LOCAL rule only.
    assert_compiles_and_runs(
        r#"
        import std::print;
        let early = late + 1;
        let late = 1;
        fun main() { print(early); }
        "#,
        "2\n",
    );
}

#[test]
fn a_view_copy_across_a_shadow_keeps_its_viewness() {
    // `let v = v;` with a prior view `v` is a legal view copy between two
    // DISTINCT bindings — the exact shape that was a self-cycle before.
    assert_compiles(
        r#"
        fun main() {
            mut c = 1;
            let v = &c;
            let v = v;
            let _ = v;
        }
        "#,
    );
}

#[test]
fn an_unterminated_string_at_end_of_input_stays_a_clean_diagnostic() {
    // The lexer's end-of-input salvage skips the quote, so `let prefix =
    // "prefix` tokenizes as `let prefix = prefix` — the live editor-typing
    // path into the self-referential shape (B34).
    assert_fails("fun main() { let prefix = \"prefix");
}

#[test]
fn edge_reassignment_re_owns_a_resource_binding() {
    // R2: assigning onto a `mut` binding that still owns a resource re-owns it
    // (the old value's drop lands in S2); a later use of the new value is fine.
    assert_compiles(
        r#"
        resource struct Db { handle: i32 }
        fun sink(own d: Db) {}
        fun f() {
            mut d = Db { handle = 1 };
            d = Db { handle = 2 };
            sink(d);
        }
        fun main() { f(); }
        "#,
    );
}

#[test]
fn r7_non_terminal_if_tail_move_is_rejected() {
    // The R7/R4 boundary: a branch tail producing a resource is a move-out only
    // in TERMINAL position. Bound to a `let` (the branches rejoin into
    // continuing code), an arm that yields an outer binding while the other
    // yields a fresh value is a conditional move of `d` — rejected.
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        fun open(): Db { Db { handle = 2 } }
        fun f(condition: bool) {
            let d = Db { handle = 1 };
            let r = if condition { d } else { open() };
            let again = &d;
        }
        fun main() { f(true); }
        "#,
        "one path",
    );
}

#[test]
fn r5_variant_construction_moves_the_payload() {
    // `Some(db)` is a constructor move (R5 for enum payloads): the payload
    // leaves `db`, so a later use of `db` is use-after-move.
    assert_fails_with(
        r#"
        import std::option::Option::{ self, Some };
        resource struct Db { handle: i32 }
        fun f() {
            let db = Db { handle = 1 };
            let stored: Option<Db> = Some(db);
            let again = &db;
        }
        fun main() { f(); }
        "#,
        "moved",
    );
}

// === C4 S1 chunk 4: R11 — generics must be move-clean per resource instantiation
// (destruction.md §4/§11). Instantiating a type parameter with a resource re-checks
// the instantiated body under the affine rules (T := the resource): each T-typed
// value used at most once as a move, no captures, no copies. The diagnostic is
// spanned at the INSTANTIATION site (the call), with a note into the generic body.
// The chunk-3 scan is reused verbatim — R11 supplies it a `scan` whose resource
// sets are the body's T-typed places, per instantiation.

// --- Accept: a move-clean generic body, instantiated at a resource --------------

#[test]
fn r11_unwrap_shape_accept() {
    // `own self`/`own x` consumed once, payload moved out once — the canonical
    // move-clean shape (destruction.md §4: `Option::unwrap(self): T` passes).
    assert_compiles(
        r#"
        resource struct Db { handle: i32 }
        fun take_one<T>(own x: T): T { x }
        fun sink(own d: Db) {}
        fun main() {
            let db = Db { handle = 1 };
            let out = take_one(db);
            sink(out);
        }
        "#,
    );
}

#[test]
fn r11_std_option_unwrap_at_a_resource_accept() {
    // The real std `Option::unwrap` (self consumed once by the match, payload
    // moved out once) is clean under R11 when instantiated at `Option<Db>`.
    assert_compiles(
        r#"
        import std::option::Option::{ self, Some };
        resource struct Db { handle: i32 }
        fun sink(own d: Db) {}
        fun main() {
            let db = Db { handle = 1 };
            let opt: Option<Db> = Some(db);
            let d = opt.unwrap();
            sink(d);
        }
        "#,
    );
}

#[test]
fn r11_map_shape_closure_free_accept() {
    // `T` moved exactly once into a (closure-free) transform — a constructor.
    assert_compiles(
        r#"
        resource struct Db { handle: i32 }
        struct Box<T> { inner: T }
        fun wrap<T>(own x: T): Box<T> { Box { inner = x } }
        fun sink(own b: Box<Db>) {}
        fun main() {
            let db = Db { handle = 1 };
            let boxed = wrap(db);
            sink(boxed);
        }
        "#,
    );
}

#[test]
fn r11_std_option_map_at_a_resource_rejects() {
    // CORRECTED by B66. This pin used to assert ACCEPT on the premise that
    // "`Option::map` moves the payload into the transform once (`Some(fn(x))`)".
    // The premise is false: a closure-valued callee LOANS every argument, so
    // `fn(x)` reads `x` and hands back a `U` — the payload `x` is never moved
    // anywhere and dies with the arm.
    //
    // Verified against the pre-B66 tree, which compiled the program below and
    // printed `1 / end` with NO `drop 1`. `map` at a resource was a silent leak.
    assert_fails_spanning(
        r#"
        import std::option::Option::{ self, Some };
        resource struct Db { handle: i32 }
        fun main() {
            let db = Db { handle = 1 };
            let opt: Option<Db> = Some(db);
            let n = opt.map(|d| d.handle);
        }
        "#,
        "opt.map(|d| d.handle)",
        "a resource-typed value still owns its payload where its scope ends",
    );
}

#[test]
fn r11_std_option_map_at_data_is_untouched() {
    // Data instantiations are never enqueued, so `map` is unchanged for them.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            let opt: Option<i32> = Some(2);
            print(opt.map(|n| n * 3).unwrap_or(0));
        }
        "#,
        "6\n",
    );
}

#[test]
fn r11_the_map_shaped_leak_is_a_family_not_one_combinator() {
    // The rule is structural, so it catches every combinator with the same
    // shape — payload handed to a closure, then discarded. `and_then` is
    // pinned as the second member (and is what `filter` is built on), so a
    // future narrowing of the rule to `map` alone would redden here.
    assert_fails_spanning(
        r#"
        import std::option::Option::{ self, Some, None };
        resource struct Db { handle: i32 }
        fun main() {
            let db = Db { handle = 1 };
            let opt: Option<Db> = Some(db);
            let n = opt.and_then(|d| Some(d.handle));
        }
        "#,
        "opt.and_then(|d| Some(d.handle))",
        "a resource-typed value still owns its payload where its scope ends",
    );
}

#[test]
fn r11_generic_struct_method_accept() {
    // An impl-level type parameter (`impl W<type T>`): `into_self` moves the whole
    // resource aggregate out once — clean at `W<Db>`.
    assert_compiles(
        r#"
        resource struct Db { handle: i32 }
        struct W<T> { value: T }
        impl W<type T> {
            fun into_self(own self): W<T> { self }
        }
        fun sink(own w: W<Db>) {}
        fun main() {
            let db = Db { handle = 1 };
            let w = W { value = db };
            let w2 = w.into_self();
            sink(w2);
        }
        "#,
    );
}

#[test]
fn r11_multi_parameter_only_resource_is_checked_accept() {
    // `pick<A, B>` is instantiated with `A := Db` (resource) and `B := i32`
    // (data). `a` is used once; `b` is data. Only `A` joins the resource set, so
    // the body is clean and it compiles.
    assert_compiles(
        r#"
        resource struct Db { handle: i32 }
        fun pick<A, B>(own a: A, b: B): A { a }
        fun sink(own d: Db) {}
        fun main() {
            let db = Db { handle = 1 };
            let out = pick(db, 7);
            sink(out);
        }
        "#,
    );
}

// --- Accept: the SAME generic at a data type is unaffected ----------------------

#[test]
fn r11_same_generic_at_a_data_type_compiles() {
    // `use_twice` reads its parameter twice — a use-after-move ONLY for a
    // resource. Instantiated at `i32` (data, which copies) it is fine: no
    // instantiation is enqueued, nothing is re-checked.
    assert_compiles(
        r#"
        fun use_twice<T>(x: T): T {
            let keep = x;
            x
        }
        fun main() {
            let out = use_twice(5);
        }
        "#,
    );
}

#[test]
fn r11_dirty_generic_stays_usable_at_data_even_when_used_at_a_resource() {
    // The same dirty `use_twice` is instantiated at BOTH `i32` (fine) and `Db`
    // (rejected) — only the resource instantiation reports. `own x`, so the
    // rejection is the use-twice one this test is about and not B60's
    // loan-consumption rule (a bare `x: T` may not be moved out at all).
    let source = r#"
        resource struct Db { handle: i32 }
        fun use_twice<T>(own x: T): T {
            let keep = x;
            x
        }
        fun sink(own d: Db) {}
        fun main() {
            let n = use_twice(5);
            let db = Db { handle = 1 };
            let out = use_twice(db);
            sink(out);
        }
        "#;
    let rejections = r11_rejections(source);
    assert_eq!(
        rejections.len(),
        1,
        "expected exactly one R11 rejection (the resource instantiation); got: {rejections:#?}"
    );
    let call_at = source.find("use_twice(db)").unwrap();
    assert_eq!(
        rejections[0].1,
        call_at..call_at + "use_twice(db)".len(),
        "the R11 diagnostic must span the resource instantiation site"
    );
}

// --- Reject: a dirty generic body, spanned at the instantiation with a note -----

#[test]
fn r11_free_generic_used_twice_is_rejected() {
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        fun use_twice<T>(own x: T): T {
            let keep = x;
            x
        }
        fun sink(own d: Db) {}
        fun main() {
            let db = Db { handle = 1 };
            let out = use_twice(db);
            sink(out);
        }
        "#,
        "is not move-clean when instantiated with a resource",
    );
}

#[test]
fn r11_rejection_is_spanned_at_the_instantiation_with_a_body_note() {
    // Primary AT the call (`use_twice(db)`); the note points INTO the generic body
    // at the second use of `x` (the tail), which lives before the call in source.
    let source = r#"
        resource struct Db { handle: i32 }
        fun use_twice<T>(own x: T): T {
            let keep = x;
            x
        }
        fun sink(own d: Db) {}
        fun main() {
            let db = Db { handle = 1 };
            let out = use_twice(db);
            sink(out);
        }
        "#;
    let rejections = r11_rejections(source);
    assert_eq!(rejections.len(), 1, "one rejection; got: {rejections:#?}");
    let (_, primary, note) = &rejections[0];
    let call_at = source.find("use_twice(db)").unwrap();
    assert_eq!(
        *primary,
        call_at..call_at + "use_twice(db)".len(),
        "primary spans the instantiation site"
    );
    let (note_msg, note_range, _) = note.as_ref().expect("a note into the body");
    assert!(
        note_msg.contains("used here after it was moved"),
        "the note describes the second use; got: {note_msg:?}"
    );
    // The note anchors at the tail `x` — inside the body, before the call site.
    assert!(
        note_range.end <= call_at,
        "the note points into the generic body (before the instantiation): {note_range:?}"
    );
}

#[test]
fn r11_generic_struct_method_used_twice_is_rejected() {
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        struct W<T> { value: T }
        impl W<type T> {
            fun use_twice(own self): W<T> {
                let keep = self;
                self
            }
        }
        fun main() {
            let db = Db { handle = 1 };
            let w = W { value = db };
            let w2 = w.use_twice();
        }
        "#,
        "is not move-clean when instantiated with a resource",
    );
}

#[test]
fn r11_conditional_move_in_a_generic_body_is_rejected() {
    // R7 under T := resource: `x` is moved on one path through the `if` but not
    // the other — rejected at the instantiation of `maybe_sink` at `Db`.
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        fun consume<U>(own u: U) {}
        fun maybe_sink<T>(own x: T, c: bool) {
            if c { consume(x); }
        }
        fun main() {
            let db = Db { handle = 1 };
            maybe_sink(db, true);
        }
        "#,
        "is not move-clean when instantiated with a resource",
    );
}

#[test]
fn r11_closure_capturing_the_type_parameter_is_rejected() {
    // R9-for-T: a closure inside the generic body captures the T-typed parameter
    // — rejected when T is a resource.
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        fun run(fn: || i32): i32 { fn() }
        fun consume<U>(own u: U): i32 { 1 }
        fun capturing<T>(own x: T): i32 {
            run(|| consume(x))
        }
        fun main() {
            let db = Db { handle = 1 };
            let n = capturing(db);
        }
        "#,
        "is not move-clean when instantiated with a resource",
    );
}

#[test]
fn r11_resource_aggregate_type_argument_is_a_resource_instantiation() {
    // The type argument need not be a leaf resource: `Pair<Db, i32>` is a resource
    // by containment, so `use_twice<T>` at `T := Pair<Db, i32>` is re-checked and
    // its double use rejected.
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        struct Pair<A, B> { first: A, second: B }
        fun use_twice<T>(own x: T): T {
            let keep = x;
            x
        }
        fun sink(own p: Pair<Db, i32>) {}
        fun main() {
            let db = Db { handle = 1 };
            let pair = Pair { first = db, second = 2 };
            let out = use_twice(pair);
            sink(out);
        }
        "#,
        "is not move-clean when instantiated with a resource",
    );
}

// --- Dedup: the same dirty instantiation reached twice reports once -------------

#[test]
fn r11_same_dirty_instantiation_reported_once() {
    let source = r#"
        resource struct Db { handle: i32 }
        fun use_twice<T>(own x: T): T {
            let keep = x;
            x
        }
        fun sink(own d: Db) {}
        fun main() {
            let a = Db { handle = 1 };
            let b = Db { handle = 2 };
            let r1 = use_twice(a);
            let r2 = use_twice(b);
            sink(r1);
            sink(r2);
        }
        "#;
    let rejections = r11_rejections(source);
    assert_eq!(
        rejections.len(),
        1,
        "two calls, same (callee, resource-set) key — one report; got: {rejections:#?}"
    );
    // Reported at the FIRST instantiation site.
    let first_call = source.find("use_twice(a)").unwrap();
    assert_eq!(
        rejections[0].1,
        first_call..first_call + "use_twice(a)".len()
    );
}

// --- Indirect: dirt discovered through the call chain ---------------------------

#[test]
fn r11_indirect_generic_chain_is_rejected() {
    // `outer<T>` is clean itself, but passes its resource `T` on to `inner<U>`,
    // which is dirty — the worklist propagates `outer`'s instantiation to `inner`
    // and reports at the `inner(x)` call inside `outer`.
    let source = r#"
        resource struct Db { handle: i32 }
        fun inner<U>(own x: U): U {
            let keep = x;
            x
        }
        fun outer<T>(own x: T): T {
            inner(x)
        }
        fun sink(own d: Db) {}
        fun main() {
            let db = Db { handle = 1 };
            let out = outer(db);
            sink(out);
        }
        "#;
    let rejections = r11_rejections(source);
    assert_eq!(
        rejections.len(),
        1,
        "one rejection (inner); got: {rejections:#?}"
    );
    // Spanned at the indirect instantiation site — the `inner(x)` call in `outer`.
    let inner_call = source.find("inner(x)").unwrap();
    assert_eq!(
        rejections[0].1,
        inner_call..inner_call + "inner(x)".len(),
        "the indirect rejection spans the inner call inside the outer generic"
    );
}

// KNOWN GAP (destruction-impl-plan.md §2, recorded residue): the R11 move scan
// descends into DIRECT lexical closures only, so a nested closure's own T-typed
// parameter double-moved inside its body is not seen (verified: this program
// compiles today). Captures ARE caught transitively — only the nested body's
// internal moves escape. Un-ignore when the scan recurses through closure
// nesting.
#[test]
fn r11_nested_closure_internal_double_move_is_rejected() {
    assert_fails_with(
        r#"
        resource struct Db { handle: i32 }
        fun devour<T2>(own v: T2) {}
        fun g<T>(own value: T) {
            let outer = || {
                let inner = |x: T| {
                    devour(x);
                    devour(x);
                };
            };
        }
        fun main() {
            g(Db { handle = 1 });
        }
        "#,
        "moved",
    );
}

// destruction.md §5 — the `Drop` trait and its restrictions (C4 S2 chunk a).
// `Drop` (std `std::drop`) declares `fun drop(&mut self)` and is INERT this
// slice: no scope-end insertion, no lowering. The analyzer enforces two
// restrictions, keyed on the RESOLVED std `Drop` entity (never the bare name):
// it is implementable only for a resource, and its `drop` body is synchronous.

#[test]
fn drop_on_a_data_struct_is_rejected() {
    // A destructor on plain data errors, steering to add `resource` — teardown
    // without move discipline is exactly the double-close bug (§3, §11).
    assert_fails_with(
        r#"
        import std::drop::Drop;
        struct Data { x: i32 }
        impl Data with Drop {
            fun drop(&mut self) {}
        }
        fun main() {}
        "#,
        "declare it a `resource`",
    );
}

#[test]
fn drop_on_a_data_enum_is_rejected() {
    // The reject spans enums too — classification is by `type_is_resource`, not
    // the declared modifier alone.
    assert_fails_with(
        r#"
        import std::drop::Drop;
        enum Color { Red, Blue }
        impl Color with Drop {
            fun drop(&mut self) {}
        }
        fun main() {}
        "#,
        "is not a resource",
    );
}

#[test]
fn drop_runs_at_scope_end() {
    // S2b makes destruction real (destruction.md §5): the still-owned resource
    // local drops at `main`'s end, AFTER the body runs — `main-done` then
    // `DROPPED`. (This pinned the INERT S2a behavior; S2b flips it, as its
    // comment then anticipated.)
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { x: i32 }
        impl Res with Drop {
            fun drop(&mut self) { print("DROPPED"); }
        }
        fun main() {
            let r = Res { x = 1 };
            print("main-done");
            print(r.x);
        }
        "#,
        "main-done\n1\nDROPPED\n",
    );
}

#[test]
fn a_user_defined_trait_named_drop_on_data_is_accepted() {
    // The check keys on the std `Drop` entity, not the bare name: a user's own
    // `trait Drop` (std::drop never imported) is a different trait and must not
    // trip the resource restriction.
    assert_compiles(
        r#"
        trait Drop { fun drop(&mut self); }
        struct Data { x: i32 }
        impl Data with Drop {
            fun drop(&mut self) {}
        }
        fun main() {}
        "#,
    );
}

#[test]
fn a_declared_async_drop_body_is_rejected() {
    // `drop` is synchronous in v1 (§5): a declared-`async` body is rejected.
    assert_fails_with(
        r#"
        import std::drop::Drop;
        resource struct Res { x: i32 }
        impl Res with Drop {
            async fun drop(&mut self) {}
        }
        fun main() {}
        "#,
        "teardown must be synchronous",
    );
}

#[test]
fn an_awaiting_drop_body_is_rejected() {
    // The other async shape: a declared-sync body that AWAITS (calls an async
    // function) is async only by inference, and is rejected after `async_infer`.
    assert_fails_with(
        r#"
        import std::drop::Drop;
        async fun teardown() {}
        resource struct Res { x: i32 }
        impl Res with Drop {
            fun drop(&mut self) { teardown(); }
        }
        fun main() {}
        "#,
        "teardown must be synchronous",
    );
}

#[test]
fn a_context_requiring_drop_body_is_rejected() {
    // destruction.md §8: a `drop` that writes a `Signal` threads the turn as a
    // hidden context argument, but a destructor's call sites are scope exits that
    // thread none — so a context-requiring `drop` is rejected. Runs after
    // `thread_contexts` records the context-dependent functions.
    assert_fails_with(
        r#"
        import std::drop::Drop;
        import std::reactive::Signal;
        let counter = Signal::new(0);
        resource struct Bump { x: i32 }
        impl Bump with Drop {
            fun drop(&mut self) { counter.set(counter.get() + 1); }
        }
        fun main() {}
        "#,
        "teardown must be context-free",
    );
}

#[test]
fn a_resource_without_a_drop_impl_is_accepted() {
    // Containment alone is enough (§5): a resource needs no `Drop` impl to be
    // legal — its move discipline stands, and (from S2b) its fields drop.
    assert_compiles(
        r#"
        resource struct Res { x: i32 }
        fun main() {
            let r = Res { x = 1 };
        }
        "#,
    );
}

#[test]
fn drop_on_a_resource_with_contained_resource_fields_is_accepted() {
    // The realistic S4 shape: a resource that OWNS resources (a contained
    // `resource external` leaf) may carry a `Drop` impl.
    assert_compiles(
        r#"
        import std::drop::Drop;
        resource external struct Handle;
        resource struct Session { handle: Handle }
        impl Session with Drop {
            fun drop(&mut self) {}
        }
        fun main() {}
        "#,
    );
}

#[test]
fn drop_on_a_containment_inferred_resource_is_accepted() {
    // A struct that is a resource ONLY by containment (no `resource` modifier of
    // its own, but a resource field) is still a resource, so a `Drop` impl on it
    // is accepted — the check consults `type_is_resource`, which sees inference.
    assert_compiles(
        r#"
        import std::drop::Drop;
        resource external struct Handle;
        struct Wrapper { handle: Handle }
        impl Wrapper with Drop {
            fun drop(&mut self) {}
        }
        fun main() {}
        "#,
    );
}

// destruction.md §5, restriction 4: a `Drop` impl must declare exactly
// `fun drop(&mut self)` — a `&mut self` receiver, no other parameters, void
// return. S2b's targeted signature check (keyed on the std `Drop` entity; the
// general per-member conformance is backlog B29) rejects the four ways to get it
// wrong. The inserted teardown loans `self` mutably and discards the result.

#[test]
fn a_drop_impl_with_a_by_value_receiver_is_rejected() {
    // A by-value `self` could move `self` out and keep it alive (resurrection),
    // and would need to suppress its own re-drop — rejected.
    assert_fails_with(
        r#"
        import std::drop::Drop;
        resource struct Res { x: i32 }
        impl Res with Drop {
            fun drop(self) {}
        }
        fun main() {}
        "#,
        "match the receiver convention",
    );
}

#[test]
fn a_drop_impl_with_a_shared_receiver_is_rejected() {
    // `&self` cannot run the mutating teardown the destructor needs.
    assert_fails_with(
        r#"
        import std::drop::Drop;
        resource struct Res { x: i32 }
        impl Res with Drop {
            fun drop(&self) {}
        }
        fun main() {}
        "#,
        "match the receiver convention",
    );
}

#[test]
fn a_drop_impl_with_an_extra_parameter_is_rejected() {
    // The compiler calls `drop` with only the receiver; an extra parameter has
    // nothing to bind.
    assert_fails_with(
        r#"
        import std::drop::Drop;
        resource struct Res { x: i32 }
        impl Res with Drop {
            fun drop(&mut self, extra: i32) {}
        }
        fun main() {}
        "#,
        "match the declared parameter list",
    );
}

#[test]
fn a_drop_impl_with_a_non_void_return_is_rejected() {
    // Teardown produces nothing; a declared non-void return is rejected (the
    // inserted call discards the result).
    assert_fails_with(
        r#"
        import std::drop::Drop;
        resource struct Res { x: i32 }
        impl Res with Drop {
            fun drop(&mut self): i32 { 0 }
        }
        fun main() {}
        "#,
        "match the declared return type",
    );
}

#[test]
fn a_drop_impl_with_the_exact_signature_is_accepted() {
    // The one legal shape compiles.
    assert_compiles(
        r#"
        import std::drop::Drop;
        resource struct Res { x: i32 }
        impl Res with Drop {
            fun drop(&mut self) {}
        }
        fun main() {}
        "#,
    );
}

// destruction.md §5/§7 — the inserted teardown, observed through prints from the
// drop bodies (C4 S2 chunk b). Each pin runs the emitted JS and checks the drop
// ORDER. (The corpus `resource.vl` bundles the same behaviors as a byte-checked
// golden AND runs them through the interpreter equivalence gate.)

#[test]
fn drop_locals_drop_in_reverse_declaration_order() {
    // At the scope end, still-owned resource locals drop in REVERSE declaration
    // order: `b` before `a`.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(self.tag); } }
        fun main() {
            let a = Res { tag = "a" };
            let b = Res { tag = "b" };
            print("body");
            print(a.tag + b.tag);
        }
        "#,
        "body\nab\nb\na\n",
    );
}

#[test]
fn drop_body_runs_before_fields_which_drop_in_reverse() {
    // A value's own `drop` body runs BEFORE its fields, and the fields drop in
    // reverse declaration order: `owner-body`, then `second`, then `first`.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Leaf { tag: str }
        impl Leaf with Drop { fun drop(&mut self) { print(self.tag); } }
        resource struct Owner { first: Leaf, second: Leaf }
        impl Owner with Drop { fun drop(&mut self) { print("owner-body"); } }
        fun main() {
            let o = Owner { first = Leaf { tag = "first" }, second = Leaf { tag = "second" } };
            print("body");
        }
        "#,
        "owner-body\nsecond\nfirst\nbody\n",
    );
}

#[test]
fn drop_enum_payload_drops_with_the_value() {
    // An enum value drops its payload with it: `Some(Res)` at scope end drops the
    // contained `Res`.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        import std::option::Option::{ self, Some, None };
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(self.tag); } }
        fun main() {
            let opt = Some(Res { tag = "payload" });
            print("body");
        }
        "#,
        "payload\nbody\n",
    );
}

#[test]
fn containment_only_resource_drops_its_fields() {
    // A resource with NO `Drop` impl (a resource only by containment) still frees
    // its resource field at scope end.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Leaf { tag: str }
        impl Leaf with Drop { fun drop(&mut self) { print(self.tag); } }
        resource struct Bag { item: Leaf }
        fun main() {
            let bag = Bag { item = Leaf { tag = "item" } };
            print(i"body {bag.item.tag}");
        }
        "#,
        "body item\nitem\n",
    );
}

// === B113: the PLAIN-struct containment matrix, verified 2026-08-10 ============
// B109's arc observed a struct with a resource FIELD running no scope-end
// destructor and corrected a draft pin to match. The observation was PROBE
// IDIOM, not a defect (destruction.md §3): its probe wrote `impl Res { fun
// drop(own self) }` — an inherent method that happens to be spelled `drop` —
// where the language hook is `impl Res with Drop { fun drop(&mut self) }`, so
// no destructor was ever registered and none could run. The control that
// exonerates containment is `b113_an_inherent_method_named_drop_never_runs`
// below: the SAME idiom is equally silent on a bare resource with no
// containing struct at all.
//
// Every shape below tears down correctly, and each is its own pin per the
// per-case rule. The declared-`resource` twin is
// `containment_only_resource_drops_its_fields` directly above; the enum-payload
// shape is `drop_enum_payload_drops_with_the_value`; the fixed-array shape is
// `an_element_write_drops_the_old_value`'s trailing teardown.

/// The B113 matrix's shared prelude: a leaf resource that announces its
/// teardown. Each shape supplies its own containing type, which is the one
/// variable under test.
fn b113_program(body: &str) -> String {
    format!(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Leaf {{ tag: str }}
        impl Leaf with Drop {{ fun drop(&mut self) {{ print(self.tag); }} }}
        {body}
        "#
    )
}

#[test]
fn b113_a_plain_struct_with_a_resource_field_drops_it_at_scope_end() {
    // THE question B113 asks. A struct with no `resource` modifier of its own is
    // a resource by containment (destruction.md §3), so its local takes the
    // scope-end teardown and the contained leaf is destroyed.
    assert_compiles_and_runs(
        &b113_program(
            r#"
        struct Bag { item: Leaf }
        fun main() {
            let bag = Bag { item = Leaf { tag = "leaf" } };
            print(i"body {bag.item.tag}");
        }
        "#,
        ),
        "body leaf\nleaf\n",
    );
}

#[test]
fn b113_a_plain_struct_drops_its_resource_fields_in_reverse_declaration_order() {
    // The order rule (§5) does not care that the aggregate is a resource only by
    // inference: fields drop in reverse declaration order either way.
    assert_compiles_and_runs(
        &b113_program(
            r#"
        struct Bag { first: Leaf, second: Leaf }
        fun main() {
            let bag = Bag { first = Leaf { tag = "first" }, second = Leaf { tag = "second" } };
            print("body");
        }
        "#,
        ),
        "second\nfirst\nbody\n",
    );
}

#[test]
fn b113_a_plain_struct_nested_in_a_plain_struct_drops_transitively() {
    // Containment is RECURSIVE: neither aggregate is declared `resource`, and
    // the leaf is two levels down. The trailing field drops before the nested
    // aggregate's, whose own fields then drop in reverse — one order rule
    // applied at each level.
    assert_compiles_and_runs(
        &b113_program(
            r#"
        struct Inner { one: Leaf, two: Leaf }
        struct Outer { inner: Inner, tail: Leaf }
        fun main() {
            let outer = Outer {
                inner = Inner { one = Leaf { tag = "one" }, two = Leaf { tag = "two" } },
                tail = Leaf { tag = "tail" },
            };
            print("body");
        }
        "#,
        ),
        "tail\ntwo\none\nbody\n",
    );
}

#[test]
fn b113_a_plain_containment_struct_moved_into_an_own_parameter_drops_in_the_callee() {
    // R3 + §5: the move hands ownership to the callee, whose scope end runs the
    // teardown — so the leaf is destroyed BEFORE the caller's next statement.
    assert_compiles_and_runs(
        &b113_program(
            r#"
        struct Bag { item: Leaf }
        fun sink(own bag: Bag) { print(i"in-sink {bag.item.tag}"); }
        fun main() {
            let bag = Bag { item = Leaf { tag = "leaf" } };
            sink(bag);
            print("after");
        }
        "#,
        ),
        "in-sink leaf\nleaf\nafter\n",
    );
}

#[test]
fn b113_overwriting_a_plain_containment_struct_drops_the_old_value() {
    // R2 over an inferred resource: the outgoing aggregate's field is destroyed
    // at the write, the incoming one at the scope end.
    assert_compiles_and_runs(
        &b113_program(
            r#"
        struct Bag { item: Leaf }
        fun main() {
            mut bag = Bag { item = Leaf { tag = "first" } };
            print("before");
            bag = Bag { item = Leaf { tag = "second" } };
            print("after");
        }
        "#,
        ),
        "before\nfirst\nsecond\nafter\n",
    );
}

#[test]
fn b113_a_returned_plain_containment_struct_drops_at_the_callers_scope_end() {
    // R4: the return moves the aggregate out, so the callee's scope end must NOT
    // destroy it — the single teardown belongs to the caller's binding.
    assert_compiles_and_runs(
        &b113_program(
            r#"
        struct Bag { item: Leaf }
        fun make(): Bag { Bag { item = Leaf { tag = "leaf" } } }
        fun main() {
            let bag = make();
            print(i"body {bag.item.tag}");
        }
        "#,
        ),
        "body leaf\nleaf\n",
    );
}

#[test]
fn b113_a_tuple_local_drops_its_resource_members_in_reverse_order() {
    // The positional aggregate, which had only a component-WRITE pin
    // (`a_tuple_component_write_drops_the_old_value`) and no scope-end one: a
    // tuple is a value aggregate, so any resource member marks the whole.
    assert_compiles_and_runs(
        &b113_program(
            r#"
        fun main() {
            let pair = (Leaf { tag = "a" }, Leaf { tag = "b" });
            print("body");
        }
        "#,
        ),
        "b\na\nbody\n",
    );
}

#[test]
fn b113_a_plain_containment_struct_is_move_only() {
    // The other half of "containment decides": inference does not buy teardown
    // alone, it buys the whole R-rule surface. A plain struct holding a resource
    // moves on binding exactly as a declared one does.
    assert_fails_with(
        &b113_program(
            r#"
        struct Bag { item: Leaf }
        fun main() {
            let a = Bag { item = Leaf { tag = "leaf" } };
            let b = a;
            print(a.item.tag);
        }
        "#,
        ),
        "after it was moved",
    );
}

#[test]
fn b113_an_inherent_method_named_drop_never_runs() {
    // The control that diagnoses B109's observation, and the reason it is filed
    // here rather than as a defect. `impl Leaf { fun drop(own self) }` declares
    // an ordinary method whose name happens to be `drop`; the language hook is
    // the TRAIT impl (`impl Leaf with Drop`, `&mut self` — a by-value receiver
    // on the trait is rejected by `a_drop_impl_with_a_by_value_receiver_is_rejected`).
    // So no destructor is registered and nothing runs — and the subject here is
    // a BARE resource local with no containing struct at all, which is what
    // clears containment: the silence is identical with and without it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        resource struct Leaf { tag: str }
        impl Leaf { fun drop(own self) { print(i"drop {self.tag}"); } }
        fun main() {
            let leaf = Leaf { tag = "leaf" };
            print("body");
        }
        "#,
        "body\n",
    );
}

#[test]
fn drop_runs_on_early_ret() {
    // A resource owned at an early `ret` drops on the way out — and on the
    // fall-through path too (both exits run the teardown).
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(self.tag); } }
        fun run(stop: bool) {
            let r = Res { tag = "r" };
            if stop { print("stopping"); ret; }
            print(i"continuing {r.tag}");
        }
        fun main() {
            run(true);
            print("--");
            run(false);
        }
        "#,
        "stopping\nr\n--\ncontinuing r\nr\n",
    );
}

#[test]
fn drop_runs_on_jump_break_leaving_only_the_loop_scope() {
    // `jump break` drops the loop-body local it leaves (`inner`) but NOT the
    // function local (`outer`), which drops at the function's end.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(self.tag); } }
        fun main() {
            let outer = Res { tag = "outer" };
            mut rounds = 0;
            for {
                let inner = Res { tag = "inner" };
                rounds = rounds + 1;
                if rounds > 0 { jump break; }
                print(inner.tag);
            }
            print(i"after-loop {outer.tag}");
        }
        "#,
        "inner\nafter-loop outer\nouter\n",
    );
}

#[test]
fn drop_runs_on_jump_continue_each_iteration() {
    // `jump continue` drops the loop-body local it leaves, every iteration.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(self.tag); } }
        fun main() {
            mut i = 0;
            for i < 2 {
                let r = Res { tag = "iter" };
                i = i + 1;
                if i > 0 { jump continue; }
                print(i"body {r.tag}");
            }
            print("done");
        }
        "#,
        "iter\niter\ndone\n",
    );
}

#[test]
fn overwrite_drops_the_old_value_then_the_new_at_scope_end() {
    // R2: assigning onto a still-owning binding drops the OLD value first
    // (`old`), then the NEW value drops at the scope end (`new`).
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(self.tag); } }
        fun main() {
            mut r = Res { tag = "old" };
            r = Res { tag = "new" };
            print("body");
            print(r.tag);
        }
        "#,
        "old\nbody\nnew\nnew\n",
    );
}

#[test]
fn a_module_level_resource_never_drops() {
    // A module-level resource lives for the process (destruction.md §5): its
    // `drop` never runs — only `main` prints.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(self.tag); } }
        let global = Res { tag = "global" };
        fun main() {
            print("main");
        }
        "#,
        "main\n",
    );
}

#[test]
fn a_resource_owned_across_an_await_drops_at_scope_end() {
    // Owning a resource across a suspension is legal (destruction.md §5): the
    // frame owns its locals, so the resource drops at the async fn's scope end,
    // after the await.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        import std::time::sleep;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(self.tag); } }
        async fun work() {
            let r = Res { tag = "res" };
            print("before");
            await sleep(1);
            print(i"after {r.tag}");
        }
        async fun main() {
            await work();
            print("done");
        }
        "#,
        "before\nafter res\nres\ndone\n",
    );
}

#[test]
fn a_process_needing_drop_colors_its_owning_scope() {
    // destruction.md §8: a resource whose ONLY `@process` surface is its `Drop`
    // impl, owned in an otherwise-uncolored function, colors that function
    // `@process` — the compiler inserts the drop at the scope exit, and the
    // synthetic reachability edge makes coloring see it. A browser build reaching
    // the owner is therefore rejected. (Without the edge the drop is invisible to
    // reachability and this would wrongly compile.)
    assert_fails_browser_with(
        r#"
        import std::fs::write_file;
        import std::drop::Drop;
        resource struct Logger { path: str }
        impl Logger with Drop {
            fun drop(&mut self) { write_file(self.path, "closing"); }
        }
        fun use_it() {
            let logger = Logger { path = "log.txt" };
            print_marker();
        }
        fun print_marker() {}
        fun main() {
            use_it();
        }
        "#,
        "requires the `process` layer of `std`",
    );
}

#[test]
fn a_platform_free_drop_adds_no_coloring() {
    // The inverse control: a context-free, platform-free `drop` (just `print`,
    // which runs on every host) adds NO coloring — the owning function stays
    // neutral, so a browser build compiles cleanly.
    assert_compiles_browser(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(self.tag); } }
        fun use_it() {
            let r = Res { tag = "r" };
            print("used");
        }
        fun main() {
            use_it();
        }
        "#,
    );
}

#[test]
fn a_drop_sink_call_colors_its_owning_function() {
    // destruction.md §8 (S3): a resource whose only `@process` surface is its
    // `Drop`, destroyed ONLY via the `drop(x)` SINK (not a scope-end drop), still
    // colors its owning function `@process` — the sink call lowers transformer-side
    // to the `__drop` helper, invisible to reachability, so a synthetic edge from
    // the function to the destructor is seeded from the sink argument. A browser
    // build reaching the owner is rejected.
    assert_fails_browser_with(
        r#"
        import std::fs::write_file;
        import std::drop::{ Drop, drop };
        resource struct Logger { path: str }
        impl Logger with Drop {
            fun drop(&mut self) { write_file(self.path, "closing"); }
        }
        fun use_it() {
            let logger = Logger { path = "log.txt" };
            print_marker();
            drop(logger);
        }
        fun print_marker() {}
        fun main() {
            use_it();
        }
        "#,
        "requires the `process` layer of `std`",
    );
}

#[test]
fn a_platform_free_drop_sink_call_adds_no_coloring() {
    // The sink-call inverse control: a platform-free `drop(x)` (just `print`) adds
    // no coloring, so a browser build compiles cleanly.
    assert_compiles_browser(
        r#"
        import std::print;
        import std::drop::{ Drop, drop };
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(self.tag); } }
        fun use_it() {
            let r = Res { tag = "r" };
            print("used");
            drop(r);
        }
        fun main() {
            use_it();
        }
        "#,
    );
}

#[test]
fn a_drop_runs_synchronously_at_the_scope_exit() {
    // §8 Turns: drops are ordinary statements at scope exits — they run
    // synchronously, in program order, so a nested scope's drop precedes code
    // after that scope. This is the property the §8 turn interaction rests on (a
    // signal-writing drop joins the ambient wave BECAUSE the write is a plain
    // synchronous statement inside the turn). The full turn observation is NOT
    // pinned here: a signal write threads the ambient turn as a hidden CONTEXT
    // argument, and the generated `__drop` helper does not forward it — so a
    // context-requiring drop body is unsupported in this slice (see the report).
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(self.tag); } }
        fun main() {
            {
                let r = Res { tag = "dropped" };
                print(i"in-scope {r.tag}");
            }
            print("after-scope");
        }
        "#,
        "in-scope dropped\ndropped\nafter-scope\n",
    );
}

// ============================================================================
// C4 S3 — `Option.take`/`replace`, the `drop<T>(own)` sink, own-parameter drops,
// and the generic exactly-once rule (destruction.md §5/§6, impl-plan §4).
// ============================================================================

// --- `Option.take` / `replace` (destruction.md §6) --------------------------

#[test]
fn option_take_on_data_leaves_none_and_yields_the_value() {
    // `take` reads the slot, writes `None` back in place (the caller's binding
    // sees it), and returns the old contents. Data works exactly like a resource.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            mut opt = Some(5);
            let taken = opt.take();
            print(i"taken={taken.unwrap_or(0)} opt_is_none={opt.is_none()}");
        }
        "#,
        "taken=5 opt_is_none=true\n",
    );
}

#[test]
fn option_take_on_none_stays_none() {
    // Taking from `None` yields `None` and leaves `None` — the idempotent edge.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            mut opt: Option<i32> = None;
            let taken = opt.take();
            print(i"taken_none={taken.is_none()} opt_none={opt.is_none()}");
        }
        "#,
        "taken_none=true opt_none=true\n",
    );
}

#[test]
fn option_replace_on_data_returns_the_old_and_installs_the_new() {
    // `replace` puts the new value in and returns the old — `Some(old)` here.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            mut opt = Some(1);
            let old = opt.replace(2);
            print(i"old={old.unwrap_or(0)} now={opt.unwrap_or(0)}");
        }
        "#,
        "old=1 now=2\n",
    );
}

#[test]
fn option_replace_on_none_returns_none() {
    // Replacing into `None` returns `None` and installs `Some(new)` — the edge.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            mut opt: Option<i32> = None;
            let old = opt.replace(7);
            print(i"old_none={old.is_none()} now={opt.unwrap_or(0)}");
        }
        "#,
        "old_none=true now=7\n",
    );
}

#[test]
fn option_take_on_a_resource_moves_the_payload_out() {
    // The sanctioned partial move (destruction.md §6): `take` moves the resource
    // payload into its new owner (`moved`), which drops it at ITS scope end; the
    // slot (`opt`, now `None`) drops nothing. Reverse-order drop is visible.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            mut opt: Option<Res> = Some(Res { tag = "r" });
            {
                let moved = opt.take();
                print("in-block");
            }
            print("after-block");
        }
        "#,
        "drop r\nin-block\nafter-block\n",
    );
}

#[test]
fn option_replace_returns_the_old_resource_for_the_caller_to_own() {
    // `replace` hands the old resource back to the caller; the returned value and
    // the new one both drop at the caller's scope end, in reverse declaration
    // order (`previous` then `slot`).
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            mut slot: Option<Res> = Some(Res { tag = "old" });
            let previous = slot.replace(Res { tag = "new" });
            print("replaced");
        }
        "#,
        "drop old\ndrop new\nreplaced\n",
    );
}

// --- B153: `replace` KEEPS what it is handed, so the position is `own` -------
//
// `external fun replace(&mut self, value: T)` declared the new value BARE — a
// loan under R3 — and then stored it. A loan changes no ownership, so the
// caller's binding stayed live and stayed readable, and the value was destroyed
// twice: once by the slot, once by the caller. The honest declaration is `own
// value: T`, which is also what let C11's temporary predicate go back to its
// ruled width (only `own` moves).
//
// The sweep for the same shape across std ("a bare parameter the callee
// stores") found three more and cleared two of them: `Shared::new(value: T)`
// stores, but `r10_refuses_an_inferred_shared_of_a_resource` above shows no
// resource can reach it; `Context::run(self, value: T, body)` binds its value
// for the dynamic extent of `body` and hands it back, so a loan is the honest
// reading there. The third, `NativeMap::insert`, was genuinely broken (B154)
// and is closed below — not by moving its declaration to `own`, but by putting
// the internal head in R10's rejecting list, so no resource ever reaches it.

#[test]
fn option_replace_moves_the_new_value_in_rather_than_loaning_it() {
    // B153's miscompile: `r` moves into the slot, so the slot is the one owner
    // and the value is destroyed exactly once. Declared bare this printed
    // `drop r` TWICE on an accepted program.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            mut slot: Option<Res> = None;
            let r = Res { tag = "r" };
            let old = slot.replace(r);
            print("after");
        }
        "#,
        "drop r\nafter\n",
    );
}

#[test]
fn option_replace_rejects_a_read_of_the_value_it_was_handed() {
    // The static half of the same defect: a loan leaves the name readable, so
    // the move checker had nothing to say about a value the slot now owns.
    // `own` restores the single-owner rule `List::push` has always enforced.
    assert_fails_with(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            mut slot: Option<Res> = None;
            let r = Res { tag = "r" };
            let old = slot.replace(r);
            print(i"still readable: {r.tag}");
        }
        "#,
        "use of `r` after it was moved",
    );
}

#[test]
fn r10_refuses_a_native_map_of_a_resource() {
    // B154, closed by the ruled fix. `NativeMap::insert` stores a bare
    // `value: V` exactly as `replace` did, and the measured consequence was
    // worse: the caller destroyed the value at the insert statement while the
    // table went on holding it (`drop r` then `after`, host-side
    // use-after-free).
    //
    // The declaration fix (`own value: V`) was NOT the right one: `Map`/`Set`
    // are built on `NativeMap` and pass a value they already own, so `own`
    // buys a redundant `__clone` on every insert in the language and moves 12
    // corpus goldens — a language-wide cost to close a hazard reachable only by
    // importing `std::native_map` directly, which the module documents as not
    // public surface. The zero-cost fix is R10: `List`/`Map`/`Set`/`Shared`/
    // `Context`/`Promise`/`Task` all refuse a resource argument, and the
    // internal head simply was not in the list. It is now, so no resource ever
    // reaches `insert` and the bare declaration is honest again.
    assert_fails_with(
        r#"
        import std::print;
        import std::native_map::NativeMap;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            mut table: NativeMap<Res> = NativeMap::new();
            print("built");
        }
        "#,
        "`NativeMap` cannot hold the resource `Res`",
    );
}

#[test]
fn r10_refuses_the_measured_native_map_use_after_free() {
    // B154's actual program, the one that measured the defect: the insert
    // stores the resource in the table AND the widened temporary predicate
    // destroys it at that statement, so the table held a freed value (it
    // printed `drop r` then `after` and ran on). R10 now refuses the type
    // before the insert is ever reached.
    assert_fails_with(
        r#"
        import std::print;
        import std::native_map::NativeMap;
        import std::hash::Hashable;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            mut table: NativeMap<Res> = NativeMap::new();
            let r = Res { tag = "r" };
            table.insert("k".hash(), r);
            print("after");
        }
        "#,
        "`NativeMap` cannot hold the resource `Res`",
    );
}

#[test]
fn r10_admits_a_native_map_of_a_non_resource() {
    // The green negative the head extension has to keep: `NativeMap` is only
    // refused AT a resource. The raw layer is still the thing `Map`/`Set` are
    // built on, and every one of their inserts goes through this.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::native_map::NativeMap;
        import std::hash::Hashable;
        fun main() {
            mut table: NativeMap<str> = NativeMap::new();
            table.insert("k".hash(), "v");
            print(i"{table.len()}");
        }
        "#,
        "1\n",
    );
}

#[test]
fn native_map_insert_loans_its_hash_key() {
    // `insert(&mut self, key: Hash, value: V)` declares the KEY bare, and that
    // stays honest under B154's fix where `value` needed the head: a `Hash` is
    // a non-generic external struct nothing can declare `resource`, so no
    // destructor ever hangs on the key position and a loan changes nothing.
    // The key is read after the insert to prove the caller still owns it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::native_map::NativeMap;
        import std::hash::{ Hash, Hashable };
        fun main() {
            mut table: NativeMap<str> = NativeMap::new();
            let key: Hash = "k".hash();
            table.insert(key, "v");
            print(i"{table.contains_key(key)}");
        }
        "#,
        "true\n",
    );
}

// --- B154's two-head shape: one mistake, one diagnostic ----------------------
//
// `Map<K, V>` stores a `NativeMap<(K, V)>` and `Set<T>` a `NativeMap<T>`, so
// with the internal head in R10's rejecting list a `Map<str, Res>` offends at
// BOTH heads. The public one is the mistake — it is the type the user wrote and
// the only one they can fix — and the inner one is that refusal a layer in.
// Kept to one report by the general rule, not by naming the type: R11's
// in-body container check stands down when the callee's own instantiated
// SIGNATURE is refused and the caller has already been told (E98's shape).

#[test]
fn r10_map_of_a_resource_reports_once() {
    assert_fails_once_with(
        r#"
        import std::map::Map;
        resource struct Db { handle: i32 }
        fun sink(table: Map<str, Db>) {}
        fun main() {}
        "#,
        "cannot hold the resource",
    );
}

#[test]
fn r10_set_of_a_resource_reports_once() {
    assert_fails_once_with(
        r#"
        import std::set::Set;
        resource struct Db { handle: i32 }
        fun sink(items: Set<Db>) {}
        fun main() {}
        "#,
        "cannot hold the resource",
    );
}

#[test]
fn r10_map_construction_never_reports_the_native_map_inside_it() {
    // The constructing form, where `Map::new()`'s body binds the offending
    // `NativeMap<(str, Res)>` under the substitution. The user's diagnostic is
    // about `Map`; the storage inside it is not a second thing to fix.
    assert_fails_without(
        r#"
        import std::print;
        import std::map::Map;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            mut table: Map<str, Res> = Map::new();
            print("built");
        }
        "#,
        "`NativeMap` cannot hold",
    );
}

#[test]
fn r10_set_construction_never_reports_the_native_map_inside_it() {
    assert_fails_without(
        r#"
        import std::print;
        import std::set::Set;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            mut items: Set<Res> = Set::new();
            print("built");
        }
        "#,
        "`NativeMap` cannot hold",
    );
}

#[test]
fn r11_still_reports_a_container_the_caller_cannot_see() {
    // The other half of the claim: standing down on a refused signature must
    // not blind R11's in-body check, whose whole subject is the container a
    // generic body builds out of its own `T` and never hands back. `stash`'s
    // signature holds no container at all, so it still reports.
    assert_fails_with(
        r#"
        resource struct Guard { handle: i32 }
        fun stash<type T>(own value: T) { let items = [value]; }
        fun main() {
            stash(Guard { handle = 1 });
        }
        "#,
        "`List` cannot hold the resource `Guard`",
    );
}

#[test]
fn r11_reports_an_independent_body_container_beside_a_refused_signature() {
    // E104: the stand-down is asked PER OFFENDING TYPE, not once for the whole
    // instantiation. `Map<str, A>` is refused at the signature and the caller
    // was told at the type it wrote, so nothing more is owed for it; the body's
    // `List<Other>` is built out of a DIFFERENT parameter, is a type no caller
    // holds, and is nobody's consequence — so it still reports. Both heads,
    // once each.
    let source = r#"
        import std::map::Map;
        resource struct Guard { handle: i32 }
        resource struct Other { handle: i32 }
        fun two<type A, type B>(a: Map<str, A>, own b: B) { let items = [b]; }
        fun caller(table: Map<str, Guard>) {
            two(table, Other { handle = 2 });
        }
        fun main() {}
        "#;
    assert_fails_once_with(source, "`Map` cannot hold the resource `Guard`");
    assert_fails_once_with(source, "`List` cannot hold the resource `Other`");
}

#[test]
fn r11_stands_down_on_a_body_container_built_from_the_refused_parameter() {
    // The mixed case, the other side of the same predicate: one parameter is
    // refused at the signature AND that same parameter's container is built in
    // the body. The body's `List<A>` is a structure the caller never holds, so
    // no dedup reaches it — only the stand-down does, and it should, because
    // `A` is exactly the parameter the reported `Map<str, Guard>` covers. One
    // decision the caller made, one report.
    assert_fails_once_with(
        r#"
        import std::map::Map;
        resource struct Guard { handle: i32 }
        fun wrap<type A>(own a: A, table: Map<str, A>) { let items = [a]; }
        fun caller(table: Map<str, Guard>) {
            wrap(Guard { handle = 1 }, table);
        }
        fun main() {}
        "#,
        "cannot hold the resource",
    );
}

#[test]
fn option_take_under_a_live_view_is_rejected() {
    // `take` is an invalidating mutation, so rule 4 / E2 fences it exactly as it
    // fences any geometry-bumping write: taking through `opt` while a `&mut` view
    // into `opt` is live is rejected. Pinned to prove take opens NO new hole.
    assert_fails_with(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            mut opt: Option<i32> = Some(5);
            let view = &mut opt;
            let taken = opt.take();
            print(i"{view.is_some()}");
        }
        "#,
        "while a view into it is live",
    );
}

// --- The `drop<T>(own)` sink (destruction.md §6) ----------------------------

#[test]
fn drop_of_a_resource_tears_down_immediately() {
    // `drop(db)` destroys at its immediate site — BEFORE the following statement
    // — instead of waiting for the owner's scope end. The sink call is rewritten
    // to the resource's destructor; `db` then drops nowhere else (no double-drop).
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::{ Drop, drop };
        resource struct Db { tag: str }
        impl Db with Drop { fun drop(&mut self) { print(i"close {self.tag}"); } }
        fun main() {
            let db = Db { tag = "one" };
            print("before");
            drop(db);
            print("after");
        }
        "#,
        "before\nclose one\nafter\n",
    );
}

#[test]
fn drop_of_data_is_a_no_op() {
    // On data `drop` is a no-op that still evaluates its argument for effects (no
    // destructor exists) — the sink is ordinary std surface, useful for both.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::drop;
        fun main() {
            let n = 5;
            drop(n);
            print("ok");
        }
        "#,
        "ok\n",
    );
}

// --- B68: a VALUE argument to the sink (affine-moves.md §9.4) ---------------
//
// `drop` takes its argument `own`, so a non-place argument — a call result, a
// construction — is owned by the `drop` expression itself and must be destroyed
// there. Nothing else can destroy it: the value is never bound, so no scope-end
// teardown and no overwrite drop can reach it. The rewrite therefore has to
// resolve the type of ANY expression in argument position, not just the forms
// that happen to carry a stored type.

#[test]
fn b68_drop_of_a_call_result_destroys_it() {
    // The §9.4 repro. `drop(identity(Db{..}))` must destroy exactly what
    // `let bound = identity(Db{..}); drop(bound)` destroys — the binding is not
    // what makes the value droppable, the `own` sink is.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::{ Drop, drop };
        resource struct Db { tag: str }
        impl Db with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun identity(own value: Db): Db { value }
        fun main() {
            let bound = identity(Db { tag = "bound" });
            drop(bound);
            print("--");
            drop(identity(Db { tag = "direct" }));
            print("done");
        }
        "#,
        "drop bound\n--\ndrop direct\ndone\n",
    );
}

#[test]
fn b68_drop_of_a_construction_destroys_it() {
    // The other non-place form: a construction handed straight to the sink. This
    // one already worked (a struct initializer records its own type), and stays
    // pinned so the B68 widening cannot regress it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::{ Drop, drop };
        resource struct Db { tag: str }
        impl Db with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            drop(Db { tag = "literal" });
            print("done");
        }
        "#,
        "drop literal\ndone\n",
    );
}

#[test]
fn b68_drop_of_a_method_call_result_destroys_it() {
    // The receiver-substituted form: the sink argument's type comes from a
    // method's declared return type, which is only known through the call.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::{ Drop, drop };
        resource struct Db { tag: str }
        impl Db with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        struct Factory { tag: str }
        impl Factory { fun open(self): Db { Db { tag = self.tag } } }
        fun main() {
            let factory = Factory { tag = "made" };
            drop(factory.open());
            print("done");
        }
        "#,
        "drop made\ndone\n",
    );
}

#[test]
fn b68_drop_of_a_nested_call_result_destroys_it() {
    // Nesting is not a new case, but it is the one that proves the rewrite reads
    // the OUTER call's result type rather than pattern-matching one call shape.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::{ Drop, drop };
        resource struct Db { tag: str }
        impl Db with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun identity(own value: Db): Db { value }
        fun main() {
            drop(identity(identity(Db { tag = "nested" })));
            print("done");
        }
        "#,
        "drop nested\ndone\n",
    );
}

#[test]
fn b68_drop_of_a_data_call_result_is_a_no_op() {
    // The data control: an i32-returning call in argument position stays the
    // no-op consume that still evaluates its argument for effects. Widening the
    // type query must not conjure a destructor where there is none.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::drop;
        fun sum(a: i32, b: i32): i32 { print("called"); a + b }
        fun main() {
            drop(sum(1, 2));
            print("ok");
        }
        "#,
        "called\nok\n",
    );
}

#[test]
fn b68_a_generic_forwarding_a_call_result_to_the_sink_is_rejected_at_a_resource() {
    // The B66/R11 interplay. `drop(t)` in an erased generic body is dirt at a
    // resource instantiation (`a_generic_forwarding_own_t_to_the_drop_sink_is_
    // rejected_at_a_resource`), for the reason that the erased body has no
    // concrete destructor. Routing the same `T` through a call first changes
    // nothing about that, so the call-result form joins the place form rather
    // than slipping past the check untyped.
    assert_fails_spanning(
        r#"
        import std::print;
        import std::drop::{ Drop, drop };
        resource struct Db { tag: str }
        impl Db with Drop { fun drop(&mut self) { print(self.tag); } }
        fun identity<T>(own value: T): T { value }
        fun consume<T>(own x: T) { drop(identity(x)); }
        fun main() {
            let db = Db { tag = "one" };
            consume(db);
        }
        "#,
        "consume(db)",
        "pass a resource to `drop<T>`, whose erased body has no concrete destructor",
    );
}

#[test]
fn b68_a_generic_forwarding_a_call_result_to_the_sink_is_accepted_at_data() {
    // The control for the pin above: the same generic instantiated only at data
    // stays accepted — `drop` on data is the correct no-op consume.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::drop;
        fun identity<T>(own value: T): T { value }
        fun consume<T>(own x: T) { drop(identity(x)); }
        fun main() {
            consume(5);
            print("ok");
        }
        "#,
        "ok\n",
    );
}

#[test]
fn the_conditional_teardown_idiom_tears_down_in_both_arms() {
    // The idiom R7 pushes toward (destruction.md §6): `match opt.take() { Some(let
    // c) => drop(c), None => {} }`. `take` moves the payload out; `drop(c)` tears
    // it down in the `Some` arm; the `None` arm tears down nothing. Both exercised.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        import std::drop::{ Drop, drop };
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            mut full: Option<Res> = Some(Res { tag = "cond" });
            match full.take() {
                Some(let c) => drop(c),
                None => {}
            }
            print("after-some");
            mut empty: Option<Res> = None;
            match empty.take() {
                Some(let c) => drop(c),
                None => print("none-arm"),
            }
            print("after-none");
        }
        "#,
        "drop cond\nafter-some\nnone-arm\nafter-none\n",
    );
}

// --- Concrete own-parameter drops (destruction.md §6) -----------------------

#[test]
fn a_concrete_own_resource_parameter_drops_at_the_callee_scope_end() {
    // An `own` parameter of concrete resource type not moved out drops at the
    // callee's scope end (S3 closes S2b's recorded leak) — BEFORE the caller's
    // later statement runs.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun sink(own r: Res) {
            print(i"in-sink {r.tag}");
        }
        fun main() {
            sink(Res { tag = "x" });
            print("after-sink");
        }
        "#,
        "in-sink x\ndrop x\nafter-sink\n",
    );
}

#[test]
fn two_own_resource_parameters_drop_in_reverse_declaration_order() {
    // Multiple owned parameters drop in reverse declaration order at the scope
    // end, like locals — the ordering-sensitive edge (`b` before `a`).
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun two(own a: Res, own b: Res) {
            print(i"in-two {a.tag}{b.tag}");
        }
        fun main() {
            two(Res { tag = "a" }, Res { tag = "b" });
            print("after");
        }
        "#,
        "in-two ab\ndrop b\ndrop a\nafter\n",
    );
}

#[test]
fn an_own_parameter_moved_out_on_every_path_drops_nowhere() {
    // A parameter returned out of the function (R7: moved on every path) drops
    // NOWHERE in the callee — the caller owns it and drops it once. No double-drop.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::{ Drop, drop };
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun passthrough(own r: Res): Res {
            print("in-passthrough");
            r
        }
        fun main() {
            let back = passthrough(Res { tag = "y" });
            print("after-passthrough");
            drop(back);
            print("done");
        }
        "#,
        "in-passthrough\nafter-passthrough\ndrop y\ndone\n",
    );
}

#[test]
fn an_async_own_resource_parameter_drops_after_the_await_at_scope_end() {
    // An `own` resource parameter of an ASYNC function drops at the function's
    // scope end — AFTER the `await` (destruction.md §5: owning a resource across a
    // suspension is legal). `wrap_own_param_drops` wraps the whole async body in
    // one `try`/`finally`, and JS `finally` runs after every `await` in the `try`
    // completes, so the drop lands after "after-await" and before the caller's
    // later statement. Finally placement is not subtle: the wrap is outside all
    // awaits. (Async — node only, not the interpreter subset.)
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;

        [extern("node:timers/promises", "setTimeout")]
        async external fun sleep(ms: i32): void;

        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }

        fun work(own r: Res) {
            print(i"before-await {r.tag}");
            sleep(0);
            print(i"after-await {r.tag}");
        }
        fun main() {
            work(Res { tag = "x" });
            print("done");
        }
        "#,
        "before-await x\nafter-await x\ndrop x\ndone\n",
    );
}

// --- The generic exactly-once rule (R11 tightening, destruction.md §6) -------

#[test]
fn a_generic_own_t_never_moved_out_is_rejected_at_a_resource_instantiation() {
    // Under a resource instantiation an `own T` parameter must be moved on EVERY
    // path — a shared generic body cannot drop it (it is emitted erased across
    // instantiations, and drop flags are ratified out). Zero-move is the leak the
    // body cannot close, rejected AT the instantiation site with the steer.
    assert_fails_spanning(
        r#"
        import std::print;
        resource struct Db { tag: str }
        fun leak<T>(own x: T) {}
        fun main() {
            let db = Db { tag = "one" };
            leak(db);
        }
        "#,
        "leak(db)",
        "move it out on every path, or take a concrete type",
    );
}

#[test]
fn the_same_generic_own_t_zero_move_at_a_data_type_is_accepted() {
    // The SAME zero-move generic is fine at a data instantiation: data copies, so
    // nothing leaks and no instantiation is enqueued. Only resources tighten.
    assert_compiles(
        r#"
        import std::print;
        fun leak<T>(own x: T) {}
        fun main() {
            leak(5);
            print("ok");
        }
        "#,
    );
}

#[test]
fn the_drop_sink_itself_is_accepted_at_a_resource() {
    // `drop<T>(own value)` zero-moves `value` — yet it is EXEMPT from the
    // exactly-once rule: it IS the drop site (its call rewrites to the
    // destructor), special-known like the `Shared` intrinsics. `drop(db)` on a
    // resource compiles.
    assert_compiles(
        r#"
        import std::print;
        import std::drop::{ Drop, drop };
        resource struct Db { tag: str }
        impl Db with Drop { fun drop(&mut self) { print(self.tag); } }
        fun main() {
            let db = Db { tag = "one" };
            drop(db);
        }
        "#,
    );
}

#[test]
fn a_generic_own_t_moved_out_by_return_is_accepted_at_a_resource() {
    // The canonical clean shape: an `own T` returned out (moved on the only path)
    // is accepted at a resource — the caller receives and owns it.
    assert_compiles(
        r#"
        import std::print;
        import std::drop::{ Drop, drop };
        resource struct Db { tag: str }
        impl Db with Drop { fun drop(&mut self) { print(self.tag); } }
        fun identity<T>(own x: T): T { x }
        fun main() {
            let db = Db { tag = "one" };
            let out = identity(db);
            drop(out);
        }
        "#,
    );
}

#[test]
fn a_generic_own_t_moved_out_on_every_branch_is_accepted() {
    // Moved out on EVERY path through a branch (R7): both arms return `x` to the
    // caller — accepted (not a zero-move; the caller then owns and drops it).
    assert_compiles(
        r#"
        import std::print;
        import std::drop::{ Drop, drop };
        resource struct Db { tag: str }
        impl Db with Drop { fun drop(&mut self) { print(self.tag); } }
        fun choose<T>(own x: T, flag: bool): T {
            if flag { x } else { x }
        }
        fun main() {
            let db = Db { tag = "one" };
            drop(choose(db, true));
        }
        "#,
    );
}

#[test]
fn two_own_generics_moved_on_different_branches_is_not_every_path() {
    // The rule says "moved out on EVERY path". `first` is moved in the `then`
    // arm, `second` in the `else`, so `pick(true, Some(a), Some(b))` returned
    // `a` and destroyed NOTHING — `b` was a resource simply never torn down.
    //
    // CLOSED by B67, and the diagnosis in the original filing was half wrong, so
    // it is corrected here. The merge was NOT the defect: `plan_branches`'
    // intersection is exact GIVEN R7, which makes ownership single-valued at
    // every program point. What was broken is that R7's reach had been cut short
    // by an over-broad R4 exemption in `scan_move_branches` — each arm's tail
    // place was stripped from the cross-arm comparison, which correctly permits
    // `if flag { x } else { x }` and wrongly permits this. Removing the
    // exemption restores R7 and the existing merge becomes correct again; no
    // union merge and no second walk were needed (`affine-moves.md` §9.3).
    //
    // The `is`-refinement the filing demanded is real and is what replaced the
    // exemption: `or_else`'s `if self is Some(_) { self } else { fn() }` leaves
    // `self` un-moved on the else path, which is SOUND because a `self` reaching
    // it is `None` and has no payload (`b63_or_else_at_a_resource_instantiation`
    // is the load-bearing guard).
    //
    // The diagnostic is R7's OWN, reused verbatim — a branch-divergent move IS a
    // conditional move, not a new rule, and R7's precedent is an error rather
    // than a synthesized drop ("there are no runtime drop flags in v1"). This
    // pin's message fragment was a guess by the filing arc that the R11 leak
    // diagnostic would fire; the ruling says otherwise, so it names the R7 one.
    assert_fails_spanning(
        r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        resource struct Res { tag: str }
        fun pick<T>(flag: bool, own first: Option<T>, own second: Option<T>): Option<T> {
            if flag { first } else { second }
        }
        fun main() {
            let kept = pick(true, Some(Res { tag = "kept" }), Some(Res { tag = "tossed" }));
        }
        "#,
        "pick(true, Some(Res { tag = \"kept\" }), Some(Res { tag = \"tossed\" }))",
        "a resource-typed value is moved on one path but not all",
    );
}

#[test]
fn b67_the_concrete_twin_of_pick_is_rejected_too() {
    // The bug was never generic-only, and fixing it in `scan_move_branches`
    // rather than in the R11 leak check is what closes both. At a CONCRETE
    // resource the same shape leaked the same way: `plan_branches` saw both
    // parameters moved, planned no teardown, and `second` was never destroyed.
    assert_fails_spanning(
        r#"
        import std::print;
        import std::drop::{ Drop, drop };
        import std::option::Option::{ self, Some, None };
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun pick(flag: bool, own first: Option<Res>, own second: Option<Res>): Option<Res> {
            if flag { first } else { second }
        }
        fun main() {
            let kept = pick(true, Some(Res { tag = "kept" }), Some(Res { tag = "tossed" }));
        }
        "#,
        "if flag { first } else { second }",
        "is moved on one path through this branch but not another",
    );
}

#[test]
fn b67_both_divergent_parameters_are_reported() {
    // Two values, each leaking on a different path — two diagnostics, and that
    // is not a B5 violation: fixing `first` does not fix `second`. Pinned so a
    // later "report once per branch" tidy-up cannot silently halve the report.
    let source = r#"
        import std::print;
        import std::option::Option::{ self, Some, None };
        resource struct Res { tag: str }
        fun pick<T>(flag: bool, own first: Option<T>, own second: Option<T>): Option<T> {
            if flag { first } else { second }
        }
        fun main() {
            let kept = pick(true, Some(Res { tag = "kept" }), Some(Res { tag = "tossed" }));
        }
        "#;
    let rejections = r11_rejections(source);
    assert_eq!(
        rejections.len(),
        2,
        "one per divergently-moved parameter; got: {rejections:#?}"
    );
}

#[test]
fn b67_the_same_binding_returned_from_every_branch_stays_accepted() {
    // The case the removed R4 exemption was written for, and the reason removing
    // it is safe: when EVERY arm moves the tail binding, R7's counts already
    // match and no exemption is needed. This is `pick`'s shape with one value
    // instead of two, and it must stay legal.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::{ Drop, drop };
        resource struct Db { tag: str }
        impl Db with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun choose<T>(own x: T, flag: bool): T {
            if flag { x } else { x }
        }
        fun main() {
            // Bound, then dropped: `drop(choose(..))` on the call result direct
            // destroys nothing, which is a SEPARATE pre-existing hole (it
            // reproduces with no branch at all — `drop(identity(Db{..}))`), not
            // B67's, and not what this pin is about.
            let out = choose(Db { tag = "one" }, true);
            drop(out);
            print("end");
        }
        "#,
        "drop one\nend\n",
    );
}

#[test]
fn b67_an_is_refined_branch_may_leave_the_none_side_un_moved() {
    // The `is`-refinement in user code, not just in std: `held` is moved on the
    // `Some` path and left alone on the other — sound, because the other path is
    // reached only when `held is Some(_)` is FALSE, so `held` is `None` and
    // carries nothing to destroy. This is `or_else`'s shape, written by hand.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::{ Drop, drop };
        import std::option::Option::{ self, Some, None };
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun pick_or<T>(own held: Option<T>, own fallback: || Option<T>): Option<T> {
            if held is Some(_) { held } else { fallback() }
        }
        fun main() {
            let empty: Option<Res> = None;
            let filled = pick_or(empty, || Some(Res { tag = "made" }));
            print("end");
        }
        "#,
        "drop made\nend\n",
    );
}

#[test]
fn b67_the_refinement_does_not_excuse_a_payload_carrying_complement() {
    // The refinement's boundary, and the reason it is a payload question rather
    // than a "there is an `is` test here" question. `Pair` has no data-less
    // variant, so the complement of `First(_)` still carries a resource — the
    // else arm is NOT exempt and the divergent move is still an error. A
    // refinement that keyed off the `is` alone would wrongly accept this.
    assert_fails_spanning(
        r#"
        import std::print;
        import std::drop::{ Drop, drop };
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        enum Pair<T> { First(T), Second(T) }
        fun take_one<T>(own slot: Pair<T>, own spare: Pair<T>): Pair<T> {
            if slot is Pair::First(_) { slot } else { spare }
        }
        fun main() {
            let out = take_one(Pair::First(Res { tag = "a" }), Pair::Second(Res { tag = "b" }));
        }
        "#,
        "take_one(Pair::First(Res { tag = \"a\" }), Pair::Second(Res { tag = \"b\" }))",
        "a resource-typed value is moved on one path but not all",
    );
}

#[test]
fn b67_a_loop_divergent_move_is_still_r8_not_r7() {
    // The loop variant. R8 owns a move of an outer binding from inside a
    // repeatable interior, and it fires SYNTACTICALLY on the first pass — so
    // B67's branch reasoning never gets to reinterpret it, and the diagnostic
    // stays the loop one. Pinned because the branch inside the loop body is
    // exactly the shape B67 now inspects.
    assert_fails_with(
        r#"
        import std::print;
        import std::drop::{ Drop, drop };
        import std::option::Option::{ self, Some, None };
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun sink(own r: Option<Res>) {}
        fun main() {
            let held: Option<Res> = Some(Res { tag = "looped" });
            mut n = 0;
            for n < 3 {
                if n == 1 {
                    sink(held);
                }
                n = n + 1;
            }
        }
        "#,
        "is declared outside this loop and moved inside it",
    );
}

#[test]
fn a_generic_forwarding_own_t_to_the_drop_sink_is_rejected_at_a_resource() {
    // A free generic with an inferred type argument is emitted ONCE (erased), so
    // `drop(x)` on a `T`-typed value has no concrete destructor and would leak.
    // The exactly-once check treats `x` as moved (it IS passed to the `own` sink),
    // so R11 catches this specifically: passing a resource-instantiation's own type
    // parameter to `drop<T>` is dirt AT the instantiation (destruction.md §6, the
    // 2026-07-19 ruling). Spanned at the instantiation, with the steer.
    assert_fails_spanning(
        r#"
        import std::print;
        import std::drop::{ Drop, drop };
        resource struct Db { tag: str }
        impl Db with Drop { fun drop(&mut self) { print(self.tag); } }
        fun consume<T>(own x: T) { drop(x); }
        fun main() {
            let db = Db { tag = "one" };
            consume(db);
        }
        "#,
        "consume(db)",
        "pass a resource to `drop<T>`, whose erased body has no concrete destructor",
    );
}

#[test]
fn a_generic_forwarding_own_t_to_the_drop_sink_is_accepted_at_data() {
    // The control: the SAME generic instantiated only at a data type stays accepted
    // — `drop(x)` on data is the correct no-op consume. No resource instantiation is
    // enqueued, so the R11 drop-forwarding check never runs.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::drop;
        fun consume<T>(own x: T) { drop(x); }
        fun main() {
            consume(5);
            print("ok");
        }
        "#,
        "ok\n",
    );
}

#[test]
fn a_concrete_own_parameter_dropped_via_the_sink_is_destroyed() {
    // A concrete `own` parameter destroyed via `drop(d)` (the parameter used in
    // expression position is an `Expr::Local` of a parameter id) — the rewrite
    // resolves the parameter's type and lowers to the destructor, BEFORE the
    // following statement. (Guards a latent no-op: a bare `drop(param)` used to
    // read as untyped and silently leak.)
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::{ Drop, drop };
        resource struct Db { tag: str }
        impl Db with Drop { fun drop(&mut self) { print(i"close {self.tag}"); } }
        fun consume(own d: Db) {
            print("in");
            drop(d);
            print("post");
        }
        fun main() {
            consume(Db { tag = "one" });
            print("done");
        }
        "#,
        "in\nclose one\npost\ndone\n",
    );
}

// --- Match-move (R6, destruction.md §5) -------------------------------------

#[test]
fn a_resource_match_consume_moves_the_payload_to_its_new_owner() {
    // Matching a resource BY VALUE consumes the subject; the capture aliases the
    // payload, and because the subject is dead the alias IS the move (R6). Moving
    // the payload out of the arm hands it to a new owner (`extracted`), whose
    // scope-end drop is visible — the runtime alias-as-move the resource path
    // relies on (impl-plan §7 risk).
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        resource enum Holder { Full(Res), Empty }
        fun main() {
            let holder = Holder::Full(Res { tag = "held" });
            let extracted = match holder {
                Holder::Full(let inner) => inner,
                Holder::Empty => Res { tag = "default" },
            };
            print(i"extracted {extracted.tag}");
        }
        "#,
        "extracted held\ndrop held\n",
    );
}

// --- C4 S4 chunk a: `Database` — the first real std resource (destruction.md
// §9), plus the §5 loan-only corollary for module-level resources. The
// `[service]`-owns-a-resource collision is recorded as backlog C9; the pin below
// fixes it as the defined v1 rejection (the blessed idiom keeps the resource at
// module scope). `Database` closes its `node:sqlite` handle on drop.

#[test]
fn a_database_binding_moves_and_a_later_use_is_use_after_move() {
    // R1 on the real std resource: `Database` moves on binding, and using the
    // moved binding is use-after-move — the note at the move site (occurrence 1
    // of "handle", the `let heir = handle`).
    assert_use_after_move_noting(
        r#"
        import std::db::Database;
        fun main() {
            let handle = Database::open(":memory:");
            let heir = handle;
            handle.exec("SELECT 1");
        }
        "#,
        "handle",
        1,
    );
}

#[test]
fn a_struct_holding_a_database_is_a_resource_by_containment() {
    // Containment: a struct with a `Database` field is itself a resource, so it
    // moves (R1) — a later use of the moved aggregate is use-after-move.
    assert_fails_with(
        r#"
        import std::db::Database;
        struct Session { db: Database }
        fun main() {
            let session = Session { db = Database::open(":memory:") };
            let moved = session;
            session.db.exec("SELECT 1");
        }
        "#,
        "after it was moved",
    );
}

#[test]
fn a_list_of_databases_is_rejected() {
    // R10 with the real type: a native container cannot hold a resource.
    assert_fails_with(
        r#"
        import std::db::Database;
        fun main() {
            let dbs: List<Database> = [];
        }
        "#,
        "cannot hold the resource",
    );
}

#[test]
fn a_module_level_database_is_accepted() {
    // The serve-forever idiom (destruction.md §5): a module-level `Database` has
    // process lifetime, reached by loan through method calls — it never drops.
    assert_compiles(
        r#"
        import std::db::Database;
        let db: Database = Database::open(":memory:");
        fun query() { db.exec("SELECT 1"); }
        fun main() { query(); }
        "#,
    );
}

#[test]
fn dropping_a_local_database_compiles_under_a_process_target() {
    // `drop(db)` is the early teardown (there is no public `close()`); it lowers
    // to the handle's destructor under the process (node) target.
    assert_compiles(
        r#"
        import std::db::Database;
        import std::drop::drop;
        fun main() {
            let db = Database::open(":memory:");
            drop(db);
        }
        "#,
    );
}

#[test]
fn a_wire_derive_on_a_database_holding_struct_is_rejected() {
    // §8: the Wire all-fields check rejects a resource field — a `Database` is
    // not plain data and cannot cross the wire.
    assert_fails_with(
        r#"
        import std::db::Database;
        [derive(Wire)]
        struct Snapshot { db: Database }
        "#,
        "is not plain data",
    );
}

#[test]
fn a_service_struct_owning_a_resource_is_rejected() {
    // Backlog C9 (the defined v1 rejection): a `[service]` struct that owns a
    // resource is itself a resource, and the generated dispatcher captures `self`
    // into a per-`[rpc]` handler closure — which a resource cannot be (R9). The
    // steer is the capture message; the fix is the module-level idiom.
    assert_fails_with(
        r#"
        import std::db::Database;
        import std::reactive::Signal;
        [service(Client)]
        struct Store {
            [expose] count: Signal<i32>,
            db: Database,
        }
        impl Store {
            [rpc]
            fun ping(self): i32 { 1 }
        }
        "#,
        "cannot capture the resource",
    );
}

// --- §5 loan-only corollary: a module-level resource is process-lifetime, so it
// can only be loaned; moving / `own`-passing / `drop`ing it is rejected.

#[test]
fn a_module_level_resource_move_into_a_local_is_rejected() {
    assert_fails_with(
        r#"
        import std::print;
        resource struct Res { tag: str }
        let shared: Res = Res { tag = "global" };
        fun steal() {
            let mine = shared;
            print(mine.tag);
        }
        fun main() { steal(); }
        "#,
        "module-level resource",
    );
}

#[test]
fn a_module_level_resource_overwrite_is_rejected() {
    // The loan-only corollary's WRITE half (found 2026-07-20 by the lazy-init
    // question): overwriting a module global implies dropping the old value at
    // a site that can never drop — probed pre-fix, the old value silently
    // leaked. The initializer is the one sanctioned write.
    assert_fails_with(
        r#"
        import std::option::Option::{ self, Some, None };
        resource struct Res { tag: str }
        mut slot: Option<Res> = None;
        fun poke() {
            slot = Some(Res { tag = "made" });
        }
        fun main() { poke(); }
        "#,
        "cannot be overwritten",
    );
}

#[test]
fn a_module_level_data_binding_overwrite_is_accepted() {
    // The control: module-level DATA has no drop obligation — plain global
    // state stays writable exactly as before.
    assert_compiles(
        r#"
        import std::print;
        mut counter: i32 = 0;
        fun tick() {
            counter = counter + 1;
        }
        fun main() { tick(); print(i"{counter}"); }
        "#,
    );
}

// --- kolt.local 031 S3: `File` — the second std resource (filesystem.md §3.2,
// §5). The `Database` template followed exactly: `resource external struct`,
// construction through associated funs (over one raw async extern rather than
// an extern-new — `fsPromises.open` is a module function, not a constructor),
// release a module-level free function reachable only from `Drop`, no public
// `close()`. The one `File` divergence is RULED (Q1, 2026-08-27, (a)+(c)
// scoped to `File` alone): `FileHandle.close()` is async, so `drop` INITIATES
// the close without awaiting it and `with_file` is the idiom whose close IS
// awaited. Runtime behavior is pinned end-to-end in
// `crates/vilan-cli/tests/fs.rs`; these pin the semantics and the emission.

#[test]
fn a_file_binding_moves_and_a_later_use_is_use_after_move() {
    // R1 on the second std resource: `File` moves on binding, and a stale
    // handle is a compile error rather than a runtime `EBADF`.
    assert_use_after_move_noting(
        r#"
        import std::fs::File;
        fun main() {
            let handle = File::open("data.txt");
            let heir = handle;
            handle.stat();
        }
        "#,
        "handle",
        1,
    );
}

#[test]
fn a_list_of_files_is_rejected() {
    // R10: no `List<File>` — `Option<File>` is the sanctioned container, so
    // "a pool of open files" is not expressible in v1 (the docs say so out
    // loud, per filesystem.md §5's request).
    assert_fails_with(
        r#"
        import std::fs::File;
        fun main() {
            let files: List<File> = [];
        }
        "#,
        "cannot hold the resource",
    );
}

#[test]
fn a_closure_cannot_capture_a_file() {
    // R9 with the real type — the rule that shapes the whole fs surface
    // (`with_file` hands the body its file as a per-call PARAMETER instead,
    // and the watch design in filesystem.md §8 is pull-based because of it).
    assert_fails_with(
        r#"
        import std::fs::File;
        fun run_it(body: || void) { body(); }
        fun main() {
            let file = File::open("data.txt");
            run_it(|| { file.stat(); });
        }
        "#,
        "cannot capture the resource",
    );
}

#[test]
fn a_module_level_file_initializer_is_refused_for_awaiting() {
    // Where `File` genuinely diverges from `Database`, and the divergence is
    // structural, not chosen: every constructor is async (`fsPromises.open`),
    // and a module-level initializer cannot await (§J.3). So `Database`'s
    // module-level serve-forever idiom is NOT expressible for `File` — the
    // process-lifetime handle lives in `main`'s scope instead (owning a
    // resource across awaits is legal, and `main` never returning keeps it
    // open). filesystem.md §5 claimed the module-level idiom carried over;
    // this pin records that it does not.
    assert_fails_with(
        r#"
        import std::print;
        import std::fs::File;
        let log: File = File::append_to("app.log");
        fun main() {
            print(log.stat().size);
        }
        "#,
        "a module-level binding cannot await",
    );
}

#[test]
fn the_handles_postfix_idiom_typechecks_off_the_awaited_constructor() {
    // B141's historically-broken spellings as POSITIVE tests (filesystem.md
    // §11.1 named the fix S3's prerequisite; Order 13 shipped it): a postfix
    // straight off the implicitly-awaited constructor call. The typing half —
    // the emitted programs run in `fs.rs`'s e2e pins and the corpus golden
    // `file.vl` holds the emitted parenthesization.
    assert_compiles(
        r#"
        import std::print;
        import std::bytes::Bytes;
        import std::fs::File;
        fun main() {
            let buffer = Bytes::alloc(8);
            print(File::open("data.txt").read_at(buffer, 0));
            print(File::open("data.txt").stat().size);
        }
        "#,
    );
}

#[test]
fn a_scope_end_file_drop_is_a_finally_that_initiates_the_close() {
    // The safety net (Q1's (a)): a locally-owned `File` gets the same
    // try/finally teardown `Database` does, and the finally reaches
    // `__fs_close`, whose helper starts `close()` WITHOUT awaiting it and
    // routes a close failure to console.error instead of an unhandled
    // rejection. Plant-proven: removing `impl File with Drop` from fs.vl
    // reddens both assertions (and the `file.vl` corpus golden).
    let js = compile(
        r#"
        import std::print;
        import std::fs::File;
        fun main() {
            let file = File::open("data.txt");
            print(file.stat().size);
        }
        "#,
    )
    .expect("compiles");
    assert!(
        js.contains("finally"),
        "a local File must get a scope-end teardown finally:\n{js}"
    );
    assert!(
        js.contains("function __fs_close(file)") && js.contains("file.close().catch("),
        "the finally must reach the fire-and-forget close helper:\n{js}"
    );
}

#[test]
fn dropping_a_local_file_early_lowers_to_the_same_close() {
    // `drop(file)` is the early form (no public `close()` exists to call);
    // it lowers to the destructor, which reaches the same helper.
    let js = compile(
        r#"
        import std::fs::File;
        import std::drop::drop;
        fun main() {
            let file = File::open("data.txt");
            file.stat();
            drop(file);
        }
        "#,
    )
    .expect("compiles");
    assert!(
        js.contains("__fs_close("),
        "an explicit drop(file) must reach the close helper:\n{js}"
    );
}

#[test]
fn with_file_awaits_the_close_before_returning() {
    // Q1's (c), the half the destructor cannot give: `with_file`'s close is
    // AWAITED, so it completes — and can fail observably — before `with_file`
    // returns. The emitted `await` IS the ordering, so it is pinned on the
    // bytes. Plant-proven: dropping the `close_awaited(file)` line from
    // `with_file` in fs.vl reddens this (and the corpus golden).
    let js = compile(
        r#"
        import std::print;
        import std::fs::with_file;
        fun main() {
            let size = with_file("data.txt", |file| file.stat().size);
            print(size);
        }
        "#,
    )
    .expect("compiles");
    assert!(
        js.contains("await (__fs_close_awaited(file))"),
        "with_file must AWAIT its close (kolt.local 031 Q1):\n{js}"
    );
}

#[test]
fn opening_a_file_on_a_browser_build_is_refused_by_coloring() {
    // The browser leg refuses by COLORING (the arm that actually fires —
    // probed, and it is `Database`'s arm exactly): `File` the type is
    // colorless, but every way to OBTAIN one is seeded `@process` by
    // definition site, so a browser-reachable path can never hold a handle.
    assert_fails_browser_with(
        r#"
        import std::fs::File;
        fun main() {
            let file = File::open("data.txt");
        }
        "#,
        "`open` requires the `process` layer of `std` and cannot run on `browser`\n  reachable from the entry: main → open (std::fs)",
    );
}

#[test]
fn fs_exists_is_gone_and_the_import_says_so() {
    // kolt.local 031 Q3, ruled 2026-08-27: `fs::exists` is DELETED — its
    // justification named a category ("boot code") rather than a caller, all
    // three callers were already async, and `stat(path).is_some()` answers
    // strictly more. The module's last synchronous entry went with it.
    assert_fails_with(
        r#"
        import std::fs::exists;
        fun main() {}
        "#,
        "cannot find 'exists' in the imported path",
    );
}

// --- kolt.local 020: `Watcher` — the watch tier, and std's third resource.
// Designed to MATCH `File` by ruling (the owner, 2026-08-28: 020 owns the whole
// watch surface, shape and mechanism both, and its resource follows
// filesystem.md §5's lifetime model): `resource external struct`, construction
// through associated funs over one raw async extern, release a module-level
// free function reachable only from `Drop`, no public `stop()`. Where it
// DIVERGES from `File` is the interesting half — stopping a poll is a
// `clearTimeout`, so the teardown is genuinely synchronous and Q1's
// async-close exception (ruled for `File` ALONE, deliberately) is NOT
// inherited. Runtime behavior against real file activity is pinned in
// `crates/vilan-cli/tests/fs.rs`; these pin the semantics and the emission.

#[test]
fn a_watcher_binding_moves_and_a_later_use_is_use_after_move() {
    // R1 on the third std resource: a stale watcher is a compile error.
    assert_use_after_move_noting(
        r#"
        import std::fs::Watcher;
        fun main() {
            let watcher = Watcher::watch("src");
            let heir = watcher;
            watcher.next();
        }
        "#,
        "watcher",
        1,
    );
}

#[test]
fn a_list_of_watchers_is_rejected() {
    // R10: no `List<Watcher>` either — "watch several trees" is not
    // expressible in v1 any more than "a pool of open files" is.
    assert_fails_with(
        r#"
        import std::fs::Watcher;
        fun main() {
            let watchers: List<Watcher> = [];
        }
        "#,
        "cannot hold the resource",
    );
}

#[test]
fn a_closure_cannot_capture_a_watcher() {
    // R9 — the rule that decides the tier's shape. A callback-shaped watch
    // would have to hand the handler a captured watcher (or a captured `File`
    // to read what changed); neither is expressible, which is half the reason
    // `next()` is a pull.
    assert_fails_with(
        r#"
        import std::fs::Watcher;
        fun run_it(body: || void) { body(); }
        fun main() {
            let watcher = Watcher::watch("src");
            run_it(|| { watcher.next(); });
        }
        "#,
        "cannot capture the resource",
    );
}

#[test]
fn a_watcher_has_no_public_stop() {
    // The `Database`/`File` law, kept: the release has no public surface, so
    // there is no second teardown path to fall out of sync with the
    // destructor. `drop(watcher)` is the early form and the only one.
    assert_fails_with(
        r#"
        import std::fs::Watcher;
        fun main() {
            let watcher = Watcher::watch("src");
            watcher.stop();
        }
        "#,
        "Watcher has no method 'stop'",
    );
}

#[test]
fn a_module_level_watcher_initializer_is_refused_for_awaiting() {
    // `File`'s structural divergence from `Database` carries to `Watcher` for
    // the same reason and by a different route: the constructor is async
    // because it takes the BASELINE before returning (a watcher reports
    // changes since it was created), and a module-level initializer cannot
    // await. A process-lifetime watcher is a local in `main`.
    assert_fails_with(
        r#"
        import std::fs::Watcher;
        let sources: Watcher = Watcher::watch_all("src");
        fun main() {
            sources.next();
        }
        "#,
        "a module-level binding cannot await",
    );
}

#[test]
fn the_three_change_kinds_match_without_a_catch_all() {
    // The surface's promise: comparing two stats leaves exactly three
    // outcomes, so `ChangeKind` is exhaustive and a `match` over it needs no
    // `_` arm. (The opposite call from `Entry`'s three booleans in the same
    // module, and for the opposite reason — a host dirent has nine
    // open-ended kinds.)
    assert_compiles(
        r#"
        import std::print;
        import std::fs::{ Change, ChangeKind, Watcher };
        fun describe(change: Change): str {
            match change.kind {
                ChangeKind::Created => "created",
                ChangeKind::Modified => "modified",
                ChangeKind::Removed => "removed",
            }
        }
        fun main() {
            let watcher = Watcher::watch("src");
            print(describe(watcher.next()));
        }
        "#,
    );
}

#[test]
fn a_scope_end_watcher_drop_is_a_finally_that_stops_the_poll() {
    // The teardown that matters: a watcher polls on a host timer, and a
    // pending host timer keeps the process alive. The scope-end `finally`
    // reaching `__fs_watch_stop` is what lets a watching program exit.
    // Plant-proven end to end: removing `impl Watcher with Drop` from fs.vl
    // turns `fs.rs`'s termination pin from a 1-second run into a hang.
    let js = compile(
        r#"
        import std::fs::Watcher;
        fun main() {
            let watcher = Watcher::watch("src");
            watcher.next();
        }
        "#,
    )
    .expect("compiles");
    assert!(
        js.contains("finally"),
        "a local Watcher must get a scope-end teardown finally:\n{js}"
    );
    assert!(
        js.contains("function __fs_watch_stop(watcher)"),
        "the finally must reach the stop helper:\n{js}"
    );
}

#[test]
fn dropping_a_watcher_early_lowers_to_the_same_stop() {
    // `drop(watcher)` is the early form (no public `stop()` exists to call);
    // it lowers to the destructor, which reaches the same helper.
    let js = compile(
        r#"
        import std::fs::Watcher;
        import std::drop::drop;
        fun main() {
            let watcher = Watcher::watch("src");
            watcher.next();
            drop(watcher);
        }
        "#,
    )
    .expect("compiles");
    assert!(
        js.contains("__fs_watch_stop("),
        "an explicit drop(watcher) must reach the stop helper:\n{js}"
    );
}

#[test]
fn the_watchers_release_is_synchronous_and_inherits_no_file_exception() {
    // The pin that records the ruling's scope. `File`'s `__fs_close` is a
    // fire-and-forget over a promise, with a `.catch` because a destructor
    // cannot await; `Watcher`'s release is `clearTimeout` behind a plain
    // synchronous call, so destruction.md §5's "drop is synchronous in v1"
    // holds here with nothing asked of it. Q1's exception was ruled for
    // `File` ALONE and this tier does not quietly widen it.
    let js = compile(
        r#"
        import std::fs::Watcher;
        fun main() {
            let watcher = Watcher::watch("src");
            watcher.next();
        }
        "#,
    )
    .expect("compiles");
    assert!(
        js.contains("function __fs_watch_stop(watcher) {\n\twatcher.stop();\n}"),
        "the stop helper must be a plain synchronous call:\n{js}"
    );
    assert!(
        !js.contains("async function __fs_watch_stop"),
        "the release must not be async — that is File's exception, not this tier's:\n{js}"
    );
}

#[test]
fn a_parked_watch_carries_the_ambient_cancel_signal() {
    // Not a nicety: without the ambient signal a cancelled nursery whose body
    // is parked on a change that will never come could never drain. `sleep`
    // and `Timer::wait` bridge the same way (verified end to end: cancelling
    // a nursery around a parked `next()` unwinds instead of hanging).
    let js = compile(
        r#"
        import std::fs::Watcher;
        fun main() {
            let watcher = Watcher::watch("src");
            watcher.next();
        }
        "#,
    )
    .expect("compiles");
    assert!(
        js.contains("next_change(ambient_signal("),
        "the wait must carry the ambient cancel signal:\n{js}"
    );
}

#[test]
fn a_batch_of_changes_is_handed_over_path_sorted() {
    // Changes seen in one poll were observed together and carry no ordering
    // between them, so the batch is sorted by path rather than handed over in
    // whatever order the host's `readdir` produced — one order on every host.
    // Pinned HERE rather than end to end, honestly: the Linux host the e2e
    // suite runs on already returns `readdir` entries sorted, so no runtime
    // assertion there can tell a sorting poller from a non-sorting one
    // (planted and confirmed — `fs.rs`'s batch pin says so at its site).
    let js = compile(
        r#"
        import std::fs::Watcher;
        fun main() {
            let watcher = Watcher::watch_all("src");
            watcher.next();
        }
        "#,
    )
    .expect("compiles");
    assert!(
        js.contains("changes.sort("),
        "a batch must be handed over path-sorted:\n{js}"
    );
}

#[test]
fn starting_a_watch_on_a_browser_build_is_refused_by_coloring() {
    // The browser leg refuses by COLORING, and this is the arm that actually
    // fires — `File`'s arm and `Database`'s before it. `Watcher` the type is
    // colorless; every way to OBTAIN one is seeded `@process` by definition
    // site, so a browser-reachable path can never hold a watcher.
    assert_fails_browser_with(
        r#"
        import std::fs::Watcher;
        fun main() {
            let watcher = Watcher::watch("src");
        }
        "#,
        "`watch` requires the `process` layer of `std` and cannot run on `browser`\n  reachable from the entry: main → watch (std::fs)",
    );
}

#[test]
fn a_recursive_watch_on_a_browser_build_is_refused_too() {
    // The other constructor, separately — a coloring seed is per definition
    // site, so "one of them is refused" is not evidence about the other.
    assert_fails_browser_with(
        r#"
        import std::fs::Watcher;
        fun main() {
            let watcher = Watcher::watch_all("src");
        }
        "#,
        "`watch_all` requires the `process` layer of `std` and cannot run on `browser`\n  reachable from the entry: main → watch_all (std::fs)",
    );
}

#[test]
fn a_module_level_resource_own_argument_is_rejected() {
    assert_fails_with(
        r#"
        resource struct Res { tag: str }
        let shared: Res = Res { tag = "global" };
        fun consume(own r: Res) {}
        fun use_it() { consume(shared); }
        fun main() { use_it(); }
        "#,
        "module-level resource",
    );
}

#[test]
fn a_module_level_resource_loan_is_accepted() {
    // A method call and a bare (loan) parameter both borrow the global — accepted.
    assert_compiles(
        r#"
        import std::print;
        resource struct Res { tag: str }
        impl Res { fun peek(self) { print(self.tag); } }
        let shared: Res = Res { tag = "global" };
        fun borrow_it(r: Res) { print(r.tag); }
        fun use_it() {
            shared.peek();
            borrow_it(shared);
        }
        fun main() { use_it(); }
        "#,
    );
}

#[test]
fn dropping_a_module_level_resource_is_rejected() {
    // `drop(global)` is an `own`-move of a process-lifetime binding — rejected.
    assert_fails_with(
        r#"
        import std::drop::drop;
        resource struct Res { tag: str }
        let shared: Res = Res { tag = "global" };
        fun tear() { drop(shared); }
        fun main() { tear(); }
        "#,
        "module-level resource",
    );
}

// --- Order 19 / drop-safety: the two error-path defects the lifetimes session's
// probes found on 0.38.0 (lifetimes.md §6 names both, and its last-use lowering
// subsumes them structurally later; these are the narrow, now-shaped fixes).
//
// B151 — the mR2 overwrite double-drops when the RHS throws. The overwrite
// lowering pushed the old value's destructor BEFORE walking the new value's
// expression, so a panic in the RHS left the scope-end `finally` destroying an
// already-destroyed value: a double `close()` today, a double free on a native
// backend. The fix is evaluation ORDER, not lifetimes — the new value is
// computed into a temporary first, then the old value drops, then the write
// lands. memory.md §6.8's R2 sentence promises "drops the old value first, then
// moves the new one in", which is a claim about the drop and the WRITE, and it
// still holds: the drop stays ahead of the store.

#[test]
fn an_overwrite_whose_new_value_panics_drops_the_old_value_exactly_once() {
    // Red-first (B151, probe E3 inverted): before the fix this printed "old"
    // TWICE — once from the overwrite's drop, once from the scope-end `finally`
    // walking over the corpse.
    let (stdout, _stderr, code) = compile_and_run_status(
        r#"
        import std::print;
        import std::panic;
        import std::drop::Drop;
        resource struct Guard { tag: str }
        impl Guard with Drop {
            fun drop(&mut self) { print(self.tag); }
        }
        enum Holder { Full(Guard), Empty }
        fun boom(): Holder {
            panic("boom");
            Holder::Empty
        }
        fun main() {
            mut holder = Holder::Full(Guard { tag = "old" });
            holder = boom();
            print("unreachable");
        }
        "#,
    );
    assert_eq!(
        stdout, "old\n",
        "a throwing right-hand side must leave EXACTLY one drop of the old value"
    );
    assert_ne!(code, 0, "the panic must still take the process down");
}

#[test]
fn an_overwrite_drops_the_old_value_before_the_new_one_is_stored() {
    // R2's ordering sentence, unchanged: the old value is destroyed before the
    // new one moves in (so "old" precedes the scope-end "new"). A literal
    // right-hand side cannot throw, so its emission is untouched by the fix.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Guard { tag: str }
        impl Guard with Drop {
            fun drop(&mut self) { print(self.tag); }
        }
        fun main() {
            mut guard = Guard { tag = "old" };
            guard = Guard { tag = "new" };
            print("body");
        }
        "#,
        "old\nnew\nbody\n",
    );
}

#[test]
fn an_overwrite_evaluates_an_effectful_new_value_before_dropping_the_old_one() {
    // The half B151 moves: the right-hand side runs FIRST, so a panic inside it
    // never reaches a slot whose value is already destroyed. R2 is unharmed —
    // the drop still precedes the store — but the effects of computing the new
    // value now precede the old value's destructor, and that order is the fix.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Guard { tag: str }
        impl Guard with Drop {
            fun drop(&mut self) { print(self.tag); }
        }
        fun make(): Guard {
            print("made");
            Guard { tag = "new" }
        }
        fun main() {
            mut guard = Guard { tag = "old" };
            guard = make();
            print("body");
        }
        "#,
        "made\nold\nnew\nbody\n",
    );
}

#[test]
fn an_overwrite_through_a_view_also_evaluates_before_it_destroys() {
    // R2's loan half (B94) takes the same ordering: a write through a `&mut`
    // view destroys the pointee, so a throwing right-hand side must not leave
    // the caller's `finally` a destroyed value to walk over.
    let (stdout, _stderr, code) = compile_and_run_status(
        r#"
        import std::print;
        import std::panic;
        import std::drop::Drop;
        resource struct Guard { tag: str }
        impl Guard with Drop {
            fun drop(&mut self) { print(self.tag); }
        }
        resource struct Slot { held: Guard }
        fun boom(): Slot {
            panic("boom");
            Slot { held = Guard { tag = "never" } }
        }
        fun refill(slot: &mut Slot) {
            slot = boom();
        }
        fun main() {
            mut slot = Slot { held = Guard { tag = "held" } };
            refill(&mut slot);
            print("unreachable");
        }
        "#,
    );
    assert_eq!(
        stdout, "held\n",
        "a throwing right-hand side must leave EXACTLY one drop through the view too"
    );
    assert_ne!(code, 0, "the panic must still take the process down");
}

// B150 — `drop(x)` is not exception-safe. The explicit drop moved the binding
// into the sink, so `plan_resource_drops` unenrolled it and the scope's
// teardown `finally` disappeared: a panic between the acquisition and the
// `drop(x)` leaked the resource permanently, where a scope-end drop would have
// released it. The fix keeps the binding enrolled and makes the pair
// idempotent — the sink empties the slot (`= null`, the moved-out state
// `Option.take` leaves behind), and the `finally` destroys only a slot that is
// still full. The test is the emitted machinery's, not the source language's:
// mR7's ban on runtime drop flags is about CONDITIONAL moves, and R7 already
// rejects a conditional `drop(x)` outright.

#[test]
fn a_panic_before_an_explicit_drop_still_releases_the_resource() {
    // Red-first (B150, probe E2 inverted): before the fix stdout stopped at
    // "acquired" — the `finally` was gone and "released" never printed.
    let (stdout, _stderr, code) = compile_and_run_status(
        r#"
        import std::print;
        import std::panic;
        import std::drop::Drop;
        import std::drop::drop;
        resource struct Res { tag: str }
        impl Res with Drop {
            fun drop(&mut self) { print(self.tag); }
        }
        fun boom() { panic("boom"); }
        fun leaky() {
            let r = Res { tag = "released" };
            print("acquired");
            boom();
            drop(r);
            print("after");
        }
        fun main() { leaky(); }
        "#,
    );
    assert_eq!(
        stdout, "acquired\nreleased\n",
        "a panic before the explicit drop must still run the scope's teardown"
    );
    assert_ne!(code, 0, "the panic must still take the process down");
}

#[test]
fn an_explicit_drop_on_the_normal_path_destroys_exactly_once() {
    // The other half of the pair: with the `finally` back, the fall-through
    // path must NOT drop twice. The early teardown still happens EARLY — the
    // print after it observes a released resource.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        import std::drop::drop;
        resource struct Res { tag: str }
        impl Res with Drop {
            fun drop(&mut self) { print(self.tag); }
        }
        fun early() {
            let r = Res { tag = "released" };
            print("acquired");
            drop(r);
            print("after");
        }
        fun main() { early(); }
        "#,
        "acquired\nreleased\nafter\n",
    );
}

#[test]
fn an_explicit_drop_empties_the_slot_its_finally_tests() {
    // The emitted machinery, pinned on the bytes: the sink writes the slot
    // empty and the scope-end teardown reads that emptiness. Both halves must
    // be present — either alone is a leak or a double drop.
    let js = compile(
        r#"
        import std::print;
        import std::drop::Drop;
        import std::drop::drop;
        resource struct Res { tag: str }
        impl Res with Drop {
            fun drop(&mut self) { print(self.tag); }
        }
        fun early() {
            let r = Res { tag = "released" };
            drop(r);
            print("after");
        }
        fun main() { early(); }
        "#,
    )
    .expect("compiles");
    assert!(
        js.contains("= null"),
        "the explicit drop must empty the slot:\n{js}"
    );
    assert!(
        js.contains("!== null") && js.contains("finally"),
        "the scope-end teardown must test the slot before destroying it:\n{js}"
    );
}

#[test]
fn a_resource_with_no_explicit_drop_keeps_its_unguarded_teardown() {
    // The delta is scoped to the bindings an explicit `drop(x)` reaches: a
    // scope that never calls the sink emits the same bare `finally` it always
    // did, with no emptiness test and no rebindable slot.
    let js = compile(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop {
            fun drop(&mut self) { print(self.tag); }
        }
        fun scoped() {
            let r = Res { tag = "released" };
            print(i"body {r.tag}");
        }
        fun main() { scoped(); }
        "#,
    )
    .expect("compiles");
    assert!(
        js.contains("finally"),
        "the teardown must still be there:\n{js}"
    );
    assert!(
        !js.contains("!== null"),
        "an unguarded teardown must not grow an emptiness test:\n{js}"
    );
}

#[test]
fn a_panic_before_dropping_an_own_parameter_still_releases_it() {
    // The same pair on an `own` resource PARAMETER, whose teardown wraps the
    // whole body (destruction.md §6) rather than riding a `let`.
    let (stdout, _stderr, code) = compile_and_run_status(
        r#"
        import std::print;
        import std::panic;
        import std::drop::Drop;
        import std::drop::drop;
        resource struct Res { tag: str }
        impl Res with Drop {
            fun drop(&mut self) { print(self.tag); }
        }
        fun boom() { panic("boom"); }
        fun sink(own r: Res) {
            print("entered");
            boom();
            drop(r);
        }
        fun main() { sink(Res { tag = "released" }); }
        "#,
    );
    assert_eq!(
        stdout, "entered\nreleased\n",
        "an own parameter dropped explicitly must still drop when a panic beats the sink"
    );
    assert_ne!(code, 0, "the panic must still take the process down");
}

#[test]
fn an_own_parameter_dropped_explicitly_destroys_exactly_once() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        import std::drop::drop;
        resource struct Res { tag: str }
        impl Res with Drop {
            fun drop(&mut self) { print(self.tag); }
        }
        fun sink(own r: Res) {
            print("entered");
            drop(r);
            print("after");
        }
        fun main() { sink(Res { tag = "released" }); }
        "#,
        "entered\nreleased\nafter\n",
    );
}

#[test]
fn a_panic_before_dropping_a_match_capture_still_releases_it() {
    // The `match opt.take() { Some(let c) => drop(c), ... }` idiom memory.md
    // §6.8 sanctions for conditional teardown: its capture owns the payload, so
    // it owes the same exception safety a `let` does.
    let (stdout, _stderr, code) = compile_and_run_status(
        r#"
        import std::print;
        import std::panic;
        import std::drop::Drop;
        import std::drop::drop;
        import std::option::Option;
        import std::option::Some;
        import std::option::None;
        resource struct Res { tag: str }
        impl Res with Drop {
            fun drop(&mut self) { print(self.tag); }
        }
        fun boom() { panic("boom"); }
        fun teardown() {
            mut full: Option<Res> = Some(Res { tag = "released" });
            match full.take() {
                Some(let c) => {
                    print("captured");
                    boom();
                    drop(c);
                },
                None => {},
            }
        }
        fun main() { teardown(); }
        "#,
    );
    assert_eq!(
        stdout, "captured\nreleased\n",
        "a captured payload must drop even when a panic beats its explicit drop"
    );
    assert_ne!(code, 0, "the panic must still take the process down");
}

#[test]
fn a_match_capture_dropped_explicitly_destroys_exactly_once() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        import std::drop::drop;
        import std::option::Option;
        import std::option::Some;
        import std::option::None;
        resource struct Res { tag: str }
        impl Res with Drop {
            fun drop(&mut self) { print(self.tag); }
        }
        fun teardown() {
            mut full: Option<Res> = Some(Res { tag = "released" });
            match full.take() {
                Some(let c) => drop(c),
                None => {},
            }
            print("after");
        }
        fun main() { teardown(); }
        "#,
        "released\nafter\n",
    );
}

#[test]
fn a_binding_reassigned_after_its_explicit_drop_destroys_each_value_once() {
    // The ordering-sensitive corner where the two halves meet: the sink empties
    // the slot, the assignment refills it, and the guarded `finally` destroys
    // the SECOND value only. No overwrite drop fires — the slot was empty.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        import std::drop::drop;
        resource struct Res { tag: str }
        impl Res with Drop {
            fun drop(&mut self) { print(self.tag); }
        }
        fun reuse() {
            mut r = Res { tag = "one" };
            drop(r);
            r = Res { tag = "two" };
            print("body");
        }
        fun main() { reuse(); }
        "#,
        "one\ntwo\nbody\n",
    );
}

// ============================================================================
// S3 — last-use disposal (`lifetimes.md` §6; the ordering amendment to
// `memory.md` §6.8, RULED 2026-08-28)
// ============================================================================
//
// Disposal moved from the owner's SCOPE END to its LAST USE. The pins above
// hold every rule that did not move — which bindings drop, how many times, in
// what order among simultaneous discharges, and that a loan owns nothing —
// while the ones below hold the timing itself, per case:
//
// - the ordinary shape (drop after the last read, not at the scope's end);
// - a binding nothing reads, which drops at its declaration — the shape that
//   makes the fix total for a `main` that never returns (`lifetimes.md` §6,
//   `temporary-drop.md` §5.3);
// - branch-join specialization: a use in one arm releases on the taken AND the
//   not-taken path, at the join, with no runtime flag anywhere;
// - loops, in both directions (declared outside, declared inside);
// - the loan-extension rule, which is the one unsoundness shape §6.1 names;
// - the refusals — an opaque binding keeps the scope-end teardown it had, and
//   a module-level resource is not reached at all.
//
// Every drop here still rides a `finally`, so `ret` / `jump` / a panic release
// on the way out; `drop_runs_on_early_ret` and its neighbours above hold that.

#[test]
fn a_resource_drops_after_its_last_use_not_at_the_scope_end() {
    // THE ruling. `r`'s last read is the first `print`, so the teardown fires
    // between the two statements instead of after both.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            let r = Res { tag = "r" };
            print(r.tag);
            print("after");
        }
        "#,
        "r\ndrop r\nafter\n",
    );
}

#[test]
fn a_resource_nothing_reads_drops_at_its_declaration() {
    // No read is not "read at the scope's end": a handle the program never
    // names again is released now. This is what makes the serve-forever `main`
    // fix total — under scope-end that release never arrives.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            let r = Res { tag = "r" };
            print("body");
        }
        "#,
        "drop r\nbody\n",
    );
}

#[test]
fn a_resource_used_after_a_never_ending_loop_would_start_is_released_before_it() {
    // The shape `temporary-drop.md` §5.3 names as the case that separates the
    // options, at unit scale: the handle's last read precedes the long-running
    // tail, so it is released BEFORE the tail runs rather than after a scope
    // end the tail never reaches. (The loop is bounded here so the pin can
    // terminate; the point is the release ordering, not the loop.)
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun serve() {
            let handle = Res { tag = "handle" };
            print(handle.tag);
            for round in [ 1, 2 ] {
                print(i"serving {round}");
            }
            print("served");
        }
        fun main() { serve(); }
        "#,
        "handle\ndrop handle\nserving 1\nserving 2\nserved\n",
    );
}

#[test]
fn two_resources_last_used_in_one_statement_discharge_in_reverse_declaration_order() {
    // The ordering amendment's second half: simultaneous discharges keep the
    // reverse declaration order `memory.md` §6.8 always specified — `b` before
    // `a` — and the pair still fires before the statement after it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            let a = Res { tag = "a" };
            let b = Res { tag = "b" };
            print(a.tag + b.tag);
            print("after");
        }
        "#,
        "ab\ndrop b\ndrop a\nafter\n",
    );
}

#[test]
fn resources_last_used_in_different_statements_discharge_at_each_last_use() {
    // Not simultaneous: `a` is read after `b` is finished with, so `b` goes
    // first even though it was declared second. This is exactly what the
    // amendment breaks — under the old law both waited for the scope end and
    // came out in reverse declaration order.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            let a = Res { tag = "a" };
            let b = Res { tag = "b" };
            print(b.tag);
            print(a.tag);
            print("after");
        }
        "#,
        "b\ndrop b\na\ndrop a\nafter\n",
    );
}

#[test]
fn a_resource_used_in_one_branch_arm_releases_on_the_taken_and_the_not_taken_path() {
    // Branch-join drop specialization (§6.3, RULED): the last read is inside
    // one arm, so the drop lands at the JOIN — the point the not-taken path
    // enters too. Both paths release exactly once, and no runtime flag decides
    // which (mR7's doctrine, intact).
    let program = |taken: &str| {
        format!(
            r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res {{ tag: str }}
        impl Res with Drop {{ fun drop(&mut self) {{ print(i"drop {{self.tag}}"); }} }}
        fun run(cond: bool) {{
            let r = Res {{ tag = "r" }};
            if cond {{
                print(r.tag);
            }}
            print("join");
        }}
        fun main() {{ run({taken}); }}
        "#
        )
    };
    assert_compiles_and_runs(&program("true"), "r\ndrop r\njoin\n");
    assert_compiles_and_runs(&program("false"), "drop r\njoin\n");
}

#[test]
fn a_resource_used_in_both_arms_still_releases_once_at_the_join() {
    // The control for the specialization: a use on EVERY path is still one
    // drop at one point, not one per arm.
    let program = |taken: &str| {
        format!(
            r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res {{ tag: str }}
        impl Res with Drop {{ fun drop(&mut self) {{ print(i"drop {{self.tag}}"); }} }}
        fun run(cond: bool) {{
            let r = Res {{ tag = "r" }};
            if cond {{ print(i"yes {{r.tag}}"); }} else {{ print(i"no {{r.tag}}"); }}
            print("join");
        }}
        fun main() {{ run({taken}); }}
        "#
        )
    };
    assert_compiles_and_runs(&program("true"), "yes r\ndrop r\njoin\n");
    assert_compiles_and_runs(&program("false"), "no r\ndrop r\njoin\n");
}

#[test]
fn a_resource_used_inside_a_loop_drops_once_after_the_loop() {
    // Declared OUTSIDE, read inside: the last read's statement is the loop, so
    // the drop is at the loop's exit — once, not per iteration.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            let r = Res { tag = "r" };
            for round in [ 1, 2 ] {
                print(i"{round} {r.tag}");
            }
            print("after");
        }
        "#,
        "1 r\n2 r\ndrop r\nafter\n",
    );
}

#[test]
fn a_resource_declared_inside_a_loop_drops_at_its_last_use_each_iteration() {
    // Declared INSIDE: fresh per iteration, so its last read in the body is a
    // genuine last use and the release happens before the body's tail — the
    // precision the old lexical "inside a loop" refusal could not reach.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            for round in [ 1, 2 ] {
                let r = Res { tag = "r" };
                print(i"{round} {r.tag}");
                print("tail");
            }
            print("after");
        }
        "#,
        "1 r\ndrop r\ntail\n2 r\ndrop r\ntail\nafter\n",
    );
}

#[test]
fn a_view_extends_its_owners_liveness_to_the_views_last_use() {
    // §6.1's loan-extension rule, which is the one unsoundness shape the probe
    // battery found: the owner's own last read is the `&`, but a view rooted at
    // it is read later, so the owner must stay alive until the VIEW is done.
    // Dropping at the owner's own last use would print "drop held" before the
    // view reads through it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Guard { label: str }
        impl Guard with Drop { fun drop(&mut self) { print(i"drop {self.label}"); } }
        fun main() {
            mut held = Guard { label = "held" };
            let view = &held;
            print("between");
            print(view.label);
            print("after");
        }
        "#,
        "between\nheld\ndrop held\nafter\n",
    );
}

#[test]
fn a_teardown_region_widens_over_a_name_a_later_closure_reads() {
    // The scoping law again, in its expensive direction. `r`'s own last read is
    // the first `print`, but `label` — declared inside the region that would
    // close there — is read by a closure two statements later, and a name a
    // `try` block declares dies at its brace. So the region grows to cover it,
    // and `r` waits. This is the honest cost of statement-granular regions in a
    // language that lowers them to JS blocks, and it is a hold, never a leak.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            let r = Res { tag = "r" };
            let label = "shown";
            print(r.tag);
            let show = || print(label);
            show();
            print("after");
        }
        "#,
        "r\nshown\nafter\ndrop r\n",
    );
}

#[test]
fn a_resource_read_in_the_scopes_tail_keeps_the_scope_end_teardown() {
    // Where the two answers coincide, the shipped one is what is emitted: a
    // last read in the scope's TAIL *is* the scope's end, so the region is the
    // whole scope and the bytes do not move. This is also the shape every
    // refusal falls back to — the pass never guesses a drop point.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        impl Res { fun size(&self): i32 { 7 } }
        fun measure(): i32 {
            let r = Res { tag = "r" };
            print("body");
            r.size()
        }
        fun main() { print(measure()); }
        "#,
        "body\ndrop r\n7\n",
    );
}

#[test]
fn last_use_disposal_does_not_reach_a_module_level_resource() {
    // Module-level resources never drop (`memory.md` §6.8), and S3 changes
    // nothing about that: they are not enrolled, so no last use is ever asked
    // for. The read below would be the "last use" of a local — and still
    // nothing is destroyed.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        let global = Res { tag = "global" };
        fun main() {
            print(global.tag);
            print("after");
        }
        "#,
        "global\nafter\n",
    );
}

#[test]
fn an_explicit_drop_coincides_with_the_point_the_pass_infers() {
    // P6's identity. Moving into the sink is a USE and R7 rejects a conditional
    // one, so `drop(r)`'s statement IS `r`'s last use: the explicit spelling and
    // the inferred point are the same point, and B150's guarded `finally` — the
    // net over the window between acquisition and the sink — closes there
    // rather than at the scope end. The two programs print the same thing.
    let sunk = r#"
        import std::print;
        import std::drop::Drop;
        import std::drop::drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            let r = Res { tag = "r" };
            print(r.tag);
            drop(r);
            print("after");
        }
        "#;
    let inferred = r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            let r = Res { tag = "r" };
            print(r.tag);
            print("after");
        }
        "#;
    assert_compiles_and_runs(sunk, "r\ndrop r\nafter\n");
    assert_compiles_and_runs(inferred, "r\ndrop r\nafter\n");
}

#[test]
fn an_own_resource_parameter_drops_after_its_last_use() {
    // The parameter class moves with the locals: an `own` parameter is declared
    // before every statement and released after the statement holding its last
    // read, not at the body's end.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun sink(own r: Res) {
            print(r.tag);
            print("tail");
        }
        fun main() {
            sink(Res { tag = "x" });
            print("after");
        }
        "#,
        "x\ndrop x\ntail\nafter\n",
    );
}

#[test]
fn a_last_use_drop_still_runs_when_a_later_statement_in_its_region_panics() {
    // The drop rides a `finally`, so shortening the region never costs the
    // safety net: a panic between the acquisition and the last use still
    // releases on the way out.
    let (stdout, _stderr, code) = compile_and_run_status(
        r#"
        import std::print;
        import std::panic;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun main() {
            let r = Res { tag = "r" };
            print("before");
            panic("boom");
            print(r.tag);
        }
        "#,
    );
    assert_eq!(stdout, "before\ndrop r\n", "the teardown must still run");
    assert_ne!(code, 0, "the panic must still leave a failing exit");
}

#[test]
fn a_teardown_region_never_closes_over_a_name_read_after_it() {
    // The scoping law the emitted `try` imposes, pinned because getting it
    // wrong is a `ReferenceError` and not a leak: `value` is declared inside
    // the region the owner's last use would close, so the region is widened to
    // cover `value`'s own last read. Found by `OwnedNursery::enter`.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        impl Res { fun size(&self): i32 { 7 } }
        fun main() {
            let r = Res { tag = "r" };
            let value = r.size();
            print("between");
            print(value);
        }
        "#,
        "between\n7\ndrop r\n",
    );
}

// The same scoping law, once per NESTED shape (B159). The widening above ran
// only at a function body's top level: the extents the transformer consults
// were keyed on the OUTERMOST enclosing statement, so inside an `if` arm, a
// `match` leg, a loop body or a block two deep the key never matched the
// statement being emitted, the fixpoint never ran, and the `try` closed over a
// `const` read after it — a `ReferenceError` from accepted vilan, released in
// v0.39.0. Every pin here RUNS the program, because the emitted JavaScript
// parses fine and only fails when the trapped name is reached.

#[test]
fn a_teardown_region_inside_an_if_arm_widens_over_a_name_read_after_it() {
    // The audit's exact repro: a resource and a later-read binding in an `if`
    // arm, with a statement after the `if` — which is what makes the arm's
    // statements sit two deep in the declaration chain.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun probe(flag: bool) {
            if flag {
                let r = Res { tag = "r" };
                let value = 7;
                print(r.tag);
                print(value);
                print("arm-end");
            }
            print("after-if");
        }
        fun main() {
            probe(true);
        }
        "#,
        "r\n7\ndrop r\narm-end\nafter-if\n",
    );
}

#[test]
fn a_teardown_region_inside_a_match_arm_widens_over_a_name_read_after_it() {
    // The `match` leg is the same nesting: the leg's block is walked under the
    // `match` statement, so its declarations key the `match`, not themselves.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun probe(n: i32) {
            match n {
                1 => {
                    let r = Res { tag = "m" };
                    let value = 7;
                    print(r.tag);
                    print(value);
                    print("arm-end");
                },
                _ => { print("other"); },
            }
            print("after-match");
        }
        fun main() {
            probe(1);
        }
        "#,
        "m\n7\ndrop m\narm-end\nafter-match\n",
    );
}

#[test]
fn a_teardown_region_inside_a_loop_body_widens_over_a_name_read_after_it() {
    // A loop body, where the trapped name is also read on the next iteration's
    // way round: the region must close before the read, inside the body.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun probe(n: i32) {
            mut i = 0;
            for i < n {
                let r = Res { tag = "loop" };
                let value = 7;
                print(r.tag);
                print(value);
                i = i + 1;
            }
            print("after-loop");
        }
        fun main() {
            probe(2);
        }
        "#,
        "loop\n7\ndrop loop\nloop\n7\ndrop loop\nafter-loop\n",
    );
}

#[test]
fn a_teardown_region_two_blocks_deep_widens_over_a_name_read_after_it() {
    // Depth is not the point — a chain LONGER than one is — but the two-deep
    // shape is the one that proves the fix resolves the whole chain rather than
    // the second element of it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun probe(flag: bool) {
            if flag {
                if flag {
                    let r = Res { tag = "deep" };
                    let value = 7;
                    print(r.tag);
                    print(value);
                    print("inner-end");
                }
                print("mid");
            }
            print("outer");
        }
        fun main() {
            probe(true);
        }
        "#,
        "deep\n7\ndrop deep\ninner-end\nmid\nouter\n",
    );
}

#[test]
fn a_teardown_region_widens_whether_its_if_is_a_tail_or_a_statement() {
    // The DIFFERENTIAL the audit isolated the defect with. `tail_form`'s `if`
    // is the body's tail, so the arm's declarations key themselves and the
    // widening always ran; `statement_form` adds one statement after the `if`
    // and nothing else, which is what used to break it. Both shapes here, in
    // one program, so neither can be fixed at the other's expense.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun tail_form(flag: bool) {
            if flag {
                let r = Res { tag = "tail" };
                let value = 7;
                print(r.tag);
                print(value);
                print("tail-end");
            }
        }
        fun statement_form(flag: bool) {
            if flag {
                let r = Res { tag = "stmt" };
                let value = 7;
                print(r.tag);
                print(value);
                print("stmt-end");
            }
            print("after");
        }
        fun main() {
            tail_form(true);
            statement_form(true);
        }
        "#,
        "tail\n7\ndrop tail\ntail-end\nstmt\n7\ndrop stmt\nstmt-end\nafter\n",
    );
}

#[test]
fn a_teardown_region_inside_a_closure_body_widens_over_a_name_read_after_it() {
    // The negative half of the matrix: a closure body is its own walk region,
    // so its statement chains start at the closure and this shape was already
    // correct. It is pinned so the keying change is held to not disturbing it.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun run(f: || void) { f(); }
        fun probe() {
            print("start");
            run(|| {
                let r = Res { tag = "c" };
                let value = 7;
                print(r.tag);
                print(value);
                print("closure-end");
            });
            print("after");
        }
        fun main() {
            probe();
        }
        "#,
        "start\nc\n7\ndrop c\nclosure-end\nafter\n",
    );
}

// ============================================================================
// C11 — the resource temporary (`temporary-drop.md`, RULED 2026-08-28)
// ============================================================================
//
// A resource born and consumed inside one expression was neither dropped nor
// rejected: `collect_resource_bindings` enrolls BINDINGS, and a temporary has
// no binding to enroll. It is now owned by the STATEMENT that constructs it —
// the special case of S3's general rule, since a temporary's last use is its
// statement — and lowered the same way a `let` is: a minted `const` outside the
// `try`, the rest of the statement inside it, the destructor in the `finally`.
//
// The one shape with no statement to own it — a constructor on the right of a
// short-circuit — is refused rather than flagged (§7.3), which is the single
// place this arc turns a compiling program into an error.

#[test]
fn a_temporary_receiver_drops_at_its_statements_end() {
    // THE C11 shape: a postfix straight off a constructor call. The handle has
    // no name, so the lowering gives it one and destroys it before the next
    // statement runs.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        impl Res { fun size(&self): i32 { 3 } }
        fun make(tag: str): Res { Res { tag = tag } }
        fun main() {
            print(make("t").size());
            print("after");
        }
        "#,
        "3\ndrop t\nafter\n",
    );
}

#[test]
fn two_temporaries_in_one_statement_drop_in_reverse_construction_order() {
    // §7.1's ordering clause: within a statement the temporaries discharge in
    // reverse birth order, which the nested `finally`s give for free.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        impl Res { fun size(&self): i32 { 3 } }
        fun make(tag: str): Res { Res { tag = tag } }
        fun main() {
            print(make("first").size() + make("second").size());
            print("after");
        }
        "#,
        "6\ndrop second\ndrop first\nafter\n",
    );
}

#[test]
fn a_temporary_drops_before_a_later_statements_temporary_is_built() {
    // The property statement-end buys and scope-end does not: the live count
    // returns to its baseline inside the scope, so N temporaries in a
    // straight-line scope hold ONE resource at a time, not N (P7).
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        impl Res { fun size(&self): i32 { 3 } }
        fun make(tag: str): Res { Res { tag = tag } }
        fun main() {
            print(make("one").size());
            print(make("two").size());
            print(make("three").size());
        }
        "#,
        "3\ndrop one\n3\ndrop two\n3\ndrop three\n",
    );
}

#[test]
fn a_temporary_in_a_loop_body_drops_each_iteration() {
    // P9: peak one, whatever the iteration count.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        impl Res { fun size(&self): i32 { 3 } }
        fun make(tag: str): Res { Res { tag = tag } }
        fun main() {
            for round in [ 1, 2 ] {
                print(make("t").size());
            }
            print("after");
        }
        "#,
        "3\ndrop t\n3\ndrop t\nafter\n",
    );
}

#[test]
fn a_temporary_bound_into_a_let_still_drops_and_leaves_the_name_readable() {
    // The statement is itself a declaration, so the region would close over the
    // very name the next statement reads. The declaration is split — `let n;`
    // ahead of the region, the assignment inside it — and both halves hold: the
    // handle is destroyed, and `n` survives.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        impl Res { fun size(&self): i32 { 3 } }
        fun make(tag: str): Res { Res { tag = tag } }
        fun main() {
            let n = make("t").size();
            print("between");
            print(n);
        }
        "#,
        "drop t\nbetween\n3\n",
    );
}

#[test]
fn a_temporary_drops_when_its_statement_throws() {
    // P8: the temporary rides a `finally` exactly as a `let` does, so a caught
    // mid-statement throw releases it instead of leaking it permanently.
    let (stdout, _stderr, code) = compile_and_run_status(
        r#"
        import std::print;
        import std::panic;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        impl Res { fun boom(&self): i32 { panic("mid-statement"); 0 } }
        fun make(tag: str): Res { Res { tag = tag } }
        fun main() {
            print("before");
            print(make("t").boom());
        }
        "#,
    );
    assert_eq!(
        stdout, "before\ndrop t\n",
        "the temporary must still release"
    );
    assert_ne!(code, 0, "the panic must still leave a failing exit");
}

#[test]
fn a_temporary_moved_into_an_own_parameter_is_not_dropped_twice() {
    // P5 line 1, THE critical negative. A temporary moved into an `own`
    // parameter belongs to the callee, which destroys it at ITS scope end; the
    // caller's statement must not destroy it as well.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun make(tag: str): Res { Res { tag = tag } }
        fun sink(own r: Res) { print(i"in-sink {r.tag}"); }
        fun main() {
            sink(make("t"));
            print("after");
        }
        "#,
        "in-sink t\ndrop t\nafter\n",
    );
}

// --- The predicate at its ruled width (B153 unlocked the widening) -----------
//
// C11 shipped with the predicate NARROWED to `&` / `&mut` / a bare `self`
// receiver, because `Option::replace` declared a bare `value: T` and stored it:
// a temporary recorded there would have been destroyed under the callee. With
// that declaration corrected the spec's own rule holds — only `own` moves — so
// a BARE non-`self` parameter is a loan like any other and a temporary in one
// belongs to the statement.

#[test]
fn a_temporary_in_a_bare_loan_parameter_drops_at_its_statements_end() {
    // R3's general rule, restored: `handle: Res` is a loan, the caller still
    // owns the value after the call, and nobody bound it — so the statement
    // does. Under the narrowed predicate this leaked: no drop ran at all.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun peek(handle: Res): str { handle.tag }
        fun main() {
            print(peek(Res { tag = "t" }));
            print("after");
        }
        "#,
        "t\ndrop t\nafter\n",
    );
}

#[test]
fn a_temporary_in_a_bare_extern_parameter_drops_at_its_statements_end() {
    // The same position at an `[extern]`, which is where B153 lived: a
    // declaration with no body to read, so the convention is the only thing
    // that speaks. Unmarked, the host's read is call-bounded, and the statement
    // releases the handle straight after.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }

        [extern("Boolean")]
        external fun watch(r: Res): bool;

        fun main() {
            print(watch(Res { tag = "t" }));
            print("after");
        }
        "#,
        "true\ndrop t\nafter\n",
    );
}

#[test]
fn a_temporary_handed_to_a_retaining_extern_is_left_to_the_host() {
    // The exemption the widening has to carry. `retains` says the host keeps
    // what it is handed past the call, so the statement must NOT destroy it —
    // the argument has no binding whose scope could hold it open, and freeing
    // it at the statement's end would hand the host a dead value. It leaks
    // instead, which is the direction this rule always fails.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }

        [extern("Boolean", retains)]
        external fun keep(r: Res): bool;

        fun main() {
            print(keep(Res { tag = "t" }));
            print("after");
        }
        "#,
        "true\nafter\n",
    );
}

#[test]
fn a_temporary_moved_into_the_drop_sink_is_not_dropped_twice() {
    // P5 line 2: B68's sink already destroys an unbound resource value at the
    // site. Recording it as a temporary too would destroy it twice.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        import std::drop::drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        fun make(tag: str): Res { Res { tag = tag } }
        fun main() {
            drop(make("t"));
            print("after");
        }
        "#,
        "drop t\nafter\n",
    );
}

#[test]
fn a_temporary_returned_by_ret_is_not_dropped() {
    // P5 line 3: a value moved OUT of a helper belongs to the caller's binding.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        impl Res { fun size(&self): i32 { 3 } }
        fun make(tag: str): Res { ret Res { tag = tag }; }
        fun main() {
            let held = make("t");
            print(held.size());
            print("after");
        }
        "#,
        "3\ndrop t\nafter\n",
    );
}

#[test]
fn a_temporary_bound_by_a_let_is_not_a_temporary() {
    // The control that separates the two rules: binding it is how the language
    // says "keep this", and a bound handle takes S3's last-use teardown rather
    // than the statement's.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        impl Res { fun size(&self): i32 { 3 } }
        fun make(tag: str): Res { Res { tag = tag } }
        fun main() {
            let held = make("t");
            print("between");
            print(held.size());
            print("after");
        }
        "#,
        "between\n3\ndrop t\nafter\n",
    );
}

#[test]
fn a_conditionally_constructed_resource_temporary_is_refused() {
    // §7.3, RULED. The right of `&&` runs only on some paths, so no statement
    // can own the handle: refuse rather than admit v1's first runtime drop
    // flag. A refusal of the SPELLING — the message names the fix.
    assert_fails_with(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        impl Res { fun size(&self): i32 { 3 } }
        fun make(tag: str): Res { Res { tag = tag } }
        fun main() {
            let ready = true;
            print(ready && make("t").size() > 0);
        }
        "#,
        "would be created only on some paths",
    );
}

#[test]
fn the_conditional_temporarys_refusal_names_binding_as_the_fix() {
    // The fix the message steers to compiles and releases: one keystroke's
    // worth of restructuring, exactly the shape R7 already asks for.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        impl Res { fun size(&self): i32 { 3 } }
        fun make(tag: str): Res { Res { tag = tag } }
        fun main() {
            let ready = true;
            let handle = make("t");
            print(ready && handle.size() > 0);
        }
        "#,
        "true\ndrop t\n",
    );
}

#[test]
fn a_temporary_on_the_left_of_a_short_circuit_is_accepted() {
    // The left operand always runs, so it has a statement to belong to and
    // needs no refusal — the rule is about CONDITIONAL evaluation, not about
    // the operator.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        impl Res { fun size(&self): i32 { 3 } }
        fun make(tag: str): Res { Res { tag = tag } }
        fun main() {
            let ready = true;
            print(make("t").size() > 0 && ready);
        }
        "#,
        "true\ndrop t\n",
    );
}

#[test]
fn a_temporary_in_a_branch_arm_drops_inside_that_arm() {
    // Narrower than `temporary-drop.md` §7.3's stated refusal set on purpose:
    // an arm lowers to a JS BLOCK with its own statement list, so a temporary
    // there has a statement position of its own. Only `&&`/`||` evaluate an
    // operand inline with no block to hold it.
    let program = |taken: &str| {
        format!(
            r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res {{ tag: str }}
        impl Res with Drop {{ fun drop(&mut self) {{ print(i"drop {{self.tag}}"); }} }}
        impl Res {{ fun size(&self): i32 {{ 3 }} }}
        fun make(tag: str): Res {{ Res {{ tag = tag }} }}
        fun main() {{
            if {taken} {{
                print(make("t").size());
            }}
            print("join");
        }}
        "#
        )
    };
    assert_compiles_and_runs(&program("true"), "3\ndrop t\njoin\n");
    assert_compiles_and_runs(&program("false"), "join\n");
}

#[test]
fn a_temporary_in_a_tail_position_drops_after_the_value_is_computed() {
    // P11's shape: the return value must exist before any teardown runs, which
    // is what a `finally` around the `return` gives.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }
        impl Res { fun size(&self): i32 { 3 } }
        fun make(tag: str): Res { Res { tag = tag } }
        fun measure(): i32 { make("t").size() }
        fun main() {
            print(measure());
            print("after");
        }
        "#,
        "drop t\n3\nafter\n",
    );
}

#[test]
fn a_temporary_of_a_resource_with_no_destructor_is_not_lifted() {
    // The byte-identity clause: a `resource` whose destruction is a complete
    // no-op gets no `const`, no `try` and no `finally` — the same rule that
    // keeps a bare `resource external` binding's scope free of them.
    let js = compile(
        r#"
        resource struct Inert { tag: str }
        impl Inert { fun size(&self): i32 { 3 } }
        fun make(tag: str): Inert { Inert { tag = tag } }
        fun main() { make("t").size(); }
        "#,
    )
    .expect("compiles");
    assert!(
        !js.contains("finally"),
        "a no-op destruction must not grow a teardown:\n{js}"
    );
}

// ============================================================================
// S4 — the extern retention contract (`lifetimes.md` §6.4, RULED 2026-08-28)
// ============================================================================
//
// The one place the probe battery showed last-use is WRONG: an `[extern]` that
// stashes what it is handed and reads it after the call returns. The rule is
// that an extern loan is CALL-BOUNDED unless the declaration says otherwise,
// and `[extern(…, retains)]` is what says otherwise — it extends the argument's
// liveness to the binding's whole scope, which is the conservative envelope and
// also the teardown that shipped.
//
// The spelling settles as a trailing FLAG rather than a form word, so it
// composes with every existing binding shape (`[extern(method, "…", retains)]`)
// instead of needing an arm per combination; the formatter reprints it last.

#[test]
fn an_unmarked_extern_loan_is_call_bounded() {
    // The rule's default half, and the red-first half of the pair: nothing in
    // the declaration says the host keeps the value, so the binding is released
    // at its last use like any other.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }

        [extern("Boolean")]
        external fun watch(r: &Res): bool;

        fun main() {
            let r = Res { tag = "r" };
            print(watch(&r));
            print("after");
        }
        "#,
        "true\ndrop r\nafter\n",
    );
}

#[test]
fn a_retaining_extern_holds_its_argument_to_the_bindings_scope_end() {
    // The same program with the contract declared. The host may read the value
    // at any point after the call, so the compiler stops claiming to know when
    // the last read was: the binding falls back to the scope-end teardown.
    // Under the call bound alone this is where the probe battery read a freed
    // value host-side (`tag=["<FREED>"]`).
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::drop::Drop;
        resource struct Res { tag: str }
        impl Res with Drop { fun drop(&mut self) { print(i"drop {self.tag}"); } }

        [extern("Boolean", retains)]
        external fun keep(r: &Res): bool;

        fun main() {
            let r = Res { tag = "r" };
            print(keep(&r));
            print("after");
        }
        "#,
        "true\nafter\ndrop r\n",
    );
}

#[test]
fn a_retaining_extern_composes_with_every_binding_form() {
    // The flag is not a form word: it rides alongside `method` / `get` / `set` /
    // `new` and the plain symbol shapes alike, which is why it is stripped
    // before the positional match rather than adding an arm per combination.
    assert_compiles(
        r#"
        external struct Host;
        impl Host {
            [extern(method, "addEventListener", retains)]
            external fun on(self, event: str, handler: || void): void;

            [extern(set, "onmessage", retains)]
            external fun set_on_message(self, handler: || void);
        }
        [extern("queueMicrotask", retains)]
        external fun queue(callback: || void);
        fun main() {}
        "#,
    );
}
