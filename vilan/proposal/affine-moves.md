# A consuming call is a move — closing B60

> **Status: SHIPPED 2026-08-04** (backlog B60, found by the B53 follow-up arc
> and recorded in `capture-clones.md` §5). `o.unwrap()` consumed the option's
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
  urgent for being the recommended path. Not fixed here; it needs its own
  item.
