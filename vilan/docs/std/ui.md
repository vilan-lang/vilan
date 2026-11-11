# Elements and layout

`std::ui` builds a window's contents from five element kinds — `row`,
`column`, `stack`, `text`, `image` — and lays them out for you: every box comes
from the elements' gap, padding, size, grow, shrink, align, justify and anchor,
and a paragraph wraps to the width it is given. None of the kinds has a look of
its own; a look is a `Style` value. It stands on [`std::gfx`](gfx.md), which
draws the result (native-ui.md, part P2). The
[native guide](../guide/native.md#elements-and-layout) walks through a whole
screen.

**Native only**, as `std::gfx` is. The JavaScript build refuses a program that
reaches this module, naming it; `vilan check` accepts one. What is plain vilan —
a `Style` and its `+`, the enums — runs anywhere; the refusal is by reach, at the
first element made.

Not here yet: hit testing, events, focus, `scroll`, style conditions
(`hovered()`, `pressed()`) and properties bound to signals are part P3; grid
layout is a later slice.

## Elements

```vilan,fragment
fun row(): Row             // children left to right
fun column(): Column       // children top to bottom
fun stack(): Stack         // children on top of one another
fun text(): Text           // a paragraph
fun image(): ImageElement  // a texture in a box

struct Element;            // an element of any kind
impl Element {
	fun bounds(self): Rect                    // its box from the last layout, in dp
	fun outline(self): List<OutlineEntry>     // every element in its tree, in paint order
}
struct OutlineEntry { depth: usize, kind: str, label: str, bounds: Rect }

trait Child { fun into_element(self): Element; }   // every kind, an Element, a str
```

Each kind is a function with no parameters that answers a builder, every
attribute is a method on it, and children arrive one at a time through `child`:

```vilan,native
import std::gfx::{ Size, dp };
import std::io::print;
import std::ui::{ Align, column, layout, row, text };

fun main() {
	let screen = column()
		.padding(dp(16))
		.gap(dp(8))
		.child(text().child("Settings"))
		.child(row().gap(dp(12)).align(Align::Center).child("Name").child("Ada"));
	layout(screen, Size::new(dp(320), dp(200)), 1.0);
	for entry in screen.element().outline() {
		print(i"{entry.kind} at {entry.bounds.x.value()}, {entry.bounds.y.value()}");
	}
}
main();
```

`child` takes anything `Child`: an element of any kind, or a `str`, which
becomes a `text` holding it. A `text`'s own `child` takes a `str` and appends it
to the paragraph. Elements are RETAINED: an element is built once and kept, a
copy of one is the same element, and an element is in one place — giving it a
second parent, or making it a child of itself or of an element inside it, fails
the program with the reason. Build a second element for a second place (a
function that answers one is the usual way). An element no longer held by the
program or by a parent is freed.

## Attributes every element has

```vilan,fragment
trait Attributes {
	fun width<L: IntoLength>(self, width: L): Self       // dp(240), fraction(1, 2), Length::Auto
	fun height<L: IntoLength>(self, height: L): Self
	fun min_width<L: IntoLength>(self, width: L): Self
	fun max_width<L: IntoLength>(self, width: L): Self
	fun min_height<L: IntoLength>(self, height: L): Self
	fun max_height<L: IntoLength>(self, height: L): Self
	fun grow(self, grow: f64): Self                      // 0 until set
	fun shrink(self, shrink: f64): Self                  // 1 until set
	fun padding(self, padding: Dp): Self                 // every side
	fun padding_x(self, padding: Dp): Self               // left and right
	fun padding_y(self, padding: Dp): Self               // top and bottom
	fun padding_top(self, padding: Dp): Self             // and _right, _bottom, _left
	fun anchor(self, anchor: Anchor): Self               // its place in a stack
	fun style(self, style: Style): Self
	fun role(self, role: Role): Self                     // for assistive technology
	fun label(self, label: str): Self
}
enum Length { Auto, Fixed(Dp), Fraction(f64) }
fun fraction(numerator: f64, denominator: f64): Length
```

`dp` is the only length unit; a `fraction` is of the parent's content box (inside
its padding). `Auto` is the default: an element is as large as its content, and
across a row or a column it stretches unless the parent's `align` says not to.
There are NO MARGINS: the space between siblings is the parent's `gap`, the space
inside an element its `padding`, and nothing else moves a box.

`grow` is an element's share of a row's or a column's leftover space along its
axis, against its siblings' shares. `shrink` is its share of the shortfall when
the children do not fit: every child shrinks by default, in proportion to its
size, so an element that must keep its width — a sidebar beside a long
paragraph — says `shrink(0.0)`. No element shrinks below its content's smallest
width (a paragraph's longest word).

`role` and `label` are carried from this version for assistive technology
(`Role::Heading`, `Button`, `Link`, `List`, `ListItem`, `Image` and the rest),
and published to the system by a later one; `outline` lists the label too.

## Rows and columns

```vilan,fragment
trait Parent with Attributes {
	fun child<C: Child>(self, content: C): Self
}
trait Flex with Parent {                      // rows and columns
	fun gap(self, gap: Dp): Self
	fun align(self, align: Align): Self       // across the axis
	fun justify(self, justify: Justify): Self // along the axis
	fun wrap(self): Self                      // overflow starts a new line
}
enum Align { Start, Center, End, Stretch, Baseline }
enum Justify { Start, Center, End, SpaceBetween, SpaceAround, SpaceEvenly }
```

A row lays its children left to right and a column top to bottom, `gap` apart,
stretched across (`Align::Stretch`) and packed at the start (`Justify::Start`)
until told otherwise. `Align::Baseline` lines a row's children up on their first
baselines. `wrap()` moves children that do not fit to a new line, the lines
`gap` apart too. These are flexbox's semantics under our own names; the solver
is `taffy`, and nothing of it shows here.

## Stacks

```vilan,fragment
enum Anchor { TopLeft, Top, TopRight, Left, Center, Right, BottomLeft, Bottom, BottomRight, Fill }
```

A stack draws its children on top of one another, later ones above, each at its
`anchor` inside the stack's padding — the top left until it says otherwise;
`Fill` covers the whole stack. A stack is as large as its largest child unless
it sizes itself. A badge on a picture's corner is a stack:

```vilan,native
import std::bytes::Bytes;
import std::gfx::{ Color, Size, Texture, dp };
import std::io::print;
import std::ui::{ Anchor, Element, image, layout, stack, style, text };

fun badged(picture: Texture): Element {
	stack()
		.child(image().texture(picture).width(dp(120)))
		.child(text().child("NEW").padding_x(dp(6)).anchor(Anchor::TopRight)
			.style(style().background(Color::rgb(230, 60, 70)).text_color(Color::white()).radius(dp(8))))
		.element()
}

fun main() {
	match Texture::from_rgba(4, 3, Bytes::alloc(48)) {
		Ok(let picture) => {
			let card = badged(picture);
			layout(card, Size::new(dp(320), dp(200)), 1.0);
			for entry in card.outline() {
				// stack, then the image (120 x 90: its 4:3 aspect kept), then the badge
				print(i"{entry.kind} {entry.bounds.width.value()} x {entry.bounds.height.value()}");
			}
		},
		Err(let reason) => print(reason),
	}
}
main();
```

The root of a tree fills its window unless it sizes itself.

## Text and images

A `text` is a paragraph of the runs its `child` calls append, in the text style
it inherits and sets. It is as wide as its one line unless its parent gives it
less; then it wraps at the width it is given, and it is never narrower than its
longest word. Text is shaped once per width and style, as `std::gfx`'s
`TextLayout` is, and drawn at the width layout gave it.

```vilan,fragment
impl ImageElement {
	fun texture(self, texture: Texture): ImageElement
	fun fit(self, fit: Fit): ImageElement
}
enum Fit { Contain, Cover, Fill, None }
```

An `image` shows a `std::gfx` `Texture`. Its natural size is the texture's, one
pixel a dp; given one side, it keeps the texture's aspect. `fit` says how the
texture fills the box: inside it with the aspect kept (`Contain`, the default),
covering it with the overflow clipped (`Cover`), stretched (`Fill`), or at its own
size, centred and clipped (`None`). A style's radius rounds the picture.

## Style

```vilan,fragment
struct Style {
	background: Option<Color>,
	border: Option<Border>,          // drawn inside the box
	radius: Option<Corners>,
	shadow: Option<DropShadow>,
	opacity: Option<f64>,
	font: Option<FontFamily>,        // the text fields, inherited
	text_size: Option<Dp>,
	text_color: Option<Color>,
	line_height: Option<f64>,
	weight: Option<i32>,
	italic: Option<bool>,
}
fun style(): Style                   // sets nothing
impl Style {
	fun background(own self, color: Color): Style
	fun border(own self, border: Border): Style
	fun radius(own self, radius: Dp): Style
	fun corners(own self, corners: Corners): Style
	fun shadow(own self, shadow: DropShadow): Style
	fun opacity(own self, opacity: f64): Style
	fun font(own self, font: FontFamily): Style
	fun text_size(own self, size: Dp): Style
	fun text_color(own self, color: Color): Style
	fun line_height(own self, line_height: f64): Style
	fun weight(own self, weight: i32): Style
	fun italic(own self, italic: bool): Style
}
// Style + Style: the right side's fields where it sets them
struct DropShadow { x: Dp, y: Dp, blur: Dp, spread: Dp, color: Color }   // DropShadow::new(x, y, blur, color)
```

A style is a VALUE: build it once, keep it, combine it. `a + b` is `a` with every
field `b` sets replaced, and setting a style on an element (`.style(s)`) sets the
fields it has, so a second `.style` composes onto the first. There are no
selectors and no cascade: what a style sheet does with selectors, a program does
with functions that answer styles.

```vilan
import std::gfx::{ Color, dp };
import std::io::print;
import std::ui::style;

fun main() {
	let card = style().background(Color::white()).radius(dp(8)).text_size(dp(14));
	let warning = style().background(Color::rgb(255, 240, 200)).weight(700);
	let warning_card = card + warning;
	print(warning_card.background.unwrap().blue);   // the warning's background
	print(warning_card.text_size.unwrap().value());   // the card's size
	print(warning_card.weight.unwrap());
}
main();
```

TEXT STYLE is the one inherited thing: a font, size, colour, line height, weight
or slant set on an element reaches every text inside it that does not set its
own, field by field. Everything else belongs to the one element it is set on. A
text colour change repaints; a change to any other text field lays the texts
under it out again.

## Showing it

```vilan,fragment
fun mount<C: Child>(window: Window, root: C)            // the window shows it
fun draw<C: Child>(frame: Frame, root: C)               // in a frame of your own
fun layout<C: Child>(root: C, size: Size, scale_factor: f64)   // boxes only
```

`mount` makes the tree the window's frame callback: every frame lays it out at
the window's size and scale and draws it. The window draws a frame when it
opens, resizes or changes scale, so the layout follows the window. Layout and
painting are separate passes over the kept tree: the layout solver runs only when
something that moves a box changed, or the size did; a scale change alone snaps
the boxes again; and a frame with nothing changed draws the scene it painted
last.

```vilan,native
import std::gfx::{ Window, dp };
import std::io::print;
import std::ui::{ column, mount, text };

fun main() {
	match Window::builder().title("Hello").size(dp(320), dp(160)).open() {
		Ok(let window) => mount(window, column().padding(dp(24)).child(text().child("Hello from std::ui"))),
		Err(let reason) => print(i"no window: {reason}"),
	}
}
main();
```

`draw` lays a tree out at a frame's size and scale and draws it into that frame,
for a frame callback that draws more around it, or for an
[`Offscreen`](gfx.md#offscreen) picture: a golden image of a screen is
`Offscreen::new(..).software(true).render(|frame: Frame| draw(frame, root))`.
`layout` only lays out, for reading boxes back with `bounds` and `outline`.

Every box is SNAPPED to the device pixel grid by the renderer's own rule, each
edge on its own: boxes that share an edge in dp share it on the screen, and a
1 dp line is one device pixel at 100%, 125% and 150%. `bounds` answers the
snapped box in dp.
