# std vs official packages — the distribution shape (L10)

> Status: PROPOSED 2026-08-20 (cycle 26, work order 8, lane
> `l10-std-shape`). Proposal-only — no code ships with this paper.
> Tracker: backlog-2026-08-18.md §L item 10.
>
> The owner's question (2026-08-20): restructure std into `std` and
> `official packages` — "or maybe std should be more of a namespace
> under which all of the official packages are published?" The
> orchestrator's inline recommendation, which this paper argues for
> and against: the NAMESPACE model, sequenced behind a registry.
>
> This paper decides nothing the ratified papers decided, and it leans
> on one thing that is deliberately NOT ratified: beta.md §5's tier
> table is a DRAFT whose ruling the owner deferred to the beta
> switch's pre-work (2026-08-20 — "the answers to those questions
> might change"). Everywhere this paper uses the tier seam it cites
> the draft as a draft. process.md §5 (RATIFIED 2026-08-07) is the
> promise floor; beta.md §3.2/§3.3 (RATIFIED 2026-08-18) price it by
> tier; deprecation.md (PROPOSED 2026-08-20, L4) is the machinery and
> already defers user-package promises to "L10's world" (§6).

## 1. Today's physical reality — verified against the loader

**One embedded package (well: two, positionally married).**
`crates/vilan-embedded-std/build.rs` walks `vilan/std` and
`vilan/macro_std` at the workspace root and embeds every `.vl` and
`vilan.toml` into the binary as a sorted `FILES` table plus a
`CONTENT_HASH` over the whole set (60 `.vl` files today: 43
`std/src/*.vl`, 9 `std/src/process/*.vl`, 5 `std/src/browser/*.vl`, 3
`macro_std/src/*.vl` — the same census beta.md §5 tiered into 56
public modules). `vilan_embedded_std::materialize` writes the trees
once to `~/.vilan/std-cache/<CONTENT_HASH>/` (atomic rename, complete
by construction, age-pruned by `vilan upgrade`), and the loader then
reads ordinary files. An installed binary is fully offline and
batteries-included by construction — the std is *inside* it.

**What "the std package" means to the compiler.** `std` is an ordinary
`[library]` (library-packages.md L2): `vilan/std/vilan.toml` declares
`name = "std"` plus two platform layers (`process` for `@process`,
`browser`). `vilan-cli/src/main.rs:2303` (`std_dir`) resolves the
package directory — `$VILAN_STD`, else the nearest checkout ancestor,
else the embedded materialization — and
`vilan-core/src/manifest.rs:975` (`resolve_std`) reads the manifest
into a layered `PackageSpec`. The analyzer then builds **one flat,
root-scoped namespace**: it inventories module stems by a
non-recursive `read_dir` of the base root and each layer root
(analyzer.rs:34289), registers a single `std` module
(`module_id_by_name.insert("std", …)`, analyzer.rs:34358), and loads
`lib.vl` plus every module reachable from it and from the entry's
imports. Two consequences worth stating plainly:

- **std paths are exactly two segments deep** — `std::<stem>::<item>`.
  There is no nesting; a module *is* a file stem in some layer root.
- **Dependencies are already isolated namespaces.** A `Dep` module
  "resolves under its layered roots into its own isolated namespace,
  reachable from a dependent as `<import-name>::name`"
  (analyzer.rs:34381–34395), loaded in a canonical std → deps → pkg
  order. The *mechanism* for "a package whose modules sit under a
  named root" exists; only `std` as that root does not.

`macro_std` is found positionally — `std.base_root.parent().parent()
.join("macro_std")` (macros.rs:310) — so "the std package" is really
"the toolchain pair"; any pinning story must cover both.

**Versioned with the toolchain, and only with it.** The `Library`
manifest struct has no version field at all (manifest.rs:98); the
only version anywhere is the workspace's (0.34.0) and the
`CONTENT_HASH` that keys the cache. `vilan upgrade` replaces the
binary (and `vilan-lsp` beside it) from a GitHub release asset — and
the embedded std with it, atomically, as a side effect of being the
same file. There is no seam at which std could currently be at a
different version than the compiler.

**What a "package" can even be today.** Path dependencies, and git
dependencies pinned to exactly one `tag`/`rev` — "no resolver, no
lockfile, no 'it built yesterday' class of bug" (git_dep.rs header),
content-addressed in `~/.vilan/git-deps`, offline with a warm cache,
nothing fetched passively. The registry spelling (`dep = "1.2"`) is
*parsed and refused*: "registry dependency `{name}` is not yet
supported" (manifest.rs:772). The grammar reserves the future; D5
owns whether that future gets an audience.

## 2. The two shapes

**Shape A — the hard split.** `std` keeps a core; the framework layer
moves out as `official-packages` (or per-package names): `import
reactive::Signal`, or `import official::reactive::Signal`. Separate
versioning, separate docs, an honest name for what is and is not the
standard library.

**Shape B — the namespace.** `std::` becomes the *publishing
namespace* for official packages: the framework modules become
separately-versioned packages published under `std::`, and each
toolchain release bundles a pinned, offline-working set.
`import std::reactive::Signal` never changes spelling; the binary
stays batteries-included; a package can rev between trains for
projects that opt in.

Costed on the six axes:

**Import churn.** A is a churn event with a blast radius the tier
draft already measured: the Tier 2 candidate layer includes the
modules the todo app, the website, and kolt all stand on
(`reactive`/`ui`/`style`/`rpc`/`router`/…), plus 585KB of book whose
every fence is compile-gated. Every one of those spellings changes,
once, for every user, and the old spellings need L4's deprecation
machinery on day one of its life. B is zero churn *by construction* —
today's spellings already are the namespace model's spellings, which
is the single strongest fact in this paper: **the tree is already
forward-compatible with B and already incompatible with A.**

**Docs shape.** Today: one book, one std reference, one planned tiers
page (beta.md §5's docs note). Under A the book splits — a std
reference plus per-package docs, two places to search, and K13's
markdown story would land in the second one. Under B the book keeps
covering `std::*` as one surface; a published package's page gains a
version line. The docs gate (`cargo test --test docs`) keeps
compiling every fence against the bundled set either way only if the
sources stay in this repo — see CI below.

**Batteries-included / offline.** Today this property is not
engineered, it is *structural* (§1). B preserves it by generalizing
the same structure: the embedded `FILES` table becomes "core + the
pinned set", materialized identically. Opt-in newer package versions
cost one fetch into a content-addressed cache — exactly the git-dep
story, warm-cache offline included. A *without* bundling loses the
property outright (the tracker's phrasing stands: a split without
distribution is import churn for no capability); A *with* bundling
rebuilds B's machinery and then adds the churn on top.

**Version skew.** The real cost center, and it cuts against B, so
honestly: today skew is impossible; B makes it a supported state.
Three mitigations keep it from becoming a resolver: (1) the bundled
set is resolved *by the release engineer at cut time* — one coherent
set, tested as one tree, hashed into the binary; (2) an override is a
whole-package exact pin in `vilan.toml` (no ranges — the git-dep
stance extends unchanged); (3) the compiler-known names bound what a
package rev may do (§4). What B may NOT quietly become is per-package
version *ranges* — that is a resolver and a lockfile, the two things
the dependency design deliberately refused. A has the same skew
surface plus one more axis (core vs packages vs compiler).

**What beta's promises attach to.** process.md §5.2's window is
denominated in *minors*, and every train is a minor — the deprecation
sweep literally checks "a released `## vX.Y.Z` section"
(deprecation.md §3). Under A, each package grows its own changelog
and its own minor clock, and "one minor of warning" fragments into
per-package arithmetic — deprecation.md's question generalizes badly
(windows per package, audited where?). Under B the clean answer is:
**promises attach to the toolchain train and its bundled set.** The
pinned set is std for promise purposes; its deprecations ride the one
CHANGELOG the cut script already audits; an out-of-train package
version a project opts into is explicitly outside the window (the
same posture deprecation.md §6 already takes for user packages).

**CI / test surface.** Today one workspace gates everything: the
corpus byte-gate, the docs gate, the std-warning-clean gate, the
module-resolution tests — all against the in-tree std. A multiplies
repos, CI surfaces, and a cross-repo version matrix. B costs nothing
now, and even when publishing is real the cheap shape is
**monorepo-published**: package sources stay in this repo (the tier
seam as directory structure), the registry receives snapshots at cut
time, and the suite keeps testing the exact set the binary bundles.
The new leg B eventually owes: build each publishable package against
its declared toolchain floor.

**The honest case against B** — three arguments, none disposable:

1. **It reintroduces version resolution in miniature.** Pinned set +
   exact overrides is defensible; but the moment two `std::` packages
   depend on each other, "override one" implies a coherence check the
   toolchain must own. Small, but permanent, and it is exactly the
   class of machinery this project has twice declined to build.
2. **It spends the `std` brand.** Today `std::` means "ships with
   your compiler, at your compiler's version." Under B it means
   "blessed by the project, version varies." The spelling no longer
   tells the user which promise they hold — the tiers page and a
   `vilan` command have to. A's names are honest at the price of
   churn; B's continuity is bought with a blurred word.
3. **Nothing demands it yet.** Zero packages exist, no registry
   exists, and 56 modules ship happily as one tree. The house has
   twice taken the null recommendation on demand surveys
   (trait-objects.md, top-level-await.md). The strongest version of
   this argument: B's whole virtue is that *choosing* it costs
   nothing — which is equally an argument that the correct amount to
   build today is nothing.

## 3. Sequencing — nothing splits before a registry exists

1. **Now: decide, build nothing.** The decision is free precisely
   because B is spelling-compatible with today. Record the direction;
   keep one std; keep the alpha/beta work (L3's tiers, L4's
   machinery) exactly as planned. The tier table — cited as the draft
   it is — is the seam definition: Tier 1 core is the inseparable
   floor (§4 makes that structural, not just editorial), the Tier 2
   framework layer is the candidate publishing surface. The seam gets
   re-read when the deferred §5.1 ruling happens at the switch.
2. **The registry is D5's world.** There is no registry
   (manifest.rs:772 refuses the reserved spelling), and a registry
   without users is a service bill — process.md §7.1 already named D5
   the policy's urgent dependency. When it exists, *user* packages
   exercise it first; std is deliberately not the registry's pilot
   customer.
3. **The namespace switch is additive.** When publishing is real and
   a reason exists (a package that wants to rev between trains, or
   K13's markdown story wanting a home — see below), the framework
   modules become packages published under `std::`, each toolchain
   release bundling the pinned set. No spelling changes; no book
   split; users who do nothing observe nothing.
4. **The hard split is never on the path.** It is not a fallback
   position of B; it is a different, churn-priced product. Declining
   it now is a real decision, not a deferral.

**K13's markdown story is the first candidate — say it now.** A
`std::markdown` (docs-port.md §3.3: a parser producing a plain-data
AST) is new (no churn either way), demand-backed (the docs port is
blocked on it), pure vilan, platform-neutral, and compiler-uncoupled.
Under B it can ship *in* std at Tier 2 tomorrow and be re-homed as
the first published `std::` package later with zero spelling change —
the model's proof case. Under A it would have to guess its permanent
name before the split exists. If the markdown story is built before
any of this, building it package-shaped (own directory, no
compiler-known names, no cross-layer entanglement) costs nothing and
keeps the proof available.

## 4. What the compiler must grow — either shape, mostly B's

**Package identity for std modules.** Today a std module's identity
is a file stem in a layer root; the analyzer neither knows nor needs
a package boundary inside the namespace. B's eventual loader change:
the std namespace is populated from a **manifest of entries**, each
either "embedded" or "package `<name>` at exact version `<v>`, hash
`<h>`" — resolved through the same isolated-namespace machinery
`Origin::Dep` already implements, grafted under the `std` root
instead of a sibling root. Root-scoped flatness survives (a package
supplies stems); the platform-layer mechanism survives (any
`[library]` may declare layers — `std::ui`'s two halves stay one
module). One hygiene rule should land *before* any of this matters:
`Manifest::validate` reserves nothing today, so nothing stops a user
declaring a dependency whose import-name is `std` — at best silently
shadowed by the real namespace, at worst ambiguous (the failure mode
is untested because the case is unconsidered, which is the point) —
reserve `std`, `pkg`, and `macro_std` as dependency import-names now
(small, and correct under every shape including the status quo).

**A per-release pinning manifest.** It already exists in degenerate
form: the embedded `FILES` table + `CONTENT_HASH` *is* a pinned,
hashed, offline set of everything std-shaped, and the cache layout
already knows how to hold multiple sets side by side. B generalizes
it to named entries with versions; the binary still embeds the
bundled sources (batteries stay structural, not fetched); the
manifest is what `vilan --version`-style tooling and the docs read.
`macro_std` rides the same manifest — its positional discovery
(macros.rs:310) becomes an entry like any other.

**Tier 1 is structurally inseparable — verified, not asserted.** The
compiler holds Tier 1 core by identity, not by import: the prelude
primitives are std source whose ids the analyzer captures at load
(`list.vl`'s `List::new`/`push` lower to `[]`/`.push`; `str`, `bool`,
`null` are module-defined); the transformer resolves `print` out of
the std scope by name and panics without it (transformer.rs:1665);
`context`/`nursery` intrinsics are captured the same way. A
separately-versioned Tier 1 is therefore fiction — core std and the
compiler are one artifact with one version, under every shape. And
the entanglement does not stop cleanly at the tier seam, which B must
price: `Signal` (reactive) is captured for HMR transfer
classification (analyzer.rs:35361), `JsonValue` (json) for lowering,
and a `[service]` attribute force-loads `std::rpc` because the
`service` macro lives there (analyzer.rs:34448). **Compiler-known
names are part of the toolchain contract**: a published package's rev
may not move or rename them except in step with a toolchain release.
Each publishable package therefore declares a minimum toolchain (a
single floor, not a range), and the compiler-known-name list should
be written down once, as the packages' side of the contract.

**`vilan upgrade` and a registry coexist by scope.** `upgrade` stays
what it is: whole-toolchain, binary + embedded set, atomic, steered
away when npm/Homebrew own the install (upgrade.rs). Package version
choice is *per project, in `vilan.toml`* — an exact-pin override of a
bundled entry — so there is no second global mutable state and no
`vilan upgrade std::x` command. The registry cache mirrors git-deps:
content-addressed, never stale, warm-cache offline, fetched only by a
build that declares the pin. The one new interaction: `upgrade`
moving the bundled set forward must warn when a project's explicit
pin now *lags* the bundle — a diagnostic, not a resolver.

## 5. Recommendation

**The namespace model, as a recorded direction — and no construction
now.** Ratify three sentences: (1) `std::` is the publishing
namespace; if official packages ever exist they are published under
it, each toolchain release bundling a pinned offline-working set, and
promises attaching to the train's bundled set; (2) the hard split is
declined — spelling churn and a split book buy nothing the tier
table's published promises don't already deliver; (3) nothing is
built until D5's registry exists and a concrete package wants out of
the train — with `std::markdown` (K13) named as the expected first
case. The honest counter-arguments (§2) are answered by the
sequencing, not dismissed: the resolver-in-miniature risk is fenced
by exact-pins-only, the brand question is deferred to the moment a
package first actually revs off-train (nothing is blurred while the
set and the train are identical), and the null-demand point is
conceded — which is why the recommendation ships zero code. One
hygiene exception: file the reserved-import-name rule (§4) as a
small backlog item now.

## 6. Owner questions

1. **The direction.** Ratify namespace-over-split as recorded intent
   (§5's three sentences), building nothing now? This forecloses only
   the hard split; every future choice about *when* stays open.
   Recommend: yes — today's spellings already commit us cheaply.
2. **What the window is denominated in.** When packages can rev
   between trains: recommend beta's promises attach to the toolchain
   train and its bundled set only — an opted-into off-train package
   version carries no deprecation window (deprecation.md §6's posture
   generalized). The alternative — per-package windows on per-package
   minors — multiplies L4's audit surface. Accept?
3. **May the sequencing lean on the draft tier seam?** §3 treats the
   deferred tier table's Tier 1/Tier 2 boundary as the seam defining
   what could ever publish, subject to re-reading at the switch. If
   you expect the seam itself (not just row assignments) to move,
   this paper's §3 step 3 should wait for the ruling instead.
4. **Build `std::markdown` package-shaped?** When K13's markdown
   story is built, build it as if published (own directory, no
   compiler-known names) so it can become the first `std::` package
   without rework — at essentially zero extra cost. Accept?
5. **The one code item.** Reserve `std`/`pkg`/`macro_std` as
   dependency import-names in `Manifest::validate` — file now as a
   small hygiene item (correct under every shape)?
