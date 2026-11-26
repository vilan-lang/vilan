# std::key reference

Typed paths into a struct (B711; the rules are the spec's, types §5.13).
`Task.title` is a `Key<Task, str>`, and `task[key]` is the field it names, as a
place. `Key` is in the prelude; `KeyPath` and `KeyStep` are imported by name
where a program writes their types:

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
| `KeyStep` | struct | one step: `name()`, `index()`, `owner()` (the declaring struct, `src/model.vl::Task`, for display) |

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

## What a key does not reach

A key's steps reach the fields of structs the program declares. A step into an
`Option` or an enum's variant, a list element, a map entry, a tuple position, a
type parameter, a host (`external`) type or a resource is refused by name; a
key into a place that may be missing (`MaybeKey`) is a later stage.
