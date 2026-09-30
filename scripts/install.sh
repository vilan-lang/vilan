#!/bin/sh
# The vilan installer — downloads the latest release for this platform into
# ~/.vilan/bin (override with $VILAN_INSTALL_DIR):
#
#   curl -fsSL https://github.com/vilan-lang/vilan/releases/latest/download/install.sh | sh
#
# Idempotent: re-running it updates in place. It touches the install
# directory and, when VS Code is on this machine, its extensions: the Vilan
# extension is installed alongside the toolchain (E229), because an editor
# extension older than its language server silently lacks what the server's
# release notes promise. From the gallery by id when the editor can reach it —
# an install VS Code keeps updated from then on — and otherwise from the
# release's own `vilan-vscode.vsix`, verified with the toolchain (E230; `vilan
# upgrade` runs the same step). Opt out with VILAN_NO_VSCODE=1, or a flag:
#
#   curl -fsSL https://github.com/vilan-lang/vilan/releases/latest/download/install.sh | sh -s -- --no-vscode
set -eu

REPO="vilan-lang/vilan"
BASE_URL="https://github.com/$REPO/releases/latest/download"
BIN_DIR="${VILAN_INSTALL_DIR:-$HOME/.vilan/bin}"
VSIX="vilan-vscode.vsix"
EXTENSION_ID="vilan-lang.vilan"

say() { printf '%s\n' "$1"; }
fail() { printf 'install: %s\n' "$1" >&2; exit 1; }

target() {
    os="$(uname -s)"
    arch="$(uname -m)"
    case "$os" in
        Linux)
            case "$arch" in
                x86_64) echo "x86_64-unknown-linux-musl" ;;
                aarch64 | arm64) echo "aarch64-unknown-linux-musl" ;;
                *) fail "unsupported Linux architecture: $arch" ;;
            esac
            ;;
        Darwin)
            case "$arch" in
                x86_64) echo "x86_64-apple-darwin" ;;
                arm64) echo "aarch64-apple-darwin" ;;
                *) fail "unsupported macOS architecture: $arch" ;;
            esac
            ;;
        MINGW* | MSYS* | CYGWIN* | Windows_NT)
            fail "this script installs the unix build — for native Windows, run the PowerShell installer instead:
    irm https://github.com/$REPO/releases/latest/download/install.ps1 | iex"
            ;;
        *) fail "unsupported platform: $os" ;;
    esac
}

# Verifies $1 against the release's sha256sums.txt in the current directory.
#
# Fails CLOSED (L15): a machine with no sha256 tool is not a machine that may
# install unverified bytes, it is a machine that cannot complete the install.
# Skipping the check here would put the weakest verification on exactly the
# host least able to notice — and an installer that sometimes verifies makes a
# promise it does not keep. Every supported platform can satisfy this:
# `sha256sum` ships with GNU coreutils, `shasum` with macOS and with perl.
checksum() {
    line="$(grep " $1\$" sha256sums.txt)" || fail "sha256sums.txt has no entry for $1"
    if command -v sha256sum > /dev/null 2>&1; then
        printf '%s\n' "$line" | sha256sum -c - > /dev/null || fail "checksum mismatch for $1"
    elif command -v shasum > /dev/null 2>&1; then
        printf '%s\n' "$line" | shasum -a 256 -c - > /dev/null || fail "checksum mismatch for $1"
    else
        fail "cannot verify $1: no sha256 tool on PATH. Install one (\`sha256sum\` from coreutils, or \`shasum\`) and re-run; nothing is installed unverified."
    fi
}

# The command that installs a VS Code extension on this machine, or nothing.
# `code` on PATH first — on a desktop that is the editor itself, and inside WSL
# it is VS Code's launcher, which forwards to the WSL server. Failing that, a
# VS Code Server's own CLI (Remote-SSH, Tunnels, WSL without the launcher on
# PATH): it installs into ~/.vscode-server/extensions, which is where the
# remote editor loads extensions from. The newest server wins.
vscode_cli() {
    if command -v code > /dev/null 2>&1; then
        echo code
        return
    fi
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

main() {
    no_vscode="${VILAN_NO_VSCODE:-}"
    for argument in "$@"; do
        case "$argument" in
            --no-vscode) no_vscode=1 ;;
            *) fail "unknown option: $argument (the one option is --no-vscode)" ;;
        esac
    done

    command -v curl > /dev/null 2>&1 || fail "curl is required"
    command -v tar > /dev/null 2>&1 || fail "tar is required"

    asset="vilan-$(target).tar.gz"
    workdir="$(mktemp -d)"
    trap 'rm -rf "$workdir"' EXIT

    # The editor is decided BEFORE anything downloads, so the extension is
    # verified with the toolchain and a mismatch on either refuses both —
    # nothing is installed from a release that did not verify whole.
    editor=""
    if [ -z "$no_vscode" ]; then
        editor="$(vscode_cli)"
    fi

    say "downloading $asset ..."
    curl -fsSL -o "$workdir/$asset" "$BASE_URL/$asset" \
        || fail "download failed — https://github.com/$REPO/releases"
    curl -fsSL -o "$workdir/sha256sums.txt" "$BASE_URL/sha256sums.txt" \
        || fail "download failed (sha256sums.txt)"
    (cd "$workdir" && checksum "$asset")
    if [ -n "$editor" ]; then
        say "downloading $VSIX ..."
        curl -fsSL -o "$workdir/$VSIX" "$BASE_URL/$VSIX" \
            || fail "download failed ($VSIX) — re-run with --no-vscode to install the toolchain alone"
        (cd "$workdir" && checksum "$VSIX")
    fi

    mkdir -p "$BIN_DIR"
    # Remove first so updating a currently-running vilan can't fail on
    # overwrite (ETXTBSY on Linux).
    rm -f "$BIN_DIR/vilan" "$BIN_DIR/vilan-lsp"
    tar -xzf "$workdir/$asset" -C "$BIN_DIR"
    chmod +x "$BIN_DIR/vilan" "$BIN_DIR/vilan-lsp"

    # The toolchain is in place; an editor that refuses the extension is
    # reported, never a reason to call the install failed.
    if [ -n "$no_vscode" ]; then
        extension="VS Code extension: not installed (--no-vscode / VILAN_NO_VSCODE)"
    elif [ -z "$editor" ]; then
        extension="VS Code extension: not installed (no \`code\` on PATH) — it is $VSIX on https://github.com/$REPO/releases"
    elif "$editor" --install-extension "$EXTENSION_ID" --force > "$workdir/editor.log" 2>&1; then
        extension="VS Code extension: installed $EXTENSION_ID from the gallery with \`$editor\` (it updates itself from now on) — reload VS Code to use it"
    elif "$editor" --install-extension "$workdir/$VSIX" --force > "$workdir/editor.log" 2>&1; then
        extension="VS Code extension: installed $VSIX (the gallery was unreachable) with \`$editor\` — reload VS Code to use it"
    else
        extension="VS Code extension: NOT installed — \`$editor --install-extension\` failed: $(tail -n 1 "$workdir/editor.log")"
    fi

    say ""
    say "installed $("$BIN_DIR/vilan" --version) to $BIN_DIR"
    say "$extension"
    case ":$PATH:" in
        *":$BIN_DIR:"*) ;;
        *)
            say ""
            say "add it to your PATH — for bash/zsh:"
            say ""
            say "    export PATH=\"\$HOME/.vilan/bin:\$PATH\""
            say ""
            say "(append that line to ~/.bashrc or ~/.zshrc; fish users:"
            say "fish_add_path ~/.vilan/bin)"
            ;;
    esac
    say ""
    say "get started: https://vilan-lang.org/docs/"
}

main "$@"
