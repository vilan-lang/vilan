// E247 (R-a) and E248: the status bar item's text and the menu it opens, run
// under plain node by `npm test`.
import { test } from 'node:test';
import * as assert from 'node:assert/strict';
import {
    ACTIONS,
    FileStatus,
    MenuRow,
    TOGGLES,
    buildMenu,
    statusText,
    statusTooltip,
    toggleTarget,
} from '../menu';

const manifest = require('../../package.json') as {
    contributes: {
        commands: { command: string }[];
        configuration: { properties: Record<string, { type: string }> };
    };
};

const FILE: FileStatus = {
    platform: 'browser',
    kind: 'declared',
    reason: 'the `client` entry reaches it',
    world: '/home/user/kolt/src/client.vl',
    work: { files: 82, entities: 104233, impls: 1204 },
};

function toggles(value: boolean) {
    return TOGGLES.map((toggle) => ({ ...toggle, value }));
}

function labels(rows: MenuRow[]): string[] {
    return rows.map((row) => row.label);
}

test('the item reads the server version, not the platform', () => {
    assert.equal(statusText(true, '0.43.0 (fe092e8d1)'), 'vilan 0.43.0');
    assert.equal(statusText(true, '0.43.0'), 'vilan 0.43.0');
    assert.equal(statusText(true, undefined), 'vilan');
});

test('E248: a stopped server says so on the item', () => {
    assert.equal(statusText(false, '0.43.0 (fe092e8d1)'), 'vilan (stopped)');
    assert.match(statusTooltip(false, FILE), /stopped/);
});

test("the platform and its reason move to the tooltip's first line", () => {
    const tooltip = statusTooltip(true, FILE);
    assert.ok(
        tooltip.startsWith('This file is analyzed under browser: the `client` entry reaches it'),
        tooltip,
    );
});

test('the read-only rows: platform and why, the entry world, the work COUNTS, the server', () => {
    const rows = buildMenu({ running: true, serverVersion: '0.43.0 (fe092e8d1)', file: FILE, toggles: toggles(true) });
    const info = rows.filter((row) => row.kind === 'info').map((row) => row.label);
    assert.deepEqual(info, [
        'analyzed as: browser — declared',
        'analyzed in the world of client.vl',
        'last analysis: 82 files · 104,233 entities · 1,204 impls',
        'server 0.43.0 (fe092e8d1)',
    ]);
    // Counts, never milliseconds (the M106 ruling).
    assert.ok(!info.some((label) => / ms\b|millisecond/.test(label)), info.join('\n'));
});

test('an entry is its own world; an older server that sends no world or work leaves the rows out', () => {
    const own = buildMenu({
        running: true,
        serverVersion: '0.43.0',
        file: { ...FILE, world: null },
        toggles: toggles(true),
    });
    assert.ok(labels(own).includes('analyzed as its own entry'));
    const older = buildMenu({
        running: true,
        serverVersion: '0.43.0',
        file: { platform: 'node', kind: null, reason: null },
        toggles: toggles(true),
    });
    assert.deepEqual(
        older.filter((row) => row.kind === 'info').map((row) => row.label),
        ['analyzed as: node', 'server 0.43.0'],
    );
});

test('one toggle row per feature switch, inlay hints first, each carrying its value', () => {
    const rows = buildMenu({ running: true, serverVersion: '0.43.0', file: FILE, toggles: toggles(false) });
    const switches = rows.filter((row) => row.kind === 'toggle');
    assert.equal(switches[0].kind === 'toggle' && switches[0].setting, 'inlayHints.enabled');
    assert.deepEqual(
        switches.map((row) => row.kind === 'toggle' && row.setting),
        TOGGLES.map((toggle) => toggle.key),
    );
    assert.ok(switches.every((row) => row.kind === 'toggle' && row.value === false));
});

test('every toggle names a boolean setting package.json declares', () => {
    const properties = manifest.contributes.configuration.properties;
    for (const toggle of TOGGLES) {
        const declared = properties[`vilan.${toggle.key}`];
        assert.ok(declared, `vilan.${toggle.key} is declared`);
        assert.equal(declared.type, 'boolean', `vilan.${toggle.key} is a switch`);
    }
});

test('E248: a running server offers Restart and Stop; a stopped one offers Start', () => {
    const running = buildMenu({ running: true, serverVersion: '0.43.0', file: FILE, toggles: toggles(true) });
    const runningCommands = running.flatMap((row) => (row.kind === 'action' ? [row.command] : []));
    assert.deepEqual(runningCommands, [ACTIONS.restart, ACTIONS.stop, ACTIONS.status, ACTIONS.output]);
    const stopped = buildMenu({ running: false, serverVersion: '0.43.0', file: FILE, toggles: toggles(true) });
    const stoppedCommands = stopped.flatMap((row) => (row.kind === 'action' ? [row.command] : []));
    assert.deepEqual(stoppedCommands, [ACTIONS.start, ACTIONS.status, ACTIONS.output]);
    assert.ok(labels(stopped).includes('The language server is stopped'));
});

test('every action and the menu itself are commands package.json declares', () => {
    const declared = manifest.contributes.commands.map((command) => command.command);
    for (const command of [...Object.values(ACTIONS), 'vilan.showMenu']) {
        assert.ok(declared.includes(command), `${command} is declared: ${declared.join(', ')}`);
    }
});

test('a toggle writes the workspace setting when one is set there, else the user setting', () => {
    assert.equal(toggleTarget({}), 'global');
    assert.equal(toggleTarget({ workspaceValue: false }), 'workspace');
    assert.equal(toggleTarget({ workspaceFolderValue: true }), 'workspace');
});
