# std::transient reference

Values that come and go: a fetch in flight, a refresh on its way, a failure
that still remembers the last good value, a remote source that says there is
no such thing. Concepts: the [reactive guide](../guide/reactive.md); the pipe
model this builds on: [std::reactive](reactive.md).

```vilan,fragment
import std::transient::{ TransientState, TransientSource, Transient, TaskSource };
```

## At a glance

| Item | Kind | One line |
|---|---|---|
| `TransientState<T, E>` | enum | `Pending`, `Ready(T)`, `Refreshing(T)`, `Failed(E, Option<T>)`, `Absent` |
| `TransientSource<T, E>` | trait | a `Source<Option<T>>` that also says where it stands: `state()`, `latest()`, `is_pending()` |
| `.transient()` | method on a pipe of tasks | seal a flow of tasks: the latest task wins |
| `Transient<T, E>` | struct | what `.transient()` returns |
| `TaskSource<T, E>` | struct | one task as a transient: `TaskSource::new(task)`, `TaskSource::of(task)` |

`std::rpc`'s `RemoteSource<T>` is a `TransientSource<T, RpcError>` too.

## TransientState

| State | Meaning | `get()` | `latest()` | `is_pending()` |
|---|---|---|---|---|
| `Pending` | nothing yet | `None` | `None` | `true` |
| `Ready(v)` | the value | `Some(v)` | `Some(v)` | `false` |
| `Refreshing(v)` | a newer value is on its way; `v` is the last one | `None` | `Some(v)` | `true` |
| `Failed(e, stale)` | failed with `e`; `stale` is the last value, if there was one | `None` | `stale` | `false` |
| `Absent` | there is no such source, as of now | `None` | `None` | `false` |

`Absent` is what lets a view tell a spinner (`Pending`) from "not found": both
read `None` through `get()`. The same readings are methods on the state itself
(`state.ready()`, `state.latest()`, `state.is_pending()`).

## TransientSource

```vilan,fragment
export trait TransientSource<T, E> with Source<Option<T>> {
	fun state(self): MemoCell<TransientState<T, E>>;
	fun latest(self): dyn Pipe<Option<T>>;
	fun is_pending(self): dyn Pipe<bool>;
}
```

- **`get()`** is `Some(v)` only while `Ready(v)`. A refresh reads `None`, so a
  binding over the transient itself goes blank while a new value loads.
- **`latest()`** is a FRESH pipe per call that opts in to the stale value. Bind
  it where the old value should stay on screen during a refresh — a list that
  reloads:

  ```vilan,fragment
  view("ul").each(results.latest().derive(|rows| rows.unwrap_or([])).memo(), |row| row_view(row))
  ```

- **`is_pending()`** is a fresh pipe per call: the spinner.
- **`state()`** is the whole story, as a read-only source (a `MemoCell`, which
  nothing downstream can `set`).

## `.transient()`: a flow of tasks

A pipe whose body starts a task per change is a flow of tasks, and
`.transient()` seals it:

```vilan,fragment
let user = user_id.derive(|id| async fetch_user(id)).transient();
```

Each change of `user_id` runs the body once (a pipe has one consumer) and
starts one task. The transient moves to `Refreshing(v)` — or `Pending` before
any value — and the task's outcome moves it to `Ready` or `Failed`.

**The latest task wins.** The task a newer one superseded is cancelled with its
run — a task started in a pipe's body belongs to that run — and a reply that
arrives anyway is dropped, so a slow answer to an old question never
overwrites a fast answer to the new one. Releasing the transient (its owner
going) drops whatever is in flight too.

**Errors.** Over a flow of `Task<Result<T, E>>`, an `Err(e)` becomes
`Failed(e, stale)`: the transient is a `Transient<T, E>`. Over a flow of bare
`Task<T>`, a task that panics becomes `Failed(message, stale)`, so `E` is
`str`. (A `Task<Result<T, E>>` that panics has no `E` to become: its panic is
raised again, reported like any unobserved failure, and the transient stays
where the change left it.)

Like `.memo()`, the seal is owner-tied: made inside a boundary, it is released
with the boundary.

## TaskSource: one task

```vilan,fragment
let loaded: TaskSource<User, str> = TaskSource::new(async fetch_user(id));
let counted: TaskSource<i32, str> = TaskSource::of(async count_rows());
```

One task never leaves its answer: `Pending`, then `Ready(v)` or
`Failed(e, None)`, and nothing after. `TaskSource::new` watches a task that
answers `Result<T, E>`; `TaskSource::of` watches a bare task, whose panic is
`Failed(message, None)`.

## A remote mirror

`RemoteSource<T>` maps its `Status` arm for arm: `Waiting` is `Pending`,
`Ready` is `Ready(v)`, `Absent` is `Absent`, and `Failed(e)` is
`Failed(e, stale)`, carrying what the mirror last held. `state()` reports, like
`status()`: it leases nothing, so an unwatched mirror is `Pending`. `latest()`
and `is_pending()` lease the mirror while a view binds them. The mirror's own
`get()` reads what it holds, as it always has. See [std::rpc](rpc.md).
