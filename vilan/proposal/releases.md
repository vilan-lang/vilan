# Releases — installation and updates

## 1. Problem

vilan is public, but the only way to run it is to clone the repo and build
with cargo. The target audience is JS/TS developers who may not have a
Rust toolchain and shouldn't need one. They need: a one-command install, a
one-command update, prebuilt binaries per platform, and an editor
extension they can install without a marketplace account.

Underneath the packaging sits one real design problem: **an installed
binary cannot find std.** The compiler loads `vilan/std` from disk —
`$VILAN_STD`, else a path baked at compile time pointing into the repo
checkout. Both are meaningless on a user's machine. Every other decision
in this document is plumbing; this one is architecture.

## 2. Goals and non-goals

Goals:

- Install without Rust: download-and-run binaries for the major
  platforms, plus an install script in the style the audience knows
  (rustup/deno/bun).
- One-command update from the CLI itself.
- A self-contained toolchain: `vilan` works with no repo checkout, no
  side-by-side directories, no environment variables.
- Prebuilt `.vsix` for the VS Code extension in every release.
- Reproducible, automated releases: tag → CI builds, tests, packages,
  publishes. No hand-built artifacts.
- Privacy-clean artifacts: release binaries carry no build-machine paths
  (`--remap-path-prefix`), no phone-home behavior of any kind.

Non-goals (recorded, revisitable):

- Package managers (Homebrew, AUR, winget, apt) — after the direct
  channel proves out.
- crates.io publishing — the audience isn't cargo-first; low value now.
- Versioned documentation — the site tracks `main` (recorded in
  docs-site.md); releases link to it.
- Auto-update daemons or background update checks — updates happen when
  the user asks, full stop.

## 3. The std problem (the architectural piece)

**Decision to make: embed std in the binary.** The whole standard library
is 420K of `.vl` source (+28K macro_std) — embedded as compile-time data
it costs less than half a megabyte uncompressed, and it makes the binary
the complete toolchain. Compiler and std version together atomically,
which the wire-contract hashing and derive machinery already assume.

Resolution order becomes:

1. `$VILAN_STD` — explicit override, unchanged (power users, testing).
2. The ancestor walk — unchanged (developing IN this repo keeps loading
   std from the working tree, so std hacking needs no rebuild).
3. **The embedded copy** — replaces today's baked repo path as the
   final fallback. This is what every installed binary uses.

Implementation (refined after reading the loader): the std pipeline is
filesystem-shaped end to end — layer probing, `read_dir` module listing,
`macro_std` resolved as a sibling directory of `std`, manifests read
from disk. Teaching all of that a virtual filesystem would touch the
most battle-tested code in the compiler for no user-visible gain. So the
binary **embeds the two package trees and materializes them on first
use**: a `build.rs` in `vilan-core` embeds every `.vl` + `vilan.toml`
under `vilan/std` and `vilan/macro_std` (with a content hash), and the
fallback writes them once to `~/.vilan/std-cache/<hash>/` (atomic
tmp-dir + rename; temp-dir fallback if home is unavailable) and returns
that real path. The loader is untouched. LSP go-to-definition into std
keeps landing on real files. The content-hash key means a rebuilt dev
binary never sees a stale cache, and `vilan upgrade` swaps versions
without any sync step. The LSP replaces its baked-path fallback with the
same call (fixing the kolt-shape fragility for installed binaries).

**Pre-compiled std: measured, deferred.** Embedding *parsed* std (the
caching plan's deferred tier) was considered here. Measured on the
release binary: `check` on a hello is ~100ms end to end and a full
walkthrough build (client + server + macro worlds) is ~500ms — and the
in-process parse cache already amortizes std for watch mode, the LSP,
and multi-target builds, so pre-parsing could save only a few tens of
milliseconds on cold CLI runs. Against that: serializing an AST built
on borrowed `&'static str`, a build-script bootstrap on the compiler's
own parser, and a stale-artifact bug class. Not worth it at today's
numbers. If std parse cost ever shows up, the right shape is a
runtime-written warm cache beside the materialized std (generated on
first use by the binary's own parser, content-hash keyed) — no
build-time serialization, no staleness.

## 4. Versioning

One version for the whole toolchain — `vilan`, `vilan-lsp`, the
extension, and std move together, because they are coupled in fact
(embedded std, wire contracts, LSP protocol assumptions). Scheme:

- `0.MINOR.PATCH`, tags `v0.2.0`, `v0.2.1`, …
- Pre-1.0 semantics: minor bumps may break anything (the alpha promise);
  patch bumps are fixes.
- **Bump on the train, not on the landing.** This bullet read "bump minor
  liberally" until 2026-08-07, which was right when a cut followed every
  cycle and is wrong now. Ratified policy (`proposal/process.md` §1):
  cycles keep landing on `next` the day they finish, `## Unreleased` grows
  for a week, and the minor bump happens **at most weekly** — at the train,
  ratified as **Saturday** — or immediately when one of three urgent
  conditions fires:
  - **U1** — a miscompile in a shipped release: the compiler accepts a
    program, the program runs, and the answer is wrong, reachable from
    ordinary code.
  - **U2** — a security issue in the toolchain, in a published artifact, or
    in the install path (`install.sh` / `install.ps1` / `vilan upgrade` /
    the tap formula).
  - **U3** — a broken toolchain: the released binary fails to install,
    fails to start, or cannot compile a hello on a supported platform.

  Everything else waits. A fix being *good* is not a trigger; a fix being
  good is why there is a train. Two properties make it a train rather than
  a schedule: **it may be skipped** — a week that produced nothing a user
  can observe gets no tag, because a release with nothing in it spends the
  pipeline and five one-way publishing channels to teach users that
  upgrading is meaningless — and **it is never delayed for a feature**, so
  work that is not on `next` on cut day rides the next train. That second
  property is what makes "cut less often" cost nothing: nobody is ever
  waiting on the cut, because the cut is never waiting on anyone.

  A patch between trains carries **exactly one thing** — the fix for the
  U1/U2/U3 condition plus its changelog entry, nothing else — and is cut
  from a release branch, not from `next`. See §7.3.
- The first public release is **v0.2.0** (0.1.0 was the pre-public
  internal number; a visible jump marks the boundary).
- `vilan --version` prints `vilan 0.2.0 (<short-sha>)` so bug reports
  are precise.
- `CHANGELOG.md` at the root, hand-written per release in the docs'
  voice: what changed, what breaks, how to migrate. The release workflow
  refuses to tag a version with no changelog section (a grep gate, same
  spirit as the docs gate).

## 5. Installation channels

**Phase 1 (this proposal):**

- **GitHub Releases** — the canonical artifact store. Per release:
  - `vilan-<target>.tar.gz`, each containing `vilan` + `vilan-lsp` and
    the two license files. Asset names are deliberately **versionless**:
    `releases/latest/download/<asset>` then resolves directly, so the
    install script (and later `vilan upgrade`) needs no API round-trip
    to discover the newest version's file names. The version lives in
    the tag, the release title, and `vilan --version`; older artifacts
    stay addressable through their tags' own download URLs.
  - `vilan-vscode.vsix` — the extension, prebuilt (versionless for the
    same reason; the manifest inside carries the version).
  - `sha256sums.txt`.
- **The install script** —
  `curl -fsSL https://github.com/ReedSyllas/vilan/releases/latest/download/install.sh | sh`
  (the url as published then; the project now lives at `vilan-lang/vilan`,
  and the current one-liner is in the README — this one still resolves,
  through the transfer's permanent redirect):
  detects OS/arch, downloads the right tarball, unpacks `vilan` and
  `vilan-lsp` into `~/.vilan/bin`, prints the PATH line to add. The
  script itself is a release asset (and lives in the repo under
  `scripts/`), so it needs no separate hosting. Idempotent: re-running
  it updates in place.
- **From source** stays first-class for Rust users:
  `cargo install --path crates/vilan-cli` (already in the README).

**Targets:** `x86_64-unknown-linux-musl` (static — one binary for every
distro and WSL), `aarch64-unknown-linux-musl`, `x86_64-apple-darwin`,
`aarch64-apple-darwin`. Windows: decision below — native
`x86_64-pc-windows-msvc` marked experimental, or WSL-only at first (the
runtime story is node-based either way).

**Phase 2 (recorded):** npm distribution (`npm i -g vilan` /
`npx vilan`) via the esbuild pattern — platform binaries as
`optionalDependencies`. For a JS/TS audience this is likely the single
highest-adoption channel; it earns its own slice once the direct channel
is proven. Homebrew tap alongside it.

## 6. Updates

- **`vilan upgrade`** — a new CLI subcommand:
  1. queries the GitHub Releases API for the latest tag,
  2. compares to its own version; prints "already newest" or the
     changelog url,
  3. downloads the platform asset, verifies the sha256,
  4. swaps itself atomically (write to temp, rename over — with the
     rename-the-running-exe dance on Windows), updating `vilan-lsp`
     beside it.
  - `vilan upgrade --check` does steps 1–2 only.
  - *Implemented*, dance included, in windows-support.md's S6: the running
    `vilan.exe` is renamed aside to `vilan.exe.old` and swept at the start
    of the next upgrade run; the sha256 moved in-process (`sha2`) on every
    platform, so step 3 no longer needs `sha256sum`/`shasum`.
- **No passive checks.** The CLI never touches the network unless the
  user runs `upgrade`. This is a privacy stance, stated in the docs.
- **The extension**: point it at `~/.vilan/bin/vilan-lsp` in its binary
  search order (it already searches release/debug/cargo locations), so
  `vilan upgrade` updates the LSP the editor uses with no extra step.
  Extension updates themselves are a new `.vsix` per release until a
  marketplace listing lands (recorded for Phase 2 — publishing needs a
  publisher account decision).

## 7. The release pipeline

`.github/workflows/release.yml`, triggered by pushing a `v*` tag:

1. **Gate**: the suite on linux, run with **exactly the command `ci.yml`
   runs** — `cargo nextest run --workspace`, plus the
   `cargo test --workspace --doc` leg that nextest does not cover. The
   corpus, the docs gate, the walkthrough build and hygiene are all inside
   it; the count is not worth writing down here (it was 669 when this
   paragraph was first written and 3,046 on ubuntu at v0.32.0), but the
   *command* is, because it was wrong for a month.

   Until 2026-08-07 this step ran plain `cargo test`, and the two gates
   were therefore different instruments. `cargo test` schedules serially
   per binary; nextest interleaves every binary across all cores, which
   surfaces load-dependent failures the serial schedule hides. On the
   v0.32.0 commit (`e0e9e02`) that difference published a release: the
   `gate` job went green and shipped to GitHub Releases, npm, the VS Code
   Marketplace, Open VSX and the Homebrew tap, while `ci.yml` on the
   identical sha was **red on both ubuntu and windows**. A release gate
   weaker than the CI gate is not a gate. Keep the two commands identical,
   character for character, and change them together
   (`proposal/process.md` §7.2 and §8.1 — the highest-priority item in
   that paper).
2. **Changelog check**: `CHANGELOG.md` contains a section for this
   version.
3. **Build matrix**: the targets above, `--release` with
   `RUSTFLAGS=--remap-path-prefix` mapping `$HOME` and the workspace to
   neutral names — release binaries carry no build paths (the
   going-public discipline, mechanized).
4. **Package**: tarballs + vsix (`vsce package` in `editors/vscode`,
   pinned via `npx --yes @vscode/vsce`) + `sha256sums.txt` + the install
   script.
5. **Publish**: `gh release create v<version>` with the changelog
   section as the release notes, all assets attached.

### 7.1 The reconciliation sweep, and where each part runs

**The standing pre-tag step.** Added 2026-08-03, after a
records-reconciliation pass found the record drifting in both directions
under concurrent sessions — a shipped item with no backlog marker, and
(separately) a marker dated days before the code it names actually merged.
Before cutting, not after CI has already tagged something wrong:

(a) verify every `CHANGELOG.md` Unreleased entry's commit is an ancestor of
    the intended tag (`git merge-base --is-ancestor <commit> HEAD`, checked
    against the commit that will become the tag) before retitling it into
    the dated section — under concurrent sessions entries drift both ways,
    a commit filed under Unreleased that never landed on this branch, or a
    commit that shipped with nothing filed for it;
(b) close the backlog markers (`proposal/backlog-2026-07-18.md`) for
    everything the release carries — an unmarked shipped item is exactly
    how "entries routinely stay open after their arc ships" keeps
    recurring;
(c) move each newly-shipped entry's full body verbatim into
    `proposal/backlog.md`, per that file's restructure convention (the
    distilled file keeps a one-line tombstone, nothing is deleted).

**The sweep splits along its seam** (ratified 2026-08-07,
`proposal/process.md` §6.1; in practice since the v0.33.0 close). The sweep
was written when a cut followed a cycle, so it reads as "the cycle just
ended, close its markers". Under accumulated cycles the procedure is still
exactly right — it was never scoped to one cycle, it is scoped to
everything under `## Unreleased` — but the three parts no longer run at the
same moment. **(b) and (c) are about the *record*; (a) is about the *tag*.**

- **(b) and (c) run per cycle**, at cycle close. Marker-closing and
  body-moving are done best while the lane that shipped the item is still
  in living memory, and they grow linearly with the number of cycles
  accumulated. Left at cut time they turn a weekly cut into a two-hour
  records exercise, which is the one way "cut less often" could make things
  worse.
- **(a) runs per cut**, over the whole accumulated `## Unreleased` section,
  and it *cannot* move: it needs the commit that will become the tag, which
  is only known at cut time. It also becomes **more** valuable under
  accumulation, not less. With one cycle per cut, an Unreleased entry whose
  commit never landed is a rare accident; with a week of entries written by
  five lanes, the ancestor check is the only thing standing between the
  changelog and a public claim about code sitting on an unmerged branch.
  **The check is per entry and it is not optional.**

**How (a) finds an entry's commit** (2026-08-18, backlog L2). Entries carry no
sha and should not be asked to: nobody knows a commit's sha while writing the
entry that will be *in* it. So `scripts/cut-release.sh` derives one — the
oldest commit in the repository that introduced the entry's bold head into
`CHANGELOG.md`, `git log --all -S'**<head>**' -- CHANGELOG.md` read from the
bottom — and then asks §7.1's own question of it,
`git merge-base --is-ancestor <commit> <the commit that will become the tag>`.
Be exact about what that proves. It proves the entry is **committed** and that
the commit carrying it is **on the line being tagged**: an entry someone typed
and never committed, and an entry that reached `## Unreleased` only on a branch
that never merged, are both red, and the second is precisely the drift this
sweep was created for. It does **not** prove the entry's *code* landed, because
a changelog entry is often written in its own `changelog:` or `records:` commit
rather than beside the code it describes — on the v0.34.0 section, twenty of
forty-three. So the checklist prints the introducing commit's subject beside
every entry, and says so out loud when that commit touched nothing but
`CHANGELOG.md` and `proposal/`: *"note: `<sha>` touched records only — confirm
its code landed."* Reading twenty subjects is the part that stays human;
finding them is not. Where the derivation is wrong — an entry reworded after it
landed, an entry moved between sections — a `<!-- commit: <sha> -->` line above
the entry names its commit outright and wins.

A release that ships with a wrong changelog section or a stale backlog is a
smaller problem than one that has already tagged, so the sweep runs before
step 2's changelog check, not after.

### 7.2 Cutting a release: the whole sequence

Steps 1–5 above are what CI does once a `v*` tag exists. This is what the
person cutting does, start to finish. It is written down here because until
2026-08-07 the second half of it lived nowhere but a comment in the pages
repo's `docs.yml` header.

**Steps 1–3 are executed by `scripts/cut-release.sh <X.Y.Z>` and steps 6–10 by
`scripts/fold-release.sh v<X.Y.Z>`** (2026-08-18, backlog L2; first executed
end to end at v0.35.0 on 2026-08-21 — the cut refused nothing and flagged one
records-only entry for confirmation, the fold ran all ten steps clean with the
ruleset's bypass notice on the `main` push, as expected). **This prose
stays the authority; the scripts execute it.** A disagreement between the two
is a bug in the script, and a change to the sequence is made here first. Both
carry a `--dry-run` that performs only the read-only checks and prints every
command it would otherwise run, and neither ever tags or pushes a tag: steps 4
and 5 are the human's, and `cut-release.sh` finishes by printing them verbatim.

1. **Sweep (a).** Ancestor-verify every `## Unreleased` entry against the
   commit that will become the tag (§7.1). (b) and (c) are already done, per
   cycle.
2. **Bump.** `scripts/bump-version.sh <version>` — it rewrites every
   `crates/*/Cargo.toml`, refreshes `Cargo.lock`'s workspace-member
   entries, and runs `npm version` in `editors/vscode` so the extension's
   `package.json` and `package-lock.json` stay in step. One version for the
   whole toolchain, per §4.
3. **Retitle and order.** `## Unreleased` becomes `## v<version> — <date>`.
   At 2.7 cuts a day that section held one or two entries; at a weekly
   train it holds fifteen, so the retitle is also an **ordering** step. The
   order is the one a reader wants: breaking changes first, then
   miscompiles, then features, then diagnostics and tooling — separated by
   the `---` rules the changelog already uses between related entries.

   **The family is written down, not inferred.** Each entry under
   `## Unreleased` carries a `<!-- family: ... -->` line above its bold head —
   invisible in rendered markdown and in the release notes `release.yml`
   extracts — and it is one of four words:

   - `breaking` — a program that compiles today may stop, or change behaviour.
   - `miscompile` — the compiler was wrong about a program it *accepted*:
     wrong code emitted, or a program admitted that it must refuse.
   - `feature` — a new capability.
   - `tooling` — everything else the toolchain does better: diagnostics (a
     wrong *refusal* now lifted included), the editor, the CLI, packaging,
     this pipeline. `diagnostics` is an accepted spelling of it.

   The judgement is the entry author's and it is not derivable from the
   entry's shape: v0.34.0's own 43 entries put four editor and tooling
   improvements *inside* the features block, because that is where a reader
   wanted them. So `cut-release.sh` **refuses** an entry with no family, or
   one it does not know, and prints it — it never guesses. Within a family
   the authored order is preserved exactly, so a thematic grouping a human
   wrote survives the sort; only the four blocks are the script's doing.
   Rules are normalized on the way out: exactly one `---` between
   neighbours, none leading or trailing. (At the v0.40.0 switch `### Breaking`
   becomes a structural heading — beta.md §2 — and this marker is what will
   generate it.)

   **A marker that opens no entry is refused the same way** (2026-08-22,
   backlog L11). A marker sits *directly* above its bold head — the
   changelog's own writing note says so, and every marker in the tree does
   so — and a `family:` or `commit:` line that reaches a blank line, a `---`
   rule, a second marker of its kind, prose, or the section's end before a
   head is printed with its line (``marker `<!-- family: tooling -->` at line
   215 opens no entry``) and the cut stops. That shape is what a CHANGELOG
   merge-union leaves behind (found 2026-08-20: a marker, two blank lines, a
   rule — the parser let the rule clear it and the dry-run stayed green), and
   a dangling marker would ride into the release section as a comment nobody
   wrote. The manual cross-check after any CHANGELOG union is the anchored
   parity count, markers against heads under `## Unreleased`, which must be
   equal:
   `awk '/^## Unreleased/{p=1;next} /^## /{p=0} p&&/^<!-- family:/{m++} p&&/^\*\*/{h++} END{print m+0, h+0}' CHANGELOG.md`
4. **Commit, tag, push.** A `release: v<version>` commit on `next`, tagged
   `v<version>`; push `next` and the tag.
5. **Watch `release.yml`.** The tag push is the trigger. Ten assets, five
   publish channels, several of them one-way — an npm version can be
   deprecated but never replaced. Do not walk away from a red publish leg.
6. **Fold `main`.** Merge the tag into `main` with a merge commit
   (`Merge v<version> — main catches the release train`) and push. This is
   not cosmetic: the book builds from `vilan@main`, so nothing published
   after this point is current until the fold lands.

   It is a real three-way merge, not a fast-forward wearing `--no-ff`, and a
   check written on the assumption that `main` is an ancestor of the tag will
   refuse every fold there has ever been. `main` carries its own line: each
   previous fold commit, plus whatever was pushed straight to it (at v0.34.0
   that was seven commits the tag did not have, four of them the `AI_STANCE`
   pushes). What must hold is that the merge is **clean**, and
   `git merge-tree --write-tree main v<version>` answers that without touching
   a worktree — on `a86a7f16` and `v0.34.0` it writes the tree the real fold
   commit `0967ad52` has.
7. **Dispatch the book — FIRST.**
   `gh workflow run docs.yml -R vilan-lang/vilan-lang.github.io`, and wait
   for it to go green. It checks out `vilan@main`, rebuilds the book with
   mdBook, and commits the result into the pages repo. (A daily cron is the
   safety net for a missed dispatch, not a substitute for it.)
8. **Dispatch the site — SECOND.**
   `gh workflow run deploy.yml -R vilan-lang/website`. **The order is
   load-bearing in both directions.** It must come *after* the release
   because the site installs the toolchain from `releases/latest` and pulls
   the playground wasm from that same release — the cut is the site's
   version lever. It must come *after* the docs build because both
   workflows push commits to `vilan-lang.github.io` and they sit in
   different concurrency groups (`docs-build` and `site-deploy`), so
   nothing serializes them for you; run them together and two pushes race
   the same repo.
9. **Verify the manifest.** Fetch `/playground/manifest.json` from the live
   site: `compiler` must read the new tag and `versions[]` must list it
   first. This is the end-to-end proof that the new wasm actually reached
   Pages — the site deploy's own green is necessary and not sufficient.
   Trust `gh run list` on the pages repo over the `pages/builds/latest`
   endpoint, which lags the Actions-based Pages deployment.
10. **Refresh the local toolchain — both paths.** A development machine
    typically has *two* `vilan` binaries: one from `cargo install`
    (`~/.cargo/bin`) and one from the install script or
    `scripts/install-dev.sh` (`~/.vilan/bin`). Which one wins depends on
    the shell — interactive and non-interactive shells can resolve `PATH`
    differently, and a build script or an editor-spawned process is not the
    terminal. Refresh **both** locations, `vilan` and `vilan-lsp` together,
    and restart the language server; otherwise a stale compiler quietly
    shadows the release that was just cut, and the next thing verified is
    verified against the wrong binary. `vilan --version` prints the commit
    sha, which is the reliable check.

Everything from step 5 onward is the part that used to be tribal knowledge.

**Two notes on the fold, from mechanizing it.** The release run's verdict comes
from `gh run list` — a *completed* run whose conclusion is `success` and whose
head sha is the tag's — never from a watcher's exit code, because `gh run
watch` exits 0 for "I finished watching" and a red publish leg looks exactly
like a green one through it. And every step is written so that "already done"
is a state it recognizes rather than a failure: a fold interrupted at step 9 is
resumed by re-running it. That is what makes the sequence usable under the
thing it exists for — a train that must be boringly repeatable, not a ritual
performed correctly from memory once a week.

**A consequence worth stating plainly: the public site's freshness is now
the cut cadence.** The website deploy installs the toolchain from the latest
tagged release and takes the playground wasm from that same release, so
under a weekly train a visitor meets a playground up to a week behind
`main`. That is a deliberate trade — predictably a week behind beats
randomly four hours behind — but it is a known one, and it is the reason
step 8 cannot simply be skipped between trains.

### 7.3 Patches between trains: `release/0.MINOR`

Every tag in this repository's history sits on `next`'s own line —
`release: v0.32.0` is a commit on `next`, tagged there, and `main` is a
merge of it. That topology has nowhere to put a patch: cherry-picking a fix
onto `next` and tagging drags in a week of unreleased work, which is exactly
what the train exists to avoid. Ratified 2026-08-07
(`proposal/process.md` §1.5, open question 8), the minimal addition:

1. **At each train, do nothing extra.** No branch is created
   speculatively.
2. **When a patch is needed, branch `release/0.MINOR` from the tag** —
   lazily, only now, only because a patch exists.
3. **Cherry-pick the fix onto it.** The fix should land on `next` first and
   be cherry-picked *from* there. Bump to `0.MINOR.PATCH` with
   `scripts/bump-version.sh`, write the changelog section, tag
   `v0.MINOR.PATCH` **on that branch**, and push the tag. `release.yml`
   triggers on `v*` regardless of branch, so the pipeline needs no change
   at all — steps 5 through 10 of §7.2 run exactly as they do for a train.
4. **Merge `release/0.MINOR` back into `next` with `--no-ff`** — never
   rebase it — so the fix and its changelog entry cannot be lost at the
   next train. Where `next` already carries the fix, the merge is trivial
   and the changelog entry unions the way lane merges already do.
5. **Delete `release/0.MINOR` once the next train ships.** It is a
   scaffold, not a maintained branch. This project does not backport and
   should not pretend it might.

A patch carries **exactly one thing**: the U1/U2/U3 fix and its changelog
entry. That exclusivity is the whole promise of a patch release — "this
changes one behavior, the broken one" — and it is what makes a patch safe to
take without reading.

## 8. Delivery

- **Slice 1 — the self-contained binary**: embed std (+macro_std),
  rewire the fallback order, `vilan --version` with sha. Pins: an
  installed-binary smoke test (build, copy the binary to a temp dir
  outside the repo, compile a hello with no `VILAN_STD` and no checkout).
- **Slice 2 — the pipeline**: release workflow, packaging, install
  script, CHANGELOG, version-bump script, v0.2.0 tagged as the
  first public release.
- **Slice 3 — `vilan upgrade`**: the subcommand + extension search-path
  addition. Ships in v0.3.0 (users of v0.2.0 update by re-running the
  install script once; from then on, `vilan upgrade`).

## 9. Decisions (settled with the user, 2026-07-13)

1. **Windows**: WSL-only at first, documented. Native binaries wait for
   someone who can verify them.
2. **Install prefix**: `~/.vilan/bin` — own directory, clean uninstall,
   `vilan upgrade` owns it.
3. **First public version**: v0.2.0.
4. **npm channel**: Phase 2, its own slice soon after the direct channel
   proves out.
