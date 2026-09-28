// E214: with the `type` override installed, a `>` typed after an auto-placed
// `>` steps over it — run under plain node by `npm test`.
//
// `Keyboard` below is `extension.ts`'s `typeOne` over a one-line document, with
// the REAL `PlacedClosers`: a `>` first asks the map whether it lands on a
// placed one (and then only moves the caret); every other keystroke is typed
// through (`default:type`, which moves the caret past what it inserts); after a
// `<` the server's answer decides whether a `>` is placed AT the caret, the
// caret put back before it, and the offset remembered. Every edit goes through
// `track`, as `workspace.onDidChangeTextDocument(trackClosers)` does.
import { test } from 'node:test';
import * as assert from 'node:assert/strict';
import { PlacedClosers } from '../closers';

const DOCUMENT = 'file:///e214.vl';

class Keyboard {
    readonly closers = new PlacedClosers();
    constructor(
        public text: string,
        public caret: number,
        /// The server's `vilan/opensAGenericList` answer for the `<` just typed.
        private readonly opens: (textBeforeCaret: string) => boolean,
    ) {}

    /// An edit, as the document sees it; the caret moves past an insertion at
    /// or before it, as VS Code's does.
    private edit(offset: number, removed: number, inserted: string): void {
        this.text = this.text.slice(0, offset) + inserted + this.text.slice(offset + removed);
        this.closers.track(DOCUMENT, [{ rangeOffset: offset, rangeLength: removed, text: inserted }]);
        if (offset <= this.caret) {
            this.caret += inserted.length - Math.min(removed, this.caret - offset);
        }
    }

    type(characters: string): this {
        for (const character of characters) {
            const following = this.text.charAt(this.caret);
            if (character === '>' && this.closers.typeOver(DOCUMENT, this.caret, following)) {
                this.caret += 1;
                continue;
            }
            this.edit(this.caret, 0, character);
            if (character === '<' && this.opens(this.text.slice(0, this.caret))) {
                const caret = this.caret;
                this.edit(caret, 0, '>');
                this.caret = caret;
                this.closers.place(DOCUMENT, caret);
            }
        }
        return this;
    }

    /// The document with the caret drawn as `|`.
    get shown(): string {
        return `${this.text.slice(0, this.caret)}|${this.text.slice(this.caret)}`;
    }
}

/// E202's rule, near enough for these exhibits: a `<` after a type name opens
/// a generic list, one after a spaced operand does not.
const afterATypeName = (before: string) => /[A-Z]\w*<$/.test(before);

test('`List<` places its `>` after the caret, and typing `>` steps over it', () => {
    const keyboard = new Keyboard('let xs: ', 8, afterATypeName).type('List<');
    assert.equal(keyboard.shown, 'let xs: List<|>');
    keyboard.type('i32>');
    // The E214 symptom was `List<i32>>|`.
    assert.equal(keyboard.shown, 'let xs: List<i32>|');
    assert.deepEqual(keyboard.closers.offsets(DOCUMENT), [], 'the placed `>` is spent');
});

test('nested lists type over both closers, innermost first', () => {
    const keyboard = new Keyboard('let x: ', 7, afterATypeName).type('Map<str, List<');
    assert.equal(keyboard.shown, 'let x: Map<str, List<|>>');
    keyboard.type('i32>>;');
    assert.equal(keyboard.shown, 'let x: Map<str, List<i32>>;|');
});

test('a comparison `<` places nothing, and its `>` is typed as written', () => {
    const keyboard = new Keyboard('if a ', 5, afterATypeName).type('< b && c >');
    assert.equal(keyboard.shown, 'if a < b && c >|');
});

test('a second `>` typed past a stepped-over one is inserted, not swallowed', () => {
    const keyboard = new Keyboard('', 0, afterATypeName).type('List<i32>>');
    assert.equal(keyboard.shown, 'List<i32>>|');
});

test('an edit before the placed `>` shifts it; an edit over it forgets it', () => {
    const keyboard = new Keyboard('', 0, afterATypeName).type('List<');
    const placed = keyboard.closers.offsets(DOCUMENT)[0];
    keyboard.closers.track(DOCUMENT, [{ rangeOffset: 0, rangeLength: 0, text: 'let x: ' }]);
    assert.deepEqual(keyboard.closers.offsets(DOCUMENT), [placed + 7]);
    keyboard.closers.track(DOCUMENT, [{ rangeOffset: placed + 7, rangeLength: 1, text: '' }]);
    assert.deepEqual(keyboard.closers.offsets(DOCUMENT), []);
});

test('the caret leaving the line forgets the placed `>`', () => {
    const closers = new PlacedClosers();
    closers.place(DOCUMENT, 5);
    closers.place(DOCUMENT, 30);
    const lineOf = (offset: number) => (offset < 20 ? 0 : 1);
    closers.keepLine(DOCUMENT, 1, lineOf);
    assert.deepEqual(closers.offsets(DOCUMENT), [30]);
    closers.keepLine(DOCUMENT, 0, lineOf);
    assert.deepEqual(closers.offsets(DOCUMENT), []);
});

test('a placed `>` that is no longer a `>` is spent without a step', () => {
    const closers = new PlacedClosers();
    closers.place(DOCUMENT, 3);
    assert.equal(closers.typeOver(DOCUMENT, 3, ')'), false);
    assert.deepEqual(closers.offsets(DOCUMENT), []);
});
