# Performance report — v0.45.0

**The most expensive phase is `checks`: 33% of the generated app's thread CPU** — the first place to optimize next.

Verdict **green** at `19e7819852`, 2026-10-08, load 0.7. kolt `vilan check`: CPU x0.730, peak RSS x0.995, instructions x0.618 against the previous release.

## T2 — instruction counts (instructions:u, class reference)

| subject | count | previous | ratio |
|---|---:|---:|---:|
| example:math | 251,513,271 | — | — |
| example:watch | 316,712,608 | — | — |
| example:browser | 1,496,301,752 | — | — |
| example:fullstack | 3,216,944,647 | — | — |
| example:router | 1,764,167,515 | — | — |
| example:reactive-ui | 1,970,615,826 | — | — |
| example:ssr | 4,087,327,915 | — | — |
| example:canvas | 1,503,981,459 | — | — |
| example:rpc | 2,097,702,063 | — | — |
| example:todo | 5,323,096,056 | — | — |
| example:walkthrough | 5,485,296,221 | — | — |
| genapp:46 | 9,674,007,789 | — | — |
| plain:160 | 2,925,332,567 | — | — |
| plain:320 | 5,669,043,572 | — | — |

## E121 — the editor's targets

| edit | CPU to diagnostics | target | state |
|---|---:|---:|---|
| leaf keystroke (diagnostics_cpu_ms) | 300 ms | 500 ms | green |
| model.vl keystroke (diagnostics_cpu_ms) | 200 ms | 500 ms | green |
| model.vl keystroke, importers open (cpu_ms) | 600 ms | 500 ms | red (reporting) |
| css keystroke (diagnostics_cpu_ms) | 400 ms | 500 ms | green |
| parse break (diagnostics_cpu_ms) | 430 ms | 500 ms | green |

Growth: plain:160 -> plain:320: x1.938 per doubling (the gate is x2.3).

## Bumps since the last release

- example:reactive-ui x1.07: F78's trait defaults are instantiated per pipe stage type (+4.7% here; the browser example is 6.1% cheaper and kolt 0.3%), A152's combinators (+1.6%), and the order's other lanes (about 1.6%). Approved by the owner at Order 46's seal. (M119)
- example:todo x1.02: std grew: A152's combinators (+1.5%) and A150's states() (+1.4%), which this program does not call; F78 gives 0.9% back. Approved by the owner at Order 46's seal. (M120)
- example:walkthrough x1.03: std grew in reactive-46 (+2.5% in all: A152, A150) plus the order's other lanes (about 1.3%). Approved by the owner at Order 46's seal. (M120)
- example:reactive-ui x1.012: the example is written in element syntax: static text is a quoted child, which lowers to `child(str)` and is selected through `Slot`'s arms where `.text(..)` was a direct method (about +0.15% per text child, measured on router), +1.1% in all (2,596,270,210 -> 2,625,641,035 on docs-47's base). (K26)
- example:router x1.012: the example is written in element syntax: static text is a quoted child, which lowers to `child(str)` and is selected through `Slot`'s arms where `.text(..)` was a direct method (three of them alone cost +0.47%; the reactive hole costs nothing over `bind_text`), +1.2% in all (1,890,823,958 -> 1,912,645,325 on docs-47's base). (K26)
- example:math x1.025: a program this small is nearly all fixed cost, and the fixed cost grew in Order 47: S0's `[track_caller]` pass and location arguments plus the larger always-loaded `debug.vl` (+2.0%, debug-47), `Items<T>` and its blanket over every iterator (+0.5%, store-47), and the copy passes (+0.3%, perf-47); x1.0308 at the Order 47 seal (244,001,446 -> 251,515,912). (M120)
- example:watch x1.015: the same fixed cost as math on another small program (debug-47's S0 and `debug.vl` +1.9%); x1.0237 at the Order 47 seal (309,382,444 -> 316,715,566). (M120)
