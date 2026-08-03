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

## Pre-build commands and `default-entry`

Two more keys matter once a project grows. `[build] run` names a
command (or a list of them) to run before each build and each `--watch`
round: an asset pipeline, a codegen sidecar. And
`default-entry` names the entry `vilan run` should drive when a package
has several. Both are covered in [the dev loop](../guide/dev-loop.md).

## Workspaces

A workspace groups several packages so they build together with one
`vilan build .` at the root. You need it less often than you might
expect, since a client + server app is *one* package with two entries
(see [below](#the-shape-of-a-full-stack-app)). Reach for a workspace
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

