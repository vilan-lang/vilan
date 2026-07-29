# Browser example: raw `std::dom`

A Vilan client that runs in the browser, built directly on the `std::dom`
platform layer, with no reactive layer, components, or framework. This is
the floor everything else stands on. See [`../reactive-ui/`](../reactive-ui/)
for the same ideas expressed through `std::ui`.

## Build

```sh
vilan build .
```

The manifest declares `target = "browser"`, so no `--target` flag is needed.
This emits `client.js`, an ES module that uses DOM globals
(`document.createElement`, `addEventListener`, …) with no Node host imports and
no `process.exit`.

## Run

Open `index.html` in a browser. It provides the `<div id="app">` mount point and
loads `client.js` as a module. (If your browser restricts ES modules over
`file://`, serve the directory with any static server.)

You should see the heading, a live greeting that echoes whatever you type into
the name field (read via `query_selector` + `value` on each `input` event), a
"Clear" button that resets the field (`set_value`), and an "Add a note" button
whose paragraphs remove themselves when clicked (`remove`).

## Notes

- `client.vl` only imports `std::dom` and other universal (core) modules, so it
  compiles for `--target browser`. Importing a Node-layer module (`std::http`,
  `std::fs`, `std::process`) here is a compile error.
- The full-stack flow (a Vilan `std::http` server that serves this bundle
  from the same source tree) is [`../walkthrough/`](../walkthrough/).
