# vilan branding (vendored)

> **Licensing:** unlike the rest of the repository, the files in this
> directory are **not** MIT/Apache-2.0 — see [LICENSE](LICENSE) in this
> directory. Short version: unmodified use to refer to vilan is fine;
> derivatives and reuse as another project's branding are not.

These files are **vendored copies** from the private `vilan-lang/branding`
repository, which holds the design masters, the palette, and the bake
pipeline. Nothing here is edited in place — change the brand there, re-bake,
and re-vendor.

## Palette

| Color | Hex | Role |
|---|---|---|
| Primary light (blush) | `#F9DFE7` | Ink on dark grounds; light background |
| Primary dark | `#120004` | Ink on light grounds; dark background |

(The brand also carries four gradient-bloom accent colors; they live with the
masters and are not used in this repository.)

## Files

Prefixes name the **ink**: `light_*` is drawn in primary light (use on dark
backgrounds), `dark_*` in primary dark (use on light backgrounds).

| File | What it is |
|---|---|
| `{light,dark}_logo_flat.svg` | The mark, clean vector geometry — what `scripts/ascii_logo.py` renders the CLI's upgrade banner from. |
| `{light,dark}_lockup.png` | Baked mark-above-wordmark lockup, transparent ground — the repo README's header, via a `<picture>` element (`light_` for dark themes and vice versa). |

Two more vendored rasters live outside this directory: the VS Code
extension's `editors/vscode/icon.png` (the bake pipeline's `icon_256.png` —
the light mark on the primary-dark ground, opaque so it reads on both the
white gallery card and a dark editor theme, pinned by
`crates/vilan-cli/tests/vscode_extension.rs`) and the `galleryBanner` color
`#120004` in `editors/vscode/package.json`.

The CLI's post-upgrade banner (`UPGRADE_LOGO` in
`crates/vilan-cli/src/upgrade.rs`) is half-block art generated from
`light_logo_flat.svg` by `scripts/ascii_logo.py` — regenerate with
`python3 scripts/ascii_logo.py --rust` after any change to the mark. Its
color is the brand's primary light (`Style::BLUSH` in
`crates/vilan-cli/src/paint.rs`).
