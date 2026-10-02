# Tracked reads

The [reactive guide](reactive.md) builds every derivation from its inputs:
`count.derive(|n| n * 2)` depends on `count` because `count` is where the pipe
starts, and `combine((a, b))` depends on `a` and `b` because it names them.
That is the base layer, and it is all you ever need.

Tracked reads are an optional layer on top of it. Inside a body a pipe runs, a
source read with `.track()` instead of `.get()` becomes a dependency of that
body:

```vilan
import std::reactive::{ Signal, SignalCell, derive };

fun main() {
	let price: SignalCell<i32> = Signal::new(3);
	let quantity: SignalCell<i32> = Signal::new(2);
	let total = derive(|| price.track() * quantity.track()).memo();
	print(total.get());   // 6
	quantity.set(5);
	print(total.get());   // 15
}
```

Nothing is tracked unless a body says `track()`. `get()` never registers
anything, anywhere, so code that does not opt in pays nothing and behaves
exactly as before.

## Which bodies track

Every body a pipe runs opens a **tracking scope** for the length of one run:

- a `derive` transform: `count.derive(|c| c + offset.track())` follows `offset`
  as well as `count`;
- a `switch`, `switch_some` or `and_then` selector: a tracked read there is a
  dependency of the *selection*, so a change re-selects;
- an `effect` body: a change re-runs the body with the value its input
  delivered last;
- the free `derive(|| body)`: a pipe whose only dependencies are the ones its
  body tracks.

The scope is a context (`std::reactive::tracking`), and `track()` reads it
through the strict `get`. A `track()` that no body encloses — at the top of
`main`, in a helper called only from there, in a callback written outside every
body — is a **compile-time error**, the same coverage error as any context read
with no enclosing `run`. There is no runtime lookup and no runtime "not tracking"
case: the compiler threads the scope as a hidden parameter, exactly as it threads
the ambient owner.

A helper function may call `track()`. It then needs a scope, and the compiler
checks that every call to it has one:

```vilan
import std::reactive::{ Signal, SignalCell, Source, derive };

fun area(width: SignalCell<i32>, height: SignalCell<i32>): i32 {
	width.track() * height.track()   // tracks into whichever body calls it
}

fun main() {
	let width: SignalCell<i32> = Signal::new(2);
	let height: SignalCell<i32> = Signal::new(3);
	let sealed = derive(|| area(width, height)).memo();
	height.set(10);
	print(sealed.get());   // 20
}
```

## Dynamic dependencies

A body's dependencies are what its **last run** read. After every run the stage
compares that run's reads with the edges it holds: an edge to a source read
again is kept, a source read for the first time gets a new edge, and an edge
nothing read this time is released. A branch that is not taken stops waking the
body:

```vilan
import std::reactive::{ Owner, Signal, SignalCell, derive, run_with_owner };

fun main() {
	let signed_in: SignalCell<bool> = Signal::new(false);
	let name: SignalCell<str> = Signal::new("Ada");
	let owner = Owner::new();
	run_with_owner(owner, || {
		derive(|| if signed_in.track() { name.track() } else { "guest" })
			.effect(|who: str| print(i"hello, {who}"));
	});
	name.set("Grace");      // no run: the last run never read `name`
	signed_in.set(true);    // hello, Grace
	name.set("Lin");        // hello, Lin
	owner.dispose();
}
```

This is what `switch` does for a *structural* dependency — follow whichever
source the current value selects — written as ordinary control flow.

Keeping an edge rather than re-attaching it needs the source to say which state
it is. `Source::identity()` answers that: a `SignalCell`, a sealed `.memo()`, a
`ListCell`, a `KeyedCell` and a remote mirror (`RemoteSource`, `KeyedSource`)
name their cell, so a body that reads the same one on every run holds one edge
on it for its whole life — for a mirror, one lease. A source that answers `None` (the default for a type
you write yourself) still works — each run attaches a fresh edge and releases
the previous one.

## Glitch-free

A tracked edge wakes its body in the turn's first phase, the phase in which
derivations settle, and the consumer below it runs in the second. So in a
[turn](reactive.md#turns-when-changes-become-visible) a body that reads two values
derived from one root runs once per change, on the settled pair — never once with
a fresh value and a stale one. (With no turn at all a write notifies inline and
depth-first, as it does for every consumer, `combine` included.)

```vilan
import std::reactive::{ FlushPolicy, Owner, Signal, SignalCell, derive, run_with_owner, turn };

fun main() {
	let root: SignalCell<i32> = Signal::new(1);
	let tens = root.derive(|x: i32| x * 10).memo();
	let hundreds = root.derive(|x: i32| x * 100).memo();
	let owner = Owner::new();
	run_with_owner(owner, || {
		derive(|| tens.track() + hundreds.track())
			.effect(|sum: i32| print(i"sum {sum}"));   // sum 110
	});
	turn(FlushPolicy::AtEnd, || root.set(2));        // sum 220, once
	owner.dispose();
}
```

The same holds for an effect whose input and tracked reads change in one turn:
it runs once, with both.

## A scope is one run

A closure created inside a body captures that run's scope (the
[contexts chapter](../spec/contexts.md) calls this capture-at-creation). It can
outlive the run — a callback the body hands out, a closure stored for later —
and a `track()` through it after the run has returned registers nothing: it is
a `get()`. A body's dependencies are only ever the reads of its own runs.

To make that explicit, and to make a `track()` there a compile-time error
instead, create the closure inside `tracking.clear(..)`. `clear` runs its body
with the scope **not** established: a closure minted inside holds no scope at
all, and a `track()` written inside is refused. It is the layer's `untrack`:

```vilan
import std::reactive::{ Signal, SignalCell, derive, tracking };

fun main() {
	let followed: SignalCell<i32> = Signal::new(1);
	let ignored: SignalCell<i32> = Signal::new(10);
	let sealed = derive(|| {
		let noted = tracking.clear(|| ignored.get());   // read, never followed
		followed.track() + noted
	}).memo();
	ignored.set(20);          // no run
	followed.set(2);          // runs: 2 + 20
	print(sealed.get());      // 22
}
```

## Callbacks do not track

`on_change`, `sub`, `effect_on_change` and the UI event handlers (`on`,
`on_event`) take **callbacks**, not bodies: they open no scope of their own, and
they run their callback with `tracking` **cleared**. So a `track()` in a callback
is refused at compile time wherever the callback is written — inside a body or
outside every body — and `tracking.get_safe()` in one reads `None`. A callback
never becomes a hidden dependency of anything.

A callback position's parameter is typed `context tracking`, so it takes a
closure literal, a named function, or a value whose type carries the same
clause. A closure VALUE typed without it — `fun watch(react: |i32| void)` handing
`react` on — is refused there; wrap it in a literal
(`cell.on_change(|value| react(value))`) or type it
`(|i32| void) context tracking`.

`on_change` over a pipe that tracks still hears its first change: a consumer
that does not read at once *primes* the pipe — it runs the pipe's bodies once,
discarding the value, so their tracked reads are dependencies from the start.
The callback itself still runs only on a change.

## Collections do not track per element

A collection operator's closure (`map`, `filter`, …) runs once per element and
opens no scope: tracking there is a choice you spell, not one the operator makes
for every element of every list.

## What it costs

- A stage whose body tracks nothing pays one cell per instance (where its runs
  stand, made when a consumer starts it) and, per run, an epoch bump and a scope
  value: no edge, no relay, no list. The lists that record what a body read are
  made at its first `track()`.
- Each tracked source holds one edge per body that tracked it, reused across
  runs when the source can name its identity.
- A tracked read is checked against the run's earlier reads, so reading one
  source twice in a run still makes one edge (for a source that names its
  identity; one that cannot is followed once per read).
