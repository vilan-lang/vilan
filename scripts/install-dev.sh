#!/bin/sh
# The from-source counterpart of install.sh — builds the working tree and
# installs it over whatever release is in place:
#
#   scripts/install-dev.sh
#
# Builds `vilan` and `vilan-lsp` in release mode, installs them into
# ~/.vilan/bin (override with $VILAN_INSTALL_DIR — the same directory and
# variable install.sh uses, so a dev build and a release install overwrite
# each other rather than shadowing), refreshes any older pair already sitting
# in ~/.cargo/bin (override with $VILAN_MIRROR_DIR, set it to $VILAN_INSTALL_DIR
# to skip), and packages the VS Code extension into a `.vsix` beside its
# sources. Idempotent: re-running it updates in place.
set -eu

BIN_DIR="${VILAN_INSTALL_DIR:-$HOME/.vilan/bin}"

say() { printf '%s\n' "$1"; }
fail() { printf 'install-dev: %s\n' "$1" >&2; exit 1; }

cd "$(dirname "$0")/.."

command -v cargo > /dev/null 2>&1 || fail "cargo is required"
command -v npm > /dev/null 2>&1 || fail "npm is required (for the VS Code extension)"

say "building vilan and vilan-lsp (release) ..."
cargo build --release -p vilan-cli -p vilan-lsp

mkdir -p "$BIN_DIR"
# Remove first so replacing a currently-running vilan can't fail on
# overwrite (ETXTBSY on Linux).
rm -f "$BIN_DIR/vilan" "$BIN_DIR/vilan-lsp"
cp target/release/vilan target/release/vilan-lsp "$BIN_DIR/"
chmod +x "$BIN_DIR/vilan" "$BIN_DIR/vilan-lsp"

say "installed $("$BIN_DIR/vilan" --version) to $BIN_DIR"

# A `vilan` that predates this script keeps answering from wherever it sits on
# PATH — most often a `cargo install` copy in ~/.cargo/bin, which rustup's shell
# setup prepends — and a stale one is indistinguishable from a fresh build until
# you compare `--version` commit hashes. Worse for `vilan-lsp`, which has no
# `--version`: an old server squiggles syntax the new compiler accepts. So
# refresh a copy that is *already* there, and leave a directory without one
# alone — creating install locations is install.sh's business, not this script's.
MIRROR_DIR="${VILAN_MIRROR_DIR:-$HOME/.cargo/bin}"
if [ "$MIRROR_DIR" != "$BIN_DIR" ] &&
    { [ -e "$MIRROR_DIR/vilan" ] || [ -e "$MIRROR_DIR/vilan-lsp" ]; }; then
    rm -f "$MIRROR_DIR/vilan" "$MIRROR_DIR/vilan-lsp"
    cp target/release/vilan target/release/vilan-lsp "$MIRROR_DIR/"
    chmod +x "$MIRROR_DIR/vilan" "$MIRROR_DIR/vilan-lsp"
    say "refreshed the older copy in $MIRROR_DIR (it would otherwise shadow)"
fi

say ""
say "packaging the VS Code extension ..."
(
    cd editors/vscode
    # `npm ci` when starting clean (it is what CI does); plain `npm install`
    # after that, so the everyday re-run doesn't rebuild node_modules.
    if [ -d node_modules ]; then
        npm install
    else
        npm ci
    fi
    # vsce's prepublish hook runs the esbuild bundle; the .vsix lands beside
    # the sources as vilan-<version>.vsix, as a release build's would.
    npx --yes @vscode/vsce package
)

vsix="$(ls editors/vscode/vilan-*.vsix | tail -n 1)"
say ""
say "packaged $vsix"
say "install it with: code --install-extension $vsix"

case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *)
        say ""
        say "add $BIN_DIR to your PATH — for bash/zsh:"
        say ""
        say "    export PATH=\"\$HOME/.vilan/bin:\$PATH\""
        say ""
        say "(append that line to ~/.bashrc or ~/.zshrc; fish users:"
        say "fish_add_path ~/.vilan/bin)"
        ;;
esac
