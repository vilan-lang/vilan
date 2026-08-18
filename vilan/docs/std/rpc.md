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
client.some_signal: Signal<T>                   // per [expose] field; a live mirror
client.transport: SocketTransport               // connection state lives here

// server side
foo.dispatcher(): Dispatcher                    // the method table
dispatcher.into_protocol(codec: Codec): RpcProtocol   // what serve_service takes
```

`connect` accepts a relative url (`"/"`) in the browser; it dials the same
host over WebSocket, waits for the server's announcement, and verifies the
**contract hash**: a drifted server fails the connect with
`RpcError::Contract`.

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
	fun connection_state(self): Signal<ConnectionState>
	fun on_reconnect(self, hook: async || void)
}
```

The reconnect lifecycle (automatic): on drop → `Reconnecting`, in-flight
calls reject with `Transport("connection lost")`, new calls fail fast with
`Transport("not connected")`; dial with backoff (250 ms doubling, 4 s cap,
10 attempts); on success → contract re-check, mirrors re-attach and resync,
`Connected`. Backoff exhausted → `Closed`. Nothing is ever silently
retried; retry is the app's decision.

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

```vilan,fragment
fun connect_socket(url: str): Result<SocketDuplex, str>   // dial + announcement (backoff)
impl SocketDuplex {
	fun transport(self): SocketTransport
}
```

## Server plumbing (`std::rpc_server`, process layer)

```vilan,fragment
impl Service {
	fun new(protocol: RpcProtocol): Service   // mounted at "/"
	fun at(own self, prefix: str): Service    // mount elsewhere, e.g. "/admin/"
	fun on_connect(own self, handler: |i32, DuplexEnd| void): Service
	fun on_disconnect(own self, handler: |i32| void): Service
}
impl ServerBuilder {
	fun with_service(own self, service: Service): ServerBuilder   // repeatable
}

// The boot functions, each a few lines over `Server::builder()`:
fun serve_rpc(port: i32, protocol: RpcProtocol, on_ready: |Server| void)
fun serve_service(
	port: i32,
	protocol: RpcProtocol,
	fallback: |Request| Response,   // plain-http requests: assets + app shell
	on_ready: |Server| void,        // `server.port()` is the port actually bound
)
fun serve_connected(port, protocol, on_connection, fallback, on_ready)
```

A service is WebSocket upgrade + per-connection session registration
(mirror attach/detach) + rpc dispatch. Each handler runs in a turn
(`AtEnd`). `ServerBuilder::with_service` is the layer underneath: it
installs those routes and the handshake on a `Server::builder()` chain,
answering **before** `on_request`, so a page and a service sit on one
builder instead of one replacing the other. It is repeatable — a second
service goes on its own mount (`Service::new(protocol).at("/admin/")`),
picked by longest mount and independent of call order.

`serve_service` and `serve_connected` are short bodies over
`with_service` and keep their exact signatures: the first installs the
runtime session registry as the connection lifecycle, the second exposes
the per-connection hook instead (connection-scoped auth, an app-written
attach). Both take an http `fallback` for every path the service does not
claim — `build_handler(build, …)` is what usually fills it. `serve_rpc` is
the odd one out and deliberately so: no upgrade, no session registry, no
fallback, just the protocol answering every request — the server side of
`std::rpc`'s `HttpTransport`. Reach for `with_service` when the app owns
its builder; reach for a `serve_*` when it does not. Details:
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
