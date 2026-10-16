//! M110 S6 — the id windows' pins (`analyzer/id_windows.rs`), the spike's
//! (S5, Order 49) carried over and S6's added.
//!
//! The spike's assertions (`analyzer-pass-map.md` §7.3, Q3): every walk-minted
//! id inside its item's window, the windows laid out in load order and
//! disjoint in every lane; a mint relocated for a fixpoint anchor never leaves
//! the anchor's window (the `cross` plant turns that red); the fixpoint's
//! mints ARE anchored (the `anchor-off` plant turns that red); and the census
//! mode mints exactly today's ids (the counters' high-water marks agree with
//! the dense ones). The byte-identity of the differentials under each mode is
//! the differentials' own business, run with `VILAN_ID_WINDOWS` set.

mod replay_harness;
mod scratch;

use replay_harness::packages::*;
use replay_harness::std_spec;
use vilan_core::analyzer::id_windows::{self, IdWindowsReport, Lane, Mode, Phase, Plant};
use vilan_core::{Workspace, analyze_source};

/// Analyzes the classes package on its own thread under `mode` (and `plant`),
/// clean (no stored world), and hands back the spike's report.
fn report_under(package: &Package, mode: Mode, plant: Option<Plant>) -> IdWindowsReport {
    let entry_text = package.text("main.vl").to_string();
    let pkg_root = package.directory.clone();
    let entry = package.entry.clone();
    let platform = package.platform;
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            vilan_core::analyzer::base_cache_clear();
            id_windows::force_mode(Some(mode));
            id_windows::force_plant(plant);
            let leaked: &'static str = Box::leak(entry_text.into_boxed_str());
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                &pkg_root,
                &entry,
                Some(platform),
                &Workspace::default(),
            );
            id_windows::force_mode(None);
            id_windows::force_plant(None);
            assert!(
                errors.is_empty(),
                "the classes package is clean: {errors:?}"
            );
            program
                .expect("a program")
                .id_windows
                .expect("the spike's report when the mode is on")
        })
        .expect("spawn")
        .join()
        .expect("the analysis thread")
}

fn high_water_under(package: &Package, mode: Mode) -> (u32, u32, u32) {
    let entry_text = package.text("main.vl").to_string();
    let pkg_root = package.directory.clone();
    let entry = package.entry.clone();
    let platform = package.platform;
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            vilan_core::analyzer::base_cache_clear();
            id_windows::force_mode(Some(mode));
            let leaked: &'static str = Box::leak(entry_text.into_boxed_str());
            let (program, _) = analyze_source(
                leaked,
                &std_spec(),
                &pkg_root,
                &entry,
                Some(platform),
                &Workspace::default(),
            );
            id_windows::force_mode(None);
            let program = program.expect("a program");
            // The entity counter as the post passes left it (they mint from
            // it too, the same way under every mode); the other two lanes
            // only the report carries, so they are zero when it is off.
            let next = program.next_entity_id;
            match program.id_windows {
                Some(report) => (
                    next,
                    report.type_high_water as u32,
                    report.scope_high_water as u32,
                ),
                None => (next, 0, 0),
            }
        })
        .expect("spawn")
        .join()
        .expect("the analysis thread")
}

fn lanes_disjoint_and_ordered(report: &IdWindowsReport, lane: Lane) {
    let mut previous_end = 0u32;
    for (index, window) in report.layout.iter().enumerate() {
        let range = match lane {
            Lane::Entity => window.entities,
            Lane::Type => window.types,
            Lane::Scope => window.scopes,
        };
        assert!(
            range.start >= previous_end,
            "{lane:?} window {index} starts at {} before the previous ended at {previous_end}",
            range.start
        );
        assert!(
            range.start <= range.cursor && range.cursor <= range.end,
            "{lane:?} window {index} is malformed: {range:?}"
        );
        previous_end = range.end;
    }
}

#[test]
fn windows_are_laid_out_in_load_order_and_hold_every_walk_mint() {
    let package = classes_package();
    let report = report_under(&package, Mode::Walk, None);
    assert!(report.windows > 0, "the classes package has items");
    for lane in [Lane::Entity, Lane::Type, Lane::Scope] {
        lanes_disjoint_and_ordered(&report, lane);
    }
    // Every window names the item its walk returned.
    assert!(report.layout.iter().all(|window| window.item.is_some()));
    // The walk's mints all landed while a window was open; what the walk
    // phase minted with no window open is the drain's own (module nodes,
    // prelude seeds, the generated-expansion scopes), never an item's.
    let entities = &report.census.entities;
    assert!(entities.walk_in_window > 0);
    assert!(
        entities.walk_unwindowed < entities.walk_in_window / 10,
        "the drain's unwindowed mints should be a sliver: {entities:?}"
    );
    // In `walk` mode nothing relocates, so nothing can land outside an
    // anchor or overflow.
    assert_eq!(report.census.overflowed, 0);
    assert_eq!(entities.relocated_outside_anchor, 0);
    assert_eq!(report.census.types.relocated_outside_anchor, 0);
    // The room is real: the windows span more than the walk minted.
    assert!(report.entity_span > entities.walk_in_window);
}

#[test]
fn a_relocated_type_mint_never_leaves_its_anchors_window() {
    let package = classes_package();
    let report = report_under(&package, Mode::Types, None);
    let types = &report.census.types;
    assert!(
        types.anchored_in_window > 0,
        "the fixpoint relocates type mints into their anchors' windows: {types:?}"
    );
    assert_eq!(types.relocated_outside_anchor, 0, "{types:?}");
    for lane in [Lane::Entity, Lane::Type, Lane::Scope] {
        lanes_disjoint_and_ordered(&report, lane);
    }
    // Non-vacuity: the `cross` plant mints from the previous item's window.
    let planted = report_under(&package, Mode::Types, Some(Plant::Cross));
    assert!(
        planted.census.types.relocated_outside_anchor > 0,
        "the plant must be seen: {:?}",
        planted.census.types
    );
}

#[test]
fn the_fixpoints_mints_are_anchored() {
    let package = classes_package();
    let report = report_under(&package, Mode::Census, None);
    let types = &report.census.types;
    assert!(types.anchored() > 0, "{types:?}");
    // Non-vacuity: with the anchor never set, every fixpoint mint is
    // unanchored.
    let planted = report_under(&package, Mode::Census, Some(Plant::AnchorOff));
    assert_eq!(
        planted.census.types.anchored(),
        0,
        "{:?}",
        planted.census.types
    );
    assert!(planted.census.types.unanchored_total() > types.unanchored_total());
}

#[test]
fn the_census_mode_mints_todays_ids() {
    let package = classes_package();
    let (dense_entities, _, _) = high_water_under(&package, Mode::Off);
    let (census_entities, _, _) = high_water_under(&package, Mode::Census);
    assert_eq!(
        dense_entities, census_entities,
        "the census mode must not move the entity counter"
    );
    let (walk_entities, _, _) = high_water_under(&package, Mode::Walk);
    assert!(
        walk_entities > dense_entities,
        "the walk mode lays out room: {walk_entities} vs {dense_entities}"
    );
}

/// M134's pin (Order 50): every post-settle pass says which body it mints
/// for — no checks-phase mint is made with no anchor ASKED (`checks_unasked`
/// is 0 in every lane). What the checks-phase `unanchored` row still counts
/// is the anchors asked of a slot that lives in NO window (the resolve's and
/// the base's own unanchored mints; `check_anchor_misses`) — 13 / 72 of the
/// 11,877 / 18,818 type mints per kolt leg that read unanchored before M134,
/// 53 on the leaf package — which have no item to join by construction.
#[test]
fn the_checks_phase_mints_nothing_unanchored() {
    let checks = Phase::Checks.index();
    for (name, package) in [("leaf", leaf_package()), ("classes", classes_package())] {
        let report = report_under(&package, Mode::All, None);
        assert_eq!(report.mode, Mode::All);
        for (lane, census) in [
            (Lane::Entity, &report.census.entities),
            (Lane::Type, &report.census.types),
            (Lane::Scope, &report.census.scopes),
        ] {
            assert_eq!(census.checks_unasked, 0, "{name} {lane:?}: {census:?}");
            assert!(
                census.unanchored[checks] <= report.census.check_anchor_misses,
                "{name} {lane:?}: {census:?} misses {}",
                report.census.check_anchor_misses
            );
        }
        assert_eq!(
            report.census.entities.unanchored[checks], 0,
            "{name}: {:?}",
            report.census.entities
        );
        eprintln!(
            "{name}: checks-phase unanchored entities {} types {} scopes {}; check-anchor-misses {}",
            report.census.entities.unanchored[checks],
            report.census.types.unanchored[checks],
            report.census.scopes.unanchored[checks],
            report.census.check_anchor_misses
        );
        package.remove();
    }
}

/// S6's standing pins, on the DEFAULT (every lane relocated): no constraint
/// rewrites a slot minted for another item, no relocated mint leaves its
/// anchor's window, every lane relocates something, and the three lanes
/// stay disjoint and in load order.
#[test]
fn the_default_relocates_every_lane_inside_its_anchor() {
    let package = classes_package();
    let report = report_under(&package, Mode::All, None);
    assert_eq!(report.mode, Mode::All);
    // The classes package pushes into another module's use-inferred binding
    // on purpose (`bag`/`a_spoil`): a constraint of one item writing a slot
    // minted for another, counted, not an invariant. The leaf package has no
    // such write, and there the count is pinned at zero.
    eprintln!(
        "classes: writes own {} other {} drain-other {} tail {}",
        report.census.writes_own,
        report.census.writes_other,
        report.census.writes_drain_other,
        report.census.writes_tail
    );
    let leaf = leaf_package();
    let leaf_report = report_under(&leaf, Mode::All, None);
    assert_eq!(
        leaf_report.census.writes_other, 0,
        "{:?}",
        leaf_report.census
    );
    assert_eq!(
        leaf_report.census.writes_drain_other, 0,
        "{:?}",
        leaf_report.census
    );
    leaf.remove();
    for (lane, census) in [
        (Lane::Entity, &report.census.entities),
        (Lane::Type, &report.census.types),
        (Lane::Scope, &report.census.scopes),
    ] {
        assert_eq!(census.relocated_outside_anchor, 0, "{lane:?}: {census:?}");
        lanes_disjoint_and_ordered(&report, lane);
    }
    assert!(
        report.census.types.anchored_in_window > 0,
        "{:?}",
        report.census.types
    );
    assert!(
        report.census.entities.anchored_in_window > 0,
        "{:?}",
        report.census.entities
    );
    assert_eq!(
        report.laid_out, report.windows,
        "a cold analysis lays out every window"
    );
    package.remove();
}

/// S6: a type window is sized from the item's previous demand. The first
/// analysis of a package sizes every type lane at ×8 of its walk; the second
/// sizes it at ×1.5 of what the first actually used, so the id space the
/// windows span shrinks and nothing newly overflows.
#[test]
fn a_second_analysis_sizes_the_type_windows_from_the_demand() {
    let package = classes_package();
    let first = report_under(&package, Mode::All, None);
    let second = report_under(&package, Mode::All, None);
    assert_eq!(first.windows, second.windows);
    eprintln!(
        "first: span {} high {} overflowed {}; second: span {} high {} overflowed {}",
        first.type_span,
        first.type_high_water,
        first.census.overflowed,
        second.type_span,
        second.type_high_water,
        second.census.overflowed
    );
    assert!(
        second.type_span < first.type_span,
        "the demand-sized layout is tighter: first {} slots, second {}",
        first.type_span,
        second.type_span
    );
    assert!(
        second.census.overflowed <= first.census.overflowed + 1,
        "a demand-sized window holds what the item minted last time: first {} overflowed, second {}",
        first.census.overflowed,
        second.census.overflowed
    );
    assert_eq!(second.census.types.relocated_outside_anchor, 0);
    package.remove();
}

#[test]
fn the_report_line_names_the_mode() {
    let package = classes_package();
    let report = report_under(&package, Mode::All, None);
    assert!(
        report
            .line()
            .starts_with("[vilan windows] mode=all windows=")
    );
    assert_eq!(report.mode, Mode::All);
    // `all` relocates every lane: scopes and entities too.
    assert!(
        report.census.entities.anchored_in_window + report.census.entities.anchored_spilled > 0
            || report.census.entities.anchored() == 0
    );
}

// --- M110 S7 (Order 51): the per-item records over the windows -----------------

/// One seeded analysis of `package` (the editor's keystroke shape: the seed
/// is the edited file, every reuse on), answering its census and its
/// id-free rendering; `clean` answers the canonical analysis's rendering.
fn seeded(package: &Package, seed: &str) -> (vilan_core::incremental::Census, String) {
    let observation = observe(package, vec![package.path(seed)], Leg::Incremental);
    (observation.census, observation.rendering)
}

fn clean(package: &Package) -> String {
    observe(package, Vec::new(), Leg::Clean).rendering
}

/// S7: the second seeded analysis of an unchanged package serves every
/// re-walked item from the record the first one filed — nothing stale — and
/// answers what a clean analysis answers. The leaf package: its hot set is
/// accepted, so the first seeded analysis builds and stores the hot-set
/// world and files the hot leaf's and the entry's items after the store,
/// and the second is served from it.
#[test]
fn a_second_seeded_analysis_serves_the_items_records() {
    let package = leaf_package();
    vilan_core::analyzer::base_cache_clear();
    let (first, _) = seeded(&package, "views.vl");
    assert_eq!(first.item_records_served, 0, "{first:#?}");
    assert!(
        first.item_records_filed > 0,
        "the first analysis files the re-walked items' records: {first:#?}"
    );
    let (second, rendering) = seeded(&package, "views.vl");
    assert!(
        second.hot_world && second.base_hits > 0,
        "the second analysis is served the stored hot-set world: {second:#?}"
    );
    assert!(
        second.item_records_served > 0 && second.item_records_stale == 0,
        "an unchanged file's items are served from their records: {second:#?}"
    );
    assert_eq!(
        second.item_records_served, first.item_records_filed,
        "every item filed is served next time: {second:#?}"
    );
    // M110 S7b: the Class A passes walk the LIVE ranges — the complement of
    // the frozen, reused and served ranges — so the served analysis covers
    // fewer entities than the one that filed, and the first (nothing reused
    // yet) covers every non-std entity.
    assert!(
        second.live_ranges > 0 && second.live_entities > 0,
        "the served analysis seals live ranges: {second:#?}"
    );
    assert!(
        second.live_entities < first.live_entities,
        "the served analysis walks fewer live entities than the filing one: \
         {} vs {}",
        second.live_entities,
        first.live_entities
    );
    assert_eq!(rendering, clean(&package));
    package.remove();
}

/// M148: a served keystroke's PREFIX module answers its hover rows (E206's
/// call label, E237's definition, E241's patterns, E240's type parameter)
/// from its record rather than rendering them again — the first analysis
/// restores none, the second some — and the served rendering, the four tables
/// included, is the clean one.
#[test]
fn a_served_keystroke_restores_the_prefix_modules_hover_rows() {
    let package = Package::write(
        "hover_rows",
        vilan_core::Platform::default(),
        &[
            (
                "main.vl",
                "import pkg::shapes::{ pair, pick };\nimport pkg::views::render;\n\nfun main() {\n\tprint(pair().first);\n\tprint(pick(None));\n\tprint(render());\n}\n",
            ),
            (
                "shapes.vl",
                "export struct Pair<T> {\n\tfirst: T,\n\tsecond: T,\n}\n\nfun same<U>(value: U): U {\n\tvalue\n}\n\nexport fun pair(): Pair<i32> {\n\tlet made = Pair { first = same(1), second = 2 };\n\tmade\n}\n\nexport fun pick(value: Option<i32>): i32 {\n\tmatch value {\n\t\tSome(let inner) => inner,\n\t\tNone => 0,\n\t}\n}\n",
            ),
            ("views.vl", "export fun render(): i32 {\n\t1\n}\n"),
        ],
    );
    vilan_core::analyzer::base_cache_clear();
    let (first, _) = seeded(&package, "views.vl");
    let (second, rendering) = seeded(&package, "views.vl");
    assert_eq!(first.hover_rows_restored, 0, "{first:#?}");
    assert!(
        second.base_hits > 0 && second.records_replayed > 0,
        "the second analysis reuses the prefix module: {second:#?}"
    );
    assert!(
        second.hover_rows_restored > 0,
        "the served analysis restores the prefix module's hover rows: {second:#?}"
    );
    for row in ["definition ", "pattern ", "call-label ", "reference-hover "] {
        assert!(
            rendering.contains(row),
            "the fixture renders a {row:?} row (or the pin says nothing about it):\n{rendering}"
        );
    }
    assert_eq!(rendering, clean(&package));
    package.remove();
}

/// S7: an edit inside one item re-checks that item and the items that read
/// from it (the entry's `main` calls the edited `render`, and its import of
/// `render` binds to it), and serves the rest (the imports of the prefix's
/// `base` and `ring`) — still answering what a clean analysis answers.
#[test]
fn an_edited_items_dependents_are_re_checked_and_the_rest_served() {
    let mut package = leaf_package();
    vilan_core::analyzer::base_cache_clear();
    let (_, _) = seeded(&package, "views.vl");
    let (settled, _) = seeded(&package, "views.vl");
    assert!(
        settled.item_records_stale == 0 && settled.item_records_served > 0,
        "{settled:#?}"
    );
    let before = package.text("views.vl").to_string();
    let after = before.replacen("\tbase() + 1\n", "\tlet extra = 1;\n\tbase() + extra\n", 1);
    assert_ne!(before, after, "the edit's anchor is in views.vl");
    package.set("views.vl", after);
    let (edited, rendering) = seeded(&package, "views.vl");
    assert!(
        edited.hot_world && edited.base_hits > 0,
        "the edited leaf is served the stored hot-set world: {edited:#?}"
    );
    assert!(
        edited.item_records_stale > 0,
        "the edited item and its readers are stale: {edited:#?}"
    );
    assert!(
        edited.item_records_served > 0,
        "the items that do not read the edited one are served: {edited:#?}"
    );
    assert!(
        edited.item_records_served < settled.item_records_served,
        "fewer items are served than on the unchanged keystroke: {edited:#?} vs {settled:#?}"
    );
    assert_eq!(rendering, clean(&package));
    package.set("views.vl", before);
    let (undone, rendering) = seeded(&package, "views.vl");
    assert!(undone.item_records_served > 0, "{undone:#?}");
    assert_eq!(rendering, clean(&package));
    package.remove();
}
