# editor-44 report (Order 44)

Branch `editor-44`, rebased onto `origin/next` @6a8405ce (solver-44, native-44, reactive-44,
syntax-44, transient-44, collections-44 and tracking-44 merged). One commit per item, each with its CHANGELOG entry under `## Unreleased`.
Nothing was pushed. The proposals repo and kolt were not written. `LANE-STATUS.md` is untracked.

## Items

| Item | State | sha | Premise |
|---|---|---|---|
| E224 (R-j) | LANDED | 7e2860f6 | Held, and wider than filed: `[lints] internal_use` also warned at the import line of internal FUNCTIONS. Both lints now warn at uses only. |
| E231 | LANDED | 26161539 | Held: `SERVER_VERSION` was the bare crate version. |
| E230 | LANDED | b92a0aed | Held for the upgrade path. Not verifiable here (no VS Code): that an unversioned gallery install replaces a vsix-pinned one and unpins it. |
| E232 (door 2) | LANDED | fd8a9e37 | Held: every hint inside the anchor's line-trimmed window was withheld. |
| F27 R3 editor routing | LANDED | ac84c89f | Partly false: syntax-43 reported LSP twin legs for bare files; the LSP's no-project path added none. Fixed here. |
| E227 (R-d), phases 1 + 2 | LANDED | f76e6094 | Held. `RemoteSource`'s brief spelling is corrected (below). |

## E224 — an import line alone never warns

- `labels.rs` skips every name row inside an `import`/`use` statement (`Program::import_statement_spans`,
  recorded at the analyzer's Import/Use arms), for `[deprecated]` and for `[lints] internal_use`.
- A deprecated RE-EXPORT still warns at the import that reaches through it (`check_deprecated_reexports`):
  its renamed name is transparent everywhere else, so that import is its only use.
- Docs: grammar.md, cli.md. OWED (integrator): `deprecation.md` line 87.

## E231 — `version (sha)`

- `crates/vilan-cli/build_stamp.rs` is shared by both build scripts (new `crates/vilan-lsp/build.rs`).
- install-dev.sh and release.yml's `vsix` job write `build-sha.txt` before `vsce package`.
- `versionGap` compares commits when the versions agree and both sides carry one.
- Find, fixed: the stamp never re-ran in a LINKED worktree. `build_stamp.rs` resolves gitdir/commondir.

## E230 — `vilan upgrade` brings the editor

- It runs after a swap, and when the toolchain is newest but the installed extension is not its version.
- Gallery id first (`--force`); the release's `vilan-vscode.vsix`, checksum-verified, when the gallery
  is unreachable.
- `--no-vscode` / `VILAN_NO_VSCODE` opt out; `--check` never touches the editor; a refusal is reported,
  never a failed upgrade.
- `install.sh` and `install.ps1` install by gallery id first too. `install.ps1` is not run by anything
  in the tree.

## E232 — hints behave on the edited line

- `LandedHint` keeps the binding name. Hints follow the edit log, or the byte-exact differing region when
  there is no log (`EditTrail`, `follow_edit`).
- A hint drops only when its whole name is replaced, or its point falls inside replaced bytes.
- Limit: with no log, two far-apart edit sites read as one region, and hints between them drop until
  the analysis lands.

## F27 R3 — editor routing (platform-coloring.md §8.4 items 2–4)

- Files with twins keep their further legs' analyses (`TwinLeg`); other files still drop them.
- `answering` routes hover and completion; `definitions` answers every leg's definition, the answering
  leg's first; `capture_landed` takes the admitting leg's hints and tokens inside a fenced-out twin.
- The legs follow the live text and go with `release_analysis`.
- A bare file's twins now add legs in the editor, as `vilan check` does.
- Item 3 (gray) holds by construction; no package-union gray pin.

## E227 — `[hint(Trait<..>)]`

- **Parser.** `[hint(type)]` sits between `[internal]` and `[platform]`. `Labels::hint` is a
  `Vec<HintArgument>` (an `Arc`: `Node` is not `Clone`). `hint` joins `KNOWN_ATTRIBUTE_MARKERS`
  (grammars regenerated, spec lists, grammar.md). The formatter reprints it.
- **Analyzer** (`crates/vilan-core/src/analyzer/hint_labels.rs`). Declaration checks refuse a
  non-trait argument, an application no impl provides (structural), and a second hint; resolution
  refuses a free name; `labels.rs` refuses a hint on a trait, binding or impl.
- **Rendering.** `~Trait<args>` per variable, recursive, admitted per instantiation. The solver's
  `type_implements_trait_at` and `impl_bounds_hold` answer "true" when undecided, so `hint_bounds_hold`
  adds a positive check of every binder's bound.
- **LSP.** `vilan.inlayHints.abbreviate` (default on, live, with a refresh on change); the full type as
  tooltip; never `text_edits`; hover adds "Shown as … (`[hint]` on `Host`)".
- **std (phase 2, reactive-44's names).**
  - `[hint(Pipe<..>)]` on `Derive`, `Switch`, `SwitchSome`, `AndThen`, `ThenSome`, `Combine`,
    `Distinct` and `DistinctBy`.
  - `[hint(Source<T>)]` on `MemoCell` (`.cell()`'s `SignalCell` stays a leaf, per the paper's §5.3).
  - After tracking-44 and collections-44: `[hint(Pipe<T>)]` on `TrackedDerive` and `[hint(Pipe<U>)]` on
    `CollTally`; `[hint(CollPipe<..>)]` on `Coll`, `CollBy`, `CollFilter` and `CollFlatten`;
    `[hint(CollSource<T>)]` on `ListMemo`, the collection seal. `CollSource` rather than `Source<List<T>>`
    keeps the delta capability visible. NOT hintable: `CollMap` and `CollFilterMap`. Their element type
    `U` exists only as a binder inside the impl's bound (`type R: IntoElement<type U>`), not as a struct
    parameter, so no `[hint(..)]` can name it; they render whole.
  - `[hint(TransientSource<T, E>)]` on `Transient`, `.transient()`'s seal (transient-44). It is the
    trait that carries `state()`, `latest()` and `is_pending()`; `~Source<Option<T>>` would hide them.
    `TaskSource` stays whole, as a named leaf.
  - `RemoteSource`: `[hint(Source<Option<T>>)]`, not the brief's `Source<T>`, which check 4 refuses.
  - The eight iterator adapters.
- **The fold.** The phase-1 attributes sat on nodes reactive-44 renamed. reactive.vl was taken whole
  from next, and the pins re-pinned: `: ~Pipe<Option<str>>`, `(~Pipe<Option<str>>, i32)`, hover
  "on `Derive`", a new sealed-memo pin `: ~Source<(List<str>, usize)>`, and a transient pin
  `: ~TransientSource<i32, str>`, and a tracked/collection pin (`~Pipe<i32>`, `~CollPipe<i32>`,
  `~CollSource<i32>`).
- **Limit.** Types inside a closure with a `context` clause are not abbreviated.

## Ledger rows (all `NEW`, all E227)

1. `` `{host_name}` carries more than one `[hint(..)]`: … ``
2. `` `[hint({shown})]` must name a trait application — … ``
3. `` `[hint({shown})]`: no impl of `{shown}` for `{host}` — … ``
4. `` `[hint(..)]` names the trait a struct or an enum is shown as in an inlay hint, and {kind} is not a type — … ``

## Goldens

None moved.

## Gates (f76e6094 over next @6a8405ce)

- `cargo fmt --all --check`: clean.
- `cargo clippy --workspace --all-targets -D warnings`: clean.
- `cargo nextest run --workspace`: 9151/9151 passed, 41 skipped. This covers the vilan-lsp suite,
  vscode_extension (with the extension's own `npm test`), release_scripts, diagnostics_ledger,
  corpus, split, examples, docs, native_differential (default) and check_scope_differential.
- `VILAN_NATIVE_DIFFERENTIAL=1` native_differential: 93/93.
- Windows-msvc clippy over vilan-core, vilan-cli, vilan-lsp and vilan-embedded: clean. The
  whole-workspace msvc run cannot build `libsqlite3-sys` on this box, which is environmental.

## Finds not fixed

- E232: with no edit log, hints between two far-apart edit sites drop until the analysis lands.
- E230: whether a gallery install unpins a vsix install is unverified on a live VS Code.
- E227: context-clause closures are not abbreviated inside.
- Incident (mine): while stopping my own stale suite I ran `pkill -f "cargo nextest run --workspace"`,
  which also matched transient-44's `cargo` parent process. Its `cargo-nextest` child kept running and
  logging, but the exit code that lane read at that moment was the signal, not the suite's. The
  integrator was told at the time. I kill by PID only since.

## Owner questions

None.
