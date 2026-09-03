//! The editor-facing queries the language server and the playground share
//! (`proposal/playground-completion.md`): a line index, the completion engine,
//! and the navigation primitives it reads.
//!
//! Nothing here is a protocol. The language server maps [`Position`] to
//! `lsp_types::Position` and a [`Completion`] to a `CompletionItem` at its own
//! edge; the playground maps the same values into a `wasm_bindgen` struct at
//! its. What the two must answer identically — which members a receiver has,
//! what an import path reaches, how a call-shaped insertion is spelled — is
//! computed once, here, over an [`Analysis`]: the analyzed `Program` together
//! with the text it was analyzed from, the text being edited (the two part
//! company mid-keystroke — E52), and the per-document tables the queries need.
//!
//! The crate depends on `vilan-core` alone, so it builds for
//! `wasm32-unknown-unknown` exactly as it does natively.

pub mod analysis;
pub mod completion;
pub mod line_index;

pub use analysis::{Analysis, entity_spans, signature_label, source_call_subject, span_of};
pub use completion::{
    AUTO_IMPORT_COMPLETION_CAP, AutoImport, BOOK_BASE, CONSTRUCT_SNIPPETS, Completion,
    CompletionFunctionCall, CompletionIndex, CompletionKind, ImportRoots, InsertText, KEYWORD_DOCS,
    SnippetInsertion, call_insertion, keyword_lexeme,
};
pub use line_index::{LineIndex, Position};
