# Styling

`std::style` gives you typed, checked CSS without writing a stylesheet.
You build a `Style` value in code, the compiler evaluates it during the
build and writes real CSS rules into your bundle's `.css` file, and at
runtime the style is nothing but a set of class names on an element.

If you've used Tailwind, the feel is similar (small composable pieces, a
spacing scale, color ramps), except the pieces are typed function calls,
so a typo is a compile error instead of a silently-ignored class.

```vilan,browser
import std::ui::{ view, View, mount_root };
import std::style::{ style, space, Style, Color, Length, Display, FlexDirection };

let card = const style()
	.display(Display::Flex)
	.flex_direction(FlexDirection::Column)
	.gap(space(2))
	.padding(space(4))
	.radius(space(1))
	.background(Color::gray(100));

fun main() {
	let _root = mount_root("app", || {
		view("div").styled(card).child(view("p").text("hello"))
	});
}
```

## The model

- `style()` starts an empty style. Every method fills one property and
  returns the new style, so you chain.
- Styles are built inside `const`, the compile-time evaluation
  prefix (see [Macros & const](../tour/macros-and-const.md)). The rules
  are emitted during the build.
- `view.styled(card)` puts the style's classes on the element.

At runtime you can still *select and combine* styles you already built.
`a + b` merges two styles (per property, the right side wins), and
picking one of two styles in an `if` is fine. What you can't do is
construct new rules at runtime: a bare `style()` chain outside `const`
is a compile error. That restriction is what keeps the CSS static and
the bundle predictable.

```vilan,fragment
let button = const style().padding_x(space(3)).radius(space(1));
let primary = const button + style().background(Color::blue(600)).color(Color::white());
```

## Values

- **`space(step)`** is the spacing scale: `space(1)` is 0.25rem, and the
  steps grow like Tailwind's. It's the usual argument to `padding`,
  `gap`, `margin`, and `radius`.
- **`Length`** covers everything else: `Length::px(1.0)`,
  `Length::rem(1.5)`, `Length::pct(50.0)`, `Length::auto()`, and
  `Length::var("--w")` for a CSS variable (see dynamic values below).
- **`Color`** has `Color::white()`, `Color::black()`,
  `Color::transparent()`, `Color::hex("#663399")`, and stepped ramps
  like `Color::gray(300)`, `Color::blue(600)`, `Color::red(500)`,
  `Color::green(500)`.
- Keyword properties use enums: `Display`, `Position`, `FlexDirection`,
  `AlignItems`, `JustifyContent`, `TextAlign`, `Cursor`, `Overflow`.

For anything the typed surface doesn't cover, escape hatches:

```vilan,fragment
.raw("font-family", "system-ui, sans-serif")
.with_length("scroll-margin-top", space(4))
.with_color("outline-color", Color::blue(300))
```

## States and breakpoints

Hover, focus, and friends take an **inner** style. Everything in the
inner style applies under that condition:

```vilan,fragment
let button = const style()
	.background(Color::blue(600))
	.hover(style().background(Color::blue(500)))
	.focus(style().raw("outline", "2px solid"))
	.disabled(style().opacity(0.5));
```

Available: `.hover`, `.focus`, `.active`, `.disabled`, `.first`,
`.last`, `.dark` (dark mode via `prefers-color-scheme`), and
`.pseudo(name, inner)` for anything else. Breakpoints work the same way:
`.sm(inner)` (640px), `.md(inner)` (768px), `.lg(inner)` (1024px),
`.xl(inner)` (1280px), or `.media(min_width, inner)`. All are `min-width`
conditions, so chains are mobile-first: in
`.sm(grid_cols(2)).lg(grid_cols(3))` the widest matching breakpoint wins
(the stylesheet emits media rules in ascending min-width order, which is
what makes that true).

## Dynamic values

Styles are static, so how does a progress bar grow? Through CSS custom
properties. The style declares a variable, and the element binds the
variable to a signal with `style_var`:

```vilan,browser
import std::ui::{ view, View, mount_root };
import std::style::{ style, Style, Length, Color };
import std::reactive::Signal;

let bar = const style()
	.height(Length::rem(0.5))
	.width(Length::var("--progress"))
	.background(Color::green(500));

fun main() {
	let progress = Signal::new("40%");
	let _root = mount_root("app", || {
		view("div").styled(bar).style_var("--progress", progress)
	});
}
```

The rule is compiled once. Only the variable's value changes at runtime.
This one channel covers most "dynamic styling" needs — a value that
changes inside a rule.

## Swapping whole styles

When what changes is *which* style applies, not a value inside one, put
the style in a signal and bind it. `bind_styled` is to `styled` what
`bind_class` is to `class`:

```vilan,browser
import std::ui::{ view, View, mount_root };
import std::style::{ style, space, Style, Color };
import std::reactive::Signal;

let idle = const style().padding(space(2)).background(Color::gray(100));
let busy = const style().padding(space(2)).background(Color::blue(600));

fun main() {
	let state = Signal::new(idle);
	let _root = mount_root("app", || {
		view("div")
			.bind_styled(state)
			.child(view("button").text("start").on("click", || state.set(busy)))
	});
}
```

Both styles are built in `const`, so both sets of rules are in the
stylesheet before the page loads; the signal only chooses between class
strings that already exist. That is the construct-in-const rule holding
with a signal in the middle — you still cannot build a style at runtime,
and you never needed to.

Server-side, `bind_styled` reads the signal once, like every other
`bind_*` on the [SSR](ssr.md) layer: the style the signal holds when the
request is rendered is the one served.

> **Going deeper.** Each property-under-a-condition becomes one atomic
> CSS rule with a generated class name, deduplicated across the whole
> build: two styles that both say `padding(space(4))` share one class.
> `styled` sets `class_list()`, the space-joined class names. A
> breakpoint can't wrap another media-conditioned style (you'll get a
> compile-time panic saying so).

## Traps

- A `style()` chain outside `const` fails with an "emission outside
  const" error. Build styles in `const`, select and merge them at
  runtime.
- `+` is a per-property override, not CSS specificity. The right
  operand's value replaces the left's for the same property and
  condition.
- `.class(name)`, `.styled(style)`, `.bind_class(..)` and
  `.bind_styled(..)` all set the class attribute, so the later call
  wins — and a reactive one keeps winning every time its signal
  changes. Use one mechanism per element (custom classes can ride along
  via `.raw`).

Full method table: the [style reference](../std/style.md).
