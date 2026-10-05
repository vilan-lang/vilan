//! Item labels — `[internal("reason")]` (E213, E221) — read in ONE place, and
//! the one post-pass that acts on them.
//!
//! A label is a fact about a DECLARATION that changes nothing it means to the
//! type system: the editor reads it (completion hides the name, semantic tokens
//! dim it, hover leads with the reason), and a package that asks for it gets a
//! warning at each use (`[lints] internal_use = "warn"`). The records the
//! label lives on differ by declaration kind — `Function` and
//! `ExternalFunction` carry their own (E213), a field and an enum variant carry
//! theirs on their records, and a struct, enum, trait or module binding carries
//! its labels in [`Program::item_labels`] — so every reader asks [`internal_of`]
//! rather than re-deriving which record to look in. Two readers asking two
//! records is how the editor and the lint would come to disagree about the same
//! name.

use crate::analyzer::{Expr, Program, SourceId};
use crate::error::Error;
use crate::fx::FxHashSet as HashSet;
use crate::id::Id;
use crate::manifest::LintLevel;
use crate::node::Labels;
use crate::span::Span;

/// The `[internal("reason")]` label on the declaration `target` names — a
/// function, method, external, struct, enum, trait, module binding or enum
/// variant — or `None` for the overwhelming majority that carry none.
pub fn internal_of<'src>(program: &Program<'src>, target: Id) -> Option<&'src str> {
    if let Some(function) = program.functions.get(&target) {
        return function.internal;
    }
    if let Some(external) = program.external_functions.get(&target) {
        return external.internal;
    }
    if let Some(Expr::EnumVariant(enum_id, index)) = program.entity_map.get(&target) {
        return program.enums.get(enum_id)?.variants.get(*index)?.internal;
    }
    program.item_labels.get(&target)?.internal
}

/// The `[deprecated("use …")]` steer on the declaration `target` names — a
/// function or external (their own records), or a struct, enum, trait or
/// module binding (B382, [`Program::item_labels`]) — or `None`.
pub fn deprecated_of<'src>(program: &Program<'src>, target: Id) -> Option<&'src str> {
    if let Some(function) = program.functions.get(&target) {
        return function.deprecated;
    }
    if let Some(external) = program.external_functions.get(&target) {
        return external.deprecated;
    }
    program.item_labels.get(&target)?.deprecated
}

/// The label at a MEMBER position (`.name`), keyed by the access: the field a
/// read resolved to, or the method a call selected — both already recorded, so
/// this asks the record rather than re-resolving the name.
pub fn member_internal<'src>(program: &Program<'src>, access: Id) -> Option<&'src str> {
    match program.entity_map.get(&access)? {
        Expr::Field(_, struct_id, index) => {
            program.structs.get(struct_id)?.fields.get(*index)?.internal
        }
        Expr::Local(target) => internal_of(program, *target),
        _ => None,
    }
}

/// A149 S4 (R-e): whether an `[internal("reason")]` FIELD is out of reach as a
/// member — declared in std (`declared` is a std source) and read from code
/// that is not (`access` is a source std does not own). The reason when it
/// is, `None` when the field is a member there.
///
/// ONE rule for the analyzer (which refuses the read, and lets a store
/// handle's field syntax see past the handle's own fields) and the editor
/// (which offers no such field after a `.`), so the two cannot disagree. An
/// access with no source — synthesized — is read as std's. A field a PACKAGE
/// labels stays a member everywhere, with `[lints] internal_use` to warn at
/// it: the ruled door is about std's machinery, not about every label.
pub fn internal_field_out_of_reach<'src>(
    internal: Option<&'src str>,
    declared: Option<SourceId>,
    access: Option<SourceId>,
    std_sources: &HashSet<SourceId>,
) -> Option<&'src str> {
    let declared_in_std = declared.is_some_and(|source| std_sources.contains(&source));
    let read_outside_std = access.is_some_and(|source| !std_sources.contains(&source));
    internal.filter(|_| declared_in_std && read_outside_std)
}

/// The DECLARATION a member access reaches — the struct a field lives on, or
/// the method a call selected — for the same-module test below.
fn member_declaration(program: &Program, access: Id) -> Option<Id> {
    match program.entity_map.get(&access)? {
        Expr::Field(_, struct_id, _) => Some(*struct_id),
        Expr::Local(target) => Some(*target),
        _ => None,
    }
}

/// The name a declaration is written with, for a message.
fn name_of<'src>(program: &Program<'src>, target: Id) -> Option<&'src str> {
    if let Some(function) = program.functions.get(&target) {
        return Some(function.name);
    }
    if let Some(external) = program.external_functions.get(&target) {
        return Some(external.name);
    }
    if let Some(structure) = program.structs.get(&target) {
        return Some(structure.name);
    }
    if let Some(enumeration) = program.enums.get(&target) {
        return Some(enumeration.name);
    }
    if let Some(trait_) = program.traits.get(&target) {
        return Some(trait_.name);
    }
    if let Some(variable) = program.variables.get(&target) {
        return Some(variable.name);
    }
    if let Some(Expr::EnumVariant(enum_id, index)) = program.entity_map.get(&target) {
        return Some(program.enums.get(enum_id)?.variants.get(*index)?.name);
    }
    None
}

/// The member name at an access, for a message.
fn member_name_of<'src>(program: &Program<'src>, access: Id) -> Option<&'src str> {
    match program.entity_map.get(&access)? {
        Expr::Field(_, struct_id, index) => {
            Some(program.structs.get(struct_id)?.fields.get(*index)?.name)
        }
        Expr::Local(target) => name_of(program, *target),
        _ => None,
    }
}

/// The post-pass (E221), run from [`crate::post_analysis_passes`] so both
/// pipelines — the language server's and the CLI's — carry it.
///
/// 1. **A label on a LOCAL binding is refused.** The parser reads a labelled
///    `let` wherever a statement may stand, because a module and a function
///    body share that production; only here is it known which bindings are the
///    module's. A local has no reader outside its own body, so a label on one
///    says nothing to anybody, and silently keeping it would teach that it does.
/// 2. **`[lints] internal_use = "warn"`** — a warning at every use of an
///    internal item in the package's own code, outside the module that declares
///    it: the declaring module is where the item's reason is being honoured, not
///    ignored, and std's own uses (and a dependency's) are its authors' to
///    judge. Off unless the entry package's manifest asks.
pub fn check(program: &mut Program) {
    refuse_local_labels(program);
    refuse_impl_labels(program);
    refuse_misplaced_hints(program);
    warn_deprecated_uses(program);
    if program.lints.internal_use == LintLevel::Warn {
        warn_internal_uses(program);
    }
}

/// E224 (R-j): whether `span` in `source` sits on an `import`/`use` line.
///
/// ONE rule for every labelled item, types and functions alike: **an import
/// line alone does not warn; the USE warns.** An import names what it binds —
/// the loader resolves it, and the analyzer records a row for the leaf so the
/// editor can hover and rename it — but nothing has been used yet: the fix
/// sites are the uses, and a dead import falls out with the last one
/// (`proposal/deprecation.md`'s "what counts as a use"). A deprecated
/// RE-EXPORT is the one import that is itself the use — its name is
/// transparent at every other site — and it warns at the importing leaf from
/// the analyzer's own `check_deprecated_reexports`, not from here.
struct ImportLines(Vec<(SourceId, Span)>);

impl ImportLines {
    fn of(program: &Program) -> ImportLines {
        let mut spans = program.import_statement_spans.clone();
        spans.sort_by_key(|(source, span)| (source.0, span.start));
        ImportLines(spans)
    }

    fn contain(&self, source: SourceId, span: Span) -> bool {
        // The last statement starting at or before `span` in `source` is the
        // only one that can hold it: statements do not nest.
        let at = self.0.partition_point(|(other, statement)| {
            (other.0, statement.start) <= (source.0, span.start)
        });
        at > 0 && {
            let (other, statement) = self.0[at - 1];
            other == source && span.end <= statement.end
        }
    }
}

/// The refusal for labels on a LOCAL binding, naming the labels written
/// (B493): it used to say `[internal(..)]` whatever the label was, so a
/// `[platform(..)]` or a `[deprecated(..)]` on a local read as a refusal of a
/// label nobody wrote. Canonical order, as `vilan fmt` prints them.
fn local_label_refusal(name: &str, labels: &Labels<'_>) -> String {
    let mut written: Vec<&str> = Vec::new();
    if labels.deprecated.is_some() {
        written.push("`[deprecated(..)]`");
    }
    if labels.internal.is_some() {
        written.push("`[internal(..)]`");
    }
    if !labels.hint.is_empty() {
        written.push("`[hint(..)]`");
    }
    if !labels.platform.is_empty() {
        written.push("`[platform(..)]`");
    }
    let (labels, verb, subject, object) = match written.as_slice() {
        [one] => (one.to_string(), "labels", "the label has", "it"),
        [init @ .., last] => (
            format!("{} and {last}", init.join(", ")),
            "label",
            "the labels have",
            "them",
        ),
        [] => ("a label".to_string(), "labels", "the label has", "it"),
    };
    format!(
        "`{name}` is a local binding, and {labels} {verb} an item on a module's surface: nothing \
         outside this body can name it, so {subject} no reader — delete {object}"
    )
}

fn refuse_local_labels(program: &mut Program) {
    let module_bindings: HashSet<Id> = program.module_level_bindings().into_iter().collect();
    let mut refused: Vec<(Span, SourceId, String)> = program
        .item_labels
        .keys()
        .filter(|id| !module_bindings.contains(id))
        .filter_map(|id| {
            let variable = program.variables.get(id)?;
            let labels = program.item_labels.get(id)?;
            Some((
                variable.name_span,
                program.diagnostic_source_of(*id),
                local_label_refusal(variable.name, labels),
            ))
        })
        .collect();
    refused.sort_by_key(|(span, source, _)| (source.0, span.start));
    for (span, source, msg) in refused {
        program.push_diagnostic(
            Error {
                trace: Vec::new(),
                note: None,
                span,
                msg,
            },
            source,
        );
    }
}

/// B382: a use of a `[deprecated("use …")]` struct, enum, trait or module
/// binding warns `` `{name}` is deprecated; {steer} `` — the function
/// attribute's warning, word for word. A function's own uses are warned where
/// they always were (the analyzer's `check_deprecated`); these are the
/// declarations B382 admitted the attribute on. Silent in std and a dependency
/// (their authors migrate their own callers, as a function's deprecation is
/// silent in std) and in the module that declares the item, whose own `impl`
/// blocks and constructors are not the uses the steer is for.
fn warn_deprecated_uses(program: &mut Program) {
    let lookup = program.source_lookup();
    let import_lines = ImportLines::of(program);
    let is_user = |source: SourceId| {
        program
            .source_layers
            .get(source.0 as usize)
            .is_some_and(|layer| layer.containing.is_none())
    };
    let steer_of = |target: Id| -> Option<(&str, &str)> {
        if program.functions.contains_key(&target)
            || program.external_functions.contains_key(&target)
        {
            return None;
        }
        Some((
            name_of(program, target)?,
            program.item_labels.get(&target)?.deprecated?,
        ))
    };
    let mut sites: Vec<(Span, SourceId, String)> = Vec::new();
    let mut seen: HashSet<(u32, usize, usize)> = HashSet::default();
    let mut record = |span: Span, source: SourceId, target: Id| {
        if !is_user(source)
            || lookup.of(target) == Some(source)
            || import_lines.contain(source, span)
        {
            return;
        }
        let Some((name, steer)) = steer_of(target) else {
            return;
        };
        if seen.insert((source.0, span.start, span.end)) {
            sites.push((span, source, format!("`{name}` is deprecated; {steer}")));
        }
    };
    // Type position: an annotation, a bound, an impl subject (an import leaf
    // records a row here too, and `record` passes it by — E224).
    for (source, span, definition, _) in &program.type_references {
        if let Some(definition) = definition {
            record(*span, *source, *definition);
        }
    }
    // Value position: a struct literal's head, a binding read.
    for (id, expr) in &program.entity_map {
        let Expr::Local(target) = expr else {
            continue;
        };
        let Some(span) = program.span_map.get(id).map(|span| **span) else {
            continue;
        };
        let Some(source) = lookup.of(*id) else {
            continue;
        };
        if span.start < span.end {
            record(span, source, *target);
        }
    }
    sites.sort_by_key(|(span, source, _)| (source.0, span.start, span.end));
    for (span, source, msg) in sites {
        program.warnings.push(Error {
            trace: Vec::new(),
            note: None,
            span,
            msg,
        });
        program.warning_sources.push(source);
    }
}

/// An `impl` block takes `[platform(..)]` (F27 R1) through the same prefix a
/// type does, and so the prefix admits the other two there as well — where
/// they would label nothing: nobody NAMES an impl block, so there is no use to
/// steer or to hide. Refused, pointing at the members, which are what a
/// reader reaches for.
fn refuse_impl_labels(program: &mut Program) {
    let mut refused: Vec<(Span, SourceId)> = program
        .implementations
        .iter()
        .filter(|implementation| {
            program
                .item_labels
                .get(&implementation.impl_id)
                .is_some_and(|labels| labels.deprecated.is_some() || labels.internal.is_some())
        })
        .filter_map(|implementation| {
            let span = **program.span_map.get(&implementation.impl_id)?;
            Some((span, implementation.source))
        })
        .collect();
    refused.sort_by_key(|(span, source)| (source.0, span.start));
    refused.dedup();
    for (span, source) in refused {
        program.push_diagnostic(
            Error {
                trace: Vec::new(),
                note: None,
                span,
                msg: "`[deprecated(..)]` and `[internal(..)]` label a declaration a reader names, \
                      and nobody names an `impl` block — write the label on the members it \
                      is about"
                    .to_string(),
            },
            source,
        );
    }
}

/// E227: `[hint(Trait<..>)]` abbreviates a TYPE in an inlay hint, so it
/// labels a struct or an enum. The shared prefix admits it ahead of a trait,
/// a module `let` and an `impl` block too, where there is no type of the
/// declaration's own to abbreviate: refused, at the argument.
fn refuse_misplaced_hints(program: &mut Program) {
    let mut refused: Vec<(Span, SourceId, &'static str)> = program
        .item_labels
        .iter()
        .filter(|(id, _)| !program.structs.contains_key(*id) && !program.enums.contains_key(*id))
        .flat_map(|(id, labels)| {
            let kind = if program.traits.contains_key(id) {
                "a trait"
            } else if program.variables.contains_key(id) {
                "a module binding"
            } else {
                "an `impl` block"
            };
            let source = program.diagnostic_source_of(*id);
            labels.hint.iter().map(move |hint| (hint.0.1, source, kind))
        })
        .collect();
    refused.sort_by_key(|(span, source, _)| (source.0, span.start));
    for (span, source, kind) in refused {
        program.push_diagnostic(
            Error {
                trace: Vec::new(),
                note: None,
                span,
                msg: format!(
                    "`[hint(..)]` names the trait a struct or an enum is shown as in an inlay \
                     hint, and {kind} is not a type — write it on the struct or enum whose \
                     values the hint is about"
                ),
            },
            source,
        );
    }
}

fn warn_internal_uses(program: &mut Program) {
    for (span, source, msg) in internal_use_sites(program) {
        program.warnings.push(Error {
            trace: Vec::new(),
            note: None,
            span,
            msg,
        });
        program.warning_sources.push(source);
    }
}

/// Every use `[lints] internal_use` warns at, with its message, in a
/// deterministic order (the final sort is `normalize_diagnostic_order`'s).
fn internal_use_sites(program: &Program) -> Vec<(Span, SourceId, String)> {
    let lookup = program.source_lookup();
    let import_lines = ImportLines::of(program);
    // The package's own code: a source under no library root (std, a
    // dependency's layers and bases).
    let is_user = |source: SourceId| {
        program
            .source_layers
            .get(source.0 as usize)
            .is_some_and(|layer| layer.containing.is_none())
    };
    let mut sites: Vec<(Span, SourceId, String)> = Vec::new();
    let mut seen: HashSet<(u32, usize, usize)> = HashSet::default();
    let mut record = |span: Span, source: SourceId, name: &str, reason: &str| {
        // E224: an import line names what it binds; the use is what warns.
        if import_lines.contain(source, span) {
            return;
        }
        if seen.insert((source.0, span.start, span.end)) {
            sites.push((span, source, format!("`{name}` is internal: {reason}")));
        }
    };
    // A use is warned when it is in the package's own code and in a DIFFERENT
    // module from the declaration it reaches.
    let warned = |use_source: Option<SourceId>, declaration: Id| -> Option<SourceId> {
        let use_source = use_source?;
        (is_user(use_source) && lookup.of(declaration) != Some(use_source)).then_some(use_source)
    };
    // Names in expression position: a call's callee, a function passed as a
    // value, a binding read, a variant, a struct literal's head.
    for (id, expr) in &program.entity_map {
        let Expr::Local(target) = expr else {
            continue;
        };
        let Some(reason) = internal_of(program, *target) else {
            continue;
        };
        // A method call's subject is synthesized and spanless; its name is
        // at the member position, read below.
        let Some(span) = program.span_map.get(id).map(|span| **span) else {
            continue;
        };
        if span.start >= span.end {
            continue;
        }
        let Some(source) = warned(lookup.of(*id), *target) else {
            continue;
        };
        let Some(name) = name_of(program, *target) else {
            continue;
        };
        record(span, source, name, reason);
    }
    // Member positions: a field read, a method call.
    for (access, span) in &program.member_name_spans {
        let Some(reason) = member_internal(program, *access) else {
            continue;
        };
        let Some(declaration) = member_declaration(program, *access) else {
            continue;
        };
        let Some(source) = warned(lookup.of(*access), declaration) else {
            continue;
        };
        let Some(name) = member_name_of(program, *access) else {
            continue;
        };
        record(*span, source, name, reason);
    }
    // Type position: an annotation, a bound, an impl subject.
    for (source, span, definition, _) in &program.type_references {
        let Some(definition) = definition else {
            continue;
        };
        let Some(reason) = internal_of(program, *definition) else {
            continue;
        };
        if !is_user(*source) || lookup.of(*definition) == Some(*source) {
            continue;
        }
        let Some(name) = name_of(program, *definition) else {
            continue;
        };
        record(*span, *source, name, reason);
    }
    sites
}
