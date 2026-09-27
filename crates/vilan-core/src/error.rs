use crate::analyzer::SourceId;
use crate::span::Span;

/// A diagnostic's secondary location + label (diagnostics-standard.md C3):
/// "first call here", "the trait declares it here". One, not a list —
/// diagnostics stay terse. `source` names the note's file when it differs
/// from the primary span's (`None` = the same file); the CLI renders it as
/// an ariadne sub-label whose `file:line:col` sub-header and label derive
/// from one converted position (`char_range` in the CLI — E76), the
/// language server as related information.
///
/// **"One, not a list" was re-examined against a real second note and KEPT**
/// (N112, decided 2026-09-21). E189's broad gate (B355) is the first place a
/// second footnote looked wanted: when a program already carries an error the
/// context family stands down whole, and the count of checks that did not run
/// has to reach the reader somehow. Beside the primary error it would be a
/// footnote about a DIFFERENT subject — the primary error is one wrong
/// expression, the deferral is a statement about the pass — and it would have
/// to be attached to whichever diagnostic happened to be last, which is not a
/// relation the reader can read. It ships as its own WARNING instead, and that
/// is the general answer: a second fact that is not a second LOCATION for THIS
/// error is its own diagnostic, not another line under this one.
///
/// Growing the field to `footnotes: Vec<Note>` is cheap here and expensive
/// everywhere it is read — the terminal renderer, the HMR overlay, the
/// language server's related information and the playground each gain a list
/// to order and to truncate, and the C3 terseness rule stops being enforced by
/// the type. [`Error::trace`] is the escape hatch that already exists for the
/// one shape that genuinely is a chain (E78's requirement trace), and it is
/// deliberately NOT this field.
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
