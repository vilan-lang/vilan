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
| same-family rule ordering (A22) | **RECORDED OPEN → SHIPPED 2026-08-04** | The hazard §0bis.3 filed, fixed per §0bis.4: a family table, a shorthand dropping what it covers under the same condition, and a `*` marker putting shorthand rules ahead of their family's longhands in the existing lexical sort. **Every class name is unchanged** — the marker is in `render_rule`, not in the hash input — so the re-mint permission the slice was granted went unspent. The measurement is why: a scan of all 275 style chains, extensions and `+` merges across the website, examples, docs and tests found the hazard **live in two production sites** (`status_line`'s `margin` + `raw("margin-left")`, `df_node_lit`'s `border` + `raw("border-color")` across a `+`), and one of them is on the runtime-legal `+` path, which cannot emit — which is what ruled out resolving the conflict inside the `Style` by splitting the shorthand. The sweep also found the family inventory was four rows short: `inset` over the placement methods, `background` over the typed colour and gradient slots, and `flex` over `flex-shrink`. |
| critical CSS | **OPEN, out of this arc's scope** | A7-entangled; still §6 slice 6, proposal-only. Left filed. |
| liveness-tied dead-style elimination | **OPEN, out of this arc's scope** | Rides G2's liveness-tied emission. Left filed. |

Two verification by-products, filed rather than fixed here:

- **`View.style_var` leaks its subscription** (browser twin). It is the only
  reactive `View` method that calls `source.sub(..)` and parks the handle in a
  `let _sub` instead of going through `source.effect(..)`, so the subscription
  is never handed to the ambient owner and outlives its boundary's disposal.
  Every sibling binder (`bind_text`, `bind_class`, `bind_attr`, `show`) uses
  `effect`. Not touched here — it is a reactive-ownership bug, not a styling
  one, and it wants its own pin. **FIXED 2026-08-04 (A21)** — one line, the
  `effect` every sibling uses; the SSR twin reads once and was already right.
  Pinned in `tests/router.rs`, which already drove `swap` + disposal for
  `bind_text`: the `style_var` page's signal is written from a button OUTSIDE
  the swapped subtree, so the write lands well after the unmounting turn, and
  the stub's `style.setProperty` now RECORDS instead of no-opping (it had
  nothing to observe before). Red against the parked `let _sub`, and the only
  assertion of the fourteen that moved.

  Two corrections fell out. **`ssr.md`'s residue — "`style_var` sits outside
  the SSR differential because the DOM stub no-ops `style.setProperty`" — was
  never true**: that stub's `_upsertStyle` has folded the property into the
  `style` attribute since the first SSR commit (`309e2bb`), exactly the way the
  process twin folds it. `style_var` is in the shared component now, compared
  byte-for-byte across both twins, non-vacuity planted. And the browser twin's
  doc comment says what the convention is, so the next binder written by
  copy-paste copies the right one.
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
typed method on one slot for no demand. **Reversed 2026-08-04 by §0bis.5
(A23)** — the count was taken over the image slot alone; `background-size` (2
sites, inert without an image, always written beside one) belongs to the same
unit, which puts the pair at five. The "two surfaces on one slot" objection is
answered by `border`/`border_none`, which is the same shape and the reason the
slot is the right one: two methods on one slot override, two on two slots
race.

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
unitless is the correct default besides. **Reversed 2026-08-04 by §0bis.5
(A23)**, on the hole argument rather than the count: `line-height` is the one
length-valued property whose typed method cannot hold a unit. `line_height`
itself is untouched — the addition is the sibling `line_height_length(Length)`,
and unitless remains the documented default.

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
that is allowed to re-mint names. **Fixed 2026-08-04 by §0bis.4 (A22)** — and
the re-mint permission turned out not to be needed.

### 0bis.4 Design — same-family override order (A22)

The hazard §0bis.3 recorded, fixed. The charter for this slice lifted the
class-name-stability constraint; the design that won does not spend it.

**The measurement first, because it changed the design.** A family conflict is
two slots on one style, under one condition, whose CSS properties overlap —
one a shorthand, one something it covers. Scanning every `style()` chain, every
named-style extension and every `+` merge across the website (the one real
consumer), the examples, the docs and the tests — 275 chains — found **two
live instances**, both of which today resolve by a coin flip of the class hash:

- `vilan-website/src/playground_page.vl`, `status_line`:
  `.margin(space(0)) … .raw("margin-left", "auto")` — a chain.
- `vilan-website/src/art.vl`, `df_node_lit`:
  `df_node + style().raw("border-color", …)` — a **`+` merge**, where
  `df_node` carries `border` from `art_card`.

Two facts follow, and between them they pick the design. First, the hazard is
live, not theoretical — it is in production styling now. Second, **half of it
is on the `+` path**, and `+` is runtime-legal (`view.styled(column + nav_row)`
is an ordinary call in a view function). Anything `+` must do to resolve a
conflict has to be doable **without emitting a rule**, because reaching `emit`
from a runtime call is a compile error by construction, and making `add`
const-only would break every call site that composes styles at render time.

The sweep also corrected the family inventory. The record named two families;
there are **six**, because `raw` writes CSS properties too and the family
relation is a fact about the properties, not about which method wrote them:
`padding`, `margin`, `inset` (over `top`/`right`/`bottom`/`left` — the
placement methods are that shorthand's longhands and nobody had noticed),
`border`, `background` (36 `raw("background", …)` sites over the typed
`background`→`background-color` and `background_gradient`→`background-image`),
and `flex` (8 `raw("flex", …)` sites over 4 `raw("flex-shrink", …)`).

#### The three candidates

**(a) Emission-order tier — within a family, longhand rules sort after
shorthand rules.** The mechanism B35 already established: `assemble_assets`
has no CSS comparator, it sorts lines lexically, and the cascade bands fall out
of ASCII (`.` < `:` < `@`). Its recorded weakness was that the guarantee is
stylesheet-global while the conflict is per-element — **and that weakness does
not apply here.** `view.styled(style)` and `bind_styled(signal)` *set* the
class attribute from exactly one `Style`; `class(name)` sets it from a string;
there is no API that unions two styles' class lists onto one element. Two
styles reach one element only through `+`, which merges them into one `Style`
first. One element carries one style's slots, so a stylesheet-global order is
a per-element order.

Its real weakness is a different one: a fixed tier says *longhand beats
shorthand*, which is Tailwind's rule, not this system's. §0 is explicit that
merges resolving by stylesheet order instead of authoring order is the chronic
pain a compiler that owns the pipeline exists to remove, and
`padding_top(0).padding(4)` must resolve to `1rem` on all four edges.

**(b) Specificity doubling — longhands emit `.sX.sX{..}`.** Rejected on three
counts. It has (a)'s semantic defect *and* pays for it: the tier at least
costs nothing, while doubling rewrites the text of every longhand rule in
every build. It does not compose with the condition axis, which is also
specificity: a doubled base longhand (0,2,0) ties a plain `hover` shorthand
(0,2,0), so fixing the family axis breaks the condition axis — the two
orderings are orthogonal and there is one specificity ladder. And it cannot
order a pair like `border-top` against `border-color`, which are both
longhands.

**(c) Resolve at build — last-set-wins per (property, condition) over the
family expansion.** The chain is ordered and fully known at const time, so a
`Style` can hold the resolved result and the stylesheet never contains an
intra-family conflict for one class. Setting `padding` after `padding_top`
drops the edge; setting `padding_top` after `padding` **splits** the shorthand
into the edges it still owns. Correct for every pair, including the
non-subsuming ones (`border_top` against `border_color`), and it needs no
ordering guarantee at all.

It fails on the fact the measurement turned up. **The split has to emit** —
`padding-right:1rem` is a rule that did not exist before — and the `+` path
cannot emit. So (c) resolves the `status_line` chain and leaves `df_node_lit`
exactly as broken as it is today, or refuses it and breaks the website build.
Its second cost is that the split materializes rules that were never asked
for: `border` decomposes into twelve longhands, since `border-color` and
`border-top` cover different slices of the same twelve and nothing coarser
separates them.

#### The recommendation: (a), with the shorthand's slot dropped

Take the tier, and remove its semantic defect at the object level rather than
in the cascade. Two rules, and the interesting part is that they meet exactly:

1. **A shorthand set later drops what it covers.** Inserting a slot whose
   property covers other properties removes every slot for a covered property
   *under the same media and condition*. This is the ordinary last-wins rule
   widened from one property to a family — a map removal, no emission, so it
   holds on the `+` path too.
2. **A shorthand's rule sorts before its family's longhands.** A rule whose
   property is a family shorthand renders `*.sX{..}` rather than `.sX{..}`.
   `*` is 0x2A and `.` is 0x2E, so the existing lexical sort puts every
   shorthand rule ahead of every longhand rule inside its own cascade band,
   and `*.sX` is the same compound selector as `.sX` with the same
   specificity — the universal selector contributes nothing. B35's ordering is
   untouched: the numeric `@media` override reads a prefix the marker never
   appears in, and the band order becomes `*` < `.` < `:` < `@`.

**The tier and authoring order coincide, exactly.** Rule 1 means two slots of
one family survive together only when the longhand was set *last* — a
later shorthand would have dropped the longhand. So the one case the tier
decides is the one where the longhand should win anyway, and the fixed tier
never has to answer the question it would answer wrongly:

| chain | slots after resolution | winner | by |
|---|---|---|---|
| `padding(4).padding_top(0)` | `padding`, `padding-top` | top `0` | the tier |
| `padding_top(0).padding(4)` | `padding` | all `1rem` | the drop |
| `padding(4).padding_x(6)` | `padding`, `padding-left`, `padding-right` | `1rem 1.5rem` | the tier |
| `a{padding} + b{padding-top}` | both | b's top | the tier |
| `a{padding-top} + b{padding}` | `padding` | b's box | the drop |

The last two are why this design and not (c): `+` keeps "the right side wins"
in both directions with no emission.

**Specificity is untouched, so the condition axis is untouched.** `*.sX` is
(0,1,0) exactly as `.sX` was, `*.sX:hover` is (0,2,0), and
`:root[data-theme="dark"] *.sX` is (0,2,0) — every cross-condition pair
resolves the way it did before this slice: a `dark` or `hover` shorthand still
beats a base longhand on specificity, a `dark` longhand still beats a base
shorthand, and a media block still wins its equal-specificity tie by the `@`
band. This is the property (b) cannot have.

#### The family table, and `raw`

One level, one whole-box shorthand per family, and its longhands:

| shorthand | covers |
|---|---|
| `padding` | `padding-top/right/bottom/left` |
| `margin` | `margin-top/right/bottom/left` |
| `inset` | `top`, `right`, `bottom`, `left` |
| `border` | `border-width/style/color`, `border-top/right/bottom/left`, and the twelve `border-<edge>-<part>` |
| `background` | `background-color/image/position/size/repeat/attachment/origin/clip` |
| `flex` | `flex-grow`, `flex-shrink`, `flex-basis` |

Every row is earned by a real site (above); the rule for adding one is that a
new property method which is a CSS shorthand over another writable property
adds its row in the same commit. The table is deliberately *not* prefix-based:
`border-radius`, `border-collapse` and `flex-direction` are not covered by
`border` and `flex`, and a prefix rule would silently swallow them.

**`raw` participates, by property name — and this is a correction to the
charter's suggested "raw stays opaque".** It has to: `border_none()` *is*
`raw("border", "none")`, and both live instances of the hazard have `raw` on
one side. Opacity would leave them exactly as arbitrary as they are today. The
honest statement is that `raw` gains no mechanism of its own — it writes a slot
like every other method, and the slot's property decides its family, because
the family relation is a fact about CSS. What `raw` genuinely cannot do is
supply a decomposition (`raw("padding", "1rem 2rem")` cannot be split into
edges without parsing CSS the language deliberately does not model), and this
design never needs one. That is a further argument for (a) over (c): (c) is
the design that would have had to refuse those sites.

#### Consequences, worked through

- **Class names: none change.** The name is `class_hash(key + "|" +
  declaration)`; the marker lives in `render_rule`, which is the emitter, not
  the hash input. Every already-minted class in every program keeps its name,
  including the shorthand rules'. The re-mint permission is not spent.
- **Rule text: the shorthand rules only.** `.s1ufvr2{padding:var(--space-4)}`
  becomes `*.s1ufvr2{padding:var(--space-4)}` — same class, same declaration,
  same specificity, same matching, new sort position. Goldens change on those
  lines and on the ordering; the verification is per class, that the
  declaration is byte-identical and the selector's only delta is the marker.
- **Class lists change only where a conflict existed** — at a drop, which is
  the fix. A style with no intra-family conflict is byte-identical end to end.
- **Dead rules.** A dropped slot's rule was already emitted and stays in the
  stylesheet, unused. That is the existing over-approximation (a chain's
  overridden `padding(4).padding(6)` has always emitted both, as do a
  condition's inner base rules), and it is A8's remaining
  liveness-tied-elimination item, not a new leak.
- **The hole, stated.** Two longhands of one family that cover *different*
  parts of it — `border_top` against `border_color`, or `raw`'s
  `border-top-color` against either — are both rank 1, so they tie and the
  hash decides. No static tier can order them, because the answer is authoring
  order and the stylesheet has no record of it; only (c)'s split could, at the
  cost above. Zero instances across the corpus, the authoring rule in the docs
  covers it, and the fix is to write the edge with its colour
  (`border_top(width, colour)`). Recorded rather than papered over.

### 0bis.5 Design — the website's measured remainder (A23)

The third value-type slice, and the one whose charter the measurement
contradicted hardest. A23 was filed off the raw-call **count** — 107 of the
website's 341 `raw` sites survived the §0bis.3 conversion — on the same day
§0bis.3's value types shipped, and the row was never checked against the
supply that had landed hours earlier. Reading the 107 sites reverses the
headline outright. Counts below are that reading, file and line, over
`vilan-website/src/*.vl`.

**1. `background` (36 sites, the headline) — nothing is missing. The value
types §0bis.3 shipped already hold 33 of the 36, and the other three are
§0bis.3's own recorded cuts.** The inventory, which is the whole argument:

| what the 36 sites write | count | already expressible as |
|---|---|---|
| a hex or `rgba()` literal | 20 | `background(Color::hex(..))` / `background(Color::rgba(..))` |
| `radial-gradient(closest-side, <colour>, transparent)` | 11 | `background_gradient(Gradient::radial(RadialExtent::ClosestSide).stop(..).stop(..))` |
| `linear-gradient(to left\|to right, a, b)` | 2 | `Gradient::linear(270.0)` / `Gradient::linear(90.0)` — the side keywords ARE those angles |
| `repeating-linear-gradient(..)` | 1 | — §0bis.3's recorded cut |
| `rgba(18, 0, 4, calc(var(--nav-fade, 0) * 0.86))` | 1 | — §0bis.3's recorded cut (a live custom property in the alpha channel) |
| `radial-gradient(340px circle at var(--glow-x..) .., ..)` | 1 | — §0bis.3's recorded cut (`at <position>`, and it wants live variables) |

So the 36 sites are a **conversion** backlog, not a supply hole, and both
candidates the charter offered fall to the inventory rather than to argument:

- **A composite `Background` value type is not minted.** Every one of the 36
  sites writes exactly ONE value into the shorthand. There is no composite in
  the demand to model, and a value type for arities nobody writes is the
  `Sides` rejection again.
- **`background_position`, `background_repeat`, `background_attachment`,
  `background_origin` and `background_clip` are not minted. Zero sites each**,
  across 2926 lines of the one real consumer. Minting a slot with no site is
  exactly the speculation §0bis.3 refused for `BorderStyle`'s other variants.

What the sweep *did* find sits in the same family and outside the 36:
**`background-image` (3 sites) and `background-size` (2), always written
together** — the masthead's bloom (an eight-layer positioned gradient list,
sized in `calc()` of the hero scale), its duo tile, and the theme's grain tile
(both ~2 KB data URIs). Five sites, one unit: a background image you can set
and cannot size is a half-surface, which is the `min_width`/`max_height`
argument. So **`background_image(str)` and `background_size(str)` ship, and
nothing else in the family does.** This reverses §0bis.3's cut of
`background_image` at "3 sites, below the line", and names the error: the
count was taken over the image slot alone, and `background-size` — which is
inert without an image — was never counted with it.

`str` on both, not `Gradient` and not `Length`. What defeats the value types
at all five sites is precisely what §0bis.3 recorded as cut: a data URI and a
multi-layer positioned list on the image slot, and a **two-value**
`calc(..) calc(..)` on the size slot, which one `Length` cannot hold. §0bis.1's
bargain applies unchanged — the property name stays checked and completable
and only the value is CSS text nothing could validate.

`background_image` and `background_gradient` write the **same slot**, and that
is the point rather than a defect: it is the `border`/`border_none` shape. Two
methods on one slot override each other under the ordinary last-wins rule; two
methods on two slots would race in the cascade, which is the defect §0bis.3
rejected `BorderStyle` for. Reach for `background_gradient` when a `Gradient`
holds the value and `background_image` when it does not.

**The A22 interaction, and the one hazard the conversion lane must carry.** No
family-table row changes — A22 already lists all eight `background-*`
longhands under `background`, so the two new methods join a row that was
written for them. But converting `raw("background", v)` to a typed method
moves a slot from the family **shorthand** to a **longhand**, so the
shorthand's reset of the rest of the family stops happening. That is
observable exactly when a background colour and a background image meet on one
element. Checked at all 36 sites: `art_blob` — the base every one of the 11
glow sites extends — carries no background at all (position, radius, `filter`,
`pointer-events`), and no style anywhere in the website pairs a background
colour with a gradient. So the conversion is safe **site by site and in any
order**: a converted longhand and an unconverted `raw` shorthand still resolve
by authoring order, through A22's `*` marker.

**2. Two-value `padding` — `padding_xy` is NOT minted. §0bis.3 stands, and
working A22 through is what confirms it.** All four sites are the `y x` form
in px (`1px 6px`, `8px 20px`, `7px 14px`, `6px 10px`); still zero 3-value,
zero 4-value, and `margin` is still never given a shorthand at all. The
byte-diff is declaration SHAPE only, and the reason it is *only* that is
structural: `padding_y(v).padding_x(h)` writes all four `padding-*` longhands,
which is exactly the shorthand's coverage, so every direction agrees.

| the site's neighbourhood | `raw("padding", "y x")` | `padding_y(y).padding_x(x)` |
|---|---|---|
| after a `padding(..)` | shorthand slot, last wins | four longhands; the `*` marker sorts the earlier shorthand first, so the longhands win |
| before a `padding(..)` | shorthand slot, last wins | rule 1 drops all four — the box wins |
| after a `padding_top(..)` | drops the edge | overwrites the same `padding-top` slot |
| either side of a `+` | as above | as above, and still no emission |

Identical computed result in each row. What the composition costs is four
atomic declarations where the site wrote one — and the channel dedups
build-wide, so `padding-top:8px` is very likely a rule the stylesheet already
carries. By this section's own rule a method that buys one rule and no
expressiveness is surface, so the four sites are recorded as byte-diff
conversions the next cycle accepts, not as a missing method.

**3. `line_height_length(Length)` — a sibling method, not a value type.** All
four sites are px (`18px`, `24px`, `28px`, `48px`), which is under the ≥5 line
§0bis.3 cut this at. It is admitted anyway on the **hole** argument rather than
the tail argument: `line-height` is the one length-valued property in the
surface whose typed method cannot hold a unit, while every other length-valued
property takes `Length`. A `LineHeight` value type (unitless-or-`Length`) is
rejected for §0bis.3's `Sides` reason — with no overloading it can only reach
the property through a second method anyway, since `line_height(f64)` keeps its
name and its callers, so the value type buys a type and no reach. The sibling
takes `Length` rather than an `f64` of px because the hole is "cannot hold a
unit", not "cannot hold px". Same slot as `line_height`, so mixing the two is
an ordinary last-wins override, and `line-height` has no family. Additions
only: `line_height` is untouched and every already-minted class keeps its name.
The docs keep saying unitless is the right default, and now say why — a
unitless value inherits as a ratio and re-computes per element, a length
inherits as a computed length.

**4. `Length::zero()` renders bare `0` — 11 sites, not the row's 7.** The
recount: `inset:0` ×3, `min-width:0` ×3, `min-height:0` ×2 (the row missed
these two), `top:0` ×2, `left:0` ×1. `0` is unit-legal for a length and is what
the flex `min-width:0` idiom and the `inset:0` fill-the-parent idiom are
written as everywhere; `space(0)` renders `var(--space-0)` and
`Length::px(0.0)` renders `0px` — both computed-identical, neither
byte-identical. A constructor, and no existing rule changes.

**5. `Length::css(expression)` — the verbatim functional-value escape.
`calc` is kept, unbroken, and is now documented as the sugar it is.** The probe
first, because it decides the framing rather than the design: `clamp()`,
`min()` and `max()` are math functions and **nest inside `calc()`** (CSS Values
4), so `Length::calc("clamp(120px, 30%, 185px)")` is valid CSS today and
already covers all three sites computed-identically. This is therefore not a
capability gap, and saying otherwise would have been the easy wrong answer.
What the sites show is a *shape*: the masthead names its whole scale
(`let hero_scale = "clamp(1100px, 100vw, 1920px)"`), interpolates it into eight
`Length::calc(i"..{hero_scale}..")` arithmetic expressions, and then hands it
to `raw("width", hero_scale)` **whole** — a complete value, not an arithmetic
fragment, which `calc` cannot say without adding a wrapper the author did not
write. `Length::css` is `Color::hex`'s twin: `Color` has had an unvalidated
verbatim escape since v1 and `Length` never did, the same symmetry hole
`min_width`/`max_height` were admitted for. `calc` stays the ergonomic wrapper
for the arithmetic case it was minted for (eight live sites and the docs'
example); the relationship is stated rather than left implicit —
`Length::calc(e)` is `Length::css(i"calc({e})")`. Not renamed, not deprecated,
byte-identical output.

**Const-validation, per §0bis.3's precedent.** The three new `str`-taking
surfaces (`Length::css`, `background_image`, `background_size`) share the one
malformation a CSS-text escape can actually detect: nothing. An empty or
all-whitespace value renders `property:`, a declaration the browser drops in
silence, and the realistic way to produce one is an interpolation whose
variable was never set — which is exactly how the masthead writes its
background sizes and how it and the theme write their mask and tile URIs.
`Length::calc` gains the same guard: `calc()` is invalid CSS in every context,
so the check can only turn a silent malformation into a build error naming the
value, and no working program can reach it.

**Cut from this slice, with the trigger that would reopen each.**
`Gradient::stop`'s explicit percent stays required — 11 of the 13 convertible
gradient sites write no positions, and `A 0%, B 100%` is computed-identical to
the defaulted pair, so the cost is bytes and not behaviour. Reopen it if a
THREE-stop gradient wants defaults, where the defaulted positions stop being
trivially 0 and 100. The five unwritten `background-*` slots reopen at one real
site each. `padding_xy` reopens if a 3- or 4-value shorthand ever appears,
which no composition of the axis methods can express.

#### What the next cycle's website conversion should do

The conversion rides the release after this one (the site builds on the latest
published toolchain). Per row: the sites it unblocks, what to write, and
whether the emitted stylesheet comes out byte-identical or merely
computed-identical — the distinction the last conversion stopped at, and the
reason these sites were left behind.

| the `raw` sites | n | unblocked by | what to write | bytes |
|---|---|---|---|---|
| `background: <hex\|rgba>` | 20 | §0bis.3 (shipped) | `.background(Color::hex(..))` / `.background(Color::rgba(..))` | **differs** — the slot moves to `background-color` |
| `background: radial-gradient(closest-side, ..)` | 11 | §0bis.3 (shipped) | `.background_gradient(Gradient::radial(RadialExtent::ClosestSide).stop(c, 0.0).stop(Color::transparent(), 100.0))` | **differs** — the slot moves to `background-image`, and the stops gain explicit `0%`/`100%` |
| `background: linear-gradient(to left\|right, ..)` | 2 | §0bis.3 (shipped) | `Gradient::linear(270.0)` / `Gradient::linear(90.0)` | **differs** — the side keyword becomes its angle |
| `background:` the three §0bis.3 cuts | 3 | — | stays `raw`, deliberately | — |
| `background-image: <data URI\|layer list>` | 3 | **`background_image(str)`** | `.background_image(value)` | identical |
| `background-size: <two values>` | 2 | **`background_size(str)`** | `.background_size(value)` | identical |
| `padding: "<y> <x>"` | 4 | §0bis.3 (shipped) | `.padding_y(Length::px(y)).padding_x(Length::px(x))` | **differs** — four declarations for one |
| `line-height: <px>` | 4 | **`line_height_length`** | `.line_height_length(Length::px(v))` | identical |
| bare `"0"` on `inset`/`min-width`/`min-height`/`top`/`left` | 11 | **`Length::zero()`** | `.inset(Length::zero())`, `.min_width(Length::zero())`, … | identical |
| a whole `clamp()`/`min()` value | 3 | **`Length::css(str)`** | `.left(Length::css("clamp(120px, 30%, 185px)"))`, `.width(Length::css(hero_scale))` | identical |

**60 of the 107 surviving `raw` calls**, of which 23 convert byte-identically
and 37 computed-identically. Three sites stay `raw` by design.

The row this slice was chartered from speaks of "the website's 26 byte-diff
conversions", and **that number could not be reconciled**: no list of the 26
is recorded in the backlog, in this file, or in any commit message, so there is
nothing to check it against. The table above is a fresh measurement of the 107
sites that are actually in the tree, taken site by site, and supersedes the
figure rather than reproducing it. If the 26 was a count of sites the previous
conversion attempted and reverted, it is a subset of the 37 here. The remaining 44
are the properties still below the line — `clip-path` (4), `text-transform`
(4), `pointer-events` (3), `animation` (3), `align-self` (3), the mask
family (5), and a scatter of ones and twos — plus `Color::hex` being used to
smuggle an `rgba(..)` string at four sites, which is a conversion fix
(`Color::rgba`) rather than a missing method.

Two orderings the lane should keep. The background conversion is safe in any
order per the hazard worked through in decision 1, but it should be done
**whole-file at a time** so a reviewer reads one slot convention per file. And
the four `padding` sites and the eleven zero sites want to land in the same
commit as their `.css` golden, since both change the stylesheet's bytes without
changing a computed value — the shape of diff that is only reviewable if
nothing else moves with it.

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
