# Vilan for VS Code

[Vilan](https://vilan-lang.github.io/vilan/) is a language for building
full-stack web apps. It compiles to JavaScript and runs on node and in the
browser, but it is not JavaScript: values are copied instead of shared, there
is no `null` and no exceptions, `await` is implicit, and the compiler checks
the things you usually find out at runtime.

This extension is the editor half of that toolchain. Everything below is the
compiler's own analysis, served by `vilan-lsp` — the same front end that
builds your project, so the editor and the build never disagree.

## What you get

- **Live diagnostics** — the compiler's errors as you type, with its notes and
  help text.
- **Completion** — call-shaped, with the signature and doc comment on the item;
  scope, member, and import-path positions, plus snippets for `fun`, `struct`,
  `for`, and `match`. Configurable down to plain names (`vilan.completion.functionCall`).
- **Hover** — inferred types on locals and parameters (loan conventions
  included), doc comments on everything you declared, and documentation on the
  language's own keywords, deep-linked into the book.
- **Inlay hints** — the inferred types of `let` bindings and parameters.
- **Semantic highlighting** from the analyzer, over a TextMate grammar
  (including `vilan` code fences in Markdown).
- **Go to definition** — including into the `std` package.
- **Find references** and **rename** (locals, parameters, fields).
- **Document outline**.
- **Format Document** — `vilan fmt`'s formatter, byte for byte.
- **Organize Imports** — sorts imports into canonical order and prunes unused
  ones, from the Source Action menu or on save.
- **A `vilan.toml` schema** — completion and validation in the manifest, for
  TOML extensions that consume contributed schemas (Even Better TOML).

All of it keeps working while the file has errors: the server analyzes the
recovered parse, so a half-typed line does not take your hovers with it.

## Getting started

**1. Install the toolchain** — the extension talks to `vilan-lsp`, which ships
with the compiler. On Linux and macOS:

```sh
curl -fsSL https://github.com/vilan-lang/vilan/releases/latest/download/install.sh | sh
```

On Windows, in PowerShell:

```powershell
irm https://github.com/vilan-lang/vilan/releases/latest/download/install.ps1 | iex
```

That puts `vilan` and `vilan-lsp` in `~/.vilan/bin` — on Windows,
`%USERPROFILE%\.vilan\bin`. The unix script prints the `PATH` line to add; the
PowerShell one adds the directory to your user `PATH` itself, so open a new
terminal afterwards. `vilan upgrade` updates both later, and
[every release](https://github.com/vilan-lang/vilan/releases) also carries
plain archives if you would rather unpack one yourself.

**2. Open a `.vl` file.** The extension starts the server on first open.

Keep the two in step — the extension and the toolchain ship from one repo at
one version, and a server older than the extension will not know about newer
language features.

### Finding the server

The extension runs `vilan-lsp` from your `PATH`. If it lives somewhere the
editor's `PATH` does not reach — a checkout's `target/release/vilan-lsp`, or an
install directory you did not add — point **`vilan.server.path`** at it
(absolute, or a name to resolve on `PATH`).

The server carries the `std` sources inside itself, so definitions into `std`
resolve on a machine with nothing else installed. Inside a vilan checkout it
uses that working tree instead; **`vilan.stdPath`** overrides both.

## Settings

| Setting | Type | Default | What it does |
| --- | --- | --- | --- |
| `vilan.server.path` | string | `vilan-lsp` | Path to the `vilan-lsp` executable (on `PATH`, or absolute). |
| `vilan.stdPath` | string | `""` | Path to the `std` source root (`vilan/std/src`). Overrides auto-discovery; sets `VILAN_STD` for the server. |
| `vilan.inlayHints.enabled` | boolean | `true` | Show inlay type hints. Applies live. |
| `vilan.semanticTokens.enabled` | boolean | `true` | Use analyzer-based semantic highlighting; when off, the TextMate grammar is used. Applies live. |
| `vilan.completion.functionCall` | `none` \| `parensOnly` \| `full` | `full` | How completion inserts a function or method call. |
| `vilan.organizeImports.onSave` | boolean | `false` | Run Organize Imports before each save. |

Every setting applies live, with no window reload: the feature toggles are
pushed to the running server, and a changed `vilan.server.path` / `vilan.stdPath`
restarts it for you. (There is also a **Vilan: Restart Language Server**
command, for when you have rebuilt the binary underneath it.)

`vilan.organizeImports.onSave` is handled by the extension's own on-save hook,
so it leaves your `editor.codeActionsOnSave` untouched. If you would rather
drive it through that standard mechanism, leave this off and add
`"editor.codeActionsOnSave": { "source.organizeImports": "explicit" }` instead —
organizing is a fixed point, so having both on is harmless.

Pruning is conservative: it never runs while the file has errors, never removes
a re-export, and keeps any import that derive-generated code references.

## Learning vilan

- [The book](https://vilan-lang.github.io/vilan/) — guide, tour, and the
  language specification.
- [Coming from JavaScript](https://vilan-lang.github.io/vilan/tour/coming-from-javascript.html)
  — the differences that will bite first.
- [The repository](https://github.com/vilan-lang/vilan) — including a working
  full-stack example app.

> **Status: fast-moving alpha.** The language changes weekly and there are no
> stability promises yet.

## Developing the extension

From a checkout of the repository:

```sh
npm install
npm run build      # bundle to out/extension.js
```

Then press <kbd>F5</kbd> in VS Code to launch an Extension Development Host
with the extension loaded, and open a `.vl` file to activate it. Point
`vilan.server.path` at your `target/release/vilan-lsp` to test against a local
build of the server.

`icon.png` is a derived asset, not a source: it is rendered at 256×256 from the
brand master `assets/branding/dark_icon.svg` — regenerate it with
`python3 scripts/icon_png.py` after any change to the mark.

## License

MIT or Apache-2.0, at your option — see
[LICENSE.md](https://github.com/vilan-lang/vilan/blob/main/editors/vscode/LICENSE.md).
The vilan
logo and icon are covered by [their own license](https://github.com/vilan-lang/vilan/blob/main/assets/branding/LICENSE)
instead.
