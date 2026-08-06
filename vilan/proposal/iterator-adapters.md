# Iterator adapters and the pipeline ergonomics (I3)

> Status: RATIFIED 2026-08-04 (owner review) — all recommendations stand,
> with one owner note on §collect: an explicit `to_list()`/`to_set()`
> family is the PRIMARY termination API and must always exist — the
> owner has been burned by Rust's collect-only ambiguities. `collect`
> is acceptable as a future ADDITION, never as the only spelling.
> Prerequisites all cleared: P2/P3/P4 fixed (B55/B56/B58), B57 ratified.
> I5's protocol repair ships with this arc.
>
> ARC SHIPPED 2026-08-06 (v0.30.0); implementation notes in §11.
> Owner ruling 2026-08-06: §4 option (ii) is REFUSED — the async-closure
> blocker and the measured ~5.5x on a List source (§11) are
> disqualifying; the shipped state stands (one meaning per name, lazy
> spellings via `.iter()` only). Remaining open here: S6/Iterable
> under B4.
>
> Origin: the "whatever happened to iterators" audit (2026-08-03), filed as
> backlog I3. Proposal-first per the house rules; nothing here is
> implemented. Every claim below about what the compiler does today was
> checked against source **or run through the repo compiler** as a probe —
> the probes are called out inline, because four of them found defects that
> change the design. §10 is the open-questions set; everything before it is
> a recommendation, not a ratification.

## 0. The problem and the thesis

`std::iterator` shipped in the first std commit and never grew a surface.
It is 31 lines: `Iterator<T>` with the lone `next`, `IteratorFromFn`,
`Iterable<T>`, and a blanket `Iterable`-for-`Iterator` impl. No `map`,
`filter`, `take`, `zip`, `enumerate`, `rev`, or `collect` exists on it
anywhere. Meanwhile the *language* half is done and good — `for x in`
drives a protocol loop, `Range` iterates lazily, `for e in &mut c` lends.

**Thesis: the adapter layer needs no new type-system machinery.** Trait
defaults, adapter structs holding an upstream plus a closure, and bounded
generics all exist, and all monomorphize to direct calls. A working
two-stage pipeline — `c.taken(3).to_list()` over a user iterator, with the
terminal as a trait default — compiles and runs today, in a probe, with
the emitted JS containing no dispatch table and no vtable (§8).

What actually blocks the feature is four concrete defects on its critical
path (§2), three of which produce **silently wrong code today**. That is
the real content of this paper: the design is cheap, the prerequisites are
not, and shipping adapters without fixing them would ship a pipeline that
compiles clean and computes nothing.

## 1. Ground truth

Verified in source; the compiler is not doing what the docs say.

**The `for` protocol is duck-typed on the method *name*, not on the
trait.** `finalize_build` resolves each loop by looking for a member
literally called `next` (or `next_mut`) on the iterable's concrete type
(`analyzer.rs:21139-21158`, `for_each_next_method` at
`analyzer.rs:15080-15085`); the guard is
`matches!(iterable_type, Type::Struct(_, _) | Type::Enum(_, _))`, and
`method_member_in_impls` scans inherent impls too. **No `Iterator` impl is
required.** `docs/std/collections.md:149` ("Anything implementing
`Iterator`/`Iterable` works in a `for` loop") describes an intent, not the
implementation.

**The `Iterator` and `Iterable` traits are effectively dead.**
`IteratorFromFn` is the only type in std implementing `Iterator`, and it
has zero uses outside its own definition. `Iterable` has no implementor
but the blanket impl, and **no `.iter()` call exists anywhere** in
`std/src`, `test`, `examples`, or `crates/vilan-core/tests`. The one
corpus test that writes `impl Naturals with Iterator<i32>`
(`test/iterator-protocol.vl:12`) would behave identically without the
`with` clause — `Range` proves it.

**`Iterator::next` is declared by value and is therefore unimplementable
by any stateful iterator.** `iterator.vl:4` declares `fun next(self):
Option<T>`. `Range` — the one real lazy iterator in std — uses a bare
inherent impl with `fun next(&mut self)` and deliberately does *not*
implement the trait (`range.vl:11-25`). It cannot: B29's conformance check
compares the receiver convention. Probe, verbatim:

```
Error: `ListIterator`'s `next` receives `&mut self`, but `Iterator`
declares `self`; match the receiver convention
```

`test/iterator-protocol.vl` only satisfies the by-value signature because
its `Naturals::next` mutates a *module-level* `mut produced`, not its own
state. Every adapter this paper proposes is stateful.

**A bare-trait-typed value is a clean error (B4), and that makes
`Iterable` as declared unusable.** `Iterable::iter(self): Iterator<T>`
returns a bare trait type; calling anything on the result is
`analyzer.rs:18393`, "a trait is not a value type (vilan has no trait
objects)". It typechecks at all only because `reconcile_type` grew a
`(Trait, Trait)` arm for the identity impl (`analyzer.rs:16795-16801`,
recorded in `ret-checking.md:62-65`). **Consequence: every adapter must
return a *named concrete struct*, and the chain's type must stay concrete
end to end.** This is exactly what `list.vl:97-100` means by "needs a
concrete `List` iterator (returning `ListIterator<T>`, not the abstract
`Iterator<T>`)". There are no associated types (`parse_trait_body_clean`
admits only functions, `parsing.rs:2727`), so "the iterator type of
`Self`" has no spelling — §3 carries it as a second trait parameter
instead.

**Default trait members exist and are load-bearing.** Signature-or-body is
in the parser (`parsing.rs:2885`), inheritance is "Gap E" in the analyzer
(`method_member_in_inherited_defaults`, `analyzer.rs:8053`), and the spec
states it (`docs/spec/types.md:77`). std relies on it: all nine operator
traits (`operators.vl:32-86`), six `Compare` defaults (`compare.vl:22-58`),
and — the closest precedent to an adapter — `reactive.vl:358`, a default
on a *generic* trait taking a *closure*. `Self` in a trait body denotes
the implementing type (`docs/spec/types.md:80`).

**Bounded generics fully monomorphize.** The H8/element-syntax S1 gate
probe recorded it (`element-syntax.md:215`): "bounded-generic methods are
**fully monomorphized** — one JS function per instantiation, trait calls
emitted as **direct calls** to the concrete impl function, no dispatch
tables, no adapter arguments."

**Return-type-driven generic inference exists.** Expectation-directed
checking is normative (`docs/spec/types.md:89-108` §5.6); the solve site is
`analyzer.rs:16068-16101` ("A generic parameter fixed only by the return
type … is inferred by unifying the return type against the call's expected
type"). A full `collect<C: FromIterator<T>>(self): C` compiles and runs
today under a `let` annotation — see §5, where it is nonetheless not the
recommendation.

**`List` reaches none of this.** `List` implements neither trait; `iter` is
deferred (`list.vl:97-100`). Probe: with `impl Iterable<type T> { fun
count_them(self): i32 { 0 } }` in scope, `xs.count_them()` on a `List` is
`Error: List<i32> has no method 'count_them'`. **The entry's headline
chain, `xs.filter(f).take(3).to_list()`, is blocked before any adapter is
written.**

## 2. Prerequisites — four defects on the critical path

Each is independently reproducible with the repo compiler (2026-08-03,
`target/debug/vilan`). P2–P4 are the reason this proposal leads with
prerequisites rather than with the adapter set.

### P1 — `Iterator::next` must be redeclared `&mut self`

Mechanical, and the enabling change for everything else. `iterator.vl:4`
becomes `fun next(&mut self): Option<T>;`. Fallout is small because the
trait has one implementor: `IteratorFromFn::next` follows, and `Range`
gains the `with Iterator<i32>` clause it has always deserved. `for` keeps
working either way (it never consulted the trait). This also makes the
docs' "`Range` is one such type" (`docs/std/collections.md:150`) true for
the first time.

### P2 — a bounded-generic call through a re-dispatched callee emits an empty body

**Silent wrong code, and it hits every adapter.** An adapter pulls from its
upstream through a bounded generic (`self.upstream.next()` where
`upstream: U, U: Iter<T>`). That call monomorphizes to a direct call, but
the callee is emitted with an **empty body** when the enclosing method is
entered through a re-dispatch path. Minimal repro — compiles clean, exit 0,
no diagnostic:

```vilan
trait Iter<T> { fun next(&mut self): Option<T>; }

struct Counting { at: i32, limit: i32 }
impl Counting with Iter<i32> {
    fun next(&mut self): Option<i32> {
        if self.at < self.limit { self.at = self.at + 1; Some(self.at) } else { None }
    }
}

struct Passthrough<U, T> { upstream: U }
impl Passthrough<type U: Iter<T>, type T> with Iter<T> {
    fun next(&mut self): Option<T> { self.upstream.next() }
}

fun main() {
    mut p = Passthrough { upstream = Counting { at = 0, limit = 3 } };
    for v in p { print(v); }        // TypeError at runtime
}
```

Emitted: `function $a(self) {\n\n}` — `Counting::next`, empty. Two entry
paths were verified to trigger it, with a common symptom:

- **the `for`-loop protocol edge.** `transformer.rs:2712-2717` emits the
  callee by bare id (`ensure_function_emitted(next_id)` /
  `name_for(next_id)`) with no instantiation context, and the call-graph
  edge is a bare `CallTarget::Function(next_id)`
  (`call_graph.rs:625-634`) — no type arguments. Replacing the loop with a
  direct `p.next()` call makes the same program correct.
- **an adapter constructed inside a trait default.** `c.taken(3).to_list()`
  fails identically when `taken` is a default on the trait; constructing
  the same `Taken` from a free generic function makes it correct.

Direct concrete calls are unaffected at any nesting depth (a two-hop
`Outer<Inner<Dog>>` chain runs correctly). The two triggers are plausibly
one root cause — instantiations are not seeded for callees reached by
re-dispatch — but that diagnosis is work, not a finding, and belongs to
the slice. **This is the single largest item in the arc.**

> **Correction, 2026-08-04 (B55 slice).** Fixed; the guess above is wrong on
> the count. The two triggers are **two** root causes, not one, sharing a
> symptom:
>
> 1. **the `for`-loop edge** — the loop emitted its `next` callee by bare id
>    (`transformer.rs`, the `Expr::ForEach` protocol arm), which is the
>    *concrete*-function path. A generic callee walked with no substitution
>    left its own `U` unbound. The loop now takes the same dispatch
>    precedence as any call site (`for_each_next_dispatch`), and the analyzer
>    records the loop's impl bindings alongside `for_each_next`.
> 2. **the trait-default constructor** — nothing to do with instantiation
>    seeding. `Self` in a member's *return type* was specialized only when
>    the return type EQUALLED the `self` parameter's type, so a `Self`
>    nested in a type argument (`Taken<Self, T>`) stayed the abstract trait
>    type: `Counting{}.taken(3)` typed as `Taken<Iter, i32>`, binding
>    `Taken`'s `U` to a bare trait. Substitution is now structural
>    (`analyzer.rs`, `infer_type_inner`'s `Expr::Call` arm, via
>    `substitute_member_type`), and reaches a generic receiver as well as a
>    concrete one.
>
> The shared symptom — a call resolving to the trait's signature-only member
> and emitting `function f(self) {\n}` — is now a hard compile error in its
> own right (`transformer.rs`, `function_with_name` + assembly), so this
> CLASS cannot recur silently whatever causes it.

### P3 — `for v in self` over a generic silently becomes a native `for...of`

The natural way to write a terminal is `for v in self`. Inside
`impl Iter<type T>` or a trait default, `self`'s type is not
`Type::Struct`/`Type::Enum`, so the guard at `analyzer.rs:21145` misses and
the loop falls through to the native `for...of` arm — iterating the
struct's *flat field array*. Probe: a `to_list` written that way over
`Counting { at = 0, limit = 3 }` returns a 2-element list (the two
fields), prints `2`, and never calls `next`. Compiles clean. The fix is
either to extend the protocol resolution to generics bounded by an
iterator trait, or to reject the construct — either way it must not
silently produce a field walk.

> **Correction, 2026-08-04 (B56 slice).** Fixed, and the section's account of
> WHICH subjects miss needs one amendment: `self` inside `impl Iter<type T>`
> *is* a `Type::Struct` and always reached the protocol guard — what it hit
> instead was P2's bare-id emission. The subjects that genuinely fell through
> to the native `for...of` are the other two: `self` inside a **trait
> default** (`Type::Trait`) and a **trait-bounded generic** (`Type::Generic`,
> e.g. `it: I` where `I: Iter<T>` — that one summed the receiver's two fields
> rather than its three elements). Both are now resolved, and BOTH resolutions
> were taken: the protocol is extended to those subjects (re-dispatched per
> monomorphization through `generic_dispatch`, the same channel a method call
> on them uses), and a generic whose bounds provide no `next` is now rejected
> — `cannot iterate 'I': no bound on it provides 'next(&mut self): Option<T>'`
> — where it previously emitted a native loop that threw at runtime.

### P4 — a bound on a trait's own generic parameter does not reach its default bodies

This one gates the entry's stated direction (blanket reachability, §3).
Carrying the iterator type as a second trait parameter is the
associated-type-free spelling:

```vilan
trait Iterable<T, I: Iterator<T>> {
    fun iter(self): I;
    fun taken(self, count: i32): Taken<I, T> {
        Taken { upstream = self.iter(), remaining = count }   // rejected
    }
}
```

```
Error: generic parameter 'U' is missing the bound ': Iterator2<T>'
required by this call
  … the bound is declared here  →  trait Iterable2<T, I: Iterator2<T>>
```

The diagnostic *points at the bound it is failing to honor*. The identical
construction with the bound on a free function's own generics compiles and
runs correctly. So this is a bound-propagation gap in trait default bodies,
not a design limit — and it is the difference between "adapters on
`Iterable` work in v1" and "blocked on associated types".

> **Correction, 2026-08-04 (B58 slice).** Fixed, and **§3's unproven piece
> is now proven**: the `Iterable<T, I: Iter<T>>` program above — the
> constructor default (`taken`) *and* terminals written over `self.iter()`
> — compiles and runs, with the adapter driven by a `for` loop afterwards.
> The section's diagnosis ("a bound-propagation gap") named the right area
> but the wrong mechanism, and the backlog entry's probe steer
> (`satisfies_trait_bound` / `generic_bounds` registration) was wrong on
> both counts. Neither is at fault: the bound IS registered for a trait's
> own parameters, the analyzer always resolved the member through it, and
> `satisfies_trait_bound`'s bound-to-bound arm always accepted it. TWO root
> causes, in the two halves this proposal's other prerequisites already
> taught us to check separately:
>
> 1. **Codegen could not GROUND the dispatch.** `emit_default_instance`
>    (`transformer.rs`) specialized every trait default under an EMPTY
>    substitution, so the `GenericDispatch::OnConstraint(T, ..)` the
>    analyzer had recorded found no binding for `T` and fell through to the
>    trait's abstract member. It now runs under the trait's own parameters
>    bound to each impl's `with`-clause arguments, plus the impl's binders
>    from the concrete receiver — so a trait argument in the impl's own
>    terms (`impl Bag<type E: Bound> with Holder<E>`) grounds in two hops.
>    This is P2's world, and P2's own repair is what surfaced it: pre-B55
>    this emitted `function $a(self) {\n}` and threw at runtime; the
>    never-silent guard made it a hard compile error, which is how the
>    symptom presents on v0.25.0 for the *direct* shape (a bound member
>    called on a `T` value, which this section never showed).
> 2. **The quoted repro above failed in the ANALYZER, before any of that.**
>    The return-type-only inference re-bound a call's declared return
>    generic to the call's expected type. Its guard against re-binding
>    CALLER generics filters by the declared type — which cannot help when
>    callee and caller are members of the SAME trait and literally share the
>    parameter id. Inside `Iterable<T, I: Iter<T>>`'s default,
>    `self.iter()`'s declared return `I` *is* the enclosing `I`, so it bound
>    to `Taken`'s `U`, lost its bound, and produced the diagnostic verbatim
>    — the compiler reporting the *expectation* as the parameter missing the
>    bound, which is why the message names a `U` the source never mentions.
>    A binder owned by an enclosing declaration is now excluded: it is fixed
>    by the enclosing instantiation, not free for a call site to infer.
>
> Nine pins in `inference.rs`, seven proven red first (six on cause 1, one
> on cause 2). Unchanged and pinned as such: an unbounded trait parameter
> still refuses member access (`cannot call method 'label' on T`), and an
> impl overriding a bound-using default still wins. The bound-list
> break-on-first-hit scan B57 flags is untouched — the multi-bound pin
> (`T: A + B`, both members reached) passes on today's scan.

### P0 — `List` has no iterator

Independent of the above and needed for the headline chain: a concrete
`ListIterator<T>` (index cursor over the array) plus `List::iter(self):
ListIterator<T>`. `list.vl:97-100` also names a `get` intrinsic, but
indexing already exists in the language (`test/for-mut-container.vl:15`
writes `&mut self.items[index]`), so the remaining work is the struct and
the cursor.

## 3. The adapter set and its home

**Home: trait defaults on the repaired `Iterator<T>`, plus a two-parameter
`Iterable<T, I: Iterator<T>>` for blanket reachability.** Adapter structs
live in `std::iterator` beside the trait.

The alternative home — a trait-subject inherent impl, `impl Iterable<type
T> { … }`, following `iterator.vl:17`'s `from_fn` precedent — was probed
and is **half-usable**. Adapter *constructors* work there: `fun
taken(self, n: i32): Taken<Self, T> { Taken { upstream = self, … } }`
compiles, because it only *stores* `self`. *Terminals* do not: `self` is
bare-trait-typed in such a body, so `self.next()` is the B4 error
verbatim. Trait defaults are the only position where a call on `self` is
legal (`is_in_trait_default`, `analyzer.rs:1942`), and codegen re-dispatches
to the concrete specialization. One home for both halves beats two, so:
defaults.

The shape, all of which ran except where P4 is noted:

```vilan
trait Iterator<T> {
    fun next(&mut self): Option<T>;

    fun map<U>(self, fn: |T| U): Mapped<Self, T, U>        { Mapped { upstream = self, fn } }
    fun filter(self, predicate: |T| bool): Filtered<Self, T> { … }
    fun take(self, count: i32): Taken<Self, T>             { … }
    fun skip(self, count: i32): Skipped<Self, T>           { … }
    fun enumerate(self): Enumerated<Self, T>               { … }
    fun zip<U, J: Iterator<U>>(self, other: J): Zipped<Self, J, T, U> { … }
    fun chain<J: Iterator<T>>(self, other: J): Chained<Self, J, T>    { … }

    fun to_list(mut self): List<T> { … }                   // §5
    fun fold<B>(mut self, init: B, fn: |B, T| B): B { … }
    fun for_each(mut self, fn: |T| void): void { … }
    fun count(mut self): i32 { … }
    fun any(mut self, p: |T| bool): bool { … }
    fun all(mut self, p: |T| bool): bool { … }
}
```

The struct names are past-participle (`Mapped`, `Taken`, `Filtered`) rather
than the Rust-style `Map`/`Take`/`Filter` for a concrete reason: `Map` would
collide with `std::map::Map` in any program importing both, and vilan's
method resolution does not diagnose collisions (§4). The *method* names stay
`map`/`take`/`filter`.

Each adapter is a struct storing its upstream by value plus its closure —
`IteratorFromFn` is the precedent that a closure field monomorphizes to
nothing (`struct IteratorFromFn<T> { fn: || Option<T> }`, lowering to
`function $a(fn) { return [fn]; }`). One syntax constraint to design
around: a closure *field* is called parenthesized, `(self.fn)(value)`, not
`self.fn(value)` — method lookup does not fall back to fields, and there is
a steer for it (`analyzer.rs:7757`).

`Iterable<T, I: Iterator<T>>` carries the same adapter names as defaults
that begin `self.iter()`, which is what makes `xs.take(3)` reachable on a
`List` without an explicit `.iter()`. Its cost, recorded: the iterator type
is exposed in the trait's parameter list, so the impl reads `impl List<type
T> with Iterable<T, ListIterator<T>>`. That is the price of having no
associated types, and it is paid once per container, by std. It is also
**the one piece of this section that is not proven** — it needs P4.

> **Correction, 2026-08-04 (B58 slice).** Now proven. With P4 fixed, an
> `Iterable<T, I: Iter<T>>` carrying both a constructor default (`taken`)
> and terminals over `self.iter()` compiles and runs, and the adapter it
> returns drives a `for` loop. The unproven-feasibility caveat on S6 (§9)
> and open question (b)'s "blocked on a bound-propagation gap whose fix is
> unscoped" are both discharged; the *ergonomic* call in (b) — whether
> blanket reachability gates v1 — stands on its own merits, unblocked.

**Not in v1**, recorded: `flat_map`/`flatten` (nested adapter types with no
associated-type relief), `peekable` (needs a one-slot buffer and a
`peek(&mut self): Option<&T>` returning a view — the aggregate-storage ban,
§7), `windows`/`chunks`, `step_by`, `scan`, `cycle`, `sum`/`product` on the
trait (they live on `List` under `T: Add + Default` today and the bound
interaction wants its own look), and anything async.

## 4. The name policy — the hard call

`List` has inherent **eager** `map`/`filter`/`fold`/`for_each`
(`list.vl:26-58`), each allocating a new `List`. The adapters want the same
three names, lazily. The entry states the constraint as "inherent wins".

**That rule is half-real, and the safe half is not the half we need.**
`method_member_impl_subject` (`analyzer.rs:7663-7692`) is a plain
`.find_map` over a flat `Vec<Implementation>` that **never reads
`trait_ids`**: first matching impl in *registration order* wins.
Registration order is walk order (`analyzer.rs:14035`). The spec sentence —
"fields first, then methods of inherent impls, then trait members visible
via the type's impls" (`docs/spec/names.md:114-116`) — holds only for the
*inherited-default* case, which reaches a second stage
(`method_member_in_inherited_defaults`, `analyzer.rs:18093`) that runs only
when stage 1 missed. When a trait impl **declares** the name, both are in
stage 1 and source order decides. Probed, in one file:

```vilan
impl Bag { fun pick(self): str { "INHERENT (eager)" } }
impl Iter<type T> { fun pick(self): str { "TRAIT-INHERENT (lazy)" } }
// → "INHERENT (eager)"; swap the two blocks → "TRAIT-INHERENT (lazy)"
```

No diagnostic either way. There is **no ambiguity detection anywhere** in
the method path: two traits providing one name resolve to whichever impl
registered first, and a `T: A + B` bound `break`s on the first hit
(`analyzer.rs:18212`). Grepping `errors.md` for `ambiguous` returns
nothing. For `List` specifically the two impls live in different modules
(`list` is force-loaded as a core primitive, `analyzer.rs:24896-24903`;
`iterator` is not), so the winner would be decided by **module load
order** — a global, invisible, refactor-sensitive property.

**Option (i) — share the names, lazy reachable only via `.iter()`.** The
premise is that `xs.filter(f)` keeps meaning the eager `List` method and
`xs.iter().filter(f)` means the lazy one. Consequences: it depends on a
precedence rule the compiler does not implement; a reader cannot tell the
two apart at the call site, since both spell `filter` and only the
receiver's provenance distinguishes them; and the failure mode is
surprise-laziness — a pipeline whose closure side effects never run, or run
in a different order, because a `.iter()` was added or removed three lines
up. Diagnostics are the worst part: nothing is wrong, so nothing is
reported.

**Option (ii) — one name each, eager forms re-expressed over the adapters
and fused.** `List::map`/`filter` keep their eager signatures and bodies
become `self.iter().map(fn).to_list()`. One meaning per name, and the
allocation behavior of existing code is unchanged (still exactly one
result `List`; the adapter chain in between allocates nothing that
survives — §8). Cost: `List`'s eager methods acquire a dependency on the
whole adapter stack, so P0–P4 must all be solid before the rewrite lands,
and the corpus goldens for every `List::map` call site change (a symbol
rename, per the H8 precedent, but a real diff to verify).

**Recommendation: (ii), with the lazy names kept distinct until it lands.**
Option (i) asks the language for a guarantee it does not make, and the
first bug it produces will be unreportable. (ii) is the "fix root causes"
direction — it removes the duplication instead of arbitrating it — and it
degrades gracefully: until the rewrite, lazy `map`/`filter` are reachable
only through `.iter()` on a `List`, which is *the same surface* option (i)
promises, minus the collision.

**Marked as an open call** (§10a). It is the one decision in this paper
that changes what existing code means.

## 5. Termination surface

A `FromIterator`-style `collect` is **expressible today** — this was the
surprise of the investigation. Return-type-driven inference is normative
(`docs/spec/types.md:89-108`) and implemented
(`analyzer.rs:16068-16101`), and a probe of the full shape ran correctly:

```vilan
trait FromIterator<T> { fun from_iter(items: List<T>): Self; }
fun collect<C: FromIterator<T>>(self): C { … }

let xs:  List<i32> = it.collect();   // dispatches to List's impl
let bag: Bag<i32>  = it.collect();   // dispatches to Bag's impl
```

Multiple impls disambiguate purely by the expectation, and a wrong
annotation is caught (`'List<str>' does not implement trait
'FromIterator<i32>', required by a generic bound of this call`). It does
**not** lean on B4: `C` is a generic parameter, never a bare-trait value.

It is nonetheless not the v1 recommendation, for reasons of ergonomics and
of what the surrounding machinery does when the annotation is absent:

- The expectation must come from an annotation, a declared return type, a
  parameter type, or a field type. **`it.collect().len()` does not
  resolve** — expectations do not flow out of a method chain — which is
  precisely the shape a pipeline invites.
- Explicit type arguments exist (`f<T>(args)`, `parsing.rs:1958-1979`) on
  free functions and statics, but are **silently ignored on instance
  methods** (probed: `h.echo<str>(5)` prints `5`). So `collect<List<i32>>()`
  is not an escape hatch; the annotation is the only lever.
- The ungrounded-generic guard only covers a call's *own* bounded generics
  (`analyzer.rs:15205-15221`), and unbounded return-only generics are never
  grounded and never reported — a divergence from spec §5.6 point 4.

**Recommendation: the explicit family — `to_list()`, `to_set()`,
`to_map()` — as trait defaults, and no `collect` in v1.** They need no
annotation, they read at the call site, and they compose with chains. Their
bounds are real and already have a home: `to_set` needs `T: Hashable`
(`set.vl:9`) and `to_map` needs the element to be a `(K, V)` tuple with
`K: Hashable` (`map.vl:11`) — tuples are structural and exist
(`docs/spec/types.md:14`). `FromIterator` + `collect` is recorded as the
extension once instance-method type arguments work, at which point it is
additive and costs nothing to have waited for.

## 6. `rev`

Two shapes, both real:

- **A double-ended protocol** — a `DoubleEnded<T>` supertrait adding
  `next_back(&mut self): Option<T>`, with `rev` returning a `Reversed<I, T>`
  that swaps the two. Genuinely lazy and O(1) in space. Cost: every adapter
  must decide whether it forwards `next_back` (`map` and `filter` can;
  `take`/`skip` need a length to be reversible at all; `zip`/`chain` need
  both sides double-ended), so the trait count roughly doubles and each
  adapter grows a second conformance. `Range` and `ListIterator` would be
  the only two sources that could implement it in v1.
- **A `List`-materializing fallback** — `rev` drains the upstream into a
  `List`, reverses it, and hands back a `ListIterator`. O(n) space, and it
  makes an infinite iterator hang rather than error, but it is ~15 lines
  and works over every adapter with no per-adapter conformance.

**Recommendation: the materializing fallback in v1, with `rev` documented
as a barrier** ("consumes the upstream eagerly; not for unbounded
sources"). The double-ended protocol is the better answer and should be
designed when a consumer needs a lazy reverse over a chain — designing it
now doubles the surface of a layer that has not yet had its first user, and
it is additive later: `rev`'s signature does not change, only its body and
the bound. Note the ordering dependency — `to_list`'s eager `List::reverse`
(I4) is what the fallback calls, so I4's basics land first either way.

## 7. The `&mut` lending form

`for e in &mut c` drives `next_mut(&mut self): Option<&mut T>`. This is a
**convention, not a trait** — the entire mechanism is two lines
(`analyzer.rs:15082`), no std type implements it, and its one corpus test
defines `next_mut` as a bare inherent method
(`test/for-mut-container.vl:11`).

`memory-management-rev-1.md:595-612` records why the lending form is
tractable at all: because the return type is plainly `Option<&mut T>` with
no lifetime parameter — the borrow lives in an inferred `borrows self`
origin summary — "the lending-iterator problem that forces Rust into GATs
simply does not arise". That claim is about the **signature of one lending
step**, and it is paired with a restriction that is directly load-bearing
here: `:588-593` keeps the ban on storing a view in a struct field or
collection — "Return: yes. Storage in aggregates: still no."

**Recommendation: explicitly out of scope for v1.** Two reasons, recorded:

1. **Rule 4 has no origin for an interposed adapter.** The origin seed for
   a lending binding keys off `Expr::Reference` wrapping the iterable
   *directly*, and flattens to the container root in one hop
   (`analyzer.rs:9626-9641`). `c.iter_mut().filter(f)` puts a struct
   between the binding and `place_root(operand)`, so the composed form has
   **no origin at all** — the protection that makes the lending form safe
   silently does not apply. `rule4-completion.md:119-121` already carries
   the two-hop chain as unfinished S4 work.
2. **An adapter that must hold a view hits the aggregate-storage ban.** A
   `MapMut { inner: … }` holding a lent view is exactly what
   `memory-management-rev-1.md:588-593` forbids. Adapters that hold the
   *iterator* by value are fine; the ones that would make the lending form
   useful are not.

v1 therefore lends only at the loop, as today. The by-value adapter chain
and the lending loop are separate surfaces, and saying so plainly in the
docs is cheaper than a half-composed one.

## 8. Fusion and performance

**Today**, `xs.map(f).filter(g)` on a `List` allocates two intermediate
lists and walks twice; each eager method is a `for...of` building a fresh
array (`list.vl:26-43`).

**With adapters**, a chain allocates one small struct per stage at
construction — each holding its upstream plus a closure — and then pulls
one element at a time. The emitted code is direct calls: the probed
two-stage pipeline `c.taken(3).to_list()` lowered to

```js
function $a(self, count) { return [ self, count ]; }   // take, the constructor
function $c(self) { … $e(self[0]) … }                  // Taken::next → upstream
function $b(self) { … while (going) { … $c(self) … } } // to_list, specialized
const got = $b($a(c, 3));
```

— one JS function per instantiation, no dispatch table, no vtable, no
adapter argument, matching the H8 S1 gate probe's finding for bounded
generics (`element-syntax.md:215`). (`$e` is P2's empty body; the *shape*
is right, the callee is not emitted.)

Two honest caveats against overselling this. First, there is no fusion
*pass* — the win is structural (one traversal, no intermediate arrays),
not an optimizer, and each stage still costs a real JS call per element,
where the eager form's inner loop is inlined into one function. For short
lists the eager form may well be faster; the win is on long chains and on
short-circuiting terminals (`take`, `any`, `all`), where laziness changes
the asymptotics rather than the constant. Second, option (ii) of §4 makes
`List::map` pay the adapter path for *every* existing call site, so the
name-policy decision is also a performance decision and wants a measurement
before the rewrite lands — the `--print-chunks` precedent from A16 S1
(measure first, then decide) applies.

## 9. Slices

Each is independently shippable and suite-gated; docs in the same commit
per the house rules.

- **S1 — the protocol repair.** P1 (`next` takes `&mut self`),
  `Range` gains `with Iterator<i32>`, `IteratorFromFn` follows,
  `docs/std/collections.md` corrected (the "Anything implementing
  `Iterator`/`Iterable` works in a `for` loop" and "`Range` is one such
  type" sentences are both currently false). No adapters. Standalone
  payoff: the trait becomes implementable, and the docs stop lying. Pins:
  a stateful iterator conforming to the trait; `Range` in a `for`.
- **S2 — the codegen prerequisites.** P2 and P3, each with its own pinned
  regression *proven red first* (the `Passthrough` repro for P2 runs and
  prints 1..3; the `for v in self` repro returns 3 elements, not 2). This
  is the diagnosis-heavy slice and the arc's long pole. Nothing user-facing
  ships; it is the gate for everything after.
- **S3 — `ListIterator` + `List::iter`.** P0. Standalone payoff:
  `for x in xs.iter()` and a real seam for `Map`/`Set` later. Pins: cursor
  exhaustion, empty list, rule-4 interaction with a mutation mid-iteration.
- **S4 — the adapters.** `map`/`filter`/`take`/`skip`/`enumerate` as trait
  defaults plus their structs; `zip`/`chain` if the two-upstream form
  probes clean. Pins per adapter, plus a chain of three, plus a
  short-circuit test proving `take(3)` over an infinite source terminates.
- **S5 — the terminals.** `to_list`/`to_set`/`to_map`/`fold`/`for_each`/
  `count`/`any`/`all`, and `rev` (§6). Pins including the `Hashable` and
  tuple bounds.
- **S6 — blanket reachability.** P4, then `Iterable<T, I: Iterator<T>>`
  with the adapter names as forwarding defaults, and `List`'s impl. This is
  what removes the `.iter()` tax. Deliberately last: it is the only slice
  whose feasibility is unproven.
- **S7 — the name policy**, per §4's call. If (ii): `List`'s eager
  `map`/`filter`/`fold`/`for_each` re-expressed over the adapters, with the
  corpus diff and a benchmark as the gates.

Take-up order S1 → S2 → S3 → S4 → S5 → S6 → S7. S2 is the measure-first
gate for the arc: if P2's diagnosis turns out to be structural rather than
a missed seeding, the whole design should be re-examined before S4.

### Relationship to I4

**I4's eager basics do not wait for any of this**, per its own constraint.
`reverse`, `sort`, `join`, `contains`, `index_of`, `first`/`last`,
`insert`/`remove` are eager `List` methods with no adapter content, and
three of them are load-bearing *for* this paper: `rev`'s fallback (§6)
calls `List::reverse`, and `to_list`'s tests want `contains`/`first`. The
dependency runs I4 → I3, never the reverse.

Where the two meet is the name policy, and only for the three names I4
does not touch: I4 adds names `List` does not have, while §4 arbitrates the
three it already has. The one coordination point is that I4 should **not**
add eager `take`/`skip`/`enumerate`/`zip` to `List` — those names belong to
the lazy layer, and adding eager twins would recreate exactly the collision
§4 exists to remove. Nothing in I4's filed gap list proposes them; this
note exists so it stays that way.

## 10. Open questions

**(a) The name policy** (§4) — share `map`/`filter`/`fold` between eager
`List` methods and lazy adapters, or re-express the eager forms over the
adapters? *Recommendation: re-express (option ii).* Sharing depends on a
precedence the compiler does not implement — probed: swapping two impl
blocks flips which method a call resolves to, silently — so the surprise-
laziness bug it invites would also be undiagnosable; re-expressing costs a
corpus diff and a benchmark, and leaves one meaning per name.

**(b) Blanket reachability, and whether it gates v1** (§3, P4) — is
`xs.take(3)` without `.iter()` a requirement, or is `xs.iter().take(3)`
acceptable for v1? *Recommendation: acceptable; ship S1–S5 first and make
S6 a follow-on.* The blanket form needs a two-parameter `Iterable<T, I:
Iterator<T>>` that exposes the iterator type at every impl site, and it is
blocked on a bound-propagation gap (P4) whose fix is unscoped; the explicit
`.iter()` is one call and honest about where laziness begins.

**(c) `collect` vs the explicit family** (§5) — ship
`to_list`/`to_set`/`to_map`, or the `FromIterator` + `collect` pair, which
probes as working today? *Recommendation: the explicit family.* `collect`
resolves only from an annotation and not from a chained call
(`it.collect().len()` fails), and instance-method type arguments are
silently ignored, so the escape hatch users would reach for does not exist
yet — but the design is sound and additive whenever that gap closes.

**(d) `rev`'s shape** (§6) — a `DoubleEnded` supertrait or a
`List`-materializing fallback? *Recommendation: the fallback, documented as
a barrier.* Double-ended is the better answer but roughly doubles the
adapter surface before this layer has had its first user, and it is purely
additive later since `rev`'s signature does not change.

**(e) The `&mut` lending form** (§7) — do adapters compose over
`next_mut` in v1? *Recommendation: explicitly out of scope, with the reason
recorded.* Rule 4's origin seed keys off a reference wrapping the iterable
directly and flattens in one hop, so an interposed adapter has no origin
and loses the protection that makes lending safe; and a view-holding
adapter would violate the still-standing aggregate-storage ban.

**(f) Whether P2's prerequisites belong to this arc at all** (§2, S2) —
P2 and P3 are compiler defects that produce silently wrong code for
programs *nobody has written yet*, but also for any user who writes a
generic wrapper over an iterator today. *Recommendation: file them as their
own B-section backlog items, fixed inside this arc as S2.* They are not
iterator features and would be bugs with or without adapters; filing them
separately keeps the record honest about what was broken versus what was
added, and lets S2 be reviewed as a compiler fix on its own terms.

## 11. Implementation notes (2026-08-06, the I3 + I5 build)

Appended by the slice that built S1–S5. The paper above is unchanged; this
section records where the built thing differs from it and why. Three of the
four items are things the paper asserted without probing.

### S1–S5 shipped as written

`next(&mut self)` (P1), `Range with Iterator<i32>`, `ListIterator<T>` +
`List::iter` (P0), the seven adapters as trait defaults —
`map`/`filter`/`take`/`skip`/`enumerate`/`zip`/`chain` — and the terminals
`to_list`/`fold`/`for_each`/`count`/`any`/`all`/`rev`, `rev` being §6's
materializing fallback. §3's home call held: a terminal written in a
trait-SUBJECT inherent impl (`impl Iterator<type T> { .. }`) is still the
verbatim B4 error on `self.next()` after B55/B56/B58, so trait defaults are
the only home, exactly as §3 argued.

### Deviation 1 — `to_set`/`to_map` are `List` methods, not trait defaults

§5 recommends them "as trait defaults" and says their bounds "already have a
home". They do not. `Iterator<T>` does not bound `T`; a trait default may
not require a bound its trait does not declare; and a member cannot carry a
bound of its own that unifies with `T`. Written as a default, `to_set` is a
compile error **at its own definition**, before any call:

```
generic parameter 'T' is missing the bound ': Hashable' required by this call
  … the bound is declared here  →  impl Set<type T: Hashable>
```

Probed in all three shapes — the direct `Set::new()` construction, a forward
to a bounded `List::to_set`, and a bounded trait-subject impl (which fails
earlier, on B4). A member generic that carries the bound
(`fun to_set<E: Hashable>(mut self): Set<E>`) type-checks in the body and
then cannot be inferred at any call site, which is the `collect` ergonomics
the owner rejected, arriving by a different road.

Shipped instead, beside the bounds they need — the `join`-on-`Display`
precedent:

```vilan
impl List<type T: Hashable>            { fun to_set(self): Set<T> }    // set.vl
impl List<(type K: Hashable, type V)>  { fun to_map(self): Map<K, V> } // map.vl
```

`to_map`'s impl subject binds the element **structurally**, which is how both
of a `Map`'s requirements get stated at once; that this parses and
monomorphizes is itself a finding. The chain reads
`xs.iter().filter(f).to_list().to_set()`. Moving them onto the trait is
purely additive whenever per-member bounds exist, and the constraint is
pinned as a compiler fact so the record says why they could not be there
first.

### Deviation 2 — §4 option (ii) did NOT land, and the reason is not performance

§8 asked for a measurement before the eager forms were re-expressed. The
rewrite was made, measured, and reverted; both results are recorded here.

**The blocker: an async closure cannot adapt through an adapter chain.**
Async polymorphism (A.1) instantiates an async instance of the *callee* when
the closure argument is async. An adapter stores the closure in a struct
field and calls it from a trait-dispatched `next`, where there is no single
concrete callee to instantiate, so the pass refuses it:

```
an async closure cannot adapt a trait/generic-dispatched call (the concrete
callee varies per instantiation); bind the callee concretely, or declare the
trait parameter `async || T`
```

With `List::map` re-expressed, that error lands inside `list.vl` and
`vilan/test/adapt.vl` — the corpus program that exists to pin adaptation —
**fails to build**. `map`, `filter` and `fold` all break this way;
`for_each` survives, having nothing to return. So option (ii) as written
removes a shipped, corpus-pinned capability. Pinned `#[ignore]`d as
`an_async_closure_adapts_through_an_adapter_chain`, with a repro that
touches no std: the same failure comes from a user's own eager helper
written over the adapters.

**The measurement, separately.** `map`→`filter`→`fold` over 20 000 elements,
50 rounds, best of seven, after warm-up: **8–9 ms eager against 49–50 ms
through the adapters — about 5.5x.** §8 predicted a per-element call cost but
not the real driver, which is two O(n) *deep copies* the eager form does not
pay: `iter()` snapshots the list (rule 1 — storing a value in a slot that
outlives the call copies it, emitted as `__clone(self)`), and the terminal's
`mut self` copies the chain that holds that snapshot. The §8 claim that "the
adapter chain in between allocates nothing that survives" is therefore wrong
for a `List` source. Output was byte-identical either way, so observable
behaviour was preserved; the cost is real.

**What not landing costs.** Less than §4 assumed. The collision §4 exists to
remove does not arise in what shipped: `List` does not implement `Iterator`
(S6/`Iterable` was left for its own slice), so the lazy `map`/`filter` are
reachable only through `.iter()` and the two are told apart by the receiver
type at the call site. That is §4's own "degrades gracefully" state, and it
already has one meaning per name. What remains unpaid is the duplicated
body — four short eager methods — which is the smaller half of (ii)'s case.

**For the owner.** (ii) needs adaptation to follow a trait-dispatched callee
before it can land at all, and wants an answer on the 5.5x even then. Both
are recorded rather than decided here.

### Deviation 3 — S6 (`Iterable`) untouched

Open question (b) recommends S6 as a follow-on and this slice took that.
`Iterable<T>` is left exactly as it shipped — one parameter, returning a bare
`Iterator<T>`, still unusable under B4 and still implemented by nothing but
its own blanket impl. The collections page stops claiming it works. The
two-parameter `Iterable<T, I: Iterator<T>>` that S6 wants is unaffected by
anything here.

### A compiler fix the arc had to make: `all` collided with `Promise::all`

`Iterator::all` is in §3's table. Adding it colored every caller of
`xs.iter().all(p)` **async**, down to an `async` `main`, for a program with
nothing async in it — and the const-eval interpreter then refused such a
program outright ("async (macro bodies are synchronous)"), which is how the
corpus gate found it.

The cause is `async_infer::dispatch_candidates`. An `OnType` re-dispatch does
not carry its trait, so it falls back to every member with the call's name and
takes the caller async if any is — sound and deliberately over-approximate.
STATICS were in that set. `std::promise::Promise::all` is an `async external`
static and `promise` is a force-loaded core module, so the name alone was
enough. A dispatched `receiver.name()` can never select a member with no
receiver, so the scan now keeps only members whose first parameter is `self`
(`is_self_method`, the post-analysis twin of the analyzer's own). It is a
narrowing of an over-approximation, not a change of contract; no corpus golden
outside this arc's own moved a byte. Pinned in both directions, including that
a genuinely async dispatched member still colors its caller.

The alternative was renaming `all`, which would have been a spec deviation
hiding a defect every future same-named pair would hit again.

### Two defects found on the way, both pre-existing, both pinned `#[ignore]`d

1. **A user `impl List<type T>` block makes the checks report against std's
   own `List::map`/`List::filter`** — "the type of 'result' is never fully
   determined" — on roughly half of otherwise identical compiles. The
   diagnostic is spurious *and* it points into frozen std territory
   (`analysis-reuse.md` §6), so a user's impl is un-freezing entities it does
   not own. Reproduced on 39c951c at 14/30 through the CLI, so it predates
   this arc. It is cold-path only: the base cache serves every compile after
   the first in a process and always serves the clean world, so a loop that
   does not clear the cache is vacuous.

   **Fixed 2026-08-06 (B77).** The cause is not in the arc's territory at
   all: a constraint id does not identify one declaring FILE. `impl
   Subject<type T>` inherits the subject's constraint id on purpose
   (`register_subject_binders`), so a user's `impl List<type T>` *is*
   `list.vl`'s `T`. The residual-generic leak check collapsed that
   many-to-one relation into a `HashMap<TypeId, SourceId>` with `.collect()`
   — last write wins over a randomly-seeded hash iteration — so the recorded
   declaring file flipped run to run, and half the time the entry won and
   every `List<T>` inside `list.vl` read as foreign. The check now keeps the
   SET of declaring files and asks whether the binding's own file is among
   them. The arc's contribution was reachability: `impl List<type T>` is a
   natural thing to write only once `List` has an iterator worth extending.

2. **The protocol loop drops a tuple element's type when the iterator's
   element IS its own generic parameter.** `for pair in [(1, "a")].iter()`
   gives "cannot access field '0' on type T", while the same iterator pulled
   by hand (`if cursor.next() is Some(let pair)`) is fine, and the native
   `for pair in [(1, "a")]` over a plain `List` has always worked. The repro
   defines its own trait and cursor, so this is neither std's nor the arc's;
   `List::iter` is simply the first generically-elemented iterator in std, so
   it is the first thing that could reach the shape. `enumerate` and `zip` are
   unaffected — they name their tuple element structurally in the `with`
   clause — which places the defect in the loop binding's substitution alone.
   Documented on the collections page rather than left to be discovered.

   **Fixed 2026-08-06 (B78).** The substitution was simply absent from one arm.
   `iterable_element_type` reads the element off the DECLARED return type of the
   subject's `next`, and that payload is written in the SUBJECT's own parameters
   — abstract until instantiated against the receiver's arguments, exactly as
   the `Trait` arm one match-arm below already did with the trait's (which is
   why a bounded-generic loop, `for v in source` with `I: Iterator<T>`, always
   worked). The struct/enum arm now builds the same instantiation context from
   the subject's declared parameters. It was never about tuples: a bare `T`
   admits nothing, so a struct element's field, a nested `List`'s `len()`, an
   `Option`'s method and a closure element's CALL all refused identically, and
   the `&mut` form (`next_mut`) and an enum-shaped subject were affected too —
   eight pins, six of them red without the fix. §11's own claim that "the native
   `for pair in [(1, "a")]` over a plain `List` has always worked" is right and
   is now the regression guard beside `enumerate`/`zip`.

   A separate defect fell out of repairing the pin, which named its protocol
   method `step` and so never reached the protocol at all: `for x in subject`
   over a CONCRETE struct with no `next` is not diagnosed. It lowers to a native
   `for...of`, which walks the receiver's flat FIELD array — the program prints
   the struct's fields and exits 0. That is P3/B56's defect one type-shape over
   (B56 closed it for a generic subject), pinned `#[ignore]`d as
   `a_for_loop_over_a_struct_without_next_is_diagnosed`.
