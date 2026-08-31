# Projects and dependencies

[Hello Vilan](hello-vilan.md#projects-vilantoml) covers the two basic
manifests: an application (`[package]`) and a library (`[library]`). This
page is the rest of the manifest's vocabulary — external dependencies,
pre-build commands, and workspaces — for when a project grows into it.

## Git dependencies

Beyond `path` dependencies, a dependency can live in a git repository, pinned to one exact
point (a tag or a commit, never a branch):

```toml
[package.dependencies]
shapes = { git = "https://github.com/someone/shapes", tag = "v1.2.0" }
pinned = { git = "https://github.com/someone/other", rev = "9f2c1ab" }
```

The repository must be a `[library]` with its `vilan.toml` at the root.
`vilan build` (or `check`, or `run`) fetches it once into
`~/.vilan/git-deps/` and reuses that checkout forever after, so builds
work offline and a moved tag cannot change what you already built.
Nothing else fetches: the editor uses the cache when it's there and
never reaches the network.

## The prelude key

`prelude` names the module whose exports are in scope in every one of
this package's files with no `import`. It sits on `[package]` and on
`[library]` alike:

```toml
[package]
name = "app"
prelude = "std::web"
```

| Value | Meaning |
|---|---|
| *omitted* | std's base set: `print`, `Option`/`Some`/`None`, `Result`/`Ok`/`Err` |
| `"std::web"` | the base set plus `Signal`, `view`, `View`, and the modules `style` and `ui` |
| `"pkg::my_prelude"` | your own module — its exports are the ambient names |
| `"some_dep::their_prelude"` | a dependency's module |
| `false` | no prelude at all |

A custom prelude is an ordinary module of re-exports, and it **replaces**
std's rather than extending it — extension is spelled by re-exporting
what you want to keep:

```vilan,fragment
export import std::io::print;
export import std::option::Option::{ self, Some, None };
export import std::reactive::Signal;
export import std::style;                 // a whole MODULE, as `style::…`
```

Three rules make the key safe to use:

- **Per package, never inherited.** A dependency resolves under the
  prelude *its* manifest declares. You cannot change what a dependency's
  source means, and it cannot inject names into yours — not even through
  a `[project]` workspace root, which has no `prelude` key at all. Each
  member states its own.
- **The weakest scope.** A local declaration or an explicit import of a
  prelude name wins, silently. Adding a prelude cannot break code that
  compiles today.
- **`"std"` is not a value.** It names the package root, not a prelude
  module; the manifest refuses it and points at `"std::prelude"` (the
  default) or `"std::web"`.

A prelude that re-exports a platform-layered name makes your package's
ambient scope platform-dependent. Nothing special happens — platform
coloring still reports at the point the code becomes reachable — but
"my prelude broke my server build" is a confusing way to learn it.

The standard library declares `prelude = false`: 264 names across 59
files, where "which module is this from" has to be answerable by reading
the file.

## Reserved names

Four names are refused wherever a manifest names a package: `std`,
`pkg`, `macro_std`, and `vilan`. The first three are the import roots
the toolchain owns — `std::` is always the standard library, `pkg::` is
always your own package, `macro_std::` is always the macro world's std —
so a dependency declared under one of them could only shadow the root or
vanish behind it. `vilan` is the language's own name, held for official
packages to come. The dependency key is yours to choose, so pick any
other name; the library it points at keeps its own.

## Pre-build commands and `default-entry`

Two more keys matter once a project grows. `[build] run` is a command
line (or a list of them) for your shell, run before each build and each
`--watch` round: an asset pipeline, a codegen sidecar. It runs with your
privileges and Vilan doesn't prompt — the manifest is yours, and this is
the trust `cargo build` and `npm run` already take. A step too expensive
to repeat gets a `[[build.hook]]` instead: a name, and the `inputs` and
`outputs` that decide whether it needs to run at all. A hook that writes
Vilan modules also wants `generated`, naming the directory it writes
them into, so `vilan fmt` leaves them alone instead of rewriting bytes
the hook is watching. And `default-entry` names the entry `vilan run`
should drive when a package has several. All are covered in
[the dev loop](../guide/dev-loop.md).

## Workspaces

A workspace groups several packages so they build together with one
`vilan build .` at the root. You need it less often than you might
expect, since a client + server app is *one* package with two entries
(see [the full-stack shape](platforms.md#full-stack-packages)). Reach for a workspace
when members want their own manifests and dependency sets:

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

That is the whole form: `project = true` and nothing else, once per
dependency you want. It is opt-in on purpose: a member that stays silent
gets nothing, so adding a dependency at the root can never change what
another package sees. A `path` written at the root is relative to the
*root*, since that is where you wrote it. And a member that declares its
own `shapes = { path = "…" }` uses that one. There is no shadowing rule
to remember, because inheritance only happens where you ask for it.

