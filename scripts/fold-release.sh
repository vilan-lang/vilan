#!/bin/sh
# Executes steps 6-10 of the cut (proposal/releases.md §7.2): the fold. Run it
# once `release.yml` is green for the tag.
#
#   scripts/fold-release.sh v0.35.0 --dry-run   # check everything, run nothing
#   scripts/fold-release.sh v0.35.0             # do it
#
# In order, and the order is load-bearing (§7.2): merge the tag into `main`
# with the exact message `Merge v<version> — main catches the release train`
# and push; fast-forward `next` onto the fold and push; dispatch the book's
# `docs.yml` and wait for it; THEN dispatch the site's `deploy.yml` and wait
# for it (the two push the same pages repo from different concurrency groups,
# so nothing serializes them for you); verify the playground manifest reads the
# new tag; refresh the local toolchain in BOTH locations.
#
# `--dry-run` prints every command it would run and performs only the
# read-only checks: ancestry, the release run's conclusion, the live manifest,
# the installed binaries' versions. It reports every check rather than stopping
# at the first, so one run tells you everything that is not ready.
#
# Live mode ASSERTS each precondition in order and stops loudly at the first
# failure. A step whose work is already done is reported `done` and skipped, so
# a fold interrupted at step 9 is resumable by re-running this.
#
# The release run's verdict comes from `gh run list` - a completed run whose
# conclusion is `success` and whose head sha is the tag's. Never from a
# watcher's exit code: `gh run watch` exits 0 for "I finished watching".
set -eu

VERSION=""
DRY_RUN=0
PAGES_REPO="vilan-lang/vilan-lang.github.io"
SITE_REPO="vilan-lang/website"
MANIFEST_URL="https://vilan-lang.org/playground/manifest.json"
# How long to wait for a dispatched workflow, and how often to ask.
WAIT_SECONDS=1800
POLL_SECONDS=15

say() { printf '%s\n' "$1"; }
fail() { printf 'fold-release: %s\n' "$1" >&2; exit 1; }
run() {
    printf '+ %s\n' "$*"
    if [ "$DRY_RUN" = 0 ]; then "$@"; fi
}
plan() { printf '+ %s\n' "$*"; }

REDS=0
ok() { say "  ok      $1"; }
done_() { say "  done    $1"; }
skip() { say "  skip    $1"; }
red() {
    say "  RED     $1"
    REDS=$((REDS + 1))
    [ "$DRY_RUN" = 1 ] || fail "$1"
}

usage() { say "usage: scripts/fold-release.sh <vX.Y.Z> [--dry-run]"; }

while [ $# -gt 0 ]; do
    case "$1" in
        --dry-run) DRY_RUN=1 ;;
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
printf '%s' "$VERSION" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$' ||
    fail "'$VERSION' is not a vX.Y.Z tag name"
NUMBER="${VERSION#v}"

cd "$(dirname "$0")/.."
git rev-parse --git-dir > /dev/null 2>&1 || fail "not a git repository"

# The worktree a branch is checked out in, if any. Every mutation goes through
# `git -C` on that path (the house rule), so nothing here depends on which
# worktree you happen to be standing in.
worktree_of() {
    git worktree list --porcelain |
        awk -v want="branch refs/heads/$1" '
            /^worktree /{ path = substr($0, 10) }
            $0 == want { print path; exit }'
}
# Uncommitted *changes* to tracked files. Untracked scratch is not the fold's
# business - `git merge` refuses on its own if one would be overwritten.
worktree_dirty() { [ -n "$(git -C "$1" status --porcelain --untracked-files=no)" ]; }

say "fold $VERSION — releases.md §7.2 steps 6-10"
[ "$DRY_RUN" = 0 ] || say "(--dry-run: read-only checks, and the commands that would run)"
say ""
say "preconditions"
say ""

# --- the tag ---------------------------------------------------------------
TAG_COMMIT=""
if TAG_COMMIT="$(git rev-parse --verify --quiet "refs/tags/$VERSION^{commit}")"; then
    ok "the tag $VERSION is $(git rev-parse --short "$TAG_COMMIT")"
else
    red "no tag $VERSION in this repository — the release has not been cut"
fi

# --- origin ----------------------------------------------------------------
HAVE_ORIGIN=0
if git config --get remote.origin.url > /dev/null 2>&1; then
    HAVE_ORIGIN=1
    ok "origin is $(git config --get remote.origin.url)"
else
    red "no 'origin' remote — the fold pushes main, next and reads the release run"
fi

REPO=""
TAG_ON_ORIGIN=0
if [ "$HAVE_ORIGIN" = 1 ] && [ -n "$TAG_COMMIT" ]; then
    REPO="$(git config --get remote.origin.url |
        sed -e 's#\.git$##' -e 's#^git@[^:]*:##' -e 's#^[a-z+]*://[^/]*/##')"
    remote_tag="$(git ls-remote --tags origin "refs/tags/$VERSION" | awk '{print $1}' | head -n 1)"
    if [ -z "$remote_tag" ]; then
        red "$VERSION is not on origin — push the tag before folding"
    elif [ "$remote_tag" != "$TAG_COMMIT" ]; then
        red "origin's $VERSION is $remote_tag but the local tag is $TAG_COMMIT — refusing to fold a tag that differs from origin's"
    else
        TAG_ON_ORIGIN=1
        ok "$VERSION is on origin at the same commit"
    fi
else
    skip "the tag on origin (needs a tag and an origin)"
fi

# --- release.yml -----------------------------------------------------------
if [ "$TAG_ON_ORIGIN" = 1 ]; then
    if ! command -v gh > /dev/null 2>&1; then
        red "gh is not installed — the release run's conclusion cannot be read"
    else
        verdict="$(gh run list -R "$REPO" --workflow release.yml --branch "$VERSION" \
            --limit 10 --json status,conclusion,headSha \
            --jq "[.[] | select(.headSha == \"$TAG_COMMIT\")] | first | \
                  if . == null then \"none\" else .status + \" \" + (.conclusion // \"\") end" \
            2> /dev/null || echo "unreachable")"
        case "$verdict" in
            "completed success")
                ok "release.yml concluded success for $VERSION"
                ;;
            none)
                red "release.yml has no run for $VERSION at $(git rev-parse --short "$TAG_COMMIT") — the fold must not run before the release publishes"
                ;;
            unreachable)
                red "could not read release.yml's runs from $REPO — is gh authenticated?"
                ;;
            "completed"*)
                red "release.yml for $VERSION concluded '${verdict#completed }', not success — do not fold a red release"
                ;;
            *)
                red "release.yml for $VERSION is still $verdict — wait for it, ten assets and five one-way channels are downstream"
                ;;
        esac
    fi
else
    skip "release.yml's conclusion (needs the tag on origin)"
fi

# --- main ------------------------------------------------------------------
# NOT "main is an ancestor of the tag": it is not, and never has been. `main`
# carries its own line - every previous fold merge, and commits pushed straight
# to it - so the fold is a real three-way merge, not a fast-forward dressed up.
# What must hold is that the merge is clean, and `git merge-tree --write-tree`
# answers that without touching a worktree.
MAIN_TREE=""
FOLD_PENDING=0
MAIN_WORKTREE="$(worktree_of main)"
if [ -z "$TAG_COMMIT" ]; then
    skip "main's readiness (needs the tag)"
elif ! git rev-parse --verify --quiet refs/heads/main > /dev/null; then
    red "no branch 'main' in this repository"
elif [ -z "$MAIN_WORKTREE" ]; then
    red "branch 'main' is checked out in no worktree — check it out somewhere so the merge has a tree"
elif worktree_dirty "$MAIN_WORKTREE"; then
    red "the worktree holding main ($MAIN_WORKTREE) has uncommitted changes — commit or stash them"
elif git merge-base --is-ancestor "$TAG_COMMIT" refs/heads/main; then
    done_ "main already carries $VERSION ($(git rev-parse --short refs/heads/main))"
elif MAIN_TREE="$(git merge-tree --write-tree refs/heads/main "$TAG_COMMIT" 2> /dev/null)"; then
    FOLD_PENDING=1
    ok "$VERSION merges into main cleanly (tree $(printf '%s' "$MAIN_TREE" | cut -c1-8)), in $MAIN_WORKTREE"
else
    red "$VERSION does not merge into main cleanly — resolve it by hand, this script will not"
fi

# --- next ------------------------------------------------------------------
NEXT_PENDING=0
NEXT_WORKTREE="$(worktree_of next)"
if [ -z "$TAG_COMMIT" ]; then
    skip "next's readiness (needs the tag)"
elif ! git rev-parse --verify --quiet refs/heads/next > /dev/null; then
    red "no branch 'next' in this repository"
elif [ -z "$NEXT_WORKTREE" ]; then
    red "branch 'next' is checked out in no worktree — check it out somewhere so the fast-forward has a tree"
elif worktree_dirty "$NEXT_WORKTREE"; then
    red "the worktree holding next ($NEXT_WORKTREE) has uncommitted changes — commit or stash them"
elif [ "$FOLD_PENDING" = 0 ] &&
    git merge-base --is-ancestor refs/heads/main refs/heads/next 2> /dev/null; then
    done_ "next already carries the fold ($(git rev-parse --short refs/heads/next))"
elif git merge-base --is-ancestor refs/heads/next "$TAG_COMMIT"; then
    NEXT_PENDING=1
    ok "next fast-forwards onto the fold, in $NEXT_WORKTREE"
else
    red "next ($(git rev-parse --short refs/heads/next)) has moved past $VERSION — it cannot fast-forward onto the fold; fold by hand (merge the tag into main, push, merge main into next), then RUN THIS SCRIPT AGAIN: it recognizes the finished fold and performs the dispatches (docs.yml, deploy.yml, the manifest check) that a hand-fold skips — v0.40.0's book stayed stale for two days because they were"
fi

# --- the live manifest -----------------------------------------------------
# The end-to-end proof that the new wasm reached Pages. Read-only, so it runs
# in both modes; before the fold it reads the PREVIOUS tag, which is correct
# and not a failure - it is what step 9 will change.
MANIFEST_OK=0
if command -v curl > /dev/null 2>&1 && [ "$TAG_ON_ORIGIN" = 1 ]; then
    manifest="$(curl -fsS --max-time 30 "$MANIFEST_URL" 2> /dev/null || true)"
    compiler="$(printf '%s' "$manifest" |
        sed -n 's/.*"compiler"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')"
    if [ -z "$manifest" ]; then
        say "  note    could not fetch $MANIFEST_URL — step 9 will have to be checked by hand"
    elif [ "$compiler" = "$VERSION" ]; then
        MANIFEST_OK=1
        done_ "the playground manifest already reads $VERSION"
    else
        ok "the playground manifest reads $compiler — step 9 makes it $VERSION"
    fi
else
    skip "the playground manifest (needs curl and the tag on origin)"
fi

# --- the local toolchain ---------------------------------------------------
report_binary() {
    if [ ! -x "$1/vilan" ]; then
        say "  note    no vilan in $1 — nothing to refresh there"
        return
    fi
    reported="$("$1/vilan" --version 2> /dev/null || echo "unreadable")"
    case "$reported" in
        "vilan $NUMBER "* | "vilan $NUMBER") done_ "$1/vilan is $reported" ;;
        *) say "  stale   $1/vilan is $reported, not $NUMBER — step 10 refreshes it" ;;
    esac
}
report_binary "$HOME/.vilan/bin"
report_binary "$HOME/.cargo/bin"

say ""
if [ "$REDS" != 0 ]; then
    say "$REDS precondition(s) red — nothing was run."
    exit 1
fi

# ---------------------------------------------------------------------------
# The plan, then the work. Every command is echoed before it runs.
# ---------------------------------------------------------------------------
say "steps"
say ""

# 6. Fold main.
if [ "$FOLD_PENDING" = 1 ]; then
    run git -C "$MAIN_WORKTREE" merge --no-ff "$VERSION" \
        -m "Merge $VERSION — main catches the release train"
    run git -C "$MAIN_WORKTREE" push origin main
else
    say "  (6) main already carries $VERSION — skipped"
fi

# 7. Fast-forward next onto the fold.
if [ "$NEXT_PENDING" = 1 ]; then
    run git -C "$NEXT_WORKTREE" merge --ff-only main
    run git -C "$NEXT_WORKTREE" push origin next
else
    say "  (7) next already carries the fold — skipped"
fi

# 8/9. The two dispatches, in this order and no other.
dispatch_and_wait() {
    repo="$1"
    workflow="$2"
    run gh workflow run "$workflow" -R "$repo"
    if [ "$DRY_RUN" = 1 ]; then
        plan "gh run list -R $repo --workflow $workflow  # poll until completed, require success"
        return 0
    fi
    say "waiting for $workflow on $repo ..."
    waited=0
    while [ "$waited" -lt "$WAIT_SECONDS" ]; do
        sleep "$POLL_SECONDS"
        waited=$((waited + POLL_SECONDS))
        state="$(gh run list -R "$repo" --workflow "$workflow" --limit 1 \
            --json status,conclusion \
            --jq '.[0] | .status + " " + (.conclusion // "")' 2> /dev/null || echo "unreachable")"
        case "$state" in
            "completed success") return 0 ;;
            "completed"*) fail "$workflow on $repo concluded '${state#completed }' — fix it before going on" ;;
            unreachable) fail "lost contact with $repo while waiting for $workflow" ;;
        esac
    done
    fail "$workflow on $repo did not finish within ${WAIT_SECONDS}s — check it by hand"
}

# The book FIRST: it checks out vilan@main and commits the rebuilt book into
# the pages repo.
dispatch_and_wait "$PAGES_REPO" docs.yml
# The site SECOND: it installs the toolchain from releases/latest and takes the
# playground wasm from that same release, and it pushes the same pages repo
# from a different concurrency group. Running the two together races them.
dispatch_and_wait "$SITE_REPO" deploy.yml

# 9. The manifest is the end-to-end proof; the site deploy's own green is
# necessary and not sufficient.
if [ "$DRY_RUN" = 1 ]; then
    plan "curl -fsS $MANIFEST_URL  # \"compiler\" must read $VERSION"
elif [ "$MANIFEST_OK" = 1 ]; then
    say "  (9) the manifest already reads $VERSION — verified above"
else
    say "verifying $MANIFEST_URL ..."
    waited=0
    while :; do
        compiler="$(curl -fsS --max-time 30 "$MANIFEST_URL" 2> /dev/null |
            sed -n 's/.*"compiler"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')"
        [ "$compiler" != "$VERSION" ] || break
        [ "$waited" -lt 600 ] || fail "the manifest still reads '$compiler', not $VERSION — Pages did not get the new wasm"
        sleep "$POLL_SECONDS"
        waited=$((waited + POLL_SECONDS))
    done
    say "  manifest compiler = $VERSION"
fi

# 10. Both locations, or a stale compiler quietly shadows the release.
run sh "$MAIN_WORKTREE/scripts/install-dev.sh"
if [ "$DRY_RUN" = 1 ]; then
    plan "\$HOME/.vilan/bin/vilan --version   # must read $NUMBER"
    plan "\$HOME/.cargo/bin/vilan --version   # must read $NUMBER"
else
    for directory in "$HOME/.vilan/bin" "$HOME/.cargo/bin"; do
        [ -x "$directory/vilan" ] || continue
        reported="$("$directory/vilan" --version)"
        case "$reported" in
            "vilan $NUMBER "* | "vilan $NUMBER") say "  $directory/vilan is $reported" ;;
            *) fail "$directory/vilan still reports $reported — a stale compiler shadows $VERSION" ;;
        esac
    done
    say ""
    say "restart the language server; vilan-lsp has no --version to check."
fi

say ""
if [ "$DRY_RUN" = 1 ]; then
    say "--dry-run: nothing above was run."
else
    say "$VERSION is folded."
fi
