# Performance report — v0.44.0

**The most expensive phase is `checks`: 33% of the generated app's thread CPU** — the first place to optimize next.

Verdict **red** at `9fab0ff56c`, 2026-10-04, load 1.0. kolt `vilan check`: CPU x0.976, peak RSS x0.805, instructions x0.993 against the previous release.

> **Note:** the two sides checked DIFFERENT sources: the base kolt@984a1dfb, the tip a prepared, migrated copy of kolt (its working tree with the release's patches and `vilan check --fix` applied) — a breaking release, whose base source does not check under the tip (nor the migrated tree under the base). Each side's check was clean; the ratios compare the same app before and after its migration, not one text.

> **Note:** the LSP harness replayed its edit script over DIFFERENT sources: the base kolt@984a1dfb, the tip a prepared, migrated copy of kolt (its working tree with the release's patches and `vilan check --fix` applied) — a breaking release; each row compares the same edit in the same app before and after its migration

## T2 — instruction counts (instructions:u, class reference)

| subject | count | previous | ratio |
|---|---:|---:|---:|
| example:math | 244,224,708 | — | — |
| example:watch | 309,363,040 | — | — |
| example:browser | 1,486,114,036 | — | — |
| example:fullstack | 3,228,797,664 | — | — |
| example:router | 1,890,657,466 | — | — |
| example:reactive-ui | 2,596,618,385 | — | — |
| example:ssr | 4,167,258,820 | — | — |
| example:canvas | 1,493,793,696 | — | — |
| example:rpc | 2,107,177,143 | — | — |
| example:todo | 6,076,795,095 | — | — |
| example:walkthrough | 6,354,501,097 | — | — |
| genapp:46 | 9,873,163,127 | — | — |
| plain:160 | 2,895,500,701 | — | — |
| plain:320 | 5,616,441,323 | — | — |

## E121 — the editor's targets

| edit | CPU to diagnostics | target | state |
|---|---:|---:|---|
| leaf keystroke (diagnostics_cpu_ms) | 860 ms | 500 ms | red (reporting) |
| model.vl keystroke (diagnostics_cpu_ms) | 220 ms | 500 ms | green |
| model.vl keystroke, importers open (cpu_ms) | 1480 ms | 500 ms | red (reporting) |
| css keystroke (diagnostics_cpu_ms) | 940 ms | 500 ms | red (reporting) |
| parse break (diagnostics_cpu_ms) | 970 ms | 500 ms | red (reporting) |

Growth: plain:160 -> plain:320: x1.940 per doubling (the gate is x2.3).

## Bumps since the last release

- example:reactive-ui x1.07: F78's trait defaults are instantiated per pipe stage type (+4.7% here; the browser example is 6.1% cheaper and kolt 0.3%), A152's combinators (+1.6%), and the order's other lanes (about 1.6%). Approved by the owner at Order 46's seal. (M119)
- example:todo x1.02: std grew: A152's combinators (+1.5%) and A150's states() (+1.4%), which this program does not call; F78 gives 0.9% back. Approved by the owner at Order 46's seal. (M120)
- example:walkthrough x1.03: std grew in reactive-46 (+2.5% in all: A152, A150) plus the order's other lanes (about 1.3%). Approved by the owner at Order 46's seal. (M120)
