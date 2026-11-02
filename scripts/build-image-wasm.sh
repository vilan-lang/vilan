#!/bin/sh
# Builds `std::image`'s codec as wasm32 (tracker M185; proposal/image.md §2.3)
# and either COMMITS it or CHECKS the committed copy against it.
#
#   scripts/build-image-wasm.sh           # rebuild crates/vilan-image-wasm/vilan_image.wasm
#   scripts/build-image-wasm.sh --check   # rebuild, smoke it, and fail if the
#                                         # committed blob is not these bytes
#
# The blob is a committed artifact (§6 Q1): the JS backend carries it into every
# node program that reaches `std::image`, and building it inside `vilan-core`'s
# build script would put a nested wasm32 cargo build into every `cargo build` on
# every machine. So it moves only when this script runs — after a change to
# `vilan-image` (or its `image` dependency), and in the same commit.
#
# REPRODUCIBLE, which is what lets CI's `wasm` leg byte-compare a fresh build
# against the commit (a core change without a rebuilt blob goes red there):
#
# - the toolchain is the PINNED one (rust-toolchain.toml), whatever the caller's
#   `RUSTUP_TOOLCHAIN` says — CI's wasm job sets `stable` for the playground's
#   build, and a different rustc is a different binary;
# - the two machine paths a build could bake into the module (panic messages
#   carry source paths) are remapped: this checkout to `/vilan`, the cargo home
#   (where crates.io sources unpack) to `/cargo`; std's are `/rustc/<sha>`
#   already;
# - its own target directory, so these RUSTFLAGS never invalidate the
#   playground build's artifacts in `target/` (cargo fingerprints RUSTFLAGS).
#
# The smoke (scripts/image-wasm-smoke.mjs) decodes a 2x2 PNG through the fresh
# build under node, scales it and encodes it back — N134's rule: a wasm leg
# RUNS what it builds.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
COMMITTED="$ROOT/crates/vilan-image-wasm/vilan_image.wasm"
TARGET_DIR="$ROOT/target/image-wasm"
BUILT="$TARGET_DIR/wasm32-unknown-unknown/image-wasm/vilan_image_wasm.wasm"

check=0
case "${1:-}" in
    --check) check=1 ;;
    "") ;;
    *)
        printf 'usage: %s [--check]\n' "$0" >&2
        exit 2
        ;;
esac

pinned=$(sed -n 's/^channel = "\(.*\)"$/\1/p' "$ROOT/rust-toolchain.toml")
if [ -z "$pinned" ]; then
    printf 'build-image-wasm: no channel in rust-toolchain.toml\n' >&2
    exit 1
fi
# The pinned toolchain's wasm32 target, installed only when it is MISSING — a
# warm machine or CI cache asks rustup one question and prints nothing (an
# unconditional `rustup toolchain install` syncs the channel and logs it).
if command -v rustup > /dev/null 2>&1 &&
    ! rustup target list --installed --toolchain "$pinned" 2> /dev/null |
    grep -qx wasm32-unknown-unknown; then
    rustup toolchain install "$pinned" --profile minimal --no-self-update \
        --target wasm32-unknown-unknown > /dev/null
fi

cargo_home=${CARGO_HOME:-$HOME/.cargo}
RUSTUP_TOOLCHAIN="$pinned" \
    RUSTFLAGS="--remap-path-prefix=$ROOT=/vilan --remap-path-prefix=$cargo_home=/cargo" \
    cargo build -p vilan-image-wasm --profile image-wasm \
    --target wasm32-unknown-unknown --target-dir "$TARGET_DIR"

node "$ROOT/scripts/image-wasm-smoke.mjs" "$BUILT"
ls -l "$BUILT"

if [ "$check" = 1 ]; then
    if cmp -s "$BUILT" "$COMMITTED"; then
        printf 'build-image-wasm: the committed blob is this build (%s)\n' "$pinned"
    else
        printf '%s\n' \
            "build-image-wasm: crates/vilan-image-wasm/vilan_image.wasm is NOT what the" \
            "pinned toolchain ($pinned) builds from this tree - a change to vilan-image" \
            "(or its dependencies) without the rebuilt blob. Run scripts/build-image-wasm.sh" \
            "and commit the result with the change." >&2
        exit 1
    fi
else
    # `cp` would carry the linker output's executable bit into the commit.
    cp "$BUILT" "$COMMITTED"
    chmod 644 "$COMMITTED"
    printf 'build-image-wasm: wrote %s\n' "$COMMITTED"
fi
