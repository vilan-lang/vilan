# Server-side rendering

A single-page app ships an empty `<div id="app">` and paints nothing until its
JavaScript loads, runs, and fetches. That costs first paint and it costs SEO: a
crawler sees a blank page. Server-side rendering fixes both by doing the first
render on the server: the page arrives already painted, and the client takes over
from there.

Vilan's model is **render, then replace**. There is no hydration: the client
does not adopt the server's DOM. It renders the same view fresh and swaps it in.

1. **Render.** The server calls your own view-building code against the process
   layer's `std::ui`, which builds an HTML string instead of live DOM.
   `render(view)` serializes it.
2. **Serve.** The handler splices that markup into an HTML shell and serves the
   page. The user (and the crawler) sees the full content: first paint and SEO
   are done before a line of JavaScript runs.
3. **Replace.** On boot the client builds the same view as live DOM and `mount`
   clears the container first, replacing the server markup with the live tree.
   Since the same component produced both, the swap is imperceptible when the data
   matches; when it moved between render and boot, the replace shows the
   truth.

## One component, both legs

Both legs share one `fun app(): View`. It imports `std::ui`, which
resolves *per entry*: the browser layer (live DOM) in the client leg, the
process layer (an HTML string tree) in the server leg. The same source, no
annotation. Put it in a module beside the two entry files (one package, two
entries: the [full-stack shape](../tour/platforms.md)); in a workspace, put it
in a `common` library both packages depend on instead.

```vilan
import std::ui::{ view, View, render };
import std::reactive::Signal;
import std::print;

// The one component both legs build.
fun app(): View {
	let tasks: Signal<List<str>> = Signal::new(["Render on the server", "Replace on boot"]);
	view("main")
		.child(view("h1").text("Tasks"))
		.child(view("ul").bind_each(tasks, |task| task, |task| view("li").text(task)))
}

// On the server, `render` turns the view into markup.
fun main() {
	print(render(app()));
	// <main><h1>Tasks</h1><ul><li>Render on the server</li><li>Replace on boot</li></ul></main>
}
```

## On the server: render and splice

The server serves the client leg's build and an HTML shell. The shell has a
marker where the app goes; the handler replaces it with the render. The splice
is plain user code (one `str::replace`, no framework):

```vilan,norun
import std::build::require_build;
import std::fs;
import std::http::{ Server, Request, Response };
import std::ui::{ view, View, render };

fun app(): View {
	view("main").child(view("h1").text("Tasks"))
}

async fun main() {
	let build = require_build("client");
	let shell = fs::read_file_to_str("src/app.html");
	Server::builder()
		.port(8791)
		.serve_build(build)
		// Create, serialize, discard: a fresh render per request, spliced into
		// the shell. No effects, no subscriptions survive the response.
		.on_request(|request| Response::builder().set_header("Content-Type", "text/html").body(shell.replace("<!--ssr-->", render(app()))).build())
		.build()
		.start();
}
```

`serve_build` installs the client leg's artifacts — here just `/client.js` —
in front of `on_request`, so the handler is only ever asked about the page.

The shell puts the marker inside the mount point:

```html
<div id="app"><!--ssr--></div>
<script type="module" src="/client.js"></script>
```

## On the client: mount replaces

The client entry is the shared `app()` and one `mount_root`. There is no server
mount: mounting is a client entry, not a renderable view, which is why
the natural factoring is a shared `fun app(): View` with a per-leg `main`.

```vilan,browser
import std::ui::{ view, View, mount_root };
import std::reactive::Signal;

fun app(): View {
	let tasks: Signal<List<str>> = Signal::new(["Render on the server", "Replace on boot"]);
	view("main")
		.child(view("h1").text("Tasks"))
		.child(view("ul").bind_each(tasks, |task| task, |task| view("li").text(task)))
}

fun main() {
	// mount_root clears the container (the server markup) and mounts the live UI.
	let _root = mount_root("app", || app());
}
```

`mount`/`mount_root` call `replaceChildren()` before appending, so the live tree
*replaces* the server-rendered nodes rather than stacking on top of them. For an
ordinary client-only app the container was empty, so the clear is a no-op; on an
SSR page it discards the server DOM and mounts fresh. The client never adopts
server nodes, addresses them, or reconciles against them.

## Build pure, bind reactive

The server render creates, serializes, and discards: no effects attach, no
subscriptions survive the request. So the bindings **read once**: `bind_text`,
`bind_attr`, `bind_each`, `when`, and `swap` embed the signal's value *at render
time*. That is the value served, and it is the value the client re-derives.

The rule this implies: **build pure, bind reactive.** A component that computes
its structure from signals and binds them renders correctly on both legs. A
component that leans on an effect side-channel at build time renders stale on the
server (effects don't run there). Text and attribute values are escaped, so a
hostile string is inert markup, not injected HTML.

## The double-fetch

Server-side rendering embeds no initial state today. A data-backed app
still fetches its data over rpc on boot exactly as a client-only app does,
so the data is fetched once for the server render and again on the client. A
change between the two shows as a content update when the client replaces,
not a mismatch error.

Serializing the initial store into the page and adopting it client-side (so the
first client fetch is skipped) is planned but not built, held back because it
introduces a cross-process state contract this version does not need. Further
out is *resumability*: the server serializes the reactive graph and the handlers
too, so the client executes nothing at boot. Render-and-replace is the fallback
that remains under both.

## Try it

The repository carries the whole loop as a runnable example,
[`vilan/examples/ssr/`](https://github.com/vilan-lang/vilan/tree/main/vilan/examples/ssr/):
one package with two entries, `src/app.vl` (the shared `app()`),
`src/client.vl` (browser), and `src/server.vl` (Node). With the
[toolchain](../tour/hello-vilan.md#install-the-toolchain) installed,
clone it and run:

```sh
git clone https://github.com/vilan-lang/vilan
cd vilan
vilan run vilan/examples/ssr
# open http://localhost:8791/ — view source shows the rendered markup
```

View the page source and you will see the list already in the HTML, before any
script runs. Load it in a browser and the client boots and replaces it in place.

To build the same thing from nothing instead, `vilan init my-app --template
fullstack` gives you the one-package/two-entries scaffold this page assumes
(see [Start a project](../tour/hello-vilan.md#start-a-project)); add the
shared `app()` module, `render` on the server leg, and the `<!--ssr-->` marker
in the shell, as above.
