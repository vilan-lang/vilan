# Full-stack example: the workspace shape

This is the workspace teacher. A client + server app normally fits in
one package with two entries: that is the default shape, and
[`../walkthrough/`](../walkthrough/), [`../todo/`](../todo/) and
[`../ssr/`](../ssr/) all use it. This example deliberately keeps the other
form, a **`[project]` workspace** of separately-manifested members, which is
what you reach for once a codebase is large enough that its parts want their
own dependency sets, or once a shared library is published on its own.

The app itself is kept trivial so the *shape* is the lesson. Its root
`vilan.toml` lists three members, each with its own `vilan.toml` and target:

```toml
[project]
packages = ["common", "client", "server"]
```

- `common/`: a `[library]`. A library has no host `target`: it is compiled
  into each consumer, so this one (all core std) lands in both bundles. Both
  apps `import common::greeting`, and the platform gate rejects the library if
  it ever reaches for a Node- or browser-only module.
- `server/`: `[package] target = "node"`, depending on `common` via a `path`
  dependency. It reads the compiled client bundle once at startup and serves the
  HTML shell on every path, the bundle at `/client.js`, and a small API at
  `/api/hello` (whose body uses `common::greeting`).
- `client/`: `[package] target = "browser"`, also depending on `common`. It
  mounts into the server's `<div id="app">` and has a button that `fetch`es
  `/api/hello` and shows the reply: a live client→server round-trip.

## Run

```sh
vilan run .
```

This builds `dist/client.js` (browser) and `dist/server.mjs` (Node), then
starts the workspace's single `node` member, the server. Open
<http://localhost:3000>: the page loads the client bundle, which renders a
heading using the same `common::greeting` the server logs at startup.

Or build the bundles without running:

```sh
vilan build .          # writes dist/server.mjs + dist/client.js
node dist/server.mjs   # then run the server yourself
```

`dist/` is generated and not checked in.
