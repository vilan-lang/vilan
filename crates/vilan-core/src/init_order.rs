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
//! S1 does not diagnose them, but it must not let one corrupt the order of
//! bindings that are merely *downstream* of it. The sort therefore runs over
//! the **condensation**: strongly connected components first, then a
//! topological order of the resulting DAG (which is acyclic by construction, so
//! it always drains completely). Only the members of a genuine cycle are
//! ordered arbitrarily — among themselves, canonically; everything that merely
//! depends on a cycle still orders after it. S2 turns exactly the
//! multi-member/self-looping components into the compile error.

use std::collections::{BTreeSet, HashMap, HashSet};

use indexmap::IndexMap;

use crate::analyzer::{Expr, ExprIfBranch, Program};
use crate::call_graph::{CallGraph, CallTarget, IndirectReason};
use crate::id::Id;

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
/// Keyed in canonical order, so a consumer that iterates it (S2's cycle
/// diagnostic) is deterministic too.
pub fn load_time_dependencies(program: &Program, graph: &CallGraph) -> IndexMap<Id, Vec<Id>> {
    let mut bindings = program.module_level_bindings();
    bindings.sort_by_key(canonical_key);
    let mut walk = LoadTimeWalk {
        program,
        graph,
        entered: HashMap::new(),
    };
    bindings
        .iter()
        .map(|binding| (*binding, walk.evaluated_globals(*binding)))
        .collect()
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
    let mut component_of: HashMap<Id, usize> = HashMap::new();
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
    let mut index_of: HashMap<Id, u32> = HashMap::new();
    let mut low_of: HashMap<Id, u32> = HashMap::new();
    let mut on_stack: HashSet<Id> = HashSet::new();
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

/// Walks the load-time relation, memoizing the part that is context-free.
struct LoadTimeWalk<'a, 'src> {
    program: &'a Program<'src>,
    graph: &'a CallGraph,
    /// Code unit → the units its execution ENTERS (its resolved callees, with
    /// dispatch and every function value passed through a call expanded). This
    /// depends only on the unit, never on which binding's initialization
    /// reached it, so it is computed once and shared by every binding's walk.
    entered: HashMap<Id, Vec<Id>>,
}

impl LoadTimeWalk<'_, '_> {
    /// The module-level bindings `binding`'s initializer evaluates at load
    /// time, ascending by canonical key. Includes `binding` itself when its
    /// initializer reads it (a 1-cycle) — the ordering pass must see that, not
    /// silently drop it.
    fn evaluated_globals(&mut self, binding: Id) -> Vec<Id> {
        let mut reads: BTreeSet<u32> = BTreeSet::new();
        let mut seen: HashSet<Id> = HashSet::new();
        seen.insert(binding);
        let mut pending = vec![binding];
        while let Some(unit) = pending.pop() {
            for (_reference, global) in self.graph.global_references_of(unit) {
                reads.insert(canonical_key(global));
            }
            for next in self.entered_by(unit) {
                if seen.insert(next) {
                    pending.push(next);
                }
            }
        }
        reads.into_iter().map(Id).collect()
    }

    /// The code units executing `unit` enters. Only CALLS are followed: a
    /// closure the unit merely creates is not entered, which is the §2 rule
    /// that keeps mutually-recursive module closures legal.
    ///
    /// A binding's initializer and a function/closure body are one vocabulary
    /// here — the call graph files a binding's calls under `initializer_calls`
    /// and a node's under `calls`, and each is empty for the other kind, so
    /// chaining them reads whichever applies.
    fn entered_by(&mut self, unit: Id) -> Vec<Id> {
        if let Some(cached) = self.entered.get(&unit) {
            return cached.clone();
        }
        let graph = self.graph;
        let mut entered = Vec::new();
        for call in graph
            .calls_of(unit)
            .iter()
            .chain(graph.initializer_calls_of(unit))
        {
            match call.target {
                CallTarget::Function(callee) | CallTarget::Closure(callee) => entered.push(callee),
                // An extern is a leaf with no Vilan body; a variant constructor
                // builds a value and calls nothing. (Neither is a dead end for
                // the values passed to it — see below.)
                CallTarget::External(_) | CallTarget::Variant(_) => {}
                // Resolved by the value pass below, which subsumes it.
                CallTarget::Indirect(IndirectReason::Value) => {}
                // A generic/trait dispatch follows the same over-approximation
                // async inference and platform coloring use: every candidate.
                // §5(b) records the false-cycle risk this carries.
                CallTarget::Indirect(_) => entered.extend(crate::async_infer::dispatch_candidates(
                    self.program,
                    call.call_id,
                )),
            }
            // Every function VALUE this call can hand to its callee is entered
            // too: a call that runs at load may invoke what it was given, and
            // the callee's own signature is no guarantee it does not (an
            // `[extern]` helper — `List::map` lowers to one — has no Vilan body
            // to walk at all). The receiver of a method call is argument 0, so
            // `HOLDER.run()` reaches the closures `HOLDER` holds through this
            // too. Conservative: it only ever ADDS edges.
            if let Some(function_call) = self.program.function_calls.get(&call.call_id) {
                let mut seen = HashSet::new();
                self.value_bodies(function_call.subject_id, &mut entered, &mut seen);
                for argument in &function_call.argument_ids {
                    self.value_bodies(*argument, &mut entered, &mut seen);
                }
            }
        }
        entered.sort_by_key(canonical_key);
        entered.dedup();
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
        let mut seen = HashSet::new();
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
}
