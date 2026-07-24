# Server-side rendering (render and replace)

The A7 SSR model in three small packages (proposal/ssr.md, and the
[SSR guide](../../docs/guide/ssr.md)):

- **`common`** — a `[library]` holding the one `fun app(): View` both legs build.
  It imports `std::ui`, which resolves per platform: the browser layer (live DOM)
  in the client build, the process layer (an HTML string) in the server build —
  the same source, no annotation.
- **`client`** — the browser package. `main` is `mount_root("app", || app())`;
  on boot `mount` clears the container and mounts the live UI, *replacing* the
  server-rendered nodes.
- **`server`** — the node package. It renders `app()` to markup with `render`,
  splices it into `server/src/app.html` at the `<!--ssr-->` marker, and serves the
  page plus `dist/client.js`.

```sh
vilan run .
# open http://localhost:8791/
```

View the page source: the task list and heading are already in the HTML, before
any script runs — first paint and SEO. Load it in a browser and the client boots
and replaces the server markup in place. No hydration: the client renders fresh
and swaps.

The data here is seeded in code, so both legs produce the same markup and the swap
is imperceptible. A real app fetches over rpc; see the guide's note on the
double-fetch v1 accepts.
