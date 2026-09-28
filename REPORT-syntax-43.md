# REPORT — syntax-43 (Order 43)

Branch `syntax-43`, rebased onto `origin/next` @5e4eb1cc — all six other lanes merged (hygiene,
editor, native, reactive, solver, tuples); syntax-43 merges LAST. Three item commits, one per item,
each with its CHANGELOG entry under next's `## Unreleased`, plus this report.

Rebase notes (onto 5e4eb1cc): conflicts only in CHANGELOG.md (entries appended after tuples-43's)
and `diagnostics-ledger.tsv` (next's tail 576–586 kept whole, then this lane's four rows written
`NEW` for the helper to renumber). analyzer.rs, parsing.rs, formatter.rs and grammar.md merged
clean over solver-43/tuples-43; tuples-43's `TupleComprehension { bindings, body }` shape is
untouched (a comprehension binder may be a contextual word: `(lazy in t, jump in u => …)` parses
and reaches the analyzer). `keyword_table::reserved()`/`contextual()` and the `"contextual"` JSON
field survive on the rebased tree. RULE_STATEMENT_SITES = 55 (next's 52 + this lane's 3; no other
lane moved it) and the ledger gate is green at 55.

## Per item

### B414 — contextual keywords — LANDED `164ec34f` (S1 + S2 of the paper; S4 NOT built, see below)
- `with`, `borrows`, `own`, `dyn`, `lazy`, `jump` leave `lexing::KEYWORDS` (35 -> 29 reserved words)
  and lex as identifiers; `lexing::CONTEXTUAL_KEYWORDS` (13 rows, each with where it reads as a
  keyword: `as borrows context dyn jump lazy only own self Self sync void with`).
- The parser's decision points, named: `Parser::eat_word` (POSITIONAL: `with` after an impl subject /
  trait head, `borrows` after a return type), `Parser::eat_binder_prefix` (LL(2) at a parameter head:
  `own`, `lazy`), `starts_contextual_statement` (LL(2) at a statement head: `lazy let|mut|name`,
  `jump <target>` — one predicate for the dispatch AND the recovery sync points), `dyn` at a type head
  unless `dyn::`. Paper Q1 (jump contextual) and Q5 (hover kept) as recommended.
- `parsing::contextual_keyword_readings(source)`: the raw-parse read the LSP hover uses (a demoted
  word hovers as the keyword only where the parser read it as one).
- Placement rules (curated constants, CURATED_RULE_STATEMENTS): `OWN_IS_A_PARAMETER_CONVENTION`,
  `DYN_IS_A_TYPE_MARKER`, `LAZY_IS_WRITTEN_AT_THE_DECLARATION`; `jump;` steers to `jump break;` /
  `jump continue;` (an `import_steer` suffix). RULE_STATEMENT_SITES 52 -> 55.
- Formatter: the `borrows`/`context` canonicalization reads an identifier `borrows`; reprint pin.
- Grammars: hand-written positional rules for the six in BOTH the TextMate grammar and `vilan.js`,
  pinned both ways by regex (`b414_the_contextual_keywords_are_coloured_by_position_in_both_grammars`)
  and by the TextMate engine (`b414_each_demoted_keyword_is_a_keyword_in_its_position_and_a_name_elsewhere`);
  `grammar_sync`'s CONTEXTUAL_WORDS IS `lexing::CONTEXTUAL_KEYWORDS` now (+ UNPAINTED_CONTEXTUAL_WORDS:
  `only`); `literal_words` skips negative lookaheads; grammars regenerated.
- EBNF: `grammar_ebnf` reads the contextual table (six NON_KEYWORD_TERMINALS rows retired: context,
  sync, as, only, self, void); new pin `every_contextual_keyword_is_spelled_in_the_normative_grammar`
  (`Self` exempt, with why); §3 gains the contextual-keyword note and the `parameter` production's
  missing `[ "lazy" ]`.
- Pins: `tests/inference/contextual_keywords.rs` (NEW module) — per word, ONE file with the word as a
  binding, parameter, closure parameter, field (declared/initialised/read), method (declared/called),
  free function AND its keyword reading, compiled AND run; placement-rule pins; parser unit pins
  (readings, a declined attempt takes its reading back, the LL(2) predicate, the rules); lexer pin;
  LSP hover pin (both readings of all six in one file); formatter pin; `--print-keywords` pin.
  Native: the same probe runs identically natively (scratch, not pinned).
- Not breaking (a loosening); no corpus golden moved; ONE bindgen fixture moved
  (`tests/bindgen/inheritance.vl`: a TS member `own` binds as `own`, not `own_`).
- NOT BUILT — S4, the member tier (paper Q2, recommended yes): (1) its motivating program
  (`[derive(Json)]` decoding a `"type"` key) needs B416 (derives bind fresh locals; solver-43's,
  queued late) or json.vl's generated `let {field.name} = …` breaks on `type`; (2) a design gap the
  paper did not face — a keyword MEMBER after `.` must stay on the `.`'s own line, or every mid-edit
  `foo.` above a `let`/`if`/`ret` line swallows the next statement's keyword (E142's path rule,
  applied to members): a one-line owner ruling; (3) TextMate keyword lists need a `(?<!\.)` guard and
  hljs cannot express one. Recommend Order 44 after B416, with (2) ruled.

### D14 — the spec's contextual-keyword list from the table — LANDED `bdae67e2`
- §2.2 and §A.2 print the contextual keywords as a `text` fence; `docs.rs`
  `the_specs_contextual_keywords_are_the_lexers` holds both fences to `CONTEXTUAL_KEYWORDS` in both
  directions (N87's discipline), and the attribute names beside them to `KNOWN_ATTRIBUTE_MARKERS`
  (adds the missing `client_service` to both). `as`/`only` were already listed on the tip (premise
  partly stale); `layer` is named nowhere.

### F27 R3 — platform-fenced twin items — LANDED `4c863eaa` (compiler + `vilan check` + the editor's legs; editor request ROUTING not built)
- `platform_color::select_twins` (syntactic, per leg, before collection): twins = two items of one
  identity (module-level functions of one name; `impl`s of one trait for one subject — keyed by the
  written head), both fenced, fences disjoint over `known_hosts`. The leg's platform keeps its twin;
  the others walk as a no-op (`Analyzer::fenced_out_items`, one check at `walk_expr_node_inner`'s
  head) — no entity, no B57/B98, no emission. Userland (Pkg/Dep/entry) only; std's layer twins untouched.
- The six rulings: first build = trait impls + free functions (nominals/inherent impls have no
  twins); twins need not cover every host (a miss names the twins: `import_steer`); a single-leg build
  checks one leg (the new invariant, spec §11.3); default-plus-override refused; twin functions agree
  on their written signature; the editor keeps the second platform's analysis for twin files (legs:
  `PlatformReason::Twin`, added in `file_platform_choices(_for)`, so `vilan check` AND the LSP's
  further-leg analysis cover each twin — bare files too).
- Over `mod self;` (B415): a twin its file's declaration excludes is refused.
- Pins: `tests/inference/platform.rs` f27_r3_* (the userland `Slot` twins in std `ui.vl`'s own shape —
  browser twin reads `View.element`, process twin `View.attributes` — compile + run per platform;
  build selection incl. deno/bun; single-leg invariant; overlap; unfenced default; signature; missing
  twin; `mod self;` exclusion; non-twins unchanged); `tests/workspace.rs` f27_r3_* (package module and
  bare file checked under the twin's leg); `platform_color` unit pins. Ledger rows (written `NEW` at the rebase; were 630–633).
- NOT BUILT (Order 44 remainder, §8.4 items 2–4): hover/goto/completion INSIDE a fenced-out twin are
  answered by the primary leg only (so, nothing); go-to-definition does not offer both twins; dead-code
  paint does not know about twins.

## The keyword-table interface (for editor-43 / the site)
editor-43 landed `vilan_core::keyword_table` and `vilan --print-keywords` FIRST (502e12fe); at the
rebase syntax-43 took THEIR module/flag and dropped its own duplicate, extending it:
- `keyword_table::keywords()` — every word with a keyword reading (reserved + contextual), sorted;
  `reserved()`; `contextual()`; `is_keyword(name)` = RESERVED only (bindgen's escape test — a
  contextual word is a legal name, so bindgen now binds `lazy`/`dyn`/`own` as written; `css` still
  escapes); `to_json()`.
- `vilan --print-keywords`: `{"keywords": [...all...], "contextual": [...subset...]}` — no word an
  old reader highlighted was dropped from `"keywords"` (their seam note honoured); reserved =
  keywords − contextual. Pinned in `keyword_table.rs` and `grammar_sync::the_printed_keyword_table_is_the_lexers`.
- K24 (the site, owner's): editor-43's diff reads `"keywords"` and stays valid; a site that wants
  positional painting reads `"contextual"` too. The site's committed `keywords.json` must be
  regenerated from this toolchain (it gains `Self as context only self sync void` and the `contextual` field).

## Count words changed in docs
- spec/lexical.md §2.2 and spec/appendix.md §A.2 reserved fences: 35 -> 29 words (no count word in prose).
- tour/values-and-types.md: "two are worth knowing before you hit them" removed (the own/jump apology).
- diagnostics_ledger.rs doc: "The eighteen are named" -> "They are named" (was stale: 34 entries, now 37).
- No book_sync count word touched; no new quick fix or code action.

## Gates (on the rebased tree, 5e4eb1cc + 3 items)
- `cargo fmt --check`: clean. `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `vilan fmt --check vilan/std` and `vilan fmt --check vilan/examples`: clean.
- `cargo nextest run` over `-p vilan-cli` corpus, split, examples, diagnostics_ledger, grammar_ebnf,
  grammar_sync (TextMate scope pins ran — node_modules installed, nothing SKIPPED), hygiene,
  native_differential (default); `-p vilan-core` docs, interpreter, inference; the whole
  `-p vilan-lsp`: 5131 run, 5131 passed, 1 skipped (an `#[ignore]`), 0 failed.
- Earlier runs: the whole workspace 8724/8724 on 762c6aa5 + the items; on 49e0e817 the only reds
  were hygiene-43's (since folded on next).
- `VILAN_NATIVE_DIFFERENTIAL=1`: not run — no std file touched.

## FINDS to file
1. NATIVE (pre-existing on 0.41.1): a `lazy let` module binding passed as the argument of a `lazy`
   PARAMETER is not emitted — `take(config)` -> rustc `cannot find value config_N` ("BACKEND defect").
   `config` read directly is fine. vilan-rust.
2. JS/both (pre-existing on 0.41.1): interpolating a `&i32` view returned `borrows xs`
   (`fun first(xs: &List<i32>): &i32 borrows xs { &xs[0usize] }`, `i"{first(&xs)}"`) prints
   `7,8,0` — the view's internal representation — instead of `7`.
3. Formatter (pre-existing): `fun f(xs: &List<i32>): &i32 context c borrows xs` is NOT canonicalized to
   `borrows xs context c` (the `&T` return type; an `i32` return is) — `canonicalize_declaration_clauses`.
4. Parser/R1 (pre-existing): `[platform("browser")]` BEFORE `export impl …` reads the attribute as a
   list expression ("expected `;`" — `vilan fmt` declines the file); the spelling is
   `export [platform(..)] impl`. Deserves a steer like `pub fun`'s.
5. (resolved on next) hygiene-43's merge left reds seen at 49e0e817: corpus_manifest rows for its
   three corpus programs and REPORT-hygiene-43.md:109's absolute home path — green at 5e4eb1cc.
6. B414 S4 + F27 R3's editor remainder — above.

## Expected conflicts
- CHANGELOG.md `## Unreleased`: textual only (three entries append at the tail).
- crates/vilan-cli/tests/diagnostics-ledger.tsv: four `NEW` rows at the tail, after next's 576–586.
- crates/vilan-cli/tests/diagnostics_ledger.rs: `RULE_STATEMENT_SITES` 52 -> 55 and three
  CURATED_RULE_STATEMENTS rows — a lane adding a parse rule meets the same constant.
- crates/vilan-core/src/analyzer.rs (merged over solver-43 and tuples-43 cleanly): four small hunks — two Analyzer
  fields + init, `select_platform_twins` beside `set_current_source`, the skip at the head of
  `walk_expr_node_inner`, the two `select_platform_twins` calls at the module/entry walks, and
  `import_steer`'s `jump`/twin arms. Rebased clean onto 5e4eb1cc.
- The proposals ledger (diagnostics-ledger.md): the four `NEW` rows and row 229's population (+3 curated
  constants, sites 55) are the integrator's to write — proposals is read-only here.

## Appendix — the lane status (LANE-STATUS.md content, not committed as a file)

Kept as the lane wrote it over the day; where it disagrees with the sections above (the first
interface sketch, the pre-rebase reds), the sections above are current.

Worktree `vilan/.claude/worktrees/syntax-43`, branch `syntax-43` off origin/next @762c6aa5.
Items: B414 (contextual keywords, RULED R-e), D14 (spec list from the table), F27 R3 (fenced twins).

### The keyword-table interface (for editor-43 — E225 + K24 consume it)

DEFINED HERE (editor-43 had no LANE-STATUS when this was written); shape:

- Rust, `vilan_core::lexing`:
  - `KEYWORDS: &[(&str, Token)]` — UNCHANGED shape; now the HARD (reserved) words only.
    B414 removes `with`, `borrows`, `own`, `dyn`, `lazy`, `jump` from it (35 -> 29 rows). NOTE: the JSON shape below is SUPERSEDED by the reconciliation section.
  - `CONTEXTUAL_KEYWORDS: &[(&str, &str)]` — NEW: `(word, where it reads as a keyword)`.
    Each lexes as `Token::Ident`. Holds `as`, `borrows`, `context`, `dyn`, `jump`, `lazy`,
    `only`, `own`, `self`, `Self`, `sync`, `void`, `with`.
  - `keyword_table() -> Vec<KeywordEntry>` — NEW: both lists merged, sorted by word;
    `KeywordEntry { word: &'static str, contextual: bool, reading: Option<&'static str> }`.
- CLI: `vilan --print-keywords` prints the same table as JSON:
  `{"keywords":[{"word":"as","contextual":true,"reading":"..."},{"word":"async","contextual":false},...]}`
  (one object per word, sorted; `reading` present only when `contextual`).
- bindgen `RESERVED` (editor-43's): a free-function / binding name must avoid the HARD words;
  a contextual word is a legal name everywhere. (If the member tier lands, a METHOD/field name
  may be any word — see B414 S4 below.)

### Reconciliation with editor-43 (their 502e12fe landed the same flag first)

editor-43 shipped `vilan_core::keyword_table` (keywords / is_keyword / to_json) and
`vilan --print-keywords` printing `{"keywords": ["async", ...]}` (strings), with the seam
note "a contextual flag lands as a second JSON field, never by dropping a word an old
reader still highlights". DONE at the rebase onto 013b3860: syntax-43 took THEIR module, flag and JSON shape and
drops its own duplicate (`lexing::keyword_table_json`, its CLI flag hunk), adding:
`"contextual": [...]` — the contextual subset — while `"keywords"` lists every word with a
keyword reading (reserved + contextual), so no word an old reader highlights is dropped;
reserved = keywords - contextual. `is_keyword` (bindgen's escape test) stays the RESERVED
words only: a contextual word is a legal name, so bindgen stops escaping `with`/`own`/...

### Progress
- [x] B414 S1+S2 LANDED 164ec34f (was 58c0a6e0 before the rebase onto editor-43) (with/borrows/own/dyn/lazy/jump contextual; `vilan --print-keywords` + `lexing::CONTEXTUAL_KEYWORDS` / `vilan_core::keyword_table::{keywords, reserved, contextual, is_keyword, to_json}` per the reconciliation)
- [x] D14 LANDED bdae67e2 (§2.2 + §A.2 contextual fences gated in docs.rs against CONTEXTUAL_KEYWORDS; attribute names against KNOWN_ATTRIBUTE_MARKERS)
- [x] F27 R3 LANDED 4c863eaa (functions + trait impls; editor routing of requests into the admitting leg = §8.4 items 2-4 NOT built, Order 44 remainder)
- [ ] B414 S4 (member tier, Q2) — NOT BUILT, deliberately: (1) its motivating program (`[derive(Json)]`
  decoding a `"type"` key) cannot work until B416 (derive binds FRESH locals, solver-43's, queued late)
  lands — json.vl splices `let {field.name} = ...`, so a field named `type` would fail in generated code;
  (2) a design gap the paper did not face: a keyword MEMBER after `.` must be restricted to the `.`'s own
  line, or every mid-edit `foo.` above a `let`/`if`/`ret` line swallows the next statement's keyword
  (E142's path rule, applied to members) — a one-line owner ruling; (3) the TextMate keyword lists need a
  `(?<!\.)` guard and hljs cannot express one. Recommend Order 44, after B416, with (2) ruled.

### Next's own reds at 013b3860 (historical; all green at 5e4eb1cc)
Full `cargo nextest run --workspace` over syntax-43 rebased onto 013b3860: 8750/8756, the six reds:
- bindgen `every_fixture_matches_its_golden_byte_for_byte` — MINE (a TS member `own` now binds as `own`,
  B414); golden regenerated and folded into the B414 commit.
- FIVE from hygiene-43's merge (dfb5e511): the three new corpus programs `f64-print-boundary.vl`,
  `f64-print-negative-zero.vl`, `non-bmp-string-length.vl` have no `corpus_manifest!` rows
  (interpreter / infer_differential / release_differential `every_corpus_program_has_a_test_of_its_own`),
  `copy_elision_census` not regenerated for them, and `hygiene::no_tracked_file_contains_an_absolute_home_path`
  on REPORT-hygiene-43.md:109. The integrator's to fold.
