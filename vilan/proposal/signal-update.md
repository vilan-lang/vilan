# `Signal::update` — mutate the stored value in place (A18)

> **Status: IMPLEMENTED 2026-08-03.** The design is the project owner's call
> (2026-08-03): backlog A18's candidate **(a)**, `update(self, mutate: sync
> |&mut T| void)`. Candidate (b) is rejected — see §2. Semantics below were
> settled by probe against the shipped compiler, then pinned. Residuals in §8.

## 1. Motivation — the copy-transform-return dance

Backlog **A18**: mutating a `Signal<List<T>>` today is `set_with`, which is
typed `sync |T| T` (`reactive.vl:425`) — read the value out, transform it,
hand a whole new value back:

```vilan,fragment
todos.set_with(|mut list| {
	list.push(todo);
	list
});
```

Rule 1 makes that `mut list` a genuine copy of the stored list, so a push
costs a whole-list copy, and the shape reads as a transformation when the
author means a mutation. `Shared` already has the right form for the STORAGE
half — `write(self): &mut T borrows self` (`shared.vl:28`) — but a signal
write must also NOTIFY, so a bare view is not enough.

The point of the design is that it **generalizes over every collection**: one
method on `Signal<T>` serves `List`, `Map`, `Set`, a user struct, a nested
aggregate — anything the closure can mutate through a view. A dedicated
`ListSignal` was considered and rejected as non-general (Set/Map/user types
would each need a twin).

## 2. The decision, and why (b) was rejected

**(a) — shipped.** `update(self, mutate: sync |&mut T| void)`: the closure
receives a writable view of the stored value, and the runtime notifies once
after it returns.

**(b) — rejected.** `write(self): SignalWrite<T>`, a guard whose view mutates
storage and whose `drop` notifies. It reads best (`signal.write().push(5)`)
but hangs the notification on **temporary-drop timing**, and A18 records the
second cost: it needs a rule-4 story for a guard **held across a re-entrant
read**. (a) has neither problem — the notification boundary is the closure's
return, which is a syntactic fact, and no guard outlives the call.

## 3. The rule

**`update` publishes exactly one notification, unconditionally, after `mutate`
returns.**

- **Unconditional.** A `mutate` that writes nothing still notifies. This
  matches `set`, which never compares either; equality-gating one write and
  not the other would be two rules where one will do, and `T: PartialEq` is
  not a bound `Signal` carries. Cheap and predictable beats clever.
- **Exactly once, after.** Not per mutation inside the closure, and not
  before. `update` is a single write in every sense the rest of the model
  cares about.
- **Under `batch` / `turn`** the notification defers and coalesces exactly
  like any `set` — `update` shares `set`'s notify half verbatim (§5), so
  turn scoping, drain affinity, and dedup are inherited, not reimplemented.

## 4. Re-entrancy

- **A re-entrant READ inside `mutate` observes the in-progress value.**
  `mutate` writes the signal's storage directly, so a `get()` from inside the
  closure sees the mutations made so far. This holds uniformly for an
  aggregate `T` and a scalar `T` (a scalar view writes through `(cell, "v")`,
  which is the same slot `get` reads). Pinned both ways.
- **A re-entrant `update` of the SAME signal inside `mutate` is
  UNSUPPORTED.** It is a rule-4 invalidation — the inner call mutates the
  aggregate's geometry while the outer call's view of it is live — that the
  static check cannot see through `Shared` (spec §6.4's dynamic remainder is
  recorded future work, §6.7). It is not rejected at runtime: a guard would
  cost every `Signal` a third cell and would diverge from `set`, which
  carries no such guard, and the model's stated position is that the cell-
  level dynamic check is a whole-model slice, not a per-method patch.
  Observed today (recorded so a future change is a deliberate one, NOT
  pinned — pinning would make undefined behavior a contract): both calls
  notify, inner first, and both observers read the final state.

## 5. Implementation

- `Signal::set`'s notify half is extracted to `Signal::notify(self)` and
  shared: `set` is `value.write() = value; self.notify()`, and `update` is
  `mutate(self.value.write()); self.notify()`. Turn scoping and drain
  affinity live in one place, so the two writes cannot drift.
- `reactive.vl` is **not twinned** — one `std/src/reactive.vl` serves both
  platform legs (`browser/` and `process/` hold no `Signal`), so there is one
  implementation to keep honest.
- **Compiler support was genuinely needed** (A18 guessed "likely none"; the
  probe says otherwise), in two independent places:
  - **A closure literal's parameters now take the full parameter grammar.**
    `parse_closure_parameter` hard-coded `Convention::Bare` and inferred no
    convention from a declared type, so `|&mut list|` was a parse error and
    `|list: &mut T|` silently bound by value — a closure could not receive a
    view at all. Closure literals now share `parse_function_parameter`, so
    `(mut | own | & mut?)? binder (: type)?` means the same thing in both
    positions, including the existing rejection of `mut` combined with a
    convention. This is the language mechanism `update` needs; it is general,
    and nothing else in the tree relied on the asymmetry.
  - **A scalar `Shared::write()` now lowers to its `(base, key)` pair.** It
    emitted the bare `cell.v` slot — the VALUE — so it worked only as an
    assignment target (`cell.write() = x`) or over an aggregate (a JS
    reference). Passed where a `&mut i32` was expected it handed the callee a
    number, and the callee's `slot[0][slot[1]]` crashed at runtime with no
    diagnostic. This was a **pre-existing bug** independent of `update`
    (`replace(cell.write(), 9)` over a `Shared<i32>` reproduces it at HEAD);
    fixing it at the root is what makes `update` uniform across scalar and
    aggregate `T` instead of correct for one and broken for the other.
    Whether the pointee is scalar is decidable only per monomorphization, so
    the verdict is taken in the transformer from the receiver's resolved
    `Shared<T>` argument; the assign-through and deref sites take the `v`
    slot back off the pair.

## 6. The memory model

- **The closure parameter is a view in a sanctioned position.** Spec §6.3
  admits a view as a **parameter**; what it forbids is storing one in a
  field, a collection, an enum payload, or a `Signal`/`Shared` payload, and
  returning one except through a `borrows` projection. That ban is enforced
  for this parameter exactly as for any other — storing `xs` into a struct
  field inside the closure is the ordinary escape error. Pinned.
- **Rule 4** constrains invalidation under a live view (reassign, resize,
  move, drop, geometry advance), not aliasing and not content writes. A
  `mutate` body that pushes, inserts, or field-writes through its view is
  precisely the permitted case. The one invalidation `update` can express is
  the re-entrant `update` of §4, which the static checker cannot see through
  `Shared`.
- **`sync` is the `await` fence.** A view may not be live across an `await`
  (spec §6.6), and the view is live for exactly the closure's extent, so the
  callback must complete synchronously. `sync` states that contract, matching
  `set_with`. **Honesty note:** the contract is not *enforced* today for a
  `void`-returning closure parameter — `sync || void` accepts an awaiting
  closure where `sync || i32` refuses it. That gap is pre-existing and
  independent of `update` (§8); `sync` is written here because it is the
  correct declaration and will start biting when the gap closes. The
  rejection is pinned `#[ignore]`d, per the repo's known-but-unfixed rule.
- **`Signal<resource>`**: R10 rejects a resource argument to `Shared`, and
  `Signal<T>`'s storage *is* a `Shared<T>` — but the check keys on the
  written type's head, and `Signal`'s own `Shared<T>` field is generic at its
  declaration, so `Signal<Database>` compiles today while `Shared<Database>`
  is refused. That is a pre-existing hole in R10's coverage, not something
  `update` opens or widens (§8). Until it closes, `update` over a resource
  `T` inherits whatever `Signal` does.

## 7. What this deliberately does not change

- **`set_with` stays, unchanged.** It is the right tool for a scalar
  (`count.set_with(|n| n + 1)` beats `count.update(|&mut n| { n = *n + 1; })`)
  and for a genuine transformation. `update` is the mutation door, not a
  replacement.
- **`Source` gains nothing.** `Source<T>` is the read-only view; `update`
  joins `set`/`set_with` on `Signal`.
- **No equality gate, no dirty tracking, no `PartialEq` bound.**

## 8. Residuals / finds worth filing

- **`sync` is not enforced for a `void`-returning closure parameter.**
  `fun run_now(body: sync || void)` accepts `|| { sleep(1); }`; the same
  signature returning `i32` refuses it. Pre-existing, independent of A18,
  reproducible with no std involvement. Pinned `#[ignore]`d.
- **`Signal<resource>` slips R10.** `Shared<Database>` is refused;
  `Signal<Database>` compiles. R10's check keys on the written application's
  head, so a resource reaching `Shared` only through a generic struct field
  is invisible to it. S-sized; the fix is to seed the check from
  instantiations as well as written applications.
- **A re-entrant `update` of the same signal** (§4) is undefined rather than
  diagnosed. If it ever needs enforcing, the device is a `Shared<bool>` on
  `Signal` in `Turn::draining`'s shape — deliberately not paid for now.
- **Closure parameters now admit `own`** as a side effect of sharing the
  parameter grammar. It parses and behaves as the convention says; no use
  exists in tree. Left admitted rather than special-cased out, per "special
  cases are a smell".
