# std::time reference

Instants, durations, and timers. Both types are Wire: they ride rpc
payloads (`created_at: Instant` in a mirrored record is the standard
timestamp shape).

```vilan,fragment
import std::time::{ now, Instant, Duration, sleep, sleep_for, Timer, Debounce };
```

## Instant

A moment in time: epoch milliseconds in an `i53` under the hood.

```vilan,fragment
fun now(): Instant                       // the current wall-clock moment

impl Instant {
	fun since(self, earlier: Instant): Duration   // self - earlier
	fun to_iso(self): str                         // ISO-8601, via the host clock
}
impl Instant with Add<Duration> { … }    // instant + duration → Instant
impl Instant with Sub<Duration> { … }    // instant - duration → Instant
impl Instant with PartialOrd { … }       // <, >, == between instants
```

## Duration

```vilan,fragment
impl Duration {
	// constructors
	fun millis(count: i53): Duration
	fun seconds(count: i53): Duration
	fun minutes(count: i53): Duration
	fun hours(count: i53): Duration
	fun days(count: i53): Duration
	// truncating accessors
	fun as_seconds(self): i53
	fun as_minutes(self): i53
	fun as_hours(self): i53
	fun as_days(self): i53
	// human text: "42 seconds", "3 hours" …
	fun describe(self): str
}
impl Duration with Add { … }             // duration + duration
impl Duration with Sub { … }
impl Duration with PartialOrd { … }
```

The `"{age} ago"` UI idiom:

```vilan
import std::time::{ now, Instant, Duration };

fun main() {
	let started = now();
	let deadline = started + Duration::hours(2i53);
	print(deadline.since(started).describe());
	print(started < deadline);
}
```

Ordering two instants (or durations) is the `<`/`<=`/`>`/`>=` operators
dispatching through their `PartialOrd` impls, the same
`partial_compare` you'd call by hand.

## Sleeping

```vilan,fragment
fun sleep(ms: i32)                  // suspend (async; callers implicitly await)
fun sleep_for(duration: Duration)
```

Inside a nursery's extent a sleep carries the ambient cancel signal, so
cancelling the nursery cuts it short instead of waiting the timer out.

## Timer

A sleep nobody can reach into is sometimes not what you want: you need to
start a delay now, hand it around, and later either learn that it fired or
take it back. That's a `Timer`: `setTimeout` and `clearTimeout` as one
value.

```vilan,fragment
impl Timer {
	fun after(ms: i32): Timer                  // starts NOW
	fun after_for(duration: Duration): Timer
	fun wait(self): bool                       // async; true = fired, false = cancelled
	fun cancel(self)
}
```

A timer starts on construction and settles exactly once, on a **verdict**:
`true` if it fired, `false` if `cancel()` got there first. The verdict is
remembered, so every waiter sees the same answer (one parked before it
settled, one arriving long after), and asking a settled timer is immediate.
`cancel()` is idempotent: cancelling twice, or cancelling a timer that
already fired, does nothing.

```vilan
import std::time::Timer;

fun main() {
	let revert = Timer::after(2400);
	// … something happens that makes the pending revert wrong …
	revert.cancel();
	print(revert.wait());   // false — it never fired
}
```

That is the re-clickable-button shape: keep the timer in hand, and a new
click cancels the one still pending before starting its own.

`Timer` is an ordinary value wrapping one host handle, the way a `Signal`
wraps one cell. Copying it (assigning, passing, storing it in a field)
shares the same timer, so cancelling through any copy settles them all.

### Timers and nurseries

`wait` carries the ambient cancel signal as `sleep` does, and the
difference between the two cancellations matters:

- **`timer.cancel()`** is a verdict. The host timer is cleared and every
  waiter resolves `false`.
- **A cancelling nursery** tears down the task that was awaiting (the
  structured path) and does *not* touch the timer. No verdict, no
  `clearTimeout`: the timer belongs to whoever holds the value, so its other
  holders can still wait on it, or call it off themselves.

## Debounce

`Debounce` collapses a burst of calls into **one**, `delay` after the last of
them — trailing edge. Each `run` pushes the deadline out *and* replaces the
callback, and there is at most one timer in flight however many times `run` is
called, so a burst costs one run of the **last** closure.

```vilan,fragment
struct Debounce { … }
impl Debounce {
	fun new(delay: Duration): Debounce
	fun run(self, fn: || void)   // pushes the deadline out, replaces the callback
	fun cancel(self)             // drops the pending callback, calls the timer off
}
```

```vilan
import std::io::print;
import std::time::{ Debounce, Duration, sleep };

async fun main() {
	let save = Debounce::new(Duration::millis(300));
	save.run(|| print("first"));
	save.run(|| print("second"));
	save.run(|| print("third"));
	sleep(600);   // one line printed: "third"
}
```

**Trailing, not leading.** The first call of a burst does not fire; the last one
does, 300 ms after it. That is what a search field wants (one query, for the
text the user stopped typing) and what a resize handler wants (one layout pass,
at the size the window settled at). A leading-edge `Throttle` is deliberately
**not** shipped beside it: it is a different mechanism rather than a flag on
this one — it fires immediately and then *suppresses*, so it holds no pending
callback and its timer gates rather than schedules — and it lands when something
asks for it.

**A call that arrives during a wait cannot fire early.** The driving loop
re-reads the deadline after every wait, so the timer armed for an earlier call
fires, finds the deadline moved, and takes another turn.

`cancel()` drops the pending callback and calls the in-flight timer off, so
nothing fires and the host is released now rather than at the deadline. It is
idempotent, safe with nothing pending, and a `run` after it starts a fresh
window in the ordinary way.

Like `Timer`, a `Debounce` is a value wrapping shared cells: copying it shares
the one debounce, and `run` through any copy pushes the same deadline.

**It takes no owner and registers no cleanup.** `std::time` is the base clock
module and sits below `std::reactive`, so a `Debounce` is a plain value in the
shape `Timer` established — `cancel()` is the teardown, and an app that wants
one tied to a scope wraps it itself. A pending debounce keeps the host alive
until it fires, exactly as the outstanding `Timer` inside it does.

### Why the `Shared` cells

They are load-bearing, in both directions of the memory model
([spec §6](../spec/memory.md)). `run` takes `self` by loan and the `async` block
it spawns **outlives that loan** — it reads the pending deadline on every timer
tick, long after `run` returned. Rule 1 (§6.1) is why a plain field cannot
serve: values are copied, so the block would hold a copy of the deadline made at
spawn time and never see a later `run`'s push, and the copy it wrote would be
discarded. `Shared` is the cell that makes the escape legal, and rule 3 (§6.3)
is what lets the block **write** through what is only a loan — `write()` is a
view projected from the cell, not from `self`'s stack slot.

## Notes

- Duration constructors take `i53`; remember the suffix on computed
  literals (`Duration::millis(500i53 * factor)`); see
  [gotchas](../appendix/gotchas.md).
- `now()` is a host call, so it can't be `const`-folded, and programs using
  it aren't output-deterministic; keep it out of golden-file tests.
- A pending `Timer` keeps the host alive: on Node the process will not exit
  while one is outstanding, just as an outstanding `sleep` holds it open.
  There is no unref knob; cancel the timer if the program should be free to
  end.
- Wire format: an `Instant` serializes as its `i53` millis, exact for any
  realistic date (i53 rides the wire as a float's 53 bits, safe past year
  200,000).
