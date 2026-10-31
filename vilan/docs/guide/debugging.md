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

## Everything in scope: `dbg_stack`

`dbg_stack()` prints every binding in scope where you call it — a snapshot of
the stack frame, without naming anything:

```vilan
[resource]
struct Guard { id: i32 }

fun consume(own guard: Guard) {
	print(guard.id);
}

fun main() {
	let guard = Guard { id = 7 };
	consume(guard);
	let x = 1;
	mut rows = [Some(1), Some(2)];
	match &rows[0] {
		Some(let first) => {
			print(*first);
			rows.push(None);
			let x = "shadow";
			dbg_stack();
		},
		None => {},
	}
}
```

```text
[src/main.vl:18:4] dbg_stack() in main
  x: str = "shadow"
  x (shadowed at 17:8): i32 = 1
  first: view i32 = <view, invalidated by push at 16:4>  (a view into rows)
  guard: Guard = <moved at 10:10>
  rows: List<Option<i32>> = [Some(1), Some(2), None]
```

- **Which bindings.** The function's parameters and every local visible at the
  call, innermost scope first. A binding another one hides is listed under it,
  marked with where it was shadowed. Inside a closure: its own parameters and
  locals, then the bindings it captures, marked `(captured)`. Module-level
  bindings are not the stack and are not listed.
- **It reads nothing it must not.** A resource that was moved prints `<moved
  at L:C>` (or `<moved on some paths>`); a view that a later `push` invalidated
  prints `<view, invalidated by push at L:C>`; a pipe prints `<pipe, not
  sampled>` and a `lazy` parameter `<lazy, not forced>`, since looking would run
  them. A cell prints its current value without subscribing, so a `dbg_stack()`
  inside an effect does not make it re-run.
- **Values print as `dbg` prints them**, laid out from where they start, on the
  same stream, the same bytes on both backends. A view is marked `view` and
  says what it views.
- **Each listed binding counts as used at the call.** That can keep a value
  alive to the `dbg_stack()` line; on the native backend the worst case is a
  copy where a move used to be.
- **Release builds refuse it**, as they refuse `dbg`, and `[build] dbg` decides
  for both.

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

On node the line is the header of the uncaught `Error`, so a stack follows it,
and under `vilan run` that stack is in `.vl` files and lines too (see
[In a debugger](#in-a-debugger) below);
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
inside one is not tracking: its panic names its own line. A tracking function
cannot be passed as a value; wrap it in a closure.

On a trait method, the attribute belongs to the TRAIT: declared on the trait's
member, every implementation takes the call site (whether it writes the
attribute or not), and every call passes it — through a generic bound and a
`dyn` object too. An implementation cannot mark a member its trait does not,
since a call through the trait would have no site to pass.

```vilan
import std::debug::caller;

trait Check {
	[track_caller]
	fun check(self): str;
}

struct Always {}

impl Always with Check {
	fun check(self): str {
		caller().text()   // the line that called `check`, however it was reached
	}
}

fun through<T: Check>(value: T): str {
	value.check()   // this line
}

fun main() {
	print(Always {}.check());
	print(through(Always {}));
}
```

## In a debugger

A `debug` build (the default) writes a **source map** beside each script:
`main.mjs.map` next to `main.mjs`, `dist/server.mjs.map` next to
`dist/server.mjs`, named by a `//# sourceMappingURL=` comment on the script's
last line. A debugger reads it to show your `.vl` files where the JavaScript
would be, and node reads it to report a stack in them:

```text
panicked at main.vl:4:5: index out of bounds: the length is 2 but the index is 5
    at __at (file:///tmp/vilan-run-4242.mjs:3:8)
    at pick (/srv/app/main.vl:4:5)
    at <anonymous> (/srv/app/main.vl:9:11)
```

(`__at` is the runtime's own bounds check, which has no `.vl` line; `main`'s
body runs at the module's top level, hence `<anonymous>`.)

`vilan run` passes node `--enable-source-maps` whenever there is a map, which
is where that stack comes from. The map points at statements, calls, closures
and function heads, and its `names` carry each function's source name. It
carries your files' text; std's files are named by their path in the std
cache and marked to be skipped, so "step into" stays in your code.

`vilan run --inspect` starts the program under node's inspector, with the map:

```text
$ vilan run --inspect-brk src/main.vl
Debugger listening on ws://127.0.0.1:9229/…
```

Attach a debugger to it — `chrome://inspect` in Chrome, or VS Code's
"Attach to Node Process" — and set breakpoints in the `.vl` files.
`--inspect-brk` holds the program before its first line until the debugger
resumes it; both take an address (`--inspect=127.0.0.1:9230`). A plain
JavaScript debugger shows values as JavaScript holds them: a struct is its
field array, `[1, 2]`.

### In VS Code

The extension debugs a program itself: press F5 on a `.vl` file, click
**Debug** above a `fun main`, or add a `vilan` configuration to
`launch.json`:

```json
{
  "type": "vilan",
  "request": "launch",
  "name": "Debug vilan program",
  "program": "${file}"
}
```

Breakpoints, stepping and the call stack are in your `.vl` files, and the
Variables pane prints each binding the way `dbg` prints it — by its type,
not its JavaScript representation:

```text
point: Point = Point { x = 1, y = 2 }
shape: Shape = Shape::Rect(Point { x = 0, y = 0 }, Point { x = 5, y = 6 })
ages: HashMap<str, i32> = HashMap { "ada" => 36, "alan" => 41 }
count: SignalCell<i32> = SignalCell(3)
first: view i32 = 1
```

A type with its own `Debug` impl shows what the impl prints, as `dbg` does,
and a `dyn` value shows the value it holds by that value's own type. A struct
expands into its fields, a list into its elements, a map into its entries. A closure's frame lists what it captured from the function around it
apart from its own bindings. The Watch pane takes a path into a binding —
`segment.to`, `rows[0]`, `pair.1` — and prints it the same way; anything else
you type there is evaluated as JavaScript. A hover lays a long value out over
lines, as `dbg` does.

A stop in a function that has awaited lists the functions waiting on it below
an `await` label in the Call Stack: they are where the program comes back to,
not where it stands, so they show no variables.

`"watch": true` debugs a `vilan run --watch` session: each round's program is
held at its first line until the debugger has attached to it again, so a
breakpoint binds in every round. `"stopOnEntry": true` stops at `main`'s first
line; `"console": "internalConsole"` shows the program's output in the Debug
Console instead of a terminal (a terminal keeps stdin, for `scan()`). A
breakpoint's condition is a JavaScript expression.

The extension reads what the build writes beside the map: `main.mjs.vdbg.json`,
each function's bindings with their vilan names and types. A value it cannot
describe — a binding of a generic function whose instances share one body, so
its type differs between them — shows as JavaScript. `"adapter": "js-debug"`
debugs with VS Code's JavaScript debugger instead, every value JavaScript-shaped.

A `release` build writes no map (and no `.vdbg.json`). Ask for one with `[build] source-maps = true`,
or for a single build with `vilan build --source-maps`; `--inspect` writes one
whatever the preset says. `source-maps = false` turns the `debug` preset's off.
A map changes no byte of the program it describes: the script is the same with
it and without it, but for the comment that names it.

## Who woke this effect: the reactive trace

`trace(true)` (`std::reactive::trace::trace`) writes a line to stderr for every subscriber a
write wakes, naming the write and the cell:

```text
[reactive] effect #4 woken by cell #1 (made at src/main.vl:6:14), set at src/main.vl:12:2
```

In the VS Code debugger the lines are in the Debug Console too, and a stop in
an effect a turn ran later — at the end of a `batch`, or on a microtask — has
the write that woke it below a label naming the cause in the Call Stack. The
[`trace` reference](../std/reactive.md#trace--who-woke-what) has the details.
