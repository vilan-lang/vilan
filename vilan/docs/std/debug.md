# Debugging (`std::debug`)

The reference for the guide's [Debugging](../guide/debugging.md) chapter.

## `dbg`

```vilan,fragment
fun dbg(..)    // any number of arguments, any types; in the prelude
```

`dbg(a, b, ..)` writes `[file:line:column] <expression> = <value>` per argument
to stderr (`console.log` in the browser) and answers the argument, a tuple of
them for several, or `()` for none. The compiler types and lowers every call:
the declaration exists to be named, imported and documented. A statement
(`dbg(x);`) reads its arguments in place; an expression moves them through.

The value is written in vilan's literal syntax by a printer the compiler
generates for each type a `dbg` reaches, the same on both backends:

| value | prints |
|---|---|
| a struct | `Point { x = 1, y = 2 }`, a field-less one by its name |
| an enum variant | `Shape::Circle(1.5)`, `Shape::Empty`; `Some(5)`, `None`, `Ok(1)`, `Err("no")` |
| a tuple, a list | `(1, "two", 3.0)`, `[1, 2, 3]` |
| a string | `"a \"quoted\" line\n"` |
| a float, an integer | `3.0`, `0.25`, `1e+21`, `-0.0`; `7`, `-3` |
| a closure | `<closure |i32, i32| i32>` |
| a `HashMap`, a `HashSet` | `HashMap { "ada" => 36 }`, `HashSet { "a", "b" }`, in insertion order |
| a `Shared`, a `SignalCell` | `Shared(Point { x = 7, y = 8 })`, `SignalCell(3)` (read without tracking) |
| a pipe | `<pipe Derive<SignalCell<i32>, i32, i32>>`: sampling one would run it |
| a cycle | `Shared(Link { next = Some(<cycle>) })`: a cell met again is not entered |
| a trait object | `dyn Area(Square { side = 2 })`: the value it holds |
| a host handle | `<Task>` |

A value that fits in 80 columns from where it starts stays on one line;
otherwise each entry takes a line of its own, two spaces deeper, with a
trailing comma. A list or a set of scalars (numbers, strings, bools, an enum
whose variants carry nothing) fills each of those lines instead, as many
entries as fit the 80 columns. A list stops after 100 entries with `… N more`.

`[build] dbg` in `vilan.toml` decides what a call does in a build: the
`debug` preset prints, the `release` preset refuses the build, `"strip"` makes
each call its argument, `"keep"` prints in release too.

## `dbg_stack`

```vilan,fragment
fun dbg_stack()    // no arguments; in the prelude
```

`dbg_stack()` writes a `[file:line:column] dbg_stack() in <function>` line and
then one `  name: Type = value` line per binding in scope at the call, to the
stream `dbg` writes to. The compiler expands each call from the scope it sits
in:

- the parameters and locals visible at the call, innermost scope first and in
  declaration order within a scope; a shadowed binding follows the one that
  hides it as `x (shadowed at L:C): T`; inside a closure, its own bindings and
  then the ones it captures, `(captured)`; no module-level binding;
- a view is typed `view T`, with `(a view into rows)` naming what it views;
- a moved resource prints `<moved at L:C>`, or `<moved on some paths>`; a view
  invalidated since its last use prints `<view, invalidated by push at L:C>`
  (or `by assignment`); a pipe `<pipe, not sampled>`; a `lazy` parameter
  `<lazy, not forced>` — none of them is read;
- a `SignalCell` prints its current value without subscribing, `(read without
  tracking)`;
- every other value prints as `dbg` prints it, laid out from where it starts,
  its broken entries two spaces under the binding.

Every binding it reads counts as a use at the call. `[build] dbg` applies to it
as it does to `dbg`.

## `Debug`

```vilan,fragment
trait Debug {
	fun debug(self): str
}
```

`.debug()` renders a value in the same syntax `dbg` prints, on one line. std
implements it for `str` (quoted and escaped as `dbg` writes it), `bool`, every
number (a float keeps its `.0`: `3.0.debug()` is `"3.0"`, and negative zero is
`"-0.0"`), and for `List`, `Option`, `Result` and tuples whose elements are
`Debug` (`(1, "two").debug()` is `(1, "two")`);
`[derive(Debug)]` writes it for a struct or an enum from its fields
(`Point { x = 1, y = 2 }`, `Shape::Circle(1.5)`), so a struct holding a
`List<i32>` or an `Option<f64>` derives it. `.debug()` is opt-in: a type has
it only through the derive or an impl of its own.

`dbg` needs none of this: it prints every type. A `Debug` impl you WRITE decides
how `dbg` prints that type, wherever the value sits (a field, a list element,
a generic `T`), and a written generic impl applies only where its bounds hold.
A derived impl spells what `dbg` already prints, so `dbg` keeps laying a
derived value out over lines past 80 columns:

```vilan
import std::debug::Debug;

struct Celsius { degrees: f64 }

impl Celsius with Debug {
	fun debug(self): str {
		self.degrees.debug() + "°C"
	}
}

fun main() {
	dbg(Celsius { degrees = 21.5 });   // [src/main.vl:12:2] Celsius { degrees = 21.5 } = 21.5°C
}
```

## Locations

```vilan,fragment
external struct Location;

impl Location {
	fun text(self): str      // "src/main.vl:12:5"
	fun file(self): str      // relative to the package root; std's own as "std/src/.."
	fun line(self): i32      // 1-based
	fun column(self): i32    // 1-based, in characters
}

impl Location with Display  // `to_string()` is `text()`

[track_caller]
fun caller(): Location      // the caller's site inside a tracking function, else this call's
```

A `Location` is a call site. `[track_caller]` functions receive their caller's
as a hidden parameter, which `caller()` reads; anywhere else `caller()` answers
the site of its own call. std's `panic`, `assert`, `Option::unwrap`/`expect`,
`Result::unwrap`/`unwrap_err`/`expect`/`expect_err` and `List::remove`/`insert`
are tracking, and every subscript `xs[i]` reports its own site, so a panic
names the line that reached it: `panicked at src/main.vl:12:5: <message>`.
