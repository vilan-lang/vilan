# Remote sources — subscribe by demand, unsubscribe at zero (A25)

> Status: **BUILT 2026-08-19** on lane `a25-remote-sources` — all three
> slices, ship record in §8 (one open item there: the six corpus goldens
> the std change moves, stopped rather than regenerated on the lane).
>
> Prior: **RATIFIED 2026-08-19 as recommended** ("Recommendations in
> remote-sources.md and docs-port.md look good") — §6's four answers
> stand: **Q1** `sub` keeps `|T|`, no second public counted entry point in
> v1; **Q2** no `Stale` in v1 (`Waiting`/`Ready` from the cache alone);
> **Q3** the arms are `Waiting`/`Ready` and the seam is `or(initial)`;
> **Q4** the `Unsubscribe` is deferred to turn settle (`at_settle`). The
> design of §2 is the spec; §5's three slices are the build order, each
> gated as written there; pin **C** un-ignores as-is (it asserted exactly
> this API). Tracker: `backlog-2026-08-18.md` §A item 25.
>
> Prior status: DRAFTED 2026-08-18, awaiting ruling. The brief was
> `backlog-2026-08-18.md` §A item 25, including the owner's two
> refinements of the same day (a fallback *without* having to supply an
> initial; "status", not "state"). Three `#[ignore]`d pins stand in
> `crates/vilan-core/tests/inference.rs` (§5); two of them assert facts no
> ruling changes, the third asserts this paper's API.

## 0. The thesis

A remote mirror is a network resource, and today nothing in the language
knows when the app is done with it. `RemoteSource::sub` opens a channel;
nothing ever closes one. **Subscription should follow demand exactly: the
channel is open while — and only while — something is watching.**

Three claims carry that:

1. **The count rides ownership.** Every path that observes a mirror takes
   a lease and increments a count; releasing the lease decrements it. The
   0→1 transition sends `Subscribe`, the 1→0 transition sends
   `Unsubscribe`. Nothing new has to track scopes: `Owner`
   (`vilan/std/src/reactive.vl` 261–293) already disposes a view's
   registrations at unmount, and `mount_root` already runs its body under
   one (`vilan/std/src/browser/ui.vl` 698–715). An unmount decrements for
   free — no `getListener`, no reactive-scope reader.
2. **One honest seam.** `RemoteSource` stays bespoke — `get(): Option<T>`
   is the truth before the first frame — and gains exactly one bridge into
   the ordinary reactive world: `map`, which confronts the `Option` once
   and yields a `Signal<U>`. `or` is sugar over it. Everything downstream
   (`bind_each`, `bind_text`, `{…}` embedding) already takes a `Signal`,
   so the seam costs no new machinery anywhere else.
3. **Passive stays passive.** `get()` and `status()` read local knowledge
   and open nothing. The honest sentence, stated once and repeated in the
   docs: *any path that renders the value when it arrives subscribes; the
   count is what makes that harmless.*

## 1. The surface today, and the two facts

`RemoteSource<T>` is `vilan/std/src/rpc.vl` 1242–1293: a handle over
`cache: Signal<Option<T>>` with `get(): Option<T>` (1266–1269),
`[must_use] sub(observer: |T| void): Subscription` (1271–1281) and
`rebind(channel, subscribe)` (1286–1292). `ReactiveClient::source<T>`
(1188–1217) builds one per channel; the server half's capability table,
`start` (1100–1110), `stop` (1112–1124) and the control-frame router
(1126–1145) sit under `ReactiveServer` (1057–1145); the
`[service(Client)]` macro emits a `RemoteSource<{element}>` field per
`[expose]`d field (1593, 1646).

### 1.1 Fact one: no `Unsubscribe` is ever sent

**Statically.** The client's only frame builder for the control half is
`encode_control(codec, kind, channel)` (`rpc.vl` 982–988). It has exactly
two call sites in the entire tree, and both pass `"Subscribe"`:

- `rpc.vl:1212` — `ReactiveClient::source`, the pre-encoded frame stored
  in the mirror.
- `rpc.vl:1334` — `reattach_mirrors`, the reconnect path.

`"Unsubscribe"` appears in exactly three places tree-wide: two comments
(`rpc.vl:975`, `rpc.vl:1126`) and the server's match arm
`"Unsubscribe" => self.stop(channel)` (`rpc.vl:1139`). **`ReactiveServer::stop`
(1112–1124) is unreachable in every shipped path.** `Subscription::dispose`
(`reactive.vl` 227–256) removes a subscriber from a `Signal`'s list; it has
no reach into a transport, and `RemoteSource::sub` hands back exactly that
`Subscription` — the one for the local `cache`, not for the channel.

**Measured.** A frame-logging relay between two `duplex_pair` ends (the
harness of §5's pin A; the same shape as `inference.rs`'s in-process rpc
tests) run through this worktree's `target/debug/vilan`:

```
up   {"Subscribe":0}
down {"Update":[0,0]}
down {"Update":[0,1]}
down {"Update":[0,2]}
```

The program subscribes, sets 1, **disposes**, sets 2. The third `Update`
crosses the wire after the only subscription was disposed. No frame ever
goes up but the one `Subscribe`. `remote.get()` after the dispose still
reads `Some(2)` — the mirror is not merely still billed, it is still being
maintained.

### 1.2 The unreported consequence: a second watcher duplicates the forward

Falling out of the same probe, and worth its own pin. `ReactiveServer::start`
(1100–1110) pushes a **new** live forward per `Subscribe`, unconditionally.
`RemoteSource::sub` sends a `Subscribe` on **every** call (1274). So two
local watchers open two server-side forwards on one channel, and every
subsequent value crosses the wire twice. Measured, two watchers on one
`Signal<i32>`:

```
A sees 0
-- one watcher: set 1 --
A sees 1
A sees 1          <- B's Subscribe re-delivered the current value to A
B sees 1
-- two watchers: set 2 --
A sees 2
B sees 2
A sees 2          <- two forwards, two Update frames, two cache writes
B sees 2
-- B disposed: set 3 --
A sees 3
A sees 3          <- still two forwards: dispose closed nothing
```

Two distinct defects, both dissolved by the count: a second `sub` sends no
frame at all when the count is already ≥1, so no second forward is opened
and no existing observer is re-fired with an unchanged value.

### 1.3 Fact two: `RemoteSource` is not a `Source`, so consumers hand-mirror

`Source<T>` (`reactive.vl` 349–362) requires `get(self): T`.
`RemoteSource::get` returns `Option<T>`, so it cannot conform, and no
binder accepts it. Every consumer therefore builds a local `Signal`,
subscribes a never-disposed `sub` into it, and passes the local signal
downstream. The census across std, the examples, the playground and kolt:

| Site | Mirrors |
|---|---|
| `vilan-playground/todo/src/client.vl` 15, 21 (`Signal::new([])` + `_sync`) | 1 |
| `vilan/examples/walkthrough/src/client.vl` 17, 23 | 1 |
| `vilan/examples/todo/src/client.vl` 18, 26 | 1 |
| `kolt/src/client.vl` 19–20, 26–27 | 2 |
| `kolt/src/probe.vl` 17–18, 21–22 | 2 |

**Five files, seven hand mirrors, zero disposals.** Two doc fences teach
the same shape (`vilan/docs/guide/walkthrough.md:239`,
`vilan/docs/guide/services.md:101`) and the repo `README.md:64` markets it.
Two benchmark files mirror into a `Shared` rather than a `Signal`
(`vilan/benchmarks/src/coalescing.vl:38`, `realtime.vl:83`) — the same
idiom, one type down. std itself has none: the only `Signal`+`sub` pairing
inside std is the primitive's own cache (`rpc.vl` 1192, 1275).

kolt is the densest site, and it is the app the owner's note came from.

### 1.4 Two constraints the design must respect (both measured)

- **`impl Trait` in return position does not exist.** The brief's sketch
  `or(initial: T): impl Source<T>` is a parse error today: *"found 'impl'
  expected a type in return type"*. There are zero `): impl ` returns in
  std, the corpus or the tests, and nothing in `specification.md`. Every
  seam this paper proposes therefore returns a **concrete** type.
- **Nothing downstream consumes `Source<T>` generically.** `Source` is a
  bound in exactly one place in std — `ReactiveServer::expose<T: Wire, S:
  Source<T>>` (`rpc.vl:1087`), the *server* half. Every consumer takes a
  concrete `Signal`: `bind_text` (`browser/ui.vl:206`), `bind_class` (:215),
  `bind_styled` (:228), `bind_attr` (:237), `bind_value` (:247),
  `bind_each` (:288), `show` (:441), and both element-syntax slots —
  `impl Signal<str> with Slot` (:619) and `impl Signal<str> with AttrValue`
  (:652). Conforming to `Source` buys the `effect` default (`reactive.vl`
  359–361) and nothing else.

## 2. The design

### 2a. The count

`RemoteSource` replaces `wanted: Shared<bool>` (`rpc.vl` 1258–1260) with a
count. The cell placement is already right and already explained in the
tree: the fields are `Shared` cells precisely so that every copy of the
handle shares one state (`rpc.vl` 1252–1254), which is what makes a count
on a value-copied struct correct.

```vilan
struct RemoteSource<T> {
	channel: Shared<i32>,
	subscribe: Shared<Frame>,
	// Live demand: how many leases are outstanding. Replaces `wanted` —
	// `wanted` is now `count.read() > 0`. Cells, so every copy of the
	// handle counts into the same total.
	count: Shared<i32>,
	// A 1->0 that has not yet been flushed. Cancelled by a 0->1 in the
	// same turn: the channel never actually closed.
	closing: Shared<bool>,
	transport: DuplexEnd,
	cache: Signal<Option<T>>,
}
```

The rule, in four lines:

- **0→1**, no pending close: send `Subscribe`.
- **0→1**, close pending: clear `closing`, send nothing — the channel is
  still open server-side.
- **1→0**: set `closing` and defer the `Unsubscribe` to the ambient turn's
  settle (no ambient turn → send it inline).
- **at settle**: if `closing` is still set, send `Unsubscribe` and clear it.

**Where the count lives:** in the mirror's cells, alongside `channel` and
`subscribe`, for the reason those two are cells. It is **client-local
demand**, not wire state, which is exactly why a reconnect cannot
double-count it: `rebind` (`rpc.vl` 1286–1292) re-sends `Subscribe` iff
`count.read() > 0` and never touches the count. One correction the
reconnect needs: `rebind` must clear `closing`, because a pending
`Unsubscribe` names the *old* channel on a dead connection — flushing it
after a rebind would either be a no-op frame on a closed socket or, worse,
name a channel id the fresh session has since minted for something else.

**Interaction with turn coalescing.** Transport sends are *not* turn-deferred
today — measured: a `sub` inside a `batch(…)` puts its `Subscribe` on the
wire synchronously, inside the batch body. Only signal *notifications*
defer (`Signal::notify`, `reactive.vl` 410–424). The asymmetry above is
deliberate:

- The `Subscribe` is **latency-critical** — the caller wants the value now,
  and the server's immediate current-value `Update` is how the mirror seeds.
  Deferring it to settle would delay every first paint by a turn.
- The `Unsubscribe` is **pure economy**. It can afford to wait, and waiting
  is what makes the common case free.

The brief asks what a `sub`+`dispose` within one turn sends. **Two frames:
the `Subscribe` eagerly, the `Unsubscribe` at settle.** Once the `Subscribe`
is out the `Unsubscribe` is owed — sending neither would leave the server
forwarding to a client that stopped listening, which is the exact bug this
paper exists to close. The case the deferral actually buys is the other
order, **1→0→1**, which sends **zero** frames: a re-render that disposes a
view and rebuilds it — `bind_each` refreshing a row disposes the row's owner
and re-runs `render` (`browser/ui.vl:288`; the per-row owner at 331–335) — would otherwise churn
`Unsubscribe`+`Subscribe` on every keystroke, *and* would hit §1.2's
duplicate-forward defect on the way back up.

The deferral needs no new turn machinery. `Turn` (`reactive.vl` 59–66) has
no settle hook, but it does not need one: the flush action can ride the
pending queue as an ordinary `Subscriber`, deduped by id exactly like a
notification (`enqueue`, `reactive.vl` 99–120), which also collapses a
1→0→1→0 flutter into one flush. One small addition to `reactive.vl` —

```vilan
/// Run `action` when the ambient turn settles; with no ambient turn, now.
/// Deduped by `id`, so repeated deferrals of the same action flush once.
fun at_settle(id: i32, action: || void) { … }
```

— mirrors `Signal::notify`'s own `get_safe`/`draining_turns`/inline cascade
(410–424) and is the only new reactive primitive this paper asks for.

**The `Subscription` problem, and why it is one line.** `Subscription`
(`reactive.vl` 222–226) is a concrete struct of `{subscribers, id}` whose
`dispose` filters a subscriber list and scrubs the turn queue. It cannot
carry the decrement. It has exactly **one** construction site in the whole
tree — `reactive.vl:387` — so giving it an optional release hook is a
one-line change:

```vilan
struct Subscription {
	subscribers: Shared<List<Subscriber>>,
	id: i32,
	// Extra teardown to run on dispose, once. `Signal::sub` leaves it
	// `None`; a counted `RemoteSource::sub` puts its decrement here.
	release: Shared<Option<|| void>>,
}
```

The alternative — a bespoke `RemoteSubscription` implementing `Disposable`
— also works with `[must_use]`, `Owner::take<T: Disposable>`
(`reactive.vl` 272–277) and `get_owner().take(…)`, but it would forbid
`RemoteSource` from ever conforming to `Source`, whose `sub` returns
`Subscription` by name (`reactive.vl:352`). Recommend the hook: it is
smaller, and it keeps §2i's door open rather than nailing it shut as a
side effect.

### 2b. `map` — the fallback tool

```vilan
impl RemoteSource<type T> {
	/// Observe the mirror through `transform`, as a plain `Signal<U>`.
	/// COUNTED: this is a subscription — it opens the channel if nothing
	/// else has, and it is released when the enclosing owner is disposed,
	/// which for a view is unmount. `transform` sees `None` until the
	/// first frame lands, which is where a fallback of a DIFFERENT type
	/// than `T` belongs.
	fun map<U>(self, transform: sync |Option<T>| U): Signal<U> {
		let derived = Signal::new(transform(self.get()));
		let lease = self.lease(|value| {
			derived.set(transform(value));
		});
		get_owner().defer(|| lease.dispose());
		derived
	}
}
```

The owner's counter case:

```vilan
<p>{client.count.map(|value| match value {
	Some(let n) => i"{n}",
	None => "Loading...",
})}</p>
```

`map` is the *only* place the `Option` is confronted, and confronting it is
what buys the fallback. `U` is free of `T`, which is the whole point of the
owner's refinement: `Signal<str>` from a `RemoteSource<i32>`, `"Loading…"`
from `None`.

**`map` requires an enclosing owner scope, statically.** `get_owner()`
reads `owner_scope`, and reading it where no `owner_scope.run` encloses is
a compile error, not a runtime absence (`ambient-owner.md` §1). Verified in
this worktree: calling a `map`-shaped helper outside any scope fails with

```
Error: context `owner_scope` is read here, but this code can be reached
without an enclosing `run`
```

and calling the same helper *one plain function call down* from inside
`owner_scope.run` compiles and runs — coverage propagates through the call
graph, which is why `app(client, …)` inside `mount_root`'s body is a legal
home for a `map`. This is a **feature**, not a wart: it makes "a network
subscription must have an owner" a compile-time law, and it is the reason
§4's migration moves the mirror out of `main` and into the view.

The whole mechanism was type-checked and run end to end against this
worktree's compiler as a standalone model (a `Mirror<T>` with the same
cells, lease and `map`):

```
count before any scope = 0
text before arrival = Loading...
count under scope = 1
text after arrival = 41
count with two consumers = 2
count after unmount = 0
frames = 2
  Subscribe
  Unsubscribe
get() = Some(41)
```

Two consumers, one `Subscribe`. Unmount, one `Unsubscribe`. `get()`
untouched by either.

### 2c. `or` — sugar, not a second mechanism

```vilan
	/// `map` for the same-type case: `initial` until the first frame.
	/// Counted and owner-released exactly like `map`, because it IS `map`.
	fun or(self, initial: T): Signal<T> {
		self.map(|value| match value {
			Some(let present) => present,
			None => initial,
		})
	}
```

`Signal<T>`, not `impl Source<T>` — §1.4. The brief's `impl Source<T>` is
unparseable, and `Signal<T>` is strictly better anyway: it is the type
`bind_each`, `bind_text` and `Slot` actually take.

**The honest wart, named rather than hidden.** A `Signal<U>` handed back
from `map`/`or` has `set` on it, and writing it writes only the local
derivative. That is not new and not this paper's to fix: `Signal::map`
(`reactive.vl` 448–457) already returns a writable derived signal, and
`combine` (464–473) and `flatten` (483–494) do too. A25 inherits the
existing rule rather than inventing a second one.

### 2d. `status()` — passive, and the sentence that goes with it

```vilan
[derive(PartialEq, Debug)]
enum Status {
	/// Nothing has arrived. Note: this is what an UNWATCHED mirror reads,
	/// forever — see the sentence below.
	Waiting,
	/// The cache holds a value.
	Ready,
}

impl RemoteSource<type T> {
	/// What is locally KNOWN about this mirror. Passive: opens no channel,
	/// takes no lease, needs no owner. Derived from the cache alone.
	fun status(self): Signal<Status> {
		self.cache.map(|value| match value {
			Some(let _present) => Status::Ready,
			None => Status::Waiting,
		})
	}
}
```

`Signal<Status>` (§1.4 again: no `impl Source`), built on the ordinary
`Signal::map`, so it inherits that combinator's unowned local subscription
— a subscriber on a cell the mirror owns anyway, no wire, no owner
requirement. The asymmetry with `map` is the design, not an inconsistency:
**`map` costs network and therefore demands an owner; `status` costs
nothing and therefore demands nothing.** `status()` is callable at the top
of `main`; `map` is not.

**The honest sentence, to be stated in the paper, the doc comment and
`docs/std/rpc.md` alike:** *a `status` observer alone never sees
`Waiting → Ready`. `status` reports; it does not ask. If nothing else is
subscribed, the mirror stays `Waiting` forever, and that is correct — the
channel was never opened.*

**`Stale` is recommended OUT of v1**, on a plumbing fact rather than a
taste. Transport loss is knowable — `SocketDuplex.state: Signal<ConnectionState>`
(`rpc.vl:418`, enum at 364–368) surfaced as `SocketTransport::connection_state()`
(`rpc.vl:533`) — but a `RemoteSource` holds a bare `DuplexEnd`
(`rpc.vl:1261`), two handler cells with no notion of a connection
(`rpc.vl` 109–112), handed to `ReactiveClient::new` as `bridge(socket)`
from the generated `connect`. `ReactiveClient` is also constructed over a
raw `duplex_pair` in-process, where "connection state" has no meaning. So
`Stale` costs an optional `Signal<ConnectionState>` threaded through
`ReactiveClient` into every mirror, for an arm the app can already compose
itself today:

```vilan
let health = combine((mirror.status(), client.transport.connection_state()));
```

— the exact pairing `crates/vilan-cli/tests/transport_robustness.rs:381`
already does by hand. Ship `Waiting`/`Ready`; add `Stale` when an app asks
and pay the plumbing then. Owner question §6 Q2.

### 2e. `get()` is unchanged

`get(): Option<T>` (`rpc.vl` 1268–1270) stays exactly as it is: the passive
snapshot, no lease, no frame, `None` before the first `Update`. It is the
reason `RemoteSource` is honest, and the reason it is not a `Source<T>`.

### 2f. Embedding reaches the mirror only through the seam

`<p>{expr}</p>` lowers to `.child(expr)` (`element-syntax.md` §4's table),
and `child<C: Slot>` (`browser/ui.vl:191`, `process/ui.vl:166`) dispatches
on the value's type. The shipped `Slot` impls are `View`, `str`,
`Signal<str>` and `List<View>` (`browser/ui.vl` 598–636). So
`{mirror.or("…")}` and `{mirror.map(…)}` land on `impl Signal<str> with
Slot` (:619) with **no new impl at all** — the seam is already there.

**Recommend against `impl RemoteSource<str> with Slot`.** It would work
mechanically — `place` always runs inside a view body, so it always has an
owner to release the lease into — and it is rejected for two reasons:

1. **It reintroduces the sentinel the type system just removed.** With no
   value yet, a direct `Slot` impl has nothing to render but `""` — the
   very `""` sentinel `Option<T>` replaced (`rpc.vl` 1242–1246 says so in
   as many words). The fallback would be silently empty instead of chosen.
2. **It hides the cost behind a bare name.** `element-syntax.md` §5's own
   rule is that *the value's type carries the distinction* and that
   `<p>{status}</p>` is reactive "because `status` is a `Signal<str>`,
   visible in the source". A bare `{client.notes}` that opens a socket is
   the opposite of that, and it is precisely what the owner objected to:
   "I don't want exposed signals to auto-subscribe and carelessly waste
   network when it's not even needed."

`impl Option<T> with Display` is rejected harder: it is a global decision
about `Option` far outside A25's blast radius, it would render `"None"` or
`""` as a fallback nobody chose, and there is no `impl Signal<T: Display>
with Slot` for it to compose with anyway — only `Signal<str>` is a slot.
The type `map` returns is `Signal<str>` for exactly this reason.

### 2g. The generated client is unchanged

`[service(Client)]` keeps emitting `RemoteSource<{element}>` fields
(`rpc.vl:1593`) and `let mirror_{field} : RemoteSource<{element}> =
reactive.source(channels[i])` in `connect` (`rpc.vl:1646`), and keeps
registering `rebind` positionally (1648, 1653–1660). No macro change; the
count and the seams are all inside the type the macro already names.

### 2h. `sub` keeps `|T|`

Recommend **keeping** `sub(observer: |T| void)`. Reasons:

- **Source compatibility.** Seven mirrors, two doc fences and the repo
  README call it; widening to `|Option<T>|` breaks all of them for no gain
  at the call sites that exist (every one of them wants the present value).
- **The name means what it says.** `sub` is present-only by construction —
  `rpc.vl` 1275–1280 already discards the `None` arm.
- **The `Option` is confronted exactly once**, in `map`, which is the
  backlog's own stated goal.

Mechanically `sub` and `map` share one counted primitive underneath — a
private `lease(observer: |Option<T>| void)` — so there is one count and one
place the frames are decided; `sub` is the present-only face of it. Whether
`lease` should also be *public* (an `observe(|Option<T>|)` for the caller
who wants arrivals without a derived signal) is §6 Q1.

### 2i. Bespoke, with a seam — and what the alternatives actually cost

**Recommendation: `RemoteSource` stays bespoke and gains `map`/`or`/`status`.**

- **`Source<T>` — rejected.** `get(self): T` (`reactive.vl:350`) has no
  honest answer before the first frame. Conforming means fabricating one:
  a `Default`, a caller-supplied initial baked into the handle, or a
  panic. It puts the lie in the *type*, where every consumer inherits it,
  to save one `.or(…)` at the one place the app already knows its fallback
  (`Signal::new([])` in all five mirror sites is that fallback, written by
  hand today).
- **`Source<Option<T>>` — honest, and buys almost nothing.** It is
  type-correct and would give `effect` for free. But per §1.4 no consumer
  in std takes a `Source` — every binder and both element-syntax slots
  take a concrete `Signal` — so conformance connects `RemoteSource` to
  nothing, while pushing `Option` into every observer signature
  (`sub(|Option<T>|)`, §2h) and every downstream `map`. It also pins `sub`
  to returning `Subscription` by name (`reactive.vl:352`), which forecloses
  the bespoke-disposable option in §2a. Cost real, benefit nil, *today*.
- **Bespoke + one seam — recommended.** `map` yields `Signal<U>`, which is
  the type the whole framework already speaks. Nothing new implements
  anything; `bind_each`, `bind_text`, `show` and `{…}` all just work.

Note the door stays open: if a future widening makes the binders generic
over `Source`, `RemoteSource` can conform *then*, as `Source<Option<T>>`,
without invalidating anything here. Conforming now would be paying for a
seam that does not exist yet.

## 3. Wire and server consequences

`Unsubscribe` frames start flowing for the first time, which means
`ReactiveServer::stop` (`rpc.vl` 1112–1124) gets its first real traffic
ever. Reading it against that:

- **`stop` itself is correct, and conveniently over-strong.** It disposes
  *every* live forward whose channel matches and keeps the rest — so even
  if a client somehow opened two forwards on one channel (§1.2), one
  `Unsubscribe` cleans up both. No change needed.
- **`start` should be made idempotent anyway** (`rpc.vl` 1100–1110). Under
  a counted client the duplicate can no longer originate, but the server
  should not depend on a well-behaved client for a correctness property:
  a second `Subscribe` for a channel that already has a live forward
  should be a no-op (or a re-seed of the current value, if the immediate
  `Update` is judged worth keeping). This is the root-cause fix for §1.2
  and it is independent of the ruling.
- **Reconnect.** A reconnect mints new channel ids and a new server session
  (`transport-robustness.md` §2.5, §3), and `drop_session` disposes the old
  session's forwards on close (`rpc.vl` 1014–1027). So the count needs no
  server cooperation: `rebind` re-`Subscribe`s iff `count > 0` and the
  fresh session starts one forward. The one gap is client-side and named in
  §2a — **`rebind` must clear `closing`**, or a deferred `Unsubscribe` for
  the pre-reconnect channel flushes against the new connection.
- **Ordering, `__attach` and the hook.** The reconnect hook runs after
  `state = Connected` by design (`transport-robustness.md` §2.2), so a
  mirror's `Subscribe` and a deferred `Unsubscribe` cannot interleave with
  `__attach`: `reattach_mirrors` (`rpc.vl` 1305–1347) re-verifies the
  contract, re-`__attach`es, then rebinds. Clearing `closing` inside
  `rebind` puts the fix on the right side of that ordering.
- **No protocol change.** `Unsubscribe(channel)` is already in the frame
  vocabulary (`rpc.vl:975`), already encoded by `encode_control`, already
  routed (`rpc.vl:1139`). The wire does not move; a dead arm comes alive.
- **Pin gaps.** No test anywhere observes a reactive frame — Subscribe,
  Update or Unsubscribe. The word appears once in the whole suite, in a
  comment (`crates/vilan-cli/tests/rpc_http.rs:355`). Every existing
  reactive assertion is a `stdout.contains(…)` on what the program printed.
  §5's pins close that gap at its cheapest point.

## 4. Migration

### 4.1 The playground todo client

`vilan-playground/todo/src/client.vl` today (15–25) and after. The mirror
moves out of `main` and into the view, which is what the ownership law of
§2b requires and what makes the count mean anything:

```diff
 async fun main() {
-	let notes: Signal<List<Note>> = Signal::new([]);
 	let token = Signal::new(storage::get("notes-token"));
 	let route = current_path().map(parse);

 	match NotesClient::connect("/", json_codec()) {
 		Ok(let client) => {
-			let _sync = client.notes.sub(|x| notes.set(x));
-			let _root = mount_root("app", || app(client, notes, token, route));
+			let _root = mount_root("app", || app(client, token, route));
 		}
 		Err(let error) => print(i"connect failed: {error.debug()}")
 	}
 }

 fun app(
 	client: NotesClient<SocketTransport>,
-	notes: Signal<List<Note>>,
 	token: Signal<str>,
 	route: Signal<Route>,
 ) {
 	let note_name = Signal::new("");
+	// Counted, and released when this view is unmounted. `[]` is the
+	// fallback that used to be `Signal::new([])` two frames up.
+	let notes = client.notes.or([]);

 	<div>
 		…
 		<ul .bind_each(notes, |x| x.id, |note| <li>{note.text}</li>) />
 	</div>
 }
```

Net: three lines and a parameter gone, `Signal` no longer imported for
this purpose, and the subscription now dies with the view instead of with
the tab. The `[]` the app already supplied as an initial becomes the `or`
argument verbatim — the `Option` is confronted exactly once, where it
already was.

### 4.2 The other sites

- `vilan/examples/walkthrough/src/client.vl` 17–24 and
  `vilan/examples/todo/src/client.vl` 17–28 — the same diff, same shape.
- `kolt/src/client.vl` 19–28 — two mirrors, same diff twice; `items` and
  `tasks` become `client.workspaces.or([])` and `client.tasks.or([])`
  inside `screen`.
- `kolt/src/probe.vl` 16–22 — a `main`-level probe with no view and no
  owner. `map` is illegal there by §2b, correctly: the probe wants the
  *manual* form, and `sub` + an explicit `dispose` is exactly right for it.
  It should keep `sub` and gain the disposals it lacks. **This is the case
  that proves `sub` must survive `map`'s arrival.**
- `vilan/benchmarks/src/coalescing.vl:38`, `realtime.vl:83` — mirror into
  a `Shared`, not a `Signal`; they are measuring frame throughput and
  should keep `sub`. `realtime.vl:83`'s `_watching` should be disposed.

### 4.3 Docs

Every page that teaches the mirror needs the honest sentence and the new
seam, in the same change-set as the std change (`AGENTS.md` gate 3):

- `vilan/docs/guide/services.md:101` — `let _sync = client.entries.sub(…)`,
  the primary teaching site.
- `vilan/docs/guide/walkthrough.md:239` — the hand-mirror fence, mirrors
  `examples/walkthrough`.
- `vilan/docs/std/rpc.md:17` — **already wrong today**, independently of
  A25: it documents the generated field as `client.some_signal: Signal<T>`.
  It is a `RemoteSource<T>` (`rpc.vl:1593`), which is the entire reason
  §1.3's hand-mirroring exists. Fix in S3 regardless of the ruling.
- `vilan/docs/guide/reactive.md` 140–143 — needs the counted-`sub` note.
  Also **wrong today**: it says `sub` "fires only on *later* changes", but
  `Signal::sub` calls `observer(self.get())` before returning
  (`reactive.vl:386`). Same S3 sweep.
- `README.md:64` — the marketing fence, `let _sync = client.notes.sub(…)`.

## 5. Slices

Each slice is gated by the targeted binary, not the suite. The e2e
harness note that shapes all three: `crates/vilan-core/tests/inference.rs`
(`assert_compiles_and_runs`, `:991`) already drives the full reactive
protocol in-process over `duplex_pair` with no server, no port and no
`vilan` binary — `service_generates_dispatcher_client_and_mirror` (`:9822`)
is the precedent. The CLI suites (`rpc_http.rs`, `transport_robustness.rs`)
each cost a real compile plus one to three node children per test, and the
reconnect legs are `#[cfg(unix)]`-only. **Frame-level pins belong in
`inference.rs`**; the CLI suites stay the reconnect gate.

### S1 — the count and the `Unsubscribe`

`Subscription.release` (`reactive.vl:222`), `at_settle` (`reactive.vl`,
new), `RemoteSource.count`/`closing` replacing `wanted` (`rpc.vl` 1258–1260),
the counted `lease` under `sub`, `rebind` clearing `closing`
(`rpc.vl:1286`), and `ReactiveServer::start` made idempotent
(`rpc.vl:1103`).

- Gate: `cargo test -p vilan-core --test inference`, then
  `cargo test -p vilan-cli --test rpc_http` and
  `--test transport_robustness` (the reconnect legs are the only place
  `rebind` runs for real).
- Pins: **A** and **B** below un-ignore here. Add: a reconnect pin that a
  `closing` flush cannot cross a rebind; a pin that `sub`+`dispose`+`sub`
  inside one `batch` puts exactly one `Subscribe` and no `Unsubscribe` on
  the wire.

### S2 — `map`, `or`, `status`

`map`/`or`/`status` on `RemoteSource`, the `Status` enum, doc comments
carrying the honest sentence.

- Gate: `cargo test -p vilan-core --test inference` and
  `cargo test -p vilan-core --test docs`.
- Pins: **C** below un-ignores here. Add: `or` returns the initial before
  the first frame and the value after; two `map`s under one owner take one
  `Subscribe`; `status()` alone puts nothing on the wire and stays
  `Waiting` (the honest sentence, pinned); `map` outside an owner scope is
  a compile error (`assert_fails_with`, on the `owner_scope` message).

### S3 — consumers and docs

The five app sites of §4.1–4.2, the five doc sites of §4.3, and the two
pre-existing doc errors (`docs/std/rpc.md:17`, `docs/guide/reactive.md:141`).

- Gate: `cargo test -p vilan-core --test docs` (every fence compiles) and
  `cargo test -p vilan-cli --test corpus` (byte-identical). The playground
  and kolt are outside this repo and land in their own commits.

### The pins standing now

Three `#[ignore]`d pins at the end of `crates/vilan-core/tests/inference.rs`,
each measured against this worktree's compiler so that none is vacuous:

| Pin | Asserts | Today, measured | Ruling-dependent? |
|---|---|---|---|
| **A** `a25_disposing_the_last_remote_subscription_sends_unsubscribe` | the frame log ends `up {"Unsubscribe":0}` and the post-dispose `set` puts nothing on the wire | ends `down {"Update":[0,2]}` — §1.1 | no |
| **B** `a25_a_second_watcher_opens_no_second_server_forward` | six observer lines, one per watcher per change | ten lines — §1.2 | no |
| **C** `a25_map_carries_a_fallback_and_the_count_rides_the_owner` | `status` before any frame, one `Subscribe` at `map`, one `Unsubscribe` at `scope.dispose()` | does not compile: no `map`/`status` on `RemoteSource` | **yes** — rewritten, not un-ignored, if §6 rules otherwise |

## 6. Owner questions — all RULED 2026-08-19, each as recommended

**Q1 — `sub`'s observer shape (§2h).** Recommend keeping `sub(|T|)`
present-only and confronting the `Option` only in `map`. The counted
primitive underneath carries `|Option<T>|` either way; the question is
whether it is *also* exposed publicly (an `observe(|Option<T>|)`) for the
caller who wants arrivals without a derived signal. Recommendation: not in
v1 — `map` covers it, and a second public counted entry point is a second
thing to teach. Widening `sub` itself is the option this paper argues
against: seven call sites, two doc fences and the README, for no gain at
any of them.

**Q2 — is `Stale` in v1 (§2d)?** Recommend no: `Waiting`/`Ready` derive
from the cache alone and cost nothing, while `Stale` costs an optional
`Signal<ConnectionState>` threaded through `ReactiveClient` into every
mirror, for something an app composes today with `combine((status,
connection_state()))`. A yes is entirely reasonable if "one place to look"
beats "one less wire" — it is a plumbing bill, not a semantics risk.

**Q3 — naming beyond `status`, which is ruled.** The `Status` arms
themselves: `Waiting`/`Ready` (recommended — `Waiting` says *why* there is
no value, where SolidJS's `unresolved`/`pending` split a distinction vilan
does not have), versus `Pending`/`Ready`, versus SolidJS's own vocabulary.
And the seam's name: `or(initial)` (recommended, reads as a fallback at the
call site: `client.notes.or([])`) versus `with_default` / `unwrap_or`
— the last matching `Option::unwrap_or` but reading as a one-shot on a
thing that is not one-shot.

**Q4 — is the deferred `Unsubscribe` in v1, or does v1 send both frames
eagerly (§2a)?** Recommend deferred: it is one small `at_settle` helper,
and without it every re-render of a view that reads a mirror churns
`Unsubscribe`+`Subscribe` on the wire. The eager form is simpler and
strictly correct — it just costs frames on exactly the pattern the
framework encourages. Named as a question because it is the one place this
design adds a primitive to `reactive.vl` rather than only to `rpc.vl`.

## 7. What this paper does not do

- It does not touch the server's exposure model, the codec, or the frame
  vocabulary. §3 is a dead arm coming alive and one idempotence fix.
- It does not make the binders generic over `Source` (§2i). That widening
  is a separate item, and if it ever happens `RemoteSource` can conform as
  `Source<Option<T>>` then.
- It does not fix `Signal::map`'s writable-derived wart (§2c), or the
  unowned subscriptions in `map`/`combine`/`flatten`. A25 inherits the
  existing rule rather than adding a second one.
- It does not address `.ready(): Task<Source<T>>` (the backlog's optional
  await-first-value shape). With `map` supplying the fallback and `status`
  supplying the knowledge, no site in the census wants it; it is recorded
  here as available, not proposed.

## 8. Ship record (2026-08-19)

Built as ratified, in §5's three slices, on lane `a25-remote-sources`
(branched from `next`; three commits, one per slice, plus the records).
Every behavior below was probed through the lane's fresh `target/debug/vilan`
before its pin was written, and every new pin was planted red at least once
(the rebind pin: drop the `closing` clear and it prints `{"Unsubscribe":99}`
before `settled`).

### S1 — the count and the `Unsubscribe`

`std/src/reactive.vl`: `Subscription.release: Shared<Option<|| void>>`
(a one-shot teardown hook; `Signal::sub` leaves it `None`; `dispose` takes
it out before running it, so a by-hand dispose followed by the owner's
cannot double-decrement) and `at_settle(id, action)` — the action rides
the turn's pending queue as an ordinary `Subscriber`, deduped by `id`, and
resolves its turn exactly as `Signal::notify` does (ambient, else the
draining one, else inline). Nothing else in the scheduler moved.

`std/src/rpc.vl`: `RemoteSource` replaces `wanted` with `count` and
`closing` (§2a's shape) plus two fields §2a's sketch did not draw —
`settle_id: i32` (a `fresh_id()` the deferred flush dedups under; channel
ids share no counter with subscriber ids, so a channel id cannot serve)
and `codec: Codec` (the `Unsubscribe` is encoded at flush time against the
channel the mirror holds *then*, which is what makes a flush after a
rebind name the right channel; pre-encoding it beside `subscribe` would
have widened `rebind`'s signature and the generated rebinder list for no
gain). `sub` is the present-only face of one private counted `lease`;
`acquire`/`release`/`flush_close` are the four-line rule verbatim;
`rebind` clears `closing` and re-subscribes iff `count > 0`;
`ReactiveServer::start` is idempotent as a **no-op** for a channel whose
forward is live (the proposal offered no-op or re-seed; a counted client
never sends the duplicate, and a no-op is the smaller invariant).

Measured frames (the relay harness of §5, pin A's program): `up
{"Subscribe":0}` · `down {"Update":[0,0]}` · `down {"Update":[0,1]}` · `up
{"Unsubscribe":0}` — and the post-dispose `set` puts nothing on the wire.
Pin B prints its six lines. `sub`+`dispose`+`sub` in one `batch`: one
`Subscribe`, no `Unsubscribe`, and the channel is still live after the
settle. Two `map`s under one owner: one `Subscribe`, one `Unsubscribe`.

Pins (all in `crates/vilan-core/tests/inference.rs`, the A25 block): **A**
and **B** un-ignored; new — `a25_a_second_subscribe_frame_opens_no_second_forward`
(server idempotence under a raw double `Subscribe`),
`a25_a_same_turn_resubscribe_cancels_the_pending_unsubscribe`,
`a25_a_pending_unsubscribe_does_not_cross_a_rebind`,
`a25_a_counted_subscription_releases_its_lease_once`. Gates:
`cargo test -p vilan-core --test inference` (2334 passed), `--test
rpc_http` (6), `--test transport_robustness` (3, the reconnect legs
included) — all exit 0.

**One observation about "the ambient turn", recorded rather than
designed around.** `at_settle` reads `turn_scope.get_safe()` from
wherever it is called, and the call sits inside the lease's release hook
— a *stored* closure. By spec §8.4 a closure captures its context at
creation, so the turn the flush defers into is the one ambient when the
lease was **taken**, not when it is disposed: a lease taken under a
(since-settled) mount or event turn and disposed later enqueues onto that
settled turn and drains on its late-enqueue microtask; a lease taken with
no turn ambient and disposed inside a turn's *body* flushes inline (a
dispose inside a *drain* still joins the draining turn, through the same
device `Signal::set` uses). This is the rule every stored `set` callback
already follows, and for the case the deferral exists for — a view that
disposes and rebuilds in the same synchronous segment — the re-subscribe
lands before either the settle or the microtask, so zero frames cross
either way. The same-turn pins take their lease inside the `batch` so they
exercise the settle path as written.

### S2 — `map`, `or`, `status`

As §2b–§2e, with one spelling difference: `map` hands its lease to the
owner with `get_owner().take(lease)` rather than `defer(|| lease.dispose())`
— same effect, and `Subscription` is already `Disposable`. `Status` sits
beside `RemoteSource` in `rpc.vl` with `[derive(PartialEq, Debug)]` like
`ConnectionState`. The honest sentence is in `status`'s doc comment and
`Status::Waiting`'s.

Pin **C** un-ignored as-is (its exact eight lines). New —
`a25_map_outside_an_owner_scope_is_a_compile_error` and
`a25_or_outside_an_owner_scope_is_a_compile_error` (`assert_fails_with` on
the `owner_scope` coverage message),
`a25_or_reads_the_initial_before_the_first_frame_and_the_value_after` (a
relay that HOLDS upstream frames, so "before the first frame" is real
over an in-process wire), `a25_two_maps_under_one_owner_take_one_subscribe`,
`a25_status_alone_opens_nothing_and_stays_waiting`. Gates: inference (2340
passed, 2 ignored — the two pre-existing pins, not A25's) and `cargo test
-p vilan-core --test docs` (8 passed), both exit 0.

A find while probing, not A25's to fix: a coverage failure's diagnostic
cascades. With `std::rpc` loaded (or even without — `Signal::effect` at the
top of `main` shows it too), the one true error ("context `owner_scope` is
read here…") is accompanied by two spurious ones about an async closure
reaching the host function `run` (`rpc.vl`'s `LocalTransport` and
`task.vl`'s nursery). Pre-existing; the pins assert the true message.

### S3 — consumers and docs

`examples/walkthrough/src/{client,views}.vl` and
`examples/todo/src/{client,todos}.vl`: the §4.1 diff — the hand mirror and
the parameter go, the view reads `client.<field>.or([])`. `benchmarks/src/
coalescing.vl` already disposed; `realtime.vl` now carries its three
leases in `Session.watching` and disposes them when the measurement is
done. Docs: `guide/services.md` ("Reading a mirror", a compiled browser
fence with `mount_root` + `or` + `bind_each`, the count, the honest
sentence, the Traps bullet about the local derivative),
`guide/walkthrough.md` (the client fence matches the example; the `or`
fragment), `std/rpc.md` (the generated field is a `RemoteSource<T>`; a
"Mirrors" section with the five signatures and `Status`), `guide/reactive.md`
140–143 and `std/reactive.md:87` (`sub` fires once immediately — both said
the opposite; the second was not in the brief and is the same error),
`std/reactive.md` lists `at_settle`, `README.md:64` shows `or`. Gates:
docs (8 passed, exit 0); `cargo test -p vilan-cli --test benchmarks`
(exit 0, the deterministic counts unchanged); both examples build from
their tracked files.

**The `or([])` annotation.** Every census site is a list mirror, and
`let notes = client.notes.or([])` unannotated yields a `Signal` whose
element type is lost — "cannot access field 'done' on type any" at the
first use. It is not A25's: `Option<List<Todo>>::unwrap_or([])` loses it
the same way ("cannot index this List: its element type is never
determined"), so does a `map` with a `None => []` arm, and all three
reproduce on the v0.30.0 binary — an empty `[]` through a `T`-typed
parameter does not take `T` from the receiver's already-bound type
argument. The examples and docs write the annotated form
(`let notes: Signal<List<Note>> = client.notes.or([]);`), the docs say
why in one sentence, and the gap is pinned `#[ignore]` as
`a25_or_of_an_empty_list_infers_the_element_type_without_an_annotation`
(`AGENTS.md` gate 4) for the analyzer item that closes it.

### The corpus — STOPPED, not regenerated

`cargo test -p vilan-cli --test corpus` exits 101: **six goldens diverge**
— `reactive.vl`, `reactive-flatten.vl`, `reactive-owner.vl`,
`reactive-turns.vl`, `signal-update.vl`, `spread-parameters.vl`. The
goldens inline the std code a program reaches, and §2a's `Subscription.release`
reaches all of them: `Signal::sub`'s emitted constructor gains the third
field (`[ self[1], id, __shared_new([ 1 ]) ]`), `Subscription::dispose`'s
tail gains the release-hook match, and the minified helper names shift
behind those two. No other change of shape. Each rebuilt program's stdout
under node is byte-identical to its tracked golden's (all six compared).
This is an inherent consequence of the ratified §2a design — the
alternative the paper rejected, a bespoke `RemoteSubscription`, is the only
shape that leaves `Signal::sub` untouched — and the brief says stop, so the
goldens are untouched on the lane; regenerating them (after `cargo build`)
is the merge's call. The full suite on the lane is therefore red by exactly
this test; every other binary is green.

### Outside this repo — the diffs to apply

`vilan-playground/todo/src/client.vl`:

```diff
 async fun main() {
-	let notes: Signal<List<Note>> = Signal::new([]);
 	let token = Signal::new(storage::get("notes-token"));
 	let route = current_path().map(parse);

 	match NotesClient::connect("/", json_codec()) {
 		Ok(let client) => {
-			let _sync = client.notes.sub(|x| notes.set(x));
-			let _root = mount_root("app", || app(client, notes, token, route));
+			let _root = mount_root("app", || app(client, token, route));
 		}
 		Err(let error) => print(i"connect failed: {error.debug()}")
 	}
 }

 fun app(
 	client: NotesClient<SocketTransport>,
-	notes: Signal<List<Note>>,
 	token: Signal<str>,
 	route: Signal<Route>,
 ) {
 	let note_name = Signal::new("");
+	// Counted, and released when this view is unmounted. `[]` is the
+	// fallback that used to be `Signal::new([])` two frames up.
+	let notes: Signal<List<Note>> = client.notes.or([]);
```

`kolt/src/client.vl` (and `screen` in `kolt/src/views.vl`):

```diff
 async fun main() {
-	let items: Signal<List<Workspace>> = Signal::new([]);
-	let tasks: Signal<List<Task>> = Signal::new([]);
 	let token = Signal::new(storage::get("kolt-token"));
 	let route = current_path().map(parse);

 	match KoltClient::connect("/", json_codec()) {
 		Ok(let client) => {
-			let syncing = client.workspaces.sub(|list| items.set(list));
-			let syncing_tasks = client.tasks.sub(|list| tasks.set(list));
-			let root = mount_root("app", || screen(client, items, tasks, token, route));
+			let root = mount_root("app", || screen(client, token, route));
 		},
 		Err(let error) => print(i"connect failed: {error.debug()}"),
 	}
 }
```

```diff
 fun screen(
 	client: KoltClient<SocketTransport>,
-	items: Signal<List<Workspace>>,
-	tasks: Signal<List<Task>>,
 	token: Signal<str>,
 	route: Signal<Route>,
 ): View {
+	// The two mirrors as plain signals: `[]` until the first sync, counted
+	// and released when the screen unmounts.
+	let items: Signal<List<Workspace>> = client.workspaces.or([]);
+	let tasks: Signal<List<Task>> = client.tasks.or([]);
 	view("div")
```

(`client.vl`'s `Task`/`Workspace` imports from `pkg::shared` become
unused there and can go; `views.vl` already imports both.)

`kolt/src/probe.vl` — a `main`-level probe with no view and no owner, so
`sub` stays and gains the disposals it lacked, at the end of the observer
arm (after the acting connection's block, before the arm closes):

```diff
 							print(i"observer task count restored={observed_tasks.get() == tasks_before}");
 						},
 						Err(let error) => print(i"register rpc failed: {error.debug()}"),
 					}
 				},
 				Err(let error) => print(i"acting connect failed: {error.debug()}"),
 			}
+			// Done observing: release both leases, which closes both channels.
+			watching.dispose();
+			watching_tasks.dispose();
 		},
 		Err(let error) => print(i"observer connect failed: {error.debug()}"),
 	}
```
