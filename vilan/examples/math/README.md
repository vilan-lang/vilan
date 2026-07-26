# math — the smallest real package

What a Vilan package looks like once it outgrows a single file: a manifest, an
entry, a sibling module imported with `pkg::`, and a test file. Nothing here is
platform-specific, so it is also the shortest way to see `vilan run` and
`vilan test` work.

## What it demonstrates

- **A `[package]` manifest with `root = "."`** — sources live beside
  `vilan.toml` instead of under the default `src/`.
- **`pkg::` imports** — `main.vl` does `import pkg::square::sum_of_squares`. A
  module is just a file next to the entry; there is no module declaration.
- **`vilan test`** — `square_test.vl` is picked up by its `_test.vl` suffix. Its
  `main` runs `assert`s; a failed assert panics, which fails the test.

## Run

```sh
vilan run .      # 25
vilan test .     # ok    math/square_test.vl
```

`vilan build .` writes `main.js` beside the source; it is generated and not
checked in.
