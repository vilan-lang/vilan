# Performance report — v0.48.0

**The most expensive phase is `checks`: 30% of the generated app's thread CPU** — the first place to optimize next.

Verdict **green** at `68f25eb163`, 2026-10-11, load 1.0. kolt `vilan check`: CPU x0.987, peak RSS x1.012, instructions x0.952 against the previous release.

## T2 — instruction counts (instructions:u, class reference)

| subject | count | previous | ratio |
|---|---:|---:|---:|
| example:math | 249,175,185 | — | — |
| example:watch | 315,057,251 | — | — |
| example:browser | 1,371,583,124 | — | — |
| example:fullstack | 2,870,645,546 | — | — |
| example:router | 1,532,276,405 | — | — |
| example:reactive-ui | 1,660,268,336 | — | — |
| example:ssr | 3,448,815,594 | — | — |
| example:canvas | 1,378,211,823 | — | — |
| example:rpc | 1,766,105,821 | — | — |
| example:todo | 4,377,861,323 | — | — |
| example:walkthrough | 4,475,305,348 | — | — |
| genapp:46 | 7,759,098,337 | — | — |
| plain:160 | 2,811,851,440 | — | — |
| plain:320 | 5,424,864,369 | — | — |

## E121 — the editor's targets

| edit | CPU to diagnostics | target | state |
|---|---:|---:|---|
| leaf keystroke (diagnostics_cpu_ms) | 550 ms | 500 ms | red (reporting) |
| shared.vl keystroke (diagnostics_cpu_ms) | 200 ms | 500 ms | green |
| model.vl keystroke (diagnostics_cpu_ms) | 210 ms | 500 ms | green |
| model.vl keystroke, importers open (cpu_ms) | 660 ms | 500 ms | red (reporting) |
| css keystroke (diagnostics_cpu_ms) | 750 ms | 500 ms | red (reporting) |
| parse break (diagnostics_cpu_ms) | 760 ms | 500 ms | red (reporting) |

Growth: plain:160 -> plain:320: x1.929 per doubling (the gate is x2.3).

## Bumps since the last release

- example:rpc x1.012: store-50's std::rpc growth, M120's class: A153 S3's `WireWrite` splice record and `RemoteStoreKeys`, A169's `Gone` acknowledgement and the reconnect hooks (A153 S4) live in std::rpc, which the three rpc-bearing examples load; x1.0103 callgrind on CI run 38084320109 against the v0.47.0 ceiling (the lane measured ±0.07% instructions:u on 5fe24f86's binary, which did not reproduce). The mirror module itself loads only once a service returns a Store. Re-taken at the Order 50 seal. (M120)
- example:todo x1.01: store-50's std::rpc growth, M120's class: A153 S3's `WireWrite` splice record and `RemoteStoreKeys`, A169's `Gone` acknowledgement and the reconnect hooks (A153 S4) live in std::rpc, which the three rpc-bearing examples load; x1.0082 callgrind on CI run 38084320109 against the v0.47.0 ceiling (the lane measured ±0.07% instructions:u on 5fe24f86's binary, which did not reproduce). The mirror module itself loads only once a service returns a Store. Re-taken at the Order 50 seal. (M120)
- example:walkthrough x1.01: store-50's std::rpc growth, M120's class: A153 S3's `WireWrite` splice record and `RemoteStoreKeys`, A169's `Gone` acknowledgement and the reconnect hooks (A153 S4) live in std::rpc, which the three rpc-bearing examples load; x1.0078 callgrind on CI run 38084320109 against the v0.47.0 ceiling (the lane measured ±0.07% instructions:u on 5fe24f86's binary, which did not reproduce). The mirror module itself loads only once a service returns a Store. Re-taken at the Order 50 seal. (M120)
- example:math x1.01: lang-50's std growth (array-lengths S5): the any-length array impls of PartialEq, Hashable, Debug and Items, and the inherent to_list/iter, live in compare.vl, hash.vl, debug.vl and list.vl, which every program loads; x1.0091 instructions:u on a release build of the lane's tip against next @ff35bf64's (re-measured after the C3 rebase), measured from each tree's own std (a constant ~2-14M instructions per program). Re-taken at the Order 50 seal. (A161)
- example:watch x1.009: lang-50's std growth (array-lengths S5): the any-length array impls of PartialEq, Hashable, Debug and Items, and the inherent to_list/iter, live in compare.vl, hash.vl, debug.vl and list.vl, which every program loads; x1.0082 instructions:u on a release build of the lane's tip against next @ff35bf64's, measured from each tree's own std (a constant ~2-14M instructions per program). Re-taken at the Order 50 seal. (A161)
- example:browser x1.005: lang-50's std growth (array-lengths S5): the any-length array impls of PartialEq, Hashable, Debug and Items, and the inherent to_list/iter, live in compare.vl, hash.vl, debug.vl and list.vl, which every program loads; x1.0042 instructions:u on a release build of the lane's tip against next @ff35bf64's (re-measured after the C3 rebase), measured from each tree's own std (a constant ~2-14M instructions per program). Re-taken at the Order 50 seal. (A161)
- example:reactive-ui x1.005: lang-50's std growth (array-lengths S5): the any-length array impls of PartialEq, Hashable, Debug and Items, and the inherent to_list/iter, live in compare.vl, hash.vl, debug.vl and list.vl, which every program loads; x1.0043 instructions:u on a release build of the lane's tip against next @ff35bf64's (re-measured after the C3 rebase), measured from each tree's own std (a constant ~2-14M instructions per program). Re-taken at the Order 50 seal. (A161)
- example:ssr x1.005: lang-50's std growth (array-lengths S5): the any-length array impls of PartialEq, Hashable, Debug and Items, and the inherent to_list/iter, live in compare.vl, hash.vl, debug.vl and list.vl, which every program loads; x1.0041 instructions:u on a release build of the lane's tip against next @ff35bf64's (re-measured after the C3 rebase), measured from each tree's own std (a constant ~2-14M instructions per program). Re-taken at the Order 50 seal. (A161)
