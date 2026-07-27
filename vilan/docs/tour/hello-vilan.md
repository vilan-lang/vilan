# Hello Vilan

Vilan compiles to JavaScript. Your programs run on node (or deno, or bun)
and in the browser. One tool, the `vilan` binary, does everything:
scaffold, build, run, check, format, test.

## Install the toolchain

You need two things: [node](https://nodejs.org) (to *run* what you build)
and the `vilan` binary itself. On Linux and macOS:

```sh
curl -fsSL https://github.com/vilan-lang/vilan/releases/latest/download/install.sh | sh
```

On Windows, in PowerShell:

```powershell
irm https://github.com/vilan-lang/vilan/releases/latest/download/install.ps1 | iex
```

Either way `vilan` (and `vilan-lsp`, the language server) lands in
`~/.vilan/bin` — `%USERPROFILE%\.vilan\bin` on Windows. The unix script
prints the PATH line to add; the PowerShell one edits your user PATH
itself, so open a new terminal afterwards. `vilan --version` confirms it
worked, and `vilan upgrade` updates it later.

Homebrew (`brew install vilan-lang/vilan/vilan`) and building from source
are the other two routes; the
[repository README](https://github.com/vilan-lang/vilan#getting-started)
has both. Every command in this book assumes only that `vilan` is on your
PATH.

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

## Start a project

A single file is fine for a first look. For anything you will come back
to, scaffold a project: `vilan init` writes a manifest, sources that
already compile, and a `.gitignore`.

```sh
vilan init my-app                       # pick a template at the prompt
vilan init my-app --template fullstack  # or say which one outright
cd my-app
vilan run .
```

Three templates, for the three shapes that exist:

| `--template` | What you get |
|---|---|
| `node` | the smallest real package: an entry, a module beside it, a `*_test.vl` |
| `browser` | a reactive browser app — `index.html` beside the emitted bundle |
| `fullstack` | one package, two entries: a browser client and a node server ([below](#the-shape-of-a-full-stack-app)) |

`fullstack` is the default — a bare Enter at the prompt takes it — because
it is the shape the examples and the walkthrough teach. Pass `--template`
and there is no prompt at all, which is what a script wants (without a
terminal to prompt on, `vilan init` says so and stops rather than
hanging).

With no name, `vilan init` scaffolds into the current directory, as long
as that directory is not already a project. Nothing is ever overwritten,
either way. The `[package] name` comes from the directory's name, with
anything a name cannot carry folded to `_` (`my-app` → `my_app`). No
repository is created: you get a `.gitignore`, and `git init` stays
yours.

## The CLI

| Command | What it does |
|---|---|
| `vilan init [name]` | scaffold a project; `--template` picks `node`, `browser`, or `fullstack` |
| `vilan build [path]` | compile to `<file>.js` (no path: use the nearest `vilan.toml`) |
| `vilan check [path]` | type-check and report problems, write nothing |
| `vilan run [path] [args…]` | build and run; extra args reach `process::args()` |
| `vilan fmt [paths…]` | format source files in place (`--check` to verify only) |
| `vilan test [path]` | run `*_test.vl` files (a failed `assert` panics = test fails) |

Flags you'll actually use: `--watch` rebuilds (or re-runs, or re-checks)
whenever a source file changes. `--platform browser` builds for the
browser instead of node (`--target` also works). `--stdout` prints the JS
instead of writing a file.

A `*_test.vl` lives beside the code it tests and compiles as a file *of*
its package: it imports `pkg::` siblings and the package's dependencies
exactly the way the rest of the package does. Tests run on node, whatever
the package's `target` says.

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
`vilan build .` at the root. You need it less often than you might
expect — a client + server app is *one* package with two entries (see
[below](#the-shape-of-a-full-stack-app)) — so reach for a workspace when
members genuinely want their own manifests and dependency sets:

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

By default a package's sources live in `src/` and the entry file is
`src/main.vl` — that is where `vilan build` looks when the manifest says
nothing. Point it elsewhere with `root = "."` (sources beside the
manifest) or `entry = "app.vl"`.

## Imports

```vilan,fragment
import std::print;                          // one item
import std::reactive::{ Signal, combine };  // several at once
import std::option::Option::{ self, Some, None };  // a type plus its variants
import pkg::routes::{ Route, parse };       // another file in YOUR package
import common::{ Note, NotesClient };       // a dependency, by its name
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

Start here — `vilan init my-app --template fullstack` writes exactly this
layout, ready to run. It is the shape the examples use too: the
[walkthrough app](https://github.com/vilan-lang/vilan/tree/main/vilan/examples/walkthrough/),
the [to-do app](https://github.com/vilan-lang/vilan/tree/main/vilan/examples/todo/),
and the [SSR example](https://github.com/vilan-lang/vilan/tree/main/vilan/examples/ssr/)
are all one package with two entries. Larger apps split into a `[project]`
workspace of packages and libraries, as above. Either way, the compiler knows which standard-library modules
exist on which platform: if code the browser entry can *reach* calls into
`std::db` (a server thing), you get a clear compile error naming the call
chain — importing the module is fine, reaching it is what's checked. The
[platforms chapter](platforms.md) has the details, and the
[walkthrough](../guide/walkthrough.md) builds a whole app in this shape.
