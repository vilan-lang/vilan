#!/bin/sh
# Has CI already passed THIS TREE? (tracker N210.)
#
#   scripts/ci-already-green.sh [<commit-ish>]     # default HEAD
#
# Prints `<run id> <head sha>` and exits 0 when a `ci.yml` run on a push to this
# repository concluded success, EVERY ONE OF ITS JOBS concluded success, and its
# head commit has the same git TREE as the commit asked about. Exits 1 when no
# such run exists, and 2 when it cannot tell (no `gh`, no network, an API error).
# `ci.yml`'s `changes` job calls it and treats anything but 0 as "run the suite":
# every uncertain answer runs the suite, the same rule its path filter follows.
#
# WHY THE TREE AND NOT THE COMMIT. A commit's tree is its whole content: every
# source file, every lockfile, every script, and `.github/workflows/ci.yml`
# itself. Two commits with one tree build and test identically, whatever their
# messages and parents. A release makes several: the fold merges the tag into
# `main` and fast-forwards `next`, and both land on the tag's exact tree, which
# CI passed on the release commit an hour earlier; a merge that changes no file
# is the same case. v0.56.0 and v0.56.1 (2026-11-16) cost twelve full runs
# between them, four of those on trees already green.
#
# WHAT COUNTS AS GREEN, and why each condition:
# - `event=push`: a push run in this repository's own list ran this
#   repository's code on a branch someone with write access pushed. A fork's
#   pull request can never mint one.
# - every job `success`: a run whose suite was SKIPPED (root prose only, or a
#   run this very script short-circuited) also concludes success, and it proves
#   nothing about the tree by itself. Requiring that nothing was skipped means
#   the run found here is one in which the suite really ran on this tree.
# - at least one `test (` job among them: the same point, stated on the job
#   that matters, so a workflow rename cannot make "all zero jobs passed" true.
# - created within $CI_GREEN_DAYS days (default 14): the runners' images, the
#   advisory database `audit` reads and the toolchain `stable` resolves to all
#   move. A tree that was green a month ago is asked again.
#
# THE ESCAPE HATCH is in ci.yml, not here: a head commit whose message contains
# `[ci full]` runs the suite regardless.
set -eu

commit=${1:-HEAD}
repo=${GITHUB_REPOSITORY:-vilan-lang/vilan}
days=${CI_GREEN_DAYS:-14}

command -v gh >/dev/null 2>&1 || { echo "ci-already-green: gh is not installed" >&2; exit 2; }
tree=$(git rev-parse --verify --quiet "$commit^{tree}") || {
    echo "ci-already-green: '$commit' names no commit here" >&2
    exit 2
}
since=$(date -u -d "$days days ago" +%Y-%m-%dT%H:%M:%SZ 2>/dev/null) || {
    echo "ci-already-green: this date(1) cannot subtract days" >&2
    exit 2
}

# One page of 100 is about three weeks of pushes at this project's pace, and the
# window above is two; a tree older than the page is simply asked again.
candidates=$(gh api --method GET "repos/$repo/actions/workflows/ci.yml/runs" \
    -f status=success -f event=push -f per_page=100 -f "created=>=$since" \
    --jq ".workflow_runs[] | select(.head_commit.tree_id == \"$tree\") | \"\(.id) \(.head_sha)\"") || {
    echo "ci-already-green: could not list ci.yml's runs on $repo" >&2
    exit 2
}

[ -n "$candidates" ] || exit 1

# Newest first, as the API lists them. The first whose every job passed answers.
# (A here-document, not a pipe: the loop then runs in this shell, so its `exit`
# is the script's and `found` outlives it.)
found=""
while read -r id sha; do
    [ -n "$id" ] || continue
    verdict=$(gh api --method GET "repos/$repo/actions/runs/$id/jobs" -f per_page=100 -f filter=latest \
        --jq '([.jobs[] | select(.conclusion != "success")] | length) as $bad
              | ([.jobs[] | select(.name | startswith("test ("))] | length) as $tests
              | "\($bad) \($tests)"') || {
        echo "ci-already-green: could not read run $id's jobs" >&2
        exit 2
    }
    bad=${verdict% *}
    tests=${verdict#* }
    if [ "$bad" = 0 ] && [ "$tests" -gt 0 ]; then
        found="$id $sha"
        break
    fi
    # A success whose suite was skipped proves nothing by itself: keep looking.
done <<CANDIDATES
$candidates
CANDIDATES

[ -n "$found" ] || exit 1
printf '%s\n' "$found"
