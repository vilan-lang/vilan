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

> **Closed 2026-08-04 by B63(a) — §7.** The golden is back to its pre-B60
> form; the line above is kept as the record of why it moved at all.

## 6. Residuals

All four are settled below in §7 except the last, which is B62's.

- **`map`, `is_some_and`, `ok_or`, `unzip` at a resource instantiation are
  now a compile error rather than a silent double-destroy.** `map` was
  converted (it has a pinned spec behaviour,
  `r11_std_option_map_at_a_resource_accept`) at the cost of §5's golden; the
  other three were left, because converting each costs the same lost SHARE
  elision on data call sites and no pin asserts them clean at a resource. The
  conservative error is the honest state. Unblocked by §5's elision slice.
  **Closed by §7.**
- **`or`, `or_else`, `xor`, `inspect`, `eq`, `unwrap_or` cannot be made
  move-clean at all under R6.** Each reads `self` twice (`match self { Some(_)
  => self, .. }`) or duplicates the payload, and R6 consumes the subject on
  ANY by-value match, so there is no `own self` spelling that passes. They
  reject at a resource instantiation. Rewriting them over `is` tests (which
  loan) would fix it and is a std slice, not a checker one. **Closed by §7 —
  and this bullet was wrong twice: `eq` never rejected, and the three that
  still reject do not reject for the reason given here.**
- **A bare non-`self` resource parameter is a loan, per R3 — but §6.3's
  convention table calls bare `x: T` "by value (a copy, rule 1)".** The two
  readings disagree for resources. This arc enforces R3's (bare = loan, and
  consuming it is now an error), which is what the implementation and every
  call site already assumed. Worth reconciling in the spec text. **Closed by
  §7.**
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

## 8. B63 — the residuals closed, 2026-08-04

> **Status: SHIPPED.** Three parts: the elision blocker, the three
> conversions, and a rewrite of the six over `is` tests that decides each on
> its own merits instead of as a block. Plus §6.3's table, reconciled.

### 8.1 The blocker: a diagnostic helper was answering a semantic question

`readonly_root` answers *"what `declare it …` advice applies to this place"*.
The share elision asked it *"can this place change"*. Those coincide for
every root the diagnostic knows how to advise about and diverge for exactly
one: an `own` parameter, where the advice would be `mut own x` — a parse
error — so `readonly_root` must answer `None`, and the elision read that as
"writable" and copied.

Split, not widened. `share_subject_is_stable` is the semantic predicate: a
declared-readonly root (bare parameter, `&` view, immutable `let`) qualifies
*because the compiler rejects every write to it*, and an `own` root qualifies
when no write reaches it. The second clause is not decoration — **an `own`
parameter is genuinely writable** (`fun f(own h: Holder) { h.n = 5; }`
compiles), which is why a blanket admission would have been unsound. The
write set is collected program-wide from the three forms a write takes: an
assignment's place root, an explicit `&mut place`, and any argument (receiver
included) bound to a `&mut` parameter; an unresolvable callee is treated
conservatively. A `mut` local that is never written would qualify by the same
reasoning and is deliberately left out — it moves goldens for a gain nothing
has asked for.

The elision is invisible to output, so its proof is bytes:
`closure-param-inference.js` loses the `__clone` §5 recorded and is
byte-identical to its pre-B60 form, runtime output unchanged. The soundness
boundary has two pins, one per write source, each red when its source is
dropped.

### 8.2 The three conversions

`is_some_and`, `ok_or`, `unzip` take `own self`. Each is pinned at a resource
instantiation (accepted, correct value, and — where a value survives the call
— destroyed exactly once) and at data (behaviour identical). No golden moved
for these: with §8.1 in place, `own self` costs nothing on a data path.

Two of the three resource pins record an ABSENT drop, and it is B62's, not
this arc's: `is_some_and`'s payload goes to the predicate through a match-arm
capture, which the drop planner never seeds as owned. Every combinator B60
already converted (`is_none_or`, `map_or`, `filter`, …) has the same hole. The
conversion is what turns "compile error" into "compiles, runs, and leaks the
payload"; B62 is what turns the second into "and destroys it".

### 8.3 The six, one ruling each

Rewritten over `is`, which LOANS — so `self` is read once and the receiver
survives the test. That alone settles three of them. The other three turn out
to reject for a reason §6 did not name, and it is not R6:

> **A generic body cannot destroy a `T`** (destruction.md §6). Any combinator
> with a path that *discards* a resource value it was handed is therefore
> impossible at a resource instantiation, whatever its receiver convention.

| Combinator | Ruling at a resource | Why |
|---|---|---|
| `inspect(own self, fn)` | **WORKS** | `if self is Some(let x) { fn(x) }` loans the payload to `fn` and hands `self` straight back. The old `match` consumed the subject (R6) and then read it again — the actual two-read case. One value, one drop. |
| `eq(self, b)` | **WORKS — and always did** | §6 was wrong: the old `match (self, b)` moved nothing out of a loan, so the rule it was said to break never applied. It compiled and ran at v0.25.0 too; nobody had pinned it. The rewrite is a shape win — the per-comparison tuple is gone (`equality.js`, `generic-equality.js`), and both sides stay loans, read once per path. |
| `or_else(own self, fn)` | **WORKS** | The fallback is PRODUCED on the `None` path, never handed in and discarded. A `self` that reaches that path is `None` — no payload to destroy. |
| `or(own self, b)` | **REJECTS, correctly** | `Some(a).or(b)` must destroy `b`. `b` stays a LOAN so the rejection is forced; see §8.4 for why `own b` would be worse than useless. |
| `xor(own self, own b)` | **REJECTS, correctly** | `Some(a).xor(Some(b))` is `None` — it discards BOTH. Here `own b` is right: R7 catches the discard directly ("moved on one path but not all"), which is the honest sentence. |
| `unwrap_or(own self, fallback)` | **REJECTS, correctly** | `Some(v).unwrap_or(f)` must destroy `f`. §6 filed this under "reads `self` twice"; it does not — it reads `self` once and the problem was always the fallback. `own self` removes the receiver from the report. `unwrap_or_else` is the clean spelling and is pinned as the steer. |

What changed for the three that still reject is the **diagnostic**: each is
now a single error naming the value that genuinely cannot be handled, where
before there were two, led by a distraction about `self` whose suggested fix
(`own self`) does not fix anything. The pins assert the count, which is the
half that makes them pins rather than restatements.

### 8.4 New find — "moved out on every path" is not what the checker checks

`check_own_generic_exactly_once` implements *moved on EVERY path* as *still
owned after `plan_scope`*, and `plan_branches` merges by INTERSECTION: a
binding survives owned only if owned in every arm. That is the correct merge
for planning drops and the wrong one for finding leaks. Two `own` parameters
moved on DIFFERENT branches therefore both look moved:

```vilan,fragment
fun pick<T>(flag: bool, own first: Option<T>, own second: Option<T>): Option<T> {
    if flag { first } else { second }
}
```

`pick(true, Some(a), Some(b))` compiles, returns `a`, and destroys nothing —
`b` is a resource that is never torn down. R7 does not cover it either:
branch TAILS are R4 move-outs, not a rejoin. **Verified pre-existing on
v0.25.0**, unrelated to this arc except that it is exactly the shape `or`
would have taken. It is why `or` keeps its alternative a loan: declaring
`own b` makes `or` COMPILE at a resource and leak silently, which is the
class of bug B60 existed to remove.

Filed with an `#[ignore]`d pin
(`two_own_generics_moved_on_different_branches_is_not_every_path`). The fix is
a union merge for the leak question — but it needs `is`-refinement or it will
reject correct code: `or_else` leaves `self` un-moved on the `else` path and
that is sound, because a `self` reaching it is `None` and has no payload.

### 8.5 §6.3's table, reconciled

The table called bare `x: T` "by value (a copy, rule 1)"; R3 calls it a loan.
Both stay, as one rule read at two types: the table gains a data column and a
resource column, with the prose that says why they are the same rule — for
data a loan is indistinguishable from a copy (the callee's copy is private),
so the data column states what the implementation performs and the resource
column states the ownership the convention carries. R3 is named as normative
and gains bare `x` in its list; the table's note spells out the consequence
the "a copy" reading hid — a body may not move a bare resource parameter out.
`mut own x` being a parse error is stated where `mut` is introduced, since
§8.1 turns on it.

## 9. B65 / B66 / B67 — the accounting holes closed, 2026-08-04

> **Status: all three SHIPPED 2026-08-04** (§9.4 is the ship record). Three
> residuals filed by the B62 and B63 arcs, each a *different* rule from the one
> that filed it, each pinned `#[ignore]`d and verified pre-existing. They are
> taken as one arc because they are one story: B60's "a body may only consume
> what it owns" and R11's "a generic body cannot destroy a `T`" had each been
> implemented at one position and stated at all of them.

### 9.1 B65 — the capture position of "consume only what you own"

B60 shipped the rule as a predicate over PARAMETERS
(`binding_is_loaned_parameter`). §7.2 then established the ownership split for
captures — a capture of a CONSUMED subject owns, a capture of a LOANED subject
owns nothing — and enforced only the first half. The second half was a premise
with no prohibition attached, so `if o is Some(let r) { sink(r) }` and `match
&o { Some(let r) => sink(r) }` both compiled and destroyed one payload twice.

The fix is the twin predicate, not a special case. `collect_loaned_pattern_captures`
maps every capture bound by a loaned subject to that subject, covering all
three loan forms in §7.2's table verbatim:

| form | why it loans |
|---|---|
| `x is Some(let r)` | a *test*, never a consuming match — always a loan, whatever the subject's own form |
| `match &x { Some(let r) => … }` | R6's inspect-without-consuming |
| `let (r, n) = &pair` | the destructure twin of the same |

It rides `MoveScan`, so the concrete scan and R11's per-instantiation scan
share one set and B65 reached generic bodies with no new plumbing — the same
property that made B60(a) cheap.

**The diagnostic is its own, and the steer is where the two rules genuinely
differ.** `LoanConsumed` says "declare it `own x`". A capture carries no
convention, so that advice names a fix that does not exist; the fix is to
consume the SUBJECT. The steer therefore names the subject (`match o` by
value, with `pattern_subject_name` looking through the `&` so it prints the
place the user would actually edit) plus `Option` + `take`.

**It deliberately offers no copy.** The backlog's draft said "consume the
subject, or clone". There is no user-facing copy spelling in vilan to name:
no `Clone`/`Copy` trait, no `derive`, and the one `.clone()` in std is
`Shared::clone`, a refcount handle — the opposite of a payload copy. Copying
is implicit for data (§6.1: binding a value copies it) and *forbidden* for a
resource (R1: "no copies ever fire for a resource"). So "copy the payload"
would be a speculative steer naming an impossible fix, which
`diagnostics-standard.md` B4 rules out — "no steer is better than a
speculative one".

### 9.2 B66 — the destruction question, asked of every value

`check_own_generic_exactly_once` implemented "a generic body cannot destroy a
`T`" (destruction.md §6) as one question about `own` PARAMETERS. Everything
else a body can end up holding went unasked, so `fun peek<T>(own o: Option<T>)`
whose match consumes `o` passed — the parameter genuinely IS moved out, into
the capture — and the capture leaked at `T := Res`.

The widening is to the honest reading. `plan_scope`'s `dropped` set is exactly
the scope-end teardowns the body would have to run, and a generic body can run
none of them, so **no delta-resource binding may reach a scope-end drop**.
That subsumes the filed case (a capture holding a consumed subject's payload)
and the twin a capture-only fix would have left open (a `let` local of
delta-resource type, pinned separately).

Two gates, each a standing rule rather than a patch:

- **B5, one diagnostic per root cause.** The drop plan assumes an
  affine-valid body. When the move scan already reported, the leftover
  ownership is a *consequence* — `fun use_twice<T>(own x: T): T { let keep =
  x; x }` leaves `keep` owning only because `x` was used twice, which is
  already the error. The widening is gated on the body being move-clean.
- **C1, determinism.** `dropped` is a `HashSet`, so reports are span-sorted.

#### The compat finding: `map` and `is_some_and` were leaking

Two pins asserted the bug. Both are corrected, and the correction is the most
consequential thing in this arc.

§8.2 converted `is_some_and`, `ok_or`, `unzip` to `own self` and pinned all
three ACCEPTED at a resource. For `is_some_and` it recorded an absent `drop a`
and attributed it to B62 — "B62 owns the missing line". **The attribution was
wrong.** B62 destroys a *concrete* capture; that body is generic, where
nothing can. The absent drop was never a missing enrollment: it was §8.3's own
rule going unenforced.

The shape is general, and it is the one §8.3 already stated:

> A generic body cannot destroy a `T`. Any combinator with a path that
> *discards* a resource value it was handed is therefore impossible at a
> resource instantiation, whatever its receiver convention.

A **closure-valued callee loans every argument** — `callee_conventions`
answers `None` for one, so `plan_expr` treats each argument as a loan. So
`fn(x)` does not move the payload into the transform; it borrows it, returns a
`U`, and `x` dies with the arm. Verified by running the pre-B66 tree:
`Some(Db{handle=1}).map(|d| d.handle)` compiled and printed `1 / end` with **no
drop**. `r11_std_option_map_at_a_resource_accept`'s stated premise ("moves the
payload into the transform once") was simply false.

So `map`, `and_then`, `filter` (built on `and_then`), `is_some_and`,
`is_none_or`, `map_or`, `map_or_else`, `map_or_default` all leak at a resource
instantiation and now reject. The combinators that survive are exactly those
that move the payload somewhere real — `unwrap`, `ok_or`, `ok_or_else`,
`unzip`, `transpose` (into the returned value), `inspect` and `or_else` (which
loan via `is` and hand the receiver straight back), `unwrap_or_else` and
`unwrap_or_default` (payload is the tail). Data instantiations are never
enqueued, so every non-resource caller — which is all of them, in std, the
corpus and the examples — is untouched, and that is pinned beside each
correction.

**§8.3's rulings are not disturbed**: `or`, `xor`, `unwrap_or` still reject,
for the reasons §8.3 gives, and their pins are the tripwire.

### 9.3 B67 — restore R7's reach; the merge was never the problem

#### The shape

```vilan,fragment
fun pick<T>(flag: bool, own first: Option<T>, own second: Option<T>): Option<T> {
    if flag { first } else { second }
}
```

`pick(true, Some(a), Some(b))` compiled, returned `a`, and destroyed
**nothing**.

#### Where the defect actually is

§8.4 diagnosed it as the merge: `plan_branches` keeps a binding owned only if
every arm still owns it (INTERSECTION), so two parameters moved on different
arms both look moved; and it proposed "a union merge for the leak question".

That reading is half right and its fix is wrong, which matters because the
proposed fix is a second walk through the whole planner.

**Intersection is not merely "correct for planning drops" — it is correct
full stop, GIVEN R7.** R7 says a binding is moved on every path or none, which
makes ownership single-valued at every program point; when that holds, the
intersection and the union of the arm states are the *same set*, and the merge
is exact for both questions. The merge only diverges from the truth on
programs R7 should have rejected and did not. So the defect is not the merge:
it is that **R7's reach was cut short**, and repairing R7 makes the existing
merge correct again — no second merge mode, no second walk, no new rule.

The cut is one line in `scan_move_branches`:

```rust
let tail_place = if consuming && terminal {
    self.direct_place_binding(*tail, scan)
} else {
    None
};
…
if let Some(binding) = tail_place { r7_state.remove(&binding); }
```

Each arm's tail place is stripped from the state R7 compares across arms. It
was written for R4 — a branch tail is the branch's produced value, not a
rejoin — and it is over-broad: it permits "each arm produces its own value",
which is right, and equally permits "each arm produces a DIFFERENT value and
abandons the other", which is the bug.

#### What the exemption actually protects, measured

Removed outright, the inference binary reports **1440 passed, exactly two
failed**:

- `b63_or_else_at_a_resource_instantiation`
- `b63_or_at_a_resource_rejects_the_discarded_alternative`

Nothing else in 1445 pins depends on it. In particular
`r4_return_through_if_tails_moves_each_branch` and
`a_generic_own_t_moved_out_on_every_branch_is_accepted` (`if flag { x } else {
x }`) survive without it — when EVERY arm moves the tail binding, R7's count
already matches and no exemption is needed. Both survivors are the *same*
case, and it is the case §8.4 named as the constraint.

#### `or_else`, worked through — before implementing

```vilan,fragment
fun or_else(own self, fn: || Option<T>): Option<T> {
    if self is Some(_) {
        self
    } else {
        fn()
    }
}
```

| arm | tail | `self` |
|---|---|---|
| then | `self` | moved (R4 move-out) |
| else | `fn()` — a value PRODUCED here | **not moved** |

With the exemption gone and nothing put in its place, that is a
branch-divergent move and `or_else` is rejected. It must not be: **the else arm
is reached only when `self is Some(_)` is false, so `self` is `None`, and
`None` carries no payload.** There is nothing to destroy, so leaving it
un-moved leaks nothing. The exemption must therefore be *replaced by that
reasoning*, not deleted.

#### The refinement rule

At an `if` whose condition is `x is P`, arm *i* is entered only when condition
*i* holds and conditions `0..i` do not; the trailing `else` only when no
condition holds. A binding is **exempt from the every-path move requirement on
an arm when, on that arm's path, every variant it can still hold carries no
resource payload.**

For `or_else`: on the else arm `self` cannot be `Some`, leaving only `None`,
whose payload list is empty — exempt.

The exemption only ever asserts *"this value has nothing to destroy on this
path"*. It is a statement about the value, not about the code, and it is
conservative by construction: unless every reachable variant is provably
payload-free the binding is not exempt, so the failure mode is an
over-report (a false rejection the user can see and work around), never a
missed leak.

#### The B63 rulings under the new rule — they hold verbatim

| combinator | what happens | ruling |
|---|---|---|
| `or_else` | `self` divergent, **exempt** on the else arm (`None`) | WORKS, unchanged |
| `or` | `self` divergent but exempt (same shape); `b` is a LOAN and the else tail consumes it | **REJECTS**, one error, naming `b` — unchanged |
| `xor` | the two-`Some` path discards both; neither is exempt there (both are `Some`) | **REJECTS** — unchanged |
| `unwrap_or` | `match self` consumes on every path; the `_` arm consumes the loaned `fallback` | **REJECTS**, naming the fallback — unchanged |
| `inspect` | the `if` has no `else` and is not in tail position; `self` is moved at the body tail on the one path there is | WORKS, unchanged |

`or`'s pin asserts **exactly one** diagnostic, so it is the sharpest tripwire
in the set: without the refinement the divergent `self` becomes a second error
and the pin reddens. That is the intended guard and it is planted below.

#### The diagnostic ruling: an ERROR, and specifically R7's existing one

The question the record left open was whether a branch-divergent move should
be an error or whether the merge should synthesize a drop on the non-moving
path. **It is an error**, and R7's own sentence is normative:

> **R7: no conditional moves.** A binding must be moved on every path through
> a scope or on none; moving it on one path only is an error. This keeps
> end-of-scope ownership static: there are no runtime drop flags in v1.

A merge that synthesized a drop on the non-moving path is precisely the
per-path teardown v1 ratified out; and in the program that filed the bug the
body is GENERIC, where no drop can be synthesized at all (§9.2's rule). So
B67 reuses `ResourceMoveViolation::ConditionalMove` **verbatim** — no new
variant, no new message, no new steer, no ledger row. This follows R7's
existing precedent for conditional moves of bindings by *being* it: B67 is not
a new rule, it is R7 reaching the cases it always described, and the diagnostic
it produces is the one users already see for `if flag { sink(r) }`.

One consequence worth stating: `pick` reports **two** errors, one per
parameter. That is not a B5 violation — `first` and `second` are two values,
each leaking on a different path, and fixing one does not fix the other.

### 9.4 Ship record — pins, compat, and whether the story is complete

**Status: all three SHIPPED 2026-08-04.** The three filed `#[ignore]`d pins are
un-ignored, each verified red against the pre-fix tree first:

| item | filed pin(s) | new pins | proved by planting |
|---|---|---|---|
| B65 | `b62_an_is_capture_consumed_by_an_own_call_is_rejected`, `b62_a_loaned_match_capture_consumed_by_an_own_call_is_rejected` | 7 | dropping the check reddens 4 of 7 + both filed pins; the 3 guards correctly survive |
| B66 | `b62_a_generic_capture_never_moved_out_is_rejected_at_a_resource_instantiation` | 8 | dropping the widening reddens 6; dropping the B5 gate reddens 5; the 4 negatives survive both |
| B67 | `two_own_generics_moved_on_different_branches_is_not_every_path` | 6 | breaking the refinement reddens `or_else` + `or` + the user-code guard; restoring the exemption reddens all 4 B67 pins + R7's existing family |

**Compat: nothing in the tree changed behaviour.** `cargo test -p vilan-cli
--test corpus` (~100 programs, byte-identical goldens), `--test examples`,
`-p vilan-core --test docs` (every fenced example) all green with no golden
regenerated and no fence touched. The reason is the resource footprint: the
whole tree's resources are `Database`, `OwnedNursery` and four corpus files,
and none of them is ever handed to an `Option` combinator or matched by loan
and then consumed. The newly-flagged code is std's generic combinators *at a
resource instantiation*, which nothing in tree instantiates that way.

**The B63 combinator rulings hold verbatim.** `or`, `xor`, `unwrap_or` still
reject, for the reasons §8.3 gives; `or`'s pin asserts exactly one diagnostic
and is the sharpest tripwire in the set, since without B67's refinement the
divergent `self` becomes a second error.

**Two diagnostics changed, and both were asserting a bug** (§9.2): `map` and
`is_some_and` at a resource instantiation. Corrected, not weakened, with the
data instantiation pinned unchanged beside each.

#### Is the B60-lineage accounting story complete?

**For the two rules the lineage is about, yes — and "complete" now has a
concrete meaning rather than being a judgement call.** Both rules were
implemented at one syntactic position and stated at all of them, which is the
single mistake B65/B66/B67 each are:

- **"A body may only consume what it owns"** (B60) is now enforced at both
  positions a loan can appear in: the PARAMETER (B60) and the CAPTURE (B65).
  Those are the only two — a local owns by construction, and a module-level
  resource has its own §5 corollary.
- **"A generic body cannot destroy a `T`"** (R11) is now asked at every place
  the drop planner can schedule a destruction, which is an exhaustive list of
  three: an `own` parameter still owned at the fall-through end (the original
  check), a scope-end teardown (B66 — captures and locals), and R2's overwrite
  drop (§9.4's own find, closed here rather than filed for exactly the reason
  this arc exists). There is no fourth: `plan_scope` writes `dropped` and
  `overwrites` and nothing else.
- **"Moved on every path or none"** (R7) now reads branch tails, which was the
  one path-shape it did not (B67).

**What remains, named.** One new find, out of this lineage and filed rather
than fixed because it belongs to a different mechanism:

- **`drop(f(x))` on a call RESULT destroys nothing** — **B68, SHIPPED
  2026-08-04, see §9.5.** `drop(identity(Db{tag = "direct"}))` prints no drop,
  while `let bound = identity(..); drop(bound)` prints it. Verified to
  reproduce with no branch, no capture and no generic capture, so it is neither
  B65's, B66's nor B67's — it is the `drop` sink's rewrite not recognising a
  non-place argument. It is a leak of the same severity as the three closed
  here and wants its own item.

Also still open from earlier arcs, unchanged by this one:
`r11_nested_closure_internal_double_move_is_rejected` and
`generic_field_method_dispatch_runs` remain the two `#[ignore]`d pins in
`inference.rs`.

## 9.5 B68 — a VALUE argument to the `drop` sink, closed 2026-08-04

**Status: SHIPPED.** §9.4's filed find is closed. `drop(identity(Db{tag =
"direct"}))` destroys what it is handed, exactly as `let bound = identity(..);
drop(bound)` does.

### The root cause is not "non-place" — it is "untyped, then silent"

§9.4 filed this as "the rewrite not recognising a non-place argument", which is
one case narrower than the defect. `drop(Db{..})` — also a non-place — always
worked. What separates the two is where the type comes from:

| argument form | type read from | worked before |
|---|---|---|
| `drop(local)` / `drop(param)` | the binding's own `type_id` | yes |
| `drop(Db{..})` | `resolved_types` (a struct initializer records its own) | yes |
| `drop(f(x))` | nowhere — a call's result type is computed lazily and never stored | **no** |

So the rewrite asked "what type is this argument?" of two tables that happen to
hold entries for some expression forms, and a call result is not one of them.
That is half the defect. The other half is what it did with the answer:

```rust
if let Some(type_id) = self.drop_argument_type_id(argument_id) …
    && let Some(helper) = self.ensure_drop_helper(type_id) { … }
return Some(arg_node);   // both "data" and "no idea" land here
```

The fall-through conflates two different situations — *the type resolved and has
no destructor* (data: the correct no-op consume) and *the type did not resolve*
(unknown) — into one emission. The second is a leak from a clean compile, and it
is silent by construction: the output is a bare, correctly-evaluated argument, so
nothing downstream can tell it went wrong. `drop(f(x))` was that path for as long
as it existed.

### The fix, at both halves

1. **Type every sink argument.** A new analyzer pass,
   `record_drop_sink_argument_types`, runs after constraint solving and before
   R11's per-instantiation checks: for each `drop(x)` whose argument carries no
   type on its own expr id, it infers one and records it in
   `drop_sink_value_types`. The map rides the `Program`, so the analyzer's glue
   seeding, the §8 coloring edges, R11's forwarding check and the transformer's
   rewrite all read *one* answer rather than four hand-mirrored lookups. (Those
   four sites also now share one `drop_sink_argument_of` recognizer; the sink was
   being re-identified by hand in each.)
2. **Never-silent (B55's pattern).** An argument whose type still does not
   resolve is collected in `unresolved_drop_sinks` and turned into a hard compile
   error at assembly, spanned at the `drop` call. The *class* cannot recur
   quietly: any future argument form the type query misses reports instead of
   leaking.

Recording only where the existing lookup was silent is what keeps every
already-working form byte-identical.

### No temp binding, and no elision question

The rewrite emits `$h(<argument>)` — the argument is an ordinary JS call
argument, so the value passes into the destructor helper by parameter and no
temporary binding is needed. The elision family (`clone_sites` / `is_elidable_copy`)
is not consulted and must not be: a copy decision only applies to a *place*, and
a call result is not one. The value moves into the drop by construction, which is
what a dead temp should do.

### Where the code lives

- `analyzer.rs`: `drop_sink_argument_of`, `record_drop_sink_argument_types`,
  `drop_sink_value_types` (field + `Program`), the `drop_sink_argument_type_id`
  fallback, and the seeding/coloring/R11 sites rewritten onto the shared
  recognizer.
- `transformer.rs`: `drop_argument_type_id`'s third fallback, the sink rewrite's
  three-way match, `unresolved_drop_sinks` and its assembly-time guard.

### Pins

Seven, in `inference.rs` beside the existing sink family; **four verified red
against the pre-fix tree** (the other three are the controls that were already
green and must stay so):

| pin | red first | what it holds |
|---|---|---|
| `b68_drop_of_a_call_result_destroys_it` | **red** | §9.4's repro, beside the `let`-then-`drop` it must match |
| `b68_drop_of_a_method_call_result_destroys_it` | **red** | the receiver-substituted return type |
| `b68_drop_of_a_nested_call_result_destroys_it` | **red** | the OUTER call's type, not one matched call shape |
| `b68_a_generic_forwarding_a_call_result_to_the_sink_is_rejected_at_a_resource` | **red** | the R11 interplay (below) |
| `b68_drop_of_a_construction_destroys_it` | green | the non-place form that already worked |
| `b68_drop_of_a_data_call_result_is_a_no_op` | green | data is still the no-op consume, effects still evaluated |
| `b68_a_generic_forwarding_a_call_result_to_the_sink_is_accepted_at_data` | green | the data control for the R11 pin |

Corpus: `resource_take.vl` gains `drop(passthrough(Res{tag = "unbound"}))`
beside its existing `let back = passthrough(..); drop(back)`, so the emission is
pinned at the byte (`$h(passthrough([ "unbound" ]))` — the same helper the bound
form uses) and rides the interpreter-equivalence gate. That is the **only**
golden byte that moved: the whole corpus is otherwise byte-identical, which is
the proof that no place-argument form changed.

The never-silent guard was proved non-vacuous by planting: with
`record_drop_sink_argument_types` disabled, `drop(identity(Db{..}))` reports
"the type of this `drop` argument could not be resolved …" at the call instead
of compiling to a silent leak.

### The R11 interplay, and a §9.4 correction

`drop` is **not** an unconditional escape hatch for a generic body. §9.4's
lineage summary is right that the sink is exempt from R11's exactly-once rule
(it *is* the drop site), but `check_generic_drop_forwarding` separately refuses
`fun consume<T>(own x: T) { drop(x) }` **at a resource instantiation**: the
erased body has no concrete destructor, so the rewrite would lower to the data
no-op and leak. That ruling (destruction.md §6, 2026-07-19) is unchanged. What
B68 changes is that routing the same `T` through a call — `drop(identity(x))` —
no longer evades it: the argument now carries the abstract type it always had,
so the delta rule sees it and the call-result form joins the place form under
one diagnostic. At a *data* instantiation both stay accepted, which is the
no-op `drop` is for.

So the honest statement is: `drop` is how a generic body consumes a `T` **at a
data instantiation**, and how a *concrete* body destroys a resource. A generic
body still cannot destroy a resource `T` by any spelling.
