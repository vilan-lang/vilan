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
- There is **no reset unless you ask for one**. Browser defaults are in
  force, so `body` keeps its 8px margin and
  `width(px(200)).padding(space(4))` measures 232px, not 200. Add
  `let _reset = const preflight();` for the opinionated base stylesheet
  (`box-sizing: border-box` everywhere, margins zeroed, form-control
  chrome stripped) — it lives in its own cascade sub-layer, so every
  style you write still wins against it. The
  [reference](../std/style.md#declaration-blocks) has what it contains.

## The `css` block

The chain has a second spelling that reads like the CSS you already
know. A `css { … }` block **is** a `Style` — it is sugar over the chain
above, lowered before anything else in the compiler sees it — so the two
forms mix freely in one file, one function, one expression, and both
emit exactly the same stylesheet.

Here is one style written both ways, in one program. `card` and
`card_as_a_chain` mint the *same classes*: the block does not emit
beside the chain, it becomes the chain.

```vilan,browser
import std::ui::{ view, View, mount_root };
import std::style::{ style, space, Style, Color };

let card = const css {
	display: flex;
	gap: {space(2)};
	padding: {space(4)};
	background-color: {Color::gray(100)};
	.hover {
		background-color: {Color::gray(200)};
	}
};

let card_as_a_chain = const style()
	.raw("display", "flex")
	.raw("gap", space(2))
	.raw("padding", space(4))
	.raw("background-color", Color::gray(100))
	.hover(style().raw("background-color", Color::gray(200)));

fun main() {
	let _root = mount_root("app", || {
		view("div").styled(card).child(view("p").text("hello"))
	});
}
```

**One rule, and the whole feature falls out of it.** An undotted
`property: value;` is a declaration and becomes `.raw(property, value)`;
a dotted `.name { … }` is a condition combinator and becomes
`.name(style() … )`, with the block's own chain as its last argument.
The dot is the only thing the grammar looks at, so every condition
method works inside a block — `.hover`, `.md`, `.within("data-theme",
"dark")`, `.children`, `.attribute("data-open", "true")` — including
ones added later, and nesting order is combinator order: media outside,
then the relation, then the attribute, then the pseudo-class.

```vilan,fragment
let panel = const css {
	color: {Color::gray(900)};

	.within("data-theme", "dark") {
		color: {Color::gray(50)};
	}

	.children {
		margin-top: {space(2)};
	}
};
```

Values are text and **holes**. Anything you can write in CSS rides
through verbatim — `repeat(3, 1fr)`, `url("tile.png")`, `50%`, `1.5rem`
— and `{expression}` drops a typed vilan value in. A value that is
*exactly* one hole keeps its type, which is what carries a token's
`:root` line onto the sheet, so write `gap: {space(4)};` rather than
`gap: 1rem;` when you mean the scale.

Four things the block does not do, each on purpose:

- **The `;` is required**, including after the last declaration.
- **`#` and `@` are not vilan characters.** A colour is
  `{Color::hex("#663399")}` — which routes it through `Color`, so its
  `:root` line travels with it — and a media query is `.md { … }`.
  There are no at-rules; a declaration block under a selector of your
  own is [`declare`](../std/style.md#declaration-blocks).
- **`!important` is refused.** Merging a style is a record update, so a
  later declaration on the same property already wins.
- **A block is brace-initial**, like a struct literal, so a condition,
  a `for … in` iterable and a `match` subject take one only in
  parentheses: `if (css { … }).class_list() != "" { … }`.

**`vilan fmt` orders a block, and orders it exactly as it orders the
chain.** One item per line, nested rules one level in, holes tidied like
any other vilan expression — and the items sorted into the canonical
order: properties in Tailwind's category sequence, then the condition
rules in the order the selector nests them (media, relation, attribute,
pseudo-class). So the two spellings of one style format alike, and
grouping declarations by hand is not a thing you have to maintain.

Two carve-outs worth knowing. A property no `Style` method writes — a
vendor prefix, a custom property like `--brand-ink` — is a **barrier**:
it holds its place and nothing sorts across it, because the formatter
cannot know what it is entangled with. And a block containing a
**comment** is never reordered at all; it still prints canonically, but
the items stay where you wrote them, so a comment can never end up
explaining the wrong declaration.

```vilan,fragment
// formats as: display, padding, then `.md` before `.hover`
let button = const css {
	.hover { background-color: {Color::gray(200)}; }
	padding: {space(2)};
	.md { padding: {space(4)}; }
	display: flex;
};
```

## Getting the stylesheet onto the page

The build writes every emitted rule into a sidecar beside the bundle —
`app.js` gets `app.css`, `dist/client.js` gets `dist/client.css` — and
**your page has to link it**. Nothing injects the tag for you: the HTML
shell is yours, not the compiler's.

```text
<link rel="stylesheet" href="app.css" />
```

Both `vilan init` browser templates already carry the line, and the
fullstack one serves the sidecar without naming it — `serve_build` routes
every artifact the build wrote, the stylesheet among them:

```vilan,fragment
Server::builder().port(8080).serve_build(require_build("client"))
```

That also means a leg that stops emitting styles stops having them
served, with no `fs::stat` guard to remember: the build says whether it
wrote a sidecar, and the server believes it.

Miss the link and the app runs unstyled while the compiler faithfully
rebuilds a stylesheet nobody loads — which is why a server can hold its
shell against its build and refuse to start over exactly that:

```vilan,fragment
let page = require_shell("src/app.html", build).html();
```

`require_shell` (`std::document`, the
[reference](../std/process.md#stddocument)) checks the file every boot: a
shell that links no stylesheet over a build that emitted one stops the
server, naming the file and the fix. The fullstack template ships that
line. Nothing checks a browser-only project's `index.html` — there is no
server to check it — so that one is still on you.

A `<link>` (rather than an inlined `<style>`) is also what lets `--watch`
hot-swap CSS without reloading the page; see
[the dev loop](dev-loop.md#the-css-link-idiom).

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
  `Length::rem(1.5)`, `Length::em(0.02)`, `Length::pct(50.0)`,
  `Length::vh(100.0)`, `Length::vw(50.0)`, `Length::auto()`,
  `Length::zero()` for a bare `0`, `Length::var("--w")` for a CSS
  variable (see dynamic values below), and
  `Length::calc("100% - 2rem")` when the value is arithmetic — you write
  the expression, not the `calc(..)` wrapper.
- **`Length::raw(..)`** is the verbatim escape, `Color::hex`'s twin: a
  complete CSS value written as text, for the functional forms `Length`
  does not model — `Length::raw("clamp(1100px, 100vw, 1920px)")`,
  `min()`, `max()`, `env()`, `fit-content()`. Use `calc` when you are
  writing *arithmetic* and want the wrapper supplied; use `raw` when the
  value is already whole, which is also what lets one named expression be
  reused across several properties. An empty value stops the build.
- **`Color`** has `Color::white()`, `Color::black()`,
  `Color::transparent()`, `Color::hex("#663399")`, and stepped ramps
  like `Color::gray(300)`, `Color::blue(600)`, `Color::red(500)`,
  `Color::green(500)`.
- **Alpha** comes two ways. `Color::rgba(27, 6, 13, 0.9)` is a literal
  translucent colour — `hex`'s twin, for a palette outside the ramps.
  `some_color.alpha(0.08)` is *this colour at that alpha*, and is the
  one to reach for on a ramp step: it keeps the token underneath, so
  `Color::gray(900).alpha(0.08)` still re-themes when `--gray-900`
  changes. Both check their range at build time — `alpha(1.5)` stops the
  build.
- **`Gradient`** is a `background-image` value, not a `Color`.
  `Gradient::linear(degrees)` (0 points up, 90 to the right) or
  `Gradient::radial(RadialExtent::ClosestSide)`, then `.stop(colour,
  percent)` per stop, handed to `background_gradient`. It is a different
  slot from `background`, so a style can set a colour *and* paint a
  gradient over it. Two stops minimum. For the image values a `Gradient`
  can't hold — a `url()` or data URI, a multi-layer list, a positioned or
  `repeating-*` gradient — `background_image(str)` writes the *same* slot,
  and `background_size(str)` sizes it.
- Keyword properties use enums: `Display`, `Position`, `FlexDirection`,
  `AlignItems`, `JustifyContent`, `TextAlign`, `Cursor`, `Overflow`,
  `WhiteSpace`, `UserSelect`, `RadialExtent`. Each is a **backed enum**
  carrying its CSS keyword (`AlignItems::Start` is `"flex-start"`), so the
  variant *is* the keyword the browser reads — `.value()` hands it back and
  `Display::parse(text)` goes the other way, `None` outside the set.

```vilan,browser
import std::ui::{ view, View, mount_root };
import std::style::{ style, space, Style, Color, Gradient, Length, RadialExtent };

let hero = const style()
	.padding(space(6))
	.radius(space(2))
	.background(Color::gray(900))
	.background_gradient(
		Gradient::linear(135.0)
			.stop(Color::rgba(178, 48, 86, 0.9), 0.0)
			.stop(Color::blue(600), 100.0),
	)
	.border_top(Length::px(1), Color::white().alpha(0.14))
	.color(Color::white());

let glow = const style().background_gradient(
	Gradient::radial(RadialExtent::ClosestSide)
		.stop(Color::rgba(235, 104, 46, 0.4), 0.0)
		.stop(Color::transparent(), 100.0),
);

fun main() {
	let _root = mount_root("app", || {
		view("div").styled(hero).child(view("div").styled(glow))
	});
}
```

Some properties take a plain `str` — `font_family`, `transform`,
`box_shadow`, `text_decoration`, `flex`, `grid_template_columns`,
`background_image`, `background_size`. That
isn't a weaker `raw`: the property name is still checked and completable,
and only the *value* is a CSS expression there is nothing to validate (a
font stack, a transform list). Reach for them the same way you reach for
`padding`.

`line-height` has two methods rather than one, because it takes two kinds
of value and the language has no overloading. Prefer **`line_height(1.5)`**:
a unitless number inherits as a *ratio* and re-computes against each
descendant's own font size. Reach for **`line_height_length(Length::px(24))`**
when a design specifies a leading in absolute units — a length inherits as a
computed length and will not track a child that resizes its text. Both write
the same slot, so a later one simply replaces an earlier one.

For anything the typed surface doesn't cover, the escape hatch is `raw`:

```vilan,fragment
.raw("clip-path", "polygon(0 0, 100% 0, 100% 80%)")
.raw("scroll-margin-top", space(4))
.raw("outline-color", Color::blue(300))
```

`raw` takes any property, and any *value* the CSS channel understands: a
complete value written as a `str`, or a `Length` or `Color` — including a
theme token like `space(4)` or `Color::blue(300)`, which carries its own
`:root` declaration onto the stylesheet exactly as a typed property method
does. `with_length` and `with_color` are the same thing under older names
and stay available; they are `raw` at those two value types.

Reach for the value, not its text. A token is a *pair* — the reference
(`var(--space-4)`) and the `:root` line that declares it — and reading the
`.text` field of one hands over the reference alone, so `space(4).text` puts a
`var()` on the sheet that nothing defines. (That is the field, not the
`Length::raw(..)` constructor above, which is a value in its own right and
declares nothing to lose.) Passing `space(4)` itself keeps the pair together.

The typed surface grows by demand — if you find yourself reaching for `raw`
on the same property repeatedly, that is the evidence a method should exist.

## Boxes, edges and borders

Spacing comes in three arities, and the name says which: the whole box
(`padding`, `margin`), an axis (`padding_x`, `margin_y`), or one edge
(`padding_top`, `margin_left`, and the other six). There is no
multi-value shorthand method, because there is nothing it would buy:
`padding: 8px 16px` is `padding_y(..).padding_x(..)`, spelled with the
methods you already have. The two axes cover all four edges between them
— exactly what the shorthand covers — so the composed form also *resolves*
like the shorthand wherever it meets one (see mixing arities, below).

Borders match: `border(width, colour)` for all four edges,
`border_top`/`border_right`/`border_bottom`/`border_left` for one, and
`border_none()` to remove one. `border_none()` fills the *same* slot the
shorthand does, so `base.border_none()` genuinely takes the border off a
style that set one. `border_color` is its own slot, which is what lets a
`hover` recolour a border without restating its width.

```vilan,fragment
let card = const style()
	.border(Length::px(1), Color::gray(300))
	.hover(style().border_color(Color::blue(600)));

let flush = const card.border_none().margin_left(Length::auto());
```

**Mixing arities is fine, and it resolves in the order you wrote it.**
A property that covers others — `padding` over its edges, `margin`,
`inset` over `top`/`right`/`bottom`/`left`, `border` over its parts,
`background`, `flex` — forms a *family*, and last-wins holds across the
whole family, not just one property. So
`padding(space(4)).padding_top(space(0))` is `1rem` on three edges and
`0` on the top, and `padding_top(space(0)).padding(space(4))` is `1rem`
all round: the later whole-box value replaces the edge outright, exactly
as a second `padding(..)` would. The same holds across `+`, and across
`raw` (a `raw("margin-left", "auto")` belongs to the `margin` family
like the method does).

```vilan,fragment
// A tight box with one edge opened up, and a card whose border is
// recoloured — both read top to bottom, like the rest of the chain.
let panel = const style().padding(space(4)).padding_top(space(0));
let lit = const card + style().border_color(Color::blue(600));
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
`.last`, and `.pseudo(name, inner)` for anything else.
Breakpoints work the same way: `.sm(inner)` (640px), `.md(inner)`
(768px), `.lg(inner)` (1024px), `.xl(inner)` (1280px), or
`.media(min_width, inner)`. All are `min-width` conditions, so chains are
mobile-first: in `.sm(grid_cols(2)).lg(grid_cols(3))` the widest matching
breakpoint wins (the stylesheet emits media rules in ascending min-width
order, which is what makes that true).

## Theming, and stacking conditions

`.within(name, value, inner)` applies under an **ancestor** carrying the
attribute — `within("data-theme", "dark", ..)` is the theme condition,
under a `[data-theme="dark"]` switch you set on the document, not
`prefers-color-scheme`. That is deliberate: a server can decide the theme
and write the attribute before a byte of JavaScript runs, and a user's
toggle is one attribute write. Nothing is special about the theme: any
ancestor state rides — an n-ary theme id (`within("data-theme",
"iron-dark", ..)`), a density mode, a `[data-collapsed]` sidebar.

For colours, the stronger recipe is usually no condition at all: declare
per-theme custom properties with a [declaration
block](../std/style.md#declaration-blocks) and read them with
`Color::var(..)` — switching themes then re-paints every element through
the variables, and `within` covers the *structural* changes a value swap
cannot express.

Conditions **stack**, nesting outside-in in the order the CSS nests them:
a breakpoint outside the guard, the guard outside the pseudo-class.

```vilan,fragment
let button = const style()
	.background(Color::gray(100))
	.hover(style().background(Color::gray(200)))
	.within("data-theme", "dark", style().background(Color::gray(800)))
	.within("data-theme", "dark", style().hover(style().background(Color::gray(700))))
	.md(style().within("data-theme", "dark", style().hover(style().background(Color::gray(600)))));
```

Write them in any other order and the build stops and tells you which
order it wanted — `hover(within(..))` says to write `within(..,
hover(..))`. No axis may wrap itself, so one media, one guard and one
pseudo-class is the whole lattice.

Why the order matters beyond spelling: `within(.., hover(..))` produces a
*more specific* selector than either `within(..)` or `hover(..)`, so it
beats both. Between a plain `.within(.., x)` and a plain `.hover(y)` on
the same property the guard wins — a theme shouldn't be undone by a hover
— so when a dark theme needs its own hover colour, say so with
`within(.., hover(..))`.

## Styling children from the parent

`.children(inner)` styles every direct child of the element, and
`.divide(inner)` every direct child but the first — the parent-owned
spacing idioms (Tailwind's `space-*` and `divide-*`):

```vilan,fragment
let list = const style()
	.children(style().padding_y(space(2)))
	.divide(style().border_top(Length::px(1), Color::gray(200)));
```

Two rules make this safe to use anywhere. First, **a child's own style
always wins**: a `children`/`divide` rule is emitted in a lower cascade
layer, so anything the child says about itself — through its own
`style()` — overrides what its parent reaches in with, whatever the
selectors' specificity. They set defaults the child may refuse; they are
not a way to force a child's hand. Second, where `children` and `divide`
touch the *same* property, `divide` wins on every child but the first —
the narrower relation outranks the blanket, whichever you wrote first.

Both take an unconditioned inner style: to give the children a hover
colour, put the `hover(..)` on the child's own style.

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

## Conditional merges: `when`

When the style depends on a handful of independent flags, `+` and `if`
turn into a small pile of rebinding. `when(condition, delta)` is that
pile as a chain — `self + delta` when the condition holds, `self`
untouched when it doesn't:

```vilan,browser
import std::ui::{ view, View, mount_root };
import std::style::{ style, space, Style, Color };

let base = const style().padding(space(2)).color(Color::gray(900));
let chosen = const style().background(Color::blue(100)).color(Color::blue(900));
let muted = const style().color(Color::gray(400));

fun row(is_chosen: bool, is_muted: bool): View {
	view("li").styled(base.when(is_chosen, chosen).when(is_muted, muted))
}

fun main() {
	let _root = mount_root("app", || view("ul").child(row(true, false)));
}
```

`when` selects; it never builds. Both sides were constructed in `const`,
so the construct-in-const rule holds with a runtime flag in the middle,
exactly as it does for `bind_styled` below.

Chain order is **precedence**: when two `when`s both fire, the later
delta wins whatever properties they share — the same rule `+` follows.

The chain reads best when each condition mentions its **own** flag. If
one condition has to mention another link's flag (`!selected &&
!disabled`), the states aren't independent, and a `match` says that
structurally where a chain only implies it. The compound condition is
the tell.

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
> `styled` sets `class_list()`, the space-joined class names. Each
> combination of conditions is its own slot, so `hover(..)` and
> `within(.., hover(..))` never fight over one — they are different rules
> with different class names, resolved by CSS specificity.

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
- A shorthand and its own longhands (`padding` with `padding_top`,
  `border` with `border_color`) resolve by the order you wrote them, not
  by specificity: a later longhand narrows the shorthand, a later
  shorthand replaces the whole family. This holds per condition, so a
  `within` or `hover` variant of one family never disturbs the base.

Full method table: the [style reference](../std/style.md).
