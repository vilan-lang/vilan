//! M110 — incremental analysis (`proposal/incremental-analysis.md`, ruled
//! Q1–Q10 as recommended): firewalls inside today's pipeline, each slice
//! measured, each behind the edit-replay differential.
//!
//! This module is the slices' MEASUREMENT and GATE surface; the firewalls
//! themselves live where the work they skip lives (S1's hot-set world in the
//! analyzer's load drain and base cache).
//!
//! - **S0's fingerprints** ([`fingerprints`]): per item, an INTERFACE
//!   fingerprint — its resolved signature (the inferred return included, Q4),
//!   its effects (async, contexts, platform requirement, the `borrows`/`bumps`
//!   verdicts, Q8) and whether it tracks its caller (`[track_caller]`) — and one GLOBAL-facts fingerprint: impl headers and the
//!   member names each provides, trait headers, the resource-declared types.
//!   Rendered from the program's own editor labels, so a fingerprint is a
//!   content hash of text: no `TypeId`, no address, nothing a later process
//!   could not recompute (Q7, the persistence rule).
//! - **S0's census** ([`Census`]): what one analysis re-walked, re-checked,
//!   replayed and asked of the base cache — counts, never clocks (Q9) — and
//!   the `[vilan phase] hot-set n/m interface-moved k global-moved b` line.
//! - **Q3's gate**: [`clean_analysis`] runs one analysis with every reuse off
//!   (no base-cache lookup or store, no checks record, no hot set), which is
//!   the "clean analysis" the differential and `VILAN_INCREMENTAL=verify`
//!   compare an incremental one against; [`render_observation`] is the
//!   id-free rendering they compare; [`Plant`] is the per-slice planted bug
//!   that proves the differential can go red.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::analyzer::{Program, SourceId};
use crate::fx::FxHashMap as HashMap;
use crate::id::Id;
use crate::target::Platform;

/// What `VILAN_INCREMENTAL` asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The default: no fingerprints, no verification. The hot-set world (S1)
    /// is not a mode — it is what the analysis does whenever a front end names
    /// a hot seed.
    Off,
    /// `VILAN_INCREMENTAL=measure`: fingerprint every analysis and compare it
    /// with the previous analysis of the same entry, so the phase line can say
    /// what moved (S0). Off by default because it renders and hashes every
    /// item, which is work no keystroke should pay for unasked — and the
    /// latency harness measures keystrokes with `VILAN_PHASE_TIMING` on.
    Measure,
    /// `VILAN_INCREMENTAL=verify` (Q3's dogfooding mode): everything `measure`
    /// does, and the language server runs a CLEAN analysis beside every
    /// incremental one and logs any difference with the edit that made it.
    Verify,
}

/// 0 = read the environment; otherwise `Mode as u8 + 1`.
static MODE_OVERRIDE: AtomicU8 = AtomicU8::new(0);

/// The mode in force: [`set_mode`]'s override, else `VILAN_INCREMENTAL` (read
/// once).
pub fn mode() -> Mode {
    match MODE_OVERRIDE.load(Ordering::Relaxed) {
        1 => Mode::Off,
        2 => Mode::Measure,
        3 => Mode::Verify,
        _ => {
            static FROM_ENV: OnceLock<Mode> = OnceLock::new();
            *FROM_ENV.get_or_init(|| match std::env::var("VILAN_INCREMENTAL").as_deref() {
                Ok("measure") => Mode::Measure,
                Ok("verify") => Mode::Verify,
                _ => Mode::Off,
            })
        }
    }
}

/// Overrides `VILAN_INCREMENTAL` for this process (`None` restores it) — the
/// test surface.
#[doc(hidden)]
pub fn set_mode(mode: Option<Mode>) {
    let value = match mode {
        None => 0,
        Some(Mode::Off) => 1,
        Some(Mode::Measure) => 2,
        Some(Mode::Verify) => 3,
    };
    MODE_OVERRIDE.store(value, Ordering::Relaxed);
}

/// Whether this analysis fingerprints its items (S0).
pub fn fingerprinting() -> bool {
    mode() != Mode::Off
}

// --- Q3: the clean analysis -------------------------------------------------

/// What a clean analysis on this thread keeps of the incremental machinery.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Clean {
    /// Not a clean analysis.
    No,
    /// The canonical analysis: nothing reused, no hot set.
    Canonical,
    /// Nothing reused, but the world built in the hot-set SHAPE the seeds ask
    /// for — what the differential compares emitted JS against (S1 walks the
    /// hot set after the prefix, so its declarations number, and emit, in a
    /// different order from the canonical world's; a hot-set program is never
    /// emitted, but its JS is still the sharpest check that nothing it reused
    /// was stale).
    KeepingHotShape,
}

thread_local! {
    static CLEAN: Cell<Clean> = const { Cell::new(Clean::No) };
}

/// Runs `body` with every incremental mechanism off for the analyses it makes
/// on THIS thread: the base cache is neither consulted nor written, so no
/// world is served, no checks record is replayed or filed, and no hot set is
/// deferred — the canonical analysis, from source, every time.
///
/// It is what the edit-replay differential and `VILAN_INCREMENTAL=verify`
/// compare an incremental analysis against. Thread-local because an analysis
/// runs on one thread and the cache is process-global: a clean analysis on one
/// thread must not switch reuse off for a neighbour, and must leave the cache
/// exactly as it found it.
pub fn clean_analysis<R>(body: impl FnOnce() -> R) -> R {
    with_clean(Clean::Canonical, body)
}

/// [`clean_analysis`], keeping the hot-set SHAPE the analysis's seeds ask for:
/// nothing is reused, and the world is built the way an incremental analysis
/// with the same seeds builds it. The differential's JS comparison runs against
/// this (see [`Clean::KeepingHotShape`]).
pub fn clean_analysis_keeping_hot_shape<R>(body: impl FnOnce() -> R) -> R {
    with_clean(Clean::KeepingHotShape, body)
}

fn with_clean<R>(mode: Clean, body: impl FnOnce() -> R) -> R {
    struct Restore(Clean);
    impl Drop for Restore {
        fn drop(&mut self) {
            CLEAN.with(|clean| clean.set(self.0));
        }
    }
    let _restore = Restore(CLEAN.with(|clean| clean.replace(mode)));
    body()
}

/// Whether the analysis on this thread is a clean one (either kind).
pub fn clean_requested() -> bool {
    CLEAN.with(Cell::get) != Clean::No
}

/// Whether this thread's clean analysis keeps the hot-set shape.
pub(crate) fn clean_keeps_hot_shape() -> bool {
    CLEAN.with(Cell::get) == Clean::KeepingHotShape
}

// --- the planted bugs (Q3's non-vacuity) ------------------------------------

/// A bug a slice plants so its differential can be watched going red
/// (`incremental-analysis.md` §6.2; M57's lesson — a differential whose corpus
/// never exercised the seam stayed green over a planted bug).
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plant {
    /// S1: treat the HOT modules as reusable — replay their remembered checks
    /// as if they were part of the stored prefix. A hot module's text moves
    /// every keystroke, so a diagnostic its edit adds or removes is served
    /// stale.
    HotSetReplay,
    /// S1: skip the content validation of the stored PREFIX on a hit — serve
    /// the world whatever its modules now say.
    PrefixUnvalidated,
    /// S1 / M121: drop the impl guard — serve a hot-set world even when one of
    /// its impls answers a question the stored prefix's resolve asked, so the
    /// prefix keeps the answer it gave without that impl.
    ImplGuardOff,
    /// S1: the hot set is the WHOLE package — every module re-walked per
    /// keystroke. Not wrong, just S1 undone; the re-walk counter pins must see
    /// it.
    WholePackageHot,
    /// S1: drop the use-inferred-binding guard — defer a hot module that
    /// imports a prefix binding whose type the first use decides.
    UseInferredGuardOff,
    /// S4: serve a remembered const site without asking its project reads
    /// again — an edited input file is then served stale.
    ConstCacheUnvalidated,
    /// S4: key a const site by its own expression and prelude only, without
    /// the world declarations it reaches — an edited callee is then served
    /// stale.
    ConstKeyWithoutWorld,
    /// S2a: the bound audit skips a reused module's sites but RECORDS nothing
    /// for them — a prefix module's bound refusal is then dropped on the hit.
    BoundAuditUnrecorded,
    /// S2a: a module's recorded audit is replayed without asking whether an
    /// impl that arrived after the stored world answers one of its questions
    /// — an impl moved in a hot module then leaves the prefix's old verdict
    /// in place.
    BoundRecordUnguarded,
    /// S2b: the tail skips rendering a reused module's labels but drops the
    /// restored rows — the prefix's hovers and declaration labels vanish.
    LabelTablesUnrecorded,
    /// S3a: the recorded post-rewrite cold graph is served to an analysis
    /// that applied NO rewrite — the log replayed over a tree that was never
    /// edited, so a prefix `run` reads as lowered to a call of its body while
    /// the tree still calls `run`.
    ContextLogUnguarded,
    /// S3b: a fixpoint is seeded from the recording analysis's SETTLED set,
    /// hot nodes included, instead of its cold-only result — an effect the
    /// hot edit removed is then served from the seed (a monotone fixpoint
    /// cannot shrink).
    PostSeedFromSettled,
}

impl Plant {
    fn code(self) -> u8 {
        match self {
            Plant::HotSetReplay => 1,
            Plant::PrefixUnvalidated => 2,
            Plant::ImplGuardOff => 3,
            Plant::WholePackageHot => 5,
            Plant::UseInferredGuardOff => 6,
            Plant::ConstCacheUnvalidated => 7,
            Plant::ConstKeyWithoutWorld => 8,
            Plant::BoundAuditUnrecorded => 9,
            Plant::BoundRecordUnguarded => 10,
            Plant::LabelTablesUnrecorded => 11,
            Plant::ContextLogUnguarded => 12,
            Plant::PostSeedFromSettled => 13,
        }
    }
}

static PLANTED: AtomicU8 = AtomicU8::new(0);

#[doc(hidden)]
pub fn set_plant(plant: Option<Plant>) {
    PLANTED.store(plant.map_or(0, Plant::code), Ordering::Relaxed);
}

pub(crate) fn planted(plant: Plant) -> bool {
    PLANTED.load(Ordering::Relaxed) == plant.code()
}

// --- S0: the per-analysis census (Q9's counters) ----------------------------

/// What ONE top-level analysis did, in counts (Q9: M105's tier 1). Thread-local
/// like every analyzer counter — an analysis runs on one thread — and reset at
/// the top-level door, so a macro world's nested analysis never mixes in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Census {
    /// The hot set's modules, the entry included (so a keystroke in the entry
    /// reads 1), or 0 when no front end named a hot seed.
    pub hot_modules: usize,
    /// The package's module files the entry's world reaches, the entry
    /// included — the hot set's denominator. 0 when no seed was named.
    pub package_modules: usize,
    /// Whether this analysis BUILT its world in the hot-set shape (S1): the
    /// prefix from the base cache or stored under the hot-set key, the hot set
    /// walked over it.
    pub hot_world: bool,
    /// Why a hot set this analysis measured was NOT built as one — the guard
    /// that refused it (`impl-reached`, `use-inferred-binding`,
    /// `refused-before`, a macro, an import of the entry, a module the shape
    /// cannot load).
    pub hot_refusal: Option<&'static str>,
    /// B553: whether the world was rebuilt to resolve once, after the entry
    /// walked, because an impl in the entry answers a question the pre-entry
    /// resolve asked.
    pub resolve_deferred: bool,
    /// Base-cache lookups this analysis made that HIT, MISSED, and the worlds
    /// it STORED.
    pub base_hits: u64,
    pub base_misses: u64,
    pub base_stores: u64,
    /// Sources this analysis walked: every one on a miss, the post-store
    /// region on a hit (the entry, plus the hot set under S1).
    pub sources_walked: usize,
    /// Sources whose Class A checks were replayed from a record (M19).
    pub records_replayed: usize,
    /// Functions whose Class A checks this analysis ran — every function
    /// outside the frozen (std) and replayed ranges.
    pub functions_checked: usize,
    /// M110 S4: `const` sites this analysis served from the const cache, and
    /// the ones it evaluated (`crate::const_cache`).
    pub const_cache_hits: u64,
    pub const_cache_misses: u64,
    /// M121 / B553: the impl-table questions the pre-entry resolve recorded —
    /// zero when no late file writes an impl on a type it does not declare
    /// (`analyzer::ReachFilter`), and on every base-cache hit.
    pub reach_questions: u64,
    /// M110 S2a: the bound audit's call sites this analysis SERVED from a
    /// reused module's record (skipped, their verdicts replayed) and the ones
    /// it CHECKED.
    pub bound_sites_served: u64,
    pub bound_sites_checked: u64,
    /// M110 S3 (Order 50): call graphs this analysis took from the world's
    /// post-pass record and extended with the hot set instead of building —
    /// the graph before the context rewrite, and the one after it (served
    /// only while the rewrite's cold log matches the record's).
    pub graphs_replayed: u64,
    pub context_log_replayed: u64,
    /// Post passes that iterated from a stored prefix seed (S3b).
    pub seeded_passes: u64,
    /// Post-pass records this analysis filed.
    pub post_records_filed: u64,
}

thread_local! {
    static CENSUS: Cell<Census> = const {
        Cell::new(Census {
            hot_modules: 0,
            package_modules: 0,
            hot_world: false,
            hot_refusal: None,
            resolve_deferred: false,
            base_hits: 0,
            base_misses: 0,
            base_stores: 0,
            sources_walked: 0,
            records_replayed: 0,
            functions_checked: 0,
            const_cache_hits: 0,
            const_cache_misses: 0,
            reach_questions: 0,
            bound_sites_served: 0,
            bound_sites_checked: 0,
            graphs_replayed: 0,
            context_log_replayed: 0,
            seeded_passes: 0,
            post_records_filed: 0,
        })
    };
}

/// This thread's census for its last top-level analysis.
pub fn census() -> Census {
    CENSUS.with(Cell::get)
}

pub(crate) fn reset_census() {
    CENSUS.with(|census| census.set(Census::default()));
}

pub(crate) fn update_census(change: impl FnOnce(&mut Census)) {
    CENSUS.with(|census| {
        let mut current = census.get();
        change(&mut current);
        census.set(current);
    });
}

// --- S0: interface and global-facts fingerprints ----------------------------

/// One analysis's fingerprints (S0): an interface hash per item, keyed by a
/// name that survives an edit (file, owning block, item name), and one hash
/// over the global facts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fingerprints {
    pub items: BTreeMap<String, u64>,
    pub global: u64,
    /// Functions whose interface renders an `unknown` type — Q10's count of
    /// a signature broken mid-typing.
    pub unknown_interfaces: usize,
}

/// What moved between two analyses of one entry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Movement {
    /// Items whose interface fingerprint changed, appeared or disappeared.
    pub interfaces: usize,
    pub global: bool,
}

impl Fingerprints {
    pub fn movement_since(&self, before: &Fingerprints) -> Movement {
        let mut interfaces = 0;
        for (key, hash) in &self.items {
            if before.items.get(key) != Some(hash) {
                interfaces += 1;
            }
        }
        interfaces += before
            .items
            .keys()
            .filter(|key| !self.items.contains_key(*key))
            .count();
        Movement {
            interfaces,
            global: self.global != before.global,
        }
    }

    /// The items that moved, by key — what the session report and a verify log
    /// name.
    pub fn moved_items(&self, before: &Fingerprints) -> Vec<String> {
        let mut moved: Vec<String> = self
            .items
            .iter()
            .filter(|(key, hash)| before.items.get(*key) != Some(*hash))
            .map(|(key, _)| key.clone())
            .collect();
        moved.extend(
            before
                .items
                .keys()
                .filter(|key| !self.items.contains_key(*key))
                .cloned(),
        );
        moved.sort();
        moved
    }
}

/// The file an entity belongs to, as text — the item key's first segment.
/// Canonical, so the key is the same on every surface that names the file.
fn file_of(program: &Program, lookup: &crate::analyzer::SourceLookup<'_>, id: Id) -> String {
    lookup
        .of(id)
        .and_then(|source| program.canonical_sources.get(source.0 as usize))
        .map(|path| path.display().to_string())
        .unwrap_or_default()
}

/// The interface fingerprint of every item and the global-facts fingerprint
/// of `program` (S0). Reads the finished program — the post-passes' effects
/// included — and renders through the editor's own labels, so the inferred
/// return is in a function's signature exactly as hover shows it.
pub fn fingerprints(program: &Program) -> Fingerprints {
    let lookup = program.source_lookup();
    // The transitive requirement, without the explanatory chain: the LAYER a
    // function needs is its interface; the path to it is not (it names
    // callees, and a callee renamed inside a body would otherwise move every
    // caller's fingerprint).
    let requirements = crate::platform_color::requirements(program);
    let requirement = |id: Id| -> String {
        requirements
            .get(&id)
            .map(|line| line.split(" (via").next().unwrap_or(line).to_string())
            .unwrap_or_default()
    };
    let context_names = |contexts: Option<&Vec<Id>>| -> Vec<&str> {
        contexts
            .into_iter()
            .flatten()
            .filter_map(|context| program.variables.get(context).map(|variable| variable.name))
            .collect()
    };
    let mut items: BTreeMap<String, u64> = BTreeMap::new();
    let mut unknown_interfaces = 0;
    let mut insert = |key: String, rendered: &str| {
        // Two items one key would spell (an overload is impossible, but a
        // name repeated in two inline `mod`s of one file is not): an ordinal,
        // in the program's own declaration order, keeps both.
        let mut candidate = key.clone();
        let mut ordinal = 1;
        while items.contains_key(&candidate) {
            ordinal += 1;
            candidate = format!("{key}#{ordinal}");
        }
        items.insert(candidate, crate::content_hash(rendered));
    };
    for (id, function) in &program.functions {
        let owner = program
            .member_owners
            .get(id)
            .and_then(|owner| program.member_headers.get(owner))
            .map(String::as_str)
            .unwrap_or("");
        let signature = program
            .declaration_labels
            .get(id)
            .map(String::as_str)
            .unwrap_or("");
        if signature.contains("unknown") {
            unknown_interfaces += 1;
        }
        // `[track_caller]` is interface: a caller's emitted call gains or loses
        // the hidden trailing `Location` argument when it flips.
        let rendered = format!(
            "{signature}|track_caller={}|async={}|contexts={}|declared={:?}|borrows={:?}|bumps={:?}|{}",
            function.track_caller,
            function.is_async || program.async_functions.contains(id),
            program.context_dependent_functions.contains(id),
            context_names(program.declared_function_contexts.get(id)),
            function.borrows,
            function.bumps,
            requirement(*id),
        );
        insert(
            format!(
                "fun {}::{owner}::{}",
                file_of(program, &lookup, *id),
                function.name
            ),
            &rendered,
        );
    }
    for (id, external) in &program.external_functions {
        let signature = program
            .declaration_labels
            .get(id)
            .map(String::as_str)
            .unwrap_or("");
        insert(
            format!(
                "external {}::{}",
                file_of(program, &lookup, *id),
                external.name
            ),
            &format!(
                "{signature}|track_caller={}|{}",
                external.track_caller,
                requirement(*id)
            ),
        );
    }
    for (id, struct_) in &program.structs {
        let shape = program
            .declaration_labels
            .get(id)
            .map(String::as_str)
            .unwrap_or("");
        insert(
            format!(
                "struct {}::{}",
                file_of(program, &lookup, *id),
                struct_.name
            ),
            shape,
        );
    }
    for (id, enum_) in &program.enums {
        let shape = program
            .declaration_labels
            .get(id)
            .map(String::as_str)
            .unwrap_or("");
        insert(
            format!("enum {}::{}", file_of(program, &lookup, *id), enum_.name),
            shape,
        );
    }
    // A module binding's type is part of its MODULE's interface (i7: a push
    // in one function grounds the element type a sibling reads).
    for id in program.module_level_bindings() {
        let Some(variable) = program.variables.get(&id) else {
            continue;
        };
        let type_label = program
            .expr_types
            .get(&id)
            .map(String::as_str)
            .unwrap_or("");
        insert(
            format!("let {}::{}", file_of(program, &lookup, id), variable.name),
            &format!("{type_label}|{}", requirement(id)),
        );
    }
    Fingerprints {
        items,
        global: global_facts(program),
        unknown_interfaces,
    }
}

/// The global facts a module outside the import closure can still change
/// (§2.4): which impls exist (header and the member NAMES each provides — a
/// body edit inside an impl method moves neither), which traits exist, and
/// which types are declared resources (`declares_a_resource` switches the drop
/// planner on for the whole program).
fn global_facts(program: &Program) -> u64 {
    let mut rows: Vec<String> = Vec::new();
    for implementation in &program.implementations {
        let header = program
            .member_headers
            .get(&implementation.impl_id)
            .map(String::as_str)
            .unwrap_or("impl ?");
        let mut members: Vec<&str> = implementation.declarations.keys().copied().collect();
        members.sort_unstable();
        rows.push(format!("{header} {{{}}}", members.join(",")));
    }
    for trait_ in program.traits.values() {
        let mut members: Vec<&str> = trait_.declarations.keys().copied().collect();
        members.sort_unstable();
        rows.push(format!("trait {} {{{}}}", trait_.name, members.join(",")));
    }
    for struct_ in program.structs.values().filter(|struct_| struct_.resource) {
        rows.push(format!("resource struct {}", struct_.name));
    }
    for enum_ in program.enums.values().filter(|enum_| enum_.resource) {
        rows.push(format!("resource enum {}", enum_.name));
    }
    rows.sort_unstable();
    crate::content_hash(&rows.join("\n"))
}

/// The previous analysis's fingerprints, per (entry, platform) — what
/// "interface-moved" is measured against. A session analyzes few entries, so
/// this holds a handful of maps; it is cleared with nothing else because it
/// changes no answer, only what the phase line reports.
static PREVIOUS: OnceLock<Mutex<HashMap<(PathBuf, Platform), Fingerprints>>> = OnceLock::new();

/// Fingerprints `program`, compares them with the previous analysis of the
/// same entry, and keeps them for the next — answering what moved (`None` for
/// the first analysis of an entry, which has nothing to compare against).
pub fn record(program: &Program) -> (Fingerprints, Option<(Movement, Vec<String>)>) {
    let current = fingerprints(program);
    let entry = program
        .canonical_sources
        .first()
        .cloned()
        .unwrap_or_default();
    let previous = PREVIOUS.get_or_init(|| Mutex::new(HashMap::default()));
    let mut previous = previous
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let before = previous.insert((entry, program.platform), current.clone());
    let movement = before.map(|before| {
        (
            current.movement_since(&before),
            current.moved_items(&before),
        )
    });
    (current, movement)
}

/// The S0 phase line and the census's counters line, printed once per
/// top-level analysis after the post-passes (when the analysis's EFFECTS are
/// settled, which the interface fingerprint includes).
pub fn report(program: &Program) {
    if crate::macros::in_macro_world() {
        return;
    }
    let census = census();
    let movement = fingerprinting().then(|| record(program));
    if crate::phase_timing_enabled() {
        let hot_set = if census.package_modules == 0 {
            "-".to_string()
        } else {
            format!("{}/{}", census.hot_modules, census.package_modules)
        };
        let (interfaces, global, unknown) = match &movement {
            Some((fingerprints, Some((movement, _)))) => (
                movement.interfaces.to_string(),
                u8::from(movement.global).to_string(),
                fingerprints.unknown_interfaces.to_string(),
            ),
            Some((fingerprints, None)) => (
                "first".to_string(),
                "first".to_string(),
                fingerprints.unknown_interfaces.to_string(),
            ),
            None => ("-".to_string(), "-".to_string(), "-".to_string()),
        };
        eprintln!(
            "[vilan phase] hot-set {hot_set} interface-moved {interfaces} global-moved {global} \
             unknown-interfaces {unknown} hot-world {} hot-refusal {} resolve-deferred {}",
            u8::from(census.hot_world),
            census.hot_refusal.unwrap_or("-"),
            u8::from(census.resolve_deferred),
        );
        if let Some((_, Some((_, moved)))) = &movement
            && !moved.is_empty()
        {
            // Bounded: a whole-module edit moves everything in it, and the
            // line exists to say WHAT, not to dump it.
            let shown: Vec<&str> = moved.iter().take(8).map(String::as_str).collect();
            eprintln!(
                "[vilan phase] interface-moved-items {}{}",
                shown.join(" ; "),
                if moved.len() > shown.len() {
                    format!(" ; (+{} more)", moved.len() - shown.len())
                } else {
                    String::new()
                }
            );
        }
    }
    if crate::counters::counters_enabled() {
        eprintln!(
            "[vilan counters] incremental base-hits={} base-misses={} base-stores={} \
             hot-world={} sources-walked={} records-replayed={} functions-checked={} \
             const-hits={} const-misses={} reach-questions={} bound-sites-served={} \
             bound-sites-checked={} graphs-replayed={} context-log-replayed={} \
             seeded-passes={} post-records-filed={}",
            census.base_hits,
            census.base_misses,
            census.base_stores,
            u8::from(census.hot_world),
            census.sources_walked,
            census.records_replayed,
            census.functions_checked,
            census.const_cache_hits,
            census.const_cache_misses,
            census.reach_questions,
            census.bound_sites_served,
            census.bound_sites_checked,
            census.graphs_replayed,
            census.context_log_replayed,
            census.seeded_passes,
            census.post_records_filed,
        );
    }
}

// --- Q3: the id-free observation the differential compares ------------------

/// Everything an editor or a build can observe of one analysis, rendered
/// without a single id or `SourceId` index: those are where an incremental
/// world and a clean one are ALLOWED to differ (S1 walks the hot set after the
/// prefix, so its entities number differently), and nothing a user sees is
/// keyed by them. Every diagnostic and warning with the FILE it publishes to,
/// its span and message, its note (with the note's file) and trace; then the
/// editor's tables — every hover type label, declaration label and inlay-hint
/// label, every recorded reference — each at its (file, span).
///
/// Rendered from a finished program; `diagnostics` is the front end's full
/// list (parse diagnostics first, as `analyze_source` returns it), which the
/// program's own list is a suffix of.
pub fn render_observation(program: &Program, diagnostics: &[crate::Error]) -> String {
    let mut out = String::new();
    let path_of = |source: SourceId| -> String {
        program
            .sources
            .get(source.0 as usize)
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| format!("<source {}>", source.0))
    };
    let render_error = |out: &mut String, error: &crate::Error, file: String| {
        let _ = write!(out, "{file} {:?} {}", error.span.into_range(), error.msg);
        if let Some(note) = &error.note {
            let _ = write!(
                out,
                " | note {} {:?} {}",
                note.source.map(path_of).unwrap_or_default(),
                note.span.into_range(),
                note.msg
            );
        }
        for hop in &error.trace {
            let _ = write!(
                out,
                " | trace {} {:?} {}",
                hop.note.source.map(path_of).unwrap_or_default(),
                hop.note.span.into_range(),
                hop.note.msg
            );
        }
        out.push('\n');
    };
    // The front end's list is the parse diagnostics followed by the program's,
    // and only the program's carry a file; the parse ones are the entry's.
    let parse_prefix = diagnostics.len().saturating_sub(program.diagnostics.len());
    out.push_str("# diagnostics\n");
    for error in &diagnostics[..parse_prefix] {
        render_error(&mut out, error, path_of(SourceId(0)));
    }
    for (index, error) in program.diagnostics.iter().enumerate() {
        render_error(&mut out, error, path_of(program.diagnostic_source(index)));
    }
    out.push_str("# warnings\n");
    for (index, warning) in program.warnings.iter().enumerate() {
        render_error(&mut out, warning, path_of(program.warning_source(index)));
    }
    // The editor's tables, each row at its place in its file. Sorted, because
    // the tables are hash maps and the ORDER they iterate in is the one thing
    // about them nothing reads.
    let lookup = program.source_lookup();
    let place = |id: Id| -> Option<String> {
        let span = program.span_map.get(&id)?;
        let source = lookup.of(id)?;
        Some(format!("{} {:?}", path_of(source), span.into_range()))
    };
    let mut rows: Vec<String> = Vec::new();
    for (id, label) in &program.expr_types {
        if let Some(at) = place(*id) {
            rows.push(format!("type {at} {label}"));
        }
    }
    for (id, label) in &program.declaration_labels {
        if let Some(at) = place(*id) {
            rows.push(format!("declaration {at} {label}"));
        }
    }
    for (id, label) in &program.hint_labels {
        if let Some(at) = place(*id) {
            rows.push(format!("hint {at} {label:?}"));
        }
    }
    // Name resolution, which is what Find References, rename and go-to-
    // definition read: every use that resolved to a binding, a declaration or
    // a module, from where it is written to where it points.
    for (id, expr) in &program.entity_map {
        use crate::analyzer::Expr;
        let target = match expr {
            Expr::Local(target)
            | Expr::Variable(target)
            | Expr::Parameter(target)
            | Expr::Function(target)
            | Expr::ExternalFunction(target)
            | Expr::Struct(target)
            | Expr::Enum(target)
            | Expr::Trait(target)
            | Expr::Module(target) => *target,
            _ => continue,
        };
        if target == *id {
            continue;
        }
        if let Some(at) = place(*id) {
            rows.push(format!(
                "resolves {at} -> {}",
                place(target).unwrap_or_else(|| "?".to_string())
            ));
        }
    }
    for (source, span, target, label) in &program.type_references {
        rows.push(format!(
            "type-reference {} {:?} -> {} {label}",
            path_of(*source),
            span.into_range(),
            target.and_then(place).unwrap_or_else(|| "?".to_string())
        ));
    }
    rows.sort_unstable();
    rows.dedup();
    out.push_str("# editor tables\n");
    for row in rows {
        out.push_str(&row);
        out.push('\n');
    }
    out
}
