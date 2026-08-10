//! Module-level initialization order (`proposal/b33-emission-order.md`).
//!
//! Every module-level `let` emits as a JavaScript `const`, and `const` is not
//! hoisted: a binding whose initializer *evaluates* another binding must be
//! declared after it, or the read is a temporal-dead-zone `ReferenceError` at
//! load. Emission used to hand the transformer
//! [`Program::module_level_bindings`] in the entry scope's insertion order —
//! import-statement order, i.e. a spelling detail — so the same program could
//! build correctly or TDZ-crash depending on how its imports were listed.
//!
//! This module computes the order instead: the **load-time relation** (§2), a
//! topological sort over it, and the **canonical key** as tie-break, so the
//! emitted declaration order is a pure function of the program.
//!
//! # The load-time relation (§2)
//!
//! The edge is "B's initializer *evaluates* X at load time" — deliberately NOT
//! the call graph's reachability, which differs on one load-bearing class:
//!
//! - **Creating a closure is inert.** A closure a binding merely creates does
//!   not run at load, so its body contributes no ordering edge to its creator.
//!   This is what keeps the mutually-recursive module-closure idiom
//!   (`let EVEN = |n| { .. ODD(n-1) }` / `let ODD = |n| { .. EVEN(n-1) }`)
//!   legal: two creations, no calls, no edges, no cycle. Building on raw
//!   [`CallGraph::successors`] would manufacture one and reject working code.
//! - **Calls made during initialization are followed, transitively.** A direct
//!   call enters the callee's body; a generic/trait dispatch follows the
//!   existing `dispatch_candidates` over-approximation; and every function
//!   VALUE reachable through a load-time call's subject or arguments is
//!   entered too, because the callee may invoke it (`apply(CB)`,
//!   `apply(|| { Y })`, `LIST.map(|e| e + Y)`, and — the receiver being
//!   argument 0 — `HOLDER.run()`). Everything read inside anything entered
//!   charges to the *initializing* binding, never to the closure's creator.
//! - **Reads are edges**, at the initializer and inside anything entered.
//!
//! The relation is deliberately **conservative**: it may add an edge that no
//! execution needs, never omit one it can see. That asymmetry is the whole
//! safety argument. Once emission order is *derived*, a shape the relation
//! fails to model is not "left as it was" — it is a miscompile, because the
//! surrounding order moved out from under it. So [`LoadTimeWalk::value_bodies`]
//! matches `Expr` **exhaustively**, with no catch-all: a new variant must be
//! classified there or the crate does not compile (the same law
//! `call_graph.rs`'s collector states, after its `Index` blind spot shipped two
//! miscompiles).
//!
//! `const`-marked bindings fold to literals before any of this; the call graph
//! never collects their initializers, so they have no outgoing edges. They stay
//! legitimate *targets*: a `const X = 42;` declaration must still precede a
//! binding that reads `X`.
//!
//! # Cycles
//!
//! A cycle has no valid initialization order, so it is a **compile error**
//! ([`check_cycles`], §3). The sort must still produce something sane for the
//! bindings around one — a cycle must not corrupt the order of bindings that
//! are merely *downstream* of it — so it runs over the **condensation**:
//! strongly connected components first, then a topological order of the
//! resulting DAG (which is acyclic by construction, so it always drains
//! completely). Only the members of a genuine cycle are ordered arbitrarily —
//! among themselves, canonically; everything that merely depends on a cycle
//! still orders after it.

use std::collections::{BTreeSet, VecDeque};

use crate::fx::{FxHashMap as HashMap, FxHashSet as HashSet};

use indexmap::IndexMap;

use crate::analyzer::{Expr, ExprIfBranch, Program, SourceId};
use crate::call_graph::{CallGraph, CallTarget, IndirectReason};
use crate::error::{Error, Note};
use crate::id::Id;
use crate::span::Span;

/// Every module-level binding, in the order its initializer runs: dependency
/// order over the load-time relation, ties broken by the canonical key.
///
/// Deterministic by construction. The relation is read out of the call graph,
/// whose per-node tables are keyed by entity id and whose collection order
/// comes from `IndexMap`s walked in the canonical module order (WO-1b). The
/// sort itself never consults the input vector's order: it re-sorts by the
/// canonical key, the component walk visits roots and edges in that order, and
/// the ready set is a `BTreeSet` keyed by it. So the result is a pure function
/// of the analyzed program, not of how `module_level_bindings` happened to
/// enumerate it.
pub fn initialization_order(program: &Program, graph: &CallGraph) -> Vec<Id> {
    let dependencies = load_time_dependencies(program, graph);
    let bindings: Vec<Id> = dependencies.keys().copied().collect();
    canonical_topological_order(&bindings, &dependencies)
}

/// The load-time relation: for each module-level binding, the bindings its
/// initializer evaluates at load time, ascending by canonical key.
///
/// Keyed in canonical order, so a consumer that iterates it (the cycle
/// diagnostic) is deterministic too.
pub fn load_time_dependencies(program: &Program, graph: &CallGraph) -> IndexMap<Id, Vec<Id>> {
    let mut bindings = program.module_level_bindings();
    bindings.sort_by_key(canonical_key);
    let mut walk = LoadTimeWalk::new(program, graph);
    bindings
        .iter()
        .map(|binding| (*binding, walk.evaluated_globals(*binding)))
        .collect()
}

/// The cycle check (§3): a dependency cycle among module-level initializers is
/// a compile error, because no declaration order can satisfy it — whichever
/// member is declared first reads a `const` that has not initialized yet, which
/// is a temporal-dead-zone `ReferenceError` at load.
///
/// One diagnostic per cycle, not per member (the B29 lesson: one diagnostic per
/// mistake). A cycle is a strongly connected component of the load-time
/// relation with more than one member, or a single binding whose initializer
/// evaluates itself.
///
/// Runs post-`analyze()` beside the const pass, and — like it — only on a
/// program that analyzed cleanly. Two reasons: the relation is read out of the
/// call graph, whose per-node tables a failed analysis may have left partial
/// (a false cycle out of half-resolved data would be worse than a late one),
/// and B5 keeps one root cause on screen at a time. A cycle survives its
/// program's other errors, so it surfaces on the next analysis.
pub fn check_cycles(program: &mut Program) {
    if !program.diagnostics.is_empty() {
        return;
    }
    // The settled graph, built here and kept on the program: emission needs the
    // same one moments later, and building it twice cost ~3% of a clean compile
    // (`b33-emission-order.md` §4).
    let found = cycle_diagnostics(program, program.call_graph());
    // Each diagnostic goes in with the file its span indexes into, so a
    // cross-module cycle squiggles — and renders — in the module it is about
    // (`Program::push_diagnostic` keeps the two vectors parallel).
    for (error, source) in found {
        program.push_diagnostic(error, source);
    }
}

/// Every initialization cycle in the program, as diagnostics paired with the
/// source file each is anchored in, ordered by the canonical key of each
/// cycle's first member.
fn cycle_diagnostics(program: &Program, graph: &CallGraph) -> Vec<(Error, SourceId)> {
    let dependencies = load_time_dependencies(program, graph);
    let bindings: Vec<Id> = dependencies.keys().copied().collect();
    let members: HashSet<Id> = bindings.iter().copied().collect();
    let mut components = strongly_connected_components(&bindings, &dependencies, &members);
    for component in &mut components {
        component.sort_by_key(canonical_key);
    }
    components.sort_by_key(|component| canonical_key(&component[0]));

    let mut walk = LoadTimeWalk::new(program, graph);
    components
        .iter()
        .filter(|component| is_cycle(component, &dependencies))
        .map(|component| cycle_error(program, &mut walk, component, &dependencies))
        .collect()
}

/// Whether a component is a cycle: more than one member, or one that depends
/// on itself. A lone binding with no self-edge is just a binding.
fn is_cycle(component: &[Id], dependencies: &IndexMap<Id, Vec<Id>>) -> bool {
    component.len() > 1
        || dependencies
            .get(&component[0])
            .is_some_and(|edges| edges.contains(&component[0]))
}

/// One cycle, rendered (diagnostics-standard.md): anchored at a read that
/// closes it (A1 — the narrowest expression that identifies the problem, not
/// the whole initializer), with a `via` chain, the participants' declarations,
/// and a note at the declaration the anchored read names (C3).
fn cycle_error(
    program: &Program,
    walk: &mut LoadTimeWalk,
    component: &[Id],
    dependencies: &IndexMap<Id, Vec<Id>>,
) -> (Error, SourceId) {
    let chain = shortest_cycle(component, dependencies);
    let witnesses: Vec<Option<ReadWitness>> = chain
        .windows(2)
        .map(|step| walk.read_witness(step[0], step[1]))
        .collect();
    // The read that closes the cycle, from the member that starts the chain —
    // the canonically first, so the anchor never depends on enumeration order.
    let anchor = witnesses
        .first()
        .and_then(|witness| witness.as_ref())
        .map(|witness| witness.reference)
        // Defensive: an edge is only ever recorded because a read produced it,
        // so the witness walk finds one. Falling back to the declaration keeps
        // this a total function rather than a panic in a diagnostic path.
        .unwrap_or(chain[0]);
    let via_dispatch = witnesses
        .iter()
        .any(|witness| witness.as_ref().is_some_and(|witness| witness.via_dispatch));

    let names: Vec<String> = component
        .iter()
        .map(|member| format!("`{}`", binding_name(program, *member)))
        .collect();
    let mut message = if component.len() == 1 {
        // The degenerate case reads as what it is — a binding that evaluates
        // itself — rather than as a `via A → A` chain.
        format!(
            "`{}`'s initializer evaluates `{0}` itself, which has not initialized yet",
            binding_name(program, component[0])
        )
    } else {
        format!(
            "{} form an initialization cycle: module-level bindings initialize in dependency \
             order, and a cycle has no such order",
            join_and(&names)
        )
    };
    if chain.len() > 2 {
        let steps: Vec<String> = chain
            .iter()
            .map(|member| format!("`{}`", binding_name(program, *member)))
            .collect();
        message.push_str(&format!("\n  via {}", steps.join(" → ")));
    }
    if component.len() > 1 {
        let declarations: Vec<String> = component
            .iter()
            .map(|member| declaration_label(program, *member))
            .collect();
        message.push_str(&format!("\n  declared: {}", declarations.join(", ")));
    }
    if via_dispatch {
        // §5(b): the relation follows every candidate of a dispatched call, so
        // a cycle can be built out of an implementation this program never
        // instantiates. Saying so makes a false positive self-explaining.
        message.push_str(
            "\n  the cycle runs through a dispatched call, so it includes every implementation \
             of that method; one this program never instantiates still participates",
        );
    }
    if component.len() > 1 {
        // The escape hatch, and the rule behind it: creating a closure is not
        // evaluating it (§2), which is why mutually-recursive module closures
        // are legal.
        message.push_str(
            "\n  a closure's body is not evaluated at load; moving one of these reads inside a \
             closure breaks the cycle",
        );
    }

    // The note names the declaration the anchored read reached for. For a
    // self-cycle that is the binding itself; otherwise it is the next member of
    // the chain, whose own initializer carries the cycle onward — and it is
    // often in another file, which is exactly what a note is for. It is dropped
    // when it would add nothing: a note whose span CONTAINS the primary one, in
    // the same file, points at a declaration the reader is already looking at
    // (`let A = A + 1`).
    let noted = chain.get(1).copied().unwrap_or(chain[0]);
    let anchor_span = span_of(program, anchor);
    let noted_span = span_of(program, noted);
    let already_shown = program.note_source_of(noted) == program.note_source_of(anchor)
        && noted_span.start <= anchor_span.start
        && anchor_span.end <= noted_span.end;
    let note = (!already_shown).then(|| Note {
        span: noted_span,
        msg: format!("`{}` is declared here", binding_name(program, noted)),
        // The contract on `Note::source`: name the file only when it differs
        // from the primary span's.
        source: program
            .note_source_of(noted)
            .filter(|source| Some(*source) != program.note_source_of(anchor)),
    });
    program.anchored(
        Error {
            span: anchor_span,
            msg: message,
            note,
        },
        anchor,
    )
}

/// A shortest cycle through the component's canonically first member, as the
/// member sequence `A → … → A` (so the first and last entries are the same
/// binding). A self-edge yields `[A, A]`.
///
/// The whole component is reported as the cycle's participants, but the CHAIN
/// shows one concrete round trip: a strongly connected component need not have
/// a cycle through every member at once, so a "chain" over all of them would be
/// a fiction. Shortest, so the witness is the simplest one available; rooted at
/// the canonically first member and walking edges in canonical order, so it is
/// a pure function of the program.
fn shortest_cycle(component: &[Id], dependencies: &IndexMap<Id, Vec<Id>>) -> Vec<Id> {
    let start = component[0];
    let inside: HashSet<Id> = component.iter().copied().collect();
    let edges_of = |node: Id| -> &[Id] {
        dependencies
            .get(&node)
            .map(Vec::as_slice)
            .unwrap_or_default()
    };
    if edges_of(start).contains(&start) {
        return vec![start, start];
    }
    let mut previous: HashMap<Id, Id> = HashMap::default();
    let mut seen: HashSet<Id> = HashSet::default();
    seen.insert(start);
    let mut queue: VecDeque<Id> = VecDeque::new();
    queue.push_back(start);
    while let Some(node) = queue.pop_front() {
        for next in edges_of(node) {
            if !inside.contains(next) {
                continue;
            }
            if *next == start {
                // Nodes leave the queue in nondecreasing distance order, so the
                // first edge back to the start closes a shortest cycle. Walk
                // the predecessors back to the start, then reverse.
                let mut chain = vec![node];
                let mut cursor = node;
                while let Some(step) = previous.get(&cursor) {
                    chain.push(*step);
                    cursor = *step;
                }
                chain.reverse();
                chain.push(start);
                return chain;
            }
            if seen.insert(*next) {
                previous.insert(*next, node);
                queue.push_back(*next);
            }
        }
    }
    // Unreachable for a genuine component (strong connectivity guarantees a
    // round trip). Kept total: a diagnostic must not panic.
    let mut chain = component.to_vec();
    chain.push(start);
    chain
}

/// A binding's declaration span — the whole `let` (`span_map`'s entry), which
/// is where a note points.
fn span_of(program: &Program, id: Id) -> Span {
    program
        .span_map
        .get(&id)
        .map(|span| **span)
        .unwrap_or(Span { start: 0, end: 0 })
}

fn binding_name(program: &Program, id: Id) -> String {
    program
        .variables
        .get(&id)
        .map(|variable| variable.name.to_string())
        .unwrap_or_else(|| "?".to_string())
}

/// `` `A` in `alpha.vl` `` — the participant plus the file it is declared in,
/// so a cross-module cycle names both ends. The file name is what the user
/// wrote the module as; a binding with no recorded source (none exist today —
/// every module binding comes out of a file walk) renders as the bare name.
fn declaration_label(program: &Program, id: Id) -> String {
    let name = binding_name(program, id);
    let file = program
        .source_of(id)
        .and_then(|source| program.source_path(source))
        .and_then(|path| path.file_name())
        .map(|file| file.to_string_lossy().into_owned());
    match file {
        Some(file) => format!("`{name}` in `{file}`"),
        None => format!("`{name}`"),
    }
}

/// `` `A` and `B` ``, `` `A`, `B` and `C` `` — the participants, in the order
/// they are given.
fn join_and(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [only] => only.clone(),
        [first, second] => format!("{first} and {second}"),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Where a cycle's edge was read, and how the walk got there.
struct ReadWitness {
    /// The reference expression that reads the edge's target.
    reference: Id,
    /// Whether every path from the initializer to that read went through a
    /// dispatched call's candidate list (§5(b)'s over-approximation).
    via_dispatch: bool,
}

/// The canonical key of a binding: its entity id.
///
/// Entity ids are minted monotonically by the analyzer's one walk, and WO-1b
/// made that walk canonical, so the numeric id *is* the canonical key. The walk
/// order — probed, not assumed — is:
///
/// 1. every module drained by the load loop, smallest `load_order_key` first:
///    `(tier, package index, module name)`, tier std = 0 < dependency = 1 <
///    entry package = 2 (`analyzer.rs`, `load_order_key`);
/// 2. then `std`'s own `lib.vl`;
/// 3. then each dependency's ROOT `lib.vl`, in manifest order;
/// 4. then the entry file itself.
///
/// Items inside a file are numbered in declaration order. Note steps 2–4: a
/// package's root `lib.vl` is walked AFTER every drained module, so a
/// dependency's root-level binding sorts after an entry-package module's — the
/// tier ordering governs the drain loop, not the whole walk. Correctness does
/// not depend on which of these two a program picks (only that it is a total,
/// spelling-independent order), but ratified call (d) makes this order
/// *specified*, so S3's spec text must describe the walk above rather than the
/// drain loop's tiers alone.
///
/// Re-deriving `(origin, module name, index)` here would reinvent all of it,
/// and would have to re-export the loader's block-local `Origin`; the id is
/// also already what function emission sorts by (`transform_entry_ast`'s
/// `t_functions.sort_by`).
fn canonical_key(binding: &Id) -> u32 {
    binding.0
}

/// The initialization order for one relation: a topological order of the
/// dependency graph's **condensation**, with the canonical key as tie-break.
///
/// Two properties, in order of importance:
///
/// - **Every recorded edge is honored where it can be.** Condensing first means
///   a binding that merely *depends on* a cycle still orders after it. Appending
///   the whole undrained remainder in canonical order instead — the obvious
///   shortcut — silently drops real edges: a false cycle produced by the
///   dispatch over-approximation would sweep its downstream readers along and
///   emit them first, miscompiling a program that runs correctly today.
/// - **Among bindings the relation leaves unordered, canonical order wins.**
///   Kahn with min-selection (always the smallest available component) is that
///   rule, and it mirrors WO-1b's min-selection module drain. It is greedy, not
///   "shift the block": with `1 -> 3` and an unrelated `2`, the order is
///   `2, 3, 1`.
///
/// Every component is a singleton for an acyclic relation, so this is exactly
/// plain min-selection Kahn there.
///
/// Split out from [`load_time_dependencies`] so the ordering law is testable
/// against a synthetic relation, with no program to build.
fn canonical_topological_order(bindings: &[Id], dependencies: &IndexMap<Id, Vec<Id>>) -> Vec<Id> {
    let members: HashSet<Id> = bindings.iter().copied().collect();
    let mut canonical: Vec<Id> = bindings.to_vec();
    canonical.sort_by_key(canonical_key);

    let mut components = strongly_connected_components(&canonical, dependencies, &members);
    for component in &mut components {
        component.sort_by_key(canonical_key);
    }
    let mut component_of: HashMap<Id, usize> = HashMap::default();
    for (index, component) in components.iter().enumerate() {
        for member in component {
            component_of.insert(*member, index);
        }
    }
    // A component's key is its smallest member's — unique across components, so
    // the ready set is totally ordered.
    let component_key = |index: usize| canonical_key(&components[index][0]);

    // The condensation's edges: a component comes after every component it
    // depends on. Two members depending on the same other component enter this
    // twice; each duplicate increments and decrements once, so Kahn is
    // unaffected.
    let mut unmet: Vec<usize> = vec![0; components.len()];
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); components.len()];
    for binding in &canonical {
        let dependent = component_of[binding];
        for dependency in dependencies
            .get(binding)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            // Defensive: the relation only ever names module-level bindings.
            let Some(&provider) = component_of.get(dependency) else {
                continue;
            };
            if provider == dependent {
                continue;
            }
            dependents[provider].push(dependent);
            unmet[dependent] += 1;
        }
    }

    let mut ready: BTreeSet<(u32, usize)> = (0..components.len())
        .filter(|index| unmet[*index] == 0)
        .map(|index| (component_key(index), index))
        .collect();
    let mut order: Vec<Id> = Vec::with_capacity(canonical.len());
    while let Some(entry) = ready.iter().next().copied() {
        ready.remove(&entry);
        let (_key, index) = entry;
        order.extend(components[index].iter().copied());
        for dependent in &dependents[index] {
            unmet[*dependent] -= 1;
            if unmet[*dependent] == 0 {
                ready.insert((component_key(*dependent), *dependent));
            }
        }
    }

    // Unreachable by construction — a condensation is acyclic, so Kahn drains
    // it completely. Kept as a total function rather than a panic: an ordering
    // pass must never be the thing that takes the compiler down.
    if order.len() < canonical.len() {
        let placed: HashSet<Id> = order.iter().copied().collect();
        order.extend(canonical.iter().copied().filter(|id| !placed.contains(id)));
    }
    order
}

/// Tarjan's strongly connected components over the dependency edges.
///
/// Iterative: the recursion depth would otherwise be the binding count, and the
/// compiler already runs analysis on an enlarged stack precisely because deep
/// recursion here has bitten before. Roots are visited in canonical order and
/// each node's edges in canonical order (the relation's vectors are sorted), so
/// the components — and their member order before the caller re-sorts — are a
/// pure function of the relation.
fn strongly_connected_components(
    canonical: &[Id],
    dependencies: &IndexMap<Id, Vec<Id>>,
    members: &HashSet<Id>,
) -> Vec<Vec<Id>> {
    struct Frame {
        node: Id,
        next_edge: usize,
    }
    let mut index_of: HashMap<Id, u32> = HashMap::default();
    let mut low_of: HashMap<Id, u32> = HashMap::default();
    let mut on_stack: HashSet<Id> = HashSet::default();
    let mut stack: Vec<Id> = Vec::new();
    let mut next_index: u32 = 0;
    let mut components: Vec<Vec<Id>> = Vec::new();

    let mut open = |node: Id,
                    index_of: &mut HashMap<Id, u32>,
                    low_of: &mut HashMap<Id, u32>,
                    stack: &mut Vec<Id>,
                    on_stack: &mut HashSet<Id>| {
        index_of.insert(node, next_index);
        low_of.insert(node, next_index);
        next_index += 1;
        stack.push(node);
        on_stack.insert(node);
    };

    for root in canonical {
        if index_of.contains_key(root) {
            continue;
        }
        open(*root, &mut index_of, &mut low_of, &mut stack, &mut on_stack);
        let mut frames: Vec<Frame> = vec![Frame {
            node: *root,
            next_edge: 0,
        }];
        while let Some(frame) = frames.last_mut() {
            let node = frame.node;
            let edges = dependencies
                .get(&node)
                .map(Vec::as_slice)
                .unwrap_or_default();
            if frame.next_edge < edges.len() {
                let target = edges[frame.next_edge];
                frame.next_edge += 1;
                if !members.contains(&target) {
                    continue;
                }
                if !index_of.contains_key(&target) {
                    open(
                        target,
                        &mut index_of,
                        &mut low_of,
                        &mut stack,
                        &mut on_stack,
                    );
                    frames.push(Frame {
                        node: target,
                        next_edge: 0,
                    });
                } else if on_stack.contains(&target) {
                    let target_index = index_of[&target];
                    let low = low_of.get_mut(&node).expect("opened above");
                    *low = (*low).min(target_index);
                }
                continue;
            }
            // `node` is finished: close it, propagate its low-link to the
            // caller, and pop a component when it is a root.
            let node_low = low_of[&node];
            let node_index = index_of[&node];
            frames.pop();
            if let Some(parent) = frames.last() {
                let parent_low = low_of.get_mut(&parent.node).expect("opened above");
                *parent_low = (*parent_low).min(node_low);
            }
            if node_low == node_index {
                let mut component = Vec::new();
                while let Some(member) = stack.pop() {
                    on_stack.remove(&member);
                    component.push(member);
                    if member == node {
                        break;
                    }
                }
                components.push(component);
            }
        }
    }
    components
}

/// How a unit was entered: by a call the walk could resolve, or through the
/// dispatch over-approximation (§5(b) — every trait candidate of a bounded
/// call, instantiated or not). Only the cycle diagnostic distinguishes them;
/// the ordering relation treats both as "entered".
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum EnteredVia {
    /// Sorts first, so a unit entered BOTH ways classifies as direct.
    Direct,
    Dispatch,
}

/// Walks the load-time relation, memoizing the part that is context-free.
struct LoadTimeWalk<'a, 'src> {
    program: &'a Program<'src>,
    graph: &'a CallGraph,
    /// Code unit → the units its execution ENTERS (its resolved callees, with
    /// dispatch and every function value passed through a call expanded), each
    /// tagged with how it was reached. This depends only on the unit, never on
    /// which binding's initialization reached it, so it is computed once and
    /// shared by every binding's walk.
    entered: HashMap<Id, Vec<(Id, EnteredVia)>>,
}

impl<'a, 'src> LoadTimeWalk<'a, 'src> {
    fn new(program: &'a Program<'src>, graph: &'a CallGraph) -> Self {
        LoadTimeWalk {
            program,
            graph,
            entered: HashMap::default(),
        }
    }

    /// The module-level bindings `binding`'s initializer evaluates at load
    /// time, ascending by canonical key. Includes `binding` itself when its
    /// initializer reads it (a 1-cycle) — the ordering pass must see that, not
    /// silently drop it.
    fn evaluated_globals(&mut self, binding: Id) -> Vec<Id> {
        let mut reads: BTreeSet<u32> = BTreeSet::new();
        let mut seen: HashSet<Id> = HashSet::default();
        seen.insert(binding);
        let mut pending = vec![binding];
        while let Some(unit) = pending.pop() {
            for (_reference, global) in self.graph.global_references_of(unit) {
                reads.insert(canonical_key(global));
            }
            for (next, _via) in self.entered_by(unit) {
                if seen.insert(next) {
                    pending.push(next);
                }
            }
        }
        reads.into_iter().map(Id).collect()
    }

    /// A witness for the relation's edge `binding → target`: the reference
    /// expression that reads `target` during `binding`'s initialization, and
    /// whether reaching it required the dispatch over-approximation.
    ///
    /// The same walk as [`Self::evaluated_globals`], carrying two extra things
    /// the ordering pass has no use for: *where* the read is, and how it was
    /// reached. Ties are broken toward a DIRECT path and then the lowest
    /// reference id, so the chosen witness is a pure function of the program —
    /// and `via_dispatch` is true only when *every* path to a read of `target`
    /// went through a dispatched call's candidates, which is exactly when the
    /// §5(b) note applies.
    fn read_witness(&mut self, binding: Id, target: Id) -> Option<ReadWitness> {
        // Unit → whether the best path to it so far crossed a dispatch edge.
        // `false` is the better value, so a unit already reached directly is
        // never re-queued, and one first reached by dispatch is re-queued if a
        // direct path turns up later.
        let mut dispatched_at: HashMap<Id, bool> = HashMap::default();
        dispatched_at.insert(binding, false);
        let mut queue: VecDeque<Id> = VecDeque::new();
        queue.push_back(binding);
        let mut best: Option<(bool, u32)> = None;
        let graph = self.graph;
        while let Some(unit) = queue.pop_front() {
            let dispatched = dispatched_at[&unit];
            for (reference, global) in graph.global_references_of(unit) {
                if *global != target {
                    continue;
                }
                let candidate = (dispatched, reference.0);
                if best.is_none_or(|current| candidate < current) {
                    best = Some(candidate);
                }
            }
            for (next, via) in self.entered_by(unit) {
                let next_dispatched = dispatched || via == EnteredVia::Dispatch;
                if dispatched_at
                    .get(&next)
                    .is_some_and(|reached| *reached <= next_dispatched)
                {
                    continue;
                }
                dispatched_at.insert(next, next_dispatched);
                queue.push_back(next);
            }
        }
        best.map(|(via_dispatch, reference)| ReadWitness {
            reference: Id(reference),
            via_dispatch,
        })
    }

    /// The code units executing `unit` enters. Only CALLS are followed: a
    /// closure the unit merely creates is not entered, which is the §2 rule
    /// that keeps mutually-recursive module closures legal.
    ///
    /// A binding's initializer and a function/closure body are one vocabulary
    /// here — the call graph files a binding's calls under `initializer_calls`
    /// and a node's under `calls`, and each is empty for the other kind, so
    /// chaining them reads whichever applies.
    fn entered_by(&mut self, unit: Id) -> Vec<(Id, EnteredVia)> {
        if let Some(cached) = self.entered.get(&unit) {
            return cached.clone();
        }
        let graph = self.graph;
        let mut entered: Vec<(Id, EnteredVia)> = Vec::new();
        for call in graph
            .calls_of(unit)
            .iter()
            .chain(graph.initializer_calls_of(unit))
        {
            match call.target {
                CallTarget::Function(callee) | CallTarget::Closure(callee) => {
                    entered.push((callee, EnteredVia::Direct))
                }
                // An extern is a leaf with no Vilan body; a variant constructor
                // builds a value and calls nothing. (Neither is a dead end for
                // the values passed to it — see below.)
                CallTarget::External(_) | CallTarget::Variant(_) => {}
                // Resolved by the value pass below, which subsumes it.
                CallTarget::Indirect(IndirectReason::Value) => {}
                // A generic/trait dispatch follows the same over-approximation
                // async inference and platform coloring use: every candidate.
                // §5(b) records the false-cycle risk this carries — and tags
                // it, so a cycle built out of such an edge can say so.
                CallTarget::Indirect(_) => entered.extend(
                    crate::async_infer::dispatch_candidates(self.program, call.call_id)
                        .into_iter()
                        .map(|candidate| (candidate, EnteredVia::Dispatch)),
                ),
            }
            // Every function VALUE this call can hand to its callee is entered
            // too: a call that runs at load may invoke what it was given, and
            // the callee's own signature is no guarantee it does not (an
            // `[extern]` helper — `List::map` lowers to one — has no Vilan body
            // to walk at all). The receiver of a method call is argument 0, so
            // `HOLDER.run()` reaches the closures `HOLDER` holds through this
            // too. Conservative: it only ever ADDS edges.
            if let Some(function_call) = self.program.function_calls.get(&call.call_id) {
                let mut seen = HashSet::default();
                let mut values = Vec::new();
                self.value_bodies(function_call.subject_id, &mut values, &mut seen);
                for argument in &function_call.argument_ids {
                    self.value_bodies(*argument, &mut values, &mut seen);
                }
                entered.extend(values.into_iter().map(|body| (body, EnteredVia::Direct)));
            }
        }
        // By canonical key, then by provenance — so a unit entered both ways
        // keeps its `Direct` tag when the duplicate is dropped.
        entered.sort_by_key(|(body, via)| (canonical_key(body), *via));
        entered.dedup_by_key(|(body, _via)| *body);
        self.entered.insert(unit, entered.clone());
        entered
    }

    /// The functions and closures the value of `expr` can be, or can contain.
    ///
    /// Module bindings are immutable (`let mut` at module level is a parse
    /// error), so a global's possible closures are statically known: the ones
    /// its own initializer created, or those created along its value's def
    /// chain.
    ///
    /// **Exhaustive by law** — no `_` arm. A shape left unmodeled here is a
    /// missing ordering edge, and a missing edge under a *derived* order is a
    /// miscompile, not a status quo. A new `Expr` variant must therefore be
    /// classified as one of: a body, a pass-through, an aggregate that contains
    /// values, or a leaf that provably cannot yield a body.
    fn value_bodies(&self, expr: Id, bodies: &mut Vec<Id>, seen: &mut HashSet<Id>) {
        if !seen.insert(expr) {
            return;
        }
        let Some(entity) = self.program.entity_map.get(&expr) else {
            return;
        };
        match entity {
            // --- Bodies ---
            // A closure literal, an `async` block, or a function named as a
            // value (fn-to-closure coercion).
            Expr::Closure(closure_id) | Expr::Async(closure_id) => bodies.push(*closure_id),
            Expr::Function(function_id) => bodies.push(*function_id),
            Expr::Local(binding) | Expr::Variable(binding) | Expr::Parameter(binding) => {
                if self.program.functions.contains_key(binding) {
                    bodies.push(*binding);
                    return;
                }
                // The closures a module-level binding's initializer created —
                // recorded by the call graph, so conditional and nested
                // creations are covered without re-walking the tree.
                bodies.extend(self.graph.initializer_closures_of(*binding).iter().copied());
                if let Some(initial) = self
                    .program
                    .variables
                    .get(binding)
                    .and_then(|variable| variable.initial)
                {
                    self.value_bodies(initial, bodies, seen);
                }
            }
            // `let F = make(); .. F()` — the value came out of a call, so it can
            // be any closure created ANYWHERE in the callee's execution (a
            // two-level `fun make() { inner() }` def chain counts), or a
            // function it names as a value. A variant constructor is different:
            // it stores its arguments, so the payload is the value.
            Expr::Call(inner_call_id) => {
                let Some(inner) = self.program.function_calls.get(inner_call_id) else {
                    return;
                };
                if self.subject_is_a_variant_constructor(inner.subject_id) {
                    for argument in &inner.argument_ids {
                        self.value_bodies(*argument, bodies, seen);
                    }
                    return;
                }
                let mut callees = Vec::new();
                self.value_bodies(inner.subject_id, &mut callees, seen);
                for callee in callees {
                    for unit in self.direct_call_closure(callee) {
                        bodies.extend(
                            self.graph
                                .closure_children_of(unit)
                                .unwrap_or_default()
                                .iter()
                                .copied(),
                        );
                        bodies.extend(
                            self.graph
                                .function_references_of(unit)
                                .iter()
                                .map(|(_reference, function)| *function),
                        );
                    }
                }
            }

            // --- Pass-throughs: the value is (inside) a sub-expression ---
            // A projection out of a value — `(HOLDER.get)()`, `PAIR.0()`,
            // `TABLE[0]()`. Which field holds which closure is not modeled, so
            // this over-approximates within one value rather than missing it.
            Expr::Field(subject, _, _) | Expr::TupleIndex(subject, _, _) => {
                self.value_bodies(*subject, bodies, seen)
            }
            Expr::Index(subject, _) => self.value_bodies(*subject, bodies, seen),
            Expr::Block((_statements, tail)) => self.value_bodies(*tail, bodies, seen),
            // `(if FLAG { CB_A } else { CB_B })()` — either branch's value.
            Expr::If(branch) => self.value_bodies_of_if(branch, bodies, seen),
            Expr::Match(_subject, legs) => {
                for leg in legs {
                    self.value_bodies(leg.body, bodies, seen);
                }
            }
            // A lift region's value is its body; a `?` step's is the
            // continuation's.
            Expr::LiftRegion(_steps, body) => self.value_bodies(*body, bodies, seen),
            Expr::Lift(_subject, _binder, continuation) => {
                self.value_bodies(*continuation, bodies, seen)
            }
            // `expr?` yields the receiver's unwrapped payload.
            Expr::TryAssert(receiver) => self.value_bodies(*receiver, bodies, seen),
            // A view of a value, and an awaited promise of one, are that value.
            Expr::Reference(inner, _) | Expr::Dereference(inner) | Expr::Await(inner) => {
                self.value_bodies(*inner, bodies, seen)
            }
            // `return e` in expression position diverges, but its operand is
            // still a value that flows outward.
            Expr::FunctionReturn(Some(value)) => self.value_bodies(*value, bodies, seen),

            // --- Aggregates: they CONTAIN values ---
            Expr::List(elements) | Expr::Tuple(elements) => {
                for element in elements {
                    self.value_bodies(*element, bodies, seen);
                }
            }
            Expr::TupleComprehension(first, second, third) => {
                self.value_bodies(*first, bodies, seen);
                self.value_bodies(*second, bodies, seen);
                self.value_bodies(*third, bodies, seen);
            }
            Expr::StructInitializer(_struct_id, fields) => {
                for value in fields.values() {
                    self.value_bodies(*value, bodies, seen);
                }
            }
            // `[f; 3]` holds copies of one value.
            Expr::Repeat(value, _length) => self.value_bodies(*value, bodies, seen),

            // --- Leaves: cannot yield a body ---
            // Scalars, strings, and the empty/erroneous values.
            Expr::Bool(_)
            | Expr::Null
            | Expr::Number(..)
            | Expr::String(_)
            | Expr::MultilineString(_)
            | Expr::Void
            | Expr::Error
            | Expr::Macro
            | Expr::LiftBinder
            | Expr::FunctionReturn(None) => {}
            // Operators whose result is a scalar/bool, and `arr.len()`'s folded
            // number. (A `+` on two closures does not type-check.)
            Expr::Binary(..) | Expr::Unary(..) | Expr::Is(..) | Expr::ArrayLen(..) => {}
            // Statements: their value is void.
            Expr::Assignment(..)
            | Expr::Destructure(..)
            | Expr::For(..)
            | Expr::ForEach(..)
            | Expr::Jump(_) => {}
            // Item and type references, not values with bodies. An
            // `ExternalFunction` named as a value HAS no Vilan body to enter,
            // and a bare (payload-less) enum variant carries nothing.
            Expr::Enum(_)
            | Expr::EnumVariant(..)
            | Expr::ExternalFunction(_)
            | Expr::Generic(_)
            | Expr::Impl(_)
            | Expr::Module(_)
            | Expr::Struct(_)
            | Expr::Trait(_) => {}
        }
    }

    fn value_bodies_of_if(
        &self,
        branch: &ExprIfBranch,
        bodies: &mut Vec<Id>,
        seen: &mut HashSet<Id>,
    ) {
        match branch {
            ExprIfBranch::If(_condition, (_statements, tail), else_) => {
                self.value_bodies(*tail, bodies, seen);
                if let Some(else_) = else_ {
                    self.value_bodies_of_if(else_, bodies, seen);
                }
            }
            ExprIfBranch::Else((_statements, tail)) => self.value_bodies(*tail, bodies, seen),
        }
    }

    /// Whether a call's subject names an enum variant constructor — `Some(x)`
    /// stores `x` rather than running anything.
    fn subject_is_a_variant_constructor(&self, subject_id: Id) -> bool {
        let target = match self.program.entity_map.get(&subject_id) {
            Some(Expr::Local(target_id)) => *target_id,
            _ => subject_id,
        };
        matches!(
            self.program.entity_map.get(&target),
            Some(Expr::EnumVariant(..))
        )
    }

    /// `start` plus every unit it reaches by DIRECT calls. The def chain uses
    /// it: a value that came out of `make()` may be a closure `make` created,
    /// or one created by anything `make` itself called. Indirect and dispatched
    /// calls are not followed here — an over-approximation of the *result* is
    /// cheap, but chasing every trait candidate for a value's provenance is
    /// not, and the argument/subject pass in [`Self::entered_by`] already covers
    /// the shapes that reach a body through a call's inputs.
    fn direct_call_closure(&self, start: Id) -> Vec<Id> {
        let mut reached = Vec::new();
        let mut seen = HashSet::default();
        seen.insert(start);
        let mut pending = vec![start];
        while let Some(unit) = pending.pop() {
            reached.push(unit);
            for call in self.graph.calls_of(unit) {
                if let CallTarget::Function(callee) | CallTarget::Closure(callee) = call.target {
                    if seen.insert(callee) {
                        pending.push(callee);
                    }
                }
            }
        }
        reached
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn relation(edges: &[(u32, &[u32])]) -> IndexMap<Id, Vec<Id>> {
        edges
            .iter()
            .map(|(binding, dependencies)| {
                (
                    Id(*binding),
                    dependencies.iter().copied().map(Id).collect::<Vec<_>>(),
                )
            })
            .collect()
    }

    fn order(edges: &[(u32, &[u32])]) -> Vec<u32> {
        let dependencies = relation(edges);
        let bindings: Vec<Id> = dependencies.keys().copied().collect();
        canonical_topological_order(&bindings, &dependencies)
            .into_iter()
            .map(|id| id.0)
            .collect()
    }

    #[test]
    fn unrelated_bindings_keep_canonical_order() {
        assert_eq!(order(&[(1, &[]), (2, &[]), (3, &[])]), vec![1, 2, 3]);
    }

    #[test]
    fn input_order_does_not_leak_into_the_result() {
        // The same relation, enumerated backwards: the canonical key decides,
        // never the vector.
        assert_eq!(order(&[(3, &[]), (1, &[]), (2, &[])]), vec![1, 2, 3]);
    }

    #[test]
    fn a_dependency_is_emitted_before_its_dependent() {
        // The zeta/alpha shape: the lower-keyed binding depends on the higher.
        assert_eq!(order(&[(1, &[2]), (2, &[])]), vec![2, 1]);
    }

    #[test]
    fn only_the_forced_pair_moves() {
        // 1 waits for 3; 2 is unordered and so goes first — the greedy smallest
        // available choice, not "shift the block".
        assert_eq!(order(&[(1, &[3]), (2, &[]), (3, &[])]), vec![2, 3, 1]);
    }

    #[test]
    fn transitive_chains_order_end_to_end() {
        assert_eq!(order(&[(1, &[2]), (2, &[3]), (3, &[])]), vec![3, 2, 1]);
    }

    #[test]
    fn a_diamond_orders_its_shared_dependency_first() {
        // 1 needs 2 and 3; both need 4.
        assert_eq!(
            order(&[(1, &[2, 3]), (2, &[4]), (3, &[4]), (4, &[])]),
            vec![4, 2, 3, 1]
        );
    }

    #[test]
    fn a_self_dependency_is_its_own_component() {
        // `let A = A + 1` — a 1-cycle. Nothing else depends on it, so it keeps
        // its canonical position; only its own initialization is doomed.
        assert_eq!(order(&[(1, &[1]), (2, &[])]), vec![1, 2]);
    }

    #[test]
    fn a_cycle_does_not_displace_unrelated_bindings() {
        assert_eq!(order(&[(1, &[]), (2, &[3]), (3, &[2])]), vec![1, 2, 3]);
    }

    #[test]
    fn a_binding_downstream_of_a_cycle_still_orders_after_it() {
        // THE blocker-2 property. 4 depends on the 2/3 cycle; that edge is
        // RECORDED, so it must be honored — appending the undrained remainder
        // in canonical order would emit 4 in position 4 by luck here, so the
        // discriminating case is the next test.
        assert_eq!(
            order(&[(1, &[]), (2, &[3]), (3, &[2]), (4, &[3])]),
            vec![1, 2, 3, 4]
        );
    }

    #[test]
    fn a_low_keyed_binding_downstream_of_a_cycle_moves_after_it() {
        // The discriminating case: 1 depends on the 2/3 cycle and sorts FIRST
        // canonically. Canonical-append would emit `1, 2, 3` — 1 reading a
        // binding that has not initialized. The condensation emits `2, 3, 1`.
        assert_eq!(order(&[(1, &[2]), (2, &[3]), (3, &[2])]), vec![2, 3, 1]);
    }

    #[test]
    fn a_cycle_and_its_downstream_still_respect_unrelated_canonical_order() {
        // 1 downstream of the {3,4} cycle, 2 unrelated: 2 is free, so it goes
        // first (smallest available), then the cycle, then 1.
        assert_eq!(
            order(&[(1, &[3]), (2, &[]), (3, &[4]), (4, &[3])]),
            vec![2, 3, 4, 1]
        );
    }

    #[test]
    fn a_three_binding_cycle_condenses_to_one_component() {
        assert_eq!(
            order(&[(1, &[2]), (2, &[3]), (3, &[1]), (4, &[1])]),
            vec![1, 2, 3, 4]
        );
    }

    #[test]
    fn a_dependency_outside_the_binding_set_is_ignored() {
        // Defensive: the relation only names module-level bindings, but a stray
        // id must not deadlock the sort.
        assert_eq!(order(&[(1, &[99]), (2, &[])]), vec![1, 2]);
    }

    #[test]
    fn a_deep_chain_does_not_overflow_the_stack() {
        // The component walk is iterative; a 20 000-long chain would blow a
        // recursive Tarjan.
        let depth: u32 = 20_000;
        let mut dependencies: IndexMap<Id, Vec<Id>> = IndexMap::new();
        for step in 1..=depth {
            let next = if step == depth {
                Vec::new()
            } else {
                vec![Id(step + 1)]
            };
            dependencies.insert(Id(step), next);
        }
        let bindings: Vec<Id> = dependencies.keys().copied().collect();
        let order = canonical_topological_order(&bindings, &dependencies);
        assert_eq!(order.len(), depth as usize);
        assert_eq!(
            order[0],
            Id(depth),
            "the deepest dependency initializes first"
        );
        assert_eq!(order[depth as usize - 1], Id(1));
    }

    // --- The cycle diagnostic's rendering machinery (S2) --------------------
    //
    // The message is built from a program, but the two decisions that make it
    // deterministic — which component counts as a cycle, and which round trip
    // the `via` chain shows — are pure functions of the relation, so they pin
    // here where every shape is expressible.

    fn cycle_through(component: &[u32], edges: &[(u32, &[u32])]) -> Vec<u32> {
        let dependencies = relation(edges);
        let component: Vec<Id> = component.iter().copied().map(Id).collect();
        shortest_cycle(&component, &dependencies)
            .into_iter()
            .map(|id| id.0)
            .collect()
    }

    #[test]
    fn a_self_edge_is_a_cycle_and_a_lone_binding_is_not() {
        let dependencies = relation(&[(1, &[1]), (2, &[])]);
        assert!(is_cycle(&[Id(1)], &dependencies));
        assert!(!is_cycle(&[Id(2)], &dependencies));
    }

    #[test]
    fn a_self_cycles_chain_is_the_binding_twice() {
        assert_eq!(cycle_through(&[1], &[(1, &[1])]), vec![1, 1]);
    }

    #[test]
    fn a_two_binding_cycles_chain_is_the_round_trip() {
        assert_eq!(
            cycle_through(&[1, 2], &[(1, &[2]), (2, &[1])]),
            vec![1, 2, 1]
        );
    }

    #[test]
    fn a_longer_cycles_chain_names_every_step() {
        assert_eq!(
            cycle_through(&[1, 2, 3], &[(1, &[2]), (2, &[3]), (3, &[1])]),
            vec![1, 2, 3, 1]
        );
    }

    #[test]
    fn the_chain_is_the_shortest_round_trip_through_the_first_member() {
        // One component, two ways home: 1 → 2 → 1 and 1 → 3 → 4 → 1. The chain
        // shows the short one, and always starts at the canonically first
        // member, so the rendered witness never depends on edge order.
        assert_eq!(
            cycle_through(
                &[1, 2, 3, 4],
                &[(1, &[2, 3]), (2, &[1]), (3, &[4]), (4, &[1])]
            ),
            vec![1, 2, 1]
        );
    }

    #[test]
    fn a_component_wider_than_its_chain_still_renders_one_round_trip() {
        // 1 ↔ 2 with 3 hanging off 2 and back into 1: all three are one
        // component (each reaches the others), but no single round trip visits
        // all three, so the chain shows one and the message names the rest.
        assert_eq!(
            cycle_through(&[1, 2, 3], &[(1, &[2]), (2, &[1, 3]), (3, &[1])]),
            vec![1, 2, 1]
        );
    }

    #[test]
    fn participants_read_as_a_sentence() {
        let names = |count: usize| {
            join_and(
                &["`A`", "`B`", "`C`"][..count]
                    .iter()
                    .map(|name| name.to_string())
                    .collect::<Vec<_>>(),
            )
        };
        assert_eq!(names(1), "`A`");
        assert_eq!(names(2), "`A` and `B`");
        assert_eq!(names(3), "`A`, `B` and `C`");
    }
}
