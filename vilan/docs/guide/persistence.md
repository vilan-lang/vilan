# Persistence and the server

This chapter covers the server half of a full-stack app: SQLite via
`std::db`, http serving via `std::http`, files via `std::fs`, and the
process itself via `std::process`. These modules live in the process
layer, so they're available in Node/Deno/Bun builds. The rpc layer that
sits on top is [Services & RPC](services.md).

## SQLite: `std::db`

Vilan ships with an embedded SQLite binding (Node's built-in SQLite
underneath). There is no ORM and no query builder. You write SQL, with
`?` placeholders for values:

```vilan,norun
import std::print;
import std::db::{ Database, Statement, Row };
import std::option::Option::{ self, Some, None };

fun main() {
	let db = Database::open("app.db");
	db.exec("""
	CREATE TABLE IF NOT EXISTS task (
		id INTEGER PRIMARY KEY AUTOINCREMENT,
		name TEXT NOT NULL,
		created_at INTEGER NOT NULL
	)
	""");

	let id = db.prepare("INSERT INTO task (name, created_at) VALUES (?, ?)")
		.run(["write docs", 1720656000000i53]);
	print(id);

	match db.prepare("SELECT * FROM task WHERE id = ?").first([id]) {
		Some(let row) => print(row.text("name")),
		None => print("missing"),
	}

	for row in db.prepare("SELECT * FROM task").all([]) {
		let row_id = row.integer("id");
		let name = row.text("name");
		print(i"{row_id}: {name}");
	}
}
```

The whole surface fits in a few lines:

- `Database::open(path)` opens or creates the file. `":memory:"` gives
  you a throwaway in-memory database, handy in tests.
- `db.exec(sql)` runs DDL and one-off statements.
- `db.prepare(sql)` gives a `Statement`. Then `.run(params)` executes
  and returns the last insert id, `.first(params)` fetches an
  `Option<Row>`, and `.all(params)` fetches a `List<Row>`.
- Rows read by column name: `text`, `integer` (i32), `big_integer`
  (i53; use it for epoch-millis timestamps, which outgrow i32),
  `real` (f64), and `is_null`.

Two habits to keep:

- Values always go through `?` placeholders. Never interpolate them into
  the SQL string.
- Don't name a column with an SQL keyword. `desc` is the one that bites
  in practice: spell it `description`.

The API is synchronous, which fits rpc handlers (the dispatch path is
synchronous too), and there is no connection pool to manage.

## Serving http: `std::http`

Every server in vilan is a `Server::builder()` chain — a port, a handler,
and `start()`:

```vilan,norun
import std::print;
import std::http::{ Server, Request, Response };

fun main() {
	Server::builder()
		.port(8080)
		.on_request(|request| {
			match request.path() {
				"/health" => Response::builder().body("ok").build(),
				_ => Response::builder()
					.code(404)
					.set_header("Content-Type", "text/plain")
					.body("not found")
					.build(),
			}
		})
		.on_start(|server| print(i"listening at {server.url()}"))
		.build()
		.start();
}
```

`Request` gives you `path()`, `method()`, and `body()` (`bytes()` for a
binary POST). Responses come from a builder: `.code(i32)` (200 by
default), `.set_header(name, value)`, `.body(str)`, `.build()`.

A full-stack server adds two more links to that chain, and neither of them
names a file. **`serve_build`** installs one route per artifact the client
leg's build actually wrote — the bundle, the stylesheet if the leg emitted
one, every route chunk — and **`with_service`** installs an rpc service's
routes and its WebSocket handshake. Both answer *before* `on_request`,
whatever order you wrote the chain in, so your own handler still gets
every path they do not claim and deep links keep working (see
[Routing](routing.md)):

```vilan,norun
import std::build::require_build;
import std::document::require_shell;
import std::http::{ Response, Server };

async fun main() {
	let build = require_build("client");
	let page = require_shell("src/app.html", build).html();

	Server::builder()
		.port(8080)
		.serve_build(build)         // /client.js, /client.css, every chunk
		.on_request(|request| Response::builder().set_header("Content-Type", "text/html").body(page).build())
		.build()
		.start();
}
```

Renaming the leg, adding a stylesheet, or turning `split = true` on needs
no edit here: the build says what it emitted and the server believes it.
An rpc app adds `.with_service(Service::new(protocol))` to the same chain
— [Services & RPC](services.md#the-server-side) has it whole.

### Caching: ETag and 304

Anything you serve at a **fixed URL** re-downloads on every page load
until you give the browser a validator. The policy every static layer
converges on has two tiers, and the *name* decides which one a response
gets:

- A **fingerprinted name** — one that carries a content hash, so a new
  build writes a new URL — is free to be cached for a year and never asked
  about again: `Cache-Control: public, max-age=31536000, immutable`. No
  validator needed; the name already is one.
- A **fixed URL** — `/`, a favicon, a page the browser asks for by that
  exact path — can change under the cache, so it gets a short life or
  `no-cache`, plus an `ETag`, and a revalidation answers `304 Not
  Modified` instead of re-sending the bytes.

`std::http` ships the validator tier whole. `etag_of(bytes)` mints a
strong, quoted tag from the bytes' sha-256 — compute it once, where the
bytes settle. `etag_response(request, tag, bytes, content_type)` answers
the request: `304` with the tag echoed and no body when the request's
`If-None-Match` already names it, the full `200` otherwise. It returns the
builder still open, so the Cache-Control tier chains after it and reaches
both arms:

```vilan,norun
import std::bytes::encode_utf8;
import std::http::{ Server, etag_of, etag_response };

async fun main() {
	let page = encode_utf8("<!doctype html><h1>hello</h1>");
	let tag = etag_of(page);   // once, at boot — the bytes are settled

	Server::builder()
		.port(8080)
		.on_request(|request| {
			// A fixed URL revalidates: no-cache + ETag means every load
			// asks, and an unchanged page answers 304 with no body.
			etag_response(request, tag, page, "text/html; charset=utf-8")
				.set_header("Cache-Control", "no-cache")
				.build()
		})
		.build()
		.start();
}
```

A page you render per request works the same way — hash the rendered
bytes with `etag_of` before answering, and an unchanged render still
saves the transfer, just not the render. The matching semantics (the
list and `*` forms, weak comparison, the GET/HEAD gate) are in the
[reference](../std/process.md#stdhttp-the-server).

## Files: `std::fs`

```vilan,fragment
fun exists(path: str): bool                 // sync — the module's one blocking call
fun read_file_to_str(path: str): str        // async (implicitly awaited), UTF-8
fun read_file_encoded(path: str, encoding: str): str   // async — any host encoding
fun read_bytes(path: str): Bytes            // async — the true binary read
fun write_file(path: str, contents: str)    // async
fun read_dir(path: str): List<str>          // async — entry names, flat
fun stat(path: str): Option<Stat>           // async — None if the path isn't there
```

`read_bytes` reads a file with no decode in between — the host's buffer
binds straight to `Bytes` — which is what anything that is not text needs.
An image, a font or a favicon that your *build* emits no longer needs it:
`serve_build` reads and serves those as bytes itself (below). `read_bytes`
is for the binary file the build did not write. `read_dir` lists a
directory's immediate entries by name — flat and unordered, so call `stat`
per entry when you need file-vs-directory. `stat` reads `size`,
`modified_at_ms` (epoch millis) and `is_directory`, and is the one read
here that answers `None` instead of throwing on a missing path: it exists
for a caller asking "is this here yet". Full signatures:
[the process reference](../std/process.md#stdfs).

What a server does *not* read by hand any more is its own build.
`serve_build` knows the bundle's name, the stylesheet's, and every chunk's,
so the boot reads and the content-type table that used to live here are
gone — including for binary artifacts, which it reads and serves byte for
byte under a content type generated from the `mime-db` registry data.

## The process: `std::process`

```vilan,fragment
fun args(): List<str>          // CLI arguments (vilan run app.vl -- …)
fun env(key: str): Option<str> // an environment variable
fun exit(code: i32)            // end the process
fun scan(): str                // a line from stdin
```

One behavior to plan around: **the process exits when `main` finishes.**
A server stays alive because `start()` holds the event loop open. A
long-lived *client* process (a probe holding a socket, say) has to keep
`main` open itself: await something that ends with the app, or
`sleep_for` a long duration.

## Putting it together

The boot sequence of a full-stack server, in order:

1. `Database::open`, then `exec` the schema
   (`CREATE TABLE IF NOT EXISTS …`).
2. `require_build("client")` — ask the client leg's build what it emitted.
3. `require_shell("src/app.html", build)` — hold your page against that
   build ([SSR](ssr.md), [Styling](styling.md#getting-the-stylesheet-onto-the-page)).
   `Document::of(build)` writes the page from the build instead, if you
   would rather not keep a shell at all.
4. Load the mirrored state from SQLite into the service's signals.
5. Wire the service's handlers to statements. Write SQL first, then
   update the signal (the mirror broadcasts the signal).
6. `Server::builder()` with `serve_build(build)`, a `with_service(…)` if
   the app speaks rpc, your `on_request` fallback, and `start()`.

Steps 2 and 3 both **refuse rather than degrade**, and that is why they
belong at boot. A leg that was never built and a page whose stylesheet
`<link>` went missing are not conditions to discover one request at a
time: the first would 404 every asset for the life of the process, the
second renders unstyled and entirely correct-looking. Each stops the
server instead, naming the file and the leg.

The ordering inside step 5 matters for its own reason. Persist first, then
update the signal. That way a crash between the two can never broadcast
state that was never stored.
