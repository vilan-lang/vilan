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
**rename**, and a **document outline**. References and rename reach in
both directions across your open files: asked in the file that *defines*
a symbol, they also find the open files that import it.

**Formatting** — the same `vilan_core` formatter `vilan fmt` runs, so the
editor and the CLI cannot disagree. Whole-document only; there is no range
or on-type formatting.

**Linked editing** for markup tag pairs: rename `<div>` and `</div>`
follows.

**Dead code, faded.** Code nothing uses is dimmed rather than warned
about: it does not enter the Problems count, does not badge the file, and
gates nothing. Five things fade — an unused import, a `let` inside a
function body that nothing reads, the statements after one that cannot
return, a top-level item no entry of your package reaches, and every
top-level item of a module no entry loads at all.

The last two are the whole-package ones, and they are worth knowing
precisely, because a fade means "you may delete this."

- **What can fade:** a top-level `fun` and a module-level `let`. A
  `struct`, an `enum`, a `trait` and an `impl` block never fade, used or
  unused — they compile to nothing either way, so "does the build keep
  it" is not a question that has an answer for them.
- **What "unused" means:** no entry of the package reaches it. A package
  with several entries is a union — an item only your `probe` entry calls
  is used. An item behind a `[platform]` fence is used if the entry for
  that platform reaches it.
- **A `[library]` never fades a top-level item.** A library has no
  entries, so there is nothing to compute the answer from, and every
  top-level item is surface a consumer may import — which is what keeps
  you from having to fork a library that forgot to export something. This
  holds for a library inside a workspace too, even when the only consumer
  today is a sibling package. Use `[doc(hidden)]` to keep a name out of
  consumers' completion without forbidding it.
- **A declared `generated` root never fades.** `[package] generated =
  "src/icons"` already tells `vilan fmt` to leave a machine-written
  directory alone; it tells the fade the same thing.
- **`_`-led names never fade**, the way an unused local does not. It is
  the "I know" marker.
- **Trait impl members never fade in this version.** Whether a trait
  method is reached depends on which types your program constructs, so
  the answer moves in blocks as you edit; inherent impl members do fade.
- **The fade goes off while you type** and comes back a moment after you
  stop. The fact that would prove an item used lives in another file, so
  a fade held across an edit could say "dead" about code you have just
  started calling — and that is the one mistake a fade must not make.
  Being late is fine; being wrong is not. A syntax error anywhere in the
  package holds the whole package's fades off for the same reason.

## Completion

Where the cursor is decides what is offered.

- **A bare position** offers everything in scope, every keyword, and four
  construct snippets (`fun … ( ) { }`, `struct … { }`, `for … in { }`,
  `match … { }`), which sort last.
- **After `.`** offers the receiver's fields and methods; **after `?.`**
  offers the *lifted* element's, so an `Option<Profile>` offers `Profile`'s
  members. What counts as "after `.`" ignores the whitespace and comments
  around it, so a chain written down the page (`items`, then `.filter(f)` on
  the next line) completes at each link; and what follows the cursor never
  changes the answer, so completing *between* two links works too.
  The methods include the ones the receiver's type inherits as **trait
  defaults** — `xs.iter().` offers the whole `Iterator` surface, not the one
  method `ListIterator`'s own `impl` block writes out, and a type that
  implements `Ord` offers `min`/`max`/`clamp` with the comparisons its
  supertraits provide.
- **Inside a string or a comment** nothing is offered. A caption is text, not
  code, and a `.` in one is not a member access.
- **Inside a `css` block** the vocabulary is CSS, and nothing in scope is
  offered at all. An undotted item completes **property names** — every slot
  a `Style` method writes, so the list is std's own surface rather than an
  invented one — and a dotted item completes the **condition combinators**
  (`md`, `hover`, `within`, …). A value offers nothing: it is source text,
  not an expression, so a name in scope would be the wrong answer. A `{…}`
  hole is an ordinary expression and completes as one, which is how a typed
  value gets in. All four answers hold in a block you are still **writing** —
  one whose closing `}` you have not typed yet — because that is when
  completion is most use; a nested rule left open the same way is a body too.
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

Seven, each attached to the diagnostic that earns it:

| Action | Offered on |
|---|---|
| ``Import `X` from std::json`` | `cannot find 'X'` where `X` is importable. One action per module when more than one exports the name — never a guess between them |
| ``Change to `entries` `` | a `did you mean …?` note on a misspelled struct-initializer field |
| ``Insert `;` `` | ``expected `;` to end this statement``, at the gap the diagnostic points at |
| ``Remove `;` `` | ``the `;` discards this body's last value`` — it finds the right `;` from the diagnostic's own bookkeeping, and declines rather than guess when a comment sits in the gap |
| ``Wrap as `{Color::hex("#333")}` `` | a `#` in a `css` block. The character cannot lex at all, so the diagnostic is one column wide; the fix reads the whole colour off the line and routes it through `Color`, which carries its own `:root` line. Offered only when the run really is a colour (3, 4, 6 or 8 hex digits) |
| ``Use `.md { … }` `` | ``@media (min-width: …)`` in a `css` block. The breakpoint is chosen by the query's own min-width, and an arbitrary one becomes `.media("900px")` rather than no fix. The other at-rules have no combinator spelling, so they get the explanation alone |
| ``Remove `!important` `` | ``!important`` in a `css` block — a `Style` merges by record update, so a later declaration on the same property already wins. Takes the space before the marker with it |

and two source actions:

| Action | Does |
|---|---|
| **Organize Imports** | sorts each top-level import run into canonical order (the same key `vilan fmt` uses), prunes unused leaves (shrinking a brace set rather than deleting it), and strips imports the prelude already covers. Offered only when it would change something |
| **Add All Missing Imports** | applies every unambiguous import quickfix in the file at once, skipping the ambiguous ones |

An import the **prelude** covers is stripped for the same reason an unused
one is: `import std::io::print;` in a package whose prelude binds `print`
is redundant, and removing it changes nothing about what the file means.
The match is on the *definition*, not the name — an import of a
different `print` survives.

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

## Refactors

A refactor is offered on the construct your cursor is in, rather than on a
diagnostic or on the file:

| Action | Does |
|---|---|
| **Convert to a `style()` chain** | rewrites the `css { … }` block the cursor is in as the builder chain it lowers to — a declaration becomes a `.raw` link, a nested rule becomes a combinator link carrying the inner chain |
| **Convert to a `css` block** | the inverse, on a `style()` chain. Only the two rows of that lowering have a block spelling, so a chain carrying a typed property method (`.padding(space(4))`, which writes its slot through `with_length`) is not offered the conversion at all |

Both directions decline rather than guess. A **comment** inside the
construct stops the conversion, because its attachment is not recoverable
across the reshape — the same refusal `vilan fmt` makes when it declines to
reorder a commented block. So does a value carrying a **backslash**: a
chain's string literal has its escapes processed at emission and a block's
token run does not, so the two spellings would stop meaning the same thing.
A quoted value is fine — escaping a `"` into the literal round-trips
exactly — and the two directions are inverses on everything they accept.

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
