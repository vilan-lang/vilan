# Dev-time freshness — data a running server serves (backlog E55, general half)

> **Status: RULED 2026-08-11 — §3's recommendation is DECLINED; §5 records the
> owner's superseding design.** The hook is out (it cannot fire for the
> headline case — see §5), asset freshness moves to fullstack-dx.md's
> `serve_build`/`Document` as their dev-mode policy, and the manual path gets
> two thin primitives: `is_watching()` and `force_refresh()`. §0–§4 stand as
> the survey record. The mechanical half of E55 (css hot-swap fetching from
> the dev channel instead of the app's own stale route) shipped separately
> and needed no design call.

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

## 5. The ruling (2026-08-11) — declining §3, and what ships instead

§3's hook is declined, for a flaw §3 itself did not state: **the hook is
keyed to rounds, and the headline case produces no round.** Editing
`app.html` is invisible to the watcher (`.vl`-only, the §2(iii) invariant),
so a re-run-on-round hook fires for exactly the edits that were already
half-served and never for the one that motivated E55 — closing that gap
would drag (iii) back in, the one shape this note agreed not to touch. The
problem is pull-shaped: every HTTP request is an opportunity to be fresh,
and no signaling protocol is needed to take it.

What ships instead, per the owner's design:

1. **Asset freshness belongs to the abstraction that owns the assets.**
   fullstack-dx.md's `serve_build` (S3) and `Document` (S5) serve the build's
   own files; freshness is their *dev-mode policy* — revalidate per request
   when watching, serve from memory in release. The dev channel's push half
   (css swap, js swap) already exists; the shell needs only per-request
   re-read plus a reload to be seen. hmr.md §8 is untouched: asset freshness
   is not server-code hot-swap; restart remains the model for code.
2. **Two primitives for the hand-rolled path**, in the process layer
   (`std::process::dev` or the thin equivalent — the §4 "surface shape"
   question resolves THIN):
   - `is_watching(): bool` — §1's signal, carried by an env var the watcher
     sets on its Node child. Uniform API (§4's scope question): defined under
     every run, `true` only under `run --watch`.
   - `force_refresh(): void` — asks every connected browser to reload once.
     Plumbing: the watcher hands its child the dev channel's port (env); the
     call POSTs to a new dev-channel endpoint; the channel broadcasts a
     `reload` event; the shim calls `location.reload()`. A no-op when not
     watching. The shim's never-reload doctrine (the stale-server loop
     hazard) is intact: that rule forbids *version-gap-triggered automatic*
     reloads — this is one-shot and user-initiated, and the reloaded page's
     shim has nothing to re-fire.
   The composed manual mechanism the owner sketched — check `is_watching`,
   re-read cheaply, `force_refresh`, the browser re-pulls fresh bytes — works
   with no watcher plumbing at all. Its one rough edge is F13 (no `stat`), so
   a hand-rolled change-detector polls by re-read-and-compare until F13
   ships.
3. **(iii) stays out**, now on two grounds: it touches the load-bearing
   invariant, and nothing needs it — the abstraction covers the common case
   and the primitives cover the rest. Deferred, not a permanent non-goal.

§4's questions resolve with the ruling: the signaling-plumbing question
DISSOLVES (no hook, no plumbing); the surface is thin; (iii) deferred; the
signal is uniform. Nothing here remains the owner's to rule.

## 6. `force_refresh()`, as shipped (2026-08-11, cycle 18)

§5 item 2's second primitive, end to end. `is_watching()` is a separate
lane's addition to the same module this cycle (`VILAN_WATCH`); this record
covers `force_refresh()` only.

- **The env var**: `VILAN_HMR_PORT`, set on the Node child at the ONE spawn
  site `run --watch`'s HMR round restarts through (`spawn_node`'s call
  inside `hmr_round`, `main.rs`). `spawn_node` grew a fourth parameter —
  `envs: &[(&str, String)]` — so every other caller (plain `run`, `run
  --watch` with no HMR, the temp-script launcher) passes `&[]` and is
  unaffected; only the HMR-active restart passes
  `[("VILAN_HMR_PORT", channel.port().to_string())]`. Defined only when a
  dev channel exists — there is no `run`-without-`--watch` value, by design
  (§5's "no watch" case IS "the var is absent", not "the var is `0`" or
  similar).
- **The endpoint**: `POST /refresh` on the dev channel (`hmr.rs`). No body,
  no auth (the same trust boundary as every other route there — bound to
  `127.0.0.1`, hmr.md §2). On receipt it broadcasts one `reload` event
  (`event_json("reload", version, None, None)`, the same framing as
  `swap`/`css`) to every connected client and answers `204`. The broadcast
  logic moved out of `DevChannel::broadcast` into a free `broadcast_to`
  function so the HTTP handler — which has the client registry but no
  `&DevChannel` (the channel that owns one lives on the main watch thread,
  not the accept loop) — can reach it too.
- **The primitive**: `std::dev::force_refresh(): void`, in a NEW file,
  `std/src/process/dev.vl` — the process layer's `std::dev`, a different
  module from the browser one (`std/src/browser/dev.vl`) under the same
  import name, resolved per platform exactly as `std::ui` already is
  (`process/ui.vl` vs `browser/ui.vl`). Reads `VILAN_HMR_PORT`
  (`std::process::env`); absent ⇒ no-op. Present ⇒ fire-and-forget —
  `async post(url, "").send()` — so `force_refresh` stays a plain, SYNC
  `void` function (not inferred-async): calling it never spreads asyncness
  to its caller, the same reasoning `std::fetch`'s own docs give for `let
  _sent = async cell.write(..)`. `std::fetch` is a base (all-platform)
  module, so the process layer reaches it with no new dependency.
- **The shim**: unchanged. `hmr_shim.js`'s `handleEvent` already had
  `case "reload": reload(); break;` (the cycle-16 rework) — a one-shot
  `location.reload()`, wired to nothing until this cycle gave the dev
  channel something to send it from.
- **The doctrine pin**: `fetchAndSwap`'s never-reload comment (a
  version-gap-triggered *automatic* reload loops against a stale server)
  is untouched and still true — `force_refresh` is neither automatic nor
  version-gap-driven, it's one explicit call, and the reloaded page's shim
  has nothing left to re-fire from it. Verified non-vacuous by planting the
  violation the doctrine forbids (a version-gap `connected` calling
  `reload()` instead of `fetchAndSwap`) in `hmr_shim.js` and watching
  `hmr_swap.rs`'s existing "heal" assertion go red, then reverting — a
  manual check against shipped code, not a new standing test (the standing
  coverage is `hmr_swap.rs`'s `connected` heal and `hmr.rs`'s
  `a_css_push_heals_a_boot_time_stale_server_route`, both unchanged by this
  work).
- **e2e** (`crates/vilan-cli/tests/hmr.rs`): a server program calling
  `force_refresh()` from an HTTP route, a connected `SseClient` (the file's
  existing raw SSE test client, standing in for a browser) observing the
  broadcast `reload` event — the full wire path, no JS harness needed since
  the shim side was already covered. Event-anchored (`SseClient::expect_kind`),
  no fixed wall-clocks. The trigger server deliberately outlives watch
  rounds (it has to answer the test's request), so it carries the E60
  `/shutdown` + connect-poll teardown, asserted dead on the green path only.
  A second test pins the no-op case directly: a plain `vilan run` (no
  `--watch`, so `VILAN_HMR_PORT` is never set) calling `force_refresh()`
  exits cleanly under `support::run_liveness()`'s bound.
- **Docs**: `docs/std/process.md` (`## std::dev (process)`, cross-linked
  from `docs/std/dev.md`'s browser page) and `docs/guide/dev-loop.md`
  (`## Freshness for a hand-rolled server`), both gated by
  `cargo test -p vilan-core --test docs`.
