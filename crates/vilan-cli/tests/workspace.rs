//! End-to-end CLI tests for the multi-package workspace model (P2) and the
//! single-package multi-entry form (`[entry.<name>]`,
//! proposal/platform-coloring.md §4.2): building a workspace emits one bundle
//! per host member / per entry, the platform-compatibility rule and dependency
//! cycles are rejected, and the retired `[server]`/`[client]` form fails with
//! its migration hint. It also carries B33's cross-package pin
//! (proposal/b33-emission-order.md §1): the canonical initialization order
//! across packages needs a workspace to be observable at all, so it lives here
//! rather than beside the single-file corpus pin.
//!
//! Each test writes a throwaway project tree and drives the built `vilan` binary.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

/// A fresh temp directory for one test's project tree.
fn temp_project(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "vilan_ws_cli_{tag}_{}_{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Writes `contents` to `dir/relative`, creating parent directories.
fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// Runs the `vilan` binary with `args`. `std` resolves from the in-repo default.
fn vilan(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(args)
        .output()
        .expect("run vilan")
}

/// Writes a small client/server/common workspace into `dir` (the client and server
/// both `import common::greeting`).
fn write_fullstack_workspace(dir: &Path) {
    write(
        dir,
        "vilan.toml",
        "[project]\npackages = [\"common\", \"client\", \"server\"]\n",
    );
    write(dir, "common/vilan.toml", "[library]\nname = \"common\"\n");
    write(dir, "common/src/lib.vl", "fun greeting(): str { \"hi\" }\n");
    write(
        dir,
        "server/vilan.toml",
        "[package]\nname = \"server\"\ntarget = \"node\"\n\n[package.dependencies]\ncommon = { path = \"../common\" }\n",
    );
    write(
        dir,
        "server/src/main.vl",
        "import std::print;\nimport common::greeting;\nfun main() { print(greeting()) }\n",
    );
    write(
        dir,
        "client/vilan.toml",
        "[package]\nname = \"client\"\ntarget = \"browser\"\n\n[package.dependencies]\ncommon = { path = \"../common\" }\n",
    );
    write(
        dir,
        "client/src/main.vl",
        "import common::greeting;\nfun main() { greeting() }\n",
    );
}

#[test]
fn workspace_builds_each_host_member() {
    let dir = temp_project("build");
    write_fullstack_workspace(&dir);
    let output = vilan(&["build", dir.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // A bundle per host member; the `none` library is not built on its own.
    assert!(
        dir.join("dist/server.js").is_file(),
        "missing dist/server.js"
    );
    assert!(
        dir.join("dist/client.js").is_file(),
        "missing dist/client.js"
    );
    assert!(
        !dir.join("dist/common.js").exists(),
        "the `none` library should not be built standalone"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A command's combined stdout + stderr. Every diagnostic goes to stderr now
/// (windows-support.md §6), but `--stdout` builds and program output do not, so
/// a test asserting on "what the CLI said" still wants both.
fn combined(output: &Output) -> String {
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    text
}

#[test]
fn cross_platform_library_module_is_rejected_without_cascade() {
    // A browser app imports a module that lives only in a library's `process` layer:
    // the cross-platform import is a recoverable error (the build fails) — but the
    // module still loads for typing, so `feature` resolves and there's no
    // unresolved-name cascade (L1).
    let dir = temp_project("compat");
    write(
        dir.as_path(),
        "platlib/vilan.toml",
        "[library]\nname = \"platlib\"\n\n[library.layer.process]\nplatform = [\"@process\"]\n",
    );
    write(dir.as_path(), "platlib/src/lib.vl", "");
    write(
        dir.as_path(),
        "platlib/src/process/feature.vl",
        "fun value(): i32 { 1 }\n",
    );
    write(
        &dir,
        "web/vilan.toml",
        "[package]\nname = \"web\"\ntarget = \"browser\"\n\n[package.dependencies]\nplatlib = { path = \"../platlib\" }\n",
    );
    write(
        &dir,
        "web/src/main.vl",
        "import platlib::feature::value;\nfun main() { value() }\n",
    );
    let output = vilan(&["build", dir.join("web").to_str().unwrap()]);
    assert!(
        !output.status.success(),
        "expected a cross-platform failure"
    );
    let text = combined(&output);
    assert!(
        text.contains("requires the `process` layer of `platlib`") && text.contains("main → value"),
        "expected a chain-rendered coloring violation: {text}"
    );
    assert!(
        !text.contains("cannot find"),
        "the module should still type-check (no cascade): {text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_package_dependency_is_allowed_and_colors_inferentially() {
    // Platform coloring's blessed shape (platform-coloring.md §7.3): an app
    // may depend on a `[package]`. Its neutral items are reachable from any
    // build; reaching a function that touches platform std is the chain
    // diagnostic — the dependency's `target` declares its entry, not a gate.
    let dir = temp_project("pkgdep");
    write(
        dir.as_path(),
        "applib/vilan.toml",
        "[package]\nname = \"applib\"\ntarget = \"node\"\n",
    );
    write(dir.as_path(), "applib/src/main.vl", "fun main() {}\n");
    write(
        dir.as_path(),
        "applib/src/util.vl",
        "import std::fs::write_file;\nfun neutral(): i32 { 2 }\nfun save() { write_file(\"x\", \"y\") }\n",
    );
    write(
        &dir,
        "web/vilan.toml",
        "[package]\nname = \"web\"\ntarget = \"browser\"\n\n[package.dependencies]\napplib = { path = \"../applib\" }\n",
    );
    // Reaching the neutral item from the browser: fine.
    write(
        &dir,
        "web/src/main.vl",
        "import applib::util::neutral;\nfun main() { neutral(); }\n",
    );
    let output = vilan(&["build", dir.join("web").to_str().unwrap()]);
    assert!(
        output.status.success(),
        "a neutral package-dependency item should build for the browser: {}",
        combined(&output)
    );
    // Reaching the fs-colored item: the chain diagnostic.
    write(
        &dir,
        "web/src/main.vl",
        "import applib::util::save;\nfun main() { save(); }\n",
    );
    let output = vilan(&["build", dir.join("web").to_str().unwrap()]);
    assert!(!output.status.success(), "expected a coloring violation");
    let text = combined(&output);
    assert!(
        text.contains("requires the `process` layer of `std`") && text.contains("main → save"),
        "expected the chain diagnostic: {text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dependency_cycle_is_rejected() {
    // App `web` → library `liba` → library `libb` → `liba` (a cycle).
    let dir = temp_project("cycle");
    write(
        &dir,
        "web/vilan.toml",
        "[package]\nname = \"web\"\ntarget = \"node\"\n\n[package.dependencies]\nliba = { path = \"../liba\" }\n",
    );
    write(
        &dir,
        "web/src/main.vl",
        "import liba::va;\nfun main() { va() }\n",
    );
    write(
        &dir,
        "liba/vilan.toml",
        "[library]\nname = \"liba\"\n\n[library.dependencies]\nlibb = { path = \"../libb\" }\n",
    );
    write(&dir, "liba/src/lib.vl", "fun va(): i32 { 1 }\n");
    write(
        &dir,
        "libb/vilan.toml",
        "[library]\nname = \"libb\"\n\n[library.dependencies]\nliba = { path = \"../liba\" }\n",
    );
    write(&dir, "libb/src/lib.vl", "fun vb(): i32 { 1 }\n");
    let output = vilan(&["check", dir.join("web").to_str().unwrap()]);
    assert!(!output.status.success(), "expected a cycle failure");
    let text = combined(&output);
    assert!(text.contains("cycle"), "unexpected output: {text}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Writes a two-entry single package (§4.2): a browser `client` and a node
/// `server`, sharing a `pkg::store` module whose `load` reaches `std::fs`.
/// Only the server calls `load` — the shape a full-stack app actually has.
fn write_multi_entry_package(dir: &Path) {
    write(
        dir,
        "vilan.toml",
        "[package]\nname = \"app\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    write(
        dir,
        "src/store.vl",
        "import std::fs;\n\nfun load(): bool {\n\tfs::exists(\"state\")\n}\n",
    );
    write(
        dir,
        "src/server.vl",
        "import std::print;\nimport pkg::store::load;\n\nfun main() {\n\tif load() { print(\"loaded\") } else { print(\"fresh\") }\n}\n",
    );
    write(
        dir,
        "src/client.vl",
        "import std::print;\n\nfun main() {\n\tprint(\"ui\");\n}\n",
    );
}

#[test]
fn a_multi_entry_package_builds_every_entry_into_dist() {
    // `[entry.<name>]` lowers onto the workspace orchestration: one
    // `dist/<name>.js` per entry, each compiled for its own target — the
    // node-only `store.load` is fine because the client never reaches it.
    let dir = temp_project("entries");
    write_multi_entry_package(&dir);
    let output = vilan(&["build", dir.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "multi-entry build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        dir.join("dist/server.js").is_file(),
        "missing dist/server.js"
    );
    assert!(
        dir.join("dist/client.js").is_file(),
        "missing dist/client.js"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_picks_the_single_node_entry() {
    // `vilan run` on a multi-entry package builds everything, then runs the
    // one node entry (the workspace rule, unchanged).
    let dir = temp_project("entries_run");
    write_multi_entry_package(&dir);
    let output = vilan(&["run", dir.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("fresh"), "server should run: {stdout}");
    assert!(
        dir.join("dist/client.js").is_file(),
        "run should have built the client bundle first"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_colors_each_entry_against_its_own_target() {
    // `vilan check` checks every entry, always (§7 decision 4). The same
    // package is clean until the CLIENT entry reaches the store — then the
    // browser build fails with the coloring chain while the server stays fine.
    let dir = temp_project("entries_check");
    write_multi_entry_package(&dir);
    let clean = vilan(&["check", dir.to_str().unwrap()]);
    assert!(
        clean.status.success(),
        "clean check failed: {}",
        String::from_utf8_lossy(&clean.stderr)
    );
    write(
        &dir,
        "src/client.vl",
        "import std::print;\nimport pkg::store::load;\n\nfun main() {\n\tif load() { print(\"?\") }\n}\n",
    );
    let violating = vilan(&["check", dir.to_str().unwrap()]);
    assert!(
        !violating.status.success(),
        "the client's reach into `std::fs` must fail the browser entry"
    );
    let text = combined(&violating);
    assert!(
        text.contains("requires the `process` layer of `std`")
            && text.contains("cannot run on `browser`"),
        "unexpected output: {text}"
    );
    assert!(
        text.contains("main → load → exists (std::fs)"),
        "the chain should name the path: {text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn colliding_output_names_are_rejected() {
    // A workspace member named `app` and a sibling's `[entry.app]` would both
    // write `dist/app.js` — rejected at lowering instead of silently racing.
    let dir = temp_project("collide");
    write(
        &dir,
        "vilan.toml",
        "[project]\npackages = [\"app\", \"site\"]\n",
    );
    write(&dir, "app/vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "app/src/main.vl", "fun main() { }\n");
    write(
        &dir,
        "site/vilan.toml",
        "[package]\nname = \"site\"\n\n[entry.app]\ntarget = \"browser\"\n",
    );
    write(&dir, "site/src/app.vl", "fun main() { }\n");
    let output = vilan(&["build", dir.to_str().unwrap()]);
    assert!(!output.status.success(), "expected a collision failure");
    let text = combined(&output);
    assert!(text.contains("dist/app.js"), "unexpected output: {text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_retired_server_client_form_fails_with_the_migration_hint() {
    // The old top-level pair doesn't lower any more — it names its
    // replacement instead of building.
    let dir = temp_project("retired");
    write(
        &dir,
        "vilan.toml",
        "[server]\nentry = \"server.vl\"\n[client]\nentry = \"client.vl\"\n",
    );
    write(
        &dir,
        "server.vl",
        "import std::print;\nfun main() { print(1) }\n",
    );
    write(&dir, "client.vl", "fun main() { }\n");
    let output = vilan(&["build", dir.to_str().unwrap()]);
    assert!(!output.status.success(), "the retired form must not build");
    let text = combined(&output);
    assert!(
        text.contains("[entry.server]"),
        "the error should name the replacement: {text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn standalone_library_check_verifies_the_platform_contract() {
    // A `[library]` has no fixed platform: `vilan check` verifies its contract (every
    // module's `pkg::` imports resolve across the platforms its layer serves) rather
    // than a single-platform build — and `vilan build` rejects it (a library is built
    // only as a dependency).
    let dir = temp_project("contract");
    write(
        dir.as_path(),
        "vilan.toml",
        "[library]\nname = \"lib\"\n\n[library.layer.process]\nplatform = [\"@process\"]\n",
    );
    write(dir.as_path(), "src/lib.vl", "");
    write(dir.as_path(), "src/util.vl", "fun util(): i32 { 1 }\n");
    write(
        dir.as_path(),
        "src/process/service.vl",
        "import pkg::util::util;\nfun service(): i32 { util() }\n",
    );
    let check = vilan(&["check", dir.to_str().unwrap()]);
    assert!(
        check.status.success(),
        "a well-formed library's contract should pass: {}",
        combined(&check)
    );
    let build = vilan(&["build", dir.to_str().unwrap()]);
    assert!(!build.status.success(), "a `[library]` is not buildable");
    assert!(
        combined(&build).contains("[library]"),
        "unexpected build output: {}",
        combined(&build)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn standalone_library_check_flags_a_contract_violation() {
    // A base module (serves every platform) importing a process-only module is a
    // completeness violation (the browser can't provide it); `vilan check` reports it
    // and fails.
    let dir = temp_project("contract_bad");
    write(
        dir.as_path(),
        "vilan.toml",
        "[library]\nname = \"lib\"\n\n[library.layer.process]\nplatform = [\"@process\"]\n",
    );
    write(dir.as_path(), "src/lib.vl", "");
    write(
        dir.as_path(),
        "src/core.vl",
        "import pkg::feature::feature;\nfun core(): i32 { feature() }\n",
    );
    write(
        dir.as_path(),
        "src/process/feature.vl",
        "fun feature(): i32 { 1 }\n",
    );
    let output = vilan(&["check", dir.to_str().unwrap()]);
    assert!(!output.status.success(), "expected a contract violation");
    let text = combined(&output);
    assert!(
        text.contains("not available for") && text.contains("browser"),
        "unexpected output: {text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_parse_error_inside_a_package_module_fails_the_build_loudly() {
    // Package modules (std, libraries, `pkg::` siblings) load through the
    // error-recovering parser: a syntax error used to be silently swallowed —
    // the recovered `Node::Error` compiled to *nothing*, so the module built
    // with the broken statements simply gone. It must fail, naming the file
    // and position.
    let dir = temp_project("module-parse-error");
    write(dir.as_path(), "vilan.toml", "[package]\nname = \"app\"\n");
    write(
        dir.as_path(),
        "src/main.vl",
        "import pkg::util::util;\nfun main() { let _ = util(); }\n",
    );
    write(
        dir.as_path(),
        "src/util.vl",
        "fun util(): i32 { 1 }\nfun broken( {\n",
    );
    let build = vilan(&["build", dir.to_str().unwrap()]);
    assert!(
        !build.status.success(),
        "a module with a parse error must not build"
    );
    let output = combined(&build);
    assert!(
        output.contains("parse error in") && output.contains("util.vl"),
        "the diagnostic should name the broken module: {output}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// Block-scoped imports (backlog H2), the multi-package path: a dependency and a
// `pkg::` sibling referenced ONLY inside function bodies must still seed the
// loader's reachable set — `collect_module_refs` finds references at any depth.
#[test]
fn body_scoped_imports_load_dependencies_and_siblings() {
    let dir = temp_project("body_imports");
    write(
        &dir,
        "vilan.toml",
        "[project]\npackages = [\"common\", \"server\"]\n",
    );
    write(&dir, "common/vilan.toml", "[library]\nname = \"common\"\n");
    write(
        &dir,
        "common/src/lib.vl",
        "fun greeting(): str { \"hi\" }\n",
    );
    write(
        &dir,
        "server/vilan.toml",
        "[package]\nname = \"server\"\ntarget = \"node\"\n\n[package.dependencies]\ncommon = { path = \"../common\" }\n",
    );
    write(
        &dir,
        "server/src/main.vl",
        "import std::print;\n\nfun main() {\n    import common::greeting;\n    import pkg::helper;\n    print(greeting());\n    print(helper::tagline());\n}\n",
    );
    write(
        &dir,
        "server/src/helper.vl",
        "fun tagline(): str { \"from a sibling\" }\n",
    );
    let output = vilan(&["build", dir.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let run = Command::new("node")
        .arg(dir.join("dist/server.js"))
        .output()
        .expect("run node");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "hi\nfrom a sibling\n",
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
}

// The §4.2 completeness check sees imports at any depth (backlog H2): a base
// module smuggling a process-only module through a FUNCTION-BODY import is the
// same violation as a top-level one.
#[test]
fn standalone_library_check_flags_a_body_scoped_violation() {
    let dir = temp_project("contract_body");
    write(
        dir.as_path(),
        "vilan.toml",
        "[library]\nname = \"lib\"\n\n[library.layer.process]\nplatform = [\"@process\"]\n",
    );
    write(dir.as_path(), "src/lib.vl", "");
    write(
        dir.as_path(),
        "src/core.vl",
        "fun core(): i32 {\n    import pkg::feature::feature;\n    feature()\n}\n",
    );
    write(
        dir.as_path(),
        "src/process/feature.vl",
        "fun feature(): i32 { 1 }\n",
    );
    let output = vilan(&["check", dir.to_str().unwrap()]);
    assert!(!output.status.success(), "expected a contract violation");
    let text = combined(&output);
    assert!(
        text.contains("not available for") && text.contains("browser"),
        "unexpected output: {text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The docs walkthrough app (docs/guide/walkthrough.md quotes its files) must
/// keep building — it is the book's capstone example.
#[test]
fn the_walkthrough_example_builds() {
    let example =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/examples/walkthrough");
    let output = vilan(&["build", example.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "walkthrough build failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(example.join("dist/client.js").is_file());
    assert!(example.join("dist/client.css").is_file());
    assert!(example.join("dist/server.js").is_file());
}

// --- A15's follow-up: the manifest-designated default `run` entry -----------

/// A node entry whose stdout says which leg ran.
fn marker_source(marker: &str) -> String {
    format!("import std::print;\n\nfun main() {{\n\tprint(\"{marker}\");\n}}\n")
}

#[test]
fn a_workspace_runs_its_designated_default_member() {
    // Two node members, so `run` used to demand `--entry`; `[project]
    // default-entry` designates one and the flag becomes optional.
    let dir = temp_project("default_member");
    write(
        &dir,
        "vilan.toml",
        "[project]\npackages = [\"api\", \"jobs\"]\ndefault-entry = \"jobs\"\n",
    );
    write(&dir, "api/vilan.toml", "[package]\nname = \"api\"\n");
    write(&dir, "api/src/main.vl", &marker_source("API_RAN"));
    write(&dir, "jobs/vilan.toml", "[package]\nname = \"jobs\"\n");
    write(&dir, "jobs/src/main.vl", &marker_source("JOBS_RAN"));

    let designated = vilan(&["run", dir.to_str().unwrap()]);
    let text = combined(&designated);
    assert!(
        designated.status.success() && text.contains("JOBS_RAN") && !text.contains("API_RAN"),
        "the designated member should have run: {text}"
    );

    // The flag still wins — a one-off overrides the standing designation.
    let overridden = vilan(&["run", "--entry", "api", dir.to_str().unwrap()]);
    let text = combined(&overridden);
    assert!(
        overridden.status.success() && text.contains("API_RAN") && !text.contains("JOBS_RAN"),
        "`--entry` should override the manifest: {text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_multi_entry_package_runs_its_designated_default_entry() {
    // The other shape with the same problem: one package, several node
    // `[entry.<name>]` sections. The same key, on `[package]`.
    let dir = temp_project("default_entry");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ndefault-entry = \"server\"\n\n\
         [entry.server]\n\n[entry.probe]\n",
    );
    write(&dir, "src/server.vl", &marker_source("SERVER_RAN"));
    write(&dir, "src/probe.vl", &marker_source("PROBE_RAN"));

    let output = vilan(&["run", dir.to_str().unwrap()]);
    let text = combined(&output);
    assert!(
        output.status.success() && text.contains("SERVER_RAN") && !text.contains("PROBE_RAN"),
        "the designated entry should have run: {text}"
    );
    // The non-selected entry still compiles into the project.
    assert!(dir.join("dist/probe.js").is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_undesignated_multi_node_project_names_both_ways_to_choose() {
    let dir = temp_project("undesignated");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\n\n[entry.server]\n\n[entry.probe]\n",
    );
    write(&dir, "src/server.vl", &marker_source("SERVER_RAN"));
    write(&dir, "src/probe.vl", &marker_source("PROBE_RAN"));

    let output = vilan(&["run", dir.to_str().unwrap()]);
    let text = combined(&output);
    assert!(
        !output.status.success()
            && text.contains("--entry <name>")
            && text.contains("`[package] default-entry`")
            && text.contains("probe, server"),
        "unexpected output: {text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// --- A9: `[build] run` hooks ------------------------------------------------

#[test]
fn a_build_hook_runs_before_the_build_that_consumes_it() {
    // The ordering is observable, not asserted: the hook GENERATES a module the
    // entry imports. If hooks ran after the build (or not at all), the compile
    // would fail on the missing module.
    let dir = temp_project("hook_before");
    // The hook runs through the PLATFORM shell (the A9 design), so the fixture's
    // command is per-platform: `printf` does not exist in cmd. cmd's `echo`
    // emits a trailing space + CRLF — both trivia to the compiler, so the
    // generated module still parses (the windows CI leg caught the original
    // printf-only fixture).
    let hook = if cfg!(windows) {
        "run = \"echo fun generated(): i32 { 41 }> src/generated.vl\"\n"
    } else {
        "run = \"printf 'fun generated(): i32 { 41 }\\n' > src/generated.vl\"\n"
    };
    write(
        &dir,
        "vilan.toml",
        &format!("[package]\nname = \"app\"\n\n[build]\n{hook}"),
    );
    write(
        &dir,
        "src/main.vl",
        "import std::print;\nimport pkg::generated::generated;\n\
         fun main() { print(generated() + 1) }\n",
    );
    // The generated module does not exist yet — only the hook creates it.
    assert!(!dir.join("src/generated.vl").exists());

    let output = vilan(&["run", dir.to_str().unwrap()]);
    let text = combined(&output);
    assert!(
        output.status.success() && text.contains("42"),
        "the hook should have produced the module the build consumed: {text}"
    );
    assert!(dir.join("src/generated.vl").is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failing_build_hook_fails_the_build_and_names_the_command() {
    let dir = temp_project("hook_fails");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\n\n[build]\nrun = [\"exit 3\", \"touch never-reached\"]\n",
    );
    write(&dir, "src/main.vl", &marker_source("APP_RAN"));

    let output = vilan(&["build", dir.to_str().unwrap()]);
    let text = combined(&output);
    assert!(
        !output.status.success() && text.contains("exit 3"),
        "the failure should name the command: {text}"
    );
    // The build never happened, and neither did the hook after the failing one.
    assert!(!dir.join("src/main.js").exists());
    assert!(!dir.join("never-reached").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hooks_run_in_declaration_order_from_the_manifests_directory() {
    // Sequential, in order, with the manifest's directory as the working
    // directory — so a relative path in a hook means what it says in the file
    // that declares it.
    let dir = temp_project("hook_order");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\n\n[build]\n\
         run = [\"echo one > order.txt\", \"echo two >> order.txt\"]\n",
    );
    write(&dir, "src/main.vl", &marker_source("APP_RAN"));

    let output = vilan(&["build", dir.to_str().unwrap()]);
    assert!(output.status.success(), "{}", combined(&output));
    let order = std::fs::read_to_string(dir.join("order.txt")).expect("the hooks wrote it");
    // Normalized per line: cmd's `echo` writes a trailing space + CRLF where
    // sh writes a bare LF — the ORDER is the assertion, not the shell's
    // whitespace dialect (the windows CI leg caught the exact-bytes version).
    let lines: Vec<&str> = order.lines().map(str::trim_end).collect();
    assert_eq!(lines, ["one", "two"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn no_hooks_is_no_change_and_check_never_runs_them() {
    // Two halves of "costs nothing": a manifest with no `[build] run` builds
    // exactly as before, and `vilan check` — which produces no artifacts — runs
    // no hooks even when they are declared.
    let plain = temp_project("hook_absent");
    write(&plain, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&plain, "src/main.vl", &marker_source("APP_RAN"));
    let output = vilan(&["build", plain.to_str().unwrap()]);
    assert!(output.status.success(), "{}", combined(&output));
    assert!(plain.join("src/main.js").is_file());
    let _ = std::fs::remove_dir_all(&plain);

    let checked = temp_project("hook_check");
    write(
        &checked,
        "vilan.toml",
        "[package]\nname = \"app\"\n\n[build]\nrun = [\"touch ran-during-check\"]\n",
    );
    write(&checked, "src/main.vl", &marker_source("APP_RAN"));
    let output = vilan(&["check", checked.to_str().unwrap()]);
    assert!(output.status.success(), "{}", combined(&output));
    assert!(!checked.join("ran-during-check").exists());
    let _ = std::fs::remove_dir_all(&checked);
}

// ── `vilan test` compiles a test as a file OF its package
// (proposal/distribution.md §7's S4 residual: it compiled against an empty
// workspace, so a `*_test.vl` could import neither a sibling through the
// manifest's `root` nor any dependency) ──

/// The `*_test.vl` body for a one-assertion test.
fn test_source(imports: &str, condition: &str, label: &str) -> String {
    format!(
        "import std::assert;\n{imports}\n\nfun main() {{\n\tassert({condition}, \"{label}\");\n}}\n"
    )
}

#[test]
fn a_test_resolves_pkg_siblings_through_the_manifests_root() {
    // The test file sits beside the manifest while the sources live under the
    // declared `root` — so `pkg::` can only resolve if the test is compiled
    // with the PACKAGE's root, not with its own directory.
    let dir = temp_project("test_sibling");
    write(&dir, "vilan.toml", "[package]\nname = \"app\"\n");
    write(&dir, "src/main.vl", "fun main() {}\n");
    write(&dir, "src/thing.vl", "fun value(): i32 { 9 }\n");
    write(
        &dir,
        "outer_test.vl",
        &test_source(
            "import pkg::thing::value;",
            "value() == 9",
            "the sibling resolves",
        ),
    );

    let output = vilan(&["test", dir.to_str().unwrap()]);
    assert!(output.status.success(), "{}", combined(&output));
    assert!(
        combined(&output).contains("1 passed, 0 failed"),
        "{}",
        combined(&output)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_test_resolves_a_path_dependency() {
    let dir = temp_project("test_pathdep");
    write(&dir, "shapes/vilan.toml", "[library]\nname = \"shapes\"\n");
    write(&dir, "shapes/src/lib.vl", "fun area(): i32 { 7 }\n");
    write(
        &dir,
        "app/vilan.toml",
        "[package]\nname = \"app\"\n\n[package.dependencies]\n\
         shapes = { path = \"../shapes\" }\n",
    );
    write(&dir, "app/src/main.vl", "fun main() {}\n");
    write(
        &dir,
        "app/src/dep_test.vl",
        &test_source(
            "import shapes::area;",
            "area() == 7",
            "the dependency resolves",
        ),
    );

    let output = vilan(&["test", dir.join("app/src").to_str().unwrap()]);
    assert!(output.status.success(), "{}", combined(&output));
    assert!(
        combined(&output).contains("1 passed, 0 failed"),
        "{}",
        combined(&output)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_librarys_own_test_resolves_its_siblings_and_dependencies() {
    // A `[library]` is never a build unit, so nothing else in the CLI resolves
    // a workspace for one — but its tests are compiled from inside it, and they
    // are entitled to the same package it declares.
    let dir = temp_project("test_library");
    write(&dir, "shapes/vilan.toml", "[library]\nname = \"shapes\"\n");
    write(&dir, "shapes/src/lib.vl", "fun area(): i32 { 7 }\n");
    write(
        &dir,
        "lib2/vilan.toml",
        "[library]\nname = \"lib2\"\n\n[library.dependencies]\n\
         shapes = { path = \"../shapes\" }\n",
    );
    write(
        &dir,
        "lib2/src/lib.vl",
        "fun twice(n: i32): i32 { n * 2 }\n",
    );
    write(
        &dir,
        "lib2/src/util.vl",
        "fun triple(n: i32): i32 { n * 3 }\n",
    );
    write(
        &dir,
        "lib2/src/util_test.vl",
        &test_source(
            "import pkg::util::triple;\nimport shapes::area;",
            "triple(2) == 6 && area() == 7",
            "the library's sibling and dependency both resolve",
        ),
    );

    let output = vilan(&["test", dir.join("lib2/src").to_str().unwrap()]);
    assert!(output.status.success(), "{}", combined(&output));
    assert!(
        combined(&output).contains("1 passed, 0 failed"),
        "{}",
        combined(&output)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_test_with_no_manifest_keeps_its_old_context() {
    // The control: a lone `*_test.vl` under no package still compiles and runs,
    // rooted at its own directory — the behavior every existing `std::assert`
    // test relies on.
    let dir = temp_project("test_bare");
    write(
        &dir,
        "lonely_test.vl",
        &test_source("", "1 + 1 == 2", "arithmetic still works"),
    );

    let output = vilan(&["test", dir.to_str().unwrap()]);
    assert!(output.status.success(), "{}", combined(&output));
    assert!(
        combined(&output).contains("1 passed, 0 failed"),
        "{}",
        combined(&output)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ── a broken INHERITED declaration names the manifest that wrote it
// (proposal/distribution.md §7's S5 residual) ──

#[test]
fn a_broken_inherited_declaration_names_the_projects_manifest() {
    // The member opted in and did nothing else wrong; the path that does not
    // resolve is written in the project root's `[project.dependencies]`. The
    // CLI has to say so, or the user opens the wrong file.
    let dir = temp_project("inherit_broken");
    write(
        &dir,
        "vilan.toml",
        "[project]\npackages = [\"app\"]\n[project.dependencies]\n\
         shapes = { path = \"nowhere\" }\n",
    );
    write(
        &dir,
        "app/vilan.toml",
        "[package]\nname = \"app\"\n[package.dependencies]\n\
         shapes = { project = true }\n",
    );
    write(
        &dir,
        "app/src/main.vl",
        "import shapes::area;\nfun main() {}\n",
    );

    let output = vilan(&["build", dir.join("app").to_str().unwrap()]);
    assert!(!output.status.success(), "expected a resolution failure");
    let text = combined(&output);
    assert!(
        text.contains(&format!(
            "inherited from {}",
            dir.join("vilan.toml").display()
        )),
        "{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_broken_member_declaration_names_no_one_else() {
    // The control: the member wrote the failing declaration itself, so the
    // message stays exactly as it was.
    let dir = temp_project("member_broken");
    write(&dir, "vilan.toml", "[project]\npackages = [\"app\"]\n");
    write(
        &dir,
        "app/vilan.toml",
        "[package]\nname = \"app\"\n[package.dependencies]\n\
         shapes = { path = \"../nowhere\" }\n",
    );
    write(
        &dir,
        "app/src/main.vl",
        "import shapes::area;\nfun main() {}\n",
    );

    let output = vilan(&["build", dir.join("app").to_str().unwrap()]);
    assert!(!output.status.success(), "expected a resolution failure");
    let text = combined(&output);
    assert!(text.contains("dependency `shapes`"), "{text}");
    assert!(!text.contains("inherited from"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

// ── cross-package initialization order (B33; proposal/b33-emission-order.md §1)
//
// The canonical fallback order was pinned within a single package by the corpus
// (`emitted_js_is_independent_of_import_order`), and probed but never pinned
// ACROSS packages — b33-emission-order.md's shipped-status residual, which needs
// exactly the workspace fixture this file already knows how to build.

/// A workspace whose module-level bindings exercise every tier of the canonical
/// order at once, with nothing depending on anything else — so the emitted
/// sequence is the canonical fallback and not a dependency topo-sort.
///
/// The dependency graph is deliberately NOT alphabetical: `app` declares both
/// `alpha` and `zed`, and `alpha` itself depends on `zed`, so the post-order DFS
/// gives `zed` package index 0 and `alpha` index 1 — the reverse of the
/// manifest's (BTreeMap, i.e. alphabetical) listing. A fixture where the two
/// agree would pass under either rule and pin neither.
fn write_ordered_workspace(dir: &Path, entry: &str) {
    write(dir, "zed/vilan.toml", "[library]\nname = \"zed\"\n");
    write(dir, "zed/src/lib.vl", "let Z_ROOT: i32 = 1;\n");
    // Two bindings in one file: declaration order within a module.
    write(
        dir,
        "zed/src/zmod.vl",
        "let Z_ONE: i32 = 2;\n\nlet Z_TWO: i32 = 3;\n",
    );
    write(
        dir,
        "alpha/vilan.toml",
        "[library]\nname = \"alpha\"\n\n[library.dependencies]\nzed = { path = \"../zed\" }\n",
    );
    write(dir, "alpha/src/lib.vl", "let A_ROOT: i32 = 4;\n");
    write(dir, "alpha/src/amod.vl", "let A_MOD: i32 = 5;\n");
    write(
        dir,
        "app/vilan.toml",
        "[package]\nname = \"app\"\n\n[package.dependencies]\n\
         alpha = { path = \"../alpha\" }\nzed = { path = \"../zed\" }\n",
    );
    // The entry package's own modules, named so that name order (`beta` before
    // `gamma`) is what decides — not the order the entry imports them.
    write(dir, "app/src/beta.vl", "let OWN_B: i32 = 6;\n");
    write(dir, "app/src/gamma.vl", "let OWN_G: i32 = 7;\n");
    write(dir, "app/src/main.vl", entry);
}

/// The entry file, parameterized by the order its imports are written in. The
/// body reads every binding (through an interpolated string), so none of them is
/// tree-shaken and all of them must be emitted.
fn ordered_entry(imports: &[&str]) -> String {
    format!(
        "{}\n\nlet ENTRY: i32 = 8;\n\nfun main() {{\n\t\
         print(i\"{{PI}} {{A_ROOT}} {{A_MOD}} {{Z_ROOT}} {{Z_ONE}} {{Z_TWO}} \
         {{OWN_B}} {{OWN_G}} {{ENTRY}}\")\n}}\n",
        imports.join("\n")
    )
}

/// The imports of [`ordered_entry`], in one spelling.
fn ordered_imports() -> Vec<&'static str> {
    vec![
        "import std::print;",
        "import std::math::PI;",
        "import alpha::A_ROOT;",
        "import alpha::amod::A_MOD;",
        "import zed::Z_ROOT;",
        "import zed::zmod::Z_ONE;",
        "import zed::zmod::Z_TWO;",
        "import pkg::beta::OWN_B;",
        "import pkg::gamma::OWN_G;",
    ]
}

/// The names of the `const` declarations in an emitted bundle, in emission
/// order.
fn emitted_constant_names(javascript: &str) -> Vec<String> {
    javascript
        .lines()
        .filter_map(|line| line.strip_prefix("const "))
        .filter_map(|rest| rest.split(' ').next())
        .map(str::to_string)
        .collect()
}

#[test]
fn cross_package_initialization_follows_the_canonical_tier_order() {
    // Spec §7.1's canonical order, across every tier at once: std modules, then
    // each dependency package's modules in dependency-graph post-order, then the
    // entry package's own modules by name, then the dependency packages' ROOT
    // modules (`lib.vl`) in that same package order, then the entry file —
    // declaration order within each file.
    let dir = temp_project("init_order");
    write_ordered_workspace(&dir, &ordered_entry(&ordered_imports()));
    let output = vilan(&["build", "--stdout", dir.join("app").to_str().unwrap()]);
    assert!(output.status.success(), "{}", combined(&output));
    let javascript = String::from_utf8_lossy(&output.stdout).into_owned();
    assert_eq!(
        emitted_constant_names(&javascript),
        vec![
            "PI",     // std module (tier 0)
            "Z_ONE",  // dependency package 0 (`zed`), declaration order …
            "Z_TWO",  // … within its module
            "A_MOD",  // dependency package 1 (`alpha`) — post-order, not `a` < `z`
            "OWN_B",  // the entry package's own modules, by module name …
            "OWN_G",  // … `beta` before `gamma`
            "Z_ROOT", // the dependency ROOT modules, in the same package order …
            "A_ROOT", // … `zed` (0) before `alpha` (1)
            "ENTRY",  // the entry file, last
        ],
        "{javascript}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cross_package_emission_is_byte_identical_under_import_permutation() {
    // The point of the canonical order: the emitted bundle is a function of
    // which modules are reachable, never of how the entry spells its imports —
    // across packages, which is where the naive orders (id, name) diverge.
    let straight = temp_project("init_order_a");
    write_ordered_workspace(&straight, &ordered_entry(&ordered_imports()));
    let output = vilan(&["build", "--stdout", straight.join("app").to_str().unwrap()]);
    assert!(output.status.success(), "{}", combined(&output));
    let first = output.stdout.clone();
    let _ = std::fs::remove_dir_all(&straight);

    let mut permuted = ordered_imports();
    permuted.reverse();
    let shuffled = temp_project("init_order_b");
    write_ordered_workspace(&shuffled, &ordered_entry(&permuted));
    let output = vilan(&["build", "--stdout", shuffled.join("app").to_str().unwrap()]);
    assert!(output.status.success(), "{}", combined(&output));
    assert_eq!(
        String::from_utf8_lossy(&first),
        String::from_utf8_lossy(&output.stdout),
        "reordering the entry's imports changed the emitted bundle"
    );
    let _ = std::fs::remove_dir_all(&shuffled);
}
