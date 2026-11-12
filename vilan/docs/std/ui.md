# Elements and layout

`std::ui` builds a window's contents from six element kinds — `row`,
`column`, `stack`, `text`, `image`, `scroll` — lays them out for you, and makes
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
however long its list is. Not here yet: grid layout, text editing beyond one
line, input methods, and publishing roles and labels to the system's
accessibility API.

## Elements

```vilan,fragment
[element] fun row(): Row             // children left to right
[element] fun column(): Column       // children top to bottom
[element] fun stack(): Stack         // children on top of one another
[element] fun text(): Text           // a paragraph
[element] fun image(): ImageElement  // a texture in a box
[element] fun scroll(): ScrollElement  // a clipped viewport over one child

struct Element;            // an element of any kind
impl Element {
	fun bounds(self): Rect                    // its box from the last layout, in dp
	fun outline(self): List<OutlineEntry>     // every element in its tree, in paint order
	fun focus(self)                           // gives it the focus
	fun scroll_into_view(self)                // every scroll around it shows it
	fun release_pointer(self)                 // ends a press held on it, and its capture
	fun interaction(self): Interaction        // hovered, pressed, focused now
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
is the web UI's, and a file that imports none of these six functions keeps the
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

`role` and `label` are carried from this version for assistive technology
(`Role::Heading`, `Button`, `Link`, `List`, `ListItem`, `Image` and the rest),
and published to the system by a later one; `outline` lists the label too.

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
removal has a new box, and its scene is built again.

The region — its rows, its place, its subscription — belongs to the ambient owner
when there is one (`mount_root`, `run_with_owner`) and is taken out of the tree
when that owner is disposed; with none, it lives as long as its source, as a
bound attribute does.

## A virtual list

```vilan,fragment
// std::ui::kit
fun virtual_list<T: PartialEq, K: PartialEq + Hashable, S: RowFeed<T>, C: Child>(source: S, extent: Dp, key: |T| K, render: |T| C): VirtualList<T, K>
fun virtual_list_by<T, K: PartialEq + Hashable, S: RowFeed<T>, C: Child>(source: S, extent: Dp, key: |T| K, render: |SignalCell<T>| C): VirtualList<T, K>
impl VirtualList<T, K> {
	fun overscan(self, rows: usize): VirtualList<T, K>               // 2 until set
	fun offset(self, cell: SignalCell<Point>): VirtualList<T, K>     // bound both ways
	fun reveal(self, index: usize)                                   // scroll to a row
	fun reveal_key(self, key: K): bool
	fun count(self): usize
	fun built(self): (usize, usize)                                  // the rows in the tree
	fun viewport(self): ScrollElement                                // the scroll underneath
}
impl VirtualList with Attributes, Child
```

A list of a hundred thousand rows as `each` would build it is a hundred thousand
elements, laid out and kept. `std::ui::kit::virtual_list` is a keyed list on a
`scroll` that builds, lays out and paints only the rows its viewport shows — and
`overscan` more on each side, two until set — however long the list is. Its
source, key and render are `each`'s, and so is what a change keeps; `virtual_list_by`
is `each_by`'s twin, handing each row a cell. Every row is `extent` tall: the
list sets that height on the element `render` answers, and the arithmetic of
which rows the offset shows needs no layout. The scroll's range is still the whole
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

The extent is fixed; a list whose rows differ in height (an estimate corrected by
measuring the rows that are built) is not here yet. Every row's box is laid out in
`f32`: past about four million dp (125,000 rows of 32 dp) its position loses part
of a pixel.

## Showing it

```vilan,fragment
fun mount<C: Child>(window: Window, root: C)                     // the window shows it
fun mount_root<C: Child>(window: Window, body: || C): Owner      // built under an owner, then mounted
fun draw<C: Child>(frame: Frame, root: C)                        // in a frame of your own
fun layout<C: Child>(root: C, size: Size, scale_factor: f64)     // boxes only
fun deliver<C: Child>(root: C, event: Event)                     // an event, with no window
fun last_frame(window: Window): FrameTimes
struct FrameTimes { layout: f64, paint: f64, draw: f64, solved: bool, measured: i32, moved: i32, rebuilt: i32 }
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
typed text replaces the selection; Enter runs `on_submit`. Indices are `str`'s
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

Not in the kit yet: the clipboard (the window layer offers none), an input
method's composition (the part after this one, P5), and a blinking caret (the
caret is steady, so an idle window draws nothing; a blink needs a timer in the
host that stops while the window is unfocused). A drag held still past the
field's edge does not keep scrolling: the field moves one grapheme per pointer
move, with no timer.
