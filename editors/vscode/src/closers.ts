// E222 / E214: the `>`s the `type` override placed, and may type over.
//
// VS Code types over a closing character only when its own auto-closing placed
// it, and a `>` the language server places (`onTypeFormatting`) is not one —
// so typing the `>` of `List<i32>` by hand doubled it (E214). The override in
// `extension.ts` places the `>` itself, and this map is what lets it step over
// it: VS Code's overtype, reimplemented. Kept current through every edit (an
// edit before a placed `>` shifts it; an edit over it forgets it), and
// forgotten once the caret leaves its line, as VS Code forgets its own.
//
// No `vscode` import: offsets and lines only, so `npm test` runs the whole
// keystroke story under plain node (`src/test/closers.test.ts`).

/// One text change in document offsets — the shape of VS Code's
/// `TextDocumentContentChangeEvent` (`rangeOffset`, `rangeLength`, `text`).
export interface OffsetChange {
    rangeOffset: number;
    rangeLength: number;
    text: string;
}

/// The placed `>`s, as document offsets per document key (a URI string).
export class PlacedClosers {
    private readonly byDocument = new Map<string, number[]>();

    /// A `>` was just placed at `offset`.
    place(document: string, offset: number): void {
        const closers = this.byDocument.get(document) ?? [];
        closers.push(offset);
        this.byDocument.set(document, closers);
    }

    /// A `>` is being typed with the caret at `offset`, before `following`
    /// (the character after the caret). True when it lands on a `>` this map
    /// placed: the caller moves the caret past it and inserts nothing. The
    /// placed `>` is spent either way — typed over once, or found no longer
    /// to be a `>`.
    typeOver(document: string, offset: number, following: string): boolean {
        const closers = this.byDocument.get(document);
        const index = closers?.indexOf(offset) ?? -1;
        if (closers === undefined || index < 0) {
            return false;
        }
        closers.splice(index, 1);
        if (closers.length === 0) {
            this.byDocument.delete(document);
        }
        return following === '>';
    }

    /// Keep every placed `>` at its character through `changes`, in the order
    /// VS Code reports them; one a change replaces is gone.
    track(document: string, changes: readonly OffsetChange[]): void {
        const closers = this.byDocument.get(document);
        if (closers === undefined) {
            return;
        }
        for (const change of changes) {
            const start = change.rangeOffset;
            const end = start + change.rangeLength;
            const delta = change.text.length - change.rangeLength;
            for (let index = closers.length - 1; index >= 0; index--) {
                if (closers[index] >= end) {
                    closers[index] += delta;
                } else if (closers[index] >= start) {
                    closers.splice(index, 1);
                }
            }
        }
        if (closers.length === 0) {
            this.byDocument.delete(document);
        }
    }

    /// Forget every placed `>` not on `line` (where the caret now is);
    /// `lineOf` maps an offset to its line.
    keepLine(document: string, line: number, lineOf: (offset: number) => number): void {
        const closers = this.byDocument.get(document);
        if (closers === undefined) {
            return;
        }
        const kept = closers.filter((offset) => lineOf(offset) === line);
        if (kept.length === 0) {
            this.byDocument.delete(document);
        } else {
            this.byDocument.set(document, kept);
        }
    }

    /// The document closed: forget its `>`s.
    forget(document: string): void {
        this.byDocument.delete(document);
    }

    /// The override was removed: forget every `>`.
    clear(): void {
        this.byDocument.clear();
    }

    /// The placed offsets in `document`, for tests and the status page.
    offsets(document: string): readonly number[] {
        return this.byDocument.get(document) ?? [];
    }
}
