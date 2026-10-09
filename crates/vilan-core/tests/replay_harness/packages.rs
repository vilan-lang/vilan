//! The edit-replay differential's PACKAGES and its observation harness
//! (`incremental-analysis.md` §6; `analyzer-pass-map.md` §6), shared by the
//! two gates that drive them: `edit_replay_differential` (incremental against
//! clean, edit by edit) and `permutation_differential` (clean against clean,
//! the package's file names reversed or its modules nested deeper — Q4 of the
//! pass map, the gate the edit-replay differential cannot be).
//!
//! What lives here is what both read: a [`Package`] on disk whose open buffers
//! move through the document overlay, [`observe`] (one analysis of its entry
//! on a big-stack worker, in one of three [`Leg`]s), the [`Edit`]s the scripts
//! make, and every fixture — the §6 classes package, M121's served impls,
//! B553's entry impl, B573's browser twins, the re-walk pins' leaf package,
//! the post-pass packages (B575) and the corpus leg's hosting.

#![allow(dead_code)]

use std::path::PathBuf;

use vilan_core::incremental::{
    Census, clean_analysis, clean_analysis_keeping_hot_shape, render_observation,
};
use vilan_core::{BuildOptions, Platform, Workspace, analyze_source, transform};

use super::std_spec;

/// `Platform::default()`, spelled so a fixture can be a `const`.
pub const NODE: Platform = Platform::Node {
    version: vilan_core::target::NODE_LTS,
};

/// One package on disk, its open buffers in the document overlay.
pub struct Package {
    pub directory: PathBuf,
    pub entry: PathBuf,
    pub platform: Platform,
    /// The text each file holds NOW (the overlay's, once edited).
    pub texts: Vec<(String, String)>,
}

impl Package {
    pub fn write(name: &str, platform: Platform, files: &[(&str, &str)]) -> Package {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let directory = crate::scratch::root()
            .join(format!("vilan_m110_{name}_{}_{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("create the package directory");
        for (file, text) in files {
            let path = directory.join(file);
            // The permutation differential nests modules a directory deeper.
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("create a package directory");
            }
            std::fs::write(path, text).expect("write a package file");
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

    pub fn path(&self, file: &str) -> PathBuf {
        self.directory.join(file)
    }

    pub fn text(&self, file: &str) -> &str {
        &self
            .texts
            .iter()
            .find(|(name, _)| name == file)
            .unwrap_or_else(|| panic!("no file {file} in the package"))
            .1
    }

    /// The editor's keystroke: the buffer moves, through the overlay every
    /// load and every base-cache validation reads.
    pub fn set(&mut self, file: &str, text: String) {
        vilan_core::analyzer::set_document_overlay(&self.path(file), Some(text.clone()));
        for (name, current) in &mut self.texts {
            if name == file {
                *current = text;
                return;
            }
        }
        panic!("no file {file} in the package");
    }

    pub fn remove(self) {
        for (file, _) in &self.texts {
            vilan_core::analyzer::set_document_overlay(&self.path(file), None);
        }
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// What one analysis showed, and what it did to get there.
pub struct Observation {
    pub rendering: String,
    pub javascript: Option<String>,
    pub census: Census,
}

/// Which analysis [`observe`] runs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Leg {
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
pub fn observe(package: &Package, seeds: Vec<PathBuf>, leg: Leg) -> Observation {
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
pub fn first_difference(incremental: &str, clean: &str) -> String {
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
pub struct Edit {
    pub label: &'static str,
    pub file: &'static str,
    pub seed: Option<&'static str>,
    pub replacements: &'static [(&'static str, &'static str)],
}

// --- the §6 edit classes ------------------------------------------------------

pub const CLASSES_MAIN: &str = "import pkg::views::render;\nimport pkg::cycle_a::ring;\nimport pkg::impls;\nimport pkg::user::use_it;\nimport pkg::side::side_value;\nimport pkg::ext;\nimport pkg::reader::shouted;\nimport pkg::runner::run_read;\nimport pkg::bag::fill;\nimport pkg::a_spoil::spoil;\nimport pkg::painter::painted;\nimport pkg::shiner::shone;\nimport pkg::bar_eq;\nimport pkg::bar_user::bars_match;\nimport pkg::data_user::{ banner, smaller };\n\nfun main() {\n\tprint(render());\n\tprint(ring(1));\n\tprint(use_it());\n\tprint(side_value());\n\tprint(shouted());\n\tprint(run_read());\n\tfill();\n\tspoil();\n\tprint(painted());\n\tprint(shone());\n\tprint(bars_match());\n\tprint(banner());\n\tprint(smaller());\n}\n";

// S1's hazards, each a module the PREFIX holds and a module the hot set holds:
// an inherent impl on a prefix type, written in a module the caller never
// imports (`ext` / `reader`, the impl guard's case); a context whose value type
// only a hot module's `run` grounds (`ctx` / `runner`, the early-commitment
// guard's case); and a module binding two modules push into, the hot one
// first in load order (`bag` / `a_spoil`, which side of a conflict is blamed).
pub const CLASSES_EXT: &str =
    "import pkg::shapes::Foo;\n\nimpl Foo {\n\tfun shout(self): str {\n\t\t\"HI\"\n\t}\n}\n";

pub const CLASSES_READER: &str = "import pkg::shapes::Foo;\n\nexport fun shouted(): str {\n\tlet foo = Foo { n = 2 };\n\tfoo.shout()\n}\n";

pub const CLASSES_CTX: &str = "import std::context::Context;\n\nexport let mood = Context::new();\n\nexport fun read_mood(): i32 {\n\tmood.get() + 1\n}\n";

pub const CLASSES_RUNNER: &str = "import pkg::ctx::{ mood, read_mood };\n\nexport fun run_read(): i32 {\n\tmut seen = 0;\n\tmood.run(5, || {\n\t\tseen = read_mood();\n\t});\n\tseen\n}\n";

pub const CLASSES_BAG: &str = "export mut bag = [];\n\nexport fun fill() {\n\tbag.push(1);\n}\n";

pub const CLASSES_SPOIL: &str =
    "import pkg::bag::bag;\n\nexport fun spoil() {\n\tbag.push(2);\n}\n";

pub const CLASSES_MODEL: &str = "import std::context::Context;\n\nexport struct Item {\n\tname: str,\n\tcount: i32,\n}\n\nexport let flavor: Context<i32> = Context::new();\n\nmut items = [];\n\nexport fun add() {\n\titems.push(1);\n}\n\nexport fun show(): i32 {\n\titems[0] + 1\n}\n\nexport fun make() {\n\t1\n}\n\nexport fun leaf(): i32 {\n\t1\n}\n\nexport fun size(): i32 {\n\t4\n}\n\nexport fun first_item(): Item {\n\tItem { name = \"a\", count = 1 }\n}\n";

pub const CLASSES_VIEWS: &str = "import pkg::model::{ add, first_item, leaf, make, show, size };\n\nexport fun render(): i32 {\n\tadd();\n\tlet made: i32 = make();\n\tlet item = first_item();\n\tlet table = const size() * 2;\n\tmade + show() + leaf() + item.count + table\n}\n";

pub const CLASSES_CYCLE_A: &str = "import pkg::cycle_b::bounce;\nimport pkg::model::leaf;\n\nexport fun ring(n: i32): i32 {\n\tif n > 3 {\n\t\tn\n\t} else {\n\t\tbounce(n + 1) + leaf()\n\t}\n}\n";

pub const CLASSES_CYCLE_B: &str =
    "import pkg::cycle_a::ring;\n\nexport fun bounce(n: i32): i32 {\n\tring(n + 1)\n}\n";

pub const CLASSES_SHAPES: &str = "export struct Foo {\n\tn: i32,\n}\n\nexport trait Greet {\n\tfun greet(self): str;\n}\n\nexport struct Bar {\n\tn: i32,\n}\n\nexport trait Shine {\n\tfun shine(self): i32;\n}\n";

// M121's hot impls on PREFIX types, as kolt writes them: an inherent impl
// whose members' returns are inferred (`styles.vl`'s `impl style::Style`), and
// a trait impl on a type another module declares (`prefs.vl`'s `impl HashMap
// with Json`) — each called only from a module that imports it, so no
// question the stored prefix asked reaches them and the hot-set world serves
// them. And one the prefix DOES reach, through an operator: `bar_user` (a
// prefix module) compares two `Bar`s, and `bar_eq` (hot) is their `PartialEq`.
pub const CLASSES_PAINT: &str =
    "import pkg::shapes::Foo;\n\nimpl Foo {\n\tfun paint(self) {\n\t\t\"p\"\n\t}\n}\n";

pub const CLASSES_PAINTER: &str = "import pkg::paint;\nimport pkg::shapes::Foo;\n\nexport fun painted(): str {\n\tlet foo = Foo { n = 4 };\n\tfoo.paint()\n}\n";

pub const CLASSES_SHINE: &str = "import pkg::shapes::{ Foo, Shine };\n\nimpl Foo with Shine {\n\tfun shine(self): i32 {\n\t\tself.n\n\t}\n}\n";

pub const CLASSES_SHINER: &str = "import pkg::shine;\nimport pkg::shapes::{ Foo, Shine };\n\nexport fun shone(): i32 {\n\tlet foo = Foo { n = 5 };\n\tfoo.shine()\n}\n";

pub const CLASSES_BAR_EQ: &str = "import std::compare::PartialEq;\nimport pkg::shapes::Bar;\n\nimpl Bar with PartialEq {\n\tfun eq(self, other: Self): bool {\n\t\tself.n == other.n\n\t}\n}\n";

// M110 S4's const sites over the project and std: one reads an input file the
// edit script moves, one calls a std function the script edits in std itself.
pub const CLASSES_DATA_USER: &str = "import std::math;\nimport std::web::asset;\n\nexport fun banner(): str {\n\tconst asset::read(\"banner.txt\")\n}\n\nexport fun smaller(): i32 {\n\tconst math::minmax(2, 1).0\n}\n";

pub const CLASSES_BANNER: &str = "hello\n";

pub const CLASSES_BAR_USER: &str = "import pkg::shapes::Bar;\n\nexport fun bars_match(): bool {\n\tBar { n = 1 } == Bar { n = 1 }\n}\n";

pub const CLASSES_IMPLS: &str = "import pkg::shapes::{ Foo, Greet };\n\nexport impl Foo with Greet {\n\tfun greet(self): str {\n\t\t\"hi\"\n\t}\n}\n";

pub const CLASSES_USER: &str = "import pkg::shapes::{ Foo, Greet };\n\nexport fun use_it(): str {\n\tlet foo = Foo { n = 1 };\n\tfoo.greet()\n}\n";

pub const CLASSES_SIDE: &str = "export fun side_value(): i32 {\n\t2\n}\n";

pub const CLASSES_FILES: &[(&str, &str)] = &[
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
];

pub fn classes_package() -> Package {
    Package::write("classes", Platform::default(), CLASSES_FILES)
}

/// §6's list, one edit per class, each applied and undone.
pub const CLASS_EDITS: &[Edit] = &[
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
pub const STD_EDIT_FILE: &str = "src/math.vl";
pub const STD_EDIT_LABEL: &str = "a std function a const site calls, edited (S4's std bump)";
pub const STD_EDIT: (&str, &str) = (
    "\tif a <= b {\n\t\t(a, b)\n\t} else {\n\t\t(b, a)\n\t}",
    "\tif a <= b {\n\t\t(b, a)\n\t} else {\n\t\t(a, b)\n\t}",
);

/// M121's acceptance shape, over the classes package: a keystroke in a module
/// whose hot set writes impls on PREFIX types that nothing in the prefix asks
/// about. S1's spelling guard refused these (kolt's `theme.vl`, `model.vl`
/// and `styles.vl` every keystroke); the reach probe serves them, and each
/// must be a hot-set world as well as agree with a clean analysis.
pub const SERVED_IMPL_EDITS: &[Edit] = &[
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
pub const ENTRY_IMPL_SHAPES: &str = "export struct Foo {\n\tn: i32,\n}\n";

pub const ENTRY_IMPL_WAVER: &str = "import pkg::shapes::Foo;\n\nexport fun waved(): str {\n\tlet foo = Foo { n = 3 };\n\tfoo.wave()\n}\n";

pub const ENTRY_IMPL_MAIN: &str = "import pkg::shapes::Foo;\nimport pkg::waver::waved;\n\nimpl Foo {\n\tfun wave(self) {\n\t\t\"wave\"\n\t}\n}\n\nfun main() {\n\tprint(waved());\n}\n";

pub const ENTRY_IMPL_EDITS: &[Edit] = &[
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

pub const PLATFORM_MAIN: &str = "import pkg::shared::label;\nimport pkg::twin::place;\n\nfun main() {\n\tprint(label());\n\tprint(place());\n}\n";

// B573: a module with `[platform(..)]` twins, in the browser package — the twin
// a hot-set walk over the stored world selects is the build platform's.
pub const PLATFORM_TWIN: &str = "[platform(\"browser\")]\nexport fun place(): str {\n\t\"browser twin\"\n}\n\n[platform(\"@process\")]\nexport fun place(): str {\n\t\"process twin\"\n}\n";

pub const PLATFORM_SHARED: &str = "export fun label(): str {\n\t\"x\"\n}\n";

pub const PLATFORM_EDITS: &[Edit] = &[
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

pub fn platform_package() -> Package {
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
pub fn leaf_package() -> Package {
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
pub const LEAF_EDITS: &[Edit] = &[
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

/// The Class A refusal every corpus module gets, so each has a diagnostic of
/// its own to remember (M57's lesson: sixty clean modules agree under any
/// reuse at all).
pub const PROBE_REFUSAL: &str =
    "\nfun m110_probe_refusal(): i32 {\n\tlet total = 1;\n\ttotal = 2;\n\ttotal\n}\n";

pub const CORPUS_MAIN: &str = "import pkg::module;\nimport pkg::user;\nimport pkg::side;\n\nfun main() {\n\tprint(user::user_value());\n\tprint(side::side_value());\n}\n";

pub const CORPUS_USER: &str = "import pkg::module;\n\nexport fun user_value(): i32 {\n\t1\n}\n";

pub const CORPUS_SIDE: &str = "export fun side_value(): i32 {\n\t2\n}\n";

pub const CORPUS_EDITS: &[Edit] = &[
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

pub const PINS_MAIN: &str = "import pkg::views::render;\nimport pkg::cycle_a::ring;\nimport pkg::hub::base;\n\nfun main() {\n\tprint(render());\n\tprint(ring(1));\n\tprint(base());\n}\n";

pub const PINS_VIEWS: &str =
    "import pkg::hub::base;\n\nexport fun render(): i32 {\n\tbase() + 1\n}\n";

pub const PINS_CYCLE_A: &str = "import pkg::cycle_b::bounce;\n\nexport fun ring(n: i32): i32 {\n\tif n > 3 {\n\t\tn\n\t} else {\n\t\tbounce(n + 1)\n\t}\n}\n";

pub const PINS_CYCLE_B: &str =
    "import pkg::cycle_a::ring;\n\nexport fun bounce(n: i32): i32 {\n\tring(n + 1)\n}\n";

pub const PINS_HUB: &str = "export fun base(): i32 {\n\t2\n}\n";

impl Edit {
    /// `text` with this edit's replacements applied, in order. Panics when an
    /// anchor is missing, naming the edit: a script whose anchor drifted says
    /// nothing about the analysis.
    pub fn apply(&self, text: &str) -> String {
        let mut after = text.to_string();
        for (find, replace) in self.replacements {
            assert!(
                after.contains(find),
                "{}: the edit's anchor {find:?} is not in {}",
                self.label,
                self.file
            );
            after = after.replacen(find, replace, 1);
        }
        after
    }
}

/// One fixture package: its name, platform, files and the edits its script
/// makes — the shape both differentials iterate.
pub struct Fixture {
    pub name: &'static str,
    pub platform: Platform,
    pub files: &'static [(&'static str, &'static str)],
    pub edits: &'static [Edit],
}

impl Fixture {
    /// Every STATE the script puts the package in: the base texts, then each
    /// edit applied on its own (the undo returns to the base, which is the
    /// first state). The permutation differential compares each.
    pub fn states(&self) -> Vec<(String, Vec<(String, String)>)> {
        let base: Vec<(String, String)> = self
            .files
            .iter()
            .map(|(file, text)| (file.to_string(), text.to_string()))
            .collect();
        let mut states = vec![(format!("{}: the base texts", self.name), base.clone())];
        for edit in self.edits {
            let files = base
                .iter()
                .map(|(file, text)| {
                    let text = if file == edit.file {
                        edit.apply(text)
                    } else {
                        text.clone()
                    };
                    (file.clone(), text)
                })
                .collect();
            states.push((format!("{}: {}", self.name, edit.label), files));
        }
        states
    }

    pub fn write(&self) -> Package {
        Package::write(self.name, self.platform, self.files)
    }
}

// --- the post-pass class (pass map §5.4, §6 "does not prove" 2; B575) -------------
//
// A PREFIX module carrying a diagnostic a POST pass decides — one the Class A
// window never records, because the post passes run after `take_reuse_record`
// — with a hot leaf beside it that the script types into. The verdict has to
// survive every keystroke elsewhere, on the hot-set miss and on the hit alike.
// Four shapes, one package each, because three of the four are gated on a
// clean program (the const pass and the coverage refusals stand down when any
// other error exists): E3's implicit-suspension refusal (`check_invalidation`
// enrols, `check_view_suspensions` decides), a `context` coverage refusal, a
// platform refusal and a const failure.

const POST_PASS_VIEWS: &str = "export fun render(): i32 {\n\t1\n}\n";

const POST_PASS_MAIN: &str = "import pkg::m::probe;\nimport pkg::views::render;\n\nfun main() {\n\tprint(probe());\n\tprint(render());\n}\n";

const POST_PASS_EDITS: &[Edit] = &[
    Edit {
        label: "a statement typed into the leaf beside the prefix module's post-pass verdict",
        file: "views.vl",
        seed: None,
        replacements: &[("\t1\n", "\tlet extra = 1;\n\textra\n")],
    },
    Edit {
        label: "a statement typed into the entry beside the prefix module's post-pass verdict",
        file: "main.vl",
        seed: None,
        replacements: &[(
            "\tprint(render());\n",
            "\tprint(render());\n\tlet extra = 1;\n",
        )],
    },
];

/// E3's implicit half (B119): `stash` takes views and calls an `async` function
/// without an `await` token, so the signature rule is decided by the post pass
/// once the async set is known.
const SUSPENSION_M: &str = "export struct Point {\n\tx: i32,\n\ty: i32,\n}\n\nasync fun tick(): i32 {\n\t1\n}\n\nexport fun stash(viewed: &mut Point, other: &Point) {\n\tlet beat = tick();\n\tviewed.x = beat + other.y;\n}\n\nexport fun probe(): i32 {\n\tmut point = Point { x = 1, y = 2 };\n\tlet seen = Point { x = 3, y = 4 };\n\tstash(&mut point, &seen);\n\tpoint.x\n}\n";

/// A context read with no `run` on any path from `main`: the context pass's
/// coverage refusal.
const COVERAGE_M: &str = "import std::context::Context;\n\nexport let mood: Context<i32> = Context::new();\n\nexport fun probe(): i32 {\n\tmood.get() + 1\n}\n";

/// A node-only call reached from a browser entry: platform colour's refusal.
const PLATFORM_REFUSAL_M: &str =
    "import std::process;\n\nexport fun probe(): i32 {\n\tprocess::args().len()\n}\n";

/// A `const` site reading a file the package does not hold: the const pass's
/// failure, the one diagnostic of the program (the pass runs only when there
/// is no other).
const CONST_FAILURE_M: &str = "import std::web::asset;\n\nexport fun probe(): str {\n\tconst asset::read(\"missing.txt\")\n}\n";

pub const POST_PASS_FIXTURES: &[Fixture] = &[
    Fixture {
        name: "post_pass_suspension",
        platform: NODE,
        files: &[
            ("main.vl", POST_PASS_MAIN),
            ("m.vl", SUSPENSION_M),
            ("views.vl", POST_PASS_VIEWS),
        ],
        edits: POST_PASS_EDITS,
    },
    Fixture {
        name: "post_pass_coverage",
        platform: NODE,
        files: &[
            ("main.vl", POST_PASS_MAIN),
            ("m.vl", COVERAGE_M),
            ("views.vl", POST_PASS_VIEWS),
        ],
        edits: POST_PASS_EDITS,
    },
    Fixture {
        name: "post_pass_platform",
        platform: Platform::Browser,
        files: &[
            ("main.vl", POST_PASS_MAIN),
            ("m.vl", PLATFORM_REFUSAL_M),
            ("views.vl", POST_PASS_VIEWS),
        ],
        edits: POST_PASS_EDITS,
    },
    Fixture {
        name: "post_pass_const",
        platform: NODE,
        files: &[
            ("main.vl", POST_PASS_MAIN),
            ("m.vl", CONST_FAILURE_M),
            ("views.vl", POST_PASS_VIEWS),
        ],
        edits: POST_PASS_EDITS,
    },
];

// --- B554: blame by file name (pass map §5.2) ---------------------------------------

/// Two modules push `1` and `"two"` into one `mut bag = []`; the error lands
/// on whichever push's module loads later in name order. The permutation
/// differential's known red until the element-slot rule blames the
/// declaration (or names every push).
pub const B554_FIXTURE: Fixture = Fixture {
    name: "b554",
    platform: NODE,
    files: &[
        (
            "main.vl",
            "import pkg::bag::fill;\nimport pkg::spoil::spoil;\n\nfun main() {\n\tfill();\n\tspoil();\n}\n",
        ),
        (
            "bag.vl",
            "export mut bag = [];\n\nexport fun fill() {\n\tbag.push(1);\n}\n",
        ),
        (
            "spoil.vl",
            "import pkg::bag::bag;\n\nexport fun spoil() {\n\tbag.push(\"two\");\n}\n",
        ),
    ],
    edits: &[],
};

// --- the fixtures as the permutation differential iterates them -------------------

pub const CLASSES_FIXTURE: Fixture = Fixture {
    name: "classes",
    platform: NODE,
    files: CLASSES_FILES,
    edits: CLASS_EDITS,
};

pub const SERVED_IMPL_FIXTURE: Fixture = Fixture {
    name: "classes_served",
    platform: NODE,
    files: CLASSES_FILES,
    edits: SERVED_IMPL_EDITS,
};

pub const ENTRY_IMPL_FIXTURE: Fixture = Fixture {
    name: "entry_impl",
    platform: NODE,
    files: &[
        ("main.vl", ENTRY_IMPL_MAIN),
        ("shapes.vl", ENTRY_IMPL_SHAPES),
        ("waver.vl", ENTRY_IMPL_WAVER),
    ],
    edits: ENTRY_IMPL_EDITS,
};

pub const PLATFORM_FIXTURE: Fixture = Fixture {
    name: "platform",
    platform: Platform::Browser,
    files: &[
        ("main.vl", PLATFORM_MAIN),
        ("shared.vl", PLATFORM_SHARED),
        ("twin.vl", PLATFORM_TWIN),
    ],
    edits: PLATFORM_EDITS,
};

pub const LEAF_FIXTURE: Fixture = Fixture {
    name: "leaf",
    platform: NODE,
    files: &[
        ("main.vl", PINS_MAIN),
        ("views.vl", PINS_VIEWS),
        ("cycle_a.vl", PINS_CYCLE_A),
        ("cycle_b.vl", PINS_CYCLE_B),
        ("hub.vl", PINS_HUB),
    ],
    edits: LEAF_EDITS,
};
