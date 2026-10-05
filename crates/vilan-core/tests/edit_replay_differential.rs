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
//! importer and a bystander. From S1 on, each slice's planted bug must turn
//! the classes leg red — the M57 lesson.
//!
//! The overlay is process-global, so the tests serialize on
//! [`SWITCH_LOCK`] (plain `cargo test` runs them as threads of one process).

mod replay_harness;
mod scratch;

use std::path::PathBuf;
use std::sync::Mutex;

use replay_harness::std_spec;
use vilan_core::incremental::{Census, clean_analysis, render_observation};
use vilan_core::{BuildOptions, Platform, Workspace, analyze_source, transform};

static SWITCH_LOCK: Mutex<()> = Mutex::new(());

/// One package on disk, its open buffers in the document overlay.
struct Package {
    directory: PathBuf,
    entry: PathBuf,
    platform: Platform,
    /// The text each file holds NOW (the overlay's, once edited).
    texts: Vec<(String, String)>,
}

impl Package {
    fn write(name: &str, platform: Platform, files: &[(&str, &str)]) -> Package {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let directory =
            scratch::root().join(format!("vilan_m110_{name}_{}_{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("create the package directory");
        for (file, text) in files {
            std::fs::write(directory.join(file), text).expect("write a package file");
        }
        Package {
            entry: directory.join("main.vl"),
            directory,
            platform,
            texts: files
                .iter()
                .map(|(file, text)| (file.to_string(), text.to_string()))
                .collect(),
        }
    }

    fn path(&self, file: &str) -> PathBuf {
        self.directory.join(file)
    }

    fn text(&self, file: &str) -> &str {
        &self
            .texts
            .iter()
            .find(|(name, _)| name == file)
            .unwrap_or_else(|| panic!("no file {file} in the package"))
            .1
    }

    /// The editor's keystroke: the buffer moves, through the overlay every
    /// load and every base-cache validation reads.
    fn set(&mut self, file: &str, text: String) {
        vilan_core::analyzer::set_document_overlay(&self.path(file), Some(text.clone()));
        for (name, current) in &mut self.texts {
            if name == file {
                *current = text;
                return;
            }
        }
        panic!("no file {file} in the package");
    }

    fn remove(self) {
        for (file, _) in &self.texts {
            vilan_core::analyzer::set_document_overlay(&self.path(file), None);
        }
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// What one analysis showed, and what it did to get there.
struct Observation {
    rendering: String,
    javascript: Option<String>,
    census: Census,
}

/// Which analysis [`observe`] runs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Leg {
    /// The incremental one, every reuse on, the seeds as the edited files.
    Incremental,
    /// The canonical clean one: nothing reused, no hot set — what everything
    /// an editor reads is compared against.
    Clean,
}

/// One analysis of `package`'s entry, on a big-stack worker like every real
/// pipeline.
fn observe(package: &Package, seeds: Vec<PathBuf>, leg: Leg) -> Observation {
    let entry_text = package.text("main.vl").to_string();
    let pkg_root = package.directory.clone();
    let entry = package.entry.clone();
    let platform = package.platform;
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(entry_text.into_boxed_str());
            let workspace = Workspace {
                hot_seeds: seeds,
                ..Workspace::default()
            };
            let analyze = || {
                analyze_source(
                    leaked,
                    &std_spec(),
                    &pkg_root,
                    &entry,
                    Some(platform),
                    &workspace,
                )
            };
            let (program, errors) = match leg {
                Leg::Incremental => analyze(),
                Leg::Clean => clean_analysis(analyze),
            };
            let census = vilan_core::incremental::census();
            let rendering = program
                .as_ref()
                .map(|program| render_observation(program, &errors))
                .unwrap_or_else(|| format!("no program: {errors:?}"));
            let javascript = match program {
                Some(program) if errors.is_empty() => {
                    transform(&program, &BuildOptions::default()).ok()
                }
                _ => None,
            };
            Observation {
                rendering,
                javascript,
                census,
            }
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked")
}

/// The first row where two renderings part, for the failure message.
fn first_difference(incremental: &str, clean: &str) -> String {
    let left: Vec<&str> = incremental.lines().collect();
    let right: Vec<&str> = clean.lines().collect();
    let at = left
        .iter()
        .zip(&right)
        .position(|(a, b)| a != b)
        .unwrap_or(left.len().min(right.len()));
    format!(
        "row {at}:\n    incremental: {:?}\n    clean:       {:?}\n    ({} rows against {})",
        left.get(at),
        right.get(at),
        left.len(),
        right.len()
    )
}

/// One edit: replacements in one file (applied in order), the file the editor
/// is typing into (usually that file — `seed` overrides it for the step where
/// a PREFIX module moves under a seed elsewhere), and a name for the report.
struct Edit {
    label: &'static str,
    file: &'static str,
    seed: Option<&'static str>,
    replacements: &'static [(&'static str, &'static str)],
}

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
        if incremental.rendering != clean.rendering {
            divergences.push(format!(
                "{} ({phase}): the incremental analysis differs from the clean one at {}",
                edit.label,
                first_difference(&incremental.rendering, &clean.rendering)
            ));
        }
        if incremental.javascript != clean.javascript {
            divergences.push(format!(
                "{} ({phase}): the emitted JS differs (incremental {} bytes, clean {} bytes)",
                edit.label,
                incremental.javascript.as_ref().map_or(0, String::len),
                clean.javascript.as_ref().map_or(0, String::len),
            ));
        }
        censuses.push(incremental.census);
    }
    censuses
}

// --- the §6 edit classes ------------------------------------------------------

const CLASSES_MAIN: &str = "import pkg::views::render;\nimport pkg::cycle_a::ring;\nimport pkg::impls;\nimport pkg::user::use_it;\nimport pkg::side::side_value;\nimport pkg::ext;\nimport pkg::reader::shouted;\nimport pkg::runner::run_read;\nimport pkg::bag::fill;\nimport pkg::a_spoil::spoil;\n\nfun main() {\n\tprint(render());\n\tprint(ring(1));\n\tprint(use_it());\n\tprint(side_value());\n\tprint(shouted());\n\tprint(run_read());\n\tfill();\n\tspoil();\n}\n";

// S1's hazards, each a module the PREFIX holds and a module the hot set holds:
// an inherent impl on a prefix type, written in a module the caller never
// imports (`ext` / `reader`, the impl guard's case); a context whose value type
// only a hot module's `run` grounds (`ctx` / `runner`, the early-commitment
// guard's case); and a module binding two modules push into, the hot one
// first in load order (`bag` / `a_spoil`, which side of a conflict is blamed).
const CLASSES_EXT: &str =
    "import pkg::shapes::Foo;\n\nimpl Foo {\n\tfun shout(self): str {\n\t\t\"HI\"\n\t}\n}\n";

const CLASSES_READER: &str = "import pkg::shapes::Foo;\n\nexport fun shouted(): str {\n\tlet foo = Foo { n = 2 };\n\tfoo.shout()\n}\n";

const CLASSES_CTX: &str = "import std::context::Context;\n\nexport let mood = Context::new();\n\nexport fun read_mood(): i32 {\n\tmood.get() + 1\n}\n";

const CLASSES_RUNNER: &str = "import pkg::ctx::{ mood, read_mood };\n\nexport fun run_read(): i32 {\n\tmut seen = 0;\n\tmood.run(5, || {\n\t\tseen = read_mood();\n\t});\n\tseen\n}\n";

const CLASSES_BAG: &str = "export mut bag = [];\n\nexport fun fill() {\n\tbag.push(1);\n}\n";

const CLASSES_SPOIL: &str = "import pkg::bag::bag;\n\nexport fun spoil() {\n\tbag.push(2);\n}\n";

const CLASSES_MODEL: &str = "import std::context::Context;\n\nexport struct Item {\n\tname: str,\n\tcount: i32,\n}\n\nexport let flavor: Context<i32> = Context::new();\n\nmut items = [];\n\nexport fun add() {\n\titems.push(1);\n}\n\nexport fun show(): i32 {\n\titems[0] + 1\n}\n\nexport fun make() {\n\t1\n}\n\nexport fun leaf(): i32 {\n\t1\n}\n\nexport fun size(): i32 {\n\t4\n}\n\nexport fun first_item(): Item {\n\tItem { name = \"a\", count = 1 }\n}\n";

const CLASSES_VIEWS: &str = "import pkg::model::{ add, first_item, leaf, make, show, size };\n\nexport fun render(): i32 {\n\tadd();\n\tlet made: i32 = make();\n\tlet item = first_item();\n\tlet table = const size() * 2;\n\tmade + show() + leaf() + item.count + table\n}\n";

const CLASSES_CYCLE_A: &str = "import pkg::cycle_b::bounce;\nimport pkg::model::leaf;\n\nexport fun ring(n: i32): i32 {\n\tif n > 3 {\n\t\tn\n\t} else {\n\t\tbounce(n + 1) + leaf()\n\t}\n}\n";

const CLASSES_CYCLE_B: &str =
    "import pkg::cycle_a::ring;\n\nexport fun bounce(n: i32): i32 {\n\tring(n + 1)\n}\n";

const CLASSES_SHAPES: &str =
    "export struct Foo {\n\tn: i32,\n}\n\nexport trait Greet {\n\tfun greet(self): str;\n}\n";

const CLASSES_IMPLS: &str = "import pkg::shapes::{ Foo, Greet };\n\nexport impl Foo with Greet {\n\tfun greet(self): str {\n\t\t\"hi\"\n\t}\n}\n";

const CLASSES_USER: &str = "import pkg::shapes::{ Foo, Greet };\n\nexport fun use_it(): str {\n\tlet foo = Foo { n = 1 };\n\tfoo.greet()\n}\n";

const CLASSES_SIDE: &str = "export fun side_value(): i32 {\n\t2\n}\n";

fn classes_package() -> Package {
    Package::write(
        "classes",
        Platform::default(),
        &[
            ("main.vl", CLASSES_MAIN),
            ("model.vl", CLASSES_MODEL),
            ("views.vl", CLASSES_VIEWS),
            ("cycle_a.vl", CLASSES_CYCLE_A),
            ("cycle_b.vl", CLASSES_CYCLE_B),
            ("shapes.vl", CLASSES_SHAPES),
            ("impls.vl", CLASSES_IMPLS),
            ("user.vl", CLASSES_USER),
            ("side.vl", CLASSES_SIDE),
            ("ext.vl", CLASSES_EXT),
            ("reader.vl", CLASSES_READER),
            ("ctx.vl", CLASSES_CTX),
            ("runner.vl", CLASSES_RUNNER),
            ("bag.vl", CLASSES_BAG),
            ("a_spoil.vl", CLASSES_SPOIL),
        ],
    )
}

/// §6's list, one edit per class, each applied and undone.
const CLASS_EDITS: &[Edit] = &[
    Edit {
        label: "a body literal's type flips an inferred return (i1)",
        file: "model.vl",
        seed: None,
        replacements: &[(
            "export fun make() {\n\t1\n}",
            "export fun make() {\n\t\"one\"\n}",
        )],
    },
    Edit {
        label: "a context read added to a leaf (i2)",
        file: "model.vl",
        seed: None,
        replacements: &[(
            "export fun leaf(): i32 {\n\t1\n}",
            "export fun leaf(): i32 {\n\tflavor.get()\n}",
        )],
    },
    Edit {
        label: "a sleep makes a leaf and its callers async (i6), with its import",
        file: "model.vl",
        seed: None,
        replacements: &[
            (
                "import std::context::Context;\n",
                "import std::context::Context;\nimport std::time;\n",
            ),
            (
                "export fun leaf(): i32 {\n\t1\n}",
                "export fun leaf(): i32 {\n\ttime::sleep(1);\n\t1\n}",
            ),
        ],
    },
    Edit {
        label: "a different type pushed into a module binding (i7)",
        file: "model.vl",
        seed: None,
        replacements: &[("items.push(1);", "items.push(\"one\");")],
    },
    Edit {
        label: "a written return type changed",
        file: "model.vl",
        seed: None,
        replacements: &[("export fun leaf(): i32 {", "export fun leaf(): i53 {")],
    },
    Edit {
        label: "a field renamed",
        file: "model.vl",
        seed: None,
        replacements: &[("\tcount: i32,", "\ttotal: i32,")],
    },
    Edit {
        label: "an impl in a module the caller never imports, removed (i3)",
        file: "impls.vl",
        seed: None,
        replacements: &[(
            "export impl Foo with Greet {\n\tfun greet(self): str {\n\t\t\"hi\"\n\t}\n}\n",
            "",
        )],
    },
    Edit {
        label: "a derive flipped on",
        file: "shapes.vl",
        seed: None,
        replacements: &[(
            "export struct Foo {",
            "[derive(PartialEq)]\nexport struct Foo {",
        )],
    },
    Edit {
        label: "an import added",
        file: "views.vl",
        seed: None,
        replacements: &[(
            "import pkg::model::{",
            "import pkg::side::side_value;\nimport pkg::model::{",
        )],
    },
    Edit {
        label: "a const callee's body edited (i4)",
        file: "model.vl",
        seed: None,
        replacements: &[(
            "export fun size(): i32 {\n\t4\n}",
            "export fun size(): i32 {\n\tlet list: List<i32> = [];\n\tlist[3]\n}",
        )],
    },
    Edit {
        label: "a body edit inside an import cycle",
        file: "cycle_b.vl",
        seed: None,
        replacements: &[("\tring(n + 1)", "\tring(n + 2)")],
    },
    Edit {
        label: "an inherent impl the prefix calls, edited in a module it never imports",
        file: "ext.vl",
        seed: None,
        replacements: &[("\t\t\"HI\"", "\t\t\"HEY\"")],
    },
    Edit {
        label: "the hot module that grounds a prefix context's value type",
        file: "runner.vl",
        seed: None,
        replacements: &[("\tseen\n}", "\tseen + 0\n}")],
    },
    Edit {
        label: "a push into another module's binding, from the module loaded first",
        file: "a_spoil.vl",
        seed: None,
        replacements: &[("bag.push(2);", "bag.push(\"two\");")],
    },
    Edit {
        label: "a statement typed into a leaf importer's body",
        file: "views.vl",
        seed: None,
        replacements: &[("\tadd();\n", "\tadd();\n\tlet extra = 1;\n")],
    },
    Edit {
        label: "a PREFIX module changes while the seed stays on another file",
        file: "side.vl",
        seed: Some("views.vl"),
        replacements: &[("\t2\n", "\t3\n")],
    },
    Edit {
        label: "a module binding's push typed in the entry's hot set's neighbour",
        file: "model.vl",
        seed: None,
        replacements: &[("\titems.push(1);\n", "\titems.push(1);\n\titems.push(2);\n")],
    },
];

const PLATFORM_MAIN: &str = "import pkg::shared::label;\n\nfun main() {\n\tprint(label());\n}\n";

const PLATFORM_SHARED: &str = "export fun label(): str {\n\t\"x\"\n}\n";

const PLATFORM_EDITS: &[Edit] = &[Edit {
    label: "a browser entry reaches a node-only call (i5)",
    file: "shared.vl",
    seed: None,
    replacements: &[(
        "export fun label(): str {\n\t\"x\"\n}\n",
        "import std::process;\n\nexport fun label(): str {\n\ti\"x{process::args().len()}\"\n}\n",
    )],
}];

/// Runs every class edit over a fresh package, then the platform edit over its
/// own, answering the divergences and every incremental census.
fn replay_the_classes() -> (Vec<String>, Vec<Census>) {
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
    package.remove();
    let mut package = Package::write(
        "platform",
        Platform::Browser,
        &[("main.vl", PLATFORM_MAIN), ("shared.vl", PLATFORM_SHARED)],
    );
    for edit in PLATFORM_EDITS {
        censuses.extend(replay(&mut package, edit, &mut divergences));
    }
    package.remove();
    vilan_core::analyzer::base_cache_clear();
    (divergences, censuses)
}

/// **The gate.** Every §6 edit class, applied and undone, answers what a clean
/// analysis answers — byte for byte over everything an editor reads, and the
/// emitted JS wherever the program emits.
#[test]
fn every_edit_class_answers_what_a_clean_analysis_answers() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (divergences, censuses) = replay_the_classes();
    assert!(
        divergences.is_empty(),
        "{} step(s) of the edit script observe incremental analysis:\n{}",
        divergences.len(),
        divergences.join("\n")
    );
    // The harness green on ITSELF is S0's gate (`incremental-analysis.md`
    // §11): before S1 no edit outside the entry reuses anything, so the
    // incremental leg is the base cache's miss-and-store path against a clean
    // analysis. S1 raises this floor to the hot-set world it builds.
    assert!(
        censuses
            .iter()
            .all(|census| census.base_misses + census.base_hits == 1),
        "every incremental analysis consults the base cache exactly once"
    );
}

// --- the corpus ----------------------------------------------------------------

/// The Class A refusal every corpus module gets, so each has a diagnostic of
/// its own to remember (M57's lesson: sixty clean modules agree under any
/// reuse at all).
const PROBE_REFUSAL: &str =
    "\nfun m110_probe_refusal(): i32 {\n\tlet total = 1;\n\ttotal = 2;\n\ttotal\n}\n";

const CORPUS_MAIN: &str = "import pkg::module;\nimport pkg::user;\nimport pkg::side;\n\nfun main() {\n\tprint(user::user_value());\n\tprint(side::side_value());\n}\n";

const CORPUS_USER: &str = "import pkg::module;\n\nexport fun user_value(): i32 {\n\t1\n}\n";

const CORPUS_SIDE: &str = "export fun side_value(): i32 {\n\t2\n}\n";

const CORPUS_EDITS: &[Edit] = &[
    Edit {
        label: "a function typed into the module",
        file: "module.vl",
        seed: None,
        replacements: &[(
            "\nfun m110_probe_refusal(): i32 {",
            "\nfun m110_probe_added(): i32 {\n\tlet total = 1;\n\ttotal = 3;\n\ttotal\n}\n\nfun m110_probe_refusal(): i32 {",
        )],
    },
    Edit {
        label: "the bystander changes under the module's seed",
        file: "side.vl",
        seed: Some("module.vl"),
        replacements: &[("\t2\n", "\t3\n")],
    },
];

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
            .chunks(paths.len().div_ceil(16).max(1))
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
}
