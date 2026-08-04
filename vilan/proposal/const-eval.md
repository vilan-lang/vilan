# `const` — compile-time evaluation as a language feature

Status: **SHIPPED 2026-07-10** — the full v1, same-day as the proposal.
Slices 1–4 (the keyword, mark-and-forward + the free-variable rule, the
evaluation pass, in-place serialization — 21 pins + corpus `const.vl`), then
the **asset channel + const-only bit** (§2–3, the styling prerequisite):
`std::asset::emit` accumulates during `eval_const` only (a capability flag on
the interpreter — macro expansion and the equivalence runner reject it), the
channel dedups by line and orders lexically, EXCEPT `@media (min-width: …)`
lines, which sort as a group in ascending min-width order (B35, 2026-07-28:
the bare lexical digit sort put 1024px before 640px, so on a wide viewport
the narrow rule won the cascade tie; base `.class` < `:root` < media still
holds — argued at `assemble_assets`), and `vilan build` writes `<output>.<kind>`
beside the JS (7 pins + an end-to-end CLI test). The const-only check is the
R-fixpoint over the shared call graph: functions reaching `emit` through
non-const call sites join R, roots (`main`, top-level initializers) never
join — a root's call into R errors AT THAT call site, the outermost runtime
crossing, while `emit` inside R-functions called from `const` chains stays
legal (the styling property-function shape, pinned). Recorded refinements:
`run`/`--watch` write assets beside the canonical output each round (SHIPPED
2026-07-20, hmr.md §11 S0 — single-package `run` and the `--watch` single arm
now call `write_assets`; the workspace paths already did via
`build_workspace_artifacts`). **The rest of the tail was verified piece by
piece on 2026-08-04 and is now §8** — the indirect-call "conservative
rejection" turned out to be a silent hole and is CLOSED (§8.1); deep failure
attribution ships and expression-level spans are deferred with the question
that blocks them (§8.2); the LSP's duplicate const pass is deleted and true
Tier-2 memoization is deferred with its cache-key question (§8.3);
liveness-tied emission (dead-style elimination) stays OUT, entangled with
A7/A8. Implementation notes that
amended the design: the JS-refugee hint lives in the ANALYZER, not the
parser — `const x = 3` parses fine (assignment is an expression, so it is
`const (x = 3)`), and the forwarding arm catches the `Assign` shape with
the idiom; the `const` marker forwards to its inner expression (no wrapper
entity), so every downstream pass sees a plain subtree; and const
mini-programs skip `rename_for_scopes` so the result binding survives.
The general feature the revised styling system (`proposal/ui-styling.md`)
rides; independently motivated.

## 0. Motivation

vilan already evaluates the whole pure language at compile time — the macro
engine's interpreter runs full vilan in hermetic worlds, is equivalence-gated
against node over the entire corpus, has a depth cap, a curated host table
(`Math.*` and friends), and turns panics into diagnostics. Today that power is
reachable only by producing *source text*. `const` exposes it for producing
**values**:

```vilan
let TABLE = const build_crc_table();      // computed at compile time: a literal in the JS
let CARD = const display(flex) + padding(space(4));   // the styling use case
mut cache = const build_initial();        // compile-time initial value, runtime-mutable
```

Payoffs beyond styling: lookup tables, precomputed scales, parsed static
config, wire-format hashes (`contract_hash` can stop being compiler magic and
become plain vilan), and — through the asset channel (§3) — compile-time
*emission* of non-code build outputs. Every such value costs zero at runtime:
the emitted JS contains the result literal, not the computation.

## 1. The model

`const` is a **weak-precedence expression keyword**: it captures the largest
expression to its right within the current bracket/comma context and
evaluates it at compile time.

```vilan
let result = const 1 + 2;            // the JS contains `3`
let TABLE = const build_crc_table(); // module scope: the shared-constant idiom
f(const heavy_default(), runtime);   // argument position; stops at the comma
let narrowed = (const f()) + g();    // parenthesize to narrow the capture
```

- **One mechanism, no declaration form.** `let NAME = const expr` *is* the
  constant declaration — bindings stay ordinary `let`/`mut` (tree-shaken by
  F6, clone-sited like any binding), annotations sit on the binding as usual,
  and `mut x = const initial()` expresses a compile-time initial value for
  runtime-mutable state, which a `const` declaration could not. (`const NAME
  = expr` as sugar is deliberately not shipped — one way to say it; recorded
  as a later nicety if the corpus begs.) `const { .. }` needs no special
  case: blocks are expressions, so multi-statement compile-time computation
  falls out.
- **Evaluation**: the captured expression is evaluated at compile time by the
  macro interpreter (the worlds machinery — one evaluator, not a second
  dialect). `let` alone stays runtime; `const` is the *guarantee* (and the
  capability gate, §2) — an optimizer may fold plain initializers someday,
  but `const` promises it and errors when it can't.
- **Free variables must be const-known**: an import, a literal, or an
  immutable binding whose own initializer is a `const` expression (chaining;
  `mut` disqualifies). A parameter or runtime local errors at the reference —
  "`n` is a runtime value; a `const` expression reads only
  compile-time-known bindings". Calls are unrestricted (§2's no-coloring
  rule): only free *variables* need the judgement.
- **Serialization is in place**: the result literal replaces the expression
  at its site. A `const` inside a loop re-materializes per iteration —
  never worse than the computation it replaced (that call also produced a
  fresh value per iteration; the computation is gone, the allocation
  unchanged), and no aliasing questions arise against value semantics.
  Sharing is spelled with an ordinary binding at whatever scope you mean;
  hoisting-with-read-only-proof is a recorded optimization, not v1.
- **No function coloring.** Any function reachable from a const initializer is
  const-callable — the interpreter is total over the pure language, so there
  is no `const fn` annotation and no ecosystem split (the Rust lesson,
  avoided; this is the Zig-shaped design, available to vilan because the
  evaluator predates the feature). A const expression that reaches an
  unavailable capability fails with a **spanned static error**, not a marker
  check.
- **The result must be plain data**: numbers, strings, bools, lists, maps,
  tuples, structs, enum values — transitively. A closure, view, `Shared`
  cell, or promise in the *result* is a static error at the expression
  (internal use during evaluation is fine — the interpreter models all of it;
  only the surviving value is constrained). Value semantics makes the
  snapshot natural.
- **Failures are diagnostics**: a panic during evaluation (`Thrown` — e.g.
  the checked-subscript message), the depth cap (`Depth`), an unavailable
  capability (`Unsupported`), or a non-data result — all report at the
  `const` expression with the failure message. Deep-span fidelity (pointing
  inside the callee) is a recorded refinement shared with macro diagnostics —
  DEFERRED with its blocker stated in §8.2; the failing *function* is named
  and noted since 2026-08-04.
- **Dependencies**: const expressions form a value dependency graph through
  the const-known bindings they read (imports included); evaluation follows
  it in deterministic order (module topological order, then binding order,
  then expression order within a body); a cycle is an error.

## 2. The capability model

The const world *is* the macro world: pure vilan plus the curated host table
(math, string/collection intrinsics) — no io, dom, fetch, timers, process.
One new bit on top:

**Const-only functions.** A few std internals are legal *only* on call paths
rooted in a `const` expression — the first being `std::asset::emit` (§3),
whose whole point is a compile-time effect. Enforcement is static
reachability over the existing call graph (`src/call_graph.rs`): a call path
from runtime code into a const-only function errors at the offending call
site with what it means ("styles are compile-time values — build them in a
`const` expression", worded per API). v1 keeps the bit **std-internal** (users cannot
declare const-only functions) and requires direct call chains — a const-only
function passed indirectly (through a closure value) is conservatively
rejected — enforced since 2026-08-04 at the point the VALUE is made, which is
the only place the call graph can see it (§8.1); before that the sentence was
aspirational and the escape was silent. This is one capability bit on a
handful of internals, not function coloring: ordinary functions remain
callable from both worlds with no annotation.

## 3. The asset channel — compile-time emission

```vilan
// const-only: appends a line to the build's `kind` asset.
fun emit(kind: str, line: str): void;    // std::asset
```

During const evaluation, `emit` accumulates `(kind, line)` pairs in the
compiler. After compilation the channel, per kind:

1. **Deduplicates by line** — independent const evaluations compose into one
   output with no cross-binding coordination (the property that makes atomic
   CSS plateau).
2. **Orders deterministically** — a kind-specific rule (CSS: base < pseudo <
   media in ascending min-width order, then lexical — B35 fixed the digit
   sort that put 1024px before 640px), so outputs are byte-stable regardless
   of evaluation or caching order.
3. **Writes `<out>.<ext>` beside the compiled `.js`** (e.g. `dist/client.css`).

The channel is styling-agnostic: A7 SSR wants it for critical CSS, and any
compile-time codegen (license manifests, service worker precache lists) rides
the same mechanism. A two-target build (client + server bundles) evaluates
consts per compile; dedup makes the union coherent, and the CSS lands beside
the client bundle.

**Liveness over-approximation, recorded**: v1 evaluates every `const` and
keeps every emitted asset line, even if F6 later drops the binding from the
JS (assets are collected before assembly-time reachability). Tying emission
to binding liveness — which would give dead-style elimination for free — is
the recorded refinement, mirroring F6's own recorded over-approximations.

## 4. Cost and caching

Const evaluation runs per expression at compile time; the interpreter's
speed is corpus-proven (the equivalence suite runs whole programs), and the
worlds cache precedent applies: memoize per expression on the
dependency-closure source.
v1 ships without incremental memoization (evaluate on each compile).

**Tooling split** (settled with the user): the LSP **evaluates explicit
`const` expressions** — they are opt-in contracts, bounded in number by the
user's own hand, and their diagnostics (`space(37)` blowing a scale's
bounds) belong live in the editor — under the existing analysis debounce
and the fuel cap (an editor must survive a `while true` const mid-edit; a
capped miss reports "did not finish within the compile-time budget" like
any other evaluation failure). What the LSP **never runs is G3's inference
sweep**: inference is silent-fallback optimization by design, so it
produces no diagnostics and nothing user-visible — there is nothing for an
editor to surface; it is a build-time pass only (`vilan check` doesn't need
it either: it cannot produce errors). A design invariant keeps the
LSP-side evaluation cheap and deferrable: **no downstream pass depends on
const *values*** — the type of `const expr` is the type of `expr`, so
hover/completion/navigation never wait on evaluation, and the debounced
pass can trail typing without blocking anything. (The sharp asymmetry with
macros: the LSP must expand those, because they create items and types.
Const generics would break this invariant — a second reason they are out
of scope beyond v1 sizing.) `vilan check` evaluates explicit consts as
`build` does — check means "will it build". Incremental memoization of the
LSP-side evaluation rides the Tier-2 caching arc.

## 5. Out of scope (v1)

- Const *generics* / const parameters / `const` depending on an enclosing
  generic's type parameters. (A `const` inside a generic function body is
  legal only if its initializer is independent of the type parameters.)
- User-declared const-only functions.
- `const` *parameters* (a callee demanding compile-time arguments) — the
  expression form makes call-site `f(const ..)` free, but parameter-side
  requirements are const-generics territory, out with them.
- Cross-crate/library const export beyond what value serialization already
  gives (a library's `const` re-evaluates in the consumer's compile — fine,
  deterministic).
- Floating point: no divergence to manage — the interpreter's f64 *is* JS's
  f64 (same representation, equivalence-gated), stated for the record.

### Recorded v2: inferred `const` — DESIGNED AND SHIPPED, §9

`let a = 1 + 2;` folding without the keyword (backlog G3). No fundamental
blocker; recorded here so v1's design doesn't foreclose it. The rules that
keep it sound (each settled, and one of them found incomplete, in §9):

- **Inference is transparent; `const` stays the contract.** The explicit form
  ERRORS when evaluation fails; inference silently falls back to runtime on
  ANY failure — capability, fuel, non-data result, or a **panic**. The panic
  case is load-bearing: a dynamically-dead `if false { xs[5] }` evaluates to
  a panic but runs fine — folding it would reject a working program.
  Fallback preserves observable behavior exactly, panics included.
- **Same eligibility as the explicit form**: const-known free variables
  (which, with the plain-data rule, is also what makes internal mutation
  non-escaping — external state is unreachable without referencing it), the
  const capability world, plain-data results.
- **Const-only functions never infer.** `asset::emit` requires an explicit
  `const` root — otherwise whether a style compiles depends on optimizer
  mood. Inference folds values; it never creates const contexts.
- **Budgets are the v2-sized work**: an evaluation fuel cap (a missed fold
  beats a hung compiler) and a serialized-size cap (a 10 KB table literal
  replacing a 20-character call is a regression nobody asked for — explicit
  `const` is the opt-in for big results). Heuristics with knobs.
- **Debug ergonomics**: folded computation vanishes from stack traces; the
  `[build]` presets fit naturally — debug skips inference, release infers.
- **The LSP never runs inference** — silent fallback means there is nothing
  to surface in an editor; the sweep is a build-time optimization pass only
  (§4's tooling split).

## 6. Implementation sketch

1. **Grammar**: `const` keyword as a weak-precedence expression prefix
   (captures to the end of the bracket/comma context; lexer keyword, parser
   arm, formatter, TextMate). Parser nicety, specced here: statement-initial
   `const IDENT =` gets the JS-refugee hint — "vilan has no const
   declarations — write `let x = const ..`".
2. **Analyzer**: mark const expressions; type-check them normally (the type
   system is unchanged — const-ness is an expression property, not a type);
   enforce the const-known free-variable rule; build the dependency order;
   run the capability reachability check.
3. **Const pass**: post-analysis, evaluate marked expressions in dependency
   order via the interpreter (through the existing `transform_to_ast` path,
   as macros do); collect asset emissions; convert failures to spanned
   diagnostics.
4. **Serialization**: result value → `js::Node` literal in place (numbers,
   strings, arrays, maps, the struct/enum runtime shapes the transformer
   already defines); reject non-data results with the §1 error.
5. **Channel**: dedup/order/write per §3; `vilan build --watch` regenerates.
6. **Pins**: value classes round-trip (incl. nested enums/maps); capability
   failure spans; the free-variable rule (runtime local, `mut`, and chained
   const-known cases); weak-precedence shapes (`const a + b`, argument
   position, parenthesized narrowing); cycle detection; panic-at-const spans;
   in-place semantics in a loop; determinism of the channel across binding
   reorderings; a `const` used from both server and client layers.

## 7. Alternatives rejected

- **Rust-style `const fn` coloring** — an ecosystem-wide annotation burden
  vilan doesn't need; the interpreter's totality is the asset, use it.
- **Macros as the value channel** (the styling proposal's first draft) —
  produces source text, so every consumer pays the DSL toll: no hover, no
  go-to-def, no typed diagnostics inside the block, custom highlighting.
  Superseded by this proposal exactly to avoid that toll.
- **Build scripts** (a `build.vl` executed by the CLI) — a second program
  with its own capability story, non-composable with the module graph, and
  invisible to the type checker. The asset channel gives the useful half
  (emission) inside the language.


## 8. The G2 tail — verification, 2026-08-04

Every remaining G2 claim re-checked against the tree at `f4a51e3` (v0.26.0)
with probes before a line was implemented, the A8 pattern. Two of the four
recorded pieces did not survive contact with the code.

| Piece | Verdict | Evidence |
| --- | --- | --- |
| Deep failure spans | **OPEN, as recorded** — but the cause is deeper than "not done yet" | All three failure shapes anchor at the `const` expression, never inside the callee. Probed: a 3-level call tree ending in `xs[9]` (`Thrown`), unbounded recursion (`Depth`), a bare `for { }` (`Fuel`) — spans `level_one()`, `recurse(0)`, `spin()` respectively. `Failure` (`interpreter.rs:67`) carries a kind and a flat `String`, no location; the interpreter walks `js::Node` (`transformer.rs:6468`), which has no span on any variant, and `transform_const_program` returns no provenance side table. Macro expansion is identically shallow (`macros.rs:1363`, `span: site`). |
| Indirect-call gap | **OPEN, and MIS-RECORDED — it was a silent hole, not a refusal** | §2 said an indirectly-passed const-only function "is conservatively rejected". It was not rejected: `fun styled() { emit(..); 1 }` + `fun apply(f: \|\| i32) { f() }` + `apply(styled)` from `main` compiled clean, emitted `__emit_asset("css", …)` into the JS, and died at run time with `ReferenceError: __emit_asset is not defined`. Same for a closure literal that emits, and for a closure that merely wraps an R-member call. The fixpoint propagates only through `callers_of`, which is built for resolved `Function`/`Closure` targets alone (`call_graph.rs:81`) — a call through a value is `Indirect(Value)` and contributes no edge. No test, passing or `#[ignore]`d, covered any of it. **FIXED below.** |
| Tier-2 LSP memoization | **OPEN with a measured cost — NOT made moot by E3, but half of it is redundancy, not caching** | The base cache stores the pre-entry *world*; const evaluation runs strictly AFTER `analyze()` returns (`lib.rs:449`), so no const value ever rides it. Moot for std today — the embedded std contains **zero** `const` expressions (4 grep hits, all comments) — but not for entries. Measured (`analyze_source`, warm, 5 rounds, 16-core WSL2): 7 consts = **12.0 ms** of a 136 ms analysis (8.8 %); a 3-const styling entry = **17.0 ms** of 181 ms (9.4 %); a 1-const styling entry = **12.0 ms** of 163 ms (7.4 %); a 2000-element table = 3.5 ms. The cost is dominated by fixed overhead, not const count: `check_const_only` rebuilds a whole `CallGraph` per pass (~9.5 ms for a styling program, by the 1-vs-3-const delta), and each expression pays a fresh `transform_const_program` plus a full `entity_map` scan in `free_locals`. **And the LSP ran the entire pass twice per analysis** (`lib.rs:449`, then again at `document.rs:1087` discarding the identical map already in `program.const_results`) — so ~24–34 ms of every keystroke. **Half FIXED below (the duplicate); true memoization deferred with a question.** |
| Liveness-tied emission | **OUT** — A7/G2-entangled, untouched here (backlog A8's dead-style elimination). | |

**Sweep for recorded sub-items the backlog entry does not carry** (the entry
has drifted before). Four, none of them previously listed:

1. **Const-expression hoisting with a read-only proof** (§1, "hoisting-with-
   read-only-proof is a recorded optimization, not v1") — still open, still an
   optimization, unscheduled.
2. **`const NAME = expr` sugar** (§1, "deliberately not shipped … recorded as a
   later nicety if the corpus begs") — the corpus has not begged.
3. **The budget-failure wording** (§4 promised "did not finish within the
   compile-time budget"). That string exists nowhere in the tree; users saw the
   raw interpreter wording under a const prefix. **FIXED below.**
4. **The manifest budget claim is false.** `docs/spec/const.md` §9.3 said const
   fuel/depth are "configured by the manifest's `[macro]` section". They are
   not: `const_eval.rs` hardcodes `Limits::default()` (fuel 1 000 000, depth
   512), while `[macro]` feeds `MacroLimits` (depth **16**) to macro expansion
   only. Doc corrected to match the code. *Question, deferred: should `const`
   get its own `[const]` budget knob, or join `[macro]`'s? Joining it silently
   drops the const depth cap 512 → 16, which is a behaviour change, so it is
   not a doc-fix-shaped decision.*

### 8.1 The value escape — SHIPPED 2026-08-04

§2's rule is now enforced where the value is MADE, which is the only place the
call graph can see it. `check_value_escapes` (`const_eval.rs`) refuses two
shapes outside every `const` subtree:

- an R-member named as a function value — read off the call graph's existing
  `function_references`, which already separates coercion sites from call
  subjects (`call_graph.rs:491`), and is keyed by every function node, every
  closure node, and every module-level initializer;
- an R closure that is never immediately applied — it joins R through its own
  body but nothing calls it by identity, so no boundary error could fire.

Inside a `const` subtree nothing changes: the interpreter calls through the
value happily, and the asset still flows (pinned both ways). The diagnostic
anchors at the reference or the closure literal (A1) and states the rule (B6):
"`styled` (it reaches `asset::emit`) is compile-time-only; call it directly
inside a `const` expression — a compile-time-only function has no runtime value
form". The `asset::emit` case gained its missing backticks in passing.

Not covered, and correctly so: `emit` ITSELF as a value is already refused
upstream by fn-coercion rule 1 (externs have no value form,
`fn-coercion.md` §1). Still uncovered by design — the recorded conservative
line stands: `Indirect(GenericMember)` and `Indirect(TraitDispatch)` into an
emit-reaching method. `check_const_only` never calls `successors()`, which is
where `dispatch_candidates` would over-approximate them. *Question, deferred:
is trait/generic dispatch into R reachable at all today given that methods have
no value form, or is it a live second hole?*

Five pins (`inference.rs`), four proven red before the fix and one green
throughout as the positive control.

### 8.2 Deep failure attribution — SHIPPED 2026-08-04, spans DEFERRED

True expression-level spans are blocked on provenance the tree does not have:
`js::Node` carries no position on any variant, and the const mini-program is
built by the general transformer, so threading one means either a field on
every emitted node or a parallel side table out of `transform_const_program`.
The nearest in-repo precedent is `derived_origins` (an id-range → span table in
`analyzer.rs`). *Question, deferred: is per-node provenance worth its cost for
a compile-time-only interpreter, or should the const pass instead evaluate a
SPANNED IR — which would mean the interpreter no longer runs the same tree
codegen emits, forfeiting the equivalence gate that is its whole safety story?
That trade is the real decision, and it is bigger than G2.*

What ships instead is the attribution the trace can carry without provenance.
`Failure` gains a `trace: Vec<String>` that `call_value` appends to as the
error unwinds — one push per named frame, innermost first, nothing on the
success path (anonymous closures contribute no frame, so it is the named call
chain, not the stack). The const pass names the innermost frame in the message
and anchors a secondary note (C3) at that function's NAME span (A1), carrying
the chain elided to the innermost four — a depth miss unwinds hundreds of
identical frames, and `… → recurse → recurse → recurse → recurse` says what
512 repetitions would not. A std frame is legal in a note and would not be
legal as a primary span (A2), which is exactly the shape C3 exists for. The
frame name is printed only when it matches a declared function, so a
monomorphized or synthetic name never reaches the user (B1); the note needs a
UNIQUE match, since pointing at an arbitrary one of several would not be
deterministic (C1).

`failure.kind` stops being discarded: `Fuel` and `Depth` now render §4's
promised "did not finish within the compile-time budget", with the specific cap
named after the colon. That wording existed nowhere in the tree — users saw the
macro engine's internal phrasing under a const prefix.

Four pins, both halves plant-proven independently: dropping the trace push
reddens all four; dropping the kind branch reddens exactly the two budget pins.
En route the fourteen `Failure { .. }` literals became `Failure::new`, which is
what let the struct grow a field without touching every site twice.

### 8.3 The LSP's duplicate pass — SHIPPED 2026-08-04; memoization deferred

`document.rs` re-ran `const_eval::evaluate` purely to get hover values, while
`analyze_source` had already stored the identical map in `program.const_results`
and no one read it. Reading the field deletes a full pass — 12–17 ms of every
keystroke on a const-using entry, measured above — with no design and no
behaviour change (the second run's errors were being discarded anyway, and on a
program with const errors it returned an empty map regardless). The
`Document::const_results` field went with it: `const_value_label` already had
the `Program` in hand. Pinned by the invariant the deletion rests on — analysis
leaves the folded values on the program — plant-proven by blanking the store;
the two existing hover-value tests are the behaviour net.

That leaves the *first* pass uncached, which is the Tier-2 item proper.
*Question, deferred: what is a const expression's cache key?* Entity ids are
regenerated per analysis, so the key must be source-derived — the dependency
closure's text, as §4 sketches. Two things make that harder than the world
cache: the closure includes every function transitively reached (the
mini-program is built by a fixpoint, not a syntactic walk), and the result must
be invalidated by a std edit as well as an entry edit, so the key composes with
the base cache's content hashes rather than replacing them. The cheaper
structural win found while measuring, and not taken here: `check_const_only`
builds a whole `CallGraph` per pass (~9.5 ms on a styling entry) and
`free_locals` scans the entire `entity_map` per const expression — both are
per-analysis waste independent of any cross-analysis memo, and `platform_color`
and `init_order` build their own call graphs too. A shared per-analysis call
graph is the obvious next slice and is not const-specific.

## 9. Inferred `const` — the design, 2026-08-04 (backlog G3)

§5's "Recorded v2" is the constraint set; this section settles it into rules and
numbers. Probes before implementation, the §8/A8 pattern — and one probe found a
soundness hole §5 did not record (§9.2).

The mechanism is small because v1 built it: `Program::const_results` is a
`HashMap<Id, ConstValue>` the transformer consults for **any** entity id
(`walk_entity_inner`'s first arm), so folding a binding is nothing more than
putting its initializer's id in that map. Inference adds no serialization, no
codegen, and no new evaluator — it adds a *decision procedure* for which ids may
join, and it runs the SAME `State` machine the explicit form does, in a second
mode.

### 9.1 What infers — the filter, decided by measurement

**Universe**: every `let`/`mut` binding with an initializer, in every source —
entry, loaded modules, and **std** (std is analyzed as part of every program; a
rule that excepted it would be a special case, and the measurement below says it
does not need one). `mut` is eligible: `mut cache = <folded>` is a compile-time
initial value for runtime-mutable state, exactly what §1 already spells
`mut x = const initial()`.

Two exclusions cost nothing and gain nothing to keep:

- an initializer already `const`-marked — the explicit pass owns it;
- an initializer that is already a literal or a bare local alias — folding is
  the identity, and the attempt is pure cost.

**Everything else is attempted, and the free-variable rule is the filter.** No
extra syntactic pre-filter ships. That is a measured ruling, not a preference —
the sweep runs per build, so "attempt everything" had to be priced first. On the
largest in-tree programs (warm, release build, 16-core WSL2):

| program | analysis | candidates (std / entry) | pass free-var rule | folds |
| --- | --- | --- | --- | --- |
| `examples/walkthrough` client (browser) | 210 ms | 396 (333 / 63) | 89 | 27 |
| `examples/walkthrough` server (node) | 62 ms | 359 (311 / 48) | 69 | 23 |
| `examples/rpc` main (node) | 52 ms | 304 (259 / 45) | 53 | 19 |
| `test/style.vl` (browser) | 32 ms | 153 (153 / 0) | 17 | 13 |

Attempting all of them, with `free_locals` as v1 wrote it, costs **more than the
entire analysis**: 160 ms on the walkthrough client (76 %), 107 ms on its server
(**173 %**), 53 ms on rpc (103 %). Unshippable — and the cause is not the
evaluator. It is that `free_locals` scans the whole `entity_map` once per
expression (0.09–0.40 ms per candidate), the per-analysis waste §8.3 already
named while measuring the LSP and did not take.

So take it. A **span-sorted index of every `Expr::Local` reference, bucketed by
source and built once per program**, turns the free-variable check from a full
scan into a binary search plus a walk of the root's own span range. Same
candidates, same answers, and the whole sweep collapses:

| program | sweep, full-scan | sweep, indexed | share of analysis |
| --- | --- | --- | --- |
| walkthrough client | 160 ms | **3.4 ms** | 2 % |
| walkthrough server | 107 ms | **2.2 ms** | 4 % |
| rpc main | 53 ms | **1.6 ms** | 3 % |
| `test/style.vl` | 16 ms | **0.7 ms** | 2 % |

(0.2 ms of query plus 0.3–0.7 ms to build the index; the remainder is the
mini-program build and evaluation for the 17–89 survivors.) The explicit pass
uses the same index, so the win is not inference's alone.

The ruling therefore is: **the free-variable rule is the pre-filter.** It
rejects 78–89 % of candidates, it is the rule the explicit form already
enforces, so inference has exactly the eligibility §5 promised — and once the
quadratic is gone it is free. A second, syntactic filter would buy a couple of
milliseconds at the cost of a second definition of what infers, and the whole
value of "same eligibility as the explicit form" is that there is only one.

Two findings worth recording alongside the numbers. **84 % of candidates, and
almost every fold, are std's** (26 of 27 folds on the walkthrough client; 19 of
19 on rpc; 13 of 13 on `style.vl`, whose entry contributes no candidate at all).
Inference's value in this tree is overwhelmingly in the standard library, which
is an argument for *not* excepting std, not for excepting it. And **no fold in
the tree is large or expensive**: the biggest serializes to 33 bytes, the median
to 2, and every one of them completes within 200 fuel. That distribution is what
sizes the budgets below.

#### The one exclusion that is about soundness, not savings

Everything above is a cost argument. There is exactly one filter that is not,
and the corpus differential (§9.7) is what found it: **a binding inside a
type-parameter-dependent function body is never swept.**

`transform_const_program` builds the mini-program with no substitution context.
Inside `List<T>::sum`, whose body opens `let total = T::default();`, that does
not fail — it quietly evaluates to `undefined`, and the folded program prints
`undefined` where it printed `0`. A silent wrong answer is the worst failure
this feature can have, and it is invisible to every rule in §9.2, because
nothing went wrong as far as the evaluator is concerned.

This is not a new restriction; it is §5's recorded scope limit finally made
operational. Const generics are out of scope, and §5 already said "a `const`
inside a generic function body is legal only if its initializer is independent
of the type parameters" — a judgement the explicit form pushes onto the author,
who wrote the keyword. Inference has to make it itself, and the only safe
answer is to decline.

A function counts as type-parameter-dependent when its own generic parameters,
**any parameter's type**, or its return type reaches a `Generic` — and the
parameter clause is the load-bearing one. `List<T>::sum` has NO generic
parameters of its own; `T` belongs to the receiver type. A check that read only
`generic_parameter_constraint_ids` would have looked right, passed review, and
missed the exact bug that motivated it. Unresolved and unknown types count as
dependent too: a fold under either is unverifiable.

The cost is real and worth stating plainly. Folds drop by roughly 60 % —
27 → 10 on the walkthrough client, 23 → 12 on its server, 19 → 8 on rpc,
13 → 3 on `style.vl` — because most of std's methods take a generic receiver.
The sweep still costs 0.5–2.5 ms (1–3 % of analysis), the largest surviving fold
is 21 bytes, and the corpus differential now reports 29 of 109 programs changed
with every one of them observationally identical. Fewer folds that are all
correct is the only version of this feature worth shipping.

### 9.2 Silent fallback — and the effect rule §5 missed

Per §5 the rule is absolute: **any** failure leaves the binding runtime with
**zero** diagnostics. Concretely, every one of these falls back silently — a
free variable that is not const-known, a binding reached through a called
function that is not const-known, a dependency cycle, an unsupported capability
(`[extern]` host bindings, `__env`/`__args`/`__random_int`/`__random_float`), a
`panic` (`Thrown` — §5's load-bearing case: `if false { xs[5] }` evaluates to a
panic and runs fine), fuel or depth exhaustion, a non-plain-data result, and the
size cap. That is what makes the sweep safe to run over every binding in the
program: a wrong answer is not a mis-compile, it is a missed optimization.

Implementation-wise this is one `Mode` on the existing `State` and one
`report()` that is a no-op in `Inferred` — not a parallel pass. The alternative,
a second implementation of eligibility, is how the two forms would drift apart.

**The hole §5 did not record.** §5 says "fallback preserves observable behavior
exactly, panics included". Panics are not the only observable thing an
evaluation can do. Probed against the tree at `eb96352`:

```vilan
fun noisy(): i32 { print("side effect!"); 7 }
let x = const noisy();          // emits `const x = 7;` — and `side effect!` is GONE
```

The interpreter accumulates `console.log` into its own `stdout` and
`process.exit` into `exited`; `eval_const` returns neither, so an explicit
`const` **silently swallows both**. For the explicit form that is defensible and
stays: you asked for compile-time evaluation, and the computation — printing
included — is what you asked to move to compile time. For inference it is a
mis-compile. A working program that prints would stop printing when someone
switched preset, with no diagnostic anywhere, which is precisely the failure
mode silent fallback exists to prevent.

So the inferred form carries a rule the explicit form does not need: **an
inferred fold must be observably silent.** `eval_inferred` refuses an evaluation
that wrote to stdout or called `process::exit`, and the refusal is an ordinary
failure, so it falls back like any other. This is stated as one rule over the
interpreter's effect channels rather than three special cases, because the
channels are the thing that must stay closed as the host table grows.

The third channel is the asset channel, and it is what enforces §5's
**"const-only functions never infer"**: an inferred attempt runs with
`allow_assets: false`, so reaching `asset::emit` is an `Unsupported` capability
miss and the binding stays runtime. Enforcing it at the *reach* rather than by a
syntactic guess is both simpler and tighter — a function that could reach `emit`
but does not on this input is still foldable, and one that reaches it through
any path, direct or indirect, is not. Inference folds values; it never creates
const contexts.

### 9.3 Budgets

Explicit `const` keeps `Limits { fuel: 1_000_000, call_depth: 512 }` and has no
size cap: a budget miss there is a diagnostic (§4's "did not finish within the
compile-time budget", shipped in §8.2), so the user can see it and act. An
inferred attempt that exhausts its budget is silent, so it must be tight enough
that exhaustion is cheap and generous enough that it never bites real code:

| | explicit | inferred |
| --- | --- | --- |
| fuel | 1 000 000 | **10 000** (1 %) |
| call depth | 512 | **64** (12.5 %) |
| serialized size | uncapped | **256 bytes** |

Sized against §9.1's distribution, not by feel: every fold in the tree completes
within **200 fuel** and serializes to at most **33 bytes** (median 2) — and once
the generic exclusion above lands, to at most **21**. The numbers therefore
carry ~50× and ~12× headroom over observed reality while sitting well under the
explicit budget in every dimension. The size cap is what
§5 asked for — "a 10 KB table literal replacing a 20-character call is a
regression nobody asked for"; 256 bytes admits a small scale or lookup table and
refuses a generated one, and explicit `const` remains the opt-in for big
results.

A *relative* size rule (fold only if the literal is no longer than the source it
replaces) was considered and rejected: it makes whether a program folds depend on
its formatting, which is a determinism smell even though it is deterministic per
source, and it refuses obviously-good folds where the call is short and the
answer is a five-digit number.

These are compiler constants, not manifest knobs. §8's deferred question — should
`const` get a `[const]` budget section, or join `[macro]`'s (which would silently
drop the const depth cap 512 → 16) — is untouched here and stays open; inference
deliberately does not pre-empt it by inventing a knob of its own.

### 9.4 The `[build]` preset gate

`[build]` has exactly two presets today, `debug` and `release` (`options.rs`;
`Preset::parse` accepts nothing else and the manifest rejects the rest). There
is no `--release` flag: the preset is manifest-only, and a bare
`vilan build foo.vl` with no `vilan.toml` resolves `BuildOptions::default()`,
which *is* debug.

Inference is therefore a `BuildOptions` field like every other code-generation
knob — `infer_const`, **false** under `debug`, **true** under `release`, with a
`[build] infer-const` override beside `indent`/`spaces`/`readable-names`/
`debug-names`. Two consequences fall out for free:

- **The corpus is byte-identical by construction.** The gate builds every
  `vilan/test/*.vl` through the debug binary with no manifest, so `infer_const`
  is off and no golden can move. A corpus diff after this change means the gate
  leaked, and that is exactly what makes it a useful signal here.
- **Debug ergonomics are the reason, and they are the ones §5 named**: folded
  computation vanishes from stack traces, so the readable build keeps it.

One ruling to state rather than leave implicit: the gate is the *option*, not
the subcommand. `check` and `test` compile through the same `compile_to_js`
seam, so a release-preset project infers under all three. §4 says `check` "does
not need it", and that is true — inference produces no diagnostics — but it does
not follow that `check` should differ from `build` in what code it accepts and
how long it takes to accept it. `indent` and `readable_names` do not branch on
the subcommand either; a codegen preset that meant different things per command
is the surprise, not the consistency.

### 9.5 Determinism

Same source must fold identically across builds, and does, for two reasons that
are worth separating.

**The evaluator is already deterministic**, and that is a v1 property, not a new
one: `check_capabilities` refuses any program carrying `[extern]` host bindings
or the impure helpers `__scan`/`__env`/`__args`/`__random_int`/`__random_float`,
and there is no clock in the host table at all — `Date.now` is not a case in the
interpreter's method dispatch, which is why `time.vl` is on the equivalence
suite's excluded list and why `the_clock_is_not_const_evaluable` already pins it.
Inference inherits all of it by construction; it runs the same evaluator.

**The sweep's own order is source-derived.** Candidates are visited sorted by
`(SourceId, span.start)` rather than in `HashMap` order. Belt and braces, in
fact: the result is order-*independent* anyway, because a candidate whose free
variable is another candidate recurses through the existing `evaluate_one`,
which memoizes in `results` and detects cycles through `in_progress`. Sorting is
what makes that provable by reading rather than by trusting a hash seed.

Chaining is worth stating explicitly since §5 did not: `let a = 1 + 2; let b = a
* 2;` folds **both**, because in `Inferred` mode `classify` treats a pending
candidate as const-known and recurses. In `Explicit` mode it does not — an
explicit `const` whose free variable is a plain runtime binding must keep erroring
with §1's message. If it did not, the same program would fail in debug and
compile in release, which is the one thing the preset split must never do.

### 9.6 The LSP and wasm — unreachable, and pinned there

§4's tooling split is unconditional: the LSP evaluates explicit consts and
**never** runs the inference sweep. Structurally, `const_eval::infer` is called
from `crates/vilan-cli/src/main.rs` and nowhere else — not from
`analyze_source`, which is the function the language server, the wasm
playground, and every test harness enter through.

That is the v0.23.0 lesson restated (unconditional code in `analyze()` runs on
wasm — `Instant::now()` aborts there, and it took a deploy smoke test to find
out). The sweep contains no clock, no filesystem, and no environment access, so
linking it into `vilan-core` is safe; what must not happen is `analyze_source`
*calling* it.

Pinned the way the playground split guard is (`bundle-splitting.md` §11): a
**source-level** assertion, `include_str!` plus `contains`, that the three
analysis-side entry points never name the sweep. An output pin would be
vacuous — `analyze_source` builds with `BuildOptions::default()`, so
`infer_const` is off there whatever the call graph looks like, and a leak would
stay invisible until someone changed the default and wondered why the editor got
slow. The guard fails on the line that introduces the call, which is where the
decision is actually made.

### 9.7 The gates, and what running them found

Inference is the first optimization in the tree that rewrites what a program
computes by *running* that computation in a different engine, so the gate that
matters is not "does it still compile" but "does it still do the same thing".
Four, in increasing order of what they can catch:

1. **The corpus stays byte-identical** (`vilan-cli/tests/corpus.rs`, unchanged).
   True by construction, since the gate builds with no manifest and the default
   preset is debug — which is exactly why a corpus diff here would be a real
   signal that the preset gate leaked, rather than noise.
2. **A release golden and its debug twin** (`vilan-cli/tests/infer_preset.rs` +
   `tests/infer_preset/`). One source, no `const` keyword anywhere, compiled
   under both presets and pinned byte-for-byte, plus both run under node and
   compared. This is the only place the release path is pinned at all.
3. **Twenty pins on the sweep's own decisions** (`vilan-core/tests/
   const_inference.rs`), stated over `infer`'s result map rather than over
   emitted JS — because the interesting cases are the ones that do NOT fold, and
   a binding left alone is indistinguishable in the output from a binding nobody
   swept. Ten plants were run against them; each reddens the pin it should.
4. **The corpus differential** (`vilan-core/tests/infer_differential.rs`): every
   corpus program transformed twice off ONE analysis, with the sweep's folds
   installed and without, and every program whose emission changed run both ways
   under node. 29 of 109 change; all 29 agree.

Gate 4 is the one that earned its keep. It found the generic-context bug in
§9.1 — which no pin written from the design would have caught, because the
design did not know it was possible — on its first run.

**A pre-existing release-preset bug, found in passing and NOT fixed here.** The
differential's first draft compared release-with-inference against
release-without, and seven programs failed. **None of the seven was a folding
bug.** All were the release preset's own short-name renaming colliding, and all
seven reproduce on the **shipped v0.27.0 binary** with a `preset = "release"`
manifest and no inference anywhere near them:

| program | what v0.27.0 emits under `release` |
| --- | --- |
| `default.vl` | two module-level `function b` — the second shadows the first into infinite recursion |
| `capture-clones.vl` | `for (const p of …) { const o = p; let p = null; … }` — TDZ on `p` |
| `derive-json.vl`, `iterator-protocol.vl`, `value-semantics.vl`, `map.vl`, `list-element-type.vl` | `SyntaxError: Identifier '…' has already been declared` |

Inference exposed the same defect on an eighth, `json-roundtrip.vl`, which is
clean on v0.27.0 and collides once folding changes which bindings survive — so
the sweep does not cause the bug but can move which programs trip it. Filed as a
finding, not patched: it is a codegen-renaming arc of its own, and folding it
into an inference change would bury both.

That is why the differential compares two DEBUG builds, one with the sweep
forced on — and the reason is stronger than "less noise". Observational
neutrality is a property of folding, not of the printer, but note what the
release comparison actually did: `list-element-type.vl` is in the table above,
so under release BOTH of its builds were already dying in the renaming bug, and
the generic-context error printing `undefined` was **masked**. Confounding the
two knobs did not merely add failures to read past; it hid the real one. The
release path keeps its own pin, gate 2.
