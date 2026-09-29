# Spec §A — Appendix

## A.1 Operator precedence

Tightest to loosest; binary levels are left-associative (§3.7):

| Level | Operators |
|---|---|
| 1 | `::` paths · calls · `.member` · `[index]` · `!` (try) · `?.` |
| 2 | prefix `!` `-` `await` `async` `&` `&mut` `*` |
| 3 | `*` `/` `%` |
| 4 | `+` `-` |
| 5 | `<<` `>>` (span-adjacent) |
| 6 | `&` |
| 7 | `^` |
| 8 | `\|` |
| 9 | `==` `!=` `<` `<=` `>` `>=` |
| 10 | `is` |
| 11 | `&&` |
| 12 | `\|\|` |

Above level 12 sit the expression forms (closures, blocks, `if`,
`match`, `for`, `let`, `ret`, assignment), and above those the
top-tier-only forms: `const expr` and struct initializers (§3.8).

## A.2 Reserved words

```text
async     await     const    css     else    enum    export
external  for       fun      if      impl    import  in
is        let       macro    match   mod     mut     null
ret       struct    trait    type    use     true    false
```

Contextual keywords (an identifier everywhere but their one position;
B414 made `with`, `borrows`, `own`, `dyn`, `lazy` and `jump` contextual, and
B459 added `then`):

```text
as        borrows   context   dyn       jump      lazy      only
own       self      Self      sync      then      void      with
```

Also matched by text in one position: `break`/`continue` (after `jump`), and the
attribute names `derive` `service` `client_service` `extern` `must_use` `rpc` `trait_only`
`doc` `expose` `platform` `deprecated` `internal` `resource` (`resource`
was a keyword until B413 made it the `[resource]` attribute).

## A.3 Literal suffixes

`i8 i16 i32 i53 u8 u16 u32 u53 usize f f32 f64 n`: §2.3 (unknown suffixes
error; `i64`/`u64` were renamed to `i53`/`u53`). Unsuffixed: integer
→ `i32`, fractional → `f64`.

## A.4 Lang items

Std declarations the language itself depends on:

The **prelude** column says whether the default (base) prelude makes the
name ambient too — `Option` is both a lang item and a prelude member, and
that is where the two ideas meet (§4.7). `built-in` means the language
has it with no prelude at all.

| Item | Module | Language use | Prelude |
|---|---|---|---|
| primitives (`bool`, `str`, numerics, `BigInt`) | `std::boolean`/`string`/`number` | literal types | built-in |
| `List<T>` | `std::list` | list literals, `for` | built-in |
| `Option<T>` | `std::option` | `?.` results, view-returning lookups | base (with `Some`/`None`) |
| `Try`, `Verdict`, `Lift` | `std::operators` | `!`, `?.`, `?` (§5.10) | — (but `Result`/`Ok`/`Err` are base) |
| `Add Sub Mul Div Rem Shl Shr BitAnd BitOr BitXor` | `std::operators` | operators (§5.7) | — |
| `PartialEq`, `PartialOrd` | `std::compare` | `==`/ordering (§5.7) | — |
| `Iterator`/`Iterable` | `std::iterator` | `for … in` | — |
| `Task<T>` | `std::task` | `async`/`await` (§7.3) | — |
| `Promise<T>` | `std::promise` | host-interop promises (§7.3) | — |
| `Context<T>` | `std::context` | contexts (§8) | — |
| `panic`, `assert` | `std::io` | divergence, `vilan test` | — |

