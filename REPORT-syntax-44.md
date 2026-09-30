# REPORT — syntax-44 (Order 44)

Branch `syntax-44`, rebased onto `origin/next` @eb99d1d6 (solver-44 merged). Four item commits,
each with its CHANGELOG entry under `## Unreleased`, then this report. LANE-STATUS.md is kept
untracked (next removed it @2458c7f7).

## Per item

### E233 — LANDED `3aa65fcc` (was `cffaf928` before the rebase)
- Premise corrected: the cause is the PARSER, not the formatter. The `&` type production parses a
  whole type, so `fun f(xs: &L): &i32 context c borrows xs` put the clause on `i32`. 0.41.1
  REFUSED the program ("a `context` clause is only supported on a closure type"), and with no
  declaration clause in the tree the formatter reprinted the written order.
- Fix: `hoist_clause_out_of_a_view` lifts the clause off a view's target before B242's peel. A
  view of a closure type keeps its clause (B309). Both orders now compile, and `vilan fmt` prints
  `borrows xs context c`.
- Pins, each proven red without the hoist: a parser unit pin, a formatter pin, and
  `inference/macros.rs::e233_…` (both orders plus `&mut`, compiled and run).
- Family: fix.

### B414 S4 (R-k, RULED strict) — LANDED `06fb5ae5` (was `0950caea`)

**Census.** Counted by the compiler's own lexer (`lexing::tokenize`): `.` or `?.`, then whitespace or
a newline, then a name.
- Covered: std (65 .vl), vilan/examples (29 .vl, 23 fences), vilan/test (146), vilan/docs
  (632 fences), macro_std (3), benchmarks (7), crate .vl fixtures (25), kolt (31 .vl, 4 fences,
  read-only), the website (15 .vl, 4 fences), the playground (6), and github.io.
- Also covered: about 4,900 raw-string and 4,300 plain-string vilan snippets inside
  `crates/**/*.rs`. The plain-string hits were all prose.
- Result: **zero legitimate uses**, so the STRICT form is built.

**Member tier.** Any word, reserved ones included, names a member at every position that is entered
after a committing token:
- after `.` and `?.`, and after an element-head or `css` item dot;
- after `::` in an expression path;
- as a struct field, and as a literal field given with `=`;
- as a method in an `impl` or `trait`;
- as an impl-selector member.

A reserved word still cannot name a binding, a parameter, a free function or a type. The literal
shorthand `{ type }` stays refused.

**R-k.** Nothing may stand between a member dot and its name.
- A space on the dot's own line is reported and the member is still read: one diagnostic, "found
  'n' expected a member name written against its `.` — `value.name` …".
- A line break before a stranded name declines, E142's shape.
- A line break before a word that starts a statement keeps the old silent Error member, so the
  statement is kept and completion still types the receiver. I found this through
  `member_completion_on_a_constructor_receiver_prefers_the_nominal_in_scope`.
- A chain broken BEFORE its dots is untouched.

**What it unblocks.** The JSON `"type"` key: declared, read, encoded and decoded by `[derive(Json)]`,
on both backends. The new corpus program `keyword-members.vl` sits in the native DEFAULT_SUITE.

**`css`.** It joins the member tier. Solver-44's B471 did-you-mean now carries the rename at those
misses, and the four K2c member-position pins pin the member reading. B471's pins stay green on the
rebased tree. The parser's rename note still guards a `css` binding.

**Grammars and hover.**
- The generated TextMate keyword lists are guarded against a preceding `.` and a following field
  `:`/`=`.
- The book gains member and field-name modes.
- A method *declared* with a reserved name is left to semantic tokens.
- LSP hover answers a reserved member as the member (`parsing::keyword_member_readings`).

**Other.** `RULE_STATEMENT_SITES` went 55 → 54 (the css `::` recovery arm is gone). Family:
**breaking**, with "Migration: none in the estate".

### B459 — LANDED `1b00ff73` (was `ae8d6218`), plus R15 `d3c29963`
- The forms: the expression form `c then a else b`, and the statement forms `c then S;`,
  `c else S;` (the guard) and `c then S else S;`. All are sugar over `if`: the `If` node carries
  `IfSpelling::Then`.
- The reading: a form ended by its statement's `;` is re-read as a statement (`read_as_statement`),
  so its branches need not unify (Q6). A post-parse pass refuses forms still read as values.
- Precedence: above assignment, below `||` (Q1). Right-associative, and `else` binds the nearest
  `then` (Q2).
- Refused: a bare `then` as a value (Q3, RULED), and `let` as a branch (Q8). The guard is recognised
  only at a statement's head.
- `then` is contextual (Q4): `CONTEXTUAL_KEYWORDS` goes 13 → 14.
- Formatter (Q5): reprints the forms as written and breaks an over-budget one before each
  `then`/`else`, one level in, with `else` chains flattened.
- Spec: EBNF `conditional-expr` and `then-statement`, precedence row 13, spec §2.2 and §A.2.
- Editor: both grammars paint `then` by position. Hover on `then` has a `KEYWORD_DOCS` row and the
  tour section `then` / `else` (anchor golden +1).
- Three curated rule constants: `THEN_NEEDS_ITS_ELSE`, `THE_GUARD_IS_A_STATEMENT`,
  `A_BRANCH_BINDS_NOTHING`. `RULE_STATEMENT_SITES` goes 54 → 57.
- **R15** (`d3c29963`, analyzer, landed after the rebase), B187 mirrored:
  - The condition walk collects its TRUE path's captures (root parity, outside every `||` operand,
    negation and off-spine step).
  - A Then-spelled statement whose `else` diverges queues a `GuardContinuation` that publishes those
    captures into the enclosing scope from the statement's end.
  - `resolve_world` decides it, so all four divergence leaves count.
  - `GuardContinuation`'s block fields are renamed `diverging_*`.
  - The keyword `if` keeps B171's scoping.
- Pins:
  - `inference/then_else.rs`: 11 (7 for B459; 4 `r15_*` covering ret/jump/panic and `then…else`, a
    non-diverging `else` and the keyword `if`, negated and `||` captures, chained and nested
    guards).
  - The R15 positive pins were red before the rows. The non-diverging pin was proven red with the
    divergence verdict planted true.
  - Also: 5 parser pins, 2 formatter pins (red without the printer), 2 grammar_sync, 1 LSP hover.

## Rebase (onto eb99d1d6)
- CHANGELOG: this lane's four entries appended after solver-44's.
- `diagnostics-ledger.tsv`: next's rows kept whole, then this lane's one `NEW` row.
- `copy-elision-census.tsv`: total 467 + 2 = 469, verified by the gate.
- parsing.rs (B470's `parse_trait`), grammar.md and the anchor golden (B460) merged clean.

## Ledger rows
- `diagnostics-ledger.tsv`: one `NEW` row, the R-k expectation ("a member name written against its
  `.` — …").
- Curated rule statements: +3 (above), net sites 55 → 57. Row 229's population is for the
  integrator to write in the proposals ledger.

## Goldens
- No existing corpus golden moved.
- New: `vilan/test/keyword-members.vl` / `.mjs`.
- Census rows added: `copy-elision-census.tsv` (keyword-members 2), `native-copy-census.tsv` (1/2)
  and `native-leak-census.tsv` (0/0).
- `markdown_anchors.golden` gains `tour/control-flow.md h2 then--else`, regenerated with mdbook
  0.5.4.
- The TextMate grammar's generated keyword lists were regenerated.

## Count words changed in docs
- The spec's contextual-keyword fences (§2.2, §A.2) go 13 → 14 words.
- No count word in prose.

## Gates (rebased tree, the four item commits)
- `cargo nextest run --workspace`: **9017/9017 passed**, 32 skipped. This includes grammar_ebnf 14,
  grammar_sync 34 (TextMate engine present), vscode_extension 24, corpus 12, split 13, examples 6,
  docs 12, diagnostics_ledger 25, native_differential 81, check_scope_differential 15, inference
  4884 and vilan-lsp 900.
- `VILAN_NATIVE_DIFFERENTIAL=1` native_differential plus check_scope_differential: 96/96.
- `cargo clippy --workspace --all-targets -D warnings`: clean.
- `cargo fmt --check`: clean.
- `vilan fmt --check` over vilan/std and vilan/examples: clean.

## Touches outside this lane's owned files
- vilan-lsp `document.rs` (editor-44): the reserved-member hover arm, plus pins for it and for
  `then`.
- vilan-ide `completion.rs`: the `then` `KEYWORD_DOCS` row and the `else` sentence.
- `lift.rs`, `elements.rs` and `css.rs` carry the new `If` field.
- Test registries: the corpus manifest, `infer_differential` FOLDS, the native suite, and
  `parse_expr_regression` (B238's non-vacuity now uses the literal shorthand).

## Owner questions (none blocking)
1. bindgen still escapes reserved METHOD names (`type_`). The paper's S4 says to stop now that
   members may be any word. Not built here: it is outside this lane's files, and
   `examples/canvas` would move.
2. `let x = c else 1;` (the guard in value position) reports a plain "expected `;`". The guard's own
   message fires only at a statement's head.
