# REPORT — tuples-43 (Order 43): A122's build, B183, B399

Branch `tuples-43`, worktree `vilan/.claude/worktrees/tuples-43`. Three items LANDED, one commit each, each
with its CHANGELOG entry appended under next's `## Unreleased` (family `feature`). REBASED onto origin/next
@3cb4be55 (solver-43 merged) — tip 5ec2697f — and every gate re-run on the rebased tree (below). Rebase
conflicts resolved: CHANGELOG (entries appended, one heading), diagnostics-ledger.tsv (upstream rows 576–582
kept, this lane's four rows appended as `NEW`), transformer.rs `Expr::ForEach` head (solver's `maybe_clone`
line first, then the tuple-walk branch). analyzer.rs, reactive.vl (reactive-43's docs kept, `divorce`
added beside `combine`), types.md and tuples.rs merged clean. Pre-rebase shas: e5f61185 / 988c76cd / 6008fde4.

## Items

### B399 — LANDED 1d6ebbf3
The element bound of `T: (2..: PartialEq)` reaches the binder `U` of every mapped type over `T`
(`mapped_binder_sources`, read at use time by `generic_bound_traits`; materialized into `generic_bounds` for
the emitters). A bounded binder always UNROLLS: a shared `.map` body cannot dispatch per element (measured:
structurally compared, a `Loose` whose `eq` ignores a field answered `false` against itself; through
`Display` the body hit the bodiless requirement at emission). Premise verified (papers-41 P12).
Pins 5 (`tuples::b399_*`; 3 red-first, the unroll rule planted off reds the 3 runtime pins). Goldens: none.
Docs: types.md §5.9.

### A122 — LANDED e52e3cd7 (Q1–Q7 as ruled 2026-09-25)
- `vilan/std/src/tuple.vl`: `external struct TupleKey<T, U>` (opaque — no constructor, Q3), `trait Tuple
  { len, keys, entries, get, map }`, `impl type T: (2..) with Tuple` with five INTRINSICS lowered against
  the receiver's concrete flat layout per instance (`Intrinsic::Tuple{Len,Keys,Entries,Get,Map}`; the rust
  backend refuses them through its existing `host_gap` arm).
- §4.2 a value of `T: (2..)` is a comprehension source (`(U in T: U)`). EVERY walk (comprehension, `for`,
  `map`) now gets a binder fresh to it, rigid file-wide (`tuple_family_view`); nested walks print `U`/`U'`.
- §4.3 the family rule in `reconcile_type` (+ read-only twin in `compare_type_rigid`):
  `TupleKey<T, U>` ≡ `TupleKey<(V in T: F<V>), F[V := U]>`, through any number of mappings; unrelated
  families refused. Tuple-family receivers resolve methods through B408's blanket route with tuple-bound
  entailment (`tuple_family_entails`); `std::tuple` members are wired statically (a by-name re-dispatch of
  `get` read as possibly-async in an rpc program).
- §4.4 (Q2) a plain `for` over a family: body checked once; emitted `for...of` or per-position branches in
  one loop (`break`/`continue` keep meaning). Concrete tuples and `&mut` walks keep B209's refusal.
- §3 (Q1) `t.map(|x| e)` IS the comprehension; closure value refused; annotation must restate the element.
- `reactive.vl`: `divorce<T: (2..), S: Source<T>>(source: S): (U in T: Map<S, T, U>)` =
  `source.get().keys().map(|key| source.map(|whole| whole.get(key)))`. The signature is generic over the
  source (tuple-module.md §6.3's post-S2c form) rather than the brief's `SignalCell<T>` — a superset, and
  what makes the item's own round-trip pin `divorce(combine((a, b)))` direct. Q4: no gate; every output
  fires on every source change — stated in the doc and pinned (1 + 3 fires each over 3 sets).
- Two substitution gaps fixed on the way: `substitute_type` had no `Dyn` arm, and never substituted an
  abstract mapped type's TEMPLATE.
- Pins 20 (`tuples::a122_*`; family rule, escape and nested-walk pins planted red) + 3 lifted-refusal pins
  rewritten as passes (E223's, the bare-pack pin, B209's mapped-loop pin → `b209_a_mapped_tuple_loop_types_
  its_binder_since_a122` + `b209_a_mutable_walk_over_a_mapped_tuple_is_still_refused`).
- Golden moved: `vilan/test/spread-parameters.mjs` — CLASSIFIED RENAME-ONLY (one generated name burnt by a
  second instance key over the per-walk binder): identical with `$names` normalized, and both goldens print
  the same when run. Markdown anchor golden +2 lines (`std/misc.md h2 stdtuple`, `std/reactive.md h2
  divorce`), regenerated with mdbook v0.5.4.
- Docs: types.md §5.9 (families, keys, `map`, the `for` rule), grammar.md §3.5 prose, std/misc.md
  `## std::tuple`, std/reactive.md `divorce` row + section. No count word changed.

### B183 — LANDED 5ec2697f (the zip form; the CONCRETE arm still unbuilt)
`(a in aa, b in bb => e)`: sources must be ONE family (mappings of one `T`, or a value of it) — equal
arity by construction; two families refused naming both; a concrete source keeps the plain refusal.
Node reshaped to `TupleComprehension { bindings: Vec<ComprehensionBinding>, body }` (smaller; node_size
green); `Expr::TupleComprehension(Vec<(binder, source)>, body)`; parser, formatter, elements/css/lift,
call_graph, init_order, liveness, and three shape-only edits in vilan-rust. Emission unrolls, each source
at its OWN flat offsets. EBNF §3.6 `comprehension-binding` (grammar_ebnf/grammar_sync green).
ALSO in this commit, A122's regression found by checking kolt on a scratch copy: a blanket over tuples
stayed a candidate for non-tuple receivers, so `list.iter().map(..)` found `Tuple::map` and never reached
`Iterator::map`'s inherited default (`tuple_blanket_excludes`; pinned, planted red). Consequence: syntax-42's
pin `5.arity()` now reads `i32 has no method 'arity'` (was `'i32' is not a tuple`) — updated.
Pins 6 + formatter `zipped_tuple_comprehension`. **B183 should stay open (or be re-filed) for the concrete
arm** (piece 3: `(x in (1, 2) => x + 1)`, and the `(a, b) + (c, d)` motivating impl) and its Q2 (unroll a
`for` over a concrete tuple) — both still the owner's.

## Ledger rows (four, written `NEW` — integration numbers them from 584)
`` `map` over a tuple takes one closure LITERAL with one parameter… `` · `'{value_label}' is a
mapped tuple whose family is not bound to guarantee '{bound_label}'…` · `cannot access field
'{member_name}' on type {subject_str}: a tuple of an abstract family has no numbered positions…` ·
`a zipped comprehension walks ONE tuple family, and this source…`. All `-` (not flagship). E223's
bounded-parameter variant of row 80 is gone with its premise (row 80's key still lives).

## Kolt edit (channel.vl; the owner applies) — verified: a scratch copy checks 0/0 and builds
The brief's `channel.vl:19 // TODO: Implement` is stale: kolt f04d4cb has a hand 2-arity `divorce` at :26.
```diff
-fun divorce<A, B>(source: SignalCell<(A, B)>): (SignalCell<A>, SignalCell<B>) {
-	(source.map(|(a, _)| a).cell(), source.map(|(_, b)| b).cell())
-}
-
@@ (in channel_component, the when_some body)
-				let (message, author) = divorce(pair);
+				let (message, author) = reactive::divorce(pair);
```
(`std::reactive` is already imported as a module.) Semantics: std's outputs are cold `Map` nodes where the
hand form `.cell()`'d each; every use here (`message.get()`, `.map(..)` on each) reads them fine. To keep
the cached cells exactly: `let (message, author) = reactive::divorce(pair); let (message, author) =
(message.cell(), author.cell());`.

## Gates (on the REBASED tree, tip 5ec2697f)
`cargo fmt --check` clean; `cargo clippy --workspace --all-targets -D warnings` clean; `vilan fmt vilan/std`
clean (no change); `-p vilan-core`: inference 4764 passed / 1 ignored, interpreter 156, docs 11,
docs_enum_parity 5, markdown_golden 2, node_size 3; `-p vilan-cli`: corpus 12, split 13, examples 6,
copy_elision_census 2, diagnostics_ledger 25, shared_census 2, grammar_ebnf 13, grammar_sync 27, hygiene 3,
book_mirrors 2 — all green; `native_differential` 76/76 default AND `VILAN_NATIVE_DIFFERENTIAL=1`. No golden
needed regenerating over the merged tree. Not run: `cargo test --doc`, the whole nextest suite (seal's).

## Finds to file
1. **UNSOUND, fixed in A122 (file closed-by e52e3cd7)**: two comprehensions over one mapped value shared
   the source's binder, so `(a in cells => (b in cells => { a.set(b.get()); 0 }))` compiled and wrote
   position 1's `str` into position 0's `i32` cell. Pin `a122_two_walks_over_one_family_do_not_share_an_
   element_type`.
2. A mapped parameter with a CONSTANT template refuses its concrete argument when `T` is bound by another
   parameter: `fun f<T: (2..)>(labels: (U in T: str), cells: (U in T: SignalCell<U>))` called with
   `(("a", "b"), cells)` → `Expected (U in ((i32, i32), str): str), but got (str, str)` (the substituted
   mapped type is not expanded). Pre-existing on 0.41.1. Repro: scratchpad
   `tuples-43/finds/mapped_constant_template.vl`.
3. Tuple destructuring does not check arity against nesting: `let (a, b, c, d, e, f) = ((1, 2), (3, 4),
   (5, 6));` compiles and reads the flat slots. Pre-existing on 0.41.1. Repro:
   `tuples-43/finds/flat_destructure_arity.vl`.
4. A bare `None` in a mapped-tuple argument is not inferred: `count((Some(1), None, Some("two")))` against
   `(U in T: Option<U>)` → `Expected (U in T: Option<U>), but got (Option<i32>, Option<unknown>,
   Option<str>)`. Pre-existing on 0.41.1. Repro: `tuples-43/finds/mapped_none_argument.vl`.
5. (note) A tuple element is not `PartialEq` (`element 2 of '(i32, str, (i32, i32))' is '(i32, i32)',
   which does not implement trait 'PartialEq'`), so an element-bounded pack cannot hold a pair.

## Owner questions (none blocking; recorded)
- `Tuple::map`'s declared signature is a placeholder (`fun map<F>(self, transform: F): Self`) — the call
  is typed by the comprehension rule; hover/completion show the placeholder. Acceptable, or hide it?
- B183's concrete arm (and its Q2) remain open, as above.

## Expected conflicts (after the rebase)
- None with origin/next @3cb4be55 (this branch sits on it).
- syntax-43 touches the same parser/formatter/grammar spots (`parse_tuple_comprehension`, the formatter's
  comprehension arm, grammar.md §3.5/§3.6) — rebase with care; the comprehension node is now
  `TupleComprehension { bindings, body }`.
- The spread-parameters.mjs and markdown anchor goldens moved here; regenerate over the merged tree if a
  later lane moves them too.

## Lane status (LANE-STATUS.md content, not committed per this order's rule)
| item | state | sha |
|---|---|---|
| B399 | LANDED | 1d6ebbf3 |
| A122 | LANDED | e52e3cd7 |
| B183 | LANDED (zip form; concrete arm unbuilt) | 5ec2697f |
Premise checks (762c6aa5): papers-41 probes re-run — P02/P03/P05/P08/P12 refused as the paper said; B397 and
the impl-binder tuple bound had landed; A122's channel.vl:19 TODO is stale (hand `divorce` at :26).
