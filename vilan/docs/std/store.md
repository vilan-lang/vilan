# std::reactive::store reference

The fine-grained version of a type, generated from its shape. A
`SignalCell<User>` wakes every reader on every write — a new city reruns the
binding that shows the user's name. A **store** wakes exactly the readers whose
value changed, and nobody writes a reactive twin of `User` to get that.
Concepts: the [reactive guide](../guide/reactive.md); the traits a store
implements: [std::reactive](reactive.md).

```vilan,fragment
import std::reactive::store::{ Storable, Store, StoreSome, StoreFlag };
```

## At a glance

| Item | Kind | One line |
|---|---|---|
| `[derive(Storable)]` | derive | a struct's or an enum's store surface: its diff and one projection per field |
| `Store<T>` | struct | a root plus a path: a `Source<T>` and a `Signal<T>`, the root and every projection alike |
| `Store::new(value)` | constructor | a root holding `value` |
| `StoreSome<P>` | struct | a handle *through* an `Option` (or an enum variant): a `Source<Option<P>>` plus `patch` |
| `StoreFlag` | struct | a discriminant as a read-only `Source<bool>` (`is_some()`, `is_online()`) |
| `[reactive(coarse)]` | field attribute | the field is one slot, compared whole |
| `[reactive(name = "..")]` | field attribute | the name the field's projection is generated under |
| `when_live(handle, body)` | `std::web::ui` | a variant as content: rebuilt only when the variant changes, the payload's `Store<P>` in hand |
| `.at(key)` | on a `HashMap` field | one key's value as a `Store<Option<V>>` that wakes for that key only |
| `.contains(x)` | on a `HashSet` field | one member's presence as a `Store<bool>` |
| `.by_key(k)` | on a keyed `List` field | one element, found by its `Keyed` key, as a `Store<Option<T>>` |
| `.push(x)`, `.splice(..)`, … | on a `List` field | the `SequenceCell` writes, landed in place |
| `.keys()`, `.map(..)`, `each_by(..)` | on a collection field | the shape's operators, told per-key or per-span ops |

## A store

```vilan
import std::reactive::{ Owner, Signal, Source, run_with_owner };
import std::reactive::store::{ Storable, Store };

[derive(PartialEq, Storable)]
struct Address {
	city: str,
	zip: str,
}

[derive(Storable)]
struct User {
	name: str,
	address: Address,
}

fun main() {
	let user = Store::new(User { name = "Alice", address = Address { city = "Oslo", zip = "0150" } });
	let city: Store<str> = user.address().city();   // a handle, not a value
	let page = Owner::new();
	run_with_owner(page, || {
		user.name().effect(|name| print(i"name: {name}"));
		city.effect(|now| print(i"city: {now}"));
	});
	city.set("Bergen");                               // prints "city: Bergen" — and not the name
	print(user.address().zip().get());                // 0150
	page.dispose();
}
```

`Store::new(value)` makes a ROOT and hands back the handle to it. Every field
of a derived type is a projection — another `Store`, cut from its parent:
`user.address().city()` is a `Store<str>`. A projection reads, writes,
subscribes and projects further exactly as the root does, so there is one type
for both.

The value lives in the root, in place. `get()` copies out only the value at the
handle's path — a read of `city` copies the city, never the whole user — and
`set(v)` assigns in place through the root.

**Field syntax.** A field reads the way the value spells it: on a `Store<T>`
(and on a `StoreSome<T>`, below) a member that names a field of `T` is that
field's projection, so `user.address.city` is `user.address().city()` — the
same handle, written without the calls:

```vilan
import std::reactive::{ Signal, Source };
import std::reactive::store::{ Storable, Store };

[derive(Storable)]
struct Address {
	city: str,
}

[derive(Storable)]
struct User {
	name: str,
	address: Address,
}

fun main() {
	let user = Store::new(User { name = "Alice", address = Address { city = "Oslo" } });
	user.address.city.set("Bergen");     // the handle `user.address().city()` is
	print(user.address.city.get());      // Bergen
}
```

A field read is a handle, so it is written through, never over:
`user.name = "Bob"` is refused with the `.set(..)` that writes it. A field whose
projection is renamed (`[reactive(name = "..")]`, below) is read by the
projection's name, as a call; a type that does not derive `Storable` has no
projections, so its store reads no fields this way. The handle's own fields
are std's `[internal]` machinery and are not members outside std, so a field
of `T` named `path` or `root` reads `T`'s field.

A handle is data: copying it allocates nothing, and it has no owner. It is a
`Source<T>`, so every pipe, effect and binding takes one (`city.derive(..)`,
`<p>{city}</p>`), and a `Signal<T>`, so anything that writes a signal writes it
(`set_with`, `optimistic`, `bind_value`). Every handle to one path of one store
has one `identity()`, so a body that `track()`s a handle on every run keeps one
edge on it.

## What a write wakes

> A write wakes exactly the live slots whose value changed.

A **slot** is one path's subscriber list. It is made by the first subscription
at its path, counts its subscriptions, and goes with the last one, taking every
level of the tree it leaves empty. A read allocates nothing.

- **A whole-value write** (`user.set(next)`) compares old against new, but only
  where a slot is live — or where a whole-value slot above needs to know whether
  anything changed. A write into a store nobody watches compares nothing.
- **A handle write** (`city.set("Bergen")`) diffs only the handle's own value,
  then wakes the whole-value slots of its ancestors — `address`, then `user` —
  if it changed. Siblings are never looked at: the path is static. This is the
  fast path; a whole write is the compat path, for a fetch result or a snapshot
  arriving at a boundary.
- **A write of the value already held wakes nothing.** Unlike
  `SignalCell::set`, which never compares, a store write always does: that is
  the layer's whole job.
- **`notify()`** wakes the handle's slot and its ancestors' without comparing —
  the escape hatch for a change the store cannot see.

The wakes go through one turn, so an observer of two changed fields — a
`combine`, a tracked `derive` reading both — runs once, after the write has
fully landed.

## [derive(Storable)]

The derive is the opt-in, and it is also the boundary: vilan has no private
fields, so a type projects its fields only if its author derived `Storable`.
Every other type is a **leaf**: one slot holding the whole value, compared with
`==` when it has `PartialEq`, and counted as changed on every write that covers
it when it has not (a closure, an opaque handle). `Option` needs no derive (see
below).

A field named like a member of the handle — `get`, `set`, `derive`, `effect`,
`patch`, … — could never be reached as a projection, because a handle's own
member wins. The derive refuses it; rename the projection:

```vilan
import std::reactive::{ Signal, Source };
import std::reactive::store::{ Storable, Store };

[derive(Storable)]
struct Request {
	[reactive(name = "verb")]
	get: str,
}

fun main() {
	let request = Store::new(Request { get = "/" });
	request.verb().set("/index");
	print(request.verb().get());
}
```

`[reactive(coarse)]` makes a field ONE slot, compared whole with `==`, even when
its type derives `Storable` — an `Address` that is always edited as a whole:

```vilan,fragment
[derive(Storable)]
struct User {
	name: str,
	[reactive(coarse)]
	address: Address,     // one comparison per covering write, not one per field
}
```

## Through an Option

`Option` is structural: a handle on an `Option` reaches through its `Some`.

```vilan
import std::reactive::{ Signal, Source };
import std::reactive::store::{ Storable, Store, StoreSome };

[derive(Storable)]
struct Profile {
	nick: Option<str>,
}

fun main() {
	let profile = Store::new(Profile { nick = None });
	let nick: StoreSome<str> = profile.nick().some();
	print(nick.patch("al"));                 // false: the option is None, nothing lands
	profile.nick().set(Some("ally"));
	print(nick.patch("al"));                 // true
	print(nick.get().unwrap_or("-"));        // al
	print(profile.nick().is_some().get());   // true
}
```

`some()` is a `StoreSome<P>`: a `Source<Option<P>>` that reads `None` while the
option is `None`, and `patch(value)`, which writes only while it is `Some` and
answers whether it landed. It is not a `Signal` — `set(None)` would mean
nothing there. Making the option `None` (or `Some`) is a write to the option's
own handle. A subscription through the `Some` survives a write that only
changes what the payload holds. `is_some()` is a `StoreFlag`, the
discriminant: it wakes when the option comes or goes, and never on a write
inside it.

## Enums

A derived enum's store diffs its discriminant first, then the live variant's
payload. Per variant the derive writes `is_<variant>()`, the discriminant as a
`StoreFlag`, and per variant WITH a payload a handle through it, named after the
variant in snake case (`Online(Device)` gives `online()`, a `StoreSome<Device>`;
`Away(str, i32)` gives `away()`, a `StoreSome<(str, i32)>`):

```vilan
import std::reactive::{ Owner, Signal, Source, run_with_owner };
import std::reactive::store::{ Storable, Store, StoreSome };

[derive(PartialEq, Storable)]
struct Device {
	name: str,
	since: i32,
}

[derive(PartialEq, Storable)]
enum Presence {
	Offline,
	Online(Device),
}

fun main() {
	let presence = Store::new(Presence::Online(Device { name = "laptop", since = 1 }));
	let name: StoreSome<str> = presence.online().name();
	let page = Owner::new();
	run_with_owner(page, || {
		presence.is_online().effect(|live| print(i"online: {live}"));
	});
	print(name.patch("phone"));                  // true — and `is_online` does not wake
	presence.set(Presence::Offline);             // online: false
	print(name.patch("ghost"));                  // false: the variant is not live
	print(name.get().unwrap_or("-"));            // -
	page.dispose();
}
```

- **A same-variant write patches the payload.** `Online(d1)` to `Online(d2)`
  with only `since` changed wakes `since` and the enum's own slot — not `name`,
  not the discriminant. Subscriptions into the payload survive, because slots
  hang off the type's paths, not off the value.
- **A different variant** moves every live slot under either payload between
  `Some` and `None`, so all of them wake, and so do the two variants' flags
  (`is_offline()`, `is_online()`). A third variant's flag does not: its answer
  did not change.
- **A handle through a variant writes only while the variant is live**: `patch`
  answers whether it landed. A write cannot choose the variant; switching it is a
  write to the enum's own handle (`presence.set(..)`).

A payload variant's handle also has `live()`, its discriminant as a `StoreFlag`,
and `assume()`, the payload as a `Store<P>` for code that only runs while the
variant is live.

### when_live — rebuild only on the variant

`when_some` hands its body a cell of the WHOLE payload, so every binding in the
body reruns on every write to any of its fields. `when_live` follows only the
discriminant, and hands the body the payload's own `Store<P>`:

```vilan,fragment
<aside>{when_live(presence.online(), |device| device_panel(device))}</aside>
```

The body is built under a fresh owner when the variant goes live and disposed
when it goes away; a write inside the payload wakes only the bindings that read
what changed. Should a derivation in the body run in the turn that ends the
variant — a derivation settles before the effect that tears the body down — it
reads the LAST payload a read through the live variant saw (a message edited
and then deleted shows its edit, never the text the body was built over), and a
write then lands nowhere. On the server it renders the live payload once.

## Collections

A collection field takes its declared shape — no derive to write:

- a **`HashMap<K, V>`** is a KEYED node. `at(key)` is a `Store<Option<V>>` whose
  slot is that key's alone: a write to key 7 wakes the readers of key 7 and
  nobody else. `at(key).set(None)` removes the key, and `.some()` reaches into
  a present value. `insert(key, value)` and `remove(key)` are the same two
  writes.
- a **`HashSet<T>`** likewise: `contains(x)` is a `Store<bool>`, and
  `set(true)`/`set(false)` (or `insert`/`remove`) add and remove `x`.
- a **`List<T>`** is a SEQUENCE node. Every `SequenceCell` write — `push`,
  `prepend`, `insert_at`, `remove_at`, `splice`, `clear`, … — lands in place.
  A list of `Keyed` values reads one element with `by_key(k)`. There is no
  `at(index)`: a position shifts under a splice, and the handle would silently
  change which element it names.

```vilan
import std::reactive::delta::SequenceCell;
import std::hash_map::HashMap;
import std::reactive::{ Owner, Signal, Source, run_with_owner };
import std::reactive::store::{ Storable, Store };

[derive(PartialEq, Storable)]
struct Channel {
	name: str,
	messages: List<i32>,
}

[derive(Storable)]
struct Global {
	channels: HashMap<i32, Channel>,
}

fun main() {
	let global = Store::new(Global { channels = HashMap::new() });
	global.channels().insert(1, Channel { name = "general", messages = [] });
	global.channels().insert(2, Channel { name = "random", messages = [] });
	let page = Owner::new();
	run_with_owner(page, || {
		global.channels().at(1).some().name().effect(|name| print(i"1 is {name.unwrap_or("-")}"));
		global.channels().at(2).effect(|channel| print(i"2 has {channel.map(|held| held.messages.len()).unwrap_or(0)}"));
	});
	global.channels().at(2).some().messages().push(40);   // "2 has 1" — channel 1 does not wake
	let _renamed = global.channels().at(1).some().name().patch("lobby");   // "1 is lobby"
	global.channels().remove(2);                          // "2 has 0"
	page.dispose();
}
```

A whole write of a collection diffs by key — a list by its common prefix and
suffix, as `ListCell::reconcile_to` does — and wakes only the keys whose value
changed. A map is never compared with `==`.

A collection handle is also the shape's FLOW. `global.channels().keys()`,
`.values()`, `log.map(..)`, `each_by(log, ..)` start from it and are told OPS —
a `MapOp` per changed key, a `SetOp` per member, a `SeqOp` per splice (`SetAt`
for an element changed in place) — never handed a copy to diff. A push builds
one row. Each flow keeps its own op log at the collection, recorded only while
the flow is open.

## What it costs

- **A derive line per type**, and a knob per coarse field.
- **A handle is four fields** — the root's slot tree, its path, and an in-place
  read and write — and it allocates nothing until something subscribes.
- **A whole write costs one comparison per live slot.** A handle write costs
  the comparisons under the handle.
- **Projections are methods**: `user.address().city()`, and field syntax
  (`user.address.city`) is the same call written without the parentheses.
- **A write through a variant lands in place** for a single payload and an
  `Option`'s `Some` (a `match &mut` capture is a writable view); a variant
  with several payloads copies its tuple out and back. A write under a map key
  copies that key's value out and back: a map lends no value in place.
- **A `by_key` read scans the list** for the first element under its key.

On the native backend a store builds, wakes and is observed as it does on JS.
