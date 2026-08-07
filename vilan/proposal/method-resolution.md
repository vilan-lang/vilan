# Method resolution — a deliberate precedence rule (B57)

> Status: RATIFIED 2026-08-04 (owner review) — implement as recommended:
> inherent-over-trait, duplicate-inherent as a hard error, trait-vs-trait
> ambiguity error with `Trait::method(receiver)` disambiguation.

## 0. The problem and the thesis

`value.member` today resolves by scanning every impl whose subject matches
the receiver's type and returning the **first textual/registration-order
hit** (`method_member_impl_subject`, `crates/vilan-core/src/analyzer.rs`
7745–7774). Nothing about which impl "should" win — inherent, trait,
which trait — enters the decision. The docs claim otherwise
(`docs/spec/names.md:114–116`: "fields first, then methods of inherent
impls, then trait members") and the backlog folklore says "inherent
wins", but neither is implemented: swap two impl blocks in one file and
the answer flips, silently, with exit 0 both times.

**Thesis: the fix is a real precedence rule — inherent beats trait,
trait-vs-trait is an error — not a bigger scan and not blessing today's
accident.** The survey below (§2) found exactly one place in the whole
std+examples+corpus tree where this currently bites, and it is a bug
sitting in the shipping, CI-gated corpus right now: a test file's method
is dead code, shadowed by std's own copy of the same name, undetected
because the two bodies happen to compute the same thing. The rule this
proposal recommends turns that into a caught error instead of a
coincidence.

## 1. The problem, demonstrated

### 1.1 The mechanism

`method_member_impl_subject` (analyzer.rs 7745–7774):

```rust
self.implementations
    .iter()
    .filter(|implementation| self.compare_type(subject_type, &implementation.subject.get_type(self), &HashMap::new()))
    .find_map(|implementation| {
        if self.member_is_trait_only(implementation, member_name) { return None; }
        let member_id = implementation.declarations.get(member_name).copied()
            .filter(|member_id| self.is_self_method(*member_id))?;
        Some((member_id, implementation.subject))
    })
```

`self.implementations` is a flat `Vec<Implementation>` (struct at
analyzer.rs 620–632: `subject`, `declarations: IndexMap<name, Id>`,
`trait_ids`, `trait_args`). It is never sorted, never grouped by
provenance, and `find_map` returns the first hit — a plain linear scan
over insertion order. `trait_ids` is present on the struct but this
function never reads it: an inherent impl and a trait impl are
indistinguishable to the scan.

An impl is pushed exactly once, at the point `Node::Impl` is walked
(analyzer.rs 14139–14217, push at 14209), so **insertion order is AST
walk order** — for one file, the order the impl blocks appear in the
source, top to bottom.

### 1.2 Reproduced, with the worktree binary

Built `cargo build` in this worktree (`vilan 0.24.0 (e662973cb)`,
`target/debug/vilan`). Two files, identical except impl-block order:

```vilan
struct Bag { x: i32 }
trait Iter<T> { fun pick(self): str; }

impl Bag { fun pick(self): str { "INHERENT (eager)" } }
impl Bag with Iter<i32> { fun pick(self): str { "TRAIT-INHERENT (lazy)" } }

fun main() { print(Bag { x = 1 }.pick()); }
```

```
$ target/debug/vilan run order_a.vl   # inherent block first
INHERENT (eager)
$ target/debug/vilan run order_b.vl   # trait block first (only the two impls swapped)
TRAIT-INHERENT (lazy)
```

Both runs exit 0. No warning, no error, nothing in stderr either way —
matches iterator-adapters.md §4's own probe of the identical shape (the
proposal that surfaced this as backlog item 57 while designing I3's
iterator adapters).

### 1.3 The escape hatch that doesn't escape

The grammar already accepts a qualified-path call form,
`Type::member(receiver, args…)` (`path = IDENT "::" IDENT`, grammar.md
204–213), and the analyzer's static-accessor resolver
(`prepped_static_accessors`, analyzer.rs 20905–20947) does let a
self-method through — `Bag::pick(b)` compiles, unlike the spec's claim
in names.md §4.6 that `Type::member` only reaches "static functions...
those without `self`". But it resolves through the **same flat
`self.implementations` scan**, filtered by subject only, not by trait —
it is not UFCS-style disambiguation, it's the identical order-dependent
lookup spelled differently:

```
$ vilan run ufcs_a.vl   # Bag::pick(b), inherent block first
INHERENT (eager)
$ vilan run ufcs_a_swapped.vl   # same call, blocks swapped
TRAIT-INHERENT (lazy)
```

Naming the *trait* at the path head does not work at all today —
`Iter::pick(b)` fails with `cannot find 'pick' in Iter`: the same
resolver, given a `Type::Trait` subject, only searches `self.implementations`
for an impl whose **subject is the trait's own type** (the
`impl Iterator<type T> { fun from_fn(...) }` namespace-attachment shape
that gives `Iterator::from_fn` its home in `iterator.vl` 17–21) — it
never consults the trait's own `declarations` map. So today there is no
working spelling that picks a specific trait's version of a method.
This matters for §3's disambiguation design.

## 2. What order registration actually produces today

Three independent tie-break axes, all accidental:

- **Within one file:** textual order (§1.1) — the walk visits a file's
  nodes top to bottom.
- **Across files/modules:** the module *loader's* order, which as of the
  WO-1b canonicalization (analyzer.rs 25110–25127, comment: "the
  canonical module load order... std modules first (tier 0), then each
  dependency package by its manifest index (tier 1), then the entry
  package's own modules (tier 2); ties broken by module name") is
  **deterministic but not import-order-dependent** — the drain loop
  (25415–25436) always pulls the lexicographically-smallest
  `(tier, index, name)` key still pending, `to_load.swap_remove`, not a
  stack or queue. Concretely: two std modules tie-break alphabetically
  by name. The **entry file's own top-level code walks dead last**
  (`analyze_over_world`, 26244–26248), after every std/dependency/sibling
  module it transitively references (25839–25866). A fixed set of "core"
  std modules — `boolean`, `list`, `null`, `promise`, `compare`,
  `default`, `debug`, `json`, `hash` (25188–25192) — is force-loaded into
  tier 0 even when not imported.
- **A bounded generic's `T: A + B`:** a *third* axis, scoped to the
  written bound clause: `Type::Generic` resolution walks `bound_traits`
  in the order the constraint was written and **`break`s on the first
  trait that has the member** (analyzer.rs 18346–18389, `break` at
  18386) — no ambiguity check, no scan of the rest.
- **Platform twins never compete.** `browser/ui.vl` and `process/ui.vl`
  are not two impls of one `ui` module — module resolution is *layered*:
  `PackageSpec::search_roots(platform)` (24556–24577) orders
  platform-matching layers before the base, and
  `resolve_module_in_roots` returns the **first** hit. Exactly one file
  named `ui` loads per build. Twin impls of `View`/`Signal`/`str` in the
  two platform files are mutually exclusive by construction — they never
  land in `self.implementations` together, so they are not a source of
  ambiguity, ever.
- **The inherited-default fallback (Gap E) reuses the identical
  mechanism one level down.** `method_member_in_inherited_defaults`
  (8135–8171) runs only when stage 1 misses, and it too is a flat
  `find_map`, this time over `(implementation, trait_id)` pairs, first
  hit wins. Two supertraits (or two impl blocks) offering the same
  default name tie-break the same way, one level removed.

### 2.1 The measurement

Swept `vilan/std/src`, `vilan/examples`, `vilan/test` (the byte-gated
corpus), plus `vilan/macro_std` and `vilan/benchmarks` for completeness —
197 `.vl` files, parsed lexically (a brace-depth scan that skips over
string-literal contents, so an `i"…{expr}…"` hole's braces don't
mis-parse impl-block boundaries) for every `(subject type, method name)`
pair and which impl block(s) declare it. **1,095 impl-declared methods
scanned; 1,060 distinct `(subject, method)` pairs; 34 pairs where the
name is declared in more than one impl block.**

Every one of the 34 was checked by hand against what actually compiles
together:

- **24 are the browser/process platform twins** (`View.attr`,
  `View.bind_attr`, …, `Signal.apply`, `Signal.place`, `str.apply`,
  `str.place`, `List.place` — every one of them `[browser/ui.vl]` vs
  `[process/ui.vl]`). Per §2's twin-resolution finding, these never
  coexist in one build — zero live ambiguity.
- **9 are coincidental name matches between fully independent
  programs** — `Counter.increment`/`Counter.value_of`/`Point.eq`/
  `Point.sum`/`Point.to_string`/`Res.drop`/`Wrapper.slot` each pair two
  *different* `vilan/test/*.vl` files (none of the 107 test files
  `import pkg::` another — each is its own single-file program, checked
  directly), and `Route.to_path` pairs `examples/router/app.vl` against
  `examples/walkthrough/src/routes.vl` (two different apps). None of
  these nine pairs are ever loaded into the same `self.implementations`
  list.
- **1 is real.** `vilan/std/src/option.vl` 197–204 declares
  `impl Option<(type T, type U)> { fun unzip(self): (Option<T>,
  Option<U>) { … } }`. `vilan/test/gap-b.vl` 8–15 — a corpus file that
  `import std::option::Option::{ self, Some, None }`, i.e. the *real*
  std `Option` — declares the **verbatim same impl again**, body and
  all. `gap-b.vl` is part of the byte-identical corpus gate
  (`crates/vilan-cli/tests/corpus.rs` 1–9: "every `vilan/test/*.vl` with
  a `.js` golden compiles... to byte-identical output").

  Registration order makes this harmless by accident today (std's copy
  registers first — the entry file walks last, §2), but it is not
  vacuous: swapping the body of a *local* copy of `gap-b.vl` for one
  that prints a marker before computing the same result proves it —
  `target/debug/vilan run` on the modified probe never prints the
  marker. **`gap-b.vl`'s own `unzip` is dead code, today, in a shipping,
  CI-gated file, with no diagnostic anywhere saying so.** This is the
  proposal's best evidence that the gap is not theoretical.

**Verdict: one real dual-reachable site today, found by sweeping every
impl in std, every example, and the entire regression corpus.** The
mechanism has not caused visible harm because the codebase has been
disciplined about not reusing method names across a type's impls — not
because the compiler stops it.

### 2.2 The stakes are prospective, and the numbers say something sharper than "be careful"

iterator-adapters.md §4 names the forward-looking case directly: `List`
has inherent eager `map`/`filter`/`fold`/`for_each` (`list.vl` 27, 36,
47, 55), and I3 wants lazy adapters with the same names. If those names
are ever made to collide on `List` itself (I3 §4's name-policy option
(i), or a future blanket-`Iterable`-default path once backlog item 58's
bound-propagation gap closes), **the winner is computable today, and it
is backwards from folklore**: `iterator` and `list` are both std tier-0
modules, tie-broken alphabetically by module name — `"iterator" <
"list"` — so `iterator.vl`'s impls would register, and win,
*before* `list.vl`'s own inherent methods. "Inherent wins" is not just
undocumented; if anyone tests it against the one case anyone actually
cares about, it is currently false. §5 works this through for I3
specifically.

## 3. The precedence design

Three shapes, evaluated against the survey:

**(a) Inherent-over-trait, trait-vs-trait an error.** An inherent impl's
declared member always wins over any trait-provided member (declared or
inherited-default) for the same subject and name, unconditionally, no
error. Two *trait* impls (or two supertraits' defaults, or two arms of a
`T: A + B` bound) that both declare the same name for the same subject,
with no inherent impl in the running: an ambiguity error, disambiguated
explicitly (§3.1). This is Rust's shape.

Refined by what the survey actually found: **two *inherent* impls
declaring the same name for the same subject must also be an
unconditional error**, not merely a candidate for the trait-tier tie
rule. `Option.unzip` (§2.1) is inherent vs. inherent — there is no trait
to rank against — and Rust treats exactly this shape (duplicate
inherent-impl definitions) as a hard, definition-time error (E0592),
independent of whether any call site ever resolves it. This closes the
survey's one real hit precisely: `gap-b.vl` would stop compiling with the
"already defined" diagnostic §4 spells out, forcing the one-line fix
(delete the dead redeclaration) instead of leaving it silently inert.

**(b) Specificity-based** (a more concrete impl beats a more generic one
— e.g. `impl List<i32>` over `impl List<type T>` for the same name).
Real prior art exists for wanting this (Rust's own specialization,
unstable for exactly this reason), but it needs machinery this problem
does not currently justify: a total or partial order over every impl of
a subject (including across generic bound sets), a coherence/overlap
checker run at definition time over the whole program, and — critically
— it does not even resolve the survey's cases. `Option.unzip`'s two
impls are equally generic (same subject pattern, literally the same
text); "specificity" has nothing to rank between them. Trait-vs-trait
ambiguity (two different traits, same concrete subject) is likewise
unordered by specificity — you'd still need (a)'s tie-break on top.
Not recommended now: it is strictly more machinery than (a), doesn't
subsume it, and the survey shows zero present demand (std's existing
bound-tiered impls for `List` — `type T`, `type T: Add + Default`,
`type T: Mul + Default` — already use distinct method names per tier;
nothing in the corpus needs specificity to resolve a real collision).
Worth reconsidering only if a real specificity need surfaces later; it
would layer inside (a)'s inherent or trait tier without disturbing the
tier ordering itself.

**(c) Declaration-order-as-spec** (bless today's behavior, document the
rule instead of changing it). Rejected, for a reason grounded in this
project's own stated values as much as in the mechanism: CLAUDE.md's
first engineering principle is "Refactoring is the preferred path...
Refactoring is a *good thing*." Blessing registration order makes
*moving code* — reordering two impl blocks, renaming a module file so it
sorts differently, moving a method from one impl block to another —
a **behavior change**, silently, for any type with a same-named
collision. §2.2's alphabetical-module-name result is the sharpest
illustration available: renaming `iterator.vl` to `adapters.vl` (or
`list.vl` to `lists.vl`) would, under (c), silently flip which of
`List`'s `map` wins, with no diff to the call sites that depend on it.
A rule whose meaning depends on a source file's name is not a rule this
project would accept anywhere else in the language; it should not
accept one here either.

**Recommendation: (a), refined to make duplicate-inherent an error
too.** The survey found no code it breaks beyond the one site it is
supposed to catch.

### 3.1 The tie-breaker: disambiguation syntax

§1.3 already found the relevant grammar exists and is under-used:
`Type::member(receiver, args…)` parses today (`path = IDENT "::" IDENT`,
no new syntax needed) and its resolver already accepts a self-method
receiver — it just doesn't discriminate by trait yet. Two changes, no
grammar change:

1. **`Trait::member(receiver, args…)` becomes a real disambiguator.**
   When the static-accessor resolver's subject is `Type::Trait(trait_id,
   _)` and no attached-static match exists (today's `Iterator::from_fn`
   path, unchanged), fall back to: resolve `member` in `trait_id`'s own
   `declarations` (`method_member_in_trait`, already exists, 8092–8101),
   then dispatch it against argument 0's concrete type exactly the way
   Gap E's inherited-default fallback already does (`generic_dispatch`
   insert + re-dispatch at codegen, analyzer.rs 18267–18304) — the
   re-dispatch machinery for "found the trait's version, now specialize
   it to a concrete receiver" already exists for defaults; this reuses
   it for a call with an *explicit* trait head instead of an implicit
   inherited one.
2. **`ConcreteType::member(receiver, args…)` should mean "the inherent
   one, or an error"**, for symmetry — right now it silently rides the
   same ambiguous scan as `.member()` (§1.3), which would leave a second,
   unreformed path into the exact ambiguity the rest of this proposal
   closes. Once inherent-over-trait ships, `Bag::pick(b)` should mean
   "the inherent `pick`, if one exists" and error otherwise (never
   fall through to a trait's), rather than "whatever the scan finds
   first."

No `as`-cast form (Rust's `<Type as Trait>::method`) exists in vilan's
grammar and none is proposed — `as` isn't a keyword here (spec §5.8:
conversions go through `as_*` methods, not an `as` operator), and
`Trait::method(receiver)` already says the same thing with syntax that
parses today.

## 4. The ambiguity diagnostic

Two distinct diagnostics, per §3's two error shapes, following
diagnostics-standard.md:

**Duplicate inherent member (definition-time, joins the existing
13-pass "definition-site check" family** — CHANGELOG's own description
of that family: "mutability, views, must-use, trait conformance, and
friends" — **so this is not a new kind of pass, it's a new member of
one that already exists):**

> `'unzip' is already defined for 'Option<(T, U)>' (…/option.vl:198);
> remove or rename this one`

Anchored (A1/A3) at the **second** declaration in a deterministic
`(source_id, span)` order — independent of load/registration order, so
the same program always anchors the same place regardless of which
module happened to load first. A secondary note (C3) points at the
first declaration's span. B1: user-facing type spelling
(`Option<(T, U)>`), not `TypeId`/`Implementation`. B4: the steer is
already the whole fix — remove or rename.

**Ambiguous call (call-site, joins the "no method" family at
18510–18549 — same anchor, same steer-suffix pattern the `[trait_only]`
note and the import steer already use):**

> `'pick' is ambiguous on 'Bag': both 'Iter<i32>' and 'Ord' provide it;
> call 'Iter::pick(b)' or 'Ord::pick(b)' to pick one`

Anchored at the member-name span (A1/A4, matching "has no method" and
`BareTraitValue`'s existing anchor). Names both candidates by their
*home* (B1 user vocabulary — the trait name, never an internal id); the
steer (B4) is the exact `Trait::member(receiver)` spelling §3.1 makes
real, built from the call's own receiver expression so it is
copy-pasteable, not generic advice. C3's secondary note is available if
one candidate's definition site is worth pointing at (e.g. when one
trait is inherited-default and the other is a direct declaration) but
isn't required — both homes are already named in the primary message,
unlike the duplicate-inherent case where the second location is the
whole point.

Neither message says "registration order", "find_map", "impl", "Vec",
or any other internal term (B1) — the user-facing story is "two things
provide this name" and "here's how to pick," never "here's how the
compiler happened to find it."

## 5. Interaction with I3's name policy

iterator-adapters.md §4 is explicit that its own recommendation (option
(ii): re-express `List`'s eager methods over the adapters, one meaning
per name) is conditional on this exact rule not existing yet — its §10a
open question (a) says so directly. This proposal's answer, precisely:

**Under (a) — inherent beats trait, unconditionally, no error** — any
future attempt to make a lazy adapter reachable *directly on `List`*
under the same name (`map`/`filter`/`fold`) is simply dead: `List`'s own
inherent method wins every time, silently and by design, so I3's name-
policy option (i) ("share the names, lazy reachable only via `.iter()`")
gets no help from precedence — it still depends on the *receiver type*
differing (`List` vs. whatever `.iter()` returns) to avoid a collision
at all, which is exactly the "reader can't tell them apart, `.iter()`
being added or removed three lines up changes behavior" problem
iterator-adapters.md already flagged as unreportable. **Under (c) — bless
today's order** — §2.2 already computed the opposite, backwards outcome:
`iterator.vl` sorts before `list.vl`, so a shared name would silently
hand the lazy adapter the win over `List`'s eager inherent method,
exactly inverted from what "inherent wins" folklore assumes. Neither
option makes name-sharing (I3 §4 option (i)) safe; both outcomes
reinforce I3's own recommendation to re-express the eager forms over the
adapters (option (ii)) rather than share the three names. **Recommend
deciding this proposal first** — I3 §10a already frames its choice as
downstream of it.

## 6. Migration

The survey (§2.1) is the migration risk assessment: shipping (a) changes
observable behavior at exactly one known site (`gap-b.vl`, which becomes
a compile error, not a silent flip — the whole point), and zero other
sites in std, examples, or the corpus. That is a narrow, already-measured
blast radius, which argues against building new transition machinery
(a soft "old and new rules disagree" runtime warning) for a change this
well-bounded — CLAUDE.md's own discipline is "prove a feature... unit
tests and regression tests that pin that behavior down," and the corpus
byte-gate (`cargo test -p vilan-cli --test corpus`) already *is* that
mechanism once the fix and the new check land together.

The one caveat: §2.1's sweep is **lexical**, not the compiler's own
type-aware `compare_type`/generic-bound reconciliation — it cannot see
collisions that only appear after full generic unification (e.g. two
differently-bounded impls of the same generic subject that happen to
overlap for some but not all instantiations). The safety net this
proposal recommends: when the tiered resolver (§3, S2) lands, run it
**alongside** the current flat scan over the whole std+examples+corpus
build in a one-time internal self-check (debug-assert or a dedicated
test binary) that asserts the two agree on every call site except the
ones the new duplicate/ambiguity checks are expected to newly reject —
proving, with the real type-aware machinery rather than this proposal's
approximation, that nothing else silently changes. This is strictly
cheaper than building a persistent transition-warning diagnostic, and it
is the corpus byte-gate's own philosophy (measure the real thing,
once, before trusting it) applied to this change specifically.

## 7. Slices

- **S1 — the coherence rule (duplicate inherent), as a definition-site
  check.** Joins the existing 13-pass family (`CHANGELOG.md` line 73);
  skips std-defined entities the same way the family already does,
  except std-against-std duplicates (there are none today — §2.1 — but
  the check should not assume that stays true). Ships with `gap-b.vl`'s
  fix (delete the dead redeclaration) in the same commit. Gate: the
  duplicate shape planted as a red-first regression pin
  (`assert_fails_spanning`, per CLAUDE.md's non-vacuous-pin rule), plus
  the full corpus suite green with the fix applied.
- **S2 — the tiered resolver.** Rewrites the flat scans into: inherent
  first (error if 2+ inherent declare the name — S1's rule, now
  enforced at the point that would otherwise silently pick one),
  else exactly one trait-impl declaration (error if 2+), else exactly
  one inherited default (error if 2+, respecting supertrait dedup —
  `Ord` requiring `PartialEq` must not double-count `eq` from both).
  Touches `method_member_impl_subject`, the `prepped_static_accessors`
  loop's static-path arm, `method_member_in_inherited_defaults`, and
  the `Type::Generic` bound-list walk (18346–18389) — the same
  ambiguity discipline replaces its `break`-on-first-hit. Gate: unit
  pins per candidate shape (inherent/trait, trait/trait, default/
  default, bound-list/bound-list) — the multi-parameter, ordering-
  sensitive edge cases CLAUDE.md's testing section names explicitly.
- **S3 — the diagnostics** (§4), ledgered per diagnostics-standard.md,
  plus the `Trait::member(receiver, …)` disambiguator (§3.1) and
  `ConcreteType::member(receiver, …)`'s tightening to inherent-only.
  Gate: `assert_fails_spanning` pins for both messages; a pin proving
  the steer's exact spelling round-trips (write `Trait::method(b)`,
  it compiles and picks the named candidate).
- **S4 — the migration self-check** (§6): the old-scan-vs-new-resolver
  agreement sweep over std+examples+corpus, run once as part of landing
  S2, not kept as permanent machinery.
- **S5 — docs.** `docs/spec/names.md` §4.6 gets the real rule (today's
  sentence is aspirational, per iterator-adapters.md §4's own finding);
  `docs/appendix/errors.md` gains both new messages. Gated by the docs
  fence test (`cargo test --test docs`) per CLAUDE.md.

Order: S1 first (it stands alone and already fixes the one known-live
bug); S2 next, with S4's agreement sweep run as part of landing it, not
after; S3 alongside or immediately after S2 (the diagnostics are what
make S2's new errors legible); S5 last (docs describe the shipped rule,
not the planned one).

## 8. Open questions

**(a) The precedence shape** — (a)/(b)/(c) from §3? *Recommendation:
(a), refined so duplicate-inherent is also an unconditional error, not
only trait-vs-trait ties.* The survey's one real hit is inherent-vs-
inherent; a rule that only disambiguates trait ties would miss it.

**(b) Disambiguation syntax** — repurpose `Trait::member(receiver, …)`
(§3.1, no grammar change), or design something new? *Recommendation:
repurpose it.* It already parses, the resolver already accepts a
self-method receiver through the sibling `Type::member` path, and the
re-dispatch machinery it needs already exists for Gap E's inherited
defaults — this is assembly, not new mechanism.

**(c) Should `ConcreteType::member(receiver)` be tightened to
inherent-only once the rule ships?** *Recommendation: yes, in the same
slice as the diagnostic (S3).* Leaving it riding the old flat scan keeps
a second, unreformed door into the exact ambiguity §3 closes.

**(d) The migration safety net** — a persistent transition diagnostic,
or a one-time corpus-wide self-check? *Recommendation: the one-time
self-check (§6, S4), re-run with the real type-aware resolver, not this
proposal's lexical sweep — the survey shows the blast radius is one
file, which doesn't justify permanent transition machinery, but the
sweep's own limits mean the real check has to be the compiler's, not
this document's.*

**(e) Is the `T: A + B` bound-list `break` (18346–18389) in scope of
this arc?** *Recommendation: yes, as part of S2, not a separate
follow-up.* It's the same disease (first-hit-wins, no ambiguity check)
in a third location; shipping the concrete-subject fix without it
leaves an identically-shaped bug reachable through any bounded generic.

**(f) I3's name policy** — not this proposal's call, but see §5:
*recommend settling this proposal first*, since I3 §10a already
recommends re-expressing (option ii) *conditional on* the precedence
question this proposal answers.

## 9. Implementation notes (shipped, v0.30.0 cycle)

Every slice S1–S5 landed as designed. Six places where the built thing
differs from the paper, and why:

**(1) The tie-break order inside a tier is the declaration's entity id,
not `(source_id, span)` (§4).** Entity ids are minted in walk order:
textual order within a file, the canonical module order across files
(std first, the entry file last), which is exactly the deterministic,
import-order-independent order §2 established. The literal `SourceId`
cannot be used, and measuring it is what found this: the entry file is
pinned to `SourceId(0)` so editor features resolve against the open
document, so sorting by the raw id ranks the *user's* declarations before
std's — and the first agreement sweep duly anchored `gap-b.vl`'s
duplicate-`unzip` diagnostic inside `std/src/option.vl`, the one file the
user cannot edit. Entity-id order realizes §4's intent ("the same program
always anchors the same place"); the raw source id defeats it.

**(2) "Inherent" is a property of the MEMBER, not of the impl block.** §3
says "an inherent impl's declared member". The implementation asks
instead whether any trait of the declaring impl (or a supertrait of one)
declares that name: a method written inside a `with`-clause block that
the trait does *not* declare is the type's own, and competes in the
inherent tier. This matters for both halves of the rule — it keeps the
duplicate check honest (an "extra" method in a trait-impl block does
collide with an inherent one of the same name, and has a pin), and it
keeps the ambiguity diagnostic truthful, since it names each candidate's
home trait and such a member has none.

**(3) `gap-b.vl` is renamed, not deleted (S1).** §2.1's fix was "delete
the dead redeclaration". Deleting it would have left the file calling
*std's* `unzip`, retiring the Gap B shape (an impl subject with `type`
binders nested in a tuple) that the file exists to pin. `unzip_pair`
keeps the pin and clears the collision; the diagnostic offers both fixes
by design. The `.js` golden is byte-identical either way — the emitted
function is name-mangled and the two bodies compile the same.

**(4) The duplicate diagnostic's second location is a NOTE, not an inline
path (§4).** The paper writes `'unzip' is already defined for
'Option<(T, U)>' (…/option.vl:198)`. The renderer already carries a
cross-file secondary note (the conformance-note shape: a span plus its own
`SourceId`), which renders the other declaration with its real source
excerpt instead of a hand-formatted path. The message keeps a
`by module 'option'` clause so the one-line form still says where.

**(5) `Trait::member(receiver, …)` against an INHERITED default is wired
as the method call it is equivalent to.** §3.1 asked for "`generic_dispatch`
insert + re-dispatch at codegen", which is what happens — but the call
cannot also ride the ordinary call path, because the trait's declaration
types its receiver as the trait itself and `reconcile_type(Trait, Struct)`
has no arm (a pre-existing gap: `fun show(v: SomeTrait)` rejects a
concrete implementing value today, on `next` as much as on this branch).
So the analyzer re-wires the call through `wire_method_call`, exactly as
`receiver.member()` is wired, and records the named trait in
`bound_dispatch_traits` — without which two traits whose defaults share a
name both resolved to whichever the transformer's by-name lookup reached
first, which would have silently undone the disambiguation.

**(6) Two impls of the SAME trait are deliberately not an ambiguity.**
The trait tier dedups by trait, so the name still has one home. This is
what keeps the platform twins (§2) safe against a future build that did
load both, and it leaves the std blanket `impl type T with Into<T>`
behaving exactly as it does today for a type that also writes its own
`Into` impl — a specificity question, which §3(b) declines.

### The agreement check (§6 / S4), as run

The probe compared the tiered resolver against the old flat scan at every
concrete-receiver resolution, behind `VILAN_RESOLVER_AGREEMENT`, and was
removed before landing (S4: "run once… not kept as permanent machinery").

- **`vilan/test` — 111 programs:** one flagged site, `unzip`, flagged as
  `dup_inherent`, both resolvers picking the *same* member (std's). Zero
  resolution changes. With `gap-b.vl` fixed: zero flags.
- **`vilan/examples` — 9 packages:** zero.
- **`vilan/docs` fences (the docs gate):** zero.
- **`crates/vilan-core/tests/inference.rs` — 1588 tests:** zero.

So the survey's headline holds under the compiler's own type-aware
machinery, and is if anything narrower than §2.1 predicted: the one live
site is not a *resolution* change at all — both rules pick std's `unzip` —
it is a duplicate the old rule could not see.

## 10. B72: why a bare-trait parameter gets a steer, not an implementation

B57's §9(5) recorded the gap in passing — `reconcile_type(Trait, Struct)`
has no arm, so `fun show(v: SomeTrait)` rejects a concrete implementing
value — and filed it as B72. Taking it up, the question was whether a
bare-trait parameter should be made to WORK (as sugar for a bounded
generic, monomorphized per call site) or be steered away from. It is
steered away from. The evidence, in the order it settled the question:

**(1) The language already answered, in code and in the spec.** The
analyzer carries a named lookup outcome, `MethodLookup::BareTraitValue`,
whose whole job is this refusal: *"a trait is not a value type (vilan has
no trait objects). Use a generic parameter (`<T: A>`) or a concrete
type."* The spec says the same normatively — `spec/types.md` §5.5
("Traits are used as **bounds**; a trait is not a type") and §5.11, which
lists "Using a trait as a type" first among the rejection cases — and the
tour repeats it. Making a bare-trait parameter mean "generic" would put
one position at odds with a rule stated in three places.

**(2) Accepting it routes a value into a compiler internal error.** The
decisive measurement. Adding the missing direction as an *acceptance*
(symmetric with the existing `(Struct|Enum, Trait)` arm) lets the value
flow onward, and the moment it reaches a bounded generic the monomorphizer
has no concrete implementation to select. It lands on B55's never-silent
guard:

    internal: a call resolved to `A`'s requirement `name`, which has no
    body — emitting it would produce an empty function and a runtime
    `TypeError`. … please report this program

That is not a trait object half-built; it is a value the compiler cannot
finish compiling. A bare-trait parameter has no implementation short of
B4's `(value, vtable)` representation, which is exactly what B4 exists to
design.

**(3) The narrow, real root cause.** A CALL is the only position that
reconciles PARAMETER-FIRST — deliberately, so bindings key on the callee's
generics (`f<U>(u: U)` must bind `U = T`, not `T = U`). It is therefore
the only position that ever asks `reconcile_type(Trait, Concrete)`. Every
other position — `let` annotation, return, struct field, closure
parameter, enum-variant payload, and a METHOD's parameter — reconciles
value-first and lands on the `(Struct|Enum, Trait)` arm, which accepts.
So the asymmetry B72 reports is one site wide, and the fix is that site's
message.

Shipped: at the parameter-first mismatch, a parameter whose declared type
is a bare trait and whose argument *does* implement it gets the steer,
with a note at the parameter's declaration (carrying its own `SourceId`,
so it renders across modules). An argument that does NOT implement the
trait keeps `Expected A, but got Bag instead` — there the likelier mistake
is the missing impl, and naming the type it failed to match is the more
useful report.

### What this deliberately leaves open (B4's, not a diagnostic's)

A bare trait remains **accepted** in every value-first position: `let x: A
= bag`, a trait-typed field, a trait-typed return, and a method's
trait-typed parameter all compile today, and only *using* such a value
fails. The spec says the annotation itself should be rejected
(`types.md` §5.11); the implementation refuses the use, not the
declaration. Closing that gap means making a trait type illegal in every
value position, which is a language change with at least one std
dependency to answer for first: `std/src/iterator.vl`'s

    impl Iterator<type T> with Iterable<T> {
        fun iter(self): Iterator<T> { self }
    }

returns a bare trait, and is pinned
(`a_trait_typed_self_returns_through_a_trait_typed_signature`). Whoever
takes B4 up owns that call; the positions are pinned in both directions
under `b72_*` so the current state is described rather than assumed.

### Bycatch, filed separately

Reaching a bounded generic with a bare-trait-typed value produces the
`internal: … please report this program` guard above on a plain user
program — reachable today through `let x: A = bag; use_it(x)` with no
change from this arc. It is a wrong diagnostic (an internal-error shape
for a user-level mistake), not a miscompile, and belongs with B4.

## 11. B84: one block declaring one name twice

The duplicate rule §3/§4 designs ranks TWO BLOCKS competing for one
surface. A block competing with *itself* was never in scope, and it turned
out never to have been reachable either: `Implementation::declarations`
was collected by reading a scope's `name_to_id_map` back, and a map holds
one entry per name. Two `fun which(self)` in one `impl` overwrote each
other during the walk, so by the time `check_duplicate_inherent_members`
ran there was one declaration, no pair, and no error — the program
compiled to the second definition, silently. The identical two
declarations one block apart were a hard error throughout. Nothing about
the rule differed; only whether the second declaration had survived to be
counted.

The fix is in the record, not the check. A scope now keeps
`declaration_order` — every item declaration, in walk order, repeats
included — beside the `name_to_id_map` index, exactly as
`local_value_declarations` keeps positional value bindings beside it for
`local-shadowing.md` §2. `Implementation` and `Trait` each carry the
resulting `declared_members`, and their one-entry-per-name `declarations`
is that list collected into an `IndexMap`. So the surface is *derived*
from the record instead of standing in for it.

**Two rules, deliberately separate.** The inherent rule (§3) exempts a
trait-provided name so that two impls of one trait stay legal — §9(6)'s
platform twins, and the std `Into` blanket beside a user's own. Neither
justification reaches inside a single block: there is no second impl to be
a twin of, and a name a block writes twice is a mistake whatever trait
homes it. So `check_duplicate_block_members` asks only "was this name
written twice *here*", covering `trait` bodies for the same reason, and
the inherent check skips a same-block pair so an inherent duplicate is
reported once rather than by both. Both emit the same diagnostic through
one shared reporter, so the two rules are indistinguishable to a reader
of the error — which is the point: the block boundary was never something
a user should have been able to feel.

**Blast radius: none.** A sweep of every `.vl` file in the repo (225) and
every fenced example in `docs/` and `proposal/` found zero same-block
duplicates. std's one recorded candidate — `pop`, which `std-surface.md`
§1.1 records as declared in both `list.vl` and `option.vl` — is stale:
`option.vl` no longer redeclares it, and it was cross-file (so cross-block,
and skipped as frozen) either way. bindgen self-manages one shared name
table (`unique_name`) precisely because of this hole; its output is
unchanged, and its pin
`a_duplicate_function_name_is_silently_shadowed_rather_than_rejected` —
written to go red the day this landed — now asserts the error under the
name `a_duplicate_function_name_is_rejected`.

## 12. B83: the static path gets the tiering

> RULED 2026-08-08 (owner): CLOSED AS DESIGNED — no trait-qualified
> static syntax; the inherent declaration IS the disambiguator (it
> outranks both trait tiers, which the shipped diagnostic already
> steers to). Revisit only if a real user collision demands the
> qualified spelling. The sweep found zero in-tree collisions.

§S2's residue, filed as B83 by B74's arc. `prepped_static_accessors`
resolved `Type::member` with a flat `find_map` over `implementations` in
registration order, so a trait-provided static BEAT an inherent one that
happened to register later. §3's headline rule — inherent over trait,
unconditionally — was inverted on the one path §3 never reached, and which
answer you got depended on the order the impl blocks were written in.
Verified both ways before the fix: with `impl Bag with Default` first,
`Bag::default()` gave the trait's `7`; with the inherent block first, `1`.

The fix is the ranking, not the candidate set. §3's tiering came out of
`resolve_impl_member` into `rank_member_candidates`, and the static path
now feeds it exactly the candidates it always scanned — a declaration with
no `self` receiver qualifies, and a `[trait_only]` member is reachable
when the path head IS the trait. So a trait-SUBJECT impl (`impl
Iterator<type T> { fun from_fn(..) }`, which is how `Iterator::from_fn`
resolves) keeps working unchanged, and the only behavior that moves is
which of two competing candidates wins.

**The trait tier stays reachable here, unlike §3.1.** For a method, §3.1
tightened `Type::method(receiver, ..)` to the type's OWN member because
`Trait::method(receiver)` is the sanctioned alternative spelling. A static
has no alternative spelling, so refusing the trait tier would make every
trait-provided static uncallable — `Bag::default()` against a lone
`impl Bag with Default` must resolve, and is pinned to.

**The ambiguity diagnostic cannot offer §4's steer, and says so.** Two
traits providing one static with nothing inherent above them is the same
ambiguity §4 describes, and it is now reported instead of silently
resolved to whichever registered first. But §4's fix — "call
`Trait::member(receiver)` to pick one" — does not exist on this path, and
`Trait::static()` cannot be built on today's design: the qualified form
selects an impl THROUGH the receiver's type, and a static offers nothing
to select with. Probed and pinned in both forms, with and without a
default body on the trait's declaration: `Alpha::spawn()` reports "cannot
find 'spawn' in Alpha". Whether a static should be reachable through a
trait at all — and what would name the impl if it were — is a design
question this arc does not answer.

So the diagnostic names the fix that always works, and names the missing
one as missing rather than leaving it to be hunted for:

    'spawn' is ambiguous on 'Bag': both 'Alpha' and 'Beta' provide it as a
    static, and a static has no receiver for a `Trait::spawn` path to
    select through; declare 'Bag''s own 'spawn', which outranks every
    trait-provided one

An impossible steer is worse than no steer (B65's lesson), so the named
fix is itself pinned working rather than asserted.

**Blast radius: none.** A sweep for a subject with the same static name
declared by both an inherent block and a trait-providing one found zero
sites across std, the corpus, the examples and every bindgen fixture; the
corpus goldens are byte-identical and all ten examples build. The pin
`b74_a_trait_provided_static_does_not_collide_with_an_inherent_one` — which
recorded the inversion in a note, deliberately unpinned — now asserts the
value: `1`, the inherent one. The two claims sit on one program, which is
the honest place for them: the trait's declaration is not a DUPLICATE of
the inherent one (B74), it is OUTRANKED by it (B57).
