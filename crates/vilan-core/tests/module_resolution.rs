//! Filesystem-backed tests for package-module resolution (P1): a `pkg::` module
//! resolves equivalently whether it's a flat `foo.vl` or a directory `foo/lib.vl`,
//! both existing is an ambiguity error, and the `none` platform gates out the
//! platform `std` layers. These need real files on disk (the loader reads them),
//! so each writes a throwaway package directory and analyzes against it.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use vilan_core::manifest::PreludeSpec;
use vilan_core::{
    Error, Layer, MacroLimits, PackageSpec, Platform, PlatformPattern, PreludeRepair, Workspace,
    analyze_source,
};

fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    )
}

/// Writes `files` (relative path → contents) into a fresh temp package directory,
/// analyzes `entry` (also relative) against it as `pkg_root`, and returns the raw
/// diagnostics (message + span). The directory is removed before returning.
fn analyze_package_raw(files: &[(&str, &str)], entry: &str, platform: Platform) -> Vec<Error> {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("vilan_modres_{}_{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (relative, contents) in files {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, contents).unwrap();
    }
    let entry_path = dir.join(entry);
    let source = std::fs::read_to_string(&entry_path).unwrap();
    let leaked: &'static str = Box::leak(source.into_boxed_str());
    let (_program, errors) = analyze_source(
        leaked,
        &std_spec(),
        &dir,
        &entry_path,
        Some(platform),
        &Workspace::default(),
    );
    let _ = std::fs::remove_dir_all(&dir);
    errors
}

/// As [`analyze_package_raw`], but just the diagnostic messages.
fn analyze_package(files: &[(&str, &str)], entry: &str, platform: Platform) -> Vec<String> {
    analyze_package_raw(files, entry, platform)
        .into_iter()
        .map(|error| error.msg)
        .collect()
}

/// As [`analyze_package_raw`], but the package lives ONLY in the open-document
/// overlay — nothing is written to disk, and the root never exists. This is the
/// editor's unsaved-buffer world, and the same world a compiler running without
/// a filesystem sees (D11 S1).
///
/// The root is per-call unique because the overlay is one process-wide map and
/// the test binary runs in parallel: two tests sharing `/overlay/foo.vl` would
/// resolve into each other. Entries are removed before returning for the same
/// reason.
fn analyze_overlay_package(files: &[(&str, &str)], entry: &str, platform: Platform) -> Vec<String> {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = PathBuf::from(format!("/vilan_overlay_{}_{unique}", std::process::id()));
    let paths: Vec<PathBuf> = files
        .iter()
        .map(|(relative, contents)| {
            let path = root.join(relative);
            vilan_core::analyzer::set_document_overlay(&path, Some((*contents).to_string()));
            path
        })
        .collect();
    let entry_path = root.join(entry);
    let source = vilan_core::util::read_source(&entry_path).unwrap();
    let leaked: &'static str = Box::leak(source.into_boxed_str());
    let (_program, errors) = analyze_source(
        leaked,
        &std_spec(),
        &root,
        &entry_path,
        Some(platform),
        &Workspace::default(),
    );
    for path in &paths {
        vilan_core::analyzer::set_document_overlay(path, None);
    }
    errors.into_iter().map(|error| error.msg).collect()
}

const ENTRY: &str = "import std::io::print;\nimport pkg::foo::bar;\nfun main() { print(bar()); }\n";
const MODULE: &str = "fun bar(): i32 { 7 }\n";

/// D11 S1, and the LSP's unsaved-file bug: a module that exists ONLY as an open
/// buffer resolves. Before, `resolve_module_file` asked the disk alone, returned
/// `None` for a file the user had just created and not saved, and the caller
/// skipped the name — so `load_package_module`, the one place that ever read the
/// overlay, was never reached and the import diagnosed as missing.
#[test]
fn a_module_that_exists_only_in_the_overlay_resolves() {
    let errors = analyze_overlay_package(
        &[("main.vl", ENTRY), ("foo.vl", MODULE)],
        "main.vl",
        Platform::default(),
    );
    assert!(
        errors.is_empty(),
        "expected a clean compile with no file on disk, got: {errors:#?}"
    );
}

/// The nested form resolves overlay-only too — `resolve_module_file` probes two
/// candidate paths, and both must ask the overlay or the directory shape stays
/// editor-invisible while the flat one works.
#[test]
fn a_nested_overlay_module_resolves() {
    let errors = analyze_overlay_package(
        &[("main.vl", ENTRY), ("foo/lib.vl", MODULE)],
        "main.vl",
        Platform::default(),
    );
    assert!(
        errors.is_empty(),
        "expected the directory form to resolve from the overlay, got: {errors:#?}"
    );
}

/// Precision: resolution did not become permissive. An import naming a module
/// that is in neither the overlay nor the disk still fails, so the new arm
/// answers "is it buffered", not "yes".
#[test]
fn an_absent_overlay_module_still_fails_to_resolve() {
    let errors = analyze_overlay_package(&[("main.vl", ENTRY)], "main.vl", Platform::default());
    assert!(
        errors.iter().any(|error| error.contains("bar")),
        "expected the missing module to still diagnose, got: {errors:#?}"
    );
}

/// The overlay is the file's CURRENT truth, so it outranks a stale disk copy —
/// the E6 rule, pinned here at the resolution layer rather than the LSP's.
/// The disk says `bar` returns 1; the buffer says 7. A clean compile that also
/// sees the buffer's value proves the overlay won, not merely that something
/// resolved.
#[test]
fn the_overlay_outranks_the_file_on_disk() {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "vilan_overlay_disk_{}_{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.vl"), ENTRY).unwrap();
    std::fs::write(dir.join("foo.vl"), "fun bar(): i32 { 1 }\n").unwrap();

    let buffered = dir.join("foo.vl");
    vilan_core::analyzer::set_document_overlay(&buffered, Some(MODULE.to_string()));
    let read_back = vilan_core::util::read_source(&buffered).unwrap();
    vilan_core::analyzer::set_document_overlay(&buffered, None);
    let from_disk = vilan_core::util::read_source(&buffered).unwrap();
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(read_back, MODULE, "the overlay must win over the disk copy");
    assert_eq!(
        from_disk, "fun bar(): i32 { 1 }\n",
        "clearing the overlay must restore disk truth"
    );
}

/// The BOM asymmetry, which the seam move could silently have erased: disk text
/// is BOM-stripped so spans index the source proper, and buffered text is
/// returned exactly as the client sent it (the client's line index is
/// authoritative, and VS Code already strips the BOM over the wire). Stripping
/// a buffer again here would shift every span in it by three bytes.
#[test]
fn a_buffer_keeps_its_byte_order_mark_while_a_disk_read_drops_one() {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("vilan_overlay_bom_{}_{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bom.vl");
    std::fs::write(&path, "\u{feff}fun bar(): i32 { 7 }\n").unwrap();

    let from_disk = vilan_core::util::read_source(&path).unwrap();
    vilan_core::analyzer::set_document_overlay(
        &path,
        Some("\u{feff}fun bar(): i32 { 7 }\n".to_string()),
    );
    let buffered = vilan_core::util::read_source(&path).unwrap();
    vilan_core::analyzer::set_document_overlay(&path, None);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        !from_disk.starts_with('\u{feff}'),
        "a disk read must drop the BOM"
    );
    assert!(
        buffered.starts_with('\u{feff}'),
        "a buffer must be returned verbatim, BOM included"
    );
}

#[test]
fn flat_module_resolves() {
    let errors = analyze_package(
        &[("main.vl", ENTRY), ("foo.vl", MODULE)],
        "main.vl",
        Platform::default(),
    );
    assert!(
        errors.is_empty(),
        "expected a clean compile, got: {errors:#?}"
    );
}

#[test]
fn lib_module_resolves() {
    // The directory form `foo/lib.vl` resolves identically to the flat `foo.vl`.
    let errors = analyze_package(
        &[("main.vl", ENTRY), ("foo/lib.vl", MODULE)],
        "main.vl",
        Platform::default(),
    );
    assert!(
        errors.is_empty(),
        "expected a clean compile, got: {errors:#?}"
    );
}

#[test]
fn both_forms_is_ambiguous() {
    let errors = analyze_package(
        &[
            ("main.vl", ENTRY),
            ("foo.vl", MODULE),
            ("foo/lib.vl", MODULE),
        ],
        "main.vl",
        Platform::default(),
    );
    assert!(
        errors.iter().any(|error| error.contains("ambiguous")),
        "expected an ambiguity error, got: {errors:#?}"
    );
}

#[test]
fn none_platform_rejects_reaching_platform_std() {
    // A `none` (pure-library) platform admits only base-layer code. Importing a
    // process-layer module is fine (coloring: imports are not the checkpoint);
    // REACHING it from the entry is the violation.
    let import_only = "import std::http;\nfun main() {}\n";
    let errors = analyze_package(&[("main.vl", import_only)], "main.vl", Platform::None);
    assert!(errors.is_empty(), "import alone is legal: {errors:#?}");

    let reaching = "import std::http::Server;\nfun main() { Server::builder(); }\n";
    let errors = analyze_package(&[("main.vl", reaching)], "main.vl", Platform::None);
    assert!(
        errors
            .iter()
            .any(|error| error.contains("requires the `process` layer of `std`")),
        "expected a platform-coloring violation, got: {errors:#?}"
    );
}

#[test]
fn none_platform_allows_base_std() {
    // Base std (e.g. `print`) is universal — a `none` platform still type-checks it.
    let entry = "import std::io::print;\nfun main() { print(1); }\n";
    let errors = analyze_package(&[("main.vl", entry)], "main.vl", Platform::None);
    assert!(
        errors.is_empty(),
        "expected a clean compile, got: {errors:#?}"
    );
}

// --- Multi-package workspaces (P2) -----------------------------------------

/// A dependency package for [`analyze_workspace`]: how the entry imports it
/// (`import_name`) and its files (relative path → contents, including a `lib.vl`).
struct Dep {
    import_name: &'static str,
    files: &'static [(&'static str, &'static str)],
}

/// Analyzes an entry program against a set of dependency packages (P2). The entry
/// lives in its own `app/` directory; each dependency in `<import_name>/`. Builds
/// the `Workspace` (entry depends on every dep, each a base-only pure library) and
/// returns the diagnostics. Dependencies are not interdependent here (the loader's
/// transitive edges are exercised through `lib.vl` seeding within a dep).
fn analyze_workspace(entry: &str, deps: &[Dep], platform: Platform) -> Vec<String> {
    analyze_workspace_files(&[("main.vl", entry)], deps, platform)
}

/// As [`analyze_workspace`], but the entry package may have sibling module files
/// (relative path → contents); `main.vl` among them is the entry.
fn analyze_workspace_files(
    entry_files: &[(&str, &str)],
    deps: &[Dep],
    platform: Platform,
) -> Vec<String> {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("vilan_ws_{}_{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);

    let app_dir = root.join("app");
    std::fs::create_dir_all(&app_dir).unwrap();
    for (relative, contents) in entry_files {
        let path = app_dir.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, contents).unwrap();
    }
    let entry_path = app_dir.join("main.vl");

    let mut packages = Vec::new();
    let mut entry_dependencies = Vec::new();
    for (index, dep) in deps.iter().enumerate() {
        let dep_root = root.join(dep.import_name);
        for (relative, contents) in dep.files {
            let path = dep_root.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, contents).unwrap();
        }
        packages.push(PackageSpec {
            base_root: dep_root,
            layers: Vec::new(),
            dependencies: Vec::new(),
            surface: true,
            member: false,
            prelude: Default::default(),
        });
        entry_dependencies.push((dep.import_name.to_string(), index));
    }
    let workspace = Workspace {
        packages,
        entry_dependencies,
        macro_limits: MacroLimits::default(),
        entry_prelude: Default::default(),
        ..Workspace::default()
    };

    let source = std::fs::read_to_string(&entry_path).unwrap();
    let leaked: &'static str = Box::leak(source.into_boxed_str());
    let (_program, errors) = analyze_source(
        leaked,
        &std_spec(),
        &app_dir,
        &entry_path,
        Some(platform),
        &workspace,
    );
    let _ = std::fs::remove_dir_all(&root);
    errors.into_iter().map(|error| error.msg).collect()
}

#[test]
fn cross_package_import_resolves() {
    let entry =
        "import std::io::print;\nimport common::greeting;\nfun main() { print(greeting()); }\n";
    let common = Dep {
        import_name: "common",
        files: &[("lib.vl", "fun greeting(): str { \"hi\" }\n")],
    };
    let errors = analyze_workspace(entry, &[common], Platform::default());
    assert!(
        errors.is_empty(),
        "expected a clean compile, got: {errors:#?}"
    );
}

#[test]
fn cross_package_submodule_resolves() {
    // `common::shape::area` descends into a submodule of the dependency, whose own
    // `pkg::` self-reference (from `lib.vl`) stays within `common`.
    let entry =
        "import std::io::print;\nimport common::shape::area;\nfun main() { print(area(2)); }\n";
    let common = Dep {
        import_name: "common",
        files: &[
            ("lib.vl", "import pkg::shape::area;\n"),
            ("shape.vl", "fun area(side: i32): i32 { side * side }\n"),
        ],
    };
    let errors = analyze_workspace(entry, &[common], Platform::default());
    assert!(
        errors.is_empty(),
        "expected a clean compile, got: {errors:#?}"
    );
}

#[test]
fn dependency_pkg_self_reference_is_isolated() {
    // The dependency's `pkg::helper` must resolve to ITS OWN `helper`, not the
    // entry's same-named module. The entry also has a `helper` with a different
    // signature; if `pkg::` leaked across packages, one side would mistype.
    let entry = concat!(
        "import std::io::print;\n",
        "import pkg::helper::entry_value;\n",
        "import common::greeting;\n",
        "fun main() { print(entry_value()); print(greeting()); }\n",
    );
    let common = Dep {
        import_name: "common",
        files: &[
            (
                "lib.vl",
                "import pkg::helper::dep_value;\nfun greeting(): i32 { dep_value() }\n",
            ),
            ("helper.vl", "fun dep_value(): i32 { 1 }\n"),
        ],
    };
    // The entry's own `pkg::helper` sibling lives next to the entry.
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("vilan_wsiso_{}_{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let app_dir = root.join("app");
    std::fs::create_dir_all(&app_dir).unwrap();
    std::fs::write(app_dir.join("main.vl"), entry).unwrap();
    std::fs::write(app_dir.join("helper.vl"), "fun entry_value(): i32 { 9 }\n").unwrap();
    let dep_root = root.join("common");
    for (relative, contents) in common.files {
        let path = dep_root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, contents).unwrap();
    }
    let workspace = Workspace {
        packages: vec![PackageSpec {
            base_root: dep_root,
            layers: Vec::new(),
            dependencies: Vec::new(),
            surface: true,
            member: false,
            prelude: Default::default(),
        }],
        entry_dependencies: vec![("common".to_string(), 0)],
        macro_limits: MacroLimits::default(),
        entry_prelude: Default::default(),
        ..Workspace::default()
    };
    let entry_path = app_dir.join("main.vl");
    let source = std::fs::read_to_string(&entry_path).unwrap();
    let leaked: &'static str = Box::leak(source.into_boxed_str());
    let (_program, errors) = analyze_source(
        leaked,
        &std_spec(),
        &app_dir,
        &entry_path,
        Some(Platform::default()),
        &workspace,
    );
    let _ = std::fs::remove_dir_all(&root);
    let errors: Vec<String> = errors.into_iter().map(|error| error.msg).collect();
    assert!(
        errors.is_empty(),
        "expected a clean compile, got: {errors:#?}"
    );
}

#[test]
fn a_workspace_dependency_edge_named_std_is_refused() {
    // E88 (defensive since L12): a manifest declaring a dependency named `std`
    // is refused up front, but a programmatically built `Workspace` (wasm,
    // embedders, this very harness) could still stage one — and the analyzer
    // bound it OVER the standard library (`resolve_import_root` checks
    // dependency edges first) while the IDE's completion answered the stdlib:
    // one name, two resolvers, two answers. The `analyze` funnel now reports
    // the edge and drops it, so `std::` keeps meaning the standard library
    // for every resolver.
    let entry = "import std::io::print;\nfun main() { print(\"hi\") }\n";
    let imposter = Dep {
        import_name: "std",
        files: &[("lib.vl", "fun shadow(): str { \"shadow\" }\n")],
    };
    let errors = analyze_workspace(entry, &[imposter], Platform::default());
    assert!(
        errors
            .iter()
            .any(|error| error.contains("`std` is a reserved package name")),
        "expected the reserved-name refusal, got: {errors:#?}"
    );
    // Dropped, not fatal: `std::io::print` still resolves to the real standard
    // library, so the refusal is the only diagnostic.
    assert_eq!(
        errors.len(),
        1,
        "`std::io::print` must keep resolving to the standard library: {errors:#?}"
    );
}

#[test]
fn a_workspace_dependency_edge_named_pkg_is_refused() {
    // The other root the world itself owns. Unlike `std`, a `pkg` edge was
    // never consulted (`resolve_import_root` answers the importing package's
    // own namespace first), so pre-refusal the declaration was silently dead —
    // now it is loud, matching the manifest layer's L12 refusal.
    let entry = "fun main() {}\n";
    let imposter = Dep {
        import_name: "pkg",
        files: &[("lib.vl", "fun shadow(): str { \"shadow\" }\n")],
    };
    let errors = analyze_workspace(entry, &[imposter], Platform::default());
    assert_eq!(
        errors.len(),
        1,
        "expected exactly the reserved-name refusal, got: {errors:#?}"
    );
    assert!(
        errors[0].contains("`pkg` is a reserved package name"),
        "expected the reserved-name refusal, got: {errors:#?}"
    );
}

#[test]
fn unknown_dependency_name_errors() {
    // The entry imports a package it doesn't declare — resolution finds no such
    // root and reports it (rather than silently resolving against another package).
    let entry = "import other::thing;\nfun main() {}\n";
    let common = Dep {
        import_name: "common",
        files: &[("lib.vl", "fun greeting(): str { \"hi\" }\n")],
    };
    let errors = analyze_workspace(entry, &[common], Platform::default());
    assert!(
        !errors.is_empty(),
        "expected an unresolved-import error for `other`"
    );
}

// --- Cross-platform import error recovery (P3) -----------------------------

#[test]
fn cross_platform_std_import_does_not_cascade() {
    // A browser build of a Node program: the two cross-platform imports are reported,
    // but `std::http`/`std::fs` still load for typing, so `Server`,
    // `read_file_to_str`, etc. resolve and there's no unresolved-name cascade.
    let entry = concat!(
        "import std::http::{ Server, Response };\n",
        "import std::fs::read_file_to_str;\n",
        "fun main() {\n",
        "    let data = read_file_to_str(\"x.txt\");\n",
        "    let server = Server::builder().port(3000).build();\n",
        "    server.start();\n",
        "}\n",
    );
    let errors = analyze_package(&[("main.vl", entry)], "main.vl", Platform::Browser);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("`read_file_to_str` requires the `process` layer of `std`")),
        "missing the fs boundary violation: {errors:#?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("requires the `process` layer of `std`")
                && e.contains("main → builder")),
        "missing the http boundary violation: {errors:#?}"
    );
    assert!(
        !errors.iter().any(|e| e.contains("cannot find")),
        "expected no cascade, got: {errors:#?}"
    );
}

#[test]
fn cross_platform_diagnostic_is_spanned() {
    // The violation anchors at the user CALL SITE (not EMPTY_SPAN at 0..0, and
    // not the import — proposal/platform-coloring.md §3.6).
    let entry = "import std::http::Server;\nfun main() { Server::builder(); }\n";
    let errors = analyze_package_raw(&[("main.vl", entry)], "main.vl", Platform::Browser);
    let http = errors
        .iter()
        .find(|e| e.msg.contains("requires the `process` layer of `std`"))
        .expect("a platform-coloring violation");
    let range = http.span.into_range();
    assert!(
        range.start < range.end && range.start > 0,
        "expected a real call span, got {range:?}"
    );
}

#[test]
fn cross_platform_transitive_import_not_reported() {
    // Importing `std::http` reports `http` once; the modules it pulls in
    // transitively (std-internal) load but are not separately gated.
    let entry = "import std::http::Server;\nfun main() { Server::builder(); }\n";
    let errors = analyze_package(&[("main.vl", entry)], "main.vl", Platform::Browser);
    let violations = errors
        .iter()
        .filter(|e| e.contains("cannot run on"))
        .count();
    assert_eq!(
        violations, 1,
        "one violation at the boundary, not one per function inside the layer: {errors:#?}"
    );
    assert!(errors.iter().any(|e| e.contains("main → builder")));
}

#[test]
fn platform_modules_load_for_typing_under_opposite_platform() {
    // Loading a cross-platform std module purely to type-check it must not introduce
    // spurious errors beyond the single cross-platform diagnostic (P3 Q5 sweep).
    for (module, platform) in [
        ("http", Platform::Browser),
        ("fs", Platform::Browser),
        ("process", Platform::Browser),
        ("dom", Platform::default()),
        ("ui", Platform::default()),
    ] {
        let entry = format!("import std::{module};\nfun main() {{}}\n");
        let errors = analyze_package(&[("main.vl", &entry)], "main.vl", platform);
        assert!(
            errors.is_empty(),
            "`std::{module}` under {platform:?}: importing without reaching is legal \
             (elision), and loading-for-typing introduces no errors: {errors:#?}"
        );
    }
}

// --- Library platform layers (L1) ------------------------------------------

/// Sets up a library `plat` with layers — a base module `shared`, a `process`-layer
/// `nodeonly`, and a `clock` present in both layers (process returns `i32`, browser
/// `str`) — and an empty base `lib.vl`. Analyzes `entry` (which imports from `plat`)
/// for `platform`, returning the diagnostics.
fn analyze_layered(entry: &str, platform: Platform) -> Vec<String> {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("vilan_layer_{}_{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let app = root.join("app");
    std::fs::create_dir_all(&app).unwrap();
    let entry_path = app.join("main.vl");
    std::fs::write(&entry_path, entry).unwrap();

    let plat = root.join("plat");
    let put = |rel: &str, contents: &str| {
        let path = plat.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    };
    put("src/lib.vl", "");
    put("src/shared.vl", "fun shared(): i32 { 0 }\n");
    put("src/process/nodeonly.vl", "fun nodeonly(): i32 { 1 }\n");
    put("src/process/clock.vl", "fun clock(): i32 { 1 }\n");
    put("src/browser/clock.vl", "fun clock(): str { \"x\" }\n");

    let workspace = Workspace {
        packages: vec![PackageSpec {
            base_root: plat.join("src"),
            layers: vec![
                Layer {
                    name: "process".to_string(),
                    // The `@process` family (node + deno), like real `std`.
                    patterns: PlatformPattern::parse("@process").unwrap(),
                    root: plat.join("src/process"),
                },
                Layer {
                    name: "browser".to_string(),
                    patterns: vec![PlatformPattern::Browser],
                    root: plat.join("src/browser"),
                },
            ],
            dependencies: Vec::new(),
            surface: true,
            member: false,
            prelude: Default::default(),
        }],
        entry_dependencies: vec![("plat".to_string(), 0)],
        macro_limits: MacroLimits::default(),
        entry_prelude: Default::default(),
        ..Workspace::default()
    };
    let source = std::fs::read_to_string(&entry_path).unwrap();
    let leaked: &'static str = Box::leak(source.into_boxed_str());
    let (_program, errors) = analyze_source(
        leaked,
        &std_spec(),
        &app,
        &entry_path,
        Some(platform),
        &workspace,
    );
    let _ = std::fs::remove_dir_all(&root);
    errors.into_iter().map(|error| error.msg).collect()
}

#[test]
fn base_module_available_for_all_platforms() {
    let entry = "import plat::shared::shared;\nfun main() { shared() }\n";
    assert!(analyze_layered(entry, Platform::default()).is_empty());
    assert!(analyze_layered(entry, Platform::Browser).is_empty());
}

#[test]
fn layer_module_available_only_for_its_platform() {
    let entry = "import plat::nodeonly::nodeonly;\nfun main() { nodeonly() }\n";
    assert!(
        analyze_layered(entry, Platform::default()).is_empty(),
        "the process-layer module is available for a node build"
    );
    let browser = analyze_layered(entry, Platform::Browser);
    assert!(
        browser
            .iter()
            .any(|e| e.contains("requires the `process` layer of `plat`")
                && e.contains("cannot run on `browser`")),
        "expected a platform-coloring violation for browser, got: {browser:#?}"
    );
    assert!(
        !browser.iter().any(|e| e.contains("cannot find")),
        "the module still loads for typing (no cascade): {browser:#?}"
    );
}

#[test]
fn varying_module_resolves_the_platform_version() {
    // `clock` is `i32` in the process layer, `str` in the browser layer. Passing it
    // to an `i32` parameter type-checks for node and fails for browser — proving the
    // build platform's version loaded (the P4 case, structurally).
    let entry = concat!(
        "import plat::clock::clock;\n",
        "fun need_int(n: i32) {}\n",
        "fun main() { need_int(clock()) }\n",
    );
    assert!(
        analyze_layered(entry, Platform::default()).is_empty(),
        "node `clock` is i32"
    );
    assert!(
        !analyze_layered(entry, Platform::Browser).is_empty(),
        "browser `clock` is str — a type mismatch, proving the browser version loaded"
    );
}

#[test]
fn base_lib_reexporting_a_layer_module_errors() {
    // A library whose base `lib.vl` re-exports `nodeonly` (a process-layer module):
    // the public surface must be platform-agnostic, so this is a Q4 violation.
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("vilan_q4_{}_{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let app = root.join("app");
    std::fs::create_dir_all(&app).unwrap();
    let entry_path = app.join("main.vl");
    std::fs::write(
        &entry_path,
        "import plat::shared::shared;\nfun main() { shared() }\n",
    )
    .unwrap();
    let plat = root.join("plat");
    let put = |rel: &str, contents: &str| {
        let path = plat.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    };
    put("src/lib.vl", "export import pkg::nodeonly::nodeonly;\n");
    put("src/shared.vl", "fun shared(): i32 { 0 }\n");
    put("src/process/nodeonly.vl", "fun nodeonly(): i32 { 1 }\n");
    let workspace = Workspace {
        packages: vec![PackageSpec {
            base_root: plat.join("src"),
            layers: vec![Layer {
                name: "process".to_string(),
                patterns: vec![PlatformPattern::Node { version: None }],
                root: plat.join("src/process"),
            }],
            dependencies: Vec::new(),
            surface: true,
            member: false,
            prelude: Default::default(),
        }],
        entry_dependencies: vec![("plat".to_string(), 0)],
        macro_limits: MacroLimits::default(),
        entry_prelude: Default::default(),
        ..Workspace::default()
    };
    let source = std::fs::read_to_string(&entry_path).unwrap();
    let leaked: &'static str = Box::leak(source.into_boxed_str());
    let (_program, errors) = analyze_source(
        leaked,
        &std_spec(),
        &app,
        &entry_path,
        Some(Platform::default()),
        &workspace,
    );
    let _ = std::fs::remove_dir_all(&root);
    let errors: Vec<String> = errors.into_iter().map(|error| error.msg).collect();
    assert!(
        errors
            .iter()
            .any(|e| e.contains("re-exports") && e.contains("nodeonly")),
        "expected a base-lib re-export error, got: {errors:#?}"
    );
}

// --- Deno joins `@process` (a second process runtime) ----------------------

/// The current `deno` platform (parsed so a test needn't name the version).
fn deno() -> Platform {
    Platform::parse("deno").expect("deno is a supported platform")
}

#[test]
fn process_layer_std_is_reachable_for_deno() {
    // `std::http` lives in the `process` layer (serves `@process`). Deno is in
    // `@process`, so the import resolves with no cross-platform error — its
    // `node:`-compat bindings are portable across the family (proposal §5).
    let entry = "import std::http::Server;\nfun main() { Server::builder(); }\n";
    let errors = analyze_package(&[("main.vl", entry)], "main.vl", deno());
    assert!(
        errors.is_empty(),
        "std::http should be reachable for deno: {errors:#?}"
    );
}

#[test]
fn browser_layer_std_is_cross_platform_for_deno() {
    // The browser layer doesn't serve deno: reaching a browser-layer function
    // from a deno build is a coloring violation (pattern matching, not names).
    let entry = "import std::router::navigate;\nfun main() { navigate(\"/x\"); }\n";
    let errors = analyze_package(&[("main.vl", entry)], "main.vl", deno());
    assert!(
        errors
            .iter()
            .any(|e| e.contains("requires the `browser` layer of `std`")),
        "reaching the browser layer should violate for deno: {errors:#?}"
    );
}

#[test]
fn layered_process_module_serves_deno() {
    // The `plat` fixture's `process` layer declares `@process`, so `nodeonly` is
    // available for a deno build and `clock` resolves to the process version (i32),
    // exactly as for node — one layer, the whole family.
    assert!(
        analyze_layered(
            "import plat::nodeonly::nodeonly;\nfun main() { nodeonly() }\n",
            deno()
        )
        .is_empty(),
        "the process-layer module should be available for deno"
    );
    let clock = concat!(
        "import plat::clock::clock;\n",
        "fun need_int(n: i32) {}\n",
        "fun main() { need_int(clock()) }\n",
    );
    assert!(
        analyze_layered(clock, deno()).is_empty(),
        "deno resolves the process `clock` (i32), like node"
    );
}

// --- Platform contract check (§4.2 completeness) ----------------------------

/// Writes a library tree under a fresh temp dir — `base` files in `src/`, `process`
/// files in `src/process` (a layer serving `@process`), `browser` files in
/// `src/browser` — then runs the structural platform contract check over it and
/// returns the violation messages.
fn contract_violations(
    base: &[(&str, &str)],
    process: &[(&str, &str)],
    browser: &[(&str, &str)],
) -> Vec<String> {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("vilan_contract_{}_{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let put = |dir: &std::path::Path, files: &[(&str, &str)]| {
        std::fs::create_dir_all(dir).unwrap();
        for (name, contents) in files {
            std::fs::write(dir.join(name), contents).unwrap();
        }
    };
    let src = root.join("src");
    put(&src, base);
    put(&src.join("process"), process);
    put(&src.join("browser"), browser);
    let spec = PackageSpec {
        base_root: src.clone(),
        layers: vec![
            Layer {
                name: "process".to_string(),
                patterns: PlatformPattern::parse("@process").unwrap(),
                root: src.join("process"),
            },
            Layer {
                name: "browser".to_string(),
                patterns: vec![PlatformPattern::Browser],
                root: src.join("browser"),
            },
        ],
        dependencies: Vec::new(),
        surface: true,
        member: false,
        prelude: Default::default(),
    };
    let violations = vilan_core::analyzer::check_library_contract(&spec)
        .into_iter()
        .map(|error| error.msg)
        .collect();
    let _ = std::fs::remove_dir_all(&root);
    violations
}

#[test]
fn a_library_contract_check_keeps_the_position_in_prose() {
    // E100's other half. `vilan check <library>` walks a package's modules
    // without registering any of them as a source and renders MESSAGES only, so
    // there is no file for a span to index into — the file and the position stay
    // in the text, which is exactly what every module parse error used to do.
    let violations = contract_violations(
        &[("lib.vl", ""), ("broken.vl", "fun broken( {\n")],
        &[],
        &[],
    );
    assert!(
        violations.iter().any(|violation| {
            violation.contains("parse error in")
                && violation.contains("broken.vl")
                && violation.contains("line 1, column 11")
        }),
        "{violations:?}"
    );
}

#[test]
fn contract_ok_when_each_module_stays_within_its_served_set() {
    // A base module importing a base sibling (available everywhere) and a process
    // module importing a process sibling (available across `@process`) — both within
    // the platforms their own layer serves.
    let violations = contract_violations(
        &[
            ("lib.vl", ""),
            ("util.vl", "fun util(): i32 { 1 }\n"),
            (
                "core.vl",
                "import pkg::util::util;\nfun core(): i32 { util() }\n",
            ),
        ],
        &[
            ("feature.vl", "fun feature(): i32 { 1 }\n"),
            (
                "service.vl",
                "import pkg::feature::feature;\nfun service(): i32 { feature() }\n",
            ),
        ],
        &[],
    );
    assert!(
        violations.is_empty(),
        "expected no contract violations, got: {violations:#?}"
    );
}

#[test]
fn contract_flags_base_module_reaching_into_a_layer() {
    // A base module serves every host, so importing a process-only module breaks the
    // contract for the platforms the process layer doesn't serve (the browser).
    let violations = contract_violations(
        &[
            ("lib.vl", ""),
            (
                "core.vl",
                "import pkg::feature::feature;\nfun core(): i32 { feature() }\n",
            ),
        ],
        &[("feature.vl", "fun feature(): i32 { 1 }\n")],
        &[],
    );
    assert!(
        violations
            .iter()
            .any(|m| m.contains("core") && m.contains("feature") && m.contains("browser")),
        "expected a completeness violation naming `browser`, got: {violations:#?}"
    );
}

#[test]
fn contract_flags_process_module_reaching_into_the_browser_layer() {
    // A process module serves `@process` (node/deno/bun), so importing a browser-only
    // module isn't available for any of them — a violation, even though neither
    // module is in the base.
    let violations = contract_violations(
        &[("lib.vl", "")],
        &[(
            "service.vl",
            "import pkg::widget::widget;\nfun service(): i32 { widget() }\n",
        )],
        &[("widget.vl", "fun widget(): i32 { 1 }\n")],
    );
    assert!(
        violations
            .iter()
            .any(|m| m.contains("service") && m.contains("widget")),
        "expected a violation for the process→browser import, got: {violations:#?}"
    );
}

#[test]
fn contract_ignores_item_reexports_and_typos() {
    // `pkg::helper` here names an item re-exported through resolution, not a module
    // file — the contract check leaves it to ordinary name resolution.
    let violations = contract_violations(
        &[("lib.vl", "export import pkg::missing::thing;\n")],
        &[],
        &[],
    );
    assert!(
        violations.is_empty(),
        "a non-module `pkg::` ref isn't a contract concern: {violations:#?}"
    );
}

// --- Derives in imported modules (bug #1) -----------------------------------

#[test]
fn derive_in_an_imported_module_resolves() {
    // `[derive(Json)]` in an imported `pkg::` module synthesizes `to_json`/`from_json`
    // there, visible to the importer — derive expansion is no longer entry-file-only.
    let entry = concat!(
        "import std::json::{ Json, FromJson };\n",
        "import std::result::Result::{ self, Ok, Err };\n",
        "import pkg::contract::User;\n",
        "fun main() {\n",
        "    let user = User { id = 1, name = \"Ada\" };\n",
        "    let back: Result<User, str> = User::from_json(user.to_json());\n",
        "    back is Ok(let u) && u.name == \"Ada\"\n",
        "}\n",
    );
    let contract = "[derive(Json)]\nstruct User {\n    id: i32,\n    name: str,\n}\n";
    let errors = analyze_package(
        &[("main.vl", entry), ("contract.vl", contract)],
        "main.vl",
        Platform::default(),
    );
    assert!(
        errors.is_empty(),
        "a derived type from an imported module should round-trip, got: {errors:#?}"
    );
}

#[test]
fn derive_in_a_dependency_library_resolves() {
    // The contract-library pattern: a `[derive(Json)]` type in a dependency library's
    // `lib.vl`, used by the app — the derive expands in the dependency too.
    let entry = concat!(
        "import std::json::{ Json, FromJson };\n",
        "import std::result::Result::{ self, Ok, Err };\n",
        "import common::User;\n",
        "fun main() {\n",
        "    let user = User { id = 1, name = \"Ada\" };\n",
        "    let back: Result<User, str> = User::from_json(user.to_json());\n",
        "    back is Ok(let u) && u.name == \"Ada\"\n",
        "}\n",
    );
    let common = Dep {
        import_name: "common",
        files: &[(
            "lib.vl",
            "[derive(Json)]\nstruct User {\n    id: i32,\n    name: str,\n}\n",
        )],
    };
    let errors = analyze_workspace(entry, &[common], Platform::default());
    assert!(
        errors.is_empty(),
        "a derived type from a dependency library should round-trip, got: {errors:#?}"
    );
}

// --- Diagnostic source attribution (backlog E1) --------------------------------

/// As [`analyze_package_raw`], but returns `(message, source-file name, note's
/// source-file name)` triples — the attribution the LSP publishes by. The note's
/// file is `None` when the note carries no source of its own, which means "the
/// diagnostic's own file".
fn analyze_package_attributed(
    files: &[(&str, &str)],
    entry: &str,
    platform: Platform,
) -> Vec<(String, String, Option<String>)> {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("vilan_attr_{}_{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (relative, contents) in files {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, contents).unwrap();
    }
    let entry_path = dir.join(entry);
    let source = std::fs::read_to_string(&entry_path).unwrap();
    let leaked: &'static str = Box::leak(source.into_boxed_str());
    let (program, _errors) = analyze_source(
        leaked,
        &std_spec(),
        &dir,
        &entry_path,
        Some(platform),
        &Workspace::default(),
    );
    let program = program.expect("analysis should produce a program");
    let attributed = program
        .diagnostics
        .iter()
        .zip(program.diagnostic_sources.iter())
        .map(|(error, source)| {
            let file_of = |source| {
                program
                    .source_path(source)
                    .and_then(|path| path.file_name())
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "<none>".to_string())
            };
            let note_file = error
                .note
                .as_ref()
                .and_then(|note| note.source)
                .map(file_of);
            (error.msg.clone(), file_of(*source), note_file)
        })
        .collect();
    let _ = std::fs::remove_dir_all(&dir);
    attributed
}

/// Like [`analyze_package_attributed`], but keeping each diagnostic's SPAN as
/// `(message, file, start..end)`. A module diagnostic's position is only
/// meaningful together with the file it indexes into, so the two are read
/// together (E100).
fn analyze_package_spanned(
    files: &[(&str, &str)],
    entry: &str,
    platform: Platform,
) -> Vec<(String, String, std::ops::Range<usize>)> {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("vilan_span_{}_{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (relative, contents) in files {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, contents).unwrap();
    }
    let entry_path = dir.join(entry);
    let source = std::fs::read_to_string(&entry_path).unwrap();
    let leaked: &'static str = Box::leak(source.into_boxed_str());
    let (program, _errors) = analyze_source(
        leaked,
        &std_spec(),
        &dir,
        &entry_path,
        Some(platform),
        &Workspace::default(),
    );
    let program = program.expect("analysis should produce a program");
    let spanned = program
        .diagnostics
        .iter()
        .zip(program.diagnostic_sources.iter())
        .map(|(error, source)| {
            let file = program
                .source_path(*source)
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "<none>".to_string());
            (error.msg.clone(), file, error.span.into_range())
        })
        .collect();
    let _ = std::fs::remove_dir_all(&dir);
    spanned
}

/// The 1-based line an offset falls on — what a reader sees, and the whole
/// point of a span the loader kept.
fn line_of(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].matches('\n').count() + 1
}

// A type error INSIDE an imported module is attributed to that module's file,
// not the entry — the root cause of the LSP's vanishing-diagnostics bug (the
// error was mapped through the entry's line index and disappeared).
#[test]
fn a_type_error_in_an_imported_module_is_attributed_to_that_module() {
    let attributed = analyze_package_attributed(
        &[
            (
                "main.vl",
                "import std::io::print;\nimport pkg::broken::answer;\nfun main() { print(answer()); }\n",
            ),
            ("broken.vl", "fun answer(): i32 {\n\t\"not a number\"\n}\n"),
        ],
        "main.vl",
        Platform::default(),
    );
    let mismatch = attributed
        .iter()
        .find(|(msg, ..)| msg.contains("Expected i32"))
        .expect("the return mismatch should be reported");
    assert_eq!(
        mismatch.1, "broken.vl",
        "the error belongs to the module that contains it: {attributed:?}"
    );
}

// An unresolved name inside a module attributes there; an unresolved name in
// the entry attributes to the entry — side by side in one program.
#[test]
fn name_errors_attribute_to_their_own_files() {
    let attributed = analyze_package_attributed(
        &[
            (
                "main.vl",
                "import pkg::helper::greet;\nfun main() {\n\tgreet();\n\tmissing_in_entry();\n}\n",
            ),
            ("helper.vl", "fun greet() {\n\tmissing_in_helper();\n}\n"),
        ],
        "main.vl",
        Platform::default(),
    );
    let helper_error = attributed
        .iter()
        .find(|(msg, ..)| msg.contains("missing_in_helper"))
        .expect("the helper's name error should be reported");
    assert_eq!(helper_error.1, "helper.vl", "{attributed:?}");
    let entry_error = attributed
        .iter()
        .find(|(msg, ..)| msg.contains("missing_in_entry"))
        .expect("the entry's name error should be reported");
    assert_eq!(entry_error.1, "main.vl", "{attributed:?}");
}

// A module that fails to PARSE attributes its parse diagnostics to its own
// file, so the editor can surface them there.
#[test]
fn module_parse_errors_attribute_to_the_broken_module() {
    let attributed = analyze_package_attributed(
        &[
            (
                "main.vl",
                "import pkg::util::util;\nfun main() { let _ = util(); }\n",
            ),
            ("util.vl", "fun util(): i32 { 1 }\nfun broken( {\n"),
        ],
        "main.vl",
        Platform::default(),
    );
    let parse_error = attributed
        .iter()
        .find(|(msg, ..)| msg.contains("expected a matching `)`"))
        .expect("the module parse error should be reported");
    assert_eq!(parse_error.1, "util.vl", "{attributed:?}");
}

// --- E100: a module-load parse error carries its own span -------------------
//
// The loader rendered `line N, column M` into the message and pushed an EMPTY
// span, so the same syntax error that anchors at its true position in the ENTRY
// file anchored at 1:1 in an imported one — with the position readable only as
// prose. The measured cost was 798 errors from one bad generator character, all
// at line 1 of an 18k-line generated file and out of source order. The span now
// rides all the way from the parser, and the module's `SourceId` (already
// attributed) is what gives it a file.

#[test]
fn a_module_parse_error_anchors_at_its_true_position() {
    let helper = "fun a(): i32 { 1 }\nfun b(): i32 { 2 }\nfun broken( {\nfun c(): i32 { 3 }\n";
    let spanned = analyze_package_spanned(
        &[
            (
                "main.vl",
                "import pkg::util::a;\nfun main() { let _ = a(); }\n",
            ),
            ("util.vl", helper),
        ],
        "main.vl",
        Platform::default(),
    );
    let (message, file, range) = spanned
        .iter()
        .find(|(message, ..)| message.contains("expected a matching `)`"))
        .expect("the module parse error should be reported");
    assert_eq!(file, "util.vl", "{spanned:?}");
    assert_ne!(*range, 0..0, "an empty span is the bug: {spanned:?}");
    assert_eq!(
        line_of(helper, range.start),
        3,
        "the error is on line 3 of the module: {message} at {range:?}"
    );
    assert_eq!(
        &helper[range.clone()],
        "(",
        "the span covers the unclosed `(`: {spanned:?}"
    );
}

#[test]
fn several_parse_errors_in_one_module_keep_distinct_spans() {
    // The 798-error shape in miniature: a generated module holds many, and each
    // needs its own place. They arrive in source order, which is what
    // `normalize_diagnostic_order` sorts by once a span exists to sort on.
    let helper = "fun a(): i32 { 1 }\nfun b(): i32 { 2 @ }\nfun c(): i32 { 3 }\n                  fun d(): i32 { 4 # }\nfun e(): i32 { 5 }\n";
    let spanned = analyze_package_spanned(
        &[
            (
                "main.vl",
                "import pkg::util::a;\nfun main() { let _ = a(); }\n",
            ),
            ("util.vl", helper),
        ],
        "main.vl",
        Platform::default(),
    );
    let parse_errors: Vec<_> = spanned
        .iter()
        .filter(|(message, ..)| message.contains("is not a vilan token"))
        .collect();
    assert_eq!(parse_errors.len(), 2, "{spanned:?}");
    let lines: Vec<usize> = parse_errors
        .iter()
        .map(|(_, _, range)| line_of(helper, range.start))
        .collect();
    assert_eq!(lines, vec![2, 4], "{spanned:?}");
    assert!(
        parse_errors[0].2.start < parse_errors[1].2.start,
        "source order, not hash order: {spanned:?}"
    );
    assert!(
        parse_errors.iter().all(|(_, file, _)| file == "util.vl"),
        "{spanned:?}"
    );
}

#[test]
fn a_module_parse_error_is_the_parsers_own_error_unaltered() {
    // One mechanism, however the error flows out of the loader. The entry file
    // builds its diagnostic straight from the parser — `error.span`, and
    // `parsing::render(error)` as the message — and a loaded module now does
    // exactly the same, so the two are comparable against the same source of
    // truth rather than against each other.
    let broken = "fun util(): i32 { 1 }\nfun broken( {\n";
    let (_tree, parse_errors) = vilan_core::parsing::parse(broken);
    assert_eq!(parse_errors.len(), 1, "the fixture holds one parse error");
    let expected_message = vilan_core::parsing::render(&parse_errors[0]);
    let expected_span = parse_errors[0].span.into_range();

    let spanned = analyze_package_spanned(
        &[
            (
                "main.vl",
                "import pkg::util::util;\nfun main() { let _ = util(); }\n",
            ),
            ("util.vl", broken),
        ],
        "main.vl",
        Platform::default(),
    );
    let module_error = spanned
        .iter()
        .find(|(message, ..)| *message == expected_message)
        .unwrap_or_else(|| panic!("expected {expected_message:?} verbatim; got {spanned:?}"));
    assert_eq!(module_error.1, "util.vl", "{spanned:?}");
    assert_eq!(module_error.2, expected_span, "{spanned:?}");
}

// --- E102: the seen set keys on the REASON as well as the place ---------------
//
// E100's dedup exists because one module reaches the loader through several
// seams and the cache hands each seam the same errors. It keyed `(path, span)`,
// where the code it replaced keyed the full rendered message — so two DISTINCT
// errors at one offset collapsed into one and the second was never reported.
// A stray `"` is exactly that shape: the LEXER refuses the unterminated string
// and the PARSER, over the token stream that survives, refuses the statement
// that has no terminator, both spanning the same one character.

#[test]
fn two_distinct_module_errors_at_one_offset_are_both_reported() {
    let helper = "fun answer(): i32 { 1 }\"\nfun other(): i32 { 2 }\n";
    // The fixture's premise, checked against the parser itself rather than
    // assumed: two errors, distinct reasons, one shared span.
    let (_tree, parse_errors) = vilan_core::parsing::parse(helper);
    assert_eq!(parse_errors.len(), 2, "the fixture holds two parse errors");
    assert_eq!(
        parse_errors[0].span.into_range(),
        parse_errors[1].span.into_range(),
        "the two errors share one span, which is the whole point"
    );
    assert_ne!(
        vilan_core::parsing::render(&parse_errors[0]),
        vilan_core::parsing::render(&parse_errors[1]),
        "and they are distinct errors"
    );

    let spanned = analyze_package_spanned(
        &[
            (
                "main.vl",
                "import pkg::util::answer;\nfun main() { let _ = answer(); }\n",
            ),
            ("util.vl", helper),
        ],
        "main.vl",
        Platform::default(),
    );
    let at_the_offset: Vec<_> = spanned
        .iter()
        .filter(|(_, file, range)| file == "util.vl" && *range == (23..24))
        .collect();
    assert_eq!(
        at_the_offset.len(),
        2,
        "both errors at the offset survive the dedup: {spanned:?}"
    );
    assert!(
        at_the_offset
            .iter()
            .any(|(message, ..)| message.contains("a string cannot span lines")),
        "the lexer's refusal: {spanned:?}"
    );
    assert!(
        at_the_offset
            .iter()
            .any(|(message, ..)| message.contains("expected `;` to end this statement")),
        "the parser's refusal: {spanned:?}"
    );
}

#[test]
fn the_same_module_error_reached_through_two_seams_is_reported_once() {
    // The other half of the same key: widening it must not undo E100's dedup.
    // `util.vl` reaches the loader twice — once directly and once through the
    // re-export in `bridge.vl` — and its errors are still reported once each.
    let helper = "fun answer(): i32 { 1 }\"\nfun other(): i32 { 2 }\n";
    let spanned = analyze_package_spanned(
        &[
            (
                "main.vl",
                "import pkg::util::answer;\nimport pkg::bridge::relay;\n\
                 fun main() { let _ = answer(); let _ = relay(); }\n",
            ),
            ("util.vl", helper),
            (
                "bridge.vl",
                "import pkg::util::other;\nfun relay(): i32 { other() }\n",
            ),
        ],
        "main.vl",
        Platform::default(),
    );
    let at_the_offset: Vec<_> = spanned
        .iter()
        .filter(|(_, file, range)| file == "util.vl" && *range == (23..24))
        .collect();
    assert_eq!(
        at_the_offset.len(),
        2,
        "two distinct errors, each once — not four: {spanned:?}"
    );
}

// --- E82: the post-fixpoint (`finalize_build`) checks attribute like every
// other check. That pass ran after the per-constraint attribution wrap with
// `current_source_id` parked at the entry, so ALL of its diagnostics claimed
// the entry file while their spans indexed the module that wrote the code —
// the exact harm E1/E16 exist to stop, one pass over. One pin per probed
// shape: the derive refusal (the generated-code form, re-anchored at the
// attribute in the DERIVING file), the plain operator refusal, the non-bool
// condition, the uniterable for-each, and the out-of-range literal.

#[test]
fn a_derive_refusal_in_a_module_is_attributed_to_the_deriving_module() {
    let attributed = analyze_package_attributed(
        &[
            (
                "main.vl",
                "import std::io::print;\nimport pkg::page::{ Widget, Opaque };\n\
                 fun main() {\n\tlet w = Widget { item = Opaque { x = 1 } };\n\tprint(w.item.x);\n}\n",
            ),
            (
                "page.vl",
                "[derive(PartialEq)]\nstruct Widget { item: Opaque }\n\nstruct Opaque { x: i32 }\n",
            ),
        ],
        "main.vl",
        Platform::default(),
    );
    let refusal = attributed
        .iter()
        .find(|(msg, ..)| msg.contains("does not implement the `PartialEq` operator"))
        .expect("the derive refusal should be reported");
    assert!(
        refusal.0.contains("in code generated by this attribute:"),
        "{attributed:?}"
    );
    assert_eq!(refusal.1, "page.vl", "{attributed:?}");
}

#[test]
fn an_operator_refusal_in_a_module_is_attributed_to_that_module() {
    let attributed = analyze_package_attributed(
        &[
            (
                "main.vl",
                "import std::io::print;\nimport pkg::helper::check;\nfun main() { print(check()); }\n",
            ),
            (
                "helper.vl",
                "struct Pair { a: i32 }\n\nfun check(): bool {\n\
                 \tlet left = Pair { a = 1 };\n\tlet right = Pair { a = 2 };\n\
                 \tleft == right\n}\n",
            ),
        ],
        "main.vl",
        Platform::default(),
    );
    let refusal = attributed
        .iter()
        .find(|(msg, ..)| msg.contains("does not implement the `PartialEq` operator"))
        .expect("the operator refusal should be reported");
    assert_eq!(refusal.1, "helper.vl", "{attributed:?}");
}

#[test]
fn a_condition_error_in_a_module_is_attributed_to_that_module() {
    let attributed = analyze_package_attributed(
        &[
            (
                "main.vl",
                "import std::io::print;\nimport pkg::helper::check;\nfun main() { print(check()); }\n",
            ),
            (
                "helper.vl",
                "fun check(): bool {\n\tlet n = 3;\n\tif n {\n\t\tret true;\n\t}\n\tfalse\n}\n",
            ),
        ],
        "main.vl",
        Platform::default(),
    );
    let condition = attributed
        .iter()
        .find(|(msg, ..)| msg.contains("but a condition must be `bool`"))
        .expect("the condition error should be reported");
    assert_eq!(condition.1, "helper.vl", "{attributed:?}");
}

#[test]
fn an_uniterable_for_each_in_a_module_is_attributed_to_that_module() {
    let attributed = analyze_package_attributed(
        &[
            (
                "main.vl",
                "import pkg::helper::run;\nfun main() { run(); }\n",
            ),
            (
                "helper.vl",
                "import std::io::print;\n\nstruct Cursor { items: List<i32> }\n\n\
                 fun run() {\n\tlet cursor = Cursor { items = [1] };\n\
                 \tfor item in cursor {\n\t\tprint(item);\n\t}\n}\n",
            ),
        ],
        "main.vl",
        Platform::default(),
    );
    let uniterable = attributed
        .iter()
        .find(|(msg, ..)| msg.contains("cannot iterate `Cursor`"))
        .expect("the uniterable for-each should be reported");
    assert_eq!(uniterable.1, "helper.vl", "{attributed:?}");
}

#[test]
fn an_out_of_range_literal_in_a_module_is_attributed_to_that_module() {
    let attributed = analyze_package_attributed(
        &[
            (
                "main.vl",
                "import std::io::print;\nimport pkg::helper::level;\nfun main() { print(level()); }\n",
            ),
            (
                "helper.vl",
                "fun level(): i8 {\n\tlet n: i8 = 200;\n\tn\n}\n",
            ),
        ],
        "main.vl",
        Platform::default(),
    );
    let out_of_range = attributed
        .iter()
        .find(|(msg, ..)| msg.contains("out of range for `i8`"))
        .expect("the literal range error should be reported");
    assert_eq!(out_of_range.1, "helper.vl", "{attributed:?}");
}

// --- E.10: resolution is scoped by the import root -----------------------------
// A local module may share a std module's name: `pkg::` resolves only the entry
// package's modules and `std::` only std's, so the two never collide. (Before the
// fix, std and entry modules registered into one shared `pkg` namespace — last
// writer won, and the loser's items became unreachable.)

#[test]
fn local_module_sharing_a_std_name_resolves_for_both_roots() {
    // `json` is one of std's always-loaded core modules — the strongest collision:
    // std's `json` registers whether or not the program imports it.
    let entry = "import std::io::print;\nimport std::json::encode_json;\nimport pkg::json::stamp;\n\
                 \nfun main() { print(stamp()); print(encode_json(7)); }\n";
    let errors = analyze_package(
        &[
            ("main.vl", entry),
            ("json.vl", "fun stamp(): str { \"local\" }\n"),
        ],
        "main.vl",
        Platform::default(),
    );
    assert!(
        errors.is_empty(),
        "expected a clean compile, got: {errors:#?}"
    );
}

#[test]
fn local_module_sharing_a_layered_std_name_resolves_for_both_roots() {
    // The original E.10 report: a local `ui.vl` alongside `std::ui` (which lives
    // in std's browser layer), both imported by the same program.
    let entry = "import std::ui::view;\nimport pkg::ui::screen;\n\nfun main() { screen(); }\n";
    let errors = analyze_package(
        &[
            ("main.vl", entry),
            ("ui.vl", "fun screen(): str { \"local\" }\n"),
        ],
        "main.vl",
        Platform::Browser,
    );
    assert!(
        errors.is_empty(),
        "expected a clean compile, got: {errors:#?}"
    );
}

#[test]
fn local_module_sharing_a_primitive_hosts_name_keeps_the_captures() {
    // `string.vl` hosts the `str` primitive. A local module of the same name must
    // not displace it in the analyzer's capture map — `"abc".len()` still types.
    let entry = "import std::io::print;\nimport pkg::string::shout;\n\
                 \nfun main() { print(shout()); print(\"abc\".len()); }\n";
    let errors = analyze_package(
        &[
            ("main.vl", entry),
            ("string.vl", "fun shout(): str { \"loud\" }\n"),
        ],
        "main.vl",
        Platform::default(),
    );
    assert!(
        errors.is_empty(),
        "expected a clean compile, got: {errors:#?}"
    );
}

#[test]
fn local_io_module_does_not_displace_std_io() {
    // `io.vl` hosts `print`/`panic`; the entry's own `io.vl` must not shadow it.
    let entry = "import std::io::print;\nimport pkg::io::log_line;\n\
                 \nfun main() { print(log_line(\"x\")); }\n";
    let errors = analyze_package(
        &[
            ("main.vl", entry),
            ("io.vl", "fun log_line(message: str): str { message }\n"),
        ],
        "main.vl",
        Platform::default(),
    );
    assert!(
        errors.is_empty(),
        "expected a clean compile, got: {errors:#?}"
    );
}

#[test]
fn pkg_root_does_not_alias_std_modules() {
    // The flip side of root-scoped resolution: `pkg::` reaches only the entry
    // package's own modules. A std module with no local counterpart is not
    // addressable through it (it used to be, as an accident of the shared map).
    let entry = "import pkg::time::now;\n\nfun main() { }\n";
    let errors = analyze_package(&[("main.vl", entry)], "main.vl", Platform::default());
    assert!(
        errors
            .iter()
            .any(|msg| msg == "cannot find 'time' in the imported path"),
        "expected the root-scoped miss, got: {errors:#?}"
    );
}

#[test]
fn workspace_entry_local_module_sharing_a_std_name_resolves() {
    // The with-dependencies path: the entry is `packages[0]`, std is a later
    // package — the collision must resolve identically, alongside a dep import.
    let entry = "import std::io::print;\nimport std::json::encode_json;\n\
                 import pkg::json::stamp;\nimport common::greeting;\n\
                 \nfun main() { print(stamp()); print(encode_json(7)); print(greeting()); }\n";
    let common = Dep {
        import_name: "common",
        files: &[("lib.vl", "fun greeting(): str { \"hi\" }\n")],
    };
    let errors = analyze_workspace_files(
        &[
            ("main.vl", entry),
            ("json.vl", "fun stamp(): str { \"local\" }\n"),
        ],
        &[common],
        Platform::default(),
    );
    assert!(
        errors.is_empty(),
        "expected a clean compile, got: {errors:#?}"
    );
}

#[test]
fn compiling_a_std_file_directly_resolves_its_pkg_imports() {
    // A std file opened as the entry (`compiling_std`, e.g. from an editor): its
    // `pkg::` imports are std's own siblings and must resolve within the std
    // namespace — the entry source maps to the std package.
    let std_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std/src");
    let entry_path = std_root.join("time.vl");
    let source = std::fs::read_to_string(&entry_path).unwrap();
    let leaked: &'static str = Box::leak(source.into_boxed_str());
    let (_program, errors) = analyze_source(
        leaked,
        &std_spec(),
        &std_root,
        &entry_path,
        Some(Platform::default()),
        &Workspace::default(),
    );
    assert!(
        errors.is_empty(),
        "expected a clean std-self compile, got: {errors:#?}"
    );
}

/// A dependency namespace's display name must be interned, not re-leaked per
/// analysis: the LSP leak harness's fixtures are dependency-free (its
/// `display` term is vacuously zero there), so the plateau is pinned here,
/// where a dependency actually exists (the E23 sweep's coverage gap). The
/// first analysis must also RECORD the leak — if the site stops firing, this
/// pin has gone vacuous and says so.
#[test]
fn dependency_display_names_intern_across_analyses() {
    use vilan_core::leak_tally::{self, LeakSite};

    let entry =
        "import std::io::print;\nimport internpin::greeting;\nfun main() { print(greeting()); }\n";
    let dep = Dep {
        import_name: "internpin",
        files: &[("lib.vl", "fun greeting(): str { \"hi\" }\n")],
    };
    leak_tally::reset();
    let errors = analyze_workspace(entry, &[dep], Platform::default());
    assert!(
        errors.is_empty(),
        "expected a clean compile, got: {errors:#?}"
    );
    assert!(
        leak_tally::bytes(LeakSite::DisplayName) > 0,
        "the first analysis of a dependency workspace recorded no display-name \
         leak — the site moved and this pin is vacuous"
    );
    let dep = Dep {
        import_name: "internpin",
        files: &[("lib.vl", "fun greeting(): str { \"hi\" }\n")],
    };
    leak_tally::reset();
    let errors = analyze_workspace(entry, &[dep], Platform::default());
    assert!(
        errors.is_empty(),
        "expected a clean compile, got: {errors:#?}"
    );
    assert_eq!(
        leak_tally::bytes(LeakSite::DisplayName),
        0,
        "re-analyzing the same dependency workspace re-leaked its namespace \
         display name — the intern is not deduping"
    );
}

/// The Rust-fallback derive path — a derive with NO macro in scope, the case
/// fixture stds and macro-world compiles hit — must parse its generated impls
/// through the content cache: an unchanged program's re-analysis reuses the
/// tree instead of re-leaking one. Pinned here rather than in the LSP leak
/// harness because it needs a std WITHOUT the std macros, and this file
/// already owns the hand-built-spec fixtures (the E23 sweep's coverage gap).
#[test]
fn rust_fallback_derives_parse_through_the_content_cache() {
    use vilan_core::leak_tally::{self, LeakSite};

    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("vilan_fallback_{}_{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    // A std whose loader-forced modules are empty stubs and which defines no
    // `macro fun` anywhere: `[derive(PartialEq)]` finds no macro in scope and
    // falls back to the Rust generators. (The generated prelude's imports then
    // fail to resolve against the stubs — irrelevant here: the leak this pins
    // happens at the parse, before resolution.)
    let std_src = root.join("std").join("src");
    std::fs::create_dir_all(&std_src).unwrap();
    for module in [
        "boolean", "list", "null", "promise", "compare", "default", "debug", "json", "hash",
    ] {
        std::fs::write(std_src.join(format!("{module}.vl")), "").unwrap();
    }
    let fixture_std = PackageSpec {
        base_root: std_src,
        layers: Vec::new(),
        dependencies: Vec::new(),
        surface: true,
        member: false,
        prelude: Default::default(),
    };
    let app_dir = root.join("app");
    std::fs::create_dir_all(&app_dir).unwrap();
    let entry_path = app_dir.join("main.vl");
    let source = "[derive(PartialEq)]\nstruct FallbackPin { x: i32 }\n";
    std::fs::write(&entry_path, source).unwrap();
    let leaked: &'static str = Box::leak(source.to_string().into_boxed_str());

    let analyze = || {
        let (_program, _errors) = analyze_source(
            leaked,
            &fixture_std,
            &app_dir,
            &entry_path,
            Some(Platform::default()),
            &Workspace::default(),
        );
    };
    leak_tally::reset();
    analyze();
    assert!(
        leak_tally::bytes(LeakSite::MacroParseText) > 0,
        "the fixture never took the Rust-fallback path — no generated text was \
         parsed, and this pin is vacuous"
    );
    leak_tally::reset();
    analyze();
    let releaked =
        leak_tally::bytes(LeakSite::MacroParseText) + leak_tally::bytes(LeakSite::MacroParseAst);
    assert_eq!(
        releaked, 0,
        "the Rust-fallback derive re-leaked {releaked} B on an unchanged \
         re-analysis — `flush_rust_fallback` is not parsing through the \
         content cache"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// M7 (`leak-soak.md` §7): `analyze_source_reclaimable` is `analyze_source`
/// with the entry tree's handle attached — the handle recorded exactly what
/// the `EntryAst` site shows, and reclaiming it once the program is dropped
/// nets the site to zero while the gross record stands. The plain
/// `analyze_source` keeps the leak: nothing is released.
#[test]
fn the_reclaimable_entry_analysis_hands_back_the_tree_it_leaked() {
    use vilan_core::leak_tally::{self, LeakSite};

    let source: &'static str = "import std::io::print;\n\nfun main() {\n\tprint(\"reclaim\");\n}\n";
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let std = std_spec();
            leak_tally::reset();
            let vilan_core::AnalyzedEntry {
                program,
                diagnostics,
                ast,
                owned_modules,
            } = vilan_core::analyze_source_reclaimable(
                source,
                &std,
                Path::new("."),
                Path::new("reclaim_probe.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            assert!(
                diagnostics.is_empty(),
                "expected a clean compile, got: {diagnostics:#?}"
            );
            let program = program.expect("a clean compile yields a program");
            let ast = ast.expect("a parsed entry yields its tree handle");
            assert!(
                owned_modules.is_empty(),
                "`analyze_source_reclaimable` did not opt in to owned overlay \
                 modules, so it must own none"
            );
            let recorded = leak_tally::bytes(LeakSite::EntryAst);
            assert!(
                recorded > 0,
                "no entry tree was recorded — the pin is vacuous"
            );
            assert_eq!(
                ast.bytes(),
                recorded,
                "the handle must carry exactly what the site recorded, so a reclaim nets to zero"
            );
            assert_eq!(ast.site(), LeakSite::EntryAst);
            assert_eq!(
                leak_tally::outstanding(LeakSite::EntryAst),
                recorded as isize
            );
            drop(program);
            // SAFETY: the program — the tree's only borrower — was dropped on
            // the line above; this thread holds no other reference into it.
            unsafe { ast.reclaim() };
            assert_eq!(leak_tally::released(LeakSite::EntryAst), recorded);
            assert_eq!(leak_tally::outstanding(LeakSite::EntryAst), 0);
            assert_eq!(
                leak_tally::bytes(LeakSite::EntryAst),
                recorded,
                "the gross record stands"
            );

            // The wrapper keeps the leak.
            leak_tally::reset();
            let (_program, _errors) = analyze_source(
                source,
                &std,
                Path::new("."),
                Path::new("reclaim_probe.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            assert!(leak_tally::bytes(LeakSite::EntryAst) > 0);
            assert_eq!(
                leak_tally::released(LeakSite::EntryAst),
                0,
                "`analyze_source` must leave its tree leaked — the macro world's nested compile \
                 and every other caller rely on that"
            );
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked");
}

/// A macro world's blanked entry is analysed through the same pipeline, and its
/// tree used to land at `EntryAst` beside the real entry's. It records at
/// `MacroWorldAst` now (bounded by `WORLDS`, like the world's text and program),
/// so `EntryAst` is exactly the top-level entry's tree: a macro-defining entry
/// records the SAME `EntryAst` bytes cold (world compiled) and warm (world
/// cached), and the world's tree shows up once, at its own site.
#[test]
fn a_macro_worlds_tree_records_at_its_own_site_not_the_entrys() {
    use vilan_core::leak_tally::{self, LeakSite};

    let source: &'static str = "import std::io::print;\n\n\
        macro fun twice(arguments: Arguments): Source {\n\
        \timport macro_std::source;\n\
        \timport macro_std::meta::{ Arguments, Source };\n\
        \tsource(\"2\")\n\
        }\n\n\
        fun main() {\n\tlet value = macro twice(1);\n\tprint(value);\n}\n";
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let std = std_spec();
            let analyze = || {
                leak_tally::reset();
                let (program, errors) = analyze_source(
                    source,
                    &std,
                    Path::new("."),
                    Path::new("world_site_probe.vl"),
                    Some(Platform::default()),
                    &Workspace::default(),
                );
                assert!(
                    errors.is_empty(),
                    "expected a clean compile, got: {errors:#?}"
                );
                assert!(program.is_some());
                (
                    leak_tally::bytes(LeakSite::EntryAst),
                    leak_tally::bytes(LeakSite::MacroWorldAst),
                )
            };
            let (cold_entry, cold_world) = analyze();
            let (warm_entry, warm_world) = analyze();
            assert!(
                cold_entry > 0,
                "no entry tree recorded — the pin is vacuous"
            );
            assert!(
                cold_world > 0,
                "the cold analysis compiled no macro world (or its tree recorded elsewhere) — \
                 the pin is vacuous"
            );
            assert_eq!(
                warm_world, 0,
                "the world was recompiled on a warm, unchanged analysis — `WORLDS` missed"
            );
            assert_eq!(
                cold_entry, warm_entry,
                "`EntryAst` must be the entry's own tree alone: the cold analysis recorded \
                 {cold_entry} B and the warm one {warm_entry} B, so the world's tree is \
                 leaking into the entry's site"
            );
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked");
}

/// M9 (`leak-soak.md` §7.9.4) with M23's claim: the opted-in entry point
/// (`analyze_source_owning_overlay_modules`) parses an overlay-served module
/// into ANALYSIS-OWNED allocations — no growth at either process-global cache
/// site — and the base world built over them CLAIMS them, so a second
/// analysis of the same content is served that world and parses nothing.
/// Every claim released, the balance nets to zero. The non-opted entry points
/// keep today's behavior byte for byte: the same overlaid load goes through
/// `parse_clean_cached` exactly as before (§7.9.4c — the CLI, the wasm front
/// end, and every transient reader must not switch), and they are never
/// served a CLAIMED world, having nowhere to keep its claims.
#[test]
fn an_opted_in_analysis_owns_overlay_served_modules_and_reclaims_them() {
    use vilan_core::leak_tally::{self, LeakSite};

    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("vilan_m9_core_{}_{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let entry_path = dir.join("main.vl");
    let helper_path = dir.join("helper.vl");
    std::fs::write(
        &entry_path,
        "import pkg::helper::value;\n\nfun main() {\n\tlet doubled = value() * 2;\n}\n",
    )
    .unwrap();
    std::fs::write(&helper_path, "export fun value(): i32 {\n\t1\n}\n").unwrap();
    let overlaid = "export fun value(): i32 {\n\t22222\n}\n".to_string();
    let overlaid_bytes = overlaid.len();
    vilan_core::analyzer::set_document_overlay(&helper_path, Some(overlaid));

    let source: &'static str = Box::leak(
        std::fs::read_to_string(&entry_path)
            .unwrap()
            .into_boxed_str(),
    );
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let std = std_spec();
            let analyze_owning = || {
                vilan_core::analyze_source_owning_overlay_modules(
                    source,
                    &std,
                    &dir,
                    &entry_path,
                    Some(Platform::default()),
                    &Workspace::default(),
                )
            };
            // Warmup: fill the process-global caches with std's parses (and
            // the disk helper), so the measured window below reads only what
            // the OVERLAY loads do. Its base world is cleared before the
            // window opens (M23 stores one now), so `first` below is a MISS
            // and really does parse the overlay.
            let _ = analyze_owning();
            vilan_core::analyzer::base_cache_clear();
            leak_tally::reset();
            let first = analyze_owning();
            assert!(
                first.diagnostics.is_empty(),
                "expected a clean compile, got: {:#?}",
                first.diagnostics
            );
            assert!(first.program.is_some());
            assert_eq!(
                first.owned_modules.len(),
                1,
                "the analysis owns exactly the one overlay-served module it loaded"
            );
            assert_eq!(
                leak_tally::bytes(LeakSite::ParseCleanCacheText),
                0,
                "the overlaid content must not reach the process-global clean cache"
            );
            assert_eq!(leak_tally::bytes(LeakSite::ModuleErrorText), 0);
            assert_eq!(
                leak_tally::bytes(LeakSite::OwnedModuleText),
                overlaid_bytes,
                "the owned copy records the overlaid text to the byte"
            );

            // M23: the world the first analysis built was STORED and claims
            // that copy, so a second opted-in analysis of the same content
            // hits it — it holds a claim on the same allocation and parses
            // nothing. (Before M23 the store was refused and this analysis
            // paid the whole pre-entry world again.)
            let second = analyze_owning();
            assert_eq!(
                second.owned_modules.len(),
                1,
                "the hitting analysis must hold its own claim on the copy the \
                 stored world served it"
            );
            assert_eq!(
                leak_tally::bytes(LeakSite::OwnedModuleText),
                overlaid_bytes,
                "M23: the second analysis is served the stored world's copy \
                 and parses nothing — a claim, not a second copy"
            );
            let (claims, claim_bytes) = vilan_core::analyzer::base_cache_overlay_claims();
            assert_eq!(
                (claims, claim_bytes),
                (1, overlaid_bytes),
                "the stored world holds exactly one claim, on the overlaid \
                 module's text"
            );

            // Reclaim in the owner's order: program first, then the handles.
            for analyzed in [first, second] {
                drop(analyzed.program);
                if let Some(ast) = analyzed.ast {
                    // SAFETY: the program — the tree's only borrower — was
                    // dropped on the line above.
                    unsafe { ast.reclaim() };
                }
                // SAFETY: as above. Only this analysis's claims are given
                // back; the stored world's keeps the allocation alive.
                unsafe { analyzed.owned_modules.reclaim() };
            }
            assert_eq!(
                leak_tally::outstanding(LeakSite::OwnedModuleText),
                overlaid_bytes as isize,
                "the stored world's claim is the one still outstanding — M23's \
                 retention, and what M24's budget bounds"
            );
            vilan_core::analyzer::base_cache_clear();
            assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleText), 0);
            assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleAst), 0);
            assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleErrors), 0);
            assert_eq!(
                leak_tally::bytes(LeakSite::OwnedModuleText),
                overlaid_bytes,
                "the gross record stands after the reclaim"
            );

            // The NON-opted path, same overlay: the process-global cache is
            // used exactly as before — the §7.5 leak is the recorded,
            // deliberate behavior for every caller that does not opt in.
            // (M23: it is also never served a CLAIMED base world, having no
            // scope to keep the claims in — so it really does load.)
            leak_tally::reset();
            let (program, errors) = analyze_source(
                source,
                &std,
                &dir,
                &entry_path,
                Some(Platform::default()),
                &Workspace::default(),
            );
            assert!(errors.is_empty(), "expected a clean compile: {errors:#?}");
            assert!(program.is_some());
            assert_eq!(
                leak_tally::bytes(LeakSite::OwnedModuleText),
                0,
                "a non-opted analysis must own nothing"
            );
            assert_eq!(
                leak_tally::bytes(LeakSite::ParseCleanCacheText),
                overlaid_bytes,
                "a non-opted analysis must keep serving the overlay through \
                 `parse_clean_cached`, byte for byte as today"
            );

            vilan_core::analyzer::set_document_overlay(&helper_path, None);
            let _ = std::fs::remove_dir_all(&dir);
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked");
}

/// M9 through a DEPENDENCY package (`leak-soak.md` §7.9.4): the multi-package
/// workspace is exactly where an overlay-served module reaches a stored base
/// world, so the mechanism must hold there too — the dependency's overlaid
/// module (loaded through its surface's `pkg::` self-reference) is
/// analysis-owned, exactly one copy, reclaimed to zero once the program
/// drops. (The per-scope path memo itself is pinned at the loader,
/// `analyzer::path_tests::an_active_scope_serves_a_repeat_load_from_its_memo`
/// — no analysis seam loads one module twice today.)
#[test]
fn a_dependency_packages_overlaid_module_is_owned_and_reclaimed() {
    use vilan_core::leak_tally::{self, LeakSite};

    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("vilan_m9_memo_{}_{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let app_dir = root.join("app");
    std::fs::create_dir_all(&app_dir).unwrap();
    std::fs::write(
        app_dir.join("main.vl"),
        "import common::greeting;\n\nfun main() {\n\tlet _x = greeting();\n}\n",
    )
    .unwrap();
    let dep_root = root.join("common");
    std::fs::create_dir_all(&dep_root).unwrap();
    std::fs::write(
        dep_root.join("lib.vl"),
        "import pkg::helper::dep_value;\nfun greeting(): i32 { dep_value() }\n",
    )
    .unwrap();
    let dep_helper = dep_root.join("helper.vl");
    std::fs::write(&dep_helper, "fun dep_value(): i32 { 1 }\n").unwrap();
    let workspace = Workspace {
        packages: vec![PackageSpec {
            base_root: dep_root,
            layers: Vec::new(),
            dependencies: Vec::new(),
            surface: true,
            member: false,
            prelude: Default::default(),
        }],
        entry_dependencies: vec![("common".to_string(), 0)],
        macro_limits: MacroLimits::default(),
        entry_prelude: Default::default(),
        ..Workspace::default()
    };
    let overlaid = "fun dep_value(): i32 { 424242 }\n".to_string();
    let overlaid_bytes = overlaid.len();
    vilan_core::analyzer::set_document_overlay(&dep_helper, Some(overlaid));
    let entry_path = app_dir.join("main.vl");
    let source: &'static str = Box::leak(
        std::fs::read_to_string(&entry_path)
            .unwrap()
            .into_boxed_str(),
    );
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let std = std_spec();
            let analyze_owning = || {
                vilan_core::analyze_source_owning_overlay_modules(
                    source,
                    &std,
                    &app_dir,
                    &entry_path,
                    Some(Platform::default()),
                    &workspace,
                )
            };
            let _warmup = analyze_owning();
            // M23: the warmup's base world is stored and claims its copy, so
            // clear it — the measured analysis below must MISS and really
            // parse the overlay for the byte assertion to mean anything.
            vilan_core::analyzer::base_cache_clear();
            leak_tally::reset();
            let analyzed = analyze_owning();
            assert!(
                analyzed.diagnostics.is_empty(),
                "expected a clean compile, got: {:#?}",
                analyzed.diagnostics
            );
            assert_eq!(
                analyzed.owned_modules.len(),
                1,
                "the dependency's overlaid module must be parsed and owned once"
            );
            assert_eq!(
                leak_tally::bytes(LeakSite::OwnedModuleText),
                overlaid_bytes,
                "the owned copy records the overlaid text to the byte"
            );
            drop(analyzed.program);
            if let Some(ast) = analyzed.ast {
                // SAFETY: the program was dropped on the line above.
                unsafe { ast.reclaim() };
            }
            // SAFETY: as above. This releases only this analysis's claim.
            unsafe { analyzed.owned_modules.reclaim() };
            assert_eq!(
                leak_tally::outstanding(LeakSite::OwnedModuleText),
                overlaid_bytes as isize,
                "M23: the base world stored for this analysis claims the copy, \
                 so the analysis's own release does not free it"
            );
            vilan_core::analyzer::base_cache_clear();
            assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleText), 0);

            vilan_core::analyzer::set_document_overlay(&dep_helper, None);
            let _ = std::fs::remove_dir_all(&root);
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked");
}

// --- B112: a post-`build()` check attributes to its span's own file ------------
//
// The checks that run after `build()` push into one diagnostics vector with no
// file walk to inherit an attribution mark from, so every one of them defaulted
// to `current_source_id` — which `analyze` leaves at the entry. A written
// `List<Guard>` inside an IMPORTED module therefore claimed the entry file, and
// the editor drew the label over whatever the entry happened to hold at the
// module's offsets (the harm E16/E1 exist to stop).
//
// A single-file program cannot see any of this: every user diagnostic in one is
// `SourceId(0)`, so a check attributing to the wrong file is indistinguishable
// from one attributing to the right file. That is why these live here rather
// than beside the single-source pins in the `inference` suite — the same reason B74's
// cross-module duplicate does.

/// The `resource Guard` preamble the resource-rule cases share, since a `Guard`
/// declaration is most of each of them.
const GUARD_PREAMBLE: &str = "import std::io::print;\nimport std::drop::Drop;\n\
    resource struct Guard { label: str }\n\
    impl Guard with Drop { fun drop(&mut self) { print(self.label); } }\n";

fn guarded(body: &str) -> String {
    format!("{GUARD_PREAMBLE}{body}")
}

/// The one diagnostic containing `message`, and the file it was attributed to.
#[track_caller]
fn attributed_to(files: &[(&str, &str)], message: &str) -> (String, Option<String>) {
    let attributed = analyze_package_attributed(files, "main.vl", Platform::default());
    let found = attributed
        .iter()
        .find(|(text, ..)| text.contains(message))
        .unwrap_or_else(|| panic!("no diagnostic contains {message:?}; got {attributed:#?}"));
    (found.1.clone(), found.2.clone())
}

// The filed shape: a WRITTEN `List<Guard>` inside an imported user module. R10's
// tier 1 collects the application at `walk_type_node` and reports it long after
// the walk, so the file has to ride along with the span.
#[test]
fn b112_a_written_container_resource_in_a_module_attributes_to_that_module() {
    let (file, _) = attributed_to(
        &[
            (
                "main.vl",
                "import pkg::store::keep;\nfun main() { keep(); }\n",
            ),
            (
                "store.vl",
                &guarded("fun keep() {\n\tmut arr: List<Guard> = [];\n}\n"),
            ),
        ],
        "`List` cannot hold the resource `Guard`",
    );
    assert_eq!(
        file, "store.vl",
        "R10 must report in the file that wrote it"
    );
}

// Every post-`build()` family, one row each: the violation is written in the
// MODULE, and the diagnostic must name the module. Plant-proven as a table —
// making `attribute_diagnostics_to_anchor` a no-op turns every row red, so no
// row is passing on the entry's account.
#[test]
fn b112_every_post_build_check_attributes_to_the_module_it_fired_in() {
    let call_go = "import pkg::m::go;\nfun main() { go(); }\n";
    // (the check family, the module's source, the entry's source, the message)
    let cases: Vec<(&str, String, &str, &str)> = vec![
        (
            "R10, written application",
            guarded("fun go() {\n\tmut arr: List<Guard> = [];\n}\n"),
            call_go,
            "`List` cannot hold the resource `Guard`",
        ),
        (
            "R10, inferred type",
            guarded("fun go() {\n\tmut arr = [Guard { label = \"one\" }];\n}\n"),
            call_go,
            "`List` cannot hold the resource `Guard`",
        ),
        (
            "R10, native-method receiver",
            guarded("fun go() {\n\tlet n = [Guard { label = \"one\" }].len();\n}\n"),
            call_go,
            "`List` cannot hold the resource `Guard`",
        ),
        (
            "R1, use after move",
            guarded(
                "fun sink(own g: Guard) {}\nfun go() {\n\tlet g = Guard { label = \"one\" };\n\
                 \tsink(g);\n\tsink(g);\n}\n",
            ),
            call_go,
            "after it was moved",
        ),
        (
            "R12, resource into `any`",
            guarded(
                "fun show(v: any) {}\nfun go() {\n\tlet g = Guard { label = \"one\" };\n\
                 \tshow(g);\n}\n",
            ),
            call_go,
            "cannot be used where `any` is expected",
        ),
        (
            "the `mut` resource parameter reject",
            guarded("fun take(mut g: Guard) {}\nfun go() {}\n"),
            call_go,
            "a resource never copies",
        ),
        (
            "`Drop` on a non-resource",
            "import std::io::print;\nimport std::drop::Drop;\nstruct Plain { n: i32 }\n\
             impl Plain with Drop { fun drop(&mut self) { print(\"x\"); } }\nfun go() {}\n"
                .to_string(),
            call_go,
            "implements `Drop` but is not a resource",
        ),
        (
            "view escape",
            "fun go(): List<&i32> {\n\tlet v = 1;\n\t[&v]\n}\n".to_string(),
            call_go,
            "a view cannot escape its scope",
        ),
        (
            "readonly mutation",
            "fun go() {\n\tlet n = 1;\n\tn = 2;\n}\n".to_string(),
            call_go,
            "cannot mutate immutable 'n'",
        ),
        (
            "the async view-parameter rule",
            "async fun tick() { let _beat = 1; }\n\
             async fun go(value: &mut i32) {\n\tawait tick();\n\tvalue += 1;\n}\n"
                .to_string(),
            "import pkg::m::go;\nfun main() {\n\tmut a = 5;\n\tgo(&mut a);\n}\n",
            "an async function cannot take '&mut' parameters",
        ),
        (
            "the Wire boundary",
            "[derive(Wire)]\nstruct Holder {\n\tcallback: |i53| i53,\n}\n".to_string(),
            "import pkg::m::Holder;\nfun main() { }\n",
            "of `[derive(Wire)]` type `Holder` is `_`, which is not Wire",
        ),
        (
            "the Hashable boundary",
            "import std::hash::Hashable;\n[derive(Hashable)]\n\
             struct Handler { name: str, callback: || void }\n"
                .to_string(),
            "import pkg::m::Handler;\nfun main() { }\n",
            "which is not `Hashable`",
        ),
        (
            "the `[rpc]` signature rule",
            "struct Password { hash: str }\nstruct Service {}\n\
             impl Service {\n\t[rpc] fun store(self, secret: Password) {}\n}\n"
                .to_string(),
            "import pkg::m::Service;\nfun main() { }\n",
            "parameter `secret` of `[rpc]` method `store` is `Password`, which is not Wire",
        ),
        (
            "the `[expose]` rule",
            "import std::reactive::{ Signal, SignalCell };\nstruct Password { hash: str }\n\
             struct Session {\n\t[expose] secret: SignalCell<Password>,\n}\n"
                .to_string(),
            "import pkg::m::Session;\nfun main() { }\n",
            "is `[expose]`d, but its element `Password` is not Wire",
        ),
        (
            "the tuple-spread rule",
            "import std::io::print;\nfun forward(items: (i32, i32)): i32 { items.0 }\n\
             fun go() {\n\tlet pair = (1, 2);\n\tprint(forward(..pair));\n}\n"
                .to_string(),
            call_go,
            "`..` splices a tuple's elements into a tuple construction",
        ),
        // The `external` backed-return row retired at the merge: backed-enums
        // §9's ratified lift DELETED that refusal (the trap arm covers the
        // boundary now), so its program compiles by design and there is no
        // diagnostic left to attribute.
    ];
    // Every row is checked before anything is asserted, so a plant that breaks
    // attribution reports which families it broke rather than only the first.
    let mut wrong: Vec<String> = Vec::new();
    for (family, module, entry, message) in cases {
        let (file, _) = attributed_to(&[("main.vl", entry), ("m.vl", &module)], message);
        if file != "m.vl" {
            wrong.push(format!("{family}: attributed to {file}, want m.vl"));
        }
    }
    assert!(
        wrong.is_empty(),
        "a module's violation must name the module:\n{}",
        wrong.join("\n")
    );
}

// The other half of the claim: the ENTRY's own post-`build()` violations are
// unchanged. Both files break the same rule in one program, and each diagnostic
// goes home — a fix that simply moved everything off the entry would fail here.
#[test]
fn b112_the_entrys_own_violations_still_attribute_to_the_entry() {
    let attributed = analyze_package_attributed(
        &[
            (
                "main.vl",
                &guarded(
                    "import pkg::store::keep;\nfun main() {\n\tkeep();\n\
                     \tmut mine: List<Guard> = [];\n}\n",
                ),
            ),
            (
                "store.vl",
                "import pkg::main::Guard;\nfun keep() {\n\tmut theirs: List<Guard> = [];\n}\n",
            ),
        ],
        "main.vl",
        Platform::default(),
    );
    let files: Vec<&str> = attributed
        .iter()
        .filter(|(message, ..)| message.contains("cannot hold the resource `Guard`"))
        .map(|(_, file, _)| file.as_str())
        .collect();
    assert!(
        files.contains(&"main.vl") && files.contains(&"store.vl"),
        "each spelling reports in its own file: {attributed:#?}"
    );
}

// A cross-file NOTE pair, with neither end in the entry: the conformance
// mismatch is written in `impls.vl` and the trait it violates is declared in
// `traits.vl`. The note's `source` means "the diagnostic's own file" when it is
// `None`, so it had to stop being compared against `current_source_id` — that
// was the same thing as the diagnostic's file only while every post-`build()`
// diagnostic claimed the entry.
#[test]
fn b112_a_cross_file_note_names_the_file_it_points_into() {
    let (file, note_file) = attributed_to(
        &[
            (
                "main.vl",
                "import pkg::impls::Cat;\nfun main() { let c = Cat { n = 1 }; }\n",
            ),
            ("traits.vl", "trait Greet { fun greet(self): str; }\n"),
            (
                "impls.vl",
                "import pkg::traits::Greet;\nstruct Cat { n: i32 }\n\
                 impl Cat with Greet { fun greet(self): i32 { 1 } }\n",
            ),
        ],
        "match the declared return type",
    );
    assert_eq!(file, "impls.vl", "the impl's mistake is the impl's file");
    assert_eq!(
        note_file.as_deref(),
        Some("traits.vl"),
        "and the note names the file the trait is declared in"
    );
}

// The same pair for R11, whose primary is the INSTANTIATION and whose note is in
// the generic BODY: two different modules, and neither is the entry.
#[test]
fn b112_an_r11_violation_splits_across_the_caller_and_the_generic() {
    let (file, note_file) = attributed_to(
        &[
            (
                "main.vl",
                "import pkg::caller::run;\nfun main() { run(); }\n",
            ),
            (
                "generic.vl",
                "import std::io::print;\nfun twice<T>(own value: T) {\n\tlet a = value;\n\
                 \tlet b = value;\n\tprint(\"x\");\n}\n",
            ),
            (
                "caller.vl",
                &guarded(
                    "import pkg::generic::twice;\nfun run() {\n\
                     \ttwice(Guard { label = \"one\" });\n}\n",
                ),
            ),
        ],
        "is not move-clean when instantiated with a resource",
    );
    assert_eq!(file, "caller.vl", "the instantiation is the caller's");
    assert_eq!(
        note_file.as_deref(),
        Some("generic.vl"),
        "and the note points into the generic body's own file"
    );
}

// --- The import-path enumeration primitives (E57) ---------------------------
//
// Import-path completion has to answer about modules the program has NOT
// loaded, so it reads the package tree directly: `modules_in_root` lists what an
// origin holds, `module_source_file` resolves one name through the loader's own
// root order, and `module_importables` reads a module's importable names through
// the loader's own content-keyed parse cache. These pin that trio against the
// REAL std tree — the same source of truth the loader resolves from — rather
// than a fixture, because a hardcoded module list is exactly the failure mode.

/// The std module names an import may reach for `platform`, deduped in the
/// loader's own root order (an earlier root shadows a later one).
fn std_module_names(platform: Platform) -> Vec<String> {
    let spec = std_spec();
    let mut names: Vec<String> = Vec::new();
    for root in spec.search_roots(platform) {
        for (name, _path) in vilan_core::analyzer::modules_in_root(root) {
            if name != "lib" && !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

/// A module that exists only in the document overlay lists under its root
/// (K9, `playground-completion.md` §6): the playground has no filesystem, so
/// `import std::|` can only enumerate std this way, and in the editor an
/// unsaved new sibling completes as `import pkg::<name>` before it is saved.
/// Both module forms list, `lib.vl` lists as `lib` like the disk listing
/// does, a nested file is not a module, and a name also on disk is not
/// listed twice.
#[test]
fn an_overlay_module_lists_under_its_root() {
    let root = PathBuf::from(format!("/vilan_overlay_listing_{}", std::process::id()));
    let flat = root.join("flat.vl");
    let nested = root.join("nested").join("lib.vl");
    let surface = root.join("lib.vl");
    let deep = root.join("nested").join("deeper").join("deep.vl");
    let elsewhere =
        PathBuf::from(format!("/vilan_overlay_elsewhere_{}", std::process::id())).join("other.vl");
    for path in [&flat, &nested, &surface, &deep, &elsewhere] {
        vilan_core::analyzer::set_document_overlay(path, Some("fun f() {}\n".to_string()));
    }
    let listed = vilan_core::analyzer::modules_in_root(&root);
    for path in [&flat, &nested, &surface, &deep, &elsewhere] {
        vilan_core::analyzer::set_document_overlay(path, None);
    }
    let names: Vec<&str> = listed.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        ["flat", "lib", "nested"],
        "the overlay's modules under the root, sorted: {listed:?}"
    );
    let nested_path = &listed
        .iter()
        .find(|(name, _)| name == "nested")
        .expect("nested is listed")
        .1;
    assert!(
        nested_path.ends_with("nested/lib.vl"),
        "a directory module lists its lib.vl: {nested_path:?}"
    );
}

#[test]
fn the_std_listing_comes_from_the_std_tree() {
    let names = std_module_names(Platform::default());
    for expected in ["json", "math", "option", "list", "string"] {
        assert!(
            names.contains(&expected.to_string()),
            "`std::{expected}` is a std module: {names:?}"
        );
    }
    // A layer's directory is NOT a path segment: `src/process/fs.vl` is
    // `std::fs`, and the layer name never appears in an import.
    assert!(
        names.contains(&"fs".to_string()),
        "a layered module lists under its own name: {names:?}"
    );
    assert!(
        !names.contains(&"lib".to_string()),
        "the package surface `lib.vl` is not a module: {names:?}"
    );
}

#[test]
fn module_source_file_resolves_through_the_loaders_root_order() {
    let spec = std_spec();
    let platform = Platform::default();
    let roots = spec.search_roots(platform);
    let json = vilan_core::analyzer::module_source_file(&roots, "json")
        .expect("`std::json` resolves to a file");
    assert!(json.ends_with("json.vl"), "resolved {json:?}");
    assert_eq!(
        vilan_core::analyzer::module_source_file(&roots, "no_such_std_module"),
        None,
        "a name that is not a module resolves to nothing"
    );
}

#[test]
fn module_importables_reads_a_modules_declarations_on_demand() {
    let spec = std_spec();
    let roots = spec.search_roots(Platform::default());
    let path = vilan_core::analyzer::module_source_file(&roots, "json").expect("std::json");
    let importables = vilan_core::analyzer::module_importables(&path);
    let named = |name: &str| importables.iter().find(|item| item.name == name).cloned();
    let names: Vec<&str> = importables.iter().map(|item| item.name).collect();

    let json = named("Json").unwrap_or_else(|| panic!("`Json` is declared in json.vl: {names:?}"));
    assert_eq!(json.kind, vilan_core::analyzer::ImportableKind::Trait);
    // An `external struct` / `external fun` is importable like any other
    // declaration — `import std::json::JsonValue` is the common case.
    assert_eq!(
        named("JsonValue").map(|item| item.kind),
        Some(vilan_core::analyzer::ImportableKind::Struct),
        "an external struct is importable: {names:?}"
    );
    // An enum carries its variants, so a further segment
    // (`std::json::JsonKind::Number`) has something to complete against.
    let kind = named("JsonKind").expect("`JsonKind` is an enum in json.vl");
    assert_eq!(kind.kind, vilan_core::analyzer::ImportableKind::Enum);
    assert!(
        kind.variants.contains(&"Number") && kind.variants.contains(&"Object"),
        "JsonKind's variants: {:?}",
        kind.variants
    );
    // json.vl's own implementation imports are NOT published through it.
    assert!(
        named("Shared").is_none(),
        "a module's own `import` is not importable through it: {names:?}"
    );
}

#[test]
fn module_importables_publishes_a_modules_reexports() {
    // `std/src/prelude.vl` declares nothing at all — it is entirely
    // `export import`, and those leaves are exactly the names the base prelude
    // makes ambient. This is also the query `seed_preludes` rests on: the
    // ambient set is a module's IMPORTABLES, read syntactically.
    //
    // It used to read `lib.vl`, whose re-exports were the `std::print` /
    // `std::panic` short-name ALIASES. The alias sweep (prelude.md §10.2)
    // deleted those, and `lib.vl` now publishes nothing — pinned below.
    let spec = std_spec();
    let importables = vilan_core::analyzer::module_importables(&spec.base_root.join("prelude.vl"));
    let names: Vec<&str> = importables.iter().map(|item| item.name).collect();
    for expected in ["print", "Option", "Some", "None", "Result", "Ok", "Err"] {
        assert!(
            names.contains(&expected),
            "the base prelude's surface is missing `{expected}`: {names:?}"
        );
    }
    assert!(
        importables
            .iter()
            .all(|item| item.kind == vilan_core::analyzer::ImportableKind::Reexport),
        "every one of them is a re-export: {names:?}"
    );
}

#[test]
fn stds_package_root_publishes_nothing_after_the_alias_sweep() {
    // Thirteen aliases lived in `std/src/lib.vl` to let a caller write
    // `std::print` instead of `std::io::print`. The prelude serves that, so
    // they are gone — and `prelude = "std"` is refused by the manifest partly
    // because accepting it would now mean a silently EMPTY prelude.
    let spec = std_spec();
    let importables = vilan_core::analyzer::module_importables(&spec.base_root.join("lib.vl"));
    assert!(
        importables.is_empty(),
        "std's root must publish nothing: {:?}",
        importables.iter().map(|item| item.name).collect::<Vec<_>>()
    );
}

#[test]
fn the_web_preludes_surface_is_its_members_and_its_ambient_modules() {
    // §5.2/§5.3: three members and two MODULES, published by the one
    // mechanism — `export import pkg::style;` publishes the module `style`
    // exactly as `export import pkg::reactive::Signal;` publishes a member.
    let spec = std_spec();
    let importables = vilan_core::analyzer::module_importables(&spec.base_root.join("web.vl"));
    let names: Vec<&str> = importables.iter().map(|item| item.name).collect();
    for expected in [
        "print", "Option", "Some", "None", "Result", "Ok", "Err", "Signal", "view", "View",
        "style", "ui",
    ] {
        assert!(
            names.contains(&expected),
            "the web prelude's surface is missing `{expected}`: {names:?}"
        );
    }
}

#[test]
fn module_importables_of_an_unreadable_file_is_empty() {
    // A module that fails to load answers EMPTY. An editor query degrades; it
    // never fails, and it never panics.
    assert!(
        vilan_core::analyzer::module_importables(&PathBuf::from("/no/such/module.vl")).is_empty()
    );
}

// --- The prelude (prelude.md §5, §7, §9) ---------------------------------
//
// The prelude is a RESOLUTION SCOPE, never synthesized file-head imports
// (§9.2). That distinction is the whole risk of the feature and it is what
// these pins exist to hold: in this compiler an explicit import BEATS a
// same-file declaration, so a prelude spliced in as imports would silently
// replace a file's own `fun print` / `enum Signal` with std's. Every shadowing
// pin below goes red the moment the implementation drifts that way.

/// Analyzes `entry` against a package whose manifest declares `prelude`, with
/// `files` written beside it. Returns the diagnostic messages.
fn analyze_under_prelude(
    prelude: PreludeSpec,
    files: &[(&str, &str)],
    entry: &str,
    platform: Platform,
) -> Vec<String> {
    analyze_under_prelude_repaired(prelude, PreludeRepair::default(), files, entry, platform)
}

/// [`analyze_under_prelude`] with the front end's declared prelude REPAIR
/// (E120) — which control the web-set steer sends the reader to.
fn analyze_under_prelude_repaired(
    prelude: PreludeSpec,
    repair: PreludeRepair,
    files: &[(&str, &str)],
    entry: &str,
    platform: Platform,
) -> Vec<String> {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("vilan_prelude_{}_{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (relative, contents) in files {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, contents).unwrap();
    }
    let entry_path = dir.join(entry);
    let source = std::fs::read_to_string(&entry_path).unwrap();
    let leaked: &'static str = Box::leak(source.into_boxed_str());
    let workspace = Workspace {
        entry_prelude: prelude,
        prelude_repair: repair,
        ..Workspace::default()
    };
    let (_program, errors) = analyze_source(
        leaked,
        &std_spec(),
        &dir,
        &entry_path,
        Some(platform),
        &workspace,
    );
    let _ = std::fs::remove_dir_all(&dir);
    errors.into_iter().map(|error| error.msg).collect()
}

fn base_prelude() -> PreludeSpec {
    PreludeSpec::Module(vilan_core::manifest::DEFAULT_PRELUDE.to_string())
}

fn web_prelude() -> PreludeSpec {
    PreludeSpec::Module(vilan_core::manifest::WEB_PRELUDE.to_string())
}

#[test]
fn the_base_prelude_binds_its_seven_names_with_no_import() {
    // §5.1. The `Option` case is the argument for the whole feature: the
    // language MANUFACTURES an `Option` here (a view-returning lookup, a lang
    // item) and before the prelude refused to let the user take it apart,
    // because they had not imported names they never wrote.
    let entry = "fun main() {\n\
        \tlet found = [1, 2, 3].get(1);\n\
        \tmatch found {\n\
        \t\tSome(let n) => print(\"some\"),\n\
        \t\tNone => print(\"none\"),\n\
        \t}\n\
        \tlet outcome: Result<i32, str> = Ok(1);\n\
        \tmatch outcome {\n\
        \t\tOk(let v) => print(\"ok\"),\n\
        \t\tErr(let e) => print(e),\n\
        \t}\n\
        }\n";
    let errors = analyze_under_prelude(
        base_prelude(),
        &[("main.vl", entry)],
        "main.vl",
        Platform::default(),
    );
    assert!(
        errors.is_empty(),
        "expected a clean compile, got: {errors:#?}"
    );
}

#[test]
fn a_local_declaration_shadows_a_prelude_name_silently() {
    // §9.1/§9.2, and the single most important pin in this file. An explicit
    // import beats a same-file declaration in this compiler, so a prelude
    // implemented as synthesized imports would make this file's own `print`
    // DEAD CODE — silently. It must be the file's own, with no diagnostic.
    let entry = "import std::io;\n\
        fun print(message: str): void { io::print(message); }\n\
        fun main() { print(\"mine\"); }\n";
    let errors = analyze_under_prelude(
        base_prelude(),
        &[("main.vl", entry)],
        "main.vl",
        Platform::default(),
    );
    assert!(
        errors.is_empty(),
        "a local declaration must win over the prelude, silently: {errors:#?}"
    );
}

#[test]
fn a_local_type_declaration_shadows_a_web_prelude_name_silently() {
    // The estate's one real collision (`vilan/test/match-patterns.vl` declares
    // `enum Signal`), staged against the set that actually binds the name.
    let entry = "enum Signal { Quit, Finished }\n\
        fun main() {\n\
        \tlet s = Signal::Quit;\n\
        \tmatch s { Signal::Quit => print(\"q\"), Signal::Finished => print(\"f\") }\n\
        }\n";
    let errors = analyze_under_prelude(
        web_prelude(),
        &[("main.vl", entry)],
        "main.vl",
        Platform::Browser,
    );
    assert!(
        errors.is_empty(),
        "the file's own `enum Signal` must win over the web prelude's: {errors:#?}"
    );
}

#[test]
fn an_explicit_import_shadows_a_prelude_name_silently() {
    // The prelude is the WEAKEST scope, so re-importing what it already binds
    // is a no-op rather than a redeclaration error — which is what makes the
    // whole estate's 419 now-redundant import statements keep compiling (§12).
    let entry = "import std::io::print;\n\
        import std::option::Option::{ self, None, Some };\n\
        fun main() {\n\
        \tlet found: Option<i32> = Some(1);\n\
        \tmatch found { Some(let n) => print(\"s\"), None => print(\"n\") }\n\
        }\n";
    let errors = analyze_under_prelude(
        base_prelude(),
        &[("main.vl", entry)],
        "main.vl",
        Platform::default(),
    );
    assert!(
        errors.is_empty(),
        "expected a clean compile, got: {errors:#?}"
    );
}

#[test]
fn a_module_files_own_declaration_shadows_the_prelude_too() {
    // The entry file and a module file are seeded at DIFFERENT points — the
    // entry before its walk (so ordering alone protects it), a module after
    // its walk and after its imports (so the seed must yield with
    // `or_insert`). A prelude that plain-`insert`s would leave this module's
    // own `fun print` dead while every entry-file shadowing pin stayed green,
    // which is exactly how that gap was found.
    // The module's own `print` takes an `i32` and RETURNS one, so a prelude
    // that clobbered it would not merely call the wrong function — it would
    // fail to type. That is deliberate: the real defect here is a miscompile,
    // and an analyze-only harness can only see it if the shadowed signature
    // disagrees.
    let errors = analyze_under_prelude(
        base_prelude(),
        &[
            (
                "main.vl",
                "import pkg::helper::speak;\nfun main() { speak(); }\n",
            ),
            (
                "helper.vl",
                "fun print(count: i32): i32 { count + 1 }\n\
export fun speak(): void {\n\
\tlet next: i32 = print(1);\n\
}\n",
            ),
        ],
        "main.vl",
        Platform::default(),
    );
    assert!(
        errors.is_empty(),
        "a module's own declaration must win over the prelude: {errors:#?}"
    );
    // And the same for an explicit import inside a module file.
    let errors = analyze_under_prelude(
        base_prelude(),
        &[
            (
                "main.vl",
                "import pkg::helper::pick;\nfun main() { print(\"x\"); }\n",
            ),
            (
                "helper.vl",
                "import std::option::Option::{ self, None, Some };\n\
export fun pick(): Option<i32> { Some(1) }\n",
            ),
        ],
        "main.vl",
        Platform::default(),
    );
    assert!(
        errors.is_empty(),
        "expected a clean compile, got: {errors:#?}"
    );
}

#[test]
fn prelude_false_leaves_the_ambient_names_unbound() {
    // §10.1's posture, and the path std itself takes. The steer still points
    // at the real module path, which is what the alias sweep leaves behind.
    let errors = analyze_under_prelude(
        PreludeSpec::Off,
        &[("main.vl", "fun main() { print(\"hi\"); }\n")],
        "main.vl",
        Platform::default(),
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("cannot find 'print' in this scope")),
        "{errors:#?}"
    );
}

#[test]
fn the_prelude_reaches_a_packages_modules_not_only_its_entry() {
    // The entry walks in the global scope and a module walks in its own, so
    // the two are seeded at different points; a prelude that reached only one
    // of them would pass every single-file pin here.
    let errors = analyze_under_prelude(
        base_prelude(),
        &[
            (
                "main.vl",
                "import pkg::helper::pick;\nfun main() { print(\"x\"); }\n",
            ),
            ("helper.vl", "export fun pick(): Option<i32> { Some(1) }\n"),
        ],
        "main.vl",
        Platform::default(),
    );
    assert!(
        errors.is_empty(),
        "expected a clean compile, got: {errors:#?}"
    );
}

#[test]
fn the_web_prelude_binds_signal_view_and_the_ambient_modules() {
    // §5.3: three members and two modules. `style::Display` is the dissolution
    // of §3.4's collision — the CSS enum reached through the ambient MODULE,
    // with bare `Display` left to `std::display::Display` alone.
    let entry = "fun card(): View { view(\"div\") }\n\
        fun main() {\n\
        \tlet count = Signal::new(0);\n\
        \tlet shown = style::Display::Flex;\n\
        \tlet gap = style::space(4);\n\
        \tprint(\"web\");\n\
        }\n";
    let errors = analyze_under_prelude(
        web_prelude(),
        &[("main.vl", entry)],
        "main.vl",
        Platform::Browser,
    );
    assert!(
        errors.is_empty(),
        "expected a clean compile, got: {errors:#?}"
    );
}

#[test]
fn an_ambient_module_is_beaten_by_an_explicit_member_import() {
    // §4.1/§13.11, and the reason the `style` module costs the estate nothing:
    // `std::style::style` is a FUNCTION whose name equals its module's, and 60
    // call sites write it bare. Each carries this import, which outranks the
    // ambient module — so `style()` keeps meaning the builder.
    let entry = "import std::style::style;\n\
import std::style::Style;\n\
fun styled(): Style { style() }\n\
fun main() { print(\"styled\"); }\n";
    let errors = analyze_under_prelude(
        web_prelude(),
        &[("main.vl", entry)],
        "main.vl",
        Platform::Browser,
    );
    assert!(
        errors.is_empty(),
        "expected a clean compile, got: {errors:#?}"
    );
}

#[test]
fn shadowing_an_ambient_module_costs_that_files_qualified_spelling() {
    // The other half of the pin above, and a consequence worth pinning rather
    // than discovering: a name has ONE binding, so a file that imports the
    // FUNCTION `style` no longer reaches the MODULE `style` — `style::Display`
    // stops resolving there. This costs the estate nothing (its 60 `style()`
    // call sites import the enums they use explicitly and never write
    // `style::…`), and it is the ordinary shadowing rule rather than anything
    // the prelude adds. Recorded in prelude.md §4.1.
    let entry = "import std::style::style;\n\
fun main() {\n\
\tlet shown = style::Display::Flex;\n\
\tprint(\"styled\");\n\
}\n";
    let errors = analyze_under_prelude(
        web_prelude(),
        &[("main.vl", entry)],
        "main.vl",
        Platform::Browser,
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("is not a module") && e.contains("Display")),
        "{errors:#?}"
    );
}

#[test]
fn the_ambient_module_carries_the_style_cluster_when_nothing_shadows_it() {
    // The admission argument for an ambient MODULE (§5.2): one name in the
    // bare namespace buys a whole namespace. `Display` here is the CSS enum
    // through `style::`, which is how §3.4's collision dissolves — bare
    // `Display` stays `std::display::Display`'s alone.
    let entry = "fun main() {\n\
\tlet shown = style::Display::Flex;\n\
\tlet gap = style::space(4);\n\
\tlet builder = style::style();\n\
\tprint(\"cluster\");\n\
}\n";
    let errors = analyze_under_prelude(
        web_prelude(),
        &[("main.vl", entry)],
        "main.vl",
        Platform::Browser,
    );
    assert!(
        errors.is_empty(),
        "expected a clean compile, got: {errors:#?}"
    );
}

#[test]
fn the_base_prelude_does_not_bind_the_web_sets_names() {
    // The two sets are genuinely different scopes: a base-prelude package
    // reaching for `Signal` gets a diagnostic, not a silent bind.
    let errors = analyze_under_prelude(
        base_prelude(),
        &[(
            "main.vl",
            "fun main() { let s = Signal::new(0); print(\"x\"); }\n",
        )],
        "main.vl",
        Platform::Browser,
    );
    assert!(errors.iter().any(|e| e.contains("Signal")), "{errors:#?}");
}

#[test]
fn a_dependency_resolves_under_its_own_prelude_not_the_consumers() {
    // §7, the composability rule: two packages that disagree about what
    // `Signal` means must both keep compiling in one build. The consumer takes
    // the WEB set; the dependency declares none of its own, so it takes the
    // base one — `Some` resolves inside it and `Signal` must not.
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("vilan_preliso_{}_{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let app_dir = root.join("app");
    std::fs::create_dir_all(&app_dir).unwrap();
    std::fs::write(
        app_dir.join("main.vl"),
        "import common::greeting;\nfun main() { let s = Signal::new(0); print(\"app\"); }\n",
    )
    .unwrap();
    let dep_root = root.join("common");
    std::fs::create_dir_all(&dep_root).unwrap();
    std::fs::write(
        dep_root.join("lib.vl"),
        // Its own base prelude gives it `Option`/`Some`; the consumer's web
        // prelude must NOT give it `Signal`.
        "export fun greeting(): Option<i32> { Some(1) }\nfun leak(): i32 { let s = Signal::new(0); 0 }\n",
    )
    .unwrap();
    let workspace = Workspace {
        packages: vec![PackageSpec {
            base_root: dep_root,
            layers: Vec::new(),
            dependencies: Vec::new(),
            surface: true,
            member: false,
            prelude: Default::default(),
        }],
        entry_dependencies: vec![("common".to_string(), 0)],
        macro_limits: MacroLimits::default(),
        entry_prelude: web_prelude(),
        ..Workspace::default()
    };
    let entry_path = app_dir.join("main.vl");
    let source = std::fs::read_to_string(&entry_path).unwrap();
    let leaked: &'static str = Box::leak(source.into_boxed_str());
    let (_program, errors) = analyze_source(
        leaked,
        &std_spec(),
        &app_dir,
        &entry_path,
        Some(Platform::Browser),
        &workspace,
    );
    let _ = std::fs::remove_dir_all(&root);
    let errors: Vec<String> = errors.into_iter().map(|error| error.msg).collect();
    assert!(
        errors.iter().any(|e| e.contains("Signal")),
        "the consumer's web prelude must not reach into the dependency: {errors:#?}"
    );
    assert!(
        !errors
            .iter()
            .any(|e| e.contains("'Some'") || e.contains("'Option'")),
        "the dependency's OWN base prelude must still bind: {errors:#?}"
    );
}

#[test]
fn a_web_set_name_steers_to_the_manifest_key_not_to_an_import() {
    // §11.4 determination 1. A base-prelude package reaching for `Signal` is
    // the one new confusion two sets create, and the repair is a manifest
    // line — so the ordinary "import it first" steer would send the user the
    // wrong way. This arm fires ahead of it.
    let errors = analyze_under_prelude(
        base_prelude(),
        &[(
            "main.vl",
            "fun main() { let s = Signal::new(0); print(\"x\"); }\n",
        )],
        "main.vl",
        Platform::Browser,
    );
    assert!(
        errors.iter().any(|e| {
            e.contains("in the prelude of the web set") && e.contains("prelude = \"std::web\"")
        }),
        "{errors:#?}"
    );
}

/// The playground arm of the same steer (E120). WHICH repair the message names
/// is a fact about the front end, not about the program — a `vilan.toml` line
/// is advice a pasted buffer cannot take — so the front end declares it and the
/// analyzer owns both wordings. The manifest arm above is this one's control:
/// same program, same prelude, default repair, unchanged sentence.
#[test]
fn a_toggle_front_end_steers_at_the_prelude_toggle_not_the_manifest() {
    let errors = analyze_under_prelude_repaired(
        base_prelude(),
        PreludeRepair::Toggle,
        &[(
            "main.vl",
            "fun main() { let s = Signal::new(0); print(\"x\"); }\n",
        )],
        "main.vl",
        Platform::Browser,
    );
    let steer = errors
        .iter()
        .find(|error| error.contains("in the prelude of the web set"))
        .unwrap_or_else(|| panic!("{errors:#?}"));
    assert!(
        steer.contains("switch the playground's prelude to the web set"),
        "{steer}"
    );
    // The one-name import beside it, so the reader who wants exactly this name
    // has a repair that does not change what the rest of the buffer means.
    assert!(steer.contains("import std::reactive::Signal;"), "{steer}");
    assert!(
        !steer.contains("vilan.toml"),
        "there is no manifest to edit: {steer}"
    );
}

#[test]
fn a_name_in_neither_std_prelude_keeps_the_ordinary_import_steer() {
    // The arm must be narrow: `Map` is in no prelude, so the B4 import steer
    // still answers, and the LSP quickfix it drives still fires.
    let errors = analyze_under_prelude(
        base_prelude(),
        &[(
            "main.vl",
            "fun main() { let m: Map<str, i32> = Map::new(); print(\"x\"); }\n",
        )],
        "main.vl",
        Platform::default(),
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("import it first (`import std::map::Map;`)")),
        "{errors:#?}"
    );
}

#[test]
fn a_package_already_on_the_web_set_never_gets_the_web_steer() {
    // "You are not on the web set" is only true when it is true. On `std::web`
    // a genuinely missing name gets the ordinary steer.
    let errors = analyze_under_prelude(
        web_prelude(),
        &[(
            "main.vl",
            "fun main() { let m: Map<str, i32> = Map::new(); print(\"x\"); }\n",
        )],
        "main.vl",
        Platform::Browser,
    );
    assert!(
        !errors
            .iter()
            .any(|e| e.contains("in the prelude of the web set")),
        "{errors:#?}"
    );
}

#[test]
fn a_module_carried_web_name_gets_no_web_steer() {
    // The web set carries `style` and `ui` as MODULES, so switching preludes
    // does not make the bare name a value — the "both work" promise fails
    // exactly there (audit run 6, F2). A value-position miss on `style` must
    // fall through to the ordinary machinery (the css note beside the css
    // desugar names the import that actually compiles: `std::style::style`).
    let errors = analyze_under_prelude(
        base_prelude(),
        &[("main.vl", "fun main() { let s = style(); }\n")],
        "main.vl",
        Platform::default(),
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("cannot find 'style' in this scope")),
        "the ordinary miss still reports: {errors:?}",
    );
    assert!(
        !errors.iter().any(|e| e.contains("web set")),
        "no web-set steer for a module-carried name: {errors:?}",
    );
}

#[test]
fn a_module_carried_web_name_reaches_its_types_by_qualifying() {
    // B172, and the reason it was load-bearing rather than cosmetic. The web
    // set carries `style` as a MODULE, so a web-set user reached every VALUE in
    // `std::style` (`style::style()`, `style::Display::Flex`) and no TYPE in
    // it: `style::Style` was a PARSE error in every type position, and both web
    // templates carried a forced `import std::style::Style;` to get around it.
    // A qualified path is a type now, so the prelude's module name is enough.
    let errors = analyze_under_prelude(
        web_prelude(),
        &[(
            "main.vl",
            "fun card(): style::Style {\n\
             \tstyle::style().padding(style::space(4))\n\
             }\n\
             fun main() {\n\
             \tlet _card = const card();\n\
             }\n",
        )],
        "main.vl",
        Platform::Browser,
    );
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn the_base_seven_never_steer_toward_the_web_set() {
    // Both sets carry them, so a `prelude = false` package missing `print`
    // must be told to import it, not to switch sets.
    let errors = analyze_under_prelude(
        PreludeSpec::Off,
        &[("main.vl", "fun main() { print(\"x\"); }\n")],
        "main.vl",
        Platform::default(),
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("import it first (`import std::io::print;`)")),
        "{errors:#?}"
    );
    assert!(!errors.iter().any(|e| e.contains("web set")), "{errors:#?}");
}

// --- The alias sweep (prelude.md §10.2) ----------------------------------

#[test]
fn the_removed_std_print_alias_names_the_prelude_and_the_real_path() {
    // `std::print` will be typed from muscle memory for a long time. The
    // generic "cannot find 'print' in the imported path" names neither the
    // removal nor either way forward; this arm names both.
    let errors = analyze_under_prelude(
        base_prelude(),
        // The import line below is THE SUBJECT of this pin — the removed
        // alias must be spelled to draw its curated removal message. A
        // fixture sweep deleting `import std::print;` lines must skip this
        // one (it did not, once, at the Order 22 integration).
        &[(
            "main.vl",
            "import std::print;\nfun main() { print(\"x\"); }\n",
        )],
        "main.vl",
        Platform::default(),
    );
    assert!(
        errors.iter().any(|e| {
            e.contains("`std::print` was removed")
                && e.contains("in the default prelude")
                && e.contains("std::io::print")
        }),
        "{errors:#?}"
    );
}

#[test]
fn a_removed_alias_the_prelude_does_not_carry_names_only_its_real_path() {
    // `panic`, `assert` and `Default` are in neither std prelude, so telling
    // the user "no import needed" would be false.
    for (line, expected) in [
        (
            "import std::panic;",
            "`std::panic` was removed: its module path is `std::io::panic`",
        ),
        (
            "import std::assert;",
            "`std::assert` was removed: its module path is `std::io::assert`",
        ),
        (
            "import std::Default;",
            "`std::Default` was removed: its module path is `std::default::Default`",
        ),
    ] {
        let source = format!("{line}\nfun main() {{ }}\n");
        let errors = analyze_under_prelude(
            base_prelude(),
            &[("main.vl", &source)],
            "main.vl",
            Platform::default(),
        );
        assert!(
            errors.iter().any(|e| e.contains(expected)),
            "{line}: {errors:#?}"
        );
    }
}

#[test]
fn a_removed_primitive_alias_says_the_name_is_already_in_scope() {
    // The numerics and `str` are §4.7 primitives, ambient with no prelude at
    // all — so steering toward an import would point at something that was
    // never needed. Their aliases had zero uses in the whole estate.
    for name in ["i32", "u32", "f64", "BigInt", "str"] {
        let source = format!("import std::{name};\nfun main() {{ }}\n");
        let errors = analyze_under_prelude(
            base_prelude(),
            &[("main.vl", &source)],
            "main.vl",
            Platform::default(),
        );
        assert!(
            errors.iter().any(|e| {
                e.contains(&format!("`std::{name}` was removed"))
                    && e.contains("is a primitive and is always in scope")
            }),
            "{name}: {errors:#?}"
        );
    }
}

#[test]
fn the_real_module_paths_still_resolve_after_the_sweep() {
    // The sweep deleted the SHORT spellings, not the names. Every alias's real
    // home must still import cleanly — this is what the estate migrated onto.
    let entry = "import std::io::print;\n\
import std::io::panic;\n\
import std::default::Default;\n\
import std::string::str;\n\
import std::number::u32;\n\
fun main() { print(\"x\"); }\n";
    let errors = analyze_under_prelude(
        base_prelude(),
        &[("main.vl", entry)],
        "main.vl",
        Platform::default(),
    );
    assert!(
        errors.is_empty(),
        "expected a clean compile, got: {errors:#?}"
    );
}

#[test]
fn a_deeper_std_path_is_not_mistaken_for_a_removed_alias() {
    // The arm must fire only directly under `std`'s root: a typo inside a real
    // module keeps the ordinary message.
    let errors = analyze_under_prelude(
        base_prelude(),
        &[("main.vl", "import std::io::prnt;\nfun main() { }\n")],
        "main.vl",
        Platform::default(),
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("cannot find 'prnt' in the imported path")),
        "{errors:#?}"
    );
}
