# Type solver — capability characterization (backlog B1)

> **Status: analysis complete, B1 closed** (see the closing section: every row of the
> bug table has a passing pinned test). Kept as the capability map of what the solver
> decides and how; later channels (own-generic ordered values, `bound_dispatch_traits`,
> free-call deferral) are recorded in transport-rpc.md's follow-ups and p6-followups.md.

B1 asks: stand back from the constraint machinery, characterize what the solver
*can and cannot* decide, find the cases it gets wrong, and **merge the special cases
into general code** rather than whack-a-mole each one. This is the synthesis. The
mechanism and the prior refactor live in [`analyzer-refactor.md`](analyzer-refactor.md)
(root causes; items 1–6, with 1–5 v1 shipped) and
[`constraint-queue-plan.md`](constraint-queue-plan.md) (the unified queue; v1 shipped,
v2 the dependency engine, deferred). This doc states the model, isolates the *one*
class of failure that remains, and names the cure — which turns out to be exactly the
two refactors those docs already designed and deferred.

## The model (current)

- **Types** are interned to `TypeId` (`type_id_to_type_map`); a generic parameter is
  `Type::Generic(constraint_id)` keyed by its binder; bindings are a
  `SubstitutionContext = HashMap<TypeId, TypeId>` (generic id → concrete id).
- **Inference** is a worklist fixpoint: one `Constraint` enum (12 kinds), a
  `priority()` order, and `resolve_constraints()` which **runs every constraint each
  pass and re-queues whatever defers, until a quiet pass** — there is *no* dependency
  tracking (v2). `reconcile_type` (now parameter-first) unifies + emits bindings;
  `substitute_type` applies them.
- **Generic dispatch** is recorded once into `generic_dispatch` (which member) +
  `method_call_substitution` (the bindings), keyed by call id.
- **Monomorphization** (transformer) holds `current_substitution` (the active
  bindings) and emits a concrete instance per type-arg set: free calls via
  `get_or_create_instance(generic_argument_ids)`, nested calls via
  `inherited_substitution` (the callee's generics that appear in
  `current_substitution`). Unresolved → `ensure_function_emitted` of the *generic*
  body.

## What it decides well

Direct generic calls; struct-construction inference (bug b); parameter-first
argument reconciliation (bug c); bidirectional closure-parameter inference
(`list.map(|x| x + 1)`); the 11 constraint kinds in their priority order; the
never-overflow guards. The corpus (69) and the inference suite (39) are green.

## The one class that remains: generic bindings don't flow across boundaries

Both deep-reads of the dispatch + inference engines converge on a **single failure
path**. A generic parameter's binding is lost when it must cross an inference or
monomorphization boundary, and the transformer then emits the *generic* body, inside
which dispatch resolves to the **empty abstract trait method** → `undefined` at
runtime. The binding is lost in one of two ways:

- **(A) never recorded.** The constraint that would bind the generic runs *before*
  its input type lands, commits against `Unknown`/`Unresolved`, and is never re-run —
  the fixpoint re-runs *all* constraints each pass but has no notion of "this one
  read a type that just changed." So `method_call_substitution`/`generic_argument_ids`
  is never written, and the transformer has nothing to monomorphize with.

- **(B) recorded but not composed.** The binding is keyed by the *caller's* generic
  id, but the nested callee's body references its *own* (freshly-minted) generic id.
  `inherited_substitution` matches by id, so the callee's generics aren't in
  `current_substitution`, the composition misses, and the callee is emitted generically.

### The recurring bugs are all this class

| repro | which | why |
| --- | --- | --- |
| bug **c′** — `count.derive(\|n\| format(n))` | A | `n` types late (from `derive`'s signature); `format(n)` committed against `Unknown`, not re-run. |
| RPC **#4** — `Ok(Option::from_json(json))` | A | the element type `User` arrives via the `Ok` wrapper + return type, *after* the `from_json` constraint resolved. |
| `List<List<T>>` round-trip ✅ | A/B | the inner container's element binding isn't threaded through the outer `from_json_value`. **Fixed (baacc9c): `resolve_dispatch` monomorphizes the dispatched method for the concrete receiver.** |
| RPC **#3** — object-stub `(self.t).call()` | B | the stub's `<T>` and a routed helper's `<U>` are different ids; `inherited_substitution` can't thread one through the other. |

These are not four bugs. They are one structural leak: **the substitution model is
sound for *direct* binding and leaks across *boundaries*** — late-arriving inputs
(A) and fresh ids in nested scopes (B). The targeted patches for bugs a/b/c shrank the
class; the leak itself is what B1 says to fix generally.

## The cure (already designed, deferred — now gated in)

The prior plan deferred two refactors *and named the gate*: pursue v2 "when an
ordering bug appears that targeted defers can't cover," and item 6 "once items 1–5
land." Both gates are now met — the RPC repros are exactly that ordering/identity
class. In order:

1. **Item 5 v2 — dependency-driven re-queue** (`constraint-queue-plan.md` stage 14).
   Thread the currently-resolving constraint; at the one `infer_type` **read** of an
   `Unresolved`/`Unknown` type record `(constraint ⇽ expr)`; at the one
   `resolved_types`/`expr_id_to_type_id_map` **write** re-queue the dependents; run
   the *dirty* set instead of all-each-pass, with the bounded fixpoint kept as a
   cycle backstop. **Fixes class (A)**: a binding's constraint re-runs the moment its
   late input lands, so it's recorded. This is the structural cure the doc names for
   the ordering class, and the prerequisite that makes item 6 and memoization sound.
   **Leads.**

2. **Item 6 — type interning + stable generic identity.** One stable `TypeId` per
   generic parameter (rather than fresh copies per call/impl), so a binding composes
   across scopes by id and `inherited_substitution`/`substitute_type` stop missing.
   **Fixes class (B).** High blast radius (reworks the in-place-mutation model);
   follows v2, per the existing sequence.

**Item-4 tail — ✅ resolved** (commit 6b96d3f). The duplication was in the transformer:
two near-identical instance emitters (`get_or_create_instance` for free functions, keyed
by positional type args; `emit_method_instance` for methods, keyed by a constraint→type
substitution) plus four call-emission branches that each rebuilt the same lowering. Two
emitters fed by two binding representations is the "recorded in one channel, read in
another" shape. Collapsed to one path: `emit_instance(fn, substitution)` is the single
emitter, and `call_substitution(call, target, args)` is the single place a call's binding
is *read* — positional args (free call), else the analyzer-recorded
`method_call_substitution` (method/operator), else the inherited slice. Corpus
byte-identical (a function's constraint ids are minted in parameter order, so the
sorted-by-constraint key matches the old positional key).

Note: the originally-named pair `generic_dispatch` + `method_call_substitution` is *not*
a redundant channel. `generic_dispatch` selects *which* concrete member an abstract trait
call re-dispatches to (an early-return in the transformer); `method_call_substitution`
drives monomorphization of a concrete generic callee. They are orthogonal, sequential
concerns — co-locating them removes no failure mode, so they are left separate.

## Plan + verification

- v2 is staged per `constraint-queue-plan.md` §Staged migration (scaffold the
  dep-index + the two chokepoints behind today's run-all loop first — **corpus
  byte-identical** — then switch the runner to run-dirty). Every stage gates on the
  corpus and the inference suite; after the run-dirty switch, the `sc_100..800` perf
  benchmark must stay linear (~217/398/788/1547 ms) and a dirty backstop must keep
  cycles from hanging.
- The four repros are pinned as `#[ignore]`d tests in `inference.rs` (the project's
  known-bug convention) — each flips green as the class closes, making progress
  measurable against this doc rather than anecdotal.

## Recommendation

Lead with **item 5 v2 (dependency-driven re-queue)** — the documented next step, the
structural cure for the majority of the repros (the ordering class), and the
prerequisite for item 6. Begin with the scaffolding stage (the dep index + the read /
write chokepoints, *recording* dependencies but still running all-each-pass — provably
corpus-identical), then flip to run-dirty as its own gated stage. Item 6 (stable
generic identity) follows to close class (B).

### Open questions

- **Q1 — v2 scope now, or the targeted composition fix first?** v2 is "the riskiest
  stage." A narrower alternative for class (B) alone: have the transformer *recompute*
  a nested call's substitution by reconciling the callee's parameter types against the
  resolved argument types at emit time (no item 6). Cheaper, transformer-local, but a
  point-fix — against B1's "merge into general code." Lead with v2, or de-risk with
  the point-fix first?
- **Q2 — dep granularity.** Capture deps per `(constraint, expr_id)` (precise, more
  bookkeeping) or per `(constraint, type_id)` (coarser, fewer re-queues)? The
  `constraint-queue-plan.md` chokepoint sketch implies per-expr; confirm before
  building the index.
- **Q3 — measure first?** Before the run-dirty switch, is the all-each-pass fixpoint
  actually a correctness problem (it is — class A) *and* a perf one, or only
  correctness? If only correctness, v2 can keep run-all and *just* add re-queue-on-write
  (re-run a deferred constraint when its input lands) without the full dirty-set
  scheduler — a smaller, safer v2.

## Implementation progress

**Update — class (A) is narrower than "no re-queue."** Tracing the `from_json` repro
(#4) in code refined the diagnosis. It is a **bidirectional-inference** gap, not a
late-binding/re-queue one: the binding *would* be recorded (the function-call arm
already reconciles a call's return type against its expected `constraint`), but the
expected type never reaches the call. Two leaks, both fixable directly:

1. **Constructor propagation — ✅ fixed** (commit pending). `infer_enum_constructor_arguments`
   inferred each argument against the variant's *abstract* declared payload type and
   ignored the expected enum type. It now seeds the enum's parameter bindings from the
   expected type and substitutes the payload before inferring the argument — so
   `Ok(Option::from_json(t))` in a `Result<Option<User>, str>` context types `from_json`
   against `Option<User>`. Verified: the `let`-annotated form round-trips; corpus 69/69.
2. **Return-type-driven body inference — ✅ fixed.** A function's body tail was *not*
   inferred against its declared return type, so `fun g(): Option<User> { Option::from_json(t) }`
   left the binding unrecorded (the abstract decoder). Two pieces, both clean and
   general:
   - A **`ReturnType` constraint** (priority 10, beside `Variable`/the `let`-annotation
     path it mirrors) infers the body tail against the declared return type, so a
     return-position generic call records its binding the way `let v: R = ..` does.
   - An **`expected_types` map**, seeded for the body tail during the walk and
     **propagated through `resolve_match` into each leg body**, carries that expected
     type *through* a `match` (or nested matches) between the call and the signature —
     the RPC-client shape `match .. { "ok" => Ok(Option::from_json(json)) }`. Without
     it the legs were inferred bottom-up (`resolve_match` ran at priority 5, before
     `ReturnType`, and cached the abstract decode).

   Verified: corpus 69/69 byte-identical; `from_json_indirect_element_type_runs` and
   `from_json_return_type_flows_through_match_arm` pin both halves; the RPC example now
   uses the natural `Ok(Option::from_json(json))` directly (quirk #4 retired).

So the `from_json` class is **bidirectional flow**, more contained than the full
dependency re-queue. **All of class (A) is now closed by targeted, general means:**
constructor propagation (#1), return-type body inference incl. through-match (#2), and
— already, before this work — the **late-bound closure-parameter case** (bug c′,
`count.map(|n| format(n))`), fixed by *deferring a call while an argument is an unknown
closure parameter* (the same rule the method-call resolver applies to an unknown
closure receiver; pinned by `format_in_closure_argument`). So **item 5 v2 (dependency
re-queue) no longer has a failing repro to gate it** — its targets are all closed. It
was nonetheless **shipped as a generalization** (replace the all-each-pass loop with one
principled re-queue, per B1's "merge special cases into general code"): a per-resolution
`current_waiting_on` capture, `wake_ready_constraints` re-queuing a deferred constraint
once an input lands, and a run-all backstop that keeps termination — and so the codegen
— identical to run-all by construction (resolution is monotone). Corpus byte-identical,
perf-neutral; details in `constraint-queue-plan.md` stage 14. A maintainability change,
not a bugfix.

**Class (B) / #3 — ✅ fixed, without the full item-6 rework.** Dispatch on a
generic-typed field (`(self.inner).handle(x)`, the RPC client-object form) lowered to
the abstract trait method. The diagnosis pointed at "stable generic identity" (item 6,
a high-blast-radius type-interning rework), but tracing it in the transformer localized
the divergence precisely: the struct field's `T` carried the *struct definition's*
generic id while the call binding was keyed by the *impl/receiver's* id, and
`current_substitution` missed. Two contained root-cause fixes closed it — no global
interning needed:

1. **Field access substitutes the receiver's type arguments** (`resolve_field_accessor`
   matched `Struct(id, _)`, discarding them). `self.inner` now resolves through the
   subject's actual arguments, so it carries the receiver's `T` and the dispatch binding
   composes. This is the id-divergence cure at the one place the two ids meet.
2. **A generic struct initializer doesn't leak an abstract type while deferred.** The
   object stub then exposed a second bug: `let client = Client { transport = t }` (field
   from a variable) grounded `client` as `Client<TraitBound>` because the deferred
   initializer published an unbound type (the type-arg fallback fills with the
   constraint id) that a consumer read before the resolving run. `resolve_struct_initializer`
   no longer publishes while deferred, and `infer_type` returns `Unresolved` for a
   *pending* generic initializer, so the consumer defers until the real arguments land.

Pinned by `generic_field_method_dispatch_runs` and
`generic_field_from_a_variable_dispatches`; the RPC example uses the object stub
directly. The full item-6 type interning is *not* required — the targeted fixes subsume
it. Item 6 remains available only if a future case needs genuinely stable ids that these
local substitutions can't reach.

**The last class-A/B case — the `List<List<T>>` round-trip — is now fixed** (commit
baacc9c). The nested decode `T::from_json_value(element)` inside `List<T>::from_json_value`
lowered to the abstract decoder whenever the method was reached as a *nested* dispatch:
`resolve_dispatch` emitted the callee's *generic* body with the impl's `T` still abstract.
(Single-level worked only because its first-level dispatch goes through the
analyzer-recorded `method_call_substitution`; the inner levels go through
`resolve_dispatch`.) The cure: `resolve_dispatch` binds the impl's generics from the
concrete receiver type (`bind_generics` matches the impl subject `List<Generic(T)>`
against `List<i32>`, recursing through arguments/tuples/closures) and emits a
monomorphized instance via the one `emit_instance` path. Pinned by
`nested_container_from_json_roundtrip_runs` (to triple nesting) and
`mixed_nested_container_from_json_roundtrips`. **B1 is now genuinely fully closed — every
row of the bug table has a passing test.** (Lesson recorded: "closed" needs a pinned test
per case, not suite-green plus one example — the earlier overstatement came from skipping
that.)

## The expectation is an input of generic call resolution — P21 closed (B125, 2026-08-22)

The design lives here rather than in `editing-dx.md` because the question it
answers is a solver one — **which binding source fixes a generic, and in what
order** — and this is the paper that owns class (A), the re-queue, and the
`expected_types` channel; `editing-dx.md` §17.6 records the diagnostics
history and now points here. Lane `b125-solver-ordering`, Order 9 / cycle 27,
off `next` @ 67cd3c57. Code commits `055183be`, `20bf10ed`, `76bbd9e6`.

### The defect, in the solver's own terms

`let widths: List<i32> = points.map(|point| { point.x * 2; })` with
`List::map<U>(self, fn: |T| U): List<U>`:

1. **Walk.** The annotated `let` seeds `expected_types[call] = List<i32>`
   (analyzer.rs, the `Node::Let` arm — the same seed a declared return type's
   tail and a `ret` get). `Constraint::MethodCall` for the call (priority 6);
   `Constraint::Variable` for `widths` (priority 10).
2. **Pass 1, priority 6 — `resolve_method_call`.** The receiver binds the
   impl's `T = Point`. `bind_callee_own_generics(skip_closures)` binds
   nothing: the only argument is the closure. No defer — no non-closure
   argument is unresolved. `infer_closure_args_against_params` infers the
   closure against `|Point| U`; `U` is unbound, so the closure arm's
   substituted target is `Generic(U)`, `type_is_ground` declines it, and the
   body infers bottom-up: `|Point| void`. `bind_callee_own_generics(all)`
   reconciles `|T| U` against that and **commits `U := void`**; the call
   records `{T: Point, U: void}`, wires, and returns `Resolved`.
3. **Priority 9 — `MethodArgCheck`** re-infers the closure against
   `|Point| void`: ground now, and true. Matched.
4. **Priority 10 — `Variable`** infers the call against `List<i32>`; the call
   arm's return-type-only inference reconciles `List<void>` against
   `List<i32>`, fails, binds nothing; the `let`'s own reconcile fails and
   reports `Expected List<i32>, but got List<void> instead.` on the whole
   call. Confirmed live on the v0.35.0 binary (`8d7fe41b`), and for the
   tail, `ret`, free-function (`apply<U>(xs, f: |i32| U): List<U>`) and
   `Signal::map` spellings — same message, same whole-call anchor.

**Why the re-queue cannot wake it.** Nothing deferred. The `MethodCall`
RESOLVED (a committed binding is never revisited — resolution is monotone by
construction, `build()`'s loop comment), and the `Variable` FAILED (reported).
`current_waiting_on` is captured only for a constraint that returns
`Deferred`; item 5 v2 re-runs work that waited, and this work never waited.
P21 is not a re-queue shape at all.

**The fact the papers missed.** The expectation was available the whole
time. `expected_types[call]` is written AT WALK TIME for every shape that
carries one — the annotated `let`, the declared return type's tail, the
`ret` — so it is already in the map when the call resolves at priority 6 of
the first pass. The call resolver read it for exactly one thing, B73 R2's
home selection (`select_home_by_expected_type`), and never for its own
generic binding. "The call's own generic-parameter binding is a downstream
CONSEQUENCE of the argument inference that already ran" (§17.6) was true of
the *closure's* contribution; it was never true of the *expectation's*.

### (a) against (b), with the spike's answer

- **(b) as framed** — a deferred constraint re-triggering the closure's
  return-position check once its target becomes ground, woken by the call's
  return reconcile — **cannot close P21.** By the time the `let`'s reconcile
  runs, `U` is committed to `void`; the reconcile `List<void>` against
  `List<i32>` fails outright and binds nothing, so there is no newly-bound
  `U` for a second check to re-target. A constraint waiting on "`U` is
  ground" fires on `U := void` — which IS ground — and finds a body that
  matches it. The only way (b) works is if the call declined to commit `U`
  from a closure's return while an expectation might still arrive, and there
  is no signal that one never will: an unannotated `let widths = points.map(
  |point| { point.x * 2; })` legitimately has none and legitimately types
  `List<void>` (pinned: `b125_an_unannotated_let_keeps_the_bottom_up_binding`).
  Such a defer would wait forever and the run-all backstop would commit it
  anyway. The orchestrator's prior is refuted by the mechanism, not by
  taste.
- **(a) as framed** — reorder generic method-call resolution so a
  context-known return binds before the closure arguments — assumed the
  expectation arrives *later* and the call must wait for it. It does not.
  What is needed is not a reordering of constraints but a **third binding
  source inside the existing two-phase resolver**, in a fixed place in its
  precedence:

  > receiver → non-closure arguments → **expectation** → closure returns

  `bind_callee_own_generics_from_expectation(call_id, callee_id,
  &mut substitution)`: read `expected_types[call_id]` (skip
  `Unknown`/`Unresolved`/`Any`); take the callee's own generics still open
  after the receiver and the non-closure pass; substitute the callee's
  DECLARED return type with what is bound so far; reconcile it against the
  expectation; insert the bindings for the open generics that appear in the
  declared return type — filtered exactly as the call arm's return-type-only
  inference filters (not the enclosing binder's generic, B58; not a generic
  "inferred" to be itself, B102) and additionally refusing a binding with an
  `Unknown`/`Unresolved` hole anywhere inside it (`type_has_hole`: a
  still-open expectation is not evidence). Called at both call paths — the
  method path after its B90 defer and before `infer_closure_args_against_
  params`, the free-function path after its hoisted non-closure pass and
  before its positional loop — because B90 made the two paths one rule and a
  fix on one side would have re-opened the split.

  **Why argument-first, expectation-second.** An argument is a value the
  user wrote; the expectation is where the result goes. `let s: str =
  xs.fold(0, |acc, x| acc + x)` binds `B = i32` from the literal, the
  closure types against it, and the `let` reports `Expected str, but got i32`
  at the call — as before, one diagnostic, and the right one: the literal is
  the nearest evidence (pinned:
  `b125_an_argument_bound_generic_outranks_the_expectation`). Letting the
  expectation win would type the literal through the annotation — a
  different, wider change (literal typing by expectation) this lane does not
  make.

- **The S3 blast radius, re-read.** §16's four broken iterator tests came
  from routing a NON-ground target through `check_return_position`, which
  swallowed the binding the closure's own return would have produced
  (`Iterator::from_fn`'s `Option<T>`). This change never routes a non-ground
  target; it binds first and lets the gate decide. Measured rather than
  argued: the inference suite (2396 passed, 0 failed, 1 ignored — every
  iterator pin among them), the corpus byte-identical (7 passed), docs (8),
  benchmarks (1), the full suite (§ below).

### Who reports — the B5 question, answered and pinned

Exactly one diagnostic, at the closure:

| the disagreement | before (v0.35.0) | after |
|---|---|---|
| annotation `List<i32>`, block tail `{ point.x * 2; }` | whole call, `Expected List<i32>, but got List<void> instead.` | the closing `}`: `` Expected i32, but got void instead: the `;` discards this body's last value. `` |
| annotation `List<str>`, block tail `{ point.x * 2 }` | whole call, `…got List<i32>` | the closing `}`: `Expected str, but got i32 instead.` |
| annotation `List<str>`, bare `\|point\| point.x * 2` | whole call, `…got List<i32>` | the closure: `Expected \|Point\| str, but got \|Point\| i32 instead.` |
| parameter `\|point: str\|` vs receiver, annotation `List<i32>` | the closure, `Expected \|Point\| i32, but got \|str\| i32 instead.` | unchanged (P27's whole-value anchor) |
| all three disagree (`List<str>`, `\|point: i32\| { point; }`) | the closure, `Expected \|Point\| str, but got \|i32\| void instead.` | unchanged |
| `fold(0, ..)` under `let s: str` | the call, `Expected str, but got i32 instead.` | unchanged |

The `let`'s value-position reconcile never doubles the closure's report
because S3's rule carries it: on a mismatch the closure's REPORTED type is
the target it was held to (`|Point| i32`), so the call types as `List<i32>`,
the annotation agrees, and the `let` has nothing to add. The `MethodArgCheck`
at priority 9 re-infers the closure against the same ground target and hits
the closure arm's span+message dedup. Eight programs pinned
(`b125_*` in `tests/inference.rs`), each asserting the count with
`assert_fails_once_with` and the absence of the old whole-call message with
`assert_fails_without`.

**One anchor residual, deliberately left.** The bare-expression closure
(`|point| point.x * 2`) reports as a whole value at the argument check,
because S3 scoped the return-position route to block bodies ("no closing
brace to anchor at"). The narrower anchor — the expression itself, `Expected
str, but got i32` — is a later slice's refinement of S3, not this lane's.

### The role of `type_is_ground` after the change

Unchanged, and still load-bearing. It is the "don't freeze unbound" guard:
a target nobody has bound must not be routed through the return-position
check, or the closure's own return could never bind it (I5/B19;
`an_unannotated_next_that_yields_an_option_stays_legal` and its neighbours
pin this). The expectation binding is what makes the target ground *when the
program supplies an expectation*; when it does not — an unannotated `let`,
or an expectation that names the enclosing function's generic (`fun
ident<T>(xs: List<T>): List<T> { xs.map(|x| x) }` binds `U = T`, abstract)
— the gate declines exactly as before and the body binds bottom-up. Both
directions are pinned. There is no "second chance" constraint: the first
chance now has the information.

### Two supporting pieces the spike needed

1. **The regime-1/1' wording is decided later, not guessed.** The closure
   is first inferred by the very call that just filled its parameters, while
   the body's constraints on those parameters — `point.x`, a field accessor
   deferred on the unknown parameter with an empty wait set — are still
   pending. `missing_return_value_message` read the last statement as
   `Unresolved` and fell back to "this body ends without producing a value"
   for good, because the dedup then refused the corrected wording. The
   closure arm now says "not yet" (`Type::Unresolved`) in exactly that case:
   a `Mismatched` verdict on the synthesized-void tail whose last statement
   is pending and is not an error node. The owning call's argument check (or
   the `let`) re-infers the closure after the backstop has resolved the
   accessor, and reports the `;` wording. The first version of this fired
   before the tail had even been checked and broke
   `an_async_annotated_let_awaits_at_its_calls` (`{ tick(); 11 }` — the
   statement is a call wired at priority 11); scoping it to the verdict
   fixed that.
2. **`resolve_variable` defers on a directed inference that is not ready.**
   Its readiness probe infers the value UNDIRECTED; the directed inference
   that follows can now say "not yet" where the undirected one did not, and
   the constraint reported the non-type `unresolved` against the annotation.
   It defers, as its reassignment loop already did.

### The nested shapes — closed too (`76bbd9e6`)

The expectation binding lands only if the call's entry is in
`expected_types` when the call resolves. The three sources seeded only the
VALUE's id; a call in a block tail or a value-`if`'s branch tail was seeded
by the `Block`/`If` arms of `infer_type_path` — during inference, at priority
10, a pass after the call had committed. A match leg was seeded by
`resolve_match` at priority 5, early enough unless the match's SUBJECT was
itself a call (`match points.len() { .. }`), in which case the match
deferred past the leg's call. `seed_tail_expectations` walks the syntactic
tails (block tail, value-`if` branch tails, recursively) at the three seed
sites, and `resolve_match` seeds its legs before its subject can defer the
attempt. The final contents of `expected_types` are unchanged — the same
entries, a pass earlier — so the post-solve readers (the literal-range
check) see what they saw. Three pins, plant-proven.

### What B129's second gap actually was

`b129_a_map_on_a_let_bound_signal_types_its_closure_parameter` was filed as
this family — "a `.map` on a let-bound signal freezes its closure parameter
before the receiver's binding lands". Probed on the v0.35.0 binary, it is
neither P21's mechanism nor about the `let`:

| program | v0.35.0 |
|---|---|
| the pin (`let items = Signal::new([Todo{..}])`, `.map(\|list\| { for todo in list { .. } })`) | `cannot access field 'done' on type any` |
| the same, inlined (`Signal::new(..).map(..)`) — the pin's comment said this worked | fails identically |
| the same on `List`: `let items = [[Todo{..}]]` | fails identically |
| `let items: List<List<Todo>> = [[..]]` (annotated receiver) | compiles |
| `[[1, 2], [3]]` with `total += n` | "compiles" — with `n: any` |
| `.map(\|list\| list[0].id)` | `cannot index unknown` |

The closure parameter is filled fine once `.map` resolves. What fails is the
body's `for todo in list`: `ForEachItem` sits at priority 8; the `.map` call
deferred to the NEXT pass because its receiver had not grounded (an
un-annotated `let`, or a receiver that is itself a call); and
`resolve_for_each_item` committed the item to `any` on sight of an `Unknown`
iterable — `Unknown` is not `Unresolved`, so nothing deferred. The annotated
receiver "worked" only because the call then resolved at priority 6 of the
first pass, ahead of the loop. `resolve_subscript` had the same hole
(`cannot index unknown` on the first pass). The field-accessor, method-call,
call-subject and match resolvers all already defer on an unknown closure
parameter (C′'s family, B23); the two that did not now do. The consequence
at the bound: a closure NO call ever fills (`let walk = |xs| { for x in xs {
print(x); } }`, never called) used to compile clean with `x: any` and now
reports through the leftover sweep — `type of function call arguments could
not be resolved` at `print(x)` — exactly as `for x in List::new()` on a
never-pushed list always has. Pinned as the consistency claim; the sweep's
wording is the sweep's (owner question below).

### Gates and numbers

- `cargo test -p vilan-core --test inference`: 2396 passed, 0 failed,
  1 ignored (the one remaining ignore is unrelated to this family).
  Iterator set included, all green.
- `cargo test -p vilan-cli --test corpus`: 7 passed — every golden
  byte-identical (diagnostics do not change emitted JS, and the expectation
  binding records the same `method_call_substitution` a successful program
  already recorded through the `let`'s reconcile).
- docs 8 passed; benchmarks 1 passed.
- Perf, `perf-baseline.md` §3's command (release, in-repo subjects), `next`
  @ 67cd3c57 against the branch on the same machine, **alternating, two
  rounds** (`next`, branch, `next`, branch), medians as ratios
  branch/`next`: `std_wide` cold analyze 0.960 / 1.000, warm analyze
  1.005 / 0.991, cold total 0.959 / 1.003; `tiny` cold analyze 0.952 /
  0.995, warm analyze 1.007 / 1.021; `vilan check` of the reference
  package (end to end) 1.054 in the first session; the LSP synthetic
  keystroke p50 1.080 / 1.077 (8.47 → 9.15 ms), the one row with a
  consistent sign. Within noise by the paper's own reading — and the
  cautionary tale is the FIRST session, run once each, straight after a
  release build: it read `std_wide` analyze at 1.39× cold and 1.43× warm
  alongside `post_passes` at 1.85×, a phase this change does not touch,
  with every absolute ~25 % above the settled rounds. One run each is not a
  measurement on this machine. Deterministically (a temporary pass counter
  in `build()`'s loop, not committed): on the `std_wide` subject every std
  build has the SAME pass and backstop counts on both trees (18/7, 12/4,
  12/4, 12/4, 27/9, 3/2) and the branch makes 1–3 FEWER constraint attempts
  (3292 → 3291, 8546 → 8543); the todo example likewise (pass counts
  identical across its nine builds; 8020 → 8019, 9155 → 9152 attempts).
  The only increases are on the probe programs the fix targets, which now
  resolve instead of erroring: the B129 pin's entry build 7 → 8 passes,
  46 → 50 attempts; the nested-list form 5 → 6 passes, 14 → 16. The
  expectation binding itself is a `HashMap` lookup on every call without an
  expectation and a reconcile on the few with an open own generic.
- Plant-proofs: expectation binding disabled → 8 of the 17 tests the
  `missing_return_value_regime_3` / `b125` filter selects go red (every
  P21-family pin among them); for-each/subscript defers disabled → 5 of 5
  B129 pins red; tail seeding + match hoist disabled → 3 of 3 nested pins
  red.
- `examples/todo/src/todos.vl` drops its last annotation (the comment that
  named B125/P21 as the reason goes with it); the example builds from its
  tracked files.

### Owner questions

1. The never-called closure's untyped parameter now reports (where iterating
   it used to compile with `any`) — but through the leftover sweep's
   `type of function call arguments could not be resolved` at the USE, not
   at the parameter. B13's rule ("inferred from the closure's first call")
   has no call to point at here; a dedicated message at the parameter
   ("`xs` is never typed: annotate it or call the closure") is a diagnostics
   item, not made here. File it?
2. The bare-expression closure's anchor (the whole closure at the argument
   check, see the table) — refine S3's route to anchor at the expression?
3. CHANGELOG family: filed as `diagnostics` per the lane brief; the
   never-called-closure consequence is, strictly, a program that compiled
   and now does not. If that reads as `breaking` to you, the entry moves.
