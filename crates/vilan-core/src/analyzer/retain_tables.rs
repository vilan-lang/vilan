//! M188 (Order 56) — C16's retaining summary per ASKED callee, over a
//! body-expression index at item grain.
//!
//! `check_view_escape` refuses a view-capturing closure handed to a callee
//! that KEEPS the parameter position it lands in (C16, R7). Which positions a
//! callee keeps is an interprocedural summary — a position is kept when the
//! body stores the parameter (a struct field, a collection element, a return
//! value, an assignment's value) or hands it to a callee that keeps the
//! position it lands in — and E340 made it lazy (asked only by a live call
//! that hands such a closure on) and one sweep plus a worklist. The sweep
//! still read every expression of the program to answer ONE callee: on kolt a
//! live css keystroke and the parse break paid ~50 ms at load for it.
//!
//! Every fact the sweep reads is a fact about one body: a value expression
//! whose place roots at a parameter names that parameter's function and
//! position (a parameter id is unique to its function), and a call argument
//! rooted at a parameter names the edge from the callee's position to it. So
//! each region CONTRIBUTES rows ([`RetainRows`], a class D row family in
//! `ModuleTables`, restored for a reused module or served item and walked for
//! the family's live lane — `move_tables.rs`'s shape), the fold indexes them
//! by the function whose parameter they name ([`RetainIndex`]), and the
//! summary of an asked callee closes only over the callees its own rows reach
//! ([`Analyzer::retaining_summary`]).
//!
//! What a callee does with its argument is NOT a row: the callee may be
//! another region's function, and whether it has a body, is a variant
//! constructor or keeps its whole arity is read at the fold, from the call,
//! every time — the row says only "this argument roots at that parameter".
//!
//! The family is filed by an analysis that walked its lane: one that asked a
//! summary (the first live call handing on a view-capturing closure), or any
//! SEEDED analysis (the editor's), so the world's records carry the rows
//! before the keystroke that first needs them; a cold `vilan check` that asks
//! nothing walks nothing.

use std::collections::BTreeSet;

use crate::fx::{FxHashMap as HashMap, FxHashSet as HashSet};
use crate::id::Id;

use super::row_families::RowFamily;
use super::{Analyzer, Expr};

/// One region's (or one analysis's live) contributions to C16's summary.
/// Every row's FIRST id is the region's own expression the row was read off
/// — the partition's key — and the function a row names is the region's own
/// too (a parameter is lexical to its function).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct RetainRows {
    /// `(expression, function, position)` — a value at a KEEPING position (a
    /// struct field's value, a collection element, a `ret` value or a return
    /// site, an assignment's value) whose place roots at `function`'s
    /// parameter at `position`. The expression is the container the value
    /// was read off (the return site's value itself for a return site).
    pub kept: Vec<(Id, Id, u32)>,
    /// `(call, argument position, function, position)` — an argument of
    /// `call` whose place roots at `function`'s parameter at `position`. What
    /// the callee does with it is the fold's question.
    pub handed: Vec<(Id, u32, Id, u32)>,
}

impl RetainRows {
    pub(super) fn bytes(&self) -> usize {
        self.kept.len() * std::mem::size_of::<(Id, Id, u32)>()
            + self.handed.len() * std::mem::size_of::<(Id, u32, Id, u32)>()
    }

    /// How many rows there are.
    pub(super) fn len(&self) -> usize {
        self.kept.len() + self.handed.len()
    }

    /// Whether every id the rows carry satisfies `fits`.
    pub(super) fn fits(&self, fits: &dyn Fn(Id) -> bool) -> bool {
        self.kept
            .iter()
            .all(|(expression, function, _)| fits(*expression) && fits(*function))
            && self
                .handed
                .iter()
                .all(|(call, _, function, _)| fits(*call) && fits(*function))
    }

    /// The rows with every id re-based through `rebase`; a row naming an id
    /// it refuses is dropped (a filed record never carries one).
    pub(super) fn rebased(&self, rebase: &dyn Fn(Id) -> Option<Id>) -> RetainRows {
        RetainRows {
            kept: self
                .kept
                .iter()
                .filter_map(|(expression, function, position)| {
                    Some((rebase(*expression)?, rebase(*function)?, *position))
                })
                .collect(),
            handed: self
                .handed
                .iter()
                .filter_map(|(call, argument, function, position)| {
                    Some((rebase(*call)?, *argument, rebase(*function)?, *position))
                })
                .collect(),
        }
    }

    pub(super) fn extend(&mut self, other: &RetainRows) {
        self.kept.extend(other.kept.iter().copied());
        self.handed.extend(other.handed.iter().copied());
    }

    /// The rows partitioned by their key id through `slice_of`.
    pub(super) fn partition_into(
        &self,
        slice_of: &dyn Fn(Id) -> Option<u32>,
        into: &mut HashMap<u32, super::ModuleTables>,
    ) {
        for row in &self.kept {
            if let Some(slice) = slice_of(row.0) {
                into.entry(slice).or_default().retain.kept.push(*row);
            }
        }
        for row in &self.handed {
            if let Some(slice) = slice_of(row.0) {
                into.entry(slice).or_default().retain.handed.push(*row);
            }
        }
    }
}

/// The fold: the restored and live rows indexed by the function whose
/// parameter they name.
#[derive(Debug, Default)]
pub(super) struct RetainIndex {
    /// Per function, the parameter positions a keeping position names.
    kept: HashMap<Id, Vec<u32>>,
    /// Per function, `(call, argument position, parameter position)`.
    handed: HashMap<Id, Vec<(Id, u32, u32)>>,
}

/// What a call does with one argument, read from the call at the fold.
enum HandedTo {
    /// A bodied callee: the argument is kept when the callee keeps the
    /// position (an edge of the summary).
    Bodied(Id),
    /// Kept outright: a variant constructor stores every argument, an
    /// unresolvable subject is answered the safe way, and a callee with no
    /// readable body keeps the positions of its declared arity.
    Kept,
    /// Not kept: a position past a bodiless callee's arity.
    Dropped,
}

impl<'src> Analyzer<'src> {
    /// The live lane's rows: every return site, keeping position and call
    /// argument of a region whose rows are not restored, whose place roots at
    /// a parameter.
    pub(super) fn live_retain_rows(&self) -> RetainRows {
        let live = self.live_families.of(RowFamily::Retain);
        let mut rows = RetainRows::default();
        for (_, value_id) in &self.return_sites {
            if live.covers(value_id.0)
                && let Some((function, position)) = self.retained_parameter_of(*value_id)
            {
                rows.kept.push((*value_id, function, position));
            }
        }
        let keep = |rows: &mut RetainRows, expression: Id, value_id: Id| {
            if let Some((function, position)) = self.retained_parameter_of(value_id) {
                rows.kept.push((expression, function, position));
            }
        };
        for (expression, expr) in
            super::live_ranges::LiveEntries::over(&self.expr_id_to_expr_map, live)
        {
            match expr {
                Expr::StructInitializer(_, fields) => {
                    for value_id in fields.values() {
                        keep(&mut rows, *expression, *value_id);
                    }
                }
                Expr::List(ids) | Expr::Tuple(ids) => {
                    for value_id in ids {
                        keep(&mut rows, *expression, *value_id);
                    }
                }
                Expr::FunctionReturn(Some(value_id)) | Expr::Assignment(_, value_id) => {
                    keep(&mut rows, *expression, *value_id);
                }
                Expr::Call(call_id) => {
                    let Some(function_call) = self.function_calls.get(call_id) else {
                        continue;
                    };
                    for (argument, argument_id) in function_call.argument_ids.iter().enumerate() {
                        if let Some((function, position)) = self.retained_parameter_of(*argument_id)
                        {
                            rows.handed
                                .push((*call_id, argument as u32, function, position));
                        }
                    }
                }
                _ => {}
            }
        }
        rows
    }

    /// The fold of the restored rows and `live`. Under the plant
    /// (`Plant::RetainRowsUnrestored`) the restored rows are dropped while
    /// their regions stay out of the live lane — a summary that reads a
    /// restored region's body then answers without it.
    pub(super) fn fold_retain(&self, live: &RetainRows) -> RetainIndex {
        let mut index = RetainIndex::default();
        let restored_dropped =
            crate::incremental::planted(crate::incremental::Plant::RetainRowsUnrestored);
        let restored = &self.restored_tables.retain;
        let all: &[&RetainRows] = if restored_dropped {
            &[live]
        } else {
            &[restored, live]
        };
        for rows in all {
            for (_, function, position) in &rows.kept {
                index.kept.entry(*function).or_default().push(*position);
            }
            for (call, argument, function, position) in &rows.handed {
                index
                    .handed
                    .entry(*function)
                    .or_default()
                    .push((*call, *argument, *position));
            }
        }
        index
    }

    /// What `call_id` does with its argument at `argument` — the sweep's
    /// classification (`compute_retaining_positions_by_rounds`), read from
    /// the call now: the callee may be another region's.
    fn retain_handed_to(&self, call_id: Id, argument: u32) -> Option<HandedTo> {
        let function_call = self.function_calls.get(&call_id)?;
        match self.expr_id_to_expr_map.get(&function_call.subject_id) {
            Some(Expr::Local(callee_id)) if !self.call_is_variant_constructor(call_id) => {
                if self
                    .functions
                    .get(callee_id)
                    .is_some_and(|function| function.has_body)
                {
                    Some(HandedTo::Bodied(*callee_id))
                } else if self
                    .callee_retaining_positions(*callee_id, &HashMap::default())
                    .contains(&argument)
                {
                    Some(HandedTo::Kept)
                } else {
                    Some(HandedTo::Dropped)
                }
            }
            _ => Some(HandedTo::Kept),
        }
    }

    /// C16's summary of ONE callee: the parameter positions `callee` keeps,
    /// closed over the callees its rows reach and no others. Every summary
    /// the closure decides is final (nothing outside the closure can grow
    /// it), so all of them go into `memo` and a later ask of any is a lookup.
    pub(super) fn retaining_summary(
        &self,
        index: &RetainIndex,
        memo: &mut HashMap<Id, BTreeSet<u32>>,
        callee: Id,
    ) -> BTreeSet<u32> {
        if let Some(summary) = memo.get(&callee) {
            return summary.clone();
        }
        // The callees whose summaries this one reads, transitively, and the
        // edges `(callee, its position) -> (function, parameter position)`.
        let mut members: Vec<Id> = vec![callee];
        let mut seen: HashSet<Id> = HashSet::default();
        seen.insert(callee);
        let mut edges: HashMap<(Id, u32), Vec<(Id, u32)>> = HashMap::default();
        let mut summaries: HashMap<Id, BTreeSet<u32>> = HashMap::default();
        let mut queue: Vec<(Id, u32)> = Vec::new();
        let mut next = 0;
        while next < members.len() {
            let function = members[next];
            next += 1;
            let summary = summaries.entry(function).or_default();
            for position in index.kept.get(&function).into_iter().flatten() {
                if summary.insert(*position) {
                    queue.push((function, *position));
                }
            }
            for (call, argument, position) in index.handed.get(&function).into_iter().flatten() {
                match self.retain_handed_to(*call, *argument) {
                    Some(HandedTo::Bodied(target)) => match memo.get(&target) {
                        // A summary an earlier ask decided: the edge is a
                        // fact now.
                        Some(decided) => {
                            if decided.contains(argument)
                                && summaries.entry(function).or_default().insert(*position)
                            {
                                queue.push((function, *position));
                            }
                        }
                        None => {
                            edges
                                .entry((target, *argument))
                                .or_default()
                                .push((function, *position));
                            if seen.insert(target) {
                                members.push(target);
                            }
                        }
                    },
                    Some(HandedTo::Kept) => {
                        if summaries.entry(function).or_default().insert(*position) {
                            queue.push((function, *position));
                        }
                    }
                    Some(HandedTo::Dropped) | None => {}
                }
            }
        }
        while let Some(kept) = queue.pop() {
            let Some(targets) = edges.get(&kept) else {
                continue;
            };
            for (function, position) in targets {
                if summaries.entry(*function).or_default().insert(*position) {
                    queue.push((*function, *position));
                }
            }
        }
        if !crate::macros::in_macro_world() {
            let summarized = members.len() as u64;
            crate::incremental::update_census(|census| {
                census.retaining_summaries += summarized;
            });
        }
        let answer = summaries.get(&callee).cloned().unwrap_or_default();
        for function in members {
            let summary = summaries.remove(&function).unwrap_or_default();
            memo.insert(function, summary);
        }
        answer
    }
}
