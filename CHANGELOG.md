# Changelog

Vilan is a fast-moving alpha. Minor versions (`0.X`) may change the
language, the standard library, and the wire protocol without a
deprecation period; patch versions are fixes. Each release below links
the highlights — the [book](https://vilan-lang.org/docs/) always
tracks the latest state.

## v0.17.0 — 2026-07-28

**Same-scope shadowing.** A `let` may redeclare a name in the same scope: the later binding shadows the earlier one from its own declaration point onward, and uses before that point keep the earlier binding — `let d = parse(d);` narrows a value under the same name, the way it reads. A binding becomes visible at the *end* of its declaring statement, so an initializer never sees the binding it declares: `let x = x + 1;` reads the previous `x`, and is an error when there is none. Parameters, loop items, and pattern captures are shadowable the same way; module-level bindings keep their order-independent, one-declaration-per-name rule. Two bugs died with the old behavior: `let x = x;` sent the analyzer into a stack-overflow abort (as did a module-level `let a = b; let b = a;`), and a same-scope redeclaration used to bind *every* use of the name — earlier ones included — to the last declaration, so a cleanly-compiling program crashed at startup with a `ReferenceError`.

**Breakpoint chains are mobile-first for real.** `std::style` emitted `@media (min-width: …)` rules in lexical order, and `'1' < '6'` put the 1024px rule before the 640px one — so with two breakpoints on the same property, `.sm(two_col).lg(three_col)`, a desktop viewport matched both, specificity tied, and the *narrow* value won the cascade. Media rules now emit ascending by min-width, so the widest matching breakpoint wins — the rule the docs now state outright. Everything else about the stylesheet is byte-identical.

**Inline SVG renders.** `view("svg")` used to build an HTML-namespace element — same serialization, renders nothing — because `document.createElement` knows only one namespace. `view` now recognizes the SVG vocabulary (exact case: `clipPath`, not `clippath`) and creates those elements through `createElementNS`; on the server, an `svg` root serializes with its `xmlns` attribute, and the SSR differential pins the two legs byte-for-byte. Tags that exist in both vocabularies (`a`, `title`, `style`, `script`) stay HTML. Riding along: `class`/`styled`/`bind_class` now set the `class` attribute instead of assigning `className` — identical for HTML, and the only form SVG accepts (its `className` is readonly; assigning it throws in module code). Icons can finally live in the view tree and inherit `currentColor` instead of shipping as pre-colored `<img>` files.

**The editor stops red-flagging shared files in two-entry packages.** A file shared between a browser entry and a node entry — the fullstack shape `vilan init` scaffolds — analyzes in the editor under a platform inferred from its imports, and the inference used to read *any* `std::ui` import as browser evidence. A shared file importing the process twin's `render` was therefore analyzed as a browser file and its import squiggled "cannot find `render`", while `vilan build` was clean on every entry. Inference is now name-aware: for a module both platforms serve, the *imported names* decide (`render` says process, `mount` says browser, `view` says nothing), so shared files resolve the twin they actually use.

**One bad request can no longer take down the language server.** A panic in any request handler used to unwind through the async runtime and abort the whole server — and after five crashes in three minutes the editor stops restarting it, so one poisoned hover locked out every feature until a manual restart. Handlers are now panic-fenced: a read-only query answers its empty default, rename and formatting refuse with an honest "this is a bug" error instead of pretending there was nothing to do, and the panic is logged to the output channel either way. The fences reach deeper too — a lexer or parser panic now degrades to a diagnostic like an analyzer one always has, and the caught panic can't poison the server's locks into failing every later request.

**Compiler messages punctuate like prose.** Diagnostics, the CLI's status and error lines, the language server's hovers and completions, and the HMR overlay all used " — " as their universal separator. They now punctuate like sentences: a colon before the rule, a semicolon before the fix. The words are unchanged, so anything matching message *text* still matches; anything matching the exact punctuation will see the difference. The book and every README received the same edit, and the pages that quote compiler output quote it verbatim again.

**Release artifacts carry their third-party notices.** The release archives, the npm packages, the Homebrew install, and `vilan upgrade` all ship `THIRD-PARTY-NOTICES.txt` alongside the licenses. The file is generated from the exact dependency lockfile and a suite gate fails the build when a new dependency is missing from it, so it cannot fall behind.

## v0.16.0 — 2026-07-28

**Breaking: single-quoted strings no longer span lines.** A raw line break inside `"…"` or `i"…"` is now a compile error, and so is a backslash before one — nothing escapes a line terminator. Multi-line text goes in the triple-quoted forms, `"""…"""` and `i"""…"""` (the interpolated `i"""…"""` form is new in this release; plain `"""…"""` arrived quietly in v0.10.0); a single line break inside a one-line string is written `\n`. The payoff is error locality: a string whose closing quote you forgot used to run on to the next `"` anywhere below it, so the compiler complained somewhere else entirely — often many lines away, about something unrelated. It is now reported on the literal's own line, pointing at the opening quote, and the rest of the file still compiles and still serves hovers and completions in the editor. The editor's syntax highlighting agrees: a broken string paints at most its own line. To migrate: a multi-line `"…"` becomes `"""…"""` (an `i"…"` becomes `i"""…"""` — the triple forms are raw and strip the closing delimiter's indentation), or collapse onto one line with `\n`.

**Breaking: `serve_service` and `serve_connected` hand their ready callback the `Server`.** `on_ready` is now `|Server| void`, matching `serve_rpc` — which is what makes `serve_service(0, …)` usable: the server you're handed knows the port it actually bound (`server.port()`, new this release).

**`vilan init` — install, init, run is the whole first minute.** Three templates ship embedded in the binary — `node`, `browser`, and `fullstack` in the one-package/two-entries shape — chosen with `--template <name>` or an interactive prompt on a TTY (and a clean error, not a hang, without one). It never overwrites: a file that already exists is an error. The templates are suite-gated, so a scaffold that stops compiling fails vilan's own build, not your first minute. The `vilan/examples` tree was reshaped to match — single-package/two-entries is now the default project shape in examples and docs alike, the multi-package workspace kept where it genuinely teaches workspaces, and every example carries a README saying what it demonstrates and how to run it.

**The manifest learned the dev loop's two missing keys.** `[build] run` — a command or a list of them — runs before each build and each `--watch` round, sequentially, from the manifest's directory: the Tailwind bridge, asset pipelines, codegen sidecars. A failing hook fails the build naming the command, and `vilan check` runs none of them. And `default-entry` names the entry `vilan run` should drive in a multi-entry package, in both manifest shapes, with the precedence you'd expect: `--entry` beats the manifest, the manifest beats the lone-leg default, and with none of the three the error names both ways to choose.

**Cancelable timers.** `std::time::Timer` is a delay you keep hold of — `setTimeout` and `clearTimeout` as one value. `Timer::after(ms)` (or `after_for(duration)`) starts the timer immediately; `timer.wait()` yields `true` when it fires and `false` when `timer.cancel()` got there first, and that verdict is remembered, so every waiter — one parked before it settled, one arriving long after — gets the same answer, and asking a settled timer returns at once. Cancelling twice, or cancelling a timer that already fired, does nothing. This is the shape a re-clickable button wants: keep the pending timer in hand and call it off before starting the next one, instead of leaving a stale sleeper to wake up and hopefully notice it's stale. A `Timer` is an ordinary value wrapping one host handle, the way a `Signal` wraps one cell, so copying it shares the same timer. And the two kinds of cancellation stay distinct: `cancel()` is a verdict, while a cancelling nursery tears down only the task that was awaiting — the timer itself is untouched and its other holders can still wait on it or call it off.

**Every diagnostic renders in its own file.** Every post-analyze pass — const, platform, async, context, drops — used to anchor its diagnostics to the *entry* file: the CLI rendered the entry file's text at another module's offsets, and the editor squiggled the wrong file entirely. Each diagnostic now renders in the file it belongs to; notes attached to a diagnostic reach the editor as locations in *their* files, so a note pointing across a module boundary finally lands where it points; and the HMR overlay names each diagnostic's own file instead of the entry. A rider closes the chained-element-access trap (`grid()[0][1]` and family), with six shapes pinned.

**Highlighting and inlay hints hold still while you type.** The language server keeps two views of an open file — the text you are editing and the text it last analyzed — and it used to mix them: an answer computed from the older analysis was converted to line/column through the *newer* text, so one character inserted anywhere above shifted every token and hint below it, and hints that shifted out of the visible range vanished outright. Every answer that comes from analysis — semantic tokens, inlay hints, hover, go-to-definition, find-references, the outline — is now expressed in the coordinates of the text it was actually computed from, which stays visually correct everywhere except the line you are on; and when the new analysis lands a moment later the server asks the editor to re-request both providers, so the catch-up happens immediately instead of whenever the editor next feels like asking. Three related fixes ride along: a completed analysis now *merges* into the buffer rather than replacing it (characters typed during the 80–190 ms it takes were being thrown away), an analysis finishing after you close a file no longer resurrects it, and semantic-token lengths are counted in UTF-16 units as the protocol specifies. Rename and Organize Imports — the two requests that hand back edits — answer "still analyzing, retry" for the fraction of a second while the buffer is ahead of the analysis, instead of returning edits computed against text you no longer have. Two hover fixes ride along: a constant whose preview carried multi-byte text crashed the server outright (a byte-budget cut landing inside a character), and comments or blank lines inside a function body no longer hover as the enclosing function.

**`vilan fmt` breaks up long method chains.** A statement wider than 100 columns whose expression is a chain of two or more `.method(…)` calls now renders with its subject on the statement's line and one link per line below it, indented one level — the shape a style builder or a fluent API wants, and the shape you probably wrote before the formatter collapsed it. The choice is purely width-driven and so is stable in both directions: a chain that fits stays on one line (a hand-split short chain still collapses), and a chain that doesn't always breaks the same way, so formatting is a fixed point. Non-call postfixes (`.field`, `[i]`, `!`) ride along with the link before them rather than taking a line each, and a chain that continues into an operator — `style()…margin(space(0)) + reveal` — puts the continuation on its own line at the links' indentation. Either side of an operator can be the chain that breaks: when it is the **right** side — `let tint = const (base + style()…)`, the shape a style module full of composed rules is written in — everything through the operator and the chain's subject stays on the statement's line, the links break below it, and the closing parenthesis and the `;` glue after the last one. The left side wins when both sides could break, and the right side then breaks only when the continuation line it landed on is itself over the budget. This is the formatter's first width-aware decision, and it is applied per line, recursively: when a link's *own* line still runs over the budget, the call on that line breaks its last argument too — a nested chain drops its links one level further in, and a list literal goes one element per line, indented past the line that opened the `[`, with a trailing comma after every element and the `]` back at that opening indent. A hand-nested `std::ui` view tree — `view("div").styled(…).child(view("div").styled(…).child(…))` — therefore comes back out the way its author wrote it, at any depth, in a single pass, while every subtree that fits stays on one line. A list that fits also stays on one line, *without* a trailing comma, so the comma marks a split list and nothing else. Two things deliberately do not move: layout hangs off a call's **last** argument, the builder convention every language's formatter follows, so when an earlier argument is what makes a line long the line stays long; and argument lists are still never wrapped.

**`vilan fmt` stopped skipping files with redundant parentheses.** A parenthesized group that the language did not strictly need — `let b = (1 + 2);`, `ret (x);`, `f((1 + 2))`, `(300).as_u8()`, and above all `const (chain + reveal)` — used to make the formatter give up on the **entire file** and return it byte for byte, with `fmt --check` then reporting that file as already clean. Those files now format, and the parentheses are kept exactly where you wrote them: the formatter preserves a group you wrote rather than judging it redundant, because a redundant group is usually there for clarity. A style module written as `let heading = const (style()…margin(space(0)) + reveal);` was exactly this case, so it is also what kept the new chain splitting from reaching real files.

**`vilan test` works in workspaces and `[library]` packages.** A manifest without `[package]` used to resolve to an empty workspace, so `vilan test` there compiled against nothing. Tests now see their `pkg::` siblings, path dependencies, and git dependencies (fetched on demand) in every project shape. The editor gained the same footing: a file in a `[library]`-rooted project resolves its own modules and dependencies, a manifest that fails to parse says so in the editor, and an inherited-declaration error is addressed to the manifest that declared it.

**Handles cross the wire.** `[derive(Wire)]` now accepts types carrying `Handle<T>`, and `Arena::branded()` starts an arena's generation counters at a random value instead of zero, so a handle issued by one branded arena is rejected by another — the shape a server handing session handles to clients wants.

**The entry file's case is checked.** A case-mismatched entry path — `Main.vl` on disk, `main.vl` in the manifest — is now a clean diagnostic on every path that names an entry, including `[entry.<name>]`, extending v0.14.0's case-exactness rule to the one file it missed.

**The book's canonical home is vilan-lang.org/docs.** Old deep links redirect.

**The VS Code extension requires VS Code 1.91 or newer.** The extension's language-client library moved to its current major (v10, clearing npm audit's outstanding advisories), and that library's floor is the extension's floor. The server's output channel became a log channel on the way: the Vilan Language Server output now carries timestamps and a per-level filter.

**Vilan has a new look.** The palette moved from indigo-and-lavender to blush on near-black (`#F9DFE7` on `#120004`), and every rendering of the brand moved with it: the repository header, the VS Code extension's icon and listing banner, the CLI's post-upgrade mark, and the website. The mark itself is unchanged.

## v0.15.0 — 2026-07-25

**Module bindings initialize in dependency order.** A top-level `let` now runs after every binding its initializer actually evaluates — the ones it reads, plus everything read inside whatever it calls on the way — so a binding may reference one declared below it, in the same file or in another module, exactly as a function may call one declared later. Creating a closure evaluates nothing, so two module-level closures may still name each other freely. This kills a real miscompile: declaration order used to follow the order names happened to be listed in your *imports*, so a constant that depended on another could be emitted before it and crash at load with `Cannot access 'X' before initialization` — with nothing at compile time to warn you. v0.12.0 made the emitted JavaScript independent of import *statement* order; this closes the other half, the names inside a `{ … }` brace set, so **no spelling of your imports can change what your program does or the bytes it compiles to** — `vilan fmt` can sort them freely. And a genuine cycle among initializers (including a binding that reads itself) is now a compile error that names the round trip (`via A → B → A`), anchored at the read that closes it and noting each participant's declaration, instead of a crash at load. The order is specified rather than incidental: spec §7.1 fixes dependency order first, then a canonical module order — the standard library first, then dependency packages, then your own, modules within a package by name, the entry file last — for bindings that depend on nothing from each other. One behavior note: a module initializer with *side effects* may now run in a different relative order than before — the old order was whatever your import listing happened to produce; the new one is the rule above. Bindings that actually depend on one another are unaffected: those were the broken case.

**Git dependencies.** A dependency can now come from a repository, pinned to an immutable point: `shapes = { git = "https://…", tag = "v1.2.0" }` (or `rev = "<commit sha>"` — exactly one; a `branch` is refused, because a branch moves and so cannot pin anything). The checkout is fetched shallowly, verified to be a vilan `[library]`, and cached content-addressed under `~/.vilan/` — after the first fetch, builds are fully offline, and the cache serves the *pinned* content even if the upstream moves or disappears. A dependency's own git dependencies resolve through the same cache. Fetching happens only when a build of a declaring project needs it — the toolchain still makes no passive network calls, and the editor never fetches at all: a not-yet-fetched dependency shows as a note to run `vilan build`. Workspaces got a matching quality-of-life: `[project.dependencies]` declares a dependency once at the workspace root, and a member opts in with `shapes = { project = true }` — explicit, so nothing is inherited by surprise.

**`vilan.toml` speaks in the editor.** The manifest now has completions — keys per table, values where they're enumerable (targets, presets), quotes placed for you — and its problems finally surface where you're looking: a manifest that doesn't parse, an invalid dependency, or a git dependency that isn't fetched yet publishes a diagnostic on `vilan.toml` itself and clears when you fix it.

**Installing vilan is becoming one command.** The Homebrew tap is live today: `brew install vilan-lang/vilan/vilan` (macOS and Linux, both architectures). The npm package (`@vilan-lang/vilan` — the command is still just `vilan`) and the VS Code Marketplace / Open VSX listings are built and ship with the next releases as their publishing credentials land. `vilan upgrade` learned to respect whoever installed it: an npm- or brew-managed vilan is steered to `npm update -g` / `brew upgrade` instead of overwriting files the package manager owns — the curl-script install keeps upgrading itself exactly as before.

**A save during the first watch build is never lost.** `vilan run --watch` took its file-change baseline *after* the initial build, so a save landing while that build ran was silently absorbed — the watcher never noticed, and your change sat unbuilt until you saved again. The baseline now precedes the build: a save at any moment after the watcher starts triggers a round. Found because a CI test kept "flaking" — it was right, four times, on three platforms.

## v0.14.0 — 2026-07-24

**Vilan runs on Windows.** Native, not WSL: install with one PowerShell line (`irm https://github.com/vilan-lang/vilan/releases/latest/download/install.ps1 | iex`), and the whole toolchain is there — `vilan.exe` and `vilan-lsp.exe`, the compiler, `run --watch` with hot reload, `fmt`, `test`, and `vilan upgrade` (which learned the Windows swap: a running executable can't be replaced in place, so the old one steps aside and is swept on the next run). The VS Code extension finds the server on Windows now, and the language server treats every spelling of a file — `C:` vs `c%3A`, even DOS-era `RUNNER~1` short names — as the one file it is, so diagnostics never duplicate or stick. Stopping a watch round kills the *whole* process tree (a forking dev server can't hold its port hostage), colors render in both Windows Terminal and classic conhost, and the entire test suite now runs green on Windows in CI as a required check on every change — this isn't a port that will quietly rot.

**Line endings became law.** A `\r\n` in source is one line terminator, and string literal values are built from the normalized text — a multi-line string contains `\n` regardless of how your editor or Git saved the file. This closes a real miscompile: the same program checked out with CRLF endings used to embed `\r` in its string values and emit different JavaScript than its LF twin. A leading byte-order mark is now stripped everywhere source is read, canonical Vilan is LF (`vilan fmt` converts), and a `.gitattributes` keeps every checkout byte-stable. The full corpus compiles byte-identically from LF, CRLF, and BOM'd copies — pinned, on both platforms.

**Module names are case-exact — everywhere.** New language rule (spec §4.2): an import must match the on-disk file name byte for byte. On a case-insensitive filesystem, `import foo` finding `Foo.vl` is now a clean diagnostic naming both spellings instead of a resolution — so a program that builds on Windows builds identically on Linux, with no case-sensitivity surprises waiting in CI.

**Errors print to stderr now, and every diagnostic respects `NO_COLOR`.** Compile errors joined warnings on stderr — `vilan build --stdout` can never again interleave a diagnostic into the JavaScript it pipes — and the ariadne-rendered reports finally obey the same terminal gate as the rest of the CLI: colored when you're looking, byte-plain when piped or `NO_COLOR` is set. If you were parsing errors from stdout in a script, read stderr instead.

## v0.13.0 — 2026-07-23

**Server-side rendering: render and replace.** A full-stack app can now serve its first paint as real HTML — for the crawler, and for the human who sees content before any JavaScript arrives. The model is deliberately simple: on the server, `import std::ui` resolves to a render-only implementation of the same API, so the *same component functions* build an HTML string instead of DOM — each `bind_*` embeds its signal's current value, event handlers are accepted and ignored — and `render(view)` hands your route handler the markup to splice into its shell. On the client, `mount` now replaces the container's contents when it boots, so the server HTML gives way to the live, bound tree in place. There is **no hydration** — no node adoption, no mismatch errors, no second set of rules — and that is a design decision, not a gap: the eventual path is resumability, which makes hydration's machinery obsolete anyway. The two `ui` implementations are held together by a differential test that renders one shared component through both and requires byte-for-byte agreement. The new [`examples/ssr`](vilan/examples/ssr/) is the working loop, and the [SSR guide](https://vilan-lang.org/docs/guide/ssr.html) explains the one rule that matters (build pure, bind reactive) and where v1 fits: self-contained and server-data-seeded apps — views that read a live rpc client while building are client-side by nature.

**Snippets in completion.** `for`, `fun`, `struct`, and `match` now complete as tab-through templates alongside their bare keywords — parameter names, field stubs, and match arms pre-placed — degrading to plain keywords in editors without snippet support.

**The CLI dresses for the terminal.** Build, watch, test, fmt, and upgrade output is colored when you're looking at it — green successes, bold red errors, cyan dev-loop lines — and byte-for-byte plain the moment it's piped or `NO_COLOR` is set.

## v0.12.0 — 2026-07-22

**The editor grew up.** Completing a function now inserts a real call — tab-through parameter placeholders by default (`greet(name, times)`), parens-only or plain-name via the new `vilan.completion.functionCall` setting — with the full signature and `///` doc shown right in the suggestion popup and parameter hints opening as you land in the parens (completing a callee you already parenthesized, or a function passed as a value, stays bare). Hover now answers everywhere: variables show their typed binding, parameters show their declared convention (`own x: T`, `x: &mut T`), and every keyword explains itself in one line with a deep link into the book. **Organize Imports** sorts and prunes: unused imports and brace-set branches are removed conservatively (never while the file has errors, never re-exports, never an import that only a derive's generated code uses — the compiler knows), with an opt-in `vilan.organizeImports.onSave`. Inlay hints and semantic tokens gained toggles, every setting applies live, and — pinned by a thirteen-test guarantee — the language server keeps working in files with errors: hover, navigation, completion, and the outline all serve the parsed remainder on both sides of a typo.

**`vilan fmt` sorts imports, and import order stopped mattering.** Top-level imports format into one canonical order (`std` first, then dependencies, then `pkg`; brace sets alphabetized; comments travel with their line; block-scoped imports deliberately untouched) — and underneath it, the compiler now walks modules in a canonical order too, so **the emitted JavaScript is byte-identical no matter how your imports are arranged**. Reordering an import can never again churn your build output.

**The extension ships its licenses** (MIT OR Apache-2.0) in the package, ready for the marketplace.

## v0.11.0 — 2026-07-22

**Hot module replacement — the dev loop closes.** `vilan run --watch` on a full-stack workspace now hot-reloads the browser: save a file and the app updates in place with module-level state carried across the swap (plain values by value, `Signal`/`Shared` by payload into fresh cells — keyed and fingerprinted by the compiler, so a changed shape fresh-inits instead of adopting stale data), while the server leg restarts behind the scenes and the client's rpc mirrors resync on their own. A CSS-only edit hot-swaps the stylesheet without a reload; a compile error shows an in-page overlay carrying the *actual* compiler diagnostics (file, line, message, note — the terminal's own rendering) and clears on the next good save; `std::dev` gives app code `on_teardown` and a type-checked `stash`/`take` carryover (only plain data may cross a swap — the compiler enforces what Vite leaves to convention). Watch rounds got structurally cheaper too: parse results are content-cached across rounds and a leg whose sources re-hash identically is skipped outright, its artifacts reused byte-for-byte. Multi-server workspaces pick their entry with `vilan run --entry <name>`. The [dev-loop guide](https://vilan-lang.org/docs/guide/dev-loop.html) walks the whole loop.

**The frontend is handwritten now — builds are ~2.7× faster.** The chumsky combinator frontend is gone, replaced by a hand-rolled lexer and recursive-descent parser proven byte-identical first (279/279 whole-file tree agreement, every corpus program compiled to identical output through the new code *before* it was wired in) and then measured: a release build of the todo client dropped from ~0.49 s to **0.18 s**, instruction counts fell 5.21 B → 2.01 B, and the frontend went from ~63% of a compile to under 4% — the debug binary gains the most. Parse errors improved with it: the 30-token "expected one of …" dumps are gone, a missing separator reports `found 'y' expected ',' or '}'` at the offending token, the `a!==b` spacing trap gets a first-class hint, and a syntax error no longer discards the whole file — the parsed prefix survives, so the language server keeps working on everything above the typo.

**Trait impls must now match their trait's signatures.** Previously an impl satisfied a trait by member *name* alone; receiver convention, parameter types, arity, and return type were never compared, so a wrong `fun drop(self)` compiled against `fun drop(&mut self)`. Every member is now checked under the trait's own generics (`Self` included), with the mismatch spelled per dimension. **This can reject code that previously compiled** — the fix is to make the impl say what the trait says. (A deliberate leniency: an `async` impl of a sync-declared method stays legal — dispatch is monomorphized, so the caller always knows the concrete callee.)

**Two real bugs died.** A module-level closure referenced *only* by calls (`let helper = || …;` used as `helper()`) was tree-shaken out of the bundle while its call sites remained — a runtime `ReferenceError`; calls now count as references, and six sibling shapes (calls through `?.`/`!`, transitive closure chains, nested modules) were quietly broken the same way and are fixed with it. And a typo'd name in value position no longer cascades — one unknown identifier is one error, not a fan of `Expected i32, but got void` noise at every use.

**`vilan fmt` formats everything.** The formatter silently returned files unchanged when they used newer constructs — destructuring, fixed arrays, `?.` chains, the macro forms, numeric suffixes. Every construct now has its printer, guarded by a standing zero-bail gate over the whole corpus, and two latent printer bugs found on the way (one would have reformatted `-(2 + 3)` into `-2 + 3`) are fixed. The standard library itself is freshly `vilan fmt`-formatted.

**Sharper diagnostics across the board.** Notes that pointed into `std` for user-caused conditions were audited (they are all genuinely declaration notes — "the trait declares it here" — and stay); one unresolved name suppresses its whole echo family; and the diagnostics ledger now runs as a living gate — every new compiler error message gets verdicted against the standard as it lands, not in batches after the fact.

## v0.10.0 — 2026-07-19

**Resources: values that clean up after themselves.** A `resource struct` (or `resource external struct`) is the new owned-resource class — a value with exactly one owner that **moves** on binding and `own`-passing instead of copying, is loaned through the ordinary view conventions, and runs its `Drop` at its owner's scope end, every exit included (`ret`, `jump`, panic unwinding — and a value-returning `main` now runs its drops *before* the process exits). Containment infers: a struct, enum, tuple, or fixed array holding a resource *is* one. `Option.take`/`replace` are the sanctioned partial move, std's `drop(value)` destroys early with no public `close()` anywhere, and the affine checker rejects the whole double-close family at compile time — use-after-move (with a note at the move), conditional moves, moves in loops, resource captures in closures and spawns, resources in native containers, coercions to `any`, and derives (`Wire`/`Hashable`/`PartialEq`) on resource-holding types. `Database` is the first real resource: it closes its `node:sqlite` handle deterministically, module-level handles keep process lifetime (the serve-forever idiom — now **loan-only**, and reachable from closures, which the checker previously miscounted as captures), and `OwnedNursery` owns background tasks whose real failures still reach the console with their spawn origin while cancellation echoes stay silent. The [resources tour](https://vilan-lang.org/docs/tour/resources.html) walks it; spec [§6.8](https://vilan-lang.org/docs/spec/memory.html) is the contract.

**One law now opens the memory model.** Spec [§6.0](https://vilan-lang.org/docs/spec/memory.html): every alias is a *claim* on an owner whose *epoch* advances on a fixed set of events — and a claim is valid while its owner's epoch is unchanged. Views are the statically-proven claims, handles the dynamically-checked ones, and every mechanism in the chapter (views, projections, `Arena`/`Handle`, `Shared`, resources) is presented as a cell in that one table.

**Rule 4 is now enforced everywhere views actually come from** — and it's smarter about what invalidates. Previously only a direct `&place` view was policed; a view returned through a call (`list.at(0)`, `arena.get(h)`) or bound by a `Some(let v)` match capture was invisible to the invalidation checks (and a *chained* projection didn't even lower as a view — a real miscompile, fixed). Now every view anchors at what it projects, multi-parameter projections anchor at all of them, and mutating a viewed container, reassigning its root, or holding any of these across `await` is the same compile error the direct form always raised. **This can reject code that previously compiled** — re-derive the view after the mutation or suspension, as ever. In exchange, the checker stopped over-rejecting: only calls that may change a container's *geometry* (grow, shrink, reallocate, swap an aggregate field — inferred per method as the new `bumps` effect, hover-visible beside `borrows`) conflict with a live view; a method that merely writes fields or elements through `&mut self` now passes freely.

**`Arena.get` hands back a live view** — `Option<&T> borrows self`, the shape the spec always described, instead of a copy; `set` remains the write path, and stale handles still answer `None`.

## v0.9.0 — 2026-07-18

**Higher-order functions adapt to async callbacks.** `map` is one function, not two: passing an async closure instantiates an async copy of the receiving function — its calls through the parameter are awaited, **sequentially** (each callback settles before the next begins) — while every sync call site keeps the untouched original. Adaptation follows the closure through plain parameters transitively (`helper(xs, f)` forwarding into `map` adapts end-to-end), an adapting function traverses a snapshot of its receiver so interleaved work can't tear the iteration, and it stops honestly at the boundaries: a parameter marked **`sync`** declares the synchronous contract (the reactive graph's recompute positions — `Signal::map`, `turn`, `batch`, the UI render callbacks — are `sync`), host (`external`) functions can't await your closure (unless a parameter is *declared* `async |…| T` — the typed channel), and trait/generic dispatch has no static callee to instantiate. When the elements are independent, opt into concurrency with the spawn-then-settle idiom: `.map(|x| async work(x))` then `Task::settle_all(tasks)`.

**Spawning grew a spine: `Task<T>`, and nurseries to own them.** `async expr` now yields a `Task<T>` — an eager, opaque handle; copying it refers to the same task. Every task absorbs its own failure at construction: a spawned panic can never crash the program as a host "unhandled rejection" — a later `await` receives it, and a task nobody observes reports the error to the console stamped with the function that spawned it, then execution continues. `Task::settle_all` joins many; `Task::race` yields the first to settle. Raw host promises stay `Promise<T>` at the extern seam, and `await` unwraps both.

**`nursery(body)` is structured concurrency** (`std::task`): every task spawned in the body's *dynamic extent* — by the body, by anything it calls, by the tasks themselves — is joined before the nursery returns the body's value. Failures follow the first-observed rule: a body throw wins, otherwise the earliest-settled task failure, re-raised from the `nursery` call with its spawn origin while every other task is absorbed. `n.cancel()` aborts the whole extent — the nursery's `AbortSignal` rides ambiently into `sleep` and `fetch`, so cancellation cuts in-flight IO short instead of waiting it out (a live e2e cancels a fetch against a hanging endpoint and joins in ~3s instead of 60), cancellation rejections are absorbed echoes rather than errors, nurseries chain so an outer cancel reaches nested extents, and `Task::race` + `n.cancel()` is the race idiom. The first real failure cancels the same way, so one task's error stops its siblings' work at settle time — not when the join happens to look. Spec [§7.7](https://vilan-lang.org/docs/spec/execution.html) is the contract; the [async tour](https://vilan-lang.org/docs/tour/async.html) walks it.

**Asyncness now rides every value channel.** `async |T| U` is accepted on struct fields and function return types (calls through a field read or a returned closure await implicitly), unannotated bindings adopt asyncness from any value they hold — including `mut` rebinds — and storing an async closure where a plain value-returning closure type is declared (a field, a return type) is a compile error instead of a promise wearing the wrong type. Void-returning positions keep spawn semantics, which is why UI handlers await freely with no ceremony. The standard library's own transport and draft plumbing was migrated off its workarounds in the process.

**Variadic tuple bounds are enforced.** `T: (2..)` and `(..: Display)` parsed since variadics landed but checked nothing; arity ranges and per-element trait bounds now hold at every call and construction site, with the note pointing at where the bound was declared.

**Editor and diagnostics tail.** Notes can point into another file (the "declared here" half of a cross-module error lands in the right source); inlay type hints for inferred `let` bindings; semantic tokens gained modifiers; parse errors name the split (`a! == b` vs `a != b`) instead of dumping the expected-token soup; `x.field()` on a closure-valued field steers to `(x.field)()`; and multi-file diagnostic publishing dedupes across dependents, so fixing a shared module clears its dependents' stale squiggles in one pass.

## v0.8.0 — 2026-07-16

**Diagnostics got a standard — and every one of the compiler's 180 diagnostics was audited against it.** The rules: anchor at the narrowest span that identifies the problem, in code you wrote; speak your vocabulary; name the fix when it's unambiguous; and never bury a root cause under its own consequences. What the audit shipped: "cannot find" errors now steer to the import when the name uniquely belongs to a module (`cannot find type 'JsonValue' — import it first (\`import std::json::JsonValue;\`)`); a conflict with an inferred type points at where the inference happened — the closure's first call, the variable's initializer — as a second label at that exact spot; "has no method" anchors at the method name instead of the argument list; an error inside macro-generated code anchors at the attribute that generated it, in your file; and the near-empty "could not be resolved" residuals only appear when they're the lone signal instead of trailing a real error.

**`///` is the doc-comment syntax.** Hover surfaces `///` blocks; a plain `//` comment is an implementation note and stays private. The standard library is documented with it — hovering `now()`, `format`, or any std function shows its docs in the editor.

**The editor understands the code, not just the text.** Semantic highlighting comes from the analyzer: a generic parameter at its use site, a macro name sharing a trait's name, a method call versus a field read, a module qualifier — each colors by what it *is*. Hover on a constant shows its evaluated value (`SIZE: i32 = 64`), signatures render their `context` clauses, and `[` before an item completes the registered macro names, derives included. Unsaved edits were already visible to dependent files as of v0.7.0; the editor now reads as precisely as it recompiles.

## v0.7.0 — 2026-07-16

**Expression lifting: a bare `?` lifts the whole expression.** Where `?.`
continues a member chain, `?` on its own lifts the rest of the surrounding
expression — `count? * 2`, `deadline? < now()`, and the two-receiver form
`price? + tax?`, which is good only when every receiver is and
short-circuits left to right (a receiver after a `None`/`Err` never runs,
like `&&`; on `Result`, the first error wins and every receiver shares one
error type). The lift stops at natural boundaries — call arguments, struct
fields, parentheses — and a `?` that lifts nothing, or would turn an `if`
condition into an `Option<bool>`, is an error with a steer. `?.` chains are
unchanged. `Option`/`Result` only for now; lowers to plain branches, no
closures.

**Fixed arrays round out: `.len()` and destructuring.** `arr.len()` folds to
the constant (the length lives in the type; a side-effectful subject still
evaluates, exactly once). `let [r, g, b] = rgb;` destructures — irrefutable,
element count checked against the type, nesting arrays and tuples freely,
and it works in parameter position (`fun sum([a, b]: [i32; 2])`). Elements
come out as value copies, like everything else.

**Conditions are type-checked now.** `if 5 { .. }` used to compile and
branch on JS truthiness — an `Option` condition always took the branch.
Every `if`/`for` condition must now be a `bool`, spanned at the condition.

**Two soundness holes closed.** An unannotated `Map::new()` never grounded
its key/value types, so mixed-typed inserts compiled and ran — a binding
whose type keeps a callee's parameters now demands an annotation. And a
derive's internal imports leaked into the deriving module (`JsonValue`
resolved with no import after `[derive(Json)]`) — expansion imports are
scoped to the expansion now.

**Editor and diagnostics.** Unsaved edits propagate to dependent files
immediately (analysis reads open buffers, not disk). A conflicting call on
an unannotated closure names the first call that fixed the parameter's
type. A heterogeneous list literal (`[1, "x"]`) is rejected instead of
silently typing by its first element.

## v0.6.2 — 2026-07-15

**Two generic miscompiles fixed.** A `&mut T` view resolving to `bool` through
a generic, and integer division / bitwise ops on `i32`/`u32` through a generic,
silently did the wrong thing: the boolean write-through was a no-op, `i32`/`u32`
division skipped its truncation (`7 / 2` came out `3.5`), and a `u32` shift used
the signed operator. Both were monomorphization-time classifications that dropped
their verdict for the native-JS types — concrete code and every other integer
width were already correct. Found by an audit after v0.6.1's `&mut bool` fix.

**`!` guides you to convert errors.** `!` returns a failure as-is, so the error
types must match; when they don't, the compiler now points at the fix instead of
calling it unsupported — `.map_err(…)` to change a `Result`'s error, `.ok_or(err)`
to turn an `Option`'s `None` into one. Conversion stays explicit (no hidden
`From` behind the operator), by design.

## v0.6.1 — 2026-07-15

**`&mut bool` write-through, fixed.** A writable view of a boolean *local* —
`let v = &mut flag`, or passing `&mut flag` to a function — silently did
nothing; the write never reached the original. v0.6.0 introduced `&mut bool`
views but boxed only number and string locals, so a boolean's backing cell
was missing and the write landed nowhere. Views of boolean *list elements*
and *struct fields* were already correct; this fixes the bare-local case,
the `v = !*v` toggle included.

## v0.6.0 — 2026-07-15

**Map and Set key by value.** A struct, enum, or `List` works as a key
once it derives `Hashable` (`[derive(Hashable)]`) — two equal values are
the same key, and a freshly-built equal key finds the entry a stored one
made. Scalar keys (`i32`, `str`) still work directly. Hand-write
`impl Hashable` to key by a subset of fields, or to build your own
hash-keyed structure on the `Hash` value the trait returns.

**Decoding validates.** A generated `from_json` returns
`Result<Self, str>` and checks the shape of what it is handed — a missing
field, a wrong JSON type, an absent required value — and returns an `Err`
with a reason instead of a struct half-built from garbage. Round-tripping
your own types across the wire or through a file is safe by construction.

**A view crosses to a value explicitly.** Reading a scalar view's value
requires the `*`. `print(v)` for a `&mut i32` used to leak the view's
internal `(base, key)` representation; it now tells you to write `*v`. The
language never silently converts a view to a value — storing one where it
would escape was already an error, and this closes the read half.

**`Option<&mut T>`, built inline.** `match Some(&mut a) { Some(let v) => … }`
constructs a mutable-view option on the spot and writes through it — the
direct form, the conditional `match if c { Some(&mut x) } else { None }`,
and forwarding a `&mut` parameter. It is a transient, so it may view a
local: it never outlives the `match`. Bind it to a `let` and it escapes,
rejected as before.

**`&mut bool` writes through — and toggles.** A writable view of a boolean
now lowers like any other scalar view, so `v = true` reaches the original;
and toggling reads naturally, `v = !*v`. (The toggle also needed a lexer
fix: adjacent prefix operators like `!*`, `!!`, and `-*` were fusing into
one bogus token and failing to parse — a space was the only workaround.)

## v0.5.1 — 2026-07-14

**A type name isn't a value.** `let q = Point;` used to compile, quietly
binding the constructor object; now it's an error that points you at the
fix — construct the type, name a variant, or call a static. This also
closes a trap the v0.5.0 grammar could spring: `if p == Point { … } { … }`
(a struct-literal comparison a user meant, written without parentheses)
parsed `p == Point` against the type object and ran. It now reports
`` `Point` is a type, not a value `` at the name instead of misbehaving at
runtime. Traits, type parameters, and module names get the same check.

## v0.5.0 — 2026-07-14

**Your types order themselves.** `<` `<=` `>` `>=` now dispatch through
`PartialOrd` — implement (or derive) `partial_compare` and the
operators just work, `started < deadline` on instants included. v0.4.0
steered you to calling `lt` by hand; that detour is over.

**Platform checking follows the instantiation.** A generic function is
checked with the types each call actually binds — `save(disk_store)` in
the server entry charges `std::fs` there and only there, while
`save(memory_store)` in the browser entry stays clean. Before, one
colored instantiation could taint every use of the generic.

**Boundaries you can declare: `[platform("browser")]`.** Inference
still colors everything; a fence turns intent into a checked promise —
verified on every compile, for every host the pattern names, libraries
included. Reach outside it and the error renders the chain from the
fenced function.

**Struct literals are operands.** `Point { x = 1, y = 2 } == p`
compares and `Rect { .. }.area()` chains — no more binding to a local
first. Conditions keep the brace for the block (`if Foo { … }` stays a
condition and a block), so a literal in a condition is parenthesized:
`if p == (Point { x = 1 }) { … }`.

**A local module may share a std name.** `pkg::ui` is always your
`ui.vl`; `std::ui` is always std's. Resolution is scoped by the import
root, so naming a module `ui`, `json`, or `io` no longer collides with
— or silently loses to — the standard library. (`pkg::` also no longer
accidentally aliases std modules you never wrote.)

**Hover tells the whole story.** The editor now renders the full
declaration — signature with parameter names, generics with their
bounds, struct and enum bodies, an `async` prefix when inference adds
one — plus the `//` doc block above the item, its `[platform]` fence,
and the inferred platform requirement with its via-chain.

Also fixed and improved:

- Impl binders: a `type T` binder impl declared before the subject's
  other impls no longer misresolves, and binders in trait-argument
  position (`impl X with Wire<type F>`) register and dispatch.

## v0.4.0 — 2026-07-14

**Platform checking moved from imports to reach.** A build may import
any module; what's checked is what your entry can actually *run into*.
Every function — and now every module-level `let` — carries an inferred
platform requirement, and a browser build that reaches `std::fs` fails
with the whole call chain (`main → boot → load → exists (std::fs)`),
anchored at your call site. Since imports stopped being the boundary, a
service can live next to its resources — the database, the filesystem —
and the client imports the generated stub from that very module; the
injected-closure ceremony is gone. The editor shows all of it live:
violations as you type, and hover tells you what a function requires
and via which path it got there.

**One package, many entries.** A client + server app no longer needs
three packages. Declare two entries in one `[package]` —

```toml
[entry.client]
target = "browser"

[entry.server]
```

— and `vilan build` compiles each for its own target into
`dist/<name>.js` (browser bundles first, so a serving entry finds them
fresh), `vilan run` starts the node entry, and `vilan check` checks
them all. Packages can also depend on each other by path, so the
multi-package shape still scales when you want it. The legacy
`[server]`/`[client]` manifest form is retired; the error names the
replacement. The docs walkthrough app is rewritten in the
single-package shape — its service holds its database directly.

**Module initializers are honest.** A top-level `let` runs iff
something reachable references it — the same rule emission uses — so a
dropped binding's callees (and their `import … from "node:…"` lines,
which previously leaked into every browser bundle and broke it at
module parse) never emit. And an initializer that calls an async
function is now a clean compile error instead of a value that is
secretly a promise.

**Comparisons type-check.** `true < 3`, `1 == "a"`, and mixed-width
typed operands used to compile into coercing JS comparisons; they are
errors now. A bare integer literal still adapts to its peer
(`stamp < 1000` stays fine on an `i53`). Ordering a user-defined type
errors honestly — `PartialOrd`'s operator dispatch isn't wired yet, so
the compiler steers you to its `lt`/`le`/`gt`/`ge` methods rather than
emitting a JS object comparison that is always `false`.

**Tuples have positional access.** `pair.0`, `pair.1`, chains like
`nested.0.1`, and assignment through `mut` bindings — all over the
tuple's flat storage, so a nested write mutates the tuple, never a
copy. Destructuring is no longer the only way in.

Also fixed and improved:

- Iterator-protocol `next()` calls, indexing subjects, destructuring
  subjects, and functions passed as values are now all visible to
  platform checking and async inference — each was a blind spot that
  could hide a platform requirement or an await.
- Two build units writing the same `dist/<name>.js` are rejected at
  build instead of silently overwriting each other.
- `vilan upgrade` prunes stale materialized-std cache directories after
  a successful swap.
- `[macro]` in a manifest no longer warns as an unknown key.
- `std::time`'s documented instant comparison was wrong at runtime
  (`started < deadline` always produced `false`); the docs now use
  `lt` and the compiler rejects the old form.

## v0.3.0 — 2026-07-13

**The toolchain updates itself.** `vilan upgrade` finds the newest
release, verifies its checksum, proves the downloaded binary runs, and
swaps `vilan` and `vilan-lsp` in place; `vilan upgrade --check` only
reports. This is the CLI's one network touchpoint, and it runs only
when you ask. (v0.2.0 installs predate the command — re-run the install
script once to pick it up; it updates in place.)

**Rpc handlers can await.** An `[rpc]` method body can now call
`sleep_for`, another service, or any async API. The reply is sent when
the body finishes, and the wire turn holds across the awaits — signal
writes before and after a suspension still reach every client as one
coalesced update beside the reply.

Also fixed and improved:

- No-argument `[rpc]` methods previously ran outside the wire turn, so
  each of their signal writes was broadcast as its own update. They now
  batch exactly like argument-taking methods.
- The VS Code extension finds the language server in `~/.vilan/bin`, so
  a `vilan upgrade` reaches the editor with no extra step.

## v0.2.0 — 2026-07-13

The first public release.

**The toolchain is self-contained.** The `vilan` binary carries the
standard library inside it and materializes it on first use — download
one file (plus `vilan-lsp` beside it) and `vilan run hello.vl` works
from any directory, with no checkout and no configuration.
`vilan --version` reports the exact build.

**What's in the box:**

- The language: value semantics (assignment copies), no `null` and no
  exceptions (`Option`/`Result` with `!` and `?.`), implicit `await`,
  second-class views with compile-time invalidation checks, generics,
  traits, enums with payloads, pattern matching, and a macro system.
- `std`: collections, strings, sized numerics (`i8`–`u53`, `f32`/`f64`),
  json, time, random, crypto/jwt/base64, fetch, fs/http/process (node),
  dom/storage (browser) — platform-layered, checked at compile time.
- Fine-grained reactive UI (`std::reactive`, `std::ui`): signals bind to
  individual DOM properties; no virtual DOM; automatic cleanup; a typed
  enum-based router; compile-time styling.
- The service layer: one struct is the client/server contract —
  `[expose]`d signals mirror live to every client, `[rpc]` methods are
  typed calls, the wire contract is hashed and checked at connect, and
  reconnects resync automatically.
- The tools: `vilan build / check / run / fmt / test` (all with
  `--watch`), a language server (diagnostics, hover, go-to-definition,
  references, rename — into `std` too), and a VS Code extension,
  prebuilt as a `.vsix` on every release.
- The book: a JS/TS-developer-first guide from
  [Coming from JavaScript](https://vilan-lang.org/docs/tour/coming-from-javascript.html)
  through a full-stack walkthrough app, plus a language spec — every
  example compiled by CI.

Install:

```sh
curl -fsSL https://github.com/vilan-lang/vilan/releases/latest/download/install.sh | sh
```
