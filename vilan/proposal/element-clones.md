# Stores and returns copy — closing A20 and B54

> **Status: SHIPPED 2026-08-04** (backlog A20 and B54, one slice). B53
> (`capture-clones.md`) made pattern captures copy; this closes the two seams
> its ship record left open — a place read into a CONSTRUCTION (B54) and the
> elements a list-producing method hands back (A20). They turned out to share
> one root and to need one more rule beyond it. The backlog entries are the
> index; this is the detail.

## 1. The question, and where the copy lands

Rule 1 (`docs/spec/memory.md`): *every binding, assignment, argument pass,
field initialization, and return copies the value.* A20 asks where rule 1
demands element independence for `xs.map(f)` — at the producing method's
return, or at the consuming write? Neither. **It lands at the store.**

Trace the one aliasing value through `List::filter`:

```vilan
fun filter(self, predicate: |T| bool): List<T> {
    mut result = List::new();
    for item in self {                 // 1. binding
        if predicate(item) {           // 2. argument pass
            result.push(item);         // 3. STORE
        }
    }
    result                             // 4. return
}
```

Rule 1 names all four. Rule 2 (elision) removes any copy no conforming program
can observe — and at hops 1, 2 and 4 the alias is *in flight*: one owner at a
time, and the alias dies at the next hop. Hop 3 is the only one after which
**two durable owners exist**, `self` and `result`, both alive, both writable.
So exactly one copy is not elidable, and it is the store.

That is also why A20 and B54 are one slice. A construction literal's slots are
stores by the same definition — `[xs]`, `(xs, 1)`, `Holder { items = xs }`,
`Some(xs)` each install a place's value in a slot of a new aggregate that
outlives the expression. Rule 1's prose already said so ("field
initialization"); nothing enforced it. One rule covers both:

> **A place read into a slot of an aggregate that outlives the expression
> copies**, with rule 2's MOVE elision (a dead local donates) and the resource
> carve-out (R1: a resource never copies) unchanged.

### The store positions, and how a signature declares one

Two syntactic realizations, one rule:

- **Constructions** — `Expr::List`, `Expr::Tuple`, `Expr::StructInitializer`,
  and a variant constructor's payload. The last is spelled as a call but builds
  an aggregate; variants carry no `Parameter` entries, which is exactly why the
  pre-existing `own`-argument arm never saw them.
- **`own` arguments** — already the machinery for "the callee owns this". A
  container method that KEEPS what it is given is a store, and `own` is how a
  signature says so. `List::push(&mut self, own item: T)` was under-declared;
  saying it out loud is the whole fix for `filter` and `reverse`, which build
  their results through `push`.

`sort_by` is an intrinsic (`list.slice().sort(cmp)` — a new spine over the same
elements), so it has no `push` to ride. It became `own self`: the result is
built from the receiver's elements, so it needs to own them. A receiver that is
dead at the call donates instead of copying, which is the common case.

## 2. The generic half

`push`'s `item` is typed `T`. `is_cloneable_aggregate` answers *false* for
`Type::Generic(_)`, so the `own` declaration alone changed nothing — the store
inside `filter` was filtered out before it was asked about.

This is the same shape B53 hit at `Option::unwrap`, and it takes B53's answer:
admit a bare `T` without knowing whether it is an aggregate (`__clone` is
identity on scalars, so the conservative wrap costs nothing) and re-decide
RESOURCE-ness per monomorphization, because copying a resource mints a second
owner with a second destructor run (R1, R11).

So `clone_sites` stopped being a `HashSet<Id>` and became a
`HashMap<Id, CopyDecision>` — the same enum the capture pass already used,
renamed from `CaptureCopy` because it now serves both. `maybe_clone` and
`capture_copies` share one `copy_applies`.

## 3. The rule the store alone does not reach: returns

Closing the store left `map` still aliasing, and the reason is instructive.
`map` pushes `fn(item)` — a CALL result, which the whole elision framework
assumes is owned ("Fresh values (constructors, literals, calls) own their
result", in `compute_clone_sites`' own doc comment). That assumption was
false, and not because of anything to do with lists:

```vilan
fun identity(c: List<i32>): List<i32> { c }     // hands back the CALLER's storage
fun items_of(holder: Holder): List<i32> { holder.items }
```

Both handed the caller a live alias into the caller's own argument, verified
against v0.24.0. `|c| c` is the same function spelled inline, which is the
entire mechanism behind `map`'s element sharing.

> **A place a body returns that it does not own copies** — one rooted at a
> **by-value** parameter.

Three exemptions, each load-bearing:

- **A local root moves.** The frame dies at the return, so the local is a dead
  owner and donates. `mut result = …; result` is how every list-producing
  method ends, and it stays free.
- **An `own` parameter moves.** The caller already copied it in (or donated a
  dead one), so the callee owns it — and this is the tool a fluent builder
  wants: `fun with(own self, …): Self { … self }` returns without copying.
- **A `&`/`&mut` parameter is a borrow.** Returning through one is rule 3's
  `borrows` projection, an alias on purpose.

Keyed by the tail LEAF rather than the return value, so a tail `if`/`match`
copies only in the arms that owe it — `collect_tail_leaves`, the same walk B53
used for its seam scan. Leaf ids never collide with `clone_sites`' entries: an
expression occupies exactly one syntactic position, and a return leaf is not
also an initializer or an argument. They still get a map of their own so the
two hooks structurally cannot double-wrap.

## 4. Two findings on the way

- **`reverse` aliases too.** A20's record said it "happens not to alias,
  because it rebuilds through `push`". Rebuilding through `push` copies the
  SPINE; `self[index]` hands the element over uncopied, and `__at` — unlike
  `get`, which clones — returns the element by reference. All four
  list-producing methods aliased, not three. Pinned.
- **`Shared<T>` was marked cloneable and is not.** `Shared` lowers to a `{ v }`
  cell and `__clone` returns a plain object unchanged *on purpose* — sharing
  the cell is what `Shared` is for. The analyzer's filter disagreed with the
  runtime helper, which cost a `__clone` that copies nothing. Newly visible
  once constructions started copying (`View { attributes, … }` on the SSR
  path); the filter now matches the helper.

## 5. Where the code lives

| piece | site |
|---|---|
| store positions (constructions, variant payloads, `own` args) | `analyzer.rs::compute_clone_sites` |
| the return rule | `analyzer.rs::compute_return_clone_sites` |
| shared per-instantiation decision | `analyzer.rs::CopyDecision`, `transformer.rs::copy_applies` |
| interned type for the resource questions | `analyzer.rs::type_id_of_expr`, `place_value_type_id` |
| `Shared` is not a cloneable aggregate | `analyzer.rs::is_shared_cell` |
| construction emission | `transformer.rs` `Expr::List` / `Expr::Tuple` / `Expr::StructInitializer` arms |
| return emission | `transformer.rs::walk_entity` (wrapping `walk_entity_inner`) |
| `push` keeps what it is given | `std/src/list.vl` |
| `sort_by` owns its receiver | `std/src/compare.vl` |

## 6. Coverage

Fifteen new pins in `crates/vilan-core/tests/inference.rs`, **fourteen red
against v0.24.0** before a line was written; the fifteenth
(`a_returned_local_still_moves`) guards an elision whose presence is invisible
to output, so its proof is the golden, not the pin.

| pin | seam | proof |
|---|---|---|
| `a_list_literal_element_copies_its_source_place` | list literal | red at HEAD (3, want 2) |
| `a_tuple_literal_element_copies_its_source_place` | tuple literal | red at HEAD |
| `a_struct_literal_field_copies_its_source_place` | struct literal | red at HEAD |
| `a_variant_payload_copies_its_source_place` | variant payload | red at HEAD |
| `a_construction_does_not_see_later_writes_to_its_source` | read direction | red at HEAD |
| `a_nested_construction_copies_at_every_level` | struct inside a list | red at HEAD |
| `a_pushed_place_copies_into_the_receiver` | `own` store | red at HEAD |
| `filter_does_not_share_elements_with_its_receiver` | A20 | red at HEAD |
| `reverse_does_not_share_elements_with_its_receiver` | A20 (§4) | red at HEAD |
| `sort_by_does_not_share_elements_with_its_receiver` | A20 | red at HEAD |
| `map_does_not_share_elements_with_its_receiver` | A20 via §3 | red at HEAD |
| `a_list_method_chain_does_not_share_elements` | composed | red at HEAD |
| `a_returned_parameter_place_does_not_alias_the_caller` | §3 | red at HEAD |
| `a_returned_field_of_a_parameter_does_not_alias_the_caller` | §3 | red at HEAD |
| `a_returned_local_still_moves` | the elision | golden |

The corpus fixture `vilan/test/element-clones.vl` pins the emitted SHAPES in
bytes — including the two ABSENCES, which is where the elisions live: `donate`
constructs from dead locals and emits not one `__clone`, and `own_through`
returns an `own` parameter without copying. Planting each elision out turns the
golden red (`[ first, second ]` → `[ __clone(first), __clone(second) ]`;
`return items` → `return __clone(items)`).

**Golden impact: 36 of 107 moved, every one verified byte-identical at
runtime** before adoption. The SSR canary keeps its read-only `render`
recursion completely clone-free — B53's share elision is untouched — and gains
copies only on BUILD paths (`place`, `set_attribute`, the `return self`
builders), where a second owner really is created. Those `__clone`s are cheap
by construction: a `View` is four slots of `str` and `Shared` cells, all of
which `__clone` shares by reference.

One incidental improvement fell out of the measurement: `__clone` was emitted
whenever `clone_sites` was non-empty, which after this change was nearly every
program — 63 goldens were about to gain a helper they never call. Registration
is now use-driven (the wrap sites insert it), which cut the moved goldens from
99 to 36 and drops the dead helper from programs that had one before.

## 7. Left open, on purpose

- **The return copy is the callee's, and could be the caller's.** A builder
  chain (`view("div").text("x")`) copies at each `return self` even though the
  receiver was a dead temp — the callee cannot see that. The precise form is an
  escape summary ("this function's result may alias argument *n*") consulted at
  the call site, where `is_elidable_copy` already knows deadness. The std/UI
  builders can opt out today by declaring `own self` (§3); doing that across
  both UI twins is an API change that wants its own slice and its own
  re-verification.
- **A closure returning a CAPTURED local still aliases.** `mut xs = [1, 2];
  let get = || xs;` hands out `xs`'s storage. §3's rule keys on parameters; a
  capture is neither a parameter nor a local of the closure, and telling them
  apart needs a per-body declared-inside set (the shape
  `scan_one_closure_captures` already computes for R9). Narrow, and unrelated
  to either backlog item. **CLOSED 2026-08-04 — §8.**
- **`Set`'s store rides an undeclared `own`.** The other two std containers
  were surveyed. `Map::insert` is already covered — it stores through a
  `(key, value)` TUPLE, which is a construction, so both slots copy (visible in
  `map.js`). `Set::insert` hands its element to `NativeMap::insert`, whose
  `value: V` is bare, so the element is stored uncopied. It is not observable
  today: `contains` goes by hash and `values()` copies on the way out, so no
  program can see the sharing — and a change with no red pin behind it is not
  one to make. `own value: V` there is the one-word fix if a read-back that
  does not copy ever appears.

## 8. B64 — the closure half of the return rule, 2026-08-04

> **Status: SHIPPED.** §7's first bullet, closed — and it was two cases, not
> one.

### 8.1 The rule, restated so the closure case is not an exception

§3 named three exemptions from the return copy and framed them as facts about
conventions: *a local moves*, *an `own` parameter moves*, *a view is a borrow*.
The first two are really one fact about FRAMES:

> **The returning frame owns this storage, and it dies at the return.**

A local is a dead owner because the frame dies. An `own` parameter is the
callee's because the caller already gave it up and, again, the frame dies. So
the exemption is not "locals and `own` parameters are free" — it is "**what the
returning frame owns** is free".

Inside a closure that premise fails for anything the closure did not declare.
The capture's frame does not die at the closure's return, and a closure runs
many times where a body runs once. Both halves show:

```vilan,fragment
mut xs = [1, 2];
let get = || xs;                                        // hands out `xs`
fun make(own items: List<i32>): || List<i32> { || items } // hands out the SAME list every call
```

The first is §7's repro. The second is the case §7 did not name, and it is why
"treat a capture like a bare parameter" would have been the wrong fix: the
`own` exemption is exactly right one frame out (`fun with(own self, …): Self {
… self }` must stay free) and exactly wrong one frame in. So:

> **A returned place rooted at a binding the closure did not declare copies**,
> whatever convention that binding carries. The parameter exemptions apply to
> the closure's OWN parameters, and the local exemption to its own locals.

`&`/`&mut` is unchanged and still exempt at every depth — rule 3's `borrows`
projection is an alias on purpose.

### 8.2 The declared-inside set

`closure_declared_bindings` reuses R9's body walk (`scan_capture_body`) with an
empty resource set. R9 wants the captured RESOURCE references and computes the
declared-inside set on the way; this wants only the set. One walk, so the two
answers cannot drift — which matters, because they are answering the same
question ("is this name from inside or outside?") for different reasons.

### 8.3 A pre-existing R9 false positive, fixed by the same walk

The walk built its declared-inside set from `let`s, `match` leg patterns,
parameters and nested closures' parameters — but not from `is` patterns. So a
closure testing its own parameter reported the binding it had just introduced
as a resource captured from outside:

```vilan,fragment
let read = |o: Option<Db>| {
    if o is Some(let d) { d.handle } else { 0 }   // rejected on v0.25.0: "cannot capture the resource `d`"
};
```

The `match` twin was always accepted; only the `is` arm was missing. Pinned
(`a_closures_own_is_capture_is_not_a_resource_capture`), red against the arm
removed.

### 8.4 Coverage

Five pins, three red before the fix (the two capture cases plus the field
projection), one guarding the elision the rule must not eat (a closure's own
local still donates — proven by the absent `__clone`, since behaviour cannot
see the difference), one the R9 fix above. **No corpus golden moved**: no
in-tree program returns a capture out of a closure, which is also why this
survived to be found by review rather than by a failure.

## 9. B100 — the return rule's loan hole, 2026-08-07

> **Status: SHIPPED.** §3's third exemption, refuted. Found by B97's
> measurement (`capture-clones.md` §9.1) and filed there: `fun make(&self):
> (i32, i32) { self.pair }` emitted `return self[0]`, so the caller's result
> WAS the receiver's field storage and a later write to the receiver showed
> through it. §7.7's "an owned call result needs no rule: nothing else names
> it" is true of the capture pass and false of that body.

### 9.1 The exemption was about the wrong thing

§3 named three exemptions and §8.1 restated the first two as one fact about
FRAMES — *the returning frame owns this storage, and it dies at the return*.
The third was never that fact:

> **A `&`/`&mut` parameter is a borrow.** Returning through one is rule 3's
> `borrows` projection, an alias on purpose.

A loaned parameter is precisely storage the returning frame does **not** own,
so it fails §8.1's test outright. What the sentence was really about is the
RETURN: a function whose signature hands back a view hands back an alias. That
is a property of the signature, not of the place the body happened to read.

R3's own list is what makes the asymmetry plain — bare, `&` and `&mut`
parameters are *all* loans, and the bare one already copied. `fun make(self):
(i32, i32) { self.pair }` returned 3 while `fun make(&self): (i32, i32) {
self.pair }` returned 99, from bodies that differ by one character.

> **A by-value return of a place the frame does not own copies.** The view
> exemption is the signature's: `&T` / `&mut T` out is rule 3's projection.

### 9.2 The shapes, measured

Fourteen probes against the pre-fix tree (`next` @ `45f5d66`). Each returns a
place through a loan and writes the source afterwards; each wants 3.

| shape | body | before |
|---|---|---|
| `&self` receiver's field | `self.pair` | 99 |
| `&mut self` receiver's field | `self.pair` | 99 |
| `&` free parameter's field | `h.pair` | 99 |
| **the receiver forwarded whole** | `self`, returning `Holder` | 99 |
| nested field | `self.inner.pair` | 99 |
| a `List` field (the A20 shape) | `self.items` | 3 elements, want 2 |
| a tail `if` arm | `if first { self.pair } …` | 99 |
| an early `ret` | `ret self.pair` | 99 |
| `&mut` projection (negative) | `&mut self.pair` + `borrows` | alias, correct |
| `&` projection (negative) | `&self.pair` + `borrows` | alias, correct |
| view parameter into a VIEW return (negative) | `v`, returning `&mut Holder` | alias, correct |
| bare `self` receiver's field (negative) | `self.pair` | 3, already right |
| `own` parameter (negative) | `items` | moves, already right |
| scalar field (negative) | `self.n` | no copy owed |

A view LOCAL cannot be returned at all — rule 3's `check_view_escape` rejects
`let v = &self.pair; v` — so the `None` (local) arm never sees a loan, and the
"a local is a dead owner" elision needs no amendment.

### 9.3 The candidates, measured before choosing

| | goldens moved | analyzer gate | shapes correct |
|---|---|---|---|
| **(a)** every loaned parameter owes a copy; the exemption stays the LEAF's own view-ness | **0** | 1942 pass | 13 / 14 |
| **(b)** (a), plus: the exemption reads the SIGNATURE (`returns_view`) | **0** | 1942 pass | 14 / 14 |
| **(c)** (b), plus: `infer_borrows` gated on `returns_view` too | 0 | **1 FAIL** | 13 / 14 |

**(a) leaves one shape**, and it is the shape that decides how the exemption
is spelled: `fun copy(&self): Holder { self }`. References are transparent, so
`self` inside `&self` is a view by every test the pass can run — and the
signature says plainly that what leaves is a `Holder`, by value. Asking the
LEAF answers the wrong question. `Function::returns_view` is the answer, the
declared twin of the existing `returns_mut_view`; a closure never has one,
which is right, because rule 3 forbids a closure returning a view at all.

**(c) is the root-cause fix of a real residual, and it is refused here.** Under
(b), `infer_borrows` still records `fun copy(&self): Holder { self }` as
borrowing its receiver (its `Expr::Local` arm asks only whether the forwarded
name is a view parameter), so the result binds as a view at every call site
even though it is now a copy. Gating that arm on `returns_view` makes the two
passes agree — and makes `check_view_escape` **reject the body outright** ("a
view cannot escape its scope"), turning a program that compiles today into an
error. That is rule 3's call, not rule 1's, and it wants its own measurement.
The residual left standing is conservative (a view binding is the more
restricted one) and unchanged from before this fix. Filed.

**(b) ships.** (The residual closed as B104 — §10. Candidate (c) was right
about *where*, and its failure was a missing half rather than a wrong idea:
rule 3's escape check needed the same signature fact rule 1 already had.)

### 9.4 Coverage

Sixteen pins in `crates/vilan-core/tests/inference.rs`, three plants:

| plant | red |
|---|---|
| the `&`/`&mut` exemption restored | 8 |
| the exemption reads the LEAF — candidate (a) | 1 (`a_view_receiver_forwarded_whole_into_a_by_value_return_copies`) |
| the view-return exemption removed | 2 (both forwarded-into-a-view-return pins) |

The pins green under every plant are exactly the ones that pin UNCHANGED
behavior: the two `&place` projections (a `&mut place` leaf is not a place at
all, so it never reaches the seam), bare `self`, the `own` parameter, the
scalar, and the closure.

**No corpus golden moved**, and not by luck: a sweep of every `.vl` in
`vilan/test` and `vilan/std/src` finds **zero** functions returning a place
rooted at a `&`/`&mut` parameter. The shape survived to be found by review
rather than by a failure, exactly as §8.4's closure half did.
`element-clones.vl` gains `viewed_of` and `viewed_projection` — the same leaf
shape with opposite answers, `return __clone(holder[0])` against `return
holder`, so the byte diff between them IS the rule. The golden moved
**additively**.

### 9.5 The SHARE elision does not twin here

§9.3 of `capture-clones.md` declined to extend the capture pass's SHARE elision
to `borrows`-call subjects; the return seam declines for a stronger reason.
SHARE asks whether *nothing can write either side of the alias*, and at a
return the callee cannot see the answer: `h.make()` followed by `h.pair.1 = 99`
is the filed repro, and the write is in the caller. A read-only body proves
nothing about the caller's later writes, so a `&self` receiver earns no
exemption. The elision that *would* apply is §7's first open item — an escape
summary consulted at the CALL site, where deadness is visible — and B100's
copies join that item rather than motivating a new one.

## 10. B104 — the classification catches up, 2026-08-10

> **Status: SHIPPED.** §9.3's refused candidate (c), taken on its own terms.
> `infer_borrows` still recorded `fun copy(&self): Holder { self }` as
> borrowing its receiver, so the result bound as a VIEW at every call site
> (`mut c = h.copy()` was rejected, and a write to `c` lowered as a
> write-through) although B100 had made the return a copy.

### 10.1 One seam, one answer

B100 put the exemption in the signature: at a return seam,
`compute_return_clone_sites` reads `Function::returns_view`. Rule 3's root-set
was still reading the LEAF, so the two passes described the same seam
differently — and rule 1's was the true one, because it is the pass that emits.

> **A place the return COPIES has left the loan.** The function projects
> nothing through it, so it contributes no `borrows` position.

That is a statement about *one arm*. `collect_leaf_borrows_position` has four,
and the gate belongs only to the one whose leaf is a PLACE — the forwarded
`&`/`&mut` parameter — because that is the only leaf rule 1 reaches:

| arm | leaf | rule 1 copies it? | gated |
|---|---|---|---|
| forwarded parameter (`self`) | a place | yes | **yes** |
| `&self.x` | not a place (`place_root` = `None`) | no | no |
| `Some(&mut self.x)` | a call | no | no |
| a borrows-call chain | a call | no | no |

The three ungated arms hand back an alias whatever the signature says, and the
borrow classification is what keeps their call sites honest about it. Two of
them are latent wrongness on rule 1's side, not this one's — §10.4.

### 10.2 The escape check still accepts the forwarder

This is the hazard §9.3 recorded, and it is real: with the root-set empty,
`check_view_escape` reported *"a view cannot escape its scope"* against a body
that compiles today. The answer is not to loosen rule 3 but to give it the same
fact rule 1 has — **a by-value return hands back no view at all**, so there is
nothing to escape. The clause is deliberately narrow, and the two shapes rule 1
does not reach stay rejected exactly as before:

- a view of a **local** — rule 1 leaves it alone (the frame is a dead owner
  donating its storage), so nothing converts it and it still dangles;
- a **`&place`** leaf — not a place, never reaches the seam.

`by_value_return_copies_the_view` is `by_value_return_copies_the_place` plus
"and it roots at a loaned parameter", so the pass that empties the set and the
pass that tolerates the emptying cannot drift apart.

### 10.3 The gate is CLONEABLE-AGGREGATE, not by-value

Measured, not assumed. Gating on `returns_view` alone regressed two shapes,
because rule 1's copy does not reach them:

| forwarded parameter | rule 1 | by-value-only gate | shipped gate |
|---|---|---|---|
| `&Holder` (a struct, list, tuple, array) | `__clone` | correct | correct |
| `&mut i32` (a scalar) | nothing to clone | **leaks the `(base, key)` pair** | keeps the borrow |
| `&T` (generic) | `__clone`, identity on scalars | **leaks the pair at `T = i32`** | keeps the borrow |

A scalar view IS a `(base, key)` pair at runtime; `__clone` cannot collapse one,
and a generic `&T` is boxed for exactly that reason. So the gate asks rule 1's
own admission test — cloneable aggregate, non-resource — and where no copy is
inserted the conservative view classification stays, unchanged from before this
fix. The resource half is a guard rather than a live case: R1 refuses moving a
resource out of a loan before any of this is consulted (pinned, both the
declared and the by-containment spelling).

### 10.4 Coverage, and what stayed wrong

Ten pins in `crates/vilan-core/tests/inference.rs`, three plants:

| plant | red |
|---|---|
| the arm ungated — the B104 bug restored | 2 (both `binds_mut` pins) |
| the escape-check clause removed | 4, including **B100's own** `a_view_receiver_forwarded_whole_into_a_by_value_return_copies` |
| the gate reads by-value only | 2 (the scalar and generic `keeps_its_borrow` pins) |

**No corpus golden moved** — the same measurement as B100's, for the same
reason: nothing in `vilan/test` or `vilan/std/src` forwards a loaned parameter
whole into a by-value return.

Three `#[ignore]`d pins record bycatch found while measuring, all pre-existing
and all on rule 1's side of the seam:

- `fun same(v: &mut i32): i32 { v }` returns the view's `(base, key)` pair,
  not the `i32` the signature promises;
- `fun grab(&self): Inner { &self.inner }` — a `&place` leaf is not a place, so
  no copy is inserted and the caller's result IS the receiver's field;
- `fun get(h: &Holder): (i32, i32) { peek(h) }` — the same hole one indirection
  over, through a borrows-call leaf.

The last two are B100's residual, not B104's: the return rule reaches PLACES,
and both of those tails are expressions that produce an alias without being
one. Closing them wants the return seam to read through a returned view, which
is a rule 1 question and wants its own measurement.

## 11. B108 / B109 — the seam reads through a view, 2026-08-10

> **Status: SHIPPED.** §10.4's three `#[ignore]`d pins, closed together because
> they are one sentence: rule 1's return clause reached only leaves that were
> PLACES. Two leaf shapes name storage without being one — `&self.inner` (a
> `&place`, `place_root` = `None`) and `peek(h)` (a `borrows` call, likewise) —
> so `fun grab(&self): Inner { &self.inner }` handed the caller the receiver's
> field (99, want 3). A third leaf *did* reach the seam and fell out of the
> copy's TYPE filter: a scalar view, whose "copy" `__clone` cannot express.

### 11.1 One question, asked of the value rather than the leaf

§9 put the exemption in the SIGNATURE and §10 made the classification agree.
What neither moved is the seam's first step, which asked `place_root(leaf)` and
gave up on anything that was not a place. That question is about the leaf's
*syntax*; the rule is about the storage the return hands back.

> **A by-value return copies the storage its value NAMES**, whether or not the
> expression naming it is a place. A `&place` names its operand; a `borrows`
> call names the arguments the callee projects.

That is B97's `capture_subject_places` (`capture-clones.md` §9.3) asked at a
return instead of at a pattern subject, and it is answerable for the same
reason: *the receiver is right there in the call*. `returned_value_places`
recurses, so a chain (`o.mid_mut().slot()`) reaches the parameter at its root;
an OWNED call projects nothing and so names nothing, which is "a call owns its
result" falling out rather than being special-cased.

### 11.2 A scalar's copy is its READ

B108 is the same seam at a leaf rule 1 already reached. `fun same(v: &mut i32):
i32 { v }` printed `[ [ 5 ], 0 ]` — the view's runtime pair — because the copy
machinery is aggregate-shaped: `is_cloneable_aggregate` said no, the leaf left
the candidate list, and nothing else materialized a value there. §10.3 had
already found the shape and drawn the right conclusion for *its* question (the
gate is cloneable-aggregate, not by-value), which is why the leak was recorded
rather than fixed.

B81's doctrine is the answer: **a scalar read IS the copy**. So the seam is not
"which leaves clone" but "which leaves materialize a value", and the
representation decides how:

| the leaf emits | the crossing emits |
|---|---|
| an aggregate place or aggregate view (`self[0]`) | `__clone(self[0])` |
| a scalar place (`self[0]`) | itself — the read already happened |
| a scalar `(base, key)` pair (`v`, `peek(h)`) | `v[0][v[1]]` |
| `&`*scalar place* (`[self, 0]`) | `self[0]` — the pair is never built |

The last row is why the decision cannot live entirely in the analyzer: a
generic `&T` is a pair at exactly its scalar instantiations, and the pointee is
abstract until monomorphization. `return_view_reads` carries every leaf that
owes a copy; `emits_scalar_view_pair` resolves the representation under the
active substitution, exactly as `generic_ref_param_is_scalar` already did for
every other view question.

### 11.3 The candidates, measured before choosing

Each was implemented far enough to rebuild the whole corpus and run the
analyzer gate. "Shapes" counts 24 pinned answers over the probe set below.

| | goldens moved | analyzer gate | shapes correct |
|---|---|---|---|
| **(a)** read through `&place` leaves only | 0 | 2077 pass | 16 / 24 |
| **(b)** (a), plus `borrows`-call leaves (recursive) | 0 | 2077 pass | 20 / 24 |
| **(c)** (b), plus the scalar READ at the crossing | **0** | 2077 pass | **24 / 24** |
| **(d)** (c), plus: the borrow classification gated to match | 0 | **1 FAIL** | 24 / 24 |

**(a) and (b) are the same fix arriving in instalments**, and what they leave is
the leak §10.3 named: four of the eight shapes (a) misses are scalars, and they
stay wrong under (b) too. There is no reading of B109 that closes the `&place`
hole and leaves `&self.n` handing back a pair — it is the same leaf.

**(d) is the root-cause tidy, and it is refused with evidence.** §10.1's
sentence — *a place the return COPIES has left the loan* — now applies to two
more arms, so gating them looks like finishing the job. Gating them makes
`check_view_escape` reject **seven** shapes that compile today ("a view cannot
escape its scope"), including every aggregate `&place` probe and the resource
one, whose precise diagnostic it replaces with a worse one; and it reddens
B104's own `a_borrows_call_chain_into_a_by_value_return_keeps_its_borrow`. This
is §9.3's candidate (c) one level down, with the same shape of answer: rule 3's
escape check reads `place_root(function.body.1)`, which is `None` for exactly
the leaves B109 added, so the fact rule 1 now has does not reach it. Widening
it is rule 3's call and wants its own measurement. **The classification left
standing is conservative** — a value treated as a view, so `mut` is refused and
rule 4 counts it live — and unchanged from before this fix.

**(c) ships.**

### 11.4 The resource crossing, twinned

A resource cannot copy (R1), so the seam has no copy to offer it — and doing
nothing was not neutral. `fun take(&self): Guard { &self.g }` compiled, printed
the tag, and ran **no destructor at all**: the resource left the loan uncopied
*and* undestroyed. Its bare twin `fun take(&self): Guard { self.g }` is refused
("cannot move a resource field out of a live aggregate"), and the two differ by
one character.

So the crossing is told to the move scan rather than re-decided: a place a
by-value return hands back through a view leaf is **consumed there**
(`value_crossings`, whole-program like `loaned_captures`). The scan's own rules
then answer, and the answers are the bare twins' by construction — R1's partial
move for `&self.g`, R3's move-out-of-a-loan for a `borrows` call naming the
parameter (`cannot move the resource 'h' out of this function: it is declared
'&h', a loan`). Under a VIEW return nothing crosses and the same `&self.g` is
still rule 3's projection, pinned. R11 shares the set for the reason B65 does:
whether a leaf crosses is a property of its own signature.

### 11.5 Coverage

Twenty-one pins in `crates/vilan-core/tests/inference.rs` (three of them the
`#[ignore]`d bycatch, un-ignored), six plants:

| plant | red |
|---|---|
| the `&place` arm removed | 7 |
| the `borrows`-call arm removed | 5 |
| the chain recursion removed | 1 (`a_borrows_call_chain_leaf_in_a_by_value_return_copies`) |
| the scalar read removed | 3 (both B108 shapes + the scalar `borrows` call) |
| the `&place` crossing suppression removed | 1 (`a_scalar_reference_leaf_in_a_by_value_return_reads_the_place`) |
| the resource crossings emptied | 2 (both refusals) |

The pins green under every plant are exactly the ones that pin UNCHANGED
behavior: the OWNED call result, the `borrows` call on a LOCAL (a dead owner
donates — B100's elision, which the new arms must not eat), the view return,
the resource under a view return, and all of B100's and B104's.

**No corpus golden moved**, and the sweep says why rather than luck: an
instrumented binary reports **zero** new-arm return sites across `vilan/test`,
`vilan/examples`, `vilan/benchmarks`, `vilan/std` and `vilan/macro_std` — and
zero scalar-view return crossings — while firing on every probe. The same
answer B100 and B104 got, for the third time; `&`-of-field is the more natural
spelling of the two, and the tree still does not contain it.

`element-clones.vl` gains `reference_of`, `called_of`, `scalar_of`,
`scalar_projection` and `scalar_forward`. `reference_of` emits `return
__clone(holder[0])` — `viewed_of`'s body, byte for byte, which IS the claim
that the three spellings are indistinguishable — and `called_of` emits `return
__clone(items_view(holder))`. `scalar_of` against `scalar_projection` is the
crossing pair: `return cell2[0]` against `return [ cell2, 0 ]`, the same leaf
under the two return types. The golden moved **additively** — every pre-existing
byte unchanged, temp names included.

### 11.6 Bycatch, verified and filed

`ret &self.inner` — the explicit-`ret` spelling of B109's first shape — is
still refused with *"a view cannot escape its scope"*, while the tail spelling
one line away compiles. `check_view_escape` treats `Expr::FunctionReturn`
unconditionally as an escape and exempts only the tail (via `borrows` +
`derives_from_view_param`), so the asymmetry is rule 3's and pre-existing —
B100's own §9.2 table returned `ret self.pair` because a bare place is not a
view expression at all. Rule 1 copies both; only one of them is allowed to say
it. Filed rather than fixed here: it is the same escape-check widening (d)
wants, and it belongs to that measurement.

> **CLOSED by B116 (cycle 15) — see §12.** It was NOT (d)'s widening in the
> end: `return_sites` already indexed the `ret` as a return position, so the
> tail's own condition applies to it unchanged. (d)'s measurement is still
> owed, by the two shapes §12.3 files.

## 12. B116 — the `ret` spelling gets the tail's analysis, 2026-08-10

§11.6's bycatch, closed. `check_view_escape` read `Expr::FunctionReturn` as
an unconditional escape and exempted only `function.body.1`, so
`ret &self.inner;` was refused with *"a view cannot escape its scope"*
while the tail spelling one line away compiled. Rule 1 copies both; only
one of them was allowed to say so.

### 12.1 The filed repro could not be the probe

The lane was warned before designing, and the warning holds: **`ret` is
early-return-only**, so §11.6's `fun grab(&self): Inner { ret &self.inner; }`
is doubly invalid — a body ending in `ret x;` with no tail also draws
*"Expected Inner, but got void"*, regardless of the view question. Both
errors are reported on that program, so the escape refusal was real, but
the repro proved nothing on its own.

The probe that isolates it is a **conditional early `ret` with a legal
tail**, which is the idiom `base64.vl`'s `digit` is written in:

```vilan
fun grab(&self, flag: bool): Inner {
    if flag { ret &self.inner; }   // refused
    &self.inner                    // compiles
}
```

One error, at the `ret`, on the same expression the next line accepts.
Every pin in this section is that shape, and the asymmetry is real.

### 12.2 One index already had the answer

A `ret` is a return position exactly like the tail (`ret-checking.md`), and
`return_sites` — *(function id, value id)* for the tail **and each `ret`* —
already says so. `compute_return_clone_sites` reads it, which is why rule 1's
return clause reached the `ret` spelling all along: the copy was planned and
then the escape check refused the program that would have used it.

So the fix is to ask the same question at the same index, not to invent one.
`return_position_hands_back_no_view(function, value_id)` is the tail loop's
own condition with the seam as a parameter — the by-value copy (B104/B109)
or the `borrows` projection — and both callers now pass their own seam.

Two seams were **not** joined, and the second is the half that had teeth:

- `compute_return_clone_sites` — already `return_sites`. Unchanged.
- `compute_return_value_crossings` (§11.4's resource crossing) — walked
  `function.body.1` alone. Lifting the escape check without this one would
  have compiled a resource out of a loan through the `ret` door, uncopied
  and undestroyed: precisely the bug §11.4 shipped to close. The return
  positions are joined onto the per-function tails (the tails are kept
  separately because `return_sites` holds only functions with a DECLARED
  return type).

The two spellings emit identically, which is the claim: `__clone(self[0])` in
both branches for an aggregate, `v[0][v[1]]` in both for B108's scalar,
`h2[0]` in both for a sanctioned `borrows` projection.

### 12.3 What the lift does NOT reach, and why it is not the `ret`'s fault

Probing the fix turned up a second asymmetry with the same symptom and a
different cause, filed rather than fixed (it is §11.3 candidate (d)'s
measurement, which this lane is not):

```vilan
fun early(&self, flag: bool): Inner { if flag { ret &self.inner; } Inner { n = 0 } }   // refused
fun conditional(&self, flag: bool): Inner { if flag { Inner { n = 0 } } else { &self.inner } }   // compiles
```

Here the two spellings are examined by **different questions**. The tail
loop asks `escapes_as_view(function.body.1)` of the whole body, and an `if`
with one owned arm is not a view expression — so it is never asked at all.
The `ret` arm asks the leaf, which is. Neither exemption reaches it: the
function's `borrows` set is inferred from the tail and is empty here, and
`by_value_return_copies_the_view` roots at `place_root`, which is `None` for
a `&place` — §11.1's whole finding, in the one place rule 3 still reads it.

The same leaf-blindness runs the other way, and that direction matters more:

```vilan
fun grab(flag: bool): Inner {
    let local = Inner { n = 3 };
    if flag { Inner { n = 0 } } else { &local }   // compiles — a view of a LOCAL
}
```

Rule 3 exists to refuse exactly that, and a second owned arm hides it. The
`ret` spelling **is** refused. Benign as emitted (the frame is dead and
nothing else holds the storage, which is B100's dead-owner elision), but it
is the rule not being applied rather than the rule deciding — so it is
recorded as a limit, not as a design.

Both pinned `#[ignore]`d:
`b116_a_ret_beside_an_owned_tail_agrees_with_the_conditional_tail` and
`b116_a_conditional_tail_arm_may_not_escape_a_view_of_a_local`. Closing them
is one change — the escape check asking its question of a return's LEAVES,
which is what §11.3 measured (d) against and deferred.

### 12.4 The `Expr::FunctionReturn` sweep

Every other reader in the analyzer was checked, and `check_view_escape` was
the only special case. The rest are transparent recursions into the operand
— `plan_expr`, `scan_move` (R4's terminal move), `scan_bumps`,
`scan_view_param_ref`, `scan_closure_view_captures`, `scan_invalidation`,
`mark_repeatable`, `r11_collect_calls` — or leaves that are about
divergence, not about returning: `expr_diverges` and `Type::Never`. The one
real disagreement found is the one §12.2 fixed: two seam computations that
both mean "return position" and walked different sets.

### 12.5 Coverage

Eight live pins and two `#[ignore]`d, in `crates/vilan-core/tests/inference.rs`,
every live one the same program in both spellings: the aggregate `&place`
leaf, B108's scalar read, the `borrows`-call leaf, the sanctioned `borrows`
projection, the two resource refusals (R1's and R3's, word for word the bare
twins'), the ret-only resource crossing, and the view of a LOCAL — which
stays refused in both spellings, because agreement means agreeing on the
refusals too. Plus the boundary: a CLOSURE's `ret` still cannot hand back a
view, because a closure's rets never enter `return_sites` and a closure may
not project at all.

Two plants, each red on what it should be:

| plant | red |
|---|---|
| the `ret` an unconditional escape again | 3 (the aggregate leaf, the scalar read, the sanctioned projection) |
| the return positions unjoined from the crossing | 1 (`b116_a_ret_only_resource_crossing_is_named_by_the_move_scan`) |

The `borrows`-call pins are green under both plants and say so on purpose:
a call leaf is not a view *expression*, so the escape check never examined
either spelling — what those pins hold is the emission agreeing, which is
rule 1's half.

**No corpus golden moved**, and no docs page changed: the fix removes a
false positive, and nothing documented ever claimed the tail-only rule.

## 13. B122 — rule 3 asks its question of the LEAF, closing §11.3's owed measurement, 2026-08-10

§12.3's filed pair, closed, and §11.3 candidate (d)'s measurement — deferred
twice — finally taken. Two shapes, opposite directions, one cause: the escape
check (and the root-set inference that feeds its exemptions) asked "does this
RETURN POSITION hand back a view" of the position as a whole. An `if`/`match`
tail with one owned arm and one view arm is never, itself, a view expression,
so the question was never asked of the arm that mattered — in either
direction.

### 13.1 Verify first: both filed shapes reproduce exactly as recorded

This family has a history of filed mechanisms being wrong (B116 survived its
own invalid repro; B102 twice), so both shapes were built and run against the
live compiler before anything else:

```vilan
impl Holder {
    fun early(&self, flag: bool): Inner {
        if flag { ret &self.inner; }   // refused: "a view cannot escape its scope"
        Inner { n = 0 }
    }
    fun conditional(&self, flag: bool): Inner {
        if flag { Inner { n = 0 } } else { &self.inner }   // compiles
    }
}
```

```vilan
fun grab(flag: bool): Inner {
    let local = Inner { n = 3 };
    if flag { Inner { n = 0 } } else { &local }   // compiles — should not
}
```

Both reproduced precisely as §12.3 described: `early` refused, `conditional`
clean; `grab` compiled (and ran — the frame is a dead owner, so the alias is
benign as emitted, B100's own finding) where the same shape spelled with an
early `ret &local;` was already refused. No correction needed this time — the
premises held.

### 13.2 Two whole-position questions, not one

`check_view_escape`'s `ret` loop was already leaf-wise in the sense that
matters — `Expr::FunctionReturn(Some(value_id))` names exactly one leaf, the
`ret`'s own operand — so `early`'s refusal was not that loop asking the wrong
question. It was `return_position_hands_back_no_view`'s answer being wrong,
and that traced one level further: `infer_borrows`'s root-set walk
(`collect_borrows_positions`) called `collect_tail_leaves` on `function.body.1`
alone, never on a `ret`'s value. `early`'s tail is the plain `Inner { n = 0 }`,
which projects nothing — so `early.borrows` stayed empty, and
`return_position_hands_back_no_view`'s exemption
(`!function.borrows.is_empty() && derives_from_view_param(value_id)`) had
nothing to read. `conditional`'s tail *is* the `if`, so its leaves — including
`&self.inner` — feed the same walk and its root-set is `{0}`; the two
spellings disagreed only because one seam fed the root-set and the other
didn't, which is the exact shape B116 already fixed once for
`compute_return_clone_sites` and `compute_return_value_crossings` and had not
yet reached this third reader.

`grab`'s hole was the opposite defect in the opposite loop. `check_view_escape`
carried a *second* mechanism for the tail specifically —
`escapes_as_view(function.body.1, ..)`, asked once per function, not per leaf —
and `is_view_expr` only matches `Expr::Reference` and a view-holding
`Expr::Local` directly: an `if` is neither, so the question was **never asked**
whenever the tail was a conditional. Whether the hidden arm was `&self.inner`
(sound, B116's `early`) or `&local` (unsound, rule 3's whole reason to exist)
made no difference — the whole-body question doesn't reach either.

### 13.3 The fix: two seams get the leaf walk B116 already proved

**`infer_borrows`** now joins `return_sites` the way `compute_return_value_crossings`
already does: each function's root-set walk runs once over `function.body.1`
and once more over each of its `ret`s, unioned into the same fixpoint. This
alone closes `early` — `early.borrows` becomes `{0}`, matching `conditional`'s,
and `return_position_hands_back_no_view` reads the same answer through either
spelling.

**`check_view_escape`** drops its two special-cased loops (the per-expr
`Expr::FunctionReturn` match arm and the per-function tail loop) for one walk:
every function's `(function_id, function.body.1)` pair, unioned with every
`return_sites` entry (already both the tail *and* each `ret`, for a function
with a declared return type — `HashSet<(Id, Id)>` dedupes the resulting
overlap rather than re-walking the same seam twice), each run through
`collect_tail_leaves` — the identical helper rule 1's return clause and the
resource-crossing scan already use for these seams — and each leaf asked
`escapes_as_view(leaf) && !return_position_hands_back_no_view(function, leaf)`
on its own. `grab`'s `&local` arm is now a leaf in its own right, asked
regardless of its owned sibling; `early`'s `&self.inner` is asked as the same
leaf the `ret` loop used to isolate by hand. One mechanism now answers what
two half-mechanisms used to split between them.

### 13.4 A finding on the way: the closure `ret` regression

Deleting the per-expr `Expr::FunctionReturn` arm outright — reasoning that
`return_sites` plus the new seam walk covers every `ret` — was wrong for one
case and caught by the full `inference` run before it shipped:
`a_closures_ret_still_cannot_hand_back_a_view` went from refused to compiling
cleanly. A closure's `ret`s never enter `return_sites` (`ret-checking.md`:
they check against the closure's *inferred* tail type, not a declared one),
so the old per-expr loop was the *only* place that caught them — unconditionally,
with no exemption, because a closure may not declare `borrows` and is
second-class all the way (P4c). The new seam walk, built from
`self.functions` and `return_sites`, never sees a closure at all. The arm is
back, narrowed to exactly the leaves the seam walk does not cover
(`!function_return_value_ids.contains(value_id)`), unconditional exactly as
before. The closure TAIL's own whole-block blindness
(`escapes_as_view(closure.return_)` asks the same whole-position question the
function tail loop used to) is untouched and unfixed here — it predates this
lane, self-masked in every existing pin by the `ret` loop independently
catching the same leaf, and is not one of B122's two filed shapes. Filed, not
fixed: a closure tail's own conditional-arm blindness is the same family of
bug at a third reader, `infer_borrows` and `check_view_escape`'s function seam
being the first two.

### 13.5 Coverage

`b116_a_ret_beside_an_owned_tail_agrees_with_the_conditional_tail` and
`b116_a_conditional_tail_arm_may_not_escape_a_view_of_a_local` (§12.3) are
un-`#[ignore]`d and renamed for the bug that closes them
(`b122_a_ret_beside_an_owned_tail_agrees_with_the_conditional_tail`,
`b122_a_conditional_tail_arm_may_not_escape_a_view_of_a_local`). Four more
pin the semantics the two filed shapes only sampled: arm order reversed
(`b122_a_conditional_tail_arm_order_does_not_matter`), a view of a local two
`if`s deep (`b122_a_nested_conditional_arm_may_not_escape_a_view_of_a_local`),
the identical shape as a `match` tail
(`b122_a_match_leg_may_not_escape_a_view_of_a_local`), and the mix that proves
the walk is leaf-wise rather than merely wider —  one arm a sound parameter
view, the other an unsound local view — which must refuse **exactly once**,
naming the local arm's own span, not the parameter arm's and not the
enclosing `if`'s (`b122_a_mixed_leaf_return_refuses_only_the_local_view_leaf`,
`assert_fails_once_with` + `assert_fails_spanning`). Every pin B109/B116 left
standing — the still-refused whole-local return, both resource refusals, the
sanctioned `borrows` projection, the closure boundary — stayed green
throughout (2182 passed, 0 failed, 5 `#[ignore]`d, none of them B122's, for
the full `inference` binary).

Two plants, isolating the two fixes:

| plant | red |
|---|---|
| `infer_borrows`'s `ret` join removed | 1 (`b122_a_ret_beside_an_owned_tail_agrees_with_the_conditional_tail`) |
| `check_view_escape`'s leaf walk replaced with the whole seam | 5 (every local-view pin: arm order both ways, nested, `match`, the mixed leaf) |

Each plant reddened exactly its own half and nothing else, confirming the two
fixes are independent — `infer_borrows`'s join answers the false positive,
`check_view_escape`'s leaf walk answers the hole, and neither substitutes for
the other.

### 13.6 The owed measurement: zero, a fourth time

§11.3 candidate (d) deferred "how many shapes does widening the escape check's
reach move" to its own measurement; §12.3 deferred it again. Taken now, three
ways:

1. **Compile-based, full-scan.** `std` (both platform layers, every module
   force-imported so nothing is skipped as frozen — the S1 differential's own
   technique, `check_scope_differential.rs`) plus all 114 `vilan/test/*.vl`
   programs (each self-contained: no local imports to resolve), analyzed under
   the pre-fix and post-fix analyzer, diffing every `"a view cannot escape its
   scope"` diagnostic's message and exact span text. **Zero diagnostics,
   either tree, either side.**
2. **CLI-based, every example and benchmark project.** `vilan check` run from
   each of the 13 `vilan.toml` roots under `vilan/examples` and
   `vilan/benchmarks`, old binary against new, diffing raw output.
   **Zero diffs.**
3. **Structural, whole tree.** `grep -rn 'ret &'` across `std`, `macro_std`,
   `test`, `examples`, `benchmarks`, `docs` — shape 1's necessary precondition
   — and a broader pass for `} else { &`-shaped conditional arms — shape 2's.
   **Zero hits**, `macro_std` included (too small and too purpose-built for a
   compile-based full-scan sweep of its own to be worth building; the grep
   covers it directly).

| site | old verdict | new verdict | triage |
|---|---|---|---|
| *(none — the sweep found no site to report)* | | | |

The measurement table is empty because there is nothing to triage: every
verdict the tree contains is unchanged by the fix, because the tree contains
neither filed shape. This is the same answer §11.5 and §12's own coverage
reached for the sibling arms in this family — third and fourth, now — and it
is the reason the fix is provably safe to ship without a single golden
touched: `cargo test -p vilan-cli --test corpus` moved **zero bytes**, and
`cargo test -p vilan-core --test docs` is unaffected (no documented example
depends on the tail-only rule, because the tail-only rule was never a
documented contract — only an implementation gap).

## 14. B123 — the closure seam gets the same leaf walk, closing the filed residual

§13.4's finding, filed rather than fixed: `check_view_escape` carries a
*third* reader of the same whole-position question, alongside the two
`infer_borrows`/`check_view_escape` function-seam readers B122 closed —
`escapes_as_view(closure.return_)`, asked once per closure, never leaf-wise.
Recorded as `OPEN` (backlog B123) rather than folded into B122 because it was
not one of B122's two filed shapes, and because every pin in the suite that
could have exercised it happened to also spell a `ret` — which a *different*,
already leaf-wise mechanism (the per-expr `Expr::FunctionReturn` arm,
unconditional for a closure since B122's near-miss restored it) catches on
its own. The bug and its mask are independent facts; B123 is the lane that
had to tell them apart before touching anything.

### 14.1 The un-masking pin comes first

A masked bug is not evidence of a safe bug — only evidence that every
*existing* probe happens to trip a second mechanism first. The brief called
for constructing the disagreeing case before writing any fix: a view leaving
through one arm of a closure's conditional TAIL, with no `ret` anywhere in
the program, so the per-expr arm has nothing to catch and only the
whole-block question is left to answer.

```vilan
import std::print;
struct Inner { n: i32 }
fun main() {
    let grab = |flag: bool| {
        let local = Inner { n = 3 };
        if flag { Inner { n = 0 } } else { &local }
    };
    print(grab(false).n);
}
```

Built and run against the live (pre-fix) compiler: it compiled clean. The
emitted JS confirms what the checker missed — `local` is the same array
object handed back through the `else` arm, an alias exactly as unsound in
kind as the function-level `grab` §13.1 already proved rule 3 exists to
refuse:

```js
const grab = (flag) => {
	const local = [ 3 ];
	let $a = null;
	if (flag) { $a = [ 0 ]; } else { $a = local; }
	return $a;
};
```

The identical shape spelled with an early `ret &local;` in place of the
conditional tail was already refused, confirming the two spellings of one
expression disagreed — the same shape of disagreement §13's `grab` had, one
level further down the reader stack. This is the pin
(`b123_a_closure_conditional_tail_arm_may_not_escape_a_view_of_a_closure_local`),
planted `#[ignore]`d and confirmed red before any fix line was written. A
second construction — a captured place (`&h.inner`, `h` an owned local in the
enclosing function) escaping the same way — reproduced the identical hole,
confirming the un-masking is not specific to a closure-local's storage but to
the conditional-tail SHAPE itself, exactly as `is_view_expr`'s failure to
match `Expr::If` predicts. **Verdict: the hole is real, not merely filed.**

### 14.2 The fix: the closure seam's leaf walk, no exemption

`closure.return_` is a closure's own tail id, positionally identical to a
function's `function.body.1` — both are what `walk_expr_node` returns for the
body, so both terminate at a `Block`'s tail, an `if`'s arms, or a `match`'s
legs. `collect_tail_leaves`, unchanged since B122, already walks exactly that
shape regardless of which seam owns the id. The fix asks it of
`closure.return_` the way the function seam asks it of `function.body.1` and
each `return_sites` entry, then asks `escapes_as_view` of each leaf on its
own instead of once for the whole position:

```rust
for return_id in closure_returns {
    let mut leaves = Vec::new();
    self.collect_tail_leaves(return_id, &mut leaves);
    for leaf in leaves {
        if self.escapes_as_view(leaf, &view_bindings, &capturing) {
            escapes.push(leaf);
        }
    }
}
```

One asymmetry from the function seam, by design rather than omission: no
`return_position_hands_back_no_view` exemption is asked afterward. That
exemption exists because a `borrows` function may soundly return a view of a
loaned parameter; a closure may not declare `borrows` at all (P4c — a closure
that captures a view is second-class all the way, and cannot hand one back
either), so it has no sound view leaf to exempt. Every leaf the walk finds
that is a view is unconditionally an escape, matching the per-expr `ret` arm
it now agrees with.

### 14.3 Coverage

`b123_a_closure_conditional_tail_arm_may_not_escape_a_view_of_a_closure_local`
is the un-masking pin (§14.1), un-`#[ignore]`d once the fix lands. Six more
round out the semantics the two constructions in §14.1 only sampled:

- `b123_a_closure_ret_and_conditional_tail_arm_agree_refusing_a_view_of_a_closure_local` —
  the REFUSE-direction agreement pin (B116/B122 style): the `ret` spelling
  and the conditional-tail spelling of the identical view-of-a-closure-local
  now answer identically. The `ret` side was never broken (§13.4's near-miss
  already restored it); what changes here is that the tail side stops
  disagreeing with it.
- `b123_a_closure_ret_and_conditional_tail_arm_agree_accepting_an_owned_leaf` —
  the ACCEPT-direction counterpart, and where the closure seam's agreement
  pin necessarily differs from the function seam's
  (`b122_a_ret_beside_an_owned_tail_agrees_with_the_conditional_tail`, which
  pins two spellings *accepting a sound view*). No such case exists for a
  closure — §14.2's whole point is that it has no sound view to accept — so
  the accept side pins the neutral case instead: an OWNED leaf, which
  `escapes_as_view` was never going to flag through either spelling, still
  compiles through both after the leaf walk widens what gets ASKED. This is
  what the independence plant (below) confirms does not depend on the fix.
- `b123_a_closure_conditional_tail_arm_order_does_not_matter` — the
  view-of-a-local arm first, the owned arm second.
- `b123_a_nested_closure_conditional_arm_may_not_escape_a_view_of_a_closure_local` —
  a view two `if`s deep, reached through `collect_tail_leaves_if`'s existing
  recursion.
- `b123_a_closure_match_leg_may_not_escape_a_view_of_a_closure_local` — the
  identical shape as a `match` tail.
- `b123_a_mixed_leaf_closure_return_refuses_each_forbidden_view_leaf_separately` —
  the closure-domain shape of B122's mixed-leaf pin, and where it necessarily
  diverges: `b122_a_mixed_leaf_return_refuses_only_the_local_view_leaf` mixes
  one SOUND leaf (a parameter view) with one unsound leaf and refuses exactly
  once, because the function seam's exemption tells the two apart. A closure
  has no exemption to tell them apart with — a captured-place view and a
  closure-local view sitting in sibling arms of the same three-way
  conditional are BOTH forbidden, so the walk must refuse both, separately,
  each naming its own arm's span (`&h.inner`, `&local`) rather than
  collapsing into one diagnostic or naming the enclosing `if`. Asserted as
  exactly two matching diagnostics, not one and not merged.

Every B122 pin and the pre-existing `a_closures_ret_still_cannot_hand_back_a_view`
stayed green throughout (2259 passed, 0 failed, 3 `#[ignore]`d, none of them
B123's, for the full `inference` binary).

Two plants:

| plant | red |
|---|---|
| the un-masking pin, run against the live compiler before any fix line existed | 1 (planted `#[ignore]`d, confirmed via `cargo test -p vilan-core --test inference … -- --ignored`) |
| the closure seam's leaf walk reverted to the old whole-position `escapes_as_view(closure.return_)` call, all seven pins left in place | 6 of the 7 new pins (every one that names a view leaf); the accept-agreement pin stays green, correctly — an owned leaf was never going to be flagged by either the whole-position question or the leaf walk, so it is not evidence for or against this fix |

The second plant is also the independence proof the brief asked for: with
the closure seam reverted, every B122 function-seam pin (6) and the
pre-existing `a_closures_ret_still_cannot_hand_back_a_view` stayed green —
the closure fix touches nothing the function seam or the per-expr `ret` arm
reads, so reverting it cannot and does not redden either.

### 14.4 Zero movement, checker-only

`cargo test -p vilan-cli --test corpus` moved **zero bytes** (`every_corpus_golden_is_byte_identical`
green) and `cargo test -p vilan-core --test docs` compiled every fenced
example unchanged (`every_doc_example_compiles` green) — the regression
corpus and the documented surface contain neither the closure-local nor the
captured-place shape, so the fix is provably inert against everything
shipped. `cargo build` and the full `inference` binary are both clean
(§14.3). B123 closes SHIPPED: the un-masking construction in §14.1 is the
finding this record required before a fix could be justified — the hole was
real, not a hypothetical extension of B122's rule.
