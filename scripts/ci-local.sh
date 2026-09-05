#!/bin/sh
# CI's legs, locally (tracker L19). `.github/workflows/ci.yml` does not run the
# commands below — it CALLS THIS SCRIPT, one leg per job, so the local gate and
# the remote one cannot come apart. `crates/vilan-cli/tests/ci_local_script.rs`
# is the machine behind that sentence: every leg job's `run:` step invokes this
# script, and this script's leg list is that job list.
#
#   scripts/ci-local.sh                 # every leg, in order, then a verdict
#   scripts/ci-local.sh fmt clippy      # just these
#   scripts/ci-local.sh --list          # the legs, one per line
#   scripts/ci-local.sh --log-dir DIR   # logs there instead of target/ci-local
#
# Nothing stops at the first red. A run is the instrument that produces the
# COMPLETE failure list — the same reason `.config/nextest.toml` turns fail-fast
# off and the same reason ci.yml's matrix carries `fail-fast: false` — and the
# exit code still gates: non-zero if any leg failed.
#
# Every leg's output is teed to `<log-dir>/<leg>.log`, and the verdict names the
# file beside each result, because the thing you want after a 10-minute run is
# the failing leg's log and not a scrollback search.
#
# THE WINDOWS LEG. ci.yml's `test` job runs the suite on windows-latest as well;
# nothing on a Linux box can. `windows` here is the stand-in and says so: a
# `cargo check --target x86_64-pc-windows-msvc` over the workspace's tests,
# which is what CLAUDE.md already asks of a `#[cfg(windows)]`-only pin. It
# proves the tree BUILDS for that target and nothing about what it does when it
# runs, so it is declared local-only (the pin holds it to that) and GitHub stays
# the seal's final word.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

# The legs, in the order a full run takes them: the cheap text gates first, so a
# misformatted tree is a 20-second red rather than a 10-minute one, then the
# suite, then the two builds, then the cross-check. THIS LIST IS THE JOB LIST —
# ci_local_script.rs holds it against ci.yml's jobs in both directions.
LEGS="fmt vilan-fmt clippy test doctest audit wasm windows"

# Legs with no ci.yml job, and why (see THE WINDOWS LEG above). A leg named here
# must not appear in any workflow `run:` step; the pin checks that too, so a leg
# that quietly becomes a CI job stops being an excuse.
LOCAL_ONLY="windows"

# ── The legs ────────────────────────────────────────────────────────────────
#
# One function each, and the body is the command ci.yml used to carry inline —
# character for character, which is what `release_gate.rs` reads out of this
# file when it holds the CI side against release.yml's.

leg_fmt() {
    cargo fmt --all --check
}

# The tree's `.vl` held to `vilan fmt`'s own answer (tracker N55). Until this
# leg existed the formatter's output was ADVISORY — 96 files, seven of them
# under `vilan/std`, differed from what the compiler in the same commit would
# have written for them, and E137's reflow reached only the one file a gate
# already covered. `.` from the repository root is the whole tree: the corpus,
# std, macro_std, the benchmarks, the examples, the templates and the `.vl`
# fixtures under `crates/`. Products under a declared `generated` root are
# excluded by `fmt` itself (build-hooks.md §12.4), not by an argument here.
leg_vilan_fmt() {
    cargo run --quiet -p vilan-cli -- fmt --check .
}

leg_clippy() {
    cargo clippy --workspace --all-targets -- -D warnings
}

# THE gate command. `VILAN_CI_PARTITION` is nextest's `count:N/M` shard, set by
# ci.yml's matrix so one runner's cold compile is paid twice in parallel instead
# of once in series; unset locally, where the whole suite is the point.
leg_test() {
    if [ -n "${VILAN_CI_PARTITION:-}" ]; then
        cargo nextest run --workspace --partition "count:$VILAN_CI_PARTITION"
    else
        cargo nextest run --workspace
    fi
}

# nextest does not run doc-tests. Every doc-test set is empty today; this leg
# exists so a future doc-test cannot silently stop running — which is also why
# it is a leg of its own rather than two lines inside `test`: sharded, `test`
# runs twice per OS, and a doc-test run twice is a doc-test run once too many.
leg_doctest() {
    cargo test --workspace --doc
}

leg_audit() {
    cargo audit --deny unsound
}

# The playground's compiler reaches wasm32 at all (ci.yml's `wasm` job), with
# the same 64 MiB stack the playground ships with. The bindings are generated
# because a signature wasm-bindgen cannot express must fail here rather than on
# the page. `wasm-bindgen` must be the version `vilan-wasm/Cargo.toml` pins.
leg_wasm() {
    RUSTFLAGS="-C link-arg=-zstack-size=67108864" \
        cargo build -p vilan-wasm --profile wasm-release \
        --target wasm32-unknown-unknown
    wasm-bindgen --target web --out-dir target/wasm-pkg \
        target/wasm32-unknown-unknown/wasm-release/vilan_wasm.wasm
    ls -l target/wasm-pkg/vilan_wasm_bg.wasm
}

# LOCAL ONLY. Not the windows suite — the windows COMPILE. See the header.
leg_windows() {
    cargo check --workspace --all-targets --target x86_64-pc-windows-msvc
}

# ── The runner ──────────────────────────────────────────────────────────────

log_dir="$ROOT/target/ci-local"
requested=""

while [ $# -gt 0 ]; do
    case "$1" in
        --list)
            for leg in $LEGS; do printf '%s\n' "$leg"; done
            exit 0
            ;;
        --log-dir)
            [ $# -ge 2 ] || { echo "--log-dir needs a directory" >&2; exit 2; }
            log_dir=$2
            shift 2
            ;;
        -h|--help)
            sed -n '2,25p' "$0"
            exit 0
            ;;
        -*)
            echo "unknown option: $1" >&2
            exit 2
            ;;
        *)
            known=no
            for leg in $LEGS; do
                [ "$1" = "$leg" ] && known=yes
            done
            if [ "$known" = no ]; then
                echo "unknown leg: $1 (try --list)" >&2
                exit 2
            fi
            requested="$requested $1"
            shift
            ;;
    esac
done

[ -n "$requested" ] || requested=$LEGS

mkdir -p "$log_dir"
log_dir=$(CDPATH= cd -- "$log_dir" && pwd)

# A leg is a shell function named for it, hyphens spelled as underscores.
leg_function() {
    printf 'leg_%s\n' "$(printf '%s' "$1" | tr '-' '_')"
}

for leg in $LEGS; do
    if ! command -v "$(leg_function "$leg")" >/dev/null 2>&1; then
        echo "ci-local.sh: \`$LEGS\` names \`$leg\`, which has no function" >&2
        exit 2
    fi
done

verdict=""
failures=0

for leg in $requested; do
    printf '\n\033[1m== %s ==\033[0m\n' "$leg"
    log_file="$log_dir/$leg.log"
    status_file="$log_dir/.$leg.status"
    function_name=$(leg_function "$leg")
    started=$(date +%s)
    # `set -e` would take the whole script down with the first red leg, and the
    # complete failure list is the point. The status rides out through a file
    # because the subshell is the left side of a pipe.
    (
        set +e
        (
            cd "$ROOT"
            set -x
            "$function_name"
        ) 2>&1
        printf '%s\n' "$?" >"$status_file"
    ) | tee "$log_file"
    status=$(cat "$status_file")
    rm -f "$status_file"
    elapsed=$(( $(date +%s) - started ))
    if [ "$status" -eq 0 ]; then
        mark="ok  "
    else
        mark="FAIL"
        failures=$(( failures + 1 ))
    fi
    verdict="$verdict$(printf '  %-4s %-9s exit %-3s %4ss  %s' \
        "$mark" "$leg" "$status" "$elapsed" "$log_file")
"
done

printf '\n\033[1mci-local verdict\033[0m\n%s' "$verdict"

if [ "$failures" -eq 0 ]; then
    printf '\nall %s leg(s) green\n' "$(printf '%s' "$requested" | wc -w | tr -d ' ')"
    exit 0
fi

printf '\n%s leg(s) RED\n' "$failures"
exit 1
