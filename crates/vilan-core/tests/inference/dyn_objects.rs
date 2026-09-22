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

/// The brief's own pin, over std's `Source<i32>` with all THREE kinds of node
/// in one `List<Holder>`: the root cell, a COLD node written in the program
/// (a struct over its upstream that owns no value and pulls through `get`),
/// and an eager mapped cell. The second slot, `on_change`, is exercised
/// through the object as well as the first: the cold node's subscription is
/// taken through its `dyn` and reports the settled value.
#[test]
fn a_dyn_source_field_holds_a_root_a_cold_node_and_a_cell() {
    assert_compiles_and_runs(
        "import std::reactive::{ Source, SignalCell, Subscription };
struct Plus { up: dyn Source<i32>, k: i32 }
impl Plus with Source<i32> {
\tfun get(self): i32 { self.up.get() + self.k }
\tfun on_change(self, observer: |i32| void): Subscription {
\t\tlet k = self.k;
\t\tself.up.on_change(|n| observer(n + k))
\t}
}
struct Holder { s: dyn Source<i32> }
fun main() {
\tlet root = SignalCell::new(1);
\tlet cold = Plus { up = root, k = 100 };
\tlet mapped = root.map(|n| n * 10);
\tlet hs: List<Holder> = [ Holder { s = root }, Holder { s = cold }, Holder { s = mapped } ];
\tfor h in hs { print(h.s.get()); }
\tlet watch = hs[1].s.on_change(|n| print(i\"cold saw {n}\"));
\troot.set(5);
\tfor h in hs { print(h.s.get()); }
\twatch.dispose();
}
",
        "1\n101\n10\ncold saw 105\n5\n105\n50\n",
    );
}

/// An object is a VALUE: copying one copies what it erased. `let b = a` and
/// a list element read into a `mut` binding are rule 1's copies exactly as
/// they are for the concrete type — without them a `&mut self` member called
/// through one binding wrote through every copy (`2 2` / `6 6`), where the
/// same program over the concrete struct prints `2 1` / `6 5`.
#[test]
fn a_copied_object_does_not_alias_its_original() {
    assert_compiles_and_runs(
        "trait Counter { fun get(self): i32; fun bump(&mut self): void; }
struct C { n: i32 }
impl C with Counter {
\tfun get(self): i32 { self.n }
\tfun bump(&mut self): void { self.n = self.n + 1; }
}
fun main() {
\tmut a: dyn Counter = C { n = 1 };
\tlet b = a;
\ta.bump();
\tprint(i\"{a.get()} {b.get()}\");
\tlet list: List<dyn Counter> = [C { n = 5 }];
\tmut c = list[0];
\tc.bump();
\tprint(i\"{c.get()} {list[0].get()}\");
}
",
        "2 1\n6 5\n",
    );
}

/// trait-objects.md §5 (i): the declaration binds in object position. B29
/// lets an impl be async under a sync declaration because every generic
/// dispatch is monomorphized; a call through an object is emitted once,
/// against the declaration, so it would not await and printed
/// `Promise { <pending> }`. Refused at the coercion, naming the member.
#[test]
fn an_async_implementation_under_a_sync_declaration_cannot_be_erased() {
    assert_fails_with(
        "trait Fetch { fun get(self): str; }
struct Remote { u: str }
impl Remote with Fetch { async fun get(self): str { \"remote\" } }
fun show(f: dyn Fetch) { print(f.get()); }
fun main() { show(Remote { u = \"a\" }); }
",
        "`Remote`'s `get` is async, but `Fetch::get` is declared sync",
    );
}

/// The other disagreement is sound: an ASYNC declaration's call through an
/// object awaits — both through the object directly and through a generic
/// body and a blanket impl whose parameter bound to it — and a sync
/// implementation behind it is a plain value, which awaiting leaves alone.
#[test]
fn an_async_declaration_awaits_through_the_object() {
    assert_compiles_and_runs(
        "trait Fetch { async fun get(self): str; }
struct Remote { u: str }
impl Remote with Fetch { async fun get(self): str { \"remote\" } }
struct Local { u: str }
impl Local with Fetch { fun get(self): str { \"local\" } }
fun twice<F: Fetch>(f: F): str { f.get() + f.get() }
impl type S: Fetch { fun loud(self): str { self.get() + \"!\" } }
fun show(f: dyn Fetch) { print(f.get()); }
fun main() {
\tshow(Remote { u = \"a\" });
\tshow(Local { u = \"b\" });
\tlet a: dyn Fetch = Remote { u = \"a\" };
\tlet b: dyn Fetch = Local { u = \"b\" };
\tprint(twice(a));
\tprint(twice(b));
\tprint(a.loud());
\tprint(b.loud());
}
",
        "remote\nlocal\nremoteremote\nlocallocal\nremote!\nlocal!\n",
    );
}

/// §5 of the brief, for a blanket IMPL — `impl type S: Src { .. }` — rather
/// than a generic function: the object satisfies the bound, `S` binds to it,
/// and the body's `self.get()` goes through the table. Two concrete types
/// implement the trait, which is the case the bound's impl ranking used to
/// refuse as ambiguous: an object's bound is met by its table, not by
/// choosing between the impls of the types it may hold.
#[test]
fn a_blanket_impl_applies_to_the_object() {
    assert_compiles_and_runs(
        "trait Src { fun get(self): i32; }
struct Root { v: i32 }
impl Root with Src { fun get(self): i32 { self.v } }
struct Dbl { v: i32 }
impl Dbl with Src { fun get(self): i32 { self.v * 2 } }
impl type S: Src { fun plus_one(self): i32 { self.get() + 1 } }
fun twice<S: Src>(s: S): i32 { s.get() * 2 }
fun main() {
\tlet all: List<dyn Src> = [Root { v = 4 }, Dbl { v = 4 }];
\tfor s in all { print(i\"{s.plus_one()} {twice(s)}\"); }
}
",
        "5 8\n9 16\n",
    );
}

/// std's own blankets over `S: Source<..>` reach an object: `flatten` over a
/// cell holding a `dyn Source<i32>` follows the inner source through the table.
#[test]
fn a_std_blanket_flattens_through_the_object() {
    assert_compiles_and_runs(
        "import std::reactive::{ Source, SignalCell };
fun main() {
\tlet cell = SignalCell::new(1);
\tlet inner: dyn Source<i32> = cell;
\tlet outer = SignalCell::new(inner);
\tlet flat = outer.flatten();
\tprint(flat.get());
\tcell.set(3);
\tprint(flat.get());
}
",
        "1\n3\n",
    );
}

/// What the ERASED value implements besides the object's trait is gone with
/// its type: another trait's member, and another trait's default, are not on
/// the object. Both used to resolve — the default through the ordinary
/// inherited-default tier — and were emitted as table calls the table had no
/// slot for (`r[1].hello is not a function`).
#[test]
fn another_trait_of_the_erased_value_is_not_on_the_object() {
    let source = |call: &str| {
        format!(
            "trait Src {{ fun get(self): i32; }}
struct Root {{ v: i32 }}
impl Root with Src {{ fun get(self): i32 {{ self.v }} }}
trait Named {{ fun name(self): str; fun hello(self): str {{ \"hi \" + self.name() }} }}
impl Root with Named {{ fun name(self): str {{ \"root\" }} }}
fun main() {{
\tlet r: dyn Src = Root {{ v = 4 }};
\tprint({call});
}}
"
        )
    };
    assert_fails_with(&source("r.name()"), "dyn Src has no method 'name'");
    assert_fails_with(&source("r.hello()"), "dyn Src has no method 'hello'");
}

/// The same fact at a BOUND: an object meets the trait it was erased to (and
/// its supertraits), not a trait its erased value happens to implement. The
/// bound check used to reconcile the object against every concrete impl
/// subject and admitted it, then called a `name` slot the table never had.
#[test]
fn an_object_does_not_meet_a_bound_its_erased_value_meets() {
    assert_fails_with(
        "trait Src { fun get(self): i32; }
struct Root { v: i32 }
impl Root with Src { fun get(self): i32 { self.v } }
trait Named { fun name(self): str; }
impl Root with Named { fun name(self): str { \"root\" } }
fun greet<T: Named>(x: T): str { x.name() }
fun main() {
\tlet r: dyn Src = Root { v = 4 };
\tprint(greet(r));
}
",
        "'dyn Src' does not implement trait 'Named'",
    );
}

/// A blanket that gives an object a SECOND trait (`impl type S: Src with
/// Loud`), reached both directly and through a generic bound on that second
/// trait. The table holds `Src`'s members only, so a `Loud` call is the
/// blanket's body with `S` bound to the object — the generic-bound path used
/// to read it out of the table (`t[1].loud is not a function`).
#[test]
fn a_blanket_trait_impl_reaches_the_object_through_a_bound() {
    assert_compiles_and_runs(
        "trait Src { fun get(self): i32; }
struct Root { v: i32 }
impl Root with Src { fun get(self): i32 { self.v } }
trait Loud { fun loud(self): str; }
impl type S: Src with Loud { fun loud(self): str { i\"{self.get()}!\" } }
fun shout<T: Loud>(t: T): str { t.loud() }
fun main() {
\tlet r: dyn Src = Root { v = 4 };
\tprint(r.loud());
\tprint(shout(r));
\tprint(shout(Root { v = 5 }));
}
",
        "4!\n4!\n5!\n",
    );
}

/// A body reached ONLY through a table still belongs to the program. The call
/// graph recorded a call through an object as a direct call to the trait's
/// declaration, so `SignalCell::on_change` — reached here through the cold
/// node's `dyn Source` upstream and nowhere else — was invisible to the
/// reachability that decides which module-level bindings are emitted, and the
/// subscriber registry it reads was pruned: `ReferenceError:
/// next_subscriber_id is not defined` at the first subscription.
#[test]
fn a_body_reached_only_through_an_object_keeps_its_module_state() {
    assert_compiles_and_runs(
        "import std::reactive::{ Source, SignalCell, Subscription };
struct Plus { up: dyn Source<i32>, k: i32 }
impl Plus with Source<i32> {
\tfun get(self): i32 { self.up.get() + self.k }
\tfun on_change(self, observer: |i32| void): Subscription {
\t\tlet k = self.k;
\t\tself.up.on_change(|n| observer(n + k))
\t}
}
fun main() {
\tlet root = SignalCell::new(1);
\tlet cold: dyn Source<i32> = Plus { up = root, k = 100 };
\tlet watch = cold.on_change(|n| print(i\"cold saw {n}\"));
\troot.set(5);
\tprint(cold.get());
\twatch.dispose();
}
",
        "cold saw 105\n105\n",
    );
}

/// The brief's pin in its own words — a root, a COLD node and a `.cell()` in
/// one `List<Holder>` of `dyn Source<i32>` — over the S1 probe's real nodes
/// (`std::reactive_pipeline`, reactive-40's). Those nodes are not on this
/// lane's base, so the pin waits for the merge; it was run green over
/// `origin/next`'s two std files (`1 101 10 / 5 105 50`) before it was filed.
#[test]
#[ignore = "A124: needs std::reactive_pipeline (reactive-40, merged to next after this lane's base) - un-ignore at the dyn-40 merge"]
fn a_dyn_source_field_holds_a_root_a_map_node_and_a_cell() {
    assert_compiles_and_runs(
        "import std::reactive::{ Source, SignalCell };
import std::reactive_pipeline::{ Cold };
struct Holder { s: dyn Source<i32> }
fun main() {
\tlet root = SignalCell::new(1);
\tlet cold = root.map_node(|n| n + 100);
\tlet cached = root.map_node(|n| n * 10).cell();
\tlet hs: List<Holder> = [ Holder { s = root }, Holder { s = cold }, Holder { s = cached } ];
\tfor h in hs { print(h.s.get()); }
\troot.set(5);
\tfor h in hs { print(h.s.get()); }
}
",
        "1\n101\n10\n5\n105\n50\n",
    );
}
