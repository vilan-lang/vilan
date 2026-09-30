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
# to skip), packages the VS Code extension into a `.vsix` beside its sources,
# and installs that into VS Code (E229 door d) — a VS Code Server's own CLI
# (~/.vscode-server: WSL, Remote-SSH, Tunnels) and/or `code` on PATH — so the
# editor never runs an extension older than the server this just installed.
# Opt out of the editor step with VILAN_NO_VSCODE=1. Idempotent: re-running it
# updates in place.
#
# This is the integrator's toolchain refresh at every seal: run it from the
# sealed worktree and both `vilan` locations AND the editor move to the tip.
set -eu

BIN_DIR="${VILAN_INSTALL_DIR:-$HOME/.vilan/bin}"

say() { printf '%s\n' "$1"; }
fail() { printf 'install-dev: %s\n' "$1" >&2; exit 1; }

cd "$(dirname "$0")/.."

command -v cargo > /dev/null 2>&1 || fail "cargo is required"
command -v npm > /dev/null 2>&1 || fail "npm is required (for the VS Code extension)"

say "building vilan and vilan-lsp (release) ..."
cargo build --release -p vilan-cli -p vilan-lsp

# `vilan --version` and `vilan-lsp --version` carry the commit they were built
# from, stamped by crates/vilan-cli/build_stamp.rs (both build scripts read it),
# which re-runs when the HEAD it resolved moves — a linked worktree's too, since
# E231. Still refuse to install a binary that names the wrong commit (a stamp
# can go stale in ways no rerun-if-changed sees): re-stamp and build once more.
head="$(git rev-parse --short=9 HEAD 2> /dev/null || true)"
if [ -n "$head" ]; then
    for binary in vilan vilan-lsp; do
        case "$(target/release/$binary --version)" in
            *"($head)"* | *"($head-dirty)"*) ;;
            *)
                say "the $binary version stamp is stale (not $head) — re-stamping ..."
                touch crates/vilan-cli/build.rs crates/vilan-lsp/build.rs
                cargo build --release -p vilan-cli -p vilan-lsp
                break
                ;;
        esac
    done
fi

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
# you compare `--version` commit hashes. Worse for `vilan-lsp`, whose stale
# copy squiggles syntax the new compiler accepts. So
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
    # E231: the commit this extension is packaged from, which the extension
    # compares with the server's `serverInfo.version` stamp when the versions
    # agree — the one drift a dev build between releases can have. The
    # server's stamp reads `<sha>` or `<sha>-dirty` from the same HEAD.
    git rev-parse --short=9 HEAD > build-sha.txt 2> /dev/null || rm -f build-sha.txt
    # vsce's prepublish hook runs the esbuild bundle; the .vsix lands beside
    # the sources as vilan-<version>.vsix, as a release build's would. Named
    # explicitly, so an older one left beside it is never the one installed.
    npx --yes @vscode/vsce package --out "vilan-$(node -p "require('./package.json').version").vsix"
)

vsix="$PWD/editors/vscode/vilan-$(node -p "require('./editors/vscode/package.json').version").vsix"
say ""
say "packaged $vsix"

# The newest VS Code Server's CLI on this machine, or nothing. Every server
# build under ~/.vscode-server shares one extensions directory, so one install
# reaches whichever the editor runs.
vscode_server_cli() {
    newest=""
    for candidate in "$HOME"/.vscode-server/bin/*/bin/code-server \
        "$HOME"/.vscode-server/cli/servers/*/server/bin/code-server; do
        [ -x "$candidate" ] || continue
        if [ -z "$newest" ] || [ "$candidate" -nt "$newest" ]; then
            newest="$candidate"
        fi
    done
    [ -n "$newest" ] && echo "$newest"
    return 0
}

if [ -n "${VILAN_NO_VSCODE:-}" ]; then
    say "not installing it into VS Code (VILAN_NO_VSCODE is set) — by hand:"
    say "    code --install-extension $vsix --force"
else
    installed=""
    server="$(vscode_server_cli)"
    if [ -n "$server" ]; then
        if "$server" --install-extension "$vsix" --force > /dev/null; then
            say "installed it into the VS Code server ($HOME/.vscode-server)"
            installed=1
        else
            say "the VS Code server's CLI refused it ($server)"
        fi
    fi
    # Inside WSL, `code` is the Windows launcher, which forwards to the very
    # server just installed into — asking it again would only be slower.
    if command -v code > /dev/null 2>&1 &&
        { [ -z "$installed" ] || [ -z "${WSL_DISTRO_NAME:-}" ]; }; then
        if code --install-extension "$vsix" --force > /dev/null; then
            say "installed it into VS Code (\`code\` on PATH)"
            installed=1
        else
            say "\`code --install-extension\` refused it"
        fi
    fi
    if [ -n "$installed" ]; then
        say "reload the VS Code window (Developer: Reload Window) to run it"
    else
        say "no VS Code found to install it into — by hand:"
        say "    code --install-extension $vsix --force"
    fi
fi

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
