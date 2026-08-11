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
