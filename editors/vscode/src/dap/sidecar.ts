// E257 S7: the `.vdbg.json` sidecar `vilan build`/`vilan run` write beside a
// script's map (`crates/vilan-core/src/debug_info.rs` writes it; the format
// is documented there): each user function's bindings by vilan name, the
// JavaScript name each got, and its static type's layout. No `vscode`
// import: `npm test` pins it under node.
import type { OriginalPosition } from './sourceMap';

export const SIDECAR_VERSION = 1;

export type BindingRead = 'value' | 'boxed' | 'pair' | 'lazy';

export interface SidecarBinding {
    name: string;
    js: string;
    type?: number;
    parameter?: boolean;
    view?: boolean;
    read?: BindingRead;
}

export interface SidecarFrame {
    kind: 'function' | 'closure' | 'main';
    name: string;
    js?: string;
    /// The head's original place: `[sources index, line, column]`, 0-based.
    at?: [number, number, number];
    /// The function's whole text: `[sources index, line, column, end line,
    /// end column]`, 0-based, the end exclusive (E326).
    span?: [number, number, number, number, number];
    bindings: SidecarBinding[];
}

/// A layout (`debug_info.rs`'s `Layout`), as the walker reads it.
export interface SidecarType {
    text: string;
    shape: string;
    [field: string]: unknown;
}

/// The frame a JavaScript frame resolved to: one record, or several merged
/// (the instances a shared body serves, the copies of one closure).
export interface ResolvedFrame {
    name: string;
    kind: SidecarFrame['kind'];
    bindings: SidecarBinding[];
}

export class Sidecar {
    readonly frames: SidecarFrame[];
    readonly types: SidecarType[];
    private readonly tables = new Map<number, { types: SidecarType[]; root: number }>();

    constructor(json: any) {
        if (json?.version !== SIDECAR_VERSION) {
            throw new Error(`unsupported sidecar version ${json?.version}`);
        }
        this.frames = Array.isArray(json.frames) ? json.frames : [];
        this.types = Array.isArray(json.types) ? json.types : [];
    }

    /// The record for a JavaScript frame: `functionName` as the inspector
    /// names it, and `head`, where the function's start maps to. A frame of
    /// the script's own top level (`functionName` empty, no head) is `main`'s.
    resolve(functionName: string, head: OriginalPosition | undefined, topLevel: boolean): ResolvedFrame | undefined {
        if (topLevel) {
            return merge(this.frames.filter((frame) => frame.kind === 'main'));
        }
        let candidates: SidecarFrame[] = head
            ? this.frames.filter(
                  (frame) =>
                      frame.at !== undefined &&
                      frame.at[0] === head.source &&
                      frame.at[1] === head.line &&
                      frame.at[2] === head.column,
              )
            : [];
        if (candidates.length > 1) {
            const named = candidates.filter((frame) => frame.js === functionName);
            if (named.length > 0) {
                candidates = named;
            }
        }
        if (candidates.length === 0 && functionName) {
            candidates = this.frames.filter((frame) => frame.kind === 'function' && frame.js === functionName);
        }
        return merge(candidates);
    }

    /// The record for an AWAITING frame of an async stack (E326), which names
    /// its function but not where the function starts — only the place it
    /// stands, `place`. A named function is found by its emitted name; an
    /// anonymous one (a closure, an async `main`'s arrow, the script's top
    /// level) is the innermost record whose text holds the place.
    resolveAt(functionName: string, place: OriginalPosition | undefined): ResolvedFrame | undefined {
        if (functionName) {
            const named = this.frames.filter((frame) => frame.kind === 'function' && frame.js === functionName);
            if (named.length > 0) {
                return merge(named);
            }
        }
        if (!place) {
            return undefined;
        }
        const holding = this.frames.filter((frame) => frame.span !== undefined && holds(frame.span, place));
        if (holding.length === 0) {
            return undefined;
        }
        // Innermost: the text that starts last (texts nest, they never cross).
        const innermost = holding.reduce((best, frame) =>
            before(best.span as Span, frame.span as Span) ? frame : best,
        );
        const span = innermost.span as Span;
        return merge(holding.filter((frame) => sameSpan(frame.span as Span, span)));
    }

    /// The text a binding's type reads as in a pane: `view i32`, `List<Point>`.
    typeText(binding: SidecarBinding): string | undefined {
        if (binding.type === undefined) {
            return undefined;
        }
        const text = this.types[binding.type]?.text;
        return text === undefined ? undefined : binding.view ? `view ${text}` : text;
    }

    /// The types `root` reaches, renumbered from 0 — what the walker is sent.
    table(root: number): { types: SidecarType[]; root: number } {
        const cached = this.tables.get(root);
        if (cached) {
            return cached;
        }
        const order: number[] = [];
        const index = new Map<number, number>();
        const visit = (type: number) => {
            if (index.has(type) || this.types[type] === undefined) {
                return;
            }
            index.set(type, order.length);
            order.push(type);
            for (const reached of references(this.types[type])) {
                visit(reached);
            }
        };
        visit(root);
        const renumber = (type: number) => index.get(type) ?? 0;
        const table = {
            types: order.map((type) => renumbered(this.types[type], renumber)),
            root: 0,
        };
        this.tables.set(root, table);
        return table;
    }

    /// A vilan path's static walk from a binding's type: `.field` into a
    /// struct, `.0`/`.label` into a tuple, `[i]` into a list. Answers the
    /// JavaScript accessors that read it and the type it reaches, or
    /// `undefined` where the path leaves what the layouts describe.
    step(type: number, segment: PathSegment): { access: string; type: number } | undefined {
        const layout = this.types[type];
        if (!layout) {
            return undefined;
        }
        switch (layout.shape) {
            case 'struct': {
                if (segment.kind !== 'field') {
                    return undefined;
                }
                const fields = layout.fields as [string, number][];
                const slot = fields.findIndex(([name]) => name === segment.name);
                return slot < 0 ? undefined : { access: `[${slot}]`, type: fields[slot][1] };
            }
            case 'tuple': {
                if (segment.kind !== 'field') {
                    return undefined;
                }
                const elements = layout.elements as [string, number, number][];
                const position = /^\d+$/.test(segment.name)
                    ? Number(segment.name)
                    : elements.findIndex(([opens]) => opens === `${segment.name} = `);
                if (position < 0 || position >= elements.length) {
                    return undefined;
                }
                let offset = 0;
                for (let slot = 0; slot < position; slot++) {
                    offset += elements[slot][2];
                }
                const [, element, slots] = elements[position];
                return {
                    access: slots === 1 ? `[${offset}]` : `.slice(${offset}, ${offset + slots})`,
                    type: element,
                };
            }
            case 'list':
                return segment.kind === 'index'
                    ? { access: `[${segment.index}]`, type: layout.element as number }
                    : undefined;
            default:
                return undefined;
        }
    }
}

export type PathSegment = { kind: 'field'; name: string } | { kind: 'index'; index: number };

/// `point.from.x`, `rows[0]`, `pair.1`: a binding and the steps after it, or
/// `undefined` for anything else (which the adapter evaluates as JavaScript).
export function parsePath(expression: string): { root: string; segments: PathSegment[] } | undefined {
    const text = expression.trim();
    const head = /^[A-Za-z_][A-Za-z0-9_]*/.exec(text);
    if (!head) {
        return undefined;
    }
    const segments: PathSegment[] = [];
    let rest = text.slice(head[0].length);
    while (rest.length > 0) {
        const field = /^\.\s*([A-Za-z_][A-Za-z0-9_]*|\d+)/.exec(rest);
        if (field) {
            segments.push({ kind: 'field', name: field[1] });
            rest = rest.slice(field[0].length);
            continue;
        }
        const index = /^\[\s*(\d+)\s*\]/.exec(rest);
        if (index) {
            segments.push({ kind: 'index', index: Number(index[1]) });
            rest = rest.slice(index[0].length);
            continue;
        }
        return undefined;
    }
    return { root: head[0], segments };
}

type Span = [number, number, number, number, number];

/// Whether `span` holds `place` (its source, and between its start and its end).
function holds(span: Span, place: OriginalPosition): boolean {
    const [source, line, column, endLine, endColumn] = span;
    if (source !== place.source) {
        return false;
    }
    const afterStart = place.line > line || (place.line === line && place.column >= column);
    const beforeEnd = place.line < endLine || (place.line === endLine && place.column < endColumn);
    return afterStart && beforeEnd;
}

/// Whether `outer` starts before `inner` does.
function before(outer: Span, inner: Span): boolean {
    return outer[1] < inner[1] || (outer[1] === inner[1] && outer[2] < inner[2]);
}

function sameSpan(left: Span, right: Span): boolean {
    return left.every((value, index) => value === right[index]);
}

/// One record, or the rows several agree on: a binding keeps its type only
/// where every record that lists it gives the same one.
function merge(frames: SidecarFrame[]): ResolvedFrame | undefined {
    if (frames.length === 0) {
        return undefined;
    }
    if (frames.length === 1) {
        return { name: frames[0].name, kind: frames[0].kind, bindings: frames[0].bindings };
    }
    const bindings: SidecarBinding[] = [];
    for (const binding of frames[0].bindings) {
        const others = frames.slice(1).map((frame) => frame.bindings.find((other) => other.js === binding.js));
        const agree = others.every(
            (other) => other !== undefined && other.type === binding.type && (other.read ?? 'value') === (binding.read ?? 'value'),
        );
        bindings.push(agree ? binding : { ...binding, type: undefined });
    }
    return { name: frames[0].name, kind: frames[0].kind, bindings };
}

/// Every type index a layout names.
function references(type: SidecarType): number[] {
    switch (type.shape) {
        case 'struct':
            return (type.fields as [string, number][]).map(([, field]) => field);
        case 'enum':
            return (type.variants as [string, number[]][]).flatMap(([, payload]) => payload);
        case 'tuple':
            return (type.elements as [string, number, number][]).map(([, element]) => element);
        case 'list':
        case 'set':
            return [type.element as number];
        case 'shared':
        case 'cell':
            return [type.value as number];
        case 'map':
            return [type.key as number, type.value as number];
        default:
            return [];
    }
}

/// `type` with every index it names passed through `renumber`.
function renumbered(type: SidecarType, renumber: (type: number) => number): SidecarType {
    switch (type.shape) {
        case 'struct':
            return {
                ...type,
                fields: (type.fields as [string, number][]).map(([name, field]) => [name, renumber(field)]),
            };
        case 'enum':
            return {
                ...type,
                variants: (type.variants as [string, number[]][]).map(([name, payload]) => [
                    name,
                    payload.map(renumber),
                ]),
            };
        case 'tuple':
            return {
                ...type,
                elements: (type.elements as [string, number, number][]).map(([opens, element, slots]) => [
                    opens,
                    renumber(element),
                    slots,
                ]),
            };
        case 'list':
        case 'set':
            return { ...type, element: renumber(type.element as number) };
        case 'shared':
        case 'cell':
            return { ...type, value: renumber(type.value as number) };
        case 'map':
            return { ...type, key: renumber(type.key as number), value: renumber(type.value as number) };
        default:
            return type;
    }
}
