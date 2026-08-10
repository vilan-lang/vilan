//! Filesystem-backed tests for package-module resolution (P1): a `pkg::` module
//! resolves equivalently whether it's a flat `foo.vl` or a directory `foo/lib.vl`,
//! both existing is an ambiguity error, and the `none` platform gates out the
//! platform `std` layers. These need real files on disk (the loader reads them),
//! so each writes a throwaway package directory and analyzes against it.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use vilan_core::{
    Error, Layer, MacroLimits, PackageSpec, Platform, PlatformPattern, Workspace, analyze_source,
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

const ENTRY: &str = "import std::print;\nimport pkg::foo::bar;\nfun main() { print(bar()); }\n";
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
    let entry = "import std::print;\nfun main() { print(1); }\n";
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
        });
        entry_dependencies.push((dep.import_name.to_string(), index));
    }
    let workspace = Workspace {
        packages,
        entry_dependencies,
        macro_limits: MacroLimits::default(),
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
    let entry = "import std::print;\nimport common::greeting;\nfun main() { print(greeting()); }\n";
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
    let entry = "import std::print;\nimport common::shape::area;\nfun main() { print(area(2)); }\n";
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
        "import std::print;\n",
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
        }],
        entry_dependencies: vec![("common".to_string(), 0)],
        macro_limits: MacroLimits::default(),
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
        }],
        entry_dependencies: vec![("plat".to_string(), 0)],
        macro_limits: MacroLimits::default(),
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
        }],
        entry_dependencies: vec![("plat".to_string(), 0)],
        macro_limits: MacroLimits::default(),
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
    };
    let violations = vilan_core::analyzer::check_library_contract(&spec)
        .into_iter()
        .map(|error| error.msg)
        .collect();
    let _ = std::fs::remove_dir_all(&root);
    violations
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

// A type error INSIDE an imported module is attributed to that module's file,
// not the entry — the root cause of the LSP's vanishing-diagnostics bug (the
// error was mapped through the entry's line index and disappeared).
#[test]
fn a_type_error_in_an_imported_module_is_attributed_to_that_module() {
    let attributed = analyze_package_attributed(
        &[
            (
                "main.vl",
                "import std::print;\nimport pkg::broken::answer;\nfun main() { print(answer()); }\n",
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

// A module that fails to PARSE attributes its (spanless) parse diagnostics to
// its own file, so the editor can surface them there.
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
        .find(|(msg, ..)| msg.contains("parse error in"))
        .expect("the module parse error should be reported");
    assert_eq!(parse_error.1, "util.vl", "{attributed:?}");
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
    let entry = "import std::print;\nimport std::json::encode_json;\nimport pkg::json::stamp;\n\
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
    let entry = "import std::print;\nimport pkg::string::shout;\n\
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
    let entry = "import std::print;\nimport pkg::io::log_line;\n\
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
    let entry = "import std::print;\nimport std::json::encode_json;\n\
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
        "import std::print;\nimport internpin::greeting;\nfun main() { print(greeting()); }\n";
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
// than beside the single-source pins in `inference.rs` — the same reason B74's
// cross-module duplicate does.

/// The `resource Guard` preamble the resource-rule cases share, since a `Guard`
/// declaration is most of each of them.
const GUARD_PREAMBLE: &str = "import std::print;\nimport std::drop::Drop;\n\
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
            "import std::print;\nimport std::drop::Drop;\nstruct Plain { n: i32 }\n\
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
            "import std::reactive::Signal;\nstruct Password { hash: str }\n\
             struct Session {\n\t[expose] secret: Signal<Password>,\n}\n"
                .to_string(),
            "import pkg::m::Session;\nfun main() { }\n",
            "is `[expose]`d, but its element `Password` is not Wire",
        ),
        (
            "the tuple-spread rule",
            "import std::print;\nfun forward(items: (i32, i32)): i32 { items.0 }\n\
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
                "import std::print;\nfun twice<T>(own value: T) {\n\tlet a = value;\n\
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
