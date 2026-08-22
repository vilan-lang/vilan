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

/// The playground's stance on bundle splitting (`bundle-splitting.md` §S4,
/// item 8): `split` cannot reach here, and a program the SPLITTER would
/// recognize must still compile to one string.
///
/// There is nowhere for the flag to be written — the playground hand-builds its
/// package spec and has no `vilan.toml`, so no `[entry.<name>]` table exists —
/// and `compile_program` calls the single-file emitter, never `transform_split`.
/// That is not an accident to be tidied up later but the shape the split was
/// designed around: the playground runs its output in an opaque-origin `srcdoc`
/// iframe where a relative `import()` cannot resolve, which is why cross-chunk
/// references ride a runtime registry rather than ESM in the first place.
///
/// So the pin is on the OUTPUT: a router-shaped program — a `swap` over a route
/// enum's `match`, exactly the shape `chunks::plan` recognizes — comes back as
/// one `js` string with no chunk machinery in it. Recognizing a split point can
/// never start changing what the playground emits.
#[test]
fn a_splittable_route_match_still_compiles_to_one_playground_bundle() {
    let output = compile(
        "import std::reactive::Signal;\n\
         import std::ui::{ View, mount_root, view };\n\
         \n\
         [derive(PartialEq)]\n\
         enum Route {\n\
         \tHome,\n\
         \tAway,\n\
         }\n\
         \n\
         fun home_page(): View {\n\
         \tview(\"h1\").text(\"home\")\n\
         }\n\
         \n\
         fun away_page(): View {\n\
         \tview(\"h1\").text(\"away\")\n\
         }\n\
         \n\
         fun main() {\n\
         \tlet route: Signal<Route> = Signal::new(Route::Home);\n\
         \tlet _root = mount_root(\"app\", || view(\"main\").swap(route, |current| match current {\n\
         \t\tRoute::Home => home_page(),\n\
         \t\tRoute::Away => away_page(),\n\
         \t}));\n\
         }\n",
    );
    assert!(
        output.diagnostics.is_empty(),
        "the router shape must compile in the playground, got: {:#?}",
        output.diagnostics
    );
    let js = output.js.expect("expected emitted JavaScript");
    // One string, whole: both pages are declared in it…
    assert!(
        js.contains("function home_page(") && js.contains("function away_page("),
        "a playground bundle carries every route: {js}"
    );
    // …and none of the split's machinery is anywhere near it.
    for machinery in ["__vilan_chunks", "__chunk_registry", "__chunk_load"] {
        assert!(
            !js.contains(machinery),
            "the playground must never emit {machinery}"
        );
    }
}

/// The stance above, guarded at its cause rather than at its symptom: the
/// playground's compile path must never reach the split emitter.
///
/// The output pin alone cannot see this. `chunks::plan` recognizes nothing in
/// the playground anyway — `embedded_std_spec` hand-builds its package spec and
/// leaves `Program::std_sources` EMPTY, so `View` does not read as std-resident
/// and the `swap` recognizer finds no site — which means swapping `transform`
/// for `transform_split` here would today produce the same single string and
/// pass unnoticed. It would also be a trap: the residence rules in `chunks.rs`
/// ("std is never chunked") all read the other way under an empty
/// `std_sources`, so the day that spec learns to mark std, a playground wired
/// to the split emitter would start chunking the standard library.
#[test]
fn the_playground_compile_path_never_calls_the_split_emitter() {
    let source = include_str!("../src/lib.rs");
    assert!(
        !source.contains("transform_split"),
        "`split` is a `vilan build` decision: the playground has no manifest to \
         declare it in, a single-string `CompileResult` to carry it, and an \
         opaque-origin srcdoc frame that cannot resolve a chunk's relative import"
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

/// The zero-based line and column of the `occurrence`th (0-based) hit of
/// `snippet` in an ASCII `source` — the page's units for a diagnostic's
/// position, computed independently of the compiler's own index.
fn position_of(source: &str, snippet: &str, occurrence: usize) -> (u32, u32) {
    let mut start = 0;
    let mut at = 0;
    for _ in 0..=occurrence {
        at = start + source[start..].find(snippet).expect("the snippet occurs");
        start = at + 1;
    }
    let before = &source[..at];
    let line = before.matches('\n').count() as u32;
    let column = before.rsplit('\n').next().unwrap_or("").len() as u32;
    (line, column)
}

/// E80: the requirement trace (E78) crosses the playground's wire. The
/// owner's acceptance example — `c`'s `context.run` covers its `a()`, every
/// other path to the read is bare — yields ONE error whose `trace` carries
/// `main`'s `b()` and `b`'s `a()`, entry → read, each a call hop located in
/// the visitor's file in the page's units; the covered call is absent. It
/// carries no note: the trace is a channel beside the note, not inside it.
#[test]
fn e80_the_owners_example_carries_its_requirement_trace_on_the_wire() {
    let source = "import std::context::Context;\n\n\
        let context: Context<u32> = Context::new();\n\n\
        fun a() {\n    context.get();\n}\n\n\
        fun b() {\n    a();\n}\n\n\
        fun c() {\n    context.run(0, || a());\n}\n\n\
        fun main() {\n    b();\n    c();\n}\n";
    let output = compile(source);
    assert!(output.js.is_none(), "a failed compile emits no JavaScript");
    let errors: Vec<_> = output
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == "error")
        .collect();
    assert_eq!(errors.len(), 1, "one refusal, at the read: {errors:#?}");
    let refusal = errors[0];
    assert!(
        refusal
            .message
            .contains("can be reached without an enclosing `run`"),
        "{refusal:#?}"
    );
    assert_eq!(refusal.file, "main.vl");
    assert_eq!(
        (refusal.line, refusal.column),
        position_of(source, "context.get()", 0),
        "the primary sits at the read"
    );
    assert_eq!(refusal.note, None, "the owner's example carries no C3 note");
    // Occurrence 1 of each call skips the function's own declaration.
    let expected: Vec<(u32, u32)> =
        vec![position_of(source, "b()", 1), position_of(source, "a()", 1)];
    let located: Vec<(u32, u32)> = refusal
        .trace
        .iter()
        .map(|hop| (hop.line, hop.column))
        .collect();
    assert_eq!(located, expected, "entry → read: {:#?}", refusal.trace);
    for hop in &refusal.trace {
        assert_eq!(hop.file, "main.vl", "a hop in the visitor's file names it");
        assert_eq!(
            hop.message,
            "the context requirement flows through this call"
        );
        assert!(
            hop.call,
            "both entries are call hops; no tail under the cap"
        );
    }
}

/// The C3 note crosses the wire beside the message: an arity mismatch notes
/// the callee's declaration (`call_argument_count_notes_the_callees_declaration`
/// in core's harness), and that note is what the page's `note:` line shows.
/// A noted diagnostic carries no trace — the two channels are independent.
#[test]
fn a_declared_here_note_crosses_the_wire_beside_the_message() {
    let output = compile(
        "fun distance(x: i32, y: i32): i32 {\n    x + y\n}\n\n\
         fun main() {\n    distance(3);\n}\n",
    );
    let arity = output
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic
                .message
                .contains("`distance` expects 2 arguments")
        })
        .expect("the arity diagnostic is reported");
    assert_eq!(
        arity.note.as_deref(),
        Some("`distance` is declared here"),
        "{arity:#?}"
    );
    assert!(arity.trace.is_empty(), "a note is not a trace: {arity:#?}");
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

// --- compile_for: the server check mode's contract ---------------------------

fn compile_for_node(source: &str) -> CompileOutput {
    let _guard = compiler();
    vilan_wasm::compile_program_for(source, vilan_core::Platform::default())
}

const SERVER_PROGRAM: &str = "import std::http::{ Response, Server };\n\
    import std::print;\n\n\
    async fun main() {\n\
    \tServer::builder().port(3000).on_request(|request| {\n\
    \t\tResponse::builder().body(\"ok\").build()\n\
    \t}).on_start(|server| print(server.url())).build().start();\n\
    }\n";

#[test]
fn a_server_program_checks_clean_for_node() {
    let output = compile_for_node(SERVER_PROGRAM);
    assert!(
        output.diagnostics.is_empty(),
        "a process-leg program must check clean under the node platform, got: {:#?}",
        output.diagnostics
    );
    assert!(
        output.js.is_some(),
        "the node leg emits a real program even though the page never runs it"
    );
}

#[test]
fn a_server_program_is_rejected_for_the_browser() {
    let output = compile(SERVER_PROGRAM);
    assert!(
        output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("cannot run on `browser`")),
        "the browser build must reject std::http with the platform-coloring \
         diagnostic, got: {:#?}",
        output.diagnostics
    );
}

#[test]
fn a_browser_program_is_rejected_for_node() {
    // The rejection comes from NAME resolution, not platform coloring:
    // std::ui resolves to its process twin under node, and that twin never
    // declares `mount` — the name is absent rather than fenced.
    let output = compile_for_node(
        "import std::ui::{ mount, view };\n\nfun main() {\n\tmount(\"app\", view(\"p\").text(\"hi\"));\n}\n",
    );
    assert!(
        output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("cannot find 'mount'")),
        "a DOM program must reject under the node platform, got: {:#?}",
        output.diagnostics
    );
}

/// A diagnostic is labeled with the file it is IN, even when the visitor's own
/// program also failed to parse (backlog E42).
///
/// `analyze_source` hands back one flat list — the entry's own lex/parse errors
/// first, then the program's — while the per-diagnostic file record
/// (`Program::diagnostic_sources`) covers only the program's half. Indexing
/// that record with the FLAT position shifted every attribution by the number
/// of parse errors ahead of it, so the visitor's syntax error came back labeled
/// with a standard-library file. The language server subtracts the prefix; so
/// does this now.
///
/// A single-file playground compile normally attributes everything to
/// `main.vl`, which is precisely why the shift went unnoticed — with one file
/// the wrong index still lands on the right answer. The pin registers an extra
/// TOOLCHAIN module so the program has a second file to be wrong about. That is
/// also the only coverage `display_path`'s toolchain branch has: `Diagnostic`
/// promises "a toolchain path for a diagnostic inside std", and nothing else
/// here produces one.
///
/// The injected key is not one of `vilan_embedded_std::FILES`, so `boot()` —
/// which re-registers every embedded file on every compile — neither clobbers
/// it nor is clobbered by it, and no other test imports the module.
#[test]
fn a_diagnostic_after_a_parse_error_keeps_its_own_file() {
    let _guard = compiler();
    vilan_wasm::boot();
    vilan_core::analyzer::set_document_overlay(
        std::path::Path::new("/toolchain/std/src/e42_probe.vl"),
        // One ANALYZER error, so it lands in the program's half of the list
        // rather than in the entry's parse prefix.
        Some("fun probe(): i32 {\n    let bad: i32 = \"text\";\n    1\n}\n".to_string()),
    );
    // Two parse errors in the entry (the prefix the attribution has to lose),
    // and one analyzer error that belongs to the injected module.
    let output = vilan_wasm::compile_program(
        "import std::e42_probe::probe;\n\nfun main() {\n    let value = probe();\n}\n\n@\n@\n",
    );
    let files: Vec<&str> = output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.file.as_str())
        .collect();
    assert_eq!(
        files,
        vec!["main.vl", "main.vl", "std/src/e42_probe.vl"],
        "each diagnostic must name its own file: {:#?}",
        output.diagnostics
    );
    // Spelled out, because the shift is what the assertion above is really
    // about: the entry's syntax errors are the prefix, and they are the
    // visitor's own.
    for diagnostic in output.diagnostics.iter().take(2) {
        assert!(
            diagnostic.message.contains("expected a token"),
            "the prefix must be the entry's parse errors: {diagnostic:#?}"
        );
    }
    let last = output.diagnostics.last().expect("three diagnostics");
    assert!(
        last.message.contains("Expected i32, but got str"),
        "the program's half must be the module's type error: {last:#?}"
    );
}
