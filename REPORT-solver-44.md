# solver-44 report (Order 44, the pipe train)

Branch `solver-44`, off `origin/next` @07e8db37. Each item is one commit, with its CHANGELOG entry
under `## Unreleased`. Nothing was pushed. The proposals repo and kolt were not written.

## Items

| Item | State | sha | Premise |
|---|---|---|---|
| B463 | LANDED | e630072f | Held for the filed case: a trait default's body under a LOANED resource `Self`. The generic free-function shape was already refused. Scoped as ruled: the partial-move rule is untouched. |
| B452 | LANDED | 9c3fdfcc | Held. JS reordered siblings around a sibling that lowers to statements. |
| B469 | LANDED | 90298905 | Held. Door (a). |
| B456 | LANDED | 33835e4d | Held. Door (b): ranked admission, both orders, same module and cross-module in both load orders. The A86 pin was rewritten to the ranked admission. B300/B408 stand. |
| B470 | LANDED | c65ba161 + 8608d62e | Held. Declared `[resource] trait`. The follow-up makes it the DECLARING trait's alone: subtraits do not inherit it. The native half is native-44's. |
| B458 | LANDED | 56f3c819 | Held. `Context::clear(body)`. |
| B461 | LANDED | 493766e3 | Held for `let` and parameters. A struct field stays refused (A124 R3). |
| B462 | LANDED | 8ab2ff12 | Held. A tuple variant is its constructor where a closure is expected, on JS and native. A bare payload variant used as a value is refused. |
| B434 | LANDED | e7514bd1 | Partly held. The filed `filter_map` shape already compiled. The live case was a closure parameter reaching a generic constructor in the closure's tail. |
| B457 | LANDED | 3d3d0257 + 323a430f | Held. R-e door (a). The follow-up moves M86's pin (see Goldens). |
| B428 | LANDED | 24ed1c9f | Held. R-i. BREAKING: `128i8` is refused. |
| B460 | LANDED | d161af2d + 34682cf0 | Held. R-a door (i). The return is NOT hidden (said in the refusal's doc line and in spec/types.md). A trait method's bare-trait return stays refused, with its own steer. The follow-up adds the new heading to the book's anchor golden. |
| `.get()` on a pipe (A142 §3.3) | LANDED | 58565c6e | Held. The probe's miss offered `import std::reactive::Source`. The mechanism reads `Pipe`/`Source` by name, and the pins use a placeholder fixture. OWED after the rebase: pin the wording against reactive-44's std names. |
| B401 | LANDED | fefe49ce | Held, and worse, as solver-43 said: `import pkg::p1 only;` compiled the inherited default, and any name two loaded blocks offered was ambiguous whatever the file admitted, for declared members too. |
| B455 | PART of B401 | fefe49ce | Held. A whole-block selector admits a default-only block, and a tail can name an inherited default. NOT built: the "admissible by its trait name" spelling. |
| B471 | LANDED | 284953db | Held. Verified red and green on a throwaway branch carrying syntax-44's cffaf928 + 0950caea. The branch was deleted. |

## B401 (the restructure)

- A `LookupAdmission` is built in `resolve_world` after the import and type drains, before the fixpoint. It is rebuilt for the pre-entry world and for the build.
  - It uses the same restriction rows, the same files per statement and the same two-way selector test as the post-build `build_impl_admission`.
  - `statement_sources` and `ancestor_module_sources` are ported off `&Program` and shared by both passes. The driver hands in the source paths.
- Each constraint sets its importing file from its anchor, at the `rigid_binder_scope` seam, using B391's generated-code rule.
- `impl_member_candidates`, `inheriting_impls_of_declared_homes` and `inherited_default_candidates` NARROW by admission and never empty.
- When only a declined block answers, the answer stands:
  - a declared member is refused by the existing post-build pass;
  - an inherited default is recorded (`declined_default_calls`) and refused by name with the same message.
- Admission is keyed by block, so a whole-block selector admits the defaults the block inherits.
- The estate path builds nothing: no file restricts and no module hides a block. The path index is built only when a file restricts.

## Finds not fixed

1. B401 residue, found by reading the code and not reproduced: a `for x in receiver` whose `next` is an inherited default reachable ONLY through a declined block.
   - The lookup narrows, but the post-build refusal walks `function_calls`, and a loop is not one, so the loop would compile through the declined block.
   - This was already true before B401.
2. Native, predating B460: reading a field on a call with an INFERRED return fails natively. Repro: `scratch/solver-44/b460/un.vl`. B460's annotated returns hit the same path.
3. B457 cost: the notify `enqueue(turn, subscribers.read())` copies are conservative. The cells fall into the unplaceable component.
4. B456 does NOT cover reactive-44's find (1), the a52 inherent `sub` hijack. Find (2), the flatten order, was not looked at.
5. B471 owes a control pin at integration: a USER type's `.css` miss stays ordinary. It cannot parse before the member tier lands.

## Diagnostics ledger

- NEW (7):
  - B463: the admission leak;
  - B470: the undeclared-trait steer;
  - B458: the clear literal;
  - B461: does-not-match;
  - B462: carries-a-payload;
  - B460 (×2): the trait-method bare-trait return, and does-not-implement-the-returned-trait.
- Re-keyed (2):
  - row 261, R5 (B469);
  - row 206, misplaced `[resource]` (B470).
- The pipe steer, B401 and B471 add no rows.

## Goldens and pins moved

All are output-identical.

- B452: `json-roundtrip`, `time`, `usize`. Sibling-order spills.
- B457:
  - twelve corpus goldens (`blanket-impl`, `delta-law`, `dyn-objects`, `list-cell`, `reactive`, `reactive-flatten`, `reactive-on-change`, `reactive-owner`, `reactive-selector`, `reactive-turns`, `signal-update`, `spread-parameters`) and split `app.js`: `SignalCell` notify copies;
  - copy-elision census 419 → 467, deliberately;
  - `ui_rows` M86 `move_range wholes` 11 → 15. `each`/`each_by`'s whole-list pass hands `reconcile` the key and item runs by value, and `reconcile`'s `key_of`/`same` closure-value calls can reach an in-place write of those cells, so the runs are copied at the call. Native already copied them. **This is a JS perf cost the owner may want to see.** A closure-flow analysis could narrow it later.
- B460: `markdown_anchors.golden` gains one heading, regenerated with mdbook v0.5.4.

## Gates

Every std- or compiler-touching commit ran:
- inference;
- corpus, split, examples and copy_elision_census;
- check_scope_differential;
- `native_differential`, both default and with `VILAN_NATIVE_DIFFERENTIAL=1`;
- the touched suites.

Red runs were fixed and re-run before committing.

Last commits:

| Commit | inference | goldens | scope | native (×2) | touched suites | ledger |
|---|---|---|---|---|---|---|
| B460 | 4859/4859 | 33/33 | 15/15 | 81/81, 81/81 | docs + module_resolution 221/221 | 25/25 |
| steer | 4862/4862 | 33/33 | 15/15 | 81/81, 81/81 | — | 25/25 |
| B401 | 4862/4862 | 33/33 | 15/15 | 81/81, 81/81 | module_resolution + docs + `--lib` 1138/1138 | 25/25 |
| B471 | 4865/4865 | 33/33 | 15/15 | 81/81, 81/81 | — | 25/25 |

Full `cargo nextest run --workspace`:
- First run: 8974/8977. The failures were M86 and the two anchor-golden tests, fixed by the two follow-ups.
- Rerun: 8977/8977 passed, 32 skipped.

`cargo clippy --workspace --all-targets`: clean.

## Kolt

No kolt patch. B456 needs nothing beyond B419's owed patch.
