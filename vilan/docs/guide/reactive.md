# Reactive state

`std::reactive` is Vilan's state layer. If you've used signals in Solid or
Preact, you'll be at home immediately. If you're coming from React, think
of a signal as a piece of state that components subscribe to directly:
there is no re-render, dependency array, or memoization dance. When a
signal changes, exactly the code that watches it runs.

Four ideas make up the layer, and this chapter takes them in order:

- **Signals** hold values.
- **Effects** run code when signals change.
- **Owners** decide when effects die.
- **Turns** decide when changes become visible.

The UI layer, the rpc mirrors, and the router are all built on these, so
this chapter pays for itself quickly.

```vilan
import std::reactive::{ Signal, SignalCell, Owner, run_with_owner };

fun main() {
	let count = Signal::new(0);
	let owner = Owner::new();
	run_with_owner(owner, || {
		count.effect(|value: i32| print(value));
	});
	count.set(1);
	count.set(2);
}
```

## Signals

A `SignalCell<T>` is a mutable cell whose readers can subscribe to changes.

```vilan,fragment
Signal::new(value: T): SignalCell<T>       // a fresh signal
signal.get(): T                        // current value
signal.set(value: T)                   // write + notify subscribers
signal.set_with(transform: sync |T| T) // read-modify-write in one step
signal.update(mutate: sync |&mut T| void) // mutate in place + notify once
```

Two names, one idea. **`Signal<T>` is a trait** — the writable half of the
reactive contract, `set` and `notify` over `Source`'s `get` and `sub` — and
**`SignalCell<T>` is the canonical type that implements it**, the cell
`Signal::new` hands back. Day to day you write `Signal::new(0)` and never
think about it. The split matters in two places: when a *component* wants to
accept any signal (bound it on `Signal<T>`, and a caller may pass a cell of
their own that clamps or persists — see
[Writing a Signal](../std/reactive.md#writing-a-signal)), and when you need to
*name the type* in a struct field or a return type, where a trait may not go
and `SignalCell<T>` is the word.

```vilan,fragment
struct Store { todos: SignalCell<List<str>> }   // a field names the cell
let count: Signal<i32> = SignalCell::new(1);    // an annotation may name the trait
```

An annotation naming a trait is a **checked constraint**, not the binding's
type: `count` is still a `SignalCell<i32>` and still has `update`, which lives
on the cell alone.

Signals hold **values**. Vilan copies, so `get` hands you a copy, and the
only way to change what subscribers see is a write through the signal
itself. For a collection, `update` is the one you want: the closure gets a
**writable view of the stored value**, so you mutate it directly.

```vilan
import std::reactive::{ Signal, SignalCell };

fun main() {
	let items: SignalCell<List<str>> = Signal::new([]);
	items.update(|&mut list| {
		list.push("first");
	});
	print(items.get().len());
}
```

The `&mut` in `|&mut list|` is the same view convention a function
parameter takes — it says *this closure mutates the caller's value*, which
is exactly what makes the push land in the signal rather than in a copy.
Subscribers are notified **once**, after the closure returns, whatever it
did (a closure that writes nothing still notifies — `update` is a write,
like `set`). Inside a `batch`, that notification defers and coalesces like
any other write. `update` works for any `T` a closure can mutate: `Map`,
`Set`, a struct's fields, a nested aggregate.

`set_with` remains the read-**transform**-write form, and it still reads
better when you're computing a new value rather than editing one:

```vilan
import std::reactive::{ Signal, SignalCell };

fun main() {
	let count = Signal::new(1);
	count.set_with(|n| n + 4);
	print(count.get());
}
```

(If you tried `items.get().push("first")`, you'd be mutating a copy. The
[memory model](../tour/memory-model.md) chapter explains why that's a
feature.)

## Derived state: `map`, `combine`, `flatten`

Build state as a graph and let it recompute itself:

- `signal.map(transform)` gives a signal of the transformed value:

  ```vilan
  import std::reactive::{ Signal, SignalCell };
  fun main() {
  	let count = Signal::new(2);
  	let doubled = count.map(|n: i32| n * 2);
  	print(doubled.get());
  	count.set(5);
  	print(doubled.get());
  }
  ```
- `combine((a, b, …))` gives a signal of the tuple of several
  signals' values. It fires when any of them changes. Takes two or more.
- `nested.flatten()` on a `SignalCell<SignalCell<U>>` follows whichever inner
  signal is current, and detaches from a replaced one.

```vilan
import std::reactive::{ Signal, SignalCell, combine };

fun main() {
	let first = Signal::new("Ada");
	let last = Signal::new("Lovelace");
	let full = combine((first, last)).map(|pair: (str, str)| {
		let (a, b) = pair;
		a + " " + b
	});
	print(full.get());
	first.set("Grace");
	print(full.get());
}
```

A named function can stand in for the closure (`signal.map(parse)`).
See [functions & closures](../tour/functions-and-closures.md).

### Selection over a list: `selector`

`map` is the wrong tool for one particular shape — "is *this* row the
selected one?", asked once per row. A derivation per row means every row
recomputes on every change: `n` notifications to move a highlight one
row. `selector(source)` keeps one subscription and a cell per key, so a
change writes exactly two of them — the key that left and the key that
arrived.

```vilan
import std::reactive::{ Signal, SignalCell, selector };

fun main() {
	let current: SignalCell<i32> = Signal::new(1);
	let selected = selector(current);
	let first = selected.of(1);
	let second = selected.of(2);
	print(i"{first.get()} {second.get()}");   // true false
	current.set(2);
	print(i"{first.get()} {second.get()}");   // false true
}
```

`selected.of(id)` hands back a `SignalCell<bool>` that drops into
`.show`, `.when`, `.bind_class` or `.bind_styled`. Call it inside a
`bind_each` row and the key's entry is released when the row is — the
map stays the size of the live list. Full reference:
[`std::reactive`](../std/reactive.md#selector--per-key-selection).

## Reacting: `effect` and `sub`

Two ways to run code on change. **Use `effect` by default.**

- `signal.effect(observer)` runs the observer now with the current
  value, re-runs it on every change, and cleans itself up automatically
  when its surrounding UI (or other owner) goes away. Nothing to
  remember.
- `signal.sub(observer): Subscription` is the manual version. It fires
  the same way — once now with the current value, then on every change —
  but you keep the `Subscription` and call `dispose()` on it yourself.
  (On a service mirror, `sub` is also **counted**: the first watcher
  opens the channel and disposing the last one closes it — see
  [Services: reading a mirror](services.md#reading-a-mirror).)
- `signal.on_change(observer)` and `signal.effect_on_change(observer)`
  are the same two, **without the immediate first call**. The eager pair
  is what a UI wants — that first call is the initial paint — so reach
  for these only when the current value is already accounted for: an
  effect that must not fire on the state the program starts in (a
  "you have unsaved changes" prompt, an analytics ping), or a derivation
  that seeded its own first value.

```vilan
import std::reactive::{ Disposable, Signal, SignalCell, comp };

fun main() {
	let title: SignalCell<str> = Signal::new("untitled");
	let (_built, scope) = comp(|| {
		// Silent now; one line per rename after this.
		title.effect_on_change(|value| print(i"renamed to {value}"));
	});
	title.set("plans");        // renamed to plans
	scope.dispose();
}
```

## Ownership: who cleans up

Every effect is a subscription, and subscriptions must die when the
thing that created them goes away. Otherwise a page you navigated off
keeps reacting forever. That's a memory leak in any reactive system.
Vilan's answer is **owners**, and in normal app
code you never manage them: the UI layer creates owners exactly where
subtrees can die (a mounted root, a list row, a conditional block), and
every `effect` you create automatically registers with the nearest one.

For tests, or when you're building your own machinery:

- `Owner::new()` makes an owner; `owner.dispose()` disposes everything
  registered with it.
- `run_with_owner(owner, || …)` runs a block with that owner ambient.
  Every `effect` inside, however deep in function calls, registers
  into it.
- `get_owner()` reads the ambient owner, e.g. to attach custom cleanup
  with `owner.defer(…)`.

```vilan
import std::reactive::{ Signal, SignalCell, Owner, run_with_owner };

fun main() {
	let source = Signal::new(0);
	let owner = Owner::new();
	run_with_owner(owner, || {
		source.effect(|value: i32| print(value));
	});
	source.set(1);
	owner.dispose();
	source.set(2); // not printed: the effect died with its owner
}
```

Creating reactive state *outside* any owner is a compile error. That
sounds strict, but it's the property that makes leaks impossible by
construction, and in practice `mount_root` already gave you an owner
before your first line of UI code ran.

> **Going deeper.** Ownership flows through the `context` mechanism
> ([functions & closures](../tour/functions-and-closures.md)): the
> `owner_scope` context carries the current owner, and closure
> parameters marked `context owner_scope` receive it invisibly. `comp`
> runs a block under a fresh owner and returns `(result, owner)`; it's
> the primitive under `mount_root`.

## Turns: when changes become visible

If an event handler sets five signals, you want watchers to see the
final state once, not five intermediate states. Vilan batches writes
into **turns**. Inside a turn, `set` only records. When the turn
settles, each affected watcher runs once with the final values:

```text
click ──▶ the handler runs inside a fresh turn
          │
          │  count.set(1)    ┐
          │  items.set(…)    │   writes are recorded, not delivered
          │  count.set(2)    ┘
          │
          └─ the handler's sync part ends → the turn SETTLES
                 │
                 ├─▶ the count watcher runs once   (sees 2 — never 1)
                 └─▶ the items watcher runs once

one turn  =  one consistent wave, no matter how many writes
```

You mostly never manage turns, because the framework opens them at its
boundaries: every UI event handler runs in one, every `mount_root` build
runs in one, and every rpc handler on the server runs in one. This is
like React's automatic batching, generalized.

For the rare explicit cases:

```vilan,fragment
turn(policy, || …)   // run a block in a fresh turn; an awaiting body HOLDS it
batch(|| …)          // join the current turn, or create one
flush()              // drain the ambient turn early
```

> **Going deeper.** Suspension is where the shapes differ. An explicit
> `turn` adapts to its body: a synchronous body settles when it ends
> (the atomic turn), and an awaiting body holds every notification
> (before the first await and in every continuation) until the whole
> body finishes, then settles once: a true transaction. A *boundary*
> turn around a fire-and-forget handler (a UI event) can't wait for the
> handler's continuations, so it settles at the end of each synchronous
> stretch, one wave per segment:
>
> ```text
> handler:              |── writes ──|─── await ───|── writes ──|
>
> boundary turn:                   settle ▲              settle ▲
> (a UI event)                     (wave 1)              (wave 2)
>
> turn, awaiting body:                                   settle ▲
>                                                      (one wave)
> ```
>
> Writes that land after a turn already settled (from spawned work) are
> grouped per continuation segment and drained in a microtask, so you
> never observe half a wave.

## Optimistic writes and local-first drafts

Two ready-made lifecycles for "update the UI now, confirm with the
server after". They differ in what happens on failure, and the
difference is the point:

**`optimistic(signal, value, commit)`** paints the value immediately,
runs your async commit, and on failure rolls back. Use it for
one-shot actions like a delete button: if the delete failed, the row
should come back. When the write needs *watching* — a spinner, a button
that shouldn't fire twice, a failure banner — reach for the
[`Optimistic` cell](#watching-an-optimistic-write-land) below instead.

**`draft(initial, commit)`** is for *editing*. It keeps the user's text
on failure (rolling back mid-typing would eat their input) and retries
naturally on the next push. Bind an input to a draft and every keystroke
can safely commit through an rpc:

```vilan,fragment
struct Draft<T> {
	local: SignalCell<T>,          // bind inputs to this
	state: SignalCell<DraftState>, // Synced | Dirty | Failed(str)
	…
}
draft<T: PartialEq>(initial: T, commit: async |T| Option<str>): Draft<T>
draft.push(value)   // set local + spawn the commit (never waits on the wire)
draft.adopt(remote) // fold in a remote change
draft.commit()      // send now (the explicit save)
draft.repush()      // re-send if the remote never got the current value
```

The commit closure returns `None` on success or `Some(reason)` on
failure, so an rpc-calling closure drops straight in.

The whole lifecycle in one picture. The input never waits on
the wire, and every remote change funnels through `adopt`'s three rules:

```text
you type ──▶ local (Signal) ──▶ the input shows it INSTANTLY
                │
                └─ push: spawn the commit ──▶ rpc ──▶ server
                                                        │
                          the mirror broadcasts  ◀──────┘
                                │
                             adopt(remote):
                    ├─ same as last synced?  an ECHO — do nothing
                    ├─ local has no edits?   take the remote value
                    └─ local is DIRTY?       your text wins for now
```

```vilan
import std::reactive::{ draft, Draft, DraftState };
import std::option::Option::{ self, Some, None };
import std::shared::Shared;

fun main() {
	let saved: Shared<List<str>> = Shared::new([]);
	let name = draft("seed", |value: str| {
		saved.write().push(value);
		None
	});
	name.push("edit");         // local is "edit" immediately
	print(name.local.get());
	name.adopt("edit");        // the server echoing it back: no-op
	name.adopt("remote-edit"); // a genuine remote change: adopted (local is clean)
	print(name.local.get());
}
```

> **Going deeper.** `push` is per-keystroke safe: a generation counter
> means a slow older commit that lands late is discarded rather than
> clobbering a newer one. `adopt` follows three rules: an **echo** of
> your own push changes nothing, a **clean** local adopts the remote
> edit, and a **dirty** local wins (last-write-wins: the remote value is
> remembered so your eventual push knowingly overwrites it). The
> [reactive reference](../std/reactive.md) states all of it precisely,
> and `bind_draft` in [Building UI](ui.md) is the input-side wiring.

### One commit per burst, not per keystroke

Per-keystroke-*safe* is not per-keystroke-*cheap*: a bound input sends a
frame for every character. `debounce(millis)` coalesces a burst into one
commit, and it does **not** slow the typing down — `local` and the `Dirty`
state still land the instant you press a key, so the input is as immediate
as ever. Only the commit waits for you to stop:

```vilan
import std::reactive::{ draft, Draft, DraftState };
import std::option::Option::{ self, Some, None };
import std::shared::Shared;
import std::time::{ sleep_for, Duration };

fun main() {
	let saved: Shared<List<str>> = Shared::new([]);
	let notes = draft("", |value: str| {
		saved.write().push(value);
		None
	}).debounce(30);

	notes.push("h");
	notes.push("he");
	notes.push("hey");
	print(notes.local.get());       // "hey" — instantly, nothing was delayed
	print(saved.read().len());      // 0 — the window is still open

	sleep_for(Duration::millis(150));
	print(saved.read().len());      // 1 — one commit for the whole burst
	print(saved.read()[0]);         // "hey" — the value you ended on
}
```

The commit fires after the last push (trailing edge). `commit()` — a blur
handler, a Save button — cancels a pending window and sends immediately, so
an explicit save costs one commit rather than yours plus the window's.

### Surviving a dropped connection

A draft edited while the connection is down keeps the user's text, but
nothing re-sends it on its own: the cell holds an opaque commit closure and
has no idea what transport it rides, so it cannot notice a reconnect. You
connect the two, in one line:

```vilan,fragment
let title = draft(page.title, |value: str| { … client.rename(value) … });
client.transport.on_reconnect(|| title.repush());
```

`repush()` re-sends only if the remote never got the current value — a clean
draft does nothing, so a screen full of untouched drafts costs nothing on
reconnect. It is also what a "retry" button in a failure banner calls.

> **The honest part.** Delivery is *at-least-once*: a commit the server
> applied but could not acknowledge before the socket died looks exactly
> like one that never arrived, so it gets sent twice. That is harmless for
> the shape drafts are built for — "set this field to this value" — and it
> is not for a commit that appends. And a re-push that fails is not retried
> on a timer; it rides the next reconnect, so a value the server keeps
> refusing can't spin.

### Watching an optimistic write land

`optimistic` hands the outcome back to whoever called it, and to no one
else. That is enough for a write you await and immediately branch on, and
not enough for the usual case: a button that should grey out while its
write is in flight, and a banner that should say why it failed.

`Optimistic::over(signal)` wraps the signal you already have — no binding
changes — and adds a `state` signal to bind. Any `Signal<T>` fits, your own
implementations included; the cell's type carries the signal's
(`Optimistic<T, S>`), and inference fills both in from the call:

```vilan
import std::reactive::{ Signal, SignalCell, Optimistic, WriteState };
import std::result::Result::{ self, Ok, Err };

fun main() {
	let title = Signal::new("Draft post");
	let saving = Optimistic::over(title);

	// A write the server refuses. "Published" is painted first, so the UI
	// never waits; the rejection rolls it back and says why.
	let _refused = saving.write("Published", || {
		let reply: Result<str, str> = Err("not allowed");
		reply
	});
	print(title.get());
	print(saving.state.get() == WriteState::Rejected("not allowed"));

	// A write it accepts, answering with its own value — that value wins,
	// not the one you painted.
	let _accepted = saving.write("Published", || {
		let reply: Result<str, str> = Ok("Published (v3)");
		reply
	});
	print(title.get());
	print(saving.state.get() == WriteState::Confirmed);
}
```

`state` is `Confirmed`, `Pending`, or `Rejected(reason)`, and the value and
the state are always published *together* — an observer of both never
catches the cell mid-transition.

> **Going deeper.** The cell also fixes something you can't fix from
> outside: two writes in flight over one signal. Through the free
> function, an older write failing *after* a newer one succeeded rolls the
> newer value away, leaving the screen showing something the server
> stopped holding two writes ago. The cell discards a superseded outcome —
> the newest write owns the cell — and it rolls back to the last value the
> **server** confirmed rather than to whatever the signal held when the
> write started. Unlike a draft, it does **not** re-send on reconnect: a
> re-send is at-least-once, which is fine for "set this field to this
> value" and not for an action you'd rather not perform twice. The
> rollback is the recovery.

## Keyed reconciliation

`reconcile(old_keys, old_items, new_items, key)` computes a minimal
update plan for keyed lists (keep this row, refresh that one, these are
gone). It's the pure engine underneath `ui`'s `bind_each`. You'd only
call it directly to build your own list-rendering primitive.

## Traps

- `sub` gives you a `Subscription` to dispose manually. Prefer `effect`
  and let the owner handle it.
- Disposal stops *future* deliveries. A watcher already queued in the
  currently-settling turn may fire one final time.
- Derived signals (`map`/`combine`/`flatten`) take the ambient owner when
  there is one, so a derivation built inside a view dies with the view.
  Built where no owner is ambient — module level, the top of `main` — it
  lives as long as its source, which is what a module-level
  `current_path().map(parse)` is for. Either way you never hold a handle.
