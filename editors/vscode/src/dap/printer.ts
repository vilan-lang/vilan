// E257 S7: `dbg`'s printer, for the debugger's panes.
//
// A value is printed in two halves, as the generated `__show_*` printers
// print it (debugging.md §2, S1):
//
// - IN THE PROGRAM, the WALKER reads the value by its static type — the
//   sidecar's layout — into a DOCUMENT: a string, or a group
//   `{ o, c, p, e, f }` (open text, close text, padded, `[label, document]`
//   entries, fill). It is the generated printers' reading, written once over
//   the layout table instead of once per type: a struct is its field array, an
//   enum `[tag, ...payload]`, a tuple its flat slots, a cell its `Shared`'s
//   `.v`, a map its table's `(K, V)` pairs. It runs in the paused program
//   (`Runtime.callFunctionOn`), so a value is read in ONE round trip, and
//   it adds a `k` (which layout made the group) the panes use to name a
//   group's children.
// - HERE, the document is laid out: [`flat`] is `print`'s one line (S3) and
//   the Variables pane's value; [`layout`] is `dbg`'s break rule. Both are
//   `__dbg_flat`/`__dbg_layout`, the JS runtime's own (transformer.rs), line
//   for line — the scripted session pins the panes against `dbg`'s output.
//
// No `vscode` import: `npm test` pins it under node.

/// A printed value: text, or a group of entries.
export type Doc = string | Group;

export interface Group {
    /// Open and close text: `Point {` `}`, `[` `]`, `Some(` `)`.
    o: string;
    c: string;
    /// Entries padded by a space inside the brackets (`Point { x = 1 }`).
    p: boolean;
    e: [string, Doc][];
    /// A broken group fills its lines (a list or set of scalars, E277).
    f: boolean;
    /// Which layout made the group: `s`truct, `t`uple, `l`ist, `e`num,
    /// `m`ap, `z` (set), `c`ell, `h` (shared), `o`bject. Absent from a
    /// document a program's own `show` slot printed.
    k?: string;
}

/// Characters, as the runtime counts a width (`__dbg_width`).
export function width(text: string): number {
    let count = 0;
    for (const _ of text) {
        count++;
    }
    return count;
}

/// The document on one line (`__dbg_flat`).
export function flat(document: Doc): string {
    if (typeof document === 'string') {
        return document;
    }
    if (document.e.length === 0) {
        return document.o + document.c;
    }
    const inner = document.e.map((entry) => entry[0] + flat(entry[1])).join(', ');
    return document.p
        ? document.o + ' ' + inner + ' ' + document.c
        : document.o + inner + document.c;
}

/// The document laid out from `column`, broken past 80 columns
/// (`__dbg_layout`).
export function layout(document: Doc, column: number, indent: number): string {
    const one = flat(document);
    if (typeof document === 'string' || document.e.length === 0 || column + width(one) <= 80) {
        return one;
    }
    const pad = ' '.repeat(indent + 2);
    let out = document.o + '\n';
    if (document.f) {
        let line = '';
        for (const entry of document.e) {
            const text = entry[0] + flat(entry[1]) + ',';
            if (line === '') {
                line = pad + text;
            } else if (width(line) + 1 + width(text) > 80) {
                out += line + '\n';
                line = pad + text;
            } else {
                line += ' ' + text;
            }
        }
        return out + line + '\n' + ' '.repeat(indent) + document.c;
    }
    for (const entry of document.e) {
        out +=
            pad + entry[0] + layout(entry[1], width(pad + entry[0]), indent + 2) + ',\n';
    }
    return out + ' '.repeat(indent) + document.c;
}

/// What `dbg(name)` writes after its location: `name = <value>`, laid out
/// from where the value starts on the line `[file:line:col] name = ` opens.
export function dbgLine(location: string, name: string, document: Doc): string {
    const head = '[' + location + '] ' + name + ' = ';
    return head + layout(document, width(head), 0);
}

/// One child of a group, as a pane lists it: its name, its document, and
/// how a path to it is written (`.x`, `[0]`, `.1`) when the language can
/// write one.
export interface Child {
    name: string;
    document: Doc;
    path?: string;
}

/// A group's children: a struct's fields by name, a list's elements and a
/// tuple's slots by position, a map's values under their keys' text, a
/// payload by position. A cell, a `Shared` and a trait object have one child,
/// the value inside.
export function children(document: Doc): Child[] {
    if (typeof document === 'string') {
        return [];
    }
    return document.e.map(([label, entry], index) => {
        switch (document.k) {
            case 's': {
                const name = label.endsWith(' = ') ? label.slice(0, -3) : label;
                return { name, document: entry, path: `.${name}` };
            }
            case 'l':
                return typeof entry === 'string' && entry.startsWith('… ')
                    ? { name: '…', document: entry }
                    : { name: String(index), document: entry, path: `[${index}]` };
            case 't': {
                const named = label.endsWith(' = ') ? label.slice(0, -3) : undefined;
                return { name: named ?? String(index), document: entry, path: `.${named ?? index}` };
            }
            case 'm':
                return label.endsWith(' => ')
                    ? { name: label.slice(0, -4), document: entry }
                    : { name: '…', document: entry };
            case 'z':
                return typeof entry === 'string' && entry.startsWith('… ')
                    ? { name: '…', document: entry }
                    : { name: String(index), document: entry };
            case 'c':
            case 'h':
            case 'o':
                return { name: 'value', document: entry };
            default:
                return { name: label.replace(/ = $/, '') || String(index), document: entry };
        }
    });
}

/// Whether a pane should let the user expand `document`.
export function expandable(document: Doc): boolean {
    return typeof document !== 'string' && document.e.length > 0;
}

/// The walker, run in the paused program (`Runtime.callFunctionOn` is
/// handed [`WALKER`], its source). `this` is any object in the program's
/// context; the arguments are the value, the layout table (only the types the
/// root reaches), the root's index in it, and how the binding holds its value
/// (`value`, `boxed`, `pair`, `lazy`).
///
/// It answers the document, or throws on a value the layout does not
/// describe (an uninitialized binding); the adapter then shows the JavaScript
/// value. A type whose layout carries `fn` is printed by the program's own
/// function at that position of `printers` (E323: the type's printer, or its
/// written `Debug` impl's `debug`, which the adapter found in the paused
/// program) — what `dbg` prints, since a written impl decides that (E275). A
/// trait object is printed by the type its table's functions name (E324),
/// else by the table's `show` slot. Self-contained — it is sent as text — and kept in the runtime's own
/// spelling (`__dbg_str`, `__dbg_float`, `__dbg_list`, `__dbg_members`,
/// `__dbg_map`, the generated `__show_*` bodies), so it reads exactly what
/// they read.
export function walker(value: any, types: any[], root: number, read: string, ...printers: any[]): Doc {
    let budget = 20000;
    const seen: unknown[] = [];
    const group = (o: string, c: string, p: boolean, e: [string, Doc][], f: boolean, k: string): Group => ({
        o: o,
        c: c,
        p: p,
        e: e,
        f: f === true,
        k: k,
    });
    const str = (text: string): string => {
        let out = '"';
        for (const character of text) {
            if (character === '\\') out += '\\\\';
            else if (character === '"') out += '\\"';
            else if (character === '\n') out += '\\n';
            else if (character === '\t') out += '\\t';
            else if (character === '\r') out += '\\r';
            else if (character === '\0') out += '\\0';
            else out += character;
        }
        return out + '"';
    };
    const float = (number: number): string => {
        if (Object.is(number, -0)) return '-0.0';
        const text = String(number);
        return Number.isInteger(number) && !text.includes('e') ? text + '.0' : text;
    };
    const one = (document: Doc): string => {
        if (typeof document === 'string') return document;
        if (document.e.length === 0) return document.o + document.c;
        const inner = document.e.map((entry) => entry[0] + one(entry[1])).join(', ');
        return document.p ? document.o + ' ' + inner + ' ' + document.c : document.o + inner + document.c;
    };
    const members = (
        open: string,
        items: any[],
        entry: (item: any) => [string, Doc],
        fill: boolean,
        kind: string,
    ): Group => {
        const entries: [string, Doc][] = [];
        for (const item of items) {
            if (entries.length === 100) {
                entries.push(['', '\u2026 ' + (items.length - 100) + ' more']);
                break;
            }
            entries.push(entry(item));
        }
        return group(open, '}', true, entries, fill, kind);
    };
    const document = (printed: any): Doc | undefined =>
        typeof printed === 'string' ||
        (printed !== null && typeof printed === 'object' && typeof printed.o === 'string' && Array.isArray(printed.e))
            ? printed
            : undefined;
    const tableOf = (object: any, impls: any[]): number | undefined => {
        if (!object || !Array.isArray(impls)) return undefined;
        const slots = Object.getPrototypeOf(object) || object;
        const names: string[] = [];
        for (const member of Object.keys(slots)) {
            if (member === '$show') continue;
            names.push(typeof slots[member] === 'function' ? slots[member].name : '');
        }
        for (const known of impls) {
            const methods = known[0];
            if (methods.length === names.length && methods.every((name: string, slot: number) => name === names[slot])) {
                return known[1];
            }
        }
        return undefined;
    };
    const show = (current: any, index: number): Doc => {
        if (--budget < 0) return '\u2026';
        const type = types[index];
        if (typeof type.fn === 'number' && typeof printers[type.fn] === 'function') {
            const printed = document(printers[type.fn](current));
            if (printed !== undefined) return printed;
        }
        switch (type.shape) {
            case 'integer':
            case 'bigint':
            case 'bool':
                return '' + current;
            case 'float':
                return float(current);
            case 'str':
                return str(current);
            case 'void':
                return '()';
            case 'text':
                return type.value;
            case 'struct':
                if (type.fields.length === 0) return type.name;
                return group(
                    type.name + ' {',
                    '}',
                    true,
                    type.fields.map((field: [string, number], slot: number): [string, Doc] => [
                        field[0] + ' = ',
                        show(current[slot], field[1]),
                    ]),
                    false,
                    's',
                );
            case 'tuple': {
                let offset = 0;
                const entries: [string, Doc][] = [];
                for (const element of type.elements) {
                    const slots = element[2];
                    entries.push([
                        element[0],
                        show(slots === 1 ? current[offset] : current.slice(offset, offset + slots), element[1]),
                    ]);
                    offset += slots;
                }
                return group('(', ')', false, entries, false, 't');
            }
            case 'list': {
                const entries: [string, Doc][] = [];
                const shown = Math.min(current.length, 100);
                for (let slot = 0; slot < shown; slot++) entries.push(['', show(current[slot], type.element)]);
                if (current.length > shown) entries.push(['', '\u2026 ' + (current.length - shown) + ' more']);
                return group('[', ']', false, entries, type.fill === true, 'l');
            }
            case 'shared': {
                if (seen.includes(current)) return '<cycle>';
                seen.push(current);
                try {
                    return group('Shared(', ')', false, [['', show(current.v, type.value)]], false, 'h');
                } finally {
                    seen.pop();
                }
            }
            case 'cell':
                return group(type.label + '(', ')', false, [['', show(current[type.field].v, type.value)]], false, 'c');
            case 'map':
                return members(
                    type.label + ' {',
                    Array.from(current[type.field].values()),
                    (pair: any): [string, Doc] => {
                        const key = type.keyWidth === 1 ? pair[0] : pair.slice(0, type.keyWidth);
                        const entry =
                            type.valueWidth === 1
                                ? pair[type.keyWidth]
                                : pair.slice(type.keyWidth, type.keyWidth + type.valueWidth);
                        return [one(show(key, type.key)) + ' => ', show(entry, type.value)];
                    },
                    false,
                    'm',
                );
            case 'set':
                return members(
                    type.label + ' {',
                    Array.from(current[type.field].values()),
                    (item: any): [string, Doc] => ['', show(item, type.element)],
                    type.fill === true,
                    'z',
                );
            case 'object': {
                const table = current[1];
                const concrete = tableOf(table, type.impls);
                const inner: Doc =
                    concrete !== undefined
                        ? show(current[0], concrete)
                        : table && typeof table.$show === 'function'
                          ? table.$show(current[0])
                          : '<?>';
                return group(type.label + '(', ')', false, [['', inner]], false, 'o');
            }
            case 'enum': {
                const variants = type.variants;
                let chosen = variants.length - 1;
                for (let tag = 0; tag < variants.length - 1; tag++) {
                    if (current[0] === tag) {
                        chosen = tag;
                        break;
                    }
                }
                const variant = variants[chosen];
                if (variant === undefined) return '<?>';
                if (variant[1].length === 0) return variant[0];
                return group(
                    variant[0] + '(',
                    ')',
                    false,
                    variant[1].map((payload: number, slot: number): [string, Doc] => ['', show(current[slot + 1], payload)]),
                    false,
                    'e',
                );
            }
            case 'backed':
                for (const variant of type.variants) if (current === variant[1]) return variant[0];
                return '<?>';
            default:
                return '<?>';
        }
    };
    if (read === 'lazy') return '<lazy, not forced>';
    if (read === 'boxed') value = value[0];
    else if (read === 'pair') value = value[0][value[1]];
    return show(value, root);
}

/// [`walker`]'s source, for `Runtime.callFunctionOn`.
export const WALKER = walker.toString();
