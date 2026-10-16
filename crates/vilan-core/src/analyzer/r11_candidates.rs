//! M155 (Order 54) — R11's three lexical candidate indexes as a row family.
//!
//! An instantiation's scan (`check_resource_generic_instantiations`, M94)
//! classifies the bindings, the place expressions and the `is` tests LEXICAL
//! to its callee's declaration, and the callee may be any module's — std's,
//! on kolt, every time. So the three indexes span the world, and they were
//! built over the whole program at an analysis's first instantiation: every
//! variable and parameter, a sweep of the ~180,000-entry expression map, a
//! span and a source lookup per candidate and a sort per source — 7.5-14 ms
//! of a served kolt keystroke, all of it std's and the prefix's facts.
//!
//! Every row is one region's own fact — a candidate's id with its file and
//! span, unclassified (the classification is per instantiation and stays
//! live) — so each index is the union of per-region rows ([`CandidateRows`],
//! a class D row family in `ModuleTables`, M149's shape): restored for a
//! reused module or a served item (an item's re-based with its text), walked
//! for the family's live lane, and folded into one [`R11Candidates`] indexed
//! by span. The family is FILED only by an analysis that built the indexes
//! (one that met an instantiation) and was seeded by a front end — a cold
//! `vilan check` never reads a record back, so it pays for no partition.
//!
//! **What R11 door B reads** (`r11-instantiations.md` §4, incr-54b): the
//! fold, [`R11Candidates`] — per kind (bindings, places, `is` tests) the
//! candidates of every source sorted by `(start, id)`, with
//! [`R11Candidates::within`] answering a callee's slice — and, per region,
//! [`CandidateRows`]: per kind, `(id, source, start, end)` rows for the
//! placed candidates and the ids of the unplaced ones, every id the region's
//! own. A slice comes back in span order: every reader of it builds
//! a set (`collect_instantiation_bindings`, `collect_instantiation_value_places`,
//! `collect_is_refinements`), so the order handed in (the expression map's
//! hash order, which already differed between an incremental and a clean
//! analysis) was never part of an answer.

use crate::fx::FxHashMap as HashMap;
use crate::id::Id;
use crate::span::Span;

use super::live_ranges::LiveEntries;
use super::row_families::RowFamily;
use super::{Analyzer, Expr, SourceId};

/// One placed candidate: its id, its file and its span's bounds.
pub(super) type CandidateRow = (Id, u32, u32, u32);

/// One kind's rows: the PLACED candidates, and the ones the index cannot
/// place (no span, or no source range) — which ride every callee's slice, as
/// they always did.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct KindRows {
    pub placed: Vec<CandidateRow>,
    pub unplaced: Vec<Id>,
}

impl KindRows {
    fn len(&self) -> usize {
        self.placed.len() + self.unplaced.len()
    }

    fn push(&mut self, analyzer: &Analyzer<'_>, id: Id) {
        match (analyzer.source_of_id(id), analyzer.span_map.get(&id)) {
            (Some(source), Some(span)) => {
                self.placed
                    .push((id, source.0, span.start as u32, span.end as u32))
            }
            _ => self.unplaced.push(id),
        }
    }

    fn rebased(
        &self,
        rebase: &dyn Fn(Id) -> Option<Id>,
        shift: &dyn Fn(Span) -> Span,
        source: SourceId,
    ) -> KindRows {
        KindRows {
            placed: self
                .placed
                .iter()
                .filter_map(|(id, _, start, end)| {
                    let id = rebase(*id)?;
                    let shifted = shift(Span::new((), *start as usize..*end as usize));
                    Some((id, source.0, shifted.start as u32, shifted.end as u32))
                })
                .collect(),
            unplaced: self.unplaced.iter().filter_map(|id| rebase(*id)).collect(),
        }
    }

    fn extend(&mut self, other: &KindRows) {
        self.placed.extend(other.placed.iter().copied());
        self.unplaced.extend(other.unplaced.iter().copied());
    }

    fn fits(&self, fits: &dyn Fn(Id) -> bool) -> bool {
        self.placed.iter().all(|(id, ..)| fits(*id)) && self.unplaced.iter().all(|id| fits(*id))
    }
}

/// One region's (or one analysis's live) R11 candidates, unclassified.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct CandidateRows {
    /// Every variable and parameter.
    pub bindings: KindRows,
    /// Every place expression (`Local`, `Field`, `TupleIndex`, `Index`).
    pub places: KindRows,
    /// Every `is` test.
    pub tests: KindRows,
}

impl CandidateRows {
    pub(super) fn bytes(&self) -> usize {
        self.kinds()
            .iter()
            .map(|rows| {
                rows.placed.len() * std::mem::size_of::<CandidateRow>()
                    + rows.unplaced.len() * std::mem::size_of::<Id>()
            })
            .sum()
    }

    pub(super) fn len(&self) -> usize {
        self.kinds().iter().map(|rows| rows.len()).sum()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn kinds(&self) -> [&KindRows; 3] {
        [&self.bindings, &self.places, &self.tests]
    }

    fn kinds_mut(&mut self) -> [&mut KindRows; 3] {
        [&mut self.bindings, &mut self.places, &mut self.tests]
    }

    /// Whether every id the rows carry satisfies `fits`.
    pub(super) fn fits(&self, fits: &dyn Fn(Id) -> bool) -> bool {
        self.kinds().iter().all(|rows| rows.fits(fits))
    }

    /// An item's rows re-based to where it stands now: its ids through
    /// `rebase`, its spans through `shift`, its file as `source`. A row
    /// naming an id `rebase` refuses is dropped (a filed record carries
    /// none).
    pub(super) fn rebased(
        &self,
        rebase: &dyn Fn(Id) -> Option<Id>,
        shift: &dyn Fn(Span) -> Span,
        source: SourceId,
    ) -> CandidateRows {
        CandidateRows {
            bindings: self.bindings.rebased(rebase, shift, source),
            places: self.places.rebased(rebase, shift, source),
            tests: self.tests.rebased(rebase, shift, source),
        }
    }

    pub(super) fn extend(&mut self, other: &CandidateRows) {
        for (into, from) in self.kinds_mut().into_iter().zip(other.kinds()) {
            into.extend(from);
        }
    }

    /// The rows partitioned by their id through `slice_of`, each slice's
    /// placed rows in `(source, start, id)` order (a restored module's run
    /// then needs no sort at the fold).
    pub(super) fn partition_into(
        &self,
        slice_of: &dyn Fn(Id) -> Option<u32>,
        into: &mut HashMap<u32, super::ModuleTables>,
    ) {
        let mut touched: Vec<u32> = Vec::new();
        for (kind, rows) in self.kinds().into_iter().enumerate() {
            for row in &rows.placed {
                let Some(slice) = slice_of(row.0) else {
                    continue;
                };
                let tables = into.entry(slice).or_default();
                if tables.r11.is_empty() {
                    touched.push(slice);
                }
                tables.r11.kinds_mut()[kind].placed.push(*row);
            }
            for id in &rows.unplaced {
                let Some(slice) = slice_of(*id) else {
                    continue;
                };
                let tables = into.entry(slice).or_default();
                if tables.r11.is_empty() {
                    touched.push(slice);
                }
                tables.r11.kinds_mut()[kind].unplaced.push(*id);
            }
        }
        for slice in touched {
            if let Some(tables) = into.get_mut(&slice) {
                for rows in tables.r11.kinds_mut() {
                    rows.placed
                        .sort_unstable_by_key(|(id, file, start, _)| (*file, *start, id.0));
                }
            }
        }
    }
}

/// One kind's index: per source, `(start, end, id)` sorted by `(start, id)`.
#[derive(Debug, Default)]
pub(super) struct SpanIndex {
    by_source: HashMap<u32, Vec<(u32, u32, Id)>>,
    unplaced: Vec<Id>,
}

impl SpanIndex {
    fn fold(runs: [&KindRows; 2]) -> SpanIndex {
        let mut index = SpanIndex::default();
        for run in runs {
            for (id, file, start, end) in &run.placed {
                index
                    .by_source
                    .entry(*file)
                    .or_default()
                    .push((*start, *end, *id));
            }
            index.unplaced.extend(run.unplaced.iter().copied());
        }
        for entries in index.by_source.values_mut() {
            // A restored module's run arrives sorted; a re-walked file's
            // (its served items beside its live rows) is sorted here.
            if !entries.is_sorted_by_key(|(start, _, id)| (*start, id.0)) {
                entries.sort_unstable_by_key(|(start, _, id)| (*start, id.0));
            }
        }
        index.unplaced.sort_unstable_by_key(|id| id.0);
        index
    }

    /// The PLACED candidates whose span lies inside `start..end` of `source`.
    fn inside(&self, source: SourceId, start: usize, end: usize) -> impl Iterator<Item = Id> + '_ {
        let entries: &[(u32, u32, Id)] = self
            .by_source
            .get(&source.0)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let first = entries.partition_point(|(entry_start, ..)| (*entry_start as usize) < start);
        entries[first..]
            .iter()
            .take_while(move |(entry_start, ..)| *entry_start as usize <= end)
            .filter(move |(_, entry_end, _)| *entry_end as usize <= end)
            .map(|(_, _, id)| *id)
    }

    /// Every candidate, placed or not.
    fn all(&self) -> Vec<Id> {
        let mut all: Vec<Id> = self.unplaced.clone();
        let mut sources: Vec<&u32> = self.by_source.keys().collect();
        sources.sort_unstable();
        for source in sources {
            all.extend(self.by_source[source].iter().map(|(_, _, id)| *id));
        }
        all
    }
}

/// The fold R11 reads (and door B after it): the three indexes.
#[derive(Debug, Default)]
pub(super) struct R11Candidates {
    pub bindings: SpanIndex,
    pub places: SpanIndex,
    pub tests: SpanIndex,
}

impl R11Candidates {
    /// R11's slice (M94) of one index: the candidates inside `callee`'s
    /// declaration span, plus every candidate the index could not place — and
    /// every candidate at all when the callee itself cannot be placed. Both
    /// widen the slice back toward the whole-program set R11's readers were
    /// always correct over, so a missing span costs time, never an answer.
    pub(super) fn within(index: &SpanIndex, analyzer: &Analyzer<'_>, callee: Id) -> Vec<Id> {
        let (Some(source), Some(span)) = (
            analyzer.source_of_id(callee),
            analyzer.span_map.get(&callee),
        ) else {
            return index.all();
        };
        let mut within = index.unplaced.clone();
        within.extend(index.inside(source, span.start, span.end));
        within
    }
}

impl<'src> Analyzer<'src> {
    /// The live lane's candidate rows: the bindings, places and `is` tests of
    /// every region whose R11 rows are not restored.
    pub(super) fn live_r11_candidate_rows(&self) -> CandidateRows {
        let live = self.live_families.of(RowFamily::R11);
        let mut rows = CandidateRows::default();
        for (id, _) in LiveEntries::over(&self.variables, live) {
            rows.bindings.push(self, *id);
        }
        for (id, _) in LiveEntries::over(&self.parameters, live) {
            rows.bindings.push(self, *id);
        }
        for (id, expr) in LiveEntries::over(&self.expr_id_to_expr_map, live) {
            match expr {
                // A wired method call's subject reads its FUNCTION
                // (`wire_method_call`'s member reference): no span, no
                // source range, so it rode every callee's slice — kolt's
                // client carried ~25,000 of them into each of its 34
                // instances — and a function is never a resource place
                // (`compute_resource`), so it was never an answer.
                Expr::Local(target)
                    if self.functions.contains_key(target)
                        || self.external_functions.contains_key(target) => {}
                Expr::Local(_)
                | Expr::Field(_, _, _)
                | Expr::TupleIndex(_, _, _)
                | Expr::Index(_, _) => rows.places.push(self, *id),
                Expr::Is(..) => rows.tests.push(self, *id),
                _ => {}
            }
        }
        rows
    }

    /// The fold of the restored rows and `live` into the three indexes.
    pub(super) fn fold_r11_candidates(&self, live: &CandidateRows) -> R11Candidates {
        let restored = &self.restored_tables.r11;
        R11Candidates {
            bindings: SpanIndex::fold([&restored.bindings, &live.bindings]),
            places: SpanIndex::fold([&restored.places, &live.places]),
            tests: SpanIndex::fold([&restored.tests, &live.tests]),
        }
    }
}
