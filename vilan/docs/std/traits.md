# Core traits reference

The traits behind the operators and the derive set: `std::compare`,
`std::default`, `std::operators`.

## std::compare

```vilan,fragment
trait PartialEq<B = Self> {
	fun eq(self, b: B): bool;      // ==
	fun ne(self, b: B): bool;      // != (default: !eq)
}
trait Eq with PartialEq {}

enum Ordering { Less, Equal, Greater }

trait PartialOrd<B = Self> with PartialEq<B> {
	fun partial_compare(self, b: B): Option<Ordering>;
	fun lt(self, b: B): bool;      // <   (defaults over partial_compare)
	fun le(self, b: B): bool;      // <=
	fun gt(self, b: B): bool;      // >
	fun ge(self, b: B): bool;      // >=
}

trait Ord with Eq + PartialOrd {
	fun compare(self, b: Self): Ordering;
	fun min(self, b: Self): Self;
	fun max(self, b: Self): Self;
	fun clamp(self, min: Self, max: Self): Self;
}
```

- `==`/`!=` dispatch through `PartialEq`; `<`/`<=`/`>`/`>=` through
  `PartialOrd`. Numbers, `str`, and `bool` implement them in std.
- For your own types, `[derive(PartialEq)]` gives structural equality,
  the usual path. Implement `PartialOrd`/`Ord` by hand when ordering is
  meaningful (`Instant` does this in std).
- The `B = Self` parameter allows cross-type comparison impls; you'll
  rarely need it.

## std::default

```vilan,fragment
trait Default {
	fun default(): Self;
}
```

Zero for numbers, `""` for `str`, `false` for `bool`.
`[derive(Default)]` composes fields' defaults. Used as a bound by helpers
like `unwrap_or_default` and `List.sum`.

## std::operators: the operator traits

Each operator dispatches through a trait; implement the trait, get the
operator:

| Trait | Operator | | Trait | Operator |
|---|---|---|---|---|
| `Add<B = Self>` | `+` | | `Shl` | `<<` |
| `Sub` | `-` | | `Shr` | `>>` |
| `Mul` | `*` | | `BitAnd` | `&` |
| `Div` | `/` | | `BitOr` | `\|` |
| `Rem` | `%` | | `BitXor` | `^` |

The `B = Self` parameter types the right-hand side; mixed-operand impls
are how std's `Instant + Duration` works:

```vilan
import std::print;
import std::operators::Add;

struct Celsius {
	degrees: f64,
}

impl Celsius with Add {
	fun add(self, b: Celsius): Celsius {
		Celsius { degrees = self.degrees + b.degrees }
	}
}

fun main() {
	let morning = Celsius { degrees = 20.5 };
	let rise = Celsius { degrees = 1.5 };
	print((morning + rise).degrees);
}
```

Compound assignment (`+=`, `/=`, …) rides the same impls. The
operator's result type is `Self` (the left operand's type).

## std::operators: `Try` and `Lift`

The machinery behind `!` and `?.`
([control flow](../tour/control-flow.md)):

```vilan,fragment
enum Verdict<T, B> { Good(T), Bad(B) }

trait Try<T, B> {
	fun verdict(self): Verdict<T, B>;   // split into good/bad
	fun from_bad(bad: B): Self;         // rebuild from the bad half (for propagation)
}
trait Lift {}                           // opt-in marker for ? and ?.
```

`Option` and `Result` implement both in std. A custom two-outcome type that
implements `Try` gets `!`; adding the `Lift` marker gets both lift forms —
the `?.` chain and the bare `?` that lifts a whole expression.

`Lift` declares no members: it is consent, not a contract you fill in. What
the operators actually call is a pair of ordinary methods your container
supplies — `map<U>(self, |T| U)` for the plain case, and
`and_then<U>(self, |T| Self-of-U)` for the flattening one. The element is the
container's **first type argument**.

```vilan
import std::print;
import std::operators::Lift;

struct Tagged<T> {
	value: T,
	tag: str,
}

impl Tagged<type T> with Lift {}

impl Tagged<type T> {
	fun map<U>(self, fn: |T| U): Tagged<U> {
		Tagged { value = fn(self.value), tag = self.tag }
	}

	fun and_then<U>(self, fn: |T| Tagged<U>): Tagged<U> {
		let inner = fn(self.value);
		Tagged { value = inner.value, tag = self.tag + "+" + inner.tag }
	}
}

fun main() {
	let price = Tagged { value = 40, tag = "eur" };
	let tax = Tagged { value = 2, tag = "eur" };

	let doubled = price? * 2;        // map — Tagged<i32>, value 80
	let total = price? + tax?;       // and_then, then map — value 42
	print(total.tag);                // eur+eur
}
```

A region with several `?`s nests the calls left to right — `price.and_then(|p|
tax.map(|t| p + t))` above — so short-circuiting and laziness are whatever
your `and_then` does with the closure it is handed. Every receiver in one
expression must be the same container; a type with a `map` but no `Lift` is
refused, and a missing `map`/`and_then` is named in the error.
