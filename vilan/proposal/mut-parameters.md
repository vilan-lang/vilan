# `mut` parameters

Status: SHIPPED 2026-08-03 (backlog H9; tester report). Semantics settled
by desugar; §6 records the implementation shape and two discoveries.

## 1. The gap

`mut` bindings (`mut v = 1;`) and `mut` patterns (`Some(mut x)`) exist;
parameters have no `mut` form — neither `fun f(mut x: i32)` nor
`|mut v| { ... }` parses (the convention grammar is `"own" | "&" ["mut"]`
only). The workaround is a noise line, `|temp| { mut v = temp; ... }`,
which is exactly what the field case hit: mutating a `Signal<List<T>>`
via `set_with(|mut list| { list.push(5); list })`.

The diagnostic compounds it: assigning through a plain parameter says
"declare it `&mut x` to allow mutation" — steering a local-mutation want
toward a signature and caller-contract change.

## 2. Semantics — one desugar

```vilan
fun f(mut x: T) { body }    ≡    fun f(x': T) { mut x = x'; body }
```

`mut` on a parameter makes the callee's binding mutable: the body may
rebind it and write its fields. Under value semantics the parameter is
already the callee's own copy, so nothing is caller-visible — `mut` is
purely local, spelled at the binder like every other mutable binding.

Everything below follows from the desugar:

- **Positions**: `fun` parameters, closure parameters (with or without a
  type annotation), and `self` (`fun with_x(mut self): Self { self.x = 5;
  self }` — the builder idiom).
- **Not a convention**: `mut` never combines with `own`, `&`, or `&mut`.
  `own` transfers, views alias the caller — in both, "which thing is
  mutable, the binding or the target?" stops being one question. Combining
  is a parse-adjacent error with its own message. (If `own mut` grows a
  real use, revisit; the desugar line remains its workaround.)
- **Not part of the signature**: mutability of a local copy is an
  implementation detail. Trait-conformance signature checking (B29)
  ignores it; a trait method signature and its impl may disagree on `mut`
  freely. `external fun` parameters take no `mut` (there is no body to
  mutate in); reject with a message rather than silently accepting.
- **No codegen**: emitted JS parameters are plain identifiers and JS
  parameters are mutable; only the analyzer's write-gate changes.

## 3. Grammar

```
parameter    = [ "mut" | convention ] binder [ ":" type ]
convention   = "own" | "&" [ "mut" ]
```

(spec `grammar.md` — the parameter rule and its closure twin; `mut` and a
convention are alternatives, not composable.)

## 4. Diagnostics

The write-through-parameter error becomes two-option steering:

> cannot mutate immutable `x`; declare it `mut x` to mutate this
> function's copy, or `&mut x` to mutate the caller's value.

The binding form's message ("declare it `mut`") is already right and does
not change.

## 5. Pinned cases (per CLAUDE.md: per case, not per example)

Compile-and-run:
1. `fun` single `mut` parameter — rebinding.
2. `fun` mixed list (`mut` beside plain) — the plain one still rejects
   writes with the NEW two-option message.
3. Field write through a `mut` parameter (struct copy mutation).
4. Closure `|mut v|` unannotated — the field case's shape.
5. Closure `|mut v: T|` annotated.
6. `mut self` — the builder idiom returns the mutated copy.
7. Caller-invisibility: caller's value unchanged after the callee mutates
   its `mut` copy (run-asserted).

Rejections:
8. `mut own x`, `mut &x`, `& mut` + binder-`mut` combinations.
9. `external fun f(mut x: i32)`.
10. Write through a PLAIN parameter still rejects (message updated).

Formatter: `mut` prints back in fun and closure positions, idempotent.
Docs: the tour's functions/bindings coverage gains the form; spec
`grammar.md` + `names.md` updated in the same commit.

## 6. Ship record (2026-08-03)

All of §5's cases are pinned green in `inference.rs` (plus a corpus
fixture, `vilan/test/mut-parameters.vl`, and the formatter roundtrip);
the three mechanisms below were each planted red and restored.

**The desugar DEFINES the semantics; the implementation realizes it in
three parts rather than literally minting a binding.** A literal
synthetic `mut x = x'` binding was built first and reverted: closure
member-resolution deferral (the C′ family) wakes PARAMETERS when a call
site lands an unannotated closure's type — a binding fed from the
parameter never re-wakes, so `set_with(|mut list| …)` (the filing case)
typed as unknown. The shipped shape keeps the parameter as the one
entity:

- `readonly_root` treats a `mut` bare parameter as writable, and the
  immutability advice (hoisted to one helper, deduping three copies)
  offers BOTH spellings for a plain parameter: `mut x` (this function's
  copy) or `&mut x` (the caller's value).
- Rule 1's copy: `compute_parameter_entry_clones` marks aggregate `mut`
  parameters; the transformer emits `x = __clone(x)` first in the body
  (functions AND closures — body entry, not `own`'s call site, because
  closures and dispatched callees have no resolvable call site; the
  cost is `own`'s last-read elision, which never applied to a copy the
  caller keeps anyway).
- Scalar views: `compute_boxed_locals` boxes a viewed `mut` parameter
  like any `mut` local; the transformer re-boxes at body entry
  (`x = [x]`) so `&mut x` views write through a real cell.

Two discoveries, recorded as pins:

- **A pre-existing closure-deferral gap** (ignored pin
  `a_closure_mut_parameter_types_from_a_declared_closure_argument`): a
  binding initialized from a closure parameter typed via a plain
  function's declared closure type stays unknown — the HAND-WRITTEN
  `mut x = v;` form fails identically on v0.23.5, so the desugar
  reproduces today's behavior faithfully. `set_with`-style generic
  instantiation types fine and is pinned green.
- **A pre-existing rule-1 hole, filed as B53 — FIXED same day** (pin
  un-ignored, four siblings added): pattern captures aliased their
  source in every form — destructure, match, `is`, and out through a
  RETURN (`unwrap` leaked its payload). Captures now clone at
  `compile_pattern` under a place-gated analyzer set with a share
  elision (read-only walkers stay linear) and a move elision (dead
  `?`-lift temps donate). See the backlog B53 entry for the full record.
  A second pass the same day closed five findings an adversarial review
  raised against that fix — the `is`/guarded-leg compilation path, the
  generic copy that deep-copied resources, the two elisions composing
  unsoundly, a seam scan that missed braced and conditional tails, and
  the `mut [a, b]` grammar divergence: `proposal/capture-clones.md`.
