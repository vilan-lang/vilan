# Method resolution — a deliberate precedence rule (B57)

> **§13 RULED 2026-08-18 as recommended** ("Go with B73 as recommended"): Q1 (a) — R1 → R2 → R3 in that order, (b) not taken, (c) stays available as a later additive layer; Q2 yes — the expected type steers among argument-distinct homes; Q3 the std blanket STAYS — deleting it remains an independent simplification (tracked as B127, deferred), not part of B73; Q4 call site; Q5 `9` — a more specific impl takes the trait's default; Q6 in scope — bounds are part of applicability (row 15's pin is one of the thirteen). The fix is scheduled (Order 3, cycle 21); the 13 `b73_*` pins un-ignore as each rule lands. BETA-CRITICAL per beta.md Q2.
> 
> Prior status: RATIFIED 2026-08-04 (owner review) — implement as recommended:
> inherent-over-trait, duplicate-inherent as a hard error, trait-vs-trait
> ambiguity error with `Trait::method(receiver)` disambiguation.
>
> **§13 (B73, the blanket-vs-specific design) — drafted 2026-08-18, RULED
> the same day (above).** §9(6) declined the specificity question and §15.8 of
> `trait-objects.md` left the overlap it names to B73; beta.md Q2 (ruled
> 2026-08-18) made it beta-critical. §13 is proposal-only: no fix ships
> under it, and its thirteen pins are `#[ignore]`d until the owner rules.
> Everything §3 ratified stands unchanged — §13 layers inside the trait
> tier and does not move a tier boundary.

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

> Still true at METHOD resolution, where it was written, and now with a
> declaration-site rule beside it: B98 (`trait-objects.md` §15.8) refuses
> an exact repeat of one `(trait, arguments, subject)` pair. It refuses
> nothing this paragraph protects — the twins never coexist in a build, so
> no pair forms; a generic subject never matches a concrete one, so the
> blanket forms no pair with a user's `Into` impl; and the OVERLAP the
> last sentence names is untouched, still declaration order, still B73's.



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

## 13. Specificity: the blanket-vs-specific design (B73)

> Status: **DRAFTED 2026-08-18, AWAITING RULING.** Proposal only — no fix
> ships under this section. §9(6) declined this question ("a specificity
> question, which §3(b) declines"); `trait-objects.md` §15.8(3) left the
> overlap to it ("Blanket-vs-specific OVERLAP stays legal, and stays
> B73's"); `spec/types.md` §5.4's implementation note says a specificity
> rule "is owed". beta.md Q2, ruled 2026-08-18, made it beta-critical:
> a wrong resolution from a clean compile is miscompile-shaped, so
> process.md §5.4's trigger (c) waits on it.

### 13.1 What the arc found that the filing did not say

B73 is filed as one bug — "a blanket trait impl beats a user's specific
one by declaration order". Probing it (§13.2) found that sentence
describes a *symptom* of two independent defects, and under-describes the
damage in three ways that matter to the ruling:

1. **The headline shape is a false rejection, not the miscompile.**
   `let b: Bar = foo.into()` with a user `impl Foo with Into<Bar>` reports
   `Expected Bar, but got Foo instead.` and exits 1 (row 1). The *silent*
   wrong answer is the unannotated call: `let s = foo.into(); print(s)`
   with `impl Foo with Into<str>` compiles clean, exits 0, and prints
   `[ 1 ]` — the raw struct — where the user's impl says `converted`
   (row 2). The emitted JS is the blanket's body verbatim
   (`function $a(self) { return __clone(self); }`); the user's `into` is
   never emitted at all.

2. **There is a second, sharper miscompile in the same family with no
   blanket anywhere in it.** Two impls of one trait at *different*
   arguments are legal by the language's own rule (`spec/types.md`
   271–275: "`impl Bag with Into<Cup>` and `impl Bag with Into<Mug>` both
   stand"), and reaching one through a generic bound picks the FIRST
   declared regardless of which the bound names. `to_baz<T: Conv<Baz>>`
   against `impl Foo with Conv<Bar>` (declared first) and
   `impl Foo with Conv<Baz>` returns a `Bar` under the static type `Baz`;
   `let z: Baz = to_baz(foo); print(z.tag)` prints `2` — an `i32` where a
   `str` was declared — clean, exit 0 (row 20). That is type confusion, a
   strictly worse failure than row 2, and it survives deleting the std
   blanket. **Removing `impl type T with Into<T>` would therefore NOT
   close trigger (c)** (§13.6 Q3).

3. **The compiler already disagrees with itself.** The identical program
   answers `1` through `foo.tag()` and `7` through
   `fun show<T: Tag>(x: T) { x.tag() }` (rows 21/22), because the analyzer
   matches candidate impls with `compare_type` (a generic subject matches
   every type) and the transformer matches them with `nominal_matches` (a
   generic subject matches nothing). **Half the compiler already
   implements "a blanket never beats a nominal impl."** Any specificity
   rule that says the same thing is making the analyzer agree with shipped
   behavior, not inventing a preference.

### 13.2 The fact table

Every row run through this worktree's `target/debug/vilan run`
(`cargo build` exit 0), in a scratch package carrying a `vilan.toml`.
"Correct" is the intuitively right answer, which §13.4's recommendation
adopts as the pinned semantics.

| # | Shape | Today | Correct |
|---|---|---|---|
| 1 | `impl Foo with Into<Bar>`; `let b: Bar = foo.into()` | exit 1, `Expected Bar, but got Foo instead.` | compiles; `Bar` |
| 2 | same with `Into<str>`; `let s = foo.into(); print(s)` | **exit 0, prints `[ 1 ]`** — silent wrong answer | prints `converted`, or an ambiguity error |
| 3 | same; `fun to_bar(x: Foo): Bar { x.into() }` | exit 1, `Expected Bar, but got Foo` | compiles |
| 4 | same; `fun to_bar<T: Into<Bar>>(x: T): Bar { x.into() }` | exit 0, `101` — **correct today** | unchanged |
| 5 | same; `Into::into(foo)` (the §3.1 disambiguator) | exit 0, `1` — the blanket | reaches the user's impl, or errors |
| 6 | user blanket `impl type T with Conv<T>` **before** `impl Foo with Conv<Bar>` | `1` (blanket) | `101` (specific) |
| 7 | the same two impls, **specific first** | `101` (specific) | `101` — order must not decide |
| 8 | a user's own `impl type T with Into<T>` beside std's | exit 1, B98 duplicate-pair refusal | unchanged |
| 9 | `impl Box<type T> with Tag` before `impl Box<i32> with Tag` | `1` (generic) | `2` (concrete) |
| 10 | the same two, **concrete first** | `2` | `2` |
| 11 | `impl Box<type T> with Tag` before `impl Box<type T: Display> with Tag` | `1` (unbounded) | `2` (bounded) |
| 12 | the same two, **bounded first** | `2` | `2` |
| 13 | `impl Box<type T> with Tag` before `impl Box<List<i32>> with Tag` | `1` (generic) | `2` (nested concrete) |
| 14 | the same two, **nested concrete first** | `2` | `2` |
| 15 | `impl Box<type T: Display>` (bound unsatisfiable here) first, applicable `impl Box<type T>` second | exit 1, `'Opaque' does not implement trait 'Display'` — false rejection | `1`; the applicable impl is used |
| 16 | blanket `impl type T with Tag { }` (declares nothing) + `impl Foo with Tag { fun tag }` | `7`, both orders | `7` |
| 17 | blanket DECLARES `tag`; `impl Foo with Tag { }` takes the trait's default | `1`, **both orders** — the default never runs | `9` (the default, via the more specific impl) |
| 18 | `impl Foo with Conv<Bar>` then `impl Foo with Conv<Baz>`; `let b: Baz = foo.conv()` | exit 1, `Expected Baz, but got Bar` | compiles; `3` |
| 19 | the same two; `let b: Bar = foo.conv()` (the FIRST) | exit 0, `2` | `2` |
| 20 | the same two; `fun to_baz<T: Conv<Baz>>(x: T): Baz { x.conv() }`, `Baz { tag: str }` | **exit 0, prints `2`** — a `Bar` under the type `Baz`; **type confusion** | prints `baz` |
| 21 | true overlap `impl type T with Tag` + `impl Foo with Tag`; direct `foo.tag()` | `1` (blanket) | `7` |
| 22 | the same program through `fun show<T: Tag>(x: T) { x.tag() }` | **`7`** — the other path already prefers the specific one | `7` |
| 23 | inherent `impl Foo { fun into(self): Bar }` beside the std blanket | `101` — inherent tier wins (§3) | unchanged |
| 24 | `fun to_bar<T: Into<Bar>>(x: T)` given a `Foo` with no `Into<Bar>` impl | exit 1, `'Foo' does not implement trait 'Into<Bar>'` — **arguments compared correctly** | unchanged |

Rows 2, 20 and 21/22 are the beta-relevant ones: a clean compile with a
wrong runtime answer. Rows 1, 3, 15 and 18 are false rejections of valid
code. Rows 8, 23 and 24 are the parts that already work and must keep
working.

### 13.3 The mechanism, as it stands

**Candidate collection** — `impl_member_candidates`
(`crates/vilan-core/src/analyzer.rs` 11278–11324). The row of impls
declaring the name comes from `implementations_by_member`
(11292–11297; written at registration, 19039–19051), is filtered by
`compare_type(subject_type, implementation.subject, …)` (11299–11304),
sorted by `declaration_order` (11321) — which is the member's entity id,
`member_id.0` (11439–11441), minted in walk order: textual within a file,
canonical module order across files, **std first and the entry file
last** (§2) — and deduped only by `member_id` (11322).

That filter is the first defect's ground. `compare_type` is
*compatibility*: a generic position is a hole and an unbounded one matches
anything, which is exactly what `trait-objects.md` §15.8 measured when it
had to reject `compare_type` as the duplicate key ("a `compare_type` key
calls std's `impl type T with Into<T>` a duplicate of every user `Into`
impl"). So `impl type T with Into<T>` (`vilan/std/src/into.vl` 5–9) is a
candidate for every receiver in the program, and — std being tier 0 — it
sorts first for every one of them.

**Ranking** — `rank_member_candidates` (analyzer.rs 11338–11369). Tier 1
takes the first candidate with `home_trait: None` (11343–11348). Tier 2
collects distinct homes and errors only when there are two or more
(11352–11362); otherwise `candidates.first()` wins (11363–11366). The
home is `member_home_trait`'s return (11376–11389): **a trait `Id`, with
the arguments discarded.** So `Into<Foo>` (the blanket, instantiated at
this receiver) and `Into<Bar>` (the user's) are *one home*, `homes.len()`
is 1, no ambiguity is raised, and declaration order decides — rows 2, 6,
18, 19.

**The bound path** — analyzer.rs 24662–24734. A `Type::Generic` receiver
walks `generic_bound_traits`, takes the first bound trait declaring the
name, and records `GenericDispatch::OnConstraint` plus the *trait id* in
`bound_dispatch_traits` (24725–24731). Arguments are used to substitute
the method's signature (24705–24718) but are **not carried into the
dispatch key**.

**Codegen re-dispatch** — `resolve_dispatch_with`
(`crates/vilan-core/src/transformer.rs` 6014–6059) prefers
`resolve_member_on_trait_impl` (6113–6137), which filters
`implementation.trait_ids.contains(&trait_id)` and
`nominal_matches(subject, type_)` and takes the first hit. Two
consequences, both measured:

- `nominal_matches` (transformer.rs 1347–1353) compares struct/enum ids
  and otherwise falls to `a == b`, so a `Type::Generic` subject never
  matches a `Type::Struct` receiver. **The std blanket is invisible
  here.** That is why row 4 and rows 21/22 pick the user's impl on the
  bound path while the analyzer picks the blanket on the direct path.
- The filter ignores `trait_args` entirely, so a bound written
  `T: Conv<Baz>` re-dispatches to whichever impl of `Conv` was declared
  first — row 20's type confusion. `resolve_member_on_type`
  (transformer.rs 7109–7130) has the same first-hit-wins shape for the
  unpreferred path.

**What already gets arguments right**, and is the proof the key is
available: B98's duplicate-impl check keys on `(trait, effective
arguments, subject)` with SAMENESS rather than compatibility, padding a
`with` clause with the trait's declared defaults and resolving `= Self`
to the subject (`trait-objects.md` §15.8); and bound satisfaction refuses
`Into<Bar>` for a type carrying only the blanket's `Into<Foo>` (row 24).
The machinery exists in two places and is simply not consulted by the
third.

**So the root cause decomposes:**

- **D1 — argument blindness.** Resolution keys on the trait id where the
  duplicate check and the bound checker key on `(trait, arguments)`.
  Causes rows 2, 5, 18, 19, 20.
- **D2 — overlap by declaration order.** When two impls genuinely apply
  for the *same* trait and arguments, nothing ranks them and
  `candidates.first()` takes registration order. Causes rows 6, 7, 9–14,
  17, 21.
- **D3 — two matchers.** `compare_type` in the analyzer, `nominal_matches`
  in the transformer. Causes rows 21/22's disagreement and, in the
  opposite direction, the accident that makes row 4 correct today.

### 13.4 The design space

#### (a) A specificity ordering

A partial order over the applicable impls; the unique maximum wins; two
incomparable maxima are an ambiguity error at the call site. "More
specific", precisely, for vilan's type language:

1. **Subject shape.** Impl A's subject is more specific than B's when B's
   subject *pattern-matches* A's subject and not conversely. That makes
   `Foo` ≻ `type T`, `Box<i32>` ≻ `Box<type T>`, and `Box<List<i32>>` ≻
   `Box<type T>` (rows 9–14) fall out of one rule rather than three
   cases — it is directional `compare_type`, which the analyzer already
   computes in both directions elsewhere.
2. **Bounds.** When the subject patterns are equal up to binder renaming,
   the impl whose binders carry a strictly stronger bound set wins:
   `Box<type T: Display>` ≻ `Box<type T>` (rows 11/12). B98 already
   compares bound sets for its sameness key ("Bounds, not identity, is
   what makes (3) work in both directions", §15.8), so this reuses a
   measured comparison rather than inventing one.
3. **Incomparable.** `Box<type T: Display>` against `Box<type U: Ord>`
   for a `Box<i32>` that satisfies both: neither subsumes the other, so
   neither wins and the call site reports an ambiguity naming both impl
   sites. This is the residue (a) does not rank, deliberately.

**Composition with §3:** specificity ranks *inside* the trait tier only.
Tier 1 (inherent) is untouched — row 23 keeps working, and B57's
"inherent beats trait, unconditionally" is not weakened. The
`AmbiguousTraits` error stays exactly what it is for two *different*
traits; specificity never rescues that case.

**What it does to std:** `impl type T with Into<T>` is the only blanket
impl in the whole tree (swept: `grep "impl type " vilan` returns one hit,
`std/src/into.vl:5`). Under (a) it loses to any user `Into` impl for that
subject and keeps applying to every type that has none. No std impl pair
becomes ambiguous — std's own bound-tiered `List` impls (`list.vl` 12,
135, 151) use distinct method names per tier, which §3(b) already
measured.

**Corpus, docs, examples:** nothing in the tree calls `.into()` and
nothing writes a `T: Into<…>` bound (the only `Into` mentions outside
`into.vl` are the `docs/std/strings.md` §Into prose, `docs/spec/types.md`
271–282's rule and its tracked note, and `docs/appendix/errors.md`
120–121's B98 wording). Zero corpus goldens can move for want of a call
site to move them. Two proposals have recorded wanting this fixed:
`variadic-generics.md` 175–196 chose a bespoke `Readable<T>` over the
more elegant `Into<Source<U>>` **because** blanket dispatch is broken, and
`trait-objects.md` §10's P18 is row 1 verbatim.

**Diagnostics** (diagnostics-standard.md): the incomparable case gets a
call-site ambiguity anchored at the member-name span (A1/A4, matching
§4's existing ambiguity and `BareTraitValue`), naming both candidates by
their *impl subject* as the user wrote it (B1) with C3 notes at the two
impl sites. There is no `Trait::member` steer available here — both
candidates are the same trait — so, per B83's "an impossible steer is
worse than no steer", the message says what ranks and what does not
rather than offering a spelling that does not exist.

**Cost:** a subsumption routine over impl subjects and bound sets, run
per call site over an already-collected candidate list (the list is
almost always length 1). It is additive to `rank_member_candidates`; no
tier moves.

#### (b) Overlap rejection (Rust's coherence)

Two impls of the same trait, at the same effective arguments, whose
subjects could both match some type are an error at the second
definition site.

**Is `impl type T with Into<T>` then legal alongside ANY user `Into`
impl?** The brief's question, and the answer is *yes for the case that
matters and no for one that exists*:

- Against `impl Foo with Into<Bar>` — **legal.** With D1 fixed the
  arguments differ (`Into<Foo>` vs `Into<Bar>`), so they are not the same
  implementation and cannot overlap. This is the same reasoning B98
  already shipped for its pair key.
- Against a *reflexive* user impl `impl Foo with Into<Foo>` — **an
  error**, and one the tree contains today:
  `b98_the_std_into_blanket_is_not_a_duplicate_of_a_user_impl`
  (`crates/vilan-core/tests/inference.rs` 47946) writes exactly
  `impl Fahrenheit with Into<Fahrenheit> { fun into(self): Fahrenheit { self } }`
  and asserts it compiles. Under (b) that pin's program stops compiling.
  That is (b)'s only measured in-tree casualty, and it is a pin written
  on purpose to hold this door open.

**The deeper objection:** the overlap that fires is between a *user's*
impl and a file in `std` the user cannot edit. The diagnostic's steer
would have to be "delete std's blanket" or "do not implement `Into`
reflexively" — a dead end at the user's own definition site, which is
precisely the failure mode B65 and B83 named. (b) also forbids outright
the "generic impl plus a specialized one" pattern (rows 9–14) with no
replacement, since vilan has no `default fn`. It is simpler to specify
and strictly stricter, and it does close D2 by construction — but it
closes it by removing the expressiveness rather than ranking it.

#### (c) The hybrid — overlap is an error unless one impl is strictly more specific

(a)'s subsumption order plus (b)'s definition-site check, with the check
suppressed whenever the order ranks the pair. Rust's specialization-lite,
and what a reader coming from Rust expects.

Arithmetically, (c) = (a) + (b), and its *only* behavioral addition over
(a) is moving (a)'s incomparable residue from a call-site error to a
definition-site error. It costs a whole-program pairwise check (cheap —
it is per member-name row, the same scan B98 already runs) and it
inherits (b)'s std-blanket problem in full: `impl Foo with Into<Foo>`
beside the std blanket is *not* ranked by specificity (the blanket is
strictly more general, so it IS ranked — the specific one wins), so
actually (c) admits the row-8 pin's program where (b) refuses it. (c)'s
residual refusals are only the genuinely incomparable pairs, of which the
tree contains zero.

### 13.5 Recommendation

**Take (a), implemented as three rules in this order, and do not take (b).
Leave (c) available as a later, purely additive layer.**

- **R1 — put the trait's effective arguments in the resolution key.**
  `rank_member_candidates`'s home (analyzer.rs 11352–11359) and the
  transformer's `resolve_member_on_trait_impl` filter (transformer.rs
  6123–6136) key on `(trait_id, effective arguments as instantiated for
  this receiver)`, computed the way B98 already computes it (arguments
  in, declared defaults padded, `= Self` resolved to the subject). This
  alone fixes rows 18, 19 and 20 — the type confusion — and turns rows 2
  and 5 from a silent wrong answer into two distinct homes. It is not a
  new idea in this codebase; it is the key two other checks already use.
- **R2 — the expected type selects among argument-distinct homes.** When
  the surviving candidates differ only by their trait arguments and
  exactly one's return type reconciles with the call site's expected
  type, that one wins; zero or two or more is an ambiguity error naming
  the homes as the receiver instantiates them (`Into<Foo>` and
  `Into<str>`). This is what makes rows 1 and 3 compile and row 2 a
  *reported* ambiguity rather than a printed struct, and it is exactly
  the missing capability `variadic-generics.md` 182–187 recorded as its
  blocker ("the annotation doesn't steer impl selection").
- **R3 — specificity ranks a genuine overlap.** For candidates sharing
  one `(trait, arguments)` home, §13.4(a)'s subsumption order picks the
  maximum; incomparable maxima are a call-site ambiguity. Fixes rows 6,
  7, 9–14, 17 and 21 — and closes D3 by making the analyzer agree with
  the preference the transformer already has (row 22).

**Three sentences of reasoning.** B73's beta-critical damage is caused by
resolution keying on a trait id where the rest of the compiler keys on
`(trait, arguments)`, so R1 is a consistency repair with two shipped
precedents rather than a new semantics, and it alone removes the type
confusion that would still exist if the std blanket were simply deleted.
Overlap rejection (b) buys strictness the tree has no demand for while
costing a shipped pin, forbidding a pattern with no replacement, and
producing a diagnostic whose only fix lives in a file the user cannot
edit. Specificity is the smaller and more honest change because half the
compiler already implements it — the transformer's `nominal_matches`
prefers a concrete impl over a blanket today (rows 21/22) — so R3 makes
one program stop having two answers instead of inventing a preference
nobody has expressed.

**What this does not do.** It does not make `impl type T with Into<T>`
unremovable — deleting it is still available as an independent
simplification (§13.6 Q3) — and it does not touch §3's tiers, §3.1's
`Trait::member` disambiguator, B98's pair key, or B84's same-block rule.

### 13.6 Questions only the owner can answer — all RULED 2026-08-18, each as recommended

**Q1 — the shape.** (a) with R1–R3, (b), or (c)? *Recommendation: (a) with
R1–R3.* (b) costs a shipped pin and a user-unreachable fix site; (c) is
(a) plus a check whose only new refusals are pairs the tree does not
contain.

**Q2 — may the expected type steer impl selection (R2)?** This is the one
genuinely new capability in the recommendation: method resolution runs
receiver-first today, and R2 lets the annotation on the left of the `=`
choose among trait instantiations on the right. It is how Rust's `.into()`
works and it is what two proposals have asked for. If the ruling is **no**,
B73's headline case becomes an unconditional *ambiguity error* at every
`.into()` on a type that has its own `Into` impl — correct, never silently
wrong, and close to unusable without a bound, because there is no
disambiguating spelling (row 5: `Into::into(foo)` reaches the blanket, and
vilan has no `<Foo as Into<Bar>>::into` form and §3.1 declined to add one).
A "no" therefore probably implies deleting the blanket as well.

**Q3 — should `impl type T with Into<T>` exist at all?** Deleting
`std/src/into.vl` 5–9 is a one-line change with no in-tree dependent —
nothing calls `.into()`, nothing writes an `Into` bound, and
`variadic-generics.md` 190–196 already recommends *against* the design
that would have depended on it. It would make rows 1–5 correct
immediately. **It would not close beta trigger (c)**: row 20's type
confusion and rows 9–14/21's order dependence involve no blanket at all,
so the miscompile stays open. If the owner wants beta unblocked sooner,
the honest sequence is R1 (which closes row 20) plus the deletion, with
R2/R3 following at ordinary priority — but that is a scheduling ruling,
not a design one, and this section does not assume it.

**Q4 — where is an unrankable overlap reported, call site or definition
site?** (a) says call site (report only what a program actually asks
for, as §3 does for trait-vs-trait); (b)/(c) say definition site (refuse
early, as §3 does for duplicate-inherent). B57 shipped both anchoring
philosophies for different rules, so precedent does not settle it.

**Q5 — row 17: what should a specific impl that takes the trait's default
outrank?** `impl type T with Tag { fun tag → 1 }` beside
`impl Foo with Tag { }` prints `1` in both orders today: the blanket is
the only candidate that *declares* the name, so the trait's default `9`
is unreachable for `Foo`. §13.4(a) says the more specific impl wins and
its member is the trait's default, giving `9`. The pin encodes `9`; it is
the least obvious row in the table and the owner may prefer `1`.

**Q6 — does R3's applicability check subsume the other tracked soundness
note?** Row 15 is a false rejection with the same root: candidate
selection ignores whether an impl's bounds actually hold, so an
unsatisfiable bounded impl declared first wins the race and then fails its
own bound check while an applicable unbounded impl sits below it.
`spec/types.md` §5.4's second implementation note ("a conditional impl's
bounds are not yet re-checked when the impl is selected through a GENERIC
bound") is the same gap seen from the other side. Making bounds part of
*applicability* would fix both; keeping them out of it leaves row 15 open.
In scope of B73, or its own item?

### 13.7 The pins

Thirteen `#[ignore]`d pins in `crates/vilan-core/tests/inference.rs`, one
per case, each encoding the §13.5 recommendation and each named for its
row. They are un-ignored by the fix, not by this section.

| Pin | Row(s) | Asserts |
|---|---|---|
| `b73_a_user_into_impl_beats_the_std_blanket` | 1 | compiles; `101` |
| `b73_an_unannotated_into_call_is_ambiguous_rather_than_silently_identity` | 2 | fails with an ambiguity naming `Into<str>` |
| `b73_an_into_call_in_return_position_reaches_the_user_impl` | 3 | compiles; `101` |
| `b73_a_trait_qualified_into_call_reaches_the_user_impl` | 5 | annotated `let b: Bar = Into::into(foo)` compiles; `101` |
| `b73_a_user_blanket_loses_to_a_specific_impl_whatever_the_order` | 6, 7 | `101` in both orders |
| `b73_a_concrete_impl_subject_outranks_a_generic_one` | 9, 10 | `2` in both orders |
| `b73_a_bounded_impl_subject_outranks_an_unbounded_one` | 11, 12 | `2` in both orders |
| `b73_a_nested_concrete_impl_subject_outranks_a_generic_one` | 13, 14 | `2` in both orders |
| `b73_an_applicable_unbounded_impl_survives_an_inapplicable_bounded_one` | 15 | compiles; `1` |
| `b73_a_specific_impl_taking_the_trait_default_outranks_a_blanket_declaration` | 17 | `9` in both orders (Q5) |
| `b73_two_impls_of_one_trait_at_different_arguments_are_both_reachable` | 18, 19 | `3` and `2` |
| `b73_a_bound_selects_the_impl_matching_its_trait_arguments` | 20 | `baz`, not `2` — the type confusion |
| `b73_a_direct_call_and_a_bounded_call_agree_on_which_impl_wins` | 21, 22 | `7` on both paths |

Rows 4, 8, 16, 23 and 24 are correct today and take no new pin: they are
already held by `b84_two_impls_of_one_trait_are_still_not_a_duplicate`,
`b98_the_std_into_blanket_is_not_a_duplicate_of_a_user_impl`, and §3's
inherent-tier pins. Every pin's program was run through this worktree's
`target/debug/vilan` first, so each records a measured "today" in its
comment rather than an assumed one — and each is non-vacuous by
construction, since today's answer differs from the asserted one.
