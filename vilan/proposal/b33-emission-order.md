# B33 — module initialization order: dependency-ordered, specified, cycle-checked

> **SHIPPED 2026-07-25 — the whole arc, three slices in one day.**
> S1 f9dec2f (the relation + the order; adversarially reviewed — TWO
> blockers fixed pre-commit: argument-passed closures entered, SCC
> condensation), S2 3f82aa2 (the cycle diagnostic, ledger row 209),
> S3 (spec §7.1/§7.6, the widened import-order pin, math.vl reformatted,
> the shared-CallGraph perf rider, changelog staged). **Two premise
> corrections recorded**: (1) §1/§6/§7 said math.vl's golden would be
> "regenerated and verified" — the reformat is GOLDEN-NEUTRAL (S1's
> canonical order made brace order irrelevant before fmt touched it);
> (2) the canonical fallback's dependency clause is dependency-graph
> post-order, NOT "manifest order" (Manifest.dependencies is a BTreeMap;
> resolve_dependency_edges is a post-order DFS — probed both ways); the
> spec states the corrected rule. Residuals: cross-PACKAGE order is
> probed but carries no permanent pin (needs a vilan-cli workspace
> fixture — small follow-up if ever demanded); E16/E17 (filed) cover the
> CLI cross-module render + LSP note-drop this arc surfaced.
>
> **Status: RATIFIED 2026-07-25 — all §5 calls per recommendation** ((a)
> the load-time relation as specified; (b) strict cycles with chains,
> revisit on first false positive; (c) same-module order-freedom; (d)
> initialization order specified — the changelog carries the breaking
> note; (e) all riders). Slices per §7; S1 gets adversarial review.
>
> Original status: DRAFT 2026-07-25 — for review. Backlog B33, taken up per the
> user's call (before the F7 distribution implementation). Grounded in a
> full investigation at HEAD (2026-07-25); every probe below was run
> against a fresh debug binary. The investigation **widened the backlog
> entry** in three ways it recorded as contradictions: the churn is
> import-*statement*-order wide, not just brace-order; the TDZ hazard is a
> two-line same-file repro (no "adversarial cross-module shape" needed) and
> also fires through function calls and closures, with **no diagnostic
> anywhere**; and initialization order is already *observable* (effectful
> initializers print in import order today) and flips under a pure import
> reorder — while the spec says nothing and three doc comments say
> "declaration order", which is inaccurate.

## 0. What exists (investigated, file:line)

- **The order today**: `Program::module_level_bindings()`
  (analyzer.rs:21766) = the entry global scope's insertion-ordered
  `IndexMap` first (import statement order × brace order —
  `resolve_import` inserts at analyzer.rs:16455), then loaded modules' own
  top-level `let`s in canonical load order; dedup is first-occurrence-wins
  (:21793) — so the *entry's import listing* overrides a binding's own
  declaration position. All emission consumers read this one vector
  (transform_entry_ast :844, transform_functions :58).
- **WO-1b's canonical module walk** (analyzer.rs:23299 `load_order_key`)
  made entity ids and function emission import-order-independent; it never
  touched the entry-scope half of this vector — B33 is exactly the
  residual, pinned out of scope in
  `emitted_js_is_independent_of_import_order`'s docstring (corpus.rs:294)
  and the reason `vilan/test/math.vl` sits unreformatted.
- **TDZ today, probed**: `let A: i32 = B * 2;` before `let B: i32 = 21;`
  in ONE file builds clean and crashes (`Cannot access 'B' before
  initialization`); likewise self-reference (`let A = A + 1` emits
  `const A = A + 1;`), a forward read through a called function, and a
  call through a global-held closure. Every module binding emits `const`
  (immutable by construction — `let mut` at module level is a parse
  error), so every one is TDZ-exposed. No diagnostic exists.
- **The naive-sort miscompile, reconstructed precisely**: `zeta.vl` has
  `let Z = 21`, `alpha.vl` has `let A = Z * 2`, entry imports zeta before
  alpha. Today: `const Z = 21; const A = Z * 2;` — works. Canonical load
  order loads `alpha` first, so `A.id < Z.id`: **id-sort and name-sort
  both emit `A` first → TDZ.** (This is commit 6289dea's "provably
  TDZ-miscompiles", now stated as a concrete program.)
- **Reusable machinery**: `call_graph.rs` already treats each initializer
  as a collection unit (initializer_calls_of :284, initializer_closures_of
  :298, global_references_of :267 keyed by initializers too) with an
  exhaustive-arm successors (:316); `platform_color::reachable_bindings`
  DFSes those edges; `const_eval.rs:99` has the in-progress-set cycle
  diagnostic template; platform_color's reverse-graph BFS (:428) renders
  `via A → B` witness chains. The J.3 async-initializer pass bans only
  async calls at init — sync effectful calls are unrestricted.

## 1. The rule (spec-bound)

**Module-level bindings initialize in dependency order.** A binding's
initializer runs after every binding it *evaluates at load time* (§2's
relation). Among bindings the relation leaves unordered, initialization
follows the **canonical order** — WO-1b's load-order key (std → deps →
pkg → entry; module name; declaration order within a module) — so emitted
JS is byte-stable under any spelling of the imports, and textual order
within a module carries no meaning. **A dependency cycle among
initializers is a compile error** (§3). `const`-marked bindings fold to
literals before any of this and are exempt by construction.

Three deliberate consequences:

- **Same-module bindings become order-free.** `let A = B * 2;` above
  `let B = 21;` stops being a runtime crash and simply works — consistent
  with items (functions) already being order-free. No textual
  declaration-before-use rule is introduced; dependencies decide, the
  cycle check catches the rest. Self-reference (`let A = A + 1`) is a
  1-cycle and errors.
- **Initialization order becomes specified.** Today it is observable,
  unspecified, and flips under import reordering. The rule above becomes
  spec text (§7.1 gains the sentence; §7.6's emission guarantees list
  gains the entry; the three "in declaration order" doc comments —
  analyzer.rs:21758, transformer.rs:841, :857 — are corrected in the same
  change-set). **This changes observable behavior for effectful
  initializers** (their relative order may differ from today's
  import-listing order); the changelog carries it as the breaking note.
- **The import-order sensitivity dies entirely** — statement order and
  brace order both. `emitted_js_is_independent_of_import_order` widens to
  cover constant-importing statements (its current fixture imports only
  functions and is blind to the bug it documents), and `math.vl` finally
  gets `vilan fmt`'d, its golden regenerated and verified.

## 2. The load-time relation (the design core)

The ordering edge is **"B's initializer evaluates X at load time"** — NOT
the call graph's reachability. The two differ on exactly one edge class,
and the difference is load-bearing:

- **Closure *creation* is inert.** A closure a binding creates is not
  evaluated at load, so its body contributes no ordering edges to its
  creator. This is what keeps the mutually-recursive module-closure idiom
  legal — probed working today, deliberately supported by the B31 arc:
  `let EVEN = |n| { … ODD(n-1) };` / `let ODD = |n| { … EVEN(n-1) };`
  has no load-time evaluation at all (two creations, no calls) and stays
  accepted in any order. Building the graph on raw `successors` would
  reject it — the creator rule charges each body to its binding and
  manufactures a cycle. This is the trap the investigation flagged; the
  relation must be hand-assembled from the call graph's parts, not
  inherited whole.
- **Calls made during initialization are followed, transitively** —
  direct calls enter the callee's body; a call *through a value* (probed:
  `let X = FETCH();` TDZ-crashes on a binding `FETCH`'s body reads)
  enters the bodies of the closures that value can hold. Since module
  bindings are immutable, a global's possible closures are statically
  known: the closures its own initializer created, or the
  created-closures of functions in its value's def chain — exactly what
  `initializer_closures_of` + the creator bookkeeping already record. The
  body's reads charge to the *calling* binding (X needs Y), not to the
  closure's creator (FETCH stays unordered w.r.t. Y) — which is precisely
  the semantics of evaluation.
- **Reads are edges** (`global_references_of` at the initializer, plus
  inside anything actually entered per the rule above). Dispatch sites
  follow the existing over-approximation (`dispatch_candidates` — every
  trait candidate); §5(b) records the false-cycle risk this carries.

## 3. The cycle diagnostic

`const_eval`'s in-progress-set DFS is the template; the pass lives
post-`analyze()` beside it and J.3 (wired into BOTH `lib.rs` and the
CLI's duplicated sequence, per the standing rule, verified by a CLI
probe). The message, in the house shape: spanned at the **first read
that closes the cycle** (global_references_of gives the read expr), with
a `via A → B → A` chain (platform_color's witness-chain machinery) and a
note naming each participating binding's declaration. Same-module,
cross-module, and self-reference cycles all land here; so does a cycle
closed through a load-time call.

## 4. Mechanics

`module_level_bindings()` learns the order (or a sibling
`initialization_order()` supersedes it for emission consumers): compute
the load-time graph, topo-sort with the canonical key as tie-break,
emit. Bindings emit as node *groups* (one binding may produce several JS
nodes — groups reorder wholesale, contents untouched). Reachability
filtering, tree-shaking, and the functions-first assembly are unchanged
(functions are JS-hoisted; only the `const` groups reorder). The macro
world (`transform_functions`) applies the same order. Riders while in the
area (investigation finds): half 2 of `module_level_bindings` is
O(modules × variables) and rebuilt per consumer — cache or reshape it;
a const site's prelude iteration is over a `HashSet`
(:5074) — make it deterministic while touching the file. (Both done: the
seed caches the bindings and the prelude sorts each batch by entity id;
the const site's build lives in `ConstWorld::prepare`, const-eval.md
§10.4/§10.6.)

## 5. Open calls

(a) **The load-time relation as specified in §2** (creation inert,
    init-time calls followed into possible bodies, reads charge to the
    evaluator) — recommend: yes. The alternative — raw successors — is
    simpler and *rejects EVEN/ODD*, working code today.
(b) **Cycle strictness under dispatch over-approximation**: an
    initializer calling a bound-generic method pulls every trait
    candidate's reads into its edges, which can manufacture a false
    cycle. Recommend: ship strict (error with the full chain, which makes
    a false positive self-explaining), record the over-approximation in
    the diagnostic's note, revisit on the first real-world false
    positive. Alternative: only direct-read cycles error — but that
    leaves call-mediated true cycles as runtime TDZ, which is the bug
    class we came to kill.
(c) **Same-module order-freedom** (§1, first consequence) — recommend:
    yes; alternative is a textual declaration-before-use error, which is
    a new rule the dependency order makes unnecessary.
(d) **Specifying observable initialization order** (§1, second
    consequence; the breaking-ish note) — recommend: yes. The
    alternative — keep it implementation-defined but dependency-safe —
    forfeits the byte-stability rationale and leaves effectful-init
    programs unportable across compiler versions.
(e) **Scope riders**: widen `emitted_js_is_independent_of_import_order`
    to constants; reformat `math.vl` + regenerate its golden (verified,
    per the golden discipline); the two §4 riders. Recommend: all yes.

## 6. Pins (per case, per CLAUDE.md)

Same-file forward reference now runs (output-asserted); the zeta/alpha
naive-sort counterexample byte- and run-pinned; EVEN/ODD keeps working
(the (a) guard); the call-through-global shape (`X = FETCH()`) orders
Y before X and runs; self-reference → cycle error; cross-module cycle →
error with the chain text asserted; a load-time-call-mediated cycle →
error; two effectful initializers' observable order under the new rule
(print-order pin); const-marked bindings stay folded (unchanged);
import statement AND brace permutations byte-identical including
constants (the widened corpus pin); math.vl reformatted, golden
regenerated and its diff verified binding-order-only; docs test green
over the spec amendment. Dispatch-over-approximation probe (§5(b)):
a trait-dispatching initializer with no real cycle stays accepted.

## 7. Slices

- **S1 — the graph + the order** (M): the load-time relation over
  call-graph data, topo + canonical tie-break, emission through it;
  the non-cycle pins.
- **S2 — the cycle diagnostic** (S–M): the pass, the chain message,
  wired both entry points; the cycle pins.
- **S3 — spec + docs + the carve-out closes** (S): spec §7.1/§7.6, the
  three doc comments, the widened corpus pin, math.vl's reformat +
  golden regeneration, changelog note staged for the next cut.

S1 before S2 (the graph is the diagnostic's input); S3 last. Adversarial
review before commit on S1 (it touches the live emission pipeline).
