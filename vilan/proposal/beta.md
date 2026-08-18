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
  "blocked: specificity design" to beta-critical; the specificity design
  (method-resolution.md §9) is scheduled as a proposal-first lane.
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
