#!/usr/bin/env python3
"""Refresh the extension -> media type dataset that `std::build`'s content-type
table is generated from.

The authority is `mime-db` (https://github.com/jshttp/mime-db, MIT), the same
dataset vite's `mrmime` is generated from -- it aggregates the IANA, Apache and
nginx registries, and it is the reason no media type in this tree is typed from
memory. `mrmime` itself is NOT the authority: it drops every `vnd.`/`x-` type,
which is why it has no row for `.ico` at all, and a favicon is exactly the case
that sent kolt to a hand-rolled table (kolt.local 022).

Usage, from the repo root (requires network, or `--db`):

    python3 scripts/regen-mime-table.py            # fetch mime-db, rewrite the dataset
    python3 scripts/regen-mime-table.py --db X.json  # derive from a local db.json
    python3 scripts/regen-mime-table.py --check    # derive and diff, write nothing

Reads   mime-db's `db.json` (PINNED_MIME_DB below).
Writes  crates/vilan-core/tests/mime-table.tsv  (checked into git).

That TSV is a DATASET, not the table: the vilan `match` arms in
`vilan/std/src/process/build.vl` are generated FROM it, in Rust, by the gate
`crates/vilan-core/tests/mime_table_sync.rs` -- which is also what regenerates
them and what fails when they drift. So this script runs rarely (when mime-db
publishes) and that gate runs on every `cargo test`. After running this, eyeball
the diff, then regenerate the table:

    VILAN_REGENERATE_MIME_TABLE=1 cargo test -p vilan-core --test mime_table_sync

Bump PINNED_MIME_DB in the same commit as the dataset it produced.
"""

import argparse
import json
import pathlib
import subprocess
import sys
import tarfile
import tempfile

# The exact upstream this dataset was derived from. Bump with the TSV, never
# alone -- the TSV header repeats it, and the gate holds the two equal.
PINNED_MIME_DB = "mime-db 1.54.0"

OUTPUT = "crates/vilan-core/tests/mime-table.tsv"

# --- The curation -----------------------------------------------------------
#
# mime-db knows 2522 media types and roughly a thousand extensions. `serve_build`
# does not want them: it serves A BUILD, not a directory (fullstack-dx.md 5.10),
# and a row for `.dwg` would be surface with no caller. The rows below are the
# ones a browser build can EMIT, plus the ones a page it serves can reference as
# a sub-resource that is loaded WHOLE -- <script>, <link>, <img>, @font-face,
# fetch, WebAssembly.instantiate, the manifest.
#
# Two exclusions are deliberate and worth stating, because they are the ones a
# reader will ask about:
#
#   * Audio and video are OUT. `serve_build` writes a whole body and honours no
#     `Range` header, so a browser could not seek in anything it served. A row
#     for `.mp4` would type a response that does not work; the absence is the
#     honest answer, and the fence already says where such a file belongs.
#   * Archives, office documents and executables are OUT. No build emits one and
#     no page loads one as a sub-resource.
#
# This list is PINNED by the gate: adding a row here without the gate's pinned
# copy agreeing is a red, in both directions.
CURATED = {
    "markup and code": ["css", "htm", "html", "js", "map", "mjs"],
    "data and text": ["csv", "json", "txt", "webmanifest", "xml"],
    "images": ["apng", "avif", "bmp", "gif", "ico", "jpeg", "jpg", "png", "svg", "webp"],
    "fonts": ["otf", "ttf", "woff", "woff2"],
    "other browser-native": ["pdf", "wasm"],
}

# Registry precedence when more than one media type claims an extension. IANA is
# the registry; Apache and nginx are what servers actually shipped before it.
SOURCE_RANK = {"iana": 0, "apache": 1, "nginx": 2}

# The one place a human overrules the dataset, and why.
#
# `.ico`: mime-db's IANA row is `image/vnd.microsoft.icon`, which is correct and
# which nothing sends. Every browser, Apache and nginx use `image/x-icon`, it is
# what kolt's hand-rolled table settled on against a real favicon, and a favicon
# is this row's whole reason to exist. Recorded as an override rather than
# silently preferring Apache for one extension, so the deviation is one grep away.
OVERRIDES = {"ico": ("image/x-icon", "web reality over the vnd. registration")}


def fetch_db(destination: pathlib.Path) -> pathlib.Path:
    """`npm pack mime-db@<pinned>` and unpack its db.json."""
    version = PINNED_MIME_DB.split()[-1]
    subprocess.run(
        ["npm", "pack", f"mime-db@{version}", "--pack-destination", str(destination)],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    tarballs = list(destination.glob("mime-db-*.tgz"))
    if len(tarballs) != 1:
        sys.exit(f"expected one mime-db tarball, got {tarballs}")
    with tarfile.open(tarballs[0]) as tar:
        tar.extract("package/db.json", destination, filter="data")
    return destination / "package" / "db.json"


def derive(db: dict) -> list[tuple[str, str, str, str]]:
    """(group, extension, media type, provenance) for every curated extension."""
    claims: dict[str, list[tuple[str, str]]] = {}
    for media_type, info in db.items():
        for extension in info.get("extensions", []):
            claims.setdefault(extension, []).append((media_type, info.get("source", "none")))

    rows = []
    for group, extensions in CURATED.items():
        for extension in sorted(extensions):
            if extension in OVERRIDES:
                media_type, why = OVERRIDES[extension]
                rows.append((group, extension, media_type, f"override: {why}"))
                continue
            candidates = claims.get(extension)
            if not candidates:
                sys.exit(f"{PINNED_MIME_DB} has no media type for `.{extension}`")
            # Registry first, then the media type name, so the pick is total.
            candidates.sort(key=lambda c: (SOURCE_RANK.get(c[1], 9), c[0]))
            media_type, source = candidates[0]
            rows.append((group, extension, media_type, source))
    return rows


def render(rows: list[tuple[str, str, str, str]]) -> str:
    header = [
        "# The extension -> media type dataset `std::build`'s content-type table is",
        "# generated from. GENERATED FILE -- do not hand-edit.",
        "#",
        f"# source:    {PINNED_MIME_DB} (https://github.com/jshttp/mime-db), MIT licensed.",
        "#            The IANA/Apache/nginx registries aggregated; the dataset vite's",
        "#            `mrmime` is generated from too.",
        "# refresh:   python3 scripts/regen-mime-table.py",
        "# consumer:  crates/vilan-core/tests/mime_table_sync.rs generates the vilan",
        "#            `match` arms in vilan/std/src/process/build.vl from these rows",
        "#            and fails when they drift. It also owns the charset rule: a",
        "#            `text/*` row is served `; charset=utf-8`, everything else bare.",
        "#",
        "# group\textension\tmedia type\tprovenance",
    ]
    body = [f"{group}\t{ext}\t{media_type}\t{why}" for group, ext, media_type, why in rows]
    return "\n".join(header + body) + "\n"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--db", type=pathlib.Path, help="a local mime-db db.json")
    parser.add_argument("--check", action="store_true", help="derive and diff, write nothing")
    arguments = parser.parse_args()

    root = pathlib.Path(__file__).resolve().parent.parent
    output = root / OUTPUT

    with tempfile.TemporaryDirectory() as scratch:
        db_path = arguments.db or fetch_db(pathlib.Path(scratch))
        db = json.loads(db_path.read_text())

    rendered = render(derive(db))

    if arguments.check:
        current = output.read_text() if output.exists() else ""
        if current == rendered:
            print(f"{OUTPUT} is current with {PINNED_MIME_DB}")
            return
        sys.exit(f"{OUTPUT} is not what {PINNED_MIME_DB} derives — rerun without --check")

    output.write_text(rendered)
    print(f"wrote {OUTPUT} ({len(rendered.splitlines())} lines) from {PINNED_MIME_DB}")


if __name__ == "__main__":
    main()
