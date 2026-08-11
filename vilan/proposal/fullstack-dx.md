# Full-stack setup — the document, the assets, a server that grows (E56)

> Status: DRAFT (awaiting owner review) — filed from backlog E56, the owner's
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
> Method, per the charter: survey first, like E49. §1–§3 are the inventory and
> the measurement; §4–§6 are the design; §7 reconciles with the ratified
> records this paper touches; §10 is the open-questions set. **Everything
> before §10 is a recommendation, not a ratification.** This paper proposes no
> code and compiles nothing; every claim about what the tree does today was
> read out of the tree and is cited by `file:line` or `filename §section`.
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
compiler chose whether the bundle is a classic or module script; the shell
guesses, and in this tree it has guessed wrong in every single file (§3.5).

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
`vilan/examples/walkthrough/src/server.vl` with two identifiers changed. The
three reads are identical, the five match arms are identical character for
character, the port is the same, and even the `on_start` message is the same
string ("notes server listening on"). The only differences are `boot()` →
`Notes::new()` and `import std::print` → `import std::io::print`. The owner did
not write this ceremony; the owner *transcribed* it, because it is not
derivable from anything they knew about their own app.

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
  places in the tree: eight hand-written `.html` files, one escaped string
  literal (`examples/fullstack/server/src/main.vl:82`), and zero occurrences in
  `vilan/std/`. The nearest surface is `std::ui::render(view: View): str`
  (`vilan/std/src/process/ui.vl:367-370`), whose own header says the caller
  "splices it into its HTML shell" — a **fragment** serializer by design
  (`ssr.md` §2, §6a: `mount`/`mount_root` are omitted from the process layer).
- **There is no manifest surface for assets.** No `[assets]`, `[static]`,
  `[public]`, `[shell]` or `[html]` section exists or is parsed; the manifest's
  known-key list (`crates/vilan-core/src/manifest.rs:56`) has no room for one
  today. No `vilan.toml` in the tree names `app.html`, `index.html`,
  `dist/client.js` or `dist/client.css`. Those paths live only in `.vl` string
  literals and `.html` attributes.
- **The docs teach the ceremony rather than abstracting it.**
  `docs/guide/walkthrough.md:174` and `docs/guide/ssr.md:70` carry the same
  `fs::read_file_to_str("src/app.html")` line as the examples; `docs/guide/
  walkthrough.md:52` documents `app.html` as "the shell the server serves".
  That is honest documentation of what exists, and it is also the mechanism by
  which every new project inherits it.

One more, which is a finding about the *gate* rather than the code: the closest
thing in this repository to a specification of the HTML shell is a filename in
a Rust test array. `crates/vilan-cli/tests/init.rs:335` asserts that
`src/app.html` exists in the scaffold and in each blessed example; the browser
template's coupling is checked by substring match on the HTML text
(`tests/init.rs:141-162` — `page.contains("src=\"app.js\"")`,
`page.contains("id=\"app\"")`, `page.contains("href=\"app.css\"")`), with the
stated rationale "Scaffolding a page that never loads the CSS the build writes
is the failure mode this asserts against". Someone already identified the
owner's bug, and the only instrument available to them was `str::contains` in a
test that guards two templates and nobody's project.

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
the splice as `shell.replace("<!--ssr-->", render(app()))`. `String.replace`
with no match is a no-op that returns the original string, so a misspelled or
absent marker degrades SSR to *serving the bare shell* — first paint empty,
crawler sees nothing, and the client boot then renders the page correctly, so
the app looks fine to the developer who is testing it in a browser. The one
observer who would notice is the crawler this feature exists for.

### 3.5 The script tag nobody chose

There are **eleven** HTML shells in and around this tree — five examples, two
templates, the guide's `ssr.md` page, `examples/fullstack`'s inline string
(`vilan/examples/fullstack/server/src/main.vl:82`), and the owner's app — and
every single one of them loads the bundle with `<script type="module">`.

`bundle-splitting.md` §8 ratified the chunk base resolution on the opposite
assumption:

> `import()` resolves against `document.currentScript.src`, because a classic
> script's relative specifier resolves against the DOCUMENT's URL — the route
> the user is standing on — and would miss on every nested path.

`document.currentScript` is `null` inside a module script, by specification.
The emitted helper guards for it and falls back
(`crates/vilan-core/src/transformer.rs:765-767`):

```js
let base = "";
if (typeof document !== "undefined" && document.currentScript && document.currentScript.src) {
    base = document.currentScript.src;
}
```

so under every shell this tree ships, `base` is `""` and the chunk `import()`
resolves against the document URL — **precisely the miss the design set out to
avoid**. It has never been observed because no example declares `split = true`
(`bundle-splitting.md` §9 measured that none should) and the split fixture runs
under Node, where `document` is absent and relative resolution is already
correct. The branch has, so far as this survey can tell, never executed.

This is filed as bycatch (§9.1) rather than as E56 work, because it is a
splitting bug and not a setup one. It is *reported here* because it is the
cleanest possible demonstration of the thesis: the compiler made a decision
about how the bundle must be loaded, the shell is where that decision has to be
written down, and there is no channel between them — so eleven authors, several
of whom wrote the splitting code, wrote the other thing.

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
    /// session registry as the connection lifecycle. What `serve_service`
    /// installs today.
    fun new(protocol: RpcProtocol): Service

    /// Mount the service's routes under `prefix` instead of `/`. A prefix is
    /// what lets two services coexist, and what the client's
    /// `Client::connect(url, codec)` already supplies from its side.
    fun at(own self, prefix: str): Service

    /// Replace the connection lifecycle — the pair `serve_connected` takes
    /// today, for apps holding custom per-connection state.
    fun on_connect(own self, handler: |i32, DuplexEnd| void): Service
    fun on_disconnect(own self, handler: |i32| void): Service
}

impl ServerBuilder {
    /// Install an rpc service. Repeatable. A service's routes answer before
    /// `on_request` ever runs, and independently of the order these calls
    /// were written in (§4.3).
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
answer. There are three candidate rules and the recommendation is the one that
already ships:

1. **Services first, then `on_request`, in that order always** —
   `connected_response` (`rpc_server.vl:148-207`) is literally this: three
   `if path.starts_with(…)` arms, then `fallback(request)`. **Recommended.**
   It preserves every existing program's behavior byte for byte, and it is the
   rule a reader guesses.
2. Declaration order — whichever of `.with_service` / `.on_request` was called
   first wins. Rejected: it makes a builder chain's *order* semantic, which no
   other method on `ServerBuilder` does, and it silently changes behavior when
   someone reorders lines for readability.
3. Longest-match across a merged route table. Rejected as v1 scope: there is
   no route table — `on_request` is one opaque closure, and there is nothing to
   compare its specificity *against*.

Between services, rule 1 needs a tiebreak, and there the recommendation is
**longest mount prefix first**, computed at `build()` and independent of call
order. Two services at `/` and `/admin/` then behave the way a reader expects
regardless of how they were written.

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

Two honest limits, recorded rather than solved:

- `connections` and `next_connection` are **module-level** in `rpc_server.vl`
  (`:97`, `:100`) — "one counter for the program", as the comment says. Two
  services therefore share one connection-id space. That is harmless (ids stay
  unique, which is all they are for) and it is worth stating, because a reader
  will assume per-service numbering.
- `register_session` writes into `std::rpc`'s single `reactive_sessions` list
  (`rpc.vl:1006`), keyed by connection id only. Two services whose sessions are
  both registered there are distinguished by connection id, which is global —
  so this works, but the registry is now doing something it was not named for.
  §10.5 asks whether that is acceptable or whether the registry should be
  keyed by `(service, connection)`.

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

**Does not generalize: middleware.** A layered `.with_layer(|request, next|
…)` stack is the obvious next ask and this paper recommends **declining it**,
with reasons rather than silence:

1. It already exists and needs no surface. `on_request` is one closure; a user
   who wants logging writes `.on_request(|request| log_around(request, |r|
   routes(r)))`. A middleware API would be a naming convention for a thing the
   language already expresses.
2. `on_request` is `async |Request| Response` (`http.vl:347`), so a `next`
   continuation is an async closure passed to an async closure. That is legal
   (J2's typed channel) but it multiplies the async-coloring surface at a seam
   where every existing user program is synchronous, for no capability gain.
3. There is no demand evidence. The charter names a server that grows *into
   rpc*, not a server that grows a request pipeline. The one real app the
   survey has (`vilan-playground/todo`) wants zero middleware.

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
and behave unchanged. That is the migration story: there isn't one. The layer
is additive, the sugar stays, and the docs gain a second way to say the same
thing rather than a replacement for the first.

### 4.7 What seam (a) does not change

- `Server`'s shape. One request handler, one optional upgrade handler,
  `start()` untouched.
- The wire. No frame, route name, handshake or session semantic moves. A
  service mounted at `/` is byte-identical on the wire to `serve_service`
  today, which is what makes the corpus and e2e suites the gate.
- Server-side HMR. Untouched and still a permanent non-goal (`hmr.md` §8);
  nothing here makes a running server's *code* replaceable (§7.2).

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
| **F6** | A leg splits and the shell uses `type="module"` | chunk `import()` resolves against the document URL; nested routes 404 | every shell in the tree (§3.5) |
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
`fs::read_file_to_str(path)` — that is the whole of `std::fs`
(`vilan/std/src/process/fs.vl`, 20 lines, three functions). Both take a path
the user typed. There is no third thing.

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

Three details that are decisions, not defaults:

- **Route shape.** `/<filename>`, so `dist/client.js` serves at `/client.js`
  — which is what every shell in the tree already asks for, so no shell
  changes. A `.at("/static/")` prefix is the obvious extension and is *not*
  proposed for v1: the moment the prefix is configurable, the shell has to be
  told about it, and a second string contract is exactly what this paper is
  removing. Rung 2's `Document` could carry the prefix and keep them in sync,
  which is the argument for adding it *later*, on top of rung 2, not now.
- **Content types** come from the extension: `.js` → `text/javascript`,
  `.css` → `text/css`, `.json` → `application/json`. A short fixed table in
  std, not a user-facing map. Anything not in the table is not served by
  `serve_build`, because `serve_build` serves *the build*, not a directory —
  and this is the reason it is not a general static-file server (§5.10).
- **Missing artifacts are loud.** `build_of` names a file the leg emitted; if
  it is not on disk, that is a broken build, and `serve_build` reports it at
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
stating because the opposite is the obvious guess. Making the document a `View`
would mean the process-layer `ui` grows `<html>`/`<head>`/`<body>` semantics
and a document-level mount — and `ssr.md` §6(a) ratified the process layer as
*fragment-only*, with `mount`/`mount_root` deliberately omitted ("the natural
`fun app(): View` factoring makes it unreachable"). A `View`-shaped document
reopens that call for no gain: a document is not a reactive tree, nothing binds
to it, and it is serialized exactly once per request. **Declining is a
decision, not an omission.** `Document` composes *with* `View` at one point —
`render(view)` — and nowhere else.

Two more calls made explicitly:

- **The default document is opinionated and small**: `<!doctype html>`,
  `<html lang="en">`, `<meta charset="utf-8">`, the viewport meta, `<title>`,
  the conditional `<link>`, `<div id="app">`, the script tag. That is the
  intersection of the seven shells in §2.2, and every one of them is
  reconstructible from it plus `head`/`body`. The `<style>` block that two
  templates carry for page framing is `head()`'s first customer.
- **`html()` returns a `str`, not a `Response`.** Rung 2 hands the app a
  string; the app decides the status code, the headers and which paths get it.
  A `Document` that knew about HTTP would have to know about routing, and then
  it is a framework.

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

/// A hand-authored shell, checked. The rung-0 escape hatch, made safe: the
/// same `Document` value, its markup supplied rather than generated.
fun Document::from_shell(shell: str, build: LegBuild): Result<Document, List<ShellFault>>
```

`Document::of` is then **`from_shell` over markup it wrote itself**, and its
guarantee is that the check it would run cannot fail. One set of rules, one
implementation, two entry points — which is also the property that keeps the
generator and the checker from drifting, the way the two `ui` implementations
are kept from drifting by `ssr.md` §4's differential pin. The same instrument
applies here: *every document `Document::of` can produce passes `check_shell`*
is a property test, and it is the gate this slice owes.

**How loud is loud?** The recommendation: `check_shell` returns a `Result`, so
an application can decide; the sugar every template uses is `!`, so a broken
document **stops the server from starting** with a message naming the fault,
the file and the fix. The owner's bug would have read:

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

### 5.7 Rung 0 stays, and is the reason the rest is shaped this way

Nothing above removes the ability to write `fs::read_file_to_str("src/app.html")`
and a `match request.path()`. Rung 0 is not deprecated, not warned about, and
not scheduled for removal, and the design owes it three things:

1. `serve_build` is *additive* on the builder, so an app can serve the build
   and still answer `/legacy.js` from its own handler.
2. `check_shell` takes a `str`, so it works on a shell produced any way at all
   — read from disk, templated, fetched from a CMS.
3. `Document::html()` returns a `str`, so an app can take the generated
   document and post-process it with the same string operations it uses today.

The escape hatch is only credible if the rungs above it are made of the same
material, which is why every piece of this design is a plain value: a
description, a string, a `Result`.

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
state of the art. `LegBuild.styles: Option<str>` is that guard, answered by the
build instead of by a filesystem probe, and F1/F2 are the two directions the
probe cannot check. The `<link>` idiom itself is preserved unchanged, which
matters: `hmr.md` §2 and its 2026-08-10 appendix both depend on the stylesheet
being a findable `<link>` whose `href` ends in the sidecar's name, and the css
hot-swap supersedes it (`link.disabled = true`) rather than replacing it, so
"a plain page reload starts clean from a freshly parsed `app.html`". A
generated document must therefore emit a real `<link>` with the sidecar's
filename in its href — which it does — and must not inline styles, which it
does not.

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
an image, or a `robots.txt`, and that is deliberate — a directory server has a
traversal-safety surface, a MIME-type surface and a caching surface, and none of
them are E56's subject. It is also, today, impossible: `std::fs` cannot read a
binary file at all (`read_file_bytes(path, encoding): str` is the only read, and
it returns a string — `vilan/std/src/process/fs.vl:5-6`), so no vilan program
can serve a PNG. That is filed as bycatch (§9.3).

`hmr.md` §9 records the adjacent gap: "grow the dev channel's static serving
into a tiny dev server (`index.html` + bundle) so `run --watch` works without a
Node leg". Rung 2 is half of what that needs — a browser-only project has no
server leg to call `Document::of` from, but the CLI has the same `LegBuild`
information and could render the same document. **Recommendation: do not build
it in this arc, and note the alignment.** If `Document` lands in std as a plain
value over a plain description, the dev server's page is the same rendering
performed CLI-side, and §9's item shrinks from "design a dev server's HTML" to
"serve `Document::of(build).html()`". That is worth recording precisely so the
two do not get designed twice.

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

Today (`templates/fullstack/src/server.vl`, 25 counted lines, 22 ceremony):

```vilan
let client_js = fs::read_file_to_str("dist/client.js");
let shell = fs::read_file_to_str("src/app.html");
let client_css = if fs::exists("dist/client.css") { fs::read_file_to_str("dist/client.css") } else { "" };
Server::builder()
    .port(8080)
    .on_request(|request| {
        match request.path() {
            "/client.js" => Response::builder().set_header("Content-Type", "text/javascript").body(client_js).build(),
            "/client.css" => Response::builder().set_header("Content-Type", "text/css").body(client_css).build(),
            _ => Response::builder().set_header("Content-Type", "text/html").body(shell).build(),
        }
    })
    .on_start(|server| print(greeting() + " — http://localhost:8080/"))
    .build()
    .start();
```

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

25 counted lines become 12; 22 ceremony lines become 6. `src/app.html` is
untouched, and its comments — which currently explain that `vilan build .`
writes `dist/client.css` and that `src/server.vl` serves it at that path — get
to say something shorter, because only half of it is still the reader's problem.

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
teaching artifact read by someone who has never seen the language; §2.2 measured
it at 22 ceremony lines to 1 of intent, which teaches that a vilan server is
mostly filesystem plumbing. Rung 1 fixes that — the file becomes six lines of
which four are about *this app*. Rung 0+ then makes the remaining hand-written
artifact safe, which is precisely the demonstration the charter asks for: the
shell is still yours, and we will still tell you when it is wrong.

Rung 2 in the scaffold trades that for a smaller file and a larger surprise. A
web developer opening a new project expects to find the HTML; not finding it
is the kind of magic that makes a framework feel like one, and the charter's
third clause — *progressively lowering to full control* — reads better as "the
file is here, and it is checked" than as "there is no file until you ask". Rung
2 belongs in `docs/guide/` as the step you take when you have decided you do not
care about the document, which is a real and common decision and not the
default one.

The cost of that recommendation, stated: the blessed examples keep their
`app.html`, so `tests/init.rs:335` needs no change and neither do `walkthrough`,
`todo` or `ssr`. The cost of the *other* choice is one commit touching four
projects and one test — not large, but it is a corpus-wide shape change and it
should be the owner's, so it is §10.6.

Two smaller template notes:

- **`examples/todo` and `examples/walkthrough` should move to the builder**
  when (a) lands, even though `serve_service` keeps working (§4.6). They are
  the two files the owner transcribed from (§2.1), and they are the two that
  currently teach the dead end. `examples/ssr` and `examples/fullstack` are
  already on the builder and gain only `serve_build`.
- **The `browser` template is unaffected by (a) and (b) alike** — it has no
  server, so its `index.html` is loaded from the filesystem or a static host,
  and its coupling is checked by the substring assertions at
  `tests/init.rs:141-162`. It is, however, the one project in the tree that
  `hmr.md` §9's dev server would serve, so it is the thing that ties §5.10 to
  a real user.

## 7. Reconciliation with the ratified records

The charter asks for tensions to be addressed head-on. There are six, and each
is resolved by being precise about what was actually decided.

### 7.1 `ssr.md` §6(b) — the declined `render_into`

What was declined, verbatim:

> **(b) The splice API**: v1 keeps the shell splice in user code
> (`shell.replace("<!--app--></!--app-->", render(app()))` — recommendation: honest, zero
> new surface) vs a `render_into(shell, marker, view)` convenience in std.

Three things about that decline matter here. First, its stated reason is
**"zero new surface"** — a cost argument, not a correctness one, and one that
was correct at the time: a helper that takes a string, a marker and a view, and
performs a `replace`, buys a user nothing they cannot write, and buys std a
maintenance obligation. Second, it was scoped **to v1** of an arc whose subject
was rendering, not documents. Third, §5.8's proposal **is not `render_into`**.
It is a method on a `Document` value that this paper argues must exist for
seam (b)'s own reasons, and it takes **no marker at all** — the mount element
is a property of the document, so the failure mode the marker creates (F5) is
deleted rather than wrapped.

So the reconciliation is: `render_into(shell, marker, view)` stays declined,
and stays declined for the reason §6(b) gave. `Document::render(view)` is a
different thing, whose cost is already paid by the design that carries it, and
whose benefit is the removal of a silent failure that `ssr.md` did not have to
weigh because SSR's marker was one string in one example at the time and is now
the pattern every SSR app in the language copies.

If the owner disagrees — if the decline was about the *idea* of std knowing
what an HTML document is, not about the helper's shape — then §5's whole rung 2
falls, rung 1 and the validator stand alone, and the paper is still worth
having. That is §10.4.

### 7.2 `hmr.md` §8 — server-side HMR stays a permanent non-goal

> **Server-side HMR**: a non-goal, permanently — restart is the model for the
> Node leg; the process is cheap and correctness is free.

Nothing in this paper makes a running server's *code* replaceable. `serve_build`
changes where the asset bytes are read (from three `let`s at the top of `main`
to one library call in the request path), which is a question about **data
freshness**, not code identity — the distinction `dev-refresh.md` §0 already
drew and labelled: "A server that restarts on every code change can still serve
week-old bytes for a file it read once at the top of `main`; closing that gap
doesn't reopen the server-side-HMR question." This paper inherits that line
unchanged.

Two smaller §8 clauses are also respected: the dev channel keeps binding
`127.0.0.1` and serving only `dist/` artifacts, and nothing here asks it to
serve a user's page (§5.10 explicitly defers `hmr.md` §9's dev server).

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
