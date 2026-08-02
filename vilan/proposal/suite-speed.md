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

- **E25 — run the binaries in parallel** (the big one): 130.7 s of serial
  execution against a longest-single-binary of 29.9 s is a theoretical ~4.4×.
  `cargo-nextest` is the obvious instrument; note it runs each TEST in its
  own process, which makes today's in-process serialization (vilan-wasm's
  mutex, the LSP's overlay locks) unnecessary rather than broken, and the
  port-binding legs already survived E19's port-0 migration. The risks to
  clear per-binary: stdout-parsing e2e legs, node-spawn storms under load
  (the E20 flake history marks where timing pressure bites), and CI parity.
  Est: 131 s → ~35–45 s.
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
- **E29 — cut the edit tax**: 16 s to relink 43+ binaries at 490 % CPU.
  Two independent sub-levers: a faster linker (neither mold nor lld is
  installed today; either typically halves link-heavy rebuilds), and
  consolidating integration binaries that share a subject (the five-plus
  parse_* files, the hmr trio) to cut link JOBS — weighed against per-binary
  isolation, and worth less if E25 lands via nextest (which prefers many
  binaries). Evidence first: `cargo build --timings` on the relink.

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
outcome dictates)**. E27 (shipped: −25.4 s) takes the serial floor to
~105 s; E25's real ceiling wants re-estimating from CPU sums; E30 is the
only lever left on inference and it needs the analyzer-arc decision first.

## 3. What was NOT found

- No mystery time: gaps, harness startup and the warm compile check are all
  ≈ 0.1 s. The suite spends its time exactly where the tests say it does.
- No already-slow-by-accident binary: corpus is parallel, the e2e legs are
  bounded by real servers doing real work, and the long tail (40 sets) sums
  to ~26 s with nothing above 2.6 s.
- The Linux leak harness (`leak_measurement`, 200-analysis loops) hides
  inside the vilan-lsp unit binary's 29.9 s rather than standing alone —
  E28's fixture work should measure it separately before touching it.
