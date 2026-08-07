# Notes: the docs walkthrough app

The app the book's [full-stack walkthrough](../../docs/guide/walkthrough.md)
builds, chapter by chapter: **Notes**, with sign-in, a note list that syncs
live between browser windows, and an editor that saves as you type. Every snippet in
that chapter is quoted from the files here, and the test suite builds this
example on every run, so the chapter cannot quietly rot.

## The shape

One package, two entries, the default full-stack shape (see
[Platforms](../../docs/tour/platforms.md)). The browser client and the Node
server build from the same source tree; platform coloring keeps each entry
honest about what it reaches, so there is no workspace, no `common`
library, and no path dependency to wire up.

```toml
[package]
name = "notes"

[entry.client]
target = "browser"

[entry.server]
# target defaults to node; path defaults to server.vl under src/
```

```
walkthrough/
  vilan.toml
  src/
    client.vl     the browser entry — connect, mirror, mount
    server.vl     the node entry — serve the bundle, mount the service
    store.vl      the service: SQLite, sessions, password hashing, [rpc] bodies
    notes.vl      the shared vocabulary — Note, AuthOutcome
    routes.vl     the Route enum plus parse/href
    views.vl      the UI and its const styles
    app.html      the HTML shell
```

`src/store.vl` uses `std::db` and `node:crypto` freely even though the browser
entry imports `NotesClient` from it: only the *generated* stub crosses, and the
server-colored bodies are unreachable from the client entry. That is the whole
argument for this shape: the compiler sorts the platforms out, so you don't
have to split the tree to do it.

## Run

```sh
cd vilan/examples/walkthrough
vilan run .             # builds both entries, then starts the server
                        # → http://localhost:4600
```

Open two browser windows side by side. Sign in, add a note in one window, and
watch it appear in the other. Open a note and type; the other window follows
keystroke by keystroke.

Or build without running:

```sh
vilan build .           # writes dist/client.js, dist/client.css, dist/server.mjs
node dist/server.mjs    # the server runs from the project root
```

`dist/` is generated and not checked in; the SQLite file (`notes.db`) is created
on first run and ignored too.
