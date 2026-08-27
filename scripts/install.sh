#!/bin/sh
# The vilan installer — downloads the latest release for this platform into
# ~/.vilan/bin (override with $VILAN_INSTALL_DIR):
#
#   curl -fsSL https://github.com/vilan-lang/vilan/releases/latest/download/install.sh | sh
#
# Idempotent: re-running it updates in place. It only ever touches the
# install directory.
set -eu

REPO="vilan-lang/vilan"
BASE_URL="https://github.com/$REPO/releases/latest/download"
BIN_DIR="${VILAN_INSTALL_DIR:-$HOME/.vilan/bin}"

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

main() {
    command -v curl > /dev/null 2>&1 || fail "curl is required"
    command -v tar > /dev/null 2>&1 || fail "tar is required"

    asset="vilan-$(target).tar.gz"
    workdir="$(mktemp -d)"
    trap 'rm -rf "$workdir"' EXIT

    say "downloading $asset ..."
    curl -fsSL -o "$workdir/$asset" "$BASE_URL/$asset" \
        || fail "download failed — https://github.com/$REPO/releases"
    curl -fsSL -o "$workdir/sha256sums.txt" "$BASE_URL/sha256sums.txt" \
        || fail "download failed (sha256sums.txt)"
    (cd "$workdir" && checksum "$asset")

    mkdir -p "$BIN_DIR"
    # Remove first so updating a currently-running vilan can't fail on
    # overwrite (ETXTBSY on Linux).
    rm -f "$BIN_DIR/vilan" "$BIN_DIR/vilan-lsp"
    tar -xzf "$workdir/$asset" -C "$BIN_DIR"
    chmod +x "$BIN_DIR/vilan" "$BIN_DIR/vilan-lsp"

    say ""
    say "installed $("$BIN_DIR/vilan" --version) to $BIN_DIR"
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

main
