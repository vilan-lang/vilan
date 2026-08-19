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
	fun serve_build(own self, build: LegBuild): ServerBuilder   // one route per artifact
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

`serve_build` is the one call that replaces the boot reads and the
content-type table a full-stack server used to write by hand. It takes a
[`LegBuild`](#stdbuild) and installs one route per artifact at
`/<file name>` — the bundle, the style sidecar if the leg emitted one, and
every route chunk — in front of `on_request`, whatever order the chain was
written in. So the app's catch-all still answers every path the build does
not claim, and a leg that gains `split = true` gains its chunk routes with
no server edit. It is the one way a server serves its build: an rpc app
that wants it puts its service on the same chain with `with_service`
([below](#stdrpc_server)) rather than reaching for a `serve_*` boot
function, which hands you only a fallback and no builder to install on.

Three details are decisions, not defaults. The route shape is
`/<file name>`, which is what every shell already asks for, so adopting it
moves no HTML. Content types come from a short fixed table (`.js`/`.mjs`,
`.css`, `.json`, `.html`); anything else is not served, because
`serve_build` serves a *build*, not a directory. And an artifact the build
named but did not write **stops the server at boot**, naming the file and
the leg, rather than 404ing for the life of the process.

Freshness is its dev-mode policy: under `vilan run --watch`
([`is_watching`](#stdwatch)) each asset is re-read per request, so a
rebuild is served without a restart; otherwise the copy read at boot is
served from memory.

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
fun read_file_to_str_sync(path: str): str       // sync, UTF-8 — blocks the event loop
fun read_file_encoded(path: str, encoding: str): str   // async — decode with any host encoding
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
missing path is `None`, not a thrown exception. Prefer the async read; the
sync one exists for a read that must complete inside a callback that cannot
suspend — `serve_build`'s dev-mode revalidation is the case it was added
for.

Three reads, three different questions. `read_bytes` is the true binary
read: the host hands back a `Buffer`, which binds straight to `Bytes` with
no decode in between, and it is what serves an image, a font or a favicon.
`read_file_to_str` is that read decoded as UTF-8 — the one almost every
caller wants. `read_file_encoded(path, encoding)` is the same decode with
the encoding named (`"utf8"`, `"latin1"`, …), for a file that is text but
not UTF-8; `read_file_to_str` is a one-line call to it. (It was called
`read_file_bytes` until v0.34.0, which is the name that made the rename
worth doing: it promised bytes and returned a decoded string. No alias was
kept.)

## std::build

What a browser leg's build emitted, as the build itself knows it — the
value that lets a server stop restating `"dist/client.js"` from memory.

```vilan,fragment
struct LegBuild {
	leg: str,               // `client`, for `[entry.client]`
	dist: str,              // where its artifacts live — `dist`
	bundle: str,            // `client.js`
	styles: Option<str>,    // `client.css`, or None if the leg compiled no styles
	chunks: List<str>,      // route chunks, empty unless the leg splits
	classic_script: bool,   // true exactly when it splits
}

fun build_of(leg: str): Result<LegBuild, BuildError>   // async
fun require_build(leg: str): LegBuild                  // async; stops if it can't
```

`build_of` reads `dist/<leg>.chunks.json`, which every build of a browser
leg writes. A leg that was never built is `Err(BuildError::NotBuilt)` and
not an empty build — the manifest's presence is the difference, which is
why it is written even when the leg does not split. `require_build` is the
boot idiom: a server that cannot describe its own build has nothing to
serve, so it stops with the error's message instead of starting.

`LegBuild::artifacts()` gives `(url, file)` pairs — `("/client.js",
"dist/client.js")` — in serving order. `std::http`'s
[`serve_build`](#stdhttp-the-server) is the consumer that makes them routes.

## std::document

The HTML document a browser leg is loaded by, held against what that leg's
build emitted — so the `<script>`, the `<link>` and the mount element
cannot disagree with the artifacts they name.

```vilan,fragment
enum ShellFault {
	StylesNotLinked(str),         // the build emitted styles; the document links none
	LinkedStyleMissing(str),      // it links a stylesheet this build did not emit
	ScriptNotEmitted(str),        // it loads a script this build did not emit
	BundleNotLoaded(str),         // this build's bundle is loaded by no <script>
	MountMissing(str),            // nothing carries the id the client mounts into
	ModuleScriptWithChunks(str),  // a splitting leg loaded as a module script
}
impl ShellFault {
	fun message(self): str        // what is wrong, and what to do about it
}

fun check_shell(shell: str, build: LegBuild, mount: str): Result<void, List<ShellFault>>

impl Document {
	fun of(build: LegBuild): Document                     // generate one from the build
	fun from_shell(shell: str, build: LegBuild): Result<Document, List<ShellFault>>

	fun title(own self, title: str): Document
	fun lang(own self, lang: str): Document
	fun mount(own self, id: str): Document                // the other end of mount_root
	fun head(own self, markup: str): Document             // raw, appended inside <head>
	fun body(own self, markup: str): Document             // raw, before the script tag
	fun render(self, view: View): Document                // SSR markup, inside the mount element

	fun html(self): str
}

fun require_shell(path: str, build: LegBuild): Document   // async; stops if it can't
```

**The check is the primitive.** `check_shell` takes a plain `str`, so it
works on a shell produced any way at all — read from disk, templated,
fetched from a CMS — and reports *every* fault, not the first. That
ordering matters: a generator only protects documents it generated, and
the page most likely to be wrong is the hand-written one somebody dropped
to for a CSP header or a font preload.

**A fault stops the server from starting.** `require_shell` is the boot
idiom: it reads the file, checks it, and either hands back a `Document` or
stops with the file, the leg, and one line per fault. Refusing is
defensible because the check is cheap, total, and about the *build* rather
than about a request — it cannot fail intermittently, and a server that
starts with a document that cannot work is worse than one that does not
start.

```vilan,norun
import std::build::require_build;
import std::document::require_shell;
import std::http::{ Response, Server };

async fun main() {
	let build = require_build("client");
	// src/app.html is yours; it is checked against what the build wrote.
	let page = require_shell("src/app.html", build).html();

	Server::builder()
		.port(8080)
		.serve_build(build)
		.on_request(|request| Response::builder().set_header("Content-Type", "text/html").body(page).build())
		.build()
		.start();
}
```

An app that genuinely means it says so, once, in code — `check_shell`
returns a `Result`, so the decision is yours:

```vilan,fragment
match check_shell(shell, build, "app") {
	Ok(let _checked) => {},
	// Report and carry on instead of stopping.
	Err(let faults) => faults.for_each(|fault| print(i"warning: {fault.message()}")),
}
```

What it will and will not have an opinion about is bounded by what a
build knows. This leg's artifacts are `<leg>.js`, `<leg>.css` and
`<leg>.<Arm>.js`, and the leg's last build owns that namespace, so a
document loading `<leg>.…` files this build did not emit is loading its
own stale output and is told so. A `/theme.css` your application serves
itself is outside that namespace — the check says nothing about it, and a
stylesheet on another origin (a font CDN) is nobody's business but yours.
Comments and `<script>`/`<style>` bodies are skipped rather than searched,
so a commented-out `<link>` links nothing and a `<div id="app">` inside a
script's own string is not a mount element.

**`Document::of(build)` writes the document instead.** Same value, same
rules — every document it can produce passes `check_shell`, which is what
keeps the generator and the checker from drifting apart. It emits a
doctype, `<html lang>`, charset, viewport, `<title>`, the stylesheet link
*if and only if* the build emitted styles, the mount element, and the
bundle's script tag in the form the build requires (a classic script for a
leg that splits, since chunk resolution reads `document.currentScript`):

```vilan,norun
import std::build::require_build;
import std::document::Document;
import std::http::{ Response, Server };

async fun main() {
	let build = require_build("client");
	let page = Document::of(build)
		.title("Notes")
		.head("<style>body { font: 16px/1.5 system-ui; }</style>")
		.html();

	Server::builder()
		.port(8080)
		.serve_build(build)
		.on_request(|request| Response::builder().set_header("Content-Type", "text/html").body(page).build())
		.build()
		.start();
}
```

`head`/`body` take raw markup and append (a favicon, an `og:` tag, a CSP,
a `<noscript>`), which is what keeps the generated document small enough
to be worth having: everything else is derived. They are an escape hatch,
not an exemption — markup you add there is checked like any other, so a
`<link>` to a stylesheet the build did not emit is caught wherever it came
from.

`render(view)` is the server-rendering splice ([SSR](../guide/ssr.md)):
the markup goes *inside the mount element*, because the document already
knows where that is. It takes `self` rather than `own self` — it is the
one method called per request, on a document the handler built once at
boot:

```vilan,fragment
.on_request(|request| Response::builder().body(page.render(app()).html()).build())
```

`html()` returns a `str`, at every rung — a generated document can be
post-processed with the same string operations an app already uses, which
is what keeps the hand-written shell (rung 0) and the generated one made
of the same material.

## std::process

```vilan,fragment
fun args(): List<str>            // CLI arguments
fun env(key: str): Option<str>   // environment variable
fun exit(code: i32)
fun scan(): str                  // read a line from stdin
```

Server-side hot-swapping of code is not a thing
here and is not planned — the node leg restarts.

A completed `main` ends the process; long-lived programs must hold it open
(a listening server does; a socket-holding client needs an explicit wait).

## std::watch

```vilan,fragment
fun is_watching(): bool     // is this a `vilan run --watch` child?
fun force_refresh(): void   // ask every connected browser to reload once
```

`is_watching()` is defined under every run and is `true` only under
`vilan run --watch`, so a program branches on it without knowing how it
was started. It is about *data* freshness, not code: `serve_build` uses it
to revalidate its assets per request while watching, and a hand-rolled
server can do the same.

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
