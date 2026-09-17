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
    python3 scripts/regen-html-attributes.py           # TSV -> the Rust table
    python3 scripts/regen-html-attributes.py --check   # derive and diff, write nothing

Two arrows, one implementation each, and only the left one needs the network:

    the spec indices  --`--fetch`-->  html-attributes.tsv  --default-->  html_attributes.rs

`--fetch` is a DELIBERATE commit (the TSV header records the fetch date, the
source URLs and the sha256 of each page it read); the default mode and the gate
`crates/vilan-ide/src/html_attributes_sync.rs` are offline, so no CI job ever
touches the network. That gate is a SECOND implementation of the right arrow,
in Rust: it re-renders the table from the vendored TSV and diffs it byte for
byte, so this script and the gate are held equal by the suite. It is also what
`VILAN_REGENERATE_HTML_ATTRIBUTES=1 cargo test -p vilan-ide html_attributes`
rewrites the table instead of failing — the same entry point the mime table has.
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
TABLE = "crates/vilan-ide/src/html_attributes.rs"

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
        "# consumer:  python3 scripts/regen-html-attributes.py           (TSV -> Rust)",
        "#            crates/vilan-ide/src/html_attributes.rs is the checked-in table;",
        "#            its own `mod tests` regenerates from these rows and diffs, offline.",
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


def read_dataset(path: pathlib.Path) -> list[tuple[str, str, str]]:
    rows = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith("#") or not line.strip():
            continue
        element, attribute, source = line.split("\t")
        rows.append((element, attribute, source))
    return rows


# --- The Rust table ---------------------------------------------------------


def rust_list(name: str, doc: list[str], values: list[str]) -> list[str]:
    out = [*doc, f"pub const {name}: &[&str] = &["]
    out.extend(f'    "{value}",' for value in values)
    out.append("];")
    return out


def render_table(rows: list[tuple[str, str, str]]) -> str:
    globals_: set[str] = set()
    events: set[str] = set()
    svg_globals: set[str] = set()
    svg_elements: set[str] = set()
    per_element: dict[str, set[str]] = {}
    for element, attribute, source in rows:
        if source == SOURCE_SVG_ELEMENT:
            svg_elements.add(element)
            per_element.setdefault(element, set())
        elif source == SOURCE_HTML_EVENT:
            events.add(attribute)
        elif element == HTML_GLOBAL:
            globals_.add(attribute)
        elif element == SVG_GLOBAL:
            svg_globals.add(attribute)
        else:
            per_element.setdefault(element, set()).add(attribute)

    # `onclick` is the CONTENT attribute; vilan writes `on:click(…)`, so the
    # table holds what follows the colon.
    events = {event.removeprefix("on") for event in events}

    lines = [
        "//! Every attribute name the HTML and SVG specifications define, by element",
        "//! — the vocabulary vilan-ide's element-head completion offers at `<tag |>`",
        "//! (tracker E69).",
        "//!",
        "//! GENERATED FILE -- do not hand-edit. The dataset is `html-attributes.tsv`",
        "//! beside this file, vendored from the WHATWG HTML attribute index and the",
        "//! SVG 2 attribute index by `scripts/regen-html-attributes.py --fetch`; this",
        "//! file is what that script's DEFAULT mode makes of it. `html_attributes_sync`",
        "//! beside it is the GATE: a second, independent implementation of the same",
        "//! TSV -> table arrow, in Rust, which re-renders and diffs this file byte for",
        "//! byte. It is OFFLINE, so no gate ever reaches the network and refreshing the",
        "//! extract stays a deliberate commit.",
        "//!",
        "//! Rewrite after refreshing the dataset with",
        "//! `VILAN_REGENERATE_HTML_ATTRIBUTES=1 cargo test -p vilan-ide html_attributes`,",
        "//! or `python3 scripts/regen-html-attributes.py` — the two must agree, and the",
        "//! gate is what says so.",
        "//!",
        "//! # What this table is NOT",
        "//!",
        "//! It is not a source of truth about vilan. The element desugar stays",
        "//! NAME-BLIND (element-syntax.md §2, §9 item 3): `name(x)` lowers to",
        "//! `.attr(\"name\", x)` whatever `name` is, nothing here is consulted by the",
        "//! parser, the desugar, the analyzer or the formatter, and a `data-*` name",
        "//! absent from it costs one completion entry and nothing else. That is the",
        "//! disanalogy E67's refusal turned on — \"a second source of truth with",
        "//! nothing to gate it\" — and the generator plus the gate are that gate.",
        "",
    ]
    lines.extend(
        rust_list(
            "GLOBAL_ATTRIBUTES",
            [
                "/// The global attributes: every element takes them (the HTML attribute",
                "/// index's `HTML elements` rows).",
            ],
            sorted(globals_),
        )
    )
    lines.append("")
    lines.extend(
        rust_list(
            "SVG_GLOBAL_ATTRIBUTES",
            [
                "/// The SVG-wide vocabulary — the attribute index's section G.2",
                "/// presentation attributes — offered on every [`SVG_ELEMENTS`] tag.",
            ],
            sorted(svg_globals),
        )
    )
    lines.append("")
    lines.extend(
        rust_list(
            "EVENTS",
            [
                "/// The `GlobalEventHandlers` event names with `on` stripped: vilan spells",
                "/// the content attribute `onclick` as `on:click(…)`, so the table holds",
                "/// what follows the colon. The `body`/`Window` handlers are a different",
                "/// mixin and are not here.",
            ],
            sorted(events),
        )
    )
    lines.append("")
    lines.extend(
        rust_list(
            "SVG_ELEMENTS",
            [
                "/// The SVG-shaped tags: every element the SVG index names that the HTML",
                "/// index does not, plus the SVG root. These take",
                "/// [`SVG_GLOBAL_ATTRIBUTES`] on top of their own.",
            ],
            sorted(svg_elements),
        )
    )
    lines.append("")
    lines.extend(
        [
            "/// Each tag's OWN attributes as `(tag, attribute)` pairs, ascending — the",
            "/// globals above are not repeated, so a tag absent from this table takes",
            "/// only the globals. Sorted, so a lookup is a binary search and a diff of",
            "/// this file reads as a set difference; flat rather than nested so that",
            "/// every line is short enough for `cargo fmt` to leave it exactly where the",
            "/// generator put it (the gate compares the file BYTE for byte).",
            "pub const ELEMENT_ATTRIBUTES: &[(&str, &str)] = &[",
        ]
    )
    for element in sorted(per_element):
        wide = svg_globals if element in svg_elements else set()
        for attribute in sorted(per_element[element] - globals_ - wide):
            lines.append(f'    ("{element}", "{attribute}"),')
    lines.append("];")
    lines.append("")
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fetch", action="store_true", help="refresh the vendored TSV (network)")
    parser.add_argument("--check", action="store_true", help="derive and diff, write nothing")
    arguments = parser.parse_args()

    root = pathlib.Path(__file__).resolve().parent.parent
    dataset = root / DATASET
    table = root / TABLE

    if arguments.fetch:
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
        return

    rendered = render_table(read_dataset(dataset))
    if arguments.check:
        if table.read_text(encoding="utf-8") == rendered:
            print(f"{TABLE} is current with {DATASET}")
            return
        sys.exit(f"{TABLE} is not what {DATASET} derives — rerun without --check")
    table.write_text(rendered, encoding="utf-8")
    print(f"wrote {TABLE} ({len(rendered.splitlines())} lines)")


if __name__ == "__main__":
    main()
