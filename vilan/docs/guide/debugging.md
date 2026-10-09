# Debugging

A program that misbehaves usually answers two questions badly: *where* did it
go wrong, and *what* did it hold when it did. Vilan answers the first with
locations on every panic and the second with `dbg`.

## What: `dbg`

`dbg(..)` prints each value it is given, with the expression you wrote and
where you wrote it, in vilan's own literal syntax:

```vilan
struct Point { x: i32, y: i32 }
enum Shape { Circle(f64), Empty }

fun main() {
	let origin = Point { x = 0, y = 0 };
	dbg(origin, Shape::Circle(1.0), Some([1, 2]));
	let area = dbg(2.0 * 3.5) + 1.0;
	print(area);
}
```

```text
[src/main.vl:6:2] origin = Point { x = 0, y = 0 }
[src/main.vl:6:2] Shape::Circle(1.0) = Shape::Circle(1.0)
[src/main.vl:6:2] Some([1, 2]) = Some([1, 2])
[src/main.vl:7:13] 2.0 * 3.5 = 7.0
```

- **Any number of arguments, any types.** Each gets its own line. `dbg()`
  prints the bare location, which is a quick "did we get here".
- **It answers its argument** (a tuple of them for several, `()` for none), so
  it wraps an expression where it stands: `let area = dbg(2.0 * 3.5) + 1.0;`.
- **A statement reads in place.** `dbg(guard);` with a semicolon moves nothing,
  so a resource stays where it was. In expression position the value moves
  through, as it would into any function, and a list or struct comes back as a
  copy.
- **The format is vilan's.** A struct prints its fields, an enum variant its
  qualified name (`Some`, `None`, `Ok` and `Err` bare), a float keeps its `.0`
  (and negative zero shows as `-0.0`, where `print` writes `0`), a string is quoted and escaped, a closure prints its type
  (`<closure |i32| i32>`). std's handles print as themselves: `HashMap {
  "ada" => 36 }`, `HashSet { 1, 2 }`, `Shared(..)`, `SignalCell(3)` (read
  without subscribing), a `dyn` value as `dyn Area(Square { side = 2 })`, a
  pipe by its type (sampling it would run it), and a
  cycle through a `Shared` as `<cycle>`. A value that fits in 80 columns stays on one line;
  a longer one breaks one entry per line, two spaces deeper, with a trailing
  comma — except a list or set of numbers, strings or other scalars, which
  fills each line to the 80 columns. A list shows its first 100 entries and
  then `… N more`.
- **Your own `Debug` impl decides.** A type with a `Debug` impl you wrote
  prints through it, wherever the value sits; a `[derive(Debug)]` spells
  exactly what `dbg` prints (`Shape::Circle(1.0)`), so it changes nothing.
- **Generic code prints the real type.** In `fun show<T>(value: T)`, `dbg(value)`
  prints a `Point` as a `Point` and an `i32` as an `i32`.
- **Both backends print the same bytes**, to stderr (`console.log` in the
  browser, which has no stderr).
- **Release builds refuse it.** A `dbg` left in a `release` build is an error at
  the call, so a debugging line cannot ship by accident. `[build] dbg =
  "strip"` makes every call its argument and prints nothing; `dbg = "keep"`
  prints in release too.

A function of your own named `dbg` takes precedence, as it would over any
prelude name.

## Where: panics name their line

An uncaught panic prints the file, the line and the column that raised it, on
both backends and in release builds too:

```text
panicked at src/orders.vl:42:17: index out of bounds: the length is 2 but the index is 5
```

The file is relative to the package root. The site is the CALLER's for std's
own panicking helpers, so a failed `assert`, an `unwrap` on `None`, an `expect`
on `Err` and an index past the end all name the line you wrote, not a line
inside std:

```vilan
import std::io::assert;

fun check(n: i32) {
	assert(n > 0, "n must be positive");   // the report names this line
}

fun main() {
	check(3);
	let values = [1, 2];
	print(values.get(5).is_none());
}
```

On node the line is the header of the uncaught `Error`, so a stack follows it;
the native binary prints the line alone and exits with status 1. A panic that
is caught (`std::reactive::guarded`, a failing task) carries its message alone.

### `[track_caller]`

A helper of your own can report its caller's line the same way. Mark it
`[track_caller]`: it takes its call site as a hidden parameter, and a panic
inside it — or `std::debug::caller()` — names that site.

```vilan
import std::debug::caller;
import std::io::panic;

[track_caller]
fun positive(value: i32): i32 {
	if value <= 0 {
		panic("expected a positive value");   // reports the CALLER's line
	}
	value
}

[track_caller]
fun here(): str {
	caller().text()   // `src/main.vl:18:8`, the line that called `here`
}

fun main() {
	print(here());
	print(positive(3));
}
```

A chain of tracking functions reports the outermost caller. A closure written
inside one is not tracking: its panic names its own line. The attribute is
refused on a trait method (a call through a trait is dispatched, and no call
site knows to pass a location), and a tracking function cannot be passed as a
value; wrap it in a closure.
