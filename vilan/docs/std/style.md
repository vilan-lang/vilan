# std::style reference

Typed, compile-time atomic styles. Concepts and the emission model: the
[styling guide](../guide/styling.md).

```vilan,fragment
import std::style::{
	style, space, Style, Length, Color,
	Display, Position, FlexDirection, AlignItems, JustifyContent,
	TextAlign, Cursor, Overflow, WhiteSpace, UserSelect,
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
	fun var(name: str): Length     // a CSS custom-property reference ("--w")
	fun calc(expression: str): Length  // "100% - 2rem" — no calc(..) wrapper
}

impl Color {
	fun white(): Color
	fun black(): Color
	fun transparent(): Color
	fun hex(value: str): Color     // "#663399"
	fun gray(step: i32): Color     // ramps: 50…900
	fun blue(step: i32): Color
	fun red(step: i32): Color
	fun green(step: i32): Color
}
```

Keyword enums: `Display` (Flex, Block, …), `Position`, `FlexDirection`,
`AlignItems`, `JustifyContent`, `TextAlign`, `Cursor`, `Overflow`,
`WhiteSpace` (Normal, Nowrap, Pre, PreWrap, PreLine), `UserSelect` (Auto,
Text, All, **Off** — `none`, named to stay clear of `Option::None`, like
`Display::Hidden`).

## Style methods

Every method returns a new `Style` with one more property slot; each slot is
one atomic rule, deduplicated build-wide.

Layout:

| Method | Value |
|---|---|
| `display` | `Display` |
| `position` | `Position` |
| `flex_direction` | `FlexDirection` |
| `align_items` | `AlignItems` |
| `justify_content` | `JustifyContent` |
| `flex` | `str` — the shorthand, `"1 1 auto"` |
| `flex_shrink` | `f64` |
| `grid_template_columns` | `str` — `"repeat(3, 1fr)"` |
| `gap`, `padding`, `padding_x`, `padding_y`, `margin`, `margin_x`, `margin_y` | `Length` |
| `width`, `height`, `min_width`, `max_width`, `min_height`, `max_height` | `Length` |
| `top`, `right`, `bottom`, `left`, `inset` | `Length` |
| `overflow` | `Overflow` |

Appearance:

| Method | Value |
|---|---|
| `radius` | `Length` |
| `border` | `(width: Length, color: Color)` |
| `border_color` | `Color` — its own slot, so a `hover` can recolour without restating the width |
| `box_shadow` | `str` |
| `background`, `color` | `Color` |
| `font_family` | `str` |
| `font_size` | `Length` |
| `font_weight` | `i32` |
| `line_height` | `f64` |
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
fun pseudo(self, name: str, inner: Style): Style

fun sm(self, inner: Style): Style          // breakpoints (min-width):
fun md(self, inner: Style): Style          // 640px, 768px, 1024px, 1280px
fun lg(self, inner: Style): Style
fun xl(self, inner: Style): Style
fun media(self, min_width: str, inner: Style): Style
```

### Stacking

The three condition axes nest **outside-in, in the order the selector
nests them** — media, then dark, then the pseudo-class:

```vilan,fragment
style().md(style().dark(style().hover(style().opacity(0.8))))
// @media (min-width: 768px){:root[data-theme="dark"] .sX:hover{opacity:0.8}}
```

Every other order is a compile-time-evaluation panic naming the fix
(`hover(dark(..))` says to write `dark(hover(..))`), and no axis can wrap
itself — one media, one dark, one pseudo-class per slot. Media rules emit
in ascending min-width order, so a chain like `.sm(x).lg(y)` is
mobile-first: the widest matching breakpoint wins.

`dark` is an ancestor selector, so a composed `dark(hover(..))` rule is
more specific than either `dark(..)` or `hover(..)` alone and wins
against both. Between an *un*composed `dark(x)` and `hover(y)` on the
same property the two are equally specific and dark wins, so use
`dark(hover(..))` when a dark theme needs its own hover.

## Runtime-legal operations

Construction emits rules and therefore lives in `const`; these do not emit
and work anywhere:

```vilan,fragment
style_a + style_b          // merge: per-property, right side wins (impl Add)
style.class_list(): str    // the space-joined class attribute (what `styled` uses)
```
