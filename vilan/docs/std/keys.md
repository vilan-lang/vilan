# std::key reference

Typed paths into a struct (B711; the rules are the spec's, types §5.13).
`Task.title` is a `Key<Task, str>`, and `task[key]` is the field it names, as a
place. A path through a place that may not be there — an `Option`'s payload,
`Doc.title::Some`, or a variant's payload slot, `Doc.shape::Rect.0` — is a
`MaybeKey<Doc, F>`, reached by continuation (`get`, `view`, `change`, `put`).
`Key` and `MaybeKey` are in the prelude; `KeyPath` and `KeyStep` are imported
by name where a program writes their types:

```vilan,fragment
import std::key::{ KeyPath, KeyStep };
```

Keys work on both backends, with the same output. One shape is still
refused by the native backend (`--backend rust`), by name: a call handed two
loans into one value where one reaches through a key held in a value,
`both(&mut s[j], &mut s[k])`. Rust cannot see that two run-time places are
apart, so the build names the shape rather than failing in rustc. A key
literal there is its field chain, so `both(&mut s[S.a], &mut s[S.b])` builds,
as `both(&mut s.a, &mut s.b)` does.

## At a glance

| Item | Kind | One line |
|---|---|---|
| `S.x`, `S.a.b` | expression | a `Key<S, F>`: the field `x` of `S`, or a path of fields, at the leaf's type |
| `s[k]` | place | the field `k` names: read, assign, `&s[k]`, `&mut s[k]`, a `borrows` return |
| `Key<S, F>` | struct | a field (or path) of `S` whose type is `F`; `PartialEq`, `Hashable`; no constructor |
| `k.then(next)` | method | `Key<S, M>` then `Key<M, F>` is a `Key<S, F>` |
| `k.path()` | method | the key with its types erased, a `KeyPath` |
| `k.name()` | method | the field names joined with `.`: `"meta.owner"` |
| `k.starts_with(prefix)` | method | whether `prefix`'s path starts this key's (a key starts with itself) |
| `KeyPath` | struct | an erased path: `name()`, `steps()`, `starts_with(p)`, `then(p)`; `PartialEq`, `Hashable` |
| `KeyStep` | struct | one step: `name()`, `index()`, `owner()` (the declaring struct or enum, `src/model.vl::Task`, for display), `position()` (a variant step's payload slot), `may_miss()` |
| `[field_syntax]` | attribute | on a handle's `fun at<U>(self, key: Key<T, U>)`: `handle.name` reads as `handle.at(T.name)` |
| `S.x::Some`, `S.x::V.0` | expression | a `MaybeKey<S, F>`: an `Option`'s payload, or one slot of a variant's payload |
| `MaybeKey<S, F>` | struct | a path of `S` to a place of type `F` that may not be there; `PartialEq`, `Hashable`, `Debug`; no constructor |
| `k.get(&s)` | method | the place copied out, `Option<F>` |
| `k.view(&s, \|place\| ..)` | method | lends the place when it is there; answers whether it was |
| `k.change(&mut s, \|place\| ..)` | method | lends the place for writing when it is there; answers whether it was |
| `k.put(&mut s, v)` | method | writes the place when it is there, never creating it; answers whether it was |
| `k.maybe()` | method | a `Key` held in a value as a `MaybeKey` (a literal needs no call) |
| `k.some()` | method | on a key (either kind) whose field is an `Option<T>`: the key of its `Some` payload |

## Reading and writing through a key

```vilan
struct Meta { owner: str, stars: i32 }
struct Task { title: str, meta: Meta }

fun at<F>(task: &mut Task, key: Key<Task, F>): &mut F borrows task {
	task[key]
}

fun main() {
	mut task = Task { title = "a", meta = Meta { owner = "ada", stars = 1 } };
	task[Task.meta.owner] = "bob";
	task[Task.meta.stars] += 2;
	at(&mut task, Task.title) = "b";
	at(&mut task, Task.meta).owner = "cy";
	print(task.title);       // b
	print(task.meta.stars);  // 3
	print(task.meta.owner);  // cy
}
```

A key over a generic struct takes the struct's arguments from where it lands:

```vilan
struct Pair<T> { left: T, right: T }

fun main() {
	let pair = Pair { left = 1, right = 2 };
	let left: Key<Pair<i32>, i32> = Pair.left;
	print(pair[left]);        // 1
	print(pair[Pair.right]);  // 2: the subject gives the arguments
}
```

## What a key costs

A key is a plain value with no cycles: copying one is cheap, and storing one
keeps nothing else alive.

- **A subscript through a literal** (`task[Task.meta.owner]`) is the field
  access itself, on both backends: no key value is built.
- **A key held in a value** reaches its field through the steps it carries on
  the JS backend. Natively it carries the field's two accessors, one to read
  and one to write, beside its path.
- **A literal as a value** is one constant per distinct literal, copied where
  it is written. Natively a copy allocates the path's list once. The step
  names are shared, and a literal's accessors are two plain functions.
- **`then`** builds the joined path and, natively, one shared allocation for
  the composed accessors. A copy of the result shares that allocation.

## Identity

Keys compare and hash by their paths, so a table keyed by keys has one entry
per field however each key was written:

```vilan
import std::hash_map::HashMap;

struct Meta { owner: str }
struct Task { title: str, meta: Meta }

fun main() {
	mut labels: HashMap<Key<Task, str>, str> = HashMap::new();
	labels.insert(Task.meta.owner, "owner");
	print(labels.get(Task.meta.then(Meta.owner)).unwrap_or("none"));  // owner
	print(Task.meta.owner.starts_with(Task.meta));                     // true
	print(Task.meta.owner.path().name());                              // meta.owner
}
```

A step is the field of one struct declaration: `Left.x` and `Right.x` are
different steps even when both are their struct's first field, and
`Pair<i32>.left` and `Pair<str>.left` are the same step.

`KeyStep::owner()` names that declaration for a reader: the declaring file, the
items it sits inside, and the struct (`src/model.vl::Task`,
`src/app.vl::render::Row`, a `#2` where one function declares two of a name).
It is for display and debugging, NOT a stable storage format: it moves when the
file is renamed or its declarations move. Data that outlives a build stores a
key by `name()` (`"meta.owner"`), which a later stage reads back.

## Debug

`Key`, `KeyPath` and `KeyStep` implement `Debug`, so `[derive(Debug)]` takes a
struct holding one. A key debugs as it is written — `Task.meta.owner`, its root
struct's name and its fields' — a path as `KeyPath(Task.meta.owner)` and a step
as `KeyStep(Meta.owner)`. `print` and `dbg` show a key's layout instead.
`[derive(Json)]` and `[derive(Wire)]` refuse a struct with a key field: a key's
encoding (by index on the wire, by name in stored data) is a later stage.

## Field syntax on a handle

A handle that reaches a struct's fields by key — a cursor into shared state, an
optic, a lens's slot — declares ONE method `[field_syntax]`, and a member that
names a field of the struct then reads as that method's call with the field's
key: `user.address.city` is `user.at(User.address).at(Address.city)`. The read
is checked, typed and built as the call, so a chain follows from each call's
result type.

```vilan
import std::shared::Shared;

struct Address { city: str }
struct User { name: str, address: Address }

/// A read/write cursor at one place inside a shared root.
struct Cursor<S, T> { root: Shared<S>, key: Key<S, T> }

impl Cursor<type S, type T> {
	fun get(self): T {
		self.root.read()[self.key]
	}

	fun set(self, value: T) {
		self.root.write()[self.key] = value;
	}

	[field_syntax]
	fun at<U>(self, key: Key<T, U>): Cursor<S, U> {
		Cursor { root = self.root, key = self.key.then(key) }
	}
}

fun main() {
	let state = Shared::new(User { name = "ada", address = Address { city = "london" } });
	let address = Cursor { root = state, key = User.address };
	let city = address.city;              // address.at(Address.city)
	city.set("paris");
	print(state.read().address.city);     // paris
	print(address.city.key.name());       // address.city
}
```

The rule, for a receiver whose type has a `[field_syntax]` method taking a
`Key<T, U>`:

- **What reads through.** A member that is NOT a member of the receiver and
  names a field of `T`. `T` is the struct at the receiver's arguments — a
  generic struct's key takes its arguments from the method's parameter, as any
  key literal does. Where `T` is a type parameter at the receiver, or no
  struct, nothing reads through and the plain "no field" refusal stands.
- **The receiver's own members win.** Its own fields are read as fields. A
  field of `T` named like one of the receiver's methods (`get` above) is
  shadowed: a bare `.get` is no method reference, so the read is refused with
  both written forms, `address.get()` for the method and `address.at(Address.get)`
  for the field.
- **What a key refuses, field syntax refuses.** A field `T.name` cannot name
  — std's `[internal]` one, a field of a resource or of an `external` struct —
  is refused at the member with the key literal's own reason.
- **A value, not a place.** The read is the method's result: `user.name = v`
  is refused with the call it stands for and the type it answers (write through
  what the method returns; when that type has a `set` method, the message says
  `.set(..)`).
- **A field is not a method.** `user.name()`, where `name` is a field of `T`
  and no method of the receiver, is refused with the field syntax that reads it
  (`vilan check --fix` rewrites it).

The declaration has one shape, checked where it is written: `fun at<U>(self,
key: Key<T, U>): R`, the receiver taken as `self` or `&self` (not `&mut self`:
a chain's next step reads through a call's result, which is never a place; not
`own self`, which would move the handle on a read that looks like a field), one
plain key parameter whose field type is the method's only type parameter, and
`T` a struct or a type parameter of the impl. It labels a bodied method of an
inherent `impl` on a struct — not a trait's method or a trait impl's — and a
handle type has one: a second `[field_syntax]` method on the same struct is
refused by name. The name of the method is the author's; `at` is the
convention.

Keys have no identity key yet (`Key<S, S>`), so a handle at the ROOT of its
state is its own type, whose `[field_syntax]` method starts a cursor:
`fun at<U>(self, key: Key<S, U>): Cursor<S, U> { Cursor { root = self.state,
key } }` on a `Root<S>`.

std's store handles read their fields through exactly this door: `Store`,
`StoreSome`, `RemoteStore` and `RemoteStoreSome` each declare one
`[field_syntax] fun field<U>(self, key: Key<T, U>)`, so `app.user.name` is
`app.field(App.user).field(User.name)` (the **Field syntax** paragraph of
[std::store](store.md)). Their impls bound `T` by `StoreFields`, which
`[derive(Storable)]` implements: a door whose impl bounds the struct it reads
through refuses a struct that misses the bound at the member, quoting the
first paragraph of the bound trait's doc comment as the steer.

## Places that may not be there: `MaybeKey`

A VARIANT STEP, `::Variant`, reaches into an `Option` or another enum's
payload: `Doc.title::Some` over `title: Option<str>` is a `MaybeKey<Doc, str>`.
A payload of several slots is reached one slot at a time, by position:
`Doc.shape::Rect.0`. The position may be left out of a one-slot payload
(`Doc.shape::Circle` and `Doc.shape::Circle.0` are one key). Steps continue
past a variant step — a field of the payload struct, another variant step —
and a path with any variant step is partial: total then total is total,
anything with a partial step is a `MaybeKey`.

```vilan
enum Shape { Circle(f64), Rect(f64, f64), Dot }
struct User { name: str, age: i32 }
struct Doc { title: Option<str>, owner: Option<User>, shape: Shape }

fun main() {
	mut d = Doc {
		title = Some("a"),
		owner = Some(User { name = "ada", age = 3 }),
		shape = Shape::Rect(1.0, 2.0),
	};
	print(Doc.title::Some.get(&d));             // Some("a")
	print(Doc.shape::Circle.get(&d));           // None: another variant
	print(Doc.owner::Some.age.put(&mut d, 4));  // true
	print(Doc.shape::Rect.1.change(&mut d, |h| {
		h = *h * 10.0;
	}));                                         // true
	d.title = None;
	print(Doc.title::Some.put(&mut d, "b"));    // false: put never creates
	print(d.title);                             // None
}
```

A `MaybeKey` has no `s[k]`: its place is not always there, so it is reached
by CONTINUATION, and each form answers whether the place was there.

- `k.get(&s)` copies the place out as an `Option<F>`.
- `k.view(&s, |place| ..)` lends `place: &F` to the closure when the place is
  there.
- `k.change(&mut s, |place| ..)` lends `place: &mut F`, to write in place.
- `k.put(&mut s, value)` is `change` with an assignment. It never creates a
  missing place: a `None` stays `None`, another variant stays as it was.

A variant step is refused by name where it names no place: an unknown
variant (with a near spelling), a UNIT variant (`Doc.shape::Dot` holds no
value; test for it with `is Shape::Dot`), a multi-slot payload without its
position or with one out of range, a field that is no enum (`Doc.n::Some`
over an `i32`), and a variant step on a VALUE (`d.title::Some`, where `d` is
a binding: test a value's variant with `is` or `match`). A key is rooted at a
struct: an enum's name does not root one (`Shape::Rect.0` is no key).

**Composition.** `then` composes either kind: `Key` then `Key` is a `Key`, and
any partial half makes a `MaybeKey`. A total key LITERAL is a `MaybeKey` where
one is expected, so `partial.then(User.age)` needs nothing more; a total key
held in a value becomes one with `k.maybe()`, and `Key::then` takes a total
key (`k.maybe().then(partial)` composes a held total key with a partial
one). `k.some()`, on a key of either kind whose field is an `Option<T>`, is
the key of its `Some` payload, for generic code:

```vilan
struct User { name: str }
struct Doc { owner: Option<User> }

fun payload<S, T>(key: Key<S, Option<T>>, subject: &S): Option<T> {
	key.some().get(subject)
}

fun main() {
	let d = Doc { owner = Some(User { name = "ada" }) };
	print(payload(Doc.owner, &d).map(|user| user.name));              // Some("ada")
	print(Doc.owner.some().then(User.name) == Doc.owner::Some.name);   // true
}
```

**Identity.** A `MaybeKey` compares, hashes and debugs by its path as a `Key`
does, and the laws K3 to K5 hold for it: keys with equal paths reach the
same place, and two keys where neither path starts with the other reach
disjoint places — the two slots of one variant (`::Rect.0`, `::Rect.1`) and
two variants (`::Rect.0`, `::Circle`) among them. A variant step's `name()`
is written as the literal writes it: `"shape::Rect.0"`, `"title::Some"`,
`"owner::Some.name"` (the position only for a payload of more than one
slot). Its `KeyStep` names the variant, `index()` is the variant's position
in its enum, `position()` the payload slot, and `may_miss()` is true.

**Cost.** A literal is one constant, as a `Key`'s is. On the JS backend each
continuation walks the key's steps, testing a variant's tag before it
descends. Natively a `MaybeKey` carries two projections answering `Option`;
a literal's are plain functions, one `match` per variant step, and `then`
shares one allocation as `Key`'s does.

A `[field_syntax]` door takes a `Key`, not a `MaybeKey`: a variant step
written through one (`handle.title::Some`) is a step on the read's VALUE, and
refused as one.

## What a key does not reach

A key's FIELD steps reach the fields of structs the program declares; an
`Option` or another enum is stepped into with a variant step. A list
element, a map entry, a tuple position, a type parameter, a host
(`external`) type or a resource is refused by name; list and map steps are
a later stage.
