# Performance report — v0.56.0

**The most expensive phase is `load+walk`: 29% of the generated app's thread CPU** — the first place to optimize next.

Verdict **green** at `eca174d439`, 2026-11-16, load 1.2. kolt `vilan check`: CPU x1.015, peak RSS x1.040, instructions x0.999 against the previous release (peak RSS judged at x1.05, CPU and instructions at x1.1).

> **Note:** the two sides checked DIFFERENT sources: the base kolt@c7454052, the tip the prepared tree `kolt-tip` — a breaking release, whose base source does not check under the tip (nor the migrated tree under the base). Each side's check was clean; the ratios compare the same app before and after its migration, not one text.

> **Note:** the LSP harness replayed its edit script over DIFFERENT sources: the base kolt@c7454052, the tip the prepared tree `kolt-tip` — a breaking release; each row compares the same edit in the same app before and after its migration

## T2 — instruction counts (instructions:u, class reference)

| subject | count | previous | ratio |
|---|---:|---:|---:|
| example:math | 254,905,987 | — | — |
| example:watch | 316,415,026 | — | — |
| example:browser | 1,382,575,320 | — | — |
| example:fullstack | 2,896,342,147 | — | — |
| example:router | 1,510,779,691 | — | — |
| example:reactive-ui | 1,539,651,227 | — | — |
| example:ssr | 3,297,770,717 | — | — |
| example:canvas | 1,395,806,305 | — | — |
| example:rpc | 1,581,050,614 | — | — |
| example:todo | 3,957,661,982 | — | — |
| example:walkthrough | 4,156,775,338 | — | — |
| genapp:46 | 7,516,332,480 | — | — |
| plain:160 | 2,925,840,440 | — | — |
| plain:320 | 5,860,675,755 | — | — |

## E121 — the editor's targets

| edit | CPU to diagnostics | target | state |
|---|---:|---:|---|
| leaf keystroke (diagnostics_cpu_ms) | 1120 ms | 1500 ms | green |
| shared.vl keystroke (diagnostics_cpu_ms) | 350 ms | 500 ms | green |
| model.vl keystroke (diagnostics_cpu_ms) | 580 ms | 600 ms | green |
| model.vl keystroke, importers open (cpu_ms) | 2120 ms | 3000 ms | green |
| css keystroke (diagnostics_cpu_ms) | 1230 ms | 1300 ms | green |
| parse break (diagnostics_cpu_ms) | 1980 ms | 2500 ms | green |

Rows measured (8): leaf keystroke, leaf keystroke + pause, shared.vl keystroke, model.vl keystroke, model.vl keystroke, importers open, css keystroke, parse break, parse repair.

> **Series break, leaf keystroke:** Order 56 (N186): target RULED 2026-11-03 by the owner (interim, ~1.1x the quiet estimate; re-taken at the Order 56 seal; the 500 ms mandate stays the arc's goal) - the anchors are unchanged; the exhibit moved from kolt@46c5b59 (the Order 55 seal's pinned copy) to the owner's kolt@2684c70 (2026-11-02, organizations, editor, documents, tasks, desktop: about three times the program), so the figures start a new series.

> **Series break, shared.vl keystroke:** Order 56 (N186): target RULED 2026-11-03 by the owner (interim, ~1.1x the quiet estimate; re-taken at the Order 56 seal; the 500 ms mandate stays the arc's goal) - the anchors are unchanged; the exhibit moved from kolt@46c5b59 (the Order 55 seal's pinned copy) to the owner's kolt@2684c70 (2026-11-02, organizations, editor, documents, tasks, desktop: about three times the program), so the figures start a new series.

> **Series break, model.vl keystroke:** Order 56 (N186): target RULED 2026-11-03 by the owner (interim, ~1.1x the quiet estimate; re-taken at the Order 56 seal; the 500 ms mandate stays the arc's goal) - re-anchored in the same method body, `Channel::create` - the keystroke is at `get_client().get_safe()!.create_channel(org, name)` (the org parameter arrived with organizations, kolt@38a7ac9), hover on `create_channel(org, name)`, and the Find References probe moved from `fun global()` (gone) to `fun ids(talk` (`Channel::ids`, the mirrored store's key-set reader, used in four files none open); the exhibit moved from kolt@46c5b59 (the Order 55 seal's pinned copy) to the owner's kolt@2684c70 (2026-11-02, organizations, editor, documents, tasks, desktop: about three times the program).

> **Series break, model.vl keystroke, importers open:** Order 56 (N186): target RULED 2026-11-03 by the owner (interim, ~1.1x the quiet estimate; re-taken at the Order 56 seal; the 500 ms mandate stays the arc's goal) - the model.vl keystroke's new anchor (`create_channel(org, name)` in `Channel::create`) with client.vl, channel.vl and views.vl open beside it as before; the exhibit moved from kolt@46c5b59 (the Order 55 seal's pinned copy) to the owner's kolt@2684c70 (2026-11-02, organizations, editor, documents, tasks, desktop: about three times the program).

> **Series break, css keystroke:** Order 56 (N186): target RULED 2026-11-03 by the owner (interim, ~1.1x the quiet estimate; re-taken at the Order 56 seal; the 500 ms mandate stays the arc's goal) - the anchors are unchanged; the exhibit moved from kolt@46c5b59 (the Order 55 seal's pinned copy) to the owner's kolt@2684c70 (2026-11-02, organizations, editor, documents, tasks, desktop: about three times the program), so the figures start a new series.

> **Series break, parse break:** Order 56 (N186): target RULED 2026-11-03 by the owner (interim, ~1.1x the quiet estimate; re-taken at the Order 56 seal; the 500 ms mandate stays the arc's goal) - re-anchored from `channel_name.adopt(channel.current_name());` (gone from channel.vl with organizations, kolt@38a7ac9) to `let name = Channel { id }.current_name(g.talk());` in the same channel.vl (the hover card's channel arm) - a statement over the `talk.channels().at(id)` boundary, the `(` opened before it as before; the repair row (`parse repair`) is the undo of the same edit; the exhibit moved from kolt@46c5b59 (the Order 55 seal's pinned copy) to the owner's kolt@2684c70 (2026-11-02, organizations, editor, documents, tasks, desktop: about three times the program).

Growth: plain:160 -> plain:320: x2.003 per doubling (the gate is x2.3).

## Bumps since the last release

- example:walkthrough x1.01: Order 58 (db-58 M175/F149/M195, http-58 F151/F152, peer-58 F163): std grew for every program - the SqlValue trait and the optional row readers in std::sqlite, ServerTimeouts and the keep-alive surface in std::http, Request::peer_address; every ci row rose x1.0006-x1.0031 and the walkthrough, the widest reader of std, crossed its limit (CI read x1.0052 of the ceiling at next 09032e8f where the limit stood at x1.0050; x1.0037 at 46ff5439, x1.0005 at 9e09acf0 before db-58) (M175)
- example:ssr x1.01: Order 58 (the std growth the example:walkthrough row carries, plus M199's cell-borrow analysis, about x1.0025 on every row): CI read x1.0057 of the ceiling at next 7ca039b0 where the limit stood at x1.0050 (M199)
- example:todo x1.01: Order 58 (the std growth the example:walkthrough row carries, plus M199's cell-borrow analysis, about x1.0025 on every row): CI read x1.0056 of the ceiling at next 7ca039b0 where the limit stood at x1.0050 (M199)
- example:canvas x1.01: Order 58 (the std growth the example:walkthrough row carries, plus M199's cell-borrow analysis, about x1.0025 on every row): CI read x1.0049 of the ceiling at next 7ca039b0 where the limit stood at x1.0050 - under it, bumped with the two red rows so the row does not flip on the next small change (M199)
- example:fullstack x1.01: Order 58 (the std growth the example:walkthrough row carries, plus M199's cell-borrow analysis, about x1.0025 on every row): CI read x1.0047 of the ceiling at next 7ca039b0 where the limit stood at x1.0050 - under it, bumped with the two red rows so the row does not flip on the next small change (M199)
- example:watch x1.01: Order 58 (the std growth the example:walkthrough row carries, plus M199's cell-borrow analysis, about x1.0025 on every row): CI read x1.0042 of the ceiling at next 7ca039b0 where the limit stood at x1.0050 - under it, bumped with the two red rows so the row does not flip on the next small change (M199)
- example:browser x1.01: Order 58 (typed elements, native-ui.md section 12): callgrind against next e15985d2 (before typed elements): x1.0006 at next 85ce6095, x1.0011 with M202's fix; a build with the element recording taken out entirely read the same, so the residual is not the per-head work (most likely code layout under one codegen unit, M116) (M202)
- genapp:46 x1.01: Order 58 (typed elements, native-ui.md section 12): callgrind against next e15985d2 (before typed elements): x1.0054 at next 85ce6095, x1.0008 with M202's fix (no tag resolved in a world with no [element] function, heads recorded by a push, Function back to 264 bytes); about x1.0034 of the ceiling, under the limit, but more than +0.05% over the pre-merge count (M202)
- example:reactive-ui x1.01: Order 58 (typed elements, native-ui.md section 12): callgrind against next e15985d2 (before typed elements): x1.0008 at next 85ce6095, x1.0012 with M202's fix (the fix leaves these rows where next had them, so their residual is not the per-head work) (M202)
- example:router x1.01: Order 58 (typed elements, native-ui.md section 12): callgrind against next e15985d2 (before typed elements): x1.0012 at next 85ce6095, x1.0012 with M202's fix (the fix leaves these rows where next had them, so their residual is not the per-head work) (M202)
- example:rpc x1.01: Order 58 (typed elements, native-ui.md section 12): callgrind against next e15985d2 (before typed elements): x1.0013 at next 85ce6095, x1.0011 with M202's fix (the fix leaves these rows where next had them, so their residual is not the per-head work) (M202)
- genapp:46 x1.01: Order 58 (B740, a miscompile fix: a hidden context parameter is bare only where every entry has the value, 67372fb4): callgrind on one machine 7,492,274,724 at next 9cf4669e to 7,613,935,515 with the fix (x1.0162). The context analysis itself moved by 3.6M; the rest is downstream of 815 strict but uncovered functions in genapp-46 that now hold an Option and take a `Some(..)` wrap or a `None` argument per call, where the base passed `undefined`. That is the cost of passing the right value; M204 asks whether the wraps can be cheaper (M204)
- genapp:46 x1.01: Order 58's seal (next ceb5b6ff against the ceiling of v0.54.0's seal): x1.0176 in instructions:u. The order's costs on this subject, each measured when it landed: typed elements x1.0008 (M202), B740's Option contexts x1.0162 to x1.0170 (a miscompile fix; 815 strict but uncovered functions in genapp-46 now take a wrap or a None per call), B742 and B741 x1.0007. Every other reference row is inside its limit (x1.0045 to x1.0094) (M204)
