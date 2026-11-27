//! Field syntax on a handle (B711, `proposal/struct-keys.md` §3.5, R6).
//!
//! The handle door is one attribute on one method:
//!
//! > on a receiver whose type has a `[field_syntax]` method taking a
//! > `Key<T, _>`, a member that is no member of the receiver and names a field
//! > of the struct `T` reads as `receiver.method(T.member)`.
//!
//! std's store handles read their struct's fields through it (S2b: `Store`,
//! `StoreSome`, `RemoteStore` and `RemoteStoreSome` each declare one
//! `[field_syntax] fun field`), exactly as a library's handle does: the
//! compiler knows no store type by name.
//!
//! It is consulted only where a member read has already failed to find a
//! field (the analyzer's), and only for a struct that an inherent impl
//! gives such a method ([`Surface::keyed_methods`], empty for a program that
//! declares none), so a program without the attribute pays nothing.
//! [`Surface::key_parameter`] is the one statement of the method's shape: the
//! declaration check reports what it refuses, and the reading ignores a
//! method it refuses, so a malformed declaration is one diagnostic, never a
//! cascade at every read.
//!
//! ONE reader for both questions — the analyzer asking what `.name` means
//! while it solves, and the editor asking what to offer after `app.` — so the
//! two cannot disagree about which fields read through.

use crate::analyzer::{Expr, Function, Implementation, Parameter, Struct};
use crate::fx::{FxHashMap as HashMap, FxIndexMap as IndexMap};
use crate::id::Id;
use crate::node::Convention;
use crate::type_::{Type, TypeId, TypeTable};

/// The records a reading consults: the analyzer's while it solves, the
/// `Program`'s afterwards. Both own the same maps under the same names.
pub struct Surface<'a, 'src> {
    pub structs: &'a IndexMap<Id, Struct<'src>>,
    pub implementations: &'a [Implementation<'src>],
    pub types: &'a TypeTable,
    pub entities: &'a HashMap<Id, Expr<'src>>,
    pub functions: &'a IndexMap<Id, Function<'src>>,
    pub parameters: &'a IndexMap<Id, Parameter<'src>>,
    /// B711 S2a: the `[field_syntax]` methods of inherent impls, each as
    /// (implementation index, member id).
    pub keyed_methods: &'a [(usize, Id)],
    /// std's `Key` struct, when `std::key` is loaded.
    pub key_struct: Option<Id>,
}

/// B711 S2a: a keyed field-syntax read — `receiver.member` is
/// `receiver.method(T.member)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Keyed {
    /// The `[field_syntax]` method's member id in its impl's declarations.
    pub method: Id,
    /// The method's function id.
    pub function: Id,
    /// `T`, the struct the key is over.
    pub subject: Id,
    /// `T` as a type at the receiver's arguments, where the reader has one
    /// without minting: the receiver's own argument (the key's root is the
    /// impl's parameter) or the declared root (it names no parameter).
    pub root: Option<TypeId>,
    /// The field of `T` the member names.
    pub field: usize,
}

/// B711 S2a: what a `[field_syntax]` method's signature is, when it has the
/// one shape the door admits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyParameter {
    /// The key's root `T`, in the impl's own terms (a type parameter of the
    /// impl, or a struct).
    pub root: TypeId,
    /// The method's own type parameter `U`, the field type.
    pub field_type: TypeId,
}

/// B711 S2a: why a `[field_syntax]` method has not the door's shape. Each is
/// one clause of [`ShapeProblem::message`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShapeProblem {
    /// The first parameter is no `self`: a static has no receiver.
    NoReceiver,
    /// `&mut self` or `own self`.
    Receiver(Convention),
    /// Not exactly one parameter after `self`.
    Arity(usize),
    /// The parameter is not `Key<T, U>`.
    NotAKey,
    /// `U` is not the method's own type parameter.
    FieldTypeNotOwn,
    /// The method declares other type parameters besides `U`.
    OtherGenerics,
    /// The parameter is `lazy`, a spread, `mut` or taken by a view.
    KeyConvention,
    /// The key's root is neither a struct nor a type parameter of the impl.
    Root,
}

/// The shape every [`ShapeProblem`] message leads with.
pub const FIELD_SYNTAX_SHAPE: &str = "a `[field_syntax]` method has the shape `fun at<U>(self, \
     key: Key<T, U>): R` - a receiver taken as `self` or `&self`, then exactly one key parameter \
     over a struct `T`, whose field type `U` is the method's one type parameter";

impl ShapeProblem {
    /// The refusal at the declaration: the shape, then what this one does.
    pub fn message(self) -> String {
        let what = match self {
            ShapeProblem::NoReceiver => {
                "this one takes no `self`: field syntax reads through a receiver".to_string()
            }
            ShapeProblem::Receiver(Convention::RefMut) => "this one takes `&mut self`, and a \
                 chain's next step reads through a call's result, which is never a place"
                .to_string(),
            ShapeProblem::Receiver(_) => "this one takes `own self`, and a read that looks like \
                 a field would move the handle"
                .to_string(),
            ShapeProblem::Arity(0) => {
                "this one takes no parameter after `self`, and field syntax passes one, the key"
                    .to_string()
            }
            ShapeProblem::Arity(count) => format!(
                "this one takes {count} parameters after `self`, and field syntax passes one, \
                 the key"
            ),
            ShapeProblem::NotAKey => "this one's parameter is no `Key<T, U>`".to_string(),
            ShapeProblem::FieldTypeNotOwn => "this one's key does not end at the method's own \
                 type parameter (`Key<T, U>` with `fun at<U>`): the field type is what each read \
                 binds"
                .to_string(),
            ShapeProblem::OtherGenerics => "this one declares type parameters besides the field \
                 type, which no field-syntax read could bind"
                .to_string(),
            ShapeProblem::KeyConvention => "this one's key parameter is not a plain value (no \
                 `lazy`, `own`, `mut`, view or spread)"
                .to_string(),
            ShapeProblem::Root => "this one's key is over neither a struct nor a type parameter \
                 of the impl"
                .to_string(),
        };
        format!("{FIELD_SYNTAX_SHAPE}: {what}")
    }
}

/// What `receiver.member` means under the tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    /// The receiver's type has no `[field_syntax]` method over a struct, or
    /// `member` names no field of that struct: the tier has nothing to say.
    Inapplicable,
    /// The receiver's type has a `[field_syntax]` method over a struct `T` and
    /// `member` names a field of `T`: the read is the call
    /// `receiver.method(T.member)`.
    Keyed(Keyed),
}

impl Surface<'_, '_> {
    /// What `receiver.member` reads, under the tier.
    pub fn read(&self, receiver: &Type, member: &str) -> Reading {
        self.keyed_method(receiver)
            .and_then(|(method, subject)| {
                let field = self
                    .structs
                    .get(&subject.0)?
                    .fields
                    .iter()
                    .position(|field| field.name == member)?;
                Some(Reading::Keyed(Keyed {
                    method: method.0,
                    function: method.1,
                    subject: subject.0,
                    root: subject.1,
                    field,
                }))
            })
            .unwrap_or(Reading::Inapplicable)
    }
}

impl<'src> Surface<'_, 'src> {
    /// B711 S2a: the function a member id declares — the member itself where
    /// the impl lists the function's own id.
    pub fn function_of(&self, member_id: Id) -> Option<Id> {
        match self.entities.get(&member_id) {
            Some(Expr::Function(function_id)) => Some(*function_id),
            _ => self.functions.contains_key(&member_id).then_some(member_id),
        }
    }

    /// B711 S2a: the one shape a `[field_syntax]` method may have —
    /// `fun at<U>(self, key: Key<T, U>): R` — or why `function_id` has not
    /// got it. The declaration check reports the problem; the reading skips a
    /// method that has one.
    pub fn key_parameter(&self, function_id: Id) -> Result<KeyParameter, ShapeProblem> {
        let function = self
            .functions
            .get(&function_id)
            .ok_or(ShapeProblem::NoReceiver)?;
        let parameters: Vec<&Parameter> = function
            .parameters
            .iter()
            .filter_map(|parameter| self.parameters.get(parameter))
            .collect();
        let Some(receiver) = parameters.first().filter(|first| first.name == "self") else {
            return Err(ShapeProblem::NoReceiver);
        };
        if matches!(receiver.convention, Convention::RefMut | Convention::Own) {
            return Err(ShapeProblem::Receiver(receiver.convention));
        }
        let [key] = &parameters[1..] else {
            return Err(ShapeProblem::Arity(parameters.len() - 1));
        };
        let Some(Type::Struct(key_struct, arguments)) = self.types.get(&key.type_id) else {
            return Err(ShapeProblem::NotAKey);
        };
        if Some(*key_struct) != self.key_struct || arguments.len() != 2 {
            return Err(ShapeProblem::NotAKey);
        }
        let (root, field_type) = (arguments[0], arguments[1]);
        let own = &function.generic_parameter_constraint_ids;
        let field_is_own = match self.types.get(&field_type) {
            Some(Type::Generic(constraint)) => own.contains(constraint),
            _ => false,
        };
        if !field_is_own {
            return Err(ShapeProblem::FieldTypeNotOwn);
        }
        if own.len() != 1 {
            return Err(ShapeProblem::OtherGenerics);
        }
        if key.lazy || key.spread || key.mutable || key.convention != Convention::Bare {
            return Err(ShapeProblem::KeyConvention);
        }
        match self.types.get(&root) {
            Some(Type::Generic(constraint)) if !own.contains(constraint) => {}
            Some(Type::Struct(..)) if !self.mentions(root, own) => {}
            _ => return Err(ShapeProblem::Root),
        }
        Ok(KeyParameter { root, field_type })
    }

    /// Whether `type_id` mentions one of the `generics` (bounded walk).
    fn mentions(&self, type_id: TypeId, generics: &[TypeId]) -> bool {
        fn walk(surface: &Surface, type_id: TypeId, generics: &[TypeId], depth: usize) -> bool {
            if depth > 32 {
                return true;
            }
            match surface.types.get(&type_id) {
                Some(Type::Generic(constraint)) => generics.contains(constraint),
                Some(Type::Struct(_, arguments) | Type::Enum(_, arguments)) => arguments
                    .iter()
                    .any(|argument| walk(surface, *argument, generics, depth + 1)),
                Some(Type::Tuple(elements, _)) => elements
                    .iter()
                    .any(|element| walk(surface, *element, generics, depth + 1)),
                _ => false,
            }
        }
        walk(self, type_id, generics, 0)
    }

    /// Binds `pattern` (an impl subject's argument, in the impl's terms)
    /// against `concrete` (the receiver's), answering whether the impl can
    /// apply. A type parameter binds whatever stands there; anything else
    /// must have the receiver's shape.
    fn bind(
        &self,
        pattern: TypeId,
        concrete: TypeId,
        bindings: &mut HashMap<TypeId, TypeId>,
        depth: usize,
    ) -> bool {
        if pattern == concrete {
            return true;
        }
        if depth > 32 {
            return false;
        }
        let (Some(pattern_type), Some(concrete_type)) =
            (self.types.get(&pattern), self.types.get(&concrete))
        else {
            return false;
        };
        match (pattern_type, concrete_type) {
            (Type::Generic(constraint), _) => {
                bindings.entry(*constraint).or_insert(concrete);
                true
            }
            (Type::Struct(left, left_arguments), Type::Struct(right, right_arguments))
            | (Type::Enum(left, left_arguments), Type::Enum(right, right_arguments)) => {
                left == right && self.bind_all(left_arguments, right_arguments, bindings, depth)
            }
            (Type::Tuple(left, _), Type::Tuple(right, _)) => {
                self.bind_all(left, right, bindings, depth)
            }
            (left, right) => left == right,
        }
    }

    fn bind_all(
        &self,
        patterns: &[TypeId],
        concretes: &[TypeId],
        bindings: &mut HashMap<TypeId, TypeId>,
        depth: usize,
    ) -> bool {
        patterns.len() == concretes.len()
            && patterns
                .iter()
                .zip(concretes)
                .all(|(pattern, concrete)| self.bind(*pattern, *concrete, bindings, depth + 1))
    }

    /// B711 S2a: the `[field_syntax]` method a receiver of type `receiver`
    /// reads through, with the struct `T` its key is over: `((member id,
    /// function id), (T's struct id, T as a type where one exists))`. `None`
    /// when no inherent impl that applies to the receiver declares one of the
    /// door's shape, or its `T` is no struct at this receiver (a type
    /// parameter, a tuple, an enum): then the reading has nothing to say.
    #[allow(clippy::type_complexity)]
    pub fn keyed_method(&self, receiver: &Type) -> Option<((Id, Id), (Id, Option<TypeId>))> {
        if self.keyed_methods.is_empty() {
            return None;
        }
        let Type::Struct(receiver_struct, receiver_arguments) = receiver else {
            return None;
        };
        self.keyed_methods
            .iter()
            .find_map(|&(implementation_index, member_id)| {
                let implementation = self.implementations.get(implementation_index)?;
                let Some(Type::Struct(subject, pattern_arguments)) =
                    self.types.get(&implementation.subject)
                else {
                    return None;
                };
                if subject != receiver_struct {
                    return None;
                }
                let mut bindings = HashMap::default();
                if !self.bind_all(pattern_arguments, receiver_arguments, &mut bindings, 0) {
                    return None;
                }
                let function_id = self.function_of(member_id)?;
                let shape = self.key_parameter(function_id).ok()?;
                let (root_struct, root) = match self.types.get(&shape.root)? {
                    Type::Generic(constraint) => {
                        let bound = *bindings.get(constraint)?;
                        match self.types.get(&bound)? {
                            Type::Struct(root_struct, _) => (*root_struct, Some(bound)),
                            _ => return None,
                        }
                    }
                    Type::Struct(root_struct, arguments) => {
                        (*root_struct, arguments.is_empty().then_some(shape.root))
                    }
                    _ => return None,
                };
                self.structs.get(&root_struct)?;
                Some(((member_id, function_id), (root_struct, root)))
            })
    }

    /// B711 S2a: every field of `T` the keyed reading reaches on `receiver`,
    /// in declaration order, each as its reading — the editor's member list
    /// after `user.`. Empty when the receiver has no `[field_syntax]` method.
    /// The editor leaves out a field the receiver's own members shadow, and
    /// one a key literal refuses (an `[internal]` std field), by its rules.
    pub fn keyed_fields(&self, receiver: &Type) -> Vec<(&'src str, Keyed)> {
        let Some((method, (subject, root))) = self.keyed_method(receiver) else {
            return Vec::new();
        };
        let Some(structure) = self.structs.get(&subject) else {
            return Vec::new();
        };
        structure
            .fields
            .iter()
            .enumerate()
            .map(|(field, declared)| {
                (
                    declared.name,
                    Keyed {
                        method: method.0,
                        function: method.1,
                        subject,
                        root,
                        field,
                    },
                )
            })
            .collect()
    }
}

impl<'src> crate::analyzer::Program<'src> {
    /// The field-syntax tier's reader over a finished analysis — what the
    /// editor asks after a `.` on a handle.
    pub fn field_syntax(&self) -> Surface<'_, 'src> {
        Surface {
            structs: &self.structs,
            implementations: &self.implementations,
            types: &self.type_id_to_type_map,
            entities: &self.entity_map,
            functions: &self.functions,
            parameters: &self.parameters,
            keyed_methods: &self.field_syntax_methods,
            key_struct: self.key_struct_id,
        }
    }
}

/// B711 S2a: the editor's half of the keyed reading — a read's RESULT type at
/// a receiver, from the method's declaration alone (the editor types reads the
/// analysis has not seen, `user.address.` mid-edit).
impl crate::analyzer::Program<'_> {
    /// The keyed read's declared result type and the bindings that ground it
    /// at `receiver`: the impl's parameters at the receiver's arguments, the
    /// method's `U` at the field's type, and `T`'s own parameters at the
    /// key's root.
    fn keyed_result(
        &self,
        receiver: TypeId,
        keyed: Keyed,
    ) -> Option<(TypeId, HashMap<TypeId, TypeId>)> {
        let function = self.functions.get(&keyed.function)?;
        let declared = function
            .return_type_id
            .or_else(|| self.inferred_return_types.get(&keyed.function).copied())?;
        let (implementation_index, _) = self
            .field_syntax_methods
            .iter()
            .find(|(_, member)| *member == keyed.method)?;
        let implementation = self.implementations.get(*implementation_index)?;
        let mut bindings = HashMap::default();
        crate::impl_select::bind_subject(self, implementation.subject, receiver, &mut bindings);
        let shape = self.field_syntax().key_parameter(keyed.function).ok()?;
        let Some(Type::Generic(field_parameter)) = self.type_id_to_type_map.get(&shape.field_type)
        else {
            return None;
        };
        let subject = self.structs.get(&keyed.subject)?;
        bindings.insert(*field_parameter, subject.fields.get(keyed.field)?.type_id);
        if let Some(Type::Struct(_, arguments)) = keyed
            .root
            .and_then(|root| self.type_id_to_type_map.get(&root))
        {
            for (parameter, argument) in subject
                .generic_parameter_constraint_ids
                .iter()
                .zip(arguments)
            {
                bindings.insert(*parameter, *argument);
            }
        }
        Some((declared, bindings))
    }

    /// The keyed read's result type at `receiver`, written as a reader would
    /// (`Optic<Profile, str>`) — completion's detail. `None` where a part of
    /// it is no nominal, tuple or `void` the reader can spell.
    pub fn keyed_result_label(&self, receiver: TypeId, keyed: Keyed) -> Option<String> {
        let (declared, bindings) = self.keyed_result(receiver, keyed)?;
        self.render_bound(declared, &bindings, 0)
    }

    /// The keyed read's result type at `receiver` as a type the analysis
    /// minted — the live receiver walk's next step (`user.address.`). `None`
    /// when no type of that structure exists in the table (nothing typed one).
    pub fn keyed_result_type_id(&self, receiver: TypeId, keyed: Keyed) -> Option<TypeId> {
        let (declared, bindings) = self.keyed_result(receiver, keyed)?;
        if let Some(Type::Generic(parameter)) = self.type_id_to_type_map.get(&declared) {
            return bindings.get(parameter).copied();
        }
        let Some(Type::Struct(head, _) | Type::Enum(head, _)) =
            self.type_id_to_type_map.get(&declared)
        else {
            return Some(declared);
        };
        self.type_id_to_type_map
            .iter()
            .filter(|(_, candidate)| {
                matches!(candidate, Type::Struct(id, _) | Type::Enum(id, _) if id == head)
            })
            .find(|(candidate, _)| self.bound_matches(declared, *candidate, &bindings, 0))
            .map(|(candidate, _)| candidate)
    }

    /// Whether `candidate` is `pattern` with `bindings` applied (structurally:
    /// types are not interned, so two ids of one type compare by shape).
    fn bound_matches(
        &self,
        pattern: TypeId,
        candidate: TypeId,
        bindings: &HashMap<TypeId, TypeId>,
        depth: usize,
    ) -> bool {
        if pattern == candidate {
            return true;
        }
        if depth > 32 {
            return false;
        }
        let (Some(pattern_type), Some(candidate_type)) = (
            self.type_id_to_type_map.get(&pattern),
            self.type_id_to_type_map.get(&candidate),
        ) else {
            return false;
        };
        let all = |left: &[TypeId], right: &[TypeId]| {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| self.bound_matches(*left, *right, bindings, depth + 1))
        };
        match (pattern_type, candidate_type) {
            (Type::Generic(parameter), _) if bindings.contains_key(parameter) => {
                let bound = bindings[parameter];
                bound != pattern && self.bound_matches(bound, candidate, bindings, depth + 1)
            }
            (Type::Struct(left, left_arguments), Type::Struct(right, right_arguments))
            | (Type::Enum(left, left_arguments), Type::Enum(right, right_arguments)) => {
                left == right && all(left_arguments, right_arguments)
            }
            (Type::Tuple(left, _), Type::Tuple(right, _)) => all(left, right),
            (left, right) => left == right,
        }
    }

    /// `type_id` with `bindings` applied, spelled as source writes it.
    fn render_bound(
        &self,
        type_id: TypeId,
        bindings: &HashMap<TypeId, TypeId>,
        depth: usize,
    ) -> Option<String> {
        if depth > 32 {
            return None;
        }
        let arguments = |arguments: &[TypeId]| -> Option<String> {
            let rendered = arguments
                .iter()
                .map(|argument| self.render_bound(*argument, bindings, depth + 1))
                .collect::<Option<Vec<String>>>()?;
            Some(match rendered.is_empty() {
                true => String::new(),
                false => format!("<{}>", rendered.join(", ")),
            })
        };
        match self.type_id_to_type_map.get(&type_id)? {
            Type::Generic(parameter) => {
                let bound = *bindings.get(parameter)?;
                (bound != type_id)
                    .then(|| self.render_bound(bound, bindings, depth + 1))
                    .flatten()
            }
            Type::Struct(id, written) => Some(format!(
                "{}{}",
                self.structs.get(id)?.name,
                arguments(written)?
            )),
            Type::Enum(id, written) => Some(format!(
                "{}{}",
                self.enums.get(id)?.name,
                arguments(written)?
            )),
            Type::Tuple(elements, _) if elements.is_empty() => Some("void".to_string()),
            Type::Tuple(elements, _) => {
                let rendered = elements
                    .iter()
                    .map(|element| self.render_bound(*element, bindings, depth + 1))
                    .collect::<Option<Vec<String>>>()?;
                Some(format!("({})", rendered.join(", ")))
            }
            Type::Void => Some("void".to_string()),
            _ => None,
        }
    }
}

/// B711 S2b: the clause the refusal of a handle's field called as a method
/// carries (`app.user()`, the paren form a store's derived projections had):
/// what [`field_syntax_call_fix`] reads a diagnostic by.
pub const FIELD_SYNTAX_CALL_MARK: &str =
    "which this handle reads with field syntax, not as a method";

/// B711 S2b: the edit for the paren form `app.user()` that a refusal carrying
/// [`FIELD_SYNTAX_CALL_MARK`] reports at the member `user` (`span`): the call
/// `user()` becomes the read `user`. `None` when the message is another's, or
/// the member is not followed by an empty argument list (a call with
/// arguments is no field read to rewrite). What `vilan check --fix` applies.
pub fn field_syntax_call_fix(
    text: &str,
    span: crate::span::Span,
    message: &str,
) -> Option<(crate::span::Span, String)> {
    if !message.contains(FIELD_SYNTAX_CALL_MARK) {
        return None;
    }
    let name = text.get(span.into_range())?;
    if name.is_empty() || !name.chars().all(|c| c == '_' || c.is_alphanumeric()) {
        return None;
    }
    let after = text.get(span.end..)?;
    after.starts_with("()").then(|| {
        (
            crate::span::Span {
                start: span.start,
                end: span.end + 2,
            },
            name.to_string(),
        )
    })
}

/// B711 S2b: the first paragraph of the `///` doc comment above the
/// declaration whose name starts at `name_start` in `text`, joined into one
/// line — the steer a field-syntax refusal quotes from the trait a door's
/// impl bounds by. Attribute lines between the comment and the declaration
/// are skipped. `None` when the declaration has no doc comment.
pub fn doc_summary(text: &str, name_start: usize) -> Option<String> {
    let before = text.get(..name_start)?;
    let line_start = before.rfind('\n').map_or(0, |at| at + 1);
    let mut lines: Vec<&str> = Vec::new();
    for line in before[..line_start].lines().rev() {
        let line = line.trim();
        if line.starts_with('[') && lines.is_empty() {
            continue;
        }
        match line.strip_prefix("///") {
            Some(doc) => lines.push(doc.trim()),
            None => break,
        }
    }
    lines.reverse();
    let paragraph: Vec<&str> = lines
        .into_iter()
        .skip_while(|line| line.is_empty())
        .take_while(|line| !line.is_empty())
        .collect();
    (!paragraph.is_empty()).then(|| paragraph.join(" "))
}

#[cfg(test)]
mod tests {
    use super::{FIELD_SYNTAX_CALL_MARK, doc_summary, field_syntax_call_fix};
    use crate::span::Span;

    #[test]
    fn the_paren_form_fix_drops_an_empty_argument_list_only() {
        let message = format!(
            "Store<App> has no method 'user'; `user` is a field of `App`, {FIELD_SYNTAX_CALL_MARK}"
        );
        let text = "app.user().name()";
        let at = Span { start: 4, end: 8 };
        assert_eq!(
            field_syntax_call_fix(text, at, &message),
            Some((Span { start: 4, end: 10 }, "user".to_string()))
        );
        assert_eq!(field_syntax_call_fix("app.user(1)", at, &message), None);
        assert_eq!(field_syntax_call_fix(text, at, "another message"), None);
    }

    #[test]
    fn a_doc_summary_is_the_first_paragraph_above_the_attributes() {
        let text = "/// One line\n/// two.\n///\n/// Later.\n[hint(x)]\nexport trait Marker {}\n";
        let start = text.find("Marker").unwrap();
        assert_eq!(doc_summary(text, start).as_deref(), Some("One line two."));
        let bare = "fun f() {}\ntrait Bare {}\n";
        assert_eq!(doc_summary(bare, bare.find("Bare").unwrap()), None);
    }
}
