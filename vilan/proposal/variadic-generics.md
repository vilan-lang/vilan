# Variadic generics via mapped tuples over flat storage

> **Status: implemented** — see [`variadic-generics-plan.md`](variadic-generics-plan.md)
> (shipped, commits bc360e9…3d00f5c): `combine` works end to end and is load-bearing in the
> reactive UI and the todo example. `keyof`/spread-params/elision remain deferred (the
> plan's own deferred section).

> Supersedes the parameter-pack exploration (previous commit). The driving
> example is unchanged — `std::reactive::combine`, the product combinator the
> reactive UI needs (`reactive-ui/todos.vl` imports it; `README.md` flags it as
> "not yet built — needs variadic generics"). The mechanism is different: instead
> of dedicated pack syntax, three orthogonal, independently-useful tuple features
> — **arity-bounded generics, mapped tuple types, and tuple comprehensions** — out
> of which `combine` falls as an ordinary function.

```vilan
//              ┌─ T is a tuple of arity ≥ 2
fun combine<T: (2..)>(sources: (U in T: Source<U>)): Source<T> {
//                             └─ for each element U of T, this slot is Source<U>
    let snapshot = || (source in sources = source.get());   // a tuple comprehension
    let derived = Source::new(snapshot());
    for (_, source) in sources {                            // iterate a tuple
        source.sub(|| derived.update(snapshot()));
    }
    derived
}

let visible = combine((items, filter));        // Source<(List<Todo>, str)>
visible.derive(|(list, filter)| { … });        // destructures with binders shipped already
```

`combine` is not special-cased: it is a normal function written in the language
over a mapped-tuple parameter. The same features give `zip`, a tuple `map`,
element-wise `into`, etc.

---

## The load-bearing mechanism: monomorphization → compile-time unrolling

Iterating a heterogeneous tuple, mapping a template type per element, indexing a
slot whose type depends on the index — these are normally **dependent typing**.
In Vilan they are not, because **`T` is concrete at every monomorphization**, so
every tuple map / loop **unrolls at compile time**:

```vilan
for (_, source) in sources { source.sub(…); }   //  T = (A, B, C)
// emits, in the A,B,C variant:
sources[0].sub(…); sources[1].sub(…); sources[2].sub(…);
```

The body is *type-checked once* against the abstract element type `U` (its trait
bound, if any), then *expanded N times* against the concrete elements when the
call fixes `T`. This is exactly how generic functions already work — analyze
once, specialize per call — and it is what makes the whole feature affordable.

---

## Runtime model: flat storage, but **distinct types**

**Flat lowering.** A nested tuple lowers to a flat array at construction, using
each element's statically-known width:

```vilan
let a = (1, 2);
let b = (a, 3);
// today:  const b = [a, 3];           ->  [[1,2], 3]
// flat:   const b = [a[0], a[1], 3];  ->  [1, 2, 3]
```

So `(A, B, C)`, `((A, B), C)`, and `(A, (B, (C, ())))` are the **same value at
runtime** — coercing between them is zero-cost *when requested*.

**Types stay distinct (the deliberate choice).** Equal runtime layout does *not*
make them the same type. `(Point, Color)` with `Point = (i32, i32)`,
`Color = (i32, i32, i32)` is **not** interchangeable with `(i32, i32, i32, i32,
i32)` — nesting is a real type boundary, so a 5-int tuple cannot silently satisfy
a `(Point, Color)` parameter. Flattening happens **only** through an explicit
spread, a comprehension, or an explicit annotation — never implicitly. This keeps
the perf win without dissolving the type system's structure.

**Costs.** Construction copies the flattened elements (a later optimization elides
the copy when a source operand is dead — *deferred*, needs liveness/move
analysis). Extracting a sub-tuple is the reverse — `let (a, rest) = x` where
`rest: (B, C)` reslices `[x[1], x[2]]` (copy or view).

---

## Surface syntax

### Arity & element bounds — `T: (..)`

A tuple bound on a generic: an arity range, optionally with a per-element bound.

```vilan
T: (..)            // any tuple (incl. () and 1-tuples)
T: (2..)           // arity ≥ 2
T: (..10)          // arity ≤ 10
T: (2..10)         // 2 ≤ arity ≤ 10
T: (..: Display)   // any tuple, every element bound by Display
T: (2..: Into<bool>)   // ≥ 2 elements, each Into<bool>
```

The element bound is what lets a uniform body type-check (`item.to_string()`
needs `(..: Display)`; `item.into()` needs `(..: Into<_>)`).

### Mapped tuple types — `(U in T: F<U>)`

A tuple type built by mapping a **single-hole** template over `T`'s elements:

```vilan
(U in T: Source<U>)   //  T = (A, B)  ->  (Source<A>, Source<B>)
(_ in T: bool)        //  ignore the element, every slot is bool
(U in T: Readable<U>) //  trait template => an EXISTENTIAL slot (any impl)
```

A template is either a **concrete constructor** (`Source<U>` — the exact slot
type) or a single **trait** (`Readable<U>` — an existential slot, "any type
satisfying it", the form `combine` uses). Map preserves arity. The single-hole
restriction keeps inference decidable (see below); multi-hole templates
(`Map<U, V>`, `U::Assoc`) are **deferred**.

### Tuple comprehensions (value) — `(x in xs = e)`

The value-level twin of the mapped type — `:` ascribes a type, `=` binds a value:

```vilan
(source in sources = source.get())   //  (s0.get(), s1.get(), …)
(item in items = item.into())
```

### Tuple iteration — `for (_, item) in items`

Yields `(key, element)` per element; the body is unrolled per element at
monomorphization. The `key` slot is a placeholder in the core (`_`); a usable
`keyof`-typed key for in-place indexing is **deferred** (below).

### Explicit flatten / concat — spread `..`

The *only* way nested becomes flat, in both type and value position:

```vilan
(..T, U)       // type:  append U to the elements of tuple T
(..a, b)       // value: a = (x, y)  ->  (x, y, b)
```

The **value** form is SHIPPED — §T below, which settles everything these two
lines left open (the `..` ambiguity, the positions, abstract packs, the
non-tuple error). The **type** form is not built; §T.7 records why and what it
would take.

---

## Inference

`combine((a, b, c))` must infer `T` **backwards** through the mapped parameter
type: given the concrete argument `(Source<A>, Source<B>, Source<C>)`, solve
`(U in T: Source<U>)` for `T = (A, B, C)`. With a single-hole template this is
element-wise unification — reconcile each concrete slot `Source<Ai>` against the
template `Source<U>` to bind `U = Ai`, collect `T` — reusing the existing
`reconcile_type` tuple path (`analyzer.rs:5202`). Multi-hole templates would make
the inversion ambiguous, hence the single-hole rule.

For a **trait template** (`Readable<U>`) the inversion is by impl resolution
instead of structural unification: find `U` such that the concrete slot type
implements `Readable<U>`. Unambiguous precisely because `Readable` has no blanket
impl (one impl per type) — the reason a bespoke trait is chosen over `Into`
(below).

---

## Source vs Signal inputs — a bespoke `Readable<T>` trait

`combine`'s inputs are heterogeneous in a second way: `todos.vl` passes
`Signal`s, but a `Signal<T>` is a writable root, not a `Source<T>` (it *wraps*
one in `.node`). Two ways to let a slot accept either:

1. **A bespoke trait** both implement — `Readable<T>`, with one bridge method.
2. **`Into<Source<U>>`** — `Signal` converts to its `Source`; `Source` is covered
   for free by the reflexive blanket `impl type T with Into<T>` (`std/into.vl`).

Option 2 is more elegant *in principle* (no new trait; `Source` needs no impl),
and would be my pick **but for a verified blocker**: target-directed dispatch
through the blanket impl doesn't work today. With both `Into<Wrap>` (the reflexive
identity) and an explicit `Into<i32>` in scope, `let x: i32 = w.into()` resolves to
the *identity* impl and errors `Expected i32, but got Wrap` — the annotation
doesn't steer impl selection. Resolving `Signal<A>: Into<Source<A>>` hits exactly
this: the identity `Into<Signal<A>>` competes and wins. Fixing it is a separate
analyzer change in the generic-resolution cluster ([[analyzer-stabilization]]).

A **bespoke trait sidesteps it entirely** — `Readable<T>` has no blanket impl, so
each type has exactly one impl and per-element resolution is unambiguous (verified:
a `first<R: Readable<i32>>` dispatches cleanly for two distinct implementors).
**Recommendation: Option 1.** It also formalizes the README's claim that
"`Source<T>` is the read interface every reactive value implements." Revisit
Option 2 if/when blanket-impl dispatch is fixed — `combine` would then need no
change beyond dropping the trait.

```vilan
// std::reactive — the read interface both a Source and a Signal satisfy.
trait Readable<T> {
    fun as_source(self): Source<T>;
}
impl Source<type T> with Readable<T> { fun as_source(self): Source<T> { self } }
impl Signal<type T> with Readable<T> { fun as_source(self): Source<T> { self.node } }
```

## Worked: the reactive combinators

```vilan
// product — the driving example, keyof-free. A slot is any Readable<U> (a trait
// template => an existential slot, inverted through its single impl per type).
fun combine<T: (2..)>(sources: (U in T: Readable<U>)): Source<T> {
    let nodes = (s in sources = s.as_source());     // (Source<U0>, Source<U1>, …)
    let snapshot = || (node in nodes = node.get());
    let derived = Source::new(snapshot());
    for (_, node) in nodes {
        node.sub(|| derived.update(snapshot()));
    }
    derived
}

// a homogeneous map, for contrast — element bound gives each item its method
fun labels<T: (..: Display)>(items: T): (_ in T: str) {
    (item in items = item.to_string())
}
```

`combine`'s result type is concrete at the call (inference fixes `T`), so the
downstream `.derive(|(list, filter)| …)` sees a concrete tuple and destructures
with the binders that already shipped.

---

## Deferred aspects (in scope conceptually, out of scope for the core)

- **`keyof` + indexed per-slot write** — a real key type usable as
  `v[key] = source.get()`, turning `combine`'s O(N)-per-update snapshot recompute
  into an O(1) per-slot write. Sound only where the index is statically known
  (an unrolled loop / literal); a `keyof` value that escapes to a runtime-dynamic
  position collapses `v[key]` to the union of element types. The core uses the
  recompute form and needs none of this.
- **Spread parameters — `fun f(...items: T)` / `f(1, "hi", true)`** — a second,
  varargs call convention alongside the single-tuple `combine((a, b, c))` form.
  Orthogonal; the core ships only the tuple-argument form. **Designed and
  SHIPPED — §S below.** This entry was a two-sentence sketch: it named the
  surface and nothing else. §S settles it.
- **Copy elision on flatten** — reuse a dead source operand in place
  (`const b = [a[0], a[1], 3]` without copying `a`). Needs liveness/move analysis;
  correctness holds without it (it just copies).
- **Multi-hole mapped templates** — `(U, V in T: Map<U, V>)` and associated-type
  templates. The core is single-hole.
- **Implicit structural flatten coercion** — *explicitly rejected* (see "distinct
  types"); flattening is always opt-in.

---

## Risks & mitigations

- **Flat lowering is a breaking codegen change.** It is the one stage that is
  *not* corpus-byte-identical: any file with a genuinely nested tuple regenerates
  its golden. Today that is exactly one file (`vilan/test/destructuring.vl`:
  `(1, (2, "z"))` → `[1,2,"z"]` instead of `[1,[2,"z"]]`). All tuple access is
  compiler-generated from static types, so the rewrite is mechanical; validate by
  *running* every corpus program and diffing behavior, not just the `.js`. (Heed
  [[golden-regen-rebuild-debug]] — rebuild the debug binary before regenerating.)
- **Symbolic mapped types through generics.** While `T` is abstract (inside
  `combine` before specialization), `(U in T: Source<U>)` is a symbolic type that
  must survive substitution and expand to a concrete `Type::Tuple` when `T` is
  fixed. Needs a `Type::Mapped`-style representation, expanded in the same place
  generic substitution happens.
- **Mapped-type inference inversion** is the one genuinely new analysis; the
  single-hole rule keeps it to element-wise unification.
- **Generic-tuple-destructure propagation bug** (`pair<A,B>(): (A,B)` leaves a
  destructured binding abstract — see `analyzer-refactor.md`). Not on the core's
  critical path (combine's `T` is concrete at the call), but it bites a `combine`
  nested inside another *generic* function; track alongside.
- **Type interning (#6 in analyzer-refactor).** `Type::Tuple`/`Type::Mapped` hold
  `Vec`s; keep them out of any interned identity set initially, or make them
  participate explicitly.

## Verification

- Flat-lowering stage: regenerate affected goldens; **run** the whole corpus under
  `node` and confirm identical behavior.
- Every later stage adds unused grammar/types → corpus byte-identical until a
  program opts in.
- New `tests/inference.rs` cases: arity-bound check + error, mapped-type inference
  (forward and inverted), a comprehension, and a **runtime** `combine` (2- and
  3-input, asserting recompute on each change).
- Acceptance: `reactive-ui/todos.vl` builds and runs end to end.

## Recommendation

Build the keyof-free core: flat lowering → arity/element bounds → mapped tuple
types + inference → comprehensions + tuple `for` → `combine` in std. Defer
`keyof`/indexed-write, spread parameters, and copy elision until a second caller
or a measured need justifies them. See `variadic-generics-plan.md` for the staged
plan.

---

# §S Spread parameters

Status: SHIPPED 2026-08-04 (backlog B3's implementable slice). §Deferred named
the surface (`fun f(...items: T)` / `f(1, "hi", true)`) and stopped; this
section settles it. Semantics are settled **by desugar**, as `mut` parameters
were (`mut-parameters.md` §2) — every ruling below is read off the desugar
rather than chosen separately. §S.9 records the implementation.

## S.1 The desugar

```vilan
fun f(...items: T) { body }    ≡    fun f(items: T) { body }
f(a, b, c)                     ≡    f((a, b, c))
```

**`...` is a call convention, not a type.** The callee is untouched: a spread
parameter *is* a tuple parameter, with the same declared type, the same binding,
the same storage, the same emitted JavaScript. All `...` says is that the call
site writes the tuple's elements out flat instead of writing the tuple. Under
the desugar the core's whole existing pack machinery — arity bounds, element
bounds, mapped types, comprehensions, flat storage, inverted inference — applies
unchanged, because it is applying to the very same tuple parameter it always
was.

This is the one design decision everything else follows from, and it is what
makes the feature affordable: no sibling mechanism to the pack, no second arity
system, no new type.

## S.2 What `T` is: the pack, not the element

In `fun log<T: (..: Display)>(...items: T)`, **`T` is the pack** — the tuple of
all the collected arguments' types — exactly as `combine`'s `T` is. It is *not*
each element's type and there is no homogeneous-varargs form:

```vilan
log(1, "hi", true)      //  T = (i32, str, bool)   — heterogeneous, by construction
```

The element bound is where a *uniform* requirement is spelled (`(..: Display)`),
which is the same place the tuple form spells it. A "`T` is the element type"
reading would have been a different feature — a homogeneous `List<T>` varargs —
and it is deliberately not this one: it would not lower to the pack, so it would
be a sibling mechanism rather than sugar.

The declared type may be anything the pack machinery already accepts:

```vilan
fun log<T: (..: Display)>(...items: T)                  // a bounded pack
fun combine<T: (2..)>(...sources: (U in T: Signal<U>))  // a mapped pack
fun pair(...xy: (i32, str))                             // a concrete tuple
```

The mapped form is the payoff: `combine(a, b)` instead of `combine((a, b))`,
with `T` recovered by the shipped inversion (§Inference) and no change to
`combine`'s body.

## S.3 Arity: inherited, not invented

Nothing here is a new rule; each is the desugar plus something that already
holds of tuples.

- **The spread parameter must be last**, and there is therefore at most one per
  signature. Collection has no other end. (*Parse-adjacent error.*)
- **It must declare its type.** The pack is what the collection produces, so it
  cannot be inferred from the collection. (*Parse-adjacent error.*)
- **Its binder is a plain name.** `...(a, b): T` is refused: destructuring the
  pack in the signature hides the arity rule the call site has to satisfy.
  Destructure in the body. (*Parse-adjacent error.*)
- **The legal call arities are exactly the arities the parameter's type
  admits** — decided by the shipped tuple-bound check, not by anything new.
  `T: (2..)` refuses `f(x)`; `T: (..10)` refuses an eleventh argument;
  `T: (..)` accepts `f()`, the **empty pack** `()`; a concrete `(i32, str)`
  accepts exactly two. A spread parameter is what makes 0- and 1-arity tuples
  reachable at all — source syntax cannot write `()` or `(x)` as a value — and
  they behave as the type system already says they do (§S.9 records the probe).
- **The fixed parameters ahead of it are matched positionally first.** Fewer
  arguments than there are fixed parameters is an arity error in its own right,
  and says `at least`, because there is no upper bound to name.

## S.4 The other direction — spreading a tuple INTO a spread parameter

**SHIPPED 2026-08-04, one slice later — §T.6.** This section shipped the
**callee** direction only, and refused `f(..existing)` because under the
desugar it is exactly `f((..existing))`: a **tuple-value spread**, which
§Surface syntax describes (`(..a, b)`) and which *was never built* — no spread
node in the AST, no parser production, no type rule. That was the correct read,
and it named the next slice precisely.

The refusal, honestly labelled: it was a **plain parse failure**, not the steer
this section's prose claimed ("found '.' expected an expression"), and its pin
was the only §S refusal asserted with a bare `assert_fails` rather than its
message. §T built the missing piece, and `f(..existing)` now works by the route
this section predicted — the call site's collection puts the spread inside the
tuple it was already building, and nothing in §S changed to allow it. The pin
became a working one; the shapes that are still wrong (a spread at a call to a
function with **no** spread parameter, a spread of a non-tuple) get real steers
now, which is what this section wanted all along.

## S.5 Conventions — refused, and for a reason that outlives v1

`own ...items`, `&...items`, `&mut ...items` are refused.

The argument a spread parameter receives is **a value the call site
constructs**. There is no caller-side tuple to transfer ownership of, and none
to alias — so `own` has nothing to move and a view has nothing to point at.
This is a different reason from `mut`'s exclusion (`mut-parameters.md` §2, where
the question "which thing is mutable" stops being one question), and unlike a
scoping decision it does not expire — confirmed by §T, which landed §S.4:
`f(..existing)` still *builds* a fresh tuple (it emits `f([...existing])`), so
there is still nothing for a convention to name.
Conventions on the individual arguments are unaffected — they are ordinary
expressions.

**`mut` is permitted**: `mut ...items: T`. `mut` is binder mutability, not a
convention (H9's distinction), and the collected tuple is the callee's own value
— so the desugar carries it through untouched, and `items` is a writable local
copy like any other `mut` parameter.

## S.6 Closures — refused v1, decided rather than silent

`|...items| …` is refused with its own message.

A closure is reached through its **type** far more often than a function is —
annotated, stored in a field, passed as a callback — and a closure type
(`sync |A, B| C`) has no variadic form. A variadic closure could therefore not
be named, stored, or passed anywhere; it would be callable only at its literal.
That is not a feature, it is a trap. Revisit if closure types grow a pack form.

The parser admits `...` in closure position precisely so the refusal can be a
sentence rather than a syntax error.

## S.7 Methods and trait signatures — refused v1

A `fun` inside an `impl` or a `trait` body may not declare a spread parameter.

Unlike `mut`, **`...` is part of the signature**: it changes how a call is
written, so it is not something a trait declaration and its impl may disagree
about (which is exactly the freedom `mut` has). And a method is reached by
member dispatch on three routes — inherent, through a trait, and through a
generic bound — so a convention that regroups arguments has to hold identically
on all of them, including a conformance check that today compares receiver
convention, arity and per-position conventions only.

v1 keeps `...` where the callee is named directly, and one rule covers trait
declarations, trait impls and inherent impls alike. The steer is the tuple
form: declare `fun m(items: T)` and call `m((a, b))`.

`external fun` is refused for the neighbouring reason (and the same one H9
gave): an external binds a host function whose calling convention is the host's,
and the pack has no host form.

## S.8 Two consequences worth stating out loud

- **The convention lives on the declaration, so a function *value* has the tuple
  form.** Passed where a `sync |(i32, i32)| i32` is wanted, `fun count(...items:
  (i32, i32))` goes through — and the callback calls it `f((4, 5))`, not
  `f(4, 5)`. A value's type is the parameter's; the desugar's left-hand side is a
  *declaration*, and a value has none. Pinned. (Calling a function through a
  `let`-bound name — `let f = count; f(…)` — does not work today for *any*
  function, spread or not; that is a separate, pre-existing hole.)
- **A spread declaration is not also callable in the tuple form.**
  `log((1, 2))` collects, as every call does, to the one-element pack
  `((i32, str))` — and then fails its own `(2..)` bound. Choosing `...` chooses
  the convention. Pinned.

## S.9 Ship record (2026-08-04)

The desugar is realized at **one** point: when the solver resolves a call whose
subject is a function whose last parameter is a spread, it replaces the
argument tail with a single synthesized `Expr::Tuple` entity — the very entity
a written tuple literal lowers to — and the call proceeds as the tuple form
through typing, bound checking, the rule-1/3/4 scans and emission. The
collection is memoized per call, because the constraint may defer and retry.
Emission has **no** spread-specific path at all: the flat tuple construction
(with its `...` splice for a tuple-typed element) is the one the tuple form
already emitted, and the callee's JavaScript parameter stays a plain
identifier.

Probed and confirmed rather than assumed: **0- and 1-arity packs work.** Tuple
*types* already admit `()` and `(A)` (`parse_tuple_type` has no minimum, unlike
the ≥2 value syntax), and a spread parameter is the first thing that can
produce such a value. `f()` under `T: (..)` binds `T = ()` and emits `f([])`;
`f(x)` binds `T = (i32)` and emits `f([x])`. Both are pinned, including their
emitted bytes.

Two **pre-existing** limits the pack inherits, each pinned beside its tuple-form
twin so it is on the record rather than mistaken for a spread bug — both fail
*identically* on `fun f<T: (2..)>(items: T)`, so the desugar reproduces today's
behaviour faithfully:

- **A pack that is still an abstract `T` cannot be indexed positionally.**
  `items.0` inside the generic body is "cannot access field '0' on type T" —
  the body type-checks once, before any arity is known. A concrete pack type
  (`...items: (i32, i32, i32)`) indexes fine.
- **A comprehension's source must be a MAPPED tuple**, so a bare
  element-bounded pack (`T: (..: Display)`) has no way to walk its elements and
  the element bound has no consumer of its own. The mapped form is what a
  comprehension reaches — which is why `gather` is the worked example.

Together these say the useful shapes today are the **mapped** pack and the
**concrete** pack; a bare `T: (..)` pack can be passed on but not taken apart.
Both are squarely in the `keyof`/tuple-`for` tail this proposal already defers,
and neither is made worse by the spread.

The one position audited and deliberately left alone: **`Type::Closure`** — a
spread function's value type is the tuple one (§S.8), which is what the closure
path already produced and why the call-site collection lives on the
declaration-resolving branch only.

One mechanism is honestly labelled rather than over-claimed. The collection is
**memoized per call**, because the constraint defers and retries (an
unannotated closure argument is the ordinary way in), and the pack should keep
one identity across attempts. That is **hygiene, not correctness**: dropping
the memo was planted and the whole suite stayed green, because the fixpoint
reaches the pack only through the argument list the call finally wires — a
second attempt orphans the first entity rather than miscompiling. It stays
because the language server re-analyzes per keystroke, and an unbounded entity
per deferral is a real cost there.

Two bugs the work found and fixed on the way:

- **A free `fun` declared inside an `impl` method's body was refused a
  spread**, because the in-member-body flag stayed set through a body it had
  already finished checking. Memberhood belongs to the impl/trait *item list*,
  not to everything lexically inside it. Pinned.
- The parser's spread refusals needed the **inferred** convention, not just the
  written prefix: `...items: &T` takes its `Ref` from the type. Pinned
  separately from the prefix cases.

---

# §T Tuple-value spread

Status: SHIPPED 2026-08-04 (backlog B3's next slice, the one §S.4 named).
§Surface syntax gave this feature **one line** — `(..a, b)  // value: a = (x, y)
-> (x, y, b)` — which settles the spelling and one position and nothing else.
This section settles the rest and records the build. Where §S was settled by
*desugar*, §T is settled by **one type rule**: a tuple construction's type is
the concatenation of its parts. Every ruling below is read off that rule, and
the feature's whole implementation is that rule plus a parser production — no
new emission, no new type, no new pass over the tree.

## T.1 The spelling, and why `..` needs no disambiguation

`..` is two `.` control tokens. The lexer has no `..` and no `...`; both of the
language's existing spreads spell themselves out of separate dots
(`parse_tuple_bound`, `eat_spread`), and this one does too.

**Vilan has no range operator.** Probed rather than assumed: `0..3` is not an
expression anywhere — `let r = 0..3` and `for i in 0..3` both fail today. The
`..` in expression position was simply unclaimed, and the ambiguity this slice
was expected to resolve does not exist.

What does exist, and is the real hazard, is that a **leading dot silently
absorbs dots**: `(1..3, x)` parses today as a two-element tuple whose first
element is a member chain over `1` with an error link, *emitting no parse error
at all* (the diagnostic comes later, from the analyzer). So the ruling is by
**position, not adjacency**:

> `..` is a spread only where an **element begins** — an entry of a tuple
> construction, or a call argument. It is recognized *before* the element's
> expression is parsed, so an expression that starts with anything else can
> never reach it.

`(1..3, x)` therefore starts its first element with `1`, never reaches the
spread branch, and parses exactly as it did before this feature existed. That is
pinned in the parser's own tests, because the value of the ruling is precisely
that it left something alone.

Two consequences worth stating:

- **What this costs a future range operator.** An infix or postfix range
  (`1..3`, `1..`) stays available — those begin with an expression. A *prefix*
  range (`..end`) is now claimed inside a tuple construction and an argument
  list. That is the deliberate trade: the spread is the older design, and it is
  the one with a caller.
- **No adjacency check**, matching `eat_spread` and `parse_tuple_bound` — the
  house idiom for a multi-dot marker. Tightening one of the three in isolation
  would be a worse inconsistency than the sloppiness it removes.

`...e` written where a value spread belongs is its own steer rather than a
`..` followed by a broken expression: *"`...` marks a spread PARAMETER, on a
declaration; a tuple-value spread is `..`"*. The two markers are one dot apart
and one is the sibling feature, so the confusion is the likely one.

## T.2 The type rule: concatenation

Each entry of a tuple construction contributes a **sequence** of element types:

- an ordinary entry contributes itself — one slot;
- a spread `..e` contributes **the elements of `e`'s tuple type**, in order.

The construction's type is the concatenation, in written order; its arity is the
sum. Bidirectional inference follows: an expected tuple type is sliced by the
**count of slots produced so far**, not by the entry's index, so an annotation
still reaches the entries after a spread. A spread's operand takes no constraint
of its own — its arity is what decides where the following entries land in the
expected tuple, so there is nothing to slice until it is known.

The operand must be a tuple. Anything else is refused, naming the type:

> `cannot spread 'i32': `..` splices the ELEMENTS of a tuple into the
> construction, so its operand must be a tuple`

A **mapped** pack (`(U in T: Signal<U>)`) is a tuple once its source is
concrete, and is expanded before the question is asked — so a mapped pack
spreads like any other.

## T.3 Positions: all of them, and the lone spread

Leading, trailing, interleaved, and **any number** of spreads, freely mixed with
ordinary entries. Decided rather than inherited: concatenation is
position-independent and associative, and the emission is a JS array literal
whose splices may appear anywhere in it. A rule against interleaving or against
a second spread would have no mechanism behind it — it would be a restriction
invented to look careful.

```vilan
(..a, x)        (x, ..a)        (x, ..a, y)        (..a, ..b)
```

**A construction whose only entry is a spread is a tuple, not a group.**
`(..a)` is legal. The ≥2-entry minimum on a written tuple exists for exactly one
reason — `(e)` has to stay a grouping — and `..e` is not an expression outside
element position, so the ambiguity that motivates the minimum cannot arise. This
shape is not decorative: it is what `f(..pair)` desugars to (§T.6), and it is
the only shape in which an abstract pack may be spread (§T.4).

The **empty** tuple spreads too: `(..none, 7)` where `none: ()` is the 1-tuple
`(i32)`, emitting `[7]`. `()` and `(x)` are arities source syntax still cannot
*write*, but §S made them reachable and this concatenates them like any other.

## T.4 Abstract packs: the concatenation of one

§S.9 recorded that a still-abstract pack "can be passed on but not taken apart"
— `items.0` inside a generic body is refused because the body type-checks once,
before any call fixes the arity. A **value** spread does not need positions at
runtime (the splice is arity-agnostic), so the recorded limit does not settle
this on its own. It is settled by asking what the constructed *type* would be.

`(..items, x)` where `items: T` is an abstract pack has the type "T's elements,
then X" — a **symbolic concatenation**, the value-side twin of `Type::Mapped`,
which would have to survive substitution and expand when `T` is fixed, and which
would need the type-level `(..T, U)` (§T.7) before it could even be annotated.
That is a real slice, not a corner of this one.

But the degenerate case needs none of it. **Concatenation of one is identity**:
`(..items)` — the lone spread, nothing to concatenate with — is `T`, unchanged.
So the rule falls out whole rather than being carved:

> A spread contributes its operand's element **sequence**. An abstract pack's
> sequence is not known, so a construction containing one is well-typed only
> when there is nothing to concatenate it with — when the spread is the
> construction's only entry.

```vilan
fun outer<T: (..)>(...items: T): i32 { inner(..items) }      // OK — forwards the pack
fun outer<T: (..)>(...items: T): i32 { inner(..items, 9) }   // refused, with the reason
```

This is not a consolation prize: the lone form is **the** §S.4 use case.
Forwarding a pack to another *spread* function is exactly `inner(..items)` —
and it is not the same as `inner(items)`, which collects to the one-element pack
`((T))` by §S.8. `..` is what a pack hand-off needs, and it is the shape this
rule admits.

The refusal names the reason and both steers (spread it alone, or take a
concrete tuple type), because the fix depends on which the author meant.

## T.5 Emission and the rest of the compiler: already written

**Emission needed no code at all**, and that is the design working rather than
luck. Tuples store flat, so the transformer already splices a *tuple-typed*
element's slots into a constructed tuple (`[...inner, 12]`) — the shape §S
records as having needed no spread path of its own. A value spread produces
**the same bytes** as the nested element it replaces:

```vilan
let inner = (10, 11);
(inner, 12)      // type ((i32,i32), i32)   emits [ ...inner, 12 ]
(..inner, 12)    // type (i32, i32, i32)    emits [ ...inner, 12 ]
```

The two differ *only* in type. Flat storage is what makes that true, and it is
why `.2` on the concatenation resolves to the same slot the nested form reaches
through `.0.1` — offsets come from the type's flat widths, which already sum
across nesting.

The mechanism that carries this through the whole compiler is that **`..e`
forwards to its operand's entity instead of wrapping it**, the way `const`
already does, with the spread recorded as a *mark* on that entity. So every pass
after the walk — clone sites, escapes, const reach, the rule-1/3/4 scans, the
transformer — sees the very expression a nested element would have been, and
cannot get the spread wrong by not knowing about it. Exactly one reader consults
the mark: the tuple's type rule.

## T.6 The circle: `f(..tuple)`

§S.4's refusal is retired. Under the desugar `f(a, b)` is `f((a, b))`, so
`f(..pair)` is `f((..pair))` — the call site's collection already builds an
`Expr::Tuple` from the arguments, and a spread argument simply lands inside it.
No new call-site machinery, no second desugar; the bound is checked on the
**concatenated** arity, so a spread that violates `(2..)` still fails with the
shipped bound error, naming the concatenation.

```vilan
fun need2<T: (2..)>(...items: T): i32 { … }
let pair = (1, 2);
need2(..pair);        // T = (i32, i32) — emits need2([ ...pair ])
need2(..pair, 7);     // T = (i32, i32, i32)
```

The shapes that are still wrong get their own steer. A spread argument to a
function with **no** spread parameter builds no tuple, so it is refused with the
tuple form:

> ``` `..` splices a tuple's elements into a tuple construction, so it belongs
> inside `(…)` — or at a call to a spread parameter, which builds one. Write
> `f((..pair))` to pass the concatenation as a single tuple argument```

That check is **one post-solve sweep**, not a check at each site that could host
a `..`. Since the type rule reads the mark in exactly one place, a spread that
reaches no tuple construction is not something an individual call path can be
trusted to notice — a call to a plain function, to a closure, to a variant
constructor each resolve down their own road, and a road not yet built would
inherit the hole. Asking afterwards *which marks landed in a tuple* covers all
of them at once and cannot be dodged.

## T.7 What does NOT ship: the type-level `(..T, U)`

§Surface syntax's other line — `(..T, U)` as a **type** — is not built, and was
not in this slice. `parse_type_atom` has no `..` branch, so it does not parse;
nothing regressed, and nothing new blocks it.

It is the natural companion to §T.4: a symbolic concatenation type is what would
let an abstract pack be spread alongside other elements, and the type surface is
what would let the result be *annotated*. Building it means a `Type::Concat`-
style representation that survives substitution, expands when its source tuple
resolves, participates in `reconcile_type` and the tuple-bound arity check, and
prints — the same list `Type::Mapped` needed. It belongs with the `keyof` tail
this proposal already defers, and it now has a concrete demand attached to it
(the §T.4 refusal) rather than being a line in a syntax sketch.

## T.8 Ship record (2026-08-04)

The whole feature is four edits and a sweep:

- **Parser** — `Node::Spread(Box<Spanned<Node>>)`, produced by one helper that
  is tried where an element begins, from `parse_paren_atom`'s entry list and
  `parse_argument_list`. A leading `..` also settles the tuple/group fork on the
  spot, which is what makes `(..a)` a construction.
- **Analyzer walk** — `Node::Spread` forwards to its operand's entity and marks
  it (§T.5), so there is no `Expr::Spread` and no pass to teach.
- **Type rule** — `infer_type_path`'s `Expr::Tuple` arm concatenates, with the
  constraint cursor counting slots rather than entries, and the lone-spread
  identity short-circuit ahead of it.
- **`check_tuple_spreads`** — the post-solve sweep: every mark must have landed
  in a tuple construction, must be a tuple, and must be alone if it is abstract.
- **Formatter** — `..` then the operand printed *without* the operand rule's
  parentheses, since `..` takes the whole following expression and a wrap would
  be token drift rather than a faithful reprint.

Two things probed and confirmed rather than assumed, both recorded above because
they changed a ruling: **there is no range operator to be ambiguous with**
(§T.1), and **`(1..3, x)` already parsed silently** into a member chain (which
is why the ruling is positional, and why the pin asserts the old behaviour
rather than a new error).

One thing found and left alone: `reconcile_type`'s tuple arm zips without a
length check, so `(i32, str)` reconciles against `(i32, str, bool)` and yields a
2-tuple. Arity is caught elsewhere (argument count, the tuple bound), and
concatenation exercises this path harder than the tuple form did — but no
observed failure came out of it, so it is recorded here rather than fixed
speculatively as part of a feature slice.
