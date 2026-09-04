//! The analyzed program, read against the texts it is queried in — and the
//! navigation primitives every editor query is built on
//! (`proposal/playground-completion.md` §3).
//!
//! [`Analysis`] is a struct of references (plus one per-query text cache,
//! [`Analysis::source_texts`]), built per query by whoever owns the
//! analysis: the language server from its `Document`, the playground
//! from the handle it retains between compiles. The primitives here are the
//! ones completion shares with hover and go-to-definition — the chain walkers
//! that resolve a use to its declaration (`function_target`, `hover_label`),
//! the declaration's name span and `///` doc, the entity under an offset —
//! measured as the whole overlap (nine functions) when completion was lifted
//! out of the server. Everything hover-shaped above them stays in the server.

use std::cell::{OnceCell, Ref, RefCell};

use vilan_core::analyzer::{Expr, SourceId};
use vilan_core::fx::FxHashMap as HashMap;
use vilan_core::id::Id;
use vilan_core::type_::{Type, TypeId};
use vilan_core::{Program, Span};

use crate::completion::ImportRoots;
use crate::line_index::LineIndex;

/// One analyzed document as the queries read it: the program, the text it
/// was analyzed from, the text being edited, and the per-document tables.
///
/// Two texts, because an editor query arrives mid-keystroke: the program's
/// spans and offsets all live in the ANALYZED text's coordinate space, while
/// completion's trigger scan (the `.`, the partial identifier) reads the LIVE
/// one. [`Analysis::to_analyzed_offset`] is the one conversion between them
/// (E52, `lsp-snapshot-consistency.md`). A front-end whose two texts are the
/// same passes the same index twice, and the conversion is the identity.
pub struct Analysis<'a, 'src> {
    /// The analyzed program.
    pub program: &'a Program<'src>,
    /// The text `program` was analyzed from — the coordinate space every
    /// program span and offset lives in.
    pub analyzed: &'a LineIndex,
    /// The text being edited: `analyzed` plus whatever has been typed since
    /// the analysis landed.
    pub live: &'a LineIndex,
    /// `(start, end, id)` for every entry-file entity with a real span —
    /// [`entity_spans`], computed once per analysis.
    pub entity_spans: &'a [(usize, usize, Id)],
    /// Per-function platform requirements
    /// (`vilan_core::platform_color::requirements`), computed once per
    /// analysis; [`Analysis::function_target`] reads which ids carry one.
    pub platform_requirements: &'a HashMap<Id, String>,
    /// What an `import`/`use` path in this document can reach — `None` when
    /// the analysis resolved no package tree (the language server's degraded
    /// internal-error document).
    pub import_roots: Option<&'a ImportRoots>,
    /// What completion may read that is a function of the ANALYSIS alone,
    /// derived once when it landed (M25, E121 §2.1.4): the auto-import
    /// candidate table and the origins' module listings. A request reads it;
    /// nothing in a request rebuilds it.
    pub index: &'a crate::completion::CompletionIndex,
    /// Non-entry source texts already materialized FOR THIS QUERY — the one
    /// owned field on this otherwise reference-only struct, so the cache
    /// lives exactly as long as the query does (E83). [`Analysis::doc_comment_of`]
    /// reads a non-entry declaration's `///` doc out of its module's text,
    /// and a bare scope position resolves docs for many candidates declared
    /// in the SAME std module; before this cache each candidate re-read (and
    /// re-cloned) that text through `util::read_source`
    /// (`proposals/proposal/playground-completion.md` §9). Per query and
    /// never global on purpose: an overlay edit must land in the next
    /// query's reads. A failed read is recorded (`None`) so it is not
    /// retried. Construct with `Default::default()`.
    pub source_texts: RefCell<HashMap<SourceId, Option<String>>>,
    /// The two-sided, line-aligned edit anchor between `analyzed` and `live`,
    /// as `(prefix, suffix)` byte counts — computed at most once per query, by
    /// [`Analysis::anchor`], and only where something asks. Construct with
    /// `Default::default()`.
    ///
    /// The server's keystroke path computes the same two numbers per request
    /// (`keystroke::Anchor`) and cannot hand them here: the engine is below it,
    /// and the playground has no keystroke path at all. What reads it is E131's
    /// gate — whether the ANALYZED text still describes the bytes at a given
    /// live offset — which is a question only the engine's own two texts can
    /// answer.
    pub anchor: OnceCell<(usize, usize)>,
}

/// `(start, end, id)` for every entry-file entity with a real span, for
/// [`entity_at`]'s innermost-containing lookup. Computed once per analysis by
/// both front-ends through this one function, so neither can drift.
pub fn entity_spans(program: &Program) -> Vec<(usize, usize, Id)> {
    let mut entity_spans = Vec::new();
    for (id, span) in &program.span_map {
        if program.source_of(*id) != Some(SourceId(0)) {
            continue;
        }
        // A synthesized `Expr::Void` (S3, editing-dx.md §3.9: the parser's
        // filler for a block with no trailing expression, now spanning the
        // closing brace instead of a zero-width point past it) is not
        // something the user wrote and has no meaningful hover — excluded
        // here so a cursor on the brace still finds the next-smallest REAL
        // entity around it (the enclosing function, as before this span
        // widened from zero).
        if matches!(program.entity_map.get(id), Some(Expr::Void)) {
            continue;
        }
        let range = span.into_range();
        if range.start < range.end {
            entity_spans.push((range.start, range.end, *id));
        }
    }
    entity_spans
}

/// The innermost entry-file entity whose span contains `offset`, over an
/// [`entity_spans`] table.
pub fn entity_at(entity_spans: &[(usize, usize, Id)], offset: usize) -> Option<Id> {
    entity_spans
        .iter()
        .filter(|(start, end, _)| *start <= offset && offset < *end)
        .min_by_key(|(start, end, _)| end - start)
        .map(|(_, _, id)| *id)
}

/// The span of an entity, flattened from the `&Span` stored in `span_map`.
pub fn span_of(program: &Program, id: Id) -> Option<Span> {
    program.span_map.get(&id).map(|span| **span)
}

/// The subject to resolve a call through: the SOURCE subject. Where the
/// context lowering rewired the call record's subject — a covered
/// `get_safe`'s `Some`-wrap, `Context::run`'s body-closure call — the
/// erased original is read back from the pass's record (editing-dx.md
/// §19.3); every other call answers its wired subject. The erased subject
/// entity survives in `entity_map`, so a chain walk continues through it
/// normally and lands on the declaration the source names.
pub fn source_call_subject(program: &Program, call_id: Id) -> Option<Id> {
    let call = program.function_calls.get(&call_id)?;
    Some(
        program
            .context_erased_subjects
            .get(&call_id)
            .copied()
            .unwrap_or(call.subject_id),
    )
}

/// The pre-rendered signature of the function/external at `target` — the same
/// string hover fences — with the inferred `async` prepended, mirroring
/// [`Document::compose_hover`]. `target` is a function DEFINITION id (resolve a
/// use site through [`Document::function_target`] first). `None` when the id
/// names no declaration.
pub fn signature_label(program: &Program, target: Id) -> Option<String> {
    let declaration = program.declaration_labels.get(&target)?;
    if program.async_functions.contains(&target) && !declaration.starts_with("async ") {
        Some(format!("async {declaration}"))
    } else {
        Some(declaration.clone())
    }
}

/// The parameter names of the function/external at `target`, in order, with
/// `self` dropped (the receiver is not a call argument) — the tab-stop labels a
/// call-shaped completion fills. `Some(vec![])` for a zero-parameter callable;
/// `None` when the id is not a function or external. `target` is a DEFINITION
/// id (resolve through [`Document::function_target`] first).
pub fn call_parameter_names(program: &Program, target: Id) -> Option<Vec<String>> {
    let parameter_ids = if let Some(function) = program.functions.get(&target) {
        &function.parameters
    } else if let Some(external) = program.external_functions.get(&target) {
        &external.parameters
    } else {
        return None;
    };
    Some(
        parameter_ids
            .iter()
            .filter_map(|parameter_id| program.parameters.get(parameter_id))
            .filter(|parameter| parameter.name != "self")
            .map(|parameter| parameter.name.to_string())
            .collect(),
    )
}

/// The start of the line containing `at` — `0`, or one past the nearest
/// preceding `\n`.
fn line_start(bytes: &[u8], at: usize) -> usize {
    let mut at = at.min(bytes.len());
    while at > 0 && bytes[at - 1] != b'\n' {
        at -= 1;
    }
    at
}

/// The start of the line AFTER the one containing `at` — the length when `at`
/// is on the last line.
fn next_line_start(bytes: &[u8], at: usize) -> usize {
    let mut at = at.min(bytes.len());
    if at > 0 && bytes[at - 1] == b'\n' {
        return at;
    }
    while at < bytes.len() {
        let byte = bytes[at];
        at += 1;
        if byte == b'\n' {
            return at;
        }
    }
    bytes.len()
}

/// The nominal struct/enum id a resolved type names, ignoring its type
/// arguments — the id member resolution is keyed by.
pub(crate) fn nominal_type_id(program: &Program, type_id: TypeId) -> Option<Id> {
    match program.type_id_to_type_map.get(&type_id)? {
        Type::Struct(id, _) | Type::Enum(id, _) => Some(*id),
        _ => None,
    }
}

/// The declared type of a `let`/`mut` binding or a parameter.
pub(crate) fn binding_type_id(program: &Program, binding: Id) -> Option<TypeId> {
    program
        .variables
        .get(&binding)
        .map(|variable| variable.type_id)
        .or_else(|| {
            program
                .parameters
                .get(&binding)
                .map(|parameter| parameter.type_id)
        })
}

impl<'a, 'src> Analysis<'a, 'src> {
    /// The ANALYZED-space offset a LIVE-space offset names: through the live
    /// index's line/character coordinates, then `analyzed_offset` — the same
    /// conversion every other query performs starting straight from the LSP
    /// `Position` (S1). `completion` is the one query whose dispatch is
    /// computed from the live text (the `.`/`?.`/`::` trigger scan legitimately
    /// reads it — the prefix being typed is live by nature), so its derived
    /// offsets (the dot, the receiver's end) still need translating before
    /// they touch `program` data: `scope_at`, `entity_at`, and anything built
    /// on them would otherwise resolve the wrong scope or entity the moment
    /// the two snapshots diverge (E52). Both indices clamp out-of-range input
    /// rather than panicking, so this is safe on any offset the live-text scan
    /// produces — including one past the end of a shorter analyzed text.
    pub(crate) fn to_analyzed_offset(&self, live_offset: usize) -> usize {
        self.analyzed.offset(self.live.position(live_offset))
    }

    /// The line-aligned common prefix and suffix of the analyzed text and the
    /// live buffer, in bytes — the same two-sided anchor E121's keystroke path
    /// computes, memoized per query.
    ///
    /// Both edges are trimmed to a line boundary, for the reason the server's
    /// anchor states: a `\n` cannot occur inside a UTF-8 sequence, so a line
    /// boundary is a char boundary in both texts and never cuts a token in
    /// half.
    fn anchor(&self) -> (usize, usize) {
        *self.anchor.get_or_init(|| {
            let analyzed = self.analyzed.text().as_bytes();
            let live = self.live.text().as_bytes();
            if analyzed == live {
                return (analyzed.len(), 0);
            }
            let common_prefix = analyzed
                .iter()
                .zip(live)
                .take_while(|(old, new)| old == new)
                .count();
            let prefix = line_start(analyzed, common_prefix);
            let common_suffix = analyzed
                .iter()
                .rev()
                .zip(live.iter().rev())
                .take_while(|(old, new)| old == new)
                .count();
            // Clamp before trimming so the two edges cannot overlap in EITHER
            // text — `"aa"` -> `"a"` shares a one-byte prefix and a one-byte
            // suffix that are the same byte.
            let room = analyzed.len().min(live.len()).saturating_sub(prefix);
            let common_suffix = common_suffix.min(room);
            let live_suffix_start = next_line_start(live, live.len() - common_suffix);
            (prefix, live.len() - live_suffix_start)
        })
    }

    /// Whether the ANALYZED text still describes the live bytes at
    /// `live_offset` — outside the edit window, where the two texts are
    /// byte-identical, so a `program` lookup keyed on a converted offset is
    /// answering about the same characters the user is looking at.
    ///
    /// False inside the window, and the cursor's OWN line is inside it by
    /// construction: the anchor is line-aligned, and the line being typed on is
    /// the line that differs. That is why a converted offset cannot be trusted
    /// there — [`Analysis::to_analyzed_offset`] is a line/character round-trip
    /// that CLAMPS, so a live column past the end of the shorter analyzed line
    /// lands on that line's last character and `entity_at` answers about
    /// whatever expression used to be written there (E131).
    pub(crate) fn analyzed_agrees_at(&self, live_offset: usize) -> bool {
        let (prefix, suffix) = self.anchor();
        let live_len = self.live.text().len();
        live_offset < prefix || live_offset >= live_len.saturating_sub(suffix)
    }

    /// An ANALYZED span in LIVE coordinates, or `None` when it has no image —
    /// the engine's own form of `keystroke::Anchor::map_span`, for the answers
    /// that are computed against the analyzed text and delivered as edits to
    /// the live buffer (M29's captured import edits).
    ///
    /// A span maps only when it lies ENTIRELY inside one anchor: one that
    /// straddles the edit window is dropped rather than clamped, because half
    /// of it describes bytes that are gone. That is what keeps this a
    /// re-mapping instead of a guess.
    pub(crate) fn map_analyzed_span(&self, span: Span) -> Option<Span> {
        let (prefix, suffix) = self.anchor();
        // `prefix > 0` is not redundant, and the arm below it is why. An import
        // edit is a zero-width INSERTION POINT, and `0..0` satisfies
        // `end <= prefix` vacuously when there is no common prefix at all —
        // the shape of an edit at the top of a file whose FIRST line the user
        // just changed. Where the suffix reaches back to offset 0 (a line
        // inserted above the imports) the point belongs to the suffix and moves
        // with the text it precedes; where it does not (a file with no imports
        // at all, so the edit is "a new first line"), offset 0 is offset 0 in
        // both texts and the identity is right. Taking the head arm first would
        // answer the identity for both.
        if prefix > 0 && span.end <= prefix {
            return Some(span);
        }
        let analyzed_len = self.analyzed.text().len();
        let live_len = self.live.text().len();
        if span.start >= analyzed_len.saturating_sub(suffix) {
            let shift = live_len as i64 - analyzed_len as i64;
            let start = span.start as i64 + shift;
            let end = span.end as i64 + shift;
            if start < 0 || end < start || end as usize > live_len {
                return None;
            }
            return Some(Span {
                start: start as usize,
                end: end as usize,
            });
        }
        // The file's very first byte: an insertion point there names the same
        // place in both texts however the rest of them differ.
        if span.start == 0 && span.end == 0 {
            return Some(span);
        }
        None
    }

    /// The innermost entry-file entity whose span contains `offset`.
    pub fn entity_at(&self, offset: usize) -> Option<Id> {
        entity_at(self.entity_spans, offset)
    }

    /// The span to jump to for a definition id: the declaration's *name* for a
    /// type/function/variable (else its whole span, e.g. a module's file start).
    pub fn definition_name_span(&self, id: Id) -> Option<Span> {
        let program = self.program;
        if let Some(structure) = program.structs.get(&id) {
            return Some(structure.name_span);
        }
        if let Some(enumeration) = program.enums.get(&id) {
            return Some(enumeration.name_span);
        }
        if let Some(trait_definition) = program.traits.get(&id) {
            return Some(trait_definition.name_span);
        }
        if let Some(function) = program.functions.get(&id) {
            return Some(function.name_span);
        }
        if let Some(function) = program.external_functions.get(&id) {
            return Some(function.name_span);
        }
        if let Some(variable) = program.variables.get(&id) {
            return Some(variable.name_span);
        }
        span_of(program, id)
    }

    /// The text of a non-entry source, materialized AT MOST ONCE per query
    /// (E83): the cache is [`Analysis::source_texts`], whose doc carries the
    /// why. BOM-stripped by `read_source`, so a program span (from the
    /// analyzer's own read) slices this text at the right offset
    /// (windows-support.md §2) — and answered from the open-document overlay
    /// before disk, like every other source read.
    fn source_text(&self, source: SourceId) -> Option<Ref<'_, str>> {
        {
            let mut texts = self.source_texts.borrow_mut();
            texts.entry(source).or_insert_with(|| {
                self.program
                    .source_path(source)
                    .and_then(|path| vilan_core::util::read_source(path).ok())
            });
        }
        Ref::filter_map(self.source_texts.borrow(), |texts| {
            texts.get(&source).and_then(|text| text.as_deref())
        })
        .ok()
    }

    /// The contiguous `//` block directly above a declaration's name line —
    /// its doc comment, with the comment markers stripped. Attribute lines
    /// (`[must_use]`, `[platform(…)]`) between the block and the name are
    /// skipped. The entry file reads the ANALYZED text (`name_span` is a
    /// program span, so it slices the text the analysis consumed); other
    /// sources read on demand through the per-query cache
    /// ([`Self::source_text`] — once per module per query, not once per
    /// candidate, E83).
    pub fn doc_comment_of(&self, declaration_id: Id) -> Option<String> {
        let program = self.program;
        let source = program.source_of(declaration_id)?;
        let name_span = self.definition_name_span(declaration_id)?;
        let owned;
        let text: &str = if source == SourceId(0) {
            self.analyzed.text()
        } else {
            owned = self.source_text(source)?;
            &owned
        };
        let start = name_span.into_range().start.min(text.len());
        let head = &text[..start];
        // The lines above the declaration, read BACKWARDS from it — never
        // collected (M29). `head.lines().collect()` was O(the whole prefix),
        // and a candidate declared deep in a large module made a completion
        // that offers it cost the module's length: on E121's 1,791-function
        // exhibit that is seven thousand line slices built to look at three.
        // Only the last few lines are ever read, and the loops below already
        // stop at the first line that is not one of them.
        //
        // `str::lines()` treats a final newline as a TERMINATOR rather than a
        // separator, so the one trailing `\n` is stripped before the walk and
        // the sequence this yields is that iterator's, reversed, line for line.
        let mut remaining = head.strip_suffix('\n').unwrap_or(head);
        let mut previous_line = move || {
            if remaining.is_empty() {
                return None;
            }
            Some(match remaining.rfind('\n') {
                Some(at) => {
                    let line = &remaining[at + 1..];
                    remaining = &remaining[..at];
                    line
                }
                None => std::mem::take(&mut remaining),
            })
        };
        // Drop the (partial) declaration line itself.
        previous_line();
        let mut line = previous_line();
        // Skip attribute and modifier-only lines between docs and the name.
        while let Some(current) = line {
            let trimmed = current.trim();
            if trimmed.starts_with('[') || trimmed == "async" || trimmed == "external" {
                line = previous_line();
            } else {
                break;
            }
        }
        // `///` is the doc-comment syntax (user decision, 2026-07-16); a
        // plain `//` block is an implementation note and never surfaces.
        let mut docs: Vec<String> = Vec::new();
        while let Some(current) = line {
            let trimmed = current.trim();
            let Some(comment) = trimmed.strip_prefix("///") else {
                break;
            };
            docs.push(comment.strip_prefix(' ').unwrap_or(comment).to_string());
            line = previous_line();
        }
        if docs.is_empty() {
            return None;
        }
        docs.reverse();
        Some(docs.join("\n"))
    }

    /// The first paragraph of a declaration's `///` doc — up to the first blank
    /// line — for a completion item's brief documentation (WO-3). `None` when
    /// there is no doc.
    pub fn doc_first_paragraph(&self, declaration_id: Id) -> Option<String> {
        let docs = self.doc_comment_of(declaration_id)?;
        let paragraph = docs.split("\n\n").next().unwrap_or(&docs).trim();
        if paragraph.is_empty() {
            None
        } else {
            Some(paragraph.to_string())
        }
    }

    /// The requirement-carrying entity the cursor *names*, if any: a function
    /// declaration name, a binding that resolves to a function or to a
    /// module-level binding with a requirement (its initializer is code), or
    /// a call's callee (including method calls, whose wired subject is a
    /// `Local` pointing at the resolved method). Deliberately strict — a
    /// local holding a function's *result* names nothing; only ids the
    /// requirements map actually knows can surface a line.
    pub fn function_target(&self, id: Id) -> Option<Id> {
        let program = self.program;
        let carries_requirement = |id: &Id| {
            program.functions.contains_key(id)
                || program.external_functions.contains_key(id)
                || self.platform_requirements.contains_key(id)
        };
        // Iterative through the call → subject chain, with a seen-list: the
        // chain is `entity_map`/`function_calls` data, which a lowering can
        // (and E73 showed does) rewire into shapes the analyzer never emits —
        // a cycle here must answer `None`, not recurse off the stack.
        let mut seen: Vec<Id> = Vec::new();
        let mut current = id;
        loop {
            if carries_requirement(&current) {
                return Some(current);
            }
            if seen.contains(&current) {
                return None;
            }
            seen.push(current);
            match program.entity_map.get(&current) {
                Some(Expr::Local(binding) | Expr::Variable(binding) | Expr::Parameter(binding)) => {
                    if carries_requirement(binding) {
                        return Some(*binding);
                    }
                }
                Some(Expr::Function(function_id) | Expr::ExternalFunction(function_id)) => {
                    return Some(*function_id);
                }
                Some(Expr::Call(call_id)) => {
                    // Through the SOURCE subject: where the context pass
                    // rewired the call record itself (a covered `get_safe`,
                    // `Context::run`), the erased original is recorded and
                    // still names the source callee (E75).
                    current = source_call_subject(program, *call_id)?;
                    continue;
                }
                _ => {}
            }
            // A call a lowering rewrote (E72): the context pass replaces an
            // unprovided `get_safe()`'s entity record with the lowered form —
            // a read of its hidden parameter, a `None` literal, an opaque
            // `Null` for `Context::new()` — but the call record and its wired
            // subject survive. Resolving through them is what lets the method
            // name hover as the declaration the source names.
            if let Some(subject) = source_call_subject(program, current) {
                current = subject;
                continue;
            }
            return None;
        }
    }

    pub fn hover_label(&self, id: Id) -> Option<String> {
        let program = self.program;
        // The id → binding chain is data (`entity_map`), and a binding that
        // never got a type can sit on a self-loop — the context-threading
        // pass's hidden parameters self-describe as `Expr::Parameter(itself)`
        // with no `expr_types` entry — or, in principle, a longer cycle. The
        // walk carries a seen-list and answers an honest `None` when the
        // chain closes on itself, instead of recursing off the stack (E73:
        // the recursive form crashed the whole server on such a hover).
        let mut seen: Vec<Id> = Vec::new();
        let mut current = id;
        loop {
            // A hidden context parameter is compiler-minted and deliberately
            // record-less — not source, so no label is the honest answer.
            // Checked explicitly against the pass's marker (E75) rather than
            // left to the self-loop happening to meet the cycle guard.
            if program.context_hidden_parameters.contains_key(&current) {
                return None;
            }
            if let Some(label) = program.expr_types.get(&current) {
                return Some(label.clone());
            }
            if seen.contains(&current) {
                return None;
            }
            seen.push(current);
            // A bare use carries no type on its own id; resolve through its
            // binding (and through that binding's own kind, e.g. an imported
            // enum variant).
            match program.entity_map.get(&current)? {
                Expr::Local(binding) | Expr::Variable(binding) | Expr::Parameter(binding) => {
                    current = *binding;
                }
                Expr::EnumVariant(enum_id, _) => {
                    return program
                        .enums
                        .get(enum_id)
                        .map(|e| format!("enum {}", e.name));
                }
                // A constructor / call: hover the thing being called (e.g.
                // `Ok(x)` shows the enum) when the call's own result type
                // isn't recorded — through the SOURCE subject where the
                // context pass rewired the call record (E75).
                Expr::Call(call_id) => {
                    current = source_call_subject(program, *call_id)?;
                }
                _ => return None,
            }
        }
    }
}
