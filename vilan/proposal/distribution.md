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
matrix: assemble the six packages from the built artifacts and `npm publish`
each. Idempotence: a re-run must skip already-published versions cleanly
(publish of an existing version errors — tolerate exactly that).

**Auth is trusted publishing (OIDC), not a token** — see §7 for why and
when it moved. Three constraints the job encodes, each of which silently
breaks the publish if undone:

- `permissions: id-token: write`, and `contents: read` beside it, because a
  job-level `permissions:` block replaces the workflow-level one outright.
- **No `registry-url` on `actions/setup-node`.** It writes
  `//registry.npmjs.org/:_authToken=${NODE_AUTH_TOKEN}` into `.npmrc`, which
  with no token in the environment expands to an *empty* credential; npm
  reads the line's presence as "auth is configured", never starts the OIDC
  exchange, and fails unauthenticated (actions/setup-node#1551). The default
  registry is npmjs.org anyway.
- **npm >= 11.5.1**, installed explicitly rather than inherited from
  `node-version: 24` — which npm a Node release bundles is not ours to pin
  and moves under us.

Each of the six packages carries its own trusted publisher (org
`vilan-lang`, repo `vilan`, workflow `release.yml`, action `npm publish`);
all fields are case-sensitive on npm's side. Provenance attestations come
free with it — no `--provenance` flag — and validate because every
`package.json` under `npm/` already names this repository.

## 3. VS Code marketplace (+ Open VSX)

- **Publisher registration** — *done 2026-07-25 under F9*: the publisher
  is **`vilan-lang`** (registered via Azure DevOps as part of the org
  claim sitting); `package.json.publisher` already updated in F9's sweep.
  What remains for S2 is the publish identity — `AZURE_CLIENT_ID` /
  `AZURE_TENANT_ID`, not the `VSCE_PAT` this originally said. Azure DevOps
  retires global PATs (the "all accessible organizations" scope `vsce`
  requires) on **2026-12-01**, so a PAT would have bought about four months.
  The job authenticates by Entra workload identity federation instead:
  `vsce publish --azure-credential` behind `azure/login`, with no stored
  credential at all.
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
  hashes) and pushes to the tap. Auth is a **GitHub App**
  (`TAP_APP_ID` / `TAP_APP_PRIVATE_KEY`), matching the site deploy's, not a
  PAT: an app's private key does not expire, and this credential runs only at
  release time — a PAT's expiry would lapse unnoticed and surface mid-release
  months later, and the disabled-until-secret gate cannot catch it because an
  expired token is still a non-empty string. Each run mints an installation
  token scoped to `homebrew-vilan` alone.
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

npm account (owns `vilan` + the `@vilan-lang` scope) + a trusted publisher
on each of the six packages — no secret;
marketplace publisher id + an Entra federated identity
(`AZURE_CLIENT_ID` / `AZURE_TENANT_ID`); Open VSX account + token (if (c)
says yes); the `homebrew-vilan` repo + a GitHub App
(`TAP_APP_ID` / `TAP_APP_PRIVATE_KEY`).

*Provisioned 2026-07-29: the VS Code Marketplace.* An Entra app
registration with a federated credential bound to this repo's
`marketplace` **environment** — the subject is
`repo:vilan-lang/vilan:environment:marketplace`, which is why the job
declares `environment: marketplace` and why deleting that line breaks
auth rather than merely loosening it. Bound to the environment rather than
the tag because release runs on a tag push and a tag-bound credential
matches one literal tag, needing re-registration every release. Cost: none
— app registrations are free permanently and survive the Azure trial
lapsing, since they live in the tenant, not a subscription. (The `$3`
"Entra Workload ID" SKU on the pricing page is a different, premium
product and is not required for this.)

*Provisioned 2026-07-29: Open VSX.* The account and the `vilan-lang`
namespace exist. Two corrections to the earlier note: the namespace being
**unverified** does not block publishing (it only puts a warning icon on the
listing instead of the shield), and the real gate is not `create-namespace`
but the **Eclipse Foundation Open VSX Publisher Agreement** — a valid token
without it cannot publish. Namespace ownership, which is what earns the
shield, is claimed by a public issue on `EclipseFdn/open-vsx.org` and can
follow the first publish.

*Provisioned 2026-07-29: npm.* `NPM_TOKEN` was always a bridge — npm is
deprecating 2FA-bypass tokens (account changes early Aug 2026, direct
publishing ~Jan 2027) — and the destination, trusted publishing (OIDC),
cannot do a package's FIRST publish, because npm requires a package to
exist before a trusted publisher can be configured on it. So the token's
whole job was to create the six packages. It did, at v0.18.1, and the
bridge came down the same day: `publish-npm` authenticates by OIDC (§2 for
the three constraints that make it work). The mechanics are in §2 because
they are load-bearing and non-obvious; what belongs here is the shape —
this channel is meant to hold no stored credential, so there is nothing to
rotate and nothing to leak.

**Ordering, and the one way this bites:** the workflow change is inert
until a tag is pushed, but a trusted publisher is configured *per package
on npmjs.com*, and there are six. Until all six exist, a release
authenticates as nobody and `publish-npm` fails — loudly, which is the
intent, but the release would go out with npm a version behind. So: the six
configs land before the next tag, not after.

| step | state as of 2026-07-29 |
| --- | --- |
| `publish-npm` rewritten for OIDC | done |
| trusted publisher on each of the six packages | set by the user on all six — **unverified**, see below |
| a release publishes green by OIDC | pending — proof is the next tag |
| `NPM_TOKEN` revoked on npm + deleted from repo secrets | done — revoked, and gone from the repo |

"Unverified" is not hedging: npm exposes **no read path** for a package's
trusted-publisher config. `npm access` has subcommands for status, mfa, and
team grants and nothing for OIDC, and the registry document does not carry
it. So there is no way to confirm the six configs are right — including that
every case-sensitive field matched — short of a release using them. Plan the
next tag accordingly: if `publish-npm` fails, the GitHub Release has already
gone out (it is a `needs:` dependency), the failure is loud, and re-running
the job after a fix is safe because the already-published predicate tolerates
a partial publish.

*Also 2026-07-29:* **Publishing access → "Require two-factor authentication
and disallow tokens"** is set on four of the six packages; the other two
errored on save, cause unknown, to be retried. This is independent
of everything above — it constrains *traditional token* auth only, and
trusted publishers keep working because they present OIDC tokens. It does
change one thing though: on those four, `NPM_TOKEN` can no longer publish at
all. The token therefore stopped being a fallback the moment that setting
landed — a token that can rescue two of six packages cannot rescue a release,
since a partial publish is the exact failure being guarded against. So it came
out ahead of the proof rather than after it: revoked on npm, then deleted from
the repo's secrets. **There is deliberately no fallback credential.** If the
next release's `publish-npm` fails, the fix is to correct the trusted-publisher
config and re-run the job — not to reintroduce a token. Of the five secrets
left, none can publish to npm.

The next release's changelog gets a line for it, because one part *is*
user-visible: trusted publishing attaches provenance attestations, so the
six packages start carrying a "Built and signed on GitHub Actions" badge
linking back to the run that produced them.

Also checked against `npm/` and affecting nothing: npm's new install-time
defaults (scripts off, git and remote-URL deps blocked). No package here
declares a `scripts` field, and resolution is `optionalDependencies` +
`os`/`cpu`.

One asymmetry this creates, recorded so it is not "fixed" back: alone among
the four publish jobs, `publish-npm` has **no disabled-until-secret-exists
gate** (§6). That gate lets a job land before its channel is provisioned;
npm is provisioned, and trusted publishing leaves no secret whose absence
could stand in for "not live yet". A silent skip is now the dangerous
outcome — it is precisely how v0.18.0 shipped five of six packages and read
green.

*S5 residuals (2026-07-25):* **version skew** — an updated extension
registers `**/vilan.toml` with whatever `vilan-lsp` it discovers; an
OLDER server runs the manifest through the vilan pipeline and squiggles
it (not defensible client-side: the documentSelector is fixed before
`initialize`, and probing the old server hangs). Transient — upgrading
the toolchain clears it; marketplace installs get both halves new.
Also: with the manifest in the selector the server advertises
formatting for it (the zero-bail net returns it unchanged — harmless,
but a "multiple formatters" prompt may appear beside a TOML extension).
And the inherited-declaration gap: when the PROJECT's declaration is
what's broken, the diagnostic lands on the member manifest that opted
in, not the project root. All v1-accepted.

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
the marketplace identity lands, flip README's extension-install sentence from
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

### The VS Code Marketplace member step — RESOLVED 2026-07-29

**A service principal has no Marketplace identity until it first
authenticates.** That is the whole answer, and it is why four correct-looking
identifiers were all rejected with `TF14045: The identity could not be found`:
the identity genuinely did not exist yet. None of these work, and none of them
ever would have:

| Identifier | Value | Where it comes from |
|---|---|---|
| Application (client) ID | `55e41b2a-…` | App registration Overview |
| Service principal object ID | `c28b79e9-…` | Enterprise applications Overview |
| Azure DevOps identity ID | `f911b5fa-…` | `vssps.dev.azure.com/<org>/_apis/identities` |
| Graph descriptor | `aadsp.ZjkxMWI1…` | `_apis/graph/serviceprincipals` |

**What works:** the identity the Marketplace mints for the principal the first
time it authenticates — here `b1aef853-35e2-4e52-a45e-60a7b32ca830`, role
**Contributor**. It exists nowhere in the Azure portal, and no lookup produces
it beforehand. The way to obtain it is to let a publish **fail**: the run's
error names it outright.

```
Azure CLI login succeeds by using OIDC.
Publishing 'vilan-lang.vilan v0.18.0'...
Access Denied: b1aef853-35e2-4e52-a45e-60a7b32ca830 needs the following
  permission(s) on the resource /vilan-lang to perform this action:
  Publish new extensions to an existing publisher
```

So the correct provisioning order is counterintuitive and worth stating
plainly, because nothing in the portal hints at it:

1. Register the app, add the federated credential, materialize the service
   principal in the Azure DevOps organization.
2. **Run a release and let the marketplace job fail.** Authentication
   succeeds; authorization does not; the error names the identity.
3. Add that identity to the publisher's Members as **Contributor**.
4. The next release publishes.

**Corrections to the note this replaces.** It recorded a leading hypothesis
that the publisher was MSA-rooted while the service principal was AAD, and
that the fix would involve moving the publisher between identity domains.
That was wrong, and so was the two-identities reading of the role-assignments
error that produced it — there is one account. Chasing it cost several rounds
and briefly landed on the Create Publisher screen, which is a genuinely
dangerous place to be steered: the publisher was fine the whole time, and
creating a second would have been unrecoverable. Anyone debugging identity
here should trust `_apis/profile/profiles/me` over any inference drawn from a
descriptor string.

**Verified working end to end at v0.18.0:** the federated subject
`repo:vilan-lang@308981297/vilan@1299600279:environment:marketplace` matched,
`azure/login` succeeded with `allow-no-subscriptions: true`, and `vsce
--azure-credential` reached the Marketplace as the principal. Only the
membership was missing.
