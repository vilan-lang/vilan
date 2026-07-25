# Hello vilan

vilan compiles to JavaScript. Your programs run on node (or deno, or bun)
and in the browser. One tool, the `vilan` binary, does everything: build,
run, check, format, test.

## A first program

```vilan
import std::print;

fun main() {
	print("hello");
}
```

Save that as `hello.vl` and run it:

```sh
vilan run hello.vl      # build + run
vilan build hello.vl    # just compile — writes hello.js
vilan check hello.vl    # just type-check — writes nothing
```

`fun main` is the entrypoint. It runs automatically, so there is no
`main()` call at the bottom of the file.

Two small things you'll notice compared to JS. First, the standard library
is imported explicitly — even `print`. Your files will start with a few
`import` lines, just like ES modules. Second, indentation is tabs by
convention, and `vilan fmt` will format files for you.

## The CLI

| Command | What it does |
|---|---|
| `vilan build [path]` | compile to `<file>.js` (no path: use the nearest `vilan.toml`) |
| `vilan check [path]` | type-check and report problems, write nothing |
| `vilan run [path] [args…]` | build and run; extra args reach `process::args()` |
| `vilan fmt [paths…]` | format source files in place (`--check` to verify only) |
| `vilan test [path]` | run `*_test.vl` files (a failed `assert` panics = test fails) |

Flags you'll actually use: `--watch` rebuilds (or re-runs, or re-checks)
whenever a source file changes. `--platform browser` builds for the
browser instead of node (`--target` also works). `--stdout` prints the JS
instead of writing a file.

## Projects: `vilan.toml`

A single `.vl` file is fine for experiments. Real projects get a folder
with a `vilan.toml` manifest. An application looks like this:

```toml
[package]
name = "hello"
target = "browser"          # node (default) | deno | bun | browser

[package.dependencies]
common = { path = "../common" }
```

A library is the same idea, but it has no entrypoint. It exists to be
imported by other packages:

```toml
[library]
name = "common"
```

A dependency can also live in a git repository, pinned to one exact
point — a tag or a commit, never a branch:

```toml
[package.dependencies]
shapes = { git = "https://github.com/someone/shapes", tag = "v1.2.0" }
pinned = { git = "https://github.com/someone/other", rev = "9f2c1ab" }
```

The repository must be a `[library]` with its `vilan.toml` at the root.
`vilan build` (or `check`, or `run`) fetches it once into
`~/.vilan/git-deps/` and reuses that checkout forever after — so builds
work offline, and a moved tag cannot change what you already built.
Nothing else fetches: the editor uses the cache when it's there and
never reaches the network.

A workspace groups several packages so they build together with one
`vilan build .` at the root:

```toml
[project]
packages = ["common", "client", "server"]

[project.dependencies]
shapes = { git = "https://github.com/someone/shapes", tag = "v1.2.0" }
```

Dependencies declared at the workspace root are there for the members to
share, so the version lives in one file. A member takes one by name:

```toml
[package.dependencies]
shapes = { project = true }
```

That is the whole form — `project = true` and nothing else, once per
dependency you want. It is opt-in on purpose: a member that stays silent
gets nothing, so adding a dependency at the root can never change what
another package sees. A `path` written at the root is relative to the
*root*, since that is where you wrote it. And a member that declares its
own `shapes = { path = "…" }` simply uses that one — there is no
shadowing rule to remember, because inheritance only happens where you
ask for it.

By default the source root is the package directory and the entry file is
`main.vl`. You can point elsewhere with `root = "src"` and
`entry = "app.vl"` if you prefer.

## Imports

```vilan,fragment
import std::print;                          // one item
import std::reactive::{ Signal, combine };  // several at once
import std::option::Option::{ self, Some, None };  // a type plus its variants
import pkg::routes::{ Route, parse };       // another file in YOUR package
import common::{ Task, KoltClient };        // a dependency, by its name
```

There are three places an import can come from:

- `std::…` is the standard library.
- `pkg::…` is your own package. `pkg::routes` means "the file `routes.vl`
  next to my entry file". A module is just a file — there is no separate
  module declaration.
- Anything else is a dependency, under the name you gave it in
  `vilan.toml`.

The `{ self, Some, None }` form is worth remembering: it imports the
`Option` type *and* its variants, so you can write `Some(x)` without
qualifying it.

## The shape of a full-stack app

When you get to building a client + server app, the smallest layout is
**one package with two entries** — the browser client and the node server
build from the same source tree:

```toml
[package]
name = "app"

[entry.client]
target = "browser"

[entry.server]
```

```
app/
  vilan.toml
  src/
    client.vl     the browser entry
    server.vl     the node entry
    …             everything else, shared by whichever entry reaches it
```

Larger apps split into a `[project]` workspace of packages and libraries,
as above. Either way, the compiler knows which standard-library modules
exist on which platform: if code the browser entry can *reach* calls into
`std::db` (a server thing), you get a clear compile error naming the call
chain — importing the module is fine, reaching it is what's checked. The
[platforms chapter](platforms.md) has the details, and the
[walkthrough](../guide/walkthrough.md) builds a whole app in this shape.
