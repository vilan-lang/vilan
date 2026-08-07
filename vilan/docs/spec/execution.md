# Spec §7 — Execution & async

## 7.1 Program start and termination

A program's entry module executes its top-level statements in order. A
function named `main` at the entry module's top level is the
**entrypoint**: it is invoked automatically after the top-level
statements. (An explicit top-level `main();` call is redundant; the
program still runs `main` once.)

**Module-level bindings initialize before `main`, in dependency order.**
A binding's initializer runs after the initializer of every binding it
**evaluates at load time**: the bindings its own initializer reads, and
everything read inside whatever it calls while initializing. *Creating* a
closure evaluates nothing: a closure body runs only when something calls
it, so the bindings it mentions order nothing (two module-level closures
may name each other freely). Textual position carries no meaning: a
binding may read one declared below it, in the same file or another,
exactly as a function may call one declared later (§4.4). A **cycle** in
that relation is a compile error, since no order satisfies it, including
the one-binding cycle of an initializer that reads itself. A `const`
initializer evaluates during compilation (§9), so it evaluates nothing at
load time and waits for nothing; a binding that reads *it* still
initializes after it.

Where the relation leaves two bindings unordered, the one whose module
loaded first initializes first. Modules load in a canonical order: the
standard library's first, then the dependency packages' (each package
after the packages it depends on, in an order the dependency graph fixes
rather than the manifest's listing), then the program's own; within one
package, modules load by name. Each package's root module follows, in
that same package order, and the entry file is last. Within one file,
bindings initialize in declaration order. Nothing here depends on how a
file spells its imports, so reordering import statements, or the names
inside a `{ .. }` list, cannot change what a program does.

**Module initialization is synchronous.** An initializer may not
suspend: an `await` reachable in a module-level binding's own
initializer is a compile error, in every spelling — the implicit await
of a call to an async function, and the explicit `await` whatever its
operand (a `Task`-valued binding, a spawn, a `Task` returned by a plain
function). This is a design position, not a limit of the emission
model. The order above is *derived* from the dependency relation, and
suspending a derived sequence makes one binding's completion wait on an
ordering the program never wrote and cannot see.

*Spawning* at module level stays legal, and is the idiom the rule
steers to: `let pending: Task<T> = async work();` starts the work at
load without suspending, and `main` — which may be `async` — awaits it
where the wait is visible. Independent spawns overlap, where a sequence
of awaited initializers would serialize. Creating an async closure or
an `async { .. }` block is legal for the same reason: the body is not
the initializer, and nothing runs it at load.

Only a binding something reachable from `main` references initializes at
all (§11.2's reachability rule): an unreferenced binding's initializer
never runs, so load-time side effects are not a promise.

On process platforms (Node/Deno/Bun), the process **exits when no live
work remains**: after `main` completes, only live host handles (a
running timer, an open socket, a listening server) keep it alive, per
the host event loop. Pending tasks holding such a handle run to
completion; pending work that holds none is abandoned at exit. A
program must therefore not rely on a dropped spawn completing (join it,
§7.7, to guarantee that), and a long-lived program must keep `main`
open (a listening server does so inherently; a socket-holding client
must await something that ends with the app). On the browser platform,
`main`'s completion leaves installed handlers and subscriptions live.

`panic(message)` aborts execution with the message; it types as `any`
(§5.1). Failed `assert`s panic.

**A failing `main` terminates with a non-zero status**, whether `main`
is sync or `async`. A panic that escapes a sync `main` aborts the
process; an `async fun main()` whose work fails reports the error and
exits non-zero on the process platforms, rather than leaving the
outcome to the host's unhandled-rejection policy. Attaching that
outcome does not make the entrypoint wait: a `main` that never settles
— a server holding a listener — is unaffected and keeps running. The
browser has no exit status, so a failure there surfaces through the
host's own reporting.

## 7.2 Evaluation order

Within an expression, evaluation is **left-to-right**: operands before
operators apply, the callee before its arguments, arguments in source
order, the receiver before method arguments. A compound assignment
evaluates its target place once. Short-circuit: `&&` and `||` evaluate
the right operand only when needed. `if`/`match` evaluate exactly the
taken branch; a `match` evaluates its subject once, then tests legs top
to bottom (first matching leg wins; its guard is evaluated only when the
pattern matches).

## 7.2a Integer overflow

Arithmetic whose mathematical result exceeds the operand type's range is
**undefined behavior**: a conforming program does not overflow, and an
implementation may produce any value for one that does (the JS backend
yields f64 artifacts without trapping: precision loss for the wide
types, out-of-range magnitudes for the narrow ones). Literals are
range-checked at compile time (§2.3); runtime operations are not. An
opt-in checked family (`add_safe`, …) is recorded future work; `BigInt`
is the answer where the range itself is the problem.

## 7.3 The async model

Vilan is **await-by-default**. Asyncness is a property of *functions*,
inferred, and never written in return types:

1. A function is **async** iff its body contains a suspension point: a
   call to an async function (including async externs), an explicit
   `await`, or a call through an `async`-typed closure value.
2. A **direct call** to an async function is implicitly awaited: the
   caller receives the plain declared value, and the caller becomes
   async (asyncness propagates through the call graph).
3. A call **through a closure value** is never inferred async by
   itself: there is no static callee. The closure *type* carries the
   marker instead (§7.4).

The explicit forms:

- `async expr` (**spawn**): evaluate `expr`'s suspending computation
  concurrently; the spawn expression itself does not suspend and yields
  `Task<T>` where `T` is `expr`'s type (the task handle is opaque, and
  copying it refers to the same task). `async { … }` spawns a block.
- `await expr`: suspend until the `Task<T>` (or raw host `Promise<T>`)
  operand settles; yields `T`.

**`Task` does not nest.** A task settles with a *value*, and a task is not
one: spawning a computation that produces a task yields `Task<T>`, not
`Task<Task<T>>`, and one `await` reaches the value. This holds however the
type arises — including a generic `T` that instantiates at a task — so
`Task<..>` is idempotent: `Task<Task<T>>` is not a type any expression has.

```vilan
import std::print;
import std::task::Task;

fun wrap<T>(value: T): Task<T> {
	async { value }
}

fun main() {
	let inner: Task<i32> = async { 7 };
	let outer: Task<i32> = async { inner };   // not `Task<Task<i32>>`
	let value: i32 = await outer;             // one `await`, not two
	print(value + await wrap(inner));         // generic `T` = `Task<i32>`: same rule
}
```

The rule follows the host rather than being imposed on it: a task is a
host thenable, and the promise resolution procedure *adopts* a thenable
result instead of boxing it. A nested task is unrepresentable at runtime,
so the type says so too.

A spawned computation runs to its first suspension synchronously, then
interleaves with its spawner per the host event loop. Dropping a task
abandons nothing: the computation still runs; only its result is
discarded. A task's failure is **absorbed at the spawn**: it is
delivered to whichever `await` observes the task, and a failed task
that is never observed is reported to the host console with its spawn
origin; it does not terminate the program.

```vilan
import std::print;
import std::time::{ sleep_for, Duration };

fun step(label: str): str {
	sleep_for(Duration::millis(1));
	label
}

fun main() {
	let pending = async step("b");   // spawned
	print(step("a"));                // awaited inline — prints first
	print(await pending);
}
```

## 7.4 Async closure types, adoption, and adaptation

`async |T| U` marks a closure **value** whose calls are implicitly
awaited (suspension points in the caller). The marker is legal on
**parameter types**, **`let` annotations**, **struct fields**, and
**function return types**; calls through a marked field read
(`(self.hook)()`) and through a returned value (`make()()`, or via a
binding) await like any other. An **unannotated binding adopts**
asyncness from any value it holds (its initializer or any `mut`
rebind), so a `let f = || { …await… };` needs no marker at all.

A synchronous closure flowing into an async-typed position is fine
(awaiting a non-promise yields it unchanged).

`sync |T| U` on a **parameter** declares the opposite contract: the
callback completes inside the declaring function's synchronous
protocol, and an async closure argument is an error. (`sync` is a
contextual keyword; it only means the contract directly before a
closure type.) `std::reactive`'s recompute positions (`Signal::map`,
`set_with`, `turn`/`batch` bodies, the UI render callbacks) are `sync`.

**Adaptation.** A **plain**, value-returning closure parameter is
*asyncness-polymorphic*: each call instantiates the function with the
actual asyncness of its closure arguments:

- an async argument instantiates an **async instance**: calls through
  the parameter are awaited **sequentially** (each callback settles
  before the next begins; effects are ordered exactly as the sync body
  orders them), the instance is async, and the caller awaits it;
- sync arguments select the untouched plain instance: no awaits, no
  coloring, identical emission.

An async adapted instance traverses a **snapshot** of any value it
iterates (the receiver as of the call): its awaits admit interleaved
work, which must not tear the traversal. Adaptation follows a closure
through plain parameters transitively (`helper(xs, f)` forwarding `f`
into `map` adapts both), and never crosses these boundaries:

- a `sync` parameter (error, including transitively; the diagnostic
  names the call that made the closure async);
- a host (`external`) function's value-returning closure parameter:
  host code cannot await a Vilan closure;
- a trait/generic-dispatched call: there is no statically-known callee
  to instantiate (bind the receiver concretely, or declare the trait
  parameter `async`);
- a module-level initializer: it cannot suspend, in any spelling
  (§7.1) — spawn at load and await in `main` instead.

**The divergence rule** (the remaining stores). An async closure flowing
where a plain closure type is declared on a struct **field** or a
function's declared **return type** is:

- an **error** when that closure returns a value: the reader would
  receive a live promise typed as `T`; declare the field or return type
  `async |…| T` instead;
- **legal** when it returns `void` (*spawn semantics*): the call fires
  the closure and nobody awaits it. This is what allows UI event
  handlers and other void callbacks to suspend freely, and it applies
  to parameters as well (a void parameter spawns rather than adapts).

## 7.5 Suspension and state

At every suspension point the enclosing function's live state is
captured and restored on resumption, with one carve-out: **views may
not be live across a suspension** (§6.6). Ambient context values (§8)
are captured at closure creation and are therefore stable
across suspensions by construction.

Concurrency is cooperative and single-threaded per program: between a
suspension and its resumption, other computations (event handlers,
other spawned work) may run and mutate shared cells; within one
synchronous extent (no suspension), execution is atomic. The reactive
turn machinery (`std::reactive`) is a library discipline layered on
these primitives, not part of the language.

## 7.6 Emission guarantees (observable behavior)

A conforming implementation targeting JavaScript guarantees: the
entrypoint contract of §7.1, and the module-level initialization order
it fixes; `print` writing one line per call to the host console; panics
rejecting/aborting with the given message; left-to-right evaluation per
§7.2; truncating integer division and two's-complement `as_*` folds
(overflow excepted, §7.2a); and `i53` exactness over the wire across
its whole range. Everything else about the emitted code (names,
formatting, module layout beyond that order, the `[build]` knobs) is
implementation-defined.

## 7.7 Nurseries

`std::task::nursery(body)` runs `body` with a **nursery** established
for its **dynamic extent**: the body's execution and everything it
calls, under the ambient-value rules of §8 (in particular, closures
capture the extent at creation). Within the extent, every spawn
(§7.3's `async expr`) **registers** its task with the nursery; spawns
outside any nursery are unaffected. There is no implicit root nursery:
a program's free spawns keep §7.3's unstructured behavior.

**Join.** The `nursery` call returns the body's value, and returns only
when the body and every registered task have settled, including tasks
registered while draining (a running task's own spawns register with
the same nursery). The call is async (its callers await it), and the
body parameter is a plain closure parameter, so an awaiting body
selects an adapted instance per §7.4.

**Failure.** The nursery's outcome is the **first-observed** failure:

- a body throw wins (it always happens before the join);
- otherwise the earliest-settled task failure wins, re-raised from the
  `nursery` call; a failure that is a Vilan panic carries the spawn's
  origin (the spawning function's name) in its message.

On the first failure the nursery cancels (below). Every other
registered task is **absorbed**: observed (so it neither becomes a
host unhandled rejection nor triggers §7.3's unobserved-failure
report) with its result discarded.

**Cancellation** is cooperative. Each nursery owns a host abort signal,
fired by the body handle's `cancel()` or by the first failure; a
nursery created inside another's extent chains to its parent's signal
at creation. The standard library's IO primitives pass the ambient
nursery's signal to the host operation, so cancellation rejects
in-flight IO promptly; those rejections, and any rejection classified
as a host abort, are **cancellation echoes**: absorbed at the join,
never selected as the failure. Execution is never preempted: code runs
until its next cancellable suspension (`is_cancelled()` is the check
for compute loops). After an explicit `cancel()` the body continues and
its value is still returned; a body *suspended on* cancellable IO when
the signal fires observes the rejection, which then propagates as the
nursery's outcome under the body-throw rule.

A `nursery` call in a module initializer is rejected (§7.4's
initializer rule: an initializer cannot await).
