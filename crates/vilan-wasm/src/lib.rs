//! The Vilan compiler as a WebAssembly module — the web playground's engine
//! (backlog D11, `proposal/web-playground.md`).
//!
//! There is no filesystem behind this. The toolchain's own sources are compiled
//! into the binary by `vilan-embedded-std` and registered in core's
//! document overlay under a synthetic `/toolchain` root at boot; the visitor's
//! program is registered under `/project`. Module resolution then works exactly
//! as it does in an editor with unsaved buffers, which is the seam D11 S1 built
//! and pinned.
//!
//! **The compile logic lives here in plain Rust and is tested natively.** The
//! `wasm_bindgen` layer at the bottom of this file is a type conversion and
//! nothing else, so the wasm target's only job is to prove the thing still
//! builds — behaviour is already covered by `cargo test` on the host.
//!
//! ## One compile at a time
//!
//! Core's caches (`parse_clean_cached`, the document overlay, the error cache)
//! are process-global, so an instance must run one compile at a time. On the
//! web that is free — a worker is single-threaded — but it is a real
//! constraint, not an accident, and a future multi-threaded host must queue.
//! Those caches also leak by design (`Box::leak`, tallied by core's
//! `leak_tally`), which is why the page recycles the instance rather than
//! trusting it to run forever.
//!
//! ## What is retained between calls
//!
//! One thing, since K9 (`proposal/playground-completion.md` §5): the analysis
//! the last compile produced, so that [`complete_program`] can answer a
//! keystroke without analyzing. Every `compile_program_with` replaces it; the
//! instance dying (the page's recycle) discards it; `complete_program` only
//! reads it, leaks nothing, and answers empty when nothing is retained. The
//! same single-threaded discipline covers it: a completion never runs
//! concurrently with an analysis because nothing here runs concurrently.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use vilan_core::analyzer::SourceId;
use vilan_core::fx::FxHashMap as HashMap;
use vilan_core::id::Id;
use vilan_core::{
    BuildOptions, Layer, PackageSpec, Platform, PlatformPattern, Program, Workspace,
    analyze_source, transform,
};
use vilan_ide::{
    Analysis, Completion, CompletionIndex, CompletionKind, ImportRoots, LineIndex, Position,
};

/// The synthetic root the embedded toolchain is registered under. It never
/// exists on any disk; `util::canonical_path` normalizes a non-existent path
/// lexically, so registration and lookup agree on the key.
const TOOLCHAIN_ROOT: &str = "/toolchain";

/// The synthetic root the visitor's program is registered under.
const PROJECT_ROOT: &str = "/project";

/// The visitor's entry file. One file in v1 — multi-file editing is recorded
/// future work in the proposal's §9.
const ENTRY_NAME: &str = "main.vl";

/// Where one diagnostic points, in the shape the page renders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Byte offsets into the file the span belongs to.
    pub start: usize,
    pub end: usize,
    /// Zero-based line, and a UTF-16 character offset within it.
    pub line: u32,
    pub column: u32,
    pub message: String,
    /// The "declared here" style note, when the diagnostic carries one.
    pub note: Option<String>,
    /// The requirement trace (backlog E78, surfaced here by E80): one entry
    /// per uncovered call between the program's entry and the offending read,
    /// in that order. A channel of its own beside `note` — the note stays one
    /// location — and empty for every diagnostic but the context-coverage
    /// refusals.
    pub trace: Vec<TraceEntry>,
    /// `"error"` or `"warning"`.
    pub severity: &'static str,
    /// The file the span indexes, as the visitor would name it. `main.vl` for
    /// their own code; a toolchain path for a diagnostic inside std.
    pub file: String,
}

/// One entry of a diagnostic's requirement trace, located the way the
/// diagnostic itself is: the file the hop's span indexes (its OWN file — a
/// hop's `Note::source` names another when the call sits elsewhere), the
/// zero-based line and UTF-16 column in it, and the hop's label.
///
/// `call` marks an uncovered CALL SITE; it is `false` only for the chain's
/// elision tail, which is anchored at the last kept hop's span and carries
/// its text (`… N more uncovered calls on this path`) — a surface that names
/// each call's location renders the tail as text alone rather than naming
/// that location twice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceEntry {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub message: String,
    pub call: bool,
}

/// What a compile produced. `js` is `None` exactly when the program did not
/// compile, which is also when `diagnostics` carries at least one error.
#[derive(Debug, Clone, Default)]
pub struct CompileOutput {
    pub js: Option<String>,
    pub css: Option<String>,
    pub diagnostics: Vec<Diagnostic>,
}

/// One completion candidate, in the shape the page consumes
/// (`proposal/playground-completion.md` §6): the language server's
/// `CompletionItem` mapping (`main.rs::to_completion_item`) with the wire's
/// sort bands as a CodeMirror `boost`, and the call shape fixed at the
/// server's defaults (`Full`, snippets on) because the page has no settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    /// The name shown, and matched against the typed prefix.
    pub label: String,
    /// `macro` `function` `method` `field` `struct` `enum` `enum_variant`
    /// `trait` `variable` `module` `keyword` `snippet` — the page maps these
    /// to its icon set.
    pub kind: &'static str,
    /// The signature (functions/methods), the type (variables), or the module
    /// an auto-import candidate comes from.
    pub detail: Option<String>,
    /// The first paragraph of the declaration's `///` doc.
    pub documentation: Option<String>,
    /// What accepting inserts: the bare label, the call shape, or a construct
    /// snippet's body.
    pub insert: String,
    /// `insert` is an LSP-syntax snippet (`${1:name}` tab-stops, `$0` the
    /// final cursor) rather than plain text.
    pub is_snippet: bool,
    /// The language server's `sort_text` bands as a CodeMirror boost: an
    /// in-scope candidate `0`, an auto-import candidate `-(1 + tier)` (the
    /// user's own `pkg` above `std`), a construct snippet `-9`.
    pub boost: i32,
    /// The import an auto-import candidate adds when accepted (E54c), in the
    /// LIVE text's coordinates.
    pub import_edit: Option<ImportEdit>,
}

/// A text edit that adds an import: the range to replace (zero-based line,
/// UTF-16 character — the same units as [`Diagnostic`]) and its replacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportEdit {
    pub line: u32,
    pub character: u32,
    pub end_line: u32,
    pub end_character: u32,
    pub text: String,
}

impl CompletionItem {
    /// The page's shape of one engine candidate — the same decisions as the
    /// language server's `to_completion_item`, minus the wire types.
    fn from_completion(completion: Completion, live: &LineIndex) -> CompletionItem {
        let kind = match completion.kind {
            CompletionKind::Macro => "macro",
            CompletionKind::Function => "function",
            CompletionKind::Method => "method",
            CompletionKind::Field => "field",
            CompletionKind::Struct => "struct",
            CompletionKind::Enum => "enum",
            CompletionKind::EnumVariant => "enum_variant",
            CompletionKind::Trait => "trait",
            CompletionKind::Variable => "variable",
            CompletionKind::Module => "module",
            CompletionKind::Keyword => "keyword",
            CompletionKind::Snippet => "snippet",
        };
        let mut detail = completion.detail;
        let (mut insert, mut is_snippet) = (completion.label.clone(), false);
        let mut boost = 0;
        if let Some(parameters) = &completion.call_parameters
            && let Some(call) = vilan_ide::call_insertion(
                &completion.label,
                parameters,
                vilan_ide::CompletionFunctionCall::Full,
                true,
            )
        {
            insert = call.text;
            is_snippet = call.is_snippet;
        }
        if let Some(snippet) = completion.snippet {
            insert = snippet.body;
            is_snippet = true;
            boost = -9;
        }
        let import_edit = completion.needs_import.map(|auto_import| {
            detail = Some(auto_import.module_path.join("::"));
            boost = -(1 + i32::from(auto_import.origin_tier));
            let (start, end) = live.range(&auto_import.edit_span);
            ImportEdit {
                line: start.line,
                character: start.character,
                end_line: end.line,
                end_character: end.character,
                text: auto_import.edit_replacement,
            }
        });
        CompletionItem {
            label: completion.label,
            kind,
            detail,
            documentation: completion.documentation,
            insert,
            is_snippet,
            boost,
            import_edit,
        }
    }
}

/// The analysis the last compile left behind, kept for [`complete_program`]
/// (`proposal/playground-completion.md` §5). The program borrows only
/// `'static` data — the interned entry text and the leaked tree
/// `analyze_source` never reclaims — so holding it costs no new leak; it is
/// dropped when the next compile replaces it.
struct Retained {
    /// The interned entry text the program was analyzed from — the key a
    /// completion request's live text is compared against.
    text: &'static str,
    program: Program<'static>,
    /// The index of `text`: the coordinate space the program's spans live in.
    analyzed: LineIndex,
    entity_spans: Vec<(usize, usize, Id)>,
    platform_requirements: HashMap<Id, String>,
    import_roots: ImportRoots,
    /// What completion may read that is a function of the analysis alone
    /// (M25): the auto-import candidate table and the origins' module
    /// listings. Derived here, where the program is retained, so a keystroke
    /// reads it instead of re-deriving it — the same place in the playground's
    /// life that `Document::capture_landed` is in the server's.
    completion_index: CompletionIndex,
}

impl Retained {
    fn new(text: &'static str, program: Program<'static>) -> Retained {
        let entity_spans = vilan_ide::entity_spans(&program);
        let platform_requirements = vilan_core::platform_color::requirements(&program);
        let import_roots = ImportRoots {
            std: embedded_std_spec(),
            pkg_root: PathBuf::from(PROJECT_ROOT),
            dependencies: Vec::new(),
        };
        let completion_index = CompletionIndex::build(&program, Some(&import_roots), text);
        Retained {
            text,
            program,
            analyzed: LineIndex::new(text),
            entity_spans,
            platform_requirements,
            import_roots,
            completion_index,
        }
    }
}

thread_local! {
    /// One per instance — the instance is one thread. (Natively, the tests
    /// each run on their own thread and see exactly their own compiles.)
    static RETAINED: RefCell<Option<Retained>> = const { RefCell::new(None) };
}

/// The `PackageSpec` `resolve_std` would build for the embedded std, without
/// reading a manifest.
///
/// This mirrors `manifest::library_spec` applied to `vilan/std/vilan.toml`:
/// base root `src`, and one layer per `[library.layer.<name>]` whose root
/// defaults to `src/<name>`. The layer ORDER matters and is not the manifest's:
/// `Library::layer` is a `BTreeMap`, so `resolve_std` yields `browser` before
/// `process`, and `matching_layers` sorts stably — so ties keep this order.
/// Reproducing the order keeps the wasm build resolving identically to every
/// other front-end.
///
/// It is derived from a manifest that ships in `FILES`, so it can drift from
/// it. `the_hand_built_std_spec_matches_the_manifest` compares the two.
fn embedded_std_spec() -> PackageSpec {
    let std_root = PathBuf::from(TOOLCHAIN_ROOT).join("std");
    PackageSpec {
        base_root: std_root.join("src"),
        layers: vec![
            Layer {
                name: "browser".to_string(),
                patterns: vec![PlatformPattern::Browser],
                root: std_root.join("src").join("browser"),
            },
            Layer {
                name: "process".to_string(),
                // `@process` expands to the process-having runtimes.
                patterns: vec![
                    PlatformPattern::Node { version: None },
                    PlatformPattern::Deno { version: None },
                    PlatformPattern::Bun { version: None },
                ],
                root: std_root.join("src").join("process"),
            },
        ],
        dependencies: Vec::new(),
        surface: true,
        member: false,
        // std compiles with no ambient scope (`prelude.md` §10.1), which its
        // shipped manifest states; the parity test below compares the two.
        prelude: vilan_core::manifest::PreludeSpec::Off,
    }
}

/// Registers the embedded toolchain in the document overlay. Idempotent, and
/// cheap enough to call per compile: the keys and contents are identical every
/// time, and `parse_clean_cached` is content-addressed, so std parses once per
/// instance no matter how many times this runs.
pub fn boot() {
    let root = Path::new(TOOLCHAIN_ROOT);
    for (key, contents) in vilan_embedded_std::FILES {
        // Keys are always forward-slashed, on every host that generated them.
        vilan_core::analyzer::set_document_overlay(&root.join(key), Some((*contents).to_string()));
    }
}

/// Names the file a span belongs to, as the visitor should see it.
///
/// The project's own entry is `main.vl` rather than `/project/main.vl` — the
/// synthetic root is an implementation detail and would only confuse. A
/// toolchain path keeps enough to be recognizable (`std/src/option.vl`).
fn display_path(path: &Path) -> String {
    // Overlay keys are VIRTUAL module identifiers ("std/src/option.vl"),
    // not OS paths, so the rendered form is slash-joined on every host.
    // On the wasm32 target this replace is a no-op; it exists for the
    // native rlib on windows, where PathBuf renders the same key with
    // backslashes (caught by the E42 pin's first windows CI run).
    let render = |relative: &Path| relative.to_string_lossy().replace('\\', "/");
    if let Ok(relative) = path.strip_prefix(PROJECT_ROOT) {
        return render(relative);
    }
    if let Ok(relative) = path.strip_prefix(TOOLCHAIN_ROOT) {
        return render(relative);
    }
    render(path)
}

/// The entry source's `'static` copy, interned by content: `analyze_source`
/// borrows its source for `'static`, so every front-end leaks — but a
/// repeated compile of identical text must reuse the earlier leak instead of
/// adding one, or an instance that outlives many Runs of the same buffer
/// grows per Run. Tallied at `WasmEntryText` on the miss, like every other
/// leak site.
fn interned_entry(source: &str) -> &'static str {
    use std::collections::HashMap;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::sync::{Mutex, OnceLock};
    static ENTRIES: OnceLock<Mutex<HashMap<u64, &'static str>>> = OnceLock::new();
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    let key = hasher.finish();
    let entries = ENTRIES.get_or_init(|| Mutex::new(HashMap::new()));
    // Recovering (E97): the playground fences its compiles, so a caught panic
    // must not leave this cache poisoned for the rest of the instance's life —
    // an instance that answers "internal error" forever is the exact failure the
    // fence exists to prevent. Values are `&'static str`s leaked before the
    // lock, so a recovered guard never sees a half-written entry.
    if let Some(existing) = entries
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key)
        .copied()
    {
        return existing;
    }
    let leaked: &'static str = Box::leak(source.to_string().into_boxed_str());
    vilan_core::leak_tally::record(
        vilan_core::leak_tally::LeakSite::WasmEntryText,
        leaked.len(),
    );
    // Recovering (E97): the text is leaked before the lock is taken.
    entries
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key, leaked);
    leaked
}

/// The ambient scope a playground compile runs under (K14, `prelude.md` §5).
///
/// A pasted buffer has no `vilan.toml`, so B156's weakest-scope rule has no
/// `[package]` to hang the key on and the prelude would have nowhere to come
/// from. The playground supplies one anyway, as a SYNTHETIC package context —
/// the entry prelude the manifest would have declared, handed to the analyzer's
/// existing machinery. It is emphatically not a synthesized file-head import:
/// §9.2's mandate is that the prelude binds at the weakest layer, so a local
/// declaration and an explicit import both still win, silently.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PlaygroundPrelude {
    /// The mode's recommended set — [`PlaygroundPrelude::recommended_for`].
    #[default]
    Default,
    /// No ambient scope at all: the toggle's OFF position. Explicit imports
    /// required, exactly as `prelude = false` states it in a manifest — and
    /// the position a page teaching where a name really lives wants.
    Off,
    /// A named prelude module, for a caller that wants to pin one rather than
    /// take the mode's default.
    Module(String),
}

/// The wire spelling of [`PlaygroundPrelude::Off`] — the one value the page's
/// toggle sends that is not a module path. A module path is `root::module`
/// (`prelude_module_scope` accepts nothing else), so a single bare segment can
/// never collide with one.
pub const PRELUDE_OFF: &str = "off";

impl PlaygroundPrelude {
    /// The set a mode gets when the page says nothing: the WEB set in the
    /// browser, the BASE set on a process platform.
    ///
    /// The browser mode is not "a program that happens to target the browser" —
    /// it is the playground's running mode, and what runs there is a web app.
    /// That is the corpus §5.3 sized the web set for, and it is the one corpus
    /// §3.3 showed the base seven never empty an import block for. The server
    /// check mode is a process program and takes the base set, which is what
    /// `vilan init` would give it.
    pub fn recommended_for(platform: Platform) -> vilan_core::manifest::PreludeSpec {
        let module = match platform {
            Platform::Browser => vilan_core::manifest::WEB_PRELUDE,
            _ => vilan_core::manifest::DEFAULT_PRELUDE,
        };
        vilan_core::manifest::PreludeSpec::Module(module.to_string())
    }

    /// The page's wire form: absent is the mode's recommended set, [`PRELUDE_OFF`]
    /// is no prelude, anything else is a module path. Total by construction —
    /// the binding layer below has no decision left to make, and this one is
    /// covered by the native tests.
    pub fn from_option(value: Option<&str>) -> PlaygroundPrelude {
        match value {
            None => PlaygroundPrelude::Default,
            Some(PRELUDE_OFF) => PlaygroundPrelude::Off,
            Some(path) => PlaygroundPrelude::Module(path.to_string()),
        }
    }

    /// This option resolved against the mode it was asked for.
    fn resolve(self, platform: Platform) -> vilan_core::manifest::PreludeSpec {
        match self {
            PlaygroundPrelude::Default => PlaygroundPrelude::recommended_for(platform),
            PlaygroundPrelude::Off => vilan_core::manifest::PreludeSpec::Off,
            PlaygroundPrelude::Module(path) => vilan_core::manifest::PreludeSpec::Module(path),
        }
    }
}

/// Compiles one Vilan source string for the browser platform — what the
/// playground runs — under the browser mode's recommended ambient scope. See
/// [`compile_program_for`] for the platform-explicit form behind the page's
/// server check mode, and [`compile_program_with`] for the prelude-explicit one
/// behind its toggle.
pub fn compile_program(source: &str) -> CompileOutput {
    compile_program_for(source, Platform::Browser)
}

/// Compiles for an explicit platform, under that mode's recommended ambient
/// scope. `Platform::Browser` is the running mode; a process platform is the
/// playground's CHECK-ONLY server mode — the diagnostics (platform coloring
/// above all) are real, and the emitted program, while genuine, is for a
/// process host the page does not have. Passing the platform explicitly also
/// bypasses `infer_platform`, which probes the disk.
pub fn compile_program_for(source: &str, platform: Platform) -> CompileOutput {
    compile_program_with(source, platform, PlaygroundPrelude::Default)
}

/// Compiles for an explicit platform and an explicit ambient scope — the full
/// surface, behind the page's mode toggle and its prelude toggle.
///
/// The prelude rides on `Workspace::entry_prelude`, which is where a manifest's
/// `[package] prelude` lands for every other front end (`prelude.md` §6). The
/// playground has no manifest, so this IS the synthetic package context: one
/// field, read by the same `seed_preludes` pass that serves `vilan build`, so a
/// pasted single-file program means what it would inside a fresh `vilan init`
/// package. The base cache keys on it (`BaseCacheKey::entry_prelude`), so
/// flipping the toggle or the mode never serves the other one's world.
pub fn compile_program_with(
    source: &str,
    platform: Platform,
    prelude: PlaygroundPrelude,
) -> CompileOutput {
    boot();

    let entry_path = PathBuf::from(PROJECT_ROOT).join(ENTRY_NAME);
    vilan_core::analyzer::set_document_overlay(&entry_path, Some(source.to_string()));

    let workspace = Workspace {
        entry_prelude: prelude.resolve(platform),
        // There is no `vilan.toml` behind a pasted buffer, so the web-set steer
        // must not send a visitor to one (E120). The page's own prelude toggle
        // is where this program's ambient scope is actually set — the wire form
        // the binding layer below passes straight through — so it is what the
        // steer names.
        prelude_repair: vilan_core::PreludeRepair::Toggle,
        ..Workspace::default()
    };
    let leaked = interned_entry(source);
    let (program, errors) = analyze_source(
        leaked,
        &embedded_std_spec(),
        Path::new(PROJECT_ROOT),
        &entry_path,
        Some(platform),
        &workspace,
    );

    let Some(program) = program else {
        // No tree at all (a panic past the fence): nothing to answer
        // completion from either, so the previous program does not linger.
        RETAINED.with(|slot| *slot.borrow_mut() = None);
        let entry_index = LineIndex::new(source);
        // Only a program can name another source, so these diagnostics —
        // the entry's own — resolve every span, trace hops included, in the
        // entry.
        let mut locator = Locator {
            entry_path: &entry_path,
            entry_index: &entry_index,
            indices: Vec::new(),
        };
        let diagnostics = errors
            .iter()
            .map(|error| convert_diagnostic(error, "error", None, &|_| None, &mut locator))
            .collect();
        return CompileOutput {
            js: None,
            css: None,
            diagnostics,
        };
    };
    let retained = Retained::new(leaked, program);
    let output = emit(&retained.program, &retained.analyzed, &entry_path, &errors);
    RETAINED.with(|slot| *slot.borrow_mut() = Some(retained));
    output
}

/// The diagnostics and, when clean, the emitted program — over an analysis
/// that is about to be retained. `entry_index` indexes the entry text the
/// program was analyzed from.
fn emit(
    program: &Program<'static>,
    entry_index: &LineIndex,
    entry_path: &Path,
    errors: &[vilan_core::Error],
) -> CompileOutput {
    // The visitor's own file is the common case for a span, so it is indexed
    // already; every other file on first use.
    let mut locator = Locator {
        entry_path,
        entry_index,
        indices: Vec::new(),
    };
    let source_path = |source: SourceId| program.source_path(source).map(Path::to_path_buf);
    let mut diagnostics = Vec::new();

    // `errors` is the ENTRY's own lex/parse errors followed by the program's
    // (`analyze_source`), while `diagnostic_sources` is parallel to the
    // program's half alone — so the flat index has to lose that prefix before
    // it can index the attribution. Feeding it straight in shifted every
    // attribution by N wherever N parse errors preceded, and a recovered parse
    // is exactly when a playground program has both (backlog E42). The language
    // server does the same subtraction in `document.rs`. An index inside the
    // prefix is the entry's own, which is what `None` means to the converter.
    let prefix = errors.len().saturating_sub(program.diagnostics.len());
    for (index, error) in errors.iter().enumerate() {
        let path = index
            .checked_sub(prefix)
            .and_then(|offset| program.source_path(program.diagnostic_source(offset)))
            .map(Path::to_path_buf);
        diagnostics.push(convert_diagnostic(
            error,
            "error",
            path.as_deref(),
            &source_path,
            &mut locator,
        ));
    }

    if !errors.is_empty() {
        return CompileOutput {
            js: None,
            css: None,
            diagnostics,
        };
    }

    let css = vilan_core::const_eval::assemble_assets(&program.const_assets)
        .remove("css")
        .filter(|content| !content.is_empty());

    match transform(program, &BuildOptions::default()) {
        Ok(javascript) => CompileOutput {
            js: Some(javascript),
            css,
            diagnostics,
        },
        Err(error) => {
            diagnostics.push(convert_diagnostic(
                &error,
                "error",
                None,
                &source_path,
                &mut locator,
            ));
            CompileOutput {
                js: None,
                css: None,
                diagnostics,
            }
        }
    }
}

/// One diagnostic as the page consumes it. `path` is the file the
/// diagnostic's own span indexes (`None` = the entry); `hop_path` answers the
/// same for a trace hop that names another source — each hop is located in
/// ITS file (`Note::source` when it names one, the diagnostic's own
/// otherwise, the `None` contract of `Note`), the E16 rule per hop (E80).
fn convert_diagnostic(
    error: &vilan_core::Error,
    severity: &'static str,
    path: Option<&Path>,
    hop_path: &dyn Fn(SourceId) -> Option<PathBuf>,
    locator: &mut Locator<'_>,
) -> Diagnostic {
    let range = error.span.into_range();
    let (line, column, file) = locator.locate(path, &error.span);
    let trace = error
        .trace
        .iter()
        .map(|hop| {
            let hop_path = hop.note.source.and_then(hop_path);
            let (line, column, file) = locator.locate(hop_path.as_deref().or(path), &hop.note.span);
            TraceEntry {
                file,
                line,
                column,
                message: hop.note.msg.clone(),
                call: hop.call,
            }
        })
        .collect();
    Diagnostic {
        start: range.start,
        end: range.end,
        line,
        column,
        message: error.msg.clone(),
        note: error.note.as_ref().map(|note| note.msg.clone()),
        trace,
        severity,
        file,
    }
}

/// Resolves spans to the page's positions, one `LineIndex` per file touched:
/// the visitor's entry is indexed already (the common case — it is the
/// index the analysis retains), every other file — std, for a diagnostic or
/// a trace hop inside the toolchain — on first use.
struct Locator<'a> {
    entry_path: &'a Path,
    entry_index: &'a LineIndex,
    indices: Vec<(PathBuf, LineIndex)>,
}

impl Locator<'_> {
    /// The zero-based line, the UTF-16 column, and the visitor-facing file
    /// name of `span` in `path` (`None`, or the entry's own path, is the
    /// entry).
    fn locate(&mut self, path: Option<&Path>, span: &vilan_core::span::Span) -> (u32, u32, String) {
        let (index, file) = match path {
            None => (self.entry_index, ENTRY_NAME.to_string()),
            Some(path) if path == self.entry_path => (self.entry_index, ENTRY_NAME.to_string()),
            Some(path) => {
                if !self.indices.iter().any(|(known, _)| known == path) {
                    let text = vilan_core::util::read_source(path).unwrap_or_default();
                    self.indices
                        .push((path.to_path_buf(), LineIndex::new(&text)));
                }
                let index = self
                    .indices
                    .iter()
                    .find(|(known, _)| known == path)
                    .map(|(_, index)| index)
                    .unwrap_or(self.entry_index);
                (index, display_path(path))
            }
        };
        let (start, _) = index.range(span);
        (start.line, start.character, file)
    }
}

/// Completion candidates at a cursor in `source` — `line` zero-based, and
/// `character` a UTF-16 offset within it, the units the page already speaks
/// for diagnostics — answered from the analysis the last compile retained
/// (`proposal/playground-completion.md` §5–§6). Never analyzes: a keystroke
/// cannot pay for one, and the page's debounced check keeps the retained
/// program at most one debounce behind the buffer. Empty when nothing is
/// retained (a fresh instance before its first check).
///
/// When `source` is the retained text the two coordinate spaces coincide.
/// When it is not — the usual case, the visitor having just typed the
/// character that triggered this — the engine reads the trigger off the live
/// text and converts to the analyzed text through line/character, E52's rule
/// (`lsp-snapshot-consistency.md`), exactly as the language server does
/// between a keystroke and its debounced re-analysis.
pub fn complete_program(source: &str, line: u32, character: u32) -> Vec<CompletionItem> {
    RETAINED.with(|slot| {
        let slot = slot.borrow();
        let Some(retained) = slot.as_ref() else {
            return Vec::new();
        };
        let live_index;
        let live: &LineIndex = if source == retained.text {
            &retained.analyzed
        } else {
            live_index = LineIndex::new(source);
            &live_index
        };
        let analysis = Analysis {
            program: &retained.program,
            analyzed: &retained.analyzed,
            live,
            entity_spans: &retained.entity_spans,
            platform_requirements: &retained.platform_requirements,
            import_roots: Some(&retained.import_roots),
            index: &retained.completion_index,
            source_texts: Default::default(),
            anchor: Default::default(),
        };
        let offset = live.offset(Position { line, character });
        analysis
            .completion(offset)
            .into_iter()
            .map(|completion| CompletionItem::from_completion(completion, live))
            .collect()
    })
}

/// Formats one Vilan source string — the CLI's `vilan fmt` rule exactly
/// (`formatter::format`): canonical layout when the reprint round-trips, the
/// ORIGINAL bytes when it does not (the source does not parse, or the printer
/// bails). Pure text work: no boot, no overlay, no platform.
pub fn format_program(source: &str) -> String {
    vilan_core::formatter::format(source)
}

/// The toolchain version this module was built from, for the page's badge.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

// --- The wasm-bindgen layer -------------------------------------------------
//
// Deliberately thin: it converts `CompileOutput` into a JS object and does
// nothing else. Anything with a decision in it belongs above this line, where
// the native tests can reach it.

#[cfg(target_arch = "wasm32")]
mod bindings {
    use wasm_bindgen::prelude::*;

    /// One diagnostic, as the page consumes it.
    #[wasm_bindgen(getter_with_clone)]
    pub struct Diagnostic {
        pub start: usize,
        pub end: usize,
        pub line: u32,
        pub column: u32,
        pub message: String,
        pub note: Option<String>,
        pub severity: String,
        pub file: String,
        trace: Vec<TraceEntry>,
    }

    /// One hop of a diagnostic's requirement trace (E80), as the page
    /// consumes it: the same field names and units as `Diagnostic`'s own
    /// position (zero-based line, UTF-16 column, the visitor-facing file).
    #[wasm_bindgen(getter_with_clone)]
    #[derive(Clone)]
    pub struct TraceEntry {
        pub file: String,
        pub line: u32,
        pub column: u32,
        pub message: String,
        pub call: bool,
    }

    #[wasm_bindgen]
    impl Diagnostic {
        /// The requirement trace, as a JS array of `TraceEntry` objects in
        /// entry → read order — empty for every diagnostic but the
        /// context-coverage refusals. A getter for the same reason
        /// `CompileResult::diagnostics` is one: a `Vec` of exported structs
        /// crosses as a return value, not as a field.
        #[wasm_bindgen(getter)]
        pub fn trace(&self) -> Vec<TraceEntry> {
            self.trace.clone()
        }
    }

    #[wasm_bindgen(getter_with_clone)]
    pub struct CompileResult {
        pub js: Option<String>,
        pub css: Option<String>,
        diagnostics: Vec<Diagnostic>,
    }

    #[wasm_bindgen]
    impl CompileResult {
        /// The diagnostics, as a JS array. A getter rather than a field because
        /// `Vec<Diagnostic>` is not directly convertible.
        #[wasm_bindgen(getter)]
        pub fn diagnostics(&self) -> Vec<Diagnostic> {
            self.diagnostics
                .iter()
                .map(|diagnostic| Diagnostic {
                    start: diagnostic.start,
                    end: diagnostic.end,
                    line: diagnostic.line,
                    column: diagnostic.column,
                    message: diagnostic.message.clone(),
                    note: diagnostic.note.clone(),
                    severity: diagnostic.severity.clone(),
                    file: diagnostic.file.clone(),
                    trace: diagnostic.trace.clone(),
                })
                .collect()
        }
    }

    fn convert(output: crate::CompileOutput) -> CompileResult {
        CompileResult {
            js: output.js,
            css: output.css,
            diagnostics: output
                .diagnostics
                .into_iter()
                .map(|diagnostic| Diagnostic {
                    start: diagnostic.start,
                    end: diagnostic.end,
                    line: diagnostic.line,
                    column: diagnostic.column,
                    message: diagnostic.message,
                    note: diagnostic.note,
                    severity: diagnostic.severity.to_string(),
                    file: diagnostic.file,
                    trace: diagnostic
                        .trace
                        .into_iter()
                        .map(|hop| TraceEntry {
                            file: hop.file,
                            line: hop.line,
                            column: hop.column,
                            message: hop.message,
                            call: hop.call,
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    /// Compiles Vilan source to JavaScript for the browser.
    #[wasm_bindgen]
    pub fn compile(source: String) -> CompileResult {
        convert(crate::compile_program(&source))
    }

    /// Compiles for a named platform: "node" checks the process leg (the
    /// playground's server mode); anything else is the browser. The page
    /// feature-detects this export, so an older wasm simply hides the mode
    /// toggle.
    #[wasm_bindgen]
    pub fn compile_for(source: String, platform: String) -> CompileResult {
        convert(crate::compile_program_for(&source, platform_of(&platform)))
    }

    /// [`compile_for`] plus the ambient scope (K14): `prelude` is `undefined`
    /// for the mode's recommended set (the toggle's ON position), the string
    /// `"off"` for none (its OFF position), or a module path to pin one. The
    /// page feature-detects this export, so a glue built before it existed
    /// simply hides the prelude toggle and keeps compiling through
    /// [`compile_for`] — which takes the same recommended default.
    #[wasm_bindgen]
    pub fn compile_with(
        source: String,
        platform: String,
        prelude: Option<String>,
    ) -> CompileResult {
        convert(crate::compile_program_with(
            &source,
            platform_of(&platform),
            crate::PlaygroundPrelude::from_option(prelude.as_deref()),
        ))
    }

    /// The page's platform word. "node" is the server check mode; anything
    /// else — including the word the page sends for its running mode — is the
    /// browser.
    fn platform_of(platform: &str) -> crate::Platform {
        match platform {
            "node" => crate::Platform::default(), // Node, current LTS
            _ => crate::Platform::Browser,
        }
    }

    /// Formats Vilan source; the input comes back unchanged when it cannot be
    /// safely reformatted. The page feature-detects this export, so a glue
    /// built before it existed simply hides its Format button.
    #[wasm_bindgen]
    pub fn format(source: String) -> String {
        crate::format_program(&source)
    }

    /// The toolchain version, for the page's badge.
    #[wasm_bindgen]
    pub fn version() -> String {
        crate::version().to_string()
    }

    // --- completion (K9) — its own block, beside the compile surface ---------

    /// One completion candidate, as the page consumes it. The auto-import
    /// edit rides as five optional flat fields rather than a nested struct,
    /// which `wasm_bindgen` does not pass by value.
    #[wasm_bindgen(getter_with_clone)]
    pub struct CompletionItem {
        pub label: String,
        pub kind: String,
        pub detail: Option<String>,
        pub documentation: Option<String>,
        pub insert: String,
        pub is_snippet: bool,
        pub boost: i32,
        pub import_line: Option<u32>,
        pub import_character: Option<u32>,
        pub import_end_line: Option<u32>,
        pub import_end_character: Option<u32>,
        pub import_text: Option<String>,
    }

    /// Completion candidates at `line`/`character` (zero-based line, UTF-16
    /// character) in `source`, from the analysis the last compile retained;
    /// empty before any compile. The page feature-detects this export, so a
    /// glue built before it existed simply registers no completion source.
    #[wasm_bindgen]
    pub fn complete(source: String, line: u32, character: u32) -> Vec<CompletionItem> {
        crate::complete_program(&source, line, character)
            .into_iter()
            .map(|item| {
                let edit = item.import_edit;
                CompletionItem {
                    label: item.label,
                    kind: item.kind.to_string(),
                    detail: item.detail,
                    documentation: item.documentation,
                    insert: item.insert,
                    is_snippet: item.is_snippet,
                    boost: item.boost,
                    import_line: edit.as_ref().map(|edit| edit.line),
                    import_character: edit.as_ref().map(|edit| edit.character),
                    import_end_line: edit.as_ref().map(|edit| edit.end_line),
                    import_end_character: edit.as_ref().map(|edit| edit.end_character),
                    import_text: edit.map(|edit| edit.text),
                }
            })
            .collect()
    }
}
