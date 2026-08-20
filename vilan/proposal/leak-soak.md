# The leak soak — looking thoroughly for what does not plateau (M2)

> **Status: SHIPPED 2026-08-18.** Two instruments. Tier 1 is
> `crates/vilan-lsp/src/document.rs`'s `leak_measurement` module, extended from
> synthetic fixtures at tens-to-hundreds of analyses to the two real
> application files at thousands, through both of the language server's
> allocation lifetimes, `#[ignore]`d so the PR gate never pays for it. Tier 2 is
> `scripts/soak.sh`, a standalone multi-minute driver for the two processes a
> person actually leaves running — a `vilan run --watch` session under rebuild
> and browser-reconnect churn, and a compiled Node server under sustained
> requests. No new dependency: `/proc`, integer counters, `curl` and `node`.
>
> **The verdict is in §4, and it is not "nothing".** Every per-site leak
> counter plateaus exactly, on every corpus, on both drivers — that half is a
> clean negative. The soak also produced one finding worth filing, and it is a
> number rather than a bug: the language server's *by-design* per-analysis leak
> costs **3.12 MiB of resident memory per keystroke** on the website's
> 735-line `page.vl` and 0.74 MiB on kolt's 372-line `views.vl` — 6.1 GiB after
> two thousand keystrokes in one file, none of it returnable. Filed as backlog
> M7. The other three curves are dispositioned and closed in §4.
>
> **M7 is FIXED (2026-08-19, §7):** the language server now reclaims a
> superseded analysis's entry text and AST when the `Document` replaces or
> drops it. §7 is the design, its soundness argument, the new accounting, and
> the soak re-run against §4.1's table.

This paper is the record backlog M2 asked for, and it was written to be
falsifiable rather than reassuring: the owner's brief was *"there might not be
one, but I want to thoroughly look"*, and a "looked thoroughly, found nothing"
is only worth something if the looking is described precisely enough that
someone can disagree with it. §1 is the method and its instruments; §2 is
tier 1's results; §3 is tier 2's; §4 dispositions every curve either of them
produced; §5 is how to run both; §6 is what dhat would add and why it is not
here.

## 1. The method

### 1.1 What a leak is here, and why RSS is not the instrument

Three different things get called a leak, and only one of them is a bug:

| | what it is | how it is measured here |
|---|---|---|
| **per-analysis leak** | memory made immortal by a `Box::leak`/`String::leak` on the analysis path — the source text and AST arenas a `Program` borrows for `'static` | `leak_tally`, per site, exact bytes |
| **retention** | memory still reachable and still in a live cache — nothing is lost, but nothing is returned either | the same counters, plus the cache's own key: a curve that grows with *distinct inputs* rather than with *iterations* |
| **unbounded growth** | either of the above growing without a bound the design can name | two equal windows leaking unequally; a curve that never flattens |

`leak_tally`'s module doc already refuses RSS as the gate, and this soak agrees
with it and then goes further: RSS is dominated by allocator retention from
rebuilding and dropping the reachable `Program` on every call, so it moves by
megabytes for reasons that have nothing to do with what was made immortal. It
is printed on every row below. It is asserted on nowhere. The one place RSS
*is* the only available instrument is tier 2's Node leg, and that leg says so.

### 1.2 Tier 1 — the LSP, scaled to real files

The shipped `leak_measurement` module runs a warmup, zeroes the thread-local
counters, runs a measured window, and asserts the window's per-site totals.
This extends it in four ways:

1. **Real corpora.** `kolt/src/views.vl` (372 lines, 11,337 bytes) and
   `vilan-website/src/page.vl` (735 lines, 26,968 bytes) — the two files
   `perf-baseline.md` §2.3 measures keystroke latency on, read through the same
   environment variables (`VILAN_PERF_KOLT`, `VILAN_PERF_WEBSITE`) and
   **skipped, not failed**, when absent. One export serves both harnesses, and
   both speak about the same two files.
2. **Thousands of analyses**, where the synthetic fixtures do 40 and 200.
3. **Two windows instead of one**, which is what upgrades the claim from *the
   leak is small* to *the leak plateaus*. Two equal-length windows over an
   equal-length document must leak the same bytes at every site; anything
   accumulating makes the second larger, and exact integer counters say so with
   no threshold, no tolerance and no curve fit.
4. **Both allocation lifetimes.** `Document::analyze` — the entry point the
   real server's `spawn_blocking` wraps — spawns a fresh 256 MiB-stack thread
   per call, runs the analysis on it, and joins. The shipped fixtures all drive
   `analyze_on_this_thread` inline on one long-lived thread instead. The soak
   drives both.

The edit is a **moving single-character edit**: a fixed-width trailing comment
carrying one `x` that walks one column per iteration and wraps. Three
properties, each load-bearing — every iteration is a *distinct content*, so
nothing is served from `parse_clean_cached` and every analysis is a real
re-analysis; every iteration is the *same length*, which is what makes the
plateau assertion exact rather than statistical (the entry-text leak over N
analyses is exactly N × the file's bytes); and a trailing comment is valid in
every file, so one mutation works on any corpus.

### 1.3 Reading a thread-local counter across threads

The per-analysis-thread driver has a trap in it worth recording, because
falling in produces a *perfect-looking* result. `leak_tally`'s counters are
thread-local by deliberate design (its module doc gives the reason: a
process-global counter's before/after deltas are famously flaky under a
parallel test runner). A thread-local dies with its thread. So a driver that
spawns a thread per analysis and reads the tally *after the join* reads zero at
every site — which is indistinguishable, in the output, from a flawless
plateau.

The driver therefore reads each thread's own counters **inside** that thread,
before it exits, and sums them in the caller. The gate pin
`leak_soak_harness_smoke` exists for exactly this: it runs the same fixture
through both drivers and asserts they agree **to the byte**, so a driver that
reports nothing cannot pass as a driver that found nothing.

Two facts license the comparison, and both were checked in the tree rather than
assumed: nothing the compiler caches is thread-local — `BASE_CACHE`
(`analyzer.rs`), the macro `WORLDS`/`FAILURES`/`EXPANSIONS`/`PARSES`
(`macros.rs`) and `parse_clean_cached` (`lib.rs`) are process-global
`OnceLock<Mutex<…>>` — and the five `thread_local!`s in `analyzer`, `macros`,
`call_graph`, `util` and `transformer` are per-analysis scratch or test
counters, not caches. Both drivers therefore see the same warm caches; what
differs is that under the per-thread driver every allocation belongs to a
thread that then dies.

### 1.4 Tier 2 — the two processes people leave running

`scripts/soak.sh`, and the choice of a script over an `#[ignore]`d test is a
decision with a reason rather than a preference. A test would reuse the CLI
suite's harness (`support::WATCH_LIVENESS`, `kill_watcher`, the SSE client in
`tests/hmr.rs`), which is real value. But what a soak is *for* is being run for
minutes to hours, by hand, on a quiet box, and nightly by a scheduler — and in
all three of those a script wins: it takes `--rounds`/`--requests` without a
recompile, it streams its table as it goes instead of buffering under
`--no-capture`, and it can be pointed at a released binary rather than a
`CARGO_BIN_EXE`. The cost is duplicated waiting logic, and it is a small cost:
the two helpers a script needs (wait for a line, wait for a port) are ten lines
of `grep`.

Two legs:

- **watch** — `vilan run --watch` on a two-leg fullstack fixture, through N
  rebuild rounds. Each round rewrites `src/server.vl` with its round number, so
  the round's completion is *witnessed* by the restarted Node child's own boot
  marker rather than assumed from a timer. Between rounds, `CLIENTS` SSE
  browsers connect to the dev channel and then disconnect — the churn backlog
  M3's file-descriptor leak lived in (hmr.md's M3 appendix). Descriptors,
  threads and RSS are read from `/proc/<watcher>/` **three times a round**:
  idle, with the browsers connected, and after they leave.
- **server** — the compiled Node server, built with `vilan build` and run
  directly under `node` (never via `vilan run`, whose child would be orphaned
  by a kill — `rpc_http.rs` records that lesson), under M requests split
  between the page route and `POST /rpc`. RSS, descriptors and threads sampled
  per batch, plus a **settle sample** after an idle window, because a rising
  RSS curve under load is not by itself a leak: V8 grows its heap while nothing
  forces it to collect, and what separates *grew* from *retains* is what the
  number does once the load stops.

**The LSP edit storm is deliberately not a leg of the script.** No JSON-RPC
protocol harness exists in this repository to drive a real `vilan-lsp` process
over stdio, and inventing one for a soak would have been a larger and less
trustworthy instrument than the one that already exists: tier 1 drives the same
entry point the server's `spawn_blocking` wraps, for thousands of keystrokes,
reading exact per-site counters — where a protocol harness could only have
watched RSS, the one signal §1.1 rejects.

### 1.5 Process hygiene, and why it is in the paper

A soak that leaves processes behind is a leak generator, not a leak detector.
Three rules, each of them existing scar tissue:

- **Every fixture self-expires.** Each fixture server sleeps out a deadline
  derived from the configured run and then exits, so a soak whose driver is
  killed leaves nothing running.
- **Every process is killed *and asserted dead*.** SIGKILLing the watcher does
  not reap its Node grandchild (E60), so every fixture server carries a
  `/shutdown` route and its death is witnessed by a refused connection — each
  poll re-sends the request, exactly the css e2e's shape in `tests/hmr.rs`.
  The watcher's `vilan-watch-<pid>.mjs` temp script is removed the way
  `support::kill_watcher` removes it.
- **The zombie sweep matches the process NAME** — `pgrep -x node`, which
  matches `comm`. `pgrep -f node` matches the soak's own command line and
  reports the soak as the leak it was looking for.

## 2. Tier 1 — the language server, per corpus

Measured 2026-08-18 on the dev machine, one process, release:

| | |
|---|---|
| CPU | AMD Ryzen 7 9800X3D, 8 cores / 16 threads |
| OS | WSL2, Linux 6.18.33.1-microsoft-standard-WSL2 |
| RAM | 23 GiB |
| tree | `next` at ccf74a5f plus this change |
| profile | release |
| run | 5,040 analyses, **1021.1 s**, exit 0 |

Window = 1,000 analyses on the inline driver, 250 on the per-thread one, two
windows each, after 10 unmeasured warm-up analyses. `w1`/`w2` are the two
windows; the counted columns are `leak_tally` bytes, exact.

### 2.1 The counted leak — every site, both windows

| corpus | driver | window | analyses | entry-text B | entry-AST B | display B | macro B | total B | B/analysis |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|
| `kolt/src/views.vl` (372 lines, 11,337 B) | inline | w1 | 1000 | 11,337,000 | 404,040,000 | 0 | 0 | 415,377,000 | 415,377 |
| | inline | w2 | 1000 | 11,337,000 | 404,040,000 | 0 | 0 | 415,377,000 | 415,377 |
| | per-thread | w1 | 250 | 2,834,250 | 101,010,000 | 0 | 0 | 103,844,250 | 415,377 |
| | per-thread | w2 | 250 | 2,834,250 | 101,010,000 | 0 | 0 | 103,844,250 | 415,377 |
| `vilan-website/src/page.vl` (735 lines, 26,968 B) | inline | w1 | 1000 | 26,968,000 | 947,800,000 | 0 | 0 | 974,768,000 | 974,768 |
| | inline | w2 | 1000 | 26,968,000 | 947,800,000 | 0 | 0 | 974,768,000 | 974,768 |
| | per-thread | w1 | 250 | 6,742,000 | 236,950,000 | 0 | 0 | 243,692,000 | 974,768 |
| | per-thread | w2 | 250 | 6,742,000 | 236,950,000 | 0 | 0 | 243,692,000 | 974,768 |

Four things this table says, in the order of how load-bearing they are:

1. **Every window is byte-identical to its partner.** Not close, not within a
   tolerance — the same integers. The plateau is exact.
2. **Nothing leaks that is not named.** `total` equals `entry-text + entry-AST +
   display` on every row, so the macro path, the content-keyed module parses,
   the loader's error path and the wasm front end contributed *zero* bytes over
   5,000 measured real-application keystrokes. The gensym, world-recompile and
   broken-world plateaus that `analysis-reuse.md` §2 and E23 closed on synthetic
   fixtures hold on real files at fifty times the iteration count.
3. **The per-analysis leak is exactly file-proportional.** `entry-text` is the
   analysed source, to the byte, every time (1,000 × 11,337 = 11,337,000). It is
   proportional to the file, and the *rate* is flat in the iteration count.
4. **Both drivers agree to the byte.** 415,377 B/analysis on kolt and 974,768 on
   the website whether the analysis runs inline or on its own dying thread. The
   thread-per-analysis lifetime the shipped server actually uses changes nothing
   the counters can see.

### 2.2 RSS — the secondary signal, and what it says anyway

RSS is not asserted on, and §1.1 says why. It is reported because on this
corpus it agrees with the counters about the *shape* and disagrees about the
*scale* — and the disagreement is the interesting part.

| corpus | driver | w1 RSS growth | w2 RSS growth | KiB per analysis | × the counted leak |
|---|---|---:|---:|---:|---:|
| `views.vl` | inline (1000) | 753,012 KiB | 762,060 KiB | 753.0 / 762.1 | 1.86× |
| `views.vl` | per-thread (250) | 200,804 KiB | 191,612 KiB | 803.2 / 766.4 | 1.94× |
| `page.vl` | inline (1000) | 3,194,936 KiB | 3,190,440 KiB | 3,194.9 / 3,190.4 | 3.35× |
| `page.vl` | per-thread (250) | 794,488 KiB | 797,808 KiB | 3,178.0 / 3,191.2 | 3.34× |

The second window grows as much as the first: within 1.2 % on the two
1,000-analysis rows, within 4.6 % on the shorter 250-analysis pairs where a
single arena decision is a larger share of the total. That is what makes this
RSS number readable at all — allocator retention *saturates*, and a saturated
allocator on a steady workload stops growing. This does not stop growing, on
either corpus, on either driver, at any point in 5,000 measured analyses. It is
the counted leak, plus the deep heap the counters can only estimate.

**The scale is the finding.** One keystroke on `page.vl` costs the language
server **3.12 MiB of resident memory it never gives back**, and one on
`views.vl` costs 0.74 MiB. That is §4's finding 1.

## 3. Tier 2 — the two long-lived processes

Same machine, same tree, the release binary, 2026-08-18. Both legs exit 0 and
the zombie sweep is clean (§3.3).

### 3.1 `vilan run --watch` — 40 rounds, 4 browsers churned per round

`scripts/soak.sh --rounds 40 --requests 20000 --batch 500 --clients 4`, leg 1.
Every round rewrites the server leg, waits for the restarted Node child's own
boot marker, then samples `/proc/<watcher>/` three times: idle, with four SSE
browsers connected, and after they disconnect.

| round | fds idle | fds open | fds after | threads idle | threads open | threads after | RSS KiB |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 4 | 8 | 4 | 4 | 8 | 4 | 61,956 |
| 2 | 4 | 8 | 4 | 4 | 8 | 4 | 66,044 |
| 10 | 4 | 8 | 4 | 4 | 8 | 4 | 66,104 |
| 20 | 4 | 8 | 4 | 4 | 8 | 4 | 66,672 |
| 30 | 4 | 8 | 4 | 4 | 8 | 4 | 66,996 |
| 40 | 4 | 8 | 4 | 4 | 8 | 4 | 67,320 |
| idle +10 s | 4 | — | — | 4 | — | — | 67,320 |

**The descriptor and thread columns are constant across all forty rounds** —
`4 → 8 → 4` and `4 → 8 → 4`, with no exception at any round. Four browsers cost
exactly four descriptors and four threads while they are connected and exactly
zero once they leave, which is the M3 fix's contract stated as a field
measurement rather than a unit test: `hmr.md`'s M3 appendix measured 14 fds /
14 threads at ten open connections and 4 / 4 the moment they closed, and 160
connect-disconnect cycles across forty rebuilds reproduce it exactly. Against
the pre-fix behaviour the same run would have ended at **164** descriptors.

RSS grows 61,956 → 67,320 KiB across the session: **+5,364 KiB total**, of which
+4,088 KiB is the single step from round 1 to round 2 and +1,276 KiB is
everything the remaining 38 rounds did (~34 KiB/round, decelerating). §4
dispositions it.

### 3.2 The compiled Node server — 20,000 requests

Leg 2, `vilan build` then `node dist/server.mjs` directly (never through
`vilan run`, whose child a kill would orphan — `rpc_http.rs` records that).
Each batch is half `GET /` (the built page, served from a `Document::of` string)
and half `POST /rpc` (a real dispatch through the JSON codec and a reactive
turn, incrementing an `[expose]`d signal). Two runs, differing only in V8's
old-space limit:

```sh
scripts/soak.sh --rounds 40 --requests 20000 --batch 500 --clients 4
scripts/soak.sh --leg server --requests 20000 --batch 1000 --heap-cap 64
```

| requests | RSS KiB (default heap) | RSS KiB (`--heap-cap 64`) |
|---:|---:|---:|
| 0 | 62,692 | 63,632 |
| 2,000 | 93,544 | 93,176 |
| 5,000 | 124,424 | 112,484 |
| 10,000 | 174,620 | 130,860 |
| 15,000 | 206,372 | 131,340 |
| 20,000 | 214,100 | 159,864 |
| settled (+10 s idle) | 214,100 | 159,864 |

Descriptors held at **22** and threads at **11** for every sample of both runs —
the fd/thread half of this leg is flat, full stop.

The RSS curve rises and then stops: the last 4,500 requests of the default run
move it 188 KiB (213,912 → 214,100), against 148 MiB over the first 15,500. The
settle sample is identical to the peak, which says V8 does not hand the pages
back once it has them — expected, and not a leak.

The capped run is the discriminator, and it is the reason `--heap-cap` exists.
Under `--max-old-space-size=64` the same 20,000 requests complete with **no
heap-out-of-memory abort** and land 53 MiB lower. Its own curve is not flat
either — it sits at ~131 MiB from request 8,000 to 15,000 and then climbs
again to 160 MiB — and that is fine, because the claim it carries is not "flat"
but "never aborted": V8 fits the identical work inside a 64 MiB old space when
told to. So whatever the default run's 148 MiB was, it is heap the collector
had no reason to reclaim rather than retention it could not. An unbounded leak
does not pass that test.

### 3.3 Teardown

Both fixture servers answered `/shutdown` and their deaths were witnessed by a
refused connection, on both runs. `pgrep -x node` was empty before and after
each run: **no node process outlived any soak**. The watcher's
`vilan-watch-<pid>.mjs` was removed by the driver, the way
`support::kill_watcher` removes it.

## 4. Findings and dispositions

Four curves came out of §2 and §3. One is filed; three are dispositioned and
closed.

### 4.1 FILED — the per-analysis leak is linear in keystrokes, in megabytes

**Backlog M7.** Every analysis leaks its entry source and entry AST, by
construction: the `Program` borrows both for `'static`, so
`analyze_on_this_thread` leaks a copy of the text and `analyze_source` leaks the
parsed tree. That much is designed, named and already measured — it is what the
shipped `per_analysis_leak_is_bounded_by_named_sites` pin asserts, and
`analysis-reuse.md` §2 explicitly leaves eliminating it as "a recorded
refinement for the entry". What has never been measured is what it *costs over
a session*, and this soak measures it:

| corpus | counted B/keystroke | RSS MiB/keystroke | after 2,000 keystrokes |
|---|---:|---:|---:|
| `kolt/src/views.vl` (372 lines) | 415,377 | 0.74 | **1.4 GiB** |
| `vilan-website/src/page.vl` (735 lines) | 974,768 | 3.12 | **6.1 GiB** |

The AST is the whole of it: 947.8 MB of the website's 974.8 MB counted total is
`EntryAst`, 97 %, against 27 MB of source text. Both figures are flat per
analysis and linear in the count — the rate plateaus and the total does not,
which is precisely the distinction §1.1 draws between a bounded leak and
unbounded growth. Two thousand keystrokes is not a stress figure: at a 150 ms
debounce it is a couple of hours of typing in one file.

Not fixed here, and not a one-liner: the entry text and AST are `&'static`
because the whole `Program` type is parameterised on that lifetime, so freeing
them is a lifetime refactor (or an arena the document owns and swaps), not a
`drop`. Filed with the numbers, the site and the repro rather than patched
around.

**Repro** (both corpora, ~17 minutes):

```sh
VILAN_PERF_KOLT=/path/to/kolt VILAN_PERF_WEBSITE=/path/to/vilan-website \
cargo nextest run --release -p vilan-lsp --run-ignored ignored-only \
    -E 'test(leak_soak_corpus_plateaus)' --no-capture
```

### 4.2 CLOSED — every other leak site plateaus at zero

By-design retention, each bounded by a key that is not the iteration count, and
each contributing **0 bytes** over 5,000 measured real-file keystrokes (§2.1):

- **`parse_clean_cached`** (`crates/vilan-core/src/lib.rs`) — one leaked source
  and AST per *distinct content*, shared by every compile in the process. A
  moving keystroke never repeats a content, so this site could have grown with
  the iteration count; it did not, because the language server's entry is
  parsed directly by `analyze_source` and leaked at its own site, and every
  *module* the corpus reaches is unchanged and served from the cache. (The CLI
  does route its entry through here, which is §4.3.)
- **The macro caches** (`crates/vilan-core/src/macros.rs`: `WORLDS`,
  `FAILURES`, `EXPANSIONS`, `PARSES`) — content-keyed, bounded by distinct
  macro-world definitions and distinct expansions. Zero on both corpora, which
  is the E23 and `analysis-reuse.md` §2 plateaus holding at scale.
- **`BASE_CACHE`** (`crates/vilan-core/src/analyzer.rs`) — resolved pre-entry
  worlds keyed by `BaseCacheKey`, bounded by distinct (platform, std seeds,
  workspace, macro budgets) tuples. One key per corpus here, as intended.
- **`interned_display_name`'s `NAMES`** (`crates/vilan-core/src/analyzer.rs`) —
  one leaked string per distinct dependency display name per process. The
  `display` column is 0 on both corpora because neither package declares a
  dependency; the site is bounded by names, not by analyses, either way.
- **The server's `line_indices`** (`crates/vilan-lsp/src/main.rs`) — one index
  per stable on-disk path, deliberately never invalidated, and deliberately
  never populated for a path with an open buffer. Bounded by files visited.

### 4.3 CLOSED — the watch session's RSS is bounded and decelerating

+5,364 KiB over 40 rounds, three quarters of it in the first round-to-round
step and ~34 KiB/round thereafter (§3.1). Each round writes a **new** 495-byte
`src/server.vl`, and the CLI reads its entry through `parse_clean_cached`, so
each round does add one content-keyed entry — 40 rounds is ~20 KiB of
genuinely immortal source plus its parsed tree, and the rest is the allocator
finding its working set. Bounded by *distinct file
contents*, which is the documented design (backlog E12: keyed on content, never
mtime, so an unchanged leg is served rather than re-parsed), and two orders of
magnitude below the per-keystroke figure in 4.1. Not filed.

### 4.4 CLOSED — the Node server's RSS plateaus, and the cap proves it

Flat over the last 4,500 requests, unchanged after a 10 s idle, and the same
work fits in a 64 MiB V8 old space without an abort (§3.2). Descriptors and
threads never move. Note what this leg does and does not cover: it measures
vilan's standard library **as it runs in JavaScript**, on V8's heap, where the
collector decides what comes back — nothing here is inside Rust's memory model,
and no counter in this repository can see it. RSS was the only instrument
available and its verdict is the weakest of the four in §4; the heap cap is
what makes it worth writing down. Not filed.

### 4.5 What was looked at and found nothing to say about

Stated so the "found nothing" is falsifiable rather than a shrug. The soak
covered: both real corpora at 2,520 analyses each (5,000 measured plus 40
warm-up); both LSP allocation lifetimes; all fifteen `leak_tally` sites (via
`total`, which is the sum of every one of them); 40 watch rebuild rounds; 160
SSE connect/disconnect cycles;
40,000 HTTP requests across two heap configurations; and descriptor, thread and
RSS accounting on three separate processes. It did **not** cover: a real
`vilan-lsp` process over JSON-RPC (§1.4 — no protocol harness exists, and tier 1
drives the same entry point more precisely); the WebAssembly front end
(`WasmEntryText` is content-interned and unreachable from either driver here);
`vilan build`'s own process, which is short-lived by construction; and the deep
heap behind the AST counters, which is §6's job.

## 5. Running it

### 5.1 Tier 1 — the LSP plateau, on real corpora

One command. Release, for the reason `perf-baseline.md` §3 gives: the same run
in debug is roughly eight times longer, and nothing here is a statement about
wall time.

```sh
cd <repo>
VILAN_PERF_KOLT=/path/to/kolt \
VILAN_PERF_WEBSITE=/path/to/vilan-website \
cargo nextest run --release -p vilan-lsp --run-ignored ignored-only \
    -E 'test(leak_soak_corpus_plateaus)' --no-capture > leak.log 2>&1
echo "leak exit: $?"
grep '^LEAK ' leak.log > leak.jsonl
```

`--no-capture` streams the rows. Drop either environment variable to skip that
corpus (it is reported as `LEAK-SKIP`, never a failure). `VILAN_LEAK_SOAK_WINDOW`
sets the analyses per window — the default is 1,000, so 2,000 analyses per
corpus on the inline driver and 500 on the per-thread one.

Every row is one `LEAK {…}` line of JSON — corpus, lines, source bytes, driver,
window index, analyses, the per-site byte counts, the derived bytes-per-analysis
and the RSS growth — so two runs diff as text.

### 5.2 Tier 2 — the two long-lived processes

```sh
cd <repo>
cargo build --release                      # the soak prefers target/release
scripts/soak.sh --rounds 40 --requests 20000 --clients 4 > soak.log 2>&1
echo "soak exit: $?"
```

`scripts/soak.sh --help` lists every option. `--leg watch` or `--leg server`
runs one leg; `--heap-cap 64` re-runs the server leg under a small V8 old-space
cap, which is the cheap discriminator §4 uses; `--keep` leaves the work
directory (fixtures, logs, `soak.jsonl`) in place for inspection.

Exit status is 0 when the soak **ran**. A fixture that would not build, a
server that would not come up, a process that would not die, or a `node` that
outlived the run are the failures — because each of them means there is no
measurement. Nothing in the script asserts a threshold on a curve; §4 is where
a curve gets a verdict.

### 5.3 The gate

Nothing above is in the PR gate, and one small thing is:
`leak_soak_harness_smoke` (§1.3) — a handful of analyses through both drivers.
It joins the `leak_measurement` group, which `.config/nextest.toml` already
schedules first at priority 100 because that group holds the suite's longest
single tests. Measured rather than asserted, on this machine:

| | new smoke | the group's longest | suite Summary |
|---|---:|---:|---:|
| the group alone (5 tests) | 1.005 s | 16.258 s | 37.4 s |
| inside `--workspace`, contended | 2.889 s | 33.581 s | — |
| `--workspace` before this change | — | — | **147.562 s**, 3742 tests, exit 0 |
| `--workspace` after | — | — | **129.691 s**, 3743 tests, exit 0 |

The gate did not get slower. It measured 17.9 s *faster*, which is the box
rather than the change — the honest reading of both rows is that one ~3 s test
scheduled first, against a 33.6 s neighbour in its own priority group, is not
detectable in a 130-second wall. The four shipped plateau fixtures are
unchanged and still run every time; the heavy soak is `#[ignore]`d and shows in
the Summary's skip count (4 → 5).

## 6. What dhat would add, and why it is not here

`dhat-rs` is the whole-heap cross-check `leak_tally`'s own module doc frames
RSS as a poor proxy for. It would be a **new dependency**, which AGENTS.md
makes a stop condition, so this section describes the plug-in rather than
performing it.

**The shape.** `crates/vilan-lsp/Cargo.toml` grows an **optional** `dhat`
dependency and a `dhat-heap = ["dep:dhat"]` feature enabling it — optional
rather than a dev-dependency, because a Cargo feature cannot reference a
dev-dependency and the allocator has to be declared in the crate root either
way. That root grows

```rust
#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOCATOR: dhat::Alloc = dhat::Alloc;
```

and the tier-1 soak wraps its measured windows in a
`let _profiler = dhat::Profiler::new_heap();`, which writes `dhat-heap.json` on
drop for the DHAT viewer. Run as
`cargo nextest run --release -p vilan-lsp --features dhat-heap …`. Nothing
outside that feature changes, and the default build never links it.

**What it would answer that nothing here can.** Three gaps, in order of how
much they matter:

1. **The AST figures are estimates.** `leak_tally`'s own doc says so: the entry
   AST site records a tree-proportional estimate (node count × node size), not
   a deep heap audit, and every cache-bounded AST site records the *shallow*
   `size_of_val` of the leaked box. So §2's `entry-AST` column is a faithful
   proxy for *growth* and an unreliable one for *magnitude*. It is why §4.1
   states its headline in **RSS** — a real measurement of the real process —
   and keeps the counted total beside it as the thing that proves the *rate* is
   flat. dhat would make the counted number a real number too, and would say
   which allocation inside the tree is the expensive one.
2. **Retention is invisible to a leak counter.** Everything `leak_tally` sees
   is a `Box::leak` call. A `HashMap` that grows forever leaks nothing by that
   definition and is a leak by every other. §4.2's five by-design retentions
   were dispositioned by reading each cache's KEY and reasoning about its
   bound — a sound argument, and not a measurement. dhat's at-exit live-block
   report with backtraces would measure them, and would find the same class of
   thing in code nobody thought to read.
3. **Allocations the compiler does not make itself.** A dependency's retention
   is outside every counter in this repository.

**What it would cost.** The dependency, a global-allocator swap that slows the
measured code (so the soak's iteration counts would come down), and dhat's own
memory for backtraces. All three are acceptable *behind an opt-in feature* and
none is acceptable in the default build — which is exactly what the feature
flag is for. Recommended, as the owner's call, and small: the whole change is a
manifest entry, four lines in the crate root, and two in the soak.

## 7. M7 — the fix (2026-08-19)

§4.1 filed the number; this section is the refinement `analysis-reuse.md` §2
left "recorded for the entry", designed before it was built. The goal is the
one sentence the tracker states: **the language server's memory is bounded in
the session** — the entry text and AST of a superseded analysis are reclaimed
when the `Document` replaces it. Not a `drop` (the `Program` is parameterised
on `'static`, and that stays so), and not a lifetime refactor of the compiler
(a stop condition): a document-owned allocation, handed back when the analysis
that borrowed it is gone.

### 7.1 The shape

Three pieces, each small, in the order the bytes flow:

1. **`leak_tally::Leaked<T>`** (`crates/vilan-core/src/leak_tally.rs`) — the
   one new primitive. A `Box::leak` whose site *kept the handle*:
   `Leaked::leak(box, site, bytes)` records `bytes` at `site` exactly as
   `record` does, leaks the box, and returns the handle together with the
   `&'static T` borrow the leaking site needs. The handle has no `Drop`:
   dropping it keeps the leak, which is what every caller that does not opt in
   gets — today's behaviour, byte for byte. `unsafe fn reclaim(self)` frees
   the allocation and calls the new `leak_tally::release(site, bytes)` with the
   bytes it recorded, so a reclaimed site nets to zero *exactly*, estimate or
   not. The `unsafe` is real and carries the whole contract in its doc: every
   reference derived from the borrow `leak` returned must be dead.
2. **`analyze_source_reclaimable`** (`crates/vilan-core/src/lib.rs`) — the
   existing pipeline, returning what it always returned plus the
   `Leaked<Spanned<NodeList<'static>>>` handle to the entry tree it leaked (a
   `None` handle only when no tree was produced). `analyze_source` is now a
   two-line wrapper that calls it and drops the handle, so every other caller —
   the tests, the wasm front end, and the macro world's nested compile (whose
   tree the cached world *must* keep) — is unchanged in behaviour and in
   signature. `Program`'s lifetime is untouched.
3. **`AnalyzedProgram`** (`crates/vilan-lsp/src/document.rs`) — the
   `Document`'s `program` field becomes this pair: the `Option<Program<'static>>`
   and the two `Leaked` handles it borrows from (the text copy
   `analyze_on_this_thread` makes, and the tree from 2). Its `Drop` does the
   ordering in one visible function — drop the program, *then* reclaim the two
   allocations — rather than leaning on field declaration order. Its
   constructor is `unsafe fn`, because that is where the invariant is
   promised: *this program borrows only these two allocations (and immortal
   ones), and nothing else borrows them.* `adopt_analysis` replaces the pair as
   one value, so a superseded analysis's program and its allocations can never
   be separated; a closed document drops the pair; the degraded
   `internal_error` document holds `AnalyzedProgram::none()`.

Nothing about what the server computes changes. Every `Document` query
(hover, completion, symbols, tokens, references, quickfixes, diagnostics)
returns owned values today, and still does; `main.rs` reads
`document.program.as_ref()` transiently and stores nothing from it.

### 7.2 The soundness condition, and the audit

The condition: **no `&'static` borrow into the entry text or the entry AST may
outlive the `Program` built from them.** The `Program` itself is full of such
borrows (`Expr<'src>`, `&'src Span`, `&'src str` keys) and that is fine — it
is dropped first. What had to be proved is that nothing *else* holds one: no
process-global, no thread-local, nothing the server retains outside the
`Document`. Every static and `thread_local!` in `vilan-core`, `vilan-lsp` and
`vilan-embedded-std` was enumerated (`grep` over `static`/`thread_local!`/
`OnceLock`/`Mutex<…>`) and read:

| global | keyed by | holds | an entry borrow? |
|---|---|---|---|
| `BASE_CACHE` (`analyzer.rs`) — resolved pre-entry worlds | `BaseCacheKey`: owned `String`s | `World<'static>`, stored **scrubbed** | **No, by the cache's own invariant.** S3c's store path already had to prove this for its lifetime transmute: `source_texts[0]`, `sources[0]`, `source_hashes[0]` and `generated_by_source[SourceId(0)]` are emptied before the store; the entry's seeded module names (`std::` and `<dep>::`) go through `interned_display_name` *because* "a stored base world must hold no entry-text slices"; `pkg::` seeds, `[service]` blocks and macro-defining text (`contains("macro")`) bypass the cache; the entry's expansion is suppressed in the load region and runs over the world *after* the store; `register_file` stores `String`/`Arc<String>`/`PathBuf`, never the nodes it reads. The snapshot is taken before the entry walk, so the entry's names reach no scope. What was trivially true while the entry was immortal is now load-bearing, and it holds. |
| `parse_clean_cached`'s `CACHE`/`BROKEN` (`lib.rs`) | `u64` content hash | its **own** leaked copy of each module text and tree | No. The LSP entry is parsed directly by `analyze_source`, never through this cache (§4.2 measured 0 B at both sites over 5,000 keystrokes); modules are read from the overlay or disk into owned `String`s and leaked afresh. |
| `ERROR_CACHE` (`load_package_module`) | `u64` | its own leaked copies | No, same shape. |
| macro `WORLDS` | `u64` world key | `World { JsProgram<'static>, HashMap<String,String> }` compiled from a **leaked blanked copy** of the defining file | No — the world's `analyze_source` runs on that leaked copy; its tree is its own (now tallied at `MacroWorldAst`, §7.3). |
| macro `FAILURES` | `u64` | `Arc<Vec<Error>>` | No — `Error { span, msg: String, note: Option<Note{String}> }` is owned. |
| macro `EXPANSIONS`, `PARSES` | `u64` | leaked copies of expansion text and its parse | No. |
| `interned_display_name`'s `NAMES` | `String` | leaked copies | No. |
| `DOCUMENT_OVERLAY` | `PathBuf` | `String` | No. |
| every `thread_local!` (`RESOLVING_GENERICS`, `IN_MACRO_WORLD`, the build/recursion counters, `leak_tally`'s counters) | — | plain data | No. |
| the server's `Backend` maps (`documents`, `semantic_token_cache`, `publish_state`, `line_indices`, `manifests`, `pending`) | `Url`/`PathBuf` | `Document`, owned LSP types, `PublishedDiagnostic` (owned) | No — the only `Program` anywhere is the one inside its `Document`. |

Two further facts the argument uses: `Program` is not `Clone`, and `Document`
is not `Clone` (and `adopt_analysis` destructures it, so `Document` carries
no `Drop` of its own — the pairing lives in `AnalyzedProgram`). The panicked-
analysis path (`analyze` unwinds inside its fence) drops every analyzer local
during the unwind and touches no global with entry data (the base store
happens pre-walk on a scrubbed world), so the tree handle is returned and
reclaimed there too; the narrower outer-fence path (a panic between the leak
and the handle's construction) keeps the tree leaked, as today — once per
panic, which is a bug to fix, not a session rate.

`Send`/`Sync`: `Document` crosses the analysis thread's join and lives in a
`DashMap`; `Leaked<T>` is `Send`/`Sync` iff `T` is, which `str` and the AST
(plain data, no `Rc`/`Cell`) are. The reclaim may run on a different thread
from the leak (the server drops on a runtime thread what an analysis thread
allocated) — fine for the global allocator, and §7.3 says what the
thread-local tally does about it.

### 7.3 The accounting

`leak_tally` keeps its meaning — `record`/`bytes`/`total` are the **gross**
bytes leaked at a site on this thread, so every shipped pin and the soak's
`entry_text == window × source_bytes` claims still read the same numbers —
and grows a counterpart: `release(site, bytes)`, `released(site)`, and
`outstanding(site) -> isize` (= recorded − released) with `outstanding_total`.
Signed, because the counters are thread-local by deliberate design (the module
doc's reason stands) and a release is legitimately cross-thread: the shipped
server's analysis thread records and dies, the runtime thread releases. The
harness reads each thread's counters inside that thread and sums, as it
already did (§1.3), so its per-window `outstanding` is exact; a field report
(`VILAN_LEAK_REPORT`, printed on the analysis thread at the end of `analyze`)
is unchanged in production — that thread reclaims nothing — and appends a
`reclaimed …` clause only on a thread that did.

One site is split: a macro world's blanked entry is analysed through the same
`analyze_source`, so its tree used to land at `EntryAst` alongside the real
entry's. It now records at **`MacroWorldAst`** (bounded by `WORLDS`/`FAILURES`,
like its text and program), so `outstanding(EntryAst)` is exactly the claim
"the document's own tree", zero after *any* document drops, cold world or
warm.

### 7.4 The pins

- `leak_tally`: `Leaked::leak` records and `reclaim` releases the same bytes;
  `outstanding` nets to zero; the report's `reclaimed` clause.
- `vilan-core/tests/module_resolution.rs`: `analyze_source_reclaimable` hands
  back a tree handle whose bytes equal what `EntryAst` recorded, and reclaiming
  it (after the program is dropped) nets the site to zero; a macro-defining
  entry records its world's tree at `MacroWorldAst`, not `EntryAst`.
- `vilan-lsp/document.rs` (platform-independent, the counters need no
  `/proc`): dropping a `Document` reclaims its entry text and AST to the byte;
  **a `Document` that analyzes twice** (`adopt_analysis`, the server's path)
  releases exactly the first analysis's allocations, keeps exactly the
  second's outstanding, and still answers from the adopted program afterward;
  a `Document::analyze` result (own thread) is reclaimed on the thread that
  drops it, visible as a negative outstanding there.
- The Phase-1 pin `per_analysis_leak_is_bounded_by_named_sites` keeps every
  assertion it had and gains the reclaim: after a window whose documents are
  dropped, `outstanding(LspEntryText) == 0` and `outstanding(EntryAst) == 0` —
  renamed `per_analysis_leak_is_bounded_by_named_sites_and_the_entry_is_reclaimed`.
  The soak and its smoke carry the same two columns (`entry_text_outstanding_b`,
  `entry_ast_outstanding_b`) in every `LEAK` row and assert both are 0 on
  both drivers.

Non-vacuity was checked the cheap way: with the two `reclaim` calls in
`AnalyzedProgram::drop` commented out, the document-level pins go red (the
outstanding bytes are the full gross), and the core pin goes red when the
handle is dropped instead of reclaimed.

### 7.5 `parse_clean_cached` — a second, slower session leak (filed)

§4.2 closed `parse_clean_cached` at 0 B for the soak's shape, and that shape
is one open file. The shape it does not cover: **an edited file that another
open document imports.** `did_change` updates the overlay, and after the
edited document's own analysis lands, `reanalyze_dependents` re-analyzes every
open document whose `Program.canonical_sources` contains the edited path
(B39). The dependent's loader reads the edited buffer from the overlay and
parses it through `parse_clean_cached` — content-keyed, so every *distinct*
buffer content is leaked forever (text + tree), and a keystroke is a distinct
content. A buffer that is mid-edit and does not parse clean takes
`load_package_module`'s `ERROR_CACHE` instead — the same shape, one leaked
source + tree + rendered errors per distinct broken content. All open
dependents share one copy per content, so the rate is one (text + tree) of
the *edited* file per landed keystroke, however many dependents are open, and
zero when none is. It is bounded by distinct contents, which in an editor
session is the keystroke count: a session leak, strictly slower than M7 (it
needs a dependent open), of the same order per keystroke when one is.

Measured rather than only reasoned (a throwaway probe in the LSP's test
module, debug, 2026-08-19): `lib.vl` (62 B) open in the overlay and a
`main.vl` importing `pkg::lib`, `lib.vl` rewritten under a fixed-width moving
edit and `main.vl` re-analyzed after each rewrite through `adopt_analysis`,
the server's shape. Over 50 rewrites spanning 27 *new* distinct contents,
`ParseCleanCacheText` grew by exactly 27 × 62 = 1,674 B and
`ParseCleanCacheAst` by 27 × 40 B root boxes (the shallow record; the tree
behind each is the real cost) — one leak per distinct content, none for a
repeated one (50 re-analyses of one content: 0 B at both sites). The same
loop over 32 distinct *broken* contents leaked 32 × 76 B at `ModuleErrorText`
and 2,048 B at `ModuleErrorAst` — `load_package_module`'s error cache, the
same shape. Meanwhile the entry sites stayed at 0 B outstanding throughout,
which is the M7 fix holding under the dependent-reanalysis flow too. Not
fixed here — it is a different mechanism (a process-global content cache
without eviction, also the CLI watch loop's entry reuse) and wants its own
design (an eviction rule for the *previous* content of an open path, or an
LRU by bytes). Filed in the report as a new find for the tracker, with the
probe's shape as the repro.

### 7.6 SHIPPED 2026-08-19 — the soak, re-run

Same machine, same corpora, same command as §5.1 (release; `next` at
410b280d plus this change; 5,040 analyses, **1588.8 s**, exit 0). Every
assertion the soak had still holds — both windows byte-identical at every
site on both drivers, `entry_text == window × source_bytes` — plus the new
one: the outstanding balance at `LspEntryText` and `EntryAst` is **0 B** on
every row, both drivers, after the window's documents dropped.

**The counted leak, per keystroke — §4.1's table with the after column:**

| corpus | driver | gross recorded B/keystroke (unchanged) | outstanding B/keystroke, before | outstanding B/keystroke, **after** |
|---|---|---:|---:|---:|
| `kolt/src/views.vl` (372 lines) | inline (1000 × 2) | 415,377 | 415,377 | **0** |
| | per-thread (250 × 2) | 415,377 | 415,377 | **0** |
| `vilan-website/src/page.vl` (735 lines) | inline (1000 × 2) | 974,768 | 974,768 | **0** |
| | per-thread (250 × 2) | 974,768 | 974,768 | **0** |

The gross column is deliberately the same number as before: every analysis
still leaks one copy of the source and one tree, and gives both back. The
`leak_tally` report's `reclaimed` clause and the `LEAK` rows'
`entry_text_outstanding_b`/`entry_ast_outstanding_b` columns are where the
fix shows.

**RSS per keystroke — §2.2's table with the after column** (report only, as
ever; the instrument is §1.1's):

| corpus | driver | before (w1 / w2) | **after (w1 / w2)** | after 2,000 keystrokes, before → after |
|---|---|---:|---:|---:|
| `views.vl` | inline | 753.0 / 762.1 KiB | **49.4 / 43.7 KiB** | 1.4 GiB → ~90 MiB |
| `views.vl` | per-thread | 803.2 / 766.4 KiB | **104.2 / 39.8 KiB** | |
| `page.vl` | inline | 3,194.9 / 3,190.4 KiB | **1,529.9 / 1,521.8 KiB** | 6.1 GiB → 3.0 GiB |
| `page.vl` | per-thread | 3,178.0 / 3,191.2 KiB | **1,507.7 / 1,550.6 KiB** | |

Two readings, and the second is the important one:

1. **The reclaim is complete for what it names.** On `views.vl` RSS growth fell
   by 94 % and now *decelerates* window over window (49 → 44, 104 → 40),
   which it never did before; a 12-line file (a release probe, 150-analysis
   windows) gives **+0 KiB** of RSS in its second window, with or without a
   `const` site — flat at the page granularity RSS has. The counters say the
   two named sites net to zero and nothing unnamed grew; §7.7's allocator
   split agrees once the second leak is subtracted.
2. **`page.vl` still grows 1.5 MiB per keystroke, and it is not this leak.**
   The counted balance is zero, yet RSS climbs at exactly the same rate in
   both windows and on both drivers — a linear leak `leak_tally` cannot see,
   which the 3.12 MiB it used to sit under made invisible to §2. §7.7 runs
   it down: it is the const pass's evaluator, and `views.vl`'s residual 46 KiB
   is the same mechanism at smaller scale.

### 7.7 FILED — the const pass's evaluator leaves reference cycles, every analysis

Found by the re-run's residual, attributed the same day with glibc's
`mallinfo2` split into the harness (a throwaway probe; release; 100-analysis
windows on `page.vl`, 150 on `views.vl`, 10 warm-up):

| corpus | RSS per analysis | **in use (`uordblks`) per analysis** | free-but-retained (`fordblks`) per window |
|---|---:|---:|---:|
| `views.vl` | 39.9 / 65.4 KiB | **+45.8 / +45.9 KiB** | −1.4 / +2.9 MiB |
| `page.vl` | 1,577.7 / 1,525.4 KiB | **+1,523.9 / +1,523.5 KiB** | +5.6 / −0.1 MiB |

In-use bytes — allocated and never freed — grow at a rate flat to the
kilobyte across windows, and the free-retained side is noise around zero: a
genuine leak, not fragmentation. It is not a `Box::leak` (every site counts
zero net), not an `Arc`/`Rc` outside one module (`grep`: `Rc<` lives only in
`interpreter.rs`), and not a process-global container (§7.2's inventory).

**The mechanism**, read in `crates/vilan-core/src/interpreter.rs`: an
environment is `Rc<RefCell<Scope>>`; a function declaration is hoisted into
its scope as `Value::Closure(Rc<ClosureData { env, .. }>)` whose `env` *is*
that scope — a reference cycle per declared function, in the scope that
declares it. `run_const` (const-eval.md §10.6) builds a fresh root scope per
const site and hoists the site's whole lowered world into it, so every site of
every analysis leaks its root scope with every function closure the world
holds. `page.vl` has 35 `const` sites reaching `std::ui`/`style`; `views.vl`
has none of its own but reaches std code that is const-evaluated
(`style.vl`'s validation is const evaluation). The macro engine's
`run_entry` has the same shape, reached only on an expansion-cache miss, so
it is a per-distinct-expansion cost, not per keystroke.

**Confirmed by the cheap experiment**: one line — `globals.borrow_mut().vars
.clear()` after the result is extracted in `run_const`, breaking the root
scope's cycles — re-measured under the same probe gives **+0.0 KiB in use per
analysis on both corpora** (RSS +0 to +15 KiB per analysis, all of it on the
free-retained side). That is the whole residual. Reverted, not shipped: it is
the root scope only — a function declared inside a function body cycles with
the *call* scope, unreachable from the root after the call returns, so the
general fix is a per-run registry of every scope the evaluator creates
(eight creation sites, all inside `Interpreter` methods), cleared when the
run ends — a contained `interpreter.rs` change (the `Interpreter` struct
gains the run's lifetime) with its own pins (a const-using fixture in the leak
harness whose in-use bytes plateau, and the equivalence gate unchanged,
because teardown runs after the result is plain data). The interpreter is the
native evaluator const-eval.md and macro-engine.md own, so this is their
item, not M7's; filed in the report with these numbers as a new find.

With it fixed, the language server's session memory on `page.vl` would be
flat to the allocator's noise — which is what §7's first sentence asked for
and M7 alone delivers on the smaller file.

### 7.8 M8, SHIPPED 2026-08-20 — the per-run scope registry

§7.7 named the mechanism and the general fix; this section is that fix, built
the way M7 was: the soundness condition proved first, then the change, then
the same soak. The one-line experiment cleared the ROOT scope and could not
reach a function declared inside a called function's body — that closure
cycles with the *call* scope, unreachable from the root once the call returns
— so the shipped shape is the registry §7.7 sketched: the `Interpreter`
records every scope its run creates and, when the run's result is owned plain
data, clears every one of them.

#### The shape

`interpreter.rs` only. §7.7 counted eight scope-creation sites and eight is
right: three roots (`run_const`, `run_program`, `run_entry` — one per public
entry) and five children (the `while` iteration, the `for..of` iteration, the
two `if`-branch bodies, the call frame). All eight now go through two methods
on `Interpreter` — `root_scope` / `child_scope` — and the raw constructors
are gone, so a scope cannot be born unregistered; a ninth creation site added
later is forced through the registry by construction. The registry itself is
`Vec<Weak<RefCell<Scope>>>` (the `Interpreter` struct gains the run's
lifetime for it, as §7.7 said it would): **weak deliberately**, so the
registry never extends a scope's life — a loop that churns ten thousand
iteration scopes sees every one die exactly when it died before, at the cost
of its 8-byte slot, and only a scope a cycle (or the run's own root handle)
still holds is there for the teardown to upgrade and clear. `clear_scopes`
drains the registry and clears each live scope's bindings: the closures drop,
their `env` edges drop, and the whole scope graph unravels leaf-first. Values
the extracted result still holds by `Rc` are unaffected — clearing a scope
severs edges, it destroys nothing that anything else owns.

#### The soundness condition, and why the macro path is in

**No scope-held `Value` may be resolved after the clear.** What makes that
provable in one place is that `Value`, `Scope`, `Env` and `ClosureData` are
private to `interpreter.rs` and the module holds no `static` or
`thread_local!` state that stores them (the one global added here is the pin's
plain-integer counter) — so the only code that could read a scope after the
teardown is the entry function that owns the run, and each of the three tears
down as its LAST act, after its result is extracted to owned plain data:

- `run_const` — the result is `value_to_const`'s `ConstValue` (a deep copy:
  `String`s, `f64`s, owned `Vec`s), the asset lines are `(String, String)`
  pairs built at emit time, stdout is the interpreter's own `String`, and a
  `Failure` is `{ kind, String, Vec<String> }`. The teardown runs on the
  error arms too, which matters: the INFERRED form's capability misses are a
  routine per-site event (§9.2's silent fallback), and each one had built its
  scopes before it failed.
- `run_entry` — **the macro path gets the same registry, deliberately.** Its
  result is the expansion's `String` (`text.to_string()` out of the `Source`
  array); the caches above it (`WORLDS`, `EXPANSIONS`, `FAILURES`) store
  lowered `JsProgram` trees, leaked text and owned errors, never an
  interpreter `Value` — §7.2's inventory, re-read for this — so no later
  expansion can be served out of an earlier run's scopes, and "cleared too
  early" is structurally excluded: the clear is the entry's last statement,
  behind the owned text. §7.7 called the macro leak bounded
  (per-distinct-expansion, the worlds cached), but a session's distinct
  expansions grow with edited macro arguments, so it is the same leak on a
  slower clock, and leaving it would have meant a special case where the
  general fix costs one method call.
- `run_program` — the equivalence suite's entry; `RunOutput` is a `String`
  and an `i32`. Same argument, same teardown.

What the teardown does NOT cover, recorded honestly: a panic that unwinds out
of a run leaves its cycles in place, exactly as M7's outer-fence path leaves
one tree leaked — once per compiler bug, not a session rate. And the lowered
world (`ConstSite.world`, the pass's shared `js::Node` trees) is untouched by
design: scopes hold values, the world is the program, and a later site
re-hoists from it into its own fresh root (§10.6's isolation, unchanged).

#### The pins, and what planting them measured

The mechanism's instrument is a new thread-local counter in `interpreter.rs`,
`live_scope_count()` (scopes created minus scopes dropped — the same
only-a-counter-can-see-it argument as the const pass's lowering counter,
because the teardown is behaviour-neutral by construction). Four pins, each
planted red before shipping:

- `every_interpreter_scope_dies_with_its_const_run` (inference.rs) — a
  three-site fixture covering every cycle shape §7.7 names: hoisted module
  functions, a closure declared inside a called function's body, loop
  iterations between them. Teardown planted out of `run_const`: **4 scopes
  stranded** — exactly the predicted census, three site roots plus the one
  call scope the closure cycles with.
- `every_interpreter_scope_dies_with_its_macro_expansion` (inference.rs) — a
  macro whose body is unique to the fixture (the expansion cache cannot serve
  it unrun). Teardown planted out of `run_entry`: 1 scope, the world's root.
- `a_const_function_evaluates_again_after_an_earlier_sites_scopes_were_cleared`
  (inference.rs) — the behavioural half: a function two sites reach, one
  declaring a closure in its body, correct at the LAST site after the first
  site's teardown already ran, plus an untouched site between them. Planted
  to a teardown that runs before the result extraction: red (the site cannot
  read `__const_result` back).
- `const_evaluations_in_use_bytes_plateau` (document.rs `leak_measurement`) —
  §7.7's `mallinfo2` instrument, moved from the throwaway probe into the
  harness proper: `heap_split_bytes()` reads `(uordblks, fordblks)`, every
  window now records both deltas, and the soak's `LEAK` rows carry them as
  `in_use_grown_b` / `free_retained_grown_b` (`null` off glibc). The pin runs
  a const-heavy synthetic (five sites, closure-in-function, loops) for two
  75-analysis windows and asserts the warm window's in-use growth under 2 KiB
  per analysis — under nextest's process-per-test isolation only, because
  `uordblks` is process-global: under `cargo test`'s in-process threads the
  byte half is report-only (§1.1's reason RSS never gates) and the
  thread-local scope counter carries the assertion. Measured at the shipped shape (debug): teardown in, **+240 /
  +48 B in use per window** — under 4 B per analysis, allocator noise;
  teardown planted out, **+8.4 KiB per analysis, flat to the half-percent
  across both windows** — a leak's exact signature, beside an RSS column too
  noisy to gate on. The 2 KiB cap sits 4× under the planted rate and two
  orders over warm noise. The pin also reads `live_scope_count()` back to
  zero on the measuring thread, so on a non-glibc host the mechanism half
  still gates.

The equivalence gate is untouched and rides the suite: the interpreter must
agree with node on every in-subset corpus program (`tests/interpreter.rs`),
and the macro suites hold expansion behaviour — teardown happens after a
run's result exists, so there is nothing for either to see.

#### The soak, re-run

Same command as §5.1, same corpora (release; `next` at b52514f4 plus this
change; 5,040 analyses, **1522 s**, exit 0 — the box was shared with other
lanes' builds for part of the run, so the wall is context, not a measurement).
`page.vl` has gained a line since §7.6 — 736 lines, 26,988 bytes per
keystroke — so the gross columns moved with it: 975,068 B recorded *and
reclaimed* per analysis, outstanding 0 B on every row, both windows
byte-identical at every site on both drivers. Every assertion §7.6 added
still holds, and every `LEAK` row now carries the two `mallinfo2` columns.

**§7.7's table, with the after column** — in-use (`uordblks`) growth per
analysis, window 1 / window 2 (§7.7's probe was inline-only; both its columns
were windows of that driver):

| corpus | driver | §7.7, before | **after** |
|---|---|---:|---:|
| `kolt/src/views.vl` | inline (1000 × 2) | +45.8 / +45.9 KiB | **+1.0 / +0.05 B** |
| | per-thread (250 × 2) | | **+5.0 / +0.6 B** |
| `vilan-website/src/page.vl` | inline (1000 × 2) | +1,523.9 / +1,523.5 KiB | **+0.3 / +0.1 B** |
| | per-thread (250 × 2) | | **−4.7 / 0.0 B** |

Free-retained stays noise around zero (−2.6 to +3.3 MiB per window), as it
was in §7.7. And RSS, continuing §7.6's after column (KiB per analysis,
w1 / w2): `views.vl` inline 49.4 / 43.7 → **0.9 / 0.3**, per-thread
104.2 / 39.8 → **12.8 / 0.0**; `page.vl` inline 1,529.9 / 1,521.8 →
**0.9 / 0.0**, per-thread 1,507.7 / 1,550.6 → **0.0 / 0.0** — the second
window of every corpus-driver pair now grows zero or near-zero pages, which
RSS had never done on `page.vl` under any prior fix.

So §7's first sentence — *the language server's memory is bounded in the
session* — now holds on the const-heavy file too: what a keystroke makes
immortal is single-digit bytes, the allocator's own noise floor, and the
1.5 MiB §7.6 could only attribute is gone at the mechanism. What remains
open around session memory is §7.5's `parse_clean_cached` shape — a
different mechanism, separately filed, reached only with a dependent open —
and nothing else this soak can see.

### 7.9 M9 — evicting an open path's previous content (design, 2026-08-20; STOPPED before building)

§7.5 filed the leak; backlog M9 asks for the fix, and named the shape to
evaluate first: the cache knows hashes and the LSP knows paths, so core keeps
a small path→hash map beside `parse_clean_cached`, and on a document change
the LSP tells core "path P's content is now H" — core evicts and reclaims the
PRIOR hash it had recorded for P, through M7's `Leaked` machinery. This
section is that design pass, done the way §7 and §7.8 were done — the
soundness condition first, proved against the tree, before any `unsafe`. The
finding is negative and specific: **freeing a shared, content-keyed cache
entry is sound only under a cross-analysis ownership protocol** (per-entry
refcounts or epochs threaded through every loader entry point and both world
caches), which is exactly the not-contained shape the work order names as the
stop condition. So this section is the design and the proof, plus the
mechanism that IS contained and reaches M9's bound — recommended for the
owner's ratification — and no code ships with it.

#### 7.9.1 The measurement, re-run

§7.5's probe was throwaway; re-created in the same shape on today's tree
(`next` at 96b4272b; debug, one process, the counters read on the analyzing
thread). A 40-byte `helper.vl` "open" in the overlay, a `main.vl` importing
`pkg::helper`, the entry re-analyzed after each helper rewrite and landed
through `adopt_analysis` — the server's dependent-reanalysis flow. Warmup of
3 analyses fills std's parses and the base world; each phase then resets the
tally and reads it at the end:

| phase | `ParseCleanCacheText` | `ParseCleanCacheAst` | `ModuleErrorText` | `ModuleErrorAst` | entry sites outstanding |
|---|---:|---:|---:|---:|---:|
| 30 **distinct** clean contents (40 B each) | 1,200 B = 30 × 40 | 1,200 B = 30 × 40 | 0 | 0 | 0 / 0 |
| 30 re-analyses of ONE content | 0 | 0 | 0 | 0 | 0 / 0 |
| 20 **distinct** broken contents (38 B each) | 760 B = 20 × 38 | 0 | 760 B = 20 × 38 | 1,760 B = 20 × 88 | 0 / 0 |

§7.5's finding, byte-exact, still: one text + one tree per DISTINCT content,
nothing for a repeat, the error cache the same shape one seam over, and the
entry sites at zero outstanding throughout (M7 holds under this flow). One
detail §7.5 did not call out, visible in the third row's first column: **a
distinct broken content leaks its text TWICE** — `parse_clean_cached` leaks
the source *before* it knows the parse is clean (the tree borrows it), so a
non-clean content leaves that copy behind at `ParseCleanCacheText` and
`load_package_module`'s rich path then leaks its own at `ModuleErrorText`.
Whatever fixes M9 must cover the pre-cleanliness leak too.

#### 7.9.2 Who borrows a module entry — the inventory the eviction shape runs into

The condition is M7's, one level deeper: the evicted text and tree may be
freed only when **no live borrower exists**, and for a *module* entry the
borrowers are not §7.2's short list. Each of these was read in the tree, not
assumed:

1. **Every live adopted `Program`.** The analyzer pushes the loaded module's
   text straight into `source_texts` (`analyzer.rs`, the load region) and its
   tree's nodes into scopes and `span_map` — a dependent analyzed against the
   old content holds `&'static` slices into it until its next
   `adopt_analysis` or close. `AnalyzedProgram`'s own invariant says it in so
   many words: the program borrows its two handles *"and allocations that are
   immortal (std and module texts in `parse_clean_cached`, interned names,
   cached macro worlds)"*. Eviction deletes the word "immortal" from that
   sentence; every consumer of the invariant has to be re-proved against
   whatever replaces it.
2. **Every in-flight analysis.** Two analyses of one document can be in
   flight at once (`land`'s doc), and dependent sweeps of successive
   generations overlap. An analysis that called `load_package_module(P)`
   before the flip holds the old `LoadedModule` borrows in analyzer locals
   for its whole run — and its result still LANDS, because landing is gated
   on the dependent's own text, not on the imported file's. "Evict at
   `did_change`" frees memory a running analysis is reading; "evict after the
   sweep" is not enough either, because a previous generation's sweep can
   still be mid-flight when this one finishes.
3. **`BASE_CACHE`.** The store's lifetime transmute is *justified by* this
   cache's immortality — the SAFETY comment reads "module/dep texts and ASTs
   live in `parse_clean_cached`'s leaked cache". A stored world borrows every
   recorded std/dep module; dependency-package files are exactly the files a
   multi-package workspace has open in the editor. Validation is per-hit and
   lazy, so a world nobody looks up retains its borrows indefinitely — and
   the sharpest edge is that validation compares CONTENT hashes: an undo that
   returns the file to its prior content makes the stale world *valid* again,
   and it is cloned and analyzed against. Evict that prior content and the
   use-after-free is reached by pressing ctrl-Z.
4. **Macro `WORLDS`.** `compile_world` runs the world's analysis with the
   real `std` plus `macro_std`, loads those modules through this same cache,
   and `Box::leak`s the world's `Program<'static>` — an immortal program
   holding module borrows, keyed by the blanked DEFINING file's content and
   never content-revalidated. The `Arc<World>` is additionally memoized per
   `MacroDef` inside registries that live in stored base worlds and live
   programs (`macro_world_cache_clear`'s doc records exactly this
   reachability), so no purge of the map ends it.
5. **Content aliasing.** The cache is keyed by CONTENT; the proposed rule
   evicts by PATH. Two files with identical content — two empty files, two
   license stubs — share one entry, and evicting P's previous hash frees what
   a live program borrowed via unrelated, unedited Q. No path→hash map can
   see this; attribution has to be by content (which `Program.source_hashes`
   does record, and the transient callers below do not).
6. **Ownerless transient reads.** `module_importables` (E57 import
   completion, on the request thread) and `infer_platform`'s `declares` read
   entries with no recorded lifetime at all; an eviction on another thread
   races them.

#### 7.9.3 What a sound eviction would need, stated so its size is visible

Per-entry reference counts (or epoch stamps — same protocol, coarser grain),
acquired **under the cache lock** at every `parse_clean_cached` and
`ERROR_CACHE` acquisition so a borrow is never unprotected, held by: a
thread-local acquisition scope per analysis, drained into `AnalyzedEntry` and
released by `AnalyzedProgram::drop` (the release set is
`Program.source_hashes`, helpfully already recorded); a ref set per stored
base world, acquired at store, re-acquired into the hitting analysis's scope
under the base-cache lock, released at staleness eviction and `clear()`; a
permanent pin per compiled macro world; and a guard for every transient
caller. Evicted-but-referenced entries wait on a condemned list serviced by
the releases. That is an ownership protocol threaded through every loader
entry point in three crates plus both world caches — the "epoch scheme
across analyses" the work order names as the point to stop and report rather
than build. §7.5's other candidate, an LRU by bytes, needs the identical
protocol: an LRU only chooses WHICH entry to condemn, never when freeing it
is sound — and it would happily condemn a std entry a macro world borrows
forever.

#### 7.9.4 The contained mechanism, recommended: stop sharing what churns

The growth §7.5 measures is the cache faithfully doing its job on contents
that can never recur — keystrokes. What the cache was built for (E12) is
std and dependency modules: stable disk content, reused across every compile
in the process. The contents that churn are exactly the OVERLAY's — open
buffers. So instead of evicting the shared entry, never create it: **a
module read served from the overlay, during an analysis that opted in,
bypasses the process-global caches and parses into analysis-owned
allocations** — `Leaked` handles (text + tree; on the non-clean path text +
tree + rendered errors, which also retires 7.9.1's double-leak), pushed onto
a thread-local collection scope, drained into `AnalyzedEntry` beside the
entry's own two handles, owned by `AnalyzedProgram`, reclaimed in its `Drop`
after the program. This is M7's proven pattern applied one level down, and it
dissolves every hazard in 7.9.2 instead of ordering around them: nothing
shared is ever freed, each analysis owns its copies, supersession reclaims
them, no epochs, no condemned lists, no cross-thread barrier.

The bound it reaches is M9's stated target: outstanding module bytes = the
sum, over open documents, of the overlay-served modules their CURRENT
analysis loaded — the open set — and per-distinct-content growth is zero.
The cost is honest and small: a dependent's analysis re-parses each
overlay-resident import (a handful of files, in the frontend — the cheap
phase), and nothing changes for modules on disk.

The proof obligations the builder owes, each pinned per case:

- **(a) The base-world gate.** The analysis's program must be the ONLY
  borrower of its owned copies, so `base_cache_store` must refuse to store a
  world that loaded any overlay-served source (the collection scope carries
  the flag; the store already runs inside the analysis that owns it). S3c's
  transmute justification gains an explicit clause instead of silently losing
  one. Consequence to record: base-world caching is forfeited while a std or
  dependency file is open in the editor — correct, bounded, and visible in
  the base-cache stats.
- **(b) The macro-world carve-out.** A world outlives every analysis by
  design, so a world compile keeps the global cache — `in_macro_world`
  already marks the region; the loader gains the check. Toolchain content
  edited in an open buffer therefore stays a session leak (per distinct
  content) on that path; out of M9's scope, recorded here.
- **(c) Transient callers keep today's behavior.** With no scope active
  (`module_importables`, `declares`, the CLI, the wasm front end — which
  serves everything from the overlay and must NOT be switched), the global
  cache is used exactly as now. The wasm and CLI paths stay byte-for-byte
  unchanged because activation is the LSP's explicit opt-in on the
  reclaimable entry point, not an ambient property of the overlay.
- **(d) A per-scope path memo**, so a module reached twice in one analysis
  (a lib surface and a direct import) is parsed and owned once.
- **(e) The pins**: distinct-content growth zero at both cache sites with a
  dependent open; outstanding nets to zero when the documents drop; the
  error-path copies reclaimed the same way; a base world not stored while a
  loaded source is overlaid, stored again once it closes; repeated-content,
  no-dependent and multi-dependent shapes; the 7.9.1 probe promoted into the
  harness as the measurement; and an ASan-checked use-after-reclaim plant
  like M7's.

#### 7.9.5 Verdict

Design and stop. The path→prior-hash eviction M9 named is unsound as a
contained change — 7.9.2 items 2 through 5 are each individually fatal, and
the protocol in 7.9.3 that would make it sound is the work order's own stop
condition. The mechanism in 7.9.4 reaches M9's bound with machinery this
paper has already proven twice, but it inverts the loader's ownership story
and conditions the S3c transmute argument — a semantics-level change the
owner should ratify before `unsafe` is written. Nothing shipped with this
section: the probe behind 7.9.1 was run and reverted, and its shape is one
paragraph to re-create.
