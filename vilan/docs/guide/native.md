# Native binaries

Vilan has a second backend. `--backend rust` emits your program as Rust source,
writes it into a cargo project under `dist/native/`, and builds it with the
`cargo` already on your machine:

```sh
vilan build --backend rust hello.vl   # writes dist/native/hello/ and builds it
vilan run   --backend rust hello.vl   # builds, then runs the binary
```

The program is the same program. The point of the backend is that its output is
a binary with no runtime host — no Node, no browser, no garbage collector — and
the way that is kept honest is a differential: every program the backend accepts
must print **byte-identical** output under both backends, and a gate in the
compiler's own suite proves it over the test corpus.

## What it reaches, and what it refuses by name

What the backend reaches today:

- structs, enums, `Option` and `Result`, generics (monomorphised per type
  argument), traits, trait objects (`dyn`), overloaded operators
- `str`, `List`, `HashMap`, `HashSet`, closures, `impl` blocks, `match`, loops,
  recursion, module-level bindings, `lazy`
- `async`, `await`, `Task` and `Nursery`, on a single-threaded executor of the
  runtime's own
- `std::json`, `std::db` (SQLite, through a separate runtime crate that a
  program links only when it reaches `std::db`), `std::bytes`, `std::binary`,
  `std::time`'s clock and `Instant::to_iso`, `str::index_of` and
  `last_index_of` (in UTF-16 code units, as on node), and `std::fs`'s reads,
  writes and directories — `create_dir_all`, `remove_dir_all`, `copy_dir`,
  `scan_dir` and `read_dir_all` with node's results and failures (a listing
  comes back sorted by name, which node does not promise either)
- `std::crypto`: SHA-256, SHA-384 and SHA-512, HMAC-SHA-512, PBKDF2,
  `random_bytes` and `random_uuid` from the operating system's random source —
  and node:crypto's `pbkdf2Sync` when a program binds it itself, with the
  `Buffer` it answers read back through `toString("hex")`. Everything past
  SHA-256 lives in a second runtime crate a program links only when it reaches
  one of them, since OS randomness is the one thing here that needs a
  dependency
- **servers**: `std::http` — `Server::builder()` with `serve_build`,
  `cache_build` and its conditional GET, `on_request`, `on_start` — and
  `std::rpc::server`: a `[service]` mounted with `Service::new` or
  `Service::factory`, the WebSocket upgrade with its handshake gate
  (`authorize`), and `[expose]`/`[expose(keyed)]` mirrors a vilan client
  subscribes to, byte-for-byte on the wire with the node build
- **desktop windows**: `std::window` — a window showing a web page in the
  operating system's webview, which is how a native server becomes a desktop
  app ([below](#a-desktop-window))
- **GPU windows**: `std::gfx` — a window the program draws itself, with the GPU:
  rectangles, shadows, text and textures in display pixels, frames on demand,
  pointer and key input as values ([below](#a-gpu-drawn-window)), and `std::ui`
  on it — rows, columns, stacks, text and images laid out for the program, style
  as values ([below](#elements-and-layout))
- **rpc clients**: `std::rpc`'s WebSocket client over `ws://` — a generated
  client's `connect` (and `connect_with` a credential), `connect_socket`,
  calls, mirrors, the reconnect loop with its backoff, and a dial that honours
  its nursery's cancel (`dial_within`'s deadline included). A native client
  talks to a native server or a node one, and a node client to either

What it does **not** reach yet — each refused by name, with the construct in the
message, rather than silently mis-compiled: the browser platform (`std::web::dom`,
`std::web::ui`, `std::web::prelude`, `std::web::router`), a `resource` type with a `Drop` impl
(deterministic teardown natively is its own slice; a resource with no `Drop`,
such as a pipe node, builds as an ordinary move-only type), a handful of host bindings
(`std::fs`'s file handles and its watcher among them),
`--watch` (a native round is a full `cargo build`; the dev loop's hot-swap
belongs to the JS backend), and **TLS**: a native client dialling `wss://` stops
at the dial with a message saying the native backend has no TLS — the URL is a
value, so this one is refused when the program runs rather than when it builds.
Dial `ws://`, behind a TLS-terminating proxy (caddy, nginx) when the server
needs one, as a native server is deployed.

If your program uses one of these, `--backend rust` tells you which and stops.
Build it with `--backend js`, which is still the default.

**`Bytes` is shared.** A `Bytes` is one buffer behind every holder, exactly as
the `Uint8Array` it is on the JS backend: `let b = a` is an alias, and a
`set`, `fill` or `copy_into` through either is seen through both. `slice` and
`Bytes::concat` are the two that make a new buffer.

**`std::db` binds and reads the same values.** A parameter list is a
`List<dyn SqlValue>` on both backends, so what may be bound — a number, a
`str`, a `bool`, `Bytes` as a BLOB, an `Option` of one — is the checker's
answer, not a run-time refusal on node and a rustc error here. A BLOB reads
back through `Row::bytes` as the bytes that were bound. A typed reader
(`text`, `integer`, …) meets a NULL with the same panic on both backends,
naming the column; the `optional_` readers are the nullable door. One
difference is node's, not the program's: node binds every JS number as a
REAL, so a column with no declared type (no affinity) stores `7` as `7.0`
there and as INTEGER natively. Declare the column `INTEGER` (or `NUMERIC`)
and both read back the same.

**A server answers node's bytes, less node's `Date`.** The native HTTP server
keeps connections alive as node does — HTTP/1.1 persistent connections,
`Connection: close` honoured from either side, HTTP/1.0 answered and closed,
pipelined requests answered in order — and writes node's head field for field:
node's reason phrases, `Connection: keep-alive` with `Keep-Alive: timeout=5`, a
streamed response `Transfer-Encoding: chunked`. The one field it does not write
is node's `Date`. Node's three deadlines hold natively too, with node's
defaults: a request's head within 60 s, the whole request within 300 s (each
answered `408 Request Timeout` when missed), and 5 s idle between requests;
`ServerBuilder`'s `headers_timeout`, `request_timeout` and `keep_alive_timeout`
set them on both backends. HTTP/2 and TLS are not here: a native server is
deployed behind a proxy that terminates TLS.

**A server with no `host` binds dual-stack, as node's does.** Node's
`listen(port)` with no host takes `::` with IPv6-only switched off, so one
socket answers IPv4 and IPv6 clients on every interface, and the native server
binds the same socket the same way — on Windows too, where an IPv6 socket is
IPv6-only unless told otherwise. On a machine without IPv6 both take `0.0.0.0`
instead. `Request::peer_address` therefore spells an IPv4 client of a server
with no host mapped on both backends, `::ffff:127.0.0.1`, and an IPv6 one as
itself. Name the bind with `ServerBuilder::host` to narrow it:
`host("127.0.0.1")` and `host("0.0.0.0")` are IPv4 only and read `127.0.0.1`,
`host("::1")` is IPv6 loopback only and reads `::1`, and `host("::")` is the
same dual-stack socket as no host at all.

**A native server stops gracefully on SIGTERM.** `Server::stop()` works as it
does on node. Beyond it, a program that starts a server answers SIGTERM, SIGINT
and SIGHUP (Ctrl-C, and the console's close on Windows) by stopping its
servers: it stops accepting, closes the connections that are only waiting and
the upgraded sockets, lets a request already being answered finish — and then
the program ends, where node would have died of the signal. It ends whatever
else is still pending: a timer, a detached `async { for { ..; sleep(..) } }`
task, a `main` still suspended (which ends without a failure) — node abandons
all of them when it dies, so a native program that waited for them would be one
SIGTERM no longer stops. A second signal ends the process at once, for a server
whose open response never ends. The handler
lives in a third runtime crate, linked only by a program that serves, so a
native server build fetches the `ctrlc` crate the first time.

**The event loop sleeps until there is work.** A native program that serves,
dials a WebSocket or reads stdin waits in the operating system (`poll` on Linux
and macOS, `WSAPoll` on Windows) until a socket it uses is ready, the next timer
or request deadline is due, or another thread hands it something: an idle
server costs no CPU, and a request is answered as soon as its bytes arrive. The
wait lives in its own small runtime crate, linked only by such a program; the
order in which tasks, timers and socket events run is the same as before, and
the same as node's.

**A native client's socket is the host's, with two differences you can see
only from outside the program.** The address lookup and the TCP connect run on
a helper thread (Rust's standard library has no non-blocking connect), so a
slow DNS answer does not stall the event loop; and no compression extension is
offered on the handshake. Events arrive one per turn of the event loop, as the
host's do, so the program sees the same order of messages, errors and closes.

## A desktop window

A native program can show its own page in a window: serve it from an
in-process `std::http` server bound to loopback, and open
[`std::window`](../std/window.md) on the server's URL. That is one binary, with
no browser and no runtime host, and the page talks to the server over the real
socket exactly as it would in a browser. `vilan/examples/desktop` is the whole
app; its core is this:

```vilan,native
import std::http::{ Request, Response, Server };
import std::io::print;
import std::window::Window;

fun main() {
	Server::builder()
		.port(0)                 // a port the OS knows is free
		.host("127.0.0.1")       // reachable from this machine only
		.on_request(|request| Response::builder()
			.set_header("Content-Type", "text/html; charset=utf-8")
			.body("<h1>Hello from the program's own server</h1>")
			.build())
		.on_start(|server| show(server.url()))
		.build()
		.start();
}

fun show(url: str) {
	match Window::builder(url).title("Hello").size(640, 400).open() {
		Ok(_) => print(i"window open on {url}"),
		Err(let reason) => print(i"no window ({reason}); the app is at {url}"),
	}
}
main();
```

```sh
vilan run --backend rust .
```

**Closing the window ends the program cleanly.** Closing the last window — by
hand or with `Window::close` — is the program's graceful stop, the one SIGTERM
makes: the server stops accepting and drains, `main` returns, and the process
exits with code 0. SIGTERM does the same with the window open.

**Native only, by name.** `vilan build` (the JavaScript backend) refuses a
program that reaches `std::window`, saying so; `vilan check` accepts it, since
a check is the same for both backends.

**What happens underneath.** A window system wants the process's main thread
— macOS refuses any other, and GTK and Windows bind a window to the thread
that made it — and its event loop runs until the program ends. So a program
that reaches `std::window` gives the main thread to that loop, and its own
`main`, with the event loop that runs its server and tasks, runs on a second
thread (with an 8 MiB stack, a Linux main thread's size). `open` asks the main
thread for the window and waits for the answer; an open window keeps the
program's loop alive while doing nothing, and closing it wakes the loop. Both
loops sleep in the operating system until there is work. When there is no
window system at all — no display — the program runs as it would without one
and `open` answers why.

**What it links.** The window is the operating system's webview reached
through two crates of the Tauri project, `wry` (the webview) and `tao` (the
window and its event loop), in a runtime crate of its own that a program links
only when it reaches `std::window`. Building one needs, on Linux, the
WebKitGTK and GTK development packages (`libwebkit2gtk-4.1-dev` and
`libgtk-3-dev` on Debian and Ubuntu), and running it needs WebKitGTK 4.1;
macOS and Windows use the webview the system ships (on Windows, WebView2,
which Windows 10 and 11 carry). The first desktop build fetches and compiles
that stack, a minute or two.

**Which versions.** `wry` and `tao` are pinned exactly, and the generated
project starts from the lockfile the runtime crate was audited and licence-checked
with, so every crate under them builds at that version too. Cargo adds what
your program needs beyond them, and a lockfile you have moved yourself (a
`cargo update` in `dist/native/<entry>/`) is kept. What is not pinned is the
system's own webview: WebKitGTK on Linux is whatever the distribution ships.

**Licences you ship.** A Linux desktop binary links `option-ext` (through
`dirs`), which is MPL-2.0: distributing a binary that links it unmodified asks
that its recipients can find its source and licence. Everything the webview
stack can link, with its licence text, is in
`crates/vilan-rt-webview/THIRD-PARTY-NOTICES.txt` in the vilan repository, the
file to ship beside a desktop binary. The system webview libraries themselves
(WebKitGTK, GTK) are LGPL and dynamically linked from the system.

**On a machine with no screen.** A test that opens a window needs a display;
`xvfb-run` gives it a virtual one. GTK also registers the application on the
D-Bus session bus, so run it inside `dbus-run-session` (a CI machine has no
session bus, and a desktop's own bus can stall a headless start), and
`NO_AT_BRIDGE=1` keeps the accessibility bridge off it:

```sh
NO_AT_BRIDGE=1 dbus-run-session -- xvfb-run -a dist/native/main/target/debug/main
```

## A GPU-drawn window

A native program can also draw its window itself. [`std::gfx`](../std/gfx.md)
opens a window and, whenever a frame is due, hands the program's frame callback
a `Frame` to fill with primitives — rounded rectangles with gradients and
borders, shadows, text, underlines, textures — in display pixels (`dp`), and
hands its event callback the window's input as `Event` values. A program that
draws this way lays out and hit-tests by hand (the next section lays out for
you). `vilan/examples/gfx` is a counter: a card, a label,
and a button that lights up under the pointer, darkens while pressed and adds
one on a click. Its core:

```vilan,native
import std::gfx::{ Color, Event, Frame, Point, Rect, Rectangle, TextLayout, TextStyle, Window, dp };
import std::io::print;
import std::reactive::{ Owner, Signal, SignalCell, run_with_owner };
import std::shared::Shared;

fun main() {
	let window = match Window::builder().title("Counter").size(dp(240), dp(120)).open() {
		Ok(let window) => window,
		Err(let reason) => {
			print(i"no window: {reason}");
			ret;
		},
	};
	let button = Rect::new(dp(60), dp(40), dp(120), dp(40));
	let count: SignalCell<i32> = Signal::new(0);
	let style = TextStyle::new().size(dp(18));
	let label: Shared<TextLayout> = Shared::new(TextLayout::new("0", style));
	let hover: Shared<bool> = Shared::new(false);

	// The count changed: shape its label again, and ask for a frame.
	run_with_owner(Owner::new(), || {
		count.effect_on_change(|value: i32| {
			label.write() = TextLayout::new(i"{value}", style);
			window.request_redraw();
		});
	});
	window.on_frame(|frame: Frame| {
		let fill = if hover.read() { Color::rgb(70, 120, 250) } else { Color::rgb(45, 95, 230) };
		frame.rectangle(Rectangle::new(button, fill).radius(dp(8)));
		frame.text(label.read(), Point::new(button.x + dp(12), button.y + dp(10)), Color::white());
	});
	window.on_event(|event: Event| {
		match event {
			Event::PointerMoved(let position) => {
				let over = button.contains(position);
				if over != hover.read() {
					hover.write() = over;
					window.request_redraw();
				}
			},
			Event::PointerReleased(let release) => {
				if button.contains(release.position) {
					count.set_with(|value: i32| value + 1);
				}
			},
			_ => {},
		}
	});
}
main();
```

```sh
vilan run --backend rust .
```

**Frames on demand.** Nothing draws on a timer. A frame is drawn when the window
opens, resizes or changes scale, when the system shows part of it again, and
when the program asks with `request_redraw` — here, when the hover changes and
when the count does. An idle window costs no CPU and no GPU time. Text is
shaped into a `TextLayout` once, when it changes, and drawn in every frame that
shows it.

**Closing the window ends the program cleanly**, as a desktop window's does:
the close button's request arrives as `Event::CloseRequested`, the window closes
after that turn, and closing the last window is the graceful stop SIGTERM makes
— servers drain, `main` returns, exit code 0. A failure in a callback is the
program's failure, exit code 1.

**Native only, by name.** `vilan build` (the JavaScript backend) refuses a
program that reaches `std::gfx`, saying so; `vilan check` accepts it.

**What happens underneath.** The window system's loop (`winit`) owns the
process's main thread, and the program runs ON THAT THREAD, inside it: the
program's `main` runs, then the window system's loop turns the program's event
loop whenever there is something for it — an input event (queued for the window
and handed to the program inside a turn), a frame due, a timer reached, a socket
ready (a helper thread waits on sockets, only while there are any), a wake from
another thread — and sleeps in the operating system in between. A resize is
handled in step: the new size is recorded, the program draws at it, and the
frame is presented, before the next event. When there is no window system — no
display — the program runs as it would without `std::gfx`, and `open` answers
why. A program that reaches both `std::window` and `std::gfx` is refused by
name: each needs the main thread for its own window system's loop.

**What it links.** The renderer is the runtime crate `vilan-rt-gfx`: `wgpu`
for the GPU (Vulkan, Metal, DirectX 12, or OpenGL where none of those is), a
renderer of its own that draws every primitive as one quad shaped in the shader,
`winit` for the window and its input, and `parley` and `swash` for text. A
program links it only when it reaches `std::gfx`. Nothing is needed to build it
beyond Rust (no development package; fontconfig is opened at run time on
Linux); running it needs a GPU driver, or Mesa's software Vulkan (`lavapipe`,
`mesa-vulkan-drivers` on Debian and Ubuntu). The first build fetches and
compiles the stack, a minute or so.

**Which versions, and the licences you ship.** `wgpu`, `winit`, `parley` and
`swash` are pinned exactly, and the generated project starts from the lockfile
the runtime crate was audited and licence-checked with; a lockfile you have
moved yourself is kept. Every licence in the stack is one the compiler itself
ships under; the list with their texts is
`crates/vilan-rt-gfx/THIRD-PARTY-NOTICES.txt` in the vilan repository, the file
to ship beside a binary that draws.

**On a machine with no screen.** A window needs a display: `xvfb-run -a` gives
it a virtual one, and Mesa's software Vulkan draws without a GPU. To draw with
no display at all — a test, a thumbnail — `Offscreen::new(width,
height).render(draw)` draws one frame into a `std::image` picture.

## Elements and layout

[`std::ui`](../std/ui.md) builds a GPU window's contents from six element kinds
and lays them out: `row` and `column` put their children in a line, `gap`
apart, with `padding`, `grow`, `shrink`, `align`, `justify` and `wrap`; `stack`
puts its children on top of one another, each at its `anchor`; `text` is a
paragraph that wraps to the width it is given; `image` shows a texture by its
`fit`; `scroll` is a clipped viewport over one child. Each kind is written as an
element tag, `<row gap(dp(12))>..</row>`, which lowers to the builder calls
`row().gap(dp(12)).child(..)`. None has a look of its own: a look is a `Style`
value — background, border, radius, shadow, opacity, text style — composed with
`+`, and only its text fields are inherited. There are no margins: space between
siblings is a gap, space inside an element its padding.

```vilan,native
import std::gfx::{ Color, Window, dp };
import std::io::print;
import std::ui::{ Align, Element, column, mount, row, style, text };

fun header(): Element {
	<row
		padding(dp(16))
		gap(dp(12))
		align(Align::Center)
		style(style().background(Color::rgb(33, 36, 46)).text_color(Color::white()))
	>
		<text style(style().text_size(dp(20)).weight(700))>"vilan"</text>
		<row grow(1.0) />
		"Guide"
	</row>.element()
}

fun main() {
	let window = match Window::builder().title("Docs").size(dp(640), dp(400)).open() {
		Ok(let window) => window,
		Err(let reason) => {
			print(i"no window: {reason}");
			ret;
		},
	};
	mount(window, <column>
		{header()}
		<column padding(dp(24)) gap(dp(12))>
			<text style(style().text_size(dp(24)))>"Layout without a style sheet"</text>
			"Rows and columns place their children in a line; a paragraph wraps to the width it is given."
		</column>
	</column>);
}
main();
```

`mount` makes the tree the window's contents: each frame lays it out at the
window's size and scale and draws it, so a resize lays it out again. The tree is
RETAINED — built once, kept, laid out again only where something that moves a box
changed, and repainted only where something changed — and every box is snapped
to the device pixel grid by the renderer's own rule, so a 1 dp line stays one
pixel at 150%. `vilan/examples/ui` is a whole documentation page: a header, a
sidebar, a content column with wrapped paragraphs, and a picture card with a
badge on its corner; its end-to-end test holds the page to recorded rectangles at
two window sizes and two scales, and to golden images.

### Interaction

Every attribute takes a value or a signal, and events are attributes too. A
counter, its look supplied by the app as style conditions:

```vilan,native
import std::gfx::{ Color, Window, dp };
import std::io::print;
import std::reactive::{ Signal, SignalCell };
import std::ui::{ Style, column, focused, hovered, mount, pressed, row, style, text };

fun main() {
	let window = match Window::builder().title("Counter").size(dp(360), dp(240)).open() {
		Ok(let window) => window,
		Err(let reason) => {
			print(i"no window: {reason}");
			ret;
		},
	};
	let count: SignalCell<i32> = Signal::new(0);
	let look = style()
		.background(Color::rgb(45, 95, 230))
		.text_color(Color::white())
		.radius(dp(8))
		.when(hovered(), |look: Style| look.background(Color::rgb(70, 120, 250)))
		.when(pressed(), |look: Style| look.background(Color::rgb(28, 70, 190)))
		.when(focused(), |look: Style| look.text_color(Color::rgb(255, 220, 120)));
	mount(window, <column padding(dp(24)) gap(dp(16))>
		<text>{count.derive(|value: i32| i"Count: {value}")}</text>
		<row padding(dp(12)) style(look) focusable on_click(|| count.set_with(|value: i32| value + 1))>
			"Add one"
		</row>
	</column>);
}
main();
```

The count's text follows its signal: a click writes the signal, the one text
is laid out again, and the window draws one frame. The button's looks follow
the pointer and the focus with no code of the app's: `hovered()`, `pressed()` and
`focused()` are typed conditions the framework resolves from hit testing and the
focus, and a change repaints the one element. Tab and Shift+Tab move the focus;
a `scroll` moves its child under the wheel without laying anything out.

### Lists and conditional children

A child can be REACTIVE, written the way the web UI writes it: `each(items, key,
render)` is a keyed list — a row per item, kept across changes by its key and
moved, not rebuilt, when the order changes — and `when(condition, || ..)` is
content that is there while the condition holds. A change to the list inserts,
removes or moves the elements it must, and nothing else is built:

```vilan,native
import std::gfx::{ KeyName, KeyPress, Window, dp };
import std::io::print;
import std::reactive::{ ListCell, Signal, SignalCell };
import std::shared::Shared;
import std::ui::{ column, each, mount, row, text, when };

fun main() {
	let window = match Window::builder().title("Tasks").size(dp(360), dp(480)).open() {
		Ok(let window) => window,
		Err(let reason) => {
			print(i"no window: {reason}");
			ret;
		},
	};
	let tasks: ListCell<str> = ListCell::of(["Write", "Test"]);
	let added: Shared<i32> = Shared::new(0);
	let empty: SignalCell<bool> = Signal::new(false);
	mount(window, <column padding(dp(16)) gap(dp(8)) on_key(|key: KeyPress| {
		match key.key {
			KeyName::Char(let typed) => {
				if typed == "n" {
					added.write() = added.read() + 1;
					tasks.insert_at(0, i"Task {added.read()}");
				} else if typed == "d" && tasks.size() > 0 {
					tasks.remove_at(0);
				}
				empty.set(tasks.size() == 0);
			},
			_ => {},
		}
	})>
		<text>"n adds a task at the top, d removes the top one"</text>
		{each(tasks, |task: str| task, |task: str| <row padding(dp(8))>{task}</row>)}
		{when(empty, || <text>"Nothing to do"</text>)}
	</column>);
}
main();
```

A row is one element — whatever the render closure answers — and it is built
under an owner of its own: when it leaves, its bindings end and its handlers are
dropped with it. Hover, press and focus belong to the element, so a moved row
keeps them. `each_by` hands each row a cell holding its item instead, so an item
changed in place is written into its row and nothing is built; `swap(value, |v|
..)` shows one of several subtrees.

`vilan/examples/ui-counter` is the counter with two buttons and keyboard
activation, and `vilan/examples/ui-list` a keyed list of 10,000 rows in a
`scroll` — a click changes one row, and keys add a row at the top, remove one and
move one; their end-to-end test drives both through the X server, by pointer and
keyboard, and holds every frame to "only on change".

## Debug by default

`vilan run --backend rust` builds in **debug**, and so does `vilan build
--backend rust`. rustc is the inner loop from here on: a release build of a
seven-hundred-line program took over a second of CPU in the design probe, and a
real program is orders larger. When you want the optimised binary, build the
generated project yourself:

```sh
cargo build --release --manifest-path dist/native/hello/Cargo.toml
```

One exception is made for you: a program that reaches PBKDF2 gets the crypto
runtime crate optimised even in debug, because a password hash's cost is its
iteration count by design — 100,000 rounds of HMAC-SHA-512 takes seconds
unoptimised and a tenth of one optimised. Your own code keeps the debug build.

## Numbers

`i53` and `u53` are JavaScript-shaped types — the exact-integer range of a
double. Natively they are `i64` and `u64`, which is wider, but **the guarantee
is still the JavaScript one**: a value outside ±2^53 is outside what the
language promises, and a program that round-trips through both backends behaves
the same only inside that range. Every other numeric type is its own width on
both backends.

**`usize` is Rust's `usize` here** — the platform word, which is what a native
`len()` is — with the same JavaScript guarantee: `usize::max_value()` answers
`9007199254740992` on both backends. Subtracting one past zero is unspecified,
and the backends really do differ: JS prints the negative number, a native
debug build panics with `attempt to subtract with overflow`, a release build
wraps, and rustc refuses a subtraction it can see underflows at compile time. So
a program that underflows is not one this backend promises to print the same
bytes for, and the corpus's `usize-underflow.vl` is named outside the
differential for exactly that reason. `checked_sub` and `saturating_sub` answer
the same on both.

**`BigInt` is an `i128` here, and that is a limit rather than a promise.** On
the JS backend a `BigInt` is arbitrary precision; natively it is a 128-bit
signed integer — ±170,141,183,460,469,231,731,687,303,715,884,105,727 — because
the runtime this backend links against takes no dependencies and so has no
bignum to lower one to. The limit is enforced at both ends rather than papered
over: a literal past it is **refused at compile time**, naming the value and the
range, and an operation that leaves the range **traps at run time** with the
same sentence instead of wrapping to a wrong number. Inside the range the two
backends agree exactly, printing included — node writes a `BigInt` with its
suffix, so `7n / 2n` is `3n` on both. If you need arbitrary precision as the
SUBJECT of a program — RSA, a big factorial — that is the JS backend today.

**Integer overflow is the program's, on both backends.** A conforming program
does not overflow (spec §7.2a), and the two backends answer one that does
differently: the JS backend runs on with an out-of-range value, and a native
debug build panics. An overflow rustc can see at compile time — `a + 1` right
after `let a: i32 = 2147483647;` — does not build at all, and `vilan` says so as
the program's overflow, naming the expression rustc underlined in the emitted
Rust, rather than as a backend defect.

## Where the emitted Rust goes

`dist/native/<entry>/`, with `src/main.rs` the emitted program and `Cargo.toml`
pointing at the runtime crate. It is ordinary Rust and you can read it; that is
the point of emitting a language rather than machine code. `vilan build
--backend rust --stdout` prints it without building.

`rm -rf dist` means the same thing here as everywhere else.

## Not here yet

A desktop window shows a web page and nothing more yet: no menus, tray icon,
notifications, deep links, secure token storage, or second window to manage,
and no installer or package — each is its own slice. A GPU window's **native
UI** (`std::ui`) has its elements, layout, styles, events, focus, scrolling, and
keyed lists and conditional children;
its components (`std::ui::kit`: a button with behaviour, a text input, a virtual
list) come next, and paths, text editing, input methods and accessibility after
them.
