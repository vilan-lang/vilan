//! Converts between Vilan's byte-offset spans and line/character positions,
//! where a character is a UTF-16 code unit — the unit LSP, CodeMirror, and
//! the browser's own text APIs all count in.
//!
//! The ONE implementation (K9, `proposal/playground-completion.md` §4): the
//! language server wraps it to speak `lsp_types::Position`, the playground
//! uses it as is. Two behaviours are pinned below because a malformed span
//! must degrade, never abort — in the language server a panic here used to
//! take the server down, and in the playground it would take the whole wasm
//! instance rather than one request.

use vilan_core::Span;

/// A zero-based line and a UTF-16 character offset within it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

pub struct LineIndex {
    /// Byte offset at which each line begins (line 0 starts at 0).
    line_starts: Vec<usize>,
    text: String,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![0];
        for (offset, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(offset + 1);
            }
        }
        LineIndex {
            line_starts,
            text: text.to_string(),
        }
    }

    /// The position for a byte offset. An offset past the end CLAMPS, and an
    /// offset landing inside a multi-byte character does not panic — both are
    /// reachable from a malformed span.
    pub fn position(&self, offset: usize) -> Position {
        let offset = offset.min(self.text.len());
        let line = match self.line_starts.binary_search(&offset) {
            Ok(line) => line,
            Err(next) => next - 1,
        };
        let line_start = self.line_starts[line];
        // Count UTF-16 units from the line start by iterating characters. A
        // `text[line_start..offset]` slice would panic when `offset` falls
        // inside a multi-byte character; a line start is always on a boundary,
        // so the open-ended slice is safe.
        let mut character = 0usize;
        let mut byte = line_start;
        for c in self.text[line_start..].chars() {
            if byte >= offset {
                break;
            }
            character += c.len_utf16();
            byte += c.len_utf8();
        }
        Position {
            line: line as u32,
            character: character as u32,
        }
    }

    /// The byte offset for a position. A line past the end clamps to the end
    /// of the text; a character past the end of its line clamps to the line's
    /// end (the newline is never crossed).
    pub fn offset(&self, position: Position) -> usize {
        let line_start = self
            .line_starts
            .get(position.line as usize)
            .copied()
            .unwrap_or(self.text.len());
        let mut utf16 = 0usize;
        let mut offset = line_start;
        for c in self.text[line_start..].chars() {
            if utf16 >= position.character as usize || c == '\n' {
                break;
            }
            utf16 += c.len_utf16();
            offset += c.len_utf8();
        }
        offset
    }

    /// The start and end positions of a span.
    pub fn range(&self, span: &Span) -> (Position, Position) {
        let range = span.into_range();
        (self.position(range.start), self.position(range.end))
    }

    /// The indexed text (completion's backward scan over the characters
    /// preceding the cursor reads it).
    pub fn text(&self) -> &str {
        &self.text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(start: usize, end: usize) -> Span {
        Span::new((), start..end)
    }

    fn at(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    #[test]
    fn a_position_counts_lines_and_characters_from_zero() {
        let index = LineIndex::new("one\ntwo\nthree\n");
        assert_eq!(index.position(0), at(0, 0));
        assert_eq!(index.position(4), at(1, 0));
        assert_eq!(index.position(6), at(1, 2));
    }

    #[test]
    fn a_character_offset_counts_utf16_units_not_bytes() {
        // An emoji is 4 UTF-8 bytes and 2 UTF-16 units; `é` is 2 and 1.
        let index = LineIndex::new("é🎈x");
        assert_eq!(index.position(2), at(0, 1));
        assert_eq!(index.position(6), at(0, 3));
    }

    #[test]
    fn position_counts_utf16_units_at_char_boundaries() {
        // `—` (em-dash) is 3 bytes but 1 UTF-16 unit; `😀` is 4 bytes, 2 UTF-16 units.
        let index = LineIndex::new("// — 😀 x\n");
        assert_eq!(index.position(0).character, 0);
        assert_eq!(index.position(3).character, 3); // "// " = 3
        assert_eq!(index.position(6).character, 4); // "// —" = 4
        assert_eq!(index.position(7).character, 5); // "// — " = 5
        assert_eq!(index.position(11).character, 7); // + 😀 (2) = 7
    }

    /// Pinned because a malformed span is reachable and must not abort the
    /// process. An offset *inside* a multi-byte character resolves to the
    /// boundary AFTER it — the loop counts the character it lands in as
    /// consumed — rather than panicking the way a `text[..offset]` slice would.
    /// Landing after is arbitrary but harmless: the offset was not a real
    /// boundary, so no position is right, and only not-crashing matters.
    #[test]
    fn an_offset_inside_a_multibyte_character_does_not_panic() {
        let index = LineIndex::new("é🎈x");
        // Inside `é` (bytes 0..2) — resolves past it, to 1.
        assert_eq!(index.position(1), at(0, 1));
        // Inside `🎈` (bytes 2..6, two UTF-16 units) — resolves past it, to 3.
        assert_eq!(index.position(3), at(0, 3));
        // And the resolved positions are non-decreasing across the character.
        let text = "// plain — NO\n"; // `—` occupies bytes 9..12
        let index = LineIndex::new(text);
        let before = index.position(9).character;
        let mid = index.position(10).character;
        let after = index.position(12).character;
        assert!(before <= mid && mid <= after, "{before} {mid} {after}");
    }

    /// Likewise for an offset past the end — it clamps to the last position.
    #[test]
    fn an_out_of_range_offset_clamps() {
        let index = LineIndex::new("ab\n");
        assert_eq!(index.position(9_999), at(1, 0));
    }

    #[test]
    fn a_range_converts_both_ends() {
        let index = LineIndex::new("one\ntwo\n");
        let (start, end) = index.range(&span(4, 7));
        assert_eq!(start, at(1, 0));
        assert_eq!(end, at(1, 3));
    }

    #[test]
    fn an_offset_round_trips_a_position_in_utf16_units() {
        // Line 0 is `é🎈x` — 7 bytes, 4 UTF-16 units — so line 1 starts at
        // byte 8, and `(1, 1)` is the byte after its `a`.
        let index = LineIndex::new("é🎈x\nab\n");
        assert_eq!(index.offset(at(0, 3)), 6);
        assert_eq!(index.offset(at(1, 1)), 9);
        assert_eq!(index.position(index.offset(at(1, 2))), at(1, 2));
    }

    /// The inbound direction degrades too: a line past the end lands at the
    /// text's end, a character past its line's end stops at the newline.
    #[test]
    fn an_out_of_range_position_clamps_without_crossing_a_newline() {
        let index = LineIndex::new("ab\ncd\n");
        assert_eq!(index.offset(at(9, 0)), 6);
        assert_eq!(index.offset(at(0, 40)), 2);
    }
}
