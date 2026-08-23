# Vilan Backlog — open items (re-baselined 2026-08-18)

**The single planning surface.** Everything open lives here; nothing else
is tracked anywhere else. The chain: `backlog.md` (the alpha capture,
frozen 2026-07-18) → `backlog-2026-07-18.md` (the cycle 15–19 era, frozen
2026-08-18) → this file. `roadmap.md` is superseded the same day — its
ranked-strategy role is the **Now / Next / Later** block below; its Done
chronicle stays where it is as history.

The rules, tightened where the last tracker drifted:

- **Open items only.** When an item ships, its tombstone paragraph moves
  to [`backlog-archive.md`](backlog-archive.md) in the same sweep that
  closes it, and the number is retired. A `STATUS: OPEN` line whose body
  says "COMPLETE" (how E55/E56/I4 read by the end) is the exact failure
  this rule exists to prevent.
- **Item numbers are stable identifiers**, per-section, never reused.
  Numbering continues from the frozen tracker (highest retired: A24,
  B124→125 below, C10, D14, E61, F13, G4, H9, I5, J6).
- **Carried items keep their live remainder only**; full shipped context
  lives in the frozen file, cited as `History:`.
- `STATUS: OPEN` / `OPEN (blocked: <what>)` / `OPEN (proposal-first)` /
  `OPEN (deferred: <demand gate>)` — same legend as before, plus the
  explicit deferred form for demand-gated items.

**Owner questions parked in papers** (the recall surface — each waits on
a ruling, none blocks unrelated work): optimistic-lifecycle.md §9 (the
paint-less action-state cell; caller-less free `optimistic`),
draft-reconnect.md §4 (default debounce for `bind_draft`), bindgen.md §8
(Q1/Q2/Q3/Q6). RULED 2026-08-18, all as recommended: beta.md §4,
design-language.md §3, method-resolution.md §13.6, const-eval.md §10.5
(Option A). RULED 2026-08-19, all as recommended: remote-sources.md §6
(A25 — `sub` keeps `|T|`, no `Stale`, `Waiting`/`Ready` + `or`, deferred
`Unsubscribe`), docs-port.md §4 (K6 — option B; accept the prerequisite
filing as K13; chrome mechanism (i); keep `/docs/` anchors; keep search,
index weight = N14; `header.hbs`; no `&v=` pin).

## Now / Next / Later

- **Now** — cycles 19–26 closed. Cycle 26 (2026-08-20) shipped E78
  (the unprovided-context error underlines every uncovered call on the
  path — the owner's ask, example-as-contract), E76 (one index space at
  the ariadne boundary), E77 (hatches compose onto supplied shells,
  checked), K11 (wasm pruned to 6 + the stale-pin fallback), K13 step 3
  (the site itself on rung 2, pixel-identical), N7 (branding manifest +
  byte-equal shadow recipes), and three papers awaiting rulings: B127
  (delete the blanket — census says zero serving sites), L10 (the
  namespace model), N15 (the proposals-repo migration plan). What is
  active: nothing — **v0.35.0 SHIPPED 2026-08-21** (the owner's call, a
  day before the scheduled Saturday): the first train on the scripts
  (cut refused nothing; release 773da400, 33 entries) and the first fold
  under the ruleset (8d7fe41b, all ten steps clean, the bypass notice on
  `main` as expected), the re-themed book + masthead live, the toolchain
  at 0.35.0 in both locations. The playground-todo A25 diff turned out
  already applied by the owner; the kolt patches remain theirs. Beta
  (b): v0.35.0 is the first counted train — earliest (b) 2026-08-29.
  Wave 2 (2026-08-22, the owner's ruling batch): b126 MERGED on the nod
  (3b83d7e5 + repair aaaf4d2a); website main @f3ede99 DEPLOYED (E80 pane +
  K9 completion; live once v0.36.0's wasm ships). RULED: C3a (widened to
  any external package → E84), E79 ×7 (→ E85/E86/K17), K9 ×4, B127 DELETE,
  M9 nod, L4 ×4, N15 ×6, E69 deferred (generated vocabulary direction),
  L10 ×5 (→ L12), E87 probed and filed. Wave 2 CLOSED 2026-08-23:
  b127 SHIPPED (blanket deleted; B128 + B130 closed with it), l4 SHIPPED
  (the deprecation mechanism whole), m9 SHIPPED (overlay-owned loads,
  ASan-proven, the soak reads zero), n15 PREPARED (extraction verified,
  runbook persisted — the cutover waits on the owner creating
  `vilan-lang/proposals`). Archive 75. What is active: the **v0.36.0 cut
  on 2026-08-29** (Unreleased: 10 entries, parity 10/10, dry-run green)
  and the N15 cutover on the owner's button.
  Post-close 2026-08-20: E81 (1c21fa0f) — E78's hops now underline in
  the editor as their own diagnostics; the owner's same-day report.
  **Cycle 27 (Order 9) LAUNCHED 2026-08-21** on a bare "Go": k9 (playground
  completion — design-first), e80 (the trace into the overlay + playground),
  b125 (solver ordering — design + spike, merge bar stated), b126 (an
  unannotated function's `ret`s — rule for the owner's nod at close), l11
  (cut-script orphan markers), e79 (the §10.1 head-helpers review, paper).
  Conditional lanes waiting on rulings: b127/b130 (§14.1), m9 (§7.9.4 nod),
  l4 (four), n15 (six), e69 (semantics).
- **Next** — the owner's parked rulings (B127 §14.1; L10 §6 ×5; N15 §8
  ×6; L4's four; M9's nod; E79's §10.1 review; N8's sunset; beta.md
  §5.1 at the switch; the REWORD candidates), then the build lanes they
  unlock (B127's deletion, M9's overlay loads, N15's cutover), K9
  (design-first: the completion core's seam for wasm), E69/E80, B125,
  B126, B130, D5's session. The Zed extension (E62) is DEFERRED by
  ruling.
- **Later** — the long-gated compiler tails (A7/A8, B3/B11, C1/C2, I2,
  J4 — each blocked on a named design or the native arc), D5's traction
  plan (needs its dedicated session), and the beta switch itself
  (trigger-gated: earliest 2026-08-29 for condition (b); (d) rides D5).

---

## A. Reactive core & UI (`std::reactive`, `std::ui`)

7. **SSR tail** (S3 demand-gated; factoring undesigned)
   STATUS: OPEN (blocked: kolt/walkthrough SSR factoring undesigned; S3 demand-gated on real usage)
   v1 (render + replace) SHIPPED 2026-07-23. Live remainder: **S3, the
   Wire initial-state blob**, stays unbuilt by decision (demand-gated per
   ssr.md §6c — the double-fetch stands); and the S2 amendment's real
   open scope — **kolt and the walkthrough cannot SSR under v1** (views
   read the live rpc client at build time, handlers capture it,
   browser-layer imports), an applicability factoring question recorded
   only in ssr.md's amendment. Resumability = A7b, ssr.md §7.
   History: backlog-2026-07-18.md §A item 7.

8. **UI styling — the tail** (entangled pieces only)
   STATUS: OPEN (blocked: A7/G2 — liveness-tied emission)
   The 2026-08-04 slices closed everything unentangled. Remainder:
   critical CSS and liveness-tied dead-style elimination, riding A7/G2's
   liveness-tied emission — nothing else. Adjacent open find: A22
   (same-family rule ordering). History: backlog-2026-07-18.md §A item 8.

14. **Reactive residuals** (S–M)
    STATUS: OPEN (narrowed — one mechanism + two parked owner questions)
    Live remainder: `batch` async-join drain affinity — `batch` kept its
    `sync` fence at the turn merge; joining an ambient turn from an
    awaiting body is unresolved. The optimistic lifecycle and `Draft`
    auto re-push both SHIPPED 2026-08-04; their parked owner questions
    are indexed in this file's header. History: backlog-2026-07-18.md §A
    item 14.

## B. Type system & the type solver

3. **Variadic-generics tail** (M–L)
   STATUS: OPEN (remainder: keyof + symbolic pack concatenation, flat-tuple elision, B4-linked dispatch)
   B3a (spread parameters) SHIPPED 2026-08-04; tuple-value spread's
   circle closed. Remainder: `keyof`; symbolic pack concatenation;
   eliding the flat-tuple construction copy; trait-typed-value dispatch
   (→ B4). Record: variadic-generics.md §S/§T. History:
   backlog-2026-07-18.md §B item 3.

11. **`!` / `?.` tail** (M)
    STATUS: OPEN (design-gated only — try-and-lift.md §12.1/§12.2)
    The bare-`?` trait path shipped. Remainder, both genuinely
    undesigned: §12.1 closure `!` (arg-becomes-Result, RpcOutcome×Try
    collision, which closures may host a `!`) and §12.2 Signal/Promise
    Lift opt-ins (Signal::map SUBSCRIBES — `signal?` would mint an
    unowned subscription per render, the A21 leak shape). Recorded §11:
    B29 does not cover a wrong-shaped Lift impl. History:
    backlog-2026-07-18.md §B item 11.

133. **NEW — lift rule 4's closure refusal to the reachable-tail rule** (S–M; B126's Q1, owner-approved follow-up 2026-08-22)
    STATUS: OPEN
    `{ ret 1; }` now infers `i32` in a function but is still refused in a
    closure with rule 4's "make the ret'd value the body's tail" guidance —
    a deliberate asymmetry kept at B126's merge. Lift rule 4 to the same
    reachable-tail unification (ret-checking.md rule 3 as amended), keeping
    S3's steer where a genuine disagreement remains; pins per shape mirror
    the b126_ set. Same territory as B125's closure arm — sequence, don't
    parallel. Record: ret-checking.md "Rule 3, amended", Q1.

134. **NEW — the borrow/crossing seams do not see an unannotated function's `ret`s** (S; B126's Q2, owner-approved follow-up 2026-08-22)
    STATUS: OPEN
    `return_sites` joins only declared-return functions' `ret`s; an
    unannotated `ret &self.x` is caught by the generic FunctionReturn escape
    scan but `infer_borrows`/crossings/clone-site seams don't see it. Now that
    those `ret`s are typed (B126), extend the join; pin the view/resource
    shapes (B116/B122 families). Record: ret-checking.md "Rule 3, amended", Q2.

131. **NEW — a never-called closure's untyped parameter reports at the use, through the leftover sweep** (S; found by B125's lane 2026-08-22)
    STATUS: OPEN
    Since B125 an unannotated closure that is never called, iterating or
    indexing its parameter, reports "type of function call arguments could not
    be resolved" at the USE — the generic leftover sweep — where it used to
    compile with the item typed `any`. Right refusal, wrong anchor and head:
    B13 has no first call to point at, so the message should name the
    parameter ("`list` is never given a type: this closure is never called and
    its parameter is unannotated — annotate it") at the parameter. Ledger row,
    pin, plant. Record: type-solver.md "P21 closed" §, Q1.

132. **NEW — a bare-expression closure body reports its type disagreement at the argument, not the expression** (S; found by B125's lane 2026-08-22)
    STATUS: OPEN
    `|point| point.x * 2` under `let widths: List<str>` reports as a whole
    value at the argument check (narrower than the old whole-call anchor, but
    not at the expression), while a block body reports at the closing brace
    with S3's one-character steer. Refine S3's route to bare bodies so the
    report lands on the expression with the same steer; pin beside the eight
    `b125_*` B5 pins. Record: type-solver.md "P21 closed" §, Q2.

## C. Memory model

1. **`Weak<T>`** (M)
   STATUS: OPEN (blocked: Tier 2 refcounting, the native arc)
   Fully specified in destruction.md §10 (incl. the scoped
   `get(&self): Option<&T> borrows self` twin from claims-and-epochs.md
   §5a). Deterministic `upgrade() → None` needs a release event, which
   only exists once handles are refcounted; GC-timing `WeakRef` rejected
   2026-07-07. History: backlog-2026-07-18.md §C item 1.

2. **Dynamic rule-4 remainder** (M)
   STATUS: OPEN (blocked: F4's native memory story)
   Cross-handle aliased writes (two `Shared` handles, one cell) need
   runtime generations / poisoned views; semantically empty on JS. Build
   with the native memory story, likely debug-mode-only. History:
   backlog-2026-07-18.md §C item 2.

## D. Documentation

5. **Public traction plan** (M; a PLAN first, not execution)
   STATUS: OPEN (blocked: needs a dedicated session with the owner)
   Blogs, website, and other resources for public traction. Candidate
   skeleton in the frozen entry (landing page, "why vilan" essay, deep
   dives, demos, distribution as on-ramp). Public-exposure choices
   interact with the pseudonym discipline; voice/positioning are the
   owner's calls. Overlaps §K's web arc — coordinate, don't duplicate.
   History: backlog-2026-07-18.md §D item 5.

## E. LSP & tooling

37. **bindgen v2 — the remainder** (M–L)
    STATUS: OPEN (remaining: (c) the oxc swap-in seam and (d) the override-table direction, both unscheduled; the 183-globals "read a global" language question; §11.6's shallow `--only` mode; §8 Q1/Q2/Q3/Q6 remain the owner's)
    (a)(b)(e) SHIPPED 2026-08-06 (92.3% of lib.dom declarations). Record:
    bindgen.md §11. History: backlog-2026-07-18.md §E item 37.

62. **NEW — Zed language extension** (M–L; owner's 2026-08-18 list, item 4)
    STATUS: DEFERRED (owner ruling 2026-08-18: a tree-sitter grammar is one more thing to maintain after every syntax change — revisit when the syntax settles, i.e. at or after the beta switch)
    Zed extensions are Rust→WASM: a tree-sitter grammar plus glue
    launching `vilan-lsp` (which already ships per-release). The grammar
    is the bulk of the work and pays twice — GitHub's syntax
    highlighting consumes tree-sitter grammars too. Survey question for
    the order: what the existing `editors/` assets (TextMate grammar?)
    can seed.

69. **NEW — attribute-NAME completion in an element head is a semantics decision** (S–M; deferred by the E67 lane 2026-08-18)
    STATUS: DEFERRED (owner ruling 2026-08-22: if offered at all, the vocabulary should be GENERATED — not handcrafted or hand-maintained; decide later. The generation source is the open question when revisited — §9.3's objection was a second source of truth, which a generated, gated table answers)
    `<div .|>` now completes the View's methods and `<div |>` the dotted
    links + `on:` — but attribute NAMES (`name(..)`, `type(..)`, …) are
    not offered, because the desugar has no table of them to consume
    ("no special-cased names in the lowering table, ever"), and a list in
    the LSP would be a second source of truth with nothing to gate it;
    deriving one from DOM bindings was rejected (IDL property names ≠
    attribute names). Offering them means amending §9.3 (a curated
    vocabulary the desugar validates AND completion reads) — the owner's
    call. Also not attempted: tag-name completion and the child position.
    Record: editing-dx.md §18.

79. **NEW — the declined `Document` head helpers have their first real customers: the §10.1 review is due** (S; filed by K13 step 3, 2026-08-20)
    STATUS: paper PROPOSED 2026-08-21 — §16.13 awaits the owner's rulings
    fullstack-dx.md §15.2 declined three head helpers (description,
    favicon/icons, generic meta) and §10.1 said "review when the first
    three requests are all in `head()`". The first real site's climb put
    ALL of them in `head()` (§16.11's census: description, the icon set,
    the og:/twitter: card, the UA theme metas — the last restating
    palette values theme.vl knows, a hand-sync smell). The review:
    which, if any, graduate to builder methods — and whether the theme
    metas deserve a token-aware helper rather than hand-restated hexes.
    Adjacent, recorded not filed: §5.4's declined route prefix had its
    customer too, resolved by the deployment following the ladder's URL
    space instead. Record: fullstack-dx.md §16.11.

82. **NEW — a derive refusal on a module's struct anchors at a comment line in the ENTRY** (S–M; found by E80's lane 2026-08-22)
    STATUS: OPEN
    `[derive(PartialEq)]` on a struct in `playground_page.vl:59` whose field is a
    `List<…>` refused with "type 'List' does not implement the PartialEq operator…"
    anchored at `./src/playground.vl:6:14` — a comment line in the entry, not the
    struct. A span from a module rendered against the entry's text: the E16
    anchoring rule (the file comes from the anchor, never "the entry") is not
    honored on the derive path. Find the derive-expansion diagnostic site, route it
    through `anchored`, pin the cross-file shape. Record: E80's report, Q3.

83. **NEW — the scope-position completion re-parses the buffer once per auto-import candidate** (S–M; found by K9's lane 2026-08-22)
    STATUS: OPEN
    Measured on the folded walkthrough app (407 lines): member completion
    2.6 ms, `import std::` 2.6 ms, but a bare scope position 51 ms for 131
    items — the engine's own cost, paid by the LSP and the playground alike:
    the E54c auto-import path calls `insert_import` (a full re-parse of the
    buffer) per candidate, and `doc_comment_of` clones module text per
    candidate. Parse once per request and share it across candidates; pin the
    count of parses per request (a plant that re-parses per candidate goes
    red) and re-measure. Lives in `crates/vilan-ide` now. Record:
    playground-completion.md §9.

84. **NEW — the trace/demotion contract widens to any external/linked package** (S–M; owner ruling on C3a 2026-08-22)
    STATUS: OPEN
    diagnostics-standard.md C3a is ruled not std-specific: code the user did
    not write — std or ANY external/linked package — demotes and traces the
    same way. The implementation's `std_spanned` checks and the A2 anchoring
    walk exercise std only; verify what a git-dependency package's frames do
    today (probe a workspace with a dependency whose function reads a
    context), then widen the demotion/labeling to non-workspace packages,
    pins per surface. Record: diagnostics-standard.md C3a.

85. **NEW — `description(text)` graduates to a `Document` builder method** (S; E79 Q2 RULED yes 2026-08-22)
    STATUS: OPEN — RULED, ready to build
    `title`'s twin: escaped, in the generated prefix, one pin; the docs page
    beside it. The bound's new wording (the intersection plus the identity
    lines the document is the sole author of) lands in §15.2 with it.
    Record: fullstack-dx.md §16.13.

86. **NEW — repeatable `head()`/`body()` calls concatenate with no separator** (S; E79 Q7 RULED file 2026-08-22)
    STATUS: OPEN — RULED, ready to build
    `document.vl:313` joins hatch markup with no separator, which is why the
    site wrote its own `joined()`. Separate consecutive calls with a newline
    at the hatch's indent so the hatch is usable per item; pin the composed
    output; the site drops its helper when it lands. Record: fullstack-dx.md
    §16.13.

87. **NEW — data-* and aria-* attributes: `data-foo-bar(v)` already works — bless, pin, document** (S; owner question 2026-08-22, probed same day)
    STATUS: OPEN — the probe settled the design
    `<div data-foo-bar("x") aria-label("y")>` parses, checks and emits the
    attributes VERBATIM on v0.35.0 (built and read in the emitted JS): the
    name-blind desugar never cared that the name has hyphens. So the spelling
    IS the undotted literal form, matching HTML exactly — `data:` would mint
    a second marker family beside `on:`, and a `.data("foo-bar", v)` method
    duplicates what the attribute form does. Bless it: a pin (parse + emit,
    hyphenated), a docs line in the element-syntax page, and §2's prose
    noting hyphens are ordinary attribute-name characters. Record:
    element-syntax.md §2 when built.

## G. Macros & const

2. **Const-eval tail** (S–M)
   STATUS: OPEN (remainder is deferred-with-question, const-eval.md §8)
   Remainder, each deferred-with-question in §8: expression-level const
   spans (needs per-node provenance or a spanned IR), cross-analysis
   memoization (cache-key question; measured 7–9% of warm analysis — of
   direct interest to §M's perf arc), a const budget knob. Liveness-tied
   emission stays A7-entangled. History: backlog-2026-07-18.md §G item 2.

## I. Collections

2. **Fixed-arrays tail** (M; fixed-arrays.md §7)
   STATUS: OPEN (blocked: const-generics design — what `const N` means)
   Const-named / const-generic lengths (`[u8; SIZE]`, `<const N>`):
   proposal first (the constraint form, the staging fork — const-eval is
   post-analysis, lengths are needed mid-fixpoint). Then `List` ↔
   `[T; n]` conversions, slicing (wants a range type), generic
   `[T; N].len() → N`. History: backlog-2026-07-18.md §I item 2.

3. **Iterator adapters — the remainder** (S–M)
   STATUS: OPEN (remaining: S6/Iterable under B4)
   The arc SHIPPED 2026-08-06; §4 option (ii) REFUSED by owner ruling.
   Live remainder: S6/`Iterable` waits on B4 (trait-typed-value
   dispatch). Record: iterator-adapters.md §11. History:
   backlog-2026-07-18.md §I item 3.

4. **NEW — `List<T: PartialEq>` has no `PartialEq` impl, so a struct holding a list cannot derive it** (S; found by E80's lane 2026-08-22)
   STATUS: OPEN
   `std::List` carries only an inherent `impl List<T: PartialEq>` (`contains`/
   `index_of`); `[derive(PartialEq)]` on a struct with a `List<…>` field is
   refused, and the website's `DiagRow` wrote its `eq` by hand. Add
   `impl List<T: PartialEq> with PartialEq` (element-wise, length first), pins
   (empty/equal/unequal/nested lists, the derive on a struct field), the docs
   page for List; then return `DiagRow` to the derive (website follow-up).
   Record: E80's report, Q2.

## J. Concurrency

4. **Free-spawn lint** (S once unblocked)
   STATUS: OPEN (blocked: Tier 2 counted closure environments)
   The rule ("a spawn happens inside a `nursery` extent or an
   `OwnedNursery.enter` — anything else is a lint") cannot ship while
   std's three legitimate free spawns remain (Draft.commit, the RPC SSE
   pump, streaming `on_open`); they become ownable with §10's counted
   captures. The lint ships the same day they migrate, zero baked-in
   exceptions. History: backlog-2026-07-18.md §J item 4.

5. **Async recorded opens — the deferred pair** (S each)
   STATUS: OPEN (deferred: demand-gated)
   Live remainder: per-task cancel handles (resolved-for-delays by
   `std::time::Timer` 2026-07-28; handles stay deferred — no field case
   has asked for cancelling a computation, only a delay); the free-spawn
   lint rides J4. Everything else in the entry shipped. History:
   backlog-2026-07-18.md §J item 5.

## K. Web presence (site, playground, docs delivery) — NEW SECTION

The website, playground, and docs repos had no tracker home; that gap is
part of why planning fragmented. Spans `vilan-website` and
`vilan-lang.github.io`; compiler-repo work stays in §A–§J.

5. **The design language — adopt** (M–L; owner's items 3 + 11)
   STATUS: OPEN (RATIFIED 2026-08-18; SLICE 1 SHIPPED 2026-08-18 — web-tokens → website@561dcff; SLICE 2 SHIPPED 2026-08-18 — web-slice2 8c98bbc → website@6e549d2, deployed, with K9 (dropped on evidence) and K10 (editor theme reads tokens via CSS vars); owner on merge: "could use some refinement, but definitely moving in the right direction" — a refinement pass rides slice 3 or its own item once the owner names the specifics; slice 3 = the docs, with K6 — its design now lands in docs-port.md
   §3.1 S2–S3 (mdBook is token-driven: 42 CSS custom properties per theme
   in one `variables.css` override, and it already ships a light/dark
   picker); K6 RULED 2026-08-19 — SLICE 3 SHIPPED 2026-08-19, both halves: the book (k6-book → next 0eaa38c0: `variables.css` role tokens on `html.light`/`html.navy`, design-language.md §2.5 the light palette; live at the v0.35.0 fold) and the site (web-chrome + web-art-light → website@cb3752a, deployed: every token carries both values behind `prefers-color-scheme`, the art re-lit onto the roles, `shadow`/`art-error` tokens; owner: "Approved"). What stays open under K5: the refinement pass the owner named at slice 2 (specifics pending), and K10's one-token-source generation now that three mirrors exist)
   design-language.md is ratified: kolt's `visual-overhaul-2` role
   tokens (`up`/`down`/`stroke`/`primary`, verbatim) carrying the brand
   palette, tool surfaces fully utilitarian, the hero fenced as the one
   indulgence, CommitMono V143 with the owner's feature settings (§2.3),
   light theme with K6, editor stays CodeMirror 6. Slices: **(1)** the
   token system in `theme.vl` + site chrome (masthead, page, footer)
   restyled onto it + CommitMono self-hosted for code blocks + K1's nav
   link; **(2)** the playground page + editor onto the tool register,
   with K10 (generated editor theme) and K9; **(3)** the docs, riding
   K6. Every slice: before/after screenshots for the owner's review
   BEFORE merge — the website deploys on every push to `main`.

8. **Website features & small visual upgrades** (S–M each; owner's item 11)
   STATUS: OPEN (umbrella — refine into concrete items under K5's ratified language)

13. **NEW — the docs on the vilan framework, the port proper — behind its markdown prerequisite** (L; filed by the K6 ruling 2026-08-19)
    STATUS: OPEN (blocked: docs-port.md §3.3 — the markdown story, then the const input channel; STEP 3 DONE 2026-08-20 — the site took rung 2 whole, website@6036e21, record fullstack-dx.md §16.11: pixel-identical both pages both schemes, the shells deleted, the hatch census is the ladder's fit report, §15.2's declined helpers all found customers → E79)
    The owner's literal item 10 ("transitioning the docs to the vilan
    framework"), filed as its own item so it stays reachable while K6
    ships option B. docs-port.md §2.1 proved the port is *unavailable*
    today, not merely expensive: a `const` cannot read a file nor return a
    `View`, and the 1M fuel budget is exhausted by a char-scan of a page
    the size of the book's largest. §3.3 gives the honest order: (1) a
    markdown story — a `std::markdown` (or package) parser producing a
    plain-data AST, or a `[build] run` pre-step emitting generated `.vl`
    from `.md` (no compiler change; the cheaper proof); (2) a const input
    channel, only if the parser is to run at compile time, with the fuel
    question answered first; (3) a router and rung-2 adoption on the site
    (`Document::of` + `serve_build` + `split = true`), which fullstack-dx.md
    §16.2 notes the compiler repo cannot yet demonstrate either (E65).
    Each step is independently valuable — the test of a real prerequisite.
    **L10's paper (std-shape.md, 2026-08-20) names `std::markdown` as the
    first candidate official package — its §6 Q4 asks whether the markdown
    story should be built package-shaped from day one.**
    The 32 LSP deep links and 417 in-book links pin mdBook's anchor
    algorithm as a compatibility surface (§4 Q3) that any renderer must
    reproduce. Record: docs-port.md §2.1, §3.3, §4 Q1.

17. **NEW — site-side `theme_metas(ground)` reads `themed_values`** (S; E79 Q5 RULED yes 2026-08-22)
    STATUS: OPEN — RULED, ready to build
    The two `theme-color` metas and the three `var()` fallbacks in
    `server.vl`'s `head_hatch` restate `theme.vl:120`'s pair by hand; a
    `theme_metas` in `theme.vl` derives them from `themed_values` (one
    palette home, K10's principle). No std helper until a second site asks.
    Record: fullstack-dx.md §16.13.

## L. Release engineering & beta — NEW SECTION

The alpha→beta transition. The *contract* is RATIFIED (process.md §5,
2026-08-07: three promises, no spec freeze, the four-condition trigger,
the v0.40.0 jump); beta.md (RATIFIED 2026-08-18) is the execution
charter — its status block also records the owner's "should we defer
beta?" and the answer: **the trigger already defers the declaration**
(none of the four conditions hold today; (b) earliest 2026-08-29; (c)
waits on B73; (d) on D5), so the pre-switch items below proceed at
ordinary priority as low-regret hygiene, and nothing beta-branded ships
publicly until the trigger fires. The alpha framing in README/CHANGELOG
is **correct until the switch commit** — do not "fix" it as rot.
(L1 — ratify beta.md — CLOSED 2026-08-18; the archive's first entry.)

3. **std tier sweep** (M)
   STATUS: OPEN (table DRAFTED 2026-08-20, beta.md §5; ruling DEFERRED 2026-08-20 by the owner — "the answers to those questions might change from now until we officially enter beta" — re-present §5.1 with the beta switch's pre-work, not before; the docs page lands at ratification)
   The census: 56 public modules (54 std + 2 macro_std; canvas has no
   module yet — §4 Q4's Tier 3 binds when it lands). Proposed: 32 Tier 1,
   23 Tier 2 (with promote-on-quiet clocks and holds where an open item
   gates), 1 Tier 3 (`into`, B127's deletion question). §5.1's arguable
   rows: iterator straight to Tier 1; operators under B11's open item vs
   an item-level carve; into at Tier 3; process/dom at Tier 1 against
   their Tier-2 directories; wire's tier vs §5.2(3)'s unconditional
   Breaking pricing.
   Enumerate the public std surface, propose the Tier 1/2/3 table
   (beta.md §3.2), owner rules, docs publish it.

8. **Contribution scaffolding** (M)
   STATUS: OPEN (blocked: D5 — deferred with it, process.md §9.2)
   CONTRIBUTING.md, SECURITY.md, CODEOWNERS, issue/PR templates,
   private vulnerability reporting. Revisit when D5's session happens;
   scaffolding for an audience arrives with the audience.

10. **NEW — std vs official packages: the distribution shape** (L; owner's 2026-08-20 question — proposal-first)
    STATUS: OPEN (paper PROPOSED 2026-08-20 — std-shape.md awaits the owner's five §6 rulings; the recommendation: the NAMESPACE model as recorded direction, zero construction now — today's spellings already ARE the namespace model's, verified in the loader)
    The owner: restructure std into 'std' and 'official packages' — "or
    maybe std should be more of a namespace under which all of the
    official packages are published?" Orchestrator's recommendation,
    for the paper to argue: the NAMESPACE model, sequenced — (1) the
    tier table (beta.md §5) already draws the seam: Tier 1 core is
    inseparable std, the Tier 2 framework layer (reactive/ui/rpc/
    process/router/style…) is the candidate publishing surface; (2)
    nothing splits until a package registry exists (there is none —
    D5's territory), because a split without distribution is import
    churn for no capability; (3) when publishing is real, the framework
    modules become separately-versioned packages published UNDER the
    `std::` namespace with each toolchain release bundling a pinned,
    offline-working set — `import std::reactive::Signal` never churns,
    the binary stays batteries-included, and a package can still rev
    between trains for those who opt in. A hard `std`-vs-`official-
    packages` rename churns every import and splits the book for no
    user gain today. The paper must decide: what the compiler treats as
    "the std package" vs resolved packages, per-release pinning, whether
    Tier 1 is structurally inseparable, and how the embedded binary and
    a registry coexist. Interacts with: beta.md §3.2/§5 (tiers), L4
    (deprecation), D5 (registry/traction).

12. **NEW — reserve `std`/`pkg`/`macro_std` as package names** (S; L10 Q5 RULED 2026-08-22)
    STATUS: OPEN — RULED, ready to build
    The namespace model's one code item now: the loader/manifest refuses a
    dependency claiming the reserved roots, so `import std::…` can never be
    shadowed by a package. Pin the refusal head (ledger row), docs line in
    the packages page. Record: std-shape.md §6.

## M. Performance & footprint — NEW SECTION

Owner's items 7 (perf) and 8 (leaks). The 2026-08-18 survey found the
seams already cut: the four pipeline phases are independently callable
library entry points (`parsing::parse` → `analyzer::analyze` →
`post_analysis_passes` → `transformer::transform`, the same seam
`VILAN_PHASE_TIMING` marks), a purpose-built per-site leak harness
exists (`leak_tally` + vilan-lsp's `leak_measurement` module), and the
suite's liveness bounds already use measured-reference thresholds
(`support/mod.rs`'s `reference_compile()`), never fixed seconds.
Corpora measured: todo 119 lines (smoke only), kolt 943, website 2,996,
std 15,024 (the cold-compile stand-in).

## N. Hygiene & rot — NEW SECTION

Owner's items 1–2 (repo refactoring, rot, consolidation, README;
rotted/poorly-written code). Seeded by the 2026-08-18 rot survey (all
four repos, read-only). What the survey found and this order already
fixed in the same sweep: the proposal index's stale/missing rows,
AGENTS.md's dead arc pointer, roadmap.md routing readers to the dead
backlog (banners landed with the re-baseline). What it cleared as
NOT rot: `about.hbs`/`about.toml` (live, `cargo about` consumers),
npm's `0.0.0-placeholder` and the homebrew formula's 0.14.0 seed pin
(both deliberate and test-documented), the bindgen module's 63
`TODO(bindgen)` hits (an emission vocabulary, not debt), and the
README/CHANGELOG alpha framing (correct until the beta switch — §L).

2. **`proposal/archive/` consolidation** (M; owner ruling on the shape)
   STATUS: OPEN (proposal-first — moving cited records needs the owner's nod)
   The flat proposal directory carries three generations of the
   memory-management design (memory-management.md → -rev-1.md →
   claims-and-epochs.md/destruction.md), the 279KB frozen backlog.md,
   the now-frozen backlog-2026-07-18.md, and superseded roadmap.md
   undifferentiated among live papers. Move the dead generations and
   frozen trackers under `proposal/archive/`, leaving the banners as
   forwarding pointers. Cost to weigh: countless prose citations say
   "record: backlog.md" — the move must keep those findable (the
   banner files could stay as one-line pointers).

8. **Pages repo housekeeping** (S–M umbrella)
   STATUS: OPEN (narrowed 2026-08-20 — the three orphaned brand files are deleted and the 404 shim's sunset is PROPOSED in the pages README ("at the beta switch", proposed-not-ruled — the one open half); N12's README shipped 2026-08-19; K12/N13 closed 2026-08-20)
   The local checkout is 41 commits behind its origin (refresh before
   trusting any file-presence claim there — the survey's "no sitemap"
   class of findings were checkout staleness, not site defects). Then:
   no README distinguishing bot-generated files (`docs/`, `index.html`,
   `client.*`, `playground/` — pushed by two different workflows) from
   hand-owned (`assets/`); three orphaned brand files nothing
   references (`icon-512.png`, `light_lockup.png`,
   `dark_wordmark_flat.svg` — delete); the pre-v0.15 `404.html`
   deep-link shim deserves a recorded sunset condition. (The
   `book.toml` leak and mdBook fonts rode K6 — both closed 2026-08-19;
   the README is N12, shipped the same day, and names `chrome/` as the
   new bot-owned prefix; the stale-checkout note is moot — pulled
   2026-08-19; wasm retention is K11.)

15. **NEW — the proposals move to their own vilan-lang repo** (M; owner's 2026-08-20 ask — proposal-first)
    STATUS: OPEN (plan PROPOSED 2026-08-20 — proposals-repo.md awaits the owner's six §8 rulings; recommends filter-repo extraction (91% of proposal commits are pure records commits), paths 1:1 under `proposal/`, the TRACKER rides with the papers, a sibling checkout with single `main`; the only functionally affected test is hygiene.rs's four allowlist rows; supersedes-and-absorbs N2)
    Pull `vilan/proposal/` (the papers, the trackers, the archive — ~100
    files) out of the compiler repo into a dedicated org repo (e.g.
    `vilan-lang/proposals`). The plan must decide: history (a
    `git filter-repo`/subtree split preserving each paper's log vs a
    clean import with the compiler repo's history as the archive);
    citation churn (hundreds of "record: X.md §n" prose pointers — they
    stay greppable if paths are preserved 1:1 under the new root; the
    in-repo pointers that MUST keep working: AGENTS.md's spec pointers,
    the work-order briefs' `vilan/proposal/...` paths, README index,
    docs-site.md's "design history lives in vilan/proposal/" line in the
    book's Welcome page); the trackers' home (the single planning
    surface moves with the papers — the orchestration workflow reads/
    writes it every cycle, so the integration-worktree convention needs
    a sibling checkout or the tracker stays behind — argue it); CI (the
    hygiene gate scans tracked files — what of it applies to a prose
    repo); and N2's original question (the dead generations under
    `archive/`) lands as directory layout in the new repo instead of a
    move within this one. Cost to state honestly: every future lane
    brief and record sweep crosses a repo boundary.
