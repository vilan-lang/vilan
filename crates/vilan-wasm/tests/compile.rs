//! The playground compiler, exercised natively (D11 S2).
//!
//! These run on the host, against the same code the wasm artifact contains —
//! the `wasm_bindgen` layer is a type conversion with no decisions in it, so
//! the wasm target only has to prove the thing builds. Everything here compiles
//! with **no filesystem behind it**: the toolchain and the program both live in
//! the document overlay, which is exactly the browser's situation.
//!
//! One caveat these share with the real instance: core's caches and the overlay
//! are process-global, so these tests must not run a second compile
//! concurrently with a first. `cargo test` gives each test its own thread but
//! one process, and every test here registers the SAME `/project/main.vl` key —
//! so they are serialized on one mutex rather than left to race.

use std::sync::{Mutex, MutexGuard, OnceLock};

use vilan_wasm::{CompileOutput, compile_program};

/// Serializes the tests: they share `/project/main.vl` and core's global
/// caches, so two at once would compile each other's source.
fn compiler() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn compile(source: &str) -> CompileOutput {
    let _guard = compiler();
    compile_program(source)
}

#[test]
fn a_hello_program_compiles_with_no_filesystem() {
    let output = compile("import std::print;\nfun main() { print(42); }\n");
    assert!(
        output.diagnostics.is_empty(),
        "expected a clean compile, got: {:#?}",
        output.diagnostics
    );
    let js = output.js.expect("expected emitted JavaScript");
    assert!(
        js.contains("42"),
        "the emitted program should carry its literal: {js}"
    );
}

/// The point of the whole slice: `import std::print` resolves out of the
/// embedded toolchain, through the overlay, with nothing on disk.
#[test]
fn a_std_import_resolves_from_the_embedded_toolchain() {
    let output = compile("import std::math::min;\nfun main() { let a = min(1, 2); }\n");
    assert!(
        output.diagnostics.is_empty(),
        "expected std::math to resolve from the overlay, got: {:#?}",
        output.diagnostics
    );
}

/// The browser layer specifically — this is the platform the playground
/// compiles for, and it resolves through a LAYER root rather than the base.
#[test]
fn a_browser_layer_import_resolves() {
    let output = compile("import std::ui::view;\nfun main() { let v = view(\"div\"); }\n");
    assert!(
        output.diagnostics.is_empty(),
        "expected the browser layer to resolve, got: {:#?}",
        output.diagnostics
    );
}

/// The diagnostics are the pitch, so their shape is pinned: a real message, a
/// position in the visitor's own file, and a file name that does not leak the
/// synthetic root.
#[test]
fn a_type_error_reports_a_position_in_the_visitors_file() {
    let output = compile("fun main() {\n    let x: i32 = \"text\";\n}\n");
    assert!(output.js.is_none(), "a failed compile emits no JavaScript");
    let first = output
        .diagnostics
        .first()
        .expect("expected at least one diagnostic");
    assert_eq!(first.severity, "error");
    assert_eq!(first.file, "main.vl", "the synthetic root must not leak");
    assert_eq!(
        first.line, 1,
        "the error is on the second line (zero-based)"
    );
    assert!(
        !first.message.is_empty(),
        "a diagnostic must carry its message"
    );
}

/// A parse error must degrade to a diagnostic, never a panic — in the browser a
/// panic takes down the instance rather than one request.
#[test]
fn a_syntax_error_degrades_to_a_diagnostic() {
    let output = compile("fun main( {\n");
    assert!(output.js.is_none());
    assert!(
        !output.diagnostics.is_empty(),
        "a broken parse must still produce diagnostics"
    );
}

/// Compile-time styling emits a stylesheet beside the JavaScript, which the
/// page needs as a separate artifact.
#[test]
fn compile_time_styles_come_back_as_css() {
    let output = compile(
        "import std::style::style;\n\
         let card = const style().raw(\"color\", \"red\");\n\
         fun main() { let classes = card.class_list(); }\n",
    );
    assert!(
        output.diagnostics.is_empty(),
        "expected a clean compile, got: {:#?}",
        output.diagnostics
    );
    let css = output.css.expect("expected a stylesheet");
    assert!(
        css.contains("red"),
        "the stylesheet should carry the declared value: {css}"
    );
}

/// Boot is idempotent and the caches are content-addressed, so compiling twice
/// gives the same answer — the instance is reusable, which is what lets the
/// page keep one alive across runs.
#[test]
fn compiling_twice_gives_the_same_result() {
    let source = "import std::print;\nfun main() { print(7); }\n";
    let first = compile(source);
    let second = compile(source);
    assert_eq!(
        first.js, second.js,
        "a repeated compile of identical source must be identical"
    );
    assert!(first.diagnostics.is_empty() && second.diagnostics.is_empty());
}

/// A later compile must not see an earlier one's program: the overlay entry for
/// the entry file is replaced, not merged.
#[test]
fn a_second_compile_replaces_the_first_program() {
    let first = compile("import std::print;\nfun main() { print(1); }\n");
    let second = compile("import std::print;\nfun main() { print(2); }\n");
    assert!(first.diagnostics.is_empty());
    assert!(
        second.diagnostics.is_empty(),
        "expected a clean second compile, got: {:#?}",
        second.diagnostics
    );
    let js = second.js.expect("expected emitted JavaScript");
    assert!(
        js.contains('2') && !js.contains('1'),
        "the second compile must not carry the first program: {js}"
    );
}

/// The hand-built `PackageSpec` is derived from a manifest that ships in
/// `FILES`, so it can drift from it. This compares the two: the layer names and
/// their platform tokens must still be what `embedded_std_spec` hard-codes.
#[test]
fn the_hand_built_std_spec_matches_the_manifest() {
    let manifest = vilan_embedded_std::FILES
        .iter()
        .find(|(key, _)| *key == "std/vilan.toml")
        .map(|(_, contents)| *contents)
        .expect("the embedded toolchain must carry std's manifest");

    for (layer, token) in [("browser", "browser"), ("process", "@process")] {
        assert!(
            manifest.contains(&format!("[library.layer.{layer}]")),
            "embedded_std_spec hard-codes a `{layer}` layer the manifest no longer declares"
        );
        assert!(
            manifest.contains(token),
            "the `{layer}` layer's platform token `{token}` is no longer in the manifest"
        );
    }
    // A third layer would be resolved by every other front-end and silently
    // missing here. Counted over section HEADERS, not substring occurrences —
    // the manifest's own header comment says "[library.layer.<name>]" in prose.
    let declared = manifest
        .lines()
        .filter(|line| line.trim_start().starts_with("[library.layer."))
        .count();
    assert_eq!(
        declared, 2,
        "std declares a layer `embedded_std_spec` does not build"
    );
    // No `root` override: the hard-coded roots assume the `src/<name>` default.
    assert!(
        !manifest.contains("root ="),
        "std's manifest sets a root override the hard-coded spec ignores"
    );
}

/// The version the page badges is the toolchain's, not something invented.
#[test]
fn the_reported_version_is_the_crate_version() {
    assert_eq!(vilan_wasm::version(), env!("CARGO_PKG_VERSION"));
}

/// A compile leaks one `'static` copy of its entry text (`analyze_source`
/// borrows for `'static`) — interned by content and tallied at
/// `WasmEntryText`: recompiling identical source must reuse the first leak,
/// and the leak must be visible to the tally at all (the E23 sweep found this
/// site both unbounded per compile and untallied). The counters are
/// thread-local, so this thread sees exactly its own compiles; the source is
/// unique to this test, so the process-global intern cannot be pre-warmed by
/// a neighbor.
#[test]
fn recompiling_identical_source_interns_the_entry_text() {
    use vilan_core::leak_tally::{self, LeakSite};

    let source = "import std::print;\nfun main() { print(41047); }\n";
    leak_tally::reset();
    let first = compile(source);
    assert!(
        first.diagnostics.is_empty(),
        "expected a clean compile, got: {:#?}",
        first.diagnostics
    );
    assert_eq!(
        leak_tally::bytes(LeakSite::WasmEntryText),
        source.len(),
        "the first compile of a distinct source must leak (and tally) exactly \
         one copy of it"
    );
    leak_tally::reset();
    let second = compile(source);
    assert_eq!(
        first.js, second.js,
        "a repeated compile of identical source must be identical"
    );
    assert_eq!(
        leak_tally::bytes(LeakSite::WasmEntryText),
        0,
        "recompiling identical source re-leaked the entry text — the intern is \
         not deduping"
    );
}

// --- format: the fmt button's contract ---------------------------------------

#[test]
fn a_misindented_program_formats_to_the_canonical_layout() {
    let formatted = vilan_wasm::format_program("fun main() {\n      let a = 1;\n\tprint(a);\n}\n");
    assert_eq!(
        formatted, "fun main() {\n\tlet a = 1;\n\tprint(a);\n}\n",
        "format must canonicalize indentation the way `vilan fmt` does"
    );
    assert_eq!(
        vilan_wasm::format_program(&formatted),
        formatted,
        "formatting must be idempotent"
    );
}

#[test]
fn a_program_that_does_not_parse_formats_to_itself() {
    let broken = "fun main( {\n   let a = ;\n";
    assert_eq!(
        vilan_wasm::format_program(broken),
        broken,
        "a bail must return the original bytes untouched — a file the \
         formatter does not understand is not one to rewrite"
    );
}
