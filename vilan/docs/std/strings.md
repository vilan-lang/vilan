# Strings reference

The string type `str` (built in, immutable), plus the text-facing traits
`Display`, `Debug`, and `Into`.

## str

Concatenate with `+`. Interpolate with `i"…{expr}…"` (see
[Values and types](../tour/values-and-types.md)).

```vilan,fragment
impl str {
	fun len(self): i32
	fun is_empty(self): bool
	fun trim(self): str
	fun to_uppercase(self): str
	fun to_lowercase_ascii(self): str
	fun contains(self, needle: str): bool
	fun starts_with(self, prefix: str): bool
	fun ends_with(self, suffix: str): bool
	fun replace(self, from: str, to: str): str       // all occurrences
	fun repeat(self, count: i32): str
	fun split(self, separator: str): List<str>
	fun substring(self, start: i32, end: i32): str   // end-exclusive; see below
	fun code_at(self, index: i32): u32               // UTF-16 code unit
	fun index_of(self, needle: str): Option<i32>     // declared in std::option
	fun last_index_of(self, needle: str): Option<i32> // likewise
	fun strip_prefix(self, prefix: str): Option<str> // likewise
	fun strip_suffix(self, suffix: str): Option<str> // likewise
	fun parse_i32(self): Option<i32>                 // likewise
	fun parse_f64(self): Option<f64>                 // likewise
}
```

### Locating: `index_of` and `last_index_of`

`contains` answers *whether*; these answer *where*. They are what
`substring` needs and nothing else computed — without them, text is taken
apart with `split` and put back together. The index is zero-based and
counts UTF-16 code units — `substring`'s own unit, so a bound one of these
returns feeds `substring` directly, non-BMP text included.

```vilan
import std::print;
import std::option::{ Some, None };

fun main() {
	let line = "key: value";
	match line.index_of(": ") {
		Some(let at) => {
			print(line.substring(0, at));                 // "key"
			print(line.substring(at + 2, line.len()));    // "value"
		}
		None => print("no separator"),
	}
	print("a.b.c".last_index_of(".").unwrap_or(-1));      // 3 — the final one
}
```

**Absence is `None`, never `-1`.** That is the whole reason these are not
the host's `indexOf` renamed: `-1` is an index nothing in the type system
tells apart from a real one, and it would be reported one call later, by
`substring`, about a number this call produced. The `Option` makes "not
found" a case you have to answer.

The empty needle sits at each end, as it does in the host: `s.index_of("")`
is `Some(0)` and `s.last_index_of("")` is `Some(s.len())`.

### Slicing: `substring` refuses, it does not correct

`substring(start, end)` requires

```text
0 <= start <= end <= len()
```

and **panics** on anything else — a negative bound, an inverted pair, or an
`end` past the length. It never clamps and never swaps.

That rule is worth stating plainly because the obvious host behavior is the
opposite one. JavaScript's `substring` clamps a negative argument to `0` and
*swaps* the pair when `start > end`, so `s.substring(offset, -1)` there returns
`s[0..offset]` — the **prefix**, the exact complement of the suffix the caller
was reaching for, with no error at any point. Vilan refuses instead, on the same
principle that makes an out-of-range `list[i]` a panic rather than a silent
`undefined`.

Two consequences worth knowing:

- **"To the end" is spelled `s.len()`**, not `-1` and not an over-long `end`.
- **The bound is checked, not guessed** — `substring(0, 100)` on a short string
  is an error, not a truncation.

Where both bounds are literals the refusal happens at **compile time**:

```text
error: substring end -1 is negative — the range must satisfy
       0 <= start <= end <= len, and substring never clamps or swaps
```

Empty ranges are legal, so the natural boundary cases all work:
`substring(0, 0)`, `substring(len(), len())`, and `substring(0, len())`.

### Cutting a known affix

Reaching for `substring` to drop a leading or trailing marker is what invited
the arithmetic in the first place. Prefer the verbs, which return `Option<str>`
so that "absent" is distinguishable from "present but empty":

```vilan
import std::print;
import std::option::{ Some, None };

fun main() {
	match "data: 42".strip_prefix("data: ") {
		Some(let body) => print(body),
		None => print("not a data line"),
	}
	match "report.md".strip_suffix(".md") {
		Some(let stem) => print(stem),
		None => print("not markdown"),
	}
	// `starts_with`/`ends_with` test; these two cut.
	print("ab".strip_prefix("ab").unwrap_or("?"));   // "" — present, empty
	print("ab".strip_prefix("zz").unwrap_or("?"));   // "?" — absent
}
```

```vilan
import std::print;

fun main() {
	let path = "/w/3/task/7";
	let parts = path.split("/").filter(|part| !part.is_empty());
	print(parts.len());
	print(parts[0].to_uppercase());
	print("task".repeat(2));
}
```

**On a path, cut with [`std::path`](paths.md), not with these.** `strip_prefix`
and `starts_with` compare text, and text is the wrong unit for a path:
`"/a/bc".starts_with("/a/b")` is `true` while `/a/bc` is not inside `/a/b`.
`path::starts_with` and `path::relative` compare components, and
`path::basename`/`path::extname` cut the affixes a filename actually has.

`str` also implements `PartialEq`/`Ord` (lexicographic `==`, `<`) and
`Default` (`""`).

## Display: user-facing text

```vilan,fragment
trait Display {
	fun to_string(self): str;
}
fun format<T: Display>(value: T): str
```

Implement `Display` for values that have a natural user-facing rendering;
`format(value)` (from `std::display`) is the generic entry point.
Interpolation accepts anything already `str`-shaped; call
`format`/`to_string` explicitly for custom types.

## Debug: developer-facing text

```vilan,fragment
trait Debug {
	fun debug(self): str;
}
```

`[derive(Debug)]` generates a structural rendering (`Point { x: 1, y: 2 }`
style) for structs and enums: the standard tool for logging and error
paths (`error.debug()` on an `RpcError`).

## Into: conversions

```vilan,fragment
trait Into<T> {
	fun into(self): T;
}
```

The generic conversion seam: implement `Into<Target>` on a source type,
bound helpers as `T: Into<Target>`. (Numeric width conversions don't use
this: they're the `as_*` methods on the numbers; see
[numbers](numbers.md).)
