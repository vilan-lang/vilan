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

mod line_index;

use std::path::{Path, PathBuf};

use line_index::LineIndex;
use vilan_core::analyzer::SourceId;
use vilan_core::{
    BuildOptions, Layer, PackageSpec, Platform, PlatformPattern, Workspace, analyze_source,
    transform,
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
    if let Some(existing) = entries.lock().unwrap().get(&key).copied() {
        return existing;
    }
    let leaked: &'static str = Box::leak(source.to_string().into_boxed_str());
    vilan_core::leak_tally::record(
        vilan_core::leak_tally::LeakSite::WasmEntryText,
        leaked.len(),
    );
    entries.lock().unwrap().insert(key, leaked);
    leaked
}

/// Compiles one Vilan source string for the browser platform — what the
/// playground runs. See [`compile_program_for`] for the platform-explicit
/// form behind the page's server check mode.
pub fn compile_program(source: &str) -> CompileOutput {
    compile_program_for(source, Platform::Browser)
}

/// Compiles for an explicit platform. `Platform::Browser` is the running
/// mode; a process platform is the playground's CHECK-ONLY server mode — the
/// diagnostics (platform coloring above all) are real, and the emitted
/// program, while genuine, is for a process host the page does not have.
/// Passing the platform explicitly also bypasses `infer_platform`, which
/// probes the disk.
pub fn compile_program_for(source: &str, platform: Platform) -> CompileOutput {
    boot();

    let entry_path = PathBuf::from(PROJECT_ROOT).join(ENTRY_NAME);
    vilan_core::analyzer::set_document_overlay(&entry_path, Some(source.to_string()));

    let leaked = interned_entry(source);
    let (program, errors) = analyze_source(
        leaked,
        &embedded_std_spec(),
        Path::new(PROJECT_ROOT),
        &entry_path,
        Some(platform),
        &Workspace::default(),
    );

    // The visitor's own file is the common case for a span, so index it once.
    let mut locator = Locator {
        entry_path: &entry_path,
        entry_index: LineIndex::new(source),
        indices: Vec::new(),
    };
    let mut diagnostics = Vec::new();

    // `path` is the file the diagnostic's own span indexes (`None` = the
    // entry); `hop_path` answers the same for a trace hop that names another
    // source — only a program can, so the pre-program paths pass a resolver
    // that knows none (their diagnostics carry no trace).
    let mut convert = |error: &vilan_core::Error,
                       severity: &'static str,
                       path: Option<&Path>,
                       hop_path: &dyn Fn(SourceId) -> Option<PathBuf>| {
        let range = error.span.into_range();
        let (line, column, file) = locator.locate(path, &error.span);
        // Each hop in ITS file: `Note::source` when it names one, the
        // diagnostic's own otherwise (the `None` contract of `Note`).
        let trace = error
            .trace
            .iter()
            .map(|hop| {
                let hop_path = hop.note.source.and_then(hop_path);
                let (line, column, file) =
                    locator.locate(hop_path.as_deref().or(path), &hop.note.span);
                TraceEntry {
                    file,
                    line,
                    column,
                    message: hop.note.msg.clone(),
                    call: hop.call,
                }
            })
            .collect();
        diagnostics.push(Diagnostic {
            start: range.start,
            end: range.end,
            line,
            column,
            message: error.msg.clone(),
            note: error.note.as_ref().map(|note| note.msg.clone()),
            trace,
            severity,
            file,
        });
    };

    let Some(program) = program else {
        for error in &errors {
            convert(error, "error", None, &|_| None);
        }
        return CompileOutput {
            js: None,
            css: None,
            diagnostics,
        };
    };
    let source_path = |source: SourceId| program.source_path(source).map(Path::to_path_buf);

    // `errors` is the ENTRY's own lex/parse errors followed by the program's
    // (`analyze_source`), while `diagnostic_sources` is parallel to the
    // program's half alone — so the flat index has to lose that prefix before
    // it can index the attribution. Feeding it straight in shifted every
    // attribution by N wherever N parse errors preceded, and a recovered parse
    // is exactly when a playground program has both (backlog E42). The language
    // server does the same subtraction in `document.rs`. An index inside the
    // prefix is the entry's own, which is what `None` means to `convert`.
    let prefix = errors.len().saturating_sub(program.diagnostics.len());
    for (index, error) in errors.iter().enumerate() {
        let path = index
            .checked_sub(prefix)
            .and_then(|offset| program.source_path(program.diagnostic_source(offset)))
            .map(Path::to_path_buf);
        convert(error, "error", path.as_deref(), &source_path);
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

    match transform(&program, &BuildOptions::default()) {
        Ok(javascript) => CompileOutput {
            js: Some(javascript),
            css,
            diagnostics,
        },
        Err(error) => {
            convert(&error, "error", None, &source_path);
            CompileOutput {
                js: None,
                css: None,
                diagnostics,
            }
        }
    }
}

/// Resolves spans to the page's positions, one `LineIndex` per file touched:
/// the visitor's entry is indexed up front (the common case), every other
/// file — std, for a diagnostic or a trace hop inside the toolchain — on
/// first use.
struct Locator<'a> {
    entry_path: &'a Path,
    entry_index: LineIndex,
    indices: Vec<(PathBuf, LineIndex)>,
}

impl Locator<'_> {
    /// The zero-based line, the UTF-16 column, and the visitor-facing file
    /// name of `span` in `path` (`None`, or the entry's own path, is the
    /// entry).
    fn locate(&mut self, path: Option<&Path>, span: &vilan_core::span::Span) -> (u32, u32, String) {
        let (index, file) = match path {
            None => (&self.entry_index, ENTRY_NAME.to_string()),
            Some(path) if path == self.entry_path => (&self.entry_index, ENTRY_NAME.to_string()),
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
                    .unwrap_or(&self.entry_index);
                (index, display_path(path))
            }
        };
        let (start, _) = index.range(span);
        (start.line, start.character, file)
    }
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
        let platform = match platform.as_str() {
            "node" => crate::Platform::default(), // Node, current LTS
            _ => crate::Platform::Browser,
        };
        convert(crate::compile_program_for(&source, platform))
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
}
