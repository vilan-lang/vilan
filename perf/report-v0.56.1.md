# Performance report — v0.56.1

**The most expensive phase is `checks`: 31% of the generated app's thread CPU** — the first place to optimize next.

Verdict **green** at `e9c25ab056`, 2026-11-16, load 1.1. kolt `vilan check`: CPU x1.005, peak RSS x1.009, instructions x1.001 against the previous release (peak RSS judged at x1.05, CPU and instructions at x1.1).

## T2 — instruction counts (instructions:u, class reference)

| subject | count | previous | ratio |
|---|---:|---:|---:|
| example:math | 254,903,536 | — | — |
| example:watch | 316,413,200 | — | — |
| example:browser | 1,382,573,045 | — | — |
| example:fullstack | 2,896,359,275 | — | — |
| example:router | 1,510,810,425 | — | — |
| example:reactive-ui | 1,539,704,591 | — | — |
| example:ssr | 3,297,847,237 | — | — |
| example:canvas | 1,395,807,414 | — | — |
| example:rpc | 1,581,111,500 | — | — |
| example:todo | 3,957,272,507 | — | — |
| example:walkthrough | 4,156,300,467 | — | — |
| genapp:46 | 7,516,636,655 | — | — |
| plain:160 | 2,926,581,849 | — | — |
| plain:320 | 5,862,174,649 | — | — |

## E121 — the editor's targets

| edit | CPU to diagnostics | target | state |
|---|---:|---:|---|
| leaf keystroke (diagnostics_cpu_ms) | 1090 ms | 1500 ms | green |
| shared.vl keystroke (diagnostics_cpu_ms) | 350 ms | 500 ms | green |
| model.vl keystroke (diagnostics_cpu_ms) | 590 ms | 600 ms | green |
| model.vl keystroke, importers open (cpu_ms) | 2150 ms | 3000 ms | green |
| css keystroke (diagnostics_cpu_ms) | 1220 ms | 1300 ms | green |
| parse break (diagnostics_cpu_ms) | 1930 ms | 2500 ms | green |

Rows measured (8): leaf keystroke, leaf keystroke + pause, shared.vl keystroke, model.vl keystroke, model.vl keystroke, importers open, css keystroke, parse break, parse repair.

> **Series break, leaf keystroke:** Order 56 (N186): target RULED 2026-11-03 by the owner (interim, ~1.1x the quiet estimate; re-taken at the Order 56 seal; the 500 ms mandate stays the arc's goal) - the anchors are unchanged; the exhibit moved from kolt@46c5b59 (the Order 55 seal's pinned copy) to the owner's kolt@2684c70 (2026-11-02, organizations, editor, documents, tasks, desktop: about three times the program), so the figures start a new series.

> **Series break, shared.vl keystroke:** Order 56 (N186): target RULED 2026-11-03 by the owner (interim, ~1.1x the quiet estimate; re-taken at the Order 56 seal; the 500 ms mandate stays the arc's goal) - the anchors are unchanged; the exhibit moved from kolt@46c5b59 (the Order 55 seal's pinned copy) to the owner's kolt@2684c70 (2026-11-02, organizations, editor, documents, tasks, desktop: about three times the program), so the figures start a new series.

> **Series break, model.vl keystroke:** Order 56 (N186): target RULED 2026-11-03 by the owner (interim, ~1.1x the quiet estimate; re-taken at the Order 56 seal; the 500 ms mandate stays the arc's goal) - re-anchored in the same method body, `Channel::create` - the keystroke is at `get_client().get_safe()!.create_channel(org, name)` (the org parameter arrived with organizations, kolt@38a7ac9), hover on `create_channel(org, name)`, and the Find References probe moved from `fun global()` (gone) to `fun ids(talk` (`Channel::ids`, the mirrored store's key-set reader, used in four files none open); the exhibit moved from kolt@46c5b59 (the Order 55 seal's pinned copy) to the owner's kolt@2684c70 (2026-11-02, organizations, editor, documents, tasks, desktop: about three times the program).

> **Series break, model.vl keystroke, importers open:** Order 56 (N186): target RULED 2026-11-03 by the owner (interim, ~1.1x the quiet estimate; re-taken at the Order 56 seal; the 500 ms mandate stays the arc's goal) - the model.vl keystroke's new anchor (`create_channel(org, name)` in `Channel::create`) with client.vl, channel.vl and views.vl open beside it as before; the exhibit moved from kolt@46c5b59 (the Order 55 seal's pinned copy) to the owner's kolt@2684c70 (2026-11-02, organizations, editor, documents, tasks, desktop: about three times the program).

> **Series break, css keystroke:** Order 56 (N186): target RULED 2026-11-03 by the owner (interim, ~1.1x the quiet estimate; re-taken at the Order 56 seal; the 500 ms mandate stays the arc's goal) - the anchors are unchanged; the exhibit moved from kolt@46c5b59 (the Order 55 seal's pinned copy) to the owner's kolt@2684c70 (2026-11-02, organizations, editor, documents, tasks, desktop: about three times the program), so the figures start a new series.

> **Series break, parse break:** Order 56 (N186): target RULED 2026-11-03 by the owner (interim, ~1.1x the quiet estimate; re-taken at the Order 56 seal; the 500 ms mandate stays the arc's goal) - re-anchored from `channel_name.adopt(channel.current_name());` (gone from channel.vl with organizations, kolt@38a7ac9) to `let name = Channel { id }.current_name(g.talk());` in the same channel.vl (the hover card's channel arm) - a statement over the `talk.channels().at(id)` boundary, the `(` opened before it as before; the repair row (`parse repair`) is the undo of the same edit; the exhibit moved from kolt@46c5b59 (the Order 55 seal's pinned copy) to the owner's kolt@2684c70 (2026-11-02, organizations, editor, documents, tasks, desktop: about three times the program).

Growth: plain:160 -> plain:320: x2.003 per doubling (the gate is x2.3).
