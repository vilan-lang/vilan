//! The editor's declaration context (Order 45's hover arc): what a method
//! BELONGS to, what a parameter's receiving convention is, and what a value's
//! type IS.
//!
//! Three answers, each rendered once per analysis while the analyzer still
//! holds the type tables, because a [`Program`] keeps no generic names and so
//! cannot print a type after the fact:
//!
//! - **E235.** [`Parameter::signature_label`] — the ONE parameter rendering
//!   every signature label shares: `own x: T`, `x: &T`, `x: &mut T`, the
//!   receiver's `self` / `own self` / `&self` / `&mut self`, and the `lazy` and
//!   `...` markers. The signature printer dropped every convention, so a
//!   consuming `own self` method hovered as `self` and `List::push` hovered as
//!   `push(self, item: T)` — and the conformance steer handed a trait's
//!   implementor a declaration that would not have conformed.
//! - **E238** (ruled door (b)). [`Analyzer::member_headers`] — the declaration
//!   a member is written in, as one line of real syntax: `impl Memo<type K:
//!   Hashable, type V>`, `impl MemoCell<type T> with Source<T>`, a blanket's
//!   `impl type S: Source<type T>`, a trait's own `trait Flow<T>`. Hover and
//!   completion's detail print it on its own line above the `fun` line.
//! - **E237** (ruled). [`Analyzer::type_definitions`] — the definition of a
//!   value's type, one level deep, for the second block under a variable's or
//!   a field's `name: Type` line: a struct's fields or an enum's variants under
//!   the instantiation's arguments, a trait-typed or `dyn` value's REQUIRED
//!   members, capped at [`DEFINITION_MEMBER_CAP`] members then `…`.

use super::*;

/// How many members a definition block lists before it ends with `…` (E237:
/// "about 12 members"). A glance, not a dump: the declaration is one
/// go-to-definition away.
pub const DEFINITION_MEMBER_CAP: usize = 12;

/// The definition blocks one analysis rendered (E237), keyed by the TYPE ID a
/// binding, parameter, member read or field declares — the key the editor
/// already holds for each of those positions, so a hover is a lookup.
///
/// Rendered for the ENTRY file's positions only, because a hover's entity
/// table is the entry's (M27), and deduplicated by rendered type: two
/// bindings of `User` share one block.
#[derive(Debug, Clone, Default)]
pub struct TypeDefinitions {
    blocks: Vec<String>,
    by_type: HashMap<TypeId, u32>,
}

impl TypeDefinitions {
    /// The block rendered for `type_id`, if its type has one (a struct with
    /// fields, an enum with variants, a trait-typed or `dyn` value).
    pub fn of(&self, type_id: TypeId) -> Option<&str> {
        let index = *self.by_type.get(&type_id)?;
        self.blocks.get(index as usize).map(String::as_str)
    }
}

/// One PATTERN position the entry matches (E241), as `resolve_pattern` saw
/// it: where its name is, what it is, and the matched value's type.
#[derive(Debug, Clone)]
pub(super) struct PatternSite {
    name_span: Span,
    kind: PatternSiteKind,
    /// The matched value's type — for a variant, only when it IS the enum.
    matched: Option<TypeId>,
}

#[derive(Debug, Clone)]
enum PatternSiteKind {
    /// A variant: the enum, the variant, its payload under the arguments.
    Variant {
        enum_id: Id,
        variant_index: usize,
        payload: Vec<TypeId>,
    },
    /// `_`: matches the value whole and binds nothing.
    Wildcard,
}

/// A pattern position's hover (E241): where its name is, what it renders
/// as — `Option<i32>::Some(i32)`, `_: Option<i32>` — and, for a variant, the
/// enum and variant it names, so the editor can read the variant's `///`.
#[derive(Debug, Clone)]
pub struct PatternLabel {
    pub name_span: Span,
    pub label: String,
    pub variant: Option<(Id, usize)>,
    /// Whether the label names the matched value's own type (with its
    /// arguments) rather than the bare enum.
    pub matched: bool,
}

/// A type-position name's hover (E240), for the two kinds the plain label
/// answered with a bare name: a TYPE PARAMETER (`type I: Read<U>`, and the
/// declaration that introduces it) and a TRAIT named in a bound, a `with`
/// clause or a supertrait list (its required members, as E237's block).
#[derive(Debug, Clone)]
pub struct ReferenceHover {
    pub span: Span,
    /// The fenced line or block.
    pub code: String,
    /// The declaration a type parameter belongs to, as written.
    pub declared_by: Option<String>,
}

impl Parameter<'_> {
    /// One parameter as a signature writes it (E235): the receiver in its
    /// convention's own form (`self`, `own self`, `&self`, `&mut self`), any
    /// other parameter as `own x: T`, `x: &T`, `x: &mut T` or `x: T`, with the
    /// `lazy` and spread (`...`) markers that are part of the contract (lazy.md
    /// §5, variadic-generics.md §S). `mut x` is not part of the signature —
    /// conformance compares conventions only — so it is never rendered.
    ///
    /// The one rendering: the analyzer's declaration labels, its conformance
    /// steer, and the language server's parameter hover all print through it,
    /// so a convention cannot be shown in one and dropped in another again.
    pub fn signature_label(&self, type_label: &str) -> String {
        let lazy = if self.lazy { "lazy " } else { "" };
        if self.name == "self" {
            let receiver = match self.convention {
                Convention::Bare => "self",
                Convention::Own => "own self",
                Convention::Ref => "&self",
                Convention::RefMut => "&mut self",
            };
            return format!("{lazy}{receiver}");
        }
        if self.spread {
            return format!("{lazy}...{}: {type_label}", self.name);
        }
        let name = self.name;
        match self.convention {
            Convention::Bare => format!("{lazy}{name}: {type_label}"),
            Convention::Own => format!("{lazy}own {name}: {type_label}"),
            Convention::Ref => format!("{lazy}{name}: &{type_label}"),
            Convention::RefMut => format!("{lazy}{name}: &mut {type_label}"),
        }
    }
}

impl<'src> Analyzer<'src> {
    /// E238: every member's declaring block, and that block's header line.
    ///
    /// Two tables rather than a string per member: a member is a function id
    /// and a block holds many, so the owners map is a pair of ids per member
    /// and the header is rendered once per block. Every block in the world is
    /// rendered, because a hover on `memo.get_or_insert(..)` asks about std's
    /// block, not the entry's — the same reach `declaration_labels` has.
    pub(super) fn member_headers(&self) -> (HashMap<Id, Id>, HashMap<Id, String>) {
        let mut owners: HashMap<Id, Id> = HashMap::default();
        let mut headers: HashMap<Id, String> = HashMap::default();
        for implementation in &self.implementations {
            headers.insert(
                implementation.impl_id,
                self.impl_header_label(implementation),
            );
            for (_, member_id) in &implementation.declared_members {
                owners.insert(
                    self.resolve_member_function_id(*member_id),
                    implementation.impl_id,
                );
            }
        }
        for (trait_id, trait_) in &self.traits {
            headers.insert(*trait_id, self.trait_header_label(trait_));
            for (_, member_id) in &trait_.declared_members {
                owners.insert(self.resolve_member_function_id(*member_id), *trait_id);
            }
        }
        (owners, headers)
    }

    /// `impl Subject with A + B` as the block declares it: the subject's
    /// binders spelled `type X` (with their bounds) where they are introduced,
    /// and by name after that, so the line is one an author could write back.
    fn impl_header_label(&self, implementation: &Implementation<'src>) -> String {
        let mut introduced: Vec<TypeId> = Vec::new();
        let mut out = String::from("impl ");
        out.push_str(&self.binder_type_label(implementation.subject, &mut introduced, 0));
        let traits: Vec<String> = implementation
            .trait_ids
            .iter()
            .filter_map(|trait_id| {
                let trait_ = self.traits.get(trait_id)?;
                let arguments = implementation
                    .trait_args
                    .iter()
                    .find(|(provided, _)| provided == trait_id)
                    .map(|(_, arguments)| arguments.as_slice())
                    .unwrap_or_default();
                let mut label = trait_.name.to_string();
                if !arguments.is_empty() {
                    let rendered: Vec<String> = arguments
                        .iter()
                        .map(|argument| self.binder_type_label(*argument, &mut introduced, 1))
                        .collect();
                    label.push_str(&format!("<{}>", rendered.join(", ")));
                }
                Some(label)
            })
            .collect();
        if !traits.is_empty() {
            out.push_str(" with ");
            out.push_str(&traits.join(" + "));
        }
        out
    }

    /// `trait Name<P> with Super<P>` as the trait declares it: each parameter
    /// with its bound or its default (`trait Add<B = Self>`), and the
    /// supertraits it is written `with`.
    fn trait_header_label(&self, trait_: &Trait<'src>) -> String {
        let mut out = format!("trait {}", trait_.name);
        let declared = self.declared_generic_parameters.get(&trait_.id);
        let parameters: Vec<String> = trait_
            .generic_parameter_constraint_ids
            .iter()
            .enumerate()
            .map(|(index, constraint_id)| {
                let name = trait_
                    .generic_parameter_names
                    .get(index)
                    .copied()
                    .or_else(|| self.generic_constraint_names.get(constraint_id).copied())
                    .unwrap_or("_");
                let written = declared.and_then(|declared| declared.get(index));
                match constraint_id.borrow_type(self) {
                    // A `= Self` default resolves to the trait's own abstract
                    // type; it is written, and read, as `Self` (E128).
                    _ if written.is_some_and(|written| written.has_default) => {
                        let default = match constraint_id.borrow_type(self) {
                            Type::Trait(id, arguments)
                                if *id == trait_.id && arguments.is_empty() =>
                            {
                                "Self".to_string()
                            }
                            _ => self.declaration_type_label(*constraint_id),
                        };
                        format!("{name} = {default}")
                    }
                    Type::Trait(..) => {
                        format!("{name}: {}", self.declaration_type_label(*constraint_id))
                    }
                    _ => name.to_string(),
                }
            })
            .collect();
        if !parameters.is_empty() {
            out.push_str(&format!("<{}>", parameters.join(", ")));
        }
        if !trait_.supertraits.is_empty() {
            let supertraits: Vec<String> = trait_
                .supertraits
                .iter()
                .map(|supertrait| {
                    self.supertrait_label(trait_, *supertrait, &SubstitutionContext::default())
                })
                .collect();
            out.push_str(" with ");
            out.push_str(&supertraits.join(" + "));
        }
        out
    }

    /// One supertrait of `trait_` as its `with` clause reads, under
    /// `substitution`: an argument that is the trait's OWN abstract type —
    /// `trait PartialOrd<B = Self> with PartialEq<B>`, whose `B` defaults to
    /// `Self` — renders `Self` (E128's rule), not the trait's name.
    fn supertrait_label(
        &self,
        trait_: &Trait<'src>,
        supertrait: TypeId,
        substitution: &SubstitutionContext,
    ) -> String {
        let Type::Trait(supertrait_id, arguments) = supertrait.borrow_type(self) else {
            return self.pretty_print_type(supertrait.borrow_type(self), substitution);
        };
        let name = self
            .traits
            .get(supertrait_id)
            .map_or("?", |found| found.name);
        if arguments.is_empty() {
            return name.to_string();
        }
        let rendered: Vec<String> = arguments
            .iter()
            .map(|argument| match argument.borrow_type(self) {
                Type::Trait(id, own) if *id == trait_.id && own.is_empty() => "Self".to_string(),
                argument_type => self.pretty_print_type(argument_type, substitution),
            })
            .collect();
        format!("{name}<{}>", rendered.join(", "))
    }

    /// A type in an impl HEAD: a generic binder is spelled `type X` (with its
    /// bound, itself rendered in head terms) the first time it appears and by
    /// name after that; everything without a binder in it renders as any type
    /// label does.
    fn binder_type_label(
        &self,
        type_id: TypeId,
        introduced: &mut Vec<TypeId>,
        depth: usize,
    ) -> String {
        const MAX_DEPTH: usize = 24;
        if depth > MAX_DEPTH {
            return "…".to_string();
        }
        let type_ = type_id.borrow_type(self);
        let mut generics = Vec::new();
        self.collect_head_generics(type_, 0, &mut generics);
        let opens_a_binder = match type_ {
            Type::Generic(constraint_id) => !introduced.contains(constraint_id),
            _ => generics.iter().any(|generic| !introduced.contains(generic)),
        };
        if !opens_a_binder {
            return self.declaration_type_label(type_id);
        }
        let arguments = |arguments: &[TypeId], introduced: &mut Vec<TypeId>| -> String {
            if arguments.is_empty() {
                return String::new();
            }
            let rendered: Vec<String> = arguments
                .iter()
                .map(|argument| self.binder_type_label(*argument, introduced, depth + 1))
                .collect();
            format!("<{}>", rendered.join(", "))
        };
        match type_ {
            Type::Generic(constraint_id) => {
                introduced.push(*constraint_id);
                let name = self
                    .generic_constraint_names
                    .get(constraint_id)
                    .copied()
                    .unwrap_or("_");
                match constraint_id.borrow_type(self) {
                    Type::Trait(..) => format!(
                        "type {name}: {}",
                        self.binder_type_label(*constraint_id, introduced, depth + 1)
                    ),
                    _ => format!("type {name}"),
                }
            }
            Type::Struct(id, type_arguments) => {
                let name = self.structs.get(id).map_or("?", |struct_| struct_.name);
                format!("{name}{}", arguments(type_arguments, introduced))
            }
            Type::Enum(id, type_arguments) => {
                let name = self.enums.get(id).map_or("?", |enum_| enum_.name);
                format!("{name}{}", arguments(type_arguments, introduced))
            }
            Type::Trait(id, type_arguments) => {
                let name = self.traits.get(id).map_or("?", |trait_| trait_.name);
                format!("{name}{}", arguments(type_arguments, introduced))
            }
            Type::Dyn(id, type_arguments) => {
                let name = self.traits.get(id).map_or("?", |trait_| trait_.name);
                format!("dyn {name}{}", arguments(type_arguments, introduced))
            }
            Type::Tuple(items, labels) => {
                let rendered: Vec<String> = items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        let rendered = self.binder_type_label(*item, introduced, depth + 1);
                        match labels.get(index) {
                            Some(label) => format!("{label}: {rendered}"),
                            None => rendered,
                        }
                    })
                    .collect();
                format!("({})", rendered.join(", "))
            }
            Type::Array(element, length) => format!(
                "[{}; {length}]",
                self.binder_type_label(*element, introduced, depth + 1)
            ),
            // A closure, a mapped type: no head writes a binder inside one
            // that the plain label would not already name.
            _ => self.declaration_type_label(type_id),
        }
    }

    /// E241: bank one variant pattern for its hover. `span` is the whole
    /// path's (`Option::None`); the label belongs to the LAST segment, the
    /// variant's own name, which is what a caret on `None` is on.
    pub(super) fn record_pattern_variant(
        &mut self,
        span: Span,
        path: &[&'src str],
        enum_id: Id,
        variant_index: usize,
        matched: TypeId,
        payload: &[TypeId],
    ) {
        let start = span.into_range().start
            + path[..path.len().saturating_sub(1)]
                .iter()
                .map(|segment| segment.len() + "::".len())
                .sum::<usize>();
        let length = path.last().map_or(0, |segment| segment.len());
        let matched = matches!(
            matched.borrow_type(self),
            Type::Enum(matched_enum, _) if *matched_enum == enum_id
        )
        .then_some(matched);
        self.pattern_sites.push(PatternSite {
            name_span: (start..start + length).into(),
            kind: PatternSiteKind::Variant {
                enum_id,
                variant_index,
                payload: payload.to_vec(),
            },
            matched,
        });
    }

    /// E241: bank a `_` for its hover — the type it matches whole.
    pub(super) fn record_pattern_wildcard(&mut self, span: Span, matched: TypeId) {
        self.pattern_sites.push(PatternSite {
            name_span: span,
            kind: PatternSiteKind::Wildcard,
            matched: Some(matched),
        });
    }

    /// E241: the banked pattern positions rendered — a variant as the matched
    /// value's type (`Option<i32>`, the bare enum where nothing more is
    /// known), the variant and its payload as this match binds it; a `_` as
    /// `_: T`.
    pub(super) fn pattern_labels(&self) -> Vec<PatternLabel> {
        let empty = SubstitutionContext::default();
        let mut labels: Vec<PatternLabel> = Vec::new();
        for site in &self.pattern_sites {
            // A pattern may resolve more than once (a re-walk once its
            // scrutinee's type is known): the answer that knows the matched
            // type wins, and otherwise the latest.
            let earlier = labels
                .iter()
                .position(|label| label.name_span == site.name_span);
            if let Some(earlier) = earlier {
                if site.matched.is_none() && labels[earlier].matched {
                    continue;
                }
                labels.remove(earlier);
            }
            let matched_label = site
                .matched
                .map(|matched| self.pretty_print_type(matched.borrow_type(self), &empty));
            let (label, variant) = match &site.kind {
                PatternSiteKind::Wildcard => {
                    let Some(matched_label) = matched_label else {
                        continue;
                    };
                    // An unresolved scrutinee has nothing to say.
                    if matches!(
                        site.matched.map(|matched| matched.borrow_type(self)),
                        Some(Type::Unknown | Type::Unresolved)
                    ) {
                        continue;
                    }
                    (format!("_: {matched_label}"), None)
                }
                PatternSiteKind::Variant {
                    enum_id,
                    variant_index,
                    payload,
                } => {
                    let Some(enum_) = self.enums.get(enum_id) else {
                        continue;
                    };
                    let Some(variant) = enum_.variants.get(*variant_index) else {
                        continue;
                    };
                    let head = matched_label.unwrap_or_else(|| enum_.name.to_string());
                    let mut label = format!("{head}::{}", variant.name);
                    if !payload.is_empty() {
                        let payload: Vec<String> = payload
                            .iter()
                            .map(|type_id| {
                                self.pretty_print_type(type_id.borrow_type(self), &empty)
                            })
                            .collect();
                        label.push_str(&format!("({})", payload.join(", ")));
                    }
                    (label, Some((*enum_id, *variant_index)))
                }
            };
            labels.push(PatternLabel {
                name_span: site.name_span,
                label,
                variant,
                matched: site.matched.is_some(),
            });
        }
        labels
    }

    /// [`Analyzer::collect_generics`] through a TRAIT's or an object's
    /// arguments too — a head writes binders there (`impl type S: Read<type
    /// T>`, `with DescribeInto<type S: Sink>`), which a value type never does.
    fn collect_head_generics(&self, type_: &Type, depth: usize, out: &mut Vec<TypeId>) {
        if depth > 24 {
            return;
        }
        match type_ {
            Type::Trait(_, arguments) | Type::Dyn(_, arguments) => {
                for argument in arguments {
                    self.collect_head_generics(argument.borrow_type(self), depth + 1, out);
                }
            }
            Type::Struct(_, arguments) | Type::Enum(_, arguments) => {
                for argument in arguments {
                    self.collect_head_generics(argument.borrow_type(self), depth + 1, out);
                }
            }
            Type::Tuple(items, _) => {
                for item in items {
                    self.collect_head_generics(item.borrow_type(self), depth + 1, out);
                }
            }
            other => self.collect_generics(other, depth, out),
        }
    }

    /// E240: the entry's type-position names that are a type parameter or a
    /// trait, rendered — every other row keeps its plain label.
    pub(super) fn reference_hovers(&self) -> Vec<ReferenceHover> {
        let owners = self.generic_owners();
        let mut hovers: Vec<ReferenceHover> = Vec::new();
        for (source, span, definition, type_id) in &self.type_references {
            if *source != SourceId(0) || hovers.iter().any(|hover| hover.span == *span) {
                continue;
            }
            match type_id.borrow_type(self) {
                Type::Generic(constraint_id) => {
                    if self.implicit_generic_scopes.contains_key(constraint_id) {
                        continue;
                    }
                    let hover = ReferenceHover {
                        span: *span,
                        code: self.type_parameter_line(*constraint_id),
                        declared_by: owners.get(constraint_id).cloned(),
                    };
                    // The row's definition is the BINDER — the `U` of
                    // `fun pick<U, ..>` — which no row covers: its declaration
                    // answers the same as every use.
                    if let Some(binder_span) = definition
                        .filter(|binder| self.source_of_id(*binder) == Some(SourceId(0)))
                        .and_then(|binder| self.span_map.get(&binder))
                        && !hovers.iter().any(|known| known.span == **binder_span)
                    {
                        hovers.push(ReferenceHover {
                            span: **binder_span,
                            ..hover.clone()
                        });
                    }
                    hovers.push(hover);
                }
                Type::Trait(trait_id, _) if *definition == Some(*trait_id) => {
                    let Some(block) = self.type_definition_block(*type_id) else {
                        continue;
                    };
                    hovers.push(ReferenceHover {
                        span: *span,
                        code: block,
                        declared_by: None,
                    });
                }
                _ => {}
            }
        }
        hovers
    }

    /// `type T`, or `type I: Read<U>` with its bound — a type parameter as its
    /// declaration introduces it.
    fn type_parameter_line(&self, constraint_id: TypeId) -> String {
        let name = self
            .generic_constraint_names
            .get(&constraint_id)
            .copied()
            .unwrap_or("_");
        match constraint_id.borrow_type(self) {
            Type::Trait(..) => format!(
                "type {name}: {}",
                self.declaration_type_label(constraint_id)
            ),
            _ => format!("type {name}"),
        }
    }

    /// Every type parameter the ENTRY declares, mapped to the declaration
    /// that introduces it, as written: a function's signature, an impl's or a
    /// trait's head, a struct's or an enum's name line.
    fn generic_owners(&self) -> HashMap<TypeId, String> {
        let entry = Some(SourceId(0));
        let mut owners: HashMap<TypeId, String> = HashMap::default();
        let own = |owners: &mut HashMap<TypeId, String>, ids: &[TypeId], label: &str| {
            for id in ids {
                owners.entry(*id).or_insert_with(|| label.to_string());
            }
        };
        for (id, function) in &self.functions {
            if self.source_of_id(*id) == entry
                && !function.generic_parameter_constraint_ids.is_empty()
            {
                let label = self.function_signature_label(function);
                own(
                    &mut owners,
                    &function.generic_parameter_constraint_ids,
                    &label,
                );
            }
        }
        for implementation in &self.implementations {
            if implementation.source != SourceId(0) {
                continue;
            }
            let mut generics = Vec::new();
            self.collect_head_generics(implementation.subject.borrow_type(self), 0, &mut generics);
            for (_, arguments) in &implementation.trait_args {
                for argument in arguments {
                    self.collect_head_generics(argument.borrow_type(self), 0, &mut generics);
                }
            }
            // A binder's bound introduces binders of its own (`type S:
            // Read<type T>`): walk the bounds until nothing new appears.
            let mut index = 0;
            while index < generics.len() {
                let bound = generics[index];
                self.collect_head_generics(bound.borrow_type(self), 0, &mut generics);
                index += 1;
            }
            if !generics.is_empty() {
                let label = self.impl_header_label(implementation);
                own(&mut owners, &generics, &label);
            }
        }
        for (id, trait_) in &self.traits {
            if self.source_of_id(*id) == entry {
                let label = self.trait_header_label(trait_);
                own(
                    &mut owners,
                    &trait_.generic_parameter_constraint_ids,
                    &label,
                );
            }
        }
        for (id, struct_) in &self.structs {
            if self.source_of_id(*id) == entry {
                let label = format!(
                    "struct {}{}",
                    struct_.name,
                    self.generic_list_label(&struct_.generic_parameter_constraint_ids)
                );
                own(
                    &mut owners,
                    &struct_.generic_parameter_constraint_ids,
                    &label,
                );
            }
        }
        for (id, enum_) in &self.enums {
            if self.source_of_id(*id) == entry {
                let label = format!(
                    "enum {}{}",
                    enum_.name,
                    self.generic_list_label(&enum_.generic_parameter_constraint_ids)
                );
                own(&mut owners, &enum_.generic_parameter_constraint_ids, &label);
            }
        }
        owners
    }

    /// E237: the definition blocks for the entry's typed positions — every
    /// binding and parameter it declares, every member it reads, every field
    /// of a struct it declares — deduplicated by rendered type.
    pub(super) fn type_definitions(&self, expr_type_ids: &HashMap<Id, TypeId>) -> TypeDefinitions {
        let entry = Some(SourceId(0));
        let mut positions: Vec<TypeId> = Vec::new();
        for (id, variable) in &self.variables {
            if self.source_of_id(*id) == entry {
                positions.push(variable.type_id);
            }
        }
        for (id, parameter) in &self.parameters {
            if self.source_of_id(*id) == entry {
                positions.push(parameter.type_id);
            }
        }
        for id in self.member_name_spans.keys() {
            if self.source_of_id(*id) == entry
                && let Some(type_id) = expr_type_ids.get(id)
            {
                positions.push(*type_id);
            }
        }
        for (id, struct_) in &self.structs {
            if self.source_of_id(*id) == entry {
                positions.extend(struct_.fields.iter().map(|field| field.type_id));
            }
        }
        let empty = SubstitutionContext::default();
        let mut definitions = TypeDefinitions::default();
        let mut by_label: HashMap<String, Option<u32>> = HashMap::default();
        for type_id in positions {
            if definitions.by_type.contains_key(&type_id) {
                continue;
            }
            let label = self.pretty_print_type(type_id.borrow_type(self), &empty);
            let index = match by_label.get(&label) {
                Some(index) => *index,
                None => {
                    let index = self.type_definition_block(type_id).map(|block| {
                        definitions.blocks.push(block);
                        (definitions.blocks.len() - 1) as u32
                    });
                    by_label.insert(label, index);
                    index
                }
            };
            if let Some(index) = index {
                definitions.by_type.insert(type_id, index);
            }
        }
        definitions
    }

    /// One type's definition block (E237), or `None` where there is nothing
    /// a definition would add: a primitive, `bool`, an external (opaque) or
    /// fieldless struct, a closure, an unbounded generic.
    fn type_definition_block(&self, type_id: TypeId) -> Option<String> {
        match type_id.borrow_type(self) {
            Type::Struct(id, arguments) => {
                let struct_ = self.structs.get(id)?;
                if struct_.external
                    || struct_.fields.is_empty()
                    || self
                        .primitive_struct_ids
                        .values()
                        .any(|primitive| primitive == id)
                {
                    return None;
                }
                let substitution =
                    applied_substitution(&struct_.generic_parameter_constraint_ids, arguments);
                let members: Vec<String> = struct_
                    .fields
                    .iter()
                    .map(|field| {
                        let field_type = field.type_id.borrow_type(self);
                        format!(
                            "{}: {},",
                            field.name,
                            self.pretty_print_type(field_type, &substitution)
                        )
                    })
                    .collect();
                let head = self
                    .pretty_print_type(type_id.borrow_type(self), &SubstitutionContext::default());
                Some(definition_block(&format!("struct {head}"), &members))
            }
            Type::Enum(id, arguments) => {
                if self.bool_enum_id == Some(*id) {
                    return None;
                }
                let enum_ = self.enums.get(id)?;
                if enum_.variants.is_empty() {
                    return None;
                }
                let substitution =
                    applied_substitution(&enum_.generic_parameter_constraint_ids, arguments);
                let members: Vec<String> = enum_
                    .variants
                    .iter()
                    .map(|variant| {
                        if variant.data_type_ids.is_empty() {
                            return format!("{},", variant.name);
                        }
                        let payloads: Vec<String> = variant
                            .data_type_ids
                            .iter()
                            .map(|payload| {
                                self.pretty_print_type(payload.borrow_type(self), &substitution)
                            })
                            .collect();
                        format!("{}({}),", variant.name, payloads.join(", "))
                    })
                    .collect();
                let head = self
                    .pretty_print_type(type_id.borrow_type(self), &SubstitutionContext::default());
                Some(definition_block(&format!("enum {head}"), &members))
            }
            // The trait's own abstract type — `self` inside a trait default.
            Type::Trait(trait_id, arguments) => {
                self.trait_definition_block(*trait_id, arguments, None)
            }
            Type::Dyn(trait_id, arguments) => {
                self.trait_definition_block(*trait_id, arguments, Some(type_id))
            }
            // A trait-typed value (a bounded generic, B186's implicit one, a
            // blanket's subject): the bound is what the value promises.
            Type::Generic(constraint_id) => match constraint_id.borrow_type(self) {
                Type::Trait(trait_id, arguments) => {
                    self.trait_definition_block(*trait_id, arguments, Some(type_id))
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// A trait's REQUIRED members at the given arguments (E237 Q4): what a
    /// value of a trait-typed or `dyn` type is guaranteed to answer. `Self`
    /// renders as the value's own type where there is one (`dyn Source<i32>`,
    /// a generic's name) and as `Self` inside the trait itself.
    fn trait_definition_block(
        &self,
        trait_id: Id,
        arguments: &[TypeId],
        self_type: Option<TypeId>,
    ) -> Option<String> {
        let trait_ = self.traits.get(&trait_id)?;
        let substitution =
            applied_substitution(&trait_.generic_parameter_constraint_ids, arguments);
        let subject = SignatureSubject {
            declaring_trait_id: trait_id,
            rendered_for: match self_type {
                Some(self_type) => SignatureSide::Impl(self_type, arguments),
                None => SignatureSide::Applied(arguments),
            },
        };
        let members: Vec<String> = trait_
            .declared_members
            .iter()
            .filter_map(|(_, member_id)| {
                let function = self
                    .functions
                    .get(&self.resolve_member_function_id(*member_id))?;
                (!function.has_body).then(|| {
                    format!(
                        "{};",
                        self.function_signature_label_for(function, Some(&subject))
                    )
                })
            })
            .collect();
        let mut head = format!("trait {}", trait_.name);
        if !arguments.is_empty() {
            let rendered: Vec<String> = arguments
                .iter()
                .map(|argument| self.declaration_type_label(*argument))
                .collect();
            head.push_str(&format!("<{}>", rendered.join(", ")));
        } else if !trait_.generic_parameter_names.is_empty() {
            head.push_str(&format!("<{}>", trait_.generic_parameter_names.join(", ")));
        }
        if !trait_.supertraits.is_empty() {
            let supertraits: Vec<String> = trait_
                .supertraits
                .iter()
                .map(|supertrait| self.supertrait_label(trait_, *supertrait, &substitution))
                .collect();
            head.push_str(" with ");
            head.push_str(&supertraits.join(" + "));
        }
        Some(definition_block(&head, &members))
    }
}

/// A declaration's own parameters mapped to the arguments an instantiation
/// applied — identity pairs dropped, so an unapplied type renders as declared.
fn applied_substitution(parameters: &[TypeId], arguments: &[TypeId]) -> SubstitutionContext {
    parameters
        .iter()
        .copied()
        .zip(arguments.iter().copied())
        .filter(|(parameter, argument)| parameter != argument)
        .collect()
}

/// `head {` + one tab-indented member per line, capped at
/// [`DEFINITION_MEMBER_CAP`] and then `…`, + `}` — `head {}` with no members.
fn definition_block(head: &str, members: &[String]) -> String {
    if members.is_empty() {
        return format!("{head} {{}}");
    }
    let mut out = format!("{head} {{");
    for member in members.iter().take(DEFINITION_MEMBER_CAP) {
        out.push_str("\n\t");
        out.push_str(member);
    }
    if members.len() > DEFINITION_MEMBER_CAP {
        out.push_str("\n\t…");
    }
    out.push_str("\n}");
    out
}
