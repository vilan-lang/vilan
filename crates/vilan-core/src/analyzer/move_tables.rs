//! M149 (Order 53) — the move scan's whole-program helper sets at item grain.
//!
//! `check_resource_moves` scans only the LIVE bodies since M19 T1 (a reused
//! module's or served item's verdicts are replayed), and `plan_resource_drops`
//! plans every enrolled body; both read sets the whole program contributes
//! to — every binding whose type is a resource, every place whose value is
//! one, every loaned pattern capture, every `is` test's refinement, every
//! value crossing at a by-value return, every assignment that can overwrite
//! a live resource. Each was derived by a sweep of the whole program on every
//! analysis: 9 ms of a served kolt keystroke's checks phase, two thirds of it
//! `collect_resource_value_places` alone (a type query per place of 150k
//! entities).
//!
//! Every member of those sets is a fact about one region's own entities — a
//! binding's declared type, a place's value type, a pattern's subject form, a
//! return leaf of the region's own function — so each set is the union of
//! per-region CONTRIBUTIONS ([`MoveRows`], a class D row family in
//! `ModuleTables`), restored for a reused module or served item and walked for
//! the family's live lane, the shape `held_tables.rs` gives the held passes.
//! Two of the seven take a DECISION over every candidate on every analysis
//! instead of a recorded verdict, because their verdict reads a fact another
//! region can move:
//!
//! - the partial-move roots (B469) ask whether the root's type holds a `Drop`
//!   anywhere inside it — against the Drop-IMPLEMENTING nominal set, which an
//!   impl in any module can grow — and whether the root is module-level, a
//!   loaned capture or a view. They are decided over the folded resource
//!   places' field-chain candidates (a handful: the resource places reached
//!   through a field), so no verdict is ever served and no digest of the
//!   nominal set is needed to gate one;
//! - an assignment overwrites a live resource (R2) through a NAME only when
//!   that name's binding is a resource binding, which may be another
//!   module's: the candidates are rows, the binding test is the fold's.
//!
//! The family is filed only by an analysis that ran the scan (a program with
//! no resource binding returns before deriving the other six), so a slice
//! says whether it carries the rows (`row_families.rs`).

use crate::fx::{FxHashMap as HashMap, FxHashSet as HashSet};
use crate::id::Id;
use crate::type_::TypeId;

use super::row_families::RowFamily;
use super::{Analyzer, Expr, IsRefinement};

/// One region's (or one analysis's live) contributions to the move scan's
/// helper sets. Every row's FIRST id is the region's own — the partition's
/// key — and every id a row carries is an entity of the same region.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct MoveRows {
    /// The bindings (variables and parameters) whose type is a resource.
    pub bindings: Vec<Id>,
    /// The place expressions whose value type is a resource.
    pub places: Vec<Id>,
    /// `(capture, subject)` — a pattern capture bound by a LOANED subject.
    pub loaned: Vec<(Id, Id)>,
    /// `(test, subject, payload free when true, payload free when false)` —
    /// an `x is V(..)` test's refinement (B67); the binding is the subject's
    /// `Local`, read back on restore.
    pub is_tests: Vec<(Id, Id, bool, bool)>,
    /// The places a by-value return hands back through a view leaf (B109).
    pub crossings: Vec<Id>,
    /// `(assignment, through a name)` — an assignment whose target can hold
    /// a live resource: a writable view NAME (decided by the fold: is its
    /// binding a resource binding) or a resource COMPONENT place.
    pub overwrites: Vec<(Id, bool)>,
}

/// The fold: the restored rows and the live ones, in the shapes the scan and
/// the drop plan read.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct MoveSets {
    pub bindings: HashSet<Id>,
    pub places: HashSet<Id>,
    pub loaned: HashMap<Id, Id>,
    pub is_refinements: HashMap<Id, IsRefinement>,
    pub crossings: HashSet<Id>,
    pub overwrites: Vec<(Id, bool)>,
}

impl MoveRows {
    pub(super) fn bytes(&self) -> usize {
        let id = std::mem::size_of::<Id>();
        (self.bindings.len() + self.places.len() + self.crossings.len()) * id
            + self.loaned.len() * 2 * id
            + self.is_tests.len() * std::mem::size_of::<(Id, Id, bool, bool)>()
            + self.overwrites.len() * std::mem::size_of::<(Id, bool)>()
    }

    /// How many rows there are.
    pub(super) fn len(&self) -> usize {
        self.bindings.len()
            + self.places.len()
            + self.loaned.len()
            + self.is_tests.len()
            + self.crossings.len()
            + self.overwrites.len()
    }

    /// Whether every id the rows carry satisfies `fits`.
    pub(super) fn fits(&self, fits: &dyn Fn(Id) -> bool) -> bool {
        self.bindings
            .iter()
            .chain(&self.places)
            .chain(&self.crossings)
            .all(|id| fits(*id))
            && self.loaned.iter().all(|(a, b)| fits(*a) && fits(*b))
            && self
                .is_tests
                .iter()
                .all(|(a, b, _, _)| fits(*a) && fits(*b))
            && self.overwrites.iter().all(|(a, _)| fits(*a))
    }

    /// The rows with every id re-based through `rebase`; a row naming an id
    /// it refuses is dropped (a filed record never carries one).
    pub(super) fn rebased(&self, rebase: &dyn Fn(Id) -> Option<Id>) -> MoveRows {
        let ids = |rows: &[Id]| rows.iter().filter_map(|id| rebase(*id)).collect();
        MoveRows {
            bindings: ids(&self.bindings),
            places: ids(&self.places),
            loaned: self
                .loaned
                .iter()
                .filter_map(|(a, b)| Some((rebase(*a)?, rebase(*b)?)))
                .collect(),
            is_tests: self
                .is_tests
                .iter()
                .filter_map(|(a, b, when_true, when_false)| {
                    Some((rebase(*a)?, rebase(*b)?, *when_true, *when_false))
                })
                .collect(),
            crossings: ids(&self.crossings),
            overwrites: self
                .overwrites
                .iter()
                .filter_map(|(a, name)| Some((rebase(*a)?, *name)))
                .collect(),
        }
    }

    pub(super) fn extend(&mut self, other: &MoveRows) {
        self.bindings.extend(other.bindings.iter().copied());
        self.places.extend(other.places.iter().copied());
        self.loaned.extend(other.loaned.iter().copied());
        self.is_tests.extend(other.is_tests.iter().copied());
        self.crossings.extend(other.crossings.iter().copied());
        self.overwrites.extend(other.overwrites.iter().copied());
    }

    /// The rows partitioned by their key id through `slice_of`.
    pub(super) fn partition_into(
        &self,
        slice_of: &dyn Fn(Id) -> Option<u32>,
        into: &mut HashMap<u32, super::ModuleTables>,
    ) {
        for id in &self.bindings {
            if let Some(slice) = slice_of(*id) {
                into.entry(slice).or_default().moves.bindings.push(*id);
            }
        }
        for id in &self.places {
            if let Some(slice) = slice_of(*id) {
                into.entry(slice).or_default().moves.places.push(*id);
            }
        }
        for row in &self.loaned {
            if let Some(slice) = slice_of(row.0) {
                into.entry(slice).or_default().moves.loaned.push(*row);
            }
        }
        for row in &self.is_tests {
            if let Some(slice) = slice_of(row.0) {
                into.entry(slice).or_default().moves.is_tests.push(*row);
            }
        }
        for id in &self.crossings {
            if let Some(slice) = slice_of(*id) {
                into.entry(slice).or_default().moves.crossings.push(*id);
            }
        }
        for row in &self.overwrites {
            if let Some(slice) = slice_of(row.0) {
                into.entry(slice).or_default().moves.overwrites.push(*row);
            }
        }
    }
}

impl<'src> Analyzer<'src> {
    /// The live lane's resource BINDINGS — every variable and parameter of a
    /// region whose move rows are not restored, whose type is a resource.
    /// `&mut` because `type_is_resource` memoizes; empty outright in a world
    /// that declares no resource (no binding can be one).
    ///
    /// **M38, and what MEASURING it decided.** The item asked for a
    /// per-nominal "can any instantiation of this be a resource" pre-filter
    /// (M28's `resource_reaching_nominals` / `type_reaches_resource`) ahead of
    /// the field recursion, on the reading that `type_is_resource` over every
    /// variable and parameter was ~100 ms on kolt's browser entry. It was
    /// built, it is answer-identical (M28 proves `type_is_resource(T)` implies
    /// `type_reaches_resource(T)`), and it was measured: **+2,312,262
    /// instructions on `vilan check src/client.vl` over the full kolt copy —
    /// a LOSS, not a win.** It could not be otherwise on a program that
    /// declares a resource: the filter substitutes one recursive walk over the
    /// distinct `TypeId`s (`classify_resource`) for another over the same ones
    /// (`type_mentions_nominal`), and `resource_classification` — the
    /// analyzer's persistent memo — had already made the first cost once per
    /// distinct type rather than once per binding. The filter is a wash by
    /// construction, and pays for the nominals closure on top. It is not here.
    ///
    /// **What IS here is the gate the filter was carrying.** A world where
    /// nothing is declared `resource` has no resource type at all —
    /// containment bottoms out at a declared leaf — so no binding can be one
    /// and the walk below is skipped outright. `check_resource_moves`
    /// tested that only AFTER them, by finding the result empty: two whole-map
    /// walks and a `type_is_resource` call per binding, on every corpus
    /// program that owns no resource, for an answer two `.any()` scans give.
    pub(super) fn live_resource_bindings(&mut self) -> Vec<Id> {
        if !self.declares_a_resource() {
            return Vec::new();
        }
        let live = self.live_families.of(RowFamily::Moves);
        let mut candidates: Vec<(Id, TypeId)> =
            super::live_ranges::LiveEntries::over(&self.variables, live)
                .map(|(id, variable)| (*id, variable.type_id))
                .collect();
        candidates.extend(
            super::live_ranges::LiveEntries::over(&self.parameters, live)
                .map(|(id, parameter)| (*id, parameter.type_id)),
        );
        let mut bindings = Vec::new();
        for (id, type_id) in candidates {
            // M134: the planner's mints for a binding belong to its item.
            self.id_windows.set_anchor(id);
            if self.type_is_resource(type_id) {
                bindings.push(id);
            }
        }
        bindings
    }

    /// The live lane's contributions to the six sets beside the bindings:
    /// the resource places, the loaned captures, the `is` refinements, the
    /// value crossings and the overwrite candidates — each read off the
    /// expression (or the function) that carries it, as the whole-program
    /// helpers did (`collect_resource_value_places`,
    /// `collect_loaned_pattern_captures`, `collect_is_refinements`,
    /// `compute_return_value_crossings`, `collect_place_overwrites`).
    pub(super) fn live_move_rows(&mut self, rows: &mut MoveRows) {
        let live = self.live_families.of(RowFamily::Moves);
        let mut places: Vec<Id> = Vec::new();
        let mut tests: Vec<Id> = Vec::new();
        let mut assignments: Vec<(Id, Id)> = Vec::new();
        for (id, expr) in super::live_ranges::LiveEntries::over(&self.expr_id_to_expr_map, live) {
            match expr {
                Expr::Local(_)
                | Expr::Field(_, _, _)
                | Expr::TupleIndex(_, _, _)
                | Expr::Index(_, _) => places.push(*id),
                Expr::Is(_, _) => tests.push(*id),
                Expr::Assignment(target_id, _) => assignments.push((*id, *target_id)),
                _ => {}
            }
            self.loaned_captures_of(expr, &mut rows.loaned);
        }
        let mut live_places: HashSet<Id> = HashSet::default();
        for id in places {
            // M134: a place's classification mints belong to its item.
            self.id_windows.set_anchor(id);
            if let Some(place_type) = self.place_value_type(id) {
                let type_id = place_type.get_type_id(self);
                if self.type_is_resource(type_id) {
                    rows.places.push(id);
                    live_places.insert(id);
                }
            }
        }
        let refinements =
            self.collect_is_refinements(Some(&tests), &HashSet::default(), &mut HashMap::default());
        for (test, refinement) in refinements {
            if let Some(Expr::Is(subject, _)) = self.expr_id_to_expr_map.get(&test) {
                rows.is_tests.push((
                    test,
                    *subject,
                    refinement.payload_free_when_true,
                    refinement.payload_free_when_false,
                ));
            }
        }
        let live = self.live_families.of(RowFamily::Moves).clone();
        rows.crossings = self
            .compute_return_value_crossings(&|function_id: Id| live.covers(function_id.0))
            .into_iter()
            .collect();
        for (assignment_id, target_id) in assignments {
            if !self.is_place_expr(target_id) {
                continue;
            }
            match self.expr_id_to_expr_map.get(&target_id) {
                // The loan half: a writable view NAME; whether its binding is
                // a resource binding is the fold's question.
                Some(Expr::Local(_)) => {
                    if self.assignment_target_is_view(target_id)
                        && self.subject_is_writable_view(target_id)
                    {
                        rows.overwrites.push((assignment_id, true));
                    }
                }
                // The component half: the target's own value type decides,
                // and the target is this region's place.
                _ => {
                    if live_places.contains(&target_id) {
                        rows.overwrites.push((assignment_id, false));
                    }
                }
            }
        }
    }

    /// The fold of the restored rows and `live`.
    pub(super) fn fold_moves(&self, live: &MoveRows) -> MoveSets {
        let restored = &self.restored_tables.moves;
        let all = [restored, live];
        let mut sets = MoveSets::default();
        for rows in all {
            sets.bindings.extend(rows.bindings.iter().copied());
            sets.places.extend(rows.places.iter().copied());
            sets.loaned.extend(rows.loaned.iter().copied());
            for (test, subject, when_true, when_false) in &rows.is_tests {
                if let Some(Expr::Local(binding)) = self.expr_id_to_expr_map.get(subject) {
                    sets.is_refinements.insert(
                        *test,
                        IsRefinement {
                            binding: *binding,
                            payload_free_when_true: *when_true,
                            payload_free_when_false: *when_false,
                        },
                    );
                }
            }
            sets.crossings.extend(rows.crossings.iter().copied());
            sets.overwrites.extend(rows.overwrites.iter().copied());
        }
        sets
    }

    /// The drop plan's overwrite decision over every candidate (R2): a
    /// component candidate always (its place is a resource place), a name
    /// candidate when its binding is a resource binding — with the target's
    /// value type, read now.
    pub(super) fn decide_place_overwrites(&self, sets: &MoveSets) -> HashMap<Id, TypeId> {
        let mut overwrites: HashMap<Id, TypeId> = HashMap::default();
        for (assignment_id, through_name) in &sets.overwrites {
            let Some(Expr::Assignment(target_id, _)) = self.expr_id_to_expr_map.get(assignment_id)
            else {
                continue;
            };
            // M134: the target type's mints belong to the assignment's item.
            self.id_windows.set_anchor(*assignment_id);
            if *through_name {
                let Some(Expr::Local(binding)) = self.expr_id_to_expr_map.get(target_id) else {
                    continue;
                };
                if !sets.bindings.contains(binding) {
                    continue;
                }
            }
            if let Some(type_id) = self.place_value_type_id(*target_id) {
                overwrites.insert(*assignment_id, type_id);
            }
        }
        overwrites
    }
}
