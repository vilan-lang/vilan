//! F138 (R-a, door (1)): a call's argument rooted at a captured `mut` binding
//! is read AFTER the call's later arguments, on the JS backend as natively.
//!
//! Spec §6.9's native note orders a view of a captured binding — a `&self`
//! receiver, a `&` argument, and (F129) a by-value copy of a value JS hands
//! on by reference — after the call's other arguments, so a later argument
//! that writes the binding is seen by the callee. JavaScript had answered the
//! in-place write the same way for free (its argument is the binding's object
//! itself) but not a REBINDING: `words.measured(swap())`, `swap` assigning
//! `words` whole, handed the callee the object `words` held when the receiver
//! was evaluated (1) where the native view saw the new binding (3). The JS
//! argument now waits too: the call's other arguments that run code are bound
//! to `const`s in their own order first (B452's spill), and the binding is
//! read where the call is made.
//!
//! The rule is the native emitter's, read the same way (`vilan-rust`'s
//! `takes_a_cell_view` and `defers_its_cell_copy`): an argument waits exactly
//! where the native one does, so the two backends answer alike — the place is
//! a spine (fields, tuple slots, subscripts that run no code) over a captured
//! `mut` binding ([`crate::captures::captured_mut_bindings`], the table that
//! boxes it natively), a later argument runs code, and either the position is
//! a `&` the native backend takes as a VIEW of the binding's cell, or the
//! value is one JS hands on by reference (a struct, a list, a map, a fixed
//! array, a trait object; at a `&`, a scalar JS views as a `(base, key)`
//! pair) and the native copy is deferred.

use super::Transformer;
use crate::analyzer::{Expr, FunctionCall};
use crate::fx::FxHashSet as HashSet;
use crate::id::Id;
use crate::node::Convention;
use crate::type_::{NUMERIC_PRIMITIVE_NAMES, SCALAR_PRIMITIVE_NAMES, Type, TypeId};

/// How a callee receives one argument — the native emitter's `Receiving`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Receiving {
    ByValue,
    Ref,
    RefMut,
}

impl<'src> Transformer<'src> {
    /// F138: which of `call`'s arguments are read after its later arguments —
    /// empty when none is, so the walk is exactly B452's.
    pub(super) fn arguments_read_late(&self, call_id: Id, call: &FunctionCall) -> Vec<bool> {
        let arguments = &call.argument_ids;
        if arguments.len() < 2 {
            return Vec::new();
        }
        let captured = self
            .captured_mut_bindings
            .get_or_init(|| crate::captures::captured_mut_bindings(self.program));
        if captured.is_empty()
            || !arguments.iter().any(|argument| {
                self.captured_spine_root(self.written_place(*argument))
                    .is_some()
            })
        {
            return Vec::new();
        }
        // A call through a closure VALUE is not one (F138's scope: the native
        // emitter renders its arguments in order).
        let target = match self.program.entity_map.get(&call.subject_id) {
            Some(Expr::Local(target))
                if self.program.functions.contains_key(target)
                    || self.program.intrinsics.contains_key(target) =>
            {
                *target
            }
            _ => return Vec::new(),
        };
        let conventions = self.receiving_forms(Some(target));
        if let Some(intrinsic) = self.program.intrinsics.get(&target) {
            return self.intrinsic_receiver_reads_late(*intrinsic, arguments, &conventions);
        }
        let views_allowed = !self.program.dyn_method_calls.contains_key(&call_id)
            && self.views_its_arguments(target);
        let takes_a_view: Vec<bool> = (0..arguments.len())
            .map(|index| {
                views_allowed
                    && conventions.get(index) == Some(&Receiving::Ref)
                    && self.takes_a_cell_view(arguments, index)
            })
            .collect();
        let late: Vec<bool> = (0..arguments.len())
            .map(|index| self.reads_late(arguments, &conventions, &takes_a_view, index))
            .collect();
        if late.contains(&true) {
            late
        } else {
            Vec::new()
        }
    }

    /// Whether the argument at `index` waits for the later ones: a view of
    /// its binding's cell, or a deferred copy (see the module comment).
    fn reads_late(
        &self,
        arguments: &[Id],
        conventions: &[Receiving],
        takes_a_view: &[bool],
        index: usize,
    ) -> bool {
        let a_later_argument_runs_code = arguments[index + 1..]
            .iter()
            .any(|later| !self.runs_no_code(*later));
        if !a_later_argument_runs_code {
            return false;
        }
        if takes_a_view[index] {
            return true;
        }
        let convention = conventions
            .get(index)
            .copied()
            .unwrap_or(Receiving::ByValue);
        let argument = arguments[index];
        // A `&mut` argument over a captured binding (a `&mut self` receiver
        // among them) holds the binding's cell for the call, taken after the
        // by-value arguments are settled (§6.9: "the same way").
        if convention == Receiving::RefMut {
            return self.is_a_captured_place(self.written_place(argument));
        }
        let place = self.lent_place(argument);
        if !self.is_a_captured_place(place) {
            return false;
        }
        let by_value = convention == Receiving::ByValue;
        let read_late_on_js = self.expr_type_id(place).is_some_and(|type_id| {
            self.aliases_on_the_js_backend(type_id)
                || (!by_value && self.resolves_to_scalar_view_pointee(type_id))
        });
        if !read_late_on_js
            || (by_value && self.copy_applies(self.program.clone_sites.get(&argument)))
        {
            return false;
        }
        arguments.iter().enumerate().all(|(other, other_id)| {
            other == index
                || takes_a_view[other]
                || self.runs_no_code(*other_id)
                || match conventions
                    .get(other)
                    .copied()
                    .unwrap_or(Receiving::ByValue)
                {
                    Receiving::ByValue => true,
                    Receiving::Ref => self.computes_a_value(*other_id),
                    Receiving::RefMut => false,
                }
        })
    }

    /// F138 for an INTRINSIC: only its receiver can wait — a receiver it
    /// writes in place over a captured binding (the cell is borrowed after
    /// the other arguments are settled), or one of the reading intrinsics the
    /// native backend reads through a view of the cell
    /// ([`crate::captures::reads_its_receiver_in_place`]). Every other
    /// intrinsic reads its receiver where it stands.
    fn intrinsic_receiver_reads_late(
        &self,
        intrinsic: crate::analyzer::Intrinsic,
        arguments: &[Id],
        conventions: &[Receiving],
    ) -> Vec<bool> {
        let receiver = arguments[0];
        let place = self.written_place(receiver);
        let writes = crate::captures::mutates_its_receiver(intrinsic)
            || conventions.first() == Some(&Receiving::RefMut);
        let views = crate::captures::reads_its_receiver_in_place(intrinsic)
            && !self
                .expr_type_id(place)
                .is_some_and(|type_id| self.is_natively_copy(type_id));
        let late = (writes || views)
            && self.is_a_captured_place(place)
            && arguments[1..]
                .iter()
                .any(|later| !self.runs_no_code(*later));
        if late {
            let mut late = vec![false; arguments.len()];
            late[0] = true;
            late
        } else {
            Vec::new()
        }
    }

    /// Whether `place` is a spine over a captured `mut` binding whose
    /// subscripts run no code, so reading it later moves nothing but the
    /// read.
    fn is_a_captured_place(&self, place: Id) -> bool {
        self.captured_spine_root(place)
            .is_some_and(|(_, subscripts)| {
                subscripts
                    .iter()
                    .all(|subscript| self.runs_no_code(*subscript))
            })
    }

    /// The place a `&` or `&mut` the source wrote names, else the argument.
    fn written_place(&self, argument: Id) -> Id {
        match self.program.entity_map.get(&argument) {
            Some(&Expr::Reference(operand, _)) => operand,
            _ => argument,
        }
    }

    /// The place a `&` the source wrote names, else the argument itself.
    fn lent_place(&self, argument: Id) -> Id {
        match self.program.entity_map.get(&argument) {
            Some(&Expr::Reference(operand, false)) => operand,
            _ => argument,
        }
    }

    /// The captured `mut` binding `place` is a spine over — through fields,
    /// tuple slots and subscripts — and the spine's subscripts.
    fn captured_spine_root(&self, place: Id) -> Option<(Id, Vec<Id>)> {
        let captured = self.captured_mut_bindings.get()?;
        let mut subscripts = Vec::new();
        let mut current = place;
        loop {
            match *self.program.entity_map.get(&current)? {
                Expr::Field(subject, _, _) | Expr::TupleIndex(subject, _, _) => current = subject,
                Expr::Index(subject, subscript) => {
                    subscripts.push(subscript);
                    current = subject;
                }
                Expr::Local(binding) | Expr::Parameter(binding) if captured.contains(&binding) => {
                    return Some((binding, subscripts));
                }
                _ => return None,
            }
        }
    }

    /// The callee's parameter modes by position (none known: by value).
    fn receiving_forms(&self, target: Option<Id>) -> Vec<Receiving> {
        let Some(parameters) = target.and_then(|target| {
            self.program
                .functions
                .get(&target)
                .map(|function| &function.parameters)
                .or_else(|| {
                    self.program
                        .external_functions
                        .get(&target)
                        .map(|external| &external.parameters)
                })
        }) else {
            return Vec::new();
        };
        parameters
            .iter()
            .filter_map(|parameter| self.program.parameters.get(parameter))
            .map(|parameter| match parameter.convention {
                Convention::Ref => Receiving::Ref,
                Convention::RefMut => Receiving::RefMut,
                Convention::Bare if parameter.name == "self" && !parameter.mutable => {
                    Receiving::Ref
                }
                Convention::Bare | Convention::Own => Receiving::ByValue,
            })
            .collect()
    }

    /// Whether a call to `target` takes its `&` arguments over a captured
    /// binding as views of the binding's cell natively: the callee answers an
    /// owned value now — no view, no projection of one, not a future.
    fn views_its_arguments(&self, target: Id) -> bool {
        !self.program.async_functions.contains(&target)
            && !self.program.async_values.contains(&target)
            && self.program.functions.get(&target).is_some_and(|function| {
                !function.is_async
                    && function.borrows.is_empty()
                    && !function.returns_view
                    && !function.returns_mut_view
            })
    }

    /// A `&` argument over a captured binding the native backend reads as a
    /// view of the binding's cell: its value is not natively `Copy`, and no
    /// other argument carries a closure that writes the binding (that call is
    /// handed a copy taken in order instead).
    fn takes_a_cell_view(&self, arguments: &[Id], index: usize) -> bool {
        let place = self.lent_place(arguments[index]);
        let Some((binding, _)) = self.captured_spine_root(place) else {
            return false;
        };
        let natively_copy = self
            .expr_type_id(place)
            .is_some_and(|type_id| self.is_natively_copy(type_id));
        !natively_copy
            && !arguments.iter().enumerate().any(|(other, carried)| {
                other != index
                    && self.closure_writes(*carried, binding, &mut HashSet::default(), false)
            })
    }

    /// A numeric scalar or a `bool`: a plain `Copy` value natively.
    fn is_natively_copy(&self, type_id: TypeId) -> bool {
        match self
            .program
            .type_id_to_type_map
            .get(&self.resolve_type_id(type_id))
        {
            Some(Type::Struct(id, _)) => self.program.structs.get(id).is_some_and(|declaration| {
                declaration.name != "BigInt" && NUMERIC_PRIMITIVE_NAMES.contains(&declaration.name)
            }),
            Some(Type::Enum(id, _)) => self.program.bool_enum_id == Some(*id),
            _ => false,
        }
    }

    /// Whether the JS backend hands a value of `type_id` on BY REFERENCE — a
    /// struct other than a scalar, `BigInt` or a `Shared` handle, a fixed
    /// array, a trait object. An enum and a tuple are read where they stand.
    fn aliases_on_the_js_backend(&self, type_id: TypeId) -> bool {
        match self
            .program
            .type_id_to_type_map
            .get(&self.resolve_type_id(type_id))
        {
            Some(Type::Struct(id, _)) => self.program.structs.get(id).is_some_and(|declaration| {
                !(declaration.external
                    && (SCALAR_PRIMITIVE_NAMES.contains(&declaration.name)
                        || declaration.name == "Shared"))
            }),
            Some(Type::Array(..) | Type::Dyn(..)) => true,
            _ => false,
        }
    }

    /// Whether a closure that `id` carries — a literal, or one a `let`-bound
    /// name holds (in a value its initializer built, too) — WRITES a place
    /// rooted at `binding`. `inside` is whether the walk is in a closure's
    /// body already.
    fn closure_writes(&self, id: Id, binding: Id, visited: &mut HashSet<Id>, inside: bool) -> bool {
        if !visited.insert(id) {
            return false;
        }
        match self.program.entity_map.get(&id) {
            Some(Expr::Closure(closure_id)) => {
                return self
                    .program
                    .closures
                    .get(closure_id)
                    .is_some_and(|closure| {
                        self.closure_writes(closure.return_, binding, visited, true)
                    });
            }
            Some(&Expr::Local(named)) => {
                return self
                    .program
                    .variables
                    .get(&named)
                    .and_then(|variable| variable.initial)
                    .is_some_and(|initial| self.closure_writes(initial, binding, visited, false));
            }
            Some(&Expr::Variable(declared)) if inside => {
                return self
                    .program
                    .variables
                    .get(&declared)
                    .and_then(|variable| variable.initial)
                    .is_some_and(|initial| self.closure_writes(initial, binding, visited, inside));
            }
            _ => {}
        }
        if inside && self.writes_binding_at(id, binding) {
            return true;
        }
        crate::captures::expression_children(self.program, id)
            .into_iter()
            .any(|child| self.closure_writes(child, binding, visited, inside))
    }

    /// Whether `id` itself writes a place rooted at `binding`: an assignment,
    /// a written `&mut`, a `&mut` argument or a mutating intrinsic's receiver.
    fn writes_binding_at(&self, id: Id, binding: Id) -> bool {
        let rooted = |place: Id| {
            let place = match self.program.entity_map.get(&place) {
                Some(&Expr::Reference(operand, _)) => operand,
                _ => place,
            };
            self.captured_spine_root(place)
                .is_some_and(|(root, _)| root == binding)
        };
        match self.program.entity_map.get(&id) {
            Some(&Expr::Assignment(target, _)) => rooted(target),
            Some(&Expr::Reference(operand, true)) => rooted(operand),
            Some(Expr::Call(call_id)) => {
                let Some(call) = self.program.function_calls.get(call_id) else {
                    return false;
                };
                let target = match self.program.entity_map.get(&call.subject_id) {
                    Some(Expr::Local(target)) => Some(*target),
                    _ => None,
                };
                if let Some(intrinsic) =
                    target.and_then(|target| self.program.intrinsics.get(&target))
                {
                    let mutating = crate::captures::mutates_its_receiver(*intrinsic)
                        || matches!(
                            intrinsic,
                            crate::analyzer::Intrinsic::ListPush
                                | crate::analyzer::Intrinsic::OptionTake
                                | crate::analyzer::Intrinsic::OptionReplace
                        );
                    return mutating
                        && call
                            .argument_ids
                            .first()
                            .is_some_and(|receiver| rooted(*receiver));
                }
                let conventions = self.receiving_forms(target);
                let known =
                    target.is_some_and(|target| self.program.functions.contains_key(&target));
                call.argument_ids
                    .iter()
                    .enumerate()
                    .any(|(index, argument)| {
                        rooted(*argument)
                            && (!known
                                || conventions
                                    .get(index)
                                    .is_none_or(|convention| *convention == Receiving::RefMut))
                    })
            }
            _ => false,
        }
    }

    /// Whether evaluating `id` runs no code that could write anything — a
    /// literal, a binding read, a closure being made, and the fields, slots,
    /// subscripts, references, aggregates and variants of those.
    ///
    /// The native emitter's twin counts a variant constructor's call as code;
    /// here it is not, so `add(self, label, Some(context))` keeps its order:
    /// building a variant writes nothing, so whether a binding beside it is
    /// read before or after cannot be told apart.
    fn runs_no_code(&self, id: Id) -> bool {
        let Some(_guard) = crate::util::RecursionGuard::enter() else {
            return false;
        };
        match self.program.entity_map.get(&id) {
            Some(
                Expr::Bool(_)
                | Expr::Number(..)
                | Expr::String(_)
                | Expr::MultilineString(_)
                | Expr::Null
                | Expr::Void
                | Expr::Local(_)
                | Expr::Parameter(_)
                | Expr::Closure(_)
                | Expr::Function(_)
                | Expr::LengthValue(_),
            ) => true,
            Some(
                &Expr::Field(subject, _, _)
                | &Expr::TupleIndex(subject, _, _)
                | &Expr::Reference(subject, _)
                | &Expr::Dereference(subject)
                | &Expr::Ascribe(subject),
            ) => self.runs_no_code(subject),
            Some(&Expr::Index(subject, subscript)) => {
                self.runs_no_code(subject) && self.runs_no_code(subscript)
            }
            Some(Expr::List(elements) | Expr::Tuple(elements)) => {
                elements.iter().all(|element| self.runs_no_code(*element))
            }
            Some(Expr::StructInitializer(_, fields)) => {
                fields.values().all(|field| self.runs_no_code(*field))
            }
            Some(Expr::Call(call_id)) => {
                self.program
                    .function_calls
                    .get(call_id)
                    .is_some_and(|call| {
                        matches!(
                            self.program.entity_map.get(&call.subject_id),
                            Some(Expr::Local(target))
                                if matches!(
                                    self.program.entity_map.get(target),
                                    Some(Expr::EnumVariant(..))
                                )
                        ) && call
                            .argument_ids
                            .iter()
                            .all(|argument| self.runs_no_code(*argument))
                    })
            }
            _ => false,
        }
    }

    /// Whether a `&` argument COMPUTES its value rather than naming a place —
    /// a call (not one answering a view), an operator, a block.
    fn computes_a_value(&self, id: Id) -> bool {
        if let Some(&Expr::Reference(operand, false)) = self.program.entity_map.get(&id) {
            return self.computes_a_value(operand);
        }
        let answers_a_view = match self.program.entity_map.get(&id) {
            Some(Expr::Call(call_id)) => self
                .program
                .function_calls
                .get(call_id)
                .and_then(|call| match self.program.entity_map.get(&call.subject_id) {
                    Some(Expr::Local(target)) => self.program.functions.get(target),
                    _ => None,
                })
                .is_some_and(|function| function.returns_view || function.returns_mut_view),
            _ => false,
        };
        !answers_a_view
            && !matches!(
                self.program.entity_map.get(&id),
                Some(
                    Expr::Local(_)
                        | Expr::Parameter(_)
                        | Expr::Variable(_)
                        | Expr::Field(..)
                        | Expr::TupleIndex(..)
                        | Expr::Index(..)
                        | Expr::Dereference(_)
                        | Expr::Reference(..)
                ) | None
            )
    }
}
