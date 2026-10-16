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
use vilan_core::{
    BuildOptions, EntryMode, PackageSpec, Platform, Workspace, analyze_source, transform,
};

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
    /// M152: the dependency packages beside it — each one's import name and
    /// its root, written as a `[library]` (a `lib.vl` surface) outside the
    /// package's own root. Empty for every fixture but the dependency ones.
    pub dependencies: Vec<(String, PathBuf)>,
}

/// M152: a dependency package a fixture imports (`import shapes::Shape`): its
/// import name and its files, `lib.vl` the surface.
pub struct Dependency {
    pub name: &'static str,
    pub files: &'static [(&'static str, &'static str)],
}

impl Package {
    pub fn write(name: &str, platform: Platform, files: &[(&str, &str)]) -> Package {
        Package::write_with(name, platform, files, &[])
    }

    /// [`Package::write`] with `dependencies` written beside it (M152).
    pub fn write_with(
        name: &str,
        platform: Platform,
        files: &[(&str, &str)],
        dependencies: &[Dependency],
    ) -> Package {
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
        let dependencies = dependencies
            .iter()
            .map(|dependency| {
                let root = directory
                    .with_file_name(format!(
                        "{}_deps",
                        directory
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    ))
                    .join(dependency.name);
                let _ = std::fs::remove_dir_all(&root);
                for (file, text) in dependency.files {
                    let path = root.join(file);
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent).expect("create a dependency directory");
                    }
                    std::fs::write(path, text).expect("write a dependency file");
                }
                (dependency.name.to_string(), root)
            })
            .collect();
        Package {
            entry: directory.join("main.vl"),
            directory,
            platform,
            texts: files
                .iter()
                .map(|(file, text)| (file.to_string(), text.to_string()))
                .collect(),
            dependencies,
        }
    }

    /// The workspace an analysis of this package runs in: its dependencies as
    /// `[library]` packages (M152), and the editor's shape otherwise.
    pub fn workspace(&self, seeds: Vec<PathBuf>) -> Workspace {
        Workspace {
            packages: self
                .dependencies
                .iter()
                .map(|(_, root)| PackageSpec {
                    base_root: root.clone(),
                    layers: Vec::new(),
                    dependencies: Vec::new(),
                    surface: true,
                    member: false,
                    prelude: Default::default(),
                })
                .collect(),
            entry_dependencies: self
                .dependencies
                .iter()
                .enumerate()
                .map(|(index, (name, _))| (name.clone(), index))
                .collect(),
            hot_seeds: seeds,
            // The editor's shape (M148): the language server analyzes under
            // the reading aids, which is what makes every file render — and
            // record — its hover rows.
            reading_aids: true,
            ..Workspace::default()
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
        if let Some((_, root)) = self.dependencies.first()
            && let Some(deps) = root.parent()
        {
            let _ = std::fs::remove_dir_all(deps);
        }
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
    let workspace = package.workspace(seeds);
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(entry_text.into_boxed_str());
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

/// M159: one analysis of `file` as a LONE open document — the language
/// server's M104 hybrid, the editor's common row: the module is the entry
/// (file mode, the package's `main` named as its declared program, as a
/// manifest would name it), and the incremental leg is SEEDED with the
/// document's own path (door (a)), so the post passes' record is filed and
/// read as on the world-mode path. A seed that is the entry names no hot
/// module: there is no hot-set shape, so the clean leg is the canonical one
/// for both comparisons.
pub fn observe_lone(package: &Package, file: &str, leg: Leg) -> Observation {
    let entry_text = package.text(file).to_string();
    let pkg_root = package.directory.clone();
    let entry = package.path(file);
    let platform = package.platform;
    let seeds = match leg {
        Leg::Incremental => vec![entry.clone()],
        Leg::Clean | Leg::CleanSameShape => Vec::new(),
    };
    let workspace = Workspace {
        entry_mode: EntryMode::OpenFile {
            declared_entries: vec!["main".to_string()],
        },
        ..package.workspace(seeds)
    };
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(entry_text.into_boxed_str());
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
                Leg::Clean | Leg::CleanSameShape => clean_analysis(analyze),
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

// M110 S7b: `kept_warning` is an item the leaf's edits never touch, so every
// served keystroke replays its deprecation WARNING from the item's record —
// the plant that leaves a served window live for the Class A walk derives it
// a second time (`the_differential_sees_a_pass_walking_a_served_window`). A
// warning rather than a refusal: the package must keep emitting, since the
// differential's JS leg is what sees a prefix served stale (`2` for `3`).
pub const PINS_VIEWS: &str = "import pkg::hub::base;\n\nexport fun render(): i32 {\n\tbase() + 1\n}\n\n[deprecated(\"use render()\")]\nfun stale(): i32 {\n\t1\n}\n\nfun kept_warning(): i32 {\n\tstale()\n}\n";

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
    /// M152: the dependency packages the fixture imports (empty for most).
    pub dependencies: &'static [Dependency],
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
        Package::write_with(self.name, self.platform, self.files, self.dependencies)
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
        dependencies: &[],
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
        dependencies: &[],
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
        dependencies: &[],
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
        dependencies: &[],
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
    dependencies: &[],
};

// --- the fixtures as the permutation differential iterates them -------------------

pub const CLASSES_FIXTURE: Fixture = Fixture {
    name: "classes",
    platform: NODE,
    files: CLASSES_FILES,
    edits: CLASS_EDITS,
    dependencies: &[],
};

pub const SERVED_IMPL_FIXTURE: Fixture = Fixture {
    name: "classes_served",
    platform: NODE,
    files: CLASSES_FILES,
    edits: SERVED_IMPL_EDITS,
    dependencies: &[],
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
    dependencies: &[],
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
    dependencies: &[],
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
    dependencies: &[],
};

// --- M128: answers chosen by first match over a load-ordered table (§5.6) ---------
//
// Each fixture puts TWO candidates for one of the four sites in different
// load positions, so the reversed world meets the other one first. The
// ranking (or the scope chain) has to choose the same candidate either way.

/// `callable_call_signature`: a concrete `impl Foo with Callable` beside a
/// blanket over a marker trait `Foo` implements, in modules at the two ends
/// of the load order; a `Callable` value coerced where a closure is wanted
/// takes the concrete block's `call` (B456's rule), whichever registered
/// first.
const M128_CALLABLE_SHAPES: &str =
    "export struct Foo {\n\tn: i32,\n}\n\nexport trait Marker {}\n\nimpl Foo with Marker {}\n";

const M128_CALLABLE_A: &str = "import std::operators::Callable;\nimport pkg::shapes::Foo;\n\nexport impl Foo with Callable {\n\tfun call(self, n: i32): i32 {\n\t\tself.n + n\n\t}\n}\n";

const M128_CALLABLE_Z: &str = "import std::operators::Callable;\nimport pkg::shapes::Marker;\n\nexport impl type T: Marker with Callable {\n\tfun call(self, n: i32): str {\n\t\t\"blanket\"\n\t}\n}\n";

const M128_CALLABLE_USER: &str = "import pkg::a_call;\nimport pkg::z_call;\nimport pkg::shapes::Foo;\n\nfun apply(f: |i32| i32): i32 {\n\tf(1)\n}\n\nexport fun used(): i32 {\n\tlet foo = Foo { n = 2 };\n\tapply(foo)\n}\n";

const M128_CALLABLE_MAIN: &str = "import pkg::user::used;\n\nfun main() {\n\tprint(used());\n}\n";

/// `resolve_macro_reference`'s fallback: two modules each declare a `macro
/// fun Tagged` beside a `trait Tagged` (the derive convention: the plain name
/// is the trait's, so the reference takes the macro-namespace fallback) and
/// each applies its own macro to its own struct; the reference each
/// `[derive(Tagged)]` records (go-to-definition) is its own module's marker —
/// the scope chain — not the first module's in load order.
const M128_MACRO_A: &str = "macro fun Tagged(item: Item): Source {\n\timport macro_std::source;\n\timport macro_std::meta::{ Item, Source, StructItem };\n\timport macro_std::option::Option::{ self, Some, None };\n\n\tlet target = match item.as_struct() {\n\t\tSome(let found) => found,\n\t\tNone => StructItem { name = \"?\", fields = [], generics = [] },\n\t};\n\tsource(\"impl \" + target.name + \" with Tagged {\\nfun tag(self): str {\\n\\\"a:\" + target.name + \"\\\"\\n}\\n}\\n\")\n}\n\nexport trait Tagged {\n\tfun tag(self): str;\n}\n\n[derive(Tagged)]\nexport struct Widget {\n\tsize: i32,\n}\n\nexport fun widget_tag(): str {\n\tWidget { size = 1 }.tag()\n}\n";

const M128_MACRO_Z: &str = "macro fun Tagged(item: Item): Source {\n\timport macro_std::source;\n\timport macro_std::meta::{ Item, Source, StructItem };\n\timport macro_std::option::Option::{ self, Some, None };\n\n\tlet target = match item.as_struct() {\n\t\tSome(let found) => found,\n\t\tNone => StructItem { name = \"?\", fields = [], generics = [] },\n\t};\n\tsource(\"impl \" + target.name + \" with Tagged {\\nfun tag(self): str {\\n\\\"z:\" + target.name + \"\\\"\\n}\\n}\\n\")\n}\n\nexport trait Tagged {\n\tfun tag(self): str;\n}\n\n[derive(Tagged)]\nexport struct Gadget {\n\tsize: i32,\n}\n\nexport fun gadget_tag(): str {\n\tGadget { size = 1 }.tag()\n}\n";

const M128_MACRO_MAIN: &str = "import pkg::a_macros::widget_tag;\nimport pkg::z_macros::gadget_tag;\n\nfun main() {\n\tprint(widget_tag());\n\tprint(gadget_tag());\n}\n";

/// `import_path_of`: a trait method called without the trait imported — the
/// B515 steer spells the declaring module's path, found by the entity; the
/// site is the one first match that is not positional (an entity is declared
/// in one module scope), held to it here.
const M128_IMPORT_PATH_SHAPES: &str = "export trait Area {\n\tfun area(self): i32;\n}\n\nexport struct Sq {\n\ts: i32,\n}\n\nexport impl Sq with Area {\n\tfun area(self): i32 {\n\t\tself.s * self.s\n\t}\n}\n";

const M128_IMPORT_PATH_USER: &str =
    "import pkg::shapes::Sq;\n\nexport fun used(): i32 {\n\tSq { s = 2 }.area()\n}\n";

/// `for_each_next_providers`: two blocks provide a loop's inherited `next`
/// for one subject — `impl Box with Counting<i32> {}` and `impl Box with
/// Counting<str> {}` — in modules at the two ends of the load order, and the
/// looping file admits one of them; the recorded provider has to be the block
/// the lookup took, whichever registered first (the index stood in for it).
const M128_FOR_EACH_B: &str = "export struct Box {\n\tn: i32,\n}\n";

const M128_FOR_EACH_T: &str = "import std::option::Option::{ self, None, Some };\n\nexport trait Counting<T> {\n\tfun next(mut self): Option<T> {\n\t\tNone\n\t}\n}\n";

const M128_FOR_EACH_P1: &str =
    "import pkg::b::Box;\nimport pkg::t::Counting;\n\nexport impl Box with Counting<i32> {}\n";

const M128_FOR_EACH_P9: &str =
    "import pkg::b::Box;\nimport pkg::t::Counting;\n\nexport impl Box with Counting<str> {}\n";

const M128_FOR_EACH_MAIN: &str = "import pkg::b::Box;\nimport pkg::p1;\nimport pkg::p9 only;\n\nfun main() {\n\tmut box = Box { n = 1 };\n\tfor item in box {\n\t\tprint(item);\n\t}\n\tprint(\"done\");\n}\n";

pub const M128_FIXTURES: &[Fixture] = &[
    Fixture {
        name: "m128_callable",
        platform: NODE,
        files: &[
            ("main.vl", M128_CALLABLE_MAIN),
            ("shapes.vl", M128_CALLABLE_SHAPES),
            ("a_call.vl", M128_CALLABLE_A),
            ("z_call.vl", M128_CALLABLE_Z),
            ("user.vl", M128_CALLABLE_USER),
        ],
        edits: &[],
        dependencies: &[],
    },
    Fixture {
        name: "m128_macro",
        platform: NODE,
        files: &[
            ("main.vl", M128_MACRO_MAIN),
            ("a_macros.vl", M128_MACRO_A),
            ("z_macros.vl", M128_MACRO_Z),
        ],
        edits: &[],
        dependencies: &[],
    },
    Fixture {
        name: "m128_import_path",
        platform: NODE,
        files: &[
            ("main.vl", M128_CALLABLE_MAIN),
            ("shapes.vl", M128_IMPORT_PATH_SHAPES),
            ("user.vl", M128_IMPORT_PATH_USER),
        ],
        edits: &[],
        dependencies: &[],
    },
    Fixture {
        name: "m128_for_each",
        platform: NODE,
        files: &[
            ("main.vl", M128_FOR_EACH_MAIN),
            ("b.vl", M128_FOR_EACH_B),
            ("t.vl", M128_FOR_EACH_T),
            ("p1.vl", M128_FOR_EACH_P1),
            ("p9.vl", M128_FOR_EACH_P9),
        ],
        edits: &[Edit {
            label: "the looping file admits the other block instead",
            file: "main.vl",
            seed: None,
            replacements: &[(
                "import pkg::p1;\nimport pkg::p9 only;\n",
                "import pkg::p1 only;\nimport pkg::p9;\n",
            )],
        }],
        dependencies: &[],
    },
];

// --- M110 S2a: the bound audit's verdicts at prefix sites -----------------------
//
// A PREFIX module whose generic call fails its bound (`need(Bar { .. })` with
// `T: Shine` and no `impl Bar with Shine`) beside one that passes it through
// an impl in ANOTHER module the prefix does not import (the ENTRY loads it,
// as `ext`/`reader` load theirs), so the prefix stays prefix while the impl's
// module is hot; the leaf and the entry are typed around, and the impl is
// moved to the other type (its module hot, its question reaching the prefix:
// the real guard refuses the hot set; with the guard off the record is served
// stale). The audit's record is replayed for the prefix on every hit: a
// record served without the record (the `BoundAuditUnrecorded` plant) drops
// the refusal, a record served over a moved impl table (`ImplGuardOff`)
// keeps an answer the world no longer gives.

const BOUND_SHAPES: &str = "export struct Foo {\n\tn: i32,\n}\n\nexport struct Bar {\n\tn: i32,\n}\n\nexport trait Shine {\n\tfun shine(self): i32;\n}\n\nexport trait Marker {}\n";

/// Two foreign trait impls, so the module keeps the reach filter's "asks for
/// everything" shape — and the base-cache key — when the edit moves `Shine`
/// from `Foo` to `Bar`.
const BOUND_SHINE: &str = "import pkg::shapes::{ Bar, Foo, Marker, Shine };\n\nexport impl Bar with Marker {}\n\nexport impl Foo with Shine {\n\tfun shine(self): i32 {\n\t\tself.n\n\t}\n}\n";

const BOUND_BOUNDED: &str = "import pkg::shapes::{ Bar, Foo, Shine };\n\nfun need<type T: Shine>(t: T): i32 {\n\tt.shine()\n}\n\nexport fun bounded(): i32 {\n\tneed(Foo { n = 7 }) + need(Bar { n = 1 })\n}\n";

const BOUND_MAIN: &str = "import pkg::bounded::bounded;\nimport pkg::shine;\nimport pkg::views::render;\n\nfun main() {\n\tprint(bounded());\n\tprint(render());\n}\n";

pub const BOUND_FIXTURES: &[Fixture] = &[
    Fixture {
        name: "bound_audit",
        platform: NODE,
        files: &[
            ("main.vl", BOUND_MAIN),
            ("shapes.vl", BOUND_SHAPES),
            ("shine.vl", BOUND_SHINE),
            ("bounded.vl", BOUND_BOUNDED),
            ("views.vl", POST_PASS_VIEWS),
        ],
        edits: &[
            Edit {
                label: "a statement typed into the leaf beside the prefix module's bound refusal",
                file: "views.vl",
                seed: None,
                replacements: &[("\t1\n", "\tlet extra = 1;\n\textra\n")],
            },
            Edit {
                label: "a statement typed into the entry beside the prefix module's bound refusal",
                file: "main.vl",
                seed: None,
                replacements: &[(
                    "\tprint(render());\n",
                    "\tprint(render());\n\tlet extra = 1;\n",
                )],
            },
            Edit {
                label: "the impl a prefix module's bound is satisfied through, moved to the other type",
                file: "shine.vl",
                seed: None,
                replacements: &[(
                    "export impl Foo with Shine {",
                    "export impl Bar with Shine {",
                )],
            },
        ],
        dependencies: &[],
    },
    // M150: the same shape at a WRITTEN application (B251, `check_written_
    // nominal_bounds`) — a prefix module annotating `Holder<Foo>` (satisfied
    // through the hot module's impl) and `Holder<Bar>` (refused) — whose sites
    // ride M143's module lane: skipped for a reused module, their refusals
    // replayed from the audit's record under its late-impl guard.
    Fixture {
        name: "written_bound_audit",
        platform: NODE,
        files: &[
            ("main.vl", WRITTEN_MAIN),
            ("shapes.vl", BOUND_SHAPES),
            ("shine.vl", BOUND_SHINE),
            ("written.vl", WRITTEN_HELD),
            ("views.vl", POST_PASS_VIEWS),
        ],
        edits: &[
            Edit {
                label: "a statement typed into the leaf beside the prefix module's written-bound refusal",
                file: "views.vl",
                seed: None,
                replacements: &[("\t1\n", "\tlet extra = 1;\n\textra\n")],
            },
            Edit {
                label: "the impl a prefix module's written bound is satisfied through, moved to the other type",
                file: "shine.vl",
                seed: None,
                replacements: &[(
                    "export impl Foo with Shine {",
                    "export impl Bar with Shine {",
                )],
            },
        ],
        dependencies: &[],
    },
];

const WRITTEN_HELD: &str = "import pkg::shapes::{ Bar, Foo, Shine };\n\nstruct Holder<T: Shine> {\n\tvalue: T,\n}\n\nexport fun held(foo: Holder<Foo>, bar: Holder<Bar>): i32 {\n\t1\n}\n";

const WRITTEN_MAIN: &str = "import pkg::written::held;\nimport pkg::shine;\nimport pkg::views::render;\n\nfun main() {\n\tprint(render());\n}\n";

// --- M110 S3: the effect classes (incr-50, Order 50; pass map §6, §7.2) ----------
//
// A hot leaf whose EFFECTS the keystroke removes — a context read, a `sleep`,
// a node-only call — beside the prefix module that owns the context. The
// seeded async fixpoint (S3b) starts from the world's cold result with the
// hot nodes unknown, never from last keystroke's settled set: a monotone
// fixpoint cannot shrink from a seed that already holds the removed effect,
// which is what `Plant::PostSeedFromSettled` plants and the sleep's removal
// must catch (the emitted JS loses `wait`'s `async`). The second package is
// broken on purpose — a node call on a browser entry, and a prefix function
// made async by the awaiting body of a `run` it lowers, whose asyncness a hot
// module binding's initializer refuses — so a keystroke that stands the
// rewrite down (a context read with no `run`) changes the cold rewrite's LOG:
// the recorded post-rewrite graph must then be rebuilt, not replayed
// (`Plant::ContextLogUnguarded` replays it, and the initializer refusal
// survives a tree that no longer lowers the `run`).

pub const EFFECT_MAIN: &str = "import pkg::effects::{ read, wait };\nimport pkg::views::render;\n\nfun main() {\n\tprint(read());\n\tprint(wait());\n\tprint(render());\n}\n";

/// The context, written with its value type (`Context<i32>::new()`, as kolt
/// writes its own): a context whose value type only a `run` grounds is a
/// use-inferred binding, and a hot module that imports one refuses the
/// hot-set world (S1's early-commitment guard) — which these classes need.
pub const EFFECT_CTX: &str = "import std::context::Context;\n\nexport let mood = Context<i32>::new();\n\nexport fun read_mood(): i32 {\n\tmood.get() + 1\n}\n";

pub const EFFECT_EFFECTS: &str = "import std::time;\nimport pkg::ctx::{ mood, read_mood };\n\nexport fun read(): i32 {\n\tmut seen = 0;\n\tmood.run(5, || {\n\t\tseen = read_mood() + mood.get();\n\t});\n\tseen\n}\n\nexport fun wait(): i32 {\n\ttime::sleep(1);\n\t1\n}\n";

pub const EFFECT_BROWSER_MAIN: &str = "import pkg::effects::{ count, host };\nimport pkg::ticker::ticked;\nimport pkg::views::render;\n\nfun main() {\n\tprint(host());\n\tprint(ticked());\n\tprint(count);\n\tprint(render());\n}\n";

/// A prefix function made async by the awaiting body of a `run` it lowers.
pub const EFFECT_TICKER: &str = "import std::time;\nimport pkg::ctx::mood;\n\nexport fun ticked(): i32 {\n\tmut seen = 0;\n\tmood.run(1, || {\n\t\ttime::sleep(1);\n\t\tseen = mood.get();\n\t});\n\tseen\n}\n";

pub const EFFECT_BROWSER_EFFECTS: &str = "import std::process;\nimport pkg::ctx::mood;\nimport pkg::ticker::ticked;\n\nexport let count = ticked();\n\nexport fun host(): usize {\n\tprocess::args().len()\n}\n";

pub const EFFECT_FIXTURES: &[Fixture] = &[
    Fixture {
        name: "effects",
        platform: NODE,
        files: &[
            ("main.vl", EFFECT_MAIN),
            ("ctx.vl", EFFECT_CTX),
            ("effects.vl", EFFECT_EFFECTS),
            ("views.vl", POST_PASS_VIEWS),
        ],
        edits: &[
            Edit {
                label: "a context read removed from the hot leaf, the need kept by its callee (S3)",
                file: "effects.vl",
                seed: None,
                replacements: &[(
                    "\t\tseen = read_mood() + mood.get();\n",
                    "\t\tseen = read_mood();\n",
                )],
            },
            Edit {
                label: "the hot leaf's last context read removed: the need set must shrink (S3)",
                file: "effects.vl",
                seed: None,
                replacements: &[("\t\tseen = read_mood() + mood.get();\n", "\t\tseen = 0;\n")],
            },
            Edit {
                label: "a sleep removed from the hot leaf: the async set must shrink (S3)",
                file: "effects.vl",
                seed: None,
                replacements: &[("\ttime::sleep(1);\n\t1\n", "\t1\n")],
            },
        ],
        dependencies: &[],
    },
    Fixture {
        name: "effects_browser",
        platform: Platform::Browser,
        files: &[
            ("main.vl", EFFECT_BROWSER_MAIN),
            ("ctx.vl", EFFECT_CTX),
            ("ticker.vl", EFFECT_TICKER),
            ("effects.vl", EFFECT_BROWSER_EFFECTS),
            ("views.vl", POST_PASS_VIEWS),
        ],
        edits: &[
            Edit {
                label: "a node call removed from the hot leaf on a browser entry: the refusal must go (S3)",
                file: "effects.vl",
                seed: None,
                replacements: &[("\tprocess::args().len()\n", "\t1\n")],
            },
            Edit {
                label: "a context read with no run typed into the hot leaf: the rewrite stands down and the graph must follow (S3)",
                file: "effects.vl",
                seed: None,
                replacements: &[(
                    "\tprocess::args().len()\n",
                    "\tlet m = mood.get();\n\tprocess::args().len() + m.as_usize()\n",
                )],
            },
        ],
        dependencies: &[],
    },
];

// --- M159: the LONE open document's own class --------------------------------
//
// A module opened ALONE (the language server's M104 hybrid: file mode, seeded
// with its own path) whose keystroke removes an effect the post passes decide
// over the whole world: a `sleep` that made a function async, whose module
// binding's initializer is refused while it is. A file-mode program emits no
// JS, so the S3b seed's staleness has to show as a diagnostic here — the
// refusal a clean analysis drops once the sleep goes.

pub const LONE_MAIN: &str = "import pkg::lone::{ paused, pause_once };\nimport pkg::views::render;\n\nfun main() {\n\tprint(paused);\n\tprint(pause_once());\n\tprint(render());\n}\n";

pub const LONE_LONE: &str = "import std::time;\nimport pkg::ctx::read_mood;\n\nexport fun pause_once(): i32 {\n\ttime::sleep(1);\n\tread_mood()\n}\n\nexport let paused = pause_once();\n";

pub const LONE_FIXTURES: &[Fixture] = &[Fixture {
    name: "lone",
    platform: NODE,
    files: &[
        ("main.vl", LONE_MAIN),
        ("ctx.vl", EFFECT_CTX),
        ("lone.vl", LONE_LONE),
        ("views.vl", POST_PASS_VIEWS),
    ],
    edits: &[Edit {
        label: "a sleep removed from the lone document: the initializer's refusal must go (M159)",
        file: "lone.vl",
        seed: None,
        replacements: &[("\ttime::sleep(1);\n", "")],
    }],
    dependencies: &[],
}];

// --- M155: R11's instantiations over restored and served candidate rows -------
//
// A leaf whose first function instantiates two of the leaf's OWN generics at
// a resource — one not move-clean (R11 refuses the instantiation), one clean
// — so a keystroke typed into that function re-walks the file and SERVES the
// generics' items: their R11 candidate rows (`analyzer/r11_candidates.rs`)
// come back re-based with their text, and the instantiation's slice must
// still find them. The other edits make the clean generic dirty and the dirty
// one clean.

pub const R11_MAIN: &str = "import pkg::views::render;\n\nfun main() {\n\tprint(render());\n}\n";

pub const R11_DB: &str = "[resource]\nexport struct Db {\n\thandle: i32,\n}\n\nexport fun sink(own d: Db) {\n\tprint(\"{d.handle}\");\n}\n";

pub const R11_VIEWS: &str = "import pkg::db::{ Db, sink };\n\nexport fun render(): i32 {\n\tlet db = Db { handle = 1 };\n\tlet out = use_twice(db);\n\tsink(out);\n\tsink(pass_once(Db { handle = 2 }));\n\t1\n}\n\nfun use_twice<T>(own x: T): T {\n\tlet keep = x;\n\tx\n}\n\nfun pass_once<T>(own x: T): T {\n\tx\n}\n";

pub const R11_FIXTURES: &[Fixture] = &[Fixture {
    name: "r11_rows",
    platform: NODE,
    files: &[
        ("main.vl", R11_MAIN),
        ("db.vl", R11_DB),
        ("views.vl", R11_VIEWS),
    ],
    edits: &[
        Edit {
            label: "a statement typed above the leaf's instantiated generics: their served rows re-based (M155)",
            file: "views.vl",
            seed: None,
            replacements: &[(
                "\t1\n}\n\nfun use_twice",
                "\tlet extra = \"a statement longer than the generic it pushes down, so a served row left at its old offset falls outside the callee\";\n\textra.len() as i32\n}\n\nfun use_twice",
            )],
        },
        Edit {
            label: "the clean generic made dirty: R11 must refuse its instantiation (M155)",
            file: "views.vl",
            seed: None,
            replacements: &[(
                "fun pass_once<T>(own x: T): T {\n\tx\n}",
                "fun pass_once<T>(own x: T): T {\n\tlet keep = x;\n\tx\n}",
            )],
        },
        Edit {
            label: "the dirty generic made clean: its refusal must go (M155)",
            file: "views.vl",
            seed: None,
            replacements: &[(
                "\tlet keep = x;\n\tx\n}\n\nfun pass_once",
                "\tx\n}\n\nfun pass_once",
            )],
        },
    ],
    dependencies: &[],
}];

// --- R11 door B: instance records served, validated and refused (incr-54b, Order 54) -------
//
// A COLD module `shapes.vl` holds the generics the hot leaf instantiates at
// its resource `Conn`: `split<T>` moves a field out of an owned `Pair<T>` (a
// destructure exactly while `T` holds no `Drop` inside — the B469 question
// the record's digest reads), `use_twice<T>` is not move-clean, `pass_once<T>`
// is, and `outer<T>` passes its `T` on to `inner<T>` in `helpers.vl` (a
// PROPAGATED instance, reported at the inner call inside `outer`'s body).
// The first seeded analysis files one record per instance; every later
// keystroke in the leaf serves them. The four edits are the record's four
// validators: (a) a `Drop` impl for `Conn` typed into the leaf — the digest
// moves, `split`'s field move is no destructure, and the instantiation must
// be refused (the plant `R11RecordUnguarded` serves the stale record past
// it); (b) `use_twice`'s argument replaced by a number — the instance is no
// longer seeded, so its recorded refusal must not be spoken; (c) the clean
// generic in the cold module made dirty with the seed on the leaf — its
// module is hot, so it is scanned live; (d) the propagated child made dirty
// in ITS module — `outer`'s record leads to `inner`, whose module is hot.

pub const R11_RECORD_MAIN: &str =
    "import pkg::views::render;\n\nfun main() {\n\tprint(render());\n}\n";

pub const R11_RECORD_SHAPES: &str = "import pkg::helpers::inner;\n\nexport struct Pair<T> {\n\tfirst: T,\n\tsecond: i32,\n}\n\nexport fun split<T>(own pair: Pair<T>): T {\n\tpair.first\n}\n\nexport fun use_twice<T>(own x: T): T {\n\tlet keep = x;\n\tx\n}\n\nexport fun pass_once<T>(own x: T): T {\n\tx\n}\n\nexport fun outer<T>(own x: T): T {\n\tinner(x)\n}\n";

pub const R11_RECORD_HELPERS: &str = "export fun inner<T>(own x: T): T {\n\tx\n}\n";

pub const R11_RECORD_VIEWS: &str = "import pkg::shapes::{ Pair, outer, pass_once, split, use_twice };\nimport std::drop::Drop;\n\n[resource]\nstruct Conn {\n\tid: i32,\n}\n\nfun close(own c: Conn) {\n\tprint(\"{c.id}\");\n}\n\nexport fun render(): i32 {\n\tclose(split(Pair { first = Conn { id = 1 }, second = 2 }));\n\tclose(use_twice(Conn { id = 3 }));\n\tclose(pass_once(Conn { id = 4 }));\n\tclose(outer(Conn { id = 5 }));\n\t1\n}\n";

pub const R11_RECORD_FIXTURES: &[Fixture] = &[Fixture {
    name: "r11_records",
    platform: NODE,
    files: &[
        ("main.vl", R11_RECORD_MAIN),
        ("shapes.vl", R11_RECORD_SHAPES),
        ("helpers.vl", R11_RECORD_HELPERS),
        ("views.vl", R11_RECORD_VIEWS),
    ],
    edits: &[
        Edit {
            label: "a Drop impl for the leaf's resource typed in: the cold generic's field move is no destructure now (door B's digest)",
            file: "views.vl",
            seed: None,
            replacements: &[(
                "\tid: i32,\n}\n\n",
                "\tid: i32,\n}\n\nimpl Conn with Drop {\n\tfun drop(&mut self) {\n\t\tprint(\"closed\");\n\t}\n}\n\n",
            )],
        },
        Edit {
            label: "the refused instantiation's resource argument replaced by a number: its recorded refusal must not be spoken (door B's seeding)",
            file: "views.vl",
            seed: None,
            replacements: &[(
                "\tclose(use_twice(Conn { id = 3 }));\n",
                "\tprint(use_twice(3));\n",
            )],
        },
        Edit {
            label: "the cold module's clean generic made dirty with the seed on the leaf: its module is hot, scanned live (door B's cold validator)",
            file: "shapes.vl",
            seed: Some("views.vl"),
            replacements: &[(
                "export fun pass_once<T>(own x: T): T {\n\tx\n}",
                "export fun pass_once<T>(own x: T): T {\n\tlet keep = x;\n\tx\n}",
            )],
        },
        Edit {
            label: "the propagated child made dirty in its own module: outer's recorded edge leads to a live scan of inner (door B's edges)",
            file: "helpers.vl",
            seed: Some("views.vl"),
            replacements: &[(
                "export fun inner<T>(own x: T): T {\n\tx\n}",
                "export fun inner<T>(own x: T): T {\n\tlet keep = x;\n\tx\n}",
            )],
        },
    ],
    dependencies: &[],
}];

// --- M110 S6: the id-window classes (incr-50, Order 50; spike-49's two predicted reds) ---
//
// Two shapes the spike named as classes to ADD. (1) A std item's resolve
// mints entities (the desugars: `for .. in`, an interpolation) that the
// windows relocate into the std item's window, where `frozen_entity` reads
// them as std's — a definition-site check must not skip a verdict about the
// USER's code because of it. (2) A generated expansion (`[derive]`) gets a
// window of its own; a fixpoint type id relocated into a USER item's window
// beside it must not land in the expansion's `derived_type_origins` range,
// where B217 would re-anchor a diagnostic to the derive attribute. The prefix
// module carries both a derived struct and a bound refusal that names it; the
// leaf and the entry are typed into around them, and the derive is flipped.

pub const WINDOW_MAIN: &str = "import pkg::shapes::{ Pair, refused, same, total };\nimport pkg::views::render;\n\nfun main() {\n\tprint(total([1, 2, 3]));\n\tprint(same(Pair { a = 1, b = 2 }));\n\tprint(refused());\n\tprint(render());\n}\n";

pub const WINDOW_SHAPES: &str = "import std::compare::PartialEq;\n\n[derive(PartialEq)]\nexport struct Pair {\n\ta: i32,\n\tb: i32,\n}\n\nexport fun total(items: List<i32>): i32 {\n\tmut sum = 0;\n\tfor item in items {\n\t\tsum = sum + item;\n\t}\n\tsum\n}\n\nexport fun same(pair: Pair): bool {\n\tpair == Pair { a = pair.b, b = pair.a }\n}\n\nexport fun label(pair: Pair): str {\n\ti\"{pair.a}/{pair.b}\"\n}\n\nfun wants_eq<T: PartialEq>(value: T): bool {\n\tvalue == value\n}\n\nexport fun refused(): bool {\n\twants_eq(total)\n}\n";

pub const WINDOW_FIXTURES: &[Fixture] = &[Fixture {
    name: "id_windows",
    platform: NODE,
    files: &[
        ("main.vl", WINDOW_MAIN),
        ("shapes.vl", WINDOW_SHAPES),
        ("views.vl", POST_PASS_VIEWS),
    ],
    edits: &[
        Edit {
            label: "a statement typed into the leaf beside the derived prefix module (S6)",
            file: "views.vl",
            seed: None,
            replacements: &[("\t1\n", "\tlet extra = 1;\n\textra\n")],
        },
        Edit {
            label: "a statement typed into the entry beside the derived prefix module (S6)",
            file: "main.vl",
            seed: None,
            replacements: &[(
                "\tprint(render());\n",
                "\tprint(render());\n\tlet extra = 1;\n",
            )],
        },
        Edit {
            label: "the prefix module's derive flipped off while the seed stays on the leaf (S6)",
            file: "shapes.vl",
            seed: Some("views.vl"),
            replacements: &[("[derive(PartialEq)]\n", "")],
        },
    ],
    dependencies: &[],
}];

// --- B569: a labelled tuple in a reused module (lang-a-49, Order 49) -------------
//
// A PREFIX module whose declarations print labels — a labelled return, a
// labelled module binding built by name — and whose bodies carry a by-name
// pattern, a named-argument call, and one label refusal the tuple rule records
// in inference (`tuple_literal_label_problems`), with a hot leaf beside it the
// script types into. Its hover rows and declaration labels are recorded per
// module and restored on reuse (S2b); they must render the labels a clean
// analysis renders, and the refusal must survive every keystroke elsewhere.

pub const LABELS_MAIN: &str = "import pkg::shapes::{ bounds, origin, spread };\nimport pkg::views::render;\n\nfun main() {\n\tlet b = bounds();\n\tprint(b.max - b.min);\n\tprint(origin.x);\n\tprint(spread());\n\tprint(render());\n}\n";

pub const LABELS_SHAPES: &str = "export fun bounds(): (min: i32, max: i32) {\n\t(max = 9, min = 1)\n}\n\nexport let origin: (x: f64, y: f64) = (y = 2, x = 1);\n\nfun draw(...at: (x: i32, y: i32)): i32 {\n\tlet (y = down, x = across) = at;\n\tacross * 10 + down\n}\n\nexport fun spread(): i32 {\n\tdraw(y = 4, x = 3)\n}\n\nexport fun refused(): (x: i32, y: i32) {\n\t(x = 1, z = 2)\n}\n";

pub const LABELS_FIXTURE: Fixture = Fixture {
    name: "labelled_tuples",
    platform: NODE,
    files: &[
        ("main.vl", LABELS_MAIN),
        ("shapes.vl", LABELS_SHAPES),
        ("views.vl", POST_PASS_VIEWS),
    ],
    edits: &[
        Edit {
            label: "a statement typed into the leaf beside the prefix module's labelled tuples",
            file: "views.vl",
            seed: None,
            replacements: &[("\t1\n", "\tlet extra = 1;\n\textra\n")],
        },
        Edit {
            label: "a statement typed into the entry beside the prefix module's labelled tuples",
            file: "main.vl",
            seed: None,
            replacements: &[(
                "\tprint(render());\n",
                "\tprint(render());\n\tlet extra = 1;\n",
            )],
        },
    ],
    dependencies: &[],
};

// --- M152: programs with DEPENDENCIES (the std-and-dependencies prefix) ------
//
// A package with a module of its own (`report.vl`, hot over the prefix) and a
// `[library]` dependency with a surface that re-exports a module of its own
// (`shapes::geometry`), so the dependency tier holds a surface walk, a module
// walk and a re-export; and `examples/fullstack`'s two members over its shared
// `common` library, as the example ships them.

pub const DEPENDENCY_MAIN: &str = "import std::io::print;\nimport pkg::report::summary;\n\nfun main() {\n\tprint(summary());\n}\n";

pub const DEPENDENCY_REPORT: &str = "import std::display::Display;\nimport shapes::{ Shape, area, describe };\n\nexport fun summary(): str {\n\tlet total = area(Shape::Square(2.0)) + area(Shape::Rect(1.0, 3.0));\n\tdescribe(Shape::Square(3.0)) + i\" / total {total}\"\n}\n";

pub const DEPENDENCY_SHAPES_LIB: &str = "import std::display::Display;\n\nexport import pkg::geometry::{ Shape, area };\n\nfun describe(shape: Shape): str {\n\ti\"a shape of area {area(shape)}\"\n}\n";

pub const DEPENDENCY_SHAPES_GEOMETRY: &str = "export enum Shape {\n\tSquare(f64),\n\tRect(f64, f64),\n}\n\nexport fun area(shape: Shape): f64 {\n\tmatch shape {\n\t\tShape::Square(let side) => side * side,\n\t\tShape::Rect(let width, let height) => width * height,\n\t}\n}\n\nexport fun perimeter(shape: Shape): f64 {\n\tmatch shape {\n\t\tShape::Square(let side) => 4.0 * side,\n\t\tShape::Rect(let width, let height) => 2.0 * (width + height),\n\t}\n}\n";

pub const DEPENDENCY_SHAPES: &[Dependency] = &[Dependency {
    name: "shapes",
    files: &[
        ("lib.vl", DEPENDENCY_SHAPES_LIB),
        ("geometry.vl", DEPENDENCY_SHAPES_GEOMETRY),
    ],
}];

pub const DEPENDENCY_EDITS: &[Edit] = &[
    Edit {
        label: "a literal typed into the package module that calls the dependency",
        file: "report.vl",
        seed: None,
        replacements: &[("Square(2.0)", "Square(2.5)")],
    },
    Edit {
        label: "the package module reaches a dependency module by its path",
        file: "report.vl",
        seed: None,
        replacements: &[
            (
                "import shapes::{ Shape, area, describe };\n",
                "import shapes::{ Shape, area, describe };\nimport shapes::geometry::perimeter;\n",
            ),
            (
                "\tlet total = area(Shape::Square(2.0))",
                "\tlet total = perimeter(Shape::Square(1.0)) + area(Shape::Square(2.0))",
            ),
        ],
    },
    Edit {
        label: "a type error against a dependency's signature",
        file: "report.vl",
        seed: None,
        replacements: &[("area(Shape::Rect(1.0, 3.0))", "area(\"rect\")")],
    },
    Edit {
        label: "a statement typed into the entry beside the dependency's caller",
        file: "main.vl",
        seed: None,
        replacements: &[(
            "\tprint(summary());\n",
            "\tprint(summary());\n\tprint(\"done\");\n",
        )],
    },
];

pub const DEPENDENCY_FIXTURE: Fixture = Fixture {
    name: "dependency",
    platform: NODE,
    files: &[
        ("main.vl", DEPENDENCY_MAIN),
        ("report.vl", DEPENDENCY_REPORT),
    ],
    edits: DEPENDENCY_EDITS,
    dependencies: DEPENDENCY_SHAPES,
};

pub const FULLSTACK_COMMON: &[Dependency] = &[Dependency {
    name: "common",
    files: &[(
        "lib.vl",
        include_str!("../../../../vilan/examples/fullstack/common/src/lib.vl"),
    )],
}];

pub const FULLSTACK_SERVER_FIXTURE: Fixture = Fixture {
    name: "fullstack_server",
    platform: NODE,
    files: &[(
        "main.vl",
        include_str!("../../../../vilan/examples/fullstack/server/src/main.vl"),
    )],
    edits: &[Edit {
        label: "fullstack's server: a string typed into the entry beside `common::greeting`",
        file: "main.vl",
        seed: None,
        replacements: &[("greeting(\"server\")", "greeting(\"node server\")")],
    }],
    dependencies: FULLSTACK_COMMON,
};

pub const FULLSTACK_CLIENT_FIXTURE: Fixture = Fixture {
    name: "fullstack_client",
    platform: Platform::Browser,
    files: &[(
        "main.vl",
        include_str!("../../../../vilan/examples/fullstack/client/src/main.vl"),
    )],
    edits: &[Edit {
        label: "fullstack's client: a string typed into the entry beside `common::greeting`",
        file: "main.vl",
        seed: None,
        replacements: &[("greeting(\"browser\")", "greeting(\"the browser\")")],
    }],
    dependencies: FULLSTACK_COMMON,
};

/// M152's fixtures, in the order the gates iterate them.
pub const DEPENDENCY_FIXTURES: &[&Fixture] = &[
    &DEPENDENCY_FIXTURE,
    &FULLSTACK_SERVER_FIXTURE,
    &FULLSTACK_CLIENT_FIXTURE,
];
