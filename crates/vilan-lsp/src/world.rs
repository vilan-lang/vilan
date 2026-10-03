//! M104: entry-world analysis — which ENTRY's world an open file is analysed
//! in (the owner's ruling of 2026-10-02).
//!
//! The server used to analyse every open document as its own entry: the file
//! plus whatever it imports. With `model.vl` open beside the three files that
//! import it, one keystroke ran four analyses — `model.vl`'s world, then the
//! dependency sweep's `client.vl`, `channel.vl` and `views.vl`, three worlds
//! two of which sit almost inside the third — and every open file settled
//! only after 5.5 s of CPU on kolt.
//!
//! The ruled design: a file an entry reaches is analysed in THAT ENTRY'S
//! world, once per edit, and every open document of the world is answered from
//! the one analysis. A file shows diagnostics as its entry sees it; hover,
//! goto and completion read the entry's program; a module no entry reaches
//! keeps its own analysis.
//!
//! This module answers the first question — whose world — from the manifest
//! and the package's import graph, the same walk platform colouring takes
//! (`platform_color::file_platform_choices_for`), so the editor and `vilan
//! check` cannot come to two conclusions about which entry reaches a file.
//! The document layer serves a file from a world (`Document::view_of`), and
//! the server schedules, lands and publishes per world.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use vilan_core::Manifest;
use vilan_core::Platform;
use vilan_core::fx::FxHashSet as HashSet;

/// Which worlds a file is analysed in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorldRoots {
    /// The entry whose world serves the file — its diagnostics, and every
    /// caret request. `None` when the file is its OWN entry: a declared entry,
    /// a file no entry reaches, a file outside any `[package]`, or a file
    /// carrying platform-fenced twins (see [`RootResolver::roots`]).
    pub primary: Option<PathBuf>,
    /// The entries of the OTHER platforms whose worlds also load the file — a
    /// module the browser entry and the node entry both reach (E113's shared
    /// module). Each reports the file's diagnostics as its leg compiles it;
    /// none answers caret requests. One per platform, in build order.
    pub further: Vec<PathBuf>,
}

/// One package's entries, and what each reaches — computed at most once per
/// resolver, so a pass that asks about every open file of the package walks
/// each entry's import graph once.
pub struct RootResolver {
    pkg_root: PathBuf,
    /// The entries in BUILD order (browser-class first, stable among
    /// themselves — `platform_color::colored_platform_choices`'s order), each
    /// canonical, with its platform.
    legs: Vec<(PathBuf, Platform)>,
    /// What each leg reaches, filled on first ask.
    reach: Vec<OnceLock<HashSet<PathBuf>>>,
}

impl RootResolver {
    /// The resolver for the package the nearest `vilan.toml` above `file`
    /// declares — `None` when there is none, or it declares no `[package]`
    /// (a `[library]` has no entries, a `[project]` root builds nothing).
    pub fn for_file(file: &Path) -> Option<RootResolver> {
        let mut directory = file.parent();
        let (manifest_dir, contents) = loop {
            let current = directory?;
            let candidate = current.join("vilan.toml");
            if candidate.is_file() {
                break (current, std::fs::read_to_string(&candidate).ok()?);
            }
            directory = current.parent();
        };
        let (manifest, _warnings) = Manifest::parse(&contents).ok()?;
        let package = manifest.package.as_ref()?;
        let pkg_root = manifest_dir.join(package.root());
        let mut legs: Vec<(PathBuf, Platform)> = if manifest.entries.is_empty() {
            // The classic single-entry form: one program, `[package] entry`.
            vec![(
                vilan_core::util::canonical_path(pkg_root.join(package.entry())),
                package.resolved_target().unwrap_or_default(),
            )]
        } else {
            manifest
                .entries
                .iter()
                .map(|(name, entry)| {
                    (
                        vilan_core::util::canonical_path(pkg_root.join(entry.path(name))),
                        entry.resolved_target().unwrap_or_default(),
                    )
                })
                .collect()
        };
        legs.sort_by_key(|(_, platform)| !matches!(platform, Platform::Browser));
        let reach = legs.iter().map(|_| OnceLock::new()).collect();
        Some(RootResolver {
            pkg_root,
            legs,
            reach,
        })
    }

    /// The worlds `file` (whose buffer reads `text`) is analysed in.
    ///
    /// - A declared entry is its own world, and so is a file no entry reaches
    ///   (the ruling's "a module no entry reaches keeps its own analysis").
    /// - Otherwise the FIRST entry in build order whose import graph reaches
    ///   the file serves it, and the first reaching entry of each other
    ///   platform reports its diagnostics too — E113's legs, one per platform.
    /// - A file carrying platform-fenced TWINS keeps its own analysis for now:
    ///   its legs are kept whole per platform (F27 R3), and serving each from
    ///   a different entry's world is the follow-up this order files, not
    ///   builds.
    pub fn roots(&self, file: &Path, text: &str) -> WorldRoots {
        let file = vilan_core::util::canonical_path(file);
        if self.legs.iter().any(|(entry, _)| *entry == file) || has_twins(text) {
            return WorldRoots::default();
        }
        let mut roots = WorldRoots::default();
        let mut platforms: Vec<Platform> = Vec::new();
        for (index, (entry, platform)) in self.legs.iter().enumerate() {
            if platforms.contains(platform) {
                continue;
            }
            if !self.reaches(index).contains(&file) {
                continue;
            }
            platforms.push(*platform);
            match roots.primary {
                None => roots.primary = Some(entry.clone()),
                Some(_) => roots.further.push(entry.clone()),
            }
        }
        roots
    }

    fn reaches(&self, leg: usize) -> &HashSet<PathBuf> {
        self.reach[leg].get_or_init(|| {
            vilan_core::analyzer::package_modules_reachable_from(&self.legs[leg].0, &self.pkg_root)
        })
    }
}

/// Whether `text` declares a platform-fenced twin group. Syntactic, and
/// guarded by a substring test so the common file — no `platform(` anywhere —
/// pays no parse.
fn has_twins(text: &str) -> bool {
    text.contains("platform(") && !vilan_core::platform_color::twin_legs(text, &[]).is_empty()
}

/// The text an analysis of `path` reads: the editor's buffer when the file is
/// open (the overlay the server maintains), else the file on disk.
pub fn current_text(path: &Path) -> Option<String> {
    vilan_core::util::read_source(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "[package]\nname = \"app\"\ndefault-entry = \"server\"\n\n\
         [entry.client]\ntarget = \"browser\"\n\n[entry.server]\n";

    fn package(tag: &str, files: &[(&str, &str)]) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "vilan-m104-roots-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id(),
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(directory.join("src")).expect("a scratch package");
        std::fs::write(directory.join("vilan.toml"), MANIFEST).expect("the manifest");
        for (relative, contents) in files {
            std::fs::write(directory.join("src").join(relative), contents).expect("a source");
        }
        directory
    }

    fn roots_of(directory: &Path, relative: &str) -> WorldRoots {
        let path = directory.join("src").join(relative);
        let text = std::fs::read_to_string(&path).expect("the file");
        RootResolver::for_file(&path)
            .expect("a package")
            .roots(&path, &text)
    }

    fn canonical(directory: &Path, relative: &str) -> PathBuf {
        vilan_core::util::canonical_path(directory.join("src").join(relative))
    }

    /// The ruling's three cases: a module an entry reaches is served from that
    /// entry's world, an entry is its own world, and a module no entry reaches
    /// keeps its own analysis.
    #[test]
    fn a_reached_module_is_served_from_its_entrys_world() {
        let directory = package(
            "reached",
            &[
                (
                    "client.vl",
                    "import pkg::model::count;\n\nfun main() {\n\tcount();\n}\n",
                ),
                ("server.vl", "fun main() {}\n"),
                ("model.vl", "fun count(): i32 {\n\t1\n}\n"),
                ("orphan.vl", "fun nobody(): i32 {\n\t2\n}\n"),
            ],
        );
        assert_eq!(
            roots_of(&directory, "model.vl"),
            WorldRoots {
                primary: Some(canonical(&directory, "client.vl")),
                further: Vec::new(),
            },
        );
        assert_eq!(roots_of(&directory, "client.vl"), WorldRoots::default());
        assert_eq!(roots_of(&directory, "orphan.vl"), WorldRoots::default());
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// A module both legs compile is served from the BROWSER entry's world
    /// (build order, the colour the file reports first) and the node entry's
    /// world reports its diagnostics as a further world — E113's legs.
    #[test]
    fn a_module_both_platforms_reach_has_one_primary_and_one_further_world() {
        let directory = package(
            "shared",
            &[
                (
                    "client.vl",
                    "import pkg::shared::id;\n\nfun main() {\n\tid();\n}\n",
                ),
                (
                    "server.vl",
                    "import pkg::shared::id;\n\nfun main() {\n\tid();\n}\n",
                ),
                ("shared.vl", "fun id(): i32 {\n\t3\n}\n"),
            ],
        );
        assert_eq!(
            roots_of(&directory, "shared.vl"),
            WorldRoots {
                primary: Some(canonical(&directory, "client.vl")),
                further: vec![canonical(&directory, "server.vl")],
            },
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// A module reached only THROUGH another module is still its entry's: the
    /// reach is the import graph's closure, not the entry's own import list.
    #[test]
    fn a_transitively_reached_module_is_its_entrys() {
        let directory = package(
            "transitive",
            &[
                (
                    "client.vl",
                    "import pkg::views::page;\n\nfun main() {\n\tpage();\n}\n",
                ),
                ("server.vl", "fun main() {}\n"),
                (
                    "views.vl",
                    "import pkg::model::count;\n\nfun page(): i32 {\n\tcount()\n}\n",
                ),
                ("model.vl", "fun count(): i32 {\n\t1\n}\n"),
            ],
        );
        assert_eq!(
            roots_of(&directory, "model.vl").primary,
            Some(canonical(&directory, "client.vl")),
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// The view half: a module served from its entry's world answers caret
    /// requests over the ENTRY's program, about its OWN text.
    mod views {
        use super::*;
        use crate::document::Document;
        use crate::document::tests::std_root;

        const CLIENT: &str = "import pkg::model::{ Counter, bump };\n\n\
             fun main() {\n\tlet start = Counter { value = 1 };\n\tlet next = bump(start);\n\tlet _ = next.value;\n}\n";
        const MODEL: &str = "struct Counter {\n\tvalue: i32,\n}\n\n\
             fun bump(counter: Counter): Counter {\n\tlet stepped = counter.value + 1;\n\tCounter { value = stepped }\n}\n";

        fn world(tag: &str, model: &str) -> (PathBuf, Document) {
            let directory = package(
                tag,
                &[
                    ("client.vl", CLIENT),
                    ("server.vl", "fun main() {}\n"),
                    ("model.vl", model),
                ],
            );
            let client = directory.join("src/client.vl");
            let world = Document::analyze(CLIENT, &std_root(), &client);
            (directory, world)
        }

        fn view(directory: &Path, world: &Document, model: &str) -> Option<Document> {
            Document::view_of(
                world,
                &directory.join("src/client.vl"),
                &directory.join("src/model.vl"),
                model,
                Vec::new(),
            )
        }

        #[test]
        fn a_view_answers_hover_and_hints_about_its_own_text() {
            let (directory, world) = world("hover", MODEL);
            let view = view(&directory, &world, MODEL).expect("the client world loads model.vl");
            assert_ne!(view.focus(), vilan_core::analyzer::SourceId(0));
            assert_eq!(
                view.world_root(),
                Some(canonical(&directory, "client.vl").as_path())
            );
            assert_eq!(
                view.entry_path(),
                Some(canonical(&directory, "model.vl").as_path()),
                "the view's own path is the module's, not the entry's",
            );
            let at = MODEL.find("stepped = counter").expect("fixture") + 2;
            let hover = view.hover(at).expect("a hover on a local of the module");
            assert!(
                hover.contains("stepped") && hover.contains("i32"),
                "{hover}"
            );
            let hints: Vec<String> = view
                .inlay_hints()
                .into_iter()
                .map(|(_, label)| label)
                .collect();
            assert!(
                hints.iter().any(|label| label.contains("i32")),
                "the module's own local is hinted from the entry's program: {hints:?}",
            );
            assert!(
                !view.semantic_tokens().is_empty(),
                "the module's tokens come from the entry's program",
            );
            let _ = std::fs::remove_dir_all(&directory);
        }

        #[test]
        fn a_view_publishes_no_diagnostic_of_its_own_and_reads_its_own_from_the_world() {
            let broken = MODEL.replace("counter.value + 1", "counter.value + true");
            let (directory, world) = world("diagnostics", &broken);
            let view = view(&directory, &world, &broken).expect("a view");
            assert!(
                view.published_diagnostics().is_empty(),
                "the world's entry publishes the module's diagnostics; the view does not",
            );
            assert!(
                world.published_diagnostics().iter().any(|item| item
                    .path
                    .as_deref()
                    .is_some_and(|path| path.ends_with("model.vl"))),
                "the entry's analysis attributes the module's error to the module",
            );
            let _ = std::fs::remove_dir_all(&directory);
        }

        /// The overlay is live: a world that read the module at another text
        /// cannot serve it — `view_of` refuses rather than describing bytes
        /// the editor no longer holds.
        #[test]
        fn a_view_over_a_text_the_world_did_not_read_is_refused() {
            let (directory, world) = world("stale", MODEL);
            let edited = MODEL.replace("+ 1", "+ 2");
            assert!(view(&directory, &world, &edited).is_none());
            let _ = std::fs::remove_dir_all(&directory);
        }

        /// A file the world never loaded has no view in it.
        #[test]
        fn a_file_the_world_did_not_load_has_no_view() {
            let (directory, world) = world("unloaded", MODEL);
            let server = directory.join("src/server.vl");
            assert!(
                Document::view_of(
                    &world,
                    &directory.join("src/client.vl"),
                    &server,
                    "fun main() {}\n",
                    Vec::new()
                )
                .is_none()
            );
            let _ = std::fs::remove_dir_all(&directory);
        }
    }
}
