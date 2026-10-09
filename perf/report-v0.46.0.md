# Performance report — v0.46.0

**The most expensive phase is `checks`: 33% of the generated app's thread CPU** — the first place to optimize next.

Verdict **green** at `e12a83a4ae`, 2026-10-09, load 0.8. kolt `vilan check`: CPU x0.907, peak RSS x0.993, instructions x0.823 against the previous release.

## T2 — instruction counts (instructions:u, class reference)

| subject | count | previous | ratio |
|---|---:|---:|---:|
| example:math | 256,501,430 | — | — |
| example:watch | 323,198,913 | — | — |
| example:browser | 1,450,646,599 | — | — |
| example:fullstack | 3,070,421,860 | — | — |
| example:router | 1,694,294,616 | — | — |
| example:reactive-ui | 1,846,849,513 | — | — |
| example:ssr | 3,829,587,271 | — | — |
| example:canvas | 1,455,262,510 | — | — |
| example:rpc | 2,018,583,086 | — | — |
| example:todo | 5,015,326,748 | — | — |
| example:walkthrough | 5,086,895,866 | — | — |
| genapp:46 | 8,757,545,783 | — | — |
| plain:160 | 3,048,349,231 | — | — |
| plain:320 | 6,112,448,283 | — | — |

## E121 — the editor's targets

| edit | CPU to diagnostics | target | state |
|---|---:|---:|---|
| leaf keystroke (diagnostics_cpu_ms) | 250 ms | 500 ms | green |
| shared.vl keystroke (diagnostics_cpu_ms) | 190 ms | 500 ms | green |
| model.vl keystroke (diagnostics_cpu_ms) | 200 ms | 500 ms | green |
| model.vl keystroke, importers open (cpu_ms) | 340 ms | 500 ms | green |
| css keystroke (diagnostics_cpu_ms) | 350 ms | 500 ms | green |
| parse break (diagnostics_cpu_ms) | 360 ms | 500 ms | green |

Growth: plain:160 -> plain:320: x2.005 per doubling (the gate is x2.3).

## Bumps since the last release

- example:math x1.025: layout-48's platform fences (std's single-platform modules declare `[platform] mod self;`; the fence walk's twin stand-in rule; the declared-module leg) cost the smallest programs +0.5-0.6%: x1.0056 at the layout-48 merge (263,195,415 -> 264,677,058) on top of the order's earlier +0.1%. (F28)
- example:watch x1.025: the same fence cost on the other small program: x1.0061 at the layout-48 merge (330,268,645 -> 332,275,226). (F28)
- example:todo x1.01: x1.0052 at the layout-48 merge (5,485,862,397 -> 5,514,603,202): the fence rules on a program with a server leg. (F28)
- example:math x1.025: the smallest program carries Order 48's fixed cost: layout-48's platform fences, debug-48's larger debug.vl and E275's derive, solver-48's tuple impls (+1.4% there); x1.0199 at the Order 48 seal (251,513,271 -> 256,506,997) while every larger program is 0.90-0.98 of its ceiling. (M120)
- example:watch x1.025: the same fixed cost on the other small program; x1.0205 at the Order 48 seal (316,712,608 -> 323,203,198). (M120)
