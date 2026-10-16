// E257 S7's gate (debugging.md §5.3, §10's S7 row): scripted debug sessions
// over vilan-dap, against a real `vilan run --inspect-brk`, comparing the
// Variables pane with what `dbg` prints for the same values — a struct, a
// nested struct, an enum with a payload, a list, a map, `Option`/`Result`, a
// cell, a view, a closure — in the plain shape and the watch shape, plus the
// frames (a function's, a closure's and its captures, a body two generic
// instances share), the Watch pane's vilan paths, and stepping.
//
// They need a `vilan` binary: `VILAN_BIN` names it (the cargo test
// `vscode_extension`'s `e257_s7_…` sets it to the one it built). Without it
// they skip — `npm test` alone pins the adapter's pure parts
// (`src/test/dap.test.ts`).
import { test } from 'node:test';
import * as assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { DapClient } from './dapClient';

const VILAN = process.env.VILAN_BIN;
const FIXTURES = process.env.VILAN_DAP_FIXTURES ?? path.resolve(__dirname, '../../src/test/fixtures');
const skip = VILAN ? false : 'VILAN_BIN names no vilan binary (the cargo test `vscode_extension` runs these)';

/// A fresh copy of a fixture, in a directory of its own (the watch session
/// edits it).
function copyFixture(name: string): string {
    const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'vilan-dap-'));
    const file = path.join(directory, name);
    fs.copyFileSync(path.join(FIXTURES, name), file);
    return file;
}

/// Removes a session's directory; a file the stopped program still holds
/// (Windows) is left for the temp directory's own sweep.
function removeQuietly(directory: string): void {
    try {
        fs.rmSync(directory, { recursive: true, force: true });
    } catch {
        // Held open.
    }
}

/// The 1-based lines a fixture marks `// <kind>: <name>`.
function markers(file: string, kind: string): Map<string, number> {
    const lines = new Map<string, number>();
    fs.readFileSync(file, 'utf8')
        .split('\n')
        .forEach((line, index) => {
            const found = new RegExp(`// ${kind}: (\\w+)$`).exec(line);
            if (found) {
                lines.set(found[1], index + 1);
            }
        });
    return lines;
}

interface PaneVariable {
    name: string;
    value: string;
    type?: string;
    variablesReference: number;
    evaluateName?: string;
}

interface Stop {
    reason: string;
    frame: { id: number; name: string; line: number };
    scopes: Map<string, PaneVariable[]>;
}

/// What the editor shows at a stop: the top frame and every scope it would
/// open unasked.
async function stop(client: DapClient, stopped: Promise<any>): Promise<Stop> {
    const event = await stopped;
    const trace = await client.request('stackTrace', { threadId: 1 });
    const frame = trace.stackFrames[0];
    const scopes = new Map<string, PaneVariable[]>();
    for (const scope of (await client.request('scopes', { frameId: frame.id })).scopes) {
        if (!scope.expensive) {
            scopes.set(scope.name, (await client.request('variables', { variablesReference: scope.variablesReference })).variables);
        }
    }
    return { reason: event.body.reason, frame, scopes };
}

function variable(stopped: Stop, name: string, scope = 'Locals'): PaneVariable {
    const found = stopped.scopes.get(scope)?.find((candidate) => candidate.name === name);
    assert.ok(found, `\`${name}\` in ${scope}: ${JSON.stringify([...stopped.scopes])}`);
    return found;
}

/// Starts a session; answers the promise of its first stop (wrapped: an async
/// function's result would adopt it, and wait for the stop).
async function launch(
    client: DapClient,
    program: string,
    breakpoints: number[],
    extra: Record<string, unknown> = {},
): Promise<{ stopped: Promise<any> }> {
    await client.request('initialize', { adapterID: 'vilan', linesStartAt1: true, columnsStartAt1: true });
    const initialized = client.next('initialized');
    await client.request('launch', { vilan: VILAN, program, console: 'internalConsole', ...extra });
    await initialized;
    await client.request('setBreakpoints', {
        source: { path: program },
        breakpoints: breakpoints.map((line) => ({ line })),
    });
    await client.request('setExceptionBreakpoints', { filters: ['uncaught'] });
    const stopped = client.next('stopped');
    await client.request('configurationDone');
    return { stopped };
}

/// Continues; answers the promise of the next stop.
async function resume(client: DapClient): Promise<{ stopped: Promise<any> }> {
    const stopped = client.next('stopped');
    await client.request('continue', { threadId: 1 });
    return { stopped };
}

/// Waits until `check` holds of the session, or fails after `timeout`.
async function until(check: () => boolean, what: string, timeout = 60000): Promise<void> {
    const start = Date.now();
    while (!check()) {
        if (Date.now() - start > timeout) {
            throw new Error(`timed out waiting for ${what}`);
        }
        await new Promise((resolve) => setTimeout(resolve, 20));
    }
}

/// `dbg`'s lines, by the name each prints: `[values.vl:84:2] point = …` and
/// the lines a broken value continues on.
function dbgEntries(stderr: string): Map<string, string>[] {
    const rounds: Map<string, string>[] = [];
    let current = new Map<string, string>();
    let open: string | undefined;
    for (const line of stderr.split('\n')) {
        const entry = /^\[[^\]]+\] (\w+) = (.*)$/.exec(line);
        if (entry) {
            if (current.has(entry[1])) {
                rounds.push(current);
                current = new Map();
            }
            current.set(entry[1], entry[2]);
            open = entry[1];
        } else if (open !== undefined && (/^ {2}/.test(line) || /^[}\])]$/.test(line))) {
            current.set(open, current.get(open) + '\n' + line);
        } else {
            open = undefined;
        }
    }
    rounds.push(current);
    return rounds;
}

/// A value `dbg` broke over lines, put back on one — the layout's own rule
/// reversed: entries two spaces in, each with a trailing comma (a filled
/// line holding several), padded inside `{ }`.
export function unbreak(text: string): string {
    const lines = text.split('\n');
    if (lines.length === 1) {
        return text;
    }
    const open = lines[0];
    const close = lines[lines.length - 1].trim();
    const entries: string[] = [];
    const body = lines.slice(1, -1).map((line) => line.slice(2));
    for (let index = 0; index < body.length; index++) {
        const line = body[index];
        if (line.endsWith(',')) {
            entries.push(line.slice(0, -1));
            continue;
        }
        // A nested broken entry: its lines up to the close at its indent.
        const nested = [line];
        while (++index < body.length) {
            nested.push(body[index]);
            if (/^[}\])],$/.test(body[index])) {
                break;
            }
        }
        nested[nested.length - 1] = nested[nested.length - 1].slice(0, -1);
        entries.push(unbreak(nested.join('\n')));
    }
    return open.endsWith('{') ? `${open} ${entries.join(', ')} ${close}` : `${open}${entries.join(', ')}${close}`;
}

/// A stop's Locals as the pane holds them: each binding's value, and the
/// lines its children would take in `dbg`'s broken layout (`  x = 1,`) —
/// read while the program is paused, compared once `dbg` has printed.
async function snapshot(client: DapClient, stopped: Stop): Promise<Map<string, { value: string; children: string[] }>> {
    const panes = new Map<string, { value: string; children: string[] }>();
    for (const pane of stopped.scopes.get('Locals') ?? []) {
        const children: string[] = [];
        if (pane.variablesReference) {
            for (const child of (await client.request('variables', { variablesReference: pane.variablesReference })).variables) {
                children.push(`  ${child.name} = ${child.value},`);
            }
        }
        panes.set(pane.name, { value: pane.value, children });
    }
    return panes;
}

/// Every value `dbg` printed is the pane's value of the same binding — the
/// one line the pane shows is dbg's value put back on one line — and a
/// struct dbg broke over lines has the pane's children for its lines.
function assertPanesPrintAsDbg(
    panes: Map<string, { value: string; children: string[] }>,
    printed: Map<string, string>,
    what: string,
): void {
    assert.ok(printed.size > 0, `${what}: dbg printed something`);
    for (const [name, text] of printed) {
        const pane = panes.get(name);
        assert.ok(pane, `${what}: \`${name}\` is in the pane`);
        assert.equal(pane.value, unbreak(text), `${what}: \`${name}\`, the pane against dbg's\n${text}`);
        const lines = text.split('\n');
        if (lines.length > 1 && lines[0].endsWith('{')) {
            assert.deepEqual(pane.children, lines.slice(1, -1), `${what}: \`${name}\`'s children against dbg's lines`);
        }
    }
}

test('the plain shape: the panes print every value as dbg does', { skip }, async () => {
    const program = copyFixture('values.vl');
    const lines = markers(program, 'break');
    const client = new DapClient();
    try {
        let { stopped } = await launch(client, program, [...lines.values()]);

        // A view of a scalar reads as the value it views.
        const atView = await stop(client, stopped);
        assert.equal(atView.reason, 'breakpoint');
        assert.equal(atView.frame.line, lines.get('view'));
        assert.equal(atView.frame.name, 'main');
        const view = variable(atView, 'view');
        assert.deepEqual([view.value, view.type], ['1', 'view i32']);
        const viewPanes = await snapshot(client, atView);

        ({ stopped } = await resume(client));
        const atMain = await stop(client, stopped);
        assert.equal(atMain.frame.line, lines.get('main'));
        // Not a compiler temporary in sight; the names are vilan's.
        for (const pane of atMain.scopes.get('Locals') ?? []) {
            assert.ok(!pane.name.startsWith('$'), `${pane.name} is the compiler's`);
        }
        assert.equal(variable(atMain, 'point').type, 'Point');
        assert.equal(variable(atMain, 'ages').type, 'HashMap<str, i32>');
        assert.equal(variable(atMain, 'count').type, 'SignalCell<i32>');
        const mainPanes = await snapshot(client, atMain);

        // The Watch pane: vilan paths, printed by their types.
        const watch = async (expression: string, context = 'watch') =>
            (await client.request('evaluate', { expression, frameId: atMain.frame.id, context })).result;
        assert.equal(await watch('point.x'), '1');
        assert.equal(await watch('segment.to'), 'Point { x = 3, y = 4 }');
        assert.equal(await watch('numbers[1]'), '2');
        assert.equal(await watch('pair.1'), '"two"');
        assert.equal(await watch('long.finish.y'), '20');
        assert.equal(await watch('ages'), 'HashMap { "ada" => 36, "alan" => 41 }');
        // A hover lays a value out as dbg does from where it starts.
        assert.equal(
            await watch('long', 'hover'),
            'Labelled {\n  start = Point { x = 1, y = 2 },\n  finish = Point { x = 10, y = 20 },\n  label = "a label long enough to break",\n  weight = 2.5,\n}',
        );
        // Anything else is JavaScript, evaluated in the frame.
        assert.equal(await watch('numbers.length'), '3');
        // A child's path is the language's.
        const segment = variable(atMain, 'segment');
        const children = (await client.request('variables', { variablesReference: segment.variablesReference })).variables;
        assert.deepEqual(
            children.map((child: PaneVariable) => [child.name, child.evaluateName]),
            [
                ['from', 'segment.from'],
                ['to', 'segment.to'],
            ],
        );

        // A function's frame: its parameter, its local, an arm's binding.
        ({ stopped } = await resume(client));
        const atArea = await stop(client, stopped);
        assert.deepEqual([atArea.frame.name, atArea.frame.line], ['area', lines.get('area')]);
        assert.equal(variable(atArea, 'shape').value, 'Shape::Circle(1.5)');
        assert.equal(variable(atArea, 'scale').value, '2.0');
        assert.equal(variable(atArea, 'radius').value, '1.5');

        // A closure's frame, and what it captured from the function it is in.
        ({ stopped } = await resume(client));
        const atClosure = await stop(client, stopped);
        assert.deepEqual([atClosure.frame.name, atClosure.frame.line], ['a closure in scaled', lines.get('closure')]);
        assert.equal(variable(atClosure, 'value').value, '2');
        assert.equal(variable(atClosure, 'total').value, '6');
        assert.equal(variable(atClosure, 'factor', 'Captured from scaled').value, '3');

        // One body serves `first<i32>` and `first<str>`: `count` is a `usize`
        // in both, `items` is not one type, and shows as JavaScript.
        for (const length of ['2', '1']) {
            ({ stopped } = await resume(client));
            const atShared = await stop(client, stopped);
            assert.deepEqual([atShared.frame.name, atShared.frame.line], ['first', lines.get('shared')]);
            assert.deepEqual([variable(atShared, 'count').value, variable(atShared, 'count').type], [length, 'usize']);
            assert.match(variable(atShared, 'items').value, /^Array\(/);
        }

        const terminated = client.next('terminated');
        await client.request('continue', { threadId: 1 });
        await terminated;

        const [printed] = dbgEntries(client.stderr());
        assertPanesPrintAsDbg(viewPanes, new Map([['view', printed.get('view') ?? '']]), 'at the view');
        printed.delete('view');
        assertPanesPrintAsDbg(mainPanes, printed, 'at main');
        // Every shape the printer has was compared.
        for (const name of ['point', 'segment', 'shape', 'empty', 'numbers', 'ages', 'maybe', 'missing', 'outcome', 'count', 'add', 'pair', 'long']) {
            assert.ok(printed.has(name), `dbg printed \`${name}\``);
        }
        // `print` writes the printer's one line, which is the pane's value.
        assert.ok(
            client.stdout().split('\n').includes(mainPanes.get('long')?.value ?? '<none>'),
            `print(long) wrote the pane's value:\n${client.stdout()}`,
        );
    } finally {
        await client.close();
        removeQuietly(path.dirname(program));
    }
});

// `vilan run --watch` is pinned on unix (`watch_lifecycle.rs` is
// `#![cfg(unix)]`: a round's restart and its interrupt are the unix
// process group's), and so is its debug session.
const skipWatch = skip || (process.platform === 'win32' ? 'the watch shape is pinned on unix, as watch_lifecycle is' : false);

test('the watch shape: each round re-attaches, and its panes print as dbg does', { skip: skipWatch }, async () => {
    const program = copyFixture('values.vl');
    const lines = markers(program, 'break');
    const client = new DapClient();
    try {
        let { stopped } = await launch(client, program, [lines.get('main') ?? 0], { watch: true });
        const rounds: Map<string, { value: string; children: string[] }>[] = [];
        for (let round = 1; round <= 2; round++) {
            const atMain = await stop(client, stopped);
            assert.deepEqual([atMain.reason, atMain.frame.line], ['breakpoint', lines.get('main')]);
            rounds.push(await snapshot(client, atMain));
            ({ stopped } = await resume(client));
            await until(
                () => (dbgEntries(client.stderr())[round - 1]?.has('long') ?? false),
                `round ${round}'s dbg output`,
            );
            if (round === 1) {
                // A change starts the next round, which holds at its first line
                // until the adapter has attached to it again.
                fs.appendFileSync(program, '\n// touched by the watch session\n');
            }
        }
        const printed = dbgEntries(client.stderr());
        assert.equal(printed.length, 2, 'two rounds printed');
        for (const [index, panes] of rounds.entries()) {
            printed[index].delete('view');
            assertPanesPrintAsDbg(panes, printed[index], `round ${index + 1}`);
        }
    } finally {
        await client.close();
        removeQuietly(path.dirname(program));
    }
});

test('a terminal launch stops on entry and steps line by line', { skip }, async () => {
    const program = copyFixture('steps.vl');
    const lines = markers(program, 'step');
    const client = new DapClient();
    try {
        let { stopped } = await launch(client, program, [], { console: 'integratedTerminal', stopOnEntry: true });
        assert.ok(
            client.terminal.args?.some((arg) => /^--inspect-brk=127\.0\.0\.1:\d+$/.test(arg)),
            `the terminal ran \`vilan run --inspect-brk\`: ${client.terminal.args}`,
        );
        const entry = await stop(client, stopped);
        assert.deepEqual([entry.reason, entry.frame.name, entry.frame.line], ['entry', 'main', lines.get('entry')]);

        stopped = client.next('stopped');
        await client.request('next', { threadId: 1 });
        const call = await stop(client, stopped);
        assert.deepEqual([call.reason, call.frame.line], ['step', lines.get('call')]);
        assert.equal(variable(call, 'start').value, '20');

        stopped = client.next('stopped');
        await client.request('stepIn', { threadId: 1 });
        const inside = await stop(client, stopped);
        assert.deepEqual([inside.frame.name, inside.frame.line], ['double', lines.get('in')]);
        assert.equal(variable(inside, 'value').value, '20');

        stopped = client.next('stopped');
        await client.request('stepOut', { threadId: 1 });
        const out = await stop(client, stopped);
        assert.equal(out.frame.name, 'main');
        assert.ok(
            [lines.get('call'), lines.get('after')].includes(out.frame.line),
            `out of \`double\` at line ${out.frame.line}`,
        );

        const terminated = client.next('terminated');
        await client.request('continue', { threadId: 1 });
        await terminated;
        await until(() => client.terminal.output.includes('40'), "the program's output");
    } finally {
        await client.close();
        removeQuietly(path.dirname(program));
    }
});

test('a program that does not compile ends the session with the error', { skip }, async () => {
    const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'vilan-dap-'));
    const program = path.join(directory, 'broken.vl');
    fs.writeFileSync(program, 'fun main() {\n\tlet count: i32 = "three";\n}\n');
    const client = new DapClient();
    try {
        const terminated = client.next('terminated');
        await launch(client, program, [2]);
        await terminated;
        assert.match(client.stderr(), /broken\.vl/);
        const exited = client.events.find((event) => event.event === 'exited');
        assert.notEqual(exited?.body.exitCode, 0);
    } finally {
        await client.close();
        removeQuietly(directory);
    }
});
