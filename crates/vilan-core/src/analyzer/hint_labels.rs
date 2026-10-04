//! `[hint(Trait<..>)]` — a type shown by the trait it is used as (E227,
//! `proposal/inlay-hint-abbreviation.md`, ruled R-d in Order 44).
//!
//! An inlay hint is a reading aid, not the type. A pipeline node's type spells
//! its whole upstream — kolt's `selected_command` hints as
//! `Map<Combine<(List<Command>, usize)>, (List<Command>, usize), Option<Command>>`
//! — and the one fact the reader wants is that it is a source of
//! `Option<Command>`. The type's AUTHOR declares the trait it is read as, with
//! one attribute on the struct or enum, written in the declaration's own
//! parameters (`[hint(Source<U>)] struct Map<S, T, U>`), and the inlay hint
//! renders `: ~Source<Option<Command>>`. `~` lexes nowhere in vilan, so the
//! abbreviation cannot be mistaken for a type a reader could write.
//!
//! Three rules, all the paper's:
//!
//! 1. **No heuristic.** A type without the attribute renders exactly as the
//!    full label does.
//! 2. **The abbreviation never promises what the value lacks.** It is printed
//!    only when THIS instantiation is admitted by an impl of the named trait
//!    application — the solver's own question — and the full type otherwise
//!    (Q3: per instantiation, at render time).
//! 3. **Recursive.** A hinted node nested anywhere abbreviates in place:
//!    `(~Source<Option<str>>, i32)` (Q2).
//!
//! Only the INLAY HINT reads the abbreviation. Hover, completion, signature
//! help and every diagnostic keep the full rendering: a diagnostic states what
//! a type IS.
//!
//! One table ([`Analyzer::hint_attributes`], filled from the declarations'
//! labels once resolution has settled) and one renderer
//! ([`Analyzer::hint_labels`], run once per analysis before the label loop,
//! for the variables whose abbreviated label differs from the full one — a
//! program with no hinted type pays one containment scan per variable).

use super::*;

/// A `[hint(..)]` as the walk banked it: the declaration, the written type
/// walked in the declaration's generic scope, where it was written.
#[derive(Clone, Debug)]
pub(super) struct PendingHint {
    pub declaration: Id,
    pub type_id: TypeId,
    pub span: Span,
    pub source: SourceId,
}

/// A resolved `[hint(..)]`: the trait and its arguments, in the declaration's
/// own generic parameters.
#[derive(Clone, Debug)]
pub(super) struct HintAttribute {
    pub trait_id: Id,
    pub arguments: Vec<TypeId>,
}

/// One variable's abbreviated inlay-hint label (E227) — stored only where it
/// DIFFERS from the full label, E206's sparse-table precedent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HintLabel {
    /// The label as the hint shows it: `~Source<Option<str>>`.
    pub label: String,
    /// The declarations whose `[hint]` the label used, outermost first, for
    /// the hover line that teaches the mapping ("`[hint]` on `Map`").
    pub hosts: Vec<String>,
}

impl<'src> Analyzer<'src> {
    /// The Struct and Enum arms' half: walk each written hint in the
    /// declaration's generic scope — a TRAIT position, so naming a trait there
    /// is the point rather than §12.2's value-position mistake — and bank it
    /// for [`Self::resolve_hint_attributes`], which runs once every name has
    /// resolved.
    pub(super) fn bank_hint_attributes(
        &mut self,
        declaration: Id,
        labels: &Labels<'src>,
        generic_scope: Id,
    ) {
        for hint in &labels.hint {
            let written: &Spanned<Node<'src>> = &hint.0;
            let type_id = self.walk_trait_position_type_node(written, generic_scope);
            self.pending_hints.push(PendingHint {
                declaration,
                type_id,
                span: written.1,
                source: self.current_source_id,
            });
        }
    }

    /// The declaration checks (the paper's §3.2), each a refusal with a steer,
    /// and the table the renderer reads. A host that is not a struct or an
    /// enum is `labels::check`'s refusal (it has no generic scope to walk the
    /// hint in, so nothing is banked for it); a free name in the argument is
    /// resolution's own "cannot find" at the name.
    ///
    /// Idempotent over `pending_hints`, which a cloned base world carries
    /// forward: the table is rebuilt from the whole list on every analysis.
    pub(super) fn resolve_hint_attributes(&mut self) {
        self.hint_attributes.clear();
        let pending = self.pending_hints.clone();
        let mut counted: HashMap<Id, usize> = HashMap::default();
        for hint in &pending {
            *counted.entry(hint.declaration).or_default() += 1;
        }
        let mut refused_twice: HashSet<Id> = HashSet::default();
        for hint in pending {
            let Some((host_name, host_type)) = self.hint_host(hint.declaration) else {
                continue;
            };
            // Refused wherever it is written, std included: a hint that
            // failed a check silently would drop out of the table, and the
            // type would hint whole with nothing to say why.
            if counted.get(&hint.declaration).copied().unwrap_or(0) > 1 {
                if refused_twice.insert(hint.declaration) {
                    self.push_in_source(
                        Error {
                            trace: Vec::new(),
                            note: None,
                            span: hint.span,
                            msg: format!(
                                "`{host_name}` carries more than one `[hint(..)]`: a type is shown \
                                 as ONE trait in an inlay hint — keep the one a reader of a \
                                 `{host_name}` value needs"
                            ),
                        },
                        hint.source,
                    );
                }
                continue;
            }
            let written = hint.type_id.get_type(self);
            let (trait_id, arguments) = match written {
                Type::Trait(trait_id, arguments) => (trait_id, arguments),
                // Resolution already refused the name; one report per spelling.
                Type::Unknown | Type::Unresolved => continue,
                other => {
                    let shown = self.pretty_print_type(&other, &SubstitutionContext::default());
                    self.push_in_source(
                        Error {
                            trace: Vec::new(),
                            note: None,
                            span: hint.span,
                            msg: format!(
                                "`[hint({shown})]` must name a trait application — the trait \
                                     a `{host_name}` is shown as in an inlay hint, written in its \
                                     own parameters (`[hint(Source<T>)]`) — and `{shown}` is not \
                                     a trait"
                            ),
                        },
                        hint.source,
                    );
                    continue;
                }
            };
            if !self.some_impl_provides(hint.declaration, &host_type, trait_id, &arguments) {
                let shown = self.pretty_print_type(
                    &Type::Trait(trait_id, arguments.clone()),
                    &SubstitutionContext::default(),
                );
                let host = self.pretty_print_type(&host_type, &SubstitutionContext::default());
                self.push_in_source(
                    Error {
                        trace: Vec::new(),
                        note: None,
                        span: hint.span,
                        msg: format!(
                            "`[hint({shown})]`: no impl of `{shown}` for `{host}` — a hint may \
                                 only name a trait application the type implements, or the \
                                 abbreviation would promise what the value lacks"
                        ),
                    },
                    hint.source,
                );
                continue;
            }
            self.hint_attributes.insert(
                hint.declaration,
                HintAttribute {
                    trait_id,
                    arguments,
                },
            );
        }
    }

    /// The hinted declaration's name, and its type under its OWN parameters
    /// (`Map<S, T, U>`), or `None` for anything that is not a struct or an
    /// enum.
    fn hint_host(&mut self, declaration: Id) -> Option<(&'src str, Type)> {
        let (name, parameters, is_struct) = if let Some(structure) = self.structs.get(&declaration)
        {
            (
                structure.name,
                structure.generic_parameter_constraint_ids.clone(),
                true,
            )
        } else {
            let enumeration = self.enums.get(&declaration)?;
            (
                enumeration.name,
                enumeration.generic_parameter_constraint_ids.clone(),
                false,
            )
        };
        let arguments: Vec<TypeId> = parameters
            .iter()
            .map(|constraint| Type::Generic(*constraint).get_type_id(self))
            .collect();
        let host = if is_struct {
            Type::Struct(declaration, arguments)
        } else {
            Type::Enum(declaration, arguments)
        };
        Some((name, host))
    }

    /// The paper's check 4, STRUCTURAL: some impl whose subject's head is
    /// this declaration provides `trait_id` at `arguments` once the impl's own
    /// binders are read as the declaration's parameters. Not a solver query —
    /// under the declaration's bare parameters every conditional impl
    /// (`impl Map<type S: Source<type T>, T, type U> with Source<U>`) would
    /// fail its bound; the bound is the per-instantiation question the
    /// renderer asks.
    fn some_impl_provides(
        &mut self,
        declaration: Id,
        host: &Type,
        trait_id: Id,
        arguments: &[TypeId],
    ) -> bool {
        let host_arguments = match host {
            Type::Struct(_, arguments) | Type::Enum(_, arguments) => arguments.clone(),
            _ => return false,
        };
        let wanted = self.pretty_print_type(
            &Type::Trait(trait_id, arguments.to_vec()),
            &SubstitutionContext::default(),
        );
        let candidates: Vec<(TypeId, Vec<TypeId>)> = self
            .implementations
            .iter()
            .filter_map(|implementation| {
                let (_, provided) = implementation
                    .provided_trait_args
                    .iter()
                    .find(|(provided, _)| *provided == trait_id)?;
                Some((implementation.subject, provided.clone()))
            })
            .collect();
        for (subject, provided) in candidates {
            let subject_arguments = match subject.get_type(self) {
                Type::Struct(id, arguments) | Type::Enum(id, arguments) if id == declaration => {
                    arguments
                }
                _ => continue,
            };
            if subject_arguments.len() != host_arguments.len() {
                continue;
            }
            // Each subject argument must be one of the impl's binders, read as
            // the declaration's parameter in that position; a concrete subject
            // argument (`impl Leaf<i32> with ..`) provides nothing a hint in
            // the declaration's own parameters can name.
            let mut substitution = SubstitutionContext::default();
            let mut binders_only = true;
            for (subject_argument, host_argument) in subject_arguments.iter().zip(&host_arguments) {
                match subject_argument.get_type(self) {
                    Type::Generic(binder) => {
                        substitution.insert(binder, *host_argument);
                    }
                    _ => binders_only = false,
                }
            }
            if !binders_only {
                continue;
            }
            let provided: Vec<TypeId> = provided
                .iter()
                .map(|argument| self.substitute_member(*argument, &substitution))
                .collect();
            let shown = self.pretty_print_type(
                &Type::Trait(trait_id, provided),
                &SubstitutionContext::default(),
            );
            if shown == wanted {
                return true;
            }
        }
        false
    }

    /// Every variable whose abbreviated inlay-hint label differs from its full
    /// one. Asked before the label loop, which borrows the analyzer
    /// immutably: admission is the solver's `&mut` question, and it is asked
    /// once per distinct hinted instantiation (the cache), not per binding.
    pub(super) fn hint_labels(&mut self) -> HashMap<Id, HintLabel> {
        let mut labels: HashMap<Id, HintLabel> = HashMap::default();
        if self.hint_attributes.is_empty() {
            return labels;
        }
        let variables: Vec<(Id, TypeId)> = self
            .variables
            .iter()
            .map(|(id, variable)| (*id, variable.type_id))
            .collect();
        let mut admitted: HashMap<TypeId, bool> = HashMap::default();
        for (id, type_id) in variables {
            let type_ = type_id.get_type(self);
            if !self.mentions_a_hinted_type(&type_, 0) {
                continue;
            }
            let mut hosts: Vec<String> = Vec::new();
            let label = self.render_hint_label(&type_, &mut admitted, &mut hosts, 0);
            if hosts.is_empty() {
                continue;
            }
            labels.insert(id, HintLabel { label, hosts });
        }
        labels
    }

    /// Q3's per-instantiation admission: some impl providing `trait_id`
    /// applies to `type_` with every bound on its binders HOLDING for what
    /// this instantiation binds them to (`impl Node<type S: Stream<type T>, ..>`
    /// over an `i32` upstream does not), and provides the trait at
    /// `required`. The subject match alone (`type_implements_trait_at`) reads
    /// the binders as unconstrained, which would abbreviate a value its impl
    /// never admits.
    fn hint_admits(&mut self, type_: &Type, trait_id: Id, required: &[TypeId]) -> bool {
        let subjects: Vec<TypeId> = self
            .implementations
            .iter()
            .filter(|implementation| {
                implementation
                    .provided_trait_args
                    .iter()
                    .any(|(provided, _)| *provided == trait_id)
                    && self.impl_subject_admits(
                        type_,
                        implementation.subject.borrow_type(self),
                        &HashMap::default(),
                    )
            })
            .map(|implementation| implementation.subject)
            .collect();
        let bounded = subjects
            .into_iter()
            .any(|subject| self.hint_bounds_hold(subject, type_));
        bounded && self.type_implements_trait_at(type_, trait_id, required)
    }

    /// Every bound on the impl's binders holds for what `subject` binds them
    /// to — POSITIVELY. The solver's own `impl_bounds_hold` is conservative on
    /// purpose (an undecided bound must not drop a method candidate: a type
    /// with no impl of the bound's trait in view answers "holds"), and an
    /// abbreviation is the opposite case — it may only be printed when the
    /// impl is known to apply. So its answer is kept, and every concrete
    /// binding must also implement each of its bound's traits.
    fn hint_bounds_hold(&mut self, impl_subject: TypeId, subject: &Type) -> bool {
        if !self.impl_bounds_hold(impl_subject, subject) {
            return false;
        }
        let declared = impl_subject.get_type(self);
        let Some((_, bindings)) = self.reconcile_declaration(&declared, subject, &declared) else {
            return true;
        };
        for (constraint, bound) in bindings {
            let bound = bound.get_type(self);
            if matches!(
                bound,
                Type::Generic(_) | Type::Unknown | Type::Unresolved | Type::Any
            ) {
                continue;
            }
            for (trait_id, _) in self.generic_bound_traits(constraint) {
                if !self.type_implements_trait(&bound, trait_id) {
                    return false;
                }
            }
        }
        true
    }

    fn mentions_a_hinted_type(&self, type_: &Type, depth: usize) -> bool {
        if depth > 24 {
            return false;
        }
        match type_ {
            Type::Struct(id, arguments) | Type::Enum(id, arguments) => {
                self.hint_attributes.contains_key(id)
                    || arguments.iter().any(|argument| {
                        self.mentions_a_hinted_type(argument.borrow_type(self), depth + 1)
                    })
            }
            Type::Tuple(items) => items
                .iter()
                .any(|item| self.mentions_a_hinted_type(item.borrow_type(self), depth + 1)),
            Type::Array(element, _) => {
                self.mentions_a_hinted_type(element.borrow_type(self), depth + 1)
            }
            Type::Closure(parameters, return_id, contexts, _) if contexts.is_empty() => parameters
                .iter()
                .chain(std::iter::once(return_id))
                .any(|part| self.mentions_a_hinted_type(part.borrow_type(self), depth + 1)),
            _ => false,
        }
    }

    /// The abbreviating renderer: `pretty_print_type`'s output, except that a
    /// hinted struct or enum whose instantiation is ADMITTED renders as
    /// `~Trait<args>` with the hint's arguments read under the instantiation,
    /// recursively. Every form it does not descend into is the full renderer's
    /// — nothing it cannot abbreviate is printed any differently.
    fn render_hint_label(
        &mut self,
        type_: &Type,
        admitted: &mut HashMap<TypeId, bool>,
        hosts: &mut Vec<String>,
        depth: usize,
    ) -> String {
        const MAX_DEPTH: usize = 24;
        if depth > MAX_DEPTH || !self.mentions_a_hinted_type(type_, 0) {
            return self.pretty_print_type(type_, &SubstitutionContext::default());
        }
        match type_ {
            Type::Struct(id, arguments) | Type::Enum(id, arguments) => {
                let (id, arguments) = (*id, arguments.clone());
                if let Some(abbreviated) =
                    self.abbreviated_node(type_, id, &arguments, admitted, hosts, depth)
                {
                    return abbreviated;
                }
                let name = self
                    .structs
                    .get(&id)
                    .map(|structure| structure.name)
                    .or_else(|| self.enums.get(&id).map(|enumeration| enumeration.name))
                    .unwrap_or("?");
                let mut rendered = name.to_string();
                self.push_rendered_arguments(&mut rendered, &arguments, admitted, hosts, depth);
                rendered
            }
            Type::Tuple(items) => {
                let items = items.clone();
                let parts: Vec<String> = items
                    .iter()
                    .map(|item| {
                        let item = item.get_type(self);
                        self.render_hint_label(&item, admitted, hosts, depth + 1)
                    })
                    .collect();
                format!("({})", parts.join(", "))
            }
            Type::Array(element, length) => {
                let (element, length) = (element.get_type(self), *length);
                let element = self.render_hint_label(&element, admitted, hosts, depth + 1);
                format!("[{element}; {length}]")
            }
            Type::Closure(parameters, return_id, _, modes) => {
                let (parameters, return_id, modes) =
                    (parameters.clone(), *return_id, modes.clone());
                let parts: Vec<String> = parameters
                    .iter()
                    .enumerate()
                    .map(|(index, parameter)| {
                        let parameter = parameter.get_type(self);
                        // B495: a view parameter's `&`/`&mut` is the type's.
                        let prefix = modes
                            .get(index)
                            .map_or("", |mode| self.resolved_parameter_mode(*mode).prefix());
                        let rendered =
                            self.render_hint_label(&parameter, admitted, hosts, depth + 1);
                        format!("{prefix}{rendered}")
                    })
                    .collect();
                let return_type = return_id.get_type(self);
                let returned = self.render_hint_label(&return_type, admitted, hosts, depth + 1);
                format!("|{}| {returned}", parts.join(", "))
            }
            other => self.pretty_print_type(other, &SubstitutionContext::default()),
        }
    }

    fn push_rendered_arguments(
        &mut self,
        rendered: &mut String,
        arguments: &[TypeId],
        admitted: &mut HashMap<TypeId, bool>,
        hosts: &mut Vec<String>,
        depth: usize,
    ) {
        if arguments.is_empty() {
            return;
        }
        let parts: Vec<String> = arguments
            .iter()
            .map(|argument| {
                let argument = argument.get_type(self);
                self.render_hint_label(&argument, admitted, hosts, depth + 1)
            })
            .collect();
        rendered.push('<');
        rendered.push_str(&parts.join(", "));
        rendered.push('>');
    }

    /// `~Trait<args>` for a hinted node whose instantiation an impl of the
    /// hinted application admits, or `None` (the full rendering stands).
    fn abbreviated_node(
        &mut self,
        type_: &Type,
        id: Id,
        arguments: &[TypeId],
        admitted: &mut HashMap<TypeId, bool>,
        hosts: &mut Vec<String>,
        depth: usize,
    ) -> Option<String> {
        let hint = self.hint_attributes.get(&id)?.clone();
        let parameters = self
            .structs
            .get(&id)
            .map(|structure| structure.generic_parameter_constraint_ids.clone())
            .or_else(|| {
                self.enums
                    .get(&id)
                    .map(|enumeration| enumeration.generic_parameter_constraint_ids.clone())
            })?;
        if parameters.len() != arguments.len() {
            return None;
        }
        let substitution: SubstitutionContext = parameters
            .iter()
            .copied()
            .zip(arguments.iter().copied())
            .collect();
        let required: Vec<TypeId> = hint
            .arguments
            .iter()
            .map(|argument| self.substitute_member(*argument, &substitution))
            .collect();
        let instantiation = type_.clone().get_type_id(self);
        let is_admitted = match admitted.get(&instantiation) {
            Some(answer) => *answer,
            None => {
                let answer = self.hint_admits(type_, hint.trait_id, &required);
                admitted.insert(instantiation, answer);
                answer
            }
        };
        if !is_admitted {
            return None;
        }
        let host = self
            .structs
            .get(&id)
            .map(|structure| structure.name)
            .or_else(|| self.enums.get(&id).map(|enumeration| enumeration.name))?;
        if !hosts.iter().any(|known| known == host) {
            hosts.push(host.to_string());
        }
        let trait_name = self.traits.get(&hint.trait_id)?.name;
        let mut rendered = format!("~{trait_name}");
        self.push_rendered_arguments(&mut rendered, &required, admitted, hosts, depth);
        Some(rendered)
    }
}
