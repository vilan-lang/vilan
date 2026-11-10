# Windows

`std::window` opens a desktop window showing a web page in the operating
system's own webview — WebKitGTK on Linux, WKWebView on macOS, WebView2 on
Windows. It is the short road to a desktop app: one native binary that serves
its page from an in-process [`std::http`](process.md#stdhttp-the-server) server and shows
it, with no new UI layer. The [native guide](../guide/native.md#a-desktop-window)
walks through a whole app.

**Native only.** A window is part of a `--backend rust` build. The JavaScript
build refuses a program that reaches this module, naming it; `vilan check`
accepts one, because a check is the same for both backends. It is a process
module, so a browser build that reaches it is refused at the platform check.

```vilan,fragment
struct Window;          // an open window: a handle
struct WindowBuilder;   // what `open` makes

impl Window {
	fun builder(url: str): WindowBuilder    // untitled, 800 x 600
	fun close(self)                         // closing the last one stops the program
}

impl WindowBuilder {
	fun title(own self, title: str): WindowBuilder
	fun size(own self, width: u32, height: u32): WindowBuilder   // logical pixels
	fun open(self): Result<Window, str>
}
```

`open` answers an `Err` with the reason when there is no window system (a
machine with no display), when the webview will not start, or for a side of
zero. It answers once the window exists; the page loads after, and a page that
fails to load is the webview's to show, as a browser's is.

## How a desktop program ends

Closing the **last** window — by hand, or with `Window::close` — stops the
program the way SIGTERM stops a native server: the servers stop accepting and
drain, the event loop ends, `main` returns, and the process exits with code 0.
SIGTERM itself does the same with a window still open. A program with no
server stays up while its window is open.

```vilan,native
import std::io::print;
import std::window::Window;

fun main() {
	match Window::builder("https://example.com/").title("Example").size(900, 600).open() {
		Ok(_) => print("open"),
		Err(let reason) => print(i"no window: {reason}"),
	}
}
main();
```

## Not here yet

One window per call and nothing else to manage it: no menus, tray icon,
notifications, deep links, secure token storage, or more than open and close.
Each is its own slice.
