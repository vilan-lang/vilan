# Bundle splitting — route chunks from whole-program reachability (A16)

> **Status: PROPOSAL, drafted 2026-08-03** (user request 2026-07-24;
> proposal-first per the house rules). Ground truth gathered from the
> router/reachability machinery and the emission/HMR/SSR/playground
> consumers; every constraint cited below was verified in source, not
> assumed. Nothing here is implemented.

## 0. The problem and the thesis

A browser entry ships as one emitted JS file, so first load pays for every
route and view in the app. Solid-style `lazy()` fixes this *explicitly* and
is easy to forget. Vilan's whole-program compiler can infer the split: the
language already makes the seams visible (routes are an enum; each arm of
the route `match` is a compiler-known view subtree), and emission is
already demand-driven reachability (`ensure_function_emitted` from real
call sites; `reachable_bindings` from `main`; the multi-root precedent in
`transform_functions`). What is missing is only the OUTPUT side: chunk
partitioning, chunk emission, and a loading story.

**Thesis: the split is inferred, not annotated.** No keyword — `lazy` is
ratified for value deferral (B30, `lazy.md`) and must not be overloaded;
more importantly, an annotation would be the Solid mistake with extra
steps. The router `match` is the split point because it already is one.

## 1. The split unit

A **route chunk** is the set of functions (and their monomorphized
instances) reachable from exactly one arm of a **splittable route match**
and from nothing eager. Precisely:

- A *splittable route match* is a `match` whose subject is the parameter
  of a `View.swap` render closure (the canonical shape:
  `.swap(route, |current| match current { ... })` — examples/router
  app.vl:118, routing.md §"Pages swap on the route"). v1 recognizes
  exactly this shape; the nested inner-enum match (`items_layout`'s) is a
  natural v2 extension, recorded not designed.
- Roots: each arm's body subtree contributes its call sites (per-arm
  attribution partitions the closure's flat `calls` vector by containing
  `ExprMatchLeg.body` — a new partitioning step over existing call-graph
  data, `call_graph.rs` Collector + `platform_color.rs` Traversal).
- Membership: run the existing per-instantiation reachability walk once
  from the EAGER root (`main`, with every splittable arm's calls held
  out) and once per arm. A function reachable from the eager root or
  from ≥ 2 arms is **eager** (v1 does no shared-chunk extraction between
  sibling routes; "shared goes eager" is monotone, correct, and loses
  only optimization, never correctness). A function reachable from
  exactly one arm and nothing else is that arm's chunk.

**Module-level bindings never split.** Every module binding stays in the
entry chunk, in today's single global initialization order. This is the
load-bearing simplification: B33's initialization order is a global
correctness invariant (non-hoisted `const` in topological load-time
order; b33-emission-order.md §1–2), and partitioning it across
asynchronously-evaluated files would reintroduce exactly the TDZ class
B33 killed. Bindings are state and wiring — small; the weight a split
recovers is view/function code. v1 buys the win without touching the
invariant.

## 2. The loading story: the fetch gates the route signal

`View.swap`'s render callback is `sync` (ui.vl:385) — it cannot await a
chunk. The fetch therefore happens **upstream of the swap**, at the
navigation seam: the route signal's advance is gated on the chunk's
arrival.

- `std::router` grows a chunk-aware layer between `current_path()` and
  the app's route signal: when the decoded route's arm has an unloaded
  chunk, the router starts the `import()`, and the route signal advances
  only when it lands. `swap` then renders synchronously against code
  that is already present — no placeholder protocol, no signature
  change, no async render.
- **What renders while a chunk fetches: the previous route's view**,
  unchanged — the signal simply hasn't advanced. Apps that want a
  pending indicator get one hook: a router-provided
  `pending(): Signal<bool>` that flips during a fetch (bindable like any
  signal). No suspense machinery, no fallback trees.
- First navigation to a route pays one fetch; every later navigation is
  instant (chunks stay evaluated). Failure to fetch = the navigation
  does not happen + a console report v1 (an error hook is v2 surface).
- The initial route's chunk (the arm matching the URL at boot) is
  fetched before first mount — or, better, the emitter marks it for
  preload in the chunk map so the fetch overlaps module evaluation.

## 3. Emission

- **Cross-chunk references ride a runtime registry, not ESM exports.**
  The emitted entry has no exports and one flat renamed scope
  (`rename_for_scopes` is whole-program; nothing is exported today).
  Chunks would need either real module boundaries — a first, and broken
  in the playground's opaque-origin srcdoc iframe where relative
  `import()` cannot resolve — or a registry. v1: the entry assigns each
  chunk-visible function into a per-app registry object; a chunk is an
  emitted module whose evaluation registers its arm's functions and
  reads its dependencies from the registry. Blob-URL loadable,
  srcdoc-compatible in principle, renamer-compatible (the registry
  names are the stable boundary; each chunk renames internally like
  `transform_functions` roots).
- **Artifacts**: `dist/<leg>.js` (eager) + `dist/<leg>.<arm>.js` per
  chunk + the chunk map embedded in the eager bundle (URL + content
  hash per chunk, for cache busting and the preload mark). A sidecar
  `dist/<leg>.chunks.json` lists every emitted artifact so hand-written
  servers (SSR/todo examples: one hard-coded route per file today) can
  iterate instead of hard-coding. CSS stays one sidecar per leg v1
  (style extraction per chunk is v2).
- `reject_output_collisions` extends over the chunk namespace.

## 4. Mode, not default

Splitting is **opt-in per browser entry**: `[entry.<name>] split = true`
(a bool beside the existing per-entry `target`; `BuildOptions` stays
`Copy`). Single-file emission remains the default and a first-class mode
forever, because three consumers depend on it structurally:

- **HMR/watch**: whole-bundle byte-diff classification, whole-bundle
  blob swap, per-leg version counter (hmr.rs, hmr_shim.js). Dev builds
  ignore `split` v1; per-chunk swapping is recorded as the same deferred
  refinement hmr.md already carries ("measure first").
- **The playground**: `CompileResult { js, css }` single-string API,
  srcdoc iframe execution. Unaffected (splitting never applies).
- **SSR**: unaffected at first paint (server markup renders before any
  script — the strongest argument FOR splitting: only time-to-interactive
  is on the critical path). The server must serve the chunk files; the
  examples' hand-written routes adopt the chunks.json manifest when S4
  lands.

## 5. What this is not (v1 non-goals, recorded)

- No size-threshold or profile-guided splitting — route arms only.
- No nested-route sub-splitting (the inner `ItemsRoute` match).
- No shared chunks between sibling routes (shared code goes eager).
- No per-chunk CSS, no per-chunk HMR swap, no streaming/suspense.
- No `lazy` anything — that word belongs to B30's value deferral.

## 6. Slices and gates

- **S1 — the partition, analysis-only**: per-arm call attribution + the
  eager/chunk membership computation + `vilan build --print-chunks`
  reporting what WOULD split (name, function count, estimated bytes).
  Gated by unit pins over examples/router's shape (three arms, the
  shared pieces eager) and by the report on the corpus (zero splittable
  matches in non-router programs — the recognizer must not overfire).
- **S2 — chunk emission** behind `split = true`: the registry, the chunk
  files, the embedded map, chunks.json. Gates: the DEFAULT single-file
  mode stays byte-identical (the whole corpus is the pin); a new split
  golden (examples/router built split, all artifacts byte-pinned); the
  B33 invariant pin (every module binding still in the eager bundle, in
  unchanged order).
- **S3 — the router seam**: the gated route signal + `pending()` +
  initial-route preload. Gates: docs fences; a browser e2e (the router
  example, split, served, navigated — the existing headless-CDP harness;
  assert the detail chunk fetches on first Items navigation and not at
  boot).
- **S4 — the consumer sweep**: SSR/todo examples serve chunks via the
  manifest; docs (routing guide + a new "shipping" note); the
  reject-collisions extension; release notes.

Take-up order S1 → S2 → S3 → S4, each suite-gated. S1 is independently
valuable (the report tells us the real win on real apps before any
emission work) and is the measure-first gate for the rest: if the router
example's chunks come out trivially small, the arc stops there and this
proposal records why.

## 7. S1 shipped — the measurements (2026-08-03)

`vilan build --print-chunks` landed (`crates/vilan-core/src/chunks.rs`
plus the CLI flag; analysis-only, printed only on a clean analysis, the
emitted JavaScript untouched). Gates: unit pins over the router shape
(three arms, near-miss swap not splittable, node program plans nothing),
e2e pins over the router and walkthrough examples plus flag-off silence,
each planted red and restored.

The example sweep's plans:

- **router** — 1 match, 3 chunks: `Route::Home` 1 function ~89B,
  `Route::Items(..)` 3 functions ~716B, `Route::NotFound` 1 function
  ~86B; 6 entry functions eager, 0 shared.
- **walkthrough (client)** — 1 match, 3 chunks: `Route::Home` 5
  functions ~1801B, `Route::Note(..)` 6 functions ~2743B,
  `Route::NotFound` 1 function ~147B; 17 functions eager, 2 of them
  shared page helpers correctly kept eager.
- **every other example** — no splittable route matches (the recognizer
  does not overfire; the todo/reactive-ui/browser/ssr swaps are not
  route matches).

Two design corrections the sweep forced, now part of S1's semantics:
pages resident in a sibling module (`views.vl` — the common real shape)
chunk exactly like entry pages, with only STD eager by residence (the
first cut was entry-only and planned "1 match, 0 chunks" on the
walkthrough); and arm attribution's span nesting is source-aware — span
offsets are file-local, so containment is only meaningful within the
arm's own file.

**Measure-first verdict:** the mechanism works and the partitions are
exactly right, but at example scale the lazy mass is ~0.9KB (router) and
~4.7KB (walkthrough) of source — modest against the fixed std runtime in
the eager bundle. The win scales with app code, not with the runtime, so
S2 (emission) proceeds when a real consumer shows meaningful per-route
mass; the report is the instrument that will show it.
