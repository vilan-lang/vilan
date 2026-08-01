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
    /// `"error"` or `"warning"`.
    pub severity: &'static str,
    /// The file the span indexes, as the visitor would name it. `main.vl` for
    /// their own code; a toolchain path for a diagnostic inside std.
    pub file: String,
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
    if let Ok(relative) = path.strip_prefix(PROJECT_ROOT) {
        return relative.to_string_lossy().into_owned();
    }
    if let Ok(relative) = path.strip_prefix(TOOLCHAIN_ROOT) {
        return relative.to_string_lossy().into_owned();
    }
    path.to_string_lossy().into_owned()
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

/// Compiles one Vilan source string for the browser platform.
///
/// Always `Platform::Browser`: it is what the playground runs, and passing it
/// explicitly also bypasses `infer_platform`, which probes the disk.
pub fn compile_program(source: &str) -> CompileOutput {
    boot();

    let entry_path = PathBuf::from(PROJECT_ROOT).join(ENTRY_NAME);
    vilan_core::analyzer::set_document_overlay(&entry_path, Some(source.to_string()));

    let leaked = interned_entry(source);
    let (program, errors) = analyze_source(
        leaked,
        &embedded_std_spec(),
        Path::new(PROJECT_ROOT),
        &entry_path,
        Some(Platform::Browser),
        &Workspace::default(),
    );

    // The visitor's own file is the common case for a span, so index it once.
    let entry_index = LineIndex::new(source);
    let mut indices: Vec<(PathBuf, LineIndex)> = Vec::new();
    let mut diagnostics = Vec::new();

    let mut convert = |error: &vilan_core::Error, severity: &'static str, path: Option<&Path>| {
        let range = error.span.into_range();
        let (index, file) = match path {
            None => (&entry_index, ENTRY_NAME.to_string()),
            Some(path) if path == entry_path => (&entry_index, ENTRY_NAME.to_string()),
            Some(path) => {
                if !indices.iter().any(|(known, _)| known == path) {
                    let text = vilan_core::util::read_source(path).unwrap_or_default();
                    indices.push((path.to_path_buf(), LineIndex::new(&text)));
                }
                let index = indices
                    .iter()
                    .find(|(known, _)| known == path)
                    .map(|(_, index)| index)
                    .unwrap_or(&entry_index);
                (index, display_path(path))
            }
        };
        let (start, _) = index.range(&error.span);
        diagnostics.push(Diagnostic {
            start: range.start,
            end: range.end,
            line: start.line,
            column: start.character,
            message: error.msg.clone(),
            note: error.note.as_ref().map(|note| note.msg.clone()),
            severity,
            file,
        });
    };

    let Some(program) = program else {
        for error in &errors {
            convert(error, "error", None);
        }
        return CompileOutput {
            js: None,
            css: None,
            diagnostics,
        };
    };

    for (index, error) in errors.iter().enumerate() {
        let path = program
            .source_path(program.diagnostic_source(index))
            .map(Path::to_path_buf);
        convert(error, "error", path.as_deref());
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
            convert(&error, "error", None);
            CompileOutput {
                js: None,
                css: None,
                diagnostics,
            }
        }
    }
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
                })
                .collect()
        }
    }

    /// Compiles Vilan source to JavaScript for the browser.
    #[wasm_bindgen]
    pub fn compile(source: String) -> CompileResult {
        let output = crate::compile_program(&source);
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
                })
                .collect(),
        }
    }

    /// The toolchain version, for the page's badge.
    #[wasm_bindgen]
    pub fn version() -> String {
        crate::version().to_string()
    }
}
