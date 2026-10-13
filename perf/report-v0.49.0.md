# Performance report — v0.49.0

**The most expensive phase is `checks`: 34% of the generated app's thread CPU** — the first place to optimize next.

Verdict **green** at `070653d22f`, 2026-10-13, load 0.7. kolt `vilan check`: CPU x0.948, peak RSS x0.991, instructions x0.879 against the previous release (peak RSS judged at x1.05, CPU and instructions at x1.1).

## T2 — instruction counts (instructions:u, class reference)

| subject | count | previous | ratio |
|---|---:|---:|---:|
| example:math | 248,297,097 | — | — |
| example:watch | 313,744,712 | — | — |
| example:browser | 1,367,227,221 | — | — |
| example:fullstack | 2,868,667,667 | — | — |
| example:router | 1,529,551,859 | — | — |
| example:reactive-ui | 1,641,505,275 | — | — |
| example:ssr | 3,442,824,206 | — | — |
| example:canvas | 1,374,920,947 | — | — |
| example:rpc | 1,762,687,423 | — | — |
| example:todo | 4,303,095,758 | — | — |
| example:walkthrough | 4,425,443,881 | — | — |
| genapp:46 | 7,663,855,657 | — | — |
| plain:160 | 2,825,635,904 | — | — |
| plain:320 | 5,452,498,036 | — | — |

## E121 — the editor's targets

| edit | CPU to diagnostics | target | state |
|---|---:|---:|---|
| leaf keystroke (diagnostics_cpu_ms) | 580 ms | 500 ms | red (reporting) |
| shared.vl keystroke (diagnostics_cpu_ms) | 200 ms | 500 ms | green |
| model.vl keystroke, importers open (cpu_ms) | 700 ms | 500 ms | red (reporting) |
| css keystroke (diagnostics_cpu_ms) | 790 ms | 500 ms | red (reporting) |

Growth: plain:160 -> plain:320: x1.930 per doubling (the gate is x2.3).
