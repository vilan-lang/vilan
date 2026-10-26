// E257 S7: the source map `vilan run` and `vilan build` write beside a script
// (E257 S5/S6), read both ways: a JavaScript position to the `.vl` place it
// came from (a frame), and a `.vl` line to the JavaScript positions that run
// it (a breakpoint). No `vscode` import: `npm test` pins it under node.
import * as path from 'path';
import { fileURLToPath } from 'url';

/// One decoded segment: a generated column and, when it maps, the original
/// source, line and column (all 0-based) and a `names` index.
export interface Segment {
    column: number;
    source: number;
    line: number;
    originalColumn: number;
    name?: number;
}

/// A map source: where its text is.
export interface MapSourceEntry {
    /// The file on disk, for a `file:` source or a relative one; `undefined`
    /// for a synthetic one (`vilan:///runtime.js`).
    path?: string;
    /// The source as the map spells it.
    url: string;
    content?: string;
    ignored: boolean;
}

export interface GeneratedPosition {
    line: number;
    column: number;
}

export interface OriginalPosition {
    source: number;
    line: number;
    column: number;
    name?: string;
}

const BASE64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
const DIGIT = new Map<string, number>([...BASE64].map((character, index) => [character, index]));

export class SourceMap {
    /// Per generated line, its segments in column order.
    readonly lines: Segment[][];
    readonly sources: MapSourceEntry[];
    readonly names: string[];

    constructor(json: any, mapLocation: string) {
        const directory = mapDirectory(mapLocation);
        const ignored = new Set<number>(json.ignoreList ?? json.x_google_ignoreList ?? []);
        const sourceRoot: string = json.sourceRoot ?? '';
        this.sources = (json.sources ?? []).map((source: string, index: number) => ({
            url: source,
            path: sourcePath(sourceRoot + source, directory),
            content: json.sourcesContent?.[index] ?? undefined,
            ignored: ignored.has(index),
        }));
        this.names = json.names ?? [];
        this.lines = decodeMappings(json.mappings ?? '');
    }

    /// The source index whose file is `file`, compared as the OS compares
    /// paths (case-insensitively on Windows).
    sourceIndex(file: string): number | undefined {
        const wanted = normalize(file);
        const index = this.sources.findIndex(
            (source) => source.path !== undefined && normalize(source.path) === wanted,
        );
        return index < 0 ? undefined : index;
    }

    /// The original place of a generated position: the last segment at or
    /// before it on its line.
    original(position: GeneratedPosition): OriginalPosition | undefined {
        const segments = this.lines[position.line];
        if (!segments) {
            return undefined;
        }
        let found: Segment | undefined;
        for (const segment of segments) {
            if (segment.column > position.column) {
                break;
            }
            found = segment;
        }
        if (!found || found.source < 0) {
            return undefined;
        }
        return {
            source: found.source,
            line: found.line,
            column: found.originalColumn,
            name: found.name === undefined ? undefined : this.names[found.name],
        };
    }

    /// The segment that STARTS exactly at `position`, if one does (a function
    /// head is marked at the column V8 reports it at).
    segmentAt(position: GeneratedPosition): Segment | undefined {
        return this.lines[position.line]?.find((segment) => segment.column === position.column && segment.source >= 0);
    }

    /// The function a generated position stands in, by the nearest NAMED
    /// segment before it - a function head is the one mark that carries a
    /// `names` entry, and the emitted functions are flat at the module's top
    /// level, so the last head before a place is the function holding it (a
    /// function declared inside another is the one blind spot).
    enclosingFunctionName(position: GeneratedPosition): string | undefined {
        for (let line = Math.min(position.line, this.lines.length - 1); line >= 0; line--) {
            const segments = this.lines[line];
            for (let index = segments.length - 1; index >= 0; index--) {
                const segment = segments[index];
                if (line === position.line && segment.column > position.column) {
                    continue;
                }
                if (segment.name !== undefined) {
                    return this.names[segment.name];
                }
            }
        }
        return undefined;
    }

    /// Where a breakpoint on `line` of `source` binds: the line's statement
    /// starts — every segment at the line's smallest original column, one per
    /// emitted copy (each instance of a generic, a closure's re-emission). A
    /// line with no segment binds on the next line that has one, up to
    /// `slide` lines on, and the answer says which line that was.
    breakpointPositions(
        source: number,
        line: number,
        slide = 8,
    ): { line: number; positions: GeneratedPosition[] } | undefined {
        const index = this.originalIndex();
        for (let candidate = line; candidate <= line + slide; candidate++) {
            const hits = index.get(`${source}:${candidate}`);
            if (hits && hits.length > 0) {
                const smallest = Math.min(...hits.map((hit) => hit.column));
                return {
                    line: candidate,
                    positions: hits
                        .filter((hit) => hit.column === smallest)
                        .map((hit) => hit.position),
                };
            }
        }
        return undefined;
    }

    private byOriginal: Map<string, { position: GeneratedPosition; column: number }[]> | undefined;

    /// Every segment by its original `source:line`, built on the first ask.
    private originalIndex(): Map<string, { position: GeneratedPosition; column: number }[]> {
        if (!this.byOriginal) {
            const index = new Map<string, { position: GeneratedPosition; column: number }[]>();
            this.lines.forEach((segments, generatedLine) => {
                for (const segment of segments) {
                    if (segment.source < 0) {
                        continue;
                    }
                    const key = `${segment.source}:${segment.line}`;
                    let hits = index.get(key);
                    if (!hits) {
                        hits = [];
                        index.set(key, hits);
                    }
                    hits.push({
                        position: { line: generatedLine, column: segment.column },
                        column: segment.originalColumn,
                    });
                }
            });
            this.byOriginal = index;
        }
        return this.byOriginal;
    }

    /// The generated ranges that map into an ignore-listed source (std, the
    /// runtime helpers): `[start, end)` pairs in generated order, for the
    /// inspector's blackboxed ranges.
    ignoredRanges(): [GeneratedPosition, GeneratedPosition][] {
        const ranges: [GeneratedPosition, GeneratedPosition][] = [];
        let open: GeneratedPosition | undefined;
        this.lines.forEach((segments, line) => {
            for (const segment of segments) {
                const ignored = segment.source >= 0 && this.sources[segment.source]?.ignored === true;
                if (ignored && !open) {
                    open = { line, column: segment.column };
                } else if (!ignored && open) {
                    ranges.push([open, { line, column: segment.column }]);
                    open = undefined;
                }
            }
        });
        if (open) {
            ranges.push([open, { line: this.lines.length, column: 0 }]);
        }
        return ranges;
    }
}

/// `mappings`, decoded: per generated line, `[column, source, line, column, name?]`
/// segments with every relative field made absolute.
export function decodeMappings(mappings: string): Segment[][] {
    const lines: Segment[][] = [];
    let source = 0;
    let line = 0;
    let originalColumn = 0;
    let name = 0;
    for (const text of mappings.split(';')) {
        const segments: Segment[] = [];
        let column = 0;
        if (text.length > 0) {
            for (const field of text.split(',')) {
                const values = decodeVlq(field);
                if (values.length === 0) {
                    continue;
                }
                column += values[0];
                if (values.length >= 4) {
                    source += values[1];
                    line += values[2];
                    originalColumn += values[3];
                    const segment: Segment = { column, source, line, originalColumn };
                    if (values.length >= 5) {
                        name += values[4];
                        segment.name = name;
                    }
                    segments.push(segment);
                } else {
                    segments.push({ column, source: -1, line: -1, originalColumn: -1 });
                }
            }
        }
        segments.sort((left, right) => left.column - right.column);
        lines.push(segments);
    }
    return lines;
}

/// One segment's base-64 VLQ fields.
export function decodeVlq(text: string): number[] {
    const values: number[] = [];
    let value = 0;
    let shift = 0;
    for (const character of text) {
        const digit = DIGIT.get(character);
        if (digit === undefined) {
            return values;
        }
        value += (digit & 31) * 2 ** shift;
        if (digit & 32) {
            shift += 5;
        } else {
            const negative = value % 2 === 1;
            const magnitude = Math.floor(value / 2);
            values.push(negative ? -magnitude : magnitude);
            value = 0;
            shift = 0;
        }
    }
    return values;
}

/// A `file:` URL or a path, as a path; a relative one against `directory`.
function sourcePath(source: string, directory: string | undefined): string | undefined {
    if (source.startsWith('file:')) {
        try {
            return fileURLToPath(source);
        } catch {
            return undefined;
        }
    }
    if (/^[a-z][a-z0-9+.-]*:/i.test(source) && !/^[a-z]:[\\/]/i.test(source)) {
        // Another scheme: `vilan:///runtime.js`, synthetic.
        return undefined;
    }
    if (path.isAbsolute(source)) {
        return source;
    }
    return directory === undefined ? undefined : path.resolve(directory, decodeURIComponent(source));
}

/// The directory a map's relative sources resolve against.
function mapDirectory(mapLocation: string): string | undefined {
    if (mapLocation.startsWith('file:')) {
        try {
            return path.dirname(fileURLToPath(mapLocation));
        } catch {
            return undefined;
        }
    }
    if (path.isAbsolute(mapLocation)) {
        return path.dirname(mapLocation);
    }
    return undefined;
}

/// A path as the file system compares it.
export function normalize(file: string): string {
    const resolved = path.resolve(file);
    return process.platform === 'win32' ? resolved.toLowerCase() : resolved;
}
