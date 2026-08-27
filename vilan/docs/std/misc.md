# Misc reference

The small modules that don't need a page of their own: `std::io`,
`std::task`, `std::promise`, `std::context`, `std::crypto`, `std::jwt`,
`std::asset`.

## std::io

```vilan,fragment
fun print(message: any)                     // console.log
fun panic(message: str)                     // abort with a message
fun assert(condition: bool, message: str)   // panic when false
```

`panic` is for unreachable states (expected failures are `Result`). A
`panic` arm in a `match` diverges: the other arms decide the match's
type. `assert` is the `vilan test` failure mechanism.

## std::task

```vilan,fragment
external struct Task<T>;
impl Task<type T> {
	fun settle_all(tasks: List<Task<T>>): List<T>   // async; implicitly awaited
	fun race(tasks: List<Task<T>>): T               // async; first settled wins
}

fun nursery<T>(body: (|Nursery| T) context ambient_nursery): T

external struct Nursery;
impl Nursery {
	fun cancel(self)                 // abort the extent's signal
	fun is_cancelled(self): bool     // the compute-loop check
	fun signal(self): CancelSignal   // the raw host AbortSignal
}

resource struct OwnedNursery { nursery: Nursery }
impl OwnedNursery {
	fun new(): OwnedNursery                                       // a detached owner
	fun enter<T>(&self, body: (|| T) context ambient_nursery): T  // spawns inside → owned
	fun cancel(&self)                                             // early, idempotent
}
// `impl OwnedNursery with Drop` cancels the owned nursery at scope end.

fun ambient_signal(): Option<CancelSignal>   // the enclosing nursery's, if any
```

Tasks only arise from spawning (`async expr`); see the
[async tour](../tour/async.md). The handle is opaque and copying it
refers to the same task. Every task absorbs its own failure: a later
`await` receives it, and a task nobody awaits reports the error (with
its spawn origin) instead of crashing the program. Keep the task instead
of the results by spawning the `settle_all` itself:
`let pending = async Task::settle_all(tasks);`.

`nursery(body)` joins every task spawned in the body's dynamic extent:
the body's value passes through, the first-observed failure re-raises
with its spawn origin, and everything else is absorbed. `cancel()`
aborts the nursery's signal; `sleep` and `fetch` carry the ambient
signal automatically, so in-flight IO in the extent rejects promptly,
and those rejections absorb as cancellation echoes. `Task::race` +
`cancel()` is the race idiom: first settled wins, the losers' IO stops.
Spec: [§7.7](../spec/execution.md). `ambient_signal()` bridges host
APIs std doesn't wrap.

`OwnedNursery` is the owner for background work no function-scoped
`nursery` can hold: a task whose lifetime is an object's, not a call's.
It is a `resource`: it has a single owner, moves, and is destroyed
deterministically. `new()` makes a detached owner; `enter(body)` runs
`body` with the owner's nursery ambient, so every task spawned inside
registers with it. Unlike `nursery`, `enter` does NOT join: it
returns as soon as the body settles, leaving the tasks running.
Dropping the owner (at scope end, or `drop(owner)`) cancels them, so
in-flight bridged IO aborts. Because the owner's nursery is never joined
it runs **detached**: a child's REAL failure reports to the console with
its spawn origin (as a free-floating task would) instead of being
absorbed for a join, and children do not cancel their siblings
(ownership is lifetime, not fate-sharing). Cancellation echoes, from the
owner's `cancel` or drop, stay silent.

## std::promise

```vilan,fragment
external struct Promise<T>;
impl Promise<type T> {
	fun all(promises: List<Promise<T>>): List<T>   // async; implicitly awaited
}
```

The raw host promise, for direct host interop: an
`[extern(new, "Promise")]` constructor or a promise-returning host API
is typed `Promise<T>`, and `await` unwraps it like a task. Code
that only spawns never sees this type.

## std::context

Ambient values with dynamic extent, the machinery under `owner_scope` and
`turn_scope`:

```vilan,fragment
impl Context<type T> {
	fun new(): Context<T>
	fun run<U>(self, value: T, body: || U): U   // establish for the body's extent
	fun get(self): T                            // read (compile error if possibly absent)
	fun get_safe(self): Option<T>               // read, absence as None
}
```

- `get` is **statically covered**: the compiler proves every call path runs
  inside a `run`; an uncovered read is a compile error, not a runtime
  `None`.
- Closures capture their contexts **at creation**; parameters declare
  context needs with the `context` clause
  ([functions & closures](../tour/functions-and-closures.md)).
- Async-safe by construction: a continuation sees the value captured at
  creation, across awaits and interleaved extents.

Define module-level contexts for app-wide ambients (a session identity on
the server is the canonical use).

## std::crypto

```vilan,fragment
fun random_bytes(length: i32): Bytes        // cryptographically secure
fun random_uuid(): str
fun equals_constant_time(a: Bytes, b: Bytes): bool   // timing-safe compare
async fun hmac_sha512(key: Bytes, data: Bytes): Bytes
async fun pbkdf2_sha512(password: Bytes, salt: Bytes, iterations: i32, bits: i32): Bytes
```

WebCrypto-backed (async where the host is). `pbkdf2_sha512` is the v1
password-hashing primitive — store the salt beside the derived hash and
compare with `equals_constant_time`; `hmac_sha512` signs raw byte
messages (for tokens, `std::jwt` below is the shaped surface). Neither
needs an extern any more.

The std surface is **async** because WebCrypto is. On a path that must
stay sync — the walkthrough's rpc dispatch hashes passwords inside a
sync method — binding the host's sync primitive as an extern is still
the right move: the walkthrough example binds Node's `pbkdf2Sync` that
way, and that lesson stands.

## std::jwt

HS512 JSON Web Tokens; claims are any `Wire` type:

```vilan,fragment
async fun sign_hs512<C: Wire>(secret: Bytes, claims: C): str
async fun verify_hs512<C: Wire>(secret: Bytes, token: str): Option<C>
fun decode_claims<C: Wire>(segment: str): Option<C>   // decode WITHOUT verifying
```

`verify_hs512` checks the signature (constant-time) before yielding claims;
`decode_claims` is for non-security introspection only.

## std::asset

```vilan,fragment
fun emit(kind: str, line: str)   // compile-time only: append to a build asset
fun read(path: str): str         // compile-time only: read a project file
```

Both callable only from `const` evaluation — a runtime call path to
either is a compile error. `emit` is how `std::style` writes the CSS
file (`emit("css", rule)`). Reach for it directly only for a shape std
has no spelling for: a whole declaration block under a selector you
choose is `std::style::declare`, which builds the line — and the layer
around it — for you. A browser build with emissions produces
`<entry>.css` beside `<entry>.js` (beside `<entry>.mjs` on a process
target).

`read` is the channel's input direction: it returns a project file's
text at build time, so its result can fold into the output —
`const markdown::parse(asset::read("pages/intro.md"))` bakes a parsed
page into the bundle as plain data. The path is **relative to the
package root** (the base imports resolve under, never the process
working directory); an absolute path or one that escapes the root is
refused. Every file read becomes a **tracked build input**: `--watch`
re-runs when one changes (or when a previously missing one appears),
and an unchanged-source round still recompiles a leg whose read inputs
changed. A missing file is a compile error at the `const` expression.
Reads charge the const fuel budget per byte, so the budget bounds input
size exactly as it bounds computation.
