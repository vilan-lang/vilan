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
  `/home/reed/code/vilan-lang/vilan-lang.github.io` (reachable locally): `origin/main`
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
