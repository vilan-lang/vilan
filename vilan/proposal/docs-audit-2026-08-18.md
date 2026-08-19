# Docs currency audit against v0.34.0 — the record (backlog D15)

STATUS: SHIPPED 2026-08-18
Scope: every page of `vilan/docs/` (55 before this change, 56 after), plus
the two highlighting grammars the book and the editor share.

The book's own design is `documentation.md`; its maintenance policy (§5) is
that a std/framework/language change updates the affected page in the same
commit. This audit asks whether that policy held across cycles 15–19 — the
releases that shipped the fullstack ladder, `std::fs`'s new reads,
`std::watch`, backed enums, the return-copy rules and the editing-DX arc.

The headline is that it mostly did. **Nothing in the book teaches a form
the compiler has removed**: `read_file_bytes`, the four `JsonValue`
`is_*` predicates, `.kind()` compared against a raw string, the
`<!--ssr-->` marker, `ChunkFile`, `fs::exists("dist/…")` and `node
dist/server.js` are all at zero hits. What the audit found instead is a
thinner failure: **signature tables that drifted** (they sit in
`vilan,fragment` fences, which the docs gate does not compile — the one
place in the book with no mechanical check on it), **prose that outlived
the mechanism it described**, and **sections a sweep touched at the edges
and left teaching the pre-ladder shape in the middle**.

## 1. Inventory

`CURRENT` = nothing found. `STALE` = teaches something no longer true.
`GAP` = a shipped surface the page should carry and does not.

### Tour

| Page | Verdict |
|---|---|
| `README.md` | STALE (index): the Tour table omitted `tour/projects.md`; the Appendix bullet omitted `appendix/cli.md`; the `process` row listed 5 of 8 modules |
| `tour/coming-from-javascript.md` | CURRENT |
| `tour/hello-vilan.md` | CURRENT |
| `tour/projects.md` | STALE: `[below](#the-shape-of-a-full-stack-app)` — an anchor that lives on another page |
| `tour/values-and-types.md` | CURRENT |
| `tour/functions-and-closures.md` | STALE: `draft<T>` missing its `PartialEq` bound; `turn` carrying a `sync` marker std does not have. GAP: a closure's own `: T` return annotation is now checked |
| `tour/data-and-traits.md` | GAP: `[derive(Json)]`/`[derive(Wire)]` on a backed enum encode the **backing** value |
| `tour/control-flow.md` | GAP: the `match` chapter never mentioned a **guard**, so the rule that a guarded leg proves no exhaustiveness (and a guarded last arm is refused) was absent from the page that teaches `match`; the backed-enum trap arm likewise |
| `tour/memory-model.md` | STALE: the view rule stated on the `await` **keyword**. GAP: the return-copy rules — the signature decides |
| `tour/async.md` | CURRENT |
| `tour/resources.md` | GAP: the overwrite rule (a write destroys what it replaces) absent; the container trap predates the provenance rework and omits `[Guard; 2]` |
| `tour/macros-and-const.md` | CURRENT |
| `tour/platforms.md` | GAP (minor): the extern section does not mention backed enums; `data-and-traits.md` carries it |

### Guides

| Page | Verdict |
|---|---|
| `guide/reactive.md` | STALE: `draft` printed without its `T: PartialEq` bound |
| `guide/ui.md` | STALE: the conditionals comments print `(\|\| View)` / `(\|T\| View)` where std declares `sync`. GAP: `swap_split` absent from the conditionals table |
| `guide/styling.md` | CURRENT — fully on the ladder (`serve_build`, `require_shell`) |
| `guide/routing.md` | STALE: the deep-link fence calls `serve_service` with **three** arguments; std declares four, and there is no overloading. GAP: the builder rung, and the split-route signals the page owns the topic for |
| `guide/services.md` | GAP: `RpcError` enumerated as three variants; std has five — and the missing `Contract(str)` is the one the very next paragraph describes |
| `guide/persistence.md` | **STALE (worst page)**: `std::fs` table missing four of the module's reads; "the typical server reads the client bundle and shell into memory once at boot"; the boot sequence ending `fs::read_file_to_str the bundle and shell` → `serve_service(...)` |
| `guide/ssr.md` | CURRENT — `require_build`/`require_shell`/`serve_build`/`Document::of`, and the `<!--ssr-->` removal explained |
| `guide/walkthrough.md` | CURRENT-by-mirror: byte-faithful to `examples/walkthrough`, which is itself one rung below the ladder's top (backlog E63) |
| `guide/dev-loop.md` | STALE: "CSS hot-swap works by bumping the cache-buster on your stylesheet `<link>`" — that is exactly the mechanism v0.34.0 replaced |

### std reference

| Page | Verdict |
|---|---|
| `std/collections.md` | CURRENT — all five signature blocks verified line-for-line |
| `std/option-result.md` | GAP: `Option::transpose` and `Option::flatten` absent; `parse_f64` and `bool::then_some` unmentioned |
| `std/strings.md` | GAP: `parse_f64` beside `parse_i32` |
| `std/numbers.md` | GAP: `i32::is_even`/`is_odd`; fifteen `f64` methods behind an ellipsis, `is_nan`/`is_finite`/`is_infinite` among them |
| `std/traits.md` | CURRENT |
| `std/cells.md` | STALE: `Shared::write` printed without its `borrows self` — on a page whose next paragraph is *about* that clause. GAP: `Shared::clone`, `Arena::branded` (the prose leans on `branded` and the example calls it) |
| `std/time.md` | CURRENT |
| `std/encoding.md` | STALE ×2: `encode_utf`/`decode_utf` for `encode_utf8`/`decode_utf8`, and `decode_binary` printed returning a bare `T` where std returns `Result<T, str>` — on a page whose thesis is "decoding is fallible". `JsonKind` itself fully landed |
| `std/net.md` | STALE: `WsEvent`'s variant is `Closed`, not `Close`, and a trailing `…` promises variants the enum does not have |
| `std/reactive.md` | CURRENT (every fragment verified against `reactive.vl`). STALE (minor): two `proposal/…` cross-references written as bare text, unfollowable from the published site |
| `std/style.md` | CURRENT |
| `std/rpc.md` | GAP: the server-plumbing section knew only `serve_service`/`serve_connected` — no `Service`, no `with_service`, and `serve_rpc` named elsewhere in the book with no signature anywhere |
| `std/browser.md` | STALE ×4: `mount_root` and three View-table rows (`bind_each`, `when`, `swap`) missing `sync` markers. GAP: nothing said what `mount`/`mount_root` do on an id that is not there; `swap_split`, `router::pending`, `router::chunk_error` absent |
| `std/dev.md` | CURRENT |
| `std/process.md` | CURRENT (the most current page in the book). GAP: `read_file_encoded` — a public `std::fs` function documented nowhere at all |
| `std/misc.md` | CURRENT |
| `appendix/editor.md` | NEW — see §2 |

### Specification

| Page | Verdict |
|---|---|
| `spec/introduction.md` | CURRENT |
| `spec/lexical.md` | STALE: the reserved-word list omits `resource`, a real keyword token; the contextual list omits `sync` and `platform` |
| `spec/grammar.md` | STALE: `resource`'s semantics called "forthcoming", four releases after §6.8 became their normative home |
| `spec/names.md` | CURRENT |
| `spec/types.md` | STALE: a tracked-gap note claiming a closure passed to a method's own generic parameter reaches its body unsubstituted — closed in v0.33.0, verified closed by probe |
| `spec/memory.md` | CURRENT. Instrument finding: two fences tagged ` ```vilan,ignore `, a tag the docs gate does not know, so they fell to the prose arm and were silently uncompiled |
| `spec/execution.md` | CURRENT |
| `spec/contexts.md` | CURRENT |
| `spec/const.md` | STALE: points at §7.4 for the initializer rules, whose normative home moved to §7.1 |
| `spec/macros.md` | CURRENT |
| `spec/platform.md` | CURRENT |
| `spec/appendix.md` | STALE: A.2's reserved-word list, same omissions as `lexical.md` |

### Appendix

| Page | Verdict |
|---|---|
| `appendix/cli.md` | CURRENT |
| `appendix/errors.md` | STALE: the resource-field derive entry lists `Wire`/`Hashable`/`PartialEq` and not `Json`; the view-across-`await` entry states the rule on the keyword. GAP: the whole v0.34.0 diagnostics wave unindexed, and no entry anywhere for a runtime panic |
| `appendix/gotchas.md` | GAP: nothing from v0.33.0 or v0.34.0 had landed, despite the page's own "grown as findings land" |
| `appendix/glossary.md` | GAP: no **backed enum** entry (a term of art on three pages); the **copy** entry omitted `return`, the exact seam five changelog entries were spent on |

**Counts (55 rows, one of them the new page):** clean 22 · STALE 20 ·
GAP 19 · NEW 1. Seven pages carry both a STALE and a GAP finding, which is
why the columns sum past the row count. Two findings inside those totals
are not about the language and could fairly be discounted:
`std/reactive.md`'s two unfollowable `proposal/…` references, and
`spec/memory.md`'s two fences tagged with a word the gate does not know.

**Where the drift concentrates.** Of the 20 STALE findings, **thirteen are
signatures printed inside a ` ```vilan,fragment ` fence** — the one
construct in the book the docs gate deliberately does not compile, and
therefore the only place a claim about the language can rot in silence.
Nine of those thirteen are a single mistake made nine times: a dropped or
invented `sync` marker on a closure parameter. Every `context` clause and
every `borrows` clause in the book was then swept mechanically against
`vilan/std/src/**` — both axes are now clean end to end, which is the
check worth repeating rather than the nine fixes.

## 2. What was fixed

Stale forms first, then gaps by how load-bearing the surface is.

- `guide/persistence.md` — rewrote three sections: the http section now
  teaches the `Server::builder()` chain with `serve_build`/`with_service`
  answering ahead of `on_request` (a compiled `norun` program replaces the
  prose), the `std::fs` table gained `read_file_encoded`/`read_bytes`/
  `read_dir`/`stat` with what each is for, and the boot sequence became the
  ladder — `require_build`, `require_shell`, then the builder — with a
  paragraph on why steps 2 and 3 refuse rather than degrade.
- `guide/dev-loop.md` — the CSS `<link>` section: the swap fetches the
  sidecar from the dev channel's own `/asset/<name>` route and injects a
  superseding `<style>`; it does not re-request the app's href, and the
  reason (a css-only round never restarts the server that read it at boot)
  is the point of the section.
- **The signature sweep** — thirteen printed signatures corrected against
  `vilan/std/src/**`, each verified at the declaration:
  - `guide/routing.md` — `serve_service` called with three arguments where
    std declares four. The most consequential of the lot: it is
    copy-paste-shaped and cannot compile.
  - `std/encoding.md` — `encode_utf`/`decode_utf` → `encode_utf8`/
    `decode_utf8` (no aliases exist, so the printed names were an
    unresolved-name error waiting to happen); `decode_binary` returns
    `Result<T, str>`, not a bare `T`.
  - `std/net.md` — `WsEvent::Close` → `Closed`, and the trailing `…`
    dropped: the enum is exactly four variants, which matters to anyone
    writing an exhaustive `match` on it.
  - `std/cells.md` — `Shared::write`'s `borrows self` restored (the page
    prints the clause correctly on `Arena::get` two blocks down, and the
    paragraph under it is *about* that clause); `Shared::clone` and
    `Arena::branded` added.
  - `std/browser.md` — `mount_root`'s `sync` marker, and the `sync` markers
    on `bind_each`'s two closures, `when`'s and `swap`'s.
  - `guide/ui.md` — the same markers in the conditionals comments.
  - `guide/reactive.md`, `tour/functions-and-closures.md` — `draft`'s
    `PartialEq` bound.
  - `tour/functions-and-closures.md` — `turn`'s *spurious* `sync`, replaced
    with `batch` as the genuinely-`sync` example so the contrast teaches
    the distinction rather than hiding it.
  - `std/numbers.md` — `lerp`'s second parameter is `to`, not `other`.
- `std/browser.md` — the missing-element-id panic, with its message and the
  `check_shell` tie-in; `swap_split` and the two route-chunk signals
  (`router::pending`, `router::chunk_error`) added.
- `std/option-result.md`, `std/strings.md`, `std/numbers.md` — the
  under-reported surfaces filled in: `Option::transpose`/`flatten`,
  `parse_f64`, `bool::then_some`, `i32::is_even`/`is_odd`, and the fifteen
  `f64` methods that were behind an ellipsis, with `is_nan`/`is_finite`/
  `is_infinite` called out as the three a reader actually goes looking for.
- `guide/services.md` — `RpcError`'s five variants, and the contract check
  now names the `Contract(str)` it produces.
- `tour/memory-model.md` — the view rule restated on **suspension** rather
  than on the `await` keyword, with the "reads the declaration of the
  function you call" rule; the return-copy rules added to Going Deeper.
- `tour/resources.md` — teardown on overwrites (binding, field, `&mut`
  pointee, and the destructor-before-write order); the container trap
  restated as a question about the type, with the generic-instantiation
  form and the `[Guard; 2]` exception.
- `tour/control-flow.md` — guards: what one is, that it proves nothing
  about completeness, and that the last arm may not be guarded; the
  backed-enum trap arm.
- `tour/data-and-traits.md` — the derives encode the backing value, and
  renaming one later is a wire change.
- `spec/lexical.md`, `spec/appendix.md` — `resource` into the reserved-word
  lists; `sync` and `platform` into the contextual lists.
- `spec/grammar.md` — `resource`'s dead "forthcoming" forward reference now
  points at §6.8.
- `spec/const.md` — §7.4 → §7.1 for the initializer rules.
- `spec/types.md` — the closed third clause of the tracked-gap note rewritten
  as history rather than as a live gap (probe below).
- `spec/memory.md` — two ` ```vilan,ignore ` fences retagged
  ` ```vilan,fragment `, the documented spelling.
- `std/process.md` — `read_file_encoded` added to the `std::fs` table, with
  a paragraph separating the three reads and naming the rename it came from.
- `std/rpc.md` — the server-plumbing section rewritten around
  `Service`/`with_service`, with `serve_rpc`'s signature (and why it is the
  odd one out: no upgrade, no registry, no fallback) and the path-segment
  matching rule.
- `std/reactive.md` — two bare `proposal/…` references made links.
- `appendix/errors.md` — `Json` into the resource-field derive entry; the
  view-across-`await` entry restated on suspension; the misspelled-field
  note and its quickfix; new entries for the argument/field count messages,
  the three "no value came back" shapes, `expected ';' to end this
  statement` and `unclosed '(' …`, plus a paragraph on parser recovery
  heading the Syntax section; a new **Panics** section for the backed-enum
  trap and `mount: no element with id '…'`.
- `appendix/gotchas.md` — six entries from v0.33.0/v0.34.0: guarded last
  arm, the backed-enum trap, suspension-not-`await`, the overwrite rule,
  the `.mjs` artifact rename (a breaking change to filenames), the
  module-level-`await` refusal, and "do not read your own build by hand".
- `appendix/glossary.md` — a **backed enum** entry; **copy** now names
  `return` and says the signature decides.
- `README.md` — `tour/projects.md` into the Tour table, `appendix/cli.md`
  into the Appendix bullet, the `process` row's module list completed.
- `tour/projects.md` — the cross-page anchor repointed at
  `platforms.md#full-stack-packages`.
- `guide/routing.md`, `guide/walkthrough.md` — pointers up the ladder from
  the rung each page teaches (see §3 for why neither moved further).
- `editors/vscode/syntaxes/vilan.tmLanguage.json`,
  `vilan/docs/theme/vilan.js` — §4.
- `AGENTS.md` — the keyword-drift note pointed at `lexer.rs`, a file that
  does not exist (it is `lexing.rs`); corrected, and widened to name the
  two neighbouring lists that drift the same way.
- `editors/vscode/package.json`, `editors/vscode/README.md` — both promised
  inlay hints on **parameters**. `Document::inlay_hints` iterates
  `program.variables` only — `let`/`mut`, `for` binders, comprehension
  binders — and skips anything annotated; `program.parameters` is a
  separate map it never reads, and the pinned test is called
  `inlay_hints_show_inferred_types_only`. A parameter's type is on hover,
  not as a hint. Both corrected.

### The new page

**`appendix/editor.md`**, added to `SUMMARY.md` beside the CLI reference and
to `README.md`'s Appendix bullet. `documentation.md`'s TOC has no editor
chapter, and the editing-DX arc shipped a large user-visible surface with
nowhere to describe it: `vilan-lsp` was named twice in the whole book, both
times in passing, and the only behaviour documented anywhere was
`appendix/cli.md`'s note on `vilan check`'s recovery. The page covers how
the server ships and is found, the feature set, where each kind of
completion applies, the five quick fixes and two source actions by their
exact titles, what happens while you type (parser recovery; why answers
stay in the last-analyzed text's coordinates; the two requests that decline
rather than answer wrong), the six settings — and, deliberately, a section
naming the eight LSP features it does **not** implement, so a reader does
not go hunting for signature help or workspace symbols that were never
there.

## 3. Found and not fixed

### Filed as new backlog items (§D)

**D16 — the glossary's 25 internal cross-references are dead links.**
`appendix/glossary.md` writes each term as `**term**:` bold text and
cross-links between them with `[view](#view)`, `[owner](#owner)` and so on
— 25 such links. mdBook generates anchors from **headings**, not from bold
text, so every one of them resolves to nothing in the published book. The
fix is a shape decision, not a typo: either promote each of the ~45 terms
to a `###` heading (which gets real anchors and a page-scroll sidebar, and
changes how the page reads), or drop the links and let the alphabetical
ordering do the navigating. It predates this audit and is orthogonal to
currency, so it is filed rather than decided inside a currency sweep.

**D17 — nothing gates the three-place keyword rule.** AGENTS.md has said
since the `resource` incident that a keyword lands in the lexer, the
TextMate grammar and the book's highlight.js theme, and that the check is a
mechanical diff rather than a read-through. There is no such diff in the
suite: `crates/vilan-cli/tests/vscode_extension.rs` checks marketplace
metadata only. This audit did the diff by hand and found the keyword axis
clean but two neighbouring lists drifted (§4) — which is the argument for
the gate, since the keywords are the axis people remember to check. A test
asserting that `lexing.rs`'s keyword table, `type_.rs`'s
`SCALAR_PRIMITIVE_NAMES` and `parsing.rs`'s `is_known_attribute_marker` are
each covered by both grammar files would turn a recurring manual audit into
a gate. It is a new test in the Rust suite rather than a docs edit, so this
documentation lane filed it instead of building it.

**D18 — nothing keeps `appendix/editor.md` honest.** The new page (§2) is
the first documentation of the LSP surface, and it is hand-written prose
about a Rust crate: it quotes five code-action titles verbatim, six setting
names, and a list of eight capabilities the server does *not* advertise.
Every one of those is a claim the compiler could change tomorrow with
nothing failing. The docs gate compiles fences; it cannot check that
``Insert `;` `` is still the string `document.rs` produces. A cheap gate
exists in shape — assert each quoted title appears in `vilan-lsp`'s source,
and each setting name in `editors/vscode/package.json` — but it is a new
Rust test, so it is filed with D17 rather than built here. The audit
already found what happens without one: the extension's own README and
`package.json` promised inlay hints on parameters, which `Document::
inlay_hints` has never produced (it iterates `program.variables` only).
Both were corrected in this change; nothing would have caught them.

### Filed already, not re-filed

**E63** already tracks moving `examples/todo` and `examples/walkthrough`
onto the builder. `guide/walkthrough.md` quotes the walkthrough example
verbatim, so its server fence cannot move ahead of the example without the
page lying about the file it says it is quoting. The page gained a
paragraph naming the two rungs above it instead; the fence moves when E63
does.

### Verified against the compiler rather than assumed

Four claims were settled by probe (`cargo build`, then
`target/debug/vilan check` on a scratch package) rather than by reading:

- `5i64` is a hard error naming the rename (`i64` → `i53`) — which is what
  makes §4's grammar finding a bug and not a preference.
- The v0.33.0 closure/method-generic fix is really in: the changelog's own
  example now reports *"variant 'Other::First' does not belong to the
  matched enum"*, so `spec/types.md`'s tracked-gap note was false.
- The argument/field count messages, the "did you mean" note, the three
  no-value-came-back shapes, `expected ';' to end this statement` and
  `unclosed '(': expected a matching ')'` were each quoted from the
  compiler's own output before being written into the error index.
- The backed-enum trap's emitted text was read out of the generated
  JavaScript (`name + ": " + JSON.stringify(value) + " is not one of its
  values"`), not from the changelog.

### The ship-record claim, checked

`fullstack-dx.md` §13.3 says
*"`vilan/docs/guide/{walkthrough,ssr,routing,styling,persistence,services}.md`
were taught the new idiom in the same commit; `docs/std/process.md` carries
`serve_build`, `build_handler`, `is_watching` and the sync read."*

`std/process.md` is true and then some — it carries `LegBuild`,
`check_shell`/`ShellFault`, `Document::of`/`render` and `std::watch` as
well, and is the most current page in the book. `ssr.md`, `styling.md` and
`services.md` are true. `routing.md` and `walkthrough.md` are true of the
sentence they changed and thin around it — each got `build_handler` and
neither got the builder rung. **`persistence.md` is where the claim is
thinnest:** the sweep added a `build_handler` fragment to the middle of the
page and left both sections that actually *teach the boot sequence* — "the
typical server reads the client bundle and shell into memory once at boot",
and a numbered boot list ending in `serve_service(port, protocol, fallback,
on_ready)` — describing the shape the arc had just replaced. A reader
following the guide front to back would have built the old thing.

The lesson is about what "the affected page" means. A grep for the new
symbol finds the page; it does not find the *section* that teaches the
displaced idiom, because that section names none of the new symbols. It
names the old ones — which is exactly what a currency audit, rather than a
same-commit sweep, is for.

## 4. Keyword drift — the three-place check

Diffed mechanically: `lexing.rs`'s `read_identifier` table (32 keywords)
against `editors/vscode/syntaxes/vilan.tmLanguage.json` and
`vilan/docs/theme/vilan.js`.

**The keyword axis is clean.** All 32 appear in all three places,
`resource` included — that scar has healed.

**The lists beside it had not.** Three findings, all fixed:

1. **`i64`/`u64` were highlighted as primitive types** in the TextMate
   grammar, and `i53`/`u53` — the language's actual integers — were absent
   from it. `i64` has been a hard error since the numeric-types rename
   (`unknown numeric suffix 'i64'; 'i64' was renamed to 'i53' …`, confirmed
   by probe), so VS Code was colouring a compile error as a valid type
   while giving the correct spelling no colour at all. Fixed against
   `type_.rs`'s `SCALAR_PRIMITIVE_NAMES`.
2. **`platform` was missing from both attribute lists.** The parser's
   `is_known_attribute_marker` names nine markers; the TextMate grammar had
   eight and `vilan.js` had eight-plus-one-wrong — it listed `macro`, which
   is a `vilan.toml` **section**, never a source attribute. Both lists now
   match the parser's.
3. **`context` was in one grammar and not the other**, and `sync` was in
   neither. Both are contextual — the lexer hands them back as identifiers
   — so both are now matched by position, the way the TextMate grammar
   already handled `context`: `context` only after a closure type's `)`,
   `sync` only after the `(` that opens one. A variable named `context` and
   a type named `Sync` are untouched.

`vilan.js`'s number-suffix regex was checked and is correct and current
(`[iu](?:8|16|32|53)|f32|f64|[fn]` — 53 in, 64 out), which is a fair
illustration of the pattern: the list somebody edited when the rename
landed is right, and the list two hundred lines away in a different file is
the one that rotted.

## 5. The gates (2026-08-19)

STATUS: SHIPPED 2026-08-19 (work order 5, lane `d17-grammar-gate`) — D17,
D18 and D19 built as three mechanical gates; §3's "filed rather than built"
is closed.

**D17 — `crates/vilan-cli/tests/grammar_sync.rs`.** Lives beside
`vscode_extension.rs` because the two grammars are repo-level assets outside
any crate, the CLI crate already pins those (`npm_stub.rs`, `brew_formula.rs`)
and already reads JSON with node. The compiler's lists are read
programmatically, each the table its own consumer uses — which took three
small refactors, all behaviour-preserving: the lexer's `read_identifier`
`match` became a `pub const KEYWORDS: &[(&str, Token)]` table it looks up;
`is_known_attribute_marker`'s `matches!` became `pub const
KNOWN_ATTRIBUTE_MARKERS`; and the analyzer's numeric-suffix `matches!` became
`pub const NUMERIC_SUFFIXES` beside `SCALAR_PRIMITIVE_NAMES` in `type_.rs`.
The grammars are read with node: the TextMate file as JSON (every
`match`/`begin`/`end` under each repository entry, nested), and `vilan.js`
*evaluated* under a stub `hljs` so the checked word lists are the ones the
real highlighter registers (the keyword string is built by concatenation).
Word lists are extracted from the regexes by one rule — every maximal
identifier-shaped run that is neither an escape nor inside a bracket class —
which reads `\b(if|else)\b`, `(?:derive|…)\b` and `(?<=\))\s+(context)\b` as
their words and a shape rule like `\b[A-Z][A-Za-z0-9_]*` as none. Six tests,
each direction of each axis: every lexer keyword in both grammars; nothing in
either grammar's keyword lists the lexer does not know, with a five-entry
allowance for the contextual words (`context`, `sync`, `self`, `Self`,
`void`) each pinned to still lex as an identifier and to still be used by a
grammar; the TextMate primitive alternation equal to the lowercase
non-keyword scalars plus `bool`/`void`/`any`, and every scalar (including
`BigInt` via the PascalCase rule, `null` via the literals) matched by some
rule; `vilan.js`'s number-suffix regex accepting exactly `NUMERIC_SUFFIXES`
on decimal probes and rejecting `i64`/`u64`/`i128`/…; the attribute markers
equal in both grammars, where in the TextMate file *every* regex under
`attributes` that names any marker must name all nine (the opening lookahead
and the inner rule both — `(method|get|set)` names none and is not held).

*Found by the gate and fixed in the same change:* the TextMate grammar's
attribute **opening lookahead** still listed eight markers — §4's fix added
`platform` to the inner rule but not to the lookahead that opens the scope,
so `[platform(…)]` coloured only at a line's start. One word added.

*Not drift, noted:* `vilan.js` has no primitive-type list at all — its `TYPE`
rule is PascalCase-only, and `i32`/`str` are not coloured as types in the
book where VS Code colours them. §4 treated the number-suffix regex as the
theme's primitive surface and the gate does the same; giving the highlight.js
grammar a `type:` keyword group (the highlight.js idiom) is a book-colouring
decision for the owner, not a list drift. The TextMate number
rule accepts any identifier suffix by design (its comment says so), so there
is nothing to hold it to.

**D18 and D19 — `crates/vilan-lsp/src/book_sync.rs`,** a `#[cfg(test)]`
module of the server binary. Not `crates/vilan-lsp/tests/`: `vilan-lsp` is a
bin-only crate, so an integration test could not reach `KEYWORD_DOCS` or the
capabilities; a test module inside the crate can, with `KEYWORD_DOCS` and
`BOOK_BASE` made `pub(crate)` and `initialize`'s capabilities literal
factored into a pure `server_capabilities()` (the one refactor in the LSP).

D19: mdBook's heading-id algorithm is reimplemented (`mdbook_heading_ids`)
and every `page.html#anchor` in `KEYWORD_DOCS` is checked against the page's
headings — no renderer at test time, per `docs.rs`'s renderer-independent
design. The algorithm as verified against mdBook 0.5.4's output is *not* the
"non-alphanumerics → `-`, collapse" shape the work order described: it is
mdBook's `normalize_id` — keep alphanumerics, `_` and `-` (lowercased), turn
each whitespace character into one `-`, **drop** every other character, no
collapsing — over the heading's rendered text (inline code keeps its
characters, so `` `Shared<T>` `` is `sharedt`; `if / else` is `if--else`;
`impl: methods and statics` is `impl-methods-and-statics`), with repeats
suffixed `-1`, `-2`. An `#[ignore]`d test builds the real book into a temp
dir and compares every heading of every page (447 headings, 56 pages, zero
mismatches with mdBook 0.5.4); it runs on `cargo test -p vilan-lsp book_sync
-- --ignored` wherever `mdbook` is on PATH, and is the proof the
reimplementation leans on. `impl`'s link was the one broken entry of 22
unique targets and is fixed (`impl--methods-and-statics` →
`impl-methods-and-statics`; CHANGELOG `tooling`). `vscode_extension.rs` and
`brew_formula.rs` still pin the base URL as literals; `book_sync.rs` ties
`document.rs`'s `BOOK_BASE` to `package.json`'s `homepage`, so the three
agree by test rather than by eye.

D18: the page's claims are read out of `appendix/editor.md` by its own
formatting — the Quick fixes section's tables (double-backticked first cells
are quick-fix titles, bold ones source actions), the Settings table's
backticked names and their Default column, the bold feature names and the
"no …" phrases of "What it does not have", whitespace-flattened so a re-wrap
cannot break a pin — and each is held to the thing it describes: the
`title:` string literals inside every `QuickFix { … }` (document.rs) and
`CodeAction { … }` (main.rs) constructor, matched as templates (a `format!`
hole matches the page's example — ``Import `X` from std::json`` instantiates
`Import `{name}` from {}`), in both directions and in count;
`server_capabilities()` through a claims table of 24 `(phrase, claimed,
predicate)` rows, the phrase pinned to appear on the page; and
`package.json`'s `contributes.configuration` (names equal as sets, literal
defaults equal, `—` meaning the empty or discovery-sentinel default) and
`contributes.commands` (`Vilan: Restart Language Server`). Shape pins
throughout: a section or table that goes missing panics with the page's name
rather than extracting nothing.

*Found by the gate and fixed:* the page opened its quick-fix table with
"Five" over four rows and four `QuickFix` constructors; the count is now a
pinned claim and reads "Four".

**Demonstrated non-vacuous.** Each assertion was driven red by a planted
drift and restored: `resource` removed from `vilan.js`'s keyword string;
`i64` re-added to the TextMate primitive list; the theme's suffix widths
changed `53` → `64`; `yield` added to the TextMate keywords; `platform`
dropped from the theme's marker list; the old two-hyphen `impl` anchor; a
misspelled page name; a server title changed to ``Drop `;` ``;
`hover_provider` set to `None`; a setting renamed on the page; the Insert
row deleted from the page; "Five" restored. Every one failed the intended
test and nothing else.

**Filed, not built (for the tracker).** (i) `bindgen.rs`'s `RESERVED` list
(keywords + primitives + `any bool self void` + four std type names) is a
fourth copy of the keyword and primitive lists, clean today; it could be held
to `KEYWORDS ∪ SCALAR_PRIMITIVE_NAMES` by the same gate through a
`#[doc(hidden)] pub` accessor. (ii) `lexing.rs`'s own unit test
`keywords_classify_and_identifiers_do_not` still carries its hand copy of the
table — left as a deliberate pin of the set, but it no longer needs to be one.
(iii) Whether the book should colour primitive type names (above).
