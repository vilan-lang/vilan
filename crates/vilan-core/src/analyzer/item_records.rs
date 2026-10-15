//! M110 S7 (Order 51) — per-item records over the id windows
//! (`analyzer-pass-map.md` §7.3; `incremental-analysis.md` §3.3 door C).
//!
//! M19 remembers a module's Class A output per SOURCE, and only for the
//! stored world's prefix: a hot module's text moves with every keystroke, so
//! its record was never written and the whole file — every item of it, and
//! every item of the entry beside it — re-ran the checks and re-rendered its
//! tables on every keystroke (the ~125 ms tail of E292's served row). S6 gave
//! every top-level item its own id window; this module gives it a RECORD:
//! the item's Class A diagnostics and warnings, its class D table rows, its
//! label rows (S2b), its R10 structure keys and B575 enrolments, keyed by the
//! item's source path and ordinal and validated by what the item IS to the
//! checks — its text, its walk, and the RESOLVE's results for it, which are
//! fingerprinted structurally ([`Analyzer::item_signature`]: every slot the
//! item's expressions and declarations resolved to, every entity its
//! references bound to, each rendered relative to the window that owns it).
//!
//! **What makes a served record sound.** An item's checks read its own body,
//! the resolved types of its expressions and declarations, and what the
//! entities it references declare (a callee's signature and effects, a
//! struct's fields). The first is the text hash; the second is the
//! signature; the third is the dirty CLOSURE: an item is served only when
//! every item it reads from is clean, transitively, and every item whose
//! constraints or drains WROTE into its slots is clean (M139's edge: a push
//! into another module's use-inferred binding is a write of the owner's
//! slot by the pusher's item — `writes_drain_other`, 71–77 per kolt leg).
//! The prefix's items are clean by §2.1 on a hit (identical ids, identical
//! slots) unless a dirty hot item wrote into them or an alias of the entry
//! reaches them. A slot in no window (the resolve's and the base's
//! unanchored mints — "the program's") is read through its content and
//! re-paid if written. Nothing here carries a `TypeId` across analyses: the
//! record's rows are `Id`s the item's walk minted, re-based on replay by the
//! window's new start, and its diagnostics are byte offsets re-based by the
//! item's new text offset.
//!
//! The bound audit (S2a) stays live for a re-walked file's items: its
//! questions carry the callee's constraint slot, a `TypeId` of another
//! window, and the hot set's impls are the late impls its guard asks about.
//! `plan_resource_drops` and R11 stay live too (their tables carry
//! `TypeId`s). Both are named in the report as the remaining ms.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::error::Error;
use crate::fx::{FxHashMap as HashMap, FxHashSet as HashSet};
use crate::id::Id;
use crate::span::Span;
use crate::type_::TypeId;

use super::id_windows::Lane;
use super::{
    Analyzer, BaseCacheKey, CHECKED_CACHE, DERIVED_SOURCE, Expr, ExprIfBranch, ExprMatchLeg,
    ExprPattern, ModuleDiagnostics, ModuleTables, RestoredTables, SourceId,
};

impl RestoredTables {
    /// [`Self::absorb`] for a served ITEM's slice (M110 S7): every row's ids
    /// re-based through `rebase` on the way in — one clone, not two.
    fn absorb_rebased(
        &mut self,
        tables: &ModuleTables,
        rebase: &dyn Fn(Id) -> Option<Id>,
        shift: &dyn Fn(Span) -> Span,
    ) {
        let ids = |rows: &[Id]| {
            rows.iter()
                .filter_map(|id| rebase(*id))
                .collect::<Vec<Id>>()
        };
        self.last_uses.extend(ids(&tables.last_uses));
        self.last_use_opaque.extend(ids(&tables.last_use_opaque));
        self.last_use_unreached
            .extend(ids(&tables.last_use_unreached));
        for (rows, into) in [
            (&tables.last_use_statements, &mut self.last_use_statements),
            (
                &tables.declaration_statements,
                &mut self.declaration_statements,
            ),
        ] {
            for (id, chain) in rows {
                let chain: Option<Vec<Id>> = chain.iter().map(|id| rebase(*id)).collect();
                if let (Some(id), Some(chain)) = (rebase(*id), chain) {
                    into.insert(id, chain);
                }
            }
        }
        for (_, binding) in &tables.foreign_reads {
            if let Some(binding) = rebase(*binding) {
                self.foreign_reads.insert(binding);
            }
        }
        self.r11_seed_calls
            .extend(tables.r11_seed_calls.iter().filter_map(|id| rebase(*id)));
        for (id, positions) in &tables.bumps {
            if let Some(id) = rebase(*id) {
                self.bumps.insert(id, positions.clone());
            }
        }
        for (rows, into) in [
            (&tables.clone_sites, &mut self.clone_sites),
            (&tables.return_clone_sites, &mut self.return_clone_sites),
        ] {
            for (id, decision) in rows {
                if let Some(id) = rebase(*id) {
                    into.insert(id, decision.clone());
                }
            }
        }
        self.return_view_reads
            .extend(ids(&tables.return_view_reads));
        self.scalar_view_calls
            .extend(ids(&tables.scalar_view_calls));
        self.scalar_view_refs.extend(ids(&tables.scalar_view_refs));
        self.drop_roots.extend(ids(&tables.drop_roots));
        for (rows, into) in [
            (&tables.expr_types, &mut self.expr_types),
            (&tables.declaration_labels, &mut self.declaration_labels),
        ] {
            for (id, label) in rows {
                if let Some(id) = rebase(*id) {
                    into.insert(id, label.clone());
                }
            }
        }
        if tables.held_recorded {
            self.held.extend(&tables.held.rebased(rebase));
        }
        if tables
            .families
            .contains(super::row_families::RowFamily::Moves)
        {
            self.moves.extend(&tables.moves.rebased(rebase));
        }
        if tables
            .families
            .contains(super::row_families::RowFamily::Lazy)
        {
            self.lazy.extend(&tables.lazy.rebased(rebase));
        }
        if tables
            .families
            .contains(super::row_families::RowFamily::Hmr)
        {
            self.hmr.extend(&tables.hmr.rebased(rebase));
        }
        for hint in &tables.stage_hints {
            if let (Some(id), Some(chain)) = (rebase(hint.id), rebase(hint.chain)) {
                self.stage_hints.push(super::StageHint {
                    id,
                    chain,
                    span: shift(hint.span),
                    full: hint.full.clone(),
                    abbreviated: hint.abbreviated.clone(),
                    written: hint.written.clone(),
                });
            }
        }
        match self.drop_nominals_world {
            None => self.drop_nominals_world = Some(tables.drop_nominals_world),
            Some(existing) => self.drop_nominals_agree &= existing == tables.drop_nominals_world,
        }
    }
}

/// A record's key: the item's source PATH key and its ordinal in the source
/// — the id windows' demand key (`ItemWindow::key`), so a record survives
/// the keystrokes that re-walk the file and the sessions that rebuild its
/// world.
pub(super) type ItemKey = (u64, u32);

/// One top-level item's remembered checks (M110 S7).
#[derive(Debug, Clone)]
pub(super) struct ItemRecord {
    /// The hash of the item's text REGION: from the previous item's end (so
    /// an attribute or doc comment above the item is the item's) to its own
    /// span's end.
    pub text_hash: u64,
    /// Where that region started at record time; a replayed span is shifted
    /// by the difference to where it starts now.
    pub region_start: usize,
    /// The entity window at record time: its start, and its whole span
    /// (room included) — a row's id inside it is the item's own and is
    /// re-based; an id below the stored world's boundary is the prefix's
    /// and kept; any other id made the item unrecordable.
    pub entity_start: u32,
    pub entity_span: u32,
    /// The walk's mints per lane — a cheap validator that the item walked
    /// as it did.
    pub walk: [u32; 3],
    /// The resolve's results for the item, fingerprinted
    /// ([`Analyzer::item_signature`]).
    pub signature: u64,
    /// The laid-out windows the item depended on at record time (read from,
    /// or written by), by key — compared with the current analysis's.
    pub deps: Vec<ItemKey>,
    /// The record proper: the shape M19's per-source record has, minus the
    /// bound audit's half (live for items).
    pub module: ModuleDiagnostics,
}

impl ItemRecord {
    /// What the record retains, in M46's currency.
    pub fn bytes(&self) -> usize {
        std::mem::size_of::<ItemRecord>()
            + self.deps.len() * std::mem::size_of::<ItemKey>()
            + (self.module.diagnostics.len() + self.module.warnings.len())
                * std::mem::size_of::<Error>()
            + self.module.tables.as_ref().map_or(0, ModuleTables::bytes)
            + self
                .module
                .container_structures
                .iter()
                .map(|key| key.len())
                .sum::<usize>()
    }
}

/// What one laid-out window is to the records, computed once per analysis.
#[derive(Debug, Clone)]
struct ItemFacts {
    key: ItemKey,
    text_hash: u64,
    region_start: usize,
    signature: u64,
    deps: Vec<ItemKey>,
}

/// The per-analysis state of the item records, on the [`Analyzer`].
#[derive(Clone, Debug, Default)]
pub(super) struct ItemState {
    /// Whether this analysis reads and writes item records at all (a seeded
    /// editor analysis over a served world, the windows relocating every
    /// lane).
    enabled: bool,
    /// Per laid-out source: `(region start, span end, window)` in text
    /// order, for placing a diagnostic in its item.
    spans: HashMap<SourceId, Vec<(usize, usize, usize)>>,
    facts: HashMap<usize, ItemFacts>,
    /// The windows served this analysis, with their records.
    served: Vec<(usize, Arc<ItemRecord>)>,
    served_set: HashSet<usize>,
    /// The served windows' entity ids, sorted and disjoint — the Class A
    /// predicate's ranges — and the subsets whose records carry tables and
    /// label rows.
    ranges: Vec<(u32, u32)>,
    table_ranges: Vec<(u32, u32)>,
    label_ranges: Vec<(u32, u32)>,
    hint_ranges: Vec<(u32, u32)>,
    held_ranges: Vec<(u32, u32)>,
    /// M149/M150: per conditional row family, the served windows whose
    /// records carry it.
    family_ranges: super::row_families::FamilyRanges,
    /// The Class A output this analysis derived for the laid-out windows it
    /// did NOT serve, per window — the next analysis's records.
    derived: HashMap<usize, ModuleDiagnostics>,
    /// Windows whose output reaches outside what a record may carry.
    unrecordable: HashSet<usize>,
}

/// Where an id lives relative to the windows: in one (by index and offset),
/// below the stored world's boundary (the prefix's, identical on every hit),
/// or in the tail with no window to name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Relative {
    Window(usize, u32),
    Prefix(u32),
    Tail(u32),
}

/// The windows and the prefix tail slots an item's signature reached — the
/// read edges the dirty closure needs.
#[derive(Clone, Debug, Default)]
struct Reached {
    windows: Vec<usize>,
    tail_slots: Vec<u32>,
}

impl Reached {
    fn settle(&mut self) {
        self.windows.sort_unstable();
        self.windows.dedup();
        self.tail_slots.sort_unstable();
        self.tail_slots.dedup();
    }
}

/// The hasher the signatures use: deterministic within one process, which
/// is where the records live.
fn hasher() -> std::hash::DefaultHasher {
    std::hash::DefaultHasher::new()
}

/// Every `Id` and `TypeId` an expression node carries — its children (the
/// item's own) and the entities its references resolved to (the edges).
fn expr_ids(expr: &Expr<'_>, ids: &mut Vec<Id>, types: &mut Vec<TypeId>) {
    fn block(block: &(Vec<Id>, Id), ids: &mut Vec<Id>) {
        ids.extend(block.0.iter().copied());
        ids.push(block.1);
    }
    let pattern = expr_ids_pattern;
    let branch = expr_ids_branch;
    match expr {
        Expr::Assignment(a, b)
        | Expr::Binary(_, a, b)
        | Expr::Field(a, b, _)
        | Expr::Index(a, b) => {
            ids.push(*a);
            ids.push(*b);
        }
        Expr::Async(a)
        | Expr::Await(a)
        | Expr::Ascribe(a)
        | Expr::Call(a)
        | Expr::Closure(a)
        | Expr::Enum(a)
        | Expr::ExternalFunction(a)
        | Expr::Function(a)
        | Expr::TryAssert(a)
        | Expr::Impl(a)
        | Expr::Local(a)
        | Expr::Module(a)
        | Expr::Parameter(a)
        | Expr::Struct(a)
        | Expr::Trait(a)
        | Expr::Unary(_, a)
        | Expr::Reference(a, _)
        | Expr::Dereference(a)
        | Expr::Variable(a)
        | Expr::CallerLocation(a)
        | Expr::TupleIndex(a, _, _)
        | Expr::EnumVariant(a, _) => ids.push(*a),
        Expr::Block(body) => block(body, ids),
        Expr::TupleComprehension(pairs, tail) => {
            for (a, b) in pairs {
                ids.push(*a);
                ids.push(*b);
            }
            ids.push(*tail);
        }
        Expr::Destructure(subject, pattern_) | Expr::Is(subject, pattern_) => {
            ids.push(*subject);
            pattern(pattern_, ids, types);
        }
        Expr::For(condition, body) => {
            ids.extend(condition.iter().copied());
            block(body, ids);
        }
        Expr::ForEach(subject, binding, body) => {
            ids.push(*subject);
            ids.extend(binding.iter().copied());
            block(body, ids);
        }
        Expr::FunctionReturn(value) => ids.extend(value.iter().copied()),
        Expr::Lift(a, b, c) => {
            ids.push(*a);
            ids.push(*b);
            ids.push(*c);
        }
        Expr::LiftRegion(rows, tail) => {
            for (a, b, _) in rows {
                ids.push(*a);
                ids.push(*b);
            }
            ids.push(*tail);
        }
        Expr::Generic(type_id) | Expr::LengthValue(type_id) => types.push(*type_id),
        Expr::Repeat(a, type_id) | Expr::ArrayLen(a, type_id) => {
            ids.push(*a);
            types.push(*type_id);
        }
        Expr::If(branch_) => branch(branch_, ids),
        Expr::Match(subject, legs) => {
            ids.push(*subject);
            for ExprMatchLeg {
                pattern: leg_pattern,
                guard,
                body,
            } in legs
            {
                pattern(leg_pattern, ids, types);
                ids.extend(guard.iter().copied());
                ids.push(*body);
            }
        }
        Expr::List(elements) | Expr::Tuple(elements) => ids.extend(elements.iter().copied()),
        Expr::StructInitializer(struct_id, fields) => {
            ids.push(*struct_id);
            ids.extend(fields.values().copied());
        }
        Expr::Bool(_)
        | Expr::Error
        | Expr::LiftBinder
        | Expr::Jump(_)
        | Expr::Null
        | Expr::Number(..)
        | Expr::String(_)
        | Expr::MultilineString(_)
        | Expr::Void
        | Expr::Macro => {}
    }
}

fn expr_ids_pattern(pattern: &ExprPattern, ids: &mut Vec<Id>, types: &mut Vec<TypeId>) {
    match pattern {
        ExprPattern::Wildcard => {}
        ExprPattern::Binding(id) | ExprPattern::Literal(id) => ids.push(*id),
        ExprPattern::Variant(enum_id, _, payload) => {
            ids.push(*enum_id);
            for sub in payload {
                expr_ids_pattern(sub, ids, types);
            }
        }
        ExprPattern::Tuple(elements) => {
            for (sub, type_id) in elements {
                types.push(*type_id);
                expr_ids_pattern(sub, ids, types);
            }
        }
        ExprPattern::Array(elements) => {
            for sub in elements {
                expr_ids_pattern(sub, ids, types);
            }
        }
    }
}

fn expr_ids_branch(branch: &ExprIfBranch, ids: &mut Vec<Id>) {
    match branch {
        ExprIfBranch::If(condition, body, rest) => {
            ids.push(*condition);
            ids.extend(body.0.iter().copied());
            ids.push(body.1);
            if let Some(rest) = rest {
                expr_ids_branch(rest, ids);
            }
        }
        ExprIfBranch::Else(body) => {
            ids.extend(body.0.iter().copied());
            ids.push(body.1);
        }
    }
}

impl<'src> Analyzer<'src> {
    /// Whether this analysis keeps item records: the windows relocate every
    /// lane (so an item's ids are its window's), a front end seeded it (the
    /// editor's keystroke; a cold `vilan check` never reads one back), the
    /// world was served (its prefix ids are the stored world's, §2.1) and
    /// the checks have a key to file under.
    pub(super) fn enable_item_records(&mut self, enabled: bool) {
        self.items.enabled = enabled
            && self.id_windows.on()
            && self.id_windows.mode.relocates(Lane::Entity)
            && self.id_windows.mode.relocates(Lane::Type);
    }

    pub(super) fn item_records_enabled(&self) -> bool {
        self.items.enabled
    }

    /// Whether `source` is one of the re-walked files — the entry, or a hot
    /// module loaded past the stored prefix — whose items the records are
    /// for. By source rather than by the inherited boundary: on a miss this
    /// analysis laid the prefix's windows out itself, and those are the
    /// stored world's items, M19's per-source records' business.
    fn item_source(&self, source: SourceId) -> bool {
        source != DERIVED_SOURCE && (source.0 == 0 || source.0 >= self.reuse_prefix_len)
    }

    /// The windows this analysis laid out for a re-walked file's items
    /// (never a generated expansion's, whose spans index a template no file
    /// holds).
    fn laid_out_item_windows(&self) -> Vec<usize> {
        (self.id_windows.inherited..self.id_windows.windows.len())
            .filter(|index| {
                let window = &self.id_windows.windows[*index];
                window.item.is_some() && self.item_source(window.source)
            })
            .collect()
    }

    /// Indexes every laid-out item's text region by source, so a diagnostic
    /// finds its item by its span, and an item's text hash covers the
    /// attributes and comments above it.
    pub(super) fn seal_item_spans(&mut self) {
        self.items.spans.clear();
        if !self.items.enabled {
            return;
        }
        let mut by_source: HashMap<SourceId, Vec<(usize, usize, usize)>> = HashMap::default();
        for index in self.laid_out_item_windows() {
            let window = &self.id_windows.windows[index];
            let Some(item) = window.item else { continue };
            let Some(span) = self.span_map.get(&item) else {
                continue;
            };
            by_source
                .entry(window.source)
                .or_default()
                .push((span.start, span.end, index));
        }
        for rows in by_source.values_mut() {
            rows.sort_unstable();
            let mut previous_end = 0usize;
            for row in rows.iter_mut() {
                let (start, end, _) = *row;
                row.0 = previous_end.min(start);
                previous_end = end;
            }
        }
        self.items.spans = by_source;
    }

    /// The laid-out window whose item region holds `offset` in `source`.
    pub(super) fn item_window_at(&self, source: SourceId, offset: usize) -> Option<usize> {
        let rows = self.items.spans.get(&source)?;
        let index = rows.partition_point(|(start, _, _)| *start <= offset);
        let (_, end, window) = *rows.get(index.checked_sub(1)?)?;
        (offset < end).then_some(window)
    }

    /// Whether the item at `offset` in `source` is served this analysis —
    /// the written-application tier's per-site question (R10 walks its sites
    /// by file, not by entity).
    pub(super) fn item_served_at(&self, source: SourceId, offset: usize) -> bool {
        self.item_window_at(source, offset)
            .is_some_and(|window| self.items.served_set.contains(&window))
    }

    /// The served items' entity windows (sorted, disjoint): the Class A
    /// predicate's third half beside the frozen and the reused-module
    /// ranges, merged into them by `seal_reuse_ranges`.
    pub(super) fn item_ranges(&self) -> &[(u32, u32)] {
        &self.items.ranges
    }

    pub(super) fn item_table_ranges(&self) -> &[(u32, u32)] {
        &self.items.table_ranges
    }

    pub(super) fn item_label_ranges(&self) -> &[(u32, u32)] {
        &self.items.label_ranges
    }

    pub(super) fn item_hint_ranges(&self) -> &[(u32, u32)] {
        &self.items.hint_ranges
    }

    pub(super) fn item_held_ranges(&self) -> &[(u32, u32)] {
        &self.items.held_ranges
    }

    pub(super) fn item_family_ranges(&self) -> &super::row_families::FamilyRanges {
        &self.items.family_ranges
    }

    fn relative_entity(&self, id: Id) -> Relative {
        if let Some(window) = self.id_windows.window_of_entity(id) {
            let start = self.id_windows.windows[window].entities.start;
            return Relative::Window(window, id.0.wrapping_sub(start));
        }
        match self.prefix_entity_end {
            Some(end) if id.0 < end => Relative::Prefix(id.0),
            _ => Relative::Tail(id.0),
        }
    }

    fn relative_type(&self, id: TypeId) -> Relative {
        if let Some(window) = self.id_windows.window_of_type(id) {
            let start = self.id_windows.windows[window].types.start;
            return Relative::Window(window, id.0.wrapping_sub(start));
        }
        match self.prefix_type_end {
            Some(end) if id.0 < end => Relative::Prefix(id.0),
            _ => Relative::Tail(id.0),
        }
    }

    /// Hashes an entity reference as the records see it, noting the window
    /// it reaches.
    fn hash_entity(&self, id: Id, hash: &mut impl Hasher, reached: &mut Reached) {
        match self.relative_entity(id) {
            Relative::Window(window, offset) => {
                reached.windows.push(window);
                (0u8, self.id_windows.windows[window].key, offset).hash(hash);
            }
            Relative::Prefix(id) => (1u8, id).hash(hash),
            Relative::Tail(id) => (2u8, id).hash(hash),
        }
    }

    /// A type slot's PLACE as the item owning `window` sees it: its offset
    /// in its own window, another window's key and offset (an edge to that
    /// window — its content is that window's business, and the dirty
    /// closure carries it), a prefix tail slot's id (a tail read, re-paid if
    /// written), or a tail slot of this analysis (no stable place: hashed as
    /// such). Never the content: a type reaches an item only along the
    /// reference edges the signature already records, so the closure over
    /// those edges is what answers "did what this slot holds move".
    fn hash_type_place(
        &self,
        slot: TypeId,
        window: usize,
        hash: &mut impl Hasher,
        reached: &mut Reached,
    ) {
        match self.relative_type(slot) {
            Relative::Window(other, offset) if other == window => (0u8, offset).hash(hash),
            Relative::Window(other, offset) => {
                reached.windows.push(other);
                (1u8, self.id_windows.windows[other].key, offset).hash(hash);
            }
            Relative::Prefix(id) => {
                reached.tail_slots.push(id);
                (2u8, id).hash(hash);
            }
            Relative::Tail(_) => 3u8.hash(hash),
        }
    }

    /// The resolve's results for one item, fingerprinted by SHAPE: for every
    /// entity of its window (walk-minted and relocated, in id order) the
    /// entities the node references and the place of the slot it was typed
    /// with; for every written type reference in its region the entity it
    /// bound to and its slot's place. Answers the fingerprint and the
    /// windows and prefix tail slots it reached — the read edges.
    fn item_signature(&self, window: usize) -> (u64, Reached) {
        let item = &self.id_windows.windows[window];
        let mut hash = hasher();
        let mut reached = Reached::default();
        let mut ids: Vec<Id> = Vec::new();
        let mut types: Vec<TypeId> = Vec::new();
        let entities = (item.entities.start..item.entities.cursor)
            .chain(self.id_windows.spilled_entities_of(window));
        for id in entities {
            let id = Id(id);
            let Some(expr) = self.expr_id_to_expr_map.get(&id) else {
                continue;
            };
            ids.clear();
            types.clear();
            expr_ids(expr, &mut ids, &mut types);
            (ids.len(), types.len()).hash(&mut hash);
            for target in &ids {
                self.hash_entity(*target, &mut hash, &mut reached);
            }
            for type_id in &types {
                self.hash_type_place(*type_id, window, &mut hash, &mut reached);
            }
            match self.expr_id_to_type_id_map.get(&id) {
                Some(type_id) => self.hash_type_place(*type_id, window, &mut hash, &mut reached),
                None => u64::MAX.hash(&mut hash),
            }
        }
        if let Some(rows) = self.items.spans.get(&item.source)
            && let Some((region_start, region_end, _)) =
                rows.iter().find(|(_, _, index)| *index == window)
        {
            for (source, span, target, type_id) in &self.type_references {
                if *source != item.source || span.start < *region_start || span.start >= *region_end
                {
                    continue;
                }
                // Relative to the region (M110 S7b): the absolute offset moves
                // with every edit EARLIER in the file, which left every later
                // item that writes a type stale on every keystroke; the
                // region's text hash already answers for the spelling.
                (span.start - region_start, span.end - region_start).hash(&mut hash);
                match target {
                    Some(target) => self.hash_entity(*target, &mut hash, &mut reached),
                    None => u32::MAX.hash(&mut hash),
                }
                self.hash_type_place(*type_id, window, &mut hash, &mut reached);
            }
        }
        reached
            .windows
            .retain(|reached_window| *reached_window != window);
        reached.settle();
        (hash.finish(), reached)
    }

    /// The text region of a laid-out window: its start and the item's span
    /// end, and the region's hash.
    fn item_text(&self, window: usize) -> Option<(usize, u64)> {
        let item = &self.id_windows.windows[window];
        let (region_start, region_end, _) = *self
            .items
            .spans
            .get(&item.source)?
            .iter()
            .find(|(_, _, index)| *index == window)?;
        let text = self
            .source_texts
            .iter()
            .find(|(source, _)| *source == item.source)
            .map(|(_, text)| *text)?;
        let region = text.get(region_start..region_end)?;
        Some((region_start, crate::content_hash(region)))
    }

    /// Decides which laid-out items this analysis serves from `records`
    /// (the world's item records), computing every item's facts on the way
    /// — its text hash, its signature and the windows it reaches — and
    /// closing the dirty set over the read and write edges.
    pub(super) fn decide_item_records(
        &mut self,
        records: &HashMap<ItemKey, Arc<ItemRecord>>,
        alias_reaching: &HashSet<SourceId>,
    ) {
        self.items.served.clear();
        self.items.served_set.clear();
        self.items.ranges.clear();
        self.items.table_ranges.clear();
        self.items.label_ranges.clear();
        self.items.hint_ranges.clear();
        self.items.held_ranges.clear();
        self.items.family_ranges.clear();
        self.items.facts.clear();
        self.items.derived.clear();
        self.items.unrecordable.clear();
        if !self.items.enabled {
            return;
        }
        let laid = self.laid_out_item_windows();
        if laid.is_empty() {
            return;
        }
        let unguarded = crate::incremental::planted(crate::incremental::Plant::ItemRecordUnguarded);
        let window_count = self.id_windows.windows.len();
        let mut reads: HashMap<usize, Reached> = HashMap::default();
        let mut candidate: HashMap<usize, Arc<ItemRecord>> = HashMap::default();
        let mut stale: u64 = 0;
        for &window in &laid {
            let Some((region_start, text_hash)) = self.item_text(window) else {
                self.items.unrecordable.insert(window);
                continue;
            };
            let (signature, reached) = self.item_signature(window);
            let item = &self.id_windows.windows[window];
            let record = records.get(&item.key).cloned();
            let mut deps: Vec<ItemKey> = reached
                .windows
                .iter()
                .map(|reached| self.id_windows.windows[*reached].key)
                .collect();
            for (writer, written) in &self.id_windows.cross_writes {
                if *written as usize == window {
                    deps.push(self.id_windows.windows[*writer as usize].key);
                }
            }
            deps.sort_unstable();
            deps.dedup();
            if let Some(record) = record {
                let text_matches = record.text_hash == text_hash && record.walk == item.walk;
                let resolve_matches =
                    unguarded || (record.signature == signature && record.deps == deps);
                if text_matches && resolve_matches {
                    candidate.insert(window, record);
                } else {
                    stale += 1;
                }
            }
            reads.insert(window, reached);
            self.items.facts.insert(
                window,
                ItemFacts {
                    key: item.key,
                    text_hash,
                    region_start,
                    signature,
                    deps,
                },
            );
        }
        // The dirty closure. Dirty to begin with: every laid-out item with
        // no serviceable record; every inherited window an alias of the
        // entry reaches (its resolution can be answered by the open file);
        // every window written with no anchor to name the writer. Then, to
        // a fixpoint: a window written by a dirty one is dirty, and a
        // laid-out item reading a dirty window — or a written prefix tail
        // slot — is dirty.
        let mut dirty = vec![false; window_count];
        for &window in &laid {
            if !candidate.contains_key(&window) {
                dirty[window] = true;
            }
        }
        for (index, window) in self.id_windows.windows.iter().enumerate() {
            if alias_reaching.contains(&window.source) {
                dirty[index] = true;
            }
        }
        for written in &self.id_windows.unanchored_write_windows {
            dirty[*written as usize] = true;
        }
        // A tail slot's writes, by slot: a prefix tail slot read by an item
        // is dirty when a DIRTY (or anchorless) writer wrote it — a clean
        // writer's write is the one the record was computed under.
        let mut tail_written: Vec<(u32, Option<u32>)> = self.id_windows.tail_written.clone();
        tail_written.sort_unstable();
        tail_written.dedup();
        if !unguarded {
            loop {
                let mut moved = false;
                for (writer, written) in &self.id_windows.cross_writes {
                    if dirty[*writer as usize] && !dirty[*written as usize] {
                        dirty[*written as usize] = true;
                        moved = true;
                    }
                }
                for (&window, reached) in &reads {
                    if dirty[window] {
                        continue;
                    }
                    let tail_dirty = |slot: &u32| {
                        let from = tail_written.partition_point(|(written, _)| written < slot);
                        tail_written[from..]
                            .iter()
                            .take_while(|(written, _)| written == slot)
                            .any(|(_, writer)| writer.is_none_or(|writer| dirty[writer as usize]))
                    };
                    let reads_dirty = reached.windows.iter().any(|reached| dirty[*reached])
                        || reached.tail_slots.iter().any(tail_dirty);
                    if reads_dirty {
                        dirty[window] = true;
                        moved = true;
                    }
                }
                if !moved {
                    break;
                }
            }
        }
        let mut ranges: Vec<(u32, u32)> = Vec::new();
        let mut table_ranges: Vec<(u32, u32)> = Vec::new();
        let mut label_ranges: Vec<(u32, u32)> = Vec::new();
        let mut hint_ranges: Vec<(u32, u32)> = Vec::new();
        let mut held_ranges: Vec<(u32, u32)> = Vec::new();
        let mut family_ranges = super::row_families::FamilyRanges::default();
        for &window in &laid {
            let Some(record) = candidate.get(&window) else {
                continue;
            };
            if dirty[window] {
                stale += 1;
                continue;
            }
            let item = &self.id_windows.windows[window];
            let mut own: Vec<(u32, u32)> = vec![(item.entities.start, item.entities.end)];
            own.extend(
                self.id_windows
                    .spilled_entities_of(window)
                    .map(|id| (id, id + 1)),
            );
            ranges.extend(own.iter().copied());
            if let Some(tables) = &record.module.tables {
                table_ranges.extend(own.iter().copied());
                if tables.labels_recorded {
                    label_ranges.extend(own.iter().copied());
                }
                if tables.hints_recorded {
                    hint_ranges.extend(own.iter().copied());
                }
                if tables.held_recorded {
                    held_ranges.extend(own.iter().copied());
                }
                for family in super::row_families::RowFamily::ALL {
                    if tables.families.contains(family) {
                        family_ranges.of_mut(family).extend(own.iter().copied());
                    }
                }
            }
            self.items.served_set.insert(window);
            self.items.served.push((window, record.clone()));
        }
        ranges.sort_unstable();
        table_ranges.sort_unstable();
        label_ranges.sort_unstable();
        hint_ranges.sort_unstable();
        held_ranges.sort_unstable();
        self.items.ranges = ranges;
        self.items.table_ranges = table_ranges;
        self.items.label_ranges = label_ranges;
        self.items.hint_ranges = hint_ranges;
        self.items.held_ranges = held_ranges;
        family_ranges.sort();
        self.items.family_ranges = family_ranges;
        let served = self.items.served.len() as u64;
        if !crate::macros::in_macro_world() {
            crate::incremental::update_census(|census| {
                census.item_records_served += served;
                census.item_records_stale += stale;
            });
        }
    }

    /// Re-bases an id of a record's own window to this analysis's window;
    /// an id of the prefix is kept. `None` for an id the record should
    /// never have carried.
    fn rebase_item_id(record: &ItemRecord, start_now: u32, id: Id) -> Option<Id> {
        let offset = id.0.wrapping_sub(record.entity_start);
        if offset < record.entity_span {
            Some(Id(start_now + offset))
        } else if id.0 < record.entity_start {
            Some(id)
        } else {
            None
        }
    }

    /// Splices the served items' remembered output back in: the
    /// diagnostics and warnings with their spans re-based to where the item
    /// now sits in its file, the R10 structure keys, the B575 enrolments and
    /// the class D rows with their ids re-based to the item's window.
    pub(super) fn replay_item_records(&mut self) {
        let served = std::mem::take(&mut self.items.served);
        for (window, record) in &served {
            let item = &self.id_windows.windows[*window];
            let source = item.source;
            let start_now = item.entities.start;
            let region_now = self
                .items
                .facts
                .get(window)
                .map_or(record.region_start, |facts| facts.region_start);
            let shift = |span: Span| -> Span {
                let delta = region_now as i64 - record.region_start as i64;
                Span {
                    start: (span.start as i64 + delta) as usize,
                    end: (span.end as i64 + delta) as usize,
                }
            };
            let relocate = |error: &Error| -> Error {
                let mut error = error.clone();
                error.span = shift(error.span);
                if let Some(note) = &mut error.note
                    && note.source.is_none_or(|note_source| note_source == source)
                {
                    note.span = shift(note.span);
                }
                for hop in &mut error.trace {
                    if hop
                        .note
                        .source
                        .is_none_or(|note_source| note_source == source)
                    {
                        hop.note.span = shift(hop.note.span);
                    }
                }
                error
            };
            let from = self.diagnostics.len();
            self.diagnostics
                .extend(record.module.diagnostics.iter().map(relocate));
            self.attribute_new_diagnostics(from, source);
            self.warning_sources
                .resize(self.warnings.len(), SourceId(0));
            for warning in &record.module.warnings {
                self.warnings.push(relocate(warning));
                self.warning_sources.push(source);
            }
            for key in &record.module.container_structures {
                self.reported_container_structures.insert(key.clone());
            }
            let rebase = |id: Id| Self::rebase_item_id(record, start_now, id);
            for &(anchor, call, view, root) in &record.module.suspension_crossings {
                if let (Some(anchor), Some(call), Some(view)) =
                    (rebase(anchor), rebase(call), rebase(view))
                    && let Some(root) = root.map_or(Some(None), |root| rebase(root).map(Some))
                {
                    self.view_suspension_checks
                        .crossings
                        .push((anchor, call, view, root));
                }
            }
            for &(function_id, parameter_id, form) in &record.module.suspension_signatures {
                if let (Some(function_id), Some(parameter_id)) =
                    (rebase(function_id), rebase(parameter_id))
                {
                    self.view_suspension_checks
                        .signatures
                        .push((function_id, parameter_id, form));
                }
            }
            for &(closure_id, reference_id) in &record.module.suspension_captures {
                let (Some(closure_id), Some(reference_id)) =
                    (rebase(closure_id), rebase(reference_id))
                else {
                    continue;
                };
                let name = match self.expr_id_to_expr_map.get(&reference_id) {
                    Some(Expr::Local(binding_id)) => self
                        .variables
                        .get(binding_id)
                        .map(|variable| variable.name)
                        .unwrap_or("the view"),
                    _ => "the view",
                };
                self.view_suspension_checks
                    .captures
                    .push((closure_id, reference_id, name));
            }
            if let Some(tables) = &record.module.tables {
                self.restored_tables.absorb_rebased(tables, &rebase, &shift);
            }
        }
        self.items.served = served;
    }

    /// Whether a diagnostic's secondary locations fit an item's record: a
    /// note or hop in the item's own file lies inside the item's region
    /// (re-based with it on replay); one in another file names a stored
    /// prefix source, whose text is validated per hit. Anything else — a
    /// note into another hot item, into the entry, into generated code —
    /// is text that moves under the record.
    fn item_record_fits(
        &self,
        error: &Error,
        source: SourceId,
        region: (usize, usize),
        prefix_len: u32,
    ) -> bool {
        let inside = |span: Span| span.start >= region.0 && span.end <= region.1;
        let fits = |note_source: Option<SourceId>, span: Span| match note_source {
            None => inside(span),
            Some(other) if other == source => inside(span),
            Some(other) => other.0 != 0 && other.0 < prefix_len && other != DERIVED_SOURCE,
        };
        inside(error.span)
            && error
                .note
                .as_ref()
                .is_none_or(|note| fits(note.source, note.span))
            && error
                .trace
                .iter()
                .all(|hop| fits(hop.note.source, hop.note.span))
    }

    /// Harvests what ONE Class A check derived for the laid-out items, per
    /// item — the item-grain twin of [`Self::record_reusable_window`], filed
    /// by the diagnostic's span.
    pub(super) fn record_item_window(&mut self, diagnostics_from: usize, warnings_from: usize) {
        if !self.items.enabled {
            return;
        }
        let prefix_len = self.reuse_prefix_len;
        for index in diagnostics_from..self.diagnostics.len() {
            let source = self.diagnostic_source_at(index);
            if source == DERIVED_SOURCE || !self.items.spans.contains_key(&source) {
                continue;
            }
            let error = &self.diagnostics[index];
            self.file_item_output(source, error.clone(), prefix_len, false);
        }
        for index in warnings_from..self.warnings.len() {
            let source = self
                .warning_sources
                .get(index)
                .copied()
                .unwrap_or(SourceId(0));
            if source == DERIVED_SOURCE || !self.items.spans.contains_key(&source) {
                continue;
            }
            let warning = &self.warnings[index];
            self.file_item_output(source, warning.clone(), prefix_len, true);
        }
    }

    fn file_item_output(&mut self, source: SourceId, error: Error, prefix_len: u32, warning: bool) {
        // An empty span names no place in the file (the analyzer's
        // `EMPTY_SPAN`), so it names no item either.
        let placed = (error.span.start < error.span.end)
            .then(|| self.item_window_at(source, error.span.start))
            .flatten();
        let Some(window) = placed else {
            // A Class A diagnostic of a laid-out file that no item's region
            // holds: nothing in the file can be served, since the entity
            // it came from is some item's and that item's record would
            // replay without it.
            if let Some(rows) = self.items.spans.get(&source) {
                for (_, _, window) in rows {
                    self.items.unrecordable.insert(*window);
                }
            }
            return;
        };
        let region = self
            .items
            .spans
            .get(&source)
            .and_then(|rows| rows.iter().find(|(_, _, index)| *index == window))
            .map(|(start, end, _)| (*start, *end))
            .unwrap_or((0, 0));
        if self.items.served_set.contains(&window)
            || !self.item_record_fits(&error, source, region, prefix_len)
        {
            self.items.unrecordable.insert(window);
            return;
        }
        let derived = self.items.derived.entry(window).or_default();
        if warning {
            derived.warnings.push(error);
        } else {
            derived.diagnostics.push(error);
        }
    }

    /// An R10 structure key reported from a site in a laid-out item, filed
    /// under the item (the per-source twin is [`Self::note_container_structure`]).
    pub(super) fn note_item_container_structure(&mut self, window: Option<usize>, key: &str) {
        let Some(window) = window else { return };
        if !self.items.enabled || self.items.served_set.contains(&window) {
            return;
        }
        self.items
            .derived
            .entry(window)
            .or_default()
            .container_structures
            .push(key.to_string());
    }

    /// The laid-out window `id` belongs to, for filing a row under its item
    /// — `None` for an id of the prefix, the tail or a served item.
    pub(super) fn item_window_of(&self, id: Id) -> Option<usize> {
        if !self.items.enabled {
            return None;
        }
        let window = self.id_windows.window_of_entity(id)?;
        (window >= self.id_windows.inherited
            && !self.items.served_set.contains(&window)
            && self.item_source(self.id_windows.windows[window].source))
        .then_some(window)
    }

    /// Builds this analysis's item records from what it derived and the
    /// class D rows partitioned per item, answering the records to file and
    /// the keys to purge (items whose output a record cannot carry).
    pub(super) fn take_item_records(
        &mut self,
        mut tables: HashMap<u32, ModuleTables>,
        drop_nominals_world: u64,
        labels_recorded: bool,
        hints_recorded: bool,
        families: super::row_families::FamilySet,
    ) -> (Vec<(ItemKey, ItemRecord)>, Vec<ItemKey>) {
        let mut filed = Vec::new();
        let mut purge = Vec::new();
        if !self.items.enabled {
            return (filed, purge);
        }
        let mut derived = std::mem::take(&mut self.items.derived);
        // B575: the enrolment rows, under the item whose body enrolled them.
        for &(anchor, call, view, root) in &self.view_suspension_checks.crossings {
            if let Some(window) = self.item_window_of(anchor) {
                derived
                    .entry(window)
                    .or_default()
                    .suspension_crossings
                    .push((anchor, call, view, root));
            }
        }
        for &(function_id, parameter_id, form) in &self.view_suspension_checks.signatures {
            if let Some(window) = self.item_window_of(function_id) {
                derived
                    .entry(window)
                    .or_default()
                    .suspension_signatures
                    .push((function_id, parameter_id, form));
            }
        }
        for &(closure_id, reference_id, _) in &self.view_suspension_checks.captures {
            if let Some(window) = self.item_window_of(closure_id) {
                derived
                    .entry(window)
                    .or_default()
                    .suspension_captures
                    .push((closure_id, reference_id));
            }
        }
        let prefix_end = self.prefix_entity_end.unwrap_or(0);
        for window in self.laid_out_item_windows() {
            if self.items.served_set.contains(&window) {
                continue;
            }
            let Some(facts) = self.items.facts.get(&window) else {
                continue;
            };
            let item = &self.id_windows.windows[window];
            if self.items.unrecordable.contains(&window) {
                purge.push(facts.key);
                continue;
            }
            let mut module = derived.remove(&window).unwrap_or_default();
            // Every recorded item carries a table slice, the empty ones
            // included: "this item contributed no rows" is most items'
            // answer, and a missing slice would recompute them forever.
            let mut slice = tables.remove(&(window as u32)).unwrap_or_default();
            slice.drop_nominals_world = drop_nominals_world;
            slice.labels_recorded = labels_recorded;
            slice.hints_recorded = hints_recorded;
            slice.families = families;
            // The held rows ride only when every id they carry is the item's
            // own or the prefix's; otherwise the item stays live for the
            // held passes (`held_tables.rs`) and keeps the rest of its record.
            slice.held_recorded = slice.held.fits(&|id: Id| {
                (id.0 >= item.entities.start && id.0 < item.entities.end) || id.0 < prefix_end
            });
            if !slice.held_recorded {
                slice.held = Default::default();
            }
            // A cross-file read of another HOT window's binding needs no row:
            // the item is served only when that window is clean, and a clean
            // window's binding is restored with it (the dirty closure's read
            // edge). A prefix binding's row stays — the prefix module can be
            // dirty while this item is served.
            slice.foreign_reads.retain(|(_, binding)| {
                binding.0 < prefix_end
                    || (binding.0 >= item.entities.start && binding.0 < item.entities.end)
            });
            module.tables = Some(slice);
            module.container_structures.sort_unstable();
            module.container_structures.dedup();
            module.bound_recordable = false;
            // Every id the record carries is the item's own or the prefix's.
            let own = |id: Id| -> bool {
                (id.0 >= item.entities.start && id.0 < item.entities.end) || id.0 < prefix_end
            };
            let rows_fit = module
                .suspension_crossings
                .iter()
                .all(|(a, b, c, d)| own(*a) && own(*b) && own(*c) && d.is_none_or(own))
                && module
                    .suspension_signatures
                    .iter()
                    .all(|(a, b, _)| own(*a) && own(*b))
                && module
                    .suspension_captures
                    .iter()
                    .all(|(a, b)| own(*a) && own(*b))
                && module.tables.as_ref().is_none_or(|tables| {
                    tables.last_uses.iter().all(|id| own(*id))
                        && tables.last_use_opaque.iter().all(|id| own(*id))
                        && tables.last_use_unreached.iter().all(|id| own(*id))
                        && tables
                            .last_use_statements
                            .iter()
                            .chain(&tables.declaration_statements)
                            .all(|(id, chain)| own(*id) && chain.iter().all(|id| own(*id)))
                        && tables
                            .foreign_reads
                            .iter()
                            .all(|(region, binding)| own(*region) && own(*binding))
                        && tables.r11_seed_calls.iter().all(|id| own(*id))
                        && tables.moves.fits(&own)
                        && tables.lazy.fits(&own)
                        && tables.hmr.fits(&own)
                        && tables.resource_slots.is_empty()
                        && tables
                            .stage_hints
                            .iter()
                            .all(|hint| own(hint.id) && own(hint.chain))
                        && tables.bumps.iter().all(|(id, _)| own(*id))
                        && tables.clone_sites.iter().all(|(id, _)| own(*id))
                        && tables.return_clone_sites.iter().all(|(id, _)| own(*id))
                        && tables.return_view_reads.iter().all(|id| own(*id))
                        && tables.scalar_view_calls.iter().all(|id| own(*id))
                        && tables.scalar_view_refs.iter().all(|id| own(*id))
                        && tables.drop_roots.iter().all(|id| own(*id))
                        && tables.expr_types.iter().all(|(id, _)| own(*id))
                        && tables.declaration_labels.iter().all(|(id, _)| own(*id))
                });
            if !rows_fit {
                purge.push(facts.key);
                continue;
            }
            filed.push((
                facts.key,
                ItemRecord {
                    text_hash: facts.text_hash,
                    region_start: facts.region_start,
                    entity_start: item.entities.start,
                    entity_span: item.entities.span(),
                    walk: item.walk,
                    signature: facts.signature,
                    deps: facts.deps.clone(),
                    module,
                },
            ));
        }
        let count = filed.len() as u64;
        if !crate::macros::in_macro_world() {
            crate::incremental::update_census(|census| census.item_records_filed += count);
        }
        (filed, purge)
    }
}

/// The world's item records for `key`, if the record still describes this
/// world's prefix content. The records are shared (`Arc`), so a session's
/// keystroke clones a map of pointers.
pub(super) fn checked_cache_lookup_items(
    key: &BaseCacheKey,
    source_hashes: &[u64],
) -> Option<HashMap<ItemKey, Arc<ItemRecord>>> {
    let cache = CHECKED_CACHE.get()?;
    let mut state = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let recorded = state.records.get(key)?;
    if recorded.source_hashes.len() != source_hashes.len()
        || recorded.source_hashes[1..] != source_hashes[1..]
    {
        return None;
    }
    let served = recorded.per_item.clone();
    let tick = state.next_tick();
    if let Some(recorded) = state.records.get_mut(key) {
        recorded.last_used = tick;
    }
    Some(served)
}

/// Files this analysis's item records beside the checks record the
/// diagnostics store wrote for this key — never creating one. A purged key
/// loses whatever an earlier analysis remembered about the item.
pub(super) fn checked_cache_store_items(
    key: &BaseCacheKey,
    source_hashes: &[u64],
    items: Vec<(ItemKey, ItemRecord)>,
    purge: &[ItemKey],
) {
    if items.is_empty() && purge.is_empty() {
        return;
    }
    let Some(cache) = CHECKED_CACHE.get() else {
        return;
    };
    let mut state = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let previous_bytes = state.records.get(key).map_or(0, |record| record.bytes);
    let Some(recorded) = state.records.get_mut(key) else {
        return;
    };
    if recorded.source_hashes.len() != source_hashes.len()
        || recorded.source_hashes[1..] != source_hashes[1..]
    {
        return;
    }
    for item_key in purge {
        if let Some(removed) = recorded.per_item.remove(item_key) {
            recorded.bytes = recorded.bytes.saturating_sub(removed.bytes());
        }
    }
    for (item_key, record) in items {
        let now = record.bytes();
        if let Some(displaced) = recorded.per_item.insert(item_key, Arc::new(record)) {
            recorded.bytes = recorded.bytes.saturating_sub(displaced.bytes());
        }
        recorded.bytes += now;
    }
    let bytes = recorded.bytes;
    let tick = state.next_tick();
    if let Some(recorded) = state.records.get_mut(key) {
        recorded.last_used = tick;
    }
    state.bytes = state
        .bytes
        .saturating_sub(previous_bytes)
        .saturating_add(bytes);
    state.evict_to_budget(Some(key));
}
