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

use std::path::PathBuf;
use std::sync::Mutex;

use replay_harness::std_spec;
use vilan_core::incremental::{
    Census, Plant, clean_analysis, clean_analysis_keeping_hot_shape, render_observation, set_plant,
};
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
    /// Clean, but built in the hot-set shape the seeds ask for — what the
    /// emitted JS is compared against (a hot-set world numbers its hot
    /// modules' declarations after the prefix's, so its JS orders them
    /// differently from the canonical world's; a hot-set program is never
    /// emitted, and its JS is still the sharpest check that nothing reused was
    /// stale).
    CleanSameShape,
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
                Leg::CleanSameShape => clean_analysis_keeping_hot_shape(analyze),
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

// --- the §6 edit classes ------------------------------------------------------

const CLASSES_MAIN: &str = "import pkg::views::render;\nimport pkg::cycle_a::ring;\nimport pkg::impls;\nimport pkg::user::use_it;\nimport pkg::side::side_value;\nimport pkg::ext;\nimport pkg::reader::shouted;\nimport pkg::runner::run_read;\nimport pkg::bag::fill;\nimport pkg::a_spoil::spoil;\nimport pkg::painter::painted;\nimport pkg::shiner::shone;\nimport pkg::bar_eq;\nimport pkg::bar_user::bars_match;\nimport pkg::data_user::{ banner, smaller };\n\nfun main() {\n\tprint(render());\n\tprint(ring(1));\n\tprint(use_it());\n\tprint(side_value());\n\tprint(shouted());\n\tprint(run_read());\n\tfill();\n\tspoil();\n\tprint(painted());\n\tprint(shone());\n\tprint(bars_match());\n\tprint(banner());\n\tprint(smaller());\n}\n";

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

const CLASSES_SHAPES: &str = "export struct Foo {\n\tn: i32,\n}\n\nexport trait Greet {\n\tfun greet(self): str;\n}\n\nexport struct Bar {\n\tn: i32,\n}\n\nexport trait Shine {\n\tfun shine(self): i32;\n}\n";

// M121's hot impls on PREFIX types, as kolt writes them: an inherent impl
// whose members' returns are inferred (`styles.vl`'s `impl style::Style`), and
// a trait impl on a type another module declares (`prefs.vl`'s `impl HashMap
// with Json`) — each called only from a module that imports it, so no
// question the stored prefix asked reaches them and the hot-set world serves
// them. And one the prefix DOES reach, through an operator: `bar_user` (a
// prefix module) compares two `Bar`s, and `bar_eq` (hot) is their `PartialEq`.
const CLASSES_PAINT: &str =
    "import pkg::shapes::Foo;\n\nimpl Foo {\n\tfun paint(self) {\n\t\t\"p\"\n\t}\n}\n";

const CLASSES_PAINTER: &str = "import pkg::paint;\nimport pkg::shapes::Foo;\n\nexport fun painted(): str {\n\tlet foo = Foo { n = 4 };\n\tfoo.paint()\n}\n";

const CLASSES_SHINE: &str = "import pkg::shapes::{ Foo, Shine };\n\nimpl Foo with Shine {\n\tfun shine(self): i32 {\n\t\tself.n\n\t}\n}\n";

const CLASSES_SHINER: &str = "import pkg::shine;\nimport pkg::shapes::{ Foo, Shine };\n\nexport fun shone(): i32 {\n\tlet foo = Foo { n = 5 };\n\tfoo.shine()\n}\n";

const CLASSES_BAR_EQ: &str = "import std::compare::PartialEq;\nimport pkg::shapes::Bar;\n\nimpl Bar with PartialEq {\n\tfun eq(self, other: Self): bool {\n\t\tself.n == other.n\n\t}\n}\n";

// M110 S4's const sites over the project and std: one reads an input file the
// edit script moves, one calls a std function the script edits in std itself.
const CLASSES_DATA_USER: &str = "import std::math;\nimport std::web::asset;\n\nexport fun banner(): str {\n\tconst asset::read(\"banner.txt\")\n}\n\nexport fun smaller(): i32 {\n\tconst math::minmax(2, 1).0\n}\n";

const CLASSES_BANNER: &str = "hello\n";

const CLASSES_BAR_USER: &str = "import pkg::shapes::Bar;\n\nexport fun bars_match(): bool {\n\tBar { n = 1 } == Bar { n = 1 }\n}\n";

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
            ("paint.vl", CLASSES_PAINT),
            ("painter.vl", CLASSES_PAINTER),
            ("shine.vl", CLASSES_SHINE),
            ("shiner.vl", CLASSES_SHINER),
            ("bar_eq.vl", CLASSES_BAR_EQ),
            ("bar_user.vl", CLASSES_BAR_USER),
            ("data_user.vl", CLASSES_DATA_USER),
            ("banner.txt", CLASSES_BANNER),
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
        label: "a const callee's value edited, the site's own text unchanged (S4)",
        file: "model.vl",
        seed: None,
        replacements: &[(
            "export fun size(): i32 {\n\t4\n}",
            "export fun size(): i32 {\n\t5\n}",
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
        label: "a hot trait impl a prefix module's operator dispatches through",
        file: "bar_eq.vl",
        seed: None,
        replacements: &[("\t\tself.n == other.n", "\t\tself.n + 0 == other.n")],
    },
    Edit {
        label: "an input file a const site reads, edited (S4)",
        file: "banner.txt",
        seed: Some("data_user.vl"),
        replacements: &[("hello", "HELLO")],
    },
    Edit {
        label: "a module binding's push typed in the entry's hot set's neighbour",
        file: "model.vl",
        seed: None,
        replacements: &[("\titems.push(1);\n", "\titems.push(1);\n\titems.push(2);\n")],
    },
];

/// S4's std bump: std's own `math::minmax` edited under the analysis (an
/// edited std buffer, through the overlay every load reads), with the
/// classes package's const site that calls it. A const result remembered from
/// before the edit is the stale answer the const cache must not serve.
const STD_EDIT_FILE: &str = "src/math.vl";
const STD_EDIT_LABEL: &str = "a std function a const site calls, edited (S4's std bump)";
const STD_EDIT: (&str, &str) = (
    "\tif a <= b {\n\t\t(a, b)\n\t} else {\n\t\t(b, a)\n\t}",
    "\tif a <= b {\n\t\t(b, a)\n\t} else {\n\t\t(a, b)\n\t}",
);

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

/// M121's acceptance shape, over the classes package: a keystroke in a module
/// whose hot set writes impls on PREFIX types that nothing in the prefix asks
/// about. S1's spelling guard refused these (kolt's `theme.vl`, `model.vl`
/// and `styles.vl` every keystroke); the reach probe serves them, and each
/// must be a hot-set world as well as agree with a clean analysis.
const SERVED_IMPL_EDITS: &[Edit] = &[
    Edit {
        label: "a hot inherent impl on a prefix type, its return inferred, that no prefix module calls",
        file: "paint.vl",
        seed: None,
        replacements: &[("\t\t\"p\"", "\t\t\"pp\"")],
    },
    Edit {
        label: "a hot trait impl on a prefix type that no prefix module asks for",
        file: "shine.vl",
        seed: None,
        replacements: &[("\t\tself.n\n", "\t\tself.n + 1\n")],
    },
    Edit {
        label: "a statement typed into the module that calls a hot impl",
        file: "painter.vl",
        seed: None,
        replacements: &[("\tfoo.paint()\n", "\tlet extra = 1;\n\tfoo.paint()\n")],
    },
];

/// B553's package: the ENTRY writes the impl a module calls. Every analysis of
/// it — the entry's keystroke and the module's — resolves the world once,
/// after the entry walks, and has to agree with a clean analysis doing the
/// same.
const ENTRY_IMPL_SHAPES: &str = "export struct Foo {\n\tn: i32,\n}\n";

const ENTRY_IMPL_WAVER: &str = "import pkg::shapes::Foo;\n\nexport fun waved(): str {\n\tlet foo = Foo { n = 3 };\n\tfoo.wave()\n}\n";

const ENTRY_IMPL_MAIN: &str = "import pkg::shapes::Foo;\nimport pkg::waver::waved;\n\nimpl Foo {\n\tfun wave(self) {\n\t\t\"wave\"\n\t}\n}\n\nfun main() {\n\tprint(waved());\n}\n";

const ENTRY_IMPL_EDITS: &[Edit] = &[
    Edit {
        label: "the entry's impl a module calls, its body edited (B553)",
        file: "main.vl",
        seed: None,
        replacements: &[("\t\t\"wave\"", "\t\t\"WAVE\"")],
    },
    Edit {
        label: "the module that calls the entry's impl, edited (B553)",
        file: "waver.vl",
        seed: None,
        replacements: &[("Foo { n = 3 }", "Foo { n = 4 }")],
    },
];

const PLATFORM_MAIN: &str = "import pkg::shared::label;\nimport pkg::twin::place;\n\nfun main() {\n\tprint(label());\n\tprint(place());\n}\n";

// B573: a module with `[platform(..)]` twins, in the browser package — the twin
// a hot-set walk over the stored world selects is the build platform's.
const PLATFORM_TWIN: &str = "[platform(\"browser\")]\nexport fun place(): str {\n\t\"browser twin\"\n}\n\n[platform(\"@process\")]\nexport fun place(): str {\n\t\"process twin\"\n}\n";

const PLATFORM_SHARED: &str = "export fun label(): str {\n\t\"x\"\n}\n";

const PLATFORM_EDITS: &[Edit] = &[
    Edit {
        label: "a twinned module edited in a browser world (B573)",
        file: "twin.vl",
        seed: None,
        replacements: &[("\t\"browser twin\"", "\t\"browser twin!\"")],
    },
    Edit {
        label: "a browser entry reaches a node-only call (i5)",
        file: "shared.vl",
        seed: None,
        replacements: &[(
            "export fun label(): str {\n\t\"x\"\n}\n",
            "import std::process;\n\nexport fun label(): str {\n\ti\"x{process::args().len()}\"\n}\n",
        )],
    },
];

fn platform_package() -> Package {
    Package::write(
        "platform",
        Platform::Browser,
        &[
            ("main.vl", PLATFORM_MAIN),
            ("shared.vl", PLATFORM_SHARED),
            ("twin.vl", PLATFORM_TWIN),
        ],
    )
}

/// The package the re-walk pins measure, with nothing a guard refuses: the
/// hot-set world is built for every module of it, so the edits below run
/// through S1's reuse and nothing else.
fn leaf_package() -> Package {
    Package::write(
        "leaf",
        Platform::default(),
        &[
            ("main.vl", PINS_MAIN),
            ("views.vl", PINS_VIEWS),
            ("cycle_a.vl", PINS_CYCLE_A),
            ("cycle_b.vl", PINS_CYCLE_B),
            ("hub.vl", PINS_HUB),
        ],
    )
}

/// Edits over [`leaf_package`], each served from a hot-set world: a Class A
/// refusal typed into the hot leaf (whose remembered checks must never be
/// replayed), and the PREFIX moving under the leaf's seed (which must never be
/// served unvalidated).
const LEAF_EDITS: &[Edit] = &[
    Edit {
        label: "a statement typed into the hot leaf",
        file: "views.vl",
        seed: None,
        replacements: &[("\tbase() + 1\n", "\tlet extra = 1;\n\tbase() + extra\n")],
    },
    Edit {
        label: "a Class A refusal typed into the hot leaf",
        file: "views.vl",
        seed: None,
        replacements: &[(
            "\tbase() + 1\n",
            "\tlet frozen = 1;\n\tfrozen = 2;\n\tbase() + frozen\n",
        )],
    },
    Edit {
        label: "the PREFIX module the leaf imports changes under the leaf's seed",
        file: "hub.vl",
        seed: Some("views.vl"),
        replacements: &[("\t2\n", "\t3\n")],
    },
    Edit {
        label: "a body edit inside the hot cycle",
        file: "cycle_b.vl",
        seed: None,
        replacements: &[("\tring(n + 1)", "\tring(n + 2)")],
    },
];

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
    vilan_core::analyzer::base_cache_clear();
    Replayed {
        divergences,
        censuses,
        served,
        entry_impl,
    }
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
    // M110 S4: the const cache served sites — or every comparison above said
    // nothing about it.
    let const_hits: u64 = censuses.iter().map(|census| census.const_cache_hits).sum();
    assert!(
        const_hits > 0,
        "the classes leg must serve const sites from the const cache"
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
/// decides a prefix binding's type by its first use — a context's `run`, a
/// push into a module's empty list — is then walked after the prefix decided
/// it, where the canonical world walks it first.
#[test]
fn the_differential_sees_a_binding_the_hot_set_should_have_decided() {
    let _switch = SWITCH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let divergences = replay_with_plant(Plant::UseInferredGuardOff);
    assert!(
        divergences
            .iter()
            .any(|divergence| divergence.contains("grounds a prefix context")),
        "the use-inferred plant must turn the context edit red; it found: {divergences:#?}"
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

// --- the re-walk counter pins (Q9) ------------------------------------------------

const PINS_MAIN: &str = "import pkg::views::render;\nimport pkg::cycle_a::ring;\nimport pkg::hub::base;\n\nfun main() {\n\tprint(render());\n\tprint(ring(1));\n\tprint(base());\n}\n";

const PINS_VIEWS: &str = "import pkg::hub::base;\n\nexport fun render(): i32 {\n\tbase() + 1\n}\n";

const PINS_CYCLE_A: &str = "import pkg::cycle_b::bounce;\n\nexport fun ring(n: i32): i32 {\n\tif n > 3 {\n\t\tn\n\t} else {\n\t\tbounce(n + 1)\n\t}\n}\n";

const PINS_CYCLE_B: &str =
    "import pkg::cycle_a::ring;\n\nexport fun bounce(n: i32): i32 {\n\tring(n + 1)\n}\n";

const PINS_HUB: &str = "export fun base(): i32 {\n\t2\n}\n";

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
