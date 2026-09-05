# std::rpc reference

Transports, the generated service surface, errors, and connection state.
Concepts and usage: the [services guide](../guide/services.md). Most apps
touch only the **generated client**, `RpcError`, and `ConnectionState`;
everything else here is the machinery those sit on.

## The generated surface (`[service]`)

For `[service(FooClient)] struct Foo` with `[rpc]` methods and `[expose]`
signal fields, the macro generates:

```vilan,fragment
// client side
FooClient::connect(url: str, codec: Codec): Result<FooClient<SocketTransport>, RpcError>
client.some_rpc(args…): Result<T, RpcError>     // per [rpc] method; implicitly awaited
client.some_signal: RemoteSource<T>             // per [expose] field; a typed mirror (below)
client.transport: SocketTransport               // connection state lives here

// server side
foo.dispatcher(): Dispatcher                    // the method table
dispatcher.into_protocol(codec: Codec): RpcProtocol   // what Service::new takes
```

`connect` accepts a relative url (`"/"`) in the browser; it dials the same
host over WebSocket, waits for the server's announcement, and verifies the
**contract hash**: a drifted server fails the connect with
`RpcError::Contract`.

## Mirrors: `RemoteSource<T>`

```vilan,fragment
struct RemoteSource<T> { … }

impl RemoteSource<type T> {
	fun get(self): Option<T>                              // passive: the cache, `None` before the first update
	fun status(self): SignalCell<Status>                      // passive: `Waiting` until a value has arrived, then `Ready`
	fun or(self, initial: T): SignalCell<T>                   // counted, owner-scoped: `initial` until the first update
	fun map<U>(self, transform: sync |Option<T>| U): SignalCell<U>   // counted, owner-scoped: the `Option` confronted once
	[must_use]
	fun sub(self, observer: |T| void): Subscription       // counted, manual: present values; dispose to release
}

[derive(PartialEq, Debug)]
enum Status { Waiting, Ready }
```

A mirror holds `Option<T>` — `None` until the first `Update` lands — and
**subscribes by demand**: every `or`, `map`, and `sub` takes a counted lease
on the channel. The 0→1 lease sends `Subscribe` (the server answers with
the current value at once); the 1→0 release sends `Unsubscribe`, deferred
to the end of the ambient turn so a same-turn re-subscribe (a view
re-rendering in place) sends nothing. A second watcher on an open channel
sends no frame. On reconnect a watched mirror (count > 0) re-subscribes on
its fresh channel; an unwatched one does not.

`or` and `map` hand the lease to the ambient owner (the enclosing view, or
a `run_with_owner`), so it is released at unmount; calling either where no
owner is ambient is a compile error (context coverage), by design — a
network subscription must have a releaser. `sub` is the manual form for
code with no owner: you hold the `Subscription` and `dispose` it.

`get` and `status` open nothing. **A `status` observer alone never sees
`Waiting → Ready`**: `status` reports, it does not ask; until something
that renders the value subscribes, the mirror stays `Waiting`, and that is
correct — the channel was never opened.

The `SignalCell<T>` that `or`/`map` return is a local derivative: writing it
writes nothing back (the server owns the source) and the next update
overwrites it. An empty-list `initial` needs no annotation — the `[]`
takes its element type from the mirror
(`let notes = client.notes.or([]);` is a `SignalCell<List<Note>>`).

## Errors

```vilan,fragment
[derive(Wire, Debug)]
enum RpcError {
	Transport(str),   // couldn't reach / lost the server ("not connected", "connection lost")
	Decode(str),      // reply didn't parse
	Remote(str),      // the handler failed
	Contract(str),    // connect-time shape mismatch (old client vs new server)
	Unauthorized,
}
```

Infrastructure failures only: an *application* "not found" belongs in the
rpc's own return type (`Option<Task>`), not here.

## Connection state

```vilan,fragment
enum ConnectionState { Connected, Reconnecting, Closed }

impl SocketTransport {
	fun connection_state(self): SignalCell<ConnectionState>
	fun on_reconnect(self, hook: async || void)
}
```

The reconnect lifecycle (automatic): on drop → `Reconnecting`, in-flight
calls reject with `Transport("connection lost")`, new calls fail fast with
`Transport("not connected")`; dial with backoff (250 ms doubling, 4 s cap,
10 attempts); on success → contract re-check, mirrors re-attach and resync,
`Connected`. Nothing is ever silently retried; retry is the app's decision.

`Closed` is terminal, and three things reach it: the attempt budget runs
out; the re-dialled server's contract has **drifted** (it redeployed a
different surface, so typed mirrors would decode against a shape they were
not built for); or the **re-attach itself is refused** — the server answers,
but will not hand back channel ids. The last two close the socket rather
than staying `Connected`, because a mirror that cannot be rebound is pointed
at the previous connection's dead channels: it would never update again, and
the socket would say nothing was wrong. Bind `Closed` and offer the restart;
that decision is the app's.

On **every** path that reaches `Closed` the client **disposes itself**: its
update routes are emptied and its transport forgets it. The state is what
decides, not the reason for it — a spent budget releases exactly as a drifted
contract does. `Closed` is terminal, so nothing on that socket can ever reach
those mirrors again and nothing keeps holding them: the mirrors read their
last value forever, exactly as they did before, and the graph behind them is
released instead of wedged open. What does *not* dispose is a connection that
is merely slow — a redial in progress, or a server unreachable when the
re-attach runs — because that socket is `Reconnecting`, not `Closed`: it keeps
trying, and its mirrors resync when it succeeds.

`on_reconnect` is where that decision goes. Hooks run after each successful
re-dial, awaited in order, and the generated client registers its own mirror
re-attach when it connects — so **a hook you register runs after the mirrors
have resynced**, which `connection_state` cannot tell you: the state flips to
`Connected` one beat earlier, because the re-attach's own rpc call needs a
usable transport first. Bind the signal for a banner; use the hook for
anything that needs current mirrors.

```vilan,fragment
client.transport.on_reconnect(|| title.repush());
```

Keep a hook short — it runs inside the reconnect loop's own extent, so a long
round-trip inside one holds the reconnect open behind it.

## Transports

```vilan,fragment
trait Transport {
	fun call(self, request: Frame): Task<Result<Frame, str>>;
}
```

| Transport | Wire | Use |
|---|---|---|
| `SocketTransport` | WebSocket (reconnecting) | what `connect` gives you, the production client transport |
| `HttpTransport` | one POST per call | stateless calls, no mirrors |
| `LocalTransport` | in-process | tests: client and service in one process |

Below `SocketTransport` sits `SocketDuplex` (the reconnect-surviving socket:
pending-call registry, inbound dispatch, `on_reconnect` hooks) and the
`DuplexTransport` machinery (`duplex_pair`, `bridge`, `connect_split` for
the SSE/split fallback). App code doesn't construct these; the generated
`connect` does.

Disposing a reactive session (`ReactiveServer`/`ReactiveClient`) also clears
its end's inbound handler — `DuplexEnd::clear_on_frame`, the teardown half of
`on_frame`. Without it the wire kept the closure that captured the session, so
a closed connection stayed reachable from its transport; the server half runs
per disconnect, through `drop_session`.

```vilan,fragment
fun connect_socket(url: str): Result<SocketDuplex, str>   // dial + announcement (backoff)
fun connect_socket_with(url: str, protocols: List<str>): Result<SocketDuplex, str>
fun dial_socket(url: str, protocols: List<str>): Result<SocketDuplex, DialFailure>
enum DialFailure { Unreachable(str), Refused(str) }   // Refused carries "401"/"403"
impl SocketDuplex {
	fun transport(self): SocketTransport
}
```

`dial_socket` is the typed dial the generated `Client::connect` reaches
through: `Refused` is a server that upgraded this client and then declined it
in one frame, which is the only way a refusal can be told from an unreachable
server — no host WebSocket exposes a failed handshake's HTTP status. A refusal
ends the retry budget at the first attempt.  `connect_socket` and
`connect_socket_with` are this with the failure flattened to its sentence.

## Server plumbing (`std::rpc_server`, process layer)

```vilan,fragment
impl Service {
	fun new(protocol: RpcProtocol): Service   // mounted at "/"
	fun at(own self, prefix: str): Service    // mount elsewhere, e.g. "/admin/"
	fun on_connect(own self, handler: |i32, DuplexEnd| void): Service
	fun on_disconnect(own self, handler: |i32| void): Service
	// the handshake gate and its limits
	fun authorize(own self, check: async |Handshake| Result<Session, Reject>): Service
	fun authorize_timeout(own self, millis: i32): Service   // 429 if the hook does not answer
	fun max_connections(own self, limit: i32): Service      // upgraded sockets on this mount only
	fun handshake_rate(own self, attempts: i32, window_millis: f64): Service
	fun handshake_timeout(own self, millis: i32): Service   // bounds the greeting, not idleness
	fun trust_forwarded_for(own self, trusted: bool): Service   // key on X-Forwarded-For
}
impl ServerBuilder {
	fun with_service(own self, service: Service): ServerBuilder   // repeatable
}
```

A service is WebSocket upgrade + per-connection session registration
(mirror attach/detach) + rpc dispatch. Each handler runs in a turn
(`AtEnd`). `ServerBuilder::with_service` installs those routes and the
handshake on a `Server::builder()` chain,
answering **before** `on_request`, so a page and a service sit on one
builder instead of one replacing the other. It is repeatable — a second
service goes on its own mount (`Service::new(protocol).at("/admin/")`),
picked by longest mount and independent of call order.

`Service::new(protocol)` wires the runtime session registry as the
connection lifecycle — what `Client::connect`'s generated `__attach`
answers from; `on_connect`/`on_disconnect` replace it for apps holding
their own per-connection state (connection-scoped auth, an app-written
attach). The `{mount}rpc` route is the server side of `std::rpc`'s
`HttpTransport` (`HttpTransport { url = "http://host:port/rpc" }`), and
`on_request` answers every path no service claims — the app shell,
usually; serving the build's own artifacts is
`ServerBuilder::serve_build`'s job, on the same builder. Details:
[Services & RPC](../guide/services.md#growing-past-one-service) and the
[process reference](process.md#stdrpc_server).

One matching rule is worth knowing: a service claims a path **segment** —
its route exactly, or its route followed by `?` — so `/rpc` does not
shadow an application's `/rpcs` or `/rpc-docs`.

## Envelope & codec layer

`Frame` is the codec-agnostic unit (`std::wire`); `encode_request` /
`open_request` / `encode_reply` read and write the rpc envelope
(`{"method": …, "args": […]}` on the json codec). `Codec` comes from
`json_codec()` (`std::json`) or `binary_codec()` (`std::binary`); both ends
must use the same one. You only meet this layer when implementing a custom
transport or protocol bridge.
