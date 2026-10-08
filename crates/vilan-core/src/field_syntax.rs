//! Field syntax on store handles (A149 S4, `proposal/store.md` §3.2, ruled R-e).
//!
//! `[derive(Storable)]` puts one PROJECTION per field on `Store<T>` and
//! `StoreSome<T>`: `app.user()` is the handle on `app`'s `user` field. This tier
//! lets a reader write the field the way the value spells it, `app.user.name`
//! for `app.user().name()`:
//!
//! > on a `Store<T>` (or `StoreSome<T>`) value, a member that names a field of
//! > `T` resolves to the projection of that name.
//!
//! It sits AFTER the handle's own fields, so it can shadow nothing a handle
//! has: the handles' fields are `[internal]`, and an internal std field is not
//! a member outside std (the ruled door, the cause), so a field of `T` named
//! `path` or `root` reaches its projection rather than the handle's slot tree.
//! A method is never in the way: a bare `.name` is no method reference in
//! vilan, and the derive refuses a field named like a handle member (Q9).
//!
//! A PROJECTION is a self-only method declared in an inherent impl whose
//! subject is the handle AT `T`'s own type (`impl Store<User>`) — exactly what
//! the derive writes, and never what std's `impl Store<type T>` is. That is
//! what keeps a renamed projection honest: a field `get` renamed `verb` with
//! `[reactive(name = "verb")]` has no projection called `get`, and `request.get`
//! is refused with that said, rather than reaching the handle's own `get()`.
//!
//! ONE reader for both questions — the analyzer asking what `.name` means
//! while it solves, and the editor asking what to offer after `app.` — so the
//! two cannot disagree about which fields read through.

use crate::analyzer::{Expr, Function, Implementation, Parameter, Struct};
use crate::fx::{FxHashMap as HashMap, FxIndexMap as IndexMap};
use crate::id::Id;
use crate::type_::{Type, TypeId};

/// The records a reading consults: the analyzer's while it solves, the
/// `Program`'s afterwards. Both own the same maps under the same names.
pub struct Surface<'a, 'src> {
    pub structs: &'a IndexMap<Id, Struct<'src>>,
    pub implementations: &'a [Implementation<'src>],
    pub types: &'a HashMap<TypeId, Type>,
    pub entities: &'a HashMap<Id, Expr<'src>>,
    pub functions: &'a IndexMap<Id, Function<'src>>,
    pub parameters: &'a IndexMap<Id, Parameter<'src>>,
    /// The handle structs the tier applies to — std's `Store` and `StoreSome`,
    /// when `std::reactive::store_core` is loaded.
    pub handles: &'a [Id],
}

/// What `receiver.member` means under the tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    /// The receiver is no store handle over a struct, or `member` names no
    /// field of that struct: the tier has nothing to say.
    Inapplicable,
    /// `member` names a field of `T`, and this is its projection's
    /// declaration: the read is the call `receiver.member()`.
    Projection(Id),
    /// `member` names a field of `T` and the handle has no projection of that
    /// name — `T` does not derive `Storable`, or the field's projection is
    /// renamed. Carries `T`'s struct id, for the message.
    Unprojected(Id),
}

impl<'src> Surface<'_, 'src> {
    /// The struct a handle type is a handle ON — `User` for `Store<User>` —
    /// with the handle's own struct id, or `None` when `receiver` is not one.
    fn subject_of(&self, receiver: &Type) -> Option<(Id, Id)> {
        let Type::Struct(handle, arguments) = receiver else {
            return None;
        };
        if !self.handles.contains(handle) {
            return None;
        }
        match self.types.get(arguments.first()?)? {
            Type::Struct(subject, _) => Some((*handle, *subject)),
            _ => None,
        }
    }

    /// Whether `member_id` is a method taking `self` and nothing else.
    fn is_self_only(&self, member_id: Id) -> bool {
        let Some(Expr::Function(function_id)) = self.entities.get(&member_id) else {
            return false;
        };
        let Some(function) = self.functions.get(function_id) else {
            return false;
        };
        match function.parameters.as_slice() {
            [only] => self
                .parameters
                .get(only)
                .is_some_and(|parameter| parameter.name == "self"),
            _ => false,
        }
    }

    /// The projection named `name` on the handle `handle` at the subject
    /// struct `subject`: a self-only member of an inherent impl on exactly
    /// `handle<subject<..>>`.
    fn projection(&self, handle: Id, subject: Id, name: &str) -> Option<Id> {
        self.implementations.iter().find_map(|implementation| {
            if !implementation.trait_ids.is_empty() {
                return None;
            }
            let Some(Type::Struct(subject_handle, arguments)) =
                self.types.get(&implementation.subject)
            else {
                return None;
            };
            if *subject_handle != handle {
                return None;
            }
            let at_subject = arguments
                .first()
                .and_then(|argument| self.types.get(argument));
            if !matches!(at_subject, Some(Type::Struct(id, _)) if *id == subject) {
                return None;
            }
            let member_id = *implementation.declarations.get(name)?;
            self.is_self_only(member_id).then_some(member_id)
        })
    }

    /// What `receiver.member` reads, under the tier.
    pub fn read(&self, receiver: &Type, member: &str) -> Reading {
        let Some((handle, subject)) = self.subject_of(receiver) else {
            return Reading::Inapplicable;
        };
        let names_a_field = self
            .structs
            .get(&subject)
            .is_some_and(|structure| structure.fields.iter().any(|field| field.name == member));
        if !names_a_field {
            return Reading::Inapplicable;
        }
        match self.projection(handle, subject, member) {
            Some(member_id) => Reading::Projection(member_id),
            None => Reading::Unprojected(subject),
        }
    }

    /// Every field the tier reads through on `receiver`, in declaration
    /// order, each with its projection — the editor's member list after
    /// `app.`. Empty when `receiver` is no handle.
    pub fn projected_fields(&self, receiver: &Type) -> Vec<(&'src str, Id)> {
        let Some((handle, subject)) = self.subject_of(receiver) else {
            return Vec::new();
        };
        let Some(structure) = self.structs.get(&subject) else {
            return Vec::new();
        };
        structure
            .fields
            .iter()
            .filter_map(|field| Some((field.name, self.projection(handle, subject, field.name)?)))
            .collect()
    }
}

impl<'src> crate::analyzer::Program<'src> {
    /// The field-syntax tier's reader over a finished analysis — what the
    /// editor asks after a `.` on a store handle.
    pub fn field_syntax(&self) -> Surface<'_, 'src> {
        Surface {
            structs: &self.structs,
            implementations: &self.implementations,
            types: &self.type_id_to_type_map,
            entities: &self.entity_map,
            functions: &self.functions,
            parameters: &self.parameters,
            handles: &self.field_syntax_handles,
        }
    }
}
