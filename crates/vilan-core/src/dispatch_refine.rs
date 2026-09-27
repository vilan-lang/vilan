//! Refinement of the dispatch edges the shared [`CallGraph`] deliberately
//! leaves indirect: which concrete callees a trait-dispatched call site can
//! actually select, recovered from the analyzer's records — the dispatch kind
//! (`generic_dispatch`, read through [`crate::async_infer::dispatch_at`]) and
//! the per-call-site substitutions (`method_call_substitution`, the single
//! channel every instantiation shape records into).
//!
//! The machinery shipped inside the `context` pass as its coverage-only
//! dispatch refinement (element-syntax H8 →
//! `proposal/requirement-polymorphism.md`) and moved here when the const-only
//! capability check needed the same edges (const-eval.md §2, backlog B143):
//! an `asset::emit` reached only through a bounded generic's trait dispatch
//! used to escape that check entirely, because the check propagates over call
//! edges and a dispatch site has none.
//!
//! The refinement per site:
//!
//! - an `OnType` site with a recorded concrete receiver narrows to the
//!   members the receiver's HEAD selects (substitution cannot change a
//!   head); a receiver-less `OnType` — a `self` call inside a shared trait
//!   default body — keeps every candidate;
//! - an `OnConstraint` site is resolved per ENTRY of the function owning the
//!   constraint: an entry call whose recorded bindings ground the constraint
//!   draws edges from THAT caller to only the impl members the concrete type
//!   selects; a binding leading to another generic parameter recurses to the
//!   entry's own enclosing function, so a forwarding wrapper resolves per
//!   call site; anything unresolvable — an opaque binding, a value-taken or
//!   dispatch-reachable level whose entries cannot be enumerated — falls
//!   back to every candidate, charged at the site itself.
//!
//! Every fallback is toward MORE edges, never fewer: a consumer that treats
//! an edge as a demand (coverage) or a refusal (the const-only check) can
//! over-ask through the fallback but never under-ask. Consumers own their
//! site-enumeration policy (which calls count as dispatch sites) and pass it
//! in as [`DispatchSite`]s; this module owns the resolution.

use crate::analyzer::{Expr, GenericDispatch, Program, SourceId};
use crate::call_graph::{CallGraph, CallTarget};
use crate::fx::{FxHashMap as HashMap, FxHashSet as HashSet};
use crate::id::Id;
use crate::type_::{SubstitutionContext, Type, TypeId};

/// Who makes the call a [`RefinedEdge`] charges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefinedCaller {
    /// A function or closure node of the shared graph.
    Node(Id),
    /// Top-level code, which owns no graph node: a top-level statement, or a
    /// module-level binding's (non-`const`) initializer.
    TopLevel,
}

/// One dispatch site a consumer wants refined.
pub struct DispatchSite {
    /// Whose body contains the site.
    pub owner: RefinedCaller,
    /// The dispatching call expression itself (a `for` loop's own id for an
    /// iterator-protocol edge).
    pub call: Id,
    /// Every callee the dispatched name can select among ([`candidates_of`]).
    pub candidates: Vec<Id>,
}

/// One refined edge: `caller`, at the call expression `anchor`, may invoke
/// `callee` (an impl member or a trait default) through a dispatch site the
/// graph records as indirect.
pub struct RefinedEdge {
    pub caller: RefinedCaller,
    /// The call expression the edge is charged at: the entry call whose
    /// recorded bindings resolved an `OnConstraint` site, or the dispatch
    /// site itself for `OnType` narrowing and every conservative fallback.
    pub anchor: Id,
    pub callee: Id,
}

thread_local! {
    /// How many impl SELECTIONS [`refined_edges`] has evaluated on this thread
    /// since [`reset_selection_count`] — the memo's instrument, and the only
    /// thing that can see it working. A selection is the expensive unit here
    /// (a scan of every implementation, each entry of which may recurse into
    /// another such scan), the memo changes no output whatsoever, and a
    /// timing assertion on a shared machine is not a test — so the count is
    /// what the pin reads, exactly as `call_graph::build_count` pins the
    /// one-graph-per-analysis invariant it could not otherwise observe.
    ///
    /// Thread-local for that same reason: an analysis is single-threaded, and
    /// plain `cargo test` runs analyses concurrently in one process.
    static SELECTION_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Total wall AND CPU spent inside [`refined_edges`] on this thread since
    /// [`reset_refine_time`] — the `dispatch-refine` bucket of the
    /// `VILAN_PHASE_TIMING` post-pass line (N43).
    ///
    /// This call is reached from TWO passes that the line names separately
    /// (`context::thread_contexts` and the const pass's `check_const_only`),
    /// so its cost used to be split across two buckets whose names described
    /// neither it nor them — reading the split cost the editor-perf lane a
    /// detour through a profiler. One accumulator, summed across both call
    /// sites, is what makes the shared constant readable from the line.
    /// Accumulated unconditionally, like the const pass's own sub-split: two
    /// clock reads per call are noise next to a program-wide scan.
    static REFINE_TIME: std::cell::Cell<crate::PhaseSpan> =
        const { std::cell::Cell::new(crate::PhaseSpan::ZERO) };
    /// Every site [`refined_edges`] could not resolve, with the candidate list
    /// it widened to — B279's fence, given a face that is not a diagnostic
    /// (B355).
    ///
    /// The fence's soundness property is "every fallback widens to the WHOLE
    /// candidate list" (stated in this module's own documentation and
    /// load-bearing in `context.rs`'s dead-code exemption), and until now the
    /// only thing that could OBSERVE it was a coverage refusal — in a program
    /// that carries another diagnostic by construction, because a generic
    /// taken as a value is what makes the level unresolvable in the first
    /// place. That is why E189's broad gate could not be built: under it the
    /// property had no observable face at all. It has one here, on the same
    /// terms as [`SELECTION_COUNT`] — a thread-local the analysis fills and a
    /// pin reads, because the property is not a fact about any one `Program`
    /// field and an analysis is single-threaded while the suite is not.
    static FALLBACK_SITES: std::cell::RefCell<Vec<(Id, Vec<Id>)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The number of impl selections [`refined_edges`] has evaluated on this
/// thread since the last [`reset_selection_count`]. See [`SELECTION_COUNT`].
pub fn selection_count() -> usize {
    SELECTION_COUNT.with(std::cell::Cell::get)
}

/// Zeroes this thread's [`selection_count`].
pub fn reset_selection_count() {
    SELECTION_COUNT.with(|count| count.set(0));
}

/// The dispatch sites [`refined_edges`] could not resolve on this thread since
/// the last [`reset_dispatch_fallbacks`], each with the candidate list it
/// widened to: B279's fence, observable without a diagnostic (B355). One entry
/// per fallback, so a site that falls back at several levels appears several
/// times — the property being observed is "this site widened to all of these",
/// not how many times it did.
pub fn dispatch_fallbacks() -> Vec<(Id, Vec<Id>)> {
    FALLBACK_SITES.with(|sites| sites.borrow().clone())
}

/// Empties this thread's [`dispatch_fallbacks`].
pub fn reset_dispatch_fallbacks() {
    FALLBACK_SITES.with(|sites| sites.borrow_mut().clear());
}

/// How long this thread has spent inside [`refined_edges`] since the last
/// [`reset_refine_time`]. See [`REFINE_TIME`].
pub(crate) fn refine_time() -> crate::PhaseSpan {
    REFINE_TIME.with(std::cell::Cell::get)
}

/// Zeroes this thread's [`refine_time`] — called once per analysis, at the
/// top of `post_analysis_passes`, which is where both call sites live.
pub(crate) fn reset_refine_time() {
    REFINE_TIME.with(|time| time.set(crate::PhaseSpan::ZERO));
}

/// The trait-member name a call's dispatch record names, when the call also
/// has a `function_calls` entry. This is the `context` pass's historical site
/// gate — an iterator-protocol `for` loop records its dispatch on the loop id
/// with no `function_calls` entry, so this answers `None` for it where
/// [`crate::async_infer::dispatch_at`] alone would not; a consumer that wants
/// those sites too reads the record directly.
pub fn member_name_at<'src>(program: &Program<'src>, call_id: Id) -> Option<&'src str> {
    // A124 R3: a call through a trait OBJECT's table is a dispatch like a
    // bound's — any implementation of the member may answer it, so a hidden
    // context parameter one of them needs has to be threaded at the site.
    if let Some(name) = program.dyn_method_calls.get(&call_id) {
        return Some(name);
    }
    let subject_id = program.function_calls.get(&call_id)?.subject_id;
    for key in [call_id, subject_id] {
        match program.generic_dispatch.get(&key) {
            Some(GenericDispatch::OnConstraint(_, name))
            | Some(GenericDispatch::OnType(_, name)) => return Some(name),
            None => {}
        }
    }
    None
}

/// Every candidate a dispatch of `name` selects among: for each trait
/// declaring `name`, the trait's own default body (when it has one) plus
/// every implementation's override, across the traits declaring that name.
///
/// **KEYED BY THE MEMBER NAME, and by nothing else.** Not by the receiver,
/// not by the bound, not by the trait: two unrelated traits that both declare
/// `label` contribute to one list, and every implementation of either is in
/// it. The list is therefore an OVER-APPROXIMATION of what any one site can
/// select, and it is the source of the B254/B258 class — a name-keyed list
/// consumed as though it were receiver-specific. B279 is the sweep of its
/// consumers; each is named here with the DIRECTION it reads an edge in,
/// because that direction is what decides whether widening is safe:
///
/// - `context::analyze`'s dispatch sites — an edge is a DEMAND (the callee
///   may need the hidden value, so the caller must thread it). Widening adds
///   threading a callee ignores, which is sound. It is NOT sound for the
///   site's FLAVOR (`settle_strict` promotes a whole site off one strict
///   candidate) nor for COVERAGE, and both are narrowed: the flavor by
///   [`known_receiver_candidates`] at the site (B258), coverage by
///   [`refined_edges`] over the recorded instantiations. B279's structural
///   guard in that pass is what holds the two narrowings apart from the wide
///   list they came from.
/// - the const-only capability check (`const_eval`'s three site scans) — an
///   edge is a REFUSAL (a runtime path that reaches a const-only capability
///   is rejected). Widening REFUSES MORE, never less, so the name-keyed list
///   is safe there BY DIRECTION and is deliberately left unnarrowed: a
///   narrowing that dropped a real edge would let a `[const_only]` capability
///   ship in a runtime path, which is the failure this check exists to
///   prevent. The cost of the over-approximation is a refusal an author can
///   see and argue with; the cost of the under-approximation is silence.
///
/// A consumer that reads the candidate SET as a property of the site — "these
/// and no others" — must narrow first. There is no third direction.
pub fn candidates_of(program: &Program, file: Option<SourceId>, name: &str) -> Vec<Id> {
    // B318 S4: asked once for the FILE — the per-block test below runs inside
    // two nested loops over every trait and every implementation.
    let scope = file.filter(|file| program.impl_admission.restricts(*file));
    let mut candidates = Vec::new();
    for trait_ in program.traits.values() {
        let Some(&declaration_id) = trait_.declarations.get(name) else {
            continue;
        };
        // The trait's own default body, when it has one.
        if program
            .functions
            .get(&declaration_id)
            .is_some_and(|function| function.has_body)
        {
            candidates.push(declaration_id);
        }
        // Every implementation's override of this trait's member — every one
        // the asking FILE admits (B318 S4). An override in a block this file's
        // imports declined is not one of its candidates, which is the same
        // narrowing the rest of the module already applies by receiver and by
        // bound, applied by import instead.
        for implementation in &program.implementations {
            if implementation.trait_ids.contains(&trait_.id)
                && let Some(&member_id) = implementation.declarations.get(name)
                && scope.is_none_or(|file| {
                    program
                        .impl_admission
                        .admits_member(file, implementation, member_id)
                })
            {
                candidates.push(member_id);
            }
        }
    }
    candidates
}

/// The members a concrete subject type selects for `member`: a declared
/// member wins outright; else the trait defaults the matching impls inherit
/// (the `dispatch_candidates_for` shape, widened to primitive subjects —
/// `impl str with Slot` is real here).
///
/// KEYED BY (subject type, member name) — the narrowing twin of
/// [`candidates_of`]'s name-only key, and what a consumer that reads the set
/// as a property of the SITE has to go through (B279).
///
/// Matching is deliberately LOOSER than emission's
/// ([`crate::impl_select::select_member`]): the nominal head alone, plus every
/// impl whose subject pattern applies — which is what brings a blanket
/// `impl type T with Trait` into view, B158's body that these consumers could
/// not see at all. The extra impls only ever add members, which is the
/// direction this module's guarantee allows.
pub fn impl_members_for(
    program: &Program,
    file: Option<SourceId>,
    subject_type_id: TypeId,
    member: &str,
) -> Vec<Id> {
    impl_members_for_bound(program, file, subject_type_id, member, &[])
}

/// [`impl_members_for`], narrowed to the impls that provide one of `traits`.
///
/// KEYED BY (subject type, member name, bound traits).
///
/// A call through a BOUND can only reach an impl of the bound's own trait:
/// `v.bind(..)` under `V: MaybeSignal<str>` is answered by an impl of
/// `MaybeSignal`, never by some other trait that happens to declare a member
/// spelled `bind`. The candidate list this refines is name-keyed by design
/// (`candidates_of`), which is sound for the union edges and far too wide for
/// coverage: with two traits in a program declaring the same member name, every
/// call to either inherited the other's context reads. std shipping a blanket
/// `impl type T with MaybeSignal<T>` made that concrete — the subject matches
/// every type, so its `bind` was selected for every receiver in every program
/// that loaded `std::reactive`.
///
/// An empty `traits` keeps the unfiltered reading, and so does a filter that
/// selects nothing: this narrows where the language says it may, and widens
/// back wherever it cannot tell.
pub fn impl_members_for_bound(
    program: &Program,
    file: Option<SourceId>,
    subject_type_id: TypeId,
    member: &str,
    traits: &[Id],
) -> Vec<Id> {
    let Some(resolved) = program.type_id_to_type_map.get(&subject_type_id) else {
        return Vec::new();
    };
    // B318 S4: asked once for the FILE, not once per registered block (std
    // registers hundreds, and this loop runs per dispatch site).
    let scope = file.filter(|file| program.impl_admission.restricts(*file));
    let matches_subject = |subject: &Type| match (subject, resolved) {
        (Type::Struct(a, _), Type::Struct(b, _)) | (Type::Enum(a, _), Type::Enum(b, _)) => a == b,
        (a, b) => a == b,
    };
    let mut matching: Vec<&crate::analyzer::Implementation> = program
        .implementations
        .iter()
        // B318 S4: the per-importer namespace, applied BEFORE the subject test
        // — the cheap filter first, and the one that says whether this file may
        // see the block at all.
        .filter(|implementation| {
            scope.is_none_or(|file| program.impl_admission.admits_impl(file, implementation))
        })
        .filter(|implementation| {
            program
                .type_id_to_type_map
                .get(&implementation.subject)
                .is_some_and(matches_subject)
                || crate::impl_select::subject_applies(
                    program,
                    implementation.subject,
                    subject_type_id,
                )
        })
        .collect();
    if !traits.is_empty() {
        let narrowed: Vec<&crate::analyzer::Implementation> = matching
            .iter()
            .copied()
            .filter(|implementation| {
                implementation
                    .trait_ids
                    .iter()
                    .any(|trait_id| traits.contains(trait_id))
            })
            .collect();
        if !narrowed.is_empty() {
            matching = narrowed;
        }
    }
    let declared: Vec<Id> = matching
        .iter()
        .filter_map(|implementation| implementation.declarations.get(member).copied())
        .collect();
    if !declared.is_empty() {
        return declared;
    }
    matching
        .iter()
        .flat_map(|implementation| implementation.trait_ids.iter())
        .filter_map(|trait_id| {
            program
                .traits
                .get(trait_id)
                .and_then(|trait_| trait_.declarations.get(member).copied())
        })
        .collect()
}

/// The candidates an `OnType` site with a KNOWN receiver can actually select
/// among — the members the receiver's HEAD selects — KEYED BY (receiver type,
/// member name) — or `None` when nothing narrows the site: an `OnConstraint` or unrecorded dispatch, a receiver-less
/// `OnType` (a `self` call inside a shared trait default body), a receiver
/// that resolves to a generic or opaque type, or an empty selection. Every
/// `None` means "keep the union", so this only ever narrows where the
/// language says it may.
///
/// [`candidates_of`] is NAME-keyed and therefore program-wide: every override
/// of every trait declaring the name, whatever the receiver. That
/// over-approximation is sound for a consumer that reads an edge as a DEMAND
/// (coverage asks for more; the const-only check refuses more) and unsound for
/// one that reads the candidate SET as a property of the site — the `context`
/// pass's flavor promotion is the case B258 found: one strict candidate
/// promotes the whole site, so `RemoteSource`'s strict override of
/// `Source::map` rewrote `self.cache.map(..)`, a call on a `SignalCell` field
/// that inherits the OWNER-OPTIONAL default, into a bare-owner hand-off with
/// no owner to hand.
pub fn known_receiver_candidates(program: &Program, call_id: Id) -> Option<Vec<Id>> {
    let Some(GenericDispatch::OnType(Some(receiver), member)) =
        crate::async_infer::dispatch_at(program, call_id)
    else {
        return None;
    };
    let resolved = program.type_id_to_type_map.get(&receiver)?;
    if !crate::impl_select::is_resolvable(resolved) {
        return None;
    }
    // B318 S4: the file the CALL is in, which is where its method namespace is
    // written (`analyzer.rs`'s invariant: "`call_id` is caller-side always").
    // `admitting_file` costs one `is_empty` in a program that restricts
    // nothing, which is the whole estate.
    let selected = impl_members_for(program, program.admitting_file(call_id), receiver, member);
    (!selected.is_empty()).then_some(selected)
}

/// The traits a generic parameter's constraint names, transitively through
/// supertraits — the impls a call through that parameter may reach.
///
/// The constraint id IS the first bound's type id (`register_binder`), and any
/// further bounds hang off `generic_bounds` at the same id. An unbounded
/// parameter yields nothing, which the caller reads as "do not narrow".
fn bound_traits(program: &Program, constraint: TypeId) -> Vec<Id> {
    let mut pending: Vec<TypeId> = vec![constraint];
    pending.extend(
        program
            .generic_bounds
            .get(&constraint)
            .into_iter()
            .flatten()
            .copied(),
    );
    let mut traits: Vec<Id> = Vec::new();
    while let Some(type_id) = pending.pop() {
        let Some(Type::Trait(trait_id, _)) = program.type_id_to_type_map.get(&type_id) else {
            continue;
        };
        if traits.contains(trait_id) {
            continue;
        }
        traits.push(*trait_id);
        if let Some(trait_) = program.traits.get(trait_id) {
            pending.extend(trait_.supertraits.iter().copied());
        }
    }
    traits
}

/// How chasing a constraint through one call's recorded bindings ended.
enum Resolution {
    Concrete(TypeId),
    Parameter(TypeId),
    Opaque,
}

/// Chase a constraint through one call's recorded bindings —
/// `method_call_substitution` is the single channel every instantiation
/// shape records into, explicit generic arguments included.
fn resolve_through(
    program: &Program,
    bindings: Option<&SubstitutionContext>,
    constraint: TypeId,
) -> Resolution {
    let Some(bindings) = bindings else {
        return Resolution::Opaque;
    };
    let Some(mut resolved) = bindings.get(&constraint).copied() else {
        return Resolution::Opaque;
    };
    for _ in 0..16 {
        match program.type_id_to_type_map.get(&resolved) {
            Some(Type::Generic(inner)) => match bindings.get(inner) {
                Some(bound) if *bound != resolved => resolved = *bound,
                _ => break,
            },
            _ => break,
        }
    }
    match program.type_id_to_type_map.get(&resolved) {
        Some(Type::Generic(inner)) => Resolution::Parameter(*inner),
        Some(Type::Any | Type::Unknown | Type::Unresolved | Type::Trait(..)) | None => {
            Resolution::Opaque
        }
        Some(_) => Resolution::Concrete(resolved),
    }
}

/// Refines `sites` into concrete edges. See the module documentation for the
/// per-site rules; the guarantees are (a) every edge's `callee` is one of the
/// site's candidates, and (b) fallbacks always widen to the whole candidate
/// list, so a consumer can miss nothing a candidate list covered.
///
/// The wall of every call lands in [`REFINE_TIME`], which the phase line's
/// `dispatch-refine` bucket reads back (N43). The measured region is the
/// whole call INCLUDING the empty-`sites` early return, so the bucket
/// answers "what did dispatch refinement cost this analysis" rather than
/// "what did the calls that did work cost".
pub fn refined_edges(
    program: &Program,
    graph: &CallGraph,
    sites: &[DispatchSite],
) -> Vec<RefinedEdge> {
    let started = crate::PhaseClock::now();
    let edges = refined_edges_timed(program, graph, sites);
    REFINE_TIME.with(|time| time.set(time.get() + started.elapsed()));
    edges
}

/// [`refined_edges`] itself; the public name is its timing wrapper.
fn refined_edges_timed(
    program: &Program,
    graph: &CallGraph,
    sites: &[DispatchSite],
) -> Vec<RefinedEdge> {
    // Most programs dispatch nothing; skip the program-wide scans then.
    if sites.is_empty() {
        return Vec::new();
    }
    // The functions whose entries cannot be enumerated: taken as a value
    // (called indirectly), or themselves reachable through dispatch.
    let call_subject_entities: HashSet<Id> = program
        .function_calls
        .values()
        .map(|call| call.subject_id)
        .collect();
    let value_taken: HashSet<Id> = program
        .entity_map
        .iter()
        .filter_map(|(entity_id, expr)| match expr {
            Expr::Local(target)
                if program.functions.contains_key(target)
                    && !call_subject_entities.contains(entity_id) =>
            {
                Some(*target)
            }
            _ => None,
        })
        .collect();
    let dispatch_reachable: HashSet<Id> = sites
        .iter()
        .flat_map(|site| site.candidates.iter().copied())
        .collect();

    // The nearest enclosing function of a graph node (identity for a
    // function; a closure hops its lexical parents).
    let enclosing_function = |node: Id| -> Option<Id> {
        let mut current = node;
        loop {
            if program.functions.contains_key(&current) {
                return Some(current);
            }
            current = graph.closure_parent_of(current)?;
        }
    };

    // Incoming direct calls per function, and top-level incoming calls.
    let owned_call_ids: HashSet<Id> = graph
        .nodes()
        .iter()
        .flat_map(|node| graph.calls_of(node.id()))
        .map(|call| call.call_id)
        .collect();
    let mut incoming_calls: HashMap<Id, Vec<(Id, Id)>> = HashMap::default();
    for node in graph.nodes() {
        for call in graph.calls_of(node.id()) {
            if let CallTarget::Function(target) = call.target {
                incoming_calls
                    .entry(target)
                    .or_default()
                    .push((node.id(), call.call_id));
            }
        }
    }
    let mut top_level_incoming: HashMap<Id, Vec<Id>> = HashMap::default();
    for (call_id, call) in &program.function_calls {
        if owned_call_ids.contains(call_id) {
            continue;
        }
        if let Some(Expr::Local(target)) = program.entity_map.get(&call.subject_id) {
            top_level_incoming
                .entry(*target)
                .or_default()
                .push(*call_id);
        }
    }

    let mut edges: Vec<RefinedEdge> = Vec::new();
    for site in sites {
        // B355: the fence's non-diagnostic face. Every path below that hands
        // back `site.candidates` INSTEAD of a resolution is a fallback, and
        // each records itself here — so the widening is observable without a
        // coverage refusal to read it through.
        let record_fallback = || {
            FALLBACK_SITES.with(|recorded| {
                recorded
                    .borrow_mut()
                    .push((site.call, site.candidates.clone()));
            });
        };
        let widened = || {
            record_fallback();
            site.candidates.clone()
        };
        let union_fallback = |edges: &mut Vec<RefinedEdge>| {
            record_fallback();
            for &candidate in &site.candidates {
                edges.push(RefinedEdge {
                    caller: site.owner,
                    anchor: site.call,
                    callee: candidate,
                });
            }
        };
        let (constraint, member) = match crate::async_infer::dispatch_at(program, site.call) {
            Some(GenericDispatch::OnConstraint(constraint, member)) => (constraint, member),
            Some(GenericDispatch::OnType(Some(_), _)) => {
                // A concrete-receiver re-dispatch (the Gap-E shape: an
                // inherited trait default). The receiver's HEAD cannot
                // change under substitution, and the head is what selects
                // among candidates, so the site narrows to the members the
                // head selects ([`known_receiver_candidates`]) — edges from
                // the site's owner, no entry enumeration. A receiver
                // resolving to a generic or opaque type keeps the union, as
                // does an empty selection.
                match known_receiver_candidates(program, site.call) {
                    Some(selected) => {
                        for candidate in selected {
                            edges.push(RefinedEdge {
                                caller: site.owner,
                                anchor: site.call,
                                callee: candidate,
                            });
                        }
                    }
                    None => union_fallback(&mut edges),
                }
                continue;
            }
            // `OnType(None, _)` — a `self` call inside a shared trait
            // default body — and unrecorded sites keep the union.
            _ => {
                union_fallback(&mut edges);
                continue;
            }
        };
        // The traits the bound names, with their supertraits: a call through
        // this constraint can only reach an impl of one of them, whatever else
        // in the program spells the member the same way.
        let constraint_traits = bound_traits(program, constraint);
        // Concrete resolution → the impl members the type selects; an
        // empty selection (defensive — the bound audit rejects no-impl
        // types) falls back to every candidate.
        //
        // Memoized on the RESOLVED TYPE, not the type id (M19/E106). The
        // selection scans every implementation, and each scan may recurse
        // through `impl_select::provides_trait` — itself a scan of every
        // implementation — so one call is O(impls²); the walk below asks once
        // per ENTRY of the dispatching function, and an entry count is a
        // program-size quantity. kolt's generated icon module made that
        // concrete: 17,895 selections per pass, run twice per analysis (the
        // context pass and the const-only check both refine the same sites),
        // for **32** distinct (type, member) answers — 2.5 s of a 5 s
        // keystroke. Keying on the id memoizes nothing (a program mints one
        // id per expression: 17,802 distinct ids for those 32 answers), and
        // keying on the type is exact: `impl_members_for_bound` reads the id
        // only through `type_id_to_type_map`, and every walk under it
        // (`subject_shape_matches`, `bind_subject`, `provides_trait`) recurses
        // through the argument ids the resolved `Type` itself carries — so two
        // ids resolving to equal `Type`s drive an identical walk.
        let mut selection_memo: HashMap<Type, Vec<Id>> = HashMap::default();
        let mut selected_for = |resolved: TypeId| -> Vec<Id> {
            // An id with no resolved type selects nothing and falls back, the
            // same answer `impl_members_for_bound`'s own guard gives.
            let Some(key) = program.type_id_to_type_map.get(&resolved) else {
                return widened();
            };
            if let Some(selected) = selection_memo.get(key) {
                return selected.clone();
            }
            SELECTION_COUNT.with(|count| count.set(count.get() + 1));
            let selected = impl_members_for_bound(
                program,
                program.admitting_file(site.call),
                resolved,
                member,
                &constraint_traits,
            );
            let selected = if selected.is_empty() {
                widened()
            } else {
                selected
            };
            selection_memo.insert(key.clone(), selected.clone());
            selected
        };
        let root = match site.owner {
            RefinedCaller::Node(owner) => enclosing_function(owner),
            // A top-level `OnConstraint` site has no enclosing generic
            // function to enumerate entries of (and should not exist —
            // top-level code binds no constraints).
            RefinedCaller::TopLevel => None,
        };
        let Some(root) = root else {
            union_fallback(&mut edges);
            continue;
        };
        let mut visited: HashSet<(Id, TypeId)> = HashSet::default();
        let mut walk: Vec<(Id, TypeId)> = vec![(root, constraint)];
        while let Some((function, constraint)) = walk.pop() {
            if !visited.insert((function, constraint)) {
                // A revisit re-derives identical edges — skipping is exact.
                continue;
            }
            if value_taken.contains(&function) || dispatch_reachable.contains(&function) {
                // This level's entries cannot be enumerated.
                union_fallback(&mut edges);
                continue;
            }
            for (caller, incoming_call) in incoming_calls.get(&function).into_iter().flatten() {
                let bindings = program.method_call_substitution.get(incoming_call);
                let selected = match resolve_through(program, bindings, constraint) {
                    Resolution::Concrete(resolved) => selected_for(resolved),
                    Resolution::Parameter(parameter) => match enclosing_function(*caller) {
                        Some(outer) => {
                            walk.push((outer, parameter));
                            continue;
                        }
                        None => widened(),
                    },
                    Resolution::Opaque => widened(),
                };
                for candidate in selected {
                    edges.push(RefinedEdge {
                        caller: RefinedCaller::Node(*caller),
                        anchor: *incoming_call,
                        callee: candidate,
                    });
                }
            }
            for incoming_call in top_level_incoming.get(&function).into_iter().flatten() {
                let bindings = program.method_call_substitution.get(incoming_call);
                let selected = match resolve_through(program, bindings, constraint) {
                    Resolution::Concrete(resolved) => selected_for(resolved),
                    // Top-level code has no generic parameters to recurse
                    // into — an unresolved binding marks every candidate.
                    _ => widened(),
                };
                for candidate in selected {
                    edges.push(RefinedEdge {
                        caller: RefinedCaller::TopLevel,
                        anchor: *incoming_call,
                        callee: candidate,
                    });
                }
            }
        }
    }
    edges
}
