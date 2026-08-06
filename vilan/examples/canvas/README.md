# Canvas example: a host surface nobody typed

A browser client that draws on a `<canvas>` — fills, a gradient, arcs, text,
and a click that repaints — where **every host call goes through generated
bindings**. `canvas.vl` is `vilan bindgen` output, checked in and reviewed like
any other source. `board.vl` is the program written against it.

| file | what it is |
|---|---|
| `canvas.d.ts` | the TypeScript declaration file, the input |
| `canvas.vl` | **generated** — 345 lines, 0 typed by hand |
| `board.vl` | the program, plus **one** hand-written binding |
| `index.html` | the page that mounts `<canvas id="board">` |

## Regenerating the bindings

```sh
vilan bindgen canvas.d.ts --platform browser --only HTMLCanvasElement -o canvas.vl
```

That command is the whole example. It is not a build step: `vilan build .`
never runs it, and `canvas.vl` is committed source you may edit — narrow an
`f64` that is really an `i32`, delete a binding you do not want — without
anything undoing you.

It also demonstrates both halves of bindgen v2:

- **`--only HTMLCanvasElement`** emits that type and the transitive closure of
  what its signatures reach — through `extends`, through member types, through
  a closure parameter's own type. `MediaError`, `HTMLAudioElement`, and `Audio`
  sit in the same declaration file and are **not** in the output: nothing the
  canvas surface names reaches them. `EventTarget` and `HTMLElement` are not in
  the output either, for a different reason — their members are *flattened
  into* `HTMLCanvasElement` (vilan has no struct inheritance), and no emitted
  signature mentions either name.
- **The constructor idiom.** `canvas.d.ts` states the static side of each type
  the way `lib.dom.d.ts` does, as a `declare var` whose object type carries a
  construct signature. bindgen recognizes that shape and emits
  `[extern(new, "…")]`, so `HTMLCanvasElement::new()` exists. `Image` shows the
  aliased form — `new Image(…)` yields an `HTMLImageElement`, so the binding
  lands on `HTMLImageElement` as `new_image`, beside that type's own `new`.

## The one hand-written line

```vilan,fragment
[extern("document.getElementById")]
[platform("browser")]
external fun canvas_by_id(id: str): HTMLCanvasElement;
```

`declare var document: Document` is a global **value**, and no `[extern(…)]`
form reads one: they bind a call, or a property of a receiver. So *reaching*
the first object is still a human's line. That is the honest residue — a
handful of entry points, not a surface. Once this program holds a canvas,
every other call it makes is generated.

## Why the declaration file is written here rather than taken from `lib.dom.d.ts`

Because the answer was measured, and it is not flattering to the alternative.
Run against the real `lib.dom.d.ts` (39,429 lines, TypeScript 5.9.3):

| | declarations | generated lines |
|---|---|---|
| whole file | 2,229 / 2,415 bound | 492,986 |
| `--only HTMLCanvasElement` | 1,001 | 96,316 |

`--only` removes 80% of the output and every declaration it keeps is genuinely
reachable — but 96k lines is not a file anyone reviews. The DOM's element types
are one strongly-connected component: a `Node` has an `ownerDocument`, which is
a `Document`, which reaches almost everything; a `UIEvent` has a `view`, which
is a `Window`, which reaches the rest. Seeding from `Element`, `Node`,
`HTMLElement`, or `MouseEvent` all land on the same ~900 declarations. The
filter is not the limitation — the type graph is.

Which is the same conclusion `proposal/bindgen.md` §7 reaches from the other
direction: bindgen is for the library `std` does not wrap, and for a
canvas-shaped consumer the useful move is a declaration file scoped to what you
actually use. `canvas.d.ts` is that: written in `lib.dom.d.ts`'s own shape, so
the pipeline it exercises is the real one.

## Build

```sh
vilan build .
```

The manifest declares `target = "browser"`, so no `--target` flag is needed.
This emits `board.js`, an ES module that uses DOM globals with no Node host
imports.

## Run

Serve the directory and open `index.html` (a `file://` URL will not load an ES
module):

```sh
python3 -m http.server 8000
```

Then visit <http://localhost:8000/>. Click the canvas to repaint it.
