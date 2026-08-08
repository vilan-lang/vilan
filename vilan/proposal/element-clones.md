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

**(b) ships.**

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
