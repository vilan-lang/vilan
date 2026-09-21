#!/usr/bin/env python3
"""Refresh the element -> attribute-name dataset that vilan-ide's element-head
completion is generated from (tracker E69).

The authority is the SPEC's own indices, not a list anybody typed:

  * the WHATWG HTML attribute index (`indices.html`, the two tables
    `attributes-1` "List of attributes" and `ix-event-handlers` "List of event
    handler content attributes") and its "List of elements" table, and
  * the SVG 2 attribute index (`attindex.html`, the per-element table and
    section G.2's presentation attributes),

which is what makes offering attribute names admissible at all. E67 refused a
HAND list on the ground that it "would be a second source of truth with nothing
to gate it"; a generated, gated extract is the answer the owner ruled for
(2026-09-14), and it is the same shape `scripts/regen-mime-table.py` +
`mime-table.tsv` + `mime_table_sync.rs` already ship for media types.

Usage, from the repo root:

    python3 scripts/regen-html-attributes.py --fetch   # network; rewrite the TSV

ONE arrow, one implementation, and this script is the half that needs the
network:

    the spec indices  --`--fetch`-->  html-attributes.tsv  --the Rust gate-->  html_attributes.rs

`--fetch` is a DELIBERATE commit: the TSV header records the fetch date, the
source URLs and the sha256 of each page it read, and nothing else in the
repository ever reaches the network.

The right arrow is `crates/vilan-ide/src/html_attributes_sync.rs` and nothing
else (tracker N107). This script used to carry a second implementation of it —
a default mode that rendered the TSV into the Rust table, held equal to the
gate by the suite. Held equal is not the same as not duplicated: two renderers
of one table is two places to edit when a column moves, and the gate is the one
that runs on every `cargo test`, so it is the one that stays. Regenerate the
table with

    VILAN_REGENERATE_HTML_ATTRIBUTES=1 cargo test -p vilan-ide html_attributes

which is the same entry point the mime table has.
"""

import argparse
import datetime
import hashlib
import pathlib
import re
import sys
import urllib.request

# The exact pages this extract is derived from. Bump with the TSV, never alone —
# the TSV header repeats them.
HTML_INDEX_URL = "https://html.spec.whatwg.org/multipage/indices.html"
SVG_INDEX_URL = "https://www.w3.org/TR/SVG2/attindex.html"

DATASET = "crates/vilan-ide/src/html-attributes.tsv"

# The three keys the `element` column takes besides a literal tag name.
HTML_GLOBAL = "*"
SVG_GLOBAL = "svg:*"

# The `source` column's vocabulary — which index a row was read out of, so a
# row can be traced back to the table that produced it.
SOURCE_HTML = "html-attribute-index"
SOURCE_HTML_EVENT = "html-event-handler-index"
SOURCE_SVG = "svg-attribute-index"
SOURCE_SVG_PRESENTATION = "svg-presentation-attributes"
SOURCE_SVG_ELEMENT = "svg-element-index"

# The element cell of a global row, verbatim as the index writes it.
HTML_ELEMENTS_CELL = "HTML elements"

# Cells that name a CLASS of elements rather than a tag: no completion key.
NOT_A_TAG = re.compile(r"^(form-associated custom elements|[Aa]ny element.*)$")


def strip_tags(fragment: str) -> str:
    return re.sub(r"\s+", " ", re.sub(r"<[^>]+>", "", fragment)).strip()


def fetch(url: str) -> tuple[str, str]:
    """(text, sha256) for one spec page."""
    request = urllib.request.Request(url, headers={"User-Agent": "vilan-regen-html-attributes"})
    with urllib.request.urlopen(request, timeout=120) as response:
        raw = response.read()
    return raw.decode("utf-8"), hashlib.sha256(raw).hexdigest()


# --- The HTML index ---------------------------------------------------------


def html_table(text: str, marker: str) -> str:
    """One `<table id=…>`'s body out of the multipage index."""
    start = text.find(marker)
    if start < 0:
        sys.exit(f"the HTML index has no {marker}")
    end = text.find("</table>", start)
    return text[start:end]


def html_attribute_rows(table: str) -> list[tuple[str, str]]:
    """(attribute, element-or-`*`) for one of the index's attribute tables."""
    rows: list[tuple[str, str]] = []
    for row in re.split(r"<tr>", table)[1:]:
        cells = re.split(r"<td>", row)
        if len(cells) < 2:
            continue
        name = re.search(r"<code>([^<]+)</code>", cells[0])
        if not name:
            continue
        attribute = name.group(1).strip()
        cell = cells[1]
        if strip_tags(cell) == HTML_ELEMENTS_CELL:
            rows.append((attribute, HTML_GLOBAL))
            continue
        for element in re.findall(r"<code[^>]*>(?:<a[^>]*>)?([^<]+)", cell):
            element = element.strip()
            if not element or NOT_A_TAG.match(element):
                continue
            rows.append((attribute, element))
    return rows


# --- The SVG index ----------------------------------------------------------


def svg_attribute_rows(text: str) -> list[tuple[str, str]]:
    """(attribute, element) for the SVG 2 per-element attribute table."""
    start = text.find('<table class="proptable attrtable">')
    if start < 0:
        sys.exit("the SVG index has no attribute table")
    end = text.find("</table>", start)
    rows: list[tuple[str, str]] = []
    for row in re.split(r"<tr>", text[start:end])[1:]:
        cells = re.split(r"<td>", row)
        if len(cells) < 2:
            continue
        attributes = re.findall(r'<span class="attr-name">.*?<span>([^<]+)</span>', cells[0])
        elements = re.findall(r'<span class="element-name">.*?<span>([^<]+)</span>', cells[1])
        for attribute in attributes:
            for element in elements:
                rows.append((attribute.strip(), element.strip()))
    return rows


def svg_presentation_attributes(text: str) -> list[str]:
    """Section G.2's presentation attributes — the SVG-wide vocabulary."""
    start = text.find('id="PresentationAttributes"')
    if start < 0:
        sys.exit("the SVG index has no presentation-attribute section")
    return [
        name.strip()
        for name in re.findall(
            r'<span class="attr-name">.*?<span>([^<]+)</span>', text[start:]
        )
    ]


# --- The dataset ------------------------------------------------------------

# A name a vilan element head can actually SPELL. The head's attribute
# production is a name token joined to further name tokens by span-adjacent
# hyphens (`parsing.rs::parse_element_name` — `div`, `type`, `aria-label`,
# `stroke-width`), so a NAMESPACED index entry (`xml:space`, `xlink:href`) has
# no head spelling at all and is dropped rather than offered as something that
# cannot be typed. This is the extract's one shaping rule; it is a fact about
# the vilan grammar, not a judgement about the spec.
SPELLABLE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*(-[A-Za-z0-9_]+)*$")

# The HTML element index's description of the element that roots an SVG
# subtree. `svg` is in BOTH indices — the HTML one lists it only to say this —
# and everything written in its head is SVG, so it counts as SVG-shaped even
# though the name collides. Read off the spec's own description rather than
# named here, so a spec that renames it fails the fetch instead of lying.
SVG_ROOT_DESCRIPTION = "SVG root"


def html_element_descriptions(text: str) -> dict[str, str]:
    """Tag name -> the "List of elements" table's Description cell."""
    start = text.find("<caption>List of elements</caption>")
    if start < 0:
        sys.exit("the HTML index has no list of elements")
    end = text.find("</table>", start)
    described: dict[str, str] = {}
    for row in re.split(r"<tr>", text[start:end])[1:]:
        cells = re.split(r"<td>", row)
        if len(cells) < 2:
            continue
        description = strip_tags(cells[1])
        for element in re.findall(r"<code[^>]*>(?:<a[^>]*>)?([^<]+)", cells[0]):
            element = element.strip()
            if element and not NOT_A_TAG.match(element):
                described[element] = description
    return described


def derive(html: str, svg: str) -> list[tuple[str, str, str]]:
    """(element, attribute, source) rows, deduplicated and sorted."""
    rows: set[tuple[str, str, str]] = set()

    attributes = html_table(html, "<table id=attributes-1>")
    for attribute, element in html_attribute_rows(attributes):
        if SPELLABLE.match(attribute):
            rows.add((element, attribute, SOURCE_HTML))

    events = html_table(html, "<table id=ix-event-handlers>")
    for attribute, element in html_attribute_rows(events):
        # GlobalEventHandlers only: the `body`/`Window` handlers are a
        # different mixin, and `on:` in an element head is not where they go.
        if element == HTML_GLOBAL and SPELLABLE.match(attribute):
            rows.add((element, attribute, SOURCE_HTML_EVENT))

    described = html_element_descriptions(html)
    svg_rows = svg_attribute_rows(svg)
    # SVG-shaped: an element the SVG index names that the HTML index does not,
    # plus the SVG root itself. `a`, `audio`, `canvas`, `iframe`, `script`,
    # `style`, `title` and `video` are in both indices and are written as the
    # HTML elements they are, so the SVG vocabulary is not poured over them.
    shaped = {element for _, element in svg_rows if element not in described}
    shaped |= {
        element
        for element, description in described.items()
        if description == SVG_ROOT_DESCRIPTION
    }
    if not shaped:
        sys.exit("the SVG index named no element the HTML index does not")
    for element in sorted(shaped):
        rows.add((element, "", SOURCE_SVG_ELEMENT))
    for attribute, element in svg_rows:
        # An SVG `on…` row is an event handler CONTENT attribute; vilan writes
        # those `on:event(…)` and the event table above is where they live.
        if element in shaped and not attribute.startswith("on") and SPELLABLE.match(attribute):
            rows.add((element, attribute, SOURCE_SVG))
    for attribute in svg_presentation_attributes(svg):
        if SPELLABLE.match(attribute):
            rows.add((SVG_GLOBAL, attribute, SOURCE_SVG_PRESENTATION))

    return sorted(rows)


def render(rows: list[tuple[str, str, str]], provenance: list[str]) -> str:
    header = [
        "# The element -> attribute-name dataset vilan-ide's element-head completion",
        "# is generated from (tracker E69). GENERATED FILE -- do not hand-edit.",
        "#",
        "# refresh:   python3 scripts/regen-html-attributes.py --fetch   (network)",
        "# consumer:  crates/vilan-ide/src/html_attributes_sync.rs   (TSV -> Rust)",
        "#            the ONE implementation of that arrow (N107). It renders",
        "#            html_attributes.rs from these rows and diffs it byte for byte on",
        "#            every `cargo test`, offline; VILAN_REGENERATE_HTML_ATTRIBUTES=1",
        "#            rewrites the table instead of failing.",
        "#",
        "# `element` is a tag name, `*` (every element) or `svg:*` (every SVG-shaped",
        "# element). A row with an EMPTY attribute registers the tag and nothing else.",
        "# Only names a vilan element head can spell are extracted: a namespaced entry",
        "# (`xml:space`) has no head spelling, and an SVG `on…` row is an event handler,",
        "# which the `*`/html-event-handler-index rows carry in their own spelling.",
        "#",
    ]
    header.extend(f"# {line}" for line in provenance)
    header.append("#")
    header.append("# element\tattribute\tsource")
    body = [f"{element}\t{attribute}\t{source}" for element, attribute, source in rows]
    return "\n".join(header + body) + "\n"


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Refresh the vendored html-attributes.tsv from the spec indices.",
        epilog=(
            "The TSV -> Rust table arrow is `VILAN_REGENERATE_HTML_ATTRIBUTES=1 "
            "cargo test -p vilan-ide html_attributes`, which is its one "
            "implementation (tracker N107)."
        ),
    )
    parser.add_argument(
        "--fetch",
        action="store_true",
        required=True,
        help="refresh the vendored TSV from the spec indices (network)",
    )
    parser.parse_args()

    root = pathlib.Path(__file__).resolve().parent.parent
    dataset = root / DATASET

    html, html_hash = fetch(HTML_INDEX_URL)
    svg, svg_hash = fetch(SVG_INDEX_URL)
    provenance = [
        f"fetched:   {datetime.date.today().isoformat()}",
        f"source:    {HTML_INDEX_URL}",
        f"           sha256 {html_hash}",
        f"source:    {SVG_INDEX_URL}",
        f"           sha256 {svg_hash}",
    ]
    rendered = render(derive(html, svg), provenance)
    dataset.write_text(rendered, encoding="utf-8")
    print(f"wrote {DATASET} ({len(rendered.splitlines())} lines)")
    print(
        "now rebuild the table: "
        "VILAN_REGENERATE_HTML_ATTRIBUTES=1 cargo test -p vilan-ide html_attributes"
    )


if __name__ == "__main__":
    main()
