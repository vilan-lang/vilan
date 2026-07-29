# Cells reference

The two sharing tools: `std::shared::Shared` (one shared mutable cell) and
`std::arena::Arena` (stable identities for graphs). When to reach for
which: [the memory model](../tour/memory-model.md).

## `Shared<T>`

A heap cell two places can hold at once: the escape hatch from
value-semantics copying.

```vilan,fragment
impl Shared<type T> {
	fun new(value: T): Shared<T>
	fun read(self): T                      // a COPY of the contents
	fun write(self): &mut T                // a writable view of the contents
}
```

```vilan
import std::print;
import std::shared::Shared;

fun main() {
	let log: Shared<List<str>> = Shared::new([]);
	let record = |entry: str| {
		log.write().push(entry);
	};
	record("first");
	record("second");
	print(log.read().len());
}
```

- `read()` copies: mutating the result is lost
  (`shared.read().push(x)`, the classic trap). Mutate through `write()`.
- `write()` returns a view. Use it within the same statement
  (`cell.write() = v`, `cell.write().push(item)`); it obeys the usual view
  rules (no storing, no holding across `await`).
- Copying the `Shared` value itself copies the *handle*: both handles see
  one cell. That's the point.

## `Arena<T>` + `Handle<T>`

A **generational arena**: insert values, get back small copyable
`Handle<T>` keys. Handles are plain values, storable in struct fields and
lists (which views are not), so nodes can reference each other:

```vilan,fragment
struct Handle<T> { … }   // slot index + generation; copy freely

impl Arena<type T> {
	fun new(): Arena<T>
	fun insert(&mut self, value: T): Handle<T>
	fun get(&self, handle: Handle<T>): Option<&T> borrows self  // a view; None once removed
	fun set(&mut self, handle: Handle<T>, value: T): bool
	fun remove(&mut self, handle: Handle<T>): Option<T>
	fun contains(self, handle: Handle<T>): bool
	fun len(self): i32
	fun is_empty(self): bool
}
```

```vilan
import std::print;
import std::arena::{ Arena, Handle };
import std::option::Option::{ self, Some, None };

struct Node {
	label: str,
	edges: List<Handle<Node>>,
}

fun main() {
	mut nodes: Arena<Node> = Arena::new();
	let a = nodes.insert(Node { label = "a", edges = [] });
	let b = nodes.insert(Node { label = "b", edges = [a] });
	// Close the cycle: a → b. `get` hands back a view, so copy it (`*node`),
	// edit the copy, and write it back with `set`.
	match nodes.get(a) {
		Some(let node) => {
			mut updated = *node;
			updated.edges.push(b);
			nodes.set(a, updated);
		},
		None => {},
	}
	print(nodes.len());
}
```

- **Generational** means deletion-safe: removing a value and reusing its
  slot bumps a generation counter, so a stale handle `get`s `None` instead
  of aliasing the new occupant.
- `get` returns a **view** (`Option<&T>`), second-class like any other: read
  through it, but it may not outlive an arena mutation or be stored. To change
  a value, copy it out (`*view`), edit, and `set` it back, or design nodes so
  edges/fields update independently.
- Traversal is re-`get` per step, so the arena stays mutable while you walk.

### Handles cross the wire

A handle is two integers, so `Handle<T>` is `Wire`: it can sit in an rpc
payload, and a server-side arena becomes the **naming layer** for clients:
the stable entity reference they quote back ("update node X"). The `T` is
phantom; only `{ index, generation }` travels, so a handle names entities
whose type is not itself Wire.

```vilan
import std::print;
import std::arena::{ Arena, Handle };
import std::json::{ encode_json, decode_json };
import std::result::Result::{ self, Ok, Err };
import std::option::Option::{ self, Some, None };

[derive(Wire)]
struct Rename { node: Handle<str>, title: str }

fun main() {
	mut titles: Arena<str> = Arena::new();
	let node = titles.insert("old");
	// The client received `node` earlier and now quotes it back.
	let request: Result<Rename, str> = decode_json(encode_json(Rename { node = node, title = "new" }));
	match request {
		Ok(let rename) => {
			titles.set(rename.node, rename.title);
			print(titles.get(rename.node).unwrap_or("gone"));
		},
		Err(let reason) => print(reason),
	}
}
```

The generational rule becomes the distributed staleness story for free: a
client acting on an entity another client deleted gets the same clean `None`
(and `set` returns `false`) as local code holding a stale handle. No phantom
write, one rule from a local list to an rpc boundary.

Scope the arena to the session. A handle is a name, and `(index, generation)`
is guessable, so an arena shared across tenants hands every client names that
mean something to the others. A **per-session arena** (created when the
session is established, dropped with it) makes a handle from one session
name nothing in another, by construction. Authorize the session; then look
the handle up in that session's arena.

When one arena *is* shared and its handles must not be interchangeable,
`Arena::branded()` adds the belt to that suspenders: its generation counters
start at a random value instead of `0`, so a handle issued by one branded arena
resolves to `None` in every other one rather than naming that arena's slot of
the same index.

```vilan
import std::print;
import std::arena::{ Arena, Handle };
import std::option::Option::{ self, Some, None };

fun main() {
	mut mine: Arena<i32> = Arena::branded();
	mut theirs: Arena<i32> = Arena::branded();
	let handle = mine.insert(7);
	theirs.insert(9);
	print(theirs.get(handle).unwrap_or(-1));   // -1 — a foreign name
	print(mine.get(handle).unwrap_or(-1));     // 7
}
```

Everything else is unchanged: branding only moves where the counters start, so
removal, staleness and slot reuse behave exactly as above, and a brand mismatch
is the same clean `None` (and `false` from `set`) as a stale handle, never a
panic.

A brand is a **confusion guard, not an authorization check**. It travels inside
the handles it issues, so a client holding one valid handle can derive it. It
stops one tenant's names from meaning something to another and stops blind
guessing; it does not make a handle unforgeable. Authorize the session
first, then look the handle up in that session's arena.
