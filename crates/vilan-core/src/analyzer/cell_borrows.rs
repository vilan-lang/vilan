//! M199 — a pattern over a `Shared` read that only VIEWS what it binds reads
//! the cell in place instead of copying the payload.
//!
//! Rule 1 (spec §6.1) says a capture holds what its subject held when the
//! pattern matched. Over a `Shared` read the subject is the CELL's storage
//! (B256), so a capture that aliases that storage stays faithful for exactly
//! as long as nothing changes the storage under it and the capture cannot
//! carry it anywhere. Two answers to "nothing changes it" exist here:
//!
//! - **Whole-program** (JS only): no write mutates the cell IN PLACE anywhere
//!   — [`Analyzer::share_subject_is_stable`]'s cell arm, B267's first shape. A
//!   rebind installs a fresh value and leaves the aliased storage alone.
//! - **Per construct** (both backends, this module): no write of the cell AT
//!   ALL — rebind or in place, through any call the construct makes, B457's
//!   write summaries asked with [`WriteSubject::CellAny`] — can run while the
//!   `match` / `if` that binds the captures runs, and nothing in it suspends
//!   (spec §6.6: `await` bumps the world). The native backend then holds the
//!   cell's shared borrow for the construct and binds by reference, which a
//!   rebind WOULD meet (`borrow_mut` under a live `Ref`), hence "at all".
//!
//! Suspension is the call graph's answer (`Program::suspending_calls`), which
//! is settled only after analysis, so a candidate is recorded here as a
//! [`PendingCellBorrow`] and decided by [`settle_cell_borrows`] once
//! `async_infer` has run — the same split B119's view rule makes. Until then
//! every capture the candidate would share keeps its copy site, so a pipeline
//! that never reaches the settle copies, which is always sound.

use crate::fx::FxHashSet as HashSet;
use crate::id::Id;

use super::held_tables::HeldFold;
use super::{
    Analyzer, BinaryOp, CellSlot, Expr, ExprIfBranch, ExprPattern, LexicalCandidates, Program,
    WriteSubject,
};

/// One `match` / `if` over a `Shared` read whose construct no write of the
/// cell can reach — waiting on the suspension verdict
/// ([`settle_cell_borrows`]).
#[derive(Clone, Debug)]
pub struct PendingCellBorrow {
    /// The pattern subject: the read, or a field / slot spine over one.
    pub subject: Id,
    /// Every call expression inside the construct: it suspends when any does.
    pub calls: Vec<Id>,
    /// Whether an `await` token lies inside the construct.
    pub awaits: bool,
    /// The captures whose copy the JS backend drops when the borrow holds —
    /// the ones the capture plan would otherwise copy, admitted by
    /// [`Analyzer::shares_under_a_cell_borrow`].
    pub shared_captures: Vec<Id>,
    /// Whether the native backend may bind every capture by reference: the
    /// spine has no subscript, and every capture is immutable, reaches no
    /// value seam, is read by no other region and is no resource.
    pub native: bool,
}

impl<'src> Analyzer<'src> {
    /// M199: every `match` subject, and every `is` subject on an `if`
    /// condition's boolean spine, that reads a `Shared` cell no write can
    /// reach while the construct runs — keyed by the subject.
    ///
    /// The construct is the whole `match`, or the whole `if` chain (a superset
    /// of where an `if`'s captures live: its condition and branches). A GUARD
    /// clause's captures live past the `if` (R15, B187), so its `if` is no
    /// construct here and its captures keep the whole-program answer.
    pub(super) fn cell_borrow_candidates(
        &mut self,
        held: &HeldFold,
        calls: &std::cell::OnceCell<LexicalCandidates>,
    ) -> Vec<PendingCellBorrow> {
        if self.shared_cells.reads.is_empty() {
            return Vec::new();
        }
        let mut constructs: Vec<(Id, Id, Vec<Id>)> = Vec::new();
        for (expr_id, expr) in &self.expr_id_to_expr_map {
            match expr {
                Expr::Match(subject_id, legs) => {
                    if self.shared_read_spine_cell(*subject_id).is_none() {
                        continue;
                    }
                    let mut captures = Vec::new();
                    for leg in legs {
                        Self::collect_pattern_captures(&leg.pattern, &mut captures);
                    }
                    constructs.push((*subject_id, *expr_id, captures));
                }
                Expr::If(branch) if !self.guard_continuation_ifs.contains_key(expr_id) => {
                    let mut tests = Vec::new();
                    self.collect_if_chain_tests(branch, &mut tests);
                    for (subject_id, pattern) in tests {
                        if self.shared_read_spine_cell(subject_id).is_none() {
                            continue;
                        }
                        let mut captures = Vec::new();
                        Self::collect_pattern_captures(pattern, &mut captures);
                        constructs.push((subject_id, *expr_id, captures));
                    }
                }
                _ => {}
            }
        }
        if constructs.is_empty() {
            return Vec::new();
        }
        let awaits = LexicalCandidates::build(
            self,
            self.expr_id_to_expr_map
                .iter()
                .filter(|(_, expr)| matches!(expr, Expr::Await(_)))
                .map(|(id, _)| *id),
        );
        let calls = self.calls_index(calls);
        let mut candidates = Vec::new();
        for (subject_id, construct_id, captures) in constructs {
            let Some(cell) = self.shared_read_spine_cell(subject_id) else {
                continue;
            };
            if self.construct_reaches_a_cell_write(construct_id, cell, calls) {
                continue;
            }
            let (Some(source), Some(span)) = (
                self.source_of_id(construct_id),
                self.span_map.get(&construct_id).copied(),
            ) else {
                continue;
            };
            let calls_inside = calls
                .inside(source, span.start, span.end)
                .into_iter()
                .filter(|call_id| self.one_span_space(construct_id, *call_id))
                .collect();
            let awaits_inside = !awaits.inside(source, span.start, span.end).is_empty();
            let native = self.spine_is_borrowable_natively(subject_id)
                && captures.iter().all(|capture_id| {
                    self.capture_may_bind_a_borrow(*capture_id, &held.capture_seam_roots)
                });
            candidates.push(PendingCellBorrow {
                subject: subject_id,
                calls: calls_inside,
                awaits: awaits_inside,
                shared_captures: Vec::new(),
                native,
            });
        }
        candidates
    }

    /// The `is` tests on an `if` chain's conditions — every branch's, walked
    /// down the boolean spine (`&&`, `||`, `!`) the way R15's path captures
    /// are. A test off the spine (`if keep(x is Some(let n))`) binds nothing
    /// the branches can read and is left alone.
    fn collect_if_chain_tests<'a>(
        &'a self,
        branch: &'a ExprIfBranch,
        tests: &mut Vec<(Id, &'a ExprPattern)>,
    ) {
        if let ExprIfBranch::If(condition_id, _, next) = branch {
            self.collect_condition_tests(*condition_id, tests);
            if let Some(next) = next {
                self.collect_if_chain_tests(next, tests);
            }
        }
    }

    fn collect_condition_tests<'a>(&'a self, expr_id: Id, tests: &mut Vec<(Id, &'a ExprPattern)>) {
        let Some(_guard) = crate::util::RecursionGuard::enter() else {
            return;
        };
        match self.expr_id_to_expr_map.get(&expr_id) {
            Some(Expr::Is(subject_id, pattern)) => tests.push((*subject_id, pattern)),
            Some(Expr::Binary(BinaryOp::And | BinaryOp::Or, left, right))
                if !self.binary_op_dispatch.contains_key(&expr_id) =>
            {
                self.collect_condition_tests(*left, tests);
                self.collect_condition_tests(*right, tests);
            }
            Some(Expr::Unary('!', operand)) => self.collect_condition_tests(*operand, tests),
            _ => {}
        }
    }

    /// Whether anything inside the construct can write `cell` at all: a write
    /// of it in the construct's own text, or a call there whose write summary
    /// reaches one (B457's, asked with [`WriteSubject::CellAny`]). Spans, as
    /// every write question here asks them: containment is a superset of the
    /// subtree, the safe direction for a refusal.
    fn construct_reaches_a_cell_write(
        &self,
        construct_id: Id,
        cell: CellSlot,
        calls: &LexicalCandidates,
    ) -> bool {
        if !self.shared_cells.written.contains(&cell) {
            return false;
        }
        if self.spans_a_cell_write(construct_id, cell) {
            return true;
        }
        let (Some(source), Some(span)) = (
            self.source_of_id(construct_id),
            self.span_map.get(&construct_id),
        ) else {
            return true;
        };
        let subject = WriteSubject::CellAny(cell);
        let mut summaries = crate::fx::FxHashMap::default();
        calls
            .inside(source, span.start, span.end)
            .into_iter()
            .filter(|call_id| self.one_span_space(construct_id, *call_id))
            .any(|call_id| self.call_reaches_a_write(call_id, &subject, calls, &mut summaries))
    }

    /// Whether the native backend can render this spine as a borrow of the
    /// cell: the read itself, or fields and tuple slots over it. A subscript
    /// runs code (its index, its bounds check) the borrow would have to be
    /// taken around, and an ascription is a shape the emitter does not
    /// borrow through; both keep the copy.
    fn spine_is_borrowable_natively(&self, subject_id: Id) -> bool {
        let mut current = subject_id;
        loop {
            if self.shared_cells.reads.contains_key(&current) {
                return true;
            }
            current = match self.expr_id_to_expr_map.get(&current) {
                Some(Expr::Field(subject, _, _) | Expr::TupleIndex(subject, _, _)) => *subject,
                _ => return false,
            };
        }
    }

    /// Whether a capture can be a REFERENCE into the cell for its whole life:
    /// immutable (a `mut` capture is a copy the arm writes), at no value seam
    /// (it would carry the storage out), read by no other region (a closure
    /// could read it after the construct), and no resource (R1's loan is a
    /// different representation).
    fn capture_may_bind_a_borrow(&mut self, capture_id: Id, seam_roots: &HashSet<Id>) -> bool {
        let Some((type_id, mutable)) = self
            .variables
            .get(&capture_id)
            .map(|variable| (variable.type_id, variable.mutable))
        else {
            return false;
        };
        !mutable
            && !seam_roots.contains(&capture_id)
            && !self.last_use.is_opaque(capture_id)
            && !self.type_is_resource(type_id)
    }

    /// Whether the capture plan may SHARE a capture under a pending cell
    /// borrow — the JS half of [`Self::capture_may_bind_a_borrow`]: immutable,
    /// at no value seam, read by no other region.
    pub(super) fn shares_under_a_cell_borrow(
        &self,
        capture_id: Id,
        capture_is_mutable: bool,
        seam_roots: &HashSet<Id>,
    ) -> bool {
        !capture_is_mutable
            && !seam_roots.contains(&capture_id)
            && !self.last_use.is_opaque(capture_id)
    }
}

/// M199's verdict, once `async_infer` has settled which calls suspend: a
/// pending borrow whose construct holds no suspension point stands — its
/// captures drop the copy the capture plan held for them (JS), and its subject
/// is published for the native backend to borrow
/// ([`Program::borrowed_cell_subjects`]). One that suspends keeps every copy.
///
/// The suspension test is B119's (`check_view_suspensions`): an `await` token,
/// a call the emission awaits (`suspending_calls`, `awaited_calls`), or a call
/// some adapted instance awaits (an async closure handed to an adaptive
/// parameter, A.1) — the last asked of every instance, since the construct is
/// emitted once per instance and any one of them suspending is enough.
pub fn settle_cell_borrows(program: &mut Program) {
    let pending = std::mem::take(&mut program.pending_cell_borrows);
    if pending.is_empty() {
        return;
    }
    let adapted: HashSet<Id> = program
        .adapted_instances
        .values()
        .flat_map(|instance| instance.awaited_calls.iter().copied())
        .collect();
    for borrow in pending {
        let suspends = borrow.awaits
            || borrow.calls.iter().any(|call_id| {
                program.suspending_calls.contains(call_id)
                    || program.awaited_calls.contains(call_id)
                    || adapted.contains(call_id)
            });
        if suspends {
            continue;
        }
        for capture_id in &borrow.shared_captures {
            program.capture_clone_sites.remove(capture_id);
        }
        if borrow.native {
            program.borrowed_cell_subjects.insert(borrow.subject);
        }
    }
}
