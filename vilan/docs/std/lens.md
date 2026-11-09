# std::reactive::lens reference

One reactive wrapper for any structure, with the shape passed in. A plain
`List<T>`, a `HashMap<K, V>` or a rope of your own goes in; what comes out is
the only way to write it, and every write says what it did. Readers ask for
exactly as much as they watch: the whole value, one item (a **slot**, which
follows its item wherever it goes), or one part of one item. Concepts: the
[reactive guide](../guide/reactive.md); the pipe model this builds on:
[std::reactive](reactive.md).

```vilan,fragment
import std::reactive::lens::{ SequenceLens, KeyedLens, SetLens, Indexed, Resizable, Lookup, Membership, Change, sequence, indexed, keyed, set };
```

The module is reached **by name**. `std::reactive` does not re-export it, so a
program that never imports it does not pay to analyze it. `ListCell`,
`HashMapCell` and `HashSetCell` are not built on it: they stay what
[Cells](cells.md) describes, and the lens is the answer for a structure std
has no cell for, and for rows whose slots follow their items.

## At a glance

| Item | Kind | One line |
|---|---|---|
| `sequence(value)` / `SequenceLens::over(value)` | constructor | a lens over anything `Resizable<T>`, addressed by position |
| `indexed(value)` / `SequenceLens::indexed(value)` | constructor | a READ-ONLY sequence lens over anything `Indexed<T>`: written through `update` and `plan` only |
| `keyed(value)` / `KeyedLens::over(value)` | constructor | a lens over anything `Lookup<K, V>`, addressed by key |
| `set(value)` / `SetLens::over(value)` | constructor | a lens over anything `Membership<T>`, addressed by member |
| `Lens<S, A, T, H>` | struct | the lens itself: structure, address, item, handler |
| `SequenceLens<S, T>`, `KeyedLens<S, K, V>` | struct | the same lens, naming only the types that matter; `lens()` hands out the full one |
| `.slot(at)` | method | an `ItemSlot`: a `Source<Option<T>>` that follows the item now at `at` |
| `.part(name, read)` | method on a slot | one part of one item, as a source of its own |
| `.slot(at).sequence(..)` | method on a slot | a lens over a sequence inside the item |
| `.items()` | method | std's op feed of values, for `map`, `filter`, `memo`, folds |
| `.rows()` | method | the op feed of slots, for `each` |
| `.index_by(key)` | method | a list that is also looked up by id: `find(key)` is a slot |
| `.entries()` | method on a keyed lens | std's map op feed, for the map operators |
| `.members()` | method on a set lens | std's set op feed (`SetOp`), a `SetSource` |
| `.update(..)`, `.plan(..)` | methods | a write the shape has no method for, naming its changes |
| `sequence_checked(value)` | constructor | a sequence lens that checks every `update` and `plan` |
| `Change<A>` | enum | what one write did: `Splice`, `Insert`, `Replace`, `Delete`, `Edit`, `Move`, `Reset` |
| `Indexed<T>`, `Resizable<T>`, `Lookup<K, V>`, `Membership<T>` | traits | the shapes: what a structure implements to go under a lens |

## A sequence lens

A sequence lens has std's whole sequence surface (`push`, `insert_at`,
`remove_at`, `splice`, `clear` and the rest of `SequenceCell`), and every one of
those writes says what it did, so no call site names a change. A slot follows
the ITEM: an insert above it moves it and does not wake it, and once its item
is deleted it reads `None` and wakes once to say so.

```vilan
import std::reactive::SequenceCell;
import std::reactive::lens::SequenceLens;

fun main() {
	let names = SequenceLens::over(["ada", "bob", "cy"]);
	let bob = names.slot(1);
	let _watch = bob.on_change(|name| print(i"bob's slot now reads {name.debug()}"));
	names.insert_at(0, "zed");      // bob moves to 2; his slot does not wake
	print(bob.get());               // Some("bob")
	print(bob.at());                // Some(2)
	bob.set("Bob");                 // a write through the slot lands on his item
	names.remove_at(2);             // gone: the slot reads None, and wakes once
	print(names.all());
}
```

`get()` on the lens is the whole value, and a reader of it wakes on every
write; that is the coarse read. `slot(at)` is one item. A position past the
end has no item to follow, so its slot reads `None` and stays so.

Inside one turn the slots move AT the write, and observers run once, at the
settle: `batch(|| { names.insert_at(0, "x"); slot.set("y"); })` writes `"y"`
over the slot's item, not over whatever was at its old position.

## Changes

A write's changes are addresses, never values: a slot reads the structure in
place when it is asked.

| Change | Meaning |
|---|---|
| `Splice(at, removed, inserted)` | `removed` items left at `at` and `inserted` arrived there — what every sequence write records |
| `Insert(at, n)` | `n` items arrived at `at` (`Splice(at, 0, n)`) |
| `Delete(at, n)` | `n` items left at `at` (`Splice(at, n, 0)`) |
| `Replace(at)` | the item at `at` is another item now |
| `Edit(at, part)` | one part of the item at `at` changed; it is the same item |
| `Move(from, to)` | the same item, now at `to`; its slot goes with it, silently |
| `Reset` | anything may have changed |

One splice is one change: a listener never pairs a delete with an insert. A
keyed lens reads a `Splice` of a key as its `Replace`.

The shape's methods write these for you. For a write the shape has no method
for, `update` takes the write and the changes it made, and `plan` names its one
change BEFORE the write, which lets the op feeds see what is about to leave:

```vilan
import std::reactive::lens::{ Change, sequence_checked };

fun main() {
	let list = sequence_checked(["a", "b", "c", "d"]);
	let taken = list.plan(Change::Delete(1usize, 2usize), |held| {
		let gone = [held[1], held[2]];
		held.splice(1, 2, []);
		gone
	});
	list.update(|held| {
		held.insert(0, "z");
		(void, [Change::Insert(0usize, 1usize)])
	});
	print(taken);
	print(list.all());
}
```

A change that lies points slots at the wrong items. `sequence_checked` copies
the structure once per `update` or `plan` and panics when the lengths do not
add up or something outside the window the changes name moved; use it in tests
and debug builds. `explain_sequence(before, after, changes)` is the same check,
answering what is wrong instead of panicking. The literals carry `usize`
because an integer literal is not yet inferred through `Change<A>`.

## A structure of your own

A structure goes under a lens by implementing its shape. A sequence's shape
comes in two halves: `Indexed<T>` is the reading half (`count`, `item`, and a
default `lend`), and `Resizable<T>`, which requires `Indexed<T>`, is the
writing half (`splice`, and defaults for `put`, `change` and `shift`). A
structure overrides a default when it can do better, and one `impl ..
with Resizable<T>` may write the members of both halves. The structure holds
no reactive code:

```vilan
import std::reactive::SequenceCell;
import std::reactive::lens::{ Resizable, SequenceLens };

/// Lines of text kept in chunks of at most four.
export struct Rope {
	chunks: List<List<str>>,
}

export impl Rope with Resizable<str> {
	fun count(self): usize {
		mut total: usize = 0;
		for chunk in self.chunks {
			total += chunk.len();
		}
		total
	}

	fun item(self, at: usize): Option<str> {
		mut start: usize = 0;
		for chunk in self.chunks {
			if at < start + chunk.len() {
				ret chunk.get(at - start);
			}
			start += chunk.len();
		}
		None
	}

	fun splice(&mut self, at: usize, removed: usize, inserted: List<str>) {
		mut lines: List<str> = [];
		for chunk in self.chunks {
			lines.push_many(chunk);
		}
		mut taken: usize = 0;
		for taken < removed {
			lines.remove(at);
			taken += 1;
		}
		mut offset: usize = 0;
		for line in inserted {
			lines.insert(at + offset, line);
			offset += 1;
		}
		mut chunks: List<List<str>> = [];
		mut piece: List<str> = [];
		for line in lines {
			piece.push(line);
			if piece.len() == 4 {
				chunks.push(piece);
				piece = [];
			}
		}
		if piece.len() > 0 {
			chunks.push(piece);
		}
		self.chunks = chunks;
	}
}

fun main() {
	let text = SequenceLens::over(Rope { chunks = [["one", "two", "three"], ["four", "five"]] });
	let five = text.slot(4);
	text.splice(1, 0, ["one and a half", "one and three quarters"]);   // one change
	print(five.get());   // Some("five"): the slot followed its line across a chunk split
	print(five.at());    // Some(6)
}
```

**A shape impl in a module that exports anything must be `export impl`.** The
lens's generic body reaches the impl through its bound, and a module with an
explicit export surface exports its impls only when they are written `export
impl`. Without it, the build stops with an error that points into the lens:

```vilan,fragment
impl Rope with Resizable<str> { .. }   // in a module that exports `Rope`
// error: 'item' is provided by an `impl` in module `rope`, and module `lens`
// does not admit it: a generic body resolves under the file it was DECLARED in ..
```

The fix is `export impl Rope with Resizable<str>`, not the import the message
suggests (tracker B712). A module with `export *;` exports its impls already.

`List<T>` is `Indexed<T>` and `Resizable<T>`, `HashMap<K, V>` is
`Lookup<K, V>` and `HashSet<T>` is `Membership<T>` out of the box.

## A read-only sequence

Some structures decide where an item goes: a sorted index puts `25` after
`20`, whatever the caller wanted. Such a structure implements `Indexed<T>`
and not `Resizable<T>`, and goes under `indexed(value)`. Its lens has
everything that READS a sequence — slots, parts, `rows()`, `items()`,
`length()`, `index_by` — and no `splice`, `push`, `insert_at` or `move_item`
that could put an item where the order says it does not belong. Calling one
is a build error (`'SortedIndex' does not implement trait 'Resizable<i32>'`).

It is written through `update`, beside the structure's own mutator, which
returns the change it made because only it knows where the item landed. When
the caller can say where first, `plan` is exact: the op feed sees what is
about to leave.

```vilan
import std::reactive::lens::{ Change, Indexed, indexed };

struct SortedIndex {
	items: List<i32>,
}

impl SortedIndex {
	fun place(self, value: i32): usize {
		mut at: usize = 0;
		for at < self.items.len() && self.items[at] <= value {
			at += 1;
		}
		at
	}

	/// The structure decides, and says what it did.
	fun insert(&mut self, value: i32): Change<usize> {
		let at = self.place(value);
		self.items.insert(at, value);
		Change::Insert(at, 1)
	}
}

impl SortedIndex with Indexed<i32> {
	fun count(self): usize {
		self.items.len()
	}

	fun item(self, at: usize): Option<i32> {
		self.items.get(at)
	}
}

fun main() {
	let prices = indexed(SortedIndex { items = [10, 20, 30] });
	let thirty = prices.slot(2);
	prices.update(|held| (void, [held.insert(25)]));
	print(prices.all());   // [10, 20, 25, 30]
	print(thirty.at());    // Some(3): the slot followed 30, and did not wake
	let gone = prices.peek(|held| held.place(10)) - 1;
	prices.plan(Change::Delete(gone, 1usize), |held| {
		held.items.remove(gone);
	});
	print(prices.all());   // [20, 25, 30]
}
```

A write AT a position through the lens itself — `replace`, `edit`, or a slot's
`set` and `edit` — is a runtime panic on a read-only lens, not a build error:
those methods are shared by every lens. Write the item through `update`.

## A real rope

The structure above is a toy: its `splice` re-cuts every chunk. The corpus
program `vilan/test/lens-rope.vl` is the real thing, and it is still nothing
but a shape impl. Lines live in chunks of at most `cap` lines (and at least
half that, unless there is one chunk), with each chunk's starting line kept so
`item(at)` is a binary search. Its `splice` cuts again only the chunks the span
touches, plus one neighbour when what is left would be under half a chunk (the
merge), and replaces them in place; `put` and `change` write one line where it
sits and cut nothing.

What that buys under the lens:

- A slot follows its line through every split and merge. Chunks are the
  rope's business; the lens's slot table never hears of them.
- A 5,000-line paste is `splice(at, 0, lines)`: ONE `Splice` change, one row
  op carrying 5,000 rows, a `map` over `items()` that runs for the 5,000
  arrivals and nothing else, and one wake for the whole value and the length.
- `index_by` keeps working across both, by the same one change.

## Parts

`slot.part(name, read)` is one part of one item, as a source. It wakes when
that part is edited or the item is replaced or deleted, and not when another
part is. `edit(name, change)` writes in place and names the part:

```vilan
import std::reactive::Source;
import std::reactive::lens::sequence;

[derive(PartialEq)]
struct Task {
	title: str,
	done: bool,
}

fun main() {
	let tasks = sequence([Task { title = "write", done = false }, Task { title = "ship", done = false }]);
	let ship = tasks.slot(1);
	let title = ship.part("title", |task| task.title);
	let done = ship.part("done", |task| task.done);
	let _t = title.on_change(|text| print(i"title: {text.debug()}"));
	let _d = done.on_change(|flag| print(i"done: {flag.debug()}"));
	ship.edit("done", |task| task.done = true);   // prints only "done: Some(true)"
}
```

Parts are named by strings for now; a typed field key replaces them (B711), and
until then a misspelt name compiles and leaves its watcher asleep.

## Rows for `each`

`rows()` is the op feed of SLOTS. `each` renders one row per slot and hands
the row its item's slot, so nothing is copied, a row whose item is edited is
not rebuilt, and a moved item's row moves. Key the rows by `slot.key()`:

```vilan,browser
import std::reactive::lens::sequence;
import std::web::ui::{ View, each, mount_root };

fun main() {
	let names = sequence(["ada", "bob", "cy"]);
	let _root = mount_root("app", || {
		<ul>
			{each(names.rows(), |slot| slot.key(), |slot| {
				<li>{slot.derive(|name| name.unwrap_or(""))}</li>
			})}
		</ul>
	});
}
```

`items()` is the feed of VALUES for std's collection operators: `map`,
`filter`, `count` and `memo` follow a lens through it with no operator changed.

```vilan
import std::reactive::{ CollFlow, CollPipe, SequenceCell, Source };
import std::reactive::lens::sequence;

fun main() {
	let numbers = sequence([1, 2, 3, 4]);
	let evens = numbers.items().filter(|n| n % 2 == 0).memo();
	numbers.splice(0, 2, [10, 11]);
	print(evens.get());   // [10, 4]
}
```

## An index by key

A list that is also looked up by id is an index over a sequence lens, not a
second structure. `index_by(key)` keeps a map from each item's key to its slot,
in step with every write; `find(key)` is one lookup:

```vilan
import std::reactive::SequenceCell;
import std::reactive::lens::sequence;

fun main() {
	let tabs = sequence(["inbox", "drafts", "sent"]);
	let by_name = tabs.index_by(|name| *name);
	tabs.insert_at(0, "starred");
	match by_name.find("sent") {
		Some(let slot) => print(slot.at()),   // Some(3)
		None => print("not found"),
	}
}
```

## A keyed lens

The keyed handler's slots follow the KEY: a slot exists before its key does,
reads `None` while the key is absent, and reads the key again when it returns.
A write to one key wakes that key's slot and no other.

```vilan
import std::hash_map::HashMap;
import std::reactive::Source;
import std::reactive::lens::KeyedLens;

fun main() {
	let empty: HashMap<str, i32> = HashMap::new();
	let ages = KeyedLens::over(empty);
	let ada = ages.slot("ada");
	let _watch = ada.on_change(|age| print(i"ada: {age.debug()}"));
	ages.put("ada", 36);    // ada: Some(36)
	ages.put("bob", 41);    // nothing: another key
	ages.remove("ada");     // ada: None
	ages.put("ada", 37);    // ada: Some(37)
	print(ages.length().get());
}
```

`entries()` is std's map op feed (`MapOp`), so `filter`, `map_values` and
`memo` follow a keyed lens; `reconcile_keyed(next)` becomes `next` by the
changes that get there and leaves equal values alone.

## A sequence of ids and a keyed lens of records

A keyed lens has no order, and does not need one. What a list of records
looked up by id wants is two lenses: a SEQUENCE of ids, which owns the order,
and a KEYED lens of records, which owns the data (R14). Rows come from the
ids; each row reads its record through the keyed lens's slot for that id. A
reorder touches only the ids, so no record's reader wakes; an edit of a record
touches only the records, so no row moves. `index_by` on the ids answers where
an id sits now.

```vilan
import std::hash_map::HashMap;
import std::reactive::lens::{ KeyedLens, sequence };
import std::reactive::{ SequenceCell, Source };

[derive(PartialEq)]
struct Task {
	title: str,
	done: bool,
}

fun main() {
	mut start: HashMap<str, Task> = HashMap::new();
	start.insert("t1", Task { title = "write", done = false });
	start.insert("t2", Task { title = "review", done = false });
	start.insert("t3", Task { title = "ship", done = false });
	let records = KeyedLens::over(start);
	let order = sequence(["t1", "t2", "t3"]);
	let place = order.index_by(|id| *id);

	// A row per id; each reads its record by key.
	let rows = order.rows();
	let first = rows.get()[0];
	let ship = records.slot("t3");
	let _w = ship.on_change(|task| print(i"t3 woke: {task.map(|task| task.title).debug()}"));

	order.move_item(2, 0);                       // the order changed; no record woke
	print(place.find("t3").map(|slot| slot.at()));   // Some(Some(0))
	records.edit("t3", "done", |task| task.done = true);   // t3 woke; no row moved
	print(first.get());                          // Some("t1"): the row still holds its id
	order.push("t4");
	records.put("t4", Task { title = "celebrate", done = false });
	print(order.all());
}
```

The pairing costs one lookup per row and keeps each lens doing one thing: the
sequence's slots follow positions, the keyed lens's slots follow keys, and
neither needs a handler the other has.

## A set lens

A set is addressed by its members, so its lens is keyed by member. A structure
goes under one by implementing `Membership<T>` (`contains`, `members`, `add`,
`remove`, and a default `count`); `HashSet<T>` does out of the box. A member's
slot is its PRESENCE: `slot(x)` is a `Source<bool>`, as `HashSetCell`'s
`contains(x)` is, which exists before `x` joins, wakes when `x` joins or
leaves, and wakes for no other member. (Its `Option<T>` would only echo back
the `x` the caller already holds.)

```vilan
import std::hash_set::HashSet;
import std::reactive::Source;
import std::reactive::lens::SetLens;

fun main() {
	let empty: HashSet<str> = HashSet::new();
	let tags = SetLens::over(empty);
	let urgent = tags.slot("urgent");
	let _watch = urgent.on_change(|on| print(i"urgent: {on}"));
	tags.add("urgent");      // urgent: true
	tags.add("later");       // nothing: another member
	tags.add("urgent");      // nothing: already held, nothing is written
	tags.remove("urgent");   // urgent: false
	print(tags.length().get());
	mut next: HashSet<str> = HashSet::new();
	next.insert("later");
	next.insert("soon");
	tags.reconcile_set(next);   // one turn: "soon" joins, nothing else is written
	print(tags.all().len());
}
```

`add` and `remove` answer whether they wrote, and write nothing for a member
already held or already absent. `members()` is std's set op feed (`SetOp`): a
`SetSource`, so a set flow opens over it. Every op is known from the change
alone, since a member is its own address, so a write that names several
changes through `update` replays them in order. `reconcile_set(next)` is the
diff door: one change per member that left or joined, in one turn.

## Nesting

`slot.sequence(part, empty, lend, borrow)` is a lens over a sequence INSIDE
the slot's item. A write through it is an `Edit` of that part on the outer item:
watchers of that part hear it, watchers of the item's other parts do not. When
the outer list shifts, the inner lens follows its item; when the item is
deleted, the inner lens reads `empty`.

```vilan
import std::reactive::SequenceCell;
import std::reactive::lens::sequence;

[derive(PartialEq)]
struct Section {
	title: str,
	items: List<str>,
}

fun main() {
	let sections = sequence([Section { title = "Todo", items = ["a"] }, Section { title = "Doing", items = ["x", "y"] }]);
	let doing = sections.slot(1);
	let empty: List<str> = [];
	let items = doing.sequence("items", empty, |section, read| read(&section.items), |section, change| change(&mut section.items));
	let _title = doing.part("title", |section| section.title).on_change(|_t| print("the title woke"));
	let _count = doing.part("items", |section| section.items.len()).on_change(|n| print(i"items: {n.debug()}"));
	items.push("z");                                                    // items: Some(3)
	sections.insert_at(0, Section { title = "Inbox", items = [] });   // the inner lens follows "Doing"
	print(items.all());
}
```

`lend` and `borrow` say where the inner sequence is in the item; a typed field
key will replace both. Make the inner lens once per slot and part and share it:
two inner lenses over one part do not see each other's writes.

## Lifetime

A slot is counted by its handles. Made inside an owner scope — a view, a row,
a derive's run — a slot, a feed and an index are released with that owner;
outside one, call `release()` on each when done. The slot ends with its last
handle: it follows nothing from then on, and a new ask makes a new slot.

## What it costs

- **A new way to be wrong.** A change named at an `update` call site can lie,
  and a lie points slots at the wrong items. The shape methods cannot; use them,
  and `sequence_checked` where `update` is unavoidable.
- **Two ways to read.** `get()` is coarse and slots are fine. A view that
  indexes `get()` works, and re-renders on every write.
- **Nothing is kept for what nobody asked for.** A lens nobody follows holds its
  structure and one notification cell: the slot table is made by the first
  `slot`, a slot's cell by the first listener, and the length's and the
  positions' cells by the first `length()` and `index()`. Its bare writes cost
  about what a `ListCell`'s do. It still holds more memory than a `ListCell`
  (about twice, for a small list): its handle, its access closures and its
  empty tables.
- **JS only, for now.** The native backend does not build a program that uses
  the lens yet: it drops the binding of a `.. is Some(let at) else ret;` guard
  (tracker B709).
