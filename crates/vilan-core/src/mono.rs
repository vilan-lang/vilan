//! The monomorphisation-RESOLUTION questions both emitters ask of a finished
//! `Program` (tracker F26).
//!
//! # Why this module exists
//!
//! `transformer.rs` (JavaScript) and `vilan-rust` (native) are two emitters
//! over ONE `Program`, and a question about that program — "which generic
//! binders does this type mention", "which trait supplies the default body for
//! this member" — has exactly one answer, whoever is asking. Written twice, the
//! two copies were held together only by the whole-set differential, which can
//! only notice a drift after a program has been compiled wrong by one of them.
//! Written once, they cannot drift.
//!
//! # What belongs here, and what deliberately does not
//!
//! Only the questions whose answer is a property of the PROGRAM, plus — where
//! the emitters genuinely ask differently — the difference as an explicit
//! parameter. Impl ADMISSION is the one such difference: the JS emitter asks
//! on behalf of the file whose imports admitted the instance (B318 S4), the
//! native one with `None`, which admits every impl. It is a parameter because
//! dropping it is invisible to the whole-set differential — nothing in the
//! estate restricts an import, so every file admits every impl today — and a
//! silent default would be exactly the drift this module exists to prevent.
//!
//! What stays per-emitter, and why:
//!
//! - `type_key` / the JS `resolve_type_id` and the native `concrete`: they
//!   read the emitter's ACTIVE substitution, and the native key resolves
//!   through it at every level where the JS one resolves only where asked.
//!   Lifting them would need the difference back as a parameter, which is the
//!   same two functions with a harder signature.
//! - `inherited_substitution` and `call_substitution`: both filter the
//!   emitter's active substitution, and the two have DIVERGED in what they
//!   bind — the native ones also bind a callee's generic parameters its
//!   signature does not mention and the call's positional own-generic values
//!   (`own_generic_call_bindings`), which the JS ones do not. Folding them
//!   would move JS output; the seed they share, [`signature_generics`], is
//!   here.
//!
//! Every function here takes `&Program` (and at most an admission file), so
//! neither emitter's state can leak into an answer.

use crate::analyzer::{Expr, Program, SourceId};
use crate::fx::{FxHashMap as HashMap, FxHashSet as HashSet};
use crate::id::Id;
use crate::impl_select;
use crate::type_::{Type, TypeId};

/// Whether the member `member_id` names has a BODY — a trait declaration with
/// a default, rather than a bare requirement.
pub fn function_has_body(program: &Program<'_>, member_id: Id) -> bool {
    match program.entity_map.get(&member_id) {
        Some(Expr::Function(function_id)) => program
            .functions
            .get(function_id)
            .is_some_and(|function| function.has_body),
        _ => false,
    }
}

/// Every generic BINDER a type mentions, in first-seen order, deduplicated.
///
/// The walk is depth-bounded because a type can be cyclic through a
/// declaration and the answer is a list of binders, not a proof of
/// termination: 24 levels is past anything a signature writes, and a binder
/// deeper than that would be one nothing can bind anyway.
pub fn collect_type_generics(
    program: &Program<'_>,
    type_id: TypeId,
    depth: usize,
    out: &mut Vec<TypeId>,
) {
    if depth > 24 {
        return;
    }
    match program.type_id_to_type_map.get(&type_id) {
        Some(Type::Generic(constraint_id)) => {
            if !out.contains(constraint_id) {
                out.push(*constraint_id);
            }
        }
        Some(Type::Struct(_, arguments) | Type::Enum(_, arguments) | Type::Tuple(arguments)) => {
            for argument in arguments.clone() {
                collect_type_generics(program, argument, depth + 1, out);
            }
        }
        Some(Type::Closure(parameters, return_type_id, _)) => {
            let parameters = parameters.clone();
            let return_type_id = *return_type_id;
            for parameter in parameters {
                collect_type_generics(program, parameter, depth + 1, out);
            }
            collect_type_generics(program, return_type_id, depth + 1, out);
        }
        Some(Type::Array(element_id, _)) => {
            collect_type_generics(program, *element_id, depth + 1, out);
        }
        _ => {}
    }
}

/// The trait member with a DEFAULT body that `member` resolves to, searching
/// `trait_id` and then its supertraits.
///
/// Breadth is not the question — a name is declared once in a trait hierarchy
/// or the analyzer has already refused it — so the walk is a stack with a
/// `seen` guard, which is what makes a diamond of supertraits terminate.
pub fn trait_default_member(program: &Program<'_>, trait_id: Id, member: &str) -> Option<Id> {
    let mut stack = vec![trait_id];
    let mut seen = HashSet::default();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(trait_) = program.traits.get(&id) else {
            continue;
        };
        if let Some(&member_id) = trait_.declarations.get(member)
            && function_has_body(program, member_id)
        {
            return Some(member_id);
        }
        for supertrait_type_id in &trait_.supertraits {
            if let Some(Type::Trait(super_id, _)) =
                program.type_id_to_type_map.get(supertrait_type_id)
            {
                stack.push(*super_id);
            }
        }
    }
    None
}

/// Whether `trait_id`'s supertrait closure contains `supertrait_id` — "is an
/// impl of `trait_id` also an impl of `supertrait_id`'s surface" (B359's
/// supertrait face). A trait reaches itself.
pub fn trait_reaches_supertrait(program: &Program<'_>, trait_id: Id, supertrait_id: Id) -> bool {
    let mut stack = vec![trait_id];
    let mut seen = HashSet::default();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        if id == supertrait_id {
            return true;
        }
        let Some(trait_) = program.traits.get(&id) else {
            continue;
        };
        for supertrait_type_id in &trait_.supertraits {
            if let Some(Type::Trait(super_id, _)) =
                program.type_id_to_type_map.get(supertrait_type_id)
            {
                stack.push(*super_id);
            }
        }
    }
    false
}

/// The trait in `trait_id`'s supertrait closure that DECLARES `member`: the
/// member's declaration, the declaring trait, and the substitution the `with`
/// clauses on the way imply — each supertrait's own parameters bound to the
/// arguments its clause wrote, in the sub-trait's terms (`trait Named<T> with
/// Shape<T>` reached for `area` binds `Shape`'s `T` to `Named`'s `T`).
///
/// One walk for both emitters' object tables (A124 R3): the slot's signature
/// is written in the declaring trait's terms, and the slot's IMPL is selected
/// at the declaring trait's instantiation ([`object_member_preference`]).
pub fn object_member_declaration(
    program: &Program<'_>,
    trait_id: Id,
    member: &str,
) -> Option<(Id, Id, Vec<(TypeId, TypeId)>)> {
    let mut stack: Vec<(Id, Vec<(TypeId, TypeId)>)> = vec![(trait_id, Vec::new())];
    let mut seen: HashSet<Id> = HashSet::default();
    while let Some((id, chain)) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let trait_ = program.traits.get(&id)?;
        if let Some(declaration) = trait_.declarations.get(member) {
            return Some((*declaration, id, chain));
        }
        for supertrait_type_id in &trait_.supertraits {
            if let Some(Type::Trait(super_id, super_arguments)) =
                program.type_id_to_type_map.get(supertrait_type_id)
            {
                let mut extended = chain.clone();
                if let Some(supertrait) = program.traits.get(super_id) {
                    extended.extend(
                        supertrait
                            .generic_parameter_constraint_ids
                            .iter()
                            .copied()
                            .zip(super_arguments.iter().copied()),
                    );
                }
                stack.push((*super_id, extended));
            }
        }
    }
    None
}

/// The trait application an object over `trait_id<arguments>` dispatches
/// `member` on: the DECLARING trait, at the arguments the clause chain passes
/// it (B532). A member a supertrait declares is that supertrait's, so a table
/// for `dyn Named<str>` over `trait Named<T> with Shape<T>` takes `area` from
/// the impl of `Shape<str>` — asking for `Named<str>`'s instead matched no
/// impl providing `area`, and the by-name fallback answered from whichever
/// impl of `Shape` came first (`Shape<i32>`'s `4` where `"big"` is right).
///
/// The arguments are substituted at the top of each position (a clause
/// argument that IS a parameter of a trait on the way); one written in a
/// nested form (`with Shape<Option<T>>`) keeps its written id, which the
/// caller resolves under its own substitution as it resolves any argument.
pub fn object_member_preference(
    program: &Program<'_>,
    trait_id: Id,
    arguments: &[TypeId],
    member: &str,
) -> Option<(Id, Vec<TypeId>)> {
    let (_, declaring, chain) = object_member_declaration(program, trait_id, member)?;
    if declaring == trait_id {
        return Some((trait_id, arguments.to_vec()));
    }
    let mut map: HashMap<TypeId, TypeId> = HashMap::default();
    if let Some(trait_) = program.traits.get(&trait_id) {
        map.extend(
            trait_
                .generic_parameter_constraint_ids
                .iter()
                .copied()
                .zip(arguments.iter().copied()),
        );
    }
    map.extend(chain);
    let through = |mut type_id: TypeId| {
        // Bounded by the map's size: each hop follows one entry, and a chain
        // longer than that is a cycle.
        for _ in 0..=map.len() {
            let next = map.get(&type_id).copied().or_else(|| {
                match program.type_id_to_type_map.get(&type_id) {
                    Some(Type::Generic(constraint_id)) => map.get(constraint_id).copied(),
                    _ => None,
                }
            });
            match next {
                Some(next) if next != type_id => type_id = next,
                _ => break,
            }
        }
        type_id
    };
    let declared = program.traits.get(&declaring)?;
    let resolved = declared
        .generic_parameter_constraint_ids
        .iter()
        .map(|parameter| through(*parameter))
        .collect();
    Some((declaring, resolved))
}

/// The member a call dispatched on `trait_id`'s surface takes from an impl of
/// one of its SUB-traits — the second step of a bound-directed dispatch, after
/// the impl of `trait_id` itself has been asked (B359).
///
/// The member is declared by `trait_id` (the analyzer records the DECLARING
/// trait, B473), but the implementor may name only a sub-trait in its clause:
/// `Source<T>::sub`'s body calls `self.on_change(..)`, and a type that writes
/// `impl C with Signal<T>` provides `Source`'s members through that clause and
/// never names `Source`. The wanted-trait filter is a membership test on the
/// clause's own traits, so it turns that impl down; this asks the type's
/// PROVIDED traits (most specific first) whose supertrait closure reaches
/// `trait_id`. Without it the caller fell to the by-name lookup, which an
/// inherent member of the same name wins, or to the trait's default over the
/// override. `file` is admission, as in [`resolve_inherited_default`].
pub fn select_member_through_subtraits(
    program: &Program<'_>,
    file: Option<SourceId>,
    type_id: TypeId,
    trait_id: Id,
    member: &str,
) -> Option<impl_select::SelectedMember> {
    impl_select::applying_trait_ids(program, file, type_id)
        .into_iter()
        .filter(|provided| {
            *provided != trait_id && trait_reaches_supertrait(program, *provided, trait_id)
        })
        .find_map(|provided| {
            impl_select::select_member(
                program,
                file,
                type_id,
                member,
                Some(impl_select::WantedTrait {
                    trait_id: provided,
                    arguments: &[],
                }),
            )
        })
}

/// The default body a concrete receiver INHERITS for `member` from a trait it
/// implements.
///
/// The impl subject is written in its own generic terms (`SignalCell<T>`) and
/// the receiver in concrete ones (`SignalCell<i32>`), so the search is over the
/// impls that APPLY to the receiver ([`crate::impl_select`]): exact type
/// equality only ever matched non-generic subjects, silently dropping inherited
/// defaults on generic types, and the emitted call then bound to the trait's
/// abstract member.
///
/// `file` is the file whose imports admit impls into the search (the JS
/// emitter's current admitting file; the native emitter passes `None`, which
/// admits every impl in the program) — it is a parameter rather than a
/// constant because the two emitters genuinely ask with different admission,
/// and folding it to one would move one backend's output.
pub fn resolve_inherited_default(
    program: &Program<'_>,
    file: Option<SourceId>,
    type_id: TypeId,
    member: &str,
) -> Option<Id> {
    impl_select::applying_trait_ids(program, file, type_id)
        .into_iter()
        .find_map(|trait_id| trait_default_member(program, trait_id, member))
}

/// The generic binders a function's SIGNATURE mentions — its parameters' types
/// then its return type, first-seen order, deduplicated. The seed both
/// emitters' inherited substitution filters the active substitution through.
pub fn signature_generics(program: &Program<'_>, function_id: Id) -> Vec<TypeId> {
    let mut generics = Vec::new();
    let Some(function) = program.functions.get(&function_id) else {
        return generics;
    };
    for parameter_id in &function.parameters {
        if let Some(parameter) = program.parameters.get(parameter_id) {
            collect_type_generics(program, parameter.type_id, 0, &mut generics);
        }
    }
    if let Some(return_type_id) = function.return_type_id {
        collect_type_generics(program, return_type_id, 0, &mut generics);
    }
    generics
}

/// The substitution a trait DEFAULT body is specialized under (B58): the
/// declaring trait's own generic parameters bound to the arguments `type_id`
/// implements it at, plus the providing impl's binders bound from the concrete
/// receiver.
///
/// The declaring trait is the one whose declarations hold `default_id` — a
/// supertrait's default reached through a subtrait's impl keeps its own
/// parameters. The impl is SELECTED like every other dispatch lookup (the
/// subject is in its own generic terms, the receiver in concrete ones), so the
/// arguments are the ones the winning impl writes, not the first-declared
/// one's. A trait argument written in the impl's terms (`with Holder<E>`) stays
/// keyed to the impl's binder, and the caller's resolution composes the two
/// hops within this one map. `file` is admission, as in
/// [`resolve_inherited_default`].
pub fn trait_parameter_substitution(
    program: &Program<'_>,
    file: Option<SourceId>,
    default_id: Id,
    type_id: TypeId,
) -> HashMap<TypeId, TypeId> {
    let mut substitution = HashMap::default();
    let Some((trait_id, trait_)) = program
        .traits
        .iter()
        .find(|(_, trait_)| trait_.declarations.values().any(|id| *id == default_id))
    else {
        return substitution;
    };
    if trait_.generic_parameter_constraint_ids.is_empty() {
        return substitution;
    }
    let Some(implementation) =
        impl_select::select_implementation(program, file, type_id, *trait_id)
    else {
        return substitution;
    };
    impl_select::bind_subject_and_bounds(
        program,
        implementation.subject,
        type_id,
        &mut substitution,
    );
    let Some((_, arguments)) = implementation
        .trait_args
        .iter()
        .find(|(provided, _)| provided == trait_id)
    else {
        return substitution;
    };
    for (parameter_id, argument_id) in trait_
        .generic_parameter_constraint_ids
        .iter()
        .zip(arguments)
    {
        substitution.insert(*parameter_id, *argument_id);
    }
    substitution
}
