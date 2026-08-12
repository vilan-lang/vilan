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
match pair { (mut xs, let n) if n > 0 => …, _ => … }             // wrote back
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
>
> **Update 2026-08-10 (B115):** neither shape kept a FINAL leg's guard —
> the line that runs after the shape is chosen dropped the last leg's
> test, prelude and guard together, on an exhaustiveness proof a guarded
> leg never carries. §11 has the record.

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

## 7. B88 — the place path's own write form, 2026-08-07

> §6.5's open item, closed. Same seam, same path, one write-form over. The
> §6.1 diagnosis holds up exactly: an owned place's *whole-binding* assignment
> rebinds and leaves the temp a snapshot, so B81 could stop there — but that
> is one write form out of several, and the rest of them do to an owned place
> precisely what a view's assignment does.

### 7.1 The shapes, measured

The filed repro is one of seven. Probed against the pre-fix tree (`next` @
`d5de163`, B81 included), each an `is`/guarded leg over an owned place with a
scalar capture, each wanting 3:

| shape | write | before |
|---|---|---|
| tuple component | `t.1 = 99` | 99 |
| subject under a field | `h.pair.1 = 99` | 99 |
| fixed-array element | `marr[1] = 99` | 99 |
| indexed subject | `rows[0].1 = 99` | 99 |
| nested component | `n.0.1 = 99` | 99 |
| `&mut self` method | `counter.bump()` | 99 |
| write through a `&mut` of the subject | `vv.1 = 99` | 99 |

Seven neighbours were *already* right, and each is pinned so the fix cannot
move it: reading the capture BEFORE the write (the accessor has nothing to
observe yet, which is what makes the two reads of one binding disagree), a
whole assignment (`t = (1, 2)`), a whole FIELD assignment (`he =
E::Pair(..)`, which rebinds the property), a write to a DISJOINT field of the
same root, an unguarded `match` leg, a `let` destructure, and an aggregate
capture (B53 copies it eagerly, so there is no seam to reach).

### 7.2 The two candidates, measured before choosing

§6.5 named both. Each was implemented far enough to rebuild the whole corpus
and run the analyzer gate.

| | goldens moved | analyzer gate | probe shapes correct |
|---|---|---|---|
| **(a)** materialize every place-subject capture | **6** | 1810 pass | 13 / 13 |
| **(b)** materialize on an in-place write set | **0** | 1810 pass | 13 / 13 |

Both are *correct*. What separates them is what (a) costs, and the diffs say it
plainly — the six are not six added copies:

- `capture-clones.js` — **`width` regresses**, which is the case §6.2 widened
  the predicate to WRITABILITY to protect and pinned in bytes for exactly this
  reason. Under (a) a `&self` walker's captures stop sharing and start
  declaring. `guarded_width` and `grow_first` move too.
- `match-patterns.js`, `resource_take.js`, `capture-clones.js::guarded_width`
  — three programs leave the else-if chain for B59's flat `matched`-flag
  sequence, because a guard that reads a capture now reads a *declaration*.
  That is §6.5's "turns B59's placement question on for every guarded leg",
  observed rather than predicted, and it duplicates the capture's declaration
  into every leg that binds one (`const x = $e; … const x2 = $e;`).
- `equality.js`, `generic-equality.js`, `json-roundtrip.js` — plain additions,
  all on `&self`/immutable subjects that owe nothing.

So (a) buys totality by taking back a shipped elision and restructuring
unrelated matches. **(b) ships.**

### 7.3 The rule

> A capture is materialized — read once, at the match — when the storage its
> subject names can be mutated **in place** while the leg is live.

One question, two arms, and the asymmetry is §6.1's own finding rather than a
special case:

- Through a **writable view** root, every write is in place by construction —
  that is how a write through a view reaches the caller at all. The arm needs
  no write to be found, and stays exactly as §6.2 shipped it.
- Through an **owned place** root, a whole-binding assignment rebinds and every
  other form does not. So the arm asks whether the program contains an in-place
  write rooted there: a **component** assignment (`Field` / `TupleIndex` /
  `Index` / `Deref` target), an explicit **`&mut`**, or a **`&mut`-bound
  argument** — the receiver included, and every place argument of an
  unresolvable callee.

Those are the three forms `collect_written_roots` already records for rule 2's
SHARE elision; the pass now returns them split (`WrittenRoots { any, in_place
}`) rather than collecting them twice. `any` keeps answering the SHARE
question — *may a capture alias this subject at all* — unchanged, and
`in_place` answers this one.

### 7.4 Why the question is asked at the ROOT

An arm-scoped write-set walk — "can the body of this arm write a component of
this subject" — is **not soundly implementable** on its own, and the
counterexample is two lines:

```vilan
mut vt = (7, 3);
let vv = &mut vt;
if vt is (let a, let b) { vv.1 = 99; print(b) }
```

The write inside the arm has place root `vv`. Nothing in the arm mentions `vt`.
To connect them an arm walk would need view-alias tracking, which is the
whole-program assumption §6.5 was worried about. The root question does not
need it: minting the second name is `&mut vt`, itself one of the three recorded
forms, so the root is where the connection is already visible. Calls are sound
for the same pre-existing reason — a `&mut`-bound argument is recorded, and an
unresolvable callee (dispatched, generic) conservatively counts every place
argument.

The cost is coarseness in the other direction: a write to a *disjoint* field of
the same root (`h.tag = 1` under a subject of `h.pair`) materializes a capture
that never needed it. That changes no answer, and it is the same granularity
rule 2's SHARE elision has always used. Pinned as
`a_disjoint_field_write_leaves_a_sibling_subject_correct`.

### 7.5 Doctrine per payload shape

Unchanged from §6.2/§6.3 — that is the point, and the twins are what check it:

- **Values** materialize. A scalar read IS the copy, so the declaration alone
  fixes the timing (`an_is_capture_from_a_component_written_place_reads_the_
  prematch_value`).
- **Aggregates** copy, per B53 §2.2, and were already eager because of it.
  `both_capture_shapes_survive_a_component_write_to_the_place` is the place
  twin of §6's `both_capture_shapes_survive_an_in_place_write_through_the_view`
  — same two shapes, same two component writes, same answers.
- **Resources** materialize **BARE**: `const c = $a[0]`, no `__clone`. R1
  forbids the copy and B65 forbids inventing one (`affine-moves.md` §9.1), so
  the declaration fixes *which value is loaned* without minting a second owner.
  `a_resource_capture_from_a_component_written_place_loans_the_prematch_payload`
  asserts both halves — the value (1, not 6) and the absent `__clone`. B62's
  leg teardown is unaffected for §6.3's reason: `capture_drop_nodes` reads the
  alias table after materialization.

### 7.6 Pins and non-vacuity

Fifteen pins in `inference.rs`, four plants:

| plant | red |
|---|---|
| the place arm removed (back to §6.2's condition) | 11 |
| a whole-binding rebind counted as an in-place write | 1 (`a_whole_assignment_to_the_subject_still_leaves_its_captures_aliasing`) |
| the explicit `&mut` dropped from the in-place set | 1 (`a_write_through_a_mut_view_of_the_subject_does_not_reach_its_captures`) |
| the `&mut`-bound argument dropped from the in-place set | 1 (`a_mut_self_method_call_does_not_reach_a_capture_of_its_receiver`) |

A fifth plant (B53's capture copy removed) takes the two-shape pin to `4\n4\n`,
which is what makes it red on both axes — 12 without the materialization, 4
without the copy. The three pins green under every plant are exactly the three
that pin UNCHANGED behavior: the unguarded leg, the `let` destructure, and the
disjoint field write.

**No existing corpus golden moved.** Not by luck: `capture-clones.vl`'s
place-subject functions (`grow_first`, `sum_over`, `total_width`,
`guarded_width`, `first_or`, `first_or_guarded`) all take readonly or
never-written subjects, so none is in the in-place set, and §6's viewed trio
(`step`, `width`, `viewed_guarded`) rides the untouched view arm.

`capture-clones.vl` itself gains three functions, the way it did in §2 and §6
— the fixture is where the emitted SHAPES are pinned in bytes, and a runtime
pin cannot see a materialization that lands in the wrong place and still
prints the right number. `place_component` and `place_rebound` are the same
function twice, differing only in the write, so the byte diff between them IS
the rule: the component-write version declares `const weight = $s[1]`, the
rebinding one inlines `$t[1]`. `place_guarded` pins B59's placement on the
place path. The golden moved **additively** — every pre-existing byte
unchanged, including the temp names — and planting the place arm back out
turns exactly those two functions red (`place_rebound` stays green, as it
must).

### 7.7 Bycatch — a `borrows` CALL subject is §6.4 all over again

Found while scoping this arc, verified, **not fixed here**. A method that
returns a `&mut` projection (`borrows self`) hands the pattern a subject that
aliases the receiver's storage — but the expression is a **call**, and
`is_capture_subject_place` admits `Local`/`Field`/`TupleIndex`/`Index`/`Deref`
and nothing else. So that subject collects **no capture candidates at all**,
exactly as `*view` did before §6.4, and both rules are missing at once:

```vilan
struct Holder { pair: (i32, i32) }
impl Holder {
    fun slot(&mut self): &mut (i32, i32) borrows self { &mut self.pair }
}

mut h = Holder { pair = (7, 3) };
if h.slot() is (let a, let b) { h.pair.1 = 99; print(b) }   // 99, want 3
```

```vilan
mut g = Holder2 { cells = ([1, 2], 3) };
if g.slot() is (let xs, let n) { g.cells.0.push(9); print(xs.len()) }  // 3, want 2
```

The emitted bytes name the defect outright: `const $c = slot2(g); … $c[0]` —
no `__clone` anywhere, so the aggregate capture aliases the receiver's element
(B53 §1's original bug), and the scalar re-reads the mutated slot (§6.1's).
Pinned `#[ignore]`d as `a_borrows_call_subject_copies_its_captures` and
`a_borrows_call_subject_reads_the_prematch_value`.

Not widened into this arc for the reason §6.5 gave for this one: the fix is a
different predicate (which calls return views — `Function::borrows` /
`returns_mut_view` already answer it) reaching a different set of programs, and
it deserves its own measurement rather than a rider on this one. An owned
call result needs no rule: nothing else names it.

## 8. B94 — the doctrine leaves the capture pass, 2026-08-07

> Not a capture finding, recorded here because it is the same doctrine and the
> §7.7 arc's sibling. B81/B88 said a capture must not be able to tell whether
> its subject is reached through a view or through the place the view names.
> B94 is that sentence one layer down, about the WRITE rather than the read: a
> write through a view must not be able to tell either. Ruled 2026-08-07,
> shipped in the same lane as §9. The rule and its reasoning live in
> `destruction.md` §4 R2; this note records only what the two arcs share.

The shape is §6.1's, inverted. §6.1's premise was that a subject temp is a
stable snapshot, which holds through a rebind and fails through a view because
**a write through a view is an in-place mutation of the pointee**. R2's
implementation rested on the mirror premise — that the body doing the write
owns what it overwrites — which holds for a place and fails through a view for
the *same* reason: the value being clobbered belongs to a binding in another
frame entirely. Both premises were unstated, both were true of exactly one
spelling, and both were found by asking the owned twin what it answers.

The B88 measurement discipline did not apply and was not paid. §7.2 measured
two candidate predicates because both were correct and the choice was about
cost; here the ruling fixed the predicate (the loan drops what it overwrites)
and the only open question was its reach, which is a shape enumeration rather
than a corpus tradeoff. Eleven runtime pins, one byte pin for the drop/write
order, four plants; `resource.vl` gains `view_overwrite`, `refill`,
`view_writes` and `loaned`, the last being the byte proof of the OTHER half —
a `&` local of a resource takes no teardown of its own.

The two halves are worth naming together, because one filter serves both.
References are transparent, so `&mut Holder` *is* `Holder`, and a resource
binding is a resource binding whether it owns or borrows. The planner read that
set as "owners" and got both answers wrong in opposite directions: a loan was
excused from destroying what it overwrote, and charged for destroying what it
merely borrowed. `ResourceOwnership::owned_bindings` is the set minus the
loans, and the two bugs close on the one line.

**Left open**, on §6.5's standing reason: a COMPONENT write over a resource
(`slot.held = Holder::Empty`) destroys nothing, on an owned place and through a
view alike. R2 is written about a binding and R5 about reading and moving a
field; writing over one falls between them. A different predicate over a
different set of programs — filed, pinned `#[ignore]`d, not ridden in.
**CLOSED 2026-08-07 — §10.**

## 9. B97 — the third subject spelling, 2026-08-07

> §7.7's bycatch, closed. A `borrows` CALL hands the pattern a subject that
> names the receiver's storage, and `is_capture_subject_place` admitted
> `Local`/`Field`/`TupleIndex`/`Index`/`Deref` and nothing else — so the
> subject collected **no capture candidates at all** and both rules were
> missing at once, exactly as §6.4 found for `*view`. §7.7 declined to widen
> §7's arc into it and asked for its own measurement; this is that measurement.

### 9.1 The shapes, measured

Fourteen probes against the pre-fix tree (`next` @ `bb54150`, B94 included).
Each is a pattern over a `borrows`-returning call with a write in the leg;
the two filed repros are the first two rows.

| shape | subject | before | want |
|---|---|---|---|
| scalar capture, `&mut` projection | `h.slot()` | 99 | 3 |
| aggregate capture, `&mut` projection | `g.slot()` | 3 | 2 |
| scalar capture, **`&` projection** | `h.peek()` | 99 | 3 |
| free function, `borrows h` | `slot(&mut h)` | 99 | 3 |
| guarded `match` leg | `h.slot()` | 99 | 3 |
| unguarded `match` leg, aggregate | `g.slot()` | 3 | 2 |
| `let` destructure, aggregate | `g.slot()` | 3 | 2 |
| **chained** `&mut` of `&mut` | `o.inner_mut().slot()` | 99 | 3 |
| **chained** `&` of `&mut` | `o.inner_mut().peek()` | 99 | 3 |
| resource payload | `holder.view()` | 1 (correct) | 1 |

Four neighbours were already right, and each is pinned so the fix cannot move
them: an unguarded leg's *timing* (`compile_pattern` declares at leg entry —
only the COPY was owed), a `let` destructure's timing (same reason), a leg with
no write at all, and an OWNED call result (`fresh_pair()`), whose elements have
no second owner.

One shape is **not** B97's and is filed separately: `fun make(&self): (i32, i32)
{ self.pair }` returns the field's storage uncopied, so `let p = h.make();
h.pair.1 = 99` shows through `p`. That is rule 1 at the RETURN seam, not the
capture pass; §7.7's "an owned call result needs no rule: nothing else names
it" is true of the capture pass and false of that function's own body.

### 9.2 The candidates, measured before choosing

Each was implemented far enough to rebuild the whole corpus and run the
analyzer gate. "Shapes" counts the eleven pinned answers above.

| | goldens moved | analyzer gate | shapes correct |
|---|---|---|---|
| **(a)** admit every view-returning call; both write arms | **2 — one a SEMANTIC BREAK** | — | 11 / 11 |
| **(b)** (a), plus: a capture that IS a view never copies | **0** | 1879 pass | 11 / 11 |
| **(c)** admit only `&mut`-returning calls | 0 | — | 10 / 11 |
| **(d)** (b) without the write-set root arm | 0 | — | 10 / 11 |

**(a) is wrong, and the corpus is what said so** — this is the measurement
earning its keep rather than confirming a guess. Admitting `borrows` calls
newly reaches `Option<&mut T>` returns, whose `Some(let v)` capture *is a
view*. References are transparent, so `&mut Inner` is a cloneable aggregate by
every type test in the pass, and B53's copy fired on it: `option-view.mjs`'s
`const v3 = __clone($e[1]); v3[0] = 77` writes the copy, and the fixture's own
output changed from `77` to `1`. `arena.mjs` moved too — a read-only recursive
walker that began deep-copying its node at every level, the exact regression
§6.2's writability predicate exists to prevent, in a new place.

So the rule gains one clause, and it is not a special case: **a view never
copies**, for the reason a view exists. Materialization still applies to it —
freezing WHICH view is read changes no aliasing, the same argument §6.2 makes
for the SHARE elision.

**(c) and (d) are cheap and incomplete**, each in one direction. (c) keys
candidacy on `returns_mut_view`, which is one of the two things §7.7 named, and
leaves a `&` projection's late read broken — the receiver can still be written
under its own name while the leg is live, and the temp aliases the receiver's
storage whether or not the *view* is writable. (d) keeps candidacy and drops
the root arm, which is the same loss by a different route. Neither costs a
golden, and neither is the rule.

**(b) ships.**

### 9.3 The rule

> A pattern subject that is a **view-returning call** collects capture
> candidates: it names the storage of the arguments the callee projects, not
> storage of its own. Both write questions are then asked of **those
> arguments** rather than of the subject expression.

`capture_subject_places` is the one line: for a place or a `*view`, the subject
itself; for a `borrows` call, its projected argument places, read at the call
site from `Function::borrows`. §7.4's reason that a ROOT walk needs no alias
analysis holds here unchanged, and the task's own hint is why — **the receiver
is right there in the call**. Nothing has to be tracked to connect the subject
to the storage a write can reach.

Materialization then has the two arms it has everywhere else, and both are
pinned:

- **Writable-view arm** (B81): a `&mut` projection is a writable view by
  construction, so the arm needs no write to be found. It is not subsumed by
  the root arm, and the shape that proves it is the CHAIN: `o.inner_mut()
  .slot()` has a call for a receiver, and a call has no place root, so the root
  arm has nothing to ask about. Read one level up as well — `o.inner_mut()
  .peek()` returns `&`, yet the storage it names is writable because what it
  was projected from is.
- **Root arm** (B88): otherwise, whether a recorded in-place write reaches a
  projected argument's root. `h.peek()` with `h.pair.1 = 9` in the leg is the
  case, and it is what (c) and (d) both get wrong.

The SHARE elision is deliberately NOT extended: `share_subject_is_stable` asks
`place_root`, which is `None` for a call, so an immutable aggregate capture
from a `borrows` call copies rather than sharing. That is the conservative
direction, it moved no golden, and widening it is an optimization with its own
seam question (§2's `unwrap` leak) rather than part of this fix.

### 9.4 Doctrine per payload shape

Unchanged from §6.2/§6.3/§7.5 — which is the point, and the twins are what
check it. Each is the third member of a trio that now spans place, view, and
call:

- **Values** materialize (`a_borrows_call_subject_reads_the_prematch_value`).
- **Aggregates** copy (`a_borrows_call_subject_copies_its_captures`), and both
  shapes together in `both_capture_shapes_survive_a_write_through_a_borrows_
  call_subject` — the third member of the pair §7.5 names.
- **Resources** materialize **BARE**, no `__clone`
  (`a_resource_capture_from_a_borrows_call_subject_loans_the_prematch_payload`
  asserts both halves: the value, and the absent copy).
- **Views** — new here, because this is the path that reaches them — neither
  copy nor lose their alias.

### 9.5 Pins and non-vacuity

Twenty-two pins in `inference.rs`, five plants:

| plant | red |
|---|---|
| the call arm removed from `is_capture_subject_place` | 11 |
| the view-capture filter removed | 1 (`a_wrapped_view_capture_over_a_borrows_call_is_not_copied`) |
| the writable-view arm removed for calls | 2 (both chains) |
| the projected-receiver recursion removed | 1 (`a_readonly_projection_of_a_writable_one_reads_the_prematch_value`) |
| the root arm narrowed back to the subject place | 1 (`a_readonly_borrows_call_subject_materializes_when_a_write_reaches_the_receiver`) |

The pins green under every plant are exactly the ones that pin UNCHANGED
behavior: the owned call result, and the leg with no write in it.

`capture-clones.vl` gains `called_component`, `called_readonly` and
`owned_call`, the way it did in §2, §6 and §7 — the fixture is where the
emitted SHAPES are pinned in bytes, and a runtime pin cannot see a
materialization that lands in the wrong place and still prints the right
number. `called_component` is `place_component` with `cell.slot()` for `cell`,
so the byte diff between them IS the claim that the paths are
indistinguishable: `const cells = __clone($x[0]); const weight = $x[1];`, both
declared, one copied. `owned_call` keeps its accessors. The golden moved
**additively** — every pre-existing byte unchanged. **No other corpus golden
moved.**

## 10. B99 — the doctrine one projection down, 2026-08-07

> §8's open item, closed. Not a capture finding either, recorded here for the
> reason §8 gave: it is the same doctrine, asked of the same seam family. §8
> said a write through a view must not be distinguishable from a write to the
> place the view names. B99 is that sentence about the TARGET's shape rather
> than about its root — a write over a component must not be distinguishable
> from a write over a binding. The rule and its reasoning live in
> `destruction.md` §4 R2; this note records only what the arcs share.

### 10.1 The shapes, measured

Eleven probes against the pre-fix tree (`next` @ `45f5d66`, B94 included). Each
overwrites a resource-typed place and expects the outgoing value's destructor
to print before the write's own marker.

| shape | write | before |
|---|---|---|
| struct field of an owned place | `slot.held = Holder::Empty` | leaked |
| nested component | `o.inner.held = Holder::Empty` | leaked |
| **through a `&mut` view** | `s.held = Holder::Empty` | leaked |
| tuple component | `pair.1 = Holder::Empty` | leaked |
| element of a fixed array | `arr[0] = Guard { .. }` | leaked |
| inside a `match` arm | `slot.held = Holder::Empty` | leaked |
| a bare resource field (no enum) | `slot.guard = Guard { .. }` | leaked |
| same-width replacement | `slot.held = Holder::Full(..)` | leaked |
| element of an inferred `List<Guard>` | `arr[0] = Holder::Empty` | leaked |
| **data component** (negative) | `slot.count = 5` | correct |
| **`&mut` of the component** (B94) | `let v = &mut slot.held; v = ..` | correct |

The last two were already right and are pinned so the fix cannot move them. The
`&mut slot.held` neighbour is the whole finding in one program: minting a name
for the component and writing through it destroyed the outgoing value, and
writing the component directly did not — two spellings of one write,
disagreeing.

### 10.2 The candidates, measured before choosing

Each was implemented far enough to rebuild the whole corpus and run the
analyzer gate.

| | goldens moved | analyzer gate | shapes correct |
|---|---|---|---|
| **(a)** the component's own type decides, root-agnostic | **0** | 1930 pass | 11 / 11 |
| **(b)** (a) restricted to an OWNED root | 0 | 1930 pass | 9 / 11 |
| **(c)** (a) without `Index` (`Field`/`TupleIndex` only) | 0 | 1930 pass | 10 / 11 |

**(b) is the interesting refutation**, and it is §7.2's lesson again: the
narrowing costs the view spelling — the exact indistinguishability §8 shipped —
and it answers the wrong question in *both* directions. A `&mut Slot` root is a
resource-typed binding (references are transparent), so a root test admits the
view unless it also filters loans; and an inferred `List<Guard>` root is not
classified a resource at all, so a root test drops an element write that the
component's own type answers plainly. The root is a proxy for a question the
projection already answers.

**(c) is cheap and incomplete.** `Index` is the only remaining component
spelling, and it is not hypothetical: a fixed array of resources is the one
indexable resource aggregate (R10 rejects the native containers), and it drops
its elements at scope end today. Excluding it would have been a special case
with nothing behind it.

**(a) ships.** None of the three costs a golden, which is the expected shape:
only a program that declares a resource can reach the arm at all.

### 10.3 What the two static halves share

`collect_loan_overwrites` became `collect_place_overwrites` and answers both:
the loan half (B94) and the component half (B99) are the two ways the scanned
body is not the owner of what it overwrites, and both are settled by the
target's static shape rather than by the scan's flow. The three arguments B94
made for "no liveness question is needed" are the same three, with R5 supplying
the first: a component place always holds a live value, because a resource
field is loan-only and moving one out of a live aggregate is rejected.

Twelve pins in `inference.rs`, four plants:

| plant | red |
|---|---|
| the component arm removed | 8 |
| the component arm restricted to an owned root — candidate (b) | 1 (`a_component_write_through_a_view_drops_the_old_value`) |
| `Index` excluded from the component arm — candidate (c) | 1 (`an_element_write_drops_the_old_value`) |
| the write emitted BEFORE the drop | 7 |

The pins green under every plant are exactly the ones that pin UNCHANGED
behavior: the data component, the resource-free program, the `&mut` of the
component, and B94's own `__replace` ordering.

`resource.vl` gains `component_owned`, `component_view` and `component_data`,
the way the fixture did in §2, §6, §7 and §9. `component_owned` and
`component_view` are the same write twice, differing only in whether the root
is the place or a `&mut` of it, and the emitted bytes are identical — `$a(slot
[0]); slot[0] = [ .. ];` — which IS the claim that the spellings are
indistinguishable. `component_data` is where the ABSENCE lives: `counted[1] =
2;` with no drop, inside an aggregate that holds a resource. The golden moved
**additively** — every pre-existing byte unchanged. **No other corpus golden
moved.**

### 10.4 Bycatch, verified and filed

Three findings from the measurement, none fixed in this lane:

- **An inferred `List<Resource>` escapes R10 entirely.** `mut arr: List<Guard>
  = [Guard { .. }]` is rejected as designed; deleting the annotation compiles
  the same program, and the element is never destroyed. Recorded under R10 in
  `destruction.md` with the fixed-array contrast.
- **A by-value function that hands its view parameter straight back is still
  classified as borrowing it** (`infer_borrows`' `Expr::Local` arm), so its
  result binds as a view even though B100 now makes it a copy. Refused
  deliberately: narrowing that arm turns the body into a rule-3 view-escape
  rejection. `element-clones.md` §9.3 has the measurement; the reason is
  recorded at the arm.
- **A compound assignment evaluates an impure index twice.** `ys[bump()] += 1`
  emits `__at_put(ys, bump(), __at(ys, bump()) + 1)` and has for as long as the
  synthesized re-read has existed. Pre-existing, unrelated to resources, and
  the reason B99's drop may re-walk its target place without introducing a new
  class of defect: the emission was already this shape.

## 11. B115 — the final leg's guard, 2026-08-10

§5's B59 update taught `Expr::Match` to compile each leg into its pieces
(pattern test, prelude, guard test, body) and to pick the emission shape once
every guard has been walked. What it inherited unexamined is the line that runs
after that choice: the final leg becomes the bare `else`, "the analyzer verified
exhaustiveness". `match a { A => .., B if c => .. }` ran B's arm when `!c` —
the leg's test, its prelude and its **guard** were all dropped together.

### 11.1 The verdict: the checker was right, the lowering contradicted it

Probed before touching anything, because a guarded leg cannot prove
exhaustiveness and only one of the two halves could be believing that.

The **checker** already believed it, and had all along. `resolve_match`'s
catch-all walk requires `guard.is_none()`, and its variant-coverage walk
filters guarded legs out before counting. The filed repro is refused outright:
`match e { E::A => "a", E::B if c => "b" }` is *"match is not exhaustive:
missing 'B'"*. The **lowering** was the half asserting the opposite — that the
final leg is licensed to answer for everything left over — and a guarded leg is
exactly the leg the checker's proof never used.

So the bug is not that the two disagree about guards. It is that the lowering
consumed a proof that, in two places, was never made:

- **The subject-type exemptions.** A tuple, generic, `any`, `never` or
  not-yet-known subject skipped the exhaustiveness question entirely. That
  exemption answers the DOMAIN question — *which values can the subject take?*
  — and it never licensed a guard, but it is what let `match p { (1, 2) => "x",
  (let a, let b) if c => "guarded" }` compile and answer "guarded" for `(7, 8)`
  with `c` false.
- **A refutable tuple pattern counted as a catch-all.** `has_catch_all` matched
  `ExprPattern::Tuple(_)` flat, so `(1, 2)` — a *test* — was read as a
  destructure that matches everything. Spec §3.10 already said otherwise ("the
  `let`/parameter binder grammar is the irrefutable subset"), so this was a
  conformance bug, and it is why even the first mechanism's fix did not by
  itself refuse the tuple shape: the match appeared to have a catch-all.

### 11.2 The rule shipped

> Exhaustiveness is proven by **unguarded** legs only, whatever the subject's
> type — a guard tests the value, which the check does not reason about. A
> `match` whose final leg is guarded must therefore be exhaustive *without* it,
> and is refused otherwise. At run time the final leg keeps its test, its
> prelude and its guard; only an unguarded one becomes the bare `else`.

Both halves are load-bearing and both are planted out in the pins. The refusal
is what removes the shape from the language; keeping the guard is what makes
the emission right on its own terms rather than downstream of a verdict —
§11.5's residual is a live case where the verdict is wrong and the emission is
not.

The refusal is deliberately narrow: it lapses the *exemptions*, and asks
nothing new of a match whose earlier legs already carry the proof. A fully
covered enum with a guarded final leg (`E::A => .., E::B => .., E::B if c =>
..`) still compiles — the leg is dead where it stands, which is precisely why
its guard must survive: with the guard dropped it became the `else` and
answered for values the legs above it had already taken. A blanket "the last
leg may not be guarded" was considered and rejected: it would have to call that
match non-exhaustive, which is a lie about it.

The diagnostics gained a note at the guard's span — *"this leg is guarded, and
a guarded leg cannot prove exhaustiveness"*. Without it *"missing 'B'"* is
close to unreadable, because the author can see `E::B` written in the match.

### 11.3 The trap composes, and its message stays honest

`backed-enums.md` §11.3 recorded the collision and left it: the trap arm keeps
a backed final leg's variant test "and still drops the guard, deliberately".
Both now, with neither emitter learning about the other — the leg keeps test
AND guard, and the trap is still appended as the true `else`:

```js
if ($a === "s") { .. } else if ($a === "e") { .. }
else if ($a === "e" && count > 0) { .. }
else { __enum_trap("Align", $a); }
```

The message stays honest for the reason the refusal supplies: the legs above a
surviving guarded leg cover every variant, so an in-set value the guard rejects
was taken by one of them and only an out-of-set value reaches the trap —
exactly as when the final leg is unguarded.

### 11.4 The sweep, and what moved

701 match expressions across std, the corpus, examples, benchmarks,
`macro_std`, every `.vl` under `crates/`, all 497 docs/proposal fences and
every vilan program embedded in a Rust fixture. **Guarded final legs: one**, and
it is §2.1's illustrative fragment in this document (a tuple subject, no
catch-all) — not compiled by any gate, and corrected in place. So the flip list
is empty: no program in the tree changes verdict or emission, and no golden
moved. The corpus had no byte-level coverage of the shape at all, which is why
it now has three (`match-patterns.vl`: the chain, the backed twin with its
trap, and B59's sequence emitter reached through an `is` guard).

### 11.5 The residual, and two bycatch bugs

The one shape a guarded final leg still survives *reachably*: an enum whose
coverage comes from a leg with a REFUTABLE payload pattern.

```vilan,fragment
match p {
    Pair::Of(Align::Start) => "s",
    Pair::Of(let x) if count > 0 => "guarded",   // reachable: the proof is holed
}
```

*(Closed by §12: the fragment above is refused now, and this section is the
record of what B115 found, not of what the checker does today.)*

The coverage walk asks only which VARIANT a pattern names, never whether the
pattern can fail below the tag, so `Of` counts as covered and the match is
accepted. Before the fix this answered "guarded" for `Of(End)` whatever the
guard said; now the guard is tested and the value matches no leg. Pinned both
ways: a live pin that the guard survives here too, and an `#[ignore]`d pin for
the verdict the checker owes. Not B115's hole — the same walk accepts `match p
{ Pair::Of(Align::Start) => "s" }` alone, which answers "s" for `Of(End)` with
no guard anywhere in sight — and closing it wants real nested-pattern
usefulness analysis, its own measurement. Filed.

Filed with it, found by the same probes and left where they were found:

- **A tuple subject skips exhaustiveness even unguarded.** `match p { (1, 2) =>
  "x", (3, 4) => "y" }` compiles and answers "y" for `(7, 8)`. The exemption
  now lapses only for a guarded final leg; unguarded, the domain question is
  still never asked. Same family as the residual above — the checker accepting
  a non-exhaustive match — and the same fix closes both.

## 12. B118 — the coverage walk's nested and refutable holes, 2026-08-10

§11.5's residual, and the two shapes filed beside it. The coverage walk asked
which VARIANT a pattern named and stopped there, so a `match` was judged on its
patterns' ROOTS and everything refutable below them was invisible. `enum Pair {
Of(Align) }` has one variant, so `match p { Pair::Of(Align::Start) => "s" }`
was total — and answered "s" for `Of(End)`. No guard, no host, no boundary: an
ordinary value built by vilan, given another arm's answer, exit 0.

### 12.1 Three shapes, one question asked of the wrong thing

Filed as three, and they are three, but the check has only one place to be
wrong and it was wrong there once:

- **(a) a refutable payload proves coverage.** The walk collected
  `ExprPattern::Variant(_, variant_index, _)` and discarded the payload, so
  `Of(Align::Start)` and `Of(let x)` counted the same.
- **(b) a tuple subject skips the question even unguarded.** §11.2 lapsed the
  exemption for a guarded final leg only; unguarded, `(1, 2) => .., (3, 4) =>
  ..` was never asked anything and answered "y" for `(7, 8)`.
- **(c) the payload's own domain is never consulted.** `Wrapped::Of(1),
  Wrapped::Of(2)` over an `i32` is (a) with an OPEN payload instead of a closed
  one — the same discarded sub-pattern, a domain no set of literals exhausts.

(b) reads as the odd one out because its exemption lived in a different arm of
the same `match &subject_type`, but the exemption answers *"which values can
the subject take?"* and a tuple is the one type in that list with an answer:
the product of its elements'. It was in the list because the check had no way
to ask a compound type anything. Now it has, and the tuple leaves.

### 12.2 The rule shipped

> An unguarded leg proves coverage only for the value-space its whole pattern
> **tree** covers, not its root. An **enum** position is covered when every
> variant is named and each named variant's payload positions are covered in
> turn; a **tuple** position when its elements are, as a product; an **open**
> position — `i32`, `str`, a struct, a fixed array, a still-abstract parameter
> — only by a binder or `_`, because no finite set of tests exhausts one.

It extends §11.2 downward and changes nothing about it: guards are filtered out
before the walk begins, exactly as before, so the two rules compose by
construction rather than by agreement.

The exemption list keeps `Unknown`, `any`, `never` and a generic parameter, and
keeps §11.2's lapse for a guarded final leg. Those are the subjects with no
domain to ask about, and treating an unknown position as OPEN instead was
considered and rejected: it would refuse matches that are correct for every
instantiation, which is the direction with no recovery.

### 12.3 The walk, and why it is a matrix

`uncovered_shape` is the usefulness question asked of the all-wildcard row, in
the standard matrix form: rows are the unguarded patterns (an or-pattern
contributes one row per alternative), columns are types, and the whole of it is
three cases applied to the first column.

- The column's space is CLOSED and every constructor appears: descend into each
  one, splicing its field columns in where the head column was. This is what
  makes the rule descend — `Pair::Of(Align::Start)` reaches the case where
  `Align`'s own space is asked the same question, and answers `End`.
- The space is closed with constructors left over: those are the hole, and the
  columns behind it are asked of the rows that match anything here.
- The space is OPEN: same default step, and the witness is `_`.

A per-leg predicate — "is this leg irrefutable?" — cannot express the rule at
all, and the shape that proves it is
`match p { (Align::Start, let b) => .., (let a, Align::End) => .. }`: neither
leg covers anything on its own and together they leave exactly one value,
`(End, Start)`. Coverage is a property of the SET of legs, which is what a
matrix is for.

Two mechanisms are borrowed rather than rebuilt, and both matter. `default_rows`
filters on `is_irrefutable_pattern` — §11's own conformance fix — so "matches
everything at this column" means one thing in the catch-all walk and in the
descent. And a variant's payload columns are typed through
`enum_type_substitution` / `impl_subject_substitution`, the same substitution
`resolve_pattern` applies when it binds a capture; without it `Option<Align>`'s
payload column would be the abstract `T` and the hole invisible.

Termination is the matrix's, not the type's: a self-referential payload
(`enum Tree { Leaf, Node(Tree, Tree) }`) unfolds only as far as the patterns
do, because a column of binders is DROPPED rather than descended into. A step
budget stands behind that as a bound on the usefulness algorithm's worst case,
and it can only ever lose a diagnostic, never invent one — exhausting it
answers "covered", which is what the check said before.

### 12.4 The message names a pattern, not a paraphrase

Three renderings, in order of how much the walk learned:

| the hole | message | example |
| --- | --- | --- |
| a set of the SUBJECT's own variants | `missing 'A', 'B'` | unchanged since §1.5 |
| rules nothing out | ``add a catch-all `_` leg`` | every unbounded scalar, and `(_, _)` |
| anything below the top level | `missing <one value>` | `missing Pair::Of(Align::End)` |

The middle row is why a tuple subject of literals says *"add a catch-all"*
rather than *"missing `(_, _)`"*: the two witnesses are the same witness, and
the catch-all is the actionable spelling of it. The bottom row names ONE
uncovered value rather than a set, because a set below the top level has no
readable spelling — vilan's or-pattern is a comma between legs, not a `|`
inside a pattern.

The contract is that the witness is a pattern the author can paste, and it is
pinned by pasting it (`b118_the_witness_is_a_pattern_that_closes_the_hole`).
That pin is what caught the two places the rendering was merely *recognisable*:
a whole-variant witness dropped its payload arity (`Tree::Node`, which does not
parse where `Tree::Node(_, _)` does), and `bool`'s variants were qualified —
`true` and `false` are literals the parser takes, and `bool::true` is not a
pattern at all. Both are spelling rules of the language, so the renderer keeps
them rather than the walk.

### 12.5 The sweep, the cost, and what moved

619 fenced blocks across `docs/` and `proposal/` (500 vilan, 55 carrying a real
match expression), every `.vl` file in the tree, all 21 `vilan.toml` projects,
the 114-program corpus and the whole inference binary.

**The flip list is three, and every one of them was a program relying on the
hole:**

| site | shape | triage |
| --- | --- | --- |
| `inference.rs` `b114_a_match_carrying_no_backed_test_keeps_its_bare_else` | (c) — `Wrapped::Of(1), Wrapped::Of(2)` over `i32` | real hole. The literal moves off the final leg; the claim (an unbacked nested test grows no trap) is unchanged and still pinned. |
| `inference.rs` `b115_a_guarded_final_leg_keeps_its_guard_where_the_proof_is_holed` | (a) — the pin whose premise WAS the hole | retired premise. Its program is now refused; the pin guards the same lowering claim behind a sound nested proof, renamed `..._behind_a_nested_proof`. |
| `capture-clones.md` §11.5's fragment | (a) — the residual, illustrated | the document's own exhibit of the bug. Left in place with a forward pointer: rewriting it would erase what B115 found. |

Zero sites in std, the examples, the benchmarks, `macro_std`, the templates,
the corpus or the compiled docs. **Goldens moved: 0** — the check only ever
became stricter, so an accepted match lowers exactly as it did, and a refused
one has no lowering to move. The three CLI templates fail to check for a
pre-existing and unrelated reason (`{{name}}` is not an identifier until `vilan
init` substitutes it).

**Cost, measured**: over the 23 478 coverage walks the tree's own sources
provoke, the most expensive match costs EIGHT steps. The budget is 200 000.

**Pins: 28** under `b118_`, plus two `#[ignore]`d pins un-ignored where they
stood (`b114_a_refutable_nested_pattern_is_not_exhaustive`, and B115's residual
renamed `b118_a_refutable_payload_pattern_does_not_prove_coverage`). The
matrix is the edge cases, not the happy path: two levels of nesting, a tuple
inside a variant payload, a multi-payload variant, a recursive enum in both
directions, a generic payload that only holes once substituted, an or-pattern,
leg order, and each of `_` / binder / guard proving what it always did.

**Plants: 6**, each red on the pins it should be and only those — the walk
stopping at the top level (14 red), the tuple exemption restored (3), an open
position covered by anything (22, and B115's own pins are among them, which is
the two rules composing), guarded legs counted again (9), a whole-variant
witness forgetting its payload arity (1), `bool` spelled as a path (1).

### 12.6 The trap composes, and one of its cases retires

`backed-enums.md` §12.4 named the one point where the membership trap and this
hole interact: under (a) a match could reach the trap with every value IN set,
and the trap would then name an in-set value as out-of-set. That case is gone,
because the match that produced it is refused — the trap is now reachable only
by the value it was designed for, one the HOST invented.

Nothing in the trap changed, and nothing needed to: the walk asks the VARIANT
SET, which is the same set whatever the variants lower to, so shape (a) is
refused identically for a backed payload and an unbacked one. Eight of §12's
nine pins are green and the ninth is B121's, still `#[ignore]`d and untouched;
all 56 b76 pins are green; and the corpus's byte-level trap coverage did not
move — `match-patterns.vl`'s new `labelled` traps at the payload slot and its
new `paired` at the tuple element, both of which are §12.3's shapes reached
through a match the walk now has to approve first.

One consequence for the emission worth recording, because it narrows what can
reach the lowering rather than changing it: **a nested test over an OPEN domain
can no longer be the final leg of an accepted match.** `Wrapped::Of(1),
Wrapped::Of(2)` was the only way to write one, and it is not exhaustive. A
nested test over a closed domain still can be, which is where the trap lives.

### 12.7 What did not change, and what is filed

`_` and a binder still prove everything, at every depth. A guard still proves
nothing. The `is` operator is unaffected — it asks a refutable question and
answers `false`, which is what it always did. The subject-type exemptions are
unchanged except for the tuple's departure.

Filed, found by the same probes and left where they were found:

- **A fixed-array pattern is refutable even where the type makes it total.**
  `is_irrefutable_pattern` says `ExprPattern::Array(_)` is refutable, and over
  `[i32; 2]` the pattern `[let a, let b]` cannot fail — the length is in the
  type and resolution already checks it. Unreachable from a `match` today (the
  pattern parser takes `[..]` in a `let` binder, not in a match leg), so it
  costs nothing and is not in this arc's rule. It is the one place the walk's
  OPEN case is conservative rather than correct.
- **`proposal/router.md`'s route fragment omits a variant.** `Route::Login` has
  no arm in the fence at line 109. Non-exhaustive under the OLD rule too, so it
  is not a flip — a pre-existing inaccuracy in an ungated fence.
