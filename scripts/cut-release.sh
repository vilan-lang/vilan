#!/bin/sh
# Executes steps 1-3 of the cut (proposal/releases.md §7.2): the reconciliation
# sweep's part (a), then the CHANGELOG retitle-and-order, then the version bump.
# releases.md stays the AUTHORITY; this script is its executor.
#
#   scripts/cut-release.sh --dry-run 0.35.0    # verify and report, change nothing
#   scripts/cut-release.sh 0.35.0              # apply, then print the git commands
#   scripts/cut-release.sh --commit 0.35.0     # ... and make the `release:` commit
#
# It never tags and never pushes. Those are the human's, and the script prints
# them verbatim when it is done (§7.2 steps 4-5).
#
# Options:
#   --dry-run           report only; touch nothing
#   --commit            after applying, stage the release files and commit
#   --date YYYY-MM-DD   the section's date (default: today)
#   --against <commit>  the commit that will become the tag (default: HEAD);
#                       a §7.3 patch cut tags a release/0.MINOR branch, not HEAD
#   --out FILE          write the proposed CHANGELOG to FILE and do nothing
#                       else (implies --dry-run) - the seam the pins use
#
# Part (a), mechanized. Entries carry no sha, so provenance is derived: the
# oldest commit in the repository that introduced the entry's bold head into
# CHANGELOG.md is the entry's commit, and it must be an ancestor of the commit
# that will become the tag. An entry whose head no commit introduced is not
# committed; one introduced only on a branch that never merged is exactly the
# drift §7.1 was written about. A `<!-- commit: <sha> -->` line above an entry
# names its commit explicitly and overrides the derivation.
#
# The order (§7.2 step 3) is breaking, then miscompiles, then features, then
# diagnostics and tooling, and the family is a human judgment this script
# carries rather than infers: a
# `<!-- family: breaking|miscompile|feature|tooling -->` line above the entry's
# head. An entry with no family, or one this script does not know, is REFUSED
# and printed - never guessed. Within a family the authored order is preserved
# exactly, so a thematic grouping a human wrote survives the sort.
set -eu

VERSION=""
DRY_RUN=0
DO_COMMIT=0
SECTION_DATE=""
AGAINST="HEAD"
OUT=""
INVOKED_FROM="$PWD"
TAB="$(printf '\t')"

say() { printf '%s\n' "$1"; }
fail() { printf 'cut-release: %s\n' "$1" >&2; exit 1; }
run() {
    printf '+ %s\n' "$*"
    "$@"
}

usage() {
    say "usage: scripts/cut-release.sh [--dry-run] [--commit] [--date YYYY-MM-DD]"
    say "                              [--against <commit>] [--out FILE] <X.Y.Z>"
}

while [ $# -gt 0 ]; do
    case "$1" in
        --dry-run) DRY_RUN=1 ;;
        --commit) DO_COMMIT=1 ;;
        --date)
            SECTION_DATE="${2:?--date needs a YYYY-MM-DD argument}"
            shift
            ;;
        --against)
            AGAINST="${2:?--against needs a commit-ish argument}"
            shift
            ;;
        --out)
            OUT="${2:?--out needs a file argument}"
            DRY_RUN=1
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        -*) fail "unknown option $1" ;;
        *)
            [ -z "$VERSION" ] || fail "give exactly one version (got '$VERSION' and '$1')"
            VERSION="$1"
            ;;
    esac
    shift
done

if [ -z "$VERSION" ]; then
    usage >&2
    exit 1
fi
case "$VERSION" in
    v*) fail "give the version without its leading v (e.g. ${VERSION#v})" ;;
esac
printf '%s' "$VERSION" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' ||
    fail "'$VERSION' is not an X.Y.Z version"

if [ "$DRY_RUN" = 1 ] && [ "$DO_COMMIT" = 1 ]; then
    fail "--dry-run and --commit contradict each other"
fi

if [ -n "$OUT" ]; then
    case "$OUT" in
        /*) ;;
        *) OUT="$INVOKED_FROM/$OUT" ;;
    esac
fi

cd "$(dirname "$0")/.."
[ -f CHANGELOG.md ] || fail "no CHANGELOG.md at this script's repository root"

[ -n "$SECTION_DATE" ] || SECTION_DATE="$(date +%Y-%m-%d)"
printf '%s' "$SECTION_DATE" | grep -Eq '^[0-9]{4}-[0-9]{2}-[0-9]{2}$' ||
    fail "'$SECTION_DATE' is not a YYYY-MM-DD date"

TARGET="$(git rev-parse --verify "$AGAINST^{commit}" 2> /dev/null)" ||
    fail "--against '$AGAINST' does not name a commit"

if grep -q "^## v$VERSION " CHANGELOG.md; then
    fail "CHANGELOG.md already carries a '## v$VERSION' section - this version is cut"
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT INT HUP TERM

# ---------------------------------------------------------------------------
# The parser. One pass over CHANGELOG.md, two modes:
#   list    - one `E <rank> <family> <commit> <head>` record per entry, in the
#             order §7.2 asks for, plus a `P <message>` record per refusal.
#   rewrite - the whole file with `## Unreleased` retitled, its entries
#             reordered, and exactly one `---` rule between neighbours.
# Records are tab-separated and an absent field is `-`; a changelog head
# contains neither a tab nor a lone `-` field.
# ---------------------------------------------------------------------------
CHANGELOG_AWK='
function rank_of(family) {
    if (family == "breaking") return 1
    if (family == "miscompile") return 2
    if (family == "feature") return 3
    if (family == "tooling" || family == "diagnostics") return 4
    return 0
}
function problem(message) {
    if (mode == "list") printf "P\t%s\n", message
    refusals++
}
function marker_value(line, key,   rest, stop) {
    rest = substr(line, index(line, key) + length(key))
    sub(/^[ \t]*/, "", rest)
    stop = index(rest, "-->")
    rest = substr(rest, 1, stop - 1)
    sub(/[ \t]*$/, "", rest)
    return rest
}
function dash(value) { return value == "" ? "-" : value }
function close_entry(   text) {
    if (!open) return
    text = body[count]
    sub(/\n+$/, "", text)
    body[count] = text
    open = 0
}
function emit_section(   wanted, i, printed) {
    if (emitted) return
    emitted = 1
    close_entry()
    if (count == 0) problem("the `## Unreleased` section holds no entries")
    for (i = 1; i <= count; i++) {
        if (head[i] == "") {
            problem("entry " i " opens with `**` and never closes it")
        } else if (family[i] == "") {
            problem("entry " i " carries no `<!-- family: ... -->` marker: " head[i])
        } else if (rank_of(family[i]) == 0) {
            problem("entry " i " has the unknown family `" family[i] "`: " head[i])
        }
    }
    if (mode == "list") {
        if (refusals > 0) {
            # Still list every entry, in file order, so the sweep below can
            # report on a section this script is refusing to order.
            for (i = 1; i <= count; i++)
                printf "E\t0\t%s\t%s\t%s\n", dash(family[i]), dash(commit[i]), head[i]
            return
        }
        for (wanted = 1; wanted <= 4; wanted++)
            for (i = 1; i <= count; i++)
                if (rank_of(family[i]) == wanted)
                    printf "E\t%d\t%s\t%s\t%s\n", wanted, family[i], dash(commit[i]), head[i]
        return
    }
    if (refusals > 0) return
    printed = 0
    for (wanted = 1; wanted <= 4; wanted++) {
        for (i = 1; i <= count; i++) {
            if (rank_of(family[i]) != wanted) continue
            printed++
            if (printed > 1) print "\n---\n"
            if (commit[i] != "") print "<!-- commit: " commit[i] " -->"
            print "<!-- family: " family[i] " -->"
            print body[i]
        }
    }
}
BEGIN {
    stage = 0; count = 0; open = 0; emitted = 0; refusals = 0
    pending_family = ""; pending_commit = ""; boundary = 1
}
stage == 0 {
    if ($0 == "## Unreleased") {
        stage = 1
        if (mode == "rewrite") { print heading; print "" }
        next
    }
    if (mode == "rewrite") print
    next
}
stage == 1 {
    if (substr($0, 1, 3) == "## ") {
        emit_section()
        stage = 2
        if (mode == "rewrite") { print ""; print }
        next
    }
    if ($0 ~ /^<!--[ \t]*family:[ \t]*[A-Za-z-]+[ \t]*-->[ \t]*$/) {
        close_entry()
        pending_family = marker_value($0, "family:")
        boundary = 1
        next
    }
    if ($0 ~ /^<!--[ \t]*commit:[ \t]*[0-9a-fA-F]+[ \t]*-->[ \t]*$/) {
        close_entry()
        pending_commit = marker_value($0, "commit:")
        boundary = 1
        next
    }
    if ($0 ~ /^-{3,}[ \t]*$/) {
        close_entry()
        boundary = 1
        next
    }
    if (boundary && substr($0, 1, 2) == "**") {
        close_entry()
        count++
        head[count] = substr($0, 3, index(substr($0, 3), "**") - 1)
        family[count] = pending_family; pending_family = ""
        commit[count] = pending_commit; pending_commit = ""
        body[count] = $0 "\n"
        open = 1
        boundary = 0
        next
    }
    if (open) {
        body[count] = body[count] $0 "\n"
        boundary = ($0 ~ /^[ \t]*$/)
        next
    }
    if ($0 ~ /^[ \t]*$/) { boundary = 1; next }
    problem("text under `## Unreleased` that begins no entry: " $0)
    next
}
stage == 2 { if (mode == "rewrite") print; next }
END {
    if (stage == 0) { problem("CHANGELOG.md has no `## Unreleased` section"); exit 3 }
    emit_section()
    if (refusals > 0) exit 3
}
'

HEADING="## v$VERSION — $SECTION_DATE"
LISTING="$WORK/listing"
AWK_STATUS=0
awk -v mode=list -v heading="$HEADING" "$CHANGELOG_AWK" CHANGELOG.md > "$LISTING" ||
    AWK_STATUS=$?
if [ "$AWK_STATUS" != 0 ] && [ "$AWK_STATUS" != 3 ]; then
    fail "the changelog parser failed (exit $AWK_STATUS)"
fi

say "cut v$VERSION — $SECTION_DATE"
say "against $(git rev-parse --short "$TARGET")  $(git log -1 --format=%s "$TARGET")"
say ""

REFUSED="$AWK_STATUS"
if [ "$REFUSED" = 3 ]; then
    say 'REFUSED — an entry has no family, and this script never guesses one.'
    say '  <!-- family: breaking -->    a program that compiles today may stop, or change behaviour'
    say '  <!-- family: miscompile -->  the compiler was wrong about a program it accepted'
    say '  <!-- family: feature -->     a new capability'
    say '  <!-- family: tooling -->     diagnostics, the editor, the CLI, packaging'
    say '  (releases.md §7.2 defines each; "diagnostics" is an accepted spelling of "tooling")'
    say ""
    awk -F"$TAB" '$1 == "P" { printf "  RED   %s\n", $2 }' "$LISTING"
    say ""
fi

# ---------------------------------------------------------------------------
# Sweep (a) - releases.md §7.1: every Unreleased entry's commit is an ancestor
# of the commit that will become the tag.
# ---------------------------------------------------------------------------
sweep() {
    sweep_red=0
    while IFS="$TAB" read -r kind rank family commit entry_head <&3; do
        [ "$kind" = "E" ] || continue
        : "$rank" "$family"
        [ "$commit" != "-" ] || commit=""
        origin="$commit"
        if [ -z "$origin" ]; then
            origin="$(git log --all --format=%H -S"**$entry_head**" -- CHANGELOG.md |
                tail -n 1)"
        fi
        if [ -z "$origin" ]; then
            say "  RED   no commit in this repository introduced this entry - is it committed?"
            say "        $entry_head"
            sweep_red=1
            continue
        fi
        if ! git rev-parse --verify --quiet "$origin^{commit}" > /dev/null; then
            say "  RED   <!-- commit: $origin --> names no commit in this repository"
            say "        $entry_head"
            sweep_red=1
            continue
        fi
        short="$(git rev-parse --short "$origin")"
        subject="$(git log -1 --format=%s "$origin")"
        if ! git merge-base --is-ancestor "$origin" "$TARGET"; then
            say "  RED   $short is NOT an ancestor of the tag commit - this entry's code did not land"
            say "        $short $subject"
            say "        $entry_head"
            sweep_red=1
            continue
        fi
        say "  ok    $short  $entry_head"
        # An entry whose introducing commit changed nothing but records is the
        # shape §7.1 warns about: a changelog claim that arrived without its
        # code. Not fatal - a later reword of the head lands here too - but say so.
        touched="$(git show --name-only --format= "$origin" |
            grep -v -e '^CHANGELOG\.md$' -e '^vilan/proposal/' -e '^$' | head -n 1)"
        if [ -z "$touched" ]; then
            say "        note: $short touched records only ($subject) - confirm its code landed"
        fi
    done 3< "$LISTING"
    return "$sweep_red"
}

say "sweep (a) — each entry's commit, against $(git rev-parse --short "$TARGET")"
say ""
SWEEP_RED=0
sweep || SWEEP_RED=1
say ""

if [ "$REFUSED" != 3 ]; then
    say "order (§7.2) — breaking, then miscompiles, then features, then diagnostics and tooling"
    say ""
    awk -F"$TAB" '$1 == "E" { printf "  %2d  %-11s %s\n", ++n, $3, $5 }' "$LISTING"
    say ""
    awk -F"$TAB" '$1 == "E" { seen[$3]++ }
        END { for (family in seen) printf "  %s: %d\n", family, seen[family] }' "$LISTING" |
        sort
    say ""
fi

if [ "$REFUSED" = 3 ] || [ "$SWEEP_RED" != 0 ]; then
    fail "refusing to cut - fix the reds above (nothing was changed)"
fi

# ---------------------------------------------------------------------------
# Apply.
# ---------------------------------------------------------------------------
PROPOSED="$WORK/CHANGELOG.md"
awk -v mode=rewrite -v heading="$HEADING" "$CHANGELOG_AWK" CHANGELOG.md > "$PROPOSED"

if [ -n "$OUT" ]; then
    cp "$PROPOSED" "$OUT"
    say "wrote the proposed CHANGELOG to $OUT (nothing else changed)"
    exit 0
fi

if [ "$DRY_RUN" = 1 ]; then
    say "the retitle and order, as a diff (--dry-run: nothing changed)"
    say ""
    diff -u CHANGELOG.md "$PROPOSED" || true
    say ""
    say "run without --dry-run to apply it, bump, and print the commit commands."
    exit 0
fi

cp "$PROPOSED" CHANGELOG.md
say "CHANGELOG.md: '## Unreleased' is now '$HEADING', ordered."
say ""
run sh scripts/bump-version.sh "$VERSION"
say ""

RELEASE_FILES="CHANGELOG.md Cargo.lock crates/vilan-cli/Cargo.toml
crates/vilan-core/Cargo.toml crates/vilan-embedded-std/Cargo.toml
crates/vilan-lsp/Cargo.toml crates/vilan-wasm/Cargo.toml
editors/vscode/package.json editors/vscode/package-lock.json"

if [ "$DO_COMMIT" = 1 ]; then
    counts="$(awk -F"$TAB" '$1 == "E" { seen[$3]++ }
        END { for (family in seen) printf "%s %d, ", family, seen[family] }' "$LISTING" |
        sed 's/, $//')"
    # shellcheck disable=SC2086
    run git add $RELEASE_FILES
    run git commit -m "release: v$VERSION" -m "$counts."
    say ""
    say "the body is a placeholder - amend it with the release's own prose:"
    say ""
    say "    git commit --amend"
else
    say "staged nothing; the release commit is yours to write:"
    say ""
    say "    git add $(printf '%s' "$RELEASE_FILES" | tr '\n' ' ')"
    say "    git commit -m 'release: v$VERSION'"
fi

say ""
say "then tag and push - the tag push is release.yml's trigger (§7.2 steps 4-5):"
say ""
say "    git tag v$VERSION"
say "    git push origin next"
say "    git push origin v$VERSION"
say ""
say "watch it green, then fold: scripts/fold-release.sh v$VERSION"
