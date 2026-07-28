# Local shadowing — positional visibility for local bindings (B34)

> **Status: IMPLEMENTED 2026-07-28.** The shadowing semantics are the user's
> call (2026-07-28: "support multiple declarations of the same name in the
> same scope where the latter one shadows the former"); the resolution rule,
> edge decisions, and the analyzer hardening below were settled in
> implementation. Open calls in §6.

## 1. Motivation — a crash and a miscompile, one root

Backlog **B34**: `let x = x;` (a local binding whose initializer reads the
name being declared) sends `vilan check` into unbounded recursion and a
**stack overflow** — the process aborts. Reproduced in three shapes, all at
HEAD (v0.16.0):

1. local: `fun main() { let x = x; }`
2. module-level bare self-reference: `let a = a;` (B33's pinned cycle test
   used `let A = A + 1;`, whose initial is `Expr::Binary`; the *bare* name
   makes the initial an `Expr::Local` — a different code path)
3. module-level bare two-cycle: `let a = b; let b = a;`

The recursion site is `view_binding_mutability` (analyzer.rs): it follows
`Expr::Local` chains ("a view copied between locals") with no cycle guard,
and deferred name resolution lets `let x = x;` produce a binding whose
initializer *is itself* — a 1-cycle in the entity graph. `check_view_bindings`
walks every variable, so analysis never survives to diagnose anything. The
module shapes crash *before* B33's `check_cycles` gets to speak.

The same deferred resolution hides a **live miscompile**: every name use
resolves once, in `build()`, against the *final* scope map — so in

```vilan
let d = 1;
print(d);   // binds the SECOND d
let d = 2;
print(d);
```

both `print(d)` calls bind to the second `d`, and the emitted JS throws a TDZ
`ReferenceError` at runtime (verified at HEAD). A compiling program that
crashes at startup. The existing pin `edge_shadowing_rebinds_a_fresh_owner`
asserts this shape *compiles*; nothing asserted it *runs*.

## 2. The rule

**A local value binding is visible from the end of its declaring construct to
the end of its scope. A later declaration of the same name in the same scope
shadows the earlier one from its own visibility point onward.**

Consequences, each pinned:

- `let x = x;` — the initializer is inside the declaring construct, so the
  right-hand `x` never sees the binding being declared. With a prior `x` in
  any enclosing scope it is a legal read of that binding; with none it is a
  clean `cannot find 'x' in this scope`.
- `let d = 1; print(d); let d = 2; print(d);` prints `1` then `2`. Uses
  before the shadow point keep the former binding; uses after get the latter.
- A use before *any* visible declaration of the name resolves to an enclosing
  scope's binding if one exists (Rust's rule), and otherwise fails with the
  `cannot find` error plus a note pointing at the too-late declaration.
- "Declaring construct" per binder kind: a `let` / destructuring `let` — the
  whole statement (so an initializer never sees its own binders); a function
  or closure parameter — the parameter itself; a `for x in` item or a match /
  `if let` capture — the pattern. In every case the binding covers exactly
  the text where it can be evaluated.

**Module-level bindings are exempt.** They stay order-independent: initializers
may reference bindings declared later in the file, dependency-ordered emission
(B33) sorts them, and genuine cycles remain B33's compile error. Positional
visibility applies only to scopes that are not module scopes (function and
closure bodies, blocks, loop bodies, match arms).

## 3. Implementation

- `Scope` gains `local_value_declarations: IndexMap<name, Vec<(visible_from,
  Id)>>`, appended in declaration order — populated only in non-module scopes,
  by every value-binder registration site (`let`, destructure binders via
  `walk_pattern`, parameters, `for … in` items, pattern captures).
  `name_to_id_map` is untouched (last declaration wins there), so every
  existing map consumer — module emission order, LSP completions, the
  memoized parent-scope cache — keeps its behavior.
- Deferred use resolution (`prepped_locals`) resolves through a positional
  variant of the scope walk: in each scope, if the name has positional
  entries, the nearest one at-or-before the use's byte offset wins; entries
  all-later mean *this scope does not bind the name yet* and the walk
  continues outward. Names without positional entries (functions, types,
  imports, `use` aliases, the parent-scope memo) resolve from the map as
  before. Module scopes have no positional entries, so their resolution is
  byte-for-byte the old one.
- A use whose span is missing resolves as before (treated as
  end-of-scope); synthesized/expanded code keeps its current meaning.
- **Hardening, independent of the above:** `view_binding_mutability` becomes
  an iterative chain-follow with a seen-set, so *no* origin of an
  `Expr::Local` cycle — including module-level bare cycles, which positional
  resolution deliberately leaves representable — can overflow the analyzer
  again.

## 4. What this deliberately does not change

- **Emission**: binding ids were already the emission key (`NameGenerator`
  disambiguates same-named ids as `x` / `x2`), so shadowed bindings emit as
  distinct JS names with no redeclaration hazard.
- **References/rename/go-to-definition**: recorded by id; they become
  shadow-correct automatically (each use points at its own binding).
- **`use` aliases and items** (`fun`/`struct`/…) in local scopes stay
  hoisted-in-scope (map-only). A local `let` shadowing a *same-scope* item
  name makes uses before the `let` resolve outward rather than to the item —
  a documented edge (§6), strictly no worse than the old behavior (which
  bound them to the later `let` and miscompiled).

## 5. Behavior of the module-level bare cycles

With the overflow gone, module `let a = a;` / `let a = b; let b = a;` analyze
to completion; the bindings never ground, so the existing "type of variable
… could not be resolved" residual reports, and B33's `check_cycles` (which
bails on a program with prior diagnostics) stays silent. That is safe but
weaker than B33's cycle message — upgrading these to the B33 diagnostic is a
recorded polish (§6), not part of this arc.

## 6. Open calls / residuals

- **B33-grade message for bare module cycles** (§5): teach the residual
  reporter or `check_cycles`' gate that an ungrounded module binding whose
  initializer chain is a pure `Expr::Local` cycle deserves the initialization-
  cycle error instead of the residual. S-sized; take up on demand.
- **LSP completion lists** enumerate `name_to_id_map`, so completions inside
  a scope show the *last* same-named binding regardless of cursor position.
  Cosmetic; positional completions ride E-column work if wanted.
- **Same-scope item + later `let` of one name** (§4): pre-`let` uses resolve
  outward instead of to the same-scope item. Pathological; revisit only on a
  real report.
