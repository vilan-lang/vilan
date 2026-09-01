# RPC: request/response, pub/sub, and services over one wire

A working, end-to-end RPC + reactive runtime, written out by hand so the whole
system is visible. The library provides a codec, transports, and two sibling
protocols over them (request/response and publish/subscribe); this example
works through all of it. The reusable runtime is in [`src/rpc.vl`](src/rpc.vl);
the application in [`src/main.vl`](src/main.vl).

```sh
vilan run vilan/examples/rpc
```
```
ok: found ada (@ada)
ok: no such user
raw error: Remote("unknown method: delete_everything")
--- reactive: a remote Source<i32> ---
count = 0
count = 1
count = 2
count = 10
count = 13
count = 16
rpc add -> 16
--- session: the [service(Client)] paradigm, generated ---
status = offline
whoami -> not logged in
login -> false
status = online
login -> true
whoami -> ada (@ada)
```

Everything runs in-process over a local transport — no network, no server to
start.

## The data boundary

The headline: **data crosses the wire only as an explicit *wire type*, and
sensitive data is a type that cannot cross.**

- `Password` is not Wire (no `[derive(Wire)]`). So `[derive(Wire)] struct User
  { password: Password, .. }` *will not compile*: the field `password` of type
  `Password` is not Wire, a compile error. The boundary is enforced by the type
  system, not by a per-field reminder you might forget.
- `User` is the rich, server-side domain type; it holds a `Password`, so it
  never crosses.
- `WireUser` is the **explicit projection** (`User::to_wire`), a
  `[derive(Wire)]` DTO of only Wire fields. It drops `password` and *adds* a
  computed `handle` the domain type has no field for: the wire shape diverges
  freely from the source. The client only ever sees `WireUser`; it has no
  `password` field to leak.

`[derive(Wire)]` enforces the rule directly: **every field of a Wire type must
itself be Wire**, whether a scalar, `str`, `bool`, a `List`/`Option` of Wire,
or another `[derive(Wire)]` type; anything else is a compile error. It reuses
the `Json` round-trip for encode/decode, so a Wire type serializes like a
`[derive(Json)]` one; the difference is the boundary check.

## The layered runtime

The pieces, bottom-up:

| Layer | Here |
| --- | --- |
| **codec** | the `Json`/`FromJson` derives, used directly (frames are JSON `str`) |
| **transport** | `trait Transport` (request/response) + `LocalTransport`; `trait DuplexTransport` + `DuplexEnd` / `duplex_pair` (full-duplex, in-process) |
| **protocol** | `trait Protocol { receive }`: `RpcProtocol` (request/response) and `ReactiveServer`/`ReactiveClient` (pub/sub) all implement it |
| **service** | the hand-written foundation (`accounts_dispatcher()` + `AccountsClient` over `call`/`Dispatcher`) and the generated form: `[service(Client)] struct Session` → `Session::dispatcher()`, the `Client` sibling, and `contract_hash()` |
| **the turn** | every inbound frame is handled in a `batch`, so a handler's signal writes coalesce into one `Update` per source, delivered with the reply |

`call<T>` collapses a client round-trip (build envelope → `await` → decode)
into one line, and `Dispatcher` + `arg`/`reply` replace a hand-rolled
envelope/`match`. It is plain Vilan — the `[service(Client)]` sugar generates
exactly this, which is why the foundation is written out first.

The server `lookup_user` returns `Option<User>`: `None` is an
*application-level* "not found" (part of the return type), separate from an
`RpcError` (an *infrastructure* failure). The dispatcher projects the domain
`User` to a `WireUser` before encoding; the client stub returns
`Result<Option<WireUser>, RpcError>`.

## The reactive protocol

A `Signal`/`Source` is not data; it is a *capability* (a live reference plus
an event stream), so it never rides the codec as a value. `ReactiveProtocol`
is the second protocol, a sibling to RPC over a duplex transport:

- The server `ReactiveServer` holds a per-connection **capability table**:
  `expose(source)` registers a source under a fresh **channel id**; the id is
  what crosses the wire in place of the signal. On a `Subscribe(id)` frame it
  forwards that source's values as `Update(id, json)` frames.
- The client holds a typed `RemoteSource<i32>` (the read-only half of the
  reactive split: client code can't write a server signal; `get`/`sub` only,
  no `set`). Its `sub` opens the channel and observes a local mirror
  (`SignalCell<Option<i32>>`, `None` until the first update) that inbound `Update`
  frames keep in sync; `count = 0` is the current value, delivered on
  subscribe, then `1` and `2` as the server `set`s it. The observer receives
  decoded values, never wire text.

`RemoteSource<T>` mirrors `Source<T>`'s `get`/`sub` shape without implementing
the trait: its `get` is `Option<T>` (no value before the first update), the
honest remote signature.

## The wire turn

The scenario that motivated `std::reactive`'s batching: an RPC call mutates a
signal the client is subscribed to. Without a boundary, every `set` inside the
handler pushes its own `Update` frame, mid-handler, before the reply even
exists. So the runtime handles every inbound frame in a `batch` — the *turn*.
The demo shows all three behaviours:

- A lone `set` outside any batch stays **eager**: one write, one `Update`
  (`count = 1`, `2`).
- An explicit `batch(|| { counter.set(5); counter.set(10); })` **coalesces**:
  the mirror recomputes once, so ONE frame crosses (`count = 10`; the
  intermediate 5 is never observed).
- The `add` RPC method writes the counter twice in its handler; the turn
  defers both, so a single `Update` (`count = 16`) is delivered in the same
  turn as the reply (`rpc add -> 16`). Values commit eagerly (the second write
  reads the first's result); only the *notification* defers.

In-process the update lands just before the reply; a buffering transport
(a WebSocket, say) would flush the coalesced frames and the reply together at
the turn's end.

## The session service

From the annotated `Session` struct and its `[rpc]` impl methods, the compiler
generates `Session::dispatcher(self)` (one route per `[rpc]` method; handlers
capture the session), the *sibling* `Client<T: Transport>` (the two-signature
split: `Session::login(..): bool` vs `Client::login(..): Result<bool,
RpcError>`; the `[expose]`d `status` surfaces as a `RemoteSource` mirror), and
a shared `contract_hash()` on both sides. The `[rpc]`/`[expose]` attributes
are checked: an `[rpc]` signature must be Wire and declare a return; an
`[expose]`d field must be a `Signal` of a Wire element.

- **Per-connection state.** One `Session` is created "on connect"; the
  dispatcher's handlers capture it, so state persists across the connection's
  calls (`login` then `whoami`). Mutable state lives in `Signal`/`Shared`
  handles: closures capture a *copy* of the struct (value semantics), so the
  shared cells are what make the state one.
- **Manual auth.** `whoami` is ordinary body logic over the state `login`
  populated: unauthenticated is an application-level `None`
  (`whoami -> not logged in`). The `Password` check happens entirely
  server-side (`matches` is the only operation the type exposes; the hash
  never leaves).
- **An exposed field.** `Session.status` is exported under a channel id; the
  `Client` carries a `RemoteSource` mirror for it. A successful login flips
  it, and the wire turn delivers `status = online` in the same turn as
  `login -> true` (the failed login changes nothing). The mirror is typed
  (`RemoteSource<str>`): observers receive decoded values, and the codec
  chosen at wiring time is the only (de)serialization anywhere on the path.

## A language note the runtime leans on

A method call on a field-*projection* receiver parenthesizes the receiver, and
a trait bound on a generic field is declared **on the struct definition** (so
the field's type carries it):

```vilan
struct AccountsClient<T: Transport> { transport: T }   // bound on the struct
impl AccountsClient<type T> {                          // the impl infers it
    fun get_user(self, id) { ... (self.transport).call(..) ... }
}
```

The impl does not restate the bound: an `impl AccountsClient<type T>` can only
apply to an `AccountsClient`, whose existence already requires
`T: Transport`, so the binder inherits it. `(self.transport).call(..)` is the
same disambiguation that makes a *closure* field call `(self.handler)(request)`
— the runtime uses it throughout, e.g. `Dispatcher`'s
`(route.handler)(request)`.
