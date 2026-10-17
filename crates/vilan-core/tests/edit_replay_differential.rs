//! M110 Q3 — the EDIT-REPLAY DIFFERENTIAL (`incremental-analysis.md` §6).
//!
//! Every incremental slice answers from something it remembered: a base-cache
//! world (S3c), a module's replayed checks (M19), the stored PREFIX of an entry's
//! world with only the edited module and its importers re-walked (S1). This
//! binary is the gate that holds all of them to one claim: **an incremental
//! analysis answers exactly what a clean one does**. Packages are edited the
//! way an editor edits them — an open buffer moves (the document overlay), the
//! edited file is the analysis's hot seed (`Workspace::hot_seeds`, what the
//! language server passes), and each edit is UNDONE again, because the undo is
//! where stale-cache bugs live — and after every step the incremental analysis
//! is compared with a clean one (`vilan_core::incremental::clean_analysis`: no
//! base-cache world, no replayed record, no hot set).
//!
//! What is compared is what a user can observe, rendered without ids
//! (`render_observation`): every diagnostic and warning with the file it
//! publishes to, its span, message, note and trace; every hover type label,
//! declaration label and inlay hint at its (file, span); every resolved name
//! and type reference from where it is written to where it points. Ids and
//! `SourceId` indices are the one thing an incremental world is ALLOWED to
//! number differently (S1 walks the hot set after the prefix), and nothing a
//! user sees is keyed by them. Emitted JS is compared too, where the program is
//! clean enough to emit.
//!
//! Two legs: the §6 EDIT CLASSES over hand-written multi-module packages
//! (a body literal's type, a context read, a sleep, a module binding's element
//! type, a written return, a field rename, an impl in an unimported module, a
//! derive, an import, a `const` callee, an import cycle, a prefix module
//! changing under a seed elsewhere, a browser entry reaching a node-only call),
//! and the CORPUS, every program re-hosted as a package module with an
//! importer and a bystander. Each slice's planted bug (`incremental::Plant`)
//! must turn the classes leg red — the M57 lesson.
//!
//! The overlay and the plant are process-global, so the tests serialize on
//! [`SWITCH_LOCK`] (plain `cargo test` runs them as threads of one process).

mod replay_harness;
mod scratch;
mod test_pool;

use std::path::PathBuf;
use std::sync::Mutex;

use replay_harness::packages::*;
use vilan_core::Platform;
use vilan_core::incremental::{Census, Plant, set_plant};

static SWITCH_LOCK: Mutex<()> = Mutex::new(());

/// Applies `edit`, observes incrementally and cleanly, compares; then undoes it
/// and does the same. Answers every divergence found and the incremental
/// censuses (for the caller's non-vacuity checks).
fn replay(package: &mut Package, edit: &Edit, divergences: &mut Vec<String>) -> Vec<Census> {
    let before = package.text(edit.file).to_string();
    let mut after = before.clone();
    for (find, replace) in edit.replacements {
        assert!(
            after.contains(find),
            "{}: the edit's anchor {find:?} is not in {}",
            edit.label,
            edit.file
        );
        after = after.replacen(find, replace, 1);
    }
    let seed = package.path(edit.seed.unwrap_or(edit.file));
    let mut censuses = Vec::new();
    for (phase, text) in [("edit", after), ("undo", before)] {
        package.set(edit.file, text);
        let incremental = observe(package, vec![seed.clone()], Leg::Incremental);
        let clean = observe(package, Vec::new(), Leg::Clean);
        let same_shape = observe(package, vec![seed.clone()], Leg::CleanSameShape);
        if incremental.rendering != clean.rendering {
            divergences.push(format!(
                "{} ({phase}): the incremental analysis differs from the clean one at {}",
                edit.label,
                first_difference(&incremental.rendering, &clean.rendering)
            ));
        }
        if incremental.javascript != same_shape.javascript {
            divergences.push(format!(
                "{} ({phase}): the emitted JS differs from a clean analysis of the same \
                 shape (incremental {} bytes, clean {} bytes)",
                edit.label,
                incremental.javascript.as_ref().map_or(0, String::len),
                same_shape.javascript.as_ref().map_or(0, String::len),
            ));
        }
        // Whether the program emits at all is a user-visible answer, so it
        // has to agree with the canonical world too.
        if incremental.javascript.is_some() != clean.javascript.is_some() {
            divergences.push(format!(
                "{} ({phase}): one analysis emits and the other does not",
                edit.label
            ));
        }
        censuses.push(incremental.census);
    }
    censuses
}

/// [`replay`] for [`STD_EDIT`]: std's file moves (and moves back), the seed
/// stays on the module whose const site calls it.
fn replay_std_edit(package: &Package, divergences: &mut Vec<String>) -> Vec<Census> {
    let path = std::fs::canonicalize(replay_harness::std_root().join(STD_EDIT_FILE))
        .expect("std's math module");
    let before = std::fs::read_to_string(&path).expect("read std's math module");
    assert!(
        before.contains(STD_EDIT.0),
        "{STD_EDIT_LABEL}: the edit's anchor is not in {STD_EDIT_FILE}"
    );
    let after = before.replacen(STD_EDIT.0, STD_EDIT.1, 1);
    let seed = package.path("data_user.vl");
    let mut censuses = Vec::new();
    for (phase, text) in [("edit", Some(after)), ("undo", None)] {
        vilan_core::analyzer::set_document_overlay(&path, text);
        let incremental = observe(package, vec![seed.clone()], Leg::Incremental);
        let clean = observe(package, Vec::new(), Leg::Clean);
        let same_shape = observe(package, vec![seed.clone()], Leg::CleanSameShape);
        if incremental.rendering != clean.rendering {
            divergences.push(format!(
                "{STD_EDIT_LABEL} ({phase}): the incremental analysis differs from the clean one at {}",
                first_difference(&incremental.rendering, &clean.rendering)
            ));
        }
        if incremental.javascript != same_shape.javascript
            || incremental.javascript.is_some() != clean.javascript.is_some()
        {
            divergences.push(format!(
                "{STD_EDIT_LABEL} ({phase}): the emitted JS differs from a clean analysis"
            ));
        }
        censuses.push(incremental.census);
    }
    censuses
}

/// What [`replay_the_classes`] observed: every divergence, every incremental
/// census, and the censuses of the edits whose SHAPE is asserted besides
/// their agreement — M121's served hot impls and B553's deferred entry.
struct Replayed {
    divergences: Vec<String>,
    censuses: Vec<Census>,
    served: Vec<Census>,
    entry_impl: Vec<Census>,
}

/// Runs every class edit over a fresh package, then the platform edit, B553's
/// entry impl and the leaf edits over their own, answering the divergences and
/// every incremental census.
fn replay_the_classes() -> Replayed {
    let mut divergences = Vec::new();
    let mut censuses = Vec::new();
    vilan_core::analyzer::base_cache_clear();
    let mut package = classes_package();
    // The first analysis of the package, before any edit: a cold world.
    let first = observe(&package, vec![package.path("views.vl")], Leg::Incremental);
    let clean = observe(&package, Vec::new(), Leg::Clean);
    if first.rendering != clean.rendering {
        divergences.push(format!(
            "the cold analysis differs from the clean one at {}",
            first_difference(&first.rendering, &clean.rendering)
        ));
    }
    for edit in CLASS_EDITS {
        censuses.extend(replay(&mut package, edit, &mut divergences));
    }
    censuses.extend(replay_std_edit(&package, &mut divergences));
    let mut served = Vec::new();
    for edit in SERVED_IMPL_EDITS {
        served.extend(replay(&mut package, edit, &mut divergences));
    }
    censuses.extend(served.iter().copied());
    package.remove();
    let mut package = Package::write(
        "entry_impl",
        Platform::default(),
        &[
            ("main.vl", ENTRY_IMPL_MAIN),
            ("shapes.vl", ENTRY_IMPL_SHAPES),
            ("waver.vl", ENTRY_IMPL_WAVER),
        ],
    );
    let mut entry_impl = Vec::new();
    for edit in ENTRY_IMPL_EDITS {
        entry_impl.extend(replay(&mut package, edit, &mut divergences));
    }
    censuses.extend(entry_impl.iter().copied());
    package.remove();
    let mut package = platform_package();
    for edit in PLATFORM_EDITS {
        censuses.extend(replay(&mut package, edit, &mut divergences));
    }
    package.remove();
    let mut package = leaf_package();
    for edit in LEAF_EDITS {
        censuses.extend(replay(&mut package, edit, &mut divergences));
    }
    package.remove();
    // M110 S2a: the bound audit's verdicts at PREFIX sites are recorded and
    // replayed; the plants that serve a stale record (the audit skipped
    // without recording, the impl guard off) must turn this package red.
    for fixture in BOUND_FIXTURES
        .iter()
        .chain(WINDOW_FIXTURES)
        .chain(R11_FIXTURES)
        .chain(R11_RECORD_FIXTURES)
    {
        let mut package = fixture.write();
        for edit in fixture.edits {
            censuses.extend(replay(&mut package, edit, &mut divergences));
        }
        package.remove();
    }
    // B635's cold wraps and M138's cold instance record start from a served
    // analysis of the BASE text too: the world's records are filed over it,
    // and each keystroke replays them. Before the effect classes, which the
    // gate expects last.
    for fixture in B635_FIXTURES
        .iter()
        .chain(ASYNC_RECORD_FIXTURES)
        .chain(S3B_HELD_FIXTURES)
    {
        replay_from_the_base_text(fixture, "views.vl", &mut divergences, &mut censuses);
    }
    // M110 S3: the effect classes start from a served analysis of the BASE
    // text — the world's record is filed with every effect present, and each
    // edit then removes one (a seed that cannot shrink is the planted bug).
    for fixture in EFFECT_FIXTURES {
        replay_from_the_base_text(fixture, "effects.vl", &mut divergences, &mut censuses);
    }
    vilan_core::analyzer::base_cache_clear();
    Replayed {
        divergences,
        censuses,
        served,
        entry_impl,
    }
}

/// Writes `fixture`, analyzes its base text seeded at `seed` (the world's
/// records are filed over it) and compares it with a clean analysis, then
/// replays every edit and its undo.
fn replay_from_the_base_text(
    fixture: &Fixture,
    seed: &str,
    divergences: &mut Vec<String>,
    censuses: &mut Vec<Census>,
) {
    let mut package = fixture.write();
    let first = observe(&package, vec![package.path(seed)], Leg::Incremental);
    let clean = observe(&package, Vec::new(), Leg::Clean);
    if first.rendering != clean.rendering {
        divergences.push(format!(
            "{}: the first analysis differs from the clean one at {}",
            fixture.name,
            first_difference(&first.rendering, &clean.rendering)
        ));
    }
    for edit in fixture.edits {
        censuses.extend(replay(&mut package, edit, divergences));
    }
    package.remove();
}

/// **std-prefix P1's gate** (`std-prefix.md` §13): every §6 edit class and
/// every post-pass fixture, applied and undone, with the in-process std-only
/// prefix FORCED ON — the world stored is std alone, keyed by the std closure,
/// and every package module walks and resolves over it — answers what the
/// canonical clean analysis answers, and emits what a clean analysis of the
/// same shape emits. Non-vacuous on three counts: prefix worlds are built and
/// SERVED (the second analysis of every state hits the prefix the first one
/// stored), and §4.2's guard REFUSES the prefix where it must (the classes
/// package's hot trait impls answer questions std's resolve asked; its `bag`
/// is a use-inferred binding another module pushes into) — each refusal then
/// answering canonically too, which the comparison above holds.
#[test]
fn every_edit_class_answers_what_a_clean_analysis_answers_over_the_std_prefix() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    vilan_core::incremental::set_std_prefix(Some(true));
    let Replayed {
        divergences,
        censuses,
        ..
    } = replay_the_classes();
    let mut divergences = divergences;
    let mut censuses = censuses;
    for fixture in POST_PASS_FIXTURES {
        let mut package = fixture.write();
        for edit in fixture.edits {
            censuses.extend(replay(&mut package, edit, &mut divergences));
        }
        package.remove();
    }
    vilan_core::analyzer::base_cache_clear();
    vilan_core::incremental::set_std_prefix(None);
    assert!(
        divergences.is_empty(),
        "{} step(s) of the edit script observe the std-prefix analysis:\n{}",
        divergences.len(),
        divergences.join("\n")
    );
    let prefix_worlds = censuses.iter().filter(|census| census.prefix_world).count();
    let prefix_hits: u64 = censuses.iter().map(|census| census.prefix_hits).sum();
    let refusals: Vec<&'static str> = censuses
        .iter()
        .filter_map(|census| census.prefix_refusal)
        .collect();
    eprintln!(
        "{} analyses: {prefix_worlds} over the std prefix, {prefix_hits} prefix hits, \
         refusals {refusals:?}",
        censuses.len()
    );
    assert!(
        prefix_worlds >= 8 && prefix_hits >= 8,
        "the script must build and serve the std prefix (worlds {prefix_worlds}, hits \
         {prefix_hits}); the comparison above is otherwise vacuous about P1"
    );
    assert!(
        refusals.is_empty(),
        "no package of the script writes an impl a std question reaches, so nothing \
         refuses the prefix: {refusals:?}"
    );
}

/// **B624** (std-prefix P2's precondition): over the std-only prefix every
/// package module re-walks, so M110 S7's item records are filed and served for
/// the whole package — and an UNEDITED module's record must go stale when an
/// EDITED module pushes a different type into its use-inferred binding (M139's
/// write edge, `id_windows.cross_writes`). The classes package's `bag` /
/// `a_spoil` pair alone: `a_spoil` loads first and its push moves from `2` to
/// `"two"`; `bag`'s declaration label must read the clean `List<any>`, not its
/// record's `List<i32>`. Non-vacuous: the analyses are prefix worlds and serve
/// item records (the bystander `side` and the entry's `twice`, which read
/// nothing the edit dirties).
#[test]
fn a_push_into_an_unedited_modules_binding_stales_its_item_record_over_the_std_prefix() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    vilan_core::analyzer::base_cache_clear();
    vilan_core::incremental::set_std_prefix(Some(true));
    let mut package = Package::write(
        "b624",
        Platform::default(),
        &[
            (
                "main.vl",
                "import pkg::bag::fill;\nimport pkg::a_spoil::spoil;\nimport pkg::side::side_value;\n\nfun main() {\n\tfill();\n\tspoil();\n\tprint(twice());\n}\n\nfun twice(): i32 {\n\tside_value() * 2\n}\n",
            ),
            ("bag.vl", CLASSES_BAG),
            ("a_spoil.vl", CLASSES_SPOIL),
            ("side.vl", CLASSES_SIDE),
        ],
    );
    let mut divergences = Vec::new();
    let seed = package.path("a_spoil.vl");
    let first = observe(&package, vec![seed.clone()], Leg::Incremental);
    let mut censuses = vec![first.census];
    for edit in CLASS_EDITS.iter().filter(|edit| edit.file == "a_spoil.vl") {
        censuses.extend(replay(&mut package, edit, &mut divergences));
    }
    package.remove();
    vilan_core::analyzer::base_cache_clear();
    vilan_core::incremental::set_std_prefix(None);
    assert!(
        divergences.is_empty(),
        "the item records over the std prefix answer what a clean analysis does:\n{}",
        divergences.join("\n")
    );
    assert!(
        censuses.iter().all(|census| census.prefix_world),
        "every analysis is a prefix world: {censuses:#?}"
    );
    let served: u64 = censuses
        .iter()
        .map(|census| census.item_records_served)
        .sum();
    assert!(
        served > 0,
        "the prefix worlds serve item records, or the comparison is vacuous: {censuses:#?}"
    );
}

/// **std-prefix P2's gate** (`std-prefix.md` §13, incr-51c's Q1 as ruled): the
/// prefix LAYERED under the world build. Every §6 edit class, applied and
/// undone, with layering on — S1's hot-set worlds (and the canonical ones)
/// that MISS are built over the std-only prefix and stored under their own
/// keys; their hits are S1's as before — answers what the canonical clean
/// analysis answers and emits what a clean analysis of the same (layered)
/// shape emits. Non-vacuous: worlds are built layered, the prefix is SERVED
/// to a later world build (a second hot set in the package takes std from
/// memory), S1's hot-set hits still serve item records over a layered world,
/// and the guards still refuse where they must (each refusal answering
/// canonically, which the comparison holds).
#[test]
fn every_edit_class_answers_what_a_clean_analysis_answers_with_the_prefix_layered() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    vilan_core::incremental::set_std_prefix_layering(Some(true));
    let Replayed {
        divergences,
        censuses,
        ..
    } = replay_the_classes();
    let mut divergences = divergences;
    let mut censuses = censuses;
    for fixture in POST_PASS_FIXTURES {
        let mut package = fixture.write();
        for edit in fixture.edits {
            censuses.extend(replay(&mut package, edit, &mut divergences));
        }
        package.remove();
    }
    vilan_core::analyzer::base_cache_clear();
    vilan_core::incremental::set_std_prefix_layering(None);
    assert!(
        divergences.is_empty(),
        "{} step(s) of the edit script observe the layered analysis:\n{}",
        divergences.len(),
        divergences.join("\n")
    );
    let layered = censuses
        .iter()
        .filter(|census| census.prefix_layered)
        .count();
    let prefix_hits: u64 = censuses
        .iter()
        .filter(|census| census.prefix_layered)
        .map(|census| census.prefix_hits)
        .sum();
    let hot_hits = censuses
        .iter()
        .filter(|census| census.hot_world && census.base_hits > 0)
        .count();
    let served: u64 = censuses
        .iter()
        .map(|census| census.item_records_served)
        .sum();
    eprintln!(
        "{} analyses: {layered} layered world builds ({prefix_hits} over a served prefix), \
         {hot_hits} hot-set hits, {served} item records served",
        censuses.len()
    );
    assert!(
        layered >= 8 && prefix_hits >= 4,
        "the script must build worlds over the prefix and serve the prefix to later builds \
         (layered {layered}, prefix hits {prefix_hits}); the comparison is otherwise vacuous \
         about P2"
    );
    assert!(
        hot_hits >= 8 && served > 0,
        "S1's hot-set worlds stay what a keystroke is served from (hot-set hits {hot_hits}, \
         item records served {served})"
    );
}

/// A package impl of a std trait whose member std's own resolve asks about —
/// kolt's `impl ProxySignal<type T> with Signal<T>` and std's `get` lookups —
/// for the std-prefix tests below.
const PREFIX_PROXY: &str = "import std::reactive::{ Signal, Subscriber, Subscription };\n\n\
export struct ProxySignal<T> {\n\tget: || T,\n\tset: |T| void,\n\
\ton_settle: |Subscriber| Subscription,\n\tnotify: || void,\n}\n\n\
export impl ProxySignal<type T> with Signal<T> {\n\tfun get(self): T {\n\t\t(self.get)()\n\t}\n\n\
\t[must_use]\n\tfun on_settle(self, subscriber: Subscriber): Subscription {\n\
\t\t(self.on_settle)(subscriber)\n\t}\n\n\tfun notify(self) {\n\t\t(self.notify)();\n\t}\n\n\
\tfun set(self, value: T) {\n\t\t(self.set)(value);\n\t}\n}\n";

/// The incremental analysis of `package` with the std prefix forced on, and
/// the canonical clean one, which must agree.
fn observe_over_the_prefix(package: &Package) -> Observation {
    vilan_core::incremental::set_std_prefix(Some(true));
    let incremental = observe(package, Vec::new(), Leg::Incremental);
    vilan_core::incremental::set_std_prefix(None);
    let clean = observe(package, Vec::new(), Leg::Clean);
    assert_eq!(
        incremental.rendering, clean.rendering,
        "an analysis over the std prefix answers what the clean one does"
    );
    incremental
}

/// **std-prefix P1, §4.3's key: the std CLOSURE.** Two packages that share no
/// module and spell their std imports differently (`std::path::{ basename }`
/// against `std::path` and `std::path::basename` through a module import) but
/// reach one std set share ONE prefix: the second is served the world the first
/// stored, and walks its own package over it. A package whose std set is a
/// SUPERSET is never served the subset's prefix (§4.3: extra std modules can
/// change answers).
#[test]
fn packages_with_one_std_closure_share_one_std_prefix() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    vilan_core::analyzer::base_cache_clear();
    let first = Package::write(
        "prefix_first",
        NODE,
        &[
            (
                "main.vl",
                "import pkg::alpha::named;\n\nfun main() {\n\tprint(named());\n}\n",
            ),
            (
                "alpha.vl",
                "import std::path::{ basename };\n\nexport fun named(): str {\n\tbasename(\"/x/y.txt\")\n}\n",
            ),
        ],
    );
    let second = Package::write(
        "prefix_second",
        NODE,
        &[
            (
                "main.vl",
                "import pkg::beta::other;\nimport std::path;\n\nfun main() {\n\tprint(other());\n}\n",
            ),
            (
                "beta.vl",
                "import std::path;\n\nexport fun other(): str {\n\tpath::basename(\"/a/b.txt\")\n}\n",
            ),
        ],
    );
    let superset = Package::write(
        "prefix_superset",
        NODE,
        &[
            (
                "main.vl",
                "import pkg::gamma::third;\n\nfun main() {\n\tprint(third());\n}\n",
            ),
            (
                "gamma.vl",
                "import std::path::{ basename };\nimport std::base64;\n\n\
                 export fun third(): str {\n\tbase64::encode(basename(\"/q/r.txt\").to_utf8())\n}\n",
            ),
        ],
    );
    let stored = observe_over_the_prefix(&first);
    let served = observe_over_the_prefix(&second);
    let missed = observe_over_the_prefix(&superset);
    for package in [first, second, superset] {
        package.remove();
    }
    vilan_core::analyzer::base_cache_clear();
    assert!(
        stored.census.prefix_world && stored.census.prefix_hits == 0,
        "the first package builds the prefix: {:?}",
        stored.census
    );
    assert!(
        served.census.prefix_world && served.census.prefix_hits == 1,
        "a package with the same std closure is served the stored prefix: {:?}",
        served.census
    );
    assert!(
        missed.census.prefix_world && missed.census.prefix_hits == 0,
        "a superset std set builds its own prefix: {:?}",
        missed.census
    );
}

/// A package impl on a std CONCRETE type std's own resolve asks about —
/// `SignalCell<T>`'s `get`, asked of `SignalCell<T>` itself — for the guard's
/// refusing case.
const PREFIX_PEEK: &str = "import std::reactive::SignalCell;\n\n\
export trait Peek {\n\tfun get(self): i32;\n}\n\n\
export impl SignalCell<type T> with Peek {\n\tfun get(self): i32 {\n\t\t0\n\t}\n}\n";

/// **std-prefix P1, §4.2's guard.** A package impl that could answer a
/// question std's own resolve asked — here a trait impl on std's
/// `SignalCell<T>` declaring `get`, which std's reactive code looks up on
/// `SignalCell<T>` — refuses the prefix: the analysis is built in the shape it
/// would have taken without it (the canonical one: no hot seed), answers what
/// the clean analysis answers, and the next analysis of the same package does
/// not try again.
#[test]
fn a_package_impl_a_std_question_reaches_refuses_the_std_prefix() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    vilan_core::analyzer::base_cache_clear();
    let package = Package::write(
        "prefix_reached",
        NODE,
        &[
            (
                "main.vl",
                "import pkg::peek::Peek;\n\nfun main() {\n\tprint(\"ok\");\n}\n",
            ),
            ("peek.vl", PREFIX_PEEK),
        ],
    );
    let refused = observe_over_the_prefix(&package);
    let remembered = observe_over_the_prefix(&package);
    package.remove();
    vilan_core::analyzer::base_cache_clear();
    assert!(
        !refused.census.prefix_world
            && refused.census.prefix_refusals == 1
            && refused.census.prefix_refusal == Some("impl-reached"),
        "the package's impl refuses the prefix: {:?}",
        refused.census
    );
    assert_eq!(
        remembered.census.prefix_refusal,
        Some("refused-before"),
        "a refused package is not built over the prefix again: {:?}",
        remembered.census
    );
}

/// kolt's `src/lib/scale_step.vl`: a package struct made callable (B340).
/// std asks whether its closure types are `Callable`, and the call operator's
/// coercion admitted this impl for them (M145's second refusal on kolt).
const PREFIX_SCALE: &str = "import std::operators::Callable;\n\n\
export struct Scale {\n\tunit: f64,\n}\n\n\
export impl Scale with Callable {\n\tfun call(self, n: f64): f64 {\n\t\tself.unit * n\n\t}\n}\n";

/// **M145: the guard asks what the LOOKUP asked.** kolt's `impl
/// ProxySignal<type T> with Signal<T>` declares `get`, and std's reactive code
/// looks `get` up on its own generic `T` (`fun f<T: Signal<U>>`, a parameter
/// the declaration owns and the body's comparison holds RIGID). That lookup
/// admits no impl on a concrete head — only a blanket could answer it — yet the
/// verdict, read after the resolve with no rigidity in force, read `T` as a
/// hole and refused kolt's client. The question now remembers the rigidity it
/// was asked under and the verdict re-asks the admission under it. And kolt's
/// callable `Scale` (B340): std asks whether `|T| R` is `Callable`, and the
/// call operator's COERCION admitted `impl Scale with Callable` for a closure
/// receiver — an impl subject never coerces now. The package is SERVED over
/// the prefix, answering what the clean analysis answers.
#[test]
fn m145_a_package_impl_on_its_own_type_is_served_over_the_std_prefix() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    vilan_core::analyzer::base_cache_clear();
    let package = Package::write(
        "prefix_proxy",
        NODE,
        &[
            (
                "main.vl",
                "import pkg::proxy::ProxySignal;\nimport pkg::scale::Scale;\n\
                 import std::reactive::{ Signal, SignalCell, Subscriber, Source };\n\n\
                 fun main() {\n\tprint(Scale { unit = 2f }(3f));\n\tlet cell = SignalCell::new(1);\n\
                 \tlet proxy = ProxySignal {\n\t\tget = || cell.get(),\n\
                 \t\tset = |value: i32| cell.set(value),\n\
                 \t\ton_settle = |subscriber: Subscriber| cell.on_settle(subscriber),\n\
                 \t\tnotify = || cell.notify(),\n\t};\n\tproxy.set(2);\n\tprint(proxy.get());\n}\n",
            ),
            ("proxy.vl", PREFIX_PROXY),
            ("scale.vl", PREFIX_SCALE),
        ],
    );
    let served = observe_over_the_prefix(&package);
    let again = observe_over_the_prefix(&package);
    package.remove();
    vilan_core::analyzer::base_cache_clear();
    assert!(
        served.census.prefix_world && served.census.prefix_refusals == 0,
        "the package's impl on its own type is served over the prefix: {:?}",
        served.census
    );
    assert!(
        again.census.prefix_world && again.census.prefix_hits == 1,
        "and the next analysis is served the stored prefix: {:?}",
        again.census
    );
}

/// **The gate.** Every §6 edit class, applied and undone, answers what a clean
/// analysis answers — byte for byte over everything an editor reads, and the
/// emitted JS wherever the program emits.
#[test]
fn every_edit_class_answers_what_a_clean_analysis_answers() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    let Replayed {
        divergences,
        censuses,
        served,
        entry_impl,
    } = replay_the_classes();
    let mut divergences = divergences;
    let mut censuses = censuses;
    // M110 S3: the classes leg ends with the effect fixtures; the last two
    // censuses are the browser package's stand-down edit and its undo.
    let effects_end = censuses.len();
    // The post-pass class (pass map §6 "does not prove" 2; B575): a prefix
    // module's verdict a POST pass decides, under keystrokes elsewhere. In
    // the gate only: no plant targets a post pass, and each replay of them
    // is 48 analyses.
    for fixture in POST_PASS_FIXTURES {
        let mut package = fixture.write();
        for edit in fixture.edits {
            censuses.extend(replay(&mut package, edit, &mut divergences));
        }
        package.remove();
    }
    vilan_core::analyzer::base_cache_clear();
    assert!(
        divergences.is_empty(),
        "{} step(s) of the edit script observe incremental analysis:\n{}",
        divergences.len(),
        divergences.join("\n")
    );
    // Non-vacuity: the script has to have exercised the hot-set world, hit and
    // miss, or the comparison above says nothing about S1. Every edit here is
    // outside the entry, so without S1 nothing would be reused at all.
    let hot_worlds = censuses.iter().filter(|census| census.hot_world).count();
    let hot_hits = censuses
        .iter()
        .filter(|census| census.hot_world && census.base_hits > 0)
        .count();
    let replayed = censuses
        .iter()
        .filter(|census| census.records_replayed > 0)
        .count();
    eprintln!(
        "{} incremental analyses: {hot_worlds} hot-set worlds, {hot_hits} of them served \
         from the base cache, {replayed} replaying module records",
        censuses.len()
    );
    assert!(
        hot_hits >= 8 && replayed >= 8,
        "the script must exercise the hot-set world from the cache (served {hot_hits}, \
         replaying {replayed}); the differential above is otherwise vacuous about S1"
    );
    // M121: the hot impls no prefix question reaches are SERVED — a hot-set
    // world, the warm keystroke from the cache — not refused, which is what
    // S1's spelling guard did to every one of them.
    assert!(
        served
            .iter()
            .all(|census| census.hot_world && census.hot_refusal.is_none()),
        "a hot impl the stored prefix never asked about is served by the hot-set world: \
         {served:#?}"
    );
    // Each edit's first analysis stores the prefix its new seed keys; the undo
    // after it is the warm keystroke, served from that prefix.
    assert!(
        served
            .iter()
            .skip(1)
            .step_by(2)
            .all(|census| census.base_hits > 0),
        "the warm keystrokes reuse the stored prefix: {served:#?}"
    );
    // M110 S2a: the bound audit served PREFIX sites from the record on the
    // warm keystrokes (the undo of every edit is one) — or the comparison
    // above said nothing about the record.
    let bound_served: u64 = censuses
        .iter()
        .map(|census| census.bound_sites_served)
        .sum();
    let bound_checked: u64 = censuses
        .iter()
        .map(|census| census.bound_sites_checked)
        .sum();
    eprintln!("bound audit: {bound_served} sites served from records, {bound_checked} checked");
    assert!(
        bound_served > 0 && bound_checked > 0,
        "the classes leg must serve bound-audit sites from a reused module's record \
         (served {bound_served}, checked {bound_checked})"
    );
    // M110 S4: the const cache served sites — or every comparison above said
    // nothing about it.
    let const_hits: u64 = censuses.iter().map(|census| census.const_cache_hits).sum();
    assert!(
        const_hits > 0,
        "the classes leg must serve const sites from the const cache"
    );
    // M110 S3 (Order 50): the warm keystrokes took the call graph from the
    // world's record and seeded the async fixpoint from its cold result — the
    // "one seeded pass" counter — or the comparison above said nothing about
    // S3. The effect fixtures (`EFFECT_FIXTURES`) are replayed last, two
    // censuses per edit (the edit, then its undo), each a served keystroke.
    let graphs_replayed: u64 = censuses.iter().map(|census| census.graphs_replayed).sum();
    let logs_replayed: u64 = censuses
        .iter()
        .map(|census| census.context_log_replayed)
        .sum();
    let seeded: u64 = censuses.iter().map(|census| census.seeded_passes).sum();
    eprintln!(
        "S3: {graphs_replayed} call graphs replayed from records, {logs_replayed} of them after \
         the rewrite, {seeded} seeded fixpoints"
    );
    assert!(
        graphs_replayed > 0 && logs_replayed > 0 && seeded > 0,
        "the classes leg must replay the call graphs and seed the async fixpoint from the \
         world's record (graphs {graphs_replayed}, after the rewrite {logs_replayed}, seeded \
         {seeded})"
    );
    // M110 S6's standing counter pins, over every incremental analysis of the
    // leg: no relocated mint left its anchor's window, and the served
    // keystrokes inherited the stored world's windows. A constraint's write
    // into ANOTHER item's window is reported here and pinned at zero on the
    // leaf package (`a_keystroke_rewalks_its_hot_set_and_nothing_else`): the
    // classes package pushes into another module's use-inferred binding on
    // purpose (`bag`/`a_spoil`, S1's third hazard), and that push IS a
    // constraint of one item writing a slot minted for another — the
    // cross-item edge a per-item record (S7) carries, not an invariant.
    let writes_other: u64 = censuses
        .iter()
        .map(|census| census.window_writes_other)
        .sum();
    let outside_anchor: u64 = censuses
        .iter()
        .map(|census| census.window_outside_anchor)
        .sum();
    let laid_out: u64 = censuses.iter().map(|census| census.windows_laid_out).sum();
    let inherited: u64 = censuses.iter().map(|census| census.windows_inherited).sum();
    eprintln!(
        "S6: {laid_out} windows laid out, {inherited} inherited; writes into another item's \
         window {writes_other}; mints outside their anchor {outside_anchor}"
    );
    assert_eq!(
        outside_anchor, 0,
        "a relocated mint left its anchor's window"
    );
    assert!(
        inherited > 0,
        "the served keystrokes inherit the stored world's windows"
    );
    // M110 S7 (Order 51): the warm keystrokes SERVED per-item records for
    // the re-walked files' unchanged items — and found some stale (the
    // edited item, its dependents) — or the comparison above said nothing
    // about the records. The effect edits (a context read removed, a sleep
    // removed, a node call removed, a push into another module's binding)
    // are among the legs that served.
    let items_served: u64 = censuses
        .iter()
        .map(|census| census.item_records_served)
        .sum();
    let items_stale: u64 = censuses
        .iter()
        .map(|census| census.item_records_stale)
        .sum();
    let items_filed: u64 = censuses
        .iter()
        .map(|census| census.item_records_filed)
        .sum();
    eprintln!("S7: {items_served} item records served, {items_stale} stale, {items_filed} filed");
    assert!(
        items_served > 0 && items_stale > 0 && items_filed > 0,
        "the classes leg must serve per-item records on its warm keystrokes and find the \
         edited items stale (served {items_served}, stale {items_stale}, filed {items_filed})"
    );
    // The stand-down edit (the browser effect package's last edit, replayed
    // last: edit then undo) applies no rewrite, so the recorded post-rewrite
    // graph is NOT served to it — the log guard.
    let stand_down = &censuses[effects_end - 2];
    assert!(
        stand_down.hot_world && stand_down.base_hits > 0 && stand_down.context_log_replayed == 0,
        "a keystroke that stands the rewrite down must not be served the recorded \
         post-rewrite graph: {stand_down:#?}"
    );
    // B553: the entry's impl is one a STORED module calls, so the entry's own
    // keystrokes resolve the world once, after the entry walks — and agree.
    // A keystroke in the calling module makes it hot: it resolves after the
    // entry walked anyway, so its world is served as an ordinary hot set.
    let (entry_keystrokes, caller_keystrokes) = entry_impl.split_at(2);
    assert!(
        entry_keystrokes
            .iter()
            .all(|census| census.resolve_deferred && !census.hot_world),
        "an entry impl a stored module calls defers the world's resolve: {entry_impl:#?}"
    );
    assert!(
        caller_keystrokes
            .iter()
            .all(|census| !census.resolve_deferred && census.hot_world),
        "the calling module, hot, is served without deferring: {entry_impl:#?}"
    );
}

/// Non-vacuity of the post-pass class: each of its packages carries, in its
/// PREFIX module, a diagnostic a post pass decides — or the class says nothing
/// about what a reusing analysis drops.
#[test]
fn each_post_pass_package_carries_a_prefix_verdict() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    for fixture in POST_PASS_FIXTURES {
        let package = fixture.write();
        let clean = observe(&package, Vec::new(), Leg::Clean);
        let module = package.path("m.vl").display().to_string();
        package.remove();
        let diagnostics = clean
            .rendering
            .split("# warnings")
            .next()
            .unwrap_or("")
            .lines()
            .filter(|row| row.starts_with(&module))
            .count();
        assert!(
            diagnostics >= 1,
            "{}: the prefix module carries a post-pass verdict:\n{}",
            fixture.name,
            clean.rendering
        );
    }
}

// --- the corpus ----------------------------------------------------------------

/// **The corpus leg.** Every corpus program, re-hosted as the module of a
/// package with an importer (`user.vl`, in the module's hot set) and a
/// bystander (`side.vl`, in the stored prefix), edited and undone as above.
/// The corpus programs are not written to be modules and many are not clean
/// ones: what each means as a module it means identically to both analyses,
/// which is all the leg asks.
#[test]
fn the_corpus_as_modules_answers_what_a_clean_analysis_answers() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    let corpus = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/test");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&corpus)
        .expect("corpus directory")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            (path.extension()? == "vl").then_some(path)
        })
        .collect();
    paths.sort();
    assert!(paths.len() > 60, "suspiciously few corpus programs");
    vilan_core::analyzer::base_cache_clear();
    let results: Vec<(String, Vec<String>, Vec<Census>)> = std::thread::scope(|scope| {
        let workers: Vec<_> = paths
            .chunks(paths.len().div_ceil(test_pool::workers(16)).max(1))
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|path| {
                            let mut source =
                                std::fs::read_to_string(path).expect("read corpus file");
                            source.push_str(PROBE_REFUSAL);
                            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
                            let mut package = Package::write(
                                &name,
                                Platform::default(),
                                &[
                                    ("main.vl", CORPUS_MAIN),
                                    ("module.vl", &source),
                                    ("user.vl", CORPUS_USER),
                                    ("side.vl", CORPUS_SIDE),
                                ],
                            );
                            let mut divergences = Vec::new();
                            let mut censuses = Vec::new();
                            for edit in CORPUS_EDITS {
                                censuses.extend(replay(&mut package, edit, &mut divergences));
                            }
                            package.remove();
                            (name, divergences, censuses)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("worker panicked"))
            .collect()
    });
    vilan_core::analyzer::base_cache_clear();
    let divergences: Vec<String> = results
        .iter()
        .flat_map(|(name, divergences, _)| {
            divergences
                .iter()
                .map(move |divergence| format!("{name}: {divergence}"))
        })
        .collect();
    assert!(
        divergences.is_empty(),
        "{} corpus step(s) observe incremental analysis:\n{}",
        divergences.len(),
        divergences.join("\n")
    );
    // Non-vacuity: the corpus has to have run through the hot-set world, from
    // the cache, often enough to say something about it. Many corpus programs
    // are refused one (an impl on a std type, a use-inferred binding), which is
    // the guards working; the floor is on the ones that were not.
    let censuses: Vec<&Census> = results
        .iter()
        .flat_map(|(_, _, censuses)| censuses)
        .collect();
    let hot_hits = censuses
        .iter()
        .filter(|census| census.hot_world && census.base_hits > 0)
        .count();
    let hot_worlds = censuses.iter().filter(|census| census.hot_world).count();
    eprintln!(
        "{} incremental corpus analyses: {hot_worlds} hot-set worlds, {hot_hits} served from the base cache",
        censuses.len()
    );
    assert!(
        hot_hits >= 20,
        "only {hot_hits} corpus analyses were hot-set worlds served from the cache — the leg \
         says too little about S1"
    );
}

// --- the planted bugs (Q3's non-vacuity) -----------------------------------------

/// The classes leg with `plant` planted: answers the divergences it found.
fn replay_with_plant(plant: Plant) -> Vec<String> {
    set_plant(Some(plant));
    let replayed = std::panic::catch_unwind(replay_the_classes);
    set_plant(None);
    replayed
        .expect("the classes leg panicked under a plant")
        .divergences
}

/// S3b's plant (M110, Order 50): the async fixpoint seeded from the recording
/// analysis's SETTLED set, hot nodes included. The hot leaf's `sleep` removed
/// then leaves `wait` async in the seed — a monotone fixpoint cannot shrink —
/// and the emitted JS keeps an `async` a clean analysis drops.
#[test]
fn the_differential_sees_a_fixpoint_seeded_from_the_settled_set() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let divergences = replay_with_plant(Plant::PostSeedFromSettled);
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("the async set must shrink")),
        "the settled-seed plant must turn the sleep's removal red; it found: {divergences:#?}"
    );
}

// --- M159: the LONE open document (M104's hybrid, the editor's common row) ---

/// [`replay`] for a LONE open document: `edit.file` is the analysis's entry
/// in file mode, seeded with its own path as the language server seeds it
/// (M159 door (a)); each phase is compared with the clean analysis of the
/// same open file. A seed that is the entry names no hot module, so the
/// canonical clean analysis is the JS comparison's too.
fn replay_lone(package: &mut Package, edit: &Edit, divergences: &mut Vec<String>) -> Vec<Census> {
    let before = package.text(edit.file).to_string();
    let after = edit.apply(&before);
    let mut censuses = Vec::new();
    for (phase, text) in [("edit", after), ("undo", before)] {
        package.set(edit.file, text);
        let incremental = observe_lone(package, edit.file, Leg::Incremental);
        let clean = observe_lone(package, edit.file, Leg::Clean);
        compare_lone(
            &format!("{} ({phase}, {} open alone)", edit.label, edit.file),
            &incremental,
            &clean,
            divergences,
        );
        censuses.push(incremental.census);
    }
    censuses
}

/// The lone leg's comparison: the rendering, the emitted JS, and whether
/// the program emits at all.
fn compare_lone(
    label: &str,
    incremental: &Observation,
    clean: &Observation,
    divergences: &mut Vec<String>,
) {
    if incremental.rendering != clean.rendering {
        divergences.push(format!(
            "{label}: the incremental analysis differs from the clean one at {}",
            first_difference(&incremental.rendering, &clean.rendering)
        ));
    }
    if incremental.javascript != clean.javascript {
        divergences.push(format!(
            "{label}: the emitted JS differs from a clean analysis (incremental {} bytes, \
             clean {} bytes)",
            incremental.javascript.as_ref().map_or(0, String::len),
            clean.javascript.as_ref().map_or(0, String::len),
        ));
    }
}

/// Every fixture edit typed into a package MODULE, replayed with that module
/// open ALONE (M159): the open analysis files the world's records, each
/// keystroke and its undo is served from them. Answers the divergences and
/// the warm censuses (the open's excluded).
fn replay_the_lone_documents() -> (Vec<String>, Vec<Census>) {
    let mut divergences = Vec::new();
    let mut censuses = Vec::new();
    vilan_core::analyzer::base_cache_clear();
    for fixture in LONE_FIXTURES
        .iter()
        .chain(R11_FIXTURES)
        .chain(R11_RECORD_FIXTURES)
        .chain(EFFECT_FIXTURES)
        .chain(B635_FIXTURES)
        .chain(ASYNC_RECORD_FIXTURES)
        .chain(S3B_HELD_FIXTURES)
        .chain(POST_PASS_FIXTURES)
        .chain(BOUND_FIXTURES)
        .chain(WINDOW_FIXTURES)
    {
        let mut package = fixture.write();
        let mut opened: Vec<&str> = Vec::new();
        for edit in fixture
            .edits
            .iter()
            .filter(|edit| edit.seed.is_none() && edit.file != "main.vl")
        {
            if !opened.contains(&edit.file) {
                opened.push(edit.file);
                let open = observe_lone(&package, edit.file, Leg::Incremental);
                let clean = observe_lone(&package, edit.file, Leg::Clean);
                compare_lone(
                    &format!("{}: {} opened alone", fixture.name, edit.file),
                    &open,
                    &clean,
                    &mut divergences,
                );
            }
            censuses.extend(replay_lone(&mut package, edit, &mut divergences));
        }
        package.remove();
    }
    vilan_core::analyzer::base_cache_clear();
    (divergences, censuses)
}

/// **M159** (door (a), the owner's ruling): a LONE open document — the
/// editor's common row, a file that is the only open document of its world
/// (M104's hybrid) — is analyzed as its own entry, SEEDED with its own path,
/// and answers what a clean analysis of the same open file answers on every
/// fixture keystroke and undo. Non-vacuous: every warm keystroke found the
/// post passes' seam seeded, its record filed and matching, S3b's async cold
/// seed in it (the acceptance incr-53c's probe read `seeded false` on); the
/// call graphs were replayed and the async fixpoint seeded from the record;
/// the hover rows were restored.
#[test]
fn m159_a_lone_open_document_answers_what_a_clean_analysis_answers() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    let (divergences, censuses) = replay_the_lone_documents();
    assert!(
        divergences.is_empty(),
        "{} step(s) of the lone-document script observe the incremental analysis:\n{}",
        divergences.len(),
        divergences.join("\n")
    );
    let full_seam = vilan_core::incremental::PostSeamState {
        seeded: true,
        record: true,
        matches: true,
        cold: true,
    };
    let served: Vec<&Census> = censuses
        .iter()
        .filter(|census| census.base_hits > 0)
        .collect();
    assert!(
        !served.is_empty() && served.len() * 2 >= censuses.len(),
        "the lone keystrokes are served from the world the open stored ({} of {})",
        served.len(),
        censuses.len()
    );
    assert!(
        served.iter().all(|census| census
            .post_seam
            .is_some_and(|seam| seam.seeded && seam.record)),
        "every served lone keystroke is seeded and finds the post-pass record: {served:#?}"
    );
    let full = served
        .iter()
        .filter(|census| census.post_seam == Some(full_seam))
        .count();
    let graphs: u64 = censuses.iter().map(|census| census.graphs_replayed).sum();
    let seeded: u64 = censuses.iter().map(|census| census.seeded_passes).sum();
    let hover: u64 = censuses
        .iter()
        .map(|census| census.hover_rows_restored)
        .sum();
    eprintln!(
        "M159: {} lone keystrokes, {} served, {full} with the full seam; {graphs} graphs \
         replayed, {seeded} seeded fixpoints, {hover} hover rows restored",
        censuses.len(),
        served.len()
    );
    assert!(
        full > 0 && graphs > 0 && seeded > 0 && hover > 0,
        "the lone keystrokes read the post-pass record (full seam {full}, graphs {graphs}, \
         seeded {seeded}) and the hover rows ({hover})"
    );
}

/// M159's plant: S3b's settled-seed plant (the async fixpoint seeded from the
/// recording analysis's SETTLED set) turns the LONE leg red too — the sleep
/// removed from `lone.vl` open alone leaves `pause_once` async in the seed,
/// and the module binding's initializer keeps a refusal a clean analysis
/// drops — so the lone keystrokes really do read the record. (A file-mode
/// program emits no JS, so the world-mode leg's `wait` cannot show it here.)
#[test]
fn m159_the_lone_leg_sees_a_fixpoint_seeded_from_the_settled_set() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(Some(Plant::PostSeedFromSettled));
    let replayed = std::panic::catch_unwind(replay_the_lone_documents);
    set_plant(None);
    let (divergences, _) = replayed.expect("the lone leg panicked under a plant");
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("the initializer's refusal must go")),
        "the settled-seed plant must turn the lone sleep removal red; it found: \
         {divergences:#?}"
    );
}

/// M138's fixture leg, on a cleared base cache: the base text's analysis
/// files the world's records, then every edit and its undo is compared with a
/// clean analysis. Answers the divergences and each step's census, labelled.
fn replay_the_async_record() -> (Vec<String>, Vec<(String, Census)>) {
    vilan_core::analyzer::base_cache_clear();
    let mut divergences = Vec::new();
    let mut steps: Vec<(String, Census)> = Vec::new();
    for fixture in ASYNC_RECORD_FIXTURES {
        let mut package = fixture.write();
        let first = observe(&package, vec![package.path("views.vl")], Leg::Incremental);
        let clean = observe(&package, Vec::new(), Leg::Clean);
        if first.rendering != clean.rendering {
            divergences.push(format!(
                "{}: the first analysis differs from the clean one at {}",
                fixture.name,
                first_difference(&first.rendering, &clean.rendering)
            ));
        }
        steps.push(("the base text".to_string(), first.census));
        for edit in fixture.edits {
            let censuses = replay(&mut package, edit, &mut divergences);
            steps.push((format!("{} (edit)", edit.label), censuses[0]));
            steps.push((format!("{} (undo)", edit.label), censuses[1]));
        }
        package.remove();
    }
    vilan_core::analyzer::base_cache_clear();
    (divergences, steps)
}

/// **M138's seeded form** (Order 54, R-c): the async worklist restarts from
/// the world's COLD instance record — the adaptation over the cold nodes
/// alone, each key's reads logged — and re-walks a recorded key only when
/// something it read moved. Every keystroke of the fixture and its undo
/// answers what a clean analysis answers; non-vacuous on every count: the
/// base text's analysis FILES the record, every keystroke is served recorded
/// keys, and the two edits that move a read (a dispatch candidate turned
/// async, an async closure assigned into a cold binding) re-walk keys. (The
/// classes leg replays the fixture too, behind every other.)
#[test]
fn m138_the_async_worklist_restarts_from_the_cold_instance_record() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    let (divergences, steps) = replay_the_async_record();
    assert!(
        divergences.is_empty(),
        "the M138 fixture observes the incremental analysis:\n{}",
        divergences.join("\n")
    );
    for (step, census) in &steps {
        eprintln!(
            "M138: {step}: async-seeded={}/{} filed={} bytes={}",
            census.async_keys_served,
            census.async_keys_rewalked,
            census.async_records_filed,
            census.async_record_bytes
        );
    }
    assert!(
        steps[0].1.async_records_filed == 1,
        "the base text's analysis files the cold instance record: {:#?}",
        steps[0].1
    );
    assert!(
        steps[1..]
            .iter()
            .all(|(_, census)| census.async_keys_served > 0),
        "every keystroke is served recorded keys"
    );
    let rewalked = |step: &str| -> u64 {
        steps
            .iter()
            .find(|(label, _)| label.contains(step) && label.ends_with("(edit)"))
            .map_or(0, |(_, census)| census.async_keys_rewalked)
    };
    let quiet = rewalked("recorded instance is served");
    for moved in ["dispatch", "assignment"] {
        assert!(
            rewalked(moved) > quiet,
            "the {moved} edit moves a read a recorded key made, which re-walks it \
             ({} against the quiet edit's {quiet})",
            rewalked(moved)
        );
    }
}

/// M138's plant: the cold instance record served without validating what its
/// keys READ — a hot impl turned async under a cold generic's dispatch, or an
/// async closure assigned into a cold binding, then leaves `relay`'s or
/// `twice_hook`'s recorded verdict (no adapted callee, nothing awaited) in
/// place, and the emitted JS keeps a sync caller a clean analysis awaits.
#[test]
fn the_differential_sees_an_async_record_served_past_a_moved_read() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(Some(Plant::AsyncRecordUnguarded));
    let replayed = std::panic::catch_unwind(replay_the_async_record);
    set_plant(None);
    let (divergences, _) = replayed.expect("the M138 leg panicked under a plant");
    for edit in ["dispatch", "assignment"] {
        assert!(
            divergences
                .iter()
                .any(|divergence| divergence.contains(edit) && divergence.contains("(M138)")),
            "the unguarded async record must turn the {edit} edit red; it found: \
             {divergences:#?}"
        );
    }
}

/// **B635**: the context rewrite mints every section's COLD rows before any
/// section's hot rows, so the recorded post-rewrite cold graph — replayed
/// whenever the cold rewrite log matches, renumbered by a uniform base shift —
/// names exactly the cold calls a graph built whole over the rewritten tree
/// names, whatever the hot rows mint. The fixture's keystroke makes a hot
/// function need the context beside a cold covered-to-safe wrap (a minted
/// `Some(..)` call); until B635 the hot hidden parameter minted before the cold
/// threaded arguments and moved the wrap's id under the replay. Non-vacuous:
/// the keystroke and its undo replay the recorded graph.
#[test]
fn b635_a_replayed_cold_graph_names_the_calls_a_rebuilt_one_names() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    vilan_core::analyzer::base_cache_clear();
    vilan_core::incremental::set_verify_graph_replays(true);
    let mut divergences = Vec::new();
    let mut censuses = Vec::new();
    for fixture in B635_FIXTURES {
        let mut package = fixture.write();
        let first = observe(&package, vec![package.path("views.vl")], Leg::Incremental);
        censuses.push(first.census);
        for edit in fixture.edits {
            censuses.extend(replay(&mut package, edit, &mut divergences));
        }
        package.remove();
    }
    vilan_core::incremental::set_verify_graph_replays(false);
    vilan_core::analyzer::base_cache_clear();
    assert!(
        divergences.is_empty(),
        "the B635 fixture observes the incremental analysis:\n{}",
        divergences.join("\n")
    );
    let replayed: u64 = censuses
        .iter()
        .map(|census| census.context_log_replayed)
        .sum();
    let mismatches: u64 = censuses
        .iter()
        .map(|census| census.graph_replay_mismatches)
        .sum();
    assert!(
        replayed >= 2,
        "the keystroke and its undo replay the recorded post-rewrite graph ({replayed}): \
         {censuses:#?}"
    );
    assert_eq!(
        mismatches, 0,
        "a replayed cold graph names cold calls a graph built whole does not ({replayed} \
         replays): the cold rewrite's mints moved with the hot rows"
    );
}

/// **M155**: R11's candidate rows are filed by the leaf's first (seeded)
/// analysis and restored on its keystrokes — the served keystroke walks a
/// fraction of the rows the filing one did — and the instantiation that
/// slices a SERVED item's re-based rows still refuses (the fixture's R11
/// refusal stands on every step; the classes leg compares every step with
/// the clean analysis).
#[test]
fn m155_r11_candidate_rows_are_restored_on_a_served_keystroke() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    vilan_core::analyzer::base_cache_clear();
    let fixture = &R11_FIXTURES[0];
    let mut package = fixture.write();
    let seed = vec![package.path("views.vl")];
    let first = observe(&package, seed.clone(), Leg::Incremental);
    let warm = observe(&package, seed.clone(), Leg::Incremental);
    let edited = fixture.edits[0].apply(package.text("views.vl"));
    package.set("views.vl", edited);
    let served = observe(&package, seed.clone(), Leg::Incremental);
    let clean = observe(&package, Vec::new(), Leg::Clean);
    package.remove();
    vilan_core::analyzer::base_cache_clear();
    assert!(
        clean
            .rendering
            .contains("is not move-clean when instantiated with a resource"),
        "the fixture's R11 refusal: {}",
        clean.rendering
    );
    assert_eq!(
        served.rendering, clean.rendering,
        "the served keystroke answers clean"
    );
    eprintln!(
        "M155: r11 rows live: first {} warm {} served {} (items served {})",
        first.census.r11_rows_live,
        warm.census.r11_rows_live,
        served.census.r11_rows_live,
        served.census.item_records_served
    );
    assert!(
        first.census.r11_rows_live > 0
            && warm.census.r11_rows_live * 4 < first.census.r11_rows_live
            && served.census.r11_rows_live * 4 < first.census.r11_rows_live,
        "the warm and served keystrokes restore R11's rows rather than walking them \
         (first {}, warm {}, served {})",
        first.census.r11_rows_live,
        warm.census.r11_rows_live,
        served.census.r11_rows_live
    );
    assert!(
        served.census.item_records_served > 0,
        "the leaf's generics were served items: {:#?}",
        served.census
    );
}

/// **R11 door B** (incr-54b, Order 54; `analyzer/r11_records.rs`): the first
/// seeded analysis of the door B fixture files one instance record per
/// instantiation it scanned (every callee is a cold module's), a warm
/// keystroke on the leaf EMITS every instance from the records and scans
/// none, and the keystroke that types a `Drop` impl for the leaf's resource
/// moves the digest (the program's `Drop` set changed) — every instance is
/// scanned live again and refiled, the analysis answers what a clean one
/// answers (the field move out of `Pair<Conn>` is refused now that `Conn`
/// drops), and the next keystroke on the edited text serves every record.
#[test]
fn r11_door_b_serves_cold_instances_from_records_and_rescans_a_moved_digest() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    vilan_core::analyzer::base_cache_clear();
    let fixture = &R11_RECORD_FIXTURES[0];
    let mut package = fixture.write();
    let seed = vec![package.path("views.vl")];
    let first = observe(&package, seed.clone(), Leg::Incremental);
    let warm = observe(&package, seed.clone(), Leg::Incremental);
    let warm_clean = observe(&package, Vec::new(), Leg::Clean);
    let edited = fixture.edits[0].apply(package.text("views.vl"));
    package.set("views.vl", edited);
    let moved = observe(&package, seed.clone(), Leg::Incremental);
    let served = observe(&package, seed.clone(), Leg::Incremental);
    let clean = observe(&package, Vec::new(), Leg::Clean);
    package.remove();
    vilan_core::analyzer::base_cache_clear();
    assert_eq!(
        warm.rendering, warm_clean.rendering,
        "the warm keystroke answers clean"
    );
    assert!(
        clean
            .rendering
            .contains("a resource-typed field is moved out of a live aggregate"),
        "the Drop impl makes split's field move an R5 refusal: {}",
        clean.rendering
    );
    assert_eq!(
        moved.rendering, clean.rendering,
        "the keystroke that moves the digest answers clean"
    );
    assert_eq!(
        served.rendering, clean.rendering,
        "the keystroke served past the refiled records answers clean"
    );
    eprintln!(
        "R11 door B: first scanned {} filed {}; warm served {} scanned {}; \
         moved digest served {} scanned {} filed {}; then served {} scanned {}",
        first.census.r11_instances_scanned,
        first.census.r11_records_filed,
        warm.census.r11_instances_served,
        warm.census.r11_instances_scanned,
        moved.census.r11_instances_served,
        moved.census.r11_instances_scanned,
        moved.census.r11_records_filed,
        served.census.r11_instances_served,
        served.census.r11_instances_scanned,
    );
    assert!(
        first.census.r11_instances_scanned >= 5 && first.census.r11_records_filed >= 5,
        "the first analysis scans the five instances live and files them: {:#?}",
        first.census
    );
    assert!(
        warm.census.r11_instances_served == first.census.r11_instances_scanned
            && warm.census.r11_instances_scanned == 0,
        "the warm keystroke serves every instance from the records: {:#?}",
        warm.census
    );
    assert!(
        moved.census.r11_instances_served == 0
            && moved.census.r11_instances_scanned == first.census.r11_instances_scanned
            && moved.census.r11_records_filed == first.census.r11_records_filed,
        "the moved digest rescans every instance and refiles it: {:#?}",
        moved.census
    );
    assert!(
        served.census.r11_instances_served == first.census.r11_instances_scanned
            && served.census.r11_instances_scanned == 0,
        "the keystroke after the refiling serves every record again: {:#?}",
        served.census
    );
}

/// R11 door B's plant: an instance record served without its `Drop` digest
/// compared. The fixture's first edit types a `Drop` impl for the leaf's
/// resource; `split`'s field move out of `Pair<Conn>` is then no destructure
/// (B469) and the instantiation must be refused — the stale record keeps it
/// admitted, and the differential sees the missing refusal.
#[test]
fn the_differential_sees_an_r11_record_served_past_a_moved_drop_digest() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let divergences = replay_with_plant(Plant::R11RecordUnguarded);
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("door B's digest")),
        "the unguarded-record plant must turn the Drop impl's edit red; it found: \
         {divergences:#?}"
    );
}

/// S3a's plant (M110, Order 50): the recorded post-rewrite cold graph served
/// to an analysis that applied no rewrite — the log replayed over a tree that
/// was never edited. The browser effect package's stand-down edit (a context
/// read typed with no `run`) is where it is served, and the census says so.
///
/// What the differential itself sees of it: nothing, and the pin says that
/// too. Every consumer of a call edge the rewrite changes (`get()` → a local
/// read, `run(v, f)` → `f(v)`, `Context::new()` → a value) reads the TREE
/// beside the graph — `call_suspends` asks the call's subject, platform
/// colour walks a node's closures whether or not an edge names them — so a
/// stale edge over an unrewritten tree moves no verdict today, and the
/// program it can only arise in (a context error stands the rewrite down) is
/// never emitted. The guard stays for the consumer that would read an edge
/// alone; this pin keeps the plant's census visible until one appears.
#[test]
fn a_rewritten_graph_replayed_over_a_stood_down_rewrite_is_counted() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(Some(Plant::ContextLogUnguarded));
    let replayed = std::panic::catch_unwind(replay_the_classes);
    set_plant(None);
    let replayed = replayed.expect("the classes leg panicked under a plant");
    let stand_down = &replayed.censuses[replayed.censuses.len() - 2];
    assert!(
        stand_down.hot_world && stand_down.base_hits > 0 && stand_down.context_log_replayed == 1,
        "under the plant the stand-down edit is served the recorded post-rewrite graph: \
         {stand_down:#?}"
    );
    assert!(
        replayed.divergences.is_empty(),
        "a stale post-rewrite graph over an unrewritten tree moved a verdict — a consumer \
         now reads a call edge without the tree beside it; make the log guard's plant a \
         differential red: {:#?}",
        replayed.divergences
    );
}

/// S7's plant (M110, Order 51): an item's record served on its own text and
/// walk alone, the dirty closure never asked. A push into another module's
/// use-inferred binding typed with a different type (the M139 shape: the
/// pusher's constraint writes the owner item's slot) then leaves the owner
/// item's label at the previous keystroke's element type; a callee's changed
/// return leaves its callers' labels and checks — the gate must see it.
#[test]
fn the_differential_sees_an_item_record_served_over_a_dirty_dependency() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let divergences = replay_with_plant(Plant::ItemRecordUnguarded);
    assert!(
        divergences.iter().any(
            |divergence| divergence.contains("pushed into a module binding")
                || divergence.contains("push into another module's binding")
        ),
        "the unguarded-record plant must turn a push into another module's binding red; \
         it found: {divergences:#?}"
    );
}

/// S7b's plant (M142, Order 52): the Class A LIVE ranges sealed with the
/// served items' windows left live (`analyzer/live_ranges.rs`). A pass that
/// walks the live ranges then visits a served item's bodies and derives its
/// diagnostic a second time beside the replay of its record — a doubled
/// diagnostic, which a clean analysis reports once.
#[test]
fn the_differential_sees_a_pass_walking_a_served_window() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let divergences = replay_with_plant(Plant::LiveRangeWalksServed);
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("differs from the clean one")),
        "the served-window plant must turn a doubled diagnostic red; it found: {divergences:#?}"
    );
}

/// S1's plant: the hot modules' remembered checks replayed as if they were the
/// stored prefix's. A Class A refusal typed into a hot module is then served
/// stale — the gate must see it.
#[test]
fn the_differential_sees_a_hot_module_replayed_from_a_record() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let divergences = replay_with_plant(Plant::HotSetReplay);
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("Class A refusal")),
        "the hot-set replay plant must turn the Class A refusal edit red; it found: {divergences:#?}"
    );
}

/// S1's plant: the stored prefix served without its content check. A prefix
/// module edited under a seed elsewhere is then served stale.
#[test]
fn the_differential_sees_a_prefix_served_unvalidated() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let divergences = replay_with_plant(Plant::PrefixUnvalidated);
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("PREFIX module the leaf imports")),
        "the unvalidated-prefix plant must turn the prefix edit red; it found: {divergences:#?}"
    );
}

/// S1's plant: the impl guard dropped. A hot module's inherent impl on a
/// prefix type is then invisible to the prefix caller that uses it.
#[test]
fn the_differential_sees_a_hot_impl_the_prefix_needed() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let divergences = replay_with_plant(Plant::ImplGuardOff);
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("inherent impl the prefix calls")),
        "the impl-guard plant must turn the hot impl edit red; it found: {divergences:#?}"
    );
}

/// S1's plant: the use-inferred-binding guard dropped. A hot module that
/// decides a prefix binding's type by its first use — a push into a module's
/// empty list — is then walked after the prefix decided it, where the
/// canonical world walks it first. (A context's `run` no longer is such a
/// hazard: since B584 a call on a context's open value slot binds to the slot
/// itself, so the prefix's calls take whatever `run` fills it, in either walk
/// order — the context edit agrees under the plant, and the guard's context
/// arm is incr's to retire.)
#[test]
fn the_differential_sees_a_binding_the_hot_set_should_have_decided() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let divergences = replay_with_plant(Plant::UseInferredGuardOff);
    assert!(
        !divergences
            .iter()
            .any(|divergence| divergence.contains("grounds a prefix context")),
        "B584: the context edit is order-free, even with the guard off: {divergences:#?}"
    );
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("from the module loaded first")),
        "the use-inferred plant must turn the module-binding push red; it found: {divergences:#?}"
    );
}

/// B573: a keystroke in a twinned module of a BROWSER package, served from the
/// stored world (a hot-set hit), walks the module's twins under the browser —
/// the stored world carries the platform it was built for. Before B573 the
/// canonical and the hot-set world chose the `@process` twin alike, so the
/// differential agreed with itself; this asserts the twin.
#[test]
fn a_hot_twin_module_keeps_its_platforms_twin() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    vilan_core::analyzer::base_cache_clear();
    let package = platform_package();
    let seed = vec![package.path("twin.vl")];
    let _ = observe(&package, seed.clone(), Leg::Incremental);
    let warm = observe(&package, seed, Leg::Incremental);
    package.remove();
    vilan_core::analyzer::base_cache_clear();
    assert!(
        warm.census.hot_world && warm.census.base_hits == 1,
        "the twin module's keystroke is a hot-set hit: {:?}",
        warm.census
    );
    let javascript = warm.javascript.expect("the browser package emits");
    assert!(
        javascript.contains("browser twin") && !javascript.contains("process twin"),
        "the browser world holds the browser twin:\n{javascript}"
    );
}

/// S4's plant: a remembered const site served without asking its project reads
/// again. The input-file edit is then served stale.
#[test]
fn the_differential_sees_a_const_site_served_without_its_reads() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    vilan_core::const_cache::clear();
    let divergences = replay_with_plant(Plant::ConstCacheUnvalidated);
    vilan_core::const_cache::clear();
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("input file a const site reads")),
        "the unvalidated-const plant must turn the input-file edit red; it found: {divergences:#?}"
    );
}

/// S4's plant: a const site keyed without the world declarations it reaches. A
/// callee edited — in the package, and in std — is then served stale.
#[test]
fn the_differential_sees_a_const_key_without_its_callees() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    vilan_core::const_cache::clear();
    let divergences = replay_with_plant(Plant::ConstKeyWithoutWorld);
    vilan_core::const_cache::clear();
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("const callee's value edited")),
        "the callee-blind key plant must turn the const callee edit red; it found: {divergences:#?}"
    );
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains(STD_EDIT_LABEL)),
        "the callee-blind key plant must turn the std edit red; it found: {divergences:#?}"
    );
}

/// S2a's plant: the bound audit skips a reused module's sites and records
/// nothing for them. A prefix module's bound refusal is then dropped on the
/// hit — the gate must see it.
#[test]
fn the_differential_sees_a_bound_audit_served_without_its_record() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let divergences = replay_with_plant(Plant::BoundAuditUnrecorded);
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("module's bound refusal")),
        "the unrecorded-audit plant must turn the bound package red; it found: {divergences:#?}"
    );
    // M150: the written applications' refusals ride the same record.
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("written-bound refusal")),
        "the unrecorded-audit plant must turn the written-bound package red; it found: \
         {divergences:#?}"
    );
}

/// S2a's second plant: a module's recorded audit replayed without asking
/// whether a LATE impl answers one of its questions. The impl a prefix
/// module's bound is satisfied through is moved to another type in a hot
/// module, and the record keeps the answer the world no longer gives. (S1's
/// impl guard is not what protects the record — the audit's questions are
/// asked after the store, so the record carries its own test — which is why
/// `ImplGuardOff` leaves this package green.)
#[test]
fn the_differential_sees_a_bound_record_kept_over_a_moved_impl() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let divergences = replay_with_plant(Plant::BoundRecordUnguarded);
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("module's bound is satisfied through, moved")),
        "the unguarded-record plant must turn the moved-impl edit red; it found: {divergences:#?}"
    );
    // M150: and at a written application, under the same guard.
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("written bound is satisfied through, moved")),
        "the unguarded-record plant must turn the written moved-impl edit red; it found: \
         {divergences:#?}"
    );
}

/// S2b's plant: the tail skips a reused module's label rendering and drops
/// the restored rows — the prefix's hover types and declaration labels are
/// then missing on the hit; the gate must see it.
#[test]
fn the_differential_sees_label_tables_served_without_their_rows() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let divergences = replay_with_plant(Plant::LabelTablesUnrecorded);
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("type ") || divergence.contains("declaration ")),
        "the unrecorded-labels plant must turn a hover label red; it found: {divergences:#?}"
    );
}

/// S2b's other seam: an analysis NO front end seeded (a plain base-cache hit
/// — the CLI's watch, an editor leg served without a seed) reuses modules
/// whose record was filed by an unseeded analysis and so carries no label
/// rows; the tail must render their labels, not skip them. The hit's
/// rendering is compared with the clean one in full (the hover types and
/// declaration labels included), where the fixup that filed rows only for a
/// seeded analysis left four of vilan-lsp's hovers empty.
#[test]
fn a_reused_module_keeps_its_labels_without_a_seed() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    vilan_core::analyzer::base_cache_clear();
    let package = leaf_package();
    let cold = observe(&package, Vec::new(), Leg::Incremental);
    let warm = observe(&package, Vec::new(), Leg::Incremental);
    let clean = observe(&package, Vec::new(), Leg::Clean);
    package.remove();
    vilan_core::analyzer::base_cache_clear();
    assert!(
        cold.census.base_misses == 1 && warm.census.base_hits == 1,
        "the second unseeded analysis is served the stored world: {:?} / {:?}",
        cold.census,
        warm.census
    );
    assert!(
        warm.census.records_replayed > 0,
        "the hit replays module records: {:?}",
        warm.census
    );
    assert!(
        warm.rendering == clean.rendering,
        "an unseeded hit renders every label a clean analysis renders: {}",
        first_difference(&warm.rendering, &clean.rendering)
    );
}

/// B569 (lang-a-49, at C3's request): a reused PREFIX module's labelled
/// tuples — the labels its hover rows and declaration labels print, which S2b
/// records per module and restores on reuse, and the label refusal the tuple
/// rule records during inference — render on every keystroke elsewhere, and
/// on an unseeded hit, exactly as a clean analysis renders them.
#[test]
fn b569_a_reused_modules_tuple_labels_render_as_a_clean_analysis_renders() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    vilan_core::analyzer::base_cache_clear();
    let mut divergences = Vec::new();
    let mut censuses = Vec::new();
    let mut package = LABELS_FIXTURE.write();
    for edit in LABELS_FIXTURE.edits {
        censuses.extend(replay(&mut package, edit, &mut divergences));
    }
    vilan_core::analyzer::base_cache_clear();
    let cold = observe(&package, Vec::new(), Leg::Incremental);
    let warm = observe(&package, Vec::new(), Leg::Incremental);
    let clean = observe(&package, Vec::new(), Leg::Clean);
    package.remove();
    vilan_core::analyzer::base_cache_clear();
    assert!(divergences.is_empty(), "{divergences:#?}");
    assert!(
        censuses.iter().any(|census| census.records_replayed > 0),
        "the keystrokes reuse the prefix module's records: {censuses:#?}"
    );
    assert!(
        cold.census.base_misses == 1 && warm.census.base_hits == 1,
        "the second unseeded analysis is served the stored world: {:?} / {:?}",
        cold.census,
        warm.census
    );
    assert!(
        warm.rendering == clean.rendering,
        "an unseeded hit renders every label a clean analysis renders: {}",
        first_difference(&warm.rendering, &clean.rendering)
    );
    for printed in [
        "(min: i32, max: i32)",
        "(x: f64, y: f64)",
        "`z` is not a label of `(x: i32, y: i32)`",
    ] {
        assert!(
            clean.rendering.contains(printed),
            "the fixture renders {printed:?} (or it says nothing about labels):\n{}",
            clean.rendering
        );
    }
}

/// M121 / B553's cost: a check whose late files write no impl on a type they
/// do not declare RECORDS NOTHING — the pre-entry resolve asks the impl table
/// thousands of questions and none of them is kept (`reach_questions` 0), which
/// is every `vilan check` of the gate's examples. An entry that does write one
/// (B553's package) records, or the zero says nothing.
#[test]
fn a_check_whose_late_files_write_no_foreign_impl_records_nothing() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    vilan_core::analyzer::base_cache_clear();
    let package = leaf_package();
    let check = observe(&package, Vec::new(), Leg::Incremental);
    package.remove();
    let package = Package::write(
        "entry_impl_census",
        Platform::default(),
        &[
            ("main.vl", ENTRY_IMPL_MAIN),
            ("shapes.vl", ENTRY_IMPL_SHAPES),
            ("waver.vl", ENTRY_IMPL_WAVER),
        ],
    );
    let foreign = observe(&package, Vec::new(), Leg::Incremental);
    package.remove();
    vilan_core::analyzer::base_cache_clear();
    assert!(
        check.census.base_misses == 1 && check.census.reach_questions == 0,
        "a check with no foreign impl in its entry records nothing: {:?}",
        check.census
    );
    assert!(
        foreign.census.reach_questions > 0,
        "an entry impl on a module's type is recorded against: {:?}",
        foreign.census
    );
}

// --- the re-walk counter pins (Q9) ------------------------------------------------

/// What a keystroke in `file` re-walks once the hot-set world is warm: the
/// census of the SECOND of two analyses seeded there (the first stores the
/// prefix).
fn rewalked_on_a_warm_keystroke(package: &mut Package, file: &str) -> Census {
    let original = package.text(file).to_string();
    let seed = vec![package.path(file)];
    package.set(file, format!("{original}\n"));
    let _ = observe(package, seed.clone(), Leg::Incremental);
    package.set(file, original);
    observe(package, seed, Leg::Incremental).census
}

/// The re-walk counts the paper's §4.4 names, pinned on counts rather than
/// clocks (Q9): a leaf importer's keystroke re-walks the leaf and the entry; a
/// cycle member's re-walks the cycle and the entry; the entry's own re-walks
/// the entry alone — each served from the base cache. Run twice: as built,
/// and with the hot set planted as the WHOLE package, which must move every
/// count (the pins' non-vacuity).
#[test]
fn a_keystroke_rewalks_its_hot_set_and_nothing_else() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let measure = || {
        vilan_core::analyzer::base_cache_clear();
        let mut package = leaf_package();
        let leaf = rewalked_on_a_warm_keystroke(&mut package, "views.vl");
        let cycle = rewalked_on_a_warm_keystroke(&mut package, "cycle_b.vl");
        let entry = rewalked_on_a_warm_keystroke(&mut package, "main.vl");
        let hub = rewalked_on_a_warm_keystroke(&mut package, "hub.vl");
        package.remove();
        vilan_core::analyzer::base_cache_clear();
        (leaf, cycle, entry, hub)
    };
    set_plant(None);
    let (leaf, cycle, entry, hub) = measure();
    assert!(
        leaf.hot_world && leaf.base_hits == 1,
        "a leaf keystroke is a hot-set hit: {leaf:?}"
    );
    assert_eq!(
        (leaf.hot_modules, leaf.sources_walked),
        (2, 2),
        "a keystroke in a leaf importer re-walks the leaf and the entry: {leaf:?}"
    );
    // M110 S6: and lays out ONE module's windows beside the entry's — one
    // window per top-level item of the two re-walked files — inheriting the
    // stored world's for every other item.
    let items = |text: &str| -> u64 {
        let (tree, errors) = vilan_core::parsing::parse(text);
        assert!(errors.is_empty(), "the fixture parses");
        tree.map_or(0, |tree| tree.0.len() as u64)
    };
    assert_eq!(
        leaf.windows_laid_out,
        items(PINS_VIEWS) + items(PINS_MAIN),
        "a body edit lays out the leaf's and the entry's windows and no other: {leaf:?}"
    );
    assert!(
        leaf.windows_inherited > leaf.windows_laid_out,
        "the prefix's windows are the stored world's: {leaf:?}"
    );
    // The standing counter pins on a package with no cross-module binding
    // write: no constraint of one item rewrote a slot minted for another,
    // and no relocated mint left its anchor's window — on every keystroke.
    for (name, census) in [
        ("leaf", &leaf),
        ("cycle", &cycle),
        ("entry", &entry),
        ("hub", &hub),
    ] {
        assert_eq!(
            (census.window_writes_other, census.window_outside_anchor),
            (0, 0),
            "the {name} keystroke: a constraint rewrote a slot minted for another item, or a \
             mint left its anchor's window: {census:?}"
        );
    }
    assert!(
        cycle.hot_world && cycle.base_hits == 1,
        "a cycle keystroke is a hot-set hit: {cycle:?}"
    );
    assert_eq!(
        (cycle.hot_modules, cycle.sources_walked),
        (3, 3),
        "a keystroke in a cycle member re-walks the cycle and the entry: {cycle:?}"
    );
    assert!(
        !entry.hot_world && entry.base_hits == 1,
        "the entry's keystroke hits the ordinary world: {entry:?}"
    );
    assert_eq!(
        entry.sources_walked, 1,
        "the entry's keystroke re-walks the entry: {entry:?}"
    );
    // `hub` is imported by `views` and by the entry: its hot set is all three.
    assert_eq!(
        (hub.hot_modules, hub.sources_walked),
        (3, 3),
        "a shared module's keystroke re-walks it and its importers: {hub:?}"
    );
    assert!(
        leaf.package_modules == 5 && leaf.records_replayed > 0,
        "the census reads the whole package and the replayed prefix: {leaf:?}"
    );

    set_plant(Some(Plant::WholePackageHot));
    let (leaf, cycle, _, hub) = measure();
    set_plant(None);
    assert!(
        leaf.sources_walked > 2 && cycle.sources_walked > 3 && hub.sources_walked > 3,
        "with the hot set planted as the whole package every count must move, or the \
         pins above are vacuous: leaf {leaf:?}, cycle {cycle:?}, hub {hub:?}"
    );
}

/// **M152's gate** (std-prefix door (b)): a program with DEPENDENCIES — the
/// hand-written two-package fixture and `examples/fullstack`'s two members —
/// in every state of its script, applied and undone, answers what the
/// canonical clean analysis answers and emits what a clean analysis of the
/// same shape emits, three ways: S1's hot sets alone (the default), the std
/// prefix FORCED ON — the world stored is std plus the dependency tier, and
/// only the package is hot — and the prefix LAYERED under the world build.
/// Non-vacuous: every analysis of the forced leg is a prefix world (the
/// `dependencies` refusal is retired), the prefix is SERVED to the next
/// state, and nothing refuses it.
#[test]
fn m152_programs_with_dependencies_answer_what_a_clean_analysis_answers() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_plant(None);
    let mut divergences = Vec::new();
    let mut forced = Vec::new();
    let mut layered = Vec::new();
    for mode in ["hot sets", "forced prefix", "layered prefix"] {
        vilan_core::analyzer::base_cache_clear();
        match mode {
            "forced prefix" => vilan_core::incremental::set_std_prefix(Some(true)),
            "layered prefix" => vilan_core::incremental::set_std_prefix_layering(Some(true)),
            _ => {}
        }
        for fixture in DEPENDENCY_FIXTURES {
            let mut package = fixture.write();
            let seed = package.path(fixture.edits[0].file);
            let first = observe(&package, vec![seed], Leg::Incremental);
            let clean = observe(&package, Vec::new(), Leg::Clean);
            if first.rendering != clean.rendering {
                divergences.push(format!(
                    "{} ({mode}): the first analysis differs from the clean one at {}",
                    fixture.name,
                    first_difference(&first.rendering, &clean.rendering)
                ));
            }
            let mut step = Vec::new();
            for edit in fixture.edits {
                let mut found = Vec::new();
                step.extend(replay(&mut package, edit, &mut found));
                divergences.extend(found.into_iter().map(|found| format!("({mode}) {found}")));
            }
            match mode {
                "forced prefix" => {
                    forced.push(first.census);
                    forced.extend(step);
                }
                "layered prefix" => layered.extend(step),
                _ => {}
            }
            package.remove();
        }
        vilan_core::incremental::set_std_prefix(None);
        vilan_core::incremental::set_std_prefix_layering(None);
    }
    vilan_core::analyzer::base_cache_clear();
    assert!(
        divergences.is_empty(),
        "{} step(s) of the dependency scripts observe the reuse:\n{}",
        divergences.len(),
        divergences.join("\n")
    );
    let refusals: Vec<&'static str> = forced
        .iter()
        .filter_map(|census| census.prefix_refusal)
        .collect();
    assert!(
        refusals.is_empty() && forced.iter().all(|census| census.prefix_world),
        "every forced analysis of a program with dependencies is a prefix world, refusing \
         nothing: refusals {refusals:?}, {} of {} prefix worlds",
        forced.iter().filter(|census| census.prefix_world).count(),
        forced.len()
    );
    let prefix_hits: u64 = forced.iter().map(|census| census.prefix_hits).sum();
    assert!(
        prefix_hits >= 4,
        "the std-and-dependencies prefix must be served to later states ({prefix_hits} hits)"
    );
    let layered_builds = layered
        .iter()
        .filter(|census| census.prefix_layered)
        .count();
    assert!(
        layered_builds >= 1,
        "the layered leg must build a world over the prefix ({layered_builds} did)"
    );
}
