# REPORT — reactive-43 (Order 43)

Branch `reactive-43`, rebased onto `origin/next` @49e0e817 (hygiene-43, editor-43,
native-43 merged). Six commits, one per item (+ the coordinator's fold). Model Opus.

## Per item

| item | state | sha | pins (transport) |
|---|---|---|---|
| A134 (a)+(c) | LANDED | 7c76e11f | `reactive_channels::a134_two_stub_calls_for_one_origin_are_one_mirror_and_one_subscribe` (IN PROCESS), `a134_a_stub_in_a_cold_select_reads_its_leased_mirror_over_a_socket` (SOCKET, real WebSocket); `service_layer`'s hundred-handles pin updated (twin handles: calls 13 → 12) |
| A137 | LANDED (fix) | 61c9de90 | `a137_a_mirror_joining_a_held_forward_is_seeded_from_its_sibling` (IN PROCESS), `a137_a_joining_mirror_is_seeded_over_a_socket` (SOCKET); plain + keyed each |
| A135 (b)+(c) | LANDED | 394f6be7 | `a135_n_calls_of_a_per_call_cell_leave_one_registration_after_disconnect` (IN PROCESS, `drop_session`), `a135_a_closed_socket_releases_its_handlers_per_call_cells` (SOCKET close → server teardown); `inference::lifetimes::a135_*` ×2 (warning, static) |
| A136 (a)+(b) + I7 + I8 | LANDED | 61ba6460 | `a136_a_memo_maker_that_builds_cell_global_survives_its_first_caller` (IN PROCESS, the probe's F/G); `inference::lifetimes::a136_*` ×2; `inference::std_surface::i7_*`, `i8_*`; the memo pin moved to `get_or_insert` |
| A132 | LANDED | 5979f0eb | native leak census rows: `delta-law` 14 → 0, `list-cell` 92 → 0 (plant restoring `source.since` in `map_each` reads 92 again) |
| A133 | STOPPED (owner) | — | see below |
| fold (coordinator) | LANDED | 2c34918c | `subscribe_pulling` back to one statement after native-43's F46 |

### A134 — the premise held
Verified on 762c6aa5: the probe's section A read `A sees 0` forever. Mechanism as filed
(a fresh unleased mirror per pull). Built: the `[service]` expansion declares one
module-level `MirrorTable<M>` per handle method (the only place the element type is
known — `ReactiveClient` is not generic and the language has no type-erased read-back;
a Client-struct field would break every hand-built `Client { transport, codec,
reactive }` literal in the tree); the stub calls `ReactiveClient::origin_source` /
`origin_keyed_source`, keyed by the client's identity + the request envelope `call`
would send. Entry released with the mirror's last REAL close (a `retire` hook on
`RemoteSource`/`KeyedSource`), with its owner if never leased, and with the client's
`dispose`. The probe's section A reads 1 then 2; B/C now make ONE server call. Docs:
the stub's generator comment, `unleased_source`'s doc, the services guide ("one handle
per origin", the cold-select shape), `std::rpc` reference; `and_then`'s doc (in the
A136 commit) says its select runs on every pull and must be pure.

### A137 — NOT clean over a socket; mechanism named and fixed
Section H over a real WebSocket (scratch project: server + client processes): the
second mirror of one deduped server cell got NO seed — its `Subscribe` JOINS the live
forward (`LiveForward.holds` +1) and the server sends nothing (correctly: a frame names
a channel, a re-seed would double-notify the sibling). Plain joiner: silent until the
next change. KEYED joiner: worse — the next patch lands on an empty mirror and it stays
DESYNCED (`keyed=2/1`). The in-process "neither saw the second add" was the lazy mint
racing the adds, not a loss. Fix is client-side (`seed_from_sibling`): each route
offers a snapshot while its mirror holds the forward; a mirror that just sent
`Subscribe` is seeded from a sibling's snapshot through its own deliverer. Since A134
the same-origin H case is one mirror; the pins use two origins on one cell.

### A135
(b) the generated route runs its turn inside `rpc::under_connection(__request, ..)`
(an injected `context owner_scope` closure) → the session's new `ReactiveServer.owner`,
disposed by `dispose`/`drop_session`; no session → a fresh never-disposed owner (the old
ownerless behaviour). Covers `Service::new` and `factory` alike (the seam is the
request's session, not the factory). (c) warning at a handle method's tail `.cell()`
(tail, block tail, `Some(..)` payload) — `vilan_core::lifetime_steers`, a post-analysis
pass finding handle methods through the generated `reply_source*` calls. Ledger row
**610**.

### A136 + I7 + I8
memo.vl head + `get_or_insert` doc rewritten (context = caller's, lifetime = memo's;
the "covered by the scope" sentence gone). Warning for `.cell()` or any std `effect`
spelling called directly in a maker closure literal of `get_or_insert`/`get_or`
(trait-dispatched `effect` resolved by the NAMED member) — ledger row **611**. I7:
`get_or` kept as `[deprecated("use get_or_insert(key, make)")]`. I8: `Shared<Option<T>>::
get_or_insert(make)` in shared.vl (stayed S). Collections reference's Memo section
rewritten.

### A132
`DeltaSource` gains a provided `reader(self): |DeltaCursor| List<O>` (default captures
`self`; `ListCell`/`KeyedCell` override with log + items/elements-value only);
`map_each` drains through it; `ListCell::on_change` captures its items cell, not the
handle. `delta-law.vl`'s test-local `map_each` copy takes the same shape (its 14 were
there). Goldens `delta-law.mjs`, `list-cell.mjs` moved (program edit + renumbering; same
output, node before/after). Copy-elision census +2 NEW sites each (415 → 419:
`reader`'s one-time `DeltaLog` handle copy — not a regression); native copy census +2
MOVES each.

### A133 — STOPPED, needs the owner
Door (a) prototyped: `ReactiveServer` handles inbound control frames on the next
microtask (FIFO), so the seed lands after the subscribing call returns, exactly like a
socket; the reactive-42 repro prints `sees 0` once instead of `sees 7` twice. Measured
blast radius of that contract change: 15 `reactive_channels` pins, 2 `service_layer`,
1 `transport_robustness` (socket reconnect ordering), `vilan/examples/rpc`'s output,
and the in-process A25 record in `inference` (the `markdown.rs` A25 series asserts the
synchronous seed line by line). That rewrites the documented in-process contract
("the server's immediate current-value Update lands synchronously") and a long pin
record — beyond S–M and not a lane's call. Reverted. Options for the owner: (a) as
ruled with the pin rewrite budgeted (≈20 cli pins + the A25 series + the RemoteSource
doc), (a′) defer only the seed of a Subscribe that arrives inside the subscriber's own
`acquire` (same pin churn), (b) a node dedups a notify equal to its last pull, (c)
document. Stays OPEN.

## FINDS to file
1. **A137 residual:** a PER-KEY lease joining a sibling's held per-key forward gets no
   seed `Insert` (only whole-collection joins are seeded).
2. **Client route/replay growth:** every mint pushes a route and a replay closure into
   `ReactiveClient` for the connection's life, never pruned — bounded by origins ×
   lease cycles now (A134), but still unbounded over a long session.
3. **A135 behaviour note:** a `Service::new` handler that lazily builds a `.cell()` and
   CACHES it on a shared store now has it die with the first connection; the warning
   catches only the tail shape.
4. **Parser diagnostic:** a parameter named `own` (a keyword) is reported as
   "found '>' expected ','" at the PREVIOUS parameter's closing `>` (repro:
   `fun f(a: Shared<List<Foo>>, own: i32) {}`) — B414/syntax-43's area.
5. `docs/appendix/editor.md` (editor-owned) quotes `fun get_or(..)` as its hover
   example; still valid (the alias exists), update when the alias goes.

## remote-sources.md (proposals, read-only for this lane) — paragraph for the integrator
> **A stub inside a cold select is idempotent (A134).** A generated handle stub dedups
> its mirrors per ORIGIN — the method and its described arguments, per client — so a
> second call answers the same mirror, one call and one `Subscribe`, until the mirror's
> last lease closes. A cold node (`and_then`, `switch`) runs its select on every pull;
> before A134 a stub there minted a fresh mirror per pull and `.cell()` read a mirror
> nothing leased. **A `.cell()` inside a maker, a memo, a module-level cache or any
> structure that outlives the caller is `.cell_global()` or a bug** (A136): what the
> maker builds outlives the caller; a lease belongs at the call site. A mirror whose
> `Subscribe` joins a forward a sibling already holds is seeded from the sibling on the
> client (A137).

Ledger prose (proposals' diagnostics-ledger.md) owed for rows **610** (A135 steer,
`lifetime_steers.rs`, warning, pins `inference::lifetimes::a135_*`) and **611** (A136
steer, same file, pins `a136_*`).

## Kolt follow-ups (the owner's)
- After A134 the hand tables in `model.vl` can drop to a bare stub call in the select
  (`get_client().and_then(|c| c.get_x(id)).map(..).cell()`): `user_handles` (User::find),
  `channel_handles` (Channel::find), `channel_name_handles` (Channel::name),
  `channel_message_handles` (Channel::messages), `message_handles` (Message::find) —
  five `Memo`s — and `channel_list_handle` (the `Shared<Option<..>>` in
  `Channel::get_channels`). `Account::user`'s `.cell()` pin is optional too.
  (Keeping a memo keeps the paint-last-value-while-re-minting across visits.)
- I7's sites (deprecation warnings): `model.vl` lines 63, 99, 109, 126, 165 → `get_or_insert`;
  plus `store.vl:160` (`users.get_or`, server side).
- A135's warning fires at `store.vl` `get_channels` (195), `get_channel` (203),
  `get_message` (258): move each derived cell onto a service `Memo` whose maker writes
  `.cell_global()`.
- `channel_list_handle`'s by-hand `match read()` + `write() = Some(made)` is I8's
  `get_or_insert` if the owner keeps it.

## Gates (final, over the rebased tip 2c34918c)
All green, each run in the foreground over 2c34918c:
- `vilan fmt vilan/std` clean; `cargo fmt --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `-p vilan-cli`: corpus 12, split 13, examples 6, copy_elision_census 2, diagnostics_ledger 25, shared_census 2, reactive_channels 24 (+2 ignored, pre-existing), reactive_lifetimes 16, service_layer 59, rpc_http 9, transport_robustness 7, delta_log 7, ui_rows 59 — all pass.
- `native_differential` default 69/69 and `VILAN_NATIVE_DIFFERENTIAL=1` 69/69 (leak census: every row 0 live).
- `-p vilan-core --test inference` 4675 passed (2 ignored, pre-existing); `--test docs` 11; `cargo test --workspace --doc` green.
- Each item's commit was gated the same way before it was made (A134's gate list missed `service_layer`, whose twin-handles pin A134 moved; caught at A135's gate and folded INTO the A134 commit before the rebase).

## Expected conflicts at merge
- `CHANGELOG.md` `## Unreleased` (five entries appended after native-43's).
- `crates/vilan-cli/tests/diagnostics-ledger.tsv` (rows 610/611 at the end).
- `crates/vilan-cli/tests/shared_census.rs` (rpc.vl 47 → 52, total 141 → 146).
- `copy-elision-census.tsv` (total 419, delta-law 22, list-cell 45),
  `native-copy-census.tsv`, `native-leak-census.tsv` — regenerate over the merged tree.
- `vilan/std/src/reactive.vl` (tuples-43's `divorce`; mine: `AndThen` doc,
  `subscribe_pulling`), `rpc.vl` generator (anyone touching `[service]`),
  `crates/vilan-core/src/lib.rs` (one line + `pub mod lifetime_steers`),
  `inference/lifetimes.rs` / `std_surface.rs` (appends), `services.md`.
- Goldens moved: `vilan/test/delta-law.mjs`, `list-cell.mjs`, `reactive.mjs`,
  `reactive-on-change.mjs`, `tests/split/golden/app.js` — `regen_goldens.sh` over the
  merged tree if another lane moved them too.
- Count words in docs pages: none changed.

## LANE-STATUS (not committed, per the coordinator)
Worktree `.claude/worktrees/reactive-43`, branch `reactive-43` (started off 762c6aa5, rebased onto origin/next @49e0e817).

| item | state | sha | notes |
|---|---|---|---|
| A137 | LANDED | 61c9de90 | NOT clean over a socket: a Subscribe that JOINS a held forward gets no seed (plain: silent until next change; keyed: desynced). Fixed client-side (seed_from_sibling); pins in-process + socket. Per-key join seeding NOT covered (find) |
| A133 | STOPPED (owner) | — | door (a) prototyped (ReactiveServer handles control frames on the next microtask => the seed lands after the subscribing call, like a socket; A133 repro prints `sees 0` once). Blast radius measured: 15 reactive_channels pins, 2 service_layer, 1 transport_robustness (socket reconnect), examples/rpc output, plus the in-process A25 record in inference. Needs the owner to confirm rewriting the in-process synchronous-seed contract; reverted |
| A134 | LANDED | 7c76e11f | pins in-process + socket (reactive_channels a134_*); probe section A reads 1, 2 |
| A135 | LANDED | 394f6be7 | (b) under_connection + ReactiveServer.owner; (c) warning ledger 610; pins in-process + socket + inference |
| A136 + I7 + I8 | LANDED | 61ba6460 | doc rewrite, warning ledger 611, get_or deprecated alias, Shared<Option<T>>::get_or_insert |
| A132 | LANDED | 5979f0eb | DeltaSource::reader seam; leak census delta-law 14->0, list-cell 92->0 |

Notes:
- remote-sources.md lives only in proposals (read-only for this lane): the paragraph for it is in the report, for the integrator.
- Coordinator: after native-43 merges to next, rebase and fold subscribe_pulling to one statement (3-line commit); LANE-STATUS stays uncommitted.
- Rebased onto origin/next @49e0e817; the coordinator's subscribe_pulling fold landed as 2c34918c.
