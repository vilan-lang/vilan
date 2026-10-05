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
| a float, an integer | `3.0`, `0.25`, `1e+21`; `7`, `-3` |
| a closure | `<closure |i32, i32| -> i32>` |
| a `HashMap`, a `HashSet` | `HashMap { "ada" => 36 }`, `HashSet { "a", "b" }`, in insertion order |
| a `Shared`, a `SignalCell` | `Shared(Point { x = 7, y = 8 })`, `SignalCell(3)` (read without tracking) |
| a pipe | `<pipe Derive<SignalCell<i32>, i32, i32>>`: sampling one would run it |
| a cycle | `Shared(Link { next = Some(<cycle>) })`: a cell met again is not entered |
| a trait object, a host handle | `<dyn Area>`, `<Task>` |

A value that fits in 80 columns from where it starts stays on one line;
otherwise each entry takes a line of its own, two spaces deeper, with a
trailing comma. A list stops after 100 entries with `… N more`.

`[build] dbg` in `vilan.toml` decides what a call does in a build: the
`debug` preset prints, the `release` preset refuses the build, `"strip"` makes
each call its argument, `"keep"` prints in release too.

## `Debug`

```vilan,fragment
trait Debug {
	fun debug(self): str
}
```

`.debug()` renders a value in the same syntax `dbg` prints, on one line. std
implements it for `str`, `bool`, every number (a float keeps its `.0`:
`3.0.debug()` is `"3.0"`), and for `List`, `Option` and `Result` whose
elements are `Debug`; `[derive(Debug)]` writes it for a struct or an enum from
its fields, so a struct holding a `List<i32>` or an `Option<f64>` derives it.
`dbg` needs none of this: it prints every type.

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
