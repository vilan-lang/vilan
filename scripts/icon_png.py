#!/usr/bin/env python3
"""Render the VS Code extension's PNG icon from the brand master.

The marketplace will not take an SVG (128x128 PNG minimum, 256 recommended),
so `editors/vscode/icon.png` is a derived raster — this script is how it is
derived, the same arrangement `ascii_logo.py` has with the CLI's upgrade
banner. Regenerate after any change to the mark:

    python3 scripts/icon_png.py

Source is `assets/branding/dark_icon.svg` — the simplified two-shape mark the
branding README designates for small sizes and editor icons, in its dark
variant (pale-lavender fill), composited onto the palette's deep indigo. The
ground is not decoration: marketplace icons render against a white gallery
card *and* against the user's editor theme, and a transparent asset can only
read on one of the two. Lavender on indigo is the brand's own dark pairing
(12.4:1) and matches the listing's `galleryBanner`.

Stdlib only, and exactly so: the mark is pure straight-edge polygons, which
means a scanline fill is not an approximation of a "real" rasterizer's output
but the same answer — coverage is computed exactly across each row and
sampled `--supersample` times down it. A regenerate command that needs a
rasterizer nobody has installed is a provenance note that rots.
"""

import argparse
import pathlib
import re
import struct
import zlib

REPO = pathlib.Path(__file__).resolve().parent.parent
SVG = REPO / "assets" / "branding" / "dark_icon.svg"
PNG = REPO / "editors" / "vscode" / "icon.png"

# "Deep indigo — dark page background" in the branding README's palette.
GROUND = "#110C31"


def load_paths():
    """The SVG's filled polygons, in document order, as (points, rgb)."""
    paths = []
    for data, fill in re.findall(r'<path d="([^"]+)"\s+fill="(#[0-9A-Fa-f]{6})"', SVG.read_text()):
        coordinates = [float(value) for value in re.findall(r"-?\d+\.?\d*", data)]
        paths.append((list(zip(coordinates[0::2], coordinates[1::2])), rgb(fill)))
    if not paths:
        raise SystemExit(f"no filled <path> elements in {SVG} — has the master changed shape?")
    return paths


def view_box():
    width, height = re.search(r'viewBox="0 0 (\d+) (\d+)"', SVG.read_text()).groups()
    if width != height:
        raise SystemExit(f"{SVG} is not square ({width}x{height}) — the icon canvas is")
    return float(width)


def rgb(color):
    return tuple(int(color[index : index + 2], 16) for index in (1, 3, 5))


def coverage(polygon, size, scale, supersample):
    """Per-pixel area coverage of one polygon, exact across a row."""
    covered = [0.0] * (size * size)
    for sample in range(size * supersample):
        y = (sample + 0.5) / (supersample * scale)
        crossings = []
        count = len(polygon)
        for index in range(count):
            x1, y1 = polygon[index]
            x2, y2 = polygon[(index + 1) % count]
            if (y1 > y) != (y2 > y):
                crossings.append(x1 + (y - y1) * (x2 - x1) / (y2 - y1))
        crossings.sort()
        row = (sample // supersample) * size
        for left, right in zip(crossings[0::2], crossings[1::2]):
            left, right = left * scale, right * scale
            for pixel in range(max(0, int(left)), min(size, int(right) + 1)):
                span = min(right, pixel + 1) - max(left, pixel)
                if span > 0:
                    covered[row + pixel] += span / supersample
    return covered


def render(size, supersample):
    scale = size / view_box()
    pixels = [list(rgb(GROUND)) for _ in range(size * size)]
    for polygon, fill in load_paths():
        for index, amount in enumerate(coverage(polygon, size, scale, supersample)):
            if amount > 0:
                amount = min(1.0, amount)
                pixel = pixels[index]
                for channel in range(3):
                    pixel[channel] = round(pixel[channel] * (1 - amount) + fill[channel] * amount)
    return pixels


def png(pixels, size):
    raw = bytearray()
    for row in range(size):
        raw.append(0)  # filter type 0 (None) — the image is tiny, filtering buys nothing
        for pixel in pixels[row * size : (row + 1) * size]:
            raw.extend(pixel)

    def chunk(kind, data):
        return (
            struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
        )

    header = struct.pack(">IIBBBBB", size, size, 8, 2, 0, 0, 0)  # 8-bit truecolor, no interlace
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--size", type=int, default=256)
    parser.add_argument("--supersample", type=int, default=8, help="vertical samples per pixel")
    parser.add_argument("--out", type=pathlib.Path, default=PNG)
    arguments = parser.parse_args()

    arguments.out.write_bytes(png(render(arguments.size, arguments.supersample), arguments.size))
    print(f"{arguments.out.relative_to(REPO)}: {arguments.size}x{arguments.size} from {SVG.name}")


if __name__ == "__main__":
    main()
