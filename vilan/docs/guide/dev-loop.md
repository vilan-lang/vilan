# The dev loop

You built an app in the [walkthrough](walkthrough.md). This chapter is about
*iterating* on it: the edit-save-see loop. `vilan run --watch` on a
full-stack project closes that loop with **hot module replacement (HMR)**:
save a source file and the running browser app updates in place, reactive
state intact, without a full page reload.

Nothing here needs the walkthrough app specifically. Any project with a
browser leg will do, and `vilan init my-app --template fullstack` writes
one ([Start a project](../tour/hello-vilan.md#start-a-project)). From the
project directory:

```sh
vilan run --watch .
```

A project with a browser leg prints one extra line at startup:

```text
hmr: dev channel on 127.0.0.1:35917
```

That is the **dev channel**: a tiny local endpoint the browser connects back
to. From then on every save rebuilds all legs and the channel tells the
browser exactly what changed. There is no separate `dev` command:
`run --watch` already *means* "the dev loop".

## What each edit does

Change detection is by output bytes, not by guessing from the source: each
save rebuilds every leg, and the *artifacts* are compared. That makes the
verdict exact.

| You edited… | What happens |
|---|---|
| **Client code** | The browser bundle changes → a **swap**: the new bundle is evaluated in place, module state carried across (below). No reload. |
| **A stylesheet only** | Only the CSS sidecar changed → the stylesheet is **hot-swapped**, with no reload and no swap; the page doesn't flicker. |
| **Server code** | The server bundle changed → the **Node process restarts**. The browser stays connected; its live rpc mirror reconnects on its own (the same backoff that survives a server crash) and resyncs from the server's current values. |
| **Shared code** (a module both entries reach, or a `common` library both legs use) | Both bundles change → the server restarts and the browser swaps. The fresh client dials the new contract, so a changed rpc shape never leaves a stale client talking to a new server. |
| **A file with a mistake** | The compile error shows in the terminal *and* as an in-page **overlay** (the real file, line, and message) while the running app keeps its last good build. Fix it and the next good save clears the overlay and swaps normally. |

A server-only edit pushes nothing to the browser, so the client is
undisturbed. That the Node leg *restarts* rather than hot-swapping is
deliberate: the process is cheap and a fresh start is always correct, so
there is no server-side HMR to reason about.

The error overlay carries the real diagnostics (the file, the `line:col`,
the message, and any note): the same text your terminal shows, rendered over
the page so the eyes already on the browser don't miss it. The terminal stays
authoritative; the overlay is the copy, and the next successful save clears it.

## What carries across a swap, and what resets

A swap re-evaluates the whole client bundle. Two things survive it:

- **Module-level state.** Every top-level binding is carried across by its
  key (`package::module::name`) and a fingerprint of its type. A plain-data
  binding carries its value; a module-level `Signal` or `Shared` carries its
  *payload* into a fresh cell.
- **Everything the server holds.** The server doesn't swap: it restarts with
  its state in SQLite (or wherever it lives), and the client's mirror resyncs.
  In a full-stack app that is *most* of your durable state, which is why the
  swap can afford to be simple about the rest.

Top-level bindings like these keep their live values while you edit the view
that renders them:

```vilan,browser
import std::dev;
import std::reactive::Signal;

// Carried across every swap by key + type. Edit main's body, save, and these
// hold their values — only the view re-runs.
mut opened = 0;
let recent: Signal<List<str>> = Signal::new([]);

fun main() {
	opened = opened + 1;
	recent.set(["home"]);
}
```

What resets is state minted inside functions during mount: an ephemeral
signal created in a component, the focused element, scroll position,
half-typed text not yet pushed. Fine-grained reactivity gives these no stable
identity to reattach to, so a swap lets them go. A plain browser refresh is the
always-available complete reset.

**The initializer-edit rule.** Editing a binding's *initializer* without
changing its *type* keeps the live value; the new initializer does not
run:

```vilan,fragment
mut counter = 0;      // edit this to `mut counter = 100`, save…
// …and `counter` stays at whatever it had climbed to. During iteration the
// value you're watching *is* the work — this is the behavior every mainstream
// hot-reloader converged on.
```

Change the binding's *type*, though, and the old value is the wrong shape:
that binding fresh-initializes (a "fingerprint miss"), which is the correct
answer, not a failure. To carry a value your edit reshapes anyway, or to carry
something minted inside a function, reach for the manual channel:
[`std::dev`](../std/dev.md)'s `stash`/`take`.

## Escape hatches

- **`--no-hmr`**: turn HMR off and get the plain restart-the-whole-app watch
  loop (exactly the pre-HMR behavior). Reach for it if a swap ever surprises
  you and you want the blunt instrument back.
- **`--hmr-port <port>`**: the dev channel defaults to `35917`; change it if
  that port is taken. `--hmr-port 0` asks the OS for any free port and the
  startup line reports the one it got.
- **A browser refresh** is always a full, clean reset. Seed state lives only
  in the page's heap, so reloading throws all of it away.

## Running something alongside the build

A project that needs a step Vilan doesn't do (a Tailwind pass, an asset
pipeline, a codegen sidecar) declares it in the manifest:

```toml
[build]
run = "npx tailwindcss -i src/app.css -o dist/app.css"
```

Several steps go in a list and run in order:

```toml
[build]
run = [
	"npx tailwindcss -i src/app.css -o dist/app.css",
	"node scripts/generate-icons.mjs",
]
```

The rules are short:

- They run before each build, including every `--watch` round, because a
  hook exists to produce something the build then consumes.
- Each is a command line for your shell (`sh -c` / `cmd /C`), so pipes, globs
  and `&&` work; the working directory is the manifest's own, so relative paths
  mean what they say in the file you wrote them in.
- A hook that exits non-zero fails the build, naming the command. Nothing
  after it runs.
- Output goes straight to your terminal. (Under `vilan build --stdout`, which
  writes the emitted JS to stdout, a chatty hook shares that stream; redirect
  it in the command if you pipe the build.)
- `vilan check` produces no artifacts, so it runs no hooks.

## Picking which server to run

You only need this section if your project has two or more runnable Node
legs: a `[project]` of several packages, or one package with several Node
`[entry.<name>]` sections. The usual one-package app has a single Node entry,
so `run` runs it.

`run` (and `run --watch`) executes one Node leg. A project with a single `node`
leg needs no help; that one runs. With two or more (say a `server` and a
diagnostics `probe`), designate one in the manifest:

```toml
# one package, several entries
[package]
name = "app"
default-entry = "server"
```

```toml
# a workspace of packages
[project]
packages = ["client", "server", "probe"]
default-entry = "server"
```

Then `vilan run .` needs no flag. For a one-off (running the probe instead),
`--entry <name>` overrides the manifest:

```sh
vilan run --watch --entry probe .
```

With neither, `run` stops and lists the candidates:

```text
error: this workspace has more than one `node` package to run; pick one with --entry <name>, or designate one for good with `[project] default-entry` in vilan.toml: probe, server
```

A `default-entry` that names nothing runnable is an error too, rather than a
silent fallback. It names an `[entry.<name>]` section in the package shape, and
a member package's `name` in the workspace shape.

The non-selected Node legs still compile as part of the project: their
bundles land in `dist/` and a shared edit still recompiles them. They
aren't launched. Under `--watch` the browser legs hot-swap as usual; the
chosen server restarts on its own edits, and a change to a leg that isn't running
does nothing visible (its `dist/` bundle refreshes, but nothing restarts).

## The CSS `<link>` idiom

CSS hot-swap works by bumping the cache-buster on your stylesheet `<link>`, so
it needs the stylesheet to *be* a `<link>` to `dist/<leg>.css`:

```text
<link rel="stylesheet" href="/client.css">
```

An app that inlines its CSS into the page instead gets a full swap on a
style change rather than the flicker-free stylesheet reload. That is still
correct (the byte-diff classifies inlined CSS as a bundle change), but not
as surgical. The `<link>` form is the one to prefer for the tightest loop.

## Shipping routes separately

A browser leg ships as one file, so first load pays for every page in the
app. Opt one leg into route chunks and the compiler splits it for you:

```toml
[entry.client]
target = "browser"
split = true
```

Now `vilan build` writes an eager `dist/client.js` plus one
`dist/client.<Route>.js` per arm of your route `match`, and a
`dist/client.chunks.json` listing them (serve the whole directory and you
are done). No keyword, no `lazy()` wrapper, nothing to forget: the router
`match` already marks the seams, so the split is inferred from the code you
wrote. A function only one arm can reach rides that arm's file; anything
two arms share, and every module-level binding, stays eager.

**What the user sees while a chunk loads: the page they were on.** The
route signal doesn't advance until the code arrives, so there is no blank
frame and no placeholder tree to design — the previous page simply stays,
then swaps. First visit to a route pays one fetch; every later visit is
instant. If the fetch fails, the navigation doesn't happen (and the console
says why). For a progress indicator, bind `router::pending()` — it's an
ordinary `Signal<bool>`, so `show` (or `bind_text`, or a class) is all it
takes:

```vilan,browser
import std::reactive::Signal;
import std::router::{ current_path, pending, segments };
import std::ui::{ View, mount_root, view };

[derive(PartialEq)]
enum Route {
	Home,
	NotFound,
}

fun parse(path: str): Route {
	if segments(path).len() == 0 { Route::Home } else { Route::NotFound }
}

fun home_page(): View {
	view("h1").text("Home")
}

fun missing_page(): View {
	view("h1").text("Nothing here")
}

fun main() {
	let route = current_path().map(parse);
	let _root = mount_root("app", || {
		view("main")
			// Visible only while a route chunk is in flight; the page behind
			// it is the one you were already on.
			.child(view("div").class("spinner").text("Loading…").show(pending()))
			.swap(route, |current| match current {
				Route::Home => home_page(),
				Route::NotFound => missing_page(),
			})
	});
}
```

Two things to know. `split` is a `browser` leg's key — a Node entry has no
navigation to gate, and the build says so rather than ignoring the line.
And `--watch` builds ignore it: HMR swaps whole bundles, so the dev loop
keeps working on one file. Single-file emission stays the default and is
not going anywhere; `split` is opt-in, per leg.

`vilan build --print-chunks` reports what *would* split without emitting
anything, which is the way to find out whether a leg has enough per-route
code to be worth it.

## Cleaning up strays

The swap disposes the UI root and closes the live rpc socket for you. Anything
*else* a bundle started outside the reactive system (a raw interval, a bare
task) keeps running after a swap unless you register a cleanup. That, plus
the `stash`/`take` carryover channel and the `hmr_active` guard, is the whole
of [`std::dev`](../std/dev.md).
