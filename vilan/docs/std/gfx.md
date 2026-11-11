# GPU graphics

`std::gfx` opens a window and draws in it with the GPU: rounded rectangles with
gradients and borders, shadows, text, underlines and textures, under clips,
transforms and opacity layers. Every length is in display pixels (`dp`), frames
are drawn only when something changed, and input arrives as values. It is the
layer a native UI framework stands on (native-ui.md, part P1): a program that
draws with it directly lays out and hit-tests by hand, and one that would rather
not builds its window from [`std::ui`](ui.md)'s elements, laid out for it. The
[native guide](../guide/native.md#a-gpu-drawn-window) walks through a whole
program.

**Native only.** A GPU window is part of a `--backend rust` build. The
JavaScript build refuses a program that reaches this module, naming it; `vilan
check` accepts one, because a check is the same for both backends. It is a
process module, so a browser build that reaches it is refused at the platform
check. What is plain vilan — lengths, geometry, colours, the builders — runs
anywhere; the refusal is by reach, at the first call that needs the GPU or the
window system (opening a window, shaping text, a texture, an offscreen frame).

## Lengths: `Dp`

```vilan,fragment
struct Dp;                         // a length in display pixels
fun dp(value: f64): Dp             // dp(8)
impl Dp {
	fun value(self): f64
	fun min(self, other: Dp): Dp
	fun max(self, other: Dp): Dp
}
// Dp + Dp, Dp - Dp, Dp * f64, Dp / f64, ==, <, <=, >, >=
```

One dp is one logical pixel times the display's scale factor: `dp(8)` is 8
device pixels at 100% and 12 at 150%. A border or hairline under 2 dp is never
thinner than one device pixel and snaps to the pixel grid, so it stays sharp at
125% and 150%. Device pixels never appear in this module: positions, sizes and
events are dp, and the scale factor is a plain ratio. `Dp` is a type rather
than a bare number so a length cannot be mixed with a number that is not one (a
scale, an opacity, a count) without saying so; there is no `Dp * Dp`.

## Geometry, colour, paint

```vilan,fragment
struct Point { x: Dp, y: Dp }                  // Point::new(x, y)
struct Size { width: Dp, height: Dp }           // Size::new(width, height)
struct Rect { x: Dp, y: Dp, width: Dp, height: Dp }
impl Rect {
	fun new(x: Dp, y: Dp, width: Dp, height: Dp): Rect
	fun right(self): Dp
	fun bottom(self): Dp
	fun contains(self, point: Point): bool     // left and top edges in, right and bottom out
	fun inset(self, amount: Dp): Rect
	fun offset(self, dx: Dp, dy: Dp): Rect
}
struct Corners { top_left: Dp, top_right: Dp, bottom_right: Dp, bottom_left: Dp }  // Corners::all, Corners::none
struct Edges { top: Dp, right: Dp, bottom: Dp, left: Dp }                         // Edges::all
struct Transform { x: Dp, y: Dp, scale: f64 }  // Transform::translate(x, y), Transform::scale(s)

struct Color { red: f64, green: f64, blue: f64, alpha: f64 }   // sRGB, straight alpha, 0 to 1
impl Color {
	fun rgb(red: i32, green: i32, blue: i32): Color            // 0 to 255
	fun rgba(red: i32, green: i32, blue: i32, alpha: f64): Color
	fun white(): Color
	fun black(): Color
	fun transparent(): Color
	fun faded(self, factor: f64): Color
}
struct LinearGradient { start: Point, end: Point, start_color: Color, end_color: Color }
enum Fill { Solid(Color), Linear(LinearGradient) }
struct Border { widths: Edges, color: Color }  // Border::all(width, color), Border::none()
```

A gradient is interpolated in sRGB, as CSS's `linear-gradient` is, between two
points in the frame's coordinates. A transform is a translation and a UNIFORM
scale: a rounded rectangle stays a rounded rectangle and text is drawn at the
scaled size. Rotation is a later part.

## Primitives and frames

```vilan,fragment
struct Rectangle { bounds: Rect, corners: Corners, fill: Fill, border: Border }
impl Rectangle {
	fun new(bounds: Rect, color: Color): Rectangle   // square, borderless, solid
	fun radius(own self, radius: Dp): Rectangle
	fun corners(own self, corners: Corners): Rectangle
	fun fill(own self, fill: Fill): Rectangle
	fun gradient(own self, gradient: LinearGradient): Rectangle
	fun border(own self, border: Border): Rectangle   // drawn inside the bounds
}
struct Shadow { bounds: Rect, radius: Dp, blur: Dp, color: Color }
struct Underline { origin: Point, width: Dp, thickness: Dp, color: Color }

struct Frame;   // the frame being drawn, handed to a frame callback
impl Frame {
	fun size(self): Size
	fun scale_factor(self): f64
	fun rectangle(self, rectangle: Rectangle)
	fun shadow(self, shadow: Shadow)
	fun text(self, layout: TextLayout, origin: Point, color: Color)
	fun underline(self, underline: Underline)
	fun texture(self, texture: Texture, bounds: Rect, corners: Corners)
	fun push_clip(self, bounds: Rect, corners: Corners)
	fun push_transform(self, transform: Transform)
	fun push_layer(self, opacity: f64)
	fun pop(self)
}
```

Primitives are drawn in the order they are given. A later primitive that
overlaps an earlier one is drawn over it; primitives that overlap nothing are
batched together however the frame interleaves them. A shadow's offset and
spread are yours: move or grow its bounds. A layer's opacity is applied per
primitive, so two overlapping primitives inside one translucent layer show
through each other.

A frame is filled ONLY while its callback runs. Drawing into a `Frame` kept
after its callback returned fails the program, and so does a frame whose
`push_` calls are not all closed by a `pop` before the callback returns, with
the reason. A frame callback must not wait on anything: a frame is drawn now or
not at all.

## Text

```vilan,fragment
enum FontFamily { SansSerif, Serif, Monospace, Named(str) }
struct TextStyle;   // 14 dp regular sans-serif, line height 1.25
impl TextStyle {
	fun new(): TextStyle
	fun family(own self, family: FontFamily): TextStyle
	fun size(own self, size: Dp): TextStyle
	fun line_height(own self, line_height: f64): TextStyle   // a multiple of the size
	fun weight(own self, weight: i32): TextStyle             // 100 to 900; 400 regular, 700 bold
	fun italic(own self, italic: bool): TextStyle
}
struct TextLayout;  // a shaped paragraph: a host value
impl TextLayout {
	fun new(text: str, style: TextStyle): TextLayout                    // one line
	fun wrapped(text: str, style: TextStyle, max_width: Dp): TextLayout
	fun size(self): Size
	fun first_baseline(self): Dp
	fun line_count(self): usize
}
fun register_font(data: Bytes): Result<List<str>, str>   // the family names registered
```

Shaping is the costly part of text — tens of microseconds a paragraph — so a
`TextLayout` is made once, when its text or style changes, and drawn in as many
frames as show it; it is independent of the display, so a scale change does not
reshape it. Text is shaped by `parley` with the system's fonts and any you
register (Thai, Lao, Khmer and Burmese break at spaces only). A layout is a
host value: a copy is the same layout, and two are `==` only when they are one.

## Textures

```vilan,fragment
struct Texture;    // RGBA pixels the GPU draws: a host value
impl Texture {
	fun from_image(image: Image): Result<Texture, str>     // a `std::image` picture
	fun from_rgba(width: u32, height: u32, pixels: Bytes): Result<Texture, str>
	fun width(self): u32
	fun height(self): u32
}
```

A texture's pixels are copied in once; they are uploaded to the GPU on the
first frame that draws them and freed after the last copy of the texture is
gone. A texture larger than the GPU's largest texture fails the frame that
draws it.

## Windows

```vilan,fragment
struct Window;         // an open window: a handle
struct WindowBuilder;  // what `open` makes
impl Window {
	fun builder(): WindowBuilder                  // untitled, 800 x 600 dp, on white
	fun on_frame(self, draw: |Frame| void)        // and asks for a frame
	fun on_event(self, handle: |Event| void)
	fun request_redraw(self)
	fun close(self)                               // closing the last one stops the program
	fun size(self): Size                          // 0 x 0 once closed
	fun scale_factor(self): f64
}
impl WindowBuilder {
	fun title(own self, title: str): WindowBuilder
	fun size(own self, width: Dp, height: Dp): WindowBuilder
	fun background(own self, color: Color): WindowBuilder   // every frame starts from it
	fun open(self): Result<Window, str>                     // async
}
```

`open` answers once the window exists, and a frame for it follows. It is
`async`, because the window is made by the window system's loop between the
program's turns; an `Err` says why there is none: no window system (a machine
with no display), no GPU the renderer can use, a side under 1 dp.

**Frames are drawn on demand.** A frame is drawn when the window opens, resizes
or changes scale, when the system shows a part of it again, and when the program
asks with `request_redraw` (any number of requests before the frame are one
frame). Nothing draws on a timer: a program that changes what it shows asks, and
an idle window costs no CPU and no GPU time.

## Events

```vilan,fragment
enum Event {
	PointerMoved(Point),
	PointerPressed(Pointer),
	PointerReleased(Pointer),
	PointerLeft,
	Scrolled(Scroll),
	KeyPressed(KeyPress),
	KeyReleased(KeyPress),
	TextInput(str),
	Resized(Size),
	ScaleChanged(f64),
	CloseRequested,
}
struct Pointer { position: Point, button: Button, modifiers: Modifiers }
enum Button { Left, Right, Middle, Back, Forward, Other(i32) }
struct Modifiers { shift: bool, control: bool, alt: bool, meta: bool }
struct Scroll { position: Point, x: Dp, y: Dp, modifiers: Modifiers }
struct KeyPress { key: KeyName, repeat: bool, modifiers: Modifiers }
enum KeyName {
	Char(str), Enter, Tab, Backspace, Delete, Insert, Escape,
	Up, Down, Left, Right, Home, End, PageUp, PageDown, F(i32), Other(str),
}
```

Events arrive in order, through the window's event callback; a window with no
event callback drops them. Pointer motion between two of the program's turns
arrives as one move, to where the pointer is. A key that types a character is
`Char` with the character, shift applied (`" "` for the space bar), and is
followed by a `TextInput` with what it typed; the modifiers stay on the press,
never folded into the key. `KeyName`'s names follow `std::process::Key`'s where
they are the same key; a key with no name here (Shift alone, Caps Lock, a media key) is
`Other` with the web's name for it. A mouse wheel's notches arrive at 32 dp per
line, and positive `x` and `y` mean the content should move right and down.

`Resized` is followed in the same turn by a frame at the new size, so a window
being dragged larger redraws in step with the drag.

## How a GPU program ends

Closing the **last** window — by hand, or with `Window::close` — stops the
program the way SIGTERM stops a native server: servers stop accepting and drain,
the event loop ends, `main` returns, and the process exits with code 0. The
close button's request is delivered as `Event::CloseRequested` first, and the
window closes after that turn. SIGTERM does the same with a window still open. A
failure in a frame or event callback is the program's failure: it ends with
exit code 1 and the failure's message, as a failure in `main` does.

```vilan,native
import std::gfx::{ Color, Event, Frame, Point, Rect, Rectangle, TextLayout, TextStyle, Window, dp };
import std::io::print;

fun main() {
	let window = match Window::builder().title("Hello").size(dp(320), dp(160)).open() {
		Ok(let window) => window,
		Err(let reason) => {
			print(i"no window: {reason}");
			ret;
		},
	};
	let greeting = TextLayout::new("Hello from the GPU", TextStyle::new().size(dp(20)));
	window.on_frame(|frame: Frame| {
		frame.rectangle(Rectangle::new(Rect::new(dp(16), dp(16), dp(288), dp(128)), Color::rgb(45, 95, 230))
			.radius(dp(12)));
		frame.text(greeting, Point::new(dp(32), dp(64)), Color::white());
	});
	window.on_event(|event: Event| {
		match event {
			Event::CloseRequested => print("bye"),
			_ => {},
		}
	});
}
main();
```

## Offscreen

```vilan,fragment
struct Offscreen;
impl Offscreen {
	fun new(width: Dp, height: Dp): Offscreen     // scale 1, on white, any adapter
	fun scale_factor(own self, scale_factor: f64): Offscreen
	fun background(own self, color: Color): Offscreen
	fun software(own self, software: bool): Offscreen   // a software adapter only
	fun render(self, draw: |Frame| void): Result<Image, str>
}
```

`render` draws one frame into a picture instead of a window: for a test that
holds a scene to a golden image, and for drawing with no screen (a thumbnail, a
server that draws). It needs a GPU adapter but no window system, and answers a
`std::image` picture of `round(width * scale_factor)` x `round(height *
scale_factor)` pixels, which `encode` turns into a PNG. It is synchronous: it
waits for the GPU, and the first call also makes the device (a tenth of a second
and more). `software(true)` asks for Mesa's software Vulkan or GL only, whose
output is the same on every machine that has one — what a golden image needs,
together with a font registered from a file rather than the system's.

```vilan,native
import std::gfx::{ Color, Frame, Offscreen, Rect, Rectangle, dp };
import std::image::Format;
import std::io::print;

fun main() {
	let drawn = Offscreen::new(dp(64), dp(64)).scale_factor(2.0).render(|frame: Frame| {
		frame.rectangle(Rectangle::new(Rect::new(dp(8), dp(8), dp(48), dp(48)), Color::rgb(255, 120, 0)).radius(dp(24)));
	});
	match drawn {
		Ok(let image) => match image.encode(Format::Png, 100) {
			Ok(let png) => print(i"{image.width()} x {image.height()}, {png.len()} bytes of PNG"),
			Err(let reason) => print(reason),
		},
		Err(let reason) => print(i"no frame: {reason}"),
	}
}
main();
```

## Not here yet

Paths and icons, more than one window, rotation, text editing beyond typed text,
input methods, and publishing accessibility to the system are later parts of
native-ui.md. The user's text-scale setting is not read yet: text and spacing
are drawn at the display's scale factor alone. A program that reaches both
`std::window` and `std::gfx` is refused by name — each needs the main thread for
its own window system's loop — and so is one whose `main` answers an exit code,
under which no window loop would run.
