# Full-stack setup — the document, the assets, a server that grows (E56)

> Status: RATIFIED 2026-08-11 as recommended ("The recommendations for …
> fullstack-dx.md look good") — the nine recommended answers of §10 stand,
> and §10.4 CLOSED the same day: the owner re-read ssr.md §6(b) and ruled
> the decline was scoped to the zero-knowledge wrapper, not a boundary on
> std and the document (the §6(b) addendum records it) — `render_into` stays
> declined on its own terms and §5's `Document` proceeds. One
> post-ratification amendment: dev-mode asset freshness joins
> `serve_build`/`Document` as their dev policy (dev-refresh.md §5 records
> why). Every slice of §8 is unblocked. Filed from backlog E56, the owner's
> 2026-08-10 charter (`backlog-2026-07-18.md`, section E item 56).
>
> Origin: the owner built a real full-stack todo app
> (`vilan-playground/todo`) and the *setup* fought back. The charter names
> three seams and one experience: **(a)** `Server::builder()` and
> `serve_service` do not compose — growing an app into rpc means replacing its
> boot function; **(b)** no document abstraction exists anywhere, so every
> example, every template and the app itself hand-writes an HTML shell and
> boot-reads `dist/` from disk; **(c)** `vilan init` is the language's opening
> argument and its default template teaches both problems. The bar the owner
> set for (b), verbatim: *easy to set up, LOUD when wrong (a shell missing its
> stylesheet link shipped silently), progressively lowering to full control for
> those who want the raw shell.*
>
> Method, per the charter: survey first, like E49. §1–§3 are the inventory, the
> measurement and the mechanisms; §4–§6 are the design, one section per seam;
> §7 reconciles with the six ratified records this paper touches; §8 slices it;
> §9 files the bycatch the survey turned up; §10 is the open-questions set and
> §11 collects the recommendations. **Everything before §10 is a
> recommendation, not a ratification.** This paper proposes no code and
> compiles nothing; every claim about what the tree does today was read out of
> the tree and is cited by `file:line` or `filename §section`.
>
> Two ratified decisions sit in visible tension with the charter and are
> addressed head-on rather than worked around: `ssr.md` §6(b) declined a
> `render_into` splice helper for v1, and `hmr.md` §8 makes server-side HMR a
> permanent non-goal. §7 states why neither is contradicted here.

## 0. The thesis, and the number

The vilan full-stack story has three good halves and no seam between them. The
service is a struct with `[service]` on it and the client is generated
(`transport-rpc.md` §4.2). The UI is a `View` tree with signals under it
(`router.md`, `ui-styling.md`). The build emits exactly what a browser needs —
`dist/<leg>.js`, `dist/<leg>.css`, `dist/<leg>.chunks.json` — and knows, at the
moment it writes them, precisely what it wrote.

Between those three there is a gap that nothing in the language, the standard
library, or the toolchain fills, and the user fills it by hand, in prose, every
time: a hand-authored HTML document, a `fs::read_file_to_str` per artifact at
server boot, and a `match request.path()` table mapping URLs onto the strings
those reads produced. Nothing checks that the document names the artifacts the
build wrote. Nothing checks that the routes name the paths the document
requested. Nothing checks that the mount element the client asks for exists.
Every one of those three contracts is a bare string, written twice in two
files, and every one of them fails silently.

**The measurement.** In the owner's todo app, the service is 25 lines and the
UI is 27. Getting those two to meet in a browser cost **32 further lines across
two files, 22 of which never mention a note** (§2.1). That is the ratio the
paper is about: the ceremony is not large in absolute terms — nobody is
drowning — but it is **69% of the code between a working service and a working
page**, it is identical in every project that has ever been written in this
language, and it is where the failures are silent.

**The thesis, in one sentence: the build already knows everything the ceremony
restates, and the ceremony exists only because nothing carries that knowledge
across the leg boundary.** The compiler knows whether a stylesheet was emitted;
the server has to probe the filesystem to guess (`ui-styling.md` §0bis, the
template's `fs::exists` guard). The compiler knows every chunk it wrote and
writes them into a manifest; the server re-reads that manifest at boot and
builds a route table from it (`dev-loop.md` § "Serving the chunks"). The
compiler decided how the bundle must be loaded for its own chunk resolution to
work; the shell guesses, and every shell in this tree has guessed the one form
that makes that resolution unreachable (§3.5).

Two corollaries follow, and they are the shape of the design:

1. **The fix is a description, not a framework.** What is missing is a value —
   *what this leg's build emitted* — reachable from the server leg. Given that
   value, serving the assets is four lines of library code, generating a
   correct document is twenty, and *validating a hand-written one* is the
   thing that makes the escape hatch safe rather than merely available.
2. **Validation is the primitive; generation is sugar over it.** The charter's
   bar has three clauses and the third one — *progressively lowering to full
   control* — is the one a generator alone cannot meet. A user who drops to the
   raw shell must not drop out of the checks at the same time, or the ladder's
   bottom rung is exactly today's silence. §5.6 argues this as the paper's
   central design claim.

Seam (a) is a smaller and more contained problem with a smaller and more
contained answer: `serve_service` is already `serve_connected` pre-wired with
two session hooks, `serve_connected` is already `Server::builder()` with three
routes and an upgrade handler, and `rpc_response` is already the composable
piece. Nothing needs to be invented. What is missing is that the composition is
written **inside** `rpc_server.vl` as three whole boot functions instead of
**on** `ServerBuilder` as one installable layer, so an app that grows must
throw its boot function away and adopt one of ours. §4.

## 1. How the survey was run

### 1.1 The sample

The primary sample is the charter's own originating evidence: the owner's
hand-built todo app at `vilan-playground/todo`, six files, 122 non-blank lines,
built 2026-08-10 — the app whose setup produced the charter. It is read-only
here and is not part of this repository.

Four corpus projects and two templates were inventoried alongside it, because
the charter's claim is that the ceremony is *universal*, and a claim about
universality is checked by counting every instance:

- `vilan/examples/todo` — the shape the owner's app was written from
- `vilan/examples/ssr` — the render-and-replace example (`ssr.md` §4, S2)
- `vilan/examples/fullstack` — the chunk-serving example (`bundle-splitting.md` §10)
- `vilan/examples/walkthrough` — the guide's end-to-end app
- `crates/vilan-cli/templates/fullstack` — what `vilan init` writes
- `crates/vilan-cli/templates/browser` — the single-leg template, for contrast

### 1.2 The counting rule

Stated so the ratio can be checked rather than believed. Every line count is
**non-blank, non-comment lines**. Each such line is classified as one of three:

- **INTENT** — a line that names something about *this application*: its
  service, its store, its views, its routes, its port, its title. A line that
  would still be written if setup cost nothing.
- **CEREMONY** — a line that exists only to move a build artifact from disk to
  a browser, or to describe a document, and that is byte-for-byte
  interchangeable between two unrelated applications. The test applied
  throughout: *could this line be copied verbatim into a different app and
  still be right?* If yes, it is ceremony.
- **DECLARATION** — `vilan.toml`. Counted separately and never folded into the
  ratio: a manifest entry is a real decision (which legs exist, which target),
  not restatement, and no design in this paper removes one.

Import lines are classified by what they import *for*: `import std::fs` in a
server that reads only `dist/` artifacts is ceremony; `import
std::json::json_codec` is intent (the codec is a deployment choice,
`transport-rpc.md` §6.2 Q6).

## 2. The inventory

### 2.1 The owner's todo app — the charter's own evidence

Six files, 122 non-blank lines, written 2026-08-10.

| File | lines | ceremony | intent |
|---|---:|---:|---:|
| `src/store.vl` — the `[service]` struct and its one method | 25 | 0 | 25 |
| `src/client.vl` — connect, subscribe, mount | 27 | 0 | 27 |
| `src/routes.vl` — the route enum and `Routable` | 33 | 0 | 33 |
| `src/server.vl` — **the boot function** | 19 | **10** | 9 |
| `src/app.html` — **the shell** | 13 | **12** | 1 |
| `vilan.toml` | 5 | *(declaration)* | |

`server.vl`'s ten ceremony lines are three boot reads (`:9-11`), the five-line
content-type table (`:16-20`), and the two imports that exist only to serve
them (`import std::fs`, `import std::http::Response`). Its nine intent lines
are four imports, `async fun main() {`, `let notes = Notes::new();`, the
`serve_service` call's head and tail, and the closing brace. `app.html`'s one
intent line is `<title>Todo</title>`.

**The headline: the service is 25 lines, the UI is 27, and joining them cost 32
lines of which 22 never mention a note.** 69% of the code between a working
service and a working page is setup.

One further fact about those 32 lines, and it is the strongest evidence in the
survey for the counting rule's *copy-verbatim* test:
`vilan-playground/todo/src/server.vl` is
`vilan/examples/walkthrough/src/server.vl` with the store swapped. The three
reads are identical, the five match arms are identical character for character,
the port is the same, and even the `on_start` message is the same string
("notes server listening on"); the only differences are `boot()` →
`Notes::new()`, the store import, and `std::print` spelled `std::io::print`.
The owner did not write this ceremony; the owner *transcribed* it, because none
of it is derivable from anything they knew about their own app.

### 2.2 The corpus

Four projects and two templates, same rule. Server entries only — see §2.3 for
why that is the whole story.

| Server entry | lines | ceremony | intent | boot shape |
|---|---:|---:|---:|---|
| `examples/fullstack/server/src/main.vl` | 56 | **52** | 4 | `Server::builder()` |
| `crates/vilan-cli/templates/fullstack/src/server.vl` | 25 | **22** | 3 | `Server::builder()` |
| `examples/ssr/src/server.vl` | 20 | **15** | 5 | `Server::builder()` |
| `examples/todo/src/server.vl` | 19 | **10** | 9 | `serve_service` |
| `examples/walkthrough/src/server.vl` | 19 | **10** | 9 | `serve_service` |
| *(the owner's)* `todo/src/server.vl` | 19 | **10** | 9 | `serve_service` |
| `examples/rpc/src/main.vl` — **in-process, no browser** | 198 | **0** | 198 | none (`local_rpc`) |

And the shells:

| Shell | lines | links a stylesheet | mount id | script |
|---|---:|---|---|---|
| `templates/fullstack/src/app.html` | 16 | yes, `/client.css` | `app` | `type="module"`, `/client.js` |
| `templates/browser/index.html` | 16 | yes, `app.css` *(relative)* | `app` | `type="module"`, `app.js` |
| `examples/todo/src/app.html` | 12 | yes, `/client.css` | `app` | `type="module"`, `/client.js` |
| `examples/walkthrough/src/app.html` | 12 | yes, `/client.css` | `app` | `type="module"`, `/client.js` |
| `examples/ssr/src/app.html` | 11 | **no** | `app` (+ `<!--ssr-->`) | `type="module"`, `/client.js` |
| `examples/fullstack/server/src/main.vl:82` | 1 *(inline string)* | **no** | `app` | `type="module"`, `/client.js` |
| *(the owner's)* `todo/src/app.html` | 13 | yes, `/client.css` | `app` | `type="module"`, `/client.js` |

Four observations, each of which the design in §5 is answerable to:

1. **The two worst ratios are the two files a new user meets first.** The
   `vilan init` scaffold's server is 22 ceremony lines and *one* line of
   intent — and that one line is an `import`. `examples/fullstack`'s server, the
   one the chunk-splitting docs point at, is 52 of 56. A reader learning the
   language from either file learns the ceremony as if it were the subject.
2. **Every shell agrees on everything, and nothing made them agree.** Seven
   shells, seven `id="app"`, seven `type="module"`. That unanimity is not a
   convention the tree enforces anywhere (§3.5 shows what it cost); it is seven
   authors copying the sixth.
3. **Two of the seven link no stylesheet.** `examples/ssr` and
   `examples/fullstack` do not, and for both it is currently correct — neither
   emits one. But "currently correct" is the entire hazard: add one `const
   style()` to either and the build starts emitting `dist/<leg>.css`, the shell
   keeps not linking it, and nothing anywhere says a word. That is the owner's
   bug, latent, in two examples in this repository.
4. **`examples/rpc` is the control, and it is clean.** 198 lines, zero
   ceremony: no `std::fs`, no `request.path()`, no `.html` file, no server boot
   at all. It wires client to server with `local_rpc` in-process. The tax is
   therefore not a property of rpc, or of `[service]`, or of the language —
   **it is precisely the price of having a browser**, and it is paid at exactly
   one seam.

### 2.3 The ratio, collected

Across the six browser-serving projects surveyed (the owner's app, `todo`,
`walkthrough`, `ssr`, `fullstack`, and the `init` template):

- **Client-leg files: 0% ceremony.** Every one, in every project. The client
  leg's only coupling to the setup is the string `"app"` inside
  `mount_root("app", …)`, which is one argument, not a line.
- **Store / service / view / route files: 0% ceremony.** Every one.
- **Server entries: 53%–93% ceremony**, and the two extremes are the two files
  most likely to be read as a model.
- **Shells: 92%–100% ceremony**, where the intent is a `<title>` and, in one
  case, a paragraph of prose (`examples/todo/src/app.html:9`).

So the tax is not diffuse and it is not proportional to app size — it is a
**fixed toll charged at the leg boundary**, roughly 30 lines and two file
formats, paid identically by a 122-line app and a 500-line one. That is why the
ratio looks mild in a big app and brutal in a small one, and it is why the
`vilan init` scaffold — the smallest app anyone ever sees — shows it at its
worst.

### 2.4 What does not exist, verified

Three negative findings, each checked by exhaustive grep, because the design
depends on them:

- **There is no document abstraction anywhere.** `doctype`, `<html`,
  `render_document`, `html_shell`, `render_to_string` occur in exactly ten
  places: eight hand-written `.html` files, one escaped string literal
  (`examples/fullstack/server/src/main.vl:82`), and zero occurrences in
  `vilan/std/`. The nearest surface is `std::ui::render(view: View): str`
  (`vilan/std/src/process/ui.vl:367-370`), whose own header says the caller
  "splices it into its HTML shell" — a **fragment** serializer by design
  (`ssr.md` §2, §6a).
- **There is no manifest surface for assets.** No `[assets]`, `[static]`,
  `[public]`, `[shell]` or `[html]` section exists or is parsed
  (`crates/vilan-core/src/manifest.rs:56`), and no `vilan.toml` in the tree
  names `app.html`, `dist/client.js` or `dist/client.css`. Those paths live
  only in `.vl` string literals and `.html` attributes.
- **The docs teach the ceremony rather than abstracting it.**
  `docs/guide/walkthrough.md:174` and `docs/guide/ssr.md:70` carry the same
  `fs::read_file_to_str("src/app.html")` line as the examples. That is honest
  documentation of what exists, and it is also how every new project inherits
  it.

And one finding about the *gate* rather than the code: the closest thing in
this repository to a specification of the HTML shell is a filename in a Rust
test array. `crates/vilan-cli/tests/init.rs:335` asserts `src/app.html` exists
in the scaffold and in each blessed example; the browser template's coupling is
checked by substring match (`tests/init.rs:141-162` —
`page.contains("href=\"app.css\"")` and two more), with the stated rationale
"Scaffolding a page that never loads the CSS the build writes is the failure
mode this asserts against". Someone already identified the owner's bug, and the
only instrument available to them was `str::contains` in a test that guards two
templates and nobody's project.

## 3. What the ceremony is made of — seven mechanisms

The inventory is a list of files. This section is the list of *mechanisms* they
share, because the design in §4–§6 is addressed at these and not at the line
counts. Each one is named, located, and given the failure it produces when it
is written wrong.

### 3.1 The boot-time read

```vilan
let client_js = fs::read_file_to_str("dist/client.js");
let client_css = fs::read_file_to_str("dist/client.css");
let app_html = fs::read_file_to_str("src/app.html");
```

Three lines, byte-identical in the owner's app
(`vilan-playground/todo/src/server.vl:9-11`) and in
`examples/walkthrough/src/server.vl:13-15`, and near-identical in
`examples/todo` and the `vilan init` template. They convert a *build fact* —
what this leg emitted — into a *runtime read* of a path spelled by hand, and
they do it once per process.

This mechanism is already the subject of a ratified-pending design:
`dev-refresh.md` §0 names exactly these call sites as the E55 defect, and §3
recommends the re-run-on-round hook as the one cure. **This paper does not
redesign that** (§7.3). What it adds is the observation that E55 is about the
*bytes* going stale, and there is a second, independent problem in the same
three lines: the *names*. `"dist/client.js"` is a string the user typed, and
nothing anywhere checks it against what the build wrote. Rename the leg in
`vilan.toml` from `client` to `web` and every one of these lines still
compiles, still runs, and fails at boot with `ENOENT` — or, worse, does not
fail at all, because a previous build's `dist/client.js` is still lying there.

### 3.2 The content-type table

```vilan
match request.path() {
    "/client.js" => Response::builder().set_header("Content-Type", "text/javascript").body(client_js).build(),
    "/client.css" => Response::builder().set_header("Content-Type", "text/css").body(client_css).build(),
    _ => Response::builder().set_header("Content-Type", "text/html").body(app_html).build(),
}
```

Five lines, and the single most-copied block in the language's corpus. Every
part of it is derivable: the URL from the artifact's filename, the MIME type
from its extension, the body from the read. Nothing in it is a decision the
application made.

### 3.3 The catch-all arm that answers everything

The `_ =>` arm is the quietest hazard in the shape. It answers **every**
unmatched path with the HTML shell, at status 200, with `Content-Type:
text/html`. That is deliberate for a client-routed SPA — deep links must reach
the shell — and it means that *a mistyped asset path is not a 404*. Request
`/client.cs`, `/dist/client.js`, or `/favicon.ico` and the server cheerfully
returns the whole document with a success code.

The consequence for the charter's bar is direct. A shell whose `<script src>`
does not match the route table does not produce a missing-file error; it
produces an HTML document served as JavaScript, which the browser refuses on
MIME grounds and reports as one line in a console the user is not looking at.
The page is blank and the server log is clean. This is the same failure
*class* as the owner's missing stylesheet link, and it is produced by the
mechanism that exists to make deep links work.

### 3.4 The shell's string contracts

The hand-authored document holds four contracts with three other files, and
not one of them is checked by anything:

| The shell says | It must agree with | Checked by |
|---|---|---|
| `href="/client.css"` | the server's route table **and** whether the build emitted a stylesheet at all | nothing |
| `src="/client.js"` | the server's route table **and** the leg's output name | nothing |
| `<div id="app">` | `mount_root("app", …)` in the client leg | nothing |
| `<!--ssr-->` | the server's `shell.replace("<!--ssr-->", …)` | nothing |

The examples suite has exactly one guard in this territory, and it is a lint
over the corpus rather than a language mechanism: `ui-styling.md` §0bis records
that the examples gate grew "a GENERAL rule — every emitted stylesheet must be
linked by one of the example's pages". It is a good rule and it protects
`vilan/examples`. It protects no user's project, and it is the direct ancestor
of this paper: it exists because `examples/reactive-ui` "emitted `app.css`
(pinned in `tests/examples.rs`) while its `index.html` never loaded it — const
styles compiled and then thrown away, the sharpest evidence the hookup was
unfinished" (`ui-styling.md` §0bis). The owner then shipped the same bug in
their own app, which is what the charter's *LOUD when wrong* clause is
describing.

The fourth row deserves its own note, because it is SSR's whole value
proposition riding on a string literal written twice. `ssr.md` §4 (S2) records
the splice as `shell.replace("<!--ssr-->", render(app()))`, and
`str::replace` (`vilan/std/src/string.vl:21`) with no match returns the
original string — so a misspelled or absent marker degrades SSR to *serving the
bare shell*: first paint empty, crawler sees nothing, and the client boot then
renders the page correctly, so the app looks fine to the developer testing it
in a browser. The one observer who would notice is the crawler the feature
exists for.

### 3.5 The script tag nobody chose

There are **twelve** HTML shells in and around this tree — seven examples, two
templates, the guide's `ssr.md` page, `examples/fullstack`'s inline string
(`vilan/examples/fullstack/server/src/main.vl:82`), and the owner's app — and
every one loads the bundle with `<script type="module">`.
`bundle-splitting.md` §8 ratified the chunk base resolution on the opposite
assumption:

> `import()` resolves against `document.currentScript.src`, because a classic
> script's relative specifier resolves against the DOCUMENT's URL — the route
> the user is standing on — and would miss on every nested path.

`document.currentScript` is `null` inside a module script, by specification.
The emitted helper guards for it and falls back to `base = ""`
(`crates/vilan-core/src/transformer.rs:765-767`), so under every shell this
tree ships the chunk `import()` resolves against the document URL —
**precisely the miss the design set out to avoid**. It has never been observed
because no example declares `split = true` (`bundle-splitting.md` §9 measured
that none should) and the split fixture runs under Node, where `document` is
absent and relative resolution is already correct. So far as this survey can
tell, the branch has never executed.

Filed as bycatch (§9.2) — it is a splitting bug, not a setup one. It is
*reported here* because it is the cleanest demonstration of the thesis: the
compiler decided how the bundle must be loaded, the shell is where that
decision has to be written down, there is no channel between them, and so all
twelve shells — several of them written alongside the splitting code itself —
say the other thing.

### 3.6 The manifest the server re-derives

`bundle-splitting.md` §3 gave the split leg a sidecar so a server would not
have to hard-code a route per chunk, and `dev-loop.md` § "Serving the chunks"
documents the consequence honestly:

> A chunk is fetched from the same directory the bundle was served from, so a
> static host needs nothing: serve `dist/` and you are done. A hand-written
> server iterates `dist/client.chunks.json` instead of hard-coding a route per
> file.

Read that first clause again. **The entire ceremony of §3.1–§3.3 exists
because there is no static host.** A user with nginx in front of `dist/` writes
none of it. A user with `vilan run` writes all of it, every time, in a language
whose toolchain built the directory it is failing to serve.

The manifest itself has a second problem for anything that wants to consume it
generally: it is written **only when the leg splits**, and swept when it stops
(`bundle-splitting.md` §9 — "dropping `split` takes the manifest with it (a
manifest outliving its chunks is one that LIES)"). So the one artifact that
describes a leg's output is absent for every leg in this repository, since none
of them split. §5.4 proposes extending it; §10.3 puts the cost to the owner.

### 3.7 The boot function that must be replaced to grow

The last mechanism is seam (a), and it is a shape rather than a snippet.
`std::rpc_server` offers three whole `main` bodies:

```vilan
fun serve_rpc(port: i32, protocol: RpcProtocol, on_ready: |Server| void)
fun serve_service(port: i32, protocol: RpcProtocol, fallback: |Request| Response, on_ready: |Server| void)
fun serve_connected(port: i32, protocol: RpcProtocol, on_connect: |i32, DuplexEnd| void, on_disconnect: |i32| void, fallback: |Request| Response, on_ready: |Server| void)
```

and each of them ends in `.build().start()` on a `Server::builder()` chain it
constructed privately (`rpc_server.vl:70-77`, `:84-91`, `:122-145`). The
builder is public; the composition is not. So the growth path of a real app is
a sequence of **rewrites**, not additions:

| The app wants | Today it must |
|---|---|
| serve a page | `Server::builder()…on_request(…)` — the minimal form (`examples/ssr` uses it bare) |
| …and add rpc | **delete that**, adopt `serve_service`, and re-express its page handler as the `fallback` argument |
| …and add a custom session identity | **delete that**, adopt `serve_connected`, and hand-write `register_session`/`drop_session` back in |
| …and add a second service | there is no form; `ServerBuilder.upgrade_handler` is one `Option`, and `accept_socket` ignores the request path entirely (`rpc_server.vl:215-231`) |

Each row throws away the previous row's boot function. That is the charter's
"the API changes shape exactly when the app grows", stated as a table.
`rpc_response` (`rpc_server.vl:60-66`) is the honourable exception — a
composable `(protocol, request) -> Response` whose doc comment explicitly says
"Compose this into a `Server`'s `on_request` alongside other routes" — and it
is the piece §4 generalizes.

## 4. Seam (a) — a server that grows

### 4.1 What exists, precisely

Everything the layer needs is already written and shipped; it is written in the
wrong place. `serve_connected` (`rpc_server.vl:122-145`) is:

- `Server::builder().port(port)`
- `.on_request(|request| connected_response(protocol, on_connect, on_disconnect, fallback, request))` — a router that answers `/events`, `/send`, `/rpc` and delegates everything else to `fallback` (`:148-207`)
- `.on_upgrade(|request, socket, head| accept_socket(…))` — the WebSocket half of the same contract (`:215-315`)
- `.on_start(on_ready)`, `.build()`, `.start()`

and `serve_service` (`:84-91`) is that call with `register_session` /
`drop_session` supplied for the two lifecycle hooks — `transport-rpc.md` §4.2
records exactly this: "`serve_service(port, protocol, fallback, on_ready)` is
`serve_connected` with that registry as its connection lifecycle".

So the layer is not new machinery. It is **the same four builder calls, moved
from inside a function onto the builder**, where a user's own calls can sit
beside them.

### 4.2 The layer

```vilan
/// One rpc service mounted on a `Server`: the protocol, where its routes
/// live, and what happens to a connection's per-connection state. The routes
/// (`{mount}events`, `{mount}send`, `{mount}rpc`) and the upgrade handshake
/// are installed together, because a duplex service is both.
struct Service {
    protocol: RpcProtocol,
    mount: str,
    on_connect: |i32, DuplexEnd| void,
    on_disconnect: |i32| void,
}

impl Service {
    /// The `Client::connect` service (`transport-rpc.md` §4.2): the runtime
    /// session registry as the connection lifecycle — what `serve_service`
    /// installs today.
    fun new(protocol: RpcProtocol): Service

    /// Mount under `prefix` instead of `/`. A prefix is what lets two services
    /// coexist, and what `Client::connect(url, codec)` already supplies.
    fun at(own self, prefix: str): Service

    /// Replace the connection lifecycle — the pair `serve_connected` takes
    /// today, for apps holding custom per-connection state.
    fun on_connect(own self, handler: |i32, DuplexEnd| void): Service
    fun on_disconnect(own self, handler: |i32| void): Service
}

impl ServerBuilder {
    /// Install an rpc service. Repeatable. Its routes answer before
    /// `on_request`, independently of the order these calls were written (§4.3).
    fun with_service(own self, service: Service): ServerBuilder
}
```

`ServerBuilder` gains one field, `services: List<Service>`, and `build()` gains
two responsibilities: fold the services' routing in front of the user's
`request_handler`, and fold their handshakes into the single
`upgrade_handler`. Both folds are pure functions of the field, so `Server`
itself is unchanged in kind — it still holds one request handler and one
optional upgrade handler, and `start()` (`http.vl:290-332`) is untouched.

The owner's `main` becomes:

```vilan
async fun main() {
    let notes = Notes::new();
    Server::builder()
        .port(4600)
        .with_service(Service::new(notes.dispatcher().into_protocol(json_codec())))
        .on_request(|request| page(request))
        .on_start(|server| print(i"notes server listening on {server.url()}"))
        .build()
        .start()
}
```

and — the point of the whole exercise — deleting the `.with_service` line
leaves a program that still compiles and still serves the page. That is the
property `serve_service` cannot have.

### 4.3 Ordering, and most-specific-wins

The charter asks what happens when a protocol route and `on_request` could both
answer. Three candidate rules; the recommendation is the one that already
ships. **(1) Services first, then `on_request`, always** — which is literally
`connected_response` (`rpc_server.vl:148-207`): three `if path.starts_with(…)`
arms, then `fallback(request)`. **Recommended**: it preserves every existing
program's behaviour byte for byte, and it is the rule a reader guesses.
**(2) Declaration order** — rejected: it makes a builder chain's *order*
semantic, which no other `ServerBuilder` method does, and it changes behaviour
silently when someone reorders lines for readability. **(3) Longest match
across a merged route table** — rejected as v1 scope: there is no route table,
`on_request` is one opaque closure, and there is nothing to compare its
specificity against.

Between services, rule 1 needs a tiebreak: **longest mount prefix first**,
computed at `build()` and independent of call order, so services at `/` and
`/admin/` behave as a reader expects however they were written.

**One behavior change is recommended inside rule 1, and it is a fix.** The
matching today is `path.starts_with("/rpc")`, so a service mounted at `/`
swallows `/rpcs`, `/sendmail`, and `/events-archive` — an application route can
be shadowed by a prefix collision it has no way to see. The layer should match
a mount's routes on the **path segment**, terminated by end-of-path or `?`, so
`/rpc` and `/rpc?x=1` hit the service and `/rpcs` reaches `on_request`. This
changes behavior for any program that today has a route beginning with those
three strings — a set this survey believes is empty, and which a corpus check
would settle before the slice lands.

### 4.4 Multiple services

Two constraints make this the sharpest part of the design, and both are facts
about the code rather than choices:

- **The upgrade handler is one `Option`** (`http.vl:255`, `:265`), and
  `accept_socket` never reads `request.url()` — a WebSocket upgrade today is
  answered identically whatever path it arrived on (`rpc_server.vl:215-231`).
  So a second duplex service is not merely unwired; it is unaddressable.
- **The client already supplies the address.** `Client::connect(url, codec)`
  takes a URL (`transport-rpc.md` §4.2), which `dial_for_service` hands to
  `connect_socket` (`rpc.vl:1300`), and `connect_split(base_url)` builds
  `{base}/events` and `{base}/send?c=<id>` from it (`rpc.vl:217`, `:266`). The
  owner's app passes `"/"`. Nothing has to be invented on the client side for a
  mount prefix to work — the parameter is there and is already threaded.

So: `build()` installs **one** upgrade handler that inspects the upgrade
request's path, picks the matching service by longest mount, and calls that
service's `accept_socket`. An upgrade on a path no service claims falls to the
user's own `.on_upgrade` if they installed one, and is destroyed otherwise —
which is what happens today to a handshake with no `sec-websocket-key`
(`rpc_server.vl:224-227`).

Two honest limits, recorded rather than solved. `connections` and
`next_connection` are **module-level** in `rpc_server.vl` (`:97`, `:100`) —
"one counter for the program" — so two services share one connection-id space;
harmless (ids stay unique, which is all they are for), and worth stating
because a reader will assume per-service numbering. And `register_session`
writes into `std::rpc`'s single `reactive_sessions` list (`rpc.vl:1006`), keyed
by connection id alone: it works, because the id is global, but the registry is
now doing something it was not named for. §10.5 puts that to the owner.

### 4.5 What generalizes beyond rpc — and what deliberately does not

**Generalizes: claiming paths.** `with_service` is a special case of "a library
component installs some routes and an upgrade handler onto a builder, in front
of the app's own handler". The obvious second customer is §5's asset serving,
and this paper proposes exactly that shape for it (`serve_assets`, §5.4) rather
than a second bespoke mechanism. If a third arrives, the generalization is a
`Mount` — a value with a prefix, a request handler, and an optional upgrade
handler — with `with_service` and `serve_assets` becoming constructors of it.
**Recommendation: do not build `Mount` in this arc.** Two customers is the
number at which a shared shape is a guess; the second one is being designed in
the same paper as the first, which is precisely when the temptation to
generalize is least trustworthy. Record it, build it when a third customer
exists and disagrees with the shape.

**Does not generalize: middleware.** A layered `.with_layer(|request, next| …)`
stack is the obvious next ask, and this paper recommends **declining it**, with
reasons rather than silence. (1) It already exists and needs no surface:
`on_request` is one closure, so a user who wants logging writes
`.on_request(|request| log_around(request, |r| routes(r)))` — a middleware API
would be a naming convention for something the language already expresses.
(2) `on_request` is `async |Request| Response` (`http.vl:347`), so a `next`
continuation is an async closure passed to an async closure: legal (J2's typed
channel), but it multiplies the async-coloring surface at a seam where every
existing user program is synchronous, for no capability gain. (3) There is no
demand evidence — the charter names a server that grows *into rpc*, not one
that grows a request pipeline, and the survey's one real app wants zero
middleware.

Declining is a decision, not an omission: if it is wanted later, `Mount` above
is the shape it should be built on, so that "a service" and "a layer" are one
concept and not two.

### 4.6 `serve_service` stays, as sugar

```vilan
fun serve_service(port: i32, protocol: RpcProtocol, fallback: |Request| Response, on_ready: |Server| void) {
    Server::builder()
        .port(port)
        .with_service(Service::new(protocol))
        .on_request(fallback)
        .on_start(on_ready)
        .build()
        .start()
}
```

All three `serve_*` functions reduce to four-line bodies over the layer and
**keep their signatures**, so `examples/todo`, `examples/walkthrough`, the
benchmarks, `docs/guide/services.md`, `docs/guide/routing.md`,
`docs/guide/persistence.md` and every e2e in `crates/vilan-cli/tests/` compile
and behave unchanged. That is the migration story: there isn't one.

Nor does anything else move. `Server`'s shape is the same (one request handler,
one optional upgrade handler, `start()` untouched); no frame, route name,
handshake or session semantic changes, so a service mounted at `/` is
byte-identical on the wire to today's `serve_service` — which is what makes the
existing corpus and e2e suites the gate; and server-side HMR remains a
permanent non-goal (`hmr.md` §8, §7.2 below).

## 5. Seam (b) — the document

### 5.1 The bar, restated as failure modes

The charter's bar is three clauses: *easy to set up, LOUD when wrong,
progressively lowering to full control*. The middle clause is the one that
decides the design, so it is restated here as the list of things that are wrong
today and silent. Each is real: each was either shipped by the owner, is
present in this repository, or is one edit away in a file this repository
ships.

| | Failure | Today's symptom | Where it lives |
|---|---|---|---|
| **F1** | The build emitted styles; the shell links none | none at all — the page renders unstyled and correct-looking | the owner's shipped bug; latent in `examples/ssr` and `examples/fullstack` (§2.2) |
| **F2** | The shell links a stylesheet the build did not emit | the `_ =>` arm answers the `.css` request with the HTML document, 200 (§3.3); the browser drops it silently | any project that deletes its last `const style()` |
| **F3** | The shell's `<script src>` misses the route table | HTML served as JavaScript; MIME refusal; blank page; one console line | any rename of a leg |
| **F4** | `<div id>` and `mount_root(id)` disagree | `Cannot read properties of null` in `element.clear()` (`browser/ui.vl:665-668`) | any project |
| **F5** | The SSR marker is missing or misspelled | `str::replace` no-ops; the shell serves empty; the client then renders it correctly, so only a crawler ever sees the bug | `examples/ssr`, and every SSR app written from it |
| **F6** | A leg splits and the shell uses `type="module"` | chunk `import()` resolves against the document URL; nested routes 404 | every shell in the tree — latent, since no leg splits today (§3.5) |
| **F7** | The build emits a new artifact the server has no route for | 404, or the `_ =>` arm's HTML-as-anything | any leg that gains `split = true` |
| **F8** | The bytes on disk moved after the server read them | stale asset served forever | `dev-refresh.md` §0 — E55, not re-solved here |

F1–F7 share one root: **a fact the build knew was restated by hand somewhere
else, and nothing compared the two copies.** That is what the design has to
close, and it closes all seven the same way.

### 5.2 What the build knows, and what the server can ask

The build's knowledge, at the moment `write_assets` and `write_chunks` run
(`crates/vilan-cli/src/main.rs:2265`, `:2291`):

- the leg's name and its bundle filename (`dist/<leg>.js`) —
  `crates/vilan-cli/src/main.rs:1523`;
- **whether a stylesheet was emitted at all**, because `write_assets` iterates
  `assemble_assets` and writes one file per asset kind actually collected — a
  program with no `const style()` produces no `css` entry and therefore no
  file;
- every chunk it wrote, with its arm and tag, which it serializes into
  `dist/<leg>.chunks.json` (`main.rs:2330-2345`);
- how the bundle must be loaded, since it emitted the `__chunk_registry`
  base resolution itself (`transformer.rs:760-772`).

What the server can ask today, exhaustively: `fs::exists(path)` and
`fs::read_file_to_str(path)` — that is the whole of `std::fs` for reading
(`vilan/std/src/process/fs.vl`, twenty lines, four declarations). Both take a
path the user typed. There is no third thing.

So the design's first move is not an abstraction, it is a **channel**: a value
naming what a leg's build produced, reachable from the server leg.

```vilan
/// What one browser leg's build emitted, as the build itself knows it.
struct LegBuild {
    /// The leg's name — `client` for `[entry.client]`.
    leg: str,
    /// The eager bundle's filename, e.g. `client.js`.
    bundle: str,
    /// The style sidecar's filename, when the build emitted one. `None` means
    /// the leg compiled no styles — a fact a shell must respect and cannot
    /// see today (F1, F2).
    styles: Option<str>,
    /// The route chunks, in the build's own order. Empty for a leg that does
    /// not `split` (`bundle-splitting.md` §4).
    chunks: List<str>,
    /// Whether the bundle must be loaded as a classic script — true exactly
    /// when the leg splits, because chunk resolution reads
    /// `document.currentScript` (§3.5).
    classic_script: bool,
}

/// The description of `leg`'s most recent build.
fun build_of(leg: str): Result<LegBuild, BuildError>
```

Where the value comes from is the paper's largest cost and is put to the owner
as §10.2. The recommendation is the cheap one for v1 — read an extended,
always-written `dist/<leg>.chunks.json` through E55's freshness hook — with a
compiler-minted constant recorded as the end-state. §5.9 and §10.3 carry the
detail.

One honest note on the last field: `classic_script` exists because today's
chunk resolution reads `document.currentScript`. If §9.2's real fix lands and
the emitter resolves against `import.meta.url` instead, the field is always
`false` and should be removed rather than kept — F6 disappears with it. It is
in the sketch because the description must describe the emitter that exists,
not the one that ought to.

### 5.3 The ladder

Three rungs and a validator. Each is independently adoptable; none requires the
one above it; the bottom one is what exists today and is not going anywhere.

| Rung | The server says | What it stops writing |
|---|---|---|
| **0** | today's code, unchanged | nothing — the escape hatch |
| **0+** | `check_shell(shell, build, "app")!` | *nothing* — it stops being silent (§5.6) |
| **1** | `.serve_build(build_of("client")!)` | the reads, the content-type table, the chunk plumbing |
| **2** | `Document::of(build).title("Todo").html()` | `app.html` itself |

### 5.4 Rung 1 — the served build

```vilan
impl ServerBuilder {
    /// Serve one leg's build output: one route per artifact at `/<filename>`,
    /// with the content type its extension implies, read through the dev
    /// freshness hook (`dev-refresh.md` §3). Installed like a service (§4.3):
    /// these routes answer before `on_request`, so the app's own catch-all
    /// still gets every path they do not claim.
    fun serve_build(own self, build: LegBuild): ServerBuilder
}
```

This is `with_service`'s sibling and deliberately the same shape (§4.5): a
library value claiming a set of paths in front of the app's handler. It is
where F7 goes to die — a leg that gains `split = true` gains its chunk routes
with no server edit, which is what `bundle-splitting.md` §3 wanted the sidecar
for in the first place — and it is what `dev-loop.md` § "Serving the chunks"
is describing when it says a static host "needs nothing".

What it deletes, measured against §2.2: `examples/fullstack`'s server loses its
two reads, its seven-line route match, and all 25 lines of `ChunkFile`,
`route_chunks` and `find_chunk` — 34 of its 52 ceremony lines, replaced by one.
`examples/todo`'s loses 8 of 10.

Three details that are decisions, not defaults. **Route shape** is
`/<filename>`, so `dist/client.js` serves at `/client.js` — what every shell in
the tree already asks for, so no shell changes; a `.at("/static/")` prefix is
the obvious extension and is deliberately *not* v1, because the moment the
prefix is configurable the shell must be told about it, and a second string
contract is what this paper is removing (rung 2's `Document` could carry it and
keep both in sync, which is the argument for adding it later, on top of rung 2).
**Content types** come from the extension via a short fixed table in std, not a
user-facing map; anything not in the table is not served, because `serve_build`
serves *the build*, not a directory (§5.10). **Missing artifacts are loud**: if
`build_of` names a file that is not on disk, that is a broken build, reported at
boot naming the file and the leg rather than 404ing at request time.

### 5.5 Rung 2 — the document

```vilan
/// The HTML document a browser leg is loaded by, built from what that leg's
/// build emitted — so the `<script>`, the `<link>` and the mount element
/// cannot disagree with the artifacts they name.
struct Document { … }

impl Document {
    /// The default document for a build: doctype, `<html lang>`, charset,
    /// viewport, `<title>`, the stylesheet link IF AND ONLY IF the build
    /// emitted styles, the mount element, and the bundle's script tag in the
    /// form the build requires (§3.5).
    fun of(build: LegBuild): Document

    fun title(own self, title: str): Document
    fun lang(own self, lang: str): Document
    /// The mount element's id — the other end of `mount_root(id, …)`.
    /// Defaults to `"app"`, which is what all seven shells in the tree use.
    fun mount(own self, id: str): Document
    /// Raw markup appended inside `<head>`: a favicon, an og: tag, a CSP,
    /// an inline `<style>` for the page frame (which two templates want).
    fun head(own self, markup: str): Document
    /// Raw markup appended inside `<body>`, before the script tag.
    fun body(own self, markup: str): Document
    /// Server-rendered markup for the mount element (`ssr.md` §1) — the
    /// splice, with no marker string in it. §5.8.
    fun render(own self, view: View): Document

    fun html(self): str
}
```

`Document` is a **string builder, not a `View`**, and that is a decision worth
stating because the opposite is the obvious guess. A `View`-shaped document
would mean the process-layer `ui` grows `<html>`/`<head>`/`<body>` semantics
and a document-level mount — and `ssr.md` §6(a) ratified that layer as
*fragment-only*, with `mount`/`mount_root` deliberately omitted. It reopens
that call for no gain: a document is not a reactive tree, nothing binds to it,
and it is serialized once per request. **Declining is a decision, not an
omission.** `Document` composes with `View` at exactly one point,
`render(view)`.

Two smaller calls, made explicitly. **The default document is opinionated and
small** — doctype, `<html lang>`, charset, viewport, `<title>`, the conditional
`<link>`, the mount element, the script tag: the intersection of the seven
shells in §2.2, each of which is reconstructible from it plus `head`/`body`
(the `<style>` page-frame block two templates carry is `head()`'s first
customer). And **`html()` returns a `str`, not a `Response`** — the app decides
the status code, the headers and which paths get it. A `Document` that knew
about HTTP would have to know about routing, and then it is a framework.

### 5.6 Generation versus validation — the paper's central claim

The charter asks for a ladder "progressively lowering to full control". The
obvious reading is that the rungs are *generation* — more generated at the top,
less at the bottom — and that the bottom rung is the raw shell. That reading
produces a design that fails the charter's own middle clause, and the reason is
worth stating carefully.

A generator only protects documents it generated. The user who most needs F1–F7
caught is exactly the user who dropped to the raw shell — because they wanted a
CSP header, or a font preload, or an analytics snippet, or a `<base>` tag — and
under a generation-only ladder, stepping down one rung steps out of every check
at the same time. That is today's behavior, relabelled as a feature.

**So: validation is the primitive, and generation is sugar over it.**

```vilan
enum ShellFault {
    /// The build emitted styles and the document links no stylesheet. (F1)
    StylesNotLinked(str),
    /// The document links a stylesheet this build did not emit. (F2)
    LinkedStyleMissing(str),
    /// The document loads a script this build did not emit. (F3)
    ScriptNotEmitted(str),
    /// This build's bundle is loaded by no `<script>` in the document. (F3)
    BundleNotLoaded(str),
    /// No element carries the id the client mounts into. (F4)
    MountMissing(str),
    /// The leg splits, and its bundle is loaded as a module script, so chunk
    /// resolution will miss on every nested route. (F6)
    ModuleScriptWithChunks(str),
}

/// Check a hand-authored shell against a leg's build. Every fault, not the
/// first — a shell with two problems should report two.
fun check_shell(shell: str, build: LegBuild, mount: str): Result<void, List<ShellFault>>

impl Document {
    /// A hand-authored shell, checked. The rung-0 escape hatch, made safe:
    /// the same `Document` value, its markup supplied rather than generated.
    fun from_shell(shell: str, build: LegBuild): Result<Document, List<ShellFault>>
}
```

`Document::of` is then **`from_shell` over markup it wrote itself**, and its
guarantee is that the check it would run cannot fail. One rule set, one
implementation, two entry points — which is also what keeps the generator and
the checker from drifting, the way `ssr.md` §4's differential pin keeps the two
`ui` implementations from drifting. The same instrument applies: *every
document `Document::of` can produce passes `check_shell`* is a property test,
and it is the gate this slice owes.

**How loud is loud?** `check_shell` returns a `Result`, so an application can
decide; the sugar every template uses is `!`, so a broken document **stops the
server from starting** with a message naming the fault, the file and the fix.
The owner's bug would have read:

```
error: src/app.html links no stylesheet, but the `client` build emitted
       dist/client.css
  note: add `<link rel="stylesheet" href="/client.css" />` inside <head>
  note: or call `.styles(Ignored)` if the page loads its styles another way
```

Refusing to boot is defensible precisely because the check is cheap, total, and
about the *build*, not about the request: it cannot fail intermittently, and a
server that starts with a document that cannot work is worse than one that does
not start. An app that genuinely means it says so, once, in code.

### 5.7 Rung 0 — the escape hatch, kept

Nothing above
removes the ability to write `fs::read_file_to_str("src/app.html")` and a
`match request.path()`; rung 0 is not deprecated, not warned about, and not
scheduled for removal. The design owes it three things: `serve_build` is
*additive* on the builder, so an app can serve the build and still answer
`/legacy.js` itself; `check_shell` takes a `str`, so it works on a shell
produced any way at all — read from disk, templated, fetched from a CMS; and
`Document::html()` returns a `str`, so a generated document can be
post-processed with the same string operations an app uses today. The escape
hatch is only credible if the rungs above it are made of the same material,
which is why every piece of this design is a plain value: a description, a
string, a `Result`.

### 5.8 SSR — the marker, and what replaces it

`ssr.md` §4 (S2) shipped the splice as user code:
`shell.replace("<!--ssr-->", render(app()))`. That is F5: a string literal in
`.vl` that must equal a string literal in `.html`, whose mismatch is a silent
no-op, whose only observer is a crawler.

`Document::render(view)` removes the marker rather than checking it. The
document already knows where the mount element is — it is the same `mount(id)`
the client attaches to — so server-rendered markup goes *inside that element*
by construction, and there is nothing to spell wrong:

```vilan
Document::of(build).title("Notes").render(app()).html()
```

For rung 0, `from_shell` still needs a marker (the shell was hand-authored and
the document cannot find the mount element's contents without one), so
`ShellFault` gains no marker case and `Document::from_shell(...).render(view)`
splices into the *element the check already located by id* — the mount element
— rather than into a comment. That means **the `<!--ssr-->` convention becomes
unnecessary at every rung**, and `examples/ssr` loses its marker.

§7.1 handles the ratified-decline reconciliation; the short version is that
`ssr.md` §6(b) declined `render_into(shell, marker, view)` because it was new
surface for a one-line string replace, and this is a method on a value the
charter requires to exist for entirely different reasons.

### 5.9 Chunks, styles, and freshness — the three interactions

**Chunks.** `bundle-splitting.md` §3's sidecar is exactly the description this
design needs, and it is written **only when the leg splits**; §9 ratified the
sweep that removes it when `split` is dropped, on the grounds that "a manifest
outliving its chunks is one that LIES". The recommendation is to **extend the
sidecar into the leg's build manifest and write it on every build of the leg,
chunks or none** — keeping the filename, adding `styles` and `classic_script`,
and leaving `"chunks": []` when there are none. This does not weaken §9's
invariant: the invariant is *the leg's last build owns the file*, which the
sweep enforces and which an always-written manifest enforces more strongly (a
present-but-empty chunk list is a positive statement; an absent file is an
ambiguity between "did not split" and "was never built"). It does churn a
byte-pinned golden and reverse one ratified line, so it is §10.3, the owner's.

**Styles.** `ui-styling.md` §4 ratified "browser builds emit `<out>.css`; the
html host links it", and §0bis records the template's `fs::exists` guard as the
state of the art. `LegBuild.styles: Option<str>` is that guard answered by the
build instead of by a filesystem probe, and F1/F2 are the two directions the
probe cannot check. The `<link>` idiom is preserved unchanged, which matters:
`hmr.md` §2 and its 2026-08-10 appendix both depend on the stylesheet being a
findable `<link>` whose `href` ends in the sidecar's name, superseded on a css
push (`link.disabled = true`) rather than replaced, so "a plain page reload
starts clean from a freshly parsed `app.html`". A generated document therefore
emits a real `<link>` carrying the sidecar's filename, and never inlines
styles.

**Freshness.** `dev-refresh.md` §3 recommends the re-run-on-round hook as the
one mechanism and `fs`-specific sugar over it. This paper **does not redesign
that and does not need to**, because the two concerns separate cleanly:

- the **description** (`LegBuild`) is a build fact — it changes only when the
  build changes, which under `run --watch` means the server restarted anyway
  for a code change, or did not need to for a css-only round;
- the **bytes** are what go stale, and `serve_build` is the single place they
  are read, so it is the single call site E55's hook has to reach. That is
  strictly better than today's three-reads-in-`main`, and it is the
  restructuring `dev-refresh.md` §2(i) says the revalidating read needs: "a
  primitive in search of a call site that invokes it more than once".

`dev-refresh.md` §2(iv) hands this paper the template question explicitly ("It
is **not** the larger question backlog item 56 opens … that charter is
explicitly out of scope here and is its own design note"). The handoff is
returned in kind: E55 ships the hook; `serve_build` is its first and best
customer; neither blocks the other, and if E55 ships first the template edit is
mechanical, exactly as §2(iv) predicts.

### 5.10 What this is not: a static file server, and the single-leg gap

`serve_build` serves **a build**, not a directory. It will not serve a favicon,
an image or a `robots.txt`, deliberately — a directory server has traversal,
MIME and caching surfaces, none of them E56's subject. It is also impossible
today: `std::fs` cannot read a binary file at all (`read_file_bytes(path,
encoding): str` is the only read and it returns a string,
`vilan/std/src/process/fs.vl:5-6`), so no vilan program can serve a PNG.
Bycatch, §9.3.

`hmr.md` §9 records the adjacent gap: "grow the dev channel's static serving
into a tiny dev server (`index.html` + bundle) so `run --watch` works without a
Node leg". Rung 2 is half of that — a browser-only project has no server leg to
call `Document::of` from, but the CLI holds the same `LegBuild` information.
**Recommendation: do not build it in this arc, and note the alignment**: if
`Document` lands as a plain value over a plain description, §9's item shrinks
from "design a dev server's HTML" to "serve `Document::of(build).html()`",
CLI-side. Recorded so the two are not designed twice.

## 6. Seam (c) — what `vilan init` becomes

### 6.1 init's own constraints, read out of the code

`vilan init` is not a free surface, and the charter is right to say so. Four
constraints bind any change here, all verified:

1. **Templates are `include_str!`-embedded** from
   `crates/vilan-cli/templates/<template>/`, as a static table of
   `(destination, contents)` pairs (`crates/vilan-cli/src/init.rs:86-144`; the
   fullstack arm lists exactly six files at `:117-142`). An installed binary
   carries its scaffolds and never looks for a templates directory
   (`init.rs:17-29`).
2. **One substitution token, `{{name}}`** (`init.rs:54`, `:233-235`). "Nothing
   else in a template is templated; a scaffold is a file you can read."
   Whatever the template becomes, it stays a readable file, not a generator.
3. **`every_template_scaffolds_exactly_its_embedded_files_already_formatted`**
   (`crates/vilan-cli/tests/init.rs:356-387`) — the scaffold's file set must
   equal the directory's, `vilan fmt --check .` must pass on a fresh scaffold,
   and no `{{name}}` may survive.
4. **The field-by-field manifest gate**
   (`the_fullstack_template_matches_the_blessed_example_layout`,
   `tests/init.rs:275-346`). Against each of the three blessed examples —
   `walkthrough`, `todo`, `ssr` — it asserts the scaffold's `[package] root`,
   `entry` and `target`; that the blessed shape is one package (no `[project]`,
   no `[library]`); that the **entry name lists are equal and in the same
   order**; and per entry, `target`, `path` and `split`.

Constraint 4 has a fifth clause that turns out to be the binding one for seam
(b), and it is worth quoting because nothing else in the tree says it:

```rust
// tests/init.rs:334-344 — ...and the layout the manifest implies is on disk in both.
for relative in ["src/client.vl", "src/server.vl", "src/app.html"] {
    assert!(project.join(relative).is_file(), "the scaffold is missing {relative}");
    assert!(directory.join(relative).is_file(), "{example} is missing {relative}");
}
```

**`src/app.html` is a pinned member of the blessed full-stack shape, in the
scaffold and in three examples simultaneously.** So the template cannot lose
its shell alone: rung 2 in the scaffold is a change to `walkthrough`, `todo`,
`ssr` and this test in one commit. That is the gate working exactly as designed
— it exists to keep the scaffold and the corpus the same shape — and it is the
reason §6.3 recommends what it does.

Note also what the gate does *not* cover: it compares manifests and three
filenames, and says nothing about the contents of `server.vl`. The ceremony of
§3.1–§3.3 is replicated between scaffold and examples by convention, not by
test — which is how the scaffold came to carry the stale-read pattern
`dev-refresh.md` §2(iv) names.

### 6.2 The template at each rung

Today, `templates/fullstack/src/server.vl` is 25 counted lines of which 22 are
ceremony (§2.2): three boot reads including the five-line `fs::exists` guard for
the stylesheet, a six-line route table, and the three imports that serve them,
around a `Server::builder()` chain whose only application content is
`greeting()`.

**With seam (a) only** — nothing changes. The template already uses the
builder, which is the good side of (a); what (a) buys it is that the *next*
step is additive. One comment line can now say something true:
`// add rpc later with .with_service(…) — nothing here has to move.`
That sentence is the charter's "a server that grows", and it costs one line of
the opening argument.

**At rung 1** — the three reads, the `fs::exists` guard and the route table go;
the two ceremony imports go with them:

```vilan
let build = build_of("client")!;
let shell = fs::read_file_to_str("src/app.html");
Server::builder()
    .port(8080)
    .serve_build(build)
    .on_request(|request| Response::builder().set_header("Content-Type", "text/html").body(shell).build())
    .on_start(|server| print(greeting() + " — http://localhost:8080/"))
    .build()
    .start();
```

25 counted lines become 16; 22 ceremony lines become 6. `src/app.html` is
untouched, and its comments — which currently explain that `vilan build .`
writes `dist/client.css` and that `src/server.vl` serves it there — get to say
something shorter, since only half of that is still the reader's problem.

**At rung 0+** — one word longer, and the scaffold now meets the charter's bar:

```vilan
let document = Document::from_shell(fs::read_file_to_str("src/app.html"), build)!;
```

A scaffolded project whose `app.html` loses its `<link>` no longer starts. That
is the single highest-value line in this section: the failure the charter was
written about becomes impossible in the file every new user edits first.

**At rung 2** — `src/app.html` is deleted and the document is built:

```vilan
let page = Document::of(build)
    .title("{{name}}")
    .head("<style>body { font: 16px/1.5 system-ui, sans-serif; max-width: 32rem; margin: 2rem auto; }</style>")
    .html();
```

Six template files become five, and the `{{name}}` substitution moves from
`app.html` into `server.vl` — where it already appears, so no new mechanism.

### 6.3 The recommendation, and its cost

**Ship the template at rung 1 + rung 0+. Do not ship it at rung 2.**

The reasoning is about what a scaffold is for. `vilan init`'s server file is a
teaching artifact read by someone who has never seen the language, and §2.2
measured it at 22 ceremony lines to 1 of intent — which teaches that a vilan
server is mostly filesystem plumbing. Rung 1 fixes that: six lines, four of
them about *this app*. Rung 0+ then makes the remaining hand-written artifact
safe, which is exactly the demonstration the charter asks for — the shell is
still yours, and we will still tell you when it is wrong.

Rung 2 in the scaffold trades that for a smaller file and a larger surprise. A
web developer opening a new project expects to find the HTML; not finding it is
the kind of magic that makes a framework feel like one, and *progressively
lowering to full control* reads better as "the file is here, and it is checked"
than as "there is no file until you ask". Rung 2 belongs in `docs/guide/`, as
the step you take once you have decided you do not care about the document — a
real and common decision, and not the default one. Cost of this
recommendation: none — the blessed examples keep their `app.html`, so
`tests/init.rs:335` and the three examples are untouched. Cost of the other
choice: one commit across four projects and one test, which is a corpus-wide
shape change and so is §10.6.

Two smaller notes. **`examples/todo` and `examples/walkthrough` should move to
the builder** when (a) lands, even though `serve_service` keeps working (§4.6):
they are the two files the owner transcribed from (§2.1) and the two that teach
the dead end. And **the `browser` template is unaffected by (a) and (b) alike**
— no server, `index.html` loaded from disk or a static host, coupling checked
by `tests/init.rs:141-162` — but it is the one project `hmr.md` §9's dev server
would serve, which is what ties §5.10 to a real user.

## 7. Reconciliation with the ratified records

The charter asks for tensions to be addressed head-on. There are six, and each
is resolved by being precise about what was actually decided.

### 7.1 `ssr.md` §6(b) — the declined `render_into`

What was declined, verbatim:

> **(b) The splice API**: v1 keeps the shell splice in user code
> (`shell.replace("<!--app-->", render(app()))` — recommendation: honest, zero
> new surface) vs a `render_into(shell, marker, view)` convenience in std.

Three things about that decline matter here. Its stated reason is **"zero new
surface"** — a cost argument, not a correctness one, and correct at the time: a
helper taking a string, a marker and a view to perform a `replace` buys a user
nothing they cannot write and buys std a maintenance obligation. It was scoped
**to v1** of an arc whose subject was rendering, not documents. And §5.8's
proposal **is not `render_into`**: it is a method on a `Document` value this
paper argues must exist for seam (b)'s own reasons, and it takes **no marker at
all** — the mount element is a property of the document, so F5 is deleted
rather than wrapped.

So: `render_into(shell, marker, view)` stays declined, for the reason §6(b)
gave. `Document::render(view)` is a different thing, whose cost is already paid
by the design carrying it, and whose benefit is the removal of a silent failure
`ssr.md` did not have to weigh — SSR's marker was one string in one example
then, and is now the pattern every SSR app in the language copies. If the owner
disagrees, and the decline was about the *idea* of std knowing what an HTML
document is rather than about the helper's shape, then rung 2 falls, rung 1 and
the validator stand alone, and the paper is still worth having. §10.4.

### 7.2 `hmr.md` §8 — server-side HMR stays a permanent non-goal

> **Server-side HMR**: a non-goal, permanently — restart is the model for the
> Node leg; the process is cheap and correctness is free.

Nothing in this paper makes a running server's *code* replaceable. `serve_build`
changes only where the asset bytes are read — from three `let`s at the top of
`main` to one library call in the request path — which is a question about
**data freshness**, not code identity, the distinction `dev-refresh.md` §0
already drew: "A server that restarts on every code change can still serve
week-old bytes for a file it read once at the top of `main`; closing that gap
doesn't reopen the server-side-HMR question." Inherited unchanged. Two smaller
§8 clauses are respected too: the dev channel keeps binding `127.0.0.1` and
serving only `dist/` artifacts, and nothing here asks it to serve a user's page
(§5.10 defers `hmr.md` §9's dev server explicitly).

### 7.3 `dev-refresh.md` — the boundary, in both directions

E55's general half is DRAFTED and awaiting ratification. This paper **does not
redesign it, does not depend on its outcome, and does not pre-empt its open
questions** (§4 of that note: the signalling plumbing, the process-layer
surface's shape, whether (iii) is a permanent non-goal, and the scope of the
dev signal). What it does is supply the call site that note says the
revalidating read is missing — §2(i)'s "a primitive in search of a call site
that invokes it more than once" — by moving the read out of `main` and into
`serve_build`. Sequencing: either can ship first. If E55 lands first,
`serve_build` is written against the hook. If seam (b) lands first,
`serve_build` reads per request the plain way and gains the hook in a
one-line change.

### 7.4 `bundle-splitting.md` — splitting stays opt-in; the manifest changes

Untouched: `split = true` remains a `vilan build` optimisation that `run`
ignores and says so once (§4, §10); single-file emission stays first-class;
nothing here recommends any example declare `split` (§9 measured that none
should).

Changed, and put to the owner as §10.3: the sidecar becomes the leg's build
manifest and is written on every build of the leg. §5.9 argues this
*strengthens* §9's stated invariant rather than weakening it, but it does
reverse one ratified sentence and churn a byte-pinned golden, and a ratified
sentence should not be reversed inside a design note.

One further alignment: §9 recorded that `<link rel="modulepreload">` "is a
page's decision, not a compiler's", and that "an SSR server can [write it], and
now has `chunks.json` to write it from". A `Document` built from a `LegBuild`
is exactly the thing that can, and §9's recorded first-paint fix becomes
reachable without any new information. This paper does not propose emitting the
preload in v1 — it is a performance decision with its own measurement — but it
notes that rung 2 is where it would go.

### 7.5 `ssr.md` §6(a) — the process `ui` stays fragment-only

§6(a) ratified omitting `mount`/`mount_root` from the process layer. §5.5's
`Document` is a string builder precisely so that call stays made: no `View`
grows document semantics, no process-layer mount is needed, and the two `ui`
implementations' differential pin (`ssr.md` §4, S1) is unaffected because
`Document` sits above `render(view)` and calls it.

### 7.6 `transport-rpc.md` §4.2 — `serve_service`'s contract is preserved

§4.2 records `serve_service` as "`serve_connected` with that registry as its
connection lifecycle", and that "manual wiring stays available
(`serve_connected` + your own attach) for SSE clients and custom session
state". §4.6 keeps both signatures and both behaviours; `Service::new` *is* the
registry lifecycle and `Service::on_connect`/`on_disconnect` *is* the manual
wiring, so the two documented paths become two constructors of one value rather
than two functions with different arities.

## 8. Slices

Suite-gated, docs in the same commit, per-case pins — the house discipline.
S1 is independent of everything else; S2 gates S3; S3 gates S4 and S5.

1. **S1 — the server that grows.** `Service`, `ServerBuilder::with_service`,
   the build-time fold of services in front of `on_request`, the path-routing
   upgrade dispatcher, and `serve_rpc` / `serve_service` / `serve_connected`
   rewritten as four-line bodies over it with their signatures unchanged.
   **Gate**: the existing e2e suite is the pin — `crates/vilan-cli/tests/
   rpc_http.rs`, `transport_robustness.rs` and `streaming.rs` drive all three
   `serve_*` forms and the benchmarks drive two more, so an unchanged suite is
   an unchanged wire. **New pins, each planted red**: services answer before
   `on_request` regardless of call order; two services on distinct mounts each
   answer their own; an upgrade routes to its mount's service; the segment
   match lets `/rpcs` through where `starts_with` swallowed it; `serve_service`
   over the layer is byte-identical on the wire to `serve_service` today.
   Docs: `docs/std/process.md`, `docs/guide/services.md`.
2. **S2 — the channel.** `LegBuild`, `build_of(leg)`, and the manifest
   extension (§10.3's ruling decides whether this is an always-written
   `<leg>.chunks.json` or something else). **Gate**: the split fixture's
   goldens (`crates/vilan-cli/tests/split/golden/`) churn by exactly the added
   fields; a non-splitting leg now writes a manifest whose `chunks` is empty;
   `build_of` on a leg that was never built is a named error, not a panic.
3. **S3 — rung 1.** `ServerBuilder::serve_build`, the extension→content-type
   table, and the consumer sweep: `examples/todo`, `examples/walkthrough`,
   `examples/ssr`, `examples/fullstack` and the `init` template all lose their
   reads and route tables in one commit. **Gate**: every example's existing
   e2e still passes unchanged (they assert served bytes, which do not move);
   `examples/fullstack` serves its chunks with no `ChunkFile` type in the tree;
   a leg that gains `split = true` serves its chunks with no server edit —
   planted by adding `split` to the fixture and asserting the routes appear.
4. **S4 — the validator.** `ShellFault`, `check_shell`,
   `Document::from_shell`, and the template's one-line adoption. **Gate**: one
   pin per fault variant, each planted by breaking a real shell — delete the
   `<link>` from a scaffolded project and assert the server refuses to boot
   naming the file; add a `<link>` to a leg with no styles; rename the mount
   div; and the F6 case, a `type="module"` shell over a splitting leg.
5. **S5 — rung 2.** `Document::of` and the builder methods, `Document::render`,
   `examples/ssr` losing its `<!--ssr-->` marker. **Gate**: the property that
   makes the two entry points one design — *every document `Document::of` can
   produce passes `check_shell`* — pinned over the builder's option space, in
   the spirit of `ssr.md` §4's cross-implementation differential; plus
   `examples/ssr`'s existing e2e, which asserts server-rendered content before
   any JS and a replacing client boot, unchanged after the marker is gone.
6. **The two independents**, either of which can ship alone and neither of
   which needs a design: `ServerBuilder::on_stop` made real (§9.1), and
   `mount_root` naming the id it could not find (§9.5).

## 9. Bycatch — found by the survey, not E56's to fix

### 9.1 `ServerBuilder::on_stop` is a documented no-op

`ServerBuilder` declares the field (`http.vl:264`), initialises it
(`:274`), exposes a setter (`:364-366`), and **`build()` does not carry it into
`Server`** (`:369-375`) — `Server` has no `on_stop` field (`:245-256`) and
`start()` never calls one. It is documented as public API in
`vilan/docs/std/process.md:55`. So `.on_stop(|server| cleanup())` compiles,
type-checks, reads correctly, and never runs. There is also no `Server::stop`,
so there is nothing that could fire it.

This is a one-field fix plus a decision about what "stop" means (there is no
shutdown path today), and it is squarely in seam (a)'s neighbourhood without
being seam (a)'s problem. **File it.**

### 9.2 The chunk base resolution has never executed

§3.5. `document.currentScript` is `null` in a module script; all twelve shells
in and around the tree use `type="module"`; the emitted guard therefore always
takes `base = ""`, which resolves the chunk `import()` against the document URL
— the exact miss `bundle-splitting.md` §8 names. It has gone unnoticed because
no example splits and the split fixture runs under Node. **File it against the
A16 line**: either the shell must be a classic script when a leg splits (which
is what `LegBuild.classic_script` and F6 exist to enforce, so E56 can carry the
*detection*), or the base must be resolved by something a module script has —
`import.meta.url`, which is exactly the right answer for a module and is
available in every environment the bundle runs in.

### 9.3 `std::fs` cannot read bytes

`vilan/std/src/process/fs.vl` is twenty lines: `read_file_bytes(path,
encoding): str` (misleadingly named — it returns a decoded string), the
`read_file_to_str` wrapper over it, `write_file`, and `exists`. No binary read,
no directory listing, no stat. So
no vilan program can serve an image, a font or a favicon; nothing can enumerate
a directory; and `dev-refresh.md` §2(i)'s mtime-revalidating read has no `stat`
to call. **File it** — a std gap E56 walks past four times and never needs, but
the next paper will.

### 9.4 Two documentation drifts in `examples/todo`

`src/server.vl:3-4`'s header comment says the file uses
`std::rpc_server::serve_connected`; the import at `:12` and the call at `:28`
are `serve_service`. And `src/app.html:6` closes `</head>` on the same line as
the stylesheet `<link>`, three lines below the opening `<head>` — valid HTML,
reads as a mistake, and it is the first shell many readers see. One-line fixes
for whichever slice touches the file next.

### 9.5 `mount_root` on a missing id fails as a null dereference

`get_element_by_id(id): Element` (`browser/dom.vl:13-14`) has no `Option`
return, so a missing element is JS `null` typed as `Element`, and
`mount(id, view)`'s `element.clear()` (`browser/ui.vl:665-668`) throws
`Cannot read properties of null (reading …)`. The id the user got wrong appears
nowhere in the message. This is F4's runtime half and the cheapest loud win in
the whole survey — a guarded lookup that names the id — and it is worth doing
**whether or not any of §5 ships**, because it is the one check that works
without knowing anything about the build.

## 10. Open questions — the owner's to rule

### 10.1 Does the ladder go past rung 1 at all? — recommend: yes, and it is the paper's biggest ask

Rung 1 (`serve_build`) is nearly uncontroversial: it deletes derivable code and
adds a value that describes a build. Rung 2 asks **std to know what an HTML
document is**, which it has never known, and which is a genuinely different
kind of commitment — a surface that will grow (meta tags, preload hints, CSP,
`<base>`, i18n attributes) and that has no obvious stopping point. **Draft: in
scope, bounded by §5.5's "the intersection of the seven shells plus
`head`/`body` escape hatches", and reviewed again if the first three feature
requests are all in `head()`.** The cost of saying no: F1, F2, F3 and F6 are
still caught by the validator, F5 (the SSR marker) is not, and the charter's
"easy to set up" clause is answered only for the server, not for the page.

### 10.2 Where does `LegBuild` come from? — recommend: a read manifest for v1, compiler-minted recorded

Two shapes. **(A) Read** an always-written `dist/<leg>.chunks.json` at the
server, through E55's freshness hook. Costs nothing in the compiler, and makes
a build fact into a boot-time failure. **(B) Compiler-minted**: the CLI hands
the server leg's build a description of the browser leg's outputs, and
`build_of("client")` is a constant. Workspace members already build in
declaration order specifically so "the server's `dist/client.js` exists"
(`crates/vilan-cli/src/main.rs:1814-1817`), so the ordering is there — but the
*plumbing* is not: legs compile independently today and nothing passes one
leg's outputs to another's build. That is real new machinery and it is this
paper's largest single cost. **Draft: (A) now, (B) recorded as the end-state**,
because (B)'s payoff is moving four checks from boot to compile time, which is
better but not different in kind. Flagged because (B) is the only shape that
could ever make a wrong shell a *compile* error, which is the strongest reading
of "LOUD".

### 10.3 Is the chunks sidecar allowed to become the leg's build manifest? — recommend: yes

It means writing `<leg>.chunks.json` on every build of the leg rather than only
when it splits, adding `styles` and `classic_script`, and reversing
`bundle-splitting.md` §9's "dropping `split` takes the manifest with it".
§5.9 argues the invariant is preserved and strengthened. **Draft: do it.**
Flagged because it reverses a ratified sentence and churns a byte-pinned golden
(`crates/vilan-cli/tests/split/golden/app.chunks.json`), and a design note
should not reverse a ratified sentence on its own authority. The alternative —
a second sidecar next to the first — is worse on every axis except this one.

### 10.4 Was `ssr.md` §6(b)'s decline about the helper, or about the idea? — flagged, no recommendation

§7.1 reads it as a cost argument about a specific three-argument helper, which
makes `Document::render` a different question. If the owner meant it as "std
does not model documents", then §10.1 is already answered no and rung 2 falls.
**No draft**: this is a reading of the owner's own prior ruling and only the
owner can settle which reading was meant. It is asked separately from §10.1
because the two could be answered differently — one may want rung 2 *and* want
the record to say the earlier decline was narrower than it looked.

### 10.5 Should the reactive session registry be keyed by service? — recommend: no, record the constraint

`std::rpc`'s `reactive_sessions` is keyed by connection id alone
(`rpc.vl:1006-1012`), and connection ids are minted from one module-level
counter in `rpc_server.vl` (`:97-106`). Two services therefore share one id
space and one registry — which *works*, because ids stay unique. **Draft: leave
it, and say so in the doc comment**, since keying by `(service, connection)`
buys isolation nobody has asked for and touches a shipped `Client::connect`
contract (`transport-rpc.md` §4.2). Flagged because "two services share a
global session table" is the kind of thing that is fine until it is not.

### 10.6 What rung does `vilan init` ship? — recommend: rung 1 + the validator

§6.3. The alternative (rung 2, no `app.html` in the scaffold) is defensible and
is a smaller file, and it costs one commit touching `walkthrough`, `todo`,
`ssr`, the template and `tests/init.rs:335` — because that gate pins
`src/app.html` as a member of the blessed shape in all four places at once.
**Draft: rung 1 + validator in the scaffold, rung 2 in `docs/guide/`.** Flagged
because the scaffold is the language's opening argument and the owner should
choose what it argues.

### 10.7 How loud is loud? — recommend: refuse to boot

§5.6. A shell fault stops the server from starting, with a message naming the
file, the fault and the fix; `check_shell` returns a `Result` so an app can
choose otherwise. The alternative is a startup warning, which the eleven shells
of §2.2 suggest would be read exactly as often as the console line that already
reports F3. **Draft: refuse.** Flagged because refusing to boot on a *style*
problem is a strong position and it is the owner's bar to set.

### 10.8 Route matching — segment, not prefix? — recommend: yes

§4.3. `path.starts_with("/rpc")` shadows `/rpcs`; matching the path *segment*
does not. This is a behaviour change for any program with a route beginning
`/rpc`, `/send` or `/events` — a set this survey believes is empty and a corpus
check would settle. **Draft: change it, as part of S1, with the corpus check
run first and reported.**

### 10.9 Middleware — declined? — recommend: yes, declined with the reason recorded

§4.5. `on_request` is one closure, so wrapping it is already the language's
answer; a `next`-passing layer stack multiplies async coloring for no
capability; there is no demand. **Draft: decline, and record `Mount` as the
shape it would be built on if it is ever wanted**, so "a service" and "a layer"
never become two concepts.

### 10.10 The names — the owner's, as always

`LegBuild` / `build_of(leg)` / `serve_build` / `Document` / `check_shell` /
`ShellFault` / `Service` / `with_service`. Two are worth a second look:
`LegBuild` reads oddly beside `[entry.<name>]` (the manifest calls them
entries, the CLI calls them legs, and the docs use both), and `Document` is a
big name for a small struct. Alternatives considered and not preferred:
`Emitted` / `Artifacts` / `BuildOutput` for the first, `Page` / `Shell` /
`Html` for the second. **Draft: as listed**, with `Shell` as the runner-up for
`Document` since every existing doc comment in the tree already calls the thing
"the HTML shell".

## 11. The recommendations, collected

1. **Build the layer, not a fourth boot function.** `Service` +
   `ServerBuilder::with_service`, services answering before `on_request` —
   which is what `connected_response` already does — with a longest-mount
   tiebreak and one path-routing upgrade dispatcher. `serve_rpc`,
   `serve_service` and `serve_connected` keep their signatures and become
   four-line bodies over it. **S1.** (§4)
2. **Give the server leg a value describing the browser leg's build.**
   `LegBuild` + `build_of(leg)`, from an extended, always-written manifest.
   This is the whole of the fix; everything else in §5 is a use of it. **S2.**
   (§5.2, §10.2, §10.3)
3. **Serve the build, not a list of files.** `serve_build` installs one route
   per artifact with derived content types, in front of `on_request`, and takes
   E55's freshness hook as its single read site. It deletes 34 of
   `examples/fullstack`'s 52 ceremony lines and 8 of `examples/todo`'s 10.
   **S3.** (§5.4)
4. **Make validation the primitive and generation the sugar.** `check_shell` +
   `Document::from_shell` first; `Document::of` is the same rules applied to
   markup std wrote. This is the only shape in which the escape hatch is safe
   rather than merely available, and it is the paper's central claim. **S4,
   S5.** (§5.6)
5. **Delete the SSR marker rather than checking it.** `Document::render(view)`
   splices into the mount element the document already knows, so F5 has nothing
   to misspell. `render_into(shell, marker, view)` stays declined for the reason
   `ssr.md` §6(b) gave. **S5.** (§5.8, §7.1)
6. **Ship the scaffold at rung 1 + the validator**, not rung 2: 25 lines to 16,
   22 ceremony lines to 6, and a shell that cannot silently lose its stylesheet
   link — with the raw file still there to read. **S3, S4.** (§6.3)
7. **Leave alone**: server-side HMR (`hmr.md` §8 — permanently a non-goal, and
   nothing here touches it); the process-layer `ui`'s fragment-only surface
   (`ssr.md` §6a); `split` as a build-only optimisation (`bundle-splitting.md`
   §4, §10); the `<link>` idiom the css hot-swap depends on (`hmr.md` §2 and
   its appendix); `serve_service`'s signature and the manual-wiring path
   (`transport-rpc.md` §4.2); the session registry's key (§10.5).
8. **Decline, with reasons**: middleware (§4.5); a general `Mount` abstraction
   built before its third customer (§4.5); a static file server (§5.10); a
   `View`-shaped document (§5.5); configurable asset route prefixes in v1
   (§5.4); the modulepreload emission (§7.4).
9. **File separately** (§9): `ServerBuilder::on_stop` is a documented no-op;
   the chunk base resolution has never executed and `import.meta.url` is its
   real answer; `std::fs` cannot read bytes, list a directory, or stat;
   `mount_root` fails as a null dereference instead of naming the id it could
   not find; two documentation drifts in `examples/todo`.
10. **Do not redesign E55.** `dev-refresh.md` §3's hook is the freshness
    mechanism; this paper supplies the call site that note says it is missing
    (§2(i)), and the two can ship in either order. (§5.9, §7.3)

## 12. S2 shipped — the channel (2026-08-11)

`LegBuild`, `build_of(leg)`, and the manifest extension §10.3 ruled on. The
slice is deliberately small and entirely a *description*: no server surface
moves here, and nothing consumes the value yet (S3 does).

### 12.1 The manifest's final shape

`dist/<leg>.chunks.json`, written by **every build of a browser leg**:

```json
{
	"leg": "client",
	"entry": "client.js",
	"styles": "client.css",
	"classic_script": false,
	"chunks": []
}
```

Three fields were added to the two `bundle-splitting.md` §3 already wrote,
and the two it wrote are untouched — `entry` is still the eager bundle's
file name and `chunks` is still the same `{ arm, tag, file }` rows in the
same order. `leg` is the manifest's own name for the leg, so a reader needs
no filename convention to know what it describes; `styles` is the sidecar's
file name or `null`; `classic_script` is `!chunks.is_empty()`, spelled as
the fact a shell needs rather than as the fact the compiler has (§3.5, F6).

**The golden churned by exactly three inserted lines.**
`crates/vilan-cli/tests/split/golden/app.chunks.json`, diffed:

```
 {
+	"leg": "app",
 	"entry": "app.js",
+	"styles": null,
+	"classic_script": true,
 	"chunks": [
```

No existing line moved a byte — the three fields are *inserted*, `"leg"`
before `"entry"` and the other two after it, so the `"entry"` line, all
three `chunks` rows and both closing braces are the bytes they were. The
other four goldens (`app.js` and the three chunk files) are byte-identical:
this slice changed no emitted JavaScript. `"styles": null` because the
fixture compiles no styles; `"classic_script": true` because it splits.

The empty case is spelled `"chunks": []` on one line rather than an empty
multi-line array, which is the only formatting decision in the change.

### 12.2 Where the manifest is written, and where it is not

Three call sites write a leg, and all three now go through the same
`write_chunks(output, chunks, styles, is_browser)`: `build_single`,
`build_workspace_artifacts`, and the HMR watch round — which previously
called `sweep_stale_chunks` directly and would otherwise have *deleted* the
manifest on every round, exactly where a dev-loop server needs it most.
`write_assets` now returns the style sidecar's file name so the two facts
are collected at the one place that knows them.

**A node leg writes none.** `classic_script` has no meaning off the browser
and a node leg has no chunks; a manifest describing a `.mjs` nobody loads
would be a value `build_of` could return and no consumer could use.

**The build log is byte-identical** for every project that has one today:
the manifest line is printed only when it describes chunks, which is exactly
when it was printed before.

### 12.3 What §9's invariant became

Recorded as an appendix note in `bundle-splitting.md` (Appendix A) rather
than as a Status edit, since it reverses one sentence of a shipped arc's
record. The short form: the invariant is *the leg's last build owns the
namespace*, and it is stronger now, not weaker — the sweep that used to
DELETE the manifest now REWRITES it, and `"chunks": []` is a positive
statement where an absent file was an ambiguity.

### 12.4 The surface, and one addition beyond §5.2's sketch

`std::build` (`vilan/std/src/process/build.vl`) — a new process-layer
module, no twin, no shadow. `LegBuild` carries §5.2's five fields plus
`dist`, the directory its file names are relative to, so `serve_build` reads
a path off the value instead of re-deriving the `dist/` convention
independently. `LegBuild::artifacts()` gives `(url, file)` pairs in serving
order and `content_type_of(file)` is the extension table (§5.4): `js`/`mjs`,
`css`, `json`, `html`, and `None` for anything else — not served rather than
guessed at, because `serve_build` serves a build and not a directory (§5.10).

`BuildError` is `NotBuilt(path)` / `Unreadable(path)` with a `message()`
that names the path and the command that would produce it. **`require_build(leg)`
is an addition**, and the reason is a language fact §6.2's sketch missed:
`build_of("client")!` cannot be written in `async fun main()`, because `!`
asserts-or-RETURNS and a void `main` has no residual to carry
(`try-and-lift.md` §2 — "`!` in a bare-void function" is a pinned error). The
alternative was `.expect("…")` with a hand-written string in every consumer,
which is the ceremony this paper exists to delete. `require_build` is §10.7's
"refuse to boot" doctrine spelled once, in std, where the message can name
the leg.

### 12.5 The gates

`crates/vilan-cli/tests/build_manifest.rs`, five pins, each planted red:

- a browser leg that does not split still writes its manifest, with an empty
  chunk list and `classic_script: false`;
- `styles` names the sidecar exactly when one was emitted, and is `null` when
  the leg compiled none *and there is no file on disk to probe for*;
- a node leg writes none;
- `build_of` describes the leg the build wrote, reported from a real server
  leg through `artifacts()`;
- **`build_of` on a leg that was never built is a named error** — the process
  keeps running, and the message names `dist/nosuchleg.chunks.json` and
  `vilan build`.

Planted: (a) restoring the `chunks.is_empty()` early return reddens three of
the five; (b) suppressing the `styles` report reddens two. The split
fixture's own goldens and the three `split.rs` pins that read the manifest's
*absence* as "did not split" now read its `chunks` list, which is what they
meant.

## 13. S3 shipped — rung 1, and the dev policy (2026-08-11)

`ServerBuilder::serve_build`, the extension→content-type table, the consumer
sweep, and — the post-ratification amendment §0 records — dev-mode asset
freshness as `serve_build`'s own policy (`dev-refresh.md` §5, items 1–2a).

### 13.1 The surface

```vilan
impl ServerBuilder {
	fun serve_build(own self, build: LegBuild): ServerBuilder
}
fun build_handler(build: LegBuild, fallback: |Request| Response): |Request| Response
```

`ServerBuilder` gains one field, `assets: List<BuildAsset>`, and `build()`
gains one responsibility: fold the build's routes in front of the user's
`request_handler`. The fold is a pure function of the field, so `Server`
still holds exactly one request handler and `start()` is untouched — the
shape §4.2 promised for `with_service`, arrived at independently for its
sibling. Installing before or after `.on_request(…)` behaves identically,
which is the property a field (rather than a wrapper applied at call time)
buys.

The content-type table is `.js`/`.mjs` → `text/javascript`, `.css` →
`text/css`, `.json` → `application/json`, `.html` → `text/html`. Anything
else is **not served** rather than guessed at (§5.10): four rows, covering
exactly what a vilan build can emit. A query string is stripped before
matching, so `/client.js?v=2` is the bundle — a cache-buster is not a
different file.

**`build_handler` is the slice's one unplanned name, and it is a direct
consequence of S1 not being in this slice.** `serve_service` and
`serve_connected` construct their own `Server` and hand the app only a
`fallback`, which is §3.7's whole complaint; until `with_service` lands,
`examples/todo` and `examples/walkthrough` have no builder to install
`serve_build` on, and the sweep's mandate covers them. `build_handler` is
the same fold, the same reads, the same freshness policy, exposed as a
`|Request| Response` — composable exactly as `rpc_response` is (§3.7's
"honourable exception"). When S1 lands, those two examples take
`.with_service(…) + .serve_build(…)` on a builder and `build_handler` has
nothing left to do; it is not load-bearing for the design and should be
reviewed for removal then.

One language fact forced the shape: the two forms cannot share their outer
body because the fallback's COLOR differs — `ServerBuilder`'s handler is
`async` and `serve_service`'s is not, and a closure that calls an async
closure is async. They share `asset_response`, which is where the policy
lives; the duplication is four lines of `match`.

### 13.2 The dev policy, and a sync read

Under `run --watch`, `serve_build` re-reads each asset per request; outside
one, it serves the copy read at boot. `dev-refresh.md` §5's argument in one
line: the problem is pull-shaped, every HTTP request is an opportunity to be
fresh, and taking it needs no signalling protocol — which is what sank the
re-run-on-round hook, since editing `app.html` produces no round to fire on.

The signal is **`VILAN_WATCHING=1`**, set by `spawn_watched_node` on the
Node child of both watch paths (the HMR round and the plain restart loop)
and by neither `vilan run` nor `vilan build`. `std::process::is_watching()`
reads it. Placement: `std::process`, not a new `std::dev` twin —
`vilan/std/src/browser/dev.vl` already owns that module name, and a process
twin with a different surface would trip `std_twin_parity`'s inventory gate
for no gain. `std::process` is where `env`, `args` and `exit` already live,
and `is_watching` is an env lookup. §4's scope question resolves as §5 ruled:
DEFINED under every run, `true` only under a watch.

**One std addition fell out of it**: `fs::read_file_to_str_sync`. The
revalidating read runs inside a request handler that, in the
`serve_service` shape, cannot suspend — so the async read cannot be used
there. A synchronous read is also the better trade for a dev-only
revalidation and removes an await from the release path entirely. Recorded
here because §9.3 files `std::fs`'s gaps as bycatch and this is a (small)
one closed in passing.

### 13.3 The consumer sweep — the ceremony numbers, re-measured

Counted by §1.2's rule (non-blank, non-comment lines), before → after:

| File | lines | ceremony |
|---|---|---|
| `examples/fullstack/server/src/main.vl` | 56 → **25** | 52 → ~7 |
| `templates/fullstack/src/server.vl` | 25 → **16** | 22 → **7** |
| `examples/ssr/src/server.vl` | 20 → **17** | 15 → ~6 |
| `examples/todo/src/server.vl` | 19 → **13** | 10 → 5 |
| `examples/walkthrough/src/server.vl` | 19 → **13** | 10 → 5 |

The template lands where §6.2 predicted almost exactly — *"25 counted lines
become 16; 22 ceremony lines become 6"* — the one extra being that
`serve_build(build)` is its own chain line beside `let build = …`.
`examples/fullstack` loses 31 lines net: the two boot reads, the seven-line
route match, and the whole `ChunkFile` / `route_chunks` / `find_chunk` block
§5.4 measured at 25 lines. **There is no `ChunkFile` type left in the tree.**

`todo` and `walkthrough` land short of §5.4's predicted "8 of 10" for the
reason 13.1 gives: without S1 they keep `serve_service`, and therefore keep
`import std::fs` and `import std::http::Response` for the shell they still
read and answer by hand. The three reads did become one and the five-line
table did become one call — 6 lines each — and the remaining gap closes when
S1 lets them move to the builder (§6.3's own note says they should).

`vilan/docs/guide/{walkthrough,ssr,routing,styling,persistence,services}.md`
were taught the new idiom in the same commit; `docs/std/process.md` carries
`serve_build`, `build_handler`, `is_watching` and the sync read.

### 13.4 The gates

Every example's existing e2e passes **unchanged** — not one assertion in
`tests/examples.rs`, `tests/ssr_fullstack.rs` or `tests/init.rs` moved,
including the init template's field-by-field manifest gate and its
spawn-and-fetch e2e (`/`, `/client.js`, `/client.css`, the `{display:flex}`
rule). They assert served bytes, and the bytes did not move.

`crates/vilan-cli/tests/serve_build.rs`, six pins:

- one route per artifact with its content type; a query string does not
  change which artifact; an unclaimed path still reaches `on_request`;
  `/dist/client.js` is not a route (a build, not a directory);
- **a leg that gains `split = true` serves its chunks with no server edit** —
  the manifest is rewritten with `split = true` and the server file is then
  asserted BYTE-IDENTICAL across the two halves, so the three chunk routes
  can only have come from the build;
- a named artifact that is not on disk stops the server, naming the file and
  the leg;
- the dev policy BOTH ways: bytes moved under a running server are served
  fresh with `VILAN_WATCHING=1` and from the boot copy without it;
- `is_watching()` is `false` under plain `vilan run`;
- `run --watch` really does set the signal on its child.

Planted red, each restored: serving `asset.content` unconditionally (E55's
defect) reddens the dev pin; revalidating unconditionally reddens its release
half; dropping `chunks` from `artifacts()` reddens the split pin; and
spawning the watch child without `VILAN_WATCHING` reddens the watcher pin.

### 13.5 Bycatch cleared in passing

§9.4's two drifts in `examples/todo`, both in files this sweep rewrote:
`src/server.vl`'s header said `serve_connected` where the code says
`serve_service`, and `src/app.html:6` closed `</head>` on the stylesheet
`<link>`'s own line. Both fixed.
