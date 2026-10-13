//! M110 S7b (Order 52, M142) — the six HELD whole-program table passes at
//! item grain.
//!
//! The drop extents, the shared cells (B267), the written roots, the shared
//! reads, the shared place `let`s (M90) and the capture plan (B53) were the
//! rows S2b HELD: each sweeps every expression of the program and decides
//! from whole-program inputs — the union-find over every `Shared` slot, the
//! set of every written root, every value seam's root — so a per-item
//! record of their OUTPUT could go stale through an edge the dirty closure
//! does not carry (an item that mutates a cell through a struct's field, or
//! writes a module-level `mut` binding, moves another item's elision). This
//! module splits each pass in two:
//!
//! - a **contribution** per region — the facts the pass reads off the
//!   region's own expressions (its written roots, its cell facts, its seam
//!   roots, its candidate `let`s and captures, its hand-ons), every one a
//!   function of the region's text and resolve, keyed by an id the region
//!   minted and so a class D row ([`HeldRows`], in `ModuleTables`): restored
//!   for a reused module or served item, walked for the live ranges
//!   (`live_held_entries`);
//! - the **fold** — the union of every contribution — and the decisions,
//!   taken over EVERY candidate on every analysis, so the whole-program
//!   inputs are exact and no output row is ever served.
//!
//! A region whose rows would carry an id of another re-walked window (a
//! hot module-level binding written from another item) files no held rows
//! (`held_recorded` false) and stays live for these passes: its window is
//! outside the `held_ranges` the walk skips.

use crate::fx::{FxHashMap as HashMap, FxHashSet as HashSet};
use crate::id::Id;

use super::{Analyzer, CellSlot, Expr, SharedCells, WrittenRoots};

/// One region's (or one analysis's live) contributions to the held passes.
/// Every row's FIRST id is the region's own — the partition's key.
#[derive(Clone, Debug, Default)]
pub(super) struct HeldRows {
    /// `(writer expression, root, in place)` — the written roots.
    pub written: Vec<(Id, Id, bool)>,
    /// `(key expression, slot, slot)` — a union of two cell slots.
    pub cell_unions: Vec<(Id, CellSlot, CellSlot)>,
    /// `(the read call, its slot)` and `(the write call, its slot, rebinds)`.
    pub cell_reads: Vec<(Id, CellSlot)>,
    pub cell_writes: Vec<(Id, CellSlot, bool)>,
    /// `(leaf, root)` — every value seam's root; the ones whose leaf passes
    /// the place-`let` filter; the capture plan's own seams.
    pub seam_roots: Vec<(Id, Id)>,
    pub plain_seam_roots: Vec<(Id, Id)>,
    pub capture_seam_roots: Vec<(Id, Id)>,
    /// `(binding, initializer value, projection root)` — M90's candidates.
    pub place_lets: Vec<(Id, Id, Id)>,
    /// `(position, binding)` — a binding handed on WHOLE at a position.
    pub handed_on: Vec<(Id, Id)>,
    /// `(the `let` expression, the binding, the `Shared.read()` call)`.
    pub shared_reads: Vec<(Id, Id, Id)>,
    /// `(capture, subject)` — the capture plan's candidates.
    pub captures: Vec<(Id, Id)>,
}

impl HeldRows {
    pub(super) fn bytes(&self) -> usize {
        let id = std::mem::size_of::<Id>();
        (self.written.len() + self.cell_writes.len() + self.place_lets.len()) * 3 * id
            + (self.cell_reads.len()
                + self.seam_roots.len()
                + self.plain_seam_roots.len()
                + self.capture_seam_roots.len()
                + self.handed_on.len()
                + self.captures.len())
                * 2
                * id
            + self.shared_reads.len() * 3 * id
            + self.cell_unions.len() * std::mem::size_of::<(Id, CellSlot, CellSlot)>()
    }

    /// Whether every id the rows carry satisfies `fits` — the record's
    /// "own or the prefix's" rule.
    pub(super) fn fits(&self, fits: &dyn Fn(Id) -> bool) -> bool {
        let slot = |slot: &CellSlot| match slot {
            CellSlot::Binding(id) | CellSlot::Field(id, _) => fits(*id),
            CellSlot::Unknown => true,
        };
        self.written.iter().all(|(a, b, _)| fits(*a) && fits(*b))
            && self
                .cell_unions
                .iter()
                .all(|(a, b, c)| fits(*a) && slot(b) && slot(c))
            && self.cell_reads.iter().all(|(a, b)| fits(*a) && slot(b))
            && self.cell_writes.iter().all(|(a, b, _)| fits(*a) && slot(b))
            && self
                .seam_roots
                .iter()
                .chain(&self.plain_seam_roots)
                .chain(&self.capture_seam_roots)
                .chain(&self.handed_on)
                .chain(&self.captures)
                .all(|(a, b)| fits(*a) && fits(*b))
            && self
                .place_lets
                .iter()
                .chain(&self.shared_reads)
                .all(|(a, b, c)| fits(*a) && fits(*b) && fits(*c))
    }

    /// The rows with every id re-based through `rebase` — an id it refuses
    /// drops the row (a record never carries one, by [`Self::fits`]).
    pub(super) fn rebased(&self, rebase: &dyn Fn(Id) -> Option<Id>) -> HeldRows {
        let slot = |slot: &CellSlot| -> Option<CellSlot> {
            Some(match slot {
                CellSlot::Binding(id) => CellSlot::Binding(rebase(*id)?),
                CellSlot::Field(id, index) => CellSlot::Field(rebase(*id)?, *index),
                CellSlot::Unknown => CellSlot::Unknown,
            })
        };
        let pair = |rows: &[(Id, Id)]| -> Vec<(Id, Id)> {
            rows.iter()
                .filter_map(|(a, b)| Some((rebase(*a)?, rebase(*b)?)))
                .collect()
        };
        HeldRows {
            written: self
                .written
                .iter()
                .filter_map(|(a, b, in_place)| Some((rebase(*a)?, rebase(*b)?, *in_place)))
                .collect(),
            cell_unions: self
                .cell_unions
                .iter()
                .filter_map(|(a, b, c)| Some((rebase(*a)?, slot(b)?, slot(c)?)))
                .collect(),
            cell_reads: self
                .cell_reads
                .iter()
                .filter_map(|(a, b)| Some((rebase(*a)?, slot(b)?)))
                .collect(),
            cell_writes: self
                .cell_writes
                .iter()
                .filter_map(|(a, b, rebinds)| Some((rebase(*a)?, slot(b)?, *rebinds)))
                .collect(),
            seam_roots: pair(&self.seam_roots),
            plain_seam_roots: pair(&self.plain_seam_roots),
            capture_seam_roots: pair(&self.capture_seam_roots),
            place_lets: self
                .place_lets
                .iter()
                .filter_map(|(a, b, c)| Some((rebase(*a)?, rebase(*b)?, rebase(*c)?)))
                .collect(),
            handed_on: pair(&self.handed_on),
            shared_reads: self
                .shared_reads
                .iter()
                .filter_map(|(a, b, c)| Some((rebase(*a)?, rebase(*b)?, rebase(*c)?)))
                .collect(),
            captures: pair(&self.captures),
        }
    }

    pub(super) fn extend(&mut self, other: &HeldRows) {
        self.written.extend(other.written.iter().copied());
        self.cell_unions.extend(other.cell_unions.iter().cloned());
        self.cell_reads.extend(other.cell_reads.iter().cloned());
        self.cell_writes.extend(other.cell_writes.iter().cloned());
        self.seam_roots.extend(other.seam_roots.iter().copied());
        self.plain_seam_roots
            .extend(other.plain_seam_roots.iter().copied());
        self.capture_seam_roots
            .extend(other.capture_seam_roots.iter().copied());
        self.place_lets.extend(other.place_lets.iter().copied());
        self.handed_on.extend(other.handed_on.iter().copied());
        self.shared_reads.extend(other.shared_reads.iter().copied());
        self.captures.extend(other.captures.iter().copied());
    }

    /// The rows partitioned by their key id through `slice_of`, into the
    /// slices' own held rows.
    pub(super) fn partition_into(
        &self,
        slice_of: &dyn Fn(Id) -> Option<u32>,
        into: &mut HashMap<u32, super::ModuleTables>,
    ) {
        macro_rules! file {
            ($rows:ident) => {
                for row in &self.$rows {
                    if let Some(slice) = slice_of(row.0) {
                        into.entry(slice).or_default().held.$rows.push(row.clone());
                    }
                }
            };
        }
        file!(written);
        file!(cell_unions);
        file!(cell_reads);
        file!(cell_writes);
        file!(seam_roots);
        file!(plain_seam_roots);
        file!(capture_seam_roots);
        file!(place_lets);
        file!(handed_on);
        file!(shared_reads);
        file!(captures);
    }
}

/// The fold: every contribution of this analysis, restored and live, in
/// the shapes the decisions read.
pub(super) struct HeldFold {
    pub written: WrittenRoots,
    pub cells: SharedCells,
    pub seam_roots: HashSet<Id>,
    pub plain_seam_roots: HashSet<Id>,
    pub capture_seam_roots: HashSet<Id>,
    pub place_lets: Vec<(Id, Id, Id)>,
    pub handed_on: HashSet<Id>,
    pub shared_reads: Vec<(Id, Id, Id)>,
    pub captures: Vec<(Id, Id)>,
}

impl<'src> Analyzer<'src> {
    /// The LIVE regions' contributions — one pass over the live expressions,
    /// functions, closures, modules, parameters and bindings
    /// (`live_held_entries`), the facts of
    /// `collect_written_roots`, `compute_shared_cells`' passes 1–3,
    /// `value_seam_leaves`, `compute_shared_place_lets`' candidates and
    /// hand-ons, `compute_shared_read_bindings`' candidates and
    /// `compute_capture_clone_sites`' phase 1, each read off the expression
    /// that carries it.
    pub(super) fn collect_held_rows(&self) -> HeldRows {
        let mut rows = HeldRows::default();
        let shared = self.shared_members();
        let shared_struct_id = self.primitive_struct_ids.get("Shared").copied();
        // --- the cell facts' per-region state (`compute_shared_cells`) ---
        let mut clone_receivers: HashMap<Id, Id> = HashMap::default();
        let mut new_calls: HashSet<Id> = HashSet::default();
        let mut write_receivers: Vec<(Id, Id)> = Vec::new();
        let mut benign: HashSet<Id> = HashSet::default();
        let mut rebinding: HashSet<Id> = HashSet::default();
        let mut read_calls: Vec<Id> = Vec::new();
        let mut moved: Vec<(Id, CellSlot, Id)> = Vec::new();
        let mut moved_targets: Vec<(Id, Id, Id)> = Vec::new();
        let mut placed: HashSet<Id> = HashSet::default();
        let mut handle_positions: Vec<Id> = Vec::new();
        // --- the place-`let` candidates' bindings, for the hand-on rows ---
        let mut candidate_bindings: HashSet<Id> = HashSet::default();
        // Hand-on positions, raw: a call argument's loan-ness is decided
        // after the walk, only for a binding that matters, with the
        // callee's conventions asked once per callee.
        let module_level = self.module_level_binding_ids();
        let mut hand_on_positions: Vec<(Id, Id)> = Vec::new();
        let mut hand_on_arguments: Vec<(Id, Id, Id, usize)> = Vec::new();
        let mut conventions_memo: HashMap<Id, Option<Vec<super::Convention>>> = HashMap::default();
        let mut conventions_of =
            |analyzer: &Self, subject_id: Id| -> Option<Vec<super::Convention>> {
                conventions_memo
                    .entry(subject_id)
                    .or_insert_with(|| analyzer.callee_conventions(subject_id))
                    .clone()
            };
        // Seam leaves, raw: the place-`let` filter (a type query per leaf)
        // runs after the walk, only for a root that matters. The VALUE seams
        // (a function's or block's tail, a `ret` value) are expanded only
        // when something can ask for them — a candidate `let` of this walk,
        // or a module-level immutable aggregate binding anywhere (the one
        // root a candidate in ANOTHER region can have), as the base computed
        // them lazily; the capture plan's seams are always expanded.
        let mut seam_leaves: Vec<(Id, Id, bool)> = Vec::new();
        let mut value_seams: Vec<Id> = Vec::new();
        let module_aggregate_lets = module_level.iter().any(|binding| {
            self.variables.get(binding).is_some_and(|variable| {
                !variable.mutable && self.is_cloneable_aggregate(&variable.type_id.get_type(self))
            })
        });
        // --- every `let` with an initializer: a shared-read candidate when
        // the initializer turns out to be a `Shared.read()` call ---
        let mut let_rows: Vec<(Id, Id, Id)> = Vec::new();

        for (expr_id, expr) in self.live_held_entries(&self.expr_id_to_expr_map) {
            let expr_id = *expr_id;
            // Written roots (`collect_written_roots`).
            match expr {
                Expr::Assignment(target_id, _) => {
                    let rebinds = matches!(
                        self.expr_id_to_expr_map.get(target_id),
                        Some(Expr::Local(_))
                    ) && !self.subject_is_writable_view(*target_id);
                    if let Some(root) = self.place_root(*target_id) {
                        rows.written.push((expr_id, root, !rebinds));
                    }
                }
                Expr::Reference(operand_id, true) => {
                    if let Some(root) = self.place_root(*operand_id) {
                        rows.written.push((expr_id, root, true));
                    }
                }
                Expr::Call(call_id) => {
                    if let Some(function_call) = self.function_calls.get(call_id) {
                        match conventions_of(self, function_call.subject_id) {
                            Some(conventions) => {
                                for (index, argument_id) in
                                    function_call.argument_ids.iter().enumerate()
                                {
                                    if conventions.get(index).copied()
                                        == Some(super::Convention::RefMut)
                                        && let Some(root) = self.place_root(*argument_id)
                                    {
                                        rows.written.push((expr_id, root, true));
                                    }
                                }
                            }
                            None => {
                                for argument_id in &function_call.argument_ids {
                                    if let Some(root) = self.place_root(*argument_id) {
                                        rows.written.push((expr_id, root, true));
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            // Cell facts, pass 1.
            if let Some([new_ids, read_ids, write_ids, clone_ids]) = &shared {
                if let Expr::Assignment(target_id, _) = expr {
                    let named = match self.expr_id_to_expr_map.get(target_id) {
                        Some(Expr::Dereference(operand_id)) => *operand_id,
                        _ => *target_id,
                    };
                    rebinding.insert(named);
                }
                if let Some((callee_id, receiver_id)) = self.call_callee_and_receiver(expr_id) {
                    if new_ids.contains(&callee_id) {
                        new_calls.insert(expr_id);
                    } else if read_ids.contains(&callee_id) {
                        benign.insert(receiver_id);
                        read_calls.push(expr_id);
                    } else if write_ids.contains(&callee_id) {
                        benign.insert(receiver_id);
                        write_receivers.push((expr_id, receiver_id));
                    } else if clone_ids.contains(&callee_id) {
                        benign.insert(receiver_id);
                        clone_receivers.insert(expr_id, receiver_id);
                    }
                }
                // Pass 2's moves and pass 3's handle positions are read off
                // the same expression; their resolution waits for pass 1's
                // sets (below the loop).
                match expr {
                    Expr::Variable(variable_id) => {
                        if let Some(variable) = self.variables.get(variable_id)
                            && let Some(value_id) = variable.initial
                            && self.is_shared_handle(value_id)
                        {
                            moved.push((expr_id, CellSlot::Binding(*variable_id), value_id));
                        }
                    }
                    Expr::StructInitializer(struct_id, assignments) => {
                        for (index, value_id) in assignments {
                            if self.is_shared_handle(*value_id) {
                                moved.push((
                                    expr_id,
                                    CellSlot::Field(*struct_id, *index),
                                    *value_id,
                                ));
                            }
                        }
                    }
                    Expr::Assignment(target_id, value_id) => {
                        if self.is_shared_handle(*target_id) {
                            // Resolved after pass 1 (`slot_of` needs the clone
                            // receivers).
                            moved_targets.push((expr_id, *target_id, *value_id));
                        }
                    }
                    Expr::Local(_) | Expr::Field(_, _, _) => handle_positions.push(expr_id),
                    _ => {}
                }
            }
            // Value seam leaves (`value_seam_leaves`), the capture plan's
            // seams and candidates (`compute_capture_clone_sites`, phase 1).
            match expr {
                Expr::FunctionReturn(Some(value_id)) => value_seams.push(*value_id),
                Expr::Match(subject_id, legs) => {
                    for leg in legs {
                        self.held_seam_leaves(leg.body, &mut seam_leaves, true);
                        if self.is_capture_subject_place(*subject_id) {
                            let mut captures = Vec::new();
                            Self::collect_pattern_captures(&leg.pattern, &mut captures);
                            rows.captures
                                .extend(captures.into_iter().map(|id| (id, *subject_id)));
                        }
                    }
                }
                Expr::Destructure(value_id, pattern) => {
                    if self.is_capture_subject_place(*value_id) {
                        let mut captures = Vec::new();
                        Self::collect_pattern_captures(pattern, &mut captures);
                        rows.captures
                            .extend(captures.into_iter().map(|id| (id, *value_id)));
                    }
                }
                Expr::Is(subject_id, pattern) => {
                    if self.is_capture_subject_place(*subject_id) {
                        let mut captures = Vec::new();
                        Self::collect_pattern_captures(pattern, &mut captures);
                        rows.captures
                            .extend(captures.into_iter().map(|id| (id, *subject_id)));
                    }
                }
                Expr::Block((_, tail_id))
                | Expr::For(_, (_, tail_id))
                | Expr::ForEach(_, _, (_, tail_id)) => value_seams.push(*tail_id),
                Expr::If(_) => value_seams.push(expr_id),
                _ => {}
            }
            // The place-`let` candidates (`compute_shared_place_lets`) and
            // the shared-read candidates (`compute_shared_read_bindings`).
            if let Expr::Variable(variable_id) = expr
                && let Some(variable) = self.variables.get(variable_id)
                && let Some(value_id) = variable.initial
            {
                let peeled = self.peel_ascriptions(value_id);
                if !variable.mutable
                    && self.view_binding_mutability(*variable_id).is_none()
                    && let Some(root) = self.projection_root(peeled)
                    && root != *variable_id
                    && self.is_cloneable_aggregate(&variable.type_id.get_type(self))
                    && self.type_is_plain_value(variable.type_id, &mut Vec::new())
                {
                    rows.place_lets.push((expr_id, peeled, root));
                    candidate_bindings.insert(*variable_id);
                }
                let_rows.push((expr_id, *variable_id, value_id));
            }
            // The hand-on positions (`bindings_handed_on_whole`), resolved
            // against the candidates below the loop.
            let reads_one = |value_id: Id| -> Option<Id> {
                match self.expr_id_to_expr_map.get(&value_id) {
                    Some(Expr::Local(binding_id)) => Some(*binding_id),
                    _ => None,
                }
            };
            match expr {
                Expr::Variable(variable_id) => {
                    if let Some(initial) = self
                        .variables
                        .get(variable_id)
                        .and_then(|variable| variable.initial)
                        && let Some(binding) = reads_one(initial)
                    {
                        hand_on_positions.push((expr_id, binding));
                    }
                }
                Expr::Assignment(_, value_id) => {
                    if let Some(binding) = reads_one(*value_id) {
                        hand_on_positions.push((expr_id, binding));
                    }
                }
                Expr::List(slots) | Expr::Tuple(slots) => {
                    for slot in slots {
                        if let Some(binding) = reads_one(*slot) {
                            hand_on_positions.push((expr_id, binding));
                        }
                    }
                }
                Expr::StructInitializer(_, assignments) => {
                    for value_id in assignments.values() {
                        if let Some(binding) = reads_one(*value_id) {
                            hand_on_positions.push((expr_id, binding));
                        }
                    }
                }
                Expr::Match(subject_id, _)
                | Expr::Is(subject_id, _)
                | Expr::Destructure(subject_id, _) => {
                    if let Some(binding) = reads_one(*subject_id) {
                        hand_on_positions.push((expr_id, binding));
                    }
                }
                Expr::Call(call_id) => {
                    if let Some(function_call) = self.function_calls.get(call_id) {
                        for (index, argument_id) in function_call.argument_ids.iter().enumerate() {
                            if let Some(binding) = reads_one(*argument_id) {
                                hand_on_arguments.push((
                                    expr_id,
                                    binding,
                                    function_call.subject_id,
                                    index,
                                ));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        // The bodies' tails: a function's, a closure's, a module's.
        for (_, function) in self.live_held_entries(&self.functions) {
            if function.has_body {
                value_seams.push(function.body.1);
            }
        }
        for (_, closure) in self.live_held_entries(&self.closures) {
            self.held_seam_leaves(closure.return_, &mut seam_leaves, true);
        }
        for (_, module) in self.live_held_entries(&self.modules) {
            value_seams.push(module.body.1);
        }
        if !candidate_bindings.is_empty() || !let_rows.is_empty() || module_aggregate_lets {
            for seam in value_seams {
                match self.expr_id_to_expr_map.get(&seam) {
                    Some(Expr::If(branch)) => {
                        let mut leaves = Vec::new();
                        self.collect_branch_tail_leaves(branch, &mut leaves);
                        for leaf in leaves {
                            if let Some(root) = self.place_root(leaf) {
                                seam_leaves.push((leaf, root, false));
                            }
                        }
                    }
                    _ => self.held_seam_leaves(seam, &mut seam_leaves, false),
                }
            }
        }
        // The seam rows: every root; the place-`let` filter (a type query
        // per leaf) only where the root is a candidate of this walk or a
        // module-level binding — the only roots a place-`let` decision asks.
        for (leaf, root, capture) in seam_leaves {
            rows.seam_roots.push((leaf, root));
            if capture {
                rows.capture_seam_roots.push((leaf, root));
            }
            if (candidate_bindings.contains(&root) || module_level.contains(&root))
                && self.place_value_type_id(leaf).is_none_or(|type_id| {
                    let leaf_type = type_id.get_type(self);
                    self.is_cloneable_aggregate(&leaf_type)
                        || matches!(leaf_type, super::Type::Generic(_))
                })
            {
                rows.plain_seam_roots.push((leaf, root));
            }
        }
        // Every function return position, for the capture plan's seams.
        for (function_id, value_id) in &self.return_sites {
            if self.live_held.covers(function_id.0) {
                let mut leaves = Vec::new();
                self.collect_tail_leaves(*value_id, &mut leaves);
                for leaf in leaves {
                    if let Some(root) = self.place_root(leaf) {
                        rows.capture_seam_roots.push((*function_id, root));
                    }
                }
            }
        }
        // Cell facts, pass 2's unions and pass 3's taints, resolved with
        // pass 1's sets. `slot_of` follows `clone()` chains inside the
        // region, as `compute_shared_cells` does.
        if shared.is_some() {
            let slot_of = |mut expr_id: Id| -> Option<CellSlot> {
                for _ in 0..clone_receivers.len() + 1 {
                    match self.expr_id_to_expr_map.get(&expr_id)? {
                        Expr::Local(binding_id) => return Some(CellSlot::Binding(*binding_id)),
                        Expr::Field(_, struct_id, index) => {
                            return Some(CellSlot::Field(*struct_id, *index));
                        }
                        Expr::Call(_) => expr_id = *clone_receivers.get(&expr_id)?,
                        _ => return None,
                    }
                }
                None
            };
            if let Some(shared_struct_id) = shared_struct_id {
                for (parameter_id, parameter) in self.live_held_entries(&self.parameters) {
                    if self.type_is_shared_handle(parameter.type_id, shared_struct_id) {
                        rows.cell_unions.push((
                            *parameter_id,
                            CellSlot::Binding(*parameter_id),
                            CellSlot::Unknown,
                        ));
                    }
                }
                for (variable_id, variable) in self.live_held_entries(&self.variables) {
                    if variable.initial.is_none()
                        && self.type_is_shared_handle(variable.type_id, shared_struct_id)
                    {
                        rows.cell_unions.push((
                            *variable_id,
                            CellSlot::Binding(*variable_id),
                            CellSlot::Unknown,
                        ));
                    }
                }
            }
            for (key, target_id, value_id) in moved_targets {
                if let Some(target_slot) = slot_of(target_id) {
                    placed.insert(target_id);
                    moved.push((key, target_slot, value_id));
                }
            }
            for (key, slot, value_id) in moved {
                placed.insert(value_id);
                if new_calls.contains(&value_id) {
                    continue;
                }
                rows.cell_unions
                    .push((key, slot, slot_of(value_id).unwrap_or(CellSlot::Unknown)));
            }
            for expr_id in handle_positions {
                if benign.contains(&expr_id)
                    || placed.contains(&expr_id)
                    || !self.is_shared_handle(expr_id)
                {
                    continue;
                }
                rows.cell_unions.push((
                    expr_id,
                    slot_of(expr_id).unwrap_or(CellSlot::Unknown),
                    CellSlot::Unknown,
                ));
            }
            for read_id in read_calls {
                let slot = self
                    .call_callee_and_receiver(read_id)
                    .and_then(|(_, receiver_id)| slot_of(receiver_id))
                    .unwrap_or(CellSlot::Unknown);
                rows.cell_reads.push((read_id, slot));
            }
            for (write_id, receiver_id) in write_receivers {
                let slot = slot_of(receiver_id).unwrap_or(CellSlot::Unknown);
                rows.cell_writes
                    .push((write_id, slot, rebinding.contains(&write_id)));
            }
            let reads: HashSet<Id> = rows.cell_reads.iter().map(|(read, _)| *read).collect();
            rows.shared_reads = let_rows
                .into_iter()
                .filter(|(_, _, value_id)| reads.contains(value_id))
                .collect();
        }
        // The hand-ons that matter: of a candidate of THIS walk, or of a
        // module-level binding — a local is handed on only inside its own
        // region, a module-level binding's candidacy is its declaring
        // region's business and may be decided on another analysis.
        for (position, binding) in hand_on_positions {
            if candidate_bindings.contains(&binding) || module_level.contains(&binding) {
                rows.handed_on.push((position, binding));
            }
        }
        for (position, binding, subject_id, index) in hand_on_arguments {
            if !(candidate_bindings.contains(&binding) || module_level.contains(&binding)) {
                continue;
            }
            let loaned = conventions_of(self, subject_id).is_some_and(|conventions| {
                matches!(
                    conventions.get(index),
                    Some(super::Convention::Bare | super::Convention::Ref)
                )
            });
            if !loaned {
                rows.handed_on.push((position, binding));
            }
        }
        rows
    }

    /// One seam expression's tail leaves with their roots; `capture` marks
    /// the capture plan's seams.
    fn held_seam_leaves(&self, seam: Id, into: &mut Vec<(Id, Id, bool)>, capture: bool) {
        let mut leaves = Vec::new();
        self.collect_tail_leaves(seam, &mut leaves);
        for leaf in leaves {
            if let Some(root) = self.place_root(leaf) {
                into.push((leaf, root, capture));
            }
        }
    }

    /// The fold of the restored rows and `live`.
    pub(super) fn fold_held(&self, live: &HeldRows) -> HeldFold {
        let restored = &self.restored_tables.held;
        let all = [restored, live];
        let mut written = WrittenRoots {
            any: HashSet::default(),
            in_place: HashSet::default(),
        };
        for (_, root, in_place) in all.iter().flat_map(|rows| &rows.written) {
            written.any.insert(*root);
            if *in_place {
                written.in_place.insert(*root);
            }
        }
        let mut cells = SharedCells::default();
        for (_, a, b) in all.iter().flat_map(|rows| &rows.cell_unions) {
            cells.union(*a, *b);
        }
        for (read_id, slot) in all.iter().flat_map(|rows| &rows.cell_reads) {
            let slot = cells.find(*slot);
            cells.reads.insert(*read_id, slot);
        }
        for (write_id, slot, rebinds) in all.iter().flat_map(|rows| &rows.cell_writes) {
            let slot = cells.find(*slot);
            cells.writes.insert(*write_id, (slot, *rebinds));
            if !rebinds {
                cells.mutated.insert(slot);
            }
        }
        let roots = |pick: &dyn Fn(&HeldRows) -> &Vec<(Id, Id)>| -> HashSet<Id> {
            all.iter()
                .flat_map(|rows| pick(rows))
                .map(|(_, root)| *root)
                .collect()
        };
        HeldFold {
            written,
            cells,
            seam_roots: roots(&|rows| &rows.seam_roots),
            plain_seam_roots: roots(&|rows| &rows.plain_seam_roots),
            capture_seam_roots: roots(&|rows| &rows.capture_seam_roots),
            place_lets: all
                .iter()
                .flat_map(|rows| rows.place_lets.iter().copied())
                .collect(),
            handed_on: all
                .iter()
                .flat_map(|rows| &rows.handed_on)
                .map(|(_, binding)| *binding)
                .collect(),
            shared_reads: all
                .iter()
                .flat_map(|rows| rows.shared_reads.iter().copied())
                .collect(),
            captures: all
                .iter()
                .flat_map(|rows| rows.captures.iter().copied())
                .collect(),
        }
    }
}
