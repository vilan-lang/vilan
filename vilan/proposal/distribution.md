# Distribution — npm, marketplace, brew (F7) + the project-model deferrals (F5)

> **AMENDMENT 2026-07-25 — call (a) re-decided by registry fact**: npm's
> typosquat protection **blocks bare `vilan`** (403 at publish: "too
> similar to existing packages vibas, livan" — a registry rule, not
> appealable in practice). The meta package is **`@vilan-lang/vilan`**
> (the call's recorded alternative, user-confirmed): install =
> `npm install -g @vilan-lang/vilan`; the **command stays `vilan`** (the
> `bin` field names it). Silver lining: the org owns the scope, so the
> placeholder-publish idea (org-migration.md §1.2) is obsolete — nothing
> inside `@vilan-lang` can be squatted.
>
> **Status: RATIFIED 2026-07-25 — calls (a)–(f) all per recommendation**
> (bare `vilan` + `@vilan-lang` scope; upgrade steers; Open VSX yes;
> publisher `vilan`-if-free + minimal wordmark; F5 = git dependencies v1;
> F5 rides this arc). **(g) — RESOLVED 2026-07-25: B33 goes first**
> (proposal/b33-emission-order.md, ratified same day); F7 S1 starts when
> B33's arc lands.** Two additions from ratification (user,
> 2026-07-25): **winget** is a recorded follow-up (§10 — user accepts
> deferral; it needs its own plan, and it is where the signing decision
> stops being deferrable) and **the repo-to-org question is filed as
> backlog F9** — a planning item that should be DECIDED before §7's
> account provisioning, since every channel bakes in an owner identity
> (npm scope, marketplace publisher, tap repo, and the URLs embedded in
> released binaries). Implementation of S1–S3's *code* is
> identity-independent (publish jobs are disabled-until-secret); only the
> account creation waits on F9.
>
> Original status: DRAFT 2026-07-24 — for review. The adoption slice, next in the
> agreed order after F8 (Windows support), which was its deliberate
> prerequisite: every channel below inherits a release that already ships
> five targets (linux musl x64/arm64, macOS x64/arm64, windows msvc),
> versionless assets, `sha256sums.txt`, both install scripts, and a `.vsix`
> — published green by tag-driven CI, with the suite gating on ubuntu and
> windows as required checks. B33 may be inserted between this proposal's
> ratification and its implementation (the user's standing note; decided at
> that fork, not here).

## 0. What exists

- **The release flow** (releases.md, live through v0.14.0): tag → gate
  (suite + changelog section) → 5-target build matrix with reproducibility
  checks → publish. Assets are versionless (`vilan-<target>.tar.gz` /
  `.zip`), so `releases/latest/download/` resolves directly — `install.sh`,
  `install.ps1`, and `vilan upgrade` all rely on this.
- **`vilan upgrade`**: redirect-based discovery, in-process SHA-256,
  platform-correct swap. It assumes it owns `~/.vilan/bin`.
- **The extension**: built + packaged in CI (`vilan-vscode.vsix` release
  asset), licenses shipped (v0.12.0), `publisher: "vilan"` is an
  **unregistered placeholder**, no icon. Installing it today means
  downloading the `.vsix` by hand.
- **The manifest** already models registry dependencies (`dep = "1.2"` =
  bare version, `{ version, registry, path }` table form) and errors
  "not yet supported" on anything without a `path` — F5's stub.
- **Names**: the npm package name `vilan` is unclaimed (checked
  2026-07-24; `vilan-lang` also free).

## 1. Goal and shape

**Installing vilan becomes one command in each ecosystem's native tool**,
without the curl script being the only door:

```
npm install -g vilan        # the JS/TS audience — vilan's actual audience
brew install vilan-lang/vilan/vilan
code --install-extension …  # or just: search "vilan" in VS Code
```

Publishing is **CI work on the existing tag flow** — a release tag fans out
to npm, the marketplace, and the brew tap the same way it already fans out
to GitHub Releases. One version everywhere stays true by construction (the
bump script already stamps every surface). The user provides the accounts
and secrets once (§7); no channel adds a runtime dependency or a passive
network call to the toolchain itself.

## 2. npm (the headline channel)

**Pattern: per-platform packages, not postinstall download** — the
esbuild/swc/turbo shape, which is the proven one:

- `@vilan-lang/vilan` (meta; *amended 2026-07-25 — bare `vilan` is blocked
  by npm's similarity rule, see the status block*): `bin` stubs for `vilan`
  and `vilan-lsp`;
  `optionalDependencies` on the five platform packages. The stub resolves
  the platform package and hands off (`spawnSync` with inherited stdio and
  the real argv — no shim logic beyond resolution).
- `@vilan-lang/linux-x64`, `/linux-arm64`, `/darwin-x64`, `/darwin-arm64`,
  `/win32-x64` — each carrying that target's two binaries, `os`/`cpu`
  fields set so npm installs exactly one.

Why not a postinstall downloader: it breaks under firewalls/proxies/offline
mirrors, defeats `npm ci` reproducibility, and every serious binary-shipping
CLI has migrated off it. The per-platform pattern costs six small packages,
generated by CI from the same artifacts the release already builds.

**Channel-aware upgrade**: an npm-installed vilan must not scribble over
files npm owns. `vilan upgrade` learns to detect it is running from inside
a `node_modules` tree and steers: "installed via npm — run
`npm update -g vilan`". Same courtesy for brew (a path under the Homebrew
prefix steers to `brew upgrade vilan`). The `~/.vilan/bin` install keeps
today's behavior. Detection is by path inspection at runtime — no build
variants, one binary everywhere.

**CI**: a `publish-npm` job on the release workflow, after the build
matrix: assemble the six packages from the built artifacts, `npm publish`
each with `NPM_TOKEN`. Idempotence: a re-run must skip already-published
versions cleanly (publish of an existing version errors — tolerate exactly
that).

## 3. VS Code marketplace (+ Open VSX)

- **Publisher registration** — *done 2026-07-25 under F9*: the publisher
  is **`vilan-lang`** (registered via Azure DevOps as part of the org
  claim sitting); `package.json.publisher` already updated in F9's sweep.
  What remains for S2 is the `VSCE_PAT` secret.
- **An icon is required in practice** (the marketplace renders a gray box
  otherwise) — a small deliverable this arc: simple wordmark/glyph, no
  branding ambitions, checked into `editors/vscode/`.
- **CI**: `vsce publish --packagePath` the already-built `.vsix` in the
  release workflow.
- **Open VSX** (recommended: yes): the same `.vsix`, one more publish step
  (`ovsx publish`, its own token) — serves VSCodium/Cursor/Theia users at
  near-zero marginal cost.

## 4. Homebrew

A tap, not homebrew-core (core has notability requirements and review
latency; a tap ships today and migrates later if ever wanted):

- New repo `vilan-lang/homebrew-vilan` with `Formula/vilan.rb` — per-target
  `url` + `sha256` (from `sha256sums.txt`), installing both binaries.
  macOS x64/arm64 + linux x64/arm64 all supported by `on_macos`/`on_linux`
  + `Hardware::CPU` branches over the existing tarballs.
- **CI**: a release-workflow job updates the formula (version + four
  hashes) and pushes to the tap with a scoped token (`TAP_TOKEN`).
- `install.sh` stays the README's first option; brew is listed beside it.

## 5. F5 — the project-model deferrals

The manifest's registry-dependency stub finally gets semantics. The design
question is **what a non-path dependency resolves against**, and the
recommendation is to *not* build or adopt a registry yet:

- **v1: git dependencies.** `dep = { git = "https://…", tag = "v1.2.0" }`
  (or `rev = "<sha>"`; exactly one required — no branches, no ranges, so no
  resolver and no lockfile are needed yet; a branch name is a clean error
  steering to tag/rev). Fetched shallowly into a content-addressed cache
  under `~/.vilan/` (beside the std cache, same pruning discipline); the
  checkout must contain a `[library]` manifest at its root; everything after
  resolution is the existing path-dependency machinery. Cargo's precedent:
  git deps carried the ecosystem for years before crates.io mattered.
- **Rejected for now: npm as the vilan registry.** Publishing `.vl` source
  packages to npm couples vilan library identity to npm accounts, pollutes
  a JS namespace with non-JS packages, and buys only discovery — which D5
  (the traction plan) is better placed to answer. A true registry is a
  D5-era decision, demand-gated; the `registry` manifest field stays parsed
  and "not yet supported".
- **Riders** (small, independent): `[project.dependencies]` inheritance
  (workspace members share declared deps — mechanical manifest merge, spec'd
  in the P2 deferral) and **server-side manifest completions** (the LSP
  completes keys/values in `vilan.toml` — the schema already exists for the
  editor's TOML validation; the LSP variant serves everyone else).

## 6. Slices

- **S1 — npm** (M): the six packages, the stub, CI publish, channel-aware
  upgrade steer + pins (stub resolution unit-tested; steer pinned by path
  fixture). The user's npm account + `NPM_TOKEN` gate the final step.
- **S2 — marketplace + Open VSX** (S–M): publisher id, icon, publish jobs.
  Gated on the publisher registration.
- **S3 — brew tap** (S): the tap repo, formula, CI update job.
- **S4 — F5 git dependencies** (M–L): proposal §5's v1 — manifest form,
  fetch + cache, clean errors (missing manifest, branch-not-tag, offline
  with cold cache), e2e with a real git fixture; pins per case.
- **S5 — F5 riders** (S each): inheritance; manifest completions.

S1–S3 are independent of each other and of S4/S5; account/secret
provisioning (user) can happen in parallel with implementation. Each
publish job lands **disabled-until-secret-exists** (skip with a clear
notice), so the workflow stays green before accounts exist.

## 7. What the user provides (once, all pseudonym-safe)

npm account (owns `vilan` + the `@vilan-lang` scope) + `NPM_TOKEN` secret;
marketplace publisher id + `VSCE_PAT`; Open VSX account + token (if (c)
says yes); the `homebrew-vilan` repo + `TAP_TOKEN`.

*S4 residuals (2026-07-25):* a crashed fetch's `.staging-…` leftover in
`~/.vilan/git-deps/` is never swept (the natural home — `vilan
upgrade`'s prune — is release machinery, deliberately not touched by
S4; a follow-up call). The git cache deliberately has NO age-pruning:
a std tree re-materializes free, a git entry needs the network, so an
mtime sweep would delete exactly what makes offline-with-warm-cache
true. `vilan test` compiles with an empty workspace (pre-existing), so
`*_test.vl` can import neither path nor git deps — more visible now,
recorded. The LSP has no `vilan.toml` diagnostic channel (manifest
failures are swallowed today); a git-dep cache miss therefore degrades
to unresolved-import diagnostics — the channel is S5-adjacent work.

*S2 additions (2026-07-25):* Open VSX does **not** auto-create
namespaces — before the first tagged release with `OVSX_TOKEN` set, run
`npx ovsx create-namespace vilan-lang -p <token>` once. And the day
`VSCE_PAT` lands, flip README's extension-install sentence from
"Install from VSIX" to "search Vilan in the marketplace" (recorded in
the S2 slice report; a documented install that doesn't exist yet is
worse than none, so the sentence waits for the channel).

## 8. Open calls

(a) **npm naming** — recommend: take bare `vilan` for the meta package
    (it is free today and is the name people will type) + the
    `@vilan-lang` scope for the platform packages. Alternative: everything
    under `@vilan-lang`.
(b) **Channel-aware upgrade** — recommend: steer (never overwrite
    npm/brew-owned files). Alternative: hard-refuse with no hint (hostile),
    or allow-with-warning (breaks the package manager's ledger).
(c) **Open VSX** — recommend: yes.
(d) **Marketplace publisher id** — RESOLVED by F9: `vilan-lang`,
    registered 2026-07-25. Icon: minimal wordmark unless you supply one.
(e) **F5 registry model** — recommend: git dependencies v1, registry
    demand-gated to D5. This is the one *language-adjacent* call here.
(f) **F5 in this arc or split** — recommend: S4/S5 ride this arc after
    S1–S3 (they share no code with the channels; splitting is also fine if
    you want distribution shipped faster).
(g) **B33 insertion** — per your standing note: before implementation
    starts, decide whether B33 (dependency-ordered global emission) goes
    first.

## 9. Non-goals

MSI/code-signing as standalone deliverables (signing is decided inside
§10's winget plan when that is taken up); homebrew-core; a hosted vilan
registry; version ranges and a resolver (no ecosystem to resolve yet);
publishing vilan *libraries* anywhere (F5 v1 consumes git, publishes
nothing).

## 10. Recorded follow-up: winget (deferred by user call, 2026-07-25)

Now that Windows is first-class, winget is the natural fourth channel —
deferred, not rejected, and it needs **its own plan** before execution
because it differs from the other three in kind:

- **Publishing is a PR, not a push**: winget manifests live in the shared
  `microsoft/winget-pkgs` repo and go through their validation pipeline —
  a release-automation shape unlike our token-gated jobs (tooling like
  `wingetcreate` automates the PR; still an external review loop per
  release).
- **This is where signing stops being deferrable in practice**: an
  unsigned portable-zip manifest is *permitted*, but Defender
  SmartScreen's reputation friction on unsigned binaries is the first-run
  experience — the signing decision (cert cost, identity under the
  pseudonym discipline, or waiting for reputation to accrue) is the real
  content of the winget plan, not the manifest syntax.
- Interacts with F9 (the org question): the manifest's publisher/moniker
  fields and the package identifier (now settled by F9: `VilanLang.Vilan`)
  are permanent-ish; sequencing winget after F9 is decided costs nothing
  and avoids a rename PR.

Take-up trigger: after this arc's channels are live, or when a Windows
user asks for it — whichever comes first.
