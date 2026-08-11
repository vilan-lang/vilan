# Server-side rendering — render and replace (A7)

> **Status: v1 SHIPPED 2026-07-23 — arc complete for S1 + S2** (S1 the
> process-layer `std::ui` twin; S2 `386074a`, replace semantics + the
> full-stack e2e). **S3 (the Wire initial-state blob) stays UNBUILT by
> decision** — demand-gated per §6c, so v1 keeps the double-fetch. Two
> residues carried, neither blocking: `style_var` is snapshot-pinned but sits
> outside the differential (the DOM stub no-ops `style.setProperty`) —
> **CORRECTED 2026-08-04 (A21): the stub never no-opped it, and `style_var` is
> in the differential now; see §S1's residue** — and the
> S2 amendment's applicability question — **kolt and the walkthrough cannot
> SSR under v1**, since their views read the live rpc client at build time,
> handlers capture it, and browser-layer imports come with it. That factoring
> is filed nowhere else; take it up when an app demands SSR.
> *Flagged 2026-07-29, corrected 2026-08-03: §S2 below described
> `examples/ssr` as a 3-package workspace — D7 converted it to one package,
> two entries; the §S2 text now reads that way.*
>
> Prior status: RATIFIED 2026-07-22 — implementation starts 2026-07-23 (user
> call: too large for end of day). The §6 calls landed per recommendation:
> (a) the process layer omits `mount_root`; (b) the shell splice stays in
> user code for v1; (c) S3 waits on demand — kolt's usage decides.
>
> Original status: DRAFT 2026-07-22 — for review. Backlog A7, reframed per
> the user's call (2026-07-22): **v1 is render + replace — hydration is not
> deferred, it is rejected.** The client never adopts server DOM; it renders
> fresh and replaces. Resumability (A7b, §7) is the recorded end-state, and
> the progression replace → resumability builds nothing hydration would have
> built: hydration's whole apparatus (adopt-in-`bind_*`, deterministic node
> addressing, mismatch reconciliation) is exactly the re-execution machinery
> resumability deletes. SEO and first paint are delivered by v1; only
> time-to-interactive waits for A7b — the same as it would under hydration.

## 0. What exists (all verified in prior arcs)

- **Platform layers shadow whole modules** (the platform model): a module
  name resolves per building platform through `search_roots`; std's browser
  layer holds `ui.vl`/`dom.vl`, and a process build importing them today gets
  the clean cross-platform error. Adding a **process-layer `ui.vl`** makes
  `import std::ui` legal on `@process` with a different implementation — the
  exact mechanism `fs`/`http` already use in reverse. A `common` library's
  component module compiles against the browser `ui` in the client leg and
  the process `ui` in the server leg with no annotation — the layer system's
  whole point.
- **`std::ui`'s browser surface**: `View { element }`, `view(tag)` builders,
  `bind_text`/`bind_attr`/`bind_each`/`when`/router boundaries — each a
  subscription writing one DOM property; `mount_root` returns the root
  `Owner`; teardown is `Owner`-scoped. `std::reactive` (signals, turns) is
  **base-layer** — it runs everywhere already.
- **The serving idiom**: the server leg reads `dist/client.js` and a
  hand-authored HTML shell from disk and serves them (walkthrough, todo,
  kolt). There is no framework server; routes are user code.
- **Wire/json**: derive-gated serialization, codec-neutral, no reflection.
- **HMR's groundwork**: compiler-minted state identity + fingerprints + the
  transferable-as-value classification — the serialization-side inputs A7b
  needs; v1 needs none of them (nothing crosses a boundary except HTML).

## 1. The v1 model — render, serve, replace

1. **Server**: the route handler calls the app's own view-building code —
   the same component functions the client runs — against the process-layer
   `ui`, which builds an **HTML string tree** instead of DOM. `render(view)`
   returns the markup. The handler splices it into the HTML shell at the
   mount point and serves it. Create-serialize-discard: no effects, no
   subscriptions, no owner survives the request.
2. **Browser, before JS**: the user (and the crawler) sees the full page.
   First paint and SEO are done. No handlers exist yet — identical to
   hydration's pre-JS window.
3. **Browser, on boot**: `mount(id, view)`/`mount_root` **clear the container
   first** (`replaceChildren`), then mount the freshly-built live UI. Since
   the same component code produced both trees, the replacement is visually
   idempotent when the data matches; when it doesn't (the data moved between
   render and boot), the replace shows the truth — which is the correct
   behavior, not a mismatch error.

No adoption, no addressing, no reconciliation. The client-side change is a
single semantic adjustment — mount clears before appending — which is a no-op
for every existing app (their containers are empty).

## 2. The process-layer `ui` (the one real piece of work)

A second `ui.vl` in std's process layer, same public surface, over a string
tree:

- `View` wraps a **node record** (tag, attributes, children, text) instead of
  a DOM `Element`; `view(tag)`, `.child`, `.text`, `.attr`, `.class` build it.
- **`bind_*` reads once**: `bind_text(signal)` takes `signal.get()` and
  embeds it as text — no subscription is created (nothing to notify; the
  value at render time is the value served). `bind_each` renders the current
  list; `when` renders the taken branch; `bind_draft`/event handlers (`on`,
  `on_event`) **accept and discard** — a server render of a button is just
  `<button>` (its handler is client business).
- `mount`/`mount_root` do not exist server-side... or exist as errors?
  **Call (§6a)**: the process `ui` either omits them (a component calling
  `mount_root` is a client entry, not a renderable view — the natural
  factoring is `fun app(): View` shared, `main` differs per leg) or defines
  them as compile-time-absent so the error is clear. Recommendation: omit;
  the missing-member error already names the module and platform.
- **`render(view: View) -> str`** — the serializer: proper escaping (text
  nodes and attribute values), void elements (`<br>`, `<img>`…), and
  attribute ordering **deterministic** (insertion order — the components
  wrote them in code order, both sides identical by construction).
- `std::dom` stays browser-only. A component reaching for raw DOM cannot
  SSR, and the existing cross-platform gate says so at the `import` with the
  standard error — the correct boundary, free of charge.
- The maintenance cost, stated: two `ui` implementations whose *rendered
  output* must correspond. The gate is a differential pin (§4): the same
  view built both ways — the browser one under the A10 DOM stub, the server
  one via `render` — must produce equal trees (tag/attr/text structure).

## 3. What v1 explicitly does not do

- **No hydration** — rejected, not deferred (§0).
- **No initial-state embedding** (the double-fetch stands in v1): the client
  re-fetches via rpc on boot exactly as today, and a data change between
  render and boot shows as a content update. The fix — serializing initial
  store data into the HTML and adopting it client-side (`Wire` + a
  `<script type="application/vilan-state">` blob) — is **S3, its own slice
  with its own calls (§6c)**, because it introduces a cross-process state
  contract v1 doesn't need.
- **No streaming/suspense**, no partial rendering, no router integration
  (the server route matches the path and picks which view to render by
  hand; A10's enum-route model composes with that today — a `match` on the
  route enum server-side — and deeper integration waits for demand).
- **No pre-JS event capture** — clicks before boot do nothing (same as
  hydration; A7b's event-replay closes it).

## 4. Slices (suite-gated, docs same commit, per-case pins)

1. **S1 — SHIPPED 2026-07-23**: `vilan/std/src/process/ui.vl` (295 lines) —
   the full read-once surface (`bind_text/class/attr/value/draft`,
   `bind_each`/`when`/`swap`/`show`, `style_var`), handlers accepted and
   discarded, `mount`/`mount_root` omitted per §6a. `render`: insertion-order
   attributes, text/attr escaping, the void set (children of a void drop
   silently — the recorded §5 risk). 22 pins + corpus `ssr-render.vl`
   (equivalence-gated) + the cross-implementation differential
   (`ssr_differential.rs`: property→attribute canonical mapping, browser-stub
   tree ≡ rendered string **byte-for-byte**) + the `std::dom` platform-gate
   pin; docs in `guide/ui.md` same commit. One §2 amendment settled in
   flight: `on_event` must be MIRRORED-and-discarded, not omitted — a
   process program may import a browser module (the router) it never calls
   into, and requirement analysis loads it; the handler's event type stays
   generic (a discarded handler never touches it). Residue: `style_var` is
   snapshot-pinned but outside the differential (the DOM stub no-ops
   `style.setProperty`). **CORRECTED 2026-08-04 (A21).** The premise was
   wrong from the day it was written: this stub's `_upsertStyle` folds the
   property into the `style` attribute — added in `309e2bb`, the very
   commit this residue was recorded in — exactly the way the process twin
   folds it. Nothing had to be extended; `style_var` joined the shared
   component and both twins agree byte-for-byte (planted non-vacuous by
   diverging the browser value). The lesson is the ordinary one about a
   residue recorded from reading rather than running: a note that says a
   harness cannot see something is cheap to *check*.
2. **S2 — SHIPPED 2026-07-23**: `mount` clears before appending
   (`mount_root` inherits; the HMR teardown clear composes untouched; no
   golden churned — every existing container was empty). New
   `examples/ssr` (one package, two entries as of D7 — 2026-08-03
   correction, was a 3-package workspace at S2's own ship date; `fun
   app(): View` is shared between them, the server entry splices
   `render(app())` at the `<!--ssr-->` marker); the
   full-stack e2e asserts the served HTML carries the rendered content
   **before any JS** (escaped text, signal-fed list, the `when` branch)
   and that the client boot **replaces** — one live root, server nodes
   detached and unupdated, a click propagating to the new tree.
   `guide/ssr.md` + sidebar, same commit.

   > **Amendment (2026-07-23 — the kolt finding, v1's applicability
   > boundary):** kolt cannot SSR in v1, for three verified reasons: its
   > views read the live rpc client during the BUILD (not in discarded
   > handlers), every handler captures that client (which the server — the
   > rpc *host* — has no value for), and they import browser-layer modules
   > (`router`, `storage`) so a process build correctly refuses at the
   > import. The walkthrough shares the shape. **v1 render-and-replace
   > fits self-contained and server-data-seeded apps** — the pattern
   > `examples/ssr` demonstrates — not client-mirror-threaded ones. The
   > path for the latter is a factoring question (shell renders
   > server-side; mirror-bound regions stay client-only) plus S3's seeded
   > data where the double-fetch bites; neither is v1 scope. This is the
   > §6c demand signal recorded honestly: kolt's SSR waits on that
   > factoring, not on this arc.
3. **S3 (optional, gated on §6c) — the initial-state blob**: server
   `embed`/client `adopt` over `Wire`, keyed like HMR's stash; the client's
   first fetch skipped when the blob answers. Recorded here so its absence
   in v1 is a decision, not an oversight.

## 5. Risks, named

- **Output divergence between the two `ui`s** — held by the S1 differential
  pin; any new `bind_*` must land in both layers or the pin fails.
- **The replace flash** — real but small: same code + same data ⇒ identical
  markup swap (imperceptible); different data ⇒ an honest update. Transient
  pre-JS input/media state is lost at the swap — documented, accepted for
  v1 (the HMR scroll/focus restore machinery exists if it ever matters).
- **Server-side signal misuse** — a component that *writes* signals during
  build runs identically server-side (signals are base-layer); effects
  don't attach, so nothing leaks — but a component relying on effect
  side-channels at build time renders stale. The docs page states the rule:
  build pure, bind reactive.

## 6. Open calls — wanted before S1

- **(a) `mount_root` server-side**: omit from the process layer
  (recommendation — the natural `fun app(): View` factoring makes it
  unreachable) vs defining a clear compile-error stub.
- **(b) The splice API**: v1 keeps the shell splice in user code
  (`shell.replace("<!--app-->", render(app()))` — recommendation: honest,
  zero new surface) vs a `render_into(shell, marker, view)` convenience in
  std.
  *Re-read 2026-08-11 (the owner, deciding fullstack-dx.md §10.4): this
  decline was scoped to the zero-knowledge wrapper — "honest, zero new
  surface" is a cost-benefit call about a helper that saved nothing — not a
  boundary on std and the document. `render_into` stays declined on its own
  terms; fullstack-dx §5's `Document` supersedes the splice by EARNING its
  surface (tags derived from the build, loud faults), which is exactly what
  the wrapper lacked.*
- **(c) S3 in this arc or on demand**: recommendation — build S1+S2, ship,
  and let kolt's real usage decide whether the double-fetch hurts enough to
  pull S3 forward.

## 7. A7b — resumability (recorded, not planned)

The end-state: the server serializes the reactive graph and handler wiring
so the client executes **nothing** at boot — handlers wake lazily, by
address, on first interaction. vilan's position is unusually strong: closure
conversion is compiler-owned, so handler-splitting is a compiler pass, not a
bundler trick; HMR's identity/fingerprint/transfer machinery is the
serialization substrate; `Wire` is the codec. The missing pieces are the
compiler pass (stable handler addresses + captured-environment
serialization), the wake runtime, and pre-boot event capture/replay. It
replaces §1's step 3: instead of render-fresh-and-replace, the client keeps
the server DOM and wires it lazily. Nothing in v1 obstructs it, and nothing
in v1 is discarded by it — the render-only `ui` and the replace semantics
remain the fallback for non-resumable pages and the dev loop.
