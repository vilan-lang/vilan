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

use std::path::{Path, PathBuf};

use vilan_core::{BuildOptions, PackageSpec, Platform, Workspace, analyze_source, transform};

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
    let directory = std::env::temp_dir().join(format!(
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
/// because nothing was reused would be vacuous).
pub type ReuseObservation = (String, String, Option<String>, (usize, usize, usize));

pub fn observe_in_package(
    pkg_root: &Path,
    entry_path: &Path,
    entry_source: String,
) -> ReuseObservation {
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
