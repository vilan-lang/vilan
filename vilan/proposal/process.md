# Process — releasing, protecting, and taking contributions

> Status: DRAFT (awaiting owner review) — filed from the owner's 2026-08-07 process ask.

## 0. The ask, and what this paper settles

The owner's framing, verbatim:

> releases are currently too frequent for an audience of "myself, a tester,
> and you" and waste GitHub runner resources … as we graduate from alpha to
> beta we'll want a more stable release schedule … Perhaps beta is weekly
> with important patches released in between? … we should begin moving
> towards best practices for public projects: PRs, protected default, etc.
> Perhaps rebases on merge? I'm open to whatever is best. Operating a public
> project of this scale is somewhat new to me.

One constraint frames everything below: **work continues at the same pace.**
Nothing here slows a lane, delays a merge, or puts a reviewer in front of
work that has no second reviewer. The only thing that becomes less frequent
is `git tag`.

Six sections, each ending in a recommendation; then the open questions the
owner must rule on, and a list of things in the tree today that contradict
the recommended policy and must be amended when it is ratified.

---

## 1. Release cadence

### 1.1 What the cadence actually is

Forty-four public releases between v0.2.0 (2026-07-13) and v0.32.0
(2026-08-06). Twenty-four calendar days. **1.8 releases per day, sustained,
for three and a half weeks.**

The recent stretch is denser still. Twenty-seven tags between v0.16.0
(2026-07-28) and v0.32.0 (2026-08-06) — seventeen minor bumps and ten
patches — over ten calendar days, or **2.7 releases per day**. The peak was
2026-08-03: eight tags in one day (v0.23.0 through v0.23.6, then v0.24.0).
Three cut on 2026-08-06 alone.

The audience for all forty-four is three, one of whom is a compiler.

### 1.2 What a cut costs, measured

The figures below are per-job wall times pulled from the Actions API for the
last five release runs and five representative CI runs, not estimates.

**The release workflow** (`release.yml`, 13 jobs producing 10 assets —
five platform archives, the `.vsix`, the playground wasm tarball, the two
install scripts, `sha256sums.txt`):

| tag | `gate` | everything else | total job-minutes |
|---|---|---|---|
| v0.28.0 | 4.7 | 16.3 | **21.0** |
| v0.29.0 | 6.2 | 14.5 | **20.7** |
| v0.30.0 | 9.2 | 16.6 | **25.8** |
| v0.31.0 | 21.3 | 14.6 | **35.9** |
| v0.32.0 | 26.9 | 14.2 | **41.1** |

Two things fall out. The build matrix and the five publish jobs cost about
**15 job-minutes, flat**, across every release — that part of the pipeline is
cheap and well-built. Everything above it is the `gate` job re-running the
suite, and that number has quadrupled in four releases.

**A CI run** (`ci.yml`, 3 jobs: ubuntu, windows, wasm) costs **22–32
job-minutes** under normal conditions (median 28.0 across five sampled runs).
On 2026-08-07 the hosted runners were degraded and the same three jobs cost
**102–117 job-minutes**; that tail is not hypothetical and it is not
controllable.

**Two site deploys** ride every cut: a dispatch of `docs.yml` on
`vilan-lang.github.io` (rebuilds the book from `vilan@main`) and a dispatch
of `deploy` on `vilan-lang/website` (reinstalls the toolchain from the
latest tagged release and pulls that release's playground wasm). Together
with the Pages build they are about 1.5 job-minutes — negligible in compute,
two manual acts in practice.

**Marginal cost of one cut**, at today's measured numbers:

| leg | job-minutes |
|---|---|
| CI on `next` for the `release:` commit | ~28 |
| the release workflow on the tag | ~26 |
| CI on `main` for the `Merge next` fold | ~28 |
| two site deploys + the Pages build | ~1.5 |
| **total** | **~85** |

Wall clock from release commit to a live site, v0.32.0: pushed 02:21,
published 02:53, site deployed 02:55, `main`'s CI finished 03:55. Call it
**an hour and a half of elapsed pipeline per cut**.

Multiply out: the ten-day stretch to v0.32.0 cost roughly **2,300
job-minutes — thirty-eight job-hours** in cut-attributable CI. Since v0.2.0,
roughly **3,700 job-minutes, sixty-two job-hours.**

### 1.3 Honest accounting: the meter reads zero

The repository is public, and GitHub does not meter standard runners on
public repositories. The API says so plainly: `/actions/runs/<id>/timing`
for the v0.32.0 release run returns `total_ms: 0` for all thirteen jobs
across UBUNTU, MACOS and WINDOWS. **No invoice is being generated.**

That is worth stating because it changes which argument is load-bearing. If
the same run were metered at the standard private-repository weights (linux
×1, windows ×2, macOS ×10), v0.32.0's forty-one job-minutes would be about
**eighty-eight minute-equivalents**, with the two macOS legs alone weighing
forty. That is the number to quote when reasoning about whether this
pipeline is *proportionate*. It is not the number to quote as a bill.

So the case for cutting less often does not rest on money. It rests on three
things that are real:

1. **Shared capacity and queue time.** Release run 31115456975 (v0.30.0,
   created 2026-08-06 15:21) still showed `queued` with no conclusion six
   hours later at 21:42; the release actually went out from a second run at
   01:37 the next morning. Frequency meets a busy queue and the cut stops
   being a five-minute act.
2. **The human cost per cut, which does not appear in minutes at all.**
   Every cut runs the reconciliation sweep, the version bump across three
   crate manifests plus the extension manifest, a changelog retitle, a tag,
   a fold to `main`, and two manual workflow dispatches. Multiply by 2.7 per
   day.
3. **Risk per cut.** Each release publishes to five channels — GitHub
   Releases, npm (six packages), the VS Code Marketplace, Open VSX, and the
   Homebrew tap — and several of those are one-way: `release.yml`'s own
   comments record that an npm version "can be deprecated but never
   replaced", and that v0.18.0 shipped five of six npm packages and read
   green. Every cut is one more roll of that die for an audience of three.

### 1.4 Alpha, from now until the beta trigger

**Accumulate cycles on `next`; cut at most weekly, or on an urgent trigger.**

Cycles keep landing on `next` the day they finish. The `## Unreleased`
section of `CHANGELOG.md` simply grows for a week instead of a few hours.
Nothing else about the working day changes.

"Urgent" must be a short, closed list, or it becomes "whatever felt
important on Tuesday". Three conditions, and only these three:

- **U1 — a miscompile in a shipped release.** The compiler accepts a
  program, the program runs, and the answer is wrong, reachable from
  ordinary code. This is not a hypothetical class: v0.32.0's own headline
  entries are four of them (a capture re-reading a slot through a `&mut`
  view, duplicate enum discriminants collapsing two variants onto one
  branch, `for x in set` walking a set exactly once, `Type::static()`
  resolving by registration order). A user whose running program is wrong
  and silent does not get to wait for Saturday.
- **U2 — a security issue** in the toolchain, in a published artifact, or
  in the install path (`install.sh` / `install.ps1` / `vilan upgrade` /
  the tap formula).
- **U3 — a broken toolchain.** The released binary fails to install, fails
  to start, or cannot compile a hello on a supported platform. The installed
  binary is the product; a broken one is not a bug report, it is an outage.

Everything else waits: features, diagnostics improvements, performance,
ergonomics, and anything a tester is merely excited about. A fix being
*good* is not a trigger. A fix being good is why there is a train.

### 1.5 Beta: a weekly train with out-of-band patches

The owner's own suggestion, fleshed out.

**The train.** One `0.MINOR.0` per week, on a fixed weekday, cut from
`next`. Two properties make it a train rather than a schedule:

- **It may be skipped, and a skip is not an event.** If the week produced
  nothing a user can observe, do not cut. A release with nothing in it is
  worse than no release: it spends the pipeline, spends the channels, and
  teaches users that upgrading is meaningless.
- **It is never delayed for a feature.** Work that is not on `next` on cut
  day rides the next train. This is the whole point of a train, and it is
  the discipline that makes "cut less often" cost nothing: no one is ever
  waiting on the cut, because the cut is never waiting on anyone.

**The patch.** A `0.MINOR.PATCH` cut between trains, carrying **exactly one
thing**: a fix for a U1/U2/U3 condition, plus its changelog entry, and
nothing else. That exclusivity is what makes a patch safe to take without
reading — the promise of a patch release is "this changes one behavior, the
broken one".

**The mechanics, which the repository cannot do today.** Every tag in the
history is on `next`'s own line: `release: v0.32.0` is a commit on `next`,
tagged there, and `main` is a `Merge next` of it. That topology has no place
to put a patch. Cherry-picking a fix onto `next` and tagging drags in a
week of unreleased work — which is precisely the thing the train exists to
avoid.

The minimal addition:

1. At each train, after tagging `v0.MINOR.0`, do nothing extra. No branch is
   created speculatively.
2. When a patch is needed, branch **`release/0.MINOR` from the tag** —
   lazily, only now, only because a patch exists.
3. Cherry-pick the fix onto it. Bump to `0.MINOR.PATCH`. Write the changelog
   section. Tag `v0.MINOR.PATCH` on that branch, push the tag: `release.yml`
   triggers on `v*` regardless of branch, so the existing pipeline needs no
   change at all.
4. **Merge `release/0.MINOR` back into `next` with `--no-ff`** (never
   rebase it), so the fix and its changelog entry cannot be lost at the next
   train. If `next` already carries the fix — the usual case, since the fix
   should land on `next` first and be cherry-picked *from* there — the merge
   is trivial and the changelog entry unions the way lane merges already do.
5. Delete `release/0.MINOR` once the next train ships. It is a scaffold, not
   a maintained branch. This project does not backport, and should not
   pretend it might.

That is one branch, existing for hours at a time, a handful of times a year.
It is the smallest structure that makes "important patches released in
between" mean anything.

**Recommendation (§1).** Alpha now: accumulate cycles on `next` and cut at
most weekly, or immediately on U1/U2/U3. At beta: a weekly train on a fixed
weekday, skippable, never delayed, plus out-of-band `0.MINOR.PATCH`
releases cut from a lazily created `release/0.MINOR` branch off the tag and
merged back into `next`. Measured saving: cut-attributable CI drops from
roughly 1,600 job-minutes a week to roughly 85 — **about 95%** — with no
change to how fast anything is built.

---

## 2. Branch protection

### 2.1 What is protecting these branches today

Nothing.

`GET /repos/vilan-lang/vilan/branches/main/protection` returns
`404 Branch not protected`. Both `main` and `next` report
`protected: false`. Force-pushing `main`, deleting it, or pushing an
untested commit straight to it are all one command away.

There is, however, a ruleset already sitting in the repository: **`Protect
default`, id 18887216, created 2026-07-13, `enforcement: "disabled"`.** It
targets `~DEFAULT_BRANCH` and carries four rules — `deletion`,
`non_fast_forward`, `pull_request` (0 required approvals), and
`required_signatures` — with `bypass_actors: []` and
`current_user_can_bypass: "never"`.

Read that configuration carefully and it is obvious why it has been off for
a month: **enabling it as written locks the owner out.** No bypass actor
means the `Merge next` fold must go through a PR; `required_signatures`
means every commit on `main`, including the ones `scripts/bump-version.sh`
produces, must be signed. Someone drafted the right instinct, correctly
declined to enable it, and left it there. It should be amended, not enabled.

### 2.2 Rulesets, not classic protection

Use **rulesets**. Four reasons, all of which matter here:

- One ruleset can target several refs (`~DEFAULT_BRANCH` and
  `refs/heads/next`) where classic protection needs a rule per branch
  pattern.
- `bypass_actors` is explicit, enumerable and auditable, with a per-actor
  mode (`always`, or bypass only for pull requests). Classic protection's
  "include administrators" checkbox is a single blunt bit.
- A ruleset can be set to **`evaluate`** — enforcement off, violations
  logged — which is a dry run against real traffic before anything blocks.
  That is the right first step here and classic protection has no
  equivalent. (Verify availability on this account's plan before relying on
  it; if `evaluate` is not offered, enable with the owner in `bypass_actors`
  and read the bypass entries in the audit log instead, which gives the same
  information a week later.)
- The repository already has exactly one ruleset object. Amending it keeps
  the answer to "what protects `main`?" in one place instead of two.

### 2.3 `main`

`main` is the default branch, and it moves once per cut — a single
`Merge next` commit. Protecting it is therefore almost free: it constrains
one mechanical act per week.

Recommended rules on `~DEFAULT_BRANCH`:

- **`deletion`** and **`non_fast_forward`.** No deletions, no force pushes.
  These are unconditional and cost nothing.
- **`pull_request`** with `required_approving_review_count: 0`. Zero, not
  one. On a solo project a required approval cannot be satisfied — GitHub
  will not let an author approve their own PR — so requiring one converts
  every merge into a bypass, and a rule that is always bypassed is not a
  rule. Zero approvals still buys the thing that matters: the change is
  proposed as a PR, the required checks attach to it, and merging is a
  deliberate second act.
- **`required_status_checks`**, naming `ci.yml`'s aggregate job (see §2.5).
  This is the rule with real teeth, and §2.5 is what makes it usable.
- **`allowed_merge_methods: ["merge", "squash"]`.** No `rebase` — see §3.
- **`bypass_actors`: the owner, mode `always`** — initially. The point of
  this ruleset at three users is to make the wrong thing require a
  deliberate act, not to make it impossible. A locked-out maintainer at 2
  a.m. during a U3 outage disables the ruleset, and a disabled ruleset stays
  disabled — which is precisely the failure mode already sitting in the
  repository.
- **`required_signatures`: not in the first slice.** Signed commits are a
  good end state and a separate project: the owner's machine needs signing
  configured, and so does anything that commits (`bump-version.sh`'s release
  commit; the `publish-brew` job already commits to the tap as
  `github-actions[bot]`, which cannot sign). Bundling it with PR-only is how
  this ruleset ends up disabled a second time.

### 2.4 `next`

`next` is where the orchestrated lane workflow lives. Lanes branch from it,
land on it with `--no-ff` merges, and the CHANGELOG is unioned at each
merge. **Maintainers must keep pushing to it directly**, so:

- **`deletion` and `non_fast_forward`: yes.** `non_fast_forward` is the rule
  that actually matters on `next` — it makes lane history unrewritable,
  which is the property the CHANGELOG-union discipline and the
  reconciliation sweep's ancestor check both depend on.
- **`pull_request`: no.** A PR requirement on `next` would put a
  reviewerless PR in front of every lane merge, at the cadence lanes merge.
  It buys nothing and taxes the one thing this policy promised not to slow.
- **`required_status_checks`: no, and this is deliberate.** Required checks
  gate the commit being introduced; on a branch that is *pushed to
  directly*, the checks for the pushed commit do not exist yet at push time.
  Putting required checks on `next` makes direct pushes fail or forces a
  standing bypass. Required checks belong on `main`, where changes arrive by
  PR.

**"PRs required for external contributors" needs no rule.** External
contributors have no write access to this repository, so they cannot push to
`next` or `main` by construction; a fork PR is their only path. The rules
above exist to constrain *maintainers*, which is the only place a rule can
have effect.

### 2.5 The required-checks footgun `ci.yml` already documents

`ci.yml` lines 10–12 predict the problem in the file itself:

> If these checks ever become required on protected PRs, switch from
> paths-ignore to an always-running filter job — a skipped required check
> leaves a PR stuck at "Expected".

That is exactly right and it is worse than it sounds. `ci.yml` carries
`paths-ignore` on **both** the `pull_request` and `push` triggers, listing
`README.md`, `AI_STANCE.md`, `AGENTS.md`, `CLAUDE.md`,
`CODE_OF_CONDUCT.md`, both licenses, and `.gitignore`. A PR touching only
those files produces **no workflow run at all** — not a skipped job, no run.
A required check that never reports leaves the PR at "Expected — waiting for
status to be reported", permanently.

And look at that list. It is, almost exactly, **the set of files a first-time
external contributor is most likely to touch.** A README typo fix would be
the project's first external PR and it would be unmergeable.

Moving the filter to the job level does not fix it either: with
`paths-ignore` at the `on:` level the workflow does not trigger, so an
aggregator job inside it would not run either. The filter has to move
*into* the workflow.

The shape that works:

1. `on:` loses `paths-ignore` entirely. The workflow always triggers.
2. A cheap `changes` job (ubuntu, a `git diff --name-only` against the base,
   seconds) outputs whether any non-prose path changed.
3. `test` and `wasm` gain `needs: changes` and
   `if: needs.changes.outputs.code == 'true'`.
4. A final job — call it `ci` — with `needs: [changes, test, wasm]` and
   `if: always()`, which fails if any needed job's result is `failure` or
   `cancelled` and passes if they succeeded *or were skipped*.
5. **`ci` is the required check.** It always runs, it is green on a
   prose-only PR, and it is red whenever a real leg failed.

Cost: about ten seconds of ubuntu per prose-only PR, where today there is
zero. That is the price of a required check that can actually go green, and
it is worth paying.

### 2.6 Three free settings that are currently off

`security_and_analysis` reports `secret_scanning`,
`secret_scanning_push_protection` and `dependabot_security_updates` all
**disabled**. All three are free on a public repository. Push protection is
the one that matters most here: the release path handles
`TAP_APP_PRIVATE_KEY`, `AZURE_CLIENT_ID`, `AZURE_TENANT_ID` and
`OVSX_TOKEN`, and the failure mode it prevents is a credential committed by
accident to a public repository.

`delete_branch_on_merge` is `false`; turn it on. It deletes only the *head*
branch of a merged PR, and under §3 lane branches are not merged via PR, so
the lane workflow is untouched. (If lanes ever do move to PRs, revisit this
first.)

**Recommendation (§2).** Amend ruleset 18887216 rather than enabling it:
drop `required_signatures`, add the owner to `bypass_actors` with mode
`always`, set `pull_request` to zero required approvals, add
`required_status_checks` naming the new `ci` aggregate job, and restrict
`allowed_merge_methods` to merge and squash. Add a second ruleset on
`refs/heads/next` with `deletion` and `non_fast_forward` only — no PR
requirement, no required checks. Set both to `evaluate` for one week, read
the violations, then enforce. Rework `ci.yml` per §2.5 **before** the checks
become required. Enable secret scanning, push protection and Dependabot
security updates.

---

## 3. Merge strategy

The owner asked specifically: "Perhaps rebases on merge?" It deserves the
real argument, not a reflex.

### 3.1 The case for rebase-on-merge, made properly

- **A linear first-parent history.** `git log` on `main` reads as a sequence
  of changes with no merge commits to step over. For someone arriving cold,
  that is genuinely easier — and this project is about to start acquiring
  people who arrive cold.
- **No merge bubbles to interpret.** A newcomer reading a merge-heavy graph
  has to learn the project's branching model before the history means
  anything. A linear history means the same thing everywhere.
- **`git bisect` walks a straight line.** No decisions about which parent to
  follow.
- **Every commit lands in the position it will occupy**, so in principle each
  one is a coherent state of the tree. (In principle: GitHub's rebase-merge
  does not re-run CI on the rewritten commits, so this is a claim about
  *shape*, not about evidence.)

These are real. On a repository with short-lived feature branches, a single
reviewer, and no cross-references into its own history, rebase-merge is
often the right default.

### 3.2 Why it is the wrong default for this repository, specifically

**(a) The merge commit is the cycle record.** The last twenty merges read
like `merge: the loop keeps its iterable's type, and the watch budgets
measure the program (B85, E39)` and `merge: declaration validation lands
(B79, B84, B83)`. One line naming the arc and its backlog ids. The commits
underneath are `suite: …`, `records: …`, `fix: …` — correct, granular, and
individually silent about what the lane was *for*. The merge commit is the
only place the thesis is written. Rebase-merge deletes it, and
`git log --merges --oneline`, which is how this history is actually read,
returns nothing at all.

**(b) The CHANGELOG union is resolved once, at the merge, with full
context.** Two of the last five merges carry `# Conflicts: CHANGELOG.md` in
their message. That is the discipline working: two lanes each wrote an
Unreleased entry, and the merge unioned them, once, with both texts in front
of the person doing it. Rebase-merge replays each lane commit onto the new
tip and asks for that resolution **per commit**. A five-commit lane that
touched `CHANGELOG.md` three times becomes three resolutions instead of one,
each with strictly less context than the single merge had. This gets worse
under §1, not better: a week-long `## Unreleased` section is a bigger,
denser conflict surface.

**(c) Bisect is already fine.** Lane commits are granular and honest
(`suite: the cancellation e2e times the join, not the compiler`;
`suite: eleven vilan run watchdogs stop asserting a compile`) and they are
on the graph. `git bisect` traverses merges and lands inside a lane without
help. And if the *first* question is "which lane broke it" — usually the
more useful one — `git bisect --first-parent` answers it directly, which is
a capability the merge topology provides and a linear history destroys.

**(d) Rewriting breaks the project's own citations, mechanically.** The
record cites shas constantly: the backlog's D section alone names `bb98564`,
`5c351a0`, `b1f42b8`, `156166b`, `5bb74b9`, `0a0bdd4`. Rebase-merge mints
new shas for every lane commit, so every sha written down during a lane's
life points at a commit that is no longer reachable. Worse,
`releases.md` §7(a) — the reconciliation sweep — *literally runs*
`git merge-base --is-ancestor <commit> HEAD` against those shas before a cut.
**Rebase-on-merge breaks the release procedure's own verification step by
construction.**

Point (d) is decisive and is not a matter of taste.

### 3.3 External PRs are a different question

An external contributor's branch history is theirs, and it is usually
"fix", "fix again", "address review", "oops". The project's unit of record
is the change, not the contributor's process. **Squash-merge external PRs**:
one commit, one message in the house voice, one sha to cite in a backlog
entry or a changelog section.

One caveat worth configuring correctly: set
`squash_merge_commit_message: COMMIT_MESSAGES` so the squashed body carries
the original commits' trailers, and `Co-Authored-By` survives. For a project
whose `AI_STANCE.md` is fundamentally about honest attribution, silently
dropping co-authorship on the first external contribution would be a poor
look.

**Recommendation (§3).** Keep `--no-ff` merge commits for internal lanes.
Squash-merge external PRs, with `COMMIT_MESSAGES` as the squash body so
trailers survive. **No rebase-on-merge** — and set
`allow_rebase_merge: false` in repository settings so the button is not
there to be pressed during an incident. `main` keeps receiving `next` as a
merge; nothing about the fold changes.

---

## 4. Contribution scaffolding

### 4.1 What exists, and what does not

Present and in good shape: `LICENSE-MIT` and `LICENSE-APACHE`;
`CODE_OF_CONDUCT.md`; `AI_STANCE.md`; `CLAUDE.md` and `AGENTS.md` (the house
rules, written for agents but substantially applicable to humans); Issues
and Discussions both enabled. The README's License section already carries
the inbound-equals-outbound clause — "any contribution intentionally
submitted … shall be dual licensed as above, without any additional terms" —
which is the single most important legal sentence a project of this shape
needs, and it is already correct.

Absent: `CONTRIBUTING.md`, `SECURITY.md`, `CODEOWNERS`, any issue or PR
template. `.github/` contains two workflow files and nothing else. Zero
forks, one star, zero open issues.

One cosmetic note: GitHub's license detection reports **Apache-2.0 only**,
because a dual license expressed as two files with no `LICENSE` gives the
detector one thing to pick. See the open questions.

### 4.2 Order, and the reason for the order

The governing principle: **do not write documents that describe things which
do not exist.** At one maintainer and zero forks, a governance model, a
maintainer ladder, an RFC process with numbers and comment periods, and a
triage rota are all fiction. `vilan/proposal/` already *is* the RFC process,
it demonstrably works, and its public form is one paragraph in
`CONTRIBUTING.md` — not a new mechanism.

**Slice 1 — before any promotion. This is what D5's session unblocks.**

1. **`CONTRIBUTING.md`.** One page. Mostly a translation of `AGENTS.md`'s
   Definition of Done into terms a human outsider can act on:
   - how to build (`cargo build`) and what the gate is
     (`cargo nextest run --workspace`, judged by the runner's exit code,
     never through a pipe — that trap is worth stating for humans too);
   - the corpus's byte-identical goldens and the stop condition that follows:
     *if an existing golden changes, say so in the PR — do not regenerate*;
   - docs are part of done: a change to std, a framework, or the language
     updates the affected `vilan/docs/` page in the same change;
   - one pin per case, not one representative example;
   - `cargo fmt`, four-space Rust indent, full variable names.

   Plus the two things `AGENTS.md` does not need to say and a stranger does:

   - **design lands in `vilan/proposal/` before code.** Say it early and say
     it kindly, so nobody writes a feature that gets rejected on semantics
     after a weekend of work.
   - **what will not be accepted**, stated plainly: a special case that
     quiets a checker it should have failed; any change that weakens a gate
     in order to pass it. Both are already stop conditions internally; both
     are exactly what a well-meaning outsider tries first.

   Link `AI_STANCE.md` as the AI policy.

2. **`SECURITY.md`.** Three lines and a channel. This is the genuinely
   urgent one, because the alternative to a stated private channel is a
   public issue describing an exploitable bug. Recommend GitHub's **private
   vulnerability reporting** (a repository setting): no address to publish,
   nothing that interacts with the pseudonym discipline D5's entry flags,
   and it is free.

3. **Enable secret scanning and push protection** (§2.6). Not paperwork, but
   it belongs in this slice.

**Slice 2 — when the first external PR or issue actually arrives.**

4. **`CODEOWNERS`**, one line: `* @ReedSyllas`. It does nothing today with
   zero required approvals, and that is fine — it is a placeholder that makes
   `require_code_owner_review` available later without a second decision.

5. **A PR template**: three prompts, not a form. *What changed and why. What
   you ran, and the exit code. What you did not verify.* That is `AGENTS.md`'s
   reporting contract verbatim, and it is the highest-value template in the
   set, because it is what makes a review possible without a round trip.

**Slice 3 — when issue volume justifies triage.**

6. **Issue templates: two, and deliberately not three.** A **bug** form
   (`vilan --version` output, platform, a minimal `.vl` program, expected vs
   actual) and a **contact link** routing questions to Discussions. Set
   `blank_issues_enabled: false`.

   **No feature-request template.** For a language project whose entire
   discipline is design-in-proposal-first, a feature-request form is an
   invitation to design-by-issue, and every one of them costs a polite
   explanation of a process the issue tracker cannot host. Route feature
   ideas to Discussions, and say in `CONTRIBUTING.md` that a feature becomes
   real by becoming a proposal.

**Recommendation (§4).** Ship `CONTRIBUTING.md`, `SECURITY.md` (private
vulnerability reporting), and the two security settings as one slice before
any promotion. Add `CODEOWNERS` and a three-prompt PR template when the
first external PR arrives. Add a bug template and a Discussions contact link
when issue volume justifies it. Write no governance documents.

---

## 5. Versioning: alpha → beta

### 5.1 What alpha currently promises

`CHANGELOG.md`'s header, verbatim:

> Vilan is a fast-moving alpha. Minor versions (`0.X`) may change the
> language, the standard library, and the wire protocol without a
> deprecation period; patch versions are fixes.

`releases.md` §4 agrees and goes further: "minor bumps may break anything
(the alpha promise); patch bumps are fixes. **Bump minor liberally.**" The
README's status line says "The language changes weekly."

All three are honest, and all three are currently true.

### 5.2 What beta must add — exactly three things

Beta is not "nearly 1.0". Beta is one sentence: **your program will keep
compiling for longer than a week.** Three commitments deliver that, and
nothing else needs to be promised.

1. **A deprecation window replaces a removal.** A breaking change to the
   language or std ships first as a *warning* that names its replacement;
   the removal comes no earlier than the following minor. One minor of
   warning — not two, not a calendar quarter. The diagnostics standard
   already gives the compiler the machinery to say "X is deprecated; use Y"
   well, which is most of the cost already paid.
2. **Breaking changes get their own heading.** The changelog already writes
   migration prose better than most projects write documentation; beta makes
   a `### Breaking` subsection mandatory in each minor's section, so a user
   scanning for "does this break me" reads one heading instead of five
   essays.
3. **Wire-format changes are always breaking entries.** The one thing that
   breaks a *running deployment* rather than a build is the RPC/transport
   wire contract — a server on `0.N` meeting a client on `0.N+1`. Under beta
   a wire change is a Breaking entry, unconditionally, even when both ends
   recompile cleanly.

### 5.3 What beta must NOT promise: a spec freeze

A per-minor spec freeze sounds like the responsible thing and would be the
most expensive item on this list. Look at what the recent releases actually
contain: v0.30–v0.32 are almost entirely *semantics corrections* —
method-resolution precedence, enum discriminant validation, captures of a
viewed subject, `for x in set`. A spec freeze converts every one of those
into a version-planning exercise, and the compiler's whole approach is to
fix the general path immediately.

Draw the line where it belongs: **beta freezes removal, not correction.** A
miscompile fix that changes observable behavior is not a breaking change
under any beta promise worth making — the previous behavior was never the
contract, it was a bug. `CONTRIBUTING.md` and the changelog header should
say so in as many words, because the alternative is a user filing "you broke
my program" about a program that was already wrong.

The spec keeps tracking the compiler. It is a description, not a treaty.

### 5.4 A trigger, not a date

Call it beta when all four hold, and not before:

- **(a) The reference application runs on a released toolchain**, with no
  in-tree patches and no `VILAN_STD` pointing at a checkout.
  `kolt-migration.md` is already the living tracker for this, so the
  condition is instrumented today.
- **(b) Two consecutive weekly trains ship with no urgent patch between
  them.** This is the sharpest of the four: it is not a promise that the
  project *can* go a week without an emergency, it is a demonstration that
  it *did*, twice. It is also the condition that cannot be gamed, since U1
  is defined by user-visible wrongness rather than by judgment.
- **(c) No known miscompile is open** in the backlog's B section. Beta with
  a known wrong-answer bug on the board is a promise made in bad faith.
- **(d) There is somebody to break the promise to** — at least one user
  outside the current three. This is D5, and §7 returns to it.

**Version at the switch:** a deliberate jump — **v0.40.0** — with the
changelog header rewritten in the same commit, exactly the way v0.2.0 marked
"first public". It worked once, it costs nothing, and it makes the boundary
legible in `vilan --version`. **Do not use 1.0 for beta.** 1.0 means the
deprecation window becomes semver's guarantee, and that is a much larger
promise than this section is recommending.

**Recommendation (§5).** Beta adds one-minor deprecation windows, a
mandatory `### Breaking` heading per minor, and wire changes always counted
as breaking. Beta explicitly does *not* freeze the spec: correction stays
free, removal becomes expensive. Switch on the four-condition trigger above,
not on a date, and mark it with a deliberate jump to v0.40.0.

---

## 6. What does not change

**The working day.** Lanes branch from `next`, land when they are done,
merge `--no-ff` with a merge commit naming the arc and its backlog ids.
Nothing in this paper delays a merge or inserts a reviewer. The cadence
change is a change to `git tag`, not to anything upstream of it.

**The cycle and lane machinery.** Orchestration, the worktree convention,
per-lane branches, the CHANGELOG union at merge time. §2 deliberately
declines to put a PR requirement on `next` precisely so this stays intact.

**The release pipeline itself.** `release.yml`'s five stages, the ten-asset
matrix, the five publish channels with their one-way semantics, the
`--remap-path-prefix` privacy discipline, `scripts/bump-version.sh` across
the three crate manifests and the extension manifest, `scripts/npm-package.sh`
stamping the six npm packages from the tag, `scripts/brew-formula.sh`
rendering the tap from the release's own `sha256sums.txt`. None of it cares
how often it runs. The only edit §8 asks for is to the `gate` job's command.

**The two site deploys.** Dispatch `docs.yml` on `vilan-lang.github.io` after
pushing `main`, then the website deploy. Unchanged in mechanism — but note
the consequence, because D5's session will want it: the website deploy
installs the toolchain **from the latest tagged release**, and the playground
wasm from that same release, so **the public site's freshness becomes the cut
cadence.** Under a weekly train, a visitor meets a playground up to a week
behind `main`. That is acceptable and it is a deliberate trade, but it should
be a known one.

### 6.1 The reconciliation sweep, at a deferred cut

`releases.md` §7's standing pre-tag sweep has three parts:

> (a) verify every `CHANGELOG.md` Unreleased entry's commit is an ancestor of
> the intended tag … (b) close the backlog markers for everything the release
> carries … (c) move each newly-shipped entry's full body verbatim into
> `proposal/backlog.md`.

The sweep was written when a cut followed a cycle, so it reads as "the cycle
just ended, close its markers". Under accumulated cycles the *procedure* is
still exactly right — it was never scoped to one cycle, it is scoped to
everything under `## Unreleased` — but three things change and the text must
say so.

**(a) becomes more valuable, not less, and must not be skipped.** With one
cycle per cut, an Unreleased entry whose commit never landed is a rare
accident. With five cycles' worth of entries, written by five lanes over a
week, the ancestor check is the only thing standing between the changelog and
a public claim about code that is sitting on an unmerged branch. Under
concurrent sessions this drifts in both directions, which is the finding that
created the sweep in the first place. Amend §7(a) to state that at a deferred
cut the check is per entry and is not optional.

**(b) and (c) should move to cycle end, not cut time.** This is the important
amendment. The sweep has a natural seam: **(b) and (c) are about the
*record*, (a) is about the *tag*.** Marker-closing and body-moving are done
best when the lane that shipped the item is still in living memory — at cycle
end — and they grow linearly with the number of cycles accumulated. Leaving
them at cut time turns a weekly cut into a two-hour records exercise, which
is the one way "cut less often" could make things worse. Ancestor
verification, by contrast, *cannot* move: it needs the commit that will
become the tag, which is only known at cut time.

So: **(b) and (c) run per cycle. (a) runs per cut, over the whole accumulated
`## Unreleased` section.**

**The `## Unreleased` section becomes a week long, and wants ordering.** At
2.7 cuts a day it holds one or two entries. At a weekly train it holds
fifteen. The changelog's existing convention already separates related
entries with `---` rules, so the amendment is small: §7's retitle step
becomes **"retitle and order"**, and the order is the one a reader wants —
breaking changes first, then miscompiles, then features, then diagnostics and
tooling.

**Recommendation (§6).** Nothing about the work, the lanes, or the pipeline
changes. Amend `releases.md` §7 to split the sweep along its seam — marker
closing and body moving per cycle, ancestor verification per cut over the
whole accumulated section — and to make the cut-time retitle an ordering
step. Record that the public site's freshness now tracks the cut cadence.

---

## 7. Open questions for the owner

Each with a recommendation; none of them blocks the others.

1. **Which weekday is the train?** *Recommend Saturday.* The cut is a manual
   sequence — sweep, bump, changelog, tag, fold, two dispatches — and putting
   it on a weekday makes it compete with a working session. Any fixed day
   beats no day; the property that matters is predictability, not which day.

2. **Does `release.yml`'s `gate` stay a second, weaker suite run?** *Recommend:
   make it run the same command CI runs, and keep it.* Keep it because a tag
   can be pushed from a commit that never went through a PR, and the gate is
   the last thing between that and five one-way publishing channels. Change
   the command because the two gates currently disagree and it has already
   mattered — see §8.3. This is the highest-priority item in this paper.

3. **Signed commits: in or out of the first ruleset?** *Recommend out.* A
   separate slice, after the automation that commits to this repository is
   known to be able to sign. Including it now is how the ruleset gets
   disabled again.

4. **Must a contributor disclose AI assistance in a PR?** `AI_STANCE.md`
   allows AI contributions with human oversight but does not say whether
   they must be declared. *Recommend: not required.* The bar is the code and
   the gates, which apply identically either way; a disclosure field invites
   either a lie or an argument. State in `CONTRIBUTING.md` that the same
   review bar applies regardless of tooling, and link `AI_STANCE.md`. (This
   is a values call as much as a process one, and it is the owner's.)

5. **Does `main` become PR-only for the owner too, or does the owner keep
   bypass?** *Recommend bypass `always`, initially.* The fold-to-main merge
   is mechanical and reviewing one's own PR is theatre. Revisit the moment a
   second maintainer exists — that is the event that makes the rule mean
   something.

6. **Beta's version number: next minor, or a deliberate jump?** *Recommend
   the jump, v0.40.0.* v0.2.0 already used this device to mark "first
   public"; it is free and it is legible in `vilan --version`.

7. **The dual license and GitHub's detector.** GitHub reports Apache-2.0
   only. Adding a `LICENSE` pointer file would make it report "Other", which
   is arguably worse. *Recommend leaving it:* the README states the dual
   license and the inbound clause correctly, and `CONTRIBUTING.md` will
   restate it. Flagged because it is the first thing a
   compliance-minded adopter checks and the owner may want the sidebar to
   read differently.

8. **Do patch releases get a `release/0.MINOR` branch?** *Recommend yes,
   created lazily and deleted after the next train.* Without it, "important
   patches in between" is not implementable — the only alternative is
   tagging `next`, which drags in a week of unreleased work and defeats the
   train.

### 7.1 D5 becomes more urgent under this policy, not less

Every recommendation in this paper has a payoff of exactly zero at three
users. Protected branches, PR-only merges, `CONTRIBUTING.md`, a security
channel, a deprecation promise — none of them do anything for an audience
that is already inside the repository. D5 (public traction, `STATUS: OPEN`,
blocked on a dedicated session with the owner) is the reason to build them
anyway. Three ways the two are coupled:

- **The scaffolding has lead time, and the lead time runs backwards.**
  `SECURITY.md` and private vulnerability reporting must exist *before* the
  first stranger reads the code, not after the first report arrives. The
  required-check configuration wants a few weeks of real PRs to shake out —
  the `paths-ignore` footgun in §2.5 is precisely the kind of thing found on
  the first prose-only PR, and it is much better if that PR is the owner's.
  Doing D5 first and the scaffolding after gets the order exactly wrong.

- **The beta trigger contains D5.** Condition §5.4(d) — somebody to break the
  promise to — cannot be satisfied without D5 having run. Graduating to beta
  is therefore *blocked on* D5, not merely adjacent to it. Under the current
  cadence that dependency is invisible; under this policy it is on the
  critical path.

- **The cadence change hands D5 a publishing rhythm.** At 2.7 cuts a day
  there is nothing to announce — no release is an event when the next one is
  four hours away. A weekly train with an ordered changelog *is* a content
  calendar, and D5's candidate skeleton (a landing page, a "why vilan"
  essay, two or three deep dives, show-don't-tell demos) has something
  regular to hang on. It also fixes what the site can promise: the playground
  a visitor meets is the last release, so under a weekly train the site is at
  most a week behind, predictably, rather than four hours behind, randomly.

**Recommendation (§7.1).** Schedule D5's session **before** §4's slice 1
ships, and certainly before the beta switch. Its answers — who the audience
is, what gets a face and what stays text, which venues — change what
`CONTRIBUTING.md` and `SECURITY.md` should say, and its outcome is a
precondition of calling the project beta at all.

---

## 8. What contradicts this policy today

Everything below is in the tree or in the repository configuration now, and
needs amending when this paper is ratified. Ordered by severity.

**8.1 — `release.yml`'s gate and `ci.yml` disagree, and a release has already
shipped on the disagreement.** The v0.32.0 tag and the CI run on `next` share
head sha `e0e9e02`. The release run's `gate` job passed and published to
GitHub Releases, npm, the VS Code Marketplace, Open VSX and the Homebrew
tap — while `ci.yml` on that identical commit went **red on both ubuntu and
windows**. The two failures were `cancel_aborts_an_in_flight_fetch` and
`benchmarks_run_and_report_the_deterministic_counts`: load-dependent budget
failures that `cargo test`'s serial per-binary scheduling does not reproduce
and nextest's full interleave does. (The E40 lane has since fixed them, and
`crates/vilan-cli/tests/benchmarks.rs` now carries the sentence "It failed at
exactly 90.0 s on both CI runners in v0.32.0.") The point is not the flakes.
The point is that **the project's release gate is a weaker and different
instrument than its CI gate, and nothing connects them.** Under a policy with
required checks this is not survivable. Amend `release.yml`'s gate to run
`cargo nextest run --workspace` plus the doc-test leg, i.e. the same
instrument `ci.yml` runs, and add `ci.yml`'s aggregate job as a required
check on `main`.

**8.2 — `ci.yml`'s `paths-ignore` is incompatible with required checks**, as
its own comment at lines 10–12 predicts. A PR touching only `README.md`,
`AI_STANCE.md`, `AGENTS.md`, `CLAUDE.md`, `CODE_OF_CONDUCT.md`, either
license, or `.gitignore` produces no run at all, and a required check that
never reports strands the PR at "Expected" forever. That file list is close
to a description of a first-time contributor's first PR. Amend per §2.5:
move the filter from `on:` into an always-running `changes` job, gate `test`
and `wasm` on it, and add an `if: always()` aggregate job as the required
check.

**8.3 — Ruleset 18887216 must be amended, not enabled.** As configured
(`bypass_actors: []`, `current_user_can_bypass: "never"`,
`required_signatures`) turning it on blocks the fold-to-main merge and every
automated commit, including the tap job's `github-actions[bot]` commit which
cannot sign. Amend per §2.3 first.

**8.4 — `releases.md` §7 does not describe the cut the project actually
performs.** It documents the five workflow stages and the sweep, and stops.
It does not mention pushing `main`, dispatching `docs.yml` on
`vilan-lang.github.io`, or the website deploy. The only place that sequencing
is written down anywhere is a comment in `docs.yml`'s own header ("the vilan
cut flow dispatches this workflow after pushing main; the daily schedule is
the safety net for anything missed"). Amend §7 to carry the full cut
sequence.

**8.5 — `releases.md` §7 step 1 is stale in both its count and its command.**
It describes the gate as "full `cargo test` on linux (the suite: 669 tests,
corpus, docs gate, walkthrough build, hygiene)". The suite on v0.32.0's
commit was **3,046 tests on ubuntu and 3,013 on windows**, and the project's
gate is `cargo nextest run --workspace` per `CLAUDE.md`, not `cargo test`.
Amend both, and fold in 8.1's change.

**8.6 — `releases.md` §4 says "Bump minor liberally."** That is correct alpha
advice and directly contradicts a weekly train, where a minor is a weekly
event rather than a per-landing one. Amend §4 with the cadence rule and, when
beta is called, with the three beta promises from §5.2.

**8.7 — `releases.md` has no notion of a release branch.** Every tag in the
history is on `next`'s line, so a patch off a released tag has nowhere to
live. §7 needs a `release/0.MINOR` paragraph before "important patches
released in between" is implementable.

**8.8 — the three free security settings are off** (`secret_scanning`,
`secret_scanning_push_protection`, `dependabot_security_updates`), and
`allow_rebase_merge` is `true` while §3 recommends it be `false`.
Repository settings, not files, but they are part of the same ratification.

**8.9 (minor) — `releases.md` §5 quotes the install one-liner with the
pre-migration owner string.** This is deliberate and explicitly allowlisted
in `crates/vilan-cli/tests/hygiene.rs` ("release history quotes the install
one-liner as it was published"), so it is not a bug. But `CONTRIBUTING.md`
will point newcomers at `releases.md`, and a reader who copies §5's URL as
current instructions follows a redirect. A parenthetical — *(as published
then; the project now lives at `vilan-lang/vilan`)* — costs one clause and
keeps the allowlist entry honest.
