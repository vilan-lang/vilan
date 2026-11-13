# Elements and layout

`std::ui` builds a window's contents from seven element kinds — `row`,
`column`, `stack`, `text`, `image`, `icon`, `scroll` — lays them out for you, and makes
them interactive: every box comes from the elements' gap, padding, size, grow,
shrink, align, justify and anchor; a paragraph wraps to the width it is given;
the element under the pointer or holding the focus hears the events it listens
for; and any attribute can follow a signal. None of the kinds has a look of its
own; a look is a `Style` value, and its conditions (`hovered()`, `pressed()`,
`focused()`, the window's width) are typed. It stands on [`std::gfx`](gfx.md),
which draws the result (native-ui.md, parts P2 and P3). The
[native guide](../guide/native.md#elements-and-layout) walks through a whole
screen and a counter.

**Native only**, as `std::gfx` is. The JavaScript build refuses a program that
reaches this module, naming it; `vilan check` accepts one. What is plain vilan —
a `Style` and its `+`, its conditions, the enums — runs anywhere; the refusal is
by reach, at the first element made.

A child can be reactive: a keyed list (`each`) or a conditional (`when`) keeps
its place among the static children and inserts, removes and moves elements as
its source changes ([below](#lists-and-conditional-children)).

Components with behaviour and no look are [`std::ui::kit`](#components-stduikit):
a button, a checkbox, a single-line text input, and a
[virtual list](#a-virtual-list) that holds only the rows near its viewport,
however long its list is. Every element's role, label and state reach the
system's accessibility API while an assistive tool listens, and the tool's actions
come back as pointer and keyboard input ([Accessibility](#accessibility)). Not
here yet: grid layout, text editing beyond one line, and input methods.

## Elements

```vilan,fragment
[element] fun row(): Row             // children left to right
[element] fun column(): Column       // children top to bottom
[element] fun stack(): Stack         // children on top of one another
[element] fun text(): Text           // a paragraph
[element] fun image(): ImageElement  // a texture in a box
[element] fun icon(): IconElement    // a path in a square box, in the text's colour
[element] fun scroll(): ScrollElement  // a clipped viewport over one child

struct Element;            // an element of any kind
impl Element {
	fun bounds(self): Rect                    // its box from the last layout, in dp
	fun outline(self): List<OutlineEntry>     // every element in its tree, in paint order
	fun focus(self)                           // gives it the focus
	fun scroll_into_view(self)                // every scroll around it shows it
	fun release_pointer(self)                 // ends a press held on it, and its capture
	fun interaction(self): Interaction        // hovered, pressed, focused now
	fun accessibility(self): str              // what an assistive tool is told of it
}
impl Element with Attributes                 // an Element takes every attribute too
struct OutlineEntry { depth: usize, kind: str, label: str, bounds: Rect }
struct Interaction { hovered: bool, pressed: bool, focused: bool }

trait Child { fun into_element(self): Element; }   // every kind, an Element, a str
trait ChildSlot { fun place(own self, parent: Element); }   // what `child` takes
```

Each kind is an `[element]` function with no parameters that answers a builder,
every attribute is a method on it, and children arrive one at a time through
`child`. So a kind is written as a TAG — `<row gap(dp(12))>"Name"{value}</row>`
lowers to `row().gap(dp(12)).child("Name").child(value)` — or as those calls,
and either spelling builds the same element. Tags read better wherever the shape
of the tree is fixed when you write it; the calls suit a tree built in a loop
(`rows.child(..)` per item) or extended as a value. A bare attribute is a call
with no arguments (`<row wrap focusable>`), static text is a quoted child, and a
value is a hole (`{count}`); the [element syntax](../guide/ui.md#markup-or-chain)
is the web UI's, and a file that imports none of these seven functions keeps the
web meaning of `<text>` and `<image>`.

```vilan,native
import std::gfx::{ Size, dp };
import std::io::print;
import std::ui::{ Align, column, layout, row, text };

fun main() {
	let screen = <column padding(dp(16)) gap(dp(8))>
		<text>"Settings"</text>
		<row gap(dp(12)) align(Align::Center)>"Name" "Ada"</row>
	</column>;
	// The same screen as the calls the tags lower to:
	let again = column()
		.padding(dp(16))
		.gap(dp(8))
		.child(text().child("Settings"))
		.child(row().gap(dp(12)).align(Align::Center).child("Name").child("Ada"));
	layout(screen, Size::new(dp(320), dp(200)), 1.0);
	layout(again, Size::new(dp(320), dp(200)), 1.0);
	for entry in screen.element().outline() {
		print(i"{entry.kind} at {entry.bounds.x.value()}, {entry.bounds.y.value()}");
	}
}
main();
```

`child` takes anything `ChildSlot`: anything `Child` — an element of any kind,
or a `str`, which becomes a `text` holding it — an `Option` of one (placed when
it is `Some`), or a reactive child ([Lists and conditional
children](#lists-and-conditional-children)). A `text`'s own `child` takes a `str` (or a signal of
one) and appends it to the paragraph. Elements are RETAINED: an element is built
once and kept, a copy of one is the same element, and an element is in one place —
giving it a second parent, or making it a child of itself or of an element inside
it, fails the program with the reason. Build a second element for a second place
(a function that answers one is the usual way). An element no longer held by the
program or by a parent is freed.

## Values and signals

```vilan,fragment
trait Bindable<T> { fun bind(own self, apply: |T| void); }
impl type T with Bindable<T>                       // a value: set once
impl type F: Flow<type T> with Bindable<T>         // a signal: set now and at every write
```

Every attribute that takes a value also takes a SIGNAL of it — a `SignalCell`, a
memo, a pipe (`count.derive(|n: i32| i"Count: {n}")`): the attribute follows it.
A write sets that one attribute again, which marks that one element for layout
(a size, a padding, a text) or for paint (a style, a texture's fit), and asks the
window the tree is mounted in for a frame. The frame lays out only what the
solver must — a text that changed is measured again, alone, and its parent
places its children from the solver's cache — and repaints only the elements
that changed and the chain of scenes above them. Nothing else runs.

`Bindable` is `std::reactive`'s `MaybeSignal` with one difference: a binding
belongs to the ambient owner when there is one (`mount_root`, `run_with_owner`)
and dies with it, and to its source otherwise, so a tree can be built and bound
at the top of `main`. (`MaybeSignal`'s reactive arm requires an owner, and the
native backend gives every function that could reach it an owner parameter,
whatever value a call passes: with it, every tree would have to be built under
`mount_root` or `comp`, static ones included. The two become one trait when the
backend stops doing that.)

## Attributes every element has

```vilan,fragment
trait Attributes {
	fun width<L: IntoLength, V: Bindable<L>>(self, width: V): Self   // dp(240), fraction(1, 2), Length::Auto
	fun height<L: IntoLength, V: Bindable<L>>(self, height: V): Self
	fun min_width<L: IntoLength, V: Bindable<L>>(self, width: V): Self
	fun max_width<L: IntoLength, V: Bindable<L>>(self, width: V): Self
	fun min_height<L: IntoLength, V: Bindable<L>>(self, height: V): Self
	fun max_height<L: IntoLength, V: Bindable<L>>(self, height: V): Self
	fun grow<V: Bindable<f64>>(self, grow: V): Self                   // 0 until set
	fun shrink<V: Bindable<f64>>(self, shrink: V): Self               // 1 until set
	fun padding<V: Bindable<Dp>>(self, padding: V): Self              // every side
	fun padding_x<V: Bindable<Dp>>(self, padding: V): Self            // left and right
	fun padding_y<V: Bindable<Dp>>(self, padding: V): Self            // top and bottom
	fun padding_top<V: Bindable<Dp>>(self, padding: V): Self          // and _right, _bottom, _left
	fun anchor<V: Bindable<Anchor>>(self, anchor: V): Self            // its place in a stack
	fun style<V: Bindable<Style>>(self, style: V): Self
	fun clip(self): Self                                              // children inside its box only
	fun role(self, role: Role): Self                                  // for assistive technology
	fun label<V: Bindable<str>>(self, label: V): Self
	fun checked<V: Bindable<bool>>(self, checked: V): Self            // see Accessibility
	fun disabled_state<V: Bindable<bool>>(self, disabled: V): Self
	fun items_shown(self, first: usize, count: usize): Self
	fun focusable(self): Self                                         // see Focus
	fun tab_index(self, index: i32): Self
	// and the events: see Events
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

`clip()` paints its children, and lets them be hit, inside its padding box only
(its border and rounded corners included); without it a child larger than its
parent draws outside it. A `scroll` always clips.

`role`, `label` and the three states are what assistive technology is told of
the element (`Role::Heading`, `Button`, `Link`, `List`, `ListItem`, `Image` and
the rest; see [Accessibility](#accessibility)); `outline` lists the label too.

## Rows and columns

```vilan,fragment
trait Parent with Attributes {
	fun child<C: ChildSlot>(self, content: C): Self
}
trait Flex with Parent {                      // rows and columns
	fun gap<V: Bindable<Dp>>(self, gap: V): Self
	fun align<V: Bindable<Align>>(self, align: V): Self       // across the axis
	fun justify<V: Bindable<Justify>>(self, justify: V): Self // along the axis
	fun wrap(self): Self                                      // overflow starts a new line
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
	fun texture<V: Bindable<Texture>>(self, texture: V): ImageElement
	fun fit<V: Bindable<Fit>>(self, fit: V): ImageElement
}
enum Fit { Contain, Cover, Fill, None }
```

An `image` shows a `std::gfx` `Texture`. Its natural size is the texture's, one
pixel a dp; given one side, it keeps the texture's aspect. `fit` says how the
texture fills the box: inside it with the aspect kept (`Contain`, the default),
covering it with the overflow clipped (`Cover`), stretched (`Fill`), or at its own
size, centred and clipped (`None`). A style's radius rounds the picture.

## Icons

```vilan,fragment
struct Icon { data: str, view_box: f64, paint: PathPaint }
impl Icon {
	fun new(data: str, view_box: f64, paint: PathPaint): Icon
	fun stroked(data: str, view_box: f64, width: f64): Icon   // round joins and caps
	fun filled(data: str, view_box: f64): Icon                // the non-zero rule
}
impl IconElement {
	fun of<V: Bindable<Icon>>(self, icon: V): IconElement
	fun size<V: Bindable<Dp>>(self, size: V): IconElement
	fun color<V: Bindable<Color>>(self, color: V): IconElement
}
```

An `Icon` is SVG path data in a square box (`view_box` units on a side, 24 for
Lucide, Tabler or Heroicons), stroked or filled as
[`std::gfx`'s paths](gfx.md#paths) are; an `icon` element draws one. It is drawn
in the TEXT colour it inherits, as SVG's `currentColor`, so an icon in a white
button is white and turns with the button's text when a hover changes it; `color`
sets its own. It is square: `size` on a side, or else the text size it inherits,
so an icon beside a label matches it. Its box measures as that square and is
snapped to the device grid like any box; a box made larger — stretched across a
column, or given a `width` — draws the icon at its side from the box's top-left
corner, as a text draws its glyphs, so `size` is what sizes an icon. Its path is
sharp at every size and scale (a coverage mask made at the size it is drawn at,
shared by every icon of the same shape).

```vilan,native
import std::gfx::{ Color, Size, dp };
import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ Align, Icon, Role, column, icon, layout, row, style, text };

fun check(): Icon {
	Icon::stroked("M20 6 9 17l-5-5", 24.0, 2.0)
}

fun chevron(open: bool): Icon {
	if open {
		Icon::stroked("m18 15-6-6-6 6", 24.0, 2.0)
	} else {
		Icon::stroked("m6 9 6 6 6-6", 24.0, 2.0)
	}
}

fun main() {
	let open: SignalCell<bool> = Signal::new(false);
	let screen = <column gap(dp(8)) style(style().text_size(dp(15)))>
		<row gap(dp(8)) align(Align::Center)>
			<icon of(check()) />
			<text>"Saved"</text>
		</row>
		<row gap(dp(6)) align(Align::Center) role(Role::Button) label("details")>
			<text>"Details"</text>
			<icon of(open.derive(chevron)) size(dp(18)) color(Color::rgb(46, 104, 220)) />
		</row>
		<icon of(check()) size(dp(24)) label("done") />
	</column>.element();
	layout(screen, Size::new(dp(320), dp(200)), 1.0);
	for entry in screen.outline() {
		print(i"{entry.kind} {entry.bounds.width.value()} x {entry.bounds.height.value()}");
	}
	print(screen.accessibility());
}
main();
```

`of`, `size` and `color` each take a value or a signal: the chevron above flips
when `open` writes, and that repaints the icon alone — its box does not depend on
its shape, so nothing is laid out again. An icon is **decorative** unless it has a
`label`: an assistive tool is told nothing of it (the `Saved` row is read as its
text, and the `Details` button by its label). Labelled, it is an image named by
its label — the last icon above is `Image "done"`.

Data that does not parse **fails the program** where the icon is given to an
element (`of`), with the reason naming the byte, as `Path::from_svg`'s `Err`
does: an icon's data is written in the program, or generated into it, so a typo
is a mistake to see at once and never an icon that silently draws nothing. Each
different data is parsed once however many icons show it. Path data that comes
from outside the program is checked first with `Path::from_svg`.

### Icon sets

Std ships no icons, as it ships no look: an icon set is a package of functions
answering `Icon` values, generated from the set's SVG files. This repository
carries one, Lucide's 1,999 icons (ISC), at `vilan/packages/lucide`: a
`[library]` generated by `scripts/lucide-native.mjs` from a pinned
`lucide-static` release, one function per icon, upstream's name with `-` as `_`
(`arrow-up-right` is `lucide::arrow_up_right()`; a keyword takes a trailing
underscore, `lucide::type_()`). A program takes it by a path dependency, relative
to its own `vilan.toml`:

```toml
[package.dependencies]
lucide = { path = "../../packages/lucide" }
```

```vilan,fragment
import lucide;

<row gap(dp(8)) align(Align::Center)>
	<icon of(lucide::check()) />
	<text>"Saved"</text>
</row>
```

`vilan/examples/ui-icons` is a row of them. An icon a program does not call costs
its binary nothing: the native build emits only the functions the program
reaches. Drawing one is native only, as all of `std::ui` is; a JavaScript build of
a program that draws one stops with the same message as for any `std::ui`
element. The package's README says how to regenerate it and what its licence asks.

An icon font (Material Symbols, Font Awesome: `register_font` and a text run)
works too, with no package. It is not the route to take: it names its icons by
code point or ligature, aligns them by font metrics, and says nothing to
assistive technology, where an `icon` is an image by its label.

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
	conditions: List<Conditional>,   // the `when` styles
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
	fun when(own self, condition: Condition, build: |Style| Style): Style
}
// Style + Style: the right side's fields where it sets them, both's conditions
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

## Conditions

```vilan,fragment
enum Condition { Hovered, Pressed, Focused, WindowAtLeast(Dp), WindowBelow(Dp) }
fun hovered(): Condition                    // the pointer over it, or over an element inside it
fun pressed(): Condition                    // the primary button went down on it and is still down, wherever the pointer is
fun focused(): Condition                    // it holds the focus
fun window_at_least(width: Dp): Condition
fun window_below(width: Dp): Condition
```

`when(condition, |s| ..)` adds fields to a style that apply only while the
condition holds; the framework decides that from hit testing, the focus and the
window. The conditions are typed and closed — these five — and a `when` inside a
`when` applies while both hold. A condition's fields win over the unconditional
fields whatever order the two were composed in; among conditions that hold, the
later wins.

```vilan
import std::gfx::{ Border, Color, dp };
import std::io::print;
import std::ui::{ Style, focused, hovered, pressed, style, window_below };

fun main() {
	let button = style()
		.background(Color::rgb(45, 95, 230))
		.radius(dp(6))
		.when(hovered(), |look: Style| look.background(Color::rgb(70, 120, 250)))
		.when(pressed(), |look: Style| look.background(Color::rgb(28, 70, 190)))
		.when(focused(), |look: Style| look.border(Border::all(dp(2), Color::rgb(255, 196, 60))))
		.when(window_below(dp(400)), |look: Style| look.radius(dp(0)));
	print(button.conditions.len());   // four conditional styles
}
main();
```

A condition change repaints the one element, and nothing else — unless its
fields shape text (a font, a size, a line height, a weight, italic), which the
solver then lays out again, as any change to those fields does. A text colour in
a condition reaches the texts inside the element, as an unconditional one does.

## Events

```vilan,fragment
trait Attributes {
	fun on_press(self, handler: || void): Self                // the primary button went down on it
	fun on_release(self, handler: || void): Self              // ... came up, after going down on it
	fun on_click(self, handler: || void): Self                // a press and its release, both on it
	fun on_pointer_enter(self, handler: || void): Self        // does not bubble
	fun on_pointer_leave(self, handler: || void): Self        // does not bubble
	fun on_pointer_move(self, handler: |Point| void): Self    // over it, or anywhere while it holds the press
	fun on_scroll(self, handler: |Scroll| void): Self
	fun on_key(self, handler: |KeyPress| void): Self          // to the focused element
	fun on_key_release(self, handler: |KeyPress| void): Self
	fun on_text(self, handler: |str| void): Self
	fun on_focus(self, handler: || void): Self                // does not bubble
	fun on_blur(self, handler: || void): Self                 // does not bubble
	fun on_resize(self, handler: |Size| void): Self           // laid out at a new size
	fun on_window_focus(self, handler: |bool| void): Self     // its window gained or lost the focus
}
fun stop_event()                                              // inside a handler
fun click_count(): usize                                      // presses in a row: 2 a double click
```

Events are attributes like any other (`<row on_click(|| count.set(0))>`). A
pointer event goes to the element under the pointer — hit testing walks the
last layout back to front, a later sibling above an earlier one and a child
above its parent, with rounded corners and clipping elements honoured — and a
key or text event to the element holding the focus (the root, when nothing
does). From there it BUBBLES: each ancestor that listens hears it, inner first.
`stop_event()` inside a handler ends that, and also keeps the framework's own
response from happening: Tab moving the focus, a `scroll` moving. A click goes
to the innermost element holding both where the press and the release landed.
Press, release and click are the primary button's; positions are dp from the
window's top-left corner. Each handler runs as one turn, as a web event does: its
signal writes settle as one wave.

A press CAPTURES the pointer. Until the button comes up, every pointer move and
the release go to the element the press landed on — and bubble from there —
wherever the pointer goes: off the element, and off the window (the window
systems keep sending a held button's moves and its release to the window it went
down in). So a drag keeps going: a text selection, a slider, a scrollbar's thumb.
During the drag `pressed()` holds on that element and its ancestors wherever the
pointer is, while `hovered()` stays where the pointer is, capture or not — so a
look that means "pressed and still over it" asks for both — and `on_pointer_enter`
and `on_pointer_leave` follow the pointer too. The wheel and the keys are not
captured. The click is decided by where the release LANDS: back over the pressed
element it clicks it, elsewhere it clicks the innermost element holding both, and
outside the window it clicks nothing. A capture ends with the release; with the
element (or an ancestor) being removed, which cancels the press: no click, and the
rest of the drag goes to whatever is under the pointer; and with
`release_pointer()`, which does the same on the program's word — a control
disabled in the middle of a drag calls it.

`click_count()` says how many presses in a row the last press was: 1, 2 for a
double click, 3 for a triple click. A press counts with the one before it when it
lands on the same element within 4 dp and half a second of it. A press the program
delivers itself (`deliver`) carries no time, so presses delivered at one place
count up.

When content moves under a still pointer — a scroll the program moved, a reveal, a
row removed or inserted — the hover follows at the next layout, with its enter
and leave handlers, before the frame is painted.

`on_resize` is not an input: it is told after a layout that gave the element a
new size (its box, in dp) — the first layout, and every later one that changes
it, a move alone not counted. It runs before the frame is painted, and what it
changes is laid out again in the same frame, so a layout that depends on an
element's size never shows a frame of the old one. The virtual list reads its
viewport's height this way.

## Focus

An element is focusable when it says so: `focusable()` puts it in the tab order
in tree order, and `tab_index(n)` at a place of its own — every positive index
before the elements in tree order, lowest first; a negative one is focusable by a
press or by `focus()` but never reached by Tab. Tab moves the focus to the next
element of the tab order and Shift+Tab to the previous one, wrapping at the ends;
a press focuses the innermost focusable element at or above where it landed, and
takes the focus away when there is none. Tab also brings the element it focuses
into view: every `scroll` around it moves as little as it takes to show it —
not at all when it shows already — and `scroll_into_view()` does the same for any
element. Key and text events go to the focused
element first. The `focused()` condition, `on_focus` and `on_blur` follow it.

That is the focus INSIDE a window. The window itself has the system's keyboard
focus or not — another window may have it — and an element can ask:
`window_focused()` answers whether the window its tree is in has it, and
`on_window_focus(|focused| ..)` is told whenever that changes, on every element
that listens, in tree order, whatever holds the focus inside. Neither bubbles
nor can be stopped. A tree in no window is in no focused window, until a
`std::gfx` `Event::FocusChanged(true)` is `deliver`ed to it. A blinking caret
stops in an unfocused window this way, so an idle window in the background
wakes for nothing.

## Timers

```vilan,fragment
impl Element {
	fun after(self, delay: Duration, handler: || void): ElementTimer    // once, `delay` from now
	fun every(self, period: Duration, handler: || void): ElementTimer   // every `period`, from `period` on
	fun window_focused(self): bool
}
struct ElementTimer;
impl ElementTimer {
	fun cancel(self)                                                     // never fires again
}
```

A timer belongs to an element, as its handlers do. Its handler runs as one turn,
the way an event handler does, and it is gone for good when it is cancelled or
when its element is removed for good (a keyed list's row going, a conditional
turning off); after either it never fires. Times are `std::time`'s `Duration`.
A timer fires at its time or later, never before: timers due together fire in
the order of their times, and two due at the same time in the order they were
set. A timer set by a timer's handler waits for the next round, even with no
delay. `every` keeps its phase — each call is due a whole number of periods
after the first, however late the one before ran — and a program that falls
more than a period behind skips the calls it missed rather than catching up in a
burst. The period is at least a millisecond.

However many timers are set, the program waits for the earliest alone, in the
same event loop as everything else; with no timer set, an idle window wakes for
nothing. A pending timer keeps the program running, as a `std::time::Timer` does,
until it fires its last time, is cancelled, or ends with its element. A caret
that blinks while its window has the focus:

```vilan,native
import std::gfx::{ Window, dp };
import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::shared::Shared;
import std::time::Duration;
import std::ui::{ ElementTimer, mount_root, row, text };

fun main() {
	match Window::builder().title("Blink").size(dp(240), dp(120)).open() {
		Ok(let window) => {
			let shown: SignalCell<bool> = Signal::new(true);
			let blink: Shared<Option<ElementTimer>> = Shared::new(None);
			let _owner = mount_root(window, || {
				let caret = <row padding(dp(24))>
					<text>{shown.derive(|on: bool| if on { "|" } else { " " })}</text>
				</row>;
				let element = caret.element();
				caret.on_window_focus(|focused: bool| {
					if blink.read() is Some(let timer) {
						timer.cancel();
					}
					shown.set(true);
					blink.write() = if focused {
						Some(element.every(Duration::millis(530), || shown.set(!shown.get())))
					} else {
						None
					};
				})
			});
		},
		Err(let reason) => print(i"no window: {reason}"),
	}
}
main();
```

## Scrolling

```vilan,fragment
impl ScrollElement {
	fun axis(self, axis: ScrollAxis): ScrollElement
	fun offset(self, cell: SignalCell<Point>): ScrollElement   // bound both ways
	fun scrolled(self): Point                                  // the offset now
	fun range(self): Size                                      // the largest offset
}
enum ScrollAxis { Vertical, Horizontal, Both }
```

A `scroll` is a viewport over its one child. The child keeps its own size along
the axis — a column of a thousand rows is as tall as its rows — and the scroll
asks for that size but takes what its parent gives it: its content never holds
it open, so in a column it shrinks to the room left (give it a size, or `grow`
it into the room, to fill it). The wheel moves the child, clamped to how far it reaches past the
viewport; a wheel turned over a scroll that is already at its end bubbles on to
the next one out. The child is clipped to the scroll's padding box, and hit where
it is drawn. `offset(cell)` binds the offset to a signal both ways: a write
scrolls it, and the wheel writes the cell.

A scroll MOVING lays nothing out and repaints nothing inside it: only the
scroll's own scene changes, and when the frame is drawn the rows outside the
viewport are skipped whole. Every row is still a real element, laid out once —
laying out only the rows in view is `std::ui::kit`'s
[virtual list](#a-virtual-list), not this. Tab and `scroll_into_view` move a scroll to show an element
([Focus](#focus)).

## Lists and conditional children

```vilan,fragment
fun each<T: PartialEq, K: PartialEq + Hashable, S: RowFeed<T>, C: Child>(source: S, key: |T| K, render: |T| C): Each<T, K, S, C>
fun each_values<T: PartialEq + Hashable, S: RowFeed<T>, C: Child>(source: S, render: |T| C): EachValues<T, S, C>
fun each_by<T, K: PartialEq + Hashable, S: RowFeed<T>, C: Child>(source: S, key: |T| K, render: |SignalCell<T>| C): EachBy<T, K, S, C>
fun when<S: Flow<bool>, C: Child>(condition: S, body: || C): When<S, C>
fun swap<T: PartialEq, S: Flow<T>, C: Child>(source: S, render: |T| C): Swap<T, S, C>
```

A child position can be REACTIVE, written as the web UI writes it, in a hole or
handed to `child`: `each` is a KEYED list over a list source (a `ListCell`, a
`SignalCell<List<T>>`, a collection pipe) — a row per item, built by `render`; `when`
is content that is there while its condition holds; `swap` is the subtree for a
value, built again when the value changes (one of two: `swap(flag, |on| if on {
a() } else { b() })`). Each keeps the place where it is written among its static
siblings, however the parent grows afterwards.

```vilan,native
import std::gfx::{ Size, dp };
import std::io::print;
import std::reactive::{ FlushPolicy, ListCell, Signal, SignalCell, turn };
import std::ui::{ column, each, layout, row, text, when };

fun main() {
	let names: ListCell<str> = ListCell::of(["Ada", "Grace"]);
	let busy: SignalCell<bool> = Signal::new(false);
	let screen = <column gap(dp(4))>
		<text>"Team"</text>
		{each(names, |name: str| name, |name: str| <row label(name)>{name}</row>)}
		{when(busy, || <text label("busy")>"Saving…"</text>)}
		<text>"end"</text>
	</column>;
	turn(FlushPolicy::AtSuspension, || {
		names.insert_at(0, "Linus");
		busy.set(true);
	});
	layout(screen, Size::new(dp(320), dp(200)), 1.0);
	for entry in screen.element().outline() {
		if entry.label != "" {
			print(entry.label);   // Linus, Ada, Grace, busy: in place, between the two texts
		}
	}
}
main();
```

A ROW IS ONE ELEMENT — whatever `render` answers, anything `Child` — and each row is
built under an OWNER of its own. A change keeps every row whose key stays (and,
for `each`, whose item is equal), MOVING it when the order changed; it builds a
row for each new key or changed item, and it REMOVES each row whose key left: the
row's owner is disposed, so its bindings end, and the element leaves the tree
with its handlers dropped — a handler that holds its own row does not keep it
alive. Over a `ListCell` the list follows its operations: a push builds one row,
a removal removes one, a `set_at` rebuilds that one row, a `move_range` moves the
rows that moved. `each_values` keys a row by its item. `each_by` hands each row a
CELL holding its item instead of the item: a row whose key stays is never built
again, a changed item is written into its cell, and whatever the row binds to
that cell follows — its items need no equality.

What the tree pays: a row arriving is one insert, a row leaving one removal, a
row moving one move — each tells the layout solver that the PARENT changed and
nothing else, so it places the parent's children again from its cache and
measures only a new row's texts; the rows that stay where they are in the new
order are not touched at all. Hover, press and focus belong to the element, so a
moved row keeps them; a removed one takes them with it (a press inside it is
cancelled, and no click follows its release). Every row below an insert or a
removal has a new box, but its scene is kept: each element's scene is drawn in
its own coordinates and placed by its parent's, so only the parent's is built
again (and a row whose background, border or shadow is now a device pixel
taller or shorter where it sits).

The region — its rows, its place, its subscription — belongs to the ambient owner
when there is one (`mount_root`, `run_with_owner`) and is taken out of the tree
when that owner is disposed; with none, it lives as long as its source, as a
bound attribute does.

## A virtual list

```vilan,fragment
// std::ui::kit
fun virtual_list<T: PartialEq, K: PartialEq + Hashable, S: RowFeed<T>, C: Child, E: RowExtent>(source: S, extent: E, key: |T| K, render: |T| C): VirtualList<T, K>
fun virtual_list_by<T, K: PartialEq + Hashable, S: RowFeed<T>, C: Child, E: RowExtent>(source: S, extent: E, key: |T| K, render: |SignalCell<T>| C): VirtualList<T, K>
fun estimated(extent: Dp): Estimated                             // rows of differing heights
trait RowExtent                                                  // Dp (fixed) and Estimated
impl VirtualList<T, K> {
	fun overscan(self, rows: usize): VirtualList<T, K>               // 2 until set
	fun offset(self, cell: SignalCell<Point>): VirtualList<T, K>     // bound both ways
	fun reveal(self, index: usize)                                   // scroll to a row
	fun reveal_key(self, key: K): bool
	fun count(self): usize
	fun built(self): (usize, usize)                                  // the rows in the tree
	fun scrolled(self): Dp                                           // from the whole list's top
	fun range(self): Dp                                              // the largest offset
	fun row_top(self, index: usize): Dp                              // where a row starts
	fun row_at(self, offset: Dp): Option<usize>                      // the row at an offset
	fun viewport(self): ScrollElement                                // the scroll underneath
}
impl VirtualList with Attributes, Child
```

A list of a hundred thousand rows as `each` would build it is a hundred thousand
elements, laid out and kept. `std::ui::kit::virtual_list` is a keyed list on a
`scroll` that builds, lays out and paints only the rows its viewport shows — and
`overscan` more on each side, two until set — however long the list is. Its
source, key and render are `each`'s, and so is what a change keeps; `virtual_list_by`
is `each_by`'s twin, handing each row a cell. With a `Dp` extent every row is
that tall: the list sets that height on the element `render` answers, and the
arithmetic of which rows the offset shows needs no layout. With
`estimated(dp(..))` each row takes its own height — see
[rows of differing heights](#rows-of-differing-heights). The scroll's range is still the whole
list's (the rows above the window are padding), so a scrollbar drawn from it is
right; the list takes every attribute an element takes, and grows or sizes like
any `scroll`.

```vilan,native
import std::gfx::{ Size, dp };
import std::io::print;
import std::range::Range;
import std::reactive::ListCell;
import std::ui::kit::virtual_list;
import std::ui::{ column, layout, row, text };

fun main() {
	let numbers: ListCell<i32> = ListCell::of(Range::new(0, 100000).to_list());
	let list = virtual_list(numbers, dp(32), |number: i32| number, |number: i32| {
		<row padding_x(dp(12))><text>{i"Row {number}"}</text></row>
	})
		.grow(1.0);
	let screen = <column>{list}</column>;
	layout(screen, Size::new(dp(480), dp(640)), 1.0);
	let (first, last) = list.built();
	print(i"{list.count()} rows, {last - first} built");   // 100000 rows, 22 built
	list.reveal(50000);                                     // the window moves there
	layout(screen, Size::new(dp(480), dp(640)), 1.0);
}
main();
```

WHAT MOVES THE WINDOW: the offset (the wheel, Tab, `reveal`, a write to the cell
`offset` binds), the viewport's height (`on_resize`: laid out again in the same
frame, so the first frame already shows rows), and the source — a `ListCell` is
followed by its operations, a plain `SignalCell<List<T>>` by its whole list per
change. A change outside the window changes the column's height and builds
nothing. A row that STAYS in the window is the same element through every change
— a scroll, an insert above it, another row's change — so its hover, press and
focus stay with it. A row that LEAVES is removed as a keyed row is: its owner is
disposed and its handlers are dropped — and a focused row the wheel scrolls out
takes the focus with it (nothing is focused, and keys go to the root), because
keeping it built would put one row outside the window's single run of rows.
**State a row must keep after it leaves
the window belongs in the item, or in the app's model, not in the row**: a text
typed into it, whether it is expanded. The row is built again from its item when
it comes back.

`reveal(index)` scrolls as little as it takes to show a row, from the extent — so
a row pushed in the same turn can be revealed — and the row is built by the end of
the turn; `reveal_key` finds the row by its key first. TAB never leaves a list
that has more rows: Tab inside it builds one row more after the window before the
focus moves (Shift+Tab one before), and Tab's reveal then scrolls to the row it
focused, which moves the window on. At the list's real ends Tab goes on through
the tab order as it does anywhere.

The list's own padding (`padding(..)` on the list) is read with its height after
each layout that resizes it: the rows built are the ones its padding box shows
(the scroll clips there, so a row in the padding shows too), and `reveal` brings
a row inside the content box. A padding changed while the list keeps its size is
read at the next resize.

What it costs: a scroll step builds the rows that arrive and removes the ones that
leave; the rows that stay do not move, so the solver places the column's few rows
again and measures only the newcomers. A change to the source is a pass over the
window's items, whatever the list's length, plus the list's own copy of the
items (an insert at the top moves that copy along, as a `List` does).

### Rows of differing heights

`virtual_list(source, estimated(dp(48)), key, render)` lays every row out at its
own height — a wrapped text, a row that grows when it is expanded — and guesses
the rows it has not built: at the estimate until the first rows are measured, then
at the mean of the rows measured so far. A built row is measured each time it lays
out at a new size (its height as the layout solver gave it), and the list keeps a
height per measured row and the guess elsewhere in a structure that answers where
a row starts and which row an offset falls in, each in O(log n) (two Fenwick
trees; an insert or a removal rebuilds them from that row on, as the list's own
copy of the items moves). The scroll's range is their sum, so it is exact once
every row has been seen and a guess before.

```vilan,native
import std::gfx::{ Size, dp };
import std::range::Range;
import std::reactive::ListCell;
import std::ui::kit::{ estimated, virtual_list };
import std::ui::{ column, layout, text };

fun main() {
	let notes: ListCell<str> = ListCell::of(Range::new(0, 1000).to_list().map(|number: i32| {
		if number % 3 == 0 { i"Note {number}, which runs on for long enough to wrap" } else { i"Note {number}" }
	}));
	let list = virtual_list(notes, estimated(dp(40)), |note: str| note, |note: str| {
		<text padding(dp(8))>{note}</text>
	})
		.grow(1.0);
	let screen = <column>{list}</column>;
	layout(screen, Size::new(dp(240), dp(400)), 1.0);
	list.reveal(500);                                       // by the guess, then measured
	layout(screen, Size::new(dp(240), dp(400)), 1.0);
}
main();
```

A measured height that differs from the guess, or a change of the source above
the viewport, would move what the viewport shows. The list ANCHORS instead: in
the same layout it moves its offset so that what you are looking at stays put.

- **The anchor** is the first row whose top edge is inside the viewport — or, when
  no row starts inside it, the row across its top edge. After the change, that
  row's top edge is where it was on the screen: a row above it that grows, an
  insert or a removal above it, moves the offset by as much, and the rows from the
  anchor down do not move. A row below the anchor, or the anchor itself, growing
  pushes the rows under it down.
- **When the change removes the anchor**, the row that followed it (at the end of
  the list, the one before) keeps its own place on the screen.
- **A row the source moves** carries the anchor with it; **a whole new list** (a
  plain `SignalCell<List<T>>` changes as one) finds the anchor again by its key,
  and keeps the heights measured for every key it still has.
- **At the very top** (offset 0) there is no anchor: the list stays at the top, so
  a row inserted there shows. **At the very bottom** (scrolled to the end, the list
  taller than the viewport) the bottom edge is the anchor: the list stays at its
  end, as a log or a chat does.
- **`reveal(index)`** of a row never measured scrolls by the guess, then puts the
  row where `reveal` meant it once the rows around it are measured: its top edge
  at the viewport's top when it came from above, its bottom edge at the
  viewport's bottom when it came from below (moving as little as it takes, as
  always). The next change of another kind — the wheel, an operation of the
  source, a row measured again — ends that.

Each measurement is laid out again in the same frame (`on_resize`), so a frame
never shows a row where the guess put it; a window of rows that all differ from
the guess settles in a few passes of the host's eight (`FrameTimes::passes`: three
for rows of 30 to 150 dp guessed at 400). A row of an estimated list is never
shrunk to fit (`shrink(0)` is set on it).

A fixed extent keeps its rules: no measuring, and an insert above the window keeps
the offset (the rows move down one place).

### Positions past four million dp

The layout runs in `f32`, which past about four million dp cannot place a box to
the pixel. A list taller than 4,194,304 dp therefore lays its column out relative
to a BASE the list keeps in `f64` near its window — the rows above the base are
not in the column at all — so the boxes it holds stay under a million dp from the
column's top whatever the list's length: ten million rows of 32 dp scroll, click
and meet exactly. Past that size the `scroll` underneath (`viewport()`) speaks the
column's numbers, and so does its accessibility node's scroll position; read the
list's own `scrolled()`, `range()`, `row_top` and `row_at`, which are the whole
list's, and bind `offset(cell)`, which is too.

## Showing it

```vilan,fragment
fun mount<C: Child>(window: Window, root: C)                     // the window shows it
fun mount_root<C: Child>(window: Window, body: || C): Owner      // built under an owner, then mounted
fun draw<C: Child>(frame: Frame, root: C)                        // in a frame of your own
fun layout<C: Child>(root: C, size: Size, scale_factor: f64)     // boxes only
fun deliver<C: Child>(root: C, event: Event)                     // an event, with no window (a focus change too)
fun last_frame(window: Window): FrameTimes
struct FrameTimes { layout: f64, paint: f64, draw: f64, solved: bool, measured: i32, moved: i32, rebuilt: i32, passes: i32 }
```

`mount` makes the tree the window's contents: its frames lay the tree out at the
window's size and scale and draw it; its events go to the tree's elements before
the window's own event callback, if it has one, hears them; and a change to the
tree asks for a frame. The window also draws when it opens, resizes or changes
scale, so the layout follows the window — and nothing is drawn while nothing
changes. `mount_root` builds the tree under a new owner first, as one turn, so
its bindings die with that owner when it is disposed. A counter:

```vilan,native
import std::gfx::{ Window, dp };
import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ Style, column, hovered, mount_root, row, style, text };

fun main() {
	match Window::builder().title("Counter").size(dp(320), dp(160)).open() {
		Ok(let window) => {
			let count: SignalCell<i32> = Signal::new(0);
			let look = style().when(hovered(), |look: Style| look.text_size(dp(16)));
			let _owner = mount_root(window, || <column padding(dp(24)) gap(dp(8))>
				<text>{count.derive(|value: i32| i"Count: {value}")}</text>
				<row style(look) focusable on_click(|| count.set_with(|value: i32| value + 1))>"Add one"</row>
			</column>);
		},
		Err(let reason) => print(i"no window: {reason}"),
	}
}
main();
```

`draw` lays a tree out at a frame's size and scale and draws it into that frame,
for a frame callback that draws more around it, or for an
[`Offscreen`](gfx.md#offscreen) picture: a golden image of a screen is
`Offscreen::new(..).software(true).render(|frame: Frame| draw(frame, root))`.
`layout` only lays out, for reading boxes back with `bounds` and `outline`, and
`deliver` routes a `std::gfx` `Event` into a laid-out tree as a window would —
hit testing, conditions, focus, handlers, scrolling — for a test, or a program
that drives its tree itself. `last_frame` answers what the last layout and paint
took (in microseconds) and did: whether the solver ran, the texts and images it
measured, the boxes that moved, the element scenes rebuilt.

Every box is SNAPPED to the device pixel grid by the renderer's own rule, each
edge on its own: boxes that share an edge in dp share it on the screen, and a
1 dp line is one device pixel at 100%, 125% and 150%. `bounds` answers the
snapped box in dp (an element inside a `scroll` is where it is laid out, before
the scroll's offset).

## Components: `std::ui::kit`

```vilan,fragment
fun button<C: ButtonContent>(content: C, on_click: || void): Button   // a str (its label too) or an element
fun checkbox(checked: SignalCell<bool>): Checkbox                    // toggles the cell
fun text_input(value: SignalCell<str>): TextInput                    // edits the cell
fun text_area(value: SignalCell<str>): TextArea                       // edits the cell, many lines

impl Button   { fun disabled(self, disabled: Bindable<bool>): Button  fun is_disabled(self): bool }
impl Checkbox { fun disabled(self, disabled: Bindable<bool>): Checkbox  fun is_disabled(self): bool }
impl TextInput {
	fun placeholder(self, text: Bindable<str>): TextInput
	fun placeholder_style(self, look: Bindable<Style>): TextInput
	fun caret(self, color: Color, width: Dp): TextInput
	fun selection_color(self, color: Color): TextInput
	fun on_submit(self, handler: |str| void): TextInput           // Enter, with the value
	fun disabled(self, disabled: Bindable<bool>): TextInput
	fun selection(self): Selection                                 // { anchor: usize, caret: usize }
	fun select(self, anchor: usize, caret: usize)
}
impl TextArea {
	// placeholder, placeholder_style, caret, selection_color, disabled,
	// is_disabled, selection and select: as TextInput's
	fun on_submit(self, handler: |str| void): TextArea            // Ctrl+Enter, with the value
	fun scrolled(self): Point                                      // how far its text is scrolled up
}
```

The kit is behaviour with no look. A component is built from the elements above
— a button and a checkbox are rows, a text input a row around a horizontal
`scroll` — and has every attribute an element has, so the app dresses it as it
dresses a row: `style`, `padding`, `gap`, `width`, and the conditions
`hovered()`, `pressed()` and `focused()`. The kit sets no colour, no radius and no
spacing of its own. Each carries its accessibility role (`Role::Button`,
`Role::Checkbox`, `Role::TextField`); a button made from a `str` is labelled with
it, and the app labels the rest with `label(..)`.

A **button** runs `on_click` on a click, and on Enter or Space while it has the
focus (a held key runs it once); Enter and Space go no further up the tree. A
**checkbox** writes the opposite of its cell the same three ways, and is an empty
row: the app draws the box and the mark as its children, from the same cell. A
component is **disabled** while the flag it is handed holds — a `bool` or a
signal of one: it does nothing, and Tab and a press pass it by. The app styles
that state from the same signal, `style(off.derive(|off: bool| if off { dim }
else { look }))`, where a disabled look with no `hovered()` in it shows no hover.

A **text input** edits its cell: every edit writes it, and a write from anywhere
else is what it shows. A press places the caret and a drag selects — off the
input and out of the window too, the press having captured the pointer: past the
field's left or right edge each pointer move extends the selection one grapheme
beyond what shows, and the field scrolls by it. A double click selects the word
under the pointer (a run of letters and digits, or of the other graphemes: a
double click on a space selects the spaces) and a drag from it extends by whole
words; a triple click selects everything. Left and
Right move a grapheme (with Ctrl or Alt, a word), Home and End to the ends, each
extending the selection with Shift; Backspace and Delete remove the selection,
or a grapheme (with Ctrl or Alt, a word); Ctrl+A (or Cmd+A) selects everything;
typed text replaces the selection; Enter runs `on_submit`; Ctrl+Z (Cmd+Z) undoes
and Ctrl+Shift+Z or Ctrl+Y redoes. Indices are `str`'s
own (UTF-16 units, as `len` and `substring` count) and always on a GRAPHEME
boundary: an emoji with its skin tone, a flag or a letter with a combining accent
is one step and one Backspace. A text wider than the input scrolls sideways to
keep the caret in view. The caret and the selection show while the input has the
focus, drawn by the text itself in its own layout; their colours are the app's
(`caret`, `selection_color`), and otherwise follow the text colour. The keys the
input acts on, and the keys that type, go no further up the tree, so a shortcut
on an ancestor does not fire while the user types.

```vilan,native
import std::gfx::{ Border, Color, Window, dp };
import std::io::print;
import std::reactive::{ ListCell, Signal, SignalCell };
import std::ui::kit::{ button, checkbox, text_input };
import std::ui::{ Style, column, each, focused, hovered, mount, row, style, text, when };

fun main() {
	let window = match Window::builder().title("Tasks").size(dp(360), dp(320)).open() {
		Ok(let window) => window,
		Err(let reason) => {
			print(i"no window: {reason}");
			ret;
		},
	};
	let tasks: ListCell<str> = ListCell::of([]);
	let draft: SignalCell<str> = Signal::new("");
	let urgent: SignalCell<bool> = Signal::new(false);
	let field = style()
		.background(Color::white())
		.border(Border::all(dp(1), Color::rgb(180, 180, 190)))
		.when(focused(), |look: Style| look.border(Border::all(dp(2), Color::rgb(46, 104, 220))));
	let add = || {
		if draft.get() != "" {
			tasks.push(if urgent.get() { draft.get() + "!" } else { draft.get() });
			draft.set("");
		}
	};
	mount(window, <column padding(dp(16)) gap(dp(8))>
		<row gap(dp(8))>
			{text_input(draft).placeholder("A task").grow(1.0).padding(dp(6)).style(field).on_submit(|_: str| add())}
			{button("Add", add).padding(dp(6)).style(style().when(hovered(), |look: Style| look.background(Color::rgb(220, 226, 240))))}
		</row>
		{checkbox(urgent).label("urgent").gap(dp(6)).child(text().child(urgent.derive(|on: bool| if on { "[x] urgent" } else { "[ ] urgent" })))}
		{each(tasks, |task: str| task, |task: str| <text>{task}</text>)}
		{when(tasks.derive(|list: List<str>| list.len() == 0), || <text>"Nothing yet"</text>)}
	</column>);
}
main();
```

A **text area** is the multi-line text input: a column (`Role::TextArea`) around a
vertical `scroll` around its text, which wraps at the area's width (a word longer
than a line breaks where it must) and scrolls inside the height the app gives it
— with no height it grows with its text. It does everything the input does, with
these differences. Enter types a line break (`"\n"`) and Ctrl+Enter (Cmd+Enter)
runs `on_submit`. Up and Down move a VISUAL line toward a goal column: the x the
caret had when the run of vertical moves began, kept until the caret moves any
other way, so the caret crossing a short line comes back to its column on the
next long one; Up from the first line goes to the start, Down from the last to
the end. Home and End go to the ends of the visual line, Ctrl+Home and Ctrl+End
(Cmd too) to the text's; Page Up and Page Down move as many lines as the
viewport shows and scroll it by as much; all of them extend the selection with
Shift. Tab moves the focus, as everywhere. A drag past the top or bottom of what
shows extends the selection one line per pointer move and the text scrolls by
it; a triple click selects the PARAGRAPH (from one line break to the next, the
break included) and a drag from it extends by paragraphs. A selection is painted
one box per visual line, and a selected line break as a small extent at its
line's end, so a selected empty line shows.

At a soft wrap — where a long line continues on the next visual line — one index
is both the end of the upper line and the start of the lower one. It is on the
LOWER line: its caret is drawn at the lower line's start, and Home there stays.
The upper line's last caret position is before the space it broke at (or, for a
word broken inside, before its last grapheme), which is where End, a press past
the line's end and a move up or down onto that line put the caret. Nothing ever
answers an index that draws on another line than the one it was asked for.

Undo is the same in both. Each edit is recorded as the text it removed and the
text it inserted, with the selection before it, in a history of 100 changes;
undo puts the text and that selection back, redo makes the change again. Typing
is ONE change until the caret moves some other way, a line break is typed or
another kind of edit comes, and runs of Backspace and of Delete are one change
each; a new edit forgets what was undone. The value belongs to the app too, and a
write from outside (clearing a draft after a submit) ends the history, since the
recorded changes no longer describe the value.

Not in the kit yet: the clipboard (the window layer offers none), an input
method's composition (an edit inserts committed text only; a preedit would be a
run the edited text draws at the caret without the value holding it), and a
blinking caret (the caret is steady, so an idle window draws nothing; a blink
needs a timer in the host that stops while the window is unfocused). A drag held
still past the field's edge does not keep scrolling: the field moves one
grapheme, or one line, per pointer move, with no timer. Tabs and indentation are
not an editor feature: Tab moves the focus.

## Accessibility

While an assistive tool listens — a screen reader, a magnifier, a test driver —
every mounted window publishes its tree to the system's accessibility API
(AT-SPI on Linux, UI Automation on Windows, NSAccessibility on macOS, through
`accesskit`), and the tool's actions come back into the program. When none
listens nothing is built and nothing wakes: an idle window stays idle.

```vilan,fragment
enum Role { Group, Heading, Paragraph, Button, Link, TextField, Checkbox, List, ListItem, Image, TextArea }
```

What is published, element by element:

| Element | Published as |
|---|---|
| any element with a `role` | that role |
| a text | a label whose text the tool reads |
| a labelled image | an image named by its label; an unlabelled one is decorative and left out |
| a labelled icon | an image named by its label; an unlabelled one is decorative and left out, inside a list too |
| a `scroll` | a scroll view, with its offset and range |
| a labelled row, column or stack | a group named by its label |
| a row, column or stack with no role and no label | nothing: its children hang from the element above it |
| a child of a `Role::List` element | a list item |
| a marker (`each`, `when`) | nothing |

A **button**, a **checkbox**, a **link** and a **heading** are named by their
`label`, or else by the text inside them, and nothing inside them is published
on its own (a button's text is its name, not a second thing to read). A **text
field** is published with its value, its placeholder and its selection; a
`Role::TextArea` as a multi-line one, its text in a run per visual line and its
selection as a line and a grapheme in it (`selection=line:grapheme..` in
`accessibility()`'s listing). An
element's states go with it: `checked`, `disabled_state`, whether it has the
focus, and for a list holding only some of its items, each item's place in the
whole (`items_shown`). The kit sets all of them: `checkbox` its checked state,
`disabled(..)` the disabled state, `virtual_list` the items it shows.

**What an app must do** is name what shows no text. A button made from a `str`
is named by it; a button made from an element — an icon — is named only by its
`label`:

```vilan,native
import std::gfx::{ Size, dp };
import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::kit::{ button, checkbox };
import std::ui::{ Role, column, layout, stack, text };

fun main() {
	let saved: SignalCell<bool> = Signal::new(false);
	let screen = <column label("settings")>
		<text role(Role::Heading)>"Settings"</text>
		{button(stack().width(dp(16)).height(dp(16)), || saved.set(true)).label("save")}
		{checkbox(saved).label("saved")}
	</column>.element();
	layout(screen, Size::new(dp(320), dp(200)), 1.0);
	print(screen.accessibility());
}
main();
```

prints what a tool is told:

```text
Group "settings"
  Heading "Settings"
  Button "save" [click focus]
  CheckBox "saved" unchecked [click focus]
```

Name a meaningful image too, give a heading `Role::Heading`, and give a control
built from plain rows the role it acts as. `accessibility()` is that listing —
role, name, value, states, the actions offered — for a test of an app's labels.

**Actions** reach the program as the input a person would give, so the program
cannot tell them apart and needs no code for them: *focus* moves the focus as a
press does; *click* (a button's, a checkbox's or a link's default action)
scrolls the element into view and presses and releases the primary button at
the middle of it — unless something covers it there, when nothing happens;
*set value* on a text field focuses it, selects everything (Ctrl+A) and types
the value (or presses Delete, for an empty one); *set selection* presses Home
and then the arrow keys, grapheme by grapheme, with Shift for the selected part;
*scroll into view* is `scroll_into_view()`; *scroll up* and *down* (and left and
right) turn the wheel over the scroll, a page or a line at a time. A disabled
element offers none but scrolling to it.

Bounds are published in dp with the window's scale beside them, so the platform
gets device pixels at any scale. Verified on Linux, against AT-SPI; Windows and
macOS go through the same tree and are not yet verified at run time.

