# std::style reference

Typed, compile-time atomic styles. Concepts and the emission model: the
[styling guide](../guide/styling.md).

```vilan,fragment
import std::style::{
	style, space, Style, Length, Color, Gradient,
	Display, Position, FlexDirection, AlignItems, JustifyContent,
	TextAlign, Cursor, Overflow, WhiteSpace, UserSelect, RadialExtent,
};
```

## Constructors and values

```vilan,fragment
fun style(): Style                 // empty style; chain from here (inside a const)
fun space(step: i32): Length       // spacing scale: space(1) = 0.25rem

impl Length {
	fun px(value: f64): Length
	fun rem(value: f64): Length
	fun em(value: f64): Length     // relative to the element's own font size
	fun pct(value: f64): Length
	fun vh(value: f64): Length     // viewport units
	fun vw(value: f64): Length
	fun auto(): Length
	fun zero(): Length             // bare `0`, not `0px`
	fun var(name: str): Length     // a CSS custom-property reference ("--w")
	fun calc(expression: str): Length  // "100% - 2rem" — no calc(..) wrapper
	fun css(expression: str): Length   // a COMPLETE value, verbatim: "clamp(..)"
}

impl Color {
	fun white(): Color
	fun black(): Color
	fun transparent(): Color
	fun hex(value: str): Color     // "#663399"
	fun var(name: str): Color      // a custom-property reference ("--accent"); the app declares it
	fun gray(step: i32): Color     // ramps: 50…900
	fun blue(step: i32): Color
	fun red(step: i32): Color
	fun green(step: i32): Color

	fun rgba(red: i32, green: i32, blue: i32, alpha: f64): Color  // a literal, 0-255 / 0.0-1.0
	fun oklch(lightness: f64, chroma: f64, hue: f64): Color  // perceptual: 0.0-1.0 / 0.0-0.5 / degrees
	fun alpha(self, value: f64): Color   // THIS colour at that alpha
}

impl Gradient {
	fun linear(degrees: f64): Gradient            // 0 up, 90 right, 180 down
	fun radial(extent: RadialExtent): Gradient    // extent keyword; centred
	fun stop(self, color: Color, percent: f64): Gradient
}
```

`alpha` renders the relative-colour form, `rgb(from <colour> r g b / a)`,
so a ramp step stays a `var(--gray-900)` and keeps re-theming — which an
8-digit hex could not do. Channels, alphas and the two-stop gradient
minimum are checked during const evaluation, so a bad value stops the
build naming itself.

`oklch` is the perceptual literal — hold a hue angle and step the
lightness, and the steps look even across hues, which is what deriving a
palette wants and what `rgba` cannot promise. Lightness takes the CSS
**number** form (0.0–1.0, not a percentage), chroma runs 0.0–0.5 (0 is
achromatic; sRGB tops out near 0.37), and the hue angle is degrees in its
canonical 0–360 turn — angles wrap in CSS, so one colour keeps one
spelling and one class. All three ranges are checked during const
evaluation, and `.alpha()` composes over the result like over any other
colour.

`Color::var` is `Length::var`'s counterpart — the typed end of the
dynamic-value channel. It renders `var(--name)` and **declares nothing**:
the app owns the custom property's declaration (its emitted theme block,
or `view.style_var` writing it at runtime). `.alpha()` composes over it
through the same relative-colour form, so a variable-backed colour
translucifies exactly like a ramp token.

`calc` wraps and `css` does not: `Length::calc(e)` is
`Length::css("calc(" + e + ")")`. Write `calc` for arithmetic, `css` for a
value that is already whole — `clamp()`, `min()`, `max()`, `env()`,
`fit-content()`, or one named expression reused across properties. Both
refuse an empty value at const time.

A `Gradient` is a **`background-image`** value, not a `Color`: it reaches
a style through `background_gradient`, which fills a different slot from
`background`. What a `Gradient` cannot hold — positioned gradients
(`at 20% 40%`), multi-layer lists, `repeating-*`, and data-URI images —
goes to `background_image(str)`, which writes the **same slot**, so the two
override each other instead of racing in the cascade.

Keyword enums: `Display` (Flex, Block, …), `Position`, `FlexDirection`,
`AlignItems`, `JustifyContent`, `TextAlign`, `Cursor`, `Overflow`,
`WhiteSpace` (Normal, Nowrap, Pre, PreWrap, PreLine), `UserSelect` (Auto,
Text, All, **Off** — `none`, named to stay clear of `Option::None`, like
`Display::Hidden`).

All eleven are **backed enums**: each variant carries the CSS keyword it
stands for, so the enum *is* that keyword at runtime and `.value()` hands it
back. That is why the names need not match the keywords — `AlignItems::Start`
is `"flex-start"`, `Display::Hidden` is `"none"` — and why passing one costs
nothing over passing the string.

```vilan,fragment
Display::Hidden.value()          // "none"
AlignItems::Start.value()        // "flex-start"
Display::parse("inline-block")   // Some(Display::InlineBlock)
Display::parse("nope")           // None
```

## Style methods

Every method returns a new `Style` with one more property slot; each slot is
one atomic rule, deduplicated build-wide.

Layout:

| Method | Value |
|---|---|
| `display` | `Display` (Flex, Grid, Block, Inline, InlineBlock, InlineFlex, InlineGrid, Hidden) |
| `position` | `Position` |
| `flex_direction` | `FlexDirection` |
| `align_items` | `AlignItems` |
| `justify_content` | `JustifyContent` |
| `flex` | `str` — the shorthand, `"1 1 auto"` |
| `flex_shrink` | `f64` |
| `grid_template_columns` | `str` — `"repeat(3, 1fr)"` |
| `gap`, `padding`, `padding_x`, `padding_y`, `margin`, `margin_x`, `margin_y` | `Length` |
| `padding_top`, `padding_right`, `padding_bottom`, `padding_left` | `Length` — one edge |
| `margin_top`, `margin_right`, `margin_bottom`, `margin_left` | `Length` — one edge |
| `width`, `height`, `min_width`, `max_width`, `min_height`, `max_height` | `Length` |
| `size` | `Length` — width *and* height, the square case (`size(Length::rem(1.0))` for an icon box); writes the same two slots, so mixing with `width`/`height` is last-wins |
| `top`, `right`, `bottom`, `left`, `inset` | `Length` |
| `overflow` | `Overflow` |

Appearance:

| Method | Value |
|---|---|
| `radius` | `Length` |
| `border` | `(width: Length, color: Color)` — always `solid` |
| `border_top`, `border_right`, `border_bottom`, `border_left` | `(width: Length, color: Color)` |
| `border_none` | — fills the `border` slot, so it *removes* a border set earlier |
| `border_color` | `Color` — its own slot, so a `hover` can recolour without restating the width |
| `box_shadow` | `str` |
| `background`, `color` | `Color` |
| `background_gradient` | `Gradient` — the `background-image` slot |
| `background_image` | `str` — the same slot, for what a `Gradient` can't hold |
| `background_size` | `str` — up to two components, so not a `Length` |
| `font_family` | `str` |
| `font_size` | `Length` |
| `font_weight` | `i32` |
| `line_height` | `f64` — unitless, and the one to prefer (inherits as a ratio) |
| `line_height_length` | `Length` — the same slot, when the leading is absolute |
| `letter_spacing` | `Length` — usually `Length::em(..)` |
| `text_align` | `TextAlign` |
| `text_decoration` | `str` |
| `white_space` | `WhiteSpace` |
| `user_select` | `UserSelect` |
| `cursor` | `Cursor` |
| `opacity` | `f64` |
| `transition` | `str` |
| `transform` | `str` |

A `str`-valued method is not a weaker `raw`: it keeps the property name
checked and completable while the *value* stays a CSS expression the compiler
has nothing to validate (a font stack, a transform list, a shadow layer).

**The name carries the arity** — whole box (`padding`), axis
(`padding_x`), edge (`padding_top`) — and there is no multi-value
shorthand method, because `padding_y(v).padding_x(h)` already computes
`padding: v h`.

**Arities mix, and resolve in authoring order.** A property that covers
others forms a family — `padding`, `margin`, `inset` (over `top`,
`right`, `bottom`, `left`), `border` (over its parts and edges),
`background`, `flex` — and last-wins applies to the whole family: a
longhand written after the shorthand narrows it, a shorthand written
after a longhand replaces the family outright. Per condition, so a
`hover` or `dark` variant never disturbs the base, and through `raw`
too, since the family is a fact about the CSS property.

Escape hatches:

```vilan,fragment
fun raw(self, property: str, value: str): Style
fun with_length(self, property: str, value: Length): Style
fun with_color(self, property: str, value: Color): Style
```

## Conditions

Each takes an inner `Style` and conditions all of its slots:

```vilan,fragment
fun hover(self, inner: Style): Style
fun focus(self, inner: Style): Style
fun active(self, inner: Style): Style
fun disabled(self, inner: Style): Style
fun first(self, inner: Style): Style      // :first-child
fun last(self, inner: Style): Style       // :last-child
fun dark(self, inner: Style): Style       // :root[data-theme="dark"] ancestor
fun attribute(self, name: str, value: str, inner: Style): Style  // .sX[name="value"] — the element itself
fun pseudo(self, name: str, inner: Style): Style

fun sm(self, inner: Style): Style          // breakpoints (min-width):
fun md(self, inner: Style): Style          // 640px, 768px, 1024px, 1280px
fun lg(self, inner: Style): Style
fun xl(self, inner: Style): Style
fun media(self, min_width: str, inner: Style): Style
```

### Stacking

The four condition axes nest **outside-in, in the order the selector
nests them** — media, then dark, then the attribute, then the
pseudo-class:

```vilan,fragment
style().md(style().dark(style().hover(style().opacity(0.8))))
// @media (min-width: 768px){:root[data-theme="dark"] .sX:hover{opacity:0.8}}

style().md(style().dark(style().attribute("data-open", "true", style().hover(style().opacity(0.8)))))
// @media (min-width: 768px){:root[data-theme="dark"] .sX[data-open="true"]:hover{opacity:0.8}}
```

Every other order is a compile-time-evaluation panic naming the fix
(`hover(dark(..))` says to write `dark(hover(..))`), and no axis can wrap
itself — one media, one dark, one attribute, one pseudo-class per slot.
Media rules emit in ascending min-width order, so a chain like
`.sm(x).lg(y)` is mobile-first: the widest matching breakpoint wins.

`attribute` conditions on the element **itself** — `.sX[data-open="true"]`
— where `dark` is the ancestor form. It is the general spelling of state
carried in markup: `data-state`, `data-open`, `aria-expanded` — any
attribute rides, `aria-*` included, and the value matches exactly. The
app owns *setting* the attribute on the element; the style only selects
on it. Name and value refuse quotes, spaces and `:` at const time (they
delimit the machinery underneath), and a styling hook is a single token
in practice.

`dark` is an ancestor selector, so a composed `dark(hover(..))` rule is
more specific than either `dark(..)` or `hover(..)` alone and wins
against both — and the same holds along the attribute axis:
`attribute(.., hover(..))` outranks both of its parts. Between an
*un*composed `dark(x)` and `hover(y)` on the same property the two are
equally specific and dark wins, so use `dark(hover(..))` when a dark
theme needs its own hover.

## Declaration blocks

`Style` dresses an *element*. A **declaration block** puts a set of
declarations under a selector **you** choose — a theme's custom properties
under `[data-theme="…"]`, a `:root` token table, a reset's `box-sizing` —
minting no class, producing no `Style` and touching no slot key.

```vilan,fragment
fun declarations(): Declarations                 // opens a declaration chain
fun declare(selector: str, body: Declarations)   // puts the block in the stylesheet

impl Declarations {
    fun raw(self, property: str, value: str): Declarations
    fun color(self, property: str, value: Color): Declarations     // carries the token's :root line
    fun length(self, property: str, value: Length): Declarations   // likewise
}
```

Like `style()`, this is compile-time-only — `declare` reaches
`std::asset::emit`, so it belongs inside a `const` expression:

```vilan
import std::style::{ Color, declare, declarations, space };

fun theme(id: str) {
    declare(
        i"[data-theme=\"{id}\"]",
        declarations()
            .color("--color-ink", Color::hex("#fafafa"))
            .color("--color-ground", Color::hex("#161616"))
            .length("--gap", space(4)),
    );
}

let _iron = const theme("iron-dark");

fun main() {}
```

That emits one line into the build's stylesheet:

```text
@layer vilan{[data-theme="iron-dark"]{--color-ink:#fafafa;--color-ground:#161616;--gap:var(--space-4)}}
```

A `Color` or `Length` spent in a block carries its own `:root` token line
onto the sheet exactly as a `Style` property does, so a ramp or spacing
token used here is never a dangling `var()`.

### Ordering

Every block emits inside one cascade layer, `@layer vilan`, and that is the
whole ordering rule. Unlayered styles beat layered ones whatever their
specificity, so **a `Style` always wins against a declaration block**: a
block cannot reach in and out-specify a view's own rules, however specific
the selector it names, and where its line lands in the stylesheet's sort
decides nothing. Among blocks, ordinary CSS applies — specificity first,
then the sheet's own deterministic line order.

The other face of the same rule: a block cannot override an **unlayered**
declaration either, which includes std's own token lines (`--space-4`,
`--gray-50`) and any hand-written CSS the page loads. Declare your own
custom properties and read them back with `Color::var` / `Length::var` —
that composes exactly, and it is what a theme wants anyway.

### Refusals

Checked at const time, each naming its fix:

- a selector that is blank, carries a **newline** — the asset channel is
  line-granular, so a newline does not indent the rule, it splits it into
  two independently deduplicated and sorted lines — or carries a **brace**,
  which is `declare`'s to write;
- a selector that is an **at-rule**: `declare` puts declarations under a
  selector, and a group at-rule (`@media`, `@supports`) holds rules, not
  declarations;
- a block with **no declarations**;
- a property carrying `:` or `;`, the two separators it owns, or a blank
  value. A `;` inside a *value* stays legal, so a data URI rides
  (`url("data:image/svg+xml;base64,…")`).

`vilan fmt` never reorders a `declarations()` chain. Its links are cascade
text joined in authoring order, where a `style()` chain's links each own a
slot and may be sorted freely.

## Runtime-legal operations

Construction emits rules and therefore lives in `const`; these do not emit
and work anywhere:

```vilan,fragment
style_a + style_b          // merge: per-property, right side wins (impl Add)
style.class_list(): str    // the space-joined class attribute (what `styled` uses)
```
