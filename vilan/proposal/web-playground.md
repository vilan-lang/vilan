# Web playground — the compiler in the visitor's browser (D11)

> **Status: S0–S3 BUILT — S3 DONE 2026-08-01** (website repo, uncommitted
> pending review; committing + pushing the website deploys the page). S4
> (share-via-fragment, badge placement, editor niceties) remains, each
> independently shippable. §7 calls SETTLED 2026-07-28 — the user took
> every recommendation: (a) vendored CodeMirror 6, (b) toolchain rides the
> site build's source, (c) compile on Run only in v1, (d) the path is
> `/playground`; promotion timing stays with D5/D10.
> Original status: DRAFT 2026-07-28 — for review. Backlog D11 (user request
> 2026-07-28). Decides the architecture question the backlog poses: **(a)
> in-browser WASM compile** is proposed; (b) a server-side compile service is
> rejected for v1 (§1). Sequencing with D5 (promotion) and D10/F9 (org
> identity) is the user's call and is recorded, not decided, here (§7d).

## 0. What exists (all verified 2026-07-28)

- `vilan-core` is the whole compiler behind two thin front-ends (`lib.rs:1`).
  Its dependency closure is `indexmap`, `serde`, `toml` and their pure-Rust
  transitives — no tokio, no rayon, no libc, no `getrandom`. Nothing on the
  compile path touches threads, time, env, or process APIs (`git_dep.rs` and
  `#[cfg(test)]` excepted, both avoidable). It should compile for
  `wasm32-unknown-unknown` today; S0 verifies that in minutes.
- **An in-memory compile path already exists and the LSP uses it**:
  `analyze_source(source, std, pkg_root, entry_path, platform, workspace)`
  (`lib.rs:283`) plus `transform` (`transformer.rs:16`), fed by the document
  overlay `set_document_overlay(path, text)` (`analyzer.rs:22176`), which
  `load_package_module` consults before disk. The overlay is incomplete —
  `resolve_module_file`'s two `.exists()` probes (`analyzer.rs:23168`) and
  `util::read_source` (`util.rs:26`) still go straight to the filesystem —
  and that gap is the only core change this proposal needs (§2).
- `vilan-embedded-std` carries the full `std` + `macro_std` trees as data (59
  files, ~357 KB of source) but only exposes them by `materialize()`-ing to a
  real directory — filesystem-shaped by design. `FILES` is already
  `pub static`; the playground reads it directly and never materializes.
- Browser-leg output is **one self-contained JS file with zero imports by
  construction**: the browser layer's externs are module-less globals, the
  `__` runtime helpers are inlined, std is compiled in like user code, and the
  file is valid as a `<script type="module">`. CSS comes from the pure
  `assemble_assets` (`const_eval.rs:66`). Nothing else is needed to run.
- Diagnostics are minimal and structured — `Error { span, msg, note }` with
  byte spans, severity positional (`Program.diagnostics` vs `.warnings`),
  per-file attribution via `SourceId`. ariadne is CLI-only; the LSP's
  `line_index.rs` (126 pure lines) already converts byte spans to UTF-16
  line/col. Exactly the shape an editor pane wants.
- The site is a single SSR page (`server.vl` matches three paths and a
  catch-all); deploy.yml builds the toolchain from `vilan@main`, renders
  `http://localhost:3000/`, and commits an explicit three-file allowlist to
  the pages repo. The pages repo already serves multi-MB files (2.19 MB
  search index) and has no root `.nojekyll` (only `docs/` has one).
- Stale pointer, for the record: D11's "`examples/playground` is the
  local-CLI cousin" — that directory was pruned in the D7 cleanup; only a
  prose comment in `inference.rs` remembers it. The sibling workspace dir
  `vilan-playground/` is personal scratch, unrelated.

## 1. The decision — in-browser (architecture (a))

The compiler compiles to WASM, runs in the visitor's browser, and the JS it
emits executes in a sandboxed iframe. **No server exists anywhere in this
project today, and this proposal does not create one.** A compile service
(architecture (b)) would be the project's first hosted process — provisioned,
rate-limited, patched, and paid for — to save a one-time download that §4
budgets at roughly 2 MB compressed. It buys running node-leg programs nothing
(executing visitor code server-side is off the table either way, per the
backlog). Rejected for v1, not deferred out of difficulty: if (a) ships, (b)
has no remaining job. The one thing (b) would have avoided — the WASM build's
size and trap surface — is bounded and named in §6.

The accepted asymmetry: browser-leg programs run; process-leg programs
typecheck. That is the good half of the language's own platform story —
`platform_color::check` reports a call chain from the entry when a browser
build reaches `std::http`, and that diagnostic rendering in the pane *is* the
pitch. Distinct from F3 throughout: F3 makes vilan *programs* target WASM;
this compiles the *compiler* to WASM. They share nothing but the word.

## 2. The mechanism — `vilan-wasm` over the document overlay

A new workspace crate `crates/vilan-wasm` (`cdylib`, wasm-bindgen), a thin
third front-end beside the CLI and LSP:

- **Boot**: iterate `vilan_embedded_std::FILES`, registering each entry in the
  document overlay under a synthetic root (`/toolchain/std/...`,
  `/toolchain/macro_std/...`). Hand-construct the `PackageSpec` (all fields
  `pub`) with std's known layer table — base `src`, `process = @process`,
  `browser = browser` — skipping `resolve_std` and all manifest discovery.
  `Workspace::default()` keeps `git_dep` unreachable.
- **Core patch (the S1 arc)**: teach `resolve_module_file` and
  `util::read_source` to consult the overlay before the filesystem, so overlay
  entries resolve as modules without a disk. ~30 lines across two files, and
  an honest improvement on its own: it completes the LSP's overlay story for
  unsaved files. No new seam, no virtual-filesystem trait — the overlay is
  already the seam.
- **Export**: `compile(source: String) -> CompileResult` where the result
  carries `js`, `css` (from `assemble_assets`), and a diagnostics array of
  `{ start, end, line, col, message, note, severity, file }` — spans converted
  to UTF-16 line/col on the Rust side (port of `line_index`, or move it into
  core where both front-ends reach it). The compile is always
  `Some(Platform::Browser)`, which also bypasses `infer_platform`'s disk
  probing. Also exported: the toolchain version string for the page's badge.
- **The user program** enters through the same overlay under `/project/`,
  entry `main.vl`, single file in v1. `parse_clean_cached` is
  content-addressed, so std parses once per instance and warm compiles are
  cheap.

## 3. The page

`vilan-lang.org/playground` — a second SSR-rendered page in the website
package, written in vilan like the rest of the site (the `[extern]` +
`external struct` pattern in `client.vl` already binds ~100 lines of browser
API; the editor binds the same way). Layout: editor pane, Run, output beside
it, diagnostics beneath the editor.

- **Compile**: the WASM instance lives in a Web Worker. Run sends the buffer,
  the worker answers with `CompileResult`. Compile-on-Run only in v1 (§7c
  records the live-diagnostics call); a queued single-flight discipline —
  one compile in the instance at a time (core's global caches assume it).
- **Run**: rebuild an `<iframe sandbox="allow-scripts" srcdoc=...>` per run —
  opaque origin, no same-origin access, torn down and rebuilt each Run, which
  is also the story for runaway loops (the frame is removed, not reasoned
  with). The srcdoc carries `<style>` (the emitted CSS), a mount div, and the
  emitted JS as a module script. `console.log`/`print` and uncaught errors
  forward to the output pane via `postMessage`.
- **Diagnostics**: errors and warnings rendered in-pane with the span
  highlighted in the editor and the note attached — the compiler-showcase
  treatment, not a raw dump. A compiler trap (panic) surfaces as "the
  compiler crashed on this input — please report it" with a prefilled issue
  link; the worker is recreated silently.
- **Seeding**: an examples dropdown of a few curated browser-leg programs
  (the reactive counter first), doubling as D6's try-it-without-installing
  path.

## 4. Delivery — building, shipping, serving

- **Build**: `cargo build -p vilan-wasm --target wasm32-unknown-unknown
  --release` with `opt-level = "z"`, fat LTO, `codegen-units = 1`,
  `-C link-arg=-zstack-size=67108864` (§6), then `wasm-opt -Oz`. Estimate:
  4–7 MB raw, ~1.5–2.5 MB compressed (the native CLI is 5.85 MB stripped and
  carries ariadne/clap that WASM won't; std's 357 KB of source text
  compresses well). S0/S2 replace the estimate with a measurement.
- **Ship**: the artifact is committed to the pages repo **gzipped**
  (`playground/vilan.wasm.gz`); the page fetches it, pipes through
  `DecompressionStream("gzip")`, and instantiates from the buffer. This
  sidesteps both unknowns GitHub Pages refuses to let us control — the
  `.wasm` Content-Type and whether Pages compresses `application/wasm` — and
  halves the git-history cost of each rebuild. Fetched lazily on the
  playground route only; the landing page pays nothing.
> **AMENDED 2026-07-29 — the two bullets below are superseded on delivery;
> everything else in them stands.** They assumed the site deploy job builds
> from a `toolchain/` source checkout. It does not: `53aa11d` switched
> deploy.yml to install the toolchain from the latest release via
> `install.sh`, and its header states the property outright — "No Rust
> toolchain, no cargo, no cache." The job checks out only the website repo and
> the pages repo, so a wasm build step could not have run there.
>
> **The user's call (2026-07-29): architecture (b).** `vilan-wasm` builds in
> THIS repo's release pipeline and publishes as a release asset; the site job
> downloads it exactly as it already downloads `install.sh`. Rejected: (a)
> re-adding Rust + a source checkout to the site job, which gives back the
> weight `53aa11d` removed; (c) a separate workflow committing the artifact to
> the pages repo, which adds a third moving part.
>
> Consequences to carry into S2/S3:
> - The wasm artifact joins release.yml's build matrix and asset list, beside
>   the platform archives and the vsix. The completeness gate that already
>   walks release assets should cover it.
> - The "Version" bullet becomes *whatever release the site installs* — still
>   the single lever §7(b) wanted, with one less moving part, and the badge
>   reports a released version rather than a main-of-the-moment build.
> - The playground's freshness is now tied to cutting releases. Acceptable,
>   and it makes the dormant publish channels (F7) worth more.
> - The pages repo still receives the artifact via the site job's explicit
>   allowlist; only its SOURCE changes (downloaded, not built in-job).
>
- **Wire**: deploy.yml grows a wasm build step against the `toolchain/`
  checkout already present in the job, a second render
  (`curl .../playground -o export/playground/index.html` plus the `<!--ssr-->`
  guard, currently written against a single file), and the new paths added to
  the explicit `cp` + `git add` allowlist — nothing ships from that job
  unless allowlisted, deliberately. The wasm artifact is committed only when
  the toolchain hash changes. A root `.nojekyll` lands alongside (the root is
  Jekyll-processed today; `docs/` already carries one). The playground lives
  at root, not under `docs/`, clear of docs.yml's `rm -rf docs` rebuild.
- **Version**: the playground compiles with whatever the site's deploy
  builds — `vilan@main` today, release tarballs when deploy.yml makes that
  already-recorded switch. One lever, not two (§7b). The page badges the
  version the wasm reports.

## 5. What v1 explicitly does not do

- **No server-side anything** — no compile service, no snippet storage, no
  telemetry. Sharing, when it comes, is a URL fragment (§8, S4):
  `CompressionStream`-deflated source, base64url, never in server logs.
- **No process-leg execution**, and in v1 not even a process-leg check mode —
  Run always compiles `Platform::Browser`. The check-only toggle is recorded
  future work (§8), not scope.
- **No editor intelligence beyond diagnostics** — no completion, hover, or
  semantic tokens. That is the LSP compiled to WASM, a different and much
  larger arc; recorded, not planned.
- **No multi-file projects, no manifest editing** — one buffer, fixed
  entry, std only. `Manifest::parse` is pure and ready when a driver appears.
- **No wasm threads / SharedArrayBuffer** — Pages cannot set COOP/COEP, and
  nothing here wants them.

## 6. Risks, named

- **`panic=abort` disarms the B40 fences.** `catch_unwind` at `lib.rs:290`
  and `:369` is dead code on wasm32-unknown-unknown; a compiler panic traps
  the instance and poisons its memory. Mitigation is structural: the worker
  *is* the fence — trap → recreate instance → report (§3). The blast radius
  is one compile, same as the LSP's per-request fence.
- **Stack depth.** The CLI and LSP both run compiles on a 256 MB stack
  (`main.rs:155` — deep AST/type recursion on valid programs); wasm defaults
  to 1 MB, and overflow is an unrecoverable trap. `-zstack-size` to 64 MB
  covers playground-scale programs; macro-world compiles (a full compile
  nested inside a compile) are the deepest case and get watched in S2. A trap
  is handled identically to a panic: recycle, report.
- **Leaks by design.** `analyze_source` requires `&'static str`; every
  compile `Box::leak`s its buffer, and four process-global caches never
  evict. Linear memory only grows. Compile-on-Run bounds the rate; the worker
  recycles the instance every N compiles (N tuned in S2) — a page-lifetime
  ceiling, not a fix, and that is fine for a playground. One case tunes N
  down, not up: a buffer that *defines* a macro recompiles its world on any
  length-changing edit (`blank_to_world` is length-keyed), leaking a full
  world `Program` per Run — E3 Phase 1's recorded-but-unmeasured residual.
- **Size.** If S0/S2 measure materially above the ~2 MB compressed estimate,
  the fallback levers are `wasm-opt` flags, dropping `macro_std` from the
  embedded set for the playground build, and — only if it comes to it —
  reopening (b). The estimate has to fail badly before a server beats a
  lazy-loaded 3 MB fetch.
- **Repo growth.** Each committed wasm rebuild is a multi-MB blob in the
  pages repo's history forever. Commit-on-toolchain-change plus the
  release-tarball switch (which drops rebuild frequency to release cadence)
  keeps this to a few blobs per release. Named, accepted.

## 7. Open calls — wanted before S1

- **(a) Editor**: vendored CodeMirror 6 (one ESM bundle built once, committed
  to `assets/`, ~200 KB, bound via the site's existing `external struct`
  pattern; no CDN at runtime, per the site's zero-external-dependency stance)
  vs a plain `<textarea>` v1 (no dependency, but no span highlighting — and
  the diagnostics are the pitch). **Recommendation: vendored CodeMirror 6.**
- **(b) Toolchain pinning**: ride the site build's source (main now, release
  tarballs when deploy.yml switches) vs an independent release-only pin for
  the playground. **Recommendation: ride the site build — one lever;** the
  badge makes whatever it is honest.
- **(c) Compile cadence**: Run-only vs debounced compile-as-you-type for live
  diagnostics. **Recommendation: Run-only in v1** — it bounds the leak rate
  and the worker churn; live diagnostics become an S4-or-later slice with the
  recycling policy proven.
- **(d) Name and promotion** (recorded as the user's call, per the backlog):
  `/playground` as the path; when it gets linked from the landing page and
  the book interacts with D5's traction plan and D10/F9's org timing. Nothing
  in S1–S3 blocks on this — the page can exist unlinked.

## 8. Slices (suite-gated, docs same commit, per-case pins)

- **S0 — the spike — DONE 2026-07-29. Verdict: GO.**
  - `cargo check -p vilan-core --target wasm32-unknown-unknown` is **clean** —
    zero errors, zero warnings, 6.4s, and the whole dependency tree (indexmap,
    toml, serde, hashbrown, winnow) comes along without a murmur. No `cfg`
    surgery, no dependency swap, no forked crate. This was the architecture's
    single biggest risk and it is simply not a problem.
  - **Measured artifact: 1.58 MB raw (1,652,575 B), 0.54 MB gzip -9
    (570,784 B).** With std's source alongside (476 KB raw, ~100 KB gzipped)
    the page ships **~2.06 MB raw / ~0.64 MB compressed**. §4's estimate was
    4–7 MB raw and 1.5–2.5 MB compressed — the real thing is ~4× smaller than
    the optimistic end of that range, so every sizing argument in this
    proposal has more headroom than it claimed, and the gzip-to-pages ship
    plan is comfortable rather than tight.
  - Method: a throwaway `cdylib` (scratchpad, not committed) depending on
    `vilan-core` and exporting one `extern "C"` entry that calls
    `analyze_source` then `transform`, so the linker retains both halves of
    the pipeline. Built with §4's flags — `opt-level = "z"`, fat LTO,
    `codegen-units = 1`, `strip = true`.
  - **Read the number as a lower bound, for three reasons that push in both
    directions.** Up: wasm-bindgen's glue is not in it, and code reachable
    only from paths my one entry point does not touch may have been
    dead-code-eliminated. Down: no `wasm-opt -Oz` pass ran (not installed),
    which typically takes another 10–20%. The bound is loose enough that even
    2× the measurement stays well inside budget, which is what makes this a GO
    rather than a "measure again first".
  - **Correction to this slice's own plan:** the second half as written —
    "compiling one counter program end to end in a browser" — cannot happen at
    S0. `analyze_source` resolves `import std::print` through the filesystem
    (`PackageSpec` is `PathBuf`-rooted; `vilan-embedded-std::materialize()`
    writes std to a cache dir), so a real compile needs the overlay work that
    IS S1. The dependency runs S0 -> S1 -> end-to-end, not S0 -> end-to-end.
    Nothing is lost: the size question is what S0 existed to answer, and it is
    answered.
  - **Tooling gap for S2:** `wasm-bindgen`, `wasm-opt` and `wasm-pack` are all
    absent from this machine. S2 needs at least `wasm-bindgen`; the CI leg
    will need it too, and `wasm-opt` if the release pipeline runs the
    size pass (it should, per §4).
- **S1 — overlay completion — DONE 2026-07-29** (`3f387fe`). `read_source` is
  now the one overlay-then-disk seam and `resolve_module_file` reads "exists"
  as on-disk-OR-buffered; `load_package_module` dropped the open-coded match
  that was the reason the overlay reached it and nothing else. Both editor
  bugs the slice promised are closed: an unsaved-only module was invisible
  (the existence probe said no, so the one overlay reader was never reached),
  and analysis-vs-publish read different texts for an unsaved on-disk module,
  putting every diagnostic in it off by the line delta. The BOM asymmetry is
  preserved and pinned (disk stripped, buffer verbatim). Five pins on a new
  `analyze_overlay_package` helper that writes nothing to disk — **that helper
  is S2's dry run for the wasm boot path**, so S2 starts with its module
  resolution already proven. Both halves proven non-vacuous by planted probe;
  corpus byte-identical (the CLI never populates the overlay, so the
  `OnceLock` stays uninitialized); full suite green.
  - *Unplanned fix the slice forced:* `location_for` routes EVERY non-entry
    source through the LSP's `line_indices` cache, not just `std` despite its
    comment, and that cache never invalidates. Once `read_source` answered
    from the overlay it began caching buffer text forever — stale after one
    keystroke, where before it was only stale after a save. A buffered path is
    now indexed fresh and never cached. Worth knowing for S3: anything derived
    from a buffer is valid only until the next edit, which is why
    `document_overlay_contains` is public.
- **S2 — `crates/vilan-wasm` — DONE 2026-07-29.** `["cdylib", "rlib"]`: the
  boot-and-compile logic is plain Rust tested natively, the `wasm_bindgen`
  layer is a type conversion gated to `target_arch = "wasm32"` (so a host build
  never pulls the dependency), and the CI leg's only job is proving the crate
  still REACHES wasm32 — the failure host tests cannot see. 15 pins.
  **Measured: 2.22 MB raw / 0.64 MB gzipped**, embedded std included, matching
  S0's projection. Needed a dedicated `[profile.wasm-release]`: putting §4's
  size flags on `[profile.release]` would shrink and slow the native binaries
  for everyone. `panic = "abort"` deliberately NOT set despite the size win —
  core fences analysis in `catch_unwind` so a compiler panic degrades to one
  diagnostic, and aborting trades that for a dead instance.
  - **The layer order is load-bearing.** `Library::layer` is a `BTreeMap`, so
    `resolve_std` yields `browser` before `process` whatever the manifest says,
    and `matching_layers` sorts stably so ties keep it. The hand-built spec
    reproduces that order; getting it backwards would resolve differently from
    every other front-end. `the_hand_built_std_spec_matches_the_manifest`
    compares the hard-coded spec against the manifest shipping in `FILES`, so
    a new layer fails loudly instead of being silently dropped.
  - **Two core patches S2 forced**, both the same root cause S1 fixed — a disk
    probe that no overlay can answer. `resolve_macro_std` gated on `is_file()`,
    so every program defining a `macro fun` or using `[service]` reported
    `macro_std` missing; and `resolve_library` read the manifest with raw
    `fs::read_to_string`, bypassing the seam. Both now go through the one
    reader. Neither changes the editor: the LSP deliberately never REGISTERS a
    `vilan.toml` overlay, so the lookup misses and falls through to disk
    exactly as before.
  - **Recorded v1 limitations, none blocking.** `analyze`'s std-module
    inventory and `modules_in_root` both walk `read_dir`, which degrades to
    empty rather than erroring — so a failed import in the playground loses its
    "did you mean `std::option`?" steer. Cosmetic, and worth knowing before it
    is reported as a bug. `macros.rs`'s `rpc.vl` `is_file()` guard is the same
    class.
  - **Release wiring (call (b)):** a `wasm` job ships
    `vilan-playground-wasm.tar.gz` (gzipped wasm + the JS glue), joining
    `publish`'s existing `release-*` glob and checksum step with no edits
    there. `wasm-bindgen` is pinned `=0.2.126` in three places that must agree
    — crate, CI, release — because a mismatch fails in the browser at runtime,
    not at build time.
  - **F10's gate fired, as designed:** the new dependencies put seven crates in
    `Cargo.lock` that `THIRD-PARTY-NOTICES.txt` did not cover, and the suite
    refused until it was regenerated. Working as intended, and a reminder that
    adding a dependency to this workspace is never just a Cargo.toml edit.
  - *Still open for S3:* stack-depth and instance-recycle tuning were not
    measured here — they want a real browser, which S3 brings. The
    `-zstack-size=67108864` link arg is carried from §6 unverified.
- **S3 — the page (website + pages repos) — DONE 2026-08-01** (website repo,
  uncommitted pending review; the next website push DEPLOYS it). What
  shipped, and where it deliberately differs from the sketch above:
  - **A third entry, not a second render of one page**: `[entry.playground]`
    in the manifest — `src/playground_page.vl` is the shared view (both legs,
    SSR parity), `src/playground.vl` the browser entry, `src/playground.html`
    the shell. The landing page pays nothing. `server.vl` serves the routes
    (`/playground` + `/playground/*` assets, both slash spellings) and reads
    the gzipped wasm as `Bytes` via a local `readFile` extern.
  - **The vilan/JS split found its seam**: the page's STATE and rendering are
    vilan (signals in, closures in, `bind_each` panes); the vendored bundle
    owns the DELIVERY machinery vilan cannot express — the editor widget, the
    worker lifecycle (a Worker needs `new`), and the per-Run
    `sandbox="allow-scripts"` srcdoc iframe with its console/error
    `postMessage` bootstrap. `window.VilanPlayground` is the whole interface.
  - **Vendored CodeMirror 6 per call (a)**: `playground/editor-src/` (npm,
    esbuild) builds the committed `playground/editor.js` (328 KB minified
    IIFE): brand theme, a StreamLanguage vilan tokenizer (the lexer's keyword
    list), `@codemirror/lint` squiggles fed straight from the worker's
    diagnostics (UTF-16 line/col for the anchor; byte-length end, recorded
    approximation).
  - **Worker policy (§6 made concrete)**: single-flight with a latest-pending
    queue (a Run is never silently lost), crash → recycle + respawn, recycle
    after 32 compiles at idle, load-failure respawn capped at 3. The 64 MB
    stack link-arg rode the release build the browser test ran against;
    neither it nor N=32 has been stress-measured — tune when a real program
    complains.
  - **Examples are files, not a dropdown**: `playground/examples/*.vl`
    (counter, hello, styles) picked by buttons; shipped as the GENERATED
    `playground/examples.js` (`scripts/gen-examples.mjs`, deterministic), so
    the smoke gate can regenerate and byte-compare — a stale copy fails the
    deploy instead of shipping. (The gate caught exactly that when `vilan
    fmt` reflowed two examples during the build-out.)
  - **Delivery per the 2026-07-29 amendment**: `scripts/fetch-wasm.sh` pulls
    `vilan-playground-wasm.tar.gz` from `releases/latest` — the same lever as
    the toolchain install — locally into gitignored `playground/wasm/` and in
    deploy.yml before the build. The deploy renders both pages (ssr-marker
    guard on each), runs `scripts/smoke-playground.mjs` (wasm loads + reports
    a version; examples.js current; every example compiles clean), and
    commits the enumerated `playground/` set + root `.nojekyll` to the pages
    repo; unchanged wasm bytes drop out of the commit naturally.
  - **Verified end to end in a real browser** (headless Chrome against the
    local server): editor mounts, worker gunzips + inits wasm ("Ready — vilan
    0.18.2"), Run compiles and mounts the counter, a click inside the iframe
    drives its signal, console forwards to the pane, a broken program renders
    the compiler's steer in-pane AND squiggles the span, and the styles
    example's compiler-emitted CSS applies. The status line already reports
    the wasm's version, so S4's badge is a placement question, not plumbing.
  - **The page is UNLINKED** per §7d — no nav or landing-page link anywhere;
    promotion stays with D5/D10.
  Original slice text: `playground.vl`, the `server.vl` route, worker +
  iframe runner, diagnostics pane, examples dropdown; deploy.yml wiring per
  §4 (second render, allowlist additions, wasm build + commit-on-change, root
  `.nojekyll`). Gate: the deploy's existing render check extended to the
  second page, plus a scripted smoke-compile of each seeded example against
  the shipped wasm.
- **S4 — polish:** share-via-fragment, version badge, editor niceties.
  Each independently shippable after S3.
  - **Share-via-fragment — SHIPPED 2026-08-01** (website `4a229f9`, deployed).
    `#code=<base64url(deflate-raw(source))>` per §5: Share writes the
    fragment into the address bar (`window.history.replaceState` — bare
    `history` inside the bundle is CodeMirror's undo extension, a real
    collision found by the first test run) and copies the full URL; the
    status line reports "copied" or, when the clipboard refuses (permission,
    insecure context), "ready in the address bar" — the link exists either
    way. A page opened with a fragment loads it in place of the default
    example; a mangled payload falls back. Browser-verified: clipboard
    exact, unicode round-trips byte-identically, the shared program
    compiles and runs, malformed links fall back.
  - **Format button — SHIPPED 2026-08-01 (dormant until the next release).**
    `vilan-wasm` exports `format` (the CLI's `formatter::format` exactly:
    canonical layout or the original bytes on a bail; two pins in
    `tests/compile.rs`). The page feature-detects it: the worker imports the
    glue as a NAMESPACE (a static named import of a missing export would fail
    the whole module on an older glue) and reports `canFormat` in its ready
    message; the button `show`s on it. Verified in-browser against a
    next-built wasm (format + idempotence + bail-untouched + compile-after)
    AND against the v0.18.2 release wasm (button hidden). Also shipped with
    it, same website commit: the workbench column widened to 1880px (the
    site's 1264 column stays everywhere else), and the styles example hoists
    its view chain into a `let` so the formatter can split it (the
    mount-argument shape is backlog 43's boundary, sidestepped).

## 9. Recorded future work (not planned)

Process-leg check-only mode (one toggle, `Some(Platform::Node)`, the
platform-coloring showcase); live diagnostics (§7c); multi-file/tabs and
manifest editing; LSP-in-the-browser; snippet sharing beyond the fragment
(anything with storage reopens the no-server stance deliberately, not by
drift); prerendered playground embeds in the book's "Try it" blocks (D6's
natural continuation).
