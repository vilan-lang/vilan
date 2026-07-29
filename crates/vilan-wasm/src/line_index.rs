//! Converts Vilan's byte-offset spans into line/character positions, where a
//! character is a UTF-16 code unit — the unit both LSP and the browser's own
//! text APIs count in.
//!
//! A near-copy of the language server's `line_index.rs`. It is duplicated
//! rather than shared because the original returns `lsp_types::Position`, and
//! moving it into core would mean either dragging `lsp_types` along or
//! rewriting the LSP's twelve call sites for a type change this crate does not
//! need. The algorithm is ~40 lines and settled; the drift risk is low and the
//! coupling cost is not. **Keep the two in step** — in particular the two
//! behaviours pinned below, which exist because a malformed span must degrade,
//! never abort.

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
    /// reachable from a malformed span, and in the playground a panic would
    /// take down the whole wasm instance rather than one request.
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

    /// The start and end positions of a span.
    pub fn range(&self, span: &Span) -> (Position, Position) {
        let range = span.into_range();
        (self.position(range.start), self.position(range.end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(start: usize, end: usize) -> Span {
        Span::new((), start..end)
    }

    #[test]
    fn a_position_counts_lines_and_characters_from_zero() {
        let index = LineIndex::new("one\ntwo\nthree\n");
        assert_eq!(
            index.position(0),
            Position {
                line: 0,
                character: 0
            }
        );
        assert_eq!(
            index.position(4),
            Position {
                line: 1,
                character: 0
            }
        );
        assert_eq!(
            index.position(6),
            Position {
                line: 1,
                character: 2
            }
        );
    }

    #[test]
    fn a_character_offset_counts_utf16_units_not_bytes() {
        // An emoji is 4 UTF-8 bytes and 2 UTF-16 units; `é` is 2 and 1.
        let index = LineIndex::new("é🎈x");
        assert_eq!(
            index.position(2),
            Position {
                line: 0,
                character: 1
            }
        );
        assert_eq!(
            index.position(6),
            Position {
                line: 0,
                character: 3
            }
        );
    }

    /// Pinned because a malformed span is reachable and must not abort the
    /// instance. An offset *inside* a multi-byte character resolves to the
    /// boundary AFTER it — the loop counts the character it lands in as
    /// consumed — rather than panicking the way a `text[..offset]` slice would.
    /// Landing after is arbitrary but harmless: the offset was not a real
    /// boundary, so no position is right, and only not-crashing matters.
    #[test]
    fn an_offset_inside_a_multibyte_character_does_not_panic() {
        let index = LineIndex::new("é🎈x");
        // Inside `é` (bytes 0..2) — resolves past it, to 1.
        assert_eq!(
            index.position(1),
            Position {
                line: 0,
                character: 1
            }
        );
        // Inside `🎈` (bytes 2..6, two UTF-16 units) — resolves past it, to 3.
        assert_eq!(
            index.position(3),
            Position {
                line: 0,
                character: 3
            }
        );
    }

    /// Likewise for an offset past the end — it clamps to the last position.
    #[test]
    fn an_out_of_range_offset_clamps() {
        let index = LineIndex::new("ab\n");
        assert_eq!(
            index.position(9_999),
            Position {
                line: 1,
                character: 0
            }
        );
    }

    #[test]
    fn a_range_converts_both_ends() {
        let index = LineIndex::new("one\ntwo\n");
        let (start, end) = index.range(&span(4, 7));
        assert_eq!(
            start,
            Position {
                line: 1,
                character: 0
            }
        );
        assert_eq!(
            end,
            Position {
                line: 1,
                character: 3
            }
        );
    }
}
