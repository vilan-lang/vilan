# `!` and `?` — early return and lifted chains (backlog B11)

Status: **BOTH SLICES SHIPPED 2026-07-04** — `!` and `?.` are live (`void` also became
the unit expression en route). Slice 2 landed as specified: `?` lexes as an operator,
`?.member` joins the postfix chain, and the parser groups each `?.`'s continuation (the
member plus every following plain postfix up to the next `?.`/`!`/chain end — escaping a
group is parenthesization, as in TS) over a `LiftBinder` hole; `Constraint::Lift` grounds
the binder as the subject's element (waking the continuation's deferred constraints),
picks map-vs-flatten from the continuation's type (same-container = flatten; `Result`
flatten checks the error types), and records the lowering; the transformer emits the
match-shaped inline form — bad tag short-circuits AS-IS, the element aliases into the
continuation (no closure), map rewraps via the container's good variant. A lifted chain
is rejected as an assignment target. The `Lift` marker + Option/Result impls ship in std;
user `Lift` lowering shipped in the stabilization pass: a marked container dispatches to
its own `map`/`and_then` instance (the flattening rule picks; the member's `U` binds from
the continuation), the continuation emitted as a closure whose parameter aliases the
binder — the element convention is the container's FIRST type argument (`M<T, ..>`), and
the marker stays the gate (a mappable type without `impl .. with Lift` refuses, pinned).
LSP: completion after `a?.` offers the ELEMENT's members. Ten `?.` pins + corpus
`lift-chain.vl` cover §7's rows.
The two-operator design, the four refinements in §0, and the §8 resolutions (opt-in
`Lift`; the `Try`/`Lift`/`Verdict` names; `Try` as a real trait from day one) are all
settled. Slice 1 landed as specified: postfix `!` in the member chain; `Verdict`/`Try`
+ the `Option<T> → Try<T, void>` / `Result<T, E> → Try<T, E>` impls as real std source
(`operators.vl`/`option.vl`/`result.vl`); `Constraint::TryAssert` types the good half,
checks the enclosing function (std pair by identity — Option-in-Option any element,
Result-in-Result same error; user `Try` types exact-match), and records the dispatch;
the transformer lowers std receivers to the inline tag branch (bad `Option`/`Result`
values return AS-IS — byte-identical at any success type) and user types through their
impl's emitted `verdict`/`from_bad`. Ten pins + a corpus test (`try-assert.vl`) cover
§7's `!` rows; the `assert_fails_spanning` harness pins every error at the `expr!` span.
One solver lesson en route: a new expression kind MUST have an `infer_type` arm
reporting `Unresolved` pre-resolution — without it, a `let` grounding on `expr!`
committed to void before the constraint ran.

## 0. The split, and the settled decisions

Rust folds two different jobs into one `?`: *bail out early* and *keep working inside the
container*. Vilan splits them:

- **`expr!`** — *assert the value is good, secured by a return*: evaluate `expr`; if it is
  good, the expression is the unwrapped value; if bad, **return the bad half from the
  nearest enclosing callable**. Rust's `?` semantics under a more assertive glyph.
- **`a?.b.c(d)`** — *lifted member chains*: apply the rest of the chain to the value
  *inside* the container, staying inside it. TypeScript's `?.` shape with honest monadic
  semantics — and, like every mainstream `?.`, **flattening**.

Settled up front (from review):

1. **`!=` always lexes as not-equals.** Postfix `!` followed by an `=`-starting operator
   requires the space: `a! == b` compares an unwrapped value; `a!==b` is a lex error
   (`!=` then `=`). The formatter always emits the space; the parser's error for the
   soup case should hint at it. (`expr!` is a *value*, not a place — an assignment
   target `a! = b` is rejected in v1; place-ness of unwrapped results is a view-model
   question deferred with the rest.)
2. **`?` flattens.** When the chain's continuation produces the receiver's own container
   type, the result is one level, not nested (`a?.get(1)` on `a: Option<List<T>>` is
   `Option<T>`, not `Option<Option<T>>`). Semantically `map` + `flatten`, i.e. `and_then`.
3. **Expression-level lifting is deferred.** `a? + 10` (reinterpreting an enclosing
   arbitrary expression as the closure body) and the applicative form (`a? + b?`) are
   *not* in scope; `a.map(|x| x + 10)` stays the spelling. `?` is valid only as `?.` — a
   link in a member/call chain.
4. **Both operators are *operators*, not source-text macros.** They dispatch through
   declared operator implementations (the `Add`/`PartialEq` model), so `Signal`, `Promise`,
   or a user type can implement them; the compiler lowers the std cases directly. `!`'s
   *meaning* is fixed — return-when-bad — but *what "bad" is* is programmable per type.

## 1. Motivation

P6 made `Result` the dominant type at every user-facing seam: every generated stub call,
every `decode`, every `connect`. The examples grew `report(...)`-style helpers purely to
hide match boilerplate:

```vilan
// today                                          // with !
match client.add(label) {                         let id = client.add(label)!;
	Ok(let id) => use(id),
	Err(let error) => {                           // with ?.
		print(error.debug());                     let name = user?.profile.name;
		ret;                                      // today: user.map(|u| u.profile.name)
	},
}
```

`!` also unblocks I3's remaining half (validating per-type decode wants `Result`-returning
`from_json` that call sites can propagate tersely) and would simplify *generated* dispatcher
and stub code as much as hand-written code.

## 2. `expr!` — assert-or-return

### Semantics

`expr!` where `expr: M` and `M` implements `Try`:

1. Evaluate `expr` once.
2. Split it by the type's `Try` implementation into **good** (`T`) or **bad** (`B`).
3. Good: the whole `expr!` has type `T`, value = the good half.
4. Bad: **return from the nearest enclosing callable** (the B10 rule — the same boundary
   `ret` uses) with the bad half rewrapped in the callable's return type.

### The `Try` seam *(agreed — a real trait from day one)*

"Bad" is programmed by implementing the operator trait. The trait, `Verdict`, and the two
std impls are **real vilan code in std** from the first slice — not compiler-known
shortcuts (§8.3); the transformer's inline fast path (§4) is an *optimization over* those
impls, pinned semantically identical to the trait dispatch:

```vilan
enum Verdict<T, B> {
	Good(T),
	Bad(B),
}

trait Try<T, B> {
	// Split: is this value good (yielding T) or bad (yielding the residual B)?
	fun verdict(self): Verdict<T, B>;
	// Rebuild a value of Self from a residual — how a bad half returns.
	fun from_bad(bad: B): Self;
}

// Option's residual is the absence itself — `void`, which in vilan IS the
// unit type (an empty tuple; a prettier alias for `()`). It instantiates
// generics like any type (probed: `Result<void, str>` / `Option<void>`
// construct, match, and run), and `void` is also the unit EXPRESSION —
// the type's one value (`Verdict::Bad(void)`, `Some(void)`).
impl Option<type T> with Try<T, void> {
	fun verdict(self): Verdict<T, void> {
		match self {
			Some(let value) => Verdict::Good(value),
			None => Verdict::Bad(void),
		}
	}

	fun from_bad(bad: void): Option<T> {
		None
	}
}

impl Result<type T, type E> with Try<T, E> { .. }  // Bad = the error; from_bad = Err(e)
```

- **v1 compatibility rule:** the nearest callable's declared return type must be the
  **same named type** as the receiver — `Option<_>` inside an `Option`-returning function
  (any element: the bad half is `None`, which fits every `Option<U>`), `Result<_, E>` with
  the **same `E`** inside a `Result`-returning function (`Err(e)` re-wraps at any success
  type). No `Option` inside `Result`, no error conversion — a `From`-style conversion layer
  is the recorded follow-up, not v1.
- **Why `from_bad` isn't enough generally:** `from_bad(bad): Self` returns the *receiver's*
  `Self` (`Option<i32>`), while the enclosing function may return `Option<str>`. Vilan has
  no higher-kinded types to say "same constructor, other element", so for the std pair the
  compiler rebuilds directly (`None` / `Err(e)` at the enclosing type's arguments), and for
  **user `Try` types v1 requires the enclosing return type to equal the receiver type
  exactly**. Stated limitation, loosened if associated-type machinery ever lands.
- **Where `!` is legal (v1):** inside a *function* whose declared return type satisfies the
  rule. Inside a closure or `async` block: a clean compile error for now — closures' return
  types are inferred, and B10 deliberately left `ret`-in-closures unchecked. **First
  follow-up** (not v1): allow `!` where the closure's return type is contextually known —
  the motivating case is RPC handler closures (`|request| { ... }` returning `RpcOutcome`,
  which would carry its own `Try` impl so a handler can write `let n: i32 = arg(request)!`).
  B10's return-position checking is what makes every one of these cases *diagnosable*.

### Grammar & lexing

- Postfix, binds tighter than prefix `!` (logical not) and all binary operators;
  chains left-to-right: `a!.b!` unwraps twice, `config().port!` applies to the call result.
- The `!=` rule from §0. The only reserved pair: `!=` wins; everything else about postfix
  `!` is whitespace-insensitive.
- The glyph deliberately diverges from Swift/Kotlin (`!` = trap there). Vilan's postfix `!`
  **never panics** — trapping stays spelled `.unwrap()`. The docs own this loudly.

## 3. `a?.b` — lifted member chains

### Semantics

`?` appears only as `?.` — a link in a member/call chain. The segments **from one `?` to
the next `?` (or the chain's end)** form one continuation:

```vilan
a?.b.c(d)          // chain(a,  |x| x.b.c(d))
a?.b.c(d)?.e       // chain(chain(a, |x| x.b.c(d)), |y| y.e)
```

Each `chain(recv, k)` is typed by the continuation's result:

- `k: |T| U` where `U` is **not** the receiver's container → **map**: result `M<U>`.
- `k: |T| M<V>` (the receiver's own named type) → **map + flatten**: result `M<V>`.

This is the flattening every mainstream `?.` has (settled, §0.2): `a?.get(1)` on an
`Option<List<T>>` is `Option<T>`. "The receiver's own container" = the same struct/enum id
— the analyzer's ordinary nominal check, no higher-kinded reasoning needed.

- **Not an assignment target:** `a?.b = x` is a parse error (v1; matches TS).
- **Bare `a?`** (no following `.`) is a parse error — it would be `map(identity)`.
- Mixing is natural and ordered postfix-left-to-right: `a?.parse()!` lifts, then
  asserts-or-returns on the lifted result.

### The `Lift` seam *(agreed — opt-in)*

Opt-in (§8.1), so `?.` doesn't silently work on everything that happens to have a `map`:

```vilan
trait Lift {}                      // the marker: this type supports `?.`
impl Option<type T> with Lift {}
impl Result<type T, type E> with Lift {}
```

The operator then resolves the receiver's **`map`** and **`and_then`** methods by the
ordinary method machinery (the `for … in` / `next()` duck-typed-protocol precedent) and
picks per the flattening rule. A type opting in supplies those two methods with the usual
shapes; `Signal` (derived signals: `signal?.field` — its `and_then` is exactly the A4
`flatten` combinator) and `Promise` are the recorded candidates, **not v1** — each is its
own decision because the reading of `?.` silently changes domain (reactive/async) with the
receiver.

## 4. Lowering *(agreed)* — operators, not rewrites

Per §0.4, neither operator is a source-text expansion. The house pattern is the binary
operators (`Add`/`PartialEq`: trait-declared, analyzer-recorded in `binary_op_dispatch`,
transformer-emitted):

- The analyzer records a `try_dispatch` / `lift_dispatch` entry per operator site (receiver
  type, continuation ids, chosen map-vs-chain), monomorphizing the continuation as an
  IR-level closure — never pasted source.
- The transformer emits:
  - **std fast path** — `Option`/`Result` lower to inline tag checks (`Option` is a tagged
    array at runtime): `a?.b.c` becomes a branch, no closure allocation; `expr!` becomes a
    branch + `return` — *cheaper* than the `.map(..)` the sugar replaces.
  - **trait path** — any other `Lift`/`Try` type dispatches to its impl's methods, exactly
    like a user `Add`.

## 5. Interactions with what already shipped

- **B10:** `!`'s "nearest enclosing callable" is `ret`'s rule; the return-position checker
  is what turns every misuse (wrong enclosing type, `!` in a bare-void function) into a
  clean spanned error instead of a miscompile.
- **E7:** both operators anchor their diagnostics at the operator token / the offending
  chain link; every error case in the test plan carries an `assert_fails_spanning` pin.
- **LSP:** completion after `a?.` must offer the **inner** `T`'s members (not `Option`'s) —
  the receiver for member resolution is the lifted value. Hover on `!` shows the
  unwrapped type.
- **Formatter:** `a! = b` prints with the space (§0.1); `?.` prints tight.

## 6. Deferred (recorded, not drifted into)

- ~~Return-position generics through `!`~~ — **fixed** (stabilization pass): annotated
  lets seed their expectation onto the value, and `resolve_try_assert` re-infers the
  receiver as `Container<expected, ..>` once the container is known — the binding rides
  the same reconcile-and-record channel as the two-step form. Pin un-ignored.

- Expression-level lifting (`a? + 10`) and the applicative form (`a? + b?`) — §0.3.
  **Proposal drafted 2026-07-16 (`expression-lifting.md`, awaiting review):** lift
  regions bounded at slot roots, applicative = left-to-right short-circuiting
  `and_then` nesting, std inline lowering, five open questions recorded.
- ~~Error conversion across types (`Option` in a `Result` fn; `From`-style `E1 → E2`)~~
  — **resolved 2026-07-15: EXPLICIT by design (§9)**. `!` stays same-type; convert at
  the value first — `.map_err(to_e2)!` for `E1 → E2`, `.ok_or(err)!` for `Option` in a
  `Result` fn. No implicit `From`/`Into` coercion (the no-silent-conversion rule).
- `!` inside closures/async blocks — kept deferred through the stabilization pass: its
  real payoff needs the `arg → Result` API redesign (the RPC-handler case), and a
  bang-in-tail closure is semantically invalid anyway (`|k| lookup(k)!` cannot rebuild
  the bad half into its own unwrapped return); the future check can say so precisely.
- `Signal`/`Promise` opting into `Lift` (each its own review).
- User-`Try` types returning a *different* instantiation than the receiver (needs
  associated-type machinery).

## 7. Test plan (per case, as always)

- **`!`:** `Ok`→value / `Err`→returned (observable via caller); `None`→returned; wrong
  enclosing return type (span pin at the `!`); mismatched `E`; bare-void function; `!` in
  a closure (v1 error); `a!.b!` chains; `a! = b` spacing (lex pin both ways: `a!=b` is
  comparison); formatter idempotence; goldens for the inline lowering.
- **`?.`:** map case (plain member) and flatten case (Option-returning member) both
  pinned by *type* (`Option<T>`, not `Option<Option<T>>`); segment grouping
  (`a?.b.c` short-circuits `.c` when `a` is `None` — runtime pin); multi-link chains;
  `?.method(args)`; `?.` on a non-`Lift` type (span pin); `?` not followed by `.` (parse
  pin); `a?.b = x` rejected; `?.` + `!` composition; corpus byte-identical throughout
  (nothing uses the operators yet).

## 8. Resolved (2026-07-04)

1. **`Lift` is an opt-in marker trait** — silent lifting over any mappable type reads as
   a footgun.
2. **The names stand:** `Try`, `Lift`, `Verdict`. (A fourth name, `Absent`, was briefly
   proposed as Option's residual and dropped: `void` instantiates generics fine — probed —
   and is the canonical nothing, so `Try<T, void>` needs no new type. `Result<void, str>`
   stays exactly `Result<void, str>` everywhere.)
3. **`Try` is a real trait from day one** — the trait, `Verdict`, `Absent`, and the
   `Option`/`Result` impls ship as std source in slice 1; the compiler's inline lowering
   is an optimization over those impls, not a substitute for them (pinned equivalent: a
   user-`Try` type and `Option` must behave identically through `!` modulo the v1
   same-type restriction).

## 9. Error conversion at the `!` boundary — resolved: EXPLICIT (2026-07-15)

§6's `E1 → E2` deferral asked how `!` should cross error types. **Decision (settled
with the user): it does not — conversion is explicit, at the value, before the `!`.**
`!` stays same-type: it returns the bad half *as-is*, so the value's error type must
already be the function's. Rust folds a `From`-conversion into `?`; vilan does not,
for the same reason it forbids a silent view→value cross (transparent-references) — an
error changing type is a real operation, and the language does not perform real
operations invisibly. The `Add`/`Try` "programmable per type" rule (§0.4) governs what
*bad* means, not an automatic coercion of it.

**The explicit path — already complete, no new machinery.** The std combinators
compose with `!` today:

- **`E1 → E2` (`Result`):** `value.map_err(to_e2)!` — `Result::map_err(|E1| E2)` maps
  the error, then `!` returns the now-matching `E2`. A named fn or a closure both work
  (`query().map_err(|e| AppError { msg = e })!`).
- **`Option` in a `Result` fn:** `opt.ok_or(err)!` — `Option::ok_or(E)` turns `None`
  into `Err(err)` with a caller-supplied error, then `!` returns it. `ok_or_else(|| …)`
  for a lazy error. This is why `Option`-through-`!` requires a `Result` fn: the error
  value is *supplied here*, not fabricated.

So the only compiler work is **diagnostics**: the two mismatch errors, which read like
a missing feature ("error conversion is not supported yet"), instead point at the
explicit helper —

- `Result` `E1 != E2` → "…the error types must match; convert first with
  `.map_err(…)` before `!`".
- `Option` in a non-`Option` (`Result`) fn → "…convert to `Result` first with
  `.ok_or(err)` (or `ok_or_else`)".

**Scope.** `!` diagnostics + the analogous `?.` flatten mismatch message (§3, same
shape). No lowering change — `!` is untouched, so all existing codegen stays
byte-identical.

**Test plan (per case).** `map_err(fn)!` and `map_err(|e| …)!` run and convert
(observable via the caller's `Err`); `ok_or(e)!` converts `None`→`Err` and runs;
same-type `!` unchanged; the `E1 != E2` mismatch is rejected with the `.map_err` hint;
`Option`-in-`Result` rejected with the `.ok_or` hint; a docs example shows the pattern.

## 10. The B11 tail, verified (2026-08-04)

B11's entry predates the STATUS convention and had never been reconciled, so
every one of its three remainders was re-verified against the tree — the
record read, then a probe compiled through the worktree binary. Result: one
was settled and buildable, two are genuinely design-gated.

| Remainder | Verdict | Evidence |
| --- | --- | --- |
| the bare-`?` **trait path** (user `Lift` containers) | **SETTLED — built, §11** | claim true (probe below); design fully specified by `expression-lifting.md` §4 + §7 |
| **closure `!`** (the RPC-handler follow-up) | **OPEN — §12.1** | claim true (probe below); the `arg → Result` linkage exists nowhere in `proposal/`, and std has built nothing toward it |
| **`Signal`/`Promise` `Lift` opt-ins** | **OPEN — §12.2** | claim true (probe below); the record names them as candidates and supplies no semantics; the contract members do not exist |

### 10.1 A disambiguation the entry needed

"Bare `?`" in B11 is **expression lifting's** `?` (`expression-lifting.md`),
not §3's sentence "**Bare `a?`** (no following `.`) is a parse error" — that
line was **superseded 2026-07-16**, when expression lifting shipped v1 and
took over the grammar space. §3 is left as written because it is the record
of the `?.` slice as designed; read it with this note.

And the `?.` **chain** trait path is *not* the remainder: it shipped in the
stabilization pass (the status header says so, and
`a_user_lift_container_dispatches_to_its_own_map_and_and_then` pins a `Boxy`
dispatching to its own `map`/`and_then`). What was left is **region-only**:
expression lifting shipped "live for the std pair", so a user `Lift`
container at a bare `?` was rejected.

### 10.2 The probes

Each run through `target/debug/vilan check` in the `lift-tail` worktree
(the `PATH` binary is 0.27.0 and predates none of this, but is not the tree
under test).

**(1) bare `?` on a user `Lift` container** — a `Boxy<T>` with
`impl Boxy<type T> with Lift {}` plus a conforming `map`/`and_then`,
`let doubled = boxed? * 2;`:

```
Error: a bare `?` lifts an `Option` or a `Result`; this is Boxy<i32>
       (expression lifting for user `Lift` containers is a recorded
        follow-up; `?.` chains already support them)
```

Claim **confirmed** — a clean error that steers to `?.`, raised in
`resolve_lift_region`, pinned by
`expression_lift_on_a_user_container_is_the_recorded_follow_up`. And the
design is **complete on paper**: `expression-lifting.md` §4 specifies the
trait path exactly — "nested `and_then` calls ending in `map`, each
continuation an IR-level closure over the remaining region — the user-`Lift`
chain lowering, nested. Left-to-right, so effects order as written" — and §7's
test plan already carries the row "user-`Lift` type through the trait path
(effects ordered)". Nothing was left to decide, only to build. → §11.

**(2) `!` inside a closure** — `|k: str| { let n = lookup(k)!; n + 1 }`:

```
Error: `!` requires the nearest enclosing function to declare an
       `Option`/`Result`-compatible return type (closures and `async`
        blocks are not yet supported)
```

Claim **confirmed**. The check is in the walk (`Node::TryAssert`) and turns
on the *frame kind* — anything that is not a `ReturnFrame::Function` is
refused — so a contextually-typed closure gets the same error today; the
type's availability is not what gates it.

Is the linkage design settled anywhere? **No.** An anchored sweep of
`proposal/` finds four mentions, all of them deferrals:

- §2 here: "**First follow-up** (not v1) … the motivating case is RPC handler
  closures (`|request| { ... }` returning `RpcOutcome`, which would carry its
  own `Try` impl …)".
- §6 here: "its real payoff needs the `arg → Result` API redesign".
- `transport-rpc.md` Q10: "really a **general error-handling dependency** …
  Track as a prerequisite; revisit when `?`/try lands."
- `p6-followups.md`: filed under "Further out (own proposals)".

And std confirms nothing was built toward it: `arg<T: Wire>(request, index): T`
returns `T` bare (a garbled argument poisons the request's deserializer, and
`decode_failed` is the separate gate), and `grep "with Try"` over
`vilan/std/src` finds exactly two impls — `Option` and `Result`. `RpcOutcome`
has none. → recorded as the design question it is, §12.1.

**(3) `?.` on a `Signal`** — `Signal<Profile>`, `s?.name`:

```
Error: `?.` lifts an `Option`, a `Result`, or a type opting in with
       `impl .. with Lift`; this is Signal<Profile>
```

Claim **confirmed**, and it is the *ordinary* opt-in gate — nothing
special-cases `Signal` or `Promise`. The record never gave them semantics:
§3 calls them "the recorded candidates, **not v1** — each is its own decision
because the reading of `?.` silently changes domain (reactive/async) with the
receiver", and §6 repeats "each its own review".

The contract members do not exist either. `Signal<T>` has
`map<U>(self, transform: sync |T| U): Signal<U>` and **no `and_then`**; its
`flatten` lives on a *specialized* `impl Signal<Signal<type U>>`, so §3's
"its `and_then` is exactly the A4 `flatten` combinator" is a sketch, not a
member — someone would have to write
`and_then<U>(self, fn: sync |T| Signal<U>): Signal<U>` (= `map(fn).flatten()`,
carrying `map`'s `sync` bound outward). `Promise<T>` is an `external struct`
whose only member is `Promise::all`: neither `map` nor `and_then`. → §12.2.

## 11. SHIPPED 2026-08-04 — the bare-`?` trait path

A user `Lift` container now lifts a whole expression, not only a `?.` chain.
Built exactly as `expression-lifting.md` §4 specifies it; nothing was
redesigned on the way in.

**The lowering.** `s₁.and_then(|x₁| … sₙ.map(|xₙ| body))` — nested `and_then`
calls ending in `map`, each continuation a closure over the rest of the
region, evaluated left to right. The last split takes `and_then` instead of
`map` when the body already yields the container (the flatten rule,
inherited). Emitted for `left? + right?`:

```js
$K(left, ($L) => { return $H(right, ($M) => { return $L + $M; }); })
```

Short-circuiting and laziness are therefore **the container's own
`and_then`** — the region does not branch on a tag, because there is no tag
it may assume. That is the honest reading of an opt-in operator, and it is
what §4 asked for. The std pair keeps its inline tag-branch lowering
untouched; nothing about `Option`/`Result` moved a byte.

**A region is a nest, so dispatch is per SPLIT, not per region.** This is the
one structural difference from the chain, where a single `LiftDispatch::Trait`
sufficed. `record_lift_region_trait_path` records one `Trait` entry per split
under the split's **binder id** — a synthetic id the region rewrite mints, so
it can never collide with a receiver that is itself a `?.` chain carrying its
own entry — and a new marker variant, `LiftDispatch::TraitRegion`, sits at the
region to select the path. Every split but the last wants `and_then`; the last
wants `map`, or `and_then` under flatten. All of them share **one** `U`, the
region's element: `and_then<U>(self, |T| Self-of-U)` and `map<U>(self, |T| U)`
agree on it, which is why the nest needs no per-level `U` bookkeeping.

**A hoisted eval binds inside the enclosing continuation.** §2's rule that
everything left of a `?` has already evaluated survives the nesting for free:
an eval step between two splits emits into the *closure body* of the split to
its left, so `boxed("L")? + noise("M") + boxed("R")?` prints L, M, R. Pinned.

**Four helpers now serve both operators** rather than one — `opts_into_lift`
(the marker gate), `lift_opt_in_error`, `lift_contract_member`,
`lift_element_of`. Two diagnostics improved as a side effect: the `?.`
contract error now says "an `and_then`", and `render_container_name` learned
user enums and structs, so a mixed region names `Boxy` instead of
"container" and drops the `.ok_or` steer when the pair is not the std pair
(where it would have been nonsense).

**`Lift` is a marker, so B29 has nothing to check.** The trait declares zero
members; per-member conformance cannot verify a `map`/`and_then` that are not
trait members. The duck-typed lookup **is** the contract's gate — the same
one the `for … in` / `next()` protocol uses — and it names the member it
wanted. Recorded here because "B29 covers it" is the natural wrong assumption.

**Pins: 8, seven red-first** (`inference.rs`) — map; the nested
and_then-ending-in-map shape; flatten; effect order across a hoisted eval; the
marker gate (a mappable non-`Lift` type still refused); the named contract
member; a mixed user/std region; and chain non-absorption, which was green
before the change and after it (the `?.` meaning is untouched). Every pin
reads the tag its `Boxy` fixture appends per member, so a pass proves *which*
member ran and in what order — a value-only assertion could not tell the trait
path from the std one. Corpus `expression-lift.vl` gains the three runnable
shapes; its golden is purely additive (+35/−0, no existing byte moved) and
interpreter equivalence holds. Docs: `std/traits.md` gains the worked example
and states the marker-vs-contract split; `tour/control-flow.md` drops the
"std pair only for now" note.

**One limitation, stated not hidden.** A region's result reuses the FIRST
split's type arguments with the element replaced — so for a container with
arguments beyond the element (`M<T, Meta>`), a region mixing two different
`Meta`s takes the first. This mirrors the chain path exactly, and `Result`'s
same-`E` rule is the only tail check §2 ever specified. No user container in
the tree has a second argument; revisit if one appears.

## 12. Open — the design-gated remainder of B11

Both are recorded here so B11 can be narrowed to exactly this at
reconciliation. Neither is "not built yet"; each is "not decided yet", and
the deciding is the work.

### 12.1 `!` inside a closure — the `arg → Result` linkage

**What exists.** `!` requires a declared enclosing *function* return type; a
closure or `async` block is refused with a clean, spanned error (§2's v1
rule). The check turns on the frame kind, so even a contextually-typed
closure is refused today.

**The motivating case, restated.** An RPC handler is a closure —
`async |RpcRequest| RpcOutcome`. Reading its arguments should be terse:

```vilan
dispatcher.on("add", |request| {
    let label: str = arg(request, 0)!;   // wanted
    reply(service.add(label))
})
```

Today `arg<T: Wire>(request: RpcRequest, index: i32): T` returns `T` bare — a
garbled argument poisons the request's deserializer instead of failing at the
pull, and `decode_failed(request)` is a separate gate the handler must
remember to check before running the impl. So the handler either regrows a
per-argument match or leans on the poison-then-check dance.

**What would have to be decided** — three couplings, and the reason this is
one design and not three fixes:

1. **Does `arg` become `Result<T, RpcError>`?** That is the "linkage": it
   changes the generated dispatcher, the hand-written handler shape, and the
   relationship between per-pull failure and the sticky `decode_failed` gate
   (does the poison stay as a second mechanism, or does it go?). Owned by
   `transport-rpc.md` Q10, which explicitly declined to decide it without the
   `?`/try operator — which now exists.
2. **Does `RpcOutcome` implement `Try`?** It is the closure's return type, so
   `!` must be able to rebuild a bad half into it. `RpcOutcome::Failure(RpcError)`
   is the obvious `Bad`, which makes `Try<T, RpcError>` — but `RpcOutcome`'s
   `Success` carries a *describer closure*, not a value, so what `T` is (and
   whether `verdict` can even split it) needs stating. Today std has exactly
   two `Try` impls, `Option` and `Result`; this would be the first user-shaped
   one, and §2's v1 rule that a user `Try` type requires the enclosing return
   type to **equal the receiver type exactly** collides head-on with a handler
   whose receiver is a `Result` and whose return is an `RpcOutcome`. Either
   that rule loosens (associated types — §6's other deferral) or the
   conversion is explicit at the value (§9's rule), which may be the answer.
3. **Which closures may host a `!` at all?** §6 records the reason this is not
   simply "allow it where the return type is known": a bang-in-tail closure is
   *semantically invalid* — `|k| lookup(k)!` cannot rebuild the bad half into
   its own unwrapped return. So the rule is not "closures with a known return
   type" but something narrower, and the check must be able to say precisely
   which case a given closure is. B10's return-position checking is what makes
   that diagnosable.

**Verdict: genuinely open.** No proposal in the tree settles any of the three;
`p6-followups.md` files it under "Further out (own proposals)", which is the
right shelf. It wants its own proposal, and (1) is the load-bearing decision —
(2) and (3) mostly follow from it.

### 12.2 `Signal` / `Promise` opting into `Lift`

**Why this is not a std one-liner.** `impl Signal<type T> with Lift {}` would
compile the moment the members exist — the marker is the whole opt-in. The
members are the problem, and behind them a semantic fork the record has always
flagged ("the reading of `?.` silently changes domain with the receiver").

**`Signal` — reading versus subscribing.** `Signal::map` mints a *derived*
signal and pushes a subscriber; the subscription is, in the source's own
words, "deliberately unowned". So `signal?.field` would not read a field: it
would create a new signal and a live subscription with a lifetime. That is a
defensible reading — `?.` as the derived-signal combinator — but it is a
choice, and the alternative (`?.` as a one-shot `get()` then project) is the
one a reader coming from `Option` will assume. **Whichever is chosen, the
subscription's ownership has to be answered**, and B21 (`View.style_var` leaks
its subscription, shipped 2026-08-04) is the evidence that this is not
theoretical: an unowned subscription minted by a terse operator is exactly the
shape that leaked. A `?` per render would mint one per render.

Mechanically, `Signal` also has **no `and_then`**. §3's claim that "its
`and_then` is exactly the A4 `flatten` combinator" is a sketch: `flatten` is a
method on the *specialized* `impl Signal<Signal<type U>>`, not a member of the
general impl. Someone would write
`and_then<U>(self, fn: sync |T| Signal<U>): Signal<U>` as `map(fn).flatten()`
— and `map`'s `sync` bound propagates outward, so a lifted expression over
signals could not contain an `await`. Worth stating up front rather than
discovering.

**`Promise` — `?` versus `await`.** `Promise<T>` is an `external struct` with
neither `map` nor `and_then`, and adding them means adding `.then` wrappers.
But the prior question is whether `promise?.field` should exist *at all* in a
language where `async fun` calls are implicitly awaited and `await p` is the
spelling for reaching a promise's value. `?` on a promise would be a second,
quieter way to sequence async work — and unlike `Option`, deferring the
reading is not free: it changes *when* the code runs. The domain-change
warning in §3 is sharpest here.

**Verdict: genuinely open, and correctly two separate reviews.** Neither
opt-in is blocked on compiler work — the trait path §11 shipped is the
machinery both would use. They are blocked on answering, for each type, what
`?` should *mean*.
