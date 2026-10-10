//! M110 S5 — the id-window SPIKE (`VILAN_ID_WINDOWS`; `incremental-analysis.md`
//! §3.3 door C, `analyzer-pass-map.md` §7.3).
//!
//! Today the three id counters (`entity_id`, `type_id`, `scope_id`) are dense:
//! every walk continues from the last id, and the constraint fixpoint and the
//! checks mint after every walk in THEIR order, interleaved across items. An
//! item-level WINDOW gives each top-level item its own range in every lane,
//! with room after it, laid out in load order; an id minted for the item
//! later (by the fixpoint, for a constraint anchored inside it) lands in the
//! item's window instead of the global tail. The spike measures whether that
//! is possible without changing an answer, and counts what cannot be
//! windowed.
//!
//! Modes (`VILAN_ID_WINDOWS`):
//!
//! - unset / `0` — off: the counters are the dense ones, nothing is recorded.
//! - `census` — windows RECORDED (each item's exact minted range, no room),
//!   nothing relocated: every id is the one today's compiler mints. The
//!   anchoring and the census run, so the numbers can be read against an
//!   unchanged analysis.
//! - `walk` — windows laid out with room (`VILAN_ID_WINDOW_SLACK` percent of
//!   the walk's mints, default 100): the counters jump past each window's
//!   room, every later mint goes to the global tail (stamped with its anchor).
//!   Proves nothing reads the counters as dense.
//! - `types` — as `walk`, and a TYPE id minted for a known anchor lands in the
//!   anchor's window while it has room (then spills to the tail, counted).
//! - `all` / `1` — as `types` for every lane (entities and scopes too).
//!
//! Plants (`VILAN_ID_WINDOWS_PLANT`): `anchor-off` (the fixpoint never sets
//! its anchor — every fixpoint mint is unanchored), `cross` (a relocated mint
//! lands in the PREVIOUS item's window — the containment count goes red).
//!
//! The census (`[vilan windows]` on stderr, and [`IdWindowsReport`] on the
//! `Program`): per lane, the mints made while a window was being laid out
//! (the walk's own), the walk-phase mints made with no window open (the
//! drain's module nodes, prelude seeds, generated-expansion scopes), the
//! anchored mints relocated into their window / spilled / left in the tail,
//! and the unanchored ones by phase (the base resolve's other stages, `build`,
//! the checks — §4.1's post-settle mints have no anchor today); the
//! pre-settle slot WRITES classified by the writing constraint's anchor (own
//! window / another item's window / the tail — the "shared slot" measure B77
//! and B95 care about, and the number S7 needs); and, over the final tables,
//! which window each expression's type slot sits in (own / another item's /
//! the tail).

use crate::analyzer::SourceId;
use crate::id::Id;
use crate::type_::TypeId;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    #[default]
    Off,
    Census,
    Walk,
    Types,
    All,
}

impl Mode {
    /// Whether this mode lays windows out with room (and jumps the counters).
    pub fn lays_out(self) -> bool {
        matches!(self, Mode::Walk | Mode::Types | Mode::All)
    }

    /// Whether an anchored mint in `lane` is relocated into its window.
    pub fn relocates(self, lane: Lane) -> bool {
        match self {
            Mode::Off | Mode::Census | Mode::Walk => false,
            Mode::Types => lane == Lane::Type,
            Mode::All => true,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::Off => "off",
            Mode::Census => "census",
            Mode::Walk => "walk",
            Mode::Types => "types",
            Mode::All => "all",
        }
    }
}

thread_local! {
    static FORCED: std::cell::Cell<Option<Mode>> = const { std::cell::Cell::new(None) };
    static FORCED_PLANT: std::cell::Cell<Option<Plant>> = const { std::cell::Cell::new(None) };
}

/// Forces the mode on this thread (tests); `None` restores the environment's.
pub fn force_mode(mode: Option<Mode>) {
    FORCED.with(|forced| forced.set(mode));
}

/// Forces a plant on this thread (tests); `None` restores the environment's.
pub fn force_plant(plant: Option<Plant>) {
    FORCED_PLANT.with(|forced| forced.set(plant));
}

fn env_mode() -> Mode {
    static MODE: std::sync::OnceLock<Mode> = std::sync::OnceLock::new();
    *MODE.get_or_init(|| match std::env::var("VILAN_ID_WINDOWS").as_deref() {
        Ok("census") => Mode::Census,
        Ok("walk") => Mode::Walk,
        Ok("types") => Mode::Types,
        Ok("all") | Ok("1") => Mode::All,
        _ => Mode::Off,
    })
}

/// The mode this analysis runs under.
pub fn mode() -> Mode {
    FORCED.with(|forced| forced.get()).unwrap_or_else(env_mode)
}

/// `VILAN_ID_WINDOW_SLACK`: the room after an item's walk, as a percent of
/// the ids the walk minted (default 100: a window is twice its walk), never
/// less than [`MIN_SLACK`] ids.
pub fn slack_percent() -> u32 {
    static SLACK: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *SLACK.get_or_init(|| {
        std::env::var("VILAN_ID_WINDOW_SLACK")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(100)
    })
}

/// The least room a window keeps in every lane.
pub const MIN_SLACK: u32 = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Plant {
    AnchorOff,
    Cross,
}

fn env_plant() -> Option<Plant> {
    static PLANT: std::sync::OnceLock<Option<Plant>> = std::sync::OnceLock::new();
    *PLANT.get_or_init(|| match std::env::var("VILAN_ID_WINDOWS_PLANT").as_deref() {
        Ok("anchor-off") => Some(Plant::AnchorOff),
        Ok("cross") => Some(Plant::Cross),
        _ => None,
    })
}

pub fn plant() -> Option<Plant> {
    FORCED_PLANT.with(|forced| forced.get()).or_else(env_plant)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lane {
    Entity,
    Type,
    Scope,
}

/// Where the analysis is, for the census of unanchored mints.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Phase {
    /// The module, lib, generated and entry walks (and the drain around them).
    #[default]
    Walk,
    /// `resolve_world` before the entry (the base).
    Resolve,
    /// `build()`: the second resolve and `finalize_build`.
    Build,
    /// After `types_settled`: the checks and the extraction tail.
    Checks,
}

impl Phase {
    fn index(self) -> usize {
        match self {
            Phase::Walk => 0,
            Phase::Resolve => 1,
            Phase::Build => 2,
            Phase::Checks => 3,
        }
    }
}

/// One lane of one window: `[start, end)`; `cursor` is the next free id for a
/// relocated mint (the walk's mints occupy `[start, cursor)` at close).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Range {
    pub start: u32,
    pub end: u32,
    pub cursor: u32,
}

impl Range {
    pub fn contains(&self, id: u32) -> bool {
        self.start <= id && id < self.end
    }

    pub fn span(&self) -> u32 {
        self.end - self.start
    }

    pub fn used(&self) -> u32 {
        self.cursor - self.start
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemWindow {
    /// The top-level node's id (the item), known once its walk returns.
    pub item: Option<Id>,
    pub source: SourceId,
    pub entities: Range,
    pub types: Range,
    pub scopes: Range,
    /// A relocated mint found no room here (counted once per window).
    pub overflowed: bool,
    /// Relocated mints that found no room here, per lane (entity, type,
    /// scope) — with the lane's `used()`, the item's whole demand.
    pub spilled: [u32; 3],
    /// The walk's own mints per lane, fixed at close.
    pub walk: [u32; 3],
}

impl ItemWindow {
    fn lane(&self, lane: Lane) -> &Range {
        match lane {
            Lane::Entity => &self.entities,
            Lane::Type => &self.types,
            Lane::Scope => &self.scopes,
        }
    }

    fn lane_mut(&mut self, lane: Lane) -> &mut Range {
        match lane {
            Lane::Entity => &mut self.entities,
            Lane::Type => &mut self.types,
            Lane::Scope => &mut self.scopes,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaneCensus {
    /// Minted while an item's window was being laid out: the walk's own.
    pub walk_in_window: u64,
    /// Minted in the walk phase with no window open (module nodes, seeds).
    pub walk_unwindowed: u64,
    /// Minted later for a known anchor and relocated into its window.
    pub anchored_in_window: u64,
    /// Minted later for a known anchor whose window had no room: the tail.
    pub anchored_spilled: u64,
    /// Minted later for a known anchor, left in the tail (the mode does not
    /// relocate this lane).
    pub anchored_tail: u64,
    /// Minted later with no anchor, by phase (`Phase::index`).
    pub unanchored: [u64; 4],
    /// A relocated mint that landed OUTSIDE its anchor's window — zero by
    /// construction, nonzero under the `cross` plant.
    pub relocated_outside_anchor: u64,
}

impl LaneCensus {
    pub fn anchored(&self) -> u64 {
        self.anchored_in_window + self.anchored_spilled + self.anchored_tail
    }

    pub fn unanchored_total(&self) -> u64 {
        self.unanchored.iter().sum()
    }

    pub fn total(&self) -> u64 {
        self.walk_in_window + self.walk_unwindowed + self.anchored() + self.unanchored_total()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Census {
    pub entities: LaneCensus,
    pub types: LaneCensus,
    pub scopes: LaneCensus,
    /// Pre-settle world-changing slot writes, by the writing constraint's
    /// anchor: into the anchor's own window, into ANOTHER item's window, into
    /// the tail (a slot no window owns), and writes made with no anchor.
    pub writes_own: u64,
    pub writes_other: u64,
    pub writes_tail: u64,
    pub writes_unanchored: u64,
    /// Windows whose relocated mints spilled.
    pub overflowed: u64,
}

impl Census {
    fn lane_mut(&mut self, lane: Lane) -> &mut LaneCensus {
        match lane {
            Lane::Entity => &mut self.entities,
            Lane::Type => &mut self.types,
            Lane::Scope => &mut self.scopes,
        }
    }
}

/// The final-table containment census: each expression's type slot, by the
/// window it sits in relative to the expression's own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Containment {
    /// The slot is in the expression's own item window.
    pub own: u64,
    /// The slot is in ANOTHER item's window.
    pub foreign: u64,
    /// The slot is in no window (the tail: unanchored or spilled mints).
    pub tail: u64,
    /// The expression itself is in no window (drain-minted, synthetic).
    pub unwindowed_expr: u64,
}

/// What the `Program` carries when the spike is on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdWindowsReport {
    pub mode: Mode,
    pub windows: u64,
    pub census: Census,
    pub containment: Containment,
    /// Per lane: ids the windows span (room included) and ids the counter
    /// reached — the slack's cost.
    pub entity_span: u64,
    pub entity_high_water: u64,
    pub type_span: u64,
    pub type_high_water: u64,
    pub scope_span: u64,
    pub scope_high_water: u64,
    /// The windows themselves, in layout order (the pins read the layout).
    pub layout: Vec<ItemWindow>,
}

impl IdWindowsReport {
    /// `VILAN_ID_WINDOWS_DUMP=<path>`: one row per window, appended — the
    /// item, its source, and per lane the walk's mints, the relocated mints
    /// that fit and the ones that spilled (the per-item demand the sizing
    /// policy must meet).
    pub fn dump(&self) {
        let Ok(path) = std::env::var("VILAN_ID_WINDOWS_DUMP") else {
            return;
        };
        use std::io::Write;
        let Ok(mut file) = std::fs::OpenOptions::new().append(true).create(true).open(path)
        else {
            return;
        };
        let _ = writeln!(
            file,
            "# mode={} windows={} | item source | entity walk/relocated/spilled | type ... | scope ...",
            self.mode.name(),
            self.windows
        );
        for window in &self.layout {
            let lane = |range: &Range, walk: u32, spilled: u32| {
                // The walk's mints are `[start, start + walk)`; the relocated
                // ones that fit follow them up to the cursor.
                format!("{walk}/{}/{spilled}", range.used() - walk)
            };
            let _ = writeln!(
                file,
                "{} {} {} {} {}",
                window.item.map_or(0, |item| item.0),
                window.source.0,
                lane(&window.entities, window.walk[0], window.spilled[0]),
                lane(&window.types, window.walk[1], window.spilled[1]),
                lane(&window.scopes, window.walk[2], window.spilled[2]),
            );
        }
    }

    pub fn line(&self) -> String {
        let lane = |name: &str, lane: &LaneCensus| {
            format!(
                "{name}: walk={} unwindowed={} anchored-in={} spilled={} anchored-tail={} \
                 unanchored=w{}/r{}/b{}/c{} outside-anchor={}",
                lane.walk_in_window,
                lane.walk_unwindowed,
                lane.anchored_in_window,
                lane.anchored_spilled,
                lane.anchored_tail,
                lane.unanchored[0],
                lane.unanchored[1],
                lane.unanchored[2],
                lane.unanchored[3],
                lane.relocated_outside_anchor,
            )
        };
        format!(
            "[vilan windows] mode={} windows={} overflowed={} | {} | {} | {} | writes own={} \
             other={} tail={} unanchored={} | expr-types own={} foreign={} tail={} \
             unwindowed-expr={} | span entity={}/{} type={}/{} scope={}/{}",
            self.mode.name(),
            self.windows,
            self.census.overflowed,
            lane("entities", &self.census.entities),
            lane("types", &self.census.types),
            lane("scopes", &self.census.scopes),
            self.census.writes_own,
            self.census.writes_other,
            self.census.writes_tail,
            self.census.writes_unanchored,
            self.containment.own,
            self.containment.foreign,
            self.containment.tail,
            self.containment.unwindowed_expr,
            self.entity_span,
            self.entity_high_water,
            self.type_span,
            self.type_high_water,
            self.scope_span,
            self.scope_high_water,
        )
    }
}

#[derive(Clone, Debug, Default)]
pub struct IdWindows {
    pub mode: Mode,
    pub windows: Vec<ItemWindow>,
    /// The window being laid out (a top-level item's walk is running).
    pub laying_out: Option<usize>,
    /// The anchor window for mints outside the walk (the constraint's).
    pub anchor: Option<usize>,
    /// The entity the anchor was last resolved from, to skip the search when
    /// consecutive constraints share it.
    anchor_entity: Option<Id>,
    pub phase: Phase,
    pub census: Census,
}

impl IdWindows {
    pub fn new() -> Self {
        Self {
            mode: mode(),
            ..Default::default()
        }
    }

    pub fn on(&self) -> bool {
        self.mode != Mode::Off
    }

    /// Opens a window at the counters' current values. The caller walks the
    /// item, then [`Self::close`]s it.
    pub fn open(&mut self, source: SourceId, entity: u32, type_: u32, scope: u32) -> Option<usize> {
        if !self.on() {
            return None;
        }
        let range = |start: u32| Range {
            start,
            end: start,
            cursor: start,
        };
        self.windows.push(ItemWindow {
            item: None,
            source,
            entities: range(entity),
            types: range(type_),
            scopes: range(scope),
            overflowed: false,
            spilled: [0; 3],
            walk: [0; 3],
        });
        let index = self.windows.len() - 1;
        self.laying_out = Some(index);
        Some(index)
    }

    /// Closes the window: its walk's mints are `[start, counter)`; in a
    /// laying-out mode the room is added and the counters jump past it. The
    /// returned ranges are the (type) ids the caller pads its dense census
    /// with — `(from, to)` per lane, empty when nothing jumped.
    pub fn close(
        &mut self,
        index: usize,
        item: Id,
        entity: &mut u32,
        type_: &mut u32,
        scope: &mut u32,
    ) -> [(u32, u32); 3] {
        debug_assert_eq!(self.laying_out, Some(index));
        self.laying_out = None;
        let lays_out = self.mode.lays_out();
        let slack = slack_percent();
        let mut padded = [(0, 0); 3];
        let window = &mut self.windows[index];
        window.item = Some(item);
        for (slot, (lane, counter)) in [
            (Lane::Entity, entity),
            (Lane::Type, type_),
            (Lane::Scope, scope),
        ]
        .into_iter()
        .enumerate()
        {
            let range = window.lane_mut(lane);
            range.cursor = *counter;
            let minted = *counter - range.start;
            range.end = if lays_out {
                let room = ((minted as u64) * (slack as u64) / 100).max(MIN_SLACK as u64) as u32;
                *counter + room
            } else {
                *counter
            };
            padded[slot] = (*counter, range.end);
            *counter = range.end;
            window.walk[slot] = minted;
        }
        padded
    }

    /// Mints one id in `lane`: from the global counter while a window is being
    /// laid out or when nothing anchors the mint, from the anchor's window
    /// when the mode relocates the lane and the window has room. Returns the
    /// id and whether it came from the global counter (the caller then
    /// records its dense census row).
    pub fn mint(&mut self, lane: Lane, global: &mut u32) -> (u32, bool) {
        let phase = self.phase;
        if let Some(index) = self.laying_out {
            debug_assert!(phase == Phase::Walk || !self.on());
            let _ = index;
            self.census.lane_mut(lane).walk_in_window += 1;
            let id = *global;
            *global += 1;
            return (id, true);
        }
        match self.anchor {
            None => {
                let census = self.census.lane_mut(lane);
                if phase == Phase::Walk {
                    census.walk_unwindowed += 1;
                } else {
                    census.unanchored[phase.index()] += 1;
                }
                let id = *global;
                *global += 1;
                (id, true)
            }
            Some(anchor) => {
                if !self.mode.relocates(lane) {
                    self.census.lane_mut(lane).anchored_tail += 1;
                    let id = *global;
                    *global += 1;
                    return (id, true);
                }
                // The `cross` plant mints from the previous item's window.
                let target = match plant() {
                    Some(Plant::Cross) if anchor > 0 => anchor - 1,
                    _ => anchor,
                };
                let window = &mut self.windows[target];
                let range = window.lane_mut(lane);
                if range.cursor < range.end {
                    let id = range.cursor;
                    range.cursor += 1;
                    let census = self.census.lane_mut(lane);
                    census.anchored_in_window += 1;
                    if !self.windows[anchor].lane(lane).contains(id) {
                        self.census.lane_mut(lane).relocated_outside_anchor += 1;
                    }
                    (id, false)
                } else {
                    if !window.overflowed {
                        window.overflowed = true;
                        self.census.overflowed += 1;
                    }
                    window.spilled[match lane {
                        Lane::Entity => 0,
                        Lane::Type => 1,
                        Lane::Scope => 2,
                    }] += 1;
                    self.census.lane_mut(lane).anchored_spilled += 1;
                    let id = *global;
                    *global += 1;
                    (id, true)
                }
            }
        }
    }

    fn window_of(&self, lane: Lane, id: u32) -> Option<usize> {
        if self.windows.is_empty() {
            return None;
        }
        let index = self
            .windows
            .partition_point(|window| window.lane(lane).start <= id);
        let candidate = index.checked_sub(1)?;
        self.windows[candidate]
            .lane(lane)
            .contains(id)
            .then_some(candidate)
    }

    pub fn window_of_entity(&self, id: Id) -> Option<usize> {
        self.window_of(Lane::Entity, id.0)
    }

    pub fn window_of_type(&self, id: TypeId) -> Option<usize> {
        self.window_of(Lane::Type, id.0)
    }

    /// The fixpoint's hook: the constraint about to resolve is anchored at
    /// `anchor`, so its mints belong to that item's window.
    pub fn set_anchor(&mut self, anchor: Id) {
        if !self.on() || plant() == Some(Plant::AnchorOff) {
            return;
        }
        if self.anchor_entity == Some(anchor) {
            return;
        }
        self.anchor_entity = Some(anchor);
        self.anchor = self.window_of_entity(anchor);
    }

    pub fn clear_anchor(&mut self) {
        self.anchor = None;
        self.anchor_entity = None;
    }

    /// A pre-settle world-changing write to `slot`, classified by the anchor.
    pub fn note_slot_write(&mut self, slot: TypeId) {
        if !self.on() {
            return;
        }
        let Some(anchor) = self.anchor else {
            self.census.writes_unanchored += 1;
            return;
        };
        match self.window_of_type(slot) {
            Some(window) if window == anchor => self.census.writes_own += 1,
            Some(_) => self.census.writes_other += 1,
            None => self.census.writes_tail += 1,
        }
    }

    /// The source a type id belongs to by its window, when one holds it.
    pub fn source_of_type(&self, id: TypeId) -> Option<SourceId> {
        self.window_of_type(id).map(|index| self.windows[index].source)
    }

    pub fn report<'a>(
        &self,
        entity_high_water: u32,
        type_high_water: u32,
        scope_high_water: u32,
        expr_types: impl Iterator<Item = (Id, TypeId)>,
    ) -> IdWindowsReport {
        let mut containment = Containment::default();
        for (expr, type_id) in expr_types {
            match (self.window_of_entity(expr), self.window_of_type(type_id)) {
                (None, _) => containment.unwindowed_expr += 1,
                (Some(_), None) => containment.tail += 1,
                (Some(own), Some(other)) if own == other => containment.own += 1,
                (Some(_), Some(_)) => containment.foreign += 1,
            }
        }
        let span = |lane: Lane| -> u64 {
            self.windows
                .iter()
                .map(|window| u64::from(window.lane(lane).span()))
                .sum()
        };
        IdWindowsReport {
            mode: self.mode,
            windows: self.windows.len() as u64,
            census: self.census.clone(),
            containment,
            entity_span: span(Lane::Entity),
            entity_high_water: u64::from(entity_high_water),
            type_span: span(Lane::Type),
            type_high_water: u64::from(type_high_water),
            scope_span: span(Lane::Scope),
            scope_high_water: u64::from(scope_high_water),
            layout: self.windows.clone(),
        }
    }
}
