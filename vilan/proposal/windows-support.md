# Windows support — first-class native Windows for the toolchain

> **Status: RATIFIED 2026-07-24.** All §10 calls settled by the user:
> (a) **normalize, not disallow** — the disallow question was investigated
> and answered by evidence: the only multi-line single-quoted strings in the
> tree (and in kolt) are multi-line **i-strings**, the macro-authoring idiom
> (corpus `macro-derive.vl`, compile-gated docs, macro-engine proposal);
> disallowing cleanly needs H7 (`i"""`) first, and a plain-only ban would be
> an inconsistent rule. §2's normalization applies uniformly to plain and
> i-strings. Revisit disallowing when H7 ships (noted on H7).
> (b) fmt converts to LF; (c) case-exact resolution enforced; (d)
> `windows-sys` accepted; (e) in-process SHA-256 accepted; (f) errors move
> to stderr; (g) PR CI introduced; (h) self-upgrade in v1.
> **Dependency latitude (user, 2026-07-24): cross-platform helper crates are
> welcome where they beat hand-rolling** — e.g. `windows-sys` (Job objects +
> VT), `sha2` (checksums), `dunce` (verbatim-prefix stripping in §5's
> canonical helper). Implementer's call per crate, named in the slice record.
>
> Original status: DRAFT 2026-07-24 — for review. Supersedes the release system's
> recorded decision (`releases.md` §"Windows": *"WSL-only at first,
> documented. Native binaries wait for someone who can verify them"*) per the
> user's call 2026-07-24. Sequenced deliberately **before the F7+F5
> distribution slice**: an npm-installable toolchain that breaks on Windows
> is worse than none, and the verifier the old decision waited for exists —
> the dev machine is WSL2, so a native Windows host is available for the
> live checks CI can't run.
>
> Grounded in a two-lane audit (2026-07-24): a code-level sweep of the Rust
> sources and a infrastructure sweep of tests/CI/release/extension. Every
> claim below carries file:line evidence from those sweeps; items marked
> **probe-verified** were executed, the rest are read-from-source plus known
> Windows semantics and get converted to fact by S0's Windows CI run.

## 0. The audit's shape

**The compiler is the portable part.** `vilan-core` builds paths with
`PathBuf::join` throughout (zero `format!("{}/…")`, zero `.split('/')`, zero
`MAIN_SEPARATOR`), module resolution never splits on separators, and —
probe-verified on `examples/walkthrough` — **no filesystem path is embedded
in emitted JS** (bundles carry only `node:` specifiers), so the byte-identical
golden gate is separator-safe by construction. The lexer already tolerates
CRLF for ordinary code (probe-verified: LF and CRLF copies of `bool.vl` emit
byte-identical JS with correct spans). Emitted JS is LF on every platform
(`join("\n")` + `fs::write`, no translation) — correct, but unpinned.

The breakage concentrates in five places:

1. **One silent-miscompile class, and it is a line-ending bug**: a plain
   `"…"` string literal may span lines, and a CRLF source keeps the `\r` in
   the string's *value* (probe-verified: `"alpha\nbeta"` vs
   `"alpha\r\nbeta"` from the same program). §2.
2. **The repo has no `.gitattributes`**, so Git-for-Windows' default
   `autocrlf=true` rewrites all 98 goldens + fixtures at checkout and the
   corpus gate fails wholesale — with a diagnostic that *lies* (`lines()`
   strips `\r`, so it reports "lengths differ" with equal numbers). §3.
3. **`cargo test` does not compile on Windows** (`tests/upgrade.rs:8`
   imports `std::os::unix::fs::PermissionsExt` unconditionally; there is
   zero `cfg(unix)`/`cfg(windows)` anywhere in `crates/`), and **no CI runs
   on PRs at all** — the suite executes once, on `ubuntu-latest`, in the
   release gate. §4, §8.
4. **The ship surface is unix-shaped end to end**: `install.sh` explicitly
   refuses Windows, `upgrade.rs` shells to `curl`/`tar`/`sha256sum`, writes
   to `/dev/null`, stages extension-less binary names, and renames over the
   running executable (forbidden on Windows — and `releases.md` §self-upgrade
   already anticipated "the rename-the-running-exe dance on Windows"; it was
   never implemented). The release matrix has four targets, none MSVC. §8.
5. **The editor surface misses by one suffix**: all four `vilan-lsp`
   discovery candidates in `extension.ts:53-57` lack `.exe`, so the
   documented dev-build-outranks-install ordering silently never holds on
   Windows; and the LSP publish planner keys diagnostics on raw `Url`
   equality, where `file:///C:/…` (minted server-side) and `file:///c%3A/…`
   (sent by VS Code) never meet — duplicated squiggles that never clear. §7.

The audit also caught one **cross-platform** bug this arc should fix:
ariadne diagnostics ignore the TTY/`NO_COLOR` gate entirely (probe-verified:
7 ANSI escapes written to a redirected file with `NO_COLOR=1`), contradicting
`paint.rs`'s per-stream contract — worst on a legacy Windows conhost, wrong
everywhere. §6.

Two claims only a real Windows machine can settle, both scheduled first
(S0): whether `vilan-embedded-std`'s `build.rs` `include_str!` of a
canonicalized (`\\?\`-verbatim) path compiles — the sole potential
*build-time* failure — and whether NTFS case-insensitivity lets `import foo`
resolve `Foo.vl` (§5's policy question).

## 1. Goal and scope

**v1 = the full toolchain is first-class on native Windows**: `cargo build`
and the complete suite green on a `windows-latest` runner (unix-only tests
honestly gated, not skipped silently); the dev loop works — `build`, `run`,
`--watch` + HMR, `fmt`, the LSP and VS Code extension; released
`vilan.exe`/`vilan-lsp.exe` artifacts with an install and self-upgrade path;
docs updated. Behavior stays platform-independent: a program that compiles
on Windows compiles identically on Linux (case policy, §5) and emits
byte-identical JS (newline policy, §2).

**Non-goals for v1**: `aarch64-pc-windows-msvc` (add when demanded — the
matrix extension is mechanical); winget/MSI/code-signing (distribution
follow-ups, F7's territory); any Windows-specific std surface; running the
K6 SIGSTOP/SIGCONT robustness tests on Windows (no pause/resume analogue —
permanently `#[cfg(unix)]`, recorded, not silent).

## 2. Newline and BOM semantics — the correctness core

The one miscompile gets a *language-level* rule, not a lexer patch:

**Rule (spec amendment, §2 source text): a Vilan source file's line
terminators are `\n`; a `\r\n` sequence in source text is one line
terminator. String literal values are built from the normalized text — a
multi-line string literal (plain `"…"` and interpolated `i"…"` alike)
contains `\n` for each source line break, never `\r\n`, regardless of
on-disk encoding.** This is exactly what triple-quoted strings already do
deliberately (`util.rs` strips CR); the rule makes the single-quoted forms
consistent instead of accidentally encoding-sensitive. Multi-line i-strings
are load-bearing (the macro-authoring `source(i"…")` idiom — corpus, docs),
so they get the same normalization and a pin of their own; the
whether-to-disallow-single-quoted-multi-line question is recorded as a
revisit when H7 (`i"""`) ships. A lone `\r` (classic-Mac) is not a line terminator we
bless; it stays what it is today (trivia between tokens; inside a string
literal it is preserved — pathological input, not worth a rule).

Also: **a leading U+FEFF BOM is stripped at file read** (the LSP's disk
reads and the analyzer's — VS Code already strips it over the wire, so today
the two disagree about line-0 columns on BOM'd files, a Windows-editor
default).

**Formatter/LSP newline policy: canonical Vilan is LF.** `vilan fmt`
converting a CRLF file to LF is a *correct reformat*, not a bug — one
canonical form, same as indentation. With §3's `.gitattributes` a checkout
is LF anyway, so this fires only on files created CRLF by an editor; the
LSP's format-on-save behaves identically by construction (same comparison).
What we must NOT ship is today's accident where the *entire tree* reads as
"would reformat" on an autocrlf checkout — that dies with §3.

Pins: plain multi-line literal from CRLF source emits `\n` (the miscompile,
pinned as a byte-comparison against the LF twin); LF/CRLF sources of a
corpus program emit byte-identical JS; emitted JS contains no `\r`
(currently true and unpinned); BOM'd source gets correct line-0 spans;
`fmt` on a CRLF-only-difference file produces the LF form exactly once
(idempotent after).

## 3. The repo gate — `.gitattributes` + an honest corpus diagnostic

```gitattributes
* text=auto eol=lf
vilan/test/** -text
```

Line 1 makes every checkout LF (kills the all-98-goldens failure and the
tree-wide `fmt --check` false positive at the root). Line 2 is
belt-and-braces: the byte-identity corpus is exempt from *any* filter, ever.

`corpus.rs`'s `first_difference` switches from `lines()` zip to a byte-level
comparison (report the first differing byte offset + a `{:?}` excerpt of
both lines), so an EOL-class mismatch is *named* instead of reported as
"lengths differ (golden 412 lines, rebuilt 412)". This diagnostic would have
cost a Windows contributor hours; it is also just better on Linux.

## 4. Suite portability

- **Gate what is genuinely unix**: `#![cfg(unix)]` on `tests/upgrade.rs`
  (the whole file is the unix install-tree fixture: sh-script fake binaries,
  `0o755`, `tar`, `file://` URL built by string concat); `#[cfg(unix)]` on
  `transport_robustness.rs`'s STOP/CONT tests (`kill -STOP` has no Windows
  analogue). A Windows upgrade-test twin arrives with §8's upgrade work, not
  before.
- **Portable rewrites where the gate must run on Windows**:
  `interpreter.rs:76`'s `timeout 30 node` (the macro/native equivalence
  gate!) becomes a Rust-side wait-with-timeout + `child.kill()`;
  `embedded_std.rs:124`'s `touch -d` becomes `File::set_modified`.
- **`.exe` correctness in tests**: `install.rs:26` and friends copy
  `CARGO_BIN_EXE_vilan` to a hand-built extension-less name — derive the
  name from the source path instead. (The 29 direct `CARGO_BIN_EXE_*` uses
  are already correct; only hand-derived names break.)
- **Fixed ports → port 0**: migrate the eleven fixed-port sites
  (`inference.rs` 48411/48412 — the known EADDRINUSE flake that bit the
  v0.12.0 gate — `cancellation.rs`, `streaming.rs`, `rpc_http.rs` ×6,
  `transport_robustness.rs`) to the bind-`:0`-then-substitute pattern
  already proven in `ssr_fullstack.rs:78` and `hmr_overlay.rs:32`. On
  Windows this is more than hygiene: Hyper-V/WSL *reserve* large dynamic
  port blocks outright, and the 45000–48500 literals sit in the commonly
  reserved range — unbindable, not merely contended. This also closes the
  recorded flake on every platform.
- Cosmetic: `vilan-lsp/src/main.rs:305`'s `/tmp/definitely/not/a/checkout`
  fixture path becomes a constructed non-existent path that means what the
  comment says on both platforms.

## 5. Path and filesystem semantics

- **One canonicalization helper** replaces the three ad-hoc
  canonicalize-with-raw-string-fallback sites (`analyzer.rs` `overlay_key`,
  `document.rs` `same_file`/`is_within`, `manifest.rs`'s dependency-dedup
  map). On Windows, `canonicalize` returns `\\?\C:\…` verbatim paths; the
  helper strips the verbatim prefix (dunce-style) so results compare with
  plain paths — today `platform_color.rs`'s `Path::starts_with` tests
  canonicalized library roots against *uncanonicalized* `program.sources`,
  which never match under `\\?\`, silently degrading library frames to
  "user code" and changing platform diagnostics. The fallback arm (path not
  yet on disk) normalizes components instead of comparing raw strings.
- **Lossy round-trips die**: `std_module_path`/`resolve_module_file` return
  `PathBuf`, not `to_string_lossy()` strings that get re-parsed (a
  non-representable path currently opens the *wrong file* via U+FFFD); the
  `to_str()` sites that silently drop modules from the import-steer
  inventory handle `OsStr` properly.
- **Case policy — enforce exact case**: after a successful module
  resolution, verify the on-disk directory-entry name matches the imported
  name byte-for-byte; mismatch is a clean error naming both spellings.
  Rationale: without this, NTFS's case-insensitivity makes `import foo`
  resolve `Foo.vl` — a program that builds on Windows and fails on Linux,
  breaking the platform-independence invariant. Enforcing is the general
  fix; the check is one `read_dir`-backed comparison on the resolve path
  (cached alongside the existing resolution). Applies to std, `pkg::`, and
  dependency roots alike.
- **`HOME` vs `USERPROFILE`**: the embedded-std cache root checks `HOME`
  first even on Windows, where Git-Bash/MSYS sets `HOME` to an MSYS
  pseudo-path Win32 can't resolve. Under `cfg(windows)` the order flips
  (`USERPROFILE` first); the install tests' `HOME`-seeding stays correct on
  unix and gets a Windows-order twin.
- **Watch temp script leak**: `vilan-watch-{pid}.js` in `temp_dir()` is
  rewritten every round and never removed — a leak everywhere and an
  intermittent sharing violation on Windows (no unlink-while-open). Delete
  after the child is killed + waited; best-effort delete on exit.

## 6. Process lifecycle and terminal

- **Child teardown via Job objects**: `run --watch`'s
  `previous.kill()` is `TerminateProcess` on the direct `node.exe` only —
  a dev server that forks keeps its port and the next round fails to bind.
  The design comment banks on the unix shared process group; the Windows
  equivalent is a Job object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`
  assigned at spawn, `cfg(windows)`-gated. This needs the `windows-sys`
  crate (§10d).
- **Virtual terminal enablement**: `paint.rs`'s gate init additionally
  enables `ENABLE_VIRTUAL_TERMINAL_PROCESSING` on Windows consoles (same
  `windows-sys`), so ANSI renders on conhost, not just Windows Terminal.
  The gate logic itself is already correct and stays.
- **ariadne goes through the gate** (cross-platform fix): every `Report`
  gets `.with_config(… color(paint-gate-for-the-target-stream))` so
  diagnostics obey TTY + `NO_COLOR` like every other line of output —
  probe-verified broken today. **And errors move to stderr** (§10f): today
  errors `.print()` to stdout (a recorded P3-era behavior) while warnings
  already go to stderr *specifically to avoid corrupting `build --stdout`* —
  errors can still corrupt it. Warnings had it right; errors join them.
  The CLI tests that read combined output are updated in the same slice.

## 7. LSP and the VS Code extension

- **Publish-planner key unification**: the planner keys on raw `Url`
  equality; on Windows, server-minted `file:///C:/…` and client-sent
  `file:///c%3A/…` are distinct keys for one file → diagnostics duplicate
  and never clear (a class that cannot exist on Linux, where both forms
  serialize identically). Fix: normalize every URL entering the planner
  through one function (round-trip `to_file_path` → canonical helper (§5) →
  `from_file_path`), while *publishing* to the client's original URI when
  the document is open (the client's key is authoritative for its own
  buffers). Pinned by a unit test feeding both forms.
- **Extension `.exe` discovery** — one expression: `const exe =
  process.platform === 'win32' ? '.exe' : ''` applied to the four
  candidates, the `'vilan-lsp'` sentinel comparison (so a user-set
  `vilan-lsp.exe` doesn't silently bypass discovery), and the
  absolute-path existence check that currently rejects a correct
  `C:\…\vilan-lsp` setting before spawn. The `~/.vilan/bin` hint string
  becomes per-platform. The `.vsix` itself is already a universal artifact
  (no bundled binaries, no native modules) — zero packaging work.

## 8. CI, release, install, upgrade

- **PR CI is born** (§10g): a `ci.yml` running `cargo test` on
  `ubuntu-latest` **and** `windows-latest` for PRs and pushes to `next`.
  Today the suite runs exactly once per release, at tag-push, on ubuntu —
  Windows support without a Windows gate would rot by the next arc. The
  Windows leg lands in S0 *expecting red* (it is the instrument that
  converts the audit's inferences to facts, including the two
  unverifiables), and flips to required once S1–S5 land.
- **Release matrix += `x86_64-pc-windows-msvc`**: `.exe` binary names, a
  `.zip` asset (`Compress-Archive`; the Windows convention, and what
  `install.ps1`/`upgrade` consume), `shell: bash` on the
  remap-verify steps (git-bash is preinstalled; the `$HOME`-grep
  reproducibility check keeps working). The ubuntu-side `sha256sums.txt`
  step already hashes whatever assets exist — no change.
- **`install.ps1`** (`irm https://…/install.ps1 | iex`): resolve latest,
  download the `.zip`, verify via `Get-FileHash` against `sha256sums.txt`,
  unpack to `%USERPROFILE%\.vilan\bin`, append to the *user* PATH
  (`[Environment]::SetEnvironmentVariable(…, 'User')`), print what it did.
  Ships as a release asset next to `install.sh`. `install.sh`'s
  Windows-refusal message now points at the PowerShell line instead of WSL.
- **`vilan upgrade` learns Windows**: per-target asset extension
  (`.zip` for msvc targets); extraction via `tar` (Windows 10's bsdtar
  reads zip archives — same command, probe in S0's CI run); the checksum
  shell-out (`sha256sum`/`shasum`, neither exists on Windows) is replaced
  by an in-process SHA-256 (§10e) on *all* platforms — one code path,
  one failure class removed; `/dev/null` → platform null device; staged
  names gain `.exe`; and the **swap dance**: Windows forbids
  renaming-over or deleting a running exe but *permits renaming it aside* —
  so: rename running `vilan.exe` → `vilan.exe.old`, move the staged new
  binary into place, best-effort delete `.old` on the next invocation.
  Exactly the algorithm `releases.md` §self-upgrade recorded and never
  implemented. The unix path keeps its current (correct) rename-over.
- **Docs**: README gains the PowerShell install one-liner and a
  PowerShell-safe hello-world variant (the current `echo '…tab…' >` is
  sh-quoting with a literal tab); `vilan/docs/` needs nothing (audited
  portable — and `docs.rs` already normalizes paths and newlines).

## 9. Slices

- **S0 — instruments first** (S): `.gitattributes`, the corpus byte-level
  diagnostic, `ci.yml` with the ubuntu + windows legs (windows
  allowed-fail). Output: the *real* Windows failure list; settles the
  `include_str!` `\\?\` build-time risk and the NTFS case probe. Nothing
  else is verifiable until this exists.
- **S1 — compile-clean, suite-green** (M): §4 in full (cfg gates, the two
  portable rewrites, test `.exe` fixes, port-0 migration).
- **S2 — newline/BOM correctness** (M): §2 in full + the spec amendment;
  the miscompile pin and the emitted-LF pin.
- **S3 — path semantics** (M): §5 in full (canonical helper, verbatim
  strip, lossy round-trips, case enforcement, HOME order, watch temp
  cleanup).
- **S4 — process + terminal** (M): §6 in full (Job objects, VT enable,
  ariadne gating + errors→stderr).
- **S5 — LSP + extension** (S–M): §7 in full.
- **S6 — ship** (M): §8's release/install/upgrade/docs; the windows CI leg
  flips to required; live verification on the native Windows host (extension
  launch + discovery order, `install.ps1` e2e, Windows Terminal + conhost
  rendering, `run --watch` teardown with a forking server).

S0 strictly first. S1–S5 are independent after it (S2/S3 touch the
analyzer/lexer and carry the usual unit-pin duty; S1/S4/S5 are
edge-surface). S6 last — it consumes all of them. Per CLAUDE.md, every
fixed behavior in every slice gets its own pin; unix-gated tests are
*listed* in the slice record, never silently skipped.

## 10. Open calls

(a) **Plain multi-line string CRLF** — recommend: normalize `\r\n` → `\n`
    (the §2 rule; matches triple-quoted behavior; alternative: error on
    unnormalized source, which punishes editors for an invisible property).
(b) **`fmt` converts CRLF files to LF** — recommend: yes, canonical LF (the
    alternative — preserve dominant ending — makes format output
    input-dependent, which fmt exists to prevent).
(c) **Exact-case module resolution enforced everywhere** — recommend:
    enforce (keeps "compiles here ⇒ compiles there"; the alternative leaves
    a Windows-only-builds program class).
(d) **Accept the `windows-sys` dependency** (`cfg(windows)`-only: Job
    objects + VT enable) — recommend: yes; it is the official bindings
    crate, feature-gated to a handful of functions. Alternative: shelling
    to `taskkill /T /F` (no dep, but a process spawn per teardown and no
    kill-on-close guarantee if the CLI itself dies).
(e) **In-process SHA-256 for `upgrade`** (a `sha2`-class dependency,
    replacing the `sha256sum`/`shasum` shell-out on all platforms) —
    recommend: yes; deletes a cross-platform failure probe and the "no
    sha256 tool found" error class. Alternative: `certutil -hashfile` on
    Windows only (keeps the shell-out zoo).
(f) **Errors move from stdout to stderr** alongside the ariadne color
    gating — recommend: yes (warnings already made this exact call for
    `build --stdout` protection; some CLI-test churn, updated in-slice).
    This is a user-visible behavior change on all platforms — flagging it
    explicitly.
(g) **PR CI introduction** (ubuntu + windows on PRs/pushes to `next`) —
    recommend: yes; note this is new *process*, not just new platform —
    every future PR pays a Windows leg (~5–10 min, parallel).
(h) **Self-upgrade on Windows in v1** — recommend: yes (S6); alternative:
    defer to the distribution slice and ship v1 install-only. The dance is
    specified and small; deferring splits the "toolchain works on Windows"
    story for little savings.

## 11. What this deliberately leaves for F7+F5

The distribution slice inherits a Windows-ready release: `.exe` assets,
`.zip` + checksums, `install.ps1`, a green windows CI leg. npm/brew/
marketplace packaging, winget/MSI/signing, and registry-dependency loading
stay F7/F5 scope — nothing in this arc pre-empts their design.
