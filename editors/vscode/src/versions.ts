// E229: the extension and its language server are ONE version.
//
// The CLI, the server, the embedded std and this extension share one version
// (releases.md §4; `scripts/bump-version.sh` sets it everywhere), and a gap
// between the two halves is silent: an extension a release behind its server
// lacks every client half the server's release notes promise. The owner ran
// 0.40.0 against a 0.41.1 toolchain for a full order — no
// `vilan.autoClosing.generics`, E214's doubled `>` live after its fix — and
// nothing noticed. So the extension reads the server's `serverInfo.version`
// (the `initialize` result, `vilan-lsp`'s `SERVER_VERSION`) at start and names
// a gap ONCE, with the command that closes it.
//
// No `vscode` import: this file is the decision, and `npm test` runs it under
// plain node (`src/test/versions.test.ts`).

/// Where the releases live — each carries `vilan-vscode.vsix`.
export const RELEASES_URL = 'https://github.com/vilan-lang/vilan/releases';

/// A gap between the two halves, as the notification states it.
export interface VersionGap {
    /// The sentence the one notification shows.
    message: string;
    /// The command that closes the gap, for the "Copy Command" button.
    command: string;
    /// The page to open for the "Open Release" button.
    releaseUrl: string;
}

/// `major.minor.patch` as numbers, or `undefined` for anything else. A
/// pre-release or build suffix (`0.42.0-dev`) is read up to its first `-`/`+`.
export function parseVersion(version: string): [number, number, number] | undefined {
    const match = /^v?(\d+)\.(\d+)\.(\d+)(?:[-+].*)?$/.exec(version.trim());
    if (!match) {
        return undefined;
    }
    return [Number(match[1]), Number(match[2]), Number(match[3])];
}

/// Negative, zero or positive as `left` is older than, equal to, or newer than
/// `right`; `undefined` when either is not a version.
export function compareVersions(left: string, right: string): number | undefined {
    const a = parseVersion(left);
    const b = parseVersion(right);
    if (!a || !b) {
        return undefined;
    }
    for (let index = 0; index < 3; index++) {
        if (a[index] !== b[index]) {
            return a[index] - b[index];
        }
    }
    return 0;
}

/// E231: the file the packaging writes the extension's commit into, beside
/// `package.json` (`scripts/install-dev.sh`, release.yml's `vsix` job).
export const BUILD_SHA_FILE = 'build-sha.txt';

/// The extension's gallery identity — `publisher.name` in `package.json`.
/// Every release publishes it to the VS Code Marketplace and Open VSX
/// (release.yml's `publish-marketplace` / `publish-openvsx`), each version
/// kept, so `@<version>` names exactly the one that ships with a toolchain.
export const EXTENSION_ID = 'vilan-lang.vilan';

/// The one-liner that installs this extension at `version` over whatever is
/// installed: the same command on every platform and in every VS Code build
/// with a gallery (Open VSX answers for VSCodium and Cursor).
export function extensionInstallCommand(version: string): string {
    return `code --install-extension ${EXTENSION_ID}@${version} --force`;
}

/// E231: the server's `serverInfo.version` read apart. Since E231 the server
/// answers `<version> (<sha>)` — `vilan --version`'s shape, from one build
/// stamp — and before it the bare version. The commit is `undefined` for the
/// bare form and for a tarball build's `unknown`; a dirty tree's `-dirty`
/// suffix is read past, since the build is still that commit's.
export function parseServerVersion(server: string): { version: string; sha: string | undefined } {
    const match = /^(.*?)\s*\(([^()]*)\)\s*$/.exec(server.trim());
    if (!match) {
        return { version: server.trim(), sha: undefined };
    }
    return { version: match[1], sha: normalizeSha(match[2]) };
}

/// A build stamp as a commit: `undefined` for `unknown` or an empty stamp, and
/// the `-dirty` suffix dropped.
export function normalizeSha(stamp: string | undefined): string | undefined {
    const sha = stamp?.trim().replace(/-dirty$/, '');
    return sha && /^[0-9a-f]{4,40}$/i.test(sha) ? sha.toLowerCase() : undefined;
}

/// Whether two short shas name one commit: one is a prefix of the other (git
/// abbreviates to different lengths in different places).
export function sameCommit(left: string, right: string): boolean {
    return left.startsWith(right) || right.startsWith(left);
}

/// The gap between this extension (`extension`, and `extensionSha`, the commit
/// its packaging embedded — `undefined` for a vsix packaged without one) and
/// the server it started (`server`, the `serverInfo.version` it answered,
/// `undefined` when it answered none), or `undefined` when there is none to
/// name.
///
/// E231: when BOTH halves carry a commit and the versions agree, the commits
/// are compared too — a dev build between releases keeps the last release's
/// crate version, so a sealed-tip server and a stale extension compared equal.
/// Two commits of one version name the dev refresh, which installs both halves
/// from one checkout; which half is the stale one a sha cannot say. A half
/// without a commit is compared by version alone, as before.
///
/// Three shapes. The server is NEWER: the editor is the stale half, and the
/// command installs the server's release's extension (the installer does this
/// itself since E229, so the case is an editor that predates it or an
/// install with `--no-vscode`). The server is OLDER, or answers no version
/// (every server before E229): the toolchain is the stale half, and
/// `vilan upgrade` closes it. Two strings that are not versions (a hand build
/// with an odd manifest) are named plainly, with the upgrade.
export function versionGap(
    extension: string,
    server: string | undefined,
    extensionSha?: string,
): VersionGap | undefined {
    if (server !== undefined && server !== '') {
        const parsed = parseServerVersion(server);
        const ownSha = normalizeSha(extensionSha);
        if (parsed.version === extension) {
            if (parsed.sha === undefined || ownSha === undefined || sameCommit(parsed.sha, ownSha)) {
                return undefined;
            }
            return {
                message:
                    `Vilan: this extension (${extension}, built from ${ownSha}) and the language ` +
                    `server (${extension}, built from ${parsed.sha}) come from different commits — ` +
                    'one of them is stale. Re-run `scripts/install-dev.sh` from the checkout you ' +
                    'build the toolchain from (it installs both halves), then reload the window.',
                command: 'scripts/install-dev.sh',
                releaseUrl: `${RELEASES_URL}/tag/v${extension}`,
            };
        }
        server = parsed.version;
    }
    if (server === extension) {
        return undefined;
    }
    if (server === undefined || server === '') {
        return {
            message:
                `Vilan: the language server reports no version, so it predates this extension ` +
                `(${extension}) — it is missing what the extension relies on. ` +
                'Update the toolchain: `vilan upgrade` (or re-run the installer).',
            command: 'vilan upgrade',
            releaseUrl: `${RELEASES_URL}/tag/v${extension}`,
        };
    }
    const order = compareVersions(server, extension);
    if (order !== undefined && order > 0) {
        const command = extensionInstallCommand(server);
        return {
            message:
                `Vilan: this extension is ${extension} but the language server is ${server} — ` +
                `the editor is missing what ${server} added. Install the matching extension: ` +
                `\`${command}\` (from a checkout, \`scripts/install-dev.sh\` builds and installs ` +
                'it), then reload the window.',
            command,
            releaseUrl: `${RELEASES_URL}/tag/v${server}`,
        };
    }
    return {
        message:
            `Vilan: this extension is ${extension} but the language server is ${server}. ` +
            'Update the toolchain to match: `vilan upgrade` (or re-run the installer, which ' +
            'installs both).',
        command: 'vilan upgrade',
        releaseUrl: `${RELEASES_URL}/tag/v${extension}`,
    };
}
