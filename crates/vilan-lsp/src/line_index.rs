//! The language server's view of the shared line index: the same
//! byte-offset ↔ line/character conversion as `vilan_ide::LineIndex` (the ONE
//! implementation, K9), speaking `lsp_types::Position`/`Range` at this edge.
//! Nothing here converts; the newtype exists so the handlers' twelve call
//! sites keep reading `index.position(offset)` and get the wire type.

use tower_lsp::lsp_types::{Position, Range};
use vilan_core::Span;

pub struct LineIndex(vilan_ide::LineIndex);

fn to_lsp(position: vilan_ide::Position) -> Position {
    Position {
        line: position.line,
        character: position.character,
    }
}

fn from_lsp(position: Position) -> vilan_ide::Position {
    vilan_ide::Position {
        line: position.line,
        character: position.character,
    }
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        LineIndex(vilan_ide::LineIndex::new(text))
    }

    /// The shared index itself, for the queries `vilan_ide` answers over it.
    pub fn shared(&self) -> &vilan_ide::LineIndex {
        &self.0
    }

    /// The LSP position for a byte offset (clamped, never panicking — see the
    /// shared index's pins).
    pub fn position(&self, offset: usize) -> Position {
        to_lsp(self.0.position(offset))
    }

    /// The document's source text (for completion's backward scan over the
    /// characters preceding the cursor).
    pub fn text(&self) -> &str {
        self.0.text()
    }

    /// The byte offset for an LSP position.
    pub fn offset(&self, position: Position) -> usize {
        self.0.offset(from_lsp(position))
    }

    /// The LSP range for a span.
    pub fn range(&self, span: &Span) -> Range {
        let (start, end) = self.0.range(span);
        Range {
            start: to_lsp(start),
            end: to_lsp(end),
        }
    }
}
