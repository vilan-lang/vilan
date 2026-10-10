//! A type spelled the way the FILE can write it — the one writer behind
//! E278's "Ascribe this stage" (`type-ascription.md` §11) and B570's `auto`
//! rewrite (`auto-annotations.md` §5, §6).
//!
//! Two rules, both RULED:
//!
//! 1. **What the inlay hint shows, a bare trait for `~X`** (B570 Q3 door (b),
//!    B571 Q8). A hinted struct or enum whose instantiation an impl of the
//!    hinted application ADMITS (E227's own question) is written as that
//!    trait, bare — `Pipe<Option<str>>` — which an ascription and an `auto`
//!    annotation both read as B161's constraint: checked wide, the concrete
//!    type kept. Everything else is written in full.
//! 2. **Qualify, never import** (B570 Q7). A nominal type is written by the
//!    shortest spelling that RESOLVES where it is written: its name, when the
//!    scope resolves that name to it; else through a module the scope has
//!    imported (`reactive::Derive`, `reactive::store::Store`). The paper's
//!    third tier — the full path, `std::reactive::Derive` — does not resolve
//!    in vilan (a qualified path reaches through an IMPORTED module name,
//!    §4.2), so a type the file cannot reach either way is not written: the
//!    writer declines with the reason, and the import that would let it.
//!
//! A type no source spelling denotes — `never`, `unknown`, a function item, a
//! closure carrying a `context` clause, a mapped tuple — declines too.

use super::*;

/// Why a type cannot be written where it was asked for — the note `--fix`
/// and the editor show in place of an edit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Unwritable(pub String);

impl<'src> Analyzer<'src> {
    /// `type_` spelled as `scope_id`'s file can write it (see the module
    /// doc). `abbreviate` writes an admitted hinted node as its bare trait;
    /// `admitted` caches the admission per instantiation across one batch.
    pub(super) fn written_spelling(
        &mut self,
        type_: &Type,
        scope_id: Id,
        abbreviate: bool,
        admitted: &mut HashMap<TypeId, bool>,
    ) -> Result<String, Unwritable> {
        self.written_spelling_at(type_, scope_id, abbreviate, admitted, 0)
    }

    fn written_spelling_at(
        &mut self,
        type_: &Type,
        scope_id: Id,
        abbreviate: bool,
        admitted: &mut HashMap<TypeId, bool>,
        depth: usize,
    ) -> Result<String, Unwritable> {
        const MAX_DEPTH: usize = 24;
        if depth > MAX_DEPTH {
            return Err(Unwritable("the type nests too deeply to write".to_string()));
        }
        match type_ {
            Type::Void => Ok("void".to_string()),
            Type::Any => Ok("any".to_string()),
            Type::Struct(id, arguments) | Type::Enum(id, arguments) => {
                let (id, arguments) = (*id, arguments.clone());
                if abbreviate
                    && let Some((trait_id, required)) =
                        self.admitted_hint(type_, id, &arguments, admitted)
                {
                    let head = self.written_name(trait_id, scope_id)?;
                    return self.with_written_arguments(
                        head, &required, scope_id, abbreviate, admitted, depth,
                    );
                }
                let head = self.written_name(id, scope_id)?;
                self.with_written_arguments(head, &arguments, scope_id, abbreviate, admitted, depth)
            }
            Type::Trait(id, arguments) => {
                let (id, arguments) = (*id, arguments.clone());
                let head = self.written_name(id, scope_id)?;
                self.with_written_arguments(head, &arguments, scope_id, abbreviate, admitted, depth)
            }
            Type::Dyn(id, arguments) => {
                let (id, arguments) = (*id, arguments.clone());
                let head = self.written_name(id, scope_id)?;
                let written = self.with_written_arguments(
                    head, &arguments, scope_id, abbreviate, admitted, depth,
                )?;
                Ok(format!("dyn {written}"))
            }
            Type::Generic(constraint_id) => {
                let name = self
                    .generic_constraint_names
                    .get(constraint_id)
                    .copied()
                    .ok_or_else(|| Unwritable("a generic parameter with no name".to_string()))?;
                // The parameter must be the one the scope names, or the
                // spelling would mean a different type there.
                match self.try_get_type_id_by_name(name, scope_id) {
                    Some(_) => Ok(name.to_string()),
                    None => Err(Unwritable(format!(
                        "it mentions the generic parameter `{name}`, which is not in scope here"
                    ))),
                }
            }
            // A labelled tuple is written with its labels (B569): they are
            // no part of the type, but they are what its readers name.
            Type::Tuple(items, labels) => {
                let (items, labels) = (items.clone(), labels.clone());
                let mut parts = Vec::with_capacity(items.len());
                for (index, item) in items.into_iter().enumerate() {
                    let item = item.get_type(self);
                    let written =
                        self.written_spelling_at(&item, scope_id, abbreviate, admitted, depth + 1)?;
                    parts.push(match labels.get(index) {
                        Some(label) => format!("{label}: {written}"),
                        None => written,
                    });
                }
                Ok(format!("({})", parts.join(", ")))
            }
            Type::Array(element, length) => {
                let (element, length) = (element.get_type(self), *length);
                let element =
                    self.written_spelling_at(&element, scope_id, abbreviate, admitted, depth + 1)?;
                Ok(format!("[{element}; {length}]"))
            }
            Type::Closure(parameters, return_id, contexts, modes) => {
                if !contexts.is_empty() {
                    return Err(Unwritable(
                        "it is a closure type with a `context` clause".to_string(),
                    ));
                }
                let (parameters, return_id, modes) =
                    (parameters.clone(), *return_id, modes.clone());
                let mut parts = Vec::with_capacity(parameters.len());
                for (index, parameter) in parameters.iter().enumerate() {
                    let parameter = parameter.get_type(self);
                    let prefix = modes
                        .get(index)
                        .map_or("", |mode| self.resolved_parameter_mode(*mode).prefix());
                    let written = self.written_spelling_at(
                        &parameter,
                        scope_id,
                        abbreviate,
                        admitted,
                        depth + 1,
                    )?;
                    parts.push(format!("{prefix}{written}"));
                }
                let returned = return_id.get_type(self);
                let returned =
                    self.written_spelling_at(&returned, scope_id, abbreviate, admitted, depth + 1)?;
                Ok(format!("|{}| {returned}", parts.join(", ")))
            }
            other => Err(Unwritable(format!(
                "`{}` is not a type a file can write",
                self.pretty_print_type(other, &SubstitutionContext::default())
            ))),
        }
    }

    fn with_written_arguments(
        &mut self,
        head: String,
        arguments: &[TypeId],
        scope_id: Id,
        abbreviate: bool,
        admitted: &mut HashMap<TypeId, bool>,
        depth: usize,
    ) -> Result<String, Unwritable> {
        if arguments.is_empty() {
            return Ok(head);
        }
        let mut parts = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let argument = argument.get_type(self);
            parts.push(self.written_spelling_at(
                &argument,
                scope_id,
                abbreviate,
                admitted,
                depth + 1,
            )?);
        }
        Ok(format!("{head}<{}>", parts.join(", ")))
    }

    /// The shortest spelling of the declaration `entity` that resolves at
    /// `scope_id`: its name, else `module::name` through a module the scope
    /// imported (one level of child modules deep). Never an import.
    fn written_name(&self, entity: Id, scope_id: Id) -> Result<String, Unwritable> {
        let name = self
            .structs
            .get(&entity)
            .map(|declaration| declaration.name)
            .or_else(|| self.enums.get(&entity).map(|declaration| declaration.name))
            .or_else(|| self.traits.get(&entity).map(|declaration| declaration.name))
            .ok_or_else(|| Unwritable("it names no declaration a file can write".to_string()))?;
        let writer_in_std = self
            .source_of_id(entity)
            .zip(self.scope_file(scope_id))
            .is_some_and(|(declared, written)| {
                self.std_sources.contains(&declared) && self.std_sources.contains(&written)
            });
        if !writer_in_std
            && self
                .item_labels
                .get(&entity)
                .and_then(|labels| labels.internal)
                .is_some()
        {
            return Err(Unwritable(format!(
                "`{name}` is `[internal]` to std, so this file cannot name it"
            )));
        }
        if self.try_get_type_id_by_name(name, scope_id) == Some(entity) {
            return Ok(name.to_string());
        }
        // Through a module the scope holds by name: the aliases in scope,
        // nearest scope first, each name in its sorted order so the answer
        // does not depend on a map's iteration.
        let mut current = Some(scope_id);
        let mut seen: HashSet<&str> = HashSet::default();
        while let Some(at) = current {
            let Some(scope) = self.scopes.get(&at) else {
                break;
            };
            let mut aliases: Vec<(&'src str, Id)> = scope
                .name_to_id_map
                .iter()
                .filter(|(_, id)| self.modules.contains_key(*id))
                .map(|(alias, id)| (*alias, *id))
                .collect();
            aliases.sort_by_key(|(alias, id)| (*alias, id.0));
            for (alias, module_id) in aliases {
                if !seen.insert(alias) {
                    continue;
                }
                if let Some(path) = self.reached_through_module(entity, name, module_id, alias, 0) {
                    return Ok(path);
                }
            }
            current = scope.parent_id;
        }
        Err(Unwritable(match self.import_path_of(entity) {
            Some(path) => format!(
                "this file can reach `{name}` neither by name nor through a module it imports \
                 (`import {path};` would let it)"
            ),
            None => format!("this file cannot reach `{name}`"),
        }))
    }

    /// `alias::name` (or `alias::child::name`) when the module `module_id`
    /// exports `entity` under `name`, directly or one child module down.
    fn reached_through_module(
        &self,
        entity: Id,
        name: &str,
        module_id: Id,
        alias: &str,
        depth: usize,
    ) -> Option<String> {
        let module = self.modules.get(&module_id)?;
        let scope_id = module.body.1;
        let scope = self.scopes.get(&scope_id)?;
        if scope.name_to_id_map.get(name) == Some(&entity) && self.is_exported_in(entity, scope_id)
        {
            return Some(format!("{alias}::{name}"));
        }
        if depth >= 1 {
            return None;
        }
        let children = self
            .module_children_scopes
            .get(&module_id)
            .and_then(|children| self.scopes.get(children))?;
        let mut names: Vec<(&str, Id)> = children
            .name_to_id_map
            .iter()
            .filter(|(_, id)| self.modules.contains_key(*id))
            .map(|(child, id)| (*child, *id))
            .collect();
        names.sort_by_key(|(child, id)| (*child, id.0));
        names.into_iter().find_map(|(child, child_id)| {
            self.reached_through_module(
                entity,
                name,
                child_id,
                &format!("{alias}::{child}"),
                depth + 1,
            )
        })
    }
}

impl<'src> Analyzer<'src> {
    /// E278 (`type-ascription.md` §11): a hint per STAGE of every chain split
    /// one stage per line in the package's own files — the stage's type as
    /// hover spells it, E227's abbreviation where a `[hint]` gives one, and
    /// what "Ascribe this stage" writes.
    ///
    /// A stage hints when a line break stands between it and what it is
    /// called on, and its own text ends its line (a `;` or `,` may follow).
    /// The chain's HEAD hints by the same rule when the first link starts a
    /// new line. A stage already ascribed hints nothing, as an annotated
    /// binding gets none, and neither does a value landing where its type is
    /// already written (an annotated `let`'s initializer).
    pub(super) fn stage_hints(&mut self) -> Vec<StageHint> {
        let stages = std::mem::take(&mut self.chain_stages);
        let ascribed: HashSet<Id> = self
            .ascriptions
            .values()
            .map(|site| site.value_id)
            .collect();
        let mut hinted: Vec<(Id, Id, Span)> = Vec::new();
        let mut seen: HashSet<Id> = HashSet::default();
        for stage in &stages {
            let Some(source) = self.source_of_id(stage.id) else {
                continue;
            };
            if self.std_sources.contains(&source) {
                continue;
            }
            let Some(text) = self.source_text(source) else {
                continue;
            };
            let split = text
                .get(stage.subject_end..stage.member_start)
                .is_some_and(|gap| gap.contains('\n'));
            if !split {
                continue;
            }
            // The head, when the first link of the chain breaks onto a new
            // line after it.
            if stage.subject_id == stage.head_id
                && let Some(head_span) = self.span_map.get(&stage.head_id).map(|span| **span)
                && ends_its_line(text, head_span.end)
                && seen.insert(stage.head_id)
            {
                hinted.push((stage.head_id, stage.head_id, head_span));
            }
            if ends_its_line(text, stage.span.end) && seen.insert(stage.id) {
                hinted.push((stage.id, stage.head_id, stage.span));
            }
        }
        self.chain_stages = stages;
        let mut admitted: HashMap<TypeId, bool> = HashMap::default();
        let mut hints = Vec::with_capacity(hinted.len());
        for (id, chain, span) in hinted {
            if ascribed.contains(&id) || self.annotated_landings.contains(&id) {
                continue;
            }
            if matches!(self.expr_id_to_expr_map.get(&id), Some(Expr::Ascribe(_))) {
                continue;
            }
            // A call's type is not tabled (only its callee's return is), so a
            // stage is asked of the settled solver; a binding read answers
            // through its declaration.
            let type_ = match self.place_value_type_id(id) {
                Some(type_id) => type_id.get_type(self),
                None => {
                    // A reading aid asks; it never reports. Whatever the
                    // settled solver re-says about a broken stage was said
                    // when it was checked, so it is rolled back here.
                    let (diagnostics, marks) =
                        (self.diagnostics.len(), self.diagnostic_source_marks.len());
                    let type_ = self.infer_type(id, &Type::Unknown, &HashMap::default());
                    self.diagnostics.truncate(diagnostics);
                    self.diagnostic_source_marks.truncate(marks);
                    type_
                }
            };
            if matches!(
                type_,
                Type::Unknown | Type::Unresolved | Type::Void | Type::Never | Type::Any
            ) {
                continue;
            }
            let full = self.pretty_print_type(&type_, &SubstitutionContext::default());
            let abbreviated = if self.mentions_a_hinted_type(&type_, 0) {
                let mut hosts = Vec::new();
                let label = self.render_hint_label(&type_, &mut admitted, &mut hosts, 0);
                (!hosts.is_empty()).then_some(label)
            } else {
                None
            };
            let scope_id = self.expr_id_to_scope_id_map.get(&id).copied();
            let written = match scope_id {
                Some(scope_id) => self
                    .written_spelling(&type_, scope_id, true, &mut admitted)
                    .map_err(|Unwritable(reason)| reason),
                None => Err("the stage has no scope to write its type in".to_string()),
            };
            hints.push(StageHint {
                id,
                chain,
                span,
                full,
                abbreviated,
                written,
            });
        }
        hints.sort_by_key(|hint| (hint.span.start, hint.span.end));
        hints
    }
}

/// Whether the text after `end` closes its line: nothing but spaces, an
/// optional `;` or `,`, and a `//` comment before the line break or the end.
fn ends_its_line(text: &str, end: usize) -> bool {
    let Some(rest) = text.get(end..) else {
        return false;
    };
    let line = rest.split('\n').next().unwrap_or("");
    let line = line.split("//").next().unwrap_or("").trim();
    matches!(line, "" | ";" | ",")
}
