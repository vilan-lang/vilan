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
# `cargo clippy --target x86_64-pc-windows-msvc` over the workspace's tests,
# which is what CLAUDE.md already asks of a `#[cfg(windows)]`-only pin. It
# proves the tree BUILDS for that target and nothing about what it does when it
# runs, so it is declared local-only (the pin holds it to that) and GitHub stays
# the seal's final word. CLIPPY with `-D warnings`, not `check` (N130): the
# `clippy` leg is ubuntu-only, so a `cfg`-gated import left unused on Windows
# was a warning in every log and red nowhere. `vilan-rt-sqlite` is excluded
# because its bundled SQLite is C, built by a compiler for the target, which a
# Linux box does not have for msvc — the crate has no `cfg(windows)` code, and
# CI's windows `test` leg builds it natively.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

# The legs, in the order a full run takes them: the cheap text gates first, so a
# misformatted tree is a 20-second red rather than a 10-minute one, then the
# suite, then the two builds, then the cross-check. THIS LIST IS THE JOB LIST —
# ci_local_script.rs holds it against ci.yml's jobs in both directions.
LEGS="fmt vilan-fmt clippy test doctest audit wasm perf webview gfx windows"

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
#
# STATED EXCLUSION - the ```vilan fences of `vilan/docs` (and README.md) are
# NOT held to the formatter, and this is a ruling rather than an oversight
# (tracker N58). `.vl` FILES are the gate's subject; a fence is prose that
# happens to compile, and `crates/vilan-core/tests/docs.rs` already holds all
# 208 of them to the compiler, which is the claim that matters about a doc
# example. Two reasons the fmt half is declined:
#
#   1. The formatter would rewrite 152 of the 208 - 1,062 lines - and 91 of
#      those reflows are the same one: an ALIGNED TRAILING COMMENT moved onto a
#      line of its own. `for i in Range::new(0, 3) {   // the end is exclusive`
#      becomes two lines with the comment above the code it annotates. That
#      alignment is how these pages teach; a gate that deletes it every time
#      makes the docs worse to read in exchange for a consistency nobody reading
#      them can see. E147's missing binary-operator break rule (N55's own find,
#      still open) would join it on the conditions.
#   2. There is no extractor to reuse from here. The fence rules live in a TEST
#      target (`docs.rs`, already copied into `parse_differential.rs` because
#      test targets cannot import one another) and `vilan fmt` is a vilan-cli
#      BINARY, so extending this leg means lifting fence extraction into
#      `vilan-core`'s library and teaching `fmt` markdown - a change with its own
#      proposal, not a line in a shell script.
#
# If the exclusion is ever lifted, the reflow is its own commit (N55's shape):
# reformat first, gate second, so the diff is readable as one thing.
#
# The leg reads the EXIT CODE, and since N90 there are three: 0 clean, 1 the
# tree is not formatted, 2 the formatter could not format a file (the
# `declined` lines say which and why). Both non-zero codes fail this leg - the
# distinction is for the reader, who otherwise cannot tell an unformatted tree
# from a printer gap, and before N90 a printer gap read as a clean 0.
leg_vilan_fmt() {
    cargo run --quiet -p vilan-cli -- fmt --check .
}

leg_clippy() {
    cargo clippy --workspace --all-targets -- -D warnings
}

# THE gate command. `VILAN_CI_PARTITION` is nextest's `count:N/M` shard, set by
# ci.yml's matrix so one runner's cold compile is paid twice in parallel instead
# of once in series; unset locally, where the whole suite is the point.
#
# THE PROFILE (N156). The suite runs under Cargo.toml's `ci-test` profile - the
# default `dev` with `vilan-core` at opt-level 1, which more than halves the
# CPU every test pays for its cold std world - and its artifacts land in
# `target/ci-test/`, apart from the `target/debug/` a lane's edit loop uses
# (that loop keeps the ~17 s analyzer rebuild; this one pays ~80 s for it).
# `VILAN_TEST_PROFILE=dev scripts/ci-local.sh test` runs the suite the old way.
TEST_PROFILE=${VILAN_TEST_PROFILE:-ci-test}

# ONE binary stays on `dev`: `deep_nesting`. Its pins are stack-size claims -
# "the walk fits libtest's 2 MiB thread" (the Windows canary), "a 490-link chain
# overruns a DECLARED 2 MiB" - and a frame is exactly what opt-level changes.
# Unoptimized frames are the worst case the canary exists to hold, and at
# opt-level 1 the chain's walk fits (~26 KB a level unoptimized, far less
# optimized), so three declared-stack pins go red there (measured, Order 49).
# So the suite run leaves the binary out and it runs once, on `dev`, after it -
# on the first shard only, since it is one binary and not a share of the suite.
leg_test() {
    status=0
    if [ -n "${VILAN_CI_PARTITION:-}" ]; then
        cargo nextest run --workspace --cargo-profile "$TEST_PROFILE" -E 'not binary(deep_nesting)' --partition "count:$VILAN_CI_PARTITION" || status=$?
    else
        cargo nextest run --workspace --cargo-profile "$TEST_PROFILE" -E 'not binary(deep_nesting)' || status=$?
    fi
    case "${VILAN_CI_PARTITION:-1/1}" in
        1/*) cargo nextest run -p vilan-core --test deep_nesting || status=$? ;;
    esac
    return "$status"
}

# nextest does not run doc-tests. Every doc-test set is empty today; this leg
# exists so a future doc-test cannot silently stop running — which is also why
# it is a leg of its own rather than two lines inside `test`: sharded, `test`
# runs twice per OS, and a doc-test run twice is a doc-test run once too many.
leg_doctest() {
    cargo test --workspace --doc --profile "$TEST_PROFILE"
}

leg_audit() {
    cargo audit --deny unsound
}

# The playground's compiler reaches wasm32 at all (ci.yml's `wasm` job), with
# the same 16 MiB stack the playground ships with (release.yml; this said 64 MiB
# and linked 64 MiB for a release after the parser's bound let release.yml drop
# to 16 — `vilan-wasm`'s `WASM_STACK_SIZE` pin holds both now, N128). The bindings are generated
# because a signature wasm-bindgen cannot express must fail here rather than on
# the page. `wasm-bindgen` must be the version `vilan-wasm/Cargo.toml` pins.
leg_wasm() {
    RUSTFLAGS="-C link-arg=-zstack-size=16777216" \
        cargo build -p vilan-wasm --profile wasm-release \
        --target wasm32-unknown-unknown
    wasm-bindgen --target web --out-dir target/wasm-pkg \
        target/wasm32-unknown-unknown/wasm-release/vilan_wasm.wasm
    ls -l target/wasm-pkg/vilan_wasm_bg.wasm
    # M185: the playground's compiler is built WITHOUT vilan-core's
    # `image-codec` feature, so it must not carry `std::image`'s ~780 KB codec
    # blob — whose base64 begins with these 64 characters wherever it is
    # embedded. And without the feature a program that reaches the codec is
    # refused by name, which the workspace suite (always built WITH it) cannot
    # see, so that pin runs here.
    blob_head=$(head -c 48 crates/vilan-image-wasm/vilan_image.wasm | base64 | tr -d '\n')
    if grep -qaF "$blob_head" target/wasm-pkg/vilan_wasm_bg.wasm; then
        printf '%s\n' "the playground's compiler carries the image codec blob: something" \
            "enabled vilan-core's \`image-codec\` feature in vilan-wasm's graph" >&2
        return 1
    fi
    cargo test -p vilan-core --no-default-features --test image_codec_feature
    # And RUN it (N134): v0.41.0 shipped a compiler that built, bound, and
    # trapped on every compile. The smoke is the page's four claims under node.
    node scripts/wasm-smoke.mjs target/wasm-pkg
    # M185: `std::image`'s codec, the OTHER wasm32 artifact — committed, and
    # carried into every node program that reaches the module. Built fresh with
    # the PINNED toolchain (the script sets it, whatever this job's
    # `RUSTUP_TOOLCHAIN` says), run under node (a 2x2 PNG decoded, scaled and
    # re-encoded), and byte-compared against the committed blob, so a
    # `vilan-image` change without the rebuilt blob is red here.
    scripts/build-image-wasm.sh --check
}

# The instruction budgets (M105 S3, `proposal/performance-gates.md` §3's T2).
# A RELEASE build — counts are taken on the binary users run, and debug/release
# differ by more than a constant (§2.4) — then `perf_gate.py gate`: every row
# of `perf/budgets.toml` against its ceiling for this machine's CLASS
# (`VILAN_PERF_CLASS`; ci.yml sets `ci`, the reference machine `reference`), and
# the growth rows (a package twice the size costs at most x2.3) everywhere.
# Counts, never clocks: `instructions:u` from the hardware counter, callgrind's
# Ir where the machine exposes no PMU. A class with no ceilings yet (`local`)
# reports its counts and refuses only on growth; `reference` and `ci` have
# theirs (`ci`'s adopted from the CI job's own counts, M114).
leg_perf() {
    cargo build --release -p vilan-cli
    python3 scripts/perf_gate.py gate --vilan target/release/vilan \
        --class "${VILAN_PERF_CLASS:-local}" --json target/perf-gate/measured.json
}

# F1 M2 (native-objective.md D2a, RULED 2026-11-10): `std::window`'s runtime
# crate, `crates/vilan-rt-webview` (`wry` + `tao`), is OUTSIDE the compiler's
# workspace with a lockfile of its own, so no leg above reaches it; this one
# holds it to the same bars, with its own two written exceptions. It needs the
# WebKitGTK and GTK development packages and a virtual display (ci.yml's
# `webview` job installs `libwebkit2gtk-4.1-dev libgtk-3-dev xvfb`), plus
# `cargo-audit` and `cargo-about`.
#
# - RUSTSEC-2024-0429 is ignored BY NAME: an `unsound` advisory against glib
#   0.18's `VariantStrIter` (`Variant::array_iter_str`). glib 0.18 is the last
#   line of the gtk3 bindings, which WebKitGTK 4.1 (and so `wry` on Linux)
#   needs; the fixed glib is gtk4-only, so no gtk3 release will carry the fix.
#   Nothing in this crate's tree calls it: a grep of every crate in its
#   lockfile (2026-11-10, 252 packages) finds `VariantStrIter` and
#   `array_iter_str` only in glib's own definition and glib's own tests.
# - The notices are regenerated with the crate's own `about.toml` (MPL-2.0 and
#   Apache-2.0 WITH LLVM-exception accepted there, each with its reason) and
#   must match the committed `THIRD-PARTY-NOTICES.txt` byte for byte, so a
#   licence outside the list, or a lockfile change without its notices, is red.
# - The end-to-end proof (`desktop_shell.rs`) runs REQUIRED here: it skips
#   elsewhere when there is no WebKitGTK or xvfb, and must not skip here.
leg_webview() {
    status=0
    (
        cd crates/vilan-rt-webview
        cargo fmt --check || exit 1
        cargo clippy --all-targets -- -D warnings || exit 1
        cargo test || exit 1
        cargo audit --deny unsound --ignore RUSTSEC-2024-0429 || exit 1
        # `--offline`: the licence texts come from the crates' own files in
        # the lockfile's sources, which the steps above already fetched.
        # Without it cargo-about also asks clearlydefined.io, whose answer
        # for a crate changes when it harvests a new version, and this byte
        # check went red on CI with nothing changed here (run 43519114590).
        # The notices cover every platform's crates, so fetch the whole
        # lockfile first: a Linux build never downloaded the Windows ones.
        cargo fetch --locked || exit 1
        notices=$(mktemp)
        cargo about generate --offline -c about.toml ../../about.hbs -o "$notices" || exit 1
        tr -d '\r' <"$notices" | cmp - THIRD-PARTY-NOTICES.txt || exit 1
        rm -f "$notices"
    ) || status=$?
    VILAN_REQUIRE_WEBVIEW_E2E=1 cargo nextest run -p vilan-cli --test desktop_shell || status=$?
    return "$status"
}

# native-ui.md P1 (R11, RULED 2026-11-10): `std::gfx`'s runtime crate,
# `crates/vilan-rt-gfx` (`wgpu` + `winit` + `parley`), is OUTSIDE the
# compiler's workspace with a lockfile of its own, so no leg above reaches it;
# this one holds it to the same bars with NO exception: its fmt, clippy, its
# tests, `cargo audit --deny unsound`, and its own notices
# (`crates/vilan-rt-gfx/about.toml`, the compiler's accepted list unchanged)
# regenerated and compared byte for byte. It needs a GPU adapter (ci.yml's
# `gfx` job installs Mesa's software Vulkan, `mesa-vulkan-drivers`), `xvfb`,
# `cargo-audit`, `cargo-about` and `cargo-nextest`; no development package
# (fontconfig is opened at run time).
#
# - `VILAN_REQUIRE_GFX=1`: the golden images (`tests/golden.rs`, a tolerance
#   of one per channel) and the window proof (`tests/window.rs`, the window
#   example under xvfb) skip elsewhere when there is no software adapter or no
#   xvfb, and must not skip here.
# - The `std::gfx` end-to-end proof (`crates/vilan-cli/tests/gfx_window.rs`):
#   the compiler builds `vilan/examples/gfx` with the Rust backend, and the
#   counter runs in a real window under xvfb, driven by pointer input injected
#   through the X server, with its scene held to golden images offscreen. It
#   builds the compiler, so this leg needs the workspace's toolchain and
#   `cargo-nextest` too; REQUIRED here by the same `VILAN_REQUIRE_GFX=1`.
# - The `std::ui` proof (`crates/vilan-cli/tests/ui_screen.rs`, native-ui.md
#   P2): `vilan/examples/ui` built the same way, its screen laid out at two
#   window sizes and two scales and held to recorded rectangles, drawn
#   offscreen and held to golden images, and shown in a window under xvfb;
#   REQUIRED here too. The crate's own `cargo test` above runs the element
#   tree's unit pins (`src/ui/tests.rs`).
# - The `std::ui` interaction proof (`crates/vilan-cli/tests/ui_interaction.rs`,
#   native-ui.md P3's exit test): `vilan/examples/ui-counter` driven with no
#   window and held to a recorded listing, then in a window under xvfb by
#   pointer AND keyboard through XTEST; `vilan/examples/ui-list` (10,000 rows
#   in a scroll) scrolled by the wheel and clicked; and P4's exit test,
#   `vilan/examples/ui-todo` on `std::ui::kit`, held to its own recorded
#   listing and then typed into and clicked in a window. REQUIRED here too.
leg_gfx() {
    status=0
    (
        cd crates/vilan-rt-gfx
        cargo fmt --check || exit 1
        cargo clippy --all-targets -- -D warnings || exit 1
        VILAN_REQUIRE_GFX=1 cargo test || exit 1
        cargo audit --deny unsound || exit 1
        # `--offline`: the licence texts come from the crates' own files in
        # the lockfile's sources, which the steps above already fetched.
        # Without it cargo-about also asks clearlydefined.io, whose answer
        # for a crate changes when it harvests a new version, and this byte
        # check went red on CI with nothing changed here (run 43519114590).
        # The notices cover every platform's crates, so fetch the whole
        # lockfile first: a Linux build never downloaded the Windows ones.
        cargo fetch --locked || exit 1
        notices=$(mktemp)
        cargo about generate --offline -c about.toml ../../about.hbs -o "$notices" || exit 1
        tr -d '\r' <"$notices" | cmp - THIRD-PARTY-NOTICES.txt || exit 1
        rm -f "$notices"
    ) || status=$?
    VILAN_REQUIRE_GFX=1 cargo nextest run -p vilan-cli --test gfx_window --test ui_screen \
        --test ui_interaction ||
        status=$?
    return "$status"
}

# LOCAL ONLY. Not the windows suite — the windows COMPILE. See the header.
# The second and third commands are the webview and GPU crates' (outside the
# workspace, so the first does not reach them): `wry`'s WebView2 half and
# `tao`'s Win32 half, and `wgpu`'s DirectX 12 and `winit`'s Win32 halves,
# cross-check from Linux, which nothing in CI does.
leg_windows() {
    status=0
    cargo clippy --workspace --exclude vilan-rt-sqlite --all-targets \
        --target x86_64-pc-windows-msvc -- -D warnings || status=$?
    (cd crates/vilan-rt-webview && cargo clippy --all-targets \
        --target x86_64-pc-windows-msvc -- -D warnings) || status=$?
    (cd crates/vilan-rt-gfx && cargo clippy --all-targets \
        --target x86_64-pc-windows-msvc -- -D warnings) || status=$?
    return "$status"
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
