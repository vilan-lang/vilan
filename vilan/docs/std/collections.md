# Collections reference

The container types: `List` (built in), `std::map::Map`, `std::set::Set`,
`std::range::Range`, and the `std::iterator` protocol underneath `for`.

## `List<T>`

Built in, with literal syntax: `[1, 2, 3]`. An empty literal needs a type
annotation (`let xs: List<str> = [];`).

```vilan,fragment
impl List<type T> {
	fun new(): List<T>
	fun push(&mut self, item: T)
	fun pop(&mut self): Option<T>
	fun insert(&mut self, index: i32, value: T)   // panics out of bounds
	fun remove(&mut self, index: i32): T          // panics out of bounds
	fun len(self): i32
	fun is_empty(self): bool
	fun get(self, index: i32): Option<T>
	fun first(self): Option<T>
	fun last(self): Option<T>
	fun map<U>(self, fn: |T| U): List<U>
	fun filter(self, predicate: |T| bool): List<T>
	fun find(self, predicate: |T| bool): Option<T>
	fun fold<B>(self, init: B, fn: |B, T| B): B
	fun for_each(self, fn: |T| void)
	fun reverse(self): List<T>
	fun sort_by(self, compare: |T, T| Ordering): List<T>   // stable
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
[memory model](../tour/memory-model.md)).

The methods that take `self` by value are pure — they return a new list and
leave the receiver alone. The mutating ones take `&mut self`: `push`, `pop`,
`insert`, `remove`.

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
}
```

Keys compare **by value**. Scalars work directly; a struct, enum, tuple, or
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
}
```

Value-keyed like `Map` (element `T` must be `Hashable`); `for x in set`
iterates the elements in insertion order.

## `Hashable`

A key's value is turned into a `Hash` (a canonical key) by `key.hash()`.
`[derive(Hashable)]` implements it for a struct/enum whose fields are all
`Hashable` (scalars, `str`, `bool`, `List`/`Option` of `Hashable`, or another
derived type); a closure, `Set`, `Map`, or `Shared` field is rejected. You can
also hand-write `impl Hashable` to key by a subset of fields, and build your own
container by bounding on `K: Hashable` and keying a `Map<Hash, …>` yourself.

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
trait Iterator<T> { fun next(self): Option<T>; }
trait Iterable<T> { fun iter(self): Iterator<T>; }
Iterator::from_fn(fn: || Option<T>): IteratorFromFn<T>   // an iterator from a closure
```

Anything implementing `Iterator`/`Iterable` works in a `for` loop.
`Range` is one such type.
