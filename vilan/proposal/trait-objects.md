# Trait objects — one representation, two meanings (B4)

> Status: DRAFT (awaiting owner review) — filed from backlog B4; proposal-first per the house rules.
>
> Origin: backlog B4, open since the lettered-section scheme began, carrying
> three cycles of accumulated pressure: B29's asyncness note (2026-07-20),
> B57's `reconcile_type(Trait, Struct)` find (2026-08-06), B72's
> parameter-position steer and its internal-error amendment (2026-08-06), and
> the S6/`Iterable` residue the iterator-adapters arc left riding on this
> file. Nothing here is implemented. The paper recommends that most of it
> never be.
>
> Every claim below about what the compiler does today was checked against
> source **or run through the repo compiler** as a probe. The probes are
> called out inline (P1…P18), because five of them (P7–P11) found defects that
> change the design — including a **live resource leak** reachable in nine
> lines of ordinary code, where a bare trait annotation silently suppresses a
> destructor (§2.2), and a heterogeneous container that already compiles and
> runs (§2.3). A sixth (P17) found the records and the compiler disagreeing
> about R11. Probes ran against `target/debug/vilan` built in this worktree
> from `next @d5de163`. §13 is the open-questions set; every section before it
> ends in a recommendation, not a ratification.

## 0. The problem and the thesis

`let x: Display = bag;` compiles. `x.show()` does not:

```
Error: cannot call 'show' on a value of bare trait type 'Display': a trait is
not a value type (vilan has no trait objects). Use a generic parameter
(`<T: Display>`) or a concrete type.
```

The backlog entry frames B4 as a feature request — *"making it work by value
needs a `(value, vtable)` representation. Nothing demands it yet."* Three
cycles of pressure have not produced a demand that blocks anything. They have
produced something else, and it is what this paper is actually about.

**Thesis: B4 is not a missing feature. It is a missing distinction.** The
type system has one representation, `Type::Trait(Id, Vec<TypeId>)`
(`crates/vilan-core/src/type_.rs:37-41`), and it carries two entirely
different meanings:

1. **The abstract `Self` of a trait default**, mid-monomorphization — an
   internal placeholder guaranteed to be replaced by a concrete type before
   anything is emitted. `Self` interns as exactly `Type::Trait(trait_id, [])`
   (`analyzer.rs:16419`).
2. **A value the user annotated with a trait** — a real runtime value whose
   concrete type the compiler has thrown away, guaranteed *never* to become
   concrete.

These are opposites, and they are the same variant with the same payload. The
compiler tells them apart by asking where in the scope tree it is standing —
`is_in_trait_default` (`analyzer.rs:2262-2271`) walks the scope chain — which
works exactly as far as the scope chain reaches and no further.

Every symptom in B4's file is one subsystem acting on meaning (1) while the
program meant (2):

| subsystem | its rule for `Type::Trait` | right for `Self` | right for a value |
|---|---|---|---|
| method lookup (`analyzer.rs:21374`) | reject unless in a default body | yes | yes — by accident of scope |
| generic-bound checking (`analyzer.rs:2581-2589`) | indeterminate, skip | yes | **no** → §2.1's internal error |
| resource classification (`analyzer.rs:4373`) | "never a resource", *definitively* | yes | **no** → §2.2's leak |
| `reconcile_type` (`analyzer.rs:19430`) | a concrete value satisfies it, and the result is the *concrete* type | yes | **no** → §2.2's laundering, §2.3's silent narrowing |

The last two rows are defects, not design. One of them destroys data.

So the recommendation, stated up front and defended through §12:

> **Do not build trait objects.** The demand survey (§3) finds three real
> sites, all inside std's own runtime plumbing, all already served by working
> closure erasure, and none blocked — while the one time this codebase
> measured erasure against monomorphization, monomorphization won by 14–18%
> and the erasure was cut back to the single site that cannot do without it
> (§3.3). 17 of std's 39 traits could not be objects anyway, including every
> operator trait, `Iterator`, and the `Wire` trait behind the tree's largest
> hand-written vtable (§4).
>
> **Do build the distinction.** Split `Type::Trait`'s two meanings, make a
> trait illegal in value position as the spec has said since it was written
> (§1.6), and rewrite std's five bare-trait sites to `Self` — which P3 proves
> works today. That closes the internal error, closes the resource leak, and
> costs the language nothing it uses.

If the owner rules the other way, §§5–11 design the feature properly rather
than sketching it, because a paper that only argues one side is not evidence.

## 1. Ground truth — what the language does today

### 1.1 One type, two meanings

```rust
// A trait and its generic arguments (`Display` -> `Trait(display_id, [])`,
// `Into<bool>` -> `Trait(into_id, [bool])`, `Readable<U>` ->
// `Trait(readable_id, [U])`). The arguments drive parameterized-trait impl
// selection and a mapped trait template's inversion.
Trait(Id, Vec<TypeId>),
```
`crates/vilan-core/src/type_.rs:37-41`

There is one type enum, `Type` (`type_.rs:18-56`); there is no `TypeKind` and
no separate sort for bounds. `Type::Trait` is structurally identical to
`Type::Struct` and `Type::Enum` — the same `(Id, Vec<TypeId>)` shape, in the
same lattice. **That is why a trait flows into value positions unchecked:
nothing in the representation says it may not.**

And `Self` is one of these. `analyzer.rs:16419` interns the receiver type of a
trait's own members as `Type::Trait(id, Vec::new())`. So inside

```
trait Walk<T> {
	fun step(self): Option<T>;
	fun twice(self): Option<T> { self.step() }
}
```

`self` in `twice`'s body has *the same type* as `x` in `let x: Walk<i32> = …`.
The first is legitimate and monomorphizes; the second has no implementation
anywhere.

### 1.2 The refusal sites, mapped

There are exactly **two** refusals of a bare trait *value*, and they disagree
about everything except their wording.

**(a) `MethodLookup::BareTraitValue`** — a function-local enum variant inside
the method-call resolver (`analyzer.rs:21207`), constructed at exactly one
site (`analyzer.rs:21374-21375`):

```rust
// The only legitimate bare-`Type::Trait` receiver is `self`/`Self`
// inside a trait default body, re-dispatched at codegen to the
// concrete specialization. A *value* typed as a bare trait
// (`let x: Display = 5; x.to_string()`) has no concrete type to
// dispatch to — vilan has no trait objects — so reject it rather
// than silently lowering to the empty abstract method.
if member.is_some() && !self.is_in_trait_default(id) {
    MethodLookup::BareTraitValue(trait_id)
```

and reported at one site (`analyzer.rs:21684-21702`). Note `member.is_some()`:
if the trait does *not* declare the name, the call falls through to the
ordinary "has no method" error instead. The diagnostic carries `note: None`
and anchors on the **receiver** span.

**(b) B72's parameter steer** — `argument_mismatch` (`analyzer.rs:20455-
20498`), reached only from the `None` branch of the parameter-first
`reconcile_type` at `analyzer.rs:21053`:

```rust
format!(
    "parameter '{parameter_name}' has bare trait type '{trait_name}': a trait is \
     not a value type (vilan has no trait objects), so it cannot accept {got}. \
     Declare a generic parameter bounded by the trait instead — `<T: \
     {trait_name}>` with '{parameter_name}: T'."
)
```

It anchors on the **argument** and carries a note at the parameter's
declaration with its own `SourceId`, so it renders across modules
(`analyzer.rs:20480-20488`).

A third refusal exists but is a different question: `bare_name_not_a_value`
(`analyzer.rs:23725-23728`) rejects the trait *name* used as a value
(`let q = Display;`). That is a name-resolution rule, not a type rule, and B4
does not change it.

Why (b) exists at only one site is the whole shape of the problem
(`analyzer.rs:20448-20454`):

```
/// Why only here: a CALL is the one position that reconciles
/// parameter-first (so bindings key on the callee's generics), which makes
/// it the one position that ever asks `reconcile_type(Trait, Concrete)`.
/// Every other position — a `let` annotation, a return, a field, a method
/// argument — reconciles value-first and lands on the `(Struct|Enum, Trait)`
/// arm, which ACCEPTS.
```

**Neither refusal refuses a declaration.** Both refuse a *use*.

### 1.3 The positions that accept, measured

> **P1.** One trait `Display`, one implementing `struct Bag`, six programs
> putting the bare trait in six positions, `vilan check` on each.

| position | program | verdict |
|---|---|---|
| `let` annotation | `let x: Display = Bag { n = 1 };` | **compiles** |
| `let` + use | `… ; x.show()` | refused — `BareTraitValue` |
| free-fn parameter | `fun render(v: Display)`, called with a `Bag` | refused — B72 steer |
| return type | `fun make(): Display { Bag { n = 1 } }` | **compiles** |
| struct field | `struct Holder { item: Display }`, built with a `Bag` | **compiles** |
| method parameter | `impl Screen { fun draw(self, v: Display) }`, called | **compiles** |

Four of six positions accept. The four that accept are the four that reconcile
**value-first** and land on `reconcile_type`'s `(Struct|Enum, Trait)` arm
(`analyzer.rs:19430-19455`), which returns `a.clone()` — the *concrete* side.
The trait is absorbed at the check and then reasserted by the annotation,
which is how a value ends up carrying a type it has no implementation for.

All six are pinned, deliberately, under `b72_*`
(`crates/vilan-core/tests/inference.rs:41678-41739`) plus
`bare_trait_value_method_call_is_rejected` (`inference.rs:4801-4818`). The
current state is described, not assumed.

### 1.4 What the reconciler accepts

`reconcile_type` (`analyzer.rs:19340-19599`) and its read-only twin
`compare_type_rigid` (`analyzer.rs:19683-19717`) agree exactly:

| left | right | site | verdict |
|---|---|---|---|
| `Trait(id, a)` | `Trait(id, b)` — **same id** | `analyzer.rs:19538` | accept, arguments reconciled |
| `Trait(id₁, _)` | `Trait(id₂, _)` — different | falls to `19595` | fail |
| `Struct`/`Enum` | `Trait(id, _)` | `analyzer.rs:19430` | accept iff it implements; result is the **concrete** type |
| `Trait(_, _)` | `Struct`/`Enum` | *no arm* → `19595` | fail — B72's site |
| `Trait` | `Generic(c)` | `analyzer.rs:19393` — the arm is `(_, Generic)` | accept, **binds `c := Trait`** |

> **P2.** Two unrelated traits: `fun make_alpha(): Alpha { … }` assigned to
> `let x: Beta` gives `Expected Beta, but got Alpha instead.` The same trait
> on both sides compiles. The `if l_id == r_id` guard is exactly what it says.

The last row is the one that hurts. `Trait` unifies with a generic parameter
and *binds it* — so a bare-trait value passed to
`fun use_it<T: Display>(v: T)` binds `T := Display`, the trait itself, and the
monomorphizer is then asked to specialize for a type that has no impls. And it
is not a trait-specific arm that lets this through: the arm is written
`(_, Type::Generic(constraint_id))`, so a trait binds a generic for the same
reason any type does. Nothing about `Type::Trait` was considered here, which
is §0's thesis in one line of Rust.

Nothing stops it, because five bound-checking sites deliberately treat
`Type::Trait` as indeterminate and `continue` — `analyzer.rs:2376-2381`,
`2416-2428`, `2584-2589`, `2814-2819`, `3054-3059`. The comment at
`analyzer.rs:2581-2589` says why: *"a value typed AS a bare trait is the
trait-object error's business"*. It is a deliberate deferral to a diagnostic
that never fires. §2.1 is what happens next.

**Recommendation (§1.4):** the `(Trait, Generic)` binding arm is the leak, and
it is the one arm a tightening must close first. A trait may satisfy a bound;
it may not *be* the binding.

### 1.5 The load-bearing acceptance — five sites, not one

The B4 amendment records the carve-out as *"iterator.vl returns a bare
trait"*. The sweep found **five** bare-trait type-position occurrences in the
whole tree, across **three** std files — and every one of them is the same
thing wearing a different hat:

| site | text | shape |
|---|---|---|
| `vilan/std/src/iterator.vl:308` | `fun iter(self): Iterator<T>;` | trait declaration |
| `vilan/std/src/iterator.vl:312` | `fun iter(self): Iterator<T> { self }` | blanket impl body |
| `vilan/std/src/wire.vl:69` | `fun rebuild<D: Deserialize>(deserializer: D): Wire;` | **static** |
| `vilan/std/src/json.vl:148` | `fun from_json(text: str): Result<FromJson, str>;` | **static**, nested in `Result` |
| `vilan/std/src/json.vl:150` | `fun from_json_value(value: JsonValue): Result<FromJson, str>;` | **static**, nested |

**Zero** in parameter position, **zero** in field position, **zero** in `let`
position, anywhere in 225 `.vl` files. And all five are `Self` stand-ins
written inside a trait declaration, not dispatch requests. They exist because
the author wanted to write "this type" and reached for the trait's name.

`iterator.vl:308`/`:312` typecheck because of the `(Trait, Trait)` arm at
`analyzer.rs:19538`, added by the return-checking arc for exactly this program
(`ret-checking.md:63-66`) and pinned at `inference.rs:8136-8162`
(`a_trait_typed_self_returns_through_a_trait_typed_signature`).

**The carve-out is not needed, because `Self` already works.**

> **P3.** Two programs, `vilan check`, both clean:
>
> **(a)** the `Iterable` shape with `Self` — structurally the std program with
> names changed:
> ```
> trait AsWalk<T> { fun as_walk(self): Self; }
> impl Walk<type T> with AsWalk<T> { fun as_walk(self): Self { self } }
> ```
> **(b)** the `wire`/`json` shape — `Self` in a **static's** return:
> ```
> trait Rebuild { fun make(text: str): Self; }
> impl Bag with Rebuild { fun make(text: str): Bag { Bag { n = 1 } } }
> ```
> Both compile. For completeness, (c) today's bare-trait static return
> (`fun make(text: str): Rebuild;`) also compiles — which is why `wire.vl` and
> `json.vl` build.

So the migration is mechanical: five declarations gain the word `Self` and the
`(Trait, Trait)` arm stops being load-bearing for anything. This closes what
the survey correctly identified as *"the real underlying gap those five sites
expose — `Self` in return position, not trait objects."*

The honest caveat: `Iterable` is dead weight regardless.
`iterator-adapters.md:63-67` records that it *"has no implementor but the
blanket impl, and no `.iter()` call exists anywhere"* in `std/src`, `test`,
`examples`, or `crates/vilan-core/tests`, and the arc's ship record confirms
S6 left it untouched (`iterator-adapters.md:823-831`). It is load-bearing for
the *pins*, not for any program.

**And S6 does not ride B4 after all.** The iterator-adapters arc left
S6/`Iterable` as the residue pointing at this file, but its own design was
never a trait object. `iterator-adapters.md:250-258` calls the two-parameter
form *"the associated-type-free spelling"* and writes it out:

```
trait Iterable<T, I: Iterator<T>> {
    fun iter(self): I;
```

`I` is a **generic parameter**, not a bare trait — the return that today
reads `Iterator<T>` becomes an ordinary bound one. And S6's recorded blocker
was P4, a bound-propagation gap shipped as B58 on 2026-08-04, whose backlog
entry states outright: *"I3's P4/S6 caveats discharged"*
(`backlog-2026-07-18.md:266`).

So **declining B4 does not block S6, and building B4 would not unblock it** —
the two are independent, and the `Iterable` line riding on this file is a
pointer to §1.5's `Self` question, not to dynamic dispatch.

**Recommendation (§1.5):** rewrite all five to `Self`. Do it as its own slice,
before any tightening, because it is a readability improvement that stands on
its own merits and it removes the only obstacle the amendment named. Record
that S6 is unblocked by this and always was.

### 1.6 What the spec says, and the gap

> Traits are used as **bounds**; a trait is not a type: `let x: Display` is a
> compile error (no trait objects).

`vilan/docs/spec/types.md:110-112`. §5.11's normative rejection list opens
with it (`types.md:339`):

> - Using a trait as a type (`let x: Display = …`).

The tour agrees, and adds the prescribed alternative
(`vilan/docs/tour/data-and-traits.md:231-232`):

> `let x: Greet = …` is a compile error. When you want "one of several things
> at runtime", use an enum.

Three documents, one rule. The implementation refuses the *use* and accepts
the *declaration* in four of six positions (P1). The divergence is deliberate
and pinned (`inference.rs:41678-41691`), but it is still a divergence: the
spec describes a language the compiler does not implement.

**Recommendation (§1.6):** the cheapest thing in the paper to settle. Either
the compiler moves to the spec (§12's slice) or the spec moves to the
compiler. It should be the compiler, because the spec's rule is the one that
makes §2.1 and §2.2 impossible.

### 1.7 What a value *is* at runtime

> **P4.** `struct Remote { url: str }`; `Remote { url = "u" }` compiles to the
> JavaScript `[ "u" ]`. `struct Bag { n: i32 }`; `Bag { n = 1 }` compiles to
> `[ 1 ]`.

A struct is a **bare array with no tag**. A C-like enum is a bare number
(`backed-enums.md` P1). Nothing in the emitted value records what it is.

> **P5.** Method calls do not lower to JavaScript methods.
> `impl Bag with Show { fun show(self): str { … } }` emits
> `function show(self) { … }`, and `b.show()` emits `show(b)` — a free
> function with the receiver as argument 0. No prototypes, no `this`, no
> method tables (`transformer.rs:5453-5470`; `analyzer.rs:21741-21748`:
> *"`self` is parameter 0, so arguments align at offset 1"*).

These set the representation question's floor. **There is nowhere to hang a
vtable on an existing value** — no header, no prototype, no tag. A trait
object cannot be a pointer-with-extra-bits; it has to be a *new* pair. §6
takes that up.

### 1.8 Monomorphization, confirmed

> **P6.** `fun render<T: Show>(v: T)` called at `A`, `A`, `B`, `C` emits three
> functions — `$a`, `$b`, `$c` — with `$a` used twice. Each body contains a
> **direct call** to its concrete impl (`show(v)`, `show2(v)`, `show3(v)`).
> No dispatch table, no dictionary argument.

`emit_instance_with_bits` (`transformer.rs:6024-6060`) memoizes on
`(function_id, bound type keys, asyncness bits)`, so instantiation is
deduplicated per binding vector, exactly as `spec/types.md:163-166` says.
Instance names come from `self.ng.next_name()` (`transformer.rs:6050`) — an
opaque generated identifier — so instantiation identity lives in a compiler
memo, not in the emitted name. **There is no mangling scheme a vtable could
reuse.**

## 2. The four holes this file already owns

### 2.1 The internal error — three routes, not one

The B4 amendment (`backlog-2026-07-18.md:144`) records one route:
`let x: A = bag; use_it(x)`. There are three.

> **P7.** Three programs, each `Display`/`Bag` plus
> `fun use_it<T: Display>(v: T): str { v.show() }`:
>
> - `let x: Display = Bag { n = 1 }; use_it(x)` — the recorded route
> - `struct Holder { item: Display }; use_it(h.item)` — the **field** route
> - `fun make(): Display { … }; use_it(make())` — the **return** route
>
> All three produce, byte for byte:
>
> ```
> Error: internal: a call resolved to `Display`'s requirement `show`, which
> has no body — emitting it would produce an empty function and a runtime
> `TypeError`. The receiver's type could not be resolved to a concrete
> implementation at this call; please report this program
> ```

The guard is B55's, at `transformer.rs:1834-1860`, fed by the collection point
in `function_with_name` (`transformer.rs:5118-5126`). It is a hard `Err`
returned from assembly, not a pushed diagnostic — it aborts the build.

Two things follow that the amendment does not say:

1. **The route count is three**, matching P1's accepting positions minus the
   method-parameter case (refused earlier by `BareTraitValue` when the body
   uses the parameter). Every position that accepts a bare trait is a route to
   the internal error the moment the value meets a bounded generic. A fix that
   only handles `let` fixes a third of it.
2. **The span points at the trait declaration**, not the user's call —
   `span: function.map(|function| function.name_span)`. The caret lands on
   `fun show(self): str;` inside the trait, which for a std trait is inside
   std. The message says *"at this call"* and then points somewhere else.

**Recommendation (§2.1):** the diagnostic B4 owes (§12) must be emitted in the
*analyzer*, at the point the bare-trait value meets the bound — one of the
five `continue` sites in §1.4 — not in the transformer. By the time B55's
guard fires the call site is gone.

### 2.2 The destructor-suppressing cast — a live resource leak

This one is new, it is in no record, and it is the sharpest thing the survey
found.

`Type::Trait` is classified by the resource analysis at `analyzer.rs:4373`:

```rust
// Everything else is a non-value or a scalar: never a resource by
// containment.
Type::Any
| Type::Never
| Type::Void
| Type::Unknown
| Type::Unresolved
| Type::Closure(_, _)
| Type::Function(_)
| Type::Module(_)
| Type::Trait(_, _)
| Type::Mapped(_, _, _) => (false, true),
```

The tuple's second element is *completeness*: `true` means "this verdict is
definitive, do not look further". So a bare-trait-typed field is
authoritatively declared to carry no resource. For meaning-(1) `Self` that is
correct — `Self` is a placeholder, not a value. For meaning-(2) it is a lie,
and the affine machinery believes it. (The same classification appears again
at `analyzer.rs:4692` for transferability.)

> **P8.** `resource struct Handle { id: i32 }` with
> `impl Handle with Drop { fun drop(&mut self): void { print("closing"); } }`,
> and a trait `Named` that `Handle` implements. Four programs, `vilan run`:
>
> | program | output |
> |---|---|
> | `let n: Handle = Handle { id = 1 };` | `ok` / **`closing`** |
> | `let n: Named  = Handle { id = 1 };` | `ok` — **no `closing`** |
> | `struct Box { item: Handle }; let b = Box { item = Handle { id = 1 } };` | `ok` / **`closing`** |
> | `struct Box { item: Named  }; let b = Box { item = Handle { id = 1 } };` | `ok` — **no `closing`** |
>
> The destructor runs in both concrete cases and neither trait case. Changing
> one word of a type annotation silently deletes a destructor call.

The affine *checker* goes with it:

> **P9.** With `struct Box { item: Handle }`, `let c = b; let d = b;` is
>
> ```
> Error: use of `b` after it was moved: a resource has a single owner
> ```
>
> With `struct Box { item: Named }`, the identical program **compiles, runs,
> and prints `ok`**, emitting `const b = [ [ 1 ] ];` — one resource, two live
> owners, no drop.

Read against the spec, a bare trait annotation does precisely what **R12**
forbids (`spec/memory.md:449-451`):

> **R12: no coercion to `any`.** A resource passed where `any` is expected is
> an error (`print(db)` included): `any` is a data sink, and the discipline
> must not launder away.

R12 names one sink. There is a second, unguarded one, and unlike `any` it does
not even produce a diagnostic. **Containment inference
(`spec/memory.md:341-346`) cannot see through a trait type**, so an aggregate
holding a resource behind a bare-trait field is classified as data — R1 does
not fire, R2 does not fire, R10 does not fire, and scope-end destruction does
not happen.

This is not a trait-objects question. It is a hole in the affine discipline
that exists **today**, in shipped code, with no trait objects anywhere. It is
also the strongest argument in the paper for the §12 tightening: making a
trait illegal in value position closes it by construction, where any narrower
patch has to enumerate positions.

**Recommendation (§2.2):** file as its own backlog item with a red pin, and
treat it as **higher priority than B4 itself**. If the owner defers the
trait-object decision indefinitely, this still has to be fixed. If the §12
tightening ships, it is fixed as a side effect — but the pin should exist
first, so the fix is proven rather than assumed.

### 2.3 The heterogeneous container already compiles

> **P10.** Three programs over `trait Alpha` with implementors `Bag` and
> `Cup`:
>
> - `let xs: List<Alpha> = [ Bag { n = 1 }, Cup { m = "x" } ];`
>   → `Expected Bag (this literal's element type), but got Cup instead.`
>   The annotation is accepted, and the diagnostic names **`Bag`** — not
>   `Alpha`, and not "a trait is not a type".
> - `mut xs: List<Alpha> = []; xs.push(Bag { n = 1 }); xs.push(Cup { m = "x" });`
>   → **compiles and runs**, emitting `xs.push([ 1 ]); xs.push([ "x" ]);`
> - `… ; for item in xs { item.a() }` → `BareTraitValue`.

The *storage* half of a trait object exists today, unsoundly: a genuinely
heterogeneous list, built and run, holding two structurally different values
under one element type. Only dispatch is refused — and it is refused because
P4 already told us there is no tag to dispatch on.

The first case is worth reading carefully, because it is **not** the
list-literal checker ignoring the annotation. That checker was fixed
deliberately and is pinned:
`a_mixed_literal_under_a_list_of_any_parameter_is_legitimate`
(`inference.rs:24656-24673`) states the contract — *"the check consults the
`List<T>` expectation before reporting"* — and
`a_heterogeneous_list_literal_is_rejected` (`inference.rs:24629-24643`)
records that typing a literal by its first element was a soundness bug that
got closed.

The expectation *is* consulted here. It is consulted against
`reconcile_type`'s `(Struct|Enum, Trait)` arm (`analyzer.rs:19430-19455`),
which returns **`a.clone()` — the concrete side**. So consulting `Alpha`
against `Bag` yields `Bag`, the element type narrows to the first element's
concrete type by a route that looks nothing like "first element wins", and
`Cup` then fails against it.

**This is §1.3's root cause reappearing in a fourth place.** The arm that
makes `let x: Display = bag` compile is the arm that makes `List<Alpha>`
silently mean `List<Bag>`. One fix closes both.

**Recommendation (§2.3):** the second entry point the §12 tightening closes
for free. `List<Alpha>` should be rejected at the annotation, same rule, same
message — and the fix is at the `(Struct|Enum, Trait)` arm, not in the list
checker, which is behaving exactly as designed.

### 2.4 Two impls of one trait for one type — bycatch

> **P11.** `impl Bag with Show { fun show(self): str { "first" } }` followed
> by `impl Bag with Show { fun show(self): str { "second" } }` — **no error**.
> `b.show()` prints `first`; through a bounded generic, also `first`. The
> second impl is emitted nowhere and is silently dead.

B57 shipped a duplicate-inherent hard error and a trait-vs-trait ambiguity
error (`backlog-2026-07-18.md:264`); B74 closed duplicate statics (`:311`).
The **same trait implemented twice for the same type** falls between them and
resolves by declaration order.

This matters to B4 specifically: a vtable is a table per `(type, trait)` pair,
and if two impls of one trait exist for one type there is no single table to
build. Coherence is a *prerequisite* for vtables, not a consequence. It is
also a plain bug without them.

**Recommendation (§2.4):** file as its own backlog item, independent of B4's
outcome. The fix is B57's existing duplicate machinery reaching one more
candidate set.

## 3. The demand survey

The backlog says *"nothing demands it yet"*. This section is the check, not
the restatement — and the check is more interesting than the claim.

### 3.1 What the sweep found

225 `.vl` files, every declared trait hunted for in every type position, plus
every heterogeneous-collection and manual-vtable shape, plus every recorded
tester ask.

**Nobody writes a trait in a value position.** §1.5's table is the complete
list: five sites, three std files, all `Self` stand-ins. Zero in parameter,
field, or binding position anywhere. Zero trait *declarations* outside std and
the test corpus. The entire application surface — todo, walkthrough, ssr,
router, canvas, fullstack, reactive-ui, benchmarks, CLI templates — contains
**two** trait impls in total, both `impl Route with Routable`.

**But three places in std are shaped by the absence, and say so.** These are
real, and a survey that omitted them would be dishonest:

**(a) The RPC capability table.** `vilan/std/src/rpc.vl:1063`:

```
sources: Shared<List<(i32, || Subscription)>>,
```

with the reason stated twice, at the module head (`rpc.vl:12-14`) and again at
the method (`rpc.vl:1082-1086`):

> the capability table holds **heterogeneous sources**, and vilan has no trait
> objects

`expose<T: Wire, S: Source<T>>` (`rpc.vl:1087`) erases its two type parameters
into a starter closure and pushes that. `List<dyn Source<_>>` was the natural
type.

**(b) `Owner`, std's central lifecycle primitive.**
`vilan/std/src/reactive.vl:262`:

```
cleanups: Shared<List<|| void>>,
```

`take<T: Disposable>(self, item: T): T` (`reactive.vl:272-277`) pushes
`|| { item.dispose(); }` — one closure minted per item. The collection is
genuinely heterogeneous: the doc comment at `reactive.vl:270-271` names
`Subscription`, `View`, and a child `Owner`. The generic bound expresses the
*insertion*; it cannot express the *collection*. `List<dyn Disposable>` is the
textbook shape.

**(c) `Serializer`/`Deserializer`/`Codec` — a hand-written vtable.**
`vilan/std/src/wire.vl:78-100` is a struct with 15 closure fields — one per
member of the `Serialize` trait it then implements; `wire.vl:112-131` is one
with 18, mirroring `Deserialize`; `wire.vl:395-398` is `Codec`, holding two
more. Each `impl … with Serialize` member delegates to its own same-named
field (`wire.vl:137-140`). The code names it (`wire.vl:8-14`):

> The closure RECORDS below remain as the **codec-as-a-VALUE erasure** only
> (`Codec` must hand back one concrete type)

Alongside, `rpc.vl:780` and `:881` take `args: List<|Serializer| void>` — a
heterogeneous argument list erased element by element, each element one
argument of a different `Wire` type.

Smaller instances of the same shape run through the handler/registry surface:
RPC routes are closures keyed by string with a linear scan (`rpc.vl:943-971`),
HTTP handlers are closure fields on a builder
(`process/http.vl:251-265`), rpc-server lifecycle hooks likewise
(`process/rpc_server.vl:87-88`, `:125-128`). Transports, by contrast, are
**generics** — `call<T: Wire, Tx: Transport>` (`rpc.vl:877`),
`bridge<Tx: DuplexTransport>` (`rpc.vl:140`) — and so are the UI slots
(`browser/ui.vl:159`, `:190`) and the router (`browser/router.vl:90`).

**Enums-as-dispatch: none.** No enum in the tree wraps several structs merely
to obtain a common type. Every closed sum found — routes, frames, `Child` —
is a closed sum on its merits, which is the tour's prescribed idiom
(`docs/tour/data-and-traits.md:231-232`), not a workaround.

### 3.2 What the tester asked for

Nothing. Recorded tester reports exist and were enumerated —
`backlog.md:3189`, `:3219`, `:4042`; `mut-parameters.md:3`; `canvas.md:7`;
`backlog-2026-07-18.md:117`, `:675`, `:685`; `std-surface.md:21`. Their
subjects are `mut` parameters and closure parameters, HTML canvas bindings,
and std surface thinness. **None concerns dynamic dispatch, trait objects,
heterogeneous lists, or plugin registries.** The agent-memory stores outside
the worktree contain one hit for `dyn`, and it is a Rust `&dyn Fn` in an
unrelated LSP signature.

### 3.3 The measurement that settles it

The codebase has already run this experiment, in the opposite direction, and
recorded the number.

`p6-followups.md:60-75` — the `Wire` visitor moved **off** closure records
**onto** monomorphized traits:

> measured +18% json / +14% binary on the 25-todo round-trip (51.3k/29.9k per
> sec vs the 42.5k/25.6k baseline), with the codec path unregressed

The closure records survive *only* as the codec-as-a-value erasure — the
`Codec` path — precisely because that one site needs to hand back a single
concrete type. So the erasure that trait objects would generalize is the slow
path this project deliberately walked away from, keeping it in exactly one
place and documenting why.

That is the load-bearing fact. §3.1(a)–(c) are not blocked by the absence of
trait objects; they are *served* by an alternative the project measured as
faster on the paths that matter.

### 3.4 The two answers the language already has

> **P12.** Two programs, both `vilan run`, both printing `bag` then `cup`.
>
> **(a) A closed set — an enum.** Exhaustive, checked, zero runtime cost:
> ```
> enum Shape { AsBag(Bag), AsCup(Cup) }
> fun show(s: Shape): str {
> 	match s {
> 		Shape::AsBag(let b) => "bag",
> 		Shape::AsCup(let c) => "cup",
> 	}
> }
> ```
> **(b) An open set — a struct of closures.**
> ```
> struct Shown { show: || str }
> let xs: List<Shown> = [ Shown { show = || "bag" }, Shown { show = || "cup" } ];
> for item in xs { let f = item.show; print(f()); }
> ```

**(b) is a trait object.** The closure's captured environment *is* the value;
the closure itself *is* the vtable slot; `struct Shown` *is* the pair. It is
hand-rolled and unchecked against a trait, but the representation is identical
to what §6 would build, and it needs no language change. §3.1(a)–(c) are this
pattern, written out by hand at std scale.

That reframes the feature honestly: **B4 is not "make dynamic dispatch
possible" — it is "let the compiler build the table the user is already
building".** A real ergonomic win, and a much smaller claim than the backlog's.

### 3.5 The verdict

**Demand exists and is bounded: three sites, all inside std's runtime
plumbing, all already served, none blocked, and each paying a cost the project
measured and accepted.** Zero demand from application code. Zero from any
recorded tester ask. And the one head-to-head measurement in the record went
against erasure by 14–18%.

**Recommendation (§3): decline.** The steer B72 ships routes the common
mistake to the generic that works; the enum covers closed sets; the closure
record covers open ones and is what std uses. Revisit if a driver application
produces a registry that (a) is genuinely open, (b) needs a trait's *checked*
surface rather than a closure field, and (c) cannot be a generic because the
set is assembled at runtime — all three, not any one. `Transport` and `Source`
(§4) are where that would first appear.

## 4. Would it even work? Object safety, measured

Suppose the answer were yes. Which of std's traits could be objects?

A vtable slot needs a receiver to dispatch through and a signature that does
not mention the erased type. Three disqualifiers follow from §1.7–§1.8,
and §5 adds a fourth:

- **No receiver** (a static): nothing to select an impl with. This is B83's
  finding restated — `method-resolution.md:754-756` records that
  `Trait::static()` *"cannot be built on today's design: the qualified form
  selects an impl THROUGH the receiver's type, and a static offers nothing to
  select with."* A vtable has the same problem for the same reason.
- **`Self` in the return** (`fun add(self, b: B): Self`): the caller would
  have to know the erased type to receive the result.
- **A generic method** (`fun map<U>(self, fn: |T| U)`): one slot cannot hold
  an unbounded family of specializations.
- **Declared asyncness disagreeing with an impl's** — §5.

> **P13.** A census over `vilan/std/src/**/*.vl`: **39 declared traits, 96
> members**, each member classified against the first three disqualifiers.
>
> **22 object-safe, 17 not.**
>
> | trait | file:line | members / disqualifying | disqualified by |
> |---|---|---|---|
> | `Iterator` | `iterator.vl:7` | 15 / 8 | `Self` return on `filter`/`take`/`skip`/`enumerate`; generic **and** `Self` on `map`/`zip`/`chain`; generic `fold` |
> | `Ord` | `compare.vl:39` | 4 / 3 | `Self` return on `min`, `max`, `clamp` |
> | `Wire` | `wire.vl:67` | 2 / 2 | generic `describe`; static generic `rebuild` |
> | `FromJson` | `json.vl:146` | 2 / 2 | both members static |
> | `Try` | `operators.vl:17` | 2 / 1 | static `from_bad` + `Self` return |
> | `Default` | `default.vl:1` | 1 / 1 | static + `Self` return |
> | `Random` | `random.vl:11` | 1 / 1 | static + `Self` return |
> | `Add` `Sub` `Mul` `Div` `Rem` `Shl` `Shr` `BitAnd` `BitXor` `BitOr` | `operators.vl:31-86` | 1 / 1 each | `Self` return, all ten |
>
> The 22 that would work: `PartialEq`, `Eq`, `PartialOrd`, `Debug`,
> `Display`, `Drop`, `Hashable`, `Into`, `Iterable`, `Json`, `Lift`,
> `Disposable`, `Source`, `Transport`, `DuplexTransport`, `Serialize`,
> `Deserialize`, `Routable`, and `Slot`/`AttrValue` in both UI twins.

Two readings, both worth having.

**Against:** the ten operator traits and `Iterator` — the traits with the most
use — are all out. `Iterator` is the canonical trait-object motivation in
every language that has them, and vilan's is disqualified by 8 of its 15
members. `Wire` is out, which means §3.1(c)'s hand-written codec vtable
**could not be replaced by a real one**: its trait is not object-safe, so the
largest manual vtable in the tree would stay hand-written.

**For:** `Transport`/`DuplexTransport` (`rpc.vl:42`, `:101`), `Source`
(`reactive.vl:349`), `Disposable` (`reactive.vl:10`), and `Serialize`/
`Deserialize` are all object-safe. Those are exactly §3.1(a) and §3.1(b) — so
two of the three demand sites would be served. The feature would land where
the demand is, for two sites out of three.

**Recommendation (§4):** if trait objects are ever built, **object safety is a
per-trait property the compiler computes, not an opt-in keyword**, and a
non-object-safe trait used in object position gets a diagnostic naming the
**disqualifying member** and its reason — not the trait. Naming the trait
sends the user to read fifteen signatures; naming `Iterator::map` and why
sends them to the line.

## 5. The asyncness-agreement check (B29)

B4's mandate from the backlog (`backlog-2026-07-18.md:146-151`): B29 permits
an async impl of a sync trait declaration because dispatch is monomorphized; a
vtable call knows only the declaration, so B4 must design the check.

### 5.1 What the freedom looks like today

> **P14.** `trait Fetch { fun get(self): str; }` — declared **sync** — with
> `impl Remote with Fetch { async fun get(self): str { "remote" } }` and
> `impl Local with Fetch { fun get(self): str { "local" } }`, plus
> `fun consume<T: Fetch>(v: T) { print(v.get()); }` called at both types.
> Emitted JavaScript, verbatim:
>
> ```js
> async function get(self)  { return "remote"; }
> function       get2(self) { return "local";  }
> async function $a(v) { console.log(await (get(v))); }
> function       $b(v) { console.log(get2(v));        }
> (async () => { await ($a([ "u" ])); await ($b([ "p" ])); })();
> ```
>
> **One vilan function, two JavaScript functions of different asyncness.**
> `$a` is `async` and awaits; `$b` is not and does not. The choice was made by
> which impl `T` bound to.
>
> The freedom runs both ways: an `async fun get` *declaration* with a sync
> impl also compiles, and a concrete `r.get()` on an async impl of a sync
> declaration emits `await (get(r))` and runs.

This is B29 working as designed. The instantiation key carries asyncness bits
alongside the type bindings (`transformer.rs:6024-6060`);
`async-polymorphism.md:77-79` names the mechanism: *"The instantiation key
gains a per-closure-parameter asyncness bit … asyncness is a second effect
per-instantiation."*

### 5.2 Why a vtable breaks it

A vtable call emits **one** body. It cannot be both `$a` and `$b`. At the call
site the compiler must choose statically whether to write `await`, and the
only thing it knows is the *declaration* — which today is binding on nothing.

The question is therefore not "how do we check asyncness" but "what does the
declaration mean once it is all we have". Three answers:

**(i) The declaration becomes binding, for object-safe traits only.** An impl
whose asyncness differs from the declaration disqualifies the trait from
object use — §4's fourth disqualifier — reported at that impl. *Cost:*
object-safety now depends on impls possibly in other modules, so the
disqualification is discovered late and reported far from the coercion.
*Benefit:* zero cost at the call, no representation change, and the
monomorphized path keeps B29's freedom entirely.

**(ii) The declaration becomes binding everywhere.** Retract B29's permission.
*Cost:* a behavior break in a shipped, deliberate feature, for a feature
nobody has asked for. Disqualifying.

**(iii) Adapt through the vtable — always `await`.** Every slot is declared
async; a sync impl is wrapped to return a settled promise; every vtable call
awaits. *Cost:* every dynamic call colors its caller async — the exact cascade
`iterator-adapters.md:832-854` records as a real defect when `Iterator::all`
collided with `Promise::all` and took an entire synchronous program async, to
the point where the const-eval interpreter refused it outright.

A fourth shape is worth noting because the language already uses it:
**asyncness rides the type.** `async-polymorphism.md:25-27` records that for
closures, *"asyncness rides the type — `async |T| U`"*. A trait-object type
could do the same — `dyn Fetch` versus `async dyn Fetch` — making the
agreement check a plain type check at the coercion site, at the price of one
more spelling.

**Recommendation (§5):** **(i)**, extended by the type-carried form if the
owner wants the async case at all. The declaration becomes binding exactly
where it must be — in object position — and stays advisory everywhere else, so
B29's shipped freedom is untouched on the path that has users. The diagnostic
names the impl whose asyncness disagrees, at that impl, with a note at the
trait declaration; B72's cross-module note (`analyzer.rs:20480-20488`) is the
precedent and already renders correctly.

## 6. Representation, if it were built

### 6.1 The pair

`(value, vtable)`, as the backlog says, and P4 forces it: a struct is a bare
untagged array, so there is nothing to widen in place and no header to write
into. The concrete form on the JS backend:

```js
[ value, vtable ]
```

a two-element array, structurally indistinguishable from a two-field struct —
a feature, since every existing lowering (copying, `__clone`, equality) then
works on it unmodified.

**Not a fat pointer.** A fat pointer is a native-backend concept and there are
no pointers here. The forward-compatibility question for the native arc
(`memory.md:330-334`'s Tier 2) is whether `(value, vtable)` survives — it
does, as a two-word pair, which is what a fat pointer is.

### 6.2 The vtable

A vtable is a JS object literal mapping member name to the emitted free
function for that `(type, trait)` pair. Because methods are already free
functions taking the receiver as argument 0 (P5), **no adapter shims are
needed**:

```js
const $vt_Bag_Show = { show: show2 };   // show2 is Bag's emitted `show`
```

and `x.show()` on a trait object becomes `x[1].show(x[0])`.

**Emission and deduplication.** One vtable per `(type, trait)` pair that is
actually coerced — reachability-driven, like `bundle-splitting.md`'s
whole-program reachability, not per-instantiation. The memo key has the same
shape `emit_instance_with_bits` already uses (`transformer.rs:6024-6060`); the
natural home is a second map beside `self.instances`. Deduplication is
therefore free: two coercions of `Bag` to `Show` share one table because the
key is identical.

The real cost is a **new root set for monomorphization**. Today an impl method
is emitted only if a call reaches it; a vtable makes *every* member of a
coerced pair reachable whether called or not. For `Serialize` (15 members) and
`Deserialize` (18) that is 33 functions per coerced type the bundle splitter
can no longer prove dead.

**Recommendation (§6):** `(value, vtable)` as a two-element array, one table
per coerced `(type, trait)` pair, keyed and deduplicated in the transformer
beside `instances`. Record the reachability cost as a known dead-code-
elimination regression and measure it before shipping, per the measure-first
house habit.

## 7. Positions and coercion

### 7.1 Which positions accept a trait type

If the type exists it is a real type, and every position accepts it: binding,
parameter, return, field, generic argument, tuple element. Restricting it
positionally would recreate today's confusion, where four positions accept and
two do not for reasons no user can predict (P1).

The one non-arbitrary restriction: **a trait object may not be the subject of
an `impl`.** `impl dyn Show { … }` has no meaning; the subject is not a type
with an identity.

### 7.2 Explicit or implicit

Implicit coercion — `let x: Show = bag;` allocating a pair silently — is the
shape today's syntax already has, and it is the wrong one, for a reason that
has nothing to do with allocation.

> **P15.** A `Bag` with **both** an inherent `show` and a trait `show`:
>
> ```
> impl Bag { fun show(self): str { "inherent" } }
> impl Bag with Show { fun show(self): str { "trait" } }
> ```
>
> `b.show()` prints **`inherent`** — B57's rule, inherent outranks trait. The
> same value through `fun via_generic<T: Show>(v: T)` prints **`trait`**.
> Emitted: `console.log(show(b))` beside `console.log($a(b))`, where `$a`
> calls `show2`.

A vtable is a per-`(type, trait)` table, so a vtable call is *always* the
trait tier. Coercing `b` into a `Show` object would therefore **change which
method runs**, from `inherent` to `trait`. If the coercion is implicit, adding
a type annotation to a binding changes program behavior with no syntactic
trace — the same class of hazard as §2.2's destructor suppression.

**Recommendation (§7):** coercion is **explicit**, at the coercion site, with
its own spelling (`dyn Show`, or whatever the owner prefers — the paper takes
no position on the token beyond wanting one). A bare `Show` in type position
stays what the spec says it is: an error. This also makes §11's migration
mechanical: today's four accepting positions become errors, and there is a new
thing to write instead.

## 8. Resources and drop through a vtable

§2.2 established that today a bare trait annotation destroys the affine
discipline. A designed trait object must do better, and the R-rules say how
much better is achievable.

### 8.1 Can a trait object hold a resource?

The rules that bear:

- **R10** (`memory.md:424-433`): no resource elements in native containers,
  read **per instantiation**, so `Signal<Database>` is refused exactly as
  `Shared<Database>` is. A trait object is not a native container — it is a
  pair the compiler emits — so R10 does not automatically refuse it. But
  R10's *reason* applies: the objection is that *"their internals are host
  code the move checker cannot see"*, and a vtable's internals are host code
  the move checker cannot see either.
- **R11** (`memory.md:434-448`): generics must be move-clean per
  instantiation, re-checked with `T :=` the resource type. **A trait object
  has no per-instantiation re-check available** — that is the point of
  erasure. R11's mechanism is structurally unavailable here.

  *Drift found, recorded not fixed.* R11's parenthetical justifies its
  `own T` tightening with *"a generic body is emitted once and so cannot run
  an instantiation-conditional destructor"*. That premise is not what the
  compiler does.

  > **P17.** `fun pass<T>(v: T): T { v }` called at two struct types emits
  > **two** bodies, `$a` and `$b` — an unbounded generic monomorphizes exactly
  > like a bounded one. `spec/types.md:163-166` says so normatively
  > (*"each distinct binding vector … produces its own specialization"*), and
  > P6 shows the same for the bounded case.
  >
  > So a monomorphized body *could* carry an instantiation-conditional
  > destructor; it does not, by choice.

  The **rule** is likely still right — it keeps end-of-scope ownership static,
  which is R7's whole design — but its stated **reason** contradicts
  `spec/types.md` §5.6 and the compiler. Worth a one-line correction in
  `memory.md` whoever next touches that paragraph; it is not B4's to fix, and
  B4's argument above does not rest on it.
- **R7** (`memory.md:401-412`): no conditional moves, because *"there are no
  runtime drop flags in v1"*. A trait object's drop is runtime-dispatched by
  definition.
- **Drop timing** (`memory.md:482-496`): *"A value's own `drop` body runs
  before its fields, and the fields drop in reverse field order."* Through a
  vtable the compiler does not know the fields.

### 8.2 The drop-glue design, if resources were allowed

It is buildable, and it is not small. The vtable gains a synthesized `$drop`
slot holding **drop glue** for that concrete type: a generated function
running the type's `Drop::drop` body if it has one and then destroying its
fields in reverse order — the work `memory.md:482-496` describes, lifted out
of the compiler's static knowledge into an emitted function. Scope-end
teardown of a trait-object binding calls `x[1].$drop(x[0])`.

Three constraints follow, none optional:

1. **Every coerced type must be drop-glue-complete at the coercion site**,
   which is where the concrete type is still known. That is fine.
2. **`drop` is synchronous and context-free** (`memory.md:468-474`) — already
   true, and the glue inherits it. Good: no interaction with §5.
3. **R7 keeps forbidding conditional moves.** The glue makes the *destructor*
   dynamic, not the *ownership*. Whether a binding still owns at scope end
   stays static; the trait object is one value with one owner, and only what
   its teardown *does* is dispatched.

### 8.3 The recommendation

Even with the glue designed, allowing resources buys a capability nothing
asked for and loses the property R10 was written to protect — that the move
checker can see the whole story. It also multiplies §4's object-safety
question by the resource question, so a trait's object usability would depend
on which types implement it.

**Recommendation (§8):** if trait objects are built, **v1 refuses resources**
— a resource type coerced into a trait object is an error, R10 extended by one
sink, the message naming R10's own alternative (*"holding the resource in a
struct field of your own is the sanctioned alternative"*). The glue is
designed here so the refusal is a **choice with a known price**, not a gap;
§13's Q5 carries the reopening trigger.

**And independently of all of it: §2.2's leak is refused the same way, today,
whether or not any of this is built.** That is the point of §12.

## 9. B57's precedence and B83's statics under a vtable

### 9.1 B57

P15 is the finding: **which method runs depends on which tier the call
resolves through**, and a vtable call is always the trait tier. B57's rule
(`method-resolution.md` §3, shipped 2026-08-06) is "inherent over trait,
unconditionally" — but that rule is stated over a *concrete* receiver, and a
trait object has no concrete receiver to apply it to.

Two coherent positions, one incoherent:

- **The vtable is the trait's surface.** A `dyn Show` dispatches `Show`'s
  member, always. Simple and honest, and it makes §7's explicitness mandatory
  because the behavior change must be visible.
- **The vtable is built from the resolved member.** At coercion the compiler
  runs B57's ranking for that concrete type and puts *the winner* in the slot
  — so `Bag`'s inherent `show` goes into the `Show` vtable. Behavior is
  preserved across coercion.
- *(Incoherent: decide at the call. There is nothing at the call to decide
  with.)*

The second is better than it first looks. It preserves the invariant "the
method that runs on a value does not depend on the type the value is currently
viewed through" — the invariant P15 shows is otherwise broken. Its cost is
that the vtable is no longer "the trait's table" but "this type's answers to
the trait's questions", which is arguably what a vtable always was.

**Recommendation (§9.1):** **build the vtable from B57's ranking**, not from
the trait's declarations. It costs nothing — the ranking already exists as
`rank_member_candidates` (`method-resolution.md:733-734`) — and it makes
coercion behavior-preserving, removing the sharpest hazard in §7.

### 9.2 B83

> **P16.** `trait Spawn { fun make(): i32; }` with
> `impl Bag with Spawn { fun make(): i32 { 7 } }` — `Bag::make()` runs and
> prints `7`. A trait-provided static is reachable through the concrete type,
> which is B83's shipped behavior (`method-resolution.md:742-747`: *"The trait
> tier stays reachable here, unlike §3.1 … A static has no alternative
> spelling, so refusing the trait tier would make every trait-provided static
> uncallable"*).

Statics cannot enter a vtable at all — no receiver, nothing to select through
(§4). This is not a limitation to work around; it is B83's own finding from
the other direction, and it means:

**A trait with any static member is not object-safe**, and B83's open designer
residue — *"should a static be trait-reachable at all"*
(`method-resolution.md:758-760`) — is *upstream* of B4, not downstream. If the
owner ever rules that statics are not trait-reachable, four of §4's seventeen
disqualifications (`Default`, `FromJson`, `Try`, `Random`) change character.

**Recommendation (§9.2):** B4 takes no position on B83's residue and does not
wait for it. Statics are excluded from vtables under either ruling; only the
*count* of object-safe traits moves.

## 10. B73 — is it the same design question?

Asked to be settled honestly rather than bundled. **It is not the same
question, and this paper does not fold it in.**

> **P18.** `struct Celsius` with `impl Celsius with Into<Fahrenheit>`, and
> `let f: Fahrenheit = c.into();` gives
> `Expected Fahrenheit, but got Celsius instead.` The blanket
> `impl type T with Into<T>` (`vilan/std/src/into.vl:5-9`) won; the user's own
> impl is unreachable. B73 is live, exactly as filed.

The distinction:

- **B73 is a static question with a static answer.** Two impls match one
  *known* concrete type; the compiler must pick one; the pick happens at
  compile time and could be decided by a specificity rule tomorrow with no
  runtime representation change at all.
- **B4 is a question with no static answer by construction.** No concrete type
  is known; the pick is a runtime lookup; the whole feature *is* a
  representation.

Where they genuinely touch is narrower and worth recording: **a vtable is a
table per `(type, trait)` pair, so it presupposes that the pair names exactly
one impl.** B73 (a blanket and a specific impl both matching) and §2.4 (two
identical impls) are both cases where it does not.

**Recommendation (§10):** B73 is a **prerequisite** for coherent vtables, not
a component of B4's design, and should be settled on its own merits — it is a
live bug today with no trait objects anywhere. If B4 is ever built, B73 and
§2.4 are entry criteria. If B4 is declined, B73 is untouched by that decision.

## 11. Migration — what today's acceptances become

Under the §12 tightening (a trait is not a type, per the spec), each of P1's
six positions has a defined destination:

| today | today's behavior | becomes | the user writes |
|---|---|---|---|
| `let x: Display = bag;` | compiles | **error** at the annotation | `let x = bag;` (or `dyn Display` if §7 ships) |
| `x.show()` | `BareTraitValue` | unreachable — the binding errored first | — |
| `fun f(v: Display)` | B72 steer at the call | **error** at the declaration | `fun f<T: Display>(v: T)` |
| `fun make(): Display` | compiles | **error** at the return type | the concrete type, or `Self` |
| `struct H { item: Display }` | compiles | **error** at the field | the concrete type, or a generic field |
| `impl S { fun draw(self, v: Display) }` | compiles | **error** at the parameter | `fun draw<T: Display>(self, v: T)` |
| `let xs: List<Display> = []` | compiles (P10) | **error** at the argument | `List<T>` in a generic, or an enum (§3.4a) |

Three notes on cost:

1. **The blast radius inside the repo is five declarations in three std
   files** — §1.5's table — and every one becomes `Self`, which P3 proves
   compiles in both the method and the static shapes. No call site anywhere
   changes, because none of the five is called through its bare-trait type.
2. **B72's steer moves from the call site to the declaration site**, which is
   strictly better: today the error appears where the value is passed with a
   note pointing at the declaration; tomorrow it appears at the declaration,
   where the fix goes. The steer text survives nearly verbatim.
3. **Four `b72_*` pins invert** — the four asserting acceptance
   (`inference.rs:41678-41739`). They were filed as descriptions of a known
   inconsistency, so inverting them is the arc closing, not a regression. The
   two asserting refusal are unchanged, as is
   `a_trait_typed_self_returns_through_a_trait_typed_signature`
   (`inference.rs:8136-8162`) once rewritten to `Self`.

**Recommendation (§11):** migrate in one slice, not six. The rule is one rule
("a trait is not a type"), the pins already enumerate the positions, and a
partial tightening leaves exactly the confusion §1.6 is trying to remove.

## 12. The diagnostic owed either way

This part should ship regardless of how the owner rules on §3, and it is what
B4 has owed since the 2026-08-06 amendment.

### 12.1 What is owed

**(a) The internal error must stop being an internal error.** P7's three
routes each report *"please report this program"* for a plain user mistake.
The fix is not in the transformer — by the time B55's guard fires the call site
is gone (§2.1). It belongs at one of the five analyzer sites that currently
`continue` past a `Type::Trait` bound (§1.4), where both the value and the
bound are in hand. The message names the value's trait type, the bound it
cannot satisfy, and the fix, in B72's register:

> `'x' has bare trait type 'Display', so it cannot bind 'T': a trait is not a
> value type (vilan has no trait objects), and there is no concrete
> implementation to select. Give 'x' the concrete type, or make the producer
> generic.`

with a note at the binding's own annotation.

**(b) §2.2's leak must be refused.** A `resource` behind a bare trait type is
a silent destructor deletion (P8) and a silent double-owner (P9). Under the
§12.2 tightening it is refused by construction; without the tightening it
needs its own check, and R12's message is the model.

**(c) The declaration should be refused where the spec says it is** (§1.6) —
which subsumes (a) and (b) entirely.

### 12.2 The recommended shape

**One rule: a trait type in value position is an error at the annotation.**
That is (c), and it delivers (a) and (b) as consequences rather than as
separate checks. It is also the smallest thing to specify, because the spec
already specifies it — this is the compiler catching up, not a language
change.

Slices, in dependency order:

- **S1 — `Self` for std's five sites.** Rewrite `iterator.vl:308`/`:312`,
  `wire.vl:69`, `json.vl:148`/`:150` to `Self`. P3 proves both shapes compile.
  Independent of everything below, and worth having on its own: it says what
  those declarations mean.
- **S2 — the leak pin.** P8's four-program table as a red-first regression
  test, *before* S3, so the fix is proven rather than assumed. **This pin must
  exist even if S3 is deferred**, because it records live data loss.
- **S3 — the refusal.** A trait in value position (binding, parameter, return,
  field, generic argument) is an error at the annotation, carrying B72's steer
  text at the declaration. Invert the four `b72_*` acceptance pins; add P10's
  `List<Trait>` case. S2 goes green here.
- **S4 — bycatch, filed not fixed.** §2.4's duplicate trait impl gets a
  backlog entry, not code, in this arc; so does P17's R11 rationale, as a
  one-line `memory.md` correction. §2.3's `List<Trait>` narrowing needs no
  separate entry — S3 closes it at the same arm.

**Recommendation (§12):** take S1–S3 this cycle whatever the owner decides
about trait objects, and take **S2 first** — it records a live data-loss bug
and costs nothing.

## 13. Open questions

Each carries a recommendation, per the house rule.

**Q1 — Are trait objects wanted at all?**
*Recommendation:* **no.** §3's survey found three real sites, all in std
plumbing, all already served, none blocked; the one head-to-head measurement
in the record went against erasure by 14–18% (§3.3); §4 shows 17 of std's 39
traits could not participate, including the `Wire` trait behind the largest
hand-written vtable. Decline, ship §12, revisit only against a driver
application meeting all three of §3.5's criteria. **This is the owner's call
and the evidence points one way; it does not decide for them.**

**Q2 — If declined, does the spec's rule get enforced, or does the spec
change?**
*Recommendation:* **enforce the spec** (§12.2). Amending `types.md:110-112`,
`:339` and the tour to describe today's four-of-six acceptance documents a
hole rather than closing one, and leaves §2.2's leak live.

**Q3 — Is §2.2's leak B4's, or its own item?**
*Recommendation:* **its own item, filed now, fixed regardless.** It is live
data loss in shipped code with no trait objects anywhere; parking it behind an
L-sized language question is the wrong home for it. This is the paper's one
genuinely urgent finding.

**Q4 — If built: `dyn Show`, or bare `Show` with implicit coercion?**
*Recommendation:* **explicit**, on P15's evidence — implicit coercion silently
changes which method runs. The token is the owner's; the explicitness is not
negotiable on the evidence.

**Q5 — If built: may a trait object hold a resource?**
*Recommendation:* **no in v1** (§8.3), with the drop glue designed so the
refusal is priced rather than accidental. Reopening trigger: a driver
application needing a heterogeneous collection of owned handles — at which
point §8.2's glue is the design, not a new question.

**Q6 — If built: does the vtable carry the trait's members, or B57's
winners?**
*Recommendation:* **B57's winners** (§9.1), which makes coercion
behavior-preserving at no implementation cost.

**Q7 — Does declining B4 leave §3.1's three std sites permanently
hand-written?**
*Recommendation:* **yes, and that is the right outcome for two of them.** The
codec (§3.1c) could not use trait objects anyway — `Wire` is not object-safe
(§4). `Owner` and the capability table could, and would read better for it;
they are the two sites to re-examine if Q1 is ever revisited. Recording them
here is the point: the next person to ask does not have to re-run this sweep.

## 14. The recommendations, collected

| § | question | recommendation |
|---|---|---|
| 3 | trait objects wanted? | **No** — decline on the survey and the measurement |
| 12 | the diagnostic owed | **Ship it** — S1–S3, this cycle, either way |
| 2.2 | the resource leak | Own backlog item, red pin **first**; higher priority than B4 |
| 1.5 | std's five bare-trait sites | Rewrite to `Self`; P3 proves it works |
| 1.5 | does S6/`Iterable` ride B4? | **No** — its design is `Iterable<T, I: Iterator<T>>`; independent either way |
| 1.6 | spec vs. compiler | The compiler moves to the spec |
| 2.3 | `List<Trait>` narrowing | No separate item — S3 closes it at the same reconcile arm |
| 2.4 | duplicate trait impls | Own backlog item; entry criterion if B4 ever ships |
| 8.1 | R11's stated rationale (P17) | Records drift; one-line `memory.md` correction, not B4's to make |
| 10 | B73 | Not the same question — a prerequisite, not a component |
| 6 | representation *if built* | `(value, vtable)` two-element array, one table per coerced pair |
| 7 | coercion *if built* | Explicit, on P15's evidence |
| 5 | asyncness *if built* | Declaration binding in object position only; B29 untouched elsewhere |
| 8 | resources *if built* | Refused in v1, glue designed |
| 9.1 | B57 *if built* | Vtable built from B57's ranking |
| 9.2 | B83 *if built* | Statics excluded either way; B4 does not wait |
| 4 | object safety *if built* | Compiler-computed; the diagnostic names the disqualifying member |
| 11 | migration | One slice, not six |
