# AGENTS.md — briefing for coding agents

Read this fully before touching code. `CLAUDE.md` states the contract (correctness over
speed of delivery, refactor-first, proven-before-implemented, root causes over
symptoms); this file is the map and the accumulated scar tissue. Where the two seem to
conflict, `CLAUDE.md` wins.

## The lay of the land

Rust workspace, six crates, plus the language's own tree:

- `crates/vilan-core` — the whole compiler as a library. Pipeline order: `lexing.rs` /
  `token.rs` → `parsing.rs` (a handwritten recursive-descent frontend; replaced
  chumsky 2026-07-22, `proposals/projects/vilan/proposal/frontend.md`) → AST in `node.rs` → macro
  expansion in `macros.rs` (with `interpreter.rs`, the native evaluator that
  must stay behaviorally equivalent to emitted JS) → `elements.rs`
  (element-syntax desugar) and `lift.rs` (the expression-lifting rewrite;
  both hooked at every parse entry — `lib.rs`, the CLI, the module loader,
  macro expansion; the formatter deliberately receives raw, un-lifted trees)
  → `analyzer.rs` (type solving + the inferred effects: `async_infer.rs`,
  `platform_color.rs`, `context.rs`, `call_graph.rs`, `const_eval.rs`) →
  `transformer.rs` (JS emission). Shared type machinery in `type_.rs`;
  diagnostics in `error.rs` — there is a house diagnostics standard, so match
  the shape of existing messages; `formatter.rs` is `vilan fmt`. Also in this
  crate: `bindgen/` (`vilan bindgen`, generating `external` bindings from
  `.d.ts` files, `proposals/projects/vilan/proposal/bindgen.md`) and `leak_tally.rs` (per-site leak
  instrumentation the test suite reads — not a compile stage).
- `crates/vilan-cli` — the `vilan` binary and the end-to-end suites
  (`tests/corpus.rs`, `cancellation.rs`, `rpc_http.rs`, `streaming.rs`,
  `transport_robustness.rs`, …).
- `crates/vilan-lsp` — the language server.
- `crates/vilan-ide` — the editor-facing queries the language server and the web
  playground SHARE (K9, `proposals/projects/vilan/proposal/playground-completion.md` §3): the line
  index, the completion engine, and the navigation primitives it reads. It depends on
  `vilan-core` and nothing else on purpose, so it builds wherever core does —
  including `wasm32-unknown-unknown`, where the language server's tower-lsp/tokio
  stack cannot follow. A completion behavior belongs here, not in `vilan-lsp`, whose
  `line_index.rs` is only a newtype speaking `lsp_types` at the protocol edge.
- `crates/vilan-embedded-std` — embeds the std source into the binary.
- `crates/vilan-wasm` — the compiler as a WebAssembly module; the web
  playground's engine (`proposals/projects/vilan/proposal/web-playground.md`). The compile logic is
  plain Rust tested natively on the host; the `wasm_bindgen` layer at the
  bottom is a thin type-conversion shim, not where behavior lives.
- `vilan/std/src/*.vl` — the standard library, written in vilan. std loads as its own
  package with root-scoped module resolution.
- `vilan/test/` — the corpus: `.vl` programs with **byte-identical** `.mjs` goldens.
- `vilan/docs/` — the user-facing book + spec; every fenced example compiles.
- The `proposals` sibling repo (`proposals/projects/vilan/proposal/…`, workspace-relative —
  N15 moved the design memory out of this tree) — design documents. Semantics
  are settled there **before** code; the proposal named in your work order is
  the spec for your change.

## Definition of done (the gates)

1. **Full suite green by exit code.** `cargo nextest run --workspace` is the gate
   (`cargo test --workspace --no-fail-fast` is a correct, slower equivalent —
   `CLAUDE.md` §"Running the suite"); it must exit 0. Never judge success by
   grepping output: a piped grep masks the status, and a test target that fails
   to *compile* prints no `test result:` line at all. Capture the exit code
   explicitly — redirect and check the runner's own code, never pipe through
   `grep`/`head`/`tail` (`cargo nextest run --workspace > suite.log 2>&1; echo
   $?`) — and report that line verbatim.
2. **Corpus byte-identical** (`cargo test -p vilan-cli --test corpus`) unless the work
   order says otherwise. If an *existing* golden changes: stop and report — never
   regenerate. New goldens require rebuilding the debug binary first (`cargo build`);
   a stale binary silently writes wrong goldens.
3. **Docs compile** (`cargo test -p vilan-core --test docs`), and any change to std, a
   framework, or the language updates the affected `vilan/docs/` page in the same
   change-set.
4. **Per-case pins.** Every behavior added or changed gets its own tests in
   `crates/vilan-core/tests/inference/` (`assert_compiles`,
   `assert_compiles_and_runs`, `assert_fails`) — one pin per case, including the edge
   cases (multi-parameter, nested, mixed, ordering-sensitive), not one representative
   example. A known-but-unfixed gap is pinned `#[ignore]` with a comment saying why —
   and the reason must **lead with its tracker item id** (`#[ignore = "C13: …"]`), so
   the defect lives in the tracker and not in a test attribute. A gate enforces it
   (`crates/vilan-cli/tests/ci_ignored_pins.rs`): the id is one family letter and one
   to three digits, and an ignore that is deliberately NOT a bug — cost, a missing
   tool — is added to `DELIBERATE_NON_BUG_IGNORES` by its exact reason string.
   B145 split the old single 69k-line `tests/inference.rs` into subject modules of
   ONE binary: `main.rs` declares them, the harness is `support.rs`, and a new pin
   goes in the subject module that owns its area (a new subject is a new `mod`).
   The command is unchanged: `cargo test -p vilan-core --test inference`.
5. **`cargo fmt` after every Rust change.** It may reformat neighboring code —
   expected and desired. 4-space indent in Rust; full variable names (`parameter`,
   never `p`).
6. **`cargo clippy --workspace --all-targets -- -D warnings` before you call a Rust
   change done** — the exact CI leg (backlog N21), and it denies rustc's warnings
   too, so a helper you left behind is a failure rather than a log line. Exceptions
   live in ONE place, the root `Cargo.toml`'s `[workspace.lints.clippy]`, each with
   its reason; a per-site `#[allow]` carries a `reason = "…"`. Both this and
   `cargo fmt` run under `rust-toolchain.toml`'s pin, which is what makes your
   answer and CI's the same answer. A dependency change also owes
   `cargo audit --deny unsound` (its own CI leg) alongside the notices gate.

## Invariants and scar tissue (each of these has bitten before)

- **A new codegen helper in `transformer.rs` needs a matching `interpreter.rs` arm in
  the same change**, or the macro/native equivalence gate breaks.
- **Scalar-view classification goes only through `is_scalar_view_pointee` /
  `SCALAR_PRIMITIVE_NAMES`** (`type_.rs`). `bool` is an enum special-case that must
  appear in *every* view-pointee predicate, analyzer and transformer both — three
  drift sites have shipped real miscompiles.
- **Adding a variant to a core enum (`Type`, `Expr`, node kinds…) requires auditing
  every `_ =>` catch-all** that now silently mistreats it. Prefer exhaustive matches.
- **Never special-case a checker to quiet a pattern it rejects.** If legitimate std,
  corpus, or docs code trips a new check, that is a semantics-level event: stop and
  report it.
- **`.vl` probe files outside a package resolve no std** (no `Some`/`None`, no
  imports). Put probes in `vilan/test/` or a scratch directory carrying a
  `vilan.toml`.
- **Writing vilan:** match arms need a trailing comma even after `{}` block bodies;
  pattern bindings use `let` (`Some(let x)`); `.vl` indentation is tabs — the
  formatter (`target/debug/vilan fmt`) is authoritative.
- **std `.vl` files must not dispatch macros at world-load.** Derives carry their own
  imports and can leak them into the deriving module — never depend on a leaked
  import.
- **Generic-inference traps:** a trait bound can fail to propagate through the second
  parameter of a two-parameter generic call (restructure toward single-parameter
  shapes); struct-literal fields do not direct generic-call inference — annotate via a
  `let` binding.
- **Numerics:** the JS-backed integers are `i53`/`u53` (a ±2^53 contract); unknown
  numeric suffixes are hard errors.
- **A new keyword lands in THREE places — and two of them are generated** —
  the lexer (`lexing.rs`, whose keyword table is the `KEYWORDS` const
  `read_identifier` looks up), the TextMate grammar
  (`editors/vscode/syntaxes/vilan.tmLanguage.json`), and the book's
  highlight.js theme (`vilan/docs/theme/vilan.js`). The `resource` keyword
  shipped with only the first and was caught twice, days apart. The same
  drift reaches the **primitive-type**, **attribute-marker**,
  **numeric-suffix** and **operator** lists that sit beside the keywords in
  both grammars (`SCALAR_PRIMITIVE_NAMES` and `NUMERIC_SUFFIXES` in
  `type_.rs`, `KNOWN_ATTRIBUTE_MARKERS` in `parsing.rs`,
  `TWO_CHARACTER_OPERATORS` in `lexing.rs` are the sources of truth); D15's
  audit found `i64`/`u64` still highlighted as valid types a release after
  they became a hard error. Since E91 the grammars' word-list halves are
  GENERATED from those tables: `crates/vilan-cli/tests/grammar_sync.rs`
  byte-holds every generated fragment — and what each grammar actually
  registers — to the compiler's tables on every suite run. A new keyword is a
  `KEYWORDS` row + a `Token` variant + a role row in grammar_sync.rs's
  `KEYWORD_ROLES`; then
  `VILAN_REGENERATE_GRAMMARS=1 cargo test -p vilan-cli --test grammar_sync generated`
  rewrites both grammars in place. Never hand-edit a generated fragment —
  only the structural rules (strings, elements, captures) are hand-written.
  Its sibling `crates/vilan-lsp/src/book_sync.rs` holds the LSP's 32
  keyword-hover deep links to the book's headings and
  `docs/appendix/editor.md` to the server's code-action titles, capabilities
  and settings (D18/D19).
- **`serve_build`'s content-type table is GENERATED too** — the third fragment in
  this tree that must never be hand-edited. The rows in `content_type_of`
  (`vilan/std/src/process/build.vl`) sit between `GENERATED(mime-table)` markers
  and are generated from `crates/vilan-core/tests/mime-table.tsv`, itself derived
  from the `mime-db` registry data by `scripts/regen-mime-table.py`.
  `crates/vilan-core/tests/mime_table_sync.rs` byte-holds the arms to the dataset
  on every suite run and gates the charset rule, the curated extension list, the
  §5.10 fence and the provenance. To add an extension: add it to `CURATED` in the
  script AND in the gate (both, deliberately — neither may move alone), refresh
  the dataset, then
  `VILAN_REGENERATE_MIME_TABLE=1 cargo test -p vilan-core --test mime_table_sync`.
- **A post-`analyze()` pass must be wired into BOTH pipelines** — `lib.rs`'s
  `analyze_source` (tests + LSP) *and* the CLI's duplicated sequence in
  `crates/vilan-cli/src/main.rs` — and verified with a CLI probe, not only an
  inference pin. A pass added to one place ships a check the other silently skips.
- **The panic fence is four sites, and a new pipeline entry point picks a
  side deliberately** (N19). The workspace deliberately does NOT build
  with `panic = "abort"` (the comment above `[profile.wasm-release]` in
  the root `Cargo.toml` records why): the long-lived surfaces fence
  their work in `catch_unwind` so a compiler panic degrades to one
  honest diagnostic instead of a dead process. The four sites:
  `crates/vilan-core/src/lib.rs`'s outer fence in
  `analyze_source_reclaimable` (covers lex/parse/lift — degrades to "no
  program" plus an internal-error diagnostic; before it, a panic
  unwound through `Document::analyze`'s join and aborted the language
  server, B40) and its inner fence in `analyze_source_unfenced` (around
  `analyze` + `post_analysis_passes` — the one that matters for tree
  reclaim); `crates/vilan-lsp/src/main.rs`'s per-request `fenced` seam
  (one bad request answers its fallback instead of locking the user out
  of every LSP feature); and `crates/vilan-lsp/src/document.rs`'s
  analysis thread (a panicked analysis becomes an internal-error
  document, never a re-raise through the join). There is no panic hook
  anywhere: the default hook's stderr write IS the "details are on
  stderr" every fence's diagnostic promises. On wasm32 the fence only
  holds if the target unwinds — the playground's instance-recycle path
  exists as the cover for when it does not, not as an optimization. The
  CLI is deliberately OUTSIDE the fence: `main.rs` imports `analyze`,
  not the fenced `analyze_source`, and joins its compiler thread with
  `.expect("compiler thread panicked")`, so a compiler panic in a
  one-shot `vilan build` double-panics and exits loudly — nothing there
  to keep alive, and a swallowed panic would be a wrong build. The rule
  for a fifth site: a new long-lived entry point into the pipeline (a
  server, a watcher, an editor surface) fences at its own boundary —
  catch, degrade to an honest internal-error answer, details left to
  stderr; a new one-shot entry takes the CLI's stance. Either way,
  write which and why at the site.
- **Every lock RECOVERS from poisoning — no exceptions, and a test
  holds the line** (E97, ruled 2026-08-28: "do the safe thing, prevent
  a poisoned cache"). `.lock()`, `.read()` and `.write()` are followed
  by `.unwrap_or_else(std::sync::PoisonError::into_inner)` at every
  site in every crate's `src/`, because a *caught* panic (the fence
  above is four of them) is exactly how a poisoned mutex outlives the
  request that poisoned it — and one poisoned process-global turns a
  one-shot compiler bug into a language server that answers "internal
  error" for the rest of the session. The defect this closed was
  DRIFT, not absence (one file defended a cache in one function and
  not in its neighbour thirty lines later), so
  `every_lock_in_the_workspace_recovers_from_poisoning`
  (`crates/vilan-core/src/lib.rs`) scans the sources and fails on the
  next `.unwrap()`-ing lock anyone adds; an in-`src` test that must
  acquire a lock says `.expect("…")`. Recovery is only safe because
  these guards are held over whole-value inserts — build the value
  BEFORE you take the lock, or the recovered guard hands the next
  reader a half-written entry. Where a guard is held across
  panic-prone code, clear the entry before rebuilding it rather than
  leaving a stale one behind (`publish.rs`'s `plan_publish` is the
  worked example).
- **Git is scoped to your worktree.** A lane works in its own git worktree under
  `.claude/worktrees/<lane>`, on its own branch off `next`, and commits there —
  `git add <paths>` naming each file explicitly (never `-A`), with a
  `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>` trailer. It never
  pushes, tags, touches `main`/`next`, sets git identity, or regenerates a
  published artifact (goldens, `THIRD-PARTY-NOTICES.txt`, the homebrew seed)
  unless the work order says so. Run git from the worktree root, or via
  `git -C <worktree>`; never share a compound command with `cd` that could
  land in another checkout.

## How to work

- **Probe before you assert.** A five-line `.vl` program run through the freshly built
  binary (`cargo build`, then `target/debug/vilan run …`) beats speculation about
  semantics. Rebuild before trusting output.
- **Root causes.** Fix the general path; a special case that handles one input is a
  smell. If the general fix implies a refactor, say so in your report rather than
  building around the debt.
- **Read the named proposal sections first.** `proposals/tracker/backlog.md`
  (in the `vilan-lang/proposals` sibling repo) is the single planning surface
  (its Now/Next/Later block names what's active); the papers under
  `proposals/projects/vilan/proposal/` are the specs, and each backlog item cites the one
  that governs it. A work-order brief that names sections
  overrides this default. (Arcs move; the tracker is the pointer that stays
  true.)
- **Report honestly and compactly — your final message is the report,** not a
  separate file: what changed (files + why), what you ran with exact exit
  codes, what you did *not* verify, open questions. A true "unverified" is
  worth more than a false "works".

## Stop conditions (report instead of proceeding)

- An existing corpus golden or shipped test would need to change.
- A new check rejects existing std / corpus / docs code.
- The proposal underdetermines a semantic choice you would otherwise be making alone.
- The change wants a new dependency, new public CLI surface, or release machinery.
- Anything that would weaken a gate in order to pass it.
