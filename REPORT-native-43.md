# REPORT — lane native-43 (Order 43)

Branch `native-43`, worktree `vilan/.claude/worktrees/native-43`, base origin/next 762c6aa5.
Seven commits (six items + this report). Every item's premise was re-verified on the base first.

## Items

| item | verdict | sha | pins (native differential unless noted) |
|---|---|---|---|
| F41 | LANDED | 8be0eac5 | `a_field_named_by_a_path_keyword_builds_the_same_on_both_backends` |
| F42 | LANDED | ed91b4d5 | `a_keyed_cell_builds_and_journals_the_same_on_both_backends` |
| F44 | LANDED | 7674feff | `a_closure_stored_in_what_it_reads_builds_the_same_on_both_backends` |
| F46 | LANDED | 3b8e4aab | `an_argument_handing_on_the_borrowed_receiver_copies_it_on_both_backends` |
| F45 | LANDED | e9a394db | `a_stopped_server_ends_the_program_the_same_on_both_backends`, `a_native_server_stops_on_sigterm_and_reaches_its_process_end` (unix), `a_second_termination_signal_ends_a_server_whose_stream_never_closes` (unix), vilan-rt `a_termination_closes_the_waiting_and_lets_the_answering_finish`, the embedding pin |
| F47 | LANDED (coordinator add, UNSOUND) | 76a873c8 | `a_captured_mut_parameter_is_shared_with_its_closure_on_both_backends` |
| census | DONE, no commit | — | copy census regenerated identical; leak census unmoved |

Every pin was shown red first: planted-out fix (or the pre-change toolchain) → red, restored → green.

- **F41** — premise confirmed (`r#self`/`r#super`, rustc refuses). `sanitize` now mangles the four PATH
  keywords (`self`, `Self`, `super`, `crate`) with a trailing `_`, and any name that is one of them plus
  only underscores gets one more `_` (injective: `self` and `self_` coexist). Every field site emits
  through `sanitize`, so the read-back needs no table; no output moves (structs render positionally).
- **F42** — premise corrected: NOT B406 (landed; analyzer is right). The native emitter rendered
  `Shared::new`'s argument under the expectation for the intrinsic's RESULT, so `Map::new()` matched
  `Map<K,V>` against `Shared<Map<..>>` and closed nothing. Each intrinsic argument now takes the
  expectation its intrinsic gives it (the `Shared`'s element for `Shared::new`; nothing otherwise).
  Also probed: reactive_channels' `KEYED_CELL_AGAINST_THE_DIFF` and `GENERATED_KEYED_CELL` are now
  byte-identical natively (not pinned in native_differential; they are reactive_channels' programs).
- **F44** — premise confirmed ("cyclic type of infinite size"; a second closure: "a different closure").
  Root: `Shared::new([])` gave inference nothing, so the first closure's anonymous type became the
  element type. Every closure literal is now `Rc::new(move |..| ..) as Rc<dyn Fn(<literal's param
  types>) -> _>` — the type every closure type already renders as. `parameter_declaration` split into
  `parameter_parts` so the signature and the declaration share one rendering.
- **F46** — premise half right: evaluating the argument first alone still moves the binding the
  receiver then borrows. The liveness pass now walks a LOANED bare-place argument (receiver included)
  after the call's other arguments, so the by-value read copies; F35's hoist still evaluates a `&mut`
  receiver's argument first. **Fold: NOT folded** — `subscribe_pulling`'s one-statement form was
  verified over a scratch std (reactive-on-change, reactive-flatten, dyn-objects, reactive-selector all
  identical with node; E0505 on the base toolchain), but reactive.vl is reactive-43's file. It is a
  3-line fold for reactive-43 or the integrator.
- **F45** (R-i, build) — premise corrected in part: `Server::stop()` already ended a native server
  program (now pinned). Built: `vilan_rt::http::request_termination()` (dependency-free, safe from any
  thread): first request = every server stops accepting, closes waiting + upgraded connections (their
  `close` subscribers run), lets in-flight requests finish, then the loop ends and `main` returns;
  second request = exit 1 with a sentence. The OS wire needs FFI or a crate, so it lives in a THIRD
  optional crate `crates/vilan-rt-signal` (`forbid(unsafe_code)`, over `ctrlc` with `termination` —
  already in the lockfile for the CLI, so no new third-party crate; THIRD-PARTY-NOTICES regenerated:
  one `Covers:` line). Generated manifest names it only when the program lowers `createServer`; the
  emitted `main` calls `vilan_rt_signal::install()` first. Embedding (build.rs, lib.rs, test),
  AGENTS.md map, native guide updated. **Find fixed with it:** `listen` ran `on_ready` synchronously
  inside `start()`; node emits `'listening'` from nextTick, so `server.start(); print("main returned")`
  printed in the opposite order natively. It is a microtask now (cannot race a request: accepts happen
  only in the I/O poll, after microtasks drain).
  Kolt's REAL server (scratch copy, port patched to 0, client leg built): GET / 200, SIGTERM → exit 0,
  census `cells minted=3 live=0`. The kolt SHAPE's exit census (`minted=1 live=0`) is held by the pin.
- **F47** — premise confirmed (p11: rustc E0525; p12: `0` vs `5`). A by-value `mut` parameter a
  closure captures joins `compute_boxed_bindings` and is re-bound into its `Captured` cell on entry
  (functions and closures, `boxed_parameter_prologue`). A closure's own parameters are now seeded as
  declared-inside in that scan — without the seed `|mut v| { v = v - 1; v }` boxed itself
  (mut-parameters.vl leak row went 0→1; with it, unmoved).

CHANGELOG: a fresh `## Unreleased` above `## v0.41.1`, six entries — F41 `fix`, F42 `fix`, F44 `fix`,
F46 `fix`, F45 `feature`, F47 `miscompile`. No diagnostic changed: no ledger rows (block 640–649 unused).

## Census deltas

- Native copy census (`VILAN_REGENERATE_NATIVE_COPY_CENSUS=1`, tip 76a873c8): **no delta** — the
  regenerated table is byte-identical to the committed one (F46 only adds copies where rustc refused).
- Native leak census (re-run on the tip): **no delta** (reactive-43's A132 is not in this tree; the
  re-run after it merges is the integrator's). New server exit line: kolt shape `minted=1 live=0`
  (pinned); kolt's real server `minted=3 live=0` (probed, scratch).
- Whole platform-free set: **125 enumerated / 94 identical / 31 refused by name / 0 broken** on the
  tip; the base toolchain builds the same 94 (no corpus program flipped — the lane's wins are shapes
  the corpus does not carry, now pinned). Async set not changed.

## Gates (tip 76a873c8, each also before its own commit)

- `cargo fmt` clean; `cargo clippy --workspace --all-targets -- -D warnings` 0.
- `-p vilan-cli --test corpus/split/examples/copy_elision_census`: ok (2/12/6/13).
- `-p vilan-cli --test native_differential` default: 68 passed; `VILAN_NATIVE_DIFFERENTIAL=1` whole
  set: 68 passed (the leak and copy census tests included).
- `cargo test --workspace --doc`: ok. vilan-rt 80, vilan-rt-signal/-crypto/embedded units ok;
  `agents_map`, `third_party_notices`, `release_gate`, `release_scripts`, `docs` (vilan-core),
  `book_mirrors`: ok.
- Kolt `src/server.vl` (scratch copy) builds natively at every commit (`vilan build --backend rust`
  exit 0; its manifest now names vilan-rt-sqlite, -crypto and -signal).
- `#[cfg(unix)]`: the two SIGTERM pins run here and are compiled-out on Windows (no `kill`); nothing
  in them is Windows-only.

## Finds to file

1. **native — a captured binding READ AS A CLOSURE'S VALUE moves out of an `Fn` closure (rustc E0507)**:
   the closure body tail and a `match` leg are not consuming positions, so `copy_a_consumed_place_read`
   never sees them. Blocks `std::rpc`'s `KeyedSource::or` natively (reactive_channels'
   `KEYED_MIRROR_IS_A_SOURCE`). Repro:
   ```text
   import std::io::print;
   fun plain(initial: List<i32>): || List<i32> { || initial }
   fun fallback(initial: List<i32>): |Option<List<i32>>| List<i32> {
   	|value| { match value { Some(let present) => present, None => initial } }
   }
   fun main() { let again = plain([4, 5, 6]); print(again().len() + again().len());
   	let pick = fallback([1, 2]); print(pick(None).len()); }
   ```
   (JS prints `6` then `2`; native: E0507 twice.) Sizing S, native.
2. **analyzer — a closure whose `&mut` view is its PARAMETER is refused as capturing a view when
   pushed**: `mut edits: List<|&mut List<i32>| void> = []; edits.push(|&mut list| list.push(7));` →
   "a view cannot escape its scope: this closure captures one, and `push` keeps what it is handed…" —
   the closure captures nothing; its message even suggests the parameter form it already uses. Both
   backends refuse it (it is the analyzer's). Sizing S, solver.

## Owner/integrator notes

- F45's dependency choice (ctrlc in a third optional crate) was made by F40 (a)'s precedent, not ruled
  specifically: a native SERVER build now fetches `ctrlc` (→ nix / windows-sys) the first time; and in
  WORKSPACE builds cargo unifies `ctrlc`'s `termination` feature into vilan-cli, so the CLI's watch
  hook also runs on SIGTERM/SIGHUP there (release builds are `-p vilan-cli -p vilan-lsp`: unaffected).
- A SIGTERM stop does not fire the vilan-level `on_stop` (the runtime never sees it unless `stop()` was
  called); a program with a live repeating timer does not reach its end on the first signal (the second
  ends it). Both are in the native guide's wording by implication; say so if they should be explicit.
- Count words: AGENTS.md "ten crates" → "eleven crates" (agents_map gate). The native guide gained a
  paragraph, no count word. No quick fix / code action added.

## Expected conflicts

- `CHANGELOG.md` — the only textual conflict against today's origin/next (013b3860, after hygiene-43
  and editor-43): both append to `## Unreleased`; keep every entry.
- `crates/vilan-cli/tests/native_differential.rs` — auto-merges against hygiene-43's staging cleanup
  and solver-43's pins (checked with `git merge-tree`); the native copy census should be regenerated
  over the merged tree as usual.
- `crates/vilan-rt/src/lib.rs` is untouched here (hygiene-43 edits it); `http.rs` is mine alone.
- `LANE-STATUS.md` is left UNTRACKED in the worktree (editor-43's is already on next; committing mine
  would add/add-conflict).
