#!/usr/bin/env python3
"""Every `#[ignore]`d pin's tracker item, checked OPEN (tracker N27).

CI already holds the FORMAT half: `crates/vilan-cli/tests/ci_ignored_pins.rs`
refuses an `#[ignore]` reason that does not lead with a tracker item id, so a
pinned bug cannot live nowhere but its own attribute string (N31). CI cannot
hold the other half - the tracker is a different repository and is not in a CI
checkout - so the shape is all a workflow can check, and "the id names an item
that is still OPEN" is left to whoever has both repositories in hand.

That is this script, run by the orchestrator at close. A pin whose owner has
been CLOSED is the N31 anti-pattern back again from the other end: the defect
is recorded as fixed while a test still asserts it is not, and nobody is
looking at either. A pin whose owner is in neither the index nor the archive is
worse - it points at nothing at all.

**Platform-gated pins are seen, and that is the point of enumerating TEXTUALLY**
(tracker N45). The enumeration is `git ls-files '*.rs'` plus a line scan: no
compilation, no test list, no target. So a `#[cfg(windows)]` pin's `#[ignore]`
reason is read on a Linux box exactly as a portable one is, and the close-time
cross-check covers it - which matters because the OTHER half of N27, the weekly
`--run-ignored only` leg in `.github/workflows/ignored-pins.yml`, runs on
`ubuntu-latest` alone: there a `cfg(windows)` pin compiles away, never runs, and
so can never be reported as "now PASSES". The two halves fail differently on
purpose, and this is the half that does not go blind at a platform boundary.
`a cfg-gated pin is enumerated like any other` in the self-test below is what
holds that.

Usage, from the repository root:

    scripts/ignored-pins.py --tracker ../proposals/projects/vilan/tracker
    scripts/ignored-pins.py --list
    scripts/ignored-pins.py --self-test

Exit status is 1 when a pin's owner is not open, so the run can gate a close.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

# The same fence `ci_ignored_pins.rs` uses to say "this `#[ignore` is a string
# literal in a fixture, not an attribute". Spelled in halves for the same reason
# it is there: the marker must not match itself.
FIXTURE_FENCE_OPEN = "ignore-sweep-fixture" ":start"
FIXTURE_FENCE_CLOSE = "ignore-sweep-fixture" ":end"

ATTRIBUTE = "#[ignore"

# A tracker item id, at the START of a reason: one family letter, one to three
# digits, ending where the digits do. The shape is `names_a_tracker_item`'s in
# `ci_ignored_pins.rs` and closes the same hole (N33): `ARM64`, `UTF8`, `ES6`
# and `ISO8601` are all "capitals then a digit" and none of them points
# anywhere, so the letter run must be ONE letter and the digit run must end at a
# non-alphanumeric. This script needs no roster of family letters, unlike its
# Rust twin: it has the tracker itself, and a letter no family uses simply
# resolves to nothing, which is already the answer it would report.
ITEM_ID = re.compile(r"^([A-Z]\d{1,3})(?![0-9A-Za-z_])")


@dataclass(frozen=True)
class Pin:
    """One `#[ignore]`d test, as its reason string presents itself."""

    file: str
    line: int
    reason: str

    @property
    def item(self) -> str | None:
        """The tracker item the reason leads with, if it leads with one."""
        match = ITEM_ID.match(self.reason.lstrip())
        return match.group(1) if match else None


def without_fixture_fences(text: str) -> str:
    """`text` with fenced fixture regions blanked, line numbers preserved."""
    out: list[str] = []
    fenced = False
    for line in text.splitlines(keepends=True):
        if not fenced and FIXTURE_FENCE_OPEN in line:
            fenced = True
        elif fenced and FIXTURE_FENCE_CLOSE in line:
            fenced = False
            out.append("\n" if line.endswith("\n") else "")
            continue
        out.append(("\n" if line.endswith("\n") else "") if fenced else line)
    return "".join(out)


def read_string_literal(text: str, start: int) -> tuple[str, int]:
    """The Rust string literal opening at `start`, resolved, and its end.

    Only the escapes that change what the program prints are resolved, and the
    one that matters is the line CONTINUATION: `\\` + newline + indentation
    swallows both, which is how every long reason in this tree is written.
    """
    out: list[str] = []
    index = start + 1
    while index < len(text):
        character = text[index]
        if character == "\\" and index + 1 < len(text):
            following = text[index + 1]
            if following in "\r\n":
                index += 2
                while index < len(text) and text[index] in " \t\r\n":
                    index += 1
                continue
            out.append({"n": "\n", "t": "\t"}.get(following, following))
            index += 2
            continue
        if character == '"':
            return "".join(out), index + 1
        out.append(character)
        index += 1
    return "".join(out), len(text)


def ignore_attributes(text: str) -> list[Pin]:
    """Every `#[ignore = "..."]` in one file, by 1-based line.

    An attribute counts only where one is WRITTEN: at the start of a line,
    whitespace aside. That is what keeps the sweep off this tree's many prose
    mentions of `#[ignore]` without parsing Rust - the same rule, for the same
    reason, as the Rust sweep next door.
    """
    text = without_fixture_fences(text)
    found: list[Pin] = []
    for line_number, line in enumerate(text.splitlines(), start=1):
        stripped = line.lstrip()
        if not stripped.startswith(ATTRIBUTE):
            continue
        # Located in the whole text, not in the line, because a reason wraps.
        start = _offset_of_line(text, line_number) + (len(line) - len(stripped))
        cursor = start + len(ATTRIBUTE)
        while cursor < len(text) and text[cursor] in " \t\r\n=":
            cursor += 1
        if cursor < len(text) and text[cursor] == '"':
            reason, _ = read_string_literal(text, cursor)
            found.append(Pin(file="", line=line_number, reason=reason))
    return found


def _offset_of_line(text: str, line_number: int) -> int:
    offset = 0
    for _ in range(line_number - 1):
        offset = text.index("\n", offset) + 1
    return offset


def tracked_rust_sources(root: Path) -> list[tuple[str, str]]:
    """Every committed `.rs` file, as `(repo-relative path, contents)`.

    `git ls-files` is the enumerator on purpose - exactly the committed tree, so
    the sweep can never wander into `target/` or into a sibling worktree under
    `.claude/` and read somebody else's branch as this one.
    """
    listing = subprocess.run(
        ["git", "ls-files", "-z", "*.rs"],
        cwd=root,
        capture_output=True,
        check=True,
        text=True,
    ).stdout
    sources = []
    for name in listing.split("\0"):
        if not name:
            continue
        try:
            sources.append((name, (root / name).read_text(encoding="utf-8")))
        except (OSError, UnicodeDecodeError):
            continue
    return sources


def pins(root: Path) -> list[Pin]:
    found = []
    for name, text in tracked_rust_sources(root):
        for pin in ignore_attributes(text):
            found.append(Pin(file=name, line=pin.line, reason=pin.reason))
    return found


def open_items(tracker: Path) -> set[str]:
    """The ids the tracker's INDEX still lists - the OPEN set.

    Read from the index's own links (`[B149](items/B149.md)`) rather than from
    the table's shape, because the link is what `backlog <ID>` resolves through.
    """
    index = (tracker / "INDEX.md").read_text(encoding="utf-8")
    return set(re.findall(r"\(items/([A-Z]\d{1,3})\.md\)", index))


def archive_chain(tracker: Path) -> list[Path]:
    """Every file a `backlog <ID>` citation may resolve to as a tombstone.

    The current archive plus the FROZEN eras before it, which is not optional
    bookkeeping: the pin this script first caught names `B126`, closed on
    2026-08-22 - four months and one whole tracker migration before the current
    `archive.md` was started - so a checker reading only `archive.md` would call
    a closed owner "unknown" and say the wrong thing about a real fault.
    `tracker/archive.md`'s own header names the chain but not its path, and the
    frozen files have moved once already (they sit at the proposals repository's
    root today, not beside the tracker), so the chain is FOUND rather than
    spelled: the first `archive/` directory on the way up from the tracker.
    Searching upward only ever reaches real ancestors, so a sibling worktree's
    copy cannot be mistaken for this one's.
    """
    files = [tracker / "archive.md"]
    directory = tracker.resolve()
    for _ in range(5):
        frozen = directory / "archive"
        if frozen.is_dir():
            files.extend(sorted(frozen.glob("*.md")))
            break
        if directory.parent == directory:
            break
        directory = directory.parent
    return [path for path in files if path.is_file()]


def closed_items(tracker: Path) -> set[str]:
    """The ids the archive chain tombstones - closed, the id retired."""
    closed: set[str] = set()
    for path in archive_chain(tracker):
        text = path.read_text(encoding="utf-8")
        closed.update(re.findall(r"^\s*-\s+\*\*([A-Z]\d{1,3})\.", text, re.MULTILINE))
    return closed


def report(root: Path, tracker: Path, out) -> int:
    swept = pins(root)
    named = [pin for pin in swept if pin.item]
    unnamed = len(swept) - len(named)
    opened, closed = open_items(tracker), closed_items(tracker)

    faults: list[str] = []
    for pin in sorted(named, key=lambda pin: (pin.file, pin.line)):
        item = pin.item
        if item in opened:
            print(f"  ok      {pin.file}:{pin.line}  {item}", file=out)
        elif item in closed:
            faults.append(
                f"  CLOSED  {pin.file}:{pin.line}  {item} — the item is tombstoned in "
                f"archive.md while this pin still asserts the defect. Un-ignore the pin "
                f"if the fix landed, or reopen the item if it did not."
            )
        else:
            faults.append(
                f"  UNKNOWN {pin.file}:{pin.line}  {item} — named by the reason and in "
                f"neither INDEX.md nor archive.md, so the pin points at nothing. File "
                f"the item, or re-point the reason at the id that owns the defect."
            )
    for fault in faults:
        print(fault, file=out)

    print(
        f"\n{len(named)} pin(s) name a tracker item, {len(faults)} not open; "
        f"{unnamed} pin(s) name none (the CI format gate owns those).",
        file=out,
    )
    return 1 if faults else 0


# --- The self-test ----------------------------------------------------------
#
# The script's own gate: it is run by hand rather than by CI, so it carries the
# proof that its two readers work instead of relying on a suite to notice.

SELF_TEST_SOURCE = '''
#[test]
#[ignore = "B173: a blanket impl never satisfies a bound \\
            for a generic value"]
fn wrapped() {}

    #[ignore = "the leak soak: run deliberately"]
fn no_item() {}

// #[ignore = "E1: a mention in prose is not an attribute"]

#[ignore = "ARM64 is an acronym, not an item"]
fn acronym() {}

#[ignore = "I18n is not item 18"]
fn not_item_eighteen() {}

#[cfg(windows)]
#[test]
#[ignore = "B198: a platform-gated pin, invisible to the weekly ubuntu run \\
            and visible here"]
fn platform_gated() {}
'''


def self_test(out) -> int:
    failures: list[str] = []

    def check(claim: str, held: bool) -> None:
        if not held:
            failures.append(claim)

    found = ignore_attributes(SELF_TEST_SOURCE)
    check("five attributes are read, prose is not", len(found) == 5)
    reasons = [pin.reason for pin in found]
    check(
        "a wrapped reason is one run",
        reasons[0]
        == "B173: a blanket impl never satisfies a bound for a generic value",
    )
    check("the wrapped reason leads with B173", found[0].item == "B173")
    check("a reason naming no item has no item", found[1].item is None)
    check("`ARM64` is not read as an id", found[2].item is None)
    check("`I18n` is not read as item 18", found[3].item is None)
    # N45. The enumeration is textual, so a pin the ubuntu leg cannot even
    # compile is still read here, reason and all - which is the only automated
    # route by which a `cfg(windows)` pin's `#[ignore]` can expire. Read by
    # REASON rather than by position, so a scanner that goes blind at the `cfg`
    # gate reports a named failure instead of running off the end of the list.
    gated = next(
        (pin for pin in found if pin.reason.startswith("B198")),
        None,
    )
    check("a cfg-gated pin is enumerated like any other", gated is not None)
    check(
        "and its wrapped reason is one run",
        gated is not None
        and gated.reason
        == "B198: a platform-gated pin, invisible to the weekly ubuntu run "
        "and visible here",
    )
    check(
        "a fenced fixture is blanked",
        ATTRIBUTE
        not in without_fixture_fences(
            f"{FIXTURE_FENCE_OPEN}\n{ATTRIBUTE} = \"x\"]\n{FIXTURE_FENCE_CLOSE}\n"
        ),
    )
    check(
        "blanking keeps line numbers",
        without_fixture_fences(
            f"{FIXTURE_FENCE_OPEN}\na\nb\n{FIXTURE_FENCE_CLOSE}\nkept\n"
        ).splitlines()[4]
        == "kept",
    )
    check("an id ends at the digits", Pin("", 1, "B1234: too long").item is None)
    check("one to three digits is an id", Pin("", 1, "C13 — a pin").item == "C13")

    for failure in failures:
        print(f"  FAIL  {failure}", file=out)
    print(
        f"self-test: {len(failures)} failure(s)" if failures else "self-test: ok",
        file=out,
    )
    return 1 if failures else 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--tracker",
        type=Path,
        help="the tracker directory holding INDEX.md and archive.md",
    )
    parser.add_argument(
        "--list",
        action="store_true",
        help="print every pin that names an item, and stop",
    )
    parser.add_argument(
        "--self-test", action="store_true", help="check this script's own readers"
    )
    arguments = parser.parse_args(argv)

    if arguments.self_test:
        return self_test(sys.stdout)

    root = Path(__file__).resolve().parent.parent
    if arguments.list:
        for pin in sorted(pins(root), key=lambda pin: (pin.file, pin.line)):
            print(f"{pin.file}:{pin.line}\t{pin.item or '-'}\t{pin.reason}")
        return 0

    if arguments.tracker is None:
        parser.error("--tracker is required (or pass --list / --self-test)")
    if not (arguments.tracker / "INDEX.md").is_file():
        parser.error(f"{arguments.tracker}/INDEX.md is not there")
    return report(root, arguments.tracker, sys.stdout)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
