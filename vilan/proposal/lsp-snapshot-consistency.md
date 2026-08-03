# LSP snapshot consistency

**Status:** approved for implementation (2026-07-28).
**Symptoms (user-reported):** semantic highlighting breaks often while typing; inlay hints move
around on type.

## Diagnosis

The server keeps two views of an open document that advance at different times, and the request
handlers mix them.

- `did_change` applies the new text immediately: `Document::set_text` rebuilds `line_index` and
  `text` (`document.rs`), deliberately, so completion's context scan sees the just-typed
  character. `program` — and everything derived from analysis — stays at the last *analyzed*
  text until the debounced re-analysis lands (150 ms debounce + 80–190 ms analysis on realistic
  files).
- `semantic_tokens_full` and `inlay_hint` then convert the stale program's **byte spans**
  through the **fresh** line index (`main.rs`). Same bytes, different text: one insertion above
  shifts every token and hint below it. Inlay hints can also vanish outright — the viewport
  filter runs on the mis-converted position.
- Nothing corrects the wrong answer when analysis lands: the server sends no
  `workspace/semanticTokens/refresh` or `workspace/inlayHint/refresh`, tracks no version, and
  returns `result_id: None`. The client re-asks only on its own schedule.
- Independent second cause for highlighting: H6 salvage keeps only the prefix on some breaks
  (unterminated triple-quoted string, stray top-level token) — pinned as intended — so the
  file's tail loses all tokens until the text is whole again.
- Confirmed en route: a completed analysis **clobbers** newer buffer state
  (`documents.insert` replaces the whole `Document`; the debounce generation is only checked
  before the analysis starts), an analysis completing after `did_close` **resurrects** the
  closed document, token wire `length` is in **bytes** while positions are UTF-16, and
  `reanalyze_dependents` serially re-analyzes every other open file per typing pause.

## The law

A `Document` holds two snapshots:

- the **live snapshot**: `text` + `line_index`, advanced synchronously on every `did_change`;
- the **analyzed snapshot**: the `program`, every analysis product, and the line index of the
  text the analysis consumed, advanced only when an analysis lands.

**S1 — Program-space conversions use the analyzed snapshot.** Every conversion between a
program span/offset and an LSP position — outbound (semantic tokens, inlay hints, hover ranges,
definition/reference/symbol locations) and inbound (the position→offset lookup feeding
`entity_at` and friends) — goes through the analyzed snapshot's line index. Rationale:
line/column coordinates are stable under edits on *other* lines, so an answer that is exactly
correct for the analyzed text is also visually correct everywhere except the lines the user is
actively changing. Converting old byte offsets through the new index is correct for *neither*
text. (A useful corollary: an old span past the new text's end no longer clamps to EOF — tokens
stop piling up at the end of a shrunken file.)

**S2 — Live-text operations use the live snapshot.** Completion's backward context scan,
whole-document formatting, and manifest completion read the live text and keep the live index.
Completion's entity probes tolerate skew by design (it must work mid-typing on an offset the
program has never seen); this stays as-is and is out of scope.

**S3 — Mutating program-space requests refuse while the snapshots diverge.** A request that
returns workspace/text edits computed from program data (rename; organize imports; any other
edit-producing handler found in the sweep) refuses when the live text differs from the analyzed
text. Edits computed against one text and applied to another corrupt the buffer; refusal is
honest and, at human timescales, unobservable — these requests happen at rest, after the
debounce has landed. The refusal's *spelling* follows how the request was invoked: an
**explicitly** user-invoked request (rename) answers `RequestFailed` with "still analyzing —
retry", which the client surfaces inline without a toast; an **automatically** fired one (code
actions: menu population, the on-save hooks) answers `ContentModified`, which the client
swallows into the default empty answer — a save mid-typing must be a clean no-op, not an error
toast. Formatting is exempt (live-space, S2). Read-only queries never refuse — they answer
correctly-for-the-snapshot (S1).

**S4 — An analysis lands by merge, never by clobber — and only if it is *the* live text's.**
Landing an analysis:

- if the document was **closed** while the analysis ran → drop the result (no resurrection);
- if the analysis is **not of the current live text** → drop the result. Two analyses of one
  document can be in flight at once (the debounce generation is checked only before an analysis
  *starts*) and can finish in either order; adopting an out-of-order result would regress the
  analyzed snapshot underneath a newer one and leave the document stuck stale — wrong
  coordinates and refused renames with nothing scheduled to heal them until the next keystroke.
  Dropping is always safe: a live text the analysis doesn't match implies a later `did_change`
  whose own debounced task (or an already-landed fresher analysis) covers the buffer;
- if the analysis is of the live text → adopt everything.

So the analyzed snapshot only ever advances to the live text, never sideways to a different
stale one. The `Document`-level merge keeps its own keep-the-live-side guard regardless (adopt
the analysis side, preserve `text` + `line_index` when they differ) — two independent layers:
the landing gate never regresses the snapshot, the merge never loses typed text even if a
future caller lands something the gate would have refused.

Every `Document` field is classified live-side or analysis-side; the merge is a `Document`
method so the classification lives in one place and is unit-testable. The open path must still
guarantee a document lands in the map on first analysis.

**S5 — The server announces freshness.** When a completed pause lands analyses (the edited
file and its dependents sweep; likewise `did_save`'s sweep), the server sends
`workspace/semanticTokens/refresh` and `workspace/inlayHint/refresh` — once per completed
sweep, not per document, and not when the unchanged-text short-circuit skipped the work.
Errors are ignored (a client may not support refresh; `vscode-languageclient` 9.x does, and
re-requests both providers automatically — no extension change). tower-lsp 0.20 exposes both
as `Client::semantic_tokens_refresh` / `Client::inlay_hint_refresh`.

**S6 — Wire lengths are UTF-16.** Semantic token `length` is computed in UTF-16 code units
(end character minus start character; tokens never span lines — guard totally anyway), matching
the UTF-16 positions the line index already produces.

## Test plan (each case its own pin)

Document-level, in `document.rs` tests, plus a pure token-encoder function extracted so the
wire encoding is testable outside the handler:

1. Analyze, then `set_text` inserting one character on an early line → semantic-token
   positions unchanged (old-text coordinates), not byte-shifted.
2. Same skew pin for inlay hints.
3. Newline-insertion variant: positions still old-text coordinates.
4. Shrinking edit that leaves a token's old offset past the new EOF: no panic, no EOF-clamp
   pile-up; position stays the old-text one.
5. Multi-byte line (em-dash + astral char before an identifier): token `delta_start` and
   `length` both in UTF-16 units, via the extracted encoder.
6. Encoder delta rules pinned on their own (same-line delta, cross-line reset).
7. Merge with advanced live text: live side kept, analysis side adopted; token conversion uses
   the adopted analyzed index; completion scan still sees the live text.
8. Merge with unchanged text: full adoption.
9. Inbound skew pin: after an early edit, a program lookup (hover/definition) at an
   analyzed-text position still resolves through the analyzed index.
10. Stale-refusal pin per mutating handler (rename; organize imports; any other found).
11. Refresh decision: landed sweep → one refresh pair planned; unchanged-skip → none. Pin at a
    planner-style seam (the E6 pattern) if the transport itself can't be faked.
12. No-resurrection: closed-during-analysis result is dropped — pin at whatever seam allows;
    some committed test must cover it.

Acceptance beyond the suite: a stdio probe (lsp-probe pattern, scratchpad) advertising
`refreshSupport`, driving a didChange burst — mid-burst token answers are analyzed-snapshot
stable, and both refresh requests arrive after the pause; wire-log excerpt as evidence. And a
grep audit: every remaining `line_index` conversion call site in the handlers is justified
live-space in the report.

## Implementation notes (2026-07-28, as built)

Three places where the build found more (or less) than the design assumed:

- **S6's `length` fix is latent, not observable.** Only `length` was in bytes; `start` and
  `delta_start` already came from the line index and were UTF-16 all along. And every span
  the classifier produces is an identifier span, with identifiers ASCII-only
  (`lexing.rs::is_identifier_start`) — so byte width and UTF-16 width agree for every token
  reachable today. The fix and its pin stand as the guard for the first classifier that
  covers non-ASCII text (an interpolation hole, a comment, a wider identifier alphabet);
  they are not part of what users see fixed. The *visible* wire bug was S1's alone.
- **Hover's lexical half moved with it.** `keyword_hover` and `doc_comment_of`'s
  entry-file branch read the document's TEXT at a program offset. Since hover's inbound
  conversion is now analyzed-space (S1), both read the analyzed text — otherwise hover
  would mix one snapshot's offset with the other's characters, which is the same defect
  one level down. Keyword hover still works on a document that does not compile: lexing
  does not depend on analysis succeeding.
- **`Document::organize_import_edits` stays live-space.** Its spans come from the
  formatter's own parse of the live text, so the handler converts them through the live
  index — S2, not S1. Its prune half is program data, and the S3 refusal in the handler
  now runs before it, so the two texts are equal by the time it is reached; the internal
  freshness gate became `!is_stale()` (exact) instead of a `text_hash` comparison. Note the
  one behavior cost: `vilan.organizeImports.onSave` saving *within* the debounce window
  now gets a refusal instead of the sort-only pass it used to do. That follows from S3 as
  written (it names organize imports); the user-visible effect is that an organize-on-save
  during active typing does nothing rather than half the job.

## Review round (2026-07-28, adversarial review before commit)

The review blocked the first build on two findings, both confirmed and fixed; the law above is
the post-review form.

- **The landing gate is the review's.** As first built, `land` merged ANY completed analysis
  onto the open document — the out-of-order interleaving in S4's second bullet (older analysis
  finishing second, snapshot regressed, document stuck stale, every rename refused until the
  next keystroke) was real and unpinned. The fix is the adopt-only-the-live-text's-analysis
  rule now in S4, pinned both ways (`a_stale_analysis_finishing_out_of_order_is_dropped`,
  `an_analysis_the_buffer_has_moved_past_is_dropped`) plus the fresh-adoption case. Landing an
  analysis of a moved-on buffer *was* the first build's behavior (its "keep the live side"
  merge branch); the gate supersedes it, and the merge branch stays as the second safety layer.
- **The refusal code as first built raised an error toast.** Everything answered
  `RequestFailed` (`-32803`), justified by a comment claiming tower-lsp 0.20 predates the named
  variants — false for `ContentModified` (`-32801`), which the crate has. On the code-action
  path `vscode-languageclient` shows a toast for any code except `ContentModified`
  (`handleFailedRequest`, `showNotification` defaulting true), so `organizeImports.onSave`
  during the debounce window popped "Request textDocument/codeAction failed." — and the
  as-built note below, which claims an organize-on-save mid-typing "does nothing rather than
  half the job", described intent, not what shipped. With the S3 spelling split (rename keeps
  `RequestFailed` — its path passes `showNotification: false` and the rename widget shows the
  message inline; code actions answer `ContentModified`) that claim is now true.
- **Recorded, not fixed:** the inlay-hint viewport filter compares an analyzed-space position
  against the client's live-space range — under FULL sync there is no mapping between the two
  spaces, so the filter is exact for same-line edits and off by the inserted/deleted lines near
  the viewport edge until the refresh lands (~200 ms). Inherent until incremental sync; noted
  at the filter and in backlog 39(c).
- **Sanctioned as unpinned:** the `publish.rs` conversion switch (behavior-neutral today —
  publishing runs right after a landing, when the snapshots are equal by S4; the switch is
  uniformity) and the `landed`-bool plumbing from sweep to `refresh_plan` (the decision
  functions are pinned; the plumbing is exercised end-to-end by the wire probe, including the
  no-refresh path for a burst that reverts to the analyzed text). The handler wiring for
  hover/definition/references/symbols, flagged as revert-survivable, is now pinned per handler
  (`*_answers_the_analyzed_snapshot_while_typing`).

## Out of scope (recorded follow-ups)

- Salvage tail retention for semantic tokens — **DONE 2026-08-03** (backlog B38): retention is
  scoped to the byte-identical line-aligned common suffix and served only when the fresh stream
  is silent within it, so this proposal's law (no retained tokens for changed text) is preserved
  by construction.
- Gating `reanalyze_dependents` on actual dependency (perf; serial full re-analysis of every
  open file per pause).
- `semanticTokens/range` + delta/`resultId` providers; incremental (non-FULL) sync.
- Completion's skew tolerance (deliberate, S2).
