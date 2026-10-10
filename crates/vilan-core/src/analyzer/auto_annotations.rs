//! `auto` annotations — types the toolchain writes and keeps current (B570,
//! `proposal/auto-annotations.md`, its nine questions RULED as recommended).
//!
//! **A signature, not a constraint** (Q1). For an item annotated `auto T`:
//!
//! 1. the body — a function's, or a binding's initializer — is inferred as if
//!    the annotation were absent: `T` never enters it as an expected type, so
//!    `fun f(): auto f64 { 5 }` infers `i32` and is STALE, never an `f64`;
//! 2. everything outside reads `T` (Q2): a function's callers
//!    (`infer_type_path`'s call arm), a binding's readers (`resolve_variable`
//!    hands them `T`), hover, and the S0 interface fingerprint — which renders
//!    the function's declaration label, so it reads the written `T` for a
//!    fully written item without a line of its own;
//! 3. `check` compares the inferred type with `T` (§4.1) and refuses a
//!    difference as stale, carrying the rewrite `vilan check --fix` and the
//!    editor apply.
//!
//! In a program that checks, (1) and (2) give one type, so deleting every
//! `auto` changes nothing a program does — "output only" — while the item's
//! interface is readable from the parse.
//!
//! **Long stage types** (Q3, door (b)): `auto` writes what the inlay hint
//! shows, a hinted node as its bare trait (`auto Pipe<str>`), and checks a
//! bare trait at any depth as B161 checks one at a binding — the value's own
//! type implements it — and equality everywhere else. Such an item is no
//! firewall: its callers keep reading the concrete inferred type.
//!
//! **A bare `auto`** (Q4) promised nothing, so it is a WARNING with the fill
//! as its fix, and its readers read the inferred type.

use super::written_types::Unwritable;
use super::*;

impl<'src> Analyzer<'src> {
    /// Records an `auto` annotation at the walk (B570 §2): the written type,
    /// when there is one, is walked where the annotation stands with every
    /// position in it a TRAIT position — a bare trait there is door (b)'s
    /// spelling, checked against the inferred type rather than refused as a
    /// value-position trait.
    pub(super) fn record_auto_annotation(
        &mut self,
        owner: AutoOwner,
        name: &'src str,
        span: Span,
        written: Option<&'src Spanned<Node<'src>>>,
        scope_id: Id,
    ) {
        let written = written.map(|written| {
            let type_id = self.walk_type_node(written, scope_id);
            self.mark_trait_positions(type_id);
            type_id
        });
        let owner_id = match owner {
            AutoOwner::Return(id) | AutoOwner::Binding(id) | AutoOwner::Ascription(id) => id,
        };
        if let Some(written) = written {
            self.auto_written.insert(owner_id, written);
        }
        self.auto_annotations.push(AutoAnnotation {
            owner,
            name,
            span,
            written,
            scope_id,
        });
    }

    /// `type_id` and every slot written inside it read a trait name as the
    /// trait itself (`trait_position_type_ids`), as `[hint(..)]` does.
    fn mark_trait_positions(&mut self, type_id: TypeId) {
        let mut pending = vec![type_id];
        let mut seen: HashSet<TypeId> = HashSet::default();
        while let Some(slot) = pending.pop() {
            if !seen.insert(slot) {
                continue;
            }
            self.trait_position_type_ids.insert(slot);
            pending.extend(self.annotation_child_type_ids(slot));
        }
    }

    /// The written `T` of `owner`'s `auto T` when it is FULLY written — no
    /// bare trait and no hole anywhere in it — which is when its readers read
    /// it (§3, §6). `None` for an unannotated owner, a bare `auto` and a
    /// door-(b) type.
    pub(super) fn auto_written_type(&self, owner: Id) -> Option<Type> {
        self.auto_written_type_id(owner)
            .map(|type_id| type_id.get_type(self))
    }

    /// [`Self::auto_written_type`]'s slot.
    pub(super) fn auto_written_type_id(&self, owner: Id) -> Option<TypeId> {
        if self.auto_written.is_empty() {
            return None;
        }
        let written = *self.auto_written.get(&owner)?;
        self.fully_written(&written.get_type(self), 0)
            .then_some(written)
    }

    fn fully_written(&self, type_: &Type, depth: usize) -> bool {
        if depth > 24 {
            return false;
        }
        let all = |ids: &[TypeId]| {
            ids.iter()
                .all(|id| self.fully_written(id.borrow_type(self), depth + 1))
        };
        match type_ {
            Type::Trait(..) | Type::Unknown | Type::Unresolved => false,
            Type::Struct(_, arguments) | Type::Enum(_, arguments) | Type::Dyn(_, arguments) => {
                all(arguments)
            }
            Type::Tuple(items, _) => all(items),
            Type::Array(element, _) => self.fully_written(element.borrow_type(self), depth + 1),
            Type::Closure(parameters, return_id, _, _) => {
                all(parameters) && self.fully_written(return_id.borrow_type(self), depth + 1)
            }
            _ => true,
        }
    }

    /// Whether the inferred type agrees with the written `auto` type (§4.1):
    /// equal resolved forms, a bare trait anywhere in the written type met by
    /// a type that implements it (door (b), B161's reading), and a hole left
    /// in the inferred type met by whatever the written type says there — the
    /// lock is what decides it (u03).
    fn auto_agrees(&mut self, written: &Type, inferred: &Type, depth: usize) -> bool {
        if depth > 24 {
            return true;
        }
        if matches!(inferred, Type::Unknown | Type::Unresolved) {
            return true;
        }
        let pairwise = |analyzer: &mut Self, written: &[TypeId], inferred: &[TypeId]| {
            written.len() == inferred.len()
                && written.iter().zip(inferred).all(|(written, inferred)| {
                    let written = written.get_type(analyzer);
                    let inferred = inferred.get_type(analyzer);
                    analyzer.auto_agrees(&written, &inferred, depth + 1)
                })
        };
        match (written, inferred) {
            (Type::Trait(trait_id, arguments), _) => {
                let (trait_id, arguments) = (*trait_id, arguments.clone());
                self.satisfies_trait_bound(inferred, trait_id, &arguments, 0)
            }
            (Type::Struct(a, written), Type::Struct(b, inferred))
            | (Type::Enum(a, written), Type::Enum(b, inferred))
            | (Type::Dyn(a, written), Type::Dyn(b, inferred)) => {
                a == b && pairwise(self, &written.clone(), &inferred.clone())
            }
            // Labels are no part of a tuple's type (B569), so they agree as
            // the language reconciles them — by position, a label set and its
            // absence, two disjoint sets — except a label both carry at
            // different slots: callers reading the written labels would name
            // the slot the body put elsewhere.
            (Type::Tuple(written, written_labels), Type::Tuple(inferred, inferred_labels)) => {
                written_labels.contradiction(inferred_labels).is_none()
                    && pairwise(self, &written.clone(), &inferred.clone())
            }
            (Type::Array(written, n), Type::Array(inferred, m)) => {
                n == m && pairwise(self, &[*written], &[*inferred])
            }
            (
                Type::Closure(written_parameters, written_return, _, written_modes),
                Type::Closure(inferred_parameters, inferred_return, _, inferred_modes),
            ) => {
                let modes_agree = written_modes.len() == inferred_modes.len()
                    && written_modes
                        .iter()
                        .zip(inferred_modes)
                        .all(|(written, inferred)| {
                            self.resolved_parameter_mode(*written)
                                == self.resolved_parameter_mode(*inferred)
                        });
                let (written_parameters, inferred_parameters) =
                    (written_parameters.clone(), inferred_parameters.clone());
                let (written_return, inferred_return) = (*written_return, *inferred_return);
                modes_agree
                    && pairwise(self, &written_parameters, &inferred_parameters)
                    && pairwise(self, &[written_return], &[inferred_return])
            }
            (Type::Generic(a), Type::Generic(b)) => a == b,
            (written, inferred) => written == inferred,
        }
    }

    /// B570 §4: every `auto` annotation against what its owner inferred, once
    /// the program has settled. A stale written type is an ERROR carrying its
    /// rewrite; a bare `auto` a WARNING carrying its fill; an inferred type
    /// that is still a hole is the unannotated item's own business (it reports
    /// it), neither filled nor stale.
    pub(super) fn check_auto_annotations(&mut self) {
        let annotations = std::mem::take(&mut self.auto_annotations);
        let mut admitted: HashMap<TypeId, bool> = HashMap::default();
        for annotation in &annotations {
            let (owner_id, inferred) = match annotation.owner {
                AutoOwner::Return(function_id) => {
                    (function_id, self.inferred_return_type_of(function_id))
                }
                AutoOwner::Binding(variable_id) => {
                    let inferred = match self.auto_inferred.get(&variable_id) {
                        Some(type_id) => type_id.get_type(self),
                        None => match self.variables.get(&variable_id) {
                            Some(variable) => variable.type_id.get_type(self),
                            None => continue,
                        },
                    };
                    (variable_id, inferred)
                }
                AutoOwner::Ascription(ascription_id) => {
                    match self.auto_inferred.get(&ascription_id) {
                        Some(type_id) => (ascription_id, type_id.get_type(self)),
                        None => continue,
                    }
                }
            };
            if matches!(inferred, Type::Unknown | Type::Unresolved | Type::Any) {
                continue;
            }
            let name = annotation.name;
            let shown = self.pretty_print_type(&inferred, &SubstitutionContext::default());
            // An ascription names its stage when it is one, as B571's
            // mismatch does (§8), else "the value".
            let stage = match annotation.owner {
                AutoOwner::Ascription(id) => self.ascriptions.get(&id).and_then(|site| site.stage),
                _ => None,
            };
            let what = match (annotation.owner, stage) {
                (AutoOwner::Return(_), _) => format!("`{name}` returns `{shown}`"),
                (AutoOwner::Binding(_), _) => format!("`{name}` is `{shown}`"),
                (AutoOwner::Ascription(_), Some((stage, _))) => {
                    format!("`.{stage}()` returns `{shown}`")
                }
                (AutoOwner::Ascription(_), None) => format!("the value is `{shown}`"),
            };
            let spelled = match inferred {
                Type::Never => Err(Unwritable(
                    "`never` is not a type a file can write".to_string(),
                )),
                _ => self.written_spelling(&inferred, annotation.scope_id, true, &mut admitted),
            };
            let fix = match &spelled {
                Ok(spelling) => format!("{AUTO_REWRITE_MARK}auto {spelling}`"),
                Err(Unwritable(reason)) => format!(" — write the type by hand: {reason}"),
            };
            match annotation.written {
                None => {
                    self.warnings.push(Error {
                        trace: Vec::new(),
                        note: None,
                        span: annotation.span,
                        msg: format!("unfilled `auto`: {what}{fix}"),
                    });
                    self.warning_sources
                        .push(self.source_of_id(owner_id).unwrap_or(SourceId(0)));
                }
                Some(written_id) => {
                    let written = written_id.get_type(self);
                    if self.auto_agrees(&written, &inferred, 0) {
                        continue;
                    }
                    let written_shown =
                        self.pretty_print_type(&written, &SubstitutionContext::default());
                    let readers = match annotation.owner {
                        AutoOwner::Return(function_id) => {
                            let callers = self.call_sites_of(function_id);
                            match callers {
                                0 => String::new(),
                                1 => ", and its 1 caller was checked against it".to_string(),
                                many => format!(", and its {many} callers were checked against it"),
                            }
                        }
                        AutoOwner::Binding(_) => {
                            ", and its uses were checked against it".to_string()
                        }
                        AutoOwner::Ascription(_) => {
                            ", and the chain after it was checked against it".to_string()
                        }
                    };
                    let now = match (annotation.owner, stage) {
                        (AutoOwner::Return(_), _) => format!("`{name}` now returns `{shown}`"),
                        (AutoOwner::Binding(_), _) => format!("`{name}` is now `{shown}`"),
                        (AutoOwner::Ascription(_), Some((stage, _))) => {
                            format!("`.{stage}()` now returns `{shown}`")
                        }
                        (AutoOwner::Ascription(_), None) => format!("the value is now `{shown}`"),
                    };
                    self.push_anchored(
                        Error {
                            trace: Vec::new(),
                            note: None,
                            span: annotation.span,
                            msg: format!(
                                "stale `auto`: {now}, not the written `auto {written_shown}`{readers}{fix}"
                            ),
                        },
                        owner_id,
                    );
                }
            }
        }
        self.auto_annotations = annotations;
    }

    /// How many calls name `function_id` directly — the callers a stale
    /// `auto` return was read by.
    fn call_sites_of(&self, function_id: Id) -> usize {
        self.function_calls
            .values()
            .filter(|call| {
                matches!(
                    self.expr_id_to_expr_map.get(&call.subject_id),
                    Some(Expr::Local(callee)) if *callee == function_id
                )
            })
            .count()
    }
}

impl<'src> Analyzer<'src> {
    /// B570 S3: the `: auto T` each unannotated return and `let` binding of
    /// the package's own files would take — what the editor's "Add `auto`
    /// type" writes — built on every analysis for every id (no per-module
    /// record takes part). A void return, a type still a hole and a type the
    /// file cannot name offer nothing.
    ///
    /// B570 S4: under the entry package's `[check] auto` opt-in, each fill an
    /// item the setting covers takes is also a WARNING carrying it, so `vilan
    /// check --fix` and the on-save action write them (§8): `"exported"` —
    /// a return or module binding reachable from outside its module (Q6
    /// RULED); `"all"` — every return and module binding. A local never.
    ///
    /// `table` is whether a front end reads the fills
    /// (`Workspace::reading_aids`): without it only the opt-in's warnings are
    /// written — nothing at all when the package has not opted in — and no
    /// point the opt-in does not cover is inferred or spelled.
    pub(super) fn auto_fills(
        &mut self,
        opt_in: crate::manifest::AutoOptIn,
        table: bool,
    ) -> Vec<AutoFill> {
        if !table && opt_in == crate::manifest::AutoOptIn::Off {
            return Vec::new();
        }
        // The inherent methods of each declared type, for Q6's third clause.
        let mut inherent_owner: HashMap<Id, TypeId> = HashMap::default();
        if opt_in != crate::manifest::AutoOptIn::Off {
            for implementation in &self.implementations {
                if !implementation.trait_ids.is_empty() {
                    continue;
                }
                for (_, member_id) in &implementation.declared_members {
                    inherent_owner.insert(
                        self.resolve_member_function_id(*member_id),
                        implementation.subject,
                    );
                }
            }
        }
        let mut points: Vec<(Id, (Span, usize))> = self.auto_fill_points.clone();
        // A point a re-walk met twice is one point.
        points.sort_by_key(|(id, _)| id.0);
        points.dedup_by_key(|(id, _)| *id);
        let mut admitted: HashMap<TypeId, bool> = HashMap::default();
        let mut fills = Vec::new();
        for (id, (name, at)) in points {
            if self
                .source_of_id(id)
                .is_none_or(|source| self.std_sources.contains(&source))
            {
                continue;
            }
            let Some(scope_id) = self.expr_id_to_scope_id_map.get(&id).copied() else {
                continue;
            };
            let covered = self.auto_opt_in_covers(id, scope_id, opt_in, &inherent_owner);
            if !table && covered.is_none() {
                continue;
            }
            crate::counters::count_reading_aid();
            let inferred = if self.functions.contains_key(&id) {
                // A reading aid asks; it never reports (`stage_hints`' rule).
                let (diagnostics, marks) =
                    (self.diagnostics.len(), self.diagnostic_source_marks.len());
                let inferred = self.inferred_return_type_of(id);
                self.diagnostics.truncate(diagnostics);
                self.diagnostic_source_marks.truncate(marks);
                inferred
            } else if let Some(variable) = self.variables.get(&id) {
                variable.type_id.get_type(self)
            } else {
                continue;
            };
            if matches!(
                inferred,
                Type::Unknown | Type::Unresolved | Type::Void | Type::Never | Type::Any
            ) {
                continue;
            }
            let Ok(spelling) = self.written_spelling(&inferred, scope_id, true, &mut admitted)
            else {
                continue;
            };
            let text = format!(": auto {spelling}");
            if let Some(covered) = covered {
                self.warnings.push(Error {
                    trace: Vec::new(),
                    note: None,
                    span: Span::from(at..at),
                    msg: format!(
                        "{covered} is inferred, and this package asks for its `auto` \
                         (`[check] auto = \"{}\"`){AUTO_REWRITE_MARK}{text}`",
                        match opt_in {
                            crate::manifest::AutoOptIn::All => "all",
                            _ => "exported",
                        }
                    ),
                });
                self.warning_sources
                    .push(self.source_of_id(id).unwrap_or(SourceId(0)));
            }
            if table {
                fills.push(AutoFill { id, name, at, text });
            }
        }
        fills
    }

    /// What the opt-in names an item as when it covers it — "`load`'s
    /// return", "`names`'s type" — or `None` (B570 S4, §8, Q6 RULED).
    fn auto_opt_in_covers(
        &self,
        id: Id,
        scope_id: Id,
        opt_in: crate::manifest::AutoOptIn,
        inherent_owner: &HashMap<Id, TypeId>,
    ) -> Option<String> {
        use crate::manifest::AutoOptIn;
        if opt_in == AutoOptIn::Off {
            return None;
        }
        let module_level = |scope: Id| {
            self.module_scope_ids.contains(&scope)
                || self
                    .scopes
                    .get(&scope)
                    .is_some_and(|scope| scope.parent_id.is_none())
        };
        if let Some(function) = self.functions.get(&id) {
            let name = function.name;
            let exported = match inherent_owner.get(&id) {
                // An inherent method is reached through its type.
                Some(subject) => match subject.get_type(self) {
                    Type::Struct(declaration, _) | Type::Enum(declaration, _) => self
                        .expr_id_to_scope_id_map
                        .get(&declaration)
                        .is_some_and(|scope| self.is_exported_in(declaration, *scope)),
                    _ => false,
                },
                None if module_level(scope_id) => self.is_exported_in(id, scope_id),
                None => return None,
            };
            return (opt_in == AutoOptIn::All || exported).then(|| format!("`{name}`'s return"));
        }
        let variable = self.variables.get(&id)?;
        if !module_level(scope_id) {
            return None;
        }
        (opt_in == AutoOptIn::All || self.is_exported_in(id, scope_id))
            .then(|| format!("`{}`'s type", variable.name))
    }
}
