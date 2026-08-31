# Gotchas

A checklist of idioms that trip people up, each with the working shape.
Grown as findings land.

Arriving with an error message in hand? The [error index](errors.md) is
organized by message instead of by topic.

## Language

- **`match` can't be an operator operand.**
  `(match x { … }) + 1` → bind the match to a local first.
- **A bare integer literal adapts to its peer; two typed variables
  don't.** `stamp + 1000` and `stamp < 1000` are fine on an `i53` (the
  literal takes the peer's type), but mixing two differently-typed
  *variables* in a comparison or an addition is an error: there are no
  implicit conversions. Convert with `as_*` or unify the declarations.
- **Concatenation renders nothing for you, and an i-string is
  concatenation.** `"p=" + point` and `i"p={point}"` are the same
  expression, and both refuse a value with no string form rather than
  printing its runtime shape (`p=1,2`). Call `to_string()` — writing an
  `impl … with Display` if the type has none. The string also has to be
  on the *left*: an expression takes its type from its left operand, so
  `count + "!"` is an error, not an `i32` holding `"1!"`.
- **A guarded last `match` arm proves nothing.** A guard tests the value
  and exhaustiveness reasons about the type, so `B if ready => …` leaves
  `B` missing. Write the arm you meant to fall through to:
  `B if ready => …, _ => …`.
- **An exhaustive `match` over a backed enum can still panic.** The enum
  *is* its backing string or number at run time, so a host value outside
  the set (an `external fun` return, a callback's argument) reaches the
  trap arm: `Align: "middle" is not one of its values`. Give it a `_` arm,
  or take the value through `Align::parse`, which answers `None`.
- **A view may not be held across a *suspension*, not just an `await`.**
  Calling an async function without the keyword suspends identically —
  and so does a sync-looking function that reaches one.
- **Writing over a resource-holding place destroys what it replaces.**
  `slot.held = Holder::Empty` runs the old value's destructor before the
  write, through a `&mut` view and on an owned place alike. That is the
  rule working; it is worth knowing it is *when* teardown happens.

## Reactive & UI

- **`shared.read()` is a copy.** `shared.read().push(x)` is lost; write
  through the cell: `shared.write().push(x)`.
- **Mutate signal collections with `update`** — `signal.update(|&mut list|
  { list.push(x); })` writes the stored value through a view and notifies
  once. Never mutate a `get()` result (also a copy). `set_with` stays the
  read-*transform*-write form.
- **`bind_value` fights remote updates.** For server-backed fields use
  `bind_draft`.
- **`show` keeps bindings live** while hidden; use `when` to drop state and
  subscriptions.
- **Disposal doesn't cancel the in-flight wave**: a subscriber already
  queued in the draining turn may fire once more; only *later* deliveries
  are guaranteed gone.

## Services & the wire

- **Contract-mismatch errors on connect usually mean a leaked old server**
  still holding the port: `ss -tlnp | grep <port>`, kill by PID.
- **`desc` is an SQL keyword.** Name the column `description` (any SQL
  keyword as a column name fails in `CREATE TABLE`).
- **Value semantics cross the wire**: a mirrored list is a fresh copy per
  update; mutate via rpcs, never by writing the client's mirror signal.

## Process & testing

- **A completed Node `main` exits the process.** Long-lived
  clients/servers must hold `main` open.
- **Process-target artifacts are `.mjs`, not `.js`.** `vilan build
  app.vl` writes `app.mjs`, and a multi-entry package writes
  `dist/<name>.mjs` per process entry — so `node dist/server.js` becomes
  `node dist/server.mjs` in scripts, Dockerfiles and process managers. A
  **browser** leg keeps `.js`: its `<script type="module">` declares the
  module at the load site, where the extension carries no weight.
- **A module-level binding cannot `await`, in any spelling.** Not the
  implicit await of an async call, and not an explicit `await` on a
  `Task`, a spawn, or an `async { … }` block. Spawn at module level
  (`let pending = async work();`) and do the waiting inside
  `async fun main()`.
- **A server should not read its own build by hand.** `require_build`,
  `require_shell` and `serve_build` describe it from what the build
  actually wrote; a path typed from memory leaves a renamed leg compiling
  and the page blank ([Persistence](../guide/persistence.md)).
- **`pkill -f <pattern>` can match your own shell's command string.** Kill
  by tracked PID.
- **Rebuild the debug binary before regenerating corpus goldens.** A stale
  binary silently writes wrong goldens.
