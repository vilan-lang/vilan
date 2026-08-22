# Playground completion — the completion core's seam for wasm (K9)

> Status: DESIGNED AND BUILT 2026-08-22 (lane k9-playground-completion,
> cycle 27, design-first). The seam was measured before anything moved:
> the helper graph below is the verdict that let the build go ahead. §9
> is the latency record, §10 what shipped, §11 the owner's questions.
>
> Filed from backlog-2026-08-18.md §K9 ("autocomplete is imported, never
> wired") and design-language.md §2.7's K9 entry. This is its own paper
> rather than an editing-dx.md section because nothing here is about the
> *language server's* editing DX: editing-dx.md §18–§19 record what the
> completion engine answers; this paper records where the engine LIVES,
> what the playground retains between calls, and what crosses the wasm
> boundary — architecture, not behaviour. The engine's behaviour is not
> changed by this paper; every LSP completion pin stays where it is.

## 0. The ask, and the verdict

The playground bundles `@codemirror/autocomplete` and registers no
completion source; the comment at `editor.mjs:17-28` already names the
only honest fix — a `complete(…)` export on `vilan-wasm`, plugged in where
`scheduleCheck` round-trips the worker — and refuses the dishonest one (a
keyword list typed on the website side, "a language feature invented on
the wrong side of the fence").

The engine exists. `Document::completion` in `crates/vilan-lsp/src/document.rs`
answers member, lifted-member, `::` path, import-path, element-head, macro-name,
scope and auto-import completion, with E52's live-vs-analyzed offset
discipline and E57's package-tree reads. It is plain Rust over `vilan-core`'s
`Program`: `tower_lsp::lsp_types` is imported once in that file and used only
by `apply_change`, `analyzed_range`, `analyzed_position`, `analyzed_offset` —
none of which is on the completion path. The whole LSP wire mapping is
`to_completion_item` in `main.rs`. The obstacle is purely structural:

- `crates/vilan-lsp/Cargo.toml` declares only a `[[bin]]`. Nothing can call
  `Document` — and depending on the binary crate would drag `tower-lsp`,
  `tokio` and `dashmap` into a `wasm32-unknown-unknown` build, which cannot
  take them.
- The completion gatherers are methods on `Document`, interleaved with
  hover, definition, references and rename in an 11k-line file, sharing
  ~40 navigation helpers. Whether completion can be lifted out depends on
  which of those helpers it actually pulls — the question §2 measures.
- `LineIndex` is forked: `crates/vilan-lsp/src/line_index.rs` returns
  `lsp_types::Position`; `crates/vilan-wasm/src/line_index.rs` (whose header
  calls the fork deliberate and says "keep the two in step") returns a
  plain line/character pair.
- `vilan-wasm` keeps no state between calls: `compile_program_for` re-runs
  `boot()` + overlay registration + a full `analyze_source` per call and
  drops the `Program`. A completion request cannot afford a full analysis
  per keystroke, so something must be retained — under `lib.rs`'s
  one-compile-at-a-time / instance-is-recycled constraint.

**Verdict: the seam is clean; build.** Completion's transitive helper set
shares nine small primitives with hover (196 lines) and nothing of hover's
own composition; `definition_of`, `occurrences` (rename/references) and
`semantic_tokens` do not come along at all. The stop condition the brief
named — "the helper graph pulls hover/definition along" — did not fire.

## 1. Where completion lives today (verified 2026-08-21/22)

`Document::completion(&self, offset: usize) -> Vec<Completion>`
(`document.rs:3376`) reads six `Document` fields:

| field | what completion reads it for |
|---|---|
| `program: AnalyzedProgram` | every `Program` lookup (scopes, entities, types, impls, modules) |
| `line_index: Arc<LineIndex>` | the LIVE text — the trigger scan (`.`, `?.`, `::`, `[`, the partial identifier), `in_element_head`'s re-parse, `insert_import`'s edit |
| `analyzed_index` (via `analyzed_offset`/`analyzed_text`) | E52's live→analyzed mapping; `doc_comment_of`'s read of the entry text |
| `entity_spans: Vec<(usize, usize, Id)>` | `entity_at` / `scope_at` |
| `platform_requirements: HashMap<Id, String>` | `function_target`'s "carries a requirement" test (key presence only) |
| `import_roots: Option<ImportRoots>` | E57's package-tree reads for `import std::…` |

`live_edits` / `live_offset` are NOT on the path (they serve the inlay
viewport filter); `to_analyzed_offset` is a composition of the two indices
(`analyzed_index.offset(line_index.position(live))`), which is what makes
the "analyzed-offset fn" of the brief a derived thing rather than an injected
closure — §3.

The value types (`Completion`, `CompletionKind`, `SnippetInsertion`,
`AutoImport`), the two tables (`KEYWORD_DOCS`, `CONSTRUCT_SNIPPETS`) and
`keyword_lexeme` are plain Rust. `manifest_completion.rs` (1134 lines, zero
`lsp_types`) is the existing proof that a pure completion module works in
this crate.

## 2. The helper dependency graph, measured

Method: a call-graph walk over `document.rs`'s non-test code (every `fn` at
indent 0 or 4, bodies delimited by the next item, comment lines stripped
before scanning for `name(` references to other defined names), transitive
closure from each public query. Numbers are function-body lines.

| query | functions | lines | shares with completion |
|---|---|---|---|
| `completion` | **68** | **1235** | — |
| `hover` | 21 | 550 | 9 fns / 196 lines |
| `definition` | 8 | 167 | 5 fns / 48 lines |
| `references` (and rename, via `occurrences`/`target_of`) | 9 | 222 | 3 fns / 13 lines |
| `semantic_tokens` | 5 | 226 | 3 fns / 9 lines |
| `quickfixes` | 8 | 214 | 2 fns / 35 lines (`origin_roots`) |

The nine primitives completion shares with hover, with what each needs:

| primitive | lines | reads |
|---|---|---|
| `span_of` | 3 | `program.span_map` |
| `source_call_subject` | 14 | `program.function_calls`, `context_erased_subjects` (E75) |
| `definition_name_span` | 21 | `program` declaration tables |
| `doc_comment_of` | 44 | the analyzed entry text; `util::read_source` for another file (overlay-then-disk) |
| `entity_at` | 7 | `entity_spans` |
| `function_target` | 52 | `program.functions`/`external_functions`, `platform_requirements` (key presence) |
| `hover_label` | 49 | `program.expr_types`, `entity_map`, `context_hidden_parameters` (E73/E75) |
| `analyzed_text`, `as_ref` | 6 | accessors |

What stays in the LSP: hover's own composition (`hover`, `compose_hover`,
`keyword_hover`, `binding_hover`, `member_hover`, `const_value_label`,
`type_declaration_target`, `parameter_signature`, `clamp_preview`,
`offset_touches_a_token`, `type_reference_at` — 12 functions, 354 lines);
`definition`, `definition_of`, `type_reference_at` (119 lines);
`occurrences`, `target_of`, `field_decl_at`, `type_reference_target`;
`semantic_tokens`, `inlay_hints`, `document_symbols`, `published_diagnostics`,
`organize_import_edits`, `import_leaf_is_used`, `quickfixes`,
`import_candidates`; and everything about project context
(`resolve_project_context`, `resolve_dependencies`, `ManifestProblem`) and
the live/analyzed snapshot bookkeeping (`set_text`, `apply_change`,
`adopt_analysis`, `compute_retained_tail`, `live_offset`).

So the extraction moves: 58 completion-only functions (1007 lines), the
nine shared primitives (196 lines), the value types and tables, and the
`ImportRoots` struct with `origin_roots` (which `import_candidates` in the
LSP keeps calling). Hover, definition and rename are left where they are,
calling the shared primitives through the new crate.

## 3. The seam — `crates/vilan-ide`

A new workspace crate, `vilan-ide`, depending on `vilan-core` only. It is
the editor-facing half of the compiler that is NOT a protocol: what the
language server and the playground both need, with each protocol's types
mapped at its own edge. (Name: the brief suggested `vilan-completion`; this
crate also owns the unforked `LineIndex` and the navigation primitives hover
reads, neither of which is completion — `vilan-ide` says what it is. Owner
question §11.1.)

```
crates/vilan-ide/src/
  lib.rs          the crate doc, re-exports
  line_index.rs   LineIndex { new, position, offset, range, text } — §4
  analysis.rs     Analysis<'a, 'src> + the navigation primitives of §2
  completion.rs   the value types, the tables, the gatherers, the insertion rule
```

### 3.1 `Analysis` — the brief's `CompletionContext`

```rust
pub struct Analysis<'a, 'src> {
    /// The analyzed program.
    pub program: &'a Program<'src>,
    /// The text the program was analyzed from: the coordinate space every
    /// program span and offset lives in.
    pub analyzed: &'a LineIndex,
    /// The text being edited. The same index as `analyzed` when nothing has
    /// been typed since the analysis landed.
    pub live: &'a LineIndex,
    /// `(start, end, id)` for every entry-file entity with a real span —
    /// `entity_spans(program)`, computed once per analysis.
    pub entity_spans: &'a [(usize, usize, Id)],
    /// `platform_color::requirements(program)`, computed once per analysis.
    pub platform_requirements: &'a HashMap<Id, String>,
    /// What an import path can reach; `None` when the analysis resolved no
    /// package tree (the LSP's degraded internal-error document).
    pub import_roots: Option<&'a ImportRoots>,
}
```

A struct of references, built per query by whoever owns the analysis. The
brief's "analyzed-offset fn" is not a field: `to_analyzed_offset(live)` is
`analyzed.offset(live.position(live_offset))`, E52's rule exactly, and both
indices are already here — injecting a closure would be a second way to say
the same thing. The LSP's `Document` builds one from its own fields
(`fn analysis(&self, program) -> Analysis<'_, 'static>`); the wasm builds
one from its retained handle (§5). `Analysis::completion(offset)` is
`Document::completion` moved verbatim; the navigation primitives are its
methods, with `program` taken from `self` rather than passed along.

Two functions that the LSP's analysis constructor open-codes move beside
it so the two front-ends cannot compute them differently:
`entity_spans(program)` (the `Expr::Void` exclusion of editing-dx.md §3.9
included) and — already core's — `platform_color::requirements`.

### 3.2 The insertion rule moves too

`to_completion_item`'s `call_insertion` (the `vilan.completion.functionCall`
shaping: `name(${1:a}, ${2:b})$0`, `name($0)`, `name()`, and the plain-text
degradation without snippet support) is a pure string rule that the wasm
export needs with the same answers. It moves to `vilan-ide` as
`call_insertion(label, parameters, mode, snippet_support) -> Option<InsertText>`
with `InsertText { text, is_snippet }`; `CompletionFunctionCall` (the
setting's value type) moves with it. `main.rs` keeps a thin `call_insertion`
that maps `is_snippet` to `InsertTextFormat`, so its eight pins stay as
written. The playground fixes the mode at `Full` with snippet support on —
the LSP's defaults — because it has no settings surface.

### 3.3 What `vilan-lsp` looks like after

- `Cargo.toml` gains `vilan-ide = { path = "../vilan-ide" }`; it stays a
  `[[bin]]` — nothing needs to call into the server.
- `document.rs` loses the moved code; `Document::completion` is
  `self.analysis(program).completion(offset)`; hover/definition call
  `self.analysis(program).hover_label(id)` and friends; `span_of` /
  `source_call_subject` are used from the crate; `KEYWORD_DOCS`, `BOOK_BASE`,
  `keyword_lexeme`, `CONSTRUCT_SNIPPETS` are imported (`book_sync.rs` too).
- `to_completion_item` is unchanged on the wire: same kinds, same
  `insert_text`/`insert_text_format`, same `sort_text` bands (`|tier` for
  auto-imports, `~` for snippets), same `additionalTextEdits`.
- Every completion pin in `document.rs` and `main.rs` keeps driving
  `Document::analyze` + `document.completion(offset)`; none moves.

## 4. The `LineIndex` unfork — the first concrete win

One implementation, in `vilan-ide`: `LineIndex::new(text)`,
`position(byte) -> Position { line, character }` (UTF-16 units, clamps past
the end, never panics inside a multi-byte character), `offset(Position) ->
byte`, `range(&Span) -> (Position, Position)`, `text()`. The tests are the
union of both forks' — the wasm copy's degrade pins (inside-a-multibyte-char,
out-of-range clamp) and the LSP's.

- `crates/vilan-lsp/src/line_index.rs` becomes a newtype over it, mapping
  to `lsp_types::Position`/`Range` at the edge and keeping the method names,
  so its twelve call sites (`main.rs`, `publish.rs`, `document.rs`) do not
  change.
- `crates/vilan-wasm/src/line_index.rs` is deleted; the wasm uses the shared
  type directly (its `Position` already had the shared shape).

## 5. What `vilan-wasm` retains across calls

```rust
struct Retained {
    /// The interned entry text the program borrows — the key.
    text: &'static str,
    platform: Platform,
    program: Program<'static>,
    analyzed: LineIndex,
    entity_spans: Vec<(usize, usize, Id)>,
    platform_requirements: HashMap<Id, String>,
    import_roots: ImportRoots,
}
thread_local! { static RETAINED: RefCell<Option<Retained>> }
```

- **Written** by every `compile_program_for` (both the page's `compile` and
  its `check`): the analysis that just ran replaces whatever was retained,
  whether or not it compiled clean — a program with type errors still
  answers completion, and a parse that produced no tree retains nothing.
  `analyze_source` already leaks the tree and the text for `'static`, so
  holding the `Program` costs no new leak; it is dropped when replaced.
- **Keyed** by the interned entry text (`interned_entry`'s `&'static str`)
  and the platform. `complete(source, …)` compares the live `source` to
  `retained.text` to decide between the identity mapping and E52's
  line/character mapping; it never compares against anything older.
- **Read** only by `complete`. `complete` never analyzes: with nothing
  retained it answers an empty list. The page issues a `check` on every
  worker `ready` and ≤400 ms after every edit, so the retained program is at
  most one debounce plus one analysis behind the buffer — the language
  server's own situation, which E52's mapping exists for.
- **Invalidated** by the next `compile_program_for` (replaced) and by the
  instance dying: the page recycles the worker after `RECYCLE_AFTER = 32`
  compiles/checks and after any crash, and a fresh instance has nothing
  retained until its first `check` lands (~400 ms after `ready`).
- **Single-flight**: the worker is single-threaded and answers messages in
  order, so `complete` never runs concurrently with an analysis, and a
  `complete` posted while a `check` is in flight simply waits behind it.
  `complete` leaks nothing and counts nothing toward the recycle budget —
  like `format`, pure over state the compiles already paid for — so the
  page's `inFlight` gate and `compileCount` do not see it.

`thread_local!` rather than a `static Mutex`: the instance is one thread,
and the native tests (which share one process and serialize on a mutex)
each see exactly their own thread's retention.

## 6. The export

`complete(source: String, line: u32, character: u32) -> Vec<CompletionItem>`.

The cursor is a line/character pair, not a whole-document offset: the wire
already speaks "zero-based line, UTF-16 character within it" in the
diagnostics direction, and the page computes it in two lines from
CodeMirror's `doc.lineAt(pos)`. The sketch in `editor.mjs` said
`complete(source, offset)`; a UTF-16 document offset would have been a
second unit on one wire.

One item, flat (the shape `Diagnostic` already uses — `wasm-bindgen`
structs with `getter_with_clone`; `Vec<CompletionItem>` comes back as a JS
array):

| field | type | meaning |
|---|---|---|
| `label` | string | the name shown and matched against the typed prefix |
| `kind` | string | `macro` `function` `method` `field` `struct` `enum` `enum_variant` `trait` `variable` `module` `keyword` `snippet` — the page maps these to CodeMirror icon types |
| `detail` | string? | the signature / type / auto-import module, as the LSP's `detail` |
| `documentation` | string? | the `///` first paragraph |
| `insert` | string | what accepting inserts: the bare label, the call shape, the snippet body |
| `is_snippet` | bool | `insert` carries `${n:…}` tab-stops (LSP syntax; the page rewrites bare `$0` to CodeMirror's `${0}`) |
| `boost` | i32 | the LSP's `sort_text` bands as a CodeMirror `boost`: in-scope `0`, auto-import `-(1 + tier)`, construct snippet `-9` |
| `import_*` | line/character ×2 + text, optional | the auto-import edit (E54c), in the LIVE text's coordinates, applied with the insertion |

The word to replace is the page's to compute (`matchBefore(/[A-Za-z0-9_]*/)`,
the same byte class as `is_identifier_byte`) — the LSP leaves it to the
client the same way when no `textEdit` is given.

Two things `Document::completion` needs that the wasm must supply:

- `import_roots`: `ImportRoots { std: embedded_std_spec(), pkg_root:
  "/project", dependencies: [] }` — the same spec the analysis ran with.
- **A module listing with no filesystem.** `import std::|` enumerates
  modules with `analyzer::modules_in_root`, which was `read_dir` only;
  `module_source_file` and `module_importables` already consult the overlay.
  `modules_in_root` now also lists the overlay's entries directly under the
  root (flat `name.vl` or `name/lib.vl`), deduplicated and still sorted —
  the loader's own existence rule (`resolve_module_file` asks
  `document_overlay_contains`) applied to listing. In the editor this means
  an unsaved new sibling file completes as `import pkg::<name>` before it
  is saved, which is the overlay's whole point. Pinned in core.

## 7. The website wiring

- `worker.js`: `canComplete = typeof glue.complete === "function"` rides
  the `ready` message like `canFormat`; a `complete` action answers
  `{ kind: "completed", id, items }`. Its block is separate from the
  diagnostic map (lane e80 edits that; the union is mechanical).
- `editor.mjs`: `autocompletion` is imported at last; a
  `CompletionSource` posts `{ action: "complete", id, source, line,
  character }` and resolves on the matching `completed` message; a recycle
  rejects every pending request as `null` (no list) rather than leaving a
  promise hanging. Requests carry no `inFlight` bookkeeping (§5). Items map
  to `{ label, type, detail, info, apply, boost }` — `snippetCompletion`
  for `is_snippet`, a two-change dispatch for an auto-import, the plain
  text otherwise. The completion tooltip takes the K10 slots the lint
  tooltip already wears (`--code-*`), no new tokens.
- The header comment that scoped this fix is rewritten to say what is wired.
- `scripts/smoke-playground.mjs` gains a `complete` probe, guarded on the
  export's presence (the deploy fetches the latest *release*, which will
  not carry the export until the next train).

## 8. What does not change

`Document::completion`'s answers, byte for byte — the gatherers move, they
are not edited. The LSP wire (`to_completion_item`). The corpus goldens
(nothing on this path touches the transformer). The wasm's `compile`,
`compile_for`, `format`, `version` contracts and the leak accounting the E23
pins hold (`complete` adds no leak site). The recycle policy.

## 9. Latency

**Method.** The walkthrough app (`vilan/examples/walkthrough`, the docs'
team-notes app) folded into one browser-leg file — `notes.vl`, `routes.vl`,
`views.vl` and `client.vl` verbatim minus their `pkg::` imports, plus the
store's `[service(NotesClient)]` declaration with in-memory bodies in place
of the SQLite ones, so the generated `NotesClient` is the real one — 407
lines, 11.7 KB, compiles clean (71 KB of JS). The release wasm
(`wasm-release` profile, `wasm-bindgen --target web`, the pair the deploy
ships) loaded under node 24 exactly as `scripts/smoke-playground.mjs` loads
it; each figure is the median of 20 calls after one warm-up compile, on the
2026-08-22 tree. "Worker round trip" is a `worker_threads` worker answering
the page's `complete` message shape — post the whole buffer and the cursor,
receive the plain items — which is what a keystroke pays before CodeMirror
renders; the browser's `postMessage` is the same structured clone.

| site | in-process `complete` | items |
|---|---|---|
| member: `.` just typed after `client` (stale text — the real keystroke case, E52's mapping) | **2.6 ms** | 10 |
| member, same text as analyzed (identity mapping) | 2.6 ms | 10 |
| import path: `import std::` (overlay listing + surface) | 2.6 ms | 71 |
| scope: a bare position inside `screen` (names + keywords + snippets + auto-imports) | 51 ms | 131 |

Worker round trip, member case: **2.9 ms** median (min 2.7, max 6.1).
For scale: the page's `check` on the same program — the full analysis it
already pays per 400 ms debounce — is 150 ms warm and 440 ms cold.

So a `.` costs the keystroke ~3 ms end to end; the page never waits on an
analysis for it. The one slow shape is the bare scope position, and it is the
engine's own cost, not the playground's: `auto_import_completions` runs
`formatter::insert_import` per surviving candidate (up to the cap of 20),
and each call parses the whole buffer; `entity_completion` reads a std
declaration's `///` doc through `read_source`, which clones the module's
text per candidate. The language server pays the same today. Parsing the
buffer once per request would take the scope case to the member case's
cost; it is a change to the E54c/WO-3 engine with its own pins, filed under
§11.5 rather than folded into a move that promised to move code verbatim.

Not measured: a real browser's paint. No browser is reachable from this
lane's environment; the figures above end where the worker's reply is in
the page's hands, and the render that follows is CodeMirror's own popup.

## 10. What shipped

**Compiler repo**, branch `k9-playground-completion` off `next`:

- `crates/vilan-ide` (new): `line_index.rs` (the one `LineIndex`,
  `Position`, eight pins — both forks' plus the inbound direction's clamp
  and a UTF-16 round trip); `analysis.rs` (`Analysis`, `entity_spans`,
  `entity_at`, `span_of`, `source_call_subject`, `signature_label`,
  `call_parameter_names`, and the shared primitives as methods);
  `completion.rs` (the value types, `KEYWORD_DOCS`, `CONSTRUCT_SNIPPETS`,
  `keyword_lexeme`, `ImportRoots`, the gatherers verbatim, the free
  helpers, `CompletionFunctionCall`/`InsertText`/`call_insertion`).
- `crates/vilan-lsp`: `document.rs` lost 1,859 lines; `Document::completion`
  delegates; `analysis(program)` builds the seam; hover/definition call the
  primitives through it; `line_index.rs` is the 60-line newtype;
  `main.rs`'s `call_insertion` wraps the shared rule; `book_sync.rs`
  imports the tables. `cargo test -p vilan-lsp`: 342 passed, 0 failed, 3
  ignored (the pre-existing three) — every completion pin drives the moved
  code through the unchanged harness.
- `crates/vilan-core`: `modules_in_root` lists the overlay (§6);
  `document_overlay_paths` (crate-private). Pin
  `an_overlay_module_lists_under_its_root` in `module_resolution.rs` —
  plant-proven red with the overlay loop disabled.
- `crates/vilan-wasm`: `line_index.rs` deleted; `Retained` + the
  thread-local; `compile_program_for` split into the retention and `emit`;
  `complete_program`; `CompletionItem`/`ImportEdit`; the `complete` export
  and its `wasm_bindgen` struct in their own block. Seven pins in
  `tests/compile.rs`, each plant-proven red with the targeted binary:
  `member_completion_answers_from_the_retained_analysis` (retention
  disabled), `import_path_completion_enumerates_the_embedded_toolchain`
  (the overlay listing disabled), `construct_snippets_and_call_shapes_come_back_as_snippets`
  (the snippet band dropped to 0), `a_stale_buffer_maps_through_line_and_character_not_bytes`
  (identity mapping forced — it offers `Point`'s `x` for `b`'s `Other`),
  `completion_before_any_compile_is_empty_and_an_out_of_range_position_clamps`,
  `completing_leaks_nothing` (a planted `Box::leak` + tally in `complete`),
  `an_auto_import_edit_is_positioned_in_the_live_text` (the edit mapped
  through the analyzed index — its first form was vacuous under that plant
  and was tightened to the exact line/character per edit shape before it
  went red).
- Records: this paper; `design-language.md` §2.7 K9 and §2.8; `editing-dx.md`
  §20; `CHANGELOG.md` (tooling); `proposal/README.md`. Scripts:
  `cut-release.sh`'s `RELEASE_FILES` and the notices test's workspace list
  learn the crate.
- Corpus goldens untouched; no new diagnostic head (no ledger row).

**Website repo**, branch `k9-playground-completion` off `main`:

- `playground/worker.js`: `canComplete` on `ready`; the `complete` action
  (its own block, away from the diagnostic map lane e80 edits); items
  flattened to plain objects and freed.
- `playground/editor-src/editor.mjs`: `autocompletion` registered with
  the worker-backed source (§7); the request map; `abandonCompletions` on
  recycle; the popup theme on the `--code-*` slots; the header comment
  rewritten. `playground/editor.js` rebuilt (the orchestrator regenerates
  it at merge).
- `scripts/smoke-playground.mjs`: the guarded completion probe (claim 4).
  Run against the freshly built wasm: every example ok, `completion: ok
  (12 candidates after \`count.\`)`.

## 11. Open questions for the owner

1. **The crate's name.** `vilan-ide` (this paper) vs the brief's
   `vilan-completion`. The crate owns the line index and the navigation
   primitives as well as completion; renaming is a one-commit change.
2. **Should `complete` analyze on a cold instance?** Today it answers
   empty until the first `check` lands (≤ ~400 ms after `ready`). Analyzing
   on demand would cost a full analysis on the keystroke and a leak against
   the recycle budget; the design says no, and the first-keystroke gap is
   the price. Reversible.
3. **Keyword and snippet noise.** The engine offers every keyword (32) and
   four construct snippets at every scope position, which the LSP client
   filters by prefix. CodeMirror does the same filtering, but with fuzzy
   matching; if the popup reads busy in practice, the page can drop
   `keyword` items (a presentation filter, not a language decision) — the
   export still carries them.
4. **When the popup opens.** As built: on a typed identifier character,
   on `.`/`:` (the language server's own trigger characters), and on
   Ctrl-Space — not after a space or a newline, which would float the
   keyword list after every token. VS Code opens on every character.
   Taste; one line in `vilanCompletions`.
5. **The scope position's 51 ms** (§9). `insert_import` parses the buffer
   once per auto-import candidate; parsing once per request is a small
   engine change (the E54c path) that would also speed the language server.
   Recommend filing it; not done here because this lane moved the engine
   verbatim.
