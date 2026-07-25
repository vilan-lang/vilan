# The npm channel

The sources for the six packages `npm install -g @vilan-lang/vilan` delivers
(proposal/distribution.md §2). Nothing here is published from a working copy:
`.github/workflows/release.yml`'s `publish-npm` job runs
`scripts/npm-package.sh` against the artifacts the release build matrix
already produced, and publishes the assembled directories.

```
npm/meta/                 → @vilan-lang/vilan     — the bin stubs, no binaries
npm/platform/linux-x64/   → @vilan-lang/linux-x64 — one target's two binaries
npm/platform/linux-arm64/    …
npm/platform/darwin-x64/     …
npm/platform/darwin-arm64/   …
npm/platform/win32-x64/      …
```

**Why six packages and not one postinstall downloader**: a download at install
time breaks behind firewalls, proxies and offline mirrors, and defeats
`npm ci`'s reproducibility. npm resolves the `os`/`cpu` fields itself, so a
machine downloads exactly the one platform package it can run — the
esbuild/swc shape (distribution.md §2).

**Versions are stamped by CI, not stored here.** Every `package.json` in this
tree carries `0.0.0-placeholder`, including the meta package's
`optionalDependencies` (which are locked to the exact version so a meta
package can never resolve a mismatched binary). `scripts/npm-package.sh`
replaces that string with the release tag's version, and re-checks every
manifest afterwards. This is why `scripts/bump-version.sh` says nothing about
npm: there is no version in this tree to bump.
`crates/vilan-cli/tests/npm_stub.rs` pins the placeholder, the platform table
and the `os`/`cpu` fields so the templates cannot drift apart.

**The binaries are not in git.** `npm/platform/*/bin/` exists only inside an
assembled package; the script unpacks the release archives into it and sets
the executable bit before publishing (npm's tarball preserves the mode, and
npm's own `bin` shims are the meta package's stubs, not these files).

**`preferUnplugged`** on the platform packages tells Yarn PnP to keep them
unzipped on disk — an executable inside a PnP zip cannot be spawned.

## Trying it locally

```sh
scripts/npm-package.sh 0.0.0-probe <a directory of release archives> out
npm pack --dry-run out/vilan               # what the meta package would ship
(cd out && npm pack ./vilan ./linux-x64)   # then install the two .tgz files
```

Install the **tarballs**, not the directories. A `file:` dependency (and
`npm link`) installs a *symlink*, and node resolves symlinks before it walks
`node_modules` upwards — so the stub ends up looking for its platform package
next to the packaging sources rather than next to the installed copy, and
reports the platform package as missing. Installing a `.tgz` copies the files
in, which is what the registry does and what the tests reproduce.
