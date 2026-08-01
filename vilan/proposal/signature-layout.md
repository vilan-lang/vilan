# Signature layout — the width rule reaches a declaration

**Status:** implemented 2026-08-01 (backlog 46). Semantics settled below.

## Why

After backlog 42/44/45 the formatter's width rule covers every composite an
*expression* can be built from — postfix chains, list literals, struct literals,
import brace sets. A declaration's parameter list was the largest addressable
thing left over: 16 of the 111 remaining over-budget lines across `std`,
`examples` and the corpus are `fun` signatures, the widest at 172 columns
(`serve_connected`). They are not incidental — a signature carrying closure
types (`on_connect: |i32, DuplexEnd| void`) is wide by construction, and the
author has no way to break it, because the formatter would put it back.

## The rule

A `fun` signature whose own line is over `LINE_BUDGET` renders its parameter
list one parameter per line, one indentation level in, with a trailing comma
after every parameter — the last one included — and `)` back at the
declaration's own indent, where the return type, `borrows` clause and the body's
`{` glue after it exactly as they do inline:

    fun serve_connected(
        port: i32,
        protocol: RpcProtocol,
        on_connect: |i32, DuplexEnd| void,
        on_ready: |Server| void,
    ) {

This is the rule already shipped for list literals (42's `print_split_list`),
struct literals (42) and import sets (45), applied to the one bracketed list
that is not an expression. Nothing about it is new except where it applies.

Settled points, each following from that:

- **The measured line is the signature line**, which is what the statement-level
  measurement already produces for a `fun` item: the first line of the
  function's rendering, `fun name(…): Type {` — the body is not involved.
- **Generic parameters stay on the opening line** (`fun swap<T: PartialEq>(`),
  as a struct literal's generic arguments stay with its name.
- **Everything after `)` stays on the closing line** — `): View {`,
  `): Result<T, E> borrows self {`, or `;` for a bodyless declaration. Those are
  not list entries and have no layout of their own.
- **A parameter list that fits stays inline WITHOUT a trailing comma**, so the
  comma marks a split list here too and nothing else.
- **An empty list never breaks.** `(⏎)` buys a line and no clarity.
- **No recursion into a parameter.** A parameter is `name: Type`, and a type has
  no layout, so a single parameter too wide for its own line simply stays wide —
  the same way a string literal wider than the budget does.
- **Closure parameters are untouched.** `|a: i32, b: str|` is an expression's
  own punctuation, printed through `print_closure_parameters`; only
  `print_parameters` — reached solely from `print_func` — splits.
- **Bodyless declarations split the same way**: `external fun` bindings and
  trait method signatures are the same printer path and get the same treatment.

## The asymmetry with call arguments, stated deliberately

`vilan fmt` wraps a parameter list but still never wraps a call's *argument*
list (R5, and backlog 43 for the descent). That is a real asymmetry and it is
intended, not an oversight:

- A call's argument list sits inside an expression, where layout hangs off the
  **last** argument — the builder convention the chain rule is built around
  (`.child(view(…))`). Breaking an earlier argument there needs an argument-list
  layout design that nothing in the code motivates yet.
- A declaration's parameter list is not inside an expression. It is the
  function's contract, it has no builder idiom, and one-per-line is its only
  sensible broken form.

If argument-list layout is ever designed (43's neighbourhood), this rule is what
it should match, not the other way round.

## What this does not do

It does not touch `struct`/`enum` declaration bodies (already one field per line
by construction), generic parameter lists (`<T: A, U: B>` — no observed case is
over budget on its own), or `impl`/`trait` headers.
