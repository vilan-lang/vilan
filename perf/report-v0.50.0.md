# Performance report — v0.50.0

**The most expensive phase is `checks`: 31% of the generated app's thread CPU** — the first place to optimize next.

Verdict **green** at `99b6013ed4`, 2026-10-13, load 1.0. kolt `vilan check`: CPU x0.975, peak RSS x1.003, instructions x0.938 against the previous release (peak RSS judged at x1.05, CPU and instructions at x1.1).

## T2 — instruction counts (instructions:u, class reference)

| subject | count | previous | ratio |
|---|---:|---:|---:|
| example:math | 253,309,219 | — | — |
| example:watch | 315,885,761 | — | — |
| example:browser | 1,357,571,318 | — | — |
| example:fullstack | 2,828,515,825 | — | — |
| example:router | 1,520,786,392 | — | — |
| example:reactive-ui | 1,626,337,835 | — | — |
| example:ssr | 3,393,753,538 | — | — |
| example:canvas | 1,365,846,373 | — | — |
| example:rpc | 1,740,224,790 | — | — |
| example:todo | 4,210,143,329 | — | — |
| example:walkthrough | 4,315,348,241 | — | — |
| genapp:46 | 7,677,315,266 | — | — |
| plain:160 | 2,854,594,735 | — | — |
| plain:320 | 5,506,989,705 | — | — |

## E121 — the editor's targets

| edit | CPU to diagnostics | target | state |
|---|---:|---:|---|
| leaf keystroke (diagnostics_cpu_ms) | 470 ms | 500 ms | green |
| shared.vl keystroke (diagnostics_cpu_ms) | 170 ms | 500 ms | green |
| model.vl keystroke (diagnostics_cpu_ms) | 190 ms | 500 ms | green |
| model.vl keystroke, importers open (cpu_ms) | 620 ms | 500 ms | red (reporting) |
| css keystroke (diagnostics_cpu_ms) | 750 ms | 500 ms | red (reporting) |
| parse break (diagnostics_cpu_ms) | 740 ms | 500 ms | red (reporting) |

Rows measured (8): leaf keystroke, leaf keystroke + pause, shared.vl keystroke, model.vl keystroke, model.vl keystroke, importers open, css keystroke, parse break, parse repair.

> **Series break, model.vl keystroke:** Order 52 (N174): the keystroke edit is unchanged; the Find References probe moved from `fun messages(self)` (gone from model.vl) to `fun global()`, the mirrored store reader, so that probe starts a new series.

> **Series break, parse break:** Order 52 (N174): re-anchored from `let messages = channel.messages();` (gone with the mirrored-store exhibit, kolt@dee494ab) to `channel_name.adopt(channel.current_name());` in the same channel.vl - a statement over a `g.channels()` boundary; the break and its repair are the same shape, the figures start a new series.

Growth: plain:160 -> plain:320: x1.929 per doubling (the gate is x2.3).

## Bumps since the last release

- example:math x1.025: std-52's std growth, M120's class: B621's six marker `Eq` impls (bool, List, Option, Result, HashSet, HashMap) and B623's array `Default`/`Json`/`FromJson` impls with the `default_of`/`from_json_value_of` helpers live in std modules every program loads, and the smallest example pays them whole; x1.0182 callgrind on CI run 38453647717 against the v0.49.0 ceiling (the lane's ci-local.sh perf has no ceilings in its local class). Re-taken at the Order 52 seal. (M120)
- example:watch x1.01: the same std growth on the second-smallest example: x1.0044 of its ceiling on CI run 38453647717 against the 0.5% ci tolerance - one more std line would red it. Re-taken at the Order 52 seal. (M120)
- example:todo x1.01: B618: an `async` now seeds `std::task` (the home of `async e`'s `Task<T>`), and todo spawns without importing it, so it loads `task.vl` and what it imports once: x1.0077 instructions on the lane's box against next @dcfdcba1 (4,209.9 M -> 4,242.3 M); every other row within 0.08 %. Re-taken at the Order 52 seal. (B618)
- example:todo x1.01: B618's std::task seed, as the ci row above (x1.0077 on the lane's box). (B618)
- example:math x1.02: the same std growth as the ci rows above (B621's Eq impls, B623's array impls; B618's seeded std::task is not on math's path), on the reference box's instructions:u: x1.0110 of the v0.49.0 ceiling at the Order 52 seal on 57677552 against the 1% reference tolerance. Re-taken at the cut. (M120)
