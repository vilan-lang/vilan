# The editor

Vilan ships a language server, `vilan-lsp`, beside the `vilan` binary in
every release — same repository, same version number, installed by the same
script, updated by the same `vilan upgrade` (which replaces the server
first, so the pair is never newer-CLI/older-server). It speaks LSP over
stdio, so any LSP client can drive it; the one the project configures and
tests is the VS Code extension.

## Getting it

The [toolchain install](../tour/hello-vilan.md#install-the-toolchain) puts
`vilan-lsp` next to `vilan` in `~/.vilan/bin`. Then install **Vilan**
(`vilan-lang.vilan`) from the VS Code Marketplace or Open VSX. The
extension activates on a `.vl` file or a workspace containing a
`vilan.toml`, and finds the server by looking, in order, at
`vilan.server.path`, a `target/release/vilan-lsp` or `target/debug/`
build in the workspace (so a checkout of the compiler outranks the
installed copy — deliberate, for people working on vilan itself),
`~/.cargo/bin`, `~/.vilan/bin`, and finally `vilan-lsp` on your `PATH`.
If it finds none it says so, naming the command it tried and offering a
button into settings.

No other editor is configured in the repository today. The server needs no
arguments, so a generic LSP client pointed at `vilan-lsp` for the `vilan`
language works; nothing here is VS Code-specific except the packaging.

The server resolves `std` itself — `$VILAN_STD`, then an in-checkout
`vilan/std`, then a copy embedded in its own binary — so go-to-definition
lands inside the standard library on a machine with nothing else installed.

## What it gives you

**Diagnostics**, published as you type on a 150 ms debounce, the same
messages `vilan check` prints (notes included, as related information).
A save re-checks the saved file *and* every open file that imports it, so
breaking a shared module lights up its consumers rather than waiting for
you to visit them.

**Hover** — inferred types on locals and parameters, loan conventions
included; a field's `name: type` and a method's full signature behind a
`.`; your own doc comments; and documentation on the language's own
keywords, deep-linked into this book.

**Inlay hints** — the inferred type of a binding you left unannotated
(`let`/`mut`, a `for` binder, a comprehension binder). A parameter is not
hinted: its type is written in the signature already.

**Semantic highlighting** from the analyzer, over the TextMate grammar,
which also highlights `vilan` fences inside Markdown.

**Go to definition** (including into `std`), **find references**,
**rename**, and a **document outline**.

**Formatting** — the same `vilan_core` formatter `vilan fmt` runs, so the
editor and the CLI cannot disagree. Whole-document only; there is no range
or on-type formatting.

**Linked editing** for markup tag pairs: rename `<div>` and `</div>`
follows.

## Completion

Where the cursor is decides what is offered.

- **A bare position** offers everything in scope, every keyword, and four
  construct snippets (`fun … ( ) { }`, `struct … { }`, `for … in { }`,
  `match … { }`), which sort last.
- **After `.`** offers the receiver's fields and methods; **after `?.`**
  offers the *lifted* element's, so an `Option<Profile>` offers `Profile`'s
  members.
- **After `::`** offers an enum's variants and statics, a struct's statics,
  or a module's members — and only when the name on the left is actually
  **in scope**. It used to match that name against every type the compiler
  happened to have loaded, and the derive prelude keeps a handful of std
  modules loaded permanently, so `Ordering::` offered `Less`/`Equal`/
  `Greater` in a file that had never heard of `std::compare`. It resolves
  through scope now, exactly as the receiver of a `.` always has.
- **Inside an `import` path** the whole path completes, at every level: the
  head offers the origins (`std`, `pkg`, and each dependency by the name
  you import it under), an origin offers its modules, and a module offers
  its members — loaded on demand, so `import std::random::` completes in a
  file that has never mentioned `std::random`. A brace set completes at the
  same level as its module, so `import std::json::{ Json, ` keeps going.
  (Imports are read as single-line items; a braced group's later lines are
  not recognized.)
- **A name you have not imported** is offered too, labeled with the module
  it comes from and carrying the `import` as part of accepting it — one
  action writes both. Your own package's names rank ahead of `std`'s, so a
  std-heavy file's loaded surface cannot crowd them out of the 20-candidate
  cap.

Completion shapes a call for you by default; `vilan.completion.functionCall`
turns that down to parentheses only, or off.

`vilan.toml` gets its own completion — manifest keys and their closed sets
of values — from the server rather than from a JSON schema, so every editor
sees it.

## Quick fixes

Four, each attached to the diagnostic that earns it:

| Action | Offered on |
|---|---|
| ``Import `X` from std::json`` | `cannot find 'X'` where `X` is importable. One action per module when more than one exports the name — never a guess between them |
| ``Change to `entries` `` | a `did you mean …?` note on a misspelled struct-initializer field |
| ``Insert `;` `` | ``expected `;` to end this statement``, at the gap the diagnostic points at |
| ``Remove `;` `` | ``the `;` discards this body's last value`` — it finds the right `;` from the diagnostic's own bookkeeping, and declines rather than guess when a comment sits in the gap |

and two source actions:

| Action | Does |
|---|---|
| **Organize Imports** | sorts each top-level import run into canonical order (the same key `vilan fmt` uses) and prunes unused leaves, shrinking a brace set rather than deleting it. Offered only when it would change something |
| **Add All Missing Imports** | applies every unambiguous import quickfix in the file at once, skipping the ambiguous ones |

Organizing prunes only against a clean, current analysis: a file with
diagnostics, or one you are mid-edit in, is sorted and never pruned —
because "unused" is an answer you can only trust from a program that
type-checked. A module imported and used **only** through `::`
(`import std::math;` referenced as `math::min(1, 2)`) counts as used and
is kept.

`vilan.organizeImports.onSave` (off by default) runs the same action on
save. It is the extension's own hook rather than a line in your
`editor.codeActionsOnSave`, and organizing is a fixed point, so turning
both on is harmless.

## What it does while you type

Half-typed code is what a compiler is asked about most of the time. Two
behaviors follow from that, and both are worth knowing because they are the
difference between an editor that helps mid-edit and one that goes blank.

**A syntax error no longer blanks out the file.** The parser recovers at
statement and item boundaries: a statement it cannot read is reported and
skipped to the next `;`, `}`, or declaration keyword, and everything around
it — the statements beside it, the functions below it, the whole file tail
— still reaches the analyzer. So the diagnostics a file already had stay on
screen instead of vanishing for the two keystrokes it takes to type
`print(1);`, and a function body with one broken statement in it loses that
statement rather than being discarded whole. `vilan check` answers the same
way; `vilan build` still stops, because a recovered file is not something
to emit from.

**Answers stay in the last-analyzed text's coordinates** until the next
analysis lands, a few dozen milliseconds later. Hover, go-to-definition,
references, rename, the outline, inlay hints, semantic tokens and
completion's lookups all convert the cursor through the analyzed text
first. That is correct for the analyzed program, and therefore visually
correct everywhere except the line you are actively editing; converting the
same bytes through the live buffer would be correct for neither. Semantic
highlighting additionally carries the unchanged tail of the file across an
analysis, so the colours below your edit do not flash off and back.

Two requests decline rather than answer wrong while the buffer is ahead of
the analysis: **rename** ("still analyzing this file; retry in a moment")
and **code actions**, which refuse silently because editors ask for them
automatically when a menu opens or a file saves.

## What it does not have

Named so you do not go looking: there is no signature-help popup (a
signature reaches you through hover and a completion item's detail line),
no folding ranges, no workspace-wide symbol search (the outline is
per-file), no document-highlight-on-cursor, no code lens, no call or type
hierarchy, no go-to-type-definition / implementation / declaration beyond
plain go-to-definition, and no pull diagnostics — diagnostics are pushed.

## Settings

| Setting | Default | |
|---|---|---|
| `vilan.server.path` | — | an explicit `vilan-lsp` binary; changing it restarts the client |
| `vilan.stdPath` | — | an explicit `std` root, overriding discovery |
| `vilan.inlayHints.enabled` | `true` | |
| `vilan.semanticTokens.enabled` | `true` | off falls back to the TextMate grammar |
| `vilan.completion.functionCall` | `full` | `parensOnly`, or `none` |
| `vilan.organizeImports.onSave` | `false` | |

Everything but the two paths applies live. **Vilan: Restart Language
Server** is in the command palette when you want the blunt instrument, and
the **Vilan Language Server** output channel carries the server's own log.
