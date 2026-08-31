# Glossary

One line per term, with a link to where it's actually taught. Terms are
alphabetical. If you meet a word in the docs that isn't here, that's a
bug in the docs; please add it.

<a id="adopt"></a>**adopt**: folding a remote value into a [draft](#draft) without
re-sending it. Echoes are ignored, clean fields update, dirty fields win.
[Reactive state](../guide/reactive.md).

<a id="arena"></a>**arena**: a container that owns many values and hands out
[handles](#handle) to them. The tool for graphs and cycles.
[The memory model](../tour/memory-model.md).

<a id="backed-enum"></a>**backed enum**: an enum whose payload-free variants carry an explicit
string or integer — `enum Align { Start = "flex-start" }`. The variant
*is* that value at run time (no tag, no wrapper), which is why `value()`
costs nothing, `parse` is offered, and it keys a `Map` with no derive. Its
runtime domain is the host's, so an exhaustive `match` traps on a value
outside the set. [Data and traits](../tour/data-and-traits.md),
[spec §5.2](../spec/types.md).

<a id="binding"></a>**binding**: a name introduced by `let` (immutable) or `mut` (mutable).
[Values and types](../tour/values-and-types.md).

<a id="bound"></a>**bound**: a requirement on a generic parameter, like `T: PartialEq`:
"any T that can be compared". [Data and traits](../tour/data-and-traits.md).

<a id="boundary"></a>**boundary** (disposal boundary): a place where a UI subtree can die: a
mounted root, a list row, a `when`/`swap` body. Each boundary has an
[owner](#owner). [Building UI](../guide/ui.md).

<a id="claim"></a>**claim**: any alias into an [owner](#owner): a [view](#view), an
[arena](#arena) [handle](#handle), a loan of a [resource](#resource). Valid
only while the owner's [epoch](#epoch) is unchanged. [The memory
model](../spec/memory.md).

<a id="codec"></a>**codec**: the wire format both ends of a connection agree on:
`json_codec()` (readable) or `binary_codec()` (compact).
[Services & RPC](../guide/services.md).

<a id="context"></a>**context** (ambient value): a value carried invisibly to the code that
needs it, like the current owner or turn. Established with `run`, read
with `get`. [Functions & closures](../tour/functions-and-closures.md).

<a id="contract-check"></a>**contract check**: at connect time, client and server compare a hash of
the service's shape. A stale client fails cleanly instead of corrupting
calls. [Services & RPC](../guide/services.md).

<a id="copy"></a>**copy**: what every binding, assignment, argument pass, field store, and
**return** does to a value. The receiver gets its own; the original is
untouched. The *signature* decides: a function returning `Inner` returns
a value even where its body names the receiver's storage; one returning
`&mut Inner` returns the projection.
[The memory model](../tour/memory-model.md), [spec §6.1](../spec/memory.md).

<a id="derive"></a>**derive**: an attribute like `[derive(PartialEq, Debug)]` that
generates a trait implementation from a type's shape.
[Data and traits](../tour/data-and-traits.md).

<a id="dirty"></a>**dirty**: a [draft](#draft) whose local value has edits the server
hasn't confirmed yet. Dirty fields ignore adoption; the user's text wins.
[Reactive state](../guide/reactive.md).

<a id="draft"></a>**draft**: a local-first cell for editing server state: typing updates
locally at once, commits in the background, and keeps your text on
failure. [Reactive state](../guide/reactive.md).

<a id="drop"></a>**drop**: destruction. The `Drop` hook runs at an owner's scope end;
`drop(x)` moves a value in to destroy it early. Only [resources](#resource)
have one. [Resources](../tour/resources.md).

<a id="echo"></a>**echo**: your own change arriving back through a [mirror](#mirror). A
draft recognizes it and does nothing, so your caret never jumps.
[Reactive state](../guide/reactive.md).

<a id="effect"></a>**effect**: code that runs now and again on every change of a signal,
cleaned up automatically by its [owner](#owner).
[Reactive state](../guide/reactive.md).

<a id="entrypoint"></a>**entrypoint**: `fun main` in the entry module. It runs automatically;
on Node the process exits when it finishes. [Async](../tour/async.md).

<a id="epoch"></a>**epoch**: an [owner](#owner)'s abstract version counter. It advances when
the owner is rebound, resized, moved, or dropped; a [claim](#claim) is valid
only while it has not. [The memory model](../spec/memory.md).

<a id="extern"></a>**extern**: a declaration binding a host (JavaScript) function, object,
or property so Vilan code can call it. [Platforms](../tour/platforms.md).

<a id="frame"></a>**frame**: one encoded message on the wire. You only meet it when
building custom transports. [rpc reference](../std/rpc.md).

<a id="handle"></a>**handle**: a small copyable id into an [arena](#arena). Storable
anywhere a value is, which [views](#view) are not.
[The memory model](../tour/memory-model.md).

<a id="lang-item"></a>**lang item**: a std declaration the language itself depends on, like
`Option` for `?.` or `Add` for `+`. [Spec appendix](../spec/appendix.md).

<a id="layer"></a>**layer**: the platform-specific part of the standard library. Base is
everywhere; the browser layer is browser-only; the process layer is
server-only. [Platforms](../tour/platforms.md).

<a id="local-first"></a>**local-first**: updating local state immediately and syncing in the
background, instead of waiting on the network. What [drafts](#draft)
implement. [Reactive state](../guide/reactive.md).

<a id="mirror"></a>**mirror**: an `[expose]`d server signal that every connected client
receives a live copy of. The server writes; every client updates.
[Services & RPC](../guide/services.md).

<a id="monomorphization"></a>**monomorphization**: how generics compile: each concrete use gets its
own specialized code, so generic dispatch has no runtime cost.
[Data and traits](../tour/data-and-traits.md).

<a id="nursery"></a>**nursery**: the structured way to spawn: `nursery(body)` joins every
task spawned in the body's dynamic extent before returning the body's
value, applies the first-observed error rule, and owns the cancellation
signal std IO listens on. [Async](../tour/async.md).

<a id="owner"></a>**owner**: the object that collects subscriptions and disposes them
when its subtree dies. Created by the framework at
[boundaries](#boundary); you rarely touch one directly.
[Reactive state](../guide/reactive.md).

<a id="panic"></a>**panic**: aborting the program with a message, for states that should
be impossible. Expected failures are `Result`s instead.
[Control flow](../tour/control-flow.md).

<a id="pattern"></a>**pattern**: the shape on the left of a `match` arm: a variant to
match, payloads to bind (`Some(let x)`), a literal, or `_`.
[Control flow](../tour/control-flow.md).

<a id="platform"></a>**platform**: what a package builds for: Node, Deno, Bun, or the
browser. Decides which std [layers](#layer) are importable.
[Platforms](../tour/platforms.md).

<a id="prelude"></a>**prelude**: the names available without imports. Two layers: the
built-in set the language always has (the primitive types, `List`,
`void`), and a per-package set named by `[package] prelude` — std's base
one (`print`, `Option`/`Some`/`None`, `Result`/`Ok`/`Err`) by default,
`"std::web"` for applications, any module for a custom one, or `false`
for none. A prelude is the weakest scope: declaring or importing one of
its names shadows it silently. [Spec §4.7](../spec/names.md).

<a id="resource"></a>**resource**: a value with a single owner that *moves* instead of copying
and is destroyed deterministically after its last use (a `Database`, an
`OwnedNursery`). [Resources](../tour/resources.md).

<a id="safe-integer"></a>**safe integer**: an integer JavaScript's 64-bit floats represent
exactly: anything within ±2^53. Vilan's `i53`/`u53` are named for this
window. [Values and types](../tour/values-and-types.md).

<a id="service"></a>**service**: a server struct whose `[rpc]` methods clients call and
whose `[expose]`d signals clients [mirror](#mirror).
[Services & RPC](../guide/services.md).

<a id="signal"></a>**signal**: a value cell that code can subscribe to. The unit of
reactive state. [Reactive state](../guide/reactive.md).

<a id="spawn"></a>**spawn**: starting async work without waiting for it: `async expr`.
Gives you a `Task<T>`. [Async](../tour/async.md).

<a id="subscription"></a>**subscription**: one live "call me on change" registration on a
signal. Effects manage theirs through [owners](#owner); manual `sub`
hands you the object to dispose. [Reactive state](../guide/reactive.md).

<a id="suspension"></a>**suspension**: a point where a function pauses and other code runs: a
call to something async, or an explicit `await`. [Views](#view) may not
be held across one. [Async](../tour/async.md).

<a id="task"></a>**task**: the handle a spawn yields: eager, opaque, copy-refers-to-the-
same-task. A task's failure is absorbed at the spawn: a later `await`
receives it; an unobserved failure is reported, not crashed on.
[Async](../tour/async.md).

<a id="trait"></a>**trait**: a named capability a type can implement, used as a bound on
generics. Like an interface, but explicit and compile-time only.
[Data and traits](../tour/data-and-traits.md).

<a id="transport"></a>**transport**: the thing that carries rpc calls: the reconnecting
WebSocket in production, http or in-process variants for special cases.
[rpc reference](../std/rpc.md).

<a id="turn"></a>**turn**: a batch of signal writes that becomes visible at once.
Event handlers and rpc handlers each run in one automatically.
[Reactive state](../guide/reactive.md).

<a id="value-semantics"></a>**value semantics**: the rule that data is copied, not shared, unless
you use an explicit sharing tool.
[The memory model](../tour/memory-model.md).

<a id="variant"></a>**variant**: one case of an enum, possibly carrying data:
`Shape::Circle(2.0)`. [Data and traits](../tour/data-and-traits.md).

<a id="view"></a>**view**: a short-lived borrow of a place (`&x`, `&mut x`) that aliases
instead of copying. Can't be stored, returned into long-lived state, or
held across a [suspension](#suspension).
[The memory model](../tour/memory-model.md).

<a id="wave"></a>**wave**: one settling of a [turn](#turn): every affected watcher runs
once with the final values. [Reactive state](../guide/reactive.md).

<a id="wire"></a>**Wire**: the "can travel over the network" capability: scalars, lists
and options of Wire types, and anything with `[derive(Wire)]`.
[Services & RPC](../guide/services.md).
