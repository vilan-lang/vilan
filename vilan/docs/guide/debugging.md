# Debugging

A program that misbehaves usually answers two questions badly: *where* did it
go wrong, and *what* did it hold when it did. Vilan answers the first with
locations on every panic and the second with `dbg`.

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
