# Performance report — v0.55.0

**The most expensive phase is `checks`: 36% of the generated app's thread CPU** — the first place to optimize next.

Verdict **red** at `302d355aa8`, 2026-11-09, load 1.6. kolt `vilan check`: CPU x1.000, peak RSS x0.972, instructions x0.997 against the previous release (peak RSS judged at x1.05, CPU and instructions at x1.1).

## T2 — instruction counts (instructions:u, class reference)

| subject | count | previous | ratio |
|---|---:|---:|---:|
| example:math | 253,818,568 | — | — |
| example:watch | 314,194,431 | — | — |
| example:browser | 1,378,016,795 | — | — |
| example:fullstack | 2,878,228,566 | — | — |
| example:router | 1,505,233,955 | — | — |
| example:reactive-ui | 1,533,253,556 | — | — |
| example:ssr | 3,275,183,153 | — | — |
| example:canvas | 1,391,178,929 | — | — |
| example:rpc | 1,572,411,614 | — | — |
| example:todo | 3,934,408,015 | — | — |
| example:walkthrough | 4,120,418,478 | — | — |
| genapp:46 | 7,387,924,648 | — | — |
| plain:160 | 2,918,127,499 | — | — |
| plain:320 | 5,845,685,688 | — | — |

## E121 — the editor's targets

| edit | CPU to diagnostics | target | state |
|---|---:|---:|---|
| leaf keystroke (diagnostics_cpu_ms) | 1170 ms | 1500 ms | green |
| shared.vl keystroke (diagnostics_cpu_ms) | 360 ms | 500 ms | green |
| model.vl keystroke (diagnostics_cpu_ms) | 700 ms | 600 ms | RED (blocking) |
| model.vl keystroke, importers open (cpu_ms) | 2410 ms | 3000 ms | green |
| css keystroke (diagnostics_cpu_ms) | 1410 ms | 1300 ms | RED (blocking) |
| parse break (diagnostics_cpu_ms) | 2230 ms | 2500 ms | green |

Rows measured (8): leaf keystroke, leaf keystroke + pause, shared.vl keystroke, model.vl keystroke, model.vl keystroke, importers open, css keystroke, parse break, parse repair.

> **Series break, leaf keystroke:** Order 56 (N186): target RULED 2026-11-03 by the owner (interim, ~1.1x the quiet estimate; re-taken at the Order 56 seal; the 500 ms mandate stays the arc's goal) - the anchors are unchanged; the exhibit moved from kolt@46c5b59 (the Order 55 seal's pinned copy) to the owner's kolt@2684c70 (2026-11-02, organizations, editor, documents, tasks, desktop: about three times the program), so the figures start a new series.

> **Series break, shared.vl keystroke:** Order 56 (N186): target RULED 2026-11-03 by the owner (interim, ~1.1x the quiet estimate; re-taken at the Order 56 seal; the 500 ms mandate stays the arc's goal) - the anchors are unchanged; the exhibit moved from kolt@46c5b59 (the Order 55 seal's pinned copy) to the owner's kolt@2684c70 (2026-11-02, organizations, editor, documents, tasks, desktop: about three times the program), so the figures start a new series.

> **Series break, model.vl keystroke:** Order 56 (N186): target RULED 2026-11-03 by the owner (interim, ~1.1x the quiet estimate; re-taken at the Order 56 seal; the 500 ms mandate stays the arc's goal) - re-anchored in the same method body, `Channel::create` - the keystroke is at `get_client().get_safe()!.create_channel(org, name)` (the org parameter arrived with organizations, kolt@38a7ac9), hover on `create_channel(org, name)`, and the Find References probe moved from `fun global()` (gone) to `fun ids(talk` (`Channel::ids`, the mirrored store's key-set reader, used in four files none open); the exhibit moved from kolt@46c5b59 (the Order 55 seal's pinned copy) to the owner's kolt@2684c70 (2026-11-02, organizations, editor, documents, tasks, desktop: about three times the program).

> **Series break, model.vl keystroke, importers open:** Order 56 (N186): target RULED 2026-11-03 by the owner (interim, ~1.1x the quiet estimate; re-taken at the Order 56 seal; the 500 ms mandate stays the arc's goal) - the model.vl keystroke's new anchor (`create_channel(org, name)` in `Channel::create`) with client.vl, channel.vl and views.vl open beside it as before; the exhibit moved from kolt@46c5b59 (the Order 55 seal's pinned copy) to the owner's kolt@2684c70 (2026-11-02, organizations, editor, documents, tasks, desktop: about three times the program).

> **Series break, css keystroke:** Order 56 (N186): target RULED 2026-11-03 by the owner (interim, ~1.1x the quiet estimate; re-taken at the Order 56 seal; the 500 ms mandate stays the arc's goal) - the anchors are unchanged; the exhibit moved from kolt@46c5b59 (the Order 55 seal's pinned copy) to the owner's kolt@2684c70 (2026-11-02, organizations, editor, documents, tasks, desktop: about three times the program), so the figures start a new series.

> **Series break, parse break:** Order 56 (N186): target RULED 2026-11-03 by the owner (interim, ~1.1x the quiet estimate; re-taken at the Order 56 seal; the 500 ms mandate stays the arc's goal) - re-anchored from `channel_name.adopt(channel.current_name());` (gone from channel.vl with organizations, kolt@38a7ac9) to `let name = Channel { id }.current_name(g.talk());` in the same channel.vl (the hover card's channel arm) - a statement over the `talk.channels().at(id)` boundary, the `(` opened before it as before; the repair row (`parse repair`) is the undo of the same edit; the exhibit moved from kolt@46c5b59 (the Order 55 seal's pinned copy) to the owner's kolt@2684c70 (2026-11-02, organizations, editor, documents, tasks, desktop: about three times the program).

Growth: plain:160 -> plain:320: x2.003 per doubling (the gate is x2.3).
