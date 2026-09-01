//! The reference index: one identifier-occurrence table per analyzed program.
//!
//! Find-references, rename and the organize-imports usage model all need the
//! same two answers — *what does this identifier name?* and *where else is that
//! named?* — and the class of bug this module exists to close (kolt.local 003,
//! 002) came from answering them with two different, ad-hoc mechanisms.
//!
//! The old shape resolved the cursor through `entity_at`, whose spans are whole
//! syntactic nodes, and then rebuilt each occurrence's span per target kind from
//! whichever analyzer table was nearest to hand. Both halves are unsound. A
//! whole-node span answers "which node contains the cursor", not "which
//! identifier is under it", so a coarse node shadowed the precise probes — a
//! cursor on a struct field's declaration resolved to the *struct*, and a cursor
//! on a return-type annotation resolved to the *enclosing function*. And
//! rebuilding spans per kind emitted whole-declaration spans where an identifier
//! was wanted (renaming a static method from its call site replaced the entire
//! `fun … { … }` declaration), whole-path spans (`Enum::Variant` rewritten
//! whole), and the same span twice from two tables (which is what made the
//! client reject a struct rename with "Rename failed to apply edits").
//!
//! So this module builds ONE table instead: every identifier occurrence in the
//! program, each a span covering exactly an identifier and the definition it
//! names. The cursor is resolved by *looking the offset up in that same table*,
//! which removes the ladder entirely — there is no coarse span left to shadow
//! anything, and a kind that can be found can necessarily also be enumerated,
//! because both directions read one set of rows.
//!
//! Two invariants carry the correctness, and both are pinned:
//!
//! 1. **Every span covers exactly an identifier.** A span is emitted only when
//!    its text is known to be the definition's name — either because the
//!    analyzer recorded the name span directly, or because the syntactic shape
//!    anchors the name at a known end of a longer span (see [`Anchor`]). A span
//!    that cannot be narrowed is dropped rather than emitted wrong, and the drop
//!    is *counted*, so an incomplete answer can be refused instead of silently
//!    returned.
//! 2. **No two rows share a span.** The analyzer records some references more
//!    than once (a struct's constructor name lands in both `type_references` and
//!    `struct_initializer_to_def`; a match pattern's segments are re-recorded on
//!    every type-check pass), so the table is deduplicated at build time. This
//!    is what makes a rename's edit set applicable.
//!
//! One table per PROGRAM, though — and a program reaches only its own import
//! closure, the files below its entry. So a symbol queried in the file that
//! DEFINES it could not see the files that import it (kolt.local 034, 003
//! branch (c)): in a multi-file app the declaration came back and nothing
//! else. The importers' programs had already indexed those occurrences; what
//! was missing was a definition identity that survives the program boundary.
//! [`DefinitionKey`] — the declaration's file plus its name span in that
//! file's text — is that identity: [`ReferenceIndex::key_of`] derives it,
//! [`ReferenceIndex::definition_of_key`] re-resolves it in another program's
//! index, and the document layer unions the per-program answers over every
//! open document (`Document::references_across`, with rename reading the same
//! union so the two cannot disagree about what a symbol's references are).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use vilan_core::Span;
use vilan_core::analyzer::{Expr, Program, SourceId};
use vilan_core::id::Id;

/// A definition that identifiers can name.
///
/// A struct field has no entity id of its own, so it is keyed by its owning
/// struct and its index — the same key `Expr::Field` carries.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Definition {
    /// Anything with an entity id: a local, a parameter, a function, a struct,
    /// an enum, a trait, a module, an enum variant.
    Entity(Id),
    /// A struct field, by owning struct id and field index.
    Field(Id, usize),
}

impl Definition {
    /// A total order over definitions, for a deterministic row order. `Id` is
    /// not `Ord` (it is an opaque entity handle), so order by its raw value.
    fn sort_key(self) -> (u32, usize) {
        match self {
            Definition::Entity(id) => (id.0, usize::MAX),
            Definition::Field(struct_id, index) => (struct_id.0, index),
        }
    }
}

/// A definition's identity ACROSS programs (kolt.local 034, 003 branch (c)).
///
/// Each open document analyzes its own program, so a [`Definition`]'s entity
/// ids are meaningless outside the program that minted them — two programs
/// that both loaded `library.vl` give `struct Point` two unrelated ids. What
/// every program agrees on is the declaration itself: the file it is written
/// in and the identifier's span in that file's text. That pair (plus the name,
/// so a program that read a DIFFERENT version of the file refuses to match
/// rather than linking two symbols that merely share an address) is the key
/// the cross-document union resolves through.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DefinitionKey {
    /// The canonical path of the file the definition is declared in
    /// (`Program::canonical_sources`, so every program spells it one way).
    path: PathBuf,
    /// The declaration name's span, in that file's text.
    span: Span,
    /// The declared name — a consistency check, not an address: a mismatch
    /// means the two programs analyzed different texts of `path`.
    name: String,
}

impl DefinitionKey {
    /// The canonical path of the declaring file — what
    /// [`crate::document::Document::depends_on`] scopes the union by.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// What kind of thing a definition is. Rename reports this in its refusals, and
/// uses it to decide what a rename is allowed to touch.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DefinitionKind {
    Binding,
    Field,
    Function,
    Struct,
    Enum,
    Variant,
    Trait,
    Module,
}

impl DefinitionKind {
    /// How this kind reads in a refusal message.
    pub fn noun(self) -> &'static str {
        match self {
            DefinitionKind::Binding => "binding",
            DefinitionKind::Field => "struct field",
            DefinitionKind::Function => "function",
            DefinitionKind::Struct => "struct",
            DefinitionKind::Enum => "enum",
            DefinitionKind::Variant => "enum variant",
            DefinitionKind::Trait => "trait",
            DefinitionKind::Module => "module",
        }
    }
}

/// One identifier occurrence: a span covering exactly an identifier, and the
/// definition that identifier names.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Occurrence {
    pub source: SourceId,
    pub span: Span,
    pub definition: Definition,
    /// Whether this occurrence is the definition's own declaration name.
    pub is_declaration: bool,
}

/// Where in a recorded span the identifier sits.
///
/// The analyzer records a use site's span in one of three shapes, and which one
/// applies is a property of the syntax, not a guess:
///
/// - [`Anchor::Exact`] — the span already covers just the identifier. Every span
///   the analyzer recorded through `record_reference`, `member_name_spans`, or a
///   declaration's own `name_span` is this shape.
/// - [`Anchor::End`] — a `::`-path (`Type::method`, `Enum::Variant`,
///   `module::thing`). The parser builds a static accessor left-nested and ends
///   the node at the member identifier's own end token, so the identifier is
///   exactly the final `name.len()` bytes. This is arithmetic on a guaranteed
///   shape, not a search.
/// - [`Anchor::Start`] — a construction whose first token is the identifier: a
///   struct initializer `Point { … }`, or a payload variant declaration
///   `Box2(i32, i32)` whose `span_map` entry covers the payload too. The name
///   leads, so it is exactly the first `name.len()` bytes. (The analyzer already
///   relies on this same arithmetic when it records a constructor's name.)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Anchor {
    Exact,
    End,
    Start,
}

/// Narrow a recorded span onto the identifier `name` sits at `anchor` within.
///
/// Returns `None` when the span cannot hold the name — which is the invariant's
/// escape hatch: a row that cannot be proven to cover an identifier is never
/// emitted, so a wrong span cannot reach a `WorkspaceEdit`.
fn narrow(span: Span, name: &str, anchor: Anchor) -> Option<Span> {
    let length = span.end.checked_sub(span.start)?;
    if name.is_empty() || length < name.len() {
        return None;
    }
    Some(match anchor {
        Anchor::Exact => {
            // A span that claims to be the identifier but is a different length
            // is not the identifier — refuse rather than trust it.
            if length != name.len() {
                return None;
            }
            span
        }
        Anchor::End => Span::from(span.end - name.len()..span.end),
        Anchor::Start => Span::from(span.start..span.start + name.len()),
    })
}

/// The reference index for one analyzed program.
#[derive(Default)]
pub struct ReferenceIndex {
    /// Every identifier occurrence, deduplicated and sorted by `(source, span)`
    /// so a positional lookup is a binary search and a rename's edit set comes
    /// out in a stable order.
    occurrences: Vec<Occurrence>,
    /// Row indices per definition.
    by_definition: HashMap<Definition, Vec<u32>>,
    /// How many use sites were dropped because their span could not be narrowed
    /// onto an identifier, per definition. A rename over a definition with a
    /// non-zero tally cannot produce a complete edit set and must refuse.
    dropped: HashMap<Definition, usize>,
}

impl ReferenceIndex {
    /// Build the index for `program`.
    pub fn build(program: &Program) -> Self {
        let mut rows: Vec<Occurrence> = Vec::new();
        let mut dropped: HashMap<Definition, usize> = HashMap::new();

        let push = |rows: &mut Vec<Occurrence>,
                    dropped: &mut HashMap<Definition, usize>,
                    source: Option<SourceId>,
                    span: Option<Span>,
                    name: &str,
                    anchor: Anchor,
                    definition: Definition,
                    is_declaration: bool| {
            let Some(source) = source else {
                return;
            };
            let Some(span) = span else {
                *dropped.entry(definition).or_default() += 1;
                return;
            };
            match narrow(span, name, anchor) {
                Some(span) => rows.push(Occurrence {
                    source,
                    span,
                    definition,
                    is_declaration,
                }),
                None => *dropped.entry(definition).or_default() += 1,
            }
        };

        // --- Declarations -------------------------------------------------
        // Every declaration span comes from a table that stores a NAME span.
        // `span_map` is consulted only where its entry *is* the name (a
        // parameter), never as a general fallback — falling back to it is what
        // used to put a whole `fun … { … }` declaration into a rename.
        for (id, variable) in &program.variables {
            push(
                &mut rows,
                &mut dropped,
                program.source_of(*id),
                Some(variable.name_span),
                variable.name,
                Anchor::Exact,
                Definition::Entity(*id),
                true,
            );
        }
        for (id, parameter) in &program.parameters {
            // A parameter's `span_map` entry is its name (analyzer.rs:20404).
            push(
                &mut rows,
                &mut dropped,
                program.source_of(*id),
                span_of(program, *id),
                parameter.name,
                Anchor::Exact,
                Definition::Entity(*id),
                true,
            );
        }
        for (id, function) in &program.functions {
            push(
                &mut rows,
                &mut dropped,
                program.source_of(*id),
                Some(function.name_span),
                function.name,
                Anchor::Exact,
                Definition::Entity(*id),
                true,
            );
        }
        for (id, function) in &program.external_functions {
            push(
                &mut rows,
                &mut dropped,
                program.source_of(*id),
                Some(function.name_span),
                function.name,
                Anchor::Exact,
                Definition::Entity(*id),
                true,
            );
        }
        for (id, structure) in &program.structs {
            push(
                &mut rows,
                &mut dropped,
                program.source_of(*id),
                Some(structure.name_span),
                structure.name,
                Anchor::Exact,
                Definition::Entity(*id),
                true,
            );
            for (index, field) in structure.fields.iter().enumerate() {
                push(
                    &mut rows,
                    &mut dropped,
                    program.source_of(*id),
                    Some(field.name_span),
                    field.name,
                    Anchor::Exact,
                    Definition::Field(*id, index),
                    true,
                );
            }
        }
        for (id, enumeration) in &program.enums {
            push(
                &mut rows,
                &mut dropped,
                program.source_of(*id),
                Some(enumeration.name_span),
                enumeration.name,
                Anchor::Exact,
                Definition::Entity(*id),
                true,
            );
        }
        for (id, definition) in &program.traits {
            push(
                &mut rows,
                &mut dropped,
                program.source_of(*id),
                Some(definition.name_span),
                definition.name,
                Anchor::Exact,
                Definition::Entity(*id),
                true,
            );
        }

        // --- Uses, from the entity table ----------------------------------
        for (use_id, expr) in &program.entity_map {
            match expr {
                // An enum variant's DECLARATION is an entity; its `span_map`
                // entry covers the payload too (`Box2(i32, i32)`), and the name
                // leads it.
                Expr::EnumVariant(enum_id, index) => {
                    let Some(name) = variant_name(program, *enum_id, *index) else {
                        continue;
                    };
                    push(
                        &mut rows,
                        &mut dropped,
                        program.source_of(*use_id),
                        span_of(program, *use_id),
                        name,
                        Anchor::Start,
                        Definition::Entity(*use_id),
                        true,
                    );
                }
                // The universal "resolved reference to a named thing": a local,
                // a parameter, a function called plainly, a static member, an
                // enum variant. Its span is the identifier when the reference
                // was spelled bare, and a `::`-path when it was qualified.
                Expr::Local(target) | Expr::Variable(target) | Expr::Parameter(target) => {
                    if use_id == target {
                        continue;
                    }
                    let Some(name) = name_of(program, Definition::Entity(*target)) else {
                        continue;
                    };
                    let Some(span) = span_of(program, *use_id) else {
                        continue;
                    };
                    // A bare reference's span already IS the identifier; a
                    // qualified one ends at the member's end token.
                    let anchor = if span.end - span.start == name.len() {
                        Anchor::Exact
                    } else {
                        Anchor::End
                    };
                    push(
                        &mut rows,
                        &mut dropped,
                        program.source_of(*use_id),
                        Some(span),
                        name,
                        anchor,
                        Definition::Entity(*target),
                        false,
                    );
                }
                // `x.field` — the member span is recorded exactly.
                Expr::Field(_, struct_id, index) => {
                    let definition = Definition::Field(*struct_id, *index);
                    let Some(name) = name_of(program, definition) else {
                        continue;
                    };
                    push(
                        &mut rows,
                        &mut dropped,
                        program.source_of(*use_id),
                        program.member_name_spans.get(use_id).copied(),
                        name,
                        Anchor::Exact,
                        definition,
                        false,
                    );
                }
                // `x.method()` — the member span is recorded under the call's
                // own id, and the callee is the call's wired subject.
                Expr::Call(call_id) => {
                    let Some(member_span) = program.member_name_spans.get(use_id) else {
                        continue;
                    };
                    let Some(call) = program.function_calls.get(call_id) else {
                        continue;
                    };
                    let target = match program.entity_map.get(&call.subject_id) {
                        Some(Expr::Local(target)) | Some(Expr::Function(target)) => *target,
                        _ => continue,
                    };
                    let Some(name) = name_of(program, Definition::Entity(target)) else {
                        continue;
                    };
                    push(
                        &mut rows,
                        &mut dropped,
                        program.source_of(*use_id),
                        Some(*member_span),
                        name,
                        Anchor::Exact,
                        Definition::Entity(target),
                        false,
                    );
                }
                // `Point { … }` — the name leads the initializer span. (The
                // analyzer also records this through `record_reference`; the
                // duplicate is removed below.)
                Expr::StructInitializer(initializer_id, _) => {
                    let Some(struct_id) = program.struct_initializer_to_def.get(initializer_id)
                    else {
                        continue;
                    };
                    let Some(name) = name_of(program, Definition::Entity(*struct_id)) else {
                        continue;
                    };
                    push(
                        &mut rows,
                        &mut dropped,
                        program.source_of(*use_id),
                        span_of(program, *use_id),
                        name,
                        Anchor::Start,
                        Definition::Entity(*struct_id),
                        false,
                    );
                }
                _ => {}
            }
        }

        // --- Uses, from the analyzer's own reference table -----------------
        // `type_references` already stores identifier-precise spans: type
        // annotations, import and `use` path segments, match-pattern variant
        // segments, `impl … with Trait` clauses, macro names. It is the one
        // place the analyzer records a reference *as* a reference, so it is
        // taken as-is — only checked against the name, never re-derived.
        for (source, span, definition, _label) in &program.type_references {
            let Some(target) = definition else {
                continue;
            };
            let definition = Definition::Entity(*target);
            let Some(name) = name_of(program, definition) else {
                continue;
            };
            // A declaration's own name span is also recorded here in some
            // shapes; `is_declaration` is settled by the declaration pass above
            // and the duplicate is removed below.
            push(
                &mut rows,
                &mut dropped,
                Some(*source),
                Some(*span),
                name,
                Anchor::Exact,
                definition,
                false,
            );
        }

        // --- Uses, from the struct-initializer field keys ------------------
        // `x` in `Point { x = 1 }` names the field, but it is not an access, so
        // it is recorded on its own table rather than in `member_name_spans`.
        // Missing these is what made a field rename emit a partial edit set that
        // broke the build.
        for (source, span, struct_id, index) in &program.struct_initializer_field_spans {
            let definition = Definition::Field(*struct_id, *index);
            let Some(name) = name_of(program, definition) else {
                continue;
            };
            push(
                &mut rows,
                &mut dropped,
                Some(*source),
                Some(*span),
                name,
                Anchor::Exact,
                definition,
                false,
            );
        }

        // --- Deduplicate ---------------------------------------------------
        // Sort so declarations win the tie for a span recorded by both passes,
        // then drop every repeat of a `(source, span)`. Without this a struct
        // rename emits each constructor site twice and the client rejects the
        // whole edit as overlapping.
        rows.sort_by(|left, right| {
            (left.source.0, left.span.start, left.span.end)
                .cmp(&(right.source.0, right.span.start, right.span.end))
                .then(right.is_declaration.cmp(&left.is_declaration))
                .then(left.definition.sort_key().cmp(&right.definition.sort_key()))
        });
        rows.dedup_by(|left, right| left.source == right.source && left.span == right.span);

        let mut by_definition: HashMap<Definition, Vec<u32>> = HashMap::new();
        for (index, row) in rows.iter().enumerate() {
            by_definition
                .entry(row.definition)
                .or_default()
                .push(index as u32);
        }

        ReferenceIndex {
            occurrences: rows,
            by_definition,
            dropped,
        }
    }

    /// The occurrence at `offset` in `source`, if the cursor is on an
    /// identifier.
    ///
    /// Because every row is identifier-exact, rows cannot nest, so there is at
    /// most one answer — which is precisely why this replaces the old resolution
    /// ladder rather than being another rung on it.
    pub fn at(&self, source: SourceId, offset: usize) -> Option<&Occurrence> {
        let start = self
            .occurrences
            .partition_point(|row| (row.source.0, row.span.end) <= (source.0, offset));
        self.occurrences[start..]
            .iter()
            .take_while(|row| row.source == source && row.span.start <= offset)
            .find(|row| row.span.start <= offset && offset < row.span.end)
    }

    /// Every occurrence of `definition`, declaration included, in source order.
    pub fn occurrences_of(&self, definition: Definition) -> impl Iterator<Item = &Occurrence> {
        self.by_definition
            .get(&definition)
            .map(|rows| rows.as_slice())
            .unwrap_or(&[])
            .iter()
            .map(|index| &self.occurrences[*index as usize])
    }

    /// How many references to `definition` are known to be missing from the
    /// index — use sites whose recorded span could not be proven to cover an
    /// identifier. Non-zero means an edit set over this definition would be
    /// incomplete, and a rename must refuse rather than emit it.
    pub fn dropped_for(&self, definition: Definition) -> usize {
        self.dropped.get(&definition).copied().unwrap_or(0)
    }

    /// The cross-program identity of `definition` — see [`DefinitionKey`].
    ///
    /// `None` when the definition has no usable declaration address: its
    /// declaration row was dropped (its span could not be narrowed onto an
    /// identifier), it has no declaration row at all (a module), or it lives
    /// in generated code, which no file holds. A definition with no key simply
    /// cannot be reached from another program, and the union degrades to the
    /// single-program answer.
    pub fn key_of(&self, program: &Program, definition: Definition) -> Option<DefinitionKey> {
        let declaration = self
            .occurrences_of(definition)
            .find(|occurrence| occurrence.is_declaration)?;
        let path = program
            .canonical_sources
            .get(declaration.source.0 as usize)?
            .clone();
        let name = name_of(program, definition)?.to_string();
        Some(DefinitionKey {
            path,
            span: declaration.span,
            name,
        })
    }

    /// The definition `key` names in THIS index's program, found by its
    /// declaration address — the other direction of [`ReferenceIndex::key_of`],
    /// and the step that makes a cross-document query one lookup per program.
    ///
    /// A linear scan over the declaration rows, filtered span-first so the
    /// path comparison (the expensive half) runs on at most a handful of rows.
    /// `None` when this program never loaded the declaring file, or loaded a
    /// text in which that span does not spell `key`'s name.
    pub fn definition_of_key(&self, program: &Program, key: &DefinitionKey) -> Option<Definition> {
        self.occurrences
            .iter()
            .filter(|row| row.is_declaration && row.span == key.span)
            .find(|row| {
                name_of(program, row.definition) == Some(key.name.as_str())
                    && program
                        .canonical_sources
                        .get(row.source.0 as usize)
                        .map(PathBuf::as_path)
                        == Some(key.path())
            })
            .map(|row| row.definition)
    }

    /// Every occurrence in `source`, in source order.
    pub fn occurrences_in(&self, source: SourceId) -> impl Iterator<Item = &Occurrence> {
        self.occurrences
            .iter()
            .filter(move |row| row.source == source)
    }

    /// Every row, for the invariant pins.
    #[cfg(test)]
    pub fn rows(&self) -> &[Occurrence] {
        &self.occurrences
    }
}

/// The declaration name of a definition.
pub fn name_of<'a>(program: &'a Program, definition: Definition) -> Option<&'a str> {
    match definition {
        Definition::Field(struct_id, index) => program
            .structs
            .get(&struct_id)
            .and_then(|structure| structure.fields.get(index))
            .map(|field| field.name),
        Definition::Entity(id) => {
            if let Some(variable) = program.variables.get(&id) {
                return Some(variable.name);
            }
            if let Some(parameter) = program.parameters.get(&id) {
                return Some(parameter.name);
            }
            if let Some(function) = program.functions.get(&id) {
                return Some(function.name);
            }
            if let Some(function) = program.external_functions.get(&id) {
                return Some(function.name);
            }
            if let Some(structure) = program.structs.get(&id) {
                return Some(structure.name);
            }
            if let Some(enumeration) = program.enums.get(&id) {
                return Some(enumeration.name);
            }
            if let Some(definition) = program.traits.get(&id) {
                return Some(definition.name);
            }
            if let Some(module) = program.modules.get(&id) {
                return Some(module.name);
            }
            match program.entity_map.get(&id) {
                Some(Expr::EnumVariant(enum_id, index)) => variant_name(program, *enum_id, *index),
                _ => None,
            }
        }
    }
}

/// What kind of thing a definition is.
pub fn kind_of(program: &Program, definition: Definition) -> Option<DefinitionKind> {
    match definition {
        Definition::Field(..) => Some(DefinitionKind::Field),
        Definition::Entity(id) => {
            if program.variables.contains_key(&id) || program.parameters.contains_key(&id) {
                return Some(DefinitionKind::Binding);
            }
            if program.functions.contains_key(&id) || program.external_functions.contains_key(&id) {
                return Some(DefinitionKind::Function);
            }
            if program.structs.contains_key(&id) {
                return Some(DefinitionKind::Struct);
            }
            if program.enums.contains_key(&id) {
                return Some(DefinitionKind::Enum);
            }
            if program.traits.contains_key(&id) {
                return Some(DefinitionKind::Trait);
            }
            if program.modules.contains_key(&id) {
                return Some(DefinitionKind::Module);
            }
            match program.entity_map.get(&id) {
                Some(Expr::EnumVariant(..)) => Some(DefinitionKind::Variant),
                _ => None,
            }
        }
    }
}

/// The file a definition is DECLARED in — the provenance the organize-imports
/// usage model needs: "did this file resolve anything that lives in the module
/// this import reaches into?"
pub fn declaration_source(program: &Program, definition: Definition) -> Option<SourceId> {
    match definition {
        Definition::Entity(id) => program.source_of(id),
        Definition::Field(struct_id, _) => program.source_of(struct_id),
    }
}

fn variant_name<'a>(program: &'a Program, enum_id: Id, index: usize) -> Option<&'a str> {
    program
        .enums
        .get(&enum_id)
        .and_then(|enumeration| enumeration.variants.get(index))
        .map(|variant| variant.name)
}

fn span_of(program: &Program, id: Id) -> Option<Span> {
    vilan_ide::analysis::span_of(program, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::document::tests::analyze_workspace;

    /// One source exercising every symbol kind the index has to name: a struct
    /// and its fields, an inherent method and a static one, a free function, an
    /// enum with a payload variant, a variant used in an expression AND in a
    /// match pattern, a local, and a parameter.
    const MATRIX: &str = "\
struct Point {
\tx: i32,
\ty: i32,
}

impl Point {
\tfun origin(): Point {
\t\tPoint { x = 0, y = 0 }
\t}

\tfun sum(self): i32 {
\t\tself.x + self.y
\t}
}

enum Shape {
\tDot,
\tBox2(i32, i32),
}

fun helper(value: i32): i32 {
\tvalue + 1
}

fun main(): i32 {
\tlet p = Point::origin();
\tlet total = p.sum();
\tlet bumped = helper(total);
\tlet s = Shape::Dot;
\tlet q = Point { x = 1, y = 2 };
\tmatch s {
\t\tShape::Dot => bumped + q.x,
\t\tShape::Box2(let w, let h) => w + h,
\t}
}
";

    fn matrix() -> (std::path::PathBuf, Document) {
        analyze_workspace(&[("main.vl", MATRIX)])
    }

    /// The byte offset of `needle`, plus `delta` to land inside the identifier.
    /// Panics when the needle is absent, so an edited source fails loudly rather
    /// than silently probing offset 0.
    fn at(needle: &str, delta: usize) -> usize {
        MATRIX
            .find(needle)
            .unwrap_or_else(|| panic!("{needle:?} not in the pin source"))
            + delta
    }

    /// The text of every reference to the symbol under the cursor, in source
    /// order, with each entry-file span rendered as the text it covers.
    fn references_at(document: &Document, offset: usize) -> Vec<&'static str> {
        let mut found: Vec<(usize, &str)> = document
            .references(offset)
            .into_iter()
            .map(|(_, span)| {
                let range = span.into_range();
                (
                    range.start,
                    MATRIX.get(range).expect("a span outside the entry text"),
                )
            })
            .collect();
        found.sort();
        found.into_iter().map(|(_, text)| text).collect()
    }

    // --- The invariants ------------------------------------------------

    // INVARIANT 1. Every row covers exactly an identifier — its text is the
    // definition's own name. This one assertion subsumes the whole class the
    // index was built to close: a whole-declaration span (renaming a static
    // method used to replace the entire `fun … { … }`), a whole-`::`-path span
    // (`Enum::Variant` rewritten whole), and a span narrowed onto the wrong
    // segment all fail it.
    #[test]
    fn every_indexed_span_covers_exactly_an_identifier() {
        let (dir, document) = matrix();
        let program = document.program.as_ref().expect("program");
        let index = document.reference_index();
        assert!(!index.rows().is_empty(), "the pin needs a populated index");
        let mut checked = 0;
        for row in index.rows() {
            if row.source != SourceId(0) {
                continue; // only the entry file's text is on hand here
            }
            let name = name_of(program, row.definition).expect("a named definition");
            let text = MATRIX
                .get(row.span.into_range())
                .unwrap_or_else(|| panic!("span {:?} is outside the entry text", row.span));
            assert_eq!(
                text, name,
                "row {row:?} covers {text:?}, which is not the identifier {name:?}",
            );
            checked += 1;
        }
        assert!(checked > 20, "expected a broad sample, checked {checked}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // INVARIANT 2. No two rows share a span. The analyzer records some
    // references twice over (a struct's constructor name lands in both
    // `type_references` and `struct_initializer_to_def`; a match pattern's
    // segments are re-recorded on every type-check pass), and a duplicate span
    // reaches the client as an overlapping `TextEdit` — which is what made a
    // struct rename fail with "Rename failed to apply edits".
    #[test]
    fn no_two_indexed_occurrences_share_a_span() {
        let (dir, document) = matrix();
        let mut seen = std::collections::HashSet::new();
        for row in document.reference_index().rows() {
            assert!(
                seen.insert((row.source.0, row.span)),
                "{:?} at {:?} is recorded twice",
                row.span,
                row.source,
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- The per-symbol-kind matrix -------------------------------------
    // kolt.local 003 and 002 both asked for one shared matrix rather than two
    // symptom patches. Each case names a cursor position and the complete
    // reference set it must answer with.

    #[test]
    fn a_struct_is_named_from_its_declaration_its_annotations_and_its_constructors() {
        let (dir, document) = matrix();
        let expected = ["Point"; 6];
        // The declaration, the `impl` head, the return annotation, and all
        // three constructor sites — each exactly once.
        for (label, offset) in [
            ("declaration", at("struct Point", 7)),
            ("impl head", at("impl Point", 5)),
            ("return annotation", at("): Point", 3)),
            ("constructor", at("Point { x = 1", 1)),
        ] {
            assert_eq!(
                references_at(&document, offset),
                expected,
                "from the {label}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The field-declaration case: a field's name has no entity of its own, and
    // the whole-struct entity used to swallow the cursor, so this answered with
    // the STRUCT's references instead of the field's.
    #[test]
    fn a_struct_field_is_named_from_its_declaration_its_accesses_and_its_keys() {
        let (dir, document) = matrix();
        let expected = ["x"; 5];
        for (label, offset) in [
            ("declaration", at("\tx: i32", 1)),
            ("access through self", at("self.x", 5)),
            ("access through a local", at("q.x", 2)),
            ("initializer key", at("Point { x = 1", 8)),
        ] {
            assert_eq!(
                references_at(&document, offset),
                expected,
                "from the {label}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // A field rename that misses a construction site emits a partial edit that
    // BREAKS THE BUILD — the initializer key is not a field access, so it had
    // nowhere to be recorded until `struct_initializer_field_spans`.
    #[test]
    fn a_field_reaches_every_construction_site() {
        let (dir, document) = matrix();
        let starts: Vec<usize> = document
            .references(at("\tx: i32", 1))
            .into_iter()
            .map(|(_, span)| span.start)
            .collect();
        for key in ["Point { x = 0", "Point { x = 1"] {
            let offset = at(key, 8);
            assert!(
                starts.contains(&offset),
                "the initializer key at {offset} is missing from {starts:?}",
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_instance_method_is_named_from_its_declaration_and_its_call() {
        let (dir, document) = matrix();
        for (label, offset) in [("declaration", at("fun sum", 4)), ("call", at("p.sum", 2))] {
            assert_eq!(
                references_at(&document, offset),
                ["sum", "sum"],
                "from the {label}",
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The static-method case (kolt.local 002's "problematic" symptom). A
    // `Type::method()` call is a plain `Local` reference whose span is the whole
    // `Point::origin` path, so the old code answered with the whole `fun … { … }`
    // declaration and the whole path — a rename that destroyed the declaration.
    #[test]
    fn a_static_method_is_named_from_its_declaration_and_its_qualified_call() {
        let (dir, document) = matrix();
        for (label, offset) in [
            ("declaration", at("fun origin", 4)),
            ("qualified call", at("Point::origin", 7)),
        ] {
            assert_eq!(
                references_at(&document, offset),
                ["origin", "origin"],
                "from the {label}",
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The two halves of a `::` path name DIFFERENT things, and the cursor has to
    // tell them apart — the type before the `::`, the member after it.
    #[test]
    fn each_half_of_a_qualified_path_names_its_own_definition() {
        let (dir, document) = matrix();
        assert_eq!(
            references_at(&document, at("Point::origin", 1)),
            ["Point"; 6],
            "the cursor on the type half",
        );
        assert_eq!(
            references_at(&document, at("Point::origin", 7)),
            ["origin", "origin"],
            "the cursor on the member half",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_free_function_is_named_from_its_declaration_and_its_call() {
        let (dir, document) = matrix();
        for (label, offset) in [
            ("declaration", at("fun helper", 4)),
            ("call", at("helper(total)", 1)),
        ] {
            assert_eq!(
                references_at(&document, offset),
                ["helper", "helper"],
                "from the {label}",
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The headline 003 symptom: an enum variant answered with NOTHING from its
    // declaration and NOTHING from a match pattern, because the resolver had no
    // arm for a variant at all and rejected the pattern segment the analyzer had
    // already recorded.
    #[test]
    fn an_enum_variant_is_named_from_its_declaration_its_expression_and_its_pattern() {
        let (dir, document) = matrix();
        for (label, offset) in [
            ("declaration", at("\tDot,", 1)),
            ("expression use", at("Shape::Dot;", 7)),
            ("match pattern", at("Shape::Dot =>", 7)),
        ] {
            assert_eq!(
                references_at(&document, offset),
                ["Dot", "Dot", "Dot"],
                "from the {label}",
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_enum_is_named_from_its_declaration_and_every_qualifier() {
        let (dir, document) = matrix();
        for (label, offset) in [
            ("declaration", at("enum Shape", 5)),
            ("expression qualifier", at("Shape::Dot;", 1)),
            ("pattern qualifier", at("Shape::Dot =>", 1)),
        ] {
            assert_eq!(
                references_at(&document, offset),
                ["Shape"; 4],
                "from the {label}",
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_local_and_a_parameter_are_named_from_both_ends() {
        let (dir, document) = matrix();
        assert_eq!(
            references_at(&document, at("let total", 4)),
            ["total", "total"],
        );
        assert_eq!(
            references_at(&document, at("helper(total)", 7)),
            ["total", "total"],
        );
        assert_eq!(
            references_at(&document, at("value: i32", 0)),
            ["value", "value"]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // A cursor that is not on an identifier answers empty — and that is the ONLY
    // thing an empty answer is allowed to mean now.
    #[test]
    fn a_cursor_off_any_identifier_answers_empty() {
        let (dir, document) = matrix();
        assert!(references_at(&document, at("value + 1", 6)).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- Consistency -----------------------------------------------------

    // Edit, query, edit again, query: the second answer must reflect the edit.
    // The index is rebuilt with its analysis and moved into the document as one
    // piece with the program it describes (`adopt_analysis` destructures every
    // field, so a future field cannot be forgotten silently), which is what
    // stops a query answering the previous buffer's spans.
    #[test]
    fn each_analysis_answers_its_own_text() {
        let one = "struct Point {\n\tx: i32,\n}\n\nfun main(): i32 {\n\tlet p = Point { x = 1 };\n\tp.x\n}\n";
        let two = "struct Point {\n\tx: i32,\n}\n\nfun main(): i32 {\n\tlet p = Point { x = 1 };\n\tp.x + p.x\n}\n";
        let count = |source: &str| {
            let (dir, document) = analyze_workspace(&[("main.vl", source)]);
            assert!(
                document.diagnostics.is_empty(),
                "the pin needs a clean program: {:?}",
                document.diagnostics,
            );
            let offset = source.find("\tx: i32").expect("the field declaration") + 1;
            let found = document.references(offset).len();
            let _ = std::fs::remove_dir_all(&dir);
            found
        };
        // Declaration + initializer key + one access.
        assert_eq!(count(one), 3, "the first text");
        // The edit adds a second access.
        assert_eq!(count(two), 4, "the edited text must be reflected");
        // And going back is reflected too — the answer tracks the text both ways.
        assert_eq!(count(one), 3, "the reverted text");
    }

    // --- Cross-document reach (kolt.local 034, 003 branch (c)) -----------
    // A query answers over the open file's own program — its entry file plus
    // the import closure below it — so a symbol clicked in the file that
    // DEFINES it used to see none of the files that IMPORT that file: the
    // declaration came back and nothing else, which in a multi-file app is
    // what "Find References intermittently returns nothing" was. The fix is
    // the cross-program `DefinitionKey` plus a union over every open
    // document's program, and rename reads the same union.

    const LIBRARY: &str = "struct Point {\n\tx: i32,\n}\n";
    const APPLICATION: &str =
        "import pkg::library::Point;\n\nfun main(): i32 {\n\tlet p = Point { x = 1 };\n\tp.x\n}\n";

    /// `library.vl` (the definer) analyzed as the open document, and
    /// `application.vl` (its importer) open beside it as its own document —
    /// the two-programs-in-play shape every pin in this section queries.
    fn library_and_application() -> (std::path::PathBuf, Document, Document) {
        let (dir, library_document) =
            analyze_workspace(&[("library.vl", LIBRARY), ("application.vl", APPLICATION)]);
        let application_document = Document::analyze(
            APPLICATION,
            &crate::document::tests::std_root(),
            &dir.join("application.vl"),
        );
        (dir, library_document, application_document)
    }

    /// How many of `found` land in the file named `name`.
    fn in_file(found: &[(std::path::PathBuf, Span)], name: &str) -> usize {
        found
            .iter()
            .filter(|(path, _)| path.ends_with(name))
            .count()
    }

    #[test]
    fn a_definition_sees_the_files_that_import_it() {
        let (dir, document, application_document) = library_and_application();
        let offset = LIBRARY.find("struct Point").expect("the declaration") + 7;
        let found = document.references_across(offset, [&application_document]);
        assert!(
            found.len() > 1,
            "`Point` is used in application.vl, but only {found:?} came back",
        );
        // The declaration itself is a row in BOTH programs — the union
        // reports it once.
        assert_eq!(in_file(&found, "library.vl"), 1, "{found:?}");
        // The importer contributes its import leaf and its constructor.
        assert_eq!(in_file(&found, "application.vl"), 2, "{found:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The field shape crosses programs through the OTHER key arm — a field has
    // no entity id, so its key addresses the field declaration row itself.
    #[test]
    fn a_field_definition_sees_the_accesses_in_files_that_import_it() {
        let (dir, document, application_document) = library_and_application();
        let offset = LIBRARY.find("\tx: i32").expect("the field") + 1;
        let found = document.references_across(offset, [&application_document]);
        assert_eq!(in_file(&found, "library.vl"), 1, "{found:?}");
        // The initializer key in `Point { x = 1 }` and the access `p.x`.
        assert_eq!(in_file(&found, "application.vl"), 2, "{found:?}");
        for (path, span) in &found {
            let text = if path.ends_with("library.vl") {
                LIBRARY
            } else {
                APPLICATION
            };
            assert_eq!(&text[span.into_range()], "x", "{path:?} {span:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_function_definition_sees_the_calls_in_files_that_import_it() {
        let library = "fun helper(value: i32): i32 {\n\tvalue + 1\n}\n";
        let application = "import pkg::library::helper;\n\nfun main(): i32 {\n\thelper(1)\n}\n";
        let (dir, document) =
            analyze_workspace(&[("library.vl", library), ("application.vl", application)]);
        let application_document = Document::analyze(
            application,
            &crate::document::tests::std_root(),
            &dir.join("application.vl"),
        );
        let offset = library.find("fun helper").expect("the declaration") + 4;
        let found = document.references_across(offset, [&application_document]);
        assert_eq!(in_file(&found, "library.vl"), 1, "{found:?}");
        // The import leaf and the call.
        assert_eq!(in_file(&found, "application.vl"), 2, "{found:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The union must not manufacture reach: a file that never imports the
    // definer contributes nothing — even one declaring an identically named
    // struct at the identical offsets, which is exactly what a raw
    // span-address comparison without the file would mistake for a match.
    #[test]
    fn a_file_that_never_imports_the_definer_contributes_nothing() {
        let (dir, document) = analyze_workspace(&[
            ("library.vl", LIBRARY),
            ("application.vl", APPLICATION),
            ("unrelated.vl", LIBRARY),
        ]);
        let application_document = Document::analyze(
            APPLICATION,
            &crate::document::tests::std_root(),
            &dir.join("application.vl"),
        );
        let unrelated_document = Document::analyze(
            LIBRARY,
            &crate::document::tests::std_root(),
            &dir.join("unrelated.vl"),
        );
        let offset = LIBRARY.find("struct Point").expect("the declaration") + 7;
        let found =
            document.references_across(offset, [&application_document, &unrelated_document]);
        assert_eq!(
            in_file(&found, "unrelated.vl"),
            0,
            "unrelated.vl's own `Point` is a different definition: {found:?}",
        );
        assert_eq!(found.len(), 3, "{found:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The rename face of the same reach (the standing rule from 003: rename
    // reads the same index, so it had the same blind spot and gains the same
    // fix — through the same path, not a parallel one).
    #[test]
    fn a_rename_at_a_definition_rewrites_the_files_that_import_it() {
        let (dir, document, application_document) = library_and_application();
        let offset = LIBRARY.find("struct Point").expect("the declaration") + 7;
        let spans = document
            .rename_edits_across(offset, "Renamed", [&application_document])
            .expect("the cross-file rename");
        assert_eq!(in_file(&spans, "library.vl"), 1, "{spans:?}");
        assert_eq!(in_file(&spans, "application.vl"), 2, "{spans:?}");
        // Every edit replaces exactly the identifier, in its own file's text.
        for (path, span) in &spans {
            let text = if path.ends_with("library.vl") {
                LIBRARY
            } else {
                APPLICATION
            };
            assert_eq!(&text[span.into_range()], "Point", "{path:?} {span:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // An importer whose buffer has moved past its analysis cannot be trusted
    // for edit positions, and silently skipping it would emit the partial
    // rename the refusal rule forbids — so the rename refuses, for the one
    // debounce the state lasts.
    #[test]
    fn a_rename_refuses_while_an_importing_file_is_still_analyzing() {
        let (dir, document, mut application_document) = library_and_application();
        application_document.set_text("fun main() {}\n");
        let offset = LIBRARY.find("struct Point").expect("the declaration") + 7;
        let refusal = document
            .rename_edits_across(offset, "Renamed", [&application_document])
            .expect_err("a stale importer must refuse the rename");
        assert!(
            matches!(refusal, RenameRefusal::StillAnalyzing { .. }),
            "{refusal:?}",
        );
        assert!(
            refusal.message().contains("still being analyzed"),
            "{}",
            refusal.message(),
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- Rename: the per-symbol-kind matrix -----------------------------
    // kolt.local 002 asked for exactly this list — fields, static methods,
    // instance methods, enum variants, modules, type params, locals — and for
    // rename to be a thin layer over the same index find-references reads,
    // rather than a second mechanism that can disagree with it.

    use crate::document::{RenameRefusal, is_identifier};

    /// The rename edit set, rendered as the text each span currently covers, so
    /// a wrong span shows up as the wrong word rather than as a number.
    fn rename_at(document: &Document, offset: usize) -> Result<Vec<&'static str>, RenameRefusal> {
        let mut spans = document.rename_edits(offset, "renamed")?;
        spans.sort_by_key(|(source, span)| (source.0, span.start));
        Ok(spans
            .into_iter()
            .map(|(_, span)| {
                MATRIX
                    .get(span.into_range())
                    .expect("inside the entry text")
            })
            .collect())
    }

    // Rename must rewrite exactly what find-references reports — no more (a
    // partial or duplicated set is what the client rejected with "Rename failed
    // to apply edits") and no less (a missing site breaks the build).
    #[test]
    fn a_rename_rewrites_exactly_what_find_references_reports() {
        let (dir, document) = matrix();
        for (label, offset) in [
            ("a struct", at("struct Point", 7)),
            ("a struct field", at("\tx: i32", 1)),
            ("an instance method", at("fun sum", 4)),
            ("a static method", at("fun origin", 4)),
            ("a static call", at("Point::origin", 7)),
            ("a free function", at("fun helper", 4)),
            ("an enum", at("enum Shape", 5)),
            ("an enum variant", at("\tDot,", 1)),
            ("a variant in a pattern", at("Shape::Dot =>", 7)),
            ("a local", at("let total", 4)),
            ("a parameter", at("value: i32", 0)),
        ] {
            let renamed = rename_at(&document, offset).unwrap_or_else(|refusal| {
                panic!("renaming {label} refused: {}", refusal.message())
            });
            assert_eq!(
                renamed,
                references_at(&document, offset),
                "renaming {label} must rewrite exactly the references",
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Every edit rewrites an identifier, never a declaration or a whole path.
    // Renaming a static method used to replace the entire `fun … { … }`.
    #[test]
    fn every_rename_edit_replaces_one_identifier() {
        let (dir, document) = matrix();
        for (label, offset, expected) in [
            ("a static method", at("Point::origin", 7), "origin"),
            ("a free function", at("helper(total)", 1), "helper"),
            ("a variant", at("Shape::Dot;", 7), "Dot"),
            ("a field", at("Point { x = 1", 8), "x"),
        ] {
            for text in rename_at(&document, offset).expect("a rename") {
                assert_eq!(text, expected, "renaming {label} touched {text:?}");
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // No two edits may address the same range: an overlapping `WorkspaceEdit` is
    // rejected wholesale by the client, which is the "Rename failed to apply
    // edits" the item reported for struct fields.
    #[test]
    fn a_rename_never_emits_the_same_span_twice() {
        let (dir, document) = matrix();
        for offset in [
            at("struct Point", 7),
            at("enum Shape", 5),
            at("\tx: i32", 1),
            at("Point { x = 1", 1),
        ] {
            let spans = document.rename_edits(offset, "renamed").expect("a rename");
            let mut seen = std::collections::HashSet::new();
            for (source, span) in &spans {
                assert!(
                    seen.insert((source.0, *span)),
                    "{span:?} is emitted twice by the rename at {offset}",
                );
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- Rename: the refusals -------------------------------------------

    #[test]
    fn a_rename_refuses_a_name_that_is_not_an_identifier() {
        let (dir, document) = matrix();
        let offset = at("let total", 4);
        for bad in ["", "2nd", "has space", "has-hyphen", "fun", "struct"] {
            let refusal = document
                .rename_edits(offset, bad)
                .expect_err(&format!("{bad:?} must be refused"));
            assert_eq!(refusal, RenameRefusal::InvalidName(bad.to_string()));
        }
        // And the valid spellings are not refused.
        for good in ["total2", "_total", "totalCount", "TOTAL"] {
            assert!(document.rename_edits(offset, good).is_ok(), "{good:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_identifier_check_matches_the_lexer_keywords() {
        assert!(is_identifier("value"));
        assert!(is_identifier("_"));
        assert!(!is_identifier(""));
        assert!(!is_identifier("1a"));
        // Every keyword the lexer knows is refused as a new name, so the check
        // cannot drift from the language.
        for (keyword, _) in vilan_core::lexing::KEYWORDS {
            assert!(
                !is_identifier(keyword),
                "{keyword} must not be renameable-to"
            );
        }
    }

    // A rename reached through an import must not rewrite the library it reached
    // into. The old handler would hand the client edits for files under
    // `$VILAN_STD` without a word.
    #[test]
    fn a_rename_refuses_to_edit_the_standard_library() {
        let source = "import std::io::print;\n\nfun main() {\n\tprint(\"hello\");\n}\n";
        let (dir, document) = analyze_workspace(&[("main.vl", source)]);
        assert!(
            document.diagnostics.is_empty(),
            "the pin needs a clean program: {:?}",
            document.diagnostics,
        );
        let offset = source.find("print(").expect("the call") + 1;
        let refusal = document
            .rename_edits(offset, "renamed")
            .expect_err("renaming a std function must be refused");
        assert!(
            matches!(
                refusal,
                RenameRefusal::NotOwned {
                    origin: "the standard library",
                    ..
                }
            ),
            "expected a not-owned refusal, got {refusal:?}",
        );
        assert!(
            refusal.message().contains("does not own"),
            "the refusal must say why: {}",
            refusal.message(),
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_rename_off_any_identifier_reports_nothing_to_rename() {
        let (dir, document) = matrix();
        assert_eq!(
            document.rename_edits(at("value + 1", 6), "renamed"),
            Err(RenameRefusal::NotAnIdentifier),
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
