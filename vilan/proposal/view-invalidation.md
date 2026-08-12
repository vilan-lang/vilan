# Views and invalidating events — rule 4 completed, `await` included (C3 + C2's static half)

Status: **SHIPPED 2026-07-09** — both phases, same day (E2 commit + E3
commit; ~25 pins in `inference.rs`). Implementation findings folded back in:
§2's scalar-root exemption (the E2 gate tripped on the
transparent-references corpus demo — scalar cells have no geometry), and two
E3 notes: `Shared.read()` returns a COPY by design so only `write()`'s view
fences `await` (value semantics quietly made reads safe), and the signature
rule anchors at the parameter NAME. The scan also gained wrapped-match-leg
capture liveness and `for e in &mut` loop-binding origins (fixing a
pre-existing E1 loop gap). C2's dynamic remainder stays open (§6).

## 0. The one-sentence model

A view is live from its declaration to the end of its block; while it is
live, its target must not be **invalidated** — and there are exactly three
kinds of invalidating event:

| | Event | Example | Status today |
|---|---|---|---|
| E1 | Reassignment of the viewed root | `a = []` | **Caught** (shipped rule 4, `check_invalidation`) |
| E2 | A mutating call on the viewed root | `a.remove(i)`, `a.push(x)`, `f(&mut a)` | **Silent** — deferred in the shipped check's own comment |
| E3 | A suspension point | `await tick()` | **Silent** — backlog C3 |

E1 is shipped. This proposal adds E2 and E3 to the *same lexical-liveness
scan*, which is the whole implementation story: one scan, three event kinds.
What remains of C2 afterwards is only the genuinely dynamic remainder (§6).

## 1. Current behavior (the probe programs)

Four user-posed cases plus the await case. Each is a standalone program in
the session scratchpad (`c3-p1.vl` … `c3-p4.vl`, `c3-probe.vl`); re-verify
before implementation and pin each as a test.

```vilan
// P1 — view of an element of an EMPTY list.
mut a = [];
let b = &mut a[0];
```
No machinery consults bounds or emptiness when a subscript view is minted:
`&mut a[0]` lowers to the scalar `(base, key)` pair `[a, 0]` regardless of
`a`'s length. **Observed:** this exact program happens to be a compile error
today — but only because the empty literal's element type never grounds, and
the message is circular ("cannot index List (only a `List` is indexable)").
An empty-at-runtime list of KNOWN element type (see P3) mints the view
silently. **Out of scope here** — this is the *subscript absence* question
(what `a[0]` itself means on a missing element), the same question with or
without a view. Recorded as backlog **I4**, together with the circular
message.

```vilan
// P2 — whole-root reassignment while an element view is live.
mut a = [ 1, 2, 3 ];
let b = &mut a[0];
a = [];
```
**Compile error today** (shipped rule 4):
`cannot reassign 'a' while a view into it is live (rule 4: no invalidating
mutation under a live view).` Liveness is **lexical** — declaration to end
of block, not last-use — so the error fires even if `b` is never read after
the reassignment. That conservatism is deliberate and this proposal keeps it.

```vilan
// P3 / P4 — a mutating call while an element view is live.
mut a = [ 0 ];
let b = &mut a[0];
a.pop();                // P3: method taking &mut self
grow(&mut a);           // P4 also passes the root to a free fn by &mut
b = 99;
print(a[0]);
```
**Silent today, corruption confirmed.** `check_invalidation`'s doc comment
defers it in as many words: *"(Resize / move / drop invalidation, and
index-into-container views, are deferred.)"* Only whole-binding `Assignment`
counts as invalidating. Observed runtime: `b` is the pair `[a, 0]`; after
`pop()` empties the list, `b = 99` **resurrects slot 0** — `a == [99]`, no
error anywhere. (The original discussion used `remove(i)`; `List` has no
`remove` yet — `pop()`/`push()` exhibit the class identically, and E2 covers
whatever removal methods `List` grows.) A bonus finding while probing:
`print(b)` prints the raw `(base, key)` pair (`[ [ 99 ], 0 ]`) instead of
auto-dereffing the scalar view — a transparent-references gap in
argument position, recorded in §5.

```vilan
// The await case (probed 2026-07-09: compiles, prints 99).
async fun mutate_across_await() {
    mut point = Point { x = 1 };
    let view = &mut point;
    await tick();
    view.x = 99;
}
```
**Silent today.** Safe in this exact program only because `point` is
frame-local; see §4 for why it must be rejected anyway.

## 2. E2 — mutating calls are invalidating writes (the static half of C2)

### The rule

While a view rooted at `R` is live, any call that passes `R` — or a place
rooted at `R` — by **`&mut` convention** (the receiver's inferred `borrows`
self, or an explicit `&mut` argument) is an error. Constant vs dynamic index
is irrelevant: the rule never asks *which* element dies, only that the call
*may* move, drop, or reallocate elements.

This is Rust's answer made vilan-shaped: Rust doesn't detect
`remove`-under-borrow dynamically either — `&mut a[0]` exclusively borrows
all of `a`, so *any* `&mut a` use while the element borrow lives is a
compile error. Vilan reaches the same totality through conventions the
analyzer already infers.

### What does and does not invalidate

- **Invalidates:** `a.remove(i)`, `a.push(x)`, `a.clear()`, `a.insert(..)`,
  any user method taking `self` by `&mut`, `free_fn(&mut a)`. `push` is
  included deliberately: it is harmless on the JS backend but reallocates on
  the native backends (F3/F4) — the rule is a language fact, not a backend
  accident.
- **Does not invalidate:** reads and `&self` methods (`a.length()`);
  writes *through the view itself* (`b = 99` writes the element — that is
  the view's purpose); direct writes to a *different field* of a struct
  root (`s.x = 1` while `&mut s.y` lives — field writes change contents,
  not geometry; no `(base, key)` pair is disturbed); calls on unrelated
  containers.
- **Scalar roots are exempt** *(implementation finding, 2026-07-09 — the E2
  gate tripped on the transparent-references corpus demo itself)*: a viewed
  scalar local (`mut a: i32; let b = &mut a; add_ten(&mut a)`) cannot be
  invalidated — its boxed cell has no geometry, so every possible callee
  action is a slot write, which is precisely the aliasing transparent
  references define and the corpus pins. E2 therefore applies to roots with
  **detachable structure**: containers (element geometry) and structs (a
  callee holding `&mut s` can reassign an aggregate field out from under a
  held field view). Two recorded conservatisms: a *scalar-field* view under
  a `&mut s` call is flagged though its `(base, key)` slot is actually
  stable (distinguishing it needs chain analysis; take it if the pattern
  appears in practice), and generic-typed roots are flagged (their
  monomorphized geometry is unknown at the check).
- **Whole-root reassignment stays E1** (shipped, unchanged — including for
  scalar roots, where reassignment is treated as rebinding intent even
  though the boxed cell would technically survive; the asymmetry is
  deliberate).

### Diagnostic

Anchored at the call (events anchor; the message names the root), matching
the E1 message's shape:

> `cannot mutate 'a' with '.remove(..)' while a view into it is live (rule 4: no invalidating mutation under a live view).`

## 3. E3 — a view may not live across `await` (C3)

### The rule

1. **Body rule:** inside an async body, an `await` occurring while *any*
   view is live is an error — root-independent (unlike E1/E2, suspension
   invalidates every view: the writer set during a suspension is the whole
   program).
2. **Signature rule:** an `async fun` may not declare `&`/`&mut`
   parameters. The caller's view would be held inside the suspended callee
   across *its* awaits — the same hazard one frame down. Sync callees stay
   free to take views (they cannot suspend), which is what keeps the whole
   analysis local: no call-graph pass, just async bodies and async
   signatures.
3. **Async closures:** an `async { .. }` / async closure may not capture a
   view (binding or parameter) — its body suspends with the capture live.
   The existing escape machinery already rejects view-param captures in
   escape positions; the new scan covers async closure bodies uniformly.

### Why (the three layers, from the discussion)

- **Semantic, real today:** an `await` yields to arbitrary other turns.
  Rule 4's static story works because the analyzer sees every writer
  between a view's creation and its block end; a suspension point makes the
  writer set unknowable. Anything reachable (`Shared`, signals, captured
  state) can be mutated mid-flight: `(base, key)` views get reseated
  (P3/P4's class), object views write into detached copies.
- **Architectural:** every local that survives an `await` becomes a field
  of the continuation object. A view across `await` *is a view stored in a
  struct* — the thing second-class views forbid everywhere else; the async
  frame is just a struct the compiler synthesizes. JS closure scope hides
  this; a state-machine lowering makes it literal.
- **Strategic (F3/F4):** Rust permits references across `await` by paying
  with lifetimes through the generator plus `Pin` (self-referential
  futures). Vilan's bet — second-class views ⇒ no lifetime machinery, no
  pinning, scope-end destruction — survives resumable frames only if views
  never cross suspension points.

### No `Shared` exemption

The backlog's open sub-question, answered **no**. `Shared`'s handle
(captured by value) does pin the cell — memory-safe even natively — but
memory safety was never the only hazard: another turn's `write()` reseats
or removes elements under a held `read()` view, which is exactly P3 through
a different door. Uniformity keeps the rule teachable: *views never cross
`await`; re-acquire after.* The fix is always one line, and re-acquiring is
the semantically honest operation — after a suspension the world may have
changed, and re-reading acknowledges it.

### Diagnostics

Anchored at the `await` (the event), naming the live view(s):

> `cannot hold a view across 'await': 'b' (a view into 'a') is still live here. Re-acquire the view after the await — the awaited turn may change what it points at.`

Signature rule, anchored at the parameter, naming the form it saw:

> `an async function cannot take '&mut' parameters: the view would be held across its suspension points. Pass a value, or a Shared/handle.`
> `an async function cannot take '&' parameters: the view would be held across its suspension points. Pass a value, or a Shared/handle.`

Both spellings were always implemented — the `Ref` and `RefMut` arms sit
side by side — but only the `&mut` one was pinned, and only it was quoted
here. B112's survey found the gap; cycle 15 pinned the `&` form (a free
parameter and an `&self` receiver, which is an ordinary `Ref` parameter and
anchors on the `self` token).

**A gap the pins found — CLOSED 2026-08-12, see §7.** The signature rule
fired only when the body contained an EXPLICIT `await` token (`saw_await`),
so the implicit-await spelling — calling an async function without the
keyword, which `spec/execution.md` §7 sanctions — bypassed it for both
forms: `async fun stash(viewed: &mut Point) { let beat = tick(); viewed.x =
beat; }` compiled, and emitted `const beat = await (tick());` with the
caller's view live across it. Tightening the gate to *declared* asyncness
was not the fix: it would also reject an `async fun m(&self)` whose body
never suspends, which B29's declared-async impl of a sync trait method
relies on. The answer is the CALL GRAPH's — a call gates the rule when it
can suspend — and declared asyncness read of the CALLEE, never of the body
being checked, is what lets both hold at once. Rules 1 and 3 turned out to
share the hole; §7 closes all three.

### Relation to A6

C3 does not block A6 (async turns / optimistic-write → `await` →
reconcile); it is A6's ground rule. A6's reconcile step is built on
*re-reading* state after suspension; C3 turns "state held before the await
is not trustworthy after it" from convention into a compiler-enforced fact.

## 4. Implementation plan

Both events ride `check_invalidation`'s existing scan (post-build; view
origins from `compute_view_origins`, conventions from inferred `borrows`).

- **Phase 1 — E2** (smaller; do first). In `scan_invalidation`, on
  `Expr::Call`: if any argument position (receiver included — the wired
  self argument) passes a place whose `place_root` is a viewed root by
  `Ref`**Mut** convention, record a violation like E1's. Pins per case:
  P3 (`remove(0)`), P4 (dynamic index), `push`/`clear`, a user `&mut self`
  method, `free_fn(&mut a)`, and the guards — `&self` method (no error),
  write through the view (no error), sibling-field write on a struct root
  (no error), unrelated container (no error), view created *after* the
  call (no error; scan order already handles it), nested blocks and loops.
- **Phase 2 — E3.** The same scan learns suspension events (`await`
  expressions — `async_infer` already identifies them) as violations
  against *every* live view; plus the signature rule (async fns reject
  `Ref`/`RefMut` parameter conventions) and the async-closure capture rule.
  Pins: the §1 await probe, view created after the await (no error), await
  in one branch only (error — lexical liveness), `for e in &mut c { await
  .. }` (error; the loop binding is a view — document the restructure:
  collect first or keep the loop synchronous), `Shared` read across await
  (error — the no-exemption decision), async fn with `&mut` parameter,
  async closure capturing a view, and sync functions taking views called
  from async contexts (no error — they cannot suspend).
- **Validation before merging each phase:** the std corpus, examples, and
  LSP suites are the false-positive gate. `std::reactive`/`std::ui` lean on
  `for e in &mut` and `Shared` heavily; if a legitimate std pattern trips
  E2/E3, that pattern — not the rule — gets redesigned, or the finding
  comes back here as a semantics question. Treat any such hit as a
  proposal-level event, not something to special-case in the checker.

## 5. Out of scope, recorded elsewhere

- **Subscript absence semantics** (P1): what `a[0]` — read, write, or view
  — means when the element does not exist. Bounds, not aliasing. Backlog
  **I4**.
- **Scalar views don't auto-deref in argument position** (found by P3's
  probe): `print(b)` for `let b = &mut a[0]` prints the `(base, key)` pair
  itself. Transparent references deref reads and writes; a view passed
  where a VALUE is expected (at least for `any`-typed parameters like
  `print`) leaks the representation. Small, separate fix — C5-adjacent.
- **Field-disjoint borrow splitting** (Rust's simultaneous `&mut s.x` /
  `&mut s.y` refinements): not needed — vilan already permits sibling-field
  *writes* under a field view (§2), and multiple simultaneous views remain
  governed by the existing rules.
- **C2's dynamic remainder** (§6).

## 6. What is left of C2

After E2 lands, the un-catchable-statically residue is writes through
**aliased paths**: two handles to the same `Shared` cell, one writing while
a view through the other is live — plus whatever the C4-era native
destruction semantics add. That is honest runtime-check territory
(generation counters on containers, poisoned views), needs a cost model,
and should be sized only after E2/E3 have been in use — the static rules
may leave the dynamic remainder too rare to justify machinery.

## 7. B119 — the gate asks the call graph, not the token

Status: **SHIPPED 2026-08-12** (cycle 19; ruled by the owner the same day).
§3's "gap the pins found, still open" is closed, and it was wider than
filed: all THREE of E3's arms were gated on `saw_await`, so the
implicit-await spelling walked past every one of them. The filing named
the signature rule because that is where the `&`-form probe found it.

### What was actually broken (reproduced before touching anything)

Three programs, all compiling clean on the pre-fix binary, all emitting a
real suspension:

| Arm | Program | Emission |
|---|---|---|
| Signature (§3 rule 2) | `async fun stash(viewed: &mut Point) { let beat = tick(); … }` | `async function stash(viewed) { const beat = await (tick()); … }` |
| Signature, `&` form | `async fun peek(viewed: &Point) { let beat = tick(); … }` | same shape |
| Signature, one hop | `fun hop(): i32 { tick() }` between them | `async function hop() { return await (tick()); }`, then `await (hop())` |
| Body (§3 rule 1) | a `let view = &mut point;` live across `tick()` | `const beat = await (tick());` with `view` live |
| Closure (§3 rule 3) | `async { let beat = tick(); view = beat; }` | the spawned closure awaits with the capture live |

The hop case is the one that settles the shape of the fix: `hop` declares
nothing. A gate keyed on the callee's DECLARATION sees a plain `fun` and
lets it through; only the fixpoint sees that `hop` calls `tick` and is
therefore itself suspending.

### The rule as shipped

> A call gates the view rule when it **can suspend** — when the emission
> `await`s it. The explicit `await` token stays SUFFICIENT; it stops being
> NECESSARY.

"Can suspend" is `Program::suspending_calls`, and E3's three arms read it
unchanged in shape:

- **body rule** — no view may be live across a suspending call;
- **signature rule** — a body containing one may not take view parameters;
- **closure rule** — a closure body containing one may not capture a view.

A body "contains one" when it holds an `await` token or makes a suspending
call. Neither test descends into a nested closure or `async` block, which
is what keeps a spawn's awaits off its creator — the same boundary
`CallGraph::build` already draws, and the reason the graph can answer for
the analyzer's own scan.

### Why this composes with B29, rather than colliding with it

The filing warned that tightening the gate to *declared* asyncness would
break B29's async-impl-of-a-sync-trait freedom, whose pin
(`a_declared_async_impl_of_a_sync_trait_method_is_permitted`) depends on
`async fun m(&self)` with a non-suspending body staying legal. The ruling
asserts both hold. They do, and the reason is a distinction the old gate
had no way to make:

**Declared asyncness is a property of the CALLEE, read at a call site — it
is never read of the body being checked.** A JS `async function` runs
synchronously until its first `await`. So:

- `async fun quiet(viewed: &mut Point) { viewed.x = 5; }` never yields.
  Its caller's `await (quiet(point))` resolves after the body has already
  finished with `viewed`. The signature rule does not fire, and B29's pin
  — which does not even call `m` — is untouched.
- A body that CALLS such an impl is a different question, and the answer
  is yes, it suspends: `await (m(s))` yields to the microtask queue even
  when `m` returns an already-resolved promise, so any turn may run before
  the caller resumes. `calling_an_async_impl_of_a_sync_trait_method_
  suspends_the_caller` pins that, and it is a genuine hazard the old gate
  missed, not a cost of the new one.

The runtime truth, not the declaration, is what both answers are read off
— which is why there is no conflict to trade away.

### Trait dispatch: conservatively suspending, and why that is the right seam

A `T::member()` / re-dispatched trait call cannot be pinned to one callee
before monomorphization, so it counts as suspending when **any** candidate
impl is async. That is an over-approximation, and its false-positive
surface is real and worth naming: a program whose only instantiation
selects a sync impl, while some other impl of the same trait is async, has
its view parameters refused for a suspension its instance never performs.

It is nonetheless the right answer here, because it is the SAME
over-approximation `async_infer` already uses to decide the caller's own
asyncness. Refining the checker below the emitter would mean the analysis
disagreed with the marking derived from it; the two must move together or
neither is trustworthy. No program in the tree hits the surface (the
survey below), so the trade is currently free — and if it ever bites, the
refinement (`dispatch_candidates_for`, platform coloring's per-instance
narrowing) exists and should be applied to both at once.

### Where it lives, and why it is not one pass

`check_invalidation` runs inside `analyze()`; the call graph and the async
fixpoint run after it, in `post_analysis_passes`. The check cannot move to
the facts (it owns view liveness and the S1 frozen filter), and the facts
cannot move to the check (E35: one graph per analysis, built where it is).
So the analyzer RECORDS the candidate sites and
`analyzer::check_view_suspensions` decides them — the `drop_method_checks`
idiom, which exists for exactly this shape.

The suspension answer itself is not new machinery. `async_infer`'s
fixpoint already decided, for every call edge, whether it awaits — that is
what made each caller async — but it decided it as a per-node boolean and
discarded the sites. `call_suspends` is that same per-call test factored
out of `base_fixpoint`, so the checker's answer and the emitter's cannot
drift; one further pass over the same edges records the sites into
`suspending_calls`. There is no second fixpoint, and none was needed.

**Cost.** Interleaved A/B, 80 samples per arm, on the `todo` example's
post-pass phase (median 18.1 ms): **+1.15 ms (+6.1%)** for the
materialization pass as first written, **+0.70 ms (+3.9%)** memoized. The
whole cost was `dispatch_candidates`, which scans every impl and trait for
the member name — the fixpoint calls it only for nodes that have not
flipped yet, the materialization for every indirect edge. Memoizing the
verdict by dispatch key (sound only because `async_set` is final there,
which is why `base_fixpoint` cannot share the memo) halves it. Below the
noise floor of a real compile: 60 corpus programs built through each
binary, 15.10 s vs 15.13 s.

### The survey

Zero newly-flagged sites, on every surface:

| Surface | Extent | Flagged |
|---|---|---|
| std, both layers, FULL scan | every module, process + browser | 0 |
| corpus | 114 programs, byte-identical goldens | 0 |
| docs fences | 110, all compiled by the docs gate | 0 |
| every `.vl` under `vilan/` + `crates/` | 229 files | 0 |
| project roots (examples, benchmarks, `init` templates, CLI fixtures) | 21 `vilan.toml` roots | 0 |
| proposal + docs `vilan` fences, structural | 316 scanned | 0 |

The one fence holding both a view and an `await` is §1's own exhibit of
`mutate_across_await` — an EXPLICIT await, refused since 2026-07-09, and
labelled there as the historical probe. Not a flip.

### Residue

- The `+=` shape reports the closure-capture diagnostic twice (the
  compound re-read is a second `Expr::Local` reference to the same view).
  Pre-existing and symmetric across both gates — the token path does it
  identically — so it is recorded here, not folded in.
- Module-level binding initializers have no entry in `suspending_calls`:
  their calls are not graph `nodes()`, an initializer that suspends is
  refused outright by `async_infer`, and the invalidation scan does not
  walk initializers. Nothing to answer for today; a future initializer
  scan would need the edges added.
