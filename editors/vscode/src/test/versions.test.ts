// E229: the version check's decision, run under plain node by `npm test`.
import { test } from 'node:test';
import * as assert from 'node:assert/strict';
import { EXTENSION_ID, compareVersions, parseVersion, versionGap } from '../versions';

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
