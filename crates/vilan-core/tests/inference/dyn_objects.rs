//! A124 R3 / B4 reopened — `dyn Trait`, the TRAIT OBJECT over an object-safe
//! core.
//!
//! The ruling (2026-09-22): a `dyn` over a trait whose requirements can each
//! take a table slot, written with an explicit keyword, holding no resource —
//! and a bare `Trait` at a struct FIELD refused with a steer to `dyn Trait`
//! (B184's hidden generic withdrawn at that position). `trait-objects.md`
//! §4 (object safety), §6 (representation), §7 (positions and coercion),
//! §8 (resources), §9.1 (B57's winners) are the design; this module holds it.
//!
//! The pins are written so each RULE reds on its own — object safety per
//! disqualifier, the coercion's direction each way, the resource refusal, the
//! per-member reachability — rather than one program exercising all of them.
//!
//! One subject module of the `inference` test binary; the harness is
//! `support.rs`.

use crate::support::*;

/// The shape the probes started from
/// (`sweeps/order39/probes/a124_d_dyn.vl`): `dyn` did not lex, so the
/// parameter was a parse error. It parses, resolves, and dispatches.
#[test]
fn a_dyn_parameter_reads_through_the_object() {
    assert_compiles_and_runs(
        "trait Src<T> { fun get(self): T; }
struct Root { v: i32 }
impl Root with Src<i32> { fun get(self): i32 { self.v } }
fun inspect(source: dyn Src<i32>): i32 { source.get() }
fun main() {
\tlet root = Root { v = 5 };
\tprint(i\"{inspect(root)}\");
}
",
        "5\n",
    );
}

/// `sweeps/order39/probes/a124_g_mixed_field.vl`, whose refusal is the whole
/// reason the field position needed an answer: a `Holder` over a root and a
/// `Holder` over a mapped node were `Holder<Root>` and `Holder<Dbl<Root>>`,
/// and one list of both was refused. As a `dyn` field they are one type.
#[test]
fn a_dyn_field_holds_two_node_types_in_one_list() {
    assert_compiles_and_runs(
        "trait Src<T> { fun get(self): T; }
struct Root { v: i32 }
struct Dbl<S> { up: S }
impl Root with Src<i32> { fun get(self): i32 { self.v } }
impl Dbl<type S: Src<i32>> with Src<i32> { fun get(self): i32 { self.up.get() * 2 } }
struct Holder { s: dyn Src<i32> }
fun main() {
\tlet h1 = Holder { s = Root { v = 1 } };
\tlet h2 = Holder { s = Dbl<Root> { up = Root { v = 1 } } };
\tlet both: List<Holder> = [h1, h2];
\tprint(i\"{both.len()}\");
\tfor h in both { print(i\"{h.s.get()}\"); }
}
",
        "2\n1\n2\n",
    );
}

/// The same shape over std's own `Source<T>` — the one A124 R3 rules in. A
/// root and a derived cell in one field type, live across a `set`.
#[test]
fn a_dyn_source_field_holds_a_root_and_a_derived_cell() {
    assert_compiles_and_runs(
        "import std::reactive::{ Source, SignalCell };
struct Holder { s: dyn Source<i32> }
fun main() {
\tlet root = SignalCell::new(1);
\tlet mapped = root.map(|n| n + 10);
\tlet hs: List<Holder> = [ Holder { s = root }, Holder { s = mapped } ];
\tfor h in hs { print(i\"{h.s.get()}\"); }
\troot.set(5);
\tfor h in hs { print(i\"{h.s.get()}\"); }
}
",
        "1\n11\n5\n15\n",
    );
}

/// A `dyn` in a BINDING's annotation is a type, not B161's constraint: the
/// binding holds the object, and the object's member is what a call reaches.
#[test]
fn a_dyn_binding_annotation_erases() {
    assert_compiles_and_runs(
        "trait Src { fun get(self): i32; }
struct Root { v: i32 }
impl Root with Src { fun get(self): i32 { self.v } }
fun main() {
\tlet b: dyn Src = Root { v = 7 };
\tprint(i\"{b.get()}\");
}
",
        "7\n",
    );
}

/// §5 of the lane's brief — the blanket, which is written nowhere. A body
/// generic over `S: Src<i32>` binds `S` to the object and its `s.get()` goes
/// through the table, so every blanket over the bound applies to the object.
#[test]
fn a_blanket_over_the_bound_applies_to_the_object() {
    assert_compiles_and_runs(
        "trait Src<T> { fun get(self): T; }
struct Root { v: i32 }
impl Root with Src<i32> { fun get(self): i32 { self.v } }
fun twice<S: Src<i32>>(s: S): i32 { s.get() * 2 }
fun main() {
\tlet b: dyn Src<i32> = Root { v = 7 };
\tprint(i\"{twice(b)}\");
}
",
        "14\n",
    );
}

/// §4's first disqualifier, reported at the `dyn` and naming the MEMBER —
/// §4's own recommendation, because naming the trait sends the reader to read
/// fifteen signatures.
#[test]
fn a_static_requirement_disqualifies_the_trait() {
    assert_fails_noting(
        "trait Maker { fun make(): i32; fun get(self): i32; }
struct A { v: i32 }
impl A with Maker { fun make(): i32 { 1 } fun get(self): i32 { self.v } }
fun main() { let x: dyn Maker = A { v = 1 }; print(i\"{x.get()}\"); }
",
        "`Maker::make` is a static",
        "make",
        "'make' is declared here",
    );
}

/// §4's second disqualifier.
#[test]
fn a_self_returning_requirement_disqualifies_the_trait() {
    assert_fails_with(
        "trait Same { fun get(self): i32; fun clone_me(self): Self; }
struct A { v: i32 }
impl A with Same { fun get(self): i32 { self.v } fun clone_me(self): A { A { v = self.v } } }
fun main() { let x: dyn Same = A { v = 1 }; print(i\"{x.get()}\"); }
",
        "`Same::clone_me` returns `Self`",
    );
}

/// §4's third disqualifier.
#[test]
fn a_generic_requirement_disqualifies_the_trait() {
    assert_fails_with(
        "trait Gen { fun get(self): i32; fun pick<U>(self, u: U): U; }
struct A { v: i32 }
impl A with Gen { fun get(self): i32 { self.v } fun pick<U>(self, u: U): U { u } }
fun main() { let x: dyn Gen = A { v = 1 }; print(i\"{x.get()}\"); }
",
        "`Gen::pick` is generic",
    );
}

/// §4's census counted a `Self` PARAMETER object-safe (`PartialEq` is in its
/// 22). It cannot be: two objects of one trait need not erase the same type,
/// so nothing can supply the argument. Refused, and flagged in the lane's
/// report as beyond what §4 priced.
#[test]
fn a_self_parameter_requirement_disqualifies_the_trait() {
    assert_fails_with(
        "trait Alike { fun get(self): i32; fun same(self, other: Self): bool; }
struct A { v: i32 }
impl A with Alike { fun get(self): i32 { self.v } fun same(self, other: A): bool { self.v == other.v } }
fun main() { let x: dyn Alike = A { v = 1 }; print(i\"{x.get()}\"); }
",
        "takes `Self` in its `other` parameter",
    );
}

/// A trait whose SUPERTRAIT is not object-safe is not object-safe either: an
/// object over it would have to dispatch that member too.
#[test]
fn an_unsafe_supertrait_disqualifies_the_subtrait() {
    assert_fails_with(
        "trait Base { fun make(): i32; }
trait Sub with Base { fun get(self): i32; }
struct A { v: i32 }
impl A with Base { fun make(): i32 { 1 } }
impl A with Sub { fun get(self): i32 { self.v } }
fun main() { let x: dyn Sub = A { v = 1 }; print(i\"{x.get()}\"); }
",
        "`Base::make` is a static",
    );
}

/// A DEFAULT member never disqualifies the trait: it is the trait's own code
/// over the requirements, not a slot. This is the one place §4's census — which
/// classified all 96 members rather than the requirements — is narrowed, and
/// it is what makes std's `Source<T>` (six defaults, one of them `map<U>`) an
/// object at all.
#[test]
fn a_generic_default_does_not_disqualify_the_trait() {
    assert_compiles_and_runs(
        "trait Src { fun get(self): i32; fun twice<U>(self, u: U): U { u } }
struct A { v: i32 }
impl A with Src { fun get(self): i32 { self.v } }
fun main() { let x: dyn Src = A { v = 1 }; print(i\"{x.get()}\"); }
",
        "1\n",
    );
}

/// ...and it is still not reachable THROUGH the object. The per-call refusal
/// and the object-safety check share one filter, so a member with no slot is a
/// member no call can reach.
#[test]
fn a_generic_default_is_not_reachable_through_the_object() {
    assert_fails_with(
        "trait Src { fun get(self): i32; fun twice<U>(self, u: U): U { u } }
struct A { v: i32 }
impl A with Src { fun get(self): i32 { self.v } }
fun main() { let x: dyn Src = A { v = 1 }; print(i\"{x.twice(2)}\"); }
",
        "`Src::twice` is generic, so it is not reachable through `dyn Src`",
    );
}

/// A NON-generic default is reachable, and runs the concrete type's answer —
/// §9.1/Q6's rule: the table is built from B57's winners, not from the trait's
/// declarations, so a member an impl overrides dispatches to the override.
#[test]
fn a_default_the_impl_overrides_dispatches_to_the_override() {
    assert_compiles_and_runs(
        "trait Src { fun get(self): i32; fun label(self): str { \"trait\" } }
struct A { v: i32 }
struct B { v: i32 }
impl A with Src { fun get(self): i32 { self.v } fun label(self): str { \"impl\" } }
impl B with Src { fun get(self): i32 { self.v } }
fun main() {
\tlet a = A { v = 1 };
\tlet b = B { v = 2 };
\tlet xs: List<dyn Src> = [ a, b ];
\tfor x in xs { print(x.label()); }
}
",
        "impl\ntrait\n",
    );
}

/// Q5, RULED NO in this scope (§8.3): a resource is refused AT THE COERCION,
/// which is the last point at which the destructor is still known. Without it,
/// "a `dyn` is never a resource" would be §2.2's destructor suppression
/// wearing a keyword.
#[test]
fn a_resource_cannot_be_erased() {
    assert_fails_with(
        "trait Shown { fun get(self): i32; }
resource struct Handle { v: i32 }
impl Handle with Shown { fun get(self): i32 { self.v } }
fun main() { let h = Handle { v = 1 }; let x: dyn Shown = h; print(i\"{x.get()}\"); }
",
        "is a resource, so it cannot become a `dyn Shown`",
    );
}

/// Q4/P15: the coercion is EXPLICIT and positional. Two concrete types that
/// share a trait do not meet in an object on their own — the list's element
/// type is inferred from the first element, and the second is a mismatch.
#[test]
fn two_concrete_types_do_not_meet_in_an_object_implicitly() {
    assert_fails(
        "trait Shown { fun get(self): i32; }
struct A { v: i32 }
struct B { v: i32 }
impl A with Shown { fun get(self): i32 { self.v } }
impl B with Shown { fun get(self): i32 { self.v } }
fun main() {
\tlet xs = [ A { v = 1 }, B { v = 2 } ];
\tprint(i\"{xs.len()}\");
}
",
    );
}

/// ...and an ANNOTATED element type is what makes them meet.
#[test]
fn an_annotated_element_type_is_where_two_types_meet() {
    assert_compiles_and_runs(
        "trait Shown { fun get(self): i32; }
struct A { v: i32 }
struct B { v: i32 }
impl A with Shown { fun get(self): i32 { self.v } }
impl B with Shown { fun get(self): i32 { self.v } }
fun main() {
\tlet a = A { v = 1 };
\tlet b = B { v = 2 };
\tlet xs: List<dyn Shown> = [ a, b ];
\tfor x in xs { print(i\"{x.get()}\"); }
}
",
        "1\n2\n",
    );
}

/// The coercion's other direction: an object does not narrow back. `dyn` is
/// where the concrete type went, so a concrete position refuses it — which is
/// checked at the LANDING, because `reconcile_type` is a unifier and a call
/// reconciles parameter-first while every other position reconciles
/// value-first.
#[test]
fn an_object_does_not_narrow_back_to_the_type_it_erased() {
    assert_fails_with(
        "trait Shown { fun get(self): i32; }
struct A { v: i32 }
impl A with Shown { fun get(self): i32 { self.v } }
struct H { s: dyn Shown }
fun take(a: A): i32 { a.v }
fun main() { let h = H { s = A { v = 1 } }; print(i\"{take(h.s)}\"); }
",
        "an object does not narrow back to the type it erased",
    );
}

/// `dyn` erases a TRAIT. A struct after the keyword names no member surface.
#[test]
fn dyn_over_a_struct_is_refused_by_sort() {
    assert_fails_with(
        "struct A { v: i32 }
fun main() { let x: dyn A = A { v = 1 }; print(i\"{x.v}\"); }
",
        "`dyn` erases a TRAIT, and `A` is a struct",
    );
}

/// The grammar takes a path and nothing else after the keyword.
#[test]
fn dyn_over_a_closure_type_does_not_parse() {
    assert_fails(
        "fun main() { let x: dyn |i32| str = 1; print(i\"{x}\"); }
",
    );
}

/// A value that does not implement the trait is refused at the coercion, like
/// any other mismatch — the object is a type, and this is its type error.
#[test]
fn a_value_without_the_impl_cannot_be_erased() {
    assert_fails(
        "trait Shown { fun get(self): i32; }
struct A { v: i32 }
fun main() { let x: dyn Shown = A { v = 1 }; print(i\"{x.get()}\"); }
",
    );
}

/// A member the trait does not declare is not on the object either: the
/// concrete type's own inherent surface is gone.
#[test]
fn an_inherent_member_is_not_on_the_object() {
    assert_fails(
        "trait Shown { fun get(self): i32; }
struct A { v: i32 }
impl A { fun extra(self): i32 { 9 } }
impl A with Shown { fun get(self): i32 { self.v } }
fun main() { let x: dyn Shown = A { v = 1 }; print(i\"{x.extra()}\"); }
",
    );
}

/// A `dyn` nests wherever a type does, and the coercion happens at each
/// element: the argument to a generic function whose parameter is a
/// `List<dyn ..>` is checked element-wise.
#[test]
fn an_object_passes_through_a_nested_position() {
    assert_compiles_and_runs(
        "trait Shown { fun get(self): i32; }
struct A { v: i32 }
struct B { v: i32 }
impl A with Shown { fun get(self): i32 { self.v } }
impl B with Shown { fun get(self): i32 { self.v * 10 } }
fun total(xs: List<dyn Shown>): i32 {
\tmut sum = 0;
\tfor x in xs { sum = sum + x.get(); }
\tsum
}
fun main() {
\tlet a = A { v = 1 };
\tlet b = B { v = 2 };
\tlet xs: List<dyn Shown> = [ a, b ];
\tprint(i\"{total(xs)}\");
}
",
        "21\n",
    );
}

/// The receiver is read TWICE at a table call (once for the table, once for
/// the value), so one whose evaluation is observable must be bound first. The
/// pin is the observation: a call in receiver position runs ONCE.
#[test]
fn an_impure_receiver_is_evaluated_once() {
    assert_compiles_and_runs(
        "trait Shown { fun get(self): i32; }
struct A { v: i32 }
impl A with Shown { fun get(self): i32 { self.v } }
fun make(): dyn Shown {
\tprint(\"made\");
\tA { v = 3 }
}
fun main() {
\tlet v = make().get();
\tprint(i\"{v}\");
}
",
        "made\n3\n",
    );
}
