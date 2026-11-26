# std::key reference

Typed paths into a struct (B711; the rules are the spec's, types §5.13).
`Task.title` is a `Key<Task, str>`, and `task[key]` is the field it names, as a
place. `Key` is in the prelude; `KeyPath` and `KeyStep` are imported by name
where a program writes their types:

```vilan,fragment
import std::key::{ KeyPath, KeyStep };
```

*Keys are emitted by the JS backend only for now: a native build refuses a
program that uses one by name.*

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

## What a key does not reach

A key's steps reach the fields of structs the program declares. A step into an
`Option` or an enum's variant, a list element, a map entry, a tuple position, a
type parameter, a host (`external`) type or a resource is refused by name; a
key into a place that may be missing (`MaybeKey`) is a later stage.
