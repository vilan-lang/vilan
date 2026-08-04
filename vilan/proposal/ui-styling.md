# UI styling — typed atomic styles, compiled

Status: **CORE SHIPPED 2026-07-10; TAIL SHIPPED 2026-08-04, including the
value types** — §0bis is the live status and supersedes the "Remaining" list
at the end of this paragraph, which is kept as the historical record of what
the core's authors expected to be left. What remains open: critical CSS (A7)
and liveness-tied dead-style elimination (G2), both entangled elsewhere. The
property tail's VALUE-TYPE half closed with §0bis.3.

The original core record, unedited: **CORE SHIPPED 2026-07-10** — `std::style`
(same day as the whole
prerequisite stack: `const`, the asset channel, this). Shipped: `Style` as a
slot map (`media:pseudo:property` → class + declaration), the builder chain
(~30 properties), `Color`/`Length`/`space` tokens with `:root` var emission
(per-use lines, deduplicated — no theme-emission coordination needed),
pseudo/breakpoint/dark conditions with one-level stacking, `raw`, pure-vilan
djb2 class hashing (content-addressed; the corpus and the example produce the
SAME class name for the same rule — cross-program determinism proven),
`View.styled` + `View.style_var` + the `set_style_property` DOM extern, 12
pins, corpus `style.vl` with **both `.js` and `.css` goldens**, and a styled
`counter` in the reactive-ui example emitting `app.css`. Implementation
findings: condition combinators re-emit the inner chain's declarations under
their selector (the inner's base rules also emit — the recorded
over-approximation); the interpreter's `new Map`/`new Set` host arms learned
their entries argument (serialized const Maps arrive populated). Remaining,
recorded: `bind_styled(Signal<Style>)` (compose via `bind_class(sig.map(..))`
meanwhile), dark×pseudo stacking, the html `<link>` scaffold, `vilan fmt`
chain splitting, the property-list long tail, and the first draft's items
(critical CSS with A7, dead-style elimination via liveness).

The first draft proved styles
through a macro DSL; this revision — the syntax refinement, settled with the
user — made styling **expression-flavored**: an ordinary typed API riding
the general `const` compile-time-evaluation feature
(`proposal/const-eval.md`), which this system is the forcing use case for.
The semantic layer (atomic lowering, last-wins merge, custom-property
theming, the deduplicating asset channel) carries over from the first draft
unchanged; what changed is *who evaluates it* — the language, not a macro —
so hover, go-to-def, typed diagnostics, functions, impls, and operator sugar
all work out of the box, with no DSL toolchain to build. (The macro draft
survives in git history; its §8 rejection rationale now lives here, inverted.)

## 0bis. Status — the tail, reconciled 2026-08-04

The status paragraph above was written the day the core shipped (2026-07-10)
and never reconciled; it predates the STATUS convention. Every piece it lists
as "Remaining" was re-verified against the tree at `e662973`. The table is the
record; the sections it points at carry the design.

| Piece | Verdict | Evidence |
|---|---|---|
| `bind_styled(Signal<Style>)` | **VERIFIED OPEN → SHIPPED 2026-08-04** | Both twins gained it beside `bind_class`: the browser one is an ambient `effect` (not `style_var`'s leaking `let _sub`), the process one reads once. 3 pins — the SSR read-once pin, a const-only pin (a style built at the binding site is still refused), and the SSR differential's shared component, which now carries a `bind_styled` node so both twins are compared byte-for-byte on it, plus a browser-only second output line that fires a click and re-reads the class (the reactive half the mount-time tree cannot see). Both new legs proven non-vacuous by planting. Verification evidence: `bind_styled` appeared nowhere outside three prose lines (this file, `backlog-2026-07-18.md`, `backlog.md`). Both twins carried `bind_text`/`bind_class`/`bind_attr`/`bind_value`/`bind_draft`/`bind_each` and no `bind_styled`; `guide/styling.md` documented `bind_class` as the standing workaround. |
| dark×pseudo composition | **VERIFIED OPEN → SHIPPED 2026-08-04** | Implemented per §0bis.2: the condition grammar, `dark` with its own body, `pseudo` rejecting the reverse order with a message naming the fix, `media` composing for free. **Every pre-existing class name is byte-identical** — the corpus `.css` golden grew ten lines and changed none. 9 pins (composition, all-three-axes, the four refusals, and the two pre-existing nesting guards that shipped in 2026-07-10 and had never been pinned at all — `grep "cannot wrap" crates/` returned nothing), plus the corpus golden's composed selectors in bytes. Non-vacuity planted. Verification evidence: `Style::pseudo` panicked on `parts[1] != ""`, refusing **both** nesting directions (`dark(hover(..))` and `hover(dark(..))`). Cause was structural, not a missing case: the slot key `media:pseudo:property` has no third position and `dark` occupied the pseudo slot. `style.vl`'s semantics were unchanged since `ad691a7` (2026-07-10) — the only later commits are a reflow and doc comments. |
| the html `<link>` scaffold | **VERIFIED OPEN → SHIPPED 2026-08-04** | Both `vilan init` browser-bearing templates carry the `<link>`, and both scaffolds now build a style so the link is live (the `cfeb585` move — the todo example dropping handwritten CSS for `std::style` — applied to the scaffolds); the fullstack template also gained the `/client.css` route, guarded with `fs::exists` so deleting every style doesn't crash the server at boot. `examples/reactive-ui` gained the link it was missing. 3 pins: init asserts emitted-AND-linked for the browser template, and asserts the served page links `/client.css` AND that the route returns the rules for the fullstack one; the examples gate grew a GENERAL rule — every emitted stylesheet must be linked by one of the example's pages, stated over what the build produced so a new example is covered the day it lands — proven non-vacuous by deleting reactive-ui's link. Verification evidence: emission shipped: `write_assets` (`crates/vilan-cli/src/main.rs`) writes `<out>.css` on `build`, `run`, `run --watch`, workspace and HMR paths. The *link* half existed only as hand-written bytes in two examples. No template linked it, and `examples/reactive-ui` **emitted `app.css` (pinned in `tests/examples.rs`) while its `index.html` never loaded it** — const styles compiled and then thrown away, the sharpest evidence the hookup was unfinished. |
| `vilan fmt` chain splitting | **VERIFIED SHIPPED** — `9a3d9af`, 2026-07-28 | "vilan fmt splits method chains over 100 columns"; extended by the 2026-08-01 formatter arc (backlog 42–49, notably `bad9510`'s width-independent `})` seam rule). `formatter.rs` carries `LINE_BUDGET = 100`, `is_breakable_chain`, and style-chain pins including a literal `const style().display(..).flex_direction(..).gap(..)` case. Probed with the worktree binary: a 128-column style chain splits one link per line. **The "(or preserve)" half of §1's note is rejected by design, not open** — the formatter has one canonical output and no width knob, and `an_under_width_hand_split_chain_collapses` pins the rejoin. §1's note is corrected in place. |
| the property long tail | **VERIFIED OPEN → BOTH SLICES SHIPPED 2026-08-04** | 28 property methods → **46**, plus `Length::em`/`vh`/`vw`/`calc` and the `WhiteSpace`/`UserSelect` enums. Exactly the ≥5-site head of the sweep, with two named exceptions (below). 4 pins, table-shaped so each method's exact declaration is asserted by name; non-vacuity planted. **The §3b half — the value types — then shipped the same day (§0bis.3):** `Color::rgba`/`.alpha`, the `Gradient` type on the `background-image` slot, `border_none()` and the four border edges, the eight `padding_*`/`margin_*` edges, `Display::InlineFlex`/`InlineGrid`. 8 more pins, all planted (one caught vacuous and rewritten), the corpus `.css` golden **13 lines added and none changed**, and five real `raw` sites in the examples converted with both stylesheets coming out byte-identical. The item CLOSES here; what remains under A8 is only the A7/G2-entangled pieces. Verification evidence: supply was 28 property methods over 32 CSS properties. Demand, swept across the website (the one real consumer — 2926 lines of vilan), the examples, and the docs: **341 `raw(..)` calls against ~350 typed property calls.** The escape hatch was carrying half the styling done in the language. |
| critical CSS | **OPEN, out of this arc's scope** | A7-entangled; still §6 slice 6, proposal-only. Left filed. |
| liveness-tied dead-style elimination | **OPEN, out of this arc's scope** | Rides G2's liveness-tied emission. Left filed. |

Two verification by-products, filed rather than fixed here:

- **`View.style_var` leaks its subscription** (browser twin). It is the only
  reactive `View` method that calls `source.sub(..)` and parks the handle in a
  `let _sub` instead of going through `source.effect(..)`, so the subscription
  is never handed to the ambient owner and outlives its boundary's disposal.
  Every sibling binder (`bind_text`, `bind_class`, `bind_attr`, `show`) uses
  `effect`. Not touched here — it is a reactive-ownership bug, not a styling
  one, and it wants its own pin.
- **`view.class_name(..)` is vapor.** §2.3 and §4 below promise it for
  third-party CSS; the shipped method is `.class(..)`. The prose is corrected
  in place; no API is renamed.

### 0bis.1 The demand sweep (what the long tail actually is)

The 341 `raw` sites are not one tail but two, and the split decides what a
property slice should buy:

- **Properties with no typed method at all** — led by the inset family
  (`top` 22, `left` 20, `right` 6, `bottom` 4, `inset` 3), `font-family` (25,
  and the docs' own canonical example of a gap), `transform` (13),
  `white-space` (9), `text-decoration` (9), `user-select` (8), `flex` (8),
  `grid-template-columns` (7), `letter-spacing` (6), `box-shadow` (6),
  `border-color` (5).
- **Properties that ARE typed but whose value type cannot hold the value** —
  about 120 sites. `background(Color)` bypassed 36 times for gradients and
  `rgba`; `border(Length, Color)` 19 times for `none` and for recolouring
  under `:hover` without restating the width; `width`/`height` 28 times for
  `calc()`; `min_height` for `100vh`; `letter-spacing` for `em`. The root
  cause is two value types, not twenty missing methods: `Length` had no
  `em`/`vh`/`vw`/`calc()`, and there is no per-edge border surface.

**Slice 1, shipped 2026-08-04** — the first group's ≥5-site head, plus the
value-type units that head needs, and nothing else. 18 methods: `flex`,
`grid_template_columns`, `top`/`right`/`bottom`/`left`/`inset`,
`border_color`, `box_shadow`, `font_family`, `letter_spacing`,
`text_decoration`, `white_space`, `user_select`, `transform`, `flex_shrink`,
and `min_width`/`max_height`. Four `Length` constructors: `em`, `vh`, `vw`,
`calc(expression)` (the author writes the arithmetic, not the wrapper). Two
enums: `WhiteSpace`, `UserSelect` — whose `none` is `Off`, following
`Display::Hidden`'s rule about `Option::None` at use sites.

Two deliberate departures from the ≥5 line, both stated rather than fudged:
the inset family is admitted **as a family** (`bottom` 4 and `inset` 3 come in
with `top` 22 and `left` 20 — splitting one CSS concept by a count would be
arbitrary), and `min_width`/`max_height` are admitted as **symmetry**, since
`max_width` and `min_height` already shipped and the missing halves of a
quartet are a hole rather than a tail. `flex_shrink` (4) rides in beside the
`flex` shorthand it decomposes. Everything below the line — `clip_path`,
`animation`, `background_image`, `filter`, `z_index`, `align_self`,
`flex_wrap`, `box_sizing`, `text_transform`, `pointer_events` — stays with
`raw` until it earns a place.

A `str`-valued method (`font_family`, `transform`, `box_shadow`,
`text_decoration`, `flex`, `grid_template_columns`) is not a weaker `raw`: the
property NAME stays checked and completable, and only the value — a font
stack, a transform list, a shadow layer — is CSS text nothing could validate.
That is the same bargain `transition` shipped with in v1.

**What was still open**, and is the bigger half: §3b's ~120 sites where the
property is typed and the VALUE TYPE cannot hold what was wanted —
`background(Color)` bypassed 36 times for gradients and `rgba`,
`border(Length, Color)` 19 times for `none` and non-`solid` styles, `padding`
for two-value shorthands, `Color` for alpha. `Length::calc` and the new units
take a bite out of it; the rest wants a `Color` with alpha, a gradient-capable
background channel, and a per-edge border surface — one slice, designed
together, not a scatter of methods. **Designed and shipped in §0bis.3**
(2026-08-04), whose re-sweep corrected three of the guesses in this paragraph.

§2.3's bargain still holds throughout: the typed surface buys checking and
completion for what people actually write; an exhaustive CSS mirror buys
neither.

### 0bis.2 Design — dark × pseudo composes on one axis

Settled here rather than by owner call: the mechanism follows directly from
what the emitter already does, and the naming question has a precedent.

The slot key stays three fields, but the middle one is a **condition**, not a
pseudo-class. A condition is `""`, a pseudo-class (`hover`), the dark marker
(`dark`), or dark stacked over a pseudo-class (`dark hover`) — the space
mirroring the descendant combinator the selector renders. So:

```
""            .sX{..}
"hover"       .sX:hover{..}
"dark"        :root[data-theme="dark"] .sX{..}
"dark hover"  :root[data-theme="dark"] .sX:hover{..}
```

Three consequences, all deliberate:

- **Existing slot keys are unchanged, so every already-emitted class name is
  unchanged.** Class names are content hashes of `key|declaration`; widening
  the key to a fourth field would have rehashed every rule in every build for
  no user-visible gain, and cross-program determinism is a shipped property of
  this system.
- **Nesting order is `dark(hover(..))`, and the reverse is refused** with a
  message naming the fix. This is the `md(hover(..))` rule generalized:
  conditions nest outside-in in the order the CSS nests them
  (`@media` → the dark ancestor → the pseudo suffix). `md(dark(hover(..)))`
  therefore composes for free — `media` already carries a condition through
  untouched.
- **The composed selector keeps `:` as its first byte**, which is what keeps
  B35's ordering sound. `assemble_assets` has no "base < pseudo < media"
  comparator; that band ordering falls out of ASCII (`.` < `:` < `@`) with a
  numeric override for `@media` min-widths only. A composed dark rule sorts
  into the same band as every other dark rule, and its specificity (0,3,0)
  beats plain `dark` and plain `hover` (both 0,2,0) — so the cascade resolves
  by specificity and never by the sort. This was exactly the class of bug B35
  was; the invariant is now stated where the emitter can be read against it.

The pre-existing tie it does **not** change: `dark(x)` and `hover(y)` on the
same property are both (0,2,0) and are resolved by source order, where `.`
sorting before `:` makes dark win. That is deterministic and defensible (a
mode should not be undone by a state), and `dark(hover(..))` is now the
precise way to say otherwise. Recorded, not altered.

### 0bis.3 Design — the value types (§3b, the tail's second half)

Numbered `.3` because `.2` is the shipped dark×pseudo design; this is the
section the value-type slice was chartered to write.

§0bis.1 estimated this half from the slice-1 sweep's margins. It was
**re-swept site by site before designing**, over the same three consumers
(the website's 341 `raw` calls, the examples' 6, the docs' 2), and the
re-sweep corrected the estimate in ways that changed three of the four
decisions. The counts below are that sweep; each design choice names the
number it answers.

The rule this slice states, and that future value-shaped work follows:

> **One method per CSS declaration the value type can hold, and the method
> NAME carries the arity.** A multi-value shorthand method is minted only
> when no composition of the typed methods that already exist produces the
> same computed result. vilan has no overloading, so arity lives in names —
> but a name that buys nothing over `a().b()` is surface, not expressiveness.

**1. `Color` gains alpha — 58 sites, the single biggest driver of the escape
hatch.** Two constructors, because the sweep found two different needs.
`Color::rgba(r, g, b, alpha)` is the literal (44 of the 58 sites write a bare
`rgba(..)` — a brand palette, not a ramp step), the alpha twin of
`Color::hex`, with its channels range-checked at const time the way `space`
and the ramps are. `.alpha(value)` derives one color from another and is what
the token case needs: it renders `rgb(from {css} r g b / {alpha})`, the
relative-color form, **which keeps the origin a `var(--gray-900)` rather than
resolving it** — so a themed color at 8% is still themeable, and the `root`
declaration rides along untouched. That is the whole reason not to append an
8-digit hex (works only on literals) and the reason not to multiply into a
`color-mix` percentage: `0.07 * 100.0` is `7.000000000000001` in f64 and
would be in the stylesheet, whereas relative color takes the author's alpha
verbatim. Cut, recorded: two sites put a `calc(var(--nav-fade,0) * 0.86)` in
the alpha channel. An `f64` alpha cannot hold a live custom-property
computation and no value type short of a CSS expression tree could; those two
stay `raw`.

**2. A gradient is NOT a `Color` — it is `background-image`.** Answered
honestly and against the first instinct: `background(Color)` is
`background-color`, and no amount of widening `Color` makes a two-stop
function fit a property that takes one color. So a separate `Gradient` value
type and a separate `background_gradient(Gradient)` method writing the
`background-image` slot — a *different* slot from `background`, which is
exactly right, since CSS paints an image over a color and a style may set
both. `Gradient::linear(degrees)` and `Gradient::radial(RadialExtent)` open
one; `.stop(color, percent)` adds a stop; the stops carry their colors'
`:root` lines out to the emitter, so a gradient of ramp tokens themes like
everything else. **Both constructors ship, and the demand is why the charter's
"linear v1" was widened:** radial is 12 of the 16 gradient sites (11 of them
the identical `radial-gradient(closest-side, <alpha color>, transparent)`
glow) and linear is 2 — under the ≥5-site rule linear alone would have been
*out*. Splitting one CSS concept by a count is the arbitrary cut §0bis.1
refused for the inset family; gradients are admitted as a family. Cut,
recorded, with the site that forces each: `at <position>` (1 site, and the
`circle at var(--glow-x)` one needs live variables), multi-layer lists (1
site, 8 layers), `repeating-linear-gradient` (1), and `background-image` as a
data URI (2 sites, ~2 KB of embedded SVG each — not a value type's job).
Those keep `raw`, and `background_image(str)` is deliberately NOT minted:
3 sites is below the line, and minting it now would put a `str` method and a
typed method on one slot for no demand.

**3. `border` gains `none` as a method, not a `BorderStyle` enum — because
the sweep found ZERO non-`solid` borders.** All 17 width-and-color sites are
`1px solid <alpha color>`; the alpha is what defeated them, and decision 1
fixes that with no border work at all. What is left is `none` (3 sites), so
`border_none()` ships and an enum does not: its other variants would be
speculation, and — the load-bearing reason — `border_none()` writes the
**same `border` slot** the shorthand does, so it *replaces* a border set
earlier in the chain under the ordinary last-wins rule. A `BorderStyle` on a
`border-style` longhand would instead emit a second atomic rule racing the
shorthand in the cascade (see the hazard below). Per-edge ships as a family:
`border_top/right/bottom/left(width, color)` — the sweep has 3 `border-top`
and 2 `border-bottom` and zero left/right, and a quartet with two holes is a
hole, not a tail (`min_width`/`max_height`'s precedent). All five bodies go
through one `with_border` helper, and `border`'s own declaration is
byte-identical to what it emitted before.

**4. `padding`/`margin` get the four EDGES and no multi-value shorthand.**
This is the decision the re-sweep reversed. Every multi-value site is
2-value `y x` — 4 of them, zero 3-value, zero 4-value, and `margin` is never
given a shorthand at all — and `padding_y(v).padding_x(h)` already computes
exactly `padding: v h`. By the rule at the top of this section a
`padding_xy(v, h)` would buy one atomic rule instead of four and no
expressiveness, so it is not minted. The real hole is the single-edge
longhands the surface never had: 9 sites (`margin-left: auto` alone is 5 of
them — the flex-push idiom), against a surface that shipped `padding`,
`padding_x`, `padding_y` and nothing else. So `padding_top/right/bottom/left`
and `margin_top/right/bottom/left`, admitted as families, one `Length` each.
A `Sides` value type (`Sides::xy`, `Sides::trbl`) was the alternative and is
rejected: it can only reach `padding` by changing `padding(Length)`'s
signature — churning every call site in the one real consumer — to model
arities nobody writes.

**5. `Display` gains `InlineFlex` and `InlineGrid`.** The smallest member of
this class: an enum *is* a value type, and one that cannot name a legal value
of its property has the same defect as a `Color` without alpha.
`display:inline-flex` has 1 site; `InlineGrid` has none and comes in as the
family half, since `InlineBlock` already shipped and splitting the inline
forms by a count is the arbitrary cut again.

**Cut from this slice, with the trigger that would reopen each.**
`box_shadow` stays `str`: 5 of its 6 sites are one uniform
`x y blur <color>` layer, and what actually defeated them was the color,
which decision 1 now writes — a `Shadow` value type would be minted for one
shape with no second consumer. Reopen it if a shadow wants a *ramp token*
color, which the `str` form genuinely cannot hold. `transition` stays `str`
and is the clearest cut of all: **zero `raw` sites**, 5 typed uses, all five
comma-separated multi-property lists — the value type people would need is
the list they are already writing, and nothing is escaping. `line_height`
keeps its `f64`: 4 sites want a `px` line height, under the line, and
unitless is the correct default besides.

**A hazard this slice found and did NOT fix (recorded, not papered over).**
Atomic longhand and shorthand rules of the same family carry equal
specificity, so `padding(space(4)).padding_top(space(0))` resolves by
*stylesheet order* — which here is the lexical sort over content-hashed class
names, i.e. arbitrary. This is pre-existing (`padding` + `padding_x` has it
today, as does `border` + `border_color`), and the per-edge families widen the
exposure. It is not fixed here because every fix changes rule TEXT for
already-minted rules — doubling a longhand's class for specificity
(`.sX.sX{..}`) is the standard remedy and would rewrite every `padding_x`
rule in every build — and class-name stability was this slice's hard
constraint. The authoring rule goes in the docs instead ("one arity per
family: the box or its edges, not both"), and the remedy is filed for a slice
that is allowed to re-mint names.

## 0. The problem

`std::ui` builds and updates DOM; nothing styles it. Handwritten CSS is the
current answer and an unacceptable one long-term: global names, cascade
surprises, dead rules, styles far from their components.

The best mainstream model — Tailwind-style atomic utilities — earns its keep
with locality, a market-tested design system, and a stylesheet that
*plateaus* (n components share one bounded set of single-purpose rules). Its
chronic pains are one root cause — **styling as strings**: long class
utterances (composition has no names), merges resolved by stylesheet order
rather than authoring order (`tailwind-merge` re-parses strings at runtime to
guess intent), and variant assembly (CVA) paying string-parsing costs per
render because the build discarded the structure the author had.

A compiler that owns the pipeline keeps that structure. Styles here are
**typed values, constructed at compile time, lowered to deduplicated atomic
CSS**; merge is value semantics, not cascade semantics.

## 1. The model

```vilan
import std::ui::style::{ style, space, Color, Display };

let card = const style()
    .display(Display::Flex)
    .padding(space(4))
    .background(Color::gray(50))
    .hover(style().background(Color::gray(100)));

let active = const style().padding(space(6));

view.class(card + active);   // padding resolves to space(6) — LAST WINS, always
```

- **The builder chain is the construction surface** (settled with the user):
  `style()` opens a chain; each property method merges one property in, last
  wins — so the per-property map algebra is unchanged underneath, and calling
  a property method on an EXISTING style is extend-with-override
  (`base.background(blue(600))`). Chosen over free property functions for
  vilan-specific reasons: one `style` import instead of a per-property list
  that grows and collides (`color`, `display` as user locals), and `.`-
  completion over the whole property surface — the discoverability the
  expression-flavored pivot was for. `+` (`impl Style with Add`) remains the
  combinator for NAMED styles (variants). Implementation note, **answered
  2026-07-28 (`9a3d9af`)**: `vilan fmt` splits a chain over 100 columns, one
  link per line. It does not *preserve* a hand-split narrow chain — the
  formatter has one canonical output by design, so an under-width chain
  rejoins.
- **A `Style` value** is a map from property-slot → atomic class name. Each
  `(property, value, condition)` triple lowers to one CSS rule with a
  **content-hashed class name** (never a counter — deterministic across
  builds; readable names under the `debug-names` codegen knob). Program-wide
  line-dedup in the asset channel is what makes the stylesheet plateau.
- **Merge is a record update, not a cascade.** Each property contributes
  exactly one class, so the merged map *is* the resolution — specificity
  fights are structurally impossible. Fully-const merges fold to a
  precomputed map; runtime merges of const styles are a small map union.
  String parsing never happens.
- **Construction happens inside `const` expressions; selection and merging
  are runtime.** This is the load-bearing rule (§3). (`const` is the
  weak-precedence expression keyword of `const-eval.md` — `let card = const
  ..` is the idiom, and ordinary `let` bindings mean no special naming or
  mutability rules for styles.)
- **Variants are just code** — CVA dissolves into the language:

  ```vilan
  let primary = const base.background(Color::blue(600)).color(Color::white());
  let danger = const base.background(Color::red(600)).color(Color::white());

  fun button_style(kind: Kind): Style {
      match kind {
          Kind::Primary => primary,
          Kind::Danger => danger,
      }
  }
  ```

- **Long class strings become names** — ordinary bindings, co-located with
  their component, tree-shaken (F6), composed like any value.

## 2. Tokens, themes, conditions

### 2.1 Tokens: const functions over scales, custom properties underneath

`space(4)` is a const-evaluated function over the scale — **the scale is
data, the validation is const evaluation** (`space(37)` fails the build with
a spanned error; no macro-time property table needed). What it *returns*
distinguishes two token kinds:

- **Themeable tokens** (spacing, colors, typography) resolve to **CSS custom
  properties**: `padding(space(4))` lowers to `.pA3 { padding: var(--space-4) }`
  plus one `:root { --space-4: 1rem }` declaration from the theme. The
  compiler needs token *identities*; values stay a CSS-side concern — so
  re-theming and dark mode are property swaps with zero recompilation, and
  signal-driven dynamic values ride the same channel
  (`width(var("--w"))` + `view.style_var("--w", signal)`).
- **Structural tokens** (breakpoints) resolve to **literal values at const
  time** — media queries cannot read custom properties, and const evaluation
  reads the breakpoint constants directly. The first draft needed a
  compile-time config knob here; `const` dissolves it — breakpoints are plain
  std constants a future theme layer can override like any value.

v1 ships std defaults stolen wholesale from the market-tested scales
(Tailwind's spacing scale, color ramps, type scale). **Color tokens are
namespaced on the `Color` type** (settled with the user): the type must exist
for property signatures anyway, associated functions are the established
idiom (`List::new`, `FlushPolicy::AtEnd`), and one import covers every ramp —
`Color::gray(50)`, `Color::blue(600)`, `Color::white()`, plus `Color::hex(..)`
as the typed escape. The completion flow is the point: `.background(` → the
parameter is `Color` → `Color::` lists the ramps. `space(n)` stays a bare
function — one function is not clutter, and `padding(space(4))` keeps the
familiar reading. **What `space(4)` computes to** (settled with the user): a
`Length` carrying the token's IDENTITY — rendered CSS `var(--space-4)`; the
theme's `:root` block supplies the magnitude, and const evaluation validates
the scale step. **Units namespace on `Length`** (the parameter type, the
Color rule again): `Length::px(37)`, `Length::rem(1.5)`, `Length::pct(50)`,
`Length::auto()`, and `Length::var("--w")` — the typed end of the dynamic
channel, pairing with `view.style_var("--w", signal)`. The representation
stays OPAQUE (constructors may render to CSS text immediately; structure for
`calc(..)`/unit arithmetic is deferred) — users never match on a `Length`,
so public variants buy nothing. An arbitrary value mints one atomic class
per distinct value, Tailwind-arbitrary style: the escape, not the norm,
bounded by dedup. Theme *values* are overridable day one (custom properties
are just CSS); theme *extension* (new ramps/namespaces) is deferred.

### 2.2 Conditions

Condition combinators wrap a `Style`, lowering each wrapped property to an
atomic rule with the condition baked in:

- **Pseudo**: `.hover(s)`, `.focus(s)`, `.active(s)`, `.disabled(s)`,
  `.first(s)`, `.last(s)` → `.hB7:hover { .. }`.
- **Breakpoints**: `.md(s)` → `@media (min-width: 768px) { .. }` (values
  from §2.1's structural tokens).
- **Dark mode**: `.dark(s)` → `:root[data-theme="dark"] .dC9 { .. }` —
  explicit, SSR-friendly control; an auto `prefers-color-scheme` mode is a
  recorded refinement.
- Condition methods take a `Style` built by its own chain
  (`.hover(style().background(..))`). **Stacking (as of 2026-08-04, §0bis.2):**
  media × dark × pseudo, nested outside-in in that order —
  `md(dark(hover(..)))`. Any other nesting order is refused with a message
  naming the fix; pseudo-over-pseudo stays unsupported.

### 2.3 The escape hatch

The typed property surface covers the core that styles 90% of real UI
(layout, spacing, color, typography, borders, radius, shadow, transition — 28
methods in v1, 46 after the 2026-08-04 demand-led property slice (§0bis.1),
60 after the value-type slice the same day (§0bis.3)). The tail
does not block: `raw("mask-image", "linear-gradient(..)")` lowers to an atomic rule
like any other, minus value validation. Plain string classes coexist
untouched (`view.class("leaflet-container")` — the method shipped as `class`,
not the `class_name` this draft assumed) for third-party CSS.

## 3. The construct-in-const rule (variant completeness)

The expression model's one hard problem: CSS for *every* variant must exist
at build time, but a runtime `match` never evaluates its unchosen arms — a
style constructed at runtime would have classes whose rules were never
emitted. The rule that keeps the system sound:

> **Styles construct inside `const` expressions; runtime code selects and
> merges.**

Mechanically free: property functions bottom out in `std::asset::emit`
(const-eval.md §3), which is **const-only** — so a runtime construction is a
static error at the construction site ("styles are compile-time values —
build them in a `const` expression"), enforced by call-graph reachability,
not convention. Selection (`match` over const styles) and merging (`+` as map
union over already-emitted rules) stay ordinary runtime code. This is the
constraint StyleX arrived at from the other direction, here falling out of
the capability model instead of a lint.

## 4. Compiler & std additions

Almost everything is `const-eval.md`'s: the evaluator (exists — the macro
interpreter), the `const` binding form, the const-only bit, the asset
channel with its dedup/ordering/emission (CSS ordering: base < pseudo <
media in ascending min-width order, then lexical — B35). On top, this
proposal adds only:

- **`std::ui::style`** — `Style` (the property map), `Add`, the property
  functions, condition combinators, token functions, `raw`. Pure std vilan.
- **`View.class(style: Style)`** — renders the joined class string (cached
  per map identity); reactive class switching composes with the existing
  turn/ownership machinery, staying a predictable map union under any
  interleaving. `view.style_var(name, signal)` writes custom properties for
  dynamic values, and `view.bind_styled(signal)` (2026-08-04) swaps a whole
  compiled style reactively. Plain-string `class` remains.
- **HTML hookup**: browser builds emit `<out>.css`; the html host links it
  (**both halves shipped** — emission 2026-07-10, the scaffolded `<link>` plus
  the fullstack `/client.css` route 2026-08-04); A7's server render later
  inlines critical CSS via the same channel.
- Server-layer code may hold `Style` values (plain data); platform rules are
  unaffected.

## 5. The Tailwind bridge (supported, sidecar, not the foundation)

Unchanged from the first draft: real Tailwind integrates today with near-zero
compiler work — its scanner regex-walks `**/*.vl` for class-shaped strings;
pass them through `view.class_name(..)`; run the CLI beside
`vilan build --watch` (the `[build] run` hooks item, A9, makes that
pleasant). Worth documenting as the familiarity option and escape hatch. Not
the foundation: its pains live in the string representation, and fixing merge
for real Tailwind means maintaining its per-version utility semantics inside
our compiler — the wrong home for someone else's database.

## 6. Implementation plan (slices)

1. **`const` core** (const-eval.md slices 1–4: grammar, analyzer, evaluator
   pass, serialization) — independently landable and useful.
2. **The asset channel** (const-eval.md slice 5) + the const-only capability
   bit and its call-graph check.
3. **`std::ui::style` core**: `Style` + `Add` + `class(..)`, the property
   functions and token scales, `:root` theme emission; the motivating corpus
   program (byte-stable CSS golden beside the JS golden).
4. **Conditions**: the pseudo set, breakpoints, `dark`; channel ordering
   rules pinned.
5. **`raw`, `style_var`, docs** + the Tailwind-bridge writeup.
6. (With A7, later) critical-CSS inlining; liveness-tied asset emission
   (dead-style elimination); theme extension; auto dark mode.

## 7. Open questions

- ~~The v1 property-function list~~ — settled by MEASUREMENT rather than by
  writing out a target number: v1 shipped 28, the 2026-08-04 demand sweep
  (§0bis.1) ranked the gap by real usage, and slice 1 took the head to 46. The
  remaining question is not "which properties" but §3b's value types.
- `Style` equality/hashing (memoized class strings suggest yes).
- ~~Whether method sugar ships in v1~~ — settled: the builder chain IS the
  surface; free property functions are not shipped.
- ~~Naming convention for style consts~~ — dissolved by the expression-form
  `const`: styles are ordinary `let` bindings, no special convention needed.

## 8. Alternatives rejected

- **The macro DSL** (this proposal's own first draft) — semantics identical,
  but every consumer pays the DSL toll: no hover/go-to-def/typed diagnostics
  inside the block, custom syntax highlighting, macro-grade error spans. The
  expression form gets the whole toolchain for free and composes with
  functions/impls/match natively. Kept in git history as the record.
- **Runtime CSS-in-JS** — per-render style work and SSR collection machinery;
  the industry is walking away from it for reasons vilan would inherit.
- **Compiler-maintained Tailwind semantics** (typed class strings +
  compile-time merge) — couples the compiler to an external project's
  per-version utility database. The sidecar (§5) covers familiarity.
