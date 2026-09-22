//! Which implementation a CONCRETE type selects for a trait member, over the
//! finished [`Program`] — the emission-side counterpart of the analyzer's
//! method resolution (`proposal/method-resolution.md` §13.4(a)), and the one
//! place the specificity order is written for consumers that see a program
//! rather than an analyzer.
//!
//! The analyzer answers this question for a receiver whose type it can see
//! (`resolve_impl_member`); a call reached through a BOUND cannot be answered
//! there, because the receiver is a generic parameter until monomorphization
//! binds it. The transformer therefore re-asks at emission — and until B158 it
//! re-asked with a nominal head comparison that could not see a blanket impl's
//! generic subject at all and ranked nothing, so a blanket reached through a
//! bound resolved to the trait's body-less requirement (the accept-then-ICE
//! B158 filed) and an impl pair the analyzer ranked by specificity was
//! answered here by declaration order (a silently wrong body).
//!
//! The order, one rule, three tiers, matching the spec (§5.4):
//!
//! 1. **Applicability.** An impl applies to a concrete type when its subject
//!    PATTERN matches it, the pattern's binders read as holes and the concrete
//!    type as rigid: `Box<type T>` applies to `Box<i32>`, `Box<i32>` does not
//!    apply to `Box<type T>`, and a blanket `type T` applies to everything —
//!    unless a binder carries a bound, which must hold for the type that
//!    position binds (`impl type T: Marker` reaches only the `Marker` types).
//! 2. **Instantiation.** When the caller knows which trait instantiation it
//!    wants — a bound-directed call always does — an applying impl is kept
//!    only if the arguments it provides FOR THIS RECEIVER are those arguments.
//!    A blanket `impl type T with MaybeSignal<T>` provides
//!    `MaybeSignal<SignalCell<str>>` on a `SignalCell<str>` where
//!    `impl SignalCell<type T> with MaybeSignal<T>` provides `MaybeSignal<str>`,
//!    so the bound alone separates them and each is reachable.
//! 3. **Specificity.** Among the survivors, the impl whose subject the others
//!    match and which matches none of theirs wins — any constructor-headed
//!    subject over a bare `type T`, so a blanket is the LEAST specific tier.
//!    Subjects of equal shape are ranked by their binders' bounds
//!    (`Box<type T: Marker>` over `Box<type T>`).
//!
//! Maxima that do not rank are the residue the analyzer reports at the call
//! site (§13.4(a)(3)); this returns the first of them in declaration order, so
//! an unranked pair that reached emission answers exactly as it did before
//! rather than turning a reported program into a second failure.

use crate::analyzer::{Implementation, Program, SourceId};
use crate::fx::FxHashMap as HashMap;
use crate::id::Id;
use crate::type_::{Type, TypeId};

/// One selected impl member: the function to call, and the declaring impl's
/// subject IN ITS OWN GENERIC TERMS (`List<Generic(T)>`), which the caller
/// binds against the concrete receiver to monomorphize the body.
#[derive(Clone, Copy, Debug)]
pub struct SelectedMember {
    pub member_id: Id,
    pub impl_subject: TypeId,
}

/// The trait instantiation a bound-directed call wants: the trait, and the
/// arguments the bound wrote (already grounded by the caller).
#[derive(Clone, Copy)]
pub struct WantedTrait<'a> {
    pub trait_id: Id,
    pub arguments: &'a [TypeId],
}

/// Whether a type is concrete enough to select an implementation for. A
/// position that is still a parameter, or that never resolved, selects
/// nothing: a blanket subject would otherwise "apply" to it and answer a call
/// whose receiver is not yet known.
pub fn is_resolvable(type_: &Type) -> bool {
    !matches!(
        type_,
        Type::Generic(_) | Type::Any | Type::Unknown | Type::Unresolved | Type::Trait(..)
    )
}

/// Whether `subject`'s SHAPE — an impl subject, in the impl's own generic
/// terms — matches `target`, the subject's binders read as holes and the
/// target as rigid. `Box<type T>` matches `Box<i32>` and `Box<List<i32>>`;
/// neither of those matches `Box<type T>`.
///
/// That asymmetry is the specificity order's first tier: where compatibility
/// answers yes in both directions, this answers yes in exactly one, and the
/// direction it refuses is which impl is more specific. Binder BOUNDS are not
/// consulted — [`subject_applies`] checks those against a concrete type, and
/// [`subject_outranks`] ranks them once the shapes are known equal.
pub fn subject_shape_matches(program: &Program, subject: TypeId, target: TypeId) -> bool {
    let Some(_guard) = crate::util::RecursionGuard::enter() else {
        return false;
    };
    let (Some(subject_type), Some(target_type)) = (
        program.type_id_to_type_map.get(&subject),
        program.type_id_to_type_map.get(&target),
    ) else {
        return false;
    };
    match (subject_type, target_type) {
        // A hole matches anything, including another hole; a constructor
        // headed subject does not match a hole.
        (Type::Generic(_), _) => true,
        (_, Type::Generic(_)) => false,
        (Type::Struct(left_id, left_arguments), Type::Struct(right_id, right_arguments))
        | (Type::Enum(left_id, left_arguments), Type::Enum(right_id, right_arguments))
        | (Type::Trait(left_id, left_arguments), Type::Trait(right_id, right_arguments)) => {
            left_id == right_id
                && subject_argument_shapes_match(program, left_arguments, right_arguments)
        }
        (Type::Tuple(left_items), Type::Tuple(right_items)) => {
            subject_argument_shapes_match(program, left_items, right_items)
        }
        (Type::Array(left_item, left_length), Type::Array(right_item, right_length)) => {
            left_length == right_length && subject_shape_matches(program, *left_item, *right_item)
        }
        (left, right) => left == right,
    }
}

/// Argument lists for [`subject_shape_matches`]. An ERASED side — a subject
/// written `List` with no arguments — carries no shape to compare, so the
/// heads alone decide, exactly as the analyzer's counterpart treats it.
fn subject_argument_shapes_match(program: &Program, subject: &[TypeId], target: &[TypeId]) -> bool {
    if subject.is_empty() || target.is_empty() {
        return true;
    }
    subject.len() == target.len()
        && subject
            .iter()
            .zip(target.iter())
            .all(|(subject_id, target_id)| subject_shape_matches(program, *subject_id, *target_id))
}

/// The traits an implementation provides, its `with` clause plus every
/// supertrait those pull in.
fn provided_trait_ids(program: &Program, implementation: &Implementation) -> Vec<Id> {
    let mut stack: Vec<Id> = implementation.trait_ids.clone();
    let mut provided: Vec<Id> = Vec::new();
    while let Some(trait_id) = stack.pop() {
        if provided.contains(&trait_id) {
            continue;
        }
        provided.push(trait_id);
        let Some(trait_) = program.traits.get(&trait_id) else {
            continue;
        };
        for supertrait in &trait_.supertraits {
            if let Some(Type::Trait(super_id, _)) = program.type_id_to_type_map.get(supertrait) {
                stack.push(*super_id);
            }
        }
    }
    provided
}

/// Whether some implementation provides `trait_id` for `type_id` — what a
/// binder's bound demands of the argument it binds. A position that is not
/// concrete proves nothing and is treated as satisfied, matching the analyzer's
/// own leniency in the bound audit.
fn provides_trait(program: &Program, type_id: TypeId, trait_id: Id) -> bool {
    let Some(type_) = program.type_id_to_type_map.get(&type_id) else {
        return true;
    };
    if !is_resolvable(type_) {
        return true;
    }
    // A124 R3: a trait OBJECT provides the trait it was erased to and that
    // trait's supertraits — its table is the implementation, and no impl
    // subject names it. That is what lets a blanket `impl type S: Src with
    // Loud` apply to a `dyn Src`, with `S` bound to the object.
    if let Type::Dyn(object_trait_id, _) = type_
        && object_provides(program, *object_trait_id, trait_id)
    {
        return true;
    }
    // A question already being asked further up this proof is NOT evidence
    // for itself. `trait Feed<T> with Source<List<T>>` beside
    // `impl type S: Source<List<type T>> with Feed<T>` makes the blanket a
    // provider of `Source` (a supertrait comes with the trait), and whether
    // it applies to a type is whether that type provides `Source` — the
    // question that reached it. Asked again it recursed until the stack was
    // gone, aborting every program that imported the module declaring the
    // pair. The cycle proves nothing, so it answers no, and the type's OTHER
    // providers decide: a cell with its own `Source` impl still provides it,
    // and a type with none still does not.
    let Some(_proving) = ProvingGuard::enter(type_id, trait_id) else {
        return false;
    };
    program.implementations.iter().any(|implementation| {
        provided_trait_ids(program, implementation).contains(&trait_id)
            && subject_applies(program, implementation.subject, type_id)
    })
}

thread_local! {
    /// The `(type, trait)` questions [`provides_trait`] is answering on this
    /// thread, innermost last — the proof's own stack, so a question that
    /// reaches itself is seen as the cycle it is.
    static PROVING: std::cell::RefCell<Vec<(TypeId, Id)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// One `(type, trait)` question held open on [`PROVING`] for as long as the
/// guard lives; `None` when that question is already open further up.
struct ProvingGuard;

impl ProvingGuard {
    fn enter(type_id: TypeId, trait_id: Id) -> Option<ProvingGuard> {
        PROVING.with(|proving| {
            let mut proving = proving.borrow_mut();
            if proving.contains(&(type_id, trait_id)) {
                return None;
            }
            proving.push((type_id, trait_id));
            Some(ProvingGuard)
        })
    }
}

impl Drop for ProvingGuard {
    fn drop(&mut self) {
        PROVING.with(|proving| {
            proving.borrow_mut().pop();
        });
    }
}

/// Whether an object over `object_trait_id` provides `trait_id`: the trait
/// itself, or one of its supertraits.
fn object_provides(program: &Program, object_trait_id: Id, trait_id: Id) -> bool {
    let mut stack = vec![object_trait_id];
    let mut seen: Vec<Id> = Vec::new();
    while let Some(id) = stack.pop() {
        if id == trait_id {
            return true;
        }
        if seen.contains(&id) {
            continue;
        }
        seen.push(id);
        if let Some(trait_) = program.traits.get(&id) {
            for supertrait_type_id in &trait_.supertraits {
                if let Some(Type::Trait(super_id, _)) =
                    program.type_id_to_type_map.get(supertrait_type_id)
                {
                    stack.push(*super_id);
                }
            }
        }
    }
    false
}

/// Whether `subject` — an impl subject, in the impl's own generic terms —
/// applies to the concrete `target`. Tier 1 of the module order: the shape
/// matches AND every binder's bounds hold for the type that position binds, so
/// `impl type T: Marker with Show` reaches only the types that are `Marker`.
pub fn subject_applies(program: &Program, subject: TypeId, target: TypeId) -> bool {
    if !subject_shape_matches(program, subject, target) {
        return false;
    }
    let mut bindings = HashMap::default();
    bind_subject(program, subject, target, &mut bindings);
    bindings.iter().all(|(constraint_id, bound_type)| {
        bound_trait_ids(program, *constraint_id)
            .iter()
            .all(|trait_id| provides_trait(program, *bound_type, *trait_id))
    })
}

/// [`subject_applies`], with each binder's PARAMETERIZED bounds read at their
/// own arguments (B268).
///
/// `bound_trait_ids` keeps the bound's trait id and drops its arguments, so
/// every blanket over a parameterized trait applied to every instantiation of
/// it: `impl type S: Source<str> with Slot` applied to a `SignalCell<View>`,
/// and a `Signal<View>` in child position was placed by std's TEXT arm — the
/// DOM taking the view's runtime shape, `[object Object]`.
///
/// Used only by [`applying_implementations`], and only as a NARROWING pass
/// there: the analyzer's own bound check threads no arguments through a
/// supertrait match (`satisfies_trait_bound`, v1), so a program it admitted
/// must still find a body here. The public [`subject_applies`] stays at the
/// trait id, which is what its other callers ask of it.
fn subject_applies_at_arguments(program: &Program, subject: TypeId, target: TypeId) -> bool {
    if !subject_applies(program, subject, target) {
        return false;
    }
    let mut bindings = HashMap::default();
    bind_subject(program, subject, target, &mut bindings);
    bindings.iter().all(|(constraint_id, bound_type)| {
        bound_type_ids(program, *constraint_id)
            .iter()
            .all(|bound_id| match program.type_id_to_type_map.get(bound_id) {
                Some(Type::Trait(trait_id, bound_arguments)) => {
                    bound_arguments_hold(program, *bound_type, *trait_id, bound_arguments)
                }
                _ => true,
            })
    })
}

/// Whether `concrete` provides `trait_id` AT `wanted` — the bound's own
/// arguments. Lenient wherever it cannot tell, like [`instantiation_agrees`]
/// itself: an unparameterized bound, a receiver with no impl of the trait in
/// view, an arity mismatch, and a bound argument that is still a binder
/// (`type S: Source<type T>`) all keep the impl.
fn bound_arguments_hold(
    program: &Program,
    concrete: TypeId,
    trait_id: Id,
    wanted: &[TypeId],
) -> bool {
    if wanted.is_empty() {
        return true;
    }
    let Some(provided) = provided_trait_arguments(program, concrete, trait_id) else {
        return true;
    };
    if provided.len() != wanted.len() {
        return true;
    }
    wanted.iter().zip(provided).all(|(wanted_id, provided_id)| {
        match (
            program.type_id_to_type_map.get(wanted_id),
            program.type_id_to_type_map.get(&provided_id),
        ) {
            (Some(wanted), Some(provided)) => instantiation_agrees(program, wanted, provided),
            _ => true,
        }
    })
}

/// The binders a subject writes, in walk order — the positions
/// [`bounds_are_stronger`] aligns.
fn collect_subject_binders(program: &Program, subject: TypeId, binders: &mut Vec<TypeId>) {
    let Some(_guard) = crate::util::RecursionGuard::enter() else {
        return;
    };
    match program.type_id_to_type_map.get(&subject) {
        Some(Type::Generic(constraint_id)) => binders.push(*constraint_id),
        Some(Type::Struct(_, arguments) | Type::Enum(_, arguments) | Type::Trait(_, arguments)) => {
            for argument in arguments.clone() {
                collect_subject_binders(program, argument, binders);
            }
        }
        Some(Type::Tuple(items)) => {
            for item in items.clone() {
                collect_subject_binders(program, item, binders);
            }
        }
        Some(Type::Array(item, _)) => collect_subject_binders(program, *item, binders),
        _ => {}
    }
}

/// The trait ids a generic parameter's bound names — the [`Program`]-side
/// reading of the analyzer's `generic_bound_trait_ids`: a multi-bound lives in
/// `generic_bounds`, a single one is recoverable from the constraint id.
fn bound_trait_ids(program: &Program, constraint_id: TypeId) -> Vec<Id> {
    let bounds = program
        .generic_bounds
        .get(&constraint_id)
        .cloned()
        .unwrap_or_else(|| vec![constraint_id]);
    bounds
        .iter()
        .filter_map(|type_id| match program.type_id_to_type_map.get(type_id) {
            Some(Type::Trait(trait_id, _)) => Some(*trait_id),
            _ => None,
        })
        .collect()
}

/// Tier 3's second half: with the subject shapes equal up to binder renaming,
/// the impl whose binders carry a strictly stronger bound set is more specific
/// (`Box<type T: Display>` ≻ `Box<type T>`).
fn bounds_are_stronger(program: &Program, stronger: TypeId, weaker: TypeId) -> bool {
    let mut stronger_binders = Vec::new();
    let mut weaker_binders = Vec::new();
    collect_subject_binders(program, stronger, &mut stronger_binders);
    collect_subject_binders(program, weaker, &mut weaker_binders);
    if stronger_binders.is_empty() || stronger_binders.len() != weaker_binders.len() {
        return false;
    }
    let mut strictly = false;
    for (stronger_id, weaker_id) in stronger_binders.iter().zip(weaker_binders.iter()) {
        let stronger_bounds = bound_trait_closure(program, *stronger_id);
        let weaker_bounds = bound_trait_closure(program, *weaker_id);
        if !weaker_bounds
            .iter()
            .all(|trait_id| stronger_bounds.contains(trait_id))
        {
            return false;
        }
        if stronger_bounds
            .iter()
            .any(|trait_id| !weaker_bounds.contains(trait_id))
        {
            strictly = true;
        }
    }
    strictly
}

/// A binder's declared bounds CLOSED over the supertrait graph — the emission
/// side's copy of the analyzer's `bound_trait_closure` (B378), so a subtrait
/// bound outranks its supertrait bound here exactly as it does at check time.
/// The two must agree: a program the analyzer ranks and this does not would
/// dispatch to the loser's body.
fn bound_trait_closure(program: &Program, constraint_id: TypeId) -> Vec<Id> {
    let mut closure = Vec::new();
    let mut pending = bound_trait_ids(program, constraint_id);
    while let Some(trait_id) = pending.pop() {
        if closure.contains(&trait_id) {
            continue;
        }
        closure.push(trait_id);
        let Some(trait_) = program.traits.get(&trait_id) else {
            continue;
        };
        for supertrait in &trait_.supertraits {
            if let Some(Type::Trait(super_id, _)) = program.type_id_to_type_map.get(supertrait) {
                pending.push(*super_id);
            }
        }
    }
    closure
}

/// The specificity order over two impl subjects (tier 3): shape first, then
/// the binders' bounds when the shapes are equal.
pub fn subject_outranks(program: &Program, subject: TypeId, other: TypeId) -> bool {
    if subject == other {
        return false;
    }
    // Both sides are PATTERNS here, so this is the shape comparison: binder
    // bounds are the second tier below, not part of the first.
    let matches_forward = subject_shape_matches(program, subject, other);
    let matches_backward = subject_shape_matches(program, other, subject);
    if matches_backward && !matches_forward {
        return true;
    }
    if matches_forward && !matches_backward {
        return false;
    }
    matches_forward && bounds_are_stronger(program, subject, other)
}

/// Binds the generic parameters in `pattern` (an impl subject in its own
/// generic terms, `List<Generic(T)>`) from the matching positions of the
/// concrete `type_id` (`List<i32>`), accumulating `{T -> i32}`. Recurses
/// through nominal arguments, tuples, arrays, and closures so a nested
/// parameter (`List<List<T>>` -> `T = i32`) is reached.
///
/// It matches [`Type::Generic`] and nothing else, and that is COMPLETE rather
/// than a happy path: `Generic(constraint)` is the one spelling a parameter has
/// in a nominal declaration's body (B366, the invariant is written at
/// [`Type::Generic`] and gated by `tests/nominal_generic_spelling.rs`). A walk
/// that also accepted a bare constraint id would be defending against a
/// spelling the analyzer does not mint — which is what the emit-Rust backend's
/// own copy of this walk did, and what the gate lets it stop doing.
pub fn bind_subject(
    program: &Program,
    pattern: TypeId,
    type_id: TypeId,
    out: &mut HashMap<TypeId, TypeId>,
) {
    let Some(pattern_type) = program.type_id_to_type_map.get(&pattern).cloned() else {
        return;
    };
    if let Type::Generic(constraint_id) = pattern_type {
        out.insert(constraint_id, type_id);
        return;
    }
    let Some(concrete_type) = program.type_id_to_type_map.get(&type_id).cloned() else {
        return;
    };
    let zip_arguments = |out: &mut HashMap<TypeId, TypeId>,
                         pattern_arguments: &[TypeId],
                         concrete_arguments: &[TypeId]| {
        for (pattern_argument, concrete_argument) in
            pattern_arguments.iter().zip(concrete_arguments.iter())
        {
            bind_subject(program, *pattern_argument, *concrete_argument, out);
        }
    };
    match (pattern_type, concrete_type) {
        (Type::Struct(left, pattern_arguments), Type::Struct(right, concrete_arguments))
        | (Type::Enum(left, pattern_arguments), Type::Enum(right, concrete_arguments))
            if left == right =>
        {
            zip_arguments(out, &pattern_arguments, &concrete_arguments);
        }
        (Type::Tuple(pattern_arguments), Type::Tuple(concrete_arguments)) => {
            zip_arguments(out, &pattern_arguments, &concrete_arguments);
        }
        // `[T; n]` against `[i32; n]` binds `T = i32` through the element.
        (Type::Array(pattern_element, _), Type::Array(concrete_element, _)) => {
            bind_subject(program, pattern_element, concrete_element, out);
        }
        // B309: a clause binds no generic — it names context BINDINGS, not
        // types — so impl-argument recovery walks the shape and ignores it.
        (
            Type::Closure(pattern_parameters, pattern_return, _),
            Type::Closure(concrete_parameters, concrete_return, _),
        ) => {
            zip_arguments(out, &pattern_parameters, &concrete_parameters);
            bind_subject(program, pattern_return, concrete_return, out);
        }
        _ => {}
    }
}

/// Substitutes an impl's own binders out of a type it wrote, using bindings
/// recovered from the receiver — shallow by construction, since a trait
/// argument is either a binder or a type built from them.
fn ground(program: &Program, type_id: TypeId, bindings: &HashMap<TypeId, TypeId>) -> Option<Type> {
    let _guard = crate::util::RecursionGuard::enter()?;
    match program.type_id_to_type_map.get(&type_id)? {
        Type::Generic(constraint_id) => match bindings.get(constraint_id) {
            Some(bound) => program.type_id_to_type_map.get(bound).cloned(),
            None => Some(Type::Generic(*constraint_id)),
        },
        other => Some(other.clone()),
    }
}

/// Whether two grounded types name the same thing, for the instantiation
/// filter. A position still abstract on either side proves nothing and is
/// treated as agreeing — the same leniency the transformer's older
/// `trait_instantiation_conflicts` applied, so a program whose arguments were
/// already unambiguous keeps its answer.
fn instantiation_agrees(program: &Program, wanted: &Type, provided: &Type) -> bool {
    if !is_resolvable(wanted) || !is_resolvable(provided) {
        return true;
    }
    match (wanted, provided) {
        (Type::Struct(left, left_arguments), Type::Struct(right, right_arguments))
        | (Type::Enum(left, left_arguments), Type::Enum(right, right_arguments)) => {
            if left != right {
                return false;
            }
            if left_arguments.is_empty() || right_arguments.is_empty() {
                return true;
            }
            left_arguments.len() == right_arguments.len()
                && left_arguments.iter().zip(right_arguments).all(
                    |(wanted_id, provided_id)| match (
                        program.type_id_to_type_map.get(wanted_id),
                        program.type_id_to_type_map.get(provided_id),
                    ) {
                        (Some(wanted), Some(provided)) => {
                            instantiation_agrees(program, wanted, provided)
                        }
                        _ => true,
                    },
                )
        }
        (left, right) => left == right,
    }
}

/// Whether `implementation` provides `wanted` AT THE INSTANTIATION the caller
/// asked for, once its own binders are grounded from the concrete receiver
/// (tier 2). An impl that names the trait only through a supertrait threads no
/// arguments (v1) and is kept.
fn provides_wanted_instantiation(
    program: &Program,
    implementation: &Implementation,
    concrete: TypeId,
    wanted: WantedTrait,
) -> bool {
    if !implementation.trait_ids.contains(&wanted.trait_id) {
        return false;
    }
    if wanted.arguments.is_empty() {
        return true;
    }
    let Some((_, written)) = implementation
        .trait_args
        .iter()
        .find(|(trait_id, _)| *trait_id == wanted.trait_id)
    else {
        return true;
    };
    if written.len() != wanted.arguments.len() {
        return true;
    }
    let mut bindings = HashMap::default();
    bind_subject(program, implementation.subject, concrete, &mut bindings);
    bind_bound_binders(program, implementation.subject, &mut bindings);
    written
        .iter()
        .zip(wanted.arguments)
        .all(|(written_id, wanted_id)| {
            match (
                ground(program, *written_id, &bindings),
                program.type_id_to_type_map.get(wanted_id),
            ) {
                (Some(provided), Some(wanted)) => instantiation_agrees(program, wanted, &provided),
                _ => true,
            }
        })
}

/// Grounds the binders a subject's BOUNDS introduce (B165): in
/// `impl type S: Src<type T> with Maybe<T>`, `S` binds from the receiver and
/// `T` binds from the receiver's OWN `Src` implementation — `Cell: Src<i32>`
/// binds `T = i32`, so the impl provides `Maybe<i32>` on a `Cell` and not
/// `Maybe<Cell>`.
///
/// Without this the bound's binder stayed a hole, the arguments the impl
/// provides read as "not resolvable", and tier 2's `instantiation_agrees` kept
/// the impl on the leniency clause. Two blankets separated only by their
/// instantiation — the static `impl type T with Maybe<T>` and this one — then
/// reached tier 3 together, where the bounded binder is the stronger one and
/// won: a `holder<V: Maybe<Cell>>(cell)` ran the reactive body against a value
/// that was never a `Cell`, and printed `undefined`. A silent miscompile, and
/// only expressible once a binder could be written inside a bound at all.
fn bind_bound_binders(program: &Program, subject: TypeId, bindings: &mut HashMap<TypeId, TypeId>) {
    let mut binders = Vec::new();
    collect_subject_binders(program, subject, &mut binders);
    for binder in binders {
        let Some(concrete) = bindings.get(&binder).copied() else {
            continue;
        };
        for bound_id in bound_type_ids(program, binder) {
            let Some(Type::Trait(trait_id, bound_arguments)) =
                program.type_id_to_type_map.get(&bound_id).cloned()
            else {
                continue;
            };
            if bound_arguments.is_empty() {
                continue;
            }
            let Some(provided) = provided_trait_arguments(program, concrete, trait_id) else {
                continue;
            };
            if provided.len() != bound_arguments.len() {
                continue;
            }
            for (pattern, actual) in bound_arguments.iter().zip(provided) {
                bind_subject(program, *pattern, actual, bindings);
            }
        }
    }
}

/// The bound type ids a binder carries — the `Src<type T>` of
/// `type S: Src<type T>`, arguments and all. [`bound_trait_ids`] reads the same
/// list for its ids alone.
fn bound_type_ids(program: &Program, constraint_id: TypeId) -> Vec<TypeId> {
    program
        .generic_bounds
        .get(&constraint_id)
        .cloned()
        .unwrap_or_else(|| vec![constraint_id])
}

/// The arguments `concrete` provides for `trait_id`, through the first impl
/// that applies to it and names the trait — the emission-side reading of the
/// analyzer's `trait_args_for`.
fn provided_trait_arguments(
    program: &Program,
    concrete: TypeId,
    trait_id: Id,
) -> Option<Vec<TypeId>> {
    for implementation in &program.implementations {
        let Some((_, written)) = implementation
            .trait_args
            .iter()
            .find(|(provided, _)| *provided == trait_id)
        else {
            continue;
        };
        if !subject_applies(program, implementation.subject, concrete) {
            continue;
        }
        let mut bindings = HashMap::default();
        bind_subject(program, implementation.subject, concrete, &mut bindings);
        return Some(
            written
                .iter()
                .map(|argument| ground_id(program, *argument, &bindings))
                .collect(),
        );
    }
    None
}

/// [`ground`] at the id level, one step deep: a written argument that IS a
/// binder becomes what that binder bound, and anything else stays as written.
///
/// Shallow on purpose. A constructed argument (`Source<List<T>>`) would need a
/// substituted type INTERNED, which needs a mutable analyzer this side of the
/// pipeline does not have; leaving it as written keeps the binder a hole, which
/// the instantiation filter reads as "cannot tell" and keeps the impl — the
/// behaviour before this grounding existed.
fn ground_id(program: &Program, type_id: TypeId, bindings: &HashMap<TypeId, TypeId>) -> TypeId {
    match program.type_id_to_type_map.get(&type_id) {
        Some(Type::Generic(constraint_id)) => {
            bindings.get(constraint_id).copied().unwrap_or(type_id)
        }
        _ => type_id,
    }
}

/// Every implementation that applies to `concrete`, in declaration order,
/// filtered by the caller's `wanted` trait instantiation when it has one and by
/// the FILE the question is asked on behalf of.
///
/// **`file` is the per-importer namespace (B318 S4, `visibility.md` §3.3).** An
/// impl the file's imports did not admit is not a candidate here at all — not
/// merely a candidate that loses — because the whole point of the rule is that
/// two independent packages may declare one method name for one type and a
/// file that took only one of them must not be answered by the other. `None`
/// means "no file to scope by", which is the honest answer for a caller that
/// has none and is also what the estate always gets: nothing in it writes
/// `only` or a selector, so [`crate::analyzer::ImplAdmission::admits_impl`]
/// returns `true` without a lookup.
pub fn applying_implementations<'a, 'src>(
    program: &'a Program<'src>,
    file: Option<SourceId>,
    concrete: TypeId,
    wanted: Option<WantedTrait>,
) -> Vec<&'a Implementation<'src>> {
    let Some(concrete_type) = program.type_id_to_type_map.get(&concrete) else {
        return Vec::new();
    };
    if !is_resolvable(concrete_type) {
        return Vec::new();
    }
    // B318 S4: the per-importer namespace. Asked once for the FILE here rather
    // than once per registered block below — a file that restricts nothing has
    // today's meaning and there is nothing to filter.
    let scope = file.filter(|file| program.impl_admission.restricts(*file));
    let by_instantiation: Vec<&Implementation> = program
        .implementations
        .iter()
        .filter(|implementation| {
            scope.is_none_or(|file| program.impl_admission.admits_impl(file, implementation))
        })
        .filter(|implementation| match wanted {
            Some(wanted) => {
                provides_wanted_instantiation(program, implementation, concrete, wanted)
            }
            None => true,
        })
        .collect();
    // Tier 1, at the bounds' own arguments (B268) — the pass that separates
    // `impl type S: Source<str>` from `impl type S: Source<View>` on a
    // `SignalCell<View>`.
    let at_arguments: Vec<&Implementation> = by_instantiation
        .iter()
        .copied()
        .filter(|implementation| {
            subject_applies_at_arguments(program, implementation.subject, concrete)
        })
        .collect();
    if !at_arguments.is_empty() {
        return at_arguments;
    }
    // NARROWS, never empties — the same policy the analyzer's own
    // `applicable_candidates` keeps. A bound's arguments are read more strictly
    // here than in `satisfies_trait_bound`, which threads none through a
    // supertrait match, so a program the analyzer admitted would otherwise
    // resolve to its trait's body-less requirement and be reported as an
    // internal error. It keeps the body it has always had instead.
    by_instantiation
        .into_iter()
        .filter(|implementation| subject_applies(program, implementation.subject, concrete))
        .collect()
}

/// The maxima of the specificity order over `applying`, in declaration order.
/// One maximum is the winner; several are the residue the analyzer reports at
/// the call site.
fn maxima<'a, 'src>(
    program: &Program<'src>,
    applying: &[&'a Implementation<'src>],
) -> Vec<&'a Implementation<'src>> {
    applying
        .iter()
        .copied()
        .filter(|implementation| {
            !applying
                .iter()
                .any(|other| subject_outranks(program, other.subject, implementation.subject))
        })
        .collect()
}

/// Whether one of `implementation`'s traits (or their supertraits) carries a
/// BODIED declaration of `member` — the impl answers `member` by inheriting
/// that default, without declaring anything itself.
fn inherits_a_default(program: &Program, implementation: &Implementation, member: &str) -> bool {
    let mut stack: Vec<Id> = implementation.trait_ids.clone();
    let mut seen: Vec<Id> = Vec::new();
    while let Some(trait_id) = stack.pop() {
        if seen.contains(&trait_id) {
            continue;
        }
        seen.push(trait_id);
        let Some(trait_) = program.traits.get(&trait_id) else {
            continue;
        };
        if let Some(declaration_id) = trait_.declarations.get(member)
            && program
                .functions
                .get(declaration_id)
                .is_some_and(|function| function.has_body)
        {
            return true;
        }
        for supertrait in &trait_.supertraits {
            if let Some(Type::Trait(super_id, _)) = program.type_id_to_type_map.get(supertrait) {
                stack.push(*super_id);
            }
        }
    }
    false
}

/// The member a concrete type selects for `member`, by the module's order.
/// `wanted` narrows to one trait instantiation — what a bound-directed call
/// knows and a bare re-dispatch does not.
///
/// The ranking runs over every impl that could ANSWER the name, not only those
/// that declare it: an impl inheriting its trait's default is a contender
/// (`method-resolution.md` §13.2 row 17), so a more specific impl that
/// declares nothing still outranks a blanket that does. Such a winner returns
/// `None` — it has no member of its own, and the caller reaches its answer
/// through the trait default, which is the same verdict by the same order.
/// The maxima of the specificity order among the impls that DECLARE `member`
/// for `concrete`, under `file`'s admitted set — what [`select_member`] picks
/// its answer out of.
///
/// One maximum is the winner. TWO OR MORE is the unranked residue: nothing in
/// the order separates them, so `select_member` takes the first in declaration
/// order and the program's meaning becomes a function of which block was
/// written first. That is precisely what B57 exists to prevent, and B330's
/// call-site refusal is what reports it — the question it asks is this
/// function's length, because only the SITE, which knows the receiver, can ask
/// it at all.
pub fn declaring_maxima<'a, 'src>(
    program: &'a Program<'src>,
    file: Option<SourceId>,
    concrete: TypeId,
    member: &str,
) -> Vec<&'a Implementation<'src>> {
    let scope = file.filter(|file| program.impl_admission.restricts(*file));
    let contenders: Vec<&Implementation> = applying_implementations(program, file, concrete, None)
        .into_iter()
        .filter(
            |implementation| match implementation.declarations.get(member) {
                Some(member_id) => scope.is_none_or(|file| {
                    program
                        .impl_admission
                        .admits_member(file, implementation, *member_id)
                }),
                None => false,
            },
        )
        .collect();
    maxima(program, &contenders)
}

pub fn select_member(
    program: &Program,
    file: Option<SourceId>,
    concrete: TypeId,
    member: &str,
    wanted: Option<WantedTrait>,
) -> Option<SelectedMember> {
    let scope = file.filter(|file| program.impl_admission.restricts(*file));
    let contenders: Vec<&Implementation> =
        applying_implementations(program, file, concrete, wanted)
            .into_iter()
            .filter(|implementation| {
                // B318 S4, one step finer than the block filter above: a
                // `(impl T)::{ m }` selector takes ONE member out of a block,
                // so a block this file admits may still not offer `member`.
                match implementation.declarations.get(member) {
                    Some(member_id) => scope.is_none_or(|file| {
                        program
                            .impl_admission
                            .admits_member(file, implementation, *member_id)
                    }),
                    None => inherits_a_default(program, implementation, member),
                }
            })
            .collect();
    let winner = *maxima(program, &contenders).first()?;
    Some(SelectedMember {
        member_id: *winner.declarations.get(member)?,
        impl_subject: winner.subject,
    })
}

/// The implementation a concrete type selects for one trait, by the same
/// order — for a caller that wants the IMPL rather than one of its members
/// (which arguments it implements the trait at, say).
pub fn select_implementation<'a, 'src>(
    program: &'a Program<'src>,
    file: Option<SourceId>,
    concrete: TypeId,
    trait_id: Id,
) -> Option<&'a Implementation<'src>> {
    let applying = applying_implementations(
        program,
        file,
        concrete,
        Some(WantedTrait {
            trait_id,
            arguments: &[],
        }),
    );
    maxima(program, &applying).first().copied()
}

/// The traits a concrete type's applying implementations provide, most
/// specific first — the search order for an INHERITED trait default, which no
/// impl declares and which therefore cannot be found by [`select_member`].
pub fn applying_trait_ids(program: &Program, file: Option<SourceId>, concrete: TypeId) -> Vec<Id> {
    let applying = applying_implementations(program, file, concrete, None);
    let winners = maxima(program, &applying);
    winners
        .iter()
        .chain(applying.iter())
        .flat_map(|implementation| implementation.trait_ids.iter().copied())
        .collect()
}
