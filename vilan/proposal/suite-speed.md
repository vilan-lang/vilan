# Suite speed — the measured profile (E21)

> **Status: AUDIT DONE 2026-08-03.** Every number below was measured on the
> dev machine (16 cores, WSL2, warm tree, v0.22.0-era `next`); the levers are
> filed as backlog E25–E29, each its own suite-gated slice. The constraint
> from E21's charter is restated because it binds every slice: **no gate
> weakens** — no pins dropped, no cases sampled, no goldens loosened;
> anything that changes what is *tested* is out of scope by definition.

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
| 18.7    | 1205  | tests/inference.rs         | **534 `assert_compiles_and_runs` node spawns** ≈ 35 ms each — the binary is node-startup, nearly wall-to-wall |
| 16.7    | 8     | tests/docs.rs              | every book fence compiled **serially** inside 8 tests; runnable fences also spawn node |
| 14.9    | 9     | tests/interpreter.rs       | per-case `CARGO_BIN_EXE_vilan` spawns, serial |
| 8.1     | 2     | tests/examples.rs          | 9 examples staged via `git ls-files` and built through the debug binary |
| 5.4     | 6     | tests/corpus.rs            | already 8-way parallel (`thread::scope`, chunked) — the shape the others should copy |
| 4.7/4.2/2.8 | — | hmr / rpc_http / transport | e2e legs: ports (post-E19 they bind port 0) + real servers |
| ~26     | —     | the other 40 sets combined | long tail, none above 2.6 s |

43 integration-test files build 43 binaries, each linking the full crate
stack — that is what the 16 s edit tax buys, every arc, before a single test
runs.

## 2. The levers (filed as E25–E29)

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
- **E26 — batch inference's node runs**: 534 spawns × ~35 ms of node startup
  IS the 18.7 s. One (or a few) node processes executing the emitted
  programs in sequence with per-program output markers keeps every assertion
  byte-identical while paying startup once. The backlog named this lever at
  filing; the measurement confirms it is nearly the whole binary. Est:
  18.7 s → ~3–4 s. (Interacts with E25: land this first — under nextest's
  per-test processes the spawn count cannot be amortized across tests.)
- **E27 — parallelize the docs gate and interpreter cases**: both are
  serial loops over independent compiles; corpus.rs already demonstrates
  the safe 8-way chunk shape in this very suite. Est: 16.7 s → ~4 s and
  14.9 s → ~4 s.
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

Sequencing that respects the interactions: **E26 → E27 → E25 → (E28/E29 as
E25's outcome dictates)**. E26+E27 alone take the serial floor to ~95 s;
E25 on top of them lands the suite near the longest remaining binary.

## 3. What was NOT found

- No mystery time: gaps, harness startup and the warm compile check are all
  ≈ 0.1 s. The suite spends its time exactly where the tests say it does.
- No already-slow-by-accident binary: corpus is parallel, the e2e legs are
  bounded by real servers doing real work, and the long tail (40 sets) sums
  to ~26 s with nothing above 2.6 s.
- The Linux leak harness (`leak_measurement`, 200-analysis loops) hides
  inside the vilan-lsp unit binary's 29.9 s rather than standing alone —
  E28's fixture work should measure it separately before touching it.
