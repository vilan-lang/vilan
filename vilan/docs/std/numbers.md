# Numbers reference

The sized numeric family (`std::number`), generic `min`/`max` (`std::math`),
and random values (`std::random`). Literal syntax and conversion semantics:
[Values and types](../tour/values-and-types.md).

## The family

| Type | Width | Literal |
|---|---|---|
| `i8 i16 i32 i53` | signed | bare = `i32`; others suffixed (`100i53`) |
| `u8 u16 u32 u53` | unsigned | suffixed (`0xFFu8`) |
| `f64` | float | `2.5` or `10f` |
| `f32` | float | `2.5f32` |
| `BigInt` | arbitrary | `7n` |

`i53`/`u53` are the **wide** integers, named for the precision they
actually deliver: they are f64-backed on the JS backend, and every value
in ±2^53 (f64's exact-integer window, one past JavaScript's
`Number.MAX_SAFE_INTEGER`) is exact. There is no `i64`:
a type that silently loses precision past 2^53 would be lying about its
width; for bigger integers use `BigInt`.

Literals are range-checked at compile time (an out-of-range `i53` literal
is a compile error, not a rounded value). Integer division truncates
toward zero. No implicit width coercion; convert with `as_*`. Arithmetic
that overflows a type's range is **undefined behavior**
(spec [§7.2a](../spec/execution.md)): on JS it manifests as f64
artifacts; a checked `add_safe` family is recorded future work.

Because overflow is undefined, the boundary values have to be *askable*.
Every integer type carries its two bounds as niladic functions:

```vilan
fun main() {
	print(i32::max_value());   // 2147483647
	print(i32::min_value());   // -2147483648
	print(u8::max_value());    // 255
	print(i53::min_value());   // -9007199254740992
}
```

| type | `min_value()` | `max_value()` |
|---|---|---|
| `i8` | `-128` | `127` |
| `u8` | `0` | `255` |
| `i16` | `-32768` | `32767` |
| `u16` | `0` | `65535` |
| `i32` | `-2147483648` | `2147483647` |
| `u32` | `0` | `4294967295` |
| `i53` | `-9007199254740992` | `9007199254740992` |
| `u53` | `0` | `9007199254740992` |

**This spelling is a stopgap.** vilan has no associated constants — there is
no static-member mechanism for `i32::MAX` to hang on — so the bounds ship as
functions rather than wait for that design. When it lands they become
`i32::MAX`/`i32::MIN` and this pair enters a `[deprecated("steer")]` window
that rewrites callers. The rename is scheduled, not a surprise: reach for
`max_value()`/`min_value()` freely today.

The pair reports the **type's** range, which is deliberately not the range of
literals the compiler admits: `128i8` compiles, because the signed literal
check tests the magnitude so that `-128i8` can be written at all, yet
`i8::max_value()` is `127`. Trust the functions over the looseness.

Floats have no pair, for two reasons that are worth stating rather than
guessing at. `f64`'s finite bounds cannot be written as vilan literals at all
— there is no exponent syntax, so `1.7976931348623157e308` is a parse error —
and `min_value()` would have to silently pick between the most-negative finite
(Rust's `f64::MIN`) and the smallest positive normal (C's `DBL_MIN`). That is
a choice the eventual `f64::MIN` should make deliberately, not one this
stopgap should prejudge. `BigInt` has no bounds by construction.

## Methods

Integers (per type; shown for `i32`):

```vilan,fragment
impl i32 {
	fun abs(self): i32
	fun pow(self, exponent: i32): i32
	fun min(self, other: i32): i32
	fun max(self, other: i32): i32
	fun rem(self, m: i32): i32     // the % operator's method
	fun diff(self, other: i32): i32
	fun is_even(self): bool
	fun is_odd(self): bool
}
```

Floats add the usual math surface:

```vilan,fragment
impl f64 {
	fun abs(self): f64
	fun sqrt(self): f64
	fun pow(self, exponent: f64): f64
	fun floor(self): f64
	fun ceil(self): f64
	fun round(self): f64
	fun min(self, other: f64): f64
	fun max(self, other: f64): f64
	fun clamp(self, min: f64, max: f64): f64
	fun trunc(self): f64
	fun fract(self): f64
	fun sign(self): f64
	fun lerp(self, to: f64, t: f64): f64
	fun sin(self): f64      // cos, tan, asin, acos, atan, atan2, hypot
	fun exp(self): f64      // ln, log2, log10, cbrt
	fun to_radians(self): f64  // to_degrees

	// the three a reader actually comes looking for
	fun is_nan(self): bool
	fun is_finite(self): bool
	fun is_infinite(self): bool
}
```

Every numeric type implements `Default` (zero), the operator traits, and
comparison.

`clamp` confines a value to a range. The integers inherit it from `Ord`; the
floats are deliberately *not* `Ord` (NaN has no place in a total order), so
`f64` and `f32` carry their own — same recipe, same result.

```vilan
fun main() {
	print(9.clamp(0, 5));       // 5   — i32, through Ord
	print(9f.clamp(0f, 5f));    // 5
	print((0f - 1f).clamp(0f, 5f));   // 0
}
```

## Conversions: `as_*`

Every numeric type converts to every other with Rust-`as` semantics.
Floats truncate toward zero; integers fold two's-complement into the
target width:

```vilan
fun main() {
	print((3.9).as_i32());    // 3
	print((-1).as_u8());      // 255 — folded
	print((300).as_u8());     // 44
	let wide = 9007199254740992i53;
	print(wide.as_i32());
	print((255u8).as_f64() / 2.0);
}
```

Conversions on literals fold at compile time.

## std::math

```vilan,fragment
fun min<T: Ord>(a: T, b: T): T
fun max<T: Ord>(a: T, b: T): T
fun minmax<T: Ord>(a: T, b: T): (T, T)   // (smaller, larger)

struct Vec2 { x: f64, y: f64 }           // a 2-D vector: point, offset, velocity, size
impl Vec2 {
	fun scale(self, factor: f64): Vec2   // both components times a scalar
	fun length(self): f64                // sqrt(x² + y²)
	fun length_squared(self): f64        // x² + y², for comparing without the root
	fun distance(self, other: Vec2): f64 // the length of the vector between two points
	fun dot(self, other: Vec2): f64      // x*x + y*y; v.dot(v) is v's length squared
}
// `Add`, `Sub` (component-wise, so `a + b` and `a - b`) and `PartialEq`.
```

`Vec2` is a value, not a handle: it copies, compares and parks in a signal.
The components carry **no unit** — CSS pixels at a UI call site, metres at a
physics one — which is why there is no `Length` type in the signature.

```vilan
import std::math::Vec2;

fun main() {
	let start = Vec2 { x = 10.0, y = 10.0 };
	let now = Vec2 { x = 13.0, y = 14.0 };
	let travelled = now - start;
	print(travelled.length());              // 5
	print(now.distance(start) > 3.0);       // a drag threshold
}
```

A threshold does not need the square root: `travelled.length_squared()` is
`x² + y²`, and `sqrt` is monotonic, so compare it against the *squared*
threshold on a hot path — a pointer-move handler squares its threshold once
instead of taking a root per event. Take the root when the number itself is
the answer.
`==` is exact `f64` equality, with everything that implies — a vector arrived
at by arithmetic is rarely `==` one written down, so compare a `distance`
against a tolerance where that matters.

## std::random

```vilan,fragment
fun range<T: Random>(low: T, high: T): T   // uniform in [low, high)
// implemented for i32, u32, f64
```

```vilan
import std::random;

fun main() {
	let roll = random::range(1, 7);   // 1..=6
	print(roll >= 1 && roll <= 6);
}
```

Not cryptographic: for tokens and ids use `std::crypto`
(`random_uuid`, `random_bytes`; see [misc](misc.md)).
