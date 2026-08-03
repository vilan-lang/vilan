# Analysis reuse — the E3 arc (leak closure, the prelude checkpoint)

> **Status: Phase 1 SHIPPED 2026-07-21; Phase 2 CLOSED BY MEASUREMENT same
> day — its own gate fired. E3 closes at Phase 1; Phase 3 keeps the evidence.
> 2026-08-01: a Phase-1 residual (macro-DEFINING buffers) is filed as backlog
> E23 — recorded in the residual block below.**
>
> **2026-08-02: Phase 3 is REOPENED as the std-tax arc (§6)** — the
> suite-speed audit's E28+E30 fold in here; the fixed ~115 ms per-analysis
> std tax is re-measured and re-decomposed on the v0.22.1 tree, the
> `VILAN_PHASE_TIMING` instrument is shipped (§6.3 S0), and the slice plan
> S1–S4 stands where "recorded, not planned" stood.
>
> **The Phase 2 stop (implementation step 1, no code shipped):** the §3
> premise — "snapshot after prelude/dependency loading, analyze only the
> entry on top" — assumed build/checks over the entry were cheap. Measured
> (warm LSP floor ~88 ms): loading+walking = 16.3 ms (**18.5%**, under the
> 30% stop bar); `build()` = 43 ms and the whole-program checks = 29 ms —
> **82% of the floor re-resolves/re-checks the unchanged std every
> keystroke**, *after* the entry is interwoven (the entry seeds the load
> worklist, expands inside the load loop, and one monolithic `build()`
> resolves std+entry together). A controlled tiny-vs-big-entry comparison
> confirmed build/checks are std-dominated (+1200 entry entities moved them
> only +10.5 ms). Clone cost (~56 entity-scaled maps) erodes the 18.5%
> further. **Capturing the 82% requires an entry-delta fixpoint + entry-
> scoped checks over a frozen std base — Phase 3 (§4), not a deeper
> checkpoint.** Concrete Phase-3 blockers found en route, recorded in §4.
> Backlog E3 (L), reframed by the 2026-07-21 scout; numbers re-measured that
> day.
>
> **Phase 1 outcome + two corrections to §0** (implementation, same day): the
> three uncached sites are closed — the two macro parse sites now route
> through the content-addressed `parse_cached` (gensym stamping bakes the
> site number into the text, so the content key was already site-composite;
> "stamped parses fresh" was optimization-only, per git archaeology), and the
> `run_service` leak was **removed at the root** (nothing ever borrowed the
> input — it was only hashed). Corrections: (1) the scout's "only
> `reactive.vl` uses `fresh(`" was a grep false-positive (`Refresh(`
> matched) — **std ships no gensyms**, so the per-keystroke exposure was
> user-gensym-macro projects, not every UI project; (2) a hypothesized fourth
> site (`flush_rust_fallback`) was measured and disproved — the scout's three
> were complete. Instrumentation: `leak_tally` (14 named sites, thread-local
> counters), and the harness is un-`#[ignore]`d asserting on COUNTERS — the
> measured split on a changing entry is **357 B/analysis of named leak vs
> ~60 KiB/analysis of RSS allocator churn**, which is §0's inference made
> fact and Phase 2's whole motivation: the churn *is* the re-analysis, and
> only the checkpoint removes it.
>
> **Phase 1 residual (found 2026-07-28 scoping D11's leak exposure; filed
> 2026-08-01 as backlog E23):** the world cache keys on the hash of the
> length-preserving BLANKED source — every byte outside the macro definitions
> becomes a space — so the key depends on the whole file's length and newline
> layout, and a buffer that DEFINES macros recompiles and re-leaks its world
> (`MacroWorldText`, ~file size, plus a whole `MacroWorldProgram`) on any
> length-changing edit outside the macro spans. The Phase-1 harness never
> measures this, deliberately: `gensym_expansion_leak_plateaus` holds its
> edit tail at a fixed four digits so the blanked source stays byte-identical
> and the world stays cached — a dodge the harness comments state but this
> file did not, until now. `compile_world`'s "bounded: one leak per distinct
> macro-definition set" is therefore true only while the non-macro text never
> changes length.
>
> **E23 SHIPPED 2026-08-01** (with a second find: a BROKEN definition's world
> failure was never cached, so it re-leaked per analysis even unedited — a
> `FAILURES` cache keyed on content+offsets closes it). The same-day
> `Box::leak` sweep then closed the remaining gaps, each pinned red-first:
> the wasm front-end's per-compile UNTALLIED entry leak (content-interned,
> `WasmEntryText`), the per-analysis dependency display-name leak
> (content-interned), `flush_rust_fallback`'s uncached parse (now
> `parse_cached`), and `EntryAst`'s shallow tally (now tree-proportional).

## 0. What the scout established (corrections to E3's framing)

- **The leak is real and reproduces**: 44.8 KiB/analysis RSS growth (harness
  `measure_per_analysis_leak`, 200 changing analyses, no plateau) ≈ 43.8 MiB
  per 1000 keystrokes, ×(1 + open dependents) since `reanalyze_dependents`
  re-analyzes every other open document per surviving edit.
- **But the named `'static` leaks are small**: the entry source
  (`document.rs:310`) + entry AST (`lib.rs:367`) + per-package display names
  total a few KiB/analysis. Most of the 44.8 KiB is allocator retention from
  rebuilding and dropping a whole `Program` (the reachable std) every call.
  Freeing the named leaks alone recovers little.
- **The real unbounded leak lives in macro expansion, and the harness never
  reaches it**: `parse_generated` is uncached for expression-position macros
  (`macros.rs:1284`), gensym-stamped item macros (`:1331`), and `[service]`
  inputs (`:1000`) — each leaks its parse per analysis. Only `reactive.vl`
  uses gensyms today, so *every UI project* (kolt) re-leaks the reactive
  framework's expansions on every keystroke. The module-loader and
  world/expansion caches are already content-keyed and bounded — the old
  "leaks per module" note is fixed; these three sites are the stragglers.
- **There are no global id counters.** `Id`/`TypeId`/`SourceId` are
  per-analysis fields reset each run. The incremental blocker is that ids are
  minted **densely in one whole-program traversal order** — an edit renumbers
  everything after it, so no prior analysis is partially reusable. Changing
  that touches ~1400 reference sites across every post-parse stage.
- **The LSP's per-keystroke floor** is one full analysis of the reachable std
  (~150 ms measured on a trivial 330-byte file), regardless of the edit.

## 1. The reframe

E3's two halves are really three, in sharply different weight classes:

1. **Close the true leaks** — small, bounded, do now (§2).
2. **Stop re-analyzing the unchanged prelude** — the actual latency *and*
   RSS-churn win, achievable **without touching the id model** (§3).
3. **True incremental analysis** (stable/generation-scoped ids, the ~1400-site
   cross-cut) — explicitly **deferred** (§4); Phase 2's ceiling decides if it
   is ever needed.

## 2. Phase 1 — leak closure + honest instrumentation (S)

- Content-key the three uncached `parse_generated`/`run_service` sites, same
  pattern as the existing `PARSES`/`EXPANSIONS` caches. A stamped expansion's
  text is identical across keystrokes for an unchanged site, so caching turns
  per-keystroke leaks into bounded per-distinct-content entries — the same
  transition the module loader already made.
- Add per-`Box::leak`-site byte counters (a tiny `leaked_bytes(site)` tally,
  test-only surface) so leak claims are *measured*, not RSS-inferred.
- Re-shape the harness: keep the RSS number as a report, but **assert on the
  counters** (bounded leaked-bytes per analysis after warmup) — RSS is too
  noisy to gate on. Un-`#[ignore]` it as the Phase-1 pin.
- Success: counted leak per changing-analysis ≈ entry-source + entry-AST only
  (file-size-proportional, freed... no — still leaked, but *named and
  measured*; eliminating them entirely is Phase 2's side effect for std and a
  recorded refinement for the entry).

## 3. Phase 2 — the prelude checkpoint (M)

The floor cost is re-analyzing identical std sources every call. Ids are
deterministic — identical inputs analyzed in identical order mint identical
dense ids — so the analyzer state **after the always-loaded prelude (and the
workspace's stable dependency set) is byte-reproducible**. Therefore:

- **Snapshot** the analyzer immediately after prelude + dependency loading,
  **clone per analysis**, and analyze only the entry (and changed package
  modules) on top. The clone is indistinguishable from a fresh re-analysis by
  construction (same ids, same maps), so no downstream stage can tell.
- **Keying/invalidation**: the checkpoint is keyed by the content hashes of
  everything folded into it (std sources, dependency sources, manifest shape,
  macro-limit config). Any miss → rebuild the checkpoint (exactly today's
  cost, once) and cache it. The LSP holds one checkpoint per project; the E12
  watch loop holds one per leg.
- **Preconditions to verify at S-time** (the implementation order's first
  job): the `Analyzer` is field-inventory cloneable (maps/vecs of ids and
  leaked `&'static` refs clone shallowly and remain valid — leaked data is
  immortal by definition); no field hides analysis-order state that differs
  between "cloned" and "re-run" (the S2a lesson: enumerate fields
  exhaustively, no catch-all assumptions); thread-local overlays stay
  orthogonal.
- **Expected win**: the ~150 ms trivial-file floor collapses toward
  clone-cost + entry-only analysis; RSS churn (the fragmentation driver)
  drops by the same factor; E12's parse cache composes (parse skipped, now
  analysis skipped too — a watch round approaches entry-proportional).
- **Measure**: the leak harness's wall clock per analysis before/after, plus
  a kolt-shaped fixture (a project importing `std::ui`/`reactive`) so the
  gensym path and a realistic reachable set are in the measurement.

## 4. Phase 3 — deferred: true incremental (stable ids)

**Now the sole path to the per-keystroke floor** (the Phase 2 stop proved the
82% lives in `build()` + the whole-program checks, not in loading). It is an
XL cross-cut (~1400 id sites, every post-parse stage, the
dense-`SourceId`-indexes-`sources` assumption). Recorded, not planned; take
it only when the ~88 ms warm floor demonstrably hurts on real projects.
Concrete blockers found by the Phase 2 attempt (2026-07-21), recorded so the
eventual design starts from evidence:

- `build()` **clones** (not drains) `prepped_imports`/`prepped_locals`/
  `prepped_type_locals`, so a second `build()` over a reused base would
  re-resolve std imports and **double-increment `reference_count`** — any
  delta design must make resolution idempotent or drain-once.
- The constraint fixpoint **mints type ids mid-resolution** — freezing a std
  base means new ids must not collide with or renumber frozen ones
  (generation-scoped ids, with the frozen base as generation 0).
- ~25 whole-program checks iterate everything; each needs an entry-scoped
  form (or a per-generation partition) to stop re-checking std.

## 5. Order and gates

Phase 1 → Phase 2, each suite-gated with the harness pin tightening as it
goes; docs untouched (internal); `caching-plan` gains a pointer to this file.
Phase 2 lands behind a differential guard in the spirit of the house rule:
a test that analyzes a corpus of programs both ways (fresh vs
checkpoint-cloned) and asserts identical diagnostics + identical emitted JS —
the "no downstream stage can tell" claim, pinned rather than argued.

## 6. The std-tax arc (2026-08-02): E28+E30 fold into Phase 3

The suite-speed audit's last two levers (backlog E28, the LSP fixture
repetition; E30, inference's repeated std analysis) turned out to be this
document's subject wearing test-suite clothes, and E25's nextest landing
(per-test processes) forecloses every in-process amortization for the
suite. What remains — for the suite, the LSP keystroke, the CLI build, and
the playground alike — is cutting the absolute per-analysis cost. That is
Phase 3. E28 and E30 are closed as separate items; this section is their
continuation and the arc's design surface.

### 6.1 The 2026-08-02 measurements (v0.22.1-era tree, 16-core WSL2)

The tax is FIXED and it is everything:

| entry program                  | per-analysis wall |
|--------------------------------|-------------------|
| `fun main() {}` vs EMPTY std   | **0.5 ms**        |
| `fun main() {}` vs real std    | **~115 ms**       |
| one `import std::print`        | ~120 ms           |
| several imports (shared, time) | ~160 ms           |
| 30-line struct/impl program    | ~115 ms           |

User-program size is free at this scale; 100% of a trivial compile is std
processing (54 files / 11,875 lines on disk; the always-loaded core set's
closure is ~21 files / ~4,900 lines). First-touch of a deeper module adds a
one-time process spike (+50–350 ms) from the parse cache filling.

The phase split (`VILAN_PHASE_TIMING`, landed with this section — one
stderr line per analysis, pinned in diagnostics.rs beside the leak line),
trivial entry:

| phase                              | ms  | share |
|------------------------------------|-----|-------|
| load + walk (std module bodies)    | ~19 | 16 %  |
| `build()` fixpoint (std+entry)     | ~44 | 38 %  |
| in-analyze whole-program checks    | ~30 | 26 %  |
| post-passes (contexts, async, …)   | ~22 | 19 %  |

Phase 2's stop-measurement (§0) is thereby reconfirmed on today's tree:
~84 % of every compile re-solves and re-checks unchanged std. Parsing is
already content-cached process-wide (`parse_clean_cached`) and was 3.7 % of
a compile when measured; disk reads, hashing, module probes, scope minting,
name resolution, macro registry rebuild, `build()`, and every check repeat
per call.

Suite accounting: ~2,000+ analyses per full run × ~110 ms ≈ a third of the
suite's ~915 CPU-seconds under nextest. Production accounting: the same
~115 ms floors every LSP keystroke re-analysis, playground compile, and CLI
build.

### 6.2 What is REJECTED, with reasons

- **In-process sharing for the suite** (E28/E30 as filed): dead under
  nextest's per-test processes. In-process reuse remains valuable for the
  LSP/watch/wasm (S4 below) but cannot move the suite.
- **A disk-serialized analyzed-std snapshot**: `Program` is ~90 interleaved
  id-keyed tables full of `&'static str` leaked references; a faithful
  serializer is its own XL with a worse correctness profile than
  generation-scoped reuse, and it duplicates what S1–S3 buy in-process.
  Revisit only if the suite still hurts after S3 (each nextest process
  would deserialize instead of analyze std).
- **Shrinking the always-loaded core set**: the ~21-file closure is the
  language's ambient surface (List, operators, ranges, option/result,
  string methods); gating it is a semantics change, not an optimization.

### 6.3 The slice plan

- **S0 — instrument (SHIPPED with this section)**: `VILAN_PHASE_TIMING`
  phase line at the `analyze` chokepoint + the `analyze_source` post-pass
  line; default-off; pinned red-first in diagnostics.rs.
- **S1 — entry-scoped whole-program checks** (~30 ms in-analyze + part of
  the ~22 ms post-passes): classify each of the ~25 checks by iteration
  direction — *definition-site* (iterates entities, diagnoses the iterated
  entity: skippable for std-sourced entities, since shipped std is clean —
  pin that invariant), *use-site-driven* (iterates std definitions to find
  entry uses: NOT skippable), *instantiation-driven* (diagnoses per
  entry-forced instantiation: NOT skippable). The classification table is
  the slice's first deliverable; only the provably definition-site checks
  gain the `source_ranges` filter. Gate: the §5 differential guard,
  promoted from Phase 2's plan to a permanent suite test — a corpus
  analyzed both ways asserting identical diagnostics + identical emitted
  JS. Stop-bar: if fewer than half the checks classify as definition-site,
  reweigh before landing complexity.
- **S2 — resolution idempotence** (blocker 1, groundwork): `build()` drains
  or idempotently re-resolves `prepped_imports`/`prepped_locals`/
  `prepped_type_locals`; `reference_count` survives a second `build()`
  unchanged. Behavior-neutral alone; pinned by a double-build unit test.
- **S3 — the frozen std base** (blockers 2+3, the XL core): std loads,
  walks, and `build()`s ONCE per process into a generation-0 base;
  per-analysis work is an entry-delta fixpoint over it (generation-scoped
  type/entity ids so new ids never renumber frozen ones) plus the S1
  entry-scoped checks. In-process only — which under nextest still pays
  the base once per test process, so the SUITE'S win from S3 is bounded;
  the LSP keystroke, watch loop, wasm playground, and any multi-compile
  process get the full collapse toward entry-proportional cost. Take S3
  only with S1+S2 landed and measured; its design doc extends this file.
- **S4 — wire the consumers**: content-keyed base invalidation (std
  sources + manifest + platform + compiler version), LSP/watch/wasm reuse,
  and the differential guard running over the corpus in CI.

### 6.4 Honest expectations

S1 alone: ~25–35 ms off the ~113 ms trivial floor (suite CPU −5–8 %; LSP
keystroke ~115 → ~85 ms). S3: the floor approaches load+walk+entry
(~20–25 ms warm, less if S3 also freezes the walk) for every multi-analysis
process, and the suite's per-test cost approaches one base build per
process. The suite's remaining bound after S3 is the leak-plateau critical
path (52 s under load), which no analyzer work moves — the suite case for
this arc is real but secondary; the LSP/playground/CLI latency case is the
primary one, exactly as §4 recorded ("take it only when the warm floor
demonstrably hurts").

### 6.5 S1 SHIPPED (2026-08-02): the classification, the gate, the numbers

**The classification** (four independent read passes over every check;
conservative on doubt). Verdicts and, for filtered checks, the skip key:

| pass | verdict | skip key |
|---|---|---|
| check_readonly_mutation | definition-site, FILTERED | assignment expr |
| check_mutable_arguments | definition-site, FILTERED | call site (never callee) |
| check_mutable_references | definition-site, FILTERED | reference expr |
| check_view_bindings | definition-site, FILTERED | binding id |
| check_view_arguments | definition-site, FILTERED | call site |
| check_view_value_reads | definition-site, FILTERED | expr (fixpoint stays whole-program) |
| check_must_use | definition-site, FILTERED | function + block (statement's home) |
| check_element_attribute_shadowing | definition-site, FILTERED | call site (synth nodes file under the writer) |
| check_view_escape | definition-site, FILTERED | expr / function / closure (3 sweeps) |
| check_invalidation (+async captures) | definition-site, FILTERED | function / closure (3 sweeps) |
| check_reseat_escape | definition-site, FILTERED | assignment expr |
| check_resource_any_coercion | definition-site, FILTERED | site's home (call site / binding / function) |
| check_trait_conformance | definition-site, FILTERED | the IMPL, never the trait |
| check_wire/hashable/partialeq_boundary | definition-site, NOT filtered | queues carry no id — widen at collect (S1b, if ever worth it) |
| check_rpc_signatures, check_expose_fields | definition-site, NOT filtered | same queue shape |
| check_container_resource_arguments | definition-site, NOT filtered | same queue shape |
| check_generic_bound_satisfaction | instantiation-driven | — |
| check_resource_generic_instantiations | instantiation-driven | — |
| check_hmr_transfer_bounds | use-site-driven | — |
| check_async_drops / check_context_drops | entry-dependent (async/context inference) | — |
| platform_color::check | use-site-driven (anchors at deepest USER frame) | — |
| init_order::check_cycles | whole-graph (mixed cycles can anchor in std; §5(b) over-approx admits std-only components) | — |
| check_drop_impls | data producer (drop_methods) + rider diagnostic | untouched |
| plan_resource_drops / build_drop_glue | data producers (emitted JS, call-graph edges) | untouched |
| check_resource_moves | data producer (resource_value_places → clone_sites) | untouched |

**The mechanism**: std modules loaded from DISK are recorded as frozen
sources (never the entry — even when the entry IS a std file — and never an
LSP-overlaid buffer); after `build()` the frozen sources seal into sorted
entity-id ranges (`seal_frozen_ranges`) and `frozen_entity` is a binary
search, cheap enough for per-expression asks. The full-scan override
(`set_full_scan_checks`) seals an empty index. Unattributed and
derived-source ids are never frozen — conservative by default.

**The gate** (`check_scope_differential.rs`, permanent): (1) the std-clean
invariant — an import-everything entry per platform (bare module imports;
`null` is a keyword and rides the always-loaded set), forced full-scan,
zero diagnostics AND zero warnings; (2) the whole corpus analyzed both
ways agreeing byte-for-byte on diagnostics, warnings, and emitted JS;
(3) the frozen-source recording pinned. Both plant directions proven red:
an inverted filter loses an entry diagnostic (CLI probe), and a mutability
bug planted inside std trips the invariant (with a cascade through
compare.vl's PartialEq machinery that shows how load-bearing the
invariant is).

**The measured outcome, honestly**: warm trivial-entry split went
19/44/**30**/21 → 19/44/**23**/21 — the checks phase −23 %, the total
floor ~113 → ~106 ms (−6 %). The §6.3 estimate (25–35 ms) was wrong about
composition: most of the "checks" window is the UNSKIPPABLE core —
instantiation-driven passes, the resource data-producers, and
`infer_borrows`/`infer_bumps` which share the window but are inference,
not checks. The six queue-based derive checks stay unfiltered (their
queues are declaration-sized; the win would be noise). S1's lasting value
is the differential gate and the frozen-source machinery — the safety
rail S2/S3 run on; the big money stays in `build()` (44 ms) and the
load+walk (19 ms), which are S3's targets.

### 6.6 S2 SHIPPED (2026-08-02): resolution is drain-once

The Phase-2 record named three cloned queues; the mechanical truth was
**five** — `prepped_imports`, `prepped_locals`, `prepped_type_locals`,
`prepped_type_static_accessors`, and `prepped_static_accessors` all
`.clone()`d at consumption (three sibling queues were already
`mem::take`n, so the drain-once direction was established practice). All
five now drain. `prepped_type_locals` was the one with a post-build
reader — conformance's `= Self` disambiguation reads the written
spellings — so its drain retains the projection in a new
`written_type_spellings` field, which ACCUMULATES across builds, matching
the drain-once contract S3 needs.

The pin (`build_idempotence.rs`): a `set_build_twice` test switch makes
`analyze` run `build()` twice back-to-back, and a four-program battery —
use-once alias elision (copy-elision reads `reference_count == 1`, so a
double-increment changes emitted JS), a failing import (was: reported
twice), item imports + static accessors (the member-count loops), and an
unused-import warning — must observe identical diagnostics, warnings, and
JS both ways. Plant-proven: reverting one drain to `.clone()` turned the
pin red on exactly the predicted mechanism ("use-once alias elision:
emitted JS differs").

A finding worth recording: with the queues drained, a full second
`build()` — constraint fixpoint included — is already observationally
neutral across the battery. The re-entrant-build contract S3 needs holds
today at the observation level; S3's remaining work is the id-space side
(generation-scoped ids so a second build's minting never renumbers the
frozen base), not behavioral neutrality.

### 6.7 S3 design (2026-08-02): the frozen base is a CLONE, and the
### groundwork is landed

Three evidence streams (two read sweeps plus an empirical reorder probe run
against every gate in the workspace) settle the mechanism and stage the
work. Landed with this section: the `build()` split into named phases —
`resolve_world` (queue drains + the constraint fixpoint) and
`finalize_build` (the commit tail: give-up defaults, iterator/operator
resolution, end-of-fixpoint diagnostics) — plus the `set_early_std_build`
probe switch with a `VILAN_EARLY_STD_BUILD` env arm that lets the entire
suite vote on two-phase neutrality. Default behavior is byte-identical
(`build()` = the two phases in sequence).

**Mechanism: clone-the-base, not rollback.** The mutation inventory is
dispositive. Entry-side analysis writes into std-owned state in every
class: accumulating appends into std IR (`context::thread_contexts`
PUSHES hidden parameters onto std functions and context arguments onto
std call sites — a shared base doubles them per analysis), per-use
`reference_count` accumulation on std definitions (three consumers turn
drift into emitted-JS changes), in-place fills of std-minted TypeIds
(slot unification; first-call-site-wins closure-parameter fill), and one
BACKWARDS write — name-resolution memoization caches entry ids into std
scopes. A rollback design would need a per-class repair list and would
still be wrong the first time a class is missed; a per-analysis deep
clone of the built base retires the entire table. Phase 2's clone-cost
concern is now a measurement task, not a blocker: the id problem
disappears under clone (each analysis's ids continue from the base's
mark; nothing renumbers).

**The big fear is retired.** Trait-dispatch candidate sets are never
frozen at build time: `generic_dispatch`/`bound_dispatch_traits` record a
name plus a declaration-local trait, and every candidate ENUMERATION
(async_infer, the context pass's v0.21.x coverage closure, call_graph,
init_order, platform_color) recomputes from the finished `Program` after
the last build. A std-first build cannot freeze a smaller dispatch world.

**The real residual hazards, ranked** (from the constraint sweep):
1. `Failed`-is-permanent: a std-side constraint that fails in phase 1 is
   dropped, never retried — and the arms that can fail for want of an
   ENTRY impl on a std-visible subject are exactly the impl-scanning ones
   (`MethodCall`, `TryAssert`, `Lift`, `reconcile_type`'s
   `type_implements_trait` arm). Phase 1 must treat those failures as
   deferrals (a phase flag suppressing the terminal diagnostic).
2. `finalize_build` must be structurally once-per-analysis — its
   whole-map sweeps would double-diagnose on a re-run; today only the
   call wiring guarantees it.
3. **The chained-call stall — the first concrete instance of the §4 id
   blocker, now with a minimal repro.** Under a std-first
   `resolve_world`, `points.map(|p| p.name).map(|s| s.len())` stalls
   ("type of variable could not be resolved") while the corpus, the S2
   battery, the let-split variant of the same chain, and 1215 of 1216
   inference tests pass. Falsified by experiment: it is NOT the commit
   tail (persists under the split), NOT re-entry fragility (build_twice
   is clean), NOT stale deferred-queue bookkeeping (persists when
   deferred constraints return to the queue at phase end). What remains
   is the state class both sweeps independently flagged: in-place fills
   on std-minted signature/slot TypeIds — std's own internal usage fills
   them in phase 1 where the monolithic build would have let the entry's
   call participate. The fix class is per-use freshening / copy-on-write
   of base-minted type ids — precisely the "generation-scoped ids" work
   §4 predicted, no longer hypothetical.

**Also found, filed independently**: name-resolution memoization plus the
std-scopes-parent-to-global layout means `std::math::<any entry global>`
RESOLVES today — an entry-defined function is reachable through any std
module path (repro in the backlog entry). A generation-aware memo fixes
it; it must be fixed regardless of S3.

**The slice plan from here:**
- **S3b — the id boundary**: pin the chained-call repro red-first against
  a two-phase pin, then make entry-side unification freshen (not fill)
  base-minted TypeIds. This is the XL kernel; it is also what makes the
  clone SAFE to reuse across analyses of different entries.
- **S3c — the base cache**: snapshot the built std+deps world
  (post-`resolve_world`, pre-entry) behind a content key (std sources +
  manifest + platform + workspace + compiler version); per analysis:
  clone, walk entry, `resolve_world`, `finalize_build`, checks. Bypass
  when the entry IS a std file or any std path is overlaid. Measure the
  clone; the Phase-2 stop-bar logic applies (clone cost must undercut
  the ~63 ms it replaces).
- **S3d — wire and gate**: the reorder probe's whole-suite vote
  (`VILAN_EARLY_STD_BUILD=1`) becomes the standing differential, the
  scope-memo bug fix lands, and the LSP/watch/wasm consumers adopt the
  base.

### 6.8 S3b progress (2026-08-02): the stall is localized; the kernel is
### pinned open

The freshen-not-fill hunt narrowed the chained-call stall by systematic
falsification, each step instrumented and measured on the live repro:

- **Not below-mark type-map writes.** A generation mark at the phase
  boundary plus tracing on all candidate write sites (slot unification,
  closure-parameter fill, first-call-site-wins) recorded ZERO
  cross-generation writes during the entry phase — the design's leading
  suspect is innocent as charged, at least for this repro.
- **Not slot machinery.** Std-internal slot fills are byte-identical in
  both modes (the same two internal slots fill with the same generics).
- **The actual shape**: map#1's `MethodCall` resolves in both modes and
  mints a fresh per-call result element (`List<fresh>`); the monolithic
  build fills that element LATE — observably, the inner `len` call
  resolves between map#2's retry attempts, i.e. the first closure's
  return landing `U → str` propagates into the per-call element after
  call#1 already resolved. Under two-phase, that late fill never fires:
  map#2 retries against `List<fresh-but-never-filled>` and the whole
  chain stalls to the residual diagnostics.
- **Next instrument**: the `ClosureReturns` constraint flow and the
  method-call result-instance construction — find the event that writes
  the closure's landed return into the per-call element monolithically,
  and why its trigger condition never holds when std's fixpoint ran
  first. The fix lands there, at the root.

The blocker is pinned as
`two_phase_build_resolves_chained_generic_calls` (`build_idempotence.rs`,
`#[ignore]`d per the house convention — red when run, un-ignored when the
kernel lands). All temporary instrumentation is removed; the probe
switches (`set_early_std_build` + env arm) remain the standing
instrument.

### 6.9 S3b KERNEL LANDED (2026-08-02): the stall was a latent fixpoint
### bug, and the fix is three lines of exit condition

The §6.7 prediction ("per-use freshening of base-minted TypeIds") was
WRONG, instructively: the generation-mark tracing had already shown zero
cross-generation writes, and the final localization run proved every
piece of resolution state byte-equivalent across modes — same member,
same generics, complete and CORRECT substitution (`U → str` recorded),
pristine declared return type, and the subject even inferring `List<str>`
on its final attempt. The failing ingredient was WHEN that final attempt
ran: after the fixpoint had already declared quiescence.

**The mechanism**: the fixpoint's exit breaks when a backstop pass
resolves nothing and wakes nothing — but a deferred attempt can WRITE
types without either signal firing. A chained `.map().map()`'s second
call types its closure argument's parameters (via
`infer_closure_args_against_params`) and then correctly defers at the
incomplete-bindings guard; those parameter fills are exactly what its
NEXT attempt needs, and neither the resolution count nor the wake scan
sees them. Monolithically, std's unrelated constraint churn granted the
extra rounds by accident; the two-phase probe removed the churn and
unmasked the early exit. A one-round forced-retry experiment confirmed
it: `progress=true` on the bonus round, chain clean.

**The fix, at the root**: `type_map_writes` counts every write into
`type_id_to_type_map` (11 sites, one counter bump each); quiescence now
additionally requires the counter unchanged across the backstop retry.
The `max_iterations` outer bound keeps a write-without-progress cycle
finite. This is a LATENT-BUG FIX for the monolithic compiler too — any
program whose closure-typing writes landed in the fixpoint's final quiet
round was at the mercy of constraint-order luck.

**Proof**: the `two_phase_build_resolves_chained_generic_calls` pin is
un-`#[ignore]`d and green — red-first proven live, and the plant
(reverting the writes condition) turns exactly it red again. The
whole-workspace two-phase vote (`VILAN_EARLY_STD_BUILD=1`, every gate)
is the acceptance instrument. With the kernel landed, S3c (the base
cache + measured clone) has no known blocker.
