use crate::analyzer::SourceId;
use crate::span::Span;

/// A diagnostic's secondary location + label (diagnostics-standard.md C3):
/// "first call here", "the trait declares it here". One, not a list —
/// diagnostics stay terse. `source` names the note's file when it differs
/// from the primary span's (`None` = the same file); the CLI renders it as
/// an ariadne sub-label whose `file:line:col` sub-header and label derive
/// from one converted position (`char_range` in the CLI — E76), the
/// language server as related information.
#[derive(Debug, Clone)]
pub struct Note {
    pub span: Span,
    pub msg: String,
    pub source: Option<SourceId>,
}

impl Note {
    /// A note in the SAME file as the diagnostic's primary span.
    pub fn here(span: Span, msg: String) -> Self {
        Note {
            span,
            msg,
            source: None,
        }
    }
}

/// One entry of a requirement trace (backlog E78): the note it renders as,
/// plus whether it marks an uncovered CALL SITE. The distinction is the
/// editor's (E81): a call hop publishes as its own diagnostic at the call —
/// related information draws no underline — while the elision tail
/// annotates the last kept hop's span and only ever rides as a label, or
/// the same location would report twice.
#[derive(Debug, Clone)]
pub struct TraceHop {
    pub note: Note,
    pub call: bool,
}

#[derive(Debug, Clone)]
pub struct Error {
    pub span: Span,
    pub msg: String,
    pub note: Option<Note>,
    /// The requirement chain (backlog E78): one label per UNCOVERED
    /// user-written call between the diagnostic's anchor and the offending
    /// site, ordered entry → site. Distinct from the C3 `note` — that stays
    /// one location and keeps its "one, not a list" contract; the trace is a
    /// rust-analyzer-style chain and is empty for every diagnostic except the
    /// context-coverage refusals. The CLI renders each element as an ariadne
    /// sub-label; the language server renders each as related information
    /// (before the C3 note, preserving this vector's order) and each CALL
    /// hop additionally as its own diagnostic at the call (E81).
    pub trace: Vec<TraceHop>,
}
