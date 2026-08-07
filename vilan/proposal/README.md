# Proposal directory index

One line per file in this directory: name, status, one-clause description.
Status is derived from each file's own header/status block (read, not the
whole file), as of 2026-08-03, last re-verified 2026-08-04 (records-polish
sweep: every file rechecked against disk, one duplicate row merged, ten
stale lines refreshed — see the file's own history for detail). Statuses
in play: **shipped arc** (built,
closed, the file is now a design record), **active** (a living reference,
edited as the project evolves), **draft** (proposed, not yet ratified or
built), **deferred** (designed, deliberately not built, with a trigger),
**design record** (historical analysis or partially-actioned notes, not
tracked as an open backlog item), **historical / superseded** (stale,
kept for context only).

Cycle proposals through 2026-08-04 are merged and carry real status lines below.

The open-work tracker is [`backlog-2026-07-18.md`](backlog-2026-07-18.md)
(status convention documented in its own header); [`backlog.md`](backlog.md)
is the historical record everything shipped gets moved into.

| File | Status | Description |
|---|---|---|
| `affine-moves.md` | shipped arc (2026-08-04) | A consuming call is a move — the B60 loan-consumption rule (a body may only consume what it owns); §7 closes B62, §8 closes the B63 residuals, §9/§9.5 close the B65–B68 accounting holes. |
| `ambient-owner.md` | shipped arc (2026-07-07) | The ambient owner / `comp` ergonomic layer over `std::reactive`. |
| `analysis-reuse.md` | shipped arc (std-tax arc complete 2026-08-03) | The E3 arc: per-analysis leak closure, then incremental analysis via a cached, frozen std base. |
| `analyzer-refactor.md` | design record (partially actioned) | A punch list of analyzer structural weaknesses behind a class of generic-inference bugs; several items done, the queue-v2/interning items unscheduled. |
| `argument-tail-descent.md` | shipped arc (2026-08-01, backlog B43) | A statement's split descends through a call's last argument, matching `Split::Tail`. |
| `async-polymorphism.md` | shipped arc (Part A 2026-07-17, Part B 2026-07-18) | Async polymorphism: `sync` contracts, adaptation by monomorphized asyncness, structured-concurrency scopes; Part C (the parallelism spine) is a design record, deferred (backlog §J.1) with no driver application yet. |
| `b33-emission-order.md` | shipped arc (2026-07-25) | Module-level binding emission in dependency order, cycles diagnosed. |
| `backlog-2026-07-18.md` | active | The distilled open-work tracker — read this, not `backlog.md`, for what's outstanding. |
| `backlog.md` | historical record | Superseded planning surface; every shipped item's full body lives here, including everything moved out of the distilled file. |
| `bits-and-bytes.md` | shipped arc (2026-07-02) | The binary floor: hex literals, bitwise/shift operators, `std::bytes`. |
| `bundle-splitting.md` | shipped arc (2026-08-04) | Route-chunk splitting from whole-program reachability (A16); S1 (measure-first), S2 (emission + gate), S3 (loading), S4 (consumer sweep) all shipped — §5's v1 non-goals stand, §11 scopes a v2. |
| `chain-seam-split.md` | shipped arc (2026-08-01, backlog B48) | A chain splits when a non-final link renders across lines, not just on width. |
| `claims-and-epochs.md` | shipped / foundational (ratified 2026-07-18) | The one law behind the memory model (claim validity via epochs); frames C4 as the model's last major change. |
| `compiler-bindings.md` | design record (idea sketch, unscheduled) | Unshaped idea for compiler-hosted bindings; not designed to the project's proposal bar. |
| `composite-spanning-split.md` | shipped arc (2026-08-01, backlog B49) | A list/struct literal holding a spanning element splits regardless of width. |
| `const-eval.md` | shipped arc (2026-07-10; G2 tail mostly shipped 2026-08-04; G3 shipped 2026-08-04) | `const` as a compile-time-evaluation language feature, the asset-emission channel, and inferred `const` folding (backlog G3, §9); G2's deep-failure spans and full LSP memoization remain open (§8). |
| `destruction-impl-plan.md` | shipped arc (2026-07-19) | The C4 implementation ledger: all five Tier-1 destruction slices. |
| `destruction.md` | shipped arc (Tier 1, 2026-07-19; Tier 2 tracked as backlog C1/F4) | Deterministic destruction — the owned-resource class, scope-end `Drop`. |
| `diagnostics-ledger.md` | active (living ledger) | Every `diagnostics.push` site with its audit verdict; updated per audit batch. |
| `diagnostics-standard.md` | shipped / foundational (accepted 2026-07-16) | The rules a diagnostic message must follow, plus the audit plan the ledger executes. |
| `distribution.md` | shipped arc (2026-07-25, provisioned 2026-07-29) | npm/Marketplace/Open VSX/Homebrew distribution (F7) plus the project-model deferrals (F5). |
| `docs-site.md` | shipped arc (2026-07-12) | The rendered docs site: mdBook, custom grammar, GitHub Pages publishing. |
| `draft-reconnect.md` | shipped arc (2026-08-04) | `Draft` re-push on reconnect and the debounced commit — A14's last reactive residual. |
| `documentation.md` | shipped arc (Phase 3, 2026-07-12) | D1a, the user-facing reference docs — what the book covers and its phasing. |
| `element-syntax.md` | shipped arc (2026-08-01, all five slices) | HTML-flavored markup sugar lowering to the `view` chain. |
| `element-clones.md` | shipped arc (2026-08-04) | Rule 1's copy lands at the store — constructions, `own` arguments, returned by-value parameters (A20, B54); §7 holds the B64 find. |
| `expression-lifting.md` | shipped arc (2026-07-16) | `a? + 10` / `a? + b?` — the `?` lift operator inside expressions (B11 tail slice). |
| `fixed-arrays.md` | shipped (core); tail deferred, see backlog I2 | `[T; n]` fixed-length arrays; const-generic lengths left for §7. |
| `fn-coercion.md` | shipped arc (2026-07-11) | Named functions used directly as closure values. |
| `frontend.md` | shipped arc (2026-07-22) | The handwritten recursive-descent frontend that replaced chumsky. |
| `hashable-keys.md` | draft / proposed (2026-07-14) | Hashable keys for `Map`/`Set` beyond primitives; tail tracked in the backlog trailer. |
| `hmr.md` | shipped arc (2026-07-21) | Hot module replacement closing the dev loop (A13). |
| `kolt-migration.md` | active (living document) | The kolt→vilan migration driver; tracks the reference app's porting status. |
| `lazy.md` | draft, ratified but deferred (2026-07-21/22) | `lazy` parameters and module bindings — deferred by user call until real demand appears (backlog B30). |
| `library-packages.md` | shipped arc (2026-06-23) | Library packages, replacing old roadmap item P4. |
| `local-shadowing.md` | shipped arc (2026-07-28) | Positional visibility for local bindings (B34), including the same-scope shadowing fix. |
| `lsp-snapshot-consistency.md` | shipped arc (approved/implemented 2026-07-28) | Semantic tokens never outrun the analysis they're served from. |
| `macro-engine.md` | shipped arc (complete 2026-07-07) | The macro engine's scheduled phases; a v1-beyond tail is tracked in the backlog trailer. |
| `macros-post-parse.md` | deferred (design complete, deferred 2026-07-16) | The normalized `macro_std` output contract (backlog G4) — designed, not built. |
| `memory-management-impl-plan.md` | shipped arc (Phases 1–6 essentials) | Implementation ledger for `Arena`/`Handle`/`Shared<T>`. |
| `memory-management.md` | historical / superseded | Superseded by `memory-management-rev-1.md`. |
| `memory-management-rev-1.md` | shipped arc (through Phase 6 essentials) | The memory-management design, revision 1 — largely superseded in turn by `claims-and-epochs.md` + `destruction.md`. |
| `method-resolution.md` | draft — awaiting review | A deliberate method-resolution precedence rule + ambiguity diagnostic (B57); today's order is registration-order accident. |
| `mut-parameters.md` | shipped arc (2026-08-03) | `mut` parameters — local rebindability of a callee's copy (backlog H9). |
| `numeric-types.md` | shipped arc (2026-07-07); tail in backlog trailer | Sized numeric types (`u8`…`i64`/`f32`); native-width tail recorded, not filed as an open item. |
| `org-migration.md` | shipped arc (migration complete 2026-07-29; tail resolved 2026-08-03) | The move to the `vilan-lang/vilan` org (F9), Pages tombstone, owner-string sweep. |
| `p6-followups.md` | shipped arc (complete 2026-07-03) | The post-P6 (transport/RPC) completion ladder. |
| `platform-coloring.md` | draft / proposed | Function-granular platform checking, successor to `platform-model.md`'s module-granular check. |
| `platform-model.md` | shipped arc (2026-06-23) | The build model: backends, platforms, and layers. |
| `process.md` | draft — awaiting owner review | Release cadence (weekly train + urgent triggers), branch protection, merge strategy, contribution scaffolding, and the alpha→beta promise — filed from the owner's 2026-08-07 process ask. |
| `reactive-batching.md` | shipped arc (2026-07-02) | Deferred notification and the `batch` turn. |
| `reactive-turns.md` | shipped arc (2026-07-09) | Reactive turns — scoped flush, async turns, replacing global auto-flush (A6). |
| `releases.md` | active | Installation, versioning, and the release pipeline — the process this project actually runs. |
| `requirement-polymorphism.md` | shipped arc (2026-08-02, backlog B51) | The owner-coverage fence follows instantiation chains through generic forwarders. |
| `ret-checking.md` | shipped arc (2026-07-04) | Return-position type checking (backlog B10). |
| `roadmap.md` | historical / superseded | Pre-lettered-section ranked backlog; superseded by `backlog-2026-07-18.md`'s A–J scheme. |
| `router.md` | shipped arc (settled 2026-07-11) | `std::router` — history-API routing (backlog A10). |
| `rule4-completion.md` | shipped arc (complete 2026-07-19) | The `borrows` root-set and the `bumps` effect completing rule 4 (C6 + C10). |
| `signature-layout.md` | shipped arc (2026-08-01, backlog B46) | `fun` signatures reach the width rule as a declaration site. |
| `specification.md` | shipped (design record; the spec itself now lives in `vilan/docs/spec/`) | D1b, the language specification plan. |
| `split-comment-attachment.md` | shipped arc (2026-08-01, backlog B41) | Mid-construct comments attach to the split element they precede instead of orphaning below the statement. |
| `ssr.md` | shipped arc (v1, 2026-07-23); open tail, see backlog A7 | Server-side rendering — render and replace, not hydration. |
| `suite-speed.md` | shipped arc (audit + slices, 2026-08-02; two flakes closed 2026-08-04) | The measured test-suite speed profile and the slice list that reclaimed it (E21/E25–E30); §5 closes two load-dependent flakes, harness clocks and a fixed port (E32/E33). |
| `top-level-await.md` | draft | Top-level await, the J3 design half — the shipped diagnostic is call-shaped, so module-level `await` already compiles and already miscompiles (Node classification, HMR thunk); recommends closing the hole rather than opening the feature. |
| `transparent-references.md` | shipped arc (2026-06-21) | Implicit place / explicit value semantics for references. |
| `transport-robustness.md` | shipped arc (2026-07-11) | Reconnect, backoff, and re-subscription for the transport layer (K6). |
| `transport-rpc.md` | shipped arc (implemented, the whole arc) | The transport/RPC library's model and philosophy (roadmap P6). |
| `try-and-lift.md` | shipped arc (2026-07-04; B11 tail partly closed 2026-08-04) | `!` and `?` — early return and lifted chains; the bare-`?` trait path (§11) shipped 2026-08-04, closure `!` and `Signal`/`Promise` `Lift` opt-ins (§12) remain open, design-gated. |
| `type-solver.md` | shipped / closed (design record) | Type-solver capability characterization (backlog B1) — analysis complete. |
| `ui-styling.md` | shipped (core 2026-07-10; tail shipped 2026-08-04) | Typed atomic styles, compiled (`std::style`); the property tail (backlog A8), including value types, shipped 2026-08-04 — only the A7 (critical CSS) / G2 (dead-style elimination) pieces remain open. |
| `validating-from-json.md` | shipped arc (2026-07-14); tail in backlog trailer | Per-type `from_json` returning `Result`, never garbage. |
| `variadic-generics.md` | shipped (core + spread parameters, 2026-08-04); tail tracked as backlog B3 | Variadic generics via mapped tuples over flat storage; §S spread parameters (`...items: T`) shipped 2026-08-04 (§S.9 ship record), `keyof`/elision remain deferred. |
| `view-invalidation.md` | shipped arc (2026-07-09) | Views and invalidating events — rule 4 completed, `await` included (C3 + C2's static half). |
| `watch-mode.md` | shipped arc (2026-07-02) | `--watch` mode across `build`/`check`/`test`/`run`. |
| `web-playground.md` | shipped arc (live and complete 2026-08-02) | The compiler running in the visitor's browser (D11). |
| `windows-support.md` | shipped arc (ratified + complete 2026-07-24) | First-class native Windows support for the toolchain. |
| `std-surface.md` | shipped arc (v1 cut landed 2026-08-03; flagged tail open) | Std surface audit + the missing basics — List batch, clamp, the import steer (I4). |
| `signal-update.md` | shipped arc (2026-08-03) | `Signal::update` — mutate in place, notify once (A18); design record + ship record; two of §8's residuals (B61, A19) closed 2026-08-04. |
| `iterator-adapters.md` | draft — awaiting review | Iterator adapter layer + pipeline ergonomics over `Iterable` (I3); found 4 compiler prerequisites (P1–P4) — P2/P3/P4 now fixed (B55/B56/B58), P1 and the adapter slices themselves (S1–S7) still unbuilt. |
| `bindgen.md` | draft — awaiting review | Generate `external` bindings from TypeScript `.d.ts` headers via oxc (E31). |
| `capture-clones.md` | shipped arc (2026-08-03) | Pattern captures copy on both compile paths — the B53 completion record; §5 holds the B59/B60 finds. |
| `canvas.md` | draft — awaiting review | Immediate-mode typed 2D canvas layer, proposed home `std::canvas` (A17). |
| `backed-enums.md` | draft — awaiting review | A variant carries the value the host speaks (`= "start"`) as a generalization of the integer discriminant (B76); the survey found three silent holes in the discriminant grammar it extends. |

**Not yet in this directory, in flight:** `optimistic-lifecycle.md` (A14's
remaining "optimistic-write → reconcile lifecycle" bullet) is drafted on the
concurrent `optimistic-lifecycle` branch (476 lines, not yet merged as of
this sweep) — no row above because there is no file on disk here yet; add
one when it lands. The backlog's A14 entry (`backlog-2026-07-18.md`) still
correctly shows that bullet as open.
