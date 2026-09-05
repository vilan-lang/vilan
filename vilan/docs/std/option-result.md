# Option & Result reference

`Option<T>` is how Vilan says "maybe a value" (there is no `null`), and
`Result<T, E>` is how it says "this can fail" (there are no exceptions).
Both are plain enums with a large helper-method surface, listed here. For
how they replace `null` checks and `try`/`catch` in practice (including
the `!`, `?.` and `?` operators), read [Control flow](../tour/control-flow.md)
first.

```vilan,fragment
import std::option::Option::{ self, Some, None };
import std::result::Result::{ self, Ok, Err };
```

(The `{ self, … }` form imports the type *and* its variants, so `Some(x)`
works unqualified.)

## `Option<T>`

```vilan,fragment
enum Option<T> { Some(T), None }

impl Option<type T> {
	// predicates
	fun is_some(self): bool
	fun is_some_and(own self, fn: |T| bool): bool
	fun is_none(self): bool
	fun is_none_or(own self, fn: |T| bool): bool

	// extraction
	fun unwrap(own self): T                  // panics on None
	fun unwrap_or(own self, fallback: T): T
	fun unwrap_or_else(own self, fn: || T): T

	// in-place partial move — read/replace the slot through `&mut self`,
	// always leaving a valid Option behind
	fun take(&mut self): Option<T>                   // Some(v) -> None here, Some(v) out
	fun replace(&mut self, own value: T): Option<T>  // value in, old contents out

	// transformation
	fun map<U>(own self, fn: |T| U): Option<U>
	fun map_or<U>(own self, fn: |T| U, fallback: U): U
	fun map_or_else<U>(own self, fn: |T| U, fallback: || U): U
	fun map_or_default<U: Default>(own self, fn: |T| U): U
	fun inspect(own self, fn: |T| void): Self  // peek, pass through
	fun filter(own self, predicate: |T| bool): Option<T>

	// combination
	fun and<U>(own self, own b: Option<U>): Option<U>
	fun and_then<U>(own self, fn: |T| Option<U>): Option<U>
	fun or(own self, b: Option<T>): Option<T>
	fun or_else(own self, fn: || Option<T>): Option<T>
	fun xor(own self, own b: Option<T>): Option<T>
	fun zip<U>(own self, own peer: Option<U>): Option<(T, U)>

	// bridging
	fun ok_or<E>(own self, err: E): Result<T, E>
	fun ok_or_else<E>(own self, err: || E): Result<T, E>
}
impl Option<Option<type T>> { fun flatten(own self): Option<T> }
impl Option<Result<type T, type E>> { fun transpose(own self): Result<Option<T>, E> }
impl Option<type T: Default> { fun unwrap_or_default(own self): T }
impl Option<(type T, type U)> { fun unzip(own self): (Option<T>, Option<U>) }
```

`str.parse_i32(): Option<i32>` and `str.parse_f64(): Option<f64>` (both
declared here) are the string→number path, and `bool.then_some(value)` is the
condition→`Option` one.

The combinators that hand the payload onward take **`own self`**: they move
the value out of the `Option`, so they must own it (`docs/spec/memory.md` R3).
For plain data that is invisible — `own` copies, and `opt.unwrap()` leaves
`opt` perfectly readable. For an `Option<SomeResource>` it is the affine rule
biting: `opt.unwrap()` *moves* `opt`, a later use of `opt` is a
use-after-move error, and `opt` is not torn down at all (the payload
you now hold is). The pure predicates — `is_some`, `is_none` — keep a
borrowing `self` and never consume, so they stay free on a resource.

Reaching a resource payload is a `match`, not a guarded `unwrap`: `match opt
{ Some(let value) => .., None => .. }` consumes `opt` on *every* path, which
is what R7 requires. `if (opt.is_some()) { opt.unwrap() }` moves `opt` on one
path only and is rejected as a conditional move (before this was checked it
compiled and destroyed the payload twice).

The capture *owns* the payload it took, so it is torn down at its last use
inside the leg unless you move it on — into `drop`, into a return, into an `own`
argument, into a struct field. Either way the payload is destroyed exactly
once. A capture taken from a loan (`match &opt`, or `opt is Some(let value)`)
consumes nothing and owns nothing: `opt` stays the owner and tears the
payload down itself.

Three combinators are **rejected** at an `Option<SomeResource>`, and the rule
is one sentence: each has a path that *discards* a resource value it was
handed, and a generic body cannot destroy a `T`. `opt.or(other)` throws
`other` away when `opt` is `Some`; `opt.xor(other)` throws both away when both
are `Some`; `opt.unwrap_or(fallback)` throws the fallback away when `opt` is
`Some`. Each names the discarded value in its diagnostic. The spellings that
*produce* the alternative instead of taking one in — `or_else`,
`unwrap_or_else` — work, because nothing is ever discarded. Everything else on
this surface is fine at a resource, including `inspect` and `==`, which loan
rather than consume.

`take` and `replace` mutate the `Option` in place through `&mut self`: `take`
swaps `None` in and hands the old contents back, `replace` swaps a new value in
and hands the old back. Both leave a valid `Option` behind, which is what makes
them the sanctioned way to move a value *out* of a place. For a `resource` this
is the only legal partial move (`self.slot.take()`), and `match opt.take() {
Some(let c) => drop(c), None => {} }` is the conditional-teardown idiom.

## `Result<T, E>`

```vilan,fragment
enum Result<T, E> { Ok(T), Err(E) }

impl Result<type T, type E> {
	// predicates
	fun is_ok(self): bool
	fun is_ok_and(self, fn: |T| bool): bool
	fun is_err(self): bool
	fun is_err_and(self, fn: |E| bool): bool

	// extraction
	fun unwrap(self): T                      // panics on Err
	fun unwrap_err(self): E                  // panics on Ok
	fun unwrap_or(self, fallback: T): T
	fun unwrap_or_else(self, fn: |E| T): T
	fun expect(self, message: str): T        // panic with your message
	fun expect_err(self, message: str): E

	// transformation
	fun map<U>(self, fn: |T| U): Result<U, E>
	fun map_err<F>(self, fn: |E| F): Result<T, F>
	fun map_or<U>(self, fn: |T| U, fallback: U): U
	fun map_or_else<U>(self, fn: |T| U, fallback: |E| U): U
	fun inspect(self, fn: |T| void): Self
	fun inspect_err(self, fn: |E| void): Self

	// combination
	fun and<U>(self, b: Result<U, E>): Result<U, E>
	fun and_then<U>(self, fn: |T| Result<U, E>): Result<U, E>
	fun or<F>(self, b: Result<T, F>): Result<T, F>
	fun or_else<F>(self, fn: |E| Result<T, F>): Result<T, F>

	// bridging
	fun ok(self): Option<T>
	fun err(self): Option<E>
}
impl Result<type T: Default, type E> { fun unwrap_or_default(self): T }
impl Result<Option<type T>, type E> { fun transpose(self): Option<Result<T, E>> }
```

## Idioms

```vilan
import std::option::Option::{ self, Some, None };
import std::result::Result::{ self, Ok, Err };

fun parse_port(text: str): Result<i32, str> {
	text.parse_i32()
		.ok_or(i"not a number: {text}")
		.and_then(|port| {
			if port > 0 && port < 65536 {
				Ok(port)
			} else {
				Err(i"out of range: {port}")
			}
		})
}

fun main() {
	print(parse_port("8080").unwrap_or(0));
	match parse_port("http") {
		Ok(let port) => print(port),
		Err(let reason) => print(reason),
	}
}
```

- Prefer `!` (propagate) and `unwrap_or*` over `unwrap`: `unwrap` is for
  invariants, and it panics.
- Application errors belong in `Result`'s `E`; only unreachable
  states panic.
- `match` with `Some(let x)` / `Ok(let x)` patterns is always available
  when the method chain gets clever. Clarity beats cleverness.
