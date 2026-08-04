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
build. A single-entry package writes `<file>.js` beside the entry; a
multi-entry package writes `dist/<name>.js` per entry. Assets emitted at
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

A `browser` entry with `[entry.<name>] split = true` writes an eager
bundle plus one file per route arm and a `<name>.chunks.json` listing
them, and warns when the split cost the first load more than it deferred.
The leg's chunk files belong to its last build: a build that writes none
removes any a previous one left. `vilan run` ignores `split` — the dev
loop swaps whole bundles — and emits the leg as one file.

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

## `vilan upgrade`

Replaces this binary (and `vilan-lsp` beside it) with the newest
release, downloading for your platform and swapping the pair atomically
(`vilan-lsp` first, so the two are never newer-cli/older-lsp). The
licenses and third-party notices travel along. `--check` reports whether
a newer release exists and changes nothing.
