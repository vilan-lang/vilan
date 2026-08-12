# The CLI

The whole toolchain is one binary. `vilan <command> --help` prints each
command's flags; this page adds the behavior the one-line help can't
carry. One rule up front: **`vilan upgrade` is the only command that
touches the network.** Everything else (builds, dependency resolution,
tests) works offline (git dependencies are fetched once by the first
build that needs them, then served from the cache forever).

For the guided on-ramp, see [Hello Vilan](../tour/hello-vilan.md); for
`--watch`, HMR, and the manifest keys that shape the dev loop, see
[the dev loop](../guide/dev-loop.md).

## `vilan init [name]`

Scaffolds a ready-to-run project: a manifest, sources that compile, and
a `.gitignore`. With `name`, creates that directory (it must not exist
or must be empty); without, scaffolds into the current directory (which
must not already hold a `vilan.toml`). It never overwrites a file, and
it creates no git repository; `git init` stays yours.

`--template <name>` picks the shape; omitted, an interactive prompt asks
(without a terminal that is a clean error, never a hang):

| Template | What you get |
|---|---|
| `node` | a package that runs on Node, with a sibling module and a test |
| `browser` | a reactive browser app (a counter, an `index.html`) |
| `fullstack` | one package, two entries: a browser client and a Node server |

The templates are compiled and run by Vilan's own test suite, so a
scaffold that stops working fails Vilan's build before it reaches yours.

## `vilan build [file]`

Compiles to JavaScript. The path may be a `.vl` file, a project
directory, or omitted; then the nearest `vilan.toml` decides what to
build. A single-entry package writes the bundle beside the entry; a
multi-entry package writes one into `dist/` per entry. The extension is
the target's: `.mjs` on a process runtime (Node/Deno/Bun), so the host
classifies the emitted ESM without inspecting it, and `.js` on the
browser, whose `<script type="module">` already declares it. Assets emitted at
compile time (the styling system's CSS) land beside the output.
`[build] run` hooks execute first; a failing hook fails the build.

- `--stdout`: print the JavaScript instead of writing a file.
- `--platform <p>`: `node`, `deno`, `bun`, `browser`, or `none`;
  overrides the package's `target` (`--target` is an accepted alias).
  `none` checks against no platform's layers, the strictest reading.
- `--watch`: rebuild whenever a watched source changes (Ctrl-C stops).
- `-d, --debug`: also emit `.parse.out` / `.analyze.out` /
  `.callgraph.out` dumps, for poking at the compiler's view of your code.
- `--print-chunks`: report the route-chunk plan — what a `split = true`
  browser leg would load lazily per route, with function counts and a byte
  estimate, plus a `verdict:` line measuring what splitting would actually
  cost the first load (the entry is emitted both ways and compared).
  Analysis only; the emitted JavaScript is unchanged, so this is how to
  measure a leg before opting it in
  ([the dev loop](../guide/dev-loop.md#shipping-routes-separately)).
- `--backend js`: the only backend today; the flag exists so a future
  one has somewhere to live.

Every build of a `browser` entry writes `<name>.chunks.json`, the leg's
build manifest — what it emitted, for `std::build::build_of` to read. A
`browser` entry with `[entry.<name>] split = true` additionally writes one
file per route arm, lists them in that manifest, and warns when the split
cost the first load more than it deferred. The leg's chunk files belong to
its last build: a build that writes none removes any a previous one left,
and rewrites the manifest to say the leg emitted none. `vilan run` ignores
`split` — the dev loop swaps whole bundles — and emits the leg as one file.

## `vilan check [file]`

`build` without the output: type-checks and reports diagnostics, writes
nothing, and runs no `[build] run` hooks. Same path forms and flags
(`--platform`, `--watch`, `-d`). In a multi-entry package it checks
every entry, each under its own platform. Exit is non-zero when
diagnostics were reported.

## `vilan run [file] [args…]`

Builds and runs. Anything after the file is forwarded to the program.
Reach it with `process::args()`. Under `--watch` it rebuilds and
restarts on every save; in a project with a browser leg, hot module
replacement is on by default: the page swaps changed code in place
instead of reloading (see [the dev loop](../guide/dev-loop.md)).

- `--watch`: place it before the file, ahead of any program args.
- `--no-hmr`: plain restart-the-server watching, no dev channel.
- `--hmr-port <port>`: the `127.0.0.1` port for the HMR channel
  (`0` means an OS-assigned one).
- `--entry <name>`: in a package (or workspace) with more than one
  runnable entry, which one to launch. The others still compile; they
  don't run. The manifest's `default-entry` makes the flag
  unnecessary; with neither, the error names both ways to choose.

## `vilan fmt [paths…]`

Formats source files in place; directories are walked, and the default
is the current directory. Formatting is conservative and a fixed point:

- A statement over 100 columns whose expression is a method chain
  splits (subject on the statement's line, one `.link(…)` per line
  below it), and the rule applies per line, recursively: a nested chain,
  list literal or struct literal that still overflows splits one level
  further in. A chain that fits stays on (or collapses back to) one line.
- A list literal, a struct literal or an import's brace set over the
  budget breaks one entry per line, with a trailing comma after every one
  — the last included, so adding an entry is a one-line diff. One that
  fits stays inline *without* a trailing comma, so the comma marks a
  split and nothing else.
- Width is measured on a line, not on a statement: a construct that opens
  a line and continues below it — a block-bodied closure, a `match`, a
  block — is judged by the line it opens, and its body lines are measured
  where they are printed. So `view(…)…when(cond, || { … })` splits its
  chain like any other; only what shares the opening line counts toward
  that line's width.
- A chain also splits *regardless of width* when a link that is not its
  last spans lines — when a `})` would be followed by more chain on the
  same line. A chain that ends at its spanning link is left alone, so the
  trailing-closure shape `self.cleanups.write().push(|| { … });` stays as
  written.
- A list or struct literal also splits *regardless of width* when one of
  its elements spans lines, because its closing `}` or `]` — and usually a
  `)` and `;` after it — would otherwise pile onto that element's last
  line. Unlike a chain, the *last* element counts: a composite has no
  position where a spanning element leaves a clean line.
- A comment you write *inside* one of these constructs keeps it split, and
  attaches to the element it precedes — the link, element, field, imported
  name or parameter below it. A construct that collapsed would have no line
  to keep the comment on, which is why the comment decides the layout. A
  comment inside an element (a closure body a link carries, say) is that
  body's own and changes nothing.
- A `fun` signature over the budget breaks its parameter list the same
  way, one parameter per line, with the return type, a `borrows` clause
  and the body's `{` (or a bodyless `;`) riding the closing `)`. An empty
  parameter list never breaks, so a signature pushed over by its *name*
  stays long. A closure's parameters are never broken.
- Parenthesized groups you wrote are kept, even where the grammar
  doesn't need them: a redundant paren is usually there for clarity.
- A call's *argument* list is never wrapped, but the split reaches the
  **last** argument, so a statement whose only breakable construct sits
  there still breaks it — `list.push(T { … })` splits the literal. A long
  *earlier* argument still leaves a long line: layout hangs off the final
  argument. This is deliberately not symmetric with the parameter rule
  above — an argument list sits inside an expression, where the builder
  convention decides layout, while a parameter list is a declaration's own
  contract and has no shape but one-per-line.
- A file the formatter cannot yet print faithfully is left byte-for-byte
  untouched, never half-formatted.

`--check` reports the files that would change and exits 1 if any (the
CI spelling). Nothing is rewritten.

## `vilan test [path]`

Runs `*_test.vl` files: the given file, a directory of tests, or every
test in the project. A test file lives beside the code it tests and
compiles as a file *of* its package: `pkg::` siblings and dependencies
resolve exactly as they do for the rest of the package. Each test passes
by exiting 0; a failed `assert` panics, which fails it. Tests run on
Node whatever the package's `target` says. `--watch` re-runs on save.

## `vilan bindgen <file.d.ts>`

Generates `external` bindings from a TypeScript declaration file, so a
third-party JavaScript library can be reached from Vilan without
hand-writing an `external struct` and an `[extern(…)]` `external fun`
per member.

**It is not a build step.** You run it by hand, once, and the `.vl` it
writes is yours: review it, edit it, commit it. Nothing in `build`,
`check`, or `run` reaches it, no manifest key turns it on, and nothing
regenerates it behind your back. Treat the output the way you treat
`vilan init`'s scaffold — a starting point you own from the moment it
lands, not a cache entry.

- `--platform <p>`: **required.** `node`, `deno`, `bun`, `browser`, or
  `@process`. Every generated binding is stamped `[platform("<p>")]`.
  There is no default and no sniffing of the `.d.ts`: generated bindings
  land in *your* code, which is unconstrained by platform unless it is
  fenced, so an unfenced browser-only binding in a Node project would
  compile clean and fail at runtime. A library genuinely used on both
  gets two runs, into two files.
- `--only <Type>`: emit only this declaration and everything reachable
  from its signatures — base types it `extends`, member types, parameter
  and return types, generic arguments. Repeatable, and the flags compose
  into one closure. Omitted: the whole file. A name the file does not
  declare is an error, so a typo fails instead of quietly writing less
  than you asked for. Use it when a declaration file is far larger than
  the slice you need; see [Filtering](#filtering-a-large-declaration-file).
- `-o, --output <path>`: where to write. Omitted: `<stem>.vl` beside the
  input (`leaflet.d.ts` → `leaflet.vl`).
- `--stdout`: print instead of writing.
- `--stats`: also report coverage — how many declarations and members
  bound, and which TypeScript constructs did not.

Attribute order in the output is not stylistic: the parser reads
`[extern(…)]` before `[platform(…)]`, and the other way round is a parse
error. Keep the pair in that order if you edit a generated file.

### What you get

```ts
declare function greet(name: string, loudly?: boolean): string;

interface Marker {
    readonly id: string;
    title: string;
    move(dx: number, dy: number): void;
    on(event: string, handler: (x: number) => void): void;
}
```

`vilan bindgen marker.d.ts --platform node` writes bindings in exactly
the dialect `std` is written in:

```vilan,norun
[extern("greet")]
[platform("node")]
external fun greet(name: str): str;

[extern("greet")]
[platform("node")]
external fun greet_with_loudly(name: str, loudly: bool): str;

external struct Marker;

impl Marker {
	[extern(get, "id")]
	[platform("node")]
	external fun id(self): str;

	[extern(get, "title")]
	[platform("node")]
	external fun title(self): str;

	[extern(set, "title")]
	[platform("node")]
	external fun set_title(self, value: str): void;

	[extern(method, "move")]
	[platform("node")]
	external fun move_(self, dx: f64, dy: f64): void;

	[extern(method, "on")]
	[platform("node")]
	external fun on(self, event: str, handler: |f64| void): void;
}

fun main() { }
```

Names become `snake_case` (the extern keeps the exact JS spelling), a
`readonly` property gets only a getter, and every `number` becomes `f64`,
because a `.d.ts` cannot say whether a number is meant as an integer.
Narrowing to `i32` is a human edit.

An **optional parameter becomes one binding per call arity**. TypeScript
optionals are trailing, so `greet(name)` and `greet(name, loudly)` are two
real host calls and become two real bindings of the same symbol — the
short one keeps the plain name. (`std` does the same by hand: `append` and
`append_text` both bind `appendChild`.)

### Constructors

TypeScript splits a host class in two: an `interface` for what an
instance has, and a `declare var` of the same name whose object type
carries the `new(…)` signature — the static side. bindgen reads the pair
back together, so a construct signature becomes a real constructor on the
type it yields:

```ts
interface Marker {
    readonly id: string;
}

declare var Marker: {
    prototype: Marker;
    new(id: string): Marker;
    readonly count: number;
};
```

```vilan,norun
external struct Marker;

impl Marker {
	[extern(new, "Marker")]
	[platform("node")]
	external fun new(id: str): Marker;

	[extern(get, "id")]
	[platform("node")]
	external fun id(self): str;

	[extern("Marker.count")]
	[platform("node")]
	external fun count(): f64;
}

fun main() { }
```

`new` is `Marker::new(…)`; the statics beside it become dotted globals
reached the same way, `Marker::count()`. `prototype` is the idiom's
marker, not a binding, and is dropped. A construct signature that names a
*different* type — `declare var Image: { new(): HTMLImageElement }` —
binds on that type instead, as `HTMLImageElement::new_image(…)`.

An object-typed global with **no** construct signature is not a class and
gets a TODO rather than an invented type; so does a global whose
constructor object is typed by a named interface rather than written
inline.

### Filtering a large declaration file

A declaration file is often far larger than the part you use, and
`extends` flattening multiplies it: Vilan has no struct inheritance, so
every base member is copied into each derived type. `--only` cuts the
output to a named type and its transitive closure:

```sh
vilan bindgen lib.dom.d.ts --platform browser --only HTMLCanvasElement -o canvas.vl
```

What "reachable" means is what *survives* the mapping table, not what the
TypeScript text mentions. An open union widens to `any`, so
`RenderingContext = CanvasRenderingContext2D | WebGLRenderingContext | …`
pulls in none of its members; `T | null` binds as `T`, so `T` is kept. A
base type is followed for the members flattened out of it, but its own
name is not emitted unless some signature names it.

Cycles are fine — a `Node` has an `ownerDocument` and a `Document` has a
`documentElement` — each declaration is visited once.

A caveat worth knowing before reaching for it on the DOM specifically:
the browser's element types are one strongly-connected component, so
`--only HTMLCanvasElement` against the real `lib.dom.d.ts` still keeps
about 900 declarations. The filter is doing its job; the type graph is
simply that dense. For a third-party library — bindgen's actual target —
the closure is usually a handful of types.

### Absence, and why there is no `Option`

Nothing bindgen emits is an `Option<T>`, and that is deliberate. Vilan has
no `null`, and `Option` is a **tagged array** at runtime — `Some(v)` is
`[0, v]`, `None` is `[1]` — which a third-party host neither produces nor
reads. Both directions break:

- reading, a host returning `"hello"` is tested as `value[0] == 0`, which
  is `"h" == 0`, so a *present* value arrives as `None`;
- writing, `None` reaches the host as the array `[1]` — and for an
  optional `boolean` argument, `[1]` is truthy.

So a type admitting `null`/`undefined` binds as the **bare type**, and the
binding carries a `///` note saying the value may be missing. Guarding is
yours. (`std` does use `Option` across `external` boundaries, but only
ones it owns — compiler intrinsics and its own runtime helpers, which know
the representation.)

### `// TODO(bindgen)`

**Nothing is ever dropped silently.** Every construct the generator
cannot express becomes a comment naming it and saying why, in place:

```text
// TODO(bindgen): numeric index signature `[index: number]: string` —
// this is an array-LIKE shape, NOT a JS array …
```

A generated file with TODOs is reviewable; one with silent gaps is a
landmine. The ones you will meet most:

| TypeScript | Why it can't map |
|---|---|
| `declare const x: T` (a plain global value) | Every `[extern(…)]` form binds a *call* or a receiver's property; none reads a bare global as a value. Bind its members as dotted globals: `[extern("x.member")]`. A global that is a *constructor object* is different — see [Constructors](#constructors). |
| `{ [key: string]: T }`, `{ [index: number]: T }`, `Record<K, V>` | An open keyed or array-like host object has no Vilan type at a host boundary — see below. |
| overloads | Vilan has one signature per name. The first wins; the rest are quoted so you can hand-split them into distinct names. |
| `A \| B` (open unions), intersections | No union or structural types in Vilan; widened to `any`. |
| `namespace`, `declare module`, `declare global`, conditional/mapped types, `keyof` | Out of v1 scope. |

### Why every interface becomes an `external struct`

A Vilan `struct` is a **positional array** at runtime: `struct Point { x:
f64 }` is `[x]`, and `p.x` is `p[0]`. A host object `{x: 1}` read through
one yields `undefined`, silently. Only an `external struct` — an opaque
handle whose fields are reached through `[extern(get/set, …)]` — survives
the crossing. The same fact rules out three tempting mappings:

- a TS discriminated union is a tagged *object*, while a Vilan `enum` is
  `[tag, …payload]` — unless it is a **backed** enum, which is the bare
  backing value and *does* cross (that is what a closed string set maps to,
  below);
- `Map<str, T>` is a Vilan struct over a hashed native map, not a plain
  host object;
- `List<T>` is a real JS array, and an array-*like* (`{[index: number]:
  T}` — numeric keys and `length`, no `Symbol.iterator`) is not one:
  iterating it throws. Convert at the boundary with `Array.from` and bind
  the result as `List<T>`.

A `T[]`/`Array<T>` in a `.d.ts` *is* a real array and does map to
`List<T>`; a TS tuple `[A, B]` maps to `(A, B)`, since a Vilan tuple is a
JS array too.

### Closed string sets

A named union of string literals becomes a **backed enum** — each variant
carries the host string it stands for, so the enum *is* that string at
runtime and crosses the boundary unchanged. Every position takes it
directly, with no wrapper and no forwarder: a parameter, a return, a
property's getter and setter, a `List<Align>`, a callback's own parameter.
bindgen emits signatures only; there is never a generated body.

```vilan,norun
enum Align {
	Start = "start",
	End = "end",
}

external struct Chart;

impl Chart {
	[extern(set, "align")]
	[platform("node")]
	external fun set_align(self, value: Align): void;

	[extern(get, "align")]
	[platform("node")]
	external fun align(self): Align;

	[extern(method, "onAlign")]
	[platform("node")]
	external fun on_align(self, handler: |Align| void): void;
}

fun main() { }
```

Nothing checks the read direction at the boundary — the host can answer
with a string that is none of the variants — but nothing has to: an
exhaustive `match` on a backed enum traps and names the value rather than
returning a confident wrong variant. Where an unrecognized value is an
answer you expect rather than a bug, hand-edit the binding to the guarded
shape: bind the raw `str` under a `[doc(hidden)]` name and forward through
`Align::parse`, which returns `Option<Align>`. The generated file is
ordinary source, and that edit is one of the reasons it is yours to keep.

An *inline* `"left" | "right"` is widened to `str` instead — safe and
exact, since that is what the host takes; only a union the library
author bothered to name earns a type.

### Scope

bindgen targets the **third-party library `std` doesn't wrap**. It is
not for regenerating `std::browser::dom`, which is a deliberately
curated subset reviewed line by line, and it does not resolve types
across files: a name declared in another `.d.ts` widens to `any` with a
TODO.

## `vilan upgrade`

Replaces this binary (and `vilan-lsp` beside it) with the newest
release, downloading for your platform and swapping the pair atomically
(`vilan-lsp` first, so the two are never newer-cli/older-lsp). The
licenses and third-party notices travel along. `--check` reports whether
a newer release exists and changes nothing.
