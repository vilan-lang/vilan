# The performance baseline — the harness, and the first numbers (M1)

> **Status: SHIPPED 2026-08-18.** The harness is
> `crates/vilan-cli/tests/perf_baseline.rs` (the compiler phases and the
> end-to-end package checks) plus `crates/vilan-lsp/src/document.rs`'s
> `perf_baseline` module (the editor's edit latency), both `#[ignore]`d so
> the PR gate never pays for them, both pinned by a seconds-long smoke test
> that does. No new dependency: `std::time::Instant`, repeated runs, min and
> median. Every number in §2 was measured on the dev machine on 2026-08-18
> against `next` at 86ad2128 (post-v0.34.0) plus this change — absolute
> milliseconds are a fact about **that machine and that profile**; the
> ratios, and the reference units, are the part that travels.

This paper is the record M1 asked for. §1 is what the harness measures and
how; §2 is the baseline; §3 is how to run it and how to compare two runs;
§4 is what the first run found, including the one number nobody expected.

## 1. The harness

### 1.1 What is measured

Three sections, because the compiler is asked three different questions and
they have three different answers.

**Section 1 — the four phases**, called as the library entry points they
already are, and timed individually:

| phase | the call |
|---|---|
| `parse` | `parsing::parse`, then the `elements::rewrite_items` and `lift::rewrite_items` desugars (`lib.rs:409-410`) |
| `analyze` | `analyzer::analyze` (`analyzer.rs:33215`) |
| `post_passes` | `post_analysis_passes` (`lib.rs:492-541`) |
| `transform` | `transformer::transform` (`transformer.rs:17`) |

That is the same seam `VILAN_PHASE_TIMING` marks (`lib.rs:553-595`,
`crates/vilan-core/tests/phase_timing.rs`) and the same sequence
`analyze_source` composes (`lib.rs:289-317`) — the harness calls the pieces
rather than the composition, which is the only reason it can attribute the
wall to one of them.

**Section 2 — end to end**: `vilan check <package>` spawned exactly as the
suite spawns the binary, so each measurement carries process startup, binary
load, and a genuinely cold process. `check` rather than `build` on purpose:
it writes nothing, which is what lets the sibling corpora be measured where
they live instead of being copied. Reported in milliseconds **and in units
of a freshly measured reference compile** — the convention
`crates/vilan-cli/tests/support/mod.rs` established for the suite's liveness
bounds (`reference_compile()` / `run_liveness()`), for the reason recorded
there: a ratio survives a change of machine and a fixed second does not.

**Section 3 — edit latency**: p50/p95/p99 over synthetic single-keystroke
edits driven through `Document::analyze_on_this_thread`, reusing the
warmup/measured shape of the `leak_measurement` module it sits beside. That
module answers *does a keystroke leak*; this one answers *what does a
keystroke cost*. It lives in `vilan-lsp` because `analyze_on_this_thread` is
private to that crate, and a benchmark is not a reason to widen an API.

### 1.2 How cold and warm are controlled

Explicitly, per iteration, and that is the point of the design rather than a
detail of it. Three process-global caches make a second compile of the same
input a *different measurement* from the first: the resolved pre-entry world
(`BASE_CACHE`, `analyzer.rs:32920`), the expanded macro worlds, and
`parse_clean_cached`'s content-keyed store of module texts and ASTs
(`lib.rs:218-264`). A harness that ignores them reports whichever mixture
its loop happened to produce — the exact drift `suite-speed.md` §2.1/E26
recorded, where a mechanism was credited with a wall the CPU accounting had
never confirmed.

So:

- **cold** clears `analyzer::base_cache_clear()` and
  `macro_world_cache_clear()` before every timed iteration. The module
  discovery, load, walk and resolve are all paid again.
- **warm** clears nothing, and runs one throwaway compile first so the
  measured window is genuinely warm rather than "the first sample is cold
  and the rest are not".

One honest gap, stated rather than smoothed over: `parse_clean_cached` has
no clearer, so **cold here means world-cold, parse-warm** — the module
re-load and re-resolve are measured, the module re-lex and re-parse are not.
Two consequences follow, and both are handled:

1. Without care, the *first* subject measured in a process would be charged
   every module's parse and the later ones would inherit it — a cold table
   that ranks subjects by position. The harness therefore **primes**: one
   throwaway cold compile of every subject before any of them is measured,
   so all subjects start from the same parse-cache footing. (The first
   attempt at this table, before priming, had the 119-line todo app costing
   **3.0×** the 943-line kolt app cold — 6.15 s against 2.03 s — purely
   because todo was measured first. That was the artifact, not a finding.)
2. The genuinely-cold shape — a fresh process with nothing parsed — is what
   Section 2 measures, one process per run. Closing the in-process gap would
   want a `#[doc(hidden)] pub fn parse_clean_cache_clear()` beside its two
   siblings; that is filed as backlog M6 rather than taken here.

### 1.3 The corpora, and why each one is in

| corpus | size | what it is | why it is here |
|---|---|---|---|
| `tiny` | 5 lines | `import std::print;` and a `main` | **The unit.** Not representative of anything — the same role `support::reference_compile`'s project plays for the suite — so every other number reads as a multiple of the smallest compile the toolchain can do. |
| `std_wide` | 27 lines, importing 23 `std` modules | synthetic, in-repo | **The cold whole-world compile.** `vilan/std` (57 files, 15,024 lines) is the only corpus in this repository big enough to stand for one, and reaching it through an *entry* measures what a user's first build measures — the discovery, load, walk and resolve their imports drag in — rather than the artificial act of "compiling std". |
| `todo_server` | 22-line entry, 119-line app | `vilan-playground/todo` | The smallest real fullstack package: reactive + rpc + ui, almost no user code. It is here to separate *program size* from *std reach*, and it does (§4.4). |
| `kolt_server` | 289-line entry, 943-line app | `kolt` | The reference application: db, http, rpc, three entries. |
| `website_server` | 162-line entry, 2,996-line app | `vilan-website` | The heaviest package anyone builds, and the style/view-heavy one. It is where the surprise is. |
| `kolt_views`, `website_page` | 372 / 716 lines | the same two packages | Edit-latency subjects: real files a person actually types in. |

The three sibling packages are addressed by environment variable
(`VILAN_PERF_TODO`, `VILAN_PERF_KOLT`, `VILAN_PERF_WEBSITE`) and **skipped,
not failed**, when absent — they live in checkouts a fresh clone of this
repository does not have. The two in-repo subjects always run, which is what
keeps the harness meaningful on a machine that has only this tree.

Section 1 uses each package's `server.vl` **entry** rather than a module: an
entry has a `main`, so `transform` produces real JavaScript and the fourth
phase is measured rather than skipped. The library-phase subjects are entry
files that compile standalone against `std` plus their own `pkg::` modules,
with `pkg_root` set to the package's `src` and `Workspace::default()` — none
of the three declares a dependency, so no manifest resolution has to be
re-implemented inside the harness.

### 1.4 Where it runs

Two `#[ignore]`d tests, in a new `crates/vilan-cli/tests/perf_baseline.rs`
and in `crates/vilan-lsp/src/document.rs` beside `leak_measurement`. Least
ceremony that puts each measurement in the crate that owns the code it
measures, and `#[ignore]` is what keeps a minutes-long benchmark out of a
gate that already costs what it costs.

Four things run in the normal gate, and nothing else — together under two
seconds:

- `perf_baseline_harness_smoke` (vilan-cli) — the whole harness on the
  `tiny` subject at one cold iteration, two warm, and one end-to-end
  reference check, asserting that every row it emits is internally
  consistent (ordered order statistics, finite timings, JSON-safe fields).
- `perf_baseline_lsp_harness_smoke` (vilan-lsp) — three analyses, asserting
  the sample set is non-empty, sorted, and non-zero.
- `perf_baseline_summary_reports_the_order_statistics_it_names` and
  `perf_baseline_lsp_percentiles_are_nearest_rank` — the statistics
  themselves, over a hundred *known* durations. These exist because the
  smoke runs cannot do this job and it was proven they cannot: planting
  `1.0 - fraction` in both percentile functions left both smoke tests green
  (at one to three samples every rank collapses onto the same value) and
  turned both of these red. A benchmark whose percentiles read the wrong end
  of the distribution is worse than no benchmark.

Measured, because "it does not slow the gate" is exactly the kind of claim
this paper exists to stop people making: `cargo nextest run --workspace` on
this tree, same machine, same session — **149.8 s / 3688 tests before,
139.8 s / 3692 tests after**. The four new tests and the one new test binary
cost nothing detectable; the 10 s difference is the build cache, not the
change. (CLAUDE.md's cited ~64 s figure is a different, faster box — the
comparison that means anything is before-and-after on one machine.)

**No threshold assertion belongs in the gate, and that is a decision, not an
omission.** The suite's own history is the argument: E32/E39/E40
(`suite-speed.md` §5–§7) are three separate incidents of a clock wrapped
around a compile inside the gate, each failing on CI runners that take ~54
minutes for a suite this box runs in ~2.5, and each fixed by replacing the
clock with a *liveness* bound denominated in a measured reference. A
relative-threshold regression check would be the same bet with better
manners: on a 4-core shared runner, mid-suite, with 16 test processes
interleaved, the noise band around a 30 % regression is wider than 30 %. The
gate pins that the *instrument* works; the instrument is pointed at the tree
deliberately, on a quiet machine, by a person who wants an answer.

## 2. The baseline

Measured 2026-08-18 on the dev machine:

| | |
|---|---|
| CPU | AMD Ryzen 7 9800X3D, 8 cores / 16 threads (`nproc` = 16) |
| OS | WSL2, Linux 6.18.33.1-microsoft-standard-WSL2 |
| RAM | 23 GiB |
| tree | `next` at 86ad2128 (post-v0.34.0) + this change |
| profile | `release` unless the row says otherwise |
| load | idle (nothing else running) |

### 2.1 The four phases — median milliseconds, release

Cold: `BASE_CACHE` and the macro worlds cleared before each iteration, 5
runs. Warm: nothing cleared, 15 runs after a throwaway warm-up.

| corpus | mode | parse | analyze | post_passes | transform | **total** |
|---|---|---:|---:|---:|---:|---:|
| tiny | cold | 0.1 | 11.8 | 1.4 | 0.1 | **13.7** |
| tiny | warm | 0.1 | 5.5 | 1.2 | 0.1 | **6.9** |
| std_wide | cold | 0.1 | 91.8 | 9.2 | 0.3 | **100.0** |
| std_wide | warm | 0.1 | 21.7 | 7.4 | 0.2 | **29.8** |
| todo_server | cold | 0.3 | 96.3 | 10.1 | 11.1 | **118.9** |
| todo_server | warm | 0.2 | 32.2 | 9.9 | 8.9 | **51.6** |
| kolt_server | cold | 1.8 | 103.3 | 11.9 | 12.3 | **129.6** |
| kolt_server | warm | 1.8 | 41.7 | 12.4 | 13.2 | **68.2** |
| website_server | cold | 1.0 | 105.5 | 204.9 | 20.8 | **332.9** |
| website_server | warm | 1.6 | 63.1 | 206.3 | 19.1 | **292.1** |

### 2.2 End to end — `vilan check`, cold process, 5 runs, release

| package | lines | median | min | reference units |
|---|---:|---:|---:|---:|
| reference (`std::print` only) | 5 | 35.4 ms | 34.4 ms | 1.0 |
| todo | 119 | 232.3 ms | 230.6 ms | **6.6** |
| kolt | 943 | 350.2 ms | 337.6 ms | **9.9** |
| website | 2,996 | 845.8 ms | 828.2 ms | **23.9** |

`check` on a multi-entry package checks every entry — todo 2, kolt 3,
website 3 — so these are whole-package numbers, not per-entry ones.

### 2.3 Edit latency — warm, `Document::analyze_on_this_thread`, release

| document | lines | runs | p50 | p95 | p99 | max |
|---|---:|---:|---:|---:|---:|---:|
| synthetic (std, no macros) | 15 | 2000 | 10.0 ms | 13.3 ms | 16.9 ms | 25.1 ms |
| `kolt/src/views.vl` | 372 | 100 | 78.4 ms | 88.0 ms | 94.5 ms | 95.2 ms |
| `vilan-website/src/page.vl` | 716 | 100 | **321.5 ms** | 337.8 ms | 345.0 ms | 348.6 ms |

### 2.4 The debug cross-check

The gate builds debug, so the smoke pins measure debug; the tables above are
release. The same harness, same machine, same session, the debug run
immediately after the release one:

| measurement | debug | release | ratio |
|---|---:|---:|---:|
| `tiny` cold total | 132.3 ms | 13.7 ms | 9.7× |
| `website_server` warm total | 2491.7 ms | 292.1 ms | 8.5× |
| `vilan check` website, end to end | 6601.9 ms | 845.8 ms | 7.8× |
| `vilan check` reference (the unit) | 199.8 ms | 35.4 ms | 5.6× |
| LSP synthetic keystroke, p50 | 77.3 ms | 10.0 ms | 7.7× |

**Never compare a debug row against a release row**, and note that the
*reference units* do not rescue you either: the website is 23.9 units in
release and **33.0** in debug, because the reference compile is
startup-heavy and startup is the part `-O0` slows least. Reference units
travel across machines; they do not travel across profiles. Every row
carries `profile` for exactly this reason.

## 3. Running it, and comparing two runs

One command. It builds the workspace in release first, which is the slow
part on a cold target directory:

```sh
cd <repo>
VILAN_PERF_TODO=/path/to/vilan-playground/todo \
VILAN_PERF_KOLT=/path/to/kolt \
VILAN_PERF_WEBSITE=/path/to/vilan-website \
cargo nextest run --release --workspace --run-ignored ignored-only \
    -E 'test(perf_baseline)' --no-capture > perf.log 2>&1
echo "perf exit: $?"
grep '^PERF ' perf.log > perf.jsonl
```

`--no-capture` is what streams the rows and what makes nextest run the two
tests serially — a benchmark measured beside another benchmark is measuring
the scheduler. `--run-ignored ignored-only` selects exactly the two heavy
tests and leaves the smoke pins out. Drop `--release` only to reproduce
§2.4's debug column; drop the environment variables to run the two in-repo
subjects alone. The measurement itself takes **84 s** on this machine with
all five subjects present (the release build is extra, and one-time); the
same run in debug takes about **fifteen minutes**, most of it the two
real-file latency subjects, which is the other reason the recorded table is
release.

Every measurement is one `PERF {…}` line of JSON — section, corpus, mode,
metric, profile, runs, and min/median/p95/p99/max in milliseconds — so
`perf.jsonl` diffs as text and the rows from both test binaries concatenate
into one file. A human table follows the JSON in the log for reading.

**The recorded baseline is checked in** as
[`perf-baseline.jsonl`](perf-baseline.jsonl) beside this file — the 57 rows
behind §2, plus a header line naming the date, tree, machine and profile.
That is the thing a future run diffs against; §2's tables are the same
numbers rounded for a reader.

**Comparing.** Line up `perf.jsonl` against `perf-baseline.jsonl` on
(`section`, `corpus`, `mode`, `metric`), check the `profile` fields match,
and compare `median_ms` as a ratio. Read `min_ms` as the machine's best case
and the median as what a caller waits for; ignore a moved `max_ms` on its
own, which is usually the scheduler. For a cross-machine comparison use the
`reference units` in the end-to-end notes instead of milliseconds — that is
what the unit is for.

**Attributing a change to a pass** inside `post_analysis_passes` currently
means patching `Instant` marks into it by hand and reverting them, which is
how §4.3 below was measured. That is filed as backlog M5.

## 4. What the first run found

### 4.1 Parsing is not on the map

The largest parse in the whole baseline is 1.8 ms (kolt's 289-line entry),
and the largest share of a compile it ever takes is **2.6 %** (kolt warm,
the shortest compile with the longest entry). On the website it is 0.5 %,
on `std_wide` 0.3 %. This is `suite-speed.md` §9's "It was never the
parsing" restated with the four-phase split behind it: the handwritten
frontend costs nothing worth measuring, and any future compile-speed work
that starts at the lexer is starting in the wrong place.

### 4.2 The post-analysis passes are immune to warmth

Warming the caches buys `analyze` between 40 % and 76 %:

| corpus | analyze cold → warm | post_passes cold → warm |
|---|---|---|
| tiny | 11.8 → 5.5 ms (−53 %) | 1.4 → 1.2 ms |
| std_wide | 91.8 → 21.7 ms (−76 %) | 9.2 → 7.4 ms |
| todo_server | 96.3 → 32.2 ms (−67 %) | 10.1 → 9.9 ms |
| kolt_server | 103.3 → 41.7 ms (−60 %) | 11.9 → 12.4 ms |
| website_server | 105.5 → 63.1 ms (−40 %) | 204.9 → **206.3** ms |

`analysis-reuse.md`'s base cache does what it says. The post-passes do not
benefit at all — they are whole-program recompute over the entry's reachable
world every single time, warm or cold, which is why on the website they go
from 62 % of a cold compile to **71 % of a warm one**. Every language-server
keystroke pays that in full.

### 4.3 It is `const_eval`, and G2's 7–9 % does not generalize

Attributed by patching per-pass `Instant` marks into `post_analysis_passes`
and running `vilan check` once per corpus (debug profile — the split is a
proportion, and the proportion is the claim):

| pass | website | kolt | todo | reference |
|---|---:|---:|---:|---:|
| `const_eval::evaluate` | **4207.8 ms** | 62.0 ms | 19.9 ms | 1.2 ms |
| `async_infer::infer` | 252.3 ms | 256.3 ms | 193.4 ms | 10.9 ms |
| call graph build | 198.0 ms | 134.2 ms | 106.6 ms | 3.8 ms |
| `platform_color::check` | 62.4 ms | 44.4 ms | 14.8 ms | 0.1 ms |
| `init_order::check_cycles` | 3.3 ms | 1.6 ms | 1.4 ms | 0.1 ms |
| `check_view_suspensions` | 0.0 ms | 0.0 ms | 0.0 ms | 0.0 ms |

The const pass is **89 % of everything the post-passes do on the website,
and about 64 % of the whole `vilan check` wall** (6.60 s, §2.4's debug
row — the split and the wall are the same profile). It is not proportional
to program size — the website pays **68× kolt for 3× the source**. What
separates them is `const`-marked style work (`std::style`'s compiled
atomics: 186 `const` sites across the website's sources, 81 of them in
`art.vl` alone), so the suspect is per-site evaluation cost or a missing
memo, not the pass's fixed overhead.

`const-eval.md` §8 records G2's measured **7–9 %** warm-analysis const cost.
That figure was taken on a corpus that does not look like this one and
should not be read as the general number. Filed as **backlog M4**, and it is
the first thing to profile: `check` pays it in full while emitting nothing,
and so does every keystroke.

### 4.4 Compile cost tracks `std` reach, not user line count

todo's entry is 22 lines and its whole app is 119; kolt's entry is 289 lines
and its app is 943. Cold, they cost **118.9 ms** and **129.6 ms** — 8 %
apart across an 8× difference in source. The 5-line `tiny` subject costs
13.7 ms cold with nothing in it but a `print`, and `std_wide` — 27 lines,
all of them `import` — costs 100.0 ms. What a program costs to compile is
mostly what its imports drag in.

This is the measured form of the "import-cost cliff" `suite-speed.md` §8
found from the other end, and it says where the leverage is: the reachable
world, not the user's file.

### 4.5 The editor's budget is real, and one real file overruns it 2.1×

The language server debounces at 150 ms (`editing-dx.md`), so that is the
budget a keystroke has before the next one is already waiting. A small file
answers in 10.0 ms at p50 with a 25 ms worst case over 2000 keystrokes —
comfortable. kolt's 372-line `views.vl` answers in 78.4 ms — over half the
budget, still inside it. The website's 716-line `page.vl` answers in
**321.5 ms at p50**, and its *best* sample over 100 keystrokes is 278 ms:
every keystroke in that file misses the budget, none of them narrowly.

The distribution is not the problem — p99 is 1.68× p50 on the synthetic
subject, 1.20× on kolt's file and **1.07×** on the website's, which is a
very tight tail; the work is simply that expensive every time, not
occasionally. And §4.2 says why
the file's size is not the whole story: the post-passes recompute over the
reachable world per keystroke and never warm up, and `page.vl` is the
style-heavy file M4 is about. A fix for M4 is an editor-latency fix.

### 4.6 What this run did NOT measure, and is worth measuring next

- **`vilan build` end to end.** Only `check` is measured, because the three
  package corpora are read-only and `build` writes `dist/`. The
  `transform` column of §2.1 is the emission cost measured in-process
  instead, which is the same work without the file writes.
- **A second machine.** Every absolute number here is one box. The
  reference units in §2.2 exist so the next machine's run means something.
- **CPU accounting.** `suite-speed.md` §2.1's correction — attribute a wall
  to a mechanism only after the CPU accounting confirms it — is honoured
  here by §4.3's direct per-pass split, not by a `user`-time check. A
  profiler run over the website's const pass is the natural next step, and
  belongs to M4 rather than to this record.

## 5. Filing candidates

Three, all filed against `backlog-2026-08-18.md` §M:

- **M4** — `const_eval::evaluate` is two thirds of a style-heavy compile
  (§4.3). The largest single finding.
- **M5** — `VILAN_PHASE_TIMING` cannot see inside `post_analysis_passes`;
  the split that found M4 had to be hand-patched (§3, §4.3).
- **M6** — the in-process cold measurement cannot clear
  `parse_clean_cached`, so "cold" is world-cold, parse-warm (§1.2).
