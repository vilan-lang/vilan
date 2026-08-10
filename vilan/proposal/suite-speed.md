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

## 8.1 It was not the `Hashable` bound graph: the fixpoint could not stop (E43, 2026-08-07)

§8 left the cause as "`std::set` costs ~2.2 s of constraint solving … in the
solver's handling of its `Hashable` bound graph". That guess was wrong, and
the profile says so plainly. Nothing about `set`'s constraints is expensive.
The solving loop simply never stopped running them.

**The instrument.** The box was at load 30–38 (another agent's suite), so
wall and CPU are both unusable and every number here is a **callgrind
instruction count** — deterministic, and identical under any load. Debug
binary, one `vilan check` per program in a fresh process. Ir tracks wall
faithfully on the quiet baseline: `set` measured 9.36x `map` in Ir against
the 9.5x §8 measured in milliseconds.

**Where the 2.2 s went.** `import std::set` costs 23.4e9 Ir against
`import std::map`'s 2.49e9 at the same 111 lines. The differential, by
inclusive cost, is not spread around — it is two constraint kinds:

| inclusive Ir                | set    | map    | ratio  |
|-----------------------------|--------|--------|--------|
| `resolve_for_each_item`     | 8.11e9 | 2.4e6  | 3431x  |
| `resolve_method_arg_check`  | 8.22e9 | 8.9e6  |  922x  |
| `resolve_constraints`       | 16.6e9 | 67.7e6 |  245x  |
| `infer_type_path`           | 10.3e9 | 108e6  |   95x  |

Those two kinds are 70% of the whole analysis. But their *per-call* cost is
ordinary — what differs is how often they run. `resolve_for_each_item` is
called **115,433 times for `set` and 24 times for `map`**, over five `for…in`
loops against `map`'s four. The solver was re-running a handful of
constraints tens of thousands of times.

**Why.** Instrumenting the fixpoint's own loop gave the mechanism directly:

```
set  base : iterations=14022  max=14022  backstops=14012  deferred_left=10
set  build: iterations=14842  max=14842  backstops=14840  deferred_left=10
map  base : iterations=17     max=14044  backstops=5      deferred_left=0
```

`std::set` leaves **ten constraints permanently deferred** — legitimately
unresolvable, and `finalize_build` commits them to defaults. The loop is
supposed to notice it has settled. Instead it ran to `max_iterations`
(`2 * entity count + 16`), every pass re-running those ten through full
recursive `infer_type`. That is the entire cliff. `map` leaves nothing
deferred, so it never entered the spin — which is the whole of why the two
files differed. `[derive(Debug)]` showed the same shape twice, once per
macro world (16,286 and 17,730 passes).

The quiescence test could never pass. Its third progress signal (S3b) counted
every write into `type_id_to_type_map`, and `type_id_for_type` mints a fresh
id — and writes it — on **every attempt, unconditionally**. The signal was
therefore lit on every pass, forever.

**The fix** splits the signal into the two shapes it was conflating.
*Refining* a slot in place (an `Unknown` closure parameter becoming concrete
while the attempt that filled it defers) is progress on its face — every
holder of that slot sees the new type — and it is monotone and finite, so it
buys unlimited further passes. `write_type_slot` now owns that definition and
counts exactly these: not a fresh mint (a new id has no readers), not an
idempotent rewrite (it tells readers what they already knew). *Minting* slots
a later attempt consumes is also real — it is the two-phase chained-`map`
stall, where the pass that instantiates `map`'s signature resolves nothing
and the pass after it succeeds on what that left behind — but every attempt
mints, so it can never be counted directly. Hence the asymmetry: a fruitless
backstop buys exactly **one** more pass. If the second also resolves, wakes
and refines nothing, it consumed none of the first's mints and left a
structurally identical batch of its own; the state is stationary and every
later pass would repeat it. `max_iterations` goes back to being only the
safety net against a non-converging bug, which is all it was meant to be.

Both sides of that one-pass grace are pinned, each planted red: restoring the
mint turns `the_constraint_fixpoint_stops_when_it_settles` red (`set` interns
122,561 types against `map`'s 7,270), and dropping the grace to zero turns
`mapped_maps_thread_the_element_type` and
`a_slot_grounded_list_maps_a_field_closure` red — the two chained-`map` cases
S3b was written for. Five unit tests pin `write_type_slot`'s rule directly.

**Before / after, in Ir:**

| program                        | before   | after   | ratio |
|--------------------------------|----------|---------|-------|
| `import std::set`              | 23.40e9  | 2.50e9  | 9.4x  |
| a `[derive(Debug)]` program    | 29.94e9  | 4.84e9  | 6.2x  |
| `import std::map` (the control)| 2.49e9   | 2.49e9  | 1.00x |

`set` now costs what `map` costs — 2.496e9 against 2.492e9, a 0.2% gap. The
cliff is not reduced, it is gone: there is no longer anything anomalous about
`std::set`. The `[derive(Debug)]` headline is the user-facing one, and it is
the same fix seen through two macro worlds. The inference test binary itself
runs 188 s → 110 s.

**§8's table, re-measured.** In CPU ms, the two binaries INTERLEAVED rep by
rep with the minimum of five taken — the box was at load 22–38 throughout, so
the absolute figures sit well above §8's quiet-box wall times and only the
ratios are meaningful. Every row §8 called a cliff collapses; every row it
called ordinary is untouched, which is the shape the diagnosis predicts.

| module        | before CPU ms | after CPU ms | ratio | macro worlds |
|---------------|---------------|--------------|-------|--------------|
| `ui` (node)   |       14742.1 |       1749.4 | 8.43x |            3 |
| `rpc_server`  |       16994.5 |       2523.0 | 6.74x |            3 |
| `rpc`         |       15461.5 |       2229.5 | 6.94x |            3 |
| `reactive`    |       15312.1 |       1904.3 | 8.04x |            3 |
| `time`        |       10283.3 |       1412.4 | 7.28x |            2 |
| `arena`       |        5439.3 |        915.3 | 5.94x |            1 |
| `set`         |        4009.8 |        415.0 | 9.66x |            0 |
| `crypto`      |         397.7 |        402.9 | 0.99x |            0 |
| `db`          |         441.8 |        440.3 | 1.00x |            0 |
| `map`         |         430.9 |        422.2 | 1.02x |            0 |
| `io`          |         387.1 |        392.1 | 0.99x |            0 |

The macro-world column no longer predicts anything: a world now costs what
any other analysis costs. The four modules with no worlds and no `std::set`
move by less than 2%, which is this instrument's noise floor — the fix is
free where there was no spin to remove.

No behavior change: the same ten constraints stay deferred and reach the same
defaults; inference (1866), corpus, docs and diagnostic_determinism are green
and byte-identical.

### The fixpoint-twice question: measured, and there is nothing to ship

§8 filed "the fixpoint runs twice over the same constraint set
(`resolve_world()` then `build()`)" as a structural fact to take up. It was
an artifact of the spin. The two runs' costs, in Ir:

| leg                                   | before   | after   |
|---------------------------------------|----------|---------|
| `analyze_over_world` (the base leg)   | 12.11e9  | 612e6   |
| `build`                               | 10.00e9  | 69.6e6  |
| — of which `finalize_build`           | 64.1e6   | 64.1e6  |
| — leaving `build`'s own fixpoint      | ~9.94e9  | **5.5e6** |

Before, the two legs cost the same because *both* ran to `max_iterations`.
After, the second fixpoint costs **5.5 million Ir against the first's 612
million** — 0.9% of the first leg and 0.2% of the analysis. The two runs'
inputs genuinely do differ (the second sees the entry's constraints, which is
the point of the S3 split), and resolution is monotone, so the second run
finds the work already done and settles in ~5 passes. Sharing them would save
0.2%, at the cost of the split that S3 exists for. **Verdict: nothing to
ship, and nothing left open.** The remaining §8 residue — macro worlds
excluded from the base cache by `workspace.packages.is_empty()` — is
untouched and still stands on its own.

The `macro_std` re-export of `std::set` stays, per the owner's ruling. It no
longer costs anything worth naming.

## 8.2 The two levers were one and a half: method resolution scanned every impl (E46, 2026-08-07)

E46 filed three numbers off §8.1's post-fix profile —
`impl_member_candidates` 21%, `method_member_candidates` 17%,
`get_type_by_type_id` 16% — and read them as two independent levers. The
profile confirms all three figures exactly and then says they are not
independent: **three quarters of the third number is the first one**, seen
from the other side. One fix collects both.

**The instrument.** Callgrind instruction counts, as in §8.1 — the box ran
between load 3 and load 44 across this work (another agent's suite), so wall
and CPU are usable only as interleaved minimum-of-N ratios and Ir is the
only figure that means anything on its own. Debug binary, one `vilan check`
per program in a fresh process, on §8's three-line per-module programs.

**The filed numbers, verified.** Inclusive Ir, against the pre-E46 tree:

| inclusive                    | `set`  | `rpc_server` |
|------------------------------|--------|--------------|
| `impl_member_candidates`     | 21.16% |       25.78% |
| `method_member_candidates`   | 17.25% |       21.32% |
| `get_type_by_type_id`        | 15.73% |       18.94% |
| `type_implements_trait`      |  1.00% |        6.24% |

`method_member_candidates` is not a separate cost — it is the caller that
reaches `impl_member_candidates` for a `receiver.member()` resolution, and
its 17% is inside the 21%.

### Lever 1: the scan asked the expensive question first

`impl_member_candidates` iterated all 322 std impls per resolution and, for
each, compared the receiver against the impl's subject — a recursive type
walk — *before* asking the cheap question the answer actually turned on:
does this impl declare the name at all. Almost none of them do, so ~99% of
the comparisons were performed on impls dropped one line later.

That predicate is also where the un-interned type read lives:
`implementation.subject.get_type(self)` deep-clones the subject on every
iteration. Of the **202,424** `get_type` calls an `import std::set`
analysis performed, **147,657 were this one closure** — 11.73% of the
analysis against `get_type_by_type_id`'s 15.73% total. Lever 2's headline
was mostly lever 1's scan.

**The fix** is a name index — `Analyzer::implementations_by_member`,
member name to the impls declaring it — and the only interesting question
about it is invalidation, so: **there is nothing to invalidate.** The index
is written at the ONE place `implementations` grows, from the same
`declarations` map, in the same statement. `declarations` is final at that
moment (the later conformance pass fills only `trait_ids` / `trait_args`),
which is what makes registration-time correctness available at all rather
than a cache needing a dirty bit.

**The LSP / warm-analysis verdict: the index cannot go stale, and the
reason is that it has no lifecycle of its own.** There is no separate
incremental registration path to miss. Every analysis — cold, cache-hit,
LSP document, macro world — grows `implementations` through the single
`Node::Impl` walk arm, including `walk_generated_expansion` for derives.
The base cache clones the whole `Analyzer` (`#[derive(Clone)]`), so the
index is cloned with the vector it indexes, and a warm analysis walks its
entry into *that clone*, registering the entry's impls into both. The
stored world is snapshotted BEFORE the entry walk, so no analysis's entry
impls can reach another's. The LSP holds a `Program`, never a live
`Analyzer`, and reads `program.implementations` read-only.

One structural property is worth recording because it decides what needs a
test: **the index is a pre-filter, never the answer.** Every impl it names
is still asked for `declarations[member_name]` before becoming a candidate.
So a row that is too BROAD changes nothing observable, and only a row that
is too NARROW can lose a candidate. Correctness has one direction, and the
pin covers it in its sharpest form (`base_cache.rs`,
`an_entry_declared_impl_resolves_through_a_cache_hit`: a warm analysis
whose entry declares the impl; plant-proven red by skipping entry-source
impls at registration). A pin written for the other direction was dropped
after a planted process-global index failed to turn it red — it could not,
for exactly this reason.

Behaviour is unchanged by construction: the index row is in registration
order, so the sequence reaching `sort_by_key`/`dedup_by_key` is the one the
full scan produced, element for element.

| inclusive Ir, `set` / `rpc_server` | before        | after lever 1 |
|------------------------------------|---------------|---------------|
| `impl_member_candidates`           | 21.16 / 25.78 |  0.66 / 0.78  |
| `method_member_candidates`         | 17.25 / 21.32 |  0.58 / 0.68  |
| `get_type_by_type_id`              | 15.73 / 18.94 |  4.06 / 4.83  |
| `type_implements_trait`            |  1.00 /  6.24 |  0.18 / 0.32  |
| PROGRAM TOTALS (e9 Ir)             | 2.477 / 14.79 | 1.969 / 11.06 |

### Lever 2: the general refactor is unavailable, and it was measured, not felt

After lever 1, `get_type_by_type_id` is 4.06% / 4.83% and what remains is a
flat tail: the largest single reader is `inherited_default_candidates` at
0.74%, then `compute_resource` 0.64%, `any_member_resource` 0.32%. There is
no third hot reader to fix.

Types stay un-interned — B77/B95's in-place resolution doctrine is not
reopened — so the lever was only ever the read-path clone. **Blast radius,
measured by making the change and counting:** `get_type_by_type_id`
returning `&Type` produces **185 compile errors across 218 call sites in
one file**, of which **71 are hard borrow conflicts** (66 `cannot borrow
*self as mutable because it is also borrowed as immutable`, 5 closures
requiring unique access). That is the solver mutating itself all the way
down the inference path while a type borrow is live; no signature change
answers it, only a restructuring. Fifteen times the "about a dozen"
threshold. **Not taken.**

What ships instead is the part where the borrow is free — the seven impl
scans whose enclosing method is `&self` and which hand the type straight to
`compare_type` — behind a documented sibling accessor
(`borrow_type_by_type_id`), framed in the source as a permanent pair with a
mechanical rule, not a migration with 211 sites outstanding. Alongside it,
two genuinely dead clones: `any_member_resource` and its transferable twin
read a member's type before deciding they only wanted its id, so an
unparameterized aggregate deep-cloned a value nothing looked at.

**Its honest size: 0.33% (`set`) / 0.63% (`rpc_server`) of a cold
analysis** — split about evenly between the borrows (0.32%) and the dead
clones (0.31%) on `rpc_server` — and **below the wall-clock instrument's
noise floor.** Best-of-five interleaved CPU ms moved 0.97x–1.03x with no
consistent direction; only Ir resolves it (11.064e9 → 10.995e9 on
`rpc_server`, 1.969e9 → 1.963e9 on `set`; `get_type_by_type_id` 4.83% →
3.51% and 4.06% → 3.28%). Recorded at that size deliberately: the lever as
filed was worth 16%, and 15.4 of those 16 points belonged to lever 1.

### §8.1's table, re-measured

Cold `vilan check` per module, fresh process, the two binaries INTERLEAVED
rep by rep with the minimum of seven taken, CPU ms. The box drifted from
load 39 to load 10 during the run, which the interleaved minimum is chosen
to survive; ratios are the meaningful column.

| module        | before CPU ms | after CPU ms | ratio | macro worlds |
|---------------|---------------|--------------|-------|--------------|
| `rpc_server`  |        1479.5 |       1075.4 | 1.38x |            3 |
| `rpc`         |        1324.9 |       1004.9 | 1.32x |            3 |
| `ui` (node)   |        1226.6 |        932.7 | 1.32x |            3 |
| `reactive`    |        1153.1 |        851.6 | 1.35x |            3 |
| `time`        |         832.1 |        617.9 | 1.35x |            2 |
| `arena`       |         530.2 |        408.6 | 1.30x |            1 |
| `db`          |         270.9 |        210.1 | 1.29x |            0 |
| `map`         |         250.1 |        201.5 | 1.24x |            0 |
| `set`         |         241.7 |        198.0 | 1.22x |            0 |
| `io`          |         241.4 |        190.6 | 1.27x |            0 |
| `crypto`      |         240.7 |        194.5 | 1.24x |            0 |

The shape is the opposite of §8.1's and that is the point. E43's fix was
free where there was no spin to remove, so its control rows moved by under
2%; this one moves EVERY row, controls included, because every analysis
resolves methods. There is no anomalous module here — there is a tax that
was on all of them.

The phase split says where it came off (`VILAN_PHASE_TIMING=1`, one run):

| phase, ms          | `set` before / after | `rpc_server` before / after |
|--------------------|----------------------|-----------------------------|
| `load+walk`        |      84.2 / 96.6     |            1001.0 / 891.3   |
| `base`             |      89.6 / 45.6     |             276.9 / 100.6   |
| `build`            |       9.2 /  4.8     |              21.4 /  16.4   |
| `checks`           |      59.3 / 55.1     |             167.2 / 181.9   |

The inference test binary, both builds interleaved, minimum of three, at
load 36–42: **577.6 → 476.7 CPU s (1.21x)**. Its wall figures under that
load are noise in both directions and are not quoted.

No behaviour change: inference (1930), corpus, docs, diagnostic_determinism,
release_emission and base_cache all green and byte-identical, and B57/B83's
tiering pins — the guard on this exact code — are among them.

### What the profile says next, recorded and not taken

`rpc_server`'s remaining 11.0e9 Ir is no longer solver-shaped. Parsing is
now the largest single family (`parse_binary_level` 179% inclusive, i.e.
re-entered per macro world), and `load+walk` is 891 ms of the module's 1190
— the three macro worlds' nested analyses, each of which re-parses and
re-walks `macro_std`'s workspace from scratch. That is §8's untouched
residue in a new guise: macro worlds are excluded from the base cache by a
blanket `workspace.packages.is_empty()` test they can never satisfy, even
though every macro world in a process shares one `macro_std`. It is the
next lever, and it is a caching question, not a solver one.

Inside the analyzer the remainder is flat and small: the largest single
call site is `inherited_default_candidates`'s filter at 1.08%, which is a
linear scan over all impls that the member-name index CANNOT serve (it
looks its member up in the trait, not in the impl's declarations, so the
name that keys the index is not present on the impl). Fixing it wants a
second index keyed by trait, for about a point. Filed as a fact, not a
lever.

## 9. It was never the parsing: macro worlds re-WALKED and re-RESOLVED `macro_std` (cycle 13, 2026-08-10)

§8.2 closed with "`load+walk` is 891 ms of the module's 1190 — the three
macro worlds' nested analyses, each of which **re-parses and re-walks**
`macro_std`'s workspace from scratch". Half of that sentence is wrong, and
the profile says so before any code changes: **no macro world re-parses
anything.** `parse_clean_cached` has been content-keyed and process-global
since E12, and the module loader has gone through it since; worlds two and
three of an `import std::rpc_server` compile perform 29 module loads apiece
and produce **zero** parse-cache misses. The 179%-inclusive
`parse_binary_level` §8.2 read as "re-entered per macro world" is a
recursive-descent parser being counted in its own recursive frames. What a
macro world genuinely repeats is the WALK and the pre-entry `resolve_world`
— and those are what the base cache exists to hold.

**The instrument.** One cold `vilan check` per module in a fresh process,
debug binary, `VILAN_PHASE_TIMING=1` with the macro-world phase line
temporarily un-suppressed, plus temporary process-wide counters around
`parsing::parse`, `parse_clean_cached`, `load_package_module`, the module
walk loop, the registry build and `expand_one`. All instrumentation was
reverted before the first commit; every figure below that ranks a candidate
is a **callgrind instruction count** (deterministic; the box ran between
load 3 and load 22 throughout, so wall and CPU are quoted only as
interleaved minimum-of-seven).

### The split inside a macro world

`import std::rpc_server`, one cold run — the outer analysis is 1120 ms and
its three macro worlds are 596 ms of it, all of it inside the outer's
`load+walk`:

| macro world (rpc_server's three) | load+walk | base  | build | checks | post-passes | total |
|----------------------------------|-----------|-------|-------|--------|-------------|-------|
| compare.vl                       |      34.8 |  58.2 |   9.9 |   72.8 |        21.8 | 197.5 |
| debug.vl                         |      28.9 |  60.1 |  10.4 |   62.0 |        22.5 | 183.9 |
| json.vl                          |      32.8 |  61.4 |  27.3 |   69.3 |        24.3 | 215.1 |
| the outer analysis               |     830.7 | 121.8 |   9.6 |  158.0 |           — |1120.1 |

Inside a world's ~32 ms of `load+walk`: **24 ms is the module walk, 0.9 ms
is the 29 module loads, and the parse is nil.** The process-wide counters,
printed at each analysis's end (absolute, so worlds difference cleanly):

| after…      | parse calls | parse-cache MISSES | parse-cache hits | module loads | modules walked |
|-------------|-------------|--------------------|------------------|--------------|----------------|
| world 1     |          43 |             **40** |               22 |           61 |             27 |
| world 2     |          46 |             **40** |               51 |           90 |             54 |
| world 3     |          53 |             **40** |               80 |          119 |             81 |
| the outer   |          59 |             **40** |               80 |          119 |            112 |

The miss column never moves again after the first world. The walk column
moves by exactly 27 every time.

### The four reuse levels, and which two were already shipped

| level                                   | shared today? | mechanism / invalidation | worth |
|-----------------------------------------|---------------|--------------------------|-------|
| (a) the parsed AST of `macro_std`'s files | **YES** | `parse_clean_cached`, keyed on content, leaked process-global | **0** — zero re-parses measured |
| (b) the walked module set                | no      | — | ~24 ms per world |
| (c) the resolved pre-entry world (walk + `resolve_world`) | no | the base cache's own key + a per-hit re-hash of every recorded source (E12) | **~92 ms per world** |
| (d) the whole analyzed macro world, per defining file | **YES** | `WORLDS`, keyed on the definition segments' content (E23) | **0** — three distinct defining files is three compiles, and that is the floor |

Candidate (a) as filed — "a parsed-file cache keyed on content hash" — is
the cache the compiler has had since E12; candidate (c) as filed — "caching
the analyzed macro WORLD per defining-file content" — is `compile_world`'s
`WORLDS`. Both were already in the tree, which is why neither could be the
lever. (b) and the base half of (c) are the same thing seen at two depths,
and the thing that holds them is the §8-recorded direction: **the base
cache**. So there was one candidate, not three.

### What shipped: the key describes the workspace

`base_cacheable` excluded macro worlds with `workspace.packages.is_empty()`.
That test was standing in for a KEY that did not describe the workspace, not
for a hazard: the key was `(platform, the entry's sorted std:: reference
names)`, so a world built with `macro_std` loaded and one built without were
indistinguishable in it, and refusing to store either was the only safe
move. `BaseCacheKey` now carries the workspace — dependency packages by
identity (roots, surface flag, dependency edges), the entry's dependency
edges, the entry's references INTO each dependency, and the expansion
budgets — and the `is_empty()` test goes. What the packages CONTAIN stays
out of the key and is re-hashed per hit, which is the rule the cache already
ran on.

All three of `rpc_server`'s macro worlds land in ONE slot, which is the
measured fact the lever rests on: their entry `std::` seeds are empty and
their `macro_std::` seeds dedup to the same five (`build`, `fresh`, `meta`,
`option`, `source`) whether the defining file is compare.vl, debug.vl or
json.vl.

Two seams the widening required, both recorded here because each was a way
to get it silently wrong:

- **The entry's `<dep>::module` seeds are entry-text slices.** They reach
  the world's maps exactly as the `std::` seeds do (`modules[id].name`, the
  namespace scope), and only the `std::` side was interned. The store's
  `transmute` claims every surviving reference is `'static` in fact; with a
  workspace admitted, that claim was one uninterned path short of true. The
  dep seeds now go through `interned_display_name` at extraction, and the
  load loop consumes that same interned list.
- **E23's blanked-source entanglement, met head-on rather than half-solved.**
  The `!entry_source.contains("macro")` bypass exists because a
  macro-DEFINING entry registers into the registry the world carries, which
  would leak one entry's macros into another analysis. A blanked world entry
  is full of the substring `macro_std`, so the test would have kept every
  macro world out by accident — and deleting it for macro worlds is only
  sound because of a fact, not a hope: **inside a macro world the load
  region registers NOTHING** (`macro_registry`'s `in_macro_world` early
  return), so a world's entry contributes no registry rows whatever its text
  reads. The test is now scoped to the analyses that can register at all.
  `expand_entry_over_world` mirrors that same split instead of returning
  early: a world's entry still EXPANDS, against the empty registry, which is
  what the load region did before the hoist took the entry's expansion over.
  E23's ACTUAL entanglement — the outer analysis whose own buffer defines
  macros — is untouched and still bypasses.

### Before / after

Cold `vilan check` per module, fresh process. Ir is the primary column
(deterministic); CPU ms is the interleaved minimum of seven, and its control
rows show that instrument's noise floor to be about ±4%.

| module                      | before Ir | after Ir | ratio | CPU ms ratio | macro worlds |
|-----------------------------|-----------|----------|-------|--------------|--------------|
| `reactive`                  |   9.301e9 |  7.739e9 | 1.202 |        1.205 |            3 |
| `ui` (node)                 |  10.424e9 |  8.856e9 | 1.177 |        1.174 |            3 |
| `rpc`                       |  10.934e9 |  9.362e9 | 1.168 |        1.178 |            3 |
| `rpc_server`                |  11.810e9 | 10.246e9 | 1.153 |        1.136 |            3 |
| **`[derive(Debug, PartialEq)]`** | 5.882e9 | 5.114e9 | **1.150** |    1.131 |        2 |
| `time`                      |   6.858e9 |  6.091e9 | 1.126 |        1.100 |            2 |
| **`[derive(Debug)]`**       |   3.977e9 |  4.012e9 | **0.991** |    0.990 |        1 |
| `arena`                     |   4.527e9 |  4.558e9 | 0.993 |        0.974 |            1 |
| `set` (control)             |   2.122e9 |  2.123e9 | 1.000 |        0.962 |            0 |
| `map` (control)             |   2.117e9 |  2.117e9 | 1.000 |        1.019 |            0 |
| `io` (control)              |   2.028e9 |  2.028e9 | 1.000 |        1.010 |            0 |

**The single-world row is a loss and it is the honest price of the lever.** A
cold compile whose only macro world is the first one pays the store — one
`World` clone, ~35e6 Ir — and never gets a hit back: 0.9% on
`[derive(Debug)]`, 0.7% on `arena`. It buys 13–20% wherever a second world
follows, and a second world follows as soon as a program derives from two
families, which `[derive(Debug, PartialEq)]` is: **1.15x**. That pair is the
user-facing headline, and quoting only the winning half of it would be
dishonest.

The mechanism check, inclusive Ir on `rpc_server`, is exact:

| inclusive                | before   | after    | delta     |
|--------------------------|----------|----------|-----------|
| `compile_world`          |  6.129e9 |  4.567e9 | **−1.562e9** |
| — `resolve_world` (all)  |  3.374e9 |  2.136e9 |    −1.238e9 |
| — `walk_expr_nodes`      |  0.856e9 |  0.494e9 |    −0.362e9 |
| `parse_binary_level'2`   | 20.0387e9| 20.0383e9|  **−0.0004e9** |
| PROGRAM TOTAL            | 11.810e9 | 10.246e9 |    −1.564e9 |

The whole program's saving is `compile_world`'s saving, to within 0.1%; and
parsing moves by one part in sixty thousand, which is the diagnosis at the
top of this section stated as a measurement.

A long-lived multi-analysis process gains too, but modestly and for a
different reason: `WORLDS` already bounds macro-world COMPILES to one per
distinct defining-file content, so a process that analyzes hundreds of
programs still compiles only a handful of worlds. `check_scope_differential`
(the whole corpus analyzed twice in-process) is **36.33 → 35.54 CPU s,
1.022x**, interleaved minimum of seven. The cold single-process case is
where this lever lives.

### The gates

Four pins in `base_cache.rs`, each plant-proven:
`macro_worlds_share_one_base_world_and_observe_identically` (deltas of
exactly two hits and one miss for a two-defining-file entry),
`a_warm_macro_world_observes_what_a_cold_one_observes`,
`a_missing_derive_errors_identically_through_a_warm_macro_world` (§6.13's
sharp edge one level down), and
`a_macro_world_is_never_served_an_ordinary_world`. The first two go red
under the restored `workspace.packages.is_empty()` gate. The last exists
because the other three do NOT: planting `workspace: Vec::new()` into the
key left all of them green, since a macro world's empty `std::` seeds
happened to differ from those fixtures' — and an entry that imports no std
module at all agrees with its own macro world on every field the plant left
in the key, so the world is served the ordinary base and fails with "cannot
find module 'macro_std' to import", 44 diagnostics deep. That is the shape
of the hazard the workspace row removes, and it needed its own fixture to
show it.

`macro_world_cache_clear` is the surface that makes a warm/cold differential
over a macro world writable at all: `WORLDS` memoizes by content, so without
it the first compile in a test process is the only one.

### What the profile says next, recorded and not taken

- **Hashing is now the largest single family in a cold analysis.** SipHash
  (`d_rounds` + `c_rounds` + `Hasher::write` + `u8to64_le`) is **13.2%** of
  `rpc_server`'s remaining 10.2e9 Ir, and `__memcpy_avx_unaligned_erms` is
  another 7.1% (the world clones the cache trades on, plus map growth). The
  analyzer's ~90 id-keyed tables all run on `std`'s default hasher against
  4- and 8-byte keys, where SipHash's setup dominates its own work. A
  hasher swap on the `HashMap<Id, _>` / `HashMap<TypeId, _>` families is
  contained and measurable, and it is the largest thing left. Note the
  neighbouring 12.5% of `precondition_check` is a DEBUG-build artifact and
  will not appear in a release profile — do not count it.
- **The trait-keyed second index does NOT ride along.** §8.2 filed
  `inherited_default_candidates` at ~1% and this lane was to take it only if
  trivially cheap after the main work. Re-measured: 128.66e6 Ir before,
  128.68e6 after — **unchanged in absolute terms**, because it lives
  entirely in the outer analysis and no macro world ever reaches it. It is
  1.26% of the smaller total, it still needs a second index with its own
  invalidation story and its own pin, and "cheap" it is not. Left recorded,
  with the number now measured rather than estimated.
- **The remaining macro-world cost is not redundant.** After the base hit, a
  macro world still pays its own `build` (~16 ms), `checks` (~68 ms) and
  post-passes (~23 ms) — ~107 ms of the ~199 ms it cost — and those are the
  ordinary cost of analyzing a distinct program, not a repetition of
  anything. Removing them means not analyzing the world at all, i.e.
  persisting a compiled world across PROCESSES, which is §6.2's rejected
  disk-serialization with a worse correctness profile (`World.program` is a
  `JsProgram<'static>` over leaked text, not a string). Recorded as the
  boundary of this lever, not as a next step.
