# REPORT — hygiene-43

Worktree: `vilan/.claude/worktrees/hygiene-43`, branch `hygiene-43`, off `origin/next` @762c6aa5.
Four commits ahead of base (`762c6aa5..HEAD`), oldest first:

1. `3e2c750f` — N133
2. `0816332e` — N106
3. `1946f746` — L22
4. `4e6ebfe0` — M84

L18 and M12 needed no commit — both were already fully resolved on the tip (see below).

## Per item

### N133 — LANDED @3e2c750f

`native_differential`'s per-test staging directory (`target/tmp/native-differential-src-<pid>-<counter>`)
was never removed (2,788 stale copies of the corpus measured after one day of Order 42's
runs), and CI's `vilan-fmt` leg, walking `.` from the repository root, reached them and
declined 5,556 stale `resource struct` files (a construct retired before B413).

Two fixes, both general rather than special-cased to this one test binary:

- `StagedDir` (`crates/vilan-cli/tests/native_differential.rs`) wraps the staged `PathBuf`
  and removes it on `Drop` — gone whether the test passed, failed, or panicked. `stage()`
  now returns `StagedDir` (`Deref<Target = Path>` + `AsRef<Path>`); all ~60 call sites needed
  no change beyond that.
- `vilan fmt`'s (and the file watcher's) `TreeWalk::walk` (`crates/vilan-cli/src/main.rs`)
  now skips any directory carrying a `CACHEDIR.TAG` file, per the Cache Directory Tagging
  Specification (the same signature cargo itself writes into `target/`). This is
  name-independent — `native_differential`'s own shared `CARGO_TARGET_DIR` is spelled
  `native-differential`, not `target`, so a check keyed on the literal name `target` would
  have missed exactly the directory this item is about.

Gates: `cargo test -p vilan-cli --test native_differential` and the same under
`VILAN_NATIVE_DIFFERENTIAL=1`, both green; confirmed zero stale
`native-differential-src-*` directories left in `target/tmp` after every run for the rest
of the lane.

### N106 — LANDED @0816332e

Verified the premise in three parts rather than assuming it:

- **The four `f64` printing boundaries** (`1e21`'s exponential switch, the `1e-6` floor,
  the two infinities): `vilan_rt::js_number` already implemented these correctly, but no
  corpus program printed any of them, so nothing caught a regression in the JS *emitter's*
  own arithmetic there. Added `f64-print-boundary.vl` to `DEFAULT_SUITE` — identical on
  both backends, confirmed.
- **Non-BMP `str::len`**: the item's premise (`str_len` "matches JS only for the BMP") is
  **false** on the current tree — `char::len_utf16`, which `str_len` already sums, gives 2
  for a supplementary-plane character exactly as JS's UTF-16 `.length` does. Verified with
  `"a😀b".len()` on both backends (4 both sides). Added `non-bmp-string-length.vl` to
  `DEFAULT_SUITE` and corrected the stale doc comment on `str_len`.
- **Negative zero**: found a **real, previously undocumented divergence**. `print(0f * -1f)`
  prints `"-0"` on the JS backend (node's `console.log` special-cases negative zero via
  `util.inspect`) but `"0"` on the native backend (`vilan_rt::js_number` matches
  `String(x)`/template-literal/`JSON.stringify` semantics, which all answer `"0"` — not
  `console.log`'s). This is documented at both printing sites (`print`'s and `js_number`'s
  doc comments in `crates/vilan-rt/src/lib.rs`) and pinned **outside** the native
  differential by name — `f64-print-negative-zero.vl`, added to `OUTSIDE_THE_DIFFERENTIAL`
  with a new test, `negative_zero_prints_differently_on_each_backend_by_name`, mirroring
  `usize-underflow.vl`'s existing pattern exactly.

Gates: `native_differential` default and `VILAN_NATIVE_DIFFERENTIAL=1`, both green (61/61).

**FIND** (not fixed, filed below): whether `print` should special-case `-0` to match
`console.log` is a real, unruled design question.

### L22 — LANDED @1946f746

Confirmed live via `gh api` (network reachable in this environment) rather than trusting
the item's account: `actions/create-github-app-token`'s `app-id` input was deprecated at
v3.1.0 (2026-04-11, `gh api repos/.../releases/tags/v3.1.0` — "add `client-id` input and
deprecate `app-id`"), current release v3.2.0 (2026-05-12), sha
`bcd2ba49218906704ab6c1aa796996da409d3eb1` (`gh api repos/.../tags`).

`.github/workflows/release.yml`'s `publish-brew` job: the mint step's pin moved to v3.2.0;
both the mint step and its gate ("Is there an app to push with?") now read a new secret,
`TAP_APP_CLIENT_ID`, and pass `client-id:` instead of `app-id:`. The gate's notice text
was renamed to match and now explains the CLIENT ID vs numeric ID distinction.

Also built "the workflow lint that would have caught it" the brief asked for — no such
lint existed anywhere in the tree (checked for it first). Added
`the_tap_token_mint_step_pairs_its_action_major_with_its_input_name` to
`crates/vilan-cli/tests/release_gate.rs`: it reads the pinned version comment on the mint
step's `actions/create-github-app-token` line, and if the major is 3 or later, asserts the
step's `with:` block carries `client-id:` and not `app-id:`. Verified non-vacuous by
temporarily reverting to `app-id:`/`TAP_APP_ID` and confirming the new test goes red, then
restoring (CLAUDE.md's "prove a new pin is non-vacuous" rule) — **note for the reviewer**:
this sanity check used `git checkout -- <file>` at one point to revert a scratch edit, which
(correctly, since the file had other uncommitted changes at the time) discarded the L22
edits entirely; they were redone from a fresh `Read` immediately after and the final diff
was verified intact before committing. No data was lost past that one recovery, but it is
the one destructive-command near-miss in this lane and is called out per the instructions
to flag anything the reviewer should check.

**Owner-owed**: `TAP_APP_CLIENT_ID` needs to exist as a repository secret (the App's
CLIENT ID — the `Iv…` string on its settings page, not the numeric ID `TAP_APP_ID` holds)
before the next tag, or `publish-brew`'s gate reads `publish=false` and the tap silently
stops updating (same behavior as today when `TAP_APP_ID`/`TAP_APP_PRIVATE_KEY` are unset —
not a regression, just unchanged until the owner adds the new secret). `TAP_APP_ID` itself
stays configured (harmless, unread) until a release has gone out on the new input, per the
item's own step 3.

Gates: `cargo test -p vilan-cli --test release_gate` (4/4, including the new lint).

### L18 — NOT BUILT, already resolved on the tip

The pages repo (`/home/reed/code/vilan-lang/vilan-lang.github.io`) is reachable locally.
`origin/main` — which the local checkout's `main` already sits at — carries commit
`fe81d5c` ("ci: docs.yml actions SHA-pinned, dependabot added (vilan L18)"): both
`actions/checkout` calls in `.github/workflows/docs.yml` are pinned by full sha with a
version comment, and `.github/dependabot.yml` exists (weekly, grouped, the same shape as
vilan's own). There is a separate, **unmerged** Dependabot PR branch
(`origin/dependabot/github_actions/actions-7a5a078ad4`, commit `81e5805`, bumping
`actions/checkout` to v7.0.1) sitting open — not part of this lane's scope, and not
something hygiene-43 should merge unreviewed.

**Recommend closing L18** — the tracker is stale.

### M84 — LANDED @4e6ebfe0

Verified the mechanism precisely before writing anything, with a probe (`vilan build` +
inspecting the emitted `.mjs`): `Ord::clamp`'s default body (`self.min(max).max(min)`,
`vilan/std/src/compare.vl`) was reaching `Ord`'s own generic, `compare`-based `min`/`max`
for every integer width — **two** `compare()` calls per `clamp` — even though every
integer type already declared a `Math.min`/`Math.max` extern under the same name. The
reason: that extern sat in the type's plain inherent `impl T { .. }` block, and a trait
default method body only reaches an **override** declared inside `impl T with Ord { .. }`
— an inherent method of the same name outside that block is invisible to it (this is R1's
own rule, working as designed; the bug was that the externs were never placed where R1
could see them).

Fix: moved `min`/`max` from the plain inherent block into the `impl T with Ord { .. }`
block, for the nine integer types that have both a host extern and an `Ord` impl — `i8`,
`u8`, `i16`, `u16`, `i32`, `u32`, `i53`, `u53`, `usize` (`BigInt` has `Ord` but no host
min/max and is unaffected; `f32`/`f64` are not `Ord` — `NaN` — and keep their own
`clamp`). Confirmed via the same probe: `x.clamp(lo, hi)` now compiles to
`Math.max(Math.min(x, hi), lo)`, zero `compare` calls. Direct `.min()`/`.max()` calls
still resolve correctly (probed both backends). `vilan/std/src/number.vl`'s existing
`compare()` bodies are untouched.

Two corpus goldens moved (`math.mjs`, `number-math.mjs`, both use `.clamp()`/`.min()`/
`.max()` on integers) — regenerated with `vilan build`, and in both cases the runtime
`stdout` was diffed byte-for-byte before and after the change and found identical; only
the emitted JS shrank.

Gates: `cargo test -p vilan-cli --test corpus` (12/12), `cargo test -p vilan-core --test
docs` (11/11), `cargo test -p vilan-core --test inference` (4669/4669, 2 pre-existing
ignores), `cargo test -p vilan-cli --test shared_census` (2/2), `native_differential`
default and `VILAN_NATIVE_DIFFERENTIAL=1` (61/61 both — neither corpus program M84 touches
is in `DEFAULT_SUITE`, so no census regeneration was needed), `vilan fmt vilan/std` run
clean (only rewrapped two new comments).

### M12 — NOT BUILT, already resolved on the tip

`crates/vilan-lsp/src/document.rs`'s leak soak already does exactly what M12 asks:
`soak_corpora` counts `measured_corpora` and, at the end, calls `soak_refusal`, which
`panic!`s naming every missing env var when nothing was measured (`measured_corpora == 0`)
rather than passing vacuously. Two tests already pin this and both pass:
`a_soak_that_measured_no_corpus_refuses_and_names_its_variables` (the pure `soak_refusal`
logic) and — more importantly — `the_soak_body_refuses_when_every_corpus_is_absent`, which
drives the real `soak_corpora` body through a corpus addressed by a variable nothing sets
and asserts the body itself panics with the composed message, not just the helper in
isolation. Ran `cargo test -p vilan-lsp -- soak`: 3 passed, 1 correctly `#[ignore]`d (the
real multi-hour soak).

**Recommend closing M12** — the tracker is stale.

## FINDS to file

1. **`print`'s negative-zero divergence from `console.log`** (N106). `vilan_rt::print`
   renders through `js_number`, which implements JS *stringification* semantics
   (`String(x)`/template-literal/`JSON.stringify` — all answer `"0"` for negative zero).
   But `print` on the JS backend compiles to `console.log(x)`, and node's `console.log`
   special-cases negative zero to print `"-0"` (a `util.inspect` behavior distinct from
   `ToString`). This is now documented and pinned as an intentional divergence
   (`f64-print-negative-zero.vl`, outside the native differential by name), but nobody has
   ruled on whether `print` specifically should special-case `-0` to match `console.log`.
   That would make the two backends agree at the cost of `js_number` no longer being a
   pure stringification function used elsewhere (interpolation, `Display`). Sized S-M; a
   real semantics question, not droppable.
2. **L18** — already resolved on `vilan-lang.github.io`'s `main` (`fe81d5c`); tracker
   should close without further work.
3. **M12** — already resolved in `crates/vilan-lsp/src/document.rs`; tracker should close
   without further work.

## Gates (summary)

- `cargo fmt --all` / `--check`: clean throughout, and re-checked over the whole tree at
  the end.
- `cargo clippy -p vilan-cli --all-targets -- -D warnings`: clean after every item; also
  re-run as `cargo clippy --workspace --all-targets -- -D warnings` at the end (clean).
- `cargo test -p vilan-cli --test native_differential`, default and
  `VILAN_NATIVE_DIFFERENTIAL=1`: green throughout (61/61 by the end of the lane).
- `cargo test -p vilan-cli --test corpus`: green (12/12) after N106 and M84's golden moves.
- `cargo test -p vilan-core --test docs`: green (11/11).
- `cargo test -p vilan-core --test inference`: green (4669/4669, 2 pre-existing ignores).
- `cargo test -p vilan-cli --test shared_census`: green.
- `cargo test -p vilan-cli --test release_gate`: green (4/4, including the new L22 lint).
- `cargo test -p vilan-lsp -- soak`: green (M12 verification only, no code change).
- `target/tmp` swept clean of stale `native-differential-src-*` directories after every
  run in this lane (N133's own fix, exercised live).

## Expected conflicts at merge

- `CHANGELOG.md`'s `## Unreleased` section: this lane created it (first to land, per the
  brief). Every other Order 43 lane appends its own entries under the same head — textual
  merge, not a real conflict, but worth the integrator's eyes per briefs43's own lesson
  about two lanes bumping one place in a docs page.
- `vilan/std/src/number.vl` (M84): solver-43 and tuples-43 own analyzer.rs/mono.rs/
  transformer.rs, not std — no expected overlap, but any other lane touching
  `number.vl`'s `Ord`/min/max/clamp block will conflict textually.
- No other files this lane touched (`crates/vilan-cli/tests/native_differential.rs`,
  `crates/vilan-cli/src/main.rs`, `crates/vilan-rt/src/lib.rs`,
  `crates/vilan-cli/tests/release_gate.rs`, `.github/workflows/release.yml`, the six new
  `vilan/test/*.vl`/`*.mjs` files, `crates/vilan-cli/tests/native-copy-census.tsv`,
  `crates/vilan-cli/tests/native-leak-census.tsv`) are named as owned by another Order 43
  lane in the brief's ownership map.
