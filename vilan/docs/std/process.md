# Process modules reference

The process layer (Node/Deno/Bun builds): `std::db`, `std::http`,
`std::fs`, `std::process`, `std::rpc_server`, `std::watch`. Task-oriented
usage: [Persistence and the server](../guide/persistence.md).

## std::db: SQLite

```vilan,fragment
resource external struct Database;       // a resource: moves, closes on drop

struct Migration { name: str, sql: str }

impl Database {
	fun open(path: str): Database        // ":memory:" for an in-memory db
	fun exec(self, sql: str)             // DDL / one-off statements
	fun prepare(self, sql: str): Statement
	fun migrate(self, migrations: List<Migration>): List<str>  // → the names applied
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

`migrate` carries a schema forward, which `CREATE TABLE IF NOT EXISTS`
cannot: it applies the named steps this database has not seen, **in list
order**, and records each in a `vilan_migrations` table it owns
(`name TEXT PRIMARY KEY`, `applied_at_ms INTEGER`). Call it at boot,
before the first query; a re-run applies nothing and returns an empty
list. Each step runs in its own transaction *with* its record insert, so
a step is recorded if and only if its SQL committed — which is why a step
must never contain `BEGIN`/`COMMIT`/`ROLLBACK` of its own, or a statement
SQLite refuses inside a transaction (`VACUUM`, `PRAGMA journal_mode`).

Three conditions stop the boot with a `panic`, all of them before
anything is applied: a step whose SQL **fails** (named, with the host's
message, and nothing recorded for it); a **recorded step absent from the
list**, meaning this database was migrated by a newer build; and an
**unapplied step ordered before an applied one**, meaning a step was
inserted into the past. Names must be unique and must never change once
recorded; they need not sort in order, because the list's order is the
one that counts. No down-migrations — roll forward with a new step. The
task-oriented version, including carrying `.sql` files through the const
channel, is in [Persistence](../guide/persistence.md#migrations-carrying-a-schema-forward).

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
	fun cache_build(own self, policy: |str| CachePolicy): ServerBuilder   // opt in to caching
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
	fun header(self, name: str): Option<str>   // a request header, case-insensitive
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

// conditional requests (ETag / If-None-Match)
fun etag_of(body: Bytes): str          // async — a strong validator, quoted
fun if_none_match_matches(header: str, etag: str): bool
fun etag_response(request: Request, etag: str, body: Bytes, content_type: str): ResponseBuilder

// what `cache_build` asks for, per served artifact
impl CachePolicy {
	fun none(): CachePolicy                                    // no validator, no header
	fun validated(): CachePolicy                               // ETag + 304
	fun cache_control(own self, value: str): CachePolicy       // the field value, verbatim
}
```

`Request::header` reads one header off the request, and the conditional
request built on it ships whole. `etag_of` mints a **strong validator** for
a body: the first 32 hex digits of its sha-256 with the quotes included,
because an entity-tag *is* quoted on the wire — 128 bits is enough that
colliding with a body you did not choose is a second preimage, at half the
header weight of the full digest. `etag_response` then answers the request:
a `304 Not Modified` with the ETag echoed and no body when the request's
`If-None-Match` already names the tag, or the full `200` with `ETag`,
`Content-Type` and the bytes. It hands back the `ResponseBuilder` still
open, so your policy headers chain after it and land on **whichever arm
answered** — which is exactly what a 304 needs, since it carries the
headers its 200 would have carried:

```vilan,norun
import std::bytes::encode_utf8;
import std::http::{ Server, etag_of, etag_response };

async fun main() {
	let page = encode_utf8("<h1>hello</h1>");
	let tag = etag_of(page);   // once, where the bytes settle

	Server::builder()
		.on_request(|request| {
			etag_response(request, tag, page, "text/html; charset=utf-8")
				.set_header("Cache-Control", "no-cache")
				.build()
		})
		.build()
		.start();
}
```

The matching is RFC 9110's, all three forms of the field: a single
entity-tag, a comma-separated list of them, and `*` (which matches any
representation the server holds). Comparison is **weak**, as the RFC
mandates for `If-None-Match` — a `W/` marker on either side is ignored — so
a client revalidating through a proxy that weakened the tag it forwarded
still gets its 304. Only `GET` and `HEAD` revalidate: on any other method a
matching `If-None-Match` means `412 Precondition Failed`, a state-changing
route's protocol, so there the header is not consulted and the full
response goes out. One documented limit: the list form is split on commas,
so an `etag` that itself embeds a comma cannot be found in a list — no tag
`etag_of` mints does. `if_none_match_matches(header, etag)` is the
comparison half on its own, for a handler that wants to build both arms
itself. Task-oriented usage — the two-tier cache policy this slots into —
is in [the guide](../guide/persistence.md#caching-etag-and-304).

Header names are **case-insensitive**: node lowercases every name it parses
off the wire, and `header` lowers `name` to match, so the casing you write
and the casing the client sent are both irrelevant. `None` means the request
did not carry the header, which stays distinct from `Some("")` for one sent
empty. A header the client **repeated** reads back joined with `", "` — the
list form ordinary headers already define — so nothing is silently dropped,
but nothing is split for you either; `set-cookie` is node's one unjoined
header and arrives comma-joined without the space, which a cookie value's
own commas make unsafe to split, so repeated `set-cookie` is not readable
through this accessor.

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
`/<name>` — the bundle, the style sidecar if the leg emitted one, every
route chunk, and every resource the leg bundled with
[`const asset::bundle`](misc.md#stdasset) — in front of `on_request`,
whatever order the chain was written in. So the app's catch-all still answers every path the build does
not claim, and a leg that gains `split = true` gains its chunk routes with
no server edit. It is the one way a server serves its build: an rpc app
that wants it puts its service on the same chain with `with_service`
([below](#stdrpc_server)) rather than reaching for a `serve_*` boot
function, which hands you only a fallback and no builder to install on.

Three details are decisions, not defaults. The route shape is `/<name>`,
which is what every shell already asks for, so adopting it moves no HTML —
and a bundled resource keeps its package-relative path, so a subdirectory
survives into the url rather than being flattened onto the site root. Content types come from a table generated from
[`mime-db`](https://github.com/jshttp/mime-db) — the registry aggregate
vite's own mime lookup is generated from — covering what a build emits and
what a page it serves loads as a whole sub-resource: scripts, styles and
markup, json and the web manifest, images, fonts and wasm. An extension
outside it is still not served, because `serve_build` serves a *build*, not
a directory — and the skip is said, not silent: boot prints a warning naming
the artifact it will not serve. And an artifact the build named but did not
write **stops the server at boot**, naming the file and the leg, rather than
404ing for the life of the process.

Artifacts are served as **bytes**, exactly as the build wrote them — a
favicon or a font arrives byte for byte, which a table alone would not
achieve. That is also why every `text/*` row spells `; charset=utf-8`: a raw
body carries no encoding of its own, so an unspelled `text/css` would be
decoded by whatever the browser defaults to. `application/json` and its
`+json` relatives take no charset, being utf8 by spec, and `.webmanifest` is
served `application/manifest+json` because Chrome rejects a manifest under
any other type.

Audio and video are deliberately absent from the table. `serve_build` writes
a whole body and honours no `Range` header, so a browser could not seek in
anything it served; a row for `.mp4` would type a response that does not
work. Media belongs behind the static file server §5.10 defers, not here.

Freshness is its dev-mode policy: under `vilan run --watch`
([`is_watching`](#stdwatch)) each asset is re-read per request, so a
rebuild is served without a restart; otherwise the copy read at boot is
served from memory. Both halves read bytes, so a watch never serves a
freshly decoded — and freshly corrupted — copy of a binary artifact.

**Caching is opt-in, and off by default.** `serve_build` sends one header,
`Content-Type`, and the bytes — a browser cache policy is an application's
decision, not a default std can guess right for every deployment, and
`serve_build` serves a build rather than a directory. `cache_build` is the
one call that opts in: it asks your policy, per artifact, what to send
beyond the bytes, keyed on the route the build claimed. The two-tier shape
every static layer converges on is one expression:

```vilan,norun
import std::build::require_build;
import std::http::{ CachePolicy, Response, Server };

async fun main() {
	Server::builder()
		.serve_build(require_build("client"))
		.cache_build(|url| if url.starts_with("/chunk-") {
			// A fingerprinted name cannot go stale: cache it for a year.
			CachePolicy::none().cache_control("public, max-age=31536000, immutable")
		} else {
			// Everything else revalidates — a round trip, but never a body.
			CachePolicy::validated().cache_control("no-cache")
		})
		.on_request(|request| Response::builder().code(404).body("Not Found").build())
		.build()
		.start();
}
```

`CachePolicy::validated()` mints an [`etag_of`](#stdhttp-the-server)
validator over the bytes being served and answers a matching
`If-None-Match` with a `304`; `cache_control` sets the field value
verbatim, on either base, and it reaches the `304` arm as well as the
`200`. The policy is asked about the **url** and nothing else, so it cannot
quietly become a second request handler — anything genuinely per-request
belongs in `on_request`, built from `etag_response` directly. A validator
costs one sha-256 per hit, over the bytes actually served, which is what
keeps it correct under `run --watch`. Leave `cache_build` off the chain and
nothing changes: the response is byte-for-byte what it was before the hook
existed.

A **streaming** response holds the connection open: once the status and
headers are written, `on_open` receives the live `ResponseStream` and
writes chunks over time (SSE's shape; a suspending `on_open` runs as
spawned work). `on_upgrade` mounts a WebSocket-style handshake handler
over the raw bindings (`NodeRequest`/`NodeSocket`). For an rpc-serving
app you won't touch any of this directly: the service layer
(`with_service`, below) rides this surface.

`Server::stop()` closes the listener and fires `on_stop` once it has
actually closed — call it from `on_start` (stash the `Server` value
somewhere reachable, e.g. behind a signal handler or a `/shutdown`
route) or from inside a request handler. Stopping a `Server` value
`start()` never populated (built but never started) is a no-op.

## std::rpc_server

```vilan,fragment
impl Service {
	fun new(protocol: RpcProtocol): Service   // service.dispatcher().into_protocol(codec);
	                                          // mounted at "/"; the session-registry lifecycle
	fun at(own self, prefix: str): Service    // mount elsewhere instead, e.g. "/admin/"
	fun on_connect(own self, handler: |i32, DuplexEnd| void): Service
	fun on_disconnect(own self, handler: |i32| void): Service
}
impl ServerBuilder {
	fun with_service(own self, service: Service): ServerBuilder   // repeatable
}
```

Websocket upgrade + session registry (mirror attach/detach) + rpc dispatch;
each handler runs in a turn (`AtEnd`). `Server::builder().with_service(…)`
is the one spelling, and the server grows by adding a call rather than
swapping boot functions: a second service on its own mount, a plain page
alongside one, the build's artifacts via `serve_build` — all on the same
chain. Details and the client
side: [Services & RPC](../guide/services.md) and the
[rpc reference](rpc.md).

## std::fs

```vilan,fragment
// reading
fun read_file_to_str(path: str): str            // async, UTF-8
fun read_file_encoded(path: str, encoding: str): str   // async — decode with any host encoding
fun read_bytes(path: str): Bytes                // async, true binary read
fun stat(path: str): Option<Stat>               // async — None if `path` doesn't exist; every other failure throws

// writing
fun write_file(path: str, contents: str)        // async
fun write_bytes(path: str, contents: Bytes)     // async — the binary write, `read_bytes`'s mirror
fun write_atomic(path: str, contents: str)      // async — temp sibling + rename, never a torn file
fun write_bytes_atomic(path: str, contents: Bytes)  // async — the byte twin of `write_atomic`
fun append(path: str, contents: str)            // async — extends, never truncates
fun update(path: str, revise: |str| str)        // async — read, revise, replace atomically
fun copy(from: str, to: str)                    // async — file copy; the source survives
fun rename(from: str, to: str)                  // async — move/replace; atomic within one filesystem
fun remove(path: str)                           // async — deletes a FILE

// directories
fun read_dir(path: str): List<str>              // async, entry NAMES, flat
fun read_dir_all(path: str): List<str>          // async, RELATIVE paths, the whole tree
fun scan_dir(path: str): List<Entry>            // async, flat, WITH each entry's kind
fun create_dir(path: str)                       // async — one level; EEXIST if it's already there
fun create_dir_all(path: str)                   // async — the whole chain; idempotent
fun remove_dir(path: str)                       // async — must be empty
fun remove_dir_all(path: str)                   // async — the whole tree; a missing path is a no-op
fun copy_dir(from: str, to: str)                // async — the whole tree, merged into `to`

struct Stat {
    size: i32,
    modified_at_ms: f64,   // epoch milliseconds
    is_directory: bool,
}

struct Entry {
    name: str,
    is_directory: bool,
    is_file: bool,
    is_symlink: bool,
}

// the handle tier — an open file, positional and stateless
resource external struct File

impl File {
    fun open(path: str): File        // "r"  — read; must exist
    fun create(path: str): File      // "w"  — create, truncating what was there
    fun create_new(path: str): File  // "wx" — create; FAIL if it already exists
    fun append_to(path: str): File   // "a"  — every write lands at the end
    fun modify(path: str): File      // "r+" — read/write in place; must exist

    fun read_at(self, buffer: Bytes, position: i53): i32   // bytes read; 0 at EOF
    fun write_at(self, buffer: Bytes, position: i53): i32  // bytes written
    fun stat(self): Stat             // no Option — the handle is already open
    fun truncate(self, length: i53)  // shrink, or extend zero-filled
    fun sync(self)                   // fsync — the durability primitive
    fun data_sync(self)              // fdatasync — data only
}

fun with_file<T>(path: str, body: |File| T): T   // open, run, close — close AWAITED

// the watch tier — a live watch, pulled one change at a time
resource external struct Watcher

enum ChangeKind { Created, Modified, Removed }

struct Change {
    path: str,          // the watched root joined with the entry — addressable as-is
    kind: ChangeKind,
}

impl Watcher {
    fun watch(path: str): Watcher      // the path and, if a directory, its immediate entries
    fun watch_all(path: str): Watcher  // the path and everything under it, however deep

    fun next(self): Change             // async — the next change, awaiting one if none is pending
}
```

Everything here throws host-side on any failure, missing path included —
the same posture `read_file_to_str` always had — with exactly two
exceptions, both non-throwing because a missing path already satisfies
what the call *means*. `stat` is a probe: it exists to let a caller ask "is
this here yet, and what does it look like" (a poller's use case), so a
missing path is `None`. `remove_dir_all` means "make sure this is gone",
which a path that was never there already is, so removing something absent
is a no-op. Any *other* failure of either — a permissions error, a busy
directory — still throws. If you need to know whether a thing was there,
`stat` before you act.

Nothing here is synchronous, and that is the whole calling convention:
`read_file_to_str(path)` is implicitly awaited and reads like a plain
call, so a sync variant buys a caller nothing and costs the event loop the
length of the operation. There used to be two synchronous entries, and
both are gone — a sync read justified by a caller that could not suspend
(no such caller existed), and `exists`, which outlived it by one release:
its justification named a category ("boot code") rather than a caller,
every caller it actually had was already async, and
`stat(path).is_some()` answers strictly more. That is the probe now —
`stat(path).is_some()` for "is this here", `stat(path).is_none()` for the
guard clause.

Three directory listings, one honesty policy. `read_dir` is deliberately
flat: immediate entry *names*, not path-joined, no file-vs-directory
distinction. `read_dir_all` walks the whole tree in one call — every entry
under the path, files and subdirectories alike, as paths *relative* to it,
joined with the host's own separator. `scan_dir` is `read_dir` with the
kinds: flat again, but each entry arrives as an `Entry` that already knows
whether it is a file, a directory or a symlink, so the thousand `stat`
calls a thousand-entry directory used to cost are gone — the host had that
information all along and `read_dir` threw it away. None of the three
promises an order; sort the list if order matters.

`Entry` carries three booleans rather than a kind enum, because a host
directory entry has *nine* kinds — file, directory, symlink, FIFO, socket,
block device, character device, unknown — and an enum would have to either
model five nobody will ever meet or carry a catch-all meaning "one of five
things I did not model". Three booleans answer the three questions people
actually ask, and an entry that is none of the three reads back with all
three `false`, which is true rather than wrong. The kinds do not follow
symlinks: a link to a directory is `is_symlink = true` with
`is_directory = false`, which is what stops a recursive walker from
following a loop it cannot see. `stat` *does* follow, so that is how you
ask what a link points at.

Making and unmaking directories comes in a strict form and a forgiving one,
and the pairing is deliberate. `create_dir` makes exactly one level and
fails with `EEXIST` if something is already there — which makes it the only
way this module can claim a name exclusively — while `create_dir_all` makes
every missing level and succeeds when the whole chain already exists, so
"make sure this place exists before I write into it" never fails for having
already run. `remove_dir` refuses a directory that is not empty;
`remove_dir_all` takes the tree and everything under it. `copy_dir` copies
a tree into `to`, creating it if needed: files already there with a
counterpart in the source are overwritten and files with no counterpart are
left alone, so it is a merge, not a mirror.

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

One write that cannot tear. `write_file` truncates the target and then
fills it, so a process that dies partway through leaves a half-written
file — and a store that reads itself back at boot then finds corrupt data
where its state used to be. `write_atomic` writes to a uniquely-named
sibling and `rename`s it over the target instead: a crash leaves either the
previous file intact or the complete new one, never a mix. Use it for
anything the program reads back later — a JSON store, a cache, a config
the app rewrites. Atomic is not durable, though: after a *power* loss the
rename may not have reached the device, and closing that last gap needs
`fsync`, which the host exposes only on an open file handle — that is
`File::open(path)` then `file.sync()`, below. The temporary is a sibling because a rename across
filesystems is not atomic and would fail outright, which also means a
crashed run can strand a `<path>.<uuid>.tmp` file beside the target;
`vilan` has no `try`/`catch` to sweep it up. `rename` is the primitive
underneath, and it is also how you move a file: the destination is
replaced if it exists, and the source stops existing.

Bytes go out as well as in. `write_bytes` is `read_bytes`'s mirror and it
was missing for a while, which meant a program could read a favicon and
could not write one back: `writeFile` was bound once, typed for `str`. It
takes `Bytes` and the bytes reach the file unchanged — no encode, no
decode, no replacement character. That is not a small promise. A text round
trip through UTF-8 turned kolt's 483-byte favicon into 853 bytes served,
one U+FFFD for every byte that was not a legal sequence, and that is the
failure `write_bytes` exists to make unrepresentable.
`write_bytes_atomic` is the same call with `write_atomic`'s temp-sibling
discipline, for an image or a font a running server is reading.

`append` extends a file and never truncates it, creating it if it is not
there — the right call for a log, the wrong one for a store, because an
append is not atomic and a crash partway through leaves a partial line.
`update(path, revise)` is the read-modify-write every JSON store and every
config rewrite is made of, with the write half already atomic: the file is
read as UTF-8, handed to your closure, and what comes back is written
through `write_atomic`. It is not a lock, and the difference is worth
knowing before you rely on it — two processes calling `update` on one path
can still interleave read-read-write-write, and the second write wins
whole. `update` closes the *tearing* window, not the *racing* one; closing
that would need advisory locking, which the host has no access to. For a
single writer — a store, a cache, a config — it is the complete answer.
`revise` is a plain value-returning closure, so it is asyncness-polymorphic:
a revision that awaits makes that `update` await too, with nothing to
write at either the declaration or the call site.

`copy` duplicates a file and the source survives, which is the whole
difference from `rename`; an occupied destination is replaced. `remove`
deletes a *file* and refuses a directory — the split is the host's, and
`remove_dir`/`remove_dir_all` are the other side of it. There is no
synchronous variant of any of these, and none was asked for: a sync variant
of an async operation has to name the caller that cannot suspend, and in
this module there is exactly one such caller in the whole language — a
module-level `let`, which cannot await — so the bar is a name, not a
category.

Everything above is a complete operation on a path — open, act, close, in
one call. What needs more than one act on the *same* open file is the
handle tier. `File` is a `resource`: it moves rather than copies, a stale
binding is a compile error rather than an `EBADF`, and its destructor
closes the underlying handle at the owner's scope end — there is no
`close()` to forget or to call twice, and `drop(file)` is the early form.
The five constructors replace node's flags string ("wx" and friends —
an untyped enum whose failure mode is a runtime `EINVAL`): each says in
its name what it does to a file that already exists, which is the only
thing anyone ever looks up. `create_new` is the exclusive create — how a
process claims a name without racing another — and `append_to` carries
the host's own rule, kept rather than papered over: on an append handle
every write lands at the end, position ignored.

Reads and writes are positional and stateless *by design*. A `File` has
no cursor, so two loans of one handle cannot silently interleave through
hidden position state; `read_at` answers with the byte count — short near
the end of the file, `0` at end of file, neither an error. The handle is
also what makes read-then-act sequences TOCTOU-free: `file.stat()`
answers `Stat` with no `Option`, because the handle addresses the *open
file*, not the path — rename or remove the path and the handle still
reads the file it holds. And `sync()` is the durability primitive nothing
path-addressed can offer: `write_atomic` guarantees you a whole file or
the old one, `sync` is what makes it survive the power going out.

The documented idiom is `with_file(path, |file| …)`: the body receives
the open file as a per-call parameter, the value comes back out, and the
close is *awaited* — which the destructor's close deliberately is not.
`FileHandle.close()` is asynchronous on the host and a destructor cannot
await, so a dropped `File` *initiates* its close without waiting: written
data is safe either way, but only `with_file` can observe a close
failure. Scope-end `Drop` stays underneath as the safety net. Two shapes
to know before reaching for a handle: a closure cannot capture a `File`
(hand it in as a parameter, which is exactly what `with_file` does), and
`List`/`Map`/`Set` cannot hold one — `Option<File>` is the sanctioned
container, so "a pool of open files" is not expressible today. A
module-level `File` is not expressible either — every constructor is
async and a module-level `let` cannot await — so a process-lifetime
handle is a local in `main`, held across whatever `main` awaits.

Reading a build's assets, rewriting one, and putting the result somewhere
new is the shape most of this is for:

```vilan,norun
import std::fs::{ create_dir_all, read_bytes, scan_dir, update, write_bytes_atomic };

fun main() {
	create_dir_all("dist/assets");
	for entry in scan_dir("assets") {
		if entry.is_file {
			write_bytes_atomic(i"dist/assets/{entry.name}", read_bytes(i"assets/{entry.name}"));
		}
	}
	update("dist/manifest.json", |text| i"{text}\n");
}
main();
```

Watching is the last tier, and it is a `resource` for the same reason a
handle is. `Watcher::watch(path)` observes a path and, when it is a
directory, its immediate entries; `watch_all` observes the whole tree
beneath it — `read_dir` and `read_dir_all`'s reach, and the `_all` suffix
means the same thing here. The path does not have to exist yet: a watch on
something absent reports `Created` when it appears. You consume a watch by
*pulling*:

```vilan,norun
import std::fs::{ Change, ChangeKind, Watcher };
import std::print;

fun describe(change: Change): str {
	match change.kind {
		ChangeKind::Created => i"created {change.path}",
		ChangeKind::Modified => i"modified {change.path}",
		ChangeKind::Removed => i"removed {change.path}",
	}
}

fun main() {
	let watcher = Watcher::watch_all("src");
	print(describe(watcher.next()));
	print(describe(watcher.next()));
}
main();
```

`next()` is async, so it reads like a plain call and suspends until
something happens; `change.path` is the watched root joined with the
entry, which means it is the string to hand straight to
`read_file_to_str` or `File::open` with no joining of your own. There is
no callback form, and the reason is the language rather than taste: a
`|Change| void` observer is a *void* channel, so a handler could not await
the read of the file it was just told about, and a closure cannot capture
a `File` to hold one open across events either. A pull returns into a
scope that can already own a handle.

**The mechanism is polling, and the tier says so out loud.** Every 300 ms
— the interval the compiler's own `--watch` has used since it shipped —
the watcher stats what it is watching and compares. That buys an honest
three-way answer on every platform, which the host's own `fs.watch`
cannot give: it coalesces events and sometimes duplicates them, folds
creation, deletion and renaming into one `"rename"` you have to `stat` to
disambiguate, varies by platform and version on recursive watching, and
throws outright on a path that is not there yet. What polling costs
instead: up to one interval of latency; a change that *cancels out* inside
one interval (a file created and deleted between two observations) is
never reported, because nothing is left to compare; a modification that
alters neither the size nor the mtime is invisible, which matters on
filesystems whose mtime is second-granular; and one `stat` per watched
entry per interval, so `watch_all` on a large tree is the expensive call
in this module. Watch the narrowest path that answers your question.
A rename arrives as `Removed` on the old path and `Created` on the new
one — what a rename *is* to an observer comparing two listings. A
directory reports `Created` and `Removed` but never `Modified`: its mtime
moves whenever its contents do, and those entry changes are already
reported one by one. Changes seen in the same interval arrive one pull at
a time, path-sorted, and the order among them carries no timing
information.

A `Watcher` moves rather than copies, a closure cannot capture one, and
`List`/`Map`/`Set` cannot hold one — `File`'s rules exactly. Its
destructor stops the poll after its last use, `drop(watcher)` is the
explicit form,
and there is no `stop()` to forget or to call twice. That destructor is
load-bearing rather than tidy: a pending host timer keeps the process
alive, so **a watcher that is never dropped is a program that never
exits.** Unlike `File`, its teardown is genuinely synchronous — stopping a
poll is a `clearTimeout`, not a promise — so nothing here inherits the
handle's asynchronous-close exception.

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
	assets: List<str>,      // resources `const asset::bundle` carried in
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

`assets` is what makes a built app need nothing but `dist/`: every
non-code resource the leg named with
[`const asset::bundle`](misc.md#stdasset), by its package-relative path, so
`static/icon.svg` is served at `/static/icon.svg` from
`dist/static/icon.svg`. A build written before bundling existed carries no
`assets` field and reads as a build with none, which is what it was.

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
	fun description(own self, text: str): Document        // <meta name="description">
	fun lang(own self, lang: str): Document
	fun mount(own self, id: str): Document                // the other end of mount_root
	fun head(own self, markup: str): Document             // raw, appended inside <head>
	fun body(own self, markup: str): Document             // raw, appended inside <body>
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
doctype, `<html lang>`, charset, viewport, `<title>`, the description meta
*if one was given*, the stylesheet link *if and only if* the build emitted
styles, the mount element, and the bundle's script tag in the form the
build requires (a classic script for a leg that splits, since chunk
resolution reads `document.currentScript`):

```vilan,norun
import std::build::require_build;
import std::document::Document;
import std::http::{ Response, Server };

async fun main() {
	let build = require_build("client");
	let page = Document::of(build)
		.title("Notes")
		.description("A tidy list of everything you meant to do.")
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

`title` and `description` are the two *identity lines* — head matter the
document is the sole author of, so escaping is the only thing that can go
wrong and the document does it. Both shape the generated document only: a
supplied shell's identity lines are the shell's own. Everything in the
`<head>` that names a second party the build cannot see — a file (a
favicon), an address (`og:url`), a palette (`theme-color`) — stays a
`head()` call, where the raw markup says what it is.

`head`/`body` take raw markup and append (a favicon, an `og:` tag, a CSP,
a `<noscript>`), which is what keeps the generated document small enough
to be worth having: everything else is derived. They are repeatable, and
each call lands on its own line of the written page, so a hatch reads
naturally used once per item. They work on a supplied shell too
(`require_shell`, `from_shell`): `head()` markup splices in immediately
before the shell's own `</head>`, `body()`'s immediately before its
`</body>` — and a shell that lacks the closing tag a used hatch needs
stops at `html()` rather than having the markup guessed into it. They are an escape hatch, not an exemption — when `html()` writes the
page, markup you added there is checked like any other, so a `<link>` to
a stylesheet the build did not emit stops the boot exactly as it would in
a hand-written shell. A document with no hatch markup runs no check at
all: a generated one is derived from the build alone, and a supplied one
serves the shell's own bytes, exactly as `require_shell` checked them.

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
fun cwd(): str                   // current working directory, absolute
fun env(key: str): Option<str>   // environment variable
fun exit(code: i32)
fun scan(): str                  // read a line from stdin
```

Every relative path in a `std::fs` or `std::build` call resolves against the
process's working directory; `cwd()` reads it (absolute), so a boot check can
say which directory the server actually ran from instead of guessing. It is
*not* a project-root finder — walking up to `vilan.toml` is a separate,
undecided helper, and `cwd()` does not preempt it.

Server-side hot-swapping of code is not a thing
here and is not planned — the node leg restarts.

A completed `main` ends the process; long-lived programs must hold it open
(a listening server does; a socket-holding client needs an explicit wait).

## std::watch

```vilan,fragment
fun is_watching(): bool     // is this a `vilan run --watch` child?
fun force_refresh(): void   // ask every connected browser to reload once
```

**This module is not a file watcher**, despite its name — that is
[`std::fs`'s `Watcher`](#stdfs), above. `std::watch` is the dev-refresh
channel: whether *this process* is a `vilan run --watch` child, and how to
tell connected browsers to reload. The two never meet; a program can use
both.

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
