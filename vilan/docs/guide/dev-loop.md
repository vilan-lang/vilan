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

The channel binds `127.0.0.1` only, and every one of its routes requires a
token minted fresh for that `run --watch` process. Your page has it because
`run --watch` bakes it into the browser bundle your own server hands out;
nothing else does, so another page open in the same browser cannot read your
compile diagnostics off the channel or trigger reloads at it. Nothing to
configure — the token is per-run and dies with the process — but it does mean
a browser tab left over from a *previous* watch session cannot talk to the
new one until you reload it, and that hand-driving the routes with `curl`
needs the token out of `dist/<leg>.js`.

## What each edit does

Change detection is by output bytes, not by guessing from the source: each
save rebuilds every leg, and the *artifacts* are compared. That makes the
verdict exact.

| You edited… | What happens |
|---|---|
| **Client code** | The browser bundle changes → a **swap**: the new bundle is evaluated in place, module state carried across (below). No reload. |
| **A stylesheet only** | Only the CSS sidecar's *text* changed → the stylesheet is **hot-swapped**, with no reload and no swap; the page doesn't flicker. |
| **A stylesheet appearing or disappearing** | A leg that emitted no styles now does (or stops) → a **swap**. What changed is which stylesheets the page has, not what one of them says, and that is a change to the browser's output like any other. The round declares its stylesheets, so the new sheet lands even on a page whose markup has no `<link>` for it, and a removed one is taken back out. |
| **Server code** | The server bundle changed → the **Node process restarts**. The browser stays connected; its live rpc mirror reconnects on its own (the same backoff that survives a server crash) and resyncs from the server's current values. |
| **Shared code** (a module both entries reach, or a `common` library both legs use) | Both bundles change → the server restarts and the browser swaps. The fresh client dials the new contract, so a changed rpc shape never leaves a stale client talking to a new server. |
| **A file with a mistake** | The compile error shows in the terminal *and* as an in-page **overlay** (the real file, line, and message) while the running app keeps its last good build. Fix it and the next good save clears the overlay and swaps normally. |

A server-only edit pushes nothing to the browser, so the client is
undisturbed. That the Node leg *restarts* rather than hot-swapping is
deliberate: the process is cheap and a fresh start is always correct, so
there is no server-side HMR to reason about.

The error overlay carries the real diagnostics (the file, the `line:col`,
the message, and any note): the same text your terminal shows, rendered over
the page so the eyes already on the browser don't miss it. A context
refusal brings its chain along — one `via src/client.vl:13:8 — the context
requirement flows through this call` line per uncovered call between the
entry and the read, each located in the file the call sits in — so the
overlay answers *which* call left the read uncovered, as the terminal's
labels and the editor's related information do. The terminal stays
authoritative; the overlay is the copy, and the next successful save clears it.

## When a round fails

A round is one pass of the loop: the build hooks below, then the compile, then
whatever the command does with the result. A round can fail — a compile error, a
hook that exits non-zero — and when it does the terminal says so and the session
keeps watching.

Under `vilan build --watch`, `vilan check --watch` and `vilan test --watch`, the
change that started a failed round is **kept, and the round is retried once** on
the next poll. That matters for the failure that is nobody's fault: a hook
command hiccups on a loaded machine, a generator races something outside your
tree, a file is locked for the half-second the round needed it. Without the
retry that save would be gone — not "failed" gone, *silently* gone, because the
loop had already spent the change and would sit quiet until you touched some
other file. The retry costs one extra run and closes that hole.

It happens once, and then the loop waits for the next change:

- A **compile error** retried once fails the same way a second time and the
  session goes quiet — which is correct, because the fix for a broken tree is
  your next edit, not another attempt at the same bytes. You pay one extra
  compile per broken save.
- Nothing spins. A retry is an ordinary round on the ordinary poll interval, and
  a failing tree gets at most two runs per change however fast you save.
- The loop narrates it: a retried round says it is retrying, and the run that
  gives up says it is waiting for the next change.

`vilan run --watch` is the exception, deliberately. Its rounds handle their own
failures — the terminal, the in-page error overlay, the app left running on its
last good build — and hand the loop no verdict to act on, so a failed round
there waits for the next change as it always has. Which is the same answer the
retry reaches anyway for the failure you actually hit while iterating: fix the
file, and the save that fixes it is the round that clears the overlay.

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
import std::reactive::{ Signal, SignalCell };

// Carried across every swap by key + type. Edit main's body, save, and these
// hold their values — only the view re-runs.
mut opened = 0;
let recent: SignalCell<List<str>> = Signal::new([]);

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
- They run as you. A hook has your privileges — your files, your environment,
  your keys — and nothing prompts: building a project runs its hooks, silently
  and unsandboxed, exactly as if you had typed the command. That is the trust
  `cargo build` and `npm run` already take, and Vilan doesn't ask for code you
  wrote. Only the manifest being built declares hooks — a dependency's are never
  run — so an unfamiliar `vilan.toml` is worth reading before you build it, the
  way you would read a `Makefile`.
- A hook that exits non-zero fails the build, naming the command. Nothing
  after it runs. Under `--watch` that round is retried once before the loop
  goes back to waiting ([When a round fails](#when-a-round-fails)), so a hook
  that failed on a hiccup gets a second chance at the save that started it.
- Vilan prints each command before spawning it, and the output goes straight to
  your terminal. (Under `vilan build --stdout`, which writes the emitted JS to
  stdout, a chatty hook shares that stream; redirect it in the command if you
  pipe the build.)
- `vilan check` produces no artifacts, so it runs no hooks.

Running on every round is right for a Tailwind pass and wrong for a step that
downloads something, or generates a thousand files. A hook that should run
only when its inputs move gets a name and says what it reads and writes:

```toml
[[build.hook]]
name    = "icons"
run     = "node scripts/generate-icons.mjs"
inputs  = ["scripts/generate-icons.mjs", "icons.lock"]
outputs = ["src/icons.vl"]
```

It runs on a clean checkout, and then it doesn't: while every `inputs` entry
and every `outputs` entry still hashes to what it hashed last time, and the
command itself hasn't changed, the build prints `Fresh   icons` and moves on.
The rest of the rules:

- **Content, never timestamps.** Rewriting a file with the same bytes is not a
  change. Touching it isn't either.
- **Declared paths only, and no globs.** A path is a file or a directory (a
  directory hashes its whole tree, so `inputs = ["src/static"]` means what it
  looks like it means). A pattern such as `src/**/*.css` is a manifest error,
  not a match — it would hash as a file that is never there and freeze the
  hook after its first run.
- **A directory's tree is its members**, so a subdirectory counts even while it
  is empty: creating or removing `src/static/icons` re-runs the hook the same
  way adding a file does, and so does replacing a directory with a file of the
  same name. That is also what `--watch` wakes on, so the round you get and the
  hook that runs in it always agree.
- **A missing input is recorded as missing**, so creating it later re-runs the
  hook. A missing or hand-edited output re-runs it too. A declared path that is
  a *symlink* is followed — `inputs = ["static"]` may name a link to the real
  directory, and it hashes as that directory's tree — and a link with no target
  counts as missing, the same as an absent path. Links found *inside* a declared
  tree are a different question and are not followed: one hashes as its target
  path, so the walk can neither leave the tree nor run into a cycle.
- **Under `--watch`, a declared input starts a round.** Saving a file the hook
  names in `inputs` — or adding to, or editing anything inside, a directory it
  names — wakes the loop exactly the way saving a `.vl` source does, and the
  round then re-runs the hook. One declaration, read the same way by the
  freshness check and by the watcher. `outputs` are deliberately *not* watched:
  a hook writing what it said it writes must never trigger the build that ran
  it.
- **A hook that succeeds without writing a declared output is told so**, by
  name and by path. Nothing is recorded for a hook whose output isn't there, so
  it would otherwise re-run on every build in silence while the failure
  surfaced somewhere else — at the import of the module it was supposed to
  write. Write the file, or drop it from `outputs`.
- **A hook that declares neither `inputs` nor `outputs` runs every time**,
  exactly like a `run = [...]` entry. That is the default, and `run` keeps
  working unchanged.
- **The record lives in `dist/.build-hooks.json`**, so `rm -rf dist` means
  what you already think it means: rebuild everything, hooks included. Nothing
  is cached outside your project. `vilan build --rerun-hooks` is the one-off
  version, for a hook that reads something it forgot to declare.
- Hooks run in declaration order, with every `run = [...]` command first.

Freshness is about cost, never about safety: a skipped hook is code you
already trusted to run, and skipping it buys time, not containment.

When you want the whole picture rather than one `Fresh` line — which hook
wrote which file, which `const` site emitted which stylesheet, and what a
change to a given input would move — run
[`vilan build --explain`](../appendix/cli.md#vilan-build---explain). It
builds and then prints one block per output, naming each declared hook
output with this build's verdict for it (`(ran)` or `(Fresh)`), and one
block per tracked input — hook `inputs` included — with what it
invalidates.

### When the hook generates Vilan

A hook that writes a `.vl` module has one more thing to say, and leaving it
unsaid costs you the freshness you just bought. Run `vilan fmt` over a tree
holding a generated module and you get a loop: the formatter rewrites the
module, the rewrite is a change to a declared output, the hook goes stale, the
next build regenerates the file — unformatted, because a generator emits what
its templates produce — and the next format rewrites it again. Neither tool is
wrong, nothing reports anything, and with format-on-save it happens to files
you only opened to read.

Say where the products live, and the formatter leaves them alone:

```toml
[package]
name      = "app"
generated = "src/icons"

[[build.hook]]
name    = "icons"
run     = "node scripts/generate-icons.mjs"
inputs  = ["scripts/generate-icons.mjs", "icons.lock"]
outputs = ["src/icons/lib.vl"]
```

- **Everything under the root is left byte-identical** by `vilan fmt`, by
  `vilan fmt --check`, and by your editor's format-on-save — however the file
  is reached, including by name. `fmt` says so once per run, with a count.
- **It's a directory inside the package**, and not the source `root` itself:
  pointing it there would leave every hand-written module unformatted, so it's
  a manifest error. It doesn't have to exist yet — before the first build, it
  usually doesn't.
- **Nothing else reads the key.** The generated module resolves exactly as it
  did: `src/icons/lib.vl` is `pkg::icons`, the same as any other module whose
  file is a directory's `lib.vl`.
- **Whether you commit the directory is your call.** Most projects add it to
  `.gitignore`; some deliberately commit generated sources so a reader doesn't
  need the generator's toolchain. Nothing enforces either, and the formatter's
  rule holds the same way regardless — it reads the manifest, never
  `.gitignore`.
- **The escape is the manifest, not a flag.** If a file should be formatted,
  it isn't a product: move it out, or drop the key. Both land in review.

A dependency's hooks are a different question, and the answer is still no. If
a package you depend on declares one, the build says so — one dim `note:` line
naming it, once per build — and does not run it. Granting one is a line in
your own manifest, next to where the dependency comes from:

```toml
[package.dependencies]
icons = { git = "https://example.com/icons.git", tag = "v1.2.0", build-hooks = true }
```

Absent means no, and today so does present: **no dependency's hooks run yet**,
opted in or not. The key exists now so the grant is a reviewable line in a diff
before there is any mechanism behind it.

## Freshness for a hand-rolled server

A running server that reads a file once at boot (`fs::read_file_to_str`
before `Server::builder()...start()`, say) keeps serving those bytes for
the life of the process — editing that file produces no round the server
itself ever sees. `std::watch` closes that gap for code you wrote by hand:

```vilan,norun
import std::fs;
import std::http::{ Response, Server };
import std::watch;

fun main() {
	Server::builder()
		.port(0)
		.on_request(|request| {
			// Cheap: re-read on every request instead of once at boot.
			let shell = fs::read_file_to_str("dist/app.html");
			// Tell every connected browser to reload once it has the fresh
			// bytes. A no-op outside `run --watch`, so it costs nothing to
			// leave the call in a shipped build.
			watch::force_refresh();
			Response::builder().body(shell).build()
		})
		.build()
		.start();
}
```

`force_refresh()` only ever reloads a browser connected to the SAME dev
channel this server's `run --watch` session started — it has no effect on
a plain `vilan run`, and it is not how `run --watch`'s own `swap`/`css`
push works (those fire automatically, on a round; this is the manual,
explicit escape hatch for state a round can't see). Reference:
[`std::watch`](../std/process.md).

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

CSS hot-swap looks for your stylesheet as a `<link>` to `dist/<leg>.css`:

```text
<link rel="stylesheet" href="/client.css">
```

What it does *not* do is re-fetch that href. The href is your own server's
route, and the common shape reads that file once at boot and serves the
same bytes for the life of the process — a css-only round never restarts
the server, so re-requesting the route would land right back on the
boot-time snapshot: a style edit that visibly does nothing. So the fresh
bytes come from the **dev channel's** own `/asset/<name>` route, which
serves current `dist/*.css` every round, and land as an injected `<style>`
that supersedes the `<link>` (disabled, its href untouched). A plain page
reload therefore always starts clean, and a fetch that fails warns and
leaves the current stylesheet exactly as it was rather than reloading onto
stale bytes. A named sidecar updates only the `<link>` whose file it is, so
a workspace with two browser legs refreshes exactly the one that changed.

The `<link>` is where the sheet is *superseded*, not the only way it can
arrive. A page rendered before your leg emitted any styles carries no
`<link>` for them — and since a client-only round never restarts your
server, it never grows one — so this leg's own stylesheet joins `<head>` on
its own when there is nothing to supersede. (Only ever this leg's: another
browser leg's sidecar is that leg's page's business, and is left alone
here.) The same reconciliation runs on a swap, which is why a round that
changed both your code and your styles updates both, and why deleting your
styles takes the sheet back out of the page instead of leaving the last
version showing.

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
`dist/client.chunks.json` listing them. No keyword, no `lazy()` wrapper,
nothing to forget: the router `match` already marks the seams, so the split
is inferred from the code you wrote. A function only one arm can reach rides
that arm's file; anything two arms share, and every module-level binding,
stays eager.

**What the user sees while a chunk loads: the page they were on.** The
route signal doesn't advance until the code arrives, so there is no blank
frame and no placeholder tree to design — the previous page simply stays,
then swaps. The boot route's chunk starts downloading before the shell is
even built, so first paint waits on the network and not on your own
JavaScript. First visit to a route pays one fetch; every later visit is
instant, and a route nobody visits is never downloaded.

Navigating away from a page whose chunk is still in flight is safe: the
LATEST navigation wins, whatever order the fetches finish in, so a slow
chunk can never land on top of the page you moved to.

Two signals are the whole surface. `router::pending()` is true while a chunk
is in flight; `router::chunk_error()` is `Some(reason)` when the last fetch
failed. A failed fetch means the navigation simply did not happen — the page
you were on is still there — and nothing is remembered as in flight, so
**clicking the link again retries**. There is no retry API because a link is
one. Both are ordinary signals, so `show`, `bind_text` or a class is all it
takes:

```vilan,browser
import std::option::Option::{ None, Some, self };
import std::reactive::{ Signal, SignalCell };
import std::router::{ chunk_error, current_path, pending, segments };
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
			// …and if it never arrives, say so. The next click retries.
			.child(view("div").class("error").bind_text(chunk_error().map(|failure| match failure {
				Some(let reason) => "Could not load that page: " + reason,
				None => "",
			})))
			.swap(route, |current| match current {
				Route::Home => home_page(),
				Route::NotFound => missing_page(),
			})
	});
}
```

### Serving the chunks

A chunk is fetched from the same directory the bundle was served from, so a
static host needs nothing: serve `dist/` and you are done. A vilan server
needs nothing either — `dist/client.chunks.json` is the leg's **build
manifest**, and `serve_build` turns it into routes:

```json
{
	"leg": "client",
	"entry": "client.js",
	"styles": "client.css",
	"classic_script": true,
	"chunks": [
		{ "arm": "Route::Home", "tag": 0, "file": "client.Route_Home.js" },
		{ "arm": "Route::Docs(..)", "tag": 1, "file": "client.Route_Docs.js" }
	]
}
```

Every build of a browser leg writes it, split or not — a leg that does not
split gets `"chunks": []` and `"classic_script": false`. That is deliberate:
an absent file cannot tell "did not split" from "was never built", and
`build_of` needs the difference. `styles` is `null` when the leg compiled no
`const style()`, which is the one thing an `fs::stat` probe could never
answer in both directions.

`examples/fullstack`'s server reads it with `build_of("client")` and hands
the result to `serve_build`, which installs one route per artifact — so
adding, renaming or removing a route arm needs no server change, and neither
does turning `split` on.

### Is it worth it?

Splitting is not free. The route gate, the per-chunk forwarders and the
embedded chunk map are a fixed cost paid once per split leg — on the order
of 6 KB — so a leg whose pages are small ships MORE on first load than it
would whole. The build measures this rather than leaving you to guess: it
emits your entry both ways and, when splitting came out no smaller, warns
with your leg's own numbers.

```text
warning: `split` on `client`: splitting adds 1720 bytes to the first load
and defers only 6802 — the route gate, the forwarders and the chunk map cost
more than this leg's per-route code saves. Consider dropping it…
```

`vilan build --print-chunks` prints the same verdict without opting in, so
you can measure a leg before you split it.

### The rest of the rules

`split` is a `browser` leg's key — a Node entry has no navigation to gate,
and the build says so rather than ignoring the line. `vilan run` ignores it,
watched or not: the dev loop hot-swaps whole bundles, so it emits one file
per leg and says so once. (It also clears any chunk files a previous
`vilan build` left, so `dist/` never describes a build that is no longer
there — the same sweep runs on every build, so a renamed route arm never
leaves its old chunk behind either.) Single-file emission stays the default
and is not going anywhere; `split` is opt-in, per leg, and a `vilan build`
decision.

## Cleaning up strays

The swap disposes the UI root and closes the live rpc socket for you. Anything
*else* a bundle started outside the reactive system (a raw interval, a bare
task) keeps running after a swap unless you register a cleanup. That, plus
the `stash`/`take` carryover channel and the `hmr_active` guard, is the whole
of [`std::dev`](../std/dev.md).
