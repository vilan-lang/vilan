# watch: a program that is supposed to never end

`std::fs::Watcher` polls a path and hands changes over one at a time through
`next`, which parks until there is one. The loop below has no exit condition,
which is what a watch IS — it ends when you stop the process.

## What it demonstrates

- **The pull shape.** `watcher.next()` returns the next `Change` into a scope
  that can already own a `File`; there is no callback to register. A handler
  written as `|Change| void` could not await the read it was just told about,
  which is why the surface is a value and not an observer.
- **`Change` and `ChangeKind`.** `change.path` is addressable — the watched
  root joined with the entry — so it is the string to hand straight to
  `read_file_to_str` or `File::open`. The three variants are matched with no
  catch-all; a rename arrives as `Removed` plus `Created` in one batch.
- **`watch_all` against `watch`.** Recursive against the immediate entries,
  the same distinction `read_dir_all` makes against `read_dir`.

## Run

```sh
vilan run .      # watching . — edit a file here, Ctrl-C to stop
```

Then, in another shell, `touch main.vl` — the change prints as it lands. The
program does not stop on its own; that is the point of it.

`vilan build .` writes `main.mjs` beside the source; it is generated and not
checked in.
