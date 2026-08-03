# Requirement polymorphism — the coverage fence follows instantiation chains

Status: **BOTH SLICES SHIPPED 2026-08-02** (backlog B51) — the recorded
follow-up refactor from the H8 residual fix (`element-syntax.md` §6, backlog
H8, commit `8d6980e`), plus two soundness holes its design recon surfaced,
both proven with red probes before this document was written. S1 (the holes)
landed as `f7dcb66`; S2 (the walk) follows it on the same branch. §7 holds
the ship record.

## 1. The problem, in three shapes

The context pass (`context.rs`) decides one question statically: **can a
strict context read be reached without an enclosing `run`?** Commit `8d6980e`
made that decision instantiation-sensitive one level deep: at an
`OnConstraint` dispatch site, each incoming call's recorded substitution
selects only the impl members its concrete binding reaches. Three shapes
remain wrong — one too tight, two too loose:

**(a) The forwarder fence — too tight (the recorded follow-up).** A generic
forwarding wrapper re-exports the constraint:

```
fun wrap<T: Slot>(content: T): View {
    view("p").child(content)
}
mount("app", wrap("static"));    // fenced today; nothing here subscribes
```

Inside `wrap`, the call `child(content)` binds `child`'s `C` to
`Generic(T_wrap)` — the resolution chase looks `T_wrap` up in *that same
call's* substitution, finds nothing, and falls back to the union, so the
`Signal` arm's requirement fences a `str` instantiation. The data to do
better already exists: `wrap`'s own incoming calls record `T_wrap → str` in
`method_call_substitution` — one edge further up the graph. The transformer
already composes substitutions this way (`inherited_substitution`); coverage
does not.

**(b) The closure-owned site — a soundness hole shipped in v0.21.1 (an
`8d6980e` regression).** Proven by probe: this **compiles today** and breaks
silently at runtime (the effect registers against an undefined owner):

```
fun wrap<T: Slot>(content: T): View {
    let holder = view("p");
    let attach = || content.place(holder);   // OnConstraint site, owner = the closure
    attach();
    holder
}
mount("app", wrap(Signal::new("live")));     // no boundary anywhere — accepted!
```

The refined site loop enumerates entries into the site's owner via
`incoming_calls` / `top_level_incoming`, which only hold `CallTarget::Function`
edges. A **closure** owner has neither, and none of the four fallback arms
covers it, so the site contributes *no* coverage edges at all — the `Signal`
impl's `place` is then exempt as dead code. v0.20.0 fenced this shape (the
union edges included closure-owned sites); the refinement dropped it.

**(c) The mixed-entry hole — pre-existing, all the way back.** Proven by
probe against both HEAD and v0.20.0's arm shape: this **compiles today** and
passes `undefined` for the hidden bare parameter at the top-level call:

```
fun needy() { Signal::new(1).effect(|v| {}); }
fun covered() { run_with_owner(Owner::new(), || needy()); }
fun main() { covered(); }
main();
needy();                                     // uncovered entry — accepted!
```

`covered`-ness of a function with caller edges is `all(callers ∈ bound)` —
the top-level entry is only consulted in the *no-edges* branch. One covered
caller launders any number of uncovered top-level entries. (`value_taken`
has the same blind spot in that branch, but a needy function used as a value
is already rejected by its own diagnostic, so the top-level entry is the live
hole.)

## 2. What does not change

- **`needs`, `strict`, and threading stay union-based.** The threading
  rewrite runs pre-monomorphization and appends real parameters/arguments in
  place; a per-instantiation `needs` set would fork a function's arity across
  instantiations, which the transformer cannot express (`function.parameters`
  is per-function; `argument_ids` is per-call-id). The escape valve stays
  what it is today: an instantiation that doesn't need the value receives
  `undefined` and never reads it — coverage is what guarantees the never.
- **`OnType` sites keep the union.** Narrowing concrete-receiver dispatch is
  the separate tightening already recorded in `context.rs`. *(Superseded —
  shipped post-arc as §8.)*
- **The diagnostic text and anchoring are untouched.** Anchoring the fence at
  the uncovered root instead of the std-internal `get()` stays the
  `ambient-owner.md` §4 follow-up.
- **Injected (`context`-typed) closures are out of scope** — their coverage
  rules (`ambient-owner.md` §5) are call-site-exact already.

## 3. The semantics

Coverage's refined dispatch edges are built by a **resolution walk** instead
of the current one-level lookup. For an `OnConstraint(constraint, member)`
dispatch site with candidate impl members:

1. **Site owner normalization.** Hop `closure_parent_of` from the site's
   owner up to the nearest enclosing *function* `F` — a closure monomorphizes
   with its parent's substitution, and its coverage is already defined by its
   parent's boundness. (This closes hole (b): the closure-owned site resolves
   exactly as if the call were written in `F`'s body.)
2. **Entry enumeration at `F`.** If `F` is value-taken or
   dispatch-reachable, the site falls back to the union — its entries cannot
   be enumerated. Otherwise its entries are `incoming_calls[F]` plus
   `top_level_incoming[F]`. A `(F, constraint)` pair the walk has already
   processed is **skipped, exactly**: its edge contribution is a function of
   the pair alone, so a revisit re-derives identical edges — recursion
   (self- or mutual) needs no fallback and no depth cap; the visited set
   bounds the walk.
3. **Per-entry chase.** For each entry call, resolve the constraint through
   the call's recorded `method_call_substitution` — the single channel every
   instantiation shape records into, explicit generic arguments included (a
   merged explicit-args channel was implemented and proven dead by plant:
   disabling it changed no pin, so it was removed) — chasing `Generic` links
   within the map as today:
   - **Concrete resolution** → `impl_members_for` selects the candidates;
     the edge attaches to *this entry's* caller node — or to the
     outside-entered set for a top-level entry.
   - **`Generic(P)`** → recurse from step 1 with the entry's enclosing
     function and `P` (a closure entry hops to its parent function — `P` can
     only be a function's parameter).
   - **Anything else unresolvable** (`Any`/`Unknown`/`Unresolved`/`Trait`,
     or no recorded bindings) → *per-entry* fallback: every candidate gets an
     edge from this entry's caller (or joins the outside-entered set for a
     top-level entry) — conservative for exactly the paths through this
     entry, refined edges elsewhere untouched.

**Edge attribution is the outermost resolving caller**, not the forwarder:
for `F → wrap → child → place`, the `Signal` arm's edge attaches to `F`.
This is what keeps mixed instantiations independent — an uncovered *static*
call through `wrap` must not poison a covered *Signal* call through the same
`wrap` (the forwarder's own boundness is call-site-insensitive, so attaching
to it would conflate the paths).

**Outside entries always fence (closes hole (c)).** `covered` for a function
becomes: outside-entered (refined set, top-level-called, or value-taken) →
uncovered, regardless of caller edges; no edges at all → the dead-code
exemption as today; otherwise all callers bound. Hoisting the outside checks
above the branch is the whole fix — the no-edges branch already had them.

## 4. Soundness

Every dynamic path that reaches a strict `get` enters the graph somewhere
and selects impl members by its instantiation chain. The walk enumerates
entries per level; every entry either resolves concretely (edge to that
path's outermost caller — bound-ness of exactly the node whose coverage
decides whether the value is present on that path) or falls back to the
union (edge to the site owner, whose coverage then requires *all* its
callers bound — the pre-refinement conservative shape). Enumeration
completeness is guarded per level: a function whose entries cannot be
enumerated (value-taken, dispatch-reached, cycle, depth) unions. Top-level
entries mark the resolved candidates outside-entered — uncovered by
definition. So no path exists whose candidates lack either a refined edge to
its own entry point or a union edge; precision only ever removes edges whose
paths provably bind a different impl.

## 5. Slices

- **S1 — make the fence sound again** (the two holes; release-urgent since
  (b) shipped in v0.21.1 *and* v0.22.0): closure-owner normalization *as a
  fallback check* (a closure-owned site unions, restoring v0.20.0's behavior
  for shape (b)) and the outside-entry hoist for shape (c). Both pinned
  red-first. Fence strictly tightens; no user-visible loosening.
- **S2 — the walk** (requirement polymorphism proper): the recursive
  resolution replacing the one-level `refined_for_call`, subsuming S1's
  closure fallback with real precision (a closure-owned site resolves
  through its parent chain instead of unioning), the explicit-generic-args
  channel, and the forwarder pin flipped from `expect_err` to `expect`.

## 6. Pins (per case, red-first where behavior changes)

S1: (b) closure-owned site + `Signal`, uncovered → error (red first);
(b′) closure-owned site + `str`, uncovered → still fenced under S1
(conservative), flips under S2; (c) mixed covered-caller + top-level entry →
error (red first); (c′) covered-caller-only control stays compiling.

S2: the forwarder pin flips (`wrap("static")` uncovered compiles — red
first against S1); `wrap(Signal::new(…))` uncovered stays fenced;
`wrap(sig)` under a boundary plus `wrap("static")` at top level both in one
program compile (edge attribution — red first, this is the shape one-level
composition gets wrong); two-level forwarder (`outer<U: Slot>` → `wrap<T>`)
static compiles / `Signal` fences; a self-recursive generic forwarder with a
static slot compiles and with a `Signal` fences (the visited-skip is exact,
not a fallback); explicit type arguments (`wrap<str>("static")`) compile
unfenced; closure-owned site + static slot compiles (b′ flip); the five
existing `8d6980e` pins unchanged.

## 7. Ship record (2026-08-02)

- **S1** — commit `f7dcb66`: the closure-owner union fallback and the
  outside-entry hoist. Four pins, three proven red first (the static-slot
  closure-owned pin recorded S1's deliberate conservatism and flipped under
  S2 as planned).
- **S2** — the walk, same branch: `resolve_through` (the within-map chase,
  classifying Concrete / Parameter / Opaque), `enclosing_function` (closure →
  parent hop), and a per-site worklist over `(function, constraint)` pairs
  with an exact visited-skip. Per-entry fallbacks attach every candidate to
  that entry's caller; only value-taken / dispatch-reachable levels union
  whole-site. Two flips proven red against S1; the edge-attribution pin
  proven by plant (attributing to the forwarder reds it); the
  explicit-generic-args pin kept as a behavior pin after its dedicated
  channel was proven dead by plant and removed (§3). Nine S2 pins; 1216
  inference pins green.
- **Deviations from the draft**: the cycle guard became an exact visited-skip
  (no depth cap, no fallback — a revisit re-derives identical edges); the
  explicit-args channel was dropped as dead; per-entry fallback replaced the
  draft's whole-site union for unresolvable bindings (strictly more precise,
  same soundness argument).
- **Still open**: nothing — `OnType` narrowing shipped post-arc (§8);
  requirement polymorphism for *injected closures* was never in scope (§2).

## 8. OnType narrowing (shipped post-arc, 2026-08-02)

The separate tightening, taken as its own small slice after v0.22.1. An
`OnType` dispatch record is written at exactly three shapes: a concrete
receiver whose method resolves to an *inherited trait default* (Gap E), the
operator twin of the same shape, and a `self` call inside a shared trait
default body (receiver `None`). The coverage union is *name*-keyed across
every trait declaring the member, so an unrelated needy impl under the same
member name spuriously fenced a concrete receiver's static inherited default
— proven by red probe (`5.verdict()` fenced because `str`'s unrelated
`verdict` subscribes; the probe's first draft was itself a demonstration,
colliding with a std member name and pulling in `Serialize` bounds).

The narrowing: a site with a recorded receiver selects
`impl_members_for(receiver, member)` and draws edges from the site's owner —
no entry enumeration, because the receiver's **head** cannot change under
substitution and the head is what selects among candidates (the matcher is
argument-insensitive by design, which makes `List<T>` receivers narrow
soundly to List-headed impls). A receiver resolving to a generic or opaque
type, an empty selection, and every receiver-less site keep the union.
`needs`/`strict`/threading are untouched, as everywhere in this proposal.

Three pins, one proven red first (the spurious fence), two guards (the
receiver's own needy default stays fenced; a needy impl reached through a
default-body `self` call stays fenced).
