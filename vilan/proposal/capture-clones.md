# Pattern captures copy — closing B53's findings

> **Status: SHIPPED 2026-08-03** (backlog B53, second pass). The first pass
> (`0835c7d`, v0.23.6) made destructure and unguarded `match` captures copy;
> an adversarial review of it found one whole compilation path uncovered and
> four soundness gaps. This record covers that follow-up: what was wrong, what
> the fix is, and the two holes it deliberately leaves open with their reasons.
> The backlog's B53 entry is the index; this is the detail.

## 1. What the first pass shipped

Rule 1 (value semantics, `docs/spec/memory.md`): binding an aggregate *place*
copies it. A pattern capture binds a piece of its subject, so `let (xs, n) =
pair` and `Some(let inner) => …` must copy too — before `0835c7d` they aliased,
and growing `pair.0` showed through `xs`, a `mut` capture wrote back into the
source, and a RETURNED capture (`option.unwrap()`) handed the caller a live
alias into the option's payload.

`compute_capture_clone_sites` collects the captures under a place subject and
`compile_pattern` wraps their slot reads in `__clone`, with two elisions:

- **SHARE** — an immutable capture from a readonly-rooted subject that never
  roots a value seam. Nobody can mutate either side and the alias cannot leave,
  so sharing is unobservable. This is what keeps read-only walkers (the SSR
  `render` recursion over a view tree) from deep-copying at every level.
- **MOVE** — an `is_elidable_copy` subject: a local read exactly once (a dead
  `?`-lift temp) donates its elements rather than copying them.

## 2. The five findings

### 2.1 Only one of the two pattern-compilation paths (blocking)

`match` legs compile two ways. An unguarded leg declares its captures as real
bindings (`compile_pattern`); a guarded leg cannot — the guard reads the
captures before the leg is committed, so they are recorded as ACCESSORS into
the subject (`compile_is_pattern` → `is_bindings`) and substituted at every
reference. `Expr::Is` uses the same aliasing path for the same reason. That
path never consulted the clone-site set, so:

```vilan
if pair is (let xs, let n) { pair.0.push(9); print(xs.len()) }   // 3, not 2
if held is Some(mut v) { v.push(9) }                             // grew the option
match pair { (mut xs, let n) if n > 0 => … }                     // wrote back
```

The analyzer had already been collecting these captures — nothing consumed
them. **Fix:** `materialize_capture_clones` turns a capture that owes a copy
into a real declaration and re-points its alias at the declared name. Captures
that share or move keep their accessor, so the elisions stay free.

**Where the declaration goes decides when the copy happens**, and the two call
sites answer differently on purpose:

- For an `is` test, beside the subject temp, before the test. The subject is
  already evaluated eagerly there; `__clone(undefined)` is the cost of a
  pattern that does not match, and the branch bodies are not reachable from
  that arm anyway.
- For a guarded leg, the **leg body** — so a guard that rejects has copied
  nothing and consumed nothing, and the next leg finds the subject exactly as
  it was. The guard reads the subject's slots directly, which holds the same
  values the copy would.

  The alternative (copy first, then guard) was rejected: a guard is a decision
  procedure, not part of the leg's execution, and a rejected leg must leave no
  trace. It also has no statement slot to emit into — an else-if chain has
  nowhere to put a statement before a leg's condition, which is exactly the
  shape of the pre-existing hole in §5.

### 2.2 The conservative generic copy deep-copied resources (blocking)

The type filter read the capture's DECLARED type, and `Type::Generic(_)` is
never a resource there — so `Option::unwrap`'s `Some(let inner) => inner`
emitted `__clone` in *every* monomorphization, resource instantiations
included. `docs/spec/memory.md` R11 names `Option::unwrap(self): T` as the case
that must pass with **no copies**; instead a `mut r = o.unwrap()` produced two
resources with divergent state, each destroyed separately (`drop a n=7` then
`drop a n=1`). The parent behavior was an alias — one resource, dropped twice —
so the first pass replaced a double-drop with a *copy*, which is worse: R1 says
a resource never copies, and the copy is what makes the two drops disagree.

**Fix, and it generalizes past the filed case.** Resource-ness by containment is
a union over members, so an aggregate is a resource under a set of generic
bindings exactly when it is a resource under one of them. The analyzer records,
per capture, the constraints whose binding to a resource would make the whole
capture a resource (`resource_triggering_constraints`, reusing R11's own
`type_is_resource_with`); the transformer asks the active substitution what
those constraints were bound to and consults `Program::resource_types`. That
covers a bare `T` *and* a generic-dependent aggregate — `Wrap<T>`, `(T, i32)` —
which the narrow "is the declared type `Generic`?" reading would have left
copying resources one level up.

Everything else still copies: a scalar instantiation keeps its identity
`__clone` (pinned in the corpus bytes), an aggregate one keeps its real copy.
The carve-out is for resources only.

### 2.3 SHARE composed unsoundly with the move elision (blocking)

Each elision is sound alone. Together:

```vilan
let pair = ([1, 2], 3);
let (xs, n) = pair;   // SHARE: xs aliases pair.0
mut ys = xs;          // MOVE: xs is read exactly once, so "donate"
ys.push(9);
print(pair.0.len())   // 3
```

`const xs = $a[0]; let ys = xs;` — no copy anywhere. The move elision's premise
is that the source is a dead OWNER; a shared capture owns nothing to donate.

**Fix:** `compute_capture_clone_sites` returns its shared set and runs BEFORE
`compute_clone_sites`, and `is_elidable_copy` refuses a shared capture as a
source. The second binding therefore copies — the SHARE itself is untouched, so
the read-only walkers it exists for stay clone-free (verified: every corpus
golden is byte-identical across this change).

The pass is stratified so one traversal suffices: the SHARE decision consults
no elision, so phase 2 settles it, and phase 3's move check reads the finished
set. After phase 3 every capture either copies (owns), shares (owns nothing,
and is refused as a move source), or moves from an owning dead subject (owns) —
there is no chain to iterate.

### 2.4 The seam scan only saw syntactic places (blocking)

A capture that leaves its scope must copy even when it would otherwise share —
that is what keeps `unwrap` honest. The scan asked `place_root` directly, which
walks `Local`/`Field`/`TupleIndex`/`Index`/`Deref` and nothing else, so any form
that FORWARDS a value without being a place hid the seam:

```vilan
match held { Some(let inner) => { inner } … }         // braced leg
fun pick(pair, first) { let (a, b) = pair; if first { a } else { b } }
```

Both restored the returned-capture leak. **Fix:** the scan collects tail leaves
first (`collect_tail_leaves` — blocks, `if` arms, `match` legs, recursively),
which is the same walk the view-escape checks already use, and takes the place
root of each.

### 2.5 `mut [a, b]` meant two different things

`mut` at a binder applies to every binding under it. The analyzer's
`set_pattern_bindings_mutable` recurses tuples AND arrays; the match/`is`
grammar's `apply_binding_mutability` recursed tuples only — its own comment
called the array arm "a reproduced quirk". So `mut [a, b] = arr` bound `a`/`b`
mutable while `match arr { mut [a, b] => … }` and `if arr is mut [a, b]` bound
them immutable, and writing through them said *cannot mutate immutable 'a'*.

No deliberate reason was recorded for the divergence and none is defensible
under H9's mut-parameter semantics (one keyword, one meaning, at every binder).
**Decided: the array arm recurses**, matching the analyzer twin. Both spellings
now bind mutably and copy, pinned either side.

## 3. Where the code lives

| piece | site |
|---|---|
| the pass (candidates, seams, SHARE, MOVE) | `analyzer.rs::compute_capture_clone_sites` |
| per-instantiation resource question | `analyzer.rs::resource_triggering_constraints`, `Program::resource_types` |
| seam scan through value-forwarding forms | `analyzer.rs::insert_seam_roots` |
| move elision refuses a shared capture | `analyzer.rs::is_elidable_copy` |
| the declared-binding path | `transformer.rs::compile_pattern` |
| the alias path (`is`, guarded legs) | `transformer.rs::materialize_capture_clones` |
| per-emission copy decision | `transformer.rs::capture_copies` |
| `mut` at an array binder | `parsing.rs::apply_binding_mutability` |

## 4. Pins

Fourteen new pins in `crates/vilan-core/tests/inference.rs`, each proven
non-vacuous. Ten were red against the shipped v0.23.6 as standalone programs
before a line was written; the other four are guarded by planting the fix's
own mechanism back out and watching them fail:

| pin | finding | proof |
|---|---|---|
| `an_is_capture_does_not_alias_the_subject` | 2.1 | red at HEAD (3, want 2) |
| `a_mut_is_capture_does_not_write_back_to_the_subject` | 2.1 | red at HEAD (3/3) |
| `a_guarded_match_capture_does_not_alias_the_subject` | 2.1 | red at HEAD (3/3) |
| `a_rejecting_guard_leaves_the_subject_untouched` | 2.1 | red at HEAD (3/3) |
| `a_generic_capture_moves_a_resource_instantiation` | 2.2 | red at HEAD (7/1) |
| `a_moved_resource_instantiation_destroys_one_value` | 2.2 | red at HEAD (`n=7` then `n=1`) |
| `a_generic_aggregate_capture_moves_a_resource_instantiation` | 2.2 | red at HEAD (7/1) |
| `a_generic_aggregate_capture_copies_a_data_instantiation` | 2.2 | planted (carve-out widened to every generic) |
| `a_shared_capture_is_not_an_elidable_move_source` | 2.3 | red at HEAD (3, want 2) |
| `a_mut_capture_from_an_immutable_subject_copies` | 2.3 | planted (share elision ignores capture mutability) |
| `a_braced_leg_capture_does_not_leak_an_alias` | 2.4 | red at HEAD (3/3) |
| `a_conditionally_returned_capture_does_not_leak_an_alias` | 2.4 | red at HEAD (3/3) |
| `a_mut_array_binder_in_a_match_stamps_its_elements` | 2.5 | red at HEAD (compile error) |
| `a_mut_array_binder_in_an_is_test_stamps_its_elements` | 2.5 | red at HEAD (compile error) |

The corpus fixture `vilan/test/capture-clones.vl` pins the emitted SHAPES in
bytes — the three the first pass never covered included. Its header used to
name `sum_over` as the share pin, which was wrong: those captures are `i32`,
rejected by the type filter long before any elision is consulted. The fixture
now carries a genuine aggregate share (`total_width`), the same share through
a guarded leg (`guarded_width`), a guarded leg whose copy sits INSIDE the body
after the guard (`first_or_guarded`), and an `is` capture that copies
(`grow_first`). Planting "the alias path copies everything" and "`compile_pattern`
copies everything" each turns the golden red.

## 5. Two holes left open, on purpose

> **Update 2026-08-04 (B59):** the second one below is closed. `Expr::Match`
> now compiles each leg into its pieces (pattern test, prelude, guard test,
> body) and picks the shape once every guard has been walked: a match where no
> leg needs a prelude keeps the else-if chain byte for byte, and one where any
> leg does is emitted as a flat sequence of tests with a `matched` flag standing
> in for the `else`s, each leg's slot the body of its own pattern test. A guarded
> leg's copies are materialized into that slot AHEAD of the guard whenever the
> guard hoists anything or reads a copy — so the guard and the body see the same
> binding — and stay at body entry otherwise, which is what keeps the plain-guard
> goldens (`guarded_width`, `first_or_guarded`) unchanged. Five pins, four of
> them red against the pre-fix tree; the fifth
> (`a_guard_that_reads_a_copied_capture_reads_the_copy`) guards the new
> reads-a-copy condition and is proven by planting it out.

- **A moved-from `Option` is still readable, and still destroyed** —
  **CLOSED 2026-08-04 (B60); see `affine-moves.md` for the record.**
  `o.unwrap()` consumes `self`, but the affine checker did not treat the call
  as a move of `o` — `o.is_some()` afterwards compiled clean, and `o`'s
  scope-end teardown still fired, so the payload was destroyed twice. This
  predated B53 in both directions: before `0835c7d` it was a double drop of
  one value, after it a drop of each of two copies, and then a double drop of
  one value again. The fix was not a new move system: `unwrap` was declaring
  a LOANED receiver (`self`) while moving the payload out of it, so the
  correction is `unwrap(own self)` plus a checker rule making a consumed loan
  an error at all. `a_moved_resource_instantiation_destroys_one_value` now
  asserts the single `drop a n=7`.
- **A guard that needs a hoisted statement emits a dangling reference.**
  `compile_is_pattern`'s guarded-leg arm walks the guard into a `guard_block`
  that is never emitted, because an else-if chain has no statement slot before
  a leg's condition. Any guard needing a temporary — an `is` test, a `?` lift,
  a nested match — drops it: `if ($c[0] === 0)` with no `$c`, a runtime
  `ReferenceError`. Pre-existing (verified against v0.23.6), found while
  pinning the guarded-leg copy. Pinned `#[ignore]`d as
  `a_guard_that_needs_a_temporary_emits_it`; the fix is to emit guarded legs as
  nested ifs, which is the same restructuring §2.1's ordering discussion runs
  into and deserves its own slice.

## 6. B81 — the alias path also READS late, 2026-08-06

> The fourth gap of §2's review, the "non-place seam" family, at the shape it
> actually reaches: a subject that is a **view**. §2.1 taught the alias path to
> COPY what it captures and stopped there; it left the same path READING late.
> Two seams, both closed here. Naming update: `materialize_capture_clones` is
> now `materialize_captures` (it materializes non-copies too), and §3's row for
> it reads against that name.

### 6.1 The unstated premise

`compile_is_pattern` records each capture as an accessor into the subject temp
(`const $a = <subject>`) and substitutes it at **every reference**, so the slot
is re-read wherever the capture is used. That is faithful only under a premise
nothing ever wrote down: **the subject temp is a stable snapshot of the
subject's value for the leg's lifetime.**

Against an owned place it holds, and for a reason specific to how places
change — an assignment **rebinds**. `feed = Feed::Ready(..)` installs a *fresh*
aggregate and leaves `$a` holding the old one, so a deferred read still sees
the pre-assignment value. Through a view it does not hold at all: a write
through a view is an **in-place mutation of the very object `$a` aliases**,
because that is how the write reaches the caller (`Object.assign(self, ..)`).
Every deferred read in the leg then returns post-write state:

```vilan
fun step(&mut self): Option<str> {
    if self is Feed::Ready(let items, let at) {
        self = Feed::Ready(items, at + 1);
        Some(items[at])            // indexed with at + 1
    } else { None }
}
```

`items` was already correct — an aggregate, so §2.1's copy declared it eagerly.
`at` is an `i32`: it owes no copy, so the type filter dropped it before any
elision was consulted and it kept its accessor `$a[2]`. Two `step`s over
`Ready(["a","b","c"], 0)` printed `b`, `c`.

The diagnosis came from the path that was *right*: an UNGUARDED `match` leg
prints `a`, `b` on the identical program, because `compile_pattern` declares
every capture as a real `const` at leg entry. Same subject, same view, same
write — different pattern-compilation path. So the defect was never the view;
it was the alias path's timing, which the view merely makes reachable.

### 6.2 The rule

> A capture from a subject rooted in a **writable view** is **materialized** —
> read once, into a real declaration, at the match — whatever its type.

Two independent questions, and separating them is the whole of the fix:
**whether a capture COPIES** is settled by the capture's own type (§2.2's
filter, unchanged), **when it is READ** by the subject's. `Program::
materialized_captures` answers the second; `capture_clone_sites` still answers
the first, and one statement carries both — `__clone` appears only when the
copy is owed.

That separation is what keeps both elisions intact. A SHARE materializes
*without* `__clone`, so the alias it exists to preserve is preserved — only the
slot read is frozen — and read-only walkers stay linear. The predicate asks
about **writability** for the same reason: nothing can be written through a `&`
view, so its temp is a snapshot again and `&self` methods keep their accessors
verbatim. Widening it to every view is a byte-visible regression, pinned as
such (`capture-clones.vl::width`).

### 6.3 The resource shape

R1 forbids the copy and B65 forbids inventing one — "there is no user-facing
copy spelling in vilan to name", and `x is Some(let r)` is *always* a loan
whatever the subject's form (`affine-moves.md` §9.1). So a resource capture is
materialized bare: `const c = $a[1]`, which fixes **which value is loaned**
without minting a second owner. It is not an error, and the reason is the
decision order — the PLACE-subject twin of the same program accepts it and
reads the pre-assignment payload, so rejecting it through a view would make the
two paths differ in the opposite direction. B62's leg teardown is unaffected:
`capture_drop_nodes` reads the alias table after materialization, so it finds
the declared name and destroys the same value it always did.

### 6.4 The second seam: `*view` was not a place at all

`is_place_expr` excludes `Expr::Dereference`, so a `*v` subject collected **no
capture candidates** — §2's rule was missing wholesale for that spelling, and
even the aggregate captures aliased outright (`__at($a[1], $a[2])`, no
`__clone` anywhere). A dereference is a place by every test that matters here;
`place_root` and `readonly_root` both walk through it. The capture pass now
asks `is_capture_subject_place`, which is `is_place_expr` plus `Dereference`.
`is_place_expr` itself is left alone: rule 2's binding pass reads it against
rule 3 (`assignment_target_is_view` — a forwarded view stays the same view, on
purpose), a different question from the one a pattern subject asks.

This half reaches past the alias path, because the capture pass gates
`Expr::Destructure` on the same predicate: `let (xs, n) = *view` never copied
either, and growing the element through the view grew the capture. Nothing to
do with reading late — plain aliasing, §1's original bug, surviving in the one
spelling the predicate could not see.

### 6.5 Pins, goldens, and what is left open

Thirteen pins in `inference.rs`, covering the value / List / resource payloads,
nested patterns, guarded legs, unguarded `match`, a `let` destructure, both
write orders, `&mut self`, a `&mut` parameter, a `*view` local, the
`mut`-parameter twin, the place twin, and the `&self` line. Non-vacuity by three plants: materialization out
(8 red), the deref widening out (2 red), and the copy out (1 red, on the pin
that carries both shapes). The four that stay green under every plant are
exactly the four that pin UNCHANGED behavior — the `mut`-parameter twin, the
unguarded leg, the place-subject resource, and `&self`. `capture-clones.vl` gains `step`, `width` and
`viewed_guarded` — the byte pin for the materialized declarations, the
accessors that must NOT appear under `&self`, and B59's placement under the
new rule (a guard reading a materialized capture takes the prelude shape). The
golden's pre-existing bytes are unchanged; the fix moved no corpus program,
because none had the shape.

**Left open — the place path has the same seam, one write-form over.** A
component write to an owned place mutates in place too, so the alias path's
deferred read is wrong there as well:

```vilan
mut t = (7, 3);
if t is (let a, let b) { t.1 = 99; print(b) }   // 99, want 3
```

Not widened into this arc on purpose. The rule above is *total* for its class —
a writable view has no shape where its temp is a snapshot — whereas the place
case needs either an alias analysis or materializing every capture
unconditionally, and the latter moves goldens across the corpus and turns B59's
placement question on for every guarded leg. Filed rather than patched.
