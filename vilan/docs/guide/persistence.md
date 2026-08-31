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

### Migrations: carrying a schema forward

`CREATE TABLE IF NOT EXISTS` is a schema **ensure**, and it is right
exactly until the first schema **change**. Add a `description` column to
`task` and the clause does nothing at all — the table exists, so the
statement is a no-op, and the deployed database keeps the old shape while
the code querying it has the new one. Deleting the file is not a
deployment strategy.

`db.migrate(migrations)` is the spelling for "carry this database
forward". You give it named steps in order; it applies the ones this
database has not seen and records them in a `vilan_migrations` table it
owns. Call it at boot, unconditionally, before the first query:

```vilan,norun
import std::db::{ Database, Migration };

fun main() {
	let db = Database::open("app.db");
	let applied = db.migrate([
		Migration {
			name = "001-create-task",
			sql = "CREATE TABLE task (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL)",
		},
		Migration {
			name = "002-task-description",
			sql = "ALTER TABLE task ADD COLUMN description TEXT",
		},
	]);
	print(i"applied {applied.len()} migrations");
}
```

The first boot applies both and prints `applied 2 migrations`. Every boot
after that applies nothing and prints `applied 0`. Append a third step and
only the third one runs.

**A step is recorded if and only if its SQL committed.** Each step runs
in its own transaction *with* its record insert, so there is no window
where the schema moved and the record did not. A process killed
mid-migration leaves a database whose recorded set exactly describes its
schema, and the next boot resumes from there.

Three things stop the boot loudly, all of them before a single statement
is applied — so a database `migrate` refuses is a database it did not
touch:

- **A step fails.** The message names the step and quotes SQLite's own
  diagnosis (`migration '003-add-index' failed and was not applied: no
  such table: taks`). Nothing is recorded for it. Fix the SQL, boot
  again, and it resumes exactly there — the steps before it stay applied.
- **The database is ahead of the code** — it records a step your list
  does not contain. That is a rolled-back deploy: an older binary, whose
  queries predate a schema change, about to run against a database that
  already has it.
- **A step was inserted into the past** — an unapplied step sits before
  one that is already applied. That is two branches merged, and applying
  it out of order would produce a schema no fresh database can reproduce.

Rules for writing a step:

- **The name is forever.** It is the row's key in `vilan_migrations`.
  Renaming an applied step makes the database look like it is ahead of
  the code; editing an applied step's SQL does nothing at all, because
  it will never run again. To change a shipped migration, write a new one.
- **Never manage transactions inside a step.** `migrate` supplies the
  transaction. A step containing its own `COMMIT` commits that one out
  from under it — the schema change lands permanently while its record is
  still unwritten, which is the one way to break the invariant above.
  For the same reason a step cannot use a statement SQLite refuses in a
  transaction (`VACUUM`, `PRAGMA journal_mode`).
- A step's SQL may hold as many statements as you like, separated by `;`.
- Names must be unique, but they need not sort in order — the *list's*
  order is what `migrate` applies. `001-slug` prefixes are a convention
  worth keeping anyway, for the reason below.

Vilan has no down-migrations. The recovery for a bad migration is a new
migration that undoes it, which is what production uses in practice.

#### Carrying the SQL into `dist/`

Inline strings are fine for two steps and unpleasant for twenty. Keep the
real SQL in files and pull them through the **const channel**, so the
deployed app is still `dist/` and nothing else:

```vilan,fragment
// migrations/001-create-task.sql, 002-task-description.sql, ...
db.migrate([
	Migration { name = "001-create-task", sql = const asset::read("migrations/001-create-task.sql") },
	Migration { name = "002-task-description", sql = const asset::read("migrations/002-task-description.sql") },
]);
```

`asset::read` runs at compile time, so each file's *text* is compiled
into the bundle. There is no runtime path to get wrong on the deployment
machine, and every migration edit is a tracked build input — change a
`.sql` file and the compile that read it is invalidated.

The intended idiom, once the const `read_dir` recipe lands, is to write
that loop once: list `migrations/`, sort by name, and turn each entry
into a `Migration` named after its file. That is what makes the
`NNN-slug.sql` convention pay — a name-sorted listing *is* the migration
order.

## Serving http: `std::http`

Every server in vilan is a `Server::builder()` chain — a port, a handler,
and `start()`:

```vilan,norun
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

**The same two tiers, for a served build.** The artifacts `serve_build`
installs get no caching by default — one
`Content-Type` header and the bytes — because which tier a build's files
belong in is a deployment's decision and not one std can make for you. Opt
in with `cache_build`, which asks your policy per artifact, keyed on the
route:

```vilan,norun
import std::build::require_build;
import std::http::{ CachePolicy, Response, Server };

async fun main() {
	Server::builder()
		.port(8080)
		.serve_build(require_build("client"))
		.cache_build(|url| if url.starts_with("/chunk-") {
			// Fingerprinted: the name changes when the bytes do.
			CachePolicy::none().cache_control("public, max-age=31536000, immutable")
		} else {
			// Fixed URL: revalidate, and pay a body only when it changed.
			CachePolicy::validated().cache_control("no-cache")
		})
		.on_request(|request| Response::builder().code(404).body("Not Found").build())
		.build()
		.start();
}
```

That is the same two tiers as above, written once instead of per route:
`validated()` is the `etag_of` + `etag_response` pair applied to the
artifact's own bytes, and `cache_control` is the header, reaching the `304`
arm as well as the `200`. Delete the `cache_build` line and the server is
byte-for-byte back to no caching — there is no third state, and no default
moved under you.

## Files: `std::fs`

```vilan,fragment
fun read_file_to_str(path: str): str        // async (implicitly awaited), UTF-8
fun read_file_encoded(path: str, encoding: str): str   // async — any host encoding
fun read_bytes(path: str): Bytes            // async — the true binary read
fun write_file(path: str, contents: str)    // async
fun read_dir(path: str): List<str>          // async — entry names, flat
fun stat(path: str): Option<Stat>           // async — None if the path isn't there

resource external struct File               // an open file — the handle tier
fun with_file<T>(path: str, body: |File| T): T   // open, run, close (awaited)
fun with_file_create<T>(path: str, body: |File| T): T   // …and one per constructor

resource struct Reader                      // a cursor over an open file
fun Reader::of(own file: File): Reader      // takes the handle; starts at byte 0
fun Reader::next(self, size: i32): Bytes    // the next chunk; empty at end of file

resource external struct Watcher            // a live watch — the watch tier
fun Watcher::watch(path: str): Watcher      // the path, and a directory's own entries
fun Watcher::watch_all(path: str): Watcher  // the whole tree beneath it
fun Watcher::next(self): Change             // async — the next change
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
for a caller asking "is this here yet" — `stat(path).is_some()` is the
existence probe (there is no `exists`; everything in this module is
async, and `stat` answers strictly more).

When one open file needs more than one act — read a header, then seek
into the middle; write, then `sync` for durability — you open a handle.
`File::open(path)` (and `create`, `create_new`, `append_to`, `modify`)
hands back a *resource*: it moves rather than copies, and its destructor
closes the handle after its last use, so there is no `close()` to forget —
`drop(file)` closes early, at the same point the compiler would have. Reads and writes are positional
(`file.read_at(buffer, position)`, `file.write_at(buffer, position)`),
`file.stat()` answers with no `Option` (the handle is already open, and
nothing re-resolves the path between probe and act), and `file.sync()` is
`fsync`, the durability step `write_atomic` alone cannot give you. The
documented idiom is the scoped form:

```vilan,norun
import std::bytes::{ Bytes, decode_utf8 };
import std::fs::with_file;

fun main() {
	let head = with_file("data.bin", |file| {
		let buffer = Bytes::alloc(16);
		file.read_at(buffer, 0);
		decode_utf8(buffer.slice(0, 16))
	});
}
main();
```

`with_file` opens the file, hands it to your closure as a per-call
parameter, and closes it before returning — with the close *awaited*, so
a failure to close is a failure of `with_file`; a `File` you hold
yourself closes through its destructor instead, which starts the close
without waiting on it. There is one scoped form per constructor
(`with_file_create`, `with_file_create_new`, `with_file_append`,
`with_file_modify`), and on a *writing* handle the awaited close is the
one that earns its keep: the OS is entitled to report a write's failure
at close time — a full disk, a quota — and only the scoped form makes
that a failure of your call rather than a line on stderr.

To read a big file without holding it all in memory, wrap the handle in a
`Reader` and pull chunks. `next(size)` advances a cursor this program
owns — the handle itself stays positional, so nothing hidden moves — and
answers an **empty** chunk at end of file:

```vilan,norun
import std::fs::{ File, Reader };

fun main() {
	let reader = Reader::of(File::open("big.bin"));
	mut total = 0;
	for {
		let chunk = reader.next(65536);
		if chunk.len() == 0 {
			jump break;
		}
		total += chunk.len();
	}
	print(total);
}
main();
```

Stop on empty, not on short: a chunk shorter than you asked for is
ordinary near the end, and only the empty one means the file is done. A
`Reader` owns its `File`, so it is a resource too — it moves, and dropping
it closes the file. Full signatures:
[the process reference](../std/process.md#stdfs).

When you want to know that a file changed rather than to read it once,
you open a *watch*. `Watcher::watch(path)` observes a path (and a
directory's immediate entries); `Watcher::watch_all(path)` observes the
whole tree under it. You pull changes out one at a time:

```vilan,norun
import std::fs::{ Change, ChangeKind, Watcher };

fun main() {
	let watcher = Watcher::watch_all("content");
	let change = watcher.next();
	match change.kind {
		ChangeKind::Created => print(i"new: {change.path}"),
		ChangeKind::Modified => print(i"changed: {change.path}"),
		ChangeKind::Removed => print(i"gone: {change.path}"),
	}
}
main();
```

`next()` is async like everything else here, so it reads as a plain call
and suspends until something happens, and `change.path` is ready to hand
to `read_file_to_str`. There is no callback form: a `|Change| void`
handler could not await the read of the file it was told about, and could
not hold a `File` open across events either — a pull returns into a scope
that can. Under the hood it *polls*, comparing stats every 300 ms, which
is what lets it tell creation from modification from removal on every
platform (the host's own `fs.watch` cannot); the costs are up to an
interval of latency, blindness to a change that cancels itself out inside
one interval, and a `stat` per watched entry per interval — so watch the
narrowest path that answers your question. A `Watcher` is a `resource`
like `File`, its destructor stops the poll, and that matters more than it
does for a handle: a live poll holds the event loop open, so **a watcher
that is never dropped is a program that never exits.**

`std::watch` is a different thing wearing a similar name — the dev-refresh
channel (`is_watching()`, `force_refresh()`), not a file watcher.

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
