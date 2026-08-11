# Dev-time freshness — data a running server serves (backlog E55, general half)

> **Status: DRAFT — for review** (2026-08-10). Scoped by the owner's ruling the
> same day: treat *revalidating boot-time reads* and *a general re-run-on-round
> hook* as **one design**, not two — the read idiom may simply be the hook
> applied to `fs`. This note surveys the shapes and recommends a direction; it
> proposes no code. The mechanical half of E55 (css hot-swap fetching from the
> dev channel instead of the app's own stale route) shipped separately and
> needed no design call — the dev channel's `/asset/<name>` route and the `css`
> event's asset name already existed; the shim just wasn't using them.

## 0. The problem

Every template and example that serves an asset from disk reads it **once, at
server boot**, and holds it in a closure for the life of the process —
`examples/todo/src/server.vl` and `crates/vilan-cli/templates/fullstack/src/
server.vl` both do exactly this for `dist/client.js`, `dist/client.css`, and
`src/app.html`. Under `vilan run --watch`, editing a `.vl` source recompiles
and either restarts the Node child (a code change) or pushes a browser-side
hot-swap (hmr.md §6) — either way the *running server's own read* is
untouched, because nothing tells it to read again. Editing `app.html` is worse:
`scan_vl` (`crates/vilan-cli/src/main.rs:394`) watches **only** `.vl` files —
a deliberate invariant (`watch-mode.md`) that keeps a build from ever
triggering its own rebuild — so an `app.html` edit doesn't even produce a
watch round. Either way, the fix today is a manual restart.

**This is not server-side HMR.** `hmr.md` §8 states that as a permanent
non-goal: the Node leg's *code* is cheap to restart, and restart is correctly
the model for it. What this note is about is a different axis — the
**freshness of data a running server reads from disk and serves**, which has
nothing to do with hot-swapping the server's compiled code. A server that
restarts on every code change can still serve week-old bytes for a file it
read once at the top of `main`; closing that gap doesn't reopen the
server-side-HMR question, and this note draws the line explicitly so a later
reader doesn't conflate the two.

## 1. The process-layer dev-mode signal

Every shape below needs one primitive first: a way for process-layer code to
ask "is this a `run --watch` session?" `std::dev` (hmr.md §4) is **browser-only
by construction** — its own header says so (`std/src/browser/dev.vl:1`) — so
it cannot be reused; this needs a new process-layer surface.

**Recommendation: an environment variable set by `vilan run --watch`** (e.g.
`VILAN_WATCH=1`), read by a small `std::process::dev` wrapper exposing
`is_watching(): bool`. It costs one line in the single shared launch path
(`spawn_node`, `crates/vilan-cli/src/main.rs:2809`) — every Node child `run
--watch` spawns already goes through it — and needs no wire protocol, no
socket, and nothing that can fail independently of the process actually
starting.

Alternatives considered:

- **A CLI flag threaded into the entry's own argv.** Rejected: argv is the
  *user's* — a workspace that already parses its own arguments (or forwards
  them to a library) would collide with a flag the toolchain injects.
- **A sentinel file dropped in the project root.** Works, but adds a
  filesystem object the toolchain has to create *and* clean up (a crashed
  watcher leaves it behind, and a stale one lies) — worse than an env var on
  every axis and cheaper to build, so this only wins if a signal needs to be
  *set* by something other than the watcher that spawned the process, which
  nothing here needs.
- **Querying the dev channel itself.** Wrong layer: it couples a plain "is dev
  mode on" boolean check to a live TCP round trip against a port that isn't
  guaranteed to be bound yet at process start (`DevChannel::bind` and the
  child spawn are not ordered against each other today), and it's needless
  weight for a question with a yes/no answer known at spawn time.

## 2. The survey

### (i) Revalidating reads — an `fs` idiom

A wrapper (`fs::dev_read_to_str`, or a flag on the existing call) that,
when `dev::is_watching()` is live, `stat()`s the path on every call and
re-reads when the mtime (or a content hash, matching `hmr.rs`'s own
byte-diff discipline) has moved; off, it collapses to exactly today's
single read. Cost: one syscall per call, negligible next to a network
request.

The catch is real and worth stating plainly: **a revalidating read only
revalidates a call site that runs again.** `let client_css =
fs::read_file_to_str(...)` at the top of `main` still runs exactly once
per process no matter how smart `read_file_to_str` is made — making the
*primitive* dev-aware fixes nothing unless the *call site* also moves
somewhere that runs per access (naturally, inside the request handler,
the way a real static-file server already works). That restructuring is
a template change, not a toolchain one, and it's the reason the owner's
ruling folds this into (ii) rather than treating it as a self-sufficient
fix: on its own, (i) is a primitive in search of a call site that invokes
it more than once.

### (ii) The general re-run-on-round hook

A mechanism that marks a piece of process-layer setup to re-run whenever
the CLI signals a new round — `dev::rerun_on_round(|| fs::read_file_to_str(...))`
in spirit, keeping today's plain, dumb `fs::read_file_to_str` underneath
and moving the "when" question out of the call site entirely. This is
what makes (i) unconditionally correct: the boot-time `let` shape every
template already writes needs no restructuring, because the *hook*, not
the read, decides when to re-evaluate.

It needs real CLI→child signaling, which `run --watch` has never had (the
Node child is spawned and either lives or gets killed — nothing talks to it
while it runs). Three plumbing shapes:

- **The child subscribes to the SSE dev channel** (`GET /events`, the same
  stream the browser shim already reads). Reuses `hmr.rs`'s existing
  server-sent-events machinery entirely on the CLI side, but casts the Node
  leg as an HTTP *client* of its own toolchain — a role no server code has
  today — and needs a genuinely new piece: a process-layer event-stream
  reader (nothing in `std` reads an SSE stream from the Node side; only the
  browser shim does, via `EventSource`, which doesn't exist under Node).
- **A POSIX signal** (e.g. `SIGUSR1`) sent to a child the watcher already
  holds a handle to. `ManagedChild::kill` (`job.rs`) already sends a
  terminating signal on restart (`Child::kill` — `SIGKILL` on unix,
  `TerminateJobObject` on Windows), but a **non-terminating** signal is a
  different call (`libc`/`nix`, already in the dependency tree via `job.rs`'s
  own unix path) — cheaper than standing up a new channel, but carries no
  payload (which asset changed), and Windows has no analog at all: a Job
  object's `TerminateJobObject` stops a tree, it does not deliver an
  arbitrary signal the way `kill(2)` does, so this shape is unix-only as
  described.
- **A stdin protocol.** Production `run --watch` does **not** pipe the
  child's stdio today — `spawn_node` sets no `Stdio`, so stdin/stdout/stderr
  are all inherited straight from the terminal (the e2e harnesses pipe them
  themselves, as test instrumentation, which is not the same thing). Writing
  a line to the child's stdin on each round (`{"kind":"round"}\n`) is the
  only one of the three that can carry a *structured* payload — e.g. naming
  which asset changed, mirroring the `css` event's `asset` field (hmr.rs's
  `event_json`) — but it is not free: it requires `spawn_node` to switch
  stdin from inherited to piped, which gives up direct terminal passthrough
  to the child (only a real cost for a dev server that reads its own
  keystrokes, which none of today's examples do, but worth naming), and
  nothing reads stdin in a generated Node server today — the runtime would
  need a small always-on reader installed the same way the browser shim is
  prepended to a bundle.

### (iii) Watching declared non-`.vl` assets

`watch-mode.md`'s invariant — only `.vl` files are polled — exists so a
build's own output can never trigger its own rebuild. Widening `scan_vl` to
also watch a **declared** non-`.vl` asset (`app.html`, a static file the
project ships) sounds like the direct fix for "editing `app.html` produces no
round at all," but it is the most delicate shape here for exactly that reason:
the moment the watched set includes anything under a project's own tree, it
has to positively exclude everything under `dist/` (`write_assets` runs every
round precisely there) or the invariant breaks the way it was built not to.

It also isn't obviously *sufficient* on its own: even a watched `app.html`
still needs (ii)'s signaling to reach a running server, so this shape doesn't
replace (ii) — it only widens what counts as "a round," and inherits every
open question in §1 of this survey either way. Nothing here proposes the
mechanism `.vl`-only scanning would generalize to name "declared": the
const-eval asset registry (`const_assets`, `const-eval.md`) tracks
*compiler-derived* assets a `.vl` file emits, which is a different set from a
hand-authored file like `app.html` that nothing derives — so reusing it
outright doesn't fit, and this is left as an open question rather than a
recommendation.

### (iv) Where `vilan init`'s fullstack template lands

Both example shapes this note cites — `examples/todo/src/server.vl` and
`crates/vilan-cli/templates/fullstack/src/server.vl` — carry the exact
boot-time-read pattern this note is about, and once (i)+(ii) ship, updating
them is a small, mechanical edit: wrap each boot-time `fs::read_file_to_str`
in the new hook, no restructuring. It is **not** the larger question backlog
item 56 opens (whether `Server`/`serve_service` compose, whether an HTML-shell
abstraction replaces the hand-authored shell entirely) — that charter is
explicitly out of scope here and is its own design note.

## 3. Recommendation

Build (ii) as the one mechanism, with **the stdin protocol** as the signaling
plumbing: it needs no new dependency, degrades to nothing when `--no-hmr` (or
no watch at all) is in play, works the same on every platform this project
already promises, and is the only option that can carry a per-asset payload if
one is ever wanted. Let (i) be the `fs`-specific sugar over it — a small
`std::process::dev` helper that wraps a boot-time read in the general hook —
rather than a standalone stat-checking primitive; a bespoke fs-only mechanism
would solve less than (ii) for a comparable amount of design and leave (ii)
still needed for any future non-`fs` case (a database connection warmed once,
a config parsed once). Leave (iii) **out of v1**: the `.vl`-only watched set
stays exactly as it is, and a round continues to mean "a `.vl` file changed" —
widening it is the one shape here that touches a load-bearing invariant, and
nothing in this survey needs it to close the E55 gap.

## 4. Open questions — the owner's to rule

- **The signaling plumbing** for (ii): stdin (recommended above), the SSE
  subscribe, or the POSIX signal — and whether the choice should be uniform
  with however A13's own dev channel evolves, or genuinely separate.
- **The process-layer surface's shape**: a full `std::process::dev` module
  mirroring `std::dev`'s hook style, or something thinner for v1 (a bare
  `is_watching(): bool` and nothing else until a second use case shows up).
- **Whether (iii) is a permanent non-goal**, stated as plainly as hmr.md §8
  states server-side HMR's, or merely deferred until a real app needs it.
- **Scope**: does the process-layer signal exist only under `run --watch`
  (mirroring HMR's own gating), or does plain `vilan run` also get it
  (always `false`), for one uniform API regardless of how the process was
  started?
