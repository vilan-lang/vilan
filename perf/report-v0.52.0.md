# Performance report — v0.52.0

**The most expensive phase is `checks`: 30% of the generated app's thread CPU** — the first place to optimize next.

Verdict **green** at `6c460ca4dd`, 2026-10-17, load 0.8. kolt `vilan check`: CPU x0.988, peak RSS x0.993, instructions x0.968 against the previous release (peak RSS judged at x1.05, CPU and instructions at x1.1).

## T2 — instruction counts (instructions:u, class reference)

| subject | count | previous | ratio |
|---|---:|---:|---:|
| example:math | 246,054,728 | — | — |
| example:watch | 307,292,719 | — | — |
| example:browser | 1,311,684,500 | — | — |
| example:fullstack | 2,713,464,567 | — | — |
| example:router | 1,416,572,330 | — | — |
| example:reactive-ui | 1,463,358,948 | — | — |
| example:ssr | 3,054,378,898 | — | — |
| example:canvas | 1,319,862,769 | — | — |
| example:rpc | 1,539,280,769 | — | — |
| example:todo | 3,697,041,969 | — | — |
| example:walkthrough | 3,851,874,960 | — | — |
| genapp:46 | 7,143,071,807 | — | — |
| plain:160 | 2,799,773,692 | — | — |
| plain:320 | 5,402,625,880 | — | — |

## E121 — the editor's targets

| edit | CPU to diagnostics | target | state |
|---|---:|---:|---|
| leaf keystroke (diagnostics_cpu_ms) | 330 ms | 500 ms | green |
| shared.vl keystroke (diagnostics_cpu_ms) | 140 ms | 500 ms | green |
| model.vl keystroke (diagnostics_cpu_ms) | 160 ms | 500 ms | green |
| model.vl keystroke, importers open (cpu_ms) | 450 ms | 500 ms | green |
| css keystroke (diagnostics_cpu_ms) | 600 ms | 500 ms | red (reporting) |
| parse break (diagnostics_cpu_ms) | 620 ms | 500 ms | red (reporting) |

Rows measured (8): leaf keystroke, leaf keystroke + pause, shared.vl keystroke, model.vl keystroke, model.vl keystroke, importers open, css keystroke, parse break, parse repair.

> **Series break, model.vl keystroke:** Order 52 (N174): the keystroke edit is unchanged; the Find References probe moved from `fun messages(self)` (gone from model.vl) to `fun global()`, the mirrored store reader, so that probe starts a new series.

> **Series break, parse break:** Order 52 (N174): re-anchored from `let messages = channel.messages();` (gone with the mirrored-store exhibit, kolt@dee494ab) to `channel_name.adopt(channel.current_name());` in the same channel.vl - a statement over a `g.channels()` boundary; the break and its repair are the same shape, the figures start a new series.

Growth: plain:160 -> plain:320: x1.930 per doubling (the gate is x2.3).

## Bumps since the last release

- example:browser x1.01: E257 S8's reactive-trace seam in std::reactive (the hook table, the flag reads, SignalCell's inherent tracking new/set/update; the trace itself is its own module, analyzed only where imported): x1.00746 callgrind instructions of `vilan check` on the lane's box against next @b7c7afe0; the inline first build cost todo +1.48 % and reactive-ui +1.98 %, which is what moved the trace out. Re-taken at the Order 54 seal. (E257)
- example:browser x1.01: E257 S8's reactive-trace seam in std::reactive (the hook table, the flag reads, SignalCell's inherent tracking new/set/update; the trace itself is its own module, analyzed only where imported): x1.00746 callgrind instructions of `vilan check` on the lane's box against next @b7c7afe0; the inline first build cost todo +1.48 % and reactive-ui +1.98 %, which is what moved the trace out. Re-taken at the Order 54 seal. (E257)
- example:fullstack x1.01: E257 S8's reactive-trace seam in std::reactive (the hook table, the flag reads, SignalCell's inherent tracking new/set/update; the trace itself is its own module, analyzed only where imported): x1.00502 callgrind instructions of `vilan check` on the lane's box against next @b7c7afe0; the inline first build cost todo +1.48 % and reactive-ui +1.98 %, which is what moved the trace out. Re-taken at the Order 54 seal. (E257)
- example:fullstack x1.01: E257 S8's reactive-trace seam in std::reactive (the hook table, the flag reads, SignalCell's inherent tracking new/set/update; the trace itself is its own module, analyzed only where imported): x1.00502 callgrind instructions of `vilan check` on the lane's box against next @b7c7afe0; the inline first build cost todo +1.48 % and reactive-ui +1.98 %, which is what moved the trace out. Re-taken at the Order 54 seal. (E257)
- example:router x1.01: E257 S8's reactive-trace seam in std::reactive (the hook table, the flag reads, SignalCell's inherent tracking new/set/update; the trace itself is its own module, analyzed only where imported): x1.00978 callgrind instructions of `vilan check` on the lane's box against next @b7c7afe0; the inline first build cost todo +1.48 % and reactive-ui +1.98 %, which is what moved the trace out. Re-taken at the Order 54 seal. (E257)
- example:router x1.01: E257 S8's reactive-trace seam in std::reactive (the hook table, the flag reads, SignalCell's inherent tracking new/set/update; the trace itself is its own module, analyzed only where imported): x1.00978 callgrind instructions of `vilan check` on the lane's box against next @b7c7afe0; the inline first build cost todo +1.48 % and reactive-ui +1.98 %, which is what moved the trace out. Re-taken at the Order 54 seal. (E257)
- example:reactive-ui x1.01: E257 S8's reactive-trace seam in std::reactive (the hook table, the flag reads, SignalCell's inherent tracking new/set/update; the trace itself is its own module, analyzed only where imported): x1.00893 callgrind instructions of `vilan check` on the lane's box against next @b7c7afe0; the inline first build cost todo +1.48 % and reactive-ui +1.98 %, which is what moved the trace out. Re-taken at the Order 54 seal. (E257)
- example:reactive-ui x1.01: E257 S8's reactive-trace seam in std::reactive (the hook table, the flag reads, SignalCell's inherent tracking new/set/update; the trace itself is its own module, analyzed only where imported): x1.00893 callgrind instructions of `vilan check` on the lane's box against next @b7c7afe0; the inline first build cost todo +1.48 % and reactive-ui +1.98 %, which is what moved the trace out. Re-taken at the Order 54 seal. (E257)
- example:ssr x1.01: E257 S8's reactive-trace seam in std::reactive (the hook table, the flag reads, SignalCell's inherent tracking new/set/update; the trace itself is its own module, analyzed only where imported): x1.00659 callgrind instructions of `vilan check` on the lane's box against next @b7c7afe0; the inline first build cost todo +1.48 % and reactive-ui +1.98 %, which is what moved the trace out. Re-taken at the Order 54 seal. (E257)
- example:ssr x1.01: E257 S8's reactive-trace seam in std::reactive (the hook table, the flag reads, SignalCell's inherent tracking new/set/update; the trace itself is its own module, analyzed only where imported): x1.00659 callgrind instructions of `vilan check` on the lane's box against next @b7c7afe0; the inline first build cost todo +1.48 % and reactive-ui +1.98 %, which is what moved the trace out. Re-taken at the Order 54 seal. (E257)
- example:canvas x1.01: E257 S8's reactive-trace seam in std::reactive (the hook table, the flag reads, SignalCell's inherent tracking new/set/update; the trace itself is its own module, analyzed only where imported): x1.00926 callgrind instructions of `vilan check` on the lane's box against next @b7c7afe0; the inline first build cost todo +1.48 % and reactive-ui +1.98 %, which is what moved the trace out. Re-taken at the Order 54 seal. (E257)
- example:canvas x1.01: E257 S8's reactive-trace seam in std::reactive (the hook table, the flag reads, SignalCell's inherent tracking new/set/update; the trace itself is its own module, analyzed only where imported): x1.00926 callgrind instructions of `vilan check` on the lane's box against next @b7c7afe0; the inline first build cost todo +1.48 % and reactive-ui +1.98 %, which is what moved the trace out. Re-taken at the Order 54 seal. (E257)
- example:rpc x1.01: E257 S8's reactive-trace seam in std::reactive (the hook table, the flag reads, SignalCell's inherent tracking new/set/update; the trace itself is its own module, analyzed only where imported): x1.00858 callgrind instructions of `vilan check` on the lane's box against next @b7c7afe0; the inline first build cost todo +1.48 % and reactive-ui +1.98 %, which is what moved the trace out. Re-taken at the Order 54 seal. (E257)
- example:rpc x1.01: E257 S8's reactive-trace seam in std::reactive (the hook table, the flag reads, SignalCell's inherent tracking new/set/update; the trace itself is its own module, analyzed only where imported): x1.00858 callgrind instructions of `vilan check` on the lane's box against next @b7c7afe0; the inline first build cost todo +1.48 % and reactive-ui +1.98 %, which is what moved the trace out. Re-taken at the Order 54 seal. (E257)
- example:todo x1.01: E257 S8's reactive-trace seam in std::reactive (the hook table, the flag reads, SignalCell's inherent tracking new/set/update; the trace itself is its own module, analyzed only where imported): x1.00825 callgrind instructions of `vilan check` on the lane's box against next @b7c7afe0; the inline first build cost todo +1.48 % and reactive-ui +1.98 %, which is what moved the trace out. Re-taken at the Order 54 seal. (E257)
- example:todo x1.01: E257 S8's reactive-trace seam in std::reactive (the hook table, the flag reads, SignalCell's inherent tracking new/set/update; the trace itself is its own module, analyzed only where imported): x1.00825 callgrind instructions of `vilan check` on the lane's box against next @b7c7afe0; the inline first build cost todo +1.48 % and reactive-ui +1.98 %, which is what moved the trace out. Re-taken at the Order 54 seal. (E257)
- example:walkthrough x1.01: E257 S8's reactive-trace seam in std::reactive (the hook table, the flag reads, SignalCell's inherent tracking new/set/update; the trace itself is its own module, analyzed only where imported): x1.00525 callgrind instructions of `vilan check` on the lane's box against next @b7c7afe0; the inline first build cost todo +1.48 % and reactive-ui +1.98 %, which is what moved the trace out. Re-taken at the Order 54 seal. (E257)
- example:walkthrough x1.01: E257 S8's reactive-trace seam in std::reactive (the hook table, the flag reads, SignalCell's inherent tracking new/set/update; the trace itself is its own module, analyzed only where imported): x1.00525 callgrind instructions of `vilan check` on the lane's box against next @b7c7afe0; the inline first build cost todo +1.48 % and reactive-ui +1.98 %, which is what moved the trace out. Re-taken at the Order 54 seal. (E257)
