# The diagnostics ledger (diagnostics-standard.md §5)

Every `diagnostics.push` site, its message head, and its audit verdict.
Verdicts: **QUALIFIES** (rules met, pin cited) · **RE-ANCHOR** (fails
A-rules) · **REWORD** (fails B-rules) · **DEMOTE** (cascade) ·
**NOTE-NEEDED** (wants C3) · *(blank = unreviewed)*. Line numbers are the
snapshot at generation; the message head is the stable key. Updated per
audit batch, in the batch's commit.

**Batch 7 (continuation), 2026-07-21.** The standard's "180/180 —
AUDIT COMPLETE" is the 2026-07-16 snapshot. The arcs that shipped after it
(C4 resources/destruction, R11 generic-resource, B29 conformance, A13 HMR
transfer, R12 resource-`any`, the async-polymorphism transitive checks)
added 23 new `diagnostics.push` sites — enumerated by diffing `9f59099..HEAD`
(20 new analyzer messages + 3 async_infer; the two `[rpc]`-Wire sites and the
`async` closure-type-position site only relocated). All 23 verdict QUALIFIES:
each was born inside a proven, pinned arc and already follows the B-rules
(rendered types, one-action steers, rule statements) with a family pin; span
quality spot-checked (B29 anchors at the offending parameter/return). The
five `could not be resolved` residual rows (142–145, 147) are finalized
**DEMOTE** — the `!self.diagnostics.is_empty()` guard suppresses them behind
any real error, verified with a multi-use-site pin. No new cross-source note
producer points into `std` for a user-caused condition (backlog item 11 /
E11): the only into-`std` notes are the bound- and trait-declaration notes,
both control cases — see that item's finding.

**H6 S5 cutover, 2026-07-21.** The handwritten frontend (`frontend.md`) replaces
chumsky at the S5 cutover; its parse diagnostics (`parsing::parse` +
`parsing::render`) now flow through the pipeline's fold sites (`analyze_source`,
the module loader, macro expansion, the CLI report), replacing chumsky's
`render_parse_error`. Five message forms go live (204–208), all QUALIFIES — the S4
error-quality pass built them to the standard (found/expected/context/hint, curated
expectations with none of chumsky's `context clause` / `generic arguments` noise, a
structural `!=`-soup hint, the B6 misplaced-`resource` rule), and the S5 review
added FIX 1 (a recovered CLOSED delimited region surfaces its real *located* inner
error when a committed demand or leaf recorded one — never a false "unclosed" on a
missing separator/typo; only a GENUINELY GARBLED region, declining at its first
token, falls back to the production-naming "unclosed" last resort) and FIX 2 (the
committed close demands — closing `) ] } >` via `expect_ctrl`, a closure's `|` via
`expect_op`, and each list's `,` separator — record what they wanted, so a missing
separator steers to "`,` or <closer>" at the offending token instead of the leading
keyword; opening delimiters and item keywords stay silent, keeping the expected set
curated). Pins: `parsing.rs` `render_*` (8) + `inference.rs` the three steers
(`the_not_equals_soup_hints…`, `an_unclosed_generic_steers…`,
`a_missing_parameter_comma_steers…`).

**Batch 8 — the L5 recheck (beta.md §3.1's prerequisite), 2026-08-19.**
The sweep brings the ledger current with cycles 16–23 and rechecks every
recorded head against the tree (fixed-string search over the sources: 162
of 214 matched verbatim; the misses triaged by hand against `git log -S`).
Findings, then the deltas:

1. **A wholesale rewording pass changed dozens of shipped heads and never
touched the ledger.** Commit 93d73f57 (2026-07-28, "the de-AI-ism pass")
swept ~200 user-facing strings — em dashes became colons/semicolons,
`vilan` → `Vilan` — including some twenty recorded heads. The standing
rule (2026-07-21) owes the ledger every head change in the batch that
ships it; under beta §3.1 (ratified 2026-08-18, after the pass) each
would be a `### Breaking` entry. Batch 8 re-keys the affected rows in
place (22, 24, 35, 36, 90, 91, 155–158, 160, 166, 169, 177, 184, 193,
204–206, 210, 211), each marked `re-keyed batch 8`. Heads whose recorded
60-char key ends before the changed span keep their key and needed no
edit. Three more heads were changed by ARCS, also without a ledger
update at the time: row 45 (the `sync`-contract arc merged the
async/sync type-position arms into "a closure-type marker is not valid
here", 3b5e1db5 2026-07-17), row 71 (batch 3's reword, then B75's
`not_callable_message` factoring with four reason suffixes, 1c7c41d7
2026-08-06), rows 85/91 (the user-Lift arc unified the bare-`?`/`?.`
sites behind an `{operator}` parameter and widened the alternatives,
32561077/6459cc06).

2. **The original 181-site enumeration missed the context pass
entirely.** `context.rs` (std::context, 2026-06-17) reports through
`thread_contexts`' own error channel, not a textual `diagnostics.push`,
so its sites — 11 today, the family extended since by the ambient-owner
and reactive-turns arcs — never entered the ledger. Rows 215–225 add
them; A25's owner-scope coverage complaint and E68's cascade both live
here.

3. **E68 — the coverage-error cascade — is FIXED in this batch's
change-set.** Any `owner_scope` coverage failure left `Context::run`
calls unrewritten (the pass refuses its rewrite after reporting), and
the host-boundary checks (rows 156/202) then judged std's own
async-into-`run` bodies as host-await misuses: up to two spurious
secondaries anchored IN STD (task.vl:109, rpc.vl:872) beside the
primary. `run` is `external` only as a type-checking fiction — the
threading pass erases every call it accepts — so both arms now skip the
intrinsic by its recorded id; a genuine extern misuse still errors.
Pins: `e68_an_uncovered_effect_reports_only_the_coverage_primary`,
`e68_a_refused_run_shape_reports_only_the_context_primaries`,
`e68_a_generic_forward_into_run_does_not_cascade_transitively` (all
three red pre-fix). Record: async-polymorphism.md §A.4.

4. **The cycles' arcs, verdicted.** B124 (Never merges, f94cfeac) added
zero message strings and reworded zero — it moves the incidence of the
merge/return-position families (rows 92–94, 104, and the house
`Expected/got` form) onto the branch at fault; no row owed. B73's rows
213/214 are verbatim-accurate today; R2 ships no message. The adjacent
B83 static-path ambiguity was never rowed — row 226 now. E66/E67's
completion recovery is diagnostic-neutral: one new curated expectation
(`a method name`, parsing.rs:2313) inside row 204's form; its rendered
text is pinned structurally (parser_recovery.rs
`recovers_an_unfinished_chain_link_keeping_the_element`) but not
textually — flagged. The S1/S2 synchronizer (f4a39cb3, 2026-08-11)
added two render forms rows 204–208 predate — rows 227/228 — and the
`ParseErrorReason::Rule` class has grown to ten curated statements,
nine unrowed — row 229. The fullstack process layer's refusals (E56
S2/S3 and S4/S5, 2026-08-11/12) are RUNTIME refusal strings in std — no
spans, so the A-rules do not apply; per L5's charge they enter as rows
230–239, judged on wording (B-rules) and pins (C2). std::watch,
`Service`, process/http/fs/db carry ZERO refusal messages — verified,
not assumed (watch is silent by design; Service's misses are structural
fallthroughs; fs/db surface raw host exceptions — a gap the L3 tier
sweep may want to see). Pin gaps flagged: `BuildError::Unreadable` has
no test anywhere; no test drives `require_build`/`require_shell` to
their panic end-to-end (their texts are pinned only through
`build_of`/`serve_build`/`check_shell` tests). Style note: the process
layer's messages use the em-dash style 93d73f57 removed from the
compiler surface, though they shipped two weeks after it — REWORD
candidates if the house style governs std refusal text; owner's call.

Residue: ten rows (1, 10, 18, 23, 110, 120, 123, 149, 154, 180) still
carry EMPTY heads from the batch-7 generation — keyless, so this
recheck cannot verify them mechanically; their stale line numbers are
all that identifies them. Rows 155/156 were in that state and get real
heads now (they are E68's sites). Filling the remaining ten is owed.

| # | Site | Message head | Verdict |
|---|------|--------------|---------|
| 1 | analyzer.rs:1856 | `` |QUALIFIES — reviewed in the batch-7 sweep: B6-shaped rule statement from a designed arc; pin via its family's suite |
| 2 | analyzer.rs:1960 | `{label} of `[derive(Wire)]` type `{type_name}` is `{rendered` |QUALIFIES — recursive all-fields checks name the offending field (derive pins) |
| 3 | analyzer.rs:2043 | `{label} of `[derive(Hashable)]` type `{type_name}` is `{rend` |QUALIFIES — derive all-fields check (hashable pins) |
| 4 | analyzer.rs:2106 | `{label} of `[rpc]` method `{method_name}` is `{rendered}`, ` |QUALIFIES — §4.2 contract checks (transport pins) |
| 5 | analyzer.rs:2118 | `{label} of `[rpc]` method `{method_name}` must declare a Wir` |QUALIFIES — §4.2 contract checks (transport pins) |
| 6 | analyzer.rs:2150 | `{label} is `[expose]`d, but its element `{rendered}` is not ` |QUALIFIES — Wire-element checks name the field + type (transport pins) |
| 7 | analyzer.rs:2165 | `{label} is `[expose]`d, but its type `{rendered}` is not a ` |QUALIFIES — Wire-element checks name the field + type (transport pins) |
| 8 | analyzer.rs:3167 | `a view cannot escape its scope: it may not be returned, stor` |QUALIFIES — view-invalidation E1/E2/E3 family (~25 pins); event-named wording |
| 9 | analyzer.rs:3872 | `an async function cannot take {form} parameters: the view wo` |QUALIFIES — view-invalidation E1/E2/E3 family (~25 pins); event-named wording |
| 10 | analyzer.rs:3946 | `` |QUALIFIES — reviewed in the batch-7 sweep: B6-shaped rule statement from a designed arc; pin via its family's suite |
| 11 | analyzer.rs:3998 | `an async closure cannot capture the view '{name}': the captu` |QUALIFIES — view-invalidation E1/E2/E3 family (~25 pins); event-named wording |
| 12 | analyzer.rs:4420 | `cannot reseat a view to '{name}', which goes out of scope be` |QUALIFIES — view-invalidation E1/E2/E3 family (~25 pins); event-named wording |
| 13 | analyzer.rs:4700 | `cannot mutate immutable '{name}'; {advice} to allow mutation` |QUALIFIES — B4 advice names the fix (mutability pins) |
| 14 | analyzer.rs:4758 | `cannot mutate immutable '{name}'; {advice} to allow mutation` |QUALIFIES — B4 advice names the fix (mutability pins) |
| 15 | analyzer.rs:4864 | `a view can't be read as a value here; write `*` to copy the ` |QUALIFIES — view-invalidation E1/E2/E3 family (~25 pins); event-named wording |
| 16 | analyzer.rs:4894 | `cannot take a writable view of immutable '{name}'; {advice} ` |QUALIFIES — view-invalidation E1/E2/E3 family (~25 pins); event-named wording |
| 17 | analyzer.rs:4938 | `view binding '{name}' cannot be `mut`: a view cannot be rebo` |QUALIFIES — view-invalidation E1/E2/E3 family (~25 pins); event-named wording |
| 18 | analyzer.rs:4958 | `` |QUALIFIES — reviewed in the batch-7 sweep: B6-shaped rule statement from a designed arc; pin via its family's suite |
| 19 | analyzer.rs:5019 | `a `{kind}` parameter takes a view; pass `{kind} <place>` (th` |QUALIFIES — view-invalidation E1/E2/E3 family (~25 pins); event-named wording |
| 20 | analyzer.rs:5688 | `an array length must be a non-negative integer literal ` |QUALIFIES — B6 + the const-length roadmap note (fixed-arrays pins) |
| 21 | analyzer.rs:5711 | `the `?` lifts this condition to an `Option`/`Result`, which ` |QUALIFIES — lift family (chain + region pins) |
| 22 | analyzer.rs:18122 | `Vilan has no const declarations; write `let x = const ..`` |QUALIFIES — const-eval family (21 pins): capability/free-variable wording with reference spans; re-keyed batch 8 (93d73f57) |
| 23 | analyzer.rs:5772 | `` |QUALIFIES — reviewed in the batch-7 sweep: B6-shaped rule statement from a designed arc; pin via its family's suite |
| 24 | analyzer.rs:18241 | `a tuple position is a bare number (`.0`, `.1`); drop the suffix` |QUALIFIES — flat-storage family (tuple .0 pins, 12); re-keyed batch 8 (93d73f57) |
| 25 | analyzer.rs:5885 | `expected a method name after `.`` |QUALIFIES — parse-adjacent, dot-anchored (H18 pins) |
| 26 | analyzer.rs:5895 | `expected a field or method name after `.`` |QUALIFIES — dot-anchored recovery (H18 pins) |
| 27 | analyzer.rs:5959 | `a `[T; n]` array type isn't a value; write an array ` |QUALIFIES — B6 steer to the literal forms |
| 28 | analyzer.rs:6081 | `a `macro fun` must be a top-level item` |QUALIFIES — engine family: site-anchored, previews (macro-engine pins) |
| 29 | analyzer.rs:6155 | `the invocation `macro {name}(..)` was not expanded — splice ` |QUALIFIES — engine family: site-anchored, previews (macro-engine pins) |
| 30 | analyzer.rs:6184 | `this `macro { .. }` block was not expanded — a block cannot ` |QUALIFIES — engine family: site-anchored, previews (macro-engine pins) |
| 31 | analyzer.rs:6201 | ``export` is a module-level item and cannot appear inside a b` |QUALIFIES — H2 body-export rule (scoped-import pins) |
| 32 | analyzer.rs:6326 | `an `external` function cannot have a body` |QUALIFIES — B6 declaration-shape rule |
| 33 | analyzer.rs:6375 | `function '{}' must have a body or be declared `external`` |QUALIFIES — B6 declaration-shape rule |
| 34 | analyzer.rs:6532 | `a bare `?` (expression lifting) is not supported in this pos` |QUALIFIES — lift family (chain + region pins) |
| 35 | analyzer.rs:19037 | ``?` lifts nothing here: the region is the whole expression;` |QUALIFIES — expression-lifting pins (15); re-keyed batch 8 (93d73f57) |
| 36 | analyzer.rs:19055 | ``!` cannot run after a `?` inside a lifted expression: it wo` |QUALIFIES — expression-lifting pins (15); re-keyed batch 8 (93d73f57) |
| 37 | analyzer.rs:6636 | ``!` requires the nearest enclosing function to declare an `O` |QUALIFIES — Origin-labeled reachability chains (platform-coloring pins) |
| 38 | analyzer.rs:6697 | `a `context` clause is only supported on a closure type` |QUALIFIES — coverage-fence family (ambient-owner pins); B6 names run/extent rules |
| 39 | analyzer.rs:6788 | `a destructuring `let` requires a value` |QUALIFIES — Origin-labeled reachability chains (platform-coloring pins) |
| 40 | analyzer.rs:6802 | `cannot assign through `*`: a view is written through directl` |QUALIFIES — view-invalidation E1/E2/E3 family (~25 pins); event-named wording |
| 41 | analyzer.rs:6816 | `a lifted chain (`?.`) is not an assignment target` |QUALIFIES — B6 (lift/place pins) |
| 42 | analyzer.rs:6860 | `struct '{}' must declare a body or be declared `external`` |QUALIFIES — B6 declaration-shape rule |
| 43 | analyzer.rs:7310 | `a closure type is not valid here (expected an expression)` |QUALIFIES — B6 (type-position rule) |
| 44 | analyzer.rs:7318 | `a `context`-typed closure type is not valid here (expected a` |QUALIFIES — coverage-fence family (ambient-owner pins); B6 names run/extent rules |
| 45 | analyzer.rs:19909 | `a closure-type marker is not valid here (expected an express` |QUALIFIES — B6 (type-position rule); re-keyed batch 8: the sync-contract arc merged the async/sync arms and generalized the wording (3b5e1db5, 2026-07-17) without a ledger update |
| 46 | analyzer.rs:7337 | `a mapped tuple type is not valid here (expected an expressio` |QUALIFIES — flat-storage family (tuple .0 pins, 12) |
| 47 | analyzer.rs:7427 | `a `context` clause is only supported on a closure type` |QUALIFIES — coverage-fence family (ambient-owner pins); B6 names run/extent rules |
| 48 | analyzer.rs:7715 | `cannot find '{}' in this scope` |QUALIFIES — B4 steer added (batch 1); pins: an_unknown_value_steers…, an_unknown_name_gets_no_bogus_steer, B.27 family |
| 49 | analyzer.rs:7726 | `'{}' is not an enum variant` |QUALIFIES — payload-arity + resolution wording (match pins) |
| 50 | analyzer.rs:7764 | `variant '{}' does not belong to the matched enum` |QUALIFIES — payload-arity + resolution wording (match pins) |
| 51 | analyzer.rs:7773 | `cannot match an enum variant against type {}` |QUALIFIES — payload-arity + resolution wording (match pins) |
| 52 | analyzer.rs:7807 | `variant '{}' carries {} {}, but the pattern has {}` |QUALIFIES — payload-arity + resolution wording (match pins) |
| 53 | analyzer.rs:7867 | `this pattern binds {} {}, but the array's length is {}` |QUALIFIES — array-destructure count check (destructuring pins) |
| 54 | analyzer.rs:7884 | `cannot destructure {rendered} as a fixed array — ` |QUALIFIES — B6 names the pattern's domain (destructuring pins) |
| 55 | analyzer.rs:7915 | `literal pattern of type {} cannot match type {}` |QUALIFIES — B2 both sides (match pins) |
| 56 | analyzer.rs:14180 | `an `async` closure type is only supported on parameters, `let` annotations, struct fields, and function return types` |QUALIFIES — B6 marker position; relocated + reworded (widened to struct fields / return types) since the snapshot, pin re-confirmed (inference.rs:18940) |
| 57 | analyzer.rs:8033 | `a `context` clause is only supported on a parameter's closur` |QUALIFIES — coverage-fence family (ambient-owner pins); B6 names run/extent rules |
| 58 | analyzer.rs:8740 | `this array literal has {} element{}, but its type is `[_; {l` |QUALIFIES — count-vs-type wording (fixed-arrays pins) |
| 59 | analyzer.rs:8770 | `Expected {expected} (this literal's element type), but got {` |QUALIFIES — unified list/array element wording (heterogeneous-literal pins) |
| 60 | analyzer.rs:8847 | `Expected {expected} (this literal's element type), but got {` |QUALIFIES — unified list/array element wording (heterogeneous-literal pins) |
| 61 | analyzer.rs:10184 | ``self` import has no enclosing namespace` |QUALIFIES — B6 import-shape rule |
| 62 | analyzer.rs:10204 | `cannot find module '{}' to import` |QUALIFIES — A4 segment anchor (E7 pass 1 pins) |
| 63 | analyzer.rs:10240 | `cannot find '{}' in the imported path` |QUALIFIES — B4 steer added (batch 1); pins: an_unknown_value_steers…, an_unknown_name_gets_no_bogus_steer, B.27 family |
| 64 | analyzer.rs:10483 | `Expected {} {}, but got {} instead.` |QUALIFIES — arity anchors at the arguments (they ARE the problem) |
| 65 | analyzer.rs:10549 | `Expected {}, but got {} instead.{}` |QUALIFIES — B2 + B3 note (B13 first-call origin); pin a_conflicting_later_call… |
| 66 | analyzer.rs:10577 | `Expected {} {}, but got {} instead.` |QUALIFIES — arity anchors at the arguments (they ARE the problem) |
| 67 | analyzer.rs:10605 | `Expected {}, but got {} instead.` |QUALIFIES — B2 (both sides rendered), value-anchored (A4) |
| 68 | analyzer.rs:10644 | `Expected {} {}, but got {} instead.` |QUALIFIES — arity anchors at the arguments (they ARE the problem) |
| 69 | analyzer.rs:10722 | `Expected {}, but got {} instead.` |QUALIFIES — B2 (both sides rendered), value-anchored (A4) |
| 70 | analyzer.rs:10776 | `cannot call '{name}': it is a struct, not a function — const` |QUALIFIES — B6 steer; subject-anchored (batch 3) |
| 71 | analyzer.rs:24251 (not_callable_message) | `cannot call this as a function: it is {rendered}` (+4 reason suffixes: external / generic / async / method value forms) |QUALIFIES — REWORDED to render the type + subject-anchored (batch 3); pin a_non_function_call_names…; re-keyed batch 8 (batch 3's reword was never keyed; B75 factored the message and added the four reasons, 1c7c41d7; dash→colon 93d73f57) |
| 72 | analyzer.rs:10886 | `{} has no method '{}'` |QUALIFIES — RE-ANCHORED to the method name (batch 3); pins a_no_method_error_anchors…, an_array_no_method… |
| 73 | analyzer.rs:10895 | ``len` takes no arguments` |QUALIFIES — arity anchors at the arguments (they ARE the problem) |
| 74 | analyzer.rs:11186 | `{} has no method '{}'{}` |QUALIFIES — RE-ANCHORED to the method name (batch 3); pins a_no_method_error_anchors…, an_array_no_method… |
| 75 | analyzer.rs:11200 | `cannot call method '{}' on {}` |QUALIFIES — RE-ANCHORED to the method name (batch 3) |
| 76 | analyzer.rs:11213 | `cannot call '{member_name}' on a value of bare trait type ` |QUALIFIES — B6 (B4-family pins) |
| 77 | analyzer.rs:11248 | `Expected {}, but got {} instead.` |QUALIFIES — B2 (both sides rendered), value-anchored (A4) |
| 78 | analyzer.rs:11288 | `Expected {} {}, but got {} instead.` |QUALIFIES — arity anchors at the arguments (they ARE the problem) |
| 79 | analyzer.rs:11336 | `Expected {}, but got {} instead.` |QUALIFIES — B2 (both sides rendered), value-anchored (A4) |
| 80 | analyzer.rs:11376 | `a tuple comprehension's source must be a mapped tuple, got {` |QUALIFIES — flat-storage family (tuple .0 pins, 12) |
| 81 | analyzer.rs:11454 | `Expected {}, but got {} instead.` |QUALIFIES — B2 (both sides rendered), value-anchored (A4) |
| 82 | analyzer.rs:11490 | `Expected {}, but got {} instead.` |QUALIFIES — B2 (both sides rendered), value-anchored (A4) |
| 83 | analyzer.rs:11554 | `Expected {}, but got {} instead.` |QUALIFIES — B2; reassignments carry the B3 initializer note; pins a_reassignment_mismatch…, an_annotated_variables… |
| 84 | analyzer.rs:11642 | ``?.` flattens into the chain's own `Result`, so the error ty` |QUALIFIES — B4 map_err/ok_or steers (§9 pins) |
| 85 | analyzer.rs:26890 (lift_opt_in_error) | `{operator} lifts an `Option`, a `Result`, or a type opting in with `impl .. with Lift`; this is {rendered}` |QUALIFIES — expression-lifting pins (15); re-keyed batch 8: the user-Lift arc widened the alternatives and unified the bare-`?`/`?.` sites behind `{operator}` (32561077, 6459cc06); dash 93d73f57; one emitter now serves rows 85+89 |
| 86 | analyzer.rs:11729 | `every `?` in one lifted expression must split the same ` |QUALIFIES — expression-lifting pins (15) |
| 87 | analyzer.rs:11755 | ``?` short-circuits a lifted expression with the first bad ` |QUALIFIES — expression-lifting pins (15) |
| 88 | analyzer.rs:11807 | `this lifted expression flattens into its own `Result`, so th` |QUALIFIES — expression-lifting pins (15) |
| 89 | analyzer.rs:26890 (lift_opt_in_error) | ``?.` lifts an `Option`, a `Result`, or a type opting in with` |QUALIFIES — expression-lifting pins (15); batch 8: same emitter as row 85 (`{operator}` = `?.`), key unchanged |
| 90 | analyzer.rs:26959 | ``?.` needs a container with an element type; this is {rendered}` |QUALIFIES — lift family (chain + region pins); re-keyed batch 8 (93d73f57); a bare-`?` sibling wording lives at analyzer.rs:26480 |
| 91 | analyzer.rs:26907 (lift_contract_member) | `{operator} on {rendered} needs {a\|an} `{member_name}` method: the Lift contract (`map<U>(self, \|T\| U)`, `and_then<U>(self, \|T\| Self-of-U)`)` |QUALIFIES — lift family (chain + region pins); re-keyed batch 8 (`{operator}` + the a/an chooser 6459cc06; dash→colon 93d73f57) |
| 92 | analyzer.rs:12011 | `a bare `ret` exits a closure whose body yields {tail_rendere` |QUALIFIES — ret-checking family (B10 pins) |
| 93 | analyzer.rs:12025 | `the closure's body ends without a value, but this `ret` retu` |QUALIFIES — ret-checking family (B10 pins) |
| 94 | analyzer.rs:12036 | `this `ret` returns {value_rendered}, but the closure's body ` |QUALIFIES — ret-checking family (B10 pins) |
| 95 | analyzer.rs:12114 | ``!` on an `Option` returns `None` early, so the enclosing fu` |QUALIFIES — try/lift operator family (B11 pins) |
| 96 | analyzer.rs:12150 | ``!` returns this `Result`'s error as-is, so the error types ` |QUALIFIES — try/lift operator family (B11 pins) |
| 97 | analyzer.rs:12160 | ``!` on a `Result` returns the error early, so the enclosing ` |QUALIFIES — try/lift operator family (B11 pins) |
| 98 | analyzer.rs:12199 | ``!` needs a value implementing `Try` (an `Option`, a `Result` |QUALIFIES — try/lift operator family (B11 pins) |
| 99 | analyzer.rs:12222 | `the `Try` impl is missing `verdict`/`from_bad`` |QUALIFIES — B6 names the Try contract (user-Try pins) |
| 100 | analyzer.rs:12258 | ``!` on a `Try` type returns `from_bad(..)`, which rebuilds {` |QUALIFIES — try/lift operator family (B11 pins) |
| 101 | analyzer.rs:12321 | `match guard must be a bool, but got {}` |QUALIFIES — the guard twin of B28 (existing check) |
| 102 | analyzer.rs:12373 | `match is not exhaustive: missing {}` |QUALIFIES — names the missing variants / the catch-all steer (match pins) |
| 103 | analyzer.rs:12385 | `match is not exhaustive: add a catch-all `_` leg` |QUALIFIES — names the missing variants / the catch-all steer (match pins) |
| 104 | analyzer.rs:12428 | `match legs have mismatched types: expected {}, but got {} in` |QUALIFIES — leg-body anchors (E7 pass-1 pins) |
| 105 | analyzer.rs:12479 | `unknown struct: {}` |QUALIFIES — B4 import steer (batch 7); pin an_unknown_struct_steers… |
| 106 | analyzer.rs:12495 | `cannot initialize a non-struct: {}` |QUALIFIES — B6 |
| 107 | analyzer.rs:12506 | `Expected {} {}, but got {} instead.` |QUALIFIES — arity anchors at the arguments (they ARE the problem) |
| 108 | analyzer.rs:12536 | `struct '{}' has no field '{}'` |QUALIFIES — field-anchored (E7 pins) |
| 109 | analyzer.rs:12564 | `Expected {}, but got {} instead.` |QUALIFIES — B2 (both sides rendered), value-anchored (A4) |
| 110 | analyzer.rs:12704 | `` |QUALIFIES — reviewed in the batch-7 sweep: B6-shaped rule statement from a designed arc; pin via its family's suite |
| 111 | analyzer.rs:12718 | `subject is not a struct: {}` |QUALIFIES — B2 renders the type |
| 112 | analyzer.rs:12770 | `struct '{}' has no field '{}'` |QUALIFIES — field-anchored (E7 pins) |
| 113 | analyzer.rs:12788 | `cannot access field '{}' on type {}` |QUALIFIES — B2 both sides; member-anchored |
| 114 | analyzer.rs:12853 | `cannot index this List: its element type is never determined` |QUALIFIES — B4 annotate steer (B16 pins) |
| 115 | analyzer.rs:12874 | `index {literal_index} is out of range for an array of length` |QUALIFIES — literal-OOB compile error (fixed-arrays pins) |
| 116 | analyzer.rs:12894 | `cannot index {} (only a `List` or `[T; n]` array is indexabl` |QUALIFIES — B6 names the indexable types |
| 117 | analyzer.rs:12992 | `cannot find '{}' in this scope` |QUALIFIES — B4 steer added (batch 1); pins: an_unknown_value_steers…, an_unknown_name_gets_no_bogus_steer, B.27 family |
| 118 | analyzer.rs:13014 | ``use` requires a namespace (a module or an enum)` |QUALIFIES — Origin-labeled reachability chains (platform-coloring pins) |
| 119 | analyzer.rs:13030 | `cannot find '{}' in the `use` path` |QUALIFIES — B4 steer added (batch 1); pins: an_unknown_value_steers…, an_unknown_name_gets_no_bogus_steer, B.27 family |
| 120 | analyzer.rs:13083 | `` |QUALIFIES — reviewed in the batch-7 sweep: B6-shaped rule statement from a designed arc; pin via its family's suite |
| 121 | analyzer.rs:13100 | `cannot find '{}' in this scope` |QUALIFIES — B4 steer added (batch 1); pins: an_unknown_value_steers…, an_unknown_name_gets_no_bogus_steer, B.27 family |
| 122 | analyzer.rs:13139 | `cannot assign to this expression` |QUALIFIES — place-model rule (assignment pins) |
| 123 | analyzer.rs:13205 | `` |QUALIFIES — reviewed in the batch-7 sweep: B6-shaped rule statement from a designed arc; pin via its family's suite |
| 124 | analyzer.rs:13230 | `cannot find '{}' in module '{}'` |QUALIFIES — B4 steer added (batch 1); pins: an_unknown_value_steers…, an_unknown_name_gets_no_bogus_steer, B.27 family |
| 125 | analyzer.rs:13251 | `cannot resolve `{member_name}` here: {subject_str} is not a ` |QUALIFIES — B2 renders the subject |
| 126 | analyzer.rs:13383 | `cannot find '{}' in {}{}` |QUALIFIES — B4 steer added (batch 1); pins: an_unknown_value_steers…, an_unknown_name_gets_no_bogus_steer, B.27 family |
| 127 | analyzer.rs:13405 | `cannot find '{}' in module '{}'` |QUALIFIES — B4 steer added (batch 1); pins: an_unknown_value_steers…, an_unknown_name_gets_no_bogus_steer, B.27 family |
| 128 | analyzer.rs:13422 | `cannot access '{}' on an unconstrained type parameter` |QUALIFIES — const-eval family (21 pins): capability/free-variable wording with reference spans |
| 129 | analyzer.rs:13466 | `no bound of this type parameter ({}) has a member '{}'` |QUALIFIES — B6 names the bound channel (B12 pins) |
| 130 | analyzer.rs:13487 | `cannot find trait '{}'` |QUALIFIES — B4 steer added (batch 1); pin: an_unknown_trait_steers… |
| 131 | analyzer.rs:13499 | `'{}' is not a trait` |QUALIFIES — B2 renders the subject |
| 132 | analyzer.rs:13567 | `'{}' does not implement trait '{}': missing '{}'` |QUALIFIES — impl-anchored; REFINEMENT TAKEN (notes finale): renders the signature to declare + a CROSS-SOURCE note at the trait's declaration; pin a_missing_trait_member… |
| 133 | analyzer.rs:13736 | `this {construct} is `{label}`, but a condition must be `bool` |QUALIFIES — B28 pins (6) |
| 134 | analyzer.rs:13815 | ``{symbol}` takes `bool` operands; the {side} operand is `{la` |QUALIFIES — B2 names side + type (B24 pins) |
| 135 | analyzer.rs:13831 | ``bool` has no ordering — `{symbol}` models `PartialOrd`, whi` |QUALIFIES — B6 + compare steer (B24 pins) |
| 136 | analyzer.rs:13855 | ``{symbol}` compares two values of the same type, but the ` |QUALIFIES — B24 wording (pins) |
| 137 | analyzer.rs:13973 | `type '{type_name}' does not implement the `{trait_name}` ope` |QUALIFIES — B6 operator steer (B24/B25 pins) |
| 138 | analyzer.rs:13991 | `cannot find context `{name}` in this scope` |QUALIFIES — context-pass pins |
| 139 | analyzer.rs:14006 | `duplicate context `{name}` in this clause` |QUALIFIES — coverage-fence family (ambient-owner pins); B6 names run/extent rules |
| 140 | analyzer.rs:14062 | `unknown numeric suffix `{suffix}`{hint}` |QUALIFIES — B4 rename hint (i53 rename pins) |
| 141 | analyzer.rs:14129 | `the literal `{whole}` is out of range for `{name}` ({range})` |QUALIFIES — range-checked literals, B2-shaped range text (numeric-types pins) |
| 142 | analyzer.rs:14143 | `type of struct initializer could not be resolved` |DEMOTE — same suppressed-residual family as 143–145/147 (was mis-verdicted QUALIFIES); guarded by `!self.diagnostics.is_empty()`, surfaces only as the lone signal. Pin one_unresolved_name_does_not_cascade_across_many_use_sites |
| 143 | analyzer.rs:14148 | `type of accessor subject could not be resolved` |DEMOTE — post-solve residual, guarded by `!self.diagnostics.is_empty()` (a symptom of an upstream failure, B5); surfaces only as the lone signal. Multi-use-site pin one_unresolved_name_does_not_cascade_across_many_use_sites |
| 144 | analyzer.rs:14153 | `type of variable '{}' could not be resolved` |DEMOTE — post-solve residual, guarded by `!self.diagnostics.is_empty()` (B5); surfaces only as the lone signal. Multi-use-site pin one_unresolved_name_does_not_cascade_across_many_use_sites |
| 145 | analyzer.rs:14167 | `type of function call arguments could not be resolved` |DEMOTE — post-solve residual, guarded by `!self.diagnostics.is_empty()` (B5); surfaces only as the lone signal. Multi-use-site pin one_unresolved_name_does_not_cascade_across_many_use_sites |
| 146 | analyzer.rs:14190 | `cannot index this List: its element type is never determined` |QUALIFIES — B4 annotate steer (B16 pins) |
| 147 | analyzer.rs:14210 | `type of match expression could not be resolved (subject: {})` |DEMOTE — post-solve residual, guarded (its own `residuals_are_cascade` gate, B5); surfaces only as the lone signal. Multi-use-site pin one_unresolved_name_does_not_cascade_across_many_use_sites |
| 148 | analyzer.rs:14284 | `the type of '{name}' is never fully determined: `{rendered}`` |QUALIFIES — B4 annotate steer (Map-sweep pins) |
| 149 | analyzer.rs:15270 | `` |QUALIFIES — reviewed in the batch-7 sweep: B6-shaped rule statement from a designed arc; pin via its family's suite |
| 150 | analyzer.rs:16224 | ``{importer}` imports `pkg::{module}`, but `{module}` is not ` |QUALIFIES — L1/E.10 module-shape rules (module_resolution pins) |
| 151 | analyzer.rs:16633 | `library at `{}` has no `lib.vl`` |QUALIFIES — L1 surface checks (workspace pins) |
| 152 | analyzer.rs:16674 | `library `{library_name}`'s base `lib.vl` re-exports `{module` |QUALIFIES — H2 body-export rule (scoped-import pins) |
| 153 | analyzer.rs:16776 | `module `{name}` is ambiguous: both `{name}.vl` and `{name}/l` |QUALIFIES — B6 names both candidates (module pins) |
| 154 | analyzer.rs:17062 | `` |QUALIFIES — reviewed in the batch-7 sweep: B6-shaped rule statement from a designed arc; pin via its family's suite |
| 155 | async_infer.rs:188 | ``{}` requires a synchronous closure (`sync`): its completion` |QUALIFIES — the direct sync-parameter divergence (async-polymorphism A.2), anchored at the parameter; head recorded batch 8 (was empty); tail reworded 93d73f57 |
| 156 | async_infer.rs:247 | ``{}` is a host (`external`) function: it cannot await a Vila` |QUALIFIES — the direct host-boundary divergence (async-polymorphism A.4), anchored at the argument; head recorded batch 8 (was empty; dash→colon 93d73f57). E68 (batch 8): no longer fires for the `Context::run` intrinsic — a surviving `run` call means `thread_contexts` already refused and reported (rows 216/217/222 carry the primaries); pins e68_an_uncovered_effect_reports_only_the_coverage_primary, e68_a_refused_run_shape_reports_only_the_context_primaries |
| 157 | macros.rs:397 | `a `macro { .. }` block cannot appear inside macro code: the ` |QUALIFIES — re-keyed batch 8 (93d73f57) — expansion diagnostics site-anchored w/ output previews (macro-engine pins); analyzer errors INSIDE generated code re-anchor at the attribute (batch 5 redirect; pin a_diagnostic_in_generated_code…) |
| 158 | macros.rs:414 | `the `macro_std` package was not found beside `std`: macros n` |QUALIFIES — re-keyed batch 8 (93d73f57); a second, tail-less variant lives at macros.rs:278 — expansion diagnostics site-anchored w/ output previews (macro-engine pins); analyzer errors INSIDE generated code re-anchor at the attribute (batch 5 redirect; pin a_diagnostic_in_generated_code…) |
| 159 | macros.rs:421 | `a macro named `{name}` is already defined in this module` |QUALIFIES — expansion diagnostics site-anchored w/ output previews (macro-engine pins); analyzer errors INSIDE generated code re-anchor at the attribute (batch 5 redirect; pin a_diagnostic_in_generated_code…) |
| 160 | macros.rs:523 | `a macro body may import only from `macro_std`: the macro wor` |QUALIFIES — re-keyed batch 8 (93d73f57) — expansion diagnostics site-anchored w/ output previews (macro-engine pins); analyzer errors INSIDE generated code re-anchor at the attribute (batch 5 redirect; pin a_diagnostic_in_generated_code…) |
| 161 | macros.rs:876 | ``[service]` expanded before std::rpc's `service` macro was ` |QUALIFIES — expansion diagnostics site-anchored w/ output previews (macro-engine pins); analyzer errors INSIDE generated code re-anchor at the attribute (batch 5 redirect; pin a_diagnostic_in_generated_code…) |
| 162 | macros.rs:957 | `this `macro { .. }` block was not registered — see the file'` |QUALIFIES — expansion diagnostics site-anchored w/ output previews (macro-engine pins); analyzer errors INSIDE generated code re-anchor at the attribute (batch 5 redirect; pin a_diagnostic_in_generated_code…) |
| 163 | macros.rs:1054 | `the built-in derive generators produced invalid Vilan ({mess` |QUALIFIES — expansion diagnostics site-anchored w/ output previews (macro-engine pins); analyzer errors INSIDE generated code re-anchor at the attribute (batch 5 redirect; pin a_diagnostic_in_generated_code…) |
| 164 | macros.rs:1073 | `no macro named `{name}` is in scope` |QUALIFIES — expansion diagnostics site-anchored w/ output previews (macro-engine pins); analyzer errors INSIDE generated code re-anchor at the attribute (batch 5 redirect; pin a_diagnostic_in_generated_code…) |
| 165 | macros.rs:1082 | ``{name}` is a macro HELPER (its signature is not a macro sha` |QUALIFIES — expansion diagnostics site-anchored w/ output previews (macro-engine pins); analyzer errors INSIDE generated code re-anchor at the attribute (batch 5 redirect; pin a_diagnostic_in_generated_code…) |
| 166 | macros.rs:1337 | `macro `{name}` is invocation-shaped (it takes no `Item`); ca` |QUALIFIES — re-keyed batch 8 (93d73f57) — expansion diagnostics site-anchored w/ output previews (macro-engine pins); analyzer errors INSIDE generated code re-anchor at the attribute (batch 5 redirect; pin a_diagnostic_in_generated_code…) |
| 167 | macros.rs:1133 | `no macro named `{name}` is in scope` |QUALIFIES — expansion diagnostics site-anchored w/ output previews (macro-engine pins); analyzer errors INSIDE generated code re-anchor at the attribute (batch 5 redirect; pin a_diagnostic_in_generated_code…) |
| 168 | macros.rs:1145 | ``{name}` is a macro HELPER (its signature is not a macro sha` |QUALIFIES — expansion diagnostics site-anchored w/ output previews (macro-engine pins); analyzer errors INSIDE generated code re-anchor at the attribute (batch 5 redirect; pin a_diagnostic_in_generated_code…) |
| 169 | macros.rs:1401 | `macro `{name}` is attribute-shaped (it takes an `Item`); use` |QUALIFIES — re-keyed batch 8 (93d73f57) — expansion diagnostics site-anchored w/ output previews (macro-engine pins); analyzer errors INSIDE generated code re-anchor at the attribute (batch 5 redirect; pin a_diagnostic_in_generated_code…) |
| 170 | macros.rs:1211 | `macro expansion did not settle after {cap} rounds — the chai` |QUALIFIES — expansion diagnostics site-anchored w/ output previews (macro-engine pins); analyzer errors INSIDE generated code re-anchor at the attribute (batch 5 redirect; pin a_diagnostic_in_generated_code…) |
| 171 | macros.rs:1235 | `{label}'s definition did not compile` |QUALIFIES — expansion diagnostics site-anchored w/ output previews (macro-engine pins); analyzer errors INSIDE generated code re-anchor at the attribute (batch 5 redirect; pin a_diagnostic_in_generated_code…) |
| 172 | macros.rs:1257 | `{label} failed at expansion time: {message}` |QUALIFIES — expansion diagnostics site-anchored w/ output previews (macro-engine pins); analyzer errors INSIDE generated code re-anchor at the attribute (batch 5 redirect; pin a_diagnostic_in_generated_code…) |
| 173 | macros.rs:1284 | `{label} generated invalid Vilan ({message}) — the ` |QUALIFIES — expansion diagnostics site-anchored w/ output previews (macro-engine pins); analyzer errors INSIDE generated code re-anchor at the attribute (batch 5 redirect; pin a_diagnostic_in_generated_code…) |
| 174 | macros.rs:1298 | `{label} must generate a single expression here (it is ` |QUALIFIES — expansion diagnostics site-anchored w/ output previews (macro-engine pins); analyzer errors INSIDE generated code re-anchor at the attribute (batch 5 redirect; pin a_diagnostic_in_generated_code…) |
| 175 | macros.rs:1311 | `{label} generated a `macro {{ .. }}` block — macros cannot ` |QUALIFIES — expansion diagnostics site-anchored w/ output previews (macro-engine pins); analyzer errors INSIDE generated code re-anchor at the attribute (batch 5 redirect; pin a_diagnostic_in_generated_code…) |
| 176 | macros.rs:1333 | `{label} generated invalid Vilan ({message}) — the ` |QUALIFIES — expansion diagnostics site-anchored w/ output previews (macro-engine pins); analyzer errors INSIDE generated code re-anchor at the attribute (batch 5 redirect; pin a_diagnostic_in_generated_code…) |
| 177 | macros.rs:1589 | `{label} generated a `macro fun`: macros cannot define macros` |QUALIFIES — re-keyed batch 8 (93d73f57) — expansion diagnostics site-anchored w/ output previews (macro-engine pins); analyzer errors INSIDE generated code re-anchor at the attribute (batch 5 redirect; pin a_diagnostic_in_generated_code…) |
| 178 | macros.rs:1358 | `{label} generated a `macro {{ .. }}` block — macros cannot ` |QUALIFIES — expansion diagnostics site-anchored w/ output previews (macro-engine pins); analyzer errors INSIDE generated code re-anchor at the attribute (batch 5 redirect; pin a_diagnostic_in_generated_code…) |
| 179 | platform_color.rs:110 | `unknown platform pattern `{pattern_text}` in `[platform(…)]`` |QUALIFIES — B6 lists the accepted forms; pattern-anchored ([platform] fence pins) |
| 180 | platform_color.rs:232 | `` |QUALIFIES — reviewed in the batch-7 sweep: B6-shaped rule statement from a designed arc; pin via its family's suite |
| 181 | analyzer.rs:2641 | `{label} of `[derive(Wire)]` type `{type_name}` is the resourc` |QUALIFIES — C4 resource-not-Wire variant (B2 renders the resource type, B4 plain-data-handle steer); pin derive_wire_rejects_a_resource_field |
| 182 | analyzer.rs:2729 | `{label} of `[derive(Hashable)]` type `{type_name}` is the res` |QUALIFIES — C4 resource variant; pin derive_hashable_rejects_a_resource_field |
| 183 | analyzer.rs:2767 | `{label} of `[derive(PartialEq)]` type `{type_name}` is the re` |QUALIFIES — C4 resource variant; pin derive_partialeq_rejects_a_resource_field |
| 184 | analyzer.rs:4501 | ``{rendered}` implements `Drop` but is not a resource: destru` |QUALIFIES — re-keyed batch 8 (93d73f57) — C4 §3/§11 double-close rule (B6 + declare-`resource` steer); pins `declare it a `resource``, `is not a resource` |
| 185 | analyzer.rs:2928 | ``{}`'s `{}` declares {} type parameter(s), but `{}` declares ` |QUALIFIES — B29 conformance (B2 both counts + match steer; conformance_note = declaration control); B29 pins |
| 186 | analyzer.rs:2988 | ``{}`'s `{}` takes {} parameter(s), but `{}` declares {} — matc` |QUALIFIES — B29 arity (B2 + match-the-list steer); B29 pins |
| 187 | analyzer.rs:3015 | ``{}`'s `{}` takes no receiver / a `{}` receiver, but `{}` decl` |QUALIFIES — B29 receiver presence (B6 + give-the-receiver steer); B29 pins |
| 188 | analyzer.rs:3065 | ``{}`'s `{}` receives `{}` / parameter {} is {}, but `{}` decl` |QUALIFIES — B29 receiver/param convention; pin `match the receiver convention` |
| 189 | analyzer.rs:3089 | `parameter {position} of `{}`'s `{}` is `{actual_label}`, but ` |QUALIFIES — B29 param type (B2 both sides, anchored at the parameter — A1 verified); pin `match the declared type` |
| 190 | analyzer.rs:3132 | ``{}`'s `{}` returns `{actual_label}`, but `{}` declares `{exp` |QUALIFIES — B29 return type (B2 + match-the-return steer); pin `match the declared return type` |
| 191 | analyzer.rs:4258 | ``{container_name}` cannot hold the resource `{rendered}`{reach` |QUALIFIES — C4 native-container-resource (B6 + `Option`/struct-field steer); A19 widened it per-instantiation, so it also names the member path a resource took (B3) and carries one cross-file note at the member holding the container (C3, primary stays in user code per A2); pins `cannot hold the resource`, `reached through` |
| 192 | analyzer.rs:4018 | `the resource `{rendered}` cannot be used where `any` is expec` |QUALIFIES — R12 resource-to-`any` (B2 + debug-print steer); r12_rejects_* pins |
| 193 | analyzer.rs:7049 (report_hmr_transfer) | ``{rendered}` cannot cross a hot swap: …` / ``{rendered}` is a generic type parameter here: …` |QUALIFIES — re-keyed batch 8 (93d73f57 turned both dashes into colons) — A13 HMR transfer (B6 + stash-plain-data steer, same-file `only plain data transfers` note); `cannot cross a hot swap` pins |
| 194 | analyzer.rs:6456 | `use of `{name}` after it was moved` (+7 affine arms) |QUALIFIES — C4 affine-rule family, 8 arms one push (B6 + loan/`Option`+take steers; UseAfterMove carries a same-file `moved here` note); pins `after it was moved`, `no partial moves`, `moved on one path`, `declared outside this loop`, `cannot capture the resource`, `module-level resource`, `out of this pattern`. **B65 added the 8th arm** (`LoanedCaptureConsumed`): the capture-position twin of `LoanConsumed`, and deliberately NOT its text — a capture carries no convention, so `declare it own x` is not the fix; the steer names the subject (`match o` by value) and `Option` + `take`. It offers no copy: vilan has no user-facing copy spelling and R1 forbids copying a resource, so "clone it" would be a speculative steer (B4). Pins `b65_the_is_capture_diagnostic_names_the_subject_and_the_by_value_steer`, `b65_the_loaned_match_capture_diagnostic_steers_to_the_by_value_match`, `b65_a_loaned_destructure_capture_consumed_is_rejected` |
| 195 | analyzer.rs:6639 | ``{name}` is not move-clean when instantiated with a resource ` |QUALIFIES — R11 generic-leak; primary at the instantiation (A2), cross-file note into the generic body; `not move-clean` pins. **B66 widened it from `own` parameters to every value the body would have to destroy** (`plan_scope`'s `dropped` set — captures and locals), giving two summaries off one emitter (`GenericLeak::OwnParameter` / `ScopeEndDrop`) so the family reads as one rule. The widening is gated on the body being move-clean (B5 — a leftover ownership after a reported move violation is a consequence, not a second root cause; pin `b66_a_body_that_already_failed_the_move_scan_reports_once`), and the report order is span-sorted (C1, since `dropped` is a HashSet). The E11 finding that std has no `own`-generic leaks is now FALSE for the widened rule: `map`/`and_then`/`is_some_and`/… leak at a resource instantiation and are reported cross-file. Pins `b66_the_generic_capture_leak_names_the_capture_at_the_instantiation`, `b66_a_generic_local_that_would_drop_at_its_scope_end_is_rejected`, `r11_std_option_map_at_a_resource_rejects` |
| 196 | analyzer.rs:6727 | ``{name}` is not move-clean … pass a resource to `drop<T>`` |QUALIFIES — R11 drop-forward; `not move-clean` pins |
| 197 | analyzer.rs:7290 | ``{name}` is not move-clean when instantiated with a resource ` |QUALIFIES — R11 move-violation family (per-violation summary + steer); `not move-clean` pins. B65 added the `LoanedCaptureConsumed` summary (`a capture of a loaned resource-typed subject is moved out`) — it rides the existing per-instantiation scan with no new plumbing, primary at the instantiation (A2), note into the generic body. Pin `b65_a_loaned_capture_consumed_inside_a_generic_reports_at_the_instantiation` |
| 198 | analyzer.rs:14193 | `a `sync` closure contract is only supported on parameters` |QUALIFIES — B6 marker position (async-polymorphism A.2); pin `a `sync` closure contract is only supported on parameters` |
| 199 | analyzer.rs:24914 | ``drop` for `{subject}` is async — teardown must be synchronou` |QUALIFIES — C4 §5 sync-teardown (B6 + OwnedNursery steer); pin `teardown must be synchronous` |
| 200 | analyzer.rs:24940 | ``drop` for `{subject}` requires an ambient context — teardown` |QUALIFIES — C4 context-free teardown (B6 + hand-work-to-owner steer); pin `teardown must be context-free` |
| 201 | async_infer.rs:1208 | `this call passes an async closure that reaches `{parameter}`,` |QUALIFIES — async-polymorphism transitive sync (B6 + move-async-outside steer; cross-file note at the forwarding site is user↔user — std takes sync closures directly, so its violations are DIRECT, reported at the global check); pin `forwarded into the `sync` parameter` |
| 202 | async_infer.rs:1544 | `this call passes an async closure that reaches the host (`ext` |QUALIFIES — async-polymorphism transitive extern (B6; forwarding note user↔user as above); pin `cannot await a Vilan closure`. E68 (batch 8): skips the `Context::run` intrinsic like row 156; pin e68_a_generic_forward_into_run_does_not_cascade_transitively |
| 203 | async_infer.rs:1329 | `an async closure cannot adapt a trait/generic-dispatched call` |QUALIFIES — async-polymorphism dispatch refusal (B6 + bind-concretely / declare-the-param-async steer, no note); pin `cannot adapt a trait/generic-dispatched call` |
| 204 | parsing.rs:789 (emit_failure) | `found {found} expected {expected} in {context}; {hint}` |QUALIFIES — the handwritten frontend's structured parse error (frontend.md S4/S5; diagnostics-standard §4). Farthest-failure anchored (A1); curated expectations only. Pins: parsing.rs render_carries_the_production_context, render_surfaces_the_real_error_from_inside_a_recovered_block, render_steers_a_missing_parameter_comma; inference.rs an_unclosed_generic_steers…, a_missing_parameter_comma_steers…. Batch 8: the hint suffix is `; {hint}` and the ` in {label}` suffix appends once per context entry (93d73f57 + S1/S2); E67 added the curated expectation `a method name` (parsing.rs:2313, an element head's `.` with no member) — pinned structurally (parser_recovery.rs recovers_an_unfinished_chain_link_keeping_the_element) but its rendered text asserted nowhere, flagged |
| 205 | parsing.rs recover_delimited (Unbalanced fallback) | `unclosed `{delimiter}` in {production}: expected a matching `{close}`` |QUALIFIES — re-keyed batch 8 (93d73f57) (with a noted imprecision) — the LAST-RESORT fallback for a GENUINELY GARBLED region whose content declines at its first token, recording no committed inner failure (`struct S { 1 2 3 }`, `fun f<1 2 3>`); it names the production + opener. FIX 1: whenever a committed demand OR leaf recorded a failure inside the (closed) region — a missing separator/closer or a typo — that located error is surfaced instead, so "unclosed" never fires on those. The fallback's "unclosed" wording is imprecise for a region that did close; kept as the production-namer. Pins: parsing.rs render_names_the_unclosed_delimiter_and_its_production, the_ten_delimiter_sites_recover_to_their_exact_placeholders |
| 206 | parsing.rs:4298 (parse_misplaced_resource) | `` `resource` is a type-declaration modifier: it may appear only before a `struct` or `enum` declaration `` |QUALIFIES — re-keyed batch 8 (93d73f57); asserted verbatim in parsing.rs:5569 — B6 rule statement (the prohibition explains itself + names the sanctioned position). Pin: parsing.rs render_states_the_resource_language_rule |
| 207 | parsing.rs:221 (parse, lex error) | `found '{character}' expected a token` |QUALIFIES — the S1 LexError surfaced as a found/expected error, span-anchored at the un-lexable char (mid-file → the rest still parses). Pin: parsing.rs render_reports_an_illegal_character |
| 208 | parsing.rs:663 (soup_hint) | `if this was postfix `!` before a comparison, the space is required: `a! == b`` |QUALIFIES — §6a first-class hint, recognized STRUCTURALLY (a stray `=` after a `!=` token), not by source-string match; fires from inside blocks (FIX 1). Pins: parsing.rs render_gives_the_not_equals_soup_a_first_class_message, render_surfaces_the_real_error_from_inside_a_recovered_block; inference.rs the_not_equals_soup_hints_the_postfix_bang_spacing |
| 209 | init_order.rs:134 (check_cycles) | `` `{members}` form an initialization cycle: module-level bindings initialize in dependency order, and a cycle has no such order`` / `` `{name}`'s initializer evaluates `{name}` itself, which has not initialized yet`` |QUALIFIES — B33 S2 (b33-emission-order.md §3, ratified (b)). A1 anchored at the READ that closes the cycle (not the `let`); A3 rooted at the canonically first member; B5 one diagnostic per cycle, never per member, downstream readers unnamed, and the whole check is suppressed behind any analysis error; B6 states the rule plus the closure escape hatch; C1 components sorted by canonical key, chain = shortest round trip; C3 note at the other member's declaration, cross-source aware, dropped when it would repeat the primary span. The §5(b) dispatch over-approximation states itself in the message. Pins: inference.rs the 16 `*cycle*` tests, init_order.rs 7 chain/participant unit tests, document.rs an_initialization_cycle_publishes_to_the_file_that_closes_it |
| 210 | lexing.rs LINE_BREAK_IN_STRING (surfaced at parsing.rs as ParseErrorReason::Rule) | `a string cannot span lines unless it is triple-quoted: close it before the line break; multi-line text goes in `"""…"""` (`i"""…"""` when interpolating), and a single line break is written `\n`` |QUALIFIES — the H7 disallow-revisit (ratified 2026-07-27): a raw line break, or a `\` before one, inside `"…"` / `i"…"`. B6 states the rule and names BOTH sanctioned spellings; one message deliberately covers the two shapes that reach it (a deliberate multi-line literal and a forgotten closing quote — the lexer cannot tell them apart; "close it" fixes one, `"""` the other). A1 anchors at the literal's OPENING character, not the break (the break renders as a caret past the last visible character; on CRLF it lands on an invisible `\r`). B5: a `"…"` inside a hole reports nothing — the enclosing i-string states the rule once. Salvage: the scanned body becomes the token and lexing resumes AT the break, so code below still analyzes (verified live over LSP). Pins: the lexing.rs a_line_break_*/a_backslash_cannot_escape_* family, inference.rs an_unterminated_string_is_reported_on_its_own_line + code_below_a_line_break_error_still_analyzes + the rewritten S2 family, formatter.rs a_line_break_in_a_single_quoted_string_bails_the_formatter. Re-keyed batch 8 (93d73f57; lexing.rs:876) |
| 211 | lexing.rs UNESCAPED_BRACE (i-triple, surfaced as ParseErrorReason::Rule) | `a literal `}` inside an interpolated string is written `\}`: an unescaped brace belongs to a `{expr}` hole` |QUALIFIES — H7's i-triple form (grind 6; the row was owed and is recorded here): B6 names the sanctioned spelling; catches the forgotten hole-close without abandoning a multi-line literal (the offender is reported and read as text). Pin: inference.rs an_unescaped_closing_brace_names_the_escape_that_was_meant. Re-keyed batch 8 (93d73f57; lexing.rs:867) |
| 212 | analyzer.rs (`MethodLookup::NoMethod`, row 74's site) | `{} has no method '{}'{trait_only}{field}{import}` — the fourth, IMPORT steer: `; import std::{module}::{name} to use it (\`import std::{module}::{name};\`)` |QUALIFIES — std-surface.md §5 (I4's headline complaint: `42.to_string()` reads as a missing feature when it is an unloaded `std::display`). B1 user vocabulary only (trait/module/method are all user spellings); B4 one unambiguous action, code-shaped, deliberately verbatim `import_steer_inner`'s own `(\`import …;\`)` shape so the two read as one family; A1/A2 unchanged — the span stays row 74's method-name span in the USER's file, and the hint names std in prose without a secondary span into std source (no C3 note owed). Fires only when an UNLOADED std module's `impl` block declares that method on that subject HEAD; a loaded module's bounded impl reports the bound instead and never reaches this arm (pinned). Pins: a_missing_to_string_steers_to_the_display_import, the_to_string_steer_covers_every_display_impl_subject, the_join_miss_steers_to_the_display_import, and the negatives a_method_no_std_trait_provides_carries_no_import_steer, the_import_steer_does_not_fire_for_a_user_type, the_import_steer_does_not_survive_the_import, an_unsatisfied_bound_is_reported_as_a_bound_not_as_a_steered_miss |
| 213 | analyzer.rs (`MethodLookup::AmbiguousTraitArguments`) | `'{member}' is ambiguous on '{type}': {both }{homes} provide it, and '{Trait}::{member}' names only the trait, not which of its instantiations; annotate the type this call must produce to pick one` |QUALIFIES — B73's R1 (method-resolution.md §13.5), the arm that turns §13.2 row 2's silent wrong answer into a report. A1/A4 anchors at the METHOD NAME, the same span the two-trait ambiguity (row 74's neighbour) uses. B1 names each home as THIS receiver instantiates it — `Into<Foo>` for std's blanket reached through a `Foo`, `Into<str>` for the user's impl — because `Into` twice identifies nothing. B4 steers to the expected type rather than to §3.1's `Trait::member`, which has no argument slot and so cannot pick between two instantiations of one trait (B83: an impossible steer is worse than no steer); the message says so outright rather than leaving the reader to discover it. C1 the homes render in declaration order. No C3 note: both impl sites are the user's own and the two rendered homes already name them. Pins: b73_an_unannotated_into_call_is_ambiguous_rather_than_silently_identity, b73_the_argument_ambiguity_names_both_homes_as_the_receiver_instantiates_them |
| 214 | analyzer.rs (`MethodLookup::AmbiguousImpls`) | `'{member}' is ambiguous on '{type}': {both }{subjects} provide it and neither impl subject is more specific than the other; vilan picks the more specific of two overlapping impls, so narrow one subject until it is` |QUALIFIES — B73's R3 residue (method-resolution.md §13.4(a)(3)), reported at the CALL SITE per the §13.6 Q4 ruling. A1/A4 anchors at the method name, the same span the other two ambiguities use. B1 names each candidate by its impl subject as the program writes it, WITH the binder's bound (`Box<T> where T: Display`) — without the bound the two subjects of the only shape that reaches this arm render identically. B4/B6: there is deliberately no steer to a call-site spelling, because none exists — both candidates are one trait at one instantiation, so §3.1's `Trait::member` cannot separate them (B83's "an impossible steer is worse than no steer"); the message states the rule that failed to apply and sends the reader to the definitions, which is the only place the fix lives. C1 the maxima render in declaration order. Fires only for maxima the subsumption order genuinely does not rank — impls at the SAME point in the order (identical shapes, identical bounds: §2's platform-twin shape) keep declaration order and are not reported. Pin: b73_two_impls_bounded_by_unrelated_traits_are_an_unrankable_overlap |
| 215 | context.rs:276 | `` `{method}` must be called on a context bound to a name `` |B6 rule statement (the intrinsic requires a named context binding), anchored at the `get`/`get_safe` call; NO PIN found — C2 unmet, flagged batch 8 |
| 216 | context.rs:313 | `` `run` must be called on a named context with a closure literal body `` |B6; the short arm (no context/value/body extracted at all). NO PIN exercises this arm — the e68 pins assert the shared fragment but only through row 217's arm; C2 unmet, flagged batch 8 |
| 217 | context.rs:322 | `` `run` must be called on a named context with a closure literal body, or a closure value whose type is `context`-annotated with exactly this context `` |QUALIFIES — B6 names both sanctioned spellings (ambient-owner.md §5); anchored at the `run` call. Pins (new this batch — the message was unpinned before): e68_a_refused_run_shape_reports_only_the_context_primaries, e68_a_generic_forward_into_run_does_not_cascade_transitively |
| 218 | context.rs:808 | `this parameter's `context` clause names a value that is not a context` |QUALIFIES — B6, anchored at the parameter; pin a_clause_naming_a_non_context_errors |
| 219 | context.rs:844 | `a `context`-typed binding takes a closure literal, or a value with the same `context` clause` |B6 rule statement (the ui-boundary clause-binding rule); NO PIN found — C2 unmet, flagged batch 8 |
| 220 | context.rs:892 | `a `context`-typed parameter takes a closure literal, a value with the same `context` clause, or a local closure binding (which adopts the clause)` |B6 rule statement, anchored at the offending argument; NO PIN found — C2 unmet, flagged batch 8 |
| 221 | context.rs:938 | `an injected (`context`-typed) closure can only be called, forwarded to a parameter with the same `context` clause, or passed to `run`` |QUALIFIES — B6 names every sanctioned use; pin an_injected_closure_cannot_escape asserts message AND span == the use site (an A1 pin) |
| 222 | context.rs:1164 | `context `{}` is read here, but this code can be reached without an enclosing `run`` |RE-ANCHOR — the coverage fence (A25's complaint). A1/A3 sound: anchors at each unbound STRICT `get`. But when the read is reached through a std helper the `get` sits in STD — `effect`/`map`/`or` all anchor at reactive.vl:365 (`get_owner`'s body), verified live batch 8 — failing A2 for a user-caused condition; the fix is to walk back to the user-written call with the std frame as a C3 note. Pins: effect_outside_an_owner_scope_is_a_compile_error, a25_map_outside_an_owner_scope_is_a_compile_error, a25_or_outside_an_owner_scope_is_a_compile_error, e68_an_uncovered_effect_reports_only_the_coverage_primary. E68 (batch 8): this primary no longer drags rows 156/202's host-await secondaries — fixed in this batch's change-set |
| 223 | context.rs:1174 | `an injected closure is called here, but this code can be reached without an enclosing `run` for context `{}`` |QUALIFIES — the injected-call flavor of row 222 (calling an injected closure IS a read); pin an_uncovered_injected_call_is_a_compile_error. Same A2 exposure as row 222 when the call site sits in std — noted, unprobed |
| 224 | context.rs:1196 | `` `{}` reads context `{}`, so it can't be used as a value `` |QUALIFIES — B6 refuse-rather-than-miscompile (indirect calls would bypass the threaded parameter), anchored at the value use (A1); pin a_context_reading_function_still_cannot_be_a_value |
| 225 | context.rs:1436 | `` `get_safe` needs `std::option::Option` loaded `` |Recorded batch 8 — a broken-toolchain condition, not user code: span 0..0 at SourceId(0) by construction (A-rules vacuous), no pin practicable (needs a std stripped of option.vl). Left as the last-resort message it is |
| 226 | analyzer.rs:29047 | `'{member_name}' is ambiguous on '{subject_str}': {both }{providers} provide it as a static, and a static has no receiver for a `Trait::{member_name}` path to select through; declare '{subject_str}''s own '{member_name}', which outranks every trait-provided one` |QUALIFIES — B83's static-path ambiguity, the FOURTH "is ambiguous on" head, missed when rows 212–214 were filed; B73's R2 changed which programs reach it (static_path_candidates), text untouched. Pin inference.rs:52954 (verbatim). A4 note: anchors at the whole path expr (span_map) where its three siblings anchor at the member name — narrowing available, and under beta §3.1 anchors may only narrow |
| 227 | parsing.rs:213 (render, MissingTerminator) | `expected `;` to end this statement` |QUALIFIES — the S1/S2 statement/item synchronizer's terminator-gap form (f4a39cb3, 2026-08-11; editing-dx.md), unrowed until batch 8; B6 names the fix; pinned in parser_recovery.rs + inference.rs (`to end this statement`) |
| 228 | parsing.rs:214 (render, Unclosed) | `unclosed `{delimiter}`: expected a matching `{close}`` |QUALIFIES — the production-less sibling of row 205 (same S1/S2 arc), unrowed until batch 8; pinned in parser_recovery.rs + inference.rs (`expected a matching`) |
| 229 | parsing.rs ParseErrorReason::Rule (nine sites: 2170, 3377, 3510, 3529, 3543, 3556, 3595, 3615, 3638) | curated rule statements: the spread-value spelling, `external fun` body/`mut`, `mut` vs `own`/views, spread vs `own`/views, the spread pack type, the spread plain name, `mut` plain name, spread-last, and one caller-supplied |QUALIFIES — the `Rule` class row 206 founded has grown to ten curated B6 statements (the parameter-marker arcs); each states its rule and names the sanctioned spelling; all nine verified pinned in inference.rs batch 8 (the external-`mut` one at inference.rs:2560, the rest by their own fragments). parsing.rs:99's "the one case today" doc comment is stale — flagged |
| 230 | std process/build.vl:88 (BuildError::NotBuilt) | `no build manifest at {path} — build the leg first (`vilan build .`), and run the server from the project root` |QUALIFIES — RUNTIME refusal (no span; A-rules n/a): names what was looked for and the producing command (B4/B6). Pin: vilan-cli build_manifest.rs build_of_on_a_leg_that_was_never_built_is_a_named_error (fragments: the path + `vilan build`; the head's own words unasserted — noted) |
| 231 | std process/build.vl:89 (BuildError::Unreadable) | `{path} is not a build manifest this toolchain wrote — rebuild the leg (`vilan build .`)` |RUNTIME refusal; B6 + rebuild steer met — but NO PIN anywhere (no test drives the corrupt-manifest arm): C2 unmet, flagged batch 8 |
| 232 | std process/build.vl:187 (load_build) | `the `{build.leg}` build names {file}, which is not on disk — rebuild the leg (`vilan build .`), and run the server from the project root` |QUALIFIES — RUNTIME boot refusal (refusing to start beats 404ing every asset, fullstack-dx.md §5.4/§10.7). Pin: vilan-cli serve_build.rs an_artifact_the_build_named_and_did_not_write_stops_the_server (behavioral + fragments). require_build/require_shell's own panic path is driven by no test end-to-end — flagged |
| 233 | std process/document.vl:86 (ShellFault::StylesNotLinked) | `the build emitted the stylesheet {file} and this document links no stylesheet — add `<link rel="stylesheet" href="/{file}" />` inside <head>` |QUALIFIES — RUNTIME shell-check refusal (E56 S4); B2-shaped (what the build emitted vs what the document links) + code-shaped B4 steer. Pins: shell_check.rs the_check_reads_the_markup_rather_than_searching_it (verbatim head), a_scaffolded_project_that_loses_its_stylesheet_link_refuses_to_boot |
| 234 | std process/document.vl:87 (ShellFault::LinkedStyleMissing) | `this document links {href}, which this build did not emit — remove the link, or restore the styles it names (a leg with no `const style()` emits no stylesheet at all)` |QUALIFIES — B2 + two-way steer + the rule's why. Pins: shell_check.rs a_link_to_a_stylesheet_the_build_did_not_emit_refuses_to_boot; document.rs every_document_of_can_produce_passes_check_shell (verbatim prefix) |
| 235 | std process/document.vl:88 (ShellFault::ScriptNotEmitted) | `this document loads {src}, which this build did not emit — remove the script tag, or restore the build that wrote it` |QUALIFIES — B2 + steer. Pin: shell_check.rs a_script_the_build_did_not_emit_refuses_to_boot (the shared `did not emit` needle does not discriminate this from row 234 — the payload does; noted) |
| 236 | std process/document.vl:89 (ShellFault::BundleNotLoaded) | `this build's bundle {file} is loaded by no script in this document — add `<script src="/{file}"></script>` before </body>` |QUALIFIES — B2 + code-shaped steer. Pin: shell_check.rs a_shell_that_loads_no_bundle_refuses_to_boot |
| 237 | std process/document.vl:90 (ShellFault::MountMissing) | `no element in this document carries id="{id}", which is where the client mounts — add `<div id="{id}"></div>` inside <body>` |QUALIFIES — B6 + code-shaped steer. Pins: shell_check.rs a_renamed_mount_element_refuses_to_boot, the_check_reads_the_markup_rather_than_searching_it (verbatim head). Its runtime twin `mount: no element with id '{id}'` (browser/ui.vl:679) is pinned in mount_missing_id.rs |
| 238 | std process/document.vl:91 (ShellFault::ModuleScriptWithChunks) | `{file} is loaded as a module script and this leg SPLITS — chunk resolution reads `document.currentScript`, which is null in a module, so every nested route would fail to load its chunk; drop `type="module"` from its script tag` |QUALIFIES — B6 states the mechanism and the one-action fix. Pin: shell_check.rs a_module_script_over_a_splitting_leg_refuses_to_boot |
| 239 | std process/document.vl:421 (fault_report) | `{path} does not match the `{build.leg}` build:` + one `\n  - ` bullet per fault |QUALIFIES — the require_shell envelope: B5-shaped (every fault reported once, under one header). Pins: shell_check.rs a_shell_with_two_faults_reports_both (the bullet joiner, exactly), assert_refused (behavior + the {path} slot); the header's own words asserted nowhere — noted |
