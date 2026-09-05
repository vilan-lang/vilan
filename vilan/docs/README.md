# The Vilan documentation

How to use the Vilan language, its standard library, and the frameworks
built on top of it. If you're wondering where something lives: this book
is about *using* Vilan. Design history and rationale live in the
[`vilan-lang/proposals`](https://github.com/vilan-lang/proposals) repository.

**Brand new?** [Hello Vilan](tour/hello-vilan.md) installs the toolchain and
gets a program running in a couple of minutes. Everything in this book
assumes only that the `vilan` command is on your PATH.

## Parts

- **[Tour](tour/)**: the language itself, taught informally. Start with
  [Coming from JavaScript](tour/coming-from-javascript.md) if that's your
  background, then read in order. Come back any time you need a syntax
  reminder. Nothing in the tour assumes you know the other parts.
- **[Guides](guide/)**: the frameworks, task by task. Reactive state,
  building UI, styling, routing, talking to a server. Each guide reads
  front to back and links into the reference for exact signatures.
- **[std reference](std/)**: the standard library, signatures first.
  Go here to answer "what were the parameters again?".
- **[Specification](spec/)**: the formal definition. Grammar, type
  rules, the memory model, execution. This is the advanced tier. The tour
  teaches; the spec defines; where they disagree, the spec wins.
- **[Appendix](appendix/)**: the [CLI reference](appendix/cli.md), [the editor](appendix/editor.md) (what the language server gives you), the [error index](appendix/errors.md) ("you saw this message, go here"), the [gotchas checklist](appendix/gotchas.md), and the [glossary](appendix/glossary.md).

## Where this book lives

The published copy is at <https://vilan-lang.org/docs/>, with search and
a sidebar, always built from the latest `main`. That is the copy to link
to and the one most readers want.

The source is markdown in the
[repository](https://github.com/vilan-lang/vilan) under `vilan/docs/`, so
everything also reads fine as plain files. If you have the repository
checked out, `cargo install mdbook --version 0.5.4 --locked` once and then
`mdbook serve vilan/docs` gives you the same site locally with live reload.

The version is a **pin, not a suggestion**: mdBook's heading-id algorithm
decides every anchor on the published site, and three things in this tree are
held to v0.5.4's answer — `std::markdown`'s parser (the book-wide anchor
golden, `crates/vilan-core/tests/markdown_anchors.golden`), the language
server's keyword-hover deep links (`book_sync.rs`), and the site build itself
(the pages repo fetches the v0.5.4 release by sha256). A newer renderer can
move an anchor, and a moved anchor is a broken link.

## Conventions

- Examples are complete programs unless explicitly labelled a fragment:
  copy, `vilan build`, run.
- **Every example compiles as part of the test suite** (`cargo test --test
  docs`): a fenced ` ```vilan ` block must compile for the Node target,
  ` ```vilan,browser ` for the browser target, ` ```vilan,norun ` compiles
  but needs external services to actually run, and ` ```vilan,fragment ` is
  prose-only (used sparingly, always labelled).
- Maintenance rule: a change to std, a framework, or the language updates the
  affected docs page **in the same commit**.

## Contents

### Tour
| Chapter | Covers |
|---|---|
| [Coming from JavaScript](tour/coming-from-javascript.md) | the three big shifts, a JS→Vilan phrasebook |
| [Hello Vilan](tour/hello-vilan.md) | installing, the CLI, `vilan.toml`, packages & workspaces, imports |
| [Projects & dependencies](tour/projects.md) | manifests, workspaces, declaring and resolving dependencies |
| [Values & types](tour/values-and-types.md) | bindings, primitives & numeric widths, strings & interpolation, tuples, collections |
| [Functions & closures](tour/functions-and-closures.md) | `fun`, closure types, named-fn coercion, async closures and their seams, context clauses |
| [Data & traits](tour/data-and-traits.md) | structs, enums, `impl`, generics & bounds, traits, derives |
| [Control flow](tour/control-flow.md) | `match`/`is`, loops, `ret`, Option/Result idioms, `!`, `?.` and `?` |
| [The memory model](tour/memory-model.md) | value semantics, views, `mut`/`own`, `Shared`, `Arena`/`Handle` |
| [Async](tour/async.md) | implicit await, `async expr` spawn, promises, timers |
| [Resources](tour/resources.md) | `resource`, moves & loans, `Drop`, `drop(x)`, `Option.take`, `Database`, `OwnedNursery` |
| [Macros & const](tour/macros-and-const.md) | `const` evaluation, derive macros, `macro { … }` blocks |
| [Platforms](tour/platforms.md) | std layers, full-stack packages, externs, assets |

### Guides
| Chapter | Covers |
|---|---|
| [Reactive state](guide/reactive.md) | signals, derived state, effects, ownership & disposal, turns, `optimistic`/`Optimistic`, `Draft` |
| [Building UI](guide/ui.md) | `view` chaining, binds, events, lists, conditionals, mounting |
| [Styling](guide/styling.md) | `const` typed styles, lengths/colors, dynamic values |
| [Routing](guide/routing.md) | enum routes, `parse`/`href`, `link`, `swap`, navigation |
| [Services & RPC](guide/services.md) | `[service]`/`[rpc]`/`[expose]`, Wire, mirrors, reconnection, the server side |
| [Persistence & the server](guide/persistence.md) | `std::db` (SQLite), the http server, files, the process |
| [Server-side rendering](guide/ssr.md) | render-and-replace, one component on both legs, the HTML shell |
| [A full-stack walkthrough](guide/walkthrough.md) | the Notes app end to end: every layer meeting, quoted from a real, tested example |
| [The dev loop](guide/dev-loop.md) | `run --watch`, hot module replacement, what carries across a swap |

### std reference
| Page | Modules |
|---|---|
| [collections](std/collections.md) | List, Map, Set, Range, Iterator |
| [option & result](std/option-result.md) | Option, Result and their method sets |
| [strings](std/strings.md) | str, Display, Debug, Into |
| [numbers](std/numbers.md) | the sized numerics, math, random |
| [traits](std/traits.md) | compare, default, the operator traits, Try/Lift |
| [cells](std/cells.md) | Shared, Arena/Handle |
| [time](std/time.md) | Instant, Duration, timers |
| [encoding](std/encoding.md) | json, wire, binary, bytes, base64 |
| [net](std/net.md) | fetch, ws |
| [reactive](std/reactive.md) | the full `std::reactive` API |
| [style](std/style.md) | the full `std::style` API |
| [rpc](std/rpc.md) | `std::rpc`: transports, clients, frames |
| [browser](std/browser.md) | `std::dom`, `std::ui`, `std::router`, `std::storage` |
| [dev / HMR](std/dev.md) | `std::dev`: `stash`/`take`, `on_teardown`, `hmr_active` |
| [process](std/process.md) | db, http, fs, build, document, process, rpc_server, watch |
| [misc](std/misc.md) | io, task, promise, context, crypto, jwt, asset |

### Specification
| Chapter | Defines |
|---|---|
| [§1 Introduction](spec/introduction.md) | conformance, notation, processing phases |
| [§2 Lexical structure](spec/lexical.md) | tokens, keywords, literals, operators |
| [§3 Grammar](spec/grammar.md) | the full EBNF, precedence, patterns, types |
| [§4 Names & modules](spec/names.md) | scopes, resolution, imports, namespaces |
| [§5 The type system](spec/types.md) | types, generics & inference, traits, coercions, `!`/`?.` |
| [§6 The memory model](spec/memory.md) | the four rules, views, projections, the await rule |
| [§7 Execution & async](spec/execution.md) | entrypoint, evaluation order, the async model |
| [§8 Contexts](spec/contexts.md) | ambient values: `run`/`get`, coverage, injected closures |
| [§9 Const evaluation](spec/const.md) | compile-time values, the const environment, assets |
| [§10 Macros](spec/macros.md) | attribute/derive/block macros, macro_std, splicing |
| [§11 Platform model & manifests](spec/platform.md) | layers, coloring, fences, vilan.toml |
| [§A Appendix](spec/appendix.md) | precedence & keyword tables, lang items |

