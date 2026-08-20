# Async polymorphism: adaptation, `sync` contracts, scopes, and the parallelism spine

**Status: Part A SHIPPED 2026-07-17** (four slices: `sync` marker 3b5e1db,
std audit 5fb9eb8, adaptation + snapshot 176fe8a, docs — spec §7.4 rewritten).
Deltas from the design, all recorded in place: the snapshot is implemented as
shallow-copy iteration inside async adapted instances (sound because element
aliasing doesn't exist under value semantics — A.5); dual lowering (A.6)
collapsed to nothing because the List combinators are vilan source, not
intrinsics; `settle_all` is not yet minted (the two-`map` idiom works today —
open question stands). **Part B is the seed of the J1 execution-model phases;
Part C is a design record, explicitly not v1.**

Decisions in this document were made 2026-07-17 (backlog J2's last open
channel). The headline calls are the user's: adaptation is the default and
sequential; the sync-contract marker is spelled **`sync`**; concurrency is an
opt-in idiom over spawn; void positions keep spawn semantics.

---

## 0. Where this sits

Vilan's async model is *inferred coloring*: a function is async because its
body awaits or because it calls something async; calls to async functions are
implicitly awaited; return types stay plain values. For closure **values**
(no fixed callee) asyncness rides the type — `async |T| U`, accepted on
parameters, `let` annotations, struct fields, and function return types — and
unannotated bindings adopt asyncness from what they hold. A divergence check
refuses an async closure flowing into a plain value-returning position at
every boundary (parameter, field, declared return). All of that is shipped
(J2, closed 2026-07-17).

The one remaining refusal is the useful one this proposal removes:

```vilan
fun fetch_id(url: str): i32 {
    sleep(1);              // async — inferred
    url.len()
}

let ids = urls.map(|url| fetch_id(url));
//        ^^^ error: `fn` receives an async closure, but its type awaits nothing
```

No higher-order function accepts an async closure today. The refusal is
sound — `map`'s body doesn't await — but the fix should not be a colored API
(`map_async`) or a blanket `async` parameter (which would color every sync
call site in every program).

Survey conclusion (recorded so it isn't re-litigated): Go's model on a JS
host *is* this model (which calls can suspend must be decided statically;
Go-on-JS pays a scheduler to discover it at runtime); Pony's capabilities
would replace vilan's memory model rather than extend it; Rust's explicit
futures are the infection this language exists to avoid, and Rust's stalled
"keyword generics" work marks effect-polymorphic HOFs as the hard kernel —
which vilan's whole-program monomorphization (no `dyn`, no fn pointers)
makes uniquely cheap; Gleam's per-target split (BEAM processes vs JS
promises) is the fragmentation to avoid. What the current model lacks is not
a different coloring story but the structured layer above it (Part B) and a
sendability rule shared with future parallelism (Part C).

---

## Part A — Monomorphize-by-asyncness ("adaptation")

### A.1 The rule

A plain, value-returning closure parameter is **asyncness-polymorphic**.
Each call site instantiates the function with the actual asyncness of its
closure arguments, exactly as it already instantiates by type arguments:

- **Async instance** (an argument closure is async): every call through that
  parameter is awaited; the instance itself colors async; its callers await
  it and color accordingly. Emission is a distinct monomorphized instance.
- **Sync instance** (all closure arguments sync): byte-identical to today.
  No awaits, no coloring, native lowerings preserved.

The instantiation key gains a per-closure-parameter asyncness bit beside the
type substitution. Precedent: platform requirements are already computed
per-instantiation (platform-coloring, 8772aef); asyncness is a second effect
axis on the same machinery.

**The sequential contract.** An adapted call awaits each callback before the
next begins. `array.map` over 100 elements whose callback takes 1 second
takes 100 seconds. Effects are ordered exactly as the sync version orders
them; adaptation never introduces interleaving between elements beyond what
each await itself admits. Concurrency is opt-in (A.7).

### A.2 The `sync` marker — a synchronous contract

Some callbacks are part of a synchronous protocol: reactive recomputation,
comparators, `turn` bodies. Adaptation there would break invariants at a
distance (the reactive graph's propagation is synchronous by design —
glitch-freedom, drain affinity). The author opts out per parameter:

```vilan
fun map<U>(self, f: sync |T| U): Signal<U>      // Signal::map — recompute is sync
fun turn(policy: FlushPolicy, body: sync || )    // turn_async is the async flavor
```

`sync |T| U` means: *this closure's completion is part of my synchronous
protocol; its call is used as-is and never awaited.* Passing an async
closure to a `sync` position is a compile error. The message names the why
and the steer, per the diagnostics standard (B4/B6):

> `f` requires a synchronous closure (`sync |T| U`) — recomputation is part
> of the reactive graph's synchronous protocol. For async work, use
> `turn_async` / `Draft` / `optimistic`.

(The steer text is per-site; std's `sync` positions each get a wording pass.)

Parameter positions therefore have three states:

| declaration      | async argument                     | sync argument |
| ---------------- | ---------------------------------- | ------------- |
| `\|T\| U` (plain)  | adapts (async instance, awaited)   | sync instance |
| `sync \|T\| U`     | **error** (sync contract)          | as-is         |
| `async \|T\| U`    | awaited (declared channel)         | awaited (no-op await) |

`async`-marked parameters are *not* polymorphic — they force the async
instance regardless of the argument, for closures whose provenance adoption
cannot see. `sync` is only meaningful on parameters: fields and returns
already have a two-state story (plain = refuses async stores via the
divergence check; `async` = awaited channel) and do not adapt.

Grammar: `sync` is a **contextual keyword** in closure-type position (like
`context`): it lexes as an identifier and only means the contract directly
before a closure type, so `sync`-named values stay legal.

### A.3 Void positions keep spawn semantics

Adaptation applies to **value-returning** closures only. An async closure
into a plain *void*-returning parameter stays what it is today: legal,
spawned, fire-and-forget — UI handlers and turn bodies ride this, and the
`turn` / `turn_async` distinction stays deliberate. So the full rule is:

- non-void plain parameter: adapts;
- void plain parameter: spawns (unchanged);
- `sync` parameter: refuses async arguments (any return type);
- `async` parameter: awaits (unchanged).

This preserves every existing program's semantics: today's legal programs
only ever put async closures where they spawn or where the channel is
declared; the newly-legal programs are exactly the ones that were refused.

### A.4 v1 exclusions (recorded, not solved)

- **Adaptation covers closures the body *calls*.** A body that stores a
  parameter closure into a field, returns it, or writes it into any typed
  position uses the existing rules (the field/return divergence checks catch
  lies). `fun compose(f, g): |A| C { |a| g(f(a)) }` with an async `f` stays
  an error at the return — the returned closure's asyncness *depends on* the
  parameter's, which is an effect variable connecting two positions (the
  full effect-row problem). v2 horizon at most; `compose` is rare.

  > **Corrected 2026-08-04 — the parenthesis was false, and `compose`
  > compiled.** See A.4b: the field/return divergence checks ran outside any
  > instance context, so they never saw a plain parameter that went async at
  > one call site. The *exclusion* stands and is now enforced; what was wrong
  > was the claim that the existing checks already enforced it.
- **Transitive adaptation is NOT excluded**: passing the parameter onward as
  an argument to another adaptive function is a call-position flow —
  `fun helper<T,U>(xs: List<T>, f: |T| U) { xs.map(f) }` instantiates
  `helper` async-in-`f`, which instantiates `map` async. The bit rides the
  instantiation chain; only *escape* into storage/returns is excluded.
- **Externs are implicitly `sync`** for value-returning closure parameters
  (host code cannot await a vilan closure); void extern callbacks keep spawn
  (a `setTimeout` handler that awaits is a spawn, as today).

  > **Narrowed 2026-08-19 (E68)** — the rule's domain is *host boundaries*,
  > and `Context::run` is not one: it is `external` only as a type-checking
  > fiction (context.vl's own header says so), and the context threading
  > pass erases every `run` call it accepts before this check runs. The
  > only way the check could ever see a `run` call was the error path —
  > `thread_contexts` refusing its rewrite after reporting a coverage or
  > `run`-shape error — where judging std's async-into-`run` bodies
  > (task.vl's nursery, rpc.vl's wire turn) produced spurious host-await
  > secondaries anchored in std beside the real primary: ANY `owner_scope`
  > coverage failure, even a bare `Signal::effect` at the top of `main`,
  > carried them. Both arms of the check (the direct host-boundary loop and
  > the transitive `extern_violations_at`) now skip the intrinsic by its
  > recorded id (`Program::context_run_fn_id`). This is not a demotion —
  > the premise is false for the intrinsic in every reachable state, since
  > a surviving `run` call implies the context pass already refused and
  > said why. Real externs are unaffected. Pins:
  > `e68_an_uncovered_effect_reports_only_the_coverage_primary`,
  > `e68_a_refused_run_shape_reports_only_the_context_primaries`,
  > `e68_a_generic_forward_into_run_does_not_cascade_transitively` (all
  > three verified red pre-fix); the standing extern-misuse pins
  > (`an_async_closure_into_an_extern_callback_is_refused`,
  > `an_async_parameter_cannot_launder_into_a_host_callback`) hold the
  > rule's real domain.
- **Container elements**: `List<|| T>` element types accept no markers (J2
  record) and calls through elements do not adapt; unchanged, future work.

### A.4b The adapted-instance escape — enforced 2026-08-04

**Status: SHIPPED.** A.4's first bullet excluded *escape* from adaptation and
justified leaving it unenforced with a parenthesis: "the field/return
divergence checks catch lies". They did not. This is what was actually true,
and what now is.

#### The exclusion's original reason, and the ruling on it

Two reasons are on the record, and they are not the same kind of claim:

1. **A scope-cutting reason, about `compose`.** The returned closure's
   asyncness *depends on* the parameter's — an effect variable connecting two
   positions, the full effect-row problem. **This still holds.** Nothing here
   attempts an effect row, and `compose` stays refused, which is exactly what
   A.4 said would happen.
2. **A factual claim, about everything else**: that a body storing or
   returning a parameter closure "uses the existing rules", because the
   field/return divergence checks catch it. **This was false**, and it was the
   whole load-bearing half — it is why nothing was built.

Both divergence checks call the value oracle with `no_flags` and empty bits:

```rust
if !value_async_in(program, &held_values, &async_set, &no_flags, &[], value_id) {
    continue;
}
```

That is deliberately *outside* any instance context, so the oracle can only
answer from the **global** channels — a declared `async` parameter, an `async`
field, an async closure literal, an adopted binding. A PLAIN parameter is
async only *in an instance*, and there is no instance here. The existing
laundering pins (`an_async_parameter_cannot_launder_into_a_plain_field` and
its neighbours) all source their async value from a **declared** `async`
parameter, which lands in `async_values` — a global channel. The plain
adaptive twin had no pin and no diagnostic.

#### What actually went wrong — measured, not predicted

Three programs, each compiling clean before this change:

| shape | before |
|---|---|
| `fun install(f: \|\| i32): Holder { Holder { hook = f } }`, called with an async closure, then `(holder.hook)()` | prints `Promise { <pending> }` |
| the same via field assignment (`h.hook = f`) | prints `Promise { <pending> }` |
| `fun pass(f: \|\| i32): \|\| i32 { f }`, then `got()` | prints `Promise { <pending> }` |
| A.4's own `compose` example | prints `Promise { <pending> }` — **not** the error A.4 claimed |

It is worse than a wrong print. The store leaves a promise in a slot typed
`i32`, and the type is what everything downstream trusts:

```
let n = (holder.hook)();   // n: i32, actually a Promise
print(n + 1);              // "[object Promise]1"
```

Integer addition became string concatenation, silently, from a clean compile.
The emitted JS says it plainly: the body is `async function $e(f) { … return [
f ]; }` — the instance IS adapted, its own `f()` is awaited — and the later
call is bare `holder[0]()`, unawaited, because the field's type is plain.

#### The fix: the same checks, given the context they were missing

Not a new check class. The two divergence checks now also run **per adapted
instance**, with that instance's bits, in the worklist's final pass beside
`sync_violations_at` / `extern_violations_at` / `dispatch_refusals_at` — the
established family for "async only through this instance's bits". The
positions they refuse are collected once, by two shared helpers
(`plain_closure_field_stores`, `plain_closure_return_sites`) that the global
checks now use too, so the global and per-instance halves cannot drift about
what a refusable position is.

Every exemption is inherited verbatim rather than restated:

- a declared `async || T` field or return is the fix, and is skipped;
- a **void** position keeps spawn semantics (A.3) — and in the per-instance
  path it cannot arise at all, since a void-returning parameter is not
  adaptive and never carries a bit;
- the **direct** case (async without the bits) belongs to the global check and
  is skipped here, exactly as `sync_violations_at` leaves it.

The diagnostic follows the family's voice: primary at the **call that made the
parameter async** (the instance's recorded origin), with a note at the escape:

> this call passes an async closure that reaches `Holder`'s field `hook`,
> which awaits nothing — a later call through the field would hand back a
> promise; declare the field `async || T` (or return void for spawn semantics)
>
> *note, at the store:* stored into the plain field `hook` here

Scanning every escape position per instance is sound because the report is
gated on "async **through** these bits and not without them": the bits are the
callee's own parameter ids and the flags are the component's own members, so a
position outside the instance cannot pass both halves.

> **Adopted beyond this family (E74, 2026-08-20)** — this origin discipline
> (primary at the earliest user-written originating call, least entity id;
> the internal frame demoted to the cross-source C3 note) is now also how
> the context pass anchors its `owner_scope` coverage refusal when the
> strict read sits in std: `effect`/`map`/`or` all funnel to `get_owner`'s
> read in `reactive.vl`, which used to be the primary span. `context.rs`'s
> `user_entry_of` walks the strictness edges back through unbound callers
> only — a covered call is never blamed — and anchors at the user's call,
> with "the read is inside `get_owner` here" as the note. One mechanism,
> two passes; diagnostics-ledger.md rows 222/223 carry the verdicts.

#### The precision residual, named

This is the **conservative sound** ruling, and it is the one A.4 already
chose: a now-async value reaching a plain field or a plain declared return is
an **error naming the adaptation**, not a per-instance asyncness for the field
or the return. Making the escape *work* means the value's asyncness travels
into a struct field — i.e. monomorphizing the STRUCT (and every read of it,
program-wide) by asyncness, which is the whole-program effect-row problem A.4
declined and this does not attempt.

So a program that would be perfectly sound under an effect row is refused
here, and the steer is a real one: declare the field or the return
`async || T`. That is a *precision* residual, never a soundness one — the
refusal is conservative in the safe direction. `compose` is the named member
of the refused set, and A.4 already ruled it rare and v2-horizon.

#### Compat, and the pins

**Nothing in tree relied on the escape.** `--test inference` (1509),
`--test docs` (every fenced example), `-p vilan-cli --test corpus` (byte-identical
goldens, none regenerated) and `--test examples` are all green. std's closure
fields are either void (`Server.on_start`), declared `async`
(`Server.request_handler`), or not bare closure types (`upgrade_handler:
Option<..>`), so none is a refusable position.

Eight pins, **four verified red against the pre-fix tree**: the struct-literal
store, the field assignment, the plain declared return, and A.4's own
`compose`. The four controls — a void field store, the `async || T` fix, the
same function at a SYNC instance, and transitive adaptation through a
store-free body — were green before and after, which is what says the check is
per instance and did not widen the rule.

### A.5 Snapshot semantics for adapted receivers

An adapted `map` cannot hold a view of its receiver across the callback
awaits — that is exactly what no-view-across-await forbids, and the rule is
right: during an await, arbitrary interleaved code (turns, handlers, other
spawns) runs, and *anyone* who can reach the viewed root may mutate or
reallocate it. Note the two tempting loosenings and why they fail:

- "the closure can't reach the view" — necessary but insufficient; the
  hazard is the interleavable world, not just the callee;
- "prove the view isn't mutated" — unverifiable against that same world.

The sound options are escape analysis on the *root* (a local that never
escapes — no `Shared`, no capture, never passed outward by view — is
unreachable by interleaved code, so the borrow is safe) or snapshotting.

**Decision: adapted std higher-order functions iterate a snapshot** — one
copy of the receiver taken at the call. This is the better *observable*
semantics, not just a checker dodge: an awaiting `map` traverses the
receiver as of the call; interleaved mutations do not tear the traversal.
Under value semantics "you got a copy" is the least surprising rule in the
language. The escape-based borrow is recorded as a later, purely-internal
optimization (it must not change observable behavior, which the snapshot
contract pins).

### A.6 Host-lowered functions: dual lowering

Where a sync instance lowers to a host intrinsic, the async instance emits a
vilan loop body (with awaits + the snapshot); where the function is ordinary
vilan source, both instances emit from the same body. Consequence to accept:
a distant `sleep` added deep in a callback silently moves a `map` from the
native fast path to an emitted sequential loop. That is the cost of
consistency, and it is only paid by call sites that actually went async; the
tooling mitigation is an "async because …" origin chain on hover (A.8).

`array.map(|x| async { work(x) })` involves a **sync** closure returning a
promise value — sync instance, native lowering, `List<Promise<T>>` result.
The concurrency opt-in costs nothing.

### A.7 Concurrency is an idiom, plus one helper

```vilan
// start all (sync closure returning promises), then settle in order:
let ids = urls
    .map(|url| async fetch_id(url))    // List<Promise<i32>> — all in flight
    .map(|p| await p);                 // adapts; total ≈ max, not sum
```

A std helper can name the second half (`settle_all(List<Promise<T>>):
List<T>` or a `.settle()` method; `std::promise`'s gathered form already
exists — pick one surface at implementation, don't add two).

**Failure semantics, stated:** a started promise that rejects before its
settle pass is reached is a *late unhandled rejection* if the pass is
abandoned (a panic between the two maps, a short-circuiting combinator).
v1 documents this hazard; the real fix is Part B — inside a scope, every
spawn settles at scope exit, absorbed or propagated, never orphaned.

**`Promise<T>` under value semantics must be pinned at implementation:** it
is a *handle* (copy = the same promise), never `__clone`d — a deep copy of a
pending promise is nonsense. `async-promise-all.vl` suggests the emission
already behaves; the rule needs a pin.

### A.8 Diagnostics and tooling

- Errors arising inside an adapted instance (a `sync` violation reached
  transitively, a view error in a user HOF that borrows across the new
  awaits) are **instantiation-dependent**. They attribute with origin
  chains, platform-coloring style: *"async instance required by the call at
  main.vl:12 → helper → map"*. This is the acknowledged cost of
  monomorphized effects; the chains are the mitigation.
- The `sync`-violation message carries the per-site steer (A.2).
- LSP: hover on a call can show the chosen instance's asyncness with its
  origin chain (rides the existing coloring-hover machinery). Polish, not a
  gate.

### A.9 Std audit (initial; finalized at implementation)

- **Adapt** (plain parameters): `List::map/filter/each/find/reduce/sort_by`,
  `Option::map/and_then/unwrap_or_else`, `Result::map/map_err/and_then`,
  retry/walk-style helpers.
- **`sync`**: `Signal::map/effect/set_with`, `bind_each` and render
  callbacks, reactive comparators/keys, `turn`, `batch`.
- **Spawn (void, unchanged)**: `ui.on`, `dispatcher.on` handlers,
  reconnect hooks.
- Every flip is its own reviewed line in the implementing commit; `sort_by`
  adapting (sequential awaited comparisons over the snapshot) is included
  unless the audit finds a reason not to.

### A.10 Test plan (pins before behavior ships)

adaptation runs sequentially (effect ORDER pinned, not wall time); sync
instance byte-identical (corpus); `sync` refusal message + steer; void spawn
preserved; snapshot observation (mutation during awaits doesn't tear);
transitive `helper → map`; store/return exclusions still refused; extern
refusal; the opt-in idiom compiles native (golden) and runs; mixed
closure-parameter arity (one async, one sync); `Promise<T>` never cloned.

---

## Part B — Nurseries (the J1 execution-model seed) — **SHIPPED 2026-07-18**

Shipped in four slices: `Task<T>` (ae2d675), the nursery core (9b85534),
cancellation + the AbortSignal bridge (24e4dd7), docs (this commit).
Implementation deltas from the design below:

- **Settle-time failure reaction**: a failing owned task notifies its
  nursery AT SETTLE (`__fail`: latch, abort, wake the drain) and the join
  races each child against the wake — a fast failure behind a slow healthy
  sibling reacts immediately, and "earliest-settled wins" is structural
  (the latch), no sequence stamps needed.
- **Owned tasks never default-report** — the nursery observes them; the
  unobserved-failure report is for free-floating tasks only. Post-failure
  stragglers spawned into a dead nursery are silently absorbed (owned).
- **Body-cancellation semantics** (was unrecorded): `cancel()` kills
  children, not the body — code after it runs and the value returns; a
  body SUSPENDED on cancellable IO when the signal fires observes the
  rejection, which propagates as the nursery's outcome (body-throw rule).
- **Registration mechanics**: spawns are SAFE reads of `ambient_nursery`
  in the context pass (a new demand kind riding the whole strict/safe
  apparatus), engaged only when `nursery` is called somewhere — loading
  `std::task` alone keeps every program byte-identical. The body parameter
  is an injected-clause closure (`context ambient_nursery`), so the
  literal takes its own hidden parameter; an awaiting body rides Part A
  adaptation into the machinery.
- **Holes this forced open and closed**: a directly-applied async closure
  literal (the lowered `run` body) never counted as an await point —
  latent miscompile for ANY async run body, fixed in subject_awaits /
  awaited_calls / the J3 initializer check; extern refusals now honor the
  typed channel (a DECLARED `async |…| T` extern parameter is the host's
  contract to await — `__nursery_run`); WrapSome thread forms now trigger
  Option variant resolution (spawn demand creates covered→safe boundaries
  with no `get_safe` in the program); spec §7.1's exit claim corrected
  (the host exits when no live handles remain, not "when `main`
  completes").
- **Still open**: ~~`Task<Task<T>>` assimilation~~ (**SHIPPED 2026-08-04**
  — see below); per-task cancel handles (race composes from nursery-scoped
  cancel, so deferred until a real need); the free-spawn lint (std's own
  audit found NOTHING to migrate — every std spawn is either a returned
  `Task` or object-lifetime work a function-scoped nursery cannot own,
  each now comment-marked as deliberate; the lint waits on the
  resource-owner story). The abort-in-flight `fetch` e2e is CLOSED:
  `crates/vilan-cli/tests/cancellation.rs` cancels a fetch against a
  hanging endpoint and joins in ~3s instead of 60.

### `Task<Task<T>>` assimilation — SHIPPED 2026-08-04

**The rule: `Task<..>` is IDEMPOTENT as a type constructor.** A task settles
with a value, and a task is not one — the host's promise resolution procedure
adopts a thenable result instead of boxing it, recursively — so `Task<Task<T>>`
describes nothing any expression can hold. It is no longer a type any expression
has.

**The seam is FORMATION, not `await`.** Assimilating at `await` was the obvious
move and the wrong one: it makes the unwrap agree with the runtime while leaving
the mistyped handle in circulation, so a combinator over it (`Task::settle_all`,
`Task::race`) still reads `List<Task<T>>` where the host will hand back
`List<T>`. Normalizing where a `Task<..>` is BUILT fixes both, and leaves
`await`'s single unwrap exact rather than making it a loop. Two sites form one
(`analyzer.rs`), sharing `assimilated_task_payload`:

1. **`Expr::Async`** — the only way a task value arises at all (`task.vl`: "a
   task only ever arises from `async`"). A body that is already a handle
   contributes no layer.
2. **`substitute_type`'s `Type::Struct` arm** — the generic instantiation. This
   is the sharp edge the item was filed with, and it turned out to be the same
   bug rather than a deeper one: `fun wrap<T>(value: T): Task<T>` called with a
   task substituted `T := Task<i32>` into the declared return and minted
   `Task<Task<i32>>` from a function whose body was already assimilated. The
   runtime was probed first and is unambiguous — `wrap(task)` settles with the
   `i32` — so the type follows it.

The strip loop is bounded by the payload ids it has seen, so a self-referential
payload stops instead of regressing; honest chains collapse in one pass because
each layer's id is distinct. An ERASED handle (`Task` with no argument) is left
alone — there is no payload to promote and inventing one would be a guess.
Non-`Task` nesting is untouched by construction: the normalization is gated on
the handle's own id.

**RESIDUAL (recorded, pinned `#[ignore]`d):** an `async fun` whose DECLARED
return is itself a `Task` — `async fun make(): Task<i32>` — still types one level
deeper than it runs. Its calls are implicitly awaited, so the host assimilates
the returned handle and the call site receives the `i32` (verified: the program
prints `7`, not a handle), while the call types as `Task<i32>`. This fix cannot
reach that seam: async-ness is a whole-program fixpoint over the call graph
(`async_infer::infer`), run AFTER type inference, so while a call's type is being
decided the analyzer does not yet know whether its callee is async and its result
therefore assimilated. Closing it wants the two passes interleaved, or an
`Awaited<T>` type-level operator — more than this item. Pinned both ways in
`crates/vilan-core/tests/inference.rs`
(`an_async_function_returning_a_task_is_assimilated_at_runtime_only` holds the
honest current behavior; `..._should_type_as_the_value` is the `#[ignore]`d
desired end state).

Gates: 14 pins in `inference.rs`, red-first (`await` on a nested task typed
`Task<i32>` over a runtime `7`); spec §7.3 states the rule with a compiled fence;
corpus byte-identical.

Original direction (decisions recorded 2026-07-18, all implemented):

- **`nursery(body)`** — a `std::nursery` FUNCTION, not syntax (adaptation
  makes an awaiting body just work): spawns created within its dynamic
  extent are joined at exit; the nursery's value is its body's value; it
  returns only when all children settle. Registration is **dynamic-extent
  via `context`** (the handle threads as a scoped value, like the reactive
  ambient owner) — a helper called inside spawns into it without plumbing.
  **DECIDED: the name is `nursery`** (`scope` has too many meanings), and
  **v1 is explicit** — `main` is NOT an implicit root; free-floating spawns
  keep today's behavior, with a lint later once std itself is scoped.
- **Errors — DECIDED, first-observed**: a body throw wins if it happens
  before the join; otherwise the earliest-settled rejection. The nursery
  stops awaiting the rest, **absorbs** their eventual rejections (no late
  unhandled rejections), and re-raises the winner at the join with an
  origin chain naming the spawn site. Abort-caused rejections classify as
  CANCELLATION (absorbed), never as a competing first error.
- **Cancellation is cooperative and honest about JS — with the AbortSignal
  bridge**: the nursery owns a host `AbortController`; its signal rides the
  same context value as the token, and std's host-IO wrappers (`fetch`,
  `sleep`, sockets) pass it to the host op. First error (or an explicit
  `n.cancel()`) aborts the controller, so in-flight HOST IO genuinely
  cancels and the join is fast; pure-compute loops still check
  `cancelled.get()` at their own points. Nested nurseries chain signals.
  Instrumenting every implicit await is the possible v2 (cost measured
  first); native targets can preempt better later.
- **`Task<T>` as the substrate** (2026-07-18): `async expr` lowers to a
  std `Task<T>` — a HANDLE (like `Shared`; copy = same task; never
  `__clone`d, dissolving Part A's Promise-under-value-semantics pin) that
  wraps the host promise plus its abort handle and spawn-site origin, and
  **attaches the absorption handler at construction** — unhandled
  rejections become structurally impossible program-wide, not just inside
  nurseries (an abandoned task outside any nursery gets an orderly default
  report). Host promises wrap at the std extern seam; raw `Promise<T>`
  remains for direct host interop. Tasks stay EAGER (run to first
  suspension synchronously, as §7.3 specifies) — a cold task would be a
  semantic break for no benefit. NOTE: a global `Promise` polyfill is NOT
  viable — JS async functions return the intrinsic %Promise% regardless of
  the patched global, and species games don't capture it; owning the
  lowering and the std boundary replaces it.

Token ergonomics — **DECIDED as recommended (user delegated 2026-07-18)**:
the structural AbortSignal bridge (std IO reads the ambient signal, no
token threading) + the `nursery(|n| …)` handle variant for
cancel-from-within. The `Task` surface shipped as `Task::settle_all` +
`Task::race`; the race idiom is `race` + `n.cancel()` (cancel-after-settle
is a no-op for the winner, so nursery-scoped cancel suffices).

---

## Part C — Parallelism appendix (design record; not v1, forecloses nothing)

- **Sendability is the shared spine.** Plain values cross any concurrency
  boundary by construction (value semantics — no aliases); `Shared<T>`,
  views, and non-`Wire` closures do not. The check is platform-coloring-
  shaped machinery, and `Wire` already answers serialization.
- **JS lowering**: workers + `Wire`; a parallel scope mirrors Part B's scope
  with worker execution (`par` / `worker_map` — surface deliberately
  unspecified here).
- **Native future**: the same discipline scales to threads; fork-join over
  immutable second-class views is provably race-free by construction, which
  is the safe first shared-memory extension; actors + supervision (the BEAM
  idea worth keeping) are a possible std layer above it, never core.
- Async (interleaving) and parallelism (simultaneity) share sendability and
  the scope vocabulary and **stay separate in scheduling semantics** — one
  vocabulary, different machines.

---

## Decisions and open questions

**Decided (2026-07-17):** default-adapt for plain non-void closure params;
sequential contract; marker spelled `sync` (contextual keyword);
void = spawn preserved; snapshot semantics for adapted std receivers;
effect-dependent returns excluded in v1; externs implicitly `sync`
(non-void); concurrency via the spawn-then-settle idiom + one helper.

**Open, Part A (settle at implementation):** ~~the helper's surface~~
(SHIPPED: `Task::settle_all` + `Task::race`, statics on the handle);
`sort_by` inclusion; ~~`Promise<T>` handle pin~~ (dissolved — `Task<T>` is
a class-instance handle, `__clone` passes it through); `sync` steer
wording per std site.

**Part B (SHIPPED 2026-07-18):** ~~scope keyword~~ (`nursery`, a std
function); ~~implicit root scope~~ (explicit v1); ~~token ergonomics~~
(structural bridge + handle variant); await-point instrumentation stays
NOT taken (the bridge covers IO; compute loops poll `is_cancelled`).
