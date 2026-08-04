# `Draft` on reconnect — auto re-push, and a debounced commit (A14)

Status: **proposed 2026-08-04**, implemented in the same arc.

The last of A14's three reactive residuals (`backlog-2026-07-18.md` §14):
"`Draft`: auto re-push of dirty drafts on reconnect; a debounced variant."
The deferral was recorded twice — once in A12's slice notes
(`kolt-migration.md` §4: "What the slice deliberately defers … re-push of
dirty drafts on reconnect, and a debounced variant") and once as the
mitigation HMR leans on for un-pushed `Draft` state (`hmr.md` §444).

## 0. Why a new file rather than an extension

The feature straddles two shipped records and is owned by neither:
`Draft` is A6/A12's (`kolt-migration.md` §3, implementation in
`std/src/reactive.vl`), the reconnect loop is K6's
(`transport-robustness.md`, implementation in `std/src/rpc.vl`). The whole
design question is *what the seam between them is*, so it belongs in its own
short record; both parents get a cross-reference line.

## 1. What a `Draft` can observe about a reconnect today: nothing

The investigation's finding, and it is sharper than expected.

There are two things user code can see about connection loss today:

1. `SocketTransport.connection_state(): Signal<ConnectionState>`
   (`rpc.vl:533`) — `Connected` / `Reconnecting` / `Closed`.
2. Typed per-call failure: `RpcError::Transport("not connected")` while
   down, `("connection lost")` for a call caught in flight.

Neither reaches a `Draft`, and the reason is structural, not an oversight:
**a `Draft`'s commit is an opaque `async |T| Option<str>` closure.** The cell
has no idea whether that closure rides a socket, an HTTP request, a local
function, or a test double. `Draft` cannot ask "am I connected?" because it
does not know what it is connected *through*. This one fact settles §4.

There is also a subtler gap. The reconnect loop is `handle_drop`
(`rpc.vl:653-679`), and its tail runs in this order:

```
668    duplex.state.set(ConnectionState::Connected);   // the state signal flips
669    for entry in duplex.on_reconnect.read() {        // ... then the hooks run
672        let hook: async || void = entry;
673        hook();
674    }
```

The state flip **leads** the mirror resync by a beat, deliberately
(`transport-robustness.md` §2.5: the hooks' own `__attach` call needs a
usable transport). So `connection_state()` is not just unreachable from a
`Draft` — it is the *wrong moment*: a subscriber to it cannot distinguish
"the socket is back" from "the mirrors are current". A draft that re-pushes
at the state flip races the resync that is about to `adopt` into it.

## 2. The reconnect signal's source: ride the hook list that already exists

`SocketDuplex.on_reconnect: Shared<List<|| void>>` (`rpc.vl:428`) is exactly
the "reconnected" event, already built, already awaited in order, already
carrying one entry (the generated client's `reattach_mirrors`). It is not a
public surface — it is a raw field, and `docs/std/rpc.md:78` says "App code
doesn't construct these."

**Decision: promote it, don't duplicate it.** One method:

```vilan
impl SocketTransport {
    fun on_reconnect(self, hook: async || void)
}
```

appending to the same list. Three properties fall out for free, and all
three are the ones we wanted:

- **Ordering is correct by construction.** The generated `connect` pushes
  `reattach_mirrors` at client-construction time; an app registers later, so
  its hook runs *after* the mirrors have resynced and awaited. The
  "reconnected AND resynced" moment §1 said was missing is simply *the tail
  of this list*, and registration order gives it without a second event.
- **Sequencing, not just notification.** The runner awaits each hook before
  the next (`rpc.vl:672-673`, the J2 re-mark at a `let`). A `Signal<bool>`
  could not offer that.
- **Nothing new to keep correct.** No second state machine, no new field on
  `SocketDuplex`, no change to `handle_drop`.

Rejected alternatives:

- *A `Signal<bool>` connection state.* It already exists in better form
  (`connection_state()`), and §1 shows it fires a beat too early for this
  use. Adding a second, later-firing signal would mean two signals that
  disagree about when "connected" means connected.
- *Passing the transport into `draft()`.* A layering inversion:
  `rpc.vl` imports `reactive.vl`, not the reverse, and it would make every
  `Draft` — including the ones in SSR and in tests — depend on a transport
  it may not have.
- *An rpc-side sugar `repush_on_reconnect(transport, draft)`.* Considered
  and dropped: `transport.on_reconnect(|| title.repush())` is already one
  line and composes with everything else you might want to do on reconnect.
  A second spelling is surface without reach.

## 3. Auto re-push semantics

### Which drafts

**Those whose local value the remote has not accepted: `local != synced`.**
Not the `DraftState` enum — the values. That test is the honest one and it
subsumes both cases the question asked about:

- *Uncommitted local edits* — `Dirty` with a commit that never left, or one
  that fail-fast rejected while down. `synced` unchanged. Re-pushed.
- *Committed-but-unacknowledged* — a commit caught in flight by the drop.
  `settle` resolves it `Failed("connection lost")`, which by design **keeps
  the local value and does not advance `synced`**. Re-pushed.
- *Clean* — `local == synced`, nothing the remote lacks. **No re-push**, so
  a page full of untouched drafts costs zero frames on reconnect.

A `Failed` draft whose failure was a *server rejection* rather than a
transport fault (validation said no) also has `local != synced`, so it is
re-pushed too. That is correct: the server may well answer differently now,
and the alternative — trying to classify failures a `str` reason does not
distinguish — would be guesswork.

### Delivery: at-least-once, stated plainly

**The server may see the same commit twice.** A commit that reached the
server and succeeded, whose acknowledgement was lost with the connection,
is indistinguishable at the client from one that never arrived — the reply
frame is gone either way. `reject_pending` (`rpc.vl:463`) is explicit about
this already: "the reply may exist server-side; the CALLER decides whether a
retry is safe — never a blind re-send." Auto re-push makes `Draft` a caller
that has decided.

Is `Draft`'s own reconcile idempotent? **Verified: yes, on both halves.**

- `adopt` (`reactive.vl:645`) is idempotent: `remote == synced` is an
  explicit no-op arm, so the mirror echo of a doubly-applied commit changes
  nothing, and applying it twice is the same as once.
- `settle` (`reactive.vl:629`) is generation-guarded: the re-push bumps
  `generation`, so if the *original* commit's reply somehow still lands it
  is discarded, and only the newest push settles the state. A duplicate can
  never resolve the cell twice.

Is the **commit closure** idempotent? `Draft` cannot make it so, and this is
the one thing to state honestly in the docs rather than paper over. For the
shape `Draft` is built for — "set the remote field to this value",
last-write-wins, which is what `bind_draft` produces — a repeat is naturally
idempotent and this is a non-issue. For a commit that *appends* (a log, an
audit trail, a counter increment), at-least-once means a duplicate entry.
Such a commit was already unsafe under `Draft`'s existing retry story ("the
next push retries naturally", `reactive.vl:566`); re-push widens the window,
it does not open it. Documented at the `repush` docstring and in the guide.

### Failure of the re-push itself: once, riding reconnects

A re-push that fails does exactly what any failed push does — `Failed(reason)`,
local kept, `synced` unadvanced — and **nothing retries it on a timer.** The
next reconnect re-pushes it again, because `local != synced` is still true.

The reasoning: `Draft` cannot tell a transport fault from a server rejection
(§3.1), so a timed retry loop would hammer a server that is *correctly and
permanently* refusing the value, forever, for every open draft. Riding
reconnects means a retry happens only when something observably changed
about the world. A user's next keystroke also retries, as it always has.

## 4. Opt-in or default — opt-in, and it is not a preference

The question was posed as a judgement call ("a default that re-pushes
silently changes wire behavior for existing apps"). The investigation makes
it structural instead: **there is no default available to give.** §1's
finding — a `Draft` holds an opaque commit closure and has no reference to
any transport — means a `Draft` constructed by `draft(initial, commit)`
*cannot* subscribe to a reconnect it has no way to name. The wiring must be
written where both values are in scope, and that is app code:

```vilan
let title = draft(page.title, |value: str| { .. client.rename(value) .. });
client.transport.on_reconnect(|| title.repush());
```

So the conservative outcome and the only implementable one coincide. No
existing app changes a byte on the wire until it writes that line.
`repush()` is also useful on its own — a "retry" button in a failure banner
is `|| title.repush()` — which is a second reason to make the *primitive*
the public thing and the reconnect wiring the composition.

**One thing does deserve the owner's eye, recorded here and in the report,
not silently decided:** `bind_draft` (`browser/ui.vl:264`) commits on every
keystroke, and a case can be made that *it* should apply a small default
debounce, since a per-keystroke commit is what §5 exists to fix and every
bound input has the same problem. This arc does **not** do that — it would
change the wire behavior of every existing bound input, which is precisely
the change §4 declined to make elsewhere — but "should `bind_draft` debounce
by default, and at what window?" is a product call about the framework's
out-of-the-box feel, not a correctness question, and it is the owner's.
The mechanism is in place either way: it is a one-line default if wanted.

## 5. The debounced variant

### What triggers a commit today

`push(value)` (`reactive.vl:613`) commits **every call**, unconditionally:
set `local`, mark `Dirty`, bump `generation`, spawn `settle`. Bound to an
input via `bind_draft`, that is one commit per keystroke. The generation
guard makes it *correct* under fast typing (only the newest settles) but it
does not make it *cheap*: every keystroke is a frame on the wire.

### Shape: trailing-edge, timer-backed, opt-in per draft

```vilan
let title = draft(page.title, commit).debounce(300);
```

`debounce(millis)` sets the window and returns `self` for chaining. The
window lives in a `Shared<i32>` like every other field on the cell, so a
copy of the draft is the same draft (the discipline `Signal`, `Shared` and
`Timer` all follow) — there is no way to end up holding two `Draft` values
that disagree about their own window. `0` is the default and is exactly
today's behaviour, byte for byte: the debounced path is not entered at all.

Semantics:

- **Local-first is untouched.** `push` still sets `local` and marks `Dirty`
  synchronously, before anything else. Debouncing delays the *commit*, never
  the keystroke — an input never waits, which is `Draft`'s whole premise.
- **Trailing edge.** The commit fires `millis` after the *last* push. Three
  keystrokes inside the window produce one commit, carrying the value as of
  the moment it fires (`local.get()`, not the value captured at arming — the
  point is to send the latest).
- **The timer is a real `Timer`** (`std::time`), not a generation counter:
  a superseding push `cancel()`s the outstanding one, which settles its
  verdict `false` and clears the host timeout. This matters beyond
  tidiness — `Timer`'s own docs note a pending timer keeps node alive, so a
  counter-based debounce would leave a program unable to exit for the length
  of its window.
- **`commit()` cancels a pending debounce and sends now.** The explicit
  "save" — a blur handler, a Save button, a form submit. Exactly one commit
  results: the pending timer's verdict is `false` and its parked task
  retires without touching the wire.
- **`repush()` also cancels the pending debounce and sends now.** A
  reconnect is recovery, not typing; the window exists to spare the wire
  during a burst of edits, and there is no burst in progress that matters
  more than getting the user's work to a server that just came back. If a
  timer *were* left pending, it would fire into the same value moments later
  for a second, pointless commit.

### Interaction with reconnect, spelled out

- Debounce pending when the connection drops → the timer fires, the commit
  fail-fast rejects, state is `Failed`, `local != synced`. The reconnect
  re-push then sends it. Nothing is lost.
- Reconnect while a debounce is pending → `repush` cancels it and sends
  immediately (above). One commit, not two.
- Mirror resync arriving before the re-push (the ordering §2 buys) →
  `adopt(remote)` takes the dirty branch, records the remote in `synced`,
  leaves the user's text alone; the re-push then knowingly overwrites, which
  is `Draft`'s documented last-write-wins conflict rule, unchanged.

## 6. Surface added

```vilan
// std::reactive
impl Draft<type T: PartialEq> {
    fun debounce(self, millis: i32): Draft<T>;   // coalescing window; 0 = off
    fun commit(self);                            // send now, cancel any pending window
    fun repush(self);                            // send iff local != synced
}

// std::rpc
impl SocketTransport {
    fun on_reconnect(self, hook: async || void); // runs after mirrors resync
}
```

Two private helpers on `Draft` carry the split that used to be inline in
`push`: `send(value)` (bump the generation, spawn `settle` — today's tail of
`push`) and `cancel_pending()`.

`Draft` is single-copy in the shared `reactive.vl`, so this lands once;
`browser/ui.vl`'s and `process/ui.vl`'s `bind_draft` are unaffected (they
call `push`, whose signature and local-first behaviour are unchanged).

## 7. Not in scope (recorded)

- **SSE (`SplitDuplex`) has no reconnect at all** — its pump `jump break`s on
  disconnect and no one is told (`rpc.vl:253-262`). A draft on the SSE leg
  therefore gets no re-push, because there is no reconnection to ride. This
  is not a new gap; it is `transport-robustness.md` §4's recorded beyond-v1
  item ("the SSE fallback keeps fail-fast semantics") seen from the draft
  side. Noted so the docs can say which transport the feature needs.
- **Leading-edge / `maxWait` debounce.** Trailing-edge is what coalescing
  keystrokes wants; a leading edge would send the first character of every
  burst. If a "commit at least every N ms while typing" appears in a real
  app, it is an additive second window.
- **Per-draft retry schedules.** §3 rides reconnects on purpose.
