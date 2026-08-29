# Spec §9 — Const evaluation

`const expr` evaluates `expr` **during compilation** and replaces it
with its result: the emitted program carries the value as a literal,
never the computation. Const evaluation and macro expansion (§10) run
in the same fueled compile-time interpreter; they are the two phases
that execute Vilan code at build time.

## 9.1 The `const` expression

`const` is a prefix operator over an expression. It captures
**greedily**: everything to the end of the surrounding expression folds
(`const 1 + 2 * 3` folds `7`); parenthesize to narrow the extent:
in `(const square(4)) + square(2)` the second call runs at runtime.

A `const` expression may appear anywhere an expression may, including
module-level initializers, where it is also the way to run *logic* at
load position without runtime cost: a `const` initializer ships as a
plain value, participates in no platform coloring (§11.2), and cannot
violate the initializer rules of §7.1 (nothing of it remains to run at
load time).

## 9.2 The const environment

The evaluated expression may use exactly what the compiler can know:

- literals, and the pure operations of the language (§7.2's evaluation
  order applies unchanged);
- functions whose bodies are themselves const-evaluable, transitively;
- imports, and immutable module bindings whose own initializers are
  const-evaluable.

**Host capabilities do not exist at compile time.** A call that
requires the host (the clock, the filesystem, network, randomness, any
`external` function without a compile-time definition) is a compile
error inside `const` ("`now()` is not const-evaluable"), not a deferred
runtime call: the answer would not be a constant.

The deliberate exception is the **compile-time file channel**,
`std::asset`, callable **only** during const evaluation, in three
directions. `emit(kind, content)` is the output direction for lines: it
declares a build asset (the styling system's CSS, for example) that the
build writes beside the output; its ordered spelling
`emit_keyed(kind, key, content)` carries the contribution's own sort
key, and `emit(kind, content)` is exactly `emit_keyed(kind, content,
content)`. `read(path)` is the input direction: it
returns a project file's text so the result of parsing or transforming
it can fold into the output. `bundle(path)` is the output direction for
whole files: it tells the build to carry a non-code resource into the
output directory unchanged, and evaluates to the url the copy answers
on. Every path here is relative to the **package root** (the base
imports resolve under — never the process working directory), may not
be absolute or escape that root, and the file becomes a **tracked build
input**: a change to it invalidates every build product that named it,
exactly as editing a source file would, and a missing file is a compile
error at the `const` expression. The channel keeps const evaluation
deterministic *per build-input closure*: same sources and same project
files, same output — `emit` same lines out, `read` same values in,
`bundle` same files carried.

A function that reaches `emit`, `emit_keyed`, `read` or `bundle` is
**compile-time-only**,
transitively, and the compiler enforces that statically. A call from
runtime code into compile-time-only territory is an error at the
outermost crossing — the call that leaves ordinary code. A crossing
made **through trait dispatch** counts too: a generic call whose bound
selects an emitting impl is refused at the call that selected it,
while a clean impl of the same trait member through the same generic
stays legal — the check follows the resolved instantiation, per call
site. Where the compiler cannot see which impl a dispatch selects (a
shared default body's `self` call, for instance) it refuses every
receiver rather than let one slip through and throw at run time.
Because a
call made *through a value* has no statically known callee, a
compile-time-only function also has **no runtime value form**: naming
one as a value (passing it to a higher-order function, binding it, or
writing a closure literal that reaches the channel) is an error at
that reference, outside a `const`. Inside a `const` the restriction
lifts entirely — the interpreter makes the call, so
`const apply(styled)` is legal where `apply(styled)` is not.

## 9.3 Failure and resource limits

Const evaluation is total by construction of the budget: each run is
bounded by the interpreter's **fuel** (steps; an `asset::read` charges
one per byte read, so fuel bounds input size too — an `asset::bundle`
charges a flat cost instead, since its bytes never enter the program
and a size charge would cap resources rather than work) and **depth**
(nesting). Exhausting either, or panicking during evaluation, is a
compile error carrying the const expression's span; a budget failure
says so, rather than reading as a program error. A runaway `const`
fails the build; it cannot hang it. The fuel budget is sized so that
real compile-time workloads fit with room to spare — parsing the
largest page of this book at compile time uses about an eighth of it —
while a runaway still fails fast. The budgets are compiler constants —
the manifest's `[macro]` section (§11.4) sizes macro *expansion* only,
and does not size `const`.

The primary span is the `const` expression itself, always: the tree the
interpreter evaluates is compiled output and carries no source
positions, so there is no inner expression to anchor to. The diagnostic
instead names the **function** the failure occurred in and notes that
function's declaration, with the call chain that reached it.

## 9.4 Results

The result of a `const` expression is spliced as a literal of its
type: numbers, strings, booleans, lists, tuples, structs and enums of
const-evaluable contents. The expression's *type* is checked exactly
as if it ran at runtime; `const` never changes typing, only when the
computation happens.

## 9.5 Inferred `const`

A **release** build additionally folds `let` and `mut` initializers
that were never marked `const`, where it can settle them under §9.2's
environment. `let total = 1 + 2 * 3;` ships as `7` with no keyword.

The keyword remains the contract, and the difference is what happens on
failure. An explicit `const` that cannot be evaluated is a compile
error (§9.3). An inferred fold that cannot be evaluated — for **any**
reason: a runtime free variable, a host capability, a panic, a budget,
a result that is not plain data — simply does not happen, with no
diagnostic. The binding stays exactly as written and runs at runtime.
Inference can therefore never change whether a program compiles, only
when some of its arithmetic happens.

Four rules bound it:

- **Observable behaviour is preserved exactly.** An evaluation that
  panics, prints, or exits is discarded and the binding left alone.
  (An explicit `const` *does* discard what its evaluation printed —
  that computation is what the author asked to move to compile time.)
- **Inference never creates a const context.** Reaching a
  compile-time-only function (§9.2) refuses the fold rather than
  performing the emission, so whether a style compiles never depends on
  the optimizer.
- **Inferred evaluation has its own, tighter budgets**, and a fold
  whose literal would be large is declined — explicit `const` is the
  opt-in for expensive or bulky results.
- **Type parameters are out of scope.** A binding inside a generic
  function body is never folded: its value depends on a
  monomorphization the const environment does not carry.

The `debug` preset does not infer, so a debug build keeps every such
computation where a stack trace can show it; `release` does. Folding is
deterministic — the same source folds identically on every build — and
the language server never runs the pass, since it produces nothing to
report.
