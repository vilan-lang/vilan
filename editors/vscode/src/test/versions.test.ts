// E229: the version check's decision, run under plain node by `npm test`.
import { test } from 'node:test';
import * as assert from 'node:assert/strict';
import {
    EXTENSION_ID,
    compareVersions,
    parseServerVersion,
    parseVersion,
    sameCommit,
    versionGap,
} from '../versions';

test('one version on both halves is no gap', () => {
    assert.equal(versionGap('0.41.1', '0.41.1'), undefined);
});

test("a server NEWER than the extension names the extension's install, for the server's release", () => {
    // The owner's case: extension 0.40.0, toolchain 0.41.1.
    const gap = versionGap('0.40.0', '0.41.1');
    assert.ok(gap);
    assert.match(gap.message, /this extension is 0\.40\.0 but the language server is 0\.41\.1/);
    // The gallery's exact version: one command on every platform.
    assert.equal(gap.command, 'code --install-extension vilan-lang.vilan@0.41.1 --force');
    assert.ok(gap.message.includes(`\`${gap.command}\``), gap.message);
    assert.equal(gap.releaseUrl, 'https://github.com/vilan-lang/vilan/releases/tag/v0.41.1');
});

test("the gallery identity is the manifest's publisher.name", () => {
    const manifest = require('../../package.json') as { publisher: string; name: string };
    assert.equal(EXTENSION_ID, `${manifest.publisher}.${manifest.name}`);
});

test('a server OLDER than the extension names the toolchain upgrade', () => {
    const gap = versionGap('0.42.0', '0.41.1');
    assert.ok(gap);
    assert.match(gap.message, /Update the toolchain/);
    assert.equal(gap.command, 'vilan upgrade');
});

test('a server that answers no version predates the check and is named as the stale half', () => {
    for (const server of [undefined, '']) {
        const gap = versionGap('0.41.2', server);
        assert.ok(gap);
        assert.match(gap.message, /reports no version/);
        assert.equal(gap.command, 'vilan upgrade');
    }
});

test('versions order numerically, not as strings, and a suffix is read past', () => {
    assert.ok((compareVersions('0.10.0', '0.9.9') ?? 0) > 0);
    assert.equal(compareVersions('0.42.0-dev', '0.42.0'), 0);
    assert.deepEqual(parseVersion('v1.2.3'), [1, 2, 3]);
    assert.equal(parseVersion('not a version'), undefined);
    // Two strings that are not versions still differ, and are named.
    assert.ok(versionGap('0.41.1', 'custom'));
});

// --- E231: the commit, when both halves carry one ---------------------------

test("the server's `version (sha)` splits into its version and its commit", () => {
    assert.deepEqual(parseServerVersion('0.41.1 (07e8db372)'), { version: '0.41.1', sha: '07e8db372' });
    // A dirty tree's stamp is still that commit.
    assert.deepEqual(parseServerVersion('0.41.1 (07e8db372-dirty)'), {
        version: '0.41.1',
        sha: '07e8db372',
    });
    // A tarball build stamps `unknown`, which names no commit.
    assert.deepEqual(parseServerVersion('0.41.1 (unknown)'), { version: '0.41.1', sha: undefined });
    // A server before E231 answers the bare version.
    assert.deepEqual(parseServerVersion('0.41.1'), { version: '0.41.1', sha: undefined });
});

test('one version and one commit on both halves is no gap', () => {
    assert.equal(versionGap('0.41.1', '0.41.1 (07e8db372)', '07e8db372'), undefined);
    // Short shas of different lengths name one commit when one prefixes the other.
    assert.equal(versionGap('0.41.1', '0.41.1 (07e8db372)', '07e8db37'), undefined);
    assert.ok(sameCommit('07e8db37', '07e8db372'));
    assert.ok(!sameCommit('07e8db37', '07e8db99'));
});

test('one version but two commits names the drift a version compare cannot see', () => {
    // E231's case: a sealed-tip server and a stale dev extension of one crate version.
    const gap = versionGap('0.41.1', '0.41.1 (9ced1e9cb)', '07e8db372');
    assert.ok(gap);
    assert.match(gap.message, /built from 07e8db372/);
    assert.match(gap.message, /built from 9ced1e9cb/);
    assert.equal(gap.command, 'scripts/install-dev.sh');
});

test('a side without a commit falls back to the version compare alone', () => {
    // A gallery or release vsix packaged without a stamp, against a dev server.
    assert.equal(versionGap('0.41.1', '0.41.1 (9ced1e9cb)', undefined), undefined);
    // A server before E231, against a stamped extension.
    assert.equal(versionGap('0.41.1', '0.41.1', '07e8db372'), undefined);
    // A tarball server.
    assert.equal(versionGap('0.41.1', '0.41.1 (unknown)', '07e8db372'), undefined);
});

test('a version gap is named by the version even when both carry commits', () => {
    const gap = versionGap('0.40.0', '0.41.1 (9ced1e9cb)', '07e8db372');
    assert.ok(gap);
    assert.match(gap.message, /this extension is 0\.40\.0 but the language server is 0\.41\.1/);
    assert.equal(gap.command, 'code --install-extension vilan-lang.vilan@0.41.1 --force');
});
