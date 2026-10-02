# std::store reference

The fine-grained version of a type, generated from its shape. A
`SignalCell<User>` wakes every reader on every write — a new city reruns the
binding that shows the user's name. A **store** wakes exactly the readers whose
value changed, and nobody writes a reactive twin of `User` to get that.
Concepts: the [reactive guide](../guide/reactive.md); the traits a store
implements: [std::reactive](reactive.md).

```vilan,fragment
import std::store::{ Storable, Store, StoreSome, StoreFlag };
```

## At a glance

| Item | Kind | One line |
|---|---|---|
| `[derive(Storable)]` | derive | a struct's or an enum's store surface: its diff and one projection per field |
| `Store<T>` | struct | a root plus a path: a `Source<T>` and a `Signal<T>`, the root and every projection alike |
| `Store::new(value)` | constructor | a root holding `value` |
| `StoreSome<P>` | struct | a handle *through* an `Option` (or an enum variant): a `Source<Option<P>>` plus `patch` |
| `StoreFlag` | struct | a discriminant as a read-only `Source<bool>` (`is_some()`) |
| `[reactive(coarse)]` | field attribute | the field is one slot, compared whole |
| `[reactive(name = "..")]` | field attribute | the name the field's projection is generated under |

## A store

```vilan
import std::reactive::{ Owner, Signal, Source, run_with_owner };
import std::store::{ Storable, Store };

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

A handle is data: copying it allocates nothing, and it has no owner. It is a
`Source<T>`, so every pipe, effect and binding takes one (`city.derive(..)`,
`<p>{city}</p>`), and a `Signal<T>`, so anything that writes a signal writes it
(`set_with`, `optimistic`, `bind_value`).

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
import std::store::{ Storable, Store };

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
import std::store::{ Storable, Store, StoreSome };

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

## What it costs

- **A derive line per type**, and a knob per coarse field.
- **A handle is four fields** — the root's slot tree, its path, and an in-place
  read and write — and it allocates nothing until something subscribes.
- **A whole write costs one comparison per live slot.** A handle write costs
  the comparisons under the handle.
- **Projections are methods**: `user.address().city()`.

On the native backend a store builds and wakes as it does on JS, with one gap:
observing a `StoreSome` (`nick.effect(..)`) is refused by name for now — the
native backend cannot yet start a pipe over a generic type implementing
`Source<Option<P>>`. Reading and patching through one build.
