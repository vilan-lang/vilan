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
