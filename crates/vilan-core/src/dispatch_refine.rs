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
//!   head);
//! - an `OnConstraint` site is resolved per ENTRY of the function owning the
//!   constraint: an entry call whose recorded bindings ground the constraint
//!   draws edges from THAT caller to only the impl members the concrete type
//!   selects; a binding leading to another generic parameter recurses to the
//!   entry's own enclosing function, so a forwarding wrapper resolves per
//!   call site; anything unresolvable — an opaque binding, a level whose
//!   entries cannot be enumerated — falls back to every candidate, charged
//!   at the site itself;
//! - a receiver-less `OnType` site — a `self` call inside a shared trait
//!   default body — is resolved the same way, per entry of the DEFAULT, with
//!   the default's `Self` as what each entry grounds (B742): a concrete
//!   receiver's type, a bounded receiver's parameter (recursing as a binding
//!   does), another default's `Self` (recursing to that default), or `self`
//!   handed on as a bounded argument. An entry with no receiver on record —
//!   a call through a `dyn` object's table — falls back as an opaque binding
//!   does. A walk that leaves a default through a dispatch keeps only the
//!   types for which that dispatch selects the default ([`Crossing`]).
//!
//! An ENTRY of a function is any call that can run it with bindings on
//! record: a direct call, and equally a DISPATCH site that can select it — a
//! trait default method reached through `row.gap(..)` is entered at that
//! site, whose recorded substitution binds the default's own parameters just
//! as a direct call's does ([`DispatchEntries`]). A level's entries cannot be
//! enumerated when the function is taken as a value, or when a dispatch the
//! consumer did NOT list as a site can select it.
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
use std::rc::Rc;

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
    /// The nodes the dispatched value is handed THROUGH between `caller` and
    /// the dispatch site, site outward: the site's owner and its lexical
    /// parents up to the function owning the constraint, then the same for
    /// each forwarding level an entry recursed through. Empty for an edge
    /// charged at the site itself (`caller` is then the site's owner).
    ///
    /// A per-entry edge says "this caller's instantiation selects `callee`",
    /// and for most readers that is the whole fact. A reader that follows a
    /// value the caller HANDS DOWN (the context pass's coverage) needs the
    /// path too: a node on it that receives the value from somewhere else —
    /// a `run` body, a function declaring the context — is where `callee`
    /// gets it, whatever `caller` holds.
    pub via: Rc<[Id]>,
    /// The calls the requirement crosses between the dispatch site and
    /// `anchor`, site outward: the site itself, then each entry call a
    /// forwarding level recursed through. Empty for an edge charged at the
    /// site itself (`anchor` is the site). A reader that reports WHERE a
    /// requirement flows (the context pass's walk-back, B741) reads the path
    /// from `caller` down to `callee` as these calls and `anchor`, in place
    /// of every call into the nodes on `via`: those include the calls that
    /// instantiate the generic body with another type, which select
    /// something else.
    pub through: Rc<[Id]>,
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
    /// How many selections [`impl_members_for_bound`] has COMPUTED on this
    /// thread since [`reset_bound_selections_computed`], as against served
    /// from its per-program memo (M97) — the count that shows the context
    /// pass and the const pass sharing one answer, which no output can.
    static BOUND_SELECTIONS_COMPUTED: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
    /// How many impl SUBJECT TESTS (`subject_applies`, a bound proof per
    /// blanket impl) [`impl_members_for_bound`]'s selections have run on this
    /// thread since [`reset_bound_selection_subject_tests`] (M103). The
    /// selection asks only the impls that can contribute to its answer, so
    /// an impl that neither declares the member, nor provides a trait that
    /// does, nor names one of the bound's traits is never tested — and this
    /// is the count that shows it, since the answer is the same either way.
    static BOUND_SELECTION_SUBJECT_TESTS: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
    /// How many per-importer ADMISSION tests (`ImplAdmission::admits_impl`)
    /// [`impl_members_for_bound`]'s selections have run on this thread since
    /// [`reset_bound_selection_admission_tests`] (M147). Asked after the
    /// member filter, so only of the impls that can contribute to the answer.
    static BOUND_SELECTION_ADMISSION_TESTS: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
}

/// The number of admission tests [`impl_members_for_bound`]'s selections have
/// run on this thread since the last [`reset_bound_selection_admission_tests`].
/// See [`BOUND_SELECTION_ADMISSION_TESTS`].
pub fn bound_selection_admission_tests() -> usize {
    BOUND_SELECTION_ADMISSION_TESTS.with(std::cell::Cell::get)
}

/// Zeroes this thread's [`bound_selection_admission_tests`].
pub fn reset_bound_selection_admission_tests() {
    BOUND_SELECTION_ADMISSION_TESTS.with(|count| count.set(0));
}

/// The number of impl subject tests [`impl_members_for_bound`]'s selections
/// have run on this thread since the last
/// [`reset_bound_selection_subject_tests`]. See
/// [`BOUND_SELECTION_SUBJECT_TESTS`].
pub fn bound_selection_subject_tests() -> usize {
    BOUND_SELECTION_SUBJECT_TESTS.with(std::cell::Cell::get)
}

/// Zeroes this thread's [`bound_selection_subject_tests`].
pub fn reset_bound_selection_subject_tests() {
    BOUND_SELECTION_SUBJECT_TESTS.with(|count| count.set(0));
}

/// The number of selections [`impl_members_for_bound`] computed rather than
/// served from its memo on this thread since the last
/// [`reset_bound_selections_computed`]. See [`BOUND_SELECTIONS_COMPUTED`].
pub fn bound_selections_computed() -> usize {
    BOUND_SELECTIONS_COMPUTED.with(std::cell::Cell::get)
}

/// Zeroes this thread's [`bound_selections_computed`].
pub fn reset_bound_selections_computed() {
    BOUND_SELECTIONS_COMPUTED.with(|count| count.set(0));
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

/// The impls a selection of `member` under the bound `traits` reads, as
/// indices into `program.implementations` in declaration order — the answer
/// of M203's index ([`MemberImpls`]), the face its pins compare against a
/// scan of every impl.
pub fn impls_a_selection_reads(program: &Program, member: &str, traits: &[Id]) -> Vec<usize> {
    program.member_impls().readable(member, traits).into_owned()
}

/// The impls that list `trait_id` in their `with` clause, as
/// [`impls_a_selection_reads`]' index answers it.
pub fn impls_implementing(program: &Program, trait_id: Id) -> Vec<usize> {
    program.member_impls().implementing(trait_id).to_vec()
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
        trait_member_candidates(program, scope, trait_.id, name, &mut candidates);
    }
    candidates
}

/// The members one trait's `name` can run as: the trait's own default body,
/// when it has one, and every implementation's override — the blankets over
/// a bounded generic (`impl type S: Source<type T> with Flow<T>`) among them,
/// since a blanket's `with` clause lists the trait like any other block's.
///
/// `scope` is the asking FILE when its imports restrict which blocks it sees
/// (B318 S4): an override in a block that file's imports declined is not one
/// of its candidates, which is the same narrowing the rest of the module
/// already applies by receiver and by bound, applied by import instead.
/// `None` admits every block.
fn trait_member_candidates(
    program: &Program,
    scope: Option<SourceId>,
    trait_id: Id,
    name: &str,
    candidates: &mut Vec<Id>,
) {
    let Some(&declaration_id) = program
        .traits
        .get(&trait_id)
        .and_then(|trait_| trait_.declarations.get(name))
    else {
        return;
    };
    if program
        .functions
        .get(&declaration_id)
        .is_some_and(|function| function.has_body)
    {
        candidates.push(declaration_id);
    }
    // M203: the impls that list the trait, from the index, in declaration
    // order — where every impl of the program was tested per call.
    for &index in program.member_impls().implementing(trait_id) {
        let implementation = &program.implementations[index];
        if let Some(&member_id) = implementation.declarations.get(name)
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

/// B617: every member a `dyn` object's table slot for `member` can run, keyed
/// by the DECLARATION the slot is typed by — `(declaration, union)` for each
/// member some call reaches through an object (`dyn_dispatched_members`)
/// whose declaration has no body of its own.
///
/// A call through an object names the trait's declaration, and the table's
/// slot is typed by it (its parameters, the context pass's hidden ones among
/// them), while what the slot runs is whichever implementation the erased
/// value selects — any of them, program-wide, since the object is built
/// wherever a value is erased and called wherever it travels. So the
/// declaration must carry every hidden context its union reads: a `Flow`
/// object's `start` slot runs the `Source` blanket's `start` for every source
/// erased into it, and that `start` reads `turn_scope` and `ambient_nursery`.
/// The union is the trait's whole member list ([`trait_member_candidates`]
/// unscoped), the blanket over a bounded generic included.
///
/// A declaration WITH a body is its default's own function: it has callers of
/// its own, and widening its parameters would ask each of them for a value it
/// never reads, so it keeps what its body reads (and B606's stand-in covers an
/// override that reads more).
pub fn object_slot_declaration_unions(program: &Program) -> Vec<(Id, Vec<Id>)> {
    let mut members: Vec<(Id, &str)> = program
        .dyn_dispatched_members
        .iter()
        .map(|(trait_id, member)| (*trait_id, *member))
        .collect();
    members.sort_unstable_by_key(|(trait_id, member)| (trait_id.0, *member));
    let mut unions: Vec<(Id, Vec<Id>)> = Vec::new();
    for (object_trait, member) in members {
        let Some((declaration, declaring_trait, _)) =
            crate::mono::object_member_declaration(program, object_trait, member)
        else {
            continue;
        };
        if unions.iter().any(|(seen, _)| *seen == declaration)
            || program
                .functions
                .get(&declaration)
                .is_none_or(|function| function.has_body)
        {
            continue;
        }
        let mut union = Vec::new();
        trait_member_candidates(program, None, declaring_trait, member, &mut union);
        if !union.is_empty() {
            unions.push((declaration, union));
        }
    }
    unions
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
///
/// Memoized on the `Program` (M97), keyed by everything the selection reads:
/// the admitting file, the RESOLVED type (not the id — see `refined_edges`'
/// per-site memo for why the type is exact), the member and the traits. The
/// selection is a scan of every implementation, each entry of which may
/// recurse into another, and its two big callers — the context pass and the
/// const pass, each refining its dispatch sites — asked the same questions:
/// on kolt's client ~5.9G instructions each, ~10% of a `check` apiece. The
/// program's impls, admission map and type slots are settled before the
/// post-passes begin and nothing after analysis writes them, so an answer
/// stays an answer for the life of the program.
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
    let key: BoundSelectionKey = (file, resolved.clone(), member.to_string(), traits.to_vec());
    if let Some(selected) = program.bound_selection_memo().get(&key) {
        return selected.clone();
    }
    BOUND_SELECTIONS_COMPUTED.with(|count| count.set(count.get() + 1));
    let selected =
        impl_members_for_bound_uncached(program, file, subject_type_id, resolved, member, traits);
    program.bound_selection_memo().insert(key, selected.clone());
    selected
}

/// The key [`impl_members_for_bound`]'s memo reads: (admitting file, resolved
/// subject type, member, bound traits).
pub(crate) type BoundSelectionKey = (Option<SourceId>, Type, String, Vec<Id>);

/// The impls a selection of `member` under `traits` can read (M203): an impl
/// that declares `member`, has a trait that declares it, or has one of
/// `traits` among its own — [`impl_members_for_bound`]'s first filter,
/// answered from two indices into `implementations` built once per program
/// instead of tested against every impl per selection.
#[derive(Debug)]
pub(crate) struct MemberImpls {
    /// Per member name, the impls that declare it or have a trait that does,
    /// in declaration order.
    /// (Owned names: a borrowed key would make `Program` invariant in its
    /// source lifetime, which it is not.)
    by_member: HashMap<Box<str>, Vec<usize>>,
    /// Per trait, the impls that list it in their `with` clause, in
    /// declaration order.
    by_trait: HashMap<Id, Vec<usize>>,
}

impl MemberImpls {
    pub(crate) fn build(program: &Program) -> Self {
        let mut by_member: HashMap<Box<str>, Vec<usize>> = HashMap::default();
        let mut by_trait: HashMap<Id, Vec<usize>> = HashMap::default();
        // Indices arrive in increasing order, so a list already holding this
        // impl ends with it.
        let push = |list: &mut Vec<usize>, index: usize| {
            if list.last() != Some(&index) {
                list.push(index);
            }
        };
        // The name is copied only the first time it is seen.
        let push_member =
            |by_member: &mut HashMap<Box<str>, Vec<usize>>, name: &str, index: usize| {
                match by_member.get_mut(name) {
                    Some(list) => push(list, index),
                    None => {
                        by_member.insert(Box::from(name), vec![index]);
                    }
                }
            };
        for (index, implementation) in program.implementations.iter().enumerate() {
            for name in implementation.declarations.keys() {
                push_member(&mut by_member, name, index);
            }
            for trait_id in &implementation.trait_ids {
                push(by_trait.entry(*trait_id).or_default(), index);
                if let Some(trait_) = program.traits.get(trait_id) {
                    for name in trait_.declarations.keys() {
                        push_member(&mut by_member, name, index);
                    }
                }
            }
        }
        Self {
            by_member,
            by_trait,
        }
    }

    /// The impls that list `trait_id` in their `with` clause, in declaration
    /// order.
    fn implementing(&self, trait_id: Id) -> &[usize] {
        self.by_trait.get(&trait_id).map_or(&[], Vec::as_slice)
    }

    /// The impls a selection of `member` under `traits` reads, in
    /// declaration order.
    fn readable(&self, member: &str, traits: &[Id]) -> std::borrow::Cow<'_, [usize]> {
        let by_member = self.by_member.get(member).map_or(&[][..], Vec::as_slice);
        let mut by_traits = traits
            .iter()
            .filter_map(|trait_id| self.by_trait.get(trait_id))
            .peekable();
        if by_traits.peek().is_none() {
            return std::borrow::Cow::Borrowed(by_member);
        }
        let mut readable: Vec<usize> = by_member.to_vec();
        for list in by_traits {
            readable.extend_from_slice(list);
        }
        readable.sort_unstable();
        readable.dedup();
        std::borrow::Cow::Owned(readable)
    }
}

/// [`impl_members_for_bound`]'s selection itself.
fn impl_members_for_bound_uncached(
    program: &Program,
    file: Option<SourceId>,
    subject_type_id: TypeId,
    resolved: &Type,
    member: &str,
    traits: &[Id],
) -> Vec<Id> {
    // B318 S4: asked once for the FILE, not once per registered block (std
    // registers hundreds, and this loop runs per dispatch site).
    let scope = file.filter(|file| program.impl_admission.restricts(*file));
    let matches_subject = |subject: &Type| match (subject, resolved) {
        (Type::Struct(a, _), Type::Struct(b, _)) | (Type::Enum(a, _), Type::Enum(b, _)) => a == b,
        (a, b) => a == b,
    };
    // M103: the impls this selection can READ, before the subject test that
    // costs a recursive bound proof per blanket impl. A matching impl
    // contributes to the answer in exactly two ways: a member it declares
    // under `member`'s name, or one its traits declare (the inherited defaults
    // below) — and, through `traits`, it decides whether the narrowing below
    // finds anything. An impl that does none of the three changes nothing
    // whether it matches or not, so it is never asked. The answer is the same
    // list in the same order; what moves is the count of `subject_applies`
    // calls, which grew with every blanket impl std added (maps-45's map and
    // set operators took a kolt `check` from 24.3G instructions to 29.1G, 7.0G
    // of it here).
    //
    // M203: and those impls come from an index rather than a scan. Testing
    // every impl's member and traits was most of an unmemoized selection
    // (a string hash per block, std's hundreds of blocks), and M203's
    // per-call-site dispatch entries ask for more selections than before.
    // The index answers the same predicate in declaration order.
    let readable = program.member_impls().readable(member, traits);
    let mut matching: Vec<&crate::analyzer::Implementation> = readable
        .iter()
        .map(|index| &program.implementations[*index])
        // B318 S4: the per-importer namespace, applied BEFORE the subject test,
        // which costs a bound proof — the one that says whether this file may
        // see the block at all. M147: after the member filter above, not
        // before it: both are pure predicates of the block, the member filter
        // is a map lookup that passes a handful of blocks per selection, and
        // the admission walk cost ~100 instructions for each of std's hundreds
        // (175,122 calls, 18 M instructions, on kolt's client check).
        .filter(|implementation| {
            scope.is_none_or(|file| {
                BOUND_SELECTION_ADMISSION_TESTS.with(|count| count.set(count.get() + 1));
                program.impl_admission.admits_impl(file, implementation)
            })
        })
        .filter(|implementation| {
            BOUND_SELECTION_SUBJECT_TESTS.with(|count| count.set(count.get() + 1));
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

/// B425: the candidates of a GENERIC-MEMBER dispatch (`OnConstraint`) —
/// [`candidates_of`] restricted to the traits the parameter's bounds name,
/// transitively through supertraits. A member of the same NAME on an
/// unrelated trait cannot be reached through the bound, and admitting it made
/// the context pass declare hidden parameters no call could supply. `None`
/// for any other site, and for an unbounded parameter (nothing to narrow by).
pub fn bound_candidates(program: &Program, call_id: Id) -> Option<Vec<Id>> {
    let Some(GenericDispatch::OnConstraint(constraint, member)) =
        crate::async_infer::dispatch_at(program, call_id)
    else {
        return None;
    };
    let traits = bound_traits(program, constraint);
    if traits.is_empty() {
        return None;
    }
    let scope = program
        .admitting_file(call_id)
        .filter(|file| program.impl_admission.restricts(*file));
    let mut candidates = Vec::new();
    for trait_id in &traits {
        trait_member_candidates(program, scope, *trait_id, member, &mut candidates);
    }
    Some(candidates)
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

/// What one level of the entry walk resolves at each entry of its function.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Subject {
    /// A generic parameter of the function, by its constraint id: what the
    /// entry's recorded bindings say it is.
    Constraint(TypeId),
    /// `Self` of the trait default the function is (B742): the type of the
    /// receiver the entry dispatches on. A `self` call inside a default body
    /// (`OnType(None, ..)`) dispatches on exactly that type, so the default's
    /// entries say what it selects, as a bounded call's bindings do.
    Receiver,
}

/// How chasing a level's subject through one entry ended.
enum Resolution {
    Concrete(TypeId),
    /// A generic parameter of the entry's caller, by constraint id: the
    /// answer is that caller's own entries'.
    Parameter(TypeId),
    /// `Self` of the trait (the id) whose default the entry's caller is — a
    /// `self` call, or `self` handed on as a bounded argument: the answer is
    /// that default's own entries', once the caller is confirmed to be one
    /// of that trait's defaults ([`is_default_of`]).
    CallerReceiver(Id),
    Opaque,
}

/// Whether `function` is one of `trait_id`'s own default bodies — the
/// function in which a value of the bare trait type `trait_id` is `Self`.
///
/// Inside a trait default body `self` (and anything typed `Self`) has the
/// bare trait type, and that is the only place a value of bare trait type
/// exists: a parameter written `x: Trait` is an implicit generic (B186), a
/// closure parameter so written is refused, a `let` takes its initializer's
/// concrete type (B161), and outside a default body a dispatch on one is
/// refused (`MethodLookup::BareTraitValue`). So a bare-`trait_id` value
/// inside `function` is that default's `Self` — one runtime type per entry.
fn is_default_of(program: &Program, function: Id, trait_id: Id) -> bool {
    program.traits.get(&trait_id).is_some_and(|trait_| {
        trait_
            .declarations
            .values()
            .any(|declaration| *declaration == function)
    })
}

/// The bare trait a receiver-less dispatch (`OnType(None, ..)`, a `self` call
/// inside a trait default body) dispatches on: its receiver's type, when it
/// is one. `None` for every other site, and for a receiver whose type the
/// record does not hold.
fn self_dispatch_trait(program: &Program, call: Id) -> Option<Id> {
    let Some(GenericDispatch::OnType(None, _)) = crate::async_infer::dispatch_at(program, call)
    else {
        return None;
    };
    let receiver = *program.function_calls.get(&call)?.argument_ids.first()?;
    let receiver_type = match program.entity_map.get(&receiver) {
        Some(Expr::Local(target)) => program
            .parameters
            .get(target)
            .map(|parameter| parameter.type_id)
            .or_else(|| {
                program
                    .variables
                    .get(target)
                    .map(|variable| variable.type_id)
            }),
        _ => None,
    }
    .or_else(|| program.expr_type_ids.get(&receiver).copied())?;
    match program.type_id_to_type_map.get(&receiver_type) {
        Some(Type::Trait(trait_id, _)) => Some(*trait_id),
        _ => None,
    }
}

/// What the receiver of one ENTRY of a trait default is — the default's
/// `Self` under that entry ([`Subject::Receiver`]), read off the entry's own
/// dispatch record. A top-level `Plain {}.announce()` is entered directly
/// (its subject names the default) and carries the same record as one in a
/// function; an entry with no dispatch record — a call through a `dyn`
/// object's table, whose receiver is erased — says nothing and answers
/// `Opaque`.
fn resolve_receiver(program: &Program, entry_call: Id) -> Resolution {
    match crate::async_infer::dispatch_at(program, entry_call) {
        // A concrete receiver: `Plain {}.announce()`.
        Some(GenericDispatch::OnType(Some(receiver), _)) => {
            match program.type_id_to_type_map.get(&receiver) {
                Some(Type::Generic(inner)) => Resolution::Parameter(*inner),
                Some(resolved) if crate::impl_select::is_resolvable(resolved) => {
                    Resolution::Concrete(receiver)
                }
                _ => Resolution::Opaque,
            }
        }
        // A bounded receiver: `item.announce()` under `T: Note` — the
        // receiver IS a `T`, so the answer is what `T` is at the caller's
        // entries.
        Some(GenericDispatch::OnConstraint(constraint, _)) => Resolution::Parameter(constraint),
        // Another default's `self` call: that default's `Self`.
        Some(GenericDispatch::OnType(None, _)) => match self_dispatch_trait(program, entry_call) {
            Some(trait_id) => Resolution::CallerReceiver(trait_id),
            None => Resolution::Opaque,
        },
        None => Resolution::Opaque,
    }
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
        // A binding to a bare trait is `Self` of the caller when the caller
        // is one of that trait's defaults (`announced(self)` inside one) —
        // the walk confirms it — and opaque anywhere else.
        Some(Type::Trait(trait_id, _)) => Resolution::CallerReceiver(*trait_id),
        Some(Type::Any | Type::Unknown | Type::Unresolved) | None => Resolution::Opaque,
        Some(_) => Resolution::Concrete(resolved),
    }
}

/// What one entry says a level's subject is.
fn resolve_entry(program: &Program, subject: Subject, entry_call: Id) -> Resolution {
    match subject {
        Subject::Constraint(constraint) => resolve_through(
            program,
            program.method_call_substitution.get(&entry_call),
            constraint,
        ),
        Subject::Receiver => resolve_receiver(program, entry_call),
    }
}

/// The level a `Parameter` or `CallerReceiver` resolution recurses to: the
/// entry's caller's enclosing function, with the subject it holds there.
/// `None` when there is none to recurse to — top-level code (no parameters,
/// no `Self`), a closure no function encloses, or a bare-trait binding whose
/// caller is not that trait's default (then the trait type is not a `Self`
/// the record can follow).
fn outer_level(
    program: &Program,
    graph: &CallGraph,
    path: &mut Vec<Id>,
    caller: RefinedCaller,
    resolution: &Resolution,
) -> Option<(Id, Subject)> {
    let RefinedCaller::Node(node) = caller else {
        return None;
    };
    let outer = extend_hand_off_path(program, graph, path, node)?;
    match *resolution {
        Resolution::Parameter(parameter) => Some((outer, Subject::Constraint(parameter))),
        Resolution::CallerReceiver(trait_id) if is_default_of(program, outer, trait_id) => {
            Some((outer, Subject::Receiver))
        }
        _ => None,
    }
}

/// A dispatch an entry walk crossed on its way out of a trait default whose
/// `Self` it was resolving: `(the dispatching call, the function it entered)`.
///
/// When a [`Subject::Receiver`] level is entered through a DISPATCH, the
/// dispatch's receiver is that level's `Self`, so every type the walk
/// resolves further out is a receiver of that dispatch — and only a type for
/// which the dispatch selects the level's function actually runs it. `Quiet`
/// overriding `announce` never runs the default `announce` through
/// `self.announce()` in `herald`, so a `self.note()` inside the default is
/// not reached by `Quiet {}.herald()`, whatever `Quiet::note` reads. The
/// recursion carries the same type outward (a `self` call's `Self`, a bounded
/// receiver's parameter, a forwarded parameter), so a crossing stays a fact
/// about every type the walk resolves past it.
type Crossing = (Id, Id);

/// One level of the entry walk: the function whose entries are read, what
/// they resolve, the hand-off path that reached it ([`RefinedEdge::via`]),
/// and the dispatches crossed on the way ([`Crossing`]).
type Level = (Id, Subject, Rc<[Id]>, Rc<[Crossing]>);

/// `crossed`, plus the dispatch `entry_call` into `function` when that
/// dispatch's receiver IS the level's subject — a dispatch entry of a
/// `Self` level. Any other entry hands the same set on.
fn crossing(
    crossed: &Rc<[Crossing]>,
    subject: Subject,
    through_dispatch: bool,
    entry_call: Id,
    function: Id,
) -> Rc<[Crossing]> {
    if subject != Subject::Receiver || !through_dispatch {
        return crossed.clone();
    }
    crossed
        .iter()
        .copied()
        .chain(std::iter::once((entry_call, function)))
        .collect()
}

/// Whether `resolved`, as the receiver of every crossed dispatch, selects the
/// function each one entered. A selection that answers nothing, or a record
/// it cannot read, keeps the type (the conservative direction): this only
/// drops a type the record says runs something else.
fn crosses_all(program: &Program, crossed: &[Crossing], resolved: TypeId) -> bool {
    crossed.iter().all(|&(call, entered)| {
        let Some((member, traits)) = selection_terms(program, call) else {
            return true;
        };
        let selected = impl_members_for_bound(
            program,
            program.admitting_file(call),
            resolved,
            member,
            &traits,
        );
        selected.is_empty() || selected.contains(&entered)
    })
}

/// The program-wide facts [`refined_edges`] reads at every site: which
/// functions are taken as values, and every function's incoming calls — from
/// graph nodes and from top-level code. A function of the program and the
/// graph it is asked with, never of the sites.
///
/// The context pass reads the same incoming calls for its own walk-back, so
/// it builds this once and hands it over ([`refined_edges_over`]) where the
/// refinement used to rebuild it beside the pass's own copy (M203).
pub(crate) struct RefineIndex {
    /// The functions whose entries cannot be enumerated because they are
    /// taken as a value (called indirectly): every function some entity names
    /// outside the subject of a wired call ([`Self::value_taken`]).
    value_taken: HashSet<Id>,
    /// Incoming direct calls per function: (calling node, call).
    pub(crate) incoming_calls: HashMap<Id, Vec<(Id, Id)>>,
    /// Incoming calls per function from code no graph node owns.
    pub(crate) top_level_incoming: HashMap<Id, Vec<Id>>,
    /// Every call some graph node makes — what tells a top-level call from a
    /// node's.
    pub(crate) node_owned_calls: HashSet<Id>,
    /// Whether every call the graph lists is listed once, under one node. Then
    /// no call repeats in `incoming_calls` — and none of them is top-level,
    /// whose calls are each listed once by construction — so a level's direct
    /// and top-level entries are distinct calls.
    calls_owned_once: bool,
}

impl RefineIndex {
    fn build(program: &Program, graph: &CallGraph) -> Self {
        Self::with_value_taken(program, graph, Self::value_taken(program))
    }

    /// The functions some entity names as a VALUE: an `Expr::Local` naming
    /// a function that is not the subject of a call in `function_calls`.
    /// (The context pass answers it from its own scan of the entity map.)
    fn value_taken(program: &Program) -> HashSet<Id> {
        let call_subject_entities: HashSet<Id> = program
            .function_calls
            .values()
            .map(|call| call.subject_id)
            .collect();
        program
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
            .collect()
    }

    /// The index over `graph`, with `value_taken` as [`Self::value_taken`]
    /// answers it — a caller that scanned the entity map already passes its
    /// answer in.
    pub(crate) fn with_value_taken(
        program: &Program,
        graph: &CallGraph,
        value_taken: HashSet<Id>,
    ) -> Self {
        let mut node_owned_calls: HashSet<Id> = HashSet::default();
        let mut calls_owned_once = true;
        let mut incoming_calls: HashMap<Id, Vec<(Id, Id)>> = HashMap::default();
        for node in graph.nodes() {
            for call in graph.calls_of(node.id()) {
                calls_owned_once &= node_owned_calls.insert(call.call_id);
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
            if node_owned_calls.contains(call_id) {
                continue;
            }
            if let Some(Expr::Local(target)) = program.entity_map.get(&call.subject_id) {
                top_level_incoming
                    .entry(*target)
                    .or_default()
                    .push(*call_id);
            }
        }
        Self {
            value_taken,
            incoming_calls,
            top_level_incoming,
            node_owned_calls,
            calls_owned_once,
        }
    }
}

/// How dispatch can enter each function: the other half of a level's entries,
/// beside the direct calls [`RefineIndex`] holds.
///
/// A dispatch site that can select a function is an ENTRY of it exactly as a
/// direct call is — `Row { .. }.gap(Dp { .. })` runs `Attrs::gap`'s default
/// body with `V = Dp`, and `method_call_substitution` records that binding at
/// the site under the default's own parameter, the same channel a direct call
/// records into. So an `OnConstraint` site inside a trait default resolves
/// per call site too, where it used to charge every candidate to every call
/// ("dispatch-reachable, entries cannot be enumerated") — which refused a
/// static `MaybeSignal` argument for the owner only its reactive arm reads.
///
/// Built from the consumer's sites, because the consumer owns which calls
/// count: a dispatch it did not list (the const-only check drops const
/// subtrees; the context pass lists no `for` loop's `next` and no top-level
/// statement) is not an entry this module may charge an edge at. Such a
/// dispatch still RUNS what it selects, so everything it can select is
/// [`Self::unenumerable`] and keeps the whole-candidate fallback — with one
/// exception that is not a guess: a top-level call is already an entry of the
/// function its subject names (`RefineIndex::top_level_incoming`).
struct DispatchEntries {
    /// Per function, the listed sites that can select it, as indexes into
    /// the site list.
    enumerated: HashMap<Id, Vec<usize>>,
    /// The functions a dispatch outside the listed sites can select.
    unenumerable: HashSet<Id>,
}

impl DispatchEntries {
    fn build(program: &Program, sites: &[DispatchSite], index: &RefineIndex) -> Self {
        let mut enumerated: HashMap<Id, Vec<usize>> = HashMap::default();
        // The dispatch-record keys the listed sites answer for: a record is
        // keyed by the call or by its subject (`dispatch_at`).
        let mut accounted: HashSet<Id> = HashSet::default();
        for (site_index, site) in sites.iter().enumerate() {
            accounted.insert(site.call);
            if let Some(call) = program.function_calls.get(&site.call) {
                accounted.insert(call.subject_id);
            }
            // A known receiver's head selects among the candidates, so a site
            // it narrows enters only what it selects; any other site may
            // enter every candidate it lists.
            let selected = known_receiver_candidates(program, site.call);
            for &candidate in selected.as_deref().unwrap_or(&site.candidates) {
                enumerated.entry(candidate).or_default().push(site_index);
            }
        }

        // Every dispatch record outside the listed sites, as the call it
        // belongs to: itself (a method call, a `for` loop's `next`, an
        // operator) or the call its subject heads. A record whose own key the
        // sites account for is passed over first (M203), which leaves the
        // subject-keyed ones to map to their calls — almost none, so the
        // `function_calls` scan that maps them is usually skipped outright.
        let mut unlisted_calls: Vec<Id> = Vec::new();
        let mut unlisted_subjects: Vec<Id> = Vec::new();
        for &key in program
            .generic_dispatch
            .keys()
            .chain(program.dyn_method_calls.keys())
            .chain(program.concat_render_dispatch.keys())
        {
            if accounted.contains(&key) {
                continue;
            }
            if program.function_calls.contains_key(&key) {
                unlisted_calls.push(key);
            } else {
                unlisted_subjects.push(key);
            }
        }
        if !unlisted_subjects.is_empty() {
            // The LAST call with each subject, as a map collected over every
            // call answers.
            let subjects: HashSet<Id> = unlisted_subjects.iter().copied().collect();
            let mut call_of_subject: HashMap<Id, Id> = HashMap::default();
            for (call_id, call) in &program.function_calls {
                if subjects.contains(&call.subject_id) {
                    call_of_subject.insert(call.subject_id, *call_id);
                }
            }
            unlisted_calls.extend(
                unlisted_subjects
                    .iter()
                    .map(|subject| call_of_subject.get(subject).copied().unwrap_or(*subject)),
            );
        }
        // `candidates_of` is O(traits x implementations) and a function of
        // (the asking file's scope, the name) alone.
        let mut name_candidates: HashMap<(Option<SourceId>, &str), Vec<Id>> = HashMap::default();
        let mut unenumerable: HashSet<Id> = HashSet::default();
        for call in unlisted_calls {
            if accounted.contains(&call) {
                continue;
            }
            let name = match program.dyn_method_calls.get(&call) {
                Some(name) => *name,
                None => match crate::async_infer::dispatch_at(program, call) {
                    Some(
                        GenericDispatch::OnConstraint(_, name) | GenericDispatch::OnType(_, name),
                    ) => name,
                    None => continue,
                },
            };
            let selected = known_receiver_candidates(program, call)
                .or_else(|| bound_candidates(program, call))
                .unwrap_or_else(|| {
                    let scope = program
                        .admitting_file(call)
                        .filter(|file| program.impl_admission.restricts(*file));
                    name_candidates
                        .entry((scope, name))
                        .or_insert_with(|| candidates_of(program, scope, name))
                        .clone()
                });
            // A top-level call is an entry of the function its subject names
            // already; it is unenumerable only for what else it can select.
            let entered_directly = program
                .function_calls
                .get(&call)
                .filter(|_| !index.node_owned_calls.contains(&call))
                .and_then(|call| match program.entity_map.get(&call.subject_id) {
                    Some(Expr::Local(target)) => Some(*target),
                    _ => None,
                });
            unenumerable.extend(
                selected
                    .into_iter()
                    .filter(|candidate| Some(*candidate) != entered_directly),
            );
        }
        Self {
            enumerated,
            unenumerable,
        }
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
    let edges = refined_edges_timed(program, graph, sites, None);
    REFINE_TIME.with(|time| time.set(time.get() + started.elapsed()));
    edges
}

/// [`refined_edges`] over an index the caller built for `program` and
/// `graph` — the same edges, without building the index again.
pub(crate) fn refined_edges_over(
    program: &Program,
    graph: &CallGraph,
    sites: &[DispatchSite],
    index: &RefineIndex,
) -> Vec<RefinedEdge> {
    let started = crate::PhaseClock::now();
    let edges = refined_edges_timed(program, graph, sites, Some(index));
    REFINE_TIME.with(|time| time.set(time.get() + started.elapsed()));
    edges
}

/// [`refined_edges`] itself; the public name is its timing wrapper.
fn refined_edges_timed(
    program: &Program,
    graph: &CallGraph,
    sites: &[DispatchSite],
    given: Option<&RefineIndex>,
) -> Vec<RefinedEdge> {
    // Most programs dispatch nothing; skip the program-wide scans then.
    if sites.is_empty() {
        return Vec::new();
    }
    let built;
    let index = match given {
        Some(index) => index,
        None => {
            built = RefineIndex::build(program, graph);
            &built
        }
    };
    let dispatch_entries = DispatchEntries::build(program, sites, index);
    let mut bound_site_selections: HashMap<usize, Option<HashSet<Id>>> = HashMap::default();

    let mut edges: Vec<RefinedEdge> = Vec::new();
    let site_level: Rc<[Id]> = Rc::from([]);
    let no_crossings: Rc<[Crossing]> = Rc::from([]);
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
                    via: site_level.clone(),
                    through: site_level.clone(),
                });
            }
        };
        if let Some(GenericDispatch::OnType(Some(_), _)) =
            crate::async_infer::dispatch_at(program, site.call)
        {
            // A concrete-receiver re-dispatch (the Gap-E shape: an inherited
            // trait default). The receiver's HEAD cannot change under
            // substitution, and the head is what selects among candidates, so
            // the site narrows to the members the head selects
            // ([`known_receiver_candidates`]) — edges from the site's owner, no
            // entry enumeration. A receiver resolving to a generic or opaque
            // type keeps the union, as does an empty selection.
            match known_receiver_candidates(program, site.call) {
                Some(selected) => {
                    for candidate in selected {
                        edges.push(RefinedEdge {
                            caller: site.owner,
                            anchor: site.call,
                            callee: candidate,
                            via: site_level.clone(),
                            through: site_level.clone(),
                        });
                    }
                }
                None => union_fallback(&mut edges),
            }
            continue;
        }
        // The walk's first level: the function owning the constraint (or the
        // default owning `Self`), with the hand-off path from the site's
        // owner up to it. Every other site — unrecorded, a top-level
        // `OnConstraint` site (top-level code binds no constraints), a
        // receiver-less dispatch whose receiver is not its default's `Self` —
        // keeps the union.
        let Some(WalkStart {
            root,
            path,
            subject,
            member,
            traits: constraint_traits,
        }) = walk_start(program, graph, site.owner, site.call)
        else {
            union_fallback(&mut edges);
            continue;
        };
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
        // A level is (function, subject) REACHED BY a hand-off path, past a
        // set of crossed dispatches ([`Level`]): two forwarding routes into
        // one function are two levels when the value crosses different nodes
        // on the way, because a reader of [`RefinedEdge::via`] may answer
        // them differently, or crosses different dispatches, which keep
        // different types. A revisit of the same level re-derives identical
        // edges — skipping is exact. The
        // path is a set in first-seen order (a recursive generic adds no node
        // twice), so the walk is finite; `MAX_LEVELS` bounds the one shape
        // that could still make it long, a deep diamond of forwarders each
        // crossing its own closure.
        let mut visited: HashSet<Level> = HashSet::default();
        // Beside each level, the calls crossed to reach it
        // ([`RefinedEdge::through`]). Not part of the level: a second route
        // to the same level draws the same edges, and the first route's calls
        // name one real path to them.
        let mut walk: Vec<(Level, Rc<[Id]>)> = vec![(
            (root, subject, Rc::from(path), no_crossings.clone()),
            Rc::from([site.call]),
        )];
        let mut fell_back_at_site = false;
        // Per-level scratch, reused across the walk's levels.
        let mut dispatched: Vec<(RefinedCaller, Id, bool)> = Vec::new();
        let mut entered_at: HashSet<Id> = HashSet::default();
        let mut site_fallback = |edges: &mut Vec<RefinedEdge>| {
            if !std::mem::replace(&mut fell_back_at_site, true) {
                union_fallback(edges);
            }
        };
        while let Some((level, through)) = walk.pop() {
            if visited.contains(&level) {
                continue;
            }
            if visited.len() >= MAX_LEVELS {
                site_fallback(&mut edges);
                break;
            }
            let (function, subject, via, crossed) = level.clone();
            visited.insert(level);
            if index.value_taken.contains(&function)
                || dispatch_entries.unenumerable.contains(&function)
            {
                // This level's entries cannot be enumerated.
                site_fallback(&mut edges);
                continue;
            }
            // Every entry of this level: the direct calls from graph nodes
            // and from top-level code, and the listed dispatch sites that can
            // select it. A consumer may list a top-level dispatch call that
            // is also a top-level incoming call; it is one entry.
            let direct = index
                .incoming_calls
                .get(&function)
                .into_iter()
                .flatten()
                .map(|(caller, call)| (RefinedCaller::Node(*caller), *call, false));
            let top_level = index
                .top_level_incoming
                .get(&function)
                .into_iter()
                .flatten()
                .map(|call| (RefinedCaller::TopLevel, *call, false));
            dispatched.clear();
            dispatched.extend(
                dispatch_entries
                    .enumerated
                    .get(&function)
                    .into_iter()
                    .flatten()
                    .filter(|site_index| {
                        bound_site_selections
                            .entry(**site_index)
                            .or_insert_with(|| {
                                bound_site_selection(
                                    program,
                                    graph,
                                    index,
                                    &dispatch_entries,
                                    sites,
                                    **site_index,
                                )
                            })
                            .as_ref()
                            .is_none_or(|selection| selection.contains(&function))
                    })
                    .map(|site_index| (sites[*site_index].owner, sites[*site_index].call, true)),
            );
            // A call is entered once. Direct and top-level entries cannot
            // repeat one when no call has two owners in the graph
            // (`RefineIndex::calls_owned_once`), so a level without dispatch
            // entries — nearly every level — skips the set (M203).
            let check_repeats = !dispatched.is_empty() || !index.calls_owned_once;
            entered_at.clear();
            for (caller, entry_call, through_dispatch) in
                direct.chain(top_level).chain(dispatched.iter().copied())
            {
                if check_repeats && !entered_at.insert(entry_call) {
                    continue;
                }
                let resolution = resolve_entry(program, subject, entry_call);
                let selected = match resolution {
                    // An instantiation one of the crossed dispatches does not
                    // send here never reaches this site.
                    Resolution::Concrete(resolved) if !crosses_all(program, &crossed, resolved) => {
                        continue;
                    }
                    Resolution::Concrete(resolved) => selected_for(resolved),
                    // A binding that is the caller's own parameter — or its
                    // own `Self`, a default's `self` call or `self` handed on —
                    // recurses to the caller's entries: a forwarding wrapper,
                    // one trait default calling another. Top-level code has
                    // no parameters and no `Self` to recurse into.
                    Resolution::Parameter(_) | Resolution::CallerReceiver(_) => {
                        let mut outer_path: Vec<Id> = via.to_vec();
                        match outer_level(program, graph, &mut outer_path, caller, &resolution) {
                            Some((outer, outer_subject)) => {
                                let crossed = crossing(
                                    &crossed,
                                    subject,
                                    through_dispatch,
                                    entry_call,
                                    function,
                                );
                                let through: Rc<[Id]> = through
                                    .iter()
                                    .copied()
                                    .chain(std::iter::once(entry_call))
                                    .collect();
                                walk.push((
                                    (outer, outer_subject, Rc::from(outer_path), crossed),
                                    through,
                                ));
                                continue;
                            }
                            // A bare-trait binding the record cannot follow
                            // is as opaque as it ever was.
                            None if matches!(resolution, Resolution::CallerReceiver(_))
                                && through_dispatch =>
                            {
                                site_fallback(&mut edges);
                                continue;
                            }
                            None => widened(),
                        }
                    }
                    // A dispatch entry is a site that MAY select this
                    // function, and its record binds the member it names —
                    // which is this function's own parameter only when this
                    // function is that declaration. With nothing on record
                    // the entry says nothing about this caller, so the site
                    // itself keeps every candidate, as an unenumerable level
                    // does; a direct call with an opaque binding does run
                    // this function, and is charged where it is made.
                    Resolution::Opaque if through_dispatch => {
                        site_fallback(&mut edges);
                        continue;
                    }
                    Resolution::Opaque => widened(),
                };
                for candidate in selected {
                    edges.push(RefinedEdge {
                        caller,
                        anchor: entry_call,
                        callee: candidate,
                        via: via.clone(),
                        through: through.clone(),
                    });
                }
            }
        }
    }
    edges
}

/// The most levels one site's entry walk visits before the site keeps every
/// candidate instead ([`refined_edges`]). Far above any real forwarding
/// depth; it exists so that a pathological diamond costs a fallback rather
/// than a hang.
const MAX_LEVELS: usize = 4096;

/// Appends the nodes a value handed down to `node` crosses on its way from
/// the nearest enclosing function — `node` and its lexical parents, the
/// function included — to `path`, skipping any already on it, and answers
/// that function. `None` (a closure no function encloses) leaves `path`
/// holding what was crossed.
fn extend_hand_off_path(
    program: &Program,
    graph: &CallGraph,
    path: &mut Vec<Id>,
    node: Id,
) -> Option<Id> {
    let mut current = node;
    loop {
        if !path.contains(&current) {
            path.push(current);
        }
        if program.functions.contains_key(&current) {
            return Some(current);
        }
        current = graph.closure_parent_of(current)?;
    }
}

/// The member a dispatch names and the traits a selection for it narrows
/// to: a bound's traits with their supertraits (a call through it reaches
/// only an impl of one of them); for a `self` call in a trait default, the
/// trait DECLARING the member (inside a default the call means the trait's
/// member, never an implementor's inherent one, B359); nothing to narrow by
/// for a concrete receiver. `None` with no dispatch on record.
fn selection_terms<'src>(program: &Program<'src>, call: Id) -> Option<(&'src str, Vec<Id>)> {
    Some(match crate::async_infer::dispatch_at(program, call)? {
        GenericDispatch::OnConstraint(constraint, member) => {
            (member, bound_traits(program, constraint))
        }
        GenericDispatch::OnType(None, member) => (
            member,
            program
                .bound_dispatch_traits
                .get(&call)
                .map(|(declaring, _)| vec![*declaring])
                .unwrap_or_default(),
        ),
        GenericDispatch::OnType(Some(_), member) => (member, Vec::new()),
    })
}

/// Where a site's entry walk starts: the function whose entries resolve the
/// site, the hand-off path from the site's owner up to it, what the walk
/// resolves there, and the member and traits a resolved type selects among.
struct WalkStart<'src> {
    root: Id,
    path: Vec<Id>,
    subject: Subject,
    member: &'src str,
    traits: Vec<Id>,
}

/// The [`WalkStart`] of a dispatch site the walk can resolve per entry, or
/// `None` for one it cannot:
///
/// - an `OnConstraint` site resolves its constraint at the entries of the
///   function owning it, narrowed to the traits the bound names (with their
///   supertraits): a call through it reaches only an impl of one of them;
/// - a receiver-less `OnType` site — `self.note()` inside a trait default —
///   resolves the default's `Self` at the default's entries (B742), narrowed
///   to the trait that declares the member: inside a default body the call
///   means the TRAIT's member, never an implementor's inherent one (B359).
///   Only when its receiver is that default's `Self` ([`is_default_of`]).
///
/// Top-level code binds no constraints and has no `Self`.
fn walk_start<'src>(
    program: &Program<'src>,
    graph: &CallGraph,
    owner: RefinedCaller,
    call: Id,
) -> Option<WalkStart<'src>> {
    let RefinedCaller::Node(owner) = owner else {
        return None;
    };
    let subject = match crate::async_infer::dispatch_at(program, call)? {
        GenericDispatch::OnConstraint(constraint, _) => Subject::Constraint(constraint),
        GenericDispatch::OnType(None, _) => Subject::Receiver,
        GenericDispatch::OnType(Some(_), _) => return None,
    };
    let (member, traits) = selection_terms(program, call)?;
    let mut path: Vec<Id> = Vec::new();
    let root = extend_hand_off_path(program, graph, &mut path, owner)?;
    if subject == Subject::Receiver
        && !self_dispatch_trait(program, call)
            .is_some_and(|trait_id| is_default_of(program, root, trait_id))
    {
        return None;
    }
    Some(WalkStart {
        root,
        path,
        subject,
        member,
        traits,
    })
}

/// What a listed `OnConstraint` site — or a `self` call in a trait default
/// (B742), whose `Self` resolves the same way — can select under ANY of its
/// enclosing function's entries, or `None` when nothing narrows it — the
/// test that keeps a bounded site from being counted as an entry of a member
/// no instantiation of it reaches.
///
/// `t.gap(cell)` under `T: Attrs` lists `Attrs::gap`'s default among its
/// candidates by name, and whether it runs that default is a fact about the
/// types `T` is instantiated with: if every one of them overrides `gap`, the
/// default is not entered here and its body's dispatch must not be charged to
/// this caller. The answer is the union over the same entry walk
/// [`refined_edges`] makes, taken toward MORE: an unenumerable level, an
/// opaque binding, an empty selection, or any site [`walk_start`] cannot
/// start from all answer `None`, and the dispatch entries it recurses through are not
/// themselves narrowed (which also keeps a recursive bound from asking for
/// its own answer).
fn bound_site_selection(
    program: &Program,
    graph: &CallGraph,
    index: &RefineIndex,
    dispatch_entries: &DispatchEntries,
    sites: &[DispatchSite],
    site_index: usize,
) -> Option<HashSet<Id>> {
    let site = &sites[site_index];
    let WalkStart {
        root,
        subject,
        member,
        traits: constraint_traits,
        ..
    } = walk_start(program, graph, site.owner, site.call)?;
    let types = instantiation_types(
        program,
        graph,
        index,
        dispatch_entries,
        sites,
        root,
        subject,
    )?;
    let file = program.admitting_file(site.call);
    let mut selection: HashSet<Id> = HashSet::default();
    for resolved in types {
        let selected = impl_members_for_bound(program, file, resolved, member, &constraint_traits);
        if selected.is_empty() {
            return None;
        }
        selection.extend(selected);
    }
    Some(selection)
}

/// The types `subject` is instantiated with across every entry of `root` —
/// forwarded through callers' own parameters and defaults' `Self`, and kept
/// only where every crossed dispatch selects what it entered, as
/// [`bound_site_selection`] reads them — one id per distinct RESOLVED type,
/// or `None` when some entry does not say (an unenumerable level, an opaque
/// binding, a forwarding level no function encloses, an id with no resolved
/// type).
///
/// M203: a bounded site in a generic std body is entered once per call of
/// that body, program-wide (genapp:46: 56,552 entries over its four
/// refinements), for a handful of distinct types, and each entry used to run
/// a selection — building [`impl_members_for_bound`]'s owned memo key (a
/// `Type` clone, the member, the traits) to read an answer it already had.
/// Keeping one id per resolved `Type` is exact for the reason `refined_edges`'
/// `selection_memo` is: [`impl_members_for_bound`] reads the id only through
/// its resolved type, and within one site the file, the member and the
/// traits are fixed. Walking first and selecting after also skips every
/// selection of a site the walk answers `None` for.
fn instantiation_types(
    program: &Program,
    graph: &CallGraph,
    index: &RefineIndex,
    dispatch_entries: &DispatchEntries,
    sites: &[DispatchSite],
    root: Id,
    subject: Subject,
) -> Option<Vec<TypeId>> {
    let mut types: Vec<TypeId> = Vec::new();
    let mut seen: HashSet<&Type> = HashSet::default();
    let mut visited: HashSet<(Id, Subject, Rc<[Crossing]>)> = HashSet::default();
    let mut walk: Vec<(Id, Subject, Rc<[Crossing]>)> = vec![(root, subject, Rc::from([]))];
    while let Some((function, subject, crossed)) = walk.pop() {
        if !visited.insert((function, subject, crossed.clone())) {
            continue;
        }
        if index.value_taken.contains(&function)
            || dispatch_entries.unenumerable.contains(&function)
        {
            return None;
        }
        let direct = index
            .incoming_calls
            .get(&function)
            .into_iter()
            .flatten()
            .map(|(caller, call)| (RefinedCaller::Node(*caller), *call, false));
        let top_level = index
            .top_level_incoming
            .get(&function)
            .into_iter()
            .flatten()
            .map(|call| (RefinedCaller::TopLevel, *call, false));
        let dispatched = dispatch_entries
            .enumerated
            .get(&function)
            .into_iter()
            .flatten()
            .map(|entry| (sites[*entry].owner, sites[*entry].call, true));
        for (caller, entry_call, through_dispatch) in direct.chain(top_level).chain(dispatched) {
            let resolution = resolve_entry(program, subject, entry_call);
            match resolution {
                Resolution::Concrete(resolved) if !crosses_all(program, &crossed, resolved) => {}
                Resolution::Concrete(resolved) => {
                    // An id with no resolved type selects nothing, which is
                    // `impl_members_for_bound`'s own answer for it.
                    if seen.insert(program.type_id_to_type_map.get(&resolved)?) {
                        types.push(resolved);
                    }
                }
                Resolution::Parameter(_) | Resolution::CallerReceiver(_) => {
                    let (outer, outer_subject) =
                        outer_level(program, graph, &mut Vec::new(), caller, &resolution)?;
                    let crossed =
                        crossing(&crossed, subject, through_dispatch, entry_call, function);
                    walk.push((outer, outer_subject, crossed));
                }
                Resolution::Opaque => return None,
            }
        }
    }
    Some(types)
}
