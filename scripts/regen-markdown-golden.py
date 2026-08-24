#!/usr/bin/env python3
"""Regenerate the book-wide anchor golden for std::markdown from a REAL
mdBook build (proposal/markdown.md §3: the anchor bar is bit-exact mdBook
v0.5.4 id parity, and the golden's authority is the renderer, never the
parser under test).

Usage, from the repo root (requires `mdbook` v0.5.4 on PATH):

    python3 scripts/regen-markdown-golden.py

Builds vilan/docs into a temp dir, extracts every rendered heading id in
document order, and rewrites crates/vilan-core/tests/markdown_anchors.golden
(one `page h<level> <id>` line per heading; pages in sorted path order;
SUMMARY.md and theme/ excluded — mdBook renders neither as a page).

Regenerate when a docs page's headings change, or when the mdBook pin
moves — and eyeball the diff either way: every changed line is a changed
URL anchor, a compatibility surface (LSP hovers, cross-page links).
`cargo test -p vilan-core --test markdown_golden` then proves the shipped
parser reproduces the file.
"""

import os
import re
import subprocess
import sys
import tempfile

PINNED_MDBOOK = "mdbook v0.5.4"

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DOCS = os.path.join(ROOT, "vilan", "docs")
GOLDEN = os.path.join(ROOT, "crates", "vilan-core", "tests", "markdown_anchors.golden")

HEADING = re.compile(r"<h([1-6]) id=\"([^\"]*)\"")


def rendered_pages():
    pages = []
    for dirpath, dirnames, filenames in os.walk(DOCS):
        dirnames[:] = [d for d in dirnames if d not in ("book", "theme")]
        for name in filenames:
            if name.endswith(".md") and name != "SUMMARY.md":
                relative = os.path.relpath(os.path.join(dirpath, name), DOCS)
                pages.append(relative.replace("\\", "/"))
    return sorted(pages)


def main():
    version = subprocess.run(
        ["mdbook", "--version"], capture_output=True, text=True, check=True
    ).stdout.strip()
    if version != PINNED_MDBOOK:
        sys.exit(
            f"this golden pins {PINNED_MDBOOK}; PATH has {version!r} — "
            "install the pinned renderer, or move the pin deliberately "
            "(update PINNED_MDBOOK here and the references in "
            "std/src/markdown.vl and markdown_golden.rs with it)"
        )
    with tempfile.TemporaryDirectory(prefix="vilan-markdown-golden-") as out:
        subprocess.run(
            ["mdbook", "build", DOCS, "--dest-dir", out], check=True, capture_output=True
        )
        lines = []
        for page in rendered_pages():
            stem = page[:-3]
            html_name = "index.html" if stem == "README" else stem + ".html"
            with open(os.path.join(out, html_name), encoding="utf-8") as rendered:
                for match in HEADING.finditer(rendered.read()):
                    lines.append(f"{page} h{match.group(1)} {match.group(2)}")
    with open(GOLDEN, "w", encoding="utf-8", newline="\n") as golden:
        golden.write("\n".join(lines) + "\n")
    print(f"{GOLDEN}: {len(lines)} anchors over {len(rendered_pages())} pages ({version})")


if __name__ == "__main__":
    main()
