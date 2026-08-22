# Return-position type checking (backlog B10)

Status: **implemented with this note** (2026-07-04). Pins: the two `#[ignore]`s named in
B10, un-ignored, plus the per-case suite below. **Rule 3 amended 2026-08-22 (B126)** — see
"Rule 3, amended" below; the original wording is kept there for the record.

## The gap (bigger than B10 recorded)

B10 said the solver never constrains a `ret` against the enclosing signature. Probing
showed the gap is wider: **the tail expression isn't checked either**. `fun bad(): i32 {
"nope" }` compiled clean — `Constraint::ReturnType` runs `infer_type(body, expected =
declared)` which *directs* inference (return-position generic binding) but never
*verifies* the result. Every "Expected X, but got Y" in the suite came from let-annotation
and argument checking; return position had none.

## Semantics (settled by probe, pinned)

1. **`ret` returns from the nearest enclosing callable** — function, closure, or `async`
   block (probed: a `ret` in a closure exits the closure; in an `async {}` it settles the
   block). The check is therefore scoped per-callable.
2. **In a function with a declared return type `R`:** the tail and every `ret v` check
   `typeof(v)` against `R` through the same constraint (`reconcile_type` — the same
   unification the let-annotation check uses, so generic returns bind, not just match).
   A **bare `ret`** checks a synthesized void value against `R` — so it is legal exactly
   when `R` is void, and errors as `Expected i32, but got void instead.` otherwise. No
   special case: bare `ret` is `ret <void>`.
3. **In a function with no declared return type:** the return type is **inferred from
   the body's return positions** — the tail, when the body can reach it, and every
   `ret` — and they must agree. (Amended 2026-08-22, B126; the original wording and why
   it was wrong are under "Rule 3, amended" below.)
   - The tail counts only when it is **reachable**: a body whose last statement leaves
     (`{ ret 1; }` — its synthesized void tail is dead code) or whose tail itself
     diverges (an exhaustive `if`/`else` of `ret`s, B124) contributes no tail value, so
     `fun f(x: bool) { ret 1; }` is `i32` (pin
     `b126_a_ret_only_body_infers_its_return_type`), and so is
     `fun f(x: bool) { if x { ret 1; } else { ret 2; } }` — which used to type `never`
     and let `let y: str = f(false)` through (pin
     `b126_an_exhaustive_if_else_of_rets_infers_from_the_rets`). `fun f() { 5 }` is `i32`
     exactly as before (pin `b126_a_tail_only_body_still_infers_from_its_tail`).
   - A tail the body CAN reach is evidence like any `ret`: `{ if x { ret 1; } 2 }` is
     `i32` (pin `b126_a_ret_and_a_tail_that_agree_infer_one_type`); `{ if x { ret 1; } }`
     falls through without a value, and the `ret 1` disagrees with that (pin
     `b126_a_value_ret_beside_a_fall_through_is_refused`).
   - A **bare `ret`** is `ret <void>` (rule 2's reading, no special case): it agrees with
     a void body and disagrees with a value tail (pin
     `b126_a_bare_ret_in_a_value_tailed_function_is_refused`). A `ret` of a void call
     beside a value tail is the same disagreement (pin
     `b126_a_void_ret_beside_a_value_tail_is_refused`).
   - **Disagreement is one refusal per disagreeing `ret`**, anchored at that `ret`,
     naming the `ret`'s type, the inferred type, and where the inferred type came from —
     the tail, an earlier `ret`, or the body falling through — with a note at that
     origin (pins `b126_a_ret_disagreeing_with_the_tail_is_refused_at_the_ret`,
     `b126_rets_that_disagree_are_refused_at_the_later_ret`). The evidence is read in
     one order: the tail first (when reachable), then the `ret`s in source order, each
     inferred WITH the running type as its expectation so a return-position generic in
     a `ret` binds from the tail (pin `b126_a_ret_of_a_generic_call_binds_from_the_tail`).
     A function with a disagreement has no inferred type — its calls type as `any` — so
     the refusal never cascades into an `Expected i32, but got void` at a call site (B5;
     the disagreement pins assert the cascade's absence).
   - A **self-call** inside the body contributes nothing — its type IS the answer being
     computed — so `fun count(n: i32) { if n == 0 { ret 0; } ret 1 + count(n - 1); }` is
     `i32` from `ret 0`, and a function whose only return evidence is self-calls is
     `never` (pin `b126_a_recursive_unannotated_function_infers_from_its_other_returns`).
   - The rule is the function's, not the shape's: an `async fun` without an annotation
     infers the same way and a call to it yields that type (pin
     `b126_an_async_function_without_annotation_infers_from_its_rets`); a nested closure's
     `ret`s stay on the closure's own frame under rule 4 (pin
     `b126_a_nested_closures_rets_stay_on_the_closures_frame`).
4. **In closures and `async` blocks (shipped as the follow-up):** their return types are
   *inferred*, so a closure's `ret`s collect on its frame and check against the inferred
   tail type once it resolves (`Constraint::ClosureReturns`): a value-`ret` must reconcile
   with the tail (inferred WITH the tail as expectation, so return-position generics
   bind); a bare `ret` requires a void tail; a value-`ret` in a void-tailed closure is
   rejected with guidance ("make the ret'd value the body's tail" — the conservative rule
   that avoids the diverging-tail swamp). A closure that never types (unbound, never
   called) leaves the check deferred, matching how loosely such a closure types
   everywhere else.

## Mechanism

- `resolve_return_type` gains the missing half: after `infer_type` resolves, `reconcile_type`
  against the declared type; `None` → the standard mismatch diagnostic at the value's span.
  This alone fixes the tail.
- The analyzer walks with a `return_type_stack: Vec<ReturnFrame>` — `Function(id, R)`
  pushed around a function body walk when a return type is declared, `Inferred { rets }`
  for unannotated functions, closures, and `async` blocks (the boundary that makes `ret`
  inner-scoped). At a `ret` the only question is declared-or-inferred; what becomes of
  the collected `rets` is the popper's business — a closure pushes
  `Constraint::ClosureReturns` (rule 4), a function stores them on its `Function` record
  and pushes `Constraint::FunctionReturns` (rule 3). `VoidFunction` (B10's "rets
  unchecked" frame) is gone: nothing is unchecked any more.
- `Node::FuncReturn` pushes `Constraint::ReturnType` for its value (or a synthesized
  `Expr::Void` entity spanned at the `ret` itself) against the innermost `Some(R)`, and
  seeds `expected_types` — so `ret` is a first-class return position: return-directed
  generic binding (`ret List::new()`) works exactly as it does for the tail.

## What turning the check on surfaced

Three fixes fell out of enforcement, all root-caused:

- **The nine operator-trait defaults were ill-typed** — `{ panic("not implemented yet"); }`
  with a semicolon makes the panic a *statement* and the block's tail void, defeating the
  existing never-typing (`panic(..)` calls type as `Any` — a mechanism whose own comment
  anticipates exactly this "sole body of a function with any return type" case). Dropping
  the semicolons restores the intended pattern; behavior identical (panic throws).
- **`reconcile_type` had no `(Trait, Trait)` arm** — a trait-typed `self` returned through
  a trait-typed signature (`impl Iterator<type T> with Iterable<T> { fun iter(self):
  Iterator<T> { self } }`) had never reached a *checking* position before. Same-id traits
  now reconcile their arguments pairwise, like the nominal `Struct`/`Enum` arms.
- **`reconcile_type` had no `(Mapped, Mapped)` arm** — a parameter typed `(U in T:
  List<U>)` returned through an identically-written mapped return walks as two distinct
  binder ids, so the arm reconciles *structurally* (sources and templates recurse; the
  binders' alpha-renaming bindings are dropped from the result).

## Rule 3, amended (B126, 2026-08-22)

### What rule 3 said, and what the code did

Rule 3 as ratified 2026-07-04 read:

> In a function with no declared return type (void): nothing is checked — neither the
> tail (existing behavior: `fun f() { 5 }` compiles, the value is discarded) nor any
> `ret v`. Consistency with the tail is the rule; a void function's return values are
> discarded, not diagnosed.

The "void" was never what the code did. An unannotated function's type was **inferred
from its tail** — `infer_type_inner`'s `Type::Function` arm read `f.body.1` when
`return_type_id` was `None`, and so did the closure-coercion readers and the `for`
protocol's `next` reader: `fun f() { 5 }` was `i32`, `let y: i32 = f()` compiled, and
B20's own coercion pin (`a_void_function_without_annotation_coerces`, despite its name:
"the return type comes from the body's inferred type") relied on it. What rule 3
described accurately was the CHECK — the walk pushed `ReturnFrame::VoidFunction`, and a
`ret` against it did nothing. So the paper was right that nothing was checked and wrong
about why: the function was not void, its `ret`s were simply invisible to the one
reader that typed it (the tail id), which is exactly the gap B124's lane found and
filed as B126.

Invisible `ret`s are not merely a missed inference — they are unsound. Probed
2026-08-22 against `next` @ 67cd3c57:

- `fun f(x: bool) { if x { ret "s"; } 2 }` with `let y: i32 = f(true); print(y)` compiled
  clean and printed `s` — a `str` under an `i32` binding.
- `fun f(x: bool) { if x { ret; } 2 }` with `let y: i32 = f(true)` printed `undefined`.
- `fun f(x: bool) { if x { ret 1; } else { ret 2; } }` typed `never` (B124's diverging
  tail) so `let y: str = f(false)` compiled and printed `2`.
- `fun f(x: bool) { ret 1; }` — B126's own example — refused `let y: i32 = f(true)` with
  `Expected i32, but got void instead.` at the CALL, the one place that had nothing to do
  with the mistake.
- An unannotated impl member `fun area(self) { ret "wide"; }` against a trait declaring
  `: i32` passed conformance (the reader saw an unmapped tail and matched leniently) and
  printed `wide`.

"Discarded, not diagnosed" was a description of an accident, and the accident
miscompiled.

### The rule, and the one place the recommended rule was overturned

The orchestrator's brief recommended lifting rule 4 to functions with "the same
conservative mix rule (a value-`ret` with a void tail → the value's type)". That clause
is unsound as stated: in `fun f(x: bool) { if x { ret 1; } }` the void tail is
**reachable** — `f(false)` falls through and hands back `undefined` — so typing the
function `i32` from its `ret` would be the `undefined`-under-`i32` miscompile again,
one layer up. The line that makes `{ ret 1; }` infer `i32` while `{ if x { ret 1; } }`
is refused is not void-vs-value; it is **reachable-vs-unreachable**, and the compiler
already draws it: B124's `expr_diverges`, asked of the tail and of the block's last
statement, is the question `check_return_position` asks before it reports "this body
ends without producing a value" for a declared function. Rule 3 asks the same question
of the same positions and reads the answer the other way — an unreachable tail is not
evidence. Nothing new is invented; the declared and inferred regimes now agree about
which positions exist.

Rule 4's "make the ret'd value the body's tail" guidance for closures is NOT lifted:
a closure of `ret`s is still refused with that steer
(`a_closure_of_rets_loses_the_false_mismatch_and_keeps_rule_4s_guidance`), because
closure return inference is b125's open territory and a closure almost always has an
expected type from its call site, which makes the conservative rule cheap there. The
asymmetry — `{ ret 1; }` infers in a function and is refused in a closure — is
recorded as an owner question in the B126 lane's report rather than settled here.

**Bare `ret` in a value-tailed function is a refusal**, not a void vote that wins or
loses by position. Rule 2 already reads a bare `ret` as `ret <void>` with no special
case, rule 4 already requires a void tail of it, and the probe above shows what the
alternative ships. The refusal anchors at the `ret`, like every other disagreement.

### Mechanism

One helper answers "what does this unannotated function return": `inferred_return_type`
(over `infer_function_returns`, which also lists the disagreements). Its evidence is the
reachable tail plus `Function.rets`, read tail-first then in source order; a `never`,
`any` or `unknown` item constrains nothing (it is kept only as the answer of last resort
when nothing else speaks); a disagreement makes the answer `any`. Every reader that
used to read `f.body.1` goes through it: `infer_type_inner`'s `Type::Function` arm (the
call site), `function_closure_type` (a named function coerced to a closure slot),
`for_each_next_non_option_return` (the `for` protocol's unannotated `next`, B92), and
the trait-conformance return check (`MemberSignatureShape::body_tail_id` is gone — the
check asks the helper for `check.impl_function_id`). `function_closure_type_recorded`,
the read-only coercion path, reads the helper's record (`inferred_return_types`), which
the helper writes whenever it computes an exact answer.

Recursion: the helper keeps a stack of the functions it is inferring. A re-entrant ask
for a function already on the stack answers `never` — the self-call's type is the
answer under construction, so it can constrain nothing — and marks every frame nested
inside that function's as inexact, so a function whose answer was built on an
unfinished neighbour's is not recorded (its own constraint computes it top-level and
records then). `exprs_seen` still guards expression-level cycles exactly as before.

`Constraint::FunctionReturns { function_id }` is pushed for every bodied function
without a declared return type (not only those with `ret`s — the record is how the
read-only coercion path sees `{ ret 1; }` as `i32`). It defers while any evidence is
unresolved, like `ClosureReturns`, and reports each disagreement once, at its `ret`.

The view/resource seam readers over `return_sites` (B116's join: the tail of every
function, plus each `ret` of a DECLARED-return function) are untouched by this
amendment; a `ret` in an unannotated function is already refused as a view escape by
the generic `FunctionReturn` scan (probed: `fun pick(&self) { ret &self.x; }` reports "a
view cannot escape its scope"). Whether `return_sites` should carry unannotated `ret`s
too is an owner question in the lane's report.

## Excluded (recorded, not drifted into)

- Closure-`ret` participation in closure return inference (above; `#[ignore]` pin).
- A never type: `ret`/`panic` as expressions still type void; `match` arms mixing a
  `ret` arm with value arms keep today's behavior (the arm unification is untouched).
