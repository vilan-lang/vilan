// E257 S7: vilan-dap's pure parts under plain node — the source map read both
// ways, the printer's layout, the walker that reads a value by its layout,
// the sidecar's frames and paths, and the launch. The live sessions are
// `session.test.ts`.
import { test } from 'node:test';
import * as assert from 'node:assert/strict';
import * as path from 'node:path';
import { pathToFileURL } from 'node:url';
import { describe, runCommand, shownStderr } from '../dap/adapter';
import { Doc, WALKER, children, dbgLine, flat, layout, walker } from '../dap/printer';
import { Sidecar, parsePath } from '../dap/sidecar';
import { SourceMap, decodeMappings, decodeVlq } from '../dap/sourceMap';

// --- The source map ---------------------------------------------------------

test('VLQ fields decode, signs and continuation digits included', () => {
    assert.deepEqual(decodeVlq('AAAA'), [0, 0, 0, 0]);
    assert.deepEqual(decodeVlq('CACC'), [1, 0, 1, 1]);
    assert.deepEqual(decodeVlq('D'), [-1]);
    // 16 needs a continuation digit: 16 << 1 = 32 = `gB`.
    assert.deepEqual(decodeVlq('gB'), [16]);
    assert.deepEqual(decodeVlq('hB'), [-16]);
});

test('mappings decode to absolute segments per generated line', () => {
    const lines = decodeMappings('AAAA,EAAE;;ACCA');
    assert.deepEqual(lines[0], [
        { column: 0, source: 0, line: 0, originalColumn: 0 },
        { column: 2, source: 0, line: 0, originalColumn: 2 },
    ]);
    assert.deepEqual(lines[1], []);
    assert.deepEqual(lines[2], [{ column: 0, source: 1, line: 1, originalColumn: 2 }]);
});

/// The example map's places, spelled as this platform spells an absolute
/// path (`D:\work\app\dist` on Windows): a map's sources resolve to the
/// paths the editor's breakpoints name.
const DIST = path.resolve('/work/app/dist');
const STD = path.resolve('/opt/std/list.vl');

/// A map over two sources — the user's, and an ignore-listed std — whose
/// line 0 is a std helper, line 1 a statement and a call on the user's line
/// 3, and lines 2 and 3 two emitted copies of the user's line 5.
function exampleMap(): SourceMap {
    const segments = [
        [[0, 1, 0, 0]],
        [
            [1, 0, 3, 4],
            [9, 0, 3, 12],
        ],
        [[1, 0, 5, 4]],
        [[1, 0, 5, 4]],
    ];
    return new SourceMap(
        {
            version: 3,
            sources: ['main.vl', pathToFileURL(STD).href],
            names: [],
            mappings: encode(segments),
            ignoreList: [1],
        },
        path.join(DIST, 'main.mjs.map'),
    );
}

test('a generated position maps to the last segment at or before it', () => {
    const map = exampleMap();
    assert.equal(map.sources[0].path, path.join(DIST, 'main.vl'));
    assert.equal(map.sources[1].path, STD);
    assert.deepEqual(map.original({ line: 1, column: 5 }), { source: 0, line: 3, column: 4, name: undefined });
    assert.deepEqual(map.original({ line: 1, column: 20 }), { source: 0, line: 3, column: 12, name: undefined });
    assert.equal(map.original({ line: 1, column: 0 }), undefined, 'before the line\'s first segment');
    assert.equal(map.original({ line: 9, column: 0 }), undefined);
});

test('a breakpoint binds at the line\'s statement start in every emitted copy, sliding to the next mapped line', () => {
    const map = exampleMap();
    const source = map.sourceIndex(path.join(DIST, 'main.vl'));
    assert.equal(source, 0);
    assert.deepEqual(map.breakpointPositions(0, 3), { line: 3, positions: [{ line: 1, column: 1 }] });
    assert.deepEqual(map.breakpointPositions(0, 5), {
        line: 5,
        positions: [
            { line: 2, column: 1 },
            { line: 3, column: 1 },
        ],
    });
    assert.deepEqual(map.breakpointPositions(0, 4)?.line, 5, 'an unmapped line slides on');
    assert.equal(map.breakpointPositions(0, 40), undefined);
});

test('the ignore-listed sources are blackboxed ranges', () => {
    assert.deepEqual(exampleMap().ignoredRanges(), [
        [
            { line: 0, column: 0 },
            { line: 1, column: 1 },
        ],
    ]);
});

// --- The printer ------------------------------------------------------------

const point = (x: number, y: number): Doc => ({
    o: 'Point {',
    c: '}',
    p: true,
    e: [
        ['x = ', String(x)],
        ['y = ', String(y)],
    ],
    f: false,
    k: 's',
});

test('flat is print\'s one line: padded braces, bare brackets', () => {
    assert.equal(flat(point(1, 2)), 'Point { x = 1, y = 2 }');
    assert.equal(flat({ o: '[', c: ']', p: false, e: [['', '1'], ['', '2']], f: true, k: 'l' }), '[1, 2]');
    assert.equal(flat({ o: 'HashSet {', c: '}', p: true, e: [], f: false, k: 'z' }), 'HashSet {}');
});

test('layout breaks past 80 columns as dbg does, filling a scalar list', () => {
    const segment: Doc = {
        o: 'Segment {',
        c: '}',
        p: true,
        e: [
            ['from = ', point(1, 2)],
            ['to = ', point(3, 4)],
        ],
        f: false,
        k: 's',
    };
    // dbg's own output for this value (crates/vilan-core's debugging pins).
    assert.equal(
        dbgLine('values.vl:85:2', 'segment', segment),
        '[values.vl:85:2] segment = Segment {\n  from = Point { x = 1, y = 2 },\n  to = Point { x = 3, y = 4 },\n}',
    );
    assert.equal(layout(segment, 0, 0), flat(segment), 'it fits from column 0');
    const numbers: Doc = {
        o: '[',
        c: ']',
        p: false,
        e: Array.from({ length: 30 }, (_, index): [string, Doc] => ['', String(1000 + index)]),
        f: true,
        k: 'l',
    };
    const laid = layout(numbers, 0, 0).split('\n');
    assert.equal(laid[0], '[');
    assert.ok(laid.slice(1, -1).every((line) => line.length <= 80 && line.startsWith('  ')), laid.join('\n'));
    assert.ok(laid[1].split(', ').length > 1, 'a filled line holds several');
    assert.equal(laid[laid.length - 1], ']');
});

test('a group\'s children are named as the language names them', () => {
    assert.deepEqual(
        children(point(1, 2)).map((child) => [child.name, child.path]),
        [
            ['x', '.x'],
            ['y', '.y'],
        ],
    );
    const list: Doc = { o: '[', c: ']', p: false, e: [['', '1'], ['', '… 9 more']], f: true, k: 'l' };
    assert.deepEqual(
        children(list).map((child) => [child.name, child.path]),
        [
            ['0', '[0]'],
            ['…', undefined],
        ],
    );
    const map: Doc = { o: 'HashMap {', c: '}', p: true, e: [['"ada" => ', '36']], f: false, k: 'm' };
    assert.deepEqual(children(map).map((child) => [child.name, child.document]), [['"ada"', '36']]);
});

// --- The walker -------------------------------------------------------------

/// The layout table the sidecar would carry for these values.
const TYPES = [
    { text: 'i32', shape: 'integer' },
    { text: 'Point', shape: 'struct', name: 'Point', fields: [['x', 0], ['y', 0]] },
    { text: 'Option<i32>', shape: 'enum', variants: [['Some', [0]], ['None', []]] },
    { text: 'List<i32>', shape: 'list', element: 0, fill: true },
    { text: 'str', shape: 'str' },
    { text: 'HashMap<str, i32>', shape: 'map', label: 'HashMap', field: 0, key: 4, value: 0, keyWidth: 1, valueWidth: 1 },
    { text: 'SignalCell<i32>', shape: 'cell', label: 'SignalCell', field: 0, value: 0 },
    { text: '(i32, (i32, str))', shape: 'tuple', elements: [['', 0, 1], ['', 8, 2]] },
    { text: '(i32, str)', shape: 'tuple', elements: [['', 0, 1], ['', 4, 1]] },
    { text: 'f64', shape: 'float' },
    { text: 'Shared<List<i32>>', shape: 'shared', value: 3 },
    { text: 'Color', shape: 'backed', variants: [['Color::Red', 'r'], ['Color::Blue', 'b']] },
    { text: '|i32| i32', shape: 'text', value: '<closure |i32| i32>' },
    { text: 'Empty', shape: 'struct', name: 'Empty', fields: [] },
    { text: 'Shape', shape: 'enum', variants: [['Shape::Circle', [9]], ['Shape::Rect', [1, 1]], ['Shape::Empty', []]] },
    { text: 'Node', shape: 'struct', name: 'Node', fields: [['next', 16]] },
    { text: 'Shared<Node>', shape: 'shared', value: 15 },
];

const read = (value: unknown, type: number, how = 'value') => flat(walker(value, TYPES, type, how));

test('the walker reads each JavaScript representation as its type prints', () => {
    assert.equal(read([1, 2], 1), 'Point { x = 1, y = 2 }');
    assert.equal(read([0, 5], 2), 'Some(5)');
    assert.equal(read([1], 2), 'None');
    assert.equal(read([1, 2, 3], 3), '[1, 2, 3]');
    assert.equal(read('a "quoted"\n', 4), '"a \\"quoted\\"\\n"');
    const table = new Map<unknown, unknown>([
        [101, ['ada', 36]],
        [102, ['alan', 41]],
    ]);
    assert.equal(read([table], 5), 'HashMap { "ada" => 36, "alan" => 41 }');
    assert.equal(read([{ v: 3 }, { v: [] }], 6), 'SignalCell(3)');
    // A tuple stores flat: `(1, (2, "two"))` is `[1, 2, "two"]`.
    assert.equal(read([1, 2, 'two'], 7), '(1, (2, "two"))');
    assert.equal(read(3, 9), '3.0');
    assert.equal(read(-0, 9), '-0.0');
    assert.equal(read(1.5, 9), '1.5');
    assert.equal(read('b', 11), 'Color::Blue');
    assert.equal(read(() => 1, 12), '<closure |i32| i32>');
    assert.equal(read([], 13), 'Empty');
    assert.equal(read([1, [0, 0], [5, 6]], 14), 'Shape::Rect(Point { x = 0, y = 0 }, Point { x = 5, y = 6 })');
    assert.equal(read([2], 14), 'Shape::Empty');
    // An out-of-range tag reads as the last variant, as the generated printer's
    // unconditional last leg does.
    assert.equal(read([7], 2), 'None');
});

test('a view, a boxed local and a lazy parameter read what dbg would print', () => {
    const rows = [
        [0, 1],
        [0, 2],
    ];
    assert.equal(read([rows[0], 1], 0, 'pair'), '1', 'a scalar view is its (base, key) pair');
    assert.equal(read([[1, 2]], 1, 'boxed'), 'Point { x = 1, y = 2 }');
    assert.equal(read(() => 'never', 4, 'lazy'), '<lazy, not forced>');
});

test('a list past a hundred entries says how many more, and a Shared cycle stops', () => {
    const long = Array.from({ length: 103 }, (_, index) => index);
    const document = walker(long, TYPES, 3, 'value');
    assert.ok(typeof document !== 'string');
    assert.equal(document.e.length, 101);
    assert.equal(document.e[100][1], '… 3 more');
    assert.equal(read({ v: [] }, 10), 'Shared([])');
    const shared: { v: unknown } = { v: undefined };
    shared.v = [shared];
    assert.equal(read(shared, 16), 'Shared(Node { next = <cycle> })');
});

test('the walker that is sent is the walker that is tested', () => {
    const sent = new Function(`return (${WALKER});`)() as typeof walker;
    for (const [value, type] of [
        [[1, 2], 1],
        [[0, 5], 2],
        [[1, 2, 'two'], 7],
    ] as [unknown, number][]) {
        assert.deepEqual(sent(value, TYPES, type, 'value'), walker(value, TYPES, type, 'value'));
    }
});

// --- The sidecar --------------------------------------------------------------

const SIDECAR = {
    version: 1,
    file: 'main.mjs',
    frames: [
        { kind: 'main', name: 'main', bindings: [{ name: 'point', js: 'point', type: 1 }] },
        {
            kind: 'function',
            name: 'doubled',
            js: 'doubled',
            at: [0, 9, 4],
            bindings: [{ name: 'shape', js: 'shape', type: 1, parameter: true }],
        },
        {
            kind: 'function',
            name: 'doubled',
            js: 'doubled2',
            at: [0, 9, 4],
            bindings: [{ name: 'shape', js: 'shape', type: 3, parameter: true }],
        },
        { kind: 'closure', name: 'a closure in main', at: [0, 20, 10], bindings: [{ name: 'item', js: 'item', type: 0 }] },
        { kind: 'closure', name: 'a closure in main', at: [0, 20, 10], bindings: [{ name: 'item', js: 'item', type: 4 }] },
    ],
    types: TYPES,
};

test('a frame is found by its head and its name; copies that disagree lose the type', () => {
    const sidecar = new Sidecar(SIDECAR);
    assert.equal(sidecar.resolve('', undefined, true)?.name, 'main');
    const head = { source: 0, line: 9, column: 4 };
    assert.equal(sidecar.resolve('doubled2', head, false)?.bindings[0].type, 3);
    assert.equal(sidecar.resolve('doubled', head, false)?.bindings[0].type, 1);
    // By name alone when the head does not map.
    assert.equal(sidecar.resolve('doubled2', undefined, false)?.bindings[0].type, 3);
    // Two copies of one closure, two types: no type.
    const closure = sidecar.resolve('', { source: 0, line: 20, column: 10 }, false);
    assert.equal(closure?.name, 'a closure in main');
    assert.equal(closure?.bindings[0].type, undefined);
    assert.equal(sidecar.resolve('unknown', undefined, false), undefined);
    assert.throws(() => new Sidecar({ version: 2 }), /version/);
});

test('an awaiting frame is found by its name, or by the innermost text holding its place', () => {
    const sidecar = new Sidecar({
        version: 1,
        file: 'main.mjs',
        frames: [
            { kind: 'main', name: 'main', span: [0, 12, 0, 30, 1], bindings: [] },
            { kind: 'function', name: 'outer', js: 'outer', at: [0, 2, 4], span: [0, 2, 0, 10, 1], bindings: [] },
            { kind: 'closure', name: 'a closure in outer', at: [0, 4, 12], span: [0, 4, 12, 6, 2], bindings: [] },
            { kind: 'closure', name: 'a closure in main', at: [0, 14, 8], span: [0, 14, 8, 14, 20], bindings: [] },
        ],
        types: [],
    });
    // A named function by its emitted name, wherever it stands.
    assert.equal(sidecar.resolveAt('outer', { source: 0, line: 3, column: 0 })?.name, 'outer');
    // An anonymous frame by the innermost text holding its place.
    assert.equal(sidecar.resolveAt('', { source: 0, line: 5, column: 0 })?.name, 'a closure in outer');
    assert.equal(sidecar.resolveAt('', { source: 0, line: 8, column: 3 })?.name, 'outer');
    assert.equal(sidecar.resolveAt('', { source: 0, line: 14, column: 9 })?.name, 'a closure in main');
    assert.equal(sidecar.resolveAt('', { source: 0, line: 20, column: 1 })?.name, 'main');
    // The end is exclusive; another source, or no place, finds nothing.
    assert.equal(sidecar.resolveAt('', { source: 0, line: 14, column: 20 })?.name, 'main');
    assert.equal(sidecar.resolveAt('', { source: 1, line: 5, column: 0 }), undefined);
    assert.equal(sidecar.resolveAt('', undefined), undefined);
});

test('the walker is sent only the types its root reaches, renumbered', () => {
    const table = new Sidecar(SIDECAR).table(14);
    assert.deepEqual(
        table.types.map((type) => type.text),
        ['Shape', 'f64', 'Point', 'i32'],
    );
    assert.equal(flat(walker([1, [0, 0], [5, 6]], table.types, table.root, 'value')), 'Shape::Rect(Point { x = 0, y = 0 }, Point { x = 5, y = 6 })');
});

test('a Watch path is parsed and walked through the layouts', () => {
    assert.deepEqual(parsePath('segment.to.x'), {
        root: 'segment',
        segments: [
            { kind: 'field', name: 'to' },
            { kind: 'field', name: 'x' },
        ],
    });
    assert.deepEqual(parsePath(' rows[2] '), { root: 'rows', segments: [{ kind: 'index', index: 2 }] });
    assert.equal(parsePath('a + b'), undefined);
    assert.equal(parsePath('f(x)'), undefined);
    const sidecar = new Sidecar(SIDECAR);
    assert.deepEqual(sidecar.step(1, { kind: 'field', name: 'y' }), { access: '[1]', type: 0 });
    assert.equal(sidecar.step(1, { kind: 'field', name: 'z' }), undefined);
    assert.deepEqual(sidecar.step(3, { kind: 'index', index: 4 }), { access: '[4]', type: 0 });
    // A tuple's second element spans two flat slots.
    assert.deepEqual(sidecar.step(7, { kind: 'field', name: '1' }), { access: '.slice(1, 3)', type: 8 });
    assert.equal(sidecar.typeText({ name: 'first', js: 'first', type: 0, view: true }), 'view i32');
});

// --- The launch -------------------------------------------------------------

test('the adapter runs `vilan run --inspect-brk`, under --watch for the watch shape', () => {
    assert.deepEqual(runCommand({ vilan: 'vilan', program: '/w/main.vl', args: ['-n', '2'] }, 4100), [
        'run',
        '--inspect-brk=127.0.0.1:4100',
        '/w/main.vl',
        '-n',
        '2',
    ]);
    assert.deepEqual(runCommand({ vilan: 'vilan', program: '/w', watch: true, entry: 'server' }, 4100), [
        'run',
        '--watch',
        '--inspect-brk=127.0.0.1:4100',
        '--entry',
        'server',
        '/w',
    ]);
});

test('a JavaScript value prints as the JavaScript debugger would', () => {
    assert.equal(describe({ type: 'string', value: 'a' }), '"a"');
    assert.equal(describe({ type: 'number', value: 3 }), '3');
    assert.equal(describe({ type: 'number', unserializableValue: '-0' }), '-0');
    assert.equal(describe({ type: 'object', subtype: 'array', description: 'Array(2)' }), 'Array(2)');
    assert.equal(describe({ type: 'object', subtype: 'null', value: null }), 'null');
    assert.equal(describe(undefined), 'undefined');
});

/// `[[column, source, line, column], ..]` per line, encoded as `mappings`.
function encode(lines: number[][][]): string {
    let source = 0;
    let line = 0;
    let column = 0;
    return lines
        .map((segments) => {
            let generated = 0;
            return segments
                .map((segment) => {
                    const fields = [segment[0] - generated, segment[1] - source, segment[2] - line, segment[3] - column];
                    generated = segment[0];
                    source = segment[1];
                    line = segment[2];
                    column = segment[3];
                    return fields.map(vlq).join('');
                })
                .join(',');
        })
        .join(';');
}

function vlq(value: number): string {
    const digits = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
    let rest = value < 0 ? (-value << 1) | 1 : value << 1;
    let out = '';
    do {
        let digit = rest & 31;
        rest >>= 5;
        if (rest > 0) {
            digit |= 32;
        }
        out += digits[digit];
    } while (rest > 0);
    return out;
}

test('an internalConsole launch drops the trace\'s stderr lines, which reach the console from the inspector', () => {
    const text =
        'Debugger attached.\n[reactive] effect #0 woken by cell #1, set at main.vl:4:7\ncount is 2\n[reactive] derivation #2 woken by cell #1\n';
    assert.equal(shownStderr(text), 'count is 2\n');
    // A line that merely mentions the trace is the program's own.
    assert.equal(shownStderr('see [reactive] lines\n'), 'see [reactive] lines\n');
});

test('a type the program prints itself is printed by its function, the structure kept for a refusal', () => {
    const types = [
        { text: 'Secret', shape: 'struct', name: 'Secret', fields: [['code', 1]], fn: 0 },
        { text: 'i32', shape: 'integer' },
    ];
    // A written impl's `debug` answers text; a generated printer a group.
    assert.equal(walker([42], types, 0, 'value', () => '<redacted>'), '<redacted>');
    const printed = walker([42], types, 0, 'value', () => ({ o: 'Hidden(', c: ')', p: false, e: [], f: false }));
    assert.equal(flat(printed), 'Hidden()');
    // Not a document (or no function at that position): the structure.
    assert.equal(flat(walker([42], types, 0, 'value', () => 7)), 'Secret { code = 42 }');
    assert.equal(flat(walker([42], types, 0, 'value')), 'Secret { code = 42 }');
});

test('a trait object is printed by the type its table\'s functions name, else its show slot', () => {
    function area() {}
    function area2() {}
    const types = [
        { text: 'dyn Area', shape: 'object', label: 'dyn Area', impls: [[['area'], 1], [['area2'], 3]] },
        { text: 'Square', shape: 'struct', name: 'Square', fields: [['side', 2]] },
        { text: 'f64', shape: 'float' },
        { text: 'Rect', shape: 'struct', name: 'Rect', fields: [['width', 2]] },
    ];
    const square = Object.create({ area });
    const rect = Object.create({ area: area2 });
    assert.equal(flat(walker([[2], square], types, 0, 'value')), 'dyn Area(Square { side = 2.0 })');
    assert.equal(flat(walker([[3], rect], types, 0, 'value')), 'dyn Area(Rect { width = 3.0 })');
    // A table the sidecar does not know: the show slot, else `<?>`.
    function other() {}
    const shown = Object.create({ area: other, $show: () => 'Circle' });
    assert.equal(flat(walker([[1], shown], types, 0, 'value')), 'dyn Area(Circle)');
    assert.equal(flat(walker([[1], Object.create({ area: other })], types, 0, 'value')), 'dyn Area(<?>)');
});

test('the walker is sent the tables\' concrete types, renumbered', () => {
    const sidecar = new Sidecar({
        version: 1,
        file: 'main.mjs',
        frames: [],
        types: [
            { text: 'f64', shape: 'float' },
            { text: 'Square', shape: 'struct', name: 'Square', fields: [['side', 0]] },
            { text: 'dyn Area', shape: 'object', label: 'dyn Area', impls: [[['area'], 1]] },
        ],
    });
    const table = sidecar.table(2);
    assert.deepEqual(
        table.types.map((type) => type.text),
        ['dyn Area', 'Square', 'f64'],
    );
    assert.deepEqual(table.types[0].impls, [[['area'], 1]]);
    assert.deepEqual(table.types[1].fields, [['side', 2]]);
});
