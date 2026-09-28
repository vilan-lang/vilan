# hygiene-43 lane status

Worktree: `vilan/.claude/worktrees/hygiene-43`, branch `hygiene-43` off `origin/next` @762c6aa5.

## Items

- **N133** — LANDED @3e2c750f. `StagedDir` (Drop guard) removes `native_differential`'s
  per-test staging directory at test end; `vilan fmt`'s (and the watcher's) `TreeWalk`
  skips any directory carrying a `CACHEDIR.TAG` (Cache Directory Tagging Spec), so the
  formatter never walks into `target/` (or any other cargo target dir, name-independent).
  Gates green: default + `VILAN_NATIVE_DIFFERENTIAL=1` native_differential (60/61 then
  61 after N106 added one test), fmt/clippy clean.
- **N106** — LANDED @0816332e. Added `f64-print-boundary.vl` (DEFAULT_SUITE, identical
  both backends: 1e21, 1e-7, Infinity, -Infinity, NaN) and `non-bmp-string-length.vl`
  (DEFAULT_SUITE, identical — `str_len`'s stale "BMP only" doc comment corrected; verified
  a surrogate-pair character already agrees on both backends via `char::len_utf16`).
  Found a REAL, previously undocumented divergence: `print(0f * -1f)` — negative zero —
  prints `"-0"` on JS (`console.log`'s `util.inspect` special case) vs `"0"` natively
  (`vilan_rt::js_number`, which matches `String(x)`/template-literal/`JSON.stringify`
  semantics, not `console.log`'s). Documented at the printing site (`print`'s and
  `js_number`'s doc comments) and pinned OUTSIDE the differential by name
  (`f64-print-negative-zero.vl`, mirroring `usize-underflow.vl`'s pattern) rather than
  "fixed" — changing `print`'s -0 rendering to match console.log is a real but separate
  design question (see FINDS below). Gates green both modes.
- **L22** — LANDED @1946f746. `publish-brew`'s tap-token mint moved from
  `actions/create-github-app-token@v2.2.2`'s `app-id` (deprecated at v3.1.0, confirmed via
  `gh api repos/actions/create-github-app-token/releases/tags/v3.1.0`) to `client-id` at
  v3.2.0 (sha `bcd2ba49218906704ab6c1aa796996da409d3eb1`, confirmed via `gh api
  repos/actions/create-github-app-token/tags`). New secret `TAP_APP_CLIENT_ID` read by the
  gate and the mint step; `TAP_APP_ID` stays until a release goes out on the new input.
  Added the workflow lint the brief asked for:
  `the_tap_token_mint_step_pairs_its_action_major_with_its_input_name` in
  `crates/vilan-cli/tests/release_gate.rs` — fails if a future bump repins this action to
  v3+ without carrying `client-id:`. **Owner-owed**: add `TAP_APP_CLIENT_ID` as a repo
  secret (the App's CLIENT ID, not `TAP_APP_ID`'s numeric id) before the next tag.
- **L18** — NOT BUILT, already done. Verified against
  the `vilan-lang.github.io` checkout beside this repo (reachable locally): `origin/main`
  (local main is at the tip) already has commit `fe81d5c` "ci: docs.yml actions SHA-pinned,
  dependabot added (vilan L18)" — both `actions/checkout` calls in `docs.yml` are pinned by
  sha with a version comment, and `.github/dependabot.yml` exists. An OPEN, unmerged
  Dependabot PR (`origin/dependabot/github_actions/actions-7a5a078ad4`, commit `81e5805`)
  bumps `actions/checkout` to v7.0.1 but is not part of this lane's scope. Tracker is stale;
  the item should be closed as already resolved.
- **M84** — LANDED @4e6ebfe0. `min`/`max` moved from each integer type's plain
  inherent `impl T { .. }` into its `impl T with Ord { .. }` block (i8/u8, i16/u16, i32/u32,
  i53/u53, usize — the 9 types that had a host `Math.min`/`Math.max` extern AND an `Ord`
  impl; `BigInt` has `Ord` but no host min/max, unaffected; `f32`/`f64` are not `Ord` and
  keep their own `clamp`). `Ord::clamp`'s default body (`self.min(max).max(min)`) now
  reaches the override instead of falling through to `Ord`'s generic, `compare`-based
  default — `x.clamp(lo, hi)` compiles to `Math.max(Math.min(x, hi), lo)`, zero `compare`
  calls (down from two), same semantics. `math.vl`'s corpus golden shrinks; verified its
  runtime stdout is byte-identical before/after. `vilan fmt vilan/std` run (rewrapped two
  new comments). Gates: corpus green, `vilan-core --test docs` green (11/11),
  `vilan-core --test inference` green (4669/4669), `shared_census` green,
  `native_differential` green both default (61/61) and `VILAN_NATIVE_DIFFERENTIAL=1` (61/61)
  — neither corpus program M84 touches is in `DEFAULT_SUITE`, so no census regen needed.
- **M12** — NOT BUILT, already done. `crates/vilan-lsp/src/document.rs`'s leak soak
  (`soak_corpora`/`soak_refusal`) already fails loudly (`panic!`) when it measures zero
  corpora, exactly per the item's own fix description, with two pins already in place and
  passing: `a_soak_that_measured_no_corpus_refuses_and_names_its_variables` and
  `the_soak_body_refuses_when_every_corpus_is_absent` (the latter drives the real
  `soak_corpora` body through an absent-by-construction corpus, not just the pure
  `soak_refusal` helper — proving the refusal is actually wired in, not just implementable).
  Verified both pass (`cargo test -p vilan-lsp -- soak`, 3 passed, 1 ignored [the real
  multi-hour soak itself, correctly `#[ignore]`d]). Tracker is stale; should be closed as
  already resolved.

## FINDS to file (for the integrator / a future lane)

- **`print`'s negative-zero divergence from `console.log`** (found investigating N106):
  `vilan_rt::print` compiles to `console.log(value.js())` on the native backend, mimicking
  the JS backend's `console.log(x)`. For every value EXCEPT `f64`/`f32` negative zero, this
  matches. Node's `console.log` special-cases `-0` to print `"-0"` (a `util.inspect`
  behaviour, not `ToString`), so JS's own `print(0.0 * -1.0)` also prints `"-0"` — but
  `vilan_rt::js_number` (used for the native backend's rendering) implements JS
  *stringification* semantics (`String(x)`/template-literal/`JSON.stringify`, which all
  answer `"0"` for `-0`), so the native backend prints `"0"`. This is now documented and
  pinned as an intentional, out-of-differential divergence (`f64-print-negative-zero.vl`),
  but whether `print` specifically should special-case `-0` to match `console.log` (making
  the two backends agree at the cost of `js_number` no longer being a pure stringification
  function) is a real design question nobody has ruled on. Small, but not "droppable" —
  it's a genuine three-way semantics question (`print` vs `String` vs `console.log`), not a
  typo fix.
- **L18 and M12 tracker entries are stale** — both already fully resolved in the tree (see
  above); recommend `stamp_items.py`/`close_batch.py` closes them without further work.

## Gates run (this lane)

- `cargo fmt --all` / `--check`: clean throughout.
- `cargo clippy -p vilan-cli --all-targets -- -D warnings`: clean after every item.
- `cargo test -p vilan-cli --test native_differential` (default): green after N133, N106, M84.
- `VILAN_NATIVE_DIFFERENTIAL=1 cargo test -p vilan-cli --test native_differential`: green
  after N133, N106, and M84.
- `cargo test -p vilan-cli --test corpus`: green after N106, M84 (two goldens regenerated:
  `math.mjs`, `number-math.mjs`; runtime stdout verified byte-identical before/after for both).
- `cargo test -p vilan-core --test docs`: green (11/11) after M84.
- `cargo test -p vilan-core --test inference`: green (4669 passed, 2 ignored) after M84.
- `cargo test -p vilan-cli --test shared_census`: green after M84.
- `cargo test -p vilan-cli --test release_gate`: green after L22 (includes the new lint).
- `cargo test -p vilan-lsp -- soak`: green (M12 verification, no code change).
- Final sweep at the end of the lane: `cargo fmt --all --check` and
  `cargo clippy --workspace --all-targets -- -D warnings` (the whole workspace, not just
  `vilan-cli`): both clean. `target/tmp` has zero stale `native-differential-src-*`
  directories after every run in this lane.


# LANE-STATUS — editor-43 (worktree off origin/next @762c6aa5)

- 2026-09-28 13:25 started. Queue: E229 (first) -> E214 pins -> E228 -> E225+K24 -> B422 -> E224 -> E182/E199 if time.
- Observed at start: ~/.vscode-server/extensions now holds vilan-lang.vilan-0.41.1 (installed from a vsix 2026-09-28 13:03, registered in extensions.json) beside 0.40.0 — the owner has applied R-a's one-liner.
- E229 LANDED 57296047 (doors a, b, d; door c: release.yml ALREADY has publish-marketplace/publish-openvsx jobs, gated on AZURE_CLIENT_ID/AZURE_TENANT_ID secrets — the L-item is configuring them, not writing the job).
- FIND (E229 premise): the extension IS on the marketplace — release run 36353347340's publish-marketplace logged 'Published vilan-lang.vilan v0.41.1'; the gallery lists 0.41.1/0.41.0/0.40.0 (Open VSX published too). Door (c) is DONE. The owner's installs are from vsix, which VS Code PINS (extensions.json: "pinned": true, "source": "vsix") — a pinned extension never auto-updates. So the notice's command is now `code --install-extension vilan-lang.vilan@<version> --force`. OWNER Q (not blocking): should the installers install by gallery id (auto-updating, unpinned) instead of the verified vsix (pinned, exactly the toolchain's version)? Built as the item says (vsix).
- E214 pins LANDED e82ae72a (closes at the owner's word).
- E228 LANDED 164f2733 (two causes: external returns unsubstituted; the live walk declined a bare T — now grounded from the receiver).
- E225+K24 LANDED 502e12fe (keyword_table + bindgen + vilan --print-keywords; also fixed: free functions were never escaped). Site diff (owner's): scratchpad/editor-43/k24-site.diff. Seam for syntax-43: consumers read vilan_core::keyword_table (reads lexing::KEYWORDS).
- B422 LANDED f25fce11 (key carries canonical std roots; plain cargo test -p vilan-lsp: no references:: panics, the 2 known N131 load pins still red under cargo test only). NOTE: touches analyzer.rs (BaseCacheKey only) — solver-43's file.
- E224 STOPPED (OWNER Q unanswered since Order 42's std-42 stop): deprecation.md line 356 ('An import line alone does not warn … the behavior we want') and the shipped pin generics::a_std_marked_item_warns_at_its_use contradict the brief's direction (function import leaves warn); the paper itself also says at line 87 'the failing segment at an import' is an anchor. Needs the owner: (a) function import leaves warn (reverse line 356 + the pin), or (b) type import leaves stop warning (reverse E221/B382's leaf warnings). std-42's held patch (a) is at its scratchpad; not rebuilt here.
- E182 NOT BUILT: premise still latent — std/vilan.toml has two layers (process, browser); no third `std::ui` twin exists until F1 S2's native layer, which the item says to sequence with.
- E199 STOPPED: no repro. The message's type (`StorageSignalCell`) no longer exists in std; eight probe shapes over `swap(SignalCell<bool>, …)` (bodies of the wrong type, parameters annotated `str`) all name the SOURCE's `T` correctly (`Swap<bool, SignalCell<bool>, i32>`). FIND: an annotated parameter that contradicts the source (`swap(flag, |on: str| 42)`) reports the real mismatch AND a spurious first error, "cannot infer 'C' for this call; its bound ': Slot' cannot be checked".
- Seam with syntax-43 read from their LANE-STATUS: they adopt keyword_table/--print-keywords at rebase, add "contextual", and make is_keyword RESERVED-only — so tests/bindgen.rs e225 + keyword_table's css/dyn/lazy asserts + reserved_tests need their update for dyn/lazy (contextual after B414), and `Self` breaks every_spelling_is_a_plain_identifier (uppercase).
- 2026-09-28 all items done or stopped; writing REPORT-editor-43.md.
- REPORT committed 6a572a6b.
