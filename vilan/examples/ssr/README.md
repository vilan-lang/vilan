# Server-side rendering (render and replace)

Server-side rendering ([the SSR guide](../../docs/guide/ssr.md)) in one
package with two entries, the default full-stack shape:

```
ssr/
  vilan.toml     [entry.client] target = "browser"; [entry.server]
  src/
    app.vl       the ONE `fun app(): View` both entries build
    client.vl    the browser entry — mount_root("app", || app())
    server.vl    the node entry — app() rendered into the checked shell
    app.html     the shell, with the mount element the render lands in
```

## What it demonstrates

- **One component, two rendered forms.** `src/app.vl` imports `std::ui`, which
  resolves per *entry*: the browser layer (live DOM) in the client leg, the
  process layer (an HTML string) in the server leg. Same source, no annotation,
  no conditional compilation. That per-entry shadow is the whole SSR mechanism.
- **Render, then replace.** The server renders `app()` to markup per request and
  `Document::render` splices it *inside the mount element* of `src/app.html` —
  the same `<div id="app">` the client mounts into, located when the shell was
  checked at boot, so there is no marker string in either file to keep in step.
  On boot the client builds the same view live and `mount_root` *clears* the
  container before appending it. There is no hydration: no node adoption, no
  mismatch errors, no second set of rules.
- **The shell is checked.** `require_shell("src/app.html", build)` holds it
  against what the client leg's build emitted; a shell that stopped matching
  stops the server instead of serving a page that cannot work.
- **Build pure, bind reactive.** Every binding here reads
  once on the server (the value at render time is the value served) and stays
  live on the client: a signal-fed list, an escaped heading, a `when` branch,
  and a button whose click writes a signal bound to its own text.

## Run

```sh
vilan run .
# open http://localhost:8791/
```

View the page source: the task list and heading are already in the HTML before
any script runs (first paint and SEO). Load it in a browser and the client boots
and replaces the server markup in place.

The data here is seeded in code, so both legs produce the same markup and the
swap is imperceptible. A real app fetches over rpc; see the guide's note on the
double-fetch v1 accepts.

`dist/` is generated and not checked in.
