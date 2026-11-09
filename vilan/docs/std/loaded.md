# std::reactive::lens::loaded reference

Data that arrives from a server, over a [lens](lens.md): a list or a set of
records that loads, refreshes, pages and fails without blanking its rows, and
an **overlay** of optimistic changes beside it. The phases are std's own
[transient states](transient.md).

```vilan,fragment
import std::reactive::lens::loaded::{ Loaded, LoadedMap, Overlay, MapOverlay, Edge, Page, Row };
```

The module is reached by name, like the lens itself: nothing in std imports it,
so a program that does not name it does not pay for it.

## At a glance

| Item | Kind | One line |
|---|---|---|
| `Loaded::new(key)` | constructor | a list loaded from a server, its rows told apart by `key` |
| `.items()` | method | the lens of the SERVER's rows: slots, parts, `rows()`, `items()` |
| `.phase()` | method | `TransientState<i32, E>`: where the whole list stands, with the token of the load whose rows are held |
| `.load()` / `.settle(token, reply)` | methods | start a load; land its reply, or drop it when a newer one superseded it |
| `.request_older()`, `.prepend_page(..)`, `.older()` | methods | a page at the top, and where that end stands (`Edge`) |
| `.request_newer()`, `.append_page(..)`, `.newer()` | methods | the same at the bottom |
| `.receive(item)`, `.receive_removed(key)` | methods | the server's word on one row (a push) |
| `.disconnected(error)`, `.resync(list)` | methods | the connection dropped; it is back with the whole list |
| `.fetch(..)`, `.fetch_older(..)`, `.fetch_newer(..)`, `.follow(source)` | methods | the same with real tasks, or following a std transient |
| `Overlay::beside(lens, key)` | constructor | optimistic changes beside any sequence lens (a `Loaded` has one: `.overlay()`) |
| `.propose(slot, value)`, `.propose_delete(slot)`, `.propose_insert(at, value)` | methods | show a change now, pending the server |
| `.confirm(id)`, `.confirm_with(id, item)`, `.reject(id)` | methods | the server's answer |
| `.view(slot)`, `.view_key(key)`, `.rows()`, `.shown()` | methods | read THROUGH the overlay |
| `.is_pending(slot)`, `.pending()` | methods | a row's pending flag; the count |
| `LoadedMap::new()` | constructor | records loaded one key at a time, each with its own phase, over a keyed lens |
| `MapOverlay` | struct | the overlay for records by key (`LoadedMap::overlay()`) |
| `Edge<E>` | enum | one end of a paged list: `Idle`, `Loading`, `End`, `Failed(E)` |

## The phase beside the data

A loaded list is a lens and a phase, side by side. The rows stay readable in
every phase, so a refresh or a failure never blanks them: `Refreshing(t)` and
`Failed(error, Some(t))` say the rows held are load `t`'s and something newer
is on its way, or failed to come.

Every load takes a token, and a reply carries it back. A reply whose token is
no longer the newest is dropped, so a slow answer to an old question never
overwrites a fast answer to the new one:

```vilan
import std::reactive::Source;
import std::reactive::lens::loaded::Loaded;

[derive(PartialEq)]
struct Message {
	id: i32,
	text: str,
}

fun main() {
	let feed: Loaded<Message, i32, str> = Loaded::new(|message: &Message| message.id);
	let first = feed.load();
	feed.settle(first, Ok([Message { id = 1, text = "hi" }, Message { id = 2, text = "yo" }]));
	let slow = feed.load();
	let quick = feed.load();
	feed.settle(quick, Ok([Message { id = 2, text = "yo!" }, Message { id = 3, text = "hey" }]));
	print(feed.settle(slow, Ok([])));   // false: superseded
	print(feed.items().all().len());    // 2
	feed.settle(feed.load(), Err("offline"));
	print(feed.items().all().len());    // still 2: the rows stay through a failure
}
```

## A refresh keeps its rows

A reply lands through the lens's `reconcile_by_key`: a row both lists hold
keeps its slot, wherever it moved, and wakes only if its value changed. A row
that left ends its slot; a new one gets one. A view keyed by slot rebuilds
nothing a refresh did not change.

## Pages

Each end of the list has its own state (`Edge`) and its own token. A page is
spliced in at its end in one write, and a row the list already holds is the
server's word on that row, not a second row:

```vilan
import std::reactive::Source;
import std::reactive::lens::loaded::{ Loaded, Page };

fun main() {
	let feed: Loaded<i32, i32, str> = Loaded::new(|n: &i32| *n);
	feed.resync([10, 11, 12]);
	match feed.request_older() {
		Some(let token) => feed.prepend_page(token, Ok(Page { items = [8, 9, 10], more = false })),
		None => false,
	};
	print(feed.items().all());   // [8, 9, 10, 11, 12]
	print(feed.request_older()); // None: the older end said it was done
}
```

A page asked of the old list is dropped when a load lands first, and its end
goes back to `Idle`. A failed page leaves its end `Failed(error)`; asking again
retries it. A refresh replaces the window: pages loaded before it are gone
unless the server sends them again.

## The overlay

An optimistic change is an overlay BESIDE the lens, never a write to it. The
lens holds the server's word and nothing else; the overlay holds, per row, the
changes waiting for the server, and a reader that should see them reads through
the overlay:

- `propose(slot, value)` shows `value` in that row at once. The lens's item is
  untouched; `view(slot)` reads the proposal.
- `propose_delete(slot)` HIDES the row. The lens keeps the item and its slot
  keeps following it, so a reload moves the hidden row like any other, and a
  rejection shows it again **in its own slot**: a view over it is not rebuilt.
- `propose_insert(at, value)` adds a provisional row at shown position `at`,
  under `value`'s own key. No slot follows it (`Row::slot` is `None`); it keeps
  its place by the keys of the rows shown above and below it.
- `confirm(id)` is the server taking the change: the newest confirmed change on
  a row is the server's word now, so it lands in the lens (a value replaces the
  item, a delete removes it, an insert is inserted where its provisional row
  stands). `reject(id)` writes nothing: the row shows the lens again.

Two changes on one row resolve in either order: a confirmation supersedes every
OLDER change on its row, so a late acknowledgement of an older edit does not
walk the row back. A server value that arrives while a change is pending (a
load, a page, a push) lands in the lens UNDER the change: the row keeps showing
the change, and a rejection shows the newest server value.

```vilan
import std::reactive::lens::SequenceLens;
import std::reactive::lens::loaded::Overlay;
import std::reactive::{ SequenceCell, Source };

fun main() {
	let tasks = SequenceLens::over(["write", "test", "ship"]);
	let overlay = Overlay::beside(tasks, |name| *name);
	let test = tasks.slot(1);
	let removal = overlay.propose_delete(test).unwrap();
	print(overlay.shown());   // ["write", "ship"]: hidden
	print(tasks.all());       // ["write", "test", "ship"]: the lens keeps it
	tasks.insert_at(0, "plan");
	overlay.reject(removal);
	print(overlay.shown());   // ["plan", "write", "test", "ship"]
	print(test.at());         // Some(2): the same slot, moved by the insert
}
```

**A confirmed insert is matched by key.** When the client mints the key,
`confirm(id)` inserts the proposed row where it stands (or, when a push or a
load already brought a row under that key, the provisional row simply becomes
that row). When the server mints the key, `confirm_with(id, item)` lands the
server's copy under the server's key where the provisional row stood, and the
provisional row is gone. Until that confirmation names the key, a server row
that arrived first under its own key cannot be matched, and both show.

**A row the server removes is gone,** even with an edit pending on it: the
server's word on whether a row exists outranks a local edit, and confirming
the edit does not bring the row back.

### Rows for `each`

`rows()` is the shown rows: the lens's rows in order without the hidden ones,
and the provisional rows where they stand. Key them by `row.key` and render
each through `view_key`, which reads the row held or provisional. An edit
reaches its row through the view and does not wake the row feed.

```vilan,browser
import std::reactive::lens::SequenceLens;
import std::reactive::lens::loaded::Overlay;
import std::web::ui::{ View, each, mount_root };

fun main() {
	let names = SequenceLens::over(["ada", "bob"]);
	let overlay = Overlay::beside(names, |name| *name);
	let _root = mount_root("app", || {
		<ul>
			{each(overlay.rows(), |row| row.key, |row| {
				<li>{overlay.view_key(row.key).derive(|name| name.unwrap_or(""))}</li>
			})}
		</ul>
	});
	overlay.propose_insert(1, "cy");   // shows between ada and bob, pending
}
```

## Real tasks

`fetch(|| ..)` loads with a real task. The task runs in a nursery the list
holds: a newer `fetch` cancels it (its IO aborts), and its reply, should it
come anyway, is dropped by its token. `fetch_older` and `fetch_newer` do the
same for each end. Made under an owner, the list drops every claim and cancels
its tasks when the owner goes.

`follow(source)` follows any `TransientSource` of whole lists: a pipe of tasks
sealed with `.transient()` (whose latest task wins), or one `TaskSource`. The
list lands each ready value by key, so the rows two answers share keep their
slots.

```vilan,norun
import std::reactive::SignalCell;
import std::reactive::lens::loaded::Loaded;
import std::reactive::transient::Transient;

fun search(query: str): Result<List<str>, str> {
	Ok([query, i"{query}s"])
}

fun main() {
	let query = SignalCell::new("cat");
	let results: Transient<List<str>, str> = query.derive(|asked| async search(asked)).transient();
	let found: Loaded<str, str, str> = Loaded::new(|name: &str| *name);
	let _following = found.follow(results);
	query.set("dog");   // the "cat" search is cancelled with its run
}
```

A task the pipe's body spawns belongs to that body's run only when the body
spawns it: a closure literal made outside the body (in `main`, say) carries the
nursery of the place it was made, and is not cancelled with the run. Call a
function from the body, as `search` is called here.

## Records by key

`LoadedMap<K, V, E>` holds records loaded one key at a time, over a keyed lens:
each key has its own phase and token, `settle(key, token, Ok(None))` is
"no such record" (`Absent`), and `push(key, value)` is the server's word on one
record. A keyed slot follows its key, so a record that goes `Absent` and comes
back wakes the same slot. Its overlay is a `MapOverlay`: `propose(key, value)`
is an edit, or a provisional record when the key is not held;
`propose_delete(key)` hides the record; `view(key)` reads through.

kolt's shape pairs the two: records by id in a `LoadedMap`, and an order of ids
in a `Loaded` list, each with its own phase.

```vilan
import std::reactive::Source;
import std::reactive::lens::loaded::LoadedMap;

fun main() {
	let users: LoadedMap<str, str, str> = LoadedMap::new();
	let ada = users.overlay().view("ada");
	let asked = users.load("ada");
	users.settle("ada", asked, Ok(Some("Ada")));
	let rename = users.overlay().propose("ada", "Ada L.");
	users.push("ada", Some("Ada (server)"));   // lands under the pending rename
	print(ada.get());                           // Some("Ada L.")
	users.overlay().reject(rename);
	print(ada.get());                           // Some("Ada (server)")
}
```

## What it does not do

- **No versions.** A server snapshot that is OLDER than a confirmed change, and
  arrives after it with nothing pending on the row, overwrites it. Telling the
  two apart needs a version on what the server sends, which the wire will carry
  (`proposal/reactive-lens.md` §17.3).
- **A refresh replaces the window**: pages loaded before it are dropped unless
  the server sends them again.
- **A server-minted key** cannot be matched to a provisional row until
  `confirm_with` names it (above).
- **Not a stale arm.** std's `TransientState` has none; a dropped connection is
  `Failed(error, Some(token))`.
- **JS only, for now**, like the lens (tracker B709).
