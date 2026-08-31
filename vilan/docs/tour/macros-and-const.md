# Macros & const

Vilan has two tools that run at compile time. `const` computes *values*
ahead of time. Macros generate *code*. In both cases the emitted
JavaScript carries only the results, never the computation.

Most days you'll use these indirectly: `[derive(…)]` is a macro, and
`const style()` is how the styling system works. Writing your own comes
up rarely, so treat the second half of this chapter as reference.

## `const`: compute it at compile time

Put `const` in front of an expression and the compiler evaluates it
during the build, then writes the *result* into the output as a literal:

```vilan
fun squares(): List<i32> {
	mut result: List<i32> = [];
	for i in [1, 2, 3, 4] {
		result.push(i * i);
	}
	result
}

let TABLE = const squares();   // the emitted JS holds the literal list

fun main() {
	let folded = const 1 + 2 * 3;
	print(folded);
	print(TABLE.len());
}
```

Three rules to know:

- `const` captures **greedily**: everything to the end of the expression
  folds. Parenthesize to narrow it: in
  `(const square(4)) + square(2)`, the second call runs at runtime.
- The expression can only use things the compiler can know: literals,
  imports, and immutable bindings whose own initializers are const.
- No host calls. `const now()` is an error, because the answer wouldn't
  be a constant.

The flagship user is styling: `const style()…` chains evaluate at build
time and emit CSS. See the [styling guide](../guide/styling.md).

### Release builds fold without the keyword

A `release` build also folds ordinary initializers it can work out for
itself, under the same rules — no keyword needed:

```vilan
fun square(n: i32): i32 {
	n * n
}

fun main() {
	let side = 3 + 1;
	print(square(side));   // release ships `16`; debug ships the calls
}
```

`debug` deliberately does not, so the readable build keeps the
computation where a stack trace can show it. Set the preset in
`vilan.toml`:

```toml
[build]
preset = "release"      # or: infer-const = false, to opt out
```

The keyword is still worth writing, and the difference is what happens
when evaluation *fails*. `const` promises the fold and reports an error
if it can't deliver — the budget blew, a host call snuck in, the result
wasn't plain data. Inference promises nothing: anything it can't settle
it silently leaves alone, to run at runtime exactly as written. So a
release build never rejects a program a debug build accepts, and never
changes what a program does — it only moves some of the arithmetic
earlier. Write `const` when you want the guarantee (and the error when
it breaks); let inference pick up the rest.

## Derive macros: impls from a type's shape

You've already seen `[derive(PartialEq, Debug)]`. A derive is a macro: a
function that runs at compile time, receives the type it annotates as
*data*, and returns source code to splice into the program. You can
write your own:

```vilan
import std::display::{ Display, format };

macro fun derive_display(item: Item): Source {
	import macro_std::source;
	import macro_std::meta::{ Item, Source, StructItem };
	import macro_std::option::Option::{ self, Some, None };

	let target = match item.as_struct() {
		Some(let found) => found,
		None => StructItem { name = "?", fields = [] },
	};
	mut arms = "";
	mut first = true;
	for field in target.fields {
		if first {
			first = false;
		} else {
			arms = arms + " + \", \" + ";
		}
		arms = arms + i"\"{field.name}=\" + format(self.{field.name})";
	}
	source(i"""
		impl {target.name} with Display \{
			fun to_string(self): str \{
				import std::display::format;
				{arms}
			\}
		\}
		""")
}

[derive_display]
struct Point {
	x: i32,
	y: i32,
}

fun main() {
	print(format(Point { x = 1, y = 2 }));
}
```

How to read that:

- `macro fun` declares the macro. Its body is ordinary Vilan, but it
  compiles against `macro_std`, a small compile-time standard library
  with `source`, the `meta` types (`Item`, `StructItem`, …), and the
  basics. Its imports are its own; it can't reach into your program.
- The macro receives the annotated item as data. `item.as_struct()`
  gives the struct's name and fields.
- It returns `Source`: text, usually built with interpolation. The
  `i"""…"""` form is the one to reach for: the template is written as
  the generated code will read, indented with the macro around it (the
  closing delimiter's indentation is stripped from every line). Literal
  braces in generated code are escaped as `\{` and `\}`; the holes'
  braces are not.
- The returned source is spliced in *before* type checking, so generated
  code is checked like code you wrote by hand.

## `macro { … }` blocks

An anonymous macro that expands on the spot. In item position it stamps
out a family of items; in expression position it folds to a value:

```vilan
macro fun labeled(name: str, value: i32): str {
	i"fun {name}(): i32 \{ {value} \}\n"
}

macro {
	mut generated = "";
	mut index = 0;
	for index < 3 {
		generated = generated + labeled(i"constant_{index}", index * 10);
		index = index + 1;
	}
	source(generated)
}

fun main() {
	print(constant_0() + constant_1() + constant_2());
}
```

For plain value folding, prefer `const`. Reach for an
expression-position macro block only when you're generating *code*, not
computing a value.

## Choosing the right tool

| You need | Reach for |
|---|---|
| a computed constant or lookup table | `const` |
| CSS or other build-time assets | `const` calling std's emitters |
| an impl derived from a type's shape | a derive macro |
| a family of near-identical items | `macro { … }` in item position |
| transforming a whole item (like `[service]` does) | an attribute macro |

> **Going deeper.** Macro expansion is fueled: a runaway macro is a
> compile error rather than a hung build, and the limits are tunable via
> `[macro] fuel` / `depth` in `vilan.toml`. Macros see one item at a
> time; there is no whole-program reflection. The `[service]`,
> `[rpc]`, and `[derive(Wire)]` attributes you meet in the guides are
> this same mechanism, shipped in std.
