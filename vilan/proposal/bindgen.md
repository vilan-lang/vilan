# bindgen — generating `external` bindings from TypeScript headers (E31)

> **Status: DRAFT 2026-08-03 — awaiting review**
>
> Ground truth for every claim below was read from source, not assumed: std's
> hand-written bindings (`fetch.vl`, `dom.vl`, `bytes.vl`, `time.vl`, `rpc.vl`,
> `process/fs.vl`, `process/http.vl`), the parser/analyzer's actual accepted
> grammar (`crates/vilan-core/src/{parsing,node,platform_color}.rs` — which is
> in places ahead of `docs/spec/grammar.md`, noted where it matters), the spec
> (`docs/spec/{types,execution,platform,grammar}.md`), and the CLI's existing
> subcommand seams (`crates/vilan-cli/src/{main,init}.rs`). Nothing here is
> implemented. Backlog citations are to `proposal/backlog-2026-07-18.md`
> §E.31 (the charter) unless noted.

## 0. The problem and the thesis

std's host bindings are hand-written, and they are not small: `fetch.vl` binds
`fetch`/`Response`/streaming bodies, `dom.vl` binds `document`/`Element`/
`Text`/`Event`, `process/fs.vl` and `process/http.vl` bind `node:fs`/
`node:http`. Every one of these is the same mechanical shape repeated by
hand: an `external struct` per host type, one `[extern(...)]`-attributed
`external fun` per method/property/constructor. A user reaching for a
third-party JS library (`lodash`, `express`, `leaflet`) today hand-writes the
same thing, from scratch, with no tooling — and gets it wrong in the same
ways std's authors would (wrong asyncness, wrong optionality, a missed
overload) with none of the review std's own bindings get.

**Thesis:** the shape is mechanical enough to generate from the artifact
that already describes it precisely for a huge slice of the JS ecosystem —
the library's `.d.ts`. `vilan bindgen` parses a declaration file and emits a
`.vl` module in exactly the hand-written dialect above: `external struct` +
`[extern(...)]` `external fun`, nothing new. It is not a compiler feature;
it is a source-to-source generator whose output is ordinary, reviewable
vilan source that happens to have been written by a tool instead of a
person — checked in, diffed, and owned like any other file (§1).

This proposal settles the CLI shape, the parser choice, the type-mapping
table (the bulk of the design work), platform attribution, the v1 subset,
and the testing shape. It does **not** implement anything; per house rules
(`CLAUDE.md`) a feature this size is proven on paper first.

## 1. CLI shape

```
vilan bindgen <file.d.ts> [-o <out.vl>] --platform <node|deno|bun|browser|@process>
```

- **Not a build-time step.** `vilan bindgen` is invoked by a developer, by
  hand, when they want bindings for a library. It never runs as part of
  `vilan build`/`vilan check`/`vilan run`, has no manifest wiring, and reads
  nothing from `vilan.toml`. The output is a normal `.vl` file the developer
  reviews (`git diff`), edits if needed, and commits — a **visible,
  versioned artifact**, not a generated-at-build cache entry. This is
  stated explicitly in the charter: *"Probably `vilan bindgen <file.d.ts>`
  emitting a `.vl` module to review and check in, NOT a build-time step —
  generated bindings should be visible, versioned source (matches the F5
  project-model philosophy)"* (backlog §E.31). The same instinct shows up
  elsewhere in the project-model line of work: F5's git dependencies fetch
  into a content-addressed cache and the checkout **is** the source of
  truth (`proposal/distribution.md` §5) — nothing in vilan's project model
  hides generated-but-load-bearing content behind a build step the way, say,
  a bundler's code-splitting output is disposable. bindgen output is closer
  to `vilan init`'s scaffold than to a build artifact: a starting point a
  human owns from the moment it lands.
- **Where it lives.** `crates/vilan-cli/src/main.rs` wires every subcommand
  through one `clap::Subcommand` enum, `Command` (`main.rs:38-152`), matched
  in `run_cli` (`main.rs:169-227`). Two existing subcommands set the
  precedent for a generator: `Init` (`Command::Init { name, template } =>
  init::init(name, template)`, `main.rs:225`) lives in its own module,
  `crates/vilan-cli/src/init.rs`, wired via `mod init;` in the file's module
  list (`main.rs:11-15`) alongside `hmr`/`job`/`paint`/`upgrade`. `Fmt`, by
  contrast, is a ~dozen-line private function inline in `main.rs` itself
  (`fn fmt`, `main.rs:1107`) that mostly delegates to a library function,
  `vilan_core::formatter::format`. bindgen's own logic (`.d.ts` parsing,
  type mapping, `.vl` emission) is substantial enough — and independently
  testable enough (§6) — to follow `Init`'s shape, not `Fmt`'s: a new
  `Command::Bindgen { file: PathBuf, output: Option<PathBuf>, platform:
  String }` variant, a new `mod bindgen;`, and `crates/vilan-cli/src/
  bindgen.rs` exposing `pub fn bindgen(file, output, platform) -> ExitCode`.
  Mirroring how `fmt` calls into `vilan_core::formatter`, the actual `.d.ts
  → .vl` machinery (parsing via oxc, the type-mapping table, emission)
  should live in `vilan-core` as its own module (e.g. `vilan_core::bindgen`)
  with `crates/vilan-cli/src/bindgen.rs` staying a thin CLI wrapper: this
  keeps the oxc dependency, if it ever needs to be reused (an LSP quick-fix
  that generates a binding for an unresolved import, say), reachable from
  somewhere other than the CLI binary, and keeps `vilan-cli` doing what it
  already does for every other subcommand — argument handling and exit
  codes only.
- **Emission passes through the formatter.** Whatever `vilan_core::bindgen`
  produces should be piped through `vilan_core::formatter::format` before
  writing, the same pass `vilan fmt` uses — generated code should be
  indistinguishable in style from hand-written std code, not recognizable
  by its formatting. This also means bindgen's own emitter doesn't need to
  sweat pretty-printing; it can emit straightforwardly and let the
  formatter normalize it, which simplifies both the emitter and the golden
  tests (§6).
- **Output path.** `-o out.vl` for an explicit path; omitted, default to
  `<file-stem>.vl` beside the input (`leaflet.d.ts` → `leaflet.vl`), matching
  `vilan build`'s own `<file>.js` default-beside-input convention
  (`main.rs:54-55`). Where a generated file *should* live in a project
  (`src/vendor/leaflet.vl`? a sibling `bindings/` directory?) is open — the
  charter doesn't say, and there's no existing "vendored third-party
  binding" convention in the repo to anchor a default on. Flagged in §8.

## 2. Parser choice

Three candidates, per the charter: **oxc**, **swc**, or shelling out to the
**TypeScript compiler via a node subprocess**.

- **oxc** (`oxc_parser`, MIT). Rust-native, no runtime dependency beyond the
  compiled binary. Verified 2026-08-03: MIT-licensed, actively maintained
  (latest release days old), a genuinely standalone crate (`oxc_parser`
  pulls in `oxc_ast`/`oxc_allocator`/`oxc_span`/etc. — the parser only, not
  the transformer/linter/minifier/formatter that make up the rest of the
  oxc toolchain) with 71 reverse dependents on crates.io including
  production tooling (`tauri-cli`). Its direct dependency list is short
  (~16 crates: the oxc_* family plus small, ubiquitous utilities —
  `bitflags`, `memchr`, `num-bigint`, `num-traits`, `rustc-hash`,
  `cow-utils`, `seq-macro`) and today's `Cargo.lock` (136 packages) already
  carries two of them (`bitflags`, `memchr`, via existing deps) — the net
  new surface is closer to a dozen crates, not a toolchain. Parses `.ts`/
  `.tsx`/`.d.ts` as one grammar (ambient declarations, `declare`, interface/
  type-alias/namespace syntax are ordinary AST node kinds oxc already
  round-trips, per its own isolated-declarations/`.d.ts`-emit feature —
  i.e. the *syntax* surface is exercised in both directions already).
  **This is also the charter's own named candidate.**
- **swc**. Also Rust-native, also capable of parsing `.d.ts`. Measurably
  heavier: a reported ~35 MB larger install footprint than oxc's toolchain
  and a slower parser (~3x, per oxc's own published benchmarks — a biased
  source, weighted accordingly, but the qualitative direction — oxc is the
  newer, leaner, faster project — is corroborated independently). No
  decisive functional advantage found for the `.d.ts`-parsing-only use case
  bindgen needs; swc's strengths (a mature transform/bundler pipeline) are
  not ones bindgen uses.
- **The TypeScript compiler via a node subprocess.** Trades a Rust
  dependency for a *runtime* one, and not a small one: it would make
  `vilan bindgen` the only subcommand in the toolchain that requires a node
  install to function at all. Every other subcommand — `build`, `check`,
  `fmt`, `init`, even `test`'s compile step — runs from the single
  installed `vilan` binary alone; only *running* the compiled JS output
  needs node (`vilan run`/`vilan test`). Distribution (`proposal/
  distribution.md`) ships that single binary across npm, VS Code, and
  Homebrew specifically so the toolchain doesn't have an implicit
  dependency graph beyond itself. A subprocess approach would also need its
  own IPC/marshaling layer (spawn `tsc`, get an AST back as JSON, deserialize
  it) and inherits `tsc`'s own startup cost per invocation. Its one real
  advantage — `tsc` is the reference implementation, so its `.d.ts` fidelity
  is definitionally exact, including full type resolution (which oxc's
  syntax-only parser doesn't attempt) — is not decisive for bindgen's scope
  (§5): v1 works on syntactic shapes (interfaces, functions, classes) that
  don't need a type checker to resolve, only a parser to see.

**Recommendation: oxc.** It matches the charter's own candidate, keeps the
"one binary, no runtime dependency" distribution story intact, and its
dependency cost is modest and mostly already-adjacent to what's in the
lockfile. The gap oxc leaves — no semantic type resolution across files,
so a `.d.ts` that imports a type from another `.d.ts` needs bindgen to do
its own (probably shallow, v1) cross-file resolution — is real but not a
reason to prefer `tsc`'s subprocess cost; it's scoped into §5's cut line
instead.

**The notices gate.** Adding `oxc_parser` to any crate's `Cargo.toml`
changes `Cargo.lock`, which the completeness test in
`crates/vilan-cli/tests/third_party_notices.rs` walks: *"a dependency added
without regenerating the notices would ship uncovered"* — it asserts every
non-workspace package name in `Cargo.lock` appears in
`THIRD-PARTY-NOTICES.txt`. The fix is mechanical (`cargo about generate
about.hbs -o THIRD-PARTY-NOTICES.txt`, per `CLAUDE.md`'s own cheap-gate
list), but `about.toml`'s `accepted` list is a **closed** license set (MIT,
Apache-2.0, ISC, Unicode-3.0, BSD-2-Clause, BSD-3-Clause, Zlib, CC0-1.0) —
`cargo about generate` fails loudly if any transitively-pulled crate's
license doesn't reduce to something in that list, which is the real gate,
not the notices file itself. oxc_parser itself is MIT (verified); its
utility dependencies (`bitflags`, `num-bigint`, `num-traits`, `rustc-hash`,
`memchr`) are all commonly MIT/Apache-2.0-or-similar dual-licensed crates,
but a couple (`memchr` notably) carry an *alternative* license
(`Unlicense`) not itself on the accepted list — whether `cargo-about`
resolves the OR-expression to the accepted branch cleanly or needs an
explicit `about.toml` exception is exactly the kind of thing that must be
**run, not predicted**: whoever takes this item up should run `cargo about
generate` against a real `Cargo.lock` with oxc added and read its own
output before assuming the license surface is clean. Flagged as a
take-up step, not resolved here (no lockfile exists yet to check against).

## 3. The type-mapping table

Every row follows one invariant, stated explicitly because it's the
document's central design commitment: **an unmappable TS construct never
disappears silently.** bindgen emits a clearly-marked comment
(`// TODO(bindgen): ...`) naming what it couldn't map and why, in place of
either guessing wrong or dropping the member. A generated file with TODOs
is reviewable — a human sees exactly what needs attention. A generated
file with silent gaps is a landmine for whoever depends on it later.

### 3.1 Primitives

| TS | vilan | Notes |
|---|---|---|
| `string` | `str` | |
| `boolean` | `bool` | |
| `void` (return position) | `void` | |
| `number` | `f64` | **The single most consequential default in this table.** JS `number` has no int/float distinction, so a `.d.ts` alone cannot tell bindgen whether a given `number` is meant as `i32`, `u32`, `i53`, or a genuine float — that's domain knowledge, and std's own hand-written bindings use it: `Date.now(): f64` (unbounded epoch millis) but `Date`'s constructor takes `millis: i53` (`time.vl:20-21,179-180`) — the *same conceptual quantity*, typed two different widths by a human who knew the constructor's input is meant to be a safe integer. bindgen cannot replicate that judgment from syntax alone, so it defaults every `number` to `f64` (always lossless, never wrong, matches vilan's "no implicit numeric conversions" rule — §5.8, `types.md:136-137` — so a `f64` is never silently truncated to reach an `i32` call site) and leaves narrowing to `i8/i16/i32/i53/u8/u16/u32/u53` as a human edit, not a TODO (it's not unmappable, just imprecise). |
| `bigint` | `BigInt` | vilan's arbitrary-precision escape hatch (`types.md:11-13`); a direct match. |
| `symbol`, `unique symbol` | — | No vilan equivalent (no structural identity-only primitive). `// TODO(bindgen): unique symbol has no vilan equivalent`. |
| `any`, `unknown` | `any` | vilan's `any` is "the dynamic top type, produced at host boundaries; it unifies with every type" (`types.md:25-26`) — the honest target for both TS escape hatches, though `unknown`'s TS-side safety (you must narrow before use) has no enforcement on the vilan side once mapped to `any`. Worth a doc-comment, not a TODO (it *is* mapped, just to a wider type than TS's `unknown` intends). |
| `object` | `any` | Same reasoning; too unstructured to do better in v1. |
| `never` | `Never` | Internal-only in vilan (`types.md:27-31`, "not written in source") — bindgen cannot literally emit it as a written type. A TS `never` return type usually means "this function always throws/never returns"; v1 maps it to `void` with a `// TODO(bindgen): TS `never` return — this function may never return normally` note rather than trying to write an unwritable type. |

### 3.2 `undefined`, `null`, and optionality

vilan has no `null`: *"`null` is not a member of ordinary types... std APIs
flatten it at the boundary (`Option`, or a documented sentinel)"*
(`types.md:35-40`). Optional struct members and parameters are exactly
`Option<T>`'s job — vilan's own std `Option<T>` (`enum Option<T> { Some(T),
None }`, `std/src/option.vl:7-10`) is the idiomatic "maybe a value" type, and
externs already use it directly as a parameter type in production std code
(`time.vl:221`, `async external fun wait(self, signal: Option<CancelSignal>):
bool;`).

| TS | vilan |
|---|---|
| `prop?: T` (optional member) | `prop: Option<T>` |
| `f(x?: T)` (optional parameter) | `f(x: Option<T>)` — vilan has no default/optional-parameter sugar; every parameter is required, so an optional TS parameter becomes a required `Option<T>` parameter, pushing the `None` at every call site (a real ergonomic cost worth naming, not hiding). |
| `T \| undefined` | `Option<T>` |
| `T \| null` | `Option<T>` |
| `T \| null \| undefined` | `Option<T>` (both collapse to the one absence case) |
| standalone `undefined` / `null` type | Rare outside a union; treated as `void`/`Option<any>` respectively with a doc-comment, not a TODO. |

**A real fidelity gap, stated honestly:** JS distinguishes an *omitted*
object key from a key explicitly set to `undefined`/`null`, and some host
APIs behave differently for each (a `fetch` `RequestInit` with no `body`
key is a plain GET; one with `body: undefined` can still throw on some
runtimes). A plain vilan `struct` with an `Option<T>` field doesn't, by
itself, choose whether `None` lowers to an omitted key or an explicit
`null`/`undefined` — that's an emission-level decision this proposal
doesn't settle here. This is exactly why std's own `RequestInit` binding
(`fetch.vl:106-130`) is **not** a plain struct with optional fields; it's
an `external struct` built imperatively via setters (`set_method`,
`set_body`, `set_headers`, `set_signal`), called *conditionally* by the
higher-level `Request`/`fetch` code (`fetch.vl:170-181`: the body setter is
only called `if self.method != "GET"`) — sidestepping the
omission-vs-null question entirely by never emitting the key at all unless
the caller's code decides to. §3.7/3.8 recommends bindgen follow this same
precedent for any TS interface used as an options bag passed into a host
call, rather than a field-literal struct.

### 3.3 Unions — the hardest row in this table

vilan has no union types. Three different TS shapes go by the name
"union," and each wants a different treatment:

- **Discriminated unions** (a common literal-typed tag field distinguishing
  the members) map cleanly onto vilan's real tagged-union type, `enum` with
  payload-carrying variants — a shape already proven at scale in std
  (`WsEvent`, `std/src/ws.vl:62-67`: `enum WsEvent { Text(str), Binary(Bytes),
  Ping(Bytes), Closed }`; confirmed as the intended target by the grammar,
  `docs/spec/grammar.md:100-108`, and spec: *"An `enum` introduces a nominal
  sum type; each variant is a constructor (with payload types)"*,
  `types.md:45-47`). `type Shape = { kind: "circle", r: number } | { kind:
  "square", s: number }` becomes `enum Shape { Circle(ShapeCircle),
  Square(ShapeSquare) }` with the tag field dropped (vilan's variant tag
  *is* the discriminant) and each variant's payload fields packaged into
  their own generated struct (`struct ShapeCircle { r: f64 }`) rather than
  a bare positional tuple — preserving field names, at the cost of one
  extra generated type per variant. A variant with exactly one non-tag
  field could collapse to a bare tuple payload instead; keeping the
  per-variant struct uniformly is simpler to generate and to read, and is
  the v1 recommendation.
- **Closed string-literal unions** (`type Align = "start" | "end" |
  "center"`) are extremely common in real `.d.ts` files (CSS-adjacent APIs
  especially) and are, in principle, tractable: emit a plain `enum Align {
  Start, End, Center }`, but because the actual host boundary still speaks
  the raw JS string, the emitted extern for any function taking `Align`
  can't be a bare `external fun` — it needs a private raw extern plus a
  thin public wrapper that matches the enum to its string: `[extern(set,
  "align")] external fun set_align_raw(self, value: str): void;` plus `fun
  set_align(self, value: Align) { self.set_align_raw(match value { Align::
  Start => "start", ... }) }`. This is buildable and worth doing in v1 —
  it's the single highest-value union case in practice — but it's real
  generated logic beyond a bare declaration, which is new territory for
  bindgen relative to every other row in this table (everything else emits
  signatures only, never bodies).
- **Open primitive unions** (`string | number`, `Foo | Bar[]`) have no good
  vilan target — no overloads, no union type, and widening to a common
  supertype loses real information a caller needs. v1 widens to `any` and
  emits `// TODO(bindgen): TS union `string | number` widened to `any` —
  narrow by hand`.

### 3.4 Literal types

A standalone literal type (`"foo"`, `42`, `true`) outside a union is rare
in practice (usually appears as a `const`-inferred return type). v1 widens
to the base primitive (`str`/`f64`/`bool`) with a doc-comment noting the
narrower literal was lost — informational, not a TODO, since the mapping
is total and safe, just less precise than TS's.

### 3.5 Generics

vilan's generics are close enough to TS's for the common case: unbounded
type parameters map directly (`interface Box<T> { value: T }` → `struct
Box<T> { value: T }`), and both languages support defaults on the
parameter itself — vilan's `generic-param` grammar is `[ "type" ] IDENT [
":" (bound-list | tuple-bound) ] [ "=" type ]` (`grammar.md`, the
`generic-params` production) — so `<T = string>` maps to `<type T = str>`
cleanly. Bounded generics (`<T extends Foo>`) map only when `Foo` is
itself something bindgen already emitted or a mapped primitive/nominal
type; a bound against an inline/anonymous object shape has no vilan
equivalent (no anonymous structural bounds) and becomes `// TODO(bindgen):
generic bound `T extends { ... }` has no vilan equivalent — falling back
to an unbounded `T``. Variadic/tuple generics, `keyof`, and mapped generic
utility types (`Partial<T>`, `Pick<T, K>`, …) are out of v1 scope entirely
(§5) — TODO'd rather than attempted.

### 3.6 Functions and callbacks — the asyncness question

The direct mapping is `(x: T) => U` → `|T| U`, `() => void` → `|| void`
(vilan's closure-type grammar: `|T, U| R`, `|| R`, `|| void`, structural in
parameter/return types — `types.md:17-20`). The interesting part is what
happens to `Promise` and to `void`.

vilan's asyncness model is decisive here (`docs/spec/execution.md` §7.3–
7.4, and it already has a real precedent in std, `reactive.vl:568`:
`commit: async |T| Option<str>`):

- **`(x: T) => void`** maps to plain `|T| void` — never `async`. This is
  not a simplification; it's the *correct* mapping, because vilan's
  "divergence rule" makes a void-returning closure slot special: *"legal
  when it returns `void` (spawn semantics): the call fires the closure and
  nobody awaits it... applies to parameters as well"* (`execution.md:172-
  182`). A plain `|Event| void` parameter already accepts an async vilan
  closure passed by the caller — the call fires it and moves on. This is
  exactly `dom.vl`'s existing pattern (`on(self, event: str, handler: ||
  void)`, `on_event(self, event: str, handler: |Event| void)`,
  `dom.vl:90-99`) and it means TS's overwhelmingly common event-handler
  shape (`(e: Event) => void`) needs no asyncness annotation at all —
  vilan's language design already absorbs the "handler might want to do
  async work" case for free.
- **`(x: T) => Promise<U>`** (the callback is explicitly awaited by the
  host) must map to `async |T| U`, not plain `|T| U`. This is *not* the
  same situation as a plain vilan function calling an async parameter
  (which gets polymorphic "adaptation" — §7.4, an async or sync closure
  argument instantiates the right version automatically): adaptation is
  explicitly **excluded** at the host boundary — *"never crosses these
  boundaries: ... a host (`external`) function's value-returning closure
  parameter: host code cannot await a Vilan closure"* (`execution.md:165-
  166`). Every closure bindgen emits sits in an `external fun` signature,
  so this exclusion applies to 100% of bindgen's callback rows: a
  value-returning callback parameter must be typed exactly as async or
  exactly as sync to match what the host actually does with it, with no
  polymorphic middle ground bindgen can lean on. `Promise<U>` in the `.d.ts`
  is precisely that signal.
- **`(x: T) => U`** (synchronous, non-void, non-Promise return) maps to
  plain `|T| U`. Because adaptation doesn't apply here either, the vilan
  caller must supply a genuinely synchronous closure — an informational
  doc-comment on the emitted binding, not a TODO (the mapping is exact;
  it's a usage note for whoever calls it).

`sync` (vilan's contextual keyword forbidding an async closure argument,
`execution.md:139-144`) has no TS source to derive from — TS callback
types don't distinguish "must be sync" from "returns non-promise but may
still be async underneath" — so bindgen never emits `sync`; that's a
human's call to add if a real host constraint demands it.

### 3.7 Classes

Maps directly onto the fetch/dom/bytes precedent: `external struct` +
`impl` block. TS's `new` maps onto the extern form that exists **precisely
for this** — `[extern(new, "ClassName")]` (or `[extern(new, "module",
"ClassName")]` for a named export), which the grammar's own doc comment
states plainly: *"construct a host class instance (host constructors reject
a plain call)"* (`crates/vilan-core/src/node.rs:60-63`). This form is
already load-bearing in std — `bytes.vl:14,93,98,118,128` (`Uint8Array`,
`DataView`, `TextEncoder`, `TextDecoder`), `time.vl:179` (`Date`),
`rpc.vl:290,356,498` (`WebSocket`, `Promise`) — and is the general
mechanism for any class, not a special case; the `Object()`-without-`new`
trick in `fetch.vl:109-110` is a one-off for building a plain object
literal, not the class-construction pattern. (Note for the record: `docs/
spec/grammar.md`'s own `extern-args` EBNF omits `new` — the parser and std
are ahead of that page; not this proposal's problem to fix, but worth
flagging since anyone reading the spec doc alone would miss this form.)

- Instance methods → `[extern(method, "name")]`.
- Instance properties → `[extern(get, "name")]` / `[extern(set, "name")]`
  (only when there's a corresponding TS setter or the property is
  writable).
- **`readonly` properties** map with no special mechanism at all: emit
  only the getter, never the setter. Read-only-ness falls straight out of
  which externs bindgen chooses to write — nothing in the language needs
  to know about it.
- **Static members** (`static method()`, `static readonly prop`) → vilan's
  own static-member mechanism, *"a function without `self` is a **static**,
  reached as `Subject::name(…)`"* (`types.md:65-66`), bound via the
  dotted-global extern form already documented for exactly this shape (*"a
  dotted global, like `history.pushState`"*, `docs/tour/platforms.md`):
  `impl Foo { [extern("Foo.create")] external fun create(): Foo; }`.
- **Constructor overloads** — vilan has none (§3.10): first signature wins,
  the rest TODO'd.
- Out of scope, noted for honesty: a TS class with an explicit
  `dispose()`/`close()` "disposable" shape has a natural vilan analog
  (`resource external struct` + a `Drop` impl) that bindgen does not
  attempt to infer in v1 — recognizing that pattern from a `.d.ts` alone is
  a heuristic, not a parse, and belongs in a later iteration if at all.

### 3.8 Interfaces — structural in TS, nominal in vilan

This is the honest limit of the whole exercise. TS interfaces are
structural: any value whose shape matches is assignable, including inline
object literals, values from unrelated functions, and objects with *extra*
properties beyond what the interface names. vilan structs and external
structs are nominal: only the type bindgen names is assignable where that
name is expected. Two consequences, stated plainly rather than glossed
over:

1. **Extra-property width subtyping has no vilan equivalent.** A generated
   binding is *stricter* than the TS contract it came from — never
   unsound (anything that type-checks against the generated vilan API
   also satisfies the original TS shape), just narrower. This is a real
   limitation, not a bug to fix; it's what "nominal" means.
2. **Anonymous inline object types are common** in real `.d.ts` files
   (an options parameter typed as an inline `{ ... }` shape with no
   interface name at all — extremely common in exactly the kind of
   library this tool targets, e.g. Express's per-call option bags). vilan
   has no anonymous struct types, so bindgen must *synthesize* a name
   (derived from the enclosing function/parameter, e.g. `CreateServer
   Options`) for something that was never named in the source. This is a
   heuristic, and different bindgen runs on a slightly-edited `.d.ts`
   could synthesize different names — worth flagging in §6's byte-
   stability testing and in §8's open questions.

The remaining design question — given an interface, when does bindgen emit
a plain `struct` (literal-constructible, for data the vilan side builds
and hands to the host) versus an `external struct` (opaque, host-owned,
read/written through externed accessors, for data the host builds and
hands to vilan)? — doesn't have a clean syntactic answer from a `.d.ts`
alone; TS doesn't distinguish "this shape is built by calling code" from
"this shape is only ever handed to you by the host." **v1 recommendation:
default every interface to `external struct` + getter/setter externs**,
following the `RequestInit` precedent noted in §3.2 — it's the *safe*
choice (always correct, whichever direction values actually flow, and it
naturally handles the omission-vs-null problem via conditional setter
calls) at the cost of ergonomics for the common "plain options bag the
caller literally constructs" case, where a real vilan `struct` with a
literal initializer would read far more naturally than a chain of setter
calls. Flagged as an open question in §8 rather than resolved here — it's
a genuine judgment call between "always correct" and "usually nicer,"
plausibly one the project owner has a view on.

### 3.9 Index signatures

| TS | vilan |
|---|---|
| `{ [key: string]: T }` | `Map<str, T>` — std's own generic hash map (`std/src/map.vl:11`, `struct Map<K: Hashable, V>`), a direct, already-generic target. |
| `{ [index: number]: T }` (array-like) | `List<T>` — vilan's array/list lang item. |
| An interface mixing named properties **and** an index signature | `// TODO(bindgen): mixed named-property + index-signature interface not supported in v1` — no attempt at the hybrid. |

### 3.10 Overloads — vilan has none

Both function overloads and method overloads are common in real `.d.ts`
files and vilan's `fun` grammar allows exactly one signature per name
(`grammar.md`'s `function` production, `docs/spec/grammar.md:61-71`, has no
repetition/overload form). **v1 policy: first-signature-wins.** The first
overload listed in the `.d.ts` becomes the emitted binding under the
plain name; every other overload is preserved as a comment block quoting
its TS signature verbatim, prefixed `// TODO(bindgen): N additional
overload(s) not represented (see below) — consider a differently-named
function per overload`. This satisfies the TODO invariant exactly: nothing
is silently dropped, and the comment carries enough information (the raw
TS text) for a human to hand-split the overloads into distinct vilan
function names if the extra signatures matter.

### 3.11 Conditional and mapped types — out of scope v1

Explicit in the charter (*"no conditional/mapped types — emit a TODO
comment for the unmappable"*). These are distinct AST node kinds in TS's
grammar (`ConditionalTypeNode`, `MappedTypeNode`, and relatedly `infer`
clauses and template-literal types) that oxc's parser already distinguishes
structurally — detecting them is a syntactic match on node kind, not a
semantic evaluation, so bindgen can reliably *recognize* "this is a
conditional/mapped type" without attempting to *resolve* what it evaluates
to. v1 emits `// TODO(bindgen): conditional/mapped type not supported —
member omitted` at the point of use and moves on; the enclosing interface/
function still gets everything else it can map.

## 4. Platform attribution

**Recommendation: an explicit `--platform` flag, required, no default.**
The charter itself leans this way (*"how `[platform(...)]` attribution is
chosen"* is named as a design question at take-up, alongside the general
note that explicit beats inference for generated code). Concretely:

- `vilan bindgen leaflet.d.ts --platform browser` (accepted values: `node`,
  `deno`, `bun`, `browser`, or the `@process` family — the exact vocabulary
  `[platform("...")]` fences and manifest layers already share, `docs/spec/
  platform.md:3-4,43`).
- Every emitted `external fun` is stamped with `[platform("<value>")]`.
  This is not optional polish — it's the only mechanism that gives
  generated bindings any compile-time platform safety net at all. Per
  `platform_color.rs`'s own module doc comment (the actual authority, since
  `docs/spec/platform.md` describes the *fence* mechanism but not what
  happens in its absence): *"A function's requirement is seeded by its
  definition site — an item defined in a library layer's module requires
  that layer's platforms; **base-layer and user code are unconstrained**"*
  (`crates/vilan-core/src/platform_color.rs:6-9`). std's own browser-only
  bindings get their platform-correctness for free because they live under
  a directory std itself has declared as the `browser` layer
  (`docs/spec/platform.md` §11.1); bindgen output lives in **user** code,
  which that same sentence says is unconstrained absent a fence. A
  generated `external fun` calling a browser-only global, checked into a
  node-targeted project with no fence, compiles clean and fails at
  runtime — exactly the class of bug fences exist to catch at compile time
  instead (`docs/tour/platforms.md`: *"a violation lands at the fence with
  its chain instead of at some distant entry in a dependent build"*).
- One mechanical wrinkle worth naming plainly: `[platform(...)]` is a
  **function-only** attribute — confirmed directly in the parser
  (`parse_struct`, `crates/vilan-core/src/parsing.rs:3011-3055`, never
  calls the platform-attribute parser that `parse_function` does) and in
  the spec's own struct grammar (no attribute prefix on `struct` at all,
  `grammar.md:96-97`). There is no way to fence an `external struct`
  declaration itself once, for all its methods — the fence has to be
  repeated on every single `external fun` bindgen emits for that type.
  Mechanical and noisy, not a blocker, but worth the project owner knowing
  before reviewing a generated file full of repeated fences.
- **An alternative considered and set aside for v1:** std's own layering
  (base/browser/process directories) and a `[library]` package's
  `[library.layer.<name>]` manifest overlay roots (`docs/spec/platform.md`
  §11.1/§11.4) are the *other* mechanism the language has for
  platform-scoping a whole module without per-function fences. It doesn't
  fit here: `[library.layer]` is a section of the `[library]` manifest
  table specifically — a dependency-only package shape — not something a
  plain `[package]` application (the overwhelmingly common bindgen
  consumer: an app pulling in one third-party library for its own use, not
  publishing a library) has access to at all. Per-function fences via
  `--platform` are the only mechanism that works uniformly for both
  shapes, which is the real reason to prefer them over trying to route
  bindgen output through manifest layering.
- Heuristic inference (sniffing `.d.ts` content for DOM-only vs Node-only
  globals) was considered and rejected: many real `.d.ts` files
  (`node-fetch`'s types, isomorphic libraries) intentionally straddle both,
  and a wrong inferred guess baked into checked-in, reviewed source is
  worse than an explicit flag a human had to choose once at generation
  time. `--platform` also composes trivially with re-running bindgen twice
  (once per platform, into two files) for a library genuinely used on
  both, which a single inferred file cannot do at all.

## 5. v1 subset cut line

**In scope:** interfaces (§3.8), plain functions, classes with methods and
properties (§3.7), and "type aliases of mappable shapes" — which, read
literally against the language, needs one clarification: **vilan has no
type-alias declaration at all.** The `type` keyword is reserved
(`grammar.md`'s reserved-word list, `docs/spec/appendix.md`), but its only
use is the generic-binder form (`type X: Bounds` inside `<...>` and impl
subjects) — there is no `type Foo = ...` statement anywhere in the grammar
(confirmed: no `TypeAlias` node, no alias-item production in
`crates/vilan-core/src/parsing.rs` or `grammar.md`'s item list). So "type
aliases of mappable shapes" necessarily means: bindgen resolves a TS `type`
alias to whatever nominal declaration its right-hand shape maps to under
this table — `type Options = { ... }` becomes a `struct`/`external struct`
named `Options`, `type Status = "ok" | "error"` becomes an `enum Status`
(§3.3) — never a lightweight alias, because the target language doesn't
have one. Worth stating explicitly since the charter's phrasing could
otherwise read as if vilan had an alias construct to map onto.

**Out of scope v1**, named honestly against what real `.d.ts` files
actually use (lodash, express, and leaflet as the charter's own mental test
cases, plus what surveying them for this document surfaced):

- **Namespaces** (`declare namespace Foo { ... }`) — common in older-style
  libraries (leaflet's `.d.ts` leans on them for its `L.Map`/`L.Marker`
  nesting). No vilan equivalent to a namespace-as-a-value; a namespace's
  members would need flattening into a dotted-name convention or a nested
  module, neither designed here. TODO'd at the namespace boundary.
- **Declaration merging** (an `interface` and a `namespace` of the same
  name contributing to one type, or multiple `interface Foo` blocks
  merging their members) — a TS-compiler-level semantic, not a syntactic
  one; oxc's parser sees two separate declarations, and merging them
  correctly needs semantic resolution this proposal explicitly avoids
  needing (§2's parser-choice reasoning). Each occurrence TODO'd
  independently rather than merged, which is honest but means a merged-
  interface library (common in DOM-adjacent and Express-style `.d.ts`
  augmentation patterns) gets a visibly incomplete binding.
- **Module augmentation** (`declare module "express" { interface Request {
  ... } }`, extending another package's types) — same semantic-merging
  problem, compounded by needing to resolve which module is being
  augmented. Out of scope.
- **Conditional/mapped types, `keyof`, template literal types** — §3.11,
  §3.5.
- **Ambient global augmentation** (`declare global { ... }`) — out of
  scope; global augmentation isn't really "a third-party library's API,"
  it's closer to the DOM-lib territory §7 explicitly keeps hand-curated.

**Coverage, stated honestly:** lodash's `.d.ts` is mostly plain functions
and generics — good v1 coverage, modulo its extensive overload use (§3.10
handles it, verbosely). express's `.d.ts` leans on interface augmentation
of its own `Request`/`Response` types and namespace-nested types — v1
would generate a binding for the base shapes and TODO the augmentation-
dependent parts, a real but partial win. leaflet's `.d.ts` is
namespace-heavy by convention — v1's coverage there is the weakest of the
three test cases, TODO-riddled rather than unusable. None of this is a
reason not to ship v1; it's the honest boundary the charter itself asked
for ("start: interfaces, functions, classes... no conditional/mapped
types").

## 6. Testing shape

Three gates, modeled on precedent already in the suite:

1. **Golden fixtures.** A small set of checked-in `.d.ts` files (each
   exercising one row of §3's table in isolation, plus one or two
   realistic multi-construct excerpts from real libraries — small,
   hand-trimmed slices of lodash/express/leaflet's actual `.d.ts`, not the
   whole file) paired with their expected `.vl` output, compared **byte-
   for-byte** — the same discipline the corpus gate already uses for
   compiled JS output (`CLAUDE.md`: *"Their `.js` goldens are a
   byte-identical gate"*, `crates/vilan-cli/tests/corpus.rs`). A change to
   bindgen's emitter that alters a golden is either a bug or a deliberate,
   reviewed improvement — never silently regenerated.
2. **The output must parse and check.** Every golden `.vl` file is a
   candidate for the same treatment `docs/` fences get (`cargo test --test
   docs`, `CLAUDE.md`): compiled through the current analyzer with `vilan
   check`, asserting it's diagnostic-clean (or, for a golden deliberately
   containing a TODO'd unmappable construct, that the *rest* of the file
   still checks around the TODO stub). This is the gate that would have
   caught, for instance, a type-mapping row that looks right on paper but
   produces a signature the analyzer actually rejects.
3. **Regeneration is byte-stable.** Running `vilan bindgen` twice on the
   same `.d.ts` with the same flags must produce byte-identical output —
   pinned directly, akin to `init`'s own idempotence-flavored gate
   (`every_template_scaffolds_exactly_its_embedded_files_already_formatted`,
   `crates/vilan-cli/tests/init.rs:317`). This matters specifically because
   of §3.8's synthesized-name heuristic for anonymous interfaces: anything
   involving a generated name must be **deterministic** (a stable
   derivation from the `.d.ts`'s own structure — enclosing symbol +
   parameter position, not e.g. a counter seeded by AST traversal order
   that could drift across oxc versions) or this gate fails by
   construction. Worth treating as a design constraint on the emitter, not
   just a test to add after the fact.

A fourth, softer check worth having once the golden set exists: running
bindgen against the **un-trimmed** real `.d.ts` for lodash/express/leaflet
(not gated in CI — these files change upstream and are large — but a
`cargo test -- --ignored`-style manual check) to keep the "coverage,
stated honestly" claims in §5 from rotting silently as the tool evolves.

## 7. The DOM stays hand-curated

Explicit in the charter and worth restating as a hard boundary rather than
an aspiration: **bindgen targets third-party libraries, not std.**
`std::browser::dom` is 144 lines wrapping a handful of DOM entry points
(`get_element_by_id`, `create_element`, a dozen `Element`/`Text`/`Event`
methods, `dom.vl:1-144`) against a `lib.dom.d.ts` that is enormous — this
is a deliberate, curated *subset* chosen for what `std::ui`/`std::router`
actually need, not an attempt to mirror the full DOM API surface. Running
bindgen against `lib.dom.d.ts` and pointing the result at `std::browser::
dom` would be a regression, not an upgrade: it would balloon std with
every DOM method whether or not anything in the reactive/UI layer uses it,
and would defeat the entire reason `dom.vl` is hand-written and reviewed
line by line in the first place. bindgen is a tool for the *next* library
a user reaches for that std doesn't wrap — not a reason to regenerate
anything that already exists.

## 8. Open questions (with recommendations)

1. **Interfaces: `external struct` always, or a plain `struct` for
   "obviously data" shapes?** (§3.8) Recommendation: `external struct`
   always in v1 — always correct, matches the `RequestInit` precedent,
   costs ergonomics for the common options-bag case. The alternative
   (heuristically distinguishing "data the vilan side constructs" from
   "data the host hands back") would read more naturally for callers but
   needs a heuristic this proposal doesn't have a confident rule for.
2. **Where does generated output live in a project?** (§1) No existing
   convention to anchor a default on. Recommendation: default beside the
   input `.d.ts` (`<stem>.vl`), leaving directory placement (`vendor/`,
   `bindings/`, or just wherever the user runs the command) entirely to
   the developer — but this is a genuinely open call, not a strong one.
3. **String-literal-union wrapper functions** (§3.3) are the one case in
   this table where bindgen generates real logic (a `match` arm) instead
   of a bare declaration. Confirm this is an acceptable scope expansion
   for v1 rather than something to TODO like the other union cases —
   it's high-value (common in practice) but sets a precedent that bindgen
   output isn't *always* signature-only.
4. **oxc's actual `.d.ts` fidelity** was verified for general TS/`.d.ts`
   parsing capability (§2) but not exhaustively probed against every
   construct in §3's table — that needs a real spike against oxc's AST
   once take-up starts, not just documentation reading.
5. **The license-notices gate** (§2) needs `cargo about generate` actually
   run against a lockfile with oxc added; this proposal flags the risk
   (`memchr`'s `Unlicense` alternative-license branch) but doesn't resolve
   it, since no such lockfile exists yet.
6. **Should `[platform(...)]` grow a module/file-level default** so
   bindgen (and any hand-written third-party binding file) doesn't need to
   repeat the same fence on every emitted function? (§4) This is a
   language feature request bindgen's design surfaces but doesn't itself
   need — flagged for the project owner's call on whether it's worth
   opening as its own backlog item.
