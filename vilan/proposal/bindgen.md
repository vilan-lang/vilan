# bindgen — generating `external` bindings from TypeScript headers (E31)

> **Status: RATIFIED 2026-08-04 (owner review)** — with three owner notes:
> (1) string-literal unions: investigate whether a BACKED enum
> (`enum Foo { a = "a", b = "b" }`) can replace the generated
> match-wrapper — if vilan enums cannot carry backing values today,
> record that as its own language question rather than deciding it
> inside bindgen; (2) the `{ [index: number]: T }` → `List<T>` row is
> CONDITIONAL — confirm whether List tolerates sparse keys before
> shipping that mapping, and correct the row honestly if not;
> (3) v2 direction per owner: an override table + automatic bindgen —
> v1's explicit checked-in file stands, and is the core v2 needs anyway.
>
> v1 SHIPPED 2026-08-06 (v0.30.0) — implementation notes §9, the
> lib.dom.d.ts probe §10 (notes (1) and (2) are answered there and in
> §9.4: backed enums do not exist and are filed as their own language
> question, B76; the array-like row and three siblings are corrected).
> Owner ruling 2026-08-06: §5 WIDENS to the `declare var X: { new(): X }`
> constructor idiom — E37(a) GRANTED; the v0.31.0 bindgen-v2 lane builds
> it with the `--only <Type>` filter (E37(b)).
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

---

## 9. Implementation notes (2026-08-06, take-up)

Everything below was found by **running** the compiler at take-up, not by
re-reading the document. Where it contradicts §§1–8, the running compiler
wins and the deviation is stated plainly. The STATUS block above is
untouched.

### 9.1 The parser: written here, not oxc (deviates from §2)

§2 recommends `oxc_parser` and conditions it on a gate: *"whoever takes this
item up should run `cargo about generate` against a real `Cargo.lock` with
oxc added and read its own output before assuming the license surface is
clean."* That was run. **It fails**, though not on the crate §2 predicted:

```
error: failed to satisfy license requirements
   ┌─ …/dragonbox_ecma-0.1.12/Cargo.toml:40:12
40 │ license = "Apache-2.0 WITH LLVM-exception OR BSL-1.0"
```

`dragonbox_ecma` is reached through `oxc_syntax`, which every oxc crate
depends on unconditionally — no feature flag drops it. Neither licence branch
is on `about.toml`'s **closed** `accepted` list, so adopting oxc means
amending the project's licence policy, which is the owner's call and not a
side effect of a tool.

Two further costs, both measured rather than estimated:

- **44 new crates**, not the "closer to a dozen" §2 predicted — 136 → 180
  packages in `Cargo.lock`, a 32% growth for one non-build-time subcommand.
- §1 puts the machinery in `vilan-core`, and **`vilan-wasm` depends on
  `vilan-core` unconditionally**. The playground artifact is deliberately
  size-tuned (`Cargo.toml`'s `wasm-release` profile exists for exactly that),
  and would link a whole JavaScript parser it can never call.

A third, smaller: oxc 0.143 requires rustc 1.95 and this toolchain is 1.90, so
the lockfile would pin 0.110 — 33 releases behind.

**Resolution: a purpose-built `.d.ts` parser** (`crates/vilan-core/src/
bindgen/dts.rs`). A `.d.ts` is declaration-only — no expressions, no
statements, no bodies, no JSX, no regex literals — so the grammar is small,
and this repo already contains a hand-written lexer and parser for a much
larger language. Cost: **zero** new dependencies, no licence-policy change,
`THIRD-PARTY-NOTICES.txt` and `Cargo.lock` untouched (`cargo test -p vilan-cli
--test third_party_notices` passes unchanged). It chewed 39,429 lines of
`lib.dom.d.ts` in under 8 s. If the owner prefers oxc, the swap is one module
behind an unchanged `bindgen::generate` seam plus two `about.toml` entries.

### 9.2 What crosses a host boundary — four corrected rows

OWNER NOTE 2 asked one row to be verified. Verifying it exposed a shared root
cause under three more: **a vilan aggregate has a vilan-owned runtime
representation, and a host does not speak it.** Only an `external struct` — an
opaque handle reached through `[extern(get/set, …)]` — crosses intact.

| § | Row as written | What running it does | Now |
|---|---|---|---|
| 3.9 | `{ [index: number]: T }` → `List<T>` | `List<T>` is a native JS **array**. An array-*like* (`{0:"a", length:1}`) has no `Symbol.iterator`, so `for`-in throws `TypeError: … is not iterable` — and `map`/`filter`/`fold`/`for_each`/`reverse` are all built on `for`-in. A real array with **holes** is worse than sparse-tolerant: each hole arrives as `undefined` in a `T`-typed slot and crashes on first use. | TODO, naming `Array.from` as the fix. `T[]`/`Array<T>` → `List<T>` **is** correct and kept. |
| 3.9 | `{ [key: string]: T }` → `Map<str, T>` | `std::map::Map` is a **plain vilan struct** wrapping a `NativeMap` keyed by `key.hash()` (`std/src/map.vl:11-13`), not a host object. A host `{"a":1}` read through it dies on `.has`. | TODO, steering to per-key `[extern(get, …)]` accessors. |
| 3.3 | discriminated union → `enum` + payload structs | A vilan `enum` lowers to `[tag, …payload]`; the TS union is a tagged **object**. `match` compiles to `value[0] === 0`, matches no arm, and crashes. Payload `struct`s could not receive the fields either. | TODO, steering to an opaque handle plus a hand-written tag accessor. |
| 3.2 | every absence → `Option<T>` | `Option` is a tagged array (`Some(v)` = `[0, v]`, `None` = `[1]`). **Reading:** a host returning `"hello"` is tested as `value[0] === 0`, i.e. `"h" === 0`, so a *present* value arrives as `None`. **Writing:** `None` reaches the host as `[1]`, which for an optional `boolean` is **truthy** — `arc(…, counterclockwise?)` silently reverses. | Bare type + a `///` note. An optional **parameter** becomes one binding per call arity (§9.3). |

The fact underneath all four: `struct Point { x: f64 }` lowers to `[x]` and
`p.x` to `p[0]`, so a plain `struct` reads the wrong slots of a host object
and yields `undefined` **silently**. This promotes §3.8's v1 recommendation
("`external struct` always") from a judgment call to a requirement — the
alternative it weighed is not merely less ergonomic, it is wrong.

The one aggregate that *does* cross: a vilan **tuple** is a JS array, so TS
`[A, B]` → `(A, B)` is exact.

Both facts are pinned as tests that go red if the language changes
(`crates/vilan-core/tests/bindgen.rs::a_vilan_struct_is_a_positional_array_at_runtime`,
`::option_cannot_cross_a_host_boundary_in_either_direction`).

### 9.3 Optional parameters: one binding per arity (replaces §3.2's row)

Since `Option` cannot cross, and making an optional parameter *required*
would force callers to invent a value the host is meant never to see, the
exact mapping is the one TypeScript's own rule hands over: **optionals are
trailing**, so `f(a, b?)` is exactly two call shapes. bindgen emits both, of
the same host symbol, with the short one keeping the plain name:

```
[extern(method, "getContext")] external fun get_context(self, id: str): …;
[extern(method, "getContext")] external fun get_context_with_options(self, id: str, options: …): …;
```

This is not a new idea — `std/src/browser/dom.vl:63-70` already binds one
`appendChild` twice, as `append` and `append_text`. With two or more optionals
the shortest and longest arities are bound and the intermediate ones are
TODO'd rather than combinatorially expanded.

### 9.4 OWNER NOTE 1 — backed enums do not exist; the question is the language's

**Answer: no.** A vilan enum discriminant is `= (-)? integer` and nothing else
(`crates/vilan-core/src/parsing.rs::parse_discriminant`); `enum Align { Start
= "start" }` is a parse error (*"found '=' expected ',' or '}'"*). So bindgen
ships §3.3's match-wrapper as drafted.

**The language question, recorded here rather than decided inside bindgen:**
should an enum carry a **string** backing value? The precedent is already
built and is exactly the right shape — a C-like enum (`all_data_less &&
any_explicit_discriminant`, `analyzer.rs:15367`) is `is_numeric` and lowers to
its **bare discriminant**: `enum Ordering { Less = -1, … }` compiles
`Ordering::Greater` to `1`, not to a tagged array. A string-backed enum
lowering to its bare string would make bindgen's entire match-wrapper
machinery unnecessary — the enum would *be* the host's string — and would give
the same benefit to every hand-written binding in std (`fetch.vl`'s methods,
`dom.vl`'s event names). It is worth its own backlog item; bindgen is
evidence for it, not the place to settle it.
`crates/vilan-core/tests/bindgen.rs::a_vilan_enum_cannot_carry_a_string_backing_value`
goes red the day it lands, and points at the code to delete.

### 9.5 Smaller deviations and additions

- **§5, type aliases.** An alias whose right-hand side maps to no *declaration*
  (`type GLenum = number`) is now **transparent**: substituted at every
  reference rather than TODO'd. Without it `lib.dom.d.ts` alone reported ~1,500
  references to types it plainly declares (`GLenum` 428, `GLint` 240, …).
- **Attribute order is fixed**, and §4's examples do not say so: the parser's
  chain is `extern`, `must_use`, `rpc`, `trait_only`, `doc(hidden)`,
  `platform` (`parsing.rs:2903-2913`). `[platform(…)]` before `[extern(…)]` is
  a parse error.
- **Interfaces get an `Object()` constructor.** §3.2 cites `RequestInit` as
  the precedent to follow; that precedent *includes* `[extern("Object")]
  external fun new_request_init()` (`fetch.vl:109-110`), without which the
  options-bag direction the section spends three paragraphs on cannot be
  written at all.
- **`extends` is flattened** (base members copied in, derived wins, generic
  arguments substituted) — §3.7/§3.8 do not rule on it. Flattening is what a
  human writing the binding does and the only mapping that leaves the derived
  type usable. It does **not** recover assignability: `Element` is still not
  accepted where `Node` is expected, which is §3.8's nominal limit.
- **Name collisions get a deterministic `_2` suffix** (a property `align` and a
  method `setAlign` both want `set_align`), assigned in source order so §6's
  byte-stability gate holds.
- **A string literal that is not an identifier** is prefixed: `"2d"` in
  `type OffscreenRenderingContextId` becomes variant `_2d`. This was the single
  construct that stopped 410k generated lines from parsing — found by the probe.
- **`--stats`** reports coverage per construct, so §6's "fourth, softer check"
  is a measurement rather than a claim.

## 10. Probe results — `lib.dom.d.ts` (2026-08-06)

The closing step of E31, and the evidence the deferred canvas item (A17) was
waiting on: **can the global APIs be autogenerated?**

**Input.** `typescript@5.9.3`'s `lib/lib.dom.d.ts` — 39,429 lines, 2,415
top-level declarations (1,262 `interface`, 823 `declare var`, 279 `type`, 48
`declare function`, 2 `declare namespace`). Obtained with `npm pack typescript@5`
into a scratch directory; deliberately **not** vendored into the repo.

```
vilan bindgen lib.dom.d.ts --platform browser -o dom.vl --stats
```

### 10.1 Coverage

| | | |
|---|---|---|
| declarations bound | **1,589 / 2,415** | **65.8%** |
| members bound (after `extends` flattening) | **61,224 / 61,317** | **99.8%** |
| output | **489,523 lines** of vilan | |
| `vilan check dom.vl --platform browser` | **exit 0, no errors**, 11.2 s | |

The generated file **compiles**. That is the headline: nothing in the mapping
table produces a signature the analyzer rejects, at DOM scale.

**Skipped declarations — one construct, not many:**

| count | construct |
|---|---|
| **824** | `declare var` (a global VALUE) |
| 2 | `declare namespace` |

**TODOs — 6,753**, plus 33,126 `///` absence notes (counted separately
because the type *is* bound; only the possible `null` is unsayable):

| count | construct |
|---|---|
| 2,448 | open union (`Node \| string`) |
| 1,062 | indexed access type (`HTMLElementTagNameMap[K]`) |
| 1,026 | rest parameter |
| 824 | global variable |
| 538 | function overload |
| 375 | string-literal union property |
| 237 | intermediate optional arity |
| 101 | unresolved type reference (all genuinely cross-file: `ArrayBuffer`, `Uint8Array`, `Float32Array` — `lib.es5.d.ts`) |
| 45 | call signature |
| 41 | numeric index signature |
| 24 | `Promise`-typed property |
| ≤ 8 each | string index signature, `Record`, intersection, unresolved base, namespace, construct signature, template literal |

Two caveats on reading these. **Flattening amplifies:** `lib.dom.d.ts` has 45
`...` rest parameters, but `GlobalEventHandlers` and friends are copied into
hundreds of derived interfaces, so one base member becomes many. And 61,317
members is likewise a post-flattening count. Amplification does not change the
*shape* of the answer, but it does mean the per-construct counts measure
generated surface, not source surface.

### 10.2 What fails, by construct class

- **Globals (the whole shortfall).** Every `[extern(…)]` form binds a *call*
  or a receiver's property; none reads a bare global as a value. `declare var
  document: Document` therefore has no binding. This is 100% of the missing
  34.2%.
- **Overloads** (538) degrade gracefully — first signature wins, the rest are
  quoted. `getContext` happens to list `"2d"` first, so the useful one is what
  binds.
- **Open unions** (2,448) widen to `any`. Mostly `Node | string` convenience
  parameters and `string | number`; the loss is real but local.
- **Indexed access types** (1,062) are the `querySelector<K extends keyof
  HTMLElementTagNameMap>(…): HTMLElementTagNameMap[K]` family — TypeScript's
  tag-name magic, which has no vilan analogue and would need `keyof` support
  bindgen explicitly does not attempt (§3.11).
- **Inheritance chains work.** `HTMLCanvasElement` flattens through
  `HTMLElement` → `Element` → `Node` → `EventTarget` plus the mixin interfaces
  to 516 bound methods, with generic bases substituted correctly. It costs
  size, not correctness — and the assignability limit (§3.8) remains: a
  generated `Element` is still not accepted where `Node` is expected.
- **Namespaces** are a non-issue here: `lib.dom.d.ts` has 2.

### 10.3 The one shape that would close the gap

824 skipped globals, but they are not 824 different problems. **641** are one
idiom, repeated:

```ts
declare var HTMLCanvasElement: {
    prototype: HTMLCanvasElement;
    new(): HTMLCanvasElement;
};
```

A global whose **name matches a declared interface** and whose type is an
object carrying a construct signature. That is a syntactic match, not a
heuristic, and it maps onto an extern form that already exists precisely for
it — `[extern(new, "HTMLCanvasElement")]`. Recognizing it alone would take
declaration coverage from **65.8% to ~92%**, and it is the difference between
"you cannot construct anything" and "you can". It is **not** implemented here:
§5 puts globals out of v1 scope, and widening that is the owner's call, not a
take-up agent's. It is the single highest-value item for v2. The residual ~183
are genuine value globals (`document`, `window`, `navigator`) that want a
different mechanism — a form that reads a global, or a convention that binds
`document.foo` as a dotted global the way `std/src/browser/dom.vl` already
does by hand.

### 10.4 Does a usable canvas surface fall out? **Yes.**

`CanvasRenderingContext2D` binds **99 externs with 19 TODOs**;
`HTMLCanvasElement` binds 516 (flattened); `getContext` binds to the 2D
overload. Written against the generated bindings, with **one** hand-added line
for the entry point:

```vilan,fragment
[extern("document.getElementById")]
[platform("browser")]
external fun canvas_by_id(id: str): HTMLCanvasElement;

fun main() {
    let canvas = canvas_by_id("board");
    canvas.set_width(640.0);
    let context = canvas.get_context("2d");
    context.set_fill_style("rebeccapurple");
    context.fill_rect(10.0, 10.0, 120.0, 80.0);
    context.begin_path();
    context.arc(200.0, 100.0, 40.0, 0.0, 6.28);
    context.set_font("16px sans-serif");
    context.fill_text("drawn through generated bindings", 20.0, 200.0);
    canvas.add_event_listener("click", |event| {
        context.clear_rect(0.0, 0.0, 640.0, 480.0);
    });
}
```

compiles (exit 0) to exactly the JavaScript a person would write:

```js
const canvas = document.getElementById("board");
canvas.width = 640.0;
const context = canvas.getContext("2d");
context.fillStyle = "rebeccapurple";
context.fillRect(10.0, 10.0, 120.0, 80.0);
context.beginPath();
context.arc(200.0, 100.0, 40.0, 0.0, 6.28);
context.font = "16px sans-serif";
context.fillText("drawn through generated bindings", 20.0, 200.0);
canvas.addEventListener("click", (event) => { context.clearRect(0.0, 0.0, 640.0, 480.0); return; });
```

No `Option` noise, correct arities, correct property-vs-method lowering,
correct event-handler closure.

### 10.5 Verdict

**The global APIs CAN be autogenerated — with three caveats, none fatal.**

1. **Entry points are the missing piece, and it is one construct.** Methods
   and properties bind at 99.8%. What does not bind is *reaching* an object in
   the first place. Until globals are handled, every generated DOM module needs
   a handful of hand-written entry bindings — which is a handful, not a
   surface: the canvas demonstration needed exactly one.
2. **Size is the real cost, not correctness.** 39k lines of TypeScript become
   489k lines of vilan, mostly from `extends` flattening. That is fine for a
   generated file nobody reads top-to-bottom, and unacceptable as something to
   *check in* wholesale. A canvas-shaped consumer wants bindgen pointed at a
   trimmed `.d.ts`, or a `--only <Type>` filter that emits a named type and its
   transitive closure. Nothing in the design prevents that; it is not built.
3. **§7's boundary still holds, and this probe reinforces it.** Autogenerating
   `lib.dom.d.ts` over `std::browser::dom` would replace 144 curated lines with
   489k — every method whether or not `std::ui` uses one, 6,753 TODOs, and
   `Element` no longer assignable where `Node` is expected. bindgen is for the
   library std does not wrap. For **a canvas API specifically**, the useful
   read is the opposite direction: generation is a fine way to *draft*
   `CanvasRenderingContext2D`'s 99 methods rather than type them, and a human
   then curates, narrows the `f64`s that are really `i32`s, and names the
   handful of entry points — which is what the DOM binding got and is why it
   is good.
