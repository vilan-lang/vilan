# Beta — executing the ratified contract, and the annex that extends it

> Status: **RATIFIED 2026-08-18 as recommended** ("Go with the
> recommendations on both papers") — §4's five answers stand: Q1 the
> clean-train count starts at v0.35.0; Q2 **B73 blocks trigger (c)** and
> is beta-critical from today; Q3 message-head identity, no numeric
> codes; Q4 reactive/ui Tier 2, canvas Tier 3; Q5 the annex (§3) is
> ratified as beta's extended surface. **Owner's same-day question —
> "should we defer beta? I might have jumped the gun":** the ratified
> trigger already answers it. Beta is not declared by this paper or by
> anyone; it is declared when process.md §5.4's four conditions hold,
> and today none of them do — (b) cannot be satisfied before
> 2026-08-29 at the earliest (two clean trains from v0.35.0), (c) now
> waits on B73, (a) on kolt's migration, (d) on D5. So: the charter
> stands, the pre-switch work (§2) proceeds at ordinary priority as the
> low-regret hygiene it is, and **nothing beta-branded ships publicly
> until the trigger fires** — the gun was never fired, only cocked.
> Recorded here so the question does not recur.
>
> Filed from the owner's 2026-08-18 cleanup list ("This seems like the
> turning point where we transition from alpha to beta"), item 6
> elevated to a charter.
>
> Prior status: DRAFT 2026-08-18.
>
> **This paper decides nothing process.md already decided.** The beta
> contract is RATIFIED (process.md §5, 2026-08-07): beta adds exactly
> three promises — a one-minor deprecation window, a mandatory
> `### Breaking` heading per minor, wire changes always counted breaking
> — explicitly does NOT freeze the spec ("beta freezes removal, not
> correction"), switches on a four-condition trigger rather than a date,
> and marks the switch with a deliberate jump to **v0.40.0**. What this
> paper adds: §1 tracks the trigger against today's tree, §2 plans the
> work the trigger and promises need, §3 proposes an **annex** of
> stability surfaces the ratified floor doesn't name (diagnostics
> identity, std tiers, toolchain, formats), and §4 asks the open
> questions. Tracker home: backlog-2026-08-18.md §L.

## 0. Why now

The backlog's work orders are drained, v0.34.0 unified `main` and `next`,
and the owner has called the turning point. process.md §9.2 deliberately
left §5's promises unimplemented "until beta is called" — this charter is
the walk toward calling it.

## 1. The trigger, read against today's tree

process.md §5.4, verbatim conditions, with today's status:

- **(a) The reference application runs on a released toolchain** —
  kolt-migration.md is the living tracker. In flight: the owner's
  `vilan-migration` branch on kolt is the active conversion (confirmed
  2026-08-18 — it is kolt's checked-out branch). Not yet met; closest of
  the four to mechanical completion.
- **(b) Two consecutive weekly trains ship with no urgent patch
  between them** — v0.33.0 (2026-08-08) and v0.34.0 (2026-08-12, cut
  early on the owner's word) are the first two trains. Whether an
  early-called cut counts toward "weekly" is the owner's read (§4 Q1);
  evaluation belongs to each cut sweep from here on.
- **(c) No known miscompile is open in the backlog's §B** — the open §B
  ledger is B3/B11 (feature tails), B73, B124, B125. B124/B125 are
  false *rejections* (valid code refused), not wrong answers. **B73 is
  the live question**: a blanket `Into<T>` beating a user's more
  specific impl by declaration order produces a *wrong resolution from
  a clean compile* — that is miscompile-shaped, and §4 Q2 asked whether
  it blocks the trigger. **Ruled yes, 2026-08-18**: B73 graduated from
  "blocked: specificity design" to beta-critical — and **SHIPPED the
  same day** (R1/R2/R3, method-resolution.md §13.8, cycle 21). With
  B73 closed the open §B ledger is B3/B11 (feature tails), B125/B126
  (false rejections), B127 (deferred simplification), B128 (a deferred
  residue no program in the tree exercises) — **condition (c) reads
  green as of 2026-08-18**, subject to the cut sweep's re-read each
  train.
- **(d) There is somebody to break the promise to** — D5, the traction
  plan, still `OPEN (blocked: needs a dedicated session with the
  owner)`. process.md §7.1 already named this the policy's urgent
  dependency. The cleanup arc's web work (§K: playground findability,
  docs integration, the design language) is groundwork for it either
  way.

## 2. The work the contract needs — before and at the switch

**Before** (filed in backlog §L/§M/§D):

- **Deprecation mechanism** (L4): §5.2(1)'s window needs its machinery —
  how a deprecated form warns (the diagnostics standard "already gives
  the compiler the machinery to say 'X is deprecated; use Y' well," per
  §5.2, so this is plumbing + a table, not a new subsystem), the
  one-minor lifetime, CLI alias handling for renames. Design note first;
  exercised end-to-end once before it is load-bearing.
- **Script the train** (L2 — SHIPPED 2026-08-18): the cut sequence
  (releases.md §7.2) had three hand steps — the ancestor-verify sweep, the
  CHANGELOG retitle/ordering, the fold. `scripts/cut-release.sh` and
  `scripts/fold-release.sh` execute them now (the prose stays the
  authority), which makes trigger (b)'s "clean consecutive trains" a
  property of the process rather than of the operator's concentration.
  First train to use them: 2026-08-22.
- **Ledger recheck** (L5) and **docs currency** (D15): the diagnostics
  promise (§3.1) and the switch commit both assume these are current.
- **Leak soak** (M2): not a trigger condition, but "beta" and "the LSP
  grows without bound" cannot both be said with a straight face; run it
  once with findings dispositioned before the switch.
- **process.md's own deferred tail** (L6–L8): branch protection (§8.3 —
  amend ruleset 18887216 per §2.3/§2.4, then require `ci / check`), the
  three security settings (§9.2 flagged "should not drift far" — that
  was eleven days ago; enabled 2026-08-18 under Order 2, L7), and the
  scaffolding slice (CONTRIBUTING.md et al.,
  deferred with D5). Protection and secret scanning should not wait for
  beta.

**At the switch** (one commit, the v0.40.0 cut):

- CHANGELOG.md header and releases.md §4 rewritten with the three
  promises (process.md §9.2 defers them to exactly this moment).
- README.md's "fast-moving alpha" status line replaced — it is
  *correct today* and stays until this commit (the 2026-08-18 rot
  survey flagged it as stale; per the ratified policy it is not).
- The `### Breaking` heading becomes structural in the changelog
  (v0.34.0's cut already *ordered* breaking entries first; beta makes
  the heading mandatory).
- The site and docs announce the posture change.

## 3. The annex — surfaces the ratified floor doesn't name

Proposals, each extending (never contradicting) §5.2. Ratifying this
section is what turns them on.

### 3.1 Diagnostics identity

The ledger already keys every site by **message head**
(diagnostics-ledger.md: "the message head is the stable key") under
ACCEPTED anchoring/wording rules. Beta promotes the internal key to an
external promise: rewording a message head is a `### Breaking` entry;
anchors may only narrow; every new site enters the ledger in the batch
that ships it. Numeric codes stay out (§4 Q3).

### 3.2 std tiers

Every public std module gets a published tier:

- **Tier 1 — stable**: full §5.2 ceremony to break (deprecation window,
  Breaking entry).
- **Tier 2 — provisional**: young surfaces; a break needs a Breaking
  entry but no deprecation window. Candidate: the whole process/
  fullstack layer (Service, LegBuild, serve_build, Document, watch, fs)
  — built cycles 16–19; promotes to Tier 1 after two incident-free
  trains, by default, at the cut sweep.
- **Tier 3 — experimental**: may be reworked wholesale with a CHANGELOG
  note. Assignment for reactive/ui/canvas is §4 Q4.

The module-by-module sweep is L3; the table lands in the docs.

### 3.3 Toolchain

CLI commands/flags and the `vilan.toml` schema: additive-only within
beta; a rename keeps the old spelling as a warning alias for one minor
(the same window §5.2 gives the language).

### 3.4 Formats

The build/leg manifest and the playground manifest version themselves
and keep one-version read-back compat; the RPC wire needs only
same-build agreement and §5.2(3) already makes any wire change a
Breaking entry unconditionally.

## 4. Open questions — all RULED 2026-08-18, each as recommended

- **Q1 — trigger (b) accounting**: does the v0.33.0 → v0.34.0 pair
  (second cut called early) count as the first of the two clean
  consecutive trains, or does counting start with the first *unassisted*
  Saturday cut? *Recommendation: start counting at v0.35.0 — the point
  of (b) is demonstrated rhythm, and an early call, however justified,
  is not the rhythm.*
- **Q2 — does B73 block trigger (c)?** A clean compile resolving to the
  wrong impl is a wrong answer at runtime, which reads as
  miscompile-shaped even though the tracker files it as a resolution
  bug. *Recommendation: yes — treat B73 as (c)-blocking and schedule
  the specificity design (method-resolution.md §9) accordingly. B124/
  B125 refuse valid code but never produce a wrong answer; they do not
  block (c).*
- **Q3 — diagnostic codes**: introduce `E0123`-style codes at beta, or
  keep message-head identity? *Recommendation: keep message heads — the
  ledger enforces them today, codes have no consumer yet, and they can
  be assigned mechanically later if external tooling wants machine
  keys.*
- **Q4 — tier assignment for reactive/ui/canvas**: *Recommendation:
  reactive and ui at Tier 2 — the todo app, the website, and kolt all
  stand on them, which is exactly what "provisional, promote on quiet"
  is for; canvas at Tier 3.*
- **Q5 — the annex itself**: ratify §3.1–§3.4 as beta's extended
  surface? Each is severable; §3.1 and §3.2 carry the substance.

## 5. The tier table (L3, 2026-08-20)

> Status: PROPOSED — the L3 sweep's module-by-module table (§3.2's
> "the module-by-module sweep is L3"). Owner rules; the docs page
> lands at ratification, not before.

**The census.** The honest enumeration is the embedded-std walk:
`crates/vilan-embedded-std/build.rs` embeds every `.vl` under
`vilan/std` and `vilan/macro_std` into the binary, and that walk
carries **60 `.vl` files** — 43 top-level `std/src/*.vl`, 9
`std/src/process/*.vl`, 5 `std/src/browser/*.vl`, 3
`macro_std/src/*.vl`. Not tiered: the two package roots
(`std/src/lib.vl`, `macro_std/src/lib.vl` — re-export shims, not
modules) and `std/src/native_map.vl` (its own header: "an internal
building block for the value-keyed `Map`/`Set`"; no docs page names
it). `browser/ui.vl` and `process/ui.vl` are the two platform halves
of the one public module `std::ui`. That leaves **56 public modules**:
54 in std, 2 in macro_std. Every row below is one of them; nothing
below is anything else.

**The evidence axes.** Age is `git log --follow` on the file (first
commit in the tree). Incidents are CHANGELOG entries. Breadth is
import/mention counts across std itself, `examples/`, `docs/`,
`test/`, and the website checkout (read-only). Open backlog items are
read from backlog-2026-08-18.md as of today. The prelude built-ins
(`list`, `string`, `option`, `number`, `boolean`, `null`, `result`)
import-count near zero *because they need no import* — they are
weighted by ubiquity, not by grep.

**Promote-on-quiet.** §3.2 attaches "promotes to Tier 1 after two
incident-free trains, by default, at the cut sweep" to the fullstack
candidate. This table extends that clock to every Tier 2 row as the
default, except rows marked **hold** — held because an open design
item gates the surface or because the quiet is deliberate minimalism
(one narrow consumer), not demonstrated stability. The cut sweep
re-reads the table each train.

### Tier 1 — stable (32 modules)

Full §5.2 ceremony to break: deprecation window + Breaking entry.
The core value/collection/string/math floor, the ratified traits, and
the June-old thin host bindings that have not moved in weeks.

| Module | In tree since | Why Tier 1 |
| --- | --- | --- |
| `std::arena` | 2026-06-19 | The ownership story's teaching surface (docs `std/cells.md`); no incident ever; quiet since 07-26. |
| `std::base64` | 2026-07-11 | RFC 4648 §5 is the spec; pure vilan, const-capable; quiet since 07-28. |
| `std::boolean` | 2026-06-13 | The built-in `bool`; prelude. |
| `std::bytes` | 2026-07-02 | The byte substrate under binary/ws/http interop — 13 std consumers; quiet since 07-23. |
| `std::compare` | 2026-06-13 | `PartialEq`/`Eq`/`PartialOrd`/`Ord`, the derive targets; 13 std consumers. |
| `std::context` | 2026-06-14 | Spec §8's mechanism; reactive and rpc stand on it; quiet since 07-17. |
| `std::debug` | 2026-06-19 | `[derive(Debug)]`; last touch (08-01) was a spelling sweep. |
| `std::default` | 2026-06-13 | Prelude `Default`; six std consumers. |
| `std::display` | 2026-06-18 | `Display`/`format`; the std-surface v1 audit (2026-08-03) settled its surface. |
| `std::dom` | 2026-06-19 | Minimal host binding *under* `std::ui` — older and quieter (since 08-01) than the layer above it; → Q4. |
| `std::drop` | 2026-07-19 | destruction.md §5's ratified hook; untouched since the day it landed. |
| `std::fetch` | 2026-06-19 | Thin universal binding over the host `fetch` global; quiet since 07-23. |
| `std::hash` | 2026-07-14 | `Map`/`Set`'s key contract (I1); breaking it breaks the collections; B110 (08-10) aligned it to its own spec — closed. |
| `std::io` | 2026-06-13 | `print`/`panic`/`assert` — the prelude's I/O. |
| `std::iterator` | 2026-06-13 | The protocol is June-old; the adapter arc shipped pinned 08-06 and its remainder (`Iterable`, §I3) is additive; → Q1. |
| `std::json` | 2026-06-19 | The default codec, broadest breadth (6 examples, 6 docs pages); surface corrected at v0.34 (`JsonKind`). |
| `std::list` | 2026-06-13 | The built-in growable array; prelude. |
| `std::map` | 2026-06-18 | The built-in map; the 08-10 churn was additive (`entries`/`contains_value`). |
| `std::math` | 2026-07-09 | K2's constants + generic free functions; quiet since 07-17. |
| `std::null` | 2026-06-14 | The built-in null/unit; dependency-free by design. |
| `std::number` | 2026-06-13 | The scalar types; prelude. |
| `std::option` | 2026-06-13 | The tree's most-imported module (38 std files, 18 docs pages, 7 website files). |
| `std::process` | 2026-06-18 | stdin/args/env/exit — predates the fullstack arc, boring on purpose; not in §3.2's candidate list; → Q4. |
| `std::promise` | 2026-06-14 | Opaque async handle; quiet since 07-17; B11 §12.2's `Lift` opt-in would be an addition. |
| `std::random` | 2026-06-14 | Small and quiet since 07-17. |
| `std::range` | 2026-06-18 | §I2's slicing wish is additive. |
| `std::result` | 2026-06-13 | The error model's bedrock; prelude. |
| `std::set` | 2026-06-18 | The built-in set; 08-10 churn additive. |
| `std::shared` | 2026-06-19 | 10 std consumers; §C2's cross-handle tail is native-future and debug-mode — not a surface change. |
| `std::string` | 2026-06-13 | `str`; prelude. |
| `std::task` | 2026-07-18 | `nursery`/structured spawn; quiet since 08-01; §J4's lint warns and §J5's handles add — neither breaks. |
| `std::time` | 2026-06-18 | One incident ever (v0.3.0's instant comparison, June-era); K5's deliberate minimalism; quiet since `Timer` (07-28). |

### Tier 2 — provisional (23 modules)

A break needs a Breaking entry, no deprecation window. The ratified
§4 Q4 rulings (reactive, ui), the §3.2 candidate fullstack layer, the
transport family, the browser layer's young half, and macro_std.
"hold" = exempt from the promote-on-quiet default until the named
gate clears.

| Module | In tree since | Clock | Why Tier 2 |
| --- | --- | --- | --- |
| `std::reactive` | 2026-06-19 | hold | RULED Tier 2 (§4 Q4); still growing this week (`Optimistic` 08-04, counted `RemoteSource` subscriptions 08-18/19); §A14's batch residual open. |
| `std::ui` | 2026-06-20 / 07-24 | hold | RULED Tier 2 (§4 Q4); two platform halves; §A7's SSR factoring undesigned; builder-era churn 08-11. |
| `std::style` | 2026-07-10 | hold | v0.17.0's breakpoint-order incident; §A8 tail + A22 open; the website leans on it (10 files). |
| `std::operators` | 2026-06-13 | hold | The operator traits are bedrock, but B11's undesigned `Try`/`Lift` extensions live in this file; → Q2. |
| `std::wire` | 2026-07-02 | runs | The transport family's codec vocabulary; §5.2(3) already prices any wire change as Breaking; → Q5. |
| `std::binary` | 2026-07-02 | runs | Schema-ordered young codec (transport-rpc §6.2); rides the transport family. |
| `std::rpc` | 2026-06-24 | runs | The arc is live: `on_reconnect`, counted subscriptions, `RemoteSource` S2 all landed 08-18/19. |
| `std::rpc_server` | 2026-06-14 | runs | `Service` is named in §3.2's ratified candidate; builder churn 08-11. |
| `std::ws` | 2026-07-03 | runs | RFC 6455 frame layer; niche surface (the server upgrade path + tests). |
| `std::http` | 2026-06-14 | runs | Named layer (`serve_build` home); `build_handler` retired in Unreleased — churning this week; v0.16.0 Breaking on `serve_service`. |
| `std::build` | 2026-08-11 | runs | Nine days old; `LegBuild` named in §3.2's candidate. |
| `std::document` | 2026-08-12 | runs | Eight days old; `Document` named in §3.2's candidate. |
| `std::fs` | 2026-06-14 | runs | Named in §3.2's candidate; reworked 08-11 (`read_bytes`; the docs audit caught `read_file_encoded` undocumented). |
| `std::watch` | 2026-08-11 | runs | Named in §3.2's candidate; nine days old. |
| `std::dev` | 2026-07-21 | runs | The HMR hooks (hmr.md §4); pairs with `std::watch` — the dev-mode contract settles as a pair. |
| `std::router` | 2026-07-11 | runs | `chunk_error` incident fixed v0.26.0 (08-04); the bundle-splitting arc is three trains old. |
| `std::db` | 2026-07-11 | hold | `node:sqlite` seam; single-consumer breadth (one example) — its quiet is narrowness, not proof. |
| `std::crypto` | 2026-07-11 | hold | kolt-shaped HS512-era minimum; the surface is what one consumer needed. |
| `std::jwt` | 2026-07-11 | hold | HS512-only by design; the surface is expected to grow before it hardens. |
| `std::storage` | 2026-07-11 | hold | The ""-for-missing-key read is exactly the young choice Tier 2 exists to revisit. |
| `std::asset` | 2026-07-10 | hold | const-eval §8's deferred remainder and the A7-entangled emission both sit under it. |
| `macro_std::meta` | 2026-07-06 | hold | The hermetic macro surface IS this package; the engine is a month old and its own proposal tail (G2) is open. |
| `macro_std::build` | 2026-07-07 | hold | The construction API; same engine clock as `meta`. |

### Tier 3 — experimental (1 module)

May be reworked wholesale with a CHANGELOG note.

| Module | In tree since | Why Tier 3 |
| --- | --- | --- |
| `std::into` | 2026-06-13 | B127 holds its deletion question open — the blanket impl has zero in-tree dependents beyond the B98 pin; promising even Tier 2 ceremony for a surface that may vanish is ceremony; → Q3. |

**Canvas is not a row.** §4 Q4 ruled "canvas at Tier 3", but no
module named canvas exists in the tree (canvas.md is a proposal).
The ruling is a pre-assignment: it binds when the module lands, and
the module enters this table then — not before.

**The docs page** (stated here, written at ratification): one page,
`docs/std/tiers.md` — "Stability tiers" — added to `SUMMARY.md` as
the first entry of "The std reference" part, before Collections. One
table (module | tier), prefaced by the three tier definitions in
§5.2's vocabulary and the promote-on-quiet rule. The CHANGELOG
header and releases.md §4 link to it at the switch commit. No
per-module prose — the reference pages stay the prose home, and the
tier column is repeated nowhere else.

### 5.1 Owner questions

Only the genuinely arguable rows. Everything not listed here is
proposed as tabled above.

1. **Does promote-on-quiet generalize — and where does `iterator`
   start?** The table extends §3.2's fullstack promote clock to every
   unmarked Tier 2 row; confirm or confine it. And `std::iterator` is
   proposed Tier 1 outright — its adapter surface is 14 days old, but
   two quiet trains (v0.33.0, v0.34.0) have already elapsed since it
   shipped, so Tier 2 would promote it at the 08-22 cut anyway.
   Tier 1 now, or one more train at Tier 2?
2. **`std::operators` — one module, two natures.** The B11 rule puts
   it at Tier 2 (the undesigned `Try`/`Lift` closure/opt-in extensions
   live there), but `Add`…`BitOr` are bedrock no one expects to move.
   Accept Tier 2 whole, or rule an item-level carve (operator traits
   Tier 1, `Try`/`Lift` Tier 2) — which amends §3.2's per-module
   grain.
3. **`std::into` at Tier 3.** Proposed on B127's open deletion
   question. If the `Into` trait itself is considered settled and only
   the blanket is in question, Tier 2 is the alternative.
4. **The carve-outs: `std::process` and `std::dom` at Tier 1.** Both
   are June-old, thin, and quiet, and neither is in §3.2's candidate
   list — but Tier 1 here makes their directories non-uniform (every
   sibling is Tier 2). Uniform-directory reading would put both at
   Tier 2.
5. **`std::wire` at Tier 2.** Proposed to ride with its transport
   family, but §5.2(3) already makes any wire change a Breaking entry
   regardless of tier, and the `Wire` trait is the derives' contract —
   an argument it earns Tier 1 on day one.
