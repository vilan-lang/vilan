# REPORT — editor-43 (Order 43)

Branch `editor-43` on origin/next @762c6aa5, five item commits + this report, not pushed.
Model: Opus throughout, no subagents.

## Per item

### E229: LANDED 57296047 (doors a, b, d)
- **(a)** `scripts/install.sh` and `install.ps1` install `vilan-vscode.vsix` when a VS Code CLI is found. install.sh looks for `code` on PATH first, then falls back to the newest `~/.vscode-server/**/code-server`. The vsix is checked against the same `sha256sums.txt` BEFORE anything is installed, so a vsix that fails the check refuses the whole install. Then it runs `--install-extension … --force`.
  - Opt out with `VILAN_NO_VSCODE=1`, or `sh -s -- --no-vscode`. An unknown option is refused.
  - The summary ends with one `VS Code extension:` line: installed, opted out, no `code` on PATH, or the editor's refusal. A refusal never fails the install.
- **(b)** `vilan-lsp` reports `serverInfo.version` (it was `None`) and answers `--version`.
  - The extension compares the server's version with its own at every start (`src/versions.ts`, which has no `vscode` import) and shows ONE warning per window.
  - A newer server gets `code --install-extension vilan-lang.vilan@<server> --force`, with Copy Command / Open Release buttons. An older server, or one that reports no version, gets `vilan upgrade`.
- **(d)** `scripts/install-dev.sh` is now the seal's toolchain refresh. The integrator's `seal.sh` lives in proposals and has no refresh step, so the step went here as the brief allowed. The script:
  - installs the vsix it packages into the VS Code Server's CLI and/or `code` (same opt-out);
  - names the vsix by version, instead of taking `ls | tail`;
  - re-stamps a stale `vilan --version` by touching build.rs and rebuilding. In a worktree `.git` is a file, so the stamp never refreshed.
- Verified: the server CLI `code-server --install-extension <vsix> --force --extensions-dir <scratch>` installs a vsix packaged from this tree. Nothing was installed into the owner's VS Code.
- The extension gains `npm run compile` and `npm test` (`node --test`). `vscode_extension::the_extensions_own_tests_pass` runs those tests in the suite; like grammar_sync's scope pins, it refuses to skip under `CI`.
- Pins:
  - seven installer pins in `release_scripts` (all 7 red on the old script);
  - six node tests of the version decision (red with the newer/older branch flipped);
  - a textual pin of the wiring;
  - `server_version_tests` in vilan-lsp.
- **Version bump: none.** `scripts/bump-version.sh` bumps the extension with the crates at the cut, so the tree's extension stays 0.41.1.

### E214: pins LANDED e82ae72a (closes at the owner's word)
- The placed-closer map moved to `editors/vscode/src/closers.ts` (no `vscode` import).
- `src/test/closers.test.ts` types keystrokes through it with `typeOne`'s logic:
  - `List<` gives `List<|>`, and `i32>` then gives `List<i32>|` (the E214 symptom was `List<i32>>|`);
  - nested lists step over both closers;
  - a comparison `<` places nothing;
  - a second `>` is inserted;
  - edits shift or forget a placed closer, and leaving the line forgets it.
  - Red with the step-over or the shift planted out.
- The OFF position (`vilan.autoClosing.generics: false`) is pinned end to end:
  - the override is removed and its closers cleared;
  - `generics: typeOverride !== undefined` is re-sent live;
  - `editor.formatOnType` is defaulted on for `[vilan]`;
  - vilan-lsp advertises `<` as the onTypeFormatting trigger. The existing pin covers the server returning the edit when the client declares false.

### E228: LANDED 164f2733
- Pinned first: `cell.read().` and `cell.write().` on a module-level `Shared<Option<i32>>` were EMPTY. The plain `Option` local and `let v = cell.read(); v.` both answered, so the call receiver was the problem, not the type.
- Two causes, one per path:
  1. **Analyzed path:** E130's substitution of a bare-`T` return was applied to bodied functions only. `Shared::read(self): T` and `write(self): &mut T` are `external`. Fixed in `call_result_type_id`.
  2. **Stale path** (the `.` typed inside the debounce, the ordinary moment): the live walk declined a bare `T`, and the analyzed arm is gated off on the edited line. Now `MemberTable` records each impl-declared member's subject, and `receiver_grounded_result_type_id` binds `T` from the receiver's type with `impl_select::bind_subject`. This also makes a stale `SignalCell<List<str>>::get()` (E130's own shape) answer.
- Pins:
  - four document shapes; the two call receivers were red before;
  - three stale shapes, red with the grounding removed;
  - the owner's module-level shape through `Backend::completion`, red likewise.

### E225 + K24: LANDED 502e12fe
- New `vilan_core::keyword_table`: `keywords()` / `is_keyword()` / `to_json()`, reading `lexing::KEYWORDS`.
- bindgen's reservations = `is_keyword` plus `RESERVED_NAMES`. `RESERVED_NAMES` holds only `self` and the built-in type names; a pin refuses a keyword there.
- **Found and fixed on the way:** a top-level `declare function` was never escaped at all, so `declare function lazy()` emitted `external fun lazy()`.
- `vilan --print-keywords` prints `{"keywords": [...]}`, sorted, one per line. It conflicts with a subcommand, and a bare `vilan` still prints help. It is documented in the intro of `appendix/cli.md`, with no new heading, so the anchor golden does not move.
- Pins:
  - keyword_table: the whole table, sorted, plain identifiers, the JSON shape;
  - bindgen `reserved_tests`;
  - `tests/bindgen.rs` `e225_…`: members plus a free function named css/dyn/lazy bind to escaped names that COMPILE (red on the old list);
  - the CLI flag's parse.
- K24 is the owner's to apply; the diff is under "The owner's steps".

### B422: LANDED f25fce11
- `BaseCacheKey::std_roots` = the canonical std base and layer roots. The inside-std test and the key now share one canonicalization.
- Red-first pin `b422_two_byte_identical_std_roots_are_two_base_worlds`: one program against the tree's std and a byte-identical copy. Before the fix, the copy's analysis read the tree's `lib.vl`.
- Plain `cargo test -p vilan-lsp` (whole binary): no `references::` "sources vector moved" panic. Only N131's two load-sensitive pins stay red there (overlay_module_reclaim, session_growth); nextest is green.
- This touches analyzer.rs, at the `BaseCacheKey` struct and its one construction only.

### E224: STOPPED (owner question, unanswered since std-42's Order 42 stop)
The brief's direction ("function import leaves warn") contradicts three things:
- `proposal/deprecation.md` line 356: "An `import` line alone does not warn … the behavior we want".
- The shipped pin `generics::a_std_marked_item_warns_at_its_use`.
- The paper itself is also split: line 87 lists "the failing *segment* at an import" as an anchor.

**Owner question:** (a) function import leaves warn, reversing line 356 and the pin (std-42's held patch does this); or (b) type import leaves stop warning, reversing E221/B382's leaf warnings. Not built.

### E182: NOT BUILT
The premise is still latent. `std/vilan.toml` has two layers (process, browser); a third `std::ui` twin needs F1 S2's native layer, which the item says to sequence with.

### E199: STOPPED (no repro)
- The message's type, `StorageSignalCell`, no longer exists in std.
- Eight probes over `swap(SignalCell<bool>, …)` all name the source's `T` correctly: wrong-typed bodies, and parameters annotated `str`.

## Finds to file
1. **E229's premise is partly false: the extension IS on the Marketplace.**
   - Release run 36353347340's `publish-marketplace` logged "Published vilan-lang.vilan v0.41.1". The gallery lists 0.41.1, 0.41.0 and 0.40.0, and Open VSX was published too.
   - Door (c) is DONE. The Order 44 L-item is moot.
   - Why the owner stayed on 0.40.0: vsix installs are PINNED (`extensions.json`: `"pinned": true, "source": "vsix"`), and a pinned extension never auto-updates.
   - Owner question (not blocking): should the installers install by gallery id (unpinned, auto-updating) instead of the verified vsix (pinned, exactly the toolchain's version)? I built what the item says (the vsix). The notice's command uses the gallery id at the exact version.
2. **Spurious error.** `swap(flag, |on: str| 42)` reports the real mismatch AND a first error, "cannot infer 'C' for this call; its bound ': Slot' cannot be checked". Diagnostics noise.
3. **`vilan upgrade` does not install the extension.** Its users keep a pinned old extension. The E229 notice catches it; an L-item could extend upgrade.rs.
4. **Dev builds of one version never see the notice.** A dev tree build between releases reports the last release's version, so dev-vs-dev drift inside one version is invisible to the check. `install-dev.sh` is the cover.
5. **The owner already applied R-a's one-liner.** `~/.vscode-server/extensions/vilan-lang.vilan-0.41.1` was installed from a vsix at 2026-09-28 13:03.

## The owner's steps
- **Editor, now:** already on 0.41.1. To move to a gallery install (auto-updating): `code --install-extension vilan-lang.vilan@0.41.1 --force`, then reload.
- **After the seal:** the integrator runs `scripts/install-dev.sh` from the sealed worktree. It refreshes both `vilan` locations and installs the vsix into `~/.vscode-server`. Reload the window.
- **E214:** confirm the doubled `>` is gone in 0.41.1, then it closes. The pins are in e82ae72a.
- **K24 (vilan-website):**
  1. Apply the diff below.
  2. Generate `keywords.json` with the SEALED toolchain (after syntax-43's B414 merges, which adds contextual words and a `"contextual"` field): `vilan --print-keywords > playground/editor-src/keywords.json`.
  3. Run `npm --prefix playground/editor-src run build`, then `node scripts/test.mjs`.
  - The new test needs a toolchain that has `--print-keywords` (released 0.41.1 does not).
  - Verified in scratch: the site's esbuild bundles the JSON import, the test PASSES against the rebuilt bundle, and it REDS 3/37 against today's committed `editor.js` (css/dyn/lazy missing).

```diff
diff -ruN a/playground/editor-src/editor.mjs b/playground/editor-src/editor.mjs
--- a/playground/editor-src/editor.mjs
+++ b/playground/editor-src/editor.mjs
@@ -25,19 +25,20 @@
 import { setDiagnostics, lintGutter } from "@codemirror/lint";
 import { tags } from "@lezer/highlight";
 import { decodeBase64Url, deflate, encodeBase64Url, inflate } from "../codec.js";
+// The compiler's keyword table, generated: `vilan --print-keywords >
+// playground/editor-src/keywords.json` (K24). tests/keywords.test.mjs holds
+// the copy to the installed toolchain's, so a keyword the language adds or
+// drops reds the site's test instead of drifting here.
+import VILAN_KEYWORDS from "./keywords.json";
 
 // --- the vilan mode: a stream tokenizer, enough for the pane to read as
 // --- vilan (the real grammar lives in the compiler; this is presentation)
 
-const KEYWORDS = new Set([
-	"async", "await", "borrows", "const", "else", "enum", "export", "external",
-	"for", "fun", "if", "impl", "import", "in", "is", "jump", "let", "macro",
-	"match", "mod", "mut", "own", "resource", "ret", "struct", "trait", "type",
-	"use", "with",
-]);
-
 const ATOMS = new Set(["true", "false", "null"]);
 
+// Every keyword the lexer knows, less the three the pane paints as atoms.
+const KEYWORDS = new Set(VILAN_KEYWORDS.keywords.filter((word) => !ATOMS.has(word)));
+
 // Tokenizes with a mode stack so an i-string's `{holes}` read as the code
 // they are: string text stays rose, a hole's contents go back through the
 // code rules, and the brace seams mark themselves. Attributes, function
diff -ruN a/tests/keywords.test.mjs b/tests/keywords.test.mjs
--- a/tests/keywords.test.mjs
+++ b/tests/keywords.test.mjs
@@ -0,0 +1,35 @@
+// K24: the playground editor's keyword list is the compiler's, not a copy.
+//
+// `playground/editor-src/keywords.json` is generated by the toolchain
+// (`vilan --print-keywords`), and `editor.mjs` imports it. Two ways it can go
+// stale, one check each: the installed toolchain's table moved on (a keyword
+// added, or dissolved as `resource` was by B413) and the file was not
+// regenerated; or the file was regenerated and the committed bundle
+// `src/playground/editor.js` was not rebuilt from it.
+
+import { readFileSync } from "node:fs";
+import { spawnSync } from "node:child_process";
+import { check, verdict } from "./support/check.mjs";
+
+const committed = JSON.parse(
+	readFileSync(new URL("../playground/editor-src/keywords.json", import.meta.url), "utf8"),
+);
+const printed = spawnSync("vilan", ["--print-keywords"], { encoding: "utf8" });
+check(printed.status === 0, "`vilan --print-keywords` runs (a toolchain with K24's export)");
+if (printed.status === 0) {
+	const current = JSON.parse(printed.stdout);
+	check(
+		JSON.stringify(committed.keywords) === JSON.stringify(current.keywords),
+		"keywords.json is the installed toolchain's table — regenerate with " +
+			"`vilan --print-keywords > playground/editor-src/keywords.json`, then " +
+			"`npm --prefix playground/editor-src run build`",
+	);
+}
+
+const bundle = readFileSync(new URL("../src/playground/editor.js", import.meta.url), "utf8");
+for (const word of committed.keywords) {
+	check(bundle.includes(`"${word}"`), `the committed bundle carries \`${word}\``);
+}
+check(!committed.keywords.includes("resource"), "`resource` is the `[resource]` attribute, not a keyword");
+
+verdict("keywords");
```
(`playground/editor-src/keywords.json` is the file step 2 writes; today's output is the 34-word list the sealed toolchain will supersede.)

## Gates (each commit, final tree)
- `cargo fmt --check`: clean.
- `cargo clippy --workspace --all-targets -D warnings`: 0, at every commit.
- `-p vilan-lsp` whole binary under nextest, including `book_sync`: 887 → 888 → 922 (with vilan-ide) → 896 → 897, all passed.
- `-p vilan-cli` corpus/split/examples/diagnostics_ledger (+ vscode_extension, release_scripts, grammar_sync, `--bin vilan` where touched): all passed.
- vilan-core `--lib`, bindgen, docs, markdown_golden, base_cache, world_cache_spike, check_scope_differential, relocated_std: all passed.
- `npm run compile` 0; `npm test` 13/13.
- Not run: the whole-set native differential and `cargo test --doc`. No std, emitter or doc-test was touched.
- `install.ps1`: parsed clean with pwsh's parser only; its Windows runtime is unverified (no fixture exists for it).
- **Ledger rows:** none (no new diagnostics). **Goldens moved:** none. **book_sync:** no quick fix or code action added. **Count words in docs pages:** none added or changed (hello-vilan.md, appendix/cli.md, README.md, editors/vscode/README.md edited without counts).

## Expected conflicts
- **CHANGELOG `## Unreleased`:** every lane creates it. Mine holds 5 entries, all `family: tooling`.
- **syntax-43, the keyword-table seam.** Their LANE-STATUS already plans to adopt `keyword_table` and `--print-keywords` at rebase, add `"contextual"`, and make `is_keyword` RESERVED-only. They will need to update:
  - `tests/bindgen.rs` `e225_…`, `keyword_table`'s css/dyn/lazy asserts and bindgen `reserved_tests`. `dyn` and `lazy` become contextual under B414 and stop being escaped; `css` stays reserved.
  - `every_spelling_is_a_plain_identifier…`: their `Self` is uppercase.
- **solver-43:** analyzer.rs `BaseCacheKey` (B422; one struct field plus the construction block).
- **vilan-ide completion.rs** (E228: `call_result_type_id`, `live_receiver_type_id`, `MemberTable`): no other lane is mapped there.
