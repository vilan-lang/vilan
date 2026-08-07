# Top-level await — the design half (J3)

> Status: DRAFT (awaiting owner review) — filed from backlog J3; design before code per the entry.
>
> Origin: backlog J3 (`backlog-2026-07-18.md:718-721`), the standing other
> half of a 2026-07-14 fix. The diagnostic half shipped in the v0.4.0
> bundle: an async call in a module-level initializer is refused. The entry
> scopes this paper as "*allowing* awaited module initializers = TLA on the
> emitted ESM (available on every host) plus an ordering story for dependent
> bindings".
>
> Every claim below about what the compiler does today was checked against
> source **or run through the repo compiler** as a probe. The probes are
> called out inline (P1…P11); they ran against `target/debug/vilan` built in
> this worktree from `next @95723e1`, under Node v24.2.0. **They found that
> the entry's premise is wrong in the direction that matters**: top-level
> await is not a feature vilan lacks. It is a feature vilan already emits,
> through a hole in the shipped diagnostic, on a path with no gate, no corpus
> coverage, and two live miscompiles (§1.3, §1.4). §8 is the open-questions
> set; everything before it is a recommendation, not a ratification.

## 0. The problem and the thesis

The backlog entry frames J3 as an *addition*: decide the semantics, then
allow awaited initializers. The survey inverted the question.

The shipped check is **call-shaped, not await-shaped**. It walks each module
binding's `initializer_calls_of` set and errors when a call target is async
(`async_infer.rs:301-394`). An `await` whose operand is not itself an async
*call* — an await of a `Task`-valued binding, of a spawn, of a `Task`
returned by a plain sync function — is not a call to an async target, so the
check never sees it. Those spellings compile clean today, emit a genuine
`const x = await (…)` at the top level of the bundle, and on the browser and
playground legs run correctly (P3, P4).

That would merely be a surprising gap. It is worse than that on two paths:

- On the **Node leg** the emitted TLA silently fails at runtime with
  `ReferenceError: await is not defined` whenever the bundle happens to
  contain no `import` statement — because the emitter parenthesizes the
  await operand (`await (x)`), and `await (x)` is *valid CommonJS syntax*,
  so Node's ESM syntax detection never fires (P8, P9).
- Under **HMR** the same binding is wrapped in the `__hmr_adopt` thunk,
  which is constructed with `is_async: false` (`transformer.rs:3627-3631`).
  The result is `return await (pending);` inside a non-async arrow — a
  bundle that does not parse at all (P10).

**Thesis: J3's design half should not be spent designing a feature. It
should be spent closing a hole.** The recommendation is the null one the
task allows for, and the survey supports it strongly:

**Do not open top-level await. Extend the diagnostic from call-shaped to
await-shaped so that a module-level `await` is refused in every spelling,
and fix the two miscompiles it is currently producing.** The steer the
diagnostic already gives ("wrap the work in a function and call it from
`main`") is stronger than it reads, because **module-level *spawn* already
works** — `let pending: Task<i32> = async ready();` is legal today, starts
the work at load, and is awaited in `main` (P5). That idiom is not a
consolation prize for TLA; on latency it strictly dominates TLA, because it
lets independent loads overlap where TLA serializes them.

Two of the three hard problems TLA is normally bought to solve, vilan has
already solved by other means and would *lose* by adopting TLA's model:
dependency ordering (B33's load-time relation, P6) and cycles (a compile
error with a witness chain, where ESM gives a runtime deadlock, P7).

## 1. Ground truth — what the compiler does today

### 1.1 The shipped diagnostic, and its stated reason (P1)

```rust
// --- Module-level initializers cannot await (backlog §J.3): they run at
// module load, where there is no enclosing function to become async and
// no top-level await in the emission model. A call to an
// (inferred-)async function here would leave a live promise typed as
// `T` — `state + 1` on it is garbage — so it is refused cleanly.
// Creating an async closure (or an `async { .. }` block) in an
// initializer stays legal: nothing awaits at load.
```
`crates/vilan-core/src/async_infer.rs:285-292`

**P1** — the check fires as documented:

```vilan
fun ready(): i32 { sleep(0); 7 }        // inferred async
let state: i32 = ready();
```
```
Error: the initializer of `state` calls `ready`, which is async: a module-level
binding cannot await (module initialization is synchronous); wrap the work in a
function and call it from `main`
```

The check is F6-gated by reachability from `main` (`async_infer.rs:297-307`);
with no user `main` every binding is checked. Its targets are the full async
surface: direct functions and externs, dispatch candidates, `async ||`-typed
values, and adapted instances (`:308-358`). Spec text at
`vilan/docs/spec/execution.md:197`; catalogue entry at
`vilan/docs/appendix/errors.md:403`; pins at
`crates/vilan-core/tests/inference.rs:22387-22445`, `:26861`, `:27533`.

The emitter states the same premise independently, and it is the sentence
this whole paper is about:

```rust
// An async `main` (it awaits) runs inside an invoked async arrow, since
// top-level `await` isn't assumed: `(async () => { .. })()`.
```
`crates/vilan-core/src/transformer.rs:1754-1755`

### 1.2 The boundary: an explicit `await` on a direct async call is still refused (P2)

**P2** — writing `await` does not change the verdict when the operand is an
async call, because the *call* is what is checked:

```vilan
let value: i32 = await ready();
   → Error: the initializer of `value` calls `ready`, which is async …
```

So the rule as implemented is precisely **"no async *call* at load"**. It is
not "no await at load", and the difference is the whole §1.3 hole.

### 1.3 The hole: `await` is not a call (P3, P4)

**P3** — an await of a `Task`-valued module binding compiles clean, emits a
real top-level await, and runs:

```vilan
let pending: Task<i32> = async ready();
let value: i32 = await pending;         // no diagnostic
fun main() { print(value); }
```
```
p3.vl: no errors
```
```js
const pending = __task(async () => {
	return await (ready());
}, "top level");
const value = await (pending);
console.log(value);
```
`vilan run p3.vl` → `7`.

**P4** — three further spellings, all clean, all emitting TLA:

| Spelling | Verdict today |
|---|---|
| `let v = await pending;` (a Task-valued binding) | accepted, emits TLA |
| `let v = await async ready();` (spawn then await) | accepted, emits TLA |
| `let v = await spawn_it();` (`spawn_it` is a **sync** fn returning `Task<i32>`) | accepted, emits TLA |
| `let v = await async { 7 };` (an inline block) | accepted, emits TLA |
| `let v = await ready();` (a direct async call) | **refused** (P2) |
| `let v = ready();` (an implicit await) | **refused** (P1) |

The spawn form is the sharpest: `async ready()` is a *creation*, so the call
to `ready` lives inside the spawned closure and never enters the
initializer's direct call set — exactly the "closure creation is inert" rule
B33's relation depends on (`init_order.rs:19-25`). The two passes are
consistent with each other; both are blind to the `await` node itself.

**No corpus program exercises this.** A grep of every `.js` golden in
`vilan/test/` finds no top-level `await`, so the byte-identical corpus gate
has never seen the shape, and neither has any inference pin —
`inference.rs:22390` mentions "no top-level await in the emission model"
only in a comment.

### 1.4 Miscompile #1 — the Node leg, and the parenthesized await (P8, P9)

`vilan run`, `vilan test`, and each watch round all write the bundle to a
temp file with a **`.js`** extension and hand it to `node`:

- `main.rs:2749` — `vilan-run-<pid>.js` (`run_node_script`)
- `main.rs:2143` — `vilan-test-<pid>.js` (`run_test`)
- `main.rs:924` — `vilan-watch-<pid>.js`

`env::temp_dir()` has no `package.json`, so Node classifies a bare `.js` as
CommonJS unless its **module-syntax detection** promotes it. Detection works
by parsing as CJS and retrying as ESM when the parse throws. That is why the
goldens run at all: `vilan/test/async-await.js:1` opens with
`import { setTimeout } from "node:timers/promises";`, which is an
unambiguous ESM marker.

**P8** — the detection boundary, isolated:

| File | Contents | Node v24.2.0 |
|---|---|---|
| `bare.js` | `const v = await p;` | **runs** — CJS parse throws, ESM retry succeeds |
| `paren.js` | `const v = await (p);` | **`ReferenceError: await is not defined`** |
| `paren_import.js` | an `import` + `await (p)` | runs |
| `paren.mjs` | `const v = await (p);` | runs |

`await (p)` parses as a *call to a function named `await`* under CommonJS.
No SyntaxError is thrown, so detection never fires, and the failure lands at
runtime as an unresolved identifier. The emitter always parenthesizes —
`await (pending)`, `await (ready())`, `await (setTimeout(0))` throughout the
goldens — so vilan's TLA is exactly the form detection cannot see.

**P9** — the live consequence, an import-free program:

```vilan
import std::{ print };
import std::task::Task;
let pending: Task<i32> = async { 7 };
let value: i32 = await pending;
fun main() { print(value); }
```
```
$ vilan run p6.vl
/tmp/vilan-run-1178351.js:33
const value = await (pending);
              ^
ReferenceError: await is not defined
```

A clean compile, a clean type-check, a runtime crash. The program uses no
externs, so the bundle carries no `import` line, so nothing tips Node off.
This is a miscompile in shipping code, reachable from source that the
compiler reports no errors on.

Note the dependence on Node's *version*: syntax detection is only on by
default from Node 22.7 onward. On an older Node even the `bare.js` row
fails, so the fragility is worse than the table shows on the low end.

### 1.5 Miscompile #2 — HMR emits a bundle that does not parse (P10)

HMR wraps every transferable entry-package module binding's initializer in
an `__hmr_adopt` thunk so a cache hit skips it (`hmr.md` §5,
`transformer.rs:3601-3645`). The thunk is built synchronous, unconditionally:

```rust
let thunk = js::Node::Closure(js::Closure {
    parameters: Vec::new(),
    body: thunk_block,
    is_async: false,
});
```
`crates/vilan-core/src/transformer.rs:3627-3631`

An awaited initializer is walked *into* that body.

**P10** — a real watch round (`vilan run --watch --hmr-port 0` over a
two-leg project with `[entry.client] target = "browser"`), reading the
emitted `dist/client.js`:

```js
const pending = __task(async () => {
	return 7;
}, "top level");
const value = __hmr_adopt("pkg::value", 193421165, () => {
	return await (pending);
});
```
```
$ node --check client.mjs
	return await (pending);
	       ^^^^^
SyntaxError: Unexpected reserved word
```

The `Task` binding is excluded from transfer (its type is not a transferable
form), but `value: i32` is `TransferForm::Value`, so it is wrapped — and the
bundle stops parsing. This is not a degraded behavior; the whole dev bundle
is dead. Note the runtime side agrees with the emitter's assumption:
`__hmr_adopt` returns `thunk()` directly (`hmr_shim.js:59-68`), and
`__hmr_adopt_signal`/`__hmr_adopt_shared` write `thunk()`'s result into a
cell (`:73-96`) — a promise-returning thunk would poison them even if the
syntax were fixed.

### 1.6 Module-level spawn already works, and is already concurrent (P5)

This is the load-bearing fact behind the null recommendation.

**P5** — three module-level async shapes that are legal today and always
have been:

```vilan
let pending: Task<i32> = async ready();       // spawn
let block: Task<str>   = async { sleep(0); "hi" };   // async block
let later              = || { ready() };      // an awaiting closure, created

async fun main() {
    print(await pending);
    print(await block);
    print(later());
}
```
```js
const pending = __task(async () => { return await (ready()); }, "top level");
const block   = __task(async () => { await (setTimeout(0)); return "hi"; }, "top level");
const later   = async () => { return await (ready()); };
(async () => {
	console.log(await (pending));
	console.log(await (block));
	console.log(await (later()));
})();
```
`vilan run` → `7`, `hi`, `7`.

The spawn's origin string is literally `"top level"` — the `__Task`
machinery already models load-time-started work as a first-class thing.
**The work starts at module load; only the *observation* moves into `main`.**
That is the entire latency benefit people buy TLA for, and vilan has it
without TLA — with the extra property that N independent module-level spawns
overlap, where N top-level awaits would serialize.

### 1.7 Ordering for dependent bindings already works (P6)

The backlog entry asks for "an ordering story for dependent bindings". B33
shipped it (2026-07-25, `b33-emission-order.md`), and it covers awaited
bindings without knowing about them, because `await x` is a *read* of `x`
and reads are edges (`init_order.rs:34`).

**P6** — the awaited binding declared *before* the one it awaits, plus a
dependent:

```vilan
let value: i32   = await pending;         // declared first
let pending: Task<i32> = async ready();
let doubled: i32 = value * 2;
```
```js
const pending = __task(async () => { return await (ready()); }, "top level");
const value = await (pending);
const doubled = value * 2;
```
`vilan run` → `14`. Topologically sorted, TDZ-safe, canonical-key
tie-broken, byte-stable under import reordering.

### 1.8 Cycles already error at compile time (P7)

**P7** — a cycle closed through awaited bindings:

```vilan
let a: i32 = await mk(b);
let b: i32 = await mk(a);
```
```
Error: `a` and `b` form an initialization cycle: module-level bindings initialize
in dependency order, and a cycle has no such order
  via `a` → `b` → `a`
  declared: `a` in `p9.vl`, `b` in `p9.vl`
  a closure's body is not evaluated at load; moving one of these reads inside a
  closure breaks the cycle
```

This is the comparison that should decide the paper. In an ESM graph a
top-level-await cycle is a **runtime deadlock** with no diagnostic — the JS
ecosystem's single worst TLA failure mode (§3). vilan gives a compile error
with a witness chain and a fix hint, and it does so *because* it does not
have an ESM module graph: it emits one flat, topologically ordered bundle.

## 2. The emitted-module reality, per target

The backlog's premise — "the emitted bundles are ESM, so TLA is available on
every emitted host" — is half right, and the half that is wrong is the half
that matters.

**What is emitted is one file, not a module graph.** Every leg compiles to a
single self-contained bundle: std is compiled in like user code, the `__`
runtime helpers are inlined, and the browser layer's externs are module-less
globals (`web-playground.md:46-49`). The only `import` statements that ever
appear are Node externs (`import { setTimeout } from "node:timers/promises"`).
There is no cross-module ESM evaluation, no deferred module evaluation, no
dependency-graph ordering, and no possibility of a cycle at the *module*
level. Everything in §1.7 and §1.8 follows from that.

Bundle splitting does emit additional files, but it explicitly refuses to
touch this:

> **Module-level bindings never split.** Every module binding stays in the
> entry chunk, in today's single global initialization order. … partitioning
> it across asynchronously-evaluated files would reintroduce exactly the TDZ
> class B33 killed.

`bundle-splitting.md:51-59`, gated by a pin at `:148-149`.

Per target, checked:

| Target | How the bundle is loaded | TLA today |
|---|---|---|
| Browser leg | `<script type="module">` — every entry HTML in the tree (`examples/*/index.html`, `app.html`) | **Works.** Genuine module scope. |
| Playground | emitted JS as a module script inside a per-run `<iframe sandbox="allow-scripts" srcdoc=…>` (`web-playground.md:126-130`) | **Works.** |
| Node — `vilan run` / `test` / watch | temp **`.js`**, `node <script>` (`main.rs:924, 2143, 2749`) | **Broken when the bundle has no `import`** (P9); works by accident when it does. |
| Node — `vilan build` artifact | `dist/<leg>.js`, run by the user however they choose | Same accident, now the user's problem, and invisible until it isn't. |
| HMR dev bundle | `import()` of a Blob URL (`hmr_shim.js:170-175`) | **Bundle does not parse** (P10). |
| Split chunks | `import(specifier)` (`transformer.rs:797-821`) | N/A — module bindings never split. |
| SSR / process leg | the same Node story; one module evaluation per server process | Same as Node. |
| const-eval / macro interpreter | not JS at all — a synchronous Rust evaluator | **Rejected by construction** (§4.3). |

Two of eight rows are outright broken, one is broken-by-luck, and the
"available on every host" claim survives only for browser-shaped targets.

**Recommendation.** Treat "the emitted bundles are ESM" as *false as
stated*. They are ESM-*compatible source* whose host classification is
decided by extension and by the incidental presence of an `import` line.
Any design that leans on ESM semantics must first make the classification
explicit — which is §5.3's fix, and is worth doing regardless of the TLA
call.

## 3. Prior art — what TLA did to JavaScript

Kept short, and honest about which parts transfer.

TC39 shipped top-level await (ES2022) after a long and unusually contested
design process. The contested part was never the syntax; it was what
awaiting does to *everyone else's* evaluation. Three things happened:

- **Execution-order surprises.** A module that awaits stops being a thing
  that "has finished" when its import statement returns. Sibling modules
  that used to observe initialized state observe a half-built module
  instead. The proposal's own answer — every importer of an awaiting module
  becomes asynchronously evaluated — is the correct one and is also the
  problem, because it is invisible at the import site.
- **"Async poisoning" of dependent graphs.** Asyncness is viral upward: one
  leaf module that awaits makes every transitive importer asynchronously
  evaluated. A dependency can turn a synchronous application graph
  asynchronous in a patch release, and nothing in the importing source
  changes. This was the loudest objection during standardization and it was
  accepted as a real cost, not refuted.
- **Deadlock on cycles.** Two modules in a cycle that each await the other's
  export never settle. There is no error, no timeout, no diagnostic — the
  process simply does not proceed. Tooling still does not diagnose this well.

What transfers to vilan, and what does not:

- **The cycle failure does not transfer** — vilan has no module graph to
  deadlock in, and B33 already rejects the cycle at compile time with a
  chain (P7). vilan is strictly ahead here.
- **The poisoning failure does not transfer in its viral form** — there are
  no separately-evaluated vilan modules to poison. But it transfers in a
  *flattened* form that is arguably worse to reason about: one awaited
  binding anywhere in the program stalls the single initialization sequence
  for every binding ordered after it, including bindings in unrelated
  modules that merely lost the canonical-key tie-break. The blast radius is
  the whole program, and the ordering that decides it is derived, not
  written.
- **The execution-order surprise transfers directly** to the one consumer
  that observes initialization from outside: HMR's swap (§4.2).

The honest summary: the JS ecosystem's experience says TLA's costs are paid
by parties who did not write the `await`. vilan's flat-bundle model removes
the worst of those costs and keeps the one that is hardest to see.

## 4. Blast radius, per consumer

### 4.1 SSR and the process twin

`ssr.md` is v1-shipped (S1+S2, 2026-07-23). Its story is
create-serialize-discard: the route handler builds a fresh view against the
process-layer `std::ui` twin and `render(view)` returns markup
(`ssr.md:60-65`); `mount` on the client clears the container and rebuilds
(`:69-71`). The twin is a **module-shadowing** mechanism, not an
instantiation one — one module evaluation per server process, N renders per
process (`examples/ssr/src/server.vl:23-26`).

`render` is sync by signature (`vilan/std/src/process/ui.vl:370`); the
enclosing request handler is `async |Request| Response` and is awaited by
`start()` (`process/http.vl:251, 290-293`).

**Verdict: benign, and the least affected consumer.** An awaited module
initializer on the server resolves exactly once, before `main`, before the
listener binds. It cannot interleave with a render. The only visible effect
is a slower cold start — which is the honest cost of the work, not a
correctness problem. If TLA were allowed, SSR would be its best case.

### 4.2 HMR — the swap

`hmr.md` is fully shipped (S0–S3, A13 complete 2026-07-21). A swap is
capture → teardown → evaluate → restore (`hmr.md:135-150`), where evaluate
is `import()` of a Blob URL and the swap chain is already promise-serialized
(`hmr_shim.js:112-123, 175-189`).

**Verdict: broken today (P10), and the hardest consumer to fix.** Three
distinct problems, in increasing order of difficulty:

1. **Syntax.** The adopt thunk is `is_async: false`. Making it async is a
   one-line change and immediately wrong, because it makes every thunk
   return a promise.
2. **The adopt contract.** `__hmr_adopt` returns `entry.value` on a hit and
   `thunk()` on a miss; a caller writes the result into a `const` and every
   subsequent binding reads that `const` as a value. If the thunk can be
   async the call site must become `await __hmr_adopt(…)`, which is TLA
   again, now unconditionally on every transferable binding in the bundle —
   including the ones that never await. `__hmr_adopt_signal` and
   `__hmr_adopt_shared` are worse: they do `var cell = thunk(); cell[0].v = …`,
   which has no correct promise-shaped rewriting that preserves cell
   identity.
3. **Swap timing.** `import()` resolves only after the new bundle's TLA
   settles, so `restoreScroll`/`restoreFocus` (`hmr_shim.js:183-184`) would
   be delayed by the awaited work. Today they race `main`, because `main` is
   a fire-and-forget IIFE (§4.4); under TLA they would wait on the
   initializers but still race `main`. That is a *change* in which of two
   things restore is synchronized against, and it is the ecosystem's
   execution-order surprise arriving in vilan's one place that observes
   initialization from outside.

Also recorded: the initializer-edit rule (`hmr.md:196-206`) keeps the old
value when a binding's initializer changes but its type does not — so an
edited *awaited* initializer would not re-run on a fingerprint hit. Correct
by the existing rule, and confusing in exactly the way TLA is confusing.

### 4.3 const-eval

`const-eval.md` v1 shipped 2026-07-10; G3 (inferred const) shipped
2026-08-04, release preset only (`const-eval.md:736-738`).

Compile-time evaluation is synchronous by construction and says so in three
places:

```rust
js::Node::Await(_) => Err(Failure::unsupported("await (macro bodies are synchronous)"))
```
`crates/vilan-core/src/interpreter.rs:832`, with the same rejection for
async functions (`:599-600`) and async closures (`:811-812`); the module doc
at `:22` states it as a deliberate v1 bound.

G3's inference sweep does attempt every non-`const` module initializer
(`const_eval.rs:138-142`), so an awaited initializer *does* reach the
evaluator — and bounces off `Unsupported`, falling back silently
(`const-eval.md:653-655`).

**Verdict: unaffected, and it is the cleanest boundary in the system.**
const-eval already refuses await with a message, the fallback is silent by
design, and nothing needs to change under either recommendation. Worth
noting as a precedent for §5's framing: the project already has a subsystem
that says "this evaluation is synchronous, full stop", and nobody has minded.

### 4.4 The entry point

`main` is a function named `main` defined in user code
(`platform_color.rs:688-696`). It is never emitted as a function and never
called by name — its body is **inlined at the bundle tail**, bare when sync,
wrapped in a fire-and-forget async IIFE when it awaits
(`transformer.rs:1754-1765`, quoted in §1.1). `vilan/test/crypto.js:494-498`
and `async-await.js:35-46` are the goldens.

**`main` can already be async, and this is shipped** —
`examples/ssr/src/server.vl:13` is `async fun main()`.

**P11** — an async `main` under a module-level TLA: the TLA lands first, at
true top level, and the IIFE follows it:

```js
const value = await (__task(async () => { return await (ready()); }, "top level"));
(async () => {
	await (setTimeout(0));
	console.log(value);
})();
```

**Verdict: an async `main` is the whole feature, already shipped.** This is
the argument the null recommendation rests on. Anything a user wants to
await before the program does its work can be awaited at the top of an
`async fun main()`, which is one line further down the file, orders
explicitly instead of by a derived topological sort, is visible to every
reader, and is already tested. The one thing `main` cannot do that TLA can
is make a *module binding* hold an awaited value — and §1.6's spawn covers
the latency motive for that, while §5.1 argues the remaining motive is thin.

Recorded as a genuine gap, not papered over: the async IIFE's promise is
**discarded**. Nothing awaits `main`; an unhandled rejection in `main`
surfaces through the `__Task` unhandled-error path or not at all, and the
process may exit before `main` finishes. That is a real wart, it is
independent of TLA, and TLA would *fix* it if `main`'s body were emitted as
top-level statements instead of an IIFE. §8.3 raises it.

### 4.5 Bundle splitting

Shipped 2026-08-04 (S1–S4), opt-in, and currently used by nothing — the
proposal's own measurement says "no example in this repository should
declare `split = true`" (`bundle-splitting.md:442-444`). Chunk loading is
`import(specifier)` with a promise registry (`transformer.rs:797-821`), and
the async-ness is pushed *upstream* of the render because `View.swap`'s
callback is sync (`bundle-splitting.md:62-64`).

**Verdict: no interaction, by an explicit prior decision.** Module bindings
never split; they all stay in the entry chunk in B33's order, pinned. Split
and HMR never co-occur (`bundle-splitting.md:448-453`). The one thing worth
carrying forward: bundle splitting faced this exact question — "may
initialization be spread across asynchronously evaluated units?" — and
answered no, for the TDZ reason. That answer is the same shape as this
paper's.

### 4.6 Summary table

| Consumer | Verdict |
|---|---|
| SSR / process twin | Benign — one init per process, before the listener binds; cost is cold start only. |
| HMR | **Broken today (P10)**; allowing TLA needs the adopt contract redesigned, not patched. |
| const-eval | Unaffected — `await` already rejected by the evaluator, silent G3 fallback. |
| Entry point (`main`) | Already async-capable; TLA adds nothing it cannot do, and `main`'s discarded promise is a separate wart. |
| Bundle splitting | No interaction — module bindings never split, by a pinned prior decision. |
| B33 init order | Already correct for awaited bindings (P6); cycles already rejected (P7). |
| Node host classification | **Broken today (P9)** — the parenthesized await defeats syntax detection. |

## 5. The recommendation

### 5.1 Do not open top-level await — recommend: the null option

The case for allowing it, stated fairly:

- It is the shortest spelling of "this module needs a value that takes time
  to obtain" — a config read, a wasm instantiation, a DB handle.
- ESM supports it, so vilan's browser leg would get it for free.
- Part of it already works, so "allowing" is partly just not breaking it.

The case against, which the survey makes stronger than I expected:

1. **The motive is already served, better.** Module-level spawn (P5) starts
   the work at load *and* keeps it concurrent; `async fun main()` (P11)
   orders it explicitly. Between them there is no user need left that TLA
   uniquely answers — only a syntactic preference for holding the resolved
   value in a module binding rather than a local.
2. **It would forfeit a genuine advantage.** vilan diagnoses initialization
   cycles at compile time with a witness chain (P7). ESM deadlocks. Adopting
   TLA's mental model invites users to expect ESM's shape and be surprised by
   vilan's — or, worse, invites a future design to relax the cycle check to
   "match ESM", which would trade a compile error for a hang.
3. **The cost lands on the derived order.** `init_order.rs:36-40` states the
   law: once emission order is derived, a shape the relation fails to model
   "is not 'left as it was' — it is a miscompile". Adding *suspension* to a
   derived sequence means an unrelated binding's completion now depends on
   an ordering the user did not write and cannot see. That is a category of
   confusion the project has consistently refused elsewhere.
4. **HMR cannot absorb it cheaply** (§4.2). The adopt contract is
   value-shaped at three call sites and in the runtime shim; making it
   promise-shaped is a redesign of a shipped, working subsystem for a
   feature with no driver application.
5. **There is no driver application.** The survey found no program in the
   tree — example, test, std, or website — that wants an awaited module
   binding. The backlog entry itself is a follow-up to a *bug fix*, not a
   feature request. `async-polymorphism.md`'s Part C is deferred on exactly
   this ground ("no driver application yet"), and the same standard applies.

**Recommendation: keep the restriction, and say so in the spec as a
deliberate design position rather than an emission-model limitation.** The
current phrasings — "no top-level await in the emission model"
(`async_infer.rs:287`), "since top-level `await` isn't assumed"
(`transformer.rs:1754`) — read as *we haven't got round to it*. They should
read as *initialization is synchronous, on purpose, and here is what to use
instead*.

If the owner disagrees, §5.5 sketches what allowing it would actually
require. It is not small.

### 5.2 What the diagnostic becomes

Under the null recommendation the diagnostic must change, because today it
does not enforce the rule it claims.

**The rule becomes await-shaped: a module-level initializer may not
suspend.** Concretely, an `Expr::Await` anywhere in a module binding's
initializer — outside a created closure — is an error, in addition to
today's async-call check. The two checks are complementary and both are
needed: the call check catches the *implicit* await (P1), the new check
catches the *explicit* one (P3, P4).

The boundary, stated so it can be pinned per case:

**Stays refused** (today's behavior, unchanged):
- an implicit await — a call to an inferred-async function, extern, dispatch
  candidate, `async ||` value, or adapted instance;
- an explicit `await` on such a call.

**Becomes refused** (the hole, closed):
- `await` of a `Task`-valued module binding;
- `await` of a spawn (`await async f()`, `await async { … }`);
- `await` of a `Task` returned by a sync function;
- any `await` reachable in the initializer's own expression tree.

**Stays legal** (unchanged, and the steer depends on it):
- *creating* an async closure or `async { … }` block at module level;
- *spawning* at module level — `let pending: Task<T> = async f();` — because
  nothing suspends at load (P5). This is the recommended idiom and the
  diagnostic should name it.

The message wants a second form. Today's text steers to `main`, which is
right for the implicit case but unhelpful when the user has explicitly
written `await pending` and `pending` is right there. Proposed second form,
in the house shape, spanned at the `await`:

```
the initializer of `value` awaits: a module-level binding cannot suspend
(module initialization is synchronous)
  note: `pending` is already spawned at load — hold the `Task` here and
        `await` it in `main`
```

Both forms get a catalogue entry (`vilan/docs/appendix/errors.md`), and
spec `execution.md:197` widens from "it cannot await" — which is what it
already says, and is currently untrue — to state the await-shaped rule and
name the spawn idiom.

**Recommendation: adopt the await-shaped rule, with the two message forms.**
The alternative — leave the hole and document it as an escape hatch — is
untenable while §1.4 and §1.5 exist, and would mean shipping a language
whose one-line description of initialization is false.

### 5.3 The two miscompiles, which want fixing either way

These are not conditional on the TLA call. Both are live defects.

**(a) HMR's sync thunk.** Once §5.2 lands, no awaited initializer can reach
the thunk, so P10 becomes unreachable from source. That is a fix by
construction and is sufficient. But the thunk's `is_async: false` should
also carry a comment naming the invariant it depends on, so a future change
to the await rule does not silently re-open it. If the owner takes §5.5
instead, this becomes the redesign in §4.2.

**(b) Node's host classification.** This one is *not* fixed by §5.2, and
that is the finding I would most want the owner to see. The parenthesized
await is only one way to write a construct that CommonJS parses but means
something else in ESM. The general defect is that **vilan emits ESM source
into a `.js` file and hopes Node guesses right.** Today's rescue is
incidental: the presence of a Node extern's `import` line.

Three candidate fixes, in the order I would take them:

1. **Write `.mjs`** for the three temp paths (`main.rs:924, 2143, 2749`) and
   for `dist/<leg>.js` artifacts on the Node leg. Unambiguous, no runtime
   version floor, no emitted-bytes change. The cost is a visible filename
   change for `build` artifacts, which is a compatibility question for the
   owner.
2. **Keep `.js` and pass `--input-type=module`** — does not apply to a file
   argument, so it would mean switching `run` to stdin, which
   `run_node_script:2745-2747` deliberately avoids (the program must keep
   its own stdin for `scan()`). Rejected.
3. **Emit a marker** — e.g. always emit `export {};` on the Node leg. One
   line, no filename change, and it makes every Node bundle unambiguously
   ESM. Cheapest, but it changes every Node golden's bytes.

**Recommendation: (1) for the temp paths, which no user sees and where the
change is free; raise (1)-vs-(3) for `build` artifacts as an owner call
(§8.1).** Either way this wants a pin: a Node-leg program with no externs
that would today be classified CommonJS.

### 5.4 The relationship to `lazy`

`lazy.md` is RATIFIED (2026-07-21) and dormant — S1–S3 queued, not built.
Its §2 gives module bindings first-use initialization with memoization, a
cycle trap, and poisoning.

The three candidate relationships, and the answer:

- **Competitor?** No. They move different things. `lazy` moves *when* a
  synchronous initializer runs; TLA changes *what kind* of work an
  initializer may do. A `lazy` binding is still synchronous at its forcing
  point.
- **Customer?** No, and this is the important negative. `lazy` explicitly
  rules async out: *"**Sync only.** An awaiting argument would smuggle
  asyncness into the callee at an invisible forcing point; deferred async
  already has a spelling (`async expr` → pass the `Task`)"* (`lazy.md:50-53`),
  and §2's binding rule says *"The initializer is sync and context-free"*
  (`:78`). A `lazy` binding that awaited would suspend at an arbitrary first
  touch, deep inside some unrelated call — strictly worse than TLA, which at
  least suspends at a point the initialization order fixes.
- **Complement.** Yes, and this is the verdict. **`lazy` is the better answer
  to most of what TLA is reached for**, minus the awaiting. The motivating
  case in `lazy.md` — "a module binding whose initialization is expensive and
  should not happen at load" — is *the same user problem* TLA gets used for
  in JS, and `lazy` solves it without suspension: the expensive work moves
  off the load path entirely, and if it is genuinely async it is spawned
  (P5) and awaited where it is used.

**Verdict: complement, and a reinforcing argument for the null
recommendation.** `lazy` §2 + module-level spawn + `async fun main()` cover
the demand space between them, each with a clearer failure mode than TLA.
One rider: `lazy.md:50-53`'s sync-only rule is currently justified against
*parameters*; when S2 lands, the binding form should carry the same
justification explicitly, and §5.2's await-shaped check must see through a
`lazy` initializer (a lazy binding that awaits at first touch is the same
defect one level down). Recorded so S2 does not have to rediscover it.

### 5.5 If the owner allows it anyway — what would be required

Recorded so the null recommendation is a choice, not a default.

- **The ordering rule needs no change.** B33's relation already handles it
  (P6), cycles already error (P7). This is the one part of the backlog
  entry's ask that is already done.
- **What a dependent binding sees**: the resolved value, always — because
  the bundle is one topologically ordered sequence and `await` suspends it.
  There is no partially-initialized-module observation to specify, and no
  "asynchronously evaluated importer" concept to introduce. This is the
  design's one genuine simplification over ESM and should be stated in the
  spec as a guarantee.
- **HMR**: the adopt contract redesign in §4.2, all three levels. This is the
  bulk of the work and it is a shipped subsystem.
- **Host classification**: §5.3(b) becomes mandatory rather than
  recommended, since the emitted TLA would then be intentional.
- **`main`'s IIFE**: `transformer.rs:1754-1765` should collapse — with TLA
  available, `main`'s body emits as top-level statements and the discarded
  promise (§4.4) fixes itself.
- **Pins**: every row of §5.2's boundary table, both host classifications,
  the HMR round-trip, a cold-start ordering pin, and a corpus golden — the
  corpus has no TLA today, so the gate is currently blind.
- **Spec**: `execution.md:197` inverts, and §7.6's emission guarantees gain
  the suspension rule.

Sequencing if taken: §5.3(b) first (the host classification is a prerequisite
for TLA being correct anywhere on Node), then HMR, then the rule change.
Estimated M–L, dominated by HMR.

## 6. Migration

**Nothing changes for any program that does not use it, and the default path
is provably untouched.**

The evidence, not the assertion:

- **The corpus has no TLA.** No `.js` golden in `vilan/test/` contains a
  top-level `await` — grepped. The ~100-program byte-identical corpus gate
  is therefore untouched by §5.2, and by §5.3(a).
- **No std, example, or website program uses an awaited module binding.**
  The shape that would produce one (P3/P4) is not present in the tree.
- **§5.2 only ever adds errors** to programs that today compile into one of
  the two miscompiles. There is no program that works today and stops
  working — a program in the hole either crashes at runtime (P9), fails to
  parse under HMR (P10), or is browser-only and would newly be refused.
  That last set is the only real migration cost, and it is empty in this
  tree.
- **§5.3(a) touches only temp filenames** (`main.rs:924, 2143, 2749`) — not
  emitted bytes, so the corpus gate is neutral by construction. Whether
  `build` artifacts change name is the owner call in §8.1; if they do, that
  *is* a user-visible change and belongs in the changelog as such.

The one shape that would need a migration note is a browser-only program
relying on the hole. For those the steer is mechanical and the diagnostic
should print it: hold the `Task` in the module binding, `await` it in
`main`.

## 7. Slices

Under the null recommendation (§5.1). Suite-gated, docs in the same commit,
per-case pins per CLAUDE.md.

1. **S1 — the host classification fix** (S). `.mjs` for the three temp paths
   per §5.3(b)(1); a Node-leg pin for an extern-free program. Independent of
   everything else and fixes a live defect. Take this first even if the TLA
   call goes the other way.
2. **S2 — the await-shaped rule** (S–M). The `Expr::Await` check beside the
   existing call check in `async_infer.rs`, wired into both `lib.rs` and the
   CLI's duplicated sequence per the standing rule; the second message form;
   the catalogue entry; spec `execution.md:197`. Pins: every row of §5.2's
   boundary table, per case — the four newly-refused spellings, the two
   already-refused, and the two that stay legal (creation, spawn).
3. **S3 — the HMR invariant comment and its pin** (S). `transformer.rs:3627`
   gains the comment naming what it depends on; a pin that the awaited-binding
   shape is refused before it can reach the thunk (proving P10 unreachable,
   not merely absent).
4. **S4 — docs** (S). The tour/async page names module-level spawn as the
   idiom; the spec states initialization-is-synchronous as a design position
   with the steer, not as a limitation.

S1 is independent. S2 before S3 (S3's pin depends on S2's rule). S4 last.

## 8. Open questions

### 8.1 Should `build` artifacts on the Node leg become `.mjs`? — recommend: raise, do not decide here

`dist/<leg>.js` is a user-visible artifact name; changing it is a
compatibility question with downstream scripts, Dockerfiles, and process
managers, none of which this repo can see. The temp paths (§5.3(b)(1)) are
free and should just change. For artifacts the choice is `.mjs` (clean, a
visible break) versus an emitted `export {};` marker (invisible, but
rewrites every Node golden's bytes and adds a line of noise to output the
project treats as a showcase).

**Recommendation: `.mjs` for temp paths in S1; artifacts stay `.js` plus
the `export {};` marker, so the break is in bytes we already gate rather
than in a filename users script against.** I hold this one loosely — it is
a distribution-facing call and genuinely the owner's.

### 8.2 Is the null recommendation the owner's position, or only mine? — flagged as the owner's call

This is the paper's headline and I want it named as a decision rather than
absorbed as a conclusion. The survey supports "keep the restriction" on the
evidence in §5.1, and I am confident about points 1–4. Point 5 ("no driver
application") is the weakest, because absence of demand in a young tree is
weak evidence about demand in general, and the owner has visibility into
intended applications that this repo does not contain.

**Recommendation: keep the restriction, with a named trigger for revisiting
— the first program that genuinely cannot be expressed with spawn + async
`main` + `lazy`.** If such a program exists already, §5.5 is the design and
the sequencing is there.

### 8.3 `main`'s discarded promise — recommend: file it separately

`transformer.rs:1754-1765` emits `(async () => { … })()` and drops the
promise. Nothing awaits `main`; a rejection surfaces only through the
`__Task` unhandled-error path, and on Node the process can exit before
`main` settles. This surfaced during the TLA survey but is not a TLA
problem — it is an entry-point problem that TLA would incidentally fix.

**Recommendation: file it as its own backlog item, not a rider here.** Its
own fix (await the IIFE, or emit a `.catch` that sets the exit code) is
small and independent, and bundling it with a paper whose recommendation is
"change nothing about the model" would muddle both.

### 8.4 Should the await-shaped check see into `lazy` initializers? — recommend: yes, when S2 of `lazy` lands

Per §5.4's rider. A `lazy` binding is sync by its own ratified rule
(`lazy.md:78`), so the check must apply to it too — but `lazy` is dormant,
so there is nothing to check yet.

**Recommendation: record it as a trigger on `lazy.md`'s S2 rather than
building for a keyword that does not exist.** The one-line note belongs in
`lazy.md` §2 so it is read at implementation time.

### 8.5 Should the diagnostic's spawn steer be a fix-it? — recommend: no, not yet

The `await pending` → hold-the-Task-and-await-in-`main` rewrite is
mechanical enough to be an LSP code action. But it moves code across a
function boundary and has to invent or find the `main` line, which is more
than the existing fix-it machinery does.

**Recommendation: ship the note text in S2, and let a code action follow
if the diagnostic turns out to fire often.** Prose first is the cheaper
experiment.
