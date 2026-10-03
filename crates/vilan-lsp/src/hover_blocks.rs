//! E246, extended by the owner's ruling on editor-46's report: ONE canonical
//! order for the blocks a hover is made of, and one function that applies it.
//!
//! Every hover the server answers is assembled from typed blocks
//! ([`HoverBlocks::push`]) and rendered by [`HoverBlocks::render`], which
//! orders them by [`Block`]'s rank — the declaration order of its variants —
//! keeping the order blocks of one kind were pushed in. The ruled order:
//!
//! 1. **Diagnostic** — what is wrong, or must be known before use, AT this
//!    position: a `[deprecated]` steer, an `[internal]` reason (E213/E221).
//! 2. **Signature** — the type definition or signature: the fenced
//!    declaration, `name: Type`, a binding's `let x: T`, the member's block
//!    header, a generic call's resolved reading (E206).
//! 3. **SignatureNote** — a line that qualifies the signature and belongs
//!    with it, which the ruled list does not name and which is placed here,
//!    right under the signature: E227's `Shown as ~Source<T>` (what the inlay
//!    hint abbreviates the type to), E240's `A type parameter of fun pick<..>`.
//! 4. **Doc** — the `///` doc comment the author wrote for this name (a
//!    keyword's meaning and book link is its doc).
//! 5. **Preview** — reference material: the struct/enum/trait shape one level
//!    deep (E237), a namespace's members (E152). Set apart by a horizontal
//!    rule (E246): two fences back to back render as one run.
//! 6. **Platform** — entry and platform facts: the platform layer a function
//!    requires and through which call (`platform_color::requirements`).
//!
//! VS Code renders the editor's own DIAGNOSTICS for a range in its hover
//! already (its problems section, beside this one), so the server does not
//! repeat their messages here; what ranks first is what the program says
//! about the hovered declaration itself.

/// One hover block's kind, ranked: the declaration order IS the hover order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Block {
    Diagnostic,
    Signature,
    SignatureNote,
    Doc,
    Preview,
    Platform,
}

/// A hover under assembly: blocks pushed in any order, rendered in the
/// canonical one.
#[derive(Default, Debug)]
pub struct HoverBlocks {
    blocks: Vec<(Block, String)>,
}

impl HoverBlocks {
    pub fn new() -> HoverBlocks {
        HoverBlocks::default()
    }

    /// Add a block of `kind`. An empty text adds nothing.
    pub fn push(&mut self, kind: Block, text: impl Into<String>) -> &mut HoverBlocks {
        let text = text.into();
        if !text.is_empty() {
            self.blocks.push((kind, text));
        }
        self
    }

    /// [`HoverBlocks::push`] for an optional block.
    pub fn push_some(&mut self, kind: Block, text: Option<impl Into<String>>) -> &mut HoverBlocks {
        if let Some(text) = text {
            self.push(kind, text);
        }
        self
    }

    /// A fenced `vilan` block.
    pub fn push_code(&mut self, kind: Block, code: &str) -> &mut HoverBlocks {
        self.push(kind, format!("```vilan\n{code}\n```"))
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// The hover's markdown: the blocks in rank order (stable within a kind),
    /// a blank line between two blocks, and a horizontal rule above a
    /// [`Block::Preview`].
    pub fn render(mut self) -> String {
        self.blocks.sort_by_key(|(kind, _)| *kind);
        let mut out = String::new();
        for (index, (kind, text)) in self.blocks.iter().enumerate() {
            if index > 0 {
                out.push_str(if *kind == Block::Preview {
                    "\n\n---\n\n"
                } else {
                    "\n\n"
                });
            }
            out.push_str(text);
        }
        out
    }

    /// [`HoverBlocks::render`], or `None` for a hover with no block at all.
    pub fn rendered(self) -> Option<String> {
        (!self.is_empty()).then(|| self.render())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ruled order, whatever order the blocks are pushed in.
    #[test]
    fn blocks_render_in_the_canonical_order_whatever_order_they_arrive_in() {
        let mut hover = HoverBlocks::new();
        hover
            .push(Block::Platform, "platform")
            .push(Block::Preview, "preview")
            .push(Block::Doc, "doc")
            .push(Block::SignatureNote, "note")
            .push(Block::Signature, "signature")
            .push(Block::Diagnostic, "diagnostic");
        assert_eq!(
            hover.render(),
            "diagnostic\n\nsignature\n\nnote\n\ndoc\n\n---\n\npreview\n\nplatform"
        );
    }

    /// Within a kind the pushed order stands (a deprecated steer before an
    /// internal reason), and a preview with nothing above it has no rule.
    #[test]
    fn a_kind_keeps_its_own_order_and_a_lone_preview_has_no_rule() {
        let mut hover = HoverBlocks::new();
        hover
            .push(Block::Diagnostic, "first")
            .push(Block::Diagnostic, "second");
        assert_eq!(hover.render(), "first\n\nsecond");
        let mut preview = HoverBlocks::new();
        preview.push(Block::Preview, "shape");
        assert_eq!(preview.render(), "shape");
        assert!(HoverBlocks::new().rendered().is_none());
    }
}
