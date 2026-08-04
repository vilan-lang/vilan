# A consuming call is a move — closing B60

> **Status: SHIPPED 2026-08-04** (backlog B60, found by the B53 follow-up arc
> and recorded in `capture-clones.md` §5; §7 adds B62, the residual §6 filed).
> `o.unwrap()` consumed the option's
> payload but the affine checker recorded no move, so `o.is_some()` afterwards
> compiled clean and `o`'s scope-end teardown still fired: one resource value
> destroyed twice. This is the record — what the bug actually was, the rule
> that closes it, the edge-case rulings and the precedent each follows, and
> what is deliberately left open.

## 1. The bug was a mis-declared signature, not a missing analysis

B60 was filed as "a self-by-value call is not a move to the affine checker",
on the reading that `Option::unwrap(self): T` has a by-value receiver the
checker was ignoring. That reading is wrong, and getting it right is the
whole fix.

In vilan a **bare `self` receiver is a LOAN**, in the same bucket as `&self`
and `&mut self`. `docs/spec/memory.md` R3 says so directly ("`self` / `&x` /
`&mut x` are loans, unchanged; `own x` is a move"), the tour says so
(`db.exec(..)` never consumes `db`), and the checker already agrees:
`scan_move`'s `Expr::Call` arm asks `callee_conventions` for the callee's
declared conventions — **including the receiver at index 0** — and moves the
argument exactly when that convention is `Convention::Own`. A method
declared `own self` therefore ALREADY moved its receiver, correctly, before
this arc; `r3_own_self_receiver_moves_the_subject` pins it.

The census is decisive on which reading is right: across all `.vl` in the
repo there are **973 bare `self` receivers** against 11 `own self`. Widening
bare `self` to a move is not an option — planting exactly that change reddens
`r3_bare_self_receiver_stays_a_loan` plus six real `Database` tests
(`a_database_round_trips_inserts_and_queries`,
`a_module_level_database_is_accepted`, …), because every `db.exec(..)` /
`db.prepare(..)` call site in std, the corpus, and the docs depends on the
loan.

So the real defect: **`unwrap` declared a loaned receiver and then moved the
payload out of it.**

```vilan,fragment
fun unwrap(self): T {              // `self` is a LOAN
    match self {                   // R6: matching by value CONSUMES the subject
        Some(let x) => x,          // …and hands the payload to the caller
        _ => panic("expected Some but got None"),
    }
}
```

The caller loaned `o` and therefore still owns it: it stays live, stays
readable, and still drops at scope end — while the caller also holds the
payload that just left it. Two owners, one value, two destructions. The
checker never objected because *nothing checked that a loan may not be
consumed*. That hole is general, not a `self` question at all — the same
program with a bare non-`self` parameter had the same hole:

```vilan,fragment
fun steal(d: Db): Db { d }         // `d` is a loan; returning it moves it out
```

## 2. The rule

Two halves, both small, neither a new move system.

**(a) A body may only consume what it owns.** A consuming use of a
resource-typed parameter held by a LOAN convention (`self` / bare / `&` /
`&mut`) is an error. `own` is the only convention a body may move out of.
This is R3 read in the direction nobody had enforced: a loan changes no
ownership, so ownership cannot leave through one.

Implementation is one predicate and one early return in `scan_move_touch`
(`analyzer.rs`), placed beside the module-level-resource case it mirrors —
report, and leave the binding OWNED so a later loan of the same parameter
does not cascade into a spurious use-after-move:

```rust
fn binding_is_loaned_parameter(&self, binding: Id) -> bool {
    self.parameters
        .get(&binding)
        .is_some_and(|parameter| parameter.convention != Convention::Own)
}
```

It rides both existing scans unchanged: the concrete one, and R11's
per-instantiation `scan_instantiated_body`, which reuses `scan_move` verbatim
against the delta-resource place set. So the generic case — `Option<Db>`'s
`unwrap`, whose `self` is only a resource *at this instantiation* — is
reported at the instantiation site with a note into the std body, using the
`resource_triggering_constraints` machinery the B53 arc built. No new
per-instantiation plumbing was needed.

**(b) The consuming std combinators declare `own self`.** With (a) in place
the compiler performs its own audit: every method that moves a payload out of
a loaned receiver now reports. `Option::unwrap` becomes `unwrap(own self)`,
and the call site is a move through the accounting that already existed.

`is_some` / `is_none` are untouched — their bodies are `self is Some(_)`, a
test, not a consuming `match` — so the predicates stay free on a resource.

## 3. What "consumed" means downstream — and every edge case

Nothing below is new code. Once the call is an `own` argument, each shape is
decided by the rule that already governs `own` arguments, which is exactly
why this was the right fix. Every ruling matches its precedent verbatim,
including the diagnostic text.

| Shape | Ruling | Precedent | Pin |
|---|---|---|---|
| Later use of the moved-from place | error, `use of \`o\` after it was moved`, note at the call | R1 / R3, `scan_move_touch`'s `MoveState::Moved` arm — the same diagnostic `let b = a` and `sink(a)` raise | `b60_a_consuming_call_kills_the_source_binding` |
| Scope-end teardown of the moved-from place | suppressed | `plan_expr`'s `Expr::Local` arm removes a consumed binding from `owned`, so it never reaches `dropped` | `a_moved_resource_instantiation_destroys_one_value` |
| Consuming call in ONE branch | error, `moved on one path through this branch but not another` | **R7**, `scan_move_branches` — unchanged; end-of-scope ownership must be static, there are no runtime drop flags | `b60_a_consuming_call_in_one_branch_is_a_conditional_move` |
| Consuming call in a loop body | error, `declared outside this loop and moved inside it` | **R8**, the `loop_depth > decl_loop_depth` test — rejects syntactically on the first pass, no second-iteration fixpoint | `b60_a_consuming_call_in_a_loop_is_rejected` |
| Consuming call on a field (`s.opt.unwrap()`) | error, `no partial moves` | **R5** — identical to `own`-passing the field; v1 has no partial moves, `Option::take` is the sanctioned way out of a live aggregate | `b60_a_consuming_call_on_a_field_is_a_partial_move` |
| Re-initialization after the move, `mut` binding | **allowed** | the binding-move precedent: `scan_move`'s assignment arm re-owns unconditionally, and the drop planner records an overwrite-drop only when the binding was still `owned` — so the moved-out value is not dropped twice | `b60_reinitialization_after_a_consuming_call_compiles` |
| A non-resource (`Option<i32>`) | **completely unaffected** | rule 1: `own` COPIES for data, so `o.is_some()` after `o.unwrap()` stays legal and correct | `b60_a_data_option_is_unaffected_by_the_consuming_call` |

One consequence worth stating because it changes an idiom: `if
(opt.is_some()) { opt.unwrap() }` on a resource is now **rejected** as an R7
conditional move. That is correct and is the bug — before the check it
compiled and destroyed the payload twice. The idiom for a resource is
`match opt { Some(let value) => .., None => .. }`, which consumes on every
path, or `opt.take()`.

## 4. Compat check

The survey ran over std (`.vl`), `vilan/test` (107 corpus programs),
`vilan/examples`, `vilan/benchmarks`, `macro_std`, the CLI templates, every
fenced `vilan` block under `vilan/docs/`, and the inline programs in
`inference.rs`.

The entire resource footprint of real vilan source is **two std types**
(`Database`, `OwnedNursery`) plus four corpus files. Neither std resource has
an `own self` method; every `Database` call site reaches it by loan. **No
existing call site is flagged**, and the corpus goldens are byte-identical
apart from the one file in §5.

The compiler's own audit of std (running the new rule) found the latent bugs:
`unwrap`, `is_none_or`, `unwrap_or_else`, `map`, `map_or`, `map_or_else`,
`map_or_default`, `ok_or_else`, `and`, `and_then`, `filter`, `zip`,
`unwrap_or_default`, `transpose`, `flatten` — each moves a payload out of a
loan — plus `bool::then_some`'s `value`, `and`'s `b` and `zip`'s `peer`. All
are now `own`.

## 5. The one golden that moved, and why

`vilan/test/closure-param-inference.js` gains one `__clone` inside
monomorphized `Option::map`. Runtime output is byte-identical (verified:
`7 / true / 3 / 1` before and after).

The cause is B53's **SHARE elision**, which requires the capture's subject to
be readonly-rooted (`readonly_root`). A bare `self` parameter qualifies; an
`own` one does not, so the capture falls back to the conservative copy. The
copy is unnecessary here (the capture is immutable and does not escape), but
it is not wrong, and the fix belongs to the elision, not to this arc:
`readonly_root` is a *diagnostic* helper reused for a semantic decision, and
it cannot simply be taught about `own` because `mut own x` is a parse error,
so the "declare it `mut`" hint it would have to return names an impossible
fix. Separating the elision predicate from the diagnostic helper is its own
slice (§6).

This is also why four combinators were **not** converted (below): they cost
the same elision on data paths that no resource program pays for.

## 6. Residuals

- **`map`, `is_some_and`, `ok_or`, `unzip` at a resource instantiation are
  now a compile error rather than a silent double-destroy.** `map` was
  converted (it has a pinned spec behaviour,
  `r11_std_option_map_at_a_resource_accept`) at the cost of §5's golden; the
  other three were left, because converting each costs the same lost SHARE
  elision on data call sites and no pin asserts them clean at a resource. The
  conservative error is the honest state. Unblocked by §5's elision slice.
- **`or`, `or_else`, `xor`, `inspect`, `eq`, `unwrap_or` cannot be made
  move-clean at all under R6.** Each reads `self` twice (`match self { Some(_)
  => self, .. }`) or duplicates the payload, and R6 consumes the subject on
  ANY by-value match, so there is no `own self` spelling that passes. They
  reject at a resource instantiation. Rewriting them over `is` tests (which
  loan) would fix it and is a std slice, not a checker one.
- **A bare non-`self` resource parameter is a loan, per R3 — but §6.3's
  convention table calls bare `x: T` "by value (a copy, rule 1)".** The two
  readings disagree for resources. This arc enforces R3's (bare = loan, and
  consuming it is now an error), which is what the implementation and every
  call site already assumed. Worth reconciling in the spec text.
- **A `match` arm capture of a resource payload is never destroyed.** `match
  o { Some(let r) => print(r.tag), None => {} }` prints no drop for `r`.
  Verified **pre-existing** against the v0.24.0 release, unrelated to this
  arc — the drop planner does not seed match-arm captures as owned. Found
  while checking the idiom §3 steers users toward, which makes it the more
  urgent for being the recommended path. **CLOSED 2026-08-04 (B62); §7 is
  the record.**

## 7. A pattern capture that owns a payload is destroyed — closing B62

> **Status: SHIPPED 2026-08-04** (backlog B62, the §6 residual). The bug is
> a *leak*, not a double-drop, and it sits on the path §3 steers every
> resource user onto — which is what made it urgent rather than merely
> filed.

### 7.1 The bug, and why the subject's side was already right

R6: matching by value CONSUMES the subject. The drop planner implements
that half correctly — `plan_match` walks the subject as consuming, so
`plan_expr`'s `Local` arm removes it from `owned` and its scope-end
teardown never fires. What it never implemented is the other half of the
same sentence, *"pattern captures move the payloads into the arm"*: the
capture was bound as a JS `const` and enrolled nowhere.

So the sum was **zero** drops, not two:

```js
const o = [ 0, [ "payload" ] ];
if ($a[0] === 0) { const r = $a[1]; console.log("leg " + r[0]); }
// nothing destroys r, and nothing destroys o either
```

`plan_expr`'s `Destructure` arm carried the same hole with a comment
recording it (*"a captured resource that is never re-moved leaks in v1"*),
so `let (r, n) = pair` leaked identically. Both are one root cause and are
fixed together; a symptom-level patch to `match` alone would have left the
twin.

### 7.2 The rule: who enrolls, and who must not

**A capture of a CONSUMED subject owns its payload and joins the scope's
owned set, exactly like a `let`.** It drops at that scope's end unless
moved onward. Nothing else is new — every downstream question is answered
by the machinery `let` bindings already ride.

**A capture of a LOANED subject enrolls nothing.** This is the half that
had to be got exactly right, because enrolling here destroys one value
twice:

| shape | subject | capture | why |
|---|---|---|---|
| `match o { Some(let r) => … }` | consumed (R6) | **owns** | the subject's teardown is suppressed; the capture is the only owner |
| `match &o { Some(let r) => … }` | loaned (R6) | loans | the subject still owns and still drops |
| `o is Some(let r)` | loaned | loans | a *test*, not a consuming match — `is_some`'s body is exactly this, and B60 left it free on a resource |
| `let (r, n) = pair` | consumed | **owns** | a full destructure, not R5's partial move |
| `let (r, n) = &pair` | loaned | loans | as `match &o` |
| any data capture | either | n/a | not in the resource set; the planner never sees it |

The B53 SHARE elision is **not** a hazard here and cannot become one:
`compute_capture_clone_sites` phase 2 filters `type_is_resource` *before*
the share check, so a concrete resource capture is never in the shared set.
It is never copied either (R1), so it is always a plain alias into a
subject that the match consumed — one owner, by construction.

Generic captures are out of scope for the same reason `plan_resource_drops`
does not seed a generic `own T` parameter: `Generic(T)` is not a resource in
the base classification, a generic body is emitted once, and R11 requires the
value to be moved out instead. §7.5 records that R11 does not actually check
captures yet.

### 7.3 One drop per value, on every path

The argument is short because every piece is an existing precedent:

- **The subject cannot also drop.** Consuming it removes it from `owned`
  before any leg is planned (`plan_expr`'s `Local` arm) — the same
  suppression B60 relies on for `o.unwrap()`.
- **A moved-on capture cannot also drop at the leg.** The leg's tail is
  walked as consuming, so a returned/`own`-passed/stored capture leaves
  `owned` and the sweep skips it. `drop(c)` is an `own` argument, so the
  conditional-teardown idiom is untouched.
- **A capture cannot drop on one path and not another.** R7 already
  governed captures — `scan_move_match` seeds them into the affine flow —
  so `if flag { sink(r) }` inside a leg is rejected, and end-of-scope
  ownership stays static. No runtime drop flags.
- **A rejecting guard destroys nothing.** The teardown wraps the leg BODY,
  which a rejected guard never enters — B59's ordering decision (*a guard is
  a decision procedure; a rejected leg must leave no trace*) read for
  destruction, and it falls out of the emission site rather than needing a
  rule.
- **A leg that never runs destroys nothing**, for the same reason.
- **R2 still fires.** Overwriting a `mut` capture that still owns drops the
  old value at the assignment; the leg drops the new one.

### 7.4 Where the code lives

| piece | site |
|---|---|
| a scope's entry-owned captures | `analyzer.rs::plan_scope` (`captures` parameter) |
| a leg's captures, gated on the subject | `analyzer.rs::plan_match` + `pattern_subject_is_loan` |
| a destructure's captures | `analyzer.rs::plan_expr`'s `Destructure` arm |
| the arm carrier | `analyzer.rs::PlanArm` (replaces the `(statements, tail)` pair) |
| the leg's `try`/`finally` | `transformer.rs::Expr::Match`, via `capture_drop_nodes` |
| a destructure statement's | `transformer.rs::ScopeTeardown` + `walk_scope_body` |

Two emission details worth stating. A **guarded** leg's capture is still an
accessor (`$z[1]`), because B53's `materialize_capture_clones` only
materializes captures that *copy* and a resource never copies — so the
teardown destroys through the accessor, which names a slot the match
already consumed. And a leg or scope owing **nothing** splices its body in
unchanged, which is what keeps every data pattern and every resource-free
program byte-identical.

### 7.5 Pins, compat, and what is left open

Twenty-three pins in `inference.rs`, twelve of them red against the
pre-fix tree (proven by planting `droppable_pattern_captures` back out);
the other eleven pin what must not change and are correctly insensitive to
that plant. `vilan/test/resource_take.vl` gains four functions pinning the
emitted SHAPES in bytes — the leg teardown, two captures in reverse order,
a guarded leg called both ways, and the destructure — because the fix
otherwise left every golden in the tree byte-identical.

**Compat: no existing source changes behaviour.** The whole tree contains
exactly three resource captures — `resource_take.vl:49,55` (`Some(let c)
=> drop(c)`), `resource_take.vl:77` (`Holder::Full(let inner) => inner`),
and `docs/tour/resources.md:150` (`Some(let g) => drop(g)`) — and every
one of them moves its capture onward, so none was leaking and none now
drops twice. The `Database`/`Row` captures across std, the examples, and
the docs bind `Row`/`str`, which are not resources. The corpus golden diff
is purely additive.

Three residuals, each a **different rule** from this one, pinned
`#[ignore]`d and verified pre-existing:

- **Consuming a loaned capture double-destroys.** `if o is Some(let r) {
  sink(r) }` prints `sink ic / drop ic / after / drop ic`; `match &o {
  Some(let r) => sink(r) }` does the same. This is B60's rule (a body may
  only consume what it owns) in the *capture* position rather than the
  parameter position — `scan_move_touch`'s `binding_is_loaned_parameter`
  has no capture twin. It wants its own diagnostic and steer ("match by
  value, or `take`"), which is why it is not folded in here.
  `b62_an_is_capture_consumed_by_an_own_call_is_rejected`,
  `b62_a_loaned_match_capture_consumed_by_an_own_call_is_rejected`.
- **A generic body's capture leaks at a resource instantiation.**
  `fun peek<type T>(own o: Option<T>) { match o { Some(let v) => …, None
  => … } }` at `T := Res` prints no drop. `check_own_generic_exactly_once`
  asks only about `own` parameters, and the match consumes `o`, so the
  parameter passes and the capture is never asked about. The fix is to
  treat a still-owned capture at the fall-through end the way that check
  treats a still-owned parameter.
  `b62_a_generic_capture_never_moved_out_is_rejected_at_a_resource_instantiation`.
