# The proposals move to their own repo (N15)

Status: PROPOSED 2026-08-20 (backlog N15; absorbs N2's archive-consolidation
question). A migration plan for the owner's nod — nothing here is executed.

The ask: pull `vilan/proposal/` out of the compiler repo into a dedicated
org repo (working name `vilan-lang/proposals`). The directory is the
project's design memory — the papers, the trackers, the archive — and it
has outgrown cohabitation: 794 of the 869 commits that touch it touch
nothing else (measured on today's `next`, 1,662 commits total), so more
than half the compiler repo's history is prose traffic, and every
proposal-only push pays the full CI suite because the hygiene tripwire
reads the proposals (`.github/workflows/ci.yml:75-77` excludes
`vilan/proposal/**` from the prose filter on purpose).

The cost, stated up front and honestly: after the move, every lane brief
and every record sweep crosses a repo boundary, and a change that ships
code and updates its record can never again be one commit.

## 1. What moves

Everything under `vilan/proposal/` — 102 files, 3.5 MB:

- **94 papers** (`affine-moves.md` … `windows-support.md`), including the
  living references (`process.md`, `beta.md`, `releases.md`,
  `kolt-migration.md`, `diagnostics-ledger.md`).
- **4 trackers**: `backlog-2026-08-18.md` (the live planning surface),
  `backlog-archive.md` (append-only tombstones), and the frozen eras
  `backlog.md` (276 KB) and `backlog-2026-07-18.md`.
- **1 index**: `README.md` (this directory's per-file status table).
- **1 data file**: `perf-baseline.jsonl` (harness-regenerated rows behind
  `perf-baseline.md` §2 — it rides with its paper).
- **2 draft patches**: `e63-drafts/kolt-server.patch`, `todo-app.patch`.

There is no `archive/` subdirectory today — the flat layout is N2's
complaint, and §6 lands its answer in the new repo instead of here.

**What does NOT move:** `CHANGELOG.md` (release records; `cut-release.sh`
reconciles tags against it in the compiler repo), `AGENTS.md`, `CLAUDE.md`,
`AI_STANCE.md`, `CODE_OF_CONDUCT.md`, and the book (`vilan/docs/` — user
documentation, gated by the docs fence test, published to vilan-lang.org).
The book is what the language *is*; the proposals are why. That seam is
already stated on the book's Welcome page and stays.

## 2. History: extraction, not a clean import

Two honest options for what the new repo's history contains. Either way
the compiler repo's own history is untouched — no rewrite ever runs on
`vilan-lang/vilan`, because records cite compiler shas constantly
(process.md notes "the backlog's D section alone names `bb98564`…"), and
those must resolve forever.

**(a) `git filter-repo` extraction (recommended).** Clone the compiler
repo, run `git filter-repo --path vilan/proposal/ --path-rename
vilan/proposal/:proposal/`, push the result as the new repo's `main`.
Every paper keeps its full `git log` and `git blame` in the repo where it
now lives. The measurement above is why this is cheap and clean: 794 of
869 extracted commits are pure records commits that carry over whole;
only ~75 mixed commits get trimmed to their prose half.

**(b) Clean import.** One "import from vilan-lang/vilan @ <sha>" commit;
the compiler repo's history is the archive. Simpler, but every prose
`blame` in the working repo then answers "initial import 2026-08", and
every history question about a paper requires visiting a repo the papers
no longer live in.

**Where `git log -S` archaeology keeps working — the deciding weight.**
The record sweeps lean on it: ledger Batch 8 (diagnostics-ledger.md,
2026-08-19) triaged its 52 unmatched heads "by hand against `git log -S`".
Split that use into its two halves:

- *Code archaeology* ("when did this diagnostic head change?") runs over
  `crates/` in the compiler repo. Unaffected by the move under either
  option.
- *Prose archaeology* ("when did this tracker row change?", "this file's
  history" — the index README points there explicitly) is the half that
  moves. Under (a) it works in the new repo across the whole timeline,
  one repo, one command. Under (b) it forks at the cutover date: pre-move
  questions go to the compiler repo (`git log -S … -- vilan/proposal/`
  still works there against deleted paths, forever), post-move questions
  go to the new repo, and any question spanning the seam needs both.

**What extraction does not preserve, said plainly:** commit shas change
under filter-repo, so a record that cites a sha cites a *compiler-repo*
sha and must be resolved there — true under (b) as well, so not a
differentiator. And the extracted log's merge topology is thinned (empty
merges pruned). Neither cost touches a working query; the amputated-blame
cost of (b) touches one every sweep. Recommendation: **(a)**.

## 3. Paths: `proposal/` stays the top-level directory name

The citation corpus, counted on today's tree: **329** bare
`proposal/X.md` references in `crates/` alone (`grep -rn 'proposal/'
crates/` totals 342, of which 13 say `vilan/proposal`), **72** more in
`vilan/std`, plus `vilan/test` (19), `vilan/examples` (34),
`vilan/macro_std` (3), `vilan/benchmarks` (1). Tree-wide, the long form
`vilan/proposal` appears 34 times outside the directory itself. And the
records cite each other as `record: X.md §n` — same-directory relative.

So the new repo keeps the directory: `vilan-lang/proposals` with
`proposal/` at its root, every file at `proposal/<name>` — a strict 1:1
image of `vilan/proposal/<name>`. Yes, `proposals/proposal/` stutters;
in exchange:

- `proposal/X.md §n` — the dominant citation form, ~430 code-comment
  hits — resolves verbatim in the new repo and stays greppable.
- `vilan/proposal/X.md` citations keep their `proposal/X.md` suffix
  greppable, and resolve in compiler-repo history at any pre-move sha.
- `record: X.md §n` stays same-directory relative, untouched.

Flattening the papers to the repo root would break the first form
everywhere for cosmetics. Rejected.

## 4. The tracker moves with the papers

The orchestration reads `backlog-2026-08-18.md` at the top of every cycle
to draft the work order and lane briefs, and writes it at the bottom of
every cycle (status flips, tombstone sweeps into `backlog-archive.md`) —
all from the integration worktree. Could the tracker stay behind in the
compiler repo while the papers move? Argued and answered: **no — papers
and tracker stay together.**

- The tracker's rows cite papers same-directory-relative ("record:
  design-language.md §2.6", "see beta.md §3.1") and the papers cite
  tracker ids back. That tracker↔papers mesh is the densest citation
  cluster in the project — splitting it to spare the thinner code→paper
  cluster (which is citations-as-prose, not resolvable paths, §5) trades
  the wrong way.
- Records commits already travel as a unit: the 794 pure records commits
  of §2 are tracker-and-paper edits landing together. Leaving the tracker
  behind converts every one of those future commits into a cross-repo
  pair — the exact cost the split is supposed to avoid, paid on the
  hottest path instead of the coldest.
- The frozen trackers and `backlog-archive.md` are unambiguously archive
  material and move under any reading; a live tracker whose own history
  chain (`backlog.md` → `-2026-07-18` → `-2026-08-18` → the next
  re-baseline) straddles two repos would break the era-freeze convention.

**The concrete working arrangement:**

- A sibling checkout at `/home/reed/code/vilan-lang/proposals`, beside
  `vilan/`, `vilan-website/`, `vilan-playground/`, `vilan-branding/`,
  `vilan-lang.github.io/` — the workspace already has this shape.
- Branching: a single `main`, no `next`. The next/main split exists for
  the release train's fold; prose has no release. Protection can mirror
  whatever L6 settled, minus tags.
- The worktree convention carries over verbatim: sessions work in
  `proposals/.claude/worktrees/<lane>` branched from `main`, never the
  main checkout; integration merges `--no-ff` per cycle with the same
  arc-naming message discipline.
- Lane briefs: spec pointers become `proposals/proposal/X.md`
  (workspace-relative; absolute
  `/home/reed/code/vilan-lang/proposals/proposal/X.md` in briefs). A code
  lane gets a worktree *pair* — its compiler worktree plus a proposals
  worktree for its record — and its report names both branches. The
  integration session merges both repos at cycle close; the memory files
  and brief templates that hardcode `vilan/proposal/…` update at cutover
  (they live outside the repo, so this is a checklist item, not a
  commit).

## 5. What breaks, and the fix list

Every hit below was grepped on today's `next` and is named with its line.

**Functional (code reads the path — must change in the freeze stack):**

1. `crates/vilan-cli/tests/hygiene.rs:122,126,130,134` — the only test in
   the workspace that functionally depends on `vilan/proposal/` paths.
   `OWNER_STRING_ALLOWLIST` names `vilan/proposal/org-migration.md`,
   `backlog-2026-07-18.md`, `releases.md`, `backlog.md`; all four files
   move, so the rows go dead. Fix: delete the four rows (the gate stays
   non-vacuous — it still scans every remaining tracked file); the
   allowlist reappears in the new repo's gate (§6). Note the doc comment
   at hygiene.rs:142 cites `vilan/proposal/org-migration.md` — reword.
2. `scripts/cut-release.sh:338` — the reconciliation sweep's
   `grep -v -e '^vilan/proposal/'` when checking whether a CHANGELOG
   entry's introducing commit touched code. **Keep this exclusion**: the
   sweep runs `git show` on historical commits, and pre-move commits list
   `vilan/proposal/` paths forever. Deleting it would misclassify old
   entries; it just never matches new commits.
3. `.github/workflows/ci.yml:74-77` — the prose filter's comment says
   `vilan/proposal/**` is deliberately not prose "the hygiene tripwire
   reads the proposals". Post-move the rationale is void: proposal paths
   can't appear in a diff at all. Fix the comment; the `PROSE` regex
   needs no change. (Side benefit, worth naming: records pushes stop
   costing a full suite run — including the Windows leg — entirely.)

**Verified NOT affected (the suspects were checked and cleared):**

- `crates/vilan-cli/tests/book_mirrors.rs` reads only
  `vilan/docs/theme/…` (its consts at lines 26-27). Unaffected.
- The D17-family gates: `grammar_sync.rs` diffs the lexer, the TextMate
  grammar, and `vilan.js` — no proposal path. `perf_baseline.rs` cites
  `proposal/perf-baseline.md` in prose only; it writes its rows to stdout
  logs, not to `perf-baseline.jsonl` (the jsonl is hand-committed beside
  its paper and greps to no reader in `crates/` or `scripts/`).
- `examples.rs`'s `git ls-files` is scoped `-- vilan/examples`. The docs
  gate compiles `vilan/docs/` fences only. No other test walks the
  proposal tree (`grep -rln "ls-files" crates/` = hygiene.rs +
  examples.rs).

**Pointers (prose that must be updated in the freeze stack):**

4. `AGENTS.md:41-42` ("`vilan/proposal/` — design documents…"),
   `AGENTS.md:137-139` (the read-the-proposal-first rule naming
   `vilan/proposal/backlog-2026-08-18.md`), plus the passing citations at
   lines 14, 26, 34.
5. `CLAUDE.md:19` ("a proposal under `vilan/proposal/`") and `:35`
   (`vilan/proposal/documentation.md`).
6. The book's published links — nothing gates these, so they rot
   silently if missed: `vilan/docs/README.md:6` (the Welcome page's
   "Design history and rationale live in…" — the line docs-site.md §
   non-goals specced), `spec/names.md:34`, `spec/introduction.md:16`,
   `spec/lexical.md:19`, `spec/memory.md:8,9,393`,
   `std/reactive.md:60,246` — nine `github.com/vilan-lang/vilan/…/vilan/
   proposal/…` URLs that 404 once the freeze deletes the files. Fix:
   point at `github.com/vilan-lang/proposals/blob/main/proposal/…`.
7. The four `vilan/docs/book.toml` comment citations (lines 1,6,7,44)
   and comment-citations in `scripts/` and `.github/workflows/` — these
   are records, not links; update opportunistically, never as a sweep.

**Deliberately left alone:** the ~430 `proposal/X.md §n` code-comment
citations (§3 keeps them resolvable in the new repo), `CHANGELOG.md`'s 5
`vilan/proposal/` record lines (historical prose), and every citation
inside the moving papers themselves (they move together; relative cites
survive).

## 6. CI on the new repo, and N2's archive layout

Of the compiler suite, exactly three hygiene checks apply to prose, and
they are the checks that scan the proposals today: the absolute-home-path
needle set, the personal-mailbox needle set (its allowlist names only
`THIRD-PARTY-NOTICES.txt` — nothing moves), and the owner-string needles
with the four allowlist rows from §5.1. Port them as one small CI script
(shell or python, one workflow) — a Rust workspace to run three greps
over markdown is not warranted. Keep the compiler gate's discipline:
needles assembled at runtime so the checker never trips itself.

Add one gate the compiler repo never had: **index completeness** — every
`proposal/*.md` has exactly one row in `proposal/README.md`. Today's
index carries a duplicated `design-language.md` row (found, and fixed
beside this paper); a one-per-file check would have caught it.

**N2's archive layout lands here as directory structure.** After the 1:1
import is verified (§7), a follow-up commit *in the new repo* creates
`proposal/archive/` and moves the dead generations and frozen trackers —
`memory-management.md`, `memory-management-rev-1.md` (superseded chain),
`roadmap.md`, `backlog.md`, `backlog-2026-07-18.md` — leaving one-line
banner pointers at the old paths so every "record: backlog.md" citation
still lands on a file that says where the text went. The banners answer
N2's stated cost ("countless prose citations say record: backlog.md")
at the price of five stub files. Exact membership is an owner call (§8).

## 7. The cutover sequence

1. **Nod.** Owner answers §8; a freeze sha on `next` is chosen between
   cycles (no lane in flight against the tracker).
2. **Create** `vilan-lang/proposals`, empty, public (same license posture
   as the compiler repo — §8).
3. **Import.** Fresh clone of the compiler repo at the freeze sha;
   `git filter-repo --path vilan/proposal/ --path-rename
   vilan/proposal/:proposal/`; verify the tip tree is byte-identical to
   the freeze sha's `vilan/proposal/` (`diff -r`, all 102 files); push as
   `main`. Then the scaffolding commit (repo README, LICENSE pair, the
   §6 CI workflow) and the §6 archive commit — *after* the verified 1:1
   import, so the move itself is one trivially-auditable diff.
4. **Freeze the old directory.** One stack on `next` through the normal
   train: delete `vilan/proposal/*` except a banner `README.md` naming
   the new repo, the freeze sha, and the standing rule that this
   directory's history stays queryable here (`git log -S … --
   vilan/proposal/` against deleted paths works forever); in the same
   stack, the §5 fixes (hygiene.rs rows, ci.yml comment, AGENTS.md,
   CLAUDE.md, the nine book links). Suite green — hygiene itself proves
   the allowlist cleanup right.
5. **Update the machinery.** The sibling checkout cloned to
   `/home/reed/code/vilan-lang/proposals`; memory files and brief
   templates re-pointed; the integration worktree convention extended
   per §4.
6. **First post-move cycle checklist:** briefs carry `proposals/…` spec
   paths; every lane that writes records gets its proposals worktree and
   reports both branches; integration merges both repos at close; the
   records sweep runs its greps in both repos once and confirms every
   `record:` cite in the cycle's new prose resolves; hygiene green on
   both sides; nothing tagged in the proposals repo, ever.

## 8. Owner questions

1. **Name**: `vilan-lang/proposals` assumed. Bless, or prefer `design` /
   `records`?
2. **History**: §2 recommends filter-repo extraction over a clean
   import. Confirm.
3. **License/visibility**: inherit the MIT/Apache-2.0 dual license and
   public visibility?
4. **Archive membership** (absorbed N2): the five files named in §6 —
   should the superseded `roadmap.md` and both dead memory-management
   generations all go under `archive/`, or trackers only?
5. **Book links**: §5.6 points the nine published links at the new
   repo's `main`. Alternative: pin them to compiler-repo blob-at-sha
   URLs (immutable forever, but frozen). Live links assumed.
6. **Tracker home**: §4 concludes papers and tracker move together, one
   `main`, lane worktrees on both repos. Confirm the working
   arrangement before any brief is written against it.
