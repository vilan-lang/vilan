#!/bin/sh
# The from-source counterpart of install.sh — builds the working tree and
# installs it over whatever release is in place:
#
#   scripts/install-dev.sh
#
# Builds `vilan` and `vilan-lsp` in release mode, installs them into
# ~/.vilan/bin (override with $VILAN_INSTALL_DIR — the same directory and
# variable install.sh uses, so a dev build and a release install overwrite
# each other rather than shadowing), and packages the VS Code extension into
# a `.vsix` beside its sources. Idempotent: re-running it updates in place.
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
