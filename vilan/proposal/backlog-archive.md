# Vilan Backlog — the archive

Append-only. When an item in the live tracker
([`backlog-2026-08-18.md`](backlog-2026-08-18.md)) ships or closes, the
same sweep that closes it moves its tombstone paragraph here — dated,
under a dated heading — and retires the number. The live tracker never
accumulates history; this file never holds open work.

The eras before this file:

- **Alpha capture, through 2026-07-18**: [`backlog.md`](backlog.md) —
  every shipped item's full body, context, and lessons (including the
  bodies relocated there by the 2026-08-03 restructure).
- **Cycles 15–19, 2026-07-18 → 2026-08-18**:
  [`backlog-2026-07-18.md`](backlog-2026-07-18.md) — frozen at the
  2026-08-18 re-baseline with its tombstones in place, including the
  E49/E56 charters' full arcs and the v0.33.0/v0.34.0 trains.

---

## 2026-08-18

- **L1. Ratify beta.md — CLOSED 2026-08-18** (ratified as recommended
  the same day it was drafted: Q1 the clean-train count starts at
  v0.35.0; Q2 B73 blocks trigger (c) — beta-critical; Q3 message-head
  identity, no numeric codes; Q4 reactive/ui Tier 2, canvas Tier 3; Q5
  the annex ratified. The owner's follow-up "should we defer beta?" is
  answered in the status block: the ratified trigger already defers the
  declaration; pre-switch work proceeds as low-regret hygiene. Record:
  beta.md status block / process.md §5.)

- **M3. `DevChannel.clients` accumulates dead SSE streams (S; found by the survey)** — SHIPPED 2026-08-18. The HMR dev channel pushed every SSE connection into
`clients: Arc<Mutex<Vec<TcpStream>>>` unconditionally and pruned
only as a side effect of a broadcast's write failure — a long dev
session with many reconnects and sparse rebuilds accumulated dead
streams unboundedly. Confirmed at one leaked fd per disconnect (100
cycles → 100 fds, no rebuild involved), with two corrections to the
survey's write-up: the per-connection **threads never leaked** (the
handler returned after registering the socket), and a rebuild did
**not** reap — a write to a cleanly closed peer succeeds, so a dead
client survived its first broadcast and left only on the second.
Fixed with the read-side liveness check: the connection's own thread
now stays on it, blocked reading, and unregisters on end-of-stream;
the broadcast prune stays as a backstop. After: 100 cycles, zero fd
and zero thread growth. Pinned in `hmr.rs`'s unit tests (the
registry's size is not observable on the wire, and `vilan-cli` is
bin-only). M2's tier-2 soak remains the long-horizon validation. (Record: hmr.md appendix; merged 931171dc.)
