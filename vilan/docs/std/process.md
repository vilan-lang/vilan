# Process modules reference

The process layer (Node/Deno/Bun builds): `std::db`, `std::http`,
`std::fs`, `std::process`, `std::rpc_server`, `std::watch`. Task-oriented
usage: [Persistence and the server](../guide/persistence.md).

## std::db: SQLite

```vilan,fragment
resource external struct Database;       // a resource: moves, closes on drop

impl Database {
	fun open(path: str): Database        // ":memory:" for an in-memory db
	fun exec(self, sql: str)             // DDL / one-off statements
	fun prepare(self, sql: str): Statement
}
impl Statement {
	fun run(self, parameters: List<any>): i32          // → last insert id
	fun all(self, parameters: List<any>): List<Row>
	fun first(self, parameters: List<any>): Option<Row>
}
impl Row {
	fun text(self, name: str): str
	fun integer(self, name: str): i32
	fun big_integer(self, name: str): i53   // i53-wide INTEGER (epoch millis)
	fun real(self, name: str): f64
	fun is_null(self, name: str): bool
}
```

Parameters are `?` placeholders. Synchronous by design (fits the rpc
dispatch path). `desc` and other SQL keywords fail as column names.

`Database` is a **`resource`**: it has a single owner and *moves* rather than
copies, and it closes its `node:sqlite` handle when its owner's scope ends. A
`let db = Database::open(..)` local closes on the function's return, with no
`close()` method to remember. `drop(db)` closes it early (the move spends the
binding). A **module-level** `Database` is the serve-forever idiom: it has
process lifetime, never drops, and is reachable only by loan (method calls,
`&`-passing). Moving or `drop`ing a module-level database is a compile error.
Being a resource, a `Database` cannot go into a `List` (use `Option` or a
struct field), cross the wire (`[derive(Wire)]` rejects it), or be a field of a
`[service]` struct (the generated dispatcher would capture the store; keep the
database at module scope instead, next to the service).

## std::http: the server

```vilan,fragment
impl Server { fun builder(): ServerBuilder }
impl ServerBuilder {
	fun port(own self, port: i32): ServerBuilder
	fun on_request(own self, handler: async |Request| Response): ServerBuilder
	fun on_upgrade(own self, handler: |NodeRequest, NodeSocket, Bytes| void): ServerBuilder
	fun on_start(own self, callback: |Server| void): ServerBuilder
	fun on_stop(own self, callback: |Server| void): ServerBuilder
	fun build(self): Server
}
impl Server {
	fun start(self)        // begin listening; holds the event loop
	fun stop(self)         // stop listening; fires on_stop once the listener has closed
	fun port(self): i32    // the bound port (see below)
	fun url(self): str
}

impl Request {
	fun path(self): str
	fun method(self): str
	fun body(self): str      // the body as text
	fun bytes(self): Bytes   // the same body raw (binary POSTs)
}
impl Response {
	fun builder(): ResponseBuilder
}
impl ResponseBuilder {
	fun code(own self, code: i32): ResponseBuilder          // default 200
	fun set_header(own self, name: str, value: str): ResponseBuilder   // repeatable
	fun body(own self, body: str): ResponseBuilder
	fun body_bytes(own self, body: Bytes): ResponseBuilder  // binary body
	fun streaming(own self, on_open: |ResponseStream| void): ResponseBuilder
	fun build(self): Response
}
impl ResponseStream {
	fun send(self, chunk: str)          // write without ending
	fun close(self)                     // end the response
	fun on_close(self, handler: || void)   // the client went away
}
```

`ServerBuilder::port(0)` asks the OS for a free port instead of guessing
one, and the `Server` handed to `on_start` carries the port it actually
bound, so `port()` and `url()` are right in either case:

```vilan,norun
import std::print;
import std::http::{ Response, Server };

fun main() {
	Server::builder()
		.port(0)
		.on_request(|request| Response::builder().body("hi").build())
		.on_start(|server| print(i"listening on {server.url()} (port {server.port()})"))
		.build()
		.start();
}
```

A **streaming** response holds the connection open: once the status and
headers are written, `on_open` receives the live `ResponseStream` and
writes chunks over time (SSE's shape; a suspending `on_open` runs as
spawned work). `on_upgrade` mounts a WebSocket-style handshake handler
over the raw bindings (`NodeRequest`/`NodeSocket`). For an rpc-serving
app you won't touch any of this directly: `serve_service` wraps it
(below), and `serve_connected` itself now rides this surface.

`Server::stop()` closes the listener and fires `on_stop` once it has
actually closed — call it from `on_start` (stash the `Server` value
somewhere reachable, e.g. behind a signal handler or a `/shutdown`
route) or from inside a request handler. Stopping a `Server` value
`start()` never populated (built but never started) is a no-op.

## std::rpc_server

```vilan,fragment
fun serve_service(
	port: i32,
	protocol: RpcProtocol,             // service.dispatcher().into_protocol(codec)
	fallback: |Request| Response,      // plain-http requests
	on_ready: |Server| void,           // `server.port()` is the port actually bound
)

fun serve_connected(port, protocol, on_connection, fallback, on_ready)
	// the same server with the per-connection hook exposed (custom attach/auth)

impl Service {
	fun new(protocol: RpcProtocol): Service   // mounted at "/"; the session-registry lifecycle
	fun at(own self, prefix: str): Service    // mount elsewhere instead, e.g. "/admin/"
	fun on_connect(own self, handler: |i32, DuplexEnd| void): Service
	fun on_disconnect(own self, handler: |i32| void): Service
}
impl ServerBuilder {
	fun with_service(own self, service: Service): ServerBuilder   // repeatable
}
```

Websocket upgrade + session registry (mirror attach/detach) + rpc dispatch;
each handler runs in a turn (`AtEnd`). `serve_rpc`/`serve_service`/
`serve_connected` are sugar over `Server::builder().with_service(…)` —
the underlying layer a server can grow into: install a second service on
its own mount, or a plain page alongside one, by adding a call rather
than swapping to a different `serve_*` function. Details and the client
side: [Services & RPC](../guide/services.md) and the
[rpc reference](rpc.md).

## std::fs

```vilan,fragment
fun exists(path: str): bool                     // sync
fun read_file_to_str(path: str): str            // async, UTF-8
fun read_bytes(path: str): Bytes                // async, true binary read
fun write_file(path: str, contents: str)        // async
fun read_dir(path: str): List<str>              // async, entry NAMES, flat (v1)
fun stat(path: str): Option<Stat>               // async — None if `path` doesn't exist; every other failure throws
struct Stat {
    size: i32,
    modified_at_ms: f64,   // epoch milliseconds
    is_directory: bool,
}
```

`read_bytes`, `read_dir`, and `read_file_to_str` throw host-side on any
failure, missing path included — the same posture `read_file_to_str` always
had. `stat` alone is a non-throwing probe: it exists to let a caller ask
"is this here yet, and what does it look like" (a poller's use case), so a
missing path is `None`, not a thrown exception.

## std::process

```vilan,fragment
fun args(): List<str>            // CLI arguments
fun env(key: str): Option<str>   // environment variable
fun exit(code: i32)
fun scan(): str                  // read a line from stdin
```

A completed `main` ends the process; long-lived programs must hold it open
(a listening server does; a socket-holding client needs an explicit wait).

## std::watch

```vilan,fragment
fun force_refresh(): void   // ask every connected browser to reload once
```

The process layer's dev-mode surface (`dev-refresh.md` §5 item 2) — the
manual channel for a hand-rolled server's own freshness: re-read whatever
changed on disk yourself, then call `force_refresh()` so every browser
connected to the dev channel reloads once and re-pulls it.
`force_refresh()` is a **no-op outside `vilan run --watch`** — it costs
nothing to leave the call in a shipped build. (Named apart from the
browser's [`std::dev`](dev.md) on purpose — the two share no component
source the way, say, `std::ui`'s browser and process halves do, so they
are not the same surface under two names.)

```vilan,norun
import std::fs;
import std::http::{ Response, Server };
import std::watch;

fun main() {
	Server::builder()
		.port(0)
		.on_request(|request| {
			// Re-read whatever this route serves fresh on every request,
			// then ask the browser to pick it up.
			let shell = fs::read_file_to_str("dist/app.html");
			watch::force_refresh();
			Response::builder().body(shell).build()
		})
		.build()
		.start();
}
```
