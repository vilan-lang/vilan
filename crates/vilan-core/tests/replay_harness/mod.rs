//! The M19 T1 package fixtures, shared by the two binaries that drive them
//! (`per-module-analysis-reuse.md` §5).
//!
//! T1 widens S1's skip from std — whose diagnostics are known ABSENT — to every
//! module of a base-CACHED world, where they are only REMEMBERED. Nothing but a
//! differential can hold that: replay must equal re-derivation, byte for byte.
//! Both of its gates build the same shape — a package whose `module.vl` a
//! dependent entry imports, analyzed twice so the second analysis hits the
//! world — so the shape is written once, here.
//!
//! The two callers are `check_scope_differential` (the fast pins: one package,
//! one planted diagnostic, the disable switch) and `replay_differential` (the
//! corpus sweep, which is the long one and the reason the two are separate
//! binaries at all — tracker N57).
//!
//! # Driving a REAL package with it (N66)
//!
//! The fixtures below are bare directories — a `module.vl` and an entry, no
//! manifest — so `pkg_root` is simply the directory they are in, and nothing
//! about that generalizes. A real package's `pkg_root` is the directory its
//! manifest's `root` names, **not** the directory the manifest is in: for a
//! package that declares no `root` (kolt, for one) that is `<repo>/src`, the
//! default. Handing the repository root instead does not fail; it analyzes a
//! DIFFERENT, smaller program and says nothing about it — kolt measured that
//! way loaded 39 sources instead of 69 and reported 22 spurious "cannot find …
//! in the imported path" errors, which is a trap for anyone reading the census
//! as a statement about the real package. [`package_root`] resolves the right
//! directory from the manifest, and [`observe_in_package`] refuses the wrong
//! one rather than measuring it.

// Two binaries read this harness — `replay_differential` (the corpus sweep) and
// `check_scope_differential` (the fast T1/T1b pins, which keep their own five-field
// observation helpers) — each a different subset; `support/mod.rs`'s precedent.
#![allow(dead_code)]

use std::path::{Component, Path, PathBuf};

use vilan_core::{
    BuildOptions, EntryMode, PackageSpec, Platform, Workspace, analyze_source, transform,
};

pub fn std_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std")
}

pub fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(&std_root())
}

/// One package: `module.vl` holding `module_source`, and an entry that imports
/// it. Returns the directory (the caller removes it) and the entry path.
pub fn write_module_package(name: &str, module_source: &str) -> (PathBuf, PathBuf) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let directory = crate::scratch::root().join(format!(
        "vilan_m19_t1_{name}_{}_{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("create the package directory");
    std::fs::write(directory.join("module.vl"), module_source).expect("write the module");
    let entry = directory.join("main.vl");
    (directory, entry)
}

/// The entry that makes `module.vl` a LOADED sibling — held fixed except for
/// the digit, which is the keystroke.
pub fn module_entry(revision: u32) -> String {
    format!("import pkg::module;\n\nfun main() {{\n\tlet revision = {revision};\n}}\n")
}

/// Everything the M19 differential compares, plus the census that says whether
/// the run it came from actually reused anything (a differential that agreed
/// because nothing was reused would be vacuous), and whether its world was
/// SERVED from the base cache (N132) — the precondition of any reuse, so the
/// pins can hold every pair that hit to reuse and leave a pair the LRU evicted
/// to the hit-rate floor.
pub type ReuseObservation = (String, String, Option<String>, (usize, usize, usize), bool);

/// The source root a package's manifest declares, resolved against the
/// manifest's own directory — what `pkg_root` has to be for a real package
/// (N66). `None` when `directory` holds no readable `vilan.toml`, which is the
/// fixtures' state and the reason the guard below is silent for them.
///
/// Lexical: the manifest's `root` as written (`src` by default for both
/// `[package]` and `[library]`), joined to the directory. Nothing here looks at
/// the filesystem, so a root that does not exist yet resolves the same way the
/// compiler's own reader resolves it.
pub fn package_root(directory: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(directory.join("vilan.toml")).ok()?;
    let (manifest, _) = vilan_core::manifest::Manifest::parse(&text).ok()?;
    let declared = manifest
        .package
        .as_ref()
        .map(|package| package.root().to_path_buf())
        .or_else(|| {
            manifest.library.as_ref().map(|library| {
                // `[library]`'s `root` has the same default as `[package]`'s
                // and no accessor of its own.
                library.root.clone().unwrap_or_else(|| PathBuf::from("src"))
            })
        })?;
    Some(directory.join(declared))
}

/// Refuses a `pkg_root` that is a package's MANIFEST directory rather than its
/// source root (N66). Called by [`observe_in_package`] and by
/// `check_scope_differential`'s own five-field twin of it, so both entry points
/// that take a `pkg_root` refuse the same argument.
///
/// The wrong one is not an error the analyzer reports — it is a smaller program
/// analyzed quietly — so the harness has to be what says so. The test is
/// lexical and cheap: a `vilan.toml` here whose `root` names a subdirectory
/// means this directory is the manifest's home, and the root it declares is
/// where the sources are. A `root = "."` package names no subdirectory and
/// passes; a directory with no manifest (every fixture below) passes.
pub fn refuse_a_manifest_directory(pkg_root: &Path) {
    let Some(declared) = package_root(pkg_root) else {
        return;
    };
    let names_a_subdirectory = declared
        .strip_prefix(pkg_root)
        .map(|relative| {
            relative
                .components()
                .any(|component| matches!(component, Component::Normal(_)))
        })
        .unwrap_or(false);
    assert!(
        !names_a_subdirectory,
        "{} holds a `vilan.toml` whose `root` is `{}`: analysis wants the SOURCE \
         root, so pass `{}` instead. The manifest directory is not an error the \
         analyzer reports — it loads whatever resolves from there and reports the \
         rest as `cannot find … in the imported path`, which reads as a finding \
         about the package and is a finding about the argument (N66).",
        pkg_root.display(),
        declared
            .strip_prefix(pkg_root)
            .unwrap_or(&declared)
            .display(),
        declared.display()
    );
}

pub fn observe_in_package(
    pkg_root: &Path,
    entry_path: &Path,
    entry_source: String,
) -> ReuseObservation {
    refuse_a_manifest_directory(pkg_root);
    let pkg_root = pkg_root.to_path_buf();
    let entry_path = entry_path.to_path_buf();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(entry_source.into_boxed_str());
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                &pkg_root,
                &entry_path,
                Some(Platform::default()),
                &Workspace::default(),
            );
            let diagnostics = format!("{errors:?}");
            // The FILE each diagnostic and warning publishes to rides in the
            // comparison beside its text: a replayed note carries a `SourceId`
            // INDEX (§3.2), and an index that drifted would show up here and
            // nowhere else.
            let warnings = program
                .as_ref()
                .map(|program| {
                    format!(
                        "{:?}#{:?}#{:?}",
                        program.warnings, program.warning_sources, program.diagnostic_sources
                    )
                })
                .unwrap_or_default();
            let javascript = match program {
                Some(program) if errors.is_empty() => {
                    transform(&program, &BuildOptions::default()).ok()
                }
                _ => None,
            };
            (
                diagnostics,
                warnings,
                javascript,
                vilan_core::analyzer::reuse_census(),
                vilan_core::analyzer::served_from_base_cache(),
            )
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked")
}

/// A warm pair over one package: analysis 1 fills the world and records its
/// modules' checks, analysis 2 hits that world — and, unless reuse is off,
/// replays them. The SECOND observation is the one compared.
pub fn warm_pair(pkg_root: &Path, entry_path: &Path) -> ReuseObservation {
    let _ = observe_in_package(pkg_root, entry_path, module_entry(1));
    observe_in_package(pkg_root, entry_path, module_entry(2))
}

// --- The ENTRY-SHAPED shape (M76) ----------------------------------------
//
// M70 gave the base cache a second SHAPE: a module a front end opened AS the
// entry, whose own package imports it back, so `pkg::<entry>` aliases the
// entry's scope and the world is stored UNRESOLVED. M76 files that shape's
// checks record too, and the only thing that can hold the claim is the same
// differential — replay must equal re-derivation over an entry-shaped world
// exactly as it does over a resolved one.
//
// The fixture is THREE package files, and each is load-bearing:
//
//   opened.vl  the entry, handed to the analysis as a file; imports `ring`
//   ring.vl    imports `pkg::opened` — which is what MAKES the world
//              entry-shaped — and `pkg::probe`
//   probe.vl   carries the Class A diagnostics, and is the module the record
//              is actually read for
//
// `ring.vl` reaches the alias, so M76's narrowing holds it back; `probe.vl`
// does not, so it is reusable. A two-file fixture could not tell those apart
// — the only reusable module would have been std, whose diagnostics are known
// absent anyway, and the replay half would have been vacuous.

/// One entry-shaped package. Returns the directory (the caller removes it) and
/// the OPENED file's path.
pub fn write_open_module_package(name: &str, probe_source: &str) -> (PathBuf, PathBuf) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let directory = crate::scratch::root().join(format!(
        "vilan_m76_open_{name}_{}_{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("create the package directory");
    std::fs::write(directory.join("probe.vl"), probe_source).expect("write the probe module");
    std::fs::write(
        directory.join("ring.vl"),
        "import pkg::opened;\nimport pkg::probe;\n",
    )
    .expect("write the ring module");
    let opened = directory.join("opened.vl");
    // On disk as well as in the buffer: `pkg::opened` has to RESOLVE to a file
    // before the loader can recognize it as the entry and alias it.
    std::fs::write(&opened, open_module_entry(0)).expect("write the opened module");
    (directory, opened)
}

/// The opened file's own text — held fixed except for the digit, which is the
/// keystroke.
pub fn open_module_entry(revision: u32) -> String {
    format!("import pkg::ring;\n\nfun opened_probe(): i32 {{\n\t{revision}\n}}\n")
}

/// The workspace an editor hands over for a file it opened: file mode, no
/// declared programs (so `pkg::opened` is importable — B239).
pub fn open_file_workspace() -> Workspace {
    Workspace {
        entry_mode: EntryMode::OpenFile {
            declared_entries: Vec::new(),
        },
        ..Workspace::default()
    }
}

pub fn observe_open_module(
    pkg_root: &Path,
    entry_path: &Path,
    entry_source: String,
) -> ReuseObservation {
    refuse_a_manifest_directory(pkg_root);
    let pkg_root = pkg_root.to_path_buf();
    let entry_path = entry_path.to_path_buf();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(entry_source.into_boxed_str());
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                &pkg_root,
                &entry_path,
                Some(Platform::default()),
                &open_file_workspace(),
            );
            let diagnostics = format!("{errors:?}");
            let warnings = program
                .as_ref()
                .map(|program| {
                    format!(
                        "{:?}#{:?}#{:?}",
                        program.warnings, program.warning_sources, program.diagnostic_sources
                    )
                })
                .unwrap_or_default();
            let javascript = match program {
                Some(program) if errors.is_empty() => {
                    transform(&program, &BuildOptions::default()).ok()
                }
                _ => None,
            };
            (
                diagnostics,
                warnings,
                javascript,
                vilan_core::analyzer::reuse_census(),
                vilan_core::analyzer::served_from_base_cache(),
            )
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked")
}

/// A warm pair over one entry-shaped package: analysis 1 stores the world and
/// records its modules' checks, analysis 2 hits it and — unless reuse is off —
/// replays them. The SECOND observation is the one compared.
pub fn warm_open_pair(pkg_root: &Path, entry_path: &Path) -> ReuseObservation {
    let _ = observe_open_module(pkg_root, entry_path, open_module_entry(1));
    observe_open_module(pkg_root, entry_path, open_module_entry(2))
}
