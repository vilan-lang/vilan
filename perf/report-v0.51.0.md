# Performance report — v0.51.0

**The most expensive phase is `checks`: 32% of the generated app's thread CPU** — the first place to optimize next.

Verdict **green** at `9716b72fa9`, 2026-10-16, load 1.0. kolt `vilan check`: CPU x1.006, peak RSS x0.916, instructions x0.971 against the previous release (peak RSS judged at x1.05, CPU and instructions at x1.1).

## T2 — instruction counts (instructions:u, class reference)

| subject | count | previous | ratio |
|---|---:|---:|---:|
| example:math | 250,190,375 | — | — |
| example:watch | 312,124,276 | — | — |
| example:browser | 1,330,572,454 | — | — |
| example:fullstack | 2,773,188,947 | — | — |
| example:router | 1,449,723,044 | — | — |
| example:reactive-ui | 1,559,323,849 | — | — |
| example:ssr | 3,117,145,554 | — | — |
| example:canvas | 1,338,608,120 | — | — |
| example:rpc | 1,571,179,502 | — | — |
| example:todo | 3,866,819,166 | — | — |
| example:walkthrough | 3,992,720,534 | — | — |
| genapp:46 | 7,413,799,197 | — | — |
| plain:160 | 2,840,509,239 | — | — |
| plain:320 | 5,480,174,120 | — | — |

## E121 — the editor's targets

| edit | CPU to diagnostics | target | state |
|---|---:|---:|---|
| leaf keystroke (diagnostics_cpu_ms) | 410 ms | 500 ms | green |
| shared.vl keystroke (diagnostics_cpu_ms) | 170 ms | 500 ms | green |
| model.vl keystroke (diagnostics_cpu_ms) | 170 ms | 500 ms | green |
| model.vl keystroke, importers open (cpu_ms) | 490 ms | 500 ms | green |
| css keystroke (diagnostics_cpu_ms) | 650 ms | 500 ms | red (reporting) |
| parse break (diagnostics_cpu_ms) | 610 ms | 500 ms | red (reporting) |

Rows measured (8): leaf keystroke, leaf keystroke + pause, shared.vl keystroke, model.vl keystroke, model.vl keystroke, importers open, css keystroke, parse break, parse repair.

> **Series break, model.vl keystroke:** Order 52 (N174): the keystroke edit is unchanged; the Find References probe moved from `fun messages(self)` (gone from model.vl) to `fun global()`, the mirrored store reader, so that probe starts a new series.

> **Series break, parse break:** Order 52 (N174): re-anchored from `let messages = channel.messages();` (gone with the mirrored-store exhibit, kolt@dee494ab) to `channel_name.adopt(channel.current_name());` in the same channel.vl - a statement over a `g.channels()` boundary; the break and its repair are the same shape, the figures start a new series.

Growth: plain:160 -> plain:320: x1.929 per doubling (the gate is x2.3).
