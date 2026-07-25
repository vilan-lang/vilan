#!/bin/sh
# Assembles the six npm packages of proposal/distribution.md §2 from a
# directory of release archives:
#
#   scripts/npm-package.sh 0.14.0 release-assets npm-dist
#
# — where `release-assets` holds the `vilan-<target>.tar.gz` / `.zip` files the
# release build matrix produced, and `npm-dist` is written with one directory
# per package, ready for `npm publish`. Publishing is the release workflow's
# job (.github/workflows/release.yml), which is this script's only caller in
# anger; run it by hand to inspect what a release would ship.
#
# The version lives nowhere in the npm/ tree — every manifest there carries
# `0.0.0-placeholder`, and this script stamps the release's version into all
# six (including the meta package's version-locked optionalDependencies) and
# then re-reads them to prove the stamping took. That is why
# scripts/bump-version.sh knows nothing about npm.
set -eu

VERSION="${1:?usage: scripts/npm-package.sh <version> <archives-dir> <out-dir>}"
ARCHIVES="${2:?usage: scripts/npm-package.sh <version> <archives-dir> <out-dir>}"
OUT="${3:?usage: scripts/npm-package.sh <version> <archives-dir> <out-dir>}"

PLACEHOLDER="0.0.0-placeholder"

# rust target : npm platform package : archive extension.
#
# These five rows are the release matrix's five targets
# (.github/workflows/release.yml), and the middle column is exactly the key set
# of the stub's own table (npm/meta/lib/launch.js, keyed by node's
# `${process.platform}-${process.arch}`). crates/vilan-cli/tests/npm_stub.rs
# pins the stub's table against the npm/platform/ directories, so a target
# added here without its package — or a package without a table entry — fails
# the suite rather than shipping a vilan that cannot find its own binary.
TARGETS="x86_64-unknown-linux-musl:linux-x64:tar.gz
aarch64-unknown-linux-musl:linux-arm64:tar.gz
x86_64-apple-darwin:darwin-x64:tar.gz
aarch64-apple-darwin:darwin-arm64:tar.gz
x86_64-pc-windows-msvc:win32-x64:zip"

fail() { printf 'npm-package: %s\n' "$1" >&2; exit 1; }

# The argument paths are the caller's, so resolve them before this script cds
# to the repository root.
absolute() {
    case "$1" in
        /*) printf '%s\n' "$1" ;;
        *) printf '%s\n' "$PWD/$1" ;;
    esac
}

ARCHIVES="$(absolute "$ARCHIVES")"
OUT="$(absolute "$OUT")"
cd "$(dirname "$0")/.."

command -v node > /dev/null 2>&1 || fail "node is required"

[ -d "$ARCHIVES" ] || fail "no such archive directory: $ARCHIVES"
rm -rf "$OUT"
mkdir -p "$OUT"

# Stamps the release version over every placeholder in $1, in place. `sed -i`
# spells its backup suffix differently on GNU and BSD, so write beside and move.
stamp() {
    sed "s/$PLACEHOLDER/$VERSION/g" "$1" > "$1.stamped"
    mv "$1.stamped" "$1"
}

for row in $TARGETS; do
    target="${row%%:*}"
    rest="${row#*:}"
    package="${rest%%:*}"
    extension="${rest##*:}"

    archive="$ARCHIVES/vilan-$target.$extension"
    [ -f "$archive" ] || fail "missing release archive: $archive"

    unpacked="$OUT/.unpacked-$package"
    rm -rf "$unpacked"
    mkdir -p "$unpacked" "$OUT/$package/bin"
    case "$extension" in
        # `tar -xf`, not `-xzf`: both tars in play detect gzip themselves.
        tar.gz) tar -xf "$archive" -C "$unpacked" ;;
        # GNU tar cannot read a zip at all (bsdtar can) — and the runner
        # assembling these is ubuntu, whose `tar` is GNU's.
        zip)
            command -v unzip > /dev/null 2>&1 || fail "unzip is required to unpack $archive"
            unzip -q -o "$archive" -d "$unpacked"
            ;;
        *) fail "unknown archive kind: $extension" ;;
    esac

    if [ "$package" = win32-x64 ]; then
        suffix=".exe"
    else
        suffix=""
    fi
    for binary in vilan vilan-lsp; do
        [ -f "$unpacked/$binary$suffix" ] || fail "$archive carries no $binary$suffix"
        cp "$unpacked/$binary$suffix" "$OUT/$package/bin/$binary$suffix"
    done
    # The executable bit has to be set *inside the package*: npm's tarball
    # records the mode, and that recorded mode is the one a user's install
    # gets. (Meaningless for the Windows build, which is why it is skipped
    # there rather than applied blindly.)
    if [ -z "$suffix" ]; then
        chmod +x "$OUT/$package/bin/vilan" "$OUT/$package/bin/vilan-lsp"
    fi

    # The archives carry both licenses; a package shipping the binary ships
    # them with it.
    cp "$unpacked/LICENSE-MIT" "$unpacked/LICENSE-APACHE" "$OUT/$package/"
    cp "npm/platform/$package/package.json" "$OUT/$package/package.json"
    stamp "$OUT/$package/package.json"
    rm -rf "$unpacked"
done

# The meta package: the two stubs, their shared resolution module, the readme
# npm renders on the package page, and the licenses from the repository root.
mkdir -p "$OUT/vilan"
cp -R npm/meta/bin npm/meta/lib "$OUT/vilan/"
cp npm/meta/package.json npm/meta/README.md LICENSE-MIT LICENSE-APACHE "$OUT/vilan/"
chmod +x "$OUT/vilan/bin/vilan.js" "$OUT/vilan/bin/vilan-lsp.js"
stamp "$OUT/vilan/package.json"

# Re-read what was written. A manifest that still carries the placeholder is a
# publishable semver string, so nothing downstream would catch it: the version
# would simply be wrong on the registry, forever.
NPM_OUT="$OUT" NPM_VERSION="$VERSION" node << 'VERIFY'
const fs = require("fs");
const path = require("path");

const out = process.env.NPM_OUT;
const version = process.env.NPM_VERSION;
const platforms = ["linux-x64", "linux-arm64", "darwin-x64", "darwin-arm64", "win32-x64"];
const problems = [];

const read = (name) =>
    JSON.parse(fs.readFileSync(path.join(out, name, "package.json"), "utf8"));

for (const name of [...platforms, "vilan"]) {
    const manifest = read(name);
    if (manifest.version !== version) {
        problems.push(`${manifest.name}: version is ${manifest.version}, expected ${version}`);
    }
}

const meta = read("vilan");
const optional = meta.optionalDependencies ?? {};
const expected = platforms.map((name) => `@vilan-lang/${name}`).sort();
if (Object.keys(optional).sort().join() !== expected.join()) {
    problems.push(`the meta package's optionalDependencies are ${Object.keys(optional)}`);
}
for (const [name, range] of Object.entries(optional)) {
    if (range !== version) {
        problems.push(`${name} is depended on at ${range}, not the exact ${version}`);
    }
}
for (const name of platforms) {
    const suffix = name.startsWith("win32") ? ".exe" : "";
    for (const binary of ["vilan", "vilan-lsp"]) {
        const file = path.join(out, name, "bin", `${binary}${suffix}`);
        if (!fs.existsSync(file)) {
            problems.push(`${name} is missing bin/${binary}${suffix}`);
        } else if (!suffix && !(fs.statSync(file).mode & 0o111)) {
            problems.push(`${name}'s bin/${binary} is not executable`);
        }
    }
}

if (problems.length > 0) {
    console.error("npm-package: the assembled packages are wrong:");
    for (const problem of problems) console.error(`  ${problem}`);
    process.exit(1);
}
VERIFY

printf 'assembled %s: %s\n' "$VERSION" "$OUT"
