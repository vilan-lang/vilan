# std::reactive reference

Signals, effects, ownership, turns, and the higher-level cells. Concepts and
usage patterns: the [reactive guide](../guide/reactive.md).

Import what you use:

```vilan,fragment
import std::reactive::{
	Signal, SignalCell, Source, MaybeSignal, Subscription, Disposable, combine,
	Owner, owner_scope, get_owner, run_with_owner, comp,
	Turn, FlushPolicy, turn_scope, turn, batch, flush, at_settle,
	optimistic, Optimistic, WriteState,
	draft, Draft, DraftState,
	reconcile, ReconcilePlan, RowStep,
};
```

## At a glance

| Item | Kind | One line |
|---|---|---|
| `Source<T>` | trait | anything readable + subscribable (`get`/`sub`/`effect`) |
| `Signal<T>` | trait | the writable half (`set`/`notify`/`set_with`); `Source` is its supertrait |
| `SignalCell<T>` | struct | the canonical cell — mutable value plus subscribers |
| `MaybeSignal<T>` | trait | a component value that may be static OR reactive |
| `Subscription` | struct | an explicit subscription; `Disposable` |
| `combine` | fn | tuple-signal over 2+ signals |
| `Owner` | struct | disposal bag; the lifetime unit |
| `run_with_owner`, `comp`, `get_owner`, `owner_scope` | fns/context | establish/read the ambient owner |
| `turn`, `batch`, `flush`, `at_settle`, `FlushPolicy`, `turn_scope` | fns/context | write batching |
| `optimistic` | fn | paint → commit → confirm-or-rollback (one shot) |
| `Optimistic<T>`, `WriteState` | struct/enum | the same lifecycle, observable and overlap-safe |
| `draft`, `Draft<T>`, `DraftState` | fn/struct/enum | local-first editing cell |
| `reconcile`, `ReconcilePlan`, `RowStep` | fn/structs | keyed list diffing engine |

## Signal and SignalCell

`Source` reads and `Signal` writes; `SignalCell` is the canonical cell that
implements both. A component that only observes takes a `Source`, one that
writes back takes a `Signal`, and one that needs the cell's own surface — a
`map`, an in-place `update` — names `SignalCell`.

```vilan,fragment
trait Signal<T> with Source<T> {
	fun new(value: T): SignalCell<T>             // default body: the canonical cell
	fun set(self, value: T)                      // required
	fun notify(self)                             // required
	fun set_with(self, transform: sync |T| T)    // default: set(transform(get()))
}
```

`Signal::new(v)` is the everyday spelling and does not dispatch: there is no
receiver to select an implementation from, so it resolves statically to the
trait's own default body and means the canonical cell in every file. The value
it hands back is a `SignalCell<T>`.

```vilan,fragment
impl SignalCell<type T> with Signal<T> {
	fun new(value: T): SignalCell<T>
	fun set(self, value: T)                 // write + notify
	fun notify(self)                        // publish without changing
	// from the trait default:
	fun set_with(self, transform: sync |T| T)    // read-modify-write
}
impl SignalCell<type T> {
	fun update(self, mutate: sync |&mut T| void) // mutate in place, notify once
	fun map<U>(self, transform: sync |T| U): SignalCell<U>
}
impl SignalCell<type T> with Source<T> {
	fun get(self): T
	fun sub(self, observer: |T| void): Subscription
	// from the trait default:
	fun effect(self, observer: |T| void)    // fires now + on change; owner-registered
}
impl SignalCell<SignalCell<type U>> {
	fun flatten(self): SignalCell<U>            // follow the current inner signal
}
```

`update` is **inherent to the cell**, deliberately: its value is in-place
mutation with one notification, and a generic default could only
read-copy-mutate-write-back — the copy it exists to avoid. An implementation may
want its own update logic or none at all, so a consumer that needs it asks for
`SignalCell<T>` rather than for a bound.

A trait may be written as a **`let` annotation**, where it is a checked
constraint rather than the binding's type: `let count: Signal<i32> =
SignalCell::new(1)` asserts that `SignalCell<i32>` implements `Signal<i32>` and
leaves `count` a `SignalCell<i32>`, `update` and all. Checked wide, kept narrow.

- `set` notifies through the ambient turn when one exists (writes coalesce);
  outside any turn it notifies immediately.
- `update` mutates the **stored** value through a writable view and notifies
  once, unconditionally, after the closure returns — the collection door
  ([design notes](https://github.com/vilan-lang/proposals/blob/main/proposal/signal-update.md)). It shares `set`'s notify half, so batching,
  drain affinity, and dedup behave identically. A read from *inside* the
  closure sees the in-progress value; a re-entrant `update` of the same
  signal is unsupported.

```vilan
import std::reactive::{ Signal, SignalCell, Owner, batch };

fun main() {
	let owner = Owner::new();
	let todos: SignalCell<List<str>> = Signal::new([]);
	owner.take(todos.sub(|list| print(list.len())));   // 0

	todos.update(|&mut list| { list.push("write docs"); });   // 1

	// Two updates, one notification: `update` batches like any write.
	batch(|| {
		todos.update(|&mut list| { list.push("ship it"); });
		todos.update(|&mut list| { list.push("rest"); });
	});                                                        // 3
}
```
- `map`'s result is a live derived signal, and its internal subscription is
  **detachable**: made inside a boundary — a mounted view, a `bind_each` row —
  it is registered with the ambient owner and dies when that boundary is
  disposed. `combine` and `flatten` register theirs the same way (`flatten`
  also releases whichever inner subscription is live at disposal).
- Made **outside** every boundary — module level, the top of `main` — a
  derivation has no owner to register with and lives as long as its source.
  That is what a module-level `current_path().map(parse)` wants, and it is why
  the derivations read the ambient owner optionally where `effect` demands one.
- `effect` requires an ambient owner; calling it outside every owner is a
  compile error (context coverage). It fires once immediately.
- `sub` fires once immediately with the current value, like `effect`, and
  then on every change; its `Subscription` is yours to dispose (or hand to
  `owner.take`).

## Source

```vilan,fragment
trait Source<T> {
	fun get(self): T
	[must_use]
	fun sub(self, observer: |T| void): Subscription
	fun effect(self, observer: |T| void)    // trait default; owner-registered
}
```

The read-only half of a reactive value. `SignalCell<T>` implements it, and so does
any type of yours — a storage-backed cell, a mirror over a transport, a wrapper
that logs. Implement `get` and `sub` and `effect` comes free.

```vilan
import std::reactive::{ Owner, Signal, SignalCell, Source, Subscription };

/// A signal with a place to hang persistence, and no `set` on the trait.
struct Stored<T> {
	inner: SignalCell<T>,
}

impl Stored<type T> with Source<T> {
	fun get(self): T {
		self.inner.get()
	}

	[must_use]
	fun sub(self, observer: |T| void): Subscription {
		self.inner.sub(observer)
	}
}

impl Stored<type T> {
	fun new(value: T): Stored<T> {
		Stored { inner = Signal::new(value) }
	}

	fun set(self, value: T) {
		self.inner.set(value);
	}
}

fun main() {
	let owner = Owner::new();
	let width: Stored<i32> = Stored::new(400);
	owner.take(width.sub(|value| print(value)));   // 400
	width.set(320);                                // 320
}
```

**Anything that only reads takes a `Source`, not a `Signal`.** Every read-only
binding in [`std::ui`](browser.md#view-methods) — `bind_text`, `bind_class`,
`bind_attr`, `bind_styled`, `style_var`, `bind_each`, `when`, `show`, `swap`
and `swap_split` — is generic over `Source<T>`, so `Stored<str>` above drives
them exactly like a signal does, on the browser layer and on the SSR twin
alike. `ReactiveServer`'s `expose` is generic the same way, and so are the
`Slot` and `AttrValue` arms element syntax dispatches through, so
`<p>{stored}</p>` and `<a href(stored)>` work for any source. What asks for a
`Signal` is what **writes**: `bind_value` and its SSR twin bound on
`Signal<str>`, and `optimistic` on `Signal<T>`, so a custom implementation with
its own `set` drives them. `Optimistic::over` still asks for the cell, because
`Optimistic` STORES it in a field and a field must name a real type.

## Writing a Signal

Implement `Source`'s `get`/`sub` and `Signal`'s `set`/`notify`, and the type is
usable anywhere a signal is wanted. The setter is where custom behaviour lives —
a clamp, a persistence write, a debounce — and there is only one value, so
whatever `set` stores is what every reader and every observer sees.

```vilan
import std::display::Display;
import std::reactive::{ Signal, SignalCell, Source, Subscription };

struct Clamped { inner: SignalCell<i32>, max: i32 }

impl Clamped {
	fun new(initial: i32, max: i32): Clamped {
		Clamped { inner = SignalCell::new(initial), max }
	}
}

impl Clamped with Source<i32> {
	fun get(self): i32 { self.inner.get() }
	[must_use]
	fun sub(self, observer: |i32| void): Subscription { self.inner.sub(observer) }
}

impl Clamped with Signal<i32> {
	fun set(self, value: i32) {
		self.inner.set(if value > self.max { self.max } else { value });
	}
	fun notify(self) { self.inner.notify(); }
}

/// A component. It bounds on the writable trait and knows no implementation.
fun width_control<S: Signal<i32>>(width: S) {
	width.set(1000);
	let seen: i32 = width.get();
	print(i"stored: {seen}");
}

fun main() {
	width_control(SignalCell::new(0));       // stored: 1000
	width_control(Clamped::new(0, 800));     // stored: 800
}
```

The trait promises **nothing** about notification frequency. `SignalCell`
notifies unconditionally — `set` never compares — and an implementation that
wants "don't publish an unchanged value" writes that in its own `set`.

## MaybeSignal

```vilan,fragment
trait MaybeSignal<T> {
	fun bind(self, react: |T| void);
}
```

One parameter that takes a static value or a reactive one, with no ceremony at
the call site. `bind` is effect-shaped rather than getter-shaped, which is what
lets one signature serve both: the static implementation fires the handler once,
the reactive one subscribes and keeps firing.

```vilan
import std::reactive::{ MaybeSignal, Owner, Signal, SignalCell, comp };

fun badge<V: MaybeSignal<str>>(label: V) {
	label.bind(|text| print(i"[{text}]"));
}

fun main() {
	let (_value, _owner) = comp(|| {
		badge("draft");                      // [draft]
		let live = Signal::new("saved");
		badge(live);                         // [saved]
		live.set("synced");                  // [synced]
		0
	});
}
```

std ships two implementations: a blanket `impl type T with MaybeSignal<T>` (the
static case) and `impl type S: Source<type T> with MaybeSignal<T>` (the reactive
one). Which runs is settled at the call by the specificity order, with no
runtime discrimination anywhere. The reactive arm registers with the ambient
owner, so a component's subscription dies with the boundary that built it — and
that is why `bind` may only be called under one.

## combine

```vilan,fragment
fun combine<T: (2..)>(sources: (U in T: SignalCell<U>)): SignalCell<T>
```

A signal of the tuple of the sources' current values, firing when any source
changes. Variadic over tuples of signals of mixed element types:

```vilan
import std::reactive::{ Signal, SignalCell, combine };

fun main() {
	let flag = Signal::new(true);
	let count = Signal::new(2);
	let both: SignalCell<(bool, i32)> = combine((flag, count));
	let (_on, current) = both.get();
	print(current);
}
```

(Destructuring names the parts, which reads better than positions;
`both.get().1` also works.)

## Subscription, Disposable

```vilan,fragment
trait Disposable { fun dispose(self); }
struct Subscription { … }        // impl Disposable
impl Subscription {
	fun teardown(release: || void): Subscription   // a subscription over no signal
}
```

Disposing a subscription guarantees no *later* deliveries; a delivery already
queued in the currently-draining turn may still land once.

`Subscription::teardown` is the registration shape for a source **outside** the
signal graph: `dispose` runs the hook once and does nothing else. `std::dom`'s
`listen` is built on it — a DOM listener's whole teardown is the call that
unhooks it. The hook is one-shot, so disposing twice is safe.

## Owner

```vilan,fragment
impl Owner {
	fun new(): Owner
	fun take<T: Disposable>(self, item: T): T   // adopt a disposable; returns it
	fun defer(self, cleanup: || void)           // run cleanup at dispose
}
impl Owner with Disposable {
	fun dispose(self)   // dispose everything collected + run defers
}

let owner_scope: Context<Owner>
fun get_owner(): Owner                                        // read the ambient owner
fun run_with_owner<T>(owner: Owner, body: (sync || T) context owner_scope): T
fun comp<T>(body: (sync || T) context owner_scope): (T, Owner)     // fresh owner + result
```

`body` parameters marked `context owner_scope` receive the ambient owner
implicitly: your component functions thread ownership without mentioning it.
Establish owners at **disposal boundaries** (places where a subtree can die),
not per object; in UI code the framework's boundaries (`mount_root`,
`bind_each` rows, `when`/`swap` bodies) already do this.

## Turns

```vilan,fragment
enum FlushPolicy { AtEnd, AtSuspension }
let turn_scope: Context<Turn>

fun turn<T>(policy: FlushPolicy, body: (|| T) context turn_scope): T
fun batch<T>(body: (sync || T) context turn_scope): T   // join or create
fun flush()                                             // drain the ambient turn now
fun at_settle(id: i32, action: || void)                 // run `action` at the ambient settle; now if none
```

Inside a turn, signal writes are recorded and each subscriber runs once with
final values when the turn settles. The body is asyncness-polymorphic (spec
§7.4): a synchronous body settles at the end of its synchronous extent, and
an awaiting body holds every notification until it fully completes (a
transaction). Framework boundaries establish turns for you: UI event handlers
and `mount_root` (`AtSuspension`), RPC service handlers (`AtEnd`). Writes
landing after a settle (from spawned work) drain in per-segment microtasks.

`at_settle` defers a plain action the same way a notification is deferred:
it rides the ambient turn's queue, deduped by `id` (repeat deferrals of one
action in one turn run it once), joins the currently draining turn when
called from inside a settle, and runs inline when no turn is ambient. It is
the primitive under a remote mirror's deferred `Unsubscribe`
(`std::rpc`); library code that wants "after this turn, once" uses it with
an id that cannot collide with a subscriber's (`fresh_id()` mints one).

## optimistic

```vilan,fragment
fun optimistic<T, E>(signal: SignalCell<T>, value: T, commit: async || Result<T, E>): Result<T, E>
```

Paint `value` into `signal` now, await `commit`, then reconcile: the
confirmed value on `Ok`, the previous value **rolled back** on `Err`. Returns
the outcome for error UX. For continuous editing, use `draft` instead:
rollback is wrong mid-typing.

The one-shot spelling: one write, no state to bind, and the rollback target
is whatever the signal held at the call. If more than one write can be in
flight over the same signal, or anything needs to render "saving…", use the
cell below.

## Optimistic — the observable lifecycle

```vilan,fragment
[derive(PartialEq, Debug)]
enum WriteState {
	Confirmed,      // nothing in flight; the value is the last confirmed truth
	Pending,        // the newest write is on the wire
	Rejected(str),  // the newest write was refused; the cell rolled back
}

struct Optimistic<T> {
	value: SignalCell<T>,           // the signal you handed to `over`; bind it
	state: SignalCell<WriteState>,  // bind a spinner, a disabled button, a banner
	…                           // internals: the confirmed shadow, two generations
}

impl Optimistic<type T> {
	fun over(signal: SignalCell<T>): Optimistic<T>
	fun write(self, value: T, commit: async || Result<T, str>): Result<T, str>
}
```

The same lifecycle as `optimistic`, with the two things a free function has
nowhere to keep.

- **`state` is observable.** `Pending` while the commit is on the wire,
  `Rejected(reason)` when one is refused — so a failure has somewhere to land
  besides the return value. `write` still returns the outcome; the state is an
  addition, not a replacement. `Rejected` is sticky until the next write.
- **Overlapping writes are safe.** Only the **newest** write paints the cell;
  a superseded write's outcome is discarded (it still returns to its own
  caller). And a rollback lands on the last value the **server** confirmed,
  not on whatever the signal happened to hold — a distinction that only shows
  up once writes overlap, and one that gets a counter of its own so an
  out-of-order reply cannot walk it backwards.
- **`over` wraps an existing signal**, so adopting the cell changes no
  binding, and it seeds the confirmed value from it.
- **Every transition is one wave.** The value and the state are published
  together, so an observer of both never sees "new value, still confirmed".
- `write` awaits, like `optimistic`. Fire-and-forget is
  `let _sent = async cell.write(..)`.

The commit returns `Result<T, str>` — the confirmed value or a reason — so an
rpc-calling closure maps its error the same way a `Draft` commit does.

Unlike `Draft`, there is **no re-push on reconnect**: a re-push is
at-least-once, which is safe for a draft's "set this field to this value" and
unsafe for the one-shot *actions* this cell is for. The rollback is the
recovery; the user re-issues the action.

A cell over a **mirrored** signal is out of scope for now — the mirror writes
behind the cell's back, so its confirmed value goes stale. Wrap a local
signal ([design notes](https://github.com/vilan-lang/proposals/blob/main/proposal/optimistic-lifecycle.md) §8).

## Draft — local-first cells

```vilan,fragment
enum DraftState {
	Synced,       // local matches the last pushed/adopted value
	Dirty,        // local edits not yet confirmed (in-flight included)
	Failed(str),  // last push errored; local KEPT, not rolled back
}

struct Draft<T> {
	local: SignalCell<T>,           // bind inputs to this; read like any signal
	state: SignalCell<DraftState>,  // bind a status label to this
	…                           // internals: synced value, generation, debounce window
}

fun draft<T: PartialEq>(initial: T, commit: async |T| Option<str>): Draft<T>

impl Draft<type T: PartialEq> {
	fun push(self, value: T)              // set local + SPAWN the commit (returns immediately)
	fun adopt(self, remote: T)            // fold in a remote value
	fun debounce(self, millis: i32): Draft<T>  // coalesce pushes; returns self for chaining
	fun commit(self)                      // send now, cancelling any pending window
	fun repush(self)                      // re-send iff local != synced (the reconnect path)
}
```

- `commit` returns `None` on success, `Some(reason)` on failure. The
  parameter is `async`-typed so an RPC-calling closure flows in directly; a
  plain synchronous closure works too.
- `push` is per-keystroke-safe: local-first (never waits on the wire), and a
  generation counter ensures only the **newest** push settles `state`:
  a slow older commit landing late is discarded.
- `adopt` rules: value equal to the last synced value (an **echo** of your
  own push) → no-op; **clean** local (no unpushed edits) → adopt into
  `local`; **dirty** local → local wins, the remote value is remembered so
  the eventual push knowingly overwrites (last-write-wins).
- On failure, `state` carries the reason and `local` keeps the user's text;
  the next `push` retries naturally.

### debounce — one commit per burst

`debounce(millis)` coalesces pushes: the commit fires `millis` after the
**last** one, carrying the value as of that moment. `0` (the default) commits
on every push.

- **Local-first is unaffected.** `local` and the `Dirty` state are still set
  synchronously inside `push`; only the commit waits out the window.
- **Trailing edge.** Three keystrokes inside the window produce one commit.
- `commit()` — the explicit save (a blur, a Save button) — cancels a pending
  window and sends now. Exactly one commit results, not two.
- The window belongs to the cell, so every copy of a draft agrees about it.

### repush — recover the edits an outage swallowed

`repush()` re-sends the local value **iff `local != synced`** — an edit whose
commit never left, or one caught in flight by a drop (a failed commit keeps
the local value and does not advance `synced`). A clean draft sends nothing.
A pending debounce window is cancelled and the value goes immediately.

Wire it to a transport's reconnect hook and a dropped connection stops
losing work; it is also the "retry" behind a failure banner's button:

```vilan,fragment
client.transport.on_reconnect(|| title.repush());
```

- **Delivery is at-least-once.** A commit the server applied but could not
  acknowledge before the socket died is indistinguishable here from one that
  never arrived, so the server may see it twice. `Draft`'s own reconcile
  absorbs the duplicate (`adopt` no-ops on an echo; the generation counter
  discards the superseded commit's outcome), but **your commit closure must
  tolerate a repeat**: "set the remote to this value" does, "append this
  entry" does not.
- **A failed re-push is not retried on a timer.** It settles `Failed`, keeps
  `local`, and the next reconnect sends it again — so a value the server is
  permanently refusing cannot spin.

UI wiring: `View.bind_draft(draft)`; see the [browser reference](browser.md).
The reconnect hook is in the [rpc reference](rpc.md#connection-state).

## reconcile: keyed list diffing

```vilan,fragment
enum RowStep {
	Keep(i32),     // reuse old row at index (moved into the new order)
	Refresh(i32),  // same key, changed value: rebuild, dispose old index
	Fresh,         // a new row
}
struct ReconcilePlan {
	steps: List<RowStep>,  // one per NEW item, in the new order
	removed: List<i32>,    // old indices gone entirely
}
fun reconcile<T: PartialEq, K: PartialEq>(
	old_keys: List<K>, old_items: List<T>, items: List<T>, key_of: sync |T| K,
): ReconcilePlan
```

The pure engine under `ui.bind_each`; duplicate keys claim the first
surviving row once. Reach for it directly only when building a custom
list-rendering primitive.
