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
    if program.lints.internal_use == LintLevel::Warn {
        warn_internal_uses(program);
    }
}

fn refuse_local_labels(program: &mut Program) {
    let module_bindings: HashSet<Id> = program.module_level_bindings().into_iter().collect();
    let mut refused: Vec<(Span, SourceId, String)> = program
        .item_labels
        .keys()
        .filter(|id| !module_bindings.contains(id))
        .filter_map(|id| {
            let variable = program.variables.get(id)?;
            Some((
                variable.name_span,
                program.diagnostic_source_of(*id),
                format!(
                    "`{}` is a local binding, and `[internal(..)]` labels an item on a module's \
                     surface: nothing outside this body can name it, so the label has no reader \
                     — delete it",
                    variable.name
                ),
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
