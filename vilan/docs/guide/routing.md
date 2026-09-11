# Routing

Routers you've used probably match URL pattern strings: `"/w/:id/task/:tid"`.
Vilan's router doesn't. **Routes are enums.** You describe your app's pages
as an enum, write one function that parses a path into it and one that
prints it back, and the type system takes it from there. Every link targets
a page that exists. Every page receives exactly the parameters it declares.
When you add a page, the compiler points at every `match` that now needs to
handle it. A pattern-string router can't promise any of that.

`std::router` supplies the primitives: the live path signal, `navigate`,
the `link` helper, and `parse_path` / `FromPath` for reading a URL back.

## The route model

Here's a small two-level app: a home page, and workspace pages that have
their own sub-pages.

```vilan,browser
import std::ui::{ view, View, mount_root };
import std::router::{ current_path, navigate, segments, link, Routable };
import std::reactive::{ Signal, SignalCell };
import std::option::Option::{ self, Some, None };

[derive(PartialEq)]
enum Route {
	Home,
	Workspace(i32, WorkspaceRoute),
	NotFound,
}

[derive(PartialEq)]
enum WorkspaceRoute {
	Overview,
	Task(i32),
}

// The inverse pair. `parse` consumes `segments(path)`; `href` prints the
// same shape back. Keep them adjacent — they must agree.
fun parse(path: str): Route {
	let parts = segments(path);
	if parts.len() == 0 {
		ret Route::Home;
	}
	if parts[0] == "w" && parts.len() >= 2 {
		match parts[1].parse_i32() {
			Some(let id) => {
				if parts.len() == 2 {
					ret Route::Workspace(id, WorkspaceRoute::Overview);
				}
				if parts.len() == 4 && parts[2] == "task" {
					match parts[3].parse_i32() {
						Some(let task) => ret Route::Workspace(id, WorkspaceRoute::Task(task)),
						None => {},
					}
				}
			},
			None => {},
		}
	}
	Route::NotFound
}

fun href(route: Route): str {
	match route {
		Route::Home => "/",
		Route::Workspace(let id, let inner) => match inner {
			WorkspaceRoute::Overview => i"/w/{id}",
			WorkspaceRoute::Task(let task) => i"/w/{id}/task/{task}",
		},
		Route::NotFound => "/",
	}
}

impl Route with Routable {
	fun to_path(self): str {
		href(self)
	}
}

fun main() {
	let route = current_path().map(parse);
	let _root = mount_root("app", || {
		view("div").swap(route, |current| match current {
			Route::Home => view("h1").text("Home"),
			Route::Workspace(let id, let _inner) => view("h1").text(i"Workspace {id}"),
			Route::NotFound => view("h1").text("Nothing here"),
		})
	});
}
```

That's the whole pattern. Yes, `parse` is more code than a pattern
string. In exchange it's ordinary code: type-checked, debuggable, and
free to do things pattern strings can't (validation, aliases, redirects).
Now the pieces one at a time.

## Reading the URL back

`parse` above is one half of a pair the compiler cannot hold together, so
`std::router` gives it a shape: `Routable` writes a route to a URL,
`FromPath` reads one back, and a type implementing both states its wire
format once in each direction.

`FromPath`'s member takes SEGMENTS, not a path — `parse_path` has already
cut the URL, dropped the empty segments, and percent-decoded every piece,
so what's left is a `match` on `len()` and a few string comparisons. The
free `from_path` is the call: it runs `parse_path` and hands the segments
on. It's a free function rather than a trait default because an associated
function can't be inherited through a receiver that doesn't exist — the
compiler says so if you try.

```vilan,browser
import std::io::print;
import std::router::{ FromPath, PathParts, Routable, from_path, parse_path };

[derive(PartialEq)]
enum Route {
	Home,
	Workspace(i32),
	NotFound,
}

impl Route with FromPath {
	fun from_segments(parts: List<str>): Route {
		match parts.len() {
			0 => Route::Home,
			_ => {
				if parts[0] == "w" && parts.len() > 1 {
					match parts[1].parse_i32() {
						Some(let id) => Route::Workspace(id),
						None => Route::NotFound,
					}
				} else {
					Route::NotFound
				}
			},
		}
	}
}

impl Route with Routable {
	fun to_path(self): str {
		match self {
			Route::Home => "/",
			Route::Workspace(let id) => i"/w/{id}",
			Route::NotFound => "/404",
		}
	}
}

fun main() {
	// The law the pair has: `from_path(route.to_path())` is `route`.
	let route: Route = from_path("/w/12/");
	print(route.to_path());

	// The query and the fragment, which `segments` never saw.
	let parts: PathParts = parse_path("/w/12?tab=due%20soon&open#notes");
	match parts.query.get("tab") {
		Some(let tab) => print(tab),
		None => print("no tab"),
	}
	// A key with no `=` is PRESENT with an empty value, so a flag is a
	// `contains_key` question.
	let flagged = parts.query.contains_key("open");
	print(i"open={flagged}");
	match parts.fragment {
		Some(let anchor) => print(anchor),
		None => print("no fragment"),
	}
}
```

`from_path` is total — there is no `Option`. A URL your route space
doesn't recognize is a route (`NotFound`), and modelling it as one means
you get to render it.

## The live path becomes a route signal

`current_path()` is a `SignalCell<str>` of `location.pathname`. It stays
current across `navigate` calls and the browser's back/forward buttons.
Derive your typed route from it once:

```vilan,fragment
let route = current_path().map(parse);
```

(Passing `parse` by name instead of `|p| parse(p)` is the named-function
coercion from [the tour](../tour/functions-and-closures.md).)

## Pages swap on the route

`View.swap(route, render)` is the page container. When the route
changes, it tears down the old page (disposing all its bindings) and
builds the new one. When the route *doesn't* change (say the user
clicks a link to the page they're on), nothing happens at all. That's
why route enums derive `PartialEq`.

Nesting works the way you'd hope: the workspace page can `swap` on its
own `WorkspaceRoute` while the outer swap only rebuilds when the
workspace id changes.

## Links and navigation

`link(label, route)` renders a real `<a href=…>`. Middle-click,
ctrl-click, and copy-link-address all behave like a normal link. Only a
plain left click is intercepted and turned into an in-app navigation.
And because it takes your route enum rather than a string, a dead link
is a compile error:

```vilan,fragment
link("← All workspaces", Route::Home)
link(task.name, Route::Workspace(workspace_id, WorkspaceRoute::Task(task.id)))
```

The anchor is built `draggable="false"`, because a link drag started while a
quick click's navigation is still settling wedges the whole tab in Chrome (the
[gotchas](../appendix/gotchas.md) page has the mechanism) — the `href` stays,
so nothing native is lost. If you build and style your own anchor, put
`link_to` on it and you get the same three things:

```vilan,fragment
view("a").class("nav-item").link_to(Route::Home).text("Home")
```

For programmatic navigation (after a sign-out, after creating a thing):

```vilan,fragment
navigate(href(Route::Home));
```

`navigate` joins the caller's current turn, so a handler's state changes
and the page change land together as one update.

## Deep links and the server

When someone loads `/w/3/task/7` fresh, the request goes to your
*server*, which has to answer with the app shell no matter the path.
That's the standard history-API fallback, and the `on_request` handler
on your server's builder chain does it:

```vilan,norun
import std::build::require_build;
import std::document::require_shell;
import std::http::{ Response, Server };

async fun main() {
	let build = require_build("client");
	let page = require_shell("src/app.html", build).html();

	Server::builder()
		.port(4000)
		.serve_build(build)
		// Every path the build does not claim serves the shell.
		.on_request(|request| Response::builder().set_header("Content-Type", "text/html").body(page).build())
		.on_start(|server| print(i"listening on {server.url()}"))
		.build()
		.start();
}
```

`serve_build(build)` answers the build's own artifacts (`/client.js`,
`/client.css`, and any route chunks) *before* `on_request`, whatever order
the chain was written in, so the `on_request` handler is the catch-all
deep links need: it gets every path the build does not claim. An rpc app
adds `.with_service(Service::new(protocol))` to the same chain and the
rule holds — the service and the build answer first, your handler answers
the rest ([Persistence](persistence.md#serving-http-stdhttp),
[Services & RPC](services.md#the-server-side)).

On the client side, a deep-linked page usually needs data that hasn't
synced yet. Mount it under `when(present)` so it appears when the data
does (see [Services & RPC](services.md)).

## Traps

- Keep `parse` and `href` next to each other and test them as a pair.
  Their agreement is the one thing the type system can't check for you.
  A `parse` that drops a segment silently turns a working deep link into
  a NotFound.
- `parse_path` already forgives trailing and duplicate slashes (they
  produce no segment). Don't special-case them in `from_segments`.
- `segments` is the RAW splitter and `parse_path` is the decoding one:
  `segments("/a%20b")` is `["a%20b"]`, `parse_path("/a%20b").segments` is
  `["a b"]`. Route matching wants the decoded form.
- `current_path()` is `location.pathname` — the query and the fragment are
  deliberately not in it, so a `#anchor` doesn't re-render the page and a
  route's `to_path()` still compares equal to where you are. Read the whole
  URL with `location_url()` when the query matters, on load and in a
  `popstate` handler.
