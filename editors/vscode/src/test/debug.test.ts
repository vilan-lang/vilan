// E257 S6/S7: the `vilan` debug type resolves for vilan-dap (S7) — or, with
// `"adapter": "js-debug"`, to the JavaScript debugger's launch configuration
// (S6) — and the "Debug" code lens finds a program's `fun main` — run under
// plain node by `npm test`.
import { test } from 'node:test';
import * as assert from 'node:assert/strict';
import * as path from 'node:path';
import {
    INSPECT_HOST,
    LaunchContext,
    mainFunctionLines,
    mainLensConfiguration,
    resolveAdapterLaunch,
    resolveLaunch,
    vilanCandidates,
} from '../debug';

const CONTEXT: LaunchContext = {
    vilan: '/h/dev/.vilan/bin/vilan',
    port: 40123,
    stdDirectories: ['/h/dev/.vilan/std-cache/', '/opt/std'],
    activeFile: '/work/app/src/main.vl',
    workspaceFolder: '/work/app',
};

function resolved(configuration: Parameters<typeof resolveLaunch>[0], context = CONTEXT) {
    const outcome = resolveLaunch(configuration, context);
    assert.ok(outcome.ok, outcome.ok ? '' : outcome.message);
    return outcome.configuration;
}

test('a launch stays a `vilan` one, completed for vilan-dap', () => {
    const outcome = resolveAdapterLaunch(
        {
            type: 'vilan',
            request: 'launch',
            name: 'Debug it',
            program: '/work/app/src/main.vl',
            args: ['--count', '3'],
        },
        CONTEXT,
    );
    assert.ok(outcome.ok);
    assert.deepEqual(outcome.configuration, {
        type: 'vilan',
        request: 'launch',
        name: 'Debug it',
        vilan: '/h/dev/.vilan/bin/vilan',
        program: '/work/app/src/main.vl',
        args: ['--count', '3'],
        cwd: '/work/app',
        env: undefined,
        entry: undefined,
        watch: false,
        stopOnEntry: false,
        console: 'integratedTerminal',
        port: 40123,
        adapter: 'vilan',
    });
});

test('vilan-dap takes the watch shape, the entry and a configured vilanPath', () => {
    const outcome = resolveAdapterLaunch(
        { type: 'vilan', request: 'launch', program: '/work/app', watch: true, entry: 'server', vilanPath: '/opt/vilan' },
        CONTEXT,
    );
    assert.ok(outcome.ok);
    assert.equal(outcome.configuration.watch, true);
    assert.equal(outcome.configuration.entry, 'server');
    assert.equal(outcome.configuration.vilan, '/opt/vilan');
});

test('an empty configuration debugs the active file under vilan-dap', () => {
    const outcome = resolveAdapterLaunch({ type: 'vilan', request: 'launch' }, CONTEXT);
    assert.ok(outcome.ok);
    assert.equal(outcome.configuration.program, '/work/app/src/main.vl');
});

test('`"adapter": "js-debug"` resolves to the JavaScript debugger, as S6 did', () => {
    const configuration = { type: 'vilan', request: 'launch', program: '/work/app/src/main.vl', adapter: 'js-debug' };
    assert.deepEqual(resolveAdapterLaunch(configuration, CONTEXT), resolveLaunch(configuration, CONTEXT));
    const outcome = resolveAdapterLaunch(configuration, CONTEXT);
    assert.ok(outcome.ok && outcome.configuration.type === 'node');
});

test('an unknown adapter, an attach and nothing to debug are refused', () => {
    const unknown = resolveAdapterLaunch({ type: 'vilan', request: 'launch', adapter: 'gdb' }, CONTEXT);
    assert.ok(!unknown.ok && /Unknown `adapter`/.test(unknown.message));
    const attach = resolveAdapterLaunch({ type: 'vilan', request: 'attach' }, CONTEXT);
    assert.ok(!attach.ok && /`node` attach configuration/.test(attach.message));
    const nothing = resolveAdapterLaunch(
        { type: 'vilan', request: 'launch' },
        { ...CONTEXT, activeFile: undefined, workspaceFolder: undefined },
    );
    assert.ok(!nothing.ok && /open a `\.vl` file/.test(nothing.message));
});

// S6's resolution — what `"adapter": "js-debug"` asks for.
test('a plain launch runs `vilan run --inspect-brk` and attaches at its port', () => {
    const configuration = resolved({
        type: 'vilan',
        request: 'launch',
        name: 'Debug it',
        program: '/work/app/src/main.vl',
        args: ['--count', '3'],
    });
    assert.deepEqual(configuration, {
        type: 'node',
        request: 'launch',
        name: 'Debug it',
        runtimeExecutable: '/h/dev/.vilan/bin/vilan',
        runtimeArgs: [
            'run',
            `--inspect-brk=${INSPECT_HOST}:40123`,
            '/work/app/src/main.vl',
            '--count',
            '3',
        ],
        cwd: '/work/app',
        env: undefined,
        console: 'integratedTerminal',
        sourceMaps: true,
        resolveSourceMapLocations: null,
        skipFiles: ['<node_internals>/**', '/h/dev/.vilan/std-cache/**', '/opt/std/**'],
        stopOnEntry: false,
        attachSimplePort: 40123,
        continueOnAttach: true,
    });
});

test('an empty configuration debugs the active file', () => {
    const configuration = resolved({ type: 'vilan', request: 'launch' });
    assert.deepEqual(configuration.runtimeArgs, [
        'run',
        `--inspect-brk=${INSPECT_HOST}:40123`,
        '/work/app/src/main.vl',
    ]);
    assert.equal(configuration.name, 'Debug vilan program');
});

test('with no active file the workspace folder is the program', () => {
    const configuration = resolved(
        { type: 'vilan', request: 'launch' },
        { ...CONTEXT, activeFile: undefined },
    );
    assert.deepEqual((configuration.runtimeArgs as string[]).slice(-1), ['/work/app']);
});

test('nothing to debug is refused with the reason', () => {
    const outcome = resolveLaunch(
        { type: 'vilan', request: 'launch' },
        { ...CONTEXT, activeFile: undefined, workspaceFolder: undefined },
    );
    assert.equal(outcome.ok, false);
    assert.match(outcome.ok ? '' : outcome.message, /open a `\.vl` file/);
});

test('an attach request is refused toward a node attach', () => {
    const outcome = resolveLaunch({ type: 'vilan', request: 'attach' }, CONTEXT);
    assert.equal(outcome.ok, false);
    assert.match(outcome.ok ? '' : outcome.message, /`node` attach configuration/);
});

test('a watch launch re-attaches to each round through the child-process attach', () => {
    const configuration = resolved({
        type: 'vilan',
        request: 'launch',
        program: '/work/app',
        watch: true,
        entry: 'server',
    });
    assert.deepEqual(configuration.runtimeArgs, ['run', '--watch', '--entry', 'server', '/work/app']);
    assert.equal(configuration.autoAttachChildProcesses, true);
    assert.equal(configuration.attachSimplePort, undefined, 'a round restarts the process');
    assert.equal(configuration.sourceMaps, true);
});

test('stopOnEntry keeps the --inspect-brk pause', () => {
    const configuration = resolved({ type: 'vilan', request: 'launch', stopOnEntry: true });
    assert.equal(configuration.stopOnEntry, true);
    assert.equal(configuration.continueOnAttach, false);
});

test('a configured vilanPath, cwd, env and console win', () => {
    const configuration = resolved({
        type: 'vilan',
        request: 'launch',
        vilanPath: '/opt/vilan',
        cwd: '/elsewhere',
        env: { MODE: 'debug' },
        console: 'internalConsole',
    });
    assert.equal(configuration.runtimeExecutable, '/opt/vilan');
    assert.equal(configuration.cwd, '/elsewhere');
    assert.deepEqual(configuration.env, { MODE: 'debug' });
    assert.equal(configuration.console, 'internalConsole');
});

test('the CLI is looked for beside a configured server, then where the server is', () => {
    const candidates = vilanCandidates({
        serverPath: '/opt/toolchain/bin/vilan-lsp',
        repoRoot: '/repo',
        home: '/h/dev',
        suffix: '',
        join: path.posix.join,
        dirname: path.posix.dirname,
    });
    assert.deepEqual(candidates, [
        '/opt/toolchain/bin/vilan',
        '/repo/target/release/vilan',
        '/repo/target/debug/vilan',
        '/h/dev/.cargo/bin/vilan',
        '/h/dev/.vilan/bin/vilan',
    ]);
    // The default server setting names no directory.
    const fallback = vilanCandidates({
        serverPath: 'vilan-lsp',
        repoRoot: '/repo',
        home: '/h/dev',
        suffix: '',
        join: path.posix.join,
        dirname: path.posix.dirname,
    });
    assert.equal(fallback[0], '/repo/target/release/vilan');
});

test('the lens goes on a top-level `fun main`, async or not, and nowhere else', () => {
    const text = [
        'import std::io::print;',
        '',
        'fun helper() {}',
        '',
        'fun main() {',
        '\tprint("hi");',
        '}',
        '',
        'impl Thing {',
        '\tfun main(self) {}',
        '}',
        'fun mainly() {}',
        'async fun main ( ) {}',
        '// fun main() {}',
    ].join('\r\n');
    assert.deepEqual(mainFunctionLines(text), [4, 12]);
});

test('the lens debugs the file it sits in', () => {
    assert.deepEqual(mainLensConfiguration('/work/app/src/main.vl'), {
        type: 'vilan',
        request: 'launch',
        name: 'Debug main',
        program: '/work/app/src/main.vl',
    });
});

// The manifest's half: the contributed debugger is the type the provider
// registers, and every configuration it offers resolves.
test('package.json contributes the vilan debugger and vilan breakpoints', () => {
    // `require`, so the bundler carries the manifest into the test bundle:
    // the suite's runner bundles these tests away from the source tree.
    const manifest = require('../../package.json');
    const contributes = manifest.contributes;
    assert.deepEqual(contributes.breakpoints, [{ language: 'vilan' }]);
    const [debuggerEntry] = contributes.debuggers;
    assert.equal(debuggerEntry.type, 'vilan');
    assert.deepEqual(debuggerEntry.languages, ['vilan']);
    assert.ok(manifest.activationEvents.includes('onDebugResolve:vilan'));
    const properties = Object.keys(debuggerEntry.configurationAttributes.launch.properties).sort();
    assert.deepEqual(properties, [
        'adapter',
        'args',
        'console',
        'cwd',
        'entry',
        'env',
        'program',
        'stopOnEntry',
        'vilanPath',
        'watch',
    ]);
    assert.deepEqual(debuggerEntry.configurationAttributes.launch.properties.adapter.enum, ['vilan', 'js-debug']);
    for (const initial of debuggerEntry.initialConfigurations) {
        assert.equal(resolveAdapterLaunch({ ...initial, program: '/work/app/src/main.vl' }, CONTEXT).ok, true);
    }
    for (const snippet of debuggerEntry.configurationSnippets) {
        assert.equal(resolveAdapterLaunch({ ...snippet.body, program: '/work/app' }, CONTEXT).ok, true);
    }
});
