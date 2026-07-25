#!/usr/bin/env python3
"""Generate the terminal (half-block) rendering of the vilan mark.

Reads the brand master `assets/branding/dark_logo_flat.svg` (pure straight-edge
polygons) and rasterizes it onto a character grid, two vertical subcells per
row, emitted as ` ▀▄█` — glyphs present even in CP437 legacy Windows consoles.
Terminal cells are treated as 1:2 (width:height), so subcells are square and
the mark keeps its true aspect ratio.

    python3 scripts/ascii_logo.py            # print the art
    python3 scripts/ascii_logo.py --rust     # print the Rust const for the CLI

The `--rust` output is the source of truth for `UPGRADE_LOGO` in
`crates/vilan-cli/src/upgrade.rs`; regenerate and paste it whenever the mark
or the parameters below change. Defaults (44 columns, threshold 0.35) were
chosen by eye: 44 fits an 80-column terminal with room for a caption, and
0.35 keeps the thin stroke tips connected. Below ~32 columns the full mark
stops reading — don't shrink it, that is what the simplified icon SVGs are
for.
"""

import argparse
import pathlib
import re

REPO = pathlib.Path(__file__).resolve().parent.parent
SVG = REPO / "assets" / "branding" / "dark_logo_flat.svg"


def load_polygons():
    polygons = []
    for data in re.findall(r'd="([^"]+)"', SVG.read_text()):
        coordinates = [float(value) for value in re.findall(r"-?\d+\.?\d*", data)]
        polygons.append(list(zip(coordinates[0::2], coordinates[1::2])))
    return polygons


def covered(polygons, x, y):
    for polygon in polygons:
        inside = False
        count = len(polygon)
        for i in range(count):
            x1, y1 = polygon[i]
            x2, y2 = polygon[(i + 1) % count]
            if (y1 > y) != (y2 > y) and x1 + (y - y1) * (x2 - x1) / (y2 - y1) > x:
                inside = not inside
        if inside:
            return True
    return False


def render(columns, threshold, supersample=5):
    polygons = load_polygons()
    xs = [x for polygon in polygons for x, _ in polygon]
    ys = [y for polygon in polygons for _, y in polygon]
    min_x, max_x, min_y, max_y = min(xs), max(xs), min(ys), max(ys)

    cell_width = (max_x - min_x) / columns
    subrows = int(round((max_y - min_y) / cell_width))
    if subrows % 2:
        subrows += 1

    glyphs = {(False, False): " ", (True, False): "▀", (False, True): "▄", (True, True): "█"}
    lines = []
    for row in range(0, subrows, 2):
        line = ""
        for column in range(columns):
            halves = []
            for half in (0, 1):
                hits = 0
                for sub_y in range(supersample):
                    for sub_x in range(supersample):
                        x = min_x + (column + (sub_x + 0.5) / supersample) * cell_width
                        y = min_y + (row + half + (sub_y + 0.5) / supersample) * cell_width
                        if covered(polygons, x, y):
                            hits += 1
                halves.append(hits / (supersample * supersample) >= threshold)
            line += glyphs[tuple(halves)]
        lines.append(line.rstrip())
    return lines


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cols", type=int, default=44)
    parser.add_argument("--threshold", type=float, default=0.35)
    parser.add_argument("--rust", action="store_true", help="emit the Rust const")
    arguments = parser.parse_args()

    lines = render(arguments.cols, arguments.threshold)
    if not arguments.rust:
        print("\n".join(lines))
        return

    print("/// The vilan mark as half-block art, shown once after a successful")
    print("/// `vilan upgrade`. Rows are rasterized from")
    print("/// assets/branding/dark_logo_flat.svg — do not hand-edit; regenerate this")
    print("/// whole block with `python3 scripts/ascii_logo.py --rust`.")
    print("///")
    print("/// `concat!` of one literal per row — never a `\"\\` line-continuation")
    print("/// literal: a trailing `\\` in a Rust string skips the newline **and all")
    print("/// following whitespace**, which silently eats each row's leading")
    print("/// indentation and flush-lefts the mark (pinned by")
    print("/// `the_mark_is_eleven_clean_lines_of_half_blocks`).")
    print("const UPGRADE_LOGO: &str = concat!(")
    for index, line in enumerate(lines):
        newline = "" if index == len(lines) - 1 else "\\n"
        print(f'    "{line}{newline}",')
    print(");")


if __name__ == "__main__":
    main()
