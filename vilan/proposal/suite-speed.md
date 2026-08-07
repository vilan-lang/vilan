# Suite speed — the measured profile (E21)

> **Status: AUDIT DONE 2026-08-02; E26 measured and CLOSED NEGATIVE the same
> day (§2.1) — its outcome corrects the inference attribution below.** Every
> number was measured on the dev machine (16 cores, WSL2, warm tree,
> v0.22.0-era `next`); the levers are filed as backlog E25–E30, each its own
> suite-gated slice. The constraint from E21's charter is restated because it
> binds every slice: **no gate weakens** — no pins dropped, no cases sampled,
> no goldens loosened; anything that changes what is *tested* is out of scope
> by definition.

## 1. Where the time goes

One full `cargo test --workspace` on a warm tree, every line timestamped:

| phase                              | wall     |
|------------------------------------|----------|
| compile check (nothing to build)   | 0.1 s    |
| 51 result sets, run **serially**   | 130.7 s  |
| inter-binary gaps (startup)        | 0.1 s    |
| **total**                          | **131.3 s** |

Separately, the **edit tax**: `touch crates/vilan-core/src/lib.rs` then
`cargo test --workspace --no-run` costs **16.0 s** of rebuild+relink before
any test runs — at 490 % CPU of the 1600 % available, so the link jobs are
nowhere near saturating the machine. A no-op check is 0.1 s. The per-arc
suite cost is therefore ≈ **2.5 minutes**: 16 s of relinking plus 131 s of
strictly serial test execution.

> **CORRECTED by E29's measurement (2026-08-02, §2):** the 16.0 s figure
> does not reproduce — the same probe measures **~3–4 s** (49 dirty units;
> vilan-core's no-op incremental recompile ~1.7 s is the critical path,
> each binary links in well under a second). The audit's number most
> likely conflated real recompile work or stale incremental caches with
> the relink itself. And the "link jobs" framing missed that the links
> were already fast: **rust-lld has been the default linker on
> x86_64-linux since Rust 1.90** — the toolchain this machine ran
> throughout, audit included.

The serial 130.7 s decomposes (per-binary `finished in`, top of 51 sets):

| seconds | tests | binary                     | what dominates it (verified in source) |
|---------|-------|----------------------------|----------------------------------------|
| 29.9    | 241   | vilan-lsp unit tests       | ~81 fixture sites each running a real `Document::analyze` against on-disk std (~150 ms apiece) |
| 18.7    | 1205  | tests/inference.rs         | ~1400 full-pipeline `compile()` calls burning ~276 CPU-seconds across all 16 cores — **compile-bound**, at ~90 % parallel efficiency already. (First attributed to its 534 node spawns; §2.1's measurement corrected that.) |
| 16.7    | 8     | tests/docs.rs              | every book fence compiled **serially** inside one test (the audit's "also spawns node" was wrong — the gate only compiles). E27 parallelized it: → 3.7 s |
| 14.9    | 9     | tests/interpreter.rs       | the equivalence sweep runs each admitted corpus program **serially** (in-process compile + node run + interpreter; no CLI spawns, another audit slip). E27 parallelized it: → 2.8 s |
| 8.1     | 2     | tests/examples.rs          | 9 examples staged via `git ls-files` and built through the debug binary |
| 5.4     | 6     | tests/corpus.rs            | already 8-way parallel (`thread::scope`, chunked) — the shape the others should copy |
| 4.7/4.2/2.8 | — | hmr / rpc_http / transport | e2e legs: ports (post-E19 they bind port 0) + real servers |
| ~26     | —     | the other 40 sets combined | long tail, none above 2.6 s |

43 integration-test files build 43 binaries, each linking the full crate
stack — that is what the 16 s edit tax buys, every arc, before a single test
runs.

## 2. The levers (filed as E25–E30)

Ordered by measured payoff; estimates assume the others have not landed.

- **E25 — run the binaries in parallel — SHIPPED 2026-08-02** via
  cargo-nextest (0.9.140; local install from get.nexte.st, CI via
  taiki-e/install-action). Measured on the post-E27 tree: 112 s →
  **63.5 s** (82.2 s unconfigured; the committed `.config/nextest.toml`
  turns fail-fast off and priority-starts the Linux leak plateaus, whose
  longest test — 32 s idle, ~52 s under full load — is the critical path
  when scheduled last). From the audit baseline: 131 s → 63.5 s. Parity
  exact: 2270 run + 1 skipped = cargo test's 2271 enumeration, all three
  doc-test sets empty (CI keeps a `cargo test --workspace --doc` leg so a
  future doc-test cannot silently stop running). Three full runs green,
  zero flakes — the stdout-parsing e2e legs and node-storm risks did not
  bite. Plant-proven red (exit 100, named FAIL with full report). The cost
  worth recording: user CPU rises ~70 % (530 s → ~915 s) — the per-test
  process tax under WSL2 exec plus cache contention at full interleave;
  wall is what the suite gates on, but this is why the floor is ~57 s
  (915/16), not 33 s. CLAUDE.md's suite section now names nextest as the
  gate; `cargo test --workspace --no-fail-fast` remains a correct, slower
  equivalent; release.yml's tag-time gate stays on plain cargo test,
  unchanged. CI outcome (run 30771883241, both legs green): ubuntu
  6m37s → 6m13s, windows 8m21s → 10m02s — the per-test process tax is
  steepest on Windows CreateProcess, and that leg is now CI's long pole.
  Accepted for instrument parity; reverting the windows leg to cargo test
  is a one-line change if the price stops being worth it.
- **E26 — batch inference's node runs — CLOSED NEGATIVE, see §2.1**: the
  filed premise (534 spawns × ~35 ms IS the 18.7 s) was arithmetic derived
  from the wall, not an independent measurement. Built, measured, and
  withdrawn 2026-08-02: removing every spawn moved the wall from 19.39 s to
  19.20 s — noise. The binary is compile-bound (§2.1); the real lever is
  E30.
- **E27 — parallelize the docs gate and interpreter cases — SHIPPED
  2026-08-02**: both were serial loops over independent compiles (verified
  by the E26-lesson user-time check first: docs 16.6 s user at 99 % CPU,
  interpreter 14.8 s at 104 % — genuinely single-threaded, real headroom).
  corpus.rs's 8-way `thread::scope` chunk shape, applied verbatim; chunks
  preserve item order and workers join in spawn order, so failure reports
  read identically to the serial loops'. Measured: docs 16.78 s → 3.73 s
  (531 % CPU), interpreter 15.09 s → 2.78 s (674 % CPU) — 25.4 s off the
  serial floor. Both gates plant-proven red under parallelism (a broken
  README fence; db.vl unexcluded), each failure named and attributable.
- **E28 — share the LSP's analyzed fixtures**: the unit binary's 29.9 s is
  ~81 `Document::analyze` fixture sites; a `OnceLock`-shared analysis per
  distinct SOURCE keeps every assertion while paying each analysis once.
  Est: 29.9 s → ~10 s. (Counter-interacts with E25's per-test processes —
  decide the runner first; under nextest this lever mostly evaporates.)
- **E29 — cut the edit tax — CLOSED 2026-08-02, overtaken by events**:
  evidence-first, as filed, and the evidence dissolved the item. (1) The
  16 s tax does not reproduce: the identical probe measures ~3–4 s wall /
  ~20 CPU-s (49 dirty units, `--timings`-verified; vilan-core's ~1.7 s
  no-op incremental recompile is the critical path, links well under a
  second each). (2) The "no fast linker installed" premise was stale at
  filing: `readelf -p .comment` on any test binary says `Linker: LLD
  20.1.8` — **rust-lld is rustc's default on x86_64-linux since 1.90**,
  so the linker win was banked before the audit measured. (3) mold was
  installed and probed anyway; a first pass appeared to save ~1 s until
  the .comment check showed the flag never took effect (rustc's
  self-contained lld won the link line) — the "saving" was
  rebuild-freshness, a measurement trap worth remembering. mold-over-lld's
  real ceiling here is a fraction of a second per cycle: not worth a
  toolchain dependency. The consolidation sub-lever was already dead
  under E25's nextest. Residual truth: the per-arc edit tax is ~3 s and
  it is recompile-bound, not link-bound.

- **E30 — inference's repeated std analysis** (filed by E26's measurement,
  §2.1): a single ~10-line `assert_compiles` case costs ~170 ms of
  single-threaded pipeline, so the binary's ~1400 `compile()` calls burn the
  ~276 CPU-seconds that ARE its wall — the program under test is ten lines;
  the cost must be dominated by re-resolving and re-analyzing std from disk
  per call. The lever is sharing the analyzed std across compiles (each case
  still runs its own program through the real pipeline; only the std prefix
  is shared) — the same shape as E28, and it carries the same nextest
  interaction: in-process sharing evaporates under per-test processes, and
  only a disk-cached std snapshot would survive both, which is an analyzer
  arc, not a test tweak. Profile first (how much of the 170 ms IS std),
  and decide together with E25/E28.

## 2.1 E26's measurement (2026-08-02): the negative result, recorded

The batching was fully built before it was judged: a persistent `node`
child shared by the whole binary, each program run in a `worker_threads`
worker — a fresh isolate whose event loop drains before exit — with
stdio-multiplexed framing, probe-validated byte-identical against
standalone `node file.js` on every shape the suite asserts (ESM entries,
thrown strings, exit codes, zero-exit stderr, timer-scheduled output,
concurrent isolation; worker startup amortizes to ~2.6 ms at width 16).
All 1205 cases plus 8 runner pins passed. And the wall did not move:

| variant                      | harness wall | user CPU |
|------------------------------|--------------|----------|
| per-program spawns (baseline)| 19.39 s      | 291 s    |
| persistent runner            | 19.20 s      | 276 s    |

The spawns were never the bound. The binary burns ~276 CPU-seconds of
analyzer work across 16 threads (floor 276/16 ≈ 17.3 s; it runs at ~90 %
parallel efficiency), and the node runs always overlapped that, costing
only ~15 CPU-seconds aggregate. The runner would also *regress* under
E25's per-test processes — runner startup per process exceeds one plain
spawn — so it has negative expected value and was withdrawn unlanded.

Two corrections propagate:
- **E25's ceiling is total CPU ÷ 16, not the longest binary**: inference
  already saturates the machine while it runs, so overlapping other
  binaries with it buys little during those seconds. Re-estimate from
  summed per-binary user time before promising ~4.4×.
- **E28 must start with the same user-time check** that caught this —
  attribute a binary's wall to a mechanism only after the CPU accounting
  confirms it (blocked-time arithmetic that happens to match the wall is
  not attribution).

Sequencing after the correction: **E27 → E25 → (E28/E30/E29 as E25's
outcome dictates)**. Both shipped 2026-08-02: E27 −25.4 s, then E25 landing
the suite at **63.5 s** (2.1× from the audit's 131 s).

E25's outcome settles the dependents:
- **E28 and E30 as filed are foreclosed**: nextest's per-test processes
  cannot share an in-process `OnceLock` analysis. Either lever now means a
  disk-cached std/fixture snapshot (an analyzer arc, not a test tweak) or
  cutting the per-analysis cost itself — and the payoff shrank: the leak
  plateaus and the CPU floor (~915 s ÷ 16), not fixture repetition, now
  bound the wall. **RESOLVED 2026-08-02**: both measured to the same root —
  a fixed ~115 ms per-analysis std tax — and folded into the std-tax arc,
  `analysis-reuse.md` §6 (E3 Phase 3 reopened; `VILAN_PHASE_TIMING`
  shipped as its S0).
- **E29's consolidation sub-lever is dead** (nextest prefers many
  binaries); the faster-linker sub-lever looked up-weighted here, but
  E29's own measurement then closed the item entirely — the tax is ~3 s
  and lld was already the linker (see the E29 entry above).

## 3. What was NOT found

- No mystery time: gaps, harness startup and the warm compile check are all
  ≈ 0.1 s. The suite spends its time exactly where the tests say it does.
- No already-slow-by-accident binary: corpus is parallel, the e2e legs are
  bounded by real servers doing real work, and the long tail (40 sets) sums
  to ~26 s with nothing above 2.6 s.
- The Linux leak harness (`leak_measurement`, 200-analysis loops) hides
  inside the vilan-lsp unit binary's 29.9 s rather than standing alone —
  E28's fixture work should measure it separately before touching it.

## 4. Post-cache re-measurement (2026-08-03, v0.23.2 tree)

The std-tax arc landed under the E27 gates; re-measured warm, second
samples:

| gate                | E27-era | post-cache | Δ    |
|---------------------|---------|------------|------|
| docs                | 3.73 s  | 2.95 s     | −21 % |
| interpreter         | 2.78 s  | 2.33 s     | −16 % |
| inference           | 19.38 s | 12.83 s    | −34 % |

CPU tells the same story (inference 277 → 176 CPU-s). The residual in
docs/interpreter is node/exec cost and per-program compile work with
DISTINCT import sets — the cache amortizes only repeated worlds, and both
binaries were already 8-way parallel. A single-shot CLI process is
unchanged (~140 ms; the cache is per-process, so a lone `vilan check` is
one miss + store) — the wins live where analyses repeat: the LSP session,
the playground instance, the watch loop, and the in-process test
harnesses. The suite wall now runs ~74 s against the 63.5 s E25-era
measurement — the difference is six new suite-gate files the arc added
(the differential, idempotence, and base-cache gates, two of which copy
whole std trees), not a regression in the measured binaries.

## 5. Two load-dependent flakes closed: harness clocks and a fixed port (E32/E33, 2026-08-04)

Both were suite-*discipline* bugs, not suite-*speed* ones — no lever here
made anything faster, but both were exactly the kind of load-dependent
flake this file exists to keep out of the gate, so the record belongs
here.

**E32 — the cancellation family's timing budget measured the wrong
thing.** Four inference.rs tests (three of them, in fact — see below)
wrapped `started.elapsed() < 4s` around a full compile-then-run: the
in-process `std` re-analysis `compile()` does on every call is not a
`cargo build`, but under nextest's full parallelism it can itself run to
several seconds (the same CPU-bound cost this whole file has been
chasing), leaving near-zero budget for the actual node execution the
claim is about. Fix: split `compile_and_run` into `compile()` + a new
`run_js()`, and time only the run (`compile_and_run_timed`,
`assert_runs_within`) — compile happens first, untimed. The 4 s budget
itself is unchanged; it now measures the emitted program's own
cancellation-reaction latency instead of harness overhead, which is what
makes it load-immune: a slow *machine* no longer competes with the
budget, only a slow *reaction* does. `nested_nurseries_join_inside_out`,
the fourth test the backlog entry named, carries no wall-clock assertion
in this tree — backlog drift, left untouched. Proven non-vacuous by
inflating each restructured test's reaction latency past the budget
(still under the guarded sibling's own timer, so the string assertions
stayed green) and watching the elapsed check alone go red, then
restoring.

**E33 — the benchmarks e2e bound four fixed ports.**
`vilan/benchmarks`'s `throughput.vl` (three servers: http-json,
http-binary, ws-multiplex) and `realtime.vl` (one fan-out server) bound
literal ports 48231–48234, colliding with anything else — including a
second concurrent `cargo nextest run --workspace` — already holding one.
Migrated all four to port 0 (the E19 `Server.port()` precedent already in
use for `rpc_http`/`ssr`/`http_port`); each bind announces `[port] <n>`,
and the Rust e2e test reads all four back, asserting they're real,
distinct, and that exactly four showed up. Verified non-vacuous by
holding 48231–48234 with an out-of-process listener: the fixed test
passed regardless, and the pre-fix `.vl` sources reproduced the exact
target failure (`Error: listen EADDRINUSE: address already in use
:::48231`) against the same held ports.

**Stress validation.** Both fixes were run against the shape of load that
had actually produced the flakes: two full `cargo nextest run --workspace`
runs started concurrently (2439 tests each, 16-core machine — the
closest single-machine approximation of "two overlapping suite legs"),
while the four cancellation tests and the benchmarks e2e were looped 5×
each on the side. Result: 20/20 cancellation runs green, 5/5 benchmarks
runs green, and both full-workspace runs finished clean (2439 passed, 0
failed, 2 skipped each) — including `benchmarks_run_and_report_the_deterministic_counts`
passing in *both* concurrent runs, which is the E33 collision scenario
occurring for real and not colliding. Zero flakes.

## 6. The hmr swap e2e's budgets were compile budgets (E39, 2026-08-06)

`hmr_swap`'s `the_swap_protocol_carries_state_across_a_rebuilt_bundle`
failed in two heavily-loaded full-suite runs and passed 5/5 in isolation —
E32's disease, in the one shape E32's cure does not fit.

**Where the clock was.** Every wait in the test was a literal
`Duration::from_secs(20)`, and one of them wrapped `run --watch`'s FIRST
ROUND: a full compile of both legs (a browser bundle over `std::ui` plus a
node server) before `dist/client.js` can exist. On a contended box that
alone runs past 20 s. Reproduced directly, not inferred: with the tree at
load average ~27 on 16 cores, the test failed at exactly
`"round 1 should have written dist/client.js"` — in ISOLATION, no other
copy of itself in sight. The assertion was about the swap protocol and
said nothing about speed; the number was a performance assertion nobody
wrote on purpose. A second 20 s window covered the edit's rebuild, and a
5 s socket read timeout sat inside both.

**Why E32's cure does not transfer, and what replaces it.** E32 could move
the compile out of the timed window because the claim was about the emitted
program. Here the claim IS about the watcher compiling, so the compile
stays inside. Two substitutions instead:

- **A liveness bound where the compile is** (`support::WATCH_LIVENESS`,
  300 s). Nothing in the family asserts how fast a round is, so the number
  only has to be too large for a healthy round and finite for a hung one.
  `watch_lifecycle.rs` had already reached this conclusion in a comment —
  "how long that takes is not this test's business" — at 60 s. The value is
  measured, not felt: one ordinary `vilan build` of this test's own two-leg
  project — the identical work round 1 does — costs **9.3 s** wall on an idle
  16-core box and **34 s** on the same box at load average ~38. The old 20 s
  was barely twice the idle cost, which is why load alone could exhaust it.
  120 s was tried next and is ~3.5× the loaded cost, which a box running five
  overlapping suites consumed outright (see the stress note below); 300 s is
  ~9× loaded, ~32× idle.
- **A calibrated budget for everything after it** (`support::round_budget`).
  Round 1's cost is now MEASURED, and every later wait is `4 ×` it (floored
  at 20 s, capped at the liveness bound). Round 1 is this machine's own
  price, right now, under whatever load it is under, for compiling this
  project; a rebuild of ONE leg taking several times that is a stuck
  watcher, while the same rebuild on a four-times-slower machine is not.
  That is E32's rule — the budget measures the program, not the machine —
  reached by calibration rather than by excision.

**Two smaller things fell out.** The fixed `sleep(800 ms)` "so the watcher's
baseline snapshot is taken before the edit" was paying for a bug E20 already
fixed: the snapshot is taken BEFORE the first build now, never after. It is
replaced by an event wait on the server leg's boot line (the Node child
inherits the watcher's stdout — the channel `hmr.rs` already uses for the
same purpose), which is both correct and the thing the margin was
approximating. And `http_get` ignored the result of `read_to_end`, so a read
cut short by the 5 s timeout returned a PARTIAL body — which, in a loop whose
exit condition is "the bundle differs from A", is indistinguishable from a
finished rebuild. It now returns `None` on an incomplete read and the poll
treats that as "not yet"; its timeout is the calibrated budget.

**The family.** `hmr.rs`'s six e2e bodies carried the identical literal
20 s deadline; all six now take `WATCH_LIVENESS` — a pure failure-bound
change, no behaviour moved. `hmr_overlay.rs` runs no compiler round and has
no clock at all. `watch_lifecycle.rs` was already right. What is left,
recorded and NOT changed: `hmr.rs` still has `sleep(500/800 ms)` margins of
the same vestigial kind (they are cheap to delete but sit inside sequences
this lane did not otherwise touch), and its NEGATIVE windows
(`sse.assert_no(..., 2 s)`, `!buffer_has(..., 700 ms)`) are a different
failure mode — under load they go vacuously green rather than red, which is
a weakened assertion, not a flake, and wants its own item.

**Stress validation.** Before the fix, the test failed at round 1 under a
load average of 27 with no second copy of itself in sight. After it, it
passed at 43 s wall under load average 46 — the calibrated budget growing
with the machine because it was measured on it — and then passed **in both**
of two full `cargo nextest run --workspace` runs started concurrently on the
same 16-core box (49.6 s and 51.2 s wall, load average ~32), which is the
shape that produced the original failures. That is the E32 bar, met.

**What the same runs also showed, and it is not a `hmr_swap` fact.** Neither
concurrent run finished clean, and every failure in both was a *different*
test dying on its own fixed clock, with a message that names the shape
outright: `owned_nursery`'s two legs at exactly 20.0 s ("vilan run did not
exit within 20s"), four `rpc_http` legs at exactly 60.1 s ("vilan run did
not exit within 60s"), `cancellation`'s fetch leg, and one `split` leg. All
21 of those tests pass in isolation on the same tree, and each budget wraps
a `vilan run` — i.e. a COMPILE — which is E32's original diagnosis
word-for-word, in files E32 did not touch. An earlier attempt at this
validation on a box that had drifted to five or six overlapping suites (load
~77, ~5× the cores) showed the same list plus `streaming`, `benchmarks`, two
more `split` legs and `nested_nurseries_join_inside_out` — and `hmr_swap`
itself at the then-120 s bound, which is what raised it to 300 s. The
pattern is a backlog item of its own: the E32 treatment is unfinished in
roughly a dozen places, and it wants one pass, not per-test patches.

## 7. The rest of the family's compile budgets (E40, 2026-08-07)

§6's closing paragraph filed the pattern; this is the pass. The v0.32.0 CI
run made it urgent rather than tidy: it failed on BOTH the ubuntu and the
windows leg, with the same two tests, each at exactly its own ceiling —
`cancellation`'s `cancel_aborts_an_in_flight_fetch` at 45 s and
`benchmarks`'s `benchmarks_run_and_report_the_deterministic_counts` at 90 s
— while 3044 of the other 3046 tests passed. Shared 4-core runners take
~54 min for the suite a 16-core dev box runs in ~9; under that interleave a
fixed wall clock wrapped around a `vilan run` is a bet on the runner.

**The unit, and why a cheap one is honest.** E39 could budget in units of
the round the test itself had just paid for. Nothing in this family has a
round 1, so the unit is measured separately: `support::reference_compile()`
is one `vilan build` of a `std::print`-only project, run once per test,
lazily. That probe is cheap (~0.23 s) rather than representative, and the
measurement that justifies it is the one worth keeping:

| project                             | idle    | load average ~28 |
|-------------------------------------|---------|------------------|
| the reference (`std::print` only)   | 227 ms  | 490 ms           |
| a node app importing `std::time`    | 6.07 s  | 11.4 s           |
| `vilan/benchmarks` (the heaviest)   | 13.4 s  | 24.5 s           |

Contention costs all three the same ~2×, across a 60× range of compile
weight — so the cheap probe measures the *machine* just as well as an
expensive one, and the suite does not pay a real compile per test to find
out. (`std::time` is the whole reason the range is that wide: importing it
alone costs 6 s where `std::http`, `std::fetch` and `std::task` each cost
~0.25 s. That is E43's cliff showing up in a second module; not chased
here.) In reference units the heaviest member of the family is 59× idle /
51× loaded, so `run_liveness()` is 240 units — E39's 4× over it — clamped
to [60 s, `WATCH_LIVENESS`]. A probe that fails or hangs yields the
CEILING, never a small number: a broken measurement must not be able to
manufacture a tight budget.

**Eleven watchdogs, and one real assertion.** `benchmarks` (90 s),
`rpc_http` (60 s × 7), `owned_nursery` (20 s × 2) and `streaming` (45 s)
all take the liveness bound: none of them claims a speed, and each one's
actual claim is pinned by output that stays true however long the box
takes — `benchmarks` says so in its own header, and `owned_nursery`'s
"a drop never cancelled" is caught by `task-finished` appearing, not by a
clock. Three watch-round deadlines the filing had missed join
`WATCH_LIVENESS` for the same reason E39 moved `hmr.rs`'s six: `split`'s
120 s, `assets`'s 120 s (whose comment already recorded 20 s losing this
race three times in two days) and `watch_lifecycle`'s 60 s — the comment
E39 quoted for reaching the right conclusion first, still carrying the
wrong number.

`cancellation` is the one member with a genuine claim about time, and it
had TWO clocks on the same window: the 45 s watchdog and a *tighter* 30 s
`started.elapsed()` that also began at `vilan run`'s spawn — a compile
budget spelled as a latency assertion, and therefore the first thing a
loaded box breaks. E32 excised the compile with a function boundary;
here there is a process boundary instead, so the program supplies a marker:
`on_start` prints `server-up`, the harness timestamps stdout as it arrives,
and the assertion runs from that marker to `aborted-fast`. The budget is
20 s, taken from the program's own scale — the server answers at 60 s, the
client cancels at 150 ms — and it was proven non-vacuous by planting a 25 s
stall inside the program (the string assertions stay green, so only this pin
can fire): red at 25.16 s of a 32.1 s run, which also measures the ~7 s of
compile the window no longer contains.

**Stress validation.** Two full `cargo nextest run --workspace` runs started
concurrently on a 16-core box, the shape that produced E39's failure list:
`3046 tests run: 3046 passed (21 slow), 6 skipped` in 1040.5 s and
`3046 tests run: 3046 passed (23 slow), 6 skipped` in 1038.7 s, both exit 0.
Every member of the family green in both, where E39's equivalent runs had
neither finishing clean. The margins are the interesting part:
`rpc_http`'s disconnect leg took 146.8 s and 139.1 s against the 60 s it
used to be given, `split`'s watch round 80 s against 120 s, and
`benchmarks` 67-70 s against the 90 s that CI had already exceeded.

**Left standing, on purpose.** Fixed clocks that wrap only a *node boot* of
an already-built bundle — `rpc_http`'s 60 s ready line, `split`'s 30 s port
wait, and `http_port`, `init`, `ssr_fullstack` and `transport_robustness`
outside the family — are not this bug: their compile is already outside the
window (the `vilan build`-then-`node` shape), and every one of them survived
both concurrent suites. They are worth converting if CI ever flags one, and
worth nothing before that. E41's vacuously-green negative windows in
`hmr.rs` are untouched.

## 8. The import-cost cliff is one mechanism: `std::set`, reached through every macro world (E43, 2026-08-07)

E43 filed two data points — `std::set` and `std::time` each ~6 s to import
where `std::print`/`std::http`/`std::fetch`/`std::task` cost ~0.25 s — and
guessed they were one mechanism. They are. This is the measurement-first
pass that found it, and the fix it points at is not contained, so the
diagnosis is recorded here rather than attempted.

**The instrument.** One `vilan check` per std module on a three-line program
importing exactly that module, each in a FRESH PROCESS — the base cache
(S3c) is process-global, so a new process is a cold cache, the out-of-process
equivalent of the `base_cache_clear()` the B77 pins use. `VILAN_PHASE_TIMING=1`
splits each analysis into `load+walk` / `base` (the pre-entry
`resolve_world()`) / `build` / `checks`. Best of two reps, debug binary,
quiet box. The instrument was re-verified working (E35 had fixed it
panicking warm analyses): every run produced a program and `no errors`.

One further column is doing the real work: **macro worlds**. A macro world is
a nested `analyze_source`, and its phase line is suppressed — but its
`post-passes` line is not, so counting those lines counts the nested
analyses. The CLI calls `analyze` directly, so on this path every
`post-passes` line is one macro world.

| module (44 measured)  | cold ms | load+walk | base   | build  | checks | macro worlds |
|-----------------------|---------|-----------|--------|--------|--------|--------------|
| `ui` (node layer)     |    9830 |    9421.9 |  169.4 |   10.2 |  151.0 |            3 |
| `rpc_server`          |    9795 |    9249.0 |  240.6 |    9.4 |  155.5 |            3 |
| `rpc`                 |    9707 |    9224.3 |  200.3 |    7.9 |  169.5 |            3 |
| `router` (browser)    |    9699 |    9287.9 |  180.3 |    9.0 |  138.3 |            3 |
| `reactive`            |    9471 |    9193.2 |  113.4 |    5.4 |   98.9 |            3 |
| `ui` (browser layer)  |    9406 |    9021.8 |  164.9 |    8.8 |  131.6 |            3 |
| `time`                |    6467 |    6231.4 |   95.9 |    4.6 |   84.8 |            2 |
| `arena`               |    3473 |    3303.3 |   83.5 |    4.1 |   52.3 |            1 |
| `set`                 |    2494 |      92.5 | 1095.8 | 1063.3 |  212.8 |            0 |
| `crypto`              |     522 |     171.6 |  201.3 |    7.1 |   92.6 |            0 |
| `dev` (browser)       |     424 |     119.0 |  144.2 |   10.8 |   99.7 |            0 |
| `db`                  |     391 |     103.8 |  121.2 |    6.7 |  120.9 |            0 |
| the other 32 modules  | 233–326 |      ~90  |   ~85  |    ~4  |   ~55  |            0 |

The macro-world column predicts the cliff exactly: ~3.1 s each, and nothing
else in the table moves. `time` is not a heavy module — it reaches two
macro-defining files. Worlds are cached per macro-DEFINING FILE, which is
why the count is not the number of `[derive(..)]`/`[extern(..)]` sites
(`rpc` has 18 sites and 3 worlds).

**Two shapes, one hot ingredient.** Un-suppressing the nested phase line
shows a macro world spends its time where `std::set` does — not in loading
or walking, but in constraint solving, and it pays the fixpoint twice, once
in `resolve_world()` and again in the post-entry `build()`:

| analysis                       | load+walk | base   | build  | checks |
|--------------------------------|-----------|--------|--------|--------|
| the macro world of `[derive(Debug)]` |    64.4 | 3193.5 | 2922.0 |  503.7 |
| `reactive`'s three worlds      | 25–51     | 1269–2086 | 1383–1497 | 235–263 |
| `import std::set` (no macros)  |      92.5 | 1095.8 | 1063.3 |  212.8 |

The reason both shapes are the same shape: **`macro_std/src/lib.vl` does
`export import std::set;`**. Every macro world's workspace is `[macro_std]`,
so every macro world analyzes `std::set` — and macro worlds can never reuse
the base cache, because `base_cacheable` requires `workspace.packages`
to be empty. Each one pays `std::set` from scratch.

**Proven by ablation**, on CPU seconds rather than wall (the box was loaded;
CPU time is the load-robust metric — three reps, tight):

| macro_std variant, compiling one `[derive(Debug)]` | CPU s        |
|----------------------------------------------------|--------------|
| stock                                               | 5.18 / 5.26 / 5.19 |
| without `export import std::set`                    | 0.93 / 0.93 / 0.78 |
| without `std::set` AND `std::map`                   | 0.85 / 0.78 / 0.80 |

`std::set` alone is the whole cliff; `std::map` is innocent (it imports for
262 ms on its own, against `set`'s 2494 ms, at the same 111 lines).

**This is not only a suite cost.** The same measurement says a plain user
program whose only unusual feature is `[derive(Debug)]` pays ~3.4 s wall /
5.2 s CPU to compile, and a second derive resolving to a second defining
file pays ~6.3 s. That is a compiler-UX defect, not a test-harness one.

**Why nothing is fixed here.** The contained-looking fix — dropping
`std::set` from `macro_std`'s re-exports — treats a consumer, not the cause,
and changes a published surface macro authors may bind. The cause is that
`std::set` costs ~2.2 s of constraint solving where every other std module
costs ~0.09 s, and that lives in the solver's handling of its `Hashable`
bound graph, not in any one line of `set.vl`. It was NOT the `impl List<type
T: Hashable>` tail (removing it changes nothing measurable), so localizing it
wants a profiler run on a quiet box. Two structural facts belong with it
whenever it is taken up: the fixpoint runs twice over the same constraint set
(`resolve_world()` then `build()`), and macro worlds are excluded from the
base cache by a blanket `workspace.packages.is_empty()` test that a
macro-world workspace can never satisfy even though every macro world in a
process shares the same `macro_std`.
