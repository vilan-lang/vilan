# The design language — one visual system for site, playground, and docs

> Status: **RATIFIED 2026-08-18 as recommended** ("Go with the
> recommendations on both papers") — §3's five answers stand: Q1 the
> hero survives, rebuilt on the tokens, fenced as the one indulgence;
> Q2 role tokens now, the light theme lands with K6's docs port; Q3
> CommitMono for all code surfaces — **with the owner's own settings,
> recorded in §2.3**; Q4 the editor stays CodeMirror 6; Q5 kolt's token
> vocabulary adopted verbatim. Adoption is sliced in the tracker (§K).
>
> §2.5 (the light variant) added 2026-08-19 by K6 S3 — the value
> table Q2 deferred to the docs port; the sections after it shifted by
> one (the playground fixes are §2.6, the editor ruling §2.7).
> §2.6 (the generation) added 2026-08-20 by K10/K15 — the sections
> after it shifted again (the playground fixes are §2.7, the editor
> ruling §2.8).
>
> Prior status: DRAFT 2026-08-18.
>
> Filed from the owner's 2026-08-18 cleanup list, item 3: the web
> surfaces should read
> like Zed and kolt — "clean yet utilitarian. They show power through
> their design language." Everything cited from kolt was read off its
> `visual-overhaul-2` branch (the owner's designated reference — the
> working tree is mid-conversion to the vilan framework and its visuals
> are not authoritative), via `git show`, no checkout. Tracker home:
> backlog-2026-08-18.md §K5; the playground UX fixes it frames are
> K1–K4 and K9–K10.

## 1. The references, read out of the tree

### 1.1 kolt (`visual-overhaul-2`, tip b0a140e; strict descendant of `visual-overhaul`)

The settled part is a **semantic role system**, not a swatch list
(`client/src/index.css:17-39`, five themes declared per
`[data-theme]`): `up-*` (text, bright/normal/dim + info/caution/error),
`down-*` (surface layers, bright/normal/dim + hover/active overlays +
semantic fills), `stroke-*` (hard/soft hairlines), `primary` (one
accent). Dawn-dark, the default: primary `#E4A362` warm amber on
`#282420`; surfaces `#423D37/#37322D/#282420`; text
`#F5CBA0/#D0BCA7/#967F68`; strokes `#5B534D` / 50%-alpha soft. Across
all five themes only caution/error/info hues are fixed (amber-400,
rose-400, emerald-500) — everything else re-skins by role.

The idiom: **flat and dense**. Shadows exist in exactly two places
(menus, modals — floating surfaces); everything inline separates by
background layer and 1px hairline. Type stays in an 11–15px band —
three utilities cover the app (`script-label` 13px, `script-normal`
15px, `script-badge` 11px mono) — Inter for UI, CommitMono for mono.
Paddings run 4–8px; radii 4/6/8px by elevation; motion is 50ms ambient
with deliberate 150–200ms only on drag affordances. (Caveat recorded:
six kolt components still reference the pre-rename token names —
`bg-ink`, `surface-N` — the branch is WIP; the *role vocabulary* is the
settled layer we borrow, not the file state.)

### 1.2 Zed, as principles

Stated as principles, not measurements: flat fields, one accent,
type-forward hierarchy (weight and tone, not size inflation), near-zero
motion, chrome that disappears behind content. Power shown through
restraint and precision. Kolt's extracted system already is this; no
second token source needed.

### 1.3 The current estate

The website is a **moody, gradient-lit brand surface**: ink `#120004`,
blush `#F9DFE7`, ember `#EB682E`, rose `#E5AFD9` (`src/theme.vl:20-24`),
an animated SVG bloom hero on a 44s keyframe (`masthead.vl:100-177`),
display type at 32–48px, Inter body + "Vilan Display" (Raleway 600,
renamed — `masthead.vl:13`). Single dark palette, no theming layer. The
playground editor duplicates the brand hexes by hand in a CodeMirror
theme object (`editor.mjs:157-190`), with a comment conceding the two
must be resynced manually (`code.vl:23-27`). The docs book is styled by
its generator, visually related to neither.

## 2. The proposal

### 2.1 Roles, not swatches — the brand mapped into kolt's system

Adopt the semantic role vocabulary (`up`/`down`/`stroke`/`primary` +
fixed semantic hues) as vilan's design tokens, expressed in `theme.vl`
where the site's tokens already live. The brand palette maps cleanly:
ink family → `down` layers, blush family → `up` hierarchy, ember →
`primary`, rose → the secondary accent. The identity keeps vilan's
warmth; the system gains kolt's discipline — and restyling any surface
becomes a token edit, never a component hunt. Adopting the vocabulary
verbatim also means kolt and the vilan web estate speak one design
language as kolt finishes its vilan migration (§3 Q5).

### 2.2 Two registers, one system

- **Tool surfaces** — the playground, the docs, anything interactive —
  go fully utilitarian: flat, hairline-separated, dense (13–15px type,
  4–8px padding), shadows only on floating surfaces, sub-200ms motion.
  This is the Zed/kolt register the owner asked for.
- **Marketing surfaces** — the landing hero — may keep the bloom art
  and display type as the one deliberate indulgence, *built from the
  same tokens*. Whether it stays is §3 Q1, the owner's call.

### 2.3 Typography and theming

Keep Inter (already the body face). Adopt **CommitMono** (kolt's mono,
OFL-licensed, self-hostable) for all code surfaces — the playground
editor and every code block — replacing the system mono stack. "Vilan
Display" stays a marketing-register face. Structure the tokens for
theming from day one (roles make it nearly free); ship the dark brand
theme as default plus **one light theme** for docs readability (§3 Q2).

**The CommitMono spec (ratified 2026-08-18, the owner's own editor
settings — the canonical form for every vilan code surface):**

- Face: **CommitMono V143**, the *variable* font, self-hosted
  (`CommitMono-VariableFont` woff2 — kolt's copy under
  `client/src/public/font/` on `visual-overhaul-2` is the reference;
  the `@font-face` declares the full weight range so the variable axes
  are live — the owner's `editor.fontVariations: true`).
- Features, all on: stylistic sets **ss01–ss05** and character variants
  **cv04, cv06, cv08** — in CSS,
  `font-feature-settings: "ss01", "ss02", "ss03", "ss04", "ss05", "cv04", "cv06", "cv08";`
  (kolt's `index.css` already carries exactly this set, which is how the
  study first read it off the tree). Note that ss01–ss05 include
  CommitMono's ligature/alternate sets, so this is also the ligature
  ruling: **on**, per the owner's `editor.fontLigatures`.
- One definition, in `theme.vl`'s token block, generated outward to the
  CodeMirror theme (K10) and the docs stylesheet (K6) — never
  hand-duplicated.

### 2.4 One token source, generated outward

The hand-sync between `theme.vl` and the editor's hardcoded theme
object dies: the site is a vilan program, so emit the CodeMirror theme
(and anything else that needs raw values — the docs stylesheet once K6
ports it) from `theme.vl`'s constants at build time. Filed as K10; the
mechanism as actually shipped is §2.6.

### 2.5 The light variant (K6 S3, 2026-08-19)

The light theme is the brand inverted — **ink on blush** where the
dark theme is blush on ink — built with the ladder discipline
`theme.vl:38-67` explains for the dark values, and it is the shared
source for both light surfaces: the book's `vilan/docs/theme/css/variables.css`
(`html.light`) and the site's `theme.vl` light block. The dark column is
`theme.vl` as of `vilan-website@6e549d2` (`src/theme.vl` last touched at
`8c98bbc`), copied, not re-derived; the light column is new here. Every
contrast is WCAG relative-luminance, stated the way `theme.vl:50-61`
states the dark ones.

**The ground is the brand's own blush, exactly as the dark ground is
the brand's own ink.** `down-dim` = `#F9DFE7`, `up-bright` = `#120004`.
That symmetry is what makes this an inversion rather than a second
palette: ink on blush is the same 16.3:1 that blush on ink is, and the
two ladders below step away from their ground with the same shape the
dark ladders have. Raised surfaces go *lighter* in both themes — the
`down` ladder always ascends in luminance from the ground (kolt's
`dawn-light` does the same: `down-dim` is its darkest surface), so
`down-bright` stays the floating surface and a hairline stays a
hairline.

| role | dark (`theme.vl`) | light | how the light value was derived |
|---|---|---|---|
| `down-dim` | `#120004` | `#F9DFE7` | the brand blush — the ground, un-tinted |
| `down-normal` | `#1B060D` | `#FBE7ED` | blush toward white, +2/+8/+6 — the panel; 1.06:1 against the ground (dark: 1.05:1) |
| `down-bright` | `#28101A` | `#FDF3F6` | one more step, larger, +2/+12/+9 — raised reads as raised; 1.09:1 against the panel (dark: 1.09:1) |
| `up-bright` | `#F9DFE7` | `#120004` | the brand ink — **16.3:1** on the ground (dark: 16.3:1) |
| `up-normal` | `#D8BEC8` | `#3B262D` | ink pulled toward the blush along the same warm pink (hue 340°) — **11.2:1** (dark: 11.8:1) |
| `up-dim` | `#9A7F8B` | `#6A535B` | one more pull — **5.6:1** (dark: 5.6:1); the dim tier clears 4.5:1 on every surface (5.9:1 on the panel, 6.4:1 raised), so nothing in the hierarchy is decorative-only |
| `down-hover` | `rgba(255, 255, 255, 0.06)` | `rgba(18, 0, 4, 0.06)` | ink at the same alpha: white at 6% on ink is a 1.11:1 step, ink at 6% on blush is 1.13:1 — the same perceived hover |
| `down-active` | `rgba(255, 255, 255, 0.10)` | `rgba(18, 0, 4, 0.10)` | likewise 1.23:1 / 1.24:1 |
| `stroke-hard` | `#402C32` | `#CFAFBA` | the visible hairline: blush deepened until it sits 1.59:1 off the ground (dark: 1.58:1) |
| `stroke-soft` | `rgba(64, 44, 50, 0.5)` | `rgba(207, 175, 186, 0.5)` | the hard hue at half strength, as in dark |
| `primary` | `#EB682E` | `#AE3611` | ember, deepened: at its dark value ember reads 2.6:1 on blush and cannot be a link; at `#AE3611` it is **5.0:1** on the ground, 5.3:1 on the panel — same hue, a weight that carries text |
| `primary-on` | `#120004` | `#F9DFE7` | the inversion again — blush on the light primary is 5.0:1, ink on the dark primary 6.4:1 |
| `accent` | `#E5AFD9` | `#922A7C` | rose, deepened the same way (hue 313° kept): the dark rose is 1.5:1 on blush; `#922A7C` is **5.9:1**, which is what code strings need |
| `up-info` | `#F4F4F5` (zinc-100) | `#3F3F46` (zinc-700) | see the semantic note below — 8.3:1 |
| `up-caution` | `#FBBF24` (amber-400) | `#92400E` (amber-800) | 5.7:1 |
| `up-error` | `#FB7185` (rose-400) | `#BE123C` (rose-700) | 5.0:1 |
| `down-info` | `#10B981` (emerald-500) | `#10B981` | fixed |
| `down-caution` | `#FBBF24` (amber-400) | `#FBBF24` | fixed |
| `down-danger` | `#FB7185` (rose-400) | `#FB7185` | fixed |
| `tint-callable` | `#F0A886` | `#7F260D` | the code palette's one non-role value: primary pulled toward `up-bright` (dark: toward blush; light: ~30% toward ink) — 8.1:1 on the panel (dark: 9.9:1) |

**The semantic hues, precisely.** Kolt fixes emerald/amber/rose across
its themes and only ever prints the `up-*` word *on* the `down-*` fill
(its light theme sets all three `up-*` to white). Vilan spends `up-error`
and `up-caution` as standalone text on the ground
(`playground_page.vl:362-368`), and on blush the 400-weight hues are
unreadable — amber-400 is 1.3:1, rose-400 2.1:1. So the rule this
paper fixes is: **the hue is fixed; the `down-*` fills keep their
Tailwind 400/500 values in both themes (they are borders and 6–7%
washes, never text); the `up-*` text tier steps down the same
Tailwind ladder until it clears 4.5:1 on the light ground** — rose-700,
amber-800, zinc-700. Same family, a weight that reads. The playground's
diagnostics pane needs nothing else to re-theme.

**What the code surface inherits.** `code_palette` (`theme.vl:185-206`)
is all roles plus alpha pulls on them, so it re-themes on its own — with
one stated exception: `--code-comment` is `up-bright` at 0.5 in dark
(4.4:1 on the panel) but ink at 0.5 on the light panel is 3.7:1, so
**the light theme takes 0.6** (5.3:1). `--code-attr` keeps primary at
0.65 in both (3.2:1 dark, 2.9:1 light — a deliberately dimmed register,
unchanged in kind). The hljs mapping in the book uses the same slots the
CodeMirror theme names (K10), so which role plays "keyword" is still a
single edit.

**`color-scheme` is honest on both.** The book declares
`--color-scheme: light` on `html.light` and `dark` on `html.navy`
(mdBook's `general.css` applies it to `:root`); the site's light block
must flip `app.html:6` the same way when it lands.

### 2.6 The generation (K10/K15, 2026-08-20)

§2.4 promised one token source, generated outward; this section is the
mechanism, designed against the estate as it actually stands. Two of
the "three mirrors" the tracker names were already dead when this lane
opened: the CodeMirror theme stopped being a mirror at K5 slice 2
(`editor.mjs`'s theme object names only `--code-*` slots minted by
`theme.vl`'s `code_palette` — a token edit re-themes the committed
bundle with **no rebuild**, which is strictly better than a generated
theme module, so none is built), and the exported `chrome/header.css`
is *generated* from `theme.vl` on every build, not hand-synced. The
one manual mirror left is the book's `variables.css`. What this
section adds: the palette gets one home *inside* `theme.vl`, the
chrome leg publishes it in every form a consumer needs, and the book's
mirror is held to the generated values by a suite gate instead of by
eyes.

**One home inside `theme.vl`.** The themed custom properties move into
a single const table, `themed_values` (`ThemedValue { name, dark,
light }` — every `token()` value, `tint-comment`'s per-theme alpha
pulls included, stated as the strings they compile to). The typed
`Color` consts the site consumes keep their names and their values:
`token(name)` becomes a lookup into the table, and a name the table
does not carry **fails the build at const eval** — so the table and
the consts can never disagree, and no hex is stated twice. The
`:root{…}@media (prefers-color-scheme: light){…}` declaration string
is factored into `themed_declaration`, the one formatter both
`token()` and the generator below call — parse-free reuse of the same
text the compiled stylesheets carry, never a re-statement.

**The generator.** The chrome leg (`src/chrome.vl` — already the
book-facing export, already run by every deploy) renders the table
into two more files beside `header.html`/`header.css`:

- `chrome/tokens.css` — the whole themed palette in BOTH forms
  consumers need: the site's own `prefers-color-scheme` form (the
  exact `themed_declaration` strings), then the class-scoped form on
  `html.light` / `html.navy` — the book's picker model, which beats
  the `:root` form by specificity wherever both load. Nothing links it
  yet by design (the book stays standalone, below); it is the palette
  as a served artifact, for the docs port (K13), kolt, or any consumer
  after them.
- `chrome/tokens-fixture.css` — the class-scoped form again, under a
  provenance header, byte-for-byte what the vilan repo commits as
  `vilan/docs/theme/css/tokens-fixture.css`. Regeneration is a copy,
  never an edit:
  `vilan build . && node dist/chrome.mjs export/chrome && cp
  export/chrome/tokens-fixture.css
  ../vilan/vilan/docs/theme/css/tokens-fixture.css` (paths from the
  website repo root; deploy does not stage this file).

**The book's gate.** `variables.css` keeps its role declarations as
the LOCAL source — a plain `mdbook build` must stand alone, so the
book fetches nothing — but the mirror stops being trusted:
`crates/vilan-cli/tests/book_mirrors.rs` parses every role-token
declaration in `variables.css` per theme (the `.navy` block, the
`.light` block, and the no-script `@media` copy — the file's own
third copy, which could drift internally) and holds each value
byte-equal to the committed fixture. Three fixture rows are
deliberately not mirrored and are allowlisted by name with the
reason: `tint-comment` (the book states the same fact as
`--code-comment-alpha` + `color-mix`, so the alpha is the shared
truth, not the composed color) and `shadow`/`art-error` (the art
never renders in the book). Drift between `theme.vl` and the book now
turns the suite red: a `theme.vl` change regenerates the fixture (the
copy command above), and the red test then forces `variables.css` to
move in the same change-set.

**The codec, same discipline (K15).** The share codec exists once on
the website side: `playground/codec.js` (encode/decode/deflate/
inflate), imported by the editor bundle — esbuild inlines it, so the
editor build fails if the module is missing. The book's `vilan.js`
keeps an inline copy of the two functions it needs (`encodeBase64Url`,
`deflate`) rather than importing the module: `vilan.js` is a classic
script, so only a dynamic `import()` could reach `/playground/codec.js`
— and that would make every ▶ link on every locally built book vanish
silently (the links are built eagerly at page load and today need no
server at all), a strictly worse failure than the run button's
click-gated, panel-explained dependence on `/playground/worker.js`.
So the copy is pinned instead: `vilan/docs/theme/codec-fixture.js` is
the committed copy of `codec.js`, and the same `book_mirrors.rs` holds
`vilan.js`'s two functions byte-equal (modulo the IIFE's one
indentation level) to the fixture's, with the fixture's four functions
required present so the gate cannot go vacuous.

### 2.7 The playground, made honest (the K-fixes this paper frames)

- **K1** — the playground joins the site nav: today `top_bar()` renders
  exactly three links, none of them the playground
  (`masthead.vl:86-90`), and the footer's three columns skip it too
  (`page.vl:680-708`).
- **K2** — dirty tracking: the selector's value is written once at init
  (`wirePicker`, `editor.mjs:375-390`) and nothing revisits it; the
  update listener only persists and lints (`editor.mjs:347-352`). The
  selector should mark the selected template as modified once the
  buffer diverges.
- **K3** — reselecting the active template must reset it: the handler
  hangs off the native `change` event (`editor.mjs:381-385`), which
  never fires on a same-value pick, so "load the pristine template
  again" is unreachable.
- **K4** — confirm before replacing: `pick` replaces the whole document
  unconditionally (`playground.vl:204-214` →
  `editor_set_doc(example_source(name))`) — a dirty buffer dies with no
  confirmation.
- **K9** — autocomplete is imported, never wired: the editor bundles
  `@codemirror/autocomplete` but registers no completion source
  (`editor.mjs:331-362` — `closeBrackets` only). Either wire a real
  source from the wasm analyzer or drop the import; shipping the
  dependency and not the feature is the worst of both.
- Noted, not a defect: diagnostics fan out to two consumers (the lint
  gutter via `applyEditorDiagnostics`, `editor.mjs:409-428`, and the
  DOM list via `playground.vl:266-344`) from one worker payload —
  fine, but the restyle should treat them as one designed system.

### 2.8 The editor stays CodeMirror 6

Recommendation, firm: **stay on CM6.** The study found the restyle is
"small, mechanical, well-contained" (one theme object, one tokenizer,
a tight extension list); Monaco cannot wear a bespoke skin — it always
reads as VS Code, the opposite of this paper's goal — costs roughly an
order of magnitude more bundle, and is weak on mobile. Monaco's real
advantage is a richer completion story, which buys nothing today
because no completion source is wired at all (K9); if K9 later wants
LSP-grade completion, CM6's autocomplete API is already in the bundle.

## 3. Open questions — all RULED 2026-08-18, each as recommended

- **Q1 — the hero**: does the bloom/gradient marketing register survive
  on the landing page, or does utilitarian go wall-to-wall?
  *Recommendation: it survives, rebuilt on the tokens — one indulgence,
  clearly fenced.*
- **Q2 — theming**: dark-only, or dark default + one light theme?
  *Recommendation: role tokens now, light variant with K6's docs port —
  docs are where light mode earns its keep.*
- **Q3 — CommitMono** for all code surfaces? *Recommendation: yes.*
- **Q4 — editor stack**: ratify §2.8 (stay CM6)?
- **Q5 — token vocabulary**: adopt kolt's `up`/`down`/`stroke`/`primary`
  names verbatim, or rename for vilan? *Recommendation: verbatim — the
  system is proven, and one vocabulary across kolt and the web estate
  compounds.*
