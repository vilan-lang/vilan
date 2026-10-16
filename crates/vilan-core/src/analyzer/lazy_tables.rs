//! M150 (Order 53) — the lazy tables and the HMR transfer sites at item
//! grain.
//!
//! `record_lazy_arguments` (lazy.md §1) swept every call of the program on
//! every analysis to find the ones filling a `lazy` parameter, and
//! `check_hmr_transfer_bounds` (hmr.md §4) swept every call again for the
//! `dev::stash` / `dev::take` sites — whole-program sweeps for a handful of
//! answers, re-paid on a served keystroke. The answers are lowering inputs
//! and diagnostics for EVERY body, so they need rows, not a window: each
//! region CONTRIBUTES its candidates ([`LazyRows`], [`HmrRows`] — row
//! families in `ModuleTables`, restored for a reused module or served item and
//! walked for the family's live lane), and the DECISIONS stay whole-program
//! and are taken over every candidate on every analysis:
//!
//! - which lazy parameters are eager (M81's retraction to a fixpoint over
//!   every pair, B362's convention groups), and so which arguments thunk and
//!   which forward a cell — a fact about the PARAMETER that all its call sites
//!   decide together;
//! - each `stash` value's re-inference and each `take` binding's element,
//!   which mint (the reason the pass was kept out of M19's window): only the
//!   sweep is restored, never a verdict or a slot.
//!
//! A family is filed only by an analysis that ran its pass (a program with no
//! `lazy` cell, or without `std::web::dev`, returns before looking), so a slice
//! says whether it carries the rows (`row_families.rs`).

use crate::fx::{FxHashMap as HashMap, FxHashSet as HashSet};
use crate::id::Id;

use super::row_families::RowFamily;
use super::{Analyzer, Expr, ThunkEffects};

/// One region's contributions to the lazy tables.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct LazyRows {
    /// `(call, argument position, the argument's effects)` — a resolved call
    /// whose callee declares a `lazy` parameter at that position. The call is
    /// the region's own; its argument and subject are read back from the
    /// call (`function_calls`), and the effects are those of the argument's
    /// expression (`scan_capture_body`), the region's own calls and `await`.
    pub pairs: Vec<(Id, u32, ThunkEffects)>,
    /// The region's `lazy` parameters — the eager decision's starting set.
    pub parameters: Vec<Id>,
}

/// One region's `dev::stash` value arguments and the bindings a `dev::take`
/// call initializes (hmr.md §4).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct HmrRows {
    pub stash_values: Vec<Id>,
    pub take_bindings: Vec<Id>,
}

fn rebase_effects(
    effects: &ThunkEffects,
    rebase: &dyn Fn(Id) -> Option<Id>,
) -> Option<ThunkEffects> {
    Some(ThunkEffects {
        calls: effects
            .calls
            .iter()
            .map(|call| rebase(*call))
            .collect::<Option<Vec<Id>>>()?,
        awaits: match effects.awaits {
            Some(id) => Some(rebase(id)?),
            None => None,
        },
    })
}

impl LazyRows {
    pub(super) fn bytes(&self) -> usize {
        let id = std::mem::size_of::<Id>();
        self.pairs
            .iter()
            .map(|(_, _, effects)| {
                std::mem::size_of::<(Id, u32, ThunkEffects)>() + effects.calls.len() * id
            })
            .sum::<usize>()
            + self.parameters.len() * id
    }

    pub(super) fn fits(&self, fits: &dyn Fn(Id) -> bool) -> bool {
        self.pairs.iter().all(|(call, _, effects)| {
            fits(*call)
                && effects.calls.iter().all(|id| fits(*id))
                && effects.awaits.is_none_or(fits)
        }) && self.parameters.iter().all(|id| fits(*id))
    }

    pub(super) fn rebased(&self, rebase: &dyn Fn(Id) -> Option<Id>) -> LazyRows {
        LazyRows {
            pairs: self
                .pairs
                .iter()
                .filter_map(|(call, index, effects)| {
                    Some((rebase(*call)?, *index, rebase_effects(effects, rebase)?))
                })
                .collect(),
            parameters: self
                .parameters
                .iter()
                .filter_map(|id| rebase(*id))
                .collect(),
        }
    }

    pub(super) fn extend(&mut self, other: &LazyRows) {
        self.pairs.extend(other.pairs.iter().cloned());
        self.parameters.extend(other.parameters.iter().copied());
    }

    pub(super) fn partition_into(
        &self,
        slice_of: &dyn Fn(Id) -> Option<u32>,
        into: &mut HashMap<u32, super::ModuleTables>,
    ) {
        for row in &self.pairs {
            if let Some(slice) = slice_of(row.0) {
                into.entry(slice).or_default().lazy.pairs.push(row.clone());
            }
        }
        for id in &self.parameters {
            if let Some(slice) = slice_of(*id) {
                into.entry(slice).or_default().lazy.parameters.push(*id);
            }
        }
    }
}

impl HmrRows {
    pub(super) fn bytes(&self) -> usize {
        (self.stash_values.len() + self.take_bindings.len()) * std::mem::size_of::<Id>()
    }

    pub(super) fn fits(&self, fits: &dyn Fn(Id) -> bool) -> bool {
        self.stash_values
            .iter()
            .chain(&self.take_bindings)
            .all(|id| fits(*id))
    }

    pub(super) fn rebased(&self, rebase: &dyn Fn(Id) -> Option<Id>) -> HmrRows {
        HmrRows {
            stash_values: self
                .stash_values
                .iter()
                .filter_map(|id| rebase(*id))
                .collect(),
            take_bindings: self
                .take_bindings
                .iter()
                .filter_map(|id| rebase(*id))
                .collect(),
        }
    }

    pub(super) fn extend(&mut self, other: &HmrRows) {
        self.stash_values.extend(other.stash_values.iter().copied());
        self.take_bindings
            .extend(other.take_bindings.iter().copied());
    }

    pub(super) fn partition_into(
        &self,
        slice_of: &dyn Fn(Id) -> Option<u32>,
        into: &mut HashMap<u32, super::ModuleTables>,
    ) {
        for id in &self.stash_values {
            if let Some(slice) = slice_of(*id) {
                into.entry(slice).or_default().hmr.stash_values.push(*id);
            }
        }
        for id in &self.take_bindings {
            if let Some(slice) = slice_of(*id) {
                into.entry(slice).or_default().hmr.take_bindings.push(*id);
            }
        }
    }
}

impl<'src> Analyzer<'src> {
    /// The live lane's lazy contributions: every live call filling a `lazy`
    /// parameter, with its argument's effects, and every live `lazy`
    /// parameter.
    pub(super) fn live_lazy_rows(&mut self) -> LazyRows {
        let live = self.live_families.of(RowFamily::Lazy);
        let mut pairs: Vec<(Id, u32, Id)> = Vec::new();
        for (call_id, call) in super::live_ranges::LiveEntries::over(&self.function_calls, live) {
            let Some(Expr::Local(callee_id)) = self.expr_id_to_expr_map.get(&call.subject_id)
            else {
                continue;
            };
            let Some(function) = self.functions.get(callee_id) else {
                continue;
            };
            for (index, (parameter_id, argument_id)) in function
                .parameters
                .iter()
                .zip(&call.argument_ids)
                .enumerate()
            {
                if self
                    .parameters
                    .get(parameter_id)
                    .is_some_and(|parameter| parameter.lazy)
                {
                    pairs.push((*call_id, index as u32, *argument_id));
                }
            }
        }
        let parameters: Vec<Id> = super::live_ranges::LiveEntries::over(&self.parameters, live)
            .filter(|(_, parameter)| parameter.lazy)
            .map(|(id, _)| *id)
            .collect();
        let mut rows = LazyRows {
            pairs: Vec::with_capacity(pairs.len()),
            parameters,
        };
        for (call_id, index, argument_id) in pairs {
            rows.pairs
                .push((call_id, index, self.thunk_effects_of(argument_id)));
        }
        rows
    }

    /// What an expression DOES when a thunk of it is forced (lazy.md §1).
    pub(super) fn thunk_effects_of(&self, expression_id: Id) -> ThunkEffects {
        let mut effects = ThunkEffects::default();
        self.scan_capture_body(
            expression_id,
            &HashSet::default(),
            &mut HashSet::default(),
            &mut Vec::new(),
            &mut HashSet::default(),
            &mut effects,
        );
        effects
    }

    /// The fold: every pair (restored and live) as `(parameter, argument,
    /// name, effects)` in the order the calls were recorded
    /// (`function_calls`' insertion order, then position) — the order the
    /// whole-program sweep visited them in, which the thunk table keeps —
    /// and every lazy parameter.
    pub(super) fn fold_lazy(
        &self,
        live: &LazyRows,
    ) -> (Vec<(Id, Id, &'src str, ThunkEffects)>, HashSet<Id>) {
        let restored = &self.restored_tables.lazy;
        let mut ordered: Vec<(usize, u32, Id, Id, &'src str, ThunkEffects)> = Vec::new();
        let mut parameters: HashSet<Id> = HashSet::default();
        for rows in [restored, live] {
            parameters.extend(rows.parameters.iter().copied());
            for (call_id, index, effects) in &rows.pairs {
                let Some((position, _, call)) = self.function_calls.get_full(call_id) else {
                    continue;
                };
                let Some(Expr::Local(callee_id)) = self.expr_id_to_expr_map.get(&call.subject_id)
                else {
                    continue;
                };
                let Some(parameter_id) = self
                    .functions
                    .get(callee_id)
                    .and_then(|function| function.parameters.get(*index as usize))
                else {
                    continue;
                };
                let Some(argument_id) = call.argument_ids.get(*index as usize) else {
                    continue;
                };
                let Some(parameter) = self.parameters.get(parameter_id) else {
                    continue;
                };
                ordered.push((
                    position,
                    *index,
                    *parameter_id,
                    *argument_id,
                    parameter.name,
                    effects.clone(),
                ));
            }
        }
        ordered.sort_unstable_by_key(|(position, index, ..)| (*position, *index));
        (
            ordered
                .into_iter()
                .map(|(_, _, parameter, argument, name, effects)| {
                    (parameter, argument, name, effects)
                })
                .collect(),
            parameters,
        )
    }

    /// The live lane's HMR transfer sites: each `dev::stash` call's value
    /// argument and each binding a `dev::take` call initializes.
    pub(super) fn live_hmr_rows(&self) -> HmrRows {
        let live = self.live_families.of(RowFamily::Hmr);
        let mut rows = HmrRows::default();
        let mut take_calls: HashSet<Id> = HashSet::default();
        for (call_id, function_call) in
            super::live_ranges::LiveEntries::over(&self.function_calls, live)
        {
            let callee_id = match self.expr_id_to_expr_map.get(&function_call.subject_id) {
                Some(Expr::Local(callee_id)) => *callee_id,
                _ => continue,
            };
            // A call the walk recorded and the tree still holds as one.
            if !matches!(self.expr_id_to_expr_map.get(call_id), Some(Expr::Call(_))) {
                continue;
            }
            if Some(callee_id) == self.hmr_stash_fn_id {
                if let Some(value_id) = function_call.argument_ids.get(1).copied() {
                    rows.stash_values.push(value_id);
                }
            } else if Some(callee_id) == self.hmr_take_fn_id {
                take_calls.insert(*call_id);
            }
        }
        if !take_calls.is_empty() {
            for (variable_id, variable) in
                super::live_ranges::LiveEntries::over(&self.variables, live)
            {
                if variable
                    .initial
                    .is_some_and(|initial| take_calls.contains(&initial))
                {
                    rows.take_bindings.push(*variable_id);
                }
            }
        }
        rows
    }
}
