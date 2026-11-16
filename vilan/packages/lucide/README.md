# lucide

[Lucide](https://lucide.dev)'s icons for `std::ui`: one function per icon, each
answering a `std::ui::Icon` to draw with the `icon` element.

```vilan
import lucide;
import std::gfx::dp;
import std::ui::{ Align, Element, icon, row, text };

fun saved(): Element {
	<row gap(dp(8)) align(Align::Center)>
		<icon of(lucide::check()) />
		<text>"Saved"</text>
	</row>.element()
}
```

- **What it is.** 1,999 icons from `lucide-static` **1.64.0** (the npm tarball,
  integrity
  `sha512-UxWmMteOXbKHI5E4f2qVtxCAAsI9gsInYK1lNw0nrOA0w4FkJeOVZ+UOV6aGA2377FZe3qyiFYq3lqjBKZ6OuA==`),
  every one an outline icon in a 24-unit box, stroked 2 units wide with round
  joins and caps (`Icon::stroked(data, 24.0, 2.0)`), drawn in the text colour
  around it. Deprecated alias names are left out: each icon is here once,
  under its current name.
- **Names.** Upstream's name with `-` as `_`: `arrow-up-right` is
  `lucide::arrow_up_right()`. A name that is a Vilan keyword takes a trailing
  underscore (`import` is `lucide::import_()`, `type` is `lucide::type_()`),
  and one that starts with a digit the prefix `icon_` (none does today).
- **Native only, to draw.** An icon is drawn by `std::ui`, which only the
  native backend builds: a JavaScript build of a program that draws one stops
  with "this program reaches `std::ui` … Build it with `vilan build --backend
  rust`". Reading an icon's data (`lucide::check().data`) builds on both.
- **What it costs.** Nothing for the icons a program does not call: the
  native backend emits only the functions a program reaches (five icons add
  about 500 bytes to a native binary). Checking a program that DECLARES the
  dependency parses and analyses all 1,999 functions, imported or not: with
  the release compiler at 1.64.0, about 0.06 s and 24 MB more than the same
  program without it.

## Depending on it

A path dependency in the program's `vilan.toml` (the path is relative to that
manifest):

```toml
[package.dependencies]
lucide = { path = "../../packages/lucide" }
```

then `import lucide;` and call `lucide::<name>()`, as
`vilan/examples/ui-icons` does.

## Layout

| File | What it is |
|---|---|
| `vilan.toml` | a `[library]` named `lucide`; `src/icons` is its `generated` root, so `vilan fmt` and the editor's reading aids leave it alone |
| `src/lib.vl` | written by hand: re-exports every icon under the package's name |
| `src/icons/lib.vl` | GENERATED: one `export fun <name>(): Icon` per icon |
| `icons.tsv` | GENERATED: the same icons as data (upstream name, Vilan name, box, paint, path data), read by the GPU crate's sweep |
| `LICENSE` | upstream's licence, as it ships it |

## Regenerating

The generator is `scripts/lucide-native.mjs` (Node, no packages). It refuses
any icon it cannot flatten faithfully (a transform, a group, opacity, a fill
other than Lucide's dots, a per-element stroke) and writes nothing then.

```sh
# Fetch the pinned tarball OUTSIDE the repository and check it.
mkdir -p ~/lucide && cd ~/lucide
curl -sSfO https://registry.npmjs.org/lucide-static/-/lucide-static-1.64.0.tgz
echo "sha512-$(openssl dgst -sha512 -binary lucide-static-1.64.0.tgz | base64 -w0)"
#   must print the integrity above
tar -xzf lucide-static-1.64.0.tgz

# From the repository root:
node scripts/lucide-native.mjs generate ~/lucide/package vilan/packages/lucide
```

Regenerating at the pinned version changes nothing. To move to a new version,
change `UPSTREAM` in the script (the version and the registry's integrity),
fetch that tarball, regenerate, copy its `LICENSE` here, update this file, and
run the gates that hold the package:

- `cargo test -p vilan-cli --test lucide_package` (the source is the
  generator's rendering of the data, and it checks clean),
- `scripts/ci-local.sh gfx`, whose `lucide_sweep` draws every icon at 16, 24
  and 48 dp against a reference rasteriser; read its numbers with
  `VILAN_LUCIDE_SWEEP_REPORT=<dir> cargo test --lib lucide_sweep -- --nocapture`
  in `crates/vilan-rt-gfx`.

## Licence

Lucide is ISC-licensed; the icons it derives from Feather are under Feather's
MIT licence. `LICENSE` is upstream's file, both notices and the list of
Feather-derived icons, and ships with every copy of this package.

Icon fonts (Material Symbols, Font Awesome) work in `std::ui` without a package:
register the font with `register_font` and draw the code point as text. They are
not the recommended route (`vilan/docs/std/ui.md`, Icons).
