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

/// Runs `work` on a thread with the same 256 MiB stack every vilan-core
/// test harness spawns: the analyzer's recursion on a real program can
/// overflow libtest's ~2 MiB worker thread — the v0.36.0 release gate
/// aborted (SIGABRT) exactly one margin short of a local pass — while the
/// SHIPPED wasm links with a 16 MiB stack (release.yml's `-zstack-size`),
/// so the test thread was the only under-provisioned host. `RETAINED` is
/// thread-local, so a compile and the completion that reads it must ride
/// ONE spawn — wrap whole bodies, never per call.
///
/// Measured since (B138, `VILAN_DEPTH_STATS`): the analyses these tests run
/// peak under 1 MiB of stack unoptimized — what closed the CI margin was the
/// expression walk's ~36 KiB-per-nesting-level frames, depth-bounded at 500
/// levels now, as are the return-inference chain (B139) and the parser itself
/// (B142). The 256 MiB here matches the vilan-core harness convention, not a
/// measured need of these fixtures — the SHIPPED margins are sized from the
/// bounds and are much smaller (`COMPILER_STACK_SIZE`, release.yml's
/// `-zstack-size`).
fn on_big_stack<T: Send>(work: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn_scoped(scope, work)
            .expect("spawn the big-stack test thread")
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}

fn compile(source: &str) -> CompileOutput {
    let _guard = compiler();
    on_big_stack(|| compile_program(source))
}

#[test]
fn a_hello_program_compiles_with_no_filesystem() {
    let output = compile("import std::io::print;\nfun main() { print(42); }\n");
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

/// The point of the whole slice: `import std::io::print` resolves out of the
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
        "import std::reactive::{ Signal, SignalCell };\n\
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
         \tlet route: SignalCell<Route> = Signal::new(Route::Home);\n\
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
    let source = "import std::io::print;\nfun main() { print(7); }\n";
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
    let first = compile("import std::io::print;\nfun main() { print(1); }\n");
    let second = compile("import std::io::print;\nfun main() { print(2); }\n");
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

    let source = "import std::io::print;\nfun main() { print(41047); }\n";
    // The tally is THREAD-LOCAL: every reset, both compiles, and every read
    // ride one spawn — `compile()` would nest a second thread and lose it.
    let _guard = compiler();
    on_big_stack(|| {
        leak_tally::reset();
        let first = compile_program(source);
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
        let second = compile_program(source);
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
    });
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
    on_big_stack(|| vilan_wasm::compile_program_for(source, vilan_core::Platform::default()))
}

const SERVER_PROGRAM: &str = "import std::http::{ Response, Server };\n\
    import std::io::print;\n\n\
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
    // and one analyzer error that belongs to the injected module. `$` is the
    // un-lexable byte, deliberately: it is in no charset and carries no
    // curated rule, so the prefix stays the GENERIC "expected a token" this
    // test asserts on. (`@` used to serve here and no longer can — it names
    // the `css` block's at-rule refusal now, proposal/css-block.md §4.1.)
    let output = on_big_stack(|| {
        vilan_wasm::compile_program(
            "import std::e42_probe::probe;\n\nfun main() {\n    let value = probe();\n}\n\n$\n$\n",
        )
    });
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

// --- complete: the playground's completion contract (K9) --------------------
//
// `complete_program` answers from the analysis the LAST compile retained, on
// this thread (`proposal/playground-completion.md` §5). Each pin below compiles
// first, under the same serialization the compile pins use, then completes —
// usually on a DIFFERENT text than it compiled, because that is the shape of
// every real request: the visitor has just typed the character that triggered
// it, and the debounced check has not landed yet.

use vilan_wasm::CompletionItem;

fn complete_after(compiled: &str, live: &str, line: u32, character: u32) -> Vec<CompletionItem> {
    let _guard = compiler();
    on_big_stack(|| {
        compile_program(compiled);
        vilan_wasm::complete_program(live, line, character)
    })
}

fn labels(items: &[CompletionItem]) -> Vec<&str> {
    items.iter().map(|item| item.label.as_str()).collect()
}

fn named<'a>(items: &'a [CompletionItem], label: &str) -> &'a CompletionItem {
    items
        .iter()
        .find(|item| item.label == label)
        .unwrap_or_else(|| panic!("no `{label}` offered: {:?}", labels(items)))
}

const POINT_PROGRAM: &str = "struct Point {\n\
    \tx: i32,\n\
    \ty: i32,\n\
    }\n\
    \n\
    impl Point {\n\
    \t/// The point's size.\n\
    \tfun size(self): i32 {\n\
    \t\tself.x + self.y\n\
    \t}\n\
    }\n\
    \n\
    fun main() {\n\
    \tlet p = Point { x = 1, y = 2 };\n\
    \tp\n\
    }\n";

/// The headline: a `.` typed after a receiver offers its fields and methods,
/// with the method call-shaped and carrying its signature and doc — the
/// language server's answer, from the retained analysis, on a buffer the
/// analysis has not seen (the `.` itself is new).
#[test]
fn member_completion_answers_from_the_retained_analysis() {
    let live = POINT_PROGRAM.replace("\tp\n}", "\tp.\n}");
    let items = complete_after(POINT_PROGRAM, &live, 14, 3);
    let offered = labels(&items);
    for expected in ["x", "y", "size"] {
        assert!(
            offered.contains(&expected),
            "{expected} missing: {offered:?}"
        );
    }
    let size = named(&items, "size");
    assert_eq!(size.kind, "method");
    assert_eq!(size.insert, "size()$0", "a zero-parameter call shape");
    assert!(size.is_snippet, "the `$0` cursor makes it a snippet");
    assert_eq!(size.boost, 0, "an in-scope candidate sits in the top band");
    assert!(
        size.detail
            .as_deref()
            .is_some_and(|detail| detail.contains("size(self)")),
        "the signature rides as detail: {:?}",
        size.detail
    );
    assert_eq!(size.documentation.as_deref(), Some("The point's size."));
    assert_eq!(named(&items, "x").kind, "field");
    assert!(
        !named(&items, "x").is_snippet,
        "a field inserts its bare name"
    );
}

/// Import-path completion with no filesystem: `import std::` enumerates the
/// embedded toolchain's modules out of the document overlay (the
/// `modules_in_root` overlay listing, pinned in core), plus the names std's
/// `lib.vl` surface publishes; `import std::json::` reads the module's own
/// importables through the loader's overlay-aware read.
#[test]
fn import_path_completion_enumerates_the_embedded_toolchain() {
    let compiled = "import std::json::encode_json;\n\nfun main() {}\n";
    let live = "import std::json::encode_json;\nimport std::\n\nfun main() {}\n";
    let items = complete_after(compiled, live, 1, 12);
    let offered = labels(&items);
    for module in ["json", "math", "option", "list"] {
        assert!(
            offered.contains(&module),
            "std::{module} missing: {offered:?}"
        );
    }
    // std's `lib.vl` surface publishes NOTHING since the alias sweep
    // (prelude.md §10.2). The two prelude modules are ordinary modules of the
    // embedded toolchain and enumerate as such.
    assert!(
        !offered.contains(&"print"),
        "`std::print` was removed; `print` lives at `std::io::print`: {offered:?}"
    );
    for module in ["prelude", "web", "io"] {
        assert!(
            offered.contains(&module),
            "std::{module} missing: {offered:?}"
        );
    }
    assert!(
        !offered.contains(&"lib"),
        "the surface file is not a module: {offered:?}"
    );
    assert_eq!(named(&items, "json").kind, "module");

    let live = "import std::json::encode_json;\nimport std::json::\n\nfun main() {}\n";
    let items = complete_after(compiled, live, 1, 18);
    let offered = labels(&items);
    for importable in ["json_codec", "decode_json"] {
        assert!(
            offered.contains(&importable),
            "std::json::{importable} missing: {offered:?}"
        );
    }
    assert!(
        !named(&items, "json_codec").is_snippet,
        "an import binds a name, it never calls it"
    );
}

/// The snippet forms, as the page receives them: a construct snippet's
/// tab-stopped body in the lowest band, a call-shaped function with its
/// parameters as named tab-stops (the server's `Full` default), and a plain
/// keyword as plain text.
#[test]
fn construct_snippets_and_call_shapes_come_back_as_snippets() {
    let program = "import std::io::print;\n\nfun main() {\n\t\n}\n";
    let items = complete_after(program, program, 3, 1);
    let for_snippet = named(&items, "for … in { }");
    assert_eq!(for_snippet.kind, "snippet");
    assert!(for_snippet.is_snippet);
    assert_eq!(for_snippet.insert, "for ${1:item} in ${2:items} {\n\t$0\n}");
    assert_eq!(
        for_snippet.boost, -9,
        "a construct snippet sorts below every name"
    );
    assert_eq!(
        for_snippet.detail.as_deref(),
        Some("iterate over a collection")
    );
    let print = named(&items, "print");
    assert_eq!(print.kind, "function");
    assert_eq!(print.insert, "print(${1:message})$0");
    assert!(print.is_snippet);
    assert_eq!(print.boost, 0);
    let keyword = named(&items, "let");
    assert_eq!(keyword.kind, "keyword");
    assert_eq!(keyword.insert, "let");
    assert!(!keyword.is_snippet);
    assert_eq!(keyword.boost, 0);
}

/// The stale-buffer rule (E52), on the playground's side of the fence: the
/// live text is shorter than the analyzed one before the cursor (a line above
/// was shortened), and the `.` is typed in `b`, whose `p` is an `Other`. The
/// cursor converts to the analyzed text through LINE/CHARACTER, landing in
/// `b`; as a byte offset it would land back inside `a`, resolve `a`'s `p`, and
/// offer `Point`'s `x` instead.
#[test]
fn a_stale_buffer_maps_through_line_and_character_not_bytes() {
    let padding = "a".repeat(80);
    let compiled = format!(
        "struct Point {{\n\tx: i32,\n}}\nstruct Other {{\n\ty: i32,\n}}\nfun a() {{\n\
         \tlet p = Point {{ x = 1 }};\n\tlet padding = \"{padding}\";\n}}\nfun b() {{\n\
         \tlet p = Other {{ y = 2 }};\n\tp\n}}\n"
    );
    let live = compiled.replace(&padding, "a").replace("\tp\n}", "\tp.\n}");
    let items = complete_after(&compiled, &live, 12, 3);
    let offered = labels(&items);
    assert!(offered.contains(&"y"), "b's `p` is an Other: {offered:?}");
    assert!(
        !offered.contains(&"x"),
        "a's `p` must not answer for b's: {offered:?}"
    );
}

/// Nothing retained, nothing offered — and nothing crashes: a fresh thread
/// (a fresh instance) answers empty before its first compile, and a position
/// past the end of the text clamps rather than panicking the instance.
#[test]
fn completion_before_any_compile_is_empty_and_an_out_of_range_position_clamps() {
    let _guard = compiler();
    let items = on_big_stack(|| {
        assert!(
            vilan_wasm::complete_program("fun main() {}\n", 0, 5).is_empty(),
            "no analysis retained on this thread yet (the fresh spawn guarantees it)"
        );
        let program = "import std::io::print;\n\nfun main() {\n\tprint(1);\n}\n";
        compile_program(program);
        vilan_wasm::complete_program(program, 999, 999)
    });
    assert!(
        !items.is_empty(),
        "a clamped position still answers the scope at the text's end"
    );
}

/// `complete` is pure over the retained state: it leaks nothing (the tally
/// stays at zero across requests) and so pays nothing toward the page's
/// recycle budget — which is why the page never counts it as a compile.
#[test]
fn completing_leaks_nothing() {
    use vilan_core::leak_tally;
    let _guard = compiler();
    let program = "import std::json::encode_json;\n\nfun main() {\n\t\n}\n";
    on_big_stack(|| {
        compile_program(program);
        leak_tally::reset();
        for live in [
            "import std::json::encode_json;\n\nfun main() {\n\tenc\n}\n",
            "import std::json::encode_json;\nimport std::json::\n\nfun main() {\n\t\n}\n",
            program,
        ] {
            let items = vilan_wasm::complete_program(live, 3, 1);
            assert!(!items.is_empty());
        }
        // The tally is thread-local — read it on the thread that recorded it.
        assert_eq!(
            leak_tally::total(),
            0,
            "a completion request must not leak: {}",
            leak_tally::report()
        );
    });
}

/// An auto-import candidate's edit (E54c) is positioned in the LIVE text's
/// coordinates — the text the edit was computed against — not the analyzed
/// one: the live buffer here has a blank line inserted above the import, so
/// the two disagree by exactly one line. Every candidate's edit is checked,
/// by its shape: one that EXTENDS the existing import (`{ a, b }`) sits on
/// the import's live line, 1, past its start; a new import line sorted
/// before the existing one lands at the start of line 1, one sorted after
/// it at the start of line 2. Mapped through the analyzed text each would
/// sit one line up (and the sorted-before one a character in).
#[test]
fn an_auto_import_edit_is_positioned_in_the_live_text() {
    let compiled = "import std::json::encode_json;\n\nfun main() {\n\t\n}\n";
    let live = "\nimport std::json::encode_json;\n\nfun main() {\n\t\n}\n";
    let items = complete_after(compiled, live, 4, 1);
    let candidates: Vec<&CompletionItem> = items
        .iter()
        .filter(|item| item.import_edit.is_some())
        .collect();
    assert!(
        !candidates.is_empty(),
        "auto-import candidates are offered: {:?}",
        labels(&items)
    );
    for candidate in candidates {
        let edit = candidate.import_edit.as_ref().unwrap();
        assert!(
            candidate
                .detail
                .as_deref()
                .is_some_and(|detail| detail.starts_with("std::")),
            "labeled with its module: {:?}",
            candidate.detail
        );
        assert_eq!(
            candidate.boost, -3,
            "a std candidate sits in the auto-import band below pkg's"
        );
        let expected = if edit.text.starts_with("import ") {
            let sorts_before = edit.text.as_str() < "import std::json";
            (if sorts_before { 1 } else { 2 }, 0)
        } else {
            (1, edit.character.max(1))
        };
        assert_eq!(
            (edit.line, edit.character),
            expected,
            "{}'s edit {edit:?} is in the live text's coordinates",
            candidate.label
        );
    }
}
