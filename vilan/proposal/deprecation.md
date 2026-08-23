# Deprecation — the machinery for the one-minor window (L4)

> Status: PROPOSED 2026-08-20 (cycle 25, work order 7); RULED 2026-08-22
> (all four questions as recommended — see the block above the questions);
> MACHINERY SHIPPED 2026-08-23, lane l4 (order 9 wave 2) — the ship
> record is §7. Tracker: backlog-2026-08-18.md §L item 4.
>
> This note decides nothing the ratified papers decided. process.md
> §5.2(1) (RATIFIED 2026-08-07) is the promise: "a breaking change to
> the language or std ships first as a *warning* that names its
> replacement; the removal comes no earlier than the following minor."
> beta.md (RATIFIED 2026-08-18) prices it by tier (§3.2) and extends it
> to the toolchain (§3.3). What this note adds is the machinery: the
> warning's shape (§1), what marks a form deprecated (§2), where the
> lifetime is recorded and what the cut script checks (§3), CLI alias
> plumbing (§4), and the end-to-end exercise beta.md §2 requires (§5).

## 0. What is promised, what exists, and what changes at the switch

**The obligation, scoped honestly.** The window is owed to the language
itself, to Tier 1's 32 modules (beta.md §5), and to the toolchain
surface — CLI flags/commands and the `vilan.toml` schema, where a rename
keeps the old spelling as a warning alias for one minor (§3.3). Tier 2
breaks need a Breaking entry but **no window**; Tier 3 needs a note. The
wire contract is §5.2(3)'s separate clause — always a Breaking entry,
and **window-exempt by nature**: a compiler warning cannot reach a
running deployment, so the entry is the whole promise there.

**What exists today: nothing.** `grep -rn deprecat crates/` finds no
mechanism — no attribute, no warning site, no alias plumbing, no cut
check. The baseline is E64 and E71: `build_handler` (merged 7e66a4eb,
2026-08-19) and the `serve_rpc`/`serve_service`/`serve_connected` trio
(merged dfce0cd1, 2026-08-20) both shipped as **same-train removals**
riding v0.35.0, each with a `family: breaking` CHANGELOG entry and
migration prose, and no warning period. Legal — alpha's header licenses
exactly that, and the house precedent is explicit ("no alias kept —
pre-1.0", the `read_file_encoded` rename; numeric-types.md §8).

**What precisely changes at the switch.** Less than it looks. `http` and
`rpc_server` sit at **Tier 2** in the draft table, so the E64/E71 arcs
would have been legal under beta too — Breaking entry, no window. What
the switch actually arms: (a) a Tier 1 removal becomes a two-train act,
warning then removal; (b) the "no alias kept" precedent **inverts** for
the toolchain — a flag or command rename keeps the old spelling warning
for one minor; (c) the `### Breaking` heading becomes structural and the
cut script starts refusing an unaccounted removal (§3). The machinery
below can land pre-switch as low-regret hygiene (beta.md's status block
sets that posture); the *obligation* starts at the v0.40.0 commit.

**Renames are the easy shape; in-place changes are the hard one.** A
removal or rename leaves the old form standing to hang a warning on. An
in-place change — v0.16.0's `serve_service` callback growing a `Server`
parameter is the type specimen — has no old form to keep. The honest
options there: introduce the new shape under a new name and deprecate
the old (the rename dance), or price it at its tier (Tier 2: Breaking
entry, done). §5.3 exempts the third case: a semantics *correction* is
not a removal and owes no window.

## 1. The warning — a diagnostic like any other

**The channel exists.** The analyzer already carries a warning channel
parallel to diagnostics — `warnings: Vec<Error>` + `warning_sources`
(analyzer.rs:2193), sorted for determinism (analyzer.rs:31878), rendered
by the CLI as an ariadne Warning to stderr, mapped by the LSP to
`DiagnosticSeverity::WARNING`, and **never fatal**: `vilan check`/`build`
exit 0 with warnings present, pinned end-to-end
(vilan-cli tests/diagnostics.rs:150). It has exactly two producers today
(`check_must_use`, the element-attribute shadowing check). The
deprecation warning is the third producer on that channel — not a new
severity, not a new renderer.

**The head.** One parameterized family head, in the house form §5.2
already quotes:

    `{name}` is deprecated; use {replacement}

The replacement is code-shaped when short (B4): `` `serve_service` is
deprecated; use `Server::builder().with_service(…)` ``. The CLI flavor
substitutes spellings: `` `--target` is deprecated; use `--platform` ``.
No version numbers in the message — "when it goes away" is the
CHANGELOG's fact (§3), and a message that names a version is stale the
train after it ships.

**The anchor.** A-rules apply unchanged. A1/A4: the narrowest
identifying span — the callee *name* at a call (the method/call
re-anchor precedent: names over argument lists), the failing *segment*
at an import, the type name at a type position. A2: the warning fires
**only when the use site is user code**. Mechanically this is
`check_must_use`'s pattern inverted to the same effect
(analyzer.rs:16764 keys its skip on the *discarding statement's* home,
not the callee's): the deprecation check keys on the use site's source —
a std-internal call to a deprecated form is silent. That is also what
keeps check_scope_differential.rs:124's "std must be warning-clean under
full scan" gate green; and hygiene says std migrates its own callers in
the deprecating train anyway. Per use site, not once per form — each
site is an independent fix (B5's root-cause reading) — and C1-ordered by
the existing sort.

**The pin (C2).** The suite's warning helper (`warnings(source)`,
inference.rs:825) asserts messages only. The machinery ships
`assert_warns_spanning(source, spanning, message_part)` — the warning
twin of `assert_fails_spanning`, asserting the span, the head fragment,
**and that analysis produced a program** (a warning that rides an error
is not the non-fatal path). Every deprecation entering std lands with
one. The determinism, build-idempotence, and check-scope gates already
fold warnings in, so those cover the new producer for free.

**The ledger.** The head enters diagnostics-ledger.md in the batch that
ships the machinery, per the standing rule — one family row, the way
rows 85/91 carry an `{operator}` parameter. Under beta §3.1 that head
then becomes reword-priced like any other. Deliberate: the deprecation
message is itself a stability surface. The LSP can additionally tag the
site `DiagnosticTag::Deprecated` (the client renders strikethrough) —
free once the warning carries a marker, noted as follow-up, not scope.

## 2. What marks a form deprecated — the attribute, recommended

**`[deprecated("use Y")]` in std source.** The grammar cost is nearly
zero: `[platform("@process", "browser")]` already parses an attribute
with one-or-more quoted-string arguments (parsing.rs:4154), and
attributes flatten into `Func` fields — `deprecated: Option<&str>`
lands beside `must_use: bool` (node.rs:71). The name is free: today
`[deprecated(…)]` falls through to the user-macro-attribute path and
errors "no macro named `deprecated` is in scope". The real cost is the
whitelist's three-place rule: `KNOWN_ATTRIBUTE_MARKERS` (parsing.rs:493)
is mirrored in the VS Code grammar and the docs highlighter, gated by
grammar_sync.rs:500 — one entry in each, and the gate holds you to it.

**Why not a compiler-side table** (`&[(path, replacement)]` in the
analyzer, matched at resolution)? It was the tempting "honest for a
std-only window" shape, and it fails on the two things this repo has
already been burned by:

- **Drift.** A Rust-side table shadowing vilan-side facts is exactly the
  two-surface class batch 8 documented (93d73f57: ~20 heads reworded,
  ledger untouched). At removal time you delete the item in std *and*
  the row in Rust; miss the row and the compiler warns about a form that
  no longer exists. The attribute dies with its item — one place.
- **Testability.** A path-keyed table cannot be exercised without
  mutating std or faking one. The attribute is exercised by a plain
  fixture — a test program deprecating *its own* function — which is
  what makes §5's exercise cheap and decides it.

Plus the forward-looking reason: the attribute is where the LSP hover,
completion strike-through, and any future doc generation will want the
fact, and where L10's separately-published packages will need it — a
compiler table can never serve a package the compiler didn't ship.

**Scope honesty.** An attribute is language surface the moment it
parses. Recommendation: **honored wherever it appears** (that is what
makes it testable and what packages will eventually need), documented in
the book's attribute list, with the *promise* extending only as far as
the toolchain's own beta posture (additive-only; §3.3). Not a new
subsystem: one whitelist entry ×3, one parser function cloned from
platform's, one `Func` field, one warning producer.

**What the attribute does not carry: versions.** No `since:`/`until:`
argument. The steer lives in source; the train accounting lives in the
CHANGELOG (§3). Duplicating the date into source is a second surface to
drift and a mandatory source edit at every cut.

**Forms with no item to hang it on.** A language-*form* deprecation
(surface syntax being removed, not corrected — §5.3 keeps corrections
free) is compiler-side by nature: a targeted check emitting the same
warning head. Expected rare; each is its own small change, priced by the
arc that wants it. Attribute placement beyond functions (a whole module,
a struct, a field) can arrive the first time a deprecation needs it —
the parse shape and the warning are identical.

## 3. The lifetime — the CHANGELOG is the ledger, the cut is the audit

**No new file.** The "deprecated in vX.Y, removed in vX.(Y+1)" record
lives where the cut already reads: marker lines in the family-marker
idiom `cut-release.sh` refuses on today.

- The **deprecating** train's entry — the warning ships, the attribute
  lands, std's own callers migrate — carries, above its bold head:
  `<!-- family: breaking -->` and `<!-- deprecates: KEY -->`, one line
  per form. KEY is the fully qualified path
  (`std::rpc_server::serve_service`) or the CLI spelling
  (`vilan build --target`).
- The **removing** train's entry carries `<!-- family: breaking -->` and
  `<!-- removes: KEY -->`, same keys.

Filing the deprecation itself under `breaking` is deliberate: nothing
fails to compile yet, but it is the migration notice, and the reader
§5.2(2) serves scans one heading for "does this break me" — the notice
belongs where the removal will. (At the switch, this marker is what
generates the structural `### Breaking` heading; the entry rides it.)

**The sweep check** (cut-release.sh, joining step 1's ancestor sweep):
for every `removes: KEY` under `## Unreleased`, a matching
`deprecates: KEY` must exist in a **released** `## vX.Y.Z` section —
a match inside the same Unreleased section does not count. No match =
**REFUSED and printed, never guessed** — the family discipline verbatim.
Because every train is a minor, "in a released section" *is* "at least
one minor of warning": no version arithmetic, a fixed-string search over
the file the script already parses. Patches stay out by rule —
deprecations and removals ride minors only (patches are fixes,
releases.md §4), so a cut whose patch component is nonzero refuses
either marker outright.

**Floor, not ceiling.** §5.2(1) says "no earlier than the following
minor" — there is no deadline. A `deprecates:` with no `removes:` yet is
not an error; the sweep **reports** pending deprecations (key + the
train that shipped it) at every cut, so nothing lingers invisibly.
Un-deprecating (the replacement was wrong) is removing the attribute and
a tooling entry; the historical marker stays — it is history — and the
check is one-directional.

**What the check honestly is.** It audits what is claimed. A removal
whose entry carries no `removes:` marker is invisible to the script —
the gate is bookkeeping made refusable, not omniscience. The obligation
to *use* the markers is the ratified papers', enforced the way family
markers are: refusal when present-but-unshipped, review when absent.

**Testable today.** release_scripts.rs already runs both scripts against
fixture repos with fixture CHANGELOGs. The REFUSED case, the
released-match case, the same-section non-match, and the pending report
are four ordinary pins in that harness.

## 4. CLI aliases — and `vilan.toml` keys

The CLI is clap-derive (main.rs:25). One alias exists: `--target` for
`--platform` on Build/Check — **silent, and correctly so**: it is a
documented courtesy alias with no removal intended, not a deprecation.
The two concepts stay distinct; `--target` is not this note's business.

**A rename under beta §3.3** cannot use clap's `alias` — clap does not
record which spelling matched, so there is nothing to warn on. The
plumbing: the old spelling becomes its own **hidden** arg
(`#[arg(long, hide = true)]`), reconciled at dispatch — old present
warns `` `--old` is deprecated; use `--new` `` on stderr and folds the
value into the new arg; both spellings present with conflicting values
is an error. A renamed *subcommand* is a hidden variant that warns and
delegates. One shared helper; the warning is a plain stderr line in the
house voice, no ariadne panel (there is no span). The head enters the
diagnostics ledger as a spanless row, the way batch 8 filed std's
runtime refusals (rows 230–239: A-rules n/a, B-rules and C2 judged).

**Lifecycle identical to §3.** The alias's introduction entry carries
`deprecates: vilan <cmd> --old`; dropping the alias one-plus minors
later is a Breaking entry with `removes:`; the same sweep check applies.
`vilan.toml` key renames take the same shape: old key accepted at load
with the warning, one minor, same markers.

## 5. The exercise — synthetic now, voluntary on the first real one

beta.md §2: "exercised end-to-end once before it is load-bearing."

**There is no real candidate.** E64/E71 drained the retire-shaped queue
— owner-ruled, merged, riding v0.35.0 — and the live backlog holds no
std retirement or CLI rename. Holding for a real one means the machinery
meets its first customer under a live promise, which is the exact
condition the requirement exists to prevent; and the wait could be
short — trigger (b) can fire 2026-08-29 at the earliest.

**So: the synthetic exercise, two legs, in the ordinary suite.**

1. **Mechanism leg.** Because the attribute is honored wherever it
   appears (§2), no std mutation is needed: a fixture program deprecates
   its own function — `[deprecated("use two()")] fun one() …` — and
   calls it. `assert_warns_spanning` pins head, span (the callee name),
   and success; a companion fixture pins the A2 skip (a deprecated form
   used only inside std stays silent, keeping the std-warning-clean
   gate); the determinism/idempotence/scope gates cover the producer
   for free.
2. **Process leg.** release_scripts.rs fixtures: a `removes:` with no
   shipped `deprecates:` is REFUSED; with the marker in a released
   section the cut orders and accepts; the pending-deprecation report
   prints. The `--out`/`--dry-run` seams the pins already use suffice.

**Plus one live rehearsal, voluntarily.** Standing instruction: the next
genuinely retire-shaped std ruling — whenever the owner makes one —
takes the window road even if it lands before the switch: warn in train
N with the markers, remove in N+1. Alpha permits the immediate removal;
doing the two-train dance once with no promise at stake is the rehearsal
with real users (all three of them). Recommend both: the synthetic legs
satisfy "before load-bearing" on a clock we control; the voluntary run
proves the calendar half with a real form. `into`'s possible deletion
(B127) is *not* the candidate — Tier 3 owes no window, and exercising
the machinery on a surface exempt from it proves nothing.

## 6. Non-goals

No `since:` in source (§2). No numeric codes (Q3 RULED). No tombstone
errors after removal ("removed in v0.41.0; use Y") — the CHANGELOG's
migration prose is that, and a tombstone table is the drift shape §2
rejected. No deprecation promises for user packages until L10's world
exists — the attribute will be ready; the promise waits.

## Owner questions

> **RULED 2026-08-22 — all four as recommended**: the attribute; honored
> wherever it appears and documented; the deprecating entry's family is
> `breaking`; both §5 legs now plus the voluntary window on the next
> real retirement.

1. **Attribute vs compiler-side table.** Recommend the attribute
   (§2: one-place truth, testable in a plain fixture, serves
   LSP/docs/packages later). The table was the cheaper-looking option
   and fails on drift. Accept?
2. **The attribute's visibility.** Recommend: honored wherever it
   appears and documented in the book's attribute list — versus
   parse-but-undocumented, std-only in practice. Honored-everywhere is
   what makes it testable; documenting what parses is the honest half.
3. **The deprecating entry's family.** Recommend `breaking` — the
   notice belongs under the heading the reader scans, and the marker
   generates `### Breaking` at the switch — versus a fifth family
   (`deprecation`) ranked beside it. Cosmetic either way; the sweep
   check keys on the `deprecates:`/`removes:` markers, not the family.
4. **The exercise.** Recommend both §5 legs now plus the voluntary
   window on the next real retirement — versus holding the whole
   exercise for a real candidate. If you rule "hold", the machinery
   should still not merge unpinned; only the calendar rehearsal waits.

## 7. Ship record (2026-08-23, lane l4)

The machinery of §§1–5 is built, pinned, and merged as three commits
(the attribute + warning; the lifetime markers + cut audit; the CLI
rename mechanism) plus this record. Everything below is as designed
above unless it says otherwise; the decisions the paper left open are
recorded here.

**§1–§2, the attribute and the warning — shipped.**
`[deprecated("use …")]` leads the ordered function-attribute chain
(before `[extern(..)]`; parsing.rs's `parse_deprecated_attribute`, the
whitelist entry ×3 grammars under the grammar_sync gate), flattens into
`Func::deprecated` beside `must_use`, and is carried on both `Function`
and `ExternalFunction` in the analyzer — std's retire-shaped items are
often externals. `check_deprecated` is the third producer on the
warning channel, run beside `check_must_use`; the head is ledger row
246 (row 247 the CLI flavor; both provisional to this lane — the
orchestrator renumbers at merge). Pins: inference.rs (the
`assert_warns_spanning` helper — the warning twin of
`assert_fails_spanning`, asserting span, head fragment, and a CLEAN
program — plus seven cases), vilan-lsp document.rs (publishes as a
WARNING diagnostic), all plant-proven red. The book: spec §3.3 carries
the production and the semantics; the lexical/appendix keyword lists
and the error index carry the head.

Decisions recorded:

- **The steer is the attribute's argument, verbatim.** The message is
  `` `{name}` is deprecated; {steer} `` and the `use {replacement}`
  clause of §1's family head is the steer's *convention* (std will hold
  to it; the book documents it), not compiler-enforced — the compiler
  neither prepends `use` nor parses the clause. B4's code-shaped
  replacement is the author's spelling choice inside the string.
- **A use inside another deprecated item still warns.** No Rust-style
  suppression: per §1, each site is an independent fix — the deprecated
  wrapper's body must migrate too. Pinned
  (`a_use_inside_another_deprecated_item_still_warns`).
- **What counts as a use: calls (free and method) and the function
  passed as a value.** All three warn, each at its name span, pinned.
  An `import` line alone does not warn — the loader mints no value
  reference for it — and that is the behavior we want: the actionable
  fix sites are the uses; a dead import falls out with the last one.
- **A2's boundary is `std_sources`,** not the S1 frozen ranges (which
  are empty under full scan — the differential gate would split). A
  *dependency package's* internal use of its own deprecated form is not
  exempted; only std is. Revisit when L10 gives packages a deprecation
  story of their own (§6's posture unchanged).
- **Placement stays functions-only** (free, member, external — one
  `parse_function`). A struct/module/field placement arrives with the
  first deprecation that needs it, as §2 says. `[deprecated]` with no
  argument does not parse: the warning's head needs its steer.

**§3, the lifetime markers and the cut audit — shipped.**
`deprecates:`/`removes:` markers in the family-marker idiom (one KEY
per line; repetition legal, unlike `family:`/`commit:`); the sweep in
`cut-release.sh` refuses an Unreleased `removes:` with no released
`deprecates:` (same-section match refused, printed with the §5.2(1)
rule), refuses either marker on a patch cut, reports pending
deprecations (key + shipping train) at every cut, and the rewrite
carries the markers into the release section. The L11 stranding
discipline covers the new markers; an empty KEY refuses. Seven pins in
release_scripts.rs through the `--out`/`--dry-run` seam, plant-proven.

**§4, CLI renames — the mechanism shipped, unwired.**
`reconcile_renamed_flag` + `deprecated_spelling_warning` in vilan-cli
main.rs: the old spelling as its own `#[arg(long, hide = true)]` arg,
folded at dispatch (old alone warns; both agreeing fold, still warning;
both conflicting refuse naming the spelling to drop). Pinned on a
SYNTHETIC pair (`--fresh-spelling`/`--stale-spelling`) in main.rs's
tests — no real rename exists, and none was invented; `--target` stays
the silent courtesy alias §4 distinguishes. The helpers carry
`#[cfg_attr(not(test), allow(dead_code))]` until the first real rename
wires them; a renamed *subcommand* and a `vilan.toml` key rename take
the same shape through the same head helper when they arrive.

**§5, the exercise — both synthetic legs shipped; the rehearsal
pending.** The mechanism leg runs twice: the self-deprecating fixture
program (per §5.1) and a FIXTURE STD — a copy of the real `std` with a
`deprecated_probe` module, resolved through `resolve_std` and the real
module loader — pinning both halves end to end: the std-marked item
warns at its user-code use (exactly the call site), and the same item
used only std-internally stays silent (the A2 pin). The process leg is
§3's pins: the jumped-window refusal, the released-match accept, the
same-train non-match, the pending report. **The voluntary two-train
window is PENDING, not simulated**: the standing instruction stands —
the next genuinely retire-shaped std ruling takes the window road even
if it lands before the switch (warn in train N with the markers, remove
in N+1). No candidate exists as of this record (E64/E71 drained the
queue; B127's `into` is Tier 3 and proves nothing).

**Follow-ups, unchanged from the paper:** the LSP's
`DiagnosticTag::Deprecated` strikethrough (free once wanted; not
scope), attribute placement beyond functions, `vilan.toml` key renames,
and the L10 package story.
