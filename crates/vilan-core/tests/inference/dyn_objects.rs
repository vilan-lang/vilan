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
/// root and a derived cell (a pipe sealed with `.memo()`, A142) in one field
/// type, live across a `set`.
#[test]
fn a_dyn_source_field_holds_a_root_and_a_derived_cell() {
    assert_compiles_and_runs(
        "import std::reactive::{ Source, SignalCell };
struct Holder { s: dyn Source<i32> }
fun main() {
\tlet root = SignalCell::new(1);
\tlet mapped = root.derive(|n| n + 10).memo();
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
///
/// B470 (RULED 2026-09-29) narrows it to a resource with a `Drop` inside; a
/// `Drop`-free one is steered to a `[resource] trait` (pinned below).
#[test]
fn a_resource_cannot_be_erased() {
    assert_fails_with(
        "import std::drop::Drop;
trait Shown { fun get(self): i32; }
[resource] struct Handle { v: i32 }
impl Handle with Drop { fun drop(&mut self) {} }
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
/// and an eager derived cell (a `derive` pipe sealed with `.memo()`, A142). The second slot, `on_change`, is exercised
/// through the object as well as the first: the cold node's subscription is
/// taken through its `dyn` and reports the settled value.
#[test]
fn a_dyn_source_field_holds_a_root_a_cold_node_and_a_cell() {
    assert_compiles_and_runs(
        "import std::reactive::{ Source, SignalCell, Subscriber, Subscription };
struct Plus { up: dyn Source<i32>, k: i32 }
impl Plus with Source<i32> {
\tfun get(self): i32 { self.up.get() + self.k }
\tfun on_settle(self, subscriber: Subscriber): Subscription {
\t\tself.up.on_settle(subscriber)
\t}
}
struct Holder { s: dyn Source<i32> }
fun main() {
\tlet root = SignalCell::new(1);
\tlet cold = Plus { up = root, k = 100 };
\tlet mapped = root.derive(|n| n * 10).memo();
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

/// std's own blankets over `S: Source<..>` reach an object: the total join
/// over a cell holding a `dyn Source<i32>` follows the inner source through the
/// table. Since A142 that join is `.switch(|inner| inner)` (`flatten` is the
/// `Option` join only), and the inner object is a `Flow` through the blanket
/// over every `Source`; the result is a pipe, sealed to be read twice.
#[test]
fn a_std_blanket_flattens_through_the_object() {
    assert_compiles_and_runs(
        "import std::reactive::{ Source, SignalCell };
fun main() {
\tlet cell = SignalCell::new(1);
\tlet inner: dyn Source<i32> = cell;
\tlet outer = SignalCell::new(inner);
\tlet flat = outer.switch(|inner| inner).memo();
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
        "import std::reactive::{ Source, SignalCell, Subscriber, Subscription };
struct Plus { up: dyn Source<i32>, k: i32 }
impl Plus with Source<i32> {
\tfun get(self): i32 { self.up.get() + self.k }
\tfun on_settle(self, subscriber: Subscriber): Subscription {
\t\tself.up.on_settle(subscriber)
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

/// The brief's pin in its own words — a root, a derived node and a `.cell()` in
/// one `List<Holder>` of `dyn Source<i32>` — over `std::reactive`'s own nodes
/// (reactive-40's S1 probe until A124 S2b moved them in): the root cell, a
/// derivation, and a `.cell()` materialising a second chain — std's node types
/// behind one field type. Under A142 a bare `derive` is a PIPE with no `get`,
/// so it cannot be a `dyn Source` (the cold `Map` node this pin used to hold is
/// gone); the derived slot holds the pipe sealed read-only with `.memo()`, a
/// `MemoCell` — a different type from the two `SignalCell`s beside it.
#[test]
fn a_dyn_source_field_holds_a_root_a_map_node_and_a_cell() {
    assert_compiles_and_runs(
        "import std::reactive::{ Source, SignalCell };
struct Holder { s: dyn Source<i32> }
fun main() {
\tlet root = SignalCell::new(1);
\tlet cold = root.derive(|n| n + 100).memo();
\tlet cached = root.derive(|n| n * 10).cell();
\tlet hs: List<Holder> = [ Holder { s = root }, Holder { s = cold }, Holder { s = cached } ];
\tfor h in hs { print(h.s.get()); }
\troot.set(5);
\tfor h in hs { print(h.s.get()); }
}
",
        "1\n101\n10\n5\n105\n50\n",
    );
}

/// A slot whose implementation takes a HIDDEN context parameter —
/// `SignalCell::set` reads the ambient turn — is a dispatch site the context
/// pass has to thread, exactly as a bound's dispatch is. The object call went
/// out without the hidden argument and `set` read `undefined[0]`. This is also
/// the one field the estate census found (`vilan-playground`'s `[expose]
/// notes: Signal<List<Note>>`), in the spelling the refusal steers it to.
#[test]
fn a_slot_whose_implementation_reads_a_context_gets_it_threaded() {
    assert_compiles_and_runs(
        "import std::reactive::{ Signal, SignalCell };
struct Notes { notes: dyn Signal<List<i32>> }
fun main() {
\tlet n = Notes { notes = SignalCell::new([1]) };
\tn.notes.set([1, 2]);
\tprint(n.notes.get().len());
}
",
        "2\n",
    );
}

/// dyn-40's ruling, flipped by A124 S2c: `map` was a GENERIC DEFAULT on
/// `Source`, with no table slot, and a call through a `dyn Source` was refused
/// by name. It is a blanket over `S: Source<T>` now, which an object satisfies,
/// so the call reaches through the object and builds a node over it. (A142
/// renamed `map` to `Flow::derive`, reached through the same blanket; the node
/// is a pipe, read once with `.sample()`.)
#[test]
fn a124_map_through_a_dyn_source_is_the_blanket_node() {
    assert_compiles_and_runs(
        "import std::reactive::{ Source, SignalCell };
fun main() {
\tlet cell = SignalCell::new(1);
\tlet object: dyn Source<i32> = cell;
\tlet mapped = object.derive(|n| n + 1);
\tcell.set(4);
\tprint(mapped.sample());
}
",
        "5\n",
    );
}

/// The blanket reaches the object — `proposal/reactive-pipeline.md` §3.4: a
/// node over a `dyn Source<i32>` upstream, read by pull, notified through the
/// object's `on_settle` slot, and materialised by `.cell()`. Under A142 a pipe
/// has one consumer, so each of the three builds its own `derive` over the
/// object (`.cell()`, `.on_change`, `.sample()`).
#[test]
fn a124_the_blanket_node_spelling_reaches_through_a_dyn_source() {
    assert_compiles_and_runs(
        "import std::reactive::{ Source, SignalCell };
fun main() {
\tlet cell = SignalCell::new(1);
\tlet object: dyn Source<i32> = cell;
\tlet cached = object.derive(|n| n + 1).cell();
\tlet watch = object.derive(|n| n + 1).on_change(|n| print(i\"saw {n}\"));
\tcell.set(5);
\tprint(i\"{object.derive(|n| n + 1).sample()} {cached.get()}\");
\twatch.dispose();
}
",
        "saw 6\n6 6\n",
    );
}

// ---------------------------------------------------------------------------
// B398 — a MAPPED-TUPLE position over a `dyn` element coerces its elements
// ---------------------------------------------------------------------------
//
// `(U in T: dyn Source<U>)` type-checked and then threw `s[1].get is not a
// function`: a tuple literal landing at a mapped position typed its elements
// with no expectation, so no element was recorded as erased and the raw
// values reached a comprehension that reads each as a `(value, table)` pair.
// A concrete tuple of `dyn` (`(dyn Source<i32>, dyn Source<str>)`) always
// coerced; the mapped position now directs its elements the same way once its
// source is known.

/// papers-41's probe `a122_09`: a mapped `dyn` parameter read through a
/// comprehension.
#[test]
fn b398_a_mapped_dyn_parameter_coerces_each_element_of_a_tuple_literal() {
    assert_compiles_and_runs(
        concat!(
            "import std::io::print;\n",
            "import std::reactive::{ SignalCell, Source };\n",
            "fun reads<T: (2..)>(sources: (U in T: dyn Source<U>)): T {\n",
            "\t(s in sources => s.get())\n",
            "}\n",
            "fun main() {\n",
            "\tlet a = SignalCell::new(1);\n",
            "\tlet b = SignalCell::new(\"b\");\n",
            "\tlet (x, y) = reads((a, b));\n",
            "\tprint(i\"{x} {y}\");\n",
            "}\n",
        ),
        "1 b\n",
    );
}

/// reactive-41's `combine` repro: the mapped tuple of objects is STORED in a
/// struct field and read later, through a method — three elements of three
/// types, so every position's table is its own.
#[test]
fn b398_a_stored_mapped_dyn_tuple_reads_through_its_tables() {
    assert_compiles_and_runs(
        concat!(
            "import std::io::print;\n",
            "import std::reactive::{ Signal, SignalCell, Source };\n",
            "struct Both<T: (2..)> { sources: (U in T: dyn Source<U>) }\n",
            "impl Both<type T> {\n",
            "\tfun read(self): T { (source in self.sources => source.get()) }\n",
            "}\n",
            "fun make<T: (2..)>(sources: (U in T: dyn Source<U>)): Both<T> { Both<T> { sources } }\n",
            "fun main() {\n",
            "\tlet a = Signal::new(1);\n",
            "\tlet b = Signal::new(\"x\");\n",
            "\tlet c = SignalCell::new(true);\n",
            "\tlet both = make((a, b, c));\n",
            "\ta.set(2);\n",
            "\tlet (n, s, flag) = both.read();\n",
            "\tprint(i\"{n} {s} {flag}\");\n",
            "}\n",
        ),
        "2 x true\n",
    );
}

/// An element that is ALREADY an object is not wrapped a second time; its
/// neighbour, a concrete value, is.
#[test]
fn b398_an_element_already_an_object_is_not_wrapped_again() {
    assert_compiles_and_runs(
        concat!(
            "import std::io::print;\n",
            "import std::reactive::{ SignalCell, Source };\n",
            "fun reads<T: (2..)>(sources: (U in T: dyn Source<U>)): T {\n",
            "\t(s in sources => s.get())\n",
            "}\n",
            "fun main() {\n",
            "\tlet a: dyn Source<i32> = SignalCell::new(7);\n",
            "\tlet b = SignalCell::new(\"q\");\n",
            "\tlet (x, y) = reads((a, b));\n",
            "\tprint(i\"{x} {y}\");\n",
            "}\n",
        ),
        "7 q\n",
    );
}

// ---------------------------------------------------------------------------
// B412 — a generic `S: Trait<X>` VALUE erases to `dyn Trait<X>`
// ---------------------------------------------------------------------------
//
// Inside a generic body, a value typed by the enclosing declaration's own
// parameter could not be erased: `let object: dyn Source<X> = self` in a
// blanket over `S: Source<X>` was "Expected dyn Source<X>, but got S". The
// bound guarantees the impl at every instantiation, so the erasure is
// admitted where the parameter's declared bounds provide the object's trait at
// the object's arguments, and the pair is built per instance.

/// reactive-41's repro, over std's `Source`.
#[test]
fn b412_a_blanket_receiver_erases_to_an_object_of_its_bound() {
    assert_compiles_and_runs(
        concat!(
            "import std::io::print;\n",
            "import std::reactive::{ Signal, SignalCell, Source };\n",
            "impl type S: Source<type X> {\n",
            "\tfun erased(self): dyn Source<X> {\n",
            "\t\tlet object: dyn Source<X> = self;\n",
            "\t\tobject\n",
            "\t}\n",
            "}\n",
            "fun main() {\n",
            "\tlet a = Signal::new(1);\n",
            "\tprint(a.erased().get());\n",
            "}\n",
        ),
        "1\n",
    );
}

/// The three landings — a return, an argument, a list element — at two
/// instantiations, one of them itself an OBJECT (not wrapped a second time).
#[test]
fn b412_a_generic_parameter_erases_at_a_return_an_argument_and_an_element() {
    assert_compiles_and_runs(
        concat!(
            "import std::io::print;\n",
            "trait Src<T> { fun get(self): T; }\n",
            "struct Root { v: i32 }\n",
            "impl Root with Src<i32> { fun get(self): i32 { self.v } }\n",
            "struct Twice { v: i32 }\n",
            "impl Twice with Src<i32> { fun get(self): i32 { self.v * 2 } }\n",
            "fun show(source: dyn Src<i32>): i32 { source.get() }\n",
            "fun erase<S: Src<i32>>(s: S): dyn Src<i32> { s }\n",
            "fun shown<S: Src<i32>>(s: S): i32 { show(s) }\n",
            "fun listed<S: Src<i32>>(s: S): i32 {\n",
            "\tlet all: List<dyn Src<i32>> = [s];\n",
            "\tall[0].get()\n",
            "}\n",
            "fun main() {\n",
            "\tprint(erase(Root { v = 1 }).get());\n",
            "\tprint(shown(Twice { v = 2 }));\n",
            "\tprint(listed(Root { v = 3 }));\n",
            "\tlet object: dyn Src<i32> = Twice { v = 5 };\n",
            "\tprint(erase(object).get());\n",
            "}\n",
        ),
        "1\n4\n3\n10\n",
    );
}

/// With B398: a generic caller hands its own parameters to a mapped `dyn`
/// position — `combine`'s shape in generic code.
#[test]
fn b412_generic_parameters_erase_into_a_mapped_dyn_tuple() {
    assert_compiles_and_runs(
        concat!(
            "import std::io::print;\n",
            "import std::reactive::{ SignalCell, Source };\n",
            "fun reads<T: (2..)>(sources: (U in T: dyn Source<U>)): T {\n",
            "\t(s in sources => s.get())\n",
            "}\n",
            "fun both<A: Source<i32>, B: Source<str>>(a: A, b: B): str {\n",
            "\tlet (x, y) = reads((a, b));\n",
            "\ti\"{x} {y}\"\n",
            "}\n",
            "fun main() {\n",
            "\tprint(both(SignalCell::new(1), SignalCell::new(\"b\")));\n",
            "}\n",
        ),
        "1 b\n",
    );
}

/// A parameter whose bounds do NOT provide the trait — or provide it at other
/// arguments — is still refused.
#[test]
fn b412_a_parameter_not_bounded_by_the_objects_trait_is_still_refused() {
    let prelude = concat!(
        "trait Src<T> { fun get(self): T; }\n",
        "trait Other { fun o(self): i32; }\n",
    );
    assert_fails_with(
        &format!(
            "{prelude}{}",
            "fun erase<S: Other>(s: S): dyn Src<i32> { s }\nfun main() {}\n"
        ),
        "Expected dyn Src<i32>, but got S",
    );
    assert_fails_with(
        &format!(
            "{prelude}{}",
            "fun erase<S: Src<str>>(s: S): dyn Src<i32> { s }\nfun main() {}\n"
        ),
        "Expected dyn Src<i32>, but got S",
    );
}

// ---------------------------------------------------------------------------
// B421 — MISCOMPILE: a `dyn` coercion checked the trait, not its ARGUMENTS
// ---------------------------------------------------------------------------
//
// `show(root.mapped(|n| { n * 2; }))` for `fun show(source: dyn Src<i32>)`
// checked clean and printed `NaN`: the closure's `;` made the node a
// `Mapped<Root, i32, void>` — a `Src<void>` — and the erasure asked only
// whether the value implements `Src` at all. The coercion now asks at the
// object's arguments (conservatively: only a value that positively provides
// the trait at OTHER arguments is refused).

const B421_NODE: &str = concat!(
    "import std::io::print;\n",
    "trait Src<T> { fun get(self): T; }\n",
    "struct Root { value: i32 }\n",
    "impl Root with Src<i32> { fun get(self): i32 { self.value } }\n",
    "struct Mapped<S, T, U> { up: S, transform: |T| U }\n",
    "impl Mapped<type S: Src<type T>, T, type U> with Src<U> {\n",
    "\tfun get(self): U { (self.transform)(self.up.get()) }\n",
    "}\n",
    "impl type S: Src<type T> {\n",
    "\tfun mapped<U>(self, transform: |T| U): Mapped<S, T, U> { Mapped<S, T, U> { up = self, transform } }\n",
    "}\n",
    "fun show(source: dyn Src<i32>) { print(source.get() + 1); }\n",
);

/// reactive-42's repro: refused where it printed `NaN`.
#[test]
fn b421_a_void_closure_node_does_not_erase_to_an_object_of_i32() {
    assert_fails_with(
        &format!(
            "{B421_NODE}{}",
            concat!(
                "fun main() {\n",
                "\tlet root = Root { value = 4 };\n",
                "\tshow(root.mapped(|n| {\n",
                "\t\tn * 2;\n",
                "\t}));\n",
                "}\n",
            )
        ),
        "Expected dyn Src<i32>, but got Mapped<Root, i32, void> instead.",
    );
}

/// The control: the same node with a value-producing body erases and runs.
#[test]
fn b421_a_node_at_the_objects_arguments_still_erases() {
    assert_compiles_and_runs(
        &format!(
            "{B421_NODE}{}",
            concat!(
                "fun main() {\n",
                "\tlet root = Root { value = 4 };\n",
                "\tshow(root.mapped(|n| n * 2));\n",
                "}\n",
            )
        ),
        "9\n",
    );
}

// --- B435: a `dyn` NESTED inside a landing -----------------------------------
//
// The erasure is per VALUE: a literal hands each element its `dyn` position,
// a value already BUILT has concrete elements inside and nothing re-wraps
// them. Such a landing type-checked (the unifier's `Dyn` arms answer "the
// object") and crashed on JS (`x.get is not a function`); natively rustc
// refused it. Refused now with the element-wise steer; a generic position
// bound to an object (`push` on a `List<dyn Src>`) and a generic literal
// under an object annotation erase instead.

const B435_HEAD: &str = concat!(
    "import std::io::print;\n",
    "trait Src { fun get(self): i32; }\n",
    "struct Root { n: i32 }\n",
    "impl Root with Src { fun get(self): i32 { self.n } }\n",
    "struct Boxed<T> { value: T }\n",
    "fun total(objects: List<dyn Src>): i32 {\n",
    "\tmut sum = 0;\n",
    "\tfor object in objects { sum = sum + object.get(); }\n",
    "\tsum\n",
    "}\n",
    "fun read(object: Option<dyn Src>): i32 {\n",
    "\tmatch object { Some(let found) => found.get(), None => 0 }\n",
    "}\n",
    "fun run(make: || dyn Src): i32 { make().get() }\n",
    "fun roots(): List<Root> { [Root { n = 10 }] }\n",
);

const B435_REFUSAL: &str = "does not become a";

#[test]
fn b435_a_built_list_binding_at_a_list_of_objects_is_refused() {
    assert_fails_once_with(
        &format!(
            "{B435_HEAD}{}",
            concat!(
                "fun main() {\n",
                "\tlet built: List<Root> = [Root { n = 1 }];\n",
                "\tprint(total(built));\n",
                "}\n",
            )
        ),
        "a `List<Root>` does not become a `List<dyn Src>` as a whole",
    );
}

#[test]
fn b435_a_call_result_at_a_list_of_objects_is_refused_with_the_map_steer() {
    assert_fails_once_with(
        &format!(
            "{B435_HEAD}{}",
            "fun main() {\n\tlet objects: List<dyn Src> = roots();\n\tprint(total(objects));\n}\n"
        ),
        "`let objects: List<dyn Src> = value.map(|element| element);`",
    );
}

#[test]
fn b435_a_built_option_at_an_option_of_an_object_is_refused() {
    assert_fails_once_with(
        &format!(
            "{B435_HEAD}{}",
            concat!(
                "fun main() {\n",
                "\tlet maybe: Option<Root> = Some(Root { n = 3 });\n",
                "\tprint(read(maybe));\n",
                "}\n",
            )
        ),
        "a `Option<Root>` does not become a `Option<dyn Src>` as a whole",
    );
}

#[test]
fn b435_a_closure_binding_at_an_object_returning_closure_is_refused() {
    assert_fails_once_with(
        &format!(
            "{B435_HEAD}{}",
            "fun main() {\n\tlet make = || Root { n = 7 };\n\tprint(run(make));\n}\n"
        ),
        "wrap it in a closure literal",
    );
}

#[test]
fn b435_a_built_generic_struct_at_an_object_argument_is_refused() {
    assert_fails_once_with(
        &format!(
            "{B435_HEAD}{}",
            concat!(
                "fun main() {\n",
                "\tlet built: Boxed<Root> = Boxed { value = Root { n = 1 } };\n",
                "\tlet erased: Boxed<dyn Src> = built;\n",
                "\tprint(erased.value.get());\n",
                "}\n",
            )
        ),
        "a `Boxed<Root>` does not become a `Boxed<dyn Src>` as a whole",
    );
}

#[test]
fn b435_a_built_tuple_is_refused_at_a_tuple_of_objects_inside_a_list() {
    // A tuple nested one level further down than B430's element-wise door.
    assert_fails_with(
        &format!(
            "{B435_HEAD}{}",
            concat!(
                "fun pairs(items: List<(dyn Src, i32)>): i32 { items.len().as_i32() }\n",
                "fun main() {\n",
                "\tlet built: List<(Root, i32)> = [(Root { n = 1 }, 2)];\n",
                "\tprint(pairs(built));\n",
                "}\n",
            )
        ),
        B435_REFUSAL,
    );
}

#[test]
fn b435_a_list_of_objects_at_a_list_of_the_concrete_type_is_refused() {
    // The narrowing, one level down: an object does not narrow back.
    assert_fails_once_with(
        &format!(
            "{B435_HEAD}{}",
            concat!(
                "fun sum(items: List<Root>): i32 { mut s = 0; for item in items { s = s + item.n; } s }\n",
                "fun main() {\n",
                "\tlet objects: List<dyn Src> = [Root { n = 1 }];\n",
                "\tprint(sum(objects));\n",
                "}\n",
            )
        ),
        "Expected List<Root>, but got List<dyn Src> instead: an object does not narrow back",
    );
}

#[test]
fn b435_the_literal_and_element_wise_spellings_erase_and_run() {
    assert_compiles_and_runs(
        &format!(
            "{B435_HEAD}{}",
            concat!(
                "fun main() {\n",
                "\tlet built: List<Root> = [Root { n = 1 }, Root { n = 2 }];\n",
                "\tlet objects: List<dyn Src> = built.map(|element| element);\n",
                "\tprint(total(objects));\n",
                "\tlet maybe: Option<Root> = Some(Root { n = 3 });\n",
                "\tlet object: Option<dyn Src> = maybe.map(|element| element);\n",
                "\tprint(read(object));\n",
                "\tlet root = Root { n = 4 };\n",
                "\tprint(read(Some(root)));\n",
                "\tprint(read(Some(Root { n = 5 })));\n",
                "\tlet make = || Root { n = 6 };\n",
                "\tprint(run(|| make()));\n",
                "\tprint(total([Root { n = 7 }, root]));\n",
                "}\n",
            )
        ),
        "3\n3\n4\n5\n6\n11\n",
    );
}

#[test]
fn b435_a_push_into_a_list_of_objects_erases_the_value() {
    // `List<T>::push(value: T)`: `T` is bound to the object at this call, so
    // the value's position IS a `dyn` one.
    assert_compiles_and_runs(
        &format!(
            "{B435_HEAD}{}",
            concat!(
                "struct Bag { items: List<dyn Src> }\n",
                "fun main() {\n",
                "\tmut pushed: List<dyn Src> = [];\n",
                "\tpushed.push(Root { n = 1 });\n",
                "\tlet root = Root { n = 2 };\n",
                "\tpushed.push(root);\n",
                "\tprint(total(pushed));\n",
                "\tmut bag = Bag { items = [] };\n",
                "\tbag.items.push(Root { n = 4 });\n",
                "\tprint(bag.items[0].get());\n",
                "\tmut slots: List<dyn Src> = [Root { n = 0 }];\n",
                "\tslots[0] = Root { n = 5 };\n",
                "\tprint(slots[0].get());\n",
                "}\n",
            )
        ),
        "3\n4\n5\n",
    );
}

#[test]
fn b435_a_generic_struct_literal_under_an_object_annotation_erases_its_field() {
    assert_compiles_and_runs(
        &format!(
            "{B435_HEAD}{}",
            concat!(
                "fun main() {\n",
                "\tlet boxed: Boxed<dyn Src> = Boxed { value = Root { n = 8 } };\n",
                "\tprint(boxed.value.get());\n",
                "\tlet root = Root { n = 9 };\n",
                "\tlet held: Boxed<dyn Src> = Boxed { value = root };\n",
                "\tprint(held.value.get());\n",
                "\tlet plain: Boxed<i32> = Boxed { value = 3 };\n",
                "\tprint(plain.value);\n",
                "}\n",
            )
        ),
        "8\n9\n3\n",
    );
}

// --- B430: a built tuple at a tuple-of-objects position ----------------------
//
// B398 erased the elements of a tuple LITERAL at such a position; a tuple
// VALUE (a binding, a call result) escaped, and JS read `p[0][1].get` off a
// bare struct. It is re-built by projection now, each object element paired
// with its table — at a written `(dyn A, dyn B)` and at B398's mapped
// `(U in T: dyn Source<U>)` alike.

#[test]
fn b430_a_tuple_value_erases_elementwise_at_a_tuple_of_objects() {
    assert_compiles_and_runs(
        &format!(
            "{B435_HEAD}{}",
            concat!(
                "trait Named { fun name(self): str; }\n",
                "impl Root with Named { fun name(self): str { \"root\" } }\n",
                "fun pair(p: (dyn Src, dyn Src)): i32 { p.0.get() + p.1.get() }\n",
                "fun mixed(p: (dyn Named, i32)): str { i\"{p.0.name()} {p.1}\" }\n",
                "fun make(): (Root, Root) { (Root { n = 5 }, Root { n = 6 }) }\n",
                "fun main() {\n",
                "\tlet t = (Root { n = 1 }, Root { n = 2 });\n",
                "\tprint(pair(t));\n",
                "\tprint(pair(make()));\n",
                "\tlet m = (Root { n = 0 }, 7);\n",
                "\tprint(mixed(m));\n",
                "\tlet held: (dyn Src, dyn Src) = t;\n",
                "\tprint(held.0.get() + held.1.get());\n",
                "}\n",
            )
        ),
        "3\n11\nroot 7\n3\n",
    );
}

#[test]
fn b430_a_tuple_value_erases_elementwise_at_a_mapped_position() {
    assert_compiles_and_runs(
        concat!(
            "import std::io::print;\n",
            "import std::reactive::{ SignalCell, Source };\n",
            "fun pair(p: (dyn Source<i32>, dyn Source<str>)): str {\n",
            "\ti\"{p.0.get()} {p.1.get()}\"\n",
            "}\n",
            "fun reads<T: (2..)>(sources: (U in T: dyn Source<U>)): T {\n",
            "\t(s in sources => s.get())\n",
            "}\n",
            "fun main() {\n",
            "\tlet a = SignalCell::new(1);\n",
            "\tlet b = SignalCell::new(\"b\");\n",
            "\tlet t = (a, b);\n",
            "\tprint(pair(t));\n",
            "\tlet (x, y) = reads(t);\n",
            "\tprint(i\"{x} {y}\");\n",
            "}\n",
        ),
        "1 b\n1 b\n",
    );
}

// --- B431: Q5 at every instantiation of an erased parameter ------------------
//
// B412 erases the enclosing declaration's own parameter `S` into a `dyn` when
// its bounds provide the trait; the coercion's resource check could only ask
// about `S`, so a `[resource]` instantiation compiled and the object dropped
// the resource's teardown. Refused at the call that binds it, directly or
// through a caller forwarding its own parameter.

// B470: `Handle` carries a `Drop`, so its erasure keeps Q5's refusal; the
// `Drop`-free twins are pinned below.
const B431_HEAD: &str = concat!(
    "import std::io::print;\n",
    "import std::drop::Drop;\n",
    "trait Src { fun get(self): i32; }\n",
    "[resource] struct Handle { n: i32 }\n",
    "impl Handle with Drop { fun drop(&mut self) {} }\n",
    "impl Handle with Src { fun get(self): i32 { self.n } }\n",
    "struct Plain { n: i32 }\n",
    "impl Plain with Src { fun get(self): i32 { self.n } }\n",
    "fun erase<S: Src>(own source: S): dyn Src { source }\n",
);

#[test]
fn b431_an_erased_parameter_instantiated_at_a_resource_is_refused() {
    assert_fails_once_with(
        &format!(
            "{B431_HEAD}{}",
            "fun main() {\n\tprint(erase(Plain { n = 1 }).get());\n\tprint(erase(Handle { n = 2 }).get());\n}\n"
        ),
        "`Handle` is a resource, so it cannot become a `dyn Src`",
    );
}

#[test]
fn b431_a_forwarded_parameter_reaching_a_resource_is_refused() {
    assert_fails_once_with(
        &format!(
            "{B431_HEAD}{}",
            concat!(
                "fun outer<T: Src>(own value: T): dyn Src { erase(value) }\n",
                "fun main() {\n",
                "\tprint(outer(Handle { n = 2 }).get());\n",
                "}\n",
            )
        ),
        "`Handle` is a resource, so it cannot become a `dyn Src`",
    );
}

#[test]
fn b431_an_erased_parameter_at_plain_values_still_runs() {
    assert_compiles_and_runs(
        &format!(
            "{B431_HEAD}{}",
            concat!(
                "fun outer<T: Src>(own value: T): dyn Src { erase(value) }\n",
                "fun main() {\n",
                "\tprint(erase(Plain { n = 1 }).get());\n",
                "\tprint(outer(Plain { n = 2 }).get());\n",
                "}\n",
            )
        ),
        "1\n2\n",
    );
}

/// B435's family: a WRITTEN type argument on a method call fixes the method's
/// own generic before its closure is typed — `rs.map<dyn Src>(|element|
/// element)` erases each element where it lands. The method path took the
/// argument only when it wired the call, after the closure had bound `U` to
/// `Root` from its body, and JS read `.get` off a bare struct.
#[test]
fn b435_a_written_type_argument_on_a_method_erases_the_closures_result() {
    assert_compiles_and_runs(
        &format!(
            "{B435_HEAD}{}",
            concat!(
                "fun main() {\n",
                "\tlet built: List<Root> = [Root { n = 1 }, Root { n = 2 }];\n",
                "\tlet objects = built.map<dyn Src>(|element| element);\n",
                "\tprint(total(objects));\n",
                "\tlet wide = built.map<i53>(|element| element.n.as_i53());\n",
                "\tprint(wide[1]);\n",
                "}\n",
            )
        ),
        "3\n2\n",
    );
}

// --- B470: a `Drop`-free resource into the object of a `[resource] trait` ----
//
// RULED 2026-09-29: move-only-ness of a trait object is DECLARED. `dyn T` is
// a resource exactly when `T` (or a trait it extends) is `[resource]`; a
// resource with no `Drop` anywhere inside may be erased only into such a
// trait's object (else the steer to declare it); a resource with a `Drop`
// inside stays refused with the message it always had.

const B470_HEAD: &str = r#"
import std::drop::Drop;

[resource]
trait Run {
    fun run(own self): i32;
}

[resource]
struct Node { v: i32 }

impl Node with Run {
    fun run(own self): i32 { self.v + 1 }
}

struct Plain { v: i32 }

impl Plain with Run {
    fun run(own self): i32 { self.v * 10 }
}
"#;

fn b470_program(rest: &str) -> String {
    format!("{B470_HEAD}\n{rest}")
}

#[test]
fn b470_a_drop_free_resource_becomes_a_resource_traits_object_and_moves() {
    assert_compiles_and_runs(
        &b470_program(
            r#"
            fun pick(on: bool): dyn Run {
                if on {
                    Node { v = 1 }
                } else {
                    Plain { v = 2 }
                }
            }
            fun main() {
                print(pick(true).run());
                print(pick(false).run());
            }
            "#,
        ),
        "2\n20\n",
    );
}

#[test]
fn b470_a_resource_traits_object_is_move_only() {
    // Whatever landed in it — here a DATA value — the object is moved.
    assert_fails_with(
        &b470_program(
            r#"
            fun main() {
                let object: dyn Run = Plain { v = 2 };
                let first = object;
                let second = object;
            }
            "#,
        ),
        "use of `object` after it was moved: a resource has a single owner",
    );
}

#[test]
fn b470_a_resource_with_a_drop_inside_is_still_refused() {
    assert_fails_with(
        &b470_program(
            r#"
            [resource]
            struct Held { n: i32 }
            impl Held with Drop {
                fun drop(&mut self) {}
            }
            impl Held with Run {
                fun run(own self): i32 { 0 }
            }
            fun main() {
                let object: dyn Run = Held { n = 1 };
            }
            "#,
        ),
        "`Held` is a resource, so it cannot become a `dyn Run`: a trait object's teardown would \
         have to be dispatched through its table",
    );
}

#[test]
fn b470_a_drop_free_resource_into_an_undeclared_traits_object_is_steered() {
    assert_fails_with(
        r#"
        trait Plainly {
            fun value(self): i32;
        }
        [resource]
        struct Node { v: i32 }
        impl Node with Plainly {
            fun value(self): i32 { self.v }
        }
        fun main() {
            let object: dyn Plainly = Node { v = 1 };
        }
        "#,
        "`Node` is a resource, so it can become a `dyn Plainly` only when `Plainly` is declared \
         `[resource]`: mark the trait `[resource]`",
    );
}

#[test]
fn b470_the_attribute_is_the_declaring_traits_alone() {
    // Not inherited (the integrator's correction, 2026-09-29): a subtrait of
    // a `[resource]` trait has data objects unless it declares the attribute
    // itself — its `dyn` copies freely, and cannot receive a resource.
    let subtrait = r#"
        trait Labelled with Run {
            fun label(self): str;
        }
        impl Plain with Labelled {
            fun label(self): str { "plain" }
        }
    "#;
    assert_compiles_and_runs(
        &b470_program(&format!(
            "{subtrait}\nfun main() {{\n    let object: dyn Labelled = Plain {{ v = 2 }};\n    \
             let first = object;\n    let second = object;\n    print(first.label() + second.label());\n}}\n"
        )),
        "plainplain\n",
    );
    assert_fails_with(
        &b470_program(&format!(
            "{subtrait}\nimpl Node with Labelled {{\n    fun label(self): str {{ \"node\" }}\n}}\n\
             fun main() {{\n    let object: dyn Labelled = Node {{ v = 1 }};\n}}\n"
        )),
        "`Node` is a resource, so it can become a `dyn Labelled` only when `Labelled` is declared \
         `[resource]`",
    );
    // The declared supertrait's object stays move-only.
    assert_fails_with(
        &b470_program(
            "fun main() {\n    let object: dyn Run = Plain { v = 2 };\n    let first = object;\n    \
             let second = object;\n}\n",
        ),
        "use of `object` after it was moved",
    );
}

#[test]
fn b470_an_undeclared_traits_object_stays_data() {
    // The control: `dyn` of a trait nobody declared `[resource]` copies freely.
    assert_compiles_and_runs(
        r#"
        trait Show {
            fun show(self): str;
        }
        struct Plain { v: i32 }
        impl Plain with Show {
            fun show(self): str { i"{self.v}" }
        }
        fun main() {
            let object: dyn Show = Plain { v = 2 };
            let first = object;
            let second = object;
            print(first.show() + second.show());
        }
        "#,
        "22\n",
    );
}

#[test]
fn b470_a_pipe_selector_with_arms_of_two_types_returns_the_object() {
    // A142's mixed-arm selector, cut down: a `[resource]` node and a data root
    // behind one `[resource] trait` object, started once each.
    assert_compiles_and_runs(
        r#"
        [resource]
        trait Flow {
            fun start(own self, react: |i32| void);
        }
        struct Root { v: i32 }
        impl Root with Flow {
            fun start(own self, react: |i32| void) { react(self.v); }
        }
        [resource]
        struct Doubled { up: Root }
        impl Doubled with Flow {
            fun start(own self, react: |i32| void) {
                self.up.start(|v| react(v * 2));
            }
        }
        fun select(doubled: bool, root: Root): dyn Flow {
            if doubled {
                Doubled { up = root }
            } else {
                root
            }
        }
        fun main() {
            let root = Root { v = 3 };
            select(true, root).start(|v| print(i"piped {v}"));
            select(false, root).start(|v| print(i"root {v}"));
        }
        "#,
        "piped 6\nroot 3\n",
    );
}

#[test]
fn b470_an_erased_parameter_at_a_drop_free_resource_is_steered_or_allowed() {
    // B431's per-instantiation path under B470: a `Drop`-free resource bound
    // to an erased parameter is steered to `[resource]` when the trait is not
    // declared one, and allowed (and moved) when it is.
    let head = |declared: &str| {
        format!(
            "{declared}trait Src {{ fun get(self): i32; }}\n\
             [resource] struct Node {{ n: i32 }}\n\
             impl Node with Src {{ fun get(self): i32 {{ self.n }} }}\n\
             fun erase<S: Src>(own source: S): dyn Src {{ source }}\n\
             fun main() {{\n\tprint(erase(Node {{ n = 2 }}).get());\n}}\n"
        )
    };
    assert_fails_once_with(
        &head(""),
        "`Node` is a resource, so it can become a `dyn Src` only when `Src` is declared \
         `[resource]`",
    );
    assert_compiles_and_runs(&head("[resource]\n"), "2\n");
}

#[test]
fn a142_a_mixed_arm_selector_erases_a_pipe_and_a_root_into_one_flow_object() {
    // B470's shape for A142 (R39): `Flow` is declared `[resource]`, so a
    // `switch` selector whose arms are a PIPE (a `Derive`, a `[resource]`
    // stage) and a ROOT (a `SignalCell`) meets them in one `dyn Flow<i32>`,
    // the arms erasing where they land: `100` then `3`, on both backends.
    // Written WITHOUT `switch`'s type arguments (B480): `U` binds from the
    // object's trait argument, `I: Flow<U>` at `I = dyn Flow<i32>`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Flow, MemoCell, Signal, SignalCell, Source };

        fun arm(on: bool, count: SignalCell<i32>): dyn Flow<i32> {
            if on {
                count.derive(|value| value * 100)
            } else {
                count
            }
        }

        fun main() {
            let flag = Signal::new(true);
            let count = Signal::new(1);
            let picked: MemoCell<i32> = flag.switch(|on: bool| arm(on, count)).memo();
            print(i"{picked.get()}");
            flag.set(false);
            count.set(3);
            print(i"{picked.get()}");
        }

        main();
        "#,
        "100\n3\n",
    );
}

/// B480: the same selector with NOTHING written — no `switch` type arguments,
/// an unannotated closure parameter and an unannotated binding — and the user
/// shape beneath it: a generic `<U, I: Fl<U>>` binding `U` from a `dyn Fl<i32>`
/// argument's own trait argument, beside a concrete one.
#[test]
fn b480_a_bound_through_a_trait_argument_binds_from_an_object() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Flow, Signal, SignalCell, Source };

        fun arm(on: bool, count: SignalCell<i32>): dyn Flow<i32> {
            if on {
                count.derive(|value| value * 100)
            } else {
                count
            }
        }

        trait Fl<T> { fun go(self): T; }
        struct Cell<T> { v: T }
        impl Cell<type T> with Fl<T> { fun go(self): T { self.v } }
        fun pick<U, I: Fl<U>>(i: I): U { i.go() }

        fun main() {
            let flag = Signal::new(true);
            let count = Signal::new(1);
            let picked = flag.switch(|on| arm(on, count)).memo();
            print(i"{picked.get()}");
            flag.set(false);
            count.set(3);
            print(i"{picked.get() + 1}");
            let object: dyn Fl<i32> = Cell { v = 5 };
            print(pick(object) + pick(Cell { v = 6 }));
        }

        main();
        "#,
        "100\n4\n11\n",
    );
}

/// B480's shape as kolt's `Channel::find` writes it (re-checked by reactive-46,
/// Order 46): `switch` with NO type arguments over a selector whose body is a
/// `match` on an `Option` with two arms of different stage types — a
/// `Source::constant` and a `derive` over another pipe — erased by an annotated
/// `dyn Flow<TransientState<..>>` binding, then sealed. kolt carried
/// `switch<TransientState<Channel, RpcError>>` for it; it infers.
#[test]
fn b480_a_selector_matching_two_erased_arms_needs_no_switch_type_arguments() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Flow, Pipe, Signal, SignalCell, Source };
        import std::reactive::transient::TransientState;

        fun shown(state: TransientState<str, str>): str {
            match state {
                TransientState::Ready(let name) => name,
                TransientState::Absent => "absent",
                _ => "pending",
            }
        }

        fun main() {
            let client: SignalCell<Option<i32>> = Signal::new(None);
            let names: SignalCell<Option<str>> = Signal::new(Some("general"));
            let found = client
                .switch(|connected| {
                    let state: dyn Flow<TransientState<str, str>> = match connected {
                        None => Source::constant(TransientState::Pending),
                        Some(let _id) => names.derive(|name| match name {
                            Some(let present) => TransientState::Ready(present),
                            None => TransientState::Absent,
                        }),
                    };
                    state
                })
                .memo();
            print(shown(found.get()));
            client.set(Some(1));
            print(shown(found.get()));
            names.set(None);
            print(shown(found.get()));
        }

        main();
        "#,
        "pending\ngeneral\nabsent\n",
    );
}

/// E261: the same selector WITHOUT the annotation — two stages of different
/// types meet only as one erased `dyn Flow<T>`, and the mismatch, which names
/// two half-inferred stage types, now names the annotation too. For a `match`
/// and an `if` alike (one rule, `unify_arm_bodies`), at the offending leg.
#[test]
fn e261_two_pipe_stages_in_a_match_steer_to_the_dyn_flow_annotation() {
    let header = concat!(
        "import std::reactive::{ Flow, Pipe, Signal, SignalCell, Source };\n",
        "import std::reactive::transient::TransientState;\n",
    );
    let steer = "Both are pipe stages, and two stages of different types meet only as one \
         erased flow: annotate where the value lands, `let state: dyn Flow<T> = ..`";
    assert_fails_spanning(
        &format!(
            "{header}fun main() {{\n\tlet client: SignalCell<Option<i32>> = Signal::new(None);\n\tlet names: SignalCell<Option<str>> = Signal::new(Some(\"general\"));\n\tlet found = client.switch(|connected| {{\n\t\tlet state = match connected {{\n\t\t\tNone => Source::constant(TransientState::Pending),\n\t\t\tSome(let _id) => names.derive(|name| match name {{\n\t\t\t\tSome(let present) => TransientState::Ready(present),\n\t\t\t\tNone => TransientState::Absent,\n\t\t\t}}),\n\t\t}};\n\t\tstate\n\t}}).memo();\n}}\n"
        ),
        "names.derive(|name| match name {\n\t\t\t\tSome(let present) => TransientState::Ready(present),\n\t\t\t\tNone => TransientState::Absent,\n\t\t\t})",
        steer,
    );
    assert_fails_with(
        &format!(
            "{header}fun main() {{\n\tlet n: SignalCell<i32> = Signal::new(1);\n\tlet on = true;\n\tlet either = if on {{ n.derive(|v| v + 1) }} else {{ Source::constant(3) }};\n\tlet _ = either.memo();\n}}\n"
        ),
        &format!(
            "`if` arms have mismatched types: expected Derive<SignalCell<i32>, i32, i32>, but got Constant<i32> instead. {steer}"
        ),
    );
}

/// E261's boundary: arms that are not both flows keep the plain mismatch, and
/// a function's own trait named `Flow` is not std's pipe layer.
#[test]
fn e261_arms_that_are_not_both_flows_get_no_flow_steer() {
    for source in [
        "import std::reactive::{ Signal, SignalCell, Pipe };\nfun main() {\n\tlet n: SignalCell<i32> = Signal::new(1);\n\tlet on = true;\n\tlet either = if on { n.derive(|v| v + 1) } else { 3 };\n}\n",
        "fun main() {\n\tlet on = true;\n\tlet either = if on { 1 } else { \"two\" };\n}\n",
        "trait Flow {\n\tfun go(self): i32;\n}\nstruct A {}\nstruct B {}\nimpl A with Flow {\n\tfun go(self): i32 { 1 }\n}\nimpl B with Flow {\n\tfun go(self): i32 { 2 }\n}\nfun main() {\n\tlet on = true;\n\tlet either = if on { A {} } else { B {} };\n}\n",
    ] {
        assert_fails_with(source, "have mismatched types");
        assert_fails_without(source, "pipe stages");
    }
}

#[test]
fn a142_a_source_object_stays_copyable_though_flow_is_a_resource_trait() {
    // R39 with B470's attribute on the DECLARING trait alone: `Source<T> with
    // Flow<T>` inherits `Flow`'s members, not its `[resource]`, so one
    // `dyn Source<i32>` is bound twice and read through both.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source };

        fun read_twice(source: dyn Source<i32>): i32 {
            let first: dyn Source<i32> = source;
            let second: dyn Source<i32> = source;
            first.get() + second.get()
        }

        fun main() {
            let count: SignalCell<i32> = Signal::new(2);
            print(i"{read_twice(count)}");
        }

        main();
        "#,
        "4
",
    );
}

/// B436: `print` of a trait OBJECT prints the value it erased — `dyn
/// Shape(Square { side = 5 })` through the printer since debugging.md S3 —
/// not the backend's `[ value, table ]` pair (`[ [ 5 ], {} ]`), which is how
/// the JS backend carries an object (`b503_printing_a_list_of_trait_objects_
/// prints_their_values` holds a list of them).
#[test]
fn b436_printing_a_trait_object_prints_its_value() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Shape {
            fun area(self): i32;
        }

        struct Square {
            side: i32,
        }

        impl Square with Shape {
            fun area(self): i32 { self.side * self.side }
        }

        fun main() {
            let shape: dyn Shape = Square { side = 5 };
            print(shape);
            print(shape.area());
        }

        main();
        "#,
        "dyn Shape(Square { side = 5 })\n25\n",
    );
}

/// B503: a LIST of trait objects handed to `print` prints each object's value
/// — through the printer since debugging.md S3, `dyn Shape(Square { side =
/// 2 })` — and so does a list of such lists, and an object inside an `Option`
/// (which crossed the host boundary as its stored pair before S3). The
/// program's own list is untouched (its objects still dispatch afterwards).
#[test]
fn b503_printing_a_list_of_trait_objects_prints_their_values() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, None, Some };

        trait Shape {
            fun area(self): i32;
        }

        struct Square {
            side: i32,
        }

        impl Square with Shape {
            fun area(self): i32 { self.side * self.side }
        }

        fun main() {
            let shapes: List<dyn Shape> = [Square { side = 2 }, Square { side = 3 }];
            print(shapes);
            let rows: List<List<dyn Shape>> = [shapes, [Square { side = 4 }]];
            print(rows);
            print(shapes[1].area());
            let maybe: Option<dyn Shape> = Some(Square { side = 5 });
            print(maybe);
        }

        main();
        "#,
        concat!(
            "[dyn Shape(Square { side = 2 }), dyn Shape(Square { side = 3 })]\n",
            "[[dyn Shape(Square { side = 2 }), dyn Shape(Square { side = 3 })], [dyn Shape(Square { side = 4 })]]\n",
            "9\n",
            "Some(dyn Shape(Square { side = 5 }))\n",
        ),
    );
}

/// B437: an object's TABLE is keyed by the trait APPLICATION, not the trait
/// alone. `impl Square with Shape<i32>` and `with Shape<str>` are two tables
/// over one type; keyed by the trait, `dyn Shape<str>` was handed the `i32`
/// table built first and its `area` answered `16` (native answered `big`).
#[test]
fn b437_two_applications_of_one_trait_over_one_type_take_their_own_tables() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Shape<T> {
            fun area(self): T;
        }

        struct Square {
            side: i32,
        }

        impl Square with Shape<i32> {
            fun area(self): i32 { self.side * self.side }
        }

        impl Square with Shape<str> {
            fun area(self): str { "big" }
        }

        fun main() {
            let counted: dyn Shape<i32> = Square { side = 2 };
            let named: dyn Shape<str> = Square { side = 4 };
            print(counted.area());
            print(named.area());
        }

        main();
        "#,
        "4\nbig\n",
    );
}

/// B437's other half, the control: two positions of ONE application — two
/// annotations, two type ids, a compound argument — share one table and one
/// instance of a generic over the object, because both keys spell the type
/// structurally (`type_key`'s `Dyn` arm, beside the table key's arguments).
#[test]
fn b437_two_positions_of_one_application_share_one_table_and_one_instance() {
    let source = r#"
        import std::io::print;

        trait Shape<T> {
            fun area(self): T;
        }

        struct Square {
            side: i32,
        }

        impl Square with Shape<List<i32>> {
            fun area(self): List<i32> { [self.side * self.side] }
        }

        fun measure<S: Shape<List<i32>>>(shape: S): List<i32> {
            shape.area()
        }

        fun main() {
            let first: dyn Shape<List<i32>> = Square { side = 2 };
            let second: dyn Shape<List<i32>> = Square { side = 3 };
            print(measure(first));
            print(measure(second));
        }

        main();
        "#;
    let js = compile(source).expect("compiles");
    assert_eq!(js.matches("Object.create(").count(), 1, "one table:\n{js}");
    assert_eq!(js.matches("(shape) {").count(), 1, "one instance:\n{js}");
    assert_compiles_and_runs(source, "[4]\n[9]\n");
}

// --- B502: a trait object binds a bound's arguments from what it CARRIES ------

/// B502: `measure<T, S: Shape<T>>(shape: S)` called with a `dyn Shape<str>`
/// whose type behind it implements `Shape` twice bound `T` from the first impl
/// (`i32`) and refused: "'dyn Shape<str>' does not implement trait
/// 'Shape<i32>'". The object's own arguments answer — both objects of one type,
/// in either call order, and a list of objects. (A `dyn Named<str>` over the
/// supertrait `Shape<str>` binds `T` too; its table's answer is B532's pin,
/// below.)
#[test]
fn b502_a_trait_object_binds_a_bounds_arguments_from_its_own() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Shape<T> {
            fun area(self): T;
        }

        struct Square {
            side: i32,
        }

        impl Square with Shape<i32> {
            fun area(self): i32 {
                self.side * self.side
            }
        }

        impl Square with Shape<str> {
            fun area(self): str {
                "big"
            }
        }

        fun measure<T, S: Shape<T>>(shape: S): T {
            shape.area()
        }

        fun main() {
            let named: dyn Shape<str> = Square { side = 4 };
            let counted: dyn Shape<i32> = Square { side = 5 };
            print(measure(named));
            print(measure(counted));
            let again: dyn Shape<str> = Square { side = 1 };
            print(measure(again));
            let shapes: List<dyn Shape<i32>> = [Square { side = 3 }];
            for shape in shapes {
                print(measure(shape));
            }
        }
        "#,
        "big\n25\nbig\n9\n",
    );
}

/// The program both B532 pins run: one type implementing `Shape` at two
/// instantiations, reached through objects of SUB-traits — one level up
/// (`Named<T> with Shape<T>`), two levels up (`Titled<T> with Named<T>`), a
/// clause that writes a concrete argument (`Big with Shape<str>`), and the
/// same sub-trait at the other instantiation — plus a call through a bound.
const B532_PROGRAM: &str = r#"
import std::io::print;

trait Shape<T> {
    fun area(self): T;
}

trait Named<T> with Shape<T> {}

trait Titled<T> with Named<T> {}

trait Big with Shape<str> {}

struct Square {
    side: i32,
}

impl Square with Shape<i32> {
    fun area(self): i32 {
        self.side * self.side
    }
}

impl Square with Shape<str> {
    fun area(self): str {
        "big"
    }
}

impl Square with Named<str> {}

impl Square with Named<i32> {}

impl Square with Titled<str> {}

impl Square with Big {}

fun through_bound<S: Named<str>>(shape: S): str {
    shape.area()
}

fun main() {
    let titled: dyn Named<str> = Square { side = 2 };
    print(titled.area());
    let counted: dyn Named<i32> = Square { side = 3 };
    print(counted.area());
    let deeper: dyn Titled<str> = Square { side = 4 };
    print(deeper.area());
    let big: dyn Big = Square { side = 5 };
    print(big.area());
    print(through_bound(Square { side = 6 }));
}
"#;

/// B532: an object over a SUB-trait answers a supertrait's member from the
/// supertrait's instantiation the clause chain passes — `dyn Named<str>` over
/// `Named<T> with Shape<T>` answers `area` from `Shape<str>` (JS answered from
/// `Shape<i32>`, printing `4` for `big`; natively rustc refused the emitted
/// table, E0308). `native_differential` runs the same program on both backends.
#[test]
fn b532_an_object_over_a_subtrait_answers_from_the_supertraits_instantiation() {
    assert_compiles_and_runs(B532_PROGRAM, "big\n9\nbig\nbig\nbig\n");
}

/// M89: a trait object's table is a function of its SLOT SET, so two
/// `(type, trait)` pairs whose every member is the same emitted function share
/// one table object. A generic struct whose impl does not depend on its
/// parameter emits one body for every instantiation, and coercing `Holder<i32>`
/// and `Holder<str>` built two identical tables (kolt's client: 14 tables, 10
/// distinct). Both objects still dispatch.
#[test]
fn m89_two_pairs_with_one_slot_set_share_one_table() {
    let source = r#"
        import std::io::print;

        trait Weight { fun weight(self): i32; }
        struct Holder<T> { value: T, weight: i32 }
        impl Holder<type T> with Weight {
            fun weight(self): i32 { self.weight }
        }

        fun main() {
            let a: dyn Weight = Holder { value = 1, weight = 2 };
            let b: dyn Weight = Holder { value = "x", weight = 3 };
            print(a.weight() + b.weight());
        }
    "#;
    let js = compile(source).unwrap_or_else(|errors| panic!("{errors:#?}"));
    assert_eq!(
        js.matches("Object.create(").count(),
        1,
        "two pairs with one slot set built two tables (M89):\n{js}"
    );
    assert_compiles_and_runs(source, "5\n");
}
