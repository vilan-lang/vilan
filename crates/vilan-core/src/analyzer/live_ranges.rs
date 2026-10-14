//! M110 S7b (Order 52, M142) — the LIVE ranges of id space.
//!
//! M19 T1 and S7 give every reuse predicate a sorted vector of REUSED
//! ranges (the frozen std sources, the reused modules of a served world, the
//! served items' windows) and a binary search per ask. A whole-program pass
//! written as `for (id, expr) in expr_id_to_expr_map { if reusable_entity(id)
//! { continue } .. }` then still VISITS every entity of the program on every
//! keystroke — 150k on kolt's client leg — to skip all but the edited file's,
//! and the checks phase of a served keystroke stayed at 262 ms with 39 of 46
//! items served (incr-51b's table). This module is the other half of that
//! vector: the COMPLEMENT of the reused ranges over the id space, sealed
//! once per analysis beside them, so a pass can walk the live ids and look
//! each one up instead of sweeping the table and asking each one.
//!
//! Three lanes, because the predicates differ (`analyzer.rs`):
//! - the **Class A** lane is `reusable_entity`'s complement (frozen ∪ reused
//!   modules ∪ served items);
//! - the **table** lane is `table_entity`'s (the reused modules and served
//!   items whose records carry class D rows; std only when its own record
//!   is restored);
//! - the **module** lane is the bound audit's (frozen ∪ reused modules: a
//!   re-walked file's items keep their sites live, M143).
//!
//! A cold check keeps its cost: with nothing reused the live set IS the
//! table, and [`LiveRanges::walks`] answers that a sweep of the map with the
//! predicate is the cheaper of the two (a lookup per live id would cost more
//! than an iteration step plus a search). The two orders differ — a walk
//! yields ids ascending, a sweep yields the map's order — so a pass routed
//! through [`LiveEntries`] must be order-insensitive, which the goldens, the
//! differentials and the native differential gate.

use indexmap::IndexMap;

use crate::id::Id;

/// The ids a predicate does NOT cover, as sorted disjoint `[start, end)`
/// ranges over `[0, high)`.
#[derive(Clone, Debug, Default)]
pub(super) struct LiveRanges {
    ranges: Vec<(u32, u32)>,
    /// How many ids the ranges cover.
    count: u64,
}

/// Below this share of a table's entries a walk of the live ids (one lookup
/// each) beats a sweep of the table (one iteration step and one search
/// each). Measured on kolt's plain and generated packages (M142): a lookup
/// into the analyzer's 200k-entry expression map costs several iteration
/// steps (a cache miss each), so the walk pays only when the live set is a
/// small fraction — a served keystroke's 8 %, an example's few thousand
/// ids under std's — and a cold check of a package half the size of its
/// table sweeps, as it did before the ranges existed.
const WALK_BELOW_PERMILLE: u64 = 200;

impl LiveRanges {
    /// The complement of `reused` over `[0, high)`. `reused` may overlap or
    /// touch (it is the concatenation of two sealed vectors); it is merged
    /// here.
    pub(super) fn complement(reused: &[(u32, u32)], high: u32) -> Self {
        let mut reused: Vec<(u32, u32)> = reused
            .iter()
            .copied()
            .filter(|(start, end)| end > start)
            .collect();
        reused.sort_unstable();
        let mut ranges: Vec<(u32, u32)> = Vec::with_capacity(reused.len() + 1);
        let mut cursor: u32 = 0;
        for (start, end) in reused {
            let start = start.min(high);
            let end = end.min(high);
            if start > cursor {
                ranges.push((cursor, start));
            }
            cursor = cursor.max(end);
        }
        if cursor < high {
            ranges.push((cursor, high));
        }
        let count = ranges
            .iter()
            .map(|(start, end)| u64::from(end - start))
            .sum();
        LiveRanges { ranges, count }
    }

    pub(super) fn ranges(&self) -> &[(u32, u32)] {
        &self.ranges
    }

    /// How many ids are live.
    pub(super) fn count(&self) -> u64 {
        self.count
    }

    /// Whether `id` is live — a binary search, the predicate's own cost.
    /// Inlined into the sweeping loops: as a call it cost the cold check
    /// 2 % (M142's plain rows).
    #[inline(always)]
    pub(super) fn covers(&self, id: u32) -> bool {
        let index = self.ranges.partition_point(|(start, _)| *start <= id);
        index > 0 && id < self.ranges[index - 1].1
    }

    /// Whether a pass over a table of `len` entries should WALK the live ids
    /// (sparse) rather than sweep the table and ask each entry (dense).
    pub(super) fn walks(&self, len: usize) -> bool {
        self.count * 1000 < len as u64 * WALK_BELOW_PERMILLE
    }

    /// Every live id, ascending.
    pub(super) fn ids(&self) -> LiveIds<'_> {
        LiveIds {
            ranges: &self.ranges,
            index: 0,
            next: self.ranges.first().map_or(0, |(start, _)| *start),
        }
    }
}

/// [`LiveRanges::ids`]'s cursor.
pub(super) struct LiveIds<'a> {
    ranges: &'a [(u32, u32)],
    index: usize,
    next: u32,
}

impl Iterator for LiveIds<'_> {
    type Item = u32;

    #[inline(always)]
    fn next(&mut self) -> Option<u32> {
        loop {
            let (_, end) = *self.ranges.get(self.index)?;
            if self.next < end {
                let id = self.next;
                self.next += 1;
                return Some(id);
            }
            self.index += 1;
            if let Some((start, _)) = self.ranges.get(self.index) {
                self.next = *start;
            }
        }
    }
}

/// An id-keyed table a pass can either sweep or look into: the analyzer's
/// hash maps and its insertion-ordered maps alike.
pub(super) trait IdTable<'a, V: 'a> {
    type Entries: Iterator<Item = (&'a Id, &'a V)>;
    fn len(&self) -> usize;
    fn entries(&'a self) -> Self::Entries;
    fn entry(&'a self, id: u32) -> Option<(&'a Id, &'a V)>;
}

impl<'a, V: 'a, S: std::hash::BuildHasher> IdTable<'a, V> for std::collections::HashMap<Id, V, S> {
    type Entries = std::collections::hash_map::Iter<'a, Id, V>;

    fn len(&self) -> usize {
        std::collections::HashMap::len(self)
    }

    fn entries(&'a self) -> Self::Entries {
        self.iter()
    }

    fn entry(&'a self, id: u32) -> Option<(&'a Id, &'a V)> {
        self.get_key_value(&Id(id))
    }
}

impl<'a, V: 'a, S: std::hash::BuildHasher> IdTable<'a, V> for IndexMap<Id, V, S> {
    type Entries = indexmap::map::Iter<'a, Id, V>;

    fn len(&self) -> usize {
        IndexMap::len(self)
    }

    fn entries(&'a self) -> Self::Entries {
        self.iter()
    }

    fn entry(&'a self, id: u32) -> Option<(&'a Id, &'a V)> {
        self.get_key_value(&Id(id))
    }
}

/// The entries of an id-keyed table a pass visits: the live ones whose
/// value passes `keep` (the pass's SHAPE — "a call", "an assignment"), by
/// whichever of the two forms is cheaper for this analysis. The shape is
/// asked before the range on a sweep, so a cold check pays the search only
/// at the entries the pass would have looked at anyway.
pub(super) enum LiveEntries<'a, V: 'a, T: IdTable<'a, V>, F: Fn(&V) -> bool> {
    /// Sweep the table, ask each entry.
    Sweep {
        entries: T::Entries,
        live: &'a LiveRanges,
        keep: F,
    },
    /// Walk the live ids, look each one up.
    Walk {
        ids: LiveIds<'a>,
        table: &'a T,
        keep: F,
    },
}

/// The shape that keeps every entry.
pub(super) type Any<V> = fn(&V) -> bool;

fn any<V>(_: &V) -> bool {
    true
}

impl<'a, V: 'a, T: IdTable<'a, V>> LiveEntries<'a, V, T, Any<V>> {
    pub(super) fn over(table: &'a T, live: &'a LiveRanges) -> Self {
        LiveEntries::over_where(table, live, any::<V> as Any<V>)
    }
}

impl<'a, V: 'a, T: IdTable<'a, V>, F: Fn(&V) -> bool> LiveEntries<'a, V, T, F> {
    pub(super) fn over_where(table: &'a T, live: &'a LiveRanges, keep: F) -> Self {
        if live.walks(table.len()) {
            LiveEntries::Walk {
                ids: live.ids(),
                table,
                keep,
            }
        } else {
            LiveEntries::Sweep {
                entries: table.entries(),
                live,
                keep,
            }
        }
    }
}

impl<'a, V: 'a, T: IdTable<'a, V>, F: Fn(&V) -> bool> Iterator for LiveEntries<'a, V, T, F> {
    type Item = (&'a Id, &'a V);

    /// Inlined into every routed loop, so the variant's match hoists out of
    /// the loop (one plain sweep or one plain walk per pass): as a call per
    /// entry it cost a cold check of a 160-module package 2 % (M142).
    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            LiveEntries::Sweep {
                entries,
                live,
                keep,
            } => {
                for (id, value) in entries.by_ref() {
                    if keep(value) && live.covers(id.0) {
                        return Some((id, value));
                    }
                }
                None
            }
            LiveEntries::Walk { ids, table, keep } => {
                for id in ids.by_ref() {
                    if let Some((key, value)) = table.entry(id)
                        && keep(value)
                    {
                        return Some((key, value));
                    }
                }
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_complement_merges_touching_and_overlapping_reused_ranges() {
        let live = LiveRanges::complement(&[(10, 20), (5, 12), (20, 25), (40, 41)], 50);
        assert_eq!(live.ranges(), &[(0, 5), (25, 40), (41, 50)]);
        assert_eq!(live.count(), 5 + 15 + 9);
        assert!(live.covers(0) && live.covers(4) && !live.covers(5));
        assert!(!live.covers(24) && live.covers(25) && live.covers(39));
        assert!(!live.covers(40) && live.covers(41) && live.covers(49));
        assert!(!live.covers(50));
        let ids: Vec<u32> = live.ids().collect();
        assert_eq!(ids.len() as u64, live.count());
        assert_eq!(ids[..6], [0, 1, 2, 3, 4, 25]);
        assert_eq!(ids[ids.len() - 1], 49);
    }

    #[test]
    fn nothing_reused_means_everything_live_and_a_sweep() {
        let live = LiveRanges::complement(&[], 7);
        assert_eq!(live.ranges(), &[(0, 7)]);
        assert_eq!(live.count(), 7);
        assert!(!live.walks(7));
        let empty = LiveRanges::complement(&[(0, 7)], 7);
        assert!(empty.ranges().is_empty());
        assert_eq!(empty.count(), 0);
        assert!(empty.walks(7));
        assert_eq!(empty.ids().count(), 0);
        assert!(!LiveRanges::complement(&[], 0).covers(0));
    }

    #[test]
    fn a_range_past_the_high_water_is_clipped() {
        let live = LiveRanges::complement(&[(3, 100)], 10);
        assert_eq!(live.ranges(), &[(0, 3)]);
    }

    #[test]
    fn the_entries_agree_in_both_shapes_over_both_tables() {
        let hashed: crate::fx::FxHashMap<Id, u32> =
            (0..100u32).map(|id| (Id(id), id * 2)).collect();
        let indexed: IndexMap<Id, u32> = (0..100u32).rev().map(|id| (Id(id), id * 2)).collect();
        let sparse = LiveRanges::complement(&[(0, 90), (95, 100)], 100);
        assert!(sparse.walks(hashed.len()));
        let expected: Vec<(Id, u32)> = (90..95u32).map(|id| (Id(id), id * 2)).collect();
        let even: Vec<Id> = LiveEntries::over_where(&hashed, &sparse, |value| value % 4 == 0)
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(
            even,
            vec![Id(90), Id(92), Id(94)],
            "the shape filters a walk"
        );
        let dense_even: Vec<Id> =
            LiveEntries::over_where(&hashed, &LiveRanges::complement(&[(0, 10)], 100), |value| {
                *value >= 190
            })
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(dense_even.len(), 5, "the shape filters a sweep");
        let walked: Vec<(Id, u32)> = LiveEntries::over(&hashed, &sparse)
            .map(|(id, value)| (*id, *value))
            .collect();
        assert_eq!(walked, expected);
        let walked: Vec<(Id, u32)> = LiveEntries::over(&indexed, &sparse)
            .map(|(id, value)| (*id, *value))
            .collect();
        assert_eq!(
            walked, expected,
            "a walk is ascending whatever the table's order"
        );
        let dense = LiveRanges::complement(&[(0, 10)], 100);
        assert!(!dense.walks(hashed.len()));
        let mut swept: Vec<Id> = LiveEntries::over(&hashed, &dense)
            .map(|(id, _)| *id)
            .collect();
        swept.sort_unstable_by_key(|id| id.0);
        assert_eq!(swept, (10..100u32).map(Id).collect::<Vec<_>>());
        let swept: Vec<Id> = LiveEntries::over(&indexed, &dense)
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(
            swept,
            (10..100u32).rev().map(Id).collect::<Vec<_>>(),
            "a sweep keeps the table's order"
        );
    }
}
