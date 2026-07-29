# Reactive UI: `std::reactive` + `std::ui`

Two browser components built on Vilan's state and view layers, with no server
involved. The guides ([Reactive state](../../docs/guide/reactive.md),
[Building UI](../../docs/guide/ui.md), [Styling](../../docs/guide/styling.md))
are the reference; this is the working code they describe.

## What it demonstrates

[`counter.vl`](counter.vl), the smallest component:

- **A component is a function returning a `View`.** An app is composition.
- **State is a `Signal`**; `count.map(..)` builds a *derived* signal whose
  dependency is **structural**: known when it is built, never discovered by
  running a body.
- **Bindings take a `Source`, not a tracking closure**: `bind_text(count.map(..))`.
- **`const` styles**: `style()` chains evaluated at build time, so the rules
  land in `app.css` and `.styled(..)` only sets classes.

[`todos.vl`](todos.vl), the fuller picture:

- **`combine`**: a two-input dependency (`items` and `filter`) yielding a
  source of the tuple; the combinator tree *is* the dependency graph.
- **`bind_each`**: a keyed reactive list, where rows move with their keys,
  only a changed row re-renders, and a removed row is disposed.
- **`bind_value`**: two-way binding on an `<input>`.
- **`show` vs `when`**: `show` toggles visibility with the node still mounted
  and its state intact; `when` mounts and unmounts content, so it is a disposal
  boundary for whatever the branch binds.

[`app.vl`](app.vl) is the entry: one `mount_root` per component, each root its
own disposal boundary.

## Build & run

```sh
vilan build .          # emits app.js + app.css beside index.html
npx serve .            # or any static server
```

The manifest declares `target = "browser"`, so no `--target` flag is needed.
`index.html` provides the `#counter` and `#todos` mount points and loads
`app.js` as a module. (Serve over HTTP rather than opening the file directly,
since browsers restrict ES modules over `file://`.) The emitted `app.js`/`app.css` are
generated and not checked in.
