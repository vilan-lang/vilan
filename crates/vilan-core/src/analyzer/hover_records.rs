//! M148 (Order 53) — the editor's per-file hover rows, recorded per module
//! and per item.
//!
//! E313 made four hover tables per-FILE under the reading aids — E206's call
//! labels, E237's type definitions, E240's type-position rows and E241's
//! pattern labels — so a module served as a view of its entry's world reads
//! its own rows. They were rendered for every package file on every editor
//! analysis, a served keystroke included: the cold prefix's rows re-rendered
//! beside the S2b label records that already spare `expr_types` and
//! `declaration_labels` (editor-52's rows: +0.7..1.8 % instructions per
//! keystroke). Every row is a pure function of the settled types of its own
//! region — a call's substituted signature, a position's type, a pattern's
//! matched type, a type-position name — exactly like the S2b label rows, so a
//! region whose record carries them ([`ModuleTables::hover_recorded`]) is not
//! rendered again: its rows are restored, and the tail renders the hot set
//! only.
//!
//! The id-keyed rows (E206, E237) skip by entity range, the S2b way; the
//! span-keyed ones (E240, E241) by their place in the file — a restored
//! module's source, or a served item's text region — and an item's are
//! re-based with its text on replay. A row naming an entity outside its
//! region's record (a pattern over an enum another re-walked item declares)
//! files no hover rows for that region at all, which then renders live.

use crate::fx::FxHashMap as HashMap;
use crate::id::Id;
use crate::span::Span;
use crate::type_::TypeId;

use super::hover_labels::{PatternLabel, ReferenceHover, TypeDefinitions};
use super::{Analyzer, SourceId};

/// One region's hover rows.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct RecordedHoverRows {
    /// E206: `(call or written subject, the signature label)`.
    pub call_labels: Vec<(Id, String)>,
    /// E237: `(position, field index, block)` — a binding, parameter or
    /// member read (`FIELD_NONE`), or a struct's field — with the index of
    /// its block in `blocks`.
    pub definitions: Vec<(Id, u32, u32)>,
    pub blocks: Vec<String>,
    /// E241 and E240, at their spans in their files.
    pub patterns: Vec<PatternLabel>,
    pub references: Vec<ReferenceHover>,
}

/// [`RecordedHoverRows::definitions`]' field index for a position that is not a
/// struct's field.
pub(super) const FIELD_NONE: u32 = u32::MAX;

impl RecordedHoverRows {
    pub(super) fn bytes(&self) -> usize {
        let id = std::mem::size_of::<Id>();
        self.call_labels
            .iter()
            .map(|(_, label)| id + std::mem::size_of::<String>() + label.len())
            .sum::<usize>()
            + self.definitions.len() * std::mem::size_of::<(Id, u32, u32)>()
            + self
                .blocks
                .iter()
                .map(|block| std::mem::size_of::<String>() + block.len())
                .sum::<usize>()
            + self
                .patterns
                .iter()
                .map(|row| std::mem::size_of::<PatternLabel>() + row.label.len())
                .sum::<usize>()
            + self
                .references
                .iter()
                .map(|row| {
                    std::mem::size_of::<ReferenceHover>()
                        + row.code.len()
                        + row.declared_by.as_ref().map_or(0, String::len)
                        + row.implemented_by.iter().map(String::len).sum::<usize>()
                })
                .sum::<usize>()
    }

    /// Whether every entity the rows name satisfies `fits`.
    pub(super) fn fits(&self, fits: &dyn Fn(Id) -> bool) -> bool {
        self.call_labels.iter().all(|(id, _)| fits(*id))
            && self.definitions.iter().all(|(id, _, _)| fits(*id))
            && self
                .patterns
                .iter()
                .all(|row| row.variant.is_none_or(|(enum_id, _)| fits(enum_id)))
    }

    /// The rows of an item's record re-based to where the item stands now:
    /// its entities through `rebase`, its spans through `shift`, its file as
    /// `source`.
    pub(super) fn rebased(
        &self,
        rebase: &dyn Fn(Id) -> Option<Id>,
        shift: &dyn Fn(Span) -> Span,
        source: SourceId,
    ) -> RecordedHoverRows {
        RecordedHoverRows {
            call_labels: self
                .call_labels
                .iter()
                .filter_map(|(id, label)| Some((rebase(*id)?, label.clone())))
                .collect(),
            definitions: self
                .definitions
                .iter()
                .filter_map(|(id, field, block)| Some((rebase(*id)?, *field, *block)))
                .collect(),
            blocks: self.blocks.clone(),
            patterns: self
                .patterns
                .iter()
                .filter_map(|row| {
                    let variant = match row.variant {
                        Some((enum_id, index)) => Some((rebase(enum_id)?, index)),
                        None => None,
                    };
                    Some(PatternLabel {
                        source,
                        name_span: shift(row.name_span),
                        label: row.label.clone(),
                        variant,
                        matched: row.matched,
                    })
                })
                .collect(),
            references: self
                .references
                .iter()
                .map(|row| ReferenceHover {
                    source,
                    span: shift(row.span),
                    ..row.clone()
                })
                .collect(),
        }
    }

    /// `other`'s rows appended, its definition blocks re-indexed.
    pub(super) fn extend(&mut self, other: &RecordedHoverRows) {
        let base = self.blocks.len() as u32;
        self.call_labels.extend(other.call_labels.iter().cloned());
        self.definitions.extend(
            other
                .definitions
                .iter()
                .map(|(id, field, block)| (*id, *field, base + *block)),
        );
        self.blocks.extend(other.blocks.iter().cloned());
        self.patterns.extend(other.patterns.iter().cloned());
        self.references.extend(other.references.iter().cloned());
    }
}

/// This analysis's rendered rows, before they are partitioned per region.
#[derive(Default)]
pub(super) struct RenderedHoverRows {
    pub call_labels: Vec<(Id, String)>,
    pub definitions: Vec<(Id, u32, u32)>,
    pub blocks: Vec<String>,
    pub patterns: Vec<PatternLabel>,
    pub references: Vec<ReferenceHover>,
}

impl RenderedHoverRows {
    /// The rows partitioned per region: the entity rows through `slice_of`,
    /// the span rows through `slice_at` — each slice with its own blocks.
    pub(super) fn partition_into(
        &self,
        slice_of: &dyn Fn(Id) -> Option<u32>,
        slice_at: &dyn Fn(SourceId, Span) -> Option<u32>,
        into: &mut HashMap<u32, super::ModuleTables>,
    ) {
        for (id, label) in &self.call_labels {
            if let Some(slice) = slice_of(*id) {
                into.entry(slice)
                    .or_default()
                    .hover
                    .call_labels
                    .push((*id, label.clone()));
            }
        }
        let mut local_blocks: HashMap<(u32, u32), u32> = HashMap::default();
        for (id, field, block) in &self.definitions {
            let Some(slice) = slice_of(*id) else {
                continue;
            };
            let rows = &mut into.entry(slice).or_default().hover;
            let local = *local_blocks.entry((slice, *block)).or_insert_with(|| {
                rows.blocks.push(self.blocks[*block as usize].clone());
                (rows.blocks.len() - 1) as u32
            });
            rows.definitions.push((*id, *field, local));
        }
        for row in &self.patterns {
            if let Some(slice) = slice_at(row.source, row.name_span) {
                into.entry(slice)
                    .or_default()
                    .hover
                    .patterns
                    .push(row.clone());
            }
        }
        for row in &self.references {
            if let Some(slice) = slice_at(row.source, row.span) {
                into.entry(slice)
                    .or_default()
                    .hover
                    .references
                    .push(row.clone());
            }
        }
    }
}

impl<'src> Analyzer<'src> {
    /// Whether `id`'s region restored its hover rows (an entity range of a
    /// module or a served item whose record carried them).
    pub(super) fn hover_row_restored(&self, id: Id) -> bool {
        let ranges = &self.restored_tables.hover_ranges;
        let index = ranges.partition_point(|(start, _)| *start <= id.0);
        index > 0 && id.0 < ranges[index - 1].1
    }

    /// Whether the place `(source, offset)` lies in a region that restored
    /// its hover rows — a restored module's file, or a served item's text.
    pub(super) fn hover_row_restored_at(&self, source: SourceId, offset: usize) -> bool {
        self.restored_tables.hover_sources.contains(&source)
            || self
                .item_window_at(source, offset)
                .is_some_and(|window| self.item_hover_restored(window))
    }

    /// E237's restored blocks, merged into `definitions` under this
    /// analysis's type ids for their positions.
    pub(super) fn restore_type_definitions(
        &self,
        definitions: &mut TypeDefinitions,
        expr_type_ids: &HashMap<Id, TypeId>,
        rows: &super::hover_labels::HoverRows,
    ) {
        let restored = &self.restored_tables.hover;
        if restored.definitions.is_empty() {
            return;
        }
        let mut by_text: HashMap<String, u32> = HashMap::default();
        for (id, field, block) in &restored.definitions {
            let Some(source) = self.source_of_id(*id) else {
                continue;
            };
            if !self.renders_hover_rows(source, rows) {
                continue;
            }
            let type_id = if *field == FIELD_NONE {
                self.variables
                    .get(id)
                    .map(|variable| variable.type_id)
                    .or_else(|| self.parameters.get(id).map(|parameter| parameter.type_id))
                    .or_else(|| expr_type_ids.get(id).copied())
            } else {
                self.structs
                    .get(id)
                    .and_then(|struct_| struct_.fields.get(*field as usize))
                    .map(|field| field.type_id)
            };
            let Some(type_id) = type_id else {
                continue;
            };
            let Some(text) = restored.blocks.get(*block as usize) else {
                continue;
            };
            definitions.insert_restored(type_id, text, &mut by_text);
        }
    }

    /// The restored span rows (E241, E240) a file of this analysis renders.
    pub(super) fn restored_span_rows(
        &self,
        rows: &super::hover_labels::HoverRows,
    ) -> (Vec<PatternLabel>, Vec<ReferenceHover>) {
        let restored = &self.restored_tables.hover;
        (
            restored
                .patterns
                .iter()
                .filter(|row| self.renders_hover_rows(row.source, rows))
                .cloned()
                .collect(),
            restored
                .references
                .iter()
                .filter(|row| self.renders_hover_rows(row.source, rows))
                .cloned()
                .collect(),
        )
    }

    /// The restored E206 rows a file of this analysis renders.
    pub(super) fn restored_call_labels(
        &self,
        rows: &super::hover_labels::HoverRows,
    ) -> Vec<(Id, String)> {
        self.restored_tables
            .hover
            .call_labels
            .iter()
            .filter(|(id, _)| {
                self.source_of_id(*id)
                    .is_some_and(|source| self.renders_hover_rows(source, rows))
            })
            .cloned()
            .collect()
    }
}
