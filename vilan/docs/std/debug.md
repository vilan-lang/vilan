# Debugging (`std::debug`)

The reference for the guide's [Debugging](../guide/debugging.md) chapter.

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
