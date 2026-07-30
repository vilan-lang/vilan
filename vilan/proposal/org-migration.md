# Org migration — `ReedSyllas/vilan` → `vilan-lang/vilan` (F9)

> **Status: MIGRATION COMPLETE — all slices done** (markers added 2026-07-29;
> until then only commit prose recorded this, which left the file reading as
> an unexecuted plan):
>
> - **S1 — DONE.** The org exists and `vilan-lang/vilan` is live; every repo
>   URL in the tree points at it.
> - **S2 — DONE, verified end to end 2026-07-29.** The old-host Pages
>   tombstone answers: `reedsyllas.github.io/vilan/<page>` serves a `noindex`
>   forwarder that JS-redirects fragment-intact to `vilan-lang.github.io`,
>   which 301s to the custom domain, where a second forwarder maps
>   `/vilan/*` -> `/docs/*` and lands on a **200**. Both hops return HTTP 404
>   by Pages design, so status alone is not the test — the body is. This is
>   what keeps `≤ v0.14.0` binaries' editor-hover book links working, and it
>   must keep answering indefinitely.
> - **S3 — DONE.** `vilan-lang.org` is claimed and serves the site + book.
> - **S4 — DONE** (`5bb74b9`): the owner-string sweep plus the never-again
>   gate, `tests/hygiene.rs::no_tracked_file_contains_a_pre_migration_owner_string`
>   (runtime-assembled needles, case-insensitive, 3-file allowlist with inline
>   reasons; the commit records a planted-probe non-vacuity proof).
>
> **Open tail — DECIDED 2026-07-29.** The gate's needles are
> `reedsyllas/vilan` and `reedsyllas.github.io`, so two personal-identity
> strings sat outside its reach. They were never one problem, and they were
> settled differently:
>
> - `CODE_OF_CONDUCT.md`'s personal gmail → **`conduct@vilan-lang.org`**. A
>   contact address in a public repo gets scraped, cannot be rotated without a
>   commit, and does not survive a second maintainer; a role address on the
>   project's own domain fixes all three. Pinned by a SECOND hygiene test,
>   `no_tracked_file_publishes_a_personal_mailbox`, kept separate from the
>   owner-string gate because the reasoning is unrelated — that one protects a
>   mechanical invariant (reusing the old repo name kills the
>   `releases/download/…` redirects that keep installed binaries' `vilan
>   upgrade` alive), this one is about the project speaking in an
>   organizational voice. Matched by consumer-mail DOMAIN rather than by the
>   one address that prompted it, since the failure mode here is a new file
>   rather than a regression in an old one. One allowlist entry:
>   `THIRD-PARTY-NOTICES.txt`, whose addresses are upstream authors' and not
>   ours to rewrite. **Delivery confirmed by the user 2026-07-29** — the
>   alias forwards. Note what that test had to be: alias mappings live in the
>   registrar's dashboard, not in DNS, so nothing about them is verifiable from
>   outside. DNS only ever proved the *infrastructure* (MX to Namecheap's
>   forwarders, matching SPF); the alias itself needed a real message.
>
>   **Open, and it would quietly undo this:** the forwarding is RECEIVE-ONLY.
>   There is no SMTP behind `conduct@vilan-lang.org`, so a reply to a conduct
>   report leaves from a personal mailbox — putting the removed address back,
>   in a worse setting than the one it came out of, since the recipient is a
>   stranger mid-report. Costs nothing at zero users; fix before the project
>   has a community. Either a real mailbox (Namecheap Private Email, ~$1/mo)
>   or Gmail "send mail as" over some SMTP relay.
> - `AI_STANCE.md`'s byline → **kept, by user call.** "Reed Syllas" is the
>   pseudonym the work is published under, and the line is an attestation that
>   a human wrote the document without AI tooling — a claim whose whole force
>   comes from a named person standing behind it. Stripping the name would
>   leave a document vouching for itself. The bare profile URL is a separate,
>   still-open question the user may yet remove; the name is not.
>
> Worth recording because it is the reason this was never urgent: removing
> these strings would not have bought anonymity anyway. The tombstone below
> serves `reedsyllas.github.io/vilan/*` — the account name is IN the url, and
> that page must keep answering indefinitely — so the association is
> structural and permanent by design.
>
> Prior status: RATIFIED 2026-07-25 — all §6 calls per recommendation (npm
> placeholder publish of bare `vilan`: yes; claim `vilan-lang.org`: yes;
> kolt stays personal; CHANGELOG history swept). Execution order stands as
> §5; S1–S3 are user actions, S2's content and S4 are prepared/implemented
> by the coordinator.
>
> Original status: DRAFT 2026-07-25 — for review. The decision is made (user,
> 2026-07-25: an org, named **`vilan-lang`**); this is the execution plan.
> Availability verified 2026-07-25: the GitHub org and the VS Code
> Marketplace publisher `vilan-lang` are both unclaimed; the npm scope
> cannot be checked unauthenticated and is confirmed at claim time (§1 —
> its contingency is recorded there). Sequenced **before** F7's account
> provisioning (`distribution.md` §7), which was the reason F9 exists:
> every channel bakes in the owner identity, and creating the accounts
> under the org from day one avoids every migration.

## 0. The one invariant, stated first

**The GitHub name `ReedSyllas/vilan` must never be reused.** A repository
transfer leaves permanent redirects for git operations *and*
`releases/download/…` URLs — which is what keeps every already-installed
binary's `vilan upgrade` working (their baked `DEFAULT_BASE` points at the
old name forever). Creating any new repo named `ReedSyllas/vilan` — even a
well-meant "redirect notice" repo — **kills those redirects instantly**.
The Pages tombstone (§3) is designed around this: it must NOT be a repo of
that name. This invariant outlives the migration; record it wherever
future-you looks first.

## 1. Claim phase (user actions, one sitting — squat-proofing)

In one sitting, so nothing sits half-claimed:

1. **GitHub org `vilan-lang`** — membership visibility stays **private**
   (the default; the org page then shows repositories, not people —
   pseudonym discipline intact, see `going-public`).
2. **npm org `vilan-lang`** — creating it claims the `@vilan-lang` scope
   and settles the one unverifiable availability fact. *Contingency if
   taken (unlikely — the GitHub name is free): stop and re-decide naming
   as a pair; do not improvise a mismatched scope at the keyboard.*
   ~~Also claim the bare `vilan` package name via placeholder publish.~~
   **OBSOLETE (2026-07-25, executed and refuted)**: npm's typosquat rule
   403-blocks bare `vilan` ("too similar to vibas, livan") — nobody can
   claim it, us included, so there is nothing to squat-proof. The meta
   package is **`@vilan-lang/vilan`** (distribution.md status amendment);
   the scope the org owns is the whole protection.
3. **VS Code Marketplace publisher `vilan-lang`** (via Azure DevOps) —
   registration only; publishing is F7 S2.
4. **Open VSX namespace `vilan-lang`** — per F7's ratified (c).
5. **Optional, user's call**: register `vilan-lang.org` (cheap
   squat-proofing; rust-lang.org precedent). Using it for anything is a
   D5-era decision — claiming it is not.

Not claimable in advance: the winget identifier (`VilanLang.Vilan` —
established at first PR, no reservation exists; recorded in
`distribution.md` §10) and the brew tap (just a repo the org creates in
F7 S3 — owning the org IS the claim).

## 2. What transfers automatically, and what does not

A GitHub repo transfer moves: git data, issues/PRs, releases **with
assets** (the redirect covers their download URLs), stars/watchers,
webhooks, and — per current GitHub behavior — Actions workflows. To
**verify immediately after transfer** rather than assume: Actions remain
enabled, Pages configuration survived (§3), and repo settings (default
branch `next`) held. **Deliberately nothing to migrate**: the repo has no
custom Actions secrets today (`GITHUB_TOKEN` is automatic; F7's
`NPM_TOKEN`/`VSCE_PAT`/`TAP_TOKEN` don't exist yet) — doing F9 before F7
is exactly what makes the transfer this cheap. Local remotes re-point
afterward (`git remote set-url origin git@github.com:vilan-lang/vilan.git`
here and anywhere else, e.g. kolt's checkout if it references the repo).

**kolt stays personal** unless the user says otherwise (it is the dogfood
app, not the language) — flagged as a user call, default no move.

## 3. The Pages problem and the tombstone (the one real design piece)

Pages does **not** redirect: the book moves to
`vilan-lang.github.io/vilan` the moment the docs workflow next publishes,
and `reedsyllas.github.io/vilan` goes dark — while every **released binary
through v0.14.0 deep-links it from editor hovers** (the LSP's keyword
hovers, `document.rs`), and the CHANGELOG's historical entries link it
too. Those binaries' URLs are baked forever; the old URL must keep
answering.

The mechanism — and why it does not violate §0: a **user-site repo**
`ReedSyllas/reedsyllas.github.io` (a different name, so redirects are
safe) serves the `/vilan/*` path once no project repo claims it. It
carries a single `404.html` that redirects *any* path, fragment included:

```html
<script>
  location.replace('https://vilan-lang.github.io'
      + location.pathname + location.hash);
</script>
<!-- plus a plain <a> fallback and <meta name="robots" content="noindex"> -->
```

Every old deep link — `…/vilan/guide/keywords.html#fun` included — lands
on its exact new page. **Zero-downtime ordering**: the user site can be
created and pushed *before* the transfer and sits dormant (while
`ReedSyllas/vilan` exists, its project Pages wins the `/vilan` path); the
instant the repo transfers away, the tombstone takes over. It stays up
indefinitely (binaries ≤ v0.14.0 never stop linking the old URL).

**It broke, and the way it broke is the lesson (found + fixed 2026-07-29,
`ReedSyllas/reedsyllas.github.io@c9ec5c1`).** The tombstone forwarded to
`vilan-lang.github.io/vilan`, correct when written — but the book then moved
AGAIN, to `vilan-lang.org/docs` (`0a0bdd4`), and nothing updated this page.
From that move until the fix, every old hover deep link went old URL →
tombstone → **404**: the exact outcome the tombstone exists to prevent, and
silently, because the page still returned *something*. Two changes came out
of it, and one warning:

- The path is **rewritten**, not passed through: `/vilan/<rest>` →
  `/docs/<rest>`. The old version copied `location.pathname` verbatim, which
  only ever worked because both hosts happened to share the `/vilan` prefix —
  a coincidence, not a design. (Rewriting forces an exact prefix test: a bare
  `indexOf('/vilan')` also matches `/vilanfoo`, which the slice would turn
  into a nonsense `/docsfoo`.)
- It targets the **apex domain**, not `vilan-lang.github.io`. That host 301s
  to `vilan-lang.org` anyway, so aiming at it only added a hop that could rot
  on its own — and did.

The warning: **nothing tests this.** It is a static page, in a repo with no
CI, pointing at a host it does not control, and it is load-bearing for every
binary already installed. No gate in this repository can catch it. If the
book moves a third time, this file is what silently stops working — check it
by hand, and treat "the book moved" as a two-repo change.

## 4. The owner-string sweep (code slice, after transfer)

Grep-verified inventory (2026-07-25) — seven live sites plus history:

| Site | What |
|---|---|
| `crates/vilan-cli/src/upgrade.rs` | `DEFAULT_BASE` — the upgrade discovery root |
| `crates/vilan-lsp/src/document.rs` | the hover deep-link base (`reedsyllas.github.io/vilan`) |
| `scripts/install.sh`, `scripts/install.ps1` | repo + release URLs |
| `editors/vscode/package.json` | `repository`, and `publisher` → `vilan-lang` (the string only; registration is §1, publishing is F7 S2) |
| `README.md` | install one-liners + book link |
| `CHANGELOG.md` | 7 historical book links — swept too (cheap; the tombstone covers stragglers, but shipping new copies of old links is silly) |
| `vilan/docs/**` | book links relative — but **inventory correction (S4, 2026-07-25)**: `guide/walkthrough.md` carried 7 absolute *repo* links (`github.com/ReedSyllas/vilan/tree|blob/…`) the "all relative" claim missed; swept. CHANGELOG was 9 lines/11 URLs, not 7. |

Sweep = one implementer slice: replace, then **gate on the grep** — a new
test in the hygiene family: no tracked file may contain `ReedSyllas/vilan`
or `reedsyllas.github.io` (allowlist: this proposal and `backlog`/history
docs, which discuss the migration itself). The sweep is **versioned into
binaries**: releases before it carry old URLs (saved by redirect +
tombstone), releases after it carry `vilan-lang` natively.

## 5. Order of operations

1. **S1 — claims** (§1; user, one sitting).
2. **S2 — tombstone** (user creates `ReedSyllas/reedsyllas.github.io`;
   the 404.html content is a five-minute deliverable I prepare; pushed,
   dormant).
3. **S3 — transfer** (user: repo settings → transfer to `vilan-lang`;
   pick a quiet moment, no CI in flight). Immediately after: the §2
   verification list, re-point local remotes, push a trivial commit to
   `main` (or re-run docs) so Pages publishes at the new URL, then
   **verify the tombstone redirects a real deep link** and that
   `~/.vilan/bin/vilan upgrade --check` (an old binary) still answers
   through the redirect.
4. **S4 — the sweep** (§4; implementer slice + hygiene gate; rides
   `next`, ships in the next natural release — v0.15.0 — whose cut also
   re-verifies the whole flow under the new owner end to end).
5. **S5 — F7 provisioning unblocked**: secrets created under the org;
   distribution S1–S3 proceed per `distribution.md`.

## 6. Open calls

(a) **The `vilan` npm placeholder publish** (§1.2) — recommend: yes
    (scoped-only would leave the ratified bare name squattable; npm has no
    reservation mechanism). Alternative: accept the risk until F7 S1 ships
    the real package.
(b) **`vilan-lang.org`** — recommend: claim now, use never (until D5).
(c) **kolt** — recommend: stays personal.
(d) **CHANGELOG history sweep** (§4) — recommend: sweep (new copies of
    old links serve no one); alternative: freeze history verbatim.

## 7. Non-goals

Renaming anything *inside* the toolchain (`vilan` is the product name and
is untouched); moving kolt (absent a user call); the domain's actual use;
any F7 publishing (this plan only unblocks it); org teams/permissions
machinery (a sole-maintainer org needs none).
