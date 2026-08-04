# The optimistic-write → reconcile lifecycle (A14)

Status: **proposed 2026-08-04**, core implemented in the same arc.

The second of A14's reactive residuals (`backlog-2026-07-18.md` §14): "the
optimistic-write → reconcile lifecycle (rides turns; `optimistic` covers the
one-shot case today)". Its sibling — `Draft`'s reconnect re-push and
debounce — shipped the same day and its record is `draft-reconnect.md`;
much of the vocabulary here (a generation guard, a confirmed shadow,
at-least-once) is that record's, deliberately reused rather than reinvented.

## 0. Why a new file

`optimistic` was born in `reactive-turns.md` §5.5 and that file's status
header reads **SHIPPED**, declaring "**A6 is COMPLETE**". Reopening a
completed record to hold a new open design would make its status unreadable,
which is the failure mode the backlog reconciliations keep finding. The same
call `draft-reconnect.md` §0 made, for the same reason, and with the same
remedy: a short record of its own, plus a cross-reference line in
`reactive-turns.md` §5.5's follow-on sentence.

The design also straddles three records — `reactive-turns.md` (turns),
`draft-reconnect.md` (the lifecycle vocabulary) and the `Draft` half of
`kolt-migration.md` — and is owned by none of them.

## 1. What `optimistic` is today, exactly

`std/src/reactive.vl:178-195`, in full:

```vilan
fun optimistic<T, E>(signal: Signal<T>, value: T, commit: async || Result<T, E>): Result<T, E> {
	let previous = signal.get();
	signal.set(value);
	let outcome = commit();
	match outcome {
		Ok(let confirmed) => signal.set(confirmed),
		Err(let _error) => signal.set(previous),
	}
	outcome
}
```

Probed rather than read off, because the docstring and the two pins only
cover the single-write path:

- **It is not fire-and-forget.** `commit` is `async`-typed, so the call
  through it awaits (J2), and `optimistic` adapts: the caller awaits the
  whole lifecycle and gets the outcome back.
- **A confirmation replaces the paint** with server truth — `Ok(confirmed)`
  is `set`, not merely accepted. So the commit is the reconcile channel:
  server truth arrives as the commit's own return value, not out of band.
- **A rejection rolls back** to whatever the signal held at the call, and
  the error reaches the caller only through the return value.

Three things it does *not* have, and the third is a bug:

1. **No pending state.** Nothing observes "a write is on the wire". A UI
   that wants "Saving…", or a button that should not be pressable twice,
   holds its own boolean — §2 shows the tree holds none, anywhere.
2. **No observable rejection.** The `Err` is returned. A caller that
   fire-and-forgets (`let _ = async optimistic(..)`) drops it on the floor;
   there is no cell for a failure banner to bind.
3. **Overlapping writes corrupt the cell.** `previous` is captured per call
   and there is no ordering guard, so an older write's outcome lands after a
   newer one's and wins. Probed:

   ```vilan
   let label = Signal::new("A");
   let _watch = label.sub(|value| print(i"label {value}"));
   let _first  = async optimistic(label, "B", || { sleep(50); Err("nope") });
   let _second = async optimistic(label, "C", || { sleep(10); Ok("C-confirmed") });
   ```
   ```text
   label A
   label B
   label C
   label C-confirmed
   label A          ← the older, FAILED write's rollback
   final A
   ```

   The server holds `C-confirmed`; the screen shows `A`. Two independent
   faults compose here — a superseded outcome is not discarded, and the
   rollback target is a *local* value (`B`, painted by a write that failed)
   rather than the last value the server confirmed. Both are what `Draft`'s
   `generation` + `synced` pair exists to prevent
   (`draft-reconnect.md` §3.2); `optimistic` is a free function over a bare
   `Signal` and has nowhere to keep either.

**That last sentence is the finding this design turns on**, and it is
structural, not an oversight: a correct multi-write lifecycle needs per-cell
state, and a stateless function cannot hold it. The same shape of finding as
`draft-reconnect.md` §1 ("a `Draft` cannot ask 'am I connected?' because it
does not know what it is connected *through*").

## 2. What the tree actually asks for

Swept: every `.vl` under `vilan/`, every fenced doc example, the four sibling
repos, and the real Kolt app.

**`optimistic` has no callers.** Not one, anywhere. Its entire use is two
Rust pins (`inference.rs:13372`, `:13402`), both faking the server with a
bare `async fun tick()`. `docs/std/reactive.md`'s signature sits in a
`vilan,fragment` fence, which `docs.rs` does not compile.

That is not neglect; it is a doctrine collision. `docs/guide/services.md:217`
states the shipped answer for server writes: **mutate via rpc, observe via
mirror** — "the confirmation the user sees is their own change arriving back
through the mirror". Under that doctrine the value is the mirror's, so there
is nothing local to paint and nothing to roll back. What the doctrine leaves
undone is everything *around* the value:

- `examples/walkthrough/src/views.vl:171-179` — the Delete button, which
  `docs/guide/reactive.md:257` names as `optimistic`'s example use, is
  `report(client.delete_note(..))`: no paint, no pending (so it is clickable
  N times during the round trip), and a failure goes to `console.log`.
- `examples/walkthrough/src/views.vl:83-97` — sign-in hand-writes three
  outcome branches into a `status` signal and has no `submitting` flag, so
  Submit stays live across the whole round trip. Kolt repeats it verbatim
  (`/home/reed/code/kolt/src/views.vl:86-96`).
- Four more `report(client...)` sites in `examples/todo` and three in Kolt.
- **Zero** user-authored pending booleans exist in the tree. The only
  in-flight indicator anyone renders is `Draft.state`, through a
  `state_text` mapper duplicated byte-for-byte in two repos
  (`walkthrough/src/views.vl:267`, `kolt/src/views.vl:352`).

So the measured demand is for the **lifecycle state** — pending, and a
rejection something can bind — and the optimistic paint is the part nobody
is asking for. That inverts the naive reading of the backlog entry, and §7
and §9 are where it lands.

`rpc.vl:530-534` is the one std-side gesture at the feature
("`connection_state` … or gate optimistic writes on it") and nothing in the
tree does it.

## 3. The settled lifecycle

Three states, and they are the entry's own words:

```vilan
[derive(PartialEq, Debug)]
enum WriteState {
	// Nothing in flight; the cell's value is the last confirmed truth.
	Confirmed,
	// The newest write is on the wire.
	Pending,
	// The newest write was refused; the cell has rolled back.
	Rejected(str),
}
```

`Confirmed` covers both a fresh cell and a settled success, exactly as
`DraftState::Synced` does — a fresh cell's value *is* confirmed truth by
construction (§4 seeds it from the signal). `Rejected` is sticky until the
next write, like `Failed`; a banner should stay up until something changes.

`Rejected` carries a reason `str`, not a generic `E`. Fixing the error type
is the shipped `Draft` decision (`commit: async |T| Option<str>`) and the
tree already writes the adapter that produces it —
`walkthrough/src/views.vl:255-265` turns a `Result<i32, RpcError>` into
`Some(i"rpc error: {error.debug()}")`. A generic `E` would infect the enum,
every `match`, and every binding site to carry a value that only the calling
site can render usefully anyway. Rejected in §8.

## 4. Per-cell state, and multi-writer ordering

```vilan
struct Optimistic<T> {
	value: Signal<T>,                  // the signal being written; bind it
	state: Signal<WriteState>,         // bind a spinner / a banner / disabled
	confirmed: Shared<T>,              // last server truth — the ROLLBACK TARGET
	generation: Shared<i32>,           // only the newest write paints
	confirmed_generation: Shared<i32>, // confirmations land in write order
}

fun Optimistic::over(signal: Signal<T>): Optimistic<T>
```

The cell **wraps an existing signal** rather than owning a fresh one. That
is what makes the growth compatible: `optimistic(signal, value, commit)` and
`Optimistic::over(signal).write(value, commit)` are the same lifecycle over
the same signal, and a caller adopting the cell changes no binding.
`confirmed` seeds from `signal.get()` — the value the cell is handed is, by
definition, the last thing anyone confirmed.

### The two guards, and why they are two

**Paint guard — `generation`.** Every `write` claims `generation + 1` before
painting. When its commit returns, it reconciles the cell *only if its
generation is still current*; a superseded write's outcome touches neither
`value` nor `state`. This is `Draft::settle`'s guard verbatim
(`reactive.vl:750-761`) and it alone fixes the probe in §1: the older
failure is discarded and the screen keeps `C-confirmed`.

The rule stated positively: **the newest write owns the cell.** If write 1
fails while write 2 is in flight, the cell keeps showing write 2's paint and
stays `Pending` — the user's latest intent stands until its own commit
answers. A superseded rejection is not recorded anywhere, deliberately: a
banner for a write the user has already replaced is noise.

**Knowledge guard — `confirmed_generation`.** The paint guard is not
sufficient, because it decides *who paints* and the rollback target is a
different question: *what do we know the server holds?* A superseded write
that **succeeded** still told us something true, and out-of-order
completions must not un-tell it. So an `Ok` advances `confirmed` iff its
generation is the highest to have reported one:

```
if mine > confirmed_generation { confirmed = value; confirmed_generation = mine }
```

Splitting the two is what makes a rollback honest. Without it, write 1
confirming `B'` and write 2 then failing rolls back to `A` — a value the
server has not held since. With it, the rollback lands on `B'`.

An `Err` advances nothing: a refused write taught us only that the server
still holds whatever it held.

### Ordering, in one table

Two writes on one cell, `B` then `C`, over a cell confirmed at `A`:

| write 1 (older) | write 2 (newer) | cell settles at | `state` |
|---|---|---|---|
| `Err` | `Ok(C')` | `C'` | `Confirmed` |
| `Ok(B')` | `Ok(C')` | `C'` | `Confirmed` |
| `Ok(B')` | `Err` | `B'` | `Rejected` |
| `Err` | `Err` | `A` | `Rejected` |

Row 3 is the one only `confirmed_generation` gets right. Row 1 is §1's probe.

## 5. How it rides turns: one wave per transition

The entry's constraint. A lifecycle transition writes **two** signals
(`value` and `state`), so "rides turns" means precisely: an observer of both
must never see half a transition.

It does not today. Probed on shipped `Draft`, whose `push` writes `local`
then `state`, with a `combine((local, state))` observer and no ambient turn:

```text
saw A/0      ← initial
saw B/0      ← the new text, still claiming Synced   ✗
saw B/1      ← Dirty
```

Under a boundary turn the two coalesce, which is why nothing has tripped
over it: `View.on` wraps every dispatch in `turn(AtSuspension, ..)`
(`browser/ui.vl:171-172`). But a `Draft` driven from a node program, from
SSR, or from a test has no ambient turn and publishes the incoherent middle.

**The rule: every lifecycle transition is a `batch`.** `batch` joins the
ambient turn when there is one — so under a boundary turn nothing changes at
all — and creates a fresh one when there is not, which is exactly the case
that was broken. Probed:

```text
unbatched A/0   unbatched B/0   unbatched B/1
batched  A/0    batched  B/1
```

This lands on `Draft` too. `push` (`local` + `state`) and `adopt`'s clean
branch (`local` + `state`) are the same two-signal transition and get the
same `batch` — a root-cause fix rather than a rule the new cell alone obeys.
`settle` writes one signal and needs nothing.

### The three ambient shapes, settled

The cell's `write` awaits, like the free function. What that costs and buys
was probed against the real boundary shape (`View.on`'s concrete `|| void`
handler inside an `AtSuspension` turn):

1. **A UI event handler** — two coherent waves, which is the intended
   cadence. The handler's `|| void` position keeps spawn semantics, so the
   boundary turn drains at the handler's first suspension (carrying the
   paint *and* the `Pending` flip), and the reconcile lands in the
   continuation segment's microtask drain:

   ```text
   wave A/0
   wave B/1              ← paint + Pending, one wave
   after dispatch
   wave B-confirmed/0    ← reconcile, one wave
   ```

2. **No ambient turn at all** (a node script, SSR, a test) — each `batch`
   creates its own turn, so the same two coherent waves. This is the case
   §5's rule exists for.

3. **Inside a held turn** (`turn` with an awaiting body) — **the whole
   lifecycle holds and only the final state publishes.** Probed:
   `held A / held B-confirmed`. Nothing is pending-visible, because a
   transaction that showed its own intermediate states would not be one.
   This is the shipped `optimistic` docstring's "the transaction wins",
   confirmed rather than assumed, and it is a real consequence to document:
   a "Saving…" indicator inside a held turn never appears. A rejection
   inside a held turn republishes the unchanged value once (`set` never
   compares — `reactive.vl:436`), which is consistent with every other
   write in the system.

## 6. Disconnect mid-flight

`draft-reconnect.md` settled this ground for `Draft`; the answer here is
different, and the difference is the whole reason two lifecycles exist.

A commit caught by a dropped socket returns `RpcError::Transport("connection
lost")` (`rpc.vl`, via `reject_pending`). The cell rolls back and goes
`Rejected` — and **does not re-push on reconnect.**

That is not a gap left for later. `draft-reconnect.md` §3.2 established that
re-push is *at-least-once*: the server may already have applied a commit
whose acknowledgement died with the connection. `Draft` can decide that is
safe because the shape it is built for is "set the remote field to this
value" — last-write-wins, idempotent by construction. `Optimistic` is built
for the opposite shape: `docs/guide/reactive.md:257` calls it out for
one-shot **actions**, and the un-idempotent commit that record singled out
("a commit that *appends*") is exactly an action. Silently replaying one is
the harm at-least-once warns about.

So the recovery is the rollback itself: the cell returns to the last
confirmed truth, the state says why, and the user re-issues the action. The
intent behind a one-shot action is trivially reconstructible — press the
button again — which is precisely what is *not* true of half-typed text, and
is why `Draft` keeps its local value and this cell does not. There is no
`retry()`: it would be a second spelling of a button press, and it would
need the cell to hold a rejected value it has deliberately discarded.

Composing the two records, then: **`Draft` re-pushes because its commit is a
value; `Optimistic` rolls back because its commit is an action.** Wiring
`transport.on_reconnect(|| ..)` to anything on this cell is refused for that
reason, not omitted.

## 7. The surface

```vilan
// std::reactive — ADDED
[derive(PartialEq, Debug)]
enum WriteState { Confirmed, Pending, Rejected(str) }

struct Optimistic<T> {
	value: Signal<T>,
	state: Signal<WriteState>,
	…  // internals: the confirmed shadow and the two generations
}

impl Optimistic<type T> {
	fun over(signal: Signal<T>): Optimistic<T>;
	fun write(self, value: T, commit: async || Result<T, str>): Result<T, str>;
}

// std::reactive — UNCHANGED, byte for byte
fun optimistic<T, E>(signal: Signal<T>, value: T, commit: async || Result<T, E>): Result<T, E>;
```

`over` rather than `new` because the cell wraps a value it did not make —
the `Timer::after` precedent for a constructor named for what it does.

`write` awaits and returns the outcome, matching the free function, so the
one-shot caller's `match` is unchanged when it adopts the cell.
Fire-and-forget is `let _ = async cell.write(..)`, the spelling `Draft::send`
already uses internally. A void-returning `write` was rejected: the outcome
is genuinely useful at the call site (sign-in reads its own reply), and the
observable `state` is an addition to that, not a replacement for it.

The free `optimistic` stays, unchanged and still pinned. It is the stateless
one-shot: one write, no cell, no state to bind. §9 records the question of
whether std should carry both spellings permanently.

`Draft::push` and `Draft::adopt` gain a `batch` (§5). No signature changes,
and under a boundary turn — every shipped consumer — the emitted behaviour
is identical.

## 8. Rejected, with reasons

- **A generic `E` on the cell.** See §3. `Draft` fixed its error to `str`
  and the tree already writes the adapter.
- **`adopt(remote)` — folding an out-of-band mirror value into the cell.**
  The rules look forced at first (echo → nothing; pending → record the
  remote as the new rollback target; otherwise take it), but the ordering
  between an adopt and an in-flight write's confirmation is **not decidable
  from the client's clock**: they are two claims about server truth with no
  shared sequence. `Draft` escapes this because it never rolls back — a
  wrong `synced` only makes `repush` more eager, which is safe — whereas
  here `confirmed` is what a rollback *displays*, so an ambiguous shadow is
  a wrong value on screen. Shipping the ambiguity would be worse than not
  shipping the method. Consequence, documented rather than hidden: **a cell
  over a mirrored signal is out of scope for v1** — the mirror writes behind
  the cell's back and `confirmed` goes stale. Wrap a local signal. Reopening
  this needs a server-supplied sequence number, which is a wire-protocol
  question, not a `std::reactive` one.
- **`retry()` / keeping the rejected value.** §6.
- **Auto-clearing `Rejected` on a timer.** Sticky matches `DraftState::Failed`
  and a banner that vanishes on its own is worse UX than one the next
  action clears.
- **A paint-less action-state cell** — the thing §2's demand data actually
  points at. Not rejected; recorded as an owner question (§9), because
  adding a second cell is surface, and whether the mirror doctrine's
  one-shot actions should gain a pending state *at all* is a product call.
- **Merging `Draft` and `Optimistic` into one cell with a rollback policy
  flag.** They differ in more than the flag: `Draft`'s reconcile channel is
  an out-of-band `adopt` and its commit returns `Option<str>`;
  `Optimistic`'s reconcile channel is the commit's own return value, which
  carries server truth. A flag would have to gate the commit's *type*.
  Churning a shipped, pinned surface to express that is a cost with no user
  on the other side. They share vocabulary, not a body.
- **Gating writes on `connection_state`** (`rpc.vl:532`'s suggestion). It
  belongs to the caller's commit closure, which is where the transport is in
  scope — the same layering `draft-reconnect.md` §2 refused to invert.

## 9. Owner-questions

**Q1 — the demand is for the state half, and this arc ships the value half.
Should std also carry a paint-less action-lifecycle cell?**

§2's sweep is unambiguous: `optimistic` has zero callers, and the six
one-shot server writes in the walkthrough, the todo example and Kolt all
want the same two things — a `Pending` a button can disable against, and a
`Rejected(reason)` a banner can bind — while wanting **no local paint**,
because the mirror doctrine (`docs/guide/services.md:217`) already owns the
value. `Optimistic<T>` does not serve them: there is nothing to paint.

What would serve them is small and obvious — a cell holding just
`state: Signal<WriteState>` and a generation, with `run(commit)` — and it
would reuse everything in §3–§5 unchanged. Sketch:

```vilan
let deleting = writes();
view("button").text("Delete")
	.on("click", || deleting.run(|| commit_outcome(client.delete_note(..))))
```

against today's `report(client.delete_note(..))`.

This arc does **not** build it, for the reason `draft-reconnect.md` §2
declined a second reconnect spelling: a second cell is surface, and three
lifecycle types in one module (`Draft`, `Optimistic`, and this) is a lot to
ask a reader to distinguish. It is also possible the owner's answer is that
the mirror doctrine should absorb it — a generated client could expose a
per-call in-flight signal and no user-facing cell would be needed at all.
That is a product call about the framework's shape, not a correctness
question, and everything here composes with either answer.

**Q2 — should the free `optimistic` stay once the cell exists?**

Two spellings of one lifecycle is exactly the "second spelling is surface
without reach" objection `draft-reconnect.md` §2 raised against itself. The
case for keeping it: it is shipped, pinned, documented, and genuinely
smaller for a single write. The case for removing it: it is the spelling
with the §1 bug, it has no callers to break, and leaving it invites new code
into the broken path. This arc keeps it — removing shipped surface is not a
call to make inside an implementation arc — but it is the owner's.

## 10. Test plan

Rust-side (`inference.rs`), because the `.vl` corpus gate compiles and
byte-compares emitted JS without running it (`corpus.rs`), so ordering
cannot be pinned there.

1. **The one-shot case, unchanged.** The two existing pins
   (`inference.rs:13372`, `:13402`) must keep their exact expected output.
2. **Pending is observable** — a cell write publishes `Pending`, then
   `Confirmed`, and the value reconciles to server truth.
3. **Rejection is observable** — `Rejected(reason)`, value rolled back.
4. **The §1 probe, fixed** — older-failing / newer-succeeding overlap
   settles at the newer confirmation, not the older rollback.
5. **The confirmed shadow** — older-succeeding / newer-failing overlap rolls
   back to the older *confirmation*, not to the cell's original value
   (§4's row 3).
6. **`confirmed_generation` specifically.** Written after (5) turned out not
   to reach it: in that scenario the surviving write is the current one, so
   the *comparison* is never exercised — only the shadow's existence is.
   The edge needs two writes that both **succeed** with the older reply
   arriving last, then a third that fails; the rollback must land on the
   later confirmation, not the later arrival.
7. **One coherent wave** — a `combine((value, state))` observer with no
   ambient turn sees no half-transition, for both the paint and the
   reconcile.
8. **The same, for `Draft::push` and `Draft::adopt`** — the shipped
   regression §5 found. Both written red-first against shipped code.
9. **A held turn holds the whole lifecycle** — nothing pending-visible.
10. **Over a real round trip** — `local_rpc` + a `[service]` whose handler
    suspends, following `inference.rs:6134-6174`'s precedent, so the pin
    exercises a genuine wire turn rather than a bare `tick()`, and a call
    counter proves each write reached the server exactly once.

Each of the four guards was proven non-vacuous by planting the bug it
removes and watching its own pin — and only its own pin — go red: the paint
generation, the shadow's existence, the shadow's ordering comparison, and
the `batch` around the paint transition.
