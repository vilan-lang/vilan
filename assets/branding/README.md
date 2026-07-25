# vilan branding

> **Licensing:** unlike the rest of the repository, the files in this
> directory are **not** MIT/Apache-2.0 — see [LICENSE](LICENSE) in this
> directory. Short version: unmodified use to refer to vilan is fine;
> derivatives and reuse as another project's branding are not.

The brand system for vilan. Everything here derives from the vector masters in
this directory; there are no canonical raster files — export bitmaps from the
SVGs when a consumer needs them (see "Exporting" below).

## Palette

| Color | Hex | Role |
|---|---|---|
| Deep indigo | `#110C31` | Mark/text fill on light backgrounds; dark page background |
| Pale lavender | `#D4CDFF` | Mark/text fill on dark backgrounds |
| Magenta | `#F3145B` | Accent, used only as the glow variants' drop shadow (75% opacity, blur σ=6, dy=2) |

The ambient glow in the effect files is pure white at 15% (dark theme) or pure
black at 15% (light theme), blur σ=16. Contrast: lavender on indigo ≈ 12.4:1,
indigo on white ≈ 18.6:1 — both AAA.

## Files

Every asset comes in a `dark_*` (for dark backgrounds: lavender fill) and
`light_*` (for light backgrounds: indigo fill) variant. All backgrounds are
transparent.

| File | What it is |
|---|---|
| `{dark,light}_logo_flat.svg` | The mark, paths only. **The master.** Safe in any pipeline; derive icons and print assets from this. |
| `{dark,light}_logo_glow.svg` | The mark with the full effect stack. Browser-only (see below). |
| `{dark,light}_wordmark_flat.svg` | The VILAN wordmark, paths only. |
| `{dark,light}_wordmark_glow.svg` | Wordmark with effects. Browser-only. |
| `{dark,light}_logo_beside_text.svg` | Horizontal lockup (mark + wordmark) with effects; used by the repo README header. |
| `{dark,light}_icon.svg` | Simplified two-shape mark for small sizes (≤32 px): detached stroke dropped, strokes thickened. Use for favicons and editor icons; the full mark turns to mush at 16 px. |
| `social_preview.svg` | 1280×640 social/og-image card (dark theme). Export to PNG in a browser and upload under repo Settings → Social preview. |

## Rules

- **Pink drop shadow appears on every glow variant except the light wordmark.**
  That one combination looked wrong and is deliberately shadow-free; don't
  "fix" it back. The exemption is per *treatment*, so it follows the light
  wordmark into composed assets: in `light_logo_beside_text.svg` the mark
  half carries the shadow and the wordmark half deliberately does not.
  The flat files never carry effects.
- **Glow SVGs are browser-only.** The effect stack uses `feTurbulence` +
  `feDisplacementMap`, which most CLI rasterizers (cairosvg, older librsvg,
  thumbnailers) silently drop or mangle. Anything that needs a bitmap of a
  glow variant must be exported from a real browser (or Figma).
- **Keep 64 units of clearance.** In the four standalone glow files the
  viewBox extends 64 units past the geometry on every side (worst-case
  effect spread is ~60: 3σ of the 16-blur + the 0.5 pre-blur + half the
  displacement scale) and the filter region equals the viewBox. In the
  *composed* files (the lockups and `social_preview.svg`) each group's
  filter region is expressed in that group's **local, pre-transform**
  coordinates — the transform sits on an outer `<g>`, the filter on an
  inner one — so keep the same ≥64 *local* clearance per group and, after
  any geometry or radius edit, re-check that every transformed
  geometry-plus-spread still fits the canvas. Getting the region's
  coordinate space wrong is the easy mistake here.
- **Small sizes use the icon files.** At 32 px and below, always the
  simplified `*_icon.svg`, never a downscale of the full mark.
- Theme switching in Markdown/HTML uses the `<picture>` element — see the
  repo README's header for the pattern.

## Lockup proportions

In `*_logo_beside_text.svg` the wordmark group is placed with
`translate(489.24 64.02) scale(1.18)`: wordmark scaled 1.18× about its own
center, left edge 112 units right of the mark's bounding box, vertically
centered on the mark (center y = 128). Chosen over the 1.00×/gap-128 original
after an A/B comparison — the smaller wordmark read as a caption next to the
mark's visual mass.

## Exporting

Open the SVG in a browser and capture at the needed scale (devtools
screenshot, or print-to-PDF for vector handoff). For flat files any SVG
rasterizer is fine. The VS Code marketplace requires PNG icons — export
`dark_icon.svg` at 128×128 when the extension adopts the brand.

The CLI's post-upgrade banner (`UPGRADE_LOGO` in
`crates/vilan-cli/src/upgrade.rs`) is half-block art generated from
`dark_logo_flat.svg` by `scripts/ascii_logo.py` — regenerate with
`python3 scripts/ascii_logo.py --rust` after any change to the mark.
