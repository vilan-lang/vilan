//! The analyzed program, read against the texts it is queried in — and the
//! navigation primitives every editor query is built on
//! (`proposal/playground-completion.md` §3).
//!
//! [`Analysis`] is a struct of references, built per query by whoever owns
//! the analysis: the language server from its `Document`, the playground
//! from the handle it retains between compiles. The primitives here are the
//! ones completion shares with hover and go-to-definition — the chain walkers
//! that resolve a use to its declaration (`function_target`, `hover_label`),
//! the declaration's name span and `///` doc, the entity under an offset —
//! measured as the whole overlap (nine functions) when completion was lifted
//! out of the server. Everything hover-shaped above them stays in the server.

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

    /// The contiguous `//` block directly above a declaration's name line —
    /// its doc comment, with the comment markers stripped. Attribute lines
    /// (`[must_use]`, `[platform(…)]`) between the block and the name are
    /// skipped. The entry file reads the ANALYZED text (`name_span` is a
    /// program span, so it slices the text the analysis consumed); other
    /// sources read from disk on demand (hover-time, cheap).
    pub fn doc_comment_of(&self, declaration_id: Id) -> Option<String> {
        let program = self.program;
        let source = program.source_of(declaration_id)?;
        let name_span = self.definition_name_span(declaration_id)?;
        let owned;
        let text: &str = if source == SourceId(0) {
            self.analyzed.text()
        } else {
            let path = program.source_path(source)?;
            // BOM-stripped so `name_span` (from the analyzer's read) slices
            // this text at the right offset (windows-support.md §2).
            owned = vilan_core::util::read_source(path).ok()?;
            &owned
        };
        let start = name_span.into_range().start.min(text.len());
        let head = &text[..start];
        let mut lines: Vec<&str> = head.lines().collect();
        // Drop the (partial) declaration line itself.
        lines.pop();
        // Skip attribute and modifier-only lines between docs and the name.
        while let Some(last) = lines.last() {
            let trimmed = last.trim();
            if trimmed.starts_with('[') || trimmed == "async" || trimmed == "external" {
                lines.pop();
            } else {
                break;
            }
        }
        // `///` is the doc-comment syntax (user decision, 2026-07-16); a
        // plain `//` block is an implementation note and never surfaces.
        let mut docs: Vec<String> = Vec::new();
        while let Some(last) = lines.last() {
            let trimmed = last.trim();
            let Some(comment) = trimmed.strip_prefix("///") else {
                break;
            };
            docs.push(comment.strip_prefix(' ').unwrap_or(comment).to_string());
            lines.pop();
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
