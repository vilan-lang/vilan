//! Platform coloring — function-granular platform admission
//! (proposal/platform-coloring.md, phase 1).
//!
//! Replaces import-site gating for application builds: a build may *load* any
//! module of any layer (they already load for typing), but every function
//! **reachable from the entry** must be runnable on the build platform. A
//! function's requirement is seeded by its definition site — an item defined
//! in a library layer's module requires that layer's platforms; base-layer
//! and user code are unconstrained — and the requirement travels by
//! reachability rather than a fixpoint:
//!
//! - Resolved calls descend into the callee.
//! - Trait/generic-bounded dispatch descends into every **candidate** (the
//!   impls' members and the trait default — `async_infer`'s rule; sound
//!   over-approximation, per-instantiation refinement recorded in the
//!   proposal).
//! - A call through a closure *value* descends nowhere: a closure's body was
//!   already charged to the function that **created** it (the v1 creator
//!   rule), which the walk reaches lexically via the closure-parent links.
//! - A **module-level binding** is reached by *reference*: its initializer
//!   runs iff something reachable references it (F6 — the same rule emission
//!   uses), so a reference is an edge, and the initializer's calls, created
//!   closures, and references to other bindings are the binding's out-edges.
//!   A `const`-marked initializer runs in the compile-time interpreter, not
//!   on the build platform — it has no edges and seeds nothing.
//!
//! A violation reports the call chain from the entry (backlog §E.8's
//! standard), anchored at the deepest call site in **user** code.
//!
//! [`requirements`] is the same reachability turned into tooling data: an
//! entry-independent per-function map of rendered requirement lines (what the
//! language server shows on hover), computed caller-ward from the seeds so
//! every function gets a shortest witness chain to the layer it requires.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

use crate::analyzer::{GenericDispatch, Program, SourceId};
use crate::call_graph::{Call, CallGraph, CallTarget, IndirectReason};
use crate::error::Error;
use crate::fx::{FxHashMap as HashMap, FxHashSet as HashSet};
use crate::id::Id;
use crate::manifest::Manifest;
use crate::span::Span;
use crate::target::Platform;
use crate::type_::{Type, TypeId};

/// Checks platform admission for everything reachable from the program's
/// entry (`main`), pushing chain-rendered diagnostics for violations. A
/// program with no user `main` (a library module, a fragment) has no entry
/// and nothing to admit — library boundaries are `check_library_contract`'s
/// job.
///
/// Reachability is **per instantiation** (§3.2): the walk threads each
/// call's recorded type bindings (`method_call_substitution` — the same
/// single channel monomorphization uses), so a trait/generic-bounded call
/// whose receiver is RESOLVED descends only into the member that
/// instantiation actually selects. `save_it(MemStore { .. })` no longer
/// charges `DiskStore`'s impl just because it exists. An unresolvable
/// binding falls back to every candidate — over-approximate but sound.
/// Takes the analysis tail's shared call graph rather than building its own
/// (E35): this pass writes nothing but diagnostics, so its view of the program
/// is bit-for-bit the one it used to build.
pub fn check(program: &mut Program, platform: Platform, graph: &CallGraph) {
    // Declared fences check on EVERY compile, entry or not — fencing library
    // code is their point (platform-coloring.md §3.7).
    let mut diagnostics = check_fences(program, graph);
    if let Some(entry) = entry_function(program) {
        let mut traversal = Traversal::new(program, graph, Some(platform));
        traversal.walk(entry, &SubstitutionContext::default(), None);
        diagnostics.extend(traversal.diagnostics);
    }
    // A teardown is a CONSEQUENCE: it runs because something was constructed.
    // When that construction is itself rejected for the same cause, the
    // teardown's diagnostic restates the construction's — `let file =
    // File::open(p)` in a browser build, where the rejected `File::open` sits
    // inside the very construction `File::drop`'s edge hangs from (E98). Judged
    // against the constructions only, so two teardowns never silence each other.
    let constructions: Vec<(SourceId, Span, String)> = diagnostics
        .iter()
        .filter(|violation| violation.covers.is_none())
        .map(|violation| {
            (
                violation.source,
                violation.error.span,
                violation.cause.clone(),
            )
        })
        .collect();
    diagnostics.retain(|violation| {
        let Some((source, construction)) = violation.covers else {
            return true;
        };
        !constructions.iter().any(|(rejected_in, rejected, cause)| {
            *rejected_in == source
                && *cause == violation.cause
                && rejected.start >= construction.start
                && rejected.end <= construction.end
        })
    });
    // Each violation goes in with the file its anchor span indexes into — the
    // chain crosses files, so the anchor is regularly in a module (backlog E16)
    // — and ONE per mistake (E98): the walk reaches a layer by as many edges as
    // the program has, and a fence over a FAMILY re-walks per host, so the same
    // site and the same cause arrive repeatedly. First wins — the walk's own
    // deterministic depth-first order; the kept chain is the first reached,
    // not necessarily the shortest.
    let mut seen: HashSet<(SourceId, Span, String)> = HashSet::default();
    for Violation {
        error,
        source,
        cause,
        covers: _,
    } in diagnostics
    {
        if !seen.insert((source, error.span, cause)) {
            continue;
        }
        program.push_diagnostic(error, source);
    }
}

/// The concrete host platforms the checker enumerates for a fence pattern —
/// the supported hosts (manifest layers use the same vocabulary).
fn known_hosts() -> [Platform; 4] {
    [
        Platform::Node {
            version: crate::target::NODE_LTS,
        },
        Platform::Deno {
            version: crate::target::DENO_CURRENT,
        },
        Platform::Bun {
            version: crate::target::BUN_CURRENT,
        },
        Platform::Browser,
    ]
}

/// Checks every `[platform("…")]` fence: for each concrete host matching a
/// declared pattern, everything reachable from the fenced function must admit
/// that host. Runs regardless of the build target and needs no entry —
/// violations land at the fence with the chain, not at some distant entry in
/// a dependent build. A fence on a generic function walks unbound
/// (dispatches consider every candidate): it promises for every possible
/// instantiation.
fn check_fences(program: &Program, graph: &CallGraph) -> Vec<Violation> {
    let mut diagnostics = Vec::new();
    for (id, function) in &program.functions {
        if function.platform_fence.is_empty() {
            continue;
        }
        let fence_label = function
            .platform_fence
            .iter()
            .map(|(pattern, _)| format!("\"{pattern}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let mut checked_platforms: Vec<Platform> = Vec::new();
        for (pattern_text, pattern_span) in &function.platform_fence {
            let Some(patterns) = crate::target::PlatformPattern::parse(pattern_text) else {
                // The pattern is written in the fenced function's own file.
                let msg = format!(
                    "unknown platform pattern `{pattern_text}` in `[platform(…)]` \
                     (expected `node`/`deno`/`bun`/`browser`, or a family like \
                     `@process`)"
                );
                diagnostics.push(Violation {
                    cause: msg.clone(),
                    error: Error {
                        trace: Vec::new(),
                        note: None,
                        span: *pattern_span,
                        msg,
                    },
                    source: program.diagnostic_source_of(*id),
                    covers: None,
                });
                continue;
            };
            for pattern in patterns {
                for host in known_hosts() {
                    if host.matches(pattern).is_some() && !checked_platforms.contains(&host) {
                        checked_platforms.push(host);
                    }
                }
            }
        }
        for host in checked_platforms {
            let mut traversal = Traversal::new(program, graph, Some(host));
            traversal.origin = Origin::Fence {
                function: function.name.to_string(),
                fence: fence_label.clone(),
            };
            traversal.walk(*id, &SubstitutionContext::default(), None);
            diagnostics.extend(traversal.diagnostics);
        }
    }
    diagnostics
}

/// What a violation chain hangs from: the build's entry, or a declared fence.
enum Origin {
    Entry,
    Fence { function: String, fence: String },
}

impl Origin {
    /// The origin's contribution to a violation's cause key — two fences are two
    /// promises and each wants its own diagnostic, but the entry is one.
    fn key(&self) -> String {
        match self {
            Origin::Entry => "entry".to_string(),
            Origin::Fence { function, fence } => format!("fence {function} {fence}"),
        }
    }
}

/// A coloring diagnostic together with its CAUSE — what, beyond the anchor,
/// makes two of them the same mistake. The anchor (file + span) says WHERE the
/// user must act; the cause says what is wrong there: the layer the chain
/// reaches, and what the chain hangs from.
///
/// The build platform is deliberately absent. A fence over a FAMILY is checked
/// against every host in it, so `[platform("@process")]` reaching browser-only
/// code used to draw three identical diagnostics differing only in which host
/// was named — one broken promise, reported three times (E98).
struct Violation {
    error: Error,
    source: SourceId,
    cause: String,
    /// Set when the chain crossed a synthetic teardown edge: the construction
    /// that teardown answers to. A teardown runs *because* something was
    /// constructed, so if the construction is itself rejected for the same
    /// cause the teardown's diagnostic is that rejection restated — the case
    /// behind E98, where `let file = File::open(p)` in a browser build drew the
    /// construction's error and `File::drop`'s beside it.
    covers: Option<(SourceId, Span)>,
}

/// The module-level bindings whose initializers run for a program entered at
/// `entry`, under the SAME per-instantiation reachability the admission walk
/// uses — emission and the async-initializer gate consume this, so
/// emitted ⊆ admitted holds by construction even under the refinement.
///
/// `extra_roots` covers a root the call graph cannot see: a split build's route
/// gate is selected by the EMITTER at a recognized `swap` call, so nothing in
/// source calls `View.swap_split` and the bindings its body reads (the pending
/// signal) would otherwise be shaken out from under it
/// (`bundle-splitting.md` §2).
pub(crate) fn reachable_bindings(
    program: &Program,
    graph: &CallGraph,
    entry: Id,
    extra_roots: &[Id],
) -> HashSet<Id> {
    let mut traversal = Traversal::new(program, graph, None);
    traversal.walk(entry, &SubstitutionContext::default(), None);
    for root in extra_roots {
        traversal.walk(*root, &SubstitutionContext::default(), None);
    }
    traversal.reached_bindings
}

/// E124's paint walk: every graph node — function, closure, module-level
/// binding — that the program's own `main` reaches, under the SAME
/// per-instantiation refinement admission and emission use, plus the two edge
/// kinds only the paint follows.
///
/// It differs from [`reachable_bindings`] in exactly two ways, and both are
/// `dead-code-paint.md`'s findings rather than preferences:
///
/// - it collects **nodes**, not only bindings — the paint's question is "does
///   any entry reach this declaration", and a `fun` is not a binding;
/// - it follows a `const` module binding's initializer edges, which
///   [`crate::call_graph::CallGraph`] deliberately drops for emission. A
///   function called only from `let x = const f();` is unreached and unemitted
///   and deleting it breaks the build (§1.6, probe P3).
///
/// `None` when the program has no `main` — which is most files in the editor,
/// where the OPEN file is the entry (§2.1, probe P5). That is the whole reason
/// the paint's per-entry sets are computed out of band rather than off the
/// analysis in hand.
///
/// Over-approximation is deliberate and always the safe direction here: a
/// missed gray is late, a false gray is a lie (§1.4, determination 2).
pub fn paint_reachable_nodes(program: &Program) -> Option<HashSet<Id>> {
    let entry = entry_function(program)?;
    let graph = program.call_graph();
    let mut traversal = Traversal::new(program, graph, None);
    traversal.collect_nodes = true;
    traversal.follow_const_initializers = true;
    traversal.walk(entry, &SubstitutionContext::default(), None);
    Some(traversal.reached_nodes)
}

/// A per-call type binding: the analyzer's constraint id → bound type id.
type SubstitutionContext = HashMap<TypeId, TypeId>;

/// How the walk arrived at a frame: the call site's span, the file that span
/// indexes into, and whether the site is user code (only user-code sites are
/// eligible anchors).
#[derive(Clone, Copy)]
struct Arrival {
    span: Span,
    source: SourceId,
    user_code: bool,
    /// Whether this edge is a synthetic teardown (destruction.md §8) rather
    /// than a call the user wrote — in which case `span` is the CONSTRUCTION
    /// the teardown answers to, and a violation beneath it is a consequence of
    /// that construction.
    teardown: bool,
}

/// The contextual DFS shared by admission (`platform` set: check + prune +
/// chain diagnostics) and binding reachability (`platform` empty: collect).
struct Traversal<'a, 'src> {
    program: &'a Program<'src>,
    graph: &'a CallGraph,
    platform: Option<Platform>,
    /// Nodes visited PER instantiation — keyed like `emit_instance`, by the
    /// resolved bindings — so the same generic function re-walks under a
    /// different `T` but recursion still terminates.
    visited: HashSet<(Id, Vec<(u32, u32)>)>,
    /// The walk stack: each frame's node with how it was ARRIVED at — the call
    /// site's span, the file that span indexes into, and whether it is user
    /// code. The file rides along with the span so a violation renders where it
    /// is anchored (backlog E16).
    trail: Vec<(Id, Option<Arrival>)>,
    diagnostics: Vec<Violation>,
    module_bindings: HashSet<Id>,
    reached_bindings: HashSet<Id>,
    /// E124: every node the walk reached, collected only when
    /// `collect_nodes` is set. Admission and emission ask "which BINDINGS
    /// run", which `reached_bindings` answers; the paint asks "which
    /// declarations does any entry reach", which is every node — so the set
    /// is opt-in rather than always built, and the admission walk pays
    /// nothing for it.
    reached_nodes: HashSet<Id>,
    collect_nodes: bool,
    /// E124: whether to follow the edges out of a `const` module binding's
    /// initializer (`CallGraph`'s paint-only maps). Off for emission and
    /// admission, whose answer about a const initializer — data, not code —
    /// is right; on for the paint, whose answer would otherwise gray a
    /// function whose deletion breaks the build (`dead-code-paint.md` §1.6).
    follow_const_initializers: bool,
    origin: Origin,
}

impl<'a, 'src> Traversal<'a, 'src> {
    fn new(program: &'a Program<'src>, graph: &'a CallGraph, platform: Option<Platform>) -> Self {
        Traversal {
            program,
            graph,
            platform,
            visited: HashSet::default(),
            trail: Vec::new(),
            diagnostics: Vec::new(),
            module_bindings: program.module_level_bindings().into_iter().collect(),
            reached_bindings: HashSet::default(),
            reached_nodes: HashSet::default(),
            collect_nodes: false,
            follow_const_initializers: false,
            origin: Origin::Entry,
        }
    }

    fn walk(&mut self, node: Id, substitution: &SubstitutionContext, arrived_by: Option<Arrival>) {
        let mut key: Vec<(u32, u32)> = substitution
            .iter()
            .map(|(constraint, bound)| (constraint.0, self.resolve_type_id(*bound, substitution).0))
            .collect();
        key.sort_unstable();
        if !self.visited.insert((node, key)) {
            return;
        }
        self.trail.push((node, arrived_by));

        if self.module_bindings.contains(&node) {
            self.reached_bindings.insert(node);
        }
        if self.collect_nodes {
            self.reached_nodes.insert(node);
        }

        if let Some(platform) = self.platform
            && let Some(requirement) = requirement_of(self.program, node)
        {
            let admitted = requirement
                .patterns
                .iter()
                .any(|pattern| platform.matches(*pattern).is_some());
            if !admitted {
                // Report the BOUNDARY — the first off-platform function
                // reached from admissible code — and do not descend:
                // everything beneath it lives in the same layer, and one
                // chain tells the story.
                let error = violation(
                    self.program,
                    platform,
                    &self.trail,
                    node,
                    requirement,
                    &self.origin,
                );
                self.diagnostics.push(error);
                self.trail.pop();
                return;
            }
        }

        let const_calls: &[Call] = if self.follow_const_initializers {
            self.graph.const_initializer_calls_of(node)
        } else {
            &[]
        };
        for call in self
            .graph
            .calls_of(node)
            .iter()
            .chain(self.graph.initializer_calls_of(node))
            .chain(const_calls)
        {
            let arrived = self.arrival(call.call_id);
            match call.target {
                CallTarget::Function(callee)
                | CallTarget::Closure(callee)
                | CallTarget::External(callee) => {
                    let next = self.callee_substitution(call.call_id, callee, substitution);
                    self.walk(callee, &next, arrived);
                }
                CallTarget::Variant(_) => {}
                CallTarget::Indirect(IndirectReason::Value) => {
                    // The creator rule: whoever created the closure was
                    // charged for its body; a call through the value adds
                    // nothing.
                }
                CallTarget::Indirect(_) => {
                    // THE refinement: a resolved receiver selects one impl's
                    // member; an unresolved one keeps every candidate.
                    let receiver = self.dispatch_receiver(call.call_id, substitution);
                    let candidates = crate::async_infer::dispatch_candidates_for(
                        self.program,
                        call.call_id,
                        receiver.as_ref(),
                    );
                    for candidate in candidates {
                        let next = self.callee_substitution(call.call_id, candidate, substitution);
                        self.walk(candidate, &next, arrived);
                    }
                }
            }
        }
        // Referencing a module-level binding runs its initializer (F6);
        // initializers are never generic, so they walk context-free.
        let const_globals: &[(Id, Id)] = if self.follow_const_initializers {
            self.graph.const_global_references_of(node)
        } else {
            &[]
        };
        for (reference, global) in self
            .graph
            .global_references_of(node)
            .iter()
            .chain(const_globals)
        {
            let arrived = self.arrival(*reference);
            self.walk(*global, &SubstitutionContext::default(), arrived);
        }
        // A function passed as a value charges at the reference site; with no
        // call record there is no binding to thread.
        let const_functions: &[(Id, Id)] = if self.follow_const_initializers {
            self.graph.const_function_references_of(node)
        } else {
            &[]
        };
        for (reference, function) in self
            .graph
            .function_references_of(node)
            .iter()
            .chain(const_functions)
        {
            let arrived = self.arrival(*reference);
            self.walk(*function, &SubstitutionContext::default(), arrived);
        }
        // Creating a closure charges its body (v1 creator rule); a closure
        // inherits its creator's bindings — its body uses the enclosing `T`s.
        // The copy is load-bearing, not a convenience: `closure_children_of`
        // hands back a slice borrowed from `self.graph`, and `walk` takes
        // `&mut self` — iterating the borrow directly does not compile.
        #[allow(clippy::unnecessary_to_owned, reason = "ends the graph borrow")]
        if let Some(children) = self.graph.closure_children_of(node) {
            for closure in children.to_vec() {
                self.walk(closure, &substitution.clone(), None);
            }
        }
        for closure in self.graph.initializer_closures_of(node).to_vec() {
            self.walk(closure, &SubstitutionContext::default(), None);
        }
        if self.follow_const_initializers {
            for closure in self.graph.const_initializer_closures_of(node).to_vec() {
                self.walk(closure, &SubstitutionContext::default(), None);
            }
        }
        // Synthetic destruction edges (destruction.md §8): the transformer inserts
        // the teardown at each scope exit, so this walk can't see the call
        // otherwise. Walking to the resource's `drop` impl(s) here colors the
        // owning scope by a `@process`-needing drop. Context-free (the drop impl's
        // platform requirement is on its own body, not the owner's `T`). The
        // arrival is the CONSTRUCTION the teardown answers to: the scope exit has
        // no spelling, and anchoring at the drop impl instead would point the user
        // into the library's own source (E98).
        if let Some(drop_methods) = self.program.drop_call_edges.get(&node) {
            for (drop_method, site) in drop_methods.clone() {
                let arrived = site.and_then(|site| self.arrival_by(site, true));
                self.walk(drop_method, &SubstitutionContext::default(), arrived);
            }
        }

        self.trail.pop();
    }

    fn arrival(&self, site: Id) -> Option<Arrival> {
        self.arrival_by(site, false)
    }

    fn arrival_by(&self, site: Id, teardown: bool) -> Option<Arrival> {
        let span = self.program.span_map.get(&site).map(|span| **span)?;
        Some(Arrival {
            span,
            source: self.program.diagnostic_source_of(site),
            user_code: is_user_code(self.program, site),
            teardown,
        })
    }

    /// The bindings a call hands its callee — the transformer's
    /// `call_substitution` channels, mirrored: the call's generic arguments
    /// zipped with the callee's parameters, else the recorded
    /// `method_call_substitution` entry; either way each bound type resolves
    /// under the CALLER's bindings so nested instantiations compose —
    /// exactly `emit_instance`'s rule. With neither channel, a callee that
    /// shares the caller's constraints inherits them (a nested call inside
    /// a generic body).
    fn callee_substitution(
        &self,
        call_id: Id,
        callee: Id,
        incoming: &SubstitutionContext,
    ) -> SubstitutionContext {
        if let Some(function) = self.program.functions.get(&callee)
            && !function.generic_parameter_constraint_ids.is_empty()
            && let Some(function_call) = self.program.function_calls.get(&call_id)
            && !function_call.generic_argument_ids.is_empty()
        {
            return function
                .generic_parameter_constraint_ids
                .iter()
                .copied()
                .zip(function_call.generic_argument_ids.iter().copied())
                .map(|(constraint, bound)| (constraint, self.resolve_type_id(bound, incoming)))
                .collect();
        }
        if let Some(recorded) = self.program.method_call_substitution.get(&call_id) {
            return recorded
                .iter()
                .map(|(constraint, bound)| (*constraint, self.resolve_type_id(*bound, incoming)))
                .collect();
        }
        // No record: pass the caller's bindings through — a call inside a
        // generic body resolves the shared constraints; unrelated keys are
        // inert (nothing looks them up).
        incoming.clone()
    }

    /// Follows `Generic` links through the active bindings (bounded, so a
    /// self-referential binding can't loop).
    fn resolve_type_id(&self, type_id: TypeId, substitution: &SubstitutionContext) -> TypeId {
        let mut current = type_id;
        for _ in 0..16 {
            match self.program.type_id_to_type_map.get(&current) {
                Some(Type::Generic(constraint)) => match substitution.get(constraint) {
                    Some(bound) if *bound != current => current = *bound,
                    _ => break,
                },
                _ => break,
            }
        }
        current
    }

    /// The concrete receiver a dispatch resolves to under the bindings, if
    /// the record + the substitution pin one down.
    fn dispatch_receiver(&self, call_id: Id, substitution: &SubstitutionContext) -> Option<Type> {
        let resolved = match crate::async_infer::dispatch_at(self.program, call_id)? {
            GenericDispatch::OnConstraint(constraint_id, _) => {
                self.resolve_type_id(*substitution.get(&constraint_id)?, substitution)
            }
            GenericDispatch::OnType(receiver, _) => self.resolve_type_id(receiver?, substitution),
        };
        match self.program.type_id_to_type_map.get(&resolved) {
            Some(concrete @ (Type::Struct(_, _) | Type::Enum(_, _))) => Some(concrete.clone()),
            _ => None,
        }
    }
}

/// [`CallGraph::successors`] — the shared edge vocabulary — with each site
/// expression resolved to the diagnostic's raw material: its span and whether
/// it lies in user code (`None` for a created closure's body).
fn edges(program: &Program, graph: &CallGraph, node: Id) -> Vec<(Id, Option<(Span, bool)>)> {
    graph
        .successors(program, node)
        .into_iter()
        .map(|(successor, site)| {
            let arrived = site.and_then(|site| {
                let span = program.span_map.get(&site).map(|span| **span)?;
                Some((span, is_user_code(program, site)))
            });
            (successor, arrived)
        })
        .collect()
}

/// Per-function platform requirements, rendered for tooling: every function,
/// closure, or extern that (transitively) requires a layer maps to a line
/// like
///
/// ```text
/// requires the `process` layer of `std` (via `load (server::store) → stat (std::fs)`)
/// ```
///
/// Unlike [`check`] this is **entry-independent** — a library function nobody
/// calls yet still knows its color, which is exactly what an editor hover
/// wants. Requirements propagate caller-ward from the definition-site seeds
/// (one multi-source BFS per layer label over the same [`edges`] the
/// admission walk uses), and each reached node records the callee it acquired
/// the label through, so following those witnesses callee-ward yields a
/// *shortest* via-chain down to the layer. A seeded node's own line carries
/// no chain. Multiple layers render one line each, in label order.
///
/// Reads the analysis tail's shared call graph (E35). The LSP calls this after
/// `analyze_source` has returned, i.e. after the post-passes installed it, so
/// this is the same build the admission walk used — and this pass writes
/// nothing at all, taking `&Program`.
pub fn requirements(program: &Program) -> HashMap<Id, String> {
    let graph = program.call_graph();

    // The node universe: every code-bearing node, every extern (a leaf that
    // can seed a requirement), and every module-level binding (whose
    // initializer both seeds and propagates), in deterministic build order.
    let mut universe: Vec<Id> = graph.nodes().iter().map(|node| node.id()).collect();
    universe.extend(program.external_functions.keys().copied());
    universe.extend(program.module_level_bindings());

    let mut callers: HashMap<Id, Vec<Id>> = HashMap::default();
    for id in &universe {
        for (callee, _) in edges(program, graph, *id) {
            callers.entry(callee).or_default().push(*id);
        }
    }

    let mut seeds: BTreeMap<&str, Vec<Id>> = BTreeMap::new();
    for id in &universe {
        if let Some(requirement) = requirement_of(program, *id) {
            seeds.entry(requirement.label).or_default().push(*id);
        }
    }

    let mut lines: HashMap<Id, Vec<String>> = HashMap::default();
    for (label, sources) in &seeds {
        // node → the callee it acquired this label from (`None` = seeded).
        let mut witness: HashMap<Id, Option<Id>> = HashMap::default();
        let mut queue: VecDeque<Id> = VecDeque::new();
        for source in sources {
            witness.insert(*source, None);
            queue.push_back(*source);
        }
        while let Some(node) = queue.pop_front() {
            let Some(callers_of_node) = callers.get(&node) else {
                continue;
            };
            for caller in callers_of_node {
                if !witness.contains_key(caller) {
                    witness.insert(*caller, Some(node));
                    queue.push_back(*caller);
                }
            }
        }
        for id in &universe {
            let Some(acquired_through) = witness.get(id) else {
                continue;
            };
            let mut chain = Vec::new();
            let mut cursor = *acquired_through;
            while let Some(next) = cursor {
                chain.push(frame_label(program, next));
                cursor = witness.get(&next).copied().flatten();
            }
            let line = if chain.is_empty() {
                format!("requires {label}")
            } else {
                format!("requires {label} (via `{}`)", chain.join(" → "))
            };
            lines.entry(*id).or_default().push(line);
        }
    }
    lines
        .into_iter()
        .map(|(id, lines)| (id, lines.join("\n")))
        .collect()
}

struct Requirement<'program> {
    label: &'program str,
    patterns: &'program [crate::target::PlatformPattern],
}

/// Whether `id` is a binding whose initializer is `const`-marked: evaluated
/// by the compile-time interpreter and serialized as a value, so at runtime
/// it is data — it runs nothing and requires nothing of the build platform.
fn is_const_global(program: &Program, id: Id) -> bool {
    program
        .variables
        .get(&id)
        .and_then(|variable| variable.initial)
        .is_some_and(|initial| program.const_exprs.contains(&initial))
}

/// The platform requirement seeded by `node`'s definition site: the layer
/// whose root contains its source file, if any. Base-layer and user files
/// (empty-pattern entries or no entry) seed nothing; a `const` binding is
/// compile-time data and seeds nothing wherever it is defined.
fn requirement_of<'program>(program: &'program Program, node: Id) -> Option<Requirement<'program>> {
    if is_const_global(program, node) {
        return None;
    }
    let source = program.source_of(node)?;
    // Canonicalized on both sides — `layer_platforms`' roots and
    // `canonical_sources` alike (`windows-support.md` §5).
    let path = program.canonical_sources.get(source.0 as usize)?;
    for (root, _library, label, patterns) in &program.layer_platforms {
        if !patterns.is_empty() && path.starts_with(root) {
            return Some(Requirement { label, patterns });
        }
    }
    None
}

/// A frame's display name: bare for user code, `name (lib::module)` for
/// library code — the chain then reads `main → boot (server::store) →
/// stat (std::fs)`.
fn frame_label(program: &Program, id: Id) -> String {
    let name = name_of(program, id);
    if is_user_code(program, id) {
        return name;
    }
    let module = program
        .source_of(id)
        .and_then(|source| {
            // The STEM comes from the spelling the user gave; the containment
            // test from the canonical form (`windows-support.md` §5).
            let path = program.sources.get(source.0 as usize)?;
            let canonical = program.canonical_sources.get(source.0 as usize)?;
            Some((path, canonical))
        })
        .and_then(|(path, canonical)| {
            let stem = path.file_stem()?.to_string_lossy().into_owned();
            let library = program
                .layer_platforms
                .iter()
                .find(|(root, _, _, _)| canonical.starts_with(root))
                .map(|(_, library, _, _)| library.clone())?;
            Some(if stem == "lib" {
                library
            } else {
                format!("{library}::{stem}")
            })
        });
    match module {
        Some(module) => format!("{name} ({module})"),
        None => name,
    }
}

/// Whether the entity's file is the user's own code — not under any recorded
/// library root (layers or bases).
fn is_user_code(program: &Program, id: Id) -> bool {
    let Some(source) = program.source_of(id) else {
        return false;
    };
    // Canonicalized on both sides (`windows-support.md` §5) — a mismatch here
    // silently reclassifies library code as the user's own.
    let Some(path) = program.canonical_sources.get(source.0 as usize) else {
        return false;
    };
    !program
        .layer_platforms
        .iter()
        .any(|(root, _, _, _)| path.starts_with(root))
}

fn violation(
    program: &Program,
    platform: Platform,
    trail: &[(Id, Option<Arrival>)],
    node: Id,
    requirement: Requirement,
    origin: &Origin,
) -> Violation {
    let chain = trail
        .iter()
        .map(|(id, _)| frame_label(program, *id))
        .collect::<Vec<_>>()
        .join(" → ");
    // Anchor at the deepest user-code call site on the path; a violation with
    // no user frame at all (unlikely) falls back to the entry's span.
    // The anchor's FILE travels with it: the deepest user-code call site is
    // regularly in a module, and the diagnostic renders there (backlog E16).
    let anchor = trail
        .iter()
        .rev()
        .find_map(|(_, arrived)| {
            arrived.and_then(|arrival| arrival.user_code.then_some((arrival.span, arrival.source)))
        })
        .or_else(|| {
            let span = **program.span_map.get(&node)?;
            Some((span, program.diagnostic_source_of(node)))
        })
        .unwrap_or((Span { start: 0, end: 0 }, SourceId(0)));
    let from = match origin {
        Origin::Entry => "reachable from the entry".to_string(),
        Origin::Fence { function, fence } => {
            format!("reachable from `{function}`, fenced `[platform({fence})]`")
        }
    };
    Violation {
        error: Error {
            trace: Vec::new(),
            note: None,
            span: anchor.0,
            msg: format!(
                "`{}` requires {} and cannot run on `{}`\n  {}: {}",
                name_of(program, node),
                requirement.label,
                platform.name(),
                from,
                chain
            ),
        },
        source: anchor.1,
        cause: format!("{} | {}", requirement.label, origin.key()),
        // The nearest construction a synthetic teardown on this chain answers
        // to, if any — the nearest, so a teardown reached through another one
        // is judged against its own owner rather than the outermost.
        covers: trail.iter().rev().find_map(|(_, arrived)| {
            arrived.and_then(|arrival| arrival.teardown.then_some((arrival.source, arrival.span)))
        }),
    }
}

fn name_of(program: &Program, id: Id) -> String {
    if let Some(function) = program.functions.get(&id) {
        return function.name.to_string();
    }
    if let Some(external) = program.external_functions.get(&id) {
        return external.name.to_string();
    }
    if let Some(variable) = program.variables.get(&id) {
        return variable.name.to_string();
    }
    "closure".to_string()
}

// ── Which platform colors a FILE (E113) ──────────────────────────────────────

/// The platforms a single file of a package is analyzed under — the same
/// coloring one level up, answering "which build is this file part of?" instead
/// of "may this function run here?".
///
/// It exists because a build's platform is not only what [`check`] admits: it
/// selects `std`'s layer overlay, and therefore what the file's types *are*. A
/// browser module's `View` is `{ element }`; the process twin's is
/// `{ tag, attributes, children, text }`. Pick the wrong platform for a file and
/// correct code is reported as nonsense — E113's report, where every
/// browser-only module of a fullstack app drew "struct `View` has no field
/// `element`" in the editor while `vilan build` was clean.
///
/// The answer is **reachability**, which is what the build itself uses: a
/// multi-entry package lowers to one build unit per entry, and each unit
/// compiles the modules it loads under its own target. So:
///
/// - reached by exactly one entry → that entry's platform;
/// - reached by several → **each** of their platforms, deduplicated, in the
///   build's own leg order (browser-class first, `package_units`' rule). A
///   shared module is compiled once per leg and must type-check under every
///   one of them, so a surface reporting fewer diagnostics than the build
///   would is the same lie in the other direction; callers check it under each
///   color and report the union;
/// - reached by none → the `default-entry`'s platform, when one is designated.
///
/// The classic single-entry form is unchanged and still answers first: the
/// package's `target` colors every file under its source root. A file outside
/// the source root, a manifest with no `[package]`, and an unreached module in
/// a package that designates no `default-entry` all answer with **no**
/// platform — the caller then does what it does for a file with no project at
/// all (the CLI's `node` default, the editor's inference from the file's own
/// imports).
///
/// One function, both surfaces: `vilan check <file>` and the language server
/// must not come to two conclusions about the same file, which is exactly the
/// shape G20 established — it simply unified them on the manifest's default
/// rather than on reachability.
pub fn file_platforms(pkg_root: &Path, manifest: &Manifest, file: &Path) -> Vec<Platform> {
    file_platform_choices(pkg_root, manifest, file)
        .into_iter()
        .map(|choice| choice.platform)
        .collect()
}

/// WHY a file is analyzed under the platform it is (E119). The color decides
/// which `std` layer overlays the program, and therefore what the file's types
/// ARE — so a diagnostic about a type that only exists under the *other* overlay
/// is unreadable without the reason, and reads as a compiler mistake. E113
/// computes the color; this is the sentence that goes with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformReason {
    /// The classic single-entry package: its `target` colors every file under
    /// the source root, whatever reaches what.
    PackageTarget,
    /// A multi-entry package, and this entry's leg loads the file.
    ReachedBy(String),
    /// A multi-entry package where NO leg loads the file — a module in
    /// progress, or one whose importer was just deleted — so the designated
    /// `default-entry` answers.
    DefaultEntry(String),
    /// The caller overrode everything: `--platform <p>` on the command line.
    Flag,
}

impl PlatformReason {
    /// The clause a diagnostic appends after "this file is analyzed under
    /// `<platform>`". Written to complete that sentence, not to stand alone.
    pub fn clause(&self) -> String {
        match self {
            PlatformReason::PackageTarget => {
                "the package's `target` colors every file in it".into()
            }
            PlatformReason::ReachedBy(entry) => format!("the `{entry}` entry reaches it"),
            PlatformReason::DefaultEntry(entry) => {
                format!("no entry reaches it (default-entry is `{entry}`)")
            }
            PlatformReason::Flag => "`--platform` was passed".into(),
        }
    }
}

/// Whether `file` is a MODULE of the package rather than one of its declared
/// programs: under the source root, and not the single `[package] entry`
/// (default `main.vl`) nor any `[entry.<name>]` path.
///
/// The question every path-addressed analysis has to answer about the file it
/// was handed, and — like [`file_platform_choices`] — it must get ONE answer
/// from both surfaces, because both hand it to the same analysis. `vilan check
/// <file>` reads it to skip the `main` demand and the emission walk (E113); the
/// editor and the CLI both read it to say whether the analyzed entry is
/// [`crate::EntryMode::Declared`] or an open module (B239).
///
/// A file OUTSIDE the source root is not the package's module — it is a program
/// that happens to sit in the directory — and neither is a file under a
/// manifest with no `[package]` at all. Both answer `false`, which is what keeps
/// a bare file the program it has always been.
///
/// Compared canonically on both sides, never textually: `./src/main.vl` and
/// `src/main.vl` name one file and must get one answer, and — the symlink
/// doctrine, `spec/const.md` §9.2 — so must a file reached through a link.
pub fn is_package_module(pkg_root: &Path, manifest: &Manifest, file: &Path) -> bool {
    let Some(package) = manifest.package.as_ref() else {
        return false;
    };
    let file = crate::util::canonical_path(file);
    if !file.starts_with(crate::util::canonical_path(pkg_root)) {
        return false;
    }
    let same = |candidate: PathBuf| crate::util::canonical_path(candidate) == file;
    if manifest.entries.is_empty() {
        return !same(pkg_root.join(package.entry()));
    }
    !manifest
        .entries
        .iter()
        .any(|(name, declared)| same(pkg_root.join(declared.path(name))))
}

/// The module names `pkg::…` addresses the package's DECLARED PROGRAMS by: the
/// single `[package] entry` (default `main.vl`) and every `[entry.<name>]` path.
///
/// [`is_package_module`] read from the other side, and the fact FILE MODE was
/// missing (B240). An analysis in [`crate::EntryMode::OpenFile`] is looking at
/// one of the package's modules, and it knows that its OWN file is importable
/// (B239) — but it could not tell that a SIBLING is a program: `views.vl`
/// importing `pkg::client::helper` was clean in the editor and refused by
/// `vilan check .`, whose `client` leg compiles that very file as the entry.
/// Only the front end reads the manifest, so only the front end can say.
///
/// Read LEXICALLY off the declared paths, never against the filesystem (which
/// is why it takes no root): the name is the one the loader would resolve —
/// `<name>.vl` or `<name>/lib.vl` directly under the source root. A declared
/// entry deeper than that is not addressable as `pkg::<name>` at all, so it is
/// not listed.
pub fn declared_entry_module_names(manifest: &Manifest) -> Vec<String> {
    /// `foo.vl` -> `foo`, `foo/lib.vl` -> `foo`, anything else -> `None`.
    fn module_name(relative: &Path) -> Option<String> {
        let mut segments = relative.iter();
        let first = segments.next()?.to_str()?;
        match segments.next() {
            None => first.strip_suffix(".vl").map(str::to_string),
            Some(second) if second == "lib.vl" && segments.next().is_none() => {
                Some(first.to_string())
            }
            Some(_) => None,
        }
    }
    let Some(package) = manifest.package.as_ref() else {
        return Vec::new();
    };
    let mut names: Vec<String> = match manifest.entries.is_empty() {
        true => module_name(package.entry()).into_iter().collect(),
        false => manifest
            .entries
            .iter()
            .filter_map(|(name, declared)| module_name(&declared.path(name)))
            .collect(),
    };
    names.sort();
    names.dedup();
    names
}

/// One color a file is analyzed under, with the reason it was chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformChoice {
    pub platform: Platform,
    pub reason: PlatformReason,
}

/// [`file_platforms`], each color carrying WHY it was chosen (E119). The
/// coloring rule is stated in full on `file_platforms`; nothing here decides
/// anything it does not.
pub fn file_platform_choices(
    pkg_root: &Path,
    manifest: &Manifest,
    file: &Path,
) -> Vec<PlatformChoice> {
    let Some(package) = manifest.package.as_ref() else {
        return Vec::new();
    };
    // Compared canonically on both sides, never textually: `./src/main.vl` and
    // `src/main.vl` name one file and must get one answer, and — the symlink
    // doctrine, `spec/const.md` §9.2 — so must a source root reached through a
    // link.
    let file = crate::util::canonical_path(file);
    let root = crate::util::canonical_path(pkg_root);
    if manifest.entries.is_empty() {
        return if file.starts_with(&root) {
            vec![PlatformChoice {
                platform: package.resolved_target().unwrap_or_default(),
                reason: PlatformReason::PackageTarget,
            }]
        } else {
            Vec::new()
        };
    }
    // The legs, in the order the build compiles them (`package_units`):
    // browser-class first, stable among themselves — so the FIRST color a
    // shared module reports is the first leg to compile it.
    let mut legs: Vec<(&str, Platform)> = manifest
        .entries
        .iter()
        .map(|(name, entry)| (name.as_str(), entry.resolved_target().unwrap_or_default()))
        .collect();
    legs.sort_by_key(|(_, platform)| !matches!(platform, Platform::Browser));
    let mut choices: Vec<PlatformChoice> = Vec::new();
    for (name, platform) in &legs {
        if choices.iter().any(|choice| choice.platform == *platform) {
            continue;
        }
        let entry = pkg_root.join(manifest.entries[*name].path(name));
        if crate::analyzer::package_modules_reachable_from(&entry, pkg_root).contains(&file) {
            choices.push(PlatformChoice {
                platform: *platform,
                reason: PlatformReason::ReachedBy((*name).to_string()),
            });
        }
    }
    if !choices.is_empty() {
        return choices;
    }
    // Unreached: the designated leg's platform, for a file that is the
    // package's to color at all. A file outside the source root is not (it
    // still resolves `pkg::` and the dependencies, which is what it needs), and
    // a package that designates no `default-entry` has nothing to fall back to;
    // either way the caller keeps whatever it does for a file with no project.
    if !file.starts_with(&root) {
        return Vec::new();
    }
    manifest
        .default_entry()
        .and_then(|name| Some((name, manifest.entries.get(name)?)))
        .map(|(name, entry)| {
            vec![PlatformChoice {
                platform: entry.resolved_target().unwrap_or_default(),
                reason: PlatformReason::DefaultEntry(name.to_string()),
            }]
        })
        .unwrap_or_default()
}

/// The program's entry: a function named `main` defined in user code. Also
/// used by async inference's initializer check — "which initializers run"
/// must mean the same thing to admission, emission, and awaiting.
pub(crate) fn entry_function(program: &Program) -> Option<Id> {
    program
        .functions
        .iter()
        .find(|(id, function)| function.name == "main" && is_user_code(program, **id))
        .map(|(id, _)| *id)
}
