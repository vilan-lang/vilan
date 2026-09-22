# std::reactive reference

Signals, effects, ownership, turns, and the higher-level cells. Concepts and
usage patterns: the [reactive guide](../guide/reactive.md).

Import what you use:

```vilan,fragment
import std::reactive::{
	Signal, SignalCell, Source, MaybeSignal, Subscription, Disposable, combine,
	selector, Selector,
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
| `Source<T>` | trait | anything readable + subscribable (requires `get`/`on_change`; `sub`/`effect`/`effect_on_change`/`scoped_effect`/`scoped_effect_on_change`/`map` are defaults) |
| `Signal<T>` | trait | the writable half (`set`/`notify`/`set_with`); `Source` is its supertrait |
| `SignalCell<T>` | struct | the canonical cell — mutable value plus subscribers |
| `MaybeSignal<T>` | trait | a component value that may be static OR reactive |
| `Subscription` | struct | an explicit subscription; `Disposable` |
| `combine` | fn | tuple-signal over 2+ signals |
| `selector`, `Selector<T>` | fn/struct | per-key selection: one subscription, two writes per change |
| `Owner` | struct | disposal bag; the lifetime unit |
| `on_cleanup` | fn | run a cleanup when the ambient owner is released |
| `run_with_owner`, `comp`, `get_owner`, `owner_scope` | fns/context | establish/read the ambient owner |
| `turn`, `batch`, `flush`, `at_settle`, `FlushPolicy`, `turn_scope` | fns/context | write batching |
| `optimistic` | fn | paint → commit → confirm-or-rollback (one shot) |
| `Optimistic<T>`, `WriteState` | struct/enum | the same lifecycle, observable and overlap-safe |
| `draft`, `Draft<T>`, `DraftState` | fn/struct/enum | local-first editing cell |
| `reconcile`, `ReconcilePlan`, `RowStep` | fn/structs | keyed list diffing engine (`K: PartialEq + Hashable`) |
| `SeqOp`, `MapOp`, `SetOp`, `DeltaLog`, `DeltaCursor`, `DeltaSource` | enums/struct/trait | the change structure: a collection's change as a value, and its log |
| `ListCell<T>` | struct | a `List` cell whose writes ARE its deltas |
| `SequenceCell<T>`, `Sequence<T>`, `Tracked<T>` | traits/struct | twelve mutators over one `splice` primitive; the `&mut` twin and its recorder |
| `map_each` | fn | `map g` element-wise and incrementally — one call of `g` per arriving element |

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
}
impl SignalCell<type T> with Source<T> {
	fun get(self): T
	fun on_change(self, observer: |T| void): Subscription   // the requirement: `observe`
	fun sub(self, observer: |T| void): Subscription         // the default, one call shallower
	// from the trait defaults:
	fun effect(self, observer: |T| void)    // fires now + on change; owner-registered
	fun effect_on_change(self, observer: |T| void)  // on change only; owner-registered
	fun map<U>(self, transform: sync |T| U): SignalCell<U>
}
// A BLANKET over the read trait, not a member of the cell (A86): any source
// whose element is itself a source joins — a `map` result, a derived cell, a
// mirror, a plain `SignalCell`.
impl type S: Source<type I: Source<type U>> {
	fun flatten(self): SignalCell<U>            // follow the current inner signal
}
impl type S: Source<Option<type I: Source<type U>>> {
	fun flatten(self): SignalCell<Option<U>>    // `None` detaches; `Some` follows
}
// The DYNAMIC pair (A123), written over `on_change` rather than over the join:
// which source to follow is decided by the value this one currently holds.
impl type S: Source<type T> {
	fun switch<U, I: Source<U>>(self, select: sync |T| I): SignalCell<U>
}
impl type S: Source<Option<type T>> {
	fun and_then<U, I: Source<Option<U>>>(
		self, select: sync |T| I
	): SignalCell<Option<U>>
}
```

`flatten` is a **blanket over `Source`** rather than a member of
`SignalCell<SignalCell<U>>`: the read contract is the trait, so an outer that
is a `map` result or a mirror holds an inner signal exactly as a cell does and
joins the same way. A default method on `Source<T>` could not say it — a
default cannot add a bound on `T`, and this one needs `T` to be a source — so
the bound lives in the impl subject. The `Option` form is the second blanket:
an outer of `Option<inner source>` (a lazily-created signal) answers `None` with
`None` and detaches from whichever inner was live, and `Some(inner)` follows
that inner from its current value. It is `sequence` then join, not the join of
the composite `Source`-of-`Option` — which is why a chain over optional cells
ends in a trailing `.map(|x| x.flatten())` and why `and_then` exists.

`switch` and `and_then` are the **dynamic dependencies** (A123). `map` and
`combine` are static: the expression fixes what the result reads. `switch`
follows whichever source its selector answers for the current value and
re-follows on every change — Rx's `switchMap`, meaning
`self.map(select).flatten()`. `and_then` is the same over `Source<Option<T>>`,
the total encoding of a signal that may hold nothing: it is the Kleisli
composition of that encoding (`Option::and_then` one level up), the outer
`None` and the inner `None` collapse into one, and it replaces the
`map(|x| x.map(f)).flatten().map(|x| x.flatten())` chain by hand. Both are
written directly over `on_change` rather than as the two-node `map`-then-join:
one derived cell, and the selector called exactly once per value of the source
(the `map` form calls it a second time at construction and orphans the node it
answered). Both are derivations, and both give the ambient owner the outer
subscription and whichever inner is live — `flatten`'s story exactly.

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
- What the closure receives is a `&mut` **subject**, not a binding, and a
  pattern binder under it is a binding like any other: `Some(mut list)`
  takes rule 1's copy (spec §6.1), so growing `list` grows the copy and
  the cell keeps the value it had. **Write back through the subject** —
  `held = Some(list)` — or move the payload out and put one back with
  `Option::take`/`replace`. Only a `SignalCell` whose value is a *wrapped*
  collection meets this; `update` on the collection itself mutates the
  storage directly, as the example above does.

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

The write-back, both spellings, on a cell whose value is a wrapped list:

```vilan
import std::option::Option::{ self, None, Some };
import std::reactive::{ Signal, SignalCell };

fun main() {
	let held: SignalCell<Option<List<str>>> = Signal::new(Some([]));

	// Through the `&mut` subject: the binder's copy is put back.
	held.update(|&mut held| {
		match held {
			Some(mut list) => {
				list.push("one");
				held = Some(list);
			}
			None => {}
		}
	});

	// Or move the payload out and put one back.
	held.update(|&mut held| {
		mut list = held.take().unwrap_or([]);
		list.push("two");
		held.replace(list);
	});

	match held.get() {
		Some(let list) => print(i"{list.len()}"),   // 2
		None => print("none"),
	}
}
```
- `map`'s result is a live derived signal, and its internal subscription is
  **detachable**: made inside a boundary — a mounted view, an `each` row —
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
- **`on_change` is the same subscription without that first call** — and it is
  the primitive of the two: `sub` is `on_change` plus that call, and
  `effect` is `effect_on_change` plus it. The eager pair is right for
  a UI binding — the immediate call *is* the initial paint — and wrong for an
  effect that must not fire on the state the program starts in: a "you have
  unsaved changes" prompt, an analytics ping, a derivation that already seeded
  its own first value. `map` and `combine` attach this way.

```vilan
import std::reactive::{ Disposable, Signal, SignalCell };

fun main() {
	let title: SignalCell<str> = Signal::new("untitled");
	// Nothing prints here — the current value is not a change.
	let watch = title.on_change(|value| print(i"renamed to {value}"));
	title.set("plans");        // renamed to plans
	watch.dispose();
}
```

## Source

```vilan,fragment
trait Source<T> {
	fun get(self): T                                        // required
	[must_use]
	fun on_change(self, observer: |T| void): Subscription   // required; no first call
	[must_use]
	fun sub(self, observer: |T| void): Subscription         // default; + one immediate call
	fun effect_on_change(self, observer: |T| void)          // default; owner-registered
	fun effect(self, observer: |T| void)                    // default; owner-registered, eager
	fun scoped_effect(self, body: (sync |T| void) context owner_scope)
	fun scoped_effect_on_change(self, body: (sync |T| void) context owner_scope)
	fun map<U>(self, transform: sync |T| U): SignalCell<U>  // default; derived signal
}
```

The read-only half of a reactive value. `SignalCell<T>` implements it, and so does
any type of yours — a storage-backed cell, a mirror over a transport, a wrapper
that logs. Implement `get` and **`on_change`**; `sub`, `effect`,
`effect_on_change` and `map` all come free.

**`on_change` is the primitive; `sub` is derived from it.** The requirement is
the *lazy* attach — add an observer and do not call it — and the default `sub`
is that attach plus one call with the current value, in that order. So an
implementation writes the smaller thing and gets the eager contract every
`std::ui` binding reads (`sub` fires once now, then once per change) without
writing a line of it, and there is no path anywhere in the trait on which a
call is made and then discarded.

The arrangement used to be the other way round — `sub` required, `on_change`
defaulted — and it could not be honest: the only lazy body reachable from an
eager requirement is one that subscribes eagerly and *swallows* the first call,
which costs a wrapper cell and a branch on every later notification, and makes
a genuine first change indistinguishable from the seeding one. Deriving the
eager form from the lazy one is pure addition (backlog A49).

`SignalCell` implements `on_change` as `observe` — the direct attach — and
also overrides `sub`, which is the default body with one call less
indirection; every UI binding in a program lands there.

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
	fun on_change(self, observer: |T| void): Subscription {
		self.inner.on_change(observer)
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
`bind_attr`, `bind_styled`, `style_var`, `each`, `when`, `show`, `swap`
and `swap_split` — is generic over `Source<T>`, so `Stored<str>` above drives
them exactly like a signal does, on the browser layer and on the SSR twin
alike. `ReactiveServer`'s `expose` is generic the same way, and so are the
`Slot` and `AttrValue` arms element syntax dispatches through, so
`<p>{stored}</p>` and `<a href(stored)>` work for any source. What asks for a
`Signal` is what **writes**: `bind_value` and its SSR twin bound on
`Signal<str>`, and `optimistic` on `Signal<T>`, so a custom implementation with
its own `set` drives them. `Optimistic::over` takes any `Signal<T>` too — the
cell STORES it in a field, and a field must name a real type, so the cell names
it: `Optimistic<T, S>` carries the signal's type as a second parameter.

### scoped_effect — an owner per run

```vilan,fragment
fun scoped_effect(self, body: (sync |T| void) context owner_scope)
fun scoped_effect_on_change(self, body: (sync |T| void) context owner_scope)
fun on_cleanup(cleanup: || void)
```

`effect`, except that **every run gets its own `Owner`**. Whatever the body
registers — an `on_cleanup`, a nested `effect` or `map`, an `owner.take`, a
mirror's lease — is released before the next run, and when the enclosing
boundary goes.

A plain `effect` body that subscribes to something accumulates one subscription
per change for as long as the boundary lives. That is usually what you want for
a body that only reads and writes; it is never what you want for a body that
opens something:

```vilan
import std::reactive::{ Owner, Signal, SignalCell, Source, on_cleanup, run_with_owner };

fun main() {
	let selected = Signal::new(1);
	let detail = Signal::new("loading");
	let page = Owner::new();
	run_with_owner(page, || {
		// One subscription on `detail` at a time, not one per selection.
		selected.scoped_effect(|id: i32| {
			on_cleanup(|| print(i"closing {id}"));
			detail.effect(|text: str| print(i"{id}: {text}"));
		});
	});
	selected.set(2);   // closing 1, then the new run subscribes
	page.dispose();    // closing 2
}
```

`on_cleanup(cleanup)` is `get_owner().defer(cleanup)` under a name, and it is
**one name whose meaning the ambient owner decides**: inside a `scoped_effect`
the ambient owner is that run's, so the cleanup runs per run; inside any other
boundary — a mounted view, a `swap` instantiation, an `each` row — it is the
boundary's, so it runs once, at teardown. Like `effect`, it requires an
enclosing owner *statically*: a cleanup with nothing in scope to run it is a
compile error, not a silent no-op.

The order inside a run is: release the previous run, install the fresh owner,
call the body. So a body that throws has already had its owner installed, and
what it registered before throwing is released by the next run (or by the
boundary).

`swap`, `when` and `each` are **not** built on this — they keep their own
per-instantiation owners. Reach for `scoped_effect` when you want that lifetime
without a view.

## selector — per-key selection

"Is this row the selected one?", asked once per row and answered live. The
obvious spelling derives a boolean per row off the selection signal — and every
one of them recomputes on every change, so moving a highlight one row costs `n`
notifications. `selector` takes **one** subscription on the source and keeps a
cell per key, so a change writes exactly **two**: the key that left and the key
that arrived.

```vilan,fragment
fun selector<T: Hashable + PartialEq, S: Source<T>>(source: S): Selector<T>

impl Selector<type T: Hashable + PartialEq> {
	fun of(self, key: T): SignalCell<bool>
}
```

```vilan,browser
import std::reactive::{ Signal, SignalCell, selector };
import std::ui::{ View, each, mount_root, view };

fun main() {
	let rows: SignalCell<List<i32>> = Signal::new([1, 2, 3]);
	let current: SignalCell<i32> = Signal::new(1);
	let selected = selector(current);
	let _root = mount_root("app", || {
		view("ul").child(each(rows, |id| id, |id| {
			view("li").text(i"row {id}").bind_class(selected.of(id).map(|on| {
				if on { "row current" } else { "row" }
			}))
		}))
	});
}
```

`of(key)` creates the key's cell on first ask (seeded against the source's
current value) and hands back that same cell every time after, so it is safe to
call in a row's render body. The entry's **removal** is deferred to the ambient
owner — which inside an `each` row is the row's own — so the map stays the
size of the live list rather than of every list the session ever showed.

`Selector` is a handle with a method rather than the bare closure Solid's
`createSelector` returns, and that is forced rather than chosen: a closure
captures its context **at creation**, so a closure built inside `selector` would
defer every key's cleanup to whatever owner was ambient where `selector` was
*called* — the component, never the row. A method call threads the caller's
ambient owner the ordinary way.

The key type is bounded on `Hashable` (the canonical key `Map` and `Set` use)
and on `PartialEq` (to seed a fresh cell against the current value). A key
nobody has asked about has no cell and costs nothing: a change into it writes
only the outgoing one.

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
	fun on_change(self, observer: |i32| void): Subscription { self.inner.on_change(observer) }
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

Disposing a subscription guarantees no later deliveries — **none**, including
ones already in flight. A subscriber carries a liveness flag that `dispose`
lowers before it detaches anything, and both notification loops skip a lowered
one, so a delivery is stopped whether it sits in a wave the drain has already
taken out of the queue, in a turn the disposer's own extent cannot reach, or in
the snapshot an inline notify is walking. (Until A110 a delivery queued in the
currently-draining turn could still land once. It cannot now.)

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
	fun is_disposed(self): bool                 // has this owner already been disposed?
}
impl Owner with Disposable {
	fun dispose(self)   // dispose everything collected + run defers; idempotent
}

let owner_scope: Context<Owner>
fun get_owner(): Owner                                        // read the ambient owner
fun on_cleanup(cleanup: || void)                              // = get_owner().defer(..)
fun run_with_owner<T>(owner: Owner, body: (sync || T) context owner_scope): T
fun comp<T>(body: (sync || T) context owner_scope): (T, Owner)     // fresh owner + result
```

`body` parameters marked `context owner_scope` receive the ambient owner
implicitly: your component functions thread ownership without mentioning it.
Establish owners at **disposal boundaries** (places where a subtree can die),
not per object; in UI code the framework's boundaries (`mount_root`,
`each` rows, `when`/`swap` bodies) already do this.

An owner has a **disposed state**, and it is what makes ownership hold across
`await`. A registration is a promise to release, and an async continuation
registers whenever it happens to run — a route switched away before a handle's
reply, an `each` row rebuilt while its first fetch is in flight. `take` and
`defer` on an owner that is already disposed therefore run the cleanup **now**
rather than parking it: the extent it would have belonged to is over, so the
only way left to keep the promise is to keep it immediately. `dispose` itself is
idempotent, and `is_disposed` reports the flag for a caller that can do
something cheaper than register-and-immediately-release.

An `effect` registered this late still makes its one immediate call — that call
is the observer's contract, not a subscription — and then never fires again.

A disposal group **finishes**: if one cleanup throws, the rest still run and the
first failure is raised once the group is released. That is the opposite of the
drain's rule, on purpose — an owner holds a list of independent promises to
release, so abandoning the list at the first failure would leak everything after
it, permanently.

## Turns

```vilan,fragment
enum FlushPolicy { AtEnd, AtSuspension }
let turn_scope: Context<Turn>

fun turn<T>(policy: FlushPolicy, body: (|| T) context turn_scope): T
fun batch<T>(body: (sync || T) context turn_scope): T   // join or create
fun flush()                                             // drain the ambient turn now
fun at_settle(id: i32, action: || void)                 // run `action` at the ambient settle; now if none
fun at_release_settle(id: i32, action: || void)          // the same, from a subscription's release hook
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

"Ambient" is the context rule's: reached through a **stored closure** — an
owner's cleanup, a subscription's release hook — the turn `at_settle` sees is
the one that closure captured when it was created. That is right for a write
from a stored callback and wrong for a RELEASE, which belongs to whoever
disposed the subscription and not to whoever took it; `at_release_settle` is
the release hook's spelling, and it resolves against the turn ambient at the
`dispose`. Outside a release hook the two are the same function.

**If an observer throws**, the settle is abandoned at that observer — the rest
of the wave does not run — and the error keeps unwinding out of the write that
started the settle, with its own type, message and stack. What it cannot do is
leave the scheduler broken: the draining flags are restored on the way out, so
the next write settles normally, this turn included. (Before this, one throwing
observer left its turn draining forever, and every later write in the program
queued into it and was never flushed.)

## optimistic

```vilan,fragment
fun optimistic<T, E, S: Signal<T>>(signal: S, value: T, commit: async || Result<T, E>): Result<T, E>
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

struct Optimistic<T, S: Signal<T>> {
	value: S,                       // the signal you handed to `over`; bind it
	state: SignalCell<WriteState>,  // bind a spinner, a disabled button, a banner
	…                           // internals: the confirmed shadow, two generations
}

impl Optimistic<type T, type S: Signal<T>> {
	fun over(signal: S): Optimistic<T, S>
	fun write(self, value: T, commit: async || Result<T, str>): Result<T, str>
}
```

The second parameter is the **signal's own type**. A field must name a real
type, so widening the cell past `SignalCell` means naming what it holds — and
inference binds both from the call, so `Optimistic::over(title)` needs nothing
written. You only spell `S` where you write the cell's type out: a field or a
return, `Optimistic<str, SignalCell<str>>`.

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

## Delta sources: the change structure

A collection's change is a value (tracker A112, `proposal/incremental-collections.md`).
Every collection shape has an OP type; a cell whose writes RECORD their ops can
be read by "what changed" instead of "what is now", and an operator over it runs
once per changed element rather than once per element.

```vilan,fragment
enum SeqOp<T> {
	Splice(i32, List<T>, List<T>),  // at: these left, these arrived
	SetAt(i32, T, T),               // at: was this, is now this
	Reset(List<T>),                 // the collection BECAME this list
	Move(i32, i32, i32),            // from, count, to — the same elements, elsewhere
}

enum MapOp<K: Hashable, V> { Put(K, Option<V>, V), Delete(K, V), Reset(Map<K, V>) }
enum SetOp<T: Hashable> { Add(T), Remove(T), Reset(Set<T>) }
```

Every arm carries **what left** as well as what arrived. That is a requirement,
not a courtesy: the O(1) derivative of a fold over a group (`sum`, `count`,
`mean`) is "add what arrived, subtract what left", and a payload that only
counts what left would send the operator back to read the collection — the O(N)
rerun the whole design deletes.

`SetAt` is deliberately not `Splice(at, [old], [new])` even though the
collection cannot tell them apart: for a per-element owner the two are
different events, and the difference is whether that owner survives.

The log and its cursors are the machinery:

```vilan,fragment
let delta_log_limit: i32 = 1024;

struct DeltaCursor { … }   // one consumer's place in a log

struct DeltaLog<O> { … }

impl DeltaLog<type O> {
	fun new(): DeltaLog<O>
	fun with_limit(limit: i32): DeltaLog<O>
	fun record(self, op: O)                                  // trims first, then appends
	fun cursor(self): DeltaCursor                            // minted at the current sequence
	fun drop_cursor(self, cursor: DeltaCursor)
	fun since(self, cursor: DeltaCursor): Option<List<O>>    // `None` = lost history
	fun trim(self)
	fun held(self): i32
	fun at(self): i32
	fun oldest(self): i32
}

trait DeltaSource<C, O> with Source<C> {
	fun cursor(self): DeltaCursor
	fun drop_cursor(self, cursor: DeltaCursor)
	fun since(self, cursor: DeltaCursor): List<O>            // never fails: a lost cursor gets `Reset`
}
```

`since` on the LOG answers an `Option` and `since` on the SOURCE does not: the
log does not know what a `Reset` is for its op type and the cell does, which is
the only place the two layers need to know about each other.

Writes in one turn coalesce into ONE notification, and a consumer drains every
op at the settle — so the log is as long as one turn's writes. Two behaviours
are worth stating because they are easy to state wrongly:

- **A log with no consumers holds one op, not zero.** `record` trims and then
  pushes.
- **The log is trimmed at the next WRITE, not at the drain.** A cell written
  once and then read for ever keeps one op's worth of history.

Past the limit the history is dropped and `base` jumps, so a consumer that
stopped draining is answered with one `Reset` carrying the collection: bounded
memory, at the cost of one whole-collection payload to whoever could not keep
up.

`KeyedCell<K, T>` (`std::rpc`) is the shipped delta source: it records the
positional ops and translates them into the wire's `Delta<K, T>` in its own
`since`, which is the one place the two vocabularies meet. `SignalCell<List<T>>`
is NOT a delta source — it keeps no log — so anything derived from one is on the
`Reset` path by construction, which is exactly today's behaviour.

## ListCell — a list whose writes are its deltas

```vilan,fragment
struct ListCell<T> { … }            // a SignalCell<List<T>> plus a DeltaLog<SeqOp<T>>

impl ListCell<type T> {
	fun new(): ListCell<T>
	fun of(elements: List<T>): ListCell<T>
	fun with_limit(elements: List<T>, limit: i32): ListCell<T>
	fun set_at(self, at: i32, value: T)                 // an element changed IN PLACE
	fun move_range(self, from: i32, count: i32, to: i32)
	fun edit(self, body: sync |&mut Tracked<T>| void)   // many mutations, ONE notification
	fun logged(self): i32
}
impl ListCell<type T: PartialEq> { fun reconcile_to(self, items: List<T>) }
// and: Source<List<T>>, Signal<List<T>>, SequenceCell<T>, DeltaSource<List<T>, SeqOp<T>>
```

`ListCell<T>` is an ordinary `Source<List<T>>` — `each` takes it, `map` takes
it, an effect takes it — that also records what each write DID. Nothing that
ignores the ops pays for them.

Its mutators are trait defaults over ONE primitive, so there is one place a
write is recorded and no method can forget:

```vilan,fragment
trait SequenceCell<T> {
	fun size(self): i32;
	fun splice(self, at: i32, removed: i32, inserted: List<T>);
	// twelve defaults over those two:
	// is_empty, push, prepend, insert_at, insert_all, remove_at,
	// remove_range, pop, extend, clear, set_all, truncate
}
```

`Sequence<T>` is the same surface with `&mut self` receivers, and its
implementor is `Tracked<T>` — a plain list plus the ops that produced it. That
is what `edit` hands a body, and it is why an algorithm can be written against
the BOUND rather than against a cell:

```vilan
import std::reactive::{ ListCell, Sequence };

fun fill<S: Sequence<str>>(target: &mut S) {
	target.push("first");
	target.push("second");
}

fun main() {
	let rows: ListCell<str> = ListCell<str>::new();
	rows.edit(|&mut list| {
		fill(&mut list);
		list.remove_at(0);
	});   // ONE notification, three ops
	print(rows.get().len());
}
```

Three doors write a whole list, and they cost differently on purpose:

| | records | a derivation's cost |
|---|---|---|
| `splice` and its twelve defaults | one `SeqOp::Splice` | the elements that arrived |
| `set_all(values)` | one `SeqOp::Splice` over everything | every element (they all arrived) |
| `set(values)` (the `Signal` impl) | `SeqOp::Reset` | every element, rebuilt |
| `reconcile_to(values)` | one `Splice` over what changed | the elements that changed |

`reconcile_to` is the compat door: it diffs the common prefix and the common
suffix and records ONE `Splice` over what is between them, so an append, a
prepend, an insertion, a removal or an edited span each cost only the elements
they really touched, and an identical list records nothing and notifies nobody.
It does not find a REORDER — a rotated list shares no prefix and no suffix, so
that is one `Splice` over the whole run, which is honest (every element did
move) and is what `each`'s keyed pass is for. A source that knows it reordered
says `move_range`.

## map_each — the first derivative

```vilan,fragment
fun map_each<T, U, S: DeltaSource<List<T>, SeqOp<T>>>(
	source: S, g: sync |T| U,
): ListCell<U>
```

`map_each(source, g)` is `source.get().map(g)` kept up to date by running `g`
once per element that ARRIVES or CHANGES, and zero times for anything else —
the derivative of `map g`, where `Splice(at, left, arrived)` becomes
`Splice(at, left, arrived.map(g))`:

```vilan
import std::reactive::{ ListCell, SequenceCell, map_each };

fun main() {
	let raw: ListCell<str> = ListCell<str>::new();
	let parsed = map_each(raw, |text: str| {
		print("ran");
		text.parse_f64().unwrap_or(0f)
	});
	raw.push("10.5");      // "ran" — once
	raw.remove_at(0);      // nothing
	print(parsed.get().len());
}
```

Three pushes are three calls; a removal, a `clear`, a `pop` and a `move_range`
are none; a `Reset` is the honest N. It takes any `DeltaSource`, so it serves
`ListCell`, `std::rpc`'s `KeyedCell` and anything an app writes.

The result is itself a `ListCell<U>`, so `map_each` composes: a chain runs one
call of each `g` per arriving element, at every step.

Two rules to hold on to. The subscription is a DERIVATION and the ambient
owner's, like every other combinator's, and the cursor goes with it — a
disposed derivation stops pinning the source's history. And `g` must be pure IN
THE ELEMENT: its result is kept, so a `g` that reads another signal will not
re-run when that signal changes. That is `map`'s contract already; here nothing
re-runs it at all, which makes the contract sharper rather than different.

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
fun reconcile<T, K: PartialEq + Hashable>(
	old_keys: List<K>, old_items: List<T>, items: List<T>, key_of: sync |T| K,
	same: sync |T, T| bool,
): ReconcilePlan
```

The pure engine under `ui::each`; duplicate keys claim the first
surviving row once. Reach for it directly only when building a custom
list-rendering primitive.

**"Unchanged" is the caller's predicate, not `T: PartialEq`.** The key decides
identity and whether the row moves; `same` decides, for a surviving key, reuse
against dispose-and-rebuild. `each` passes `|a, b| a == b`;
`each_by` passes `|_a, _b| true`, which is why it never emits a `Refresh`
and asks nothing of `T`.

**A pass is one walk, not N of them (tracker M82).** The matcher used to scan
the old keys from 0 for every new item, so a list that did not reorder cost
N(N+1)/2 iterations whatever the change was — 500,500 at 1,000 rows, for one
append. It keeps a position index instead: per canonical key, the chain of old
positions holding it. Measured over `each`'s scan half, callgrind Ir per change
under `node --jitless`: **355.1 M → 29.1 M at 1,000 rows**, and 95.1 M → 14.8 M
at 500 with 1,369.9 M → 58.5 M at 2,000 — 1.97× and 2.01× per doubling where it
used to be 3.73× and 3.86×, which is linear where it was quadratic.

**A REORDER is linear too, and `K: Hashable` is what pays for it (tracker
A125).** The index was keyed on the canonical hash with nothing binding that
hash to `K`'s equality, so an earlier equal key could sit outside the chain the
index named and the stretch from the smallest unclaimed index up to the
candidate had to be scanned as well. That stretch is empty when nothing moved —
which is why the append case above went linear — and it is the WHOLE prefix
when a list is reversed: N(N+1)/2 key comparisons, 500,500 at 1,000 rows, on
every sort-in-place. The bound states the obligation `Hashable` already names,
`a == b` implies `a.hash() == b.hash()`, so every key equal to this item's is
somewhere in this item's chain and the scan is gone. A 1,000-row reversal costs
**1,000** key comparisons, down from 500,500 (`vilan/test/reconcile-reorder.vl`
counts them); callgrind Ir per reversal under `node --jitless`: **738.4 M → 14.6 M at 1,000
rows**, a factor of 50.7, and 1.98× then 2.00× per doubling across 500 / 1,000
/ 2,000 rows where the scan was 3.92× from 500 to 1,000 — linear where it was
quadratic.

A COLLISION is still fine: two keys that are not `==` may share a hash — a
hand-written impl hashing a subset of the fields its `eq` reads — and the walk
steps past them along the chain, which is what any hash container does. What
the bound forbids is the other direction, an `==` coarser than the hash, which
would hide a moved row; that is now a compile error rather than a quadratic
scan. The plan is gated by a differential rather than by a golden: the
pre-index scan is reproduced verbatim in `vilan/test/reconcile-index.vl` and
1,415 cases — named shapes, 600 randomized, 400 with a coarse `==` and coarse
hash, 400 with a coarse hash alone — are compared plan for plan.
