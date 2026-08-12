# Collections reference

The container types: `List` (built in), `std::map::Map`, `std::set::Set`,
`std::range::Range`, and the `std::iterator` protocol underneath `for`.

## `List<T>`

Built in, with literal syntax: `[1, 2, 3]`. An empty literal needs a type
annotation (`let xs: List<str> = [];`).

```vilan,fragment
impl List<type T> {
	fun new(): List<T>
	fun push(&mut self, own item: T)
	fun pop(&mut self): Option<T>
	fun insert(&mut self, index: i32, value: T)   // panics out of bounds
	fun remove(&mut self, index: i32): T          // panics out of bounds
	fun len(self): i32
	fun is_empty(self): bool
	fun iter(self): ListIterator<T>              // the lazy cursor; see Iterator
	fun get(self, index: i32): Option<T>
	fun first(self): Option<T>
	fun last(self): Option<T>
	fun map<U>(self, fn: |T| U): List<U>
	fun filter(self, predicate: |T| bool): List<T>
	fun find(self, predicate: |T| bool): Option<T>
	fun fold<B>(self, init: B, fn: |B, T| B): B
	fun for_each(self, fn: |T| void)
	fun reverse(self): List<T>
	fun sort_by(own self, compare: |T, T| Ordering): List<T>   // stable
}
impl List<type T: Add + Default> { fun sum(self): T }
impl List<type T: Mul + Default> { fun product(self): T }
impl List<type T: Ord> { fun sort(self): List<T> }        // stable
impl List<type T: PartialEq> {
	fun contains(self, value: T): bool
	fun index_of(self, value: T): Option<i32>
}
impl List<type T: Display> { fun join(self, separator: str): str }
```

Indexing is `list[i]`; iterate with `for item in list` (copies) or
`for e in &mut list` (in-place views; see the
[memory model](../tour/memory-model.md)). `list.iter()` hands back a
`ListIterator<T>` — a cursor over a **snapshot** of the list, and the entry to
the [adapter chain](#iterator). The snapshot is rule 1 at work (the cursor
stores the list in a slot that outlives the call, so it copies), which means a
`push` after `iter()` is not walked, and that `iter()` itself costs a copy:

```vilan
import std::print;

fun main() {
	mut live = [1, 2];
	mut cursor = live.iter();
	live.push(3);
	mut total = 0;
	for value in cursor {
		total += value;   // 1 + 2 — the snapshot predates the push
	}
	print(total);
}
```

The methods that take `self` by value are pure — they return a new list and
leave the receiver alone. The mutating ones take `&mut self`: `push`, `pop`,
`insert`, `remove`.

"Leave the receiver alone" reaches the ELEMENTS, not just the spine: the list a
pure method returns never shares element storage with the receiver, so writing
through `xs.map(f)[0]` cannot show up in `xs[0]`. That is rule 1 (a value is
copied when it is stored), and it is why `push` and `sort_by` are written `own`
— they keep what they are given. A fresh value costs nothing there: `own` copies
a *place*, and a value with no other owner moves in.

```vilan
import std::print;

fun main() {
	let words = ["alpha", "beta", "gamma"];
	let lengths = words.map(|word| word.len());
	print(lengths.fold(0, |total, n| total + n));
	print(lengths.sum());
}
```

### Searching

`find` takes a predicate and short-circuits; `contains` and `index_of` take a
value and need `T: PartialEq`. Missing answers are `None`, never a panic.

```vilan
import std::print;

fun main() {
	let scores = [40, 91, 65];
	print(scores.find(|n| n > 50).unwrap_or(0));  // 91
	print(scores.contains(65));                   // true
	print(scores.index_of(65).unwrap_or(-1));     // 2
	print(scores.index_of(7).is_none());          // true
}
```

### Ordering

`sort` orders by `Ord`; `sort_by` takes a comparator returning
[`Ordering`](traits.md). Both are **stable** — elements the comparator calls
`Equal` keep their input order — and both return a new list, as does `reverse`.

```vilan
import std::{ print, compare::Ordering };

fun main() {
	let ns = [10, 2, 1];
	print(ns.sort()[0]);      // 1
	print(ns.reverse()[0]);   // 1
	print(ns.sort_by(|a, b| b.compare(a))[0]);   // 10 — descending
}
```

### Splicing

`insert` shifts the tail right (an `index` equal to `len()` appends); `remove`
shifts it left and returns the element. Both **panic** on an out-of-range
index, exactly as `list[i]` does — a bad index is a caller bug. Reach for
`get` when you don't know whether an index is live.

```vilan
import std::print;

fun main() {
	mut xs = [1, 2, 4];
	xs.insert(2, 3);
	print(xs.len());       // 4
	print(xs.remove(0));   // 1
	print(xs[0]);          // 2
}
```

### Joining

`join` renders each element through [`Display`](traits.md) and puts
`separator` between them. It lives in `std::display` beside the bound it
needs, so it is the one `List` method that takes an import — and calling it
without one names the import in the error.

```vilan
import std::{ print, display::Display };

fun main() {
	print(["alpha", "beta", "gamma"].join(", "));  // alpha, beta, gamma
	print([1, 2, 3].join("-"));                    // 1-2-3
}
```

## `Map<K, V>`

```vilan,fragment
impl Map<type K: Hashable, type V> {
	fun new(): Map<K, V>
	fun insert(&mut self, key: K, value: V)
	fun get(self, key: K): Option<V>
	fun contains_key(self, key: K): bool
	fun remove(&mut self, key: K)
	fun len(self): i32
	fun is_empty(self): bool
	fun keys(self): List<K>
	fun values(self): List<V>
	fun entries(self): List<(K, V)>
}
impl Map<type K: Hashable, type V: PartialEq> {
	fun contains_value(self, value: V): bool
}
impl List<(type K: Hashable, type V)> { fun to_map(self): Map<K, V> }
```

Keys compare **by value**. Scalars work directly, and so does a **backed enum**
— one with explicit backing values, which *is* that value at runtime, so the
backing value is the key and no derive is needed:

```vilan
import std::print;
import std::map::Map;

enum Align { Start = "flex-start", End = "flex-end" }

fun main() {
	mut widths: Map<Align, i32> = Map::new();
	widths.insert(Align::Start, 1);
	print(widths.get(Align::Start).unwrap_or(0)); // 1
}
```

An **unbacked** enum (`enum Plain { A, B }`) is not a key on its own: without a
backing value it lowers to an array, like a struct, so it needs the derive along
with every other aggregate. A struct, an unbacked or payload-carrying enum, or a
`List` key works as long as it is `Hashable`. Derive it:

```vilan
import std::print;
import std::map::Map;
import std::hash::Hashable;
import std::option::Option::{ self, Some, None };

[derive(Hashable)]
struct Point {
	x: i32,
	y: i32,
}

fun main() {
	mut seen: Map<Point, str> = Map::new();
	seen.insert(Point { x = 1, y = 2 }, "origin-ish");
	// A fresh, distinct Point with equal fields hits.
	match seen.get(Point { x = 1, y = 2 }) {
		Some(let label) => print(label), // origin-ish
		None => print("miss"),
	}
}
```

`keys()` returns the real `K`s (in insertion order), and the key is snapshot
on insert, so mutating the original afterward can't desync the map.

`entries()` pairs `keys()`/`values()` into one `List<(K, V)>` snapshot, so
walking both together needs no hand-zipping; `contains_value` (needing
`V: PartialEq`, unlike the rest of `Map`) is the value-side counterpart to
`contains_key`:

```vilan
import std::print;
import std::map::Map;

fun main() {
	mut scores: Map<str, i32> = Map::new();
	scores.insert("alice", 1);
	scores.insert("bob", 2);
	mut total = 0;
	for entry in scores.entries() {
		total = total + entry.1;   // entry.0 is the key, entry.1 the value
	}
	print(total);                        // 3
	print(scores.contains_value(2));     // true
	print(scores.contains_value(9));     // false
}
```

## `Set<T>`

```vilan,fragment
impl Set<type T: Hashable> {
	fun new(): Set<T>
	fun insert(&mut self, value: T)
	fun contains(self, value: T): bool
	fun remove(&mut self, value: T)
	fun len(self): i32
	fun is_empty(self): bool
	fun values(self): List<T>
	fun union(self, other: Set<T>): Set<T>
	fun intersection(self, other: Set<T>): Set<T>
	fun difference(self, other: Set<T>): Set<T>
}
impl List<type T: Hashable> { fun to_set(self): Set<T> }
```

Value-keyed like `Map` (element `T` must be `Hashable`); `for x in set`
iterates the elements in insertion order.

`union`/`intersection`/`difference` are the standard set operations, each
returning a new `Set` and leaving both receivers untouched:

```vilan
import std::print;
import std::set::Set;

fun main() {
	mut a: Set<i32> = Set::new();
	a.insert(1);
	a.insert(2);
	a.insert(3);
	mut b: Set<i32> = Set::new();
	b.insert(2);
	b.insert(3);
	b.insert(4);
	print(a.union(b).len());          // 4 -- {1, 2, 3, 4}
	print(a.intersection(b).len());   // 2 -- {2, 3}
	print(a.difference(b).len());     // 1 -- {1}
}
```

## `Hashable`

A key's value is turned into a `Hash` (a canonical key) by `key.hash()`. Three
routes reach it:

- **Scalars** (`str`, `bool`, every sized numeric) and `List`/`Option` of a
  `Hashable` — implemented by std.
- **A backed enum** — implemented by the compiler beside its `value()`/`parse()`
  (see the types chapter), off the same opt-in: writing `= "flex-start"` is what
  makes the enum that value, and the value is the key. So `Align::Start.hash()`
  and `Align::Start.value().hash()` are the same key, and a backed enum is
  accepted as a *field* of a derived type too. A `resource` enum is excluded — a
  resource cannot be hashed by value.
- **`[derive(Hashable)]`** — for a struct or an enum whose fields are all
  `Hashable` (scalars, `str`, `bool`, `List`/`Option` of `Hashable`, a backed
  enum, or another derived type); a closure, `Set`, `Map`, or `Shared` field is
  rejected. Writing it on a backed enum is harmless and does nothing.

You can also hand-write `impl Hashable` to key by a subset of fields — except on
a backed enum, whose impl the compiler already provides — and build your own
container by bounding on `K: Hashable` and keying a `Map<Hash, …>` yourself.

A `Hash` is opaque: you can hold it, compare two with `==`, hash it again (it is
itself `Hashable`, being already a canonical key), and use it as a key. You
cannot read the value inside, which is what keeps the representation free to
change. Equal values hash equal, so `==` on two hashes answers "same key?":

```vilan
import std::print;
import std::hash::Hashable;

[derive(Hashable)]
struct Point {
	x: i32,
	y: i32,
}

fun main() {
	let here = Point { x = 1, y = 2 };
	print(here.hash() == Point { x = 1, y = 2 }.hash()); // true
	print(here.hash() == Point { x = 9, y = 2 }.hash()); // false
}
```

Equality is the whole of a `Hash`'s operator surface — there is no order on a
canonical key, so `<` is a compile error rather than a lexicographic compare of
the underlying JSON.

One corner: a float *inside* an aggregate key canonicalizes through JSON, where
`NaN` becomes `null` and `-0`/`+0` collapse to `0`, so those collide. Bare
numeric keys don't have this (they key by JS value directly).

## Range

End-exclusive integer ranges, made for `for`:

```vilan,fragment
Range::new(start: i32, end: i32): Range   // start..end, end excluded
range.next(&mut self): Option<i32>
```

```vilan
import std::print;
import std::range::Range;

fun main() {
	mut total = 0;
	for i in Range::new(1, 5) {   // 1, 2, 3, 4
		total += i;
	}
	print(total);
}
```

## Iterator

The protocol `for` consumes, and the seam for custom sequences:

```vilan,fragment
trait Iterator<T> { fun next(&mut self): Option<T>; }
Iterator::from_fn(fn: || Option<T>): IteratorFromFn<T>   // an iterator from a closure
```

`next` takes `&mut self` because advancing *is* a mutation of the iterator's own
state — a cursor, a counter, a running total. Implement it on a struct of yours
and the type works in a `for` loop, and satisfies an `I: Iterator<T>` bound:

```vilan
import std::print;
import std::iterator::Iterator;
import std::option::Option::{ self, Some, None };

struct Countdown {
	remaining: i32,
}

impl Countdown with Iterator<i32> {
	fun next(&mut self): Option<i32> {
		if self.remaining <= 0 {
			None
		} else {
			self.remaining -= 1;
			Some(self.remaining)
		}
	}
}

fun main() {
	mut countdown = Countdown { remaining = 3 };
	for n in countdown {
		print(n);   // 2, 1, 0
	}
}
```

`Range` implements `Iterator<i32>`, and so does every adapter below.

One thing to know about the loop: `for` resolves the protocol on the *method
name*, so a type with a `next(&mut self): Option<T>` drives a loop whether or
not it declares the trait. Declaring it is what buys the adapters below — and
what lets a generic bound accept your type.

That resolution is ordinary method resolution, which means an **inherited
default counts**. If the trait supplies a `next` body and your impl block is
empty, your type still iterates — the same `next` you would reach by writing
`value.next()`:

```vilan
import std::print;
import std::option::Option::{ self, Some, None };

trait Countdown<T> {
	fun tick(&mut self): Option<T>;
	fun next(&mut self): Option<T> { self.tick() }   // inherited by every impl
}

struct Down { at: i32 }

impl Down with Countdown<i32> {
	fun tick(&mut self): Option<i32> {
		if self.at <= 0 { None } else { self.at -= 1; Some(self.at) }
	}
}

fun main() {
	mut down = Down { at = 3 };
	for n in down {
		print(n);   // 2, 1, 0
	}
}
```

Precedence is the usual one: a `next` declared on the type itself beats an
inherited one, and two *different* traits offering same-named defaults is an
ambiguity the loop reports rather than resolving for you — a `for` has no way to
name a provider, so declare `next` on the type to settle it.

The name is what resolves, but the **shape is still checked**: the method has to
return an `Option`, because the loop stops at `None`. A `next` that returns
anything else is a compile error rather than a loop that quietly runs zero times
— or, worse, throws. The return annotation is optional and the rule does not
care: annotate it and the annotation is checked, leave it off and the *body* is,
so `fun next(&mut self) { (self.fn)() }` is fine (it yields an `Option`) and `fun
next(&mut self) { self.count += 1; }` is not (it yields nothing).

### What a `for` can iterate

Exactly two things:

- **A type with `next`** (or `next_mut`, for `for e in &mut it`) — the protocol
  above, declared on the type or inherited from a trait default.
- **A natively iterable value**: `List<T>`, a fixed array `[T; n]`, a tuple, a
  `str` (yielding its characters), `Set<T>` (insertion order), and any host type
  an `external struct` names.

Anything else is a compile error. A struct or enum of your own that provides no
`next` is *not* iterable — it has no meaning to fall back on, since a struct is
its fields and an enum is its variant tag at runtime:

```vilan,fragment
struct Cursor { items: List<i32>, index: i32 }

fun main() {
	mut walked = Cursor { items = [1, 2], index = 0 };
	for item in walked {   // cannot iterate `Cursor`: it has no `next`
		print(item);
	}
}
```

`Map` is in that group: walk it through `entries()`, `keys()` or `values()`, as
the `Map` section above does.

### Adapters

Every `Iterator` gets these, as trait defaults — implement `next` and you have
all of them:

```vilan,fragment
fun map<U>(self, fn: |T| U): Mapped<Self, T, U>
fun filter(self, predicate: |T| bool): Filtered<Self, T>
fun take(self, count: i32): Taken<Self, T>
fun skip(self, count: i32): Skipped<Self, T>
fun enumerate(self): Enumerated<Self, T>                       // (0, a), (1, b), …
fun zip<U, J: Iterator<U>>(self, other: J): Zipped<Self, J, T, U>
fun chain<J: Iterator<T>>(self, other: J): Chained<Self, J, T>
```

They are **lazy**: each returns a small struct holding its upstream, and nothing
runs until something pulls. So a chain makes one pass over the source and builds
no intermediate lists.

```vilan
import std::print;

fun main() {
	mut pipeline = [1, 2, 3, 4, 5, 6]
		.iter()
		.filter(|n| n % 2 == 0)
		.map(|n| n * 10)
		.take(2);
	for value in pipeline {
		print(value);   // 20, 40
	}
}
```

Laziness is what makes `take` more than a convenience: it never pulls past its
budget, so it bounds a source that has no end.

```vilan
import std::print;
import std::iterator::Iterator;
import std::option::Option::{ self, Some, None };

struct Naturals {
	at: i32,
}

impl Naturals with Iterator<i32> {
	fun next(&mut self): Option<i32> {
		self.at += 1;
		Some(self.at)
	}
}

fun main() {
	mut squares = Naturals { at = 0 }.map(|n| n * n).take(3);
	for value in squares {
		print(value);   // 1, 4, 9
	}
}
```

`zip` stops with the **shorter** side, and `enumerate` numbers what reaches it —
put it after a `filter` and you get the positions in the *output*, not in the
source.

The adapter types are named in the past participle — `Mapped`, `Taken`,
`Filtered` — while the methods keep the plain names. That is deliberate: `Map`
is already a std type, and vilan's method resolution picks by registration order
rather than reporting a collision, so the type names stay out of each other's
way.

A `for` binding gets the element type the iterator was instantiated at, whatever
shape it is — a tuple, a struct, another container, a closure:

```vilan
import std::print;

struct Point { x: i32, y: i32 }

fun main() {
	for pair in [(1, "a"), (2, "b")].iter() {
		print(pair.0);                          // 1, then 2
	}
	for point in [Point { x = 1, y = 2 }].iter() {
		print(point.x);                         // 1
	}
	for inner in [[1, 2, 3], [4]].iter() {
		print(inner.len());                     // 3, then 1
	}
}
```

### Terminations

An adapter chain does nothing until it is *terminated*. These consume the
iterator and hand back an ordinary value:

```vilan,fragment
fun to_list(mut self): List<T>
fun fold<B>(mut self, init: B, fn: |B, T| B): B
fun for_each(mut self, fn: |T| void)
fun count(mut self): i32
fun any(mut self, predicate: |T| bool): bool     // short-circuits on the first hit
fun all(mut self, predicate: |T| bool): bool     // short-circuits on the first miss
fun rev(mut self): ListIterator<T>               // a BARRIER — see below
```

`to_list` is the primary one, and it is deliberately explicit. A method that
*names* what it builds needs no type annotation, reads at the call site, and
works in the middle of an expression — `xs.iter().filter(f).to_list().len()` —
which is exactly where an inference-driven `collect` gives up. There is no
`collect` in vilan, by design; if one is ever added it will sit beside this
family, never replace it.

```vilan
import std::print;

fun main() {
	let evens = [1, 2, 3, 4, 5, 6].iter().filter(|n| n % 2 == 0).to_list();
	print(evens.len());                                     // 3
	print([1, 2, 3].iter().fold(0, |total, n| total + n));   // 6
	print([1, 2, 3, 4].iter().filter(|n| n > 2).count());    // 2
	print([1, 2, 3].iter().any(|n| n == 2));                 // true
	print([1, 2, 3].iter().all(|n| n > 0));                  // true
}
```

`any` and `all` short-circuit, so they can answer over a source that has no end;
`count`, `fold`, `for_each` and `to_list` pull everything, so bound such a source
with `take` first.

`rev` is a **barrier**, not a lazy adapter: it drains its upstream into a `List`,
reverses that, and hands back a `ListIterator`. So the chain continues, but the
work up to that point has already happened — and `rev` never returns over an
unbounded source. (A lazy reverse needs a double-ended protocol, where every
adapter decides whether it can walk backwards. That is purely additive later:
`rev`'s signature would not change, only its body.)

For a `Set` or a `Map`, terminate with `to_list()` and convert:

```vilan,fragment
impl List<type T: Hashable>            { fun to_set(self): Set<T> }
impl List<(type K: Hashable, type V)>  { fun to_map(self): Map<K, V> }
```

```vilan
import std::print;
import std::map::Map;
import std::set::Set;
import std::option::Option::{ self, Some, None };

fun main() {
	let unique = [1, 2, 2, 3].iter().filter(|n| n > 1).to_list().to_set();
	print(unique.len());   // 2

	let lengths = ["alpha", "hi"].iter().map(|word| (word, word.len())).to_list().to_map();
	print(lengths.get("hi").unwrap_or(-1));   // 2
}
```

These two live on `List` rather than on `Iterator`, and the reason is worth
knowing because it shapes what you can write yourself: `to_set` needs
`T: Hashable`, `Iterator<T>` does not bound `T`, and a trait default may not
require a bound its trait does not declare — nor can a method carry one of its
own. So a `to_set` written as a trait default is rejected at its own definition,
before any call. Putting it beside the bound it needs is the same choice `join`
makes with `Display`. A repeated key in `to_map` keeps the **last** pair, matching
`insert`.
