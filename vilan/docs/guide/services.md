# Services & RPC

This is the chapter where Vilan's full-stack story comes together. The
short version: you write one ordinary struct on the server, mark a few
things on it, and you get a typed client, live data sync, and reconnect
handling without writing any protocol code.

A **service** is that struct. Three attributes do the work:

- `[service(ClientName)]` on the struct names the generated client type.
- `[rpc]` on a method makes it callable from the client.
- `[expose]` on a `Signal<T>` field **mirrors** it: every connected
  client gets a live copy that updates when the server writes it.

There are no REST endpoints, fetch calls, or JSON shapes to keep in
sync by hand. The compiler knows both sides.

Here's a complete little server:

```vilan,norun
import std::reactive::Signal;
import std::json::json_codec;
import std::http::{ Response, Server };
import std::rpc_server::Service;
import std::shared::Shared;

[derive(Wire, PartialEq, Debug)]
struct Note {
	id: i32,
	text: str,
}

[service(NotesClient)]
struct Notes {
	[expose] entries: Signal<List<Note>>,
	next_id: Shared<i32>,
}

impl Notes {
	[rpc]
	fun add(self, text: str): i32 {
		let id = self.next_id.read();
		self.next_id.write() = id + 1;
		self.entries.set_with(|list| {
			mut updated = list;
			updated.push(Note { id = id, text = text });
			updated
		});
		id
	}
}

fun main() {
	let notes = Notes {
		entries = Signal::new([]),
		next_id = Shared::new(1),
	};
	Server::builder()
		.port(4000)
		.with_service(Service::new(notes.dispatcher().into_protocol(json_codec())))
		.on_request(|request| Response::builder().body("app shell here").build())
		.on_start(|server| print(i"listening on {server.url()}"))
		.build()
		.start();
}
```

And a client. `NotesClient::connect` gives you an object whose exposed
fields are typed **mirrors** (`RemoteSource<T>`, one per `[expose]`) and
whose rpc methods are ordinary calls that return `Result`:

```vilan,browser
import std::reactive::Signal;
import std::json::json_codec;
import std::result::Result::{ self, Ok, Err };
import std::shared::Shared;

[derive(Wire, PartialEq, Debug)]
struct Note {
	id: i32,
	text: str,
}

[service(NotesClient)]
struct Notes {
	[expose] entries: Signal<List<Note>>,
	next_id: Shared<i32>,
}

impl Notes {
	[rpc]
	fun add(self, text: str): i32 {
		let id = self.next_id.read();
		self.next_id.write() = id + 1;
		id
	}
}

async fun main() {
	match NotesClient::connect("/", json_codec()) {
		Ok(let client) => {
			// The mirror, watched by hand: the first `sub` opens the channel,
			// and the observer fires on every server-side change, on every
			// client. Disposing the last watcher closes the channel again.
			let watching = client.entries.sub(|list: List<Note>| print(list.len()));
			// An rpc call: implicitly awaited, Result-typed.
			match client.add("hello") {
				Ok(let id) => print(id),
				Err(let error) => print(i"rpc failed: {error.debug()}"),
			}
			watching.dispose();
		},
		Err(let error) => print(i"connect failed: {error.debug()}"),
	}
}
```

In a real app the service lives in its own module, next to the
resources its bodies use, and the client entry imports the generated
`NotesClient` from it. See
[Where the service lives](#where-the-service-lives) below for why the
browser build may do that, and the [walkthrough](walkthrough.md) for
the full shape.

## What can cross the wire: `Wire`

Everything that travels (rpc parameters, return types, mirrored
payloads) must be serializable, which Vilan calls **Wire**. The scalars
are Wire (`bool`, the integers including `i53`, floats, `str`). `List`
and `Option` of Wire types are Wire. And your own types opt in with a
derive:

```vilan,fragment
[derive(Wire, PartialEq, Debug)]
struct Note { id: i32, text: str }
```

That triple is the standard shape for payload types: `Wire` to travel,
`PartialEq` because mirrors and UI reconciliation compare values, and
`Debug` for error paths.

`derive(Wire)` checks every field recursively. A closure or a `Signal`
hiding inside a payload type is a compile error at the derive, which is
exactly where you want to find out.

A `resource` never travels, in either position: not as a field, and not as
the derived type itself — `derive(Wire)` and `derive(Json)` are both refused
for a `resource` struct or enum. A resource is an owned handle, and the
receiving side would rebuild one out of bytes: a second handle to the same
thing, owned by nobody. Send a name for it instead, which is what the next
section is about.

The codec is chosen at connect time: `json_codec()` for a readable wire,
`binary_codec()` for a compact one. Client and server must use the same
one.

## Naming server entities: `Handle<T>`

Payloads carry *data*. When a client needs to talk about a thing the
server owns (the node to update, the draft to commit, the route's
entity), it needs a name for it, and the name has to survive the round
trip. That name is `Handle<T>` from `std::arena`, which is `Wire`:

```vilan,fragment
[derive(Wire)]
struct Rename { node: Handle<Doc>, title: str }

[service]
struct Docs {
	docs: Arena<Doc>,
}
```

The server hands out handles; the client stores them and quotes them
back. Only `{ index, generation }` travels (the `T` is phantom), so the
entity itself never has to be Wire, and never has to leave the server.

The payoff is what happens when the entity is gone. The arena's
generation check answers a stale handle with `None`, so a client acting
on something another client just deleted gets a clean "not there"
instead of a phantom write into a reused slot. It is the same rule local
code already lives by, extended to the wire, and it is why a handle
beats an integer id you check by hand.

**Scope the arena to the session.** `(index, generation)` is guessable, so
an arena shared across tenants hands every client names that mean
something to the others. Create the arena when the session is
established and drop it with the session: a handle from one session then
names nothing in another, by construction. Authorize the session first;
then look the handle up in that session's arena.

When the arena has to be shared, `Arena::branded()` numbers its
generations from a random base instead of `0`, so its handles resolve to
`None` in any other arena rather than naming the slot of the same index.
Nothing else changes: staleness, reuse and the `None` answer are as
before. Treat it as a confusion guard, not an authorization check: the
brand rides inside the handles it issues, so a client with one valid
handle can derive it. It keeps tenants' names from colliding; the session
check is still what decides who may act.

## What rpc calls do

- On the client they return `Result<T, RpcError>` and are implicitly
  awaited, like any async call.
- `RpcError` tells you what went wrong, in five variants:
  `Transport(str)` (couldn't reach the server), `Decode(str)`,
  `Remote(str)` (the handler failed), `Contract(str)` (the connect-time
  check below refused a drifted server), and `Unauthorized`. Errors are
  values. Look at them and decide.
- At connect time, both sides compare a hash of the service's shape. If
  a stale client meets a redeployed server, the connect fails cleanly —
  as `Contract(reason)` — instead of calls corrupting halfway. This is
  the **contract check**.
- On the server, each handler runs inside a turn, so all the signal
  writes one rpc makes are broadcast as a single consistent update.
- Handler bodies can await: call another service, `sleep_for`, wait on
  I/O. The reply is sent when the body finishes, and the turn holds
  across the awaits: writes before and after a suspension still
  coalesce into that same single update.

## Mirrors

The `[expose]` mirror is the piece that replaces most "fetch on mount,
refetch on focus, invalidate on mutation" client code. The server writes
its signal whenever and however it likes. Every connected client's copy
updates. That's it.

Three patterns follow from it:

- **Derive views locally.** Expose one `tasks` list and let each page
  `map` it down (filter by workspace, sort by date). Don't add an rpc
  per view.
- **Mutate via rpc, observe via mirror.** Your create/delete handlers
  write the server signal. The confirmation the user sees is their own
  change arriving back through the mirror.
- **Edit through drafts.** Bind text inputs to
  [drafts](reactive.md#optimistic-writes-and-local-first-drafts) whose
  commit is the rpc, and `adopt` mirror updates into them. Typing stays
  instant, remote edits fold in, and your own echoes are no-ops.

### Reading a mirror

A mirror is a `RemoteSource<T>`, not a `Signal<T>`, for one honest
reason: before the first update lands it has **no value**, and nothing
about the type pretends otherwise. You read it one of four ways:

- `mirror.or(initial): Signal<T>` — the common one, for a view. A plain
  signal you hand to `bind_each`, `bind_text`, or a `{…}` hole: `initial`
  until the first sync, the mirrored value after. Write it inside the
  view (not in `main`), because it is a **subscription**: it opens the
  channel, and it is released when the view that created it is unmounted.
- `mirror.map(|value| …): Signal<U>` — the same, with the `Option<T>`
  in your hands once, which is where a fallback of a *different* type
  belongs (`"loading…"` from a `RemoteSource<i32>`). `or` is `map` for
  the same-type case.
- `mirror.sub(|value| …): Subscription` — the manual form: an observer
  of present values, and a handle you dispose yourself. For code with no
  view and no owner (a probe, a script).
- `mirror.get(): Option<T>` and `mirror.status(): Signal<Status>`
  (`Waiting` / `Ready`) — passive reads. They open nothing.

```vilan,browser
import std::json::json_codec;
import std::reactive::Signal;
import std::result::Result::{ self, Ok, Err };
import std::rpc::SocketTransport;
import std::shared::Shared;
import std::ui::{ View, mount_root, view };

[derive(Wire, PartialEq, Debug)]
struct Note {
	id: i32,
	text: str,
}

[service(NotesClient)]
struct Notes {
	[expose] entries: Signal<List<Note>>,
	next_id: Shared<i32>,
}

fun notes_panel(client: NotesClient<SocketTransport>): View {
	// Counted, and released when this view is unmounted: the channel is
	// open while — and only while — the panel is showing. `[]` until the
	// first sync; the empty list takes its element type from the mirror.
	let entries = client.entries.or([]);
	view("ul").bind_each(entries, |note| note.id, |note| view("li").text(note.text))
}

async fun main() {
	match NotesClient::connect("/", json_codec()) {
		Ok(let client) => {
			let _root = mount_root("app", || notes_panel(client));
		},
		Err(let error) => print(i"connect failed: {error.debug()}"),
	}
}
```

**Subscription follows demand.** Every `or`, `map`, and `sub` takes a
counted lease on the channel: the first one sends `Subscribe`, the last
release sends `Unsubscribe` (deferred to the end of the turn, so a view
that re-renders in place churns nothing). Ten bindings on one mirror
cost one channel; unmounting the page closes it. Which is also why
`or`/`map` must be called where an owner is ambient (inside a view, or
under `run_with_owner`): a network subscription with nobody to release
it is a compile error, not a slow leak.

One sentence to keep in mind: **`status` reports; it does not ask.** A
`status()` observer alone never sees `Waiting → Ready`, because nothing
opened the channel — the mirror stays `Waiting` until something that
renders the value (`or`, `map`, `sub`) subscribes. That is the passive
read being honest, and the count is what makes the active ones cheap.

## Connection state and reconnection

Connections drop. The transport handles it: it reconnects with backoff,
re-verifies the contract, and re-attaches every mirror, so state resyncs
on its own. Your code sees two things.

First, a signal you can bind a banner to:

```vilan,fragment
let state = client.transport.connection_state();
view("p").text("reconnecting…")
	.show(state.map(|current| current == ConnectionState::Reconnecting))
```

Second, explicit call failures. A call in flight when the connection drops
rejects with "connection lost". A call made while down fails immediately
with "not connected". Nothing is silently retried, because an rpc might
not be safe to repeat. Retrying is the app's decision: a draft's next
push, or the user pressing the button again.

Third — when you ask for it — a hook that runs once the connection is back
*and* the mirrors have resynced. That is the moment to re-send whatever the
outage swallowed, and for an edited draft it is one line:

```vilan,fragment
let title = draft(page.title, |value: str| { … client.rename(value) … });
client.transport.on_reconnect(|| title.repush());
```

Without it the user's text survives in the input but never reaches the
server until they type again. With it, the reconnect carries it. `repush`
sends only when the remote is actually behind, and its delivery is
at-least-once — see
[local-first drafts](reactive.md#surviving-a-dropped-connection).

> **Going deeper.** The backoff dials at 250 ms doubling to a 4 s cap,
> ten attempts before giving up (`Closed`). Mirrors rebind by
> re-running the contract check and re-attaching each subscription; you
> never re-subscribe manually — and if either step is refused, the
> connection goes `Closed` too, rather than reporting itself live over
> mirrors that can no longer update. Reaching `Closed` — by any of those
> routes, the spent budget included — also tears the client's own wiring
> down, since those mirrors are unreachable from that socket forever; a
> redial that is merely slow does not. The full state machine is in the
> [rpc reference](../std/rpc.md).

## Authentication

The straightforward shape, and the one the walkthrough app uses: a
`login` rpc returns a token, later rpcs take the token as their first
parameter, and the server validates it per call.

```vilan,fragment
[rpc]
fun login(self, username: str, password: str): AuthOutcome { … }

[rpc]
fun create_task(self, token: str, workspace_id: i32, name: str): i32 { … }
```

When token-per-call gets noisy, the recorded refinement is
connection-scoped identity via `std::context`. It isn't built into the
generated dispatch yet.

## Where the service lives

The service lives **next to the resources its methods use**: a
database handle, the filesystem, other services. In a single-package
app (one `[package]` with an `[entry.client]` and an `[entry.server]`;
see [Platforms](../tour/platforms.md)), that's a module both
entries can see:

```vilan,fragment
// src/store.vl — bodies use server std directly
[service(TodoClient)]
struct TodoStore { … }

// src/client.vl
import pkg::store::TodoClient;
```

In a multi-package workspace the same idea reads: the service sits in
the server package, and the client package depends on it, importing
only the generated client (`import server::store::TodoClient;`).

Either way, the browser build takes only the stub and the contract hash
from that module; the method bodies and the dispatcher are
server-colored and out of its reach. A shared `common` library is still
a fine home for the payload types both sides speak; it's no
longer the only legal home for anything.

## The server side

```vilan,fragment
Server::builder()
	.port(port)
	.with_service(Service::new(service.dispatcher().into_protocol(json_codec())))
	.serve_build(require_build("client"))
	.on_request(|request| …)   // every path neither claims: the app shell
	.on_start(|server| …)      // `server.port()` is the bound port
	.build()
	.start();
```

`dispatcher()` is generated by `[service]`, `into_protocol` pairs it with
a codec, and `Service::new` mounts the result at `/` — `with_service`
installs its routes and its WebSocket handshake on the builder.
`serve_build` serves the client leg's own artifacts from the build's
description rather than from paths you typed, and `on_request` answers
every plain http request neither of them claims — return the app shell
there, so deep links work (see [Routing](routing.md); the shell, read
and checked against the build, is in
[Persistence](persistence.md#serving-http-stdhttp)). For custom
per-connection state, `Service::on_connect`/`on_disconnect` replace the
default session lifecycle (see the [rpc reference](../std/rpc.md)).

## Growing past one service

That chain is the whole layer — `Service::new(protocol)`, installed with
`ServerBuilder::with_service`. A
service's routes answer before `on_request`, which is what lets a page
and a service sit on the same builder chain instead of one replacing the
other:

```vilan,norun
import std::shared::Shared;
import std::json::json_codec;
import std::http::{ Response, Server };
import std::rpc_server::Service;

[service(TodosClient)]
struct Todos {
	count: Shared<i32>,
}

impl Todos {
	[rpc]
	fun add(self, by: i32): i32 {
		self.count.write() = self.count.read() + by;
		self.count.read()
	}
}

fun main() {
	let todos = Todos { count = Shared::new(0) };
	Server::builder()
		.port(4600)
		.with_service(Service::new(todos.dispatcher().into_protocol(json_codec())))
		.on_request(|request| Response::builder().body("app shell here").build())
		.on_start(|server| print(i"listening on {server.url()}"))
		.build()
		.start();
}
```

Delete the `.with_service(…)` line and the program still compiles and
still serves the page — the property a boot function that owns the
whole port can't have. `with_service` is repeatable: a second
service goes on its own mount, `.at("/admin/")`, so
`Client::connect("/admin/", codec)` reaches it and the first service's
routes are untouched. Two constants either way: services always answer
before `on_request` (so an app route can't accidentally shadow a
service route), and the connection lifecycle is the service's own knob —
`Service::on_connect`/`on_disconnect` swap the default session registry
for the app's per-connection state (an auth identity, an app-written
attach) without changing anything else about the chain.

## Traps

- Mysterious contract-mismatch failures while developing usually mean an
  *old server process* is still holding the port. Check with
  `ss -tlnp | grep <port>` and kill it by pid.
- The wire is value-semantic. A mirrored list is a fresh copy per
  update. Mutate through rpcs, never by writing the signal `or`/`map`
  handed you — that writes the local derivative only, and the next sync
  overwrites it.
- An rpc handler's reply is its return value, so the handler runs to
  completion before the client hears back. Long work belongs in spawned
  tasks that write signals when done.
