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
reactive contract, `set` and `notify` over `Source`'s `get` and `on_change` — and
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
any other write. `update` works for any `T` a closure can mutate: `HashMap`,
`HashSet`, a struct's fields, a nested aggregate.

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

## Derived state: `derive`, pipes and `.memo()`

Build state as a graph and let it compute itself. Two kinds of reactive value
make up the graph:

- A **source** has state and can be read: a `SignalCell`, a constant, an rpc
  mirror, and a sealed derivation. `get()` reads it, and copying one copies a
  handle to the same state.
- A **pipe** is a transformation: `count.derive(|n| n * 2)`. It is a
  *description* — the source it reads and what it does to it — with no `get()`
  of its own, and it runs only once something consumes it.

A pipe is consumed exactly **once**: by a consumer (an `effect`, a UI binding),
or by **sealing** it into a source.

```vilan
import std::reactive::{ Signal, SignalCell };

fun main() {
	let count = Signal::new(2);
	let doubled = count.derive(|n: i32| n * 2).memo();   // seal: a source again
	print(doubled.get());
	count.set(5);
	print(doubled.get());
}
```

- `source.derive(transform)` gives a pipe of the transformed value.
- `combine((a, b, …))` gives a pipe of the tuple of several sources' values,
  changing when any of them changes. Takes two or more; each input is a source,
  so a derived input is sealed first.
- `.memo()` seals a pipe into a read-only source backed by one cell: the pipe
  runs once per change of its input, and every reader shares the value.
  `.cell()` seals into a writable `SignalCell` instead (a local `set` holds until
  the next change upstream overwrites it). `.sample()` reads a pipe once — start
  it, take its value, release it — and subscribes to nothing.

```vilan
import std::reactive::{ Signal, SignalCell, combine };

fun main() {
	let first = Signal::new("Ada");
	let last = Signal::new("Lovelace");
	let full = combine((first, last))
		.derive(|pair: (str, str)| {
			let (a, b) = pair;
			a + " " + b
		})
		.memo();
	print(full.get());
	first.set("Grace");
	print(full.get());
}
```

**Why a pipe is consumed once.** Every stage of a pipe runs inside the one
consumer that started it, once per change of its input — five stages sealed
into one `.memo()` put one subscription on the root and run once per change, in
one pass. Handing the same pipe to two consumers would run it twice, so the
compiler refuses it: the second use is "use of `p` after it was moved". Two
readers of one derived value need a source between them, and sealing is that
source:

```vilan
import std::reactive::{ Signal, SignalCell };

fun main() {
	let cart = Signal::new(3);
	let total = cart.derive(|n: i32| n * 25).memo();   // one instance, one run per change
	let big = total.derive(|cents: i32| cents > 100).memo();
	let label = total.derive(|cents: i32| i"{cents} cents").memo();
	print(i"{label.get()} {big.get()}");
}
```

A source is data — pass it anywhere, as often as you like. A pipe is built where
it is consumed, and each consumer builds its own over a shared source.

A dependency is **static** when the expression fixes what the result reads —
that is `derive` and `combine` — and **dynamic** when the current value decides
*which* source to follow next, which is what "the selected channel's unread
count" needs:

- `source.switch(select)` follows whichever flow `select` builds for the current
  value and re-selects when this one changes — Rx's `switchMap`. `select` runs
  once per change, and whatever the previous selection built is released.
- `source.switch_some(select)` is the same over an `Option`: `None` detaches and
  reads `None`, `Some(x)` follows what `select` builds for `x`, wrapped.
- `source.and_then(select)` is the Kleisli form over a `Source<Option<T>>`:
  `select` answers a `Source<Option<U>>`, and the two absences collapse into
  one. It is `Option::and_then` one level up.
- `flag.then_some(source)` follows `source` while the flag is true and reads
  `None` (holding nothing) while it is false.
- `nested.flatten()` over a source of `Option<source>` follows whichever inner
  source is current and detaches from a replaced one (`None` reads `None`). The
  total join of a source of sources is `nested.switch(|inner| inner)`.

```vilan
import std::reactive::{ Signal, SignalCell };

fun main() {
	let which = Signal::new(0);
	let first = Signal::new(10);
	let second = Signal::new(20);
	let picked = which.switch(|n: i32| if n == 0 { first } else { second }).memo();
	print(picked.get());     // 10
	first.set(11);
	print(picked.get());     // 11 — the current inner drives the result
	which.set(1);
	print(picked.get());     // 20 — the switch follows the new inner
	first.set(99);
	print(picked.get());     // 20 — and detaches from the replaced one
}
```

A selector must **build** the flow it answers: a closure cannot capture a pipe
built outside it (a pipe is consumed once, and the selector runs once per
change). Selecting between sources, as above, is always fine — a source is data.

When an unchanged value should stay quiet, `.distinct()` passes a change on only
when the value differs from the last one (it asks `T: PartialEq`), and
`.distinct_by(key)` compares `key` of the value. Sealing does not compare: every
change above a `.memo()` is a write, and a write always notifies.

`Source::constant(value)` is a source that never changes — the way to fit a
static value into a position that asks for a reactive one.

### Where a derivation lives: `.memo()` and `dyn Source<T>`

- **A pipe is built where it is consumed.** A binding over a derivation takes
  the pipe directly: `text.bind_text(count.derive(|n| i"{n}"))`. Nothing is
  stored, and the binding is the pipe's one consumer.
- **`.memo()` where you share it or read it.** A derivation read by two
  consumers, or read with `get()`, is sealed once and shared. `.memo()` can sit
  anywhere in a chain — after the expensive part, not at every step.
- **`dyn Source<T>` where you store it.** A struct field names a type, and two
  derivations built differently are two types. A field of type `dyn Source<T>`
  holds any source — a cell, a sealed memo, a mirror — and a list of such
  structs mixes them freely. A pipe is not a source, so it is sealed first.

```vilan
import std::reactive::{ Signal, SignalCell, Source };

struct Label {
	text: dyn Source<str>,
}

fun main() {
	let count = Signal::new(2);
	let total = count.derive(|n: i32| n * 100).memo();   // shared below: seal it
	let labels: List<Label> = [
		Label { text = Signal::new("fixed") },
		Label { text = total.derive(|cents: i32| i"{cents} cents").memo() },
	];
	count.set(3);
	for label in labels {
		print(label.text.get());
	}
	print(total.get());
}
```

**What a pipe's type is, and where you write it.** `derive` answers a
`Derive<S, T, U>`, `switch` a `Switch`, `switch_some` and `flatten` a
`SwitchSome`, `and_then` an `AndThen`, `combine` a `Combine` — types you rarely
spell. Leave a binding unannotated, take a parameter as a `Flow<T>` bound (any
source or pipe satisfies it; write `own` on it, since a pipe is consumed), or
seal it where a `MemoCell<T>` or `SignalCell<T>` is what you mean. A struct that
holds a pipe becomes move-only itself, so a model layer stores sealed sources, or
offers methods that build pipes on request.

**At module level, `.memo_global()`.** A `.memo()` ties its subscription to the
ambient owner, and a module binding's initializer has none, so there the compiler
refuses `.memo()` (and `.cell()`) and steers you to build it under the owner that
reads it — or to write `.memo_global()` (`.cell_global()`), which says the value
lives for the program:

```vilan
import std::reactive::{ Signal, SignalCell, MemoCell, Source };

let path: SignalCell<str> = Signal::new("/docs/intro");
let segments: MemoCell<usize> = path.derive(|value: str| value.len()).memo_global();

fun main() {
	print(segments.get());
	path.set("/");
	print(segments.get());
}
```

### Selection over a list: `selector`

`derive` is the wrong tool for one particular shape — "is *this* row the
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
`each` row and the key's entry is released when the row is — the
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
- `on_cleanup(|| …)` is that last line without naming the owner — the
  spelling to reach for.

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

### Who cleans up what

Six rules, and they are the whole answer:

| What you wrote | Who releases the observer | When |
|---|---|---|
| `signal.effect(..)` / `effect_on_change(..)` | the ambient owner (required, *statically*) | the boundary is disposed |
| `signal.sub(..)` / `on_change(..)` / `observe(..)` | **nobody** — you hold the `Subscription` | you call `dispose()`, or the owner you gave it to is disposed |
| `derive` / `combine` / `flatten` / `switch` / `and_then` | nothing to release — a pipe registers nothing until it is consumed | — |
| `.memo()` / `.cell()` / `selector` **inside** a boundary | the ambient owner | the boundary is disposed |
| `.memo()` / `.cell()` / `selector` **outside** every boundary (a function body), `.memo_global()` / `.cell_global()` anywhere | nobody — it lives as long as its source | never (deliberate: see below) |
| anything an `effect` body, a `derive` body or a `switch` selector registers — and any task it starts | that **run's** owner | before the next run, and with the boundary (a task is cancelled) |

Two of those rows are worth a sentence.

**Dropping a `Subscription` does not unsubscribe it.** There are no
destructors here, so a handle you forget about keeps firing. Hold it and
`dispose()` it, hand it to an owner (`owner.take(..)`), or use `effect`,
which does that for you — and `effect` is the one to reach for.

**A cached derivation made outside every boundary lives as long as its
source, on purpose.** `current_path().derive(|path| parse(path))` at the top of
`main` is a pipe and costs nothing until something consumes it; a `.memo()` of it there is
meant to last as long as the program. Refusing that would be the stricter rule
and would break the idiom, so vilan does not — except in a module binding's
initializer, where the lifetime is spelled `.memo_global()`. Inside a boundary
a `.memo()` dies with the boundary, which is what a component wants. A
*mirror* is where the owner is asked strictly: an `effect` on a
`RemoteSource` (or on a pipe over one) requires an owner, because its
subscription costs a network frame.

A disposed owner is **single-use**: a `take` or `defer` that arrives
after it was disposed runs the cleanup on the spot rather than parking
it on a list nothing will read again. That is what makes ownership hold
across `await`.

### An owner per run

Every body a pipe runs — an `effect`'s, a `derive`'s, a `switch` selector — runs
**under an owner of its own, per run**. Whatever the body registers (an
`on_cleanup`, a nested `effect`, a `.memo()`, a mirror's lease) is released
**before the next run**, and the last run's with the boundary; a task the body
starts is cancelled at the same moment. So a body that *opens* something opens
one at a time:

```vilan
import std::reactive::{ Owner, Signal, Source, on_cleanup, run_with_owner };

fun main() {
	let selected = Signal::new(1);
	let detail = Signal::new("loading");
	let page = Owner::new();
	run_with_owner(page, || {
		selected.effect(|id: i32| {
			on_cleanup(|| print(i"closing {id}"));
			// One subscription on `detail` at a time, not one per selection.
			detail.effect(|text: str| print(i"{id}: {text}"));
		});
	});
	selected.set(2);   // closing 1 — then the new run subscribes
	page.dispose();    // closing 2
}
```

`on_cleanup` is one name whose meaning the ambient owner decides: inside a body
it is per run, inside any other boundary it is once, at teardown. A body that
registers nothing pays for no owner — the run's owner is allocated at its first
registration. (`scoped_effect` is the old name of this behaviour, kept one
release as a deprecated alias of `effect`.)

A task works the same way: a `derive` whose body starts a fetch cancels the
superseded one when its input changes, because the old run is released before
the new one starts:

```vilan
import std::reactive::{ Owner, Signal, Source, run_with_owner };
import std::time::sleep;

fun main() {
	let id = Signal::new(1);
	let page = Owner::new();
	run_with_owner(page, || {
		id.effect(|value: i32| {
			let _pending = async {
				sleep(20);
				print(i"loaded {value}");
			};
		});
	});
	id.set(2);          // run 1's task is cancelled; only `loaded 2` prints
	sleep(60);
	page.dispose();
}
```

Because a body's owner and nursery are INJECTED into it (a `context` clause), a
body must be a closure literal (or a local closure): `count.derive(|n| label(n))`,
not `count.derive(label)`.

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

`reconcile(old_keys, old_items, new_items, key, same)` computes a
minimal update plan for keyed lists (keep this row, refresh that one,
these are gone). It's the pure engine underneath `ui`'s `each`.
You'd only call it directly to build your own list-rendering primitive.
`key` decides identity — whether a row survives and moves — and `same`
decides, for a surviving key, whether the row is reused or rebuilt;
they're two questions, so they're two arguments.

The key is `PartialEq + Hashable`. The plan is found through a hash index
built over the old keys, so a key's hash must agree with its equality —
`a == b` implies `a.hash() == b.hash()`, which is what `std::hash` already
asks of a hand-written impl. Two keys that are *not* equal may share a
hash; that is an ordinary collision and costs a step along the chain. It
is the other direction — an equality coarser than the hash — that would
hide a moved row, and the bound is there so it cannot be written.

## Lists that know what changed: `ListCell` and `map_each`

A derived list over a `SignalCell<List<T>>` re-runs its function for
**every** element when one changes, because a `set` says only "the list is
this now":

```vilan,fragment
let rows: SignalCell<List<str>> = Signal::new([]);
let lengths = rows.derive(|list: List<str>| list.map(|text: str| text.len()));
// N calls of the inner function on every push
```

`ListCell<T>` is the same list with its writes recorded as *changes*, so a
derivation can run once for the element that arrived:

```vilan
import std::reactive::{ ListCell, SequenceCell, map_each };

fun main() {
	let rows: ListCell<str> = ListCell<str>::new();
	let lengths = map_each(rows, |text: str| text.len());
	rows.push("hello");     // ONE call
	rows.remove_at(0);      // none
	print(lengths.get().len());
}
```

It is an ordinary `Source<List<T>>` besides — `each`, `derive`, `effect` all
take it — and nothing that ignores the changes pays for them. `each`,
`each_values` and `each_by` use them: a push into a 1,000-row `ListCell`
builds one row, where the same push into a `SignalCell<List<T>>` re-reads
every key to find it.

Its mutators are `push`, `prepend`, `insert_at`, `insert_all`,
`remove_at`, `remove_range`, `pop`, `extend`, `clear`, `set_all`,
`truncate` and `is_empty`, and every one of them is a default over a
single `splice`, which is why none of them can forget to record what it
did. `edit` batches: hand it a body, make as many mutations as you like,
and the cell publishes once with one change per mutation.

```vilan
import std::reactive::{ ListCell, Sequence };

fun main() {
	let rows: ListCell<str> = ListCell<str>::new();
	rows.edit(|&mut list| {
		list.push("a");
		list.push("b");
		list.remove_at(0);
	});   // one notification, three changes
	print(rows.get().len());
}
```

Two things cost more, and say so: `set(whole_list)` records "the list
became this", which is every element again, and `reconcile_to(whole_list)`
diffs the ends and records only the span that moved — reach for it when a
whole list arrives from somewhere (a fetch, a form) and you want the
derivations to stay cheap. A `g` handed to `map_each` must be pure in its
element: its result is kept, and nothing re-runs it.

## Traps

- `sub` gives you a `Subscription` to dispose manually. Prefer `effect`
  and let the owner handle it.
- Disposal stops *future* deliveries. A watcher already queued in the
  currently-settling turn may fire one final time.
- Derivations (`derive`/`combine`/`flatten`/`switch`/`and_then`) are pipes:
  they register nothing until consumed, so there is nothing to dispose. A `.memo()` takes
  the ambient owner when there is one, so one built inside a view dies with
  the view; built at the top of `main` it lives as long as its source, and
  at module level it is spelled `.memo_global()`. Either way you never hold
  a handle.
- A pipe has one consumer. A derivation read in several places is sealed
  once with `.memo()` and the source shared; handing one pipe to two
  consumers is a compile error rather than silent duplicate work.
