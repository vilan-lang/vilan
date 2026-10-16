//! M149/M150 (Order 53) — the row FAMILIES a pass files only when it ran.
//!
//! The class D rows M19 T1b and S7b record are filed by every analysis that
//! files a slice at all: a slice with no `clone_sites` row says "this module
//! has no clone site", and the restore is exact. Some passes do not run on
//! every analysis, though. The move scan's whole-program helper sets
//! (`move_tables.rs`) are derived only once a program holds a resource
//! binding, and the lazy tables (M150) only once one writes a `lazy`
//! parameter or binding — so an empty row from an analysis that never asked
//! is not "nothing here", it is "not computed". A family is the set of rows
//! one such pass contributes, and a slice says, per family, whether its rows
//! were FILED ([`FamilySet`]); the regions whose slices carry a family are its
//! restored ranges, and the complement is the family's live lane — the
//! regions the pass walks for contributions. The held passes keep their own
//! lane (`held_ranges`), which predates this and differs in WHY a region is
//! left out (a row that names another re-walked window), not in shape.

use super::live_ranges::LiveRanges;

/// One family of conditionally-filed rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RowFamily {
    /// The move scan's helper sets (M149, `move_tables.rs`).
    Moves,
    /// The lazy argument tables (M150, `lazy_tables.rs`).
    Lazy,
    /// The HMR transfer sites (M150, `lazy_tables.rs`).
    Hmr,
    /// R11's lexical candidates (M155, `r11_candidates.rs`).
    R11,
}

pub(super) const FAMILIES: usize = 4;

impl RowFamily {
    pub(super) const ALL: [RowFamily; FAMILIES] = [
        RowFamily::Moves,
        RowFamily::Lazy,
        RowFamily::Hmr,
        RowFamily::R11,
    ];

    pub(super) fn index(self) -> usize {
        match self {
            RowFamily::Moves => 0,
            RowFamily::Lazy => 1,
            RowFamily::Hmr => 2,
            RowFamily::R11 => 3,
        }
    }
}

/// Which families a slice FILED.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct FamilySet(u8);

impl FamilySet {
    pub(super) fn with(self, family: RowFamily, filed: bool) -> FamilySet {
        let bit = 1u8 << family.index();
        FamilySet(if filed { self.0 | bit } else { self.0 & !bit })
    }

    pub(super) fn contains(self, family: RowFamily) -> bool {
        self.0 & (1u8 << family.index()) != 0
    }
}

/// Per family, the id ranges of the restored regions whose slices carried it
/// — sorted, disjoint after [`FamilyRanges::sort`].
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct FamilyRanges([Vec<(u32, u32)>; FAMILIES]);

impl FamilyRanges {
    pub(super) fn of(&self, family: RowFamily) -> &[(u32, u32)] {
        &self.0[family.index()]
    }

    pub(super) fn of_mut(&mut self, family: RowFamily) -> &mut Vec<(u32, u32)> {
        &mut self.0[family.index()]
    }

    pub(super) fn clear(&mut self) {
        for ranges in &mut self.0 {
            ranges.clear();
        }
    }

    pub(super) fn sort(&mut self) {
        for ranges in &mut self.0 {
            ranges.sort_unstable();
        }
    }

    /// The two vectors merged, family by family (the modules' and the
    /// items').
    pub(super) fn merged(&self, other: &FamilyRanges) -> FamilyRanges {
        let mut merged = self.clone();
        for family in RowFamily::ALL {
            merged
                .of_mut(family)
                .extend(other.of(family).iter().copied());
        }
        merged.sort();
        merged
    }
}

/// Per family, the live lane: the complement of its restored ranges.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct FamilyLanes([LiveRanges; FAMILIES]);

impl FamilyLanes {
    pub(super) fn complement(restored: &FamilyRanges, high: u32) -> FamilyLanes {
        FamilyLanes(RowFamily::ALL.map(|family| LiveRanges::complement(restored.of(family), high)))
    }

    pub(super) fn of(&self, family: RowFamily) -> &LiveRanges {
        &self.0[family.index()]
    }
}
